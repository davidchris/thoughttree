//! GPUI front-end prototype for ThoughtTree.
//!
//! Run with `cargo run` from `src-gpui/`. See README.md for build prerequisites
//! (GPUI is a git dependency from zed-industries/zed and needs Metal/Vulkan).

mod app;
mod graph;
mod state;
mod theme;
mod views;

use app::AppView;
use gpui::{px, size, App, Application, Bounds, WindowBounds, WindowOptions};

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("ThoughtTree (GPUI prototype)".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| AppView::new(window, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}
