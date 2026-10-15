//! The Remote page: how pull, fetch, push and merge run, and how commits are signed.
use gpui::prelude::*;
use gpui::{Context, Div, div};
use kodem_git::settings::{PullStrategy, SigningMethod};

use super::{GitSettingsView, page, section, segmented, setting_row, toggle_row};
use crate::theme::px;
use crate::ui::git::components::text_field;
use crate::ui::git::settings::GitSettings;

impl GitSettingsView {
    /// Returns the Remote page.
    pub(super) fn render_remote(&mut self, cx: &mut Context<Self>) -> Div {
        let s = GitSettings::get(cx).clone();

        page()
            .child(
                section("Pull & Fetch", cx)
                    .child(setting_row(
                        "Default pull strategy",
                        "How pull integrates remote commits",
                        segmented(
                            "pull-strategy",
                            PullStrategy::ALL.map(|strategy| (strategy, strategy.label())),
                            &s.pull_strategy,
                            |strategy, _, cx| {
                                GitSettings::update(cx, |s| s.pull_strategy = *strategy)
                            },
                            cx,
                        ),
                        cx,
                    ))
                    .child(toggle_row(
                        "prune-on-fetch",
                        "Prune on fetch",
                        "Remove deleted remote branches",
                        s.prune_on_fetch,
                        |v, cx| GitSettings::update(cx, |s| s.prune_on_fetch = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "fetch-all",
                        "Fetch all remotes",
                        "Include every remote in fetch",
                        s.fetch_all_remotes,
                        |v, cx| GitSettings::update(cx, |s| s.fetch_all_remotes = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "autostash",
                        "Autostash before rebase",
                        "Stash local changes before a rebase starts",
                        s.autostash_before_rebase,
                        |v, cx| GitSettings::update(cx, |s| s.autostash_before_rebase = v),
                        cx,
                    )),
            )
            .child(
                section("Push", cx)
                    .child(toggle_row(
                        "set-upstream",
                        "Set upstream on push",
                        "Add --set-upstream for new branches",
                        s.set_upstream_on_push,
                        |v, cx| GitSettings::update(cx, |s| s.set_upstream_on_push = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "push-tags",
                        "Push tags by default",
                        "Include tags with every push",
                        s.push_tags,
                        |v, cx| GitSettings::update(cx, |s| s.push_tags = v),
                        cx,
                    )),
            )
            .child(section("Merge", cx).child(toggle_row(
                "no-ff",
                "No fast-forward merge",
                "Always create a merge commit",
                s.merge_no_ff,
                |v, cx| GitSettings::update(cx, |s| s.merge_no_ff = v),
                cx,
            )))
            .child(
                section("Commit Signing", cx)
                    .child(setting_row(
                        "Signing method",
                        "Sign commits with GPG or SSH",
                        segmented(
                            "signing",
                            SigningMethod::ALL.map(|method| (method, method.label())),
                            &s.signing_method,
                            |method, _, cx| GitSettings::update(cx, |s| s.signing_method = *method),
                            cx,
                        ),
                        cx,
                    ))
                    .when(s.signing_method == SigningMethod::Gpg, |this| {
                        this.child(setting_row(
                            "GPG Key ID",
                            "The key used to sign commits",
                            div().w(px(260.)).child(text_field(&self.gpg_key)),
                            cx,
                        ))
                    }),
            )
    }
}
