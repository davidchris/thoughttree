//! Native TeX formulas for rich answers, typeset by RaTeX (a KaTeX port).
//!
//! Parsing and layout run with the Markdown parse, off the UI thread. Rendering
//! only sizes the prepared vector outline to the surrounding font and color, so
//! formulas stay sharp at any size. Selection and copy keep the TeX source.

use std::sync::{Arc, Mutex};

use gpui::{
    div, img, prelude::*, AnyElement, App, Bounds, Hsla, Image, ImageFormat, Pixels, Point, Rgba,
    SharedString, Window,
};
use gpui_component::{
    text::{
        markdown_ast::Node, InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext,
        MarkdownPlugin,
    },
    ActiveTheme,
};
use ratex_layout::LayoutOptions;
use ratex_svg::{SvgColorSyntax, SvgOptions};
use ratex_types::MathStyle;

const MAX_MATH_BYTES: usize = 8 * 1024;
/// KaTeX sets formulas at 1.21em of the surrounding text.
const KATEX_SCALE: f32 = 1.21;
/// SVG user units per em; RaTeX's default stroke width assumes this scale.
const UNITS_PER_EM: f64 = 40.0;
/// Accents and italic overhangs may leave the TeX box; keep their ink visible.
const PADDING_EM: f64 = 0.05;
/// RaTeX paints in this color; rendering substitutes the surrounding text color.
const LAYOUT_INK: &str = "rgb(0,0,0)";

/// `$…$` inside running text, aligned on the text baseline.
pub(crate) struct InlineMath;

/// `$$…$$` on its own, centered like KaTeX display math.
pub(crate) struct DisplayMath(pub(crate) DisplayRows);

/// The rows display formulas painted, with the TeX each one copies as.
#[derive(Clone, Default)]
pub(crate) struct DisplayRows(Arc<Mutex<Vec<Row>>>);

type Row = (Bounds<Pixels>, SharedString);

impl DisplayRows {
    fn rows(&self) -> std::sync::MutexGuard<'_, Vec<Row>> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub(crate) fn clear(&self) {
        self.rows().clear();
    }

    pub(crate) fn source_at(&self, point: Point<Pixels>) -> Option<SharedString> {
        self.rows()
            .iter()
            .find(|(bounds, _)| bounds.contains(&point))
            .map(|(_, source)| source.clone())
    }
}

impl MarkdownPlugin for InlineMath {
    fn name(&self) -> &str {
        "inline-math"
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::InlineMath(math) = node else {
            return None;
        };
        let source = cx
            .node_source(node)
            .map_or_else(|| format!("${}$", math.value), str::to_owned);
        Some(formula_node(
            self.name(),
            &math.value,
            MathStyle::Text,
            source,
        ))
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<InlineElement> {
        // An invalid formula falls back to its TeX source as ordinary text.
        let formula = node.data::<Result<Formula, String>>()?.as_ref().ok()?;
        let em = context.font_size() * KATEX_SCALE;
        let (image, width, height) = formula.image(em, context.text_style().color);
        Some(
            InlineElement::new(
                img(image)
                    .debug_selector(|| "inline-math".into())
                    .flex_none()
                    .w(width)
                    .h(height),
            )
            .with_baseline(em * formula.ascent),
        )
    }
}

impl MarkdownPlugin for DisplayMath {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "display-math"
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Math(math) = node else {
            return None;
        };
        let source = cx
            .node_source(node)
            .map_or_else(|| format!("$$\n{}\n$$", math.value), str::to_owned);
        Some(formula_node(
            self.name(),
            &math.value,
            MathStyle::Display,
            source,
        ))
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = (
            "display-math",
            node.source_range().map_or(0, |range| range.start),
        );
        match node.data::<Result<Formula, String>>() {
            Some(Ok(formula)) => {
                let style = window.text_style();
                let em = style.font_size.to_pixels(window.rem_size()) * KATEX_SCALE;
                let (image, width, height) = formula.image(em, style.color);
                let (rows, source) = (
                    self.0.clone(),
                    SharedString::from(node.as_text().to_owned()),
                );
                // Wide formulas scroll instead of shrinking, as KaTeX displays do.
                div()
                    .on_children_prepainted(move |bounds, _, _| {
                        rows.rows()
                            .extend(bounds.first().map(|row| (*row, source.clone())));
                    })
                    .id(id)
                    .w_full()
                    .my(em * 0.5)
                    .overflow_x_scroll()
                    .child(
                        div().flex().justify_center().min_w_full().child(
                            img(image)
                                .debug_selector(|| "display-math".into())
                                .flex_none()
                                .w(width)
                                .h(height),
                        ),
                    )
                    .into_any_element()
            }
            Some(Err(error)) => formula_error(error, node.as_text(), cx),
            None => formula_error("The formula was not prepared.", node.as_text(), cx),
        }
    }
}

fn formula_node(name: &str, tex: &str, style: MathStyle, source: String) -> MarkdownNode {
    MarkdownNode::new(name.to_owned(), Formula::prepare(tex, style))
        .text(source.clone())
        .markdown(source)
        .accessibility_label(tex.trim().to_owned())
}

fn formula_error(error: &str, source: &str, cx: &App) -> AnyElement {
    div()
        .debug_selector(|| "math-error".into())
        .my_2()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_color(cx.theme().danger)
                .child(format!("Math error: {error}")),
        )
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().muted_foreground)
                .child(source.to_owned()),
        )
        .into_any_element()
}

/// A typeset formula. Metrics are in em, padding included.
struct Formula {
    /// Everything after the root `<svg>` tag, painted in `currentColor`.
    body: String,
    view_box: String,
    width: f32,
    ascent: f32,
    height: f32,
    /// The last image, reused while font size and color stay the same.
    image: Mutex<Option<(ImageKey, Arc<Image>)>>,
}

type ImageKey = (u32, String);

impl Formula {
    fn prepare(tex: &str, style: MathStyle) -> Result<Self, String> {
        if tex.len() > MAX_MATH_BYTES {
            return Err("The formula exceeds the 8 KiB render limit.".to_owned());
        }
        let nodes = ratex_parser::parse(tex).map_err(|error| error.message)?;
        let options = LayoutOptions {
            style,
            ..Default::default()
        };
        let list = ratex_layout::to_display_list(&ratex_layout::layout(&nodes, &options));
        if list.width <= 0.0 || list.height + list.depth <= 0.0 {
            return Err("The formula is empty.".to_owned());
        }
        let svg = ratex_svg::render_to_svg_with_color_syntax(
            &list,
            &SvgOptions {
                font_size: UNITS_PER_EM,
                padding: PADDING_EM * UNITS_PER_EM,
                embed_glyphs: true,
                ..Default::default()
            },
            SvgColorSyntax::Rgb,
        );
        crate::rich_text::validate_svg(&svg)?;
        let (view_box, body) = split_root(&svg)?;
        Ok(Self {
            body: body.replace(LAYOUT_INK, "currentColor"),
            view_box,
            width: (list.width + 2.0 * PADDING_EM) as f32,
            ascent: (list.height + PADDING_EM) as f32,
            height: (list.height + list.depth + 2.0 * PADDING_EM) as f32,
            image: Mutex::default(),
        })
    }

    /// An SVG whose intrinsic size is its logical size at `em`. GPUI rasterizes
    /// SVG images at twice that size, which matches Retina displays.
    fn image(&self, em: Pixels, color: Hsla) -> (Arc<Image>, Pixels, Pixels) {
        let (width, height) = (em * self.width, em * self.height);
        let key = (f32::from(em).to_bits(), css_color(color.into()));
        let mut cached = self.image.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((_, image)) = cached.as_ref().filter(|(cached, _)| *cached == key) {
            return (image.clone(), width, height);
        }
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{}" color="{}">{}"#,
            f32::from(width),
            f32::from(height),
            self.view_box,
            key.1,
            self.body,
        );
        let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes()));
        *cached = Some((key, image.clone()));
        (image, width, height)
    }
}

/// Split RaTeX's document into its view box and the markup inside the root.
fn split_root(svg: &str) -> Result<(String, String), String> {
    let document = roxmltree::Document::parse(svg).map_err(|error| error.to_string())?;
    let view_box = document
        .root_element()
        .attribute("viewBox")
        .ok_or_else(|| "The formula has no view box.".to_owned())?
        .to_owned();
    let start = svg
        .find("<svg")
        .and_then(|start| svg[start..].find('>').map(|end| start + end + 1))
        .ok_or_else(|| "The formula has no SVG root.".to_owned())?;
    Ok((view_box, svg[start..].to_owned()))
}

fn css_color(color: Rgba) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        channel(color.r),
        channel(color.g),
        channel(color.b),
        channel(color.a)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formula(tex: &str, style: MathStyle) -> Formula {
        Formula::prepare(tex, style).unwrap()
    }

    #[test]
    fn formulas_keep_baseline_metrics_and_paint_in_the_text_color() {
        let inline = formula("E=mc^2", MathStyle::Text);
        assert!(inline.ascent > 0.5 && inline.ascent < inline.height);
        assert!(inline.width > 3.0);
        assert!(inline.body.contains("currentColor"));
        assert!(!inline.body.contains(LAYOUT_INK));
        // Display style stacks limits, as KaTeX does outside running text.
        let sum = r"\sum_{i=1}^{n} x_i";
        assert!(
            formula(sum, MathStyle::Display).height > formula(sum, MathStyle::Text).height + 0.5
        );
    }

    #[gpui::test]
    fn formula_images_match_the_font_and_rasterize_at_twice_their_size(
        cx: &mut gpui::TestAppContext,
    ) {
        let formula = formula(
            r"\frac{1}{n}\sum_{i=1}^{n} x_i = \bar{x}",
            MathStyle::Display,
        );
        let color = gpui::rgb(0xe6f4ee).into();
        let (image, width, height) = formula.image(gpui::px(19.36), color);
        let (again, ..) = formula.image(gpui::px(19.36), color);
        assert!(Arc::ptr_eq(&image, &again));
        let svg = std::str::from_utf8(image.bytes()).unwrap();
        crate::rich_text::validate_svg(svg).unwrap();
        assert!(svg.contains(r##"color="#e6f4eeff""##));
        let raster = cx.update(|cx| image.to_image_data(cx.svg_renderer()).unwrap());
        let size = raster.size(0);
        assert!((size.width.0 as f32 - f32::from(width) * 2.0).abs() <= 1.0);
        assert!((size.height.0 as f32 - f32::from(height) * 2.0).abs() <= 1.0);
        let ink: Vec<_> = raster
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 200)
            .copied()
            .collect();
        assert!(!ink.is_empty());
        // GPUI truncates when it un-premultiplies, so allow one step of rounding.
        assert!(ink.iter().all(|pixel| pixel[..3]
            .iter()
            .zip([0xee_u8, 0xf4, 0xe6])
            .all(|(actual, expected)| actual.abs_diff(expected) <= 1)));
    }

    #[test]
    fn hostile_or_invalid_tex_returns_errors_without_hanging() {
        for tex in [
            r"\frac{1}{",
            "}",
            r"\left(",
            r"\begin{matrix}",
            r"\def\a{\a}\a",
            r"\href{file:///etc/passwd}{x}",
            &"{".repeat(2000),
            &"x".repeat(MAX_MATH_BYTES + 1),
        ] {
            if let Ok(formula) = Formula::prepare(tex, MathStyle::Text) {
                assert!(!formula.body.contains("file:"), "{tex}");
            }
        }
        assert!(Formula::prepare(r"\frac{1}{", MathStyle::Text).is_err());
        assert!(Formula::prepare(" ", MathStyle::Text).is_err());
    }

    #[gpui::test]
    fn answers_flow_inline_math_center_display_math_and_copy_tex(cx: &mut gpui::TestAppContext) {
        let (_directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        let source = "Inline math: \\(E=mc^2\\). Display math:\n\n$$\n\\frac{1}{n}\\sum_{i=1}^{n} x_i = \\bar{x}\n$$\n\nAfter.";
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["answer"].content = source.into();
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            })
        });
        cx.run_until_parked();
        let answer = cx.debug_bounds("rich-answer").expect("rich answer");
        let inline = cx.debug_bounds("inline-math").expect("inline formula");
        let display = cx.debug_bounds("display-math").expect("display formula");
        // The inline formula sits inside the first text line, after its label.
        assert!(inline.left() > answer.left() + gpui::px(40.));
        assert!(inline.top() < answer.top() + gpui::px(12.));
        assert!(display.top() > inline.bottom());
        let display_center = display.center().x;
        assert!((display_center - answer.center().x).abs() < gpui::px(2.));

        let tex = "$$\n\\frac{1}{n}\\sum_{i=1}^{n} x_i = \\bar{x}\n$$";
        let line_start =
            answer.origin + gpui::point(gpui::px(1.), inline.center().y - answer.top());
        let copied = drag_copy(
            cx,
            line_start,
            answer.bottom_right() - gpui::point(gpui::px(1.), gpui::px(1.)),
        );
        assert!(
            copied.contains("Inline math: $E=mc^2$. Display math:"),
            "{copied:?}"
        );
        assert!(copied.contains(tex), "{copied:?}");
        assert!(copied.contains("After."), "{copied:?}");
        // TextView drops a display block at a selection endpoint; RichText restores it.
        let copied = drag_copy(cx, line_start, display.center());
        assert_eq!(
            copied,
            format!("Inline math: $E=mc^2$. Display math:\n\n{tex}")
        );
        let copied = drag_copy(cx, display.center(), line_start);
        assert_eq!(
            copied,
            format!("Inline math: $E=mc^2$. Display math:\n\n{tex}")
        );
        let copied = drag_copy(cx, display.origin, display.bottom_right());
        assert_eq!(copied, tex);
    }

    fn drag_copy(
        cx: &mut gpui::VisualTestContext,
        start: gpui::Point<gpui::Pixels>,
        end: gpui::Point<gpui::Pixels>,
    ) -> String {
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(
            end,
            Some(gpui::MouseButton::Left),
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_keystrokes("cmd-c");
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .expect("selected text")
    }

    #[gpui::test]
    fn invalid_formulas_show_their_source_and_a_readable_error(cx: &mut gpui::TestAppContext) {
        let (_directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["answer"].content =
                "Broken $\\frac{1}{$ inline.\n\n$$\n\\frac{1}{\n$$".into();
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            })
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("inline-math").is_none());
        assert!(cx.debug_bounds("display-math").is_none());
        assert!(cx.debug_bounds("math-error").is_some());
        let answer = cx.debug_bounds("rich-answer").unwrap();
        let start = answer.origin + gpui::point(gpui::px(1.), gpui::px(10.));
        let end = start + gpui::point(answer.size.width - gpui::px(2.), gpui::px(0.));
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(
            end,
            Some(gpui::MouseButton::Left),
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_keystrokes("cmd-c");
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap();
        // The unparsable inline formula stays readable as its own TeX source.
        assert!(copied.contains("Broken $\\frac{1}{$ inline."), "{copied:?}");
    }
}
