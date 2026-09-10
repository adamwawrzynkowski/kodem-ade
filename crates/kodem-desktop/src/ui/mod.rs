//! Native workspace state and actions. All blocking work goes through the host bridge.
mod activity;
mod browser;
mod change_tree;
mod chat;
mod chrome;
mod composition;
mod diff_refresh;
mod diff_view;
mod effort_slider;
mod image_preview;
mod menus;
mod message_content;
mod model_picker;
mod motion;
mod panels;
mod review;
mod settings;
mod sidebar;
mod tool_calls;
mod usage;
use crate::icons::icon;
use crate::{
    bridge,
    components::{
        Button,
        text_input::{SubmitInput, TextInput},
    },
    theme::Theme,
};
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
        Latest
    ]
);
#[derive(Clone, PartialEq)]
enum Panel {
    None,
    NewTask,
    Settings,
    Resources,
    Files,
    Check,
    Rename,
    Questions,
    Models,
    Reasoning,
    Access,
    Projects,
    TaskMenu,
    ThreadMenu,
    DeleteTask,
    Sort,
    Comparison,
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
}
pub struct Shell {
    tx: mpsc::Sender<bridge::Command>,
    focus: FocusHandle,
    workspace: Option<Workspace>,
    view: Option<TaskView>,
    selected: Option<String>,
    input: Entity<TextInput>,
    aux: Entity<TextInput>,
    search: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
    settings: Settings,
    theme: Theme,
    panel: Panel,
    thread_menu: Option<ThreadMenu>,
    deleting_task: Option<String>,
    error: Option<String>,
    status: String,
    host_connected: bool,
    new_worktree: bool,
    selected_project: Option<String>,
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
    folded: HashSet<String>,
    collapsed_changes: HashSet<String>,
    text_views: HashMap<String, Entity<TextInput>>,
    chat_selection: Entity<crate::components::text_document::TextDocument>,
    selection_inputs: Option<Vec<Entity<TextInput>>>,
    resources: Vec<core::Resource>,
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
    question_inputs: HashMap<String, Entity<TextInput>>,
    submitted_draft: Option<PendingSubmission>,
    draft_revision: u64,
    observed_draft: String,
    importing_tasks: HashMap<String, usize>,
    pending_attachments: Vec<AttachmentSource>,
    clipboard_images: HashMap<std::path::PathBuf, tempfile::TempPath>,
    pasting_images: usize,
    diff_view: Option<Entity<diff_view::DiffView>>,
    sidebar_hovered: bool,
    task_order: HashMap<String, usize>,
    sidebar_project: Option<String>,
    settled_collapsed: bool,
    empty_draft: String,
    start_after_create: bool,
    auto_submit_task: Option<String>,
    composer_height: f32,
    composer_spring: motion::ComposerSpring,
    settings_section: usize,
    effort_slider: Option<(String, Entity<effort_slider::EffortSlider>, Subscription)>,
    image_preview: Option<image_preview::ImagePreview>,
}
impl Shell {
    /// Creates the workspace and begins asynchronous host synchronization.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.set_reduce_motion(motion::system_reduce_motion());
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let (tx, rx) = bridge::start();
        let input =
            cx.new(|cx| TextInput::composer("Describe what you want to build or change…", cx));
        let aux = cx.new(|cx| TextInput::new("Type here…", cx));
        let search = cx.new(|cx| TextInput::new("Search tasks…", cx));
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
            cx.subscribe(&input, |s, _, _: &SubmitInput, cx| s.submit_current(cx)),
            cx.subscribe(
                &input,
                |s, _, pasted: &crate::clipboard::PastedAttachments, cx| {
                    s.paste_attachments(pasted, cx);
                },
            ),
            cx.observe(&input, |s, input, cx| {
                let text = input.read(cx).text();
                if text != s.observed_draft {
                    s.draft_revision += 1;
                    s.observed_draft = text;
                    s.draft_dirty = true;
                    s.nudge_composer(-0.35, cx);
                }
                cx.notify();
            }),
            cx.observe(&search, |_, _, cx| cx.notify()),
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
                            let _ = s
                                .tx
                                .send(bridge::Command::Poll(s.selected.clone(), s.show_terminal));
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
        let shell = Self {
            tx,
            focus,
            workspace: None,
            view: None,
            selected: None,
            input,
            aux,
            search,
            _subscriptions: subscriptions,
            settings: Settings::default(),
            theme: Theme::new(false),
            panel: Panel::None,
            thread_menu: None,
            deleting_task: None,
            error: None,
            status: "Connecting to local host…".into(),
            host_connected: false,
            new_worktree: true,
            selected_project: None,
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
            folded: HashSet::new(),
            collapsed_changes: HashSet::new(),
            text_views: HashMap::new(),
            chat_selection: cx.new(|_| Default::default()),
            selection_inputs: None,
            resources: Vec::new(),
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
            submitted_draft: None,
            draft_revision: 0,
            observed_draft: String::new(),
            importing_tasks: HashMap::new(),
            pending_attachments: Vec::new(),
            clipboard_images: HashMap::new(),
            pasting_images: 0,
            diff_view: None,
            sidebar_hovered: false,
            task_order: HashMap::new(),
            sidebar_project: None,
            settled_collapsed: false,
            empty_draft: String::new(),
            start_after_create: false,
            auto_submit_task: None,
            composer_height: 170.,
            composer_spring: motion::ComposerSpring::default(),
            settings_section: 0,
            effort_slider: None,
            image_preview: None,
        };
        shell.send(core::Request::Hello);
        shell
    }
    fn send(&self, request: core::Request) {
        let _ = self.tx.send(bridge::Command::Request(request));
    }
    fn apply(&mut self, reply: bridge::Reply, cx: &mut Context<Self>) {
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
        if !reply.connected {
            self.status = "Local host unavailable; reconnecting…".into();
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

        self.poll_pending = false;
        let response = match reply.result {
            Ok(r) => r,
            Err(e) => {
                if matches!(reply.request, core::Request::DeleteTask { .. }) {
                    self.deleting_task = None;
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
                    self.start_after_create = false;
                }
                if matches!(reply.request, core::Request::ImportAttachments { .. }) {
                    self.auto_submit_task = None;
                }
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
        match response {
            Response::Workspace(workspace) => {
                cx.set_reduce_motion(motion::system_reduce_motion());
                if self.workspace.is_none() {
                    self.settings = workspace.settings.clone();
                    self.show_review = self.settings.show_review;
                    self.show_terminal = self.settings.show_terminal;
                    self.split = self.settings.split_diff;
                    self.theme = Theme::new(self.settings.light);
                    let selected = self
                        .settings
                        .selected_task
                        .clone()
                        .filter(|id| workspace.tasks.iter().any(|t| &t.id == id));
                    self.workspace = Some(workspace);
                    if let Some(id) = selected {
                        self.select_task(id, cx)
                    } else {
                        self.status = "Open a repository to start coding".into();
                    }
                    if self
                        .workspace
                        .as_ref()
                        .is_some_and(|w| w.runtime.models.is_empty())
                    {
                        self.send(core::Request::ConnectRuntime);
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
                self.workspace = Some(workspace);
                self.status = "Local host connected".into();
            }
            Response::Project(project) => {
                self.selected_project = Some(project.id);
                self.panel = Panel::NewTask;
                self.send(core::Request::Hello);
            }
            Response::Created(task) => {
                self.selected_project = Some(task.project_id.clone());
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
                if self.start_after_create {
                    self.auto_submit_task = Some(task.id.clone());
                    self.start_after_create = false;
                }
                self.select_task(task.id, cx);
                self.panel = Panel::None;
                self.send(core::Request::Hello);
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
                            a.id != b.id || a.text != b.text || a.complete != b.complete
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
                    self.chat_scroll.reset(view.messages.len());
                    if self.follow {
                        self.chat_scroll.scroll_to_end();
                    } else {
                        self.chat_scroll.scroll_to(anchor);
                    }
                }
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
                self.status = "Message accepted".into();
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
                self.send(core::Request::Task {
                    task_id,
                    before: None,
                });
            }
            Response::Resources(resources) => self.resources = resources,
            Response::Files(files) => self.files = files,
            Response::Text(text) => {
                if let core::Request::ReadFile { path, .. } = reply.request {
                    self.file_text = Some((String::from_utf8_lossy(&path).into(), text));
                }
            }
            Response::Terminal(terminal) => self.terminal = Some(terminal),
            Response::Ok => match reply.request {
                core::Request::DeleteTask { task_id } => {
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
                core::Request::Submit { .. } => {
                    self.status = "Prompt recorded; awaiting provider acknowledgement".into();
                }
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
                core::Request::StageFile { .. } => self.refresh_diff(),
                core::Request::EditTask {
                    task_id,
                    edit: TaskEdit::Model { .. } | TaskEdit::Access(_),
                } => {
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
        self.selected = None;
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
        self.auto_submit_task = None;
    }
    fn select_task(&mut self, id: String, cx: &mut Context<Self>) {
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
        self.selected = Some(id.clone());
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
        self.file = 0;
        self.reviewing_file = false;
        self.terminal = None;
        self.file_text = None;
        self.settings.selected_task = Some(id.clone());
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
        let mut settings = self.settings.clone();
        settings.show_review = self.show_review;
        settings.show_terminal = self.show_terminal;
        settings.split_diff = self.split;
        self.send(core::Request::SaveSettings(settings));
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
    fn new_task(&mut self, _: &NewTask, _: &mut Window, cx: &mut Context<Self>) {
        self.panel = Panel::NewTask;
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
        let input = if let Some(input) = self.text_views.get(&key) {
            if input.read(cx).text() != text {
                input.update(cx, |input, cx| input.set_text(text, cx));
            }
            input.clone()
        } else {
            let input = cx.new(|cx| TextInput::readonly(text, cx));
            self.text_views.insert(key, input.clone());
            input
        };
        if let Some(inputs) = &mut self.selection_inputs {
            inputs.push(input.clone());
        }
        input
    }
    fn control(&self, id: impl Into<ElementId>) -> Button {
        let theme = self.theme;
        Button::new(id)
            .h(px(28.))
            .px(px(9.))
            .gap(px(6.))
            .rounded(px(6.))
            .bg(theme.raised)
            .shadow(theme.elevation())
            .border_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .text_size(px(12.))
            .font_weight(FontWeight::NORMAL)
            .cursor_pointer()
            .styles(|s| s.disabled(|s| s.opacity(0.4)))
            .focus(move |s| s.border_color(theme.accent))
    }
    fn button(
        &self,
        id: impl Into<ElementId>,
        glyph: &'static str,
        label: impl Into<SharedString>,
    ) -> Button {
        let theme = self.theme;
        self.control(id)
            .child(icon(glyph))
            .child(label.into())
            .hover(move |s| s.bg(theme.selected))
            .active(move |s| s.bg(theme.inset))
    }
    fn ghost(
        &self,
        id: impl Into<ElementId>,
        name: &'static str,
        label: impl Into<SharedString>,
    ) -> Button {
        let theme = self.theme;
        let label = label.into();
        self.control(id)
            .child(icon(name))
            .when(!label.is_empty(), |b| b.child(label))
            .border_0()
            .shadow(Vec::new())
            .bg(hsla(0., 0., 0., 0.))
            .text_color(theme.muted)
            .hover(move |s| s.bg(theme.raised).text_color(theme.text))
    }
    fn icon_button(
        &self,
        id: impl Into<ElementId>,
        name: &'static str,
        label: &'static str,
    ) -> Button {
        let theme = self.theme;
        self.ghost(id, name, "")
            .w(px(28.))
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
    fn primary(
        &self,
        id: impl Into<ElementId>,
        glyph: &'static str,
        label: impl Into<SharedString>,
    ) -> Button {
        let theme = self.theme;
        self.control(id)
            .child(icon(glyph))
            .child(label.into())
            .bg(theme.accent)
            .text_color(theme.inset)
            .border_color(theme.accent)
            .hover(move |s| s.opacity(0.88))
            .active(move |s| s.opacity(0.72))
    }
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
            .font_family("IBM Plex Sans")
            .max_w(px(360.))
            .px_2()
            .py_1()
            .text_size(px(12.))
            .bg(self.theme.raised)
            .text_color(self.theme.text)
            .border_1()
            .border_color(self.theme.border)
            .rounded(px(5.))
            .shadow_sm()
            .child(self.text.clone())
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
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::action("Toggle Changes", ToggleReview),
                MenuItem::action("Jump to Latest", Latest),
            ],
        },
    ]);
}
