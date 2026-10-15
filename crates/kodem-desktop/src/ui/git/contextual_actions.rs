//! Git dialogs and the menu builders that several views share: the destructive action
//! confirmation, the reset submenu, and the dialogs that create branches, track LFS patterns,
//! check out, merge, rebase (with the interactive rebase plan), remove stale branches, and
//! remove or apply worktrees.
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Context, Div, Entity, FocusHandle, Focusable as _, FontWeight, Hsla,
    IntoElement, Pixels, Render, SharedString, Window, div,
};
use kodem_git::git::{
    Branch, BranchCheckout, Commit, FileChangeKind, GitRepo, InteractiveRebaseItem, MergeOptions,
    RebaseOptions, RepoStatus, StatusEntry,
};

use crate::theme::{px, text_px};
use crate::ui::controls::Switch;
use crate::ui::dither::{Dither, DitherField, Edge};
use crate::ui::git::components::{
    button, change_kind_chip, dialog_header, dither_loader, file_path_label, mono_font, mono_label,
    text_field,
};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, Disableable as _, Icon, IconName, InputEvent,
    InputState, LiquidButton, PopupMenu, PopupMenuItem, ScrollableElement as _, Sizable as _,
    Textarea, TextareaState, WindowExt as _, h_flex, v_flex,
};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::state::RepositoryState;
use crate::ui::git::theme;

/// Returns every folder prefix of the branch names, such as `feature/` and `feature/ui/`, sorted.
/// Remote branches count without their remote name.
pub fn branch_folder_prefixes(branches: &[Branch]) -> Vec<String> {
    let mut prefixes = HashSet::new();
    for branch in branches {
        let name = if branch.is_remote {
            branch.display_name()
        } else {
            branch.name.as_str()
        };
        let segments = name
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        let mut prefix = String::new();
        for segment in segments.iter().take(segments.len().saturating_sub(1)) {
            prefix.push_str(segment);
            prefix.push('/');
            prefixes.insert(prefix.clone());
        }
    }
    let mut prefixes = prefixes.into_iter().collect::<Vec<_>>();
    prefixes.sort();
    prefixes
}

/// Opens `view` as a Git dialog of the given width. The view draws its own header, body and footer,
/// so the frame has no padding or close control, and a press outside does not close it.
pub fn open_liquid_dialog<V: Render + 'static>(
    view: Entity<V>,
    width: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, _, cx| {
        dialog
            .w(width)
            .p_0()
            .gap_0()
            .close_button(false)
            .overlay_closable(false)
            .bg(cx.theme().popover)
            .child(view.clone())
    });
}

type DestructiveAction = Box<dyn FnOnce(&mut App)>;

/// Asks the user to confirm an action that can lose work, such as a hard reset or a branch
/// deletion.
pub struct DestructiveActionDialogView {
    title: SharedString,
    message: SharedString,
    target: Option<SharedString>,
    confirm_label: SharedString,
    action: Option<DestructiveAction>,
}

impl DestructiveActionDialogView {
    fn confirm(&mut self, window: &mut Window, cx: &mut App) {
        let Some(action) = self.action.take() else {
            return;
        };
        window.close_dialog(cx);
        action(cx);
    }
}

impl Render for DestructiveActionDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::liquid_colors(cx);
        let entity = cx.entity();

        modal_frame(
            modal_header(
                icons::trash(),
                self.title.clone(),
                "Confirmation required",
                colors.danger,
                cx,
            ),
            v_flex()
                .w_full()
                .gap_3()
                .px_4()
                .py_3()
                .child(callout(self.message.clone(), colors.danger))
                .when_some(self.target.clone(), |body, target| {
                    body.child(summary_row("Target", target, cx))
                }),
            modal_footer(
                "destructive-action-cancel",
                "Cancel",
                move |_, window, cx| window.close_dialog(cx),
                button("destructive-action-confirm")
                    .danger()
                    .label(self.confirm_label.clone())
                    .icon(icons::trash())
                    .on_click(move |_, window, cx| {
                        entity.update(cx, |view, cx| view.confirm(window, cx));
                    }),
            ),
            cx,
        )
    }
}

/// Runs `action` after the user confirms it in a destructive action dialog. When the user turned
/// confirmations off in the settings, the action runs at once.
pub fn confirm_destructive_action(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    target: Option<SharedString>,
    confirm_label: impl Into<SharedString>,
    action: impl FnOnce(&mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    if !GitSettings::get(cx).confirm_destructive_actions {
        action(cx);
        return;
    }

    let view = cx.new(move |_| DestructiveActionDialogView {
        title: title.into(),
        message: message.into(),
        target,
        confirm_label: confirm_label.into(),
        action: Some(Box::new(action)),
    });
    open_liquid_dialog(view, px(440.0), window, cx);
}

/// Adds the shared local reset options for a branch tip or history commit.
pub fn reset_to_here_menu(
    menu: PopupMenu,
    state: Entity<RepositoryState>,
    hash: String,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    reset_submenu(menu.separator(), "Reset to Here", state, hash, window, cx)
}

/// Adds a submenu titled `title` with the soft, mixed, and hard resets of
/// the current branch to `hash`.
pub fn reset_submenu(
    menu: PopupMenu,
    title: impl Into<SharedString>,
    state: Entity<RepositoryState>,
    hash: String,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    menu.submenu(title, window, cx, move |mut menu, _, _| {
        for (mode, label, title, message, confirm_label) in [
            (
                "soft",
                "Soft (keep changes staged)",
                "Reset (Soft)?",
                "HEAD will move to this commit. Your changes will remain staged.",
                "Reset Soft",
            ),
            (
                "mixed",
                "Mixed (keep changes unstaged)",
                "Reset (Mixed)?",
                "HEAD will move to this commit. Your changes will remain in the working directory.",
                "Reset Mixed",
            ),
            (
                "hard",
                "Hard (discard all changes)",
                "Reset (Hard)?",
                "HEAD will move to this commit, and all uncommitted changes will be lost. This action cannot be undone.",
                "Reset Hard",
            ),
        ] {
            let state = state.clone();
            let hash = hash.clone();
            menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                let state = state.clone();
                let hash = hash.clone();
                let hide = GitSettings::get(cx).hide_checkpoint_commits;
                confirm_destructive_action(
                    title,
                    message,
                    Some(SharedString::from(hash.clone())),
                    confirm_label,
                    move |cx| {
                        state.update(cx, |state, cx| {
                            state.run_operation(
                                "Resetting",
                                hide,
                                move |repo| repo.reset(&hash, mode),
                                cx,
                            );
                        });
                    },
                    window,
                    cx,
                );
            }));
        }
        menu
    })
}

/// Creates a branch at a commit or branch tip and checks it out. It suggests the folder prefixes of
/// the existing branches.
pub struct CreateBranchDialogView {
    state: Entity<RepositoryState>,
    start_ref: String,
    detail: String,
    prefixes: Vec<String>,
    input: Entity<InputState>,
}

impl CreateBranchDialogView {
    /// Creates the dialog for a branch at `start_ref`. `detail` names the start in the header and
    /// `prefixes` are the folder suggestions.
    pub fn new(
        state: Entity<RepositoryState>,
        start_ref: String,
        detail: String,
        prefixes: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Branch name"));
        cx.observe(&input, |_, _, cx| cx.notify()).detach();
        cx.subscribe_in(&input, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter) {
                this.create_branch(window, cx);
            }
        })
        .detach();
        Self {
            state,
            start_ref,
            detail,
            prefixes,
            input,
        }
    }

    /// Returns the focus handle of the branch name field, so the opener can focus it.
    pub fn input_focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }

    fn create_branch(&self, window: &mut Window, cx: &mut App) {
        let name = self.input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let start_ref = self.start_ref.clone();
        run_git(
            &self.state,
            "Creating branch",
            move |repo| repo.create_and_checkout_branch_at(&name, &start_ref),
            cx,
        );
        window.close_dialog(cx);
    }
}

impl Render for CreateBranchDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.input.read(cx).value().trim().to_string();
        let suggestions = self
            .prefixes
            .iter()
            .filter(|prefix| value.is_empty() || prefix.starts_with(&value))
            .cloned()
            .collect::<Vec<_>>();
        let input = self.input.clone();
        let entity = cx.entity();

        modal_frame(
            modal_header(
                icons::git_branch(),
                "Create Branch",
                format!("at tip of {}", self.detail),
                cx.theme().primary,
                cx,
            ),
            v_flex()
                .w_full()
                .gap_2()
                .px_4()
                .py_3()
                .child(section_label("BRANCH NAME", cx))
                .child(text_field(&self.input))
                .when(!suggestions.is_empty(), |this| {
                    this.child(
                        v_flex()
                            .max_h(px(190.))
                            .overflow_y_scrollbar()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().muted)
                            .children(suggestions.into_iter().enumerate().map(|(ix, prefix)| {
                                let selected_input = input.clone();
                                let selected_prefix = prefix.clone();
                                h_flex()
                                    .id(("branch-prefix", ix))
                                    .w_full()
                                    .flex_shrink_0()
                                    .gap_2()
                                    .px_3()
                                    .py_2()
                                    .cursor_pointer()
                                    .when(ix > 0, |row| {
                                        row.border_t_1().border_color(cx.theme().border)
                                    })
                                    .hover(|row| row.bg(cx.theme().list_hover))
                                    .on_click(move |_, window, cx| {
                                        selected_input.update(cx, |input, cx| {
                                            input.set_value(selected_prefix.clone(), window, cx);
                                        });
                                    })
                                    .child(
                                        Icon::new(IconName::Folder)
                                            .small()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .font_family(mono_font())
                                            .text_size(text_px(12.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(prefix),
                                    )
                            })),
                    )
                }),
            modal_footer(
                "create-branch-cancel",
                "Cancel",
                move |_, window, cx| window.close_dialog(cx),
                button("create-branch-confirm")
                    .primary()
                    .label("Create")
                    .icon(IconName::Plus)
                    .disabled_readable(value.is_empty(), cx)
                    .on_click(move |_, window, cx| {
                        entity.update(cx, |view, cx| view.create_branch(window, cx));
                    }),
            ),
            cx,
        )
    }
}

/// Adds a Git LFS tracking pattern to `.gitattributes`.
pub struct TrackLfsDialogView {
    state: Entity<RepositoryState>,
    input: Entity<InputState>,
}

impl TrackLfsDialogView {
    /// Creates the dialog with an empty pattern field.
    pub fn new(
        state: Entity<RepositoryState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("For example, *.psd"));
        cx.observe(&input, |_, _, cx| cx.notify()).detach();
        cx.subscribe_in(&input, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter) {
                this.track_pattern(window, cx);
            }
        })
        .detach();
        Self { state, input }
    }

    /// Returns the focus handle of the pattern field, so the opener can focus it.
    pub fn input_focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }

    fn track_pattern(&self, window: &mut Window, cx: &mut App) {
        let pattern = self.input.read(cx).value().trim().to_string();
        if pattern.is_empty() {
            return;
        }
        run_git(
            &self.state,
            "Tracking LFS pattern",
            move |repo| repo.lfs_track(&pattern),
            cx,
        );
        window.close_dialog(cx);
    }
}

impl Render for TrackLfsDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.input.read(cx).value().trim().to_string();
        let entity = cx.entity();
        modal_frame(
            modal_header(
                icons::file_text(),
                "Track LFS Pattern",
                "Add a pattern to .gitattributes",
                cx.theme().info,
                cx,
            ),
            v_flex()
                .w_full()
                .gap_2()
                .px_4()
                .py_3()
                .child(section_label("FILE PATTERN", cx))
                .child(text_field(&self.input))
                .child(
                    div()
                        .text_size(text_px(11.5))
                        .text_color(cx.theme().muted_foreground)
                        .child("Use a Git LFS pattern such as *.psd or assets/**."),
                ),
            modal_footer(
                "lfs-track-cancel",
                "Cancel",
                move |_, window, cx| window.close_dialog(cx),
                button("lfs-track-confirm")
                    .primary()
                    .label("Track")
                    .icon(IconName::Plus)
                    .disabled_readable(value.is_empty(), cx)
                    .on_click(move |_, window, cx| {
                        entity.update(cx, |view, cx| view.track_pattern(window, cx));
                    }),
            ),
            cx,
        )
    }
}

/// Confirms a checkout of a local or remote branch, with the option to auto-stash local changes.
pub struct CheckoutDialogView {
    state: Entity<RepositoryState>,
    target: BranchCheckout,
    autostash: bool,
}

impl CheckoutDialogView {
    /// Creates a checkout confirmation with the saved auto-stash preference.
    pub fn new(state: Entity<RepositoryState>, target: BranchCheckout, cx: &App) -> Self {
        Self {
            state,
            target,
            autostash: GitSettings::get(cx).autostash_before_checkout,
        }
    }
}

impl Render for CheckoutDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let state = self.state.clone();
        let target = self.target.clone();
        let reference = match &target {
            BranchCheckout::Local(reference) | BranchCheckout::Remote { reference, .. } => {
                reference
            }
        };
        let busy = self.state.read(cx).busy.is_some();

        modal_frame(
            modal_header(
                icons::git_branch(),
                "Checkout",
                reference.clone(),
                cx.theme().primary,
                cx,
            ),
            v_flex()
                .w_full()
                .gap_3()
                .px_4()
                .py_3()
                .child(summary_row(
                    "Current branch",
                    self.state.read(cx).current_branch.clone(),
                    cx,
                ))
                .child(summary_row("Checkout", reference.clone(), cx))
                .child(option_row(
                    "Auto-stash local changes",
                    "Stash local changes and restore them after checkout",
                    Switch::new("checkout-autostash", self.autostash)
                        .label("Auto-stash local changes")
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.autostash = *checked;
                            GitSettings::update(cx, |s| s.autostash_before_checkout = *checked);
                            cx.notify();
                        })),
                    cx,
                )),
            modal_footer(
                "checkout-cancel",
                "Cancel",
                move |_, window, cx| window.close_dialog(cx),
                button("checkout-confirm")
                    .primary()
                    .label("Checkout")
                    .icon(IconName::ArrowRight)
                    .disabled_readable(busy, cx)
                    .on_click(move |_, window, cx| {
                        let autostash = entity.read(cx).autostash;
                        let target = target.clone();
                        run_git(
                            &state,
                            "Checking out",
                            move |repo| repo.checkout_branch(&target, autostash),
                            cx,
                        );
                        window.close_dialog(cx);
                    }),
            ),
            cx,
        )
    }
}

/// Merges a branch or commit into a branch, with the merge options and an optional commit message.
pub struct MergeDialogView {
    state: Entity<RepositoryState>,
    source_ref: String,
    source: String,
    target: String,
    message: Entity<TextareaState>,
    no_ff: bool,
    squash: bool,
    no_commit: bool,
    stash_first: bool,
    /// A branch to check out before the merge, when the merge goes into a
    /// branch that is not checked out.
    checkout: Option<String>,
}

impl MergeDialogView {
    /// Creates a merge dialog with the saved auto-stash preference.
    pub fn new(
        state: Entity<RepositoryState>,
        source_ref: String,
        source: String,
        target: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = format!("Merge branch '{source}' into {target}");
        let message = cx.new(|cx| TextareaState::new(window, cx).placeholder(placeholder));
        Self {
            state,
            source_ref,
            source,
            target,
            message,
            no_ff: true,
            squash: false,
            no_commit: false,
            stash_first: GitSettings::get(cx).autostash_before_merge,
            checkout: None,
        }
    }

    /// Checks out `branch` before the merge, so the merge goes into it.
    pub fn checkout_first(mut self, branch: String) -> Self {
        self.checkout = Some(branch);
        self
    }

    fn options(&self, cx: &App) -> MergeOptions {
        let message = self.message.read(cx).value().trim().to_string();
        MergeOptions {
            no_ff: self.no_ff,
            squash: self.squash,
            no_commit: self.no_commit,
            message: (!message.is_empty()).then_some(message),
            stash_first: self.stash_first,
        }
    }
}

impl Render for MergeDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let state = self.state.clone();
        let source_ref = self.source_ref.clone();
        let checkout = self.checkout.clone();
        let busy = self.state.read(cx).busy.is_some();
        let colors = theme::liquid_colors(cx);

        modal_frame(
            modal_header(
                icons::git_merge(),
                format!("Merge into {}", self.target),
                self.source.clone(),
                colors.warning,
                cx,
            ),
            v_flex()
                .w_full()
                .gap_3()
                .px_4()
                .py_3()
                .when_some(self.checkout.clone(), |this, branch| {
                    this.child(callout(
                        format!("Kodem checks out {branch} before the merge."),
                        colors.info,
                    ))
                })
                .child(
                    v_flex()
                        .gap_1()
                        .child(section_label("COMMIT MESSAGE", cx))
                        .child(Textarea::new(&self.message).h(px(72.))),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child(section_label("OPTIONS", cx))
                        .child(option_row(
                            "No fast-forward",
                            "Always create a merge commit",
                            Switch::new("merge-no-ff", self.no_ff)
                                .label("No fast-forward")
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.no_ff = *checked;
                                    if *checked {
                                        this.squash = false;
                                    }
                                    cx.notify();
                                })),
                            cx,
                        ))
                        .child(option_row(
                            "Squash",
                            "Combine the source commits into one change",
                            Switch::new("merge-squash", self.squash)
                                .label("Squash")
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.squash = *checked;
                                    if *checked {
                                        this.no_ff = false;
                                    }
                                    cx.notify();
                                })),
                            cx,
                        ))
                        .child(option_row(
                            "No auto-commit",
                            "Merge without an automatic commit",
                            Switch::new("merge-no-commit", self.no_commit)
                                .label("No auto-commit")
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.no_commit = *checked;
                                    cx.notify();
                                })),
                            cx,
                        ))
                        .child(option_row(
                            "Stash before merge",
                            "Restore local changes after the merge",
                            Switch::new("merge-stash-first", self.stash_first)
                                .label("Stash before merge")
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.stash_first = *checked;
                                    GitSettings::update(cx, |s| {
                                        s.autostash_before_merge = *checked
                                    });
                                    cx.notify();
                                })),
                            cx,
                        )),
                ),
            modal_footer(
                "merge-cancel",
                "Cancel",
                move |_, window, cx| window.close_dialog(cx),
                button("merge-confirm")
                    .primary()
                    .label("Merge")
                    .icon(icons::git_merge())
                    .disabled_readable(busy, cx)
                    .on_click(move |_, window, cx| {
                        let options = entity.read(cx).options(cx);
                        let source_ref = source_ref.clone();
                        let checkout = checkout.clone();
                        run_git(
                            &state,
                            "Merging",
                            move |repo| {
                                if let Some(branch) = &checkout {
                                    repo.checkout(branch)?;
                                }
                                repo.merge_with_options(&source_ref, options)
                            },
                            cx,
                        );
                        window.close_dialog(cx);
                    }),
            ),
            cx,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebaseAction {
    Pick,
    Squash,
    Fixup,
    Drop,
}

impl RebaseAction {
    fn label(self) -> &'static str {
        match self {
            Self::Pick => "Pick",
            Self::Squash => "Squash",
            Self::Fixup => "Fixup",
            Self::Drop => "Drop",
        }
    }

    /// The color that marks this action in the rebase plan.
    fn tone(self, cx: &App) -> Hsla {
        let colors = theme::liquid_colors(cx);
        match self {
            Self::Pick => cx.theme().primary,
            Self::Squash => colors.info,
            Self::Fixup => colors.warning,
            Self::Drop => colors.danger,
        }
    }

    fn command(self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            Self::Drop => "drop",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Pick => Self::Squash,
            Self::Squash => Self::Fixup,
            Self::Fixup => Self::Drop,
            Self::Drop => Self::Pick,
        }
    }
}

#[derive(Debug, Clone)]
struct RebasePlanItem {
    commit: Commit,
    action: RebaseAction,
}

/// Rebases a branch onto another revision. The interactive form shows the commits to move, so the
/// user can reorder, squash, fix up or drop them.
pub struct RebaseDialogView {
    state: Entity<RepositoryState>,
    upstream_ref: String,
    source: String,
    target: String,
    interactive: bool,
    autostash: bool,
    commits: Option<Result<Vec<RebasePlanItem>, String>>,
    /// The branch to rebase when it is not the current branch.
    branch: Option<String>,
    /// Moves only the commits after this revision (`git rebase --onto`).
    after: Option<RebaseStart>,
}

/// The first commit of a partial rebase.
#[derive(Debug, Clone)]
struct RebaseStart {
    /// The revision before the first moved commit.
    after: String,
    first_commit: String,
    commit_count: Option<usize>,
}

impl RebaseDialogView {
    /// Creates a rebase dialog with the preference shared by pull and Git settings.
    pub fn new(
        state: Entity<RepositoryState>,
        upstream_ref: String,
        source: String,
        target: String,
        interactive: bool,
        cx: &App,
    ) -> Self {
        Self {
            state,
            upstream_ref,
            source,
            target,
            interactive,
            autostash: GitSettings::get(cx).autostash_before_rebase,
            commits: (!interactive).then_some(Ok(Vec::new())),
            branch: None,
            after: None,
        }
    }

    /// Rebases `branch` instead of the current branch. Git checks it out
    /// first.
    pub fn rebase_branch(mut self, branch: String) -> Self {
        self.branch = Some(branch);
        self
    }

    /// Moves only `first_commit` and the commits after it. `after` is the
    /// revision just before `first_commit`.
    pub fn starting_at(
        mut self,
        after: String,
        first_commit: String,
        commit_count: Option<usize>,
    ) -> Self {
        self.after = Some(RebaseStart {
            after,
            first_commit,
            commit_count,
        });
        self
    }

    /// Shows the commits of the interactive rebase plan, or why there are none.
    pub fn set_commits(&mut self, commits: anyhow::Result<Vec<Commit>>, cx: &mut Context<Self>) {
        self.commits = Some(match commits {
            Ok(commits) if commits.is_empty() => Err(format!(
                "There are no commits to rebase onto {}.",
                self.source
            )),
            Ok(commits) => Ok(commits
                .into_iter()
                .map(|commit| RebasePlanItem {
                    commit,
                    action: RebaseAction::Pick,
                })
                .collect()),
            Err(error) => Err(error.to_string()),
        });
        cx.notify();
    }

    fn options(&self) -> RebaseOptions {
        RebaseOptions {
            autostash: self.autostash,
        }
    }

    fn interactive_plan(&self) -> Option<Vec<InteractiveRebaseItem>> {
        let items = self.commits.as_ref()?.as_ref().ok()?;
        if self.validation_message().is_some() {
            return None;
        }
        Some(
            items
                .iter()
                .map(|item| InteractiveRebaseItem {
                    action: item.action.command().to_string(),
                    hash: item.commit.hash.clone(),
                    subject: item.commit.subject.clone(),
                })
                .collect(),
        )
    }

    fn can_start(&self) -> bool {
        !self.interactive || self.interactive_plan().is_some()
    }

    fn validation_message(&self) -> Option<String> {
        if !self.interactive {
            return None;
        }
        let result = self.commits.as_ref()?;
        let items = match result {
            Ok(items) => items,
            Err(error) => return Some(error.clone()),
        };
        if let Some(first) = items.iter().find(|item| item.action != RebaseAction::Drop)
            && first.action != RebaseAction::Pick
        {
            return Some("The first active commit must use Pick.".to_string());
        }
        None
    }
}

impl Render for RebaseDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::liquid_colors(cx);
        let validation = self.validation_message();
        let entity = cx.entity();
        let state = self.state.clone();
        let upstream_ref = self.upstream_ref.clone();
        let interactive = self.interactive;
        let branch = self.branch.clone();
        let after = self.after.as_ref().map(|start| start.after.clone());
        let busy = self.state.read(cx).busy.is_some();
        let can_start = self.can_start() && !busy;

        modal_frame(
            modal_header(
                icons::refresh(),
                if interactive {
                    "Interactive Rebase".to_string()
                } else {
                    "Rebase Branch".to_string()
                },
                format!("{} onto {}", self.target, self.source),
                cx.theme().primary,
                cx,
            ),
            v_flex()
                .w_full()
                .gap_3()
                .px_4()
                .py_3()
                .child(summary_row("Upstream", self.source.clone(), cx))
                .child(summary_row("Branch", self.target.clone(), cx))
                .when_some(self.after.as_ref(), |this, start| {
                    let commits = match start.commit_count {
                        Some(1) => format!("{} only", start.first_commit),
                        Some(count) => {
                            format!("{} and later ({count} commits)", start.first_commit)
                        }
                        None => format!("{} and later", start.first_commit),
                    };
                    this.child(summary_row("Commits", commits, cx))
                })
                .when_some(self.branch.clone(), |this, branch| {
                    this.child(callout(
                        format!("Kodem checks out {branch} before the rebase."),
                        colors.info,
                    ))
                })
                .child(option_row(
                    "Autostash",
                    "Restore local changes after the rebase",
                    Switch::new("rebase-autostash", self.autostash)
                        .label("Autostash")
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.autostash = *checked;
                            GitSettings::update(cx, |s| s.autostash_before_rebase = *checked);
                            cx.notify();
                        })),
                    cx,
                ))
                .when(interactive, |this| {
                    this.child(section_label("COMMITS", cx))
                        .child(match &self.commits {
                            None => dither_loader("Loading the rebase plan", cx).into_any_element(),
                            Some(Err(error)) => {
                                callout(error.clone(), cx.theme().danger).into_any_element()
                            }
                            Some(Ok(items)) => v_flex()
                                .max_h(px(280.))
                                .overflow_y_scrollbar()
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded(px(8.))
                                .bg(colors.panel_secondary)
                                .children(items.iter().enumerate().map(|(ix, item)| {
                                    let action = item.action;
                                    let tone = action.tone(cx);
                                    let dropped = action == RebaseAction::Drop;
                                    h_flex()
                                        .w_full()
                                        .flex_shrink_0()
                                        .gap_2()
                                        .px_2()
                                        .py_1p5()
                                        .when(ix > 0, |row| {
                                            row.border_t_1().border_color(cx.theme().border)
                                        })
                                        .when(dropped, |row| row.opacity(0.55))
                                        .child(
                                            button(("rebase-action", ix))
                                                .tone(tone)
                                                .xsmall()
                                                .w(px(64.))
                                                .label(action.label())
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if let Some(Ok(items)) = &mut this.commits {
                                                        items[ix].action = items[ix].action.next();
                                                    }
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            v_flex()
                                                .flex_1()
                                                .min_w_0()
                                                .child(
                                                    div()
                                                        .font_family(mono_font())
                                                        .text_size(text_px(11.5))
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(item.commit.short_hash.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .whitespace_nowrap()
                                                        .text_size(text_px(12.5))
                                                        .child(item.commit.subject.clone()),
                                                ),
                                        )
                                        .child(
                                            button(("rebase-up", ix))
                                                .ghost()
                                                .xsmall()
                                                .icon(IconName::ChevronUp)
                                                .disabled(ix == 0)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if ix > 0
                                                        && let Some(Ok(items)) = &mut this.commits
                                                    {
                                                        items.swap(ix, ix - 1);
                                                    }
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            button(("rebase-down", ix))
                                                .ghost()
                                                .xsmall()
                                                .icon(IconName::ChevronDown)
                                                .disabled(ix + 1 == items.len())
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if let Some(Ok(items)) = &mut this.commits
                                                        && ix + 1 < items.len()
                                                    {
                                                        items.swap(ix, ix + 1);
                                                    }
                                                    cx.notify();
                                                })),
                                        )
                                }))
                                .into_any_element(),
                        })
                })
                .when_some(validation, |this, message| {
                    this.child(callout(message, cx.theme().danger))
                }),
            modal_footer(
                "rebase-cancel",
                "Cancel",
                move |_, window, cx| window.close_dialog(cx),
                button("rebase-confirm")
                    .primary()
                    .label("Start Rebase")
                    .icon(icons::refresh())
                    .disabled_readable(!can_start, cx)
                    .on_click(move |_, window, cx| {
                        let view = entity.read(cx);
                        let options = view.options();
                        if interactive {
                            let Some(plan) = view.interactive_plan() else {
                                return;
                            };
                            let upstream_ref = upstream_ref.clone();
                            run_git(
                                &state,
                                "Rebasing",
                                move |repo| repo.interactive_rebase(&upstream_ref, options, &plan),
                                cx,
                            );
                        } else {
                            let upstream_ref = upstream_ref.clone();
                            let branch = branch.clone();
                            let after = after.clone();
                            run_git(
                                &state,
                                "Rebasing",
                                move |repo| {
                                    repo.rebase_commits(
                                        &upstream_ref,
                                        after.as_deref(),
                                        branch.as_deref(),
                                        options,
                                    )
                                },
                                cx,
                            );
                        }
                        window.close_dialog(cx);
                    }),
            ),
            cx,
        )
    }
}

/// Opens a rebase dialog. An interactive dialog loads its commit plan in
/// the background.
pub fn open_rebase_dialog(view: Entity<RebaseDialogView>, window: &mut Window, cx: &mut App) {
    let (interactive, upstream_ref, repo) = {
        let dialog = view.read(cx);
        (
            dialog.interactive,
            dialog.upstream_ref.clone(),
            dialog.state.read(cx).repo.clone(),
        )
    };
    if interactive {
        let load_view = view.clone();
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move { repo.commits_between(&upstream_ref) })
                .await;
            load_view.update(cx, |view, cx| view.set_commits(result, cx));
        })
        .detach();
    }
    open_liquid_dialog(
        view,
        if interactive { px(620.) } else { px(460.) },
        window,
        cx,
    );
}

/// Removes the local branches whose upstream is gone.
pub struct StaleBranchesDialogView {
    state: Entity<RepositoryState>,
    branches: Vec<String>,
    force: bool,
}

impl StaleBranchesDialogView {
    /// Creates the dialog for `branches`.
    pub fn new(state: Entity<RepositoryState>, branches: Vec<String>) -> Self {
        Self {
            state,
            branches,
            force: false,
        }
    }
}

impl Render for StaleBranchesDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::liquid_colors(cx);
        let entity = cx.entity();
        let state = self.state.clone();
        let count = self.branches.len();
        let busy = self.state.read(cx).busy.is_some();

        modal_frame(
            modal_header(
                icons::trash(),
                "Remove Stale Local Branches",
                if count == 1 {
                    "1 local branch".to_string()
                } else {
                    format!("{count} local branches")
                },
                colors.danger,
                cx,
            ),
            v_flex()
                .w_full()
                .gap_3()
                .px_4()
                .py_3()
                .child(callout(
                    "Only local branches will be removed. Remote branches will stay unchanged.",
                    colors.warning,
                ))
                .child(option_row(
                    "Force delete unmerged branches",
                    "Use the force option for branches that Git reports as unmerged",
                    Switch::new("stale-force", self.force)
                        .label("Force delete unmerged branches")
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.force = *checked;
                            cx.notify();
                        })),
                    cx,
                ))
                .child(section_label("BRANCHES TO REMOVE", cx))
                .child(
                    v_flex()
                        .max_h(px(240.))
                        .overflow_y_scrollbar()
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded(px(8.))
                        .bg(colors.panel_secondary)
                        .children(self.branches.iter().enumerate().map(|(ix, branch)| {
                            h_flex()
                                .w_full()
                                .flex_shrink_0()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .when(ix > 0, |row| {
                                    row.border_t_1().border_color(cx.theme().border)
                                })
                                .child(icons::git_branch().small().text_color(colors.danger))
                                .child(
                                    div()
                                        .font_family(mono_font())
                                        .text_size(text_px(12.5))
                                        .child(branch.clone()),
                                )
                        })),
                ),
            modal_footer(
                "stale-cancel",
                "Cancel",
                move |_, window, cx| window.close_dialog(cx),
                button("stale-confirm")
                    .danger()
                    .label(if count == 1 {
                        "Remove Branch"
                    } else {
                        "Remove Branches"
                    })
                    .icon(icons::trash())
                    .disabled_readable(busy, cx)
                    .on_click(move |_, window, cx| {
                        let view = entity.read(cx);
                        let branches = view.branches.clone();
                        let force = view.force;
                        run_git(
                            &state,
                            "Removing stale branches",
                            move |repo| {
                                for branch in &branches {
                                    repo.delete_branch(branch, force)?;
                                }
                                Ok(())
                            },
                            cx,
                        );
                        window.close_dialog(cx);
                    }),
            ),
            cx,
        )
    }
}

/// Confirms the removal of a linked worktree.
pub struct RemoveWorktreeDialogView {
    state: Entity<RepositoryState>,
    path: PathBuf,
}

/// Applies the uncommitted changes of a linked worktree to the current worktree.
pub struct WorktreeApplyDialogView {
    state: Entity<RepositoryState>,
    path: PathBuf,
    name: String,
    target: String,
    has_target_changes: bool,
    stash_first: bool,
    changes: Option<Result<Vec<StatusEntry>, String>>,
}

impl WorktreeApplyDialogView {
    /// Creates a worktree apply dialog with the saved stash preference.
    pub fn new(
        state: Entity<RepositoryState>,
        path: PathBuf,
        name: String,
        target: String,
        has_target_changes: bool,
        cx: &App,
    ) -> Self {
        Self {
            state,
            path,
            name,
            target,
            has_target_changes,
            stash_first: GitSettings::get(cx).stash_before_worktree_apply,
            changes: None,
        }
    }

    /// Shows the changes of the source worktree, or why they cannot be applied.
    pub fn set_status(&mut self, status: anyhow::Result<RepoStatus>, cx: &mut Context<Self>) {
        self.changes = Some(match status {
            Ok(status) if !status.conflicted.is_empty() => Err(
                "Resolve conflicts in the source worktree before you apply its changes."
                    .to_string(),
            ),
            Ok(status) => {
                let mut entries = BTreeMap::new();
                for entry in status.staged.into_iter().chain(status.unstaged) {
                    entries.insert(entry.path.clone(), entry);
                }
                let entries = entries.into_values().collect::<Vec<_>>();
                if entries.is_empty() {
                    Err("This worktree has no uncommitted changes to apply.".to_string())
                } else {
                    Ok(entries)
                }
            }
            Err(error) => Err(error.to_string()),
        });
        cx.notify();
    }

    fn can_apply(&self) -> bool {
        matches!(&self.changes, Some(Ok(changes)) if !changes.is_empty())
    }
}

impl Render for WorktreeApplyDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let state = self.state.clone();
        let path = self.path.clone();
        let name = self.name.clone();
        let busy = self.state.read(cx).busy.is_some();
        let can_apply = self.can_apply() && !busy;
        let colors = theme::liquid_colors(cx);

        modal_frame(
            modal_header(
                icons::pull(),
                "Apply Worktree Changes",
                self.name.clone(),
                cx.theme().primary,
                cx,
            ),
            v_flex()
                .w_full()
                .gap_3()
                .px_4()
                .py_3()
                .child(summary_row("Path", self.path.display().to_string(), cx))
                .child(summary_row("Target", self.target.clone(), cx))
                .when(self.has_target_changes, |this| {
                    this.child(option_row(
                        "Stash current changes",
                        "Keep target changes in a stash before you apply the worktree",
                        Switch::new("worktree-apply-stash", self.stash_first)
                            .label("Stash current changes")
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.stash_first = *checked;
                                GitSettings::update(cx, |s| {
                                    s.stash_before_worktree_apply = *checked
                                });
                                cx.notify();
                            })),
                        cx,
                    ))
                })
                .child(section_label("FILES TO APPLY", cx))
                .child(match &self.changes {
                    None => dither_loader("Loading worktree changes", cx).into_any_element(),
                    Some(Err(error)) => callout(error.clone(), colors.warning).into_any_element(),
                    Some(Ok(changes)) => v_flex()
                        .max_h(px(260.))
                        .overflow_y_scrollbar()
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded(px(8.))
                        .bg(colors.panel_secondary)
                        .children(changes.iter().enumerate().map(|(ix, entry)| {
                            h_flex()
                                .w_full()
                                .flex_shrink_0()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .when(ix > 0, |row| {
                                    row.border_t_1().border_color(cx.theme().border)
                                })
                                .child(change_kind_chip(entry.kind, cx))
                                .child(file_path_label(&entry.path, cx))
                        }))
                        .into_any_element(),
                }),
            modal_footer(
                "worktree-apply-cancel",
                "Cancel",
                move |_, window, cx| window.close_dialog(cx),
                button("worktree-apply-confirm")
                    .primary()
                    .label("Apply Changes")
                    .icon(icons::pull())
                    .disabled_readable(!can_apply, cx)
                    .on_click(move |_, window, cx| {
                        let view = entity.read(cx);
                        let Some(Ok(changes)) = &view.changes else {
                            return;
                        };
                        let untracked = changes
                            .iter()
                            .filter(|entry| entry.kind == FileChangeKind::Untracked)
                            .map(|entry| entry.path.clone())
                            .collect::<Vec<_>>();
                        let stash_first = view.has_target_changes && view.stash_first;
                        let path = path.clone();
                        let name = name.clone();
                        run_git(
                            &state,
                            "Applying worktree changes",
                            move |repo| {
                                if stash_first {
                                    repo.stash_push(Some(&format!(
                                        "Stashed before applying {name}"
                                    )))?;
                                }
                                repo.apply_worktree_changes(&path, &untracked)
                            },
                            cx,
                        );
                        window.close_dialog(cx);
                    }),
            ),
            cx,
        )
    }
}

impl RemoveWorktreeDialogView {
    /// Creates the dialog for the worktree at `path`.
    pub fn new(state: Entity<RepositoryState>, path: PathBuf) -> Self {
        Self { state, path }
    }
}

impl Render for RemoveWorktreeDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::liquid_colors(cx);
        let path = self.path.clone();
        let path_label = self.path.display().to_string();
        let state = self.state.clone();
        let busy = self.state.read(cx).busy.is_some();

        modal_frame(
            modal_header(
                icons::trash(),
                "Remove Worktree",
                "This action removes files from disk",
                colors.danger,
                cx,
            ),
            v_flex()
                .w_full()
                .gap_3()
                .px_4()
                .py_3()
                .child(callout(
                    "Git will stop the removal if the worktree has uncommitted changes.",
                    colors.warning,
                ))
                .child(summary_row("Path", path_label, cx)),
            modal_footer(
                "worktree-cancel",
                "Cancel",
                move |_, window, cx| window.close_dialog(cx),
                button("worktree-confirm")
                    .danger()
                    .label("Remove")
                    .icon(icons::trash())
                    .disabled_readable(busy, cx)
                    .on_click(move |_, window, cx| {
                        let path = path.clone();
                        run_git(
                            &state,
                            "Removing worktree",
                            move |repo| repo.remove_worktree(&path),
                            cx,
                        );
                        window.close_dialog(cx);
                    }),
            ),
            cx,
        )
    }
}

fn run_git(
    state: &Entity<RepositoryState>,
    label: &str,
    operation: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
    cx: &mut App,
) {
    let hide = GitSettings::get(cx).hide_checkpoint_commits;
    state.update(cx, |state, cx| {
        state.run_operation(label, hide, operation, cx);
    });
}

/// Lays out a dialog: the header, the body and the footer, separated by hairlines.
pub(crate) fn modal_frame(
    header: impl IntoElement,
    body: impl IntoElement,
    footer: impl IntoElement,
    cx: &App,
) -> Div {
    v_flex()
        .w_full()
        .overflow_hidden()
        .bg(cx.theme().popover)
        .child(header)
        .child(divider(cx))
        .child(body)
        .child(divider(cx))
        .child(footer)
}

fn modal_header(
    icon: impl Into<Icon>,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    color: Hsla,
    cx: &App,
) -> Div {
    let title: SharedString = title.into();
    let detail: SharedString = detail.into();
    dialog_header(icon, title, detail, color, cx)
}

/// Returns a dialog footer with a cancel button and the dialog's action button, aligned to the end.
pub(crate) fn modal_footer(
    cancel_id: &'static str,
    cancel_label: &'static str,
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    action: LiquidButton,
) -> Div {
    h_flex()
        .w_full()
        .justify_end()
        .gap_2()
        .px_4()
        .py_3()
        .child(
            button(cancel_id)
                .outline()
                .label(cancel_label)
                .on_click(on_cancel),
        )
        .child(action)
}

/// Returns the quiet uppercase label above a group of dialog fields.
pub(crate) fn section_label(label: &'static str, cx: &App) -> Div {
    mono_label(label, cx)
}

/// Returns a row that shows a labelled value, such as the target of an action.
pub(crate) fn summary_row(label: &'static str, value: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .w_full()
        .gap_3()
        .px_2p5()
        .py_1p5()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted)
        .child(div().w(px(76.)).child(mono_label(label, cx)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(mono_font())
                .text_size(text_px(12.5))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(value.into()),
        )
}

fn option_row(
    title: &'static str,
    description: &'static str,
    toggle: impl IntoElement,
    cx: &App,
) -> Div {
    h_flex()
        .w_full()
        .gap_3()
        .px_2()
        .py_1p5()
        .rounded(px(6.))
        .hover(|row| row.bg(cx.theme().list_hover))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    div()
                        .text_size(text_px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(text_px(11.5))
                        .text_color(cx.theme().muted_foreground)
                        .child(description),
                ),
        )
        .child(toggle)
}

/// Returns a tinted note with a warning icon and a dithered edge in `color`.
pub(crate) fn callout(message: impl Into<SharedString>, color: Hsla) -> Div {
    h_flex()
        .relative()
        .w_full()
        .items_start()
        .gap_2()
        .pl_3p5()
        .pr_2p5()
        .py_2()
        .rounded(px(7.))
        .overflow_hidden()
        .border_1()
        .border_color(color.opacity(0.28))
        .bg(color.opacity(0.08))
        .text_color(color)
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(5.))
                .child(
                    Dither::new(DitherField::Fade {
                        from: Edge::Left,
                        start: 1.0,
                        end: 0.3,
                    })
                    .color(color)
                    .grid(1.0, 1.0)
                    .layer(),
                ),
        )
        .child(Icon::new(IconName::TriangleAlert).small())
        .child(callout_message(message))
}

fn callout_message(message: impl Into<SharedString>) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .whitespace_normal()
        .text_size(text_px(12.5))
        .child(message.into())
}

/// Returns a hairline that separates the parts of a dialog.
pub(crate) fn divider(cx: &App) -> Div {
    div().w_full().h(px(1.)).bg(cx.theme().border)
}
