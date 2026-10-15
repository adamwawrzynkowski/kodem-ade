//! The workspace rail: a narrow column of icons that swaps the window between Agentic Coding,
//! Chat and Review. Each workspace lists its own tasks, and keeps the task that was open in it
//! and the unsent draft of its start screen.
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
    pub(super) fn workspace_rail(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let chosen = self.mode == mode;
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
                    .child(kit::shade(t.accent.opacity(0.6), dither::Edge::Bottom))
            })
            .when(!chosen, |tile| {
                tile.child(kit::glint(t.dot, dither::Edge::Bottom))
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
        if self.mode == mode || self.creating_task {
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
