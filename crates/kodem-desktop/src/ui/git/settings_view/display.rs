//! The Display page: what the commit list shows. Theme, accent, text size, density and motion
//! follow Kodem's Appearance preferences.
use gpui::prelude::*;
use gpui::{Context, Div, div};
use kodem_git::settings::DateFormat;

use super::{GitSettingsView, page, section, segmented, setting_row, toggle_row};
use crate::theme::px;
use crate::ui::git::kit::ActiveTheme as _;
use crate::ui::git::settings::GitSettings;
use crate::ui::settings::note;

impl GitSettingsView {
    /// Returns the Display page.
    pub(super) fn render_display(&mut self, cx: &mut Context<Self>) -> Div {
        let s = GitSettings::get(cx).clone();

        page().child(
            section("Commit List", cx)
                .child(div().pt(px(8.)).child(note(
                    "The theme, accent, text size and motion of the Git workspace follow Appearance.",
                    cx.theme().kodem,
                )))
                .child(toggle_row(
                    "show-avatars",
                    "Show author avatars",
                    "Display Gravatar images next to commits",
                    s.show_avatars,
                    |v, cx| GitSettings::update(cx, |s| s.show_avatars = v),
                    cx,
                ))
                .child(toggle_row(
                    "show-badges",
                    "Show branch badges",
                    "Display branch and tag labels on commits",
                    s.show_branch_badges,
                    |v, cx| GitSettings::update(cx, |s| s.show_branch_badges = v),
                    cx,
                ))
                .child(toggle_row(
                    "show-stats",
                    "Show commit stats badges",
                    "Display per-commit additions and deletions in the commit list",
                    s.show_commit_stats_badges,
                    |v, cx| GitSettings::update(cx, |s| s.show_commit_stats_badges = v),
                    cx,
                ))
                .child(toggle_row(
                    "hide-checkpoints",
                    "Hide checkpoint commits",
                    "Filter automated checkpoint commits from history",
                    s.hide_checkpoint_commits,
                    |v, cx| GitSettings::update(cx, |s| s.hide_checkpoint_commits = v),
                    cx,
                ))
                .child(setting_row(
                    "Date format",
                    "How commit dates appear in the list",
                    segmented(
                        "date-format",
                        DateFormat::ALL.map(|format| (format, format.label())),
                        &s.date_format,
                        |format, _, cx| GitSettings::update(cx, |s| s.date_format = *format),
                        cx,
                    ),
                    cx,
                )),
        )
    }
}
