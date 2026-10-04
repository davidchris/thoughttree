use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    time::Duration,
};

use gpui::{prelude::*, *};
use gpui_component::{
    input::{InputEvent, InputState, TextareaState},
    RopeExt,
};
use thoughttree_desktop::{
    AgentProvider, Desktop, DesktopEvent, Message, MessageFile, MessageImage, ModelInfo,
    PermissionRequestEvent, PromptRequest, ProviderStatus, ReasoningEffort as ProviderEffort,
    Revision,
};
use thoughttree_gpui_model::{
    now_ms, Editor, GraphNode, GraphProvider, NodeKind, Position, Project, ReasoningEffort, Role,
};

use crate::{
    canvas::{GraphCanvas, GraphEvent},
    dialogs::{DialogState, Modal},
    rich_text::RichText,
    theme,
};

pub struct Workspace {
    pub(crate) desktop: Desktop,
    pub(crate) editor: Editor,
    pub(crate) project_path: Option<PathBuf>,
    pub(crate) revision: Option<Revision>,
    pub(crate) title: String,
    pub(crate) notice: Option<String>,
    pub(crate) persistence_error: Option<String>,
    pub(crate) models: BTreeMap<String, Vec<ModelInfo>>,
    pub(crate) discovering_models: BTreeSet<String>,
    pub(crate) providers: Vec<ProviderStatus>,
    pub(crate) modal: Option<Modal>,
    pub(crate) dialogs: DialogState,
    pub(crate) input: Entity<TextareaState>,
    pub(crate) focus: FocusHandle,
    pub(crate) canvas: Entity<GraphCanvas>,
    pub(crate) rich: Entity<RichText>,
    pub(crate) preview_id: Option<String>,
    pub(crate) selected_edge: Option<String>,
    pub(crate) editing: bool,
    pub(crate) provider: AgentProvider,
    pub(crate) selected_model: Option<String>,
    pub(crate) panel_width: f32,
    pub(crate) resizing: bool,
    pub(crate) permissions: VecDeque<PermissionRequestEvent>,
    pub(crate) activity_expanded: bool,
    pub(crate) raw_expanded: BTreeSet<String>,
    pub(crate) palette: Option<PaletteState>,
    pub(crate) palette_input: Entity<InputState>,
    pub(crate) file_preview:
        Option<Result<thoughttree_core::vault::files::FilePreviewResponse, String>>,
    pub(crate) file_status: BTreeMap<String, Result<thoughttree_desktop::VaultFileStatus, String>>,
    pub(crate) file_previews: BTreeMap<String, thoughttree_core::vault::files::FilePreviewResponse>,
    pub(crate) file_preview_errors: BTreeMap<String, String>,
    pub(crate) file_refresh: Option<Task<()>>,
    pub(crate) mentions: Vec<String>,
    pub(crate) mention_index: usize,
    pub(crate) mention_state: crate::files::MentionState,
    updating_input: bool,
    pub(crate) generation: u64,
    pub(crate) save_in_flight: bool,
    pub(crate) save_queued: bool,
    pub(crate) autosave: Option<Task<()>>,
    pub(crate) summaries: BTreeMap<String, (u64, String, String)>,
    summary_task: Option<Task<()>>,
    stream_checkpoint: Option<Task<()>>,
    pending_chunks: BTreeMap<(String, String), String>,
    stream_flush: Option<Task<()>>,
    pub(crate) saved_at: Option<f64>,
    pub(crate) transition_request: u64,
    pub(crate) snapshot_revision: Option<u64>,
    pub(crate) closing: bool,
    _subscriptions: Vec<Subscription>,
}

pub(crate) struct PaletteState {
    pub corpus: Vec<GraphNode>,
    pub hits: Vec<String>,
    pub index: usize,
    pub rendered_hits: Vec<thoughttree_gpui_model::SearchHit>,
    pub total: usize,
    pub scroll: ScrollHandle,
    pub previous_focus: Option<FocusHandle>,
}

impl Workspace {
    pub fn new(
        desktop: Desktop,
        events: async_channel::Receiver<DesktopEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let canvas = cx.new(|_| GraphCanvas::new());
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(12, 100)
                .placeholder("Enter your message… (@ to mention files, paste or drop images)")
        });
        let palette_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search nodes…"));
        let rich = cx.new(|cx| RichText::new("", cx));
        let canvas_sub = cx.subscribe_in(&canvas, window, |this, _, event, window, cx| {
            this.graph_event(event.clone(), window, cx)
        });
        let input_sub = cx.subscribe_in(&input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) && !this.updating_input {
                this.input_changed(cx);
            }
        });
        let palette_sub = cx.subscribe(&palette_input, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.search_palette(cx);
            }
        });
        let file_activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.refresh_files(cx);
            }
        });
        let shortcuts = Self::register_shortcuts(window, cx);
        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if this
                    .update(cx, |this, cx| this.desktop_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let config = desktop.config();
        let providers = desktop.available_providers();
        let modal = Some(if config.notes_directory.is_some() {
            Modal::Projects
        } else {
            Modal::Setup
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut result = Self {
            desktop,
            editor: Editor::default(),
            project_path: None,
            revision: None,
            title: "Untitled".into(),
            notice: None,
            persistence_error: None,
            models: BTreeMap::new(),
            discovering_models: BTreeSet::new(),
            providers,
            modal,
            dialogs: DialogState::new(window, cx),
            input,
            focus,
            canvas,
            rich,
            preview_id: None,
            selected_edge: None,
            editing: false,
            provider: config.default_provider,
            selected_model: None,
            panel_width: 520.,
            resizing: false,
            permissions: VecDeque::new(),
            activity_expanded: false,
            raw_expanded: BTreeSet::new(),
            palette: None,
            palette_input,
            file_preview: None,
            file_status: BTreeMap::new(),
            file_previews: BTreeMap::new(),
            file_preview_errors: BTreeMap::new(),
            file_refresh: None,
            mentions: Vec::new(),
            mention_index: 0,
            mention_state: Default::default(),
            updating_input: false,
            generation: 0,
            save_in_flight: false,
            save_queued: false,
            autosave: None,
            summaries: BTreeMap::new(),
            summary_task: None,
            stream_checkpoint: None,
            pending_chunks: BTreeMap::new(),
            stream_flush: None,
            saved_at: None,
            transition_request: 0,
            snapshot_revision: None,
            closing: false,
            _subscriptions: vec![
                canvas_sub,
                input_sub,
                palette_sub,
                file_activation,
                shortcuts,
            ],
        };
        if let Some(modal) = result.modal.clone() {
            result.open_modal(modal, window, cx);
        }
        result.refresh(cx);
        result
    }

    pub(crate) fn change(&mut self, edit: impl FnOnce(&mut Project), cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        if let Err(error) = self.editor.edit(|project| {
            edit(project);
            Ok(())
        }) {
            self.notice = Some(error.to_string());
        }
        self.refresh(cx);
        self.schedule_summaries(cx);
        self.checkpoint_stream(cx);
        let revision = self.editor.edit_revision;
        self.autosave = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(900))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.editor.edit_revision == revision {
                    this.save_current(cx);
                }
            });
        }));
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        let graph = &self.editor.project.graph;
        let nodes = graph
            .nodes
            .values()
            .map(|node| {
                (
                    node.clone(),
                    graph.layout.get(&node.id).copied().unwrap_or_default(),
                )
            })
            .collect();
        let blocked = graph
            .nodes
            .values()
            .filter(|node| self.editor.is_node_blocked(&node.id))
            .map(|node| node.id.clone())
            .collect();
        let generation_blocked = graph
            .nodes
            .values()
            .filter(|node| node.role() == Role::User && self.send_blocker(&node.id).is_some())
            .map(|node| node.id.clone())
            .collect();
        self.canvas.update(cx, |canvas, cx| {
            canvas.set_generation_blocked(generation_blocked, cx);
            canvas.set_graph(
                nodes,
                graph.edges.clone(),
                self.editor.selection.iter().cloned().collect(),
                self.editor.active_turns.keys().cloned().collect(),
                blocked,
                cx,
            )
        });
        if let Some(node) = self
            .preview_id
            .as_ref()
            .filter(|id| !self.editor.active_turns.contains_key(*id))
            .and_then(|id| graph.nodes.get(id))
        {
            self.rich
                .update(cx, |rich, cx| rich.set_text(node.content.clone(), cx));
        }
        cx.notify();
    }

    fn graph_event(&mut self, event: GraphEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            GraphEvent::Select(ids) => {
                if ids.is_empty() {
                    self.preview_id = None;
                    self.editing = false;
                    self.dismiss_mentions();
                }
                self.selected_edge = None;
                self.editor.selection = ids.into_iter().collect();
                self.focus.focus(window, cx);
                self.refresh(cx);
            }
            GraphEvent::SelectEdge(id) => {
                self.selected_edge = Some(id);
                self.editor.selection.clear();
                self.focus.focus(window, cx);
                self.refresh(cx);
            }
            GraphEvent::EdgeContext(id) => self.open_modal(Modal::Edge { id }, window, cx),
            GraphEvent::Preview(id, edit) => self.preview(id, edit, window, cx),
            GraphEvent::TogglePreview(id) => self.toggle_preview(id, window, cx),
            GraphEvent::Move(positions) => self.change(
                |project| {
                    for (id, p) in positions {
                        let _ = project.graph.set_position(&id, p);
                    }
                },
                cx,
            ),
            GraphEvent::Connect(mut source, mut target) => {
                if self
                    .editor
                    .project
                    .graph
                    .nodes
                    .get(&source)
                    .is_some_and(|node| node.role() == Role::User)
                    && self
                        .editor
                        .project
                        .graph
                        .nodes
                        .get(&target)
                        .is_some_and(|node| node.role() == Role::Assistant)
                {
                    std::mem::swap(&mut source, &mut target);
                }
                if self.editor.is_node_blocked(&source) || self.editor.is_node_blocked(&target) {
                    return;
                }
                let mut error = None;
                self.change(
                    |project| {
                        if let Err(e) = project.graph.add_edge(&source, &target) {
                            error = Some(e.to_string());
                        }
                    },
                    cx,
                );
                self.notice = error;
            }
            GraphEvent::New(position, parent) => {
                self.create_user(position, parent.into_iter().collect(), window, cx)
            }
            GraphEvent::Generate(id) => self.generate_from(id, false, window, cx),
            GraphEvent::Reply(id) => self.reply(id, window, cx),
            GraphEvent::Context(node, position) => {
                self.open_modal(Modal::Context { node, position }, window, cx)
            }
            GraphEvent::DropFiles(paths, position) => self.add_files(paths, position, cx),
            GraphEvent::DropImages(paths, id) => self.attach_images_to(id, paths, cx),
        }
    }

    pub(crate) fn create_user(
        &mut self,
        position: Position,
        parents: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if parents.iter().any(|id| self.editor.is_node_blocked(id)) {
            return;
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.change(
            |project| {
                project
                    .graph
                    .add_node(GraphNode::user(&id, "", now_ms()), position);
                for parent in parents {
                    let _ = project.graph.add_edge(&parent, &id);
                }
            },
            cx,
        );
        self.editor.selection.clear();
        self.editor.selection.insert(id.clone());
        self.preview(id, true, window, cx);
    }

    pub(crate) fn reply(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_node_blocked(&id) {
            return;
        }
        let p = self
            .editor
            .project
            .graph
            .layout
            .get(&id)
            .copied()
            .unwrap_or_default();
        let siblings = self.editor.project.graph.children(&id).len();
        self.create_user(
            Position {
                x: p.x + siblings as f64 * 220.,
                y: p.y + 200.,
            },
            vec![id],
            window,
            cx,
        );
    }

    pub(crate) fn preview(
        &mut self,
        id: String,
        edit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.editor.project.graph.nodes.get(&id).cloned() else {
            return;
        };
        self.dismiss_mentions();
        self.preview_id = Some(id.clone());
        self.editing = edit && node.role() == Role::User && !self.editor.is_node_blocked(&id);
        self.activity_expanded = false;
        self.raw_expanded.clear();
        self.file_preview = None;
        self.updating_input = true;
        self.input.update(cx, |input, cx| {
            input.set_value(node.content.clone(), window, cx);
            if self.editing {
                let end = input.text().offset_to_position(input.text().len());
                input.set_cursor_position(end, window, cx);
            }
        });
        self.updating_input = false;
        self.rich
            .update(cx, |rich, cx| rich.set_text(node.content.clone(), cx));
        if node.role() == Role::File {
            self.load_file_preview(id, cx);
        }
        self.ensure_models(cx);
        self.refresh(cx);
    }

    fn input_changed(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.preview_id.clone() else {
            return;
        };
        if !self.editing || self.editor.is_node_blocked(&id) {
            return;
        }
        let content = self.input.read(cx).value().to_string();
        self.change(
            |project| {
                let _ = project.graph.set_content(&id, content, now_ms());
            },
            cx,
        );
        self.update_mentions(cx);
    }

    pub(crate) fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_edge.take() {
            let allowed = self
                .editor
                .project
                .graph
                .edges
                .iter()
                .find(|e| e.id == id)
                .is_some_and(|edge| {
                    !self.editor.is_node_blocked(&edge.source)
                        && !self.editor.is_node_blocked(&edge.target)
                });
            if allowed {
                self.change(
                    |project| {
                        project.graph.remove_edge(&id);
                    },
                    cx,
                );
            }
            return;
        }
        let ids: Vec<_> = self
            .editor
            .selection
            .iter()
            .filter(|id| !self.editor.is_node_blocked(id))
            .cloned()
            .collect();
        if ids.is_empty() {
            return;
        }
        self.change(
            |project| {
                for id in &ids {
                    project.graph.remove_node(id);
                }
            },
            cx,
        );
        self.editor.selection.clear();
        for id in &ids {
            self.file_status.remove(id);
            self.file_previews.remove(id);
            self.file_preview_errors.remove(id);
        }
        if self.preview_id.as_ref().is_some_and(|id| ids.contains(id)) {
            self.file_preview = None;
            self.dismiss_mentions();
            self.preview_id = None;
            self.editing = false;
        }
        self.refresh(cx);
    }

    pub(crate) fn generate(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.generate_from(id, true, window, cx);
    }

    fn generate_from(
        &mut self,
        id: String,
        open_preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        if let Some(reason) = self.send_blocker(&id) {
            self.notice = Some(reason);
            cx.notify();
            return;
        }
        if !self.editor.project.graph.can_generate(&id) {
            return;
        }
        let messages = self.editor.project.graph.conversation_path(&id);
        let messages: Vec<_> = messages
            .into_iter()
            .map(|m| Message {
                role: m.role.as_str().into(),
                content: m.content,
                images: Some(
                    m.images
                        .into_iter()
                        .map(|i| MessageImage {
                            data: i.data,
                            mime_type: i.mime_type,
                        })
                        .collect(),
                ),
                files: Some(
                    m.files
                        .into_iter()
                        .map(|f| MessageFile {
                            path: f.path,
                            name: f.name,
                            mime_type: f.mime_type,
                            size: f.size,
                        })
                        .collect(),
                ),
            })
            .collect();
        let assistant_id = uuid::Uuid::new_v4().to_string();
        let turn_id = uuid::Uuid::new_v4().to_string();
        let provider = self.provider.clone();
        let model_id = self.selected_model.clone().or_else(|| {
            self.editor
                .project
                .project_model_preferences
                .as_ref()
                .and_then(|p| p.get(provider.descriptor().id))
                .cloned()
        });
        let effort = self
            .editor
            .project
            .project_effort_preferences
            .as_ref()
            .and_then(|p| p.get(provider.descriptor().id))
            .map(|effort| match effort {
                ReasoningEffort::Low => ProviderEffort::Low,
                ReasoningEffort::Medium => ProviderEffort::Medium,
                ReasoningEffort::High => ProviderEffort::High,
                ReasoningEffort::XHigh => ProviderEffort::XHigh,
            });
        let mut node = GraphNode::assistant(&assistant_id, "", now_ms());
        if let NodeKind::Assistant(data) = &mut node.kind {
            data.provider = Some(match provider {
                AgentProvider::ClaudeCode => GraphProvider::ClaudeCode,
                AgentProvider::Codex => GraphProvider::Codex,
            });
            data.model = model_id.clone();
        }
        let p = self
            .editor
            .project
            .graph
            .layout
            .get(&id)
            .copied()
            .unwrap_or_default();
        let siblings = self.editor.project.graph.children(&id).len();
        self.change(
            |project| {
                project.graph.add_node(
                    node,
                    Position {
                        x: p.x + siblings as f64 * 220.,
                        y: p.y + 200.,
                    },
                );
                let _ = project.graph.add_edge(&id, &assistant_id);
            },
            cx,
        );
        if let Err(error) = self.editor.start_turn(&assistant_id, &turn_id) {
            self.notice = Some(error.to_string());
            return;
        }
        let request = PromptRequest {
            node_id: assistant_id.clone(),
            turn_id: turn_id.clone(),
            messages,
            provider: Some(provider),
            model_id,
            effort,
        };
        if let Err(error) = self.desktop.start_prompt(request) {
            self.editor.fail_turn(&assistant_id, &turn_id, &error);
            self.notice = Some(error);
            self.save_current(cx);
        }
        if open_preview {
            self.editing = false;
            self.preview(assistant_id, false, window, cx);
        }
        self.checkpoint_stream(cx);
        self.refresh(cx);
    }

    fn desktop_event(&mut self, event: DesktopEvent, cx: &mut Context<Self>) {
        match event {
            DesktopEvent::StreamChunk(event) => {
                if self.editor.active_turns.get(&event.node_id) == Some(&event.turn_id) {
                    self.pending_chunks
                        .entry((event.node_id, event.turn_id))
                        .or_default()
                        .push_str(&event.chunk);
                    self.schedule_stream_flush(cx);
                }
                return;
            }
            DesktopEvent::TurnProvenance(event) => {
                if let Ok(value) = serde_json::to_value(event.provenance) {
                    self.editor
                        .set_turn_provenance(&event.node_id, &event.turn_id, &value);
                }
            }
            DesktopEvent::PromptFinished {
                node_id,
                turn_id,
                result,
            } => {
                self.flush_streams(cx);
                if self.editor.active_turns.get(&node_id) != Some(&turn_id) {
                    return;
                }
                if let Err(error) = result {
                    self.editor.fail_turn(&node_id, &turn_id, &error);
                    self.notice = Some(error);
                } else {
                    self.editor.finish_turn(&node_id, &turn_id);
                }
                self.permissions
                    .retain(|request| request.node_id != node_id);
                self.save_current(cx);
                self.schedule_summaries(cx);
            }
            DesktopEvent::PermissionRequest(event) => self.permissions.push_back(event),
            DesktopEvent::PermissionResponseFailed { error, .. } => self.notice = Some(error),
            DesktopEvent::SummaryFinished { node_id, result } => {
                self.complete_summary(node_id, result, cx)
            }
            DesktopEvent::ModelsDiscovered { provider, result } => {
                self.discovering_models.remove(provider.descriptor().id);
                match result {
                    Ok(models) => {
                        self.models.insert(provider.descriptor().id.into(), models);
                    }
                    Err(error) => self.notice = Some(error),
                }
            }
            DesktopEvent::ProviderPathValidated { result, .. } => {
                self.refresh_provider_statuses(cx);
                self.notice = Some(match result {
                    Ok(path) => format!("Provider found: {path}"),
                    Err(error) => error,
                });
            }
        }
        self.refresh(cx);
    }

    pub(crate) fn reset_transient_jobs(&mut self) {
        self.pending_chunks.clear();
        self.stream_flush = None;
        self.stream_checkpoint = None;
        self.summary_task = None;
    }

    fn schedule_stream_flush(&mut self, cx: &mut Context<Self>) {
        if self.stream_flush.is_some() {
            return;
        }
        self.stream_flush = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.stream_flush = None;
                this.flush_streams(cx);
            });
        }));
    }

    fn flush_streams(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for ((node, turn), chunk) in std::mem::take(&mut self.pending_chunks) {
            changed |= self.editor.append_turn(&node, &turn, &chunk);
        }
        if changed {
            self.checkpoint_stream(cx);
            self.refresh(cx);
        }
    }

    pub(crate) fn ensure_models(&mut self, cx: &mut Context<Self>) {
        let id = self.provider.descriptor().id;
        if self.models.contains_key(id) || self.discovering_models.contains(id) {
            return;
        }
        match self.desktop.discover_models(self.provider.clone()) {
            Ok(()) => {
                self.discovering_models.insert(id.into());
            }
            Err(error) => self.notice = Some(error),
        }
        cx.notify();
    }

    /// Saves ongoing edits and streams even when the idle debounce never fires.
    fn checkpoint_stream(&mut self, cx: &mut Context<Self>) {
        if self.stream_checkpoint.is_some() {
            return;
        }
        let generation = self.generation;
        self.stream_checkpoint = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |this, cx| {
                this.stream_checkpoint = None;
                if this.generation == generation {
                    this.save_current(cx);
                }
            });
        }));
    }

    pub(crate) fn schedule_summaries(&mut self, cx: &mut Context<Self>) {
        self.summary_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            let _ = this.update(cx, |this, cx| this.queue_summaries(cx));
        }));
    }

    fn queue_summaries(&mut self, cx: &mut Context<Self>) {
        // ACP summaries are serialized, including jobs from a previous Project.
        if !self.summaries.is_empty() {
            return;
        }
        let mut short_changed = false;
        for node in self.editor.project.graph.nodes.values_mut() {
            if node.role() == Role::File
                || self.editor.active_turns.contains_key(&node.id)
                || node.content.trim().is_empty()
            {
                continue;
            }
            if node.content.encode_utf16().count() <= 100
                && node.summary.as_ref() != Some(&node.content)
            {
                node.summary = Some(node.content.clone());
                node.summary_timestamp = Some(now_ms());
                short_changed = true;
            }
        }
        if short_changed {
            self.editor.touch();
            self.save_current(cx);
            self.refresh(cx);
        }
        let pending = self
            .editor
            .project
            .graph
            .nodes
            .values()
            .find(|node| needs_summary(node) && !self.editor.active_turns.contains_key(&node.id))
            .map(|node| (node.id.clone(), node.content.clone()));
        let Some((id, content)) = pending else { return };
        let request = uuid::Uuid::new_v4().to_string();
        self.summaries
            .insert(request.clone(), (self.generation, id, content.clone()));
        if let Err(error) = self.desktop.generate_summary(request.clone(), content) {
            self.complete_summary(request, Err(error), cx);
        }
    }

    fn complete_summary(
        &mut self,
        request: String,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        let Some((generation, id, content)) = self.summaries.remove(&request) else {
            return;
        };
        if generation == self.generation {
            if let Some(node) = self
                .editor
                .project
                .graph
                .nodes
                .get_mut(&id)
                .filter(|node| node.content == content)
            {
                node.summary = Some(result.unwrap_or_else(|_| fallback_summary(&content)));
                node.summary_timestamp = Some(now_ms());
                self.editor.touch();
                self.save_current(cx);
            }
        }
        self.schedule_summaries(cx);
    }

    pub(crate) fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        self.browse_project(window, cx);
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.menu_actions(div(), cx)
            .id("thoughttree")
            .key_context("ThoughtTree")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .text_color(theme::text())
            .font_family(".SystemUIFont")
            .text_sm()
            .child(self.toolbar(cx))
            .when_some(self.notice.clone(), |d, notice| {
                d.child(
                    div()
                        .flex()
                        .justify_between()
                        .px_4()
                        .py_2()
                        .bg(theme::raised())
                        .text_color(theme::danger())
                        .child(notice)
                        .child(
                            div()
                                .id("dismiss-notice")
                                .cursor_pointer()
                                .child("Dismiss")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.notice = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(div().flex_1().min_w_0().child(self.canvas.clone()))
                    .when(self.preview_id.is_some(), |d| {
                        d.child(self.panel(window, cx))
                    }),
            )
            .when(self.modal.is_some(), |d| {
                d.children(self.render_modal(window, cx))
            })
            .when(self.palette.is_some(), |d| {
                d.child(self.render_palette(window, cx))
            })
            .when(!self.permissions.is_empty(), |d| {
                d.child(self.render_permission(cx))
            })
    }
}

fn fallback_summary(content: &str) -> String {
    // Match the existing 50 UTF-16-unit limit without splitting a Unicode scalar.
    let mut units = 0;
    let prefix: String = content
        .chars()
        .take_while(|character| {
            units += character.len_utf16();
            units <= 50
        })
        .collect();
    format!("{prefix}...")
}

fn needs_summary(node: &GraphNode) -> bool {
    node.role() != Role::File
        && node.content.encode_utf16().count() > 100
        && !node.content.trim().is_empty()
        && (node
            .summary
            .as_ref()
            .is_none_or(|summary| summary.is_empty())
            || node
                .summary_timestamp
                .is_none_or(|at| at < node.content_updated_at.unwrap_or(node.timestamp)))
}

#[cfg(test)]
mod tests {
    use super::{fallback_summary, needs_summary, now_ms, DesktopEvent};
    use crate::interaction_tests::workspace;
    use gpui::TestAppContext;
    use thoughttree_core::events::StreamChunkEvent;

    #[test]
    fn summary_threshold_matches_javascript_string_length() {
        let mut node =
            thoughttree_gpui_model::GraphNode::assistant("emoji", "🦀".repeat(50), now_ms());
        assert!(!needs_summary(&node));
        node.content.push('🦀');
        assert!(needs_summary(&node));
        assert_eq!(
            fallback_summary(&node.content),
            format!("{}...", "🦀".repeat(25))
        );
        assert_eq!(
            fallback_summary(&format!("{}🦀", "a".repeat(49))),
            format!("{}...", "a".repeat(49))
        );
    }

    #[gpui::test]
    fn summaries_keep_content_and_project_identity_and_fallback(cx: &mut TestAppContext) {
        let (_directory, workspace, cx, _) = workspace(cx);
        workspace.update(cx, |this, cx| {
            let content = "A long response with Unicode 🦀. ".repeat(8);
            this.editor
                .project
                .graph
                .set_content("answer", content.clone(), now_ms())
                .unwrap();
            this.summaries.insert(
                "failed".into(),
                (this.generation, "answer".into(), content.clone()),
            );
            this.complete_summary("failed".into(), Err("offline".into()), cx);
            let answer = &this.editor.project.graph.nodes["answer"];
            assert_eq!(
                answer.summary,
                Some("A long response with Unicode 🦀. A long response w...".into())
            );
            assert!(!needs_summary(answer));

            this.summaries.insert(
                "stale-content".into(),
                (this.generation, "answer".into(), content.clone()),
            );
            this.editor
                .project
                .graph
                .set_content("answer", format!("{content} changed"), now_ms())
                .unwrap();
            this.complete_summary("stale-content".into(), Ok("wrong".into()), cx);
            assert!(needs_summary(&this.editor.project.graph.nodes["answer"]));

            this.summaries.insert(
                "stale-project".into(),
                (
                    this.generation.wrapping_sub(1),
                    "answer".into(),
                    format!("{content} changed"),
                ),
            );
            this.complete_summary("stale-project".into(), Ok("wrong project".into()), cx);
            assert_ne!(
                this.editor.project.graph.nodes["answer"].summary.as_deref(),
                Some("wrong project")
            );
        });
    }

    #[gpui::test]
    fn completion_drains_buffered_text_before_error_and_ignores_late_events(
        cx: &mut TestAppContext,
    ) {
        let (_directory, workspace, cx, _) = workspace(cx);
        workspace.update(cx, |this, cx| {
            this.editor.start_turn("answer", "turn").unwrap();
            this.desktop_event(
                DesktopEvent::StreamChunk(StreamChunkEvent {
                    node_id: "answer".into(),
                    turn_id: "turn".into(),
                    chunk: " final chunk".into(),
                }),
                cx,
            );
            assert!(!this.editor.project.graph.nodes["answer"]
                .content
                .ends_with("final chunk"));
            this.desktop_event(
                DesktopEvent::PromptFinished {
                    node_id: "answer".into(),
                    turn_id: "turn".into(),
                    result: Err("fixture failure".into()),
                },
                cx,
            );
            let content = this.editor.project.graph.nodes["answer"].content.clone();
            assert!(content.ends_with("final chunk\n\n[Error: fixture failure]"));
            assert!(this.editor.active_turns.is_empty());
            this.desktop_event(
                DesktopEvent::StreamChunk(StreamChunkEvent {
                    node_id: "answer".into(),
                    turn_id: "turn".into(),
                    chunk: " stale".into(),
                }),
                cx,
            );
            this.flush_streams(cx);
            assert_eq!(this.editor.project.graph.nodes["answer"].content, content);
        });
    }

    #[gpui::test]
    fn short_summaries_are_local_and_pending_jobs_serialize_long_summaries(
        cx: &mut TestAppContext,
    ) {
        let (_directory, workspace, cx, _) = workspace(cx);
        workspace.update(cx, |this, cx| {
            this.queue_summaries(cx);
            assert_eq!(
                this.editor.project.graph.nodes["question"]
                    .summary
                    .as_deref(),
                Some("A question about Rust")
            );
            let content = "Long answer. ".repeat(30);
            this.editor
                .project
                .graph
                .set_content("answer", content.clone(), now_ms())
                .unwrap();
            this.summaries.insert(
                "running".into(),
                (this.generation, "sibling".into(), content),
            );
            this.queue_summaries(cx);
            assert_eq!(this.summaries.len(), 1);
            assert!(this.summaries.contains_key("running"));
        });
    }
}
