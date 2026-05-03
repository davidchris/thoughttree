//! Multiline text input lifted from `crates/gpui/examples/input.rs` at the
//! pinned Zed rev, then extended for multiline + word wrap. GPUI exposes
//! the `EntityInputHandler` trait + the `ElementInputHandler<V>` paint-time
//! adapter, but no ready-to-use `TextInput` component — every consumer
//! (Zed itself included) implements its own.
//!
//! Multiline differences from the upstream single-line example:
//! - Uses `text_system().shape_text` (returns `Vec<WrappedLine>`) instead of
//!   `shape_line`, with `bounds.size.width` as the wrap width.
//! - Stores per-frame `Vec<WrappedLine>`; cursor / mouse / selection walk
//!   logical lines via `WrappedLineLayout::position_for_index` and
//!   `closest_index_for_position`.
//! - `Enter` action inserts `\n`; paste no longer scrubs newlines.
//! - `request_layout` reads a height cached from the previous prepaint —
//!   first frame is one-line tall and self-corrects on the next tick.
//!
//! Caveats:
//! - No vertical scroll; editor grows with content.
//! - Selection rectangles are rendered per visual row but assume the row
//!   spans the full editor width — sub-row trailing whitespace selection
//!   may look wider than upstream Zed's editor.
//! - No undo/redo, no word-wise navigation.
//! - IME marked-text underline + macOS character palette still work.

use std::ops::Range;

use gpui::{
    actions, black, div, fill, hsla, point, prelude::*, px, relative, rgb, rgba, size, white,
    yellow, App, AvailableSpace, Bounds, ClipboardItem, Context, CursorStyle, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, SharedString, Size, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle,
    Window, WrappedLine,
};
use unicode_segmentation::*;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Enter,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
    ]
);

/// Register the keybindings the input listens for. Call once at app startup,
/// before opening the window.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        KeyBinding::new("enter", Enter, None),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
    ]);
}

pub struct TextInput {
    pub focus_handle: FocusHandle,
    pub content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<Vec<WrappedLine>>,
    last_bounds: Option<Bounds<Pixels>>,
    last_line_height: Pixels,
    is_selecting: bool,
}

impl TextInput {
    pub fn new(initial: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let content: SharedString = initial.into();
        let end = content.len();
        Self {
            focus_handle: cx.focus_handle(),
            content,
            placeholder: "Type…".into(),
            selected_range: end..end,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            last_line_height: px(0.0),
            is_selecting: false,
        }
    }

    pub fn text(&self) -> String {
        self.content.to_string()
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let prev = self.previous_boundary(self.cursor_offset());
            if self.cursor_offset() == prev {
                window.play_system_bell();
                return;
            }
            self.select_to(prev, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next {
                window.play_system_bell();
                return;
            }
            self.select_to(next, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(lines)) =
            (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        let line_height = self.last_line_height;
        if line_height <= px(0.0) {
            return 0;
        }
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }

        let local_x = position.x - bounds.left();
        let mut line_top = bounds.top();
        let mut byte_offset = 0usize;
        for (i, line) in lines.iter().enumerate() {
            let line_height_total = line.size(line_height).height;
            let line_bottom = line_top + line_height_total;
            // Y inside this logical line block, OR last line catches overflow.
            if position.y < line_bottom || i == lines.len() - 1 {
                let local = point(local_x, position.y - line_top);
                let local_index = match line.closest_index_for_position(local, line_height) {
                    Ok(i) => i,
                    Err(i) => i,
                };
                return byte_offset + local_index;
            }
            byte_offset += line.len() + 1; // +1 for the '\n' separator between logical lines
            line_top = line_bottom;
        }
        self.content.len()
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let lines = self.last_layout.as_ref()?;
        let line_height = self.last_line_height;
        if line_height <= px(0.0) {
            return None;
        }
        let range = self.range_from_utf16(&range_utf16);
        // Locate start; clamp end to start's logical line for v1.
        let mut byte_offset = 0usize;
        let mut line_top = bounds.top();
        for line in lines.iter() {
            let line_len = line.len();
            if range.start <= byte_offset + line_len {
                let local_start = range.start - byte_offset;
                let local_end = range.end.saturating_sub(byte_offset).min(line_len);
                let start_pt = line.position_for_index(local_start, line_height)?;
                let end_pt = line.position_for_index(local_end, line_height)?;
                return Some(Bounds::from_corners(
                    point(bounds.left() + start_pt.x, line_top + start_pt.y),
                    point(
                        bounds.left() + end_pt.x,
                        line_top + end_pt.y + line_height,
                    ),
                ));
            }
            byte_offset += line_len + 1;
            line_top += line.size(line_height).height;
        }
        None
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        // The selected_text_range / IME path uses byte index → utf16 conversion;
        // here we just round-trip through index_for_mouse_position.
        let utf8_index = self.index_for_mouse_position(point);
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    lines: Option<Vec<WrappedLine>>,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
    line_height: Pixels,
}

/// Walk wrapped lines, locating the logical line that contains `offset`.
/// Returns `(line_index, local_offset)` where `local_offset` is the utf-8
/// byte offset within that line. The trailing `\n` between logical lines
/// belongs to the *previous* line for cursor placement.
fn locate(lines: &[WrappedLine], offset: usize) -> (usize, usize) {
    let mut consumed = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let line_len = line.len();
        if offset <= consumed + line_len {
            return (i, offset - consumed);
        }
        consumed += line_len + 1;
    }
    let last = lines.len().saturating_sub(1);
    let local = lines.last().map(|l| l.len()).unwrap_or(0);
    (last, local)
}

/// Absolute (x, y) of a byte offset relative to the bounds origin.
fn offset_to_point(
    lines: &[WrappedLine],
    offset: usize,
    line_height: Pixels,
) -> Option<Point<Pixels>> {
    let (line_idx, local) = locate(lines, offset);
    let mut y = px(0.0);
    for line in lines.iter().take(line_idx) {
        y += line.size(line_height).height;
    }
    let line = lines.get(line_idx)?;
    let local_pt = line.position_for_index(local, line_height)?;
    Some(point(local_pt.x, y + local_pt.y))
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let line_height = window.line_height();
        let style_text = window.text_style();
        let font_size = style_text.font_size.to_pixels(window.rem_size());
        let font = style_text.font();
        let color = style_text.color;
        let input = self.input.clone();

        let mut style = Style::default();
        style.size.width = relative(1.).into();

        let layout_id =
            window.request_measured_layout(style, move |known, avail, window, cx| {
                let wrap_width: Pixels = match known.width {
                    Some(w) => w,
                    None => match avail.width {
                        AvailableSpace::Definite(w) => w,
                        _ => {
                            return Size {
                                width: px(0.0),
                                height: line_height,
                            };
                        }
                    },
                };
                let input_ref = input.read(cx);
                let display_text: SharedString = if input_ref.content.is_empty() {
                    input_ref.placeholder.clone()
                } else {
                    input_ref.content.clone()
                };
                let run = TextRun {
                    len: display_text.len(),
                    font: font.clone(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let lines = window
                    .text_system()
                    .shape_text(
                        display_text,
                        font_size,
                        &[run],
                        Some(wrap_width.max(px(20.0))),
                        None,
                    )
                    .unwrap();
                let total: Pixels = lines.iter().map(|l| l.size(line_height).height).sum();
                Size {
                    width: wrap_width,
                    height: total.max(line_height),
                }
            });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();
        let line_height = window.line_height();

        let (display_text, text_color, is_placeholder) = if content.is_empty() {
            (input.placeholder.clone(), hsla(0., 0., 0.6, 0.6), true)
        } else {
            (content, style.color, false)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if !is_placeholder
            && let Some(marked_range) = input.marked_range.as_ref()
        {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let wrap_width = bounds.size.width.max(px(20.0));
        let lines: Vec<WrappedLine> = window
            .text_system()
            .shape_text(display_text, font_size, &runs, Some(wrap_width), None)
            .unwrap()
            .into_iter()
            .collect();

        // Cursor (placeholder layout: never show cursor inside placeholder text).
        let cursor_quad = if is_placeholder {
            Some(fill(
                Bounds::new(
                    point(bounds.left(), bounds.top()),
                    size(px(2.), line_height),
                ),
                crate::theme::ACCENT,
            ))
        } else {
            offset_to_point(&lines, cursor, line_height).map(|p| {
                fill(
                    Bounds::new(
                        point(bounds.left() + p.x, bounds.top() + p.y),
                        size(px(2.), line_height),
                    ),
                    crate::theme::ACCENT,
                )
            })
        };

        // Selection: walk logical lines, emit per-visual-row rects clamped
        // to each line's actual rendered width (so trailing whitespace on
        // short logical lines doesn't get highlighted).
        let mut selections: Vec<PaintQuad> = Vec::new();
        if !is_placeholder && !selected_range.is_empty() {
            let (start_line, _) = locate(&lines, selected_range.start);
            let (end_line, _) = locate(&lines, selected_range.end);
            let mut byte_offset = 0usize;
            let mut line_top = px(0.0);
            for (i, line) in lines.iter().enumerate() {
                let line_len = line.len();
                let line_height_total = line.size(line_height).height;
                if i < start_line {
                    byte_offset += line_len + 1;
                    line_top += line_height_total;
                    continue;
                }
                if i > end_line {
                    break;
                }
                let s_local = if i == start_line {
                    selected_range.start - byte_offset
                } else {
                    0
                };
                let e_local = if i == end_line {
                    selected_range.end - byte_offset
                } else {
                    line_len
                };
                let line_text_right = line.size(line_height).width;
                if let (Some(s_pt), Some(e_pt)) = (
                    line.position_for_index(s_local, line_height),
                    line.position_for_index(e_local, line_height),
                ) {
                    if (s_pt.y - e_pt.y).abs() < px(0.5) {
                        selections.push(fill(
                            Bounds::from_corners(
                                point(bounds.left() + s_pt.x, bounds.top() + line_top + s_pt.y),
                                point(
                                    bounds.left() + e_pt.x,
                                    bounds.top() + line_top + s_pt.y + line_height,
                                ),
                            ),
                            rgba(0x3311ff30),
                        ));
                    } else {
                        // First visual row of this logical line: from s_pt.x
                        // to the line's actual rendered right edge.
                        selections.push(fill(
                            Bounds::from_corners(
                                point(bounds.left() + s_pt.x, bounds.top() + line_top + s_pt.y),
                                point(
                                    bounds.left() + line_text_right,
                                    bounds.top() + line_top + s_pt.y + line_height,
                                ),
                            ),
                            rgba(0x3311ff30),
                        ));
                        // Middle visual rows of this logical line.
                        let mut y = s_pt.y + line_height;
                        while y + px(0.5) < e_pt.y {
                            selections.push(fill(
                                Bounds::from_corners(
                                    point(bounds.left(), bounds.top() + line_top + y),
                                    point(
                                        bounds.left() + line_text_right,
                                        bounds.top() + line_top + y + line_height,
                                    ),
                                ),
                                rgba(0x3311ff30),
                            ));
                            y += line_height;
                        }
                        // Last visual row of this logical line.
                        selections.push(fill(
                            Bounds::from_corners(
                                point(bounds.left(), bounds.top() + line_top + e_pt.y),
                                point(
                                    bounds.left() + e_pt.x,
                                    bounds.top() + line_top + e_pt.y + line_height,
                                ),
                            ),
                            rgba(0x3311ff30),
                        ));
                    }
                }
                byte_offset += line_len + 1;
                line_top += line_height_total;
            }
        }

        PrepaintState {
            lines: Some(lines),
            cursor: cursor_quad,
            selections,
            line_height,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for selection in prepaint.selections.drain(..) {
            window.paint_quad(selection);
        }
        let line_height = prepaint.line_height;
        let lines = prepaint.lines.take().unwrap();
        let mut y = bounds.top();
        for line in lines.iter() {
            line.paint(
                point(bounds.left(), y),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .unwrap();
            y += line.size(line_height).height;
        }

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(lines);
            input.last_bounds = Some(bounds);
            input.last_line_height = line_height;
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Suppress unused warnings for items only consumed via key/action wiring.
        let _ = (black, white, yellow, rgb);
        div()
            .flex()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .bg(crate::theme::BG_APP)
            .border_1()
            .border_color(crate::theme::ACCENT)
            .rounded(px(4.0))
            .px(px(8.0))
            .py(px(6.0))
            .text_color(crate::theme::TEXT)
            .text_size(px(13.0))
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
