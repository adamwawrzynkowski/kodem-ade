//! Native workspace state and actions. All blocking work goes through the host bridge.
mod accounts;
mod activity;
mod backdrop;
mod browser;
mod censor;
mod change_tree;
mod channel_art;
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
mod file_edit;
mod fx;
mod git;
mod harness_updates;
mod history;
mod image_preview;
mod kit;
mod menus;
mod message_content;
mod model_picker;
mod motion;
mod notices;
mod onboarding;
mod overview_ruler;
mod palette;
mod panels;
mod project_commands;
mod projects;
mod questions;
mod review;
mod settings;
mod sidebar;
mod skater;
mod slash;
mod subagents;
mod thread_import;
mod tool_calls;
mod updates;
mod usage;
mod web_pane;
mod whats_new;
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
use futures::StreamExt;
use gpui::{prelude::*, *};
use kit::Layers;
use kodem_core::{self as core, *};
use std::{
    collections::{HashMap, HashSet, VecDeque},
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
        ToggleBrowser,
        ToggleSidebar,
        ShowSettings,
        Search,
        Cancel,
        Quit,
        CloseWindow,
        Hide,
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
    ProjectCommands,
    ThreadMenu,
    /// The menu of a project group in the thread feed.
    ProjectMenu,
    SelectionMenu,
    /// The menu of the chat background, which leads to its wallpaper.
    BackdropMenu,
    DeleteTask,
    Sort,
    Comparison,
    Usage,
    /// The steps of the agent's plan, above the composer's plan tab.
    Plan,
    Palette,
    Mode,
    WhatsNew,
    Notifications,
    /// Imports the conversations of Claude Code and Codex.
    ImportThreads,
    /// Asks before Kodem quits.
    Quit,
}
/// What the side pane shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanePage {
    /// The choice of what the pane shows, which it offers when the user opens it.
    Picker,
    /// The changes of the task.
    Changes,
    /// The task's browser, which its agents use too.
    Browser,
}
struct ThreadMenu {
    task_id: String,
    position: Point<Pixels>,
}
struct ProjectMenu {
    project_id: String,
    /// How many tasks the project's group lists, which its fold moves.
    tasks: usize,
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
    /// The installed and newest release of each provider's CLI and the updates of the card.
    harness_updates: harness_updates::HarnessUpdates,
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
    /// The import dialog's threads, choice and progress, which outlive the dialog.
    thread_import: thread_import::ThreadImport,
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
    project_menu: Option<ProjectMenu>,
    /// Where the pointer opened the menu of the conversation's text selection.
    selection_menu: Option<Point<Pixels>>,
    /// Where the pointer opened the menu of the chat background.
    backdrop_menu: Option<Point<Pixels>>,
    /// The scroll of the Theme page in preferences, which the background menu moves to the
    /// wallpaper at the end of the page.
    theme_scroll: ScrollHandle,
    deleting_task: Option<String>,
    error: Option<String>,
    notices: notices::Notices,
    host_connected: bool,
    new_worktree_override: Option<bool>,
    new_preferences: Option<AgentPreferences>,
    /// The approval rule that returns when a task or the new-task draft leaves plan mode,
    /// keyed by task id; the draft uses an empty key.
    approval_memory: HashMap<String, String>,
    // Project chosen for the next task, independent of the currently viewed thread.
    selected_project: Option<String>,
    editing_project: Option<Project>,
    // Name fields of the edited project's commands, by command id.
    command_names: HashMap<String, Entity<TextInput>>,
    // Folder without Git that waits for the user to choose how it opens.
    folder_without_git: Option<PathBuf>,
    // Narrows the built-in icons in the project editor by name.
    icon_filter: Entity<TextInput>,
    greeting_name: String,
    show_review: bool,
    /// What the side pane shows while it is open.
    pane_page: PanePage,
    /// The pages of the tasks' browsers. Shared with the pane's paint, which places them.
    browsers: std::rc::Rc<std::cell::RefCell<crate::browser::Browsers>>,
    /// The address field of the browser pane.
    browser_address: Entity<TextInput>,
    /// The task and the address that the address field shows, until the user edits it.
    browser_address_url: Option<(String, String)>,
    /// The task whose page the pane shows in the frame being built.
    browser_frame: Option<String>,
    /// The tasks whose browser opened its home page, so that a failed load is not tried again
    /// on each frame.
    browser_home: HashSet<String>,
    /// The window's focus in the last frame. A move to another of its elements takes the
    /// keyboard back from a page.
    window_focus: Option<FocusHandle>,
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
    /// Whether the jump to the latest message was on screen in the last frame. The list stops
    /// and resumes following its end during layout, after the frame rendered the button.
    latest_shown: bool,
    /// Something arrived below while the user was scrolled away from the end.
    unseen_below: bool,
    /// Sequence number of the sent message whose history tick is under the pointer.
    history_hover: Option<u64>,
    folded: HashSet<String>,
    /// Long messages of the user that show their text instead of only their card.
    unfolded_texts: HashSet<String>,
    /// Tool runs and thoughts the user opened or folded by hand; the others follow their work.
    tray_choices: HashMap<String, bool>,
    /// The stretches of tool work that this frame's rows read, by conversation and row. Each is
    /// built once per frame instead of once per row; a render starts the map again.
    work_stretches: HashMap<(Conversation, usize), std::rc::Rc<tool_calls::Stretch>>,
    /// The trays, runs and calls of the work trail that open, fold or grow into place.
    folds: motion::Folds,
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
    /// A transcript reference that waits for the checkout's file list to find its file.
    pending_reference: Option<String>,
    dragging_sidebar: bool,
    dragging_review: bool,
    /// A press on a title bar that the app draws itself, which moves the window once the
    /// pointer moves. A press that ends in place stays a click.
    title_press: bool,
    // The pointer is over the meter tab on the composer, which paints its own hover fill.
    meter_tab_hovered: bool,
    meter_tab_focus: FocusHandle,
    // The same for the plan tab on the composer's other corner.
    plan_tab_hovered: bool,
    plan_tab_focus: FocusHandle,
    /// The versions of Git, Git LFS and GitHub CLI that the host read last. Empty until it
    /// answers.
    tools: Vec<ToolVersion>,
    /// Tools whose installer runs in a terminal window and that were not found since.
    tool_installers: Vec<Tool>,
    /// The release notes wait for the first workspace, because this version runs for the first
    /// time after an update.
    whats_new_due: bool,
    /// The commits of this release are unfolded under the board of What's new.
    whats_new_changes: bool,
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
    pending_attachments: Vec<composition::PendingAttachment>,
    clipboard_images: HashMap<std::path::PathBuf, tempfile::TempPath>,
    pasting_images: usize,
    diff_view: Option<Entity<diff_view::DiffView>>,
    /// Unsaved edits of working-tree files, by task, repository and path.
    edits: HashMap<file_edit::EditKey, Entity<file_edit::FileEdit>>,
    /// The file whose sides the host is reading so that its diff can be edited, with the
    /// comparison and the capture's digest of the file that it is read for.
    opening_edit: Option<(file_edit::EditKey, Comparison, String)>,
    /// The file, and its capture digest, that could not be read for editing.
    failed_edit: Option<(file_edit::EditKey, String)>,
    sidebar_rows: Vec<crate::thread_list::Row>,
    sidebar_scroll: ListState,
    sidebar_dirty: bool,
    sidebar_project: Option<String>,
    settled_rows: Vec<crate::thread_list::Row>,
    settled_collapsed: bool,
    empty_draft: String,
    creating_task: bool,
    auto_submit_task: Option<String>,
    /// The start screen's composer moving to the bottom of the task it creates.
    launch: Option<motion::Launch>,
    /// Rows of the thread feed that open because their tasks were just created.
    entering_tasks: HashMap<String, motion::Entrance>,
    feed_slide: sidebar::FeedSlide,
    group_folds: HashMap<String, sidebar::GroupFold>,
    /// The project group that a drag carries over the feed.
    feed_drag: Option<sidebar::FeedDrag>,
    settled_fold: Option<sidebar::GroupFold>,
    /// How far each row of a folding project group is open in this frame, by task.
    fold_rows: HashMap<String, f32>,
    composer_height: f32,
    /// Height below the floating composer's box, at rest: its hints and its margin, in window
    /// pixels. The conversation ends at the box, so it passes behind the box's glass only.
    composer_tail: f32,
    composer_spring: motion::ComposerSpring,
    reveals: motion::Reveals,
    rail_slide: workspaces::RailSlide,
    settings_page: settings::SettingsPage,
    effort_slider: Option<(String, Entity<effort_slider::EffortSlider>, Subscription)>,
    image_preview: Option<image_preview::ImagePreview>,
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
        let (tx, mut rx) = bridge::start();
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
        let browser_address = cx.new(|cx| {
            let mut input = TextInput::new("Address or file path", cx);
            input.set_single_line(true);
            // A long address stays on its line and scrolls with the caret.
            input.set_wrap(false);
            input
        });
        // The red traffic light asks like Command-W. A windowless macOS app would stay as a
        // background activity, which keeps an update from relaunching it.
        #[cfg(target_os = "macos")]
        {
            let shell = cx.weak_entity();
            window.on_window_should_close(cx, move |window, cx| {
                shell
                    .update(cx, |s, cx| s.confirm_quit(window, cx))
                    .is_err()
            });
        }
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
            cx.subscribe_in(
                &browser_address,
                window,
                |s, _, _: &SubmitInput, window, cx| s.browser_submit(window, cx),
            ),
        ];
        // Agents reach the browser through the window's socket; their jobs and the changes of
        // the pages run on the window's thread, which owns the pages.
        let (browser_jobs, mut jobs) = futures::channel::mpsc::unbounded();
        let (browser_events, mut events) = futures::channel::mpsc::unbounded();
        crate::browser::serve(browser_jobs);
        cx.spawn_in(window, async move |this, cx| {
            while let Some(job) = jobs.next().await {
                if this
                    .update_in(cx, |s, window, cx| s.browser_job(job, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = events.next().await {
                if this
                    .update_in(cx, |s, window, cx| s.browser_event(event, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        // GPUI does not run GTK's loop, which Linux's pages need to draw, load and answer, so
        // its waiting events run here, often enough for smooth scrolling.
        #[cfg(not(any(target_os = "macos", windows)))]
        cx.spawn(async move |this, cx| {
            while this.upgrade().is_some() {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                crate::browser::pump();
            }
        })
        .detach();
        // Each host reply applies as it arrives, together with any that came with it.
        cx.spawn(async move |this, cx| {
            while let Some(reply) = rx.next().await {
                let mut updates = vec![reply];
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
                    .timer(Duration::from_millis(100))
                    .await;
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
                        s.receive_harness_events(cx);
                        if s.harness_updates.due() {
                            s.check_harness_updates(cx);
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
                            && ((s.show_review && s.pane_page == PanePage::Changes)
                                || s.reviewing_file)
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
            harness_updates: Default::default(),
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
            thread_import: Default::default(),
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
            project_menu: None,
            selection_menu: None,
            backdrop_menu: None,
            theme_scroll: ScrollHandle::new(),
            deleting_task: None,
            error: None,
            notices: notices::Notices::default(),
            host_connected: false,
            new_worktree_override: None,
            new_preferences: None,
            approval_memory: HashMap::new(),
            selected_project: None,
            editing_project: None,
            command_names: HashMap::new(),
            folder_without_git: None,
            icon_filter,
            greeting_name: user_first_name(),
            show_review: true,
            pane_page: PanePage::Changes,
            browsers: std::rc::Rc::new(std::cell::RefCell::new(crate::browser::Browsers::new(
                browser_events,
            ))),
            browser_address,
            browser_address_url: None,
            browser_frame: None,
            browser_home: HashSet::new(),
            window_focus: None,
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
            chat_scroll: {
                let list = ListState::new(0, ListAlignment::Bottom, px(600.));
                list.set_follow_mode(FollowMode::Tail);
                list
            },
            chat_rows: Default::default(),
            latest_shown: false,
            unseen_below: false,
            history_hover: None,
            folded: HashSet::new(),
            unfolded_texts: HashSet::new(),
            tray_choices: HashMap::new(),
            work_stretches: HashMap::new(),
            folds: motion::Folds::default(),
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
            pending_reference: None,
            dragging_sidebar: false,
            dragging_review: false,
            title_press: false,
            meter_tab_hovered: false,
            meter_tab_focus: cx.focus_handle(),
            plan_tab_hovered: false,
            plan_tab_focus: cx.focus_handle(),
            tools: Vec::new(),
            tool_installers: Vec::new(),
            whats_new_due: crate::release_notes::take_unseen(),
            whats_new_changes: false,
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
            edits: HashMap::new(),
            opening_edit: None,
            failed_edit: None,
            sidebar_rows: Vec::new(),
            sidebar_scroll: ListState::new(0, ListAlignment::Top, px(100.)),
            sidebar_dirty: true,
            sidebar_project: None,
            settled_rows: Vec::new(),
            settled_collapsed: true,
            empty_draft: String::new(),
            creating_task: false,
            auto_submit_task: None,
            launch: None,
            entering_tasks: HashMap::new(),
            feed_slide: sidebar::FeedSlide::default(),
            group_folds: HashMap::new(),
            feed_drag: None,
            settled_fold: None,
            fold_rows: HashMap::new(),
            composer_height: 170.,
            composer_tail: 0.,
            composer_spring: motion::ComposerSpring::default(),
            reveals: motion::Reveals::default(),
            rail_slide: workspaces::RailSlide::default(),
            settings_page: settings::SettingsPage::default(),
            effort_slider: None,
            image_preview: None,
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
        // The import dialog reports its own failures, in the row of each thread.
        if matches!(reply.request, core::Request::ExternalThreads) {
            self.receive_external_threads(reply.result);
            cx.notify();
            return;
        }
        if let core::Request::ImportThread {
            provider, thread, ..
        } = &reply.request
        {
            let key = ExternalThread::key_of(*provider, thread);
            self.receive_imported_thread(key, reply.result);
            cx.notify();
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
                if self.edit_failed(&reply.request, cx) {
                    cx.notify();
                    return;
                }
                if matches!(
                    reply.request,
                    core::Request::CreateTask { .. }
                        | core::Request::CreateChat { .. }
                        | core::Request::CreateReview { .. }
                ) {
                    self.creating_task = false;
                    self.launch_draft = None;
                    // The composer returns to the start screen with its draft.
                    self.launch = None;
                }
                if matches!(reply.request, core::Request::ImportAttachments { .. }) {
                    self.auto_submit_task = None;
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
                // A deleted task's page closes with it.
                self.browsers
                    .borrow_mut()
                    .retain(|id| workspace.tasks.iter().any(|task| task.id == id));
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
                if !draft.is_empty() {
                    self.send(core::Request::EditTask {
                        task_id: task.id.clone(),
                        edit: TaskEdit::Draft(draft),
                    });
                }
                if from_composer && !self.pending_attachments.is_empty() {
                    let sources = std::mem::take(&mut self.pending_attachments)
                        .into_iter()
                        .map(|pending| pending.source)
                        .collect();
                    self.import_attachments(task.id.clone(), sources);
                }
                if submit {
                    self.auto_submit_task = Some(task.id.clone());
                }
                // A draft of the composer stays in it until the task's conversation loads it
                // again, so the composer never shows empty on its way.
                if !from_composer {
                    self.input
                        .update(cx, |input, cx| input.set_text(String::new(), cx));
                }
                // The task's row opens in the thread feed as soon as the workspace lists it.
                self.entering_tasks
                    .insert(task.id.clone(), motion::Entrance::new());
                self.send(core::Request::Hello);
                self.panel = Panel::None;
                match &mut self.launch {
                    Some(launch) => {
                        launch.task = Some(task.id);
                        self.finish_launch(cx);
                    }
                    None => self.select_task(task.id, cx),
                }
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
                // Whether each row held at the same place changed, compared once: a long
                // transcript holds megabytes of text.
                let edited: Vec<bool> = self
                    .view
                    .as_ref()
                    .map(|current| {
                        current
                            .messages
                            .iter()
                            .zip(&view.messages)
                            .map(|(a, b)| {
                                a.text != b.text
                                    || a.complete != b.complete
                                    || a.file_edits != b.file_edits
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let changed = self.view.as_ref().is_none_or(|v| {
                    v.messages.len() != view.messages.len()
                        || v.messages
                            .iter()
                            .zip(&view.messages)
                            .zip(&edited)
                            .any(|((a, b), edited)| a.id != b.id || *edited)
                });
                let rows = conversation::ChatRows::new(
                    &view.messages,
                    self.view
                        .as_ref()
                        .map(|current| (current.messages.as_slice(), &self.chat_rows)),
                );
                let opened = self.view.as_ref().is_none_or(|v| v.task.id != view.task.id);
                if changed {
                    // The list follows its end by itself until the user scrolls away from it,
                    // and an update never scrolls it. A task that opens starts at its end;
                    // earlier messages arrive above the place that the user reads.
                    let following = !older && (opened || self.chat_scroll.is_following_tail());
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
                        for (index, (_, b)) in current
                            .messages
                            .iter()
                            .zip(&view.messages)
                            .enumerate()
                            .rev()
                        {
                            let old = self.chat_rows.rows(index..index + 1);
                            let new = rows.rows(index..index + 1);
                            if old.len() != new.len() {
                                let kept = old.len().min(new.len());
                                self.chat_scroll
                                    .remeasure_items(old.start..old.start + kept);
                                self.chat_scroll
                                    .splice(old.start + kept..old.end, new.len() - kept);
                            } else if edited[index] && b.kind != MessageKind::Tool {
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
                        for (index, (_, b)) in
                            current.messages.iter().zip(&view.messages).enumerate()
                        {
                            if b.kind == MessageKind::Tool && edited[index] {
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
                    if opened {
                        self.follow_latest();
                    } else if !following {
                        self.chat_scroll.scroll_to(anchor);
                        if !older {
                            self.unseen_below = true;
                        }
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
                        // A following list keeps its end in view and any other keeps its place.
                        self.chat_scroll.remeasure_items(rows.rows(row..end));
                    }
                }
                // Drag selection orders the rows as the list does.
                let orders = (0..rows.count())
                    .filter_map(|row| {
                        let (index, part) = rows.locate(row)?;
                        Some((view.messages.get(index)?.seq, part))
                    })
                    .collect();
                let scroll = self.chat_scroll.clone();
                self.chat_selection
                    .update(cx, |doc, _| doc.set_viewport(scroll, orders, px(8.)));
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
                    self.install_diff(diff, cx);
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
                        self.follow_latest();
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
                if let Some(reference) = self.pending_reference.take() {
                    self.open_listed_reference(&reference, cx);
                }
            }
            Response::EditableFile(file) => self.edit_opened(file, cx),
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
            // The import dialog takes these answers before the others.
            Response::ExternalThreads(_) | Response::Imported { .. } => {}
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
                ref request @ core::Request::SaveFile { .. } => self.edit_saved(request, true, cx),
                core::Request::EditTask {
                    task_id,
                    edit:
                        TaskEdit::Model { .. }
                        | TaskEdit::Fast(_)
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
                | core::Request::TerminalResize { .. }
                | core::Request::RunProjectCommand { .. } => {}
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
                if self.edit_failed(&reply.request, cx) {
                    cx.notify();
                    return;
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
    /// Returns the height at the bottom of the conversation list that the composer's box covers,
    /// in window pixels. The list keeps it as padding, so its end rests above the box and the
    /// messages before scroll behind the box's glass.
    pub(super) fn chat_cover(&self) -> f32 {
        (self.composer_height - f32::from(px(chat::COMPOSER_LEAD)) - self.composer_tail).max(0.)
    }
    /// Reports whether the jump to the latest message belongs on screen: the user scrolled the
    /// conversation away from its end.
    pub(super) fn latest_wanted(&self) -> bool {
        !self.chat_scroll.is_following_tail()
            && self
                .view
                .as_ref()
                .is_some_and(|view| !view.messages.is_empty())
    }
    /// Scrolls the conversation to its end, which it then follows until the user scrolls away.
    pub(super) fn follow_latest(&mut self) {
        self.chat_scroll.set_follow_mode(FollowMode::Tail);
        self.unseen_below = false;
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
        self.pending_reference = None;
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
        self.sync_placeholder(cx);
    }
    fn select_task(&mut self, id: String, cx: &mut Context<Self>) {
        self.agent_popup = None;
        // Another task waits until the one being created opens.
        if self.creating_task || self.launch.is_some() {
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
        // A new task keeps its first message in the composer until its conversation loads it.
        if self.auto_submit_task.as_ref() != Some(&id) {
            self.input
                .update(cx, |input, cx| input.set_text(String::new(), cx));
        }
        self.draft_dirty = false;
        self.slash = Default::default();
        self.send_arrivals.clear();
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
        self.pending_reference = None;
        // The conversation is asked for first, so the settings write does not delay it.
        self.send(core::Request::Task {
            task_id: id.clone(),
            before: None,
        });
        self.settings.selected_task = Some(id.clone());
        self.settings.viewed.insert(id, now());
        self.sidebar_dirty = true;
        self.save_settings();
        if self.has_checkout() {
            self.refresh_diff();
            self.refresh_task_changes();
        }
        self.sync_placeholder(cx);
        cx.notify();
    }
    /// Opens the task of the launch once its composer has arrived and the host created the task.
    /// Until both happen the composer waits at the bottom of the chat.
    pub(super) fn finish_launch(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self
            .launch
            .as_ref()
            .filter(|launch| launch.arrived())
            .and_then(|launch| launch.task.clone())
        else {
            return;
        };
        self.launch = None;
        self.select_task(task, cx);
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
    // Asks before quitting. A second quit while the dialog is open quits.
    fn request_quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel == Panel::Quit {
            cx.quit();
            return;
        }
        self.confirm_quit(window, cx);
    }
    // Closing the only window ends Kodem, so it asks like a quit. Only a quit or the dialog's
    // button confirms it.
    fn close_window(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm_quit(window, cx);
    }
    // Opens the quit dialog over the workspace. The full-window views close, as they would
    // hide it.
    fn confirm_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.image_preview = None;
        self.agent_popup = None;
        self.dragging_sidebar = false;
        self.dragging_review = false;
        self.panel = Panel::Quit;
        window.focus(&self.focus, cx);
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

    // Opens a file or path that the transcript referred to. A file of the task's checkout opens
    // in the file browser. An agent often names a file relative to a neighbouring project, or by
    // its name alone, so a reference that is not under the task's folder is looked up in the
    // workspace's projects and then by name among the checkout's files.
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
        let task_id = view.task.id.clone();
        let root = view.task.cwd.clone();
        let Some(absolute) = self.locate_reference(path) else {
            if !self.has_checkout() {
                self.report_error(format!("Could not find {path}."), cx);
            } else if self.files.is_empty() {
                self.pending_reference = Some(path.to_owned());
                self.send(core::Request::Files { task_id });
            } else {
                self.open_listed_reference(path, cx);
            }
            return;
        };
        // A chat has no checkout to browse and a folder has no preview, so they open in the
        // system's application.
        if !self.has_checkout() || absolute.is_dir() {
            cx.open_with_system(&absolute);
            return;
        }
        let checkout = root
            .canonicalize()
            .map(kodem_core::platform::plain_path)
            .unwrap_or(root);
        let relative = absolute
            .canonicalize()
            .map(kodem_core::platform::plain_path)
            .ok()
            .and_then(|resolved| {
                resolved
                    .strip_prefix(&checkout)
                    .ok()
                    .map(|relative| kodem_core::platform::os_bytes(relative.as_os_str()))
            });
        if let Some(relative) = relative {
            self.show_checkout_file(relative, cx);
        } else {
            self.show_outside_file(absolute, cx);
        }
    }

    // Returns the existing file or folder that a reference names: under the task's folder
    // first, then under the task's project and the other projects of the workspace.
    fn locate_reference(&self, path: &str) -> Option<PathBuf> {
        let view = self.view.as_ref()?;
        if let Some(rest) = path.strip_prefix("~/") {
            return kodem_core::platform::home_dir()
                .ok()
                .map(|home| home.join(rest))
                .filter(|candidate| candidate.exists());
        }
        let mut projects: Vec<&Project> = self
            .workspace
            .iter()
            .flat_map(|workspace| &workspace.projects)
            .filter(|project| !project.removed)
            .collect();
        projects.sort_by_key(|project| project.id != view.task.project_id);
        std::iter::once(&view.task.cwd)
            .chain(projects.into_iter().map(|project| &project.path))
            .map(|root| root.join(path))
            .find(|candidate| candidate.exists())
    }

    // Opens the one file of the checkout's list whose path ends with a reference, such as a file
    // that the agent named without its folder.
    fn open_listed_reference(&mut self, path: &str, cx: &mut Context<Self>) {
        let name = path.trim_start_matches("./");
        let suffix = format!("/{name}");
        let found: Vec<Vec<u8>> = self
            .files
            .iter()
            .filter(|file| {
                let file = String::from_utf8_lossy(file);
                file == name || file.ends_with(&suffix)
            })
            .take(2)
            .cloned()
            .collect();
        match found.as_slice() {
            [file] => self.show_checkout_file(file.clone(), cx),
            [] => self.report_error(
                format!("Could not find {name} in this task's folder or its projects."),
                cx,
            ),
            _ => self.report_error(
                format!("{name} matches more than one file in this task's folder."),
                cx,
            ),
        }
    }

    // Previews a file of the task's checkout, given relative to it, in the file browser.
    fn show_checkout_file(&mut self, path: Vec<u8>, cx: &mut Context<Self>) {
        let Some(view) = &self.view else {
            return;
        };
        let task_id = view.task.id.clone();
        self.panel = Panel::Files;
        self.send(core::Request::Files {
            task_id: task_id.clone(),
        });
        self.send(core::Request::ReadFile { task_id, path });
        cx.notify();
    }

    // Previews a text file outside the checkout, such as one of a neighbouring project, in the
    // file browser. A file with no text preview opens in the system's application.
    fn show_outside_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let task_id = self.selected.clone();
        cx.spawn(async move |this, cx| {
            let reading = path.clone();
            let text = cx
                .background_executor()
                .spawn(async move { kodem_core::git::read_text(&reading) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.selected != task_id {
                    return;
                }
                match text {
                    Ok(text) => {
                        s.panel = Panel::Files;
                        if let Some(task_id) = task_id {
                            s.send(core::Request::Files { task_id });
                        }
                        s.file_text = Some((path.display().to_string(), text));
                    }
                    Err(_) => cx.open_with_system(&path),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens preferences on one page. A provider's page that opens from outside its provider
    /// connects the provider's CLI, which reports its version, models and access modes.
    pub(super) fn open_settings(&mut self, page: settings::SettingsPage) {
        let previous = (self.panel == Panel::Settings).then_some(self.settings_page);
        self.panel = Panel::Settings;
        self.settings_page = page;
        if previous != Some(page) {
            // A half-done account form belongs to the page that it was started on.
            self.accounts = accounts::AccountsView::default();
            // The Theme page opens at its top, like every other page.
            self.theme_scroll = ScrollHandle::new();
        }
        if let Some(provider) = page.provider()
            && previous.and_then(settings::SettingsPage::provider) != Some(provider)
        {
            self.send(core::Request::ConnectRuntime {
                provider,
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
        let label = label.into();
        self.raised(id)
            .child(icon(glyph).size(px(13.)))
            .when(!label.is_empty(), |b| b.child(label))
    }
    /// Returns the surface of a raised neutral button without content, for buttons that carry
    /// figures or other children of their own.
    fn raised(&self, id: impl Into<ElementId>) -> Button {
        let theme = self.theme;
        self.control(id)
            .overflow_hidden()
            .layer(kit::shade(theme.dot.opacity(0.9), dither::Edge::Bottom))
            .layer(kit::glint(theme.accent.opacity(0.5), dither::Edge::Bottom))
            .bg(kit::raised_fill(theme.control))
            .shadow(kit::raised_light(theme))
            .hover(move |s| {
                s.bg(kit::raised_fill(theme.control_hover))
                    .border_color(theme.accent.opacity(0.5))
            })
            // A pressed button sinks into the surface: its light goes out and the edge above
            // casts a shadow into it.
            .active(move |s| {
                s.bg(theme.control_active)
                    .shadow(kit::recessed_light(theme))
            })
            .on_press(move |position, cx| fx::ripple(position, theme.text, cx))
    }
    /// Returns a button with no surface until the pointer is over it.
    fn ghost(
        &self,
        id: impl Into<ElementId>,
        name: &'static str,
        label: impl Into<SharedString>,
    ) -> Button {
        self.ghost_icon(id, icon(name).size(px(13.)), label)
    }
    /// Returns a ghost button that leads with `glyph`, an icon that the caller has shaped.
    fn ghost_icon(
        &self,
        id: impl Into<ElementId>,
        glyph: crate::icons::Icon,
        label: impl Into<SharedString>,
    ) -> Button {
        let theme = self.theme;
        let label = label.into();
        self.control(id)
            .overflow_hidden()
            .layer(kit::glint(theme.dot, dither::Edge::Bottom))
            .child(glyph)
            .when(!label.is_empty(), |b| b.child(label))
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .text_color(theme.muted)
            .hover(move |s| {
                s.bg(theme.selected)
                    .text_color(theme.text)
                    .shadow(kit::plate_light(theme, false))
            })
            .active(move |s| {
                s.bg(theme.control_active)
                    .shadow(kit::recessed_light(theme))
            })
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
            .layer(kit::glass_body(tone))
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
        self.tinted_surface(id, tone)
            .child(icon(glyph).size(px(13.)))
            .when(!label.is_empty(), |b| b.child(label))
    }
    /// Returns the surface of a tinted button without content, for buttons that lead with a
    /// marker or carry other children of their own.
    fn tinted_surface(&self, id: impl Into<ElementId>, tone: Hsla) -> Button {
        let theme = self.theme;
        self.control(id)
            .overflow_hidden()
            .layer(kit::shade(tone.opacity(0.3), dither::Edge::Bottom))
            .layer(kit::glint(tone.opacity(0.6), dither::Edge::Bottom))
            .bg(kit::tint_fill(tone, 0.09))
            .shadow(kit::tint_light(tone))
            .border_color(tone.opacity(0.45))
            .text_color(tone)
            .hover(move |s| {
                s.bg(kit::tint_fill(tone, 0.16))
                    .border_color(tone.opacity(0.7))
            })
            .active(move |s| s.bg(tone.opacity(0.22)).shadow(kit::recessed_light(theme)))
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
    incoming.messages = if base.is_sorted_by(|a, b| a.seq < b.seq) {
        // The usual case: the rows held are in order and the news replace or follow the last
        // of them, so each finds its place without a map of the whole transcript.
        let mut messages = base;
        for message in news {
            match messages.binary_search_by_key(&message.seq, |held| held.seq) {
                Ok(index) => messages[index] = message,
                Err(index) => messages.insert(index, message),
            }
        }
        messages
    } else {
        let mut messages: std::collections::BTreeMap<_, _> = base
            .into_iter()
            .map(|message| (message.seq, message))
            .collect();
        for message in news {
            messages.insert(message.seq, message);
        }
        messages.into_values().collect()
    };
    if outdated {
        incoming.task = current.task.clone();
        incoming.activities = current.activities.clone();
        incoming.outgoing = current.outgoing.clone();
        incoming.requests = current.requests.clone();
        incoming.serial = current.serial;
    }
}
/// A provider that is shown as a placeholder. Kodem cannot run it yet, so it takes no click.
#[derive(Clone, Copy)]
struct UpcomingProvider {
    label: &'static str,
    icon: &'static str,
}
impl UpcomingProvider {
    const ALL: [Self; 3] = [
        Self {
            label: "Google",
            icon: "google",
        },
        Self {
            label: "xAI",
            icon: "xai",
        },
        Self {
            label: "Pi",
            icon: "pi",
        },
    ];
    /// Returns the hint shown when the pointer is on the placeholder.
    fn hint(self) -> String {
        format!("{} Coming Soon", self.label)
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
            .child(dither::shadow(self.theme, crate::theme::RADIUS_CONTROL, 3.))
            // Glass with the floating light, but without the dots of a menu's bottom edge: a
            // tooltip is one or two lines, so they would lie behind its text.
            .child(
                kit::glass(div(), self.theme.raised, kit::SURFACE_GLASS_FILL)
                    .font_family(crate::theme::mono())
                    .px(px(8.))
                    .py(px(5.))
                    .text_size(text_px(11.))
                    .line_height(relative(1.45))
                    .text_color(self.theme.text)
                    .border_1()
                    .border_color(self.theme.edge)
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .shadow(self.theme.floating())
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
        KeyBinding::new("secondary-shift-b", ToggleBrowser, None),
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
        KeyBinding::new("secondary-s", diff_view::SaveEdits, Some("DiffDocument")),
    ]);
    // Control-W and Control-H edit text in terminals elsewhere, so only macOS binds them.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-h", Hide, None),
    ]);
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.set_menus(vec![
        Menu {
            disabled: false,
            name: "Kodem".into(),
            items: vec![
                MenuItem::action("Settings", ShowSettings),
                MenuItem::separator(),
                MenuItem::action("Hide Kodem", Hide),
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
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
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
                MenuItem::action("Toggle Browser", ToggleBrowser),
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
