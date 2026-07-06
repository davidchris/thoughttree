//! Markdown parse layer — pure logic, no GPUI types.
//!
//! Folds `pulldown-cmark` events into a small block/inline tree that
//! the render shell (in `views::markdown_view`) maps onto GPUI elements.
//! Tested directly via `cargo test markdown::`.

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Code(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading { level: u8, inlines: Vec<Inline> },
    CodeBlock { lang: Option<String>, text: String },
}

/// Parse a Markdown source string into a flat `Vec<Block>`.
///
/// Empty input returns an empty vec.
pub fn parse(src: &str) -> Vec<Block> {
    use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

    enum OpenBlock {
        Paragraph,
        Heading(u8),
        CodeBlock { lang: Option<String>, text: String },
    }

    fn heading_level(level: HeadingLevel) -> u8 {
        match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        }
    }

    let mut blocks = Vec::new();
    let mut open_block: Option<OpenBlock> = None;
    // Stack of in-progress inline runs. Bottom frame belongs to the
    // currently open block; nested inline tags (Bold, Italic) push and
    // pop their own frames on top of it.
    let mut stack: Vec<Vec<Inline>> = Vec::new();

    let push_inline = |stack: &mut Vec<Vec<Inline>>, inline: Inline| {
        if let Some(top) = stack.last_mut() {
            top.push(inline);
        }
    };

    for event in Parser::new(src) {
        match event {
            Event::Start(Tag::Paragraph) => {
                open_block = Some(OpenBlock::Paragraph);
                stack.push(Vec::new());
            }
            Event::End(TagEnd::Paragraph) => {
                if let (Some(OpenBlock::Paragraph), Some(inlines)) = (open_block.take(), stack.pop()) {
                    blocks.push(Block::Paragraph(inlines));
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                open_block = Some(OpenBlock::Heading(heading_level(level)));
                stack.push(Vec::new());
            }
            Event::End(TagEnd::Heading(_)) => {
                if let (Some(OpenBlock::Heading(level)), Some(inlines)) =
                    (open_block.take(), stack.pop())
                {
                    blocks.push(Block::Heading { level, inlines });
                }
            }
            Event::Start(Tag::Strong) => stack.push(Vec::new()),
            Event::End(TagEnd::Strong) => {
                if let Some(inner) = stack.pop() {
                    push_inline(&mut stack, Inline::Bold(inner));
                }
            }
            Event::Start(Tag::Emphasis) => stack.push(Vec::new()),
            Event::End(TagEnd::Emphasis) => {
                if let Some(inner) = stack.pop() {
                    push_inline(&mut stack, Inline::Italic(inner));
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                use pulldown_cmark::CodeBlockKind;
                let lang = match kind {
                    CodeBlockKind::Fenced(s) if !s.is_empty() => Some(s.into_string()),
                    _ => None,
                };
                open_block = Some(OpenBlock::CodeBlock {
                    lang,
                    text: String::new(),
                });
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(OpenBlock::CodeBlock { lang, text }) = open_block.take() {
                    blocks.push(Block::CodeBlock { lang, text });
                }
            }
            Event::Text(text) => {
                if let Some(OpenBlock::CodeBlock { text: buf, .. }) = open_block.as_mut() {
                    buf.push_str(&text);
                } else {
                    push_inline(&mut stack, Inline::Text(text.into_string()));
                }
            }
            Event::Code(text) => push_inline(&mut stack, Inline::Code(text.into_string())),
            _ => {}
        }
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_no_blocks() {
        assert_eq!(parse(""), Vec::<Block>::new());
    }

    #[test]
    fn plain_text_becomes_one_paragraph() {
        assert_eq!(
            parse("hello"),
            vec![Block::Paragraph(vec![Inline::Text("hello".into())])],
        );
    }

    #[test]
    fn bold_emphasis_wraps_inner_runs() {
        assert_eq!(
            parse("**hi**"),
            vec![Block::Paragraph(vec![Inline::Bold(vec![Inline::Text(
                "hi".into()
            )])])],
        );
    }

    #[test]
    fn italic_emphasis_wraps_inner_runs() {
        assert_eq!(
            parse("*hi*"),
            vec![Block::Paragraph(vec![Inline::Italic(vec![Inline::Text(
                "hi".into()
            )])])],
        );
    }

    #[test]
    fn inline_code_becomes_code_run() {
        assert_eq!(
            parse("`x`"),
            vec![Block::Paragraph(vec![Inline::Code("x".into())])],
        );
    }

    #[test]
    fn heading_carries_level_and_inlines() {
        assert_eq!(
            parse("# Title"),
            vec![Block::Heading {
                level: 1,
                inlines: vec![Inline::Text("Title".into())],
            }],
        );
    }

    #[test]
    fn fenced_code_block_carries_lang_and_text() {
        assert_eq!(
            parse("```rust\nfn x(){}\n```"),
            vec![Block::CodeBlock {
                lang: Some("rust".into()),
                text: "fn x(){}\n".into(),
            }],
        );
    }
}
