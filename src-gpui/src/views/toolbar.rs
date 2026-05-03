use crate::state::AppState;
use crate::theme;
use gpui::{div, prelude::*, px, Context, Entity, ParentElement, Render, Styled, Window};

pub struct ToolbarView {
    state: Entity<AppState>,
}

impl ToolbarView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        // Re-render whenever the project label / streaming counts change.
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }

    fn add_root_node(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            // Place a fresh user node off to the side of the viewport origin.
            let id = state.create_user_at(crate::graph::types::Position { x: 40.0, y: 40.0 });
            state.selected = Some(id.clone());
            state.editing = Some(id);
            cx.notify();
        });
    }
}

impl Render for ToolbarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let project_label = self
            .state
            .read(cx)
            .project_path
            .clone()
            .unwrap_or_else(|| "untitled.thoughttree".into());

        let streaming_count = self.state.read(cx).streaming.len();

        div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(theme::TOOLBAR_HEIGHT))
            .px(px(12.0))
            .gap(px(12.0))
            .bg(theme::BG_PANEL)
            .border_b_1()
            .border_color(theme::BORDER)
            .child(
                div()
                    .text_color(theme::TEXT)
                    .text_size(px(14.0))
                    .child("ThoughtTree (GPUI)"),
            )
            .child(
                div()
                    .text_color(theme::TEXT_DIM)
                    .text_size(px(12.0))
                    .child(project_label),
            )
            .child(div().flex_grow())
            .child(
                div()
                    .text_color(theme::TEXT_DIM)
                    .text_size(px(11.0))
                    .child(if streaming_count > 0 {
                        format!("{streaming_count} streaming…")
                    } else {
                        String::new()
                    }),
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
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.add_root_node(cx)
                    }))
                    .child("+ Node"),
            )
    }
}
