//! GPUI render shell for parsed Markdown blocks.
//!
//! Parses via `crate::markdown::parse` then folds blocks into a flex column
//! of GPUI elements. Inlines collapse into a single `StyledText` per block
//! so word wrap respects mixed weight / style runs.
//!
//! Manual smoke only — the parse layer carries the unit tests.

use crate::markdown::{parse, Block, Inline};
use crate::theme;
use gpui::{
    div, prelude::*, px, AnyElement, FontStyle, FontWeight, HighlightStyle, IntoElement,
    SharedString, StyledText,
};
use std::ops::Range;

/// Parse `src` and return a column element rendering each block.
pub fn markdown_view(src: &str) -> AnyElement {
    let blocks = parse(src);
    let mut col = div().flex().flex_col().gap(px(8.0));
    for block in blocks {
        col = col.child(render_block(block));
    }
    col.into_any_element()
}

fn render_block(block: Block) -> AnyElement {
    match block {
        Block::Paragraph(inlines) => {
            let (text, highlights) = flatten_inlines(&inlines);
            div()
                .text_size(px(13.0))
                .text_color(theme::TEXT)
                .child(StyledText::new(text).with_highlights(highlights))
                .into_any_element()
        }
        Block::Heading { level, inlines } => {
            let (text, mut highlights) = flatten_inlines(&inlines);
            // Fold the heading-level weight into every run so inline
            // bold/italic stays additive on top of the base bold heading.
            for (_, hl) in &mut highlights {
                if hl.font_weight.is_none() {
                    hl.font_weight = Some(FontWeight::BOLD);
                }
            }
            let size = match level {
                1 => 20.0,
                2 => 17.0,
                _ => 15.0,
            };
            div()
                .pt(px(4.0))
                .text_size(px(size))
                .text_color(theme::TEXT)
                .child(
                    StyledText::new(text).with_highlights(if highlights.is_empty() {
                        vec![(
                            0..0,
                            HighlightStyle {
                                font_weight: Some(FontWeight::BOLD),
                                ..Default::default()
                            },
                        )]
                    } else {
                        highlights
                    }),
                )
                .into_any_element()
        }
        Block::CodeBlock { lang, text } => {
            let header = lang
                .as_deref()
                .map(|l| {
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::TEXT_DIM)
                        .pb(px(2.0))
                        .child(SharedString::from(l.to_string()))
                        .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element());
            div()
                .flex()
                .flex_col()
                .p(px(8.0))
                .bg(theme::BG_APP)
                .border_1()
                .border_color(theme::BORDER)
                .rounded(px(4.0))
                .child(header)
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme::TEXT)
                        .font_family("Menlo")
                        .child(SharedString::from(text)),
                )
                .into_any_element()
        }
    }
}

/// Walk an inline tree once, accumulating a single `String` and a list of
/// `(byte-range, HighlightStyle)` pairs for `StyledText::with_highlights`.
fn flatten_inlines(inlines: &[Inline]) -> (SharedString, Vec<(Range<usize>, HighlightStyle)>) {
    let mut buf = String::new();
    let mut highlights = Vec::new();
    for inline in inlines {
        write_inline(inline, &mut buf, &mut highlights, HighlightStyle::default());
    }
    (SharedString::from(buf), highlights)
}

fn write_inline(
    inline: &Inline,
    buf: &mut String,
    highlights: &mut Vec<(Range<usize>, HighlightStyle)>,
    style: HighlightStyle,
) {
    match inline {
        Inline::Text(text) => {
            let start = buf.len();
            buf.push_str(text);
            if style != HighlightStyle::default() {
                highlights.push((start..buf.len(), style));
            }
        }
        Inline::Bold(children) => {
            let mut child_style = style;
            child_style.font_weight = Some(FontWeight::BOLD);
            for child in children {
                write_inline(child, buf, highlights, child_style);
            }
        }
        Inline::Italic(children) => {
            let mut child_style = style;
            child_style.font_style = Some(FontStyle::Italic);
            for child in children {
                write_inline(child, buf, highlights, child_style);
            }
        }
        Inline::Code(text) => {
            let start = buf.len();
            buf.push_str(text);
            let mut code_style = style;
            code_style.background_color = Some(theme::BG_APP.into());
            highlights.push((start..buf.len(), code_style));
        }
    }
}
