//! The content of the Fetch, Pull and Push popovers of the Git toolbar: the options of each
//! remote action, the button that runs it, and the confirmation before a force push.
use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{App, ClickEvent, Entity, Hsla, SharedString, Window, div};
use kodem_git::settings::{PullStrategy, Settings};

use crate::theme::{RADIUS_CONTROL, RADIUS_SMALL, px, text_px};
use crate::ui::dither::{Dither, DitherField, DottedRule, Edge};
use crate::ui::git::components::{button, dither_icon_tile, mono_font, mono_label};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, Disableable as _, Icon, IconName, LiquidButton, Sizable as _, h_flex, v_flex,
};
use crate::ui::git::theme;
use crate::ui::git::workspace::GitWorkspace;

/// The remote action that a popover configures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemotePopupKind {
    Fetch,
    Pull,
    Push,
}

/// How a push treats a remote branch whose history differs from the local one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushForceMode {
    Normal,
    WithLease,
    Force,
}

/// One option that the user can turn on or off in a remote popover.
#[derive(Debug, Clone, Copy)]
pub enum RemoteOption {
    FetchAllRemotes,
    FetchPrune,
    FetchSubmodules,
    PullRebase,
    PullFastForwardOnly,
    PullNoCommit,
    PullLfs,
    PullAutoStash,
    PullSubmodules,
    PushSetUpstream,
    PushTags,
    PushSubmodules,
    PushForceWithLease,
    PushForce,
}

/// What a remote popover shows.
#[derive(Clone)]
pub struct RemotePopupData {
    /// The action that the popover configures.
    pub kind: RemotePopupKind,
    /// The current branch, shown under the title.
    pub branch: String,
    /// A Git operation runs, so the actions are disabled.
    pub busy: bool,
    /// The settings that hold the options of the popover.
    pub settings: Settings,
    /// The force mode that the next push uses.
    pub force_mode: PushForceMode,
    /// Shows the force push confirmation in place of the options.
    pub show_force_confirmation: bool,
}

#[derive(Clone, Copy)]
struct OptionTone {
    text: Hsla,
    hover: Hsla,
}

#[derive(Clone, Copy)]
struct PopupHoverColors {
    standard: Hsla,
    warning: Hsla,
    danger: Hsla,
}

/// Returns the content of a remote popover: its options and the button that runs the action,
/// or the force push confirmation.
pub fn remote_popup_content(
    data: RemotePopupData,
    workspace: Entity<GitWorkspace>,
    cx: &App,
) -> gpui::Div {
    if data.show_force_confirmation {
        return force_confirmation(data.branch, data.busy, workspace, cx);
    }

    let colors = theme::liquid_colors(cx);
    let hover_colors = popup_hover_colors(cx);

    let mut content = v_flex()
        .w(px(296.))
        .child(popup_header(data.kind, &data.branch, cx))
        .child(divider(cx));

    content = match data.kind {
        RemotePopupKind::Fetch => content
            .child(option_row(
                "fetch-all-remotes",
                "All remotes",
                "Fetch from every configured remote",
                data.settings.fetch_all_remotes,
                None,
                RemoteOption::FetchAllRemotes,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "fetch-prune",
                "Prune",
                "Clean stale remote refs and tracking branches",
                data.settings.prune_on_fetch,
                None,
                RemoteOption::FetchPrune,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "fetch-submodules",
                "Fetch submodules",
                "Fetch commits needed by nested repositories",
                data.settings.fetch_submodules,
                None,
                RemoteOption::FetchSubmodules,
                workspace.clone(),
                cx,
            )),
        RemotePopupKind::Pull => content
            .child(option_row(
                "pull-rebase",
                "Rebase",
                "Replay local commits on top of upstream",
                data.settings.pull_strategy == PullStrategy::Rebase,
                None,
                RemoteOption::PullRebase,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "pull-ff-only",
                "Fast-forward only",
                "Fail if a merge commit would be created",
                data.settings.pull_strategy == PullStrategy::FastForward,
                None,
                RemoteOption::PullFastForwardOnly,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "pull-no-commit",
                "No auto-commit",
                "Merge without an automatic commit",
                data.settings.pull_no_commit,
                None,
                RemoteOption::PullNoCommit,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "pull-lfs",
                "Pull LFS",
                "Fetch Large File Storage objects after pull",
                data.settings.pull_lfs,
                None,
                RemoteOption::PullLfs,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "pull-auto-stash",
                "Auto-stash local changes",
                "Stash and re-apply local changes",
                data.settings.autostash_before_rebase,
                None,
                RemoteOption::PullAutoStash,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "pull-submodules",
                "Update submodules",
                "Initialize and update nested repositories",
                data.settings.pull_submodules,
                None,
                RemoteOption::PullSubmodules,
                workspace.clone(),
                cx,
            )),
        RemotePopupKind::Push => content
            .child(section_label("Options", cx))
            .child(option_row(
                "push-set-upstream",
                "Set upstream",
                "Track this branch on the push remote",
                data.settings.set_upstream_on_push,
                None,
                RemoteOption::PushSetUpstream,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "push-tags",
                "Include tags",
                "Push all local tags to the remote",
                data.settings.push_tags,
                None,
                RemoteOption::PushTags,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "push-submodules",
                "Push submodules",
                "Push commits needed by nested repositories",
                data.settings.push_submodules,
                None,
                RemoteOption::PushSubmodules,
                workspace.clone(),
                cx,
            ))
            .child(divider(cx))
            .child(section_label("Dangerous", cx))
            .child(option_row(
                "push-force-with-lease",
                "Force with lease",
                "Only if the remote has not changed",
                data.force_mode == PushForceMode::WithLease,
                Some(OptionTone {
                    text: colors.warning,
                    hover: hover_colors.warning,
                }),
                RemoteOption::PushForceWithLease,
                workspace.clone(),
                cx,
            ))
            .child(option_row(
                "push-force",
                "Force push",
                "Overwrite remote history",
                data.force_mode == PushForceMode::Force,
                Some(OptionTone {
                    text: colors.danger,
                    hover: hover_colors.danger,
                }),
                RemoteOption::PushForce,
                workspace.clone(),
                cx,
            )),
    };

    content.child(divider(cx)).child(
        popup_footer()
            .child(
                run_action_row(data.kind, data.force_mode, data.busy, workspace.clone(), cx)
                    .flex_1(),
            )
            .child(cancel_button("cancel-remote-popup", workspace)),
    )
}

fn popup_header(kind: RemotePopupKind, branch: &str, cx: &App) -> gpui::Div {
    let colors = theme::liquid_colors(cx);
    let (icon, title, color) = match kind {
        RemotePopupKind::Fetch => (icons::refresh(), "Fetch from Remote", cx.theme().primary),
        RemotePopupKind::Pull => (icons::pull(), "Pull from Remote", cx.theme().primary),
        RemotePopupKind::Push => (icons::push(), "Push to Remote", colors.success),
    };
    div()
        .relative()
        .w_full()
        .overflow_hidden()
        .rounded_t(px(RADIUS_CONTROL))
        .child(
            Dither::new(DitherField::Fade {
                from: Edge::Right,
                start: 0.8,
                end: 0.0,
            })
            .color(color.opacity(0.35))
            .grid(3.0, 2.0)
            .layer(),
        )
        .child(
            h_flex()
                .relative()
                .w_full()
                .gap_2p5()
                .px_3()
                .py_2()
                .items_center()
                .child(dither_icon_tile(icon, color, 26., false))
                .child(
                    v_flex()
                        .min_w_0()
                        .line_height(gpui::relative(1.25))
                        .child(
                            div()
                                .text_size(text_px(12.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .font_family(mono_font())
                                .text_size(text_px(11.))
                                .text_color(colors.text_tertiary)
                                .child(SharedString::from(branch.to_string())),
                        ),
                ),
        )
}

#[allow(clippy::too_many_arguments)]
fn option_row(
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    checked: bool,
    tone: Option<OptionTone>,
    option: RemoteOption,
    workspace: Entity<GitWorkspace>,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let standard_hover = popup_hover_colors(cx).standard;
    let dangerous = tone.is_some();
    let accent = tone.map_or(cx.theme().primary, |tone| tone.text);

    h_flex()
        .id(id)
        .w_full()
        .gap_2()
        .px_3()
        .py_1()
        .items_center()
        .rounded(px(RADIUS_CONTROL))
        .cursor_pointer()
        .when_some(tone, |row, tone| {
            row.hover(move |style| style.bg(tone.hover))
        })
        .when(!dangerous, |row| {
            row.hover(move |style| style.bg(standard_hover))
        })
        .active(|style| style.bg(cx.theme().button_active))
        .on_click(move |_, _, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.set_remote_option(option, !checked, cx)
            });
        })
        .child(
            div()
                .size(px(14.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(RADIUS_SMALL))
                .border_1()
                .border_color(if checked {
                    accent
                } else {
                    cx.theme().muted_foreground.opacity(0.4)
                })
                .when(checked, |box_| box_.bg(accent))
                .when(checked, |box_| {
                    box_.child(
                        Icon::new(IconName::Check)
                            .size(px(10.))
                            .text_color(cx.theme().primary_foreground),
                    )
                }),
        )
        .child(
            v_flex()
                .min_w_0()
                .line_height(gpui::relative(1.25))
                .child(
                    div()
                        .text_size(text_px(12.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .when_some(tone, |text, tone| text.text_color(tone.text))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(text_px(11.5))
                        .text_color(cx.theme().muted_foreground)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(subtitle),
                ),
        )
}

fn run_action_row(
    kind: RemotePopupKind,
    force_mode: PushForceMode,
    busy: bool,
    workspace: Entity<GitWorkspace>,
    cx: &App,
) -> LiquidButton {
    let colors = theme::liquid_colors(cx);
    let (label, icon, color) = match (kind, force_mode) {
        (RemotePopupKind::Fetch, _) => ("Fetch", icons::refresh(), cx.theme().primary),
        (RemotePopupKind::Pull, _) => ("Pull", icons::pull(), cx.theme().primary),
        (RemotePopupKind::Push, PushForceMode::Normal) => ("Push", icons::push(), colors.success),
        (RemotePopupKind::Push, PushForceMode::WithLease) => {
            ("Force With Lease", icons::push(), colors.warning)
        }
        (RemotePopupKind::Push, PushForceMode::Force) => {
            ("Force Push", icons::push(), colors.danger)
        }
    };
    action_row(
        "run-remote-action",
        icon,
        label,
        color,
        busy,
        move |_, _, cx| {
            workspace.update(cx, |workspace, cx| workspace.confirm_remote_action(cx));
        },
    )
}

fn action_row(
    id: &'static str,
    icon: Icon,
    label: &'static str,
    color: Hsla,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> LiquidButton {
    button(id)
        .tone(color)
        .small()
        .icon(icon)
        .label(label)
        .disabled(disabled)
        .on_click(on_click)
}

fn force_confirmation(
    branch: String,
    busy: bool,
    workspace: Entity<GitWorkspace>,
    cx: &App,
) -> gpui::Div {
    let colors = theme::liquid_colors(cx);
    let lease_workspace = workspace.clone();
    let force_workspace = workspace.clone();
    v_flex()
        .w(px(296.))
        .child(
            h_flex()
                .gap_2()
                .px_3()
                .py_1p5()
                .child(
                    Icon::new(IconName::TriangleAlert)
                        .small()
                        .text_color(colors.danger),
                )
                .child(
                    v_flex()
                        .line_height(gpui::relative(1.25))
                        .child(
                            div()
                                .text_size(text_px(12.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Force push?"),
                        )
                        .child(
                            div()
                                .text_size(text_px(11.5))
                                .font_family(mono_font())
                                .text_color(cx.theme().muted_foreground)
                                .child(branch),
                        ),
                ),
        )
        .child(divider(cx))
        .child(
            div()
                .px_3()
                .py_1()
                .text_size(text_px(11.5))
                .text_color(cx.theme().muted_foreground)
                .child("Force With Lease is safer. It stops if the remote changed."),
        )
        .child(divider(cx))
        .child(
            popup_footer()
                .child(
                    action_row(
                        "confirm-force-with-lease",
                        icons::push(),
                        "With Lease",
                        colors.warning,
                        busy,
                        move |_, _, cx| {
                            lease_workspace.update(cx, |workspace, cx| {
                                workspace.run_confirmed_push(PushForceMode::WithLease, cx)
                            });
                        },
                    )
                    .flex_1()
                    .tooltip("Force push only if the remote did not change"),
                )
                .child(
                    action_row(
                        "confirm-force-push",
                        icons::push(),
                        "Force Push",
                        colors.danger,
                        busy,
                        move |_, _, cx| {
                            force_workspace.update(cx, |workspace, cx| {
                                workspace.run_confirmed_push(PushForceMode::Force, cx)
                            });
                        },
                    )
                    .flex_1()
                    .tooltip("Overwrite the remote branch"),
                )
                .child(cancel_button("cancel-force-push", workspace)),
        )
}

/// The row at the bottom of a remote popup: the actions side by side with
/// room around them.
fn popup_footer() -> gpui::Div {
    h_flex().w_full().gap_2().px_3().pt_1p5().pb_3()
}

/// A square button that closes the remote popup.
fn cancel_button(id: &'static str, workspace: Entity<GitWorkspace>) -> LiquidButton {
    button(id)
        .small()
        .icon(IconName::Close)
        .tooltip("Cancel")
        .on_click(move |_, _, cx| {
            workspace.update(cx, |workspace, cx| workspace.close_remote_popup(cx));
        })
}

fn popup_hover_colors(cx: &App) -> PopupHoverColors {
    let colors = theme::liquid_colors(cx);
    let surface = cx.theme().popover;
    let (standard_alpha, semantic_alpha) = if cx.theme().is_dark() {
        (0.12, 0.22)
    } else {
        (0.08, 0.14)
    };

    PopupHoverColors {
        standard: surface.blend(&with_alpha(cx.theme().foreground, standard_alpha)),
        warning: surface.blend(&with_alpha(colors.warning, semantic_alpha)),
        danger: surface.blend(&with_alpha(colors.danger, semantic_alpha)),
    }
}

// Returns the color with its alpha replaced.
fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    let mut color = color;
    color.alpha = alpha;
    color
}

fn section_label(label: &'static str, cx: &App) -> gpui::Div {
    div().px_3().pt_1p5().pb_0p5().child(mono_label(label, cx))
}

fn divider(cx: &App) -> gpui::Div {
    div().w_full().my_1().child(DottedRule::horizontal(
        theme::liquid_colors(cx).text_tertiary.opacity(0.4),
    ))
}
