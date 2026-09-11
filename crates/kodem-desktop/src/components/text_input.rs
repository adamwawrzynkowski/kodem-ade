// Adapted from GPUI input example 0.3.4; Apache-2.0.
use std::{ops::Range, time::Instant};

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, SharedString, Style,
    TextRun, UTF16Selection, Window, actions, div, fill, hsla, point, prelude::*, px, relative,
    rgb, rgba, size,
};

use unicode_segmentation::*;

actions!(
    text_input,
    [
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        SelectHome,
        SelectEnd,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteLineStart,
        DeleteLineEnd,
        SelectUp,
        SelectDown,
        Undo,
        Redo,
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Newline,
        Submit,
        Up,
        Down,
        AcceptCompletion,
        DismissCompletion,
    ]
);

/// Raised when a composer requests submission outside an IME composition.
pub struct SubmitInput;

/// Routes completion navigation while the composer retains keyboard focus.
#[derive(Clone, Copy)]
pub enum CompletionInput {
    Previous,
    Next,
    Accept,
    Dismiss,
}

/// Pointer and clipboard operations delegated by a row to its document owner.
#[derive(Clone)]
pub enum SelectionInput {
    Begin { range: Range<usize>, extend: bool },
    Move { byte: usize },
    End,
    Copy,
    SelectAll,
}

#[derive(Clone)]
struct EditState {
    content: SharedString,
    selection: Range<usize>,
    reversed: bool,
}

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Vec<(usize, super::inline_bubbles::TextLine)>,
    line_height: Pixels,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    click_anchor: Option<Range<usize>>,
    undo: Vec<EditState>,
    redo: Vec<EditState>,
    readonly: bool,
    composer: bool,
    completion_open: bool,
    delegated_selection: bool,
    wrap: bool,
    syntax: Option<(String, bool)>,
    colors: Vec<(Range<usize>, [u8; 3])>,
    prose: Option<(Vec<crate::markdown::Span>, crate::theme::Theme)>,
    streaming: super::streaming_text::StreamingText,
}

impl TextInput {
    /// Creates a selectable text input with native IME support.
    pub fn new(placeholder: &str, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.to_owned().into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: Vec::new(),
            line_height: px(22.),
            last_bounds: None,
            is_selecting: false,
            click_anchor: None,
            undo: Vec::new(),
            redo: Vec::new(),
            readonly: false,
            composer: false,
            completion_open: false,
            delegated_selection: false,
            wrap: true,
            syntax: None,
            colors: Vec::new(),
            prose: None,
            streaming: Default::default(),
        }
    }
    /// Creates a prompt editor where Enter submits and Shift-Enter inserts a newline.
    pub fn composer(placeholder: &str, cx: &mut Context<Self>) -> Self {
        let mut input = Self::new(placeholder, cx);
        input.composer = true;
        input
    }
    /// Returns the slash token at a collapsed caret outside an IME composition.
    pub fn slash_query(&self) -> Option<(Range<usize>, String)> {
        if !self.composer || self.marked_range.is_some() || !self.selected_range.is_empty() {
            return None;
        }
        slash_token(&self.content, self.cursor_offset())
    }

    /// Enables completion key routing without changing focus or IME behavior.
    pub fn set_completion_open(&mut self, open: bool) {
        self.completion_open = open;
    }

    /// Replaces a completion token as one undoable edit and keeps surrounding text.
    pub fn complete_slash(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        if self.marked_range.is_some()
            || range.end > self.content.len()
            || !self.content.is_char_boundary(range.start)
            || !self.content.is_char_boundary(range.end)
        {
            return;
        }
        self.remember_edit();
        self.streaming = Default::default();
        self.content =
            (self.content[..range.start].to_owned() + text + &self.content[range.end..]).into();
        self.selected_range = range.start + text.len()..range.start + text.len();
        self.selection_reversed = false;
        cx.notify();
    }

    fn completion_accept(&mut self, _: &AcceptCompletion, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion_open && self.marked_range.is_none() {
            cx.emit(CompletionInput::Accept);
        } else {
            cx.propagate();
        }
    }
    fn completion_dismiss(
        &mut self,
        _: &DismissCompletion,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.completion_open && self.marked_range.is_none() {
            cx.emit(CompletionInput::Dismiss);
        } else {
            cx.propagate();
        }
    }
    /// Lets a document coordinate selection across separately rendered rows.
    pub fn delegate_selection(&mut self) {
        self.delegated_selection = true;
    }
    /// Applies a document-owned byte selection to this row.
    pub fn set_selection(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let range = self
            .content
            .floor_char_boundary(range.start.min(self.content.len()))
            ..self
                .content
                .floor_char_boundary(range.end.min(self.content.len()));
        if self.selected_range != range {
            self.selected_range = range;
            cx.notify();
        }
    }
    /// Maps a pointer to a UTF-8 boundary in this row.
    pub fn byte_at(&self, position: Point<Pixels>) -> usize {
        self.index_for_mouse_position(position)
    }
    /// Returns the last painted text bounds for cross-block pointer selection.
    pub fn text_bounds(&self) -> Option<Bounds<Pixels>> {
        self.last_bounds
    }
    /// Returns the content length in UTF-8 bytes without allocating a copy.
    pub fn text_len(&self) -> usize {
        self.content.len()
    }
    /// Returns the current draft.
    pub fn text(&self) -> String {
        self.content.to_string()
    }
    /// Replaces the draft and resets selection after switching tasks.
    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.undo.clear();
        self.redo.clear();
        self.content = text.into();
        self.prose = None;
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
        self.selection_reversed = false;
        if self.composer {
            self.streaming = Default::default();
        }
        if let Some((extension, light)) = &self.syntax {
            self.colors = crate::syntax::spans(&self.content, extension, *light);
        }
        cx.notify();
    }
    /// Updates streamed text while keeping existing letters and selections in place.
    pub fn set_streamed_text(&mut self, text: String, animate: bool, cx: &mut Context<Self>) {
        if animate || self.streaming.start().is_some() {
            self.streaming.append(&self.content, &text, Instant::now());
        }
        let selection = self.selected_range.clone();
        self.set_text(text, cx);
        self.selected_range = self
            .content
            .floor_char_boundary(selection.start.min(self.content.len()))
            ..self
                .content
                .floor_char_boundary(selection.end.min(self.content.len()));
    }
    /// Builds selectable read-only text without an editing caret.
    pub fn readonly(text: String, cx: &mut Context<Self>) -> Self {
        let mut input = Self::new("", cx);
        input.content = text.into();
        input.readonly = true;
        input
    }
    /// Configures wrapping and syntax colors for a captured code block.
    pub fn code(&mut self, extension: &str, light: bool) {
        self.prose = None;
        self.wrap = false;
        if self
            .syntax
            .as_ref()
            .is_none_or(|(old, old_light)| old != extension || *old_light != light)
        {
            self.syntax = Some((extension.into(), light));
            self.colors = crate::syntax::spans(&self.content, extension, light);
        }
    }
    /// Sets whether text wraps to its available width without changing syntax colors.
    pub fn set_wrap(&mut self, wrap: bool) {
        self.wrap = wrap;
    }
    /// Applies Markdown styles to selectable prose and restores wrapping after a block changes kind.
    pub fn prose(&mut self, spans: Vec<crate::markdown::Span>, theme: crate::theme::Theme) {
        self.wrap = true;
        self.syntax = None;
        self.colors.clear();
        self.prose = Some((spans, theme));
    }
    fn bubble_padding(&self) -> Pixels {
        if self
            .prose
            .as_ref()
            .is_some_and(|(spans, _)| spans.iter().any(|span| span.style.is_bubble()))
        {
            px(crate::theme::INLINE_PADDING)
        } else {
            px(0.)
        }
    }
    fn visual_line_boundary(&self, end: bool) -> usize {
        let byte = self.cursor_offset();
        for (start, line) in &self.last_layout {
            if byte >= *start
                && byte <= start + line.len()
                && let Some(position) = line.position_for_index(byte - start, self.line_height)
            {
                let x = if end {
                    self.last_bounds.map_or(px(100000.), |b| b.size.width)
                } else {
                    px(0.)
                };
                return start
                    + line
                        .closest_index_for_position(point(x, position.y), self.line_height)
                        .unwrap_or_else(|i| i);
            }
        }
        line_boundary(&self.content, byte, end)
    }
    fn vertical(&mut self, direction: f32, extend: bool, cx: &mut Context<Self>) {
        let Some(bounds) = self.last_bounds else {
            return;
        };
        let mut y = px(0.);
        for (start, line) in &self.last_layout {
            if self.cursor_offset() >= *start
                && self.cursor_offset() <= start + line.len()
                && let Some(position) =
                    line.position_for_index(self.cursor_offset() - start, self.line_height)
            {
                let target = point(
                    bounds.left() + position.x,
                    bounds.top() + y + position.y + self.line_height * direction,
                );
                let byte = self.index_for_mouse_position(target);
                if extend {
                    self.select_to(byte, cx);
                } else {
                    self.move_to(byte, cx);
                }
                return;
            }
            y += line.size(self.line_height).height;
        }
    }
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion_open && self.marked_range.is_none() {
            cx.emit(CompletionInput::Previous);
        } else {
            self.vertical(-1., false, cx);
        }
    }
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion_open && self.marked_range.is_none() {
            cx.emit(CompletionInput::Next);
        } else {
            self.vertical(1., false, cx);
        }
    }
    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(
            word_boundary(&self.content, self.cursor_offset(), false),
            cx,
        );
    }
    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(word_boundary(&self.content, self.cursor_offset(), true), cx);
    }
    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(
            word_boundary(&self.content, self.cursor_offset(), false),
            cx,
        );
    }
    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(word_boundary(&self.content, self.cursor_offset(), true), cx);
    }
    fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.visual_line_boundary(false), cx);
    }
    fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.visual_line_boundary(true), cx);
    }
    fn select_line_start(&mut self, _: &SelectLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.visual_line_boundary(false), cx);
    }
    fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.visual_line_boundary(true), cx);
    }
    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }
    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }
    fn delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(
                word_boundary(&self.content, self.cursor_offset(), false),
                cx,
            );
        }
        self.replace_text_in_range(None, "", window, cx);
    }
    fn delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(word_boundary(&self.content, self.cursor_offset(), true), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }
    fn delete_line_start(
        &mut self,
        _: &DeleteLineStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.visual_line_boundary(false), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }
    fn delete_line_end(&mut self, _: &DeleteLineEnd, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.visual_line_boundary(true), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }
    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(-1., true, cx);
    }
    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(1., true, cx);
    }
    fn edit_state(&self) -> EditState {
        EditState {
            content: self.content.clone(),
            selection: self.selected_range.clone(),
            reversed: self.selection_reversed,
        }
    }
    fn remember_edit(&mut self) {
        if self.marked_range.is_some() {
            return;
        }
        if self.undo.len() == 100 {
            self.undo.remove(0);
        }
        self.undo.push(self.edit_state());
        self.redo.clear();
    }
    fn restore_edit(&mut self, state: EditState, cx: &mut Context<Self>) {
        self.streaming = Default::default();
        self.content = state.content;
        self.selected_range = state.selection;
        self.selection_reversed = state.reversed;
        self.marked_range = None;
        cx.notify();
    }
    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        if let Some(state) = self.undo.pop() {
            self.redo.push(self.edit_state());
            self.restore_edit(state, cx);
        }
    }
    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        if let Some(state) = self.redo.pop() {
            self.undo.push(self.edit_state());
            self.restore_edit(state, cx);
        }
    }
    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        if self.composer && self.marked_range.is_none() {
            if self.completion_open {
                cx.emit(CompletionInput::Accept);
            } else {
                cx.emit(SubmitInput);
            }
        }
    }
    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx);
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
        if self.delegated_selection {
            cx.emit(SelectionInput::SelectAll);
            return;
        }
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        if self.readonly
            && event.modifiers.platform
            && let Some((spans, _)) = &self.prose
        {
            let byte = self.index_for_mouse_position(event.position);
            if let Some(url) = spans
                .iter()
                .find(|span| span.range.contains(&byte))
                .and_then(|span| span.style.link.as_deref())
                .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
            {
                cx.open_url(url);
                return;
            }
        }
        let byte = self.index_for_mouse_position(event.position);
        let range = clicked_range(&self.content, byte, event.click_count);
        if self.delegated_selection {
            cx.emit(SelectionInput::Begin {
                range,
                extend: event.modifiers.shift,
            });
            return;
        }
        self.is_selecting = true;
        self.click_anchor = (event.click_count > 1).then_some(range.clone());
        if event.click_count > 1 {
            self.move_to(range.start, cx);
            self.select_to(range.end, cx);
        } else if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.delegated_selection {
            cx.emit(SelectionInput::End);
        }
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.delegated_selection && event.pressed_button == Some(MouseButton::Left) {
            cx.emit(SelectionInput::Move {
                byte: self.index_for_mouse_position(event.position),
            });
            return;
        }
        if self.is_selecting && event.pressed_button == Some(MouseButton::Left) {
            let byte = self.index_for_mouse_position(event.position);
            if let Some(anchor) = self.click_anchor.clone() {
                let word = clicked_range(&self.content, byte, 2);
                if byte < anchor.start {
                    self.move_to(anchor.end, cx);
                    self.select_to(word.start, cx);
                } else {
                    self.move_to(anchor.start, cx);
                    self.select_to(word.end.max(anchor.end), cx);
                }
            } else {
                self.select_to(byte, cx);
            }
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
        if self.readonly {
            return;
        }
        if let Some(content) = cx
            .read_from_clipboard()
            .and_then(|item| crate::clipboard::content(item, self.composer))
        {
            match content {
                crate::clipboard::PasteContent::Text(text) => {
                    self.replace_text_in_range(None, &text, window, cx);
                }
                crate::clipboard::PasteContent::Attachments(attachments) => cx.emit(attachments),
            }
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.delegated_selection {
            cx.emit(SelectionInput::Copy);
            return;
        }
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.delegated_selection {
            cx.emit(SelectionInput::Copy);
            return;
        }
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
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
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        let mut y = position.y - bounds.top();
        for (start, line) in &self.last_layout {
            let height = line.size(self.line_height).height;
            if y < height {
                let index = line
                    .closest_index_for_position(
                        point(position.x - bounds.left(), y.max(px(0.))),
                        self.line_height,
                    )
                    .unwrap_or_else(|i| i);
                return (start + index).min(self.content.len());
            }
            y -= height;
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
        if self.readonly {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.remember_edit();
        self.selection_reversed = false;
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if self.composer && !cx.reduce_motion() {
            self.streaming
                .replace(&self.content, range.clone(), new_text.len(), Instant::now());
        } else {
            self.streaming = Default::default();
        }
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
        if self.readonly {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.remember_edit();
        self.streaming = Default::default();
        self.selection_reversed = false;
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
            .map(|new_range| {
                (new_range.start + range.start).min(self.content.len())
                    ..(new_range.end + range.start).min(self.content.len())
            })
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let mut y = px(0.);
        for (start, line) in &self.last_layout {
            if range.start >= *start && range.start <= start + line.len() {
                let position = line.position_for_index(range.start - start, self.line_height)?;
                return Some(Bounds::new(
                    point(bounds.left() + position.x, bounds.top() + y + position.y),
                    size(px(2.), self.line_height),
                ));
            }
            y += line.size(self.line_height).height;
        }
        None
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse_position(point)))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    lines: Vec<(usize, super::inline_bubbles::TextLine)>,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
}
impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self {
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
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let input = self.input.clone();
        let text_style = window.text_style();
        let height = window.line_height();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        (
            window.request_measured_layout(style, move |known, available, window, cx| {
                let input = input.read(cx);
                let padding = input.bubble_padding();
                let width = known.width.or(match available.width {
                    gpui::AvailableSpace::Definite(w) => Some(w),
                    _ => None,
                });
                let lines = shape(
                    input,
                    &text_style,
                    font_size,
                    if input.wrap {
                        width.map(|width| (width - padding * 2.).max(px(1.)))
                    } else {
                        None
                    },
                    window,
                );
                let measured = lines
                    .iter()
                    .map(|(_, l)| l.size(height).height)
                    .fold(px(0.), |a, b| a + b)
                    .max(height);
                let natural_width = || {
                    lines
                        .iter()
                        .map(|(_, line)| line.size(height).width)
                        .fold(px(0.), |a, b| a.max(b))
                        + padding * 2.
                };
                size(
                    width.unwrap_or_else(|| {
                        if input.readonly {
                            natural_width()
                        } else {
                            px(400.)
                        }
                    }),
                    measured,
                )
            }),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
        let input = self.input.read(cx);
        let style = window.text_style();
        let padding = input.bubble_padding();
        let height = window.line_height();
        let bounds = Bounds::new(
            bounds.origin + point(padding, px(0.)),
            size(
                (bounds.size.width - padding * 2.).max(px(1.)),
                bounds.size.height,
            ),
        );
        let lines = shape(
            input,
            &style,
            style.font_size.to_pixels(window.rem_size()),
            if input.wrap {
                Some(bounds.size.width)
            } else {
                None
            },
            window,
        );
        let mut selections = Vec::new();
        let mut cursor = None;
        let mut y = px(0.);
        for (start, line) in &lines {
            let selected = &input.selected_range;
            let a = selected.start.saturating_sub(*start).min(line.len());
            let b = selected.end.saturating_sub(*start).min(line.len());
            if b > a
                && let (Some(first), Some(last)) = (
                    line.position_for_index(a, height),
                    line.position_for_index(b, height),
                )
            {
                let mut row = first.y;
                while row <= last.y {
                    let left = if row == first.y { first.x } else { px(0.) };
                    let right = if row == last.y {
                        last.x
                    } else {
                        bounds.size.width
                    };
                    selections.push(fill(
                        Bounds::from_corners(
                            point(bounds.left() + left, bounds.top() + y + row),
                            point(bounds.left() + right, bounds.top() + y + row + height),
                        ),
                        rgba(0xd9905940),
                    ));
                    row += height;
                }
            }
            if !input.readonly
                && input.focus_handle.is_focused(window)
                && selected.is_empty()
                && input.cursor_offset() >= *start
                && input.cursor_offset() <= start + line.len()
                && let Some(p) = line.position_for_index(input.cursor_offset() - start, height)
            {
                cursor = Some(fill(
                    Bounds::new(
                        point(bounds.left() + p.x, bounds.top() + y + p.y),
                        size(px(1.5), height),
                    ),
                    rgb(0xd99059),
                ));
            }
            y += line.size(height).height;
        }
        PrepaintState {
            lines,
            cursor,
            selections,
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let input = self.input.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == gpui::DispatchPhase::Bubble && input.read(cx).is_selecting {
                input.update(cx, |input, cx| input.on_mouse_move(event, window, cx));
            }
        });
        let focus = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let padding = self.input.read(cx).bubble_padding();
        let height = window.line_height();
        let bounds = Bounds::new(
            bounds.origin + point(padding, px(0.)),
            size(
                (bounds.size.width - padding * 2.).max(px(1.)),
                bounds.size.height,
            ),
        );
        let visible = window.content_mask().bounds;
        if let Some((_, theme)) = self.input.read(cx).prose.as_ref() {
            let theme = *theme;
            let mut y = px(0.);
            for (_, line) in &prepaint.lines {
                let line_bounds =
                    Bounds::new(point(bounds.left(), bounds.top() + y), line.size(height));
                if !line_bounds.dilate(height).intersects(&visible) {
                    y += line.size(height).height;
                    continue;
                }
                line.paint_bubbles(
                    point(bounds.left(), bounds.top() + y),
                    height,
                    theme,
                    window,
                );
                y += line.size(height).height;
            }
        }
        for selection in prepaint.selections.drain(..) {
            window.paint_quad(selection);
        }
        let mut y = px(0.);
        let now = Instant::now();
        for (_, line) in &prepaint.lines {
            let line_bounds =
                Bounds::new(point(bounds.left(), bounds.top() + y), line.size(height));
            if !line_bounds.dilate(height).intersects(&visible) {
                y += line.size(height).height;
                continue;
            }
            let _ = line.paint(
                point(bounds.left(), bounds.top() + y),
                height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
            let glyphs = line.streaming_glyphs(
                point(bounds.left(), bounds.top() + y),
                height,
                &self.input.read(cx).streaming,
                now,
                visible,
            );
            super::streaming_text::paint_glyphs(glyphs, window);
            y += line.size(height).height;
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = std::mem::take(&mut prepaint.lines);
            input.last_bounds = Some(bounds);
            input.line_height = height;
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if cx.reduce_motion() {
            self.streaming = Default::default();
        } else {
            self.streaming.prune(Instant::now());
            if self.streaming.start().is_some() {
                window.request_animation_frame();
            }
        }
        div()
            .flex()
            .key_context(if self.composer {
                "TextInput Composer"
            } else {
                "TextInput"
            })
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::select_line_start))
            .on_action(cx.listener(Self::select_line_end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::delete_line_start))
            .on_action(cx.listener(Self::delete_line_end))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::completion_accept))
            .on_action(cx.listener(Self::completion_dismiss))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .line_height(relative(1.5))
            .child(
                super::InputBase::new("input-frame")
                    .min_h(px(if self.readonly { 18. } else { 24. }))
                    .w_full()
                    .p(px(if self.readonly { 0. } else { 4. }))
                    .child(TextElement { input: cx.entity() }),
            )
    }
}

impl gpui::EventEmitter<SubmitInput> for TextInput {}
impl gpui::EventEmitter<CompletionInput> for TextInput {}
impl gpui::EventEmitter<crate::clipboard::PastedAttachments> for TextInput {}
impl gpui::EventEmitter<SelectionInput> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Registers standard macOS editing bindings for every native text input.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("shift-home", SelectHome, Some("TextInput")),
        KeyBinding::new("shift-end", SelectEnd, Some("TextInput")),
        KeyBinding::new("ctrl-a", LineStart, Some("TextInput")),
        KeyBinding::new("ctrl-e", LineEnd, Some("TextInput")),
        KeyBinding::new("ctrl-b", Left, Some("TextInput")),
        KeyBinding::new("ctrl-f", Right, Some("TextInput")),
        KeyBinding::new("ctrl-p", Up, Some("TextInput")),
        KeyBinding::new("ctrl-n", Down, Some("TextInput")),
        KeyBinding::new("ctrl-h", Backspace, Some("TextInput")),
        KeyBinding::new("ctrl-d", Delete, Some("TextInput")),
        KeyBinding::new("ctrl-k", DeleteLineEnd, Some("TextInput")),
        KeyBinding::new("alt-left", WordLeft, Some("TextInput")),
        KeyBinding::new("alt-right", WordRight, Some("TextInput")),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some("TextInput")),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some("TextInput")),
        KeyBinding::new("cmd-left", LineStart, Some("TextInput")),
        KeyBinding::new("cmd-right", LineEnd, Some("TextInput")),
        KeyBinding::new("cmd-shift-left", SelectLineStart, Some("TextInput")),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, Some("TextInput")),
        KeyBinding::new("cmd-shift-up", SelectHome, Some("TextInput")),
        KeyBinding::new("cmd-shift-down", SelectEnd, Some("TextInput")),
        KeyBinding::new("alt-backspace", DeleteWordLeft, Some("TextInput")),
        KeyBinding::new("alt-delete", DeleteWordRight, Some("TextInput")),
        KeyBinding::new("cmd-backspace", DeleteLineStart, Some("TextInput")),
        KeyBinding::new("cmd-delete", DeleteLineEnd, Some("TextInput")),
        KeyBinding::new("shift-up", SelectUp, Some("TextInput")),
        KeyBinding::new("shift-down", SelectDown, Some("TextInput")),
        KeyBinding::new("cmd-z", Undo, Some("TextInput")),
        KeyBinding::new("cmd-shift-z", Redo, Some("TextInput")),
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("DiffDocument")),
        KeyBinding::new("cmd-c", Copy, Some("DiffDocument")),
        KeyBinding::new("cmd-up", Home, Some("TextInput")),
        KeyBinding::new("cmd-down", End, Some("TextInput")),
        KeyBinding::new("cmd-v", Paste, Some("TextInput")),
        KeyBinding::new("cmd-c", Copy, Some("TextInput")),
        KeyBinding::new("cmd-x", Cut, Some("TextInput")),
        KeyBinding::new("enter", Newline, Some("TextInput")),
        KeyBinding::new("shift-enter", Newline, Some("TextInput")),
        KeyBinding::new("enter", Submit, Some("Composer")),
        KeyBinding::new("cmd-enter", Submit, Some("Composer")),
        KeyBinding::new("tab", AcceptCompletion, Some("Composer")),
        KeyBinding::new("escape", DismissCompletion, Some("Composer")),
        KeyBinding::new("up", Up, Some("TextInput")),
        KeyBinding::new("down", Down, Some("TextInput")),
    ]);
}

fn shape(
    input: &TextInput,
    style: &gpui::TextStyle,
    font_size: Pixels,
    width: Option<Pixels>,
    window: &mut Window,
) -> Vec<(usize, super::inline_bubbles::TextLine)> {
    let text = if input.content.is_empty() {
        input.placeholder.clone()
    } else {
        input.content.clone()
    };
    let run = TextRun {
        len: text.len(),
        font: style.font(),
        color: if input.content.is_empty() {
            hsla(0., 0., 0.62, 1.)
        } else {
            style.color
        },
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing: None,
    };
    let runs: Vec<TextRun> = if let Some((spans, theme)) = &input.prose {
        let theme = *theme;
        spans
            .iter()
            .map(|span| {
                let mut styled = run.clone();
                styled.len = span.range.len();
                if span.style.bold {
                    styled.font.weight = gpui::FontWeight::BOLD;
                }
                if span.style.italic {
                    styled.font.style = gpui::FontStyle::Italic;
                }
                if span.style.code {
                    styled.font.family = "JetBrains Mono".into();
                }
                if span.style.code || span.style.reference {
                    styled.color = theme.accent;
                }
                if span.style.strike {
                    styled.strikethrough = Some(gpui::StrikethroughStyle {
                        thickness: px(1.),
                        color: None,
                    });
                }
                if span.style.link.is_some() {
                    styled.color = theme.accent;
                    styled.underline = Some(gpui::UnderlineStyle {
                        thickness: px(1.),
                        color: None,
                        wavy: false,
                    });
                }
                styled
            })
            .collect()
    } else if input.colors.is_empty() {
        vec![run.clone()]
    } else {
        input
            .colors
            .iter()
            .map(|(range, color)| TextRun {
                len: range.len(),
                color: hsla_from_rgb(*color),
                ..run.clone()
            })
            .collect()
    };
    let paint_runs = std::sync::Arc::new(if input.streaming.start().is_some() {
        runs.clone()
    } else {
        Vec::new()
    });
    let runs = if input.streaming.start().is_some() {
        let mut offset = 0;
        runs.into_iter()
            .flat_map(|run| {
                let start = offset;
                let end = start + run.len;
                offset = end;
                let mut segments = Vec::new();
                let mut cursor = start;
                for range in input
                    .streaming
                    .ranges()
                    .filter(|range| range.end > start && range.start < end)
                {
                    let low = range.start.max(start);
                    let high = range.end.min(end);
                    if cursor < low {
                        segments.push(TextRun {
                            len: low - cursor,
                            ..run.clone()
                        });
                    }
                    let mut hidden = run.clone();
                    hidden.len = high - low;
                    hidden.color.alpha = 0.;
                    hidden.underline = None;
                    hidden.strikethrough = None;
                    segments.push(hidden);
                    cursor = high;
                }
                if cursor < end {
                    segments.push(TextRun {
                        len: end - cursor,
                        ..run
                    });
                }
                segments
            })
            .collect()
    } else {
        runs
    };
    let mut bubbles: Vec<Range<usize>> = Vec::new();
    if let Some((spans, _)) = &input.prose {
        for span in spans.iter().filter(|span| span.style.is_bubble()) {
            if let Some(last) = bubbles.last_mut()
                && last.end == span.range.start
            {
                last.end = span.range.end;
            } else {
                bubbles.push(span.range.clone());
            }
        }
    }
    let (text, runs, map) = if bubbles.is_empty() {
        (
            text,
            runs,
            std::sync::Arc::new(super::inline_bubbles::PaddingMap::default()),
        )
    } else {
        let mut spacer = TextRun {
            len: '\u{a0}'.len_utf8(),
            ..run
        };
        let space_width = window
            .text_system()
            .shape_line("\u{a0}".into(), font_size, &[spacer.clone()], None)
            .width();
        spacer.letter_spacing = Some(px(crate::theme::INLINE_PADDING) - space_width);
        let (text, runs, map) = super::inline_bubbles::pad_text(&text, runs, &bubbles, spacer);
        (text.into(), runs, map)
    };
    let mut start = 0;
    window
        .text_system()
        .shape_text(text, font_size, &runs, width, None)
        .unwrap_or_default()
        .into_iter()
        .map(|line| {
            let display_start = start;
            let offset = map.source_index(start);
            start += line.len() + 1;
            (
                offset,
                super::inline_bubbles::TextLine::new(
                    line,
                    map.clone(),
                    display_start,
                    paint_runs.clone(),
                ),
            )
        })
        .collect()
}
fn hsla_from_rgb(color: [u8; 3]) -> gpui::Hsla {
    gpui::rgb_to_hsla(rgb(((color[0] as u32) << 16)
        | ((color[1] as u32) << 8)
        | color[2] as u32))
}

fn word_boundary(text: &str, byte: usize, forward: bool) -> usize {
    if forward {
        text.unicode_word_indices()
            .map(|(start, word)| start + word.len())
            .find(|end| *end > byte)
            .unwrap_or(text.len())
    } else {
        text.unicode_word_indices()
            .map(|(start, _)| start)
            .take_while(|start| *start < byte)
            .last()
            .unwrap_or(0)
    }
}

fn line_boundary(text: &str, byte: usize, end: bool) -> usize {
    if end {
        text[byte..]
            .find('\n')
            .map_or(text.len(), |offset| byte + offset)
    } else {
        text[..byte].rfind('\n').map_or(0, |offset| offset + 1)
    }
}

fn clicked_range(text: &str, byte: usize, count: usize) -> Range<usize> {
    if count >= 3 {
        let end = line_boundary(text, byte, true);
        return line_boundary(text, byte, false)..(end + usize::from(end < text.len()));
    }
    if count == 2 {
        return text
            .split_word_bound_indices()
            .find_map(|(start, word)| {
                let end = start + word.len();
                (start <= byte && (byte < end || end == text.len())).then_some(start..end)
            })
            .unwrap_or(byte..byte);
    }
    byte..byte
}

// UTF-8 offsets are supplied by the native editor, never by a bytewise cursor increment.
fn slash_token(text: &str, end: usize) -> Option<(Range<usize>, String)> {
    let prefix = text.get(..end)?;
    let start = prefix
        .rfind(char::is_whitespace)
        .map_or(0, |i| i + text[i..].chars().next().unwrap().len_utf8());
    let query = text[start..end].strip_prefix('/')?;
    if query.contains('/') || query.contains('\\') {
        return None;
    }
    Some((start..end, query.into()))
}
