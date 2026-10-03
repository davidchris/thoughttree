use gpui::{prelude::*, *};
use gpui_component::Root;

mod canvas;
mod commands;
mod dialogs;
mod files;
mod http_client;
mod panel;
mod persistence;
mod rich_text;
mod theme;
mod viewport;
mod workspace;

#[cfg(test)]
mod interaction_tests;
#[cfg(test)]
mod native_persistence_tests;
#[cfg(test)]
mod provider_tests;

use workspace::Workspace;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let (desktop, events) = thoughttree_desktop::Desktop::open_default().unwrap_or_else(|error| {
        eprintln!("Cannot start ThoughtTree: {error}");
        std::process::exit(1);
    });
    let project = std::env::args().nth(1).map(std::path::PathBuf::from);
    Application::new()
        .with_assets(gpui_component_assets::Assets)
        .with_http_client(std::sync::Arc::new(http_client::NativeHttpClient::new()))
        .run(move |cx| {
            gpui_component::init(cx);
            theme::init(cx);
            commands::init_menus(cx);
            let bounds = Bounds::centered(None, size(px(1440.), px(940.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("ThoughtTree".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    let app = cx.new(|cx| Workspace::new(desktop, events, window, cx));
                    app.update(cx, |app, cx| app.ensure_models(cx));
                    let weak = app.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        weak.update(cx, |app, cx| app.request_close(window, cx))
                            .unwrap_or(true)
                    });
                    if let Some(project) = project {
                        app.update(cx, |app, cx| app.open_path(project, window, cx));
                    }
                    cx.new(|cx| Root::new(app, window, cx))
                },
            )
            .expect("open ThoughtTree window");
            cx.activate(true);
        });
}
