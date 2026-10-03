use gpui::{px, rgb, App, Hsla};
use gpui_component::{Theme, ThemeMode};

pub fn bg() -> Hsla {
    rgb(0x04252c).into()
}
pub fn surface() -> Hsla {
    rgb(0x0a3038).into()
}
pub fn raised() -> Hsla {
    rgb(0x113b43).into()
}
pub fn border() -> Hsla {
    rgb(0x1a464e).into()
}
pub fn text() -> Hsla {
    rgb(0xe6f4ee).into()
}
pub fn muted() -> Hsla {
    rgb(0x9cbcb4).into()
}
pub fn accent() -> Hsla {
    rgb(0xc0facc).into()
}
pub fn danger() -> Hsla {
    rgb(0xf0857f).into()
}
pub fn edge() -> Hsla {
    rgb(0x4f7a76).into()
}

pub fn init(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    theme.background = bg();
    theme.foreground = text();
    theme.primary = accent();
    theme.primary_foreground = bg();
    theme.secondary = raised();
    theme.secondary_foreground = text();
    theme.muted = surface();
    theme.muted_foreground = muted();
    theme.border = border();
    theme.input = raised();
    theme.ring = accent();
    theme.font_size = px(14.);
    theme.radius = px(6.);
}
