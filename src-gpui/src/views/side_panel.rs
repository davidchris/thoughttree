//! Side panel — the GPUI equivalent of `src/components/SidePanel/index.tsx`.
//!
//! Shows the selected node's content + the conversation path leading to it.
//! Provides actions to branch a user reply or send the path to a (stubbed)
//! agent that streams a response into a new assistant node.

use crate::graph::{types::NodeRole, GraphModel};
use crate::state::AppState;
use crate::theme;
use crate::views::markdown_view::markdown_view;
use crate::views::text_input::TextInput;
use gpui::{
    div, prelude::*, px, Context, Entity, MouseButton, ParentElement, Render, Styled, Window,
};

pub struct SidePanelView {
    state: Entity<AppState>,
    editor: Option<Entity<TextInput>>,
}

impl SidePanelView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            // If app state cleared editing externally (e.g. node deleted),
            // drop our editor too.
            let editing = this.state.read(cx).editing.clone();
            if editing.is_none() {
                this.editor = None;
            }
            cx.notify();
        })
        .detach();
        Self { state, editor: None }
    }

    fn branch_user_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_id = self.state.update(cx, |state, cx| {
            let parent = state.selected.clone()?;
            let id = state.create_user_downstream(&parent);
            state.selected = Some(id.clone());
            state.editing = Some(id.clone());
            cx.notify();
            Some(id)
        });
        if let Some(id) = new_id {
            self.open_editor_for(&id, window, cx);
        }
    }

    fn send_to_agent(&mut self, cx: &mut Context<Self>) {
        self.commit_editor(cx);
        self.state.update(cx, |state, cx| {
            let Some(parent) = state.selected.clone() else {
                return;
            };
            let id = state.start_agent_reply(&parent, cx);
            state.selected = Some(id);
            cx.notify();
        });
    }

    fn start_edit_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.state.read(cx).selected.clone();
        let Some(id) = id else { return };
        self.state.update(cx, |state, cx| {
            state.editing = Some(id.clone());
            cx.notify();
        });
        self.open_editor_for(&id, window, cx);
    }

    fn open_editor_for(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let initial = self
            .state
            .read(cx)
            .graph
            .nodes
            .get(id)
            .map(|n| n.content.clone())
            .unwrap_or_default();
        let editor = cx.new(|cx| TextInput::new(initial, cx));
        let handle = editor.read(cx).focus_handle.clone();
        window.focus(&handle, cx);
        self.editor = Some(editor);
        cx.notify();
    }

    fn commit_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.take() else { return };
        let new_content = editor.read(cx).text();
        self.state.update(cx, |state, cx| {
            if let Some(id) = state.editing.clone() {
                state.set_content(&id, new_content);
            }
            state.editing = None;
            cx.notify();
        });
    }

    fn cancel_editor(&mut self, cx: &mut Context<Self>) {
        self.editor = None;
        self.state.update(cx, |state, cx| {
            state.editing = None;
            cx.notify();
        });
    }
}

impl Render for SidePanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.state.read(cx);

        let Some(selected_id) = app.selected.clone() else {
            return div()
                .w(px(theme::SIDE_PANEL_WIDTH))
                .h_full()
                .bg(theme::BG_PANEL)
                .border_l_1()
                .border_color(theme::BORDER)
                .p(px(16.0))
                .child(
                    div()
                        .text_color(theme::TEXT_DIM)
                        .text_size(px(13.0))
                        .child("Select a node to view its content."),
                );
        };

        let path = GraphModel::conversation_path(&app.graph, &selected_id);
        let selected_node = app.graph.nodes.get(&selected_id).cloned();
        let selected_role_label = selected_node
            .as_ref()
            .map(|n| match n.role {
                NodeRole::User => "User",
                NodeRole::Assistant => "Assistant",
            })
            .unwrap_or("(missing)");

        let editing = app.editing.as_ref() == Some(&selected_id) && self.editor.is_some();
        let editor_entity = self.editor.clone();

        let mut messages = div().flex().flex_col().gap(px(8.0));
        for msg in path {
            let bg = match msg.role {
                NodeRole::User => theme::BG_NODE_USER,
                NodeRole::Assistant => theme::BG_NODE_ASSISTANT,
            };
            let role = match msg.role {
                NodeRole::User => "user",
                NodeRole::Assistant => "assistant",
            };
            messages = messages.child(
                div()
                    .p(px(10.0))
                    .bg(bg)
                    .rounded(px(6.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::TEXT_DIM)
                            .child(role),
                    )
                    .child(div().pt(px(4.0)).child(markdown_view(&msg.content))),
            );
        }

        div()
            .flex()
            .flex_col()
            .w(px(theme::SIDE_PANEL_WIDTH))
            .h_full()
            .bg(theme::BG_PANEL)
            .border_l_1()
            .border_color(theme::BORDER)
            .child(
                div()
                    .flex_none()
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(theme::BORDER)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::TEXT_DIM)
                            .child(format!("Selected · {selected_role_label}")),
                    )
                    .child(if editing {
                        div()
                            .id("selected-editor")
                            .pt(px(6.0))
                            .max_h(px(240.0))
                            .overflow_y_scroll()
                            .child(editor_entity.clone().unwrap())
                    } else {
                        let content = selected_node
                            .as_ref()
                            .map(|n| n.content.clone())
                            .unwrap_or_default();
                        let body = if content.trim().is_empty() {
                            div()
                                .text_size(px(13.0))
                                .text_color(theme::TEXT_DIM)
                                .child("(empty — click Edit to start typing)")
                                .into_any_element()
                        } else {
                            markdown_view(&content)
                        };
                        div()
                            .id("selected-content")
                            .pt(px(2.0))
                            .max_h(px(240.0))
                            .overflow_y_scroll()
                            .child(body)
                    }),
            )
            .child(
                div()
                    .id("conversation-path")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(12.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::TEXT_DIM)
                            .pb(px(8.0))
                            .child("Conversation path"),
                    )
                    .child(messages),
            )
            .child(if editing {
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .border_t_1()
                    .border_color(theme::BORDER)
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(6.0))
                            .bg(theme::ACCENT)
                            .text_color(gpui::white())
                            .text_size(px(12.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.commit_editor(cx)),
                            )
                            .child("Save"),
                    )
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(6.0))
                            .bg(theme::BG_NODE_USER)
                            .text_color(theme::TEXT)
                            .text_size(px(12.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.cancel_editor(cx)),
                            )
                            .child("Cancel"),
                    )
            } else {
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .border_t_1()
                    .border_color(theme::BORDER)
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(6.0))
                            .bg(theme::BG_NODE_USER)
                            .text_color(theme::TEXT)
                            .text_size(px(12.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.start_edit_selected(window, cx)
                                }),
                            )
                            .child("Edit"),
                    )
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(6.0))
                            .bg(theme::BG_NODE_USER)
                            .text_color(theme::TEXT)
                            .text_size(px(12.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.branch_user_reply(window, cx)
                                }),
                            )
                            .child("Branch reply"),
                    )
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(6.0))
                            .bg(theme::ACCENT)
                            .text_color(gpui::white())
                            .text_size(px(12.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.send_to_agent(cx)),
                            )
                            .child("Send to agent (stub)"),
                    )
            })
    }
}
