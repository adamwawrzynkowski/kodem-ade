//! Command palette: one searchable list of every workspace action, thread and account.
//!
//! The list is rebuilt from current state on each render, so it never offers an action that the
//! open task cannot take. Rows are ranked by how early and how tightly the query matches.
use super::*;
use crate::components::text_input::CompletionInput;

#[derive(Clone)]
enum Action {
    NewTask,
    OpenProject,
    OpenTask(String),
    Workspace(TaskKind),
    Git,
    Plugins,
    CloneRepository,
    NextAttention,
    Models,
    Reasoning,
    Access,
    Account(Option<String>),
    ToggleSidebar,
    ToggleReview,
    ToggleBrowser,
    ToggleTerminal,
    ToggleArchived,
    Files,
    Resources,
    Settings(settings::SettingsPage),
    Setup,
    WhatsNew,
    Theme(ThemeId),
    Interrupt,
    Compact,
    Rename,
    Pin(bool),
    Settle(bool),
    Archive(bool),
    Latest,
}

struct Item {
    glyph: &'static str,
    title: String,
    detail: String,
    keys: &'static str,
    group: &'static str,
    action: Action,
}

#[derive(Default)]
pub(super) struct Palette {
    selected: usize,
    scroll: ScrollHandle,
    query: String,
}

// Scores a query against text: an exact prefix first, then a word start, then any substring,
// then characters in order. Lower is better; no match returns nothing.
fn score(text: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let text = text.to_lowercase();
    if text.starts_with(query) {
        return Some(0);
    }
    if let Some(index) = text.find(query) {
        let word_start = index == 0
            || text[..index]
                .chars()
                .next_back()
                .is_some_and(|ch| !ch.is_alphanumeric());
        return Some(if word_start { 1 } else { 2 } * 100 + index.min(99));
    }
    let mut characters = text.chars();
    let mut gaps = 0;
    for wanted in query.chars().filter(|ch| !ch.is_whitespace()) {
        loop {
            match characters.next() {
                Some(ch) if ch == wanted => break,
                Some(_) => gaps += 1,
                None => return None,
            }
        }
    }
    Some(1000 + gaps)
}

impl Shell {
    /// Opens the palette with an empty query and moves keyboard focus into it.
    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = Palette::default();
        self.palette_input.update(cx, |input, cx| {
            input.set_text(String::new(), cx);
            input.set_completion_open(true);
        });
        self.panel = Panel::Palette;
        window.focus(&self.palette_input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// Resets the highlighted row when the query changes.
    pub(super) fn sync_palette(&mut self, cx: &mut Context<Self>) {
        let query = self.palette_input.read(cx).text().trim().to_lowercase();
        if query != self.palette.query {
            self.palette.query = query;
            self.palette.selected = 0;
            self.palette.scroll.set_offset(point(px(0.), px(0.)));
        }
    }

    /// Moves the highlight, runs the highlighted action or closes the palette.
    pub(super) fn palette_key(
        &mut self,
        key: &CompletionInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.panel != Panel::Palette {
            return;
        }
        let items = self.palette_items();
        match key {
            CompletionInput::Dismiss => {
                self.panel = Panel::None;
                window.focus(&self.focus, cx);
            }
            CompletionInput::Accept => {
                if let Some(item) = items.get(self.palette.selected) {
                    self.run_palette(item.action.clone(), window, cx);
                }
            }
            CompletionInput::Previous | CompletionInput::Next => {
                let count = items.len();
                if count > 0 {
                    self.palette.selected = if matches!(key, CompletionInput::Previous) {
                        (self.palette.selected + count - 1) % count
                    } else {
                        (self.palette.selected + 1) % count
                    };
                    self.palette.scroll.scroll_to_item(self.palette.selected);
                }
            }
        }
        cx.notify();
    }

    fn palette_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        let mut add = |glyph, title: &str, detail: &str, keys, group, action| {
            items.push(Item {
                glyph,
                title: title.to_owned(),
                detail: detail.to_owned(),
                keys,
                group,
                action,
            });
        };
        add(
            "plus",
            match self.mode {
                TaskKind::Coding => "New task",
                TaskKind::Chat => "New chat",
                TaskKind::Review => "New review",
            },
            "",
            "⌘N",
            "Actions",
            Action::NewTask,
        );
        add(
            "folder-open",
            "Open repository…",
            "",
            "⌘O",
            "Actions",
            Action::OpenProject,
        );
        let attention = self.workspace.as_ref().is_some_and(|w| {
            w.tasks
                .iter()
                .any(|task| !task.archived && (task.needs_input() || task.is_blocked()))
        });
        if attention {
            add(
                "shield",
                "Go to the next task that needs you",
                "",
                "⌥⌘A",
                "Actions",
                Action::NextAttention,
            );
        }
        if let Some(view) = &self.view {
            let task = &view.task;
            if task.state.active() {
                add(
                    "stop",
                    "Stop the current turn",
                    &task.title,
                    "",
                    "This task",
                    Action::Interrupt,
                );
            }
            if task.provider_thread.is_some() && !task.state.active() {
                add(
                    "layers",
                    "Compact context",
                    &task.title,
                    "",
                    "This task",
                    Action::Compact,
                );
            }
            add(
                "arrow-down",
                "Jump to the latest message",
                "",
                "",
                "This task",
                Action::Latest,
            );
            add(
                "pencil",
                "Rename task…",
                &task.title,
                "",
                "This task",
                Action::Rename,
            );
            add(
                "pin",
                if task.pinned {
                    "Unpin task"
                } else {
                    "Pin task"
                },
                "",
                "",
                "This task",
                Action::Pin(!task.pinned),
            );
            if !task.state.active() && task.state != RunState::DeliveryUnknown {
                let settled = task.is_settled(self.settings.auto_settle);
                add(
                    "circle-check",
                    if settled {
                        "Reopen task"
                    } else {
                        "Settle task"
                    },
                    "",
                    "",
                    "This task",
                    Action::Settle(!settled),
                );
            }
            add(
                "archive",
                if task.archived {
                    "Restore task from the archive"
                } else {
                    "Archive task"
                },
                "",
                "",
                "This task",
                Action::Archive(!task.archived),
            );
            // A chat has no checkout to browse.
            if self.has_checkout() {
                add(
                    "file-text",
                    "Browse project files",
                    "",
                    "",
                    "This task",
                    Action::Files,
                );
            }
        }
        for (mode, keys) in [
            (TaskKind::Coding, "⌥⌘1"),
            (TaskKind::Chat, "⌥⌘2"),
            (TaskKind::Review, "⌥⌘3"),
        ] {
            if mode != self.mode || self.git_open || self.plugins.open {
                add(
                    mode.icon(),
                    &format!("Open {}", mode.label()),
                    "",
                    keys,
                    "Workspaces",
                    Action::Workspace(mode),
                );
            }
        }
        if !self.git_open {
            add(
                "git-pull-request",
                "Open Git",
                "",
                "⌥⌘4",
                "Workspaces",
                Action::Git,
            );
        }
        if !self.plugins.open {
            add(
                "puzzle",
                "Open Plugins",
                "MCP servers and skills",
                "⌥⌘5",
                "Workspaces",
                Action::Plugins,
            );
        }
        add(
            "arrow-down-to-line",
            "Clone repository",
            "",
            "⇧⌘O",
            "Git",
            Action::CloneRepository,
        );
        let preferences = self.composer_preferences();
        let model = self.model_name(&preferences);
        add(
            preferences.provider.icon(),
            "Change provider or model…",
            &model,
            "⇧⌘M",
            "Agent",
            Action::Models,
        );
        add(
            "gauge",
            "Change reasoning effort…",
            preferences.effort.as_deref().unwrap_or(""),
            "⇧⌘E",
            "Agent",
            Action::Reasoning,
        );
        add(
            "shield",
            "Change access mode…",
            "",
            "⇧⌘A",
            "Agent",
            Action::Access,
        );
        add(
            "code",
            "Skills, instructions and MCP",
            "",
            "",
            "Agent",
            Action::Resources,
        );
        if let Some(workspace) = &self.workspace {
            for state in workspace
                .accounts
                .iter()
                .filter(|state| state.account.provider == preferences.provider)
            {
                let current = preferences.account.as_deref().unwrap_or(DEFAULT_ACCOUNT);
                if state.account.id == current {
                    continue;
                }
                let detail = state.status.email.clone().unwrap_or_default();
                add(
                    "user",
                    &format!("Use account: {}", state.account.name),
                    &detail,
                    "",
                    "Agent",
                    Action::Account(
                        (!state.account.is_default()).then(|| state.account.id.clone()),
                    ),
                );
            }
            for task in workspace.tasks.iter().filter(|task| !task.archived) {
                if self.selected.as_deref() == Some(&task.id) {
                    continue;
                }
                let project = workspace
                    .projects
                    .iter()
                    .find(|project| project.id == task.project_id)
                    .map_or("", |project| project.name.as_str());
                let glyph = if task.needs_input() {
                    "shield"
                } else if task.state.active() {
                    "loader"
                } else if task.is_blocked() {
                    "circle-x"
                } else if task.kind == TaskKind::Review {
                    task.kind.icon()
                } else {
                    "chat"
                };
                // A chat has neither a project nor a branch, so it names its workspace. A folder
                // without Git has no branch.
                let detail = if task.kind.has_project() && task.branch.is_empty() {
                    project.to_string()
                } else if task.kind.has_project() {
                    format!("{project} / {}", task.branch)
                } else {
                    task.kind.label().to_owned()
                };
                add(
                    glyph,
                    &task.title,
                    &detail,
                    "",
                    "Threads",
                    Action::OpenTask(task.id.clone()),
                );
            }
        }
        add(
            "panel-left",
            "Toggle sidebar",
            "",
            "⌘B",
            "View",
            Action::ToggleSidebar,
        );
        add(
            "panel-right",
            "Toggle changes",
            "",
            "⌘D",
            "View",
            Action::ToggleReview,
        );
        add(
            "globe",
            "Toggle browser",
            "",
            "⇧⌘B",
            "View",
            Action::ToggleBrowser,
        );
        add(
            "square-terminal",
            "Toggle terminal",
            "",
            "⌃`",
            "View",
            Action::ToggleTerminal,
        );
        add(
            "archive",
            if self.show_archived {
                "Hide archived tasks"
            } else {
                "Show archived tasks"
            },
            "",
            "",
            "View",
            Action::ToggleArchived,
        );
        for theme in ThemeId::ALL {
            if theme != self.settings.theme {
                add(
                    if theme.light() { "sun" } else { "moon" },
                    &format!("Theme: {}", theme.label()),
                    if theme.light() { "Light" } else { "Dark" },
                    "",
                    "View",
                    Action::Theme(theme),
                );
            }
        }
        for category in &settings::CATEGORIES {
            for page in category.all_pages() {
                add(
                    page.provider()
                        .map_or(category.icon, |provider| provider.icon()),
                    &format!("Settings: {}", page.trail()),
                    "",
                    if page == settings::SettingsPage::default() {
                        "⌘,"
                    } else {
                        ""
                    },
                    "Settings",
                    Action::Settings(page),
                );
            }
        }
        add(
            "sparkles",
            "Open setup",
            "Providers, Claude Code features and first choices",
            "",
            "Settings",
            Action::Setup,
        );
        if crate::release_notes::available() {
            add(
                "sparkles",
                "What's new",
                "The changes in this version",
                "",
                "Settings",
                Action::WhatsNew,
            );
        }
        let query = &self.palette.query;
        if query.is_empty() {
            return items;
        }
        let mut ranked: Vec<_> = items
            .into_iter()
            .filter_map(|item| {
                let rank = score(&item.title, query)
                    .or_else(|| score(&item.detail, query).map(|rank| rank + 2000))?;
                Some((rank, item))
            })
            .collect();
        ranked.sort_by_key(|(rank, _)| *rank);
        ranked.into_iter().map(|(_, item)| item).collect()
    }

    fn run_palette(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        self.panel = Panel::None;
        window.focus(&self.focus, cx);
        let task_id = self.selected.clone();
        let edit = |shell: &Self, edit: TaskEdit| {
            if let Some(task_id) = &task_id {
                shell.send(core::Request::EditTask {
                    task_id: task_id.clone(),
                    edit,
                });
            }
        };
        match action {
            Action::NewTask => self.new_task(&NewTask, window, cx),
            Action::OpenProject => self.open_project(&OpenProject, window, cx),
            Action::OpenTask(id) => {
                self.close_git(cx);
                self.close_plugins(cx);
                self.select_task(id, cx)
            }
            Action::Workspace(mode) => self.set_mode(mode, cx),
            Action::Git => self.open_git(cx),
            Action::Plugins => self.open_plugins(cx),
            Action::CloneRepository => {
                self.open_git(cx);
                let git = self.git_workspace(window, cx);
                git.update(cx, |git, cx| git.clone_repository_dialog(window, cx));
            }
            Action::NextAttention => self.select_next_attention(cx),
            Action::Models => self.open_agent_menu(Panel::Models, window, cx),
            Action::Reasoning => self.open_agent_menu(Panel::Reasoning, window, cx),
            Action::Access => self.open_agent_menu(Panel::Access, window, cx),
            Action::Account(account) => self.choose_account(account, cx),
            Action::ToggleSidebar => {
                self.settings.collapsed = !self.settings.collapsed;
                self.save_settings();
            }
            Action::ToggleReview => self.toggle_pane(PanePage::Changes),
            Action::ToggleBrowser => self.toggle_pane(PanePage::Browser),
            Action::ToggleTerminal => {
                self.show_terminal = !self.show_terminal;
                self.save_settings();
            }
            Action::ToggleArchived => {
                self.show_archived = !self.show_archived;
                self.sidebar_dirty = true;
            }
            Action::Files => {
                self.panel = Panel::Files;
                if let Some(task_id) = &task_id {
                    self.send(core::Request::Files {
                        task_id: task_id.clone(),
                    });
                }
            }
            Action::Resources => self.open_resources(),
            Action::Settings(section) => self.open_settings(section),
            Action::Setup => self.open_setup(),
            Action::WhatsNew => self.panel = Panel::WhatsNew,
            Action::Theme(theme) => self.set_theme(theme, cx),
            Action::Interrupt => {
                if let Some(task_id) = &task_id {
                    self.send(core::Request::Interrupt {
                        task_id: task_id.clone(),
                    });
                }
            }
            Action::Compact => self.compact_task(cx),
            Action::Rename => {
                if let Some(view) = &self.view {
                    let title = view.task.title.clone();
                    self.aux.update(cx, |input, cx| input.set_text(title, cx));
                    self.panel = Panel::Rename;
                    window.focus(&self.aux.read(cx).focus_handle(cx), cx);
                }
            }
            Action::Pin(pin) => edit(self, TaskEdit::Pin(pin)),
            Action::Settle(settled) => edit(self, TaskEdit::Settle(settled)),
            Action::Archive(archive) => edit(self, TaskEdit::Archive(archive)),
            Action::Latest => self.follow_latest(),
        }
        cx.notify();
    }

    /// Builds the palette: a query field, the ranked rows with their group and shortcut, and the
    /// keys that drive it.
    pub(super) fn palette(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sync_palette(cx);
        let t = self.theme;
        let items = self.palette_items();
        if self.palette.selected >= items.len() {
            self.palette.selected = items.len().saturating_sub(1);
        }
        let viewport = window.viewport_size();
        let mut rows = div()
            .id("palette-rows")
            .track_scroll(&self.palette.scroll)
            .max_h((viewport.height - px(240.)).max(px(120.)).min(px(420.)))
            .overflow_y_scroll()
            .p(px(4.))
            .flex()
            .flex_col();
        let grouped = self.palette.query.is_empty();
        let provider = self.composer_preferences().provider;
        let mut previous_group = "";
        for (index, item) in items.iter().enumerate() {
            let chosen = index == self.palette.selected;
            let action = item.action.clone();
            let header = (grouped && item.group != previous_group).then(|| {
                div()
                    .px(px(8.))
                    .pt(px(if index == 0 { 4. } else { 10. }))
                    .pb(px(4.))
                    .child(kit::section_header(item.group, None, t))
            });
            previous_group = item.group;
            rows = rows.child(
                div().flex().flex_col().children(header).child(
                    div()
                        .id(("palette-row", index))
                        .relative()
                        .overflow_hidden()
                        .h(px(32.))
                        .flex_shrink_0()
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .rounded(px(crate::theme::RADIUS_CONTROL))
                        .cursor_pointer()
                        .when(chosen, |row| {
                            row.bg(t.accent.opacity(0.1))
                                .layer(dither::row_wash(t.accent, true, false))
                                .child(kit::selection_edge(t.accent))
                        })
                        .when(!chosen, |row| row.hover(move |row| row.bg(t.selected)))
                        .active(move |row| row.bg(t.control_active))
                        .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                            fx::ripple(event.position, t.muted, cx)
                        })
                        .child(icon(item.glyph).size(px(14.)).text_color(if chosen {
                            t.accent
                        } else {
                            t.muted
                        }))
                        .child(
                            div()
                                .relative()
                                .flex_shrink_0()
                                .max_w(relative(0.62))
                                .text_size(text_px(13.))
                                .text_color(t.text)
                                .text_ellipsis()
                                .child(item.title.clone()),
                        )
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .min_w_0()
                                .font_family(crate::theme::mono())
                                .text_size(text_px(11.))
                                .text_color(t.subtle)
                                .text_ellipsis()
                                .map(|detail| match &item.action {
                                    // The detail of an account row is the account's email.
                                    Action::Account(id) if !item.detail.is_empty() => {
                                        detail.child(self.censored_email(
                                            provider,
                                            id.as_deref().unwrap_or(DEFAULT_ACCOUNT),
                                            &item.detail,
                                            cx,
                                        ))
                                    }
                                    _ => detail.child(item.detail.clone()),
                                }),
                        )
                        .when(!grouped, |row| row.child(caps(item.group, t)))
                        .when(!item.keys.is_empty(), |row| {
                            row.child(kit::keycap(item.keys, t))
                        })
                        .on_click(cx.listener(move |s, _, window, cx| {
                            s.run_palette(action.clone(), window, cx)
                        })),
                ),
            );
        }
        if items.is_empty() {
            rows = rows.child(kit::empty_hint("No matching command or thread.", t));
        }
        let surface = div()
            .relative()
            .w(px(640.).min((viewport.width - px(48.)).max(px(0.))))
            .child(dither::shadow(t, crate::theme::RADIUS_FLOATING, 8.))
            .child(kit::floating_face(
                t.raised,
                crate::theme::RADIUS_FLOATING,
                t,
            ))
            .child(
                div()
                    .relative()
                    .rounded(px(crate::theme::RADIUS_FLOATING))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .h(px(44.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(icon("command").size(px(15.)).text_color(t.accent))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(text_px(14.))
                                    .child(self.palette_input.clone()),
                            )
                            .child(kit::keycap("esc", t)),
                    )
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
                    .child(rows)
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
                    .child(
                        div()
                            .h(px(30.))
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .bg(t.header.opacity(kit::PANE_SHARE))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .child(kit::keycap("↑↓", t))
                                    .child(caps("Move", t)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .child(kit::keycap("⏎", t))
                                    .child(caps("Run", t)),
                            )
                            .child(div().flex_1())
                            .child(figure(format!("{} results", items.len()), t.subtle)),
                    ),
            );
        div()
            .absolute()
            .inset_0()
            .occlude()
            .child(dither::scrim(t))
            .flex()
            .flex_col()
            .items_center()
            .pt(px(96.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, window, cx| {
                    s.panel = Panel::None;
                    window.focus(&s.focus, cx);
                    cx.notify();
                }),
            )
            .child(motion::floating("palette-enter", surface, t))
            .into_any_element()
    }
}
