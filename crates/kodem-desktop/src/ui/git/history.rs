//! The History tab: the commit graph with its lanes, an activity header, search, and the
//! selected commit's files and diff. Commits and branches can be dragged onto commits to
//! rebase, merge or cherry-pick, and each commit has a menu of Git actions.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    App, BorderStyle, Bounds, ClipboardItem, Context, Entity, Hsla, IntoElement, PathBuilder,
    Pixels, Render, ScrollHandle, SharedString, Window, canvas, div, fill, point, quad, size,
};
use kodem_git::ai::{AiConfig, AiService};
use kodem_git::git::{Commit, CommitFile, GitRepo, GraphEdge, RefKind};
use kodem_git::settings::{
    DateFormat, MAX_DIFF_PANEL_HEIGHT, MAX_HISTORY_FILE_PANEL_WIDTH, MIN_DIFF_PANEL_HEIGHT,
    MIN_HISTORY_FILE_PANEL_WIDTH,
};
use kodem_git::util;

use crate::theme::{px, text_px};
use crate::ui::dither::{Dither, DitherField, DottedRule, cell_threshold, checker};
use crate::ui::git::ai::{
    AiAgentState, AiResult, ai_action_group, open_ai_result_dialog, run_ai_result_task,
};
use crate::ui::git::components::{
    self, author_avatar, button, change_kind_chip, full_size, mono_font, ref_badge, stat_chips,
};
use crate::ui::git::contextual_actions::{
    CreateBranchDialogView, RebaseDialogView, branch_folder_prefixes, open_liquid_dialog,
    open_rebase_dialog, reset_to_here_menu,
};
use crate::ui::git::diff::{DiffCloseRequested, DiffView, PreparedDiff};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, ContextMenuExt as _, Icon, IconName, InputState,
    PopupMenuItem, ResizableState, ScrollableElement as _, Sizable as _, Size,
    VirtualListScrollHandle, h_flex, h_resizable, resizable_panel, v_flex, v_resizable,
    v_virtual_list,
};
use crate::ui::git::motion;
use crate::ui::git::ref_drag::{
    DraggedRef, DropMenu, DropTarget, RefDragExt as _, drag_layer, drag_role, open_drop_menu,
};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::state::RepositoryState;
use crate::ui::git::theme;

const LANE_WIDTH: f32 = 18.0;
const RAIL_LEADING: f32 = 8.0;
const RAIL_TRAILING: f32 = 10.0;
const MAX_LANES: usize = 5;
const NODE_RADIUS: f32 = 4.6;
/// Grid pitch of the dithered glow around graph lanes, matching the
/// HEAD halo.
const GLOW_PITCH: f32 = 2.0;
/// How far the lane glow reaches from the lane center.
const GLOW_REACH: f32 = 7.0;
/// Distance from the lane center where the glow starts, past the stroke.
const GLOW_CORE: f32 = 1.5;
/// Glow intensity right next to the stroke, before dithering.
const GLOW_STRENGTH: f32 = 0.7;
const GLOW_ALPHA: f32 = 0.3;
/// Points sampled along a lane bend.
const CURVE_SAMPLES: usize = 16;

/// Advance of one mono glyph at the 11 point metadata size (0.6 em). The mono columns scale
/// it with the text size, so their text keeps fitting when the text grows.
const MONO_ADVANCE: f32 = 6.6;
/// Gap between the metadata columns, matching `gap_3`.
const COLUMN_GAP: f32 = 12.0;
const AUTHOR_COLUMN: f32 = 120.0;
/// Fits abbreviated hashes up to 11 characters; longer ones are cut.
const HASH_CHARS: f32 = 11.0;
const STATS_COLUMN: f32 = 128.0;
const AVATAR_SIZE: f32 = 20.0;
const AVATAR_LEADING: f32 = 8.0;
/// Height of a commit row. Kodem has no compact density, so rows keep Liquid's regular height.
const ROW_HEIGHT: f32 = 38.0;

/// Days covered by the activity chart above the commit list.
const ACTIVITY_DAYS: i64 = 84;
/// Number of columns in the activity chart.
const ACTIVITY_BINS: usize = 42;
const SECONDS_PER_DAY: i64 = 86_400;
/// Tallest the commit description gets in the detail panel before it
/// scrolls.
const DETAIL_BODY_MAX_HEIGHT: f32 = 84.0;

/// Facts shown in the header above the commit list.
#[derive(Debug, Clone, PartialEq)]
struct ActivitySummary {
    commits: usize,
    authors: usize,
    newest: Option<i64>,
    /// Commit counts per bin, oldest first, normalized so the busiest bin
    /// is `1.0`.
    bins: Vec<f32>,
    /// Commits inside the chart window.
    recent: usize,
}

/// Counts commits per bin over the `ACTIVITY_DAYS` before the newest
/// commit. The window ends at the newest commit, not at the current
/// time, so quiet repositories still show their last period of work.
fn activity_summary(commits: &[Commit]) -> ActivitySummary {
    let newest = commits.iter().map(|commit| commit.committer_date).max();
    let authors = commits
        .iter()
        .map(|commit| commit.author_email.trim().to_lowercase())
        .collect::<HashSet<_>>()
        .len();
    let mut counts = [0_u32; ACTIVITY_BINS];
    let mut recent = 0;
    if let Some(newest) = newest {
        let span = ACTIVITY_DAYS * SECONDS_PER_DAY;
        let start = newest - span;
        for commit in commits {
            if commit.committer_date < start {
                continue;
            }
            let offset = (commit.committer_date - start) as f64 / span as f64;
            let bin = ((offset * ACTIVITY_BINS as f64) as usize).min(ACTIVITY_BINS - 1);
            counts[bin] += 1;
            recent += 1;
        }
    }
    let max = counts.iter().copied().max().unwrap_or(0).max(1) as f32;
    ActivitySummary {
        commits: commits.len(),
        authors,
        newest,
        bins: counts.iter().map(|count| *count as f32 / max).collect(),
        recent,
    }
}

/// Formats a count with thin group separators, for example `12,480`.
fn grouped_count(value: usize) -> String {
    let digits = value.to_string();
    let mut grouped = String::new();
    for (ix, digit) in digits.chars().enumerate() {
        if ix > 0 && (digits.len() - ix).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// One stat tile in the activity header: a tiny label over a mono value.
fn activity_stat(label: &'static str, value: impl Into<SharedString>, cx: &App) -> gpui::Div {
    v_flex()
        .flex_shrink_0()
        .gap_0p5()
        .child(components::mono_label(label, cx))
        .child(
            div()
                .font_family(mono_font())
                .text_size(text_px(16.))
                .line_height(text_px(20.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .whitespace_nowrap()
                .child(value.into()),
        )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CommitRowStatus {
    is_head: bool,
    is_unpushed: bool,
}

impl CommitRowStatus {
    fn for_commit(commit: &Commit, unpushed_commits: &HashSet<String>) -> Self {
        Self {
            is_head: commit.refs.iter().any(|item| item.kind == RefKind::Head),
            is_unpushed: unpushed_commits.contains(&commit.hash),
        }
    }
}

fn commit_status_badge(icon: Icon, label: &'static str, color: Hsla) -> gpui::Div {
    h_flex()
        .flex_shrink_0()
        .h(px(18.))
        .gap_1()
        .px_1p5()
        .rounded(px(4.))
        .border_1()
        .border_dashed()
        .border_color(color.opacity(0.55))
        .bg(color.opacity(0.08))
        .font_family(mono_font())
        .text_size(text_px(11.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(color)
        .child(icon.xsmall().text_color(color))
        .child(label)
}

/// The commit history of a repository with the selected commit's details.
pub struct HistoryView {
    state: Entity<RepositoryState>,
    /// The search field that filters commits by message, author or hash.
    pub search: Entity<InputState>,
    scroll: VirtualListScrollHandle,
    selected_hash: Option<String>,
    detail_files: Vec<CommitFile>,
    detail_selected_file: Option<usize>,
    detail_diff: Entity<DiffView>,
    detail_loading: bool,
    detail_split: Entity<ResizableState>,
    detail_content_split: Entity<ResizableState>,
    /// Scroll position of the selected commit's description.
    detail_body_scroll: ScrollHandle,
    /// Restarts the detail panel reveal for each newly selected commit.
    detail_sequence: u64,
    /// The menu that opened where a commit or branch was dropped.
    drop_menu: Option<DropMenu>,
}

impl HistoryView {
    /// Creates the view for a repository.
    pub fn new(
        state: Entity<RepositoryState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search commits..."));
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let detail_diff = cx.new(|_| DiffView::new());
        detail_diff.update(cx, |diff, _| diff.closable = true);
        cx.subscribe(&detail_diff, |this, _, _: &DiffCloseRequested, cx| {
            this.close_detail(cx);
        })
        .detach();
        Self {
            state,
            search,
            scroll: VirtualListScrollHandle::new(),
            selected_hash: None,
            detail_files: Vec::new(),
            detail_selected_file: None,
            detail_diff,
            detail_loading: false,
            detail_split: cx.new(|_| ResizableState::default()),
            detail_content_split: cx.new(|_| ResizableState::default()),
            detail_body_scroll: ScrollHandle::new(),
            detail_sequence: 0,
            drop_menu: None,
        }
    }

    fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.selected_hash = None;
        self.detail_files.clear();
        self.detail_selected_file = None;
        self.detail_diff.update(cx, |diff, cx| diff.clear(cx));
        cx.notify();
    }

    fn run_git(
        &mut self,
        label: &str,
        operation: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let hide = GitSettings::get(cx).hide_checkpoint_commits;
        self.state.update(cx, |state, cx| {
            state.run_operation(label, hide, operation, cx);
        });
    }

    fn open_create_branch_dialog(
        &mut self,
        start_ref: String,
        detail: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prefixes = branch_folder_prefixes(&self.state.read(cx).branches);
        let state = self.state.clone();
        let view = cx
            .new(|cx| CreateBranchDialogView::new(state, start_ref, detail, prefixes, window, cx));
        let focus_view = view.clone();
        open_liquid_dialog(view, px(420.), window, cx);
        window.defer(cx, move |window, cx| {
            let focus_handle = focus_view.read(cx).input_focus_handle(cx);
            focus_handle.focus(window, cx);
        });
    }

    fn open_rebase_dialog(
        &mut self,
        upstream_ref: String,
        source: String,
        interactive: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.clone();
        let target = self.state.read(cx).current_branch.clone();
        let view = cx
            .new(|cx| RebaseDialogView::new(state, upstream_ref, source, target, interactive, cx));
        open_rebase_dialog(view, window, cx);
    }

    fn filtered_indices(&self, cx: &App) -> Vec<usize> {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let state = self.state.read(cx);
        if query.is_empty() {
            return (0..state.commits.len()).collect();
        }
        state
            .commits
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.subject.to_lowercase().contains(&query)
                    || c.author_name.to_lowercase().contains(&query)
                    || c.hash.starts_with(&query)
                    || c.short_hash.starts_with(&query)
            })
            .map(|(ix, _)| ix)
            .collect()
    }

    fn select_commit(&mut self, hash: String, cx: &mut Context<Self>) {
        if self.selected_hash.as_deref() == Some(hash.as_str()) {
            return;
        }
        self.selected_hash = Some(hash.clone());
        self.detail_sequence = self.detail_sequence.wrapping_add(1);
        self.detail_body_scroll.set_offset(point(px(0.), px(0.)));
        self.detail_files.clear();
        self.detail_selected_file = None;
        self.detail_loading = true;
        self.detail_diff.update(cx, |diff, cx| diff.clear(cx));
        cx.notify();

        let repo = self.state.read(cx).repo.clone();
        cx.spawn(async move |this, cx| {
            let task = {
                let repo = repo.clone();
                let hash = hash.clone();
                cx.background_executor()
                    .spawn(async move { repo.commit_files(&hash).unwrap_or_default() })
            };
            let files = task.await;
            this.update(cx, |this, cx| {
                if this.selected_hash.as_deref() != Some(hash.as_str()) {
                    return;
                }
                this.detail_files = files;
                this.detail_loading = false;
                if !this.detail_files.is_empty() {
                    this.select_detail_file(0, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select_detail_file(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.detail_files.len() {
            return;
        }
        self.detail_selected_file = Some(ix);
        let Some(hash) = self.selected_hash.clone() else {
            return;
        };
        let path = self.detail_files[ix].path.clone();
        let repo = self.state.read(cx).repo.clone();
        let context = GitSettings::get(cx).diff_context_lines;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let diff = cx
                .background_executor()
                .spawn(async move {
                    let diff = repo
                        .diff_commit_file(&hash, &path, context)
                        .unwrap_or_default();
                    PreparedDiff::new(path, &diff)
                })
                .await;
            this.update(cx, |this, cx| {
                this.detail_diff
                    .update(cx, |view, cx| view.set_diff(diff, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn explain_selected_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hash) = self.selected_hash.clone() else {
            return;
        };
        let state = self.state.read(cx);
        let Some(commit) = state.commits.iter().find(|commit| commit.hash == hash) else {
            return;
        };
        let message = if commit.body.trim().is_empty() {
            commit.subject.clone()
        } else {
            format!("{}\n\n{}", commit.subject, commit.body)
        };
        let detail = commit.short_hash.clone();
        let repo = state.repo.clone();
        let settings = GitSettings::get(cx).clone();
        let agent = AiAgentState::get(cx).agent;
        let dialog = open_ai_result_dialog("Explain Commit", detail, window, cx);
        run_ai_result_task(
            dialog,
            move |progress| {
                let diff = repo.run(&[
                    "show",
                    "--format=",
                    "--no-color",
                    "--no-ext-diff",
                    &hash,
                    "--",
                ])?;
                AiService::new(AiConfig::new(&settings, agent, repo.path.clone())).explain_diff(
                    &diff,
                    None,
                    Some(&message),
                    progress,
                )
            },
            AiResult::Explanation,
            cx,
        );
    }

    /// Width of the date column, sized for the longest text the current
    /// date format can produce.
    fn date_column_width(cx: &App) -> Pixels {
        let chars = match GitSettings::get(cx).date_format {
            // "Merged 16 September 2025 at 02:27"
            DateFormat::Merged => 33,
            // "16 September 2025 at 02:27"
            DateFormat::Absolute => 26,
            // "16 Sep 2025"
            DateFormat::Relative => 11,
            // "2025-09-16 02:27"
            DateFormat::Iso8601 => 16,
        };
        mono_column_width(chars as f32)
    }

    /// Width of the graph rail, shared by every row so the lanes line up.
    fn graph_width(&self, cx: &App) -> f32 {
        let visible_lanes = self
            .state
            .read(cx)
            .graph_rows
            .iter()
            .map(|r| r.lane_count)
            .max()
            .unwrap_or(1)
            .clamp(1, MAX_LANES);
        RAIL_LEADING + visible_lanes as f32 * LANE_WIDTH + RAIL_TRAILING
    }

    fn format_date(commit: &Commit, cx: &App) -> String {
        match GitSettings::get(cx).date_format {
            DateFormat::Merged => util::history_date(commit.committer_date, true),
            DateFormat::Relative => util::relative_date(commit.committer_date),
            DateFormat::Absolute => util::history_date(commit.committer_date, false),
            DateFormat::Iso8601 => chrono::DateTime::from_timestamp(commit.committer_date, 0)
                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default(),
        }
    }

    fn render_graph_cell(
        &self,
        commit_ix: usize,
        list_ix: usize,
        row_h: f32,
        status: CommitRowStatus,
        is_merge: bool,
        cx: &App,
    ) -> gpui::Div {
        let state = self.state.read(cx);
        let rows = &state.graph_rows;
        let palette = theme::graph_colors(cx);

        let row = rows.get(commit_ix).cloned().unwrap_or_default();
        let prev_edges: Vec<GraphEdge> = if commit_ix > 0 {
            rows.get(commit_ix - 1)
                .map(|r| r.edges.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let width = self.graph_width(cx);
        // Row offset in the list, so the glow dither lines up across rows.
        let list_y = list_ix as i32 * row_h as i32;

        let lane_x = move |lane: usize| RAIL_LEADING + lane as f32 * LANE_WIDTH + LANE_WIDTH / 2.0;
        let color_of = move |ix: usize, palette: &[Hsla]| palette[ix % palette.len()];

        let node_lane = row.lane;
        let node_color = color_of(row.color, &palette);
        let status_ring = if status.is_head {
            Some(cx.theme().primary)
        } else if status.is_unpushed {
            Some(theme::liquid_colors(cx).warning)
        } else {
            None
        };
        let edges = row.edges.clone();
        let palette_paint = palette.clone();
        let hollow_fill = cx.theme().colors.list;
        let halo_color = cx.theme().primary;

        div().flex_shrink_0().w(px(width)).h(px(row_h)).child(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window, _| {
                    let origin = bounds.origin;
                    let cy = row_h / 2.0;

                    let mut lanes: Vec<(Vec<(f32, f32)>, Hsla)> = Vec::new();

                    // Top half: verticals that arrive from the previous row.
                    for edge in &prev_edges {
                        if edge.to_lane >= MAX_LANES {
                            continue;
                        }
                        let x = lane_x(edge.to_lane);
                        let color = color_of(edge.color, &palette_paint);
                        lanes.push((vec![(x, 0.0), (x, cy)], color));
                    }

                    // Bottom half: edges that leave this row.
                    for edge in &edges {
                        if edge.from_lane >= MAX_LANES && edge.to_lane >= MAX_LANES {
                            continue;
                        }
                        let x_from = lane_x(edge.from_lane.min(MAX_LANES - 1));
                        let x_to = lane_x(edge.to_lane.min(MAX_LANES - 1));
                        let color = color_of(edge.color, &palette_paint);
                        let points = if (x_from - x_to).abs() < 0.5 {
                            vec![(x_from, cy), (x_to, row_h)]
                        } else {
                            curve_points(x_from, x_to, cy, row_h)
                        };
                        lanes.push((points, color));
                    }

                    paint_lane_glow(origin, &lanes, width, row_h, list_y, window);
                    for (points, color) in &lanes {
                        paint_lane_stroke(origin, points, *color, window);
                    }

                    // The commit node.
                    if node_lane < MAX_LANES {
                        let x = lane_x(node_lane);
                        let r = NODE_RADIUS;
                        if status.is_head {
                            // A dithered halo marks HEAD, like a lit pixel
                            // bleeding into its neighbors.
                            let pitch = 2.0;
                            let reach = 11.0;
                            let steps = (reach * 2.0 / pitch) as i32;
                            for halo_row in 0..steps {
                                for halo_column in 0..steps {
                                    let dx = (halo_column as f32 + 0.5) * pitch - reach;
                                    let dy = (halo_row as f32 + 0.5) * pitch - reach;
                                    let distance = (dx * dx + dy * dy).sqrt();
                                    if distance < r + 2.5 || distance > reach {
                                        continue;
                                    }
                                    let intensity = 1.0 - (distance - r - 2.5) / (reach - r - 2.5);
                                    if intensity <= cell_threshold(halo_column, halo_row) {
                                        continue;
                                    }
                                    let dot = Bounds::new(
                                        point(
                                            origin.x + px(x + dx - 0.5),
                                            origin.y + px(cy + dy - 0.5),
                                        ),
                                        size(px(1.), px(1.)),
                                    );
                                    window.paint_quad(fill(dot, halo_color.opacity(0.85)));
                                }
                            }
                        }
                        if let Some(ring_color) = status_ring {
                            let outer_r = r + 2.4;
                            let ring_bounds = Bounds::new(
                                point(origin.x + px(x - outer_r), origin.y + px(cy - outer_r)),
                                size(px(outer_r * 2.0), px(outer_r * 2.0)),
                            );
                            window.paint_quad(
                                fill(ring_bounds, ring_color).corner_radii(px(outer_r)),
                            );
                        }
                        let node_bounds = Bounds::new(
                            point(origin.x + px(x - r), origin.y + px(cy - r)),
                            size(px(r * 2.0), px(r * 2.0)),
                        );
                        if is_merge && status_ring.is_none() {
                            // Merge commits are hollow so they read apart
                            // from regular commits at a glance.
                            window.paint_quad(quad(
                                node_bounds,
                                px(r),
                                hollow_fill,
                                px(2.),
                                node_color,
                                BorderStyle::default(),
                            ));
                        } else {
                            window.paint_quad(fill(node_bounds, node_color).corner_radii(px(r)));
                        }
                    }
                },
            )
            .size_full(),
        )
    }

    fn render_row(
        &mut self,
        commit_ix: usize,
        list_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let settings_show_badges = GitSettings::get(cx).show_branch_badges;
        let show_avatars = GitSettings::get(cx).show_avatars;
        let show_stats = GitSettings::get(cx).show_commit_stats_badges;
        let row_h = ROW_HEIGHT;
        let palette = theme::graph_colors(cx);

        let state = self.state.read(cx);
        let commit = state.commits[commit_ix].clone();
        let status = CommitRowStatus::for_commit(&commit, &state.unpushed_commits);
        let lane_color = state
            .graph_rows
            .get(commit_ix)
            .map(|r| palette[r.color % palette.len()])
            .unwrap_or(palette[0]);
        let selected = self.selected_hash.as_deref() == Some(commit.hash.as_str());
        let colors = theme::liquid_colors(cx);
        let date_text = Self::format_date(&commit, cx);
        let date_width = Self::date_column_width(cx);
        let hash = commit.hash.clone();
        let menu_commit = commit.clone();
        let current_branch = state.current_branch.clone();
        let history = cx.entity();
        let accent = cx.theme().primary;
        let is_merge = commit.parents.len() > 1;
        let drop_target = DropTarget::commit(&commit, &current_branch);
        let drag_role = drag_role(&drop_target, cx);

        div()
            .id(("commit", commit_ix))
            .relative()
            .w_full()
            .h(px(row_h))
            .flex()
            .flex_row()
            .items_center()
            .pr_3()
            .when(!selected && status.is_head, |this| {
                this.bg(accent.opacity(0.07))
            })
            .when(!selected && !status.is_head && status.is_unpushed, |this| {
                this.bg(colors.warning.opacity(0.06))
            })
            .when(selected, |this| components::selected_row(this, accent))
            .when(!selected, |this| {
                this.hover(|s| s.bg(cx.theme().list_hover))
                    .active(|s| s.bg(cx.theme().button_active))
            })
            .when(drag_role.lifted, |this| this.opacity(0.5))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_commit(hash.clone(), cx);
            }))
            .ref_drag(
                DraggedRef::commit(&commit, lane_color),
                drop_target.clone(),
                cx.listener(move |this, source: &DraggedRef, window, cx| {
                    open_drop_menu(
                        this.state.clone(),
                        source.clone(),
                        drop_target.clone(),
                        |this: &mut Self| &mut this.drop_menu,
                        window,
                        cx,
                    );
                }),
            )
            .when(!selected && status.is_head, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_1()
                        .bottom_1()
                        .w(px(2.))
                        .rounded_r(px(2.))
                        .bg(accent),
                )
            })
            .when(!selected && !status.is_head && status.is_unpushed, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(4.))
                        .bg(checker(colors.warning.opacity(0.85), 1.0, window)),
                )
            })
            .children(drag_layer(drag_role, 0.0, false, cx))
            .when(show_avatars, |this| {
                this.child(
                    div()
                        .pl(px(AVATAR_LEADING))
                        .flex_shrink_0()
                        .child(author_avatar(
                            &commit.author_name,
                            &commit.author_email,
                            AVATAR_SIZE,
                            cx,
                        )),
                )
            })
            .child(self.render_graph_cell(commit_ix, list_ix, row_h, status, is_merge, cx))
            .when(status.is_unpushed, |this| {
                this.child(
                    h_flex()
                        .flex_shrink_0()
                        .when(settings_show_badges && !commit.refs.is_empty(), |this| {
                            this.mr_1()
                        })
                        .child(commit_status_badge(icons::push(), "push", colors.warning)),
                )
            })
            .when(settings_show_badges && !commit.refs.is_empty(), |this| {
                this.child(
                    h_flex().gap_1().flex_shrink_0().children(
                        commit
                            .refs
                            .iter()
                            .filter(|r| r.kind != RefKind::Head)
                            .take(3)
                            .map(|r| {
                                ref_badge(
                                    r,
                                    lane_color,
                                    r.kind == RefKind::LocalBranch && r.name == current_branch,
                                    cx,
                                )
                            }),
                    ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px_2()
                    .text_size(text_px(12.5))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(cx.theme().foreground)
                    .when(status.is_head, |text| {
                        text.font_weight(gpui::FontWeight::MEDIUM)
                    })
                    .child(commit.subject.clone()),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap(px(COLUMN_GAP))
                    .items_center()
                    .child(
                        metadata_cell(px(AUTHOR_COLUMN))
                            .text_size(text_px(11.5))
                            .text_color(colors.text_secondary)
                            .child(commit.author_name.clone()),
                    )
                    .child(
                        metadata_cell(date_width)
                            .font_family(mono_font())
                            .text_size(text_px(11.))
                            .text_color(colors.text_tertiary)
                            .child(date_text),
                    )
                    .child(
                        metadata_cell(mono_column_width(HASH_CHARS))
                            .font_family(mono_font())
                            .text_size(text_px(11.))
                            .text_color(if selected {
                                accent
                            } else {
                                colors.text_tertiary
                            })
                            .child(commit.short_hash.clone()),
                    )
                    .when(show_stats, |this| {
                        this.child(
                            h_flex()
                                .flex_shrink_0()
                                .w(px(STATS_COLUMN))
                                .justify_end()
                                .when_some(
                                    commit.additions.zip(commit.deletions),
                                    |this, (a, d)| this.child(stat_chips(a, d, cx)),
                                ),
                        )
                    }),
            )
            .context_menu(move |menu, window, menu_cx| {
                let checkout_history = history.clone();
                let checkout_hash = menu_commit.hash.clone();
                let cherry_history = history.clone();
                let cherry_hash = menu_commit.hash.clone();
                let revert_history = history.clone();
                let revert_hash = menu_commit.hash.clone();
                let rebase_history = history.clone();
                let rebase_hash = menu_commit.hash.clone();
                let rebase_short = menu_commit.short_hash.clone();
                let interactive_history = history.clone();
                let interactive_ref = menu_commit.parents.first().cloned();
                let interactive_short = menu_commit.short_hash.clone();
                let create_history = history.clone();
                let create_hash = menu_commit.hash.clone();
                let create_short = menu_commit.short_hash.clone();
                let copy_hash = menu_commit.hash.clone();
                let copy_short = menu_commit.short_hash.clone();
                let copy_subject = menu_commit.subject.clone();
                let reset_state = history.read(menu_cx).state.clone();

                let menu = menu
                    .item(
                        PopupMenuItem::new("Checkout Commit")
                            .icon(IconName::ArrowRight)
                            .on_click(move |_, _, cx| {
                                let checkout_hash = checkout_hash.clone();
                                checkout_history.update(cx, |this, cx| {
                                    this.run_git(
                                        "Checking out",
                                        move |repo| repo.checkout(&checkout_hash),
                                        cx,
                                    );
                                });
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Cherry-pick")
                            .icon(IconName::Copy)
                            .on_click(move |_, _, cx| {
                                let cherry_hash = cherry_hash.clone();
                                cherry_history.update(cx, |this, cx| {
                                    this.run_git(
                                        "Cherry-picking",
                                        move |repo| repo.cherry_pick(&cherry_hash),
                                        cx,
                                    );
                                });
                            }),
                    )
                    .item(PopupMenuItem::new("Revert").icon(IconName::Undo).on_click(
                        move |_, _, cx| {
                            let revert_hash = revert_hash.clone();
                            revert_history.update(cx, |this, cx| {
                                this.run_git(
                                    "Reverting",
                                    move |repo| repo.revert(&revert_hash),
                                    cx,
                                );
                            });
                        },
                    ))
                    .separator()
                    .item(
                        PopupMenuItem::new(format!(
                            "Rebase {current_branch} onto {}...",
                            menu_commit.short_hash
                        ))
                        .icon(icons::refresh())
                        .on_click(move |_, window, cx| {
                            let rebase_hash = rebase_hash.clone();
                            let rebase_short = rebase_short.clone();
                            rebase_history.update(cx, |this, cx| {
                                this.open_rebase_dialog(
                                    rebase_hash,
                                    rebase_short,
                                    false,
                                    window,
                                    cx,
                                );
                            });
                        }),
                    )
                    .item(
                        PopupMenuItem::new(format!(
                            "Interactive Rebase From {}...",
                            menu_commit.short_hash
                        ))
                        .icon(IconName::Settings)
                        .disabled(interactive_ref.is_none())
                        .on_click(move |_, window, cx| {
                            let Some(interactive_ref) = interactive_ref.clone() else {
                                return;
                            };
                            let interactive_short = interactive_short.clone();
                            interactive_history.update(cx, |this, cx| {
                                this.open_rebase_dialog(
                                    interactive_ref,
                                    interactive_short,
                                    true,
                                    window,
                                    cx,
                                );
                            });
                        }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("Create Branch Here...")
                            .icon(IconName::Plus)
                            .on_click(move |_, window, cx| {
                                let create_hash = create_hash.clone();
                                let create_short = create_short.clone();
                                create_history.update(cx, |this, cx| {
                                    this.open_create_branch_dialog(
                                        create_hash,
                                        create_short,
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    );
                reset_to_here_menu(menu, reset_state, menu_commit.hash.clone(), window, menu_cx)
                    .separator()
                    .item(
                        PopupMenuItem::new("Copy Commit Hash")
                            .icon(IconName::Copy)
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_hash.clone()));
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Copy Short Hash")
                            .icon(IconName::Copy)
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    copy_short.clone(),
                                ));
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Copy Commit Message")
                            .icon(IconName::Copy)
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    copy_subject.clone(),
                                ));
                            }),
                    )
            })
            .into_any_element()
    }

    /// The instrument-style header above the commit list: stat tiles and a
    /// dithered area chart of recent commit activity.
    fn render_activity_header(&self, matches: Option<usize>, cx: &App) -> gpui::Div {
        let colors = theme::liquid_colors(cx);
        let state = self.state.read(cx);
        let summary = activity_summary(&state.commits);
        let unpushed = state.unpushed_commits.len();
        let accent = cx.theme().primary;
        let commits = match matches {
            Some(matches) => format!(
                "{} / {}",
                grouped_count(matches),
                grouped_count(summary.commits)
            ),
            None => grouped_count(summary.commits),
        };
        let last_commit = summary
            .newest
            .map(util::relative_date)
            .unwrap_or_else(|| "—".to_string());

        h_flex()
            .w_full()
            .h(px(70.))
            .flex_shrink_0()
            .px_4()
            .gap_6()
            .items_center()
            .overflow_hidden()
            .bg(colors.panel)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(activity_stat(
                if matches.is_some() {
                    "Matches"
                } else {
                    "Commits"
                },
                commits,
                cx,
            ))
            .child(activity_stat("Authors", summary.authors.to_string(), cx))
            .child(activity_stat("Last commit", last_commit, cx))
            .child(
                v_flex()
                    .flex_shrink_0()
                    .gap_0p5()
                    .child(components::mono_label("Unpushed", cx))
                    .child(
                        div()
                            .font_family(mono_font())
                            .text_size(text_px(16.))
                            .line_height(text_px(20.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(if unpushed > 0 {
                                colors.warning
                            } else {
                                cx.theme().foreground
                            })
                            .child(unpushed.to_string()),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(180.))
                    .h_full()
                    .py_2p5()
                    .gap_1()
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(components::mono_label(
                                format!("Activity · {} weeks", ACTIVITY_DAYS / 7),
                                cx,
                            ))
                            .child(components::mono_label(
                                format!("{} in range", summary.recent),
                                cx,
                            )),
                    )
                    .child(
                        div().relative().flex_1().w_full().child(
                            Dither::new(DitherField::Area {
                                values: summary.bins.into(),
                            })
                            .color(accent)
                            .grid(3.0, 2.0)
                            .toned()
                            .layer(),
                        ),
                    )
                    .child(DottedRule::horizontal(colors.text_tertiary.opacity(0.5))),
            )
    }

    /// Column titles above the commit list, aligned with the row cells.
    fn render_column_header(&self, cx: &App) -> gpui::Div {
        let colors = theme::liquid_colors(cx);
        let settings = GitSettings::get(cx);
        let leading = if settings.show_avatars {
            AVATAR_LEADING + AVATAR_SIZE
        } else {
            0.0
        };
        let separator = colors.text_tertiary.opacity(0.45);

        h_flex()
            .w_full()
            .h(px(26.))
            .flex_shrink_0()
            .pr_3()
            .items_center()
            .bg(colors.panel)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .w(px(leading + self.graph_width(cx)))
                    .pl_2()
                    .child(components::mono_label("Graph", cx)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px_2()
                    .items_center()
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .flex()
                            .items_center()
                            .child(column_separator(separator)),
                    )
                    .child(components::mono_label("Description", cx)),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h_full()
                    .gap(px(COLUMN_GAP))
                    .items_center()
                    .child(column_title("Author", px(AUTHOR_COLUMN), separator, cx))
                    .child(column_title(
                        "Date",
                        Self::date_column_width(cx),
                        separator,
                        cx,
                    ))
                    .child(column_title(
                        "Hash",
                        mono_column_width(HASH_CHARS),
                        separator,
                        cx,
                    ))
                    .when(settings.show_commit_stats_badges, |this| {
                        this.child(
                            column_title("Diff", px(STATS_COLUMN), separator, cx).justify_end(),
                        )
                    }),
            )
    }

    fn render_detail(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = theme::liquid_colors(cx);
        let state = self.state.read(cx);
        let commit = self
            .selected_hash
            .as_deref()
            .and_then(|h| state.commits.iter().find(|c| c.hash == h))
            .cloned();

        let Some(commit) = commit else {
            return div().into_any_element();
        };
        let files = self.detail_files.clone();
        let selected_file = self.detail_selected_file;
        let loading = self.detail_loading;
        let file_panel_width = GitSettings::get(cx).effective_history_file_panel_width();

        h_resizable("history-detail-content-split")
            .with_state(&self.detail_content_split)
            .on_resize(|state, _, cx| {
                let Some(width) = state.read(cx).sizes().first().copied() else {
                    return;
                };
                GitSettings::update_saved(cx, |settings| {
                    // Saved in points, so the pane keeps its size at any interface scale.
                    settings.history_file_panel_width = (f32::from(width)
                        / crate::theme::ui_scale())
                    .clamp(MIN_HISTORY_FILE_PANEL_WIDTH, MAX_HISTORY_FILE_PANEL_WIDTH);
                });
            })
            .child(
                resizable_panel()
                    .size(px(file_panel_width))
                    .size_range(px(MIN_HISTORY_FILE_PANEL_WIDTH)..px(MAX_HISTORY_FILE_PANEL_WIDTH))
                    .flex_none()
                    .child(
                        v_flex()
                            .size_full()
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().colors.list)
                            .child(
                                v_flex()
                                    .p_3()
                                    .gap_1p5()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .gap_2()
                                            .items_start()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .text_size(text_px(12.5))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .child(commit.subject.clone()),
                                            )
                                            .when(GitSettings::get(cx).ai_explain_diffs, |this| {
                                                this.child(ai_action_group(
                                                    "history",
                                                    button("explain-selected-commit")
                                                        .ghost()
                                                        .xsmall()
                                                        .icon(icons::sparkles())
                                                        .label("Explain")
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                this.explain_selected_commit(
                                                                    window, cx,
                                                                )
                                                            },
                                                        )),
                                                    Size::XSmall,
                                                    cx,
                                                ))
                                            }),
                                    )
                                    .when(!commit.body.is_empty(), |this| {
                                        // Long descriptions scroll so the
                                        // file list keeps its room.
                                        // The cap sits on the frame; the
                                        // scroll area shrinks into it.
                                        this.child(
                                            v_flex()
                                                .relative()
                                                .max_h(px(DETAIL_BODY_MAX_HEIGHT))
                                                .child(
                                                    div()
                                                        .id("detail-body")
                                                        .min_h_0()
                                                        .overflow_y_scroll()
                                                        .track_scroll(&self.detail_body_scroll)
                                                        .child(
                                                            div()
                                                                .pr_2()
                                                                .text_size(text_px(11.5))
                                                                .text_color(colors.text_secondary)
                                                                .child(commit.body.clone()),
                                                        ),
                                                ),
                                        )
                                    })
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .pt_1()
                                            .items_center()
                                            .child(author_avatar(
                                                &commit.author_name,
                                                &commit.author_email,
                                                18.0,
                                                cx,
                                            ))
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_size(text_px(11.5))
                                                    .text_color(colors.text_secondary)
                                                    .child(commit.author_name.clone()),
                                            )
                                            .child(div().flex_1())
                                            .child(
                                                div()
                                                    .font_family(mono_font())
                                                    .text_size(text_px(11.))
                                                    .text_color(colors.text_tertiary)
                                                    .child(Self::format_date(&commit, cx)),
                                            )
                                            .child(
                                                div()
                                                    .px_1p5()
                                                    .rounded(px(4.))
                                                    .border_1()
                                                    .border_color(cx.theme().border)
                                                    .bg(cx.theme().muted)
                                                    .font_family(mono_font())
                                                    .text_size(text_px(11.))
                                                    .text_color(colors.text_secondary)
                                                    .child(commit.short_hash.clone()),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .pt_2p5()
                                    .pb_1()
                                    .child(components::section_header(
                                        "Files",
                                        Some(files.len()),
                                        cx,
                                    )),
                            )
                            .child(
                                v_flex()
                                    .id("detail-files")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scrollbar()
                                    .when(loading, |this| {
                                        this.child(components::dither_loader("Loading files", cx))
                                    })
                                    .children(files.iter().enumerate().map(|(ix, file)| {
                                        let is_selected = selected_file == Some(ix);
                                        div()
                                            .id(("detail-file", ix))
                                            .relative()
                                            .w_full()
                                            .px_3()
                                            .py_1()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .when(is_selected, |s| {
                                                components::selected_row(s, cx.theme().primary)
                                            })
                                            .when(!is_selected, |s| {
                                                s.hover(|s| s.bg(cx.theme().list_hover))
                                                    .active(|s| s.bg(cx.theme().button_active))
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.select_detail_file(ix, cx);
                                            }))
                                            .child(change_kind_chip(file.kind, cx))
                                            .child(components::file_path_label(&file.path, cx))
                                            .child(components::stat_chips(
                                                file.additions,
                                                file.deletions,
                                                cx,
                                            ))
                                    })),
                            ),
                    ),
            )
            .child(
                resizable_panel().child(
                    div()
                        .size_full()
                        .min_w_0()
                        .child(self.detail_diff.clone().cached(full_size())),
                ),
            )
            .into_any_element()
    }
}

impl Render for HistoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered = self.filtered_indices(cx);
        let row_h = ROW_HEIGHT;
        let sizes: Rc<Vec<gpui::Size<Pixels>>> =
            Rc::new(filtered.iter().map(|_| size(px(100.), px(row_h))).collect());
        let has_detail = self.selected_hash.is_some();
        let loading = self.state.read(cx).loading;
        let empty = filtered.is_empty();
        let query_active = !self.search.read(cx).value().trim().is_empty();
        let matches = query_active.then_some(filtered.len());
        let filtered = Rc::new(filtered);
        let history_detail_height = GitSettings::get(cx).effective_history_detail_height();

        let history_list = div()
            .size_full()
            .min_h_0()
            .relative()
            .when(empty, |this| {
                this.child(if loading {
                    components::dither_empty_state(
                        "Loading history",
                        "Reading commits and the branch graph.",
                        DitherField::Flow {
                            scale: 0.14,
                            speed: 4.0,
                            bias: -0.05,
                        },
                        cx,
                    )
                } else if query_active {
                    components::dither_empty_state(
                        "No matching commits",
                        "Search by message, author, or commit hash.",
                        DitherField::Glow {
                            x: 0.5,
                            y: 0.5,
                            radius: 0.45,
                            strength: 0.9,
                        }
                        .times(DitherField::Flow {
                            scale: 0.2,
                            speed: 1.5,
                            bias: 0.1,
                        }),
                        cx,
                    )
                } else {
                    components::dither_empty_state(
                        "No commits",
                        "This branch has no history yet.",
                        DitherField::Wave {
                            level: 0.25,
                            amplitude: 0.05,
                            speed: 1.2,
                        },
                        cx,
                    )
                })
            })
            .when(!empty, |this| {
                let filtered = filtered.clone();
                this.child(
                    v_flex().id("history-list").relative().size_full().child(
                        v_virtual_list(
                            cx.entity().clone(),
                            "history-rows",
                            sizes,
                            move |this, range, window, cx| {
                                range
                                    .map(|ix| this.render_row(filtered[ix], ix, window, cx))
                                    .collect()
                            },
                        )
                        .track_scroll(&self.scroll),
                    ),
                )
            });

        let layout = if has_detail {
            v_resizable("history-detail-split")
                .with_state(&self.detail_split)
                .on_resize(|state, _, cx| {
                    let Some(height) = state.read(cx).sizes().get(1).copied() else {
                        return;
                    };
                    GitSettings::update_saved(cx, |settings| {
                        settings.history_detail_height = (f32::from(height)
                            / crate::theme::ui_scale())
                        .clamp(MIN_DIFF_PANEL_HEIGHT, MAX_DIFF_PANEL_HEIGHT);
                    });
                })
                .child(
                    resizable_panel()
                        .size_range(px(MIN_DIFF_PANEL_HEIGHT)..gpui::Pixels::MAX)
                        .child(history_list),
                )
                .child(
                    resizable_panel()
                        .size(px(history_detail_height))
                        .size_range(px(MIN_DIFF_PANEL_HEIGHT)..px(MAX_DIFF_PANEL_HEIGHT))
                        .flex_none()
                        .child(
                            div()
                                .size_full()
                                .border_t_1()
                                .border_color(cx.theme().border)
                                .child(motion::detail_surface(
                                    "history-detail-motion",
                                    self.detail_sequence,
                                    self.render_detail(window, cx),
                                    cx,
                                )),
                        ),
                )
                .into_any_element()
        } else {
            history_list.into_any_element()
        };

        let drop_menu = self.drop_menu.as_ref().map(|menu| menu.render(window, cx));

        v_flex()
            .size_full()
            .min_h_0()
            .bg(cx.theme().colors.list)
            .child(self.render_activity_header(matches, cx))
            .when(!empty, |this| this.child(self.render_column_header(cx)))
            .child(v_flex().flex_1().min_h_0().child(layout))
            .children(drop_menu)
    }
}

/// Points along a lane bend from `x_from` at `cy` to `x_to` at `row_h`.
/// The bend leaves heading down and arrives heading down.
fn curve_points(x_from: f32, x_to: f32, cy: f32, row_h: f32) -> Vec<(f32, f32)> {
    let p0 = (x_from, cy);
    let p1 = (x_from, row_h);
    let p2 = (x_to, cy);
    let p3 = (x_to, row_h);
    (0..=CURVE_SAMPLES)
        .map(|sample| {
            let t = sample as f32 / CURVE_SAMPLES as f32;
            let u = 1.0 - t;
            let a = u * u * u;
            let b = 3.0 * u * u * t;
            let c = 3.0 * u * t * t;
            let d = t * t * t;
            (
                a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
            )
        })
        .collect()
}

/// Paints a lane as a smooth two-pixel stroke through `points`.
fn paint_lane_stroke(
    origin: gpui::Point<Pixels>,
    points: &[(f32, f32)],
    color: Hsla,
    window: &mut Window,
) {
    let Some(&(start_x, start_y)) = points.first() else {
        return;
    };
    let mut builder = PathBuilder::stroke(px(2.0));
    builder.move_to(point(origin.x + px(start_x), origin.y + px(start_y)));
    for &(x, y) in &points[1..] {
        builder.line_to(point(origin.x + px(x), origin.y + px(y)));
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Paints an ordered-dither glow that fades out around every lane, like
/// phosphor bleeding around a lit line. Each grid cell takes the color of
/// the lane that lights it most. `list_y` is the row's offset in the list,
/// so the Bayer pattern continues across rows and scrolls with them.
fn paint_lane_glow(
    origin: gpui::Point<Pixels>,
    lanes: &[(Vec<(f32, f32)>, Hsla)],
    width: f32,
    row_h: f32,
    list_y: i32,
    window: &mut Window,
) {
    // The Bayer pattern repeats every 8 cells, so rows with the same lanes at the same phase
    // light the same dots.
    let phase = (list_y / GLOW_PITCH as i32).rem_euclid(8);
    let mut key = vec![width.to_bits(), row_h.to_bits(), phase as u32];
    for (points, color) in lanes {
        key.push(points.len() as u32);
        key.extend(points.iter().flat_map(|(x, y)| [x.to_bits(), y.to_bits()]));
        let (hue, saturation, lightness, alpha) = color.into_components();
        key.extend([hue.into_inner(), saturation, lightness, alpha].map(f32::to_bits));
    }
    let dots = GLOW_DOTS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(dots) = cache.get(&key) {
            return dots.clone();
        }
        if cache.len() >= GLOW_CACHE {
            cache.clear();
        }
        let dots = glow_dots(lanes, width, row_h, phase);
        cache.insert(key, dots.clone());
        dots
    });
    for &(x, y, color) in dots.iter() {
        let dot = Bounds::new(
            point(origin.x + px(x - 0.5), origin.y + px(y - 0.5)),
            size(px(1.), px(1.)),
        );
        window.paint_quad(fill(dot, color.opacity(GLOW_ALPHA)));
    }
}

/// Rows of glow dots kept before the cache starts again.
const GLOW_CACHE: usize = 256;

/// The lit dots of one row's glow, in points from the row's origin, with their colors.
type GlowDots = Rc<[(f32, f32, Hsla)]>;

thread_local! {
    // The glow dots of the rows painted lately, by their lanes, size and Bayer phase. Scrolling
    // paints rows of the same shapes again on every frame.
    static GLOW_DOTS: std::cell::RefCell<std::collections::HashMap<Vec<u32>, GlowDots>> =
        Default::default();
}

// Returns the lit dots of a row's glow, in points from the row's origin, with their colors.
// `phase` is the row's offset in the Bayer pattern, in cells.
fn glow_dots(lanes: &[(Vec<(f32, f32)>, Hsla)], width: f32, row_h: f32, phase: i32) -> GlowDots {
    let columns = (width / GLOW_PITCH).ceil() as i32;
    let rows = (row_h / GLOW_PITCH).ceil() as i32;
    // Extent each lane can light, to skip far-away lanes: a lane lights nothing beyond the
    // glow's reach from its points.
    let reach: Vec<((f32, f32), (f32, f32))> = lanes
        .iter()
        .map(|(points, _)| {
            let ((min_x, max_x), (min_y, max_y)) = points.iter().fold(
                (
                    (f32::INFINITY, f32::NEG_INFINITY),
                    (f32::INFINITY, f32::NEG_INFINITY),
                ),
                |((min_x, max_x), (min_y, max_y)), &(x, y)| {
                    ((min_x.min(x), max_x.max(x)), (min_y.min(y), max_y.max(y)))
                },
            );
            (
                (min_x - GLOW_REACH, max_x + GLOW_REACH),
                (min_y - GLOW_REACH, max_y + GLOW_REACH),
            )
        })
        .collect();
    let mut dots = Vec::new();
    for row in 0..rows {
        let y = (row as f32 + 0.5) * GLOW_PITCH;
        for column in 0..columns {
            let x = (column as f32 + 0.5) * GLOW_PITCH;
            let mut brightest: Option<(f32, Hsla)> = None;
            for ((points, color), &((min_x, max_x), (min_y, max_y))) in lanes.iter().zip(&reach) {
                if x < min_x || x > max_x || y < min_y || y > max_y {
                    continue;
                }
                let intensity = glow_intensity(distance_to_polyline((x, y), points, GLOW_REACH));
                if intensity > brightest.map_or(0.0, |(best, _)| best) {
                    brightest = Some((intensity, *color));
                }
            }
            let Some((intensity, color)) = brightest else {
                continue;
            };
            if intensity <= cell_threshold(column, row + phase) {
                continue;
            }
            dots.push((x, y, color));
        }
    }
    dots.into()
}

/// Glow brightness at `distance` from a lane center: none under the
/// stroke, strongest beside it, and fading to nothing at the reach.
fn glow_intensity(distance: f32) -> f32 {
    if !(GLOW_CORE..GLOW_REACH).contains(&distance) {
        return 0.0;
    }
    let falloff = 1.0 - (distance - GLOW_CORE) / (GLOW_REACH - GLOW_CORE);
    GLOW_STRENGTH * falloff * falloff
}

/// Shortest distance from `target` to the polyline through `points`, or infinity when the
/// polyline lies farther than `reach` from it. Segments whose box lies beyond `reach` are
/// skipped, as they cannot be nearer.
fn distance_to_polyline(target: (f32, f32), points: &[(f32, f32)], reach: f32) -> f32 {
    points
        .windows(2)
        .filter(|segment| {
            let ((ax, ay), (bx, by)) = (segment[0], segment[1]);
            target.0 >= ax.min(bx) - reach
                && target.0 <= ax.max(bx) + reach
                && target.1 >= ay.min(by) - reach
                && target.1 <= ay.max(by) + reach
        })
        .map(|segment| {
            let (ax, ay) = segment[0];
            let (bx, by) = segment[1];
            let (dx, dy) = (bx - ax, by - ay);
            let length_sq = dx * dx + dy * dy;
            let t = if length_sq == 0.0 {
                0.0
            } else {
                (((target.0 - ax) * dx + (target.1 - ay) * dy) / length_sq).clamp(0.0, 1.0)
            };
            let (offset_x, offset_y) = (ax + t * dx - target.0, ay + t * dy - target.1);
            (offset_x * offset_x + offset_y * offset_y).sqrt()
        })
        .fold(f32::INFINITY, f32::min)
}

/// Width of a mono metadata column that fits `chars` glyphs at the metadata text size.
fn mono_column_width(chars: f32) -> Pixels {
    text_px((chars * MONO_ADVANCE).ceil()) + px(2.)
}

/// A fixed-width, single-line cell in the commit row metadata columns.
fn metadata_cell(width: Pixels) -> gpui::Div {
    div()
        .flex_shrink_0()
        .w(width)
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
}

/// A short dotted divider between column titles.
fn column_separator(color: Hsla) -> gpui::Div {
    div()
        .flex_shrink_0()
        .h(px(12.))
        .child(DottedRule::vertical(color))
}

/// A column title that starts with a dotted divider. The divider sits in
/// the column gap so the label lines up with the row text below it.
fn column_title(label: &'static str, width: Pixels, separator: Hsla, cx: &App) -> gpui::Div {
    h_flex()
        .flex_shrink_0()
        .w(width)
        .h_full()
        .items_center()
        .relative()
        .child(
            div()
                .absolute()
                .left(px(-COLUMN_GAP / 2.0))
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .child(column_separator(separator)),
        )
        .child(components::mono_label(label, cx))
}
