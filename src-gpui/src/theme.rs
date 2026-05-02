//! Centralized colors and spacing for the prototype.
//!
//! Mirrors the rough palette of `src/App.css` so the GPUI build looks
//! recognizably like ThoughtTree.

use gpui::{Hsla, Rgba};

const fn hex(h: u32) -> Rgba {
    Rgba {
        r: ((h >> 16) & 0xff) as f32 / 255.0,
        g: ((h >> 8) & 0xff) as f32 / 255.0,
        b: (h & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

pub const BG_APP: Rgba = hex(0x1a1a1a);
pub const BG_PANEL: Rgba = hex(0x232323);
pub const BG_NODE_USER: Rgba = hex(0x2a3550);
pub const BG_NODE_ASSISTANT: Rgba = hex(0x2a4232);
pub const BG_NODE_SELECTED: Rgba = hex(0x4a6fa5);
pub const BORDER: Rgba = hex(0x3a3a3a);
pub const TEXT: Rgba = hex(0xe0e0e0);
pub const TEXT_DIM: Rgba = hex(0x808080);
pub const ACCENT: Rgba = hex(0x5b9bd5);

pub const NODE_WIDTH: f32 = 240.0;
// Fixed (not minimum) — edges anchor at `pos.y + NODE_HEIGHT`, so a card
// growing past it would visually disconnect from its outgoing edge.
pub const NODE_HEIGHT: f32 = 120.0;
pub const SIDE_PANEL_WIDTH: f32 = 380.0;
pub const TOOLBAR_HEIGHT: f32 = 44.0;

#[allow(dead_code)]
pub fn dim(color: Rgba, alpha: f32) -> Hsla {
    let mut hsla: Hsla = color.into();
    hsla.a = alpha;
    hsla
}
