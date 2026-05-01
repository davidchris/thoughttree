//! Centralized colors and spacing for the prototype.
//!
//! Mirrors the rough palette of `src/App.css` so the GPUI build looks
//! recognizably like ThoughtTree.

use gpui::{rgb, Hsla, Rgba};

pub const BG_APP: Rgba = rgb(0x1a1a1a);
pub const BG_PANEL: Rgba = rgb(0x232323);
pub const BG_NODE_USER: Rgba = rgb(0x2a3550);
pub const BG_NODE_ASSISTANT: Rgba = rgb(0x2a4232);
pub const BG_NODE_SELECTED: Rgba = rgb(0x4a6fa5);
pub const BORDER: Rgba = rgb(0x3a3a3a);
pub const TEXT: Rgba = rgb(0xe0e0e0);
pub const TEXT_DIM: Rgba = rgb(0x808080);
pub const ACCENT: Rgba = rgb(0x5b9bd5);

pub const NODE_WIDTH: f32 = 240.0;
pub const NODE_MIN_HEIGHT: f32 = 80.0;
pub const SIDE_PANEL_WIDTH: f32 = 380.0;
pub const TOOLBAR_HEIGHT: f32 = 44.0;

#[allow(dead_code)]
pub fn dim(color: Rgba, alpha: f32) -> Hsla {
    let mut hsla: Hsla = color.into();
    hsla.a = alpha;
    hsla
}
