// Adapted from GPUI input example 0.3.4; Apache-2.0.
use std::{ops::Range, rc::Rc, time::Instant};

use gpui::{
    App, Bounds, ClipboardItem, ColorExt, ContentMask, Context, CursorStyle, Div, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, Font, GlobalElementId,
    Hsla, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PaintQuad, Pixels, Point, SharedString, Style, TextRun, UTF16Selection, Window, actions, div,
    fill, point, prelude::*, px, relative, rgb, size,
};

use unicode_segmentation::*;

/// Time between redraws while streamed text settles into place.
const REVEAL_FRAME: std::time::Duration = std::time::Duration::from_millis(28);

/// Size in bytes, and number of lines, from which a text is long. A long text that is pasted
/// into a composer that folds it becomes a card, and a long message of the user shows folded.
const LONG_TEXT_BYTES: usize = 4000;
const LONG_TEXT_LINES: usize = 60;

/// Separates the typed draft from each folded text in the composed message.
const FOLD_SEPARATOR: &str = "\n\n";

/// Compositions with folded texts that the composer remembers, so that a draft that leaves the
/// composer and comes back keeps its cards.
const REMEMBERED_FOLDS: usize = 8;

/// Reports whether a text is long enough to show folded instead of in full.
pub fn is_long_text(text: &str) -> bool {
    text.len() >= LONG_TEXT_BYTES
        || text
            .bytes()
            .filter(|byte| *byte == b'\n')
            .nth(LONG_TEXT_LINES - 1)
            .is_some()
}

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
        SubmitAlternate,
        Up,
        Down,
        AcceptCompletion,
        DismissCompletion,
    ]
);

/// Raised when a composer requests submission outside an IME composition.
pub struct SubmitInput {
    /// The user asked for the delivery that is not the configured default.
    pub alternate: bool,
}

/// Raised when the user clicks a file or symbol reference in read-only prose.
pub struct ReferenceInput(pub String);

/// Routes completion navigation while the composer retains keyboard focus.
#[derive(Clone, Copy)]
pub enum CompletionInput {
    Previous,
    Next,
    Accept,
    Dismiss,
}

/// Edits that an editable row of a document reports to the document that owns its lines.
#[derive(Clone, Copy)]
pub enum RowEdit {
    /// The row's text changed, and may hold line breaks that the document splits off.
    Changed,
    /// Backspace at the start of the row: the row joins the line above.
    JoinPrevious,
    /// Delete at the end of the row: the line below joins the row.
    JoinNext,
    /// Up on the first visual line, with the caret's byte offset.
    Previous(usize),
    /// Down on the last visual line, with the caret's byte offset.
    Next(usize),
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
    folded: Vec<SharedString>,
    selection: Range<usize>,
    reversed: bool,
}

/// The lines of one shaping, and what they were shaped from. Layout measures a text more than
/// once a frame and the window draws many frames of the same text, so the lines are reused
/// until the text, its styles, the font or the width change.
struct Shaped {
    content: SharedString,
    revision: u64,
    width: Option<Pixels>,
    font_size: Pixels,
    font: Font,
    color: Hsla,
    lines: Rc<Vec<(usize, super::inline_bubbles::TextLine)>>,
}

/// Shapings kept at once: the unwrapped one that layout measures, and the wrapped one.
const SHAPINGS: usize = 2;

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Rc<Vec<(usize, super::inline_bubbles::TextLine)>>,
    line_height: Pixels,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    click_anchor: Option<Range<usize>>,
    // Where a single press landed in read-only prose, to tell a click from a drag selection.
    press: Option<(Point<Pixels>, usize)>,
    /// The link or reference under the pointer in read-only prose, which a click opens.
    hovered: Option<Range<usize>>,
    undo: Vec<EditState>,
    redo: Vec<EditState>,
    readonly: bool,
    composer: bool,
    /// One line of text: Return submits, and pasted line breaks become spaces.
    single_line: bool,
    /// Draws each character as an asterisk and keeps the text off the clipboard.
    masked: bool,
    completion_open: bool,
    delegated_selection: bool,
    /// One line of an editable document: line joins, moves past the text and undo go to the
    /// document through `RowEdit` events and propagated actions.
    row_edits: bool,
    wrap: bool,
    /// How far the text of one line that does not wrap is scrolled to the left, so that its
    /// caret stays in view.
    scroll_x: Pixels,
    syntax: Option<(String, kodem_core::ThemeId)>,
    colors: Vec<(Range<usize>, [u8; 3])>,
    /// The highlighter's state for `colors`, so streamed code costs the lines it adds.
    highlighted: crate::syntax::Highlighted,
    prose: Option<(Vec<crate::markdown::Span>, crate::theme::Theme)>,
    /// Shows the inline code of the typed text as its message will, in the plates of this theme.
    live_code: Option<crate::theme::Theme>,
    streaming: super::streaming_text::StreamingText,
    /// Whether a redraw for the streamed-text reveal is already scheduled.
    reveal_pending: bool,
    /// Recent shapings of the text, newest first.
    shaped: Vec<Shaped>,
    /// Changes when a style that shaping depends on changes, beside the text itself.
    revision: u64,
    /// Whether a long pasted text becomes a card instead of draft text.
    folds_long_text: bool,
    /// Long texts that are part of the message but show as cards beside the draft. They follow
    /// the typed text, each after a blank line.
    folded: Vec<SharedString>,
    /// Recent compositions with folded texts: the whole text, the typed text and the folds.
    remembered: Vec<(String, SharedString, Vec<SharedString>)>,
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
            last_layout: Rc::default(),
            line_height: px(22.),
            last_bounds: None,
            is_selecting: false,
            click_anchor: None,
            press: None,
            hovered: None,
            undo: Vec::new(),
            redo: Vec::new(),
            readonly: false,
            composer: false,
            single_line: false,
            masked: false,
            completion_open: false,
            delegated_selection: false,
            row_edits: false,
            wrap: true,
            scroll_x: px(0.),
            syntax: None,
            colors: Vec::new(),
            highlighted: Default::default(),
            prose: None,
            live_code: None,
            streaming: Default::default(),
            reveal_pending: false,
            shaped: Vec::new(),
            revision: 0,
            folds_long_text: false,
            folded: Vec::new(),
            remembered: Vec::new(),
        }
    }
    /// Creates a prompt editor where Enter submits and Shift-Enter inserts a newline.
    pub fn composer(placeholder: &str, cx: &mut Context<Self>) -> Self {
        let mut input = Self::new(placeholder, cx);
        input.composer = true;
        input
    }
    /// Shows each span of the typed text between backticks as inline code while the user types,
    /// in the code plates of `theme`. The backticks stay in the text and show faint.
    pub fn set_live_code(&mut self, theme: Option<crate::theme::Theme>) {
        if self.live_code != theme {
            self.live_code = theme;
            self.revision += 1;
        }
    }
    /// Makes the input hold one line: Return submits it, and pasted line breaks become spaces.
    pub fn set_single_line(&mut self, single_line: bool) {
        self.single_line = single_line;
    }
    /// Hides the text behind asterisks and keeps it off the clipboard, for secrets.
    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        if self.masked != masked {
            self.masked = masked;
            self.revision += 1;
            cx.notify();
        }
    }
    /// Replaces the hint that shows while the input is empty.
    pub fn set_placeholder(&mut self, placeholder: &str, cx: &mut Context<Self>) {
        if self.placeholder.as_ref() != placeholder {
            self.placeholder = placeholder.to_owned().into();
            self.revision += 1;
            cx.notify();
        }
    }
    /// Makes a long pasted text a card beside the draft instead of draft text.
    pub fn set_folds_long_text(&mut self, folds: bool) {
        self.folds_long_text = folds;
    }
    /// Returns the long texts that show as cards beside the draft.
    pub fn folded(&self) -> &[SharedString] {
        &self.folded
    }
    /// Moves a folded text into the draft, after the typed text, as one undoable edit. The
    /// composed text keeps the same words.
    pub fn unfold(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.folded.len() || self.marked_range.is_some() {
            return;
        }
        self.remember_edit();
        let text = self.folded.remove(index);
        let mut content = self.content.to_string();
        if !content.is_empty() {
            content.push_str(FOLD_SEPARATOR);
        }
        content.push_str(&text);
        self.streaming = Default::default();
        self.content = content.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        cx.notify();
    }
    /// Removes a folded text from the message as one undoable edit.
    pub fn remove_folded(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.folded.len() {
            return;
        }
        self.remember_edit();
        self.folded.remove(index);
        cx.notify();
    }
    /// Returns the slash token at a collapsed caret outside an IME composition.
    pub fn slash_query(&self) -> Option<(Range<usize>, String)> {
        if !self.composer || self.marked_range.is_some() || !self.selected_range.is_empty() {
            return None;
        }
        slash_token(&self.content, self.cursor_offset())
    }

    /// Returns the `@` file mention at a collapsed caret outside an IME composition.
    pub fn mention_query(&self) -> Option<(Range<usize>, String)> {
        if !self.composer || self.marked_range.is_some() || !self.selected_range.is_empty() {
            return None;
        }
        mention_token(&self.content, self.cursor_offset())
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
    /// Makes this input one editable line of a document that owns undo and the line breaks.
    pub fn delegate_row_edits(&mut self) {
        self.row_edits = true;
    }
    /// Returns the byte offset of the caret.
    pub fn cursor(&self) -> usize {
        self.cursor_offset()
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
    /// Returns the current draft, with its folded texts after the typed text.
    pub fn text(&self) -> String {
        compose(&self.content, &self.folded)
    }
    /// Reports whether the draft and its folded texts are empty, without composing them.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty() && self.folded.is_empty()
    }
    /// Reports whether the draft and its folded texts hold only white space.
    pub fn is_blank(&self) -> bool {
        self.content.trim().is_empty() && self.folded.iter().all(|text| text.trim().is_empty())
    }
    /// Reports whether the shown text is `text`, without copying it.
    pub fn shows(&self, text: &str) -> bool {
        self.content.as_ref() == text
    }
    /// Replaces the draft and resets selection after switching tasks.
    ///
    /// In a composer that folds long text, a draft that left with folded texts comes back
    /// with them, and another long draft comes back folded.
    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.undo.clear();
        self.redo.clear();
        if self.folds_long_text {
            if !self.folded.is_empty() {
                let composed = self.text();
                self.remembered.retain(|(text, _, _)| *text != composed);
                self.remembered.insert(
                    0,
                    (
                        composed,
                        self.content.clone(),
                        std::mem::take(&mut self.folded),
                    ),
                );
                self.remembered.truncate(REMEMBERED_FOLDS);
            }
            if let Some((_, typed, folded)) = self
                .remembered
                .iter()
                .find(|(composed, _, _)| *composed == text)
            {
                self.content = typed.clone();
                self.folded = folded.clone();
            } else if is_long_text(&text) {
                self.content = SharedString::default();
                self.folded = vec![text.into()];
            } else {
                self.content = text.into();
            }
        } else {
            self.content = text.into();
        }
        self.prose = None;
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
        self.selection_reversed = false;
        if self.composer {
            self.streaming = Default::default();
        }
        if let Some((extension, theme)) = &self.syntax {
            self.colors = self.highlighted.update(&self.content, extension, *theme);
        }
        self.revision += 1;
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
    /// Configures wrapping and syntax colors for a captured code block, in the code colors
    /// of `theme`.
    pub fn code(&mut self, extension: &str, theme: kodem_core::ThemeId) {
        if self.prose.take().is_some() {
            self.revision += 1;
        }
        self.wrap = false;
        if self
            .syntax
            .as_ref()
            .is_none_or(|(old, old_theme)| old != extension || *old_theme != theme)
        {
            self.syntax = Some((extension.into(), theme));
            self.colors = self.highlighted.update(&self.content, extension, theme);
            self.revision += 1;
        }
    }
    /// Sets whether text wraps to its available width without changing syntax colors. A
    /// single-line input that does not wrap scrolls sideways to keep its caret in view.
    pub fn set_wrap(&mut self, wrap: bool) {
        self.wrap = wrap;
    }
    // Scrolls a single line that does not wrap so that the caret stays inside `width`, and
    // returns the distance. An unfocused field shows the start of its text.
    fn follow_caret(
        &mut self,
        lines: &[(usize, super::inline_bubbles::TextLine)],
        width: Pixels,
        height: Pixels,
        focused: bool,
    ) -> Pixels {
        let Some((_, line)) = lines.first().filter(|_| focused) else {
            self.scroll_x = px(0.);
            return self.scroll_x;
        };
        // The caret is two pixels wide and must show at the end of the text too.
        let room = (width - px(2.)).max(px(0.));
        let caret = line
            .position_for_index(self.cursor_offset().min(line.len()), height)
            .map_or(px(0.), |position| position.x);
        if caret < self.scroll_x {
            self.scroll_x = caret;
        } else if caret > self.scroll_x + room {
            self.scroll_x = caret - room;
        }
        let overflow = (line.size(height).width - room).max(px(0.));
        self.scroll_x = self.scroll_x.min(overflow).max(px(0.));
        self.scroll_x
    }
    /// Applies Markdown styles to selectable prose and restores wrapping after a block changes kind.
    pub fn prose(&mut self, spans: &[crate::markdown::Span], theme: crate::theme::Theme) {
        self.wrap = true;
        if self.syntax.is_none()
            && self.colors.is_empty()
            && self
                .prose
                .as_ref()
                .is_some_and(|(old, old_theme)| old.as_slice() == spans && *old_theme == theme)
        {
            return;
        }
        self.syntax = None;
        self.colors.clear();
        self.highlighted = Default::default();
        self.prose = Some((spans.to_vec(), theme));
        self.revision += 1;
    }
    fn visual_line_boundary(&self, end: bool) -> usize {
        let byte = self.cursor_offset();
        for (start, line) in self.last_layout.iter() {
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
        for (start, line) in self.last_layout.iter() {
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
    // Reports whether the caret is on the last visual line of the text, or on the first one.
    fn caret_on_edge(&self, last: bool) -> bool {
        let cursor = self.cursor_offset();
        let Some((index, (start, line))) = self
            .last_layout
            .iter()
            .enumerate()
            .find(|(_, (start, line))| cursor >= *start && cursor <= start + line.len())
        else {
            return true;
        };
        let Some(position) = line.position_for_index(cursor - start, self.line_height) else {
            return true;
        };
        if last {
            index + 1 == self.last_layout.len()
                && position.y + self.line_height >= line.size(self.line_height).height
        } else {
            index == 0 && position.y < self.line_height
        }
    }
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion_open && self.marked_range.is_none() {
            cx.emit(CompletionInput::Previous);
        } else if self.row_edits && self.caret_on_edge(false) {
            cx.emit(RowEdit::Previous(self.cursor_offset()));
        } else {
            self.vertical(-1., false, cx);
        }
    }
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion_open && self.marked_range.is_none() {
            cx.emit(CompletionInput::Next);
        } else if self.row_edits && self.caret_on_edge(true) {
            cx.emit(RowEdit::Next(self.cursor_offset()));
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
            folded: self.folded.clone(),
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
        self.folded = state.folded;
        self.selected_range = state.selection;
        self.selection_reversed = state.reversed;
        self.marked_range = None;
        cx.notify();
    }
    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        // A document row and read-only text leave undo to the document around them.
        if self.row_edits || self.readonly {
            cx.propagate();
            return;
        }
        if let Some(state) = self.undo.pop() {
            self.redo.push(self.edit_state());
            self.restore_edit(state, cx);
        }
    }
    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        // A document row and read-only text leave undo to the document around them.
        if self.row_edits || self.readonly {
            cx.propagate();
            return;
        }
        if let Some(state) = self.redo.pop() {
            self.undo.push(self.edit_state());
            self.restore_edit(state, cx);
        }
    }
    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        if (self.composer || self.single_line) && self.marked_range.is_none() {
            if self.completion_open {
                cx.emit(CompletionInput::Accept);
            } else {
                cx.emit(SubmitInput { alternate: false });
            }
        }
    }
    fn submit_alternate(&mut self, _: &SubmitAlternate, _: &mut Window, cx: &mut Context<Self>) {
        if (self.composer || self.single_line) && self.marked_range.is_none() {
            if self.completion_open {
                cx.emit(CompletionInput::Accept);
            } else {
                cx.emit(SubmitInput { alternate: true });
            }
        }
    }
    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.single_line {
            return;
        }
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
            if self.cursor_offset() == prev && self.row_edits && !self.readonly {
                cx.emit(RowEdit::JoinPrevious);
                return;
            }
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
            if self.cursor_offset() == next && self.row_edits && !self.readonly {
                cx.emit(RowEdit::JoinNext);
                return;
            }
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
        let byte = self.index_for_mouse_position(event.position);
        self.press = (self.readonly && self.prose.is_some() && event.click_count == 1)
            .then_some((event.position, byte));
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

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.delegated_selection {
            cx.emit(SelectionInput::End);
        }
        self.is_selecting = false;
        // A press that ends where it began is a click: it follows the link or reference under it.
        let Some((origin, byte)) = self.press.take() else {
            return;
        };
        let moved = event.position - origin;
        if moved.x.abs() > px(3.) || moved.y.abs() > px(3.) {
            return;
        }
        let Some(span) = self.prose.as_ref().and_then(|(spans, _)| {
            spans
                .iter()
                .find(|span| span.range.contains(&byte))
                .cloned()
        }) else {
            return;
        };
        if let Some(url) = span.style.link.as_deref() {
            if ["https://", "http://", "mailto:"]
                .iter()
                .any(|scheme| url.starts_with(scheme))
            {
                cx.open_url(url);
            } else if !url.is_empty() {
                cx.emit(ReferenceInput(url.to_owned()));
            }
        } else if span.style.reference
            && let Some(text) = self.content.get(span.range.clone())
        {
            cx.emit(ReferenceInput(text.to_owned()));
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.hover(event, cx);
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

    // Follows the link or reference under the pointer, which shows that a click opens it.
    fn hover(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let hovered = if event.pressed_button.is_none() {
            self.clickable_at(event.position)
        } else {
            None
        };
        if hovered != self.hovered {
            self.hovered = hovered;
            cx.notify();
        }
    }

    // Finds the link or reference whose letters are under `position` in read-only prose.
    fn clickable_at(&self, position: Point<Pixels>) -> Option<Range<usize>> {
        let (spans, _) = self.prose.as_ref().filter(|_| self.readonly)?;
        let bounds = self.last_bounds?;
        let mut y = position.y - bounds.top();
        if y < px(0.) {
            return None;
        }
        for (start, line) in self.last_layout.iter() {
            let height = line.size(self.line_height).height;
            if y < height {
                let index = line
                    .index_for_position(point(position.x - bounds.left(), y), self.line_height)
                    .ok()?;
                let byte = start + index;
                return spans
                    .iter()
                    .find(|span| {
                        span.range.contains(&byte)
                            && (span.style.reference
                                || span
                                    .style
                                    .link
                                    .as_deref()
                                    .is_some_and(|url| !url.is_empty()))
                    })
                    .map(|span| span.range.clone());
            }
            y -= height;
        }
        None
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
                crate::clipboard::PasteContent::Text(text)
                    if self.folds_long_text && is_long_text(&text) =>
                {
                    self.remember_edit();
                    self.folded.push(text.into());
                    cx.notify();
                }
                crate::clipboard::PasteContent::Text(text) => {
                    self.replace_text_in_range(None, &text, window, cx);
                }
                crate::clipboard::PasteContent::Attachments(attachments) => cx.emit(attachments),
            }
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.masked {
            return;
        }
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
        if self.masked {
            return;
        }
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
        for (start, line) in self.last_layout.iter() {
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

    // Colors edited code again and tells a document that one of its rows changed.
    fn edited(&mut self, cx: &mut Context<Self>) {
        if let Some((extension, theme)) = &self.syntax {
            self.colors = self.highlighted.update(&self.content, extension, *theme);
        }
        if self.row_edits {
            cx.emit(RowEdit::Changed);
        }
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

    // Returns the lines of the text at `width`, shaped again only when the text, a style or
    // the font changed. Streamed text changes its runs on every step of its reveal, so it is
    // shaped each time. An empty field shows its placeholder in the muted color of the theme
    // that the workspace publishes.
    fn lines(
        &mut self,
        style: &gpui::TextStyle,
        font_size: Pixels,
        width: Option<Pixels>,
        window: &mut Window,
        cx: &App,
    ) -> Rc<Vec<(usize, super::inline_bubbles::TextLine)>> {
        let placeholder = cx
            .try_global::<crate::theme::Theme>()
            .copied()
            .unwrap_or_default()
            .muted;
        if self.streaming.start().is_some() {
            return Rc::new(shape(self, style, font_size, width, placeholder, window));
        }
        let font = style.font();
        // The color that the run is drawn in, so a change of theme shapes the placeholder again.
        let color = if self.content.is_empty() {
            placeholder
        } else {
            style.color
        };
        let found = self.shaped.iter().position(|shaped| {
            shaped.revision == self.revision
                && shaped.width == width
                && shaped.font_size == font_size
                && shaped.color == color
                && shaped.font == font
                // The text is immutable and the shaping holds it, so the same address and
                // length are the same text.
                && shaped.content.as_ptr() == self.content.as_ptr()
                && shaped.content.len() == self.content.len()
        });
        if let Some(index) = found {
            let shaped = self.shaped.remove(index);
            let lines = shaped.lines.clone();
            self.shaped.insert(0, shaped);
            return lines;
        }
        let lines = Rc::new(shape(self, style, font_size, width, placeholder, window));
        self.shaped.insert(
            0,
            Shaped {
                content: self.content.clone(),
                revision: self.revision,
                width,
                font_size,
                font,
                color,
                lines: lines.clone(),
            },
        );
        self.shaped.truncate(SHAPINGS);
        lines
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

        let flattened;
        let new_text = if self.single_line && new_text.contains(['\r', '\n']) {
            flattened = new_text.replace("\r\n", " ").replace(['\r', '\n'], " ");
            flattened.as_str()
        } else {
            new_text
        };
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
        self.edited(cx);
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

        self.edited(cx);
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
        for (start, line) in self.last_layout.iter() {
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
    lines: Rc<Vec<(usize, super::inline_bubbles::TextLine)>>,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
    /// How far a single line that does not wrap is scrolled to the left, or `None` for text
    /// that is not clipped to its bounds.
    scroll: Option<Pixels>,
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
                let width = known.width.or(match available.width {
                    gpui::AvailableSpace::Definite(w) => Some(w),
                    _ => None,
                });
                let lines = input.update(cx, |input, cx| {
                    let width = if input.wrap { width } else { None };
                    input.lines(&text_style, font_size, width, window, cx)
                });
                let input = input.read(cx);
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
                };
                size(
                    width.unwrap_or_else(|| {
                        if input.readonly || input.row_edits {
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
        // Caret and selection follow the app accent published by the workspace.
        let accent = cx.try_global::<crate::theme::Theme>().map_or_else(
            || crate::theme::Theme::default().accent,
            |theme| theme.accent,
        );
        let style = window.text_style();
        let height = window.line_height();
        let lines = self.input.update(cx, |input, cx| {
            let width = input.wrap.then_some(bounds.size.width);
            input.lines(
                &style,
                style.font_size.to_pixels(window.rem_size()),
                width,
                window,
                cx,
            )
        });
        let focused = self.input.read(cx).focus_handle.is_focused(window);
        let scroll = self.input.update(cx, |input, _| {
            (input.single_line && !input.wrap)
                .then(|| input.follow_caret(&lines, bounds.size.width, height, focused))
        });
        let bounds = Bounds::new(
            point(bounds.left() - scroll.unwrap_or_default(), bounds.top()),
            bounds.size,
        );
        let input = self.input.read(cx);
        let mut selections = Vec::new();
        let mut cursor = None;
        let mut y = px(0.);
        for (start, line) in lines.iter() {
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
                        accent.opacity(0.28),
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
                        size(px(2.), height),
                    ),
                    accent,
                ));
            }
            y += line.size(height).height;
        }
        PrepaintState {
            lines,
            cursor,
            selections,
            scroll,
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
        // A drag that started in this text keeps selecting outside it. A press starts the drag
        // and redraws, so the listener exists only while a selection is being made.
        if self.input.read(cx).is_selecting {
            let input = self.input.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == gpui::DispatchPhase::Bubble && input.read(cx).is_selecting {
                    input.update(cx, |input, cx| input.on_mouse_move(event, window, cx));
                }
            });
        }
        // The pointer can leave the text without a move inside it, so a hovered link listens
        // to the window until the pointer is off it.
        if self.input.read(cx).hovered.is_some() {
            let input = self.input.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase == gpui::DispatchPhase::Bubble && input.read(cx).hovered.is_some() {
                    input.update(cx, |input, cx| input.hover(event, cx));
                }
            });
        }
        let clip = bounds;
        // The text of a scrolled line starts left of the field, and the field clips it.
        let bounds = Bounds::new(
            point(
                bounds.left() - prepaint.scroll.unwrap_or_default(),
                bounds.top(),
            ),
            bounds.size,
        );
        let focus = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if prepaint.scroll.is_some() {
            window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                self.paint_text(bounds, prepaint, window, cx);
            });
        } else {
            self.paint_text(bounds, prepaint, window, cx);
        }
    }
}

impl TextElement {
    // Paints the selections, the lines and the caret, with the text starting at `bounds`.
    fn paint_text(
        &mut self,
        bounds: Bounds<Pixels>,
        prepaint: &mut PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let height = window.line_height();
        let visible = window.content_mask().bounds;
        let input = self.input.read(cx);
        let bubble_theme = input
            .prose
            .as_ref()
            .map(|(_, theme)| *theme)
            .or(input.live_code);
        if let Some(theme) = bubble_theme {
            let mut y = px(0.);
            for (_, line) in prepaint.lines.iter() {
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
                    input.hovered.as_ref(),
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
        for (_, line) in prepaint.lines.iter() {
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
            let streaming = &self.input.read(cx).streaming;
            let typed = streaming.typed();
            let glyphs = line.streaming_glyphs(
                point(bounds.left(), bounds.top() + y),
                height,
                streaming,
                now,
                visible,
            );
            super::streaming_text::paint_glyphs(glyphs, typed, window);
            y += line.size(height).height;
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = prepaint.lines.clone();
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
                if self.composer {
                    // A typed letter settles in a few frames and must follow the key at once.
                    window.request_animation_frame();
                } else if !self.reveal_pending {
                    // A streamed letter forms out of dither in about a quarter of a second. A
                    // redraw every REVEAL_FRAME gives it about nine steps without rendering the
                    // conversation on every display frame.
                    self.reveal_pending = true;
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(REVEAL_FRAME).await;
                        let _ = this.update(cx, |input, cx| {
                            input.reveal_pending = false;
                            cx.notify();
                        });
                    })
                    .detach();
                }
            }
        }
        // Read-only text takes keys only while it is focused. A transcript holds many of them,
        // and a press focuses one and redraws before a key can reach it.
        let takes_keys = !self.readonly || self.focus_handle.is_focused(window);
        let root = div()
            .flex()
            .key_context(if self.composer {
                "TextInput Composer"
            } else if self.single_line {
                "TextInput SingleLine"
            } else {
                "TextInput"
            })
            .track_focus(&self.focus_handle(cx))
            .cursor(if self.hovered.is_some() {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            });
        let root = if takes_keys {
            Self::key_actions(root, cx)
        } else {
            root
        };
        root.on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .line_height(relative(1.5))
            .child(
                super::InputBase::new("input-frame")
                    .min_h(px(if self.readonly || self.row_edits {
                        18.
                    } else {
                        24.
                    }))
                    .w_full()
                    .p(px(if self.readonly || self.row_edits {
                        0.
                    } else {
                        4.
                    }))
                    .child(TextElement { input: cx.entity() }),
            )
    }
}

impl TextInput {
    // Adds the editing, movement, clipboard and completion actions of a text input.
    fn key_actions(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(Self::word_left))
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
            .on_action(cx.listener(Self::submit_alternate))
            .on_action(cx.listener(Self::completion_accept))
            .on_action(cx.listener(Self::completion_dismiss))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
    }
}

impl gpui::EventEmitter<SubmitInput> for TextInput {}
impl gpui::EventEmitter<ReferenceInput> for TextInput {}
impl gpui::EventEmitter<CompletionInput> for TextInput {}
impl gpui::EventEmitter<crate::clipboard::PastedAttachments> for TextInput {}
impl gpui::EventEmitter<SelectionInput> for TextInput {}
impl gpui::EventEmitter<RowEdit> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Registers the platform's standard editing bindings for every native text input.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("shift-up", SelectUp, Some("TextInput")),
        KeyBinding::new("shift-down", SelectDown, Some("TextInput")),
        KeyBinding::new("secondary-z", Undo, Some("TextInput")),
        KeyBinding::new("secondary-shift-z", Redo, Some("TextInput")),
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("secondary-a", SelectAll, Some("TextInput")),
        KeyBinding::new("secondary-a", SelectAll, Some("DiffDocument")),
        KeyBinding::new("secondary-c", Copy, Some("DiffDocument")),
        KeyBinding::new("secondary-z", Undo, Some("DiffDocument")),
        KeyBinding::new("secondary-shift-z", Redo, Some("DiffDocument")),
        KeyBinding::new("secondary-v", Paste, Some("TextInput")),
        KeyBinding::new("secondary-c", Copy, Some("TextInput")),
        KeyBinding::new("secondary-x", Cut, Some("TextInput")),
        KeyBinding::new("enter", Newline, Some("TextInput")),
        KeyBinding::new("shift-enter", Newline, Some("TextInput")),
        KeyBinding::new("enter", Submit, Some("Composer")),
        KeyBinding::new("secondary-enter", SubmitAlternate, Some("Composer")),
        KeyBinding::new("tab", AcceptCompletion, Some("Composer")),
        KeyBinding::new("escape", DismissCompletion, Some("Composer")),
        KeyBinding::new("enter", Submit, Some("SingleLine")),
        KeyBinding::new("secondary-enter", SubmitAlternate, Some("SingleLine")),
        KeyBinding::new("up", Up, Some("TextInput")),
        KeyBinding::new("down", Down, Some("TextInput")),
    ]);
    platform_bindings(cx);
}

/// Binds macOS's document, line, word and Emacs-style movement keys.
#[cfg(target_os = "macos")]
fn platform_bindings(cx: &mut App) {
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
        KeyBinding::new("cmd-up", Home, Some("TextInput")),
        KeyBinding::new("cmd-down", End, Some("TextInput")),
    ]);
}

/// Binds the Windows and Linux conventions: Home and End act on the line, Ctrl on words and
/// with Home and End on the whole text.
#[cfg(not(target_os = "macos"))]
fn platform_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("home", LineStart, Some("TextInput")),
        KeyBinding::new("end", LineEnd, Some("TextInput")),
        KeyBinding::new("shift-home", SelectLineStart, Some("TextInput")),
        KeyBinding::new("shift-end", SelectLineEnd, Some("TextInput")),
        KeyBinding::new("ctrl-home", Home, Some("TextInput")),
        KeyBinding::new("ctrl-end", End, Some("TextInput")),
        KeyBinding::new("ctrl-shift-home", SelectHome, Some("TextInput")),
        KeyBinding::new("ctrl-shift-end", SelectEnd, Some("TextInput")),
        KeyBinding::new("ctrl-left", WordLeft, Some("TextInput")),
        KeyBinding::new("ctrl-right", WordRight, Some("TextInput")),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("TextInput")),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("TextInput")),
        KeyBinding::new("ctrl-backspace", DeleteWordLeft, Some("TextInput")),
        KeyBinding::new("ctrl-delete", DeleteWordRight, Some("TextInput")),
        KeyBinding::new("ctrl-y", Redo, Some("TextInput")),
        KeyBinding::new("ctrl-y", Redo, Some("DiffDocument")),
    ]);
}

fn shape(
    input: &TextInput,
    style: &gpui::TextStyle,
    font_size: Pixels,
    width: Option<Pixels>,
    placeholder: Hsla,
    window: &mut Window,
) -> Vec<(usize, super::inline_bubbles::TextLine)> {
    let text = if input.content.is_empty() {
        input.placeholder.clone()
    } else if input.masked {
        // One asterisk per byte keeps every offset of the hidden text valid in the drawn one.
        "*".repeat(input.content.len()).into()
    } else {
        input.content.clone()
    };
    let run = TextRun {
        len: text.len(),
        font: style.font(),
        color: if input.content.is_empty() {
            placeholder
        } else {
            style.color
        },
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing: None,
    };
    // A draft's inline code, found again from the text that the shaping is cached for.
    let live = if input.live_code.is_some()
        && input.prose.is_none()
        && !input.content.is_empty()
        && !input.masked
    {
        crate::markdown::draft_code(&input.content)
    } else {
        Vec::new()
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
                    styled.font.family = crate::theme::mono().into();
                }
                // Code reads in the text color; only a reference that opens something is tinted.
                if span.style.reference {
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
    } else if let Some(theme) = input.live_code.filter(|_| !live.is_empty()) {
        // The code reads in the code face and its backticks fade, around unstyled text.
        let mut runs = Vec::new();
        let mut at = 0;
        let faint = TextRun {
            color: theme.subtle,
            ..run.clone()
        };
        let mut code = run.clone();
        code.font.family = crate::theme::mono().into();
        for (whole, inner) in &live {
            push_run(&mut runs, at..whole.start, run.clone());
            push_run(&mut runs, whole.start..inner.start, faint.clone());
            push_run(&mut runs, inner.clone(), code.clone());
            push_run(&mut runs, inner.end..whole.end, faint.clone());
            at = whole.end;
        }
        push_run(&mut runs, at..text.len(), run.clone());
        runs
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
        let revealing = input.streaming.ranges();
        let mut offset = 0;
        runs.into_iter()
            .flat_map(|run| {
                let start = offset;
                let end = start + run.len;
                offset = end;
                let mut segments = Vec::new();
                let mut cursor = start;
                for range in revealing
                    .iter()
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
    // Each bubble with whether a click on it opens a file. Spans that touch share one bubble,
    // which opens something only when all of it does.
    let mut bubbles: Vec<(Range<usize>, bool)> = Vec::new();
    bubbles.extend(live.iter().map(|(_, code)| (code.clone(), false)));
    if let Some((spans, _)) = &input.prose {
        for span in spans.iter().filter(|span| span.style.is_bubble()) {
            if let Some((last, opens)) = bubbles.last_mut()
                && last.end == span.range.start
            {
                last.end = span.range.end;
                *opens &= span.style.reference;
            } else {
                bubbles.push((span.range.clone(), span.style.reference));
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
// Appends a run of `style` for `range` of the text, unless the range is empty.
fn push_run(runs: &mut Vec<TextRun>, range: Range<usize>, style: TextRun) {
    if !range.is_empty() {
        runs.push(TextRun {
            len: range.len(),
            ..style
        });
    }
}
// Joins the typed draft and its folded texts into the text of the message.
fn compose(typed: &str, folded: &[SharedString]) -> String {
    let mut text = typed.to_owned();
    for fold in folded {
        if !text.is_empty() {
            text.push_str(FOLD_SEPARATOR);
        }
        text.push_str(fold);
    }
    text
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
// Finds an `@` token that ends at the caret. An address such as `name@host` is not a mention.
fn mention_token(text: &str, end: usize) -> Option<(Range<usize>, String)> {
    let prefix = text.get(..end)?;
    let start = prefix
        .rfind(char::is_whitespace)
        .map_or(0, |i| i + text[i..].chars().next().unwrap().len_utf8());
    let query = text[start..end].strip_prefix('@')?;
    Some((start..end, query.into()))
}

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
