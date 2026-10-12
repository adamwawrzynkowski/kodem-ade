//! Native workspace state and actions. All blocking work goes through the host bridge.
mod accounts;
mod activity;
mod backdrop;
mod browser;
mod change_tree;
mod chat;
mod chrome;
mod composition;
mod controls;
mod conversation;
mod diff_refresh;
mod diff_view;
mod dither;
mod effort_slider;
mod fx;
mod history;
mod image_preview;
mod kit;
mod menus;
mod message_content;
mod model_picker;
mod motion;
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
use crate::icons::icon;
use crate::{
    bridge,
    components::{
        Button,
        text_input::{CompletionInput, ReferenceInput, SubmitInput, TextInput},
    },
    theme::Theme,
};
pub use chrome::Stage;
use conversation::Conversation;
use gpui::{prelude::*, *};
use kodem_core::{self as core, *};
use std::{
    collections::{HashMap, HashSet},
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
        GoToTask9
    ]
);
#[derive(Clone, PartialEq)]
enum Panel {
    None,
    CustomizeProject,
    RemoveProject,
    ChatProjects,
    Settings,
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
}
struct ThreadMenu {
    task_id: String,
    position: Point<Pixels>,
}
/// A short confirmation that leaves on its own.
struct Toast {
    text: SharedString,
    tone: Hsla,
    sequence: u64,
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
    view: Option<TaskView>,
    agent_popup: Option<subagents::AgentPopup>,
    selected: Option<String>,
    input: Entity<TextInput>,
    aux: Entity<TextInput>,
    search: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
    settings: Settings,
    theme: Theme,
    panel: Panel,
    /// Bounds of the control that opened the current menu. The menu opens above or below it.
    menu_anchor: Option<Bounds<Pixels>>,
    thread_menu: Option<ThreadMenu>,
    deleting_task: Option<String>,
    error: Option<String>,
    toast: Option<Toast>,
    toast_sequence: u64,
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
    follow: bool,
    /// Sequence number of the sent message whose history tick is under the pointer.
    history_hover: Option<u64>,
    folded: HashSet<String>,
    /// Tool runs and thoughts the user opened or folded by hand; the others follow their work.
    tray_choices: HashMap<String, bool>,
    collapsed_changes: HashSet<String>,
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
        let input =
            cx.new(|cx| TextInput::composer("Describe what you want to build or change…", cx));
        let aux = cx.new(|cx| TextInput::new("Type here…", cx));
        let search = cx.new(|cx| TextInput::new("Search tasks…", cx));
        let palette_input =
            cx.new(|cx| TextInput::composer("Type a command or search threads…", cx));
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
                                s.show_terminal,
                                s.agent_popup.as_ref().map(|p| p.thread_id.clone()),
                            ));
                        }
                        if s.host_connected && s.selected.is_some() && s.changes_refresh.due() {
                            s.refresh_task_changes();
                        }
                        if s.host_connected
                            && s.selected.is_some()
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
        let mut shell = Self {
            tx,
            update_state: crate::updater::State::Idle,
            update_tx,
            last_update_check: std::time::Instant::now(),
            focus,
            workspace: None,
            view: None,
            agent_popup: None,
            selected: None,
            input,
            aux,
            search,
            _subscriptions: subscriptions,
            settings: Settings::default(),
            theme: Theme::new(false),
            panel: Panel::None,
            menu_anchor: None,
            usage_actions: std::collections::BTreeMap::new(),
            thread_menu: None,
            deleting_task: None,
            error: None,
            toast: None,
            toast_sequence: 0,
            host_connected: false,
            new_worktree_override: None,
            new_preferences: None,
            approval_memory: HashMap::new(),
            settings_provider: Provider::default(),
            selected_project: None,
            editing_project: None,
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
            follow: true,
            history_hover: None,
            folded: HashSet::new(),
            tray_choices: HashMap::new(),
            collapsed_changes: HashSet::new(),
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
            | core::Request::ReadFile { task_id, .. }
            | core::Request::Files { task_id }
            | core::Request::Terminal { task_id }
            | core::Request::TaskChanges { task_id } => Some(task_id),
            _ => None,
        };
        if response_task.is_some_and(|id| self.selected.as_ref() != Some(id)) {
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
                    if self.selected.as_ref() == Some(task_id) {
                        if let Some(popup) = self
                            .agent_popup
                            .as_mut()
                            .filter(|p| &p.thread_id == thread_id)
                        {
                            popup.error = Some(e);
                            cx.notify();
                        }
                    }
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
                if matches!(reply.request, core::Request::SetAgentPreferences(_)) {
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
                if matches!(reply.request, core::Request::CreateTask { .. }) {
                    self.creating_task = false;
                    self.new_submission_origin = None;
                }
                if matches!(reply.request, core::Request::ImportAttachments { .. }) {
                    self.auto_submit_task = None;
                    self.new_submission_origin = None;
                }
                self.error = Some(e);
                cx.notify();
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
                    self.theme = Theme::with_accent(self.settings.light, self.settings.accent);
                    let selected = self
                        .settings
                        .selected_task
                        .clone()
                        .filter(|id| workspace.tasks.iter().any(|t| &t.id == id));
                    self.workspace = Some(workspace);
                    if let Some(id) = selected {
                        self.select_task(id, cx)
                    }
                    if self.selected.is_none()
                        && self.workspace.as_ref().is_some_and(|w| {
                            w.runtimes
                                .get(&self.composer_preferences().provider)
                                .is_none_or(|r| r.models.is_empty())
                        })
                    {
                        self.connect_runtime();
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
            }
            Response::Account(account) => self.account_added(&account, cx),
            Response::Project(project) => {
                if self.selected.is_some() {
                    self.open_empty_chat(cx);
                }
                self.selected_project = Some(project.id);
                self.panel = Panel::None;
                self.send(core::Request::Hello);
            }
            Response::Created(task) => {
                self.new_worktree_override = None;
                self.new_preferences = None;
                self.selected_project = Some(task.project_id.clone());
                // The new task leaves the composer as a chip and lands in the thread feed.
                let label = self
                    .empty_draft
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
                if !self.empty_draft.is_empty() {
                    let draft = std::mem::take(&mut self.empty_draft);
                    self.send(core::Request::EditTask {
                        task_id: task.id.clone(),
                        edit: TaskEdit::Draft(draft),
                    });
                }
                if !self.pending_attachments.is_empty() {
                    let paths = std::mem::take(&mut self.pending_attachments);
                    self.import_attachments(task.id.clone(), paths);
                }
                if std::mem::take(&mut self.creating_task) {
                    self.auto_submit_task = Some(task.id.clone());
                }
                self.input
                    .update(cx, |input, cx| input.set_text(String::new(), cx));
                self.select_task(task.id, cx);
                self.panel = Panel::None;
                self.send(core::Request::Hello);
            }
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
                if changed {
                    self.follow =
                        !older && self.chat_scroll.is_scrolled_to_end().unwrap_or(self.follow);
                    let mut anchor = self.chat_scroll.logical_scroll_top();
                    if let Some(current) = &self.view
                        && let Some(message) = current.messages.get(anchor.item_ix)
                        && let Some(index) = view.messages.iter().position(|m| m.id == message.id)
                    {
                        anchor.item_ix = index;
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
                        if previous_count < view.messages.len() {
                            self.chat_scroll.splice(
                                previous_count..previous_count,
                                view.messages.len() - previous_count,
                            );
                            // A new tool item extends the summary rendered at the group's first
                            // row, and any new item folds the trays that ended the transcript.
                            let row = tool_rows_start(&view.messages, previous_count);
                            self.chat_scroll.remeasure_items(row..previous_count + 1);
                        }
                        for (index, (a, b)) in
                            current.messages.iter().zip(&view.messages).enumerate()
                        {
                            if a.text != b.text
                                || a.complete != b.complete
                                || a.file_edits != b.file_edits
                            {
                                let row = if b.kind == MessageKind::Tool {
                                    tool_rows_start(&view.messages, index)
                                } else {
                                    index
                                };
                                self.chat_scroll.remeasure_items(row..index + 1);
                            }
                        }
                    } else {
                        self.chat_scroll.reset(view.messages.len());
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
                        self.chat_scroll.remeasure_items(row..end);
                        if at_end {
                            self.chat_scroll.scroll_to_end();
                        }
                    }
                }
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
                _ => self.send(core::Request::Hello),
            },
            Response::Error(e) => {
                if matches!(reply.request, core::Request::SetAgentPreferences(_)) {
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
                self.error = Some(e);
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
    }
    fn select_task(&mut self, id: String, cx: &mut Context<Self>) {
        self.agent_popup = None;
        if self.creating_task {
            return;
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
        self.refresh_diff();
        self.refresh_task_changes();
        cx.notify();
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
        settings
    }
    fn open_project(&mut self, _: &OpenProject, _: &mut Window, cx: &mut Context<Self>) {
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
                let _ = this.update(cx, |s, _| s.send(core::Request::OpenProject { path }));
            }
        })
        .detach();
    }
    fn new_task(&mut self, _: &NewTask, window: &mut Window, cx: &mut Context<Self>) {
        self.open_empty_chat(cx);
        window.focus(&self.input.read(cx).focus_handle(cx), cx);
    }
    fn composer_worktree(&self) -> bool {
        self.new_worktree_override
            .unwrap_or(self.settings.default_worktree)
    }
    fn open_empty_chat(&mut self, cx: &mut Context<Self>) {
        if self.creating_task {
            return;
        }
        self.new_worktree_override = None;
        if let Some(view) = &self.view {
            if self.submitted_draft.is_none() {
                self.send(core::Request::EditTask {
                    task_id: view.task.id.clone(),
                    edit: TaskEdit::Draft(self.input.read(cx).text()),
                });
            }
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
        self.readonly_streaming(key, text, false, cx)
    }
    fn readonly_streaming(
        &mut self,
        key: String,
        text: String,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let animate = streaming && !cx.reduce_motion();
        let input = if let Some((input, _)) = self.text_views.get(&key) {
            if input.read(cx).text() != text {
                input.update(cx, |input, cx| input.set_streamed_text(text, animate, cx));
            }
            input.clone()
        } else {
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
    /// Shows a short confirmation that leaves on its own after a few seconds.
    pub(super) fn notify_user(
        &mut self,
        text: impl Into<SharedString>,
        tone: Hsla,
        cx: &mut Context<Self>,
    ) {
        self.toast_sequence += 1;
        let sequence = self.toast_sequence;
        self.toast = Some(Toast {
            text: text.into(),
            tone,
            sequence,
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(3600))
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.toast
                    .as_ref()
                    .is_some_and(|toast| toast.sequence == sequence)
                {
                    s.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    // Marks the moment a task changes hands: a finished turn, a failure, or a request for input
    // lands as an impact on its thread row.
    fn celebrate_transitions(&mut self, incoming: &Workspace, cx: &mut Context<Self>) {
        let Some(current) = &self.workspace else {
            return;
        };
        let t = self.theme;
        for task in &incoming.tasks {
            let Some(previous) = current.tasks.iter().find(|old| old.id == task.id) else {
                continue;
            };
            let tone = if !previous.needs_input() && task.needs_input() {
                Some(t.warning)
            } else if previous.state.active() && task.state == RunState::Completed {
                Some(t.added)
            } else if previous.state.active() && task.is_blocked() {
                Some(t.removed)
            } else {
                None
            };
            if let Some(tone) = tone {
                fx::impact(&sidebar::thread_anchor(&task.id), tone, cx);
            }
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
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(rest))
        } else {
            Some(root.join(path))
        };
        let Some(absolute) = absolute else {
            return;
        };
        if let Ok(relative) = absolute.strip_prefix(root) {
            self.panel = Panel::Files;
            self.send(core::Request::Files {
                task_id: view.task.id.clone(),
            });
            self.send(core::Request::ReadFile {
                task_id: view.task.id.clone(),
                path: relative.to_string_lossy().into_owned().into_bytes(),
            });
        } else {
            cx.open_url(&format!("file://{}", absolute.display()));
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
    pub(super) fn composer_project(&self) -> Option<&Project> {
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
            .font_family(crate::theme::MONO)
            .text_size(px(11.5))
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
            .hover(move |s| {
                s.bg(theme.control_hover)
                    .border_color(theme.accent.opacity(0.5))
            })
            .active(move |s| s.bg(theme.control_active))
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
            .accessibility_label(label)
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: label.into(),
                    theme,
                })
                .into()
            })
    }
    /// Returns the main action of a surface: a solid accent fill with a dithered shade.
    fn primary(
        &self,
        id: impl Into<ElementId>,
        glyph: &'static str,
        label: impl Into<SharedString>,
    ) -> Button {
        let theme = self.theme;
        // An empty label must not become a child: it would take a gap and push the icon of an
        // icon-only button off center.
        let label = label.into();
        self.control(id)
            .overflow_hidden()
            .child(kit::shade(gpui::black().opacity(0.2), dither::Edge::Bottom))
            .child(kit::glint(gpui::white().opacity(0.3), dither::Edge::Top))
            .child(icon(glyph).size(px(13.)))
            .when(!label.is_empty(), |b| b.child(label))
            .bg(theme.accent)
            .border_color(theme.accent)
            .text_color(theme.on_accent)
            .font_weight(FontWeight::SEMIBOLD)
            .hover(move |s| s.bg(theme.accent.opacity(0.9)))
            .active(move |s| s.bg(theme.accent.opacity(0.8)))
            .on_press(move |position, cx| fx::ripple(position, theme.accent, cx))
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
            .bg(tone.opacity(0.09))
            .border_color(tone.opacity(0.45))
            .text_color(tone)
            .hover(move |s| s.bg(tone.opacity(0.16)).border_color(tone.opacity(0.7)))
            .active(move |s| s.bg(tone.opacity(0.22)))
            .on_press(move |position, cx| fx::ripple(position, tone, cx))
    }
}

/// Returns a small uppercase mono label, used for section titles, table headers and captions.
pub(super) fn caps(text: impl AsRef<str>, theme: Theme) -> Div {
    div()
        .flex_shrink_0()
        .font_family(crate::theme::MONO)
        .text_size(px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .letter_spacing(px(crate::theme::LABEL_TRACKING))
        .text_color(theme.subtle)
        .line_height(px(14.))
        .child(text.as_ref().to_uppercase())
}

/// Returns tabular monospace text for counts, durations and other numeric metadata.
pub(super) fn figure(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .font_family(crate::theme::MONO)
        .text_size(px(11.))
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

// Polls refresh the tail; explicit page requests extend the retained prefix.
fn merge_transcript(current: &TaskView, incoming: &mut TaskView, older: bool) {
    if !older && !current.messages.is_empty() {
        incoming.before = current.before;
    }
    let mut messages: std::collections::BTreeMap<_, _> = current
        .messages
        .iter()
        .cloned()
        .map(|message| (message.seq, message))
        .collect();
    for message in incoming.messages.drain(..) {
        messages.insert(message.seq, message);
    }
    incoming.messages = messages.into_values().collect();
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
                    .font_family(crate::theme::MONO)
                    .px(px(8.))
                    .py(px(5.))
                    .text_size(px(11.))
                    .line_height(relative(1.45))
                    .bg(self.theme.raised)
                    .text_color(self.theme.text)
                    .border_1()
                    .border_color(self.theme.edge)
                    .rounded(px(5.))
                    .child(self.text.clone()),
            )
    }
}
/// Installs workspace keyboard commands and macOS menu actions.
pub fn init(cx: &mut App) {
    crate::components::text_input::init(cx);
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenProject, None),
        KeyBinding::new("cmd-n", NewTask, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-d", ToggleReview, None),
        KeyBinding::new("cmd-,", ShowSettings, None),
        KeyBinding::new("cmd-f", Search, None),
        KeyBinding::new("cmd-k", CommandPalette, None),
        KeyBinding::new("cmd-shift-]", NextTask, None),
        KeyBinding::new("cmd-shift-[", PreviousTask, None),
        KeyBinding::new("ctrl-tab", NextTask, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTask, None),
        KeyBinding::new("alt-cmd-a", NextAttention, None),
        KeyBinding::new("cmd-shift-m", ChooseModel, None),
        KeyBinding::new("cmd-shift-e", ChooseEffort, None),
        KeyBinding::new("cmd-shift-a", ChooseAccess, None),
        KeyBinding::new("shift-tab", CycleAccess, None),
        KeyBinding::new("ctrl-`", ToggleTerminal, None),
        KeyBinding::new("cmd-1", GoToTask1, None),
        KeyBinding::new("cmd-2", GoToTask2, None),
        KeyBinding::new("cmd-3", GoToTask3, None),
        KeyBinding::new("cmd-4", GoToTask4, None),
        KeyBinding::new("cmd-5", GoToTask5, None),
        KeyBinding::new("cmd-6", GoToTask6, None),
        KeyBinding::new("cmd-7", GoToTask7, None),
        KeyBinding::new("cmd-8", GoToTask8, None),
        KeyBinding::new("cmd-9", GoToTask9, None),
        KeyBinding::new("escape", Cancel, None),
        KeyBinding::new("cmd-q", Quit, None),
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
                MenuItem::action("New Task", NewTask),
            ],
        },
        Menu {
            disabled: false,
            name: "View".into(),
            items: vec![
                MenuItem::action("Command Palette", CommandPalette),
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
    std::process::Command::new("/usr/bin/id")
        .arg("-F")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .or_else(|| std::env::var("USER").ok())
        .and_then(|name| name.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "there".into())
}
