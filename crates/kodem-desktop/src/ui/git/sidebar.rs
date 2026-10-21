//! The repository sidebar beside the workspace rail. A category rail at the top switches between
//! branches, worktrees, remotes, tags, stashes, checkpoints, stale branches and LFS patterns.
//! Branches group into folders by their name prefixes, every row has a context menu with its Git
//! actions, and branch rows take part in commit and branch drag and drop.
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, ClipboardItem, Context, Div, Entity, FontWeight, IntoElement, Render,
    SharedString, Window, div,
};
use kodem_git::git::{Branch, BranchCheckout, GitRepo};

use crate::theme::{RADIUS_CONTROL, px, text_px};
use crate::ui::fx;
use crate::ui::git::components::{
    self, count_badge, dither_empty_hint, mono_font, mono_label, section_header, tab_cell,
    tab_indicator, tab_rail,
};
use crate::ui::git::contextual_actions::{
    CheckoutDialogView, CreateBranchDialogView, MergeDialogView, RebaseDialogView,
    RemoveWorktreeDialogView, StaleBranchesDialogView, TrackLfsDialogView, WorktreeApplyDialogView,
    branch_folder_prefixes, confirm_destructive_action, open_liquid_dialog, open_rebase_dialog,
    reset_to_here_menu,
};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ContextMenuExt as _, Icon, IconName, PopupMenu, PopupMenuItem,
    ScrollableElement as _, Sizable as _, Tooltip, h_flex, v_flex,
};
use crate::ui::git::ref_drag::{
    DraggedRef, DropMenu, DropTarget, RefDragExt as _, drag_layer, drag_role, open_drop_menu,
};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::state::RepositoryState;
use crate::ui::git::theme;
use crate::ui::kit;

/// A group of repository items that the sidebar lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarCategory {
    Branches,
    Worktrees,
    Remotes,
    Tags,
    Stashes,
    Checkpoints,
    Stale,
    Lfs,
}

impl SidebarCategory {
    const ALL: [SidebarCategory; 8] = [
        SidebarCategory::Branches,
        SidebarCategory::Worktrees,
        SidebarCategory::Remotes,
        SidebarCategory::Tags,
        SidebarCategory::Stashes,
        SidebarCategory::Checkpoints,
        SidebarCategory::Stale,
        SidebarCategory::Lfs,
    ];

    fn title(&self) -> &'static str {
        match self {
            SidebarCategory::Branches => "Branches",
            SidebarCategory::Worktrees => "Worktrees",
            SidebarCategory::Remotes => "Remotes",
            SidebarCategory::Tags => "Tags",
            SidebarCategory::Stashes => "Stashes",
            SidebarCategory::Checkpoints => "Checkpoints",
            SidebarCategory::Stale => "Stale",
            SidebarCategory::Lfs => "LFS",
        }
    }

    fn icon(&self) -> Icon {
        match self {
            SidebarCategory::Branches => icons::git_commit(),
            SidebarCategory::Worktrees => icons::layers(),
            SidebarCategory::Remotes => Icon::new(IconName::Globe),
            SidebarCategory::Tags => icons::tag(),
            SidebarCategory::Stashes => icons::archive(),
            SidebarCategory::Checkpoints => icons::clock(),
            SidebarCategory::Stale => icons::trash(),
            SidebarCategory::Lfs => icons::file_text(),
        }
    }
}

/// Splits local branches into active and stale ones, with the same
/// rules as the original app: a branch is stale when its upstream is
/// gone, or when no remote branch shares its name. The current branch
/// and main/master are never stale.
fn split_stale(branches: &[Branch]) -> (Vec<Branch>, Vec<Branch>) {
    let remote_names: HashSet<&str> = branches
        .iter()
        .filter(|b| b.is_remote)
        .map(|b| b.name.as_str())
        .collect();
    let remote_display: HashSet<&str> = branches
        .iter()
        .filter(|b| b.is_remote)
        .map(|b| b.name.split_once('/').map_or(b.name.as_str(), |(_, n)| n))
        .collect();

    let mut active = Vec::new();
    let mut stale = Vec::new();
    for branch in branches.iter().filter(|b| !b.is_remote) {
        let name = branch.name.to_lowercase();
        let protected = branch.is_current || name == "main" || name == "master";
        let is_stale = !protected
            && match &branch.upstream {
                Some(upstream) if !upstream.is_empty() => !remote_names.contains(upstream.as_str()),
                _ => !remote_display.contains(branch.name.as_str()),
            };
        if is_stale {
            stale.push(branch.clone());
        } else {
            active.push(branch.clone());
        }
    }
    (active, stale)
}

/// One node of the branch tree.
#[derive(Default)]
struct BranchNode {
    children: BTreeMap<String, BranchNode>,
    branch: Option<Branch>,
}

impl BranchNode {
    fn insert(&mut self, parts: &[&str], branch: &Branch) {
        if parts.is_empty() {
            return;
        }
        let child = self.children.entry(parts[0].to_string()).or_default();
        if parts.len() == 1 {
            child.branch = Some(branch.clone());
        } else {
            child.insert(&parts[1..], branch);
        }
    }

    fn leaf_count(&self) -> usize {
        let own = usize::from(self.branch.is_some());
        own + self
            .children
            .values()
            .map(|c| c.leaf_count())
            .sum::<usize>()
    }
}

/// A flattened row of the branch tree, ready to render.
enum TreeRow {
    Folder {
        key: String,
        name: String,
        depth: usize,
        count: usize,
        expanded: bool,
    },
    Leaf {
        name: String,
        depth: usize,
        branch: Branch,
    },
}

/// Anchor on the current branch row, where a branch switch lights up and its name flies from.
const CURRENT_BRANCH_ANCHOR: &str = "branch-current";

/// The name of the command that shows a folder in the system file manager.
#[cfg(target_os = "macos")]
pub(crate) const REVEAL_LABEL: &str = "Reveal in Finder";
/// The name of the command that shows a folder in the system file manager.
#[cfg(target_os = "windows")]
pub(crate) const REVEAL_LABEL: &str = "Show in Explorer";
/// The name of the command that shows a folder in the system file manager.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) const REVEAL_LABEL: &str = "Show in File Manager";

fn branch_checkout(branch: &Branch) -> Option<BranchCheckout> {
    if branch.is_current {
        return None;
    }
    if branch.is_remote {
        Some(BranchCheckout::Remote {
            reference: branch.name.clone(),
            local_name: branch.display_name().to_string(),
        })
    } else {
        Some(BranchCheckout::Local(branch.name.clone()))
    }
}

fn branch_row_checkout(branch: &Branch, click_count: usize) -> Option<BranchCheckout> {
    (click_count == 2)
        .then(|| branch_checkout(branch))
        .flatten()
}

/// The repository sidebar: the category rail, the section header and the list of the selected
/// category.
pub struct SidebarView {
    state: Entity<RepositoryState>,
    category: SidebarCategory,
    collapsed_folders: HashSet<String>,
    /// The branch that was current at the last render, to notice when the
    /// user switches branches.
    seen_branch: Option<String>,
    /// The menu that opened where a commit or branch was dropped.
    drop_menu: Option<DropMenu>,
}

impl SidebarView {
    /// Creates the sidebar for a repository. It shows the branches first.
    pub fn new(
        state: Entity<RepositoryState>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            category: SidebarCategory::Branches,
            collapsed_folders: HashSet::new(),
            seen_branch: None,
            drop_menu: None,
        }
    }

    fn hide_checkpoints(cx: &App) -> bool {
        GitSettings::get(cx).hide_checkpoint_commits
    }

    fn run_git(
        &mut self,
        label: &str,
        op: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let hide = Self::hide_checkpoints(cx);
        self.state.update(cx, |state, cx| {
            state.run_operation(label, hide, op, cx);
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn confirm_git_action(
        &self,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        target: Option<SharedString>,
        confirm_label: impl Into<SharedString>,
        operation_label: &'static str,
        operation: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.clone();
        let hide = Self::hide_checkpoints(cx);
        confirm_destructive_action(
            title,
            message,
            target,
            confirm_label,
            move |cx| {
                state.update(cx, |state, cx| {
                    state.run_operation(operation_label, hide, operation, cx);
                });
            },
            window,
            cx,
        );
    }

    fn flatten_tree(&self, branches: &[Branch], use_display_name: bool) -> Vec<TreeRow> {
        let mut root = BranchNode::default();
        for branch in branches {
            let path = if use_display_name {
                branch.display_name()
            } else {
                branch.name.as_str()
            };
            let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
            root.insert(&parts, branch);
        }
        let mut rows = Vec::new();
        self.flatten_node(&root, "", 0, &mut rows);
        rows
    }

    fn flatten_node(&self, node: &BranchNode, prefix: &str, depth: usize, rows: &mut Vec<TreeRow>) {
        for (name, child) in &node.children {
            let key = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if child.children.is_empty() {
                if let Some(branch) = &child.branch {
                    rows.push(TreeRow::Leaf {
                        name: name.clone(),
                        depth,
                        branch: branch.clone(),
                    });
                }
            } else {
                let expanded = !self.collapsed_folders.contains(&key);
                rows.push(TreeRow::Folder {
                    key: key.clone(),
                    name: name.clone(),
                    depth,
                    count: child.leaf_count(),
                    expanded,
                });
                if expanded {
                    if let Some(branch) = &child.branch {
                        rows.push(TreeRow::Leaf {
                            name: name.clone(),
                            depth: depth + 1,
                            branch: branch.clone(),
                        });
                    }
                    self.flatten_node(child, &key, depth + 1, rows);
                }
            }
        }
    }

    /// When the current branch changes while the branch list is shown,
    /// the new branch lights up and its name flies to the title bar. It
    /// plays on the next frame, once the new row has been painted.
    fn notice_branch_switch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.state.read(cx).current_branch.clone();
        let switched = self
            .seen_branch
            .as_ref()
            .is_some_and(|seen| *seen != current);
        if switched && !current.is_empty() && self.category == SidebarCategory::Branches {
            let accent = cx.theme().primary;
            let label = current.clone();
            window.on_next_frame(move |_, cx| {
                fx::impact(CURRENT_BRANCH_ANCHOR, accent, cx);
                fx::fly_between(
                    CURRENT_BRANCH_ANCHOR,
                    "title-branch",
                    label,
                    accent,
                    Duration::from_millis(120),
                    cx,
                );
            });
        }
        self.seen_branch = Some(current);
    }

    fn checkout_branch(
        &mut self,
        checkout: BranchCheckout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.clone();
        let view = cx.new(|cx| CheckoutDialogView::new(state, checkout, cx));
        open_liquid_dialog(view, px(460.), window, cx);
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

    fn open_merge_dialog(
        &mut self,
        source_ref: String,
        source: String,
        target: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.clone();
        let view = cx.new(|cx| MergeDialogView::new(state, source_ref, source, target, window, cx));
        open_liquid_dialog(view, px(460.), window, cx);
    }

    fn open_rebase_dialog(
        &mut self,
        upstream_ref: String,
        source: String,
        target: String,
        interactive: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.clone();
        let view = cx
            .new(|cx| RebaseDialogView::new(state, upstream_ref, source, target, interactive, cx));
        open_rebase_dialog(view, window, cx);
    }

    fn branch_menu(
        menu: PopupMenu,
        window: &mut Window,
        menu_cx: &mut Context<PopupMenu>,
        sidebar: Entity<Self>,
        branch: &Branch,
        current_branch: &str,
    ) -> PopupMenu {
        let is_remote = branch.is_remote;
        let is_current = branch.is_current;
        let full_name = branch.name.clone();
        let display = branch.display_name().to_string();
        let hash = branch.hash.clone();
        let short_hash = branch.short_hash.clone();
        let subject = branch.subject.clone();
        let current = current_branch.to_string();

        let mut menu = menu;
        if let Some(checkout) = branch_checkout(branch) {
            let sidebar_c = sidebar.clone();
            menu = menu.item(
                PopupMenuItem::new("Checkout...")
                    .icon(IconName::ArrowRight)
                    .on_click(move |_, window, cx| {
                        let checkout = checkout.clone();
                        sidebar_c.update(cx, |this, cx| {
                            this.checkout_branch(checkout, window, cx);
                        });
                    }),
            );
            let sidebar_m = sidebar.clone();
            let merge_name = full_name.clone();
            let merge_display = display.clone();
            let merge_current = current.clone();
            menu = menu.item(
                PopupMenuItem::new(format!("Merge into {current}..."))
                    .icon(icons::git_merge())
                    .on_click(move |_, window, cx| {
                        let merge_name = merge_name.clone();
                        let merge_display = merge_display.clone();
                        let merge_current = merge_current.clone();
                        sidebar_m.update(cx, |this, cx| {
                            this.open_merge_dialog(
                                merge_name,
                                merge_display,
                                merge_current,
                                window,
                                cx,
                            );
                        });
                    }),
            );
            let sidebar_r = sidebar.clone();
            let rebase_name = full_name.clone();
            let rebase_display = display.clone();
            let rebase_current = current.clone();
            menu = menu.item(
                PopupMenuItem::new(format!("Rebase {current} onto {display}..."))
                    .icon(icons::refresh())
                    .on_click(move |_, window, cx| {
                        let rebase_name = rebase_name.clone();
                        let rebase_display = rebase_display.clone();
                        let rebase_current = rebase_current.clone();
                        sidebar_r.update(cx, |this, cx| {
                            this.open_rebase_dialog(
                                rebase_name,
                                rebase_display,
                                rebase_current,
                                false,
                                window,
                                cx,
                            );
                        });
                    }),
            );
            let sidebar_i = sidebar.clone();
            let rebase_name = full_name.clone();
            let rebase_display = display.clone();
            let rebase_current = current.clone();
            menu = menu.item(
                PopupMenuItem::new(format!("Interactive Rebase {current} onto {display}..."))
                    .icon(IconName::Settings)
                    .on_click(move |_, window, cx| {
                        let rebase_name = rebase_name.clone();
                        let rebase_display = rebase_display.clone();
                        let rebase_current = rebase_current.clone();
                        sidebar_i.update(cx, |this, cx| {
                            this.open_rebase_dialog(
                                rebase_name,
                                rebase_display,
                                rebase_current,
                                true,
                                window,
                                cx,
                            );
                        });
                    }),
            );
            menu = menu.separator();
        }

        if is_remote {
            let copy_display = display.clone();
            menu = menu
                .item(
                    PopupMenuItem::new("Copy Branch Name")
                        .icon(IconName::Copy)
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_display.clone()));
                        }),
                )
                .separator();
        }

        let sidebar_cp = sidebar.clone();
        let cherry_hash = hash.clone();
        menu = menu.item(
            PopupMenuItem::new("Cherry-pick")
                .icon(IconName::Copy)
                .on_click(move |_, _, cx| {
                    let cherry_hash = cherry_hash.clone();
                    sidebar_cp.update(cx, |this, cx| {
                        this.run_git(
                            "Cherry-picking",
                            move |repo| repo.cherry_pick(&cherry_hash),
                            cx,
                        );
                    });
                }),
        );
        let sidebar_rv = sidebar.clone();
        let revert_hash = hash.clone();
        menu = menu.item(PopupMenuItem::new("Revert").icon(IconName::Undo).on_click(
            move |_, _, cx| {
                let revert_hash = revert_hash.clone();
                sidebar_rv.update(cx, |this, cx| {
                    this.run_git("Reverting", move |repo| repo.revert(&revert_hash), cx);
                });
            },
        ));
        menu = menu.separator();

        let sidebar_cb = sidebar.clone();
        let start_ref = full_name.clone();
        let detail = display.clone();
        menu = menu.item(
            PopupMenuItem::new("Create Branch Here...")
                .icon(IconName::Plus)
                .on_click(move |_, window, cx| {
                    let start_ref = start_ref.clone();
                    let detail = detail.clone();
                    sidebar_cb.update(cx, |this, cx| {
                        this.open_create_branch_dialog(start_ref, detail, window, cx);
                    });
                }),
        );

        menu = reset_to_here_menu(
            menu,
            sidebar.read(menu_cx).state.clone(),
            hash.clone(),
            window,
            menu_cx,
        );

        menu = menu.separator();
        if !is_remote {
            let copy_name = full_name.clone();
            menu = menu.item(
                PopupMenuItem::new("Copy Branch Name")
                    .icon(IconName::Copy)
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_name.clone()));
                    }),
            );
        }
        let copy_hash = hash.clone();
        let copy_short = short_hash.clone();
        let copy_subject = subject.clone();
        menu = menu
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
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_short.clone()));
                    }),
            )
            .item(
                PopupMenuItem::new("Copy Commit Message")
                    .icon(IconName::Copy)
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_subject.clone()));
                    }),
            );

        if !is_remote && !is_current {
            let delete_name = full_name;
            let sidebar_d = sidebar;
            menu = menu.separator().item(
                PopupMenuItem::new("Delete").icon(icons::trash()).on_click(
                    move |_, window, cx| {
                        let delete_name = delete_name.clone();
                        sidebar_d.update(cx, |this, cx| {
                            this.confirm_git_action(
                                "Delete Branch?",
                                "This local branch will be deleted. Unmerged commits can become difficult to recover.",
                                Some(SharedString::from(delete_name.clone())),
                                "Delete Branch",
                                "Deleting branch",
                                move |repo| repo.delete_branch(&delete_name, false),
                                window,
                                cx,
                            );
                        });
                    },
                ),
            );
        }

        menu
    }

    /// Horizontal category rail at the top of the sidebar, as in the
    /// original app.
    fn render_top_rail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        tab_rail(cx).children(
            SidebarCategory::ALL
                .iter()
                .enumerate()
                .map(|(ix, category)| {
                    let category = *category;
                    let selected = self.category == category;
                    let title = category.title();
                    tab_cell(selected, cx)
                        .id(("category", ix))
                        .flex_1()
                        .tooltip(move |window, cx| Tooltip::new(title).build(window, cx))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.category = category;
                            if category == SidebarCategory::Lfs {
                                this.state.update(cx, |state, cx| {
                                    if state.lfs.is_none() {
                                        state.refresh_lfs(cx);
                                    }
                                });
                            }
                            cx.notify();
                        }))
                        .child(category.icon().small().text_color(if selected {
                            cx.theme().foreground
                        } else {
                            cx.theme().muted_foreground
                        }))
                        .child(tab_indicator(selected, cx))
                        .context_menu({
                            let sidebar = cx.entity();
                            move |menu, _, _| Self::category_menu(menu, category, sidebar.clone())
                        })
                }),
        )
    }

    fn category_menu(
        menu: PopupMenu,
        category: SidebarCategory,
        sidebar: Entity<Self>,
    ) -> PopupMenu {
        match category {
            SidebarCategory::Worktrees => menu.item(
                PopupMenuItem::new("Remove All Stale Worktrees")
                    .icon(icons::trash())
                    .on_click(move |_, window, cx| {
                        sidebar.update(cx, |this, cx| {
                            this.confirm_git_action(
                                "Remove All Stale Worktrees?",
                                "Stale worktree records will be removed from this repository. Existing folders will not be deleted.",
                                None,
                                "Remove Stale Worktrees",
                                "Pruning worktrees",
                                |repo| repo.prune_worktrees(),
                                window,
                                cx,
                            );
                        });
                    }),
            ),
            SidebarCategory::Stale => menu.item(
                PopupMenuItem::new("Remove Stale Local Branches...")
                    .icon(icons::trash())
                    .on_click(move |_, window, cx| {
                        sidebar.update(cx, |this, cx| {
                            let (_, stale) = split_stale(&this.state.read(cx).branches);
                            let names: Vec<String> = stale.into_iter().map(|b| b.name).collect();
                            if names.is_empty() {
                                return;
                            }
                            let state = this.state.clone();
                            let view = cx.new(|_| StaleBranchesDialogView::new(state, names));
                            open_liquid_dialog(view, px(500.), window, cx);
                        });
                    }),
            ),
            SidebarCategory::Stashes => menu.item(
                PopupMenuItem::new("Stash Current Changes")
                    .icon(IconName::Plus)
                    .on_click(move |_, _, cx| {
                        sidebar.update(cx, |this, cx| {
                            this.run_git("Stashing", |repo| repo.stash_push(None), cx);
                        });
                    }),
            ),
            SidebarCategory::Lfs => menu
                .item(
                    PopupMenuItem::new("Track Pattern...")
                        .icon(IconName::Plus)
                        .on_click({
                            let sidebar = sidebar.clone();
                            move |_, window, cx| {
                                sidebar.update(cx, |this, cx| {
                                    let state = this.state.clone();
                                    let view = cx.new(|cx| {
                                        TrackLfsDialogView::new(state, window, cx)
                                    });
                                    let focus_view = view.clone();
                                    open_liquid_dialog(view, px(420.), window, cx);
                                    window.defer(cx, move |window, cx| {
                                        let focus_handle =
                                            focus_view.read(cx).input_focus_handle(cx);
                                        focus_handle.focus(window, cx);
                                    });
                                });
                            }
                        }),
                )
                .separator()
                .item(
                    PopupMenuItem::new("Fetch LFS Objects").icon(icons::pull()).on_click({
                        let sidebar = sidebar.clone();
                        move |_, _, cx| {
                            sidebar.update(cx, |this, cx| {
                                this.run_git("Fetching LFS", |repo| repo.lfs_fetch(), cx);
                            });
                        }
                    }),
                )
                .item(
                    PopupMenuItem::new("Pull LFS Objects").icon(icons::pull()).on_click({
                        let sidebar = sidebar.clone();
                        move |_, _, cx| {
                            sidebar.update(cx, |this, cx| {
                                this.run_git("Pulling LFS", |repo| repo.lfs_pull(), cx);
                            });
                        }
                    }),
                )
                .item(
                    PopupMenuItem::new("Push LFS Objects").icon(icons::push()).on_click({
                        let sidebar = sidebar.clone();
                        move |_, _, cx| {
                            sidebar.update(cx, |this, cx| {
                                this.run_git("Pushing LFS", |repo| repo.lfs_push(), cx);
                            });
                        }
                    }),
                )
                .separator()
                .item(
                    PopupMenuItem::new("Prune Old Objects").icon(icons::trash()).on_click({
                        let sidebar = sidebar.clone();
                        move |_, window, cx| {
                            sidebar.update(cx, |this, cx| {
                                this.confirm_git_action(
                                    "Prune Old LFS Objects?",
                                    "Unused local LFS objects will be deleted. You must download them again if they are needed later.",
                                    Some(SharedString::from("Local LFS object store")),
                                    "Prune Objects",
                                    "Pruning LFS",
                                    |repo| repo.lfs_prune(),
                                    window,
                                    cx,
                                );
                            });
                        }
                    }),
                ),
            _ => menu,
        }
    }

    fn empty_state(text: &'static str, cx: &App) -> Div {
        dither_empty_hint(text, cx)
    }

    /// Number of items in the selected category, for the section header.
    fn category_count(&self, cx: &App) -> Option<usize> {
        let state = self.state.read(cx);
        match self.category {
            SidebarCategory::Branches => Some(split_stale(&state.branches).0.len()),
            SidebarCategory::Stale => Some(split_stale(&state.branches).1.len()),
            SidebarCategory::Worktrees => Some(state.worktrees.len()),
            SidebarCategory::Remotes => Some(state.remotes.len()),
            SidebarCategory::Tags => Some(state.tags.len()),
            SidebarCategory::Stashes => Some(state.stashes.len()),
            SidebarCategory::Checkpoints => Some(state.checkpoints.len()),
            SidebarCategory::Lfs => state.lfs.as_ref().map(|info| info.patterns.len()),
        }
    }

    fn render_branch_tree(
        &mut self,
        branches: &[Branch],
        use_display_name: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let rows = self.flatten_tree(branches, use_display_name);
        let sidebar = cx.entity();
        let current_branch = self.state.read(cx).current_branch.clone();
        v_flex()
            .w_full()
            .children(rows.into_iter().enumerate().map(|(ix, row)| match row {
                TreeRow::Folder {
                    key,
                    name,
                    depth,
                    count,
                    expanded,
                } => {
                    let key_for_click = key.clone();
                    div()
                        .id(("folder", ix))
                        .w_full()
                        .h(px(26.))
                        .pl(px(8.0 + depth as f32 * 12.0))
                        .pr_2()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .rounded(px(RADIUS_CONTROL))
                        .hover(|s| s.bg(cx.theme().list_hover))
                        .active(|s| s.bg(cx.theme().button_active))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.collapsed_folders.contains(&key_for_click) {
                                this.collapsed_folders.remove(&key_for_click);
                            } else {
                                this.collapsed_folders.insert(key_for_click.clone());
                            }
                            cx.notify();
                        }))
                        .child(
                            Icon::new(if expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            Icon::new(IconName::Folder)
                                .small()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(text_px(12.5))
                                .text_color(cx.theme().muted_foreground)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(SharedString::from(name)),
                        )
                        .child(count_badge(count, cx))
                        .into_any_element()
                }
                TreeRow::Leaf {
                    name,
                    depth,
                    branch,
                } => {
                    let is_current = branch.is_current && !branch.is_remote;
                    let checkout_branch = branch.clone();
                    let sidebar = sidebar.clone();
                    let current_branch = current_branch.clone();
                    let menu_branch = branch.clone();
                    let accent = cx.theme().primary;
                    let drag_tone = if branch.is_remote {
                        theme::liquid_colors(cx).info
                    } else {
                        accent
                    };
                    let drop_target = DropTarget::branch(&branch);
                    let drag_role = drag_role(&drop_target, cx);
                    div()
                        .id(("branch", ix))
                        .relative()
                        .w_full()
                        .h(px(26.))
                        .pl(px(8.0 + depth as f32 * 12.0))
                        .pr_2()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .rounded(px(RADIUS_CONTROL))
                        .overflow_hidden()
                        .when(is_current, |s| components::selected_row(s, accent))
                        .when(!is_current, |s| {
                            s.hover(|s| s.bg(cx.theme().list_hover))
                                .active(|s| s.bg(cx.theme().button_active))
                        })
                        .when(is_current, |s| s.child(fx::anchor(CURRENT_BRANCH_ANCHOR)))
                        .when(drag_role.lifted, |s| s.opacity(0.5))
                        .children(drag_layer(drag_role, RADIUS_CONTROL, true, cx))
                        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                            if let Some(checkout) =
                                branch_row_checkout(&checkout_branch, event.click_count())
                            {
                                this.checkout_branch(checkout, window, cx);
                            }
                        }))
                        .ref_drag(
                            DraggedRef::branch(&branch, drag_tone),
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
                        .child(
                            div()
                                .relative()
                                .child(icons::git_branch().xsmall().text_color(if is_current {
                                    cx.theme().primary
                                } else {
                                    cx.theme().muted_foreground
                                })),
                        )
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .min_w_0()
                                .text_size(text_px(12.5))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .when(is_current, |s| s.font_weight(FontWeight::SEMIBOLD))
                                .child(SharedString::from(name)),
                        )
                        .when(branch.ahead > 0 || branch.behind > 0, |this| {
                            this.child(
                                div()
                                    .relative()
                                    .font_family(mono_font())
                                    .text_size(text_px(10.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("↑{} ↓{}", branch.ahead, branch.behind)),
                            )
                        })
                        .when(is_current, |this| {
                            this.child(Icon::new(IconName::Check).xsmall().text_color(accent))
                        })
                        .context_menu(move |menu, window, menu_cx| {
                            Self::branch_menu(
                                menu,
                                window,
                                menu_cx,
                                sidebar.clone(),
                                &menu_branch,
                                &current_branch,
                            )
                        })
                        .into_any_element()
                }
            }))
    }

    fn render_worktrees(&mut self, cx: &mut Context<Self>) -> Div {
        let worktrees = self.state.read(cx).worktrees.clone();
        if worktrees.is_empty() {
            return Self::empty_state("No worktrees", cx);
        }
        let target = self.state.read(cx).current_branch.clone();
        let target_status = &self.state.read(cx).status;
        let has_target_changes = !target_status.staged.is_empty()
            || !target_status.unstaged.is_empty()
            || !target_status.conflicted.is_empty();
        let sidebar = cx.entity();
        v_flex()
            .w_full()
            .children(worktrees.into_iter().enumerate().map(|(ix, wt)| {
                let name = wt
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let path = wt.path.clone();
                let path_copy = wt.path.clone();
                let path_apply = wt.path.clone();
                let path_remove = wt.path.clone();
                let is_current = wt.is_current;
                let apply_name = name.clone();
                let apply_target = target.clone();
                let sidebar = sidebar.clone();
                h_flex()
                    .id(("worktree", ix))
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .rounded(px(RADIUS_CONTROL))
                    .hover(|s| s.bg(cx.theme().list_hover))
                    .active(|s| s.bg(cx.theme().button_active))
                    .on_click(move |_, _, cx| {
                        cx.reveal_path(&path);
                    })
                    .child(
                        icons::layers()
                            .small()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(text_px(12.5))
                                    .child(SharedString::from(name.clone())),
                            )
                            .child(
                                div()
                                    .text_size(text_px(11.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from(
                                        wt.branch.clone().unwrap_or_else(|| wt.head.clone()),
                                    )),
                            ),
                    )
                    .when(is_current, |this| {
                        this.child(kit::chip("CURRENT", cx.theme().primary))
                    })
                    .context_menu(move |menu, _, _| {
                        let path_copy = path_copy.clone();
                        let path_reveal = path_copy.clone();
                        let path_apply = path_apply.clone();
                        let path_remove = path_remove.clone();
                        let apply_name = apply_name.clone();
                        let apply_target = apply_target.clone();
                        let sidebar = sidebar.clone();
                        menu.item(
                            PopupMenuItem::new(REVEAL_LABEL)
                                .icon(IconName::ExternalLink)
                                .on_click(move |_, _, cx| {
                                    cx.reveal_path(&path_reveal);
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Copy Path")
                                .icon(IconName::Copy)
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        path_copy.display().to_string(),
                                    ));
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Apply Changes...")
                                .icon(icons::pull())
                                .disabled(is_current)
                                .on_click({
                                    let sidebar = sidebar.clone();
                                    move |_, window, cx| {
                                        let path_apply = path_apply.clone();
                                        let apply_name = apply_name.clone();
                                        let apply_target = apply_target.clone();
                                        sidebar.update(cx, |this, cx| {
                                            let state = this.state.clone();
                                            let repo = this.state.read(cx).repo.clone();
                                            let load_path = path_apply.clone();
                                            let view = cx.new(|cx| {
                                                WorktreeApplyDialogView::new(
                                                    state,
                                                    path_apply,
                                                    apply_name,
                                                    apply_target,
                                                    has_target_changes,
                                                    cx,
                                                )
                                            });
                                            let load_view = view.clone();
                                            cx.spawn(async move |_, cx| {
                                                let task =
                                                    cx.background_executor().spawn(async move {
                                                        repo.worktree_status(&load_path)
                                                    });
                                                let result = task.await;
                                                load_view.update(cx, |view, cx| {
                                                    view.set_status(result, cx)
                                                });
                                            })
                                            .detach();
                                            open_liquid_dialog(view, px(520.), window, cx);
                                        });
                                    }
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Remove Worktree...")
                                .icon(icons::trash())
                                .disabled(is_current)
                                .on_click(move |_, window, cx| {
                                    let path_remove = path_remove.clone();
                                    sidebar.update(cx, |this, cx| {
                                        let state = this.state.clone();
                                        let view = cx.new(|_| {
                                            RemoveWorktreeDialogView::new(state, path_remove)
                                        });
                                        open_liquid_dialog(view, px(460.), window, cx);
                                    });
                                }),
                        )
                    })
            }))
    }

    fn render_remotes(&mut self, cx: &mut Context<Self>) -> Div {
        let remotes = self.state.read(cx).remotes.clone();
        if remotes.is_empty() {
            return Self::empty_state("No remotes", cx);
        }
        let all_branches = self.state.read(cx).branches.clone();
        let sidebar = cx.entity();
        let mut items = Vec::new();
        for (ix, remote) in remotes.into_iter().enumerate() {
            let group_key = format!("remote-group:{}", remote.name);
            let expanded = !self.collapsed_folders.contains(&group_key);
            let remote_branches: Vec<Branch> = all_branches
                .iter()
                .filter(|b| b.is_remote && b.remote_name() == Some(remote.name.as_str()))
                .cloned()
                .collect();
            let count = remote_branches.len();
            let tree = if expanded && !remote_branches.is_empty() {
                Some(self.render_branch_tree(&remote_branches, true, cx))
            } else {
                None
            };
            let url = remote.fetch_url.clone();
            let name = remote.name.clone();
            let sidebar = sidebar.clone();
            items.push(
                v_flex()
                    .w_full()
                    .child(
                        h_flex()
                            .id(("remote", ix))
                            .w_full()
                            .px_2()
                            .py_1()
                            .gap_2()
                            .rounded(px(RADIUS_CONTROL))
                            .hover(|s| s.bg(cx.theme().list_hover))
                            .active(|s| s.bg(cx.theme().button_active))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.collapsed_folders.contains(&group_key) {
                                    this.collapsed_folders.remove(&group_key);
                                } else {
                                    this.collapsed_folders.insert(group_key.clone());
                                }
                                cx.notify();
                            }))
                            .child(
                                Icon::new(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .xsmall()
                                .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                Icon::new(IconName::Globe)
                                    .small()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_size(text_px(12.5))
                                            .child(SharedString::from(remote.name)),
                                    )
                                    .child(
                                        div()
                                            .font_family(mono_font())
                                            .text_size(text_px(10.5))
                                            .text_color(cx.theme().muted_foreground)
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .child(SharedString::from(remote.fetch_url.clone())),
                                    ),
                            )
                            .child(count_badge(count, cx))
                            .context_menu(move |menu, _, _| {
                                let url = url.clone();
                                let name = name.clone();
                                let sidebar = sidebar.clone();
                                let is_ssh = url.starts_with("git@") || url.starts_with("ssh://");
                                let menu = if is_ssh {
                                    if let Some(https) = ssh_to_https(&url) {
                                        let name = name.clone();
                                        let sidebar = sidebar.clone();
                                        menu.item(
                                            PopupMenuItem::new("Switch to HTTPS")
                                                .icon(icons::refresh())
                                                .on_click(move |_, _, cx| {
                                                    let name = name.clone();
                                                    let https = https.clone();
                                                    sidebar.update(cx, |this, cx| {
                                                        this.run_git(
                                                            "Updating remote",
                                                            move |repo| {
                                                                repo.set_remote_url(&name, &https)
                                                            },
                                                            cx,
                                                        );
                                                    });
                                                }),
                                        )
                                    } else {
                                        menu
                                    }
                                } else if let Some(ssh) = https_to_ssh(&url) {
                                    let name = name.clone();
                                    let sidebar = sidebar.clone();
                                    menu.item(
                                        PopupMenuItem::new("Switch to SSH")
                                            .icon(icons::refresh())
                                            .on_click(move |_, _, cx| {
                                                let name = name.clone();
                                                let ssh = ssh.clone();
                                                sidebar.update(cx, |this, cx| {
                                                    this.run_git(
                                                        "Updating remote",
                                                        move |repo| {
                                                            repo.set_remote_url(&name, &ssh)
                                                        },
                                                        cx,
                                                    );
                                                });
                                            }),
                                    )
                                } else {
                                    menu
                                };
                                menu.item(
                                    PopupMenuItem::new("Copy URL")
                                        .icon(IconName::Copy)
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                url.clone(),
                                            ));
                                        }),
                                )
                            }),
                    )
                    .when_some(tree, |this, tree| this.child(tree)),
            );
        }
        v_flex().w_full().children(items)
    }

    fn render_tags(&mut self, cx: &mut Context<Self>) -> Div {
        let tags = self.state.read(cx).tags.clone();
        if tags.is_empty() {
            return Self::empty_state("No tags", cx);
        }
        v_flex()
            .w_full()
            .children(tags.into_iter().enumerate().map(|(ix, tag)| {
                let name = tag.name.clone();
                let name_menu = tag.name.clone();
                let hash = tag.target.clone();
                let sidebar = cx.entity();
                h_flex()
                    .id(("tag", ix))
                    .w_full()
                    .px_2()
                    .h(px(26.))
                    .gap_2()
                    .rounded(px(RADIUS_CONTROL))
                    .hover(|s| s.bg(cx.theme().list_hover))
                    .child(icons::tag().small().text_color(cx.theme().muted_foreground))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(text_px(12.5))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(SharedString::from(tag.name.clone())),
                    )
                    .child(
                        div()
                            .font_family(mono_font())
                            .text_size(text_px(10.5))
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(tag.target.clone())),
                    )
                    .context_menu(move |menu, _, _| {
                        let checkout = name_menu.clone();
                        let copy_name = name_menu.clone();
                        let copy_hash = hash.clone();
                        let delete_name = name.clone();
                        let sidebar = sidebar.clone();
                        let sidebar_d = sidebar.clone();
                        menu.item(
                            PopupMenuItem::new("Checkout Tag...")
                                .icon(IconName::ArrowRight)
                                .on_click(move |_, window, cx| {
                                    let checkout = checkout.clone();
                                    sidebar.update(cx, |this, cx| {
                                        this.checkout_branch(BranchCheckout::Local(checkout), window, cx);
                                    });
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Copy Tag Name").icon(IconName::Copy).on_click(
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_name.clone(),
                                    ));
                                },
                            ),
                        )
                        .item(
                            PopupMenuItem::new("Copy Hash").icon(IconName::Copy).on_click(
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_hash.clone(),
                                    ));
                                },
                            ),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Delete").icon(icons::trash()).on_click(
                                move |_, window, cx| {
                                    let delete_name = delete_name.clone();
                                    sidebar_d.update(cx, |this, cx| {
                                        this.confirm_git_action(
                                            "Delete Tag?",
                                            "This local tag will be deleted. The remote tag will not change.",
                                            Some(SharedString::from(delete_name.clone())),
                                            "Delete Tag",
                                            "Deleting tag",
                                            move |repo| repo.delete_tag(&delete_name),
                                            window,
                                            cx,
                                        );
                                    });
                                },
                            ),
                        )
                    })
            }))
    }

    fn render_checkpoints(&mut self, cx: &mut Context<Self>) -> Div {
        let checkpoints = self.state.read(cx).checkpoints.clone();
        if checkpoints.is_empty() {
            return Self::empty_state("No checkpoints", cx);
        }
        v_flex()
            .w_full()
            .children(checkpoints.into_iter().enumerate().map(|(ix, checkpoint)| {
                let hash = checkpoint.hash.clone();
                let reference = checkpoint.reference.clone();
                h_flex()
                    .id(("checkpoint", ix))
                    .w_full()
                    .px_2()
                    .h(px(26.))
                    .gap_2()
                    .rounded(px(RADIUS_CONTROL))
                    .hover(|s| s.bg(cx.theme().list_hover))
                    .child(icons::clock().small().text_color(cx.theme().primary))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(text_px(12.5))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(SharedString::from(checkpoint.turn_label())),
                    )
                    .child(
                        div()
                            .font_family(mono_font())
                            .text_size(text_px(10.5))
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(checkpoint.hash.clone())),
                    )
                    .context_menu(move |menu, _, _| {
                        let reference = reference.clone();
                        let hash = hash.clone();
                        menu.item(
                            PopupMenuItem::new("Copy Checkpoint Ref")
                                .icon(IconName::Copy)
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        reference.clone(),
                                    ));
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Copy Hash")
                                .icon(IconName::Copy)
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(hash.clone()));
                                }),
                        )
                    })
            }))
    }

    fn render_lfs(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(info) = self.state.read(cx).lfs.clone() else {
            return Self::empty_state("Loading LFS...", cx);
        };
        if !info.available {
            return Self::empty_state("LFS unavailable", cx);
        }
        if info.patterns.is_empty() {
            return Self::empty_state("No LFS patterns", cx);
        }
        v_flex()
            .w_full()
            .child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .child(
                        icons::file_text()
                            .small()
                            .text_color(cx.theme().info),
                    )
                    .child(mono_label(format!("{} LFS objects", info.object_count), cx)),
            )
            .children(info.patterns.into_iter().enumerate().map(|(ix, pattern)| {
                let pattern_copy = pattern.clone();
                let pattern_untrack = pattern.clone();
                let sidebar = cx.entity();
                h_flex()
                    .id(("lfs-pattern", ix))
                    .w_full()
                    .px_2()
                    .h(px(26.))
                    .gap_2()
                    .rounded(px(RADIUS_CONTROL))
                    .hover(|s| s.bg(cx.theme().list_hover))
                    .child(
                        icons::file_text()
                            .small()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(text_px(12.5))
                            .font_family(mono_font())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(SharedString::from(pattern)),
                    )
                    .context_menu(move |menu, _, _| {
                        let pattern_copy = pattern_copy.clone();
                        let pattern_untrack = pattern_untrack.clone();
                        let sidebar = sidebar.clone();
                        menu.item(
                            PopupMenuItem::new("Copy Pattern").icon(IconName::Copy).on_click(
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        pattern_copy.clone(),
                                    ));
                                },
                            ),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Untrack").icon(icons::trash()).on_click(
                                move |_, window, cx| {
                                    let pattern_untrack = pattern_untrack.clone();
                                    sidebar.update(cx, |this, cx| {
                                        let state = this.state.clone();
                                        let hide = Self::hide_checkpoints(cx);
                                        let target = SharedString::from(pattern_untrack.clone());
                                        confirm_destructive_action(
                                            "Untrack LFS Pattern?",
                                            "Git LFS will stop tracking this pattern. Existing LFS objects will not be removed.",
                                            Some(target),
                                            "Untrack",
                                            move |cx| {
                                                state.update(cx, |state, cx| {
                                                    state.run_operation(
                                                        "Untracking LFS",
                                                        hide,
                                                        move |repo| {
                                                            repo.lfs_untrack(&pattern_untrack)
                                                        },
                                                        cx,
                                                    );
                                                });
                                                state.update(cx, |state, cx| {
                                                    state.refresh_lfs(cx)
                                                });
                                            },
                                            window,
                                            cx,
                                        );
                                    });
                                },
                            ),
                        )
                    })
            }))
    }

    fn render_stashes(&mut self, cx: &mut Context<Self>) -> Div {
        let stashes = self.state.read(cx).stashes.clone();
        if stashes.is_empty() {
            return Self::empty_state("No stashes", cx);
        }
        v_flex()
            .w_full()
            .children(stashes.into_iter().enumerate().map(|(ix, stash)| {
                let apply_ref = stash.reference.clone();
                let pop_ref = stash.reference.clone();
                let drop_ref = stash.reference.clone();
                let sidebar = cx.entity();
                h_flex()
                    .id(("stash", ix))
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .rounded(px(RADIUS_CONTROL))
                    .hover(|s| s.bg(cx.theme().list_hover))
                    .child(
                        icons::archive()
                            .small()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(text_px(12.5))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(SharedString::from(stash.message.clone())),
                            )
                            .child(
                                div()
                                    .text_size(text_px(11.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from(stash.relative_date.clone())),
                            ),
                    )
                    .context_menu(move |menu, _, _| {
                        let apply_ref = apply_ref.clone();
                        let pop_ref = pop_ref.clone();
                        let drop_ref = drop_ref.clone();
                        let sidebar_a = sidebar.clone();
                        let sidebar_p = sidebar.clone();
                        let sidebar_d = sidebar.clone();
                        menu.item(
                            PopupMenuItem::new("Apply").icon(IconName::Check).on_click(
                                move |_, _, cx| {
                                    let apply_ref = apply_ref.clone();
                                    sidebar_a.update(cx, |this, cx| {
                                        this.run_git(
                                            "Applying stash",
                                            move |repo| repo.stash_apply(&apply_ref),
                                            cx,
                                        );
                                    });
                                },
                            ),
                        )
                        .item(
                            PopupMenuItem::new("Pop").icon(icons::pull()).on_click(move |_, _, cx| {
                                let pop_ref = pop_ref.clone();
                                sidebar_p.update(cx, |this, cx| {
                                    this.run_git(
                                        "Popping stash",
                                        move |repo| repo.stash_pop(&pop_ref),
                                        cx,
                                    );
                                });
                            }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Drop").icon(icons::trash()).on_click(
                                move |_, window, cx| {
                                    let drop_ref = drop_ref.clone();
                                    sidebar_d.update(cx, |this, cx| {
                                        this.confirm_git_action(
                                            "Drop Stash?",
                                            "The stashed changes will be deleted. This action cannot be undone.",
                                            Some(SharedString::from(drop_ref.clone())),
                                            "Drop Stash",
                                            "Dropping stash",
                                            move |repo| repo.stash_drop(&drop_ref),
                                            window,
                                            cx,
                                        );
                                    });
                                },
                            ),
                        )
                    })
            }))
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.notice_branch_switch(window, cx);
        let content = match self.category {
            SidebarCategory::Branches => {
                let (active, _) = split_stale(&self.state.read(cx).branches);
                if active.is_empty() {
                    Self::empty_state("No branches", cx)
                } else {
                    self.render_branch_tree(&active, false, cx)
                }
            }
            SidebarCategory::Worktrees => self.render_worktrees(cx),
            SidebarCategory::Remotes => self.render_remotes(cx),
            SidebarCategory::Tags => self.render_tags(cx),
            SidebarCategory::Stashes => self.render_stashes(cx),
            SidebarCategory::Checkpoints => self.render_checkpoints(cx),
            SidebarCategory::Stale => {
                let (_, stale) = split_stale(&self.state.read(cx).branches);
                if stale.is_empty() {
                    Self::empty_state("No stale branches", cx)
                } else {
                    self.render_branch_tree(&stale, false, cx)
                }
            }
            SidebarCategory::Lfs => self.render_lfs(cx),
        };

        let count = self.category_count(cx);
        let drop_menu = self.drop_menu.as_ref().map(|menu| menu.render(window, cx));
        v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .children(drop_menu)
            .child(self.render_top_rail(cx))
            .child(div().px_3().pt_3().pb_1p5().child(section_header(
                self.category.title(),
                count,
                cx,
            )))
            .child(
                v_flex()
                    .id("sidebar-content")
                    .flex_1()
                    .min_h_0()
                    .px_1p5()
                    .pb_2()
                    .overflow_y_scrollbar()
                    .child(content),
            )
    }
}

fn ssh_to_https(url: &str) -> Option<String> {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("git@") {
        let rest = rest.replacen(':', "/", 1);
        return Some(format!("https://{rest}"));
    }
    if let Some(rest) = url.strip_prefix("ssh://git@") {
        return Some(format!("https://{rest}"));
    }
    None
}

fn https_to_ssh(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    Some(format!("git@{host}:{path}"))
}
