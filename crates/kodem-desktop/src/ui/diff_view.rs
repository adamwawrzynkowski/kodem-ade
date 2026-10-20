//! Virtualized diff document with shared text selection and snapshot-backed comments. When the
//! file's working-tree text is loaded, every line of it in the diff is editable, and the hunks
//! follow each edit.
use super::file_edit::{Caret, FileEdit};
use super::*;
use crate::components::text_input::{Redo, RowEdit, SelectionInput, TextInput, Undo};
use std::ops::Range;

actions!(diff_view, [SaveEdits]);

/// Width of one character of a diff line at the default code size, in window pixels.
const CELL_WIDTH: f32 = 7.5;

// Returns the width of the longest line at the default code size.
fn text_width(lines: &[DiffLine]) -> f32 {
    lines
        .iter()
        .map(|l| {
            l.text
                .chars()
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0) as f32
        * CELL_WIDTH
}

// Splits the text of an edited row at its line breaks, without a carriage return before a
// break, and finds the caret's part and byte among the parts.
fn split_typed(text: &str, cursor: usize) -> (Vec<String>, Caret) {
    let mut parts = Vec::new();
    let mut caret = Caret { line: 0, byte: 0 };
    let mut start = 0;
    let count = text.split('\n').count();
    for (index, part) in text.split('\n').enumerate() {
        let kept = if index + 1 < count {
            part.strip_suffix('\r').unwrap_or(part)
        } else {
            part
        };
        if cursor >= start && cursor <= start + part.len() {
            caret = Caret {
                line: index,
                byte: (cursor - start).min(kept.len()),
            };
        }
        parts.push(kept.to_owned());
        start += part.len() + 1;
    }
    (parts, caret)
}

// Returns the length of the common start of two texts, at a character boundary.
fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map_or(a.len().min(b.len()), |((index, _), _)| index)
}

// Returns the height of one diff row. It follows the interface scale, and never gets tighter
// than a line of code at the code size.
fn row_height() -> Pixels {
    px(24.).max(code_px(Settings::CODE_FONT_SIZE) * 1.6)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Position {
    row: usize,
    byte: usize,
}
#[derive(Clone)]
struct TextSelection {
    anchor: Position,
    head: Position,
    column: u8,
}
fn selection_bytes(
    lines: &[DiffLine],
    selection: Option<&TextSelection>,
    row: usize,
    column: u8,
) -> Range<usize> {
    let Some(selection) = selection else {
        return 0..0;
    };
    if selection.column != column
        || matches!(
            (column, &lines[row].kind),
            (1, LineKind::Added) | (2, LineKind::Removed)
        )
    {
        return 0..0;
    }
    let (a, b) = if selection.anchor <= selection.head {
        (selection.anchor, selection.head)
    } else {
        (selection.head, selection.anchor)
    };
    if row < a.row || row > b.row {
        return 0..0;
    }
    (if row == a.row { a.byte } else { 0 })..(if row == b.row {
        b.byte
    } else {
        lines[row].text.len()
    })
}
fn selected_text(lines: &[DiffLine], selection: &TextSelection) -> String {
    let a = selection.anchor.row.min(selection.head.row);
    let b = selection.anchor.row.max(selection.head.row);
    (a..=b)
        .filter(|&row| {
            !matches!(
                (selection.column, &lines[row].kind),
                (1, LineKind::Added) | (2, LineKind::Removed)
            )
        })
        .map(|row| {
            lines[row].text[selection_bytes(lines, Some(selection), row, selection.column)]
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Text,
    Gutter,
}
/// A comment ready to attach to the current composer.
pub(super) struct ContextRequested {
    pub snapshot_id: String,
    pub file: usize,
    pub first_row: usize,
    pub last_row: usize,
    pub comment: String,
}
/// Owns one immutable diff document, its selection, and its virtual viewport.
pub(super) struct DiffView {
    snapshot: String,
    file: usize,
    lines: Vec<DiffLine>,
    extension: String,
    split: bool,
    wrap: bool,
    theme: Theme,
    list: ListState,
    horizontal: ScrollHandle,
    text: Option<TextSelection>,
    gutter: Option<(usize, usize)>,
    comment_open: bool,
    pending_comment: bool,
    comment: Entity<TextInput>,
    drag: Option<Drag>,
    pointer: Option<Point<Pixels>>,
    bounds: Option<Bounds<Pixels>>,
    rows: HashMap<(usize, u8), Entity<TextInput>>,
    subscriptions: HashMap<(usize, u8), Subscription>,
    autoscroll_running: bool,
    focus: FocusHandle,
    /// Width of the longest line at the default code size.
    text_width: f32,
    /// Height of the rows that the list was last laid out with.
    row_height: Pixels,
    /// The working-tree file that this view edits instead of showing the snapshot.
    edit: Option<Entity<FileEdit>>,
    /// The snapshot's rows of the file, which comments refer to.
    snapshot_lines: Vec<DiffLine>,
    /// The repository and path of the file.
    file_path: (Option<PathBuf>, Vec<u8>),
    /// An edit that replaces the view's rows in the next frame, which can see where the caret
    /// is.
    attaching: Option<Entity<FileEdit>>,
    /// In edit mode, the id of each row's line of the edited text.
    line_ids: Vec<Option<u64>>,
    /// In edit mode, the editors of the lines in view, by line id.
    editors: HashMap<u64, (Entity<TextInput>, Subscription)>,
    /// The line, by id, and byte that the caret moves to in the next frame.
    pending_focus: Option<(u64, usize)>,
}
impl EventEmitter<ContextRequested> for DiffView {}
impl DiffView {
    /// Creates a virtual document without constructing inputs for off-screen rows. With
    /// `edit`, its rows are the hunks of that edited working-tree text.
    pub(super) fn new(
        snapshot: &DiffSnapshot,
        file: usize,
        split: bool,
        wrap: bool,
        theme: Theme,
        edit: Option<Entity<FileEdit>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let delta = &snapshot.files[file];
        let (lines, line_ids) = match &edit {
            Some(edit) => edit.update(cx, |edit, _| edit.rows()),
            None => (delta.lines.clone(), Vec::new()),
        };
        let comment = cx.new(|cx| TextInput::new("Add context for the agent…", cx));
        Self {
            snapshot: snapshot.id.clone(),
            file,
            text_width: text_width(&lines),
            list: ListState::new(lines.len(), ListAlignment::Top, px(240.))
                .with_uniform_item_height(row_height()),
            lines,
            extension: delta.label.rsplit('.').next().unwrap_or("").into(),
            split,
            wrap,
            theme,
            horizontal: ScrollHandle::new(),
            text: None,
            gutter: None,
            comment_open: false,
            pending_comment: false,
            comment,
            drag: None,
            pointer: None,
            bounds: None,
            rows: HashMap::new(),
            subscriptions: HashMap::new(),
            autoscroll_running: false,
            focus: cx.focus_handle(),
            row_height: row_height(),
            edit,
            snapshot_lines: delta.lines.clone(),
            file_path: (delta.repository.clone(), delta.path.clone()),
            attaching: None,
            line_ids,
            editors: HashMap::new(),
            pending_focus: None,
        }
    }
    /// Reports whether this view shows the immutable file, or edits with the given edit.
    pub(super) fn shows(
        &self,
        snapshot: &str,
        file: usize,
        edit: Option<&Entity<FileEdit>>,
    ) -> bool {
        match (edit, &self.edit) {
            (Some(edit), _) if self.attaching.as_ref() == Some(edit) => true,
            (Some(edit), Some(own)) => edit == own,
            (None, None) => self.snapshot == snapshot && self.file == file,
            _ => false,
        }
    }
    /// Reports whether the view shows a file at the path of `file`.
    pub(super) fn same_file(&self, file: &FileDelta) -> bool {
        self.file_path.0 == file.repository && self.file_path.1 == file.path
    }
    /// Makes the view edit with `edit` from the next frame, in place: the scroll position
    /// stays, and a caret in the file stays on its line.
    pub(super) fn attach(&mut self, edit: Entity<FileEdit>, cx: &mut Context<Self>) {
        self.attaching = Some(edit);
        cx.notify();
    }
    /// Points comments at a newer capture of the same file, whose rows the view keeps while
    /// it edits.
    pub(super) fn retarget(&mut self, snapshot: &DiffSnapshot, file: usize) {
        if self.edit.is_some() && (self.snapshot != snapshot.id || self.file != file) {
            self.snapshot = snapshot.id.clone();
            self.file = file;
            self.snapshot_lines = snapshot.files[file].lines.clone();
        }
    }
    // Switches to the edit that `attach` gave, and returns the line of the new text and the
    // byte where the caret was: in a focused line editor, or in a selected line of the diff.
    fn finish_attach(&mut self, window: &Window, cx: &mut Context<Self>) {
        // A comment being written keeps the rows that it refers to.
        if self.comment_open || self.pending_comment {
            return;
        }
        let Some(edit) = self.attaching.take() else {
            return;
        };
        let caret = match &self.edit {
            Some(old) => self.editors.iter().find_map(|(id, (input, _))| {
                let input = input.read(cx);
                if !input.focus_handle(cx).is_focused(window) {
                    return None;
                }
                Some((old.read(cx).index_of(*id)?, input.cursor()))
            }),
            None => self
                .text
                .as_ref()
                .filter(|selection| self.focus.is_focused(window) && selection.column != 1)
                .and_then(|selection| {
                    let line = self.lines.get(selection.head.row)?.new?;
                    Some((line as usize - 1, selection.head.byte))
                }),
        };
        self.edit = Some(edit.clone());
        self.text = None;
        self.gutter = None;
        self.drag = None;
        self.rows.clear();
        self.subscriptions.clear();
        self.editors.clear();
        self.rebuild(cx);
        if let Some((line, byte)) = caret {
            let edit = edit.read(cx);
            self.pending_focus = Some((edit.id(line.min(edit.line_count() - 1)), byte));
        }
    }
    /// Counts the added and removed lines that the view shows.
    pub(super) fn totals(&self) -> (usize, usize) {
        self.lines
            .iter()
            .fold((0, 0), |(added, removed), line| match line.kind {
                LineKind::Added => (added + 1, removed),
                LineKind::Removed => (added, removed + 1),
                _ => (added, removed),
            })
    }
    /// Undoes the last edit and moves the caret to where it was before it.
    pub(super) fn undo(&mut self, cx: &mut Context<Self>) {
        self.restore(FileEdit::undo, cx);
    }
    /// Applies the last undone edit again.
    pub(super) fn redo(&mut self, cx: &mut Context<Self>) {
        self.restore(FileEdit::redo, cx);
    }
    /// Restores the text on disk as one edit that can be undone.
    pub(super) fn revert(&mut self, cx: &mut Context<Self>) {
        self.restore(|edit| Some(edit.revert()), cx);
    }
    // Applies an undo, redo or revert to the edited text, shows the new text in the editors in
    // view and moves the caret.
    fn restore(
        &mut self,
        change: impl FnOnce(&mut FileEdit) -> Option<Caret>,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.edit.clone() else {
            return;
        };
        let Some(caret) = edit.update(cx, |edit, _| change(edit)) else {
            return;
        };
        let updates: Vec<(Entity<TextInput>, String)> = {
            let edit = edit.read(cx);
            let texts: HashMap<u64, &str> = (0..edit.line_count())
                .map(|line| (edit.id(line), edit.text(line)))
                .collect();
            self.pending_focus = Some((edit.id(caret.line.min(edit.line_count() - 1)), caret.byte));
            self.editors
                .iter()
                .filter_map(|(id, (input, _))| {
                    let text = texts.get(id)?;
                    (!input.read(cx).shows(text)).then(|| (input.clone(), (*text).to_owned()))
                })
                .collect()
        };
        for (input, text) in updates {
            input.update(cx, |input, cx| input.set_text(text, cx));
        }
        self.rebuild(cx);
    }
    // Reads the rows of the edited text again after an edit. An open comment closes, because
    // its rows moved.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.edit.clone() else {
            return;
        };
        if !self.pending_comment {
            self.gutter = None;
            self.comment_open = false;
        }
        let (lines, line_ids) = edit.update(cx, |edit, _| edit.rows());
        self.text_width = text_width(&lines);
        self.lines = lines;
        self.line_ids = line_ids;
        self.reset_list();
        cx.notify();
    }
    // Applies an edit that a line's editor reports to the edited text.
    fn row_edit(
        &mut self,
        id: u64,
        input: Entity<TextInput>,
        event: RowEdit,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.edit.clone() else {
            return;
        };
        let Some(line) = edit.read(cx).index_of(id) else {
            return;
        };
        let current = edit.read(cx).text(line).to_owned();
        match event {
            RowEdit::Changed => {
                let (text, cursor) = {
                    let input = input.read(cx);
                    (input.text(), input.cursor())
                };
                if text == current {
                    return;
                }
                let before = Caret {
                    line,
                    byte: common_prefix(&current, &text),
                };
                if text.contains('\n') {
                    let (parts, caret) = split_typed(&text, cursor);
                    let first = parts[0].clone();
                    let caret = Caret {
                        line: line + caret.line,
                        byte: caret.byte,
                    };
                    edit.update(cx, |edit, _| edit.replace(line, 1, parts, before, caret));
                    input.update(cx, |input, cx| input.set_text(first, cx));
                    self.pending_focus = Some((edit.read(cx).id(caret.line), caret.byte));
                } else {
                    let after = Caret { line, byte: cursor };
                    edit.update(cx, |edit, _| {
                        edit.replace(line, 1, vec![text], before, after)
                    });
                }
            }
            RowEdit::JoinPrevious => {
                if line == 0 {
                    return;
                }
                let (above, above_id) = {
                    let edit = edit.read(cx);
                    (edit.text(line - 1).to_owned(), edit.id(line - 1))
                };
                let byte = above.len();
                let joined = above + &current;
                let (before, after) = (
                    Caret { line, byte: 0 },
                    Caret {
                        line: line - 1,
                        byte,
                    },
                );
                edit.update(cx, |edit, _| {
                    edit.replace(line - 1, 2, vec![joined.clone()], before, after)
                });
                if let Some((editor, _)) = self.editors.get(&above_id) {
                    editor.update(cx, |editor, cx| editor.set_text(joined, cx));
                }
                self.pending_focus = Some((above_id, byte));
            }
            RowEdit::JoinNext => {
                if line + 1 >= edit.read(cx).line_count() {
                    return;
                }
                let joined = current.clone() + edit.read(cx).text(line + 1);
                let caret = Caret {
                    line,
                    byte: current.len(),
                };
                edit.update(cx, |edit, _| {
                    edit.replace(line, 2, vec![joined.clone()], caret, caret)
                });
                input.update(cx, |input, cx| input.set_text(joined, cx));
                self.pending_focus = Some((id, current.len()));
            }
            RowEdit::Previous(byte) | RowEdit::Next(byte) => {
                // The caret moves to the editable row above or below, across hunk headers and
                // removed lines.
                let Some(row) = self.line_ids.iter().position(|line| *line == Some(id)) else {
                    return;
                };
                let target = if matches!(event, RowEdit::Previous(_)) {
                    self.line_ids[..row].iter().rev().find_map(|line| *line)
                } else {
                    self.line_ids[row + 1..].iter().find_map(|line| *line)
                };
                if let Some(target) = target {
                    self.pending_focus = Some((target, byte));
                    cx.notify();
                }
                return;
            }
        }
        self.rebuild(cx);
    }
    // Returns the editor of a line of the edited text, made when it first comes into view.
    fn editor(&mut self, id: u64, cx: &mut Context<Self>) -> Option<Entity<TextInput>> {
        if let Some((input, _)) = self.editors.get(&id) {
            return Some(input.clone());
        }
        let text = {
            let edit = self.edit.as_ref()?.read(cx);
            edit.text(edit.index_of(id)?).to_owned()
        };
        let (extension, theme) = (self.extension.clone(), self.theme.id);
        let input = cx.new(|cx| {
            let mut input = TextInput::new("", cx);
            input.delegate_row_edits();
            input.set_text(text, cx);
            input.code(&extension, theme);
            input
        });
        let subscription = cx.subscribe(&input, move |view, input, event: &RowEdit, cx| {
            view.row_edit(id, input, *event, cx)
        });
        self.editors.insert(id, (input.clone(), subscription));
        Some(input)
    }
    // Returns the read-only text of an old line or a hunk header in edit mode. An old line is
    // kept by its old line number, which edits do not change; a header by its row, and made
    // again when its text changed.
    fn old_input(&mut self, row: usize, column: u8, cx: &mut Context<Self>) -> Entity<TextInput> {
        let line = &self.lines[row];
        let key = match line.old {
            Some(old) => (old as usize, column),
            None => (usize::MAX - row, column),
        };
        if let Some(input) = self.rows.get(&key)
            && input.read(cx).shows(&line.text)
        {
            return input.clone();
        }
        let text = self.lines[row].text.clone();
        let input = cx.new(|cx| {
            let mut input = TextInput::readonly(text, cx);
            input.code(&self.extension, self.theme.id);
            input
        });
        self.rows.insert(key, input.clone());
        input
    }
    /// Applies presentation preferences while preserving document selection and scroll.
    pub(super) fn configure(
        &mut self,
        split: bool,
        wrap: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) {
        let layout_changed = self.split != split || self.wrap != wrap;
        let changed = layout_changed || self.theme != theme;
        // A new interface scale or code size changes the height of every row.
        let row_height = row_height();
        if self.row_height != row_height {
            self.row_height = row_height;
            self.reset_list();
        }
        if self.split != split {
            self.text = None;
            self.drag = None;
        }
        self.split = split;
        self.wrap = wrap;
        self.theme = theme;
        if layout_changed {
            self.list.remeasure();
            self.horizontal.set_offset(point(px(0.), px(0.)));
        }
        if changed {
            self.sync_selection(cx);
        }
    }
    /// Enables retry after the host rejects a context attachment.
    pub(super) fn context_failed(&mut self, cx: &mut Context<Self>) {
        self.pending_comment = false;
        cx.notify();
    }
    /// Reports whether a refresh must preserve the selected immutable snapshot.
    pub(super) fn has_selection(&self) -> bool {
        self.text
            .as_ref()
            .is_some_and(|selection| selection.anchor != selection.head)
            || self.gutter.is_some()
    }
    /// Clears a document selection while retaining its viewport and captured content.
    pub(super) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if self.pending_comment {
            return;
        }
        self.text = None;
        self.gutter = None;
        self.drag = None;
        self.comment_open = false;
        self.reset_list();
        self.sync_selection(cx);
    }
    fn range(&self) -> Option<(usize, usize)> {
        self.gutter.map(|(a, b)| (a.min(b), a.max(b)))
    }
    // Returns the selected rows as rows of the snapshot, which a comment refers to. Without
    // unsaved edits, the edited text's rows are the snapshot's rows by their line numbers.
    fn comment_rows(&self) -> Option<(usize, usize)> {
        let (first, last) = self.range()?;
        if self.edit.is_none() {
            return Some((first, last));
        }
        let snapshot_row = |row: &DiffLine| {
            self.snapshot_lines.iter().position(|line| {
                line.kind == row.kind && line.old == row.old && line.new == row.new
            })
        };
        let rows = &self.lines[first..=last];
        let first = rows
            .iter()
            .filter(|row| row.kind != LineKind::Header)
            .find_map(snapshot_row)?;
        let last = rows
            .iter()
            .rev()
            .filter(|row| row.kind != LineKind::Header)
            .find_map(snapshot_row)?;
        (first <= last).then_some((first, last))
    }
    fn comment_index(&self) -> Option<usize> {
        self.comment_open
            .then(|| self.range().map(|(_, b)| b + 1))
            .flatten()
    }
    fn actual_row(&self, item: usize) -> usize {
        item - usize::from(self.comment_index().is_some_and(|i| item > i))
    }
    fn reset_list(&self) {
        let top = self.list.logical_scroll_top();
        self.list.reset_with_uniform_height(
            self.lines.len() + usize::from(self.comment_open),
            row_height(),
        );
        self.list.scroll_to(top);
    }
    fn visible(&self, row: usize, column: u8) -> bool {
        !matches!(
            (column, &self.lines[row].kind),
            (1, LineKind::Added) | (2, LineKind::Removed)
        )
    }
    fn selected_bytes(&self, row: usize, column: u8) -> Range<usize> {
        selection_bytes(&self.lines, self.text.as_ref(), row, column)
    }
    fn sync_selection(&mut self, cx: &mut Context<Self>) {
        for (&(row, column), input) in &self.rows {
            let range = self.selected_bytes(row, column);
            input.update(cx, |input, cx| input.set_selection(range, cx));
        }
        cx.notify();
    }
    fn copy(&self, cx: &mut Context<Self>) {
        let Some(selection) = &self.text else {
            return;
        };
        let text = selected_text(&self.lines, selection);
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
    fn selection_event(
        &mut self,
        row: usize,
        column: u8,
        event: &SelectionInput,
        cx: &mut Context<Self>,
    ) {
        match event {
            SelectionInput::Begin { range, extend } => {
                if self.pending_comment {
                    return;
                }
                if self.comment_open {
                    self.comment_open = false;
                    self.reset_list();
                }
                self.gutter = None;
                let point = Position {
                    row,
                    byte: range.end,
                };
                if *extend && self.text.as_ref().is_some_and(|s| s.column == column) {
                    self.text.as_mut().unwrap().head = point;
                } else {
                    self.text = Some(TextSelection {
                        anchor: Position {
                            row,
                            byte: range.start,
                        },
                        head: point,
                        column,
                    });
                }
                self.drag = Some(Drag::Text);
                self.start_autoscroll(cx);
            }
            SelectionInput::Move { byte } if self.drag == Some(Drag::Text) => {
                if let Some(selection) = &mut self.text
                    && selection.column == column
                {
                    selection.head = Position { row, byte: *byte };
                }
            }
            SelectionInput::Copy => self.copy(cx),
            SelectionInput::SelectAll => {
                if let Some(last) = self.lines.len().checked_sub(1) {
                    self.text = Some(TextSelection {
                        anchor: Position { row: 0, byte: 0 },
                        head: Position {
                            row: last,
                            byte: self.lines[last].text.len(),
                        },
                        column,
                    });
                }
            }
            _ => {}
        }
        self.sync_selection(cx);
    }
    fn row_input(&mut self, row: usize, column: u8, cx: &mut Context<Self>) -> Entity<TextInput> {
        if let Some(input) = self.rows.get(&(row, column)) {
            return input.clone();
        }
        let text = self.lines[row].text.clone();
        let input = cx.new(|cx| {
            let mut input = TextInput::readonly(text, cx);
            input.delegate_selection();
            input.code(&self.extension, self.theme.id);
            input
        });
        self.subscriptions.insert(
            (row, column),
            cx.subscribe(&input, move |view, _, event, cx| {
                view.selection_event(row, column, event, cx)
            }),
        );
        input.update(cx, |input, cx| {
            input.set_selection(self.selected_bytes(row, column), cx)
        });
        self.rows.insert((row, column), input.clone());
        input
    }
    fn gutter_begin(
        &mut self,
        row: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Comments refer to the snapshot's rows, which unsaved edits no longer match.
        if self.pending_comment
            || self.lines[row].kind == LineKind::Header
            || self
                .edit
                .as_ref()
                .is_some_and(|edit| edit.read(cx).is_dirty())
        {
            return;
        }
        if self.comment_open {
            self.comment_open = false;
            self.reset_list();
        }
        self.text = None;
        self.gutter = Some(if event.modifiers.shift {
            (self.gutter.map_or(row, |s| s.0), row)
        } else {
            (row, row)
        });
        self.drag = Some(Drag::Gutter);
        self.pointer = Some(event.position);
        self.start_autoscroll(cx);
        window.focus(&self.focus, cx);
        cx.stop_propagation();
        self.sync_selection(cx);
    }
    fn finish_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.drag.take() == Some(Drag::Gutter) && self.gutter.is_some() {
            self.comment_open = true;
            self.reset_list();
            // Reveal the editor using measured row bounds when available.
            let entity = cx.weak_entity();
            window.on_next_frame(move |_, cx| {
                let _ = entity.update(cx, |view, cx| {
                    if let Some(index) = view.comment_index() {
                        if view.list.bounds_for_item(index).is_some() {
                            view.list.scroll_to_reveal_item(index);
                        } else {
                            view.list.scroll_to(ListOffset {
                                item_ix: index,
                                offset_in_item: px(0.),
                            });
                        }
                        cx.notify();
                    }
                });
            });
            window.focus(&self.comment.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
    fn pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pointer = Some(event.position);
        if event.pressed_button != Some(MouseButton::Left) {
            if self.drag.is_some() {
                self.finish_drag(window, cx);
            }
            return;
        }
        self.extend_at(event.position, cx);
    }
    fn extend_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.drag else {
            return;
        };
        let Some(bounds) = self.bounds else {
            return;
        };
        if self.lines.is_empty() {
            return;
        }
        let top = self.list.logical_scroll_top().item_ix;
        let y = position.y.max(bounds.top()).min(bounds.bottom() - px(0.1));
        let mut item = top;
        for index in top..self.list.item_count() {
            let Some(bounds) = self.list.bounds_for_item(index) else {
                break;
            };
            item = index;
            if y < bounds.bottom() {
                break;
            }
        }
        let row = self.actual_row(item).min(self.lines.len() - 1);
        if drag == Drag::Gutter {
            if let Some(selection) = &mut self.gutter {
                selection.1 = row;
            }
        } else if let Some(selection) = &self.text {
            let column = selection.column;
            if self.visible(row, column) {
                let byte = self
                    .rows
                    .get(&(row, column))
                    .map(|input| {
                        let input = input.read(cx);
                        let position = input.text_bounds().map_or(position, |b| {
                            point(
                                position.x,
                                position.y.max(b.top()).min(b.bottom() - px(0.1)),
                            )
                        });
                        input.byte_at(position)
                    })
                    .unwrap_or(self.lines[row].text.len());
                self.text.as_mut().unwrap().head = Position { row, byte };
            }
        }
        self.sync_selection(cx);
    }
    fn start_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.autoscroll_running {
            return;
        }
        self.autoscroll_running = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(32))
                    .await;
                if this
                    .update(cx, |view, cx| {
                        if view.drag.is_none() {
                            view.autoscroll_running = false;
                            return false;
                        }
                        view.autoscroll(cx);
                        true
                    })
                    .ok()
                    != Some(true)
                {
                    break;
                }
            }
        })
        .detach();
    }
    fn autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.drag.is_none() {
            return;
        }
        let (Some(pointer), Some(bounds)) = (self.pointer, self.bounds) else {
            return;
        };
        let distance = if pointer.y < bounds.top() + px(20.) {
            -px(12.)
        } else if pointer.y > bounds.bottom() - px(20.) {
            px(12.)
        } else {
            return;
        };
        self.list.scroll_by(distance);
        self.extend_at(pointer, cx);
    }
    fn cell(&mut self, row: usize, column: u8, cx: &mut Context<Self>) -> AnyElement {
        let line = self.lines[row].clone();
        let t = self.theme;
        if !self.visible(row, column) {
            return div()
                .min_h(row_height())
                .flex_1()
                .min_w_0()
                .bg(t.inset)
                .into_any_element();
        }
        // In edit mode a line of the edited text is editable, and an old line is read-only.
        let editable = self.edit.is_some() && column != 1 && line.new.is_some();
        let input = if editable {
            self.line_ids[row].and_then(|id| self.editor(id, cx))
        } else if self.edit.is_some() {
            Some(self.old_input(row, column, cx))
        } else {
            Some(self.row_input(row, column, cx))
        };
        let Some(input) = input else {
            return div()
                .min_h(row_height())
                .flex_1()
                .min_w_0()
                .into_any_element();
        };
        input.update(cx, |input, _| {
            input.code(&self.extension, self.theme.id);
            input.set_wrap(self.wrap);
        });
        let selected = self.range().is_some_and(|(a, b)| row >= a && row <= b);
        let header = line.kind == LineKind::Header;
        let marker = match line.kind {
            LineKind::Added => "+",
            LineKind::Removed => "−",
            _ => " ",
        };
        let numbers = if column == 0 {
            format!(
                "{:>4} {:>4} {marker}",
                line.old.map(|n| n.to_string()).unwrap_or_default(),
                line.new.map(|n| n.to_string()).unwrap_or_default()
            )
        } else {
            format!(
                "{:>4} {marker}",
                if column == 1 { line.old } else { line.new }
                    .map(|n| n.to_string())
                    .unwrap_or_default()
            )
        };
        div()
            .id(("diff-cell", row * 3 + column as usize))
            // An editor keeps the focus that a press gives it.
            .when(!editable, |d| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, window, cx| window.focus(&view.focus, cx)),
                )
            })
            .relative()
            .flex()
            .min_h(row_height())
            .when(!self.wrap, |d| d.h(row_height()))
            // Virtual rows are layout roots: auto width uses the input's 400px intrinsic fallback.
            .when(column == 0, |d| d.w_full())
            .flex_1()
            .min_w_0()
            .font_family(crate::theme::mono())
            .text_size(code_px(Settings::CODE_FONT_SIZE))
            .bg(if selected {
                t.accent.opacity(0.1)
            } else {
                match line.kind {
                    LineKind::Added => t.added_bg,
                    LineKind::Removed => t.removed_bg,
                    LineKind::Header => t.header,
                    _ => t.inset,
                }
            })
            // A hunk header is a quiet mono row that ends in a dotted rule.
            .when(header, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                )
            })
            .child(
                div()
                    .id(("line-gutter", row * 3 + column as usize))
                    .w(px(if column == 0 { 102. } else { 66. }))
                    .min_h(row_height())
                    .pt(px(3.))
                    .flex_shrink_0()
                    .pl_2()
                    .border_l_2()
                    .border_color(if selected {
                        t.accent
                    } else {
                        // A solid edge in the line's color marks changed lines down the gutter.
                        match line.kind {
                            LineKind::Added => t.added,
                            LineKind::Removed => t.removed,
                            _ => hsla(0., 0., 0., 0.),
                        }
                    })
                    .text_color(if selected {
                        t.accent
                    } else {
                        match line.kind {
                            LineKind::Added => t.added.opacity(0.8),
                            LineKind::Removed => t.removed.opacity(0.8),
                            _ => t.subtle,
                        }
                    })
                    .cursor_crosshair()
                    .whitespace_nowrap()
                    .when(header, |d| d.child(super::caps("Hunk", t)))
                    .when(!header, |d| d.child(numbers))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, event, window, cx| {
                            view.gutter_begin(row, event, window, cx)
                        }),
                    ),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .py(px(3.))
                    .overflow_hidden()
                    .text_color(if header { t.muted } else { t.text })
                    .child(input),
            )
            .into_any_element()
    }
    fn render_row(&mut self, item: usize, cx: &mut Context<Self>) -> AnyElement {
        if self.comment_index() == Some(item) {
            return self.comment_editor(cx);
        }
        let row = self.actual_row(item);
        if row >= self.lines.len() {
            return div().h(row_height()).into_any_element();
        }
        if self.split {
            div()
                .w_full()
                .flex()
                .min_h(row_height())
                .when(!self.wrap, |d| d.h(row_height()))
                .child(self.cell(row, 1, cx))
                .child(self.cell(row, 2, cx))
                .into_any_element()
        } else {
            self.cell(row, 0, cx)
        }
    }
    fn comment_editor(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let range = self.range().unwrap();
        let count = range.1 - range.0 + 1;
        div()
            .w(self.bounds.map_or(px(360.), |bounds| bounds.size.width))
            .ml(-self.horizontal.offset().x)
            .h(px(164.))
            .px(px(12.))
            .py(px(8.))
            .bg(t.sidebar)
            .child(
                kit::block(t)
                    .h_full()
                    .flex()
                    .flex_col()
                    .border_color(t.accent.opacity(0.4))
                    .bg(t.canvas)
                    .child(
                        kit::block_header("Comment", t.accent, t)
                            .pr(px(10.))
                            .child(kit::chip(
                                format!("{count} {}", if count == 1 { "LINE" } else { "LINES" }),
                                t.accent,
                            )),
                    )
                    .child(
                        div()
                            .id("diff-comment-input")
                            .px(px(10.))
                            .py(px(7.))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .font_family(crate::theme::sans())
                            .text_size(text_px(13.))
                            .child(self.comment.clone()),
                    )
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(6.))
                            .py(px(4.))
                            .flex()
                            .justify_end()
                            .gap(px(6.))
                            .child(
                                action_button("cancel-diff-comment", "close", "Cancel", false, t)
                                    .disabled(self.pending_comment)
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.gutter = None;
                                        view.comment_open = false;
                                        view.reset_list();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                action_button(
                                    "attach-diff-comment",
                                    "plus",
                                    "Add context",
                                    true,
                                    t,
                                )
                                .disabled(self.pending_comment)
                                .on_click(cx.listener(
                                    |view, _, _, cx| {
                                        if !view.pending_comment
                                            && let Some((first_row, last_row)) = view.comment_rows()
                                        {
                                            view.pending_comment = true;
                                            cx.emit(ContextRequested {
                                                snapshot_id: view.snapshot.clone(),
                                                file: view.file,
                                                first_row,
                                                last_row,
                                                comment: view.comment.read(cx).text(),
                                            });
                                            cx.notify();
                                        }
                                    },
                                )),
                            ),
                    ),
            )
            .into_any_element()
    }
}

// A button of the comment editor in the shared control language. This view is its own entity,
// so it cannot use the shell's button builders. `filled` selects the accent main action, a
// glass key like the shell's `primary`; the other is a ghost button.
fn action_button(
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    filled: bool,
    theme: Theme,
) -> Button {
    let tone = if filled { theme.accent } else { theme.muted };
    let button = Button::new(id)
        .relative()
        .overflow_hidden()
        .h(px(26.))
        .px(px(9.))
        .gap(px(5.))
        .rounded(px(crate::theme::RADIUS_CONTROL))
        .border_1()
        .font_family(crate::theme::mono())
        .text_size(text_px(11.5))
        .cursor_pointer()
        .styles(|s| s.disabled(|s| s.opacity(0.4).cursor_default()))
        .focus_visible(move |s| s.border_color(theme.accent.opacity(0.7)))
        .on_press(move |position, cx| fx::ripple(position, tone, cx));
    if filled {
        button
            .group(kit::GLASS)
            .px(px(12.))
            .layer(kit::glass_body(tone))
            .child(kit::engraved(glyph, tone, 13.))
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .shadow(kit::glass_glow(tone, theme, kit::Pose::Rest))
            .text_color(theme.on_accent)
            .font_weight(FontWeight::SEMIBOLD)
            .hover(move |s| s.shadow(kit::glass_glow(tone, theme, kit::Pose::Hover)))
            .active(move |s| s.shadow(kit::glass_glow(tone, theme, kit::Pose::Pressed)))
    } else {
        button
            .layer(kit::glint(theme.dot, dither::Edge::Bottom))
            .child(icon(glyph).size(px(13.)))
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .text_color(theme.muted)
            .font_weight(FontWeight::MEDIUM)
            .hover(move |s| {
                s.bg(theme.selected)
                    .text_color(theme.text)
                    .shadow(kit::plate_light(theme, false))
            })
            .active(move |s| {
                s.bg(theme.control_active)
                    .shadow(kit::recessed_light(theme))
            })
    }
    .child(label)
}
impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.finish_attach(window, cx);
        if let Some((id, byte)) = self.pending_focus.take()
            && let Some(input) = self.editor(id, cx)
        {
            input.update(cx, |input, cx| input.set_selection(byte..byte, cx));
            window.focus(&input.read(cx).focus_handle(cx), cx);
            if let Some(row) = self.line_ids.iter().position(|line| *line == Some(id)) {
                self.list.scroll_to_reveal_item(row);
            }
        }
        let top = self.list.logical_scroll_top().item_ix;
        self.rows.retain(|(row, _), input| {
            row.abs_diff(top) < 256 || input.read(cx).focus_handle(cx).is_focused(window)
        });
        if self.edit.is_some() {
            let end = (top + 256).min(self.line_ids.len());
            let near: HashSet<u64> = self.line_ids[top.saturating_sub(256).min(end)..end]
                .iter()
                .flatten()
                .copied()
                .collect();
            self.editors.retain(|id, (input, _)| {
                near.contains(id) || input.read(cx).focus_handle(cx).is_focused(window)
            });
        }
        self.subscriptions
            .retain(|key, _| self.rows.contains_key(key));
        let entity = cx.entity();
        let bounds_entity = entity.clone();
        let pointer_entity = entity.clone();
        // The text is as wide as its characters at the code size; the gutters follow the
        // interface scale. The sum is in window pixels.
        let text_width = self.text_width * f32::from(code_px(1.));
        let width = if self.wrap {
            0.
        } else if self.split {
            (text_width + f32::from(px(66.))) * 2.
        } else {
            text_width + f32::from(px(102.))
        };
        div()
            .id("diff-document")
            .w_full()
            .min_w_0()
            .key_context("DiffDocument")
            .track_focus(&self.focus)
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .on_action(
                cx.listener(|view, _: &crate::components::text_input::Copy, _, cx| view.copy(cx)),
            )
            .when(self.edit.is_some(), |d| {
                d.on_action(cx.listener(|view, _: &Undo, _, cx| view.undo(cx)))
                    .on_action(cx.listener(|view, _: &Redo, _, cx| view.redo(cx)))
            })
            .on_action(cx.listener(
                |view, _: &crate::components::text_input::SelectAll, _, cx| {
                    if view.edit.is_some() {
                        return;
                    }
                    let column = view
                        .text
                        .as_ref()
                        .map_or(if view.split { 2 } else { 0 }, |selection| selection.column);
                    view.selection_event(0, column, &SelectionInput::SelectAll, cx);
                },
            ))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| view.finish_drag(window, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| view.finish_drag(window, cx)),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        bounds_entity.update(cx, |view, _| view.bounds = Some(bounds));
                    },
                    move |_, _, window, _| {
                        let entity = pointer_entity.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble && entity.read(cx).drag.is_some() {
                                entity.update(cx, |view, cx| view.pointer_move(event, window, cx));
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .id("diff-horizontal")
                    .size_full()
                    .when(!self.wrap, |d| d.overflow_x_scroll())
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.horizontal)
                    .child(
                        list(self.list.clone(), move |row, _, cx| {
                            entity.update(cx, |view, cx| view.render_row(row, cx))
                        })
                        .w_full()
                        .min_w(gpui::px(width))
                        .h_full(),
                    ),
            )
    }
}
