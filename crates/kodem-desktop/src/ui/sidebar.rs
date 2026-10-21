use super::*;
use crate::thread_list::{Attention, Row, Section};
use dither::{Dither, DitherField, Edge};
use std::any::Any;
use std::time::Instant;

/// Returns the effect anchor name of one thread row.
pub(super) fn thread_anchor(task_id: &str) -> SharedString {
    SharedString::from(format!("thread:{task_id}"))
}

/// Side of a project's large tile, as the project views show it.
const TASK_TILE: f32 = 30.;
/// Size of a glyph in that tile.
const TASK_TILE_GLYPH: f32 = 18.;
/// Side of the small project tile that leads a settled task's row, whose project has no group
/// around it. It sits beside the title only.
const ROW_TILE: f32 = 16.;
/// Size of a glyph in that tile.
const ROW_TILE_GLYPH: f32 = 11.;
/// Side of the tile that leads the header of a project group.
const GROUP_TILE: f32 = 20.;
/// Size of a glyph in that tile.
const GROUP_TILE_GLYPH: f32 = 13.;
/// Height of the header of a project group.
const GROUP_HEADER: f32 = 34.;
/// Alpha of the project's tint under a group's header and rows. It only hints at the surface
/// that holds them.
const GROUP_FILL: f32 = 0.05;
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
/// Height of the band at each end of the feed where a dragged project group scrolls the feed.
const DRAG_SCROLL_EDGE: f32 = 48.;
/// How fast the feed scrolls under a dragged project group at the very end of the feed, in
/// window pixels per second.
const DRAG_SCROLL_SPEED: f32 = 900.;

/// What a drag of a project group carries: the project, which the drag's preview shows.
#[derive(Clone)]
pub(super) struct ProjectDrag {
    project: Project,
    theme: Theme,
}

impl Render for ProjectDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(GROUP_HEADER))
            .px(px(8.))
            .rounded(px(crate::theme::RADIUS_SURFACE))
            .bg(t.raised)
            .border_1()
            .border_color(project_tint(&self.project, t).opacity(0.6))
            .shadow(t.floating())
            .font_family(crate::theme::sans())
            .text_size(text_px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.text)
            .child(project_tile(&self.project, t, GROUP_TILE, GROUP_TILE_GLYPH))
            .child(self.project.name.clone())
    }
}

/// A project group on its way to another place in the feed. Near an end of the feed, the feed
/// scrolls under it.
pub(super) struct FeedDrag {
    project: String,
    /// How fast the feed scrolls, in window pixels per second. A negative speed scrolls up.
    speed: f32,
    /// When the feed last scrolled under the drag.
    scrolled: Instant,
}

// Tells whether a drag carries the group of a project other than `project`, which a drop on
// `project`'s group moves into its place.
fn carries_other_project(value: &dyn Any, project: &str) -> bool {
    value
        .downcast_ref::<ProjectDrag>()
        .is_some_and(|drag| drag.project.id != project)
}

// Returns where a menu that the pointer opens at `position` stands, so that a menu of `size`
// stays in the window.
fn pointer_menu_position(
    position: Point<Pixels>,
    size: Size<Pixels>,
    window: &Window,
) -> Point<Pixels> {
    let viewport = window.viewport_size();
    point(
        position
            .x
            .clamp(px(8.), (viewport.width - size.width).max(px(8.))),
        position
            .y
            .clamp(px(8.), (viewport.height - size.height).max(px(8.))),
    )
}

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
        self.scroll_feed_drag(cx);
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
                .shadow(kit::recessed_light(t))
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
                    .on_drag_move(cx.listener(|s, event: &DragMoveEvent<ProjectDrag>, _, cx| {
                        s.drag_feed(event, cx)
                    }))
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
            || self.settled_fold.is_some()
            || self.feed_drag.as_ref().is_some_and(|drag| drag.speed != 0.);
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

    /// Tells whether a project's group is folded or folding, as its header shows it. A search
    /// unfolds every group.
    pub(super) fn project_folded(&self, id: &str, cx: &App) -> bool {
        (self.settings.collapsed_projects.contains(id)
            || self.group_folds.get(id).is_some_and(|fold| !fold.opening))
            && self.search.read(cx).text().is_empty()
    }

    /// Folds or unfolds a project group of `rows` tasks. Its rows move one after another; a
    /// group that folds leaves the feed when its last row has folded.
    pub(super) fn toggle_project_group(&mut self, id: String, rows: usize, cx: &mut Context<Self>) {
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

    /// Moves a project's group into the place of another project's group and saves the order of
    /// every project. Projects without a group in the feed keep their places among the others.
    pub(super) fn move_project(&mut self, id: &str, target: &str, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let mut projects: Vec<&Project> = workspace.projects.iter().collect();
        projects.sort_by_key(|project| crate::thread_list::project_rank(project, &self.settings));
        let mut order: Vec<String> = projects
            .into_iter()
            .map(|project| project.id.clone())
            .collect();
        let (Some(from), Some(to)) = (
            order.iter().position(|project| project == id),
            order.iter().position(|project| project == target),
        ) else {
            return;
        };
        if from == to {
            return;
        }
        // The target shifts up when the moved project leaves above it, so a project that moves
        // down lands under the target and one that moves up lands over it.
        let moved = order.remove(from);
        order.insert(to, moved);
        self.settings.project_order = order;
        self.sidebar_dirty = true;
        self.save_settings();
        cx.notify();
    }

    /// Returns the project groups of the feed above and below a project's group.
    pub(super) fn project_neighbors(&self, id: &str) -> (Option<String>, Option<String>) {
        let groups: Vec<&str> = self
            .sidebar_rows
            .iter()
            .filter_map(|row| match row {
                Row::Project { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        let Some(index) = groups.iter().position(|group| *group == id) else {
            return (None, None);
        };
        (
            index.checked_sub(1).map(|above| groups[above].to_owned()),
            groups.get(index + 1).map(|below| (*below).to_owned()),
        )
    }

    // Follows a project group that a drag carries: its header dims, and the feed scrolls while
    // the pointer is near one of the feed's ends, the faster the nearer.
    fn drag_feed(&mut self, event: &DragMoveEvent<ProjectDrag>, cx: &mut Context<Self>) {
        let edge = DRAG_SCROLL_EDGE * crate::theme::ui_scale();
        let position = event.event.position;
        let above = f32::from(position.y - event.bounds.top());
        let below = f32::from(event.bounds.bottom() - position.y);
        let beside = position.x >= event.bounds.left() && position.x <= event.bounds.right();
        let speed = if !beside {
            0.
        } else if above < edge {
            -DRAG_SCROLL_SPEED * (1. - above.max(0.) / edge)
        } else if below < edge {
            DRAG_SCROLL_SPEED * (1. - below.max(0.) / edge)
        } else {
            0.
        };
        let project = &event.drag(cx).project.id;
        let changed = match self
            .feed_drag
            .as_mut()
            .filter(|drag| drag.project == *project)
        {
            Some(drag) => {
                let started = (drag.speed == 0.) != (speed == 0.);
                drag.speed = speed;
                started
            }
            None => {
                self.feed_drag = Some(FeedDrag {
                    project: project.clone(),
                    speed,
                    scrolled: Instant::now(),
                });
                true
            }
        };
        // A speed that changes while the feed scrolls takes effect on the next frame anyway.
        if changed {
            cx.notify();
        }
    }

    // Scrolls the feed under a dragged project group by the time since the last frame. A drag
    // that ended leaves no state behind.
    fn scroll_feed_drag(&mut self, cx: &App) {
        if !cx.has_active_drag() {
            self.feed_drag = None;
            return;
        }
        let Some(drag) = &mut self.feed_drag else {
            return;
        };
        let now = Instant::now();
        // A long frame does not throw the feed far.
        let elapsed = now.duration_since(drag.scrolled).as_secs_f32().min(0.05);
        drag.scrolled = now;
        if drag.speed != 0. {
            self.sidebar_scroll
                .scroll_by(gpui::px(drag.speed * elapsed));
        }
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
            Row::Task {
                index,
                project,
                grouped,
                last,
                ..
            } => self.task_row(*index, *project, *grouped, *last, cx),
            Row::Project {
                index: project,
                id,
                count,
                attention,
            } => {
                let (id, count, attention) = (id.clone(), *count, *attention);
                match self
                    .workspace
                    .as_ref()
                    .and_then(|w| w.projects.get(*project))
                {
                    Some(project) => div()
                        .pt(px(if index == 0 { 2. } else { 10. }))
                        .child(self.project_header(project, id, count, attention, cx))
                        .into_any_element(),
                    None => div().into_any_element(),
                }
            }
            Row::Empty(label) => kit::empty_hint(*label, t).into_any_element(),
        }
    }
    // The header of a project group: the project's tile over a dithered glow in its tint, its
    // name, a dotted rule to the counts of its tasks in the states that need a person, and the
    // fold chevron. A folded group whose tasks run keeps their motion on its header.
    fn project_header(
        &self,
        project: &Project,
        id: String,
        count: usize,
        attention: Attention,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let tint = project_tint(project, t);
        let searching = !self.search.read(cx).text().is_empty();
        let fold = self.group_folds.get(&id);
        let collapsed = self.project_folded(&id, cx);
        // A group that folds keeps its rows until the last has folded, so the header closes the
        // group's surface only then.
        let closed = self.settings.collapsed_projects.contains(&id) && !searching;
        let openness = fold.map_or(if collapsed { 0. } else { 1. }, GroupFold::chevron);
        let radius = px(crate::theme::RADIUS_SURFACE);
        let reduce_motion = cx.reduce_motion();
        let label = format!(
            "{} project {}",
            if collapsed { "Expand" } else { "Collapse" },
            project.name
        );
        // A press that moves drags the group to another place; the group stays dim until the
        // drop.
        let dragged = self
            .feed_drag
            .as_ref()
            .is_some_and(|drag| drag.project == id);
        let drag = ProjectDrag {
            project: project.clone(),
            theme: t,
        };
        let (drop_id, target, menu_id) = (id.clone(), id.clone(), id.clone());
        self.control(SharedString::from(format!("project-group:{id}")))
            .w_full()
            .min_w_0()
            .h(px(GROUP_HEADER))
            .px(px(6.))
            .gap(px(8.))
            .justify_start()
            .overflow_hidden()
            .font_family(crate::theme::sans())
            .border_color(gpui::transparent_black())
            .bg(tint.opacity(GROUP_FILL))
            .when(closed, |header| header.rounded(radius))
            .when(!closed, |header| header.rounded_t(radius).rounded_b(px(0.)))
            .hover(move |s| s.bg(tint.opacity(GROUP_FILL * 2.)))
            .when(dragged, |header| header.opacity(0.45))
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .can_drop(move |value, _, _| carries_other_project(value, &drop_id))
            .drag_over::<ProjectDrag>(move |style, _, _, _| {
                style
                    .border_color(tint.opacity(0.7))
                    .bg(tint.opacity(GROUP_FILL * 3.))
            })
            .on_drop(cx.listener(move |s, drag: &ProjectDrag, _, cx| {
                s.move_project(&drag.project.id, &target, cx)
            }))
            // The glow behind the tile runs on down the group as its spine.
            .layer(
                Dither::new(
                    DitherField::Glow {
                        x: 0.,
                        y: 0.5,
                        radius: 0.4,
                        strength: 0.8,
                    }
                    .times(DitherField::Fade {
                        from: Edge::Left,
                        start: 1.,
                        end: 0.,
                    }),
                )
                .color(tint.opacity(0.32))
                .grid(2., 1.)
                .toned(),
            )
            .layer(kit::glint(t.dot, Edge::Bottom))
            .when(
                collapsed && attention.working > 0 && !reduce_motion,
                |header| {
                    header.layer(
                        Dither::new(
                            DitherField::Flow {
                                scale: 0.2,
                                speed: 1.3,
                                bias: -0.08,
                            }
                            .times(DitherField::Fade {
                                from: Edge::Right,
                                start: 0.9,
                                end: 0.,
                            }),
                        )
                        .color(t.accent.opacity(0.45))
                        .grid(2., 1.)
                        .animated(),
                    )
                },
            )
            .accessibility_label(label)
            .child(project_tile(project, t, GROUP_TILE, GROUP_TILE_GLYPH))
            .child(
                div()
                    .min_w_0()
                    .text_size(text_px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text)
                    .text_ellipsis()
                    .child(project.name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(8.))
                    .child(dither::DottedRule::horizontal(tint.opacity(0.3))),
            )
            .children(attention_badges(attention, reduce_motion, t))
            .child(figure(count.to_string(), t.subtle))
            .child(fold_chevron(openness, 12.).text_color(t.subtle))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |s, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    s.project_menu = Some(ProjectMenu {
                        project_id: menu_id.clone(),
                        tasks: count,
                        position: pointer_menu_position(
                            event.position,
                            size(px(244.), px(330.)),
                            window,
                        ),
                    });
                    s.panel = Panel::ProjectMenu;
                    window.focus(&s.focus, cx);
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |s, _, _, cx| s.toggle_project_group(id.clone(), count, cx)))
            .into_any_element()
    }
    fn task_row(
        &self,
        index: usize,
        project: Option<usize>,
        grouped: bool,
        last: bool,
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
            .child(self.task_card(task, project, grouped, cx));
        // A row of a project group that folds or unfolds moves with the group.
        let openness = match self.entering_tasks.get(&task.id) {
            Some(entrance) => Some(entrance.progress()),
            None => self.fold_rows.get(&task.id).copied(),
        };
        let row = match openness.filter(|_| !cx.reduce_motion()) {
            Some(openness) => motion::row_enter(row, openness, self.theme.sidebar),
            None => row.into_any_element(),
        };
        // The spine of the group stays beside a row while it folds, so the group shrinks as one.
        match project.filter(|_| grouped) {
            Some(project) => {
                let tint = project_tint(project, self.theme);
                let (drop_id, target) = (project.id.clone(), project.id.clone());
                group_body(tint, last)
                    // A dropped project group takes this group's place, as on its header.
                    .can_drop(move |value, _, _| carries_other_project(value, &drop_id))
                    .drag_over::<ProjectDrag>(move |style, _, _, _| {
                        style.bg(tint.opacity(GROUP_FILL * 3.))
                    })
                    .on_drop(cx.listener(move |s, drag: &ProjectDrag, _, cx| {
                        s.move_project(&drag.project.id, &target, cx)
                    }))
                    .child(row)
                    .into_any_element()
            }
            None => row,
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
                                Some(Row::Task {
                                    index,
                                    project,
                                    grouped,
                                    last,
                                    ..
                                }) => s.task_row(*index, *project, *grouped, *last, cx),
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
        grouped: bool,
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
        // A row in its project's card leads with its state. A settled row has no card around
        // it, so it leads with its project's tile instead; a chat has no project.
        let tile = match (task.kind, project) {
            _ if grouped => None,
            (TaskKind::Coding, Some(project)) => Some(project_icon(project, t)),
            // A review shows what it examines.
            (TaskKind::Review, _) => Some(glyph_tile(
                match task.review_scope.as_ref().map(|s| &s.target) {
                    Some(ReviewTarget::Branch { .. }) => "branch",
                    _ => "git-pull-request",
                },
                project.map_or(t.accent, |project| project_tint(project, t)),
                ROW_TILE,
                ROW_TILE_GLYPH,
            )),
            _ => None,
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
        // Most tasks run on the default provider and account, so a row names its provider only
        // where it differs.
        let other_provider = account.is_some() || provider != self.settings.agent_defaults.provider;
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
            .when(other_provider, |d| {
                d.child(
                    div()
                        .id((
                            ElementId::from("task-provider"),
                            SharedString::from(id.clone()),
                        ))
                        .flex()
                        .flex_shrink_0()
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
            })
            // A row outside its project's card names its project in the project's tint.
            .when_some(project.filter(|_| !grouped), |d, project| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .max_w(relative(0.45))
                        .text_ellipsis()
                        .text_color(project_tint(project, t))
                        .child(project.name.clone()),
                )
                .child(div().flex_shrink_0().child("/"))
            })
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
                    .child(row_diffstat(review.added, review.removed, t))
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
        let full_title = task.title.clone();
        div()
            .id((ElementId::from("task"), SharedString::from(id.clone())))
            .relative()
            .overflow_hidden()
            .w_full()
            .px(px(8.))
            .py(px(if self.settings.comfortable { 9. } else { 6. }))
            .flex()
            .items_start()
            .gap(px(8.))
            .rounded(px(radius))
            .border_1()
            .border_color(gpui::transparent_black())
            .when(selected, |row| row.bg(tone.opacity(0.1 * tint)))
            .cursor_pointer()
            .when(!selected, |row| {
                row.hover(move |d| d.bg(t.selected))
                    .active(move |d| d.bg(t.control_active))
            })
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                fx::ripple(event.position, t.muted, cx)
            })
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
            // The tile or the state stands beside the title only, and the line under the title
            // hangs from it. A row without a state keeps the place, so titles line up.
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .h(px(20.))
                    .min_w(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .map(|lead| match tile {
                        Some(tile) => lead.child(tile),
                        None => lead.children(state),
                    }),
            )
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
                            .gap(px(6.))
                            .child(
                                div()
                                    .id("thread-title")
                                    .flex_1()
                                    .min_w_0()
                                    // A long title keeps its size and shows in full on hover.
                                    .tooltip(move |_, cx| {
                                        cx.new(|_| Hint {
                                            text: full_title.to_string(),
                                            theme: t,
                                        })
                                        .into()
                                    })
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
                    s.thread_menu = Some(ThreadMenu {
                        task_id: menu_id.clone(),
                        position: pointer_menu_position(
                            event.position,
                            size(px(244.), px(192.)),
                            window,
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
    project_tile(project, theme, ROW_TILE, ROW_TILE_GLYPH)
}

/// Returns the project's large icon, as the project views show it.
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

// One slice of a project group under its header. Consecutive rows draw consecutive slices of
// the group's faint surface and of a dithered spine in the project's tint, which carries the
// glow of the header's tile down the group and thins out under its last row. The last slice
// rounds the surface off over a shade of dots.
fn group_body(tint: Hsla, last: bool) -> Div {
    let across = DitherField::Fade {
        from: Edge::Left,
        start: 0.55,
        end: -0.15,
    };
    let spine = if last {
        across.times(DitherField::Fade {
            from: Edge::Top,
            start: 1.,
            end: 0.,
        })
    } else {
        across
    };
    let radius = crate::theme::RADIUS_SURFACE;
    div()
        .relative()
        .pl(px(6.))
        .pr(px(3.))
        .bg(tint.opacity(GROUP_FILL))
        .when(last, |body| {
            body.pb(px(3.)).rounded_b(px(radius)).child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(12.))
                    .child(
                        Dither::new(DitherField::Fade {
                            from: Edge::Bottom,
                            start: 0.35,
                            end: 0.,
                        })
                        .color(tint.opacity(0.3))
                        .grid(2., 1.)
                        .corner_radii(Corners {
                            bottom_left: radius,
                            bottom_right: radius,
                            ..Corners::default()
                        }),
                    ),
            )
        })
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(4.))
                .w(px(14.))
                .child(Dither::new(spine).color(tint.opacity(0.4)).grid(2., 1.)),
        )
}

// The counts of a project group's tasks in each state that needs a person, in the tones of the
// rows: input or failures, running work and unread results.
fn attention_badges(attention: Attention, reduce_motion: bool, theme: Theme) -> Vec<AnyElement> {
    let badge = |mark: AnyElement, count: usize, tone: Hsla| {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(3.))
            .child(mark)
            .child(figure(count.to_string(), tone).font_weight(FontWeight::SEMIBOLD))
            .into_any_element()
    };
    let mut badges = Vec::new();
    if attention.needs_you > 0 {
        badges.push(badge(
            icon("shield")
                .size(px(11.))
                .text_color(theme.warning)
                .into_any_element(),
            attention.needs_you,
            theme.warning,
        ));
    }
    if attention.working > 0 {
        badges.push(badge(
            dither::pulse(theme.accent, reduce_motion),
            attention.working,
            theme.accent,
        ));
    }
    if attention.ready > 0 {
        badges.push(badge(
            kit::marker(theme.added).into_any_element(),
            attention.ready,
            theme.added,
        ));
    }
    badges
}

// The saved diff of a task row as short "+N −N" counts. Counts of a thousand lines or more
// shorten, so that the column of counts keeps one width down the feed.
fn row_diffstat(added: usize, removed: usize, theme: Theme) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(5.))
        .when(added > 0, |d| {
            d.child(figure(format!("+{}", short_count(added)), theme.added))
        })
        .when(removed > 0, |d| {
            d.child(figure(format!("−{}", short_count(removed)), theme.removed))
        })
}

// Returns a count in a few characters, such as `987`, `1.8k`, `69k` or `1.2M`. It rounds down,
// so a count never shows more than it is.
fn short_count(count: usize) -> String {
    if count < 1_000 {
        count.to_string()
    } else if count < 10_000 {
        format!("{}.{}k", count / 1_000, count / 100 % 10)
    } else if count < 1_000_000 {
        format!("{}k", count / 1_000)
    } else if count < 10_000_000 {
        format!("{}.{}M", count / 1_000_000, count / 100_000 % 10)
    } else {
        format!("{}M", count / 1_000_000)
    }
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
