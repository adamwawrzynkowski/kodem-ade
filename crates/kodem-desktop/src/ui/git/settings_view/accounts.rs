//! The Accounts page: the linked GitHub accounts, the form that links another one, and how SSH
//! keys work.
use gpui::prelude::*;
use gpui::{Context, Div, div};

use super::{GitSettingsView, page, section};
use crate::theme::px;
use crate::ui::git::account_link::linked_account_rows;
use crate::ui::git::components::dither_empty_hint;
use crate::ui::git::kit::ActiveTheme as _;
use crate::ui::git::settings::GitSettings;
use crate::ui::settings::note;

impl GitSettingsView {
    /// Returns the Accounts page.
    pub(super) fn render_accounts(&mut self, cx: &mut Context<Self>) -> Div {
        let has_accounts = !GitSettings::get(cx).accounts.is_empty();
        let t = cx.theme().kodem;

        page()
            .child(
                section("Accounts", cx).child(
                    div()
                        .pt(px(10.))
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .when(!has_accounts, |this| {
                            this.child(dither_empty_hint(
                                "No accounts. Link one below to enable GitHub features.",
                                cx,
                            ))
                        })
                        .children(linked_account_rows(cx)),
                ),
            )
            .child(
                section("Link GitHub Account", cx)
                    .child(div().pt(px(8.)).pb(px(10.)).child(note(
                        "Linked accounts list your repositories for cloning, load pull requests, \
                         and sign in HTTPS fetch, pull, and push when the keychain has no login.",
                        t,
                    )))
                    .child(self.account_link.clone()),
            )
            .child(section("SSH Keys", cx).child(div().pt(px(8.)).child(note(
                "SSH keys from ~/.ssh are used by git directly. Manage them with ssh-add in a \
                 terminal.",
                t,
            ))))
    }
}
