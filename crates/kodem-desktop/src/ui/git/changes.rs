//! The Changes view: conflicted, staged and unstaged files grouped by extension or as a folder
//! tree, with stage, unstage and discard actions that throw or crumble the rows, the commit box
//! with its title, description, amend switch and AI menu, and a resizable diff panel at the
//! right or the bottom.
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    Anchor, AnyElement, App, Bounds, ClickEvent, ClipboardItem, Context, Div, Entity, EventEmitter,
    FontWeight, Hsla, IntoElement, Pixels, Point, Render, SharedString, Window, div, point, size,
};
use kodem_git::ai::{AiConfig, AiService, ChangeContext, ChangeScope, CommitGenerationTarget};
use kodem_git::git::{AmendState, ConflictOperation, FileChangeKind, StatusEntry};
use kodem_git::settings::{
    ChangesRenderMode, MAX_DIFF_PANEL_HEIGHT, MAX_DIFF_PANEL_WIDTH, MIN_DIFF_PANEL_HEIGHT,
    MIN_DIFF_PANEL_WIDTH,
};

use crate::theme::{RADIUS_CONTROL, px, text_px, ui_scale};
use crate::ui::controls::Switch;
use crate::ui::dither::{BarSegment, Dither, DitherBar, DitherField, checker};
use crate::ui::fx;
use crate::ui::git::ai::{AiResult, open_ai_result_dialog, run_ai_result_task};
use crate::ui::git::components::{
    self, button, change_kind_chip, count_badge, full_size, mono_font, text_field,
};
use crate::ui::git::contextual_actions::confirm_destructive_action;
use crate::ui::git::diff::{DiffCloseRequested, DiffView, PreparedDiff};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, ContextMenuExt as _, Disableable as _,
    DropdownMenu as _, Icon, IconName, InputState, PopupMenu, PopupMenuItem, ResizableState,
    ScrollableElement as _, Selectable as _, Sizable as _, Textarea, TextareaState, h_flex,
    h_resizable, resizable_panel, v_flex, v_resizable,
};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::state::{
    RepositoryEvent, RepositoryNotificationStatus, RepositoryState, StatusMutation,
};
use crate::ui::git::theme;

struct FileExtensionGroup<'a> {
    label: String,
    entries: Vec<(usize, &'a StatusEntry)>,
}

#[derive(Default)]
struct ChangeFileTree<'a> {
    folders: BTreeMap<String, ChangeFileTree<'a>>,
    files: BTreeMap<String, (usize, &'a StatusEntry)>,
}

impl<'a> ChangeFileTree<'a> {
    fn insert(&mut self, index: usize, entry: &'a StatusEntry) {
        let parts = entry
            .path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let Some((file_name, folders)) = parts.split_last() else {
            return;
        };

        let mut node = self;
        for folder in folders {
            node = node.folders.entry((*folder).to_string()).or_default();
        }
        node.files.insert((*file_name).to_string(), (index, entry));
    }

    fn file_count(&self) -> usize {
        self.files.len()
            + self
                .folders
                .values()
                .map(ChangeFileTree::file_count)
                .sum::<usize>()
    }

    fn entries(&self) -> Vec<StatusEntry> {
        self.files
            .values()
            .map(|(_, entry)| (*entry).clone())
            .chain(self.folders.values().flat_map(ChangeFileTree::entries))
            .collect()
    }
}

enum ChangeTreeRow<'a> {
    Folder {
        key: String,
        path: String,
        name: String,
        depth: usize,
        count: usize,
        expanded: bool,
        entries: Vec<StatusEntry>,
    },
    File {
        name: String,
        depth: usize,
        index: usize,
        entry: &'a StatusEntry,
    },
}

fn change_tree_rows<'a>(
    entries: &'a [StatusEntry],
    collapsed_folders: &HashSet<String>,
    section_key: &str,
) -> Vec<ChangeTreeRow<'a>> {
    let mut tree = ChangeFileTree::default();
    for (index, entry) in entries.iter().enumerate() {
        tree.insert(index, entry);
    }

    let mut rows = Vec::new();
    flatten_change_tree(&tree, "", 0, collapsed_folders, section_key, &mut rows);
    rows
}

fn flatten_change_tree<'a>(
    node: &ChangeFileTree<'a>,
    parent_path: &str,
    depth: usize,
    collapsed_folders: &HashSet<String>,
    section_key: &str,
    rows: &mut Vec<ChangeTreeRow<'a>>,
) {
    for (name, child) in &node.folders {
        let path = if parent_path.is_empty() {
            name.clone()
        } else {
            format!("{parent_path}/{name}")
        };
        let key = format!("{section_key}:{path}");
        let expanded = !collapsed_folders.contains(&key);
        rows.push(ChangeTreeRow::Folder {
            key,
            path: path.clone(),
            name: name.clone(),
            depth,
            count: child.file_count(),
            expanded,
            entries: child.entries(),
        });
        if expanded {
            flatten_change_tree(
                child,
                &path,
                depth + 1,
                collapsed_folders,
                section_key,
                rows,
            );
        }
    }

    rows.extend(
        node.files
            .iter()
            .map(|(name, (index, entry))| ChangeTreeRow::File {
                name: name.clone(),
                depth,
                index: *index,
                entry,
            }),
    );
}

fn group_entries_by_extension(entries: &[StatusEntry]) -> Vec<FileExtensionGroup<'_>> {
    let mut extension_groups = BTreeMap::<String, Vec<(usize, &StatusEntry)>>::new();
    let mut without_extension = Vec::new();

    for (index, entry) in entries.iter().enumerate() {
        let extension = Path::new(&entry.path)
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| !extension.is_empty())
            .map(str::to_ascii_lowercase);

        if let Some(extension) = extension {
            extension_groups
                .entry(extension)
                .or_default()
                .push((index, entry));
        } else {
            without_extension.push((index, entry));
        }
    }

    let mut groups = extension_groups
        .into_iter()
        .map(|(extension, entries)| FileExtensionGroup {
            label: format!(".{extension}"),
            entries,
        })
        .collect::<Vec<_>>();

    if !without_extension.is_empty() {
        groups.push(FileExtensionGroup {
            label: "No extension".to_string(),
            entries: without_extension,
        });
    }

    groups
}

const COMMIT_ANCHOR: &str = "commit-button";
const FILES_ANCHOR: &str = "changes-files";
const FILE_ROW_HEIGHT: f32 = 28.0;

fn section_anchor(staged: bool) -> &'static str {
    if staged {
        "changes-staged"
    } else {
        "changes-unstaged"
    }
}

fn section_header_anchor(staged: bool) -> &'static str {
    if staged {
        "changes-staged-header"
    } else {
        "changes-unstaged-header"
    }
}

/// Where a file heads while the section it moves to has not been
/// painted yet: the top of the file area, where new sections appear.
fn landing_fallback(cx: &mut App) -> Point<Pixels> {
    fx::anchor_bounds(FILES_ANCHOR, cx)
        .map(|files| point(files.center().x, files.top() + px(16.)))
        .unwrap_or_default()
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn flight_tone(to_staged: bool, cx: &App) -> Hsla {
    if to_staged {
        theme::liquid_colors(cx).success
    } else {
        cx.theme().primary
    }
}

/// Moves a file from where it was clicked to the section it moves to.
fn throw_file(from: Point<Pixels>, path: &str, to_staged: bool, cx: &mut App) {
    let target = section_header_anchor(to_staged);
    let fallback = landing_fallback(cx);
    let tone = flight_tone(to_staged, cx);
    fx::glide_to(from, target, fallback, file_name(path), tone, cx);
}

/// Moves one chip that names the batch from one section header to the
/// other.
fn throw_batch(count: usize, to_staged: bool, cx: &mut App) {
    let Some(source) = fx::anchor_bounds(section_header_anchor(!to_staged), cx) else {
        return;
    };
    let target = section_header_anchor(to_staged);
    let fallback = landing_fallback(cx);
    let tone = flight_tone(to_staged, cx);
    let label = if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    };
    fx::glide_to(source.center(), target, fallback, label, tone, cx);
}

/// The visible part of `bounds` inside the file area.
fn visible_in_files(bounds: Bounds<Pixels>, cx: &mut App) -> Option<Bounds<Pixels>> {
    let visible = match fx::anchor_bounds(FILES_ANCHOR, cx) {
        Some(files) => bounds.intersect(&files),
        None => bounds,
    };
    (visible.size.width > Pixels::ZERO && visible.size.height > Pixels::ZERO).then_some(visible)
}

/// The bounds of the unstaged file row under a click.
fn row_bounds_at(position: Point<Pixels>, cx: &mut App) -> Option<Bounds<Pixels>> {
    let section = fx::anchor_bounds(section_anchor(false), cx)?;
    let row = Bounds::new(
        point(section.left(), position.y - px(FILE_ROW_HEIGHT / 2.0)),
        size(section.size.width, px(FILE_ROW_HEIGHT)),
    );
    visible_in_files(row, cx)
}

/// Breaks the visible part of a section into falling pixels.
fn crumble_section(staged: bool, cx: &mut App) {
    let Some(section) = fx::anchor_bounds(section_anchor(staged), cx) else {
        return;
    };
    if let Some(visible) = visible_in_files(section, cx) {
        let colors = theme::liquid_colors(cx);
        fx::crumble(visible, colors.danger, cx);
    }
}

/// The new commit's title springs off the Commit button and flies into
/// the History tab.
fn play_commit_fx(label: String, cx: &mut App) {
    let accent = cx.theme().primary;
    fx::impact(COMMIT_ANCHOR, accent, cx);
    fx::fly_between(
        COMMIT_ANCHOR,
        "tab-history",
        label,
        accent,
        Duration::ZERO,
        cx,
    );
}

fn git_operation_paths(entries: &[StatusEntry]) -> Vec<String> {
    let mut paths = Vec::new();
    for entry in entries {
        if !paths.contains(&entry.path) {
            paths.push(entry.path.clone());
        }
        if let Some(original_path) = &entry.original_path
            && !paths.contains(original_path)
        {
            paths.push(original_path.clone());
        }
    }
    paths
}

fn file_change_stats(additions: u32, deletions: u32, cx: &App) -> Div {
    components::stat_chips(additions, deletions, cx)
}

/// Total added and removed lines in a list of entries.
fn line_totals(entries: &[StatusEntry]) -> (u32, u32) {
    entries
        .iter()
        .fold((0, 0), |(additions, deletions), entry| {
            (additions + entry.additions, deletions + entry.deletions)
        })
}

/// The Changes view of one repository.
pub struct ChangesView {
    state: Entity<RepositoryState>,
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    show_description: bool,
    amend: bool,
    amend_state: Option<AmendState>,
    amend_loading: bool,
    amend_reload_pending: bool,
    amend_generation: u64,
    saved_draft: Option<(String, String, bool)>,
    commit_pending: bool,
    /// The title of the commit being made, which flies into the history
    /// when the commit succeeds.
    commit_flight_label: Option<String>,
    collapsed_tree_folders: HashSet<String>,
    selected: Option<(bool, String)>,
    diff: Entity<DiffView>,
    right_diff_split: Entity<ResizableState>,
    bottom_diff_split: Entity<ResizableState>,
    ai_generating: bool,
    ai_error: Option<String>,
}

/// Events of the Changes view.
#[derive(Debug, Clone)]
pub enum ChangesEvent {
    /// The user asked to open a submodule as its own repository.
    OpenRepository(PathBuf),
}

impl EventEmitter<ChangesEvent> for ChangesView {}

struct FileContextMenuData {
    entry: StatusEntry,
    amend: bool,
    submodule_path: PathBuf,
    repository_path: String,
    hide_checkpoints: bool,
}

struct FolderContextMenuData {
    staged: bool,
    amend: bool,
    path: String,
    entries: Vec<StatusEntry>,
    hide_checkpoints: bool,
}

impl ChangesView {
    /// Creates the Changes view of a repository.
    pub fn new(
        state: Entity<RepositoryState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Commit message..."));
        let description = cx.new(|cx| {
            TextareaState::new(window, cx)
                .multi_line()
                .placeholder("Description (optional)")
        });
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        cx.observe(&title, |_, _, cx| cx.notify()).detach();
        cx.subscribe_in(&state, window, |this, _, event, window, cx| match event {
            RepositoryEvent::StatusRefreshed if this.amend => {
                if this.amend_loading {
                    this.amend_reload_pending = true;
                } else {
                    this.load_amend_state(false, window, cx);
                }
            }
            RepositoryEvent::OperationFinished(notification) if this.commit_pending => {
                this.commit_pending = false;
                let label = this.commit_flight_label.take();
                if notification.status == RepositoryNotificationStatus::Success {
                    if let Some(label) = label {
                        play_commit_fx(label, cx);
                    }
                    this.finish_commit(window, cx);
                } else {
                    let colors = theme::liquid_colors(cx);
                    fx::impact(COMMIT_ANCHOR, colors.danger, cx);
                    cx.notify();
                }
            }
            _ => {}
        })
        .detach();

        let diff = cx.new(|_| DiffView::new());
        diff.update(cx, |diff, _| diff.closable = true);
        cx.subscribe(&diff, |this, _, _: &DiffCloseRequested, cx| {
            this.selected = None;
            this.diff.update(cx, |d, cx| d.clear(cx));
            cx.notify();
        })
        .detach();

        Self {
            state,
            title,
            description,
            show_description: false,
            amend: false,
            amend_state: None,
            amend_loading: false,
            amend_reload_pending: false,
            amend_generation: 0,
            saved_draft: None,
            commit_pending: false,
            commit_flight_label: None,
            collapsed_tree_folders: HashSet::new(),
            selected: None,
            diff,
            right_diff_split: cx.new(|_| ResizableState::default()),
            bottom_diff_split: cx.new(|_| ResizableState::default()),
            ai_generating: false,
            ai_error: None,
        }
    }

    fn set_amend_enabled(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        if enabled == self.amend {
            return;
        }
        if enabled {
            self.saved_draft = Some((
                self.title.read(cx).value().to_string(),
                self.description.read(cx).value().to_string(),
                self.show_description,
            ));
            self.amend = true;
            self.amend_generation = self.amend_generation.wrapping_add(1);
            self.amend_state = None;
            self.amend_reload_pending = false;
            self.selected = None;
            self.diff.update(cx, |diff, cx| diff.clear(cx));
            self.load_amend_state(true, window, cx);
        } else {
            self.disable_amend(true, window, cx);
        }
    }

    fn load_amend_state(
        &mut self,
        load_message: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let repo = self.state.read(cx).repo.clone();
        let generation = self.amend_generation;
        let window_handle = window.window_handle();
        self.amend_loading = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let task = cx
                .background_executor()
                .spawn(async move { repo.amend_state() });
            let result = task.await;
            cx.update_window(window_handle, |_, window, cx| {
                this.update(cx, |this, cx| {
                    if !this.amend || this.amend_generation != generation {
                        return;
                    }
                    this.amend_loading = false;
                    let reload = this.amend_reload_pending;
                    this.amend_reload_pending = false;
                    match result {
                        Ok(amend_state) => {
                            if load_message {
                                this.title.update(cx, |input, cx| {
                                    input.set_value(amend_state.title.clone(), window, cx)
                                });
                                this.description.update(cx, |input, cx| {
                                    input.set_value(amend_state.description.clone(), window, cx)
                                });
                                this.show_description = !amend_state.description.is_empty();
                            }
                            if let Some((true, path)) = &this.selected
                                && !amend_state.staged.iter().any(|entry| &entry.path == path)
                            {
                                this.selected = None;
                                this.diff.update(cx, |diff, cx| diff.clear(cx));
                            }
                            this.amend_state = Some(amend_state);
                        }
                        Err(error) => {
                            let message = format!("Amend data could not load: {error}");
                            this.state.update(cx, |state, cx| {
                                state.last_error = Some(message);
                                cx.notify();
                            });
                            if load_message {
                                this.disable_amend(true, window, cx);
                            }
                        }
                    }
                    if reload && this.amend {
                        this.load_amend_state(false, window, cx);
                    }
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    fn disable_amend(&mut self, restore_draft: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.amend = false;
        self.amend_loading = false;
        self.amend_reload_pending = false;
        self.amend_generation = self.amend_generation.wrapping_add(1);
        self.amend_state = None;
        self.selected = None;
        self.diff.update(cx, |diff, cx| diff.clear(cx));
        if restore_draft {
            if let Some((title, description, show_description)) = self.saved_draft.take() {
                self.title
                    .update(cx, |input, cx| input.set_value(title, window, cx));
                self.description
                    .update(cx, |input, cx| input.set_value(description, window, cx));
                self.show_description = show_description;
            }
        } else {
            self.saved_draft = None;
        }
        cx.notify();
    }

    fn finish_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.disable_amend(false, window, cx);
        self.title
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.description
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.show_description = false;
        cx.notify();
    }

    fn apply_amend_mutation(&mut self, mutation: &StatusMutation, cx: &App) {
        if !self.amend || self.state.read(cx).busy.is_some() {
            return;
        }
        let Some(staged) = self.amend_state.as_ref().map(|state| state.staged.clone()) else {
            return;
        };
        let mut status = self.state.read(cx).status.clone();
        status.staged = staged;
        mutation.apply(&mut status);
        if let Some(amend_state) = &mut self.amend_state {
            amend_state.staged = status.staged;
        }
    }

    fn generate_commit_message(
        &mut self,
        target: CommitGenerationTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = GitSettings::get(cx).clone();
        let repo = self.state.read(cx).repo.clone();
        let amend = self.amend;
        let scope = ChangeScope::from_include_unstaged(settings.ai_include_unstaged);
        let window_handle = window.window_handle();
        let title_input = self.title.clone();
        let description_input = self.description.clone();
        self.ai_generating = true;
        self.ai_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let task = cx.background_executor().spawn(async move {
                let context =
                    ChangeContext::from_repo(&repo, amend, settings.ai_include_whole_file, scope)?;
                AiService::new(AiConfig::from(&settings)).generate_commit(&context, target)
            });
            let result = task.await;
            if let Ok(message) = &result {
                let message = message.clone();
                cx.update_window(window_handle, |_, window, cx| match target {
                    CommitGenerationTarget::Title => title_input
                        .update(cx, |input, cx| input.set_value(message.title, window, cx)),
                    CommitGenerationTarget::Description => description_input
                        .update(cx, |input, cx| {
                            input.set_value(message.description, window, cx)
                        }),
                })
                .ok();
            }
            this.update(cx, |this, cx| {
                this.ai_generating = false;
                match result {
                    Ok(_) => {
                        if target == CommitGenerationTarget::Description {
                            this.show_description = true;
                        }
                    }
                    Err(error) => this.ai_error = Some(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn review_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = GitSettings::get(cx).clone();
        let repo = self.state.read(cx).repo.clone();
        let amend = self.amend;
        let scope = ChangeScope::from_include_unstaged(settings.ai_include_unstaged);
        let detail = match scope {
            ChangeScope::Staged => "Staged changes",
            ChangeScope::StagedAndUnstaged => "Staged and unstaged changes",
        };
        let dialog = open_ai_result_dialog("AI Code Review", detail, window, cx);
        run_ai_result_task(
            dialog,
            move |progress| {
                let context = ChangeContext::from_repo(&repo, amend, false, scope)?;
                AiService::new(AiConfig::from(&settings)).review_changes(&context, progress)
            },
            AiResult::CodeReview,
            cx,
        );
    }

    fn explain_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = GitSettings::get(cx).clone();
        let repo = self.state.read(cx).repo.clone();
        let amend = self.amend;
        let scope = ChangeScope::from_include_unstaged(settings.ai_include_unstaged);
        let detail = match scope {
            ChangeScope::Staged => "Staged changes",
            ChangeScope::StagedAndUnstaged => "Staged and unstaged changes",
        };
        let dialog = open_ai_result_dialog("Explain Changes", detail, window, cx);
        run_ai_result_task(
            dialog,
            move |progress| {
                let context = ChangeContext::from_repo(&repo, amend, false, scope)?;
                AiService::new(AiConfig::from(&settings)).explain_diff(
                    &context.diff,
                    None,
                    None,
                    progress,
                )
            },
            AiResult::Explanation,
            cx,
        );
    }

    fn hide_checkpoints(cx: &App) -> bool {
        GitSettings::get(cx).hide_checkpoint_commits
    }

    fn visible_entries(&self, staged: bool, cx: &App) -> Vec<StatusEntry> {
        let state = self.state.read(cx);
        let settings = GitSettings::get(cx);
        let repo_path = state.repo.path.display().to_string();
        let ignored = settings.ignored_for(&repo_path);
        let source = if staged && self.amend {
            self.amend_state
                .as_ref()
                .map(|state| state.staged.as_slice())
                .unwrap_or_default()
        } else if staged {
            &state.status.staged
        } else {
            &state.status.unstaged
        };
        source
            .iter()
            .filter(|e| !ignored.iter().any(|p| &e.path == p))
            .cloned()
            .collect()
    }

    fn select_entry(&mut self, entry: &StatusEntry, cx: &mut Context<Self>) {
        self.selected = Some((entry.staged, entry.path.clone()));
        let repo = self.state.read(cx).repo.clone();
        let context = GitSettings::get(cx).diff_context_lines;
        let path = entry.path.clone();
        let original_path = entry.original_path.clone();
        let staged = entry.staged;
        let amend = self.amend && staged;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let task = {
                let path = path.clone();
                cx.background_executor().spawn(async move {
                    let diff = if amend {
                        repo.diff_amend_file(&path, original_path.as_deref(), context)
                    } else {
                        repo.diff_file(&path, original_path.as_deref(), staged, context)
                    }
                    .unwrap_or_default();
                    PreparedDiff::new(path, &diff)
                })
            };
            let diff = task.await;
            this.update(cx, |this, cx| {
                if this.selected.as_ref().map(|(s, p)| (*s, p.as_str()))
                    != Some((staged, path.as_str()))
                {
                    return;
                }
                this.diff.update(cx, |view, cx| view.set_diff(diff, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn commit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().trim().to_string();
        if title.is_empty()
            || self.amend_loading
            || self.commit_pending
            || self.state.read(cx).busy.is_some()
        {
            return;
        }
        let description = self.description.read(cx).value().trim().to_string();
        let amend = self.amend;
        let hide = Self::hide_checkpoints(cx);
        self.commit_pending = true;
        self.commit_flight_label = Some(title.clone());
        self.state.update(cx, |state, cx| {
            state.run_operation(
                if amend { "Amending" } else { "Committing" },
                hide,
                move |repo| repo.commit(&title, &description, amend),
                cx,
            );
        });
        cx.notify();
    }

    fn file_context_menu(
        menu: PopupMenu,
        changes: Entity<Self>,
        state: Entity<RepositoryState>,
        data: FileContextMenuData,
    ) -> PopupMenu {
        let FileContextMenuData {
            entry,
            amend,
            submodule_path,
            repository_path,
            hide_checkpoints,
        } = data;
        // Read from disk when the menu opens, not for every row on every render.
        let submodule_available = submodule_path.is_dir() && submodule_path.join(".git").exists();
        let staged = entry.staged;
        let conflicted = entry.kind == FileChangeKind::Conflicted;
        let is_submodule = entry.is_submodule;
        let path = entry.path.clone();
        let file_name = Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());

        let action_state = state.clone();
        let action_changes = changes.clone();
        let action_entry = entry.clone();
        let action_paths = git_operation_paths(std::slice::from_ref(&entry));
        let mut menu = menu.item(
            PopupMenuItem::new(if conflicted {
                "Mark as Resolved"
            } else if staged && amend {
                "Remove from Amend"
            } else if staged {
                "Unstage"
            } else {
                "Stage"
            })
            .icon(if staged {
                IconName::Minus
            } else {
                IconName::Plus
            })
            .on_click(move |_, _, cx| {
                let paths = action_paths.clone();
                let mutation = if staged {
                    StatusMutation::UnstageEntries(vec![action_entry.clone()])
                } else {
                    StatusMutation::StagePaths(paths.clone())
                };
                action_changes.update(cx, |changes, cx| {
                    changes.apply_amend_mutation(&mutation, cx);
                    cx.notify();
                });
                action_state.update(cx, |state, cx| {
                    state.run_status_operation(
                        if conflicted {
                            "Marking conflict as resolved"
                        } else if staged {
                            "Unstaging"
                        } else {
                            "Staging"
                        },
                        hide_checkpoints,
                        mutation,
                        move |repo| {
                            if staged {
                                if amend {
                                    repo.unstage_amend_paths(&paths)
                                } else {
                                    repo.unstage_paths(&paths)
                                }
                            } else {
                                repo.stage_paths(&paths)
                            }
                        },
                        cx,
                    );
                });
            }),
        );

        if is_submodule {
            let changes = changes.clone();
            menu = menu.item(
                PopupMenuItem::new("Open Submodule Repository")
                    .icon(IconName::ExternalLink)
                    .disabled(!submodule_available)
                    .on_click(move |_, _, cx| {
                        let path = submodule_path.clone();
                        changes.update(cx, |_, cx| {
                            cx.emit(ChangesEvent::OpenRepository(path));
                        });
                    }),
            );
        } else if !conflicted {
            let stash_state = state.clone();
            let stash_paths = git_operation_paths(std::slice::from_ref(&entry));
            let include_untracked = entry.kind == FileChangeKind::Untracked;
            menu =
                menu.separator()
                    .item(PopupMenuItem::new("Stash").icon(icons::archive()).on_click(
                        move |_, _, cx| {
                            let paths = stash_paths.clone();
                            stash_state.update(cx, |state, cx| {
                                state.run_operation(
                                    "Stashing",
                                    hide_checkpoints,
                                    move |repo| repo.stash_paths(&paths, include_untracked),
                                    cx,
                                );
                            });
                        },
                    ));
        }

        if !staged && !conflicted {
            let discard_state = state.clone();
            let discard_changes = changes.clone();
            let discard_entry = entry.clone();
            let discard_paths = git_operation_paths(std::slice::from_ref(&entry));
            let message = if entry.kind == FileChangeKind::Untracked {
                "This untracked file will be deleted from disk. This action cannot be undone."
            } else {
                "All unstaged changes in this file will be lost. This action cannot be undone."
            };
            let target = SharedString::from(path.clone());
            menu = menu.separator().item(
                PopupMenuItem::new("Discard Changes")
                    .icon(icons::trash())
                    .on_click(move |_, window, cx| {
                        let state = discard_state.clone();
                        let changes = discard_changes.clone();
                        let entry = discard_entry.clone();
                        let paths = discard_paths.clone();
                        confirm_destructive_action(
                            "Discard Changes?",
                            message,
                            Some(target.clone()),
                            "Discard File",
                            move |cx| {
                                let mutation = StatusMutation::DiscardPaths(paths);
                                changes.update(cx, |changes, cx| {
                                    changes.apply_amend_mutation(&mutation, cx);
                                    cx.notify();
                                });
                                state.update(cx, |state, cx| {
                                    state.run_status_operation(
                                        "Discarding",
                                        hide_checkpoints,
                                        mutation,
                                        move |repo| repo.discard(&entry),
                                        cx,
                                    );
                                });
                            },
                            window,
                            cx,
                        );
                    }),
            );
        }

        let copy_path = path.clone();
        let copy_name = file_name;
        menu = menu
            .separator()
            .item(
                PopupMenuItem::new("Copy Path")
                    .icon(IconName::Copy)
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()));
                    }),
            )
            .item(
                PopupMenuItem::new("Copy Filename")
                    .icon(IconName::Copy)
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_name.clone()));
                    }),
            );

        if !is_submodule {
            let ignored_path = path;
            let ignored_state = state;
            let ignored_changes = changes;
            let ignored_paths = git_operation_paths(std::slice::from_ref(&entry));
            menu = menu.separator().item(
                PopupMenuItem::new("Add to Local Ignore")
                    .icon(IconName::EyeOff)
                    .on_click(move |_, _, cx| {
                        let path = ignored_path.clone();
                        let repository_path = repository_path.clone();
                        GitSettings::update(cx, |settings| {
                            let ignored =
                                settings.local_ignored.entry(repository_path).or_default();
                            if !ignored.contains(&path) {
                                ignored.push(path);
                            }
                        });
                        if staged {
                            let paths = ignored_paths.clone();
                            let mutation = StatusMutation::UnstageEntries(vec![entry.clone()]);
                            ignored_changes.update(cx, |changes, cx| {
                                changes.apply_amend_mutation(&mutation, cx);
                                cx.notify();
                            });
                            ignored_state.update(cx, |state, cx| {
                                state.run_status_operation(
                                    "Unstaging ignored file",
                                    hide_checkpoints,
                                    mutation,
                                    move |repo| {
                                        if amend {
                                            repo.unstage_amend_paths(&paths)
                                        } else {
                                            repo.unstage_paths(&paths)
                                        }
                                    },
                                    cx,
                                );
                            });
                        }
                    }),
            );
        }

        menu
    }

    fn folder_context_menu(
        menu: PopupMenu,
        changes: Entity<Self>,
        state: Entity<RepositoryState>,
        data: FolderContextMenuData,
    ) -> PopupMenu {
        let FolderContextMenuData {
            staged,
            amend,
            path,
            entries,
            hide_checkpoints,
        } = data;
        let action_state = state.clone();
        let action_changes = changes.clone();
        let action_entries = entries.clone();
        let action_paths = git_operation_paths(&entries);
        let mut menu = menu.item(
            PopupMenuItem::new(if staged && amend {
                "Remove Folder from Amend"
            } else if staged {
                "Unstage Folder"
            } else {
                "Stage Folder"
            })
            .icon(if staged {
                IconName::Minus
            } else {
                IconName::Plus
            })
            .on_click(move |_, _, cx| {
                let paths = action_paths.clone();
                let mutation = if staged {
                    StatusMutation::UnstageEntries(action_entries.clone())
                } else {
                    StatusMutation::StagePaths(paths.clone())
                };
                action_changes.update(cx, |changes, cx| {
                    changes.apply_amend_mutation(&mutation, cx);
                    cx.notify();
                });
                action_state.update(cx, |state, cx| {
                    state.run_status_operation(
                        if staged {
                            "Unstaging folder"
                        } else {
                            "Staging folder"
                        },
                        hide_checkpoints,
                        mutation,
                        move |repo| {
                            if staged {
                                if amend {
                                    repo.unstage_amend_paths(&paths)
                                } else {
                                    repo.unstage_paths(&paths)
                                }
                            } else {
                                repo.stage_paths(&paths)
                            }
                        },
                        cx,
                    );
                });
            }),
        );

        if !staged {
            let discard_state = state.clone();
            let discard_changes = changes;
            let discard_entries = entries.clone();
            let discard_paths = git_operation_paths(&entries);
            let count = entries.len();
            let untracked_count = entries
                .iter()
                .filter(|entry| entry.kind == FileChangeKind::Untracked)
                .count();
            let message = if untracked_count == 0 {
                "All unstaged changes in this folder will be lost. This action cannot be undone."
                    .to_string()
            } else {
                format!(
                    "All unstaged changes in this folder will be lost, and {untracked_count} untracked files will be deleted from disk. This action cannot be undone."
                )
            };
            let target = SharedString::from(format!("{path} · {count} files"));
            menu = menu.separator().item(
                PopupMenuItem::new("Discard Folder Changes")
                    .icon(icons::trash())
                    .on_click(move |_, window, cx| {
                        let state = discard_state.clone();
                        let changes = discard_changes.clone();
                        let entries = discard_entries.clone();
                        let paths = discard_paths.clone();
                        let message = message.clone();
                        confirm_destructive_action(
                            "Discard Folder Changes?",
                            message,
                            Some(target.clone()),
                            "Discard Folder",
                            move |cx| {
                                let mutation = StatusMutation::DiscardPaths(paths);
                                changes.update(cx, |changes, cx| {
                                    changes.apply_amend_mutation(&mutation, cx);
                                    cx.notify();
                                });
                                state.update(cx, |state, cx| {
                                    state.run_status_operation(
                                        "Discarding folder changes",
                                        hide_checkpoints,
                                        mutation,
                                        move |repo| repo.discard_entries(&entries),
                                        cx,
                                    );
                                });
                            },
                            window,
                            cx,
                        );
                    }),
            );
        }

        let copy_path = path;
        menu.separator().item(
            PopupMenuItem::new("Copy Folder Path")
                .icon(IconName::Copy)
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()));
                }),
        )
    }

    fn render_entry_row(
        &mut self,
        ix: usize,
        entry: &StatusEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_named_entry_row(ix, entry, entry.path.clone(), 0, cx)
    }

    fn render_named_entry_row(
        &mut self,
        ix: usize,
        entry: &StatusEntry,
        display_name: String,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let staged = entry.staged;
        let conflicted = entry.kind == FileChangeKind::Conflicted;
        let selected = self.selected.as_ref().map(|(s, p)| (*s, p.as_str()))
            == Some((staged, entry.path.as_str()));
        let entry_for_click = entry.clone();
        let entry_for_action = entry.clone();
        let entry_for_discard = entry.clone();
        let entry_for_menu = entry.clone();
        let changes_for_menu = cx.entity();
        let state_for_menu = self.state.clone();
        let repository_root = self.state.read(cx).repo.path.clone();
        let repository_path = repository_root.display().to_string();
        let submodule_path = repository_root.join(&entry.path);
        let amend = self.amend;
        let hide = Self::hide_checkpoints(cx);
        let id = if conflicted {
            ("conflicted-file", ix)
        } else if staged {
            ("staged-file", ix)
        } else {
            ("unstaged-file", ix)
        };
        let discard_id = if conflicted {
            ("discard-conflict", ix)
        } else {
            ("discard", ix)
        };
        let stage_id = if conflicted {
            ("resolve-conflict", ix)
        } else {
            ("stage-toggle", ix)
        };

        let accent = cx.theme().primary;
        div()
            .id(id)
            .group("file-row")
            .relative()
            .w_full()
            .h(px(28.))
            .pl(px(8.0 + depth as f32 * 14.0))
            .pr_1p5()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .rounded(px(RADIUS_CONTROL))
            .overflow_hidden()
            .when(selected, |s| components::selected_row(s, accent))
            .when(!selected, |s| {
                s.hover(|s| s.bg(cx.theme().list_hover))
                    .active(|s| s.bg(cx.theme().button_active))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_entry(&entry_for_click, cx);
            }))
            .child(change_kind_chip(entry.kind, cx))
            .child(components::file_path_label(&display_name, cx))
            .when(entry.additions > 0 || entry.deletions > 0, |this| {
                this.child(file_change_stats(entry.additions, entry.deletions, cx))
            })
            .when(!staged && !conflicted, |this| {
                this.child(
                    button(discard_id)
                        .ghost()
                        .xsmall()
                        .icon(icons::trash())
                        .tooltip("Discard changes")
                        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            let row = row_bounds_at(event.position(), cx);
                            let entry = entry_for_discard.clone();
                            let hide = Self::hide_checkpoints(cx);
                            let state = this.state.clone();
                            let changes = cx.entity();
                            let paths = git_operation_paths(std::slice::from_ref(&entry));
                            let message = if entry.kind == FileChangeKind::Untracked {
                                "This untracked file will be deleted from disk. This action cannot be undone."
                            } else {
                                "All unstaged changes in this file will be lost. This action cannot be undone."
                            };
                            let target = SharedString::from(entry.path.clone());
                            confirm_destructive_action(
                                "Discard Changes?",
                                message,
                                Some(target),
                                "Discard File",
                                move |cx| {
                                    if let Some(row) = row {
                                        let colors = theme::liquid_colors(cx);
                                        fx::crumble(row, colors.danger, cx);
                                    }
                                    let mutation = StatusMutation::DiscardPaths(paths.clone());
                                    changes.update(cx, |changes, cx| {
                                        changes.apply_amend_mutation(&mutation, cx);
                                        cx.notify();
                                    });
                                    state.update(cx, |state, cx| {
                                        state.run_status_operation(
                                            "Discarding",
                                            hide,
                                            mutation,
                                            move |repo| repo.discard(&entry),
                                            cx,
                                        );
                                    });
                                },
                                window,
                                cx,
                            );
                        })),
                )
            })
            .child(
                button(stage_id)
                    .ghost()
                    .xsmall()
                    .icon(if staged {
                        IconName::Minus
                    } else {
                        IconName::Plus
                    })
                    .tooltip(if conflicted {
                        "Mark as resolved"
                    } else if staged && amend {
                        "Remove from amend"
                    } else if staged {
                        "Unstage"
                    } else {
                        "Stage"
                    })
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        let entry = entry_for_action.clone();
                        throw_file(event.position(), &entry.path, !staged, cx);
                        let paths = git_operation_paths(std::slice::from_ref(&entry));
                        let mutation = if staged {
                            StatusMutation::UnstageEntries(vec![entry.clone()])
                        } else {
                            StatusMutation::StagePaths(paths)
                        };
                        this.apply_amend_mutation(&mutation, cx);
                        this.state.update(cx, |state, cx| {
                            state.run_status_operation(
                                if conflicted {
                                    "Marking conflict as resolved"
                                } else if staged {
                                    "Unstaging"
                                } else {
                                    "Staging"
                                },
                                hide,
                                mutation,
                                move |repo| {
                                    if staged {
                                        if amend {
                                            repo.unstage_amend_paths(std::slice::from_ref(
                                                &entry.path,
                                            ))
                                        } else {
                                            repo.unstage(&entry.path)
                                        }
                                    } else {
                                        repo.stage(&entry.path)
                                    }
                                },
                                cx,
                            );
                        });
                    })),
            )
            .context_menu(move |menu, _, _| {
                Self::file_context_menu(
                    menu,
                    changes_for_menu.clone(),
                    state_for_menu.clone(),
                    FileContextMenuData {
                        entry: entry_for_menu.clone(),
                        amend,
                        submodule_path: submodule_path.clone(),
                        repository_path: repository_path.clone(),
                        hide_checkpoints: hide,
                    },
                )
            })
            .into_any_element()
    }

    fn render_extension_groups(
        &mut self,
        entries: &[StatusEntry],
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        group_entries_by_extension(entries)
            .into_iter()
            .map(|group| {
                v_flex()
                    .w_full()
                    .child(div().w_full().pl_2().pr_1().pt_2().pb_1().child(
                        components::section_header(group.label, Some(group.entries.len()), cx),
                    ))
                    .children(
                        group
                            .entries
                            .into_iter()
                            .map(|(index, entry)| self.render_entry_row(index, entry, cx))
                            .collect::<Vec<_>>(),
                    )
            })
            .collect()
    }

    fn render_tree_entries(
        &mut self,
        entries: &[StatusEntry],
        section_key: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        let rows = change_tree_rows(entries, &self.collapsed_tree_folders, section_key);
        v_flex().w_full().children(rows.into_iter().map(|row| {
            match row {
                ChangeTreeRow::Folder {
                    key,
                    path,
                    name,
                    depth,
                    count,
                    expanded,
                    entries,
                } => {
                    let key_for_click = key.clone();
                    let changes_for_menu = cx.entity();
                    let state_for_menu = self.state.clone();
                    let staged = section_key == "staged";
                    let amend = self.amend;
                    let hide = Self::hide_checkpoints(cx);
                    div()
                        .id(SharedString::from(format!("changes-tree-folder:{key}")))
                        .w_full()
                        .h(px(26.))
                        .pl(px(8.0 + depth as f32 * 14.0))
                        .pr_2()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .rounded(px(RADIUS_CONTROL))
                        .hover(|style| style.bg(cx.theme().list_hover))
                        .active(|style| style.bg(cx.theme().button_active))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.collapsed_tree_folders.contains(&key_for_click) {
                                this.collapsed_tree_folders.remove(&key_for_click);
                            } else {
                                this.collapsed_tree_folders.insert(key_for_click.clone());
                            }
                            cx.notify();
                        }))
                        .child(
                            Icon::new(if expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            Icon::new(IconName::Folder)
                                .small()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(text_px(11.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(SharedString::from(name)),
                        )
                        .child(count_badge(count, cx))
                        .context_menu(move |menu, _, _| {
                            Self::folder_context_menu(
                                menu,
                                changes_for_menu.clone(),
                                state_for_menu.clone(),
                                FolderContextMenuData {
                                    staged,
                                    amend,
                                    path: path.clone(),
                                    entries: entries.clone(),
                                    hide_checkpoints: hide,
                                },
                            )
                        })
                        .into_any_element()
                }
                ChangeTreeRow::File {
                    name,
                    depth,
                    index,
                    entry,
                } => self
                    .render_named_entry_row(index, entry, name, depth, cx)
                    .into_any_element(),
            }
        }))
    }

    fn render_entries(
        &mut self,
        entries: &[StatusEntry],
        section_key: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        match GitSettings::get(cx).changes_render_mode {
            ChangesRenderMode::Extensions => v_flex()
                .w_full()
                .children(self.render_extension_groups(entries, cx)),
            ChangesRenderMode::Tree => self.render_tree_entries(entries, section_key, cx),
        }
    }

    fn render_section(
        &mut self,
        label: &'static str,
        staged: bool,
        entries: Vec<StatusEntry>,
        cx: &mut Context<Self>,
    ) -> Div {
        let hide = Self::hide_checkpoints(cx);
        let entry_count = entries.len();
        let untracked_count = entries
            .iter()
            .filter(|entry| entry.kind == FileChangeKind::Untracked)
            .count();
        let colors = theme::liquid_colors(cx);
        // Staged files sit on a green band and unstaged files on a neutral
        // band, so the two groups stay apart at a glance.
        let (tone, band) = if staged {
            (colors.success, colors.success.opacity(0.06))
        } else {
            (
                colors.text_secondary,
                cx.theme().muted_foreground.opacity(0.035),
            )
        };
        v_flex()
            .relative()
            .w_full()
            .px_1p5()
            .pb_2()
            .bg(band)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(fx::anchor(section_anchor(staged)))
            .child(
                h_flex()
                    .relative()
                    .child(fx::anchor(section_header_anchor(staged)))
                    .w_full()
                    .pl_2()
                    .pr_0p5()
                    .pt_2p5()
                    .pb_1p5()
                    .gap_1p5()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(components::status_section_header(
                                label,
                                entry_count,
                                tone,
                                cx,
                            )),
                    )
                    .when(!staged && !entries.is_empty(), |this| {
                        this.child(
                            button("discard-all")
                                .outline()
                                .danger()
                                .xsmall()
                                .icon(icons::trash())
                                .label("Discard All")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let hide = Self::hide_checkpoints(cx);
                                    let state = this.state.clone();
                                    let changes = cx.entity();
                                    let message = if untracked_count == 0 {
                                        "All unstaged changes will be lost. This action cannot be undone."
                                            .to_string()
                                    } else {
                                        let untracked_files = if untracked_count == 1 {
                                            "1 untracked file".to_string()
                                        } else {
                                            format!("{untracked_count} untracked files")
                                        };
                                        format!(
                                            "All unstaged changes will be lost, and {untracked_files} will be deleted from disk. This action cannot be undone."
                                        )
                                    };
                                    let target = SharedString::from(if entry_count == 1 {
                                        "1 file".to_string()
                                    } else {
                                        format!("{entry_count} files")
                                    });
                                    confirm_destructive_action(
                                        "Discard All Changes?",
                                        message,
                                        Some(target),
                                        "Discard All",
                                        move |cx| {
                                            crumble_section(false, cx);
                                            let mutation = StatusMutation::DiscardAll;
                                            changes.update(cx, |changes, cx| {
                                                changes.apply_amend_mutation(&mutation, cx);
                                                cx.notify();
                                            });
                                            state.update(cx, |state, cx| {
                                                state.run_status_operation(
                                                    "Discarding all",
                                                    hide,
                                                    mutation,
                                                    |repo| repo.discard_all(),
                                                    cx,
                                                );
                                            });
                                        },
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            button("stage-all")
                                .outline()
                                .xsmall()
                                .icon(IconName::Plus)
                                .label("Stage All")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    throw_batch(entry_count, true, cx);
                                    let mutation = StatusMutation::StageAll;
                                    this.apply_amend_mutation(&mutation, cx);
                                    this.state.update(cx, |state, cx| {
                                        state.run_status_operation(
                                            "Staging all",
                                            hide,
                                            mutation,
                                            |repo| repo.stage_all(),
                                            cx,
                                        );
                                    });
                                })),
                        )
                    })
                    .when(staged && !entries.is_empty(), |this| {
                        this.child(
                            button("unstage-all")
                                .outline()
                                .xsmall()
                                .icon(IconName::Minus)
                                .label("Unstage All")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    throw_batch(entry_count, false, cx);
                                    let amend = this.amend;
                                    let mutation = if amend {
                                        StatusMutation::UnstageEntries(
                                            this.amend_state
                                                .as_ref()
                                                .map(|state| state.staged.clone())
                                                .unwrap_or_default(),
                                        )
                                    } else {
                                        StatusMutation::UnstageAll
                                    };
                                    this.apply_amend_mutation(&mutation, cx);
                                    this.state.update(cx, |state, cx| {
                                        state.run_status_operation(
                                            "Unstaging all",
                                            hide,
                                            mutation,
                                            move |repo| {
                                                if amend {
                                                    repo.unstage_amend_all()
                                                } else {
                                                    repo.unstage_all()
                                                }
                                            },
                                            cx,
                                        );
                                    });
                                })),
                        )
                    }),
            )
            .when(entries.is_empty(), |this| {
                this.child(
                    div()
                        .px_2()
                        .py_2()
                        .font_family(mono_font())
                        .text_size(text_px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child(if staged {
                            "No staged changes"
                        } else {
                            "No changes"
                        }),
                )
            })
            .child(self.render_entries(
                &entries,
                if staged { "staged" } else { "unstaged" },
                cx,
            ))
    }

    fn render_conflicts_section(
        &mut self,
        entries: Vec<StatusEntry>,
        operation: Option<ConflictOperation>,
        cx: &mut Context<Self>,
    ) -> Div {
        let count = entries.len();
        let hide = Self::hide_checkpoints(cx);
        let busy = self.state.read(cx).busy.is_some();
        let action_label = operation.map_or("Clear Conflicts", ConflictOperation::abort_label);
        let title = operation.map_or_else(
            || "Clear Conflicts?".to_string(),
            |operation| format!("Abort {}?", operation.label()),
        );
        let message = operation.map_or_else(
            || {
                "The unmerged index will be reset to HEAD. Changes in conflicted files will be lost."
                    .to_string()
            },
            |operation| {
                format!(
                    "The {} will stop. Git will restore the repository state from before the {}.",
                    operation.label().to_lowercase(),
                    operation.label().to_lowercase()
                )
            },
        );
        let target = if count == 1 {
            SharedString::from("1 conflicted file")
        } else {
            SharedString::from(format!("{count} conflicted files"))
        };

        let warning = theme::liquid_colors(cx).warning;
        v_flex()
            .w_full()
            .px_1p5()
            .pb_2()
            .bg(warning.opacity(0.06))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .w_full()
                    .pl_2()
                    .pr_0p5()
                    .pt_2p5()
                    .pb_1p5()
                    .gap_1p5()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(components::status_section_header(
                                "Conflicts",
                                count,
                                warning,
                                cx,
                            )),
                    )
                    .child(
                        button("abort-conflict-operation")
                            .danger()
                            .xsmall()
                            .label(action_label)
                            .disabled_readable(busy, cx)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let state = this.state.clone();
                                let title = title.clone();
                                let message = message.clone();
                                let target = target.clone();
                                confirm_destructive_action(
                                    title,
                                    message,
                                    Some(target),
                                    action_label,
                                    move |cx| {
                                        state.update(cx, |state, cx| {
                                            state.run_operation(
                                                "Aborting conflict operation",
                                                hide,
                                                |repo| repo.abort_conflict_operation(),
                                                cx,
                                            );
                                        });
                                    },
                                    window,
                                    cx,
                                );
                            })),
                    ),
            )
            .child(self.render_entries(&entries, "conflicts", cx))
    }

    /// The Amend switch with its label. Clicking the label toggles the switch too.
    fn render_amend_switch(&self, busy: bool, cx: &mut Context<Self>) -> Div {
        let amend = self.amend;
        let disabled = busy || self.amend_loading;
        h_flex()
            .flex_shrink_0()
            .gap_2()
            .child(
                Switch::new("amend", amend)
                    .label("Amend")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, checked: &bool, window, cx| {
                        this.set_amend_enabled(*checked, window, cx);
                    })),
            )
            .child(
                div()
                    .id("amend-label")
                    .text_size(text_px(12.5))
                    .when(disabled, |label| label.opacity(0.5))
                    .when(!disabled, |label| {
                        label
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_amend_enabled(!amend, window, cx);
                            }))
                    })
                    .child("Amend"),
            )
    }

    fn render_commit_box(&mut self, cx: &mut Context<Self>) -> Div {
        let settings = GitSettings::get(cx);
        let limit = settings.commit_title_limit;
        let title_len = self.title.read(cx).value().chars().count();
        let staged_count = self.visible_entries(true, cx).len();
        let unstaged_count = self.visible_entries(false, cx).len();
        let over = title_len > limit as usize;
        let colors = theme::liquid_colors(cx);
        let can_commit = title_len > 0
            && (staged_count > 0 || self.amend)
            && !self.amend_loading
            && !self.commit_pending;
        let busy = self.state.read(cx).busy.is_some();
        let ai_generating = self.ai_generating;
        let ai_error = self.ai_error.clone();
        let ai_commit_enabled = settings.ai_commit_messages;
        let ai_review_enabled = settings.ai_code_review;
        let ai_explain_enabled = settings.ai_explain_diffs;
        let ai_description_enabled = settings.ai_generate_description;
        let include_unstaged = settings.ai_include_unstaged;
        let has_ai_context =
            staged_count > 0 || self.amend || (include_unstaged && unstaged_count > 0);
        let can_generate = !ai_generating && !busy && has_ai_context;
        let can_review = !busy && has_ai_context;
        let can_explain = !busy && has_ai_context;
        let ai_menu_enabled = ai_commit_enabled || ai_review_enabled || ai_explain_enabled;
        let changes = cx.entity();

        let accent = cx.theme().primary;
        let used = title_len.min(limit as usize) as f32;
        let remaining = (limit as usize).saturating_sub(title_len) as f32;
        let title_input = div()
            .relative()
            .w_full()
            .child(
                text_field(&self.title).prefix(
                    div()
                        .font_family(mono_font())
                        .font_weight(FontWeight::BOLD)
                        .text_color(if over { colors.danger } else { accent })
                        .child("›"),
                ),
            )
            .when(ai_generating, |input| {
                // A flowing shimmer over the field while AI writes a message.
                input.child(
                    Dither::new(DitherField::Flow {
                        scale: 0.14,
                        speed: 6.0,
                        bias: -0.15,
                    })
                    .color(accent.opacity(0.45))
                    .grid(3.0, 2.0)
                    .animated()
                    .corner_radius(f32::from(cx.theme().radius))
                    .layer(),
                )
            });

        v_flex()
            .w_full()
            .gap_2()
            .px_3()
            .pt_2p5()
            .pb_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(colors.panel_secondary)
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(components::mono_label(
                        if self.amend { "Amend commit" } else { "Commit" },
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(components::mono_label(format!("{staged_count} staged"), cx))
                    .child(div().relative().w(px(42.)).h(px(5.)).child(DitherBar::new(
                        vec![
                            BarSegment {
                                weight: used,
                                color: if over { colors.danger } else { accent },
                            },
                            BarSegment {
                                weight: remaining,
                                color: cx.theme().muted_foreground.opacity(0.25),
                            },
                        ],
                        cx.theme().muted_foreground.opacity(0.25),
                    )))
                    .child(
                        div()
                            .font_family(mono_font())
                            .text_size(text_px(10.5))
                            .text_color(if over {
                                colors.danger
                            } else {
                                colors.text_tertiary
                            })
                            .child(format!("{title_len}/{limit}")),
                    ),
            )
            .child(title_input)
            .when_some(ai_error, |this, error| {
                this.child(
                    div()
                        .w_full()
                        .p_2()
                        .rounded(px(RADIUS_CONTROL))
                        .bg(colors.danger.opacity(0.12))
                        .text_size(text_px(11.5))
                        .text_color(colors.danger)
                        .child(error),
                )
            })
            .when(self.show_description, |this| {
                this.child(
                    Textarea::new(&self.description)
                        .h(px(80.))
                        .font_family(mono_font()),
                )
            })
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        button("ai-actions")
                            .outline()
                            .small()
                            .icon(icons::sparkles())
                            .label("AI")
                            .loading(ai_generating)
                            .dropdown_caret(true)
                            .disabled(!ai_menu_enabled)
                            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                                let generate_title = changes.clone();
                                let generate_description = changes.clone();
                                let explain_changes = changes.clone();
                                let review_changes = changes.clone();
                                menu.min_w(px(220.))
                                    .item(PopupMenuItem::element(move |_, cx| {
                                        let include_unstaged =
                                            GitSettings::get(cx).ai_include_unstaged;
                                        h_flex()
                                            .id("ai-include-unstaged-row")
                                            .w_full()
                                            .min_h(px(32.))
                                            .items_center()
                                            .justify_between()
                                            .child("Include unstaged")
                                            .child(
                                                Switch::new(
                                                    "ai-include-unstaged",
                                                    include_unstaged,
                                                )
                                                .label("Include unstaged"),
                                            )
                                            .on_click(|_, _, cx| {
                                                cx.stop_propagation();
                                                let next =
                                                    !GitSettings::get(cx).ai_include_unstaged;
                                                GitSettings::update_saved(cx, |settings| {
                                                    settings.ai_include_unstaged = next;
                                                });
                                                cx.refresh_windows();
                                            })
                                    }))
                                    .separator()
                                    .item(
                                        PopupMenuItem::new("Generate title")
                                            .icon(icons::sparkles())
                                            .disabled(!ai_commit_enabled || !can_generate)
                                            .on_click(move |_, window, cx| {
                                                generate_title.update(cx, |this, cx| {
                                                    this.generate_commit_message(
                                                        CommitGenerationTarget::Title,
                                                        window,
                                                        cx,
                                                    )
                                                });
                                            }),
                                    )
                                    .item(
                                        PopupMenuItem::new("Generate description")
                                            .icon(icons::pencil())
                                            .disabled(
                                                !ai_commit_enabled
                                                    || !ai_description_enabled
                                                    || !can_generate,
                                            )
                                            .on_click(move |_, window, cx| {
                                                generate_description.update(cx, |this, cx| {
                                                    this.generate_commit_message(
                                                        CommitGenerationTarget::Description,
                                                        window,
                                                        cx,
                                                    )
                                                });
                                            }),
                                    )
                                    .item(
                                        PopupMenuItem::new("Explain changes")
                                            .icon(IconName::Info)
                                            .disabled(!ai_explain_enabled || !can_explain)
                                            .on_click(move |_, window, cx| {
                                                explain_changes.update(cx, |this, cx| {
                                                    this.explain_changes(window, cx)
                                                });
                                            }),
                                    )
                                    .item(
                                        PopupMenuItem::new("Review changes")
                                            .icon(IconName::Check)
                                            .disabled(!ai_review_enabled || !can_review)
                                            .on_click(move |_, window, cx| {
                                                review_changes.update(cx, |this, cx| {
                                                    this.review_changes(window, cx)
                                                });
                                            }),
                                    )
                            }),
                    )
                    .child(
                        button("toggle-description")
                            .ghost()
                            .small()
                            .icon(icons::pencil())
                            .label("Description")
                            .selected(self.show_description)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_description = !this.show_description;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        button("stash")
                            .outline()
                            .small()
                            .label("Stash")
                            .on_click(cx.listener(|this, _, _, cx| {
                                let hide = Self::hide_checkpoints(cx);
                                this.state.update(cx, |state, cx| {
                                    state.run_operation(
                                        "Stashing",
                                        hide,
                                        |repo| repo.stash_push(None),
                                        cx,
                                    );
                                });
                            })),
                    )
                    .child(
                        button("commit")
                            .child(fx::anchor(COMMIT_ANCHOR))
                            .primary()
                            .small()
                            .label(if self.amend { "Amend" } else { "Commit" })
                            .disabled_readable(!can_commit || busy, cx)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.commit(window, cx);
                            })),
                    ),
            )
    }
}

impl Render for ChangesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let staged = self.visible_entries(true, cx);
        let unstaged = self.visible_entries(false, cx);
        let settings = GitSettings::get(cx).clone();
        let (conflicted, conflict_operation) = {
            let status = &self.state.read(cx).status;
            (status.conflicted.clone(), status.conflict_operation)
        };
        let has_conflicts = !conflicted.is_empty();
        let show_unstaged = !unstaged.is_empty() || (staged.is_empty() && !has_conflicts);
        let working_tree_clean =
            staged.is_empty() && unstaged.is_empty() && !has_conflicts && !self.amend;
        let (staged_additions, staged_deletions) = line_totals(&staged);
        let (unstaged_additions, unstaged_deletions) = line_totals(&unstaged);
        let total_additions = staged_additions + unstaged_additions;
        let total_deletions = staged_deletions + unstaged_deletions;
        let conflict_count = conflicted.len();
        let conflict_summary = if conflict_count == 1 {
            conflict_operation.map_or_else(
                || "Git has 1 unresolved conflict. Resolve the file or clear the conflict.".to_string(),
                |operation| {
                    format!(
                        "{} stopped with 1 unresolved conflict. Resolve the file or abort the operation.",
                        operation.label()
                    )
                },
            )
        } else {
            conflict_operation.map_or_else(
                || {
                    format!(
                        "Git has {conflict_count} unresolved conflicts. Resolve the files or clear the conflicts."
                    )
                },
                |operation| {
                    format!(
                        "{} stopped with {conflict_count} unresolved conflicts. Resolve the files or abort the operation.",
                        operation.label()
                    )
                },
            )
        };
        let colors = theme::liquid_colors(cx);
        let changes_render_mode = settings.changes_render_mode;
        let has_selection = self.selected.is_some();
        let busy = self.state.read(cx).busy.is_some();
        let head_hash = self
            .state
            .read(cx)
            .commits
            .first()
            .map(|c| c.short_hash.clone());

        let files_panel = v_flex()
            .h_full()
            .min_w_0()
            .flex_1()
            .bg(cx.theme().colors.list)
            .child(
                h_flex()
                    .w_full()
                    .h(px(38.))
                    .flex_shrink_0()
                    .px_3()
                    .gap_3()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(self.render_amend_switch(busy, cx))
                    .when(self.amend_loading, |this| {
                        this.child(
                            div()
                                .text_size(text_px(11.5))
                                .text_color(cx.theme().muted_foreground)
                                .child("Loading commit…"),
                        )
                    })
                    .when_some(head_hash.filter(|_| self.amend), |this, hash| {
                        this.child(
                            div()
                                .text_size(text_px(11.5))
                                .font_family(mono_font())
                                .text_color(cx.theme().muted_foreground)
                                .child(hash),
                        )
                    })
                    .child(div().flex_1())
                    .when(total_additions + total_deletions > 0, |this| {
                        this.child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .font_family(mono_font())
                                        .text_size(text_px(11.))
                                        .text_color(colors.success)
                                        .child(format!("+{total_additions}")),
                                )
                                .child(
                                    div()
                                        .font_family(mono_font())
                                        .text_size(text_px(11.))
                                        .text_color(colors.danger)
                                        .child(format!("−{total_deletions}")),
                                )
                                .child(div().relative().w(px(84.)).h(px(8.)).child(
                                    DitherBar::new(
                                        vec![
                                            BarSegment {
                                                weight: total_additions as f32,
                                                color: colors.success,
                                            },
                                            BarSegment {
                                                weight: total_deletions as f32,
                                                color: colors.danger,
                                            },
                                        ],
                                        cx.theme().muted_foreground.opacity(0.25),
                                    ),
                                )),
                        )
                    })
                    .child(
                        button("changes-render-mode")
                            .outline()
                            .xsmall()
                            .label(changes_render_mode.label())
                            .tooltip("File rendering mode")
                            .dropdown_caret(true)
                            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                                ChangesRenderMode::ALL.into_iter().fold(menu, |menu, mode| {
                                    menu.item(
                                        PopupMenuItem::new(mode.label())
                                            .checked(mode == changes_render_mode)
                                            .on_click(move |_, _, cx| {
                                                GitSettings::update_saved(cx, |settings| {
                                                    settings.changes_render_mode = mode;
                                                });
                                                cx.refresh_windows();
                                            }),
                                    )
                                })
                            }),
                    ),
            )
            .when(has_conflicts, |this| {
                this.child(
                    h_flex()
                        .relative()
                        .w_full()
                        .pl_4()
                        .pr_3()
                        .py_2()
                        .gap_2()
                        .bg(colors.warning.opacity(0.08))
                        .border_b_1()
                        .border_color(colors.warning.opacity(0.25))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(6.))
                                .bg(checker(colors.warning.opacity(0.85), 1.0, window)),
                        )
                        .child(
                            Icon::new(IconName::TriangleAlert)
                                .xsmall()
                                .text_color(colors.warning),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(text_px(11.5))
                                .text_color(colors.warning)
                                .child(conflict_summary),
                        ),
                )
            })
            .child(
                // The visible file area, so effects stay inside it.
                v_flex()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(fx::anchor(FILES_ANCHOR))
                    .child(
                        v_flex()
                            .id("changes-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .when(has_conflicts, |this| {
                                let section = self.render_conflicts_section(
                                    conflicted,
                                    conflict_operation,
                                    cx,
                                );
                                this.child(section)
                            })
                            .when(working_tree_clean, |this| {
                                this.child(div().h(px(320.)).child(components::dither_empty_state(
                                    "Working tree clean",
                                    "Edits in this repository will appear here.",
                                    DitherField::Wave {
                                        level: 0.32,
                                        amplitude: 0.04,
                                        speed: 0.9,
                                    },
                                    cx,
                                )))
                            })
                            .when(!staged.is_empty(), |this| {
                                let section = self.render_section("Staged", true, staged, cx);
                                this.child(section)
                            })
                            .when(show_unstaged && !working_tree_clean, |this| {
                                let section = self.render_section("Changes", false, unstaged, cx);
                                this.child(section)
                            }),
                    ),
            )
            .child(self.render_commit_box(cx));

        let layout = if !has_selection {
            files_panel.into_any_element()
        } else if settings.diff_panel_bottom {
            v_resizable("changes-bottom-diff-split")
                .with_state(&self.bottom_diff_split)
                .on_resize(|state, _, cx| {
                    let Some(height) = state.read(cx).sizes().get(1).copied() else {
                        return;
                    };
                    GitSettings::update_saved(cx, |settings| {
                        settings.diff_panel_height = (f32::from(height) / ui_scale())
                            .clamp(MIN_DIFF_PANEL_HEIGHT, MAX_DIFF_PANEL_HEIGHT);
                    });
                })
                .child(
                    resizable_panel()
                        .size_range(px(MIN_DIFF_PANEL_HEIGHT)..Pixels::MAX)
                        .child(files_panel),
                )
                .child(
                    resizable_panel()
                        .size(px(settings.effective_diff_panel_height()))
                        .size_range(px(MIN_DIFF_PANEL_HEIGHT)..px(MAX_DIFF_PANEL_HEIGHT))
                        .flex_none()
                        .child(
                            div()
                                .size_full()
                                .border_t_1()
                                .border_color(cx.theme().border)
                                .child(self.diff.clone().cached(full_size())),
                        ),
                )
                .into_any_element()
        } else {
            h_resizable("changes-right-diff-split")
                .with_state(&self.right_diff_split)
                .on_resize(|state, _, cx| {
                    let Some(width) = state.read(cx).sizes().get(1).copied() else {
                        return;
                    };
                    GitSettings::update_saved(cx, |settings| {
                        settings.diff_panel_width = (f32::from(width) / ui_scale())
                            .clamp(MIN_DIFF_PANEL_WIDTH, MAX_DIFF_PANEL_WIDTH);
                    });
                })
                .child(
                    resizable_panel()
                        .size_range(px(MIN_DIFF_PANEL_WIDTH)..Pixels::MAX)
                        .child(files_panel),
                )
                .child(
                    resizable_panel()
                        .size(px(settings.effective_diff_panel_width()))
                        .size_range(px(MIN_DIFF_PANEL_WIDTH)..px(MAX_DIFF_PANEL_WIDTH))
                        .flex_none()
                        .child(
                            div()
                                .size_full()
                                .border_l_1()
                                .border_color(cx.theme().border)
                                .child(self.diff.clone().cached(full_size())),
                        ),
                )
                .into_any_element()
        };

        div().size_full().min_h_0().min_w_0().child(layout)
    }
}
