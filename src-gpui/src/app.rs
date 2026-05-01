//! Top-level App view — composes Toolbar / Canvas / SidePanel.
//!
//! Mirrors the JSX shell in `src/App.tsx`:
//!
//! ```tsx
//! <Toolbar />
//! <div className="app-main">
//!   <Graph />
//!   <SidePanel />
//! </div>
//! ```

use crate::state::AppState;
use crate::theme;
use crate::views::{CanvasView, SidePanelView, ToolbarView};
use gpui::{
    div, prelude::*, Context, Entity, ParentElement, Render, Styled, Window,
};

pub struct AppView {
    toolbar: Entity<ToolbarView>,
    canvas: Entity<CanvasView>,
    side_panel: Entity<SidePanelView>,
}

impl AppView {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|_| AppState::new());
        let toolbar = cx.new(|cx| ToolbarView::new(state.clone(), cx));
        let canvas = cx.new(|cx| CanvasView::new(state.clone(), cx));
        let side_panel = cx.new(|cx| SidePanelView::new(state.clone(), cx));
        Self {
            toolbar,
            canvas,
            side_panel,
        }
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::BG_APP)
            .text_color(theme::TEXT)
            .child(self.toolbar.clone())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_grow()
                    .child(div().flex_grow().child(self.canvas.clone()))
                    .child(self.side_panel.clone()),
            )
    }
}
