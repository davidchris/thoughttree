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
    // `update`, unlike `global_mut`, carries the edits into the semantic
    // tokens and the text view defaults that tables and code blocks read.
    Theme::update(cx, |theme| {
        theme.background = bg();
        theme.foreground = text();
        theme.primary = accent();
        theme.primary_foreground = bg();
        theme.secondary = raised();
        theme.secondary_foreground = text();
        theme.muted = surface();
        theme.muted_foreground = muted();
        theme.popover = raised();
        theme.popover_foreground = text();
        theme.table_head = surface();
        theme.table_head_foreground = text();
        theme.border = border();
        theme.input = raised();
        theme.ring = accent();
        theme.font_size = px(14.);
        theme.radius = px(6.);
    });
}

#[cfg(test)]
mod tests {
    use gpui_component::ActiveTheme;

    #[gpui::test]
    fn palette_reaches_the_semantic_tokens_rich_text_reads(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            super::init(cx);
            let theme = cx.theme();
            // Renderable tokens drift from the colors unless edits go through `update`.
            assert_eq!(theme.tokens.popover.color, super::raised());
            assert_eq!(theme.tokens.muted.color, super::surface());
            assert_eq!(theme.tokens.table_head.color, super::surface());
            assert_eq!(theme.tokens.table_head_foreground.color, super::text());
        });
    }
}
