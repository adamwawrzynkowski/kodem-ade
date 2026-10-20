//! The working-tree text of the file open in the Changes pane, which its diff edits in place:
//! the lines, their undo history, and the hunks of the line diff against the old side of the
//! comparison, which follow every edit.
use super::*;
use std::time::Instant;

/// Consecutive changes of one line within this time undo as one step.
const COALESCE: Duration = Duration::from_millis(1000);
/// Unchanged lines that a hunk shows before and after its changes, as Git's capture does.
const CONTEXT: usize = 5;
/// Most differing lines that the line diff pairs. A larger difference shows its changed middle
/// as removed and then added, so that a keystroke never waits for the diff.
const MAX_DIFFERENCES: usize = 1024;

/// Identifies an edited file: its task, its repository and its repository-relative path.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct EditKey {
    pub task: String,
    pub repository: Option<PathBuf>,
    pub path: Vec<u8>,
}

impl EditKey {
    /// Returns the key of a file of a snapshot.
    pub(super) fn of(snapshot: &DiffSnapshot, file: &FileDelta) -> Self {
        Self {
            task: snapshot.task_id.clone(),
            repository: file.repository.clone(),
            path: file.path.clone(),
        }
    }
}

/// A caret in the edited text: a line and a byte offset in it.
#[derive(Clone, Copy)]
pub(super) struct Caret {
    pub line: usize,
    pub byte: usize,
}

// A line of the edited text. The id stays with the line while lines above it come and go, so
// that its editor keeps focus and caret.
#[derive(Clone)]
struct Line {
    id: u64,
    text: String,
}

// One undoable change: the lines at `at` that it removed and the lines that it inserted.
struct Change {
    at: usize,
    removed: Vec<Line>,
    inserted: Vec<Line>,
    before: Caret,
    after: Caret,
    time: Instant,
}

/// The edited text of a working-tree file and what it is compared with.
pub(super) struct FileEdit {
    /// The comparison whose old side `old` is.
    pub comparison: Comparison,
    /// The digest of the file in the capture that the text was read for. A capture with
    /// another digest reads the file again while nothing is unsaved.
    pub source: String,
    old: Vec<String>,
    lines: Vec<Line>,
    /// The lines on disk, which a save replaces.
    saved: Vec<String>,
    /// The SHA-256 digest of the bytes on disk.
    digest: String,
    /// The line break of the file: CRLF when every line break is one, LF otherwise.
    newline: &'static str,
    final_newline: bool,
    undo: Vec<Change>,
    redo: Vec<Change>,
    next_id: u64,
    /// The text of a save on its way to the host.
    saving: Option<String>,
    /// Lines that the diff has shown. They stay in view while edits change the hunks around
    /// them, so that the line being edited never leaves the diff.
    shown: HashSet<u64>,
}

impl FileEdit {
    /// Starts editing the working-tree text of a file against the old side of `comparison`,
    /// read for the capture whose digest of the file is `source`.
    pub(super) fn new(file: EditableFile, comparison: Comparison, source: String) -> Self {
        let (old, _, _) = split_lines(&file.before);
        let (texts, newline, final_newline) = split_lines(&file.after);
        let texts = if texts.is_empty() {
            vec![String::new()]
        } else {
            texts
        };
        let lines: Vec<Line> = texts
            .iter()
            .enumerate()
            .map(|(id, text)| Line {
                id: id as u64,
                text: text.clone(),
            })
            .collect();
        Self {
            comparison,
            source,
            old,
            next_id: lines.len() as u64,
            lines,
            saved: texts,
            digest: file.digest,
            newline,
            final_newline,
            undo: Vec::new(),
            redo: Vec::new(),
            saving: None,
            shown: HashSet::new(),
        }
    }

    /// Reports whether a new read of the file found the text on disk and the old side that
    /// this edit started from.
    pub(super) fn same_file(&self, file: &EditableFile) -> bool {
        self.digest == file.digest && self.old == split_lines(&file.before).0
    }

    /// Returns the number of lines of the edited text.
    pub(super) fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Returns the text of a line.
    pub(super) fn text(&self, line: usize) -> &str {
        &self.lines[line].text
    }

    /// Returns the id of a line.
    pub(super) fn id(&self, line: usize) -> u64 {
        self.lines[line].id
    }

    /// Returns the line that has an id, while it exists.
    pub(super) fn index_of(&self, id: u64) -> Option<usize> {
        self.lines.iter().position(|line| line.id == id)
    }

    /// Reports whether the edited text differs from the file on disk.
    pub(super) fn is_dirty(&self) -> bool {
        self.lines.len() != self.saved.len()
            || self
                .lines
                .iter()
                .zip(&self.saved)
                .any(|(line, saved)| line.text != *saved)
    }

    /// Reports whether a save is on its way to the host.
    pub(super) fn is_saving(&self) -> bool {
        self.saving.is_some()
    }

    /// Reports whether a change can be undone.
    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Replaces `count` lines at `at` with `texts` as one undoable change. The first new line
    /// keeps the id of the first replaced line. A change of one line soon after another change
    /// of the same line joins it in one undo step.
    pub(super) fn replace(
        &mut self,
        at: usize,
        count: usize,
        texts: Vec<String>,
        before: Caret,
        after: Caret,
    ) {
        let now = Instant::now();
        let inserted: Vec<Line> = texts
            .into_iter()
            .enumerate()
            .map(|(index, text)| {
                let id = if index == 0 && count > 0 {
                    self.lines[at].id
                } else {
                    self.next_id += 1;
                    self.next_id
                };
                Line { id, text }
            })
            .collect();
        let removed: Vec<Line> = self
            .lines
            .splice(at..at + count, inserted.iter().cloned())
            .collect();
        let joins = count == 1
            && inserted.len() == 1
            && self.redo.is_empty()
            && self.undo.last().is_some_and(|last| {
                last.at == at
                    && last.removed.len() == 1
                    && last.inserted.len() == 1
                    && now.duration_since(last.time) < COALESCE
            });
        self.redo.clear();
        if joins && let Some(last) = self.undo.last_mut() {
            last.inserted = inserted;
            last.after = after;
            last.time = now;
            return;
        }
        if self.undo.len() == 1000 {
            self.undo.remove(0);
        }
        self.undo.push(Change {
            at,
            removed,
            inserted,
            before,
            after,
            time: now,
        });
    }

    /// Undoes the last change and returns the caret from before it.
    pub(super) fn undo(&mut self) -> Option<Caret> {
        let change = self.undo.pop()?;
        self.lines.splice(
            change.at..change.at + change.inserted.len(),
            change.removed.iter().cloned(),
        );
        let caret = change.before;
        self.redo.push(change);
        Some(caret)
    }

    /// Applies the last undone change again and returns the caret from after it.
    pub(super) fn redo(&mut self) -> Option<Caret> {
        let change = self.redo.pop()?;
        self.lines.splice(
            change.at..change.at + change.removed.len(),
            change.inserted.iter().cloned(),
        );
        let caret = change.after;
        self.undo.push(change);
        Some(caret)
    }

    /// Restores the text on disk as one undoable change and returns the caret after it.
    pub(super) fn revert(&mut self) -> Caret {
        let caret = Caret { line: 0, byte: 0 };
        self.replace(0, self.lines.len(), self.saved.clone(), caret, caret);
        caret
    }

    /// Starts a save: returns the text to write and the digest that the file must still have.
    pub(super) fn begin_save(&mut self) -> Option<(String, String)> {
        if self.saving.is_some() || !self.is_dirty() {
            return None;
        }
        let mut text = self
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join(self.newline);
        if self.final_newline {
            text.push_str(self.newline);
        }
        self.saving = Some(text.clone());
        Some((text, self.digest.clone()))
    }

    /// Records that the save on its way reached the disk.
    pub(super) fn saved(&mut self) {
        if let Some(text) = self.saving.take() {
            self.digest = core::git::digest(text.as_bytes());
            self.saved = split_lines(&text).0;
            if self.saved.is_empty() {
                self.saved.push(String::new());
            }
        }
    }

    /// Records that the save on its way failed, so that it can be tried again.
    pub(super) fn save_failed(&mut self) {
        self.saving = None;
    }

    /// Returns the hunks of the edited text against the old side, as diff rows with a header
    /// before each hunk, and for each row the id of its line of the edited text. A hunk shows
    /// `CONTEXT` unchanged lines around its changes, and every line that the diff showed
    /// before. A text without changes shows whole. Rows are made only for the lines shown.
    pub(super) fn rows(&mut self) -> (Vec<DiffLine>, Vec<Option<u64>>) {
        let steps = self.steps();
        let changed = |step: &Step| matches!(step, Step::Removed(_) | Step::Added(_));
        // The distance of each step to the nearest changed step.
        let mut near = vec![usize::MAX; steps.len()];
        let mut last = None;
        for (index, step) in steps.iter().enumerate() {
            if changed(step) {
                last = Some(index);
            }
            if let Some(last) = last {
                near[index] = index - last;
            }
        }
        last = None;
        for (index, step) in steps.iter().enumerate().rev() {
            if changed(step) {
                last = Some(index);
            }
            if let Some(last) = last {
                near[index] = near[index].min(last - index);
            }
        }
        let any_change = steps.iter().any(changed);
        let visible: Vec<bool> = steps
            .iter()
            .zip(&near)
            .map(|(step, near)| {
                !any_change
                    || *near <= CONTEXT
                    || self
                        .step_id(*step)
                        .is_some_and(|id| self.shown.contains(&id))
            })
            .collect();
        let has_old = |step: &Step| matches!(step, Step::Same(..) | Step::Removed(_));
        let has_new = |step: &Step| !matches!(step, Step::Removed(_));
        let mut rows = Vec::new();
        let mut ids = Vec::new();
        let (mut old_before, mut new_before) = (0, 0);
        let mut index = 0;
        while index < steps.len() {
            if !visible[index] {
                old_before += usize::from(has_old(&steps[index]));
                new_before += usize::from(has_new(&steps[index]));
                index += 1;
                continue;
            }
            let end = (index..steps.len())
                .find(|step| !visible[*step])
                .unwrap_or(steps.len());
            let hunk = &steps[index..end];
            let old_count = hunk.iter().filter(|step| has_old(step)).count();
            let new_count = hunk.iter().filter(|step| has_new(step)).count();
            // As in Git, an empty side names the line before the hunk.
            let start = |before: usize, count: usize| before + usize::from(count > 0);
            rows.push(DiffLine {
                old: None,
                new: None,
                kind: LineKind::Header,
                text: format!(
                    "@@ -{},{old_count} +{},{new_count} @@",
                    start(old_before, old_count),
                    start(new_before, new_count)
                ),
            });
            ids.push(None);
            for step in hunk {
                let id = self.step_id(*step);
                rows.push(self.row(*step));
                ids.push(id);
                self.shown.extend(id);
            }
            old_before += old_count;
            new_before += new_count;
            index = end;
        }
        (rows, ids)
    }

    // Returns the line diff of the whole edited text against the old side. Each run of changes
    // lists its removed lines first. A file without text has one empty line to type in.
    fn steps(&self) -> Vec<Step> {
        let empty = self.lines.len() == 1 && self.lines[0].text.is_empty() && !self.final_newline;
        let new: Vec<&str> = if empty {
            Vec::new()
        } else {
            self.lines.iter().map(|line| line.text.as_str()).collect()
        };
        let old: Vec<&str> = self.old.iter().map(String::as_str).collect();
        let mut steps = Vec::with_capacity(old.len().max(new.len()) + 1);
        let mut added = Vec::new();
        for step in line_diff(&old, &new) {
            match step {
                Step::Added(_) => added.push(step),
                Step::Same(..) => {
                    steps.append(&mut added);
                    steps.push(step);
                }
                _ => steps.push(step),
            }
        }
        steps.append(&mut added);
        if empty {
            steps.push(Step::Empty);
        }
        steps
    }

    // Returns the id of the edited line of a step, which a removed line does not have.
    fn step_id(&self, step: Step) -> Option<u64> {
        match step {
            Step::Same(_, new) | Step::Added(new) => Some(self.lines[new].id),
            Step::Empty => Some(self.lines[0].id),
            Step::Removed(_) => None,
        }
    }

    // Returns the diff row of a step.
    fn row(&self, step: Step) -> DiffLine {
        let number = |index: usize| Some(index as u32 + 1);
        match step {
            Step::Same(old, new) => DiffLine {
                old: number(old),
                new: number(new),
                kind: LineKind::Context,
                text: self.lines[new].text.clone(),
            },
            Step::Removed(old) => DiffLine {
                old: number(old),
                new: None,
                kind: LineKind::Removed,
                text: self.old[old].clone(),
            },
            Step::Added(new) => DiffLine {
                old: None,
                new: number(new),
                kind: LineKind::Added,
                text: self.lines[new].text.clone(),
            },
            // The empty line of a file without text is not an added line.
            Step::Empty => DiffLine {
                old: None,
                new: Some(1),
                kind: LineKind::Context,
                text: String::new(),
            },
        }
    }
}

impl Shell {
    /// Reports whether the working-tree side of a file of the displayed snapshot can be edited:
    /// a live comparison with the working tree, in a task that may change the repository, of a
    /// text file that still exists.
    pub(super) fn can_edit(&self, file: &FileDelta) -> bool {
        !self.historical_diff
            && self.comparison != Comparison::Staged
            && self
                .view
                .as_ref()
                .is_some_and(|view| view.task.kind != TaskKind::Review)
            && file.notice.is_none()
            && !file.status.contains('D')
    }

    /// Returns the edit of a file of a snapshot, while the file can be edited.
    pub(super) fn file_edit(
        &self,
        snapshot: &DiffSnapshot,
        file: &FileDelta,
    ) -> Option<Entity<FileEdit>> {
        if !self.can_edit(file) {
            return None;
        }
        self.edits.get(&EditKey::of(snapshot, file)).cloned()
    }

    // Returns the open file of the displayed snapshot.
    fn reviewed_file(&self) -> Option<(&DiffSnapshot, &FileDelta)> {
        let snapshot = self.diff.as_ref()?;
        let file = snapshot
            .files
            .get(self.file)
            .filter(|_| self.reviewing_file)?;
        Some((snapshot, file))
    }

    /// Reads the open file for editing when it has no edit yet, or when the capture found it
    /// changed and nothing of its edit is unsaved. Until the text arrives the diff shows the
    /// capture. A file that cannot be read is not asked for again until it changes.
    pub(super) fn load_edit(&mut self, cx: &App) {
        if self.opening_edit.is_some() {
            return;
        }
        let Some((snapshot, file)) = self.reviewed_file() else {
            return;
        };
        if !self.can_edit(file) {
            return;
        }
        let key = EditKey::of(snapshot, file);
        let current = self.edits.get(&key).is_some_and(|edit| {
            let edit = edit.read(cx);
            edit.is_dirty()
                || edit.is_saving()
                || (edit.source == file.digest && edit.comparison == snapshot.comparison)
        });
        let attempt = (key, file.digest.clone());
        if current || self.failed_edit.as_ref() == Some(&attempt) {
            return;
        }
        let request = core::Request::ReadEditableFile {
            task_id: snapshot.task_id.clone(),
            snapshot_id: snapshot.id.clone(),
            file: self.file,
        };
        self.opening_edit = Some((attempt.0, snapshot.comparison.clone(), attempt.1));
        self.send(request);
    }

    /// Starts editing the file whose sides the host read. An edit with unsaved text stays, and
    /// one that already has the text on disk keeps its undo history.
    pub(super) fn edit_opened(&mut self, file: EditableFile, cx: &mut Context<Self>) {
        let Some((key, comparison, source)) = self.opening_edit.take() else {
            return;
        };
        if let Some(edit) = self.edits.get(&key) {
            let kept = edit.update(cx, |edit, _| {
                let kept = edit.is_dirty()
                    || edit.is_saving()
                    || (edit.comparison == comparison && edit.same_file(&file));
                if kept {
                    edit.source = source.clone();
                }
                kept
            });
            if kept {
                return;
            }
        }
        self.edits
            .insert(key, cx.new(|_| FileEdit::new(file, comparison, source)));
    }

    /// Sends the edited text of the open file to the host, which writes it when the file on
    /// disk is still the one that editing began with.
    pub(super) fn save_edit(&mut self, cx: &mut Context<Self>) {
        let Some((snapshot, file)) = self.reviewed_file() else {
            return;
        };
        let Some(edit) = self.file_edit(snapshot, file) else {
            return;
        };
        let key = EditKey::of(snapshot, file);
        let Some((text, expected)) = edit.update(cx, |edit, _| edit.begin_save()) else {
            return;
        };
        self.send(core::Request::SaveFile {
            task_id: key.task,
            snapshot_id: snapshot.id.clone(),
            repository: key.repository,
            path: key.path,
            expected,
            text,
        });
        cx.notify();
    }

    /// Records the outcome of a save, and captures the changes again after one that worked.
    pub(super) fn edit_saved(
        &mut self,
        request: &core::Request,
        saved: bool,
        cx: &mut Context<Self>,
    ) {
        let core::Request::SaveFile {
            task_id,
            repository,
            path,
            ..
        } = request
        else {
            return;
        };
        let key = EditKey {
            task: task_id.clone(),
            repository: repository.clone(),
            path: path.clone(),
        };
        if let Some(edit) = self.edits.get(&key) {
            edit.update(cx, |edit, _| {
                if saved {
                    edit.saved();
                } else {
                    edit.save_failed();
                }
            });
        }
        if saved && self.selected.as_ref() == Some(task_id) {
            self.refresh_diff();
            self.refresh_task_changes();
        }
    }

    /// Handles a failed read or save of an edited file. A file that cannot be read stays a
    /// read-only diff without an error, and the answer reports whether it was such a read.
    pub(super) fn edit_failed(&mut self, request: &core::Request, cx: &mut Context<Self>) -> bool {
        match request {
            core::Request::ReadEditableFile { .. } => {
                if let Some((key, _, source)) = self.opening_edit.take() {
                    self.failed_edit = Some((key, source));
                }
                true
            }
            core::Request::SaveFile { .. } => {
                self.edit_saved(request, false, cx);
                false
            }
            _ => false,
        }
    }

    /// Forgets the edits without unsaved text, except the one of the file that stays open.
    pub(super) fn prune_edits(&mut self, diff: &DiffSnapshot, cx: &App) {
        let open = diff
            .files
            .get(self.file)
            .filter(|_| self.reviewing_file)
            .map(|file| EditKey::of(diff, file));
        self.edits.retain(|key, edit| {
            let edit = edit.read(cx);
            edit.is_dirty() || edit.is_saving() || open.as_ref() == Some(key)
        });
    }

    /// Builds the controls of an edited file for its title bar: its state, undo, discard and
    /// save.
    pub(super) fn edit_controls(&self, edit: &Entity<FileEdit>, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let (dirty, saving, can_undo) = {
            let edit = edit.read(cx);
            (edit.is_dirty(), edit.is_saving(), edit.can_undo())
        };
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            // The same spacing as the title's other actions.
            .gap(px(8.))
            .when(saving, |d| d.child(kit::chip("SAVING", t.muted)))
            .when(dirty && !saving, |d| {
                d.child(kit::chip("UNSAVED", t.accent))
            })
            .child(
                self.icon_button("edit-undo", "undo-2", "Undo · ⌘Z")
                    .disabled(!can_undo || saving)
                    .on_click(cx.listener(|s, _, _, cx| {
                        if let Some(view) = &s.diff_view {
                            view.update(cx, |view, cx| view.undo(cx));
                        }
                    })),
            )
            .when(dirty, |d| {
                d.child(
                    self.icon_button("edit-discard", "trash-2", "Discard unsaved edits")
                        .disabled(saving)
                        .on_click(cx.listener(|s, _, _, cx| {
                            if let Some(view) = &s.diff_view {
                                view.update(cx, |view, cx| view.revert(cx));
                            }
                        })),
                )
            })
            .child(
                self.icon_button("edit-save", "arrow-down-to-line", "Save · ⌘S")
                    .disabled(!dirty || saving)
                    .when(dirty && !saving, |button| kit::lit(button, t.accent))
                    .on_click(cx.listener(|s, _, _, cx| s.save_edit(cx))),
            )
    }
}

// Splits a text into lines without their breaks. Returns the lines, the break to write them
// back with, and whether the text ends with a break. CRLF is the break only when every line
// break of the text is one; otherwise a carriage return stays part of its line.
fn split_lines(text: &str) -> (Vec<String>, &'static str, bool) {
    if text.is_empty() {
        return (Vec::new(), "\n", false);
    }
    let breaks = text.matches('\n').count();
    let crlf = breaks > 0 && text.matches("\r\n").count() == breaks;
    let newline = if crlf { "\r\n" } else { "\n" };
    let final_newline = text.ends_with('\n');
    let body = if final_newline {
        text.strip_suffix(newline).unwrap_or(text)
    } else {
        text
    };
    (
        body.split(newline).map(str::to_owned).collect(),
        newline,
        final_newline,
    )
}

/// One step of a line diff: a line on both sides, or a line of only one side. `Empty` is the
/// line to type in of a file without text.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Step {
    Same(usize, usize),
    Removed(usize),
    Added(usize),
    Empty,
}

// Pairs the lines of two texts with Myers' algorithm after their common start and end. Past
// `MAX_DIFFERENCES` the middle is removed and then added.
fn line_diff(old: &[&str], new: &[&str]) -> Vec<Step> {
    let start = old
        .iter()
        .zip(new)
        .take_while(|(old, new)| old == new)
        .count();
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let mut steps: Vec<Step> = (0..start).map(|i| Step::Same(i, i)).collect();
    // A few edits compare few lines on each diagonal, so lines compare as text: numbering
    // every line of the middle first would cost more than the search.
    let a = &old[start..old.len() - end];
    let b = &new[start..new.len() - end];
    let middle = myers(a, b).unwrap_or_else(|| {
        (0..a.len())
            .map(Step::Removed)
            .chain((0..b.len()).map(Step::Added))
            .collect()
    });
    steps.extend(middle.into_iter().map(|step| match step {
        Step::Same(o, n) => Step::Same(o + start, n + start),
        Step::Removed(o) => Step::Removed(o + start),
        Step::Added(n) => Step::Added(n + start),
        Step::Empty => Step::Empty,
    }));
    steps.extend((0..end).map(|i| Step::Same(old.len() - end + i, new.len() - end + i)));
    steps
}

// Returns the shortest edit script from `a` to `b`, or nothing when it needs more than
// `MAX_DIFFERENCES` removed and added lines. Each step `d` keeps the furthest reaching x of
// the diagonals -d..=d, from which the path is read back.
fn myers(a: &[&str], b: &[&str]) -> Option<Vec<Step>> {
    if a.is_empty() || b.is_empty() {
        return Some(
            (0..a.len())
                .map(Step::Removed)
                .chain((0..b.len()).map(Step::Added))
                .collect(),
        );
    }
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (n + m).min(MAX_DIFFERENCES as isize);
    let offset = max + 1;
    let mut v = vec![0isize; 2 * max as usize + 3];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut reached = false;
    'search: for d in 0..=max {
        for k in (-d..=d).step_by(2) {
            let index = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[index - 1] < v[index + 1]) {
                v[index + 1]
            } else {
                v[index - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[index] = x;
            if x >= n && y >= m {
                trace.push(v[(offset - d) as usize..=(offset + d) as usize].to_vec());
                reached = true;
                break 'search;
            }
        }
        trace.push(v[(offset - d) as usize..=(offset + d) as usize].to_vec());
    }
    if !reached {
        return None;
    }
    let mut steps = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (1..trace.len() as isize).rev() {
        let previous = &trace[d as usize - 1];
        let at = |k: isize| previous[(k + d - 1) as usize];
        let k = x - y;
        let down = k == -d || (k != d && at(k - 1) < at(k + 1));
        let previous_k = if down { k + 1 } else { k - 1 };
        let previous_x = at(previous_k);
        let previous_y = previous_x - previous_k;
        let (mid_x, mid_y) = if down {
            (previous_x, previous_y + 1)
        } else {
            (previous_x + 1, previous_y)
        };
        while x > mid_x && y > mid_y {
            x -= 1;
            y -= 1;
            steps.push(Step::Same(x as usize, y as usize));
        }
        if down {
            steps.push(Step::Added(previous_y as usize));
        } else {
            steps.push(Step::Removed(previous_x as usize));
        }
        x = previous_x;
        y = previous_y;
    }
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        steps.push(Step::Same(x as usize, y as usize));
    }
    steps.reverse();
    Some(steps)
}
