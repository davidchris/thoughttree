use gpui::{prelude::*, *};
use gpui_component::{input::Input, Icon, IconName, WindowExt};
use thoughttree_gpui_model::{search_nodes, GraphProvider, HighlightedText, LayoutOptions, Role};

use crate::{
    dialogs::Modal,
    theme,
    workspace::{PaletteState, Workspace},
};

const MAX_VISIBLE_HITS: usize = 20;

actions!(
    thoughttree,
    [
        NewProject,
        OpenProject,
        SaveProject,
        SaveProjectAs,
        ImportConversation,
        ExportThread,
        ExportAll,
        ShowSettings,
        SearchNodes,
        ShowRecovery,
        TidyGraph,
        Quit
    ]
);

pub(crate) fn init_menus(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, Some("ThoughtTree"))]);
    cx.set_menus(vec![
        Menu {
            name: "ThoughtTree".into(),
            items: vec![
                MenuItem::action("Settings…", ShowSettings),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit ThoughtTree", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Project…", NewProject),
                MenuItem::action("Open Project…", OpenProject),
                MenuItem::separator(),
                MenuItem::action("Save", SaveProject),
                MenuItem::action("Save As…", SaveProjectAs),
                MenuItem::separator(),
                MenuItem::action("Import Kagi Conversation…", ImportConversation),
                MenuItem::action("Export Thread…", ExportThread),
                MenuItem::action("Export All…", ExportAll),
                MenuItem::separator(),
                MenuItem::action("Recovery Snapshots…", ShowRecovery),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Search Nodes…", SearchNodes),
                MenuItem::action("Tidy Graph", TidyGraph),
            ],
        },
    ]);
    cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

fn saved_status(at: f64) -> String {
    chrono::DateTime::from_timestamp_millis(at as i64)
        .map(|time| {
            format!(
                "Saved at {}",
                time.with_timezone(&chrono::Local).format("%H:%M:%S")
            )
        })
        .unwrap_or_else(|| "Saved".into())
}

impl Workspace {
    pub(crate) fn menu_actions(&self, element: Div, cx: &Context<Self>) -> Div {
        element
            .on_action(cx.listener(|this, _: &NewProject, window, cx| this.new_project(window, cx)))
            .on_action(
                cx.listener(|this, _: &OpenProject, window, cx| this.open_dialog(window, cx)),
            )
            .on_action(cx.listener(|this, _: &SaveProject, window, cx| this.save(window, cx)))
            .on_action(cx.listener(|this, _: &SaveProjectAs, window, cx| this.save_as(window, cx)))
            .on_action(cx.listener(|this, _: &ImportConversation, window, cx| {
                this.import_dialog(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ExportThread, window, cx| this.export(false, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ExportAll, window, cx| this.export(true, window, cx)))
            .on_action(cx.listener(|this, _: &ShowSettings, window, cx| {
                this.open_modal(Modal::Settings, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &SearchNodes, window, cx| this.open_palette(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowRecovery, window, cx| {
                this.open_modal(Modal::Recovery, window, cx)
            }))
            .on_action(cx.listener(|this, _: &TidyGraph, _, cx| this.tidy(cx)))
            .on_action(cx.listener(|this, _: &Quit, window, cx| {
                if this.request_close(window, cx) {
                    cx.quit();
                }
            }))
    }
    /// Input actions run before element key listeners in GPUI. Intercept only
    /// our window so app shortcuts can own CmdEnter and image paste while
    /// ordinary text editing continues through the component's native actions.
    pub(crate) fn register_shortcuts(window: &Window, cx: &mut Context<Self>) -> Subscription {
        let owner = window.window_handle();
        let workspace = cx.weak_entity();
        cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != owner {
                return;
            }
            let _ = workspace.update(cx, |this, cx| {
                this.key_down(
                    &KeyDownEvent {
                        keystroke: event.keystroke.clone(),
                        is_held: false,
                    },
                    window,
                    cx,
                );
            });
        })
    }

    pub(crate) fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let graph = &self.editor.project.graph;
        let selected = self.editor.selection.first().cloned();
        let can_reply = selected
            .as_ref()
            .and_then(|id| graph.nodes.get(id))
            .is_some_and(|node| {
                node.role() == Role::Assistant && !self.editor.is_node_blocked(&node.id)
            });
        let idle = self.editor.active_turns.is_empty();
        let title = format!(
            "{}{}",
            self.title,
            if self.editor.is_dirty() { " *" } else { "" }
        );
        let save_status = if !idle {
            "Responding…".to_owned()
        } else if self.editor.is_dirty() {
            "Unsaved changes".to_owned()
        } else if let Some(at) = self.saved_at {
            saved_status(at)
        } else {
            String::new()
        };
        let node_count = format!("{} nodes", graph.nodes.len());
        div()
            .id("toolbar")
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_2()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .min_w(px(145.))
                    .child(
                        Icon::new(IconName::Map)
                            .size_5()
                            .text_color(theme::accent()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .max_w(px(210.))
                            .child(
                                div()
                                    .debug_selector(|| format!("toolbar-title:{title}"))
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .debug_selector(|| format!("toolbar-status:{save_status}"))
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .child(save_status),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .flex_wrap()
                    .child(toolbar_button(
                        "new",
                        "New",
                        IconName::Plus,
                        idle,
                        cx,
                        |this, window, cx| this.new_project(window, cx),
                    ))
                    .child(toolbar_button(
                        "open",
                        "Open",
                        IconName::FolderOpen,
                        idle,
                        cx,
                        |this, window, cx| this.open_dialog(window, cx),
                    ))
                    .child(toolbar_button(
                        "import",
                        "Import",
                        IconName::ArrowDown,
                        idle,
                        cx,
                        |this, window, cx| this.import_dialog(window, cx),
                    ))
                    .child(toolbar_button(
                        "save",
                        "Save",
                        IconName::File,
                        self.editor.is_dirty() || self.project_path.is_none(),
                        cx,
                        |this, window, cx| this.save(window, cx),
                    ))
                    .child(separator())
                    .child(toolbar_button(
                        "tidy",
                        "Tidy graph",
                        IconName::LayoutDashboard,
                        !graph.nodes.is_empty(),
                        cx,
                        |this, _, cx| this.tidy(cx),
                    ))
                    .child(toolbar_button(
                        "reply",
                        "Reply",
                        IconName::Redo2,
                        can_reply,
                        cx,
                        move |this, window, cx| {
                            if let Some(id) = selected.clone() {
                                this.reply(id, window, cx);
                            }
                        },
                    ))
                    .child(separator())
                    .child(toolbar_button(
                        "export-thread",
                        "Export Thread",
                        IconName::ExternalLink,
                        !self.editor.selection.is_empty(),
                        cx,
                        |this, window, cx| this.export(false, window, cx),
                    ))
                    .child(toolbar_button(
                        "export-all",
                        "Export All",
                        IconName::GalleryVerticalEnd,
                        !graph.nodes.is_empty(),
                        cx,
                        |this, window, cx| this.export(true, window, cx),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(toolbar_button(
                        "search",
                        "Search",
                        IconName::Search,
                        true,
                        cx,
                        |this, window, cx| this.open_palette(window, cx),
                    ))
                    .child(toolbar_button(
                        "recovery",
                        "Recovery",
                        IconName::Undo2,
                        idle,
                        cx,
                        |this, window, cx| this.open_modal(Modal::Recovery, window, cx),
                    ))
                    .child(toolbar_button(
                        "settings",
                        "Settings",
                        IconName::Settings,
                        true,
                        cx,
                        |this, window, cx| this.open_modal(Modal::Settings, window, cx),
                    ))
                    .child(
                        div()
                            .debug_selector(|| format!("toolbar-count:{node_count}"))
                            .ml_2()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(node_count),
                    ),
            )
            .into_any_element()
    }

    fn tidy(&mut self, cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        self.change(
            |project| project.graph.auto_layout(LayoutOptions::default()),
            cx,
        );
        self.canvas.update(cx, |canvas, cx| canvas.fit(cx));
    }

    pub(crate) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.to_lowercase();
        let modifiers = event.keystroke.modifiers;
        let command = modifiers.platform || modifiers.control;
        let composing = window.focused_input(cx).is_some_and(|input| {
            input.update(cx, |input, cx| {
                input.focus_handle(cx).is_focused(window)
                    && input.marked_text_range(window, cx).is_some()
            })
        });
        if composing {
            return;
        }
        if self.modal.is_some() || !self.permissions.is_empty() {
            if key == "escape"
                && self.permissions.is_empty()
                && !matches!(self.modal, Some(Modal::Setup | Modal::ChangingVault))
            {
                self.modal = None;
                self.focus.focus(window);
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        if self.palette.is_some() {
            if self.palette_key(&key, command, window, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if self.rich.update(cx, |rich, cx| {
            rich.handle_shortcut(&key, command, window, cx)
        }) {
            cx.stop_propagation();
            return;
        }
        // gpui-component retains its last focused Input after that Input is
        // unmounted. Inspect native focus before suppressing graph shortcuts.
        let input_focused = window
            .focused_input(cx)
            .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window));
        if self.mention_open() && !command && self.mention_key(&key, window, cx) {
            cx.stop_propagation();
            return;
        }
        let handled = if command {
            self.command_key(&key, modifiers.shift, input_focused, window, cx)
        } else if key == "escape" {
            self.escape(window, cx)
        } else if !input_focused && !modifiers.alt {
            self.graph_key(&key, window, cx)
        } else {
            false
        };
        if handled {
            cx.stop_propagation();
        }
    }

    fn command_key(
        &mut self,
        key: &str,
        shift: bool,
        input_focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "s" => {
                if shift {
                    self.save_as(window, cx);
                } else {
                    self.save(window, cx);
                }
            }
            "k" => self.open_palette(window, cx),
            "," => self.open_modal(Modal::Settings, window, cx),
            "enter" => {
                if let Some(id) = self
                    .preview_id
                    .clone()
                    .or_else(|| self.editor.selection.first().cloned())
                {
                    self.generate(id, window, cx);
                }
            }
            "v" => return self.paste_images(cx),
            "o" if !input_focused => self.open_dialog(window, cx),
            "n" if !input_focused => self.new_project(window, cx),
            "l" if !input_focused => self.tidy(cx),
            "a" if !input_focused => {
                self.editor.selection = self.editor.project.graph.nodes.keys().cloned().collect();
                self.refresh(cx);
            }
            "z" if !input_focused => {
                let changed = if shift {
                    self.editor.redo()
                } else {
                    self.editor.undo()
                };
                if changed {
                    self.finish_history_change(cx);
                }
            }
            "y" if !input_focused => {
                if self.editor.redo() {
                    self.finish_history_change(cx);
                }
            }
            _ => return false,
        }
        true
    }

    fn finish_history_change(&mut self, cx: &mut Context<Self>) {
        if self
            .preview_id
            .as_ref()
            .is_some_and(|id| !self.editor.project.graph.nodes.contains_key(id))
        {
            self.preview_id = None;
            self.editing = false;
        }
        self.refresh(cx);
        self.save_current(cx);
    }

    fn graph_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if matches!(key, "backspace" | "delete") {
            self.delete_selected(cx);
            return true;
        }
        let Some(id) = self.editor.selection.first().cloned() else {
            return false;
        };
        match key {
            "space" | " " | "p" => {
                self.toggle_preview(id, window, cx);
            }
            "e" => {
                let id = self.preview_id.clone().unwrap_or(id);
                self.preview(id, true, window, cx);
            }
            "enter"
                if self
                    .editor
                    .project
                    .graph
                    .nodes
                    .get(&id)
                    .is_some_and(|node| node.role() == Role::Assistant) =>
            {
                self.reply(id, window, cx)
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn toggle_preview(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_id.as_deref() == Some(&id) {
            self.preview_id = None;
            self.editing = false;
            self.dismiss_mentions();
            self.focus.focus(window);
            cx.notify();
        } else {
            self.preview(id, false, window, cx);
        }
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.editing {
            self.editing = false;
        } else if self.preview_id.is_some() {
            self.preview_id = None;
        } else {
            return false;
        }
        self.dismiss_mentions();
        self.focus.focus(window);
        self.refresh(cx);
        true
    }

    fn mention_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.mentions.is_empty() && key != "escape" {
            return false;
        }
        match key {
            "down" => {
                self.mention_index =
                    (self.mention_index + 1).min(self.mentions.len().saturating_sub(1))
            }
            "up" => self.mention_index = self.mention_index.saturating_sub(1),
            "enter" | "tab" => self.accept_mention(window, cx),
            "escape" => self.dismiss_mentions(),
            _ => return false,
        }
        self.mention_state.scroll.scroll_to_item(self.mention_index);
        cx.notify();
        true
    }
}

fn separator() -> impl IntoElement {
    div().mx_1().h_5().w(px(1.)).bg(theme::border())
}

impl Workspace {
    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some() || !self.permissions.is_empty() {
            return;
        }
        if self.palette.is_some() {
            self.close_palette(window, cx);
            return;
        }
        let previous_focus = if self.editing {
            Some(self.focus.clone())
        } else {
            window.focused(cx)
        };
        self.editing = false;
        self.dismiss_mentions();
        self.palette = Some(PaletteState {
            corpus: self.editor.project.graph.nodes.values().cloned().collect(),
            hits: Vec::new(),
            index: 0,
            rendered_hits: Vec::new(),
            total: 0,
            scroll: ScrollHandle::new(),
            previous_focus,
        });
        self.palette_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.search_palette(cx);
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self
            .palette
            .take()
            .and_then(|palette| palette.previous_focus);
        previous.unwrap_or_else(|| self.focus.clone()).focus(window);
        cx.notify();
    }

    pub(crate) fn search_palette(&mut self, cx: &mut Context<Self>) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        let query = self.palette_input.read(cx).value();
        let result = search_nodes(&palette.corpus, &query, MAX_VISIBLE_HITS);
        palette.hits = result.hits.iter().map(|hit| hit.node.id.clone()).collect();
        palette.rendered_hits = result.hits;
        palette.total = result.total;
        palette.index = 0;
        palette.scroll.scroll_to_item(0);
        cx.notify();
    }

    fn palette_key(
        &mut self,
        key: &str,
        command: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if key == "escape" || (key == "k" && command) {
            self.close_palette(window, cx);
            return true;
        }
        let Some(palette) = &mut self.palette else {
            return false;
        };
        match key {
            "down" => palette.index = (palette.index + 1).min(palette.hits.len().saturating_sub(1)),
            "up" => palette.index = palette.index.saturating_sub(1),
            "enter" => {
                let id = palette.hits.get(palette.index).cloned();
                if let Some(id) = id {
                    self.jump_to_hit(id, command, window, cx);
                }
                return true;
            }
            "tab" => return true,
            _ => return false,
        }
        palette.scroll.scroll_to_item(palette.index);
        cx.notify();
        true
    }

    fn jump_to_hit(
        &mut self,
        id: String,
        preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_palette(window, cx);
        if !self.editor.project.graph.nodes.contains_key(&id) {
            return;
        }
        self.editor.selection.clear();
        self.editor.selection.insert(id.clone());
        self.canvas.update(cx, |canvas, cx| canvas.jump(&id, cx));
        self.focus.focus(window);
        if preview {
            self.preview(id, false, window, cx);
        }
        self.refresh(cx);
    }

    pub(crate) fn render_palette(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.modal.is_some() || !self.permissions.is_empty() {
            self.close_palette(window, cx);
            return Empty.into_any_element();
        }
        let Some(palette) = &self.palette else {
            return Empty.into_any_element();
        };
        let query = self.palette_input.read(cx).value();
        let mut results = div()
            .id("palette-results")
            .max_h(px(450.))
            .overflow_y_scroll()
            .track_scroll(&palette.scroll);
        for (index, hit) in palette.rendered_hits.iter().enumerate() {
            let id = hit.node.id.clone();
            let role = match hit.node.role() {
                Role::User => "User",
                Role::Assistant => match hit.node.assistant_data().and_then(|data| data.provider) {
                    Some(GraphProvider::ClaudeCode) => "Claude",
                    Some(GraphProvider::Codex) => "Codex",
                    Some(GraphProvider::GeminiCli) => "Retired provider",
                    None => "Assistant",
                },
                Role::File => "File",
            };
            let row = div()
                .id(SharedString::from(format!("search-{id}")))
                .flex()
                .items_center()
                .gap_4()
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(theme::border())
                .cursor_pointer()
                .when(index == palette.index, |row| row.bg(theme::raised()))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if !*hovered {
                        return;
                    }
                    let Some(palette) = &mut this.palette else {
                        return;
                    };
                    palette.index = index;
                    cx.notify();
                }))
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    this.jump_to_hit(
                        id.clone(),
                        event.modifiers().platform || event.modifiers().control,
                        window,
                        cx,
                    )
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().truncate().child(highlighted(hit.title.clone())))
                        .when_some(hit.snippet.clone(), |row, snippet| {
                            row.child(
                                div()
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .truncate()
                                    .child(highlighted(snippet)),
                            )
                        }),
                )
                .child(div().text_xs().text_color(theme::muted()).child(role));
            results = results.child(row);
        }
        if palette.hits.is_empty() {
            results = results.child(div().p_6().text_color(theme::muted()).child(
                if query.is_empty() {
                    "No nodes yet"
                } else {
                    "No matching nodes"
                },
            ));
        }
        let count = if palette.total > palette.hits.len() {
            format!("{} of {} matches", palette.hits.len(), palette.total)
        } else {
            format!("{} matches", palette.total)
        };
        div()
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(75.))
            .bg(rgba(0x00000088))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
            )
            .child(
                div()
                    .w(px(650.))
                    .bg(theme::surface())
                    .border_1()
                    .border_color(theme::border())
                    .rounded_lg()
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .px_4()
                            .py_3()
                            .border_b_1()
                            .border_color(theme::border())
                            .child(Input::new(&self.palette_input)),
                    )
                    .child(results)
                    .child(
                        div()
                            .px_4()
                            .py_3()
                            .flex()
                            .justify_between()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(count)
                            .child("↑↓ Navigate    Enter Jump    ⌘ Enter Preview    Esc Close"),
                    ),
            )
            .into_any_element()
    }
}

fn highlighted(value: HighlightedText) -> StyledText {
    StyledText::new(value.text).with_highlights(value.spans.into_iter().map(|span| {
        (
            span.start..span.end,
            HighlightStyle {
                color: Some(theme::accent()),
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            },
        )
    }))
}

fn toolbar_button(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    enabled: bool,
    cx: &Context<Workspace>,
    action: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(|| id.to_owned())
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_2()
        .rounded_md()
        .text_xs()
        .text_color(if enabled {
            theme::text()
        } else {
            theme::muted()
        })
        .when(!enabled, |button| button.opacity(0.45))
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(theme::raised()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !matches!(this.modal, Some(Modal::ChangingVault)) {
                        action(this, window, cx);
                    }
                }))
        })
        .child(Icon::new(icon).size_3p5())
        .child(label)
}

#[cfg(test)]
mod status_tests {
    use chrono::{Local, TimeZone};

    #[test]
    fn saved_status_includes_the_local_completion_time() {
        let time = Local
            .with_ymd_and_hms(2026, 10, 3, 14, 5, 9)
            .single()
            .unwrap();
        assert_eq!(
            super::saved_status(time.timestamp_millis() as f64),
            "Saved at 14:05:09"
        );
    }
}
