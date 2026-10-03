//! Exercise actual native views, event dispatch, focus, and desktop-event routing.
//! All persistence and provider configuration are isolated under temporary paths.

use std::{cell::RefCell, rc::Rc, sync::OnceLock};

use gpui::{
    point, px, size, AppContext, Entity, EntityInputHandler, Focusable, Modifiers, MouseButton,
    Pixels, Point, TestAppContext, VisualTestContext,
};
use gpui_component::Root;
use thoughttree_desktop::{Desktop, DesktopEvent};
use thoughttree_gpui_model::{Editor, Graph, GraphNode, Position, Project};

use crate::{
    canvas::{GraphCanvas, GraphEvent},
    theme,
    workspace::Workspace,
};

struct ViewState {
    editor: Editor,
    palette: Option<Vec<String>>,
    preview_id: Option<String>,
    editing: bool,
    modal: Option<crate::dialogs::Modal>,
    title: String,
}

fn read_state(workspace: &Entity<Workspace>, cx: &VisualTestContext) -> ViewState {
    workspace.read_with(cx, |state, _| ViewState {
        editor: state.editor.clone(),
        palette: state.palette.as_ref().map(|palette| palette.hits.clone()),
        preview_id: state.preview_id.clone(),
        editing: state.editing,
        modal: state.modal.clone(),
        title: state.title.clone(),
    })
}

fn graph() -> Graph {
    let mut graph = Graph::default();
    graph.add_node(
        GraphNode::user("question", "A question about Rust", 1.0),
        Position { x: 0.0, y: 0.0 },
    );
    graph.add_node(
        GraphNode::assistant("answer", "An answer about Rust", 2.0),
        Position { x: 0.0, y: 200.0 },
    );
    graph.add_node(
        GraphNode::assistant("sibling", "Another branch", 3.0),
        Position { x: 240.0, y: 200.0 },
    );
    graph.add_edge("question", "answer").unwrap();
    graph.add_edge("question", "sibling").unwrap();
    graph
}

pub(crate) fn workspace(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Entity<Workspace>,
    &mut VisualTestContext,
    async_channel::Sender<DesktopEvent>,
) {
    static RECOVERY: OnceLock<tempfile::TempDir> = OnceLock::new();
    RECOVERY.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap();
        std::env::set_var("THOUGHTTREE_LOCAL_STATE_DIR", directory.path());
        std::env::set_var("CODEX_PATH", fixture_adapter());
        std::env::set_var("CODEX_CONFIG", "{}");
        directory
    });
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config");
    let vault = directory.path().join("vault");
    std::fs::create_dir(&config).unwrap();
    std::fs::create_dir(&vault).unwrap();
    std::fs::write(config.join("config.json"), serde_json::to_vec(&serde_json::json!({
        "notes_directory": vault,
                "provider_paths": {"codex": fixture_adapter(), "claude-code": "/nonexistent/test-claude"}
    })).unwrap()).unwrap();
    let (desktop, _) = Desktop::open(config).unwrap();
    let (sender, events) = async_channel::unbounded();
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
    });
    let mut workspace = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut workspace = Workspace::new(desktop, events, window, cx);
            workspace.modal = None;
            workspace.title = "Native interaction fixture".into();
            workspace.editor = Editor::new(Project::from(graph()));
            workspace.refresh(cx);
            workspace
        });
        workspace = Some(view.clone());
        Root::new(view, window, cx)
    });
    (directory, workspace.unwrap(), cx, sender)
}

fn fixture_adapter() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/gpui-fixture-agent.py")
        .canonicalize()
        .unwrap()
}

fn graph_point(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    position: Position,
) -> Point<Pixels> {
    let bounds = cx
        .debug_bounds("graph-canvas")
        .expect("rendered Graph bounds");
    let position = workspace.read_with(cx, |this, cx| {
        this.canvas.read(cx).viewport.screen(position)
    });
    bounds.origin + point(px(position.x as f32), px(position.y as f32))
}

fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>, modifiers: Modifiers) {
    cx.simulate_mouse_down(from, MouseButton::Left, modifiers);
    cx.simulate_mouse_move(to, Some(MouseButton::Left), modifiers);
    cx.simulate_mouse_up(to, MouseButton::Left, modifiers);
}

fn connect_nodes(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, from: &str, to: &str) {
    let graph = read_state(workspace, cx).editor.project.graph;
    let source = graph.layout[from];
    let target = graph.layout[to];
    let from = graph_point(
        workspace,
        cx,
        Position {
            x: source.x + 86.,
            y: source.y + 120.,
        },
    );
    let to = graph_point(
        workspace,
        cx,
        Position {
            x: target.x + 50.,
            y: target.y + 50.,
        },
    );
    drag(cx, from, to, Modifiers::default());
}

#[gpui::test]
fn palette_keyboard_search_jump_and_preview_use_native_focus(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_keystrokes("cmd-k");
    assert!(read_state(&workspace, cx).palette.is_some());
    cx.simulate_input("Rust");
    assert_eq!(
        read_state(&workspace, cx).palette.as_ref().unwrap().clone(),
        ["answer", "question"]
    );
    cx.simulate_keystrokes("down enter");
    let state = read_state(&workspace, cx);
    assert!(state.palette.is_none());
    assert_eq!(
        state.editor.selection.first().map(String::as_str),
        Some("question")
    );
    assert!(state.preview_id.is_none());
    cx.simulate_keystrokes("cmd-k");
    cx.simulate_input("answer");
    cx.simulate_keystrokes("cmd-enter");
    assert_eq!(
        read_state(&workspace, cx).preview_id.as_deref(),
        Some("answer")
    );
}

#[gpui::test]
fn palette_corpus_is_frozen_and_opening_clears_stale_editing(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), true, window, cx)
        })
    });
    cx.simulate_keystrokes("cmd-k");
    workspace.update(cx, |this, _| {
        this.editor.project.graph.nodes["answer"].content = "changed after Palette opened".into();
    });
    cx.simulate_input("answer");
    assert_eq!(
        read_state(&workspace, cx).palette.as_ref().unwrap().clone(),
        ["answer"]
    );
    cx.simulate_keystrokes("escape");
    assert!(!read_state(&workspace, cx).editing);
    assert!(cx.update(|window, cx| workspace.read(cx).focus.is_focused(window)));
    assert_eq!(
        read_state(&workspace, cx).preview_id.as_deref(),
        Some("question")
    );
}

#[gpui::test]
fn typing_shortcut_letters_and_backspace_never_delete_the_graph(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), true, window, cx)
        })
    });
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("space e p");
    cx.simulate_keystrokes("backspace");
    let state = read_state(&workspace, cx);
    assert!(state.editing);
    assert_eq!(state.editor.project.graph.nodes.len(), 3);
    assert_eq!(
        state.editor.project.graph.nodes["question"].content,
        "space e "
    );
    cx.simulate_keystrokes("escape");
    assert!(!read_state(&workspace, cx).editing);
    assert!(read_state(&workspace, cx).preview_id.is_some());
    cx.simulate_keystrokes("escape");
    assert!(read_state(&workspace, cx).preview_id.is_none());
}

#[gpui::test]
fn reply_delete_undo_and_redo_preserve_graph_edges(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    workspace.update(cx, |this, cx| {
        this.editor.selection.insert("answer".into());
        this.refresh(cx);
    });
    cx.simulate_keystrokes("enter");
    let new_id = read_state(&workspace, cx)
        .preview_id
        .clone()
        .expect("reply editor opens");
    assert!(read_state(&workspace, cx).editing);
    assert_eq!(
        read_state(&workspace, cx)
            .editor
            .project
            .graph
            .parents(&new_id),
        ["answer"]
    );
    cx.simulate_keystrokes("escape");
    assert!(cx.update(|window, cx| workspace.read(cx).focus.is_focused(window)));
    cx.simulate_keystrokes("backspace");
    assert!(!read_state(&workspace, cx)
        .editor
        .project
        .graph
        .nodes
        .contains_key(&new_id));
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        read_state(&workspace, cx)
            .editor
            .project
            .graph
            .parents(&new_id),
        ["answer"]
    );
    cx.simulate_keystrokes("cmd-shift-z");
    assert!(!read_state(&workspace, cx)
        .editor
        .project
        .graph
        .nodes
        .contains_key(&new_id));
}

#[gpui::test]
fn settings_owns_keyboard_until_escape(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_keystrokes("cmd-,");
    assert!(matches!(
        read_state(&workspace, cx).modal,
        Some(crate::dialogs::Modal::Settings)
    ));
    cx.simulate_keystrokes("cmd-k");
    assert!(read_state(&workspace, cx).palette.is_none());
    cx.simulate_keystrokes("escape cmd-k");
    assert!(read_state(&workspace, cx).modal.is_none());
    assert!(read_state(&workspace, cx).palette.is_some());
}

#[gpui::test]
fn canvas_click_selection_drag_and_box_select_emit_graph_events(cx: &mut TestAppContext) {
    let (canvas, cx) = cx.add_window_view(|_, _| GraphCanvas::new());
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&canvas, move |_, event, _| {
            observed.borrow_mut().push(event.clone())
        })
    });
    canvas.update(cx, |canvas, cx| {
        let graph = graph();
        let nodes = graph
            .nodes
            .values()
            .map(|node| (node.clone(), graph.layout[&node.id]))
            .collect();
        canvas.set_graph(
            nodes,
            graph.edges,
            Default::default(),
            Default::default(),
            Default::default(),
            cx,
        );
    });
    cx.run_until_parked();
    let start = point(px(120.), px(110.));
    cx.simulate_click(start, Modifiers::default());
    assert!(events
        .borrow()
        .iter()
        .any(|event| matches!(event, GraphEvent::Select(ids) if ids == &["question"])));
    events.borrow_mut().clear();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        point(px(170.), px(150.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(px(170.), px(150.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    assert!(events.borrow().iter().any(
        |event| matches!(event, GraphEvent::Move(positions) if positions.contains_key("question"))
    ));
    events.borrow_mut().clear();
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    cx.simulate_mouse_down(point(px(550.), px(450.)), MouseButton::Left, shift);
    cx.simulate_mouse_move(point(px(310.), px(250.)), Some(MouseButton::Left), shift);
    cx.simulate_mouse_up(point(px(310.), px(250.)), MouseButton::Left, shift);
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, GraphEvent::Select(ids) if ids == &["sibling"])),
        "selections: {:?}",
        events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                GraphEvent::Select(ids) => Some(ids),
                _ => None,
            })
            .collect::<Vec<_>>()
    );
}

#[gpui::test]
fn desktop_chunks_respect_turn_identity_and_project_replacement(cx: &mut TestAppContext) {
    use thoughttree_core::events::StreamChunkEvent;

    let (_directory, workspace, cx, events) = workspace(cx);
    workspace.update(cx, |this, _| {
        this.editor.start_turn("answer", "current").unwrap()
    });
    for (turn, text) in [("obsolete", " stale"), ("current", " streamed")] {
        events
            .try_send(DesktopEvent::StreamChunk(StreamChunkEvent {
                node_id: "answer".into(),
                turn_id: turn.into(),
                chunk: text.into(),
            }))
            .unwrap();
    }
    cx.run_until_parked();
    assert_eq!(
        read_state(&workspace, cx).editor.project.graph.nodes["answer"].content,
        "An answer about Rust"
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(
        read_state(&workspace, cx).editor.project.graph.nodes["answer"].content,
        "An answer about Rust streamed"
    );
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.install_project(
                Project::default(),
                None,
                None,
                "Rejected replacement".into(),
                window,
                cx,
            );
        })
    });
    assert_eq!(
        read_state(&workspace, cx).title,
        "Native interaction fixture"
    );
    workspace.update(cx, |this, _| {
        assert!(this.editor.finish_turn("answer", "current"));
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.install_project(
                Project::from(graph()),
                None,
                None,
                "Replacement".into(),
                window,
                cx,
            );
        })
    });
    events
        .try_send(DesktopEvent::StreamChunk(StreamChunkEvent {
            node_id: "answer".into(),
            turn_id: "current".into(),
            chunk: " late old turn".into(),
        }))
        .unwrap();
    cx.run_until_parked();
    let state = read_state(&workspace, cx);
    assert_eq!(state.title, "Replacement");
    assert_eq!(
        state.editor.project.graph.nodes["answer"].content,
        "An answer about Rust"
    );
}

#[gpui::test]
fn permission_dialog_interrupts_palette_and_owns_keyboard(cx: &mut TestAppContext) {
    use thoughttree_core::events::{PermissionRequestEvent, PermissionRequestOption};

    let (_directory, workspace, cx, events) = workspace(cx);
    cx.simulate_keystrokes("cmd-k");
    assert!(read_state(&workspace, cx).palette.is_some());
    events
        .try_send(DesktopEvent::PermissionRequest(
            PermissionRequestEvent::new(
                "request".into(),
                "answer".into(),
                "read".into(),
                "Read file".into(),
                "Read a synthetic fixture?".into(),
                vec![PermissionRequestOption {
                    id: "allow_once".into(),
                    label: "Allow once".into(),
                }],
            ),
        ))
        .unwrap();
    cx.run_until_parked();
    assert!(read_state(&workspace, cx).palette.is_none());
    cx.simulate_keystrokes("escape cmd-k");
    assert!(read_state(&workspace, cx).palette.is_none());
    assert_eq!(
        workspace.read_with(cx, |state, _| state.permissions.len()),
        1
    );
}

#[gpui::test]
fn delayed_file_link_cannot_enter_a_replacement_project(cx: &mut TestAppContext) {
    let (directory, workspace, cx, _events) = workspace(cx);
    let file = directory.path().join("vault/reference.md");
    std::fs::write(&file, "Synthetic linked file").unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.add_files(vec![file.clone()], Position::default(), cx);
            this.install_project(
                Project::from(graph()),
                None,
                None,
                "Replacement".into(),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    let state = read_state(&workspace, cx);
    assert_eq!(state.title, "Replacement");
    assert_eq!(state.editor.project.graph.nodes.len(), 3);
    workspace.update(cx, |this, cx| {
        this.add_files(vec![file], Position::default(), cx)
    });
    cx.run_until_parked();
    let state = read_state(&workspace, cx);
    assert_eq!(state.editor.project.graph.nodes.len(), 4);
    assert!(state.editor.project.graph.nodes.values().any(|node| node
        .file_data()
        .is_some_and(|file| file.path == "reference.md")));
}

#[gpui::test]
fn command_enter_from_focused_editor_creates_an_assistant_turn(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    workspace.update(cx, |this, _| {
        this.editor.project.graph.nodes["question"].content = "fixture:nopermission".into();
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), true, window, cx)
        })
    });
    cx.simulate_keystrokes("cmd-enter");
    let state = read_state(&workspace, cx);
    assert_eq!(state.editor.project.graph.nodes.len(), 4);
    let assistant = state.preview_id.expect("generated response is previewed");
    assert_eq!(state.editor.project.graph.parents(&assistant), ["question"]);
    assert_eq!(
        state.editor.project.graph.nodes[&assistant].role(),
        thoughttree_gpui_model::Role::Assistant
    );
    assert!(state.editor.active_turns.contains_key(&assistant));
    assert!(!state.editing);
}

#[gpui::test]
fn image_paste_is_intercepted_and_text_paste_remains_native(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), true, window, cx)
        })
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let image = gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes.into_inner());
    cx.write_to_clipboard(gpui::ClipboardItem::new_image(&image));
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    assert_eq!(
        read_state(&workspace, cx).editor.project.graph.nodes["question"]
            .images()
            .len(),
        1
    );
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(" pasted text".into()));
    cx.simulate_keystrokes("cmd-v");
    assert!(
        read_state(&workspace, cx).editor.project.graph.nodes["question"]
            .content
            .contains("pasted text")
    );
}

#[gpui::test]
fn file_mention_arrows_and_enter_choose_a_path_without_inserting_newline(cx: &mut TestAppContext) {
    let (directory, workspace, cx, _events) = workspace(cx);
    std::fs::write(directory.path().join("vault/ref-a.md"), "A").unwrap();
    std::fs::write(directory.path().join("vault/ref-b.md"), "B").unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), true, window, cx)
        })
    });
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("@ref");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    let mentions = workspace.read_with(cx, |state, _| state.mentions.clone());
    assert_eq!(mentions.len(), 2);
    cx.simulate_keystrokes("down enter");
    let text = &read_state(&workspace, cx).editor.project.graph.nodes["question"].content;
    assert_eq!(text, &format!("@/{}", mentions[1]));
    assert!(!text.contains('\n'));
}

#[gpui::test]
fn dragging_a_connection_to_blank_canvas_creates_a_downstream_user(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1200.), px(900.)));
    cx.run_until_parked();
    let source = graph_point(&workspace, cx, Position { x: 86., y: 320. });
    let drop = Position { x: 600., y: 420. };
    let target = graph_point(&workspace, cx, drop);
    drag(cx, source, target, Modifiers::default());
    let state = read_state(&workspace, cx);
    assert_eq!(state.editor.project.graph.nodes.len(), 4);
    let id = state.preview_id.expect("new user opens in the editor");
    assert_eq!(state.editor.project.graph.parents(&id), ["answer"]);
    assert_eq!(state.editor.project.graph.layout[&id], drop);
    assert_eq!(
        state.editor.project.graph.nodes[&id].role(),
        thoughttree_gpui_model::Role::User
    );
    assert!(state.editing);
}

#[gpui::test]
fn native_connections_reject_cycles_and_file_targets_and_reverse_user_to_assistant(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1200.), px(900.)));
    workspace.update(cx, |this, cx| {
        this.editor.project.graph.add_node(
            GraphNode::user("detached", "Join this branch", 5.),
            Position { x: 460., y: 360. },
        );
        this.editor.project.graph.add_node(
            GraphNode::file(
                "file",
                thoughttree_gpui_model::FileData {
                    path: "note.md".into(),
                    name: "note.md".into(),
                    mime_type: "text/markdown".into(),
                    size: 0,
                    seen_size: 0,
                    seen_mtime: 0,
                },
                4.,
            ),
            Position { x: 500., y: 0. },
        );
        this.refresh(cx);
    });
    cx.run_until_parked();
    let edges = read_state(&workspace, cx).editor.project.graph.edges;
    connect_nodes(&workspace, cx, "answer", "question");
    assert_eq!(read_state(&workspace, cx).editor.project.graph.edges, edges);
    connect_nodes(&workspace, cx, "answer", "file");
    assert_eq!(read_state(&workspace, cx).editor.project.graph.edges, edges);
    connect_nodes(&workspace, cx, "detached", "answer");
    let graph = read_state(&workspace, cx).editor.project.graph;
    assert_eq!(graph.parents("detached"), ["answer"]);
    assert_eq!(graph.parents("answer"), ["question"]);
}

#[gpui::test]
fn selected_edge_deletion_respects_an_active_turns_lineage(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1200.), px(900.)));
    cx.run_until_parked();
    let edge = read_state(&workspace, cx).editor.project.graph.edges[0]
        .id
        .clone();
    let midpoint = graph_point(&workspace, cx, Position { x: 85., y: 160. });
    cx.simulate_click(midpoint, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |this, _| this.selected_edge.clone()),
        Some(edge.clone())
    );
    workspace.update(cx, |this, cx| {
        this.editor.start_turn("answer", "active").unwrap();
        this.refresh(cx);
    });
    cx.simulate_keystrokes("backspace");
    assert!(read_state(&workspace, cx)
        .editor
        .project
        .graph
        .edges
        .iter()
        .any(|item| item.id == edge));
    workspace.update(cx, |this, cx| {
        this.editor.finish_turn("answer", "active");
        this.refresh(cx);
    });
    cx.simulate_click(midpoint, Modifiers::default());
    cx.simulate_keystrokes("backspace");
    let state = read_state(&workspace, cx);
    assert_eq!(state.editor.project.graph.nodes.len(), 3);
    assert!(!state
        .editor
        .project
        .graph
        .edges
        .iter()
        .any(|item| item.id == edge));
    cx.simulate_keystrokes("cmd-z");
    assert!(read_state(&workspace, cx)
        .editor
        .project
        .graph
        .edges
        .iter()
        .any(|item| item.id == edge));
}

#[gpui::test]
fn graph_lock_prevents_node_drag_connection_and_selection_but_retains_pan(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1200.), px(900.)));
    cx.run_until_parked();
    let lock = cx.debug_bounds("lock").expect("lock control").center();
    cx.simulate_click(lock, Modifiers::default());
    assert!(!workspace.read_with(cx, |this, cx| this.canvas.read(cx).interactive));
    let before = read_state(&workspace, cx).editor.project.graph;
    let source = graph_point(&workspace, cx, Position { x: 40., y: 45. });
    drag(
        cx,
        source,
        source + point(px(60.), px(35.)),
        Modifiers::default(),
    );
    connect_nodes(&workspace, cx, "answer", "sibling");
    let from = graph_point(&workspace, cx, Position { x: 450., y: 390. });
    let to = graph_point(&workspace, cx, Position { x: 200., y: 170. });
    drag(
        cx,
        from,
        to,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    let state = read_state(&workspace, cx);
    assert_eq!(state.editor.project.graph, before);
    assert!(state.editor.selection.is_empty());
    let pan = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport.pan);
    let from = graph_point(&workspace, cx, Position { x: 550., y: 370. });
    drag(
        cx,
        from,
        from + point(px(40.), px(30.)),
        Modifiers::default(),
    );
    let after = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport.pan);
    assert_eq!(
        after,
        Position {
            x: pan.x + 40.,
            y: pan.y + 30.
        }
    );
}

#[gpui::test]
fn resized_canvas_minimap_centers_the_clicked_world_point(cx: &mut TestAppContext) {
    use crate::viewport::{Viewport, NODE_HEIGHT, NODE_WIDTH};
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    cx.run_until_parked();
    let canvas_bounds = cx.debug_bounds("graph-canvas").unwrap();
    let view = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport);
    assert_eq!(view.width, f32::from(canvas_bounds.size.width) as f64);
    assert_eq!(view.height, f32::from(canvas_bounds.size.height) as f64);
    let graph = read_state(&workspace, cx).editor.project.graph;
    let mut mini = Viewport {
        width: 180.,
        height: 120.,
        ..Default::default()
    };
    mini.fit(graph.layout.values().copied());
    let p = graph.layout["sibling"];
    let center = Position {
        x: p.x + NODE_WIDTH / 2.,
        y: p.y + NODE_HEIGHT / 2.,
    };
    let target = mini.screen(center);
    let bounds = cx.debug_bounds("minimap").expect("minimap bounds");
    cx.simulate_click(
        bounds.origin + point(px(target.x as f32), px(target.y as f32)),
        Modifiers::default(),
    );
    let bounds = cx.debug_bounds("graph-canvas").unwrap();
    let actual = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport.screen(center));
    assert!(
        (actual.x - f32::from(bounds.size.width) as f64 / 2.).abs() < 1.,
        "actual={actual:?}, bounds={bounds:?}, target={target:?}"
    );
    assert!(
        (actual.y - f32::from(bounds.size.height) as f64 / 2.).abs() < 1.,
        "actual={actual:?}, bounds={bounds:?}, target={target:?}"
    );
    let mini_bounds = cx.debug_bounds("minimap").unwrap();
    let source = mini_bounds.origin + point(px(target.x as f32), px(target.y as f32));
    drag(
        cx,
        source,
        source + point(px(5.), px(-3.)),
        Modifiers::default(),
    );
    let shifted = Position {
        x: center.x + 5. / mini.zoom,
        y: center.y - 3. / mini.zoom,
    };
    let actual = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport.screen(shifted));
    assert!((actual.x - f32::from(bounds.size.width) as f64 / 2.).abs() < 1.);
    assert!((actual.y - f32::from(bounds.size.height) as f64 / 2.).abs() < 1.);
}

#[gpui::test]
fn side_panel_drag_resizes_the_actual_panel_and_stops_after_release(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("answer".into(), false, window, cx)
        })
    });
    cx.run_until_parked();
    let handle = cx
        .debug_bounds("panel-resize")
        .expect("resize handle")
        .center();
    let before = workspace.read_with(cx, |this, _| this.panel_width);
    drag(
        cx,
        handle,
        handle + point(px(-80.), px(0.)),
        Modifiers::default(),
    );
    let after = workspace.read_with(cx, |this, _| this.panel_width);
    assert!(
        (after - before - 80.).abs() < 5.,
        "before={before}, after={after}"
    );
    assert!(!workspace.read_with(cx, |this, _| this.resizing));
    let bounds = cx.debug_bounds("side-panel").expect("side panel");
    assert!((f32::from(bounds.size.width) - after).abs() < 1.);
    cx.simulate_mouse_move(handle, None, Modifiers::default());
    assert_eq!(workspace.read_with(cx, |this, _| this.panel_width), after);
    let handle = cx.debug_bounds("panel-resize").unwrap().center();
    drag(cx, handle, point(px(1430.), handle.y), Modifiers::default());
    assert_eq!(workspace.read_with(cx, |this, _| this.panel_width), 200.);
    let handle = cx.debug_bounds("panel-resize").unwrap().center();
    drag(cx, handle, point(px(1.), handle.y), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |this, _| this.panel_width),
        1440. * 0.8
    );
}

#[gpui::test]
fn preview_copy_and_provenance_disclosures_preserve_source_and_reset_on_node_change(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    let original = "## Source\n\nA **Markdown** response.\n";
    workspace.update(cx, |this, _| {
        let node = this.editor.project.graph.nodes.get_mut("answer").unwrap();
        node.content = original.into();
        let thoughttree_gpui_model::NodeKind::Assistant(assistant) = &mut node.kind else {
            unreachable!()
        };
        assistant.provenance = thoughttree_gpui_model::normalize_provenance(&serde_json::json!({
            "completeness":"complete","references":[],
            "activity":[{"type":"commentary","content":"Synthetic progress detail","timestamp":1}]
        }));
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("answer".into(), false, window, cx)
        })
    });
    cx.run_until_parked();
    let copy = cx.debug_bounds("copy-node").expect("Copy action").center();
    cx.simulate_click(copy, Modifiers::default());
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(original)
    );
    assert!(!workspace.read_with(cx, |this, _| this.activity_expanded));
    let toggle = cx
        .debug_bounds("provenance-toggle")
        .expect("provenance disclosure")
        .center();
    cx.simulate_click(toggle, Modifiers::default());
    assert!(workspace.read_with(cx, |this, _| this.activity_expanded));
    let activity = cx
        .debug_bounds("activity-0")
        .expect("activity disclosure")
        .center();
    cx.simulate_click(activity, Modifiers::default());
    assert!(workspace.read_with(cx, |this, _| this.raw_expanded.contains("0")));
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("sibling".into(), false, window, cx)
        })
    });
    assert!(!workspace.read_with(cx, |this, _| this.activity_expanded));
    assert!(workspace.read_with(cx, |this, _| this.raw_expanded.is_empty()));
}

#[gpui::test]
fn empty_or_pending_file_mentions_close_with_escape_without_leaving_the_editor(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), true, window, cx)
        })
    });
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("@missing");
    assert!(workspace.read_with(cx, |this, _| this.mention_open()));
    cx.simulate_keystrokes("escape");
    assert!(read_state(&workspace, cx).editing);
    assert!(!workspace.read_with(cx, |this, _| this.mention_open()));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    assert!(!workspace.read_with(cx, |this, _| this.mention_open()));
    cx.simulate_input("-still-missing");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |this, _| this.mention_open()
        && this.mentions.is_empty()));
    cx.simulate_keystrokes("escape");
    assert!(read_state(&workspace, cx).editing);
    assert!(!workspace.read_with(cx, |this, _| this.mention_open()));
    for settled in [false, true] {
        for key in ["enter", "tab", "up", "down"] {
            cx.update(|window, cx| {
                let input = workspace.read(cx).input.clone();
                input.update(cx, |input, cx| input.focus(window, cx));
            });
            cx.simulate_keystrokes("cmd-a");
            cx.simulate_input("first line\n\nlast line");
            cx.update(|window, cx| {
                let input = workspace.read(cx).input.clone();
                input.update(cx, |input, cx| {
                    input.set_cursor_position(
                        gpui_component::input::Position::new(1, 0),
                        window,
                        cx,
                    )
                });
            });
            cx.simulate_input("@missing");
            if settled {
                cx.executor()
                    .advance_clock(std::time::Duration::from_millis(100));
                cx.run_until_parked();
            }
            assert!(workspace.read_with(cx, |this, _| this.mention_open()
                && this.mentions.is_empty()));
            cx.simulate_keystrokes(key);
            let (value, cursor) = workspace.read_with(cx, |this, cx| {
                (
                    this.input.read(cx).value().to_string(),
                    this.input.read(cx).cursor(),
                )
            });
            match key {
                "enter" => assert_eq!(value, "first line\n@missing\n\nlast line"),
                "tab" => {
                    assert_eq!(value, "first line\n@missing\nlast line");
                    assert!(!cx.update(|window, cx| workspace
                        .read(cx)
                        .input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)));
                }
                "up" => assert!(cursor < "first line\n".len()),
                "down" => assert!(cursor >= "first line\n@missing\n".len()),
                _ => unreachable!(),
            }
            assert!(read_state(&workspace, cx).editing);
        }
    }
}

#[gpui::test]
fn focused_rich_answer_select_all_copies_exact_markdown_without_selecting_graph(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    let source = "# An answer\n\n**Bold** text with $x^2$.\n\n```mermaid\ngraph LR\nA-->B\n```\n";
    workspace.update(cx, |this, _| {
        this.editor.project.graph.nodes["answer"].content = source.into()
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("answer".into(), false, window, cx)
        })
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("rich-answer").expect("rich answer");
    cx.simulate_click(bounds.origin + point(px(3.), px(3.)), Modifiers::default());
    cx.simulate_keystrokes("cmd-a cmd-c");
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(source)
    );
    assert!(read_state(&workspace, cx).editor.selection.len() < 3);
}

#[gpui::test]
fn composing_text_cannot_jump_palette_or_generate_a_turn(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_keystrokes("cmd-k");
    cx.update(|window, cx| {
        workspace
            .read(cx)
            .palette_input
            .clone()
            .update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "Rust", Some(0..4), window, cx);
            })
    });
    cx.simulate_keystrokes("tab cmd-enter");
    assert!(read_state(&workspace, cx).palette.is_some());
    assert_eq!(
        read_state(&workspace, cx).editor.project.graph.nodes.len(),
        3
    );
    cx.update(|window, cx| {
        workspace
            .read(cx)
            .palette_input
            .clone()
            .update(cx, |input, cx| input.unmark_text(window, cx))
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), true, window, cx)
        })
    });
    cx.update(|window, cx| {
        workspace.read(cx).input.clone().update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "質問", Some(0..2), window, cx);
        })
    });
    cx.simulate_keystrokes("cmd-enter");
    assert_eq!(
        read_state(&workspace, cx).editor.project.graph.nodes.len(),
        3
    );
    assert!(read_state(&workspace, cx).editor.active_turns.is_empty());
}

fn double_click(cx: &mut VisualTestContext, position: Point<Pixels>) {
    cx.simulate_event(gpui::MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
}

#[gpui::test]
fn selection_pane_clear_double_click_and_card_toggle_keep_preview_state_consistent(
    cx: &mut TestAppContext,
) {
    let (directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    cx.run_until_parked();
    let user = graph_point(&workspace, cx, Position { x: 40., y: 40. });
    cx.simulate_click(user, Modifiers::default());
    assert_eq!(
        read_state(&workspace, cx)
            .editor
            .selection
            .first()
            .map(String::as_str),
        Some("question")
    );
    assert!(!read_state(&workspace, cx).editor.is_dirty());
    double_click(cx, user);
    assert!(read_state(&workspace, cx).editing);
    let blank = graph_point(&workspace, cx, Position { x: 510., y: 430. });
    cx.simulate_click(blank, Modifiers::default());
    assert!(read_state(&workspace, cx).preview_id.is_none());
    assert!(read_state(&workspace, cx).editor.selection.is_empty());
    assert!(!read_state(&workspace, cx).editing);
    let assistant = graph_point(&workspace, cx, Position { x: 40., y: 240. });
    double_click(cx, assistant);
    assert_eq!(
        read_state(&workspace, cx).preview_id.as_deref(),
        Some("answer")
    );
    assert!(!read_state(&workspace, cx).editing);
    let caret = cx.debug_bounds("preview-answer").unwrap().center();
    cx.simulate_click(caret, Modifiers::default());
    assert!(read_state(&workspace, cx).preview_id.is_none());
    std::fs::write(directory.path().join("vault/note.md"), "note").unwrap();
    workspace.update(cx, |this, cx| {
        this.editor.project.graph.add_node(
            GraphNode::file(
                "file",
                thoughttree_gpui_model::FileData {
                    path: "note.md".into(),
                    name: "note.md".into(),
                    mime_type: "text/markdown".into(),
                    size: 4,
                    seen_size: 4,
                    seen_mtime: 0,
                },
                3.,
            ),
            Position { x: 500., y: 0. },
        );
        this.refresh(cx);
    });
    cx.run_until_parked();
    let file = graph_point(&workspace, cx, Position { x: 540., y: 40. });
    double_click(cx, file);
    assert_eq!(
        read_state(&workspace, cx).preview_id.as_deref(),
        Some("file")
    );
    assert!(!read_state(&workspace, cx).editing);
    cx.simulate_keystrokes("escape");
    workspace.update(cx, |this, _| {
        this.file_status
            .insert("file".into(), Err("synthetic stale status".into()));
        this.file_preview_errors
            .insert("file".into(), "synthetic stale preview".into());
    });
    cx.simulate_keystrokes("backspace");
    workspace.read_with(cx, |this, _| {
        assert!(!this.editor.project.graph.nodes.contains_key("file"));
        assert!(!this.file_status.contains_key("file"));
        assert!(!this.file_previews.contains_key("file"));
        assert!(!this.file_preview_errors.contains_key("file"));
        assert!(this.preview_id.is_none());
        assert!(this.editor.selection.is_empty());
    });
    let position = Position { x: 560., y: 430. };
    let blank = graph_point(&workspace, cx, position);
    double_click(cx, blank);
    let state = read_state(&workspace, cx);
    let id = state.preview_id.unwrap();
    assert!(state.editing);
    assert_eq!(state.editor.project.graph.layout[&id], position);
}

#[gpui::test]
fn node_drag_snaps_guides_then_persists_the_final_layout(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.run_until_parked();
    let from = graph_point(&workspace, cx, Position { x: 40., y: 40. });
    let to = from + point(px(235.), px(198.));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    assert!(workspace.read_with(cx, |this, cx| this.canvas.read(cx).guide_count()) >= 2);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |this, cx| this.canvas.read(cx).guide_count()),
        0
    );
    let state = read_state(&workspace, cx);
    assert_eq!(
        state.editor.project.graph.layout["question"],
        Position { x: 240., y: 200. }
    );
    assert!(state.editor.is_dirty());
    let reloaded = Project::from_json(&state.editor.project.to_json().unwrap()).unwrap();
    assert_eq!(reloaded.graph.layout, state.editor.project.graph.layout);
}

#[gpui::test]
fn wheel_zoom_buttons_fit_and_native_menu_search_control_the_view(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    cx.run_until_parked();
    let initial = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport);
    let center = cx.debug_bounds("graph-canvas").unwrap().center();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: center,
        delta: gpui::ScrollDelta::Pixels(point(px(20.), px(30.))),
        ..Default::default()
    });
    let pan = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport.pan);
    assert_eq!(
        pan,
        Position {
            x: initial.pan.x + 20.,
            y: initial.pan.y + 30.
        }
    );
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: center,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-30.))),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        ..Default::default()
    });
    assert!(workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport.zoom) > initial.zoom);
    for id in ["zoom-in", "zoom-out", "fit"] {
        let bounds = cx.debug_bounds(id).unwrap();
        cx.simulate_click(bounds.center(), Modifiers::default());
    }
    workspace.read_with(cx, |this, cx| {
        let view = this.canvas.read(cx).viewport;
        for p in this.editor.project.graph.layout.values() {
            let screen = view.screen(*p);
            assert!(screen.x >= 0. && screen.y >= 0.);
            assert!(
                screen.x + 170. * view.zoom <= view.width
                    && screen.y + 120. * view.zoom <= view.height
            );
        }
    });
    cx.dispatch_action(crate::commands::SearchNodes);
    assert!(read_state(&workspace, cx).palette.is_some());
    cx.simulate_keystrokes("tab");
    assert!(cx.update(|window, cx| workspace
        .read(cx)
        .palette_input
        .read(cx)
        .focus_handle(cx)
        .is_focused(window)));
    cx.simulate_keystrokes("escape");
    cx.dispatch_action(crate::commands::ShowSettings);
    assert!(matches!(
        read_state(&workspace, cx).modal,
        Some(crate::dialogs::Modal::Settings)
    ));
}

#[gpui::test]
fn native_context_actions_create_reply_and_delete_the_selected_node(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    cx.run_until_parked();
    let assistant = graph_point(&workspace, cx, Position { x: 40., y: 240. });
    cx.simulate_mouse_down(assistant, MouseButton::Right, Modifiers::default());
    cx.simulate_keystrokes("escape");
    assert!(read_state(&workspace, cx).modal.is_none());
    cx.simulate_mouse_down(assistant, MouseButton::Right, Modifiers::default());
    cx.simulate_click(point(px(4.), px(4.)), Modifiers::default());
    assert!(read_state(&workspace, cx).modal.is_none());
    cx.simulate_mouse_down(assistant, MouseButton::Right, Modifiers::default());
    let reply = cx.debug_bounds("context-reply").unwrap().center();
    cx.simulate_click(reply, Modifiers::default());
    let created = read_state(&workspace, cx).preview_id.unwrap();
    assert_eq!(
        read_state(&workspace, cx)
            .editor
            .project
            .graph
            .parents(&created),
        ["answer"]
    );
    assert!(read_state(&workspace, cx).editing);
    cx.simulate_keystrokes("escape escape");
    let p = read_state(&workspace, cx).editor.project.graph.layout[&created];
    let node = graph_point(
        &workspace,
        cx,
        Position {
            x: p.x + 40.,
            y: p.y + 40.,
        },
    );
    cx.simulate_mouse_down(node, MouseButton::Right, Modifiers::default());
    let delete = cx.debug_bounds("context-delete").unwrap().center();
    cx.simulate_click(delete, Modifiers::default());
    assert!(!read_state(&workspace, cx)
        .editor
        .project
        .graph
        .nodes
        .contains_key(&created));
    let p = Position { x: 500., y: 400. };
    let blank = graph_point(&workspace, cx, p);
    cx.simulate_mouse_down(blank, MouseButton::Right, Modifiers::default());
    assert!(cx.debug_bounds("context-add-file").is_some());
    let new = cx.debug_bounds("context-new").unwrap().center();
    cx.simulate_click(new, Modifiers::default());
    let state = read_state(&workspace, cx);
    assert_eq!(
        state.editor.project.graph.layout[&state.preview_id.unwrap()],
        p
    );
}

#[gpui::test]
fn native_export_menu_writes_exact_ancestor_ids_and_every_branch(cx: &mut TestAppContext) {
    let (directory, workspace, cx, _events) = workspace(cx);
    workspace.update(cx, |this, _| {
        this.editor.project.graph.nodes["sibling"].content =
            this.editor.project.graph.nodes["answer"].content.clone();
        this.editor.selection.insert("answer".into());
    });
    cx.dispatch_action(crate::commands::ExportThread);
    cx.simulate_new_path_selection(|dir| Some(dir.join("thread.md")));
    cx.run_until_parked();
    let thread = std::fs::read_to_string(directory.path().join("vault/thread.md")).unwrap();
    assert_eq!(thread.matches("## Assistant").count(), 1);
    assert!(thread.contains("A question about Rust"));
    cx.dispatch_action(crate::commands::ExportAll);
    cx.simulate_new_path_selection(|dir| Some(dir.join("all.md")));
    cx.run_until_parked();
    let all = std::fs::read_to_string(directory.path().join("vault/all.md")).unwrap();
    assert_eq!(all.matches("## Assistant").count(), 2);
}

#[gpui::test]
fn imported_picker_result_opens_unsaved_title_and_malformed_import_keeps_it(
    cx: &mut TestAppContext,
) {
    let (directory, workspace, cx, _events) = workspace(cx);
    workspace.update(cx, |this, _| {
        this.editor.project.project_model_preferences =
            Some([("codex".into(), "old-project-model".into())].into());
        this.editor.project.project_effort_preferences = Some(
            [(
                "codex".into(),
                thoughttree_gpui_model::ReasoningEffort::High,
            )]
            .into(),
        );
    });
    let path = directory.path().join("kagi.json");
    std::fs::write(
        &path,
        include_bytes!("../../../test/fixtures/kagi-export-v1.json"),
    )
    .unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.import_picked(vec![path.clone()], window, cx)
        })
    });
    cx.run_until_parked();
    let before = read_state(&workspace, cx);
    assert_eq!(before.title, "Example research conversation");
    assert_eq!(before.editor.project.graph.nodes.len(), 4);
    assert!(before.editor.is_dirty());
    assert!(before.editor.project.project_model_preferences.is_none());
    assert!(before.editor.project.project_effort_preferences.is_none());
    assert!(workspace.read_with(cx, |this, _| this.project_path.is_none()));
    std::fs::write(&path, b"malformed").unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.import_picked(vec![path], window, cx))
    });
    cx.run_until_parked();
    assert_eq!(
        read_state(&workspace, cx).editor.project,
        before.editor.project
    );
    assert!(workspace.read_with(cx, |this, _| this.notice.is_some()));
}

#[gpui::test]
fn image_drop_callback_attaches_to_its_user_even_when_another_node_is_previewed(
    cx: &mut TestAppContext,
) {
    let (directory, workspace, cx, _events) = workspace(cx);
    let path = directory.path().join("image.png");
    image::DynamicImage::new_rgba8(2, 2).save(&path).unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("answer".into(), false, window, cx)
        })
    });
    workspace.update(cx, |this, cx| {
        this.canvas.update(cx, |_, cx| {
            cx.emit(GraphEvent::DropImages(vec![path], "question".into()))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        read_state(&workspace, cx).editor.project.graph.nodes["question"]
            .images()
            .len(),
        1
    );
    assert!(
        read_state(&workspace, cx).editor.project.graph.nodes["answer"]
            .images()
            .is_empty()
    );
    assert_eq!(
        read_state(&workspace, cx).preview_id.as_deref(),
        Some("answer")
    );
}

#[gpui::test]
fn palette_caps_highlighted_results_and_new_modals_close_its_focus_scope(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    workspace.update(cx, |this, cx| {
        for index in 0..30 {
            this.editor.project.graph.add_node(
                GraphNode::user(format!("match-{index}"), "Résumé match", index as f64),
                Position::default(),
            );
        }
        this.refresh(cx);
    });
    cx.simulate_keystrokes("ctrl-k");
    cx.simulate_input("résumé match");
    workspace.read_with(cx, |this, _| {
        let palette = this.palette.as_ref().unwrap();
        assert_eq!(palette.hits.len(), 20);
        assert_eq!(palette.total, 30);
        assert!(palette
            .rendered_hits
            .iter()
            .all(|hit| !hit.title.spans.is_empty()));
    });
    cx.dispatch_action(crate::commands::ShowSettings);
    assert!(read_state(&workspace, cx).palette.is_none());
    cx.simulate_keystrokes("escape ctrl-k");
    assert!(read_state(&workspace, cx).palette.is_some());
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.open_modal(crate::dialogs::Modal::Conflict, window, cx)
        })
    });
    assert!(read_state(&workspace, cx).palette.is_none());
    cx.simulate_keystrokes("ctrl-k");
    assert!(read_state(&workspace, cx).palette.is_none());
}

#[gpui::test]
fn preview_edit_and_settings_shortcuts_preserve_input_focus_rules(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    workspace.update(cx, |this, cx| {
        this.editor.selection.insert("question".into());
        this.refresh(cx);
    });
    cx.simulate_keystrokes("space");
    assert_eq!(
        read_state(&workspace, cx).preview_id.as_deref(),
        Some("question")
    );
    cx.simulate_keystrokes("space");
    assert!(read_state(&workspace, cx).preview_id.is_none());
    cx.simulate_keystrokes("e");
    assert!(read_state(&workspace, cx).editing);
    // The test platform cannot open an OS picker: reaching it here would panic.
    cx.simulate_keystrokes("cmd-o");
    assert!(read_state(&workspace, cx).editing);
    cx.simulate_keystrokes("cmd-,");
    assert!(matches!(
        read_state(&workspace, cx).modal,
        Some(crate::dialogs::Modal::Settings)
    ));
}

#[gpui::test]
fn concurrent_generation_rejects_blocked_ancestors_but_accepts_an_independent_user(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    workspace.update(cx, |this, cx| {
        this.editor.project.graph.add_node(
            GraphNode::user("independent", "fixture:nopermission", 5.),
            Position { x: 500., y: 0. },
        );
        this.editor.start_turn("answer", "existing").unwrap();
        this.refresh(cx);
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), true, window, cx)
        })
    });
    assert!(!read_state(&workspace, cx).editing);
    cx.simulate_keystrokes("cmd-enter");
    assert_eq!(
        read_state(&workspace, cx).editor.project.graph.nodes.len(),
        4
    );
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("independent".into(), true, window, cx)
        })
    });
    cx.simulate_keystrokes("cmd-enter");
    let state = read_state(&workspace, cx);
    assert_eq!(state.editor.project.graph.nodes.len(), 5);
    assert_eq!(state.editor.active_turns.len(), 2);
    assert!(state.editor.active_turns.contains_key("answer"));
}

#[gpui::test]
fn card_and_panel_generate_without_editing_and_preserve_their_preview_behavior(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    workspace.update(cx, |this, cx| {
        this.editor.project.graph.add_node(
            GraphNode::file(
                "file",
                thoughttree_gpui_model::FileData {
                    path: "note.md".into(),
                    name: "note.md".into(),
                    mime_type: "text/markdown".into(),
                    size: 4,
                    seen_size: 4,
                    seen_mtime: 0,
                },
                3.,
            ),
            Position { x: 500., y: 0. },
        );
        this.refresh(cx);
    });
    // GPUI 0.2.2 retains old debug bounds. Check absent role controls before a
    // user panel has ever mounted, then exercise both real generation buttons.
    for id in ["answer", "file"] {
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| this.preview(id.into(), false, window, cx))
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("generate-panel").is_none());
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let generate = cx.debug_bounds("action-question").unwrap().center();
    cx.simulate_click(generate, Modifiers::default());
    let state = read_state(&workspace, cx);
    assert!(!state.editing);
    assert!(state.preview_id.is_none());
    let (card_assistant, card_turn) = state.editor.active_turns.iter().next().unwrap();
    assert_eq!(
        state.editor.project.graph.parents(card_assistant),
        ["question"]
    );
    events
        .try_send(DesktopEvent::PromptFinished {
            node_id: card_assistant.clone(),
            turn_id: card_turn.clone(),
            result: Ok(String::new()),
        })
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), false, window, cx)
        })
    });
    assert!(!read_state(&workspace, cx).editing);
    let generate = cx.debug_bounds("generate-panel").unwrap().center();
    cx.simulate_click(generate, Modifiers::default());
    let state = read_state(&workspace, cx);
    assert!(!state.editing);
    let (panel_assistant, panel_turn) = state.editor.active_turns.iter().next().unwrap();
    assert_ne!(panel_assistant, card_assistant);
    assert_eq!(state.preview_id.as_ref(), Some(panel_assistant));
    assert_eq!(
        state.editor.project.graph.parents(panel_assistant),
        ["question"]
    );
    cx.run_until_parked();
    assert!(cx.debug_bounds("generating-label").is_some());
    events
        .try_send(DesktopEvent::PromptFinished {
            node_id: panel_assistant.clone(),
            turn_id: panel_turn.clone(),
            result: Ok(String::new()),
        })
        .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn native_toolbar_renders_project_save_time_dirty_streaming_and_node_count(
    cx: &mut TestAppContext,
) {
    use chrono::{Local, TimeZone};
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    workspace.update(cx, |this, cx| {
        this.title = "Toolbar fixture".into();
        this.saved_at = Some(
            Local
                .with_ymd_and_hms(2026, 10, 3, 14, 5, 9)
                .single()
                .unwrap()
                .timestamp_millis() as f64,
        );
        this.refresh(cx);
    });
    cx.run_until_parked();
    for selector in [
        "toolbar-title:Toolbar fixture",
        "toolbar-status:Saved at 14:05:09",
        "toolbar-count:3 nodes",
    ] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
    }
    workspace.update(cx, |this, cx| {
        this.editor
            .edit(|project| {
                project.graph.add_node(
                    GraphNode::user("fourth", "New note", 4.),
                    Position::default(),
                );
                Ok(())
            })
            .unwrap();
        this.refresh(cx);
    });
    cx.run_until_parked();
    for selector in [
        "toolbar-title:Toolbar fixture *",
        "toolbar-status:Unsaved changes",
        "toolbar-count:4 nodes",
    ] {
        assert!(cx.debug_bounds(selector).is_some());
    }
    workspace.update(cx, |this, cx| {
        this.editor.start_turn("answer", "streaming").unwrap();
        this.refresh(cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("toolbar-status:Responding…").is_some());
}

#[gpui::test]
fn native_toolbar_exposes_all_actions_and_respects_selection_and_turn_guards(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    cx.run_until_parked();
    for selector in [
        "new",
        "open",
        "import",
        "save",
        "tidy",
        "reply",
        "export-thread",
        "export-all",
        "search",
        "recovery",
        "settings",
    ] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
    }
    let reply = cx.debug_bounds("reply").unwrap().center();
    cx.simulate_click(reply, Modifiers::default());
    assert_eq!(
        read_state(&workspace, cx).editor.project.graph.nodes.len(),
        3
    );
    workspace.update(cx, |this, cx| {
        this.editor.selection.insert("answer".into());
        this.editor.start_turn("answer", "toolbar-guard").unwrap();
        this.refresh(cx);
    });
    cx.run_until_parked();
    for selector in ["new", "open", "import", "recovery", "reply"] {
        let button = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(button, Modifiers::default());
        let state = read_state(&workspace, cx);
        assert!(state.modal.is_none());
        assert_eq!(state.editor.project.graph.nodes.len(), 3);
        assert_eq!(state.editor.active_turns.len(), 1);
    }
    workspace.update(cx, |this, cx| {
        this.editor.finish_turn("answer", "toolbar-guard");
        this.refresh(cx);
    });
    let reply = cx.debug_bounds("reply").unwrap().center();
    cx.simulate_click(reply, Modifiers::default());
    let state = read_state(&workspace, cx);
    assert_eq!(state.editor.project.graph.nodes.len(), 4);
    assert!(state.editing);
    let created = state.preview_id.unwrap();
    assert_eq!(state.editor.project.graph.parents(&created), ["answer"]);
    cx.simulate_keystrokes("escape escape");
    let search = cx.debug_bounds("search").unwrap().center();
    cx.simulate_click(search, Modifiers::default());
    assert!(read_state(&workspace, cx).palette.is_some());
    cx.simulate_keystrokes("escape");
    let settings = cx.debug_bounds("settings").unwrap().center();
    cx.simulate_click(settings, Modifiers::default());
    assert!(matches!(
        read_state(&workspace, cx).modal,
        Some(crate::dialogs::Modal::Settings)
    ));
    cx.simulate_keystrokes("escape");
    let recovery = cx.debug_bounds("recovery").unwrap().center();
    cx.simulate_click(recovery, Modifiers::default());
    assert!(matches!(
        read_state(&workspace, cx).modal,
        Some(crate::dialogs::Modal::Recovery)
    ));
}

#[gpui::test]
fn expanded_fixture_provenance_scrolls_with_native_line_and_pixel_wheels(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1360.), px(900.)));
    workspace.update(cx, |this, cx| {
        this.editor = Editor::new(
            Project::from_json(include_str!(
                "../../../docs/gpui/fixtures/parity.thoughttree"
            ))
            .unwrap(),
        );
        this.panel_width = 600.;
        this.refresh(cx);
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("parity-synthesis".into(), false, window, cx);
            this.activity_expanded = true;
            this.raw_expanded = (0..5).map(|index| index.to_string()).collect();
            cx.notify();
        })
    });
    cx.run_until_parked();
    // Generated figures decode after the first paint. Let their measured size
    // settle before comparing displacement caused by the wheel itself.
    for _ in 0..6 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let scroll = cx.debug_bounds("panel-scroll").unwrap();
    let last = cx.debug_bounds("activity-4").unwrap();
    assert!(
        last.bottom() > scroll.bottom(),
        "fixture must overflow: last={last:?}, scroll={scroll:?}"
    );
    let graph = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport);
    for (step, (delta, touch_phase)) in [
        (
            gpui::ScrollDelta::Lines(point(0., -5.)),
            gpui::TouchPhase::Moved,
        ),
        (
            gpui::ScrollDelta::Pixels(point(px(0.), px(-120.))),
            gpui::TouchPhase::Started,
        ),
        (
            gpui::ScrollDelta::Pixels(point(px(0.), px(-120.))),
            gpui::TouchPhase::Moved,
        ),
        (
            gpui::ScrollDelta::Pixels(point(px(0.), px(-120.))),
            gpui::TouchPhase::Ended,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let before = cx.debug_bounds("activity-4").unwrap();
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: scroll.center(),
            delta,
            touch_phase,
            ..Default::default()
        });
        cx.run_until_parked();
        let after = cx.debug_bounds("activity-4").unwrap();
        assert!(after.top() < before.top(), "wheel step {step} did not move expanded evidence: before={before:?}, after={after:?}, scroll={scroll:?}");
    }
    workspace.read_with(cx, |this, cx| {
        assert_eq!(this.canvas.read(cx).viewport.pan, graph.pan);
        assert_eq!(this.canvas.read(cx).viewport.zoom, graph.zoom);
    });
}

#[gpui::test]
fn modal_palette_and_permission_scrolls_do_not_move_or_zoom_the_graph(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    for overlay in ["settings", "palette", "permission"] {
        match overlay {
            "settings" => cx.dispatch_action(crate::commands::ShowSettings),
            "palette" => cx.dispatch_action(crate::commands::SearchNodes),
            "permission" => workspace.update(cx, |this, cx| {
                this.permissions.push_back(Default::default());
                cx.notify();
            }),
            _ => unreachable!(),
        }
        cx.run_until_parked();
        let before = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport);
        for (position, command) in [
            (point(px(720.), px(600.)), false),
            (point(px(720.), px(600.)), true),
            (point(px(25.), px(200.)), false),
            (point(px(25.), px(200.)), true),
        ] {
            cx.simulate_event(gpui::ScrollWheelEvent {
                position,
                delta: gpui::ScrollDelta::Lines(point(0., -5.)),
                modifiers: Modifiers {
                    platform: command,
                    ..Default::default()
                },
                ..Default::default()
            });
            cx.run_until_parked();
            let after = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport);
            assert_eq!(
                after.pan, before.pan,
                "{overlay} wheel panned underlying Graph"
            );
            assert_eq!(
                after.zoom, before.zoom,
                "{overlay} wheel zoomed underlying Graph"
            );
        }
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.modal = None;
                this.palette = None;
                this.permissions.clear();
                this.focus.focus(window);
                cx.notify();
            })
        });
    }
}

#[gpui::test]
fn modal_palette_and_permission_clicks_preserve_background_selection_and_preview(
    cx: &mut TestAppContext,
) {
    use thoughttree_core::events::{PermissionRequestEvent, PermissionRequestOption};
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    for overlay in ["settings", "palette", "permission"] {
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.editor.selection.insert("answer".into());
                this.preview("answer".into(), false, window, cx);
            })
        });
        match overlay {
            "settings" => cx.dispatch_action(crate::commands::ShowSettings),
            "palette" => cx.dispatch_action(crate::commands::SearchNodes),
            "permission" => workspace.update(cx, |this, cx| {
                this.permissions.push_back(PermissionRequestEvent::new(
                    "request".into(),
                    "answer".into(),
                    "read".into(),
                    "Read file".into(),
                    "Read fixture?".into(),
                    vec![PermissionRequestOption {
                        id: "allow_once".into(),
                        label: "Allow once".into(),
                    }],
                ));
                cx.notify();
            }),
            _ => unreachable!(),
        }
        cx.run_until_parked();
        let target = if overlay == "permission" {
            cx.debug_bounds("permission-allow_once").unwrap().center()
        } else {
            point(px(25.), px(200.))
        };
        cx.simulate_click(target, Modifiers::default());
        let state = read_state(&workspace, cx);
        assert_eq!(
            state.preview_id.as_deref(),
            Some("answer"),
            "{overlay} click closed preview"
        );
        assert_eq!(
            state
                .editor
                .selection
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["answer"],
            "{overlay} click changed selection"
        );
        workspace.read_with(cx, |this, _| {
            assert!(this.modal.is_none());
            assert!(this.palette.is_none());
            assert!(this.permissions.is_empty());
        });
    }
}

#[gpui::test]
fn settings_body_keeps_scrolling_inside_the_mouse_blocking_overlay(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(600.)));
    cx.dispatch_action(crate::commands::ShowSettings);
    cx.run_until_parked();
    let body = cx.debug_bounds("dialog-scroll").unwrap();
    let first = cx.debug_bounds("settings-vault").unwrap();
    let close = cx.debug_bounds("dialog-close").unwrap();
    let graph = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport);
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: body.center(),
        delta: gpui::ScrollDelta::Lines(point(0., -5.)),
        ..Default::default()
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-vault").unwrap().top() < first.top());
    assert_eq!(cx.debug_bounds("dialog-close").unwrap(), close);
    workspace.read_with(cx, |this, cx| {
        assert_eq!(this.canvas.read(cx).viewport.pan, graph.pan);
        assert_eq!(this.canvas.read(cx).viewport.zoom, graph.zoom);
    });
}

#[gpui::test]
fn full_window_overlays_cover_toolbar_buttons(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    for overlay in ["settings", "palette", "permission"] {
        match overlay {
            "settings" => cx.dispatch_action(crate::commands::ShowSettings),
            "palette" => cx.dispatch_action(crate::commands::SearchNodes),
            "permission" => workspace.update(cx, |this, cx| {
                this.permissions.push_back(Default::default());
                cx.notify();
            }),
            _ => unreachable!(),
        }
        cx.run_until_parked();
        let button = cx.debug_bounds("settings").unwrap().center();
        cx.simulate_click(button, Modifiers::default());
        workspace.read_with(cx, |this, _| {
            assert!(
                this.modal.is_none(),
                "{overlay} let the Settings toolbar button activate"
            );
            assert!(this.palette.is_none());
            assert_eq!(this.permissions.len(), usize::from(overlay == "permission"));
        });
        workspace.update(cx, |this, cx| {
            this.permissions.clear();
            cx.notify();
        });
    }
}

#[gpui::test]
fn startup_and_settings_dialogs_remain_centered_within_the_full_window(cx: &mut TestAppContext) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    cx.simulate_resize(size(px(1440.), px(940.)));
    for modal in [
        crate::dialogs::Modal::Setup,
        crate::dialogs::Modal::Projects,
        crate::dialogs::Modal::Settings,
    ] {
        cx.update(|window, cx| workspace.update(cx, |this, cx| this.open_modal(modal, window, cx)));
        cx.run_until_parked();
        let overlay = cx.debug_bounds("desktop-overlay").unwrap();
        let dialog = cx.debug_bounds("desktop-dialog").unwrap();
        assert_eq!(overlay.origin, point(px(0.), px(0.)));
        assert_eq!(overlay.size, size(px(1440.), px(940.)));
        assert!((dialog.center().x - overlay.center().x).abs() <= px(1.));
        assert!((dialog.center().y - overlay.center().y).abs() <= px(1.));
        assert!(dialog.size.height <= px(940. * 0.9 + 1.));
        assert!(dialog.top() >= overlay.top() && dialog.bottom() <= overlay.bottom());
    }
}
