//! The workspace rail: a narrow column of icons that swaps the window between Agentic Coding,
//! Chat, Review and Git. Each task workspace lists its own tasks, and keeps the task that was
//! open in it and the unsent draft of its start screen. The Git workspace covers the task
//! workspace that was open and keeps its own repositories.
use super::*;
use crate::thread_list::Section;

/// Width of the workspace rail, in points.
pub(super) const RAIL: f32 = 48.;

/// The unsent draft of a workspace's start screen, kept while another workspace is open.
#[derive(Default)]
pub(super) struct StartDraft {
    text: String,
    attachments: Vec<AttachmentSource>,
}

/// Returns the keys that open a workspace.
fn mode_keys(mode: TaskKind) -> &'static str {
    match mode {
        TaskKind::Coding => "⌥⌘1",
        TaskKind::Chat => "⌥⌘2",
        TaskKind::Review => "⌥⌘3",
    }
}

impl Shell {
    /// Builds the workspace rail. It also holds the quick actions while the sidebar is closed.
    pub(super) fn workspace_rail(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut rail = div()
            .id("workspace-rail")
            .relative()
            .w(px(RAIL))
            .flex_shrink_0()
            .h_full()
            .bg(t.sidebar)
            .flex()
            .flex_col()
            .items_center()
            .pt(px(10.))
            .pb(px(8.))
            .gap(px(4.));
        for mode in TaskKind::ALL {
            rail = rail.child(self.mode_button(mode, cx));
        }
        rail = rail.child(self.git_button(cx));
        if self.git_open
            && let Some(git) = self.git.clone()
        {
            // The Git workspace's repositories and actions take the rest of the rail.
            return rail
                .child(
                    div()
                        .w(px(22.))
                        .py(px(4.))
                        .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .child(git.update(cx, |git, cx| git.render_rail(window, cx))),
                )
                .into_any_element();
        }
        if self.reveals.sidebar > 0. {
            return rail.into_any_element();
        }
        rail.child(
            div()
                .w(px(22.))
                .py(px(4.))
                .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
        )
        .child(
            self.icon_button("rail-new", "edit", self.new_thread_hint())
                .on_click(cx.listener(|s, _, w, cx| s.new_task(&NewTask, w, cx))),
        )
        .child(
            self.icon_button("rail-search", "search", "Search tasks · ⌘F")
                .on_click(cx.listener(|s, _, w, cx| s.open_search(w, cx))),
        )
        .child(
            self.icon_button("rail-palette", "command", "Command palette · ⌘K")
                .on_click(cx.listener(|s, _, w, cx| s.open_palette(w, cx))),
        )
        .child(
            self.icon_button("rail-open", "folder-open", "Open repository · ⌘O")
                .on_click(cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx))),
        )
        .child(div().flex_1())
        .child(self.update_control(true, cx))
        .child(self.notification_bell("rail-notifications", cx))
        .child(
            self.icon_button("rail-settings", "settings", "Settings · ⌘,")
                .on_click(cx.listener(|s, _, _, cx| {
                    s.panel = Panel::Settings;
                    cx.notify();
                })),
        )
        .into_any_element()
    }

    // One workspace of the rail: an icon tile that carries the accent while its workspace is
    // open, an edge on the rail's leading side, and a marker for tasks that want attention.
    fn mode_button(&self, mode: TaskKind, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let chosen = self.mode == mode && !self.git_open;
        let attention = self.mode_attention(mode);
        let hint = match &attention {
            Some((_, note)) => format!("{} · {note} · {}", mode.label(), mode_keys(mode)),
            None => format!("{} · {}", mode.label(), mode_keys(mode)),
        };
        let tile = self
            .control(("workspace-mode", mode as usize))
            .relative()
            .overflow_hidden()
            .size(px(34.))
            .px_0()
            .justify_center()
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .text_color(t.muted)
            .accessibility_label(mode.label())
            .when(chosen, |tile| {
                tile.bg(kit::tint_fill(t.accent, 0.12))
                    .shadow(kit::tint_light(t.accent))
                    .border_color(t.accent.opacity(0.4))
                    .text_color(t.accent)
                    .layer(kit::shade(t.accent.opacity(0.6), dither::Edge::Bottom))
            })
            .when(!chosen, |tile| {
                tile.layer(kit::glint(t.dot, dither::Edge::Bottom))
                    .hover(move |tile| tile.bg(t.selected).text_color(t.text))
            })
            .active(move |tile| tile.bg(t.control_active))
            .on_press(move |position, cx| fx::ripple(position, t.accent, cx))
            .child(icon(mode.icon()).size(px(17.)))
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: hint.clone(),
                    theme: t,
                })
                .into()
            })
            .on_click(cx.listener(move |s, _, _, cx| s.set_mode(mode, cx)));
        div()
            .relative()
            .w_full()
            .h(px(34.))
            .flex()
            .justify_center()
            .when(chosen, |cell| cell.child(kit::selection_edge(t.accent)))
            // The tile of the workspace that just opened forms out of dither.
            .child(if chosen {
                motion::dissolve(
                    ("workspace-chosen", mode as usize),
                    tile,
                    t.sidebar,
                    crate::theme::RADIUS_CONTROL,
                    0.,
                    2.,
                    260,
                )
            } else {
                tile.into_any_element()
            })
            .when_some(attention.filter(|_| !chosen), |cell, (tone, _)| {
                cell.child(
                    div()
                        .absolute()
                        .top(px(3.))
                        .right(px(8.))
                        .size(px(6.))
                        .rounded(px(1.))
                        .border_1()
                        .border_color(t.sidebar)
                        .bg(tone),
                )
            })
            .into_any_element()
    }

    // Returns the tone and wording of the most urgent state among a workspace's tasks.
    fn mode_attention(&self, mode: TaskKind) -> Option<(Hsla, String)> {
        let t = self.theme;
        let workspace = self.workspace.as_ref()?;
        let mut counts = [0usize; 3];
        for task in workspace
            .tasks
            .iter()
            .filter(|task| task.kind == mode && !task.archived)
        {
            match Section::of(task, &self.settings) {
                Section::NeedsYou => counts[0] += 1,
                Section::Working => counts[1] += 1,
                Section::Ready => counts[2] += 1,
                Section::Threads => {}
            }
        }
        let [waiting, working, ready] = counts;
        if waiting > 0 {
            Some((
                t.warning,
                format!(
                    "{waiting} {} you",
                    if waiting == 1 { "needs" } else { "need" }
                ),
            ))
        } else if working > 0 {
            Some((t.accent, format!("{working} working")))
        } else if ready > 0 {
            Some((t.added, format!("{ready} ready")))
        } else {
            None
        }
    }

    /// Returns the tooltip of the control that starts a new thread in the open workspace.
    pub(super) fn new_thread_hint(&self) -> &'static str {
        match self.mode {
            TaskKind::Coding => "New task · ⌘N",
            TaskKind::Chat => "New chat · ⌘N",
            TaskKind::Review => "New review · ⌘N",
        }
    }

    /// Opens another workspace. The task that was open in it comes back; without one, its start
    /// screen shows.
    pub(super) fn set_mode(&mut self, mode: TaskKind, cx: &mut Context<Self>) {
        if self.creating_task {
            return;
        }
        if self.git_open {
            self.close_git(cx);
        }
        if self.mode == mode {
            return;
        }
        self.enter_mode(mode, cx);
        let remembered = self.mode_tasks.get(&mode).cloned().filter(|id| {
            self.workspace.as_ref().is_some_and(|workspace| {
                workspace
                    .tasks
                    .iter()
                    .any(|task| &task.id == id && task.kind == mode)
            })
        });
        match remembered {
            Some(id) => self.select_task(id, cx),
            None => self.save_settings(),
        }
        cx.notify();
    }

    /// Moves the window to another workspace with no task open in it.
    ///
    /// The task that was open and the draft of the start screen stay with the workspace that is
    /// left, and the draft of the entered workspace returns to the composer.
    pub(super) fn enter_mode(&mut self, mode: TaskKind, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        match &self.selected {
            Some(id) => self.mode_tasks.insert(self.mode, id.clone()),
            None => self.mode_tasks.remove(&self.mode),
        };
        // Closing the open task returns the composer to the start screen's draft.
        self.open_empty_chat(cx);
        let left = StartDraft {
            text: self.input.read(cx).text(),
            attachments: std::mem::take(&mut self.pending_attachments),
        };
        self.mode_drafts.insert(self.mode, left);
        let entered = self.mode_drafts.remove(&mode).unwrap_or_default();
        // The baseline of a review is the merge base of what it examines, so its changes panel
        // opens on that comparison. Leaving Review returns to the working tree.
        if mode == TaskKind::Review {
            self.comparison = Comparison::Baseline;
        } else if self.mode == TaskKind::Review {
            self.comparison = Comparison::WorkingTree;
        }
        self.mode = mode;
        self.settings.mode = mode;
        self.new_preferences = None;
        self.new_worktree_override = None;
        self.empty_draft = entered.text.clone();
        self.pending_attachments = entered.attachments;
        self.input
            .update(cx, |input, cx| input.set_text(entered.text, cx));
        self.draft_dirty = false;
        self.search_visible = false;
        self.search
            .update(cx, |input, cx| input.set_text(String::new(), cx));
        self.sidebar_dirty = true;
        self.sync_placeholder(cx);
        if mode == TaskKind::Review {
            self.load_review_targets(false);
        }
        if self.selected_runtime().models.is_empty() {
            self.connect_runtime();
        }
        cx.notify();
    }

    /// Sets the composer's hint to what the open workspace and task expect.
    pub(super) fn sync_placeholder(&mut self, cx: &mut Context<Self>) {
        let placeholder = match (self.mode, self.selected.is_some()) {
            (TaskKind::Coding, _) => "Describe what you want to build or change…",
            (TaskKind::Chat, _) => "Ask anything…",
            (TaskKind::Review, false) => "Tell the reviewer what to focus on (optional)…",
            (TaskKind::Review, true) => "Ask about this review…",
        };
        self.input
            .update(cx, |input, cx| input.set_placeholder(placeholder, cx));
    }
}

impl Shell {
    // The Git workspace's tile in the rail: the branch glyph with a Beta badge on its corner.
    fn git_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let chosen = self.git_open;
        let tile = self
            .control("workspace-git")
            .relative()
            .overflow_hidden()
            .size(px(34.))
            .px_0()
            .justify_center()
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .text_color(t.muted)
            .accessibility_label("Git, beta")
            .when(chosen, |tile| {
                tile.bg(kit::tint_fill(t.accent, 0.12))
                    .shadow(kit::tint_light(t.accent))
                    .border_color(t.accent.opacity(0.4))
                    .text_color(t.accent)
                    .layer(kit::shade(t.accent.opacity(0.6), dither::Edge::Bottom))
            })
            .when(!chosen, |tile| {
                tile.layer(kit::glint(t.dot, dither::Edge::Bottom))
                    .hover(move |tile| tile.bg(t.selected).text_color(t.text))
            })
            .active(move |tile| tile.bg(t.control_active))
            .on_press(move |position, cx| fx::ripple(position, t.accent, cx))
            .child(icon("git-pull-request").size(px(17.)))
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: "Git · Beta · ⌥⌘4".into(),
                    theme: t,
                })
                .into()
            })
            .on_click(cx.listener(|s, _, _, cx| s.open_git(cx)));
        div()
            .relative()
            .w_full()
            .h(px(34.))
            .flex()
            .justify_center()
            .when(chosen, |cell| cell.child(kit::selection_edge(t.accent)))
            .child(if chosen {
                motion::dissolve(
                    "workspace-git-chosen",
                    tile,
                    t.sidebar,
                    crate::theme::RADIUS_CONTROL,
                    0.,
                    2.,
                    260,
                )
            } else {
                tile.into_any_element()
            })
            .child(git_beta_badge(t).absolute().bottom(px(-1.)).right(px(3.)))
            .into_any_element()
    }

    /// Returns the Git workspace, created the first time it is needed.
    pub(super) fn git_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<git::GitWorkspace> {
        if let Some(git) = &self.git {
            return git.clone();
        }
        let git = cx.new(|cx| git::GitWorkspace::new(window, cx));
        // The title bar, rail and sidebar draw parts of the Git workspace inside this view.
        self._subscriptions
            .push(cx.observe(&git, |_, _, cx| cx.notify()));
        self._subscriptions
            .push(cx.subscribe(&git, |s, _, event: &git::GitEvent, cx| {
                s.git_event(event, cx)
            }));
        self.git = Some(git.clone());
        self.sync_git_projects(cx);
        // The repositories that the workspace restored opened before this view listened.
        for path in git.read(cx).repository_paths(cx) {
            self.git_event(&git::GitEvent::RepositoryOpened(path), cx);
        }
        git
    }

    /// Opens the Git workspace over the task workspace.
    pub(super) fn open_git(&mut self, cx: &mut Context<Self>) {
        if self.git_open || self.creating_task {
            return;
        }
        self.git_open = true;
        self.settings.git_workspace = true;
        self.panel = Panel::None;
        self.search_visible = false;
        self.save_settings();
        cx.notify();
    }

    /// Returns to the task workspace that the Git workspace covered.
    pub(super) fn close_git(&mut self, cx: &mut Context<Self>) {
        if !self.git_open {
            return;
        }
        self.git_open = false;
        self.settings.git_workspace = false;
        if let Some(git) = &self.git {
            git.update(cx, |git, cx| git.dismiss_popovers(cx));
        }
        self.save_settings();
        cx.notify();
    }

    /// Gives the Git workspace Kodem's projects, which name and draw its repositories.
    pub(super) fn sync_git_projects(&mut self, cx: &mut Context<Self>) {
        let Some(git) = &self.git else {
            return;
        };
        let projects = self
            .workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .projects
                    .iter()
                    .filter(|project| !project.removed)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        git.update(cx, |git, cx| git.set_projects(projects, cx));
    }

    // Carries out what the Git workspace asks of the window.
    fn git_event(&mut self, event: &git::GitEvent, cx: &mut Context<Self>) {
        match event {
            git::GitEvent::RepositoryOpened(path) => {
                let known = self.workspace.as_ref().is_some_and(|workspace| {
                    workspace
                        .projects
                        .iter()
                        .any(|project| !project.removed && &project.path == path)
                });
                if !known {
                    self.git_registrations.insert(plain_path(path));
                    self.send(core::Request::OpenProject {
                        path: path.clone(),
                        missing_repository: MissingRepository::Ask,
                    });
                }
            }
            git::GitEvent::CustomizeRepository(path) => {
                let project = self.workspace.as_ref().and_then(|workspace| {
                    workspace
                        .projects
                        .iter()
                        .find(|project| !project.removed && &project.path == path)
                        .cloned()
                });
                if let Some(project) = project {
                    let name = project.name.clone();
                    self.aux.update(cx, |input, cx| input.set_text(name, cx));
                    self.icon_filter
                        .update(cx, |input, cx| input.set_text(String::new(), cx));
                    self.editing_project = Some(project);
                    self.error = None;
                    self.panel = Panel::CustomizeProject;
                    cx.notify();
                }
            }
        }
    }
}

/// Returns the Beta badge of the Git workspace: a tiny accent label in the mono face.
pub(super) fn git_beta_badge(theme: Theme) -> Div {
    div()
        .px(px(2.))
        .rounded(px(2.))
        .bg(theme.accent)
        .text_color(theme.on_accent)
        .font_family(crate::theme::mono())
        .text_size(text_px(6.5))
        .line_height(text_px(8.))
        .font_weight(FontWeight::BOLD)
        .letter_spacing(text_px(0.3))
        .child("BETA")
}

/// Returns a folder in the form that the host records for a project, for comparisons.
pub(super) fn plain_path(path: &std::path::Path) -> PathBuf {
    path.canonicalize()
        .map(kodem_core::platform::plain_path)
        .unwrap_or_else(|_| path.to_path_buf())
}
