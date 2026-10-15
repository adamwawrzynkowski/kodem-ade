//! Drag and drop of commits and branches. Drag a history commit or a
//! sidebar branch onto another commit or branch, and a menu offers the Git
//! actions between them: merge, rebase, cherry-pick, and moving a branch.
//! The dragged item always goes into, or onto, the item it is dropped on.
//!
//! While a drag runs, the source row lifts, valid targets glow under the
//! pointer, and the chip under the pointer names the target. A drop lands
//! with an impact; a drag that ends nowhere glides back to its row.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    Anchor, AnyElement, App, Context, DismissEvent, Div, DragMoveEvent, Entity, FontWeight, Global,
    Hsla, IntoElement, Pixels, Point, Render, SharedString, Stateful, Subscription, Window, div,
};
use kodem_git::git::{Branch, Commit, GitRepo, RefKind};

use crate::theme::{px, text_px};
use crate::ui::dither::{Dither, DitherField, Edge};
use crate::ui::fx;
use crate::ui::git::components::mono_font;
use crate::ui::git::contextual_actions::{
    MergeDialogView, RebaseDialogView, confirm_destructive_action, open_liquid_dialog,
    open_rebase_dialog, reset_submenu,
};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, Icon, PopupMenu, PopupMenuItem, Sizable as _, h_flex, popup_menu_layer,
    v_flex,
};
use crate::ui::git::motion;
use crate::ui::git::settings::GitSettings;
use crate::ui::git::state::RepositoryState;
use crate::ui::git::theme;

/// Anchor on the row a drag started from, where a cancelled drag glides
/// back.
const SOURCE_ANCHOR: &str = "ref-drag-source";
/// Anchor on the drop target under the pointer, where a drop lands.
const TARGET_ANCHOR: &str = "ref-drag-target";
/// Gap between the pointer and the dragged chip.
const CHIP_GAP_X: f32 = 14.0;
const CHIP_GAP_Y: f32 = 10.0;
const CHIP_SUBJECT_WIDTH: f32 = 220.0;
const MENU_MIN_WIDTH: f32 = 280.0;
/// A cancelled drag shorter than this was a shaky click, so it does not
/// glide back.
const GLIDE_BACK_MIN_DISTANCE: f32 = 12.0;

/// What the user drags.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DragKind {
    /// A history commit. `tip_branches` are the local branches that point
    /// at it, and `parent` is its first parent.
    Commit {
        tip_branches: Vec<String>,
        parent: Option<String>,
    },
    LocalBranch(String),
    RemoteBranch(String),
}

/// The payload of a commit or branch drag.
#[derive(Debug, Clone)]
pub(crate) struct DraggedRef {
    kind: DragKind,
    hash: String,
    short_hash: String,
    subject: String,
    tone: Hsla,
    /// The key of the row the drag started from.
    key: SharedString,
}

impl DraggedRef {
    /// A history commit, drawn in the color of its graph lane.
    pub(crate) fn commit(commit: &Commit, tone: Hsla) -> Self {
        let tip_branches = commit
            .refs
            .iter()
            .filter(|item| item.kind == RefKind::LocalBranch)
            .map(|item| item.name.clone())
            .collect();
        Self {
            kind: DragKind::Commit {
                tip_branches,
                parent: commit.parents.first().cloned(),
            },
            hash: commit.hash.clone(),
            short_hash: commit.short_hash.clone(),
            subject: commit.subject.clone(),
            tone,
            key: history_key(&commit.hash),
        }
    }

    /// A local or remote sidebar branch.
    pub(crate) fn branch(branch: &Branch, tone: Hsla) -> Self {
        let kind = if branch.is_remote {
            DragKind::RemoteBranch(branch.name.clone())
        } else {
            DragKind::LocalBranch(branch.name.clone())
        };
        Self {
            kind,
            hash: branch.hash.clone(),
            short_hash: branch.short_hash.clone(),
            subject: branch.subject.clone(),
            tone,
            key: branch_key(&branch.name),
        }
    }

    /// The name people know the dragged item by.
    fn label(&self) -> &str {
        match &self.kind {
            DragKind::Commit { .. } => &self.short_hash,
            DragKind::LocalBranch(name) | DragKind::RemoteBranch(name) => name,
        }
    }

    /// The revision Git uses for the dragged item.
    fn reference(&self) -> &str {
        match &self.kind {
            DragKind::Commit { .. } => &self.hash,
            DragKind::LocalBranch(name) | DragKind::RemoteBranch(name) => name,
        }
    }
}

/// A row that takes drops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DropTarget {
    key: SharedString,
    hash: String,
    /// The revision Git uses: a branch name when there is one.
    reference: String,
    label: String,
    /// The local branch that takes merges, cherry-picks, and moves.
    branch: Option<String>,
}

impl DropTarget {
    /// A history commit. A local branch at the commit, preferably the
    /// current one, takes what is dropped on it.
    pub(crate) fn commit(commit: &Commit, current_branch: &str) -> Self {
        let local = commit
            .refs
            .iter()
            .filter(|item| item.kind == RefKind::LocalBranch)
            .map(|item| item.name.as_str());
        let branch = local
            .clone()
            .find(|name| *name == current_branch)
            .or_else(|| local.clone().next())
            .map(str::to_string);
        let remote = commit
            .refs
            .iter()
            .find(|item| item.kind == RefKind::RemoteBranch)
            .map(|item| item.name.clone());
        let (reference, label) = match (&branch, remote) {
            (Some(branch), _) => (branch.clone(), branch.clone()),
            (None, Some(remote)) => (remote.clone(), remote),
            (None, None) => (commit.hash.clone(), commit.short_hash.clone()),
        };
        Self {
            key: history_key(&commit.hash),
            hash: commit.hash.clone(),
            reference,
            label,
            branch,
        }
    }

    /// A local or remote sidebar branch.
    pub(crate) fn branch(branch: &Branch) -> Self {
        Self {
            key: branch_key(&branch.name),
            hash: branch.hash.clone(),
            reference: branch.name.clone(),
            label: branch.name.clone(),
            branch: (!branch.is_remote).then(|| branch.name.clone()),
        }
    }
}

fn history_key(hash: &str) -> SharedString {
    SharedString::from(format!("history:{hash}"))
}

fn branch_key(name: &str) -> SharedString {
    SharedString::from(format!("branch:{name}"))
}

/// Whether a drop of `source` on `target` can do anything. Checked while
/// the pointer moves, so it uses only what the drag already knows.
fn accepts(source: &DraggedRef, target: &DropTarget) -> bool {
    if source.hash == target.hash {
        return false;
    }
    match &source.kind {
        // A remote branch can only go into a local branch.
        DragKind::RemoteBranch(_) => target.branch.is_some(),
        DragKind::Commit { .. } | DragKind::LocalBranch(_) => true,
    }
}

/// Facts about the dragged item that only Git can answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SourceFacts {
    /// Local branches that already contain the dragged commit.
    containing: HashSet<String>,
    /// The local branch that whole-branch actions move or merge.
    owner: Option<String>,
    /// Commits from the dragged commit to the tip of `owner`.
    commits_to_tip: Option<usize>,
}

/// Asks Git which branches hold the dragged commit and which one it
/// belongs to.
fn resolve_facts(repo: &GitRepo, source: &DraggedRef, current_branch: &str) -> SourceFacts {
    let containing: HashSet<String> = repo
        .branches_containing(&source.hash)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let owner = match &source.kind {
        DragKind::LocalBranch(name) => Some(name.clone()),
        DragKind::RemoteBranch(_) => None,
        DragKind::Commit { tip_branches, .. } => {
            commit_owner(&containing, tip_branches, current_branch, || {
                repo.nearest_branch(&source.hash).ok().flatten()
            })
        }
    };
    let commits_to_tip = match (&source.kind, &owner) {
        (
            DragKind::Commit {
                parent: Some(parent),
                ..
            },
            Some(owner),
        ) => repo.count_commits(&format!("{parent}..{owner}")).ok(),
        _ => None,
    };
    SourceFacts {
        containing,
        owner,
        commits_to_tip,
    }
}

/// Picks the branch a dragged commit belongs to: a branch whose tip it
/// is, then the branch Git names it after, then the current branch, then
/// any branch that contains it.
fn commit_owner(
    containing: &HashSet<String>,
    tip_branches: &[String],
    current_branch: &str,
    nearest: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if let Some(tip) = tip_branches
        .iter()
        .find(|name| *name == current_branch)
        .or_else(|| tip_branches.first())
    {
        return Some(tip.clone());
    }
    if let Some(nearest) = nearest().filter(|name| containing.contains(name)) {
        return Some(nearest);
    }
    if containing.contains(current_branch) {
        return Some(current_branch.to_string());
    }
    let mut names: Vec<&String> = containing.iter().collect();
    names.sort();
    names.first().map(|name| (*name).clone())
}

/// The first commit of a partial rebase or cherry-pick.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RangeStart {
    /// The revision just before the first commit.
    after: String,
    first_commit: String,
    count: Option<usize>,
}

/// One entry of the drop menu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DropAction {
    Merge {
        source_ref: String,
        source_label: String,
        into: String,
        /// Merges only up to the dragged commit, not its whole branch.
        up_to_commit: bool,
        checkout: bool,
    },
    Rebase {
        branch: String,
        onto_ref: String,
        onto_label: String,
        start: Option<RangeStart>,
        checkout: bool,
    },
    InteractiveRebase {
        branch: String,
        onto_ref: String,
        onto_label: String,
    },
    CherryPick {
        revisions: String,
        start: RangeStart,
        onto: String,
        /// Picks the commits from the dragged one to the tip of its branch.
        range: bool,
        checkout: bool,
    },
    MoveBranch {
        branch: String,
        to_ref: String,
        to_label: String,
    },
    ResetBranch {
        branch: String,
        to_hash: String,
        to_label: String,
    },
}

impl DropAction {
    /// Actions of one group share a menu section.
    fn group(&self) -> u8 {
        match self {
            DropAction::Merge { .. } => 0,
            DropAction::Rebase { .. } | DropAction::InteractiveRebase { .. } => 1,
            DropAction::CherryPick { .. } => 2,
            DropAction::MoveBranch { .. } | DropAction::ResetBranch { .. } => 3,
        }
    }

    fn title(&self) -> String {
        match self {
            DropAction::Merge {
                source_label,
                into,
                up_to_commit,
                ..
            } => {
                if *up_to_commit {
                    format!("Merge up to {source_label} into {into}...")
                } else {
                    format!("Merge {source_label} into {into}...")
                }
            }
            DropAction::Rebase {
                branch,
                onto_label,
                start,
                ..
            } => match start {
                None => format!("Rebase {branch} onto {onto_label}..."),
                Some(start) => match start.count {
                    Some(1) => format!("Rebase Only {} onto {onto_label}...", start.first_commit),
                    Some(count) => format!(
                        "Rebase {count} Commits from {} onto {onto_label}...",
                        start.first_commit
                    ),
                    None => format!(
                        "Rebase {branch} from {} onto {onto_label}...",
                        start.first_commit
                    ),
                },
            },
            DropAction::InteractiveRebase {
                branch, onto_label, ..
            } => format!("Interactive Rebase {branch} onto {onto_label}..."),
            DropAction::CherryPick {
                start,
                onto,
                range,
                checkout,
                ..
            } => {
                let picked = match (range, start.count) {
                    (false, _) => start.first_commit.clone(),
                    (true, Some(count)) => format!("{count} Commits from {}", start.first_commit),
                    (true, None) => format!("Commits from {}", start.first_commit),
                };
                if *checkout {
                    format!("Switch to {onto} and Cherry-pick {picked}")
                } else {
                    format!("Cherry-pick {picked} onto {onto}")
                }
            }
            DropAction::MoveBranch {
                branch, to_label, ..
            } => format!("Move {branch} to {to_label}..."),
            DropAction::ResetBranch {
                branch, to_label, ..
            } => format!("Reset {branch} to {to_label}"),
        }
    }

    fn icon(&self) -> Icon {
        match self {
            DropAction::Merge { .. } => icons::git_merge(),
            DropAction::Rebase { .. } | DropAction::InteractiveRebase { .. } => icons::refresh(),
            DropAction::CherryPick { .. } => icons::git_commit(),
            DropAction::MoveBranch { .. } | DropAction::ResetBranch { .. } => icons::git_branch(),
        }
    }
}

/// Lists what a drop of `source` on `target` can do. The dragged item
/// goes into or onto the target: it merges into the target branch, its
/// branch rebases onto the target, its commits are picked onto the target
/// branch, or the target branch moves to it. A dragged commit offers both
/// the commit alone and the whole branch it belongs to.
fn plan_drop(
    source: &DraggedRef,
    target: &DropTarget,
    facts: &SourceFacts,
    current_branch: &str,
) -> Vec<DropAction> {
    if !accepts(source, target) {
        return Vec::new();
    }
    let is_current = |branch: &str| {
        !current_branch.is_empty() && current_branch != "HEAD" && branch == current_branch
    };
    let owner = match &source.kind {
        DragKind::LocalBranch(name) => Some(name.as_str()),
        DragKind::RemoteBranch(_) => None,
        DragKind::Commit { .. } => facts.owner.as_deref(),
    };
    let (parent, owner_tip) = match &source.kind {
        DragKind::Commit {
            tip_branches,
            parent,
        } => (
            parent.as_deref(),
            owner.is_some_and(|owner| tip_branches.iter().any(|tip| tip == owner)),
        ),
        DragKind::LocalBranch(_) | DragKind::RemoteBranch(_) => (None, true),
    };
    let receiver = target.branch.as_deref();
    let contained = receiver.is_some_and(|branch| facts.containing.contains(branch));
    let single = RangeStart {
        after: parent.unwrap_or_default().to_string(),
        first_commit: source.short_hash.clone(),
        count: Some(1),
    };
    let mut actions = Vec::new();

    if let Some(into) = receiver {
        let checkout = !is_current(into);
        match &source.kind {
            DragKind::LocalBranch(name) | DragKind::RemoteBranch(name) => {
                if !contained {
                    actions.push(DropAction::Merge {
                        source_ref: name.clone(),
                        source_label: name.clone(),
                        into: into.to_string(),
                        up_to_commit: false,
                        checkout,
                    });
                }
            }
            DragKind::Commit { .. } => {
                if let Some(owner) = owner.filter(|owner| *owner != into)
                    && !(owner_tip && contained)
                {
                    actions.push(DropAction::Merge {
                        source_ref: owner.to_string(),
                        source_label: owner.to_string(),
                        into: into.to_string(),
                        up_to_commit: false,
                        checkout,
                    });
                }
                if !contained && !owner_tip {
                    actions.push(DropAction::Merge {
                        source_ref: source.hash.clone(),
                        source_label: source.short_hash.clone(),
                        into: into.to_string(),
                        up_to_commit: true,
                        checkout,
                    });
                }
            }
        }
    }

    if let Some(branch) = owner.filter(|branch| Some(*branch) != receiver) {
        let checkout = !is_current(branch);
        actions.push(DropAction::Rebase {
            branch: branch.to_string(),
            onto_ref: target.reference.clone(),
            onto_label: target.label.clone(),
            start: None,
            checkout,
        });
        if let Some(parent) = parent.filter(|parent| *parent != target.hash) {
            actions.push(DropAction::Rebase {
                branch: branch.to_string(),
                onto_ref: target.reference.clone(),
                onto_label: target.label.clone(),
                start: Some(RangeStart {
                    after: parent.to_string(),
                    first_commit: source.short_hash.clone(),
                    count: facts.commits_to_tip,
                }),
                checkout,
            });
        }
        if !checkout {
            actions.push(DropAction::InteractiveRebase {
                branch: branch.to_string(),
                onto_ref: target.reference.clone(),
                onto_label: target.label.clone(),
            });
        }
    }

    if let Some(onto) = receiver {
        let checkout = !is_current(onto);
        if !contained {
            actions.push(DropAction::CherryPick {
                revisions: source.hash.clone(),
                start: single,
                onto: onto.to_string(),
                range: false,
                checkout,
            });
        }
        if let Some(parent) = parent
            && let Some(owner) = owner.filter(|owner| *owner != onto)
            && !owner_tip
        {
            actions.push(DropAction::CherryPick {
                revisions: format!("{parent}..{owner}"),
                start: RangeStart {
                    after: parent.to_string(),
                    first_commit: source.short_hash.clone(),
                    count: facts.commits_to_tip,
                },
                onto: onto.to_string(),
                range: true,
                checkout,
            });
        }
        if is_current(onto) {
            actions.push(DropAction::ResetBranch {
                branch: onto.to_string(),
                to_hash: source.hash.clone(),
                to_label: source.label().to_string(),
            });
        } else {
            actions.push(DropAction::MoveBranch {
                branch: onto.to_string(),
                to_ref: source.reference().to_string(),
                to_label: source.label().to_string(),
            });
        }
    }

    actions
}

/// Why a drop offers nothing, shown in place of the actions.
fn empty_reason(source: &DraggedRef, target: &DropTarget, facts: &SourceFacts) -> String {
    match target.branch.as_deref() {
        Some(branch) if facts.containing.contains(branch) => {
            format!("{branch} already contains {}", source.label())
        }
        _ => format!("Nothing to do with {} here", source.label()),
    }
}

/// The drag in progress, shared by every row and the chip.
#[derive(Default)]
struct DragSession {
    /// Counts drags, so a finished chip only clears its own drag.
    sequence: u64,
    source: Option<DraggedRef>,
    hovered: Option<HoveredTarget>,
    /// Counts target changes, so the chip hint animates on each one.
    hover_sequence: u64,
    dropped: bool,
}

#[derive(Clone)]
struct HoveredTarget {
    key: SharedString,
    label: SharedString,
}

#[derive(Default)]
struct RefDragSession(Rc<RefCell<DragSession>>);

impl Global for RefDragSession {}

fn session(cx: &mut App) -> Rc<RefCell<DragSession>> {
    if !cx.has_global::<RefDragSession>() {
        cx.set_global(RefDragSession::default());
    }
    cx.global::<RefDragSession>().0.clone()
}

/// How a row takes part in the drag in progress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DragRole {
    /// The drag started from this row.
    pub lifted: bool,
    /// The pointer is over this row, and the row accepts the drop.
    pub hovered: bool,
    /// The row accepts the drop, but the pointer is elsewhere.
    pub invited: bool,
}

/// Reads how the row of `target` takes part in the drag in progress.
pub(crate) fn drag_role(target: &DropTarget, cx: &App) -> DragRole {
    if !cx.has_active_drag() {
        return DragRole::default();
    }
    let Some(session) = cx.try_global::<RefDragSession>() else {
        return DragRole::default();
    };
    let session = session.0.borrow();
    let Some(source) = &session.source else {
        return DragRole::default();
    };
    let accepted = accepts(source, target);
    let hovered = accepted
        && session
            .hovered
            .as_ref()
            .is_some_and(|hovered| hovered.key == target.key);
    DragRole {
        lifted: source.key == target.key,
        hovered,
        invited: accepted && !hovered,
    }
}

/// Paints how a row takes part in a drag: a glowing frame under the
/// pointer, a dashed outline on rows that accept the drop when `invite`
/// is set, and the marks the drop effects aim at. Add it as an early child
/// of a `relative` row, so the row content paints on top.
pub(crate) fn drag_layer(
    role: DragRole,
    radius: f32,
    invite: bool,
    cx: &App,
) -> Option<AnyElement> {
    let accent = cx.theme().primary;
    if role.hovered {
        return Some(
            div()
                .absolute()
                .inset_0()
                .rounded(px(radius))
                .bg(accent.opacity(0.12))
                .border_1()
                .border_color(accent.opacity(0.9))
                .child(
                    Dither::new(
                        DitherField::Flow {
                            scale: 0.2,
                            speed: 3.0,
                            bias: 0.05,
                        }
                        .times(DitherField::Fade {
                            from: Edge::Left,
                            start: 1.0,
                            end: 0.15,
                        }),
                    )
                    .color(accent.opacity(0.5))
                    .grid(3.0, 1.0)
                    .corner_radius(radius)
                    .animated()
                    .layer(),
                )
                .child(fx::anchor(TARGET_ANCHOR))
                .into_any_element(),
        );
    }
    if role.lifted {
        return Some(
            div()
                .absolute()
                .inset_0()
                .rounded(px(radius))
                .border_1()
                .border_dashed()
                .border_color(accent.opacity(0.7))
                .child(fx::anchor(SOURCE_ANCHOR))
                .into_any_element(),
        );
    }
    if role.invited && invite {
        return Some(
            div()
                .absolute()
                .inset_0()
                .rounded(px(radius))
                .border_1()
                .border_dashed()
                .border_color(accent.opacity(0.3))
                .into_any_element(),
        );
    }
    None
}

/// Adds commit and branch drag and drop to a row.
pub(crate) trait RefDragExt: Sized {
    /// Lets the row be dragged as `source` and take drops as `target`.
    /// `on_drop` runs when a commit or branch that fits is dropped on it.
    fn ref_drag(
        self,
        source: DraggedRef,
        target: DropTarget,
        on_drop: impl Fn(&DraggedRef, &mut Window, &mut App) + 'static,
    ) -> Self;
}

impl RefDragExt for Stateful<Div> {
    fn ref_drag(
        self,
        source: DraggedRef,
        target: DropTarget,
        on_drop: impl Fn(&DraggedRef, &mut Window, &mut App) + 'static,
    ) -> Self {
        let hover_target = target.clone();
        self.on_drag(source, start_drag)
            .on_drag_move::<DraggedRef>(move |event, _, cx| {
                track_hover(event, &hover_target, cx);
            })
            .can_drop(move |dragged, _, _| {
                dragged
                    .downcast_ref::<DraggedRef>()
                    .is_some_and(|source| accepts(source, &target))
            })
            .on_drop(move |source: &DraggedRef, window, cx| {
                session(cx).borrow_mut().dropped = true;
                on_drop(source, window, cx);
            })
    }
}

fn start_drag(
    source: &DraggedRef,
    offset: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<DragChip> {
    let session = session(cx);
    let sequence = {
        let mut session = session.borrow_mut();
        session.sequence = session.sequence.wrapping_add(1);
        session.source = Some(source.clone());
        session.hovered = None;
        session.dropped = false;
        session.sequence
    };
    let pointer = window.mouse_position();
    let source = source.clone();
    cx.new(|cx| DragChip::new(source, offset, pointer, sequence, cx))
}

/// Keeps the session's hovered target in step with the pointer. Every
/// target hears every move, so each one claims or releases itself.
fn track_hover(event: &DragMoveEvent<DraggedRef>, target: &DropTarget, cx: &mut App) {
    let inside = event.bounds.contains(&event.event.position);
    let accepted = inside
        && event
            .dragged_item()
            .downcast_ref::<DraggedRef>()
            .is_some_and(|source| accepts(source, target));
    let session = session(cx);
    let mut session = session.borrow_mut();
    let is_hovered = session
        .hovered
        .as_ref()
        .is_some_and(|hovered| hovered.key == target.key);
    if accepted && !is_hovered {
        session.hovered = Some(HoveredTarget {
            key: target.key.clone(),
            label: SharedString::from(target.label.clone()),
        });
        session.hover_sequence = session.hover_sequence.wrapping_add(1);
    } else if !accepted && is_hovered {
        session.hovered = None;
        session.hover_sequence = session.hover_sequence.wrapping_add(1);
    }
}

/// The chip that follows the pointer during a drag.
pub(crate) struct DragChip {
    source: DraggedRef,
    /// Where the pointer grabbed the row, relative to the row. GPUI places
    /// the chip at the pointer minus this offset.
    offset: Point<Pixels>,
    /// The pointer position when the drag started.
    origin: Point<Pixels>,
    /// The pointer position at the last frame, where a cancelled drag
    /// starts to glide back.
    pointer: Point<Pixels>,
    sequence: u64,
}

impl DragChip {
    fn new(
        source: DraggedRef,
        offset: Point<Pixels>,
        pointer: Point<Pixels>,
        sequence: u64,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.on_release(|chip, cx| chip.finish(cx)).detach();
        Self {
            source,
            offset,
            origin: pointer,
            pointer,
            sequence,
        }
    }

    /// Ends the drag. A drag that was not dropped glides back to its row.
    fn finish(&mut self, cx: &mut App) {
        let session = session(cx);
        let dropped = {
            let mut session = session.borrow_mut();
            if session.sequence != self.sequence {
                return;
            }
            session.source = None;
            session.hovered = None;
            session.dropped
        };
        let travelled = (self.pointer - self.origin).magnitude() as f32;
        if !dropped && travelled >= GLIDE_BACK_MIN_DISTANCE {
            fx::glide_to(
                self.pointer,
                SOURCE_ANCHOR,
                self.pointer,
                self.source.label().to_string(),
                self.source.tone,
                cx,
            );
        }
    }
}

impl Render for DragChip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.pointer = window.mouse_position();
        let (hovered, hover_sequence) = {
            let session = session(cx);
            let session = session.borrow();
            (session.hovered.clone(), session.hover_sequence)
        };
        let colors = theme::liquid_colors(cx);
        let accent = cx.theme().primary;
        let tone = if hovered.is_some() {
            accent
        } else {
            self.source.tone
        };
        let is_branch = !matches!(self.source.kind, DragKind::Commit { .. });
        let hint = match &hovered {
            Some(hovered) => h_flex()
                .h(px(18.))
                .gap_1()
                .px_1p5()
                .rounded(px(4.))
                .border_1()
                .border_color(accent.opacity(0.55))
                .bg(cx.theme().popover)
                .font_family(mono_font())
                .text_size(text_px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(accent)
                .child("→")
                .child(hovered.label.clone()),
            None => h_flex()
                .h(px(18.))
                .px_1p5()
                .rounded(px(4.))
                .bg(cx.theme().popover.opacity(0.85))
                .font_family(mono_font())
                .text_size(text_px(10.5))
                .text_color(colors.text_tertiary)
                .child("Drop on a branch or commit"),
        };

        let chip = h_flex()
            .relative()
            .h(px(26.))
            .gap_1p5()
            .px_2()
            .rounded(px(6.))
            .border_1()
            .border_color(tone)
            .bg(cx.theme().popover)
            .shadow_md()
            .overflow_hidden()
            .child(
                Dither::new(DitherField::Fade {
                    from: Edge::Left,
                    start: 0.55,
                    end: -0.1,
                })
                .color(tone.opacity(0.35))
                .grid(3.0, 1.0)
                .corner_radius(6.0)
                .layer(),
            )
            .child(if is_branch {
                icons::git_branch()
                    .xsmall()
                    .text_color(tone)
                    .into_any_element()
            } else {
                div()
                    .size(px(8.))
                    .rounded(px(4.))
                    .bg(tone)
                    .into_any_element()
            })
            .child(
                div()
                    .font_family(mono_font())
                    .text_size(text_px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().foreground)
                    .child(SharedString::from(self.source.label().to_string())),
            )
            .child(
                div()
                    .max_w(px(CHIP_SUBJECT_WIDTH))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(text_px(11.5))
                    .text_color(colors.text_secondary)
                    .child(SharedString::from(self.source.subject.clone())),
            );

        div()
            .pl(self.offset.x + px(CHIP_GAP_X))
            .pt(self.offset.y + px(CHIP_GAP_Y))
            .child(motion::drag_lift_surface(
                self.sequence,
                v_flex()
                    .items_start()
                    .gap_1()
                    .child(chip)
                    .child(motion::drop_hint_surface(hover_sequence, hint)),
            ))
    }
}

/// The menu that opens where a commit or branch was dropped.
pub(crate) struct DropMenu {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    sequence: u64,
    _dismiss: Subscription,
}

impl DropMenu {
    /// Renders the menu above the window, at the drop point.
    pub(crate) fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        popup_menu_layer(
            self.menu.clone(),
            self.position,
            Anchor::TopLeft,
            self.sequence,
            window,
            cx,
        )
    }
}

/// Lands a drop on `target` and opens the drop menu at the pointer once
/// Git has said where the dragged commit belongs. `slot` is the field of
/// the view that holds the open menu.
pub(crate) fn open_drop_menu<V: 'static>(
    state: Entity<RepositoryState>,
    source: DraggedRef,
    target: DropTarget,
    slot: fn(&mut V) -> &mut Option<DropMenu>,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    fx::impact(TARGET_ANCHOR, cx.theme().primary, cx);
    let position = window.mouse_position();
    let sequence = session(cx).borrow().sequence;
    let (repo, current_branch) = {
        let state = state.read(cx);
        (state.repo.clone(), state.current_branch.clone())
    };
    let facts_source = source.clone();
    let facts_branch = current_branch.clone();
    cx.spawn_in(window, async move |this, cx| {
        let facts = cx
            .background_executor()
            .spawn(async move { resolve_facts(&repo, &facts_source, &facts_branch) })
            .await;
        this.update_in(cx, |this, window, cx| {
            let actions = plan_drop(&source, &target, &facts, &current_branch);
            let menu = build_drop_menu(actions, source, target, facts, state, window, cx);
            let dismiss =
                cx.subscribe_in(&menu, window, move |this, _, _: &DismissEvent, _, cx| {
                    *slot(this) = None;
                    cx.notify();
                });
            *slot(this) = Some(DropMenu {
                menu,
                position,
                sequence,
                _dismiss: dismiss,
            });
            cx.notify();
        })
        .ok();
    })
    .detach();
}

fn build_drop_menu(
    actions: Vec<DropAction>,
    source: DraggedRef,
    target: DropTarget,
    facts: SourceFacts,
    state: Entity<RepositoryState>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<PopupMenu> {
    let previous_focus = window.focused(cx);
    PopupMenu::build(window, cx, move |menu, window, cx| {
        let menu = match previous_focus {
            Some(handle) => menu.action_context(handle),
            None => menu,
        };
        let header_source = source.clone();
        let header_target = target.label.clone();
        let mut menu = menu
            .min_w(px(MENU_MIN_WIDTH))
            .item(
                PopupMenuItem::element(move |_, cx| {
                    drop_header(&header_source, &header_target, cx)
                })
                .disabled(true),
            )
            .separator();
        if actions.is_empty() {
            return menu.label(empty_reason(&source, &target, &facts));
        }
        let mut group = None;
        for action in actions.iter() {
            if group.is_some_and(|group| group != action.group()) {
                menu = menu.separator();
            }
            group = Some(action.group());
            menu = add_drop_action(menu, action.clone(), state.clone(), window, cx);
        }
        menu
    })
}

/// The first menu row: the dragged item, an arrow, and the target.
fn drop_header(source: &DraggedRef, target: &str, cx: &App) -> Div {
    let accent = cx.theme().primary;
    let colors = theme::liquid_colors(cx);
    h_flex()
        .gap_1p5()
        .py_1()
        .child(drop_header_badge(source.label(), source.tone))
        .child(
            div()
                .font_family(mono_font())
                .text_size(text_px(11.))
                .text_color(colors.text_tertiary)
                .child("→"),
        )
        .child(drop_header_badge(target, accent))
}

fn drop_header_badge(label: &str, tone: Hsla) -> Div {
    h_flex()
        .h(px(18.))
        .px_1p5()
        .rounded(px(4.))
        .border_1()
        .border_color(tone.opacity(0.5))
        .bg(tone.opacity(0.14))
        .font_family(mono_font())
        .text_size(text_px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tone)
        .child(SharedString::from(label.to_string()))
}

fn add_drop_action(
    menu: PopupMenu,
    action: DropAction,
    state: Entity<RepositoryState>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let title = action.title();
    let icon = action.icon();
    if let DropAction::ResetBranch { to_hash, .. } = &action {
        return reset_submenu(menu, title, state, to_hash.clone(), window, cx);
    }
    menu.item(
        PopupMenuItem::new(title)
            .icon(icon)
            .on_click(move |_, window, cx| run_drop_action(&action, state.clone(), window, cx)),
    )
}

fn run_drop_action(
    action: &DropAction,
    state: Entity<RepositoryState>,
    window: &mut Window,
    cx: &mut App,
) {
    match action.clone() {
        DropAction::Merge {
            source_ref,
            source_label,
            into,
            checkout,
            ..
        } => {
            let view = cx.new(|cx| {
                let dialog =
                    MergeDialogView::new(state, source_ref, source_label, into.clone(), window, cx);
                if checkout {
                    dialog.checkout_first(into)
                } else {
                    dialog
                }
            });
            open_liquid_dialog(view, px(460.), window, cx);
        }
        DropAction::Rebase {
            branch,
            onto_ref,
            onto_label,
            start,
            checkout,
        } => {
            let view = cx.new(|cx| {
                let mut dialog =
                    RebaseDialogView::new(state, onto_ref, onto_label, branch.clone(), false, cx);
                if checkout {
                    dialog = dialog.rebase_branch(branch);
                }
                if let Some(start) = start {
                    dialog = dialog.starting_at(start.after, start.first_commit, start.count);
                }
                dialog
            });
            open_rebase_dialog(view, window, cx);
        }
        DropAction::InteractiveRebase {
            branch,
            onto_ref,
            onto_label,
        } => {
            let view =
                cx.new(|cx| RebaseDialogView::new(state, onto_ref, onto_label, branch, true, cx));
            open_rebase_dialog(view, window, cx);
        }
        DropAction::CherryPick {
            revisions,
            onto,
            checkout,
            ..
        } => {
            run_git(
                &state,
                "Cherry-picking",
                move |repo| {
                    if checkout {
                        repo.checkout(&onto)?;
                    }
                    repo.cherry_pick(&revisions)
                },
                cx,
            );
        }
        DropAction::MoveBranch {
            branch,
            to_ref,
            to_label,
        } => {
            let message = format!(
                "{branch} will point to {to_label}. Commits that only {branch} reaches now can become difficult to find."
            );
            let target = SharedString::from(branch.clone());
            confirm_destructive_action(
                "Move Branch?",
                message,
                Some(target),
                "Move Branch",
                move |cx| {
                    run_git(
                        &state,
                        "Moving branch",
                        move |repo| repo.move_branch(&branch, &to_ref),
                        cx,
                    );
                },
                window,
                cx,
            );
        }
        // The reset opens its own submenu, so it never gets here.
        DropAction::ResetBranch { .. } => {}
    }
}

fn run_git(
    state: &Entity<RepositoryState>,
    label: &str,
    operation: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
    cx: &mut App,
) {
    let hide = GitSettings::get(cx).hide_checkpoint_commits;
    state.update(cx, |state, cx| {
        state.run_operation(label, hide, operation, cx);
    });
}
