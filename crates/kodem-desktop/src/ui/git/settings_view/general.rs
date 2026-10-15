//! The General page: auto-fetch, repository defaults, confirmations and the welcome tour.
use gpui::prelude::*;
use gpui::{Context, Div};

use super::{GitSettingsView, page, section, segmented, setting_row, toggle_row};
use crate::ui::git::components::button;
use crate::ui::git::icons;
use crate::ui::git::kit::Sizable as _;
use crate::ui::git::settings::GitSettings;

impl GitSettingsView {
    /// Returns the General page.
    pub(super) fn render_general(&mut self, cx: &mut Context<Self>) -> Div {
        let s = GitSettings::get(cx).clone();
        let default_branch = match s.default_branch_name.as_str() {
            "master" => "master",
            "develop" => "develop",
            _ => "main",
        };

        page()
            .child(section("Fetch & Sync", cx).child(setting_row(
                "Auto-fetch interval",
                "Fetch remotes on a timer",
                segmented(
                    "auto-fetch",
                    [
                        (0u32, "Off"),
                        (1, "1 min"),
                        (5, "5 min"),
                        (10, "10 min"),
                        (30, "30 min"),
                    ],
                    &s.auto_fetch_interval,
                    |value, _, cx| GitSettings::update(cx, |s| s.auto_fetch_interval = *value),
                    cx,
                ),
                cx,
            )))
            .child(section("Repository Defaults", cx).child(setting_row(
                "Default branch name",
                "Used when a new repository is created",
                segmented(
                    "default-branch",
                    ["main", "master", "develop"].map(|name| (name, name)),
                    &default_branch,
                    |name, _, cx| {
                        GitSettings::update(cx, |s| s.default_branch_name = (*name).into())
                    },
                    cx,
                ),
                cx,
            )))
            .child(
                section("Behavior", cx)
                    .child(toggle_row(
                        "restore-tabs",
                        "Restore tabs on launch",
                        "Reopen the repositories from the last session",
                        s.restore_tabs_on_launch,
                        |v, cx| GitSettings::update(cx, |s| s.restore_tabs_on_launch = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "confirm-destructive",
                        "Confirm destructive actions",
                        "Ask before discard, reset, and force push",
                        s.confirm_destructive_actions,
                        |v, cx| GitSettings::update(cx, |s| s.confirm_destructive_actions = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "show-notifications",
                        "Desktop notifications",
                        "Notify when long operations complete",
                        s.show_notifications,
                        |v, cx| GitSettings::update(cx, |s| s.show_notifications = v),
                        cx,
                    )),
            )
            .child(
                section("Onboarding", cx).child(setting_row(
                    "Reset onboarding",
                    "Show the welcome tour of the Git workspace again",
                    button("reset-onboarding")
                        .small()
                        .outline()
                        .icon(icons::refresh())
                        .label(if s.onboarding_completed {
                            "Reset"
                        } else {
                            "Showing"
                        })
                        .disabled_readable(!s.onboarding_completed, cx)
                        .on_click(|_, _, cx| {
                            GitSettings::update(cx, |s| s.onboarding_completed = false);
                        }),
                    cx,
                )),
            )
    }
}
