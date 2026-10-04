//! Native Markdown with in-process Mermaid rendering and native TeX formulas.
//!
//! Generated figures use an entity-owned image cache. No document text becomes
//! executable HTML, an operating-system file path, or a temporary image file.

use std::{collections::HashMap, future::Future, ops::Range, sync::Arc};

use base64::Engine;
use futures::AsyncReadExt;
use gpui::{
    div, image_cache, prelude::*, App, Asset, ClipboardItem, Context, Entity, FocusHandle, Image,
    ImageCache, ImageCacheError, ImageFormat, Render, RenderImage, Resource, Task, Window,
};
use gpui_component::{text::TextView, ActiveTheme};
use markdown::{mdast::Node, ParseOptions};

use crate::math::{DisplayMath, InlineMath};

const GENERATED_SCHEME: &str = "thoughttree-render://";
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
const MAX_IMAGE_SIDE: u32 = 8000;
const MAX_DIAGRAM_BYTES: usize = 64 * 1024;

/// A selectable rich answer. Keep this entity alive across parent renders.
pub struct RichText {
    source: String,
    markdown: String,
    images: Entity<RichImageCache>,
    generation: u64,
    preparing: bool,
    task: Option<Task<()>>,
    focus: FocusHandle,
    selected_all: bool,
}

impl RichText {
    pub fn new(source: impl Into<String>, cx: &mut Context<Self>) -> Self {
        let images = cx.new(|_| RichImageCache::default());
        App::observe_release(cx, &images, |cache, cx| cache.replace(HashMap::new(), cx)).detach();
        let mut view = Self {
            source: String::new(),
            markdown: String::new(),
            images,
            generation: 0,
            preparing: false,
            task: None,
            focus: cx.focus_handle(),
            selected_all: false,
        };
        view.set_text(source, cx);
        view
    }

    /// Prepare only changed answers. The caller renders active streams as text.
    pub fn set_text(&mut self, source: impl Into<String>, cx: &mut Context<Self>) {
        let source = source.into();
        if source == self.source {
            return;
        }
        self.source = source.clone();
        self.selected_all = false;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.preparing = true;
        // Never show a previous node's answer while its replacement is prepared.
        self.markdown.clear();
        self.task = Some(cx.spawn(async move |view, cx| {
            let prepared = cx
                .background_spawn(async move { prepare_document(&source) })
                .await;
            let _ = view.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.images
                    .update(cx, |cache, cx| cache.replace(prepared.images, cx));
                view.markdown = prepared.markdown;
                view.preparing = false;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// GPUI's TextView exposes partial selection, but no Select All operation.
    /// Keep whole-answer selection source-aware so figures and Markdown survive.
    pub fn handle_shortcut(
        &mut self,
        key: &str,
        command: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !command || !self.focus.contains_focused(window, cx) {
            return false;
        }
        match key {
            "a" => {
                self.selected_all = true;
                cx.notify();
                true
            }
            "c" if self.selected_all => {
                cx.write_to_clipboard(ClipboardItem::new_string(self.source.clone()));
                true
            }
            _ => false,
        }
    }
}

impl Render for RichText {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.preparing {
            return div()
                .py_2()
                .text_color(cx.theme().muted_foreground)
                .child("Preparing rich content…")
                .into_any_element();
        }
        let text = TextView::markdown("answer", self.markdown.clone())
            .plugin(InlineMath)
            .plugin(DisplayMath)
            .selectable(true);
        div()
            .id("rich-answer")
            .debug_selector(|| "rich-answer".into())
            .w_full()
            .track_focus(&self.focus)
            .when(self.selected_all, |d| d.bg(cx.theme().selection))
            .capture_any_mouse_down(cx.listener(|this, _, window, cx| {
                this.selected_all = false;
                this.focus.focus(window, cx);
                cx.notify();
            }))
            .child(image_cache(self.images.clone()).w_full().child(text))
            .into_any_element()
    }
}

#[derive(Default)]
struct RichImageCache {
    generated: HashMap<String, Arc<Image>>,
}

impl RichImageCache {
    fn replace(&mut self, images: HashMap<String, Arc<Image>>, cx: &mut App) {
        for image in self.generated.values() {
            cx.remove_asset::<NativeImageDecoder>(image);
        }
        self.generated = images;
    }
}

impl ImageCache for RichImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let uri = match resource {
            Resource::Uri(uri) => uri.as_ref(),
            _ => {
                return Some(Err(image_error(
                    "Local images are not loaded from answer text.",
                )))
            }
        };
        if let Some(image) = self.generated.get(uri) {
            return window
                .use_asset::<NativeImageDecoder>(image, cx)
                .map(|result| result.map_err(|error| image_error(&error)));
        }
        if !web_url(uri) {
            return Some(Err(image_error("Unsupported image address.")));
        }
        match window.use_asset::<RemoteImage>(&uri.to_owned(), cx)? {
            Ok(image) => window
                .use_asset::<NativeImageDecoder>(&image, cx)
                .map(|result| result.map_err(|error| image_error(&error))),
            Err(error) => Some(Err(image_error(&error))),
        }
    }
}

fn image_error(message: &str) -> ImageCacheError {
    ImageCacheError::Asset(message.to_owned().into())
}

struct NativeImageDecoder;

impl Asset for NativeImageDecoder {
    type Source = Arc<Image>;
    type Output = Result<Arc<RenderImage>, String>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let renderer = cx.svg_renderer();
        // GPUI rasterizes SVGs at twice their logical size in straight BGRA.
        async move {
            source
                .to_image_data(renderer)
                .map_err(|error| error.to_string())
        }
    }
}

/// A bounded remote image loader. SVGs cannot refer to local or remote files.
struct RemoteImage;

impl Asset for RemoteImage {
    type Source = String;
    type Output = Result<Arc<Image>, String>;

    fn load(source: String, cx: &mut App) -> impl Future<Output = Self::Output> + Send + 'static {
        let client = cx.http_client();
        async move {
            if !web_url(&source) {
                return Err("Unsupported image address.".to_owned());
            }
            let mut response = client
                .get(&source, ().into(), true)
                .await
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("Image request returned {}", response.status()));
            }
            let mut bytes = Vec::new();
            response
                .body_mut()
                .take((MAX_IMAGE_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .await
                .map_err(|e| e.to_string())?;
            image_from_bytes(bytes)
        }
    }
}

fn image_from_bytes(bytes: Vec<u8>) -> Result<Arc<Image>, String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Image exceeds the 16 MiB preview limit.".to_owned());
    }
    if let Ok(format) = image::guess_format(&bytes) {
        let reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
        let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
        if width > MAX_IMAGE_SIDE || height > MAX_IMAGE_SIDE {
            return Err("Image dimensions exceed the preview limit.".to_owned());
        }
        let format = ImageFormat::from_mime_type(format.to_mime_type())
            .ok_or_else(|| "Unsupported image format.".to_owned())?;
        return Ok(Arc::new(Image::from_bytes(format, bytes)));
    }
    let svg = std::str::from_utf8(&bytes).map_err(|_| "Unsupported image format.".to_owned())?;
    validate_svg(svg)?;
    Ok(Arc::new(Image::from_bytes(ImageFormat::Svg, bytes)))
}

#[derive(Default)]
struct PreparedDocument {
    markdown: String,
    images: HashMap<String, Arc<Image>>,
}

struct Replacement {
    range: Range<usize>,
    text: String,
}

fn prepare_document(source: &str) -> PreparedDocument {
    let normalized = normalize_math_delimiters(source);
    let mut options = ParseOptions::gfm();
    options.constructs.math_flow = true;
    options.constructs.math_text = true;
    let tree = match markdown::to_mdast(&normalized, &options) {
        Ok(tree) => tree,
        Err(error) => {
            return PreparedDocument {
                markdown: format!(
                    "Markdown error: {}\n\n{}",
                    escape_text(&error.to_string()),
                    source_fence(source)
                ),
                ..Default::default()
            }
        }
    };
    let mut prepared = PreparedDocument::default();
    let mut replacements = Vec::new();
    prepare_node(&tree, &mut prepared, &mut replacements);
    prepared.markdown = apply_replacements(&normalized, replacements);
    prepared
}

fn prepare_node(node: &Node, document: &mut PreparedDocument, replacements: &mut Vec<Replacement>) {
    let replacement = match node {
        Node::Code(code) if code.lang.as_deref() == Some("mermaid") => Some(rendered_figure(
            "Mermaid",
            &code.value,
            render_mermaid(&code.value),
            document,
        )),
        Node::Link(link) if !link_url(&link.url) => Some(escape_text(&node.to_string())),
        Node::Image(image) if !web_url(&image.url) => {
            Some(escape_text(&format!("[Image: {}]", image.alt)))
        }
        Node::Definition(definition) if !link_url(&definition.url) => Some(String::new()),
        // ReactMarkdown treats raw HTML as text. Keep that boundary in native UI.
        Node::Html(html) => Some(escape_text(&html.value)),
        _ => None,
    };
    if let (Some(text), Some(position)) = (replacement, node.position()) {
        replacements.push(Replacement {
            range: position.start.offset..position.end.offset,
            text,
        });
        return;
    }
    if let Some(children) = node.children() {
        for child in children {
            prepare_node(child, document, replacements);
        }
    }
}

fn rendered_figure(
    kind: &str,
    source: &str,
    rendered: Result<String, String>,
    document: &mut PreparedDocument,
) -> String {
    let image = rendered.and_then(|svg| image_from_bytes(svg.into_bytes()));
    match image {
        Ok(image) => {
            let key = format!("{GENERATED_SCHEME}{}.svg", image.id());
            document.images.insert(key.clone(), image);
            let label = source.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("![{}]({key})", escape_text(&label))
        }
        Err(error) => format!(
            "\n\n**{kind} error:** {}\n\n{}\n\n",
            escape_text(&error),
            source_fence(source)
        ),
    }
}

fn render_mermaid(source: &str) -> Result<String, String> {
    if source.len() > MAX_DIAGRAM_BYTES {
        return Err("The diagram exceeds the 64 KiB render limit.".to_owned());
    }
    if !has_mermaid_header(source) {
        return Err("The diagram needs a supported Mermaid diagram declaration.".to_owned());
    }
    let mut options = mermaid_rs_renderer::RenderOptions {
        theme: mermaid_rs_renderer::Theme::dark(),
        ..Default::default()
    };
    options.theme.background = "#113b43".to_owned();
    options.theme.primary_color = "#0a3038".to_owned();
    options.theme.secondary_color = "#0a3038".to_owned();
    options.theme.tertiary_color = "#0a3038".to_owned();
    options.theme.primary_text_color = "#e6f4ee".to_owned();
    options.theme.text_color = "#e6f4ee".to_owned();
    options.theme.primary_border_color = "#c0facc".to_owned();
    options.theme.line_color = "#91c6bd".to_owned();
    options.theme.edge_label_background = "#113b43".to_owned();
    options.theme.cluster_background = "#04252c".to_owned();
    options.theme.cluster_border = "#1a464e".to_owned();
    mermaid_rs_renderer::render_with_options(source, options).map_err(|e| e.to_string())
}

// The native parser accepts arbitrary text as an implicit flowchart. Mermaid
// fences must retain the source frontend's explicit diagram declaration rule.
fn has_mermaid_header(source: &str) -> bool {
    let mut frontmatter = false;
    for line in source.lines().map(str::trim) {
        if line == "---" {
            frontmatter = !frontmatter;
            continue;
        }
        if frontmatter || line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let header = line
            .split(|character: char| character.is_whitespace() || matches!(character, ';' | ':'))
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let header = header
            .strip_suffix("-beta")
            .or_else(|| header.strip_suffix("-v2"))
            .unwrap_or(&header);
        return matches!(
            header,
            "graph"
                | "flowchart"
                | "sequencediagram"
                | "classdiagram"
                | "statediagram"
                | "erdiagram"
                | "pie"
                | "mindmap"
                | "journey"
                | "timeline"
                | "gantt"
                | "requirementdiagram"
                | "gitgraph"
                | "c4context"
                | "c4container"
                | "c4component"
                | "c4dynamic"
                | "c4deployment"
                | "sankey"
                | "quadrantchart"
                | "zenuml"
                | "block"
                | "packet"
                | "kanban"
                | "architecture"
                | "radar"
                | "treemap"
                | "xychart"
        );
    }
    false
}

/// SVG rasterization must not resolve arbitrary image references on the host.
pub(crate) fn validate_svg(svg: &str) -> Result<(), String> {
    let document = roxmltree::Document::parse(svg).map_err(|e| e.to_string())?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return Err("The renderer did not return an SVG image.".to_owned());
    }
    validate_svg_dimensions(root)?;
    for node in document.descendants().filter(|node| node.is_element()) {
        validate_svg_element(node)?;
    }
    Ok(())
}

fn validate_svg_dimensions(root: roxmltree::Node<'_, '_>) -> Result<(), String> {
    let viewbox: Vec<f64> = root
        .attribute("viewBox")
        .unwrap_or_default()
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    for (attribute, index) in [("width", 2), ("height", 3)] {
        let dimension = root
            .attribute(attribute)
            .and_then(svg_pixels)
            .or_else(|| viewbox.get(index).copied());
        if dimension.is_some_and(|value| {
            !value.is_finite() || value <= 0.0 || value > MAX_IMAGE_SIDE as f64
        }) {
            return Err("SVG dimensions exceed the preview limit.".to_owned());
        }
    }
    Ok(())
}

fn svg_pixels(length: &str) -> Option<f64> {
    let length = length.trim();
    for (unit, factor) in [
        ("px", 1.0),
        ("pt", 96.0 / 72.0),
        ("pc", 16.0),
        ("in", 96.0),
        ("cm", 96.0 / 2.54),
        ("mm", 96.0 / 25.4),
        ("em", 16.0),
        ("ex", 8.0),
    ] {
        if let Some(number) = length.strip_suffix(unit) {
            return number.parse::<f64>().ok().map(|number| number * factor);
        }
    }
    length.parse().ok()
}

fn validate_svg_element(node: roxmltree::Node<'_, '_>) -> Result<(), String> {
    if matches!(node.tag_name().name(), "script" | "foreignObject") {
        return Err("Executable SVG content is not supported.".to_owned());
    }
    for attribute in node.attributes() {
        if attribute.name() == "href"
            && !attribute.value().starts_with('#')
            && !(node.tag_name().name() == "image" && embedded_raster_image(attribute.value()))
        {
            return Err("External SVG references are not supported.".to_owned());
        }
        if attribute.name().starts_with("on") || unsafe_css(attribute.value()) {
            return Err("External or executable SVG content is not supported.".to_owned());
        }
    }
    if node.tag_name().name() == "style" && node.text().is_some_and(unsafe_css) {
        return Err("External SVG styles are not supported.".to_owned());
    }
    Ok(())
}

// Mermaid C4 person symbols embed small PNGs. Permit bounded raster data only;
// another SVG could recursively load resources beyond this validation boundary.
fn embedded_raster_image(uri: &str) -> bool {
    let Some((mime, data)) = uri
        .strip_prefix("data:")
        .and_then(|uri| uri.split_once(";base64,"))
    else {
        return false;
    };
    if !matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    ) || data.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4
    {
        return false;
    }
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return false;
    };
    if bytes.len() > MAX_IMAGE_BYTES {
        return false;
    }
    let Ok(format) = image::guess_format(&bytes) else {
        return false;
    };
    format.to_mime_type() == mime
        && image::ImageReader::with_format(std::io::Cursor::new(bytes), format)
            .into_dimensions()
            .is_ok_and(|(width, height)| {
                width > 0 && height > 0 && width <= MAX_IMAGE_SIDE && height <= MAX_IMAGE_SIDE
            })
}

fn unsafe_css(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("@import")
        || lower.split("url(").skip(1).any(|suffix| {
            !suffix
                .trim_start()
                .trim_start_matches(['\'', '"'])
                .starts_with('#')
        })
}

fn web_url(value: &str) -> bool {
    url::Url::parse(value)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

fn link_url(value: &str) -> bool {
    web_url(value) || url::Url::parse(value).is_ok_and(|url| url.scheme() == "mailto")
}

fn escape_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if "\\`*_{}[]<>()#+-.!|~$".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn source_fence(source: &str) -> String {
    let longest = source
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(3.max(longest + 1));
    format!("{fence}text\n{source}\n{fence}")
}

fn apply_replacements(source: &str, mut replacements: Vec<Replacement>) -> String {
    replacements.sort_by_key(|replacement| replacement.range.start);
    let mut output = String::new();
    let mut cursor = 0;
    for replacement in replacements {
        if replacement.range.start < cursor {
            continue;
        }
        output.push_str(&source[cursor..replacement.range.start]);
        output.push_str(&replacement.text);
        cursor = replacement.range.end;
    }
    output.push_str(&source[cursor..]);
    output
}

/// Convert LaTeX delimiters while preserving fenced, indented, and inline code.
fn normalize_math_delimiters(source: &str) -> String {
    let Ok(tree) = markdown::to_mdast(source, &ParseOptions::gfm()) else {
        return source.to_owned();
    };
    let mut code = Vec::new();
    collect_code_ranges(&tree, &mut code);
    code.sort_by_key(|range| range.start);
    let mut output = String::new();
    let mut cursor = 0;
    for range in code {
        output.push_str(&normalize_prose(&source[cursor..range.start]));
        output.push_str(&source[range.clone()]);
        cursor = range.end;
    }
    output.push_str(&normalize_prose(&source[cursor..]));
    output
}

fn collect_code_ranges(node: &Node, ranges: &mut Vec<Range<usize>>) {
    if matches!(node, Node::Code(_) | Node::InlineCode(_)) {
        if let Some(position) = node.position() {
            ranges.push(position.start.offset..position.end.offset);
        }
        return;
    }
    if let Some(children) = node.children() {
        for child in children {
            collect_code_ranges(child, ranges);
        }
    }
}

fn normalize_prose(source: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    while cursor < source.len() {
        let rest = &source[cursor..];
        if let Some((consumed, replacement)) = latex_delimited(rest) {
            output.push_str(&replacement);
            cursor += consumed;
            continue;
        }
        // An escaped backslash cannot start a math delimiter.
        if rest.starts_with("\\\\") {
            output.push_str("\\\\");
            cursor += 2;
            continue;
        }
        let character = rest.chars().next().expect("cursor is within source");
        output.push(character);
        cursor += character.len_utf8();
    }
    output
}

fn latex_delimited(source: &str) -> Option<(usize, String)> {
    let (close, delimiter) = if source.starts_with("\\[") {
        ("\\]", "$$")
    } else if source.starts_with("\\(") {
        ("\\)", "$")
    } else {
        return None;
    };
    let end = source[2..].find(close)? + 2;
    Some((
        end + 2,
        format!("{delimiter}{}{delimiter}", &source[2..end]),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latex_delimiters_preserve_code_escapes_and_unicode() {
        let source = "λ \\(x^2\\)\n\\[y = 2\\]\n`\\(code\\)`\n````text\n\\[fenced\\]\n```\n````\n\n    \\(indented\\)\n\\\\(escaped)";
        assert_eq!(normalize_math_delimiters(source),
            "λ $x^2$\n$$y = 2$$\n`\\(code\\)`\n````text\n\\[fenced\\]\n```\n````\n\n    \\(indented\\)\n\\\\(escaped)");
    }

    #[test]
    fn sanitization_removes_local_and_executable_links_without_changing_code() {
        let source = "[unsafe](javascript:alert(1)) [private](file:///etc/passwd)\n\n![secret](file:///etc/passwd)\n\n<img src=\"/etc/passwd\">\n\n[web](https://example.com)\n\n```html\n<script>literal</script>\n```";
        let result = prepare_document(source);
        let parsed = markdown::to_mdast(&result.markdown, &ParseOptions::gfm()).unwrap();
        fn assert_safe(node: &Node) {
            match node {
                Node::Link(link) => assert!(link_url(&link.url)),
                Node::Image(image) => assert!(web_url(&image.url)),
                Node::Html(_) => panic!("raw HTML escaped the boundary"),
                _ => {}
            }
            if let Some(children) = node.children() {
                for child in children {
                    assert_safe(child);
                }
            }
        }
        assert_safe(&parsed);
        assert!(result
            .markdown
            .contains("```html\n<script>literal</script>\n```"));
        assert!(result.markdown.contains("[web](https://example.com)"));
    }

    #[test]
    fn source_error_fence_cannot_be_closed_by_untrusted_input() {
        let source = "bad\n```\n![host](file:///etc/passwd)\n````";
        let tree = markdown::to_mdast(&source_fence(source), &ParseOptions::gfm()).unwrap();
        let children = tree.children().unwrap();
        assert_eq!(children.len(), 1);
        assert!(matches!(&children[0], Node::Code(code) if code.value == source));
    }

    #[test]
    fn svg_allows_glyph_references_but_refuses_host_files_and_scripts() {
        assert!(validate_svg(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><use href="#glyph"/></svg>"##
        )
        .is_ok());
        for svg in [
            r#"<svg><image href="file:///etc/passwd"/></svg>"#,
            r#"<svg><image href="data:image/svg+xml;base64,PHN2Zy8+"/></svg>"#,
            r#"<svg><image href="data:image/png;base64,bm90IGEgcGljdHVyZQ=="/></svg>"#,
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg><style>@import '/etc/passwd';</style></svg>"#,
            r#"<svg><rect style="fill:url(file:///etc/passwd)"/></svg>"#,
            r#"<svg width="1000000000" height="1000000000"/>"#,
            r#"<svg viewBox="0 0 1000000000 1000000000"/>"#,
        ] {
            assert!(validate_svg(svg).is_err(), "{svg}");
        }
    }

    #[test]
    fn embedded_svg_raster_images_require_matching_bytes_and_bounded_dimensions() {
        for (width, accepted) in [(2, true), (MAX_IMAGE_SIDE + 1, false)] {
            let mut png = std::io::Cursor::new(Vec::new());
            image::RgbImage::from_pixel(width, 3, image::Rgb([17, 59, 67]))
                .write_to(&mut png, image::ImageFormat::Png)
                .unwrap();
            let data = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
            let svg = format!(r#"<svg><image href="data:image/png;base64,{data}"/></svg>"#);
            assert_eq!(validate_svg(&svg).is_ok(), accepted);
            assert!(validate_svg(&svg.replace("image/png", "image/jpeg")).is_err());
            assert!(validate_svg(&svg.replace("<image ", "<use ")).is_err());
        }
    }

    #[gpui::test]
    fn diagrams_become_crisp_native_images_and_math_stays_tex_for_the_view(
        cx: &mut gpui::TestAppContext,
    ) {
        let source = r#"# Native

Before $x^2$ after. Inline LaTeX \(E=mc^2\).

\[
\int_a^b x \, dx
\]

| A | B |
| - | - |
| 1 | 2 |

```mermaid
flowchart LR
 A[Read] --> B[Think]
```

```rust
let text = "$literal$";
```"#;
        let result = prepare_document(source);
        assert_eq!(result.images.len(), 1);
        assert!(result
            .markdown
            .contains("Before $x^2$ after. Inline LaTeX $E=mc^2$."));
        assert!(result.markdown.contains("$$\n\\int_a^b x \\, dx\n$$"));
        assert!(result.markdown.contains("| A | B |"));
        assert!(result
            .markdown
            .contains("```rust\nlet text = \"$literal$\";\n```"));
        let image = result.images.values().next().unwrap();
        let svg = std::str::from_utf8(&image.bytes).unwrap();
        validate_svg(svg).unwrap();
        let logical_width = roxmltree::Document::parse(svg)
            .unwrap()
            .root_element()
            .attribute("width")
            .and_then(svg_pixels)
            .unwrap();
        // Exercise the exact SVG decoder used by native GPUI image elements.
        let raster = cx
            .update(|cx| futures::executor::block_on(NativeImageDecoder::load(image.clone(), cx)))
            .unwrap();
        // Diagrams rasterize at twice their logical size, so Retina stays sharp.
        assert_eq!(raster.size(0).width.0, (logical_width * 2.0) as i32);
        assert!(raster
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0));
    }

    #[gpui::test]
    fn svg_pixels_keep_the_theme_color_and_translucent_edges(cx: &mut gpui::TestAppContext) {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2\" height=\"1\"><path fill=\"#113b43\" d=\"M0 0H1V1H0Z\"/><path fill=\"#e6f4ee\" opacity=\"0.5\" d=\"M1 0H2V1H1Z\"/></svg>";
        let image = image_from_bytes(svg.as_bytes().to_vec()).unwrap();
        let raster = cx
            .update(|cx| futures::executor::block_on(NativeImageDecoder::load(image, cx)))
            .unwrap();
        assert_eq!((raster.size(0).width.0, raster.size(0).height.0), (4, 2));
        let pixels = raster.as_bytes(0).unwrap().as_chunks::<4>().0;
        assert_eq!(pixels[0], [0x43, 0x3b, 0x11, 255]);
        // Straight BGRA keeps the bright foreground where edges are translucent.
        for (actual, expected) in pixels[3][..3].iter().zip([0xee_u8, 0xf4, 0xe6]) {
            assert!(actual.abs_diff(expected) <= 1);
        }
        assert!(pixels[3][3].abs_diff(128) <= 1);
    }

    #[test]
    fn invalid_diagram_keeps_readable_error_and_exact_source() {
        let source = "```mermaid\nnot-a-diagram\n```";
        let result = prepare_document(source);
        assert!(result.images.is_empty());
        assert!(result.markdown.contains("**Mermaid error:**"));
        assert!(result.markdown.contains("not-a-diagram"));
    }

    #[gpui::test]
    fn mermaid_families_keep_labels_and_decode_as_native_figures(cx: &mut gpui::TestAppContext) {
        let diagrams = [
            ("flowchart LR\n subgraph Evidence\n A[Read] --> B[Reflect]\n end", "Reflect"),
            ("sequenceDiagram\n Reader->>Writer: Review\n Writer-->>Reader: Accepted", "Accepted"),
            ("classDiagram\n class Thought {\n +String title\n +revise()\n }", "Thought"),
            ("stateDiagram-v2\n [*] --> Draft\n Draft --> Saved", "Saved"),
            ("erDiagram\n AUTHOR ||--o{ NOTE : writes\n NOTE {\n string title\n }", "AUTHOR"),
            ("pie title Sources\n \"Books\" : 3\n \"Papers\" : 5", "Books"),
            ("gantt\n title Reading\n dateFormat YYYY-MM-DD\n section Review\n Read :a1, 2026-01-01, 2d", "Reading"),
            ("mindmap\n root((Ideas))\n   Evidence\n   Questions", "Questions"),
            ("timeline\n title Progress\n 2025 : Draft\n 2026 : Review", "Review"),
            ("journey\n title Research\n section Reading\n Notes: 5: Reader", "Notes"),
            ("gitGraph\n commit id: \"Draft\"\n branch review\n checkout review\n commit id: \"Revise\"", "Revise"),
            ("xychart-beta\n title \"Evidence\"\n x-axis [Books, Papers]\n y-axis \"Count\" 0 --> 10\n bar [3, 5]", "Evidence"),
            ("quadrantChart\n title Priorities\n x-axis Low --> High\n y-axis Slow --> Fast\n Read: [0.2, 0.8]", "Read"),
            ("sankey-beta\n Books,Notes,3\n Papers,Notes,5", "Notes"),
            ("block-beta\n columns 2\n Read[\"Read\"] Review[\"Review\"]\n Read --> Review", "Review"),
            ("architecture-beta\n group vault(cloud)[Vault]\n service notes(database)[Notes] in vault\n service reader(server)[Reader] in vault\n reader:R --> L:notes", "Notes"),
            ("requirementDiagram\n requirement readable {\n id: 1\n text: Notes remain readable\n risk: low\n verifymethod: test\n }", "readable"),
            ("kanban\n todo[To do]\n   reading[Read]\n done[Done]\n   saved[Saved]", "Saved"),
            ("packet-beta\n 0-7: \"Version\"\n 8-15: \"Count\"", "Version"),
            ("radar-beta\n axis Clarity, Depth, Coverage\n curve Draft {3,4,2}", "Clarity"),
            ("treemap-beta\n \"Research\"\n   \"Books\": 3\n   \"Papers\": 5", "Books"),
            ("C4Context\n Person(reader, \"Reader\")\n System(notes, \"Notes\")\n Rel(reader, notes, \"Reads\")", "Reader"),
            ("zenuml\n Reader->Writer: Review\n Writer-->Reader: Accepted", "Accepted"),
        ];
        for (source, label) in diagrams {
            let result = prepare_document(&format!("```mermaid\n{source}\n```"));
            assert_eq!(result.images.len(), 1, "{source}\n{}", result.markdown);
            let image = result.images.values().next().unwrap();
            let svg = std::str::from_utf8(&image.bytes).unwrap();
            let document = roxmltree::Document::parse(svg).unwrap();
            let labels: String = document
                .descendants()
                .filter_map(|node| node.is_text().then(|| node.text()).flatten())
                .collect();
            assert!(
                labels.contains(label),
                "Missing {label:?} in {source}\n{svg}"
            );
            let raster = cx.update(|cx| {
                futures::executor::block_on(NativeImageDecoder::load(image.clone(), cx)).unwrap()
            });
            assert!(raster.size(0).width.0 > 10, "{source}");
            assert!(raster.size(0).height.0 > 10, "{source}");
            assert!(
                raster
                    .as_bytes(0)
                    .unwrap()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| { pixel[3] > 0 && pixel[..3] != [0x43, 0x3b, 0x11] }),
                "Empty diagram: {source}"
            );
        }
    }

    #[gpui::test]
    fn native_partial_mouse_selection_copies_only_the_selected_text(cx: &mut gpui::TestAppContext) {
        let (_directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        let source =
            "Select a few words from this sentence.\n\nKeep the second paragraph separate.";
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["answer"].content = source.into();
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            });
        });
        cx.run_until_parked();
        let bounds = cx.debug_bounds("rich-answer").expect("rich answer");
        let start = bounds.origin + gpui::point(gpui::px(1.), gpui::px(10.));
        let end = start + gpui::point(gpui::px(85.), gpui::px(0.));
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
            .expect("selected text");
        assert!(!copied.is_empty());
        assert!(source.starts_with(&copied));
        assert!(copied.len() < source.find('\n').unwrap());
    }

    #[gpui::test]
    fn native_markdown_link_dispatches_the_safe_url_to_the_platform(cx: &mut gpui::TestAppContext) {
        let (_directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["answer"].content =
                "[External reference](https://example.org/native-fixture)".into();
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            })
        });
        cx.run_until_parked();
        let bounds = cx.debug_bounds("rich-answer").unwrap();
        cx.simulate_click(
            bounds.origin + gpui::point(gpui::px(20.), gpui::px(10.)),
            gpui::Modifiers::default(),
        );
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.org/native-fixture")
        );
    }

    #[gpui::test]
    fn native_gfm_selection_contains_rendered_blocks_and_the_end_of_wide_code(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        cx.simulate_resize(gpui::size(gpui::px(1440.), gpui::px(2200.)));
        let source = format!("# Heading\n\nA paragraph with *emphasis*, **strong**, ~~removed~~, and `inline_code`.\n\n> Quoted words.\n\n- Bullet alpha\n- Bullet beta\n\n1. Ordered one\n2. Ordered two\n\n| Column one | Column two |\n| --- | --- |\n| Value A | Value B |\n\n- [x] Task done\n- [ ] Task waiting\n\nhttps://example.org/auto\n\n```rust\n    let text = \"{} END_MARKER\";\n```", "wide_code_".repeat(35));
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["answer"].content = source.clone()
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            })
        });
        cx.run_until_parked();
        let bounds = cx.debug_bounds("rich-answer").unwrap();
        let panel = cx.debug_bounds("side-panel").unwrap();
        assert!(bounds.right() <= panel.right());
        let start = bounds.origin + gpui::point(gpui::px(1.), gpui::px(8.));
        let end = bounds.bottom_right() - gpui::point(gpui::px(1.), gpui::px(1.));
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
        for text in [
            "Heading",
            "emphasis",
            "strong",
            "removed",
            "inline_code",
            "Quoted words.",
            "Bullet alpha",
            "Bullet beta",
            "Ordered one",
            "Ordered two",
            "Column one",
            "Column two",
            "Value A",
            "Value B",
            "Task done",
            "Task waiting",
            "https://example.org/auto",
            "    let text",
            "END_MARKER",
        ] {
            assert!(copied.contains(text), "Missing {text:?} in {copied:?}");
        }
        for markup in [
            "# Heading",
            "*emphasis*",
            "**strong**",
            "~~removed~~",
            "`inline_code`",
            "```rust",
        ] {
            assert!(
                !copied.contains(markup),
                "Unrendered markup {markup:?} in {copied:?}"
            );
        }
        assert_ne!(copied, source);
        let short = source.replace(&"wide_code_".repeat(35), "short");
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["answer"].content = short
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            })
        });
        cx.run_until_parked();
        let short_bounds = cx.debug_bounds("rich-answer").unwrap();
        assert!(
            bounds.size.height > short_bounds.size.height + gpui::px(40.),
            "Wide code must wrap visibly: long={bounds:?}, short={short_bounds:?}"
        );
    }

    #[gpui::test]
    fn remote_markdown_image_loads_through_http_and_the_native_image_decoder(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::io::{Read, Write};
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(12, 8, image::Rgb([17, 59, 67]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let bytes = png.into_inner();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/figure.png", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0 && request.len() < 8192);
                request.extend_from_slice(&buffer[..count]);
            }
            assert!(request.starts_with(b"GET /figure.png HTTP/1.1\r\n"));
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).unwrap();
            stream.write_all(&bytes).unwrap();
        });
        let image = cx.update(|cx| {
            cx.set_http_client(Arc::new(crate::http_client::NativeHttpClient::new()));
            futures::executor::block_on(RemoteImage::load(url.clone(), cx)).unwrap()
        });
        server.join().unwrap();
        let raster = cx
            .update(|cx| futures::executor::block_on(NativeImageDecoder::load(image, cx)).unwrap());
        assert_eq!((raster.size(0).width.0, raster.size(0).height.0), (12, 8));
        assert!(raster
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [67, 59, 17, 255]));
        let source = format!("![Remote diagram]({url})");
        assert_eq!(prepare_document(&source).markdown, source);
    }
}
