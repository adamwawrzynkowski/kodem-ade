//! Native workspace state and actions. All blocking work goes through the host bridge.
mod accounts;
mod activity;
mod backdrop;
mod browser;
mod censor;
mod change_tree;
mod chat;
mod chrome;
mod code_review;
mod composition;
mod controls;
mod conversation;
mod diff_refresh;
mod diff_view;
mod dither;
mod effort_slider;
mod fx;
mod git;
mod history;
mod image_preview;
mod kit;
mod menus;
mod message_content;
mod model_picker;
mod motion;
mod notices;
mod onboarding;
mod palette;
mod panels;
mod projects;
mod questions;
mod review;
mod settings;
mod sidebar;
mod slash;
mod subagents;
mod tool_calls;
mod updates;
mod usage;
mod workspaces;
use crate::icons::icon;
use crate::{
    bridge,
    components::{
        Button,
        text_input::{CompletionInput, ReferenceInput, SubmitInput, TextInput},
    },
    theme::{Theme, code_px, px, text_px},
};
pub use chrome::Stage;
use conversation::Conversation;
use gpui::{prelude::*, *};
use kodem_core::{self as core, *};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

actions!(
    kodem,
    [
        OpenProject,
        NewTask,
        SendPrompt,
        ToggleReview,
        ToggleSidebar,
        ShowSettings,
        Search,
        Cancel,
        Quit,
        Latest,
        CommandPalette,
        NextTask,
        PreviousTask,
        NextAttention,
        ChooseModel,
        ChooseEffort,
        ChooseAccess,
        CycleAccess,
        ToggleTerminal,
        GoToTask1,
        GoToTask2,
        GoToTask3,
        GoToTask4,
        GoToTask5,
        GoToTask6,
        GoToTask7,
        GoToTask8,
        GoToTask9,
        ShowCoding,
        ShowChat,
        ShowReview,
        ShowGit,
        CloneRepository
    ]
);
#[derive(Clone, PartialEq)]
enum Panel {
    None,
    CustomizeProject,
    RemoveProject,
    NoRepository,
    ChatProjects,
    Settings,
    Setup,
    Resources,
    Files,
    Check,
    Rename,
    Models,
    Reasoning,
    Access,
    Projects,
    TaskMenu,
    ThreadMenu,
    DeleteTask,
    Sort,
    Comparison,
    Usage,
    Palette,
    Mode,
    WhatsNew,
    Notifications,
}
struct ThreadMenu {
    task_id: String,
    position: Point<Pixels>,
}
struct PendingSubmission {
    task_id: String,
    id: String,
    text: String,
    revision: u64,
    origin: Option<Point<Pixels>>,
    queued: bool,
}
pub struct Shell {
    tx: mpsc::Sender<bridge::Command>,
    update_state: crate::updater::State,
    update_tx: mpsc::Sender<crate::updater::State>,
    last_update_check: std::time::Instant,
    focus: FocusHandle,
    workspace: Option<Workspace>,
    /// The open workspace. An open task is always of this kind.
    mode: TaskKind,
    /// The Git workspace, created when it opens the first time.
    git: Option<Entity<git::GitWorkspace>>,
    /// The Git workspace covers the workspace of `mode`.
    git_open: bool,
    /// Repository folders that the Git workspace asked the host to list as projects.
    git_registrations: HashSet<PathBuf>,
    /// The pages of the Git section of preferences.
    git_settings: Option<Entity<git::settings_view::GitSettingsView>>,
    /// The task that was open in each workspace when the window left it.
    mode_tasks: HashMap<TaskKind, String>,
    /// The start-screen draft of each workspace that is not open.
    mode_drafts: HashMap<TaskKind, workspaces::StartDraft>,
    /// First message of a task that is being created with a prompt the composer does not hold.
    launch_draft: Option<String>,
    review: code_review::ReviewSetup,
    review_filter: Entity<TextInput>,
    view: Option<TaskView>,
    agent_popup: Option<subagents::AgentPopup>,
    selected: Option<String>,
    input: Entity<TextInput>,
    aux: Entity<TextInput>,
    search: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
    settings: Settings,
    theme: Theme,
    /// The interface scale under a drag of its slider. It applies when the drag ends.
    pending_scale: Option<f32>,
    /// Families of the fonts on this Mac, for the font pickers. Empty until they are listed.
    installed_fonts: HashSet<String>,
    panel: Panel,
    /// Bounds of the control that opened the current menu. The menu opens above or below it.
    menu_anchor: Option<Bounds<Pixels>>,
    thread_menu: Option<ThreadMenu>,
    deleting_task: Option<String>,
    error: Option<String>,
    notices: notices::Notices,
    host_connected: bool,
    new_worktree_override: Option<bool>,
    new_preferences: Option<AgentPreferences>,
    /// The approval rule that returns when a task or the new-task draft leaves plan mode,
    /// keyed by task id; the draft uses an empty key.
    approval_memory: HashMap<String, String>,
    settings_provider: Provider,
    // Project chosen for the next task, independent of the currently viewed thread.
    selected_project: Option<String>,
    editing_project: Option<Project>,
    // Folder without Git that waits for the user to choose how it opens.
    folder_without_git: Option<PathBuf>,
    // Narrows the built-in icons in the project editor by name.
    icon_filter: Entity<TextInput>,
    greeting_name: String,
    show_review: bool,
    show_terminal: bool,
    terminal: Option<TerminalView>,
    terminal_focus: FocusHandle,
    diff: Option<DiffSnapshot>,
    new_diff: Option<DiffSnapshot>,
    diff_refresh: diff_refresh::DiffRefresh,
    changes_refresh: diff_refresh::DiffRefresh,
    task_changes: Option<ChangeSummary>,
    diff_subscription: Option<Subscription>,
    file: usize,
    reviewing_file: bool,
    historical_diff: bool,
    search_visible: bool,
    comparison: Comparison,
    split: bool,
    chat_scroll: ListState,
    /// The rows of `chat_scroll`: a long reply takes several, so the list lays out only its
    /// part in view.
    chat_rows: conversation::ChatRows,
    follow: bool,
    /// Sequence number of the sent message whose history tick is under the pointer.
    history_hover: Option<u64>,
    folded: HashSet<String>,
    /// Long messages of the user that show their text instead of only their card.
    unfolded_texts: HashSet<String>,
    /// Tool runs and thoughts the user opened or folded by hand; the others follow their work.
    tray_choices: HashMap<String, bool>,
    collapsed_changes: HashSet<String>,
    change_tree: Option<change_tree::ChangeTree>,
    text_views: HashMap<String, (Entity<TextInput>, Subscription)>,
    chat_selection: Entity<crate::components::text_document::TextDocument>,
    selection_inputs: Option<Vec<Entity<TextInput>>>,
    resources: Vec<core::Resource>,
    slash: slash::SlashPicker,
    send_arrivals: HashMap<String, motion::Arrival>,
    context_arrivals: HashMap<String, Option<std::time::Instant>>,
    context_task: Option<String>,
    files: Vec<Vec<u8>>,
    file_text: Option<(String, String)>,
    dragging_sidebar: bool,
    dragging_review: bool,
    /// A press on a title bar that the app draws itself, which moves the window once the
    /// pointer moves. A press that ends in place stays a click.
    title_press: bool,
    /// The versions of Git, Git LFS and GitHub CLI that the host read last. Empty until it
    /// answers.
    tools: Vec<ToolVersion>,
    /// Tools whose installer runs in a terminal window and that were not found since.
    tool_installers: Vec<Tool>,
    /// The release notes wait for the first workspace, because this version runs for the first
    /// time after an update.
    whats_new_due: bool,
    window_width: f32,
    window_placement: WindowBounds,
    window_save_task: Option<gpui::Task<()>>,
    show_archived: bool,
    poll_pending: bool,
    draft_dirty: bool,
    question_inputs: HashMap<(String, String), questions::AnswerDraft>,
    answering: HashSet<String>,
    submitted_draft: Option<PendingSubmission>,
    draft_revision: u64,
    observed_draft: String,
    importing_tasks: HashMap<String, usize>,
    pending_attachments: Vec<AttachmentSource>,
    clipboard_images: HashMap<std::path::PathBuf, tempfile::TempPath>,
    pasting_images: usize,
    diff_view: Option<Entity<diff_view::DiffView>>,
    sidebar_rows: Vec<crate::thread_list::Row>,
    sidebar_scroll: ListState,
    sidebar_dirty: bool,
    sidebar_project: Option<String>,
    settled_rows: Vec<crate::thread_list::Row>,
    settled_collapsed: bool,
    empty_draft: String,
    creating_task: bool,
    auto_submit_task: Option<String>,
    new_submission_origin: Option<Point<Pixels>>,
    composer_height: f32,
    composer_spring: motion::ComposerSpring,
    reveals: motion::Reveals,
    settings_section: usize,
    effort_slider: Option<(String, Entity<effort_slider::EffortSlider>, Subscription)>,
    image_preview: Option<image_preview::ImagePreview>,
    plan_expanded: bool,
    palette_input: Entity<TextInput>,
    palette: palette::Palette,
    accounts: accounts::AccountsView,
    censor: censor::Censor,
    usage_actions: std::collections::BTreeMap<(Provider, String), usage::UsageActions>,
}
impl Shell {
    /// Creates the workspace and begins asynchronous host synchronization.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.set_reduce_motion(motion::system_reduce_motion());
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let (tx, rx) = bridge::start();
        let (update_tx, update_rx) = mpsc::channel();
        let input = cx.new(|cx| {
            let mut input = TextInput::composer("Describe what you want to build or change…", cx);
            // A long paste shows as a file beside the draft instead of in it.
            input.set_folds_long_text(true);
            input
        });
        let aux = cx.new(|cx| TextInput::new("Type here…", cx));
        let search = cx.new(|cx| TextInput::new("Search tasks…", cx));
        let palette_input =
            cx.new(|cx| TextInput::composer("Type a command or search threads…", cx));
        let review_filter = cx.new(|cx| TextInput::new("Filter…", cx));
        let icon_filter = cx.new(|cx| TextInput::new("Search icons…", cx));
        let subscriptions = vec![
            cx.observe_window_bounds(window, |s, window, cx| {
                let placement = window.window_bounds();
                if s.window_placement != placement {
                    s.window_placement = placement;
                    s.window_save_task = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(350))
                            .await;
                        let _ = this.update(cx, |s, _| s.save_window_placement());
                    }));
                }
            }),
            cx.on_release(|s, _| s.save_window_placement()),
            cx.on_app_quit(|s, cx| {
                s.save_window_placement();
                if s.workspace.is_some() {
                    s.save_settings();
                }
                let (done, receiver) = std::sync::mpsc::channel();
                let _ = s.tx.send(bridge::Command::Flush(done));
                let drain = cx.background_executor().spawn(async move {
                    let _ = receiver.recv_timeout(Duration::from_secs(2));
                });
                async move {
                    drain.await;
                }
            }),
            cx.on_focus_lost(window, |s, window, cx| window.focus(&s.focus, cx)),
            cx.subscribe(&input, |s, _, event: &SubmitInput, cx| {
                s.submit_with(event.alternate, cx)
            }),
            cx.subscribe_in(
                &palette_input,
                window,
                |s, _, key: &CompletionInput, window, cx| s.palette_key(key, window, cx),
            ),
            cx.observe(&palette_input, |_, _, cx| cx.notify()),
            cx.subscribe(
                &input,
                |s, _, key: &crate::components::text_input::CompletionInput, cx| {
                    s.slash_key(key, cx)
                },
            ),
            cx.subscribe(
                &input,
                |s, _, pasted: &crate::clipboard::PastedAttachments, cx| {
                    s.paste_attachments(pasted, cx);
                },
            ),
            cx.observe(&input, |s, input, cx| {
                s.sync_slash(cx);
                let text = input.read(cx).text();
                if text != s.observed_draft {
                    s.draft_revision += 1;
                    s.observed_draft = text;
                    s.draft_dirty = true;
                    s.nudge_composer(-0.35, cx);
                }
                cx.notify();
            }),
            cx.observe(&search, |s, _, cx| {
                s.sidebar_dirty = true;
                cx.notify();
            }),
            cx.observe(&aux, |_, _, cx| cx.notify()),
            cx.observe(&review_filter, |_, _, cx| cx.notify()),
            cx.observe(&icon_filter, |_, _, cx| cx.notify()),
        ];
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let mut updates = Vec::new();
                while let Ok(reply) = rx.try_recv() {
                    updates.push(reply);
                }
                if this
                    .update(cx, |s, cx| {
                        while let Ok(state) = update_rx.try_recv() {
                            s.update_state = state;
                            cx.notify();
                        }
                        if s.last_update_check.elapsed() >= Duration::from_secs(30 * 60)
                            && matches!(
                                s.update_state,
                                crate::updater::State::Idle | crate::updater::State::Failed(_)
                            )
                        {
                            s.check_for_updates(cx);
                        }
                        for reply in updates {
                            s.apply(reply, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(600))
                    .await;
                if this
                    .update(cx, |s, cx| {
                        if matches!(s.update_state, crate::updater::State::Installing) {
                            return;
                        }
                        if s.draft_dirty && s.view.is_some() && s.submitted_draft.is_none() {
                            if let Some(task_id) = &s.selected {
                                s.send(core::Request::EditTask {
                                    task_id: task_id.clone(),
                                    edit: TaskEdit::Draft(s.input.read(cx).text()),
                                });
                            }
                            s.draft_dirty = false;
                        }
                        if !s.poll_pending {
                            s.poll_pending = true;
                            let _ = s.tx.send(bridge::Command::Poll(
                                s.selected.clone(),
                                s.show_terminal && s.has_checkout(),
                                s.agent_popup.as_ref().map(|p| p.thread_id.clone()),
                                s.view
                                    .as_ref()
                                    .is_some_and(|view| s.selected.as_ref() == Some(&view.task.id)),
                            ));
                        }
                        if s.host_connected && s.has_checkout() && s.changes_refresh.due() {
                            s.refresh_task_changes();
                        }
                        if s.host_connected
                            && s.has_checkout()
                            && (s.show_review || s.reviewing_file)
                            && !s.historical_diff
                            && s.diff_refresh.due()
                        {
                            s.refresh_diff();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        // Listing every font of the system takes a moment, so it runs off the main thread. The
        // font pickers show the bundled fonts until it ends.
        let text_system = cx.text_system().clone();
        cx.spawn(async move |this, cx| {
            let names = cx
                .background_executor()
                .spawn(async move { text_system.all_font_names() })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.installed_fonts = names.into_iter().collect();
                cx.notify();
            });
        })
        .detach();
        let mut shell = Self {
            tx,
            update_state: crate::updater::State::Idle,
            update_tx,
            last_update_check: std::time::Instant::now(),
            focus,
            workspace: None,
            mode: TaskKind::default(),
            git: None,
            git_open: false,
            git_registrations: HashSet::new(),
            git_settings: Some(cx.new(|cx| git::settings_view::GitSettingsView::new(window, cx))),
            mode_tasks: HashMap::new(),
            mode_drafts: HashMap::new(),
            launch_draft: None,
            review: Default::default(),
            review_filter,
            view: None,
            agent_popup: None,
            selected: None,
            input,
            aux,
            search,
            _subscriptions: subscriptions,
            settings: Settings::default(),
            theme: Theme::default(),
            pending_scale: None,
            installed_fonts: HashSet::new(),
            panel: Panel::None,
            menu_anchor: None,
            usage_actions: std::collections::BTreeMap::new(),
            thread_menu: None,
            deleting_task: None,
            error: None,
            notices: notices::Notices::default(),
            host_connected: false,
            new_worktree_override: None,
            new_preferences: None,
            approval_memory: HashMap::new(),
            settings_provider: Provider::default(),
            selected_project: None,
            editing_project: None,
            folder_without_git: None,
            icon_filter,
            greeting_name: user_first_name(),
            show_review: true,
            show_terminal: false,
            terminal: None,
            terminal_focus: cx.focus_handle(),
            diff: None,
            new_diff: None,
            diff_refresh: diff_refresh::DiffRefresh::default(),
            changes_refresh: Default::default(),
            task_changes: None,
            diff_subscription: None,
            file: 0,
            reviewing_file: false,
            historical_diff: false,
            search_visible: false,
            comparison: Comparison::WorkingTree,
            split: false,
            chat_scroll: ListState::new(0, ListAlignment::Bottom, px(600.)),
            chat_rows: Default::default(),
            follow: true,
            history_hover: None,
            folded: HashSet::new(),
            unfolded_texts: HashSet::new(),
            tray_choices: HashMap::new(),
            collapsed_changes: HashSet::new(),
            change_tree: None,
            text_views: HashMap::new(),
            chat_selection: cx.new(|_| Default::default()),
            selection_inputs: None,
            resources: Vec::new(),
            slash: Default::default(),
            send_arrivals: HashMap::new(),
            context_arrivals: HashMap::new(),
            context_task: None,
            files: Vec::new(),
            file_text: None,
            dragging_sidebar: false,
            dragging_review: false,
            title_press: false,
            tools: Vec::new(),
            tool_installers: Vec::new(),
            whats_new_due: crate::release_notes::take_unseen(),
            window_width: f32::from(window.viewport_size().width),
            window_placement: window.window_bounds(),
            window_save_task: None,
            show_archived: false,
            poll_pending: false,
            draft_dirty: false,
            question_inputs: HashMap::new(),
            answering: HashSet::new(),
            submitted_draft: None,
            draft_revision: 0,
            observed_draft: String::new(),
            importing_tasks: HashMap::new(),
            pending_attachments: Vec::new(),
            clipboard_images: HashMap::new(),
            pasting_images: 0,
            diff_view: None,
            sidebar_rows: Vec::new(),
            sidebar_scroll: ListState::new(0, ListAlignment::Top, px(100.)),
            sidebar_dirty: true,
            sidebar_project: None,
            settled_rows: Vec::new(),
            settled_collapsed: true,
            empty_draft: String::new(),
            creating_task: false,
            auto_submit_task: None,
            new_submission_origin: None,
            composer_height: 170.,
            composer_spring: motion::ComposerSpring::default(),
            reveals: motion::Reveals::default(),
            settings_section: 0,
            effort_slider: None,
            image_preview: None,
            plan_expanded: false,
            palette_input,
            palette: Default::default(),
            accounts: Default::default(),
            censor: Default::default(),
        };
        shell.check_for_updates(cx);
        shell.send(core::Request::Hello);
        shell
    }
    fn send(&self, request: core::Request) {
        let _ = self.tx.send(bridge::Command::Request(request));
    }
    fn apply(&mut self, reply: bridge::Reply, cx: &mut Context<Self>) {
        if reply.poll_complete {
            self.poll_pending = false;
        }
        if matches!(reply.request, core::Request::TaskChanges { .. }) {
            self.changes_refresh.completed();
        }
        if matches!(reply.request, core::Request::CaptureDiff { .. }) {
            self.diff_refresh.completed();
        }
        if !diff_refresh::accepts_reply(
            &reply.request,
            self.selected.as_deref(),
            &self.comparison,
            self.historical_diff,
        ) {
            return;
        }
        if reply.connected && !self.host_connected {
            self.error = None;
        }
        self.host_connected = reply.connected;
        if matches!(
            reply.request,
            core::Request::RefreshUsage { .. } | core::Request::ConsumeUsageReset { .. }
        ) {
            let (provider, account) = match reply.request {
                core::Request::RefreshUsage { provider, account }
                | core::Request::ConsumeUsageReset {
                    provider, account, ..
                } => (provider, account),
                _ => unreachable!(),
            };
            let account = account.unwrap_or_else(|| DEFAULT_ACCOUNT.into());
            self.apply_usage_reply(provider, account, reply.result, cx);
            return;
        }
        if let core::Request::ImportAttachments { task_id, .. } = &reply.request
            && let Some(count) = self.importing_tasks.get_mut(task_id)
        {
            *count = count.saturating_sub(1);
        }
        let response_task = match &reply.request {
            core::Request::Task { task_id, .. }
            | core::Request::Watch { task_id, .. }
            | core::Request::ReadFile { task_id, .. }
            | core::Request::Files { task_id }
            | core::Request::Terminal { task_id }
            | core::Request::TaskChanges { task_id } => Some(task_id),
            _ => None,
        };
        if response_task.is_some_and(|id| self.selected.as_ref() != Some(id)) {
            return;
        }
        // After its first answer the watcher sends only the rows that changed. They are not a
        // conversation by themselves, so they are dropped until the window holds the first
        // page of the task, which the poll requests while it is missing.
        if let core::Request::Watch {
            task_id,
            after: Some(_),
        } = &reply.request
            && self
                .view
                .as_ref()
                .is_none_or(|view| &view.task.id != task_id)
        {
            return;
        }

        if let core::Request::Resources {
            provider,
            task_id,
            project_id,
            ..
        } = &reply.request
        {
            let current_project = self.selected_project.clone().or_else(|| {
                self.workspace
                    .as_ref()?
                    .projects
                    .first()
                    .map(|project| project.id.clone())
            });
            if *provider != self.composer_preferences().provider
                || task_id != &self.selected
                || (task_id.is_none() && project_id != &current_project)
            {
                return;
            }
        }
        let response = match reply.result {
            Ok(r) => r,
            Err(e) => {
                if let core::Request::SubagentThread {
                    task_id, thread_id, ..
                } = &reply.request
                {
                    if self.selected.as_ref() == Some(task_id)
                        && let Some(popup) = self
                            .agent_popup
                            .as_mut()
                            .filter(|p| &p.thread_id == thread_id)
                    {
                        popup.error = Some(e);
                        cx.notify();
                    }
                    return;
                }
                if matches!(reply.request, core::Request::ReviewTargets { .. }) {
                    // The picker shows why its lists are empty; no notice covers the window.
                    self.review_targets_failed(&reply.request, e);
                    cx.notify();
                    return;
                }
                if matches!(reply.request, core::Request::ComposerCatalog { .. }) {
                    self.receive_slash_catalog(
                        &reply.request,
                        ComposerCatalog {
                            errors: vec![e],
                            ..Default::default()
                        },
                    );
                    cx.notify();
                    return;
                }
                if matches!(
                    reply.request,
                    core::Request::SetAgentPreferences(_) | core::Request::SetAccessChoice(_)
                ) {
                    self.new_preferences = None;
                    self.send(core::Request::Hello);
                }
                self.answer_failed(&reply.request);
                if matches!(reply.request, core::Request::DeleteTask { .. }) {
                    self.deleting_task = None;
                }
                if matches!(reply.request, core::Request::Files { .. }) {
                    self.files_loaded();
                }
                if matches!(reply.request, core::Request::TaskChanges { .. }) {
                    // Retain the last consistent totals while files are being written.
                    cx.notify();
                    return;
                }
                if let core::Request::Submit {
                    task_id,
                    submission_id,
                    ..
                } = &reply.request
                    && self
                        .submitted_draft
                        .as_ref()
                        .is_some_and(|p| &p.task_id == task_id && &p.id == submission_id)
                {
                    self.submitted_draft = None;
                }
                if matches!(reply.request, core::Request::AddFeedbackRows { .. })
                    && let Some(viewer) = &self.diff_view
                {
                    viewer.update(cx, |v, cx| v.context_failed(cx));
                }
                if matches!(
                    reply.request,
                    core::Request::CreateTask { .. }
                        | core::Request::CreateChat { .. }
                        | core::Request::CreateReview { .. }
                ) {
                    self.creating_task = false;
                    self.launch_draft = None;
                    self.new_submission_origin = None;
                }
                if matches!(reply.request, core::Request::ImportAttachments { .. }) {
                    self.auto_submit_task = None;
                    self.new_submission_origin = None;
                }
                self.report_error(e, cx);
                return;
            }
        };
        match response {
            response @ (Response::Usage { .. } | Response::UsageReset { .. }) => {
                let (provider, account) = match &response {
                    Response::Usage {
                        provider, account, ..
                    }
                    | Response::UsageReset {
                        provider, account, ..
                    } => (*provider, account.clone()),
                    _ => unreachable!(),
                };
                self.apply_usage_reply(provider, account, Ok(response), cx);
            }
            Response::Workspace(workspace) => {
                self.sidebar_dirty = true;
                if self.workspace.is_none() {
                    // A new task starts in the project that was chosen last.
                    self.selected_project = workspace.settings.selected_project.clone();
                }
                if !workspace
                    .projects
                    .iter()
                    .any(|p| self.selected_project.as_ref() == Some(&p.id))
                {
                    self.selected_project = workspace.projects.first().map(|p| p.id.clone());
                }
                if self
                    .sidebar_project
                    .as_ref()
                    .is_some_and(|id| !workspace.projects.iter().any(|p| &p.id == id))
                {
                    self.sidebar_project = None;
                }
                cx.set_reduce_motion(motion::system_reduce_motion());
                if self.workspace.is_none() {
                    self.settings = workspace.settings.clone();
                    // Results that finished before unread tracking existed count as read.
                    if self.settings.viewed.is_empty() {
                        let seen = now();
                        self.settings.viewed = workspace
                            .tasks
                            .iter()
                            .filter(|task| !task.state.active())
                            .map(|task| (task.id.clone(), seen))
                            .collect();
                    }
                    self.show_review = self.settings.show_review;
                    self.show_terminal = self.settings.show_terminal;
                    self.split = self.settings.split_diff;
                    self.mode = self.settings.mode;
                    self.git_open = self.settings.git_workspace;
                    if self.mode == TaskKind::Review {
                        self.comparison = Comparison::Baseline;
                    }
                    self.sync_placeholder(cx);
                    self.theme = Theme::with_accent(self.settings.theme, self.settings.accent);
                    let selected = self
                        .settings
                        .selected_task
                        .clone()
                        .filter(|id| workspace.tasks.iter().any(|t| &t.id == id));
                    self.workspace = Some(workspace);
                    self.sync_git_projects(cx);
                    if let Some(id) = selected {
                        self.select_task(id, cx)
                    }
                    if !self.settings.onboarded {
                        // The setup opens by itself only once, and it connects every provider.
                        self.settings.onboarded = true;
                        self.save_settings();
                        self.open_setup();
                    } else if self.selected.is_none()
                        && self.workspace.as_ref().is_some_and(|w| {
                            w.runtimes
                                .get(&self.composer_preferences().provider)
                                .is_none_or(|r| r.models.is_empty())
                        })
                    {
                        self.connect_runtime();
                    }
                    if std::mem::take(&mut self.whats_new_due) && self.panel == Panel::None {
                        self.panel = Panel::WhatsNew;
                    }
                    cx.notify();
                    return;
                }
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|id| !workspace.tasks.iter().any(|task| &task.id == id))
                {
                    self.clear_selected_task(cx);
                }
                self.celebrate_transitions(&workspace, cx);
                self.workspace = Some(workspace);
                self.sync_git_projects(cx);
            }
            Response::Account(account) => self.account_added(&account, cx),
            Response::Project(project)
                if self
                    .git_registrations
                    .remove(&workspaces::plain_path(&project.path)) =>
            {
                // The Git workspace listed one of its repositories as a project; the window
                // stays where it is.
                self.send(core::Request::Hello);
            }
            Response::Project(project) => {
                // A chat has no project, so a repository opens in Agentic Coding.
                if self.mode == TaskKind::Chat {
                    self.enter_mode(TaskKind::Coding, cx);
                } else if self.selected.is_some() {
                    self.open_empty_chat(cx);
                }
                self.selected_project = Some(project.id);
                self.save_settings();
                self.folder_without_git = None;
                self.panel = Panel::None;
                self.send(core::Request::Hello);
            }
            Response::NoRepository(path) => {
                self.error = None;
                self.folder_without_git = Some(path);
                self.panel = Panel::NoRepository;
            }
            Response::Created(task) => {
                self.new_worktree_override = None;
                self.new_preferences = None;
                let submit = std::mem::take(&mut self.creating_task);
                // A task made for another workspace opens there, such as the task that fixes
                // the findings of a review.
                if task.kind != self.mode {
                    self.enter_mode(task.kind, cx);
                }
                if task.kind.has_project() {
                    self.selected_project = Some(task.project_id.clone());
                }
                if task.kind == TaskKind::Review {
                    self.review.pick = None;
                }
                // The first message is the start screen's draft, unless the task was made with
                // a prompt of its own. Then the start screen keeps its draft and attachments.
                let launched = self.launch_draft.take();
                let from_composer = launched.is_none();
                let draft = launched.unwrap_or_else(|| std::mem::take(&mut self.empty_draft));
                // The new task leaves the composer as a chip and lands in the thread feed.
                let label = draft
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("New task")
                    .to_owned();
                fx::fly_between(
                    chat::COMPOSER_ANCHOR,
                    sidebar::FEED_ANCHOR,
                    label,
                    self.theme.accent,
                    Duration::ZERO,
                    cx,
                );
                if !draft.is_empty() {
                    self.send(core::Request::EditTask {
                        task_id: task.id.clone(),
                        edit: TaskEdit::Draft(draft),
                    });
                }
                if from_composer && !self.pending_attachments.is_empty() {
                    let paths = std::mem::take(&mut self.pending_attachments);
                    self.import_attachments(task.id.clone(), paths);
                }
                if submit {
                    self.auto_submit_task = Some(task.id.clone());
                }
                self.input
                    .update(cx, |input, cx| input.set_text(String::new(), cx));
                self.select_task(task.id, cx);
                self.panel = Panel::None;
                self.send(core::Request::Hello);
            }
            Response::ReviewTargets(targets) => self.receive_review_targets(targets),
            Response::SubagentThread(view) => {
                self.update_agent_popup(
                    view,
                    matches!(
                        reply.request,
                        core::Request::SubagentThread {
                            before: Some(_),
                            ..
                        }
                    ),
                    cx,
                );
            }
            Response::Task(mut view) => {
                if self.selected.as_deref() != Some(&view.task.id) {
                    return;
                }
                if matches!(reply.request, core::Request::RestoreOutgoing { .. }) {
                    let current = self.input.read(cx).text();
                    let restored = if current.is_empty() || current == view.task.draft {
                        view.task.draft.clone()
                    } else {
                        format!("{}\n\n{}", view.task.draft, current)
                    };
                    self.input.update(cx, |i, cx| i.set_text(restored, cx));
                    self.draft_dirty = true;
                }
                let initial = self.view.as_ref().is_none_or(|v| v.task.id != view.task.id);
                if initial {
                    if self.provider_runtime(view.task.provider).models.is_empty() {
                        self.send(core::Request::ConnectRuntime {
                            provider: view.task.provider,
                            account: view.task.account.clone(),
                        });
                    }
                    self.observed_draft = view.task.draft.clone();
                    self.draft_revision += 1;
                    self.input
                        .update(cx, |input, cx| input.set_text(view.task.draft.clone(), cx));
                    self.draft_dirty = false;
                }
                let older = matches!(
                    reply.request,
                    core::Request::Task {
                        before: Some(_),
                        ..
                    }
                );
                if let Some(current) = &self.view {
                    merge_transcript(current, &mut view, older);
                }
                let changed = self.view.as_ref().is_none_or(|v| {
                    v.messages.len() != view.messages.len()
                        || v.messages.iter().zip(&view.messages).any(|(a, b)| {
                            a.id != b.id
                                || a.text != b.text
                                || a.complete != b.complete
                                || a.file_edits != b.file_edits
                        })
                });
                let rows = conversation::ChatRows::new(&view.messages);
                if changed {
                    self.follow =
                        !older && self.chat_scroll.is_scrolled_to_end().unwrap_or(self.follow);
                    // The row at the top stays at the top: the same part of the same message.
                    let mut anchor = self.chat_scroll.logical_scroll_top();
                    if let Some(current) = &self.view
                        && let Some((index, part)) = self.chat_rows.locate(anchor.item_ix)
                        && let Some(message) = current.messages.get(index)
                        && let Some(index) = view.messages.iter().position(|m| m.id == message.id)
                    {
                        let parts = rows.rows(index..index + 1);
                        anchor.item_ix = (parts.start + part).min(parts.end.saturating_sub(1));
                    }
                    if let Some(current) = &self.view
                        && current.messages.len() <= view.messages.len()
                        && current
                            .messages
                            .iter()
                            .zip(&view.messages)
                            .all(|(a, b)| a.id == b.id)
                    {
                        let previous_count = current.messages.len();
                        let old_total = self.chat_rows.count();
                        if previous_count < view.messages.len() {
                            let added = rows.count() - rows.rows(0..previous_count).end;
                            self.chat_scroll.splice(old_total..old_total, added);
                        }
                        // The last messages first, so that the rows before each keep their
                        // places while a message gains or loses parts.
                        for (index, (a, b)) in current
                            .messages
                            .iter()
                            .zip(&view.messages)
                            .enumerate()
                            .rev()
                        {
                            let old = self.chat_rows.rows(index..index + 1);
                            let new = rows.rows(index..index + 1);
                            let edited = a.text != b.text
                                || a.complete != b.complete
                                || a.file_edits != b.file_edits;
                            if old.len() != new.len() {
                                let kept = old.len().min(new.len());
                                self.chat_scroll
                                    .remeasure_items(old.start..old.start + kept);
                                self.chat_scroll
                                    .splice(old.start + kept..old.end, new.len() - kept);
                            } else if edited && b.kind != MessageKind::Tool {
                                self.chat_scroll.remeasure_items(old);
                            }
                        }
                        if previous_count < view.messages.len() {
                            // A new tool item extends the summary rendered at the group's first
                            // row, and any new item folds the trays that ended the transcript.
                            // The reply before those rows gives up its row of actions when
                            // work follows it.
                            let row =
                                tool_rows_start(&view.messages, previous_count).saturating_sub(1);
                            self.chat_scroll
                                .remeasure_items(rows.rows(row..previous_count + 1));
                        }
                        for (index, (a, b)) in
                            current.messages.iter().zip(&view.messages).enumerate()
                        {
                            if b.kind == MessageKind::Tool
                                && (a.text != b.text
                                    || a.complete != b.complete
                                    || a.file_edits != b.file_edits)
                            {
                                // A tool item can open or fold the parent tray of its
                                // stretch, which sizes every row of the stretch.
                                self.chat_scroll.remeasure_items(rows.rows(
                                    tool_rows_start(&view.messages, index)
                                        ..tool_rows_end(&view.messages, index),
                                ));
                            }
                        }
                    } else {
                        self.chat_scroll.reset(rows.count());
                    }
                    if self.follow {
                        self.chat_scroll.scroll_to_end();
                    } else {
                        self.chat_scroll.scroll_to(anchor);
                    }
                }
                // The trays that end the transcript open when a turn starts and fold when it
                // ends, also when no message changed with it.
                if let Some(current) = &self.view
                    && current.task.id == view.task.id
                    && current.task.state.active() != view.task.state.active()
                {
                    let end = view.messages.len();
                    let row = tool_rows_start(&view.messages, end);
                    if row < end {
                        let at_end = self.chat_scroll.is_scrolled_to_end().unwrap_or(self.follow);
                        self.chat_scroll.remeasure_items(rows.rows(row..end));
                        if at_end {
                            self.chat_scroll.scroll_to_end();
                        }
                    }
                }
                self.chat_rows = rows;
                // A result that arrives while its task is open is read as it arrives.
                if !view.task.state.active()
                    && view
                        .task
                        .is_unread(self.settings.viewed.get(&view.task.id).copied())
                {
                    self.settings.viewed.insert(view.task.id.clone(), now());
                    self.sidebar_dirty = true;
                    self.save_settings();
                }
                let pending: HashSet<_> = view.requests.iter().map(|r| r.id.to_string()).collect();
                self.question_inputs
                    .retain(|(request, _), _| pending.contains(request));
                self.answering.retain(|request| pending.contains(request));
                self.view = Some(view);
                if self.auto_submit_task.as_ref() == self.selected.as_ref()
                    && self.auto_submit_task.is_some()
                    && self.pasting_images == 0
                    && self
                        .selected
                        .as_ref()
                        .is_none_or(|id| self.importing_tasks.get(id).copied().unwrap_or(0) == 0)
                {
                    self.auto_submit_task = None;
                    self.submit_current(cx);
                }
            }
            Response::TaskChanges { task_id, summary } => {
                if self.selected.as_ref() == Some(&task_id) {
                    self.task_changes = Some(summary);
                }
            }
            Response::Diff(diff) => {
                if self.selected.as_deref() != Some(&diff.task_id) {
                    return;
                }
                let historical = matches!(reply.request, core::Request::LoadSnapshot { .. });
                if !historical
                    && self.comparison == Comparison::Branch
                    && diff
                        .repositories
                        .first()
                        .is_none_or(|repo| repo.branch_base.is_none())
                {
                    self.comparison = Comparison::WorkingTree;
                    self.diff = None;
                    self.new_diff = None;
                    self.diff_view = None;
                    self.diff_subscription = None;
                    self.reviewing_file = false;
                    self.refresh_diff();
                    cx.notify();
                    return;
                }
                if !historical
                    && self
                        .diff
                        .as_ref()
                        .is_some_and(|current| current.same_content(&diff))
                {
                    self.new_diff = None;
                } else if !historical
                    && self.reviewing_file
                    && self
                        .diff_view
                        .as_ref()
                        .is_some_and(|viewer| viewer.read(cx).has_selection())
                {
                    self.new_diff = Some(diff);
                } else {
                    self.install_diff(diff);
                }
            }
            Response::Accepted {
                task_id,
                submission_id,
            } => {
                if self
                    .submitted_draft
                    .as_ref()
                    .is_some_and(|p| p.task_id == task_id && p.id == submission_id)
                {
                    let pending = self.submitted_draft.take().unwrap();
                    // A queued message hops from the composer into the queue tray above it.
                    if pending.queued
                        && self.selected.as_ref() == Some(&task_id)
                        && let Some(composer) = fx::anchor_bounds(chat::COMPOSER_ANCHOR, cx)
                    {
                        fx::glide_to(
                            composer.center(),
                            composition::OUTBOX_ANCHOR,
                            point(composer.center().x, composer.top() - px(24.)),
                            pending.text.clone(),
                            self.theme.accent,
                            cx,
                        );
                    }
                    if self.selected.as_ref() == Some(&task_id) && !cx.reduce_motion() {
                        self.send_arrivals.retain(|_, arrival| !arrival.expired());
                        let mut arrival = motion::Arrival::new(pending.origin);
                        arrival.queued = pending.queued;
                        self.send_arrivals.insert(submission_id.clone(), arrival);
                        self.follow = true;
                        self.chat_scroll.scroll_to_end();
                    }
                    if self.selected.as_ref() == Some(&task_id)
                        && self.draft_revision == pending.revision
                        && self.input.read(cx).text() == pending.text
                    {
                        self.input.update(cx, |i, cx| i.set_text(String::new(), cx));
                    }
                    // Acceptance owns the submitted content. Persist whatever remains in the composer.
                    self.draft_dirty = true;
                }
                if let core::Request::Submit {
                    feedback_ids,
                    attachment_ids,
                    ..
                } = &reply.request
                    && let Some(view) = &mut self.view
                    && view.task.id == task_id
                {
                    view.task
                        .attachments
                        .retain(|a| !attachment_ids.contains(&a.id));
                    view.task.feedback.retain(|f| !feedback_ids.contains(&f.id));
                }
                self.send(core::Request::Task {
                    task_id,
                    before: None,
                });
            }
            Response::Attachments {
                task_id,
                attachments,
            } => {
                if let Some(view) = &mut self.view
                    && view.task.id == task_id
                {
                    for attachment in attachments {
                        if !view.task.attachments.iter().any(|a| a.id == attachment.id) {
                            view.task.attachments.push(attachment);
                        }
                    }
                }
            }
            Response::ComposerCatalog(catalog) => {
                self.receive_slash_catalog(&reply.request, catalog)
            }
            Response::Resources(resources) => self.resources = resources,
            Response::Files(files) => {
                self.files = files;
                self.files_loaded();
            }
            Response::Text(text) => {
                if let core::Request::ReadFile { path, .. } = reply.request {
                    self.file_text = Some((String::from_utf8_lossy(&path).into(), text));
                }
            }
            Response::Terminal(terminal) => self.terminal = Some(terminal),
            Response::Tools(tools) => {
                self.tool_installers.retain(|installing| {
                    tools
                        .iter()
                        .all(|found| found.tool != *installing || found.version.is_none())
                });
                self.tools = tools;
            }
            // The bridge turns each answer of its watcher into a task view.
            Response::Watched(_) => {}
            // Answers to the tools that a provider agent calls. The desktop never asks for them.
            Response::Agents(_) | Response::Delegated(_) => {}
            Response::Ok => match reply.request {
                core::Request::CompactTask { .. } => {
                    let tone = self.theme.info;
                    self.notify_user("Compacting context…", tone, cx);
                }
                core::Request::RemoveAttachment { task_id, id } => {
                    if let Some(view) = &mut self.view
                        && view.task.id == task_id
                    {
                        view.task
                            .attachments
                            .retain(|attachment| attachment.id != id);
                    }
                }
                core::Request::CustomizeProject { .. } => {
                    self.editing_project = None;
                    self.panel = Panel::None;
                    self.send(core::Request::Hello);
                }
                core::Request::RemoveProject { project_id } => {
                    self.sidebar_dirty = true;
                    self.editing_project = None;
                    self.panel = Panel::None;
                    if self.selected_project.as_ref() == Some(&project_id) {
                        self.selected_project = None;
                    }
                    if self.sidebar_project.as_ref() == Some(&project_id) {
                        self.sidebar_project = None;
                    }
                    self.send(core::Request::Hello);
                }
                core::Request::DeleteTask { task_id } => {
                    // The deleted thread's row breaks into pixels that fall away.
                    if let Some(row) = fx::anchor_bounds(&sidebar::thread_anchor(&task_id), cx) {
                        fx::crumble(row, self.theme.removed, cx);
                    }
                    self.settings.viewed.remove(&task_id);
                    self.sidebar_dirty = true;
                    self.deleting_task = None;
                    self.thread_menu = None;
                    self.panel = Panel::None;
                    if let Some(workspace) = &mut self.workspace {
                        workspace.tasks.retain(|task| task.id != task_id);
                    }
                    if self.selected.as_ref() == Some(&task_id) {
                        self.clear_selected_task(cx);
                    }
                    self.send(core::Request::Hello);
                }
                core::Request::EditTask {
                    task_id,
                    edit: TaskEdit::Settle(_) | TaskEdit::Pin(_) | TaskEdit::Archive(_),
                } => {
                    if self.selected.as_ref() == Some(&task_id) {
                        self.send(core::Request::Task {
                            task_id,
                            before: None,
                        });
                    }
                    self.send(core::Request::Hello);
                }
                core::Request::Submit { .. } => {}
                core::Request::AddFeedback { task_id, .. }
                | core::Request::AddFeedbackRows { task_id, .. }
                    if self.selected.as_ref() == Some(&task_id) =>
                {
                    self.panel = Panel::None;
                    self.reviewing_file = false;
                    self.diff_view = None;
                    if let Some(task_id) = &self.selected {
                        self.send(core::Request::Task {
                            task_id: task_id.clone(),
                            before: None,
                        });
                    }
                }
                core::Request::Answer { task_id, .. } => {
                    self.send(core::Request::Task {
                        task_id,
                        before: None,
                    });
                    self.send(core::Request::Hello);
                }
                core::Request::StageFile { .. } => self.refresh_diff(),
                core::Request::EditTask {
                    task_id,
                    edit:
                        TaskEdit::Model { .. }
                        | TaskEdit::Access(_)
                        | TaskEdit::Account(_)
                        | TaskEdit::Provider(_)
                        | TaskEdit::ProviderAccess(_),
                } => {
                    self.new_preferences = None;
                    self.send(core::Request::Task {
                        task_id,
                        before: None,
                    });
                    self.send(core::Request::Hello);
                }
                core::Request::EditTask { .. }
                | core::Request::SaveSettings(_)
                | core::Request::TestNotification { .. }
                | core::Request::TerminalInput { .. }
                | core::Request::TerminalResize { .. } => {}
                core::Request::Shutdown => cx.quit(),
                core::Request::InstallTool { tool } => {
                    if !self.tool_installers.contains(&tool) {
                        self.tool_installers.push(tool);
                    }
                }
                _ => self.send(core::Request::Hello),
            },
            Response::Error(e) => {
                if matches!(
                    reply.request,
                    core::Request::SetAgentPreferences(_) | core::Request::SetAccessChoice(_)
                ) {
                    self.new_preferences = None;
                    self.send(core::Request::Hello);
                }
                self.answer_failed(&reply.request);
                if let core::Request::Submit {
                    task_id,
                    submission_id,
                    ..
                } = &reply.request
                    && self
                        .submitted_draft
                        .as_ref()
                        .is_some_and(|p| &p.task_id == task_id && &p.id == submission_id)
                {
                    self.submitted_draft = None;
                }
                if matches!(reply.request, core::Request::AddFeedbackRows { .. })
                    && let Some(viewer) = &self.diff_view
                {
                    viewer.update(cx, |v, cx| v.context_failed(cx));
                }
                // The setup dialog reports a provider that did not start in that provider's row.
                if self.panel != Panel::Setup
                    || !matches!(reply.request, core::Request::ConnectRuntime { .. })
                {
                    self.report_error(e, cx);
                }
            }
        }
        cx.notify();
    }
    fn clear_selected_task(&mut self, cx: &mut Context<Self>) {
        self.new_worktree_override = None;
        self.slash = Default::default();
        self.send_arrivals.clear();
        self.selected = None;
        self.files.clear();
        self.view = None;
        self.diff = None;
        self.new_diff = None;
        self.diff_view = None;
        self.diff_subscription = None;
        self.reviewing_file = false;
        self.historical_diff = false;
        self.task_changes = None;
        self.terminal = None;
        self.file_text = None;
        self.text_views.clear();
        self.unfolded_texts.clear();
        self.chat_selection = cx.new(|_| Default::default());
        self.input
            .update(cx, |input, cx| input.set_text(String::new(), cx));
        self.draft_dirty = false;
        self.settings.selected_task = None;
        self.save_settings();
        self.submitted_draft = None;
        self.question_inputs.clear();
        self.answering.clear();
        self.auto_submit_task = None;
        self.new_submission_origin = None;
        self.sync_placeholder(cx);
    }
    fn select_task(&mut self, id: String, cx: &mut Context<Self>) {
        self.agent_popup = None;
        if self.creating_task {
            return;
        }
        // A task opens in the workspace of its kind.
        let kind = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.tasks.iter().find(|task| task.id == id))
            .map(|task| task.kind);
        if let Some(kind) = kind
            && kind != self.mode
        {
            self.enter_mode(kind, cx);
        }
        if self.selected.is_none() && self.auto_submit_task.as_ref() != Some(&id) {
            self.empty_draft = self.input.read(cx).text();
        }
        if let Some(task_id) = &self.selected
            && self.view.is_some()
            && self.submitted_draft.as_ref().is_none_or(|p| {
                p.revision != self.draft_revision || p.text != self.input.read(cx).text()
            })
        {
            self.send(core::Request::EditTask {
                task_id: task_id.clone(),
                edit: TaskEdit::Draft(self.input.read(cx).text()),
            });
        }
        self.submitted_draft = None;
        self.input
            .update(cx, |input, cx| input.set_text(String::new(), cx));
        self.draft_dirty = false;
        self.slash = Default::default();
        self.send_arrivals.clear();
        if self.auto_submit_task.as_ref() != Some(&id) {
            self.new_submission_origin = None;
        }
        self.selected = Some(id.clone());
        self.history_hover = None;
        self.files.clear();
        self.task_changes = None;
        self.view = None;
        self.diff = None;
        self.historical_diff = false;
        self.diff_view = None;
        self.diff_subscription = None;
        self.new_diff = None;
        self.text_views.clear();
        self.unfolded_texts.clear();
        self.chat_selection = cx.new(|_| Default::default());
        self.question_inputs.clear();
        self.answering.clear();
        self.file = 0;
        self.reviewing_file = false;
        self.terminal = None;
        self.file_text = None;
        self.settings.selected_task = Some(id.clone());
        self.settings.viewed.insert(id.clone(), now());
        self.sidebar_dirty = true;
        self.save_settings();
        self.send(core::Request::Task {
            task_id: id,
            before: None,
        });
        if self.has_checkout() {
            self.refresh_diff();
            self.refresh_task_changes();
        }
        self.sync_placeholder(cx);
        cx.notify();
    }
    /// Reports that a task is open and works in a Git checkout. A chat has none, so it has no
    /// changes, files or terminal.
    pub(super) fn has_checkout(&self) -> bool {
        self.selected.is_some() && self.mode.has_project()
    }
    fn save_window_placement(&self) {
        if let Err(error) = crate::window_state::save(self.window_placement) {
            eprintln!("Could not save window placement: {error}");
        }
    }
    fn save_settings(&self) {
        self.send(core::Request::SaveSettings(Box::new(
            self.settings_snapshot(),
        )));
    }
    fn settings_snapshot(&self) -> Settings {
        let mut settings = self.settings.clone();
        settings.show_review = self.show_review;
        settings.show_terminal = self.show_terminal;
        settings.split_diff = self.split;
        settings.selected_project = self.selected_project.clone();
        settings
    }
    fn open_project(&mut self, _: &OpenProject, window: &mut Window, cx: &mut Context<Self>) {
        // The Git workspace opens repositories in its own rail; each one becomes a project too.
        if self.git_open {
            let git = self.git_workspace(window, cx);
            git.update(cx, |git, cx| git.open_repository_dialog(window, cx));
            return;
        }
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open repository".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = chosen.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                let _ = this.update(cx, |s, _| {
                    s.send(core::Request::OpenProject {
                        path,
                        missing_repository: MissingRepository::Ask,
                    })
                });
            }
        })
        .detach();
    }
    /// Opens the folder that has no Git repository, as the user chose in its dialog.
    fn open_folder_without_git(&mut self, choice: MissingRepository) {
        if let Some(path) = self.folder_without_git.clone() {
            self.send(core::Request::OpenProject {
                path,
                missing_repository: choice,
            });
        }
    }
    /// Tells whether a project's folder has a Git repository of its own.
    fn project_has_git(&self, project_id: &str) -> bool {
        self.workspace
            .as_ref()
            .is_none_or(|workspace| !workspace.without_git.contains(project_id))
    }
    /// Reports that the open task works in a Git checkout, so its changes can be compared.
    fn tracks_changes(&self) -> bool {
        self.has_checkout()
            && self
                .workspace
                .as_ref()
                .and_then(|w| {
                    w.tasks
                        .iter()
                        .find(|t| self.selected.as_ref() == Some(&t.id))
                })
                .is_none_or(|task| self.project_has_git(&task.project_id))
    }
    fn new_task(&mut self, _: &NewTask, window: &mut Window, cx: &mut Context<Self>) {
        self.close_git(cx);
        self.open_empty_chat(cx);
        window.focus(&self.input.read(cx).focus_handle(cx), cx);
    }
    /// Tells whether the next task gets its own worktree. A folder without Git has none.
    fn composer_worktree(&self) -> bool {
        self.composer_project()
            .is_none_or(|project| self.project_has_git(&project.id))
            && self
                .new_worktree_override
                .unwrap_or(self.settings.default_worktree)
    }
    fn open_empty_chat(&mut self, cx: &mut Context<Self>) {
        if self.creating_task {
            return;
        }
        self.new_worktree_override = None;
        if let Some(view) = &self.view
            && self.submitted_draft.is_none()
        {
            self.send(core::Request::EditTask {
                task_id: view.task.id.clone(),
                edit: TaskEdit::Draft(self.input.read(cx).text()),
            });
        }
        if self.selected.is_some() {
            self.clear_selected_task(cx);
            let draft = self.empty_draft.clone();
            self.input.update(cx, |input, cx| input.set_text(draft, cx));
        }
        self.panel = Panel::None;
        cx.notify();
    }
    fn submit(&mut self, _: &SendPrompt, _: &mut Window, cx: &mut Context<Self>) {
        self.submit_current(cx);
    }
    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel == Panel::None
            && let Some(viewer) = &self.diff_view
        {
            viewer.update(cx, |v, cx| v.clear_selection(cx));
        }
        if self.panel == Panel::None && self.search_visible {
            self.search_visible = false;
            self.search
                .update(cx, |input, cx| input.set_text(String::new(), cx));
            window.focus(&self.focus, cx);
        }
        self.panel = Panel::None;
        self.dragging_sidebar = false;
        self.dragging_review = false;
        cx.notify();
    }
    fn readonly(&mut self, key: String, text: String, cx: &mut Context<Self>) -> Entity<TextInput> {
        self.readonly_streaming(key, &text, false, cx)
    }
    fn readonly_streaming(
        &mut self,
        key: String,
        text: &str,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let animate = streaming && !cx.reduce_motion();
        let input = if let Some((input, _)) = self.text_views.get(&key) {
            if !input.read(cx).shows(text) {
                let text = text.to_owned();
                input.update(cx, |input, cx| input.set_streamed_text(text, animate, cx));
            }
            input.clone()
        } else {
            let text = text.to_owned();
            let input = cx.new(|cx| {
                let mut input = TextInput::readonly(String::new(), cx);
                input.set_streamed_text(text, animate, cx);
                input
            });
            let subscription = cx.subscribe(&input, |s, _, reference: &ReferenceInput, cx| {
                s.open_reference(&reference.0, cx)
            });
            self.text_views.insert(key, (input.clone(), subscription));
            input
        };
        if let Some(inputs) = &mut self.selection_inputs {
            inputs.push(input.clone());
        }
        input
    }
    // Marks the moment a task changes hands: a finished turn, a failure, or a request for input
    // lands as an impact on its thread row. The notification center keeps the events of tasks
    // other than the open one.
    fn celebrate_transitions(&mut self, incoming: &Workspace, cx: &mut Context<Self>) {
        let Some(current) = &self.workspace else {
            return;
        };
        let t = self.theme;
        let mut events = Vec::new();
        for task in &incoming.tasks {
            let Some(previous) = current.tasks.iter().find(|old| old.id == task.id) else {
                continue;
            };
            let kind = if !previous.needs_input() && task.needs_input() {
                Some(notices::NoticeKind::NeedsInput)
            } else if previous.state.active() && task.state == RunState::Completed {
                Some(notices::NoticeKind::Finished)
            } else if previous.state.active() && task.is_blocked() {
                Some(notices::NoticeKind::Blocked)
            } else {
                None
            };
            if let Some(kind) = kind {
                fx::impact(&sidebar::thread_anchor(&task.id), kind.tone(t), cx);
                if self.selected.as_ref() != Some(&task.id) {
                    events.push((kind, task.id.clone(), task.title.clone()));
                }
            }
        }
        for (kind, task_id, title) in events {
            self.record_task_event(kind, task_id, title, cx);
        }
    }

    // Opens a file or path that the transcript referred to: in the file browser when it belongs
    // to the task's checkout, otherwise in the system's default application.
    fn open_reference(&mut self, reference: &str, cx: &mut Context<Self>) {
        let path = reference
            .split(['#', ':'])
            .next()
            .unwrap_or(reference)
            .trim_start_matches('@');
        if path.is_empty() {
            return;
        }
        let Some(view) = &self.view else {
            return;
        };
        let root = &view.task.cwd;
        let absolute = if let Some(rest) = path.strip_prefix("~/") {
            kodem_core::platform::home_dir()
                .ok()
                .map(|home| home.join(rest))
        } else {
            Some(root.join(path))
        };
        let Some(absolute) = absolute else {
            return;
        };
        // A chat has no checkout to browse, so its references open in their own application.
        if self.has_checkout()
            && let Ok(relative) = absolute.strip_prefix(root)
        {
            self.panel = Panel::Files;
            self.send(core::Request::Files {
                task_id: view.task.id.clone(),
            });
            self.send(core::Request::ReadFile {
                task_id: view.task.id.clone(),
                path: kodem_core::platform::os_bytes(relative.as_os_str()),
            });
        } else {
            cx.open_with_system(&absolute);
        }
        cx.notify();
    }

    /// Opens preferences on one section.
    pub(super) fn open_settings(&mut self, section: usize) {
        self.panel = Panel::Settings;
        self.settings_section = section;
        if section == settings::PROVIDERS {
            self.settings_provider = self.composer_preferences().provider;
            self.send(core::Request::ConnectRuntime {
                provider: self.settings_provider,
                account: None,
            });
        }
    }

    /// Opens the provider's inventory of skills, instructions and MCP servers.
    pub(super) fn open_resources(&mut self) {
        let preferences = self.composer_preferences();
        self.panel = Panel::Resources;
        self.send(core::Request::Resources {
            provider: preferences.provider,
            account: preferences.account,
            project_id: self.composer_project().map(|project| project.id.clone()),
            task_id: self.selected.clone(),
        });
    }

    /// Returns the project that the open task belongs to, or that a new task will start in.
    /// A chat has no project.
    pub(super) fn composer_project(&self) -> Option<&Project> {
        if !self.mode.has_project() {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        let id = self
            .view
            .as_ref()
            .map(|view| &view.task.project_id)
            .or(self.selected_project.as_ref());
        workspace
            .projects
            .iter()
            .find(|project| Some(&project.id) == id)
            .or(workspace.projects.first())
    }

    /// Returns the bare control base: geometry, mono label type and focus edge, with no surface
    /// texture. Callers that restyle it into a row or chip start from here.
    fn control(&self, id: impl Into<ElementId>) -> Button {
        let theme = self.theme;
        Button::new(id)
            .relative()
            .h(px(26.))
            .px(px(9.))
            .gap(px(5.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .bg(theme.control)
            .border_1()
            .border_color(theme.edge)
            .text_color(theme.text)
            .font_family(crate::theme::mono())
            .text_size(text_px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .styles(|s| s.disabled(|s| s.opacity(0.4).cursor_default()))
            // Only keyboard focus draws the edge; a click must not leave a control outlined.
            .focus_visible(move |s| s.border_color(theme.accent.opacity(0.7)))
    }
    /// Returns a raised neutral button: a mono label on a dithered surface that glints on hover
    /// and ripples from the press point.
    fn button(
        &self,
        id: impl Into<ElementId>,
        glyph: &'static str,
        label: impl Into<SharedString>,
    ) -> Button {
        let theme = self.theme;
        let label = label.into();
        self.control(id)
            .overflow_hidden()
            .child(kit::shade(theme.dot.opacity(0.9), dither::Edge::Bottom))
            .child(kit::glint(theme.accent.opacity(0.5), dither::Edge::Bottom))
            .child(icon(glyph).size(px(13.)))
            .when(!label.is_empty(), |b| b.child(label))
            .bg(kit::raised_fill(theme.control))
            .shadow(kit::raised_light(theme))
            .hover(move |s| {
                s.bg(kit::raised_fill(theme.control_hover))
                    .border_color(theme.accent.opacity(0.5))
            })
            // A pressed button sinks flat: its light goes out.
            .active(move |s| s.bg(theme.control_active).shadow(Vec::new()))
            .on_press(move |position, cx| fx::ripple(position, theme.text, cx))
    }
    /// Returns a button with no surface until the pointer is over it.
    fn ghost(
        &self,
        id: impl Into<ElementId>,
        name: &'static str,
        label: impl Into<SharedString>,
    ) -> Button {
        let theme = self.theme;
        let label = label.into();
        self.control(id)
            .overflow_hidden()
            .child(kit::glint(theme.dot, dither::Edge::Bottom))
            .child(icon(name).size(px(13.)))
            .when(!label.is_empty(), |b| b.child(label))
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .text_color(theme.muted)
            .hover(move |s| s.bg(theme.selected).text_color(theme.text))
            .active(move |s| s.bg(theme.control_active))
            .on_press(move |position, cx| fx::ripple(position, theme.muted, cx))
    }
    /// Returns a square ghost button with a tooltip and an accessible name.
    fn icon_button(
        &self,
        id: impl Into<ElementId>,
        name: &'static str,
        label: &'static str,
    ) -> Button {
        let theme = self.theme;
        self.ghost(id, name, "")
            .w(px(26.))
            .px_0()
            .accessibility_label(kit::shortcut(label))
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: label.into(),
                    theme,
                })
                .into()
            })
    }
    /// Returns the main action of a surface: a raised glass key in the accent.
    fn primary(
        &self,
        id: impl Into<ElementId>,
        glyph: &'static str,
        label: impl Into<SharedString>,
    ) -> Button {
        self.glass(id, glyph, label, self.theme.accent)
    }
    /// Returns a raised glass key in a tone: a bezel ring and a lens ring around a dithered
    /// face, an engraved icon and a soft glow of the tone below it.
    fn glass(
        &self,
        id: impl Into<ElementId>,
        glyph: &'static str,
        label: impl Into<SharedString>,
        tone: Hsla,
    ) -> Button {
        let theme = self.theme;
        // An empty label must not become a child: it would take a gap and push the icon of an
        // icon-only button off center.
        let label = label.into();
        self.control(id)
            .group(kit::GLASS)
            .overflow_hidden()
            .px(px(12.))
            .child(kit::glass_body(tone))
            .child(kit::engraved(glyph, tone, 13.))
            .when(!label.is_empty(), |b| b.child(label))
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .shadow(kit::glass_glow(tone, theme, kit::Pose::Rest))
            .text_color(theme.on_accent)
            .font_weight(FontWeight::SEMIBOLD)
            .hover(move |s| s.shadow(kit::glass_glow(tone, theme, kit::Pose::Hover)))
            .active(move |s| s.shadow(kit::glass_glow(tone, theme, kit::Pose::Pressed)))
            .on_press(move |position, cx| fx::ripple(position, tone, cx))
    }
    /// Returns a button tinted with a tone that carries its own meaning, such as a destructive
    /// or approving action.
    fn tinted(
        &self,
        id: impl Into<ElementId>,
        glyph: &'static str,
        label: impl Into<SharedString>,
        tone: Hsla,
    ) -> Button {
        let label = label.into();
        self.control(id)
            .overflow_hidden()
            .child(kit::shade(tone.opacity(0.3), dither::Edge::Bottom))
            .child(kit::glint(tone.opacity(0.6), dither::Edge::Bottom))
            .child(icon(glyph).size(px(13.)))
            .when(!label.is_empty(), |b| b.child(label))
            .bg(kit::tint_fill(tone, 0.09))
            .shadow(kit::tint_light(tone))
            .border_color(tone.opacity(0.45))
            .text_color(tone)
            .hover(move |s| {
                s.bg(kit::tint_fill(tone, 0.16))
                    .border_color(tone.opacity(0.7))
            })
            .active(move |s| s.bg(tone.opacity(0.22)).shadow(Vec::new()))
            .on_press(move |position, cx| fx::ripple(position, tone, cx))
    }
}

/// Returns a small uppercase mono label, used for section titles, table headers and captions.
pub(super) fn caps(text: impl AsRef<str>, theme: Theme) -> Div {
    div()
        .flex_shrink_0()
        .font_family(crate::theme::mono())
        .text_size(text_px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .letter_spacing(text_px(crate::theme::LABEL_TRACKING))
        .text_color(theme.subtle)
        .line_height(text_px(14.))
        .child(text.as_ref().to_uppercase())
}

/// Returns tabular monospace text for counts, durations and other numeric metadata.
pub(super) fn figure(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .font_family(crate::theme::mono())
        .text_size(text_px(11.))
        .text_color(color)
        .child(text.into())
}

/// Returns colored addition and removal totals in tabular figures.
pub(super) fn line_totals(
    added: impl std::fmt::Display,
    removed: impl std::fmt::Display,
    theme: Theme,
) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .gap(px(5.))
        .child(figure(format!("+{added}"), theme.added))
        .child(figure(format!("−{removed}"), theme.removed))
}

// Returns the first of the tool rows that end before `end`, or `end` when the row before it is
// not a tool row. A change at `end` can resize every tray in that stretch.
fn tool_rows_start(messages: &[Message], end: usize) -> usize {
    let mut row = end.min(messages.len());
    while row > 0 && messages[row - 1].kind == MessageKind::Tool {
        row -= 1;
    }
    row
}

// Returns the row after the tool rows that start at `start`.
fn tool_rows_end(messages: &[Message], start: usize) -> usize {
    let mut row = start.min(messages.len());
    while row < messages.len() && messages[row].kind == MessageKind::Tool {
        row += 1;
    }
    row
}

// The watcher and polls refresh the tail; explicit page requests extend the retained prefix.
fn merge_transcript(current: &TaskView, incoming: &mut TaskView, older: bool) {
    if !older && !current.messages.is_empty() {
        incoming.before = current.before;
    }
    // The watcher and the explicit requests use separate connections, so an answer can arrive
    // after one that the host produced later. Such an answer adds the rows that the window
    // does not hold, but it must not put older rows or an older task state back.
    let outdated = incoming.task.id == current.task.id
        && incoming.epoch == current.epoch
        && incoming.serial < current.serial;
    let (base, news) = if outdated {
        (
            std::mem::take(&mut incoming.messages),
            current.messages.clone(),
        )
    } else {
        (
            current.messages.clone(),
            std::mem::take(&mut incoming.messages),
        )
    };
    let mut messages: std::collections::BTreeMap<_, _> = base
        .into_iter()
        .map(|message| (message.seq, message))
        .collect();
    for message in news {
        messages.insert(message.seq, message);
    }
    incoming.messages = messages.into_values().collect();
    if outdated {
        incoming.task = current.task.clone();
        incoming.activities = current.activities.clone();
        incoming.outgoing = current.outgoing.clone();
        incoming.requests = current.requests.clone();
        incoming.serial = current.serial;
    }
}
struct Hint {
    text: String,
    theme: Theme,
}
impl Render for Hint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .max_w(px(360.))
            .child(dither::shadow(self.theme, 5., 3.))
            .child(
                div()
                    .font_family(crate::theme::mono())
                    .px(px(8.))
                    .py(px(5.))
                    .text_size(text_px(11.))
                    .line_height(relative(1.45))
                    .bg(self.theme.raised)
                    .text_color(self.theme.text)
                    .border_1()
                    .border_color(self.theme.edge)
                    .rounded(px(5.))
                    .child(kit::shortcut(self.text.clone())),
            )
    }
}
/// Installs workspace keyboard commands and the application menu. `secondary` is Command on
/// macOS and Control elsewhere.
pub fn init(cx: &mut App) {
    crate::components::text_input::init(cx);
    git::init(cx);
    cx.bind_keys([
        KeyBinding::new("secondary-o", OpenProject, None),
        KeyBinding::new("secondary-n", NewTask, None),
        KeyBinding::new("secondary-b", ToggleSidebar, None),
        KeyBinding::new("secondary-d", ToggleReview, None),
        KeyBinding::new("secondary-,", ShowSettings, None),
        KeyBinding::new("secondary-f", Search, None),
        KeyBinding::new("secondary-k", CommandPalette, None),
        KeyBinding::new("secondary-shift-]", NextTask, None),
        KeyBinding::new("secondary-shift-[", PreviousTask, None),
        KeyBinding::new("ctrl-tab", NextTask, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTask, None),
        KeyBinding::new("secondary-alt-a", NextAttention, None),
        KeyBinding::new("secondary-shift-m", ChooseModel, None),
        KeyBinding::new("secondary-shift-e", ChooseEffort, None),
        KeyBinding::new("secondary-shift-a", ChooseAccess, None),
        KeyBinding::new("shift-tab", CycleAccess, None),
        KeyBinding::new("ctrl-`", ToggleTerminal, None),
        KeyBinding::new("secondary-1", GoToTask1, None),
        KeyBinding::new("secondary-2", GoToTask2, None),
        KeyBinding::new("secondary-3", GoToTask3, None),
        KeyBinding::new("secondary-4", GoToTask4, None),
        KeyBinding::new("secondary-5", GoToTask5, None),
        KeyBinding::new("secondary-6", GoToTask6, None),
        KeyBinding::new("secondary-7", GoToTask7, None),
        KeyBinding::new("secondary-8", GoToTask8, None),
        KeyBinding::new("secondary-9", GoToTask9, None),
        KeyBinding::new("secondary-alt-1", ShowCoding, None),
        KeyBinding::new("secondary-alt-2", ShowChat, None),
        KeyBinding::new("secondary-alt-3", ShowReview, None),
        KeyBinding::new("secondary-alt-4", ShowGit, None),
        KeyBinding::new("secondary-shift-o", CloneRepository, None),
        KeyBinding::new("escape", Cancel, None),
        KeyBinding::new("secondary-q", Quit, None),
    ]);
    cx.set_menus(vec![
        Menu {
            disabled: false,
            name: "Kodem".into(),
            items: vec![
                MenuItem::action("Settings", ShowSettings),
                MenuItem::separator(),
                MenuItem::action("Quit Kodem", Quit),
            ],
        },
        Menu {
            disabled: false,
            name: "File".into(),
            items: vec![
                MenuItem::action("Open Repository", OpenProject),
                MenuItem::action("Clone Repository", CloneRepository),
                MenuItem::action("New Task", NewTask),
            ],
        },
        Menu {
            disabled: false,
            name: "View".into(),
            items: vec![
                MenuItem::action("Command Palette", CommandPalette),
                MenuItem::separator(),
                MenuItem::action("Agentic Coding", ShowCoding),
                MenuItem::action("Chat", ShowChat),
                MenuItem::action("Review", ShowReview),
                MenuItem::action("Git (Beta)", ShowGit),
                MenuItem::separator(),
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::action("Toggle Changes", ToggleReview),
                MenuItem::action("Toggle Terminal", ToggleTerminal),
                MenuItem::action("Jump to Latest", Latest),
                MenuItem::separator(),
                MenuItem::action("Next Task", NextTask),
                MenuItem::action("Previous Task", PreviousTask),
                MenuItem::action("Next Task That Needs You", NextAttention),
            ],
        },
    ]);
}

// Resolve the local account's first name once, outside rendering.
fn user_first_name() -> String {
    full_user_name()
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("USERNAME").ok())
        .and_then(|name| name.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "there".into())
}

// The account's full name as macOS records it.
#[cfg(target_os = "macos")]
fn full_user_name() -> Option<String> {
    std::process::Command::new("/usr/bin/id")
        .arg("-F")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
}

// The account's full name from the comment field of its passwd entry.
#[cfg(all(unix, not(target_os = "macos")))]
fn full_user_name() -> Option<String> {
    let user = std::env::var("USER").ok()?;
    let output = std::process::Command::new("getent")
        .args(["passwd", &user])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let entry = String::from_utf8(output.stdout).ok()?;
    let name = entry.split(':').nth(4)?.split(',').next()?.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

// Windows gives the sign-in name only, without asking a directory service.
#[cfg(windows)]
fn full_user_name() -> Option<String> {
    None
}
