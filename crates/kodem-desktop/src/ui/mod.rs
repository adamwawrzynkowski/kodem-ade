//! Native workspace state and actions. All blocking work goes through the host bridge.
mod activity;
mod browser;
mod chat;
mod chrome;
mod menus;
mod motion;
mod panels;
mod review;
mod settings;
mod sidebar;
use crate::icons::icon;
use crate::{
    bridge,
    components::{Button, text_input::TextInput},
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
    Feedback,
    Rename,
    Questions,
    Models,
    TaskMenu,
    Sort,
    Comparison,
}
#[derive(Clone)]
struct Selection {
    side: Side,
    start: u32,
    end: u32,
    dragging: bool,
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
    error: Option<String>,
    status: String,
    new_worktree: bool,
    selected_project: Option<String>,
    show_review: bool,
    show_terminal: bool,
    terminal: Option<TerminalView>,
    terminal_focus: FocusHandle,
    diff: Option<DiffSnapshot>,
    new_diff: Option<DiffSnapshot>,
    file: usize,
    reviewing_file: bool,
    search_visible: bool,
    comparison: Comparison,
    selection: Option<Selection>,
    split: bool,
    diff_scroll: UniformListScrollHandle,
    chat_scroll: ListState,
    follow: bool,
    folded: HashSet<String>,
    text_views: HashMap<String, Entity<TextInput>>,
    resources: Vec<core::Resource>,
    files: Vec<Vec<u8>>,
    file_text: Option<(String, String)>,
    dragging_sidebar: bool,
    dragging_review: bool,
    window_width: f32,
    show_archived: bool,
    poll_pending: bool,
    draft_dirty: bool,
    question_inputs: HashMap<String, Entity<TextInput>>,
    submitted_draft: Option<(String, String)>,
    sidebar_hovered: bool,
    task_order: HashMap<String, usize>,
    collapsed_projects: HashSet<String>,
    empty_draft: String,
    settings_section: usize,
}
impl Shell {
    /// Creates the workspace and begins asynchronous host synchronization.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.set_reduce_motion(motion::system_reduce_motion());
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let (tx, rx) = bridge::start();
        let input = cx.new(|cx| TextInput::new("Describe what you want to build or change…", cx));
        let aux = cx.new(|cx| TextInput::new("Type here…", cx));
        let search = cx.new(|cx| TextInput::new("Search tasks…", cx));
        let subscriptions = vec![
            cx.on_focus_lost(window, |s, window, cx| window.focus(&s.focus, cx)),
            cx.observe(&input, |s, _, cx| {
                s.draft_dirty = true;
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
            error: None,
            status: "Connecting to local host…".into(),
            new_worktree: true,
            selected_project: None,
            show_review: true,
            show_terminal: false,
            terminal: None,
            terminal_focus: cx.focus_handle(),
            diff: None,
            new_diff: None,
            file: 0,
            reviewing_file: false,
            search_visible: false,
            comparison: Comparison::WorkingTree,
            selection: None,
            split: false,
            diff_scroll: UniformListScrollHandle::new(),
            chat_scroll: ListState::new(0, ListAlignment::Bottom, px(600.)),
            follow: true,
            folded: HashSet::new(),
            text_views: HashMap::new(),
            resources: Vec::new(),
            files: Vec::new(),
            file_text: None,
            dragging_sidebar: false,
            dragging_review: false,
            window_width: 1440.,
            show_archived: false,
            poll_pending: false,
            draft_dirty: false,
            question_inputs: HashMap::new(),
            submitted_draft: None,
            sidebar_hovered: false,
            task_order: HashMap::new(),
            collapsed_projects: HashSet::new(),
            empty_draft: String::new(),
            settings_section: 0,
        };
        shell.send(core::Request::Hello);
        shell
    }
    fn send(&self, request: core::Request) {
        let _ = self.tx.send(bridge::Command::Request(request));
    }
    fn apply(&mut self, reply: bridge::Reply, cx: &mut Context<Self>) {
        let response_task = match &reply.request {
            core::Request::Task { task_id, .. }
            | core::Request::ReadFile { task_id, .. }
            | core::Request::Files { task_id }
            | core::Request::Terminal { task_id } => Some(task_id),
            _ => None,
        };
        if response_task.is_some_and(|id| self.selected.as_ref() != Some(id)) {
            return;
        }

        self.poll_pending = false;
        let response = match reply.result {
            Ok(r) => r,
            Err(e) => {
                if matches!(reply.request, core::Request::Submit { .. }) {
                    self.submitted_draft = None;
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
                    return;
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
                self.select_task(task.id, cx);
                self.panel = Panel::None;
                self.send(core::Request::Hello);
            }
            Response::Task(mut view) => {
                if self.selected.as_deref() != Some(&view.task.id) {
                    return;
                }
                if let Some((id, text)) = &self.submitted_draft
                    && id == &view.task.id
                    && view.task.draft.is_empty()
                    && !matches!(
                        view.task.state,
                        RunState::Idle
                            | RunState::Starting
                            | RunState::DeliveryUnknown
                            | RunState::Failed
                    )
                {
                    if self.input.read(cx).text() == *text {
                        self.input.update(cx, |i, cx| i.set_text(String::new(), cx));
                    }
                    self.submitted_draft = None;
                }
                if matches!(
                    view.task.state,
                    RunState::Failed | RunState::DeliveryUnknown
                ) {
                    self.submitted_draft = None;
                }
                let initial = self.view.as_ref().is_none_or(|v| v.task.id != view.task.id);
                if initial {
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
            }
            Response::Diff(diff) => {
                if self.selected.as_deref() != Some(&diff.task_id) {
                    return;
                }
                if self.selection.is_some() && self.diff.as_ref().is_some_and(|d| d.id != diff.id) {
                    self.new_diff = Some(diff);
                } else {
                    let selected_label = self
                        .diff
                        .as_ref()
                        .and_then(|d| d.files.get(self.file))
                        .map(|f| &f.label);
                    if self.reviewing_file {
                        if let Some(index) = diff
                            .files
                            .iter()
                            .position(|f| Some(&f.label) == selected_label)
                        {
                            self.file = index;
                        } else {
                            self.reviewing_file = false;
                            self.file = 0;
                        }
                    }
                    self.diff = Some(diff);
                    self.text_views.retain(|k, _| !k.starts_with("diff:"));
                }
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
                core::Request::Submit { .. } => {
                    self.status = "Prompt recorded; awaiting provider acknowledgement".into();
                }
                core::Request::AddFeedback { .. } => {
                    self.selection = None;
                    self.panel = Panel::None;
                }
                core::Request::StageFile { .. } => self.refresh_diff(),
                core::Request::EditTask { .. }
                | core::Request::SaveSettings(_)
                | core::Request::TerminalInput { .. }
                | core::Request::TerminalResize { .. } => {}
                core::Request::Shutdown => cx.quit(),
                _ => self.send(core::Request::Hello),
            },
            Response::Error(e) => self.error = Some(e),
        }
        cx.notify();
    }
    fn select_task(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(task_id) = &self.selected
            && self.view.is_some()
            && self.submitted_draft.is_none()
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
        if self.window_width <= 1100. {
            self.show_review = false;
        }
        self.view = None;
        self.diff = None;
        self.new_diff = None;
        self.selection = None;
        self.text_views.clear();
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
        cx.notify();
    }
    fn refresh_diff(&self) {
        if let Some(task_id) = &self.selected {
            self.send(core::Request::CaptureDiff {
                task_id: task_id.clone(),
                comparison: self.comparison.clone(),
            });
        }
    }
    fn save_settings(&self) {
        self.send(core::Request::SaveSettings(self.settings.clone()));
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
        if self.panel != Panel::None || self.input.read(cx).text().trim().is_empty() {
            return;
        }
        if let Some(view) = &self.view {
            if view.task.state.active() {
                self.error =
                    Some("Wait for this turn to finish, or stop it before sending.".into());
                return;
            }
            if self.submitted_draft.is_some() {
                return;
            }
            self.submitted_draft = Some((view.task.id.clone(), self.input.read(cx).text()));
            self.send(core::Request::Submit {
                task_id: view.task.id.clone(),
                submission_id: new_id(),
                text: self.input.read(cx).text(),
                feedback_ids: view.task.feedback.iter().map(|f| f.id.clone()).collect(),
            });
        }
    }
    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel == Panel::None && self.search_visible {
            self.search_visible = false;
            self.search
                .update(cx, |input, cx| input.set_text(String::new(), cx));
            window.focus(&self.focus, cx);
        }
        self.panel = Panel::None;
        self.selection = None;
        self.dragging_sidebar = false;
        self.dragging_review = false;
        cx.notify();
    }
    fn readonly(&mut self, key: String, text: String, cx: &mut Context<Self>) -> Entity<TextInput> {
        if let Some(input) = self.text_views.get(&key) {
            if input.read(cx).text() != text {
                input.update(cx, |input, cx| input.set_text(text, cx));
            }
            return input.clone();
        }
        if self.text_views.len() > 400 {
            self.text_views.clear();
        }
        let input = cx.new(|cx| TextInput::readonly(text, cx));
        self.text_views.insert(key, input.clone());
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
        KeyBinding::new("cmd-enter", SendPrompt, None),
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
