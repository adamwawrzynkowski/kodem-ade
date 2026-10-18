use super::*;
use crate::thread_list::{Row, Section};
use dither::{Dither, DitherField};
use std::time::Instant;

/// Returns the effect anchor name of one thread row.
pub(super) fn thread_anchor(task_id: &str) -> SharedString {
    SharedString::from(format!("thread:{task_id}"))
}

/// Side of the tile that leads a task card. It spans the title and the line under it.
const TASK_TILE: f32 = 30.;
/// Size of a glyph in that tile.
const TASK_TILE_GLYPH: f32 = 18.;
/// The effect anchor of the sidebar, which places the sliding selection edge.
const SIDEBAR_ANCHOR: &str = "sidebar-panel";
/// How long the selection edge of the feed takes to slide to another row.
const SELECT_SLIDE: Duration = Duration::from_millis(380);
/// How long one row takes to fold or unfold.
const FOLD_ROW: Duration = Duration::from_millis(260);
/// How long each row of a fold waits after the row above it.
const FOLD_STEP: Duration = Duration::from_millis(35);
/// Rows after this many share the last wait, so a long group folds as quickly as a short one.
const FOLD_STAGGER: usize = 8;

/// The slide of the feed's selection edge from the row that was open to the row that opened.
#[derive(Default)]
pub(super) struct FeedSlide {
    /// The task that the last frame marked.
    shown: Option<String>,
    /// The task that the selection left and its edge as `(left, top, bottom)`, in window pixels
    /// from the sidebar's corner.
    from: Option<(String, (f32, f32, f32))>,
    started: Option<Instant>,
}

impl FeedSlide {
    // How far the running slide has come, from 0 to 1. A slide that has arrived returns none.
    fn progress(&self) -> Option<f32> {
        let progress = self.started?.elapsed().as_secs_f32() / SELECT_SLIDE.as_secs_f32();
        (progress < 1.).then_some(progress)
    }

    // The task that the running slide left.
    fn left(&self) -> Option<&str> {
        self.progress()?;
        self.from.as_ref().map(|(id, _)| id.as_str())
    }
}

/// A project group or the settled list on its way between folded and open. Its rows move one
/// after another from the top.
#[derive(Clone, Copy)]
pub(super) struct GroupFold {
    started: Instant,
    opening: bool,
    rows: usize,
}

impl GroupFold {
    /// Starts a fold of `rows` rows toward open, or toward folded.
    pub(super) fn new(opening: bool, rows: usize) -> Self {
        Self {
            started: Instant::now(),
            opening,
            rows,
        }
    }

    // How far the row at `order` from the top of the group has moved, from 0 to 1.
    fn progress(&self, order: usize) -> f32 {
        let wait = FOLD_STEP * order.min(FOLD_STAGGER) as u32;
        let elapsed = self.started.elapsed().saturating_sub(wait);
        (elapsed.as_secs_f32() / FOLD_ROW.as_secs_f32()).min(1.)
    }

    // How far the row at `order` from the top of the group is open, from 0 to 1.
    fn openness(&self, order: usize) -> f32 {
        let progress = self.progress(order);
        if self.opening {
            progress
        } else {
            1. - progress
        }
    }

    // How far the group's chevron has turned toward open, from 0 to 1.
    fn chevron(&self) -> f32 {
        let turned = motion::ease_out_cubic(self.progress(0));
        if self.opening { turned } else { 1. - turned }
    }

    // Reports whether every row of the group has arrived.
    fn done(&self) -> bool {
        let last = self.rows.saturating_sub(1).min(FOLD_STAGGER) as u32;
        self.started.elapsed() >= FOLD_STEP * last + FOLD_ROW
    }
}

// A chevron that turns from pointing right, folded, to pointing down, open, as `openness` goes
// from 0 to 1.
fn fold_chevron(openness: f32, size: f32) -> crate::icons::Icon {
    icon("chevron-right")
        .size(px(size))
        .rotate(std::f32::consts::FRAC_PI_2 * openness.clamp(0., 1.))
}

// The selection edge of a task's row as `(left, top, bottom)`, in window pixels from the
// sidebar's corner, where the last frame painted it.
fn edge_span(task_id: &str, cx: &mut App) -> Option<(f32, f32, f32)> {
    let panel = fx::anchor_bounds(SIDEBAR_ANCHOR, cx)?;
    let row = fx::anchor_bounds(&thread_anchor(task_id), cx)?;
    let inset = 4. * crate::theme::ui_scale();
    Some((
        f32::from(row.left() - panel.left()),
        f32::from(row.top() - panel.top()) + inset,
        f32::from(row.bottom() - panel.top()) - inset,
    ))
}

// The tone of a task's row while it is open: amber while it waits for input, red while it is
// blocked, and the accent otherwise.
fn selection_tone(task: &core::Task, theme: Theme) -> Hsla {
    if task.needs_input() {
        theme.warning
    } else if task.is_blocked() {
        theme.removed
    } else {
        theme.accent
    }
}

impl Shell {
    /// Builds the attention-first thread feed with status, branch and saved review metadata.
    pub(super) fn sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        self.settle_folds();
        self.sync_feed(cx);
        // The list measures a row once, so a row that opens or folds is measured again on each
        // frame, and once more on the frame it is done.
        self.fold_rows.clear();
        if !self.entering_tasks.is_empty() || !self.group_folds.is_empty() {
            let mut group = None;
            let mut order = 0;
            for (index, row) in self.sidebar_rows.iter().enumerate() {
                let id = match row {
                    Row::Project { id, .. } => {
                        group = self.group_folds.get(id);
                        order = 0;
                        continue;
                    }
                    Row::Task { id, .. } => id,
                    _ => {
                        group = None;
                        continue;
                    }
                };
                let folding = group.map(|fold| fold.openness(order));
                order += 1;
                if let Some(openness) = folding {
                    self.fold_rows.insert(id.clone(), openness);
                }
                if folding.is_some() || self.entering_tasks.contains_key(id) {
                    self.sidebar_scroll.remeasure_items(index..index + 1);
                }
            }
            self.entering_tasks.retain(|_, entrance| !entrance.done());
            self.group_folds.retain(|_, fold| !fold.done());
        }
        self.sync_feed_slide(cx);
        // A closed sidebar draws nothing; the workspace rail holds its quick actions. While it
        // slides, the full sidebar is drawn and clipped.
        if self.reveals.sidebar <= 0. {
            return div().into_any_element();
        }
        let mode = self.mode;
        let project_label = self
            .workspace
            .as_ref()
            .and_then(|w| {
                w.projects
                    .iter()
                    .find(|p| Some(&p.id) == self.sidebar_project.as_ref())
            })
            .map_or_else(|| "All projects".to_owned(), |p| p.name.clone());
        let search = if self.search_visible {
            div()
                .flex_1()
                .min_w_0()
                .h(px(28.))
                .pl(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .bg(t.inset)
                .border_1()
                .border_color(t.accent.opacity(0.5))
                .font_family(crate::theme::mono())
                .child(icon("search").size(px(13.)).text_color(t.muted))
                .child(div().flex_1().min_w_0().child(self.search.clone()))
                .child(
                    self.icon_button("close-search", "close", "Close search · Esc")
                        .h(px(24.))
                        .w(px(24.))
                        .on_click(cx.listener(|s, _, w, cx| {
                            s.search_visible = false;
                            s.search.update(cx, |i, cx| i.set_text(String::new(), cx));
                            s.sidebar_dirty = true;
                            w.focus(&s.focus, cx);
                            cx.notify();
                        })),
                )
                .into_any_element()
        } else {
            self.button(
                "new-task",
                "plus",
                match mode {
                    TaskKind::Coding => "New task",
                    TaskKind::Chat => "New chat",
                    TaskKind::Review => "New review",
                },
            )
            .flex_1()
            .min_w_0()
            .h(px(28.))
            .justify_start()
            .child(div().flex_1())
            .child(kit::keycap("⌘N", t))
            .on_click(cx.listener(|s, _, w, cx| s.new_task(&NewTask, w, cx)))
            .into_any_element()
        };
        // A chat has no project, so its feed has a title where the others have a project filter.
        let scope = if mode.has_project() {
            self.ghost("project-filter", "folder-closed", "")
                .flex_1()
                .min_w_0()
                .justify_start()
                .px(px(6.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .child(project_label),
                )
                .child(icon("chevrons-up-down").size(px(12.)))
                .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                    s.menu_anchor = Some(*bounds);
                    s.panel = Panel::Projects;
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .h(px(26.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(icon(mode.icon()).size(px(13.)).text_color(t.muted))
                .child(caps("Conversations", t).text_color(t.muted))
                .into_any_element()
        };
        let mut panel = div()
            .id("sidebar")
            .relative()
            .w(px(self.settings.sidebar_width))
            .flex_shrink_0()
            .h_full()
            .map(|panel| kit::panel_glass(panel, t))
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_size(text_px(12.))
            .child(fx::anchor(SIDEBAR_ANCHOR))
            .child(
                div()
                    .px(px(10.))
                    .pt(px(10.))
                    .pb(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(search)
                    .when(!self.search_visible, |row| {
                        row.child(
                            self.icon_button("search-tasks", "search", "Search tasks · ⌘F")
                                .h(px(28.))
                                .w(px(28.))
                                .on_click(cx.listener(|s, _, w, cx| s.open_search(w, cx))),
                        )
                        .child(
                            self.icon_button("open-palette", "command", "Command palette · ⌘K")
                                .h(px(28.))
                                .w(px(28.))
                                .on_click(cx.listener(|s, _, w, cx| s.open_palette(w, cx))),
                        )
                    }),
            )
            .child(
                div()
                    .px(px(10.))
                    .pb(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(scope)
                    .child(
                        self.icon_button("sort-tasks", "settings-2", "Thread list settings")
                            .on_click_with_bounds(cx.listener(
                                |s, bounds: &Bounds<Pixels>, _, cx| {
                                    s.menu_anchor = Some(*bounds);
                                    s.panel = Panel::Sort;
                                    cx.notify();
                                },
                            )),
                    )
                    .when(mode.has_project(), |row| {
                        row.child(
                            self.icon_button("add-project", "folder-open", "Open repository · ⌘O")
                                .on_click(
                                    cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx)),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .px(px(10.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
            );
        let entity = cx.entity();
        let feed = list(self.sidebar_scroll.clone(), move |index, _, cx| {
            // List positions rows at its left edge; horizontal gutters belong inside each row.
            div()
                .w_full()
                .px(px(6.))
                .child(entity.update(cx, |s, cx| s.sidebar_row(index, cx)))
                .into_any_element()
        })
        .flex_1()
        .min_h_0()
        .w_full()
        .pt(px(6.))
        .pb_2();
        let settled_open = self.settled_open(cx);
        let settled_header =
            (!self.settled_rows.is_empty()).then(|| self.settled_header(settled_open, cx));
        let settled_tasks =
            (settled_open && !self.settled_rows.is_empty()).then(|| self.settled_tasks(cx));
        panel = panel
            // The feed of a workspace that just opened clears out of a dissolve.
            .child(motion::switched(
                ("feed-enter", mode as usize),
                div()
                    .relative()
                    .flex_1()
                    // Open tasks keep a share of the panel beside a long settled list.
                    .min_h(px(96.))
                    .flex()
                    .flex_col()
                    .child(feed),
                t.sidebar,
                4.,
            ))
            .children(settled_header)
            .children(settled_tasks)
            .child(self.sidebar_usage(cx))
            .child(
                div()
                    .px(px(10.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
            )
            .child(
                div()
                    .h(px(38.))
                    .flex_shrink_0()
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        self.icon_button("settings", "settings", "Settings · ⌘,")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.panel = Panel::Settings;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.icon_button("archive-view", "archive", "Toggle archived tasks")
                            .when(self.show_archived, |b| {
                                b.bg(t.accent.opacity(0.1)).text_color(t.accent)
                            })
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.show_archived = !s.show_archived;
                                s.sidebar_dirty = true;
                                cx.notify();
                            })),
                    )
                    .child(self.notification_bell("notification-center", cx))
                    .child(div().flex_1())
                    .child(self.update_control(false, cx))
                    .child(
                        // A placeholder for the accounts menu: it shows a hint and takes no click.
                        div().px(px(4.)).child(
                            kit::icon_tile("user", t.subtle, 24., false)
                                .id("accounts-avatar")
                                .rounded_full()
                                .hover(move |s| {
                                    s.border_color(t.accent.opacity(0.5))
                                        .bg(kit::tint_fill(t.accent, 0.1))
                                })
                                .tooltip(move |_, cx| {
                                    cx.new(|_| Hint {
                                        text: "Accounts are coming soon".into(),
                                        theme: t,
                                    })
                                    .into()
                                }),
                        ),
                    ),
            );
        let moving = self.feed_slide.progress().is_some()
            || !self.group_folds.is_empty()
            || self.settled_fold.is_some();
        panel
            .children(self.slide_edge(cx))
            // Rows out of view still finish their folds.
            .when(moving, |panel| panel.child(motion::next_frame()))
            .into_any_element()
    }

    // Starts the slide of the selection edge when another task of the feed opened since the
    // last frame, from the edge of the row that the last frame marked.
    fn sync_feed_slide(&mut self, cx: &mut Context<Self>) {
        if self.feed_slide.shown == self.selected {
            return;
        }
        let left = std::mem::replace(&mut self.feed_slide.shown, self.selected.clone());
        self.feed_slide.from = None;
        self.feed_slide.started = None;
        let (Some(left), Some(opened)) = (left, self.selected.as_deref()) else {
            return;
        };
        if cx.reduce_motion() || self.reveals.sidebar < 1. {
            return;
        }
        // A task of another workspace or one that the feed hides has no row to slide between.
        let listed = |id: &str| {
            self.sidebar_rows
                .iter()
                .chain(&self.settled_rows)
                .any(|row| row.task_id() == Some(id))
        };
        if !listed(&left) || !listed(opened) {
            return;
        }
        if let Some(edge) = edge_span(&left, cx) {
            self.feed_slide.from = Some((left, edge));
            self.feed_slide.started = Some(Instant::now());
        }
    }

    // The selection edge on its way to the row that opened. Its leading end springs ahead and
    // its trailing end follows, as the rail's selection does. The row draws its own edge once
    // the slide has arrived.
    fn slide_edge(&self, cx: &mut App) -> Option<AnyElement> {
        let progress = self.feed_slide.progress()?;
        let (_, from) = self.feed_slide.from.as_ref()?;
        let selected = self.selected.as_deref()?;
        let task = self
            .workspace
            .as_ref()?
            .tasks
            .iter()
            .find(|task| task.id == selected)?;
        let to = edge_span(selected, cx)?;
        let (top, bottom) = fx::stretch_span((from.1, from.2), (to.1, to.2), progress);
        Some(
            div()
                .absolute()
                .left(gpui::px(from.0 + (to.0 - from.0) * progress))
                .top(gpui::px(top))
                .w(px(2.))
                .h(gpui::px(bottom - top))
                .rounded(px(1.))
                .bg(selection_tone(task, self.theme))
                .into_any_element(),
        )
    }

    // Ends the folds that have arrived. A project group or the settled list that folded leaves
    // the feed now.
    fn settle_folds(&mut self) {
        let folded: Vec<String> = self
            .group_folds
            .iter()
            .filter(|(_, fold)| !fold.opening && fold.done())
            .map(|(id, _)| id.clone())
            .collect();
        for id in folded {
            self.group_folds.remove(&id);
            self.settings.collapsed_projects.insert(id);
            self.sidebar_dirty = true;
            self.save_settings();
        }
        if let Some(fold) = self.settled_fold.filter(|fold| fold.done()) {
            self.settled_collapsed = !fold.opening;
            self.settled_fold = None;
        }
    }

    // Folds or unfolds a project group of `rows` tasks. Its rows move one after another; a
    // group that folds leaves the feed when its last row has folded.
    fn toggle_project_group(&mut self, id: String, rows: usize, cx: &mut Context<Self>) {
        let animate = !cx.reduce_motion() && self.search.read(cx).text().is_empty();
        let folding = self.group_folds.get(&id).is_some_and(|fold| !fold.opening);
        if folding {
            self.group_folds.insert(id, GroupFold::new(true, rows));
        } else if self.settings.collapsed_projects.remove(&id) {
            if animate {
                self.group_folds.insert(id, GroupFold::new(true, rows));
            }
            self.sidebar_dirty = true;
            self.save_settings();
        } else if animate {
            self.group_folds.insert(id, GroupFold::new(false, rows));
        } else {
            self.settings.collapsed_projects.insert(id);
            self.sidebar_dirty = true;
            self.save_settings();
        }
        cx.notify();
    }

    // Folds or unfolds the settled tasks, one row after another.
    fn toggle_settled(&mut self, cx: &mut Context<Self>) {
        let rows = self.settled_rows.len();
        let animate = !cx.reduce_motion() && self.search.read(cx).text().is_empty();
        let folding = self.settled_fold.is_some_and(|fold| !fold.opening);
        if folding || self.settled_collapsed {
            self.settled_collapsed = false;
            self.settled_fold = animate.then(|| GroupFold::new(true, rows));
        } else if animate {
            self.settled_fold = Some(GroupFold::new(false, rows));
        } else {
            self.settled_collapsed = true;
            self.settled_fold = None;
        }
        cx.notify();
    }

    // Rebuilds the rows of the open workspace's feed after its tasks, filter or order changed.
    // The scroll position stays on the row it was on.
    fn sync_feed(&mut self, cx: &mut Context<Self>) {
        if !self.sidebar_dirty {
            return;
        }
        self.sidebar_dirty = false;
        let Some(workspace) = &self.workspace else {
            return;
        };
        let query = self.search.read(cx).text().to_lowercase();
        let crate::thread_list::Feed {
            open: rows,
            settled,
        } = crate::thread_list::rows(
            workspace,
            &self.settings,
            crate::thread_list::Filter {
                kind: self.mode,
                project: self.sidebar_project.as_deref(),
                query: &query,
                archived: self.show_archived,
            },
        );
        self.settled_rows = settled;
        let changed = rows.len() != self.sidebar_rows.len()
            || rows
                .iter()
                .zip(&self.sidebar_rows)
                .any(|(a, b)| !a.same_item(b));
        if changed {
            let mut anchor = self.sidebar_scroll.logical_scroll_top();
            if let Some(previous) = self.sidebar_rows.get(anchor.item_ix) {
                anchor.item_ix = rows
                    .iter()
                    .position(|row| row.same_item(previous))
                    .unwrap_or(anchor.item_ix.min(rows.len().saturating_sub(1)));
            }
            self.sidebar_scroll.reset(rows.len());
            self.sidebar_scroll.scroll_to(anchor);
        }
        self.sidebar_rows = rows;
    }
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.collapsed = false;
        self.search_visible = true;
        window.focus(&self.search.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    /// Returns the tasks of the feed in display order, for keyboard navigation. Settled tasks
    /// follow the open ones while their list is unfolded.
    pub(super) fn feed_tasks(&self, cx: &App) -> Vec<String> {
        let settled_open = self.settled_open(cx);
        self.sidebar_rows
            .iter()
            .chain(self.settled_rows.iter().filter(|_| settled_open))
            .filter_map(|row| row.task_id().map(str::to_owned))
            .collect()
    }
    /// Opens the task `offset` rows away from the open one in the feed, wrapping at the ends.
    pub(super) fn select_adjacent_task(&mut self, offset: isize, cx: &mut Context<Self>) {
        self.close_git(cx);
        let tasks = self.feed_tasks(cx);
        if tasks.is_empty() {
            return;
        }
        let count = tasks.len() as isize;
        let current = self
            .selected
            .as_ref()
            .and_then(|id| tasks.iter().position(|task| task == id));
        let next = match current {
            Some(index) => (index as isize + offset).rem_euclid(count),
            None if offset < 0 => count - 1,
            None => 0,
        };
        self.select_task(tasks[next as usize].clone(), cx);
    }
    /// Opens the next task after the open one that waits for the user or reported a failure.
    /// The tasks of the open workspace come first, in feed order; then those of the others.
    pub(super) fn select_next_attention(&mut self, cx: &mut Context<Self>) {
        self.close_git(cx);
        let Some(workspace) = &self.workspace else {
            return;
        };
        let needs_attention = |task: &core::Task| {
            !task.archived && Section::of(task, &self.settings) == Section::NeedsYou
        };
        let mut waiting: Vec<_> = self
            .feed_tasks(cx)
            .into_iter()
            .filter(|id| {
                workspace
                    .tasks
                    .iter()
                    .any(|task| &task.id == id && needs_attention(task))
            })
            .collect();
        waiting.extend(
            workspace
                .tasks
                .iter()
                .filter(|task| task.kind != self.mode && needs_attention(task))
                .map(|task| task.id.clone()),
        );
        let after = self
            .selected
            .as_ref()
            .and_then(|id| waiting.iter().position(|task| task == id))
            .map_or(0, |index| index + 1);
        if let Some(id) = waiting.get(after).or(waiting.first()) {
            self.select_task(id.clone(), cx);
        }
    }
    fn sidebar_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.sidebar_rows.get(index) else {
            return div().into_any_element();
        };
        let t = self.theme;
        match row {
            Row::Section { section, count } => {
                let header = match section {
                    Section::NeedsYou => kit::status_header(section.label(), *count, t.warning, t),
                    Section::Working => kit::status_header(section.label(), *count, t.accent, t),
                    Section::Ready => kit::status_header(section.label(), *count, t.added, t),
                    Section::Threads => kit::section_header(section.label(), Some(*count), t),
                };
                div()
                    .px(px(6.))
                    .pt(px(if index == 0 { 4. } else { 12. }))
                    .pb(px(4.))
                    .child(header)
                    .into_any_element()
            }
            Row::Task { index, project, .. } => self.task_row(*index, *project, false, cx),
            Row::Project { index, id, count } => {
                let Some(project) = self.workspace.as_ref().and_then(|w| w.projects.get(*index))
                else {
                    return div().into_any_element();
                };
                let fold = self.group_folds.get(id);
                let collapsed = (self.settings.collapsed_projects.contains(id)
                    || fold.is_some_and(|fold| !fold.opening))
                    && self.search.read(cx).text().is_empty();
                let openness = fold.map_or(if collapsed { 0. } else { 1. }, GroupFold::chevron);
                let id = id.clone();
                let rows = *count;
                div()
                    .pt(px(8.))
                    .child(
                        self.ghost_icon(
                            SharedString::from(format!("project-group:{id}")),
                            fold_chevron(openness, 13.),
                            "",
                        )
                        .w_full()
                        .h(px(26.))
                        .px(px(6.))
                        .justify_start()
                        .min_w_0()
                        .accessibility_label(format!(
                            "{} project {}",
                            if collapsed { "Expand" } else { "Collapse" },
                            project.name
                        ))
                        .child(project_icon(project, t))
                        .child(
                            caps(&project.name, t)
                                .max_w(relative(0.6))
                                .text_ellipsis()
                                .text_color(t.muted),
                        )
                        .child(figure(count.to_string(), t.subtle))
                        .child(
                            div()
                                .flex_1()
                                .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.toggle_project_group(id.clone(), rows, cx)
                        })),
                    )
                    .into_any_element()
            }
            Row::Empty(label) => kit::empty_hint(*label, t).into_any_element(),
        }
    }
    fn task_row(
        &self,
        index: usize,
        project: Option<usize>,
        settled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(workspace) = &self.workspace else {
            return div().into_any_element();
        };
        let Some(task) = workspace.tasks.get(index) else {
            return div().into_any_element();
        };
        let project = project.and_then(|project| workspace.projects.get(project));
        // List measures the root's height without its margins; keep the gap inside the row.
        let row = div()
            .pb(px(2.))
            .child(self.task_card(task, project, settled, cx));
        if cx.reduce_motion() {
            return row.into_any_element();
        }
        // A row of a project group that folds or unfolds moves with the group.
        let openness = match self.entering_tasks.get(&task.id) {
            Some(entrance) => Some(entrance.progress()),
            None => self.fold_rows.get(&task.id).copied(),
        };
        match openness {
            Some(openness) => motion::row_enter(row, openness, self.theme.sidebar),
            None => row.into_any_element(),
        }
    }
    // A search unfolds the settled tasks, so that its matches among them show.
    fn settled_open(&self, cx: &App) -> bool {
        !self.settled_collapsed || !self.search.read(cx).text().is_empty()
    }
    // The header of the settled tasks of every project. It stays above the account row.
    fn settled_header(&self, open: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        div()
            .flex_shrink_0()
            .px(px(6.))
            .py(px(4.))
            .child(
                self.control("settled")
                    .border_color(gpui::transparent_black())
                    .bg(gpui::transparent_black())
                    .hover(move |b| b.bg(t.selected))
                    .w_full()
                    .h(px(26.))
                    .px(px(6.))
                    .justify_start()
                    .text_color(t.subtle)
                    .accessibility_label(if open {
                        "Collapse settled tasks"
                    } else {
                        "Expand settled tasks"
                    })
                    .child(fold_chevron(
                        self.settled_fold
                            .map_or(if open { 1. } else { 0. }, |fold| fold.chevron()),
                        12.,
                    ))
                    .child(caps("Settled", t))
                    .child(figure(self.settled_rows.len().to_string(), t.subtle))
                    .child(
                        div()
                            .flex_1()
                            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                    )
                    .on_click(cx.listener(|s, _, _, cx| s.toggle_settled(cx))),
            )
            .into_any_element()
    }
    // The settled tasks as one list that takes the height of its rows, up to half of the panel.
    fn settled_tasks(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        uniform_list(
            "settled-tasks",
            self.settled_rows.len(),
            move |range, _, cx| {
                entity.update(cx, |s, cx| {
                    range
                        .map(|index| {
                            let row = match s.settled_rows.get(index) {
                                Some(Row::Task { index, project, .. }) => {
                                    s.task_row(*index, *project, true, cx)
                                }
                                _ => div().into_any_element(),
                            };
                            // The rows of a uniform list keep their height, so they drop in.
                            let row = match s.settled_fold.filter(|_| !cx.reduce_motion()) {
                                Some(fold) => motion::drop_in(row, fold.openness(index)),
                                None => row,
                            };
                            div().w_full().px(px(6.)).child(row)
                        })
                        .collect::<Vec<_>>()
                })
            },
        )
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .w_full()
        .min_h_0()
        .max_h(relative(0.5))
        .pb(px(4.))
        .into_any_element()
    }
    fn task_card(
        &self,
        task: &core::Task,
        project: Option<&Project>,
        settled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let selected = self.selected.as_deref() == Some(&task.id);
        let radius = crate::theme::RADIUS_CONTROL;
        let id = task.id.clone();
        let menu_id = id.clone();
        let section = Section::of(task, &self.settings);
        let needs_input = task.needs_input();
        let active = section == Section::Working;
        let blocked = task.is_blocked();
        let unread = section == Section::Ready;
        // The tile always names the project, so tasks of different projects stay apart in a
        // flat list. The state of the task leads its title instead.
        let tile = match (task.kind, project) {
            (TaskKind::Coding, Some(project)) => {
                project_tile(project, t, TASK_TILE, TASK_TILE_GLYPH)
            }
            // A review shows what it examines; a chat has no project to show.
            (TaskKind::Review, _) => glyph_tile(
                match task.review_scope.as_ref().map(|s| &s.target) {
                    Some(ReviewTarget::Branch { .. }) => "branch",
                    _ => "git-pull-request",
                },
                project.map_or(t.accent, |project| project_tint(project, t)),
                TASK_TILE,
                TASK_TILE_GLYPH,
            ),
            _ => glyph_tile("chat", t.subtle, TASK_TILE, TASK_TILE_GLYPH),
        };
        let state = if needs_input {
            Some(motion::active_icon(
                "shield",
                t.warning,
                false,
                cx.reduce_motion(),
            ))
        } else if active {
            Some(dither::pulse(t.accent, cx.reduce_motion()))
        } else if blocked {
            Some(
                icon("circle-x")
                    .size(px(13.))
                    .text_color(t.removed)
                    .into_any_element(),
            )
        } else if unread {
            Some(kit::marker(t.added).into_any_element())
        } else {
            None
        };
        let status = if needs_input {
            kit::chip("INPUT", t.warning).into_any_element()
        } else if blocked {
            kit::chip(task.state.label().to_uppercase(), t.removed).into_any_element()
        } else {
            figure(
                elapsed(task.updated),
                if active { t.accent } else { t.subtle },
            )
            .into_any_element()
        };
        let review_hint = task.review.as_ref().map_or_else(|| "No saved task diff. A snapshot will be recorded when the next turn finishes.".to_owned(), |r| {
            if let Some(error) = &r.error { format!("Could not capture task diff: {error}") }
            else { format!("Saved task diff · {} ago\n+{} added · −{} removed\n{} files omitted from inline counts. Later checkout changes do not affect these totals.", elapsed(r.captured),r.added,r.removed,r.omitted_files) }
        });
        let provider = task.provider;
        let account = self
            .workspace
            .as_ref()
            .and_then(|w| w.account(provider, task.account.as_deref()))
            .filter(|state| !state.account.is_default())
            .map(|state| {
                (
                    state.account.name.clone(),
                    self.account_tone(&state.account),
                )
            });
        let provider_hint = match &account {
            Some((name, _)) => format!("{} · {name}", provider.label()),
            None => provider.label().to_owned(),
        };
        // The second line names where the task works. A chat has no branch, so it names its
        // model instead. A folder without Git has no branch either.
        let place = if task.kind.has_project() && task.branch.is_empty() {
            "No Git".to_owned()
        } else if task.kind.has_project() {
            task.branch.clone()
        } else {
            self.model_name(&AgentPreferences {
                provider,
                model: task.model.clone(),
                ..AgentPreferences::default()
            })
        };
        let mut meta = div()
            .h(px(16.))
            .flex()
            .items_center()
            .gap(px(6.))
            .font_family(crate::theme::mono())
            .text_size(text_px(10.5))
            .text_color(if selected { t.muted } else { t.subtle })
            .child(
                div()
                    .id((
                        ElementId::from("task-provider"),
                        SharedString::from(id.clone()),
                    ))
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(icon(provider.icon()).size(px(11.)))
                    .when_some(account.as_ref(), |d, (_, tone)| {
                        d.child(div().size(px(5.)).rounded(px(1.)).bg(*tone))
                    })
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint {
                            text: provider_hint.clone(),
                            theme: t,
                        })
                        .into()
                    }),
            )
            // Settled tasks are never under a project header, so they name their project.
            .when_some(
                project.filter(|_| {
                    self.sidebar_project.is_none()
                        && (settled || !self.settings.group_tasks_by_project)
                }),
                |d, project| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .max_w(relative(0.4))
                            .text_ellipsis()
                            .child(project.name.clone()),
                    )
                    .child(div().flex_shrink_0().child("/"))
                },
            )
            .child(div().flex_1().min_w_0().text_ellipsis().child(place));
        if let Some(review) = task.review.as_ref().filter(|r| r.error.is_none()) {
            let snapshot = review.snapshot_id.clone();
            let task_id = id.clone();
            meta = meta.child(
                self.control((ElementId::from("task-diff"), SharedString::from(id.clone())))
                    .border_0()
                    .bg(gpui::transparent_black())
                    .hover(move |b| b.bg(t.selected))
                    .h(px(16.))
                    .px(px(3.))
                    .rounded(px(crate::theme::RADIUS_SMALL))
                    .disabled(snapshot.is_none())
                    .accessibility_label("Open saved task diff")
                    .when(review.omitted_files > 0, |b| b.child(figure("~", t.subtle)))
                    .child(kit::diffstat(review.added, review.removed, t))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        cx.stop_propagation();
                        s.select_task(task_id.clone(), cx);
                        s.show_pane(PanePage::Changes);
                        s.save_settings();
                        if let Some(id) = &snapshot {
                            s.historical_diff = true;
                            s.new_diff = None;
                            s.send(core::Request::LoadSnapshot { id: id.clone() });
                        }
                    }))
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint {
                            text: review_hint.clone(),
                            theme: t,
                        })
                        .into()
                    }),
            );
        }
        let tone = selection_tone(task, t);
        // While the selection edge slides, the row that opened takes its tint as the edge comes,
        // and the row that it left lets its tint fade.
        let slide = self.feed_slide.progress().map(motion::ease_out_cubic);
        let arriving = slide.filter(|_| selected);
        let leaving = slide.filter(|_| !selected && self.feed_slide.left() == Some(id.as_str()));
        let tint = arriving.unwrap_or(1.);
        div()
            .id((ElementId::from("task"), SharedString::from(id.clone())))
            .relative()
            .overflow_hidden()
            .w_full()
            .px(px(8.))
            .py(px(if self.settings.comfortable { 9. } else { 6. }))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(radius))
            .border_1()
            .border_color(gpui::transparent_black())
            .when(selected, |row| row.bg(tone.opacity(0.1 * tint)))
            .cursor_pointer()
            .when(!selected, |row| row.hover(move |d| d.bg(t.selected)))
            .when_some(leaving, |row, leaving| {
                row.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(tone.opacity(0.1 * (1. - leaving))),
                )
            })
            .when(selected || active, |row| {
                row.layer(dither::row_wash(tone.opacity(tint), selected, active))
            })
            // The open task carries an edge in its tone, not an outline.
            .when(selected && arriving.is_none(), |row| {
                row.child(kit::selection_edge(tone))
            })
            .child(fx::anchor(thread_anchor(&id)))
            .child(div().relative().flex_shrink_0().child(tile))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .h(px(20.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .when_some(state, |d, state| {
                                d.child(
                                    div()
                                        .size(px(14.))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(state),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(text_px(13.))
                                    .font_weight(if selected || unread || needs_input {
                                        FontWeight::MEDIUM
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(if selected || active || needs_input || unread {
                                        t.text
                                    } else {
                                        t.text.opacity(0.86)
                                    })
                                    .text_ellipsis()
                                    .child(task.title.clone()),
                            )
                            .when(task.pinned, |d| {
                                d.child(icon("pin").size(px(11.)).text_color(t.accent))
                            })
                            .child(div().flex_shrink_0().child(status)),
                    )
                    .child(meta),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |s, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let viewport = window.viewport_size();
                    s.thread_menu = Some(ThreadMenu {
                        task_id: menu_id.clone(),
                        position: point(
                            event
                                .position
                                .x
                                .clamp(px(8.), (viewport.width - px(244.)).max(px(8.))),
                            event
                                .position
                                .y
                                .clamp(px(8.), (viewport.height - px(192.)).max(px(8.))),
                        ),
                    });
                    s.panel = Panel::ThreadMenu;
                    window.focus(&s.focus, cx);
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |s, _, _, cx| s.select_task(id.clone(), cx)))
            .into_any_element()
    }
    /// Returns the tint that identifies an account on rows and chips.
    pub(super) fn account_tone(&self, account: &Account) -> Hsla {
        account.color.map_or(self.theme.info, |color| {
            crate::theme::accent_color(color, self.theme.light)
        })
    }
}
/// Returns a stable, distinct project glyph and tint without filesystem work during rendering.
pub(super) fn project_icon(project: &Project, theme: Theme) -> AnyElement {
    project_tile(project, theme, 16., 11.)
}

/// Returns the project's icon at the size that leads a task row.
pub(super) fn project_task_tile(project: &Project, theme: Theme) -> AnyElement {
    project_tile(project, theme, TASK_TILE, TASK_TILE_GLYPH)
}

// The glyphs that a project without a chosen icon can get. A project keeps its glyph while the
// order and the length of this list stay the same.
const AUTOMATIC_GLYPHS: [ProjectIcon; 8] = [
    ProjectIcon::Code,
    ProjectIcon::Cube,
    ProjectIcon::Book,
    ProjectIcon::Branch,
    ProjectIcon::Terminal,
    ProjectIcon::Folder,
    ProjectIcon::Cog,
    ProjectIcon::File,
];
// The glyph color of a tile: the paper white of the application icon.
const TILE_PAPER: u32 = 0xefe8dc;

// Returns the corner radius of a square tile of `size` points.
fn tile_radius(size: f32) -> f32 {
    (size * 0.27).round()
}

// Returns the project's icon as a square tile of `size` points. A project without an image shows
// a glyph of `glyph_size` points.
fn project_tile(project: &Project, theme: Theme, size: f32, glyph_size: f32) -> AnyElement {
    if let ProjectIcon::Image(path) = &project.icon {
        return img(path.clone())
            .size(px(size))
            .rounded(px(tile_radius(size)))
            .flex_shrink_0()
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                icon("file")
                    .size(px(size))
                    .text_color(theme.accent)
                    .into_any_element()
            })
            .into_any_element();
    }
    let glyph = project.icon.glyph().unwrap_or_else(|| {
        let name = project.name.to_lowercase();
        let automatic = if name.contains("unity") {
            ProjectIcon::Cube
        } else if name.contains("kodem") {
            ProjectIcon::Code
        } else if name.contains("liquid") {
            ProjectIcon::Branch
        } else if name.contains("quill") {
            ProjectIcon::Book
        } else {
            let hash = project
                .path
                .to_string_lossy()
                .bytes()
                .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
            AUTOMATIC_GLYPHS[hash as usize % AUTOMATIC_GLYPHS.len()].clone()
        };
        automatic.glyph().unwrap_or_default()
    });
    glyph_tile(glyph, project_tint(project, theme), size, glyph_size)
}

// Returns a square tile of `size` points with a glyph of `glyph_size` points. The tile is a dark
// plate in both appearances, like an application icon, so that glyph tiles and image tiles look
// alike in one list. The tint colors the plate, its edge and a glow of fine dots in its leading
// top corner. The glyph is paper white.
fn glyph_tile(glyph: &'static str, tint: Hsla, size: f32, glyph_size: f32) -> AnyElement {
    let radius = tile_radius(size);
    // The tint of the light appearance is too dark to read on the plate.
    let mut glow = tint;
    glow.lightness = glow.lightness.max(0.66);
    let mut plate = tint;
    plate.saturation = plate.saturation.min(0.4);
    plate.lightness = 0.055;
    div()
        .relative()
        .size(px(size))
        .rounded(px(radius))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .border_1()
        .border_color(glow.opacity(0.3))
        .bg(plate)
        .layer(
            Dither::new(DitherField::Glow {
                x: 0.,
                y: 0.,
                radius: 0.75,
                strength: 0.85,
            })
            .color(glow.opacity(0.45))
            .grid(1., 1.),
        )
        .child(
            div().relative().child(
                icon(glyph)
                    .size(px(glyph_size))
                    .text_color(rgb_to_hsla(rgb(TILE_PAPER))),
            ),
        )
        .into_any_element()
}

/// Returns readable theme variants for named colors and preserves legacy custom RGB colors.
pub(super) fn project_tint(project: &Project, theme: Theme) -> Hsla {
    project.color.map_or(theme.accent, |color| {
        AccentColor::ALL
            .into_iter()
            .find(|accent| accent.rgb(false) == color)
            .map_or_else(
                || rgb_to_hsla(rgb(color)),
                |accent| crate::theme::accent_color(accent, theme.light),
            )
    })
}

/// Returns the age of a timestamp as one short figure, such as `12m` or `3h`.
pub(super) fn elapsed(time: u64) -> String {
    let s = now().saturating_sub(time) / 1000;
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86400 {
        format!("{}h", s / 3600)
    } else {
        format!("{}d", s / 86400)
    }
}
