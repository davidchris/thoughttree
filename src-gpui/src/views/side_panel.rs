//! Side panel — the GPUI equivalent of `src/components/SidePanel/index.tsx`.
//!
//! Shows the selected node's content + the conversation path leading to it.
//! Provides actions to branch a user reply or send the path to a (stubbed)
//! agent that streams a response into a new assistant node.

use crate::graph::{model::ConversationMessage, types::NodeRole, GraphModel};
use crate::state::AppState;
use crate::theme;
use gpui::{
    div, prelude::*, px, Context, Entity, MouseButton, ParentElement, Render, Styled, Window,
};

pub struct SidePanelView {
    state: Entity<AppState>,
}

impl SidePanelView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }

    fn branch_user_reply(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let Some(parent) = state.selected.clone() else {
                return;
            };
            let id = state.create_user_downstream(&parent);
            state.selected = Some(id.clone());
            state.editing = Some(id);
            cx.notify();
        });
    }

    fn send_to_agent(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let Some(parent) = state.selected.clone() else {
                return;
            };
            let id = state.create_assistant_downstream(&parent);
            state.selected = Some(id.clone());
            // Build a fake reply that references the conversation path so the
            // simulated stream looks plausible without an ACP subprocess.
            let path = GraphModel::conversation_path(&state.graph, &parent);
            let reply = mock_reply_for(&path);
            state.simulate_stream(&id, reply, cx);
            cx.notify();
        });
    }
}

fn mock_reply_for(path: &[ConversationMessage]) -> String {
    if path.is_empty() {
        return "(no context — try writing something in the parent node)".into();
    }
    let last = path.last().unwrap();
    format!(
        "Stubbed reply ({} turns of context). Last user said: \"{}\". The real build would route this through the existing ACP backend at src-tauri/src/backend/acp/.",
        path.len(),
        truncate(&last.content, 80)
    )
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
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
                    .child(
                        div()
                            .pt(px(4.0))
                            .text_size(px(13.0))
                            .text_color(theme::TEXT)
                            .child(msg.content),
                    ),
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
                    .child(
                        div()
                            .pt(px(2.0))
                            .text_size(px(13.0))
                            .text_color(theme::TEXT)
                            .child(
                                selected_node
                                    .as_ref()
                                    .map(|n| {
                                        if n.content.trim().is_empty() {
                                            "(empty — would open inline editor in real build)".to_string()
                                        } else {
                                            n.content.clone()
                                        }
                                    })
                                    .unwrap_or_default(),
                            ),
                    ),
            )
            .child(
                div()
                    .id("conversation-path")
                    .flex_grow()
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
            .child(
                div()
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
                                cx.listener(|this, _, _, cx| this.branch_user_reply(cx)),
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
                    ),
            )
    }
}
