//! The Git workspace: the open repositories, each with its History, Changes and Pull Requests
//! views and its sidebar, the remote actions, notifications and the first-run tour. Kodem's
//! window draws its parts: the title bar holds the repository and the remote actions, the
//! workspace rail holds the repository cards, the sidebar slot holds the repository's sidebar,
//! and the content area holds this view.
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    Anchor, Animation, AnimationExt as _, AnyElement, AnyView, App, Bounds, Context, Div,
    ElementId, Entity, EventEmitter, Focusable, Hsla, IntoElement, PathPromptOptions, Pixels,
    Render, SharedString, Stateful, Subscription, Window, canvas, div, point,
};
use kodem_core::Project;
use kodem_git::git::{FetchOptions, GitRepo, PullOptions, PushOptions};
use kodem_git::settings::PullStrategy;

use super::changes::{ChangesEvent, ChangesView};
use super::clone_repository::open_clone_dialog;
use super::components::{
    accent_count_badge, button, dither_icon_tile, full_size, mono_font, mono_label,
    repository_rail_labels, section_header, tab_cell, tab_indicator, tab_indicator_bar, tab_rail,
    text_field,
};
use super::history::HistoryView;
use super::icons;
use super::kit::{
    ActiveTheme as _, ButtonVariants as _, ContextMenuExt as _, Icon, IconName, InputState,
    Notification, Popover, PopupMenuItem, ScrollableElement as _, Selectable as _, Sizable as _,
    WindowExt as _, h_flex, v_flex,
};
use super::motion;
use super::onboarding::{OnboardingEvent, OnboardingExit, OnboardingView};
use super::pull_requests::PullRequestsView;
use super::remote_actions::{
    PushForceMode, RemoteOption, RemotePopupData, RemotePopupKind, remote_popup_content,
};
use super::settings::GitSettings;
use super::sidebar::SidebarView;
use super::state::{
    RepositoryEvent, RepositoryNotification, RepositoryNotificationStatus, RepositoryOperation,
    RepositoryState,
};
use super::theme;
use crate::theme::{px, text_px};
use crate::ui::dither::{Dither, DitherField, Edge, checker};
use crate::ui::fx;

/// Names the toast of each repository, so that a new result replaces the last one.
struct RepositoryOperationBanner;

/// Width of an operation toast, in points.
const TOAST_WIDTH: f32 = 260.;

/// Side of a repository card in the rail, in points.
const REPOSITORY_CARD_SIZE: f32 = 34.0;

/// What the Git workspace asks of the window around it.
#[derive(Debug, Clone)]
pub enum GitEvent {
    /// The user opened or cloned a repository; Kodem lists it as a project.
    RepositoryOpened(PathBuf),
    /// The user asked to change the name, icon and color of a repository's project.
    CustomizeRepository(PathBuf),
}

fn restored_active_repository_index(
    open_repositories: &[PathBuf],
    saved_active_repository: Option<&Path>,
) -> Option<usize> {
    saved_active_repository
        .and_then(|saved| {
            open_repositories
                .iter()
                .position(|repository| repository == saved)
        })
        .or_else(|| open_repositories.len().checked_sub(1))
}

/// The surface of a repository card without a Kodem project: a letter label on a dithered
/// fill in the accent.
fn repository_card_surface(selected: bool, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .relative()
        .size(px(REPOSITORY_CARD_SIZE))
        .flex_shrink_0()
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .rounded(px(9.))
        .border_1()
        .font_family(mono_font())
        .text_size(text_px(13.))
        .font_weight(gpui::FontWeight::BOLD)
        .when(selected, |card| {
            card.bg(theme.primary.opacity(0.16))
                .border_color(theme.primary.opacity(0.55))
                .text_color(theme.foreground)
        })
        .when(!selected, |card| {
            card.bg(theme.muted)
                .border_color(theme.border)
                .text_color(theme.muted_foreground)
                .hover(move |style| style.bg(theme.list_hover).text_color(theme.foreground))
        })
        .child(repository_card_dither(selected, theme.primary, 9.))
}

/// The dithered fill behind a repository card label. Selected cards show
/// a dense fill that rises from the bottom; other cards keep a light fill.
fn repository_card_dither(selected: bool, tone: Hsla, radius: f32) -> Div {
    let field = DitherField::Fade {
        from: Edge::Bottom,
        start: if selected { 0.95 } else { 0.5 },
        end: 0.0,
    };
    Dither::new(field)
        .color(tone.opacity(if selected { 0.7 } else { 0.32 }))
        .grid(2.0, 1.0)
        .corner_radius(radius)
        .layer()
}

/// The view of a repository that the content area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    History,
    Changes,
    PullRequests,
}

/// One open repository: its state, its three views and its sidebar.
pub struct RepoTab {
    pub state: Entity<RepositoryState>,
    pub history: Entity<HistoryView>,
    pub changes: Entity<ChangesView>,
    pub prs: Entity<PullRequestsView>,
    pub sidebar: Entity<SidebarView>,
    pub view: ViewTab,
}

/// The Git workspace of one window.
pub struct GitWorkspace {
    tabs: Vec<RepoTab>,
    active: usize,
    remote_popup: Option<RemotePopupKind>,
    push_force_mode: PushForceMode,
    show_force_confirmation: bool,
    notification_center_open: bool,
    commit_search_open: bool,
    popup_animation_sequence: u64,
    operation_animation_sequence: u64,
    content_animation_sequence: u64,
    /// Where the view tab indicator was when the user last switched tabs,
    /// relative to the tab rail. The indicator slides from there.
    tab_slide_from: Option<(f32, f32)>,
    /// Restarts the tab indicator slide on every switch.
    tab_slide_sequence: u64,
    /// The remote action the user started and which repository runs it,
    /// so its result can play an effect.
    pending_remote_fx: Option<(PathBuf, RemotePopupKind)>,
    /// The first-run tour, shown in place of the workspace's content until the user
    /// finishes it.
    onboarding: Option<Entity<OnboardingView>>,
    _onboarding_subscription: Option<Subscription>,
    /// Kodem's projects, which give the repositories their names, icons and colors.
    projects: Vec<Project>,
}

impl EventEmitter<GitEvent> for GitWorkspace {}

impl GitWorkspace {
    /// Creates the workspace and opens the repositories that were open last.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        GitSettings::init(cx);
        let mut this = Self {
            tabs: Vec::new(),
            active: 0,
            remote_popup: None,
            push_force_mode: PushForceMode::Normal,
            show_force_confirmation: false,
            notification_center_open: false,
            commit_search_open: false,
            popup_animation_sequence: 0,
            operation_animation_sequence: 0,
            content_animation_sequence: 0,
            tab_slide_from: None,
            tab_slide_sequence: 0,
            pending_remote_fx: None,
            onboarding: None,
            _onboarding_subscription: None,
            projects: Vec::new(),
        };

        let settings = GitSettings::get(cx).clone();
        if settings.restore_tabs_on_launch {
            for path in settings.open_repositories.clone() {
                if GitRepo::is_repository(&path) {
                    this.add_repository(path, window, cx);
                }
            }
            let open_repositories = this
                .tabs
                .iter()
                .map(|tab| tab.state.read(cx).repo.path.clone())
                .collect::<Vec<_>>();
            if let Some(active) = restored_active_repository_index(
                &open_repositories,
                settings.active_repository.as_deref(),
            ) {
                this.activate_repository(active, cx);
            }
        }

        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.refresh_active_changes(cx);
            }
        })
        .detach();
        cx.on_release(|_, cx| GitSettings::save(cx)).detach();

        this.start_auto_fetch(cx);
        this
    }

    /// Updates Kodem's projects, which give the repositories their names, icons and colors.
    pub fn set_projects(&mut self, projects: Vec<Project>, cx: &mut Context<Self>) {
        if self.projects != projects {
            self.projects = projects;
            cx.notify();
        }
    }

    /// Returns the sidebar of the open repository.
    pub fn sidebar(&self) -> Option<AnyView> {
        self.tabs
            .get(self.active)
            .map(|tab| AnyView::from(tab.sidebar.clone()))
    }

    // Returns Kodem's project of a repository folder.
    fn project(&self, path: &Path) -> Option<&Project> {
        self.projects.iter().find(|project| project.path == path)
    }

    fn start_auto_fetch(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut minutes: u64 = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                minutes += 1;
                let result = this.update(cx, |this, cx| {
                    let interval = GitSettings::get(cx).auto_fetch_interval as u64;
                    if interval == 0 || !minutes.is_multiple_of(interval) {
                        return;
                    }
                    let hide = GitSettings::get(cx).hide_checkpoint_commits;
                    if let Some(tab) = this.tabs.get(this.active) {
                        tab.state.update(cx, |state, cx| {
                            if state.busy.is_none() {
                                state.run_silent_operation(
                                    "Fetching",
                                    hide,
                                    |repo| repo.fetch(),
                                    cx,
                                );
                            }
                        });
                    }
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn refresh_active_changes(&self, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.view != ViewTab::Changes {
            return;
        }

        tab.state.update(cx, |state, cx| {
            if state.busy.is_none() {
                state.refresh_status(cx);
            }
        });
    }

    /// Opens a repository in a new tab, or shows its tab when it is open. Kodem lists it as a
    /// project as well.
    pub fn add_repository(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self
            .tabs
            .iter()
            .position(|t| t.state.read(cx).repo.path == path)
        {
            self.activate_repository(ix, cx);
            return;
        }

        let hide = GitSettings::get(cx).hide_checkpoint_commits;
        let state = cx.new(|_| RepositoryState::new(path.clone()));
        state.update(cx, |state, cx| state.refresh(hide, cx));

        let history = cx.new(|cx| HistoryView::new(state.clone(), window, cx));
        let changes = cx.new(|cx| ChangesView::new(state.clone(), window, cx));
        let prs = cx.new(|cx| PullRequestsView::new(state.clone(), cx));
        let sidebar = cx.new(|cx| SidebarView::new(state.clone(), window, cx));

        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        cx.subscribe_in(&changes, window, |this, _, event, window, cx| match event {
            ChangesEvent::OpenRepository(path) => {
                this.add_repository(path.clone(), window, cx);
            }
        })
        .detach();
        cx.subscribe_in(&state, window, |this, state, event, window, cx| {
            let key = state.read(cx).repo.path.display().to_string();
            match event {
                RepositoryEvent::OperationFinished(notification) => {
                    let success = notification.status == RepositoryNotificationStatus::Success;
                    let repo_path = state.read(cx).repo.path.clone();
                    let branch = state.read(cx).current_branch.clone();
                    this.play_remote_fx(&repo_path, success, &branch, cx);
                    if GitSettings::get(cx).show_notifications {
                        let status = notification.status;
                        let title = SharedString::from(notification.title.clone());
                        let body = SharedString::from(notification.body.clone());
                        let toast_sequence = notification.created_at.timestamp_millis() as u64;
                        window.push_notification(
                            Notification::new()
                                .content(move |_, _, cx| {
                                    motion::toast_surface(
                                        toast_sequence,
                                        operation_toast(status, title.clone(), body.clone(), cx),
                                    )
                                })
                                .id1::<RepositoryOperationBanner>(key)
                                .placement(Anchor::BottomLeft),
                            cx,
                        );
                    } else {
                        window.remove_notification1::<RepositoryOperationBanner>(key, cx);
                    }
                }
                RepositoryEvent::OperationStarted => {
                    this.operation_animation_sequence =
                        this.operation_animation_sequence.wrapping_add(1);
                }
                RepositoryEvent::Refreshed | RepositoryEvent::StatusRefreshed => {}
            }
            cx.notify();
        })
        .detach();

        self.tabs.push(RepoTab {
            state,
            history,
            changes,
            prs,
            sidebar,
            view: ViewTab::History,
        });
        self.active = self.tabs.len() - 1;

        GitSettings::update(cx, |s| {
            if !s.open_repositories.contains(&path) {
                s.open_repositories.push(path.clone());
            }
            if !s.recent_repositories.contains(&path) {
                s.recent_repositories.insert(0, path.clone());
                s.recent_repositories.truncate(10);
            }
            s.active_repository = Some(path.clone());
        });
        cx.emit(GitEvent::RepositoryOpened(path));
        cx.notify();
    }

    fn activate_repository(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(ix) else {
            return;
        };
        let path = tab.state.read(cx).repo.path.clone();
        if self.active != ix {
            self.content_animation_sequence = self.content_animation_sequence.wrapping_add(1);
        }
        self.active = ix;
        self.remote_popup = None;
        self.notification_center_open = false;
        self.commit_search_open = false;
        GitSettings::update_saved(cx, |settings| {
            settings.active_repository = Some(path);
        });
        cx.notify();
    }

    fn close_repository(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        let path = self.tabs[ix].state.read(cx).repo.path.clone();
        self.tabs.remove(ix);
        self.remote_popup = None;
        self.notification_center_open = false;
        self.commit_search_open = false;
        self.active = if self.tabs.is_empty() {
            0
        } else if ix < self.active {
            self.active - 1
        } else if ix == self.active {
            ix.min(self.tabs.len() - 1)
        } else {
            self.active
        };
        let active_repository = self
            .tabs
            .get(self.active)
            .map(|tab| tab.state.read(cx).repo.path.clone());
        GitSettings::update(cx, |s| {
            s.open_repositories.retain(|p| p != &path);
            s.active_repository = active_repository;
        });
        cx.notify();
    }

    /// Opens the Clone Repository dialog. The cloned repository opens in
    /// a new tab.
    pub fn clone_repository_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        open_clone_dialog(cx.entity().downgrade(), window, cx);
    }

    /// Shows the first-run tour while onboarding is not completed, such
    /// as the first time the Git workspace opens or after a reset in Preferences.
    fn sync_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let completed = GitSettings::get(cx).onboarding_completed;
        if completed {
            self.onboarding = None;
            self._onboarding_subscription = None;
            return;
        }
        if self.onboarding.is_some() {
            return;
        }
        let onboarding = cx.new(|cx| OnboardingView::new(window, cx));
        self._onboarding_subscription = Some(cx.subscribe_in(
            &onboarding,
            window,
            |this, _, event: &OnboardingEvent, window, cx| {
                let OnboardingEvent::Finished(exit) = event;
                GitSettings::update(cx, |settings| settings.onboarding_completed = true);
                this.onboarding = None;
                this._onboarding_subscription = None;
                match exit {
                    OnboardingExit::OpenRepository => this.open_repository_dialog(window, cx),
                    OnboardingExit::CloneRepository => this.clone_repository_dialog(window, cx),
                    OnboardingExit::Done => {}
                }
                cx.notify();
            },
        ));
        self.onboarding = Some(onboarding);
    }

    /// Asks for a repository folder and opens it.
    pub fn open_repository_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Repository".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                this.update_in(cx, |this, window, cx| {
                    for path in paths {
                        if GitRepo::is_repository(&path) {
                            this.add_repository(path, window, cx);
                        }
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    fn run_git(
        &mut self,
        label: &'static str,
        route: Option<String>,
        op: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let hide = GitSettings::get(cx).hide_checkpoint_commits;
        if let Some(tab) = self.tabs.get(self.active) {
            tab.state.update(cx, |state, cx| {
                state.run_operation_with_route(label, route, hide, op, cx);
            });
        }
    }

    fn set_remote_popup(&mut self, kind: RemotePopupKind, open: bool, cx: &mut Context<Self>) {
        if open {
            if self.remote_popup != Some(kind) {
                self.popup_animation_sequence = self.popup_animation_sequence.wrapping_add(1);
            }
            self.remote_popup = Some(kind);
            self.push_force_mode = PushForceMode::Normal;
            self.show_force_confirmation = false;
        } else if self.remote_popup == Some(kind) {
            self.remote_popup = None;
        }
        cx.notify();
    }

    /// Closes the Fetch, Pull or Push options.
    pub(crate) fn close_remote_popup(&mut self, cx: &mut Context<Self>) {
        self.remote_popup = None;
        self.show_force_confirmation = false;
        cx.notify();
    }

    /// Turns one option of the remote actions on or off. Force modes last for one push; the
    /// others are saved settings.
    pub(crate) fn set_remote_option(
        &mut self,
        option: RemoteOption,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        match option {
            RemoteOption::PushForceWithLease => {
                self.push_force_mode = if enabled {
                    PushForceMode::WithLease
                } else {
                    PushForceMode::Normal
                };
                self.show_force_confirmation = false;
            }
            RemoteOption::PushForce => {
                self.push_force_mode = if enabled {
                    PushForceMode::Force
                } else {
                    PushForceMode::Normal
                };
                self.show_force_confirmation = false;
            }
            _ => GitSettings::update(cx, |settings| match option {
                RemoteOption::FetchAllRemotes => settings.fetch_all_remotes = enabled,
                RemoteOption::FetchPrune => settings.prune_on_fetch = enabled,
                RemoteOption::FetchSubmodules => settings.fetch_submodules = enabled,
                RemoteOption::PullRebase => {
                    settings.pull_strategy = if enabled {
                        PullStrategy::Rebase
                    } else {
                        PullStrategy::Merge
                    }
                }
                RemoteOption::PullFastForwardOnly => {
                    settings.pull_strategy = if enabled {
                        PullStrategy::FastForward
                    } else {
                        PullStrategy::Merge
                    }
                }
                RemoteOption::PullNoCommit => settings.pull_no_commit = enabled,
                RemoteOption::PullLfs => settings.pull_lfs = enabled,
                RemoteOption::PullAutoStash => settings.autostash_before_rebase = enabled,
                RemoteOption::PullSubmodules => settings.pull_submodules = enabled,
                RemoteOption::PushSetUpstream => settings.set_upstream_on_push = enabled,
                RemoteOption::PushTags => settings.push_tags = enabled,
                RemoteOption::PushSubmodules => settings.push_submodules = enabled,
                RemoteOption::PushForceWithLease | RemoteOption::PushForce => {}
            }),
        }
        cx.notify();
    }

    /// Runs the remote action whose options are open. A forced push asks for confirmation
    /// first when destructive actions need it.
    pub(crate) fn confirm_remote_action(&mut self, cx: &mut Context<Self>) {
        match self.remote_popup {
            Some(RemotePopupKind::Fetch) => self.run_selected_fetch(cx),
            Some(RemotePopupKind::Pull) => self.run_selected_pull(cx),
            Some(RemotePopupKind::Push) if self.push_force_mode != PushForceMode::Normal => {
                if GitSettings::get(cx).confirm_destructive_actions {
                    self.show_force_confirmation = true;
                    self.popup_animation_sequence = self.popup_animation_sequence.wrapping_add(1);
                    cx.notify();
                } else {
                    self.run_selected_push(self.push_force_mode, cx);
                }
            }
            Some(RemotePopupKind::Push) => self.run_selected_push(self.push_force_mode, cx),
            None => {}
        }
    }

    /// Runs a push after the user confirmed its force mode.
    pub(crate) fn run_confirmed_push(&mut self, mode: PushForceMode, cx: &mut Context<Self>) {
        self.run_selected_push(mode, cx);
    }

    fn run_selected_fetch(&mut self, cx: &mut Context<Self>) {
        let settings = GitSettings::get(cx).clone();
        let route = self.tabs.get(self.active).and_then(|tab| {
            tab.state
                .read(cx)
                .fetch_operation_route(settings.fetch_all_remotes)
        });
        let options = FetchOptions {
            all_remotes: settings.fetch_all_remotes,
            prune: settings.prune_on_fetch,
            recurse_submodules: settings.fetch_submodules,
        };
        self.close_remote_popup(cx);
        self.expect_remote_fx(RemotePopupKind::Fetch, cx);
        self.run_git(
            "Fetching",
            route,
            move |repo| repo.fetch_with_options(options),
            cx,
        );
    }

    fn run_selected_pull(&mut self, cx: &mut Context<Self>) {
        let settings = GitSettings::get(cx).clone();
        let route = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.state.read(cx).pull_operation_route());
        let options = PullOptions {
            rebase: settings.pull_strategy == PullStrategy::Rebase,
            ff_only: settings.pull_strategy == PullStrategy::FastForward,
            no_commit: settings.pull_no_commit,
            pull_lfs: settings.pull_lfs,
            auto_stash: settings.autostash_before_rebase,
            recurse_submodules: settings.pull_submodules,
        };
        self.close_remote_popup(cx);
        self.expect_remote_fx(RemotePopupKind::Pull, cx);
        self.run_git(
            "Pulling",
            route,
            move |repo| repo.pull_with_options(options),
            cx,
        );
    }

    fn run_selected_push(&mut self, mode: PushForceMode, cx: &mut Context<Self>) {
        let settings = GitSettings::get(cx).clone();
        let route = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.state.read(cx).push_operation_route());
        let options = PushOptions {
            force: mode == PushForceMode::Force,
            force_with_lease: mode == PushForceMode::WithLease,
            set_upstream: settings.set_upstream_on_push,
            tags: settings.push_tags,
            recurse_submodules: settings.push_submodules,
        };
        self.close_remote_popup(cx);
        self.expect_remote_fx(RemotePopupKind::Push, cx);
        self.run_git(
            "Pushing",
            route,
            move |repo| repo.push_with_options(options),
            cx,
        );
    }

    /// Remembers which remote action the user started, so its result can
    /// play an effect. Skipped while another operation is running,
    /// because the new one will not start.
    fn expect_remote_fx(&mut self, kind: RemotePopupKind, cx: &mut Context<Self>) {
        self.pending_remote_fx = self.tabs.get(self.active).and_then(|tab| {
            let state = tab.state.read(cx);
            state
                .busy
                .is_none()
                .then(|| (state.repo.path.clone(), kind))
        });
    }

    /// Plays the effect for a finished remote action: a pushed branch
    /// flies off the top of the window, pulled and fetched work drops
    /// into its button, and a failure hits the button in red.
    fn play_remote_fx(
        &mut self,
        repo_path: &Path,
        success: bool,
        branch: &str,
        cx: &mut Context<Self>,
    ) {
        let Some((path, kind)) = self.pending_remote_fx.take() else {
            return;
        };
        if path != repo_path {
            self.pending_remote_fx = Some((path, kind));
            return;
        }
        let colors = theme::liquid_colors(cx);
        let accent = cx.theme().primary;
        let anchor = match kind {
            RemotePopupKind::Fetch => "remote-fetch",
            RemotePopupKind::Pull => "remote-pull",
            RemotePopupKind::Push => "remote-push",
        };
        if !success {
            fx::impact(anchor, colors.danger, cx);
            return;
        }
        let Some(button) = fx::anchor_bounds(anchor, cx) else {
            return;
        };
        let above = point(button.center().x, gpui::px(-40.));
        let branch = if branch.is_empty() { "HEAD" } else { branch };
        match kind {
            RemotePopupKind::Push => {
                fx::impact(anchor, colors.success, cx);
                fx::fly_away(anchor, above, format!("↑ {branch}"), colors.success, cx);
            }
            RemotePopupKind::Pull => {
                fx::fly_to(
                    above,
                    anchor,
                    format!("↓ {branch}"),
                    accent,
                    Duration::ZERO,
                    cx,
                );
                // The pulled work continues on into the history.
                fx::fly_between(
                    anchor,
                    "tab-history",
                    "new commits",
                    accent,
                    Duration::from_millis(820),
                    cx,
                );
            }
            RemotePopupKind::Fetch => {
                fx::fly_to(above, anchor, "fetched", colors.info, Duration::ZERO, cx);
            }
        }
    }

    /// Builds the title bar's content: the workspace's name and Beta badge, the repository, its
    /// branch with the commits ahead and behind, the running operation and the remote actions.
    pub fn render_title_bar(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let active_tab = self.tabs.get(self.active);
        let (branch, ahead, behind, busy) = active_tab
            .map(|t| {
                let s = t.state.read(cx);
                (s.current_branch.clone(), s.ahead, s.behind, s.busy.clone())
            })
            .unwrap_or_default();
        let repository = active_tab.map(|t| {
            let state = t.state.read(cx);
            let path = state.repo.path.clone();
            let name = self
                .project(&path)
                .map_or_else(|| state.name(), |project| project.name.clone());
            (name, path)
        });
        let has_repo = active_tab.is_some();
        let busy_active = busy.is_some();
        let settings = GitSettings::get(cx).clone();
        let workspace = cx.entity();
        let popup_animation_sequence = self.popup_animation_sequence;
        let kodem = cx.theme().kodem;

        let fetch_data = RemotePopupData {
            kind: RemotePopupKind::Fetch,
            branch: branch.clone(),
            busy: busy_active,
            settings: settings.clone(),
            force_mode: PushForceMode::Normal,
            show_force_confirmation: false,
        };
        let pull_data = RemotePopupData {
            kind: RemotePopupKind::Pull,
            branch: branch.clone(),
            busy: busy_active,
            settings: settings.clone(),
            force_mode: PushForceMode::Normal,
            show_force_confirmation: false,
        };
        let push_data = RemotePopupData {
            kind: RemotePopupKind::Push,
            branch: branch.clone(),
            busy: busy_active,
            settings,
            force_mode: self.push_force_mode,
            show_force_confirmation: self.show_force_confirmation,
        };

        let colors = theme::liquid_colors(cx);
        let remote_popup = self.remote_popup;

        h_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_2()
            .items_center()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap(px(7.))
                    .child(
                        icons::git_pull_request()
                            .size(px(14.))
                            .text_color(cx.theme().primary),
                    )
                    .child(
                        div()
                            .text_size(text_px(13.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(cx.theme().foreground)
                            .child("Git"),
                    )
                    .child(crate::ui::kit::chip("BETA", kodem.accent)),
            )
            .when_some(repository, |this, (name, path)| {
                let project = self.project(&path).cloned();
                this.child(
                    div()
                        .text_size(text_px(13.))
                        .text_color(colors.text_tertiary)
                        .child("·"),
                )
                .child(
                    h_flex()
                        .min_w_0()
                        .gap_2()
                        .items_center()
                        .when_some(project, |row, project| {
                            row.child(crate::ui::sidebar::project_icon(&project, kodem))
                        })
                        .child(
                            div()
                                .text_size(text_px(13.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .whitespace_nowrap()
                                .child(SharedString::from(name)),
                        )
                        .child(
                            div()
                                .text_size(text_px(13.))
                                .text_color(colors.text_tertiary)
                                .child("/"),
                        )
                        .child(
                            h_flex()
                                .relative()
                                .min_w_0()
                                .gap_1()
                                .items_center()
                                .text_color(colors.text_secondary)
                                .child(fx::anchor("title-branch"))
                                .child(icons::git_branch().xsmall())
                                .child(
                                    div()
                                        .font_family(mono_font())
                                        .text_size(text_px(12.))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .child(if branch.is_empty() {
                                            SharedString::from("detached")
                                        } else {
                                            SharedString::from(branch.clone())
                                        }),
                                ),
                        )
                        .when(ahead > 0 || behind > 0, |this| {
                            this.child(
                                div()
                                    .font_family(mono_font())
                                    .text_size(text_px(11.))
                                    .text_color(colors.text_tertiary)
                                    .child(format!("↑{ahead} ↓{behind}")),
                            )
                        }),
                )
            })
            .child(div().flex_1())
            .when_some(busy, |this, label| {
                this.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div().relative().w(px(30.)).h(px(9.)).child(
                                Dither::new(DitherField::Flow {
                                    scale: 0.3,
                                    speed: 7.0,
                                    bias: -0.05,
                                })
                                .color(cx.theme().primary)
                                .grid(3.0, 2.0)
                                .animated()
                                .layer(),
                            ),
                        )
                        .child(mono_label(format!("{label}…"), cx)),
                )
            })
            .when(has_repo, |this| {
                let fetch_workspace = workspace.clone();
                let fetch_content_workspace = workspace.clone();
                let pull_workspace = workspace.clone();
                let pull_content_workspace = workspace.clone();
                let push_workspace = workspace.clone();
                let push_content_workspace = workspace.clone();
                this.child(
                    h_flex()
                        .h(px(30.))
                        .p(px(2.))
                        .gap(px(3.))
                        .items_center()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().muted)
                        // A press on the actions must not start a move of the window: they
                        // occlude the title bar under them.
                        .occlude()
                        .child(
                            Popover::new("fetch-popup")
                                .appearance(false)
                                .open(remote_popup == Some(RemotePopupKind::Fetch))
                                .trigger(
                                    button("fetch")
                                        .child(fx::anchor("remote-fetch"))
                                        .ghost()
                                        .xsmall()
                                        .icon(icons::refresh())
                                        .label("Fetch")
                                        .selected(remote_popup == Some(RemotePopupKind::Fetch)),
                                )
                                .content(move |_, _, cx| {
                                    motion::popover_surface(
                                        "fetch-popup-motion",
                                        popup_animation_sequence,
                                        px(8.0),
                                        remote_popup_content(
                                            fetch_data.clone(),
                                            fetch_content_workspace.clone(),
                                            cx,
                                        ),
                                        cx,
                                    )
                                })
                                .on_open_change(move |open, _, cx| {
                                    fetch_workspace.update(cx, |this, cx| {
                                        this.set_remote_popup(RemotePopupKind::Fetch, *open, cx)
                                    });
                                }),
                        )
                        .child(
                            div()
                                .relative()
                                .child(
                                    Popover::new("pull-popup")
                                        .appearance(false)
                                        .open(remote_popup == Some(RemotePopupKind::Pull))
                                        .trigger(
                                            button("pull")
                                                .child(fx::anchor("remote-pull"))
                                                .ghost()
                                                .xsmall()
                                                .icon(icons::pull())
                                                .label("Pull")
                                                .selected(
                                                    remote_popup == Some(RemotePopupKind::Pull),
                                                ),
                                        )
                                        .content(move |_, _, cx| {
                                            motion::popover_surface(
                                                "pull-popup-motion",
                                                popup_animation_sequence,
                                                px(8.0),
                                                remote_popup_content(
                                                    pull_data.clone(),
                                                    pull_content_workspace.clone(),
                                                    cx,
                                                ),
                                                cx,
                                            )
                                        })
                                        .on_open_change(move |open, _, cx| {
                                            pull_workspace.update(cx, |this, cx| {
                                                this.set_remote_popup(
                                                    RemotePopupKind::Pull,
                                                    *open,
                                                    cx,
                                                )
                                            });
                                        }),
                                )
                                .when(behind > 0, |this| {
                                    this.child(
                                        accent_count_badge(
                                            behind as usize,
                                            remote_popup == Some(RemotePopupKind::Pull),
                                            cx,
                                        )
                                        .absolute()
                                        .top(px(-6.))
                                        .right(px(-4.)),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .relative()
                                .child(
                                    Popover::new("push-popup")
                                        .appearance(false)
                                        .open(remote_popup == Some(RemotePopupKind::Push))
                                        .trigger(
                                            button("push")
                                                .child(fx::anchor("remote-push"))
                                                .ghost()
                                                .xsmall()
                                                .icon(icons::push())
                                                .label("Push")
                                                .selected(
                                                    remote_popup == Some(RemotePopupKind::Push),
                                                ),
                                        )
                                        .content(move |_, _, cx| {
                                            motion::popover_surface(
                                                "push-popup-motion",
                                                popup_animation_sequence,
                                                px(8.0),
                                                remote_popup_content(
                                                    push_data.clone(),
                                                    push_content_workspace.clone(),
                                                    cx,
                                                ),
                                                cx,
                                            )
                                        })
                                        .on_open_change(move |open, _, cx| {
                                            push_workspace.update(cx, |this, cx| {
                                                this.set_remote_popup(
                                                    RemotePopupKind::Push,
                                                    *open,
                                                    cx,
                                                )
                                            });
                                        }),
                                )
                                .when(ahead > 0, |this| {
                                    this.child(
                                        accent_count_badge(
                                            ahead as usize,
                                            remote_popup == Some(RemotePopupKind::Push),
                                            cx,
                                        )
                                        .absolute()
                                        .top(px(-6.))
                                        .right(px(-4.)),
                                    )
                                }),
                        ),
                )
            })
            .into_any_element()
    }

    /// Builds the workspace rail's part of the Git workspace: one card per open repository, the
    /// buttons that open and clone repositories, and at the bottom the notification center,
    /// commit search and the Git preferences.
    pub fn render_rail(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let workspace = cx.entity();
        let notification_state = self.tabs.get(self.active).map(|tab| tab.state.clone());
        let (notifications, notification_operation, notification_branch, unread_count) =
            notification_state
                .as_ref()
                .map(|state| {
                    let state = state.read(cx);
                    (
                        state.notifications.clone(),
                        state.active_operation.clone(),
                        state.current_branch.clone(),
                        state.unread_notification_count(),
                    )
                })
                .unwrap_or_default();
        let notification_workspace = workspace.clone();
        let popup_animation_sequence = self.popup_animation_sequence;
        let repository_names = self
            .tabs
            .iter()
            .map(|tab| tab.state.read(cx).name())
            .collect::<Vec<_>>();
        let repository_labels = repository_rail_labels(&repository_names);
        let kodem = cx.theme().kodem;
        let repository_cards = self
            .tabs
            .iter()
            .zip(repository_names)
            .zip(repository_labels)
            .map(|((tab, name), label)| {
                let path = tab.state.read(cx).repo.path.clone();
                let project = self.project(&path).cloned();
                (name, label, path, project)
            })
            .collect::<Vec<_>>();
        v_flex()
            .size_full()
            .items_center()
            .gap_2()
            .children(repository_cards.into_iter().enumerate().map(
                |(ix, (name, label, path, project))| {
                    let selected = ix == self.active;
                    let tooltip_name = SharedString::from(
                        project
                            .as_ref()
                            .map_or_else(|| name.clone(), |project| project.name.clone()),
                    );
                    let tone = project.as_ref().map_or(kodem.accent, |project| {
                        crate::ui::sidebar::project_tint(project, kodem)
                    });
                    let workspace = workspace.clone();
                    let customizable = project.is_some();
                    let card = match &project {
                        // A Kodem project draws its own tile: its glyph or image in its color.
                        Some(project) => div()
                            .relative()
                            .size(px(REPOSITORY_CARD_SIZE))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(10.))
                            .cursor_pointer()
                            .border_1()
                            .border_color(if selected {
                                tone.opacity(0.6)
                            } else {
                                gpui::transparent_black()
                            })
                            .when(!selected, |card| {
                                card.opacity(0.72).hover(|style| style.opacity(1.))
                            })
                            .child(crate::ui::sidebar::project_task_tile(project, kodem)),
                        None => repository_card_surface(selected, cx).child(
                            div()
                                .relative()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(label),
                        ),
                    };
                    let card = card
                        .id(("repo-square", ix))
                        .child(fx::anchor(repo_card_anchor(ix)))
                        .tooltip(super::kit::Tooltip::new(tooltip_name).into_builder())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.active != ix {
                                // The chosen card takes a hit as it lights up.
                                fx::impact(&repo_card_anchor(ix), tone, cx);
                            }
                            this.activate_repository(ix, cx);
                        }))
                        .context_menu(move |menu, _, _| {
                            let close_workspace = workspace.clone();
                            let customize_workspace = workspace.clone();
                            let customize_path = path.clone();
                            menu.item(
                                PopupMenuItem::new("Customize…")
                                    .icon(IconName::Palette)
                                    .disabled(!customizable)
                                    .on_click(move |_, _, cx| {
                                        let path = customize_path.clone();
                                        customize_workspace.update(cx, |_, cx| {
                                            cx.emit(GitEvent::CustomizeRepository(path));
                                        });
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("Close Repository").on_click(move |_, _, cx| {
                                    close_workspace.update(cx, |this, cx| {
                                        this.close_repository(ix, cx);
                                    });
                                }),
                            )
                        });
                    div()
                        .relative()
                        .w_full()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top(px(7.))
                                .w(px(3.))
                                .h(px(20.))
                                .rounded_r(px(2.))
                                .when(selected, |pill| pill.bg(tone)),
                        )
                        .child(card)
                },
            ))
            .child(
                rail_action("add-repo-square", Icon::new(IconName::Plus), cx)
                    .tooltip(super::kit::Tooltip::new("Open repository · ⌘O").into_builder())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_repository_dialog(window, cx);
                    })),
            )
            .child(
                rail_action("clone-repo-square", icons::pull(), cx)
                    .tooltip(super::kit::Tooltip::new("Clone repository · ⇧⌘O").into_builder())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.clone_repository_dialog(window, cx);
                    })),
            )
            .child(div().flex_1())
            .child(
                v_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .when_some(notification_state, |this, state| {
                        let open_state = state.clone();
                        let content_state = state.clone();
                        this.child(
                            div()
                                .relative()
                                .w_full()
                                .flex()
                                .justify_center()
                                .child(
                                    Popover::new("notification-center")
                                        .beside()
                                        .open(self.notification_center_open)
                                        .trigger(
                                            button("open-notifications")
                                                .ghost()
                                                .icon(IconName::Bell)
                                                .tooltip("Notifications"),
                                        )
                                        .content(move |_, _, cx| {
                                            motion::popover_surface(
                                                "notification-center-motion",
                                                popup_animation_sequence,
                                                px(4.0),
                                                notification_center_content(
                                                    notifications.clone(),
                                                    notification_operation.clone(),
                                                    notification_branch.clone(),
                                                    content_state.clone(),
                                                    cx,
                                                ),
                                                cx,
                                            )
                                        })
                                        .on_open_change(move |open, _, cx| {
                                            notification_workspace.update(cx, |workspace, cx| {
                                                if *open && !workspace.notification_center_open {
                                                    workspace.popup_animation_sequence = workspace
                                                        .popup_animation_sequence
                                                        .wrapping_add(1);
                                                }
                                                workspace.notification_center_open = *open;
                                                if *open {
                                                    workspace.commit_search_open = false;
                                                }
                                                cx.notify();
                                            });
                                            if *open {
                                                open_state.update(cx, |state, cx| {
                                                    state.mark_notifications_read(cx)
                                                });
                                            }
                                        }),
                                )
                                .when(unread_count > 0, |this| {
                                    this.child(
                                        accent_count_badge(
                                            unread_count,
                                            self.notification_center_open,
                                            cx,
                                        )
                                        .absolute()
                                        .top(px(-3.))
                                        .right(px(3.)),
                                    )
                                }),
                        )
                    })
                    .when_some(
                        self.tabs
                            .get(self.active)
                            .map(|t| t.history.read(cx).search.clone()),
                        |this, search| {
                            let search_content = search.clone();
                            let search_workspace = workspace.clone();
                            let search_focus = search.read(cx).focus_handle(cx);
                            this.child(
                                Popover::new("commit-search")
                                    .beside()
                                    .open(self.commit_search_open)
                                    .track_focus(&search_focus)
                                    .trigger(
                                        button("open-commit-search")
                                            .ghost()
                                            .icon(IconName::Search)
                                            .tooltip("Search commits"),
                                    )
                                    .content(move |_, _, cx| {
                                        motion::popover_surface(
                                            "commit-search-motion",
                                            popup_animation_sequence,
                                            px(4.0),
                                            commit_search_content(search_content.clone(), cx),
                                            cx,
                                        )
                                    })
                                    .on_open_change(move |open, _, cx| {
                                        search_workspace.update(cx, |workspace, cx| {
                                            if *open && !workspace.commit_search_open {
                                                workspace.popup_animation_sequence = workspace
                                                    .popup_animation_sequence
                                                    .wrapping_add(1);
                                            }
                                            workspace.commit_search_open = *open;
                                            if *open {
                                                workspace.notification_center_open = false;
                                                if let Some(tab) =
                                                    workspace.tabs.get_mut(workspace.active)
                                                {
                                                    tab.view = ViewTab::History;
                                                }
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                        },
                    )
                    .child(
                        button("open-settings")
                            .ghost()
                            .icon(IconName::Settings)
                            .tooltip("Git preferences · ⌘,")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(crate::ui::ShowSettings), cx);
                            }),
                    ),
            )
            .into_any_element()
    }

    /// Closes the remote options, the notification center and commit search, as when the
    /// window leaves the Git workspace.
    pub fn dismiss_popovers(&mut self, cx: &mut Context<Self>) {
        self.remote_popup = None;
        self.show_force_confirmation = false;
        self.notification_center_open = false;
        self.commit_search_open = false;
        cx.notify();
    }

    /// Returns the folders of the open repositories.
    pub fn repository_paths(&self, cx: &App) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .map(|tab| tab.state.read(cx).repo.path.clone())
            .collect()
    }

    /// Opens commit search over the History of the open repository.
    pub fn open_commit_search(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        tab.view = ViewTab::History;
        if !self.commit_search_open {
            self.popup_animation_sequence = self.popup_animation_sequence.wrapping_add(1);
        }
        self.commit_search_open = true;
        self.notification_center_open = false;
        cx.notify();
    }

    /// Records where the tab indicator is before switching to `next`, so
    /// it slides from there to the new tab.
    fn start_tab_slide(&mut self, next: ViewTab, cx: &mut Context<Self>) {
        let Some(current) = self.tabs.get(self.active).map(|tab| tab.view) else {
            return;
        };
        if current == next {
            return;
        }
        self.tab_slide_from = indicator_target(current, cx);
        self.tab_slide_sequence = self.tab_slide_sequence.wrapping_add(1);
    }

    // Builds one view tab of the tab rail.
    fn view_tab(
        &self,
        id: &'static str,
        icon: Icon,
        label: &'static str,
        this_view: ViewTab,
        badge: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let view = self.tabs.get(self.active).map(|tab| tab.view);
        let selected = view == Some(this_view);
        let sliding = view.is_some_and(|view| indicator_target(view, cx).is_some());
        let color = if selected {
            cx.theme().foreground
        } else {
            cx.theme().muted_foreground
        };
        tab_cell(selected, cx)
            .id(id)
            .px(px(10.))
            .rounded_t(px(6.))
            .child(fx::anchor(id))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.start_tab_slide(this_view, cx);
                let (entered_changes, switched) = if let Some(tab) = this.tabs.get_mut(this.active)
                {
                    let entered_changes =
                        tab.view != ViewTab::Changes && this_view == ViewTab::Changes;
                    let switched = tab.view != this_view;
                    tab.view = this_view;
                    if this_view == ViewTab::PullRequests {
                        tab.prs.update(cx, |prs, cx| prs.ensure_loaded(cx));
                    }
                    (entered_changes, switched)
                } else {
                    (false, false)
                };
                if switched {
                    this.content_animation_sequence =
                        this.content_animation_sequence.wrapping_add(1);
                }
                if entered_changes {
                    this.refresh_active_changes(cx);
                }
                cx.notify();
            }))
            .child(
                h_flex()
                    .gap(px(6.))
                    .items_center()
                    .child(icon.small().text_color(if selected {
                        cx.theme().primary
                    } else {
                        color
                    }))
                    .child(
                        div()
                            .text_size(text_px(12.5))
                            .text_color(color)
                            .when(selected, |s| s.font_weight(gpui::FontWeight::SEMIBOLD))
                            .child(label),
                    )
                    .when(badge > 0, |this| {
                        this.child(accent_count_badge(badge, false, cx))
                    }),
            )
            .child(tab_indicator(selected && !sliding, cx))
    }

    fn render_view_tabs(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(tab) = self.tabs.get(self.active) else {
            return div();
        };
        let view = tab.view;
        let target = indicator_target(view, cx);
        let (changes_count, ahead_behind, commit_count, staged_count) = {
            let s = tab.state.read(cx);
            (
                s.changes_count(),
                (s.ahead + s.behind) as usize,
                s.commits.len(),
                s.status.staged.len(),
            )
        };

        let summary = match view {
            ViewTab::History => format!("{commit_count} commits"),
            ViewTab::Changes => format!("{staged_count} staged · {changes_count} changed"),
            ViewTab::PullRequests => String::from("GitHub"),
        };

        let slider = target.map(|target| {
            tab_slider(
                self.tab_slide_from,
                target,
                self.tab_slide_sequence,
                cx.theme().primary,
            )
        });

        let history = self.view_tab(
            "tab-history",
            icons::clock(),
            "History",
            ViewTab::History,
            ahead_behind,
            cx,
        );
        let changes = self.view_tab(
            "tab-changes",
            icons::file_text(),
            "Changes",
            ViewTab::Changes,
            changes_count,
            cx,
        );
        let prs = self.view_tab(
            "tab-prs",
            icons::git_pull_request(),
            "Pull Requests",
            ViewTab::PullRequests,
            0,
            cx,
        );
        tab_rail(cx)
            .relative()
            .gap(px(2.))
            .child(fx::anchor(TAB_RAIL_ANCHOR))
            .children(slider)
            .child(history)
            .child(changes)
            .child(prs)
            .child(div().flex_1())
            .child(div().pr(px(8.)).child(mono_label(summary, cx)))
            // The indicator is placed from the last frame's tab bounds.
            // When a tab has since changed width, for example because its
            // badge count changed, draw once more with the new bounds.
            .child(
                canvas(
                    move |_, window, cx| {
                        if indicator_target(view, cx) != target {
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }

    fn render_empty(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::liquid_colors(cx);
        let accent = cx.theme().primary;
        let recents = GitSettings::get(cx).recent_repositories.clone();
        let last_repository = recents.first().cloned();

        let hero = div()
            .relative()
            .w_full()
            .h(px(236.))
            .flex_shrink_0()
            .rounded(px(14.))
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .bg(colors.panel)
            .child(
                Dither::new(DitherField::Wave {
                    level: 0.44,
                    amplitude: 0.07,
                    speed: 1.4,
                })
                .color(accent)
                .grid(4.0, 3.0)
                .toned()
                .animated()
                .corner_radius(14.0)
                .layer(),
            )
            .child(
                Dither::new(
                    DitherField::Glow {
                        x: 0.92,
                        y: 0.05,
                        radius: 0.42,
                        strength: 0.9,
                    }
                    .times(DitherField::Flow {
                        scale: 0.12,
                        speed: 1.2,
                        bias: 0.1,
                    }),
                )
                .color(colors.ink)
                .grid(4.0, 3.0)
                .animated()
                .corner_radius(14.0)
                .layer(),
            )
            .child(
                v_flex()
                    .relative()
                    .p(px(24.))
                    .gap(px(4.))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(mono_label("Kodem · Git workspace", cx))
                            .child(crate::ui::kit::chip("BETA", accent)),
                    )
                    .child(
                        div()
                            .font_family(mono_font())
                            .text_size(text_px(44.))
                            .line_height(text_px(50.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(cx.theme().foreground)
                            .child("git"),
                    )
                    .child(
                        div()
                            .max_w(px(380.))
                            .text_size(text_px(12.5))
                            .text_color(colors.text_secondary)
                            .child(
                                "A native Git client inside Kodem. Open a repository to see its \
                                 history, changes, and pull requests.",
                            ),
                    ),
            );

        let mut cards = h_flex().w_full().gap(px(12.)).child(
            welcome_card(
                "welcome-open",
                Icon::new(IconName::FolderOpen),
                "Open Repository",
                SharedString::from("Choose a folder on disk"),
                Some("⌘O"),
                DitherField::Ember {
                    scale: 0.16,
                    speed: 2.4,
                },
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_repository_dialog(window, cx);
            })),
        );
        cards = cards.child(
            welcome_card(
                "welcome-clone",
                icons::pull(),
                "Clone Repository",
                SharedString::from("From GitHub or any URL"),
                Some("⇧⌘O"),
                DitherField::Wave {
                    level: 0.3,
                    amplitude: 0.12,
                    speed: 1.8,
                },
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.clone_repository_dialog(window, cx);
            })),
        );
        if let Some(path) = last_repository {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            cards = cards.child(
                welcome_card(
                    "welcome-reopen",
                    icons::clock(),
                    "Reopen Last",
                    SharedString::from(name),
                    None,
                    DitherField::Flow {
                        scale: 0.1,
                        speed: 1.6,
                        bias: -0.18,
                    }
                    .times(DitherField::Fade {
                        from: Edge::Bottom,
                        start: 1.0,
                        end: 0.2,
                    }),
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    if GitRepo::is_repository(&path) {
                        this.add_repository(path.clone(), window, cx);
                    }
                })),
            );
        }
        cards = cards.child(
            welcome_card(
                "welcome-settings",
                Icon::new(IconName::Settings),
                "Preferences",
                SharedString::from("Accounts, AI, and Git options"),
                Some("⌘,"),
                DitherField::Glow {
                    x: 0.2,
                    y: 1.0,
                    radius: 0.9,
                    strength: 0.8,
                }
                .times(DitherField::Flow {
                    scale: 0.2,
                    speed: 1.0,
                    bias: 0.0,
                }),
                cx,
            )
            .on_click(|_, window, cx| {
                window.dispatch_action(Box::new(crate::ui::ShowSettings), cx);
            }),
        );

        let recent_count = recents.len().min(6);
        let palette = theme::graph_colors(cx);
        let recent_list = v_flex()
            .w_full()
            .gap(px(4.))
            .child(div().px(px(4.)).pb(px(4.)).child(section_header(
                "Recent",
                Some(recent_count),
                cx,
            )))
            .children(recents.into_iter().take(6).enumerate().map(|(ix, path)| {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                let initial = name
                    .chars()
                    .next()
                    .map(|character| character.to_uppercase().to_string())
                    .unwrap_or_else(|| "?".to_string());
                let display_path = home_relative_path(&path);
                let tone = palette[1 + ix % (palette.len() - 1)];
                let project = self.project(&path).cloned();
                let list_hover = cx.theme().list_hover;
                h_flex()
                    .id(("recent", ix))
                    .w_full()
                    .px(px(8.))
                    .py(px(6.))
                    .gap(px(12.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .hover(move |style| style.bg(list_hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if GitRepo::is_repository(&path) {
                            this.add_repository(path.clone(), window, cx);
                        }
                    }))
                    .child(match project {
                        Some(project) => {
                            crate::ui::sidebar::project_task_tile(&project, cx.theme().kodem)
                        }
                        None => div()
                            .relative()
                            .size(px(28.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.))
                            .overflow_hidden()
                            .border_1()
                            .border_color(tone.opacity(0.35))
                            .bg(tone.opacity(0.08))
                            .child(
                                Dither::new(DitherField::Fade {
                                    from: Edge::Bottom,
                                    start: 0.8,
                                    end: 0.0,
                                })
                                .color(tone.opacity(0.6))
                                .grid(2.0, 1.0)
                                .corner_radius(7.0)
                                .layer(),
                            )
                            .child(
                                div()
                                    .relative()
                                    .font_family(mono_font())
                                    .text_size(text_px(12.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(cx.theme().foreground)
                                    .child(initial),
                            )
                            .into_any_element(),
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(text_px(12.5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(cx.theme().foreground)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .font_family(mono_font())
                                    .text_size(text_px(11.))
                                    .text_color(colors.text_tertiary)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(display_path),
                            ),
                    )
                    .child(
                        Icon::new(IconName::ChevronRight)
                            .xsmall()
                            .text_color(colors.text_tertiary),
                    )
            }));

        v_flex()
            .id("welcome")
            .size_full()
            .items_center()
            .overflow_y_scrollbar()
            .bg(colors.window_backdrop)
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(780.))
                    .px(px(32.))
                    .py(px(40.))
                    .gap(px(16.))
                    .child(hero)
                    .child(cards)
                    .when(recent_count > 0, |this| {
                        this.child(div().pt(px(12.)).child(recent_list))
                    }),
            )
            .into_any_element()
    }
}

const TAB_RAIL_ANCHOR: &str = "tab-rail";

/// The anchor name of a repository card in the rail.
fn repo_card_anchor(ix: usize) -> SharedString {
    SharedString::from(format!("repo-card-{ix}"))
}
const TAB_SLIDE_DURATION: Duration = Duration::from_millis(420);
/// Inset of the tab indicator from the sides of its tab.
const TAB_INDICATOR_INSET: f32 = 4.0;

/// The anchor name of a view tab. It matches the tab's element id.
fn view_tab_anchor(view: ViewTab) -> &'static str {
    match view {
        ViewTab::History => "tab-history",
        ViewTab::Changes => "tab-changes",
        ViewTab::PullRequests => "tab-prs",
    }
}

/// The left and right edges of the indicator under a tab, relative to
/// the tab rail, in window pixels.
fn indicator_span(tab: Bounds<Pixels>, rail: Bounds<Pixels>) -> (f32, f32) {
    let inset = TAB_INDICATOR_INSET * crate::theme::ui_scale();
    let left = f32::from(tab.left() - rail.left()) + inset;
    let right = f32::from(tab.right() - rail.left()) - inset;
    (left, right.max(left))
}

/// The indicator edges under a view tab, from the last painted frame.
fn indicator_target(view: ViewTab, cx: &mut App) -> Option<(f32, f32)> {
    let rail = fx::anchor_bounds(TAB_RAIL_ANCHOR, cx)?;
    fx::anchor_bounds(view_tab_anchor(view), cx).map(|tab| indicator_span(tab, rail))
}

/// The tab indicator drawn on the rail, sliding from `from` to `to`
/// when a slide is pending. Edges are in window pixels.
fn tab_slider(from: Option<(f32, f32)>, to: (f32, f32), sequence: u64, accent: Hsla) -> AnyElement {
    let bar = move |(left, right): (f32, f32)| {
        div()
            .absolute()
            .bottom_0()
            .left(gpui::px(left))
            .w(gpui::px(right - left))
            .h(px(9.))
            .child(tab_indicator_bar(accent))
    };
    let Some(from) = from.filter(|from| *from != to) else {
        return bar(to).into_any_element();
    };
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .with_animation(
            ElementId::NamedInteger("tab-slide".into(), sequence),
            Animation::new(TAB_SLIDE_DURATION),
            move |layer, delta| layer.child(bar(fx::stretch_span(from, to, delta))),
        )
        .into_any_element()
}

/// Shows a path with the home folder written as `~`.
pub(crate) fn home_relative_path(path: &Path) -> String {
    match kodem_core::platform::home_dir()
        .ok()
        .and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
    {
        Some(relative) => format!("~/{}", relative.display()),
        None => path.display().to_string(),
    }
}

/// A square ghost action of the rail with a dashed edge, such as Open or Clone repository.
fn rail_action(id: &'static str, icon: Icon, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .size(px(REPOSITORY_CARD_SIZE))
        .flex_shrink_0()
        .rounded(px(9.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .border_1()
        .border_dashed()
        .border_color(theme.muted_foreground.opacity(0.35))
        .text_color(theme.muted_foreground)
        .hover(move |s| {
            s.bg(theme.list_hover)
                .border_color(theme.primary.opacity(0.6))
                .text_color(theme.primary)
        })
        .child(icon.small())
}

/// One action card on the welcome screen: an ambient dither field in a
/// neutral ink, with an accent bloom that follows the pointer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn welcome_card(
    id: &'static str,
    icon: Icon,
    title: &'static str,
    detail: SharedString,
    shortcut: Option<&'static str>,
    field: DitherField,
    cx: &App,
) -> Stateful<Div> {
    let colors = theme::liquid_colors(cx);
    let accent = cx.theme().primary;
    let border = cx.theme().border;
    div()
        .id(id)
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(156.))
        .rounded(px(12.))
        .overflow_hidden()
        .border_1()
        .border_color(border)
        .bg(colors.panel)
        .cursor_pointer()
        .hover(move |style| style.border_color(accent.opacity(0.55)))
        .on_mouse_move(|_, window, _| window.refresh())
        .child(
            Dither::new(field)
                .color(colors.ink)
                .grid(4.0, 3.0)
                .animated()
                .corner_radius(12.0)
                .layer(),
        )
        .child(
            Dither::new(DitherField::Pointer {
                radius: 0.6,
                strength: 1.0,
            })
            .color(accent.opacity(0.8))
            .grid(4.0, 3.0)
            .hover_only()
            .corner_radius(12.0)
            .layer(),
        )
        .child(
            v_flex()
                .relative()
                .size_full()
                .p(px(12.))
                .justify_between()
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_start()
                        .child(
                            div()
                                .size(px(32.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(8.))
                                .border_1()
                                .border_color(border)
                                .bg(colors.elevated)
                                .child(icon.small().text_color(cx.theme().foreground)),
                        )
                        .when_some(shortcut, |row, shortcut| {
                            row.child(crate::ui::kit::keycap(shortcut, cx.theme().kodem))
                        }),
                )
                .child(
                    v_flex()
                        .w_full()
                        .px(px(10.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(border)
                        .bg(colors.elevated.opacity(0.92))
                        .child(
                            div()
                                .text_size(text_px(12.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(text_px(11.5))
                                .text_color(colors.text_tertiary)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(detail),
                        ),
                ),
        )
}

/// A compact popover header with a dithered glow behind the title.
fn popover_header(icon: Icon, title: &'static str, cx: &App) -> Div {
    div()
        .relative()
        .w_full()
        .overflow_hidden()
        .rounded_t(px(6.))
        .child(
            Dither::new(DitherField::Fade {
                from: Edge::Right,
                start: 0.75,
                end: 0.0,
            })
            .color(cx.theme().primary.opacity(0.35))
            .grid(3.0, 2.0)
            .layer(),
        )
        .child(
            h_flex()
                .relative()
                .w_full()
                .gap_2()
                .px_2()
                .py_2()
                .items_center()
                .child(icon.small().text_color(cx.theme().primary))
                .child(
                    div()
                        .text_size(text_px(12.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(title),
                ),
        )
}

fn commit_search_content(search: Entity<InputState>, cx: &App) -> Div {
    let colors = theme::liquid_colors(cx);
    v_flex()
        .w(px(320.))
        .gap(px(2.))
        .child(popover_header(
            Icon::new(IconName::Search),
            "Search Commits",
            cx,
        ))
        .child(notification_divider(cx))
        .child(
            v_flex()
                .gap(px(6.))
                .px_2()
                .py_2()
                .child(
                    text_field(&search)
                        .small()
                        .cleanable()
                        .prefix(Icon::new(IconName::Search).xsmall()),
                )
                .child(
                    div()
                        .font_family(mono_font())
                        .text_size(text_px(11.))
                        .text_color(colors.text_tertiary)
                        .child("message · author · hash"),
                ),
        )
}

fn notification_center_content(
    notifications: Vec<RepositoryNotification>,
    active_operation: Option<RepositoryOperation>,
    branch: String,
    state: Entity<RepositoryState>,
    cx: &App,
) -> Div {
    let has_items = active_operation.is_some() || !notifications.is_empty();
    let mut rows = v_flex()
        .w_full()
        .max_h(px(360.))
        .id("notification-rows")
        .overflow_y_scroll();

    if let Some(operation) = active_operation {
        let title = if operation.label.ends_with("...") {
            operation.label
        } else {
            format!("{}...", operation.label)
        };
        rows = rows.child(notification_row(
            title,
            operation.route.unwrap_or(branch),
            None,
            None,
            cx,
        ));
    }

    rows = rows.children(notifications.into_iter().map(|notification| {
        let status = Some(notification.status);
        let time = notification.created_at.format("%H:%M").to_string();
        notification_row(
            notification.title,
            notification.body,
            status,
            Some(time),
            cx,
        )
    }));

    let mut content = v_flex()
        .w(px(332.))
        .gap(px(2.))
        .child(popover_header(
            Icon::new(IconName::Bell),
            "Notifications",
            cx,
        ))
        .child(notification_divider(cx));

    if has_items {
        content = content.child(rows);
    } else {
        content = content.child(
            v_flex()
                .px_2()
                .py_3()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(text_px(12.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child("No recent notifications"),
                )
                .child(
                    div()
                        .text_size(text_px(11.5))
                        .text_color(cx.theme().muted_foreground)
                        .child("Git task updates will appear here."),
                ),
        );
    }

    if !state.read(cx).notifications.is_empty() {
        content = content.child(notification_divider(cx)).child(
            button("clear-notifications")
                .ghost()
                .danger()
                .small()
                .w_full()
                .justify_start()
                .icon(icons::trash())
                .label("Clear")
                .on_click(move |_, _, cx| {
                    state.update(cx, |state, cx| state.clear_notifications(cx));
                }),
        );
    }

    content
}

fn notification_row(
    title: String,
    body: String,
    status: Option<RepositoryNotificationStatus>,
    time: Option<String>,
    cx: &App,
) -> Div {
    let colors = theme::liquid_colors(cx);
    let tone = match status {
        Some(RepositoryNotificationStatus::Success) => colors.success,
        Some(RepositoryNotificationStatus::Failure) => colors.danger,
        None => cx.theme().primary,
    };
    let icon = match status {
        Some(RepositoryNotificationStatus::Success) => Icon::new(IconName::Check),
        Some(RepositoryNotificationStatus::Failure) => Icon::new(IconName::TriangleAlert),
        None => icons::refresh(),
    };
    let list_hover = cx.theme().list_hover;
    h_flex()
        .w_full()
        .gap_2()
        .px_2()
        .py(px(6.))
        .items_center()
        .rounded(px(6.))
        .hover(move |style| style.bg(list_hover))
        .child(dither_icon_tile(icon, tone, 22.0, status.is_none()))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(text_px(12.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(title),
                        )
                        .when_some(time, |row, time| {
                            row.child(
                                div()
                                    .font_family(mono_font())
                                    .text_size(text_px(10.5))
                                    .text_color(colors.text_tertiary)
                                    .child(time),
                            )
                        }),
                )
                .when(!body.is_empty(), |row| {
                    row.child(
                        div()
                            .font_family(mono_font())
                            .text_size(text_px(11.))
                            .text_color(cx.theme().muted_foreground)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(body),
                    )
                }),
        )
}

/// The toast shown when a Git operation finishes: a dithered status tile, the title and the
/// mono detail line on a floating card.
fn operation_toast(
    status: RepositoryNotificationStatus,
    title: SharedString,
    body: SharedString,
    cx: &App,
) -> AnyElement {
    let colors = theme::liquid_colors(cx);
    let kodem = cx.theme().kodem;
    let (tone, icon) = match status {
        RepositoryNotificationStatus::Success => (colors.success, Icon::new(IconName::Check)),
        RepositoryNotificationStatus::Failure => {
            (colors.danger, Icon::new(IconName::TriangleAlert))
        }
    };
    div()
        .relative()
        .w(px(TOAST_WIDTH))
        .child(crate::ui::dither::shadow(
            kodem,
            crate::theme::RADIUS_SURFACE,
            5.,
        ))
        .child(
            h_flex()
                .relative()
                .w_full()
                .gap(px(12.))
                .items_start()
                .p(px(12.))
                .rounded(px(crate::theme::RADIUS_SURFACE))
                .bg(kodem.raised)
                .border_1()
                .border_color(tone.opacity(0.45))
                .shadow(kodem.floating())
                .child(dither_icon_tile(icon, tone, 28.0, false))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(text_px(12.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(kodem.text)
                                .child(title),
                        )
                        .when(!body.is_empty(), |content| {
                            content.child(
                                div()
                                    .font_family(mono_font())
                                    .text_size(text_px(11.))
                                    .text_color(colors.text_secondary)
                                    .child(body),
                            )
                        }),
                ),
        )
        .into_any_element()
}

fn notification_divider(cx: &App) -> Div {
    div().w_full().h(gpui::px(1.)).my_1().bg(cx.theme().border)
}

/// The floating card shown while a Git operation runs: a liquid level in
/// a dithered tank, the operation title, its route, and a flowing strip.
fn operation_status_badge(operation: RepositoryOperation, cx: &App) -> Div {
    let title = if operation.label.ends_with("...") {
        operation.label
    } else {
        format!("{}...", operation.label)
    };
    let accent = cx.theme().primary;
    let colors = theme::liquid_colors(cx);
    let kodem = cx.theme().kodem;

    h_flex()
        .w(px(340.))
        .gap(px(12.))
        .items_center()
        .p(px(12.))
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(kodem.edge)
        .bg(cx.theme().popover)
        .shadow(kodem.floating())
        .child(
            div()
                .relative()
                .size(px(46.))
                .flex_shrink_0()
                .rounded(px(10.))
                .overflow_hidden()
                .border_1()
                .border_color(accent.opacity(0.35))
                .bg(accent.opacity(0.06))
                .child(
                    Dither::new(DitherField::Wave {
                        level: 0.55,
                        amplitude: 0.09,
                        speed: 3.2,
                    })
                    .color(accent)
                    .grid(3.0, 2.0)
                    .toned()
                    .animated()
                    .corner_radius(10.0)
                    .layer(),
                ),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(4.))
                .child(
                    div()
                        .min_w_0()
                        .text_size(text_px(12.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(title),
                )
                .when_some(operation.route, |this, route| {
                    this.child(
                        div()
                            .w_full()
                            .font_family(mono_font())
                            .text_size(text_px(11.))
                            .text_color(colors.text_tertiary)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(route),
                    )
                })
                .child(
                    div().relative().w_full().h(px(6.)).mt(px(2.)).child(
                        Dither::new(DitherField::Flow {
                            scale: 0.18,
                            speed: 8.0,
                            bias: 0.05,
                        })
                        .color(accent)
                        .grid(3.0, 2.0)
                        .animated()
                        .layer(),
                    ),
                ),
        )
}

impl Render for GitWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_onboarding(window, cx);
        if let Some(onboarding) = self.onboarding.clone() {
            return div()
                .relative()
                .size_full()
                .child(onboarding)
                .into_any_element();
        }
        let has_repo = !self.tabs.is_empty();
        let active_operation = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.state.read(cx).active_operation.clone());
        let error = self
            .tabs
            .get(self.active)
            .and_then(|t| t.state.read(cx).last_error.clone());
        let operation_animation_sequence = self.operation_animation_sequence;
        let content_animation_sequence = self.content_animation_sequence;
        let busy = self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.state.read(cx).busy.is_some());

        let main = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .when(!has_repo, |this| this.child(self.render_empty(cx)))
            .when(has_repo, |this| {
                let content = {
                    let tab = &self.tabs[self.active];
                    let content: AnyElement = match tab.view {
                        // Cached views render again only when they are notified, so
                        // workspace animation steps do not render the whole tab.
                        ViewTab::History => {
                            tab.history.clone().cached(full_size()).into_any_element()
                        }
                        ViewTab::Changes => {
                            tab.changes.clone().cached(full_size()).into_any_element()
                        }
                        ViewTab::PullRequests => {
                            tab.prs.clone().cached(full_size()).into_any_element()
                        }
                    };
                    content
                };

                this.when_some(error, |this, error| {
                    let danger = cx.theme().danger;
                    this.child(
                        h_flex()
                            .relative()
                            .w_full()
                            .gap_2()
                            .pl(px(16.))
                            .pr(px(12.))
                            .py(px(6.))
                            .items_center()
                            .bg(danger.opacity(0.08))
                            .border_b_1()
                            .border_color(danger.opacity(0.25))
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .bottom_0()
                                    .w(px(6.))
                                    .bg(checker(danger.opacity(0.8), 1.0, window)),
                            )
                            .child(
                                Icon::new(IconName::TriangleAlert)
                                    .xsmall()
                                    .text_color(danger),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(mono_font())
                                    .text_size(text_px(11.5))
                                    .text_color(danger)
                                    .child(error),
                            ),
                    )
                })
                .child(self.render_view_tabs(cx))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .child(motion::content_surface(
                            "workspace-content-motion",
                            content_animation_sequence,
                            content,
                            cx,
                        )),
                )
            });

        div()
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .child(main)
            .when(busy, |this| {
                // A flowing dither band along the top edge shows that a Git task is running.
                this.child(
                    div().absolute().left_0().right_0().top_0().h(px(3.)).child(
                        Dither::new(DitherField::Flow {
                            scale: 0.08,
                            speed: 14.0,
                            bias: 0.08,
                        })
                        .color(cx.theme().primary)
                        .grid(2.0, 2.0)
                        .animated()
                        .layer(),
                    ),
                )
            })
            .when_some(active_operation, |this, operation| {
                this.child(div().absolute().right(px(16.)).bottom(px(16.)).child(
                    motion::floating_surface(
                        "operation-status-motion",
                        operation_animation_sequence,
                        operation_status_badge(operation, cx),
                        cx,
                    ),
                ))
            })
            .into_any_element()
    }
}
