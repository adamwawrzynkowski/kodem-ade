//! Links a GitHub account with a personal access token. The same form
//! is used by onboarding, the clone dialog, and the Accounts settings,
//! so an account linked anywhere works everywhere: pull requests,
//! repository lists, and HTTPS git commands.

use gpui::prelude::*;
use gpui::{
    App, Context, Div, Entity, EventEmitter, IntoElement, Render, SharedString, Subscription,
    Window, div,
};
use kodem_git::github::{self, AccountUser};
use kodem_git::settings::Account;

use crate::theme::{px, text_px};
use crate::ui::controls::Switch;
use crate::ui::git::components::{author_avatar, button, dither_loader, mono_font, text_field};
use crate::ui::git::contextual_actions::confirm_destructive_action;
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, IconName, InputEvent, InputState, Sizable as _, h_flex,
    v_flex,
};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::settings_view::{list_card, status_note};
use crate::ui::git::theme::liquid_colors;

const DEFAULT_HOST: &str = "github.com";

/// Sent when the form links an account.
pub enum AccountLinkEvent {
    /// The account was verified and saved.
    Linked(Account),
}

/// Where the form is in linking an account, with the message it shows.
enum LinkStatus {
    Idle,
    Verifying(SharedString),
    Failed(SharedString),
    Linked(SharedString),
}

/// A token field with a Link button, shortcuts to create a token or to
/// reuse the GitHub CLI login, and an optional Enterprise host.
pub struct AccountLinkForm {
    token: Entity<InputState>,
    host: Entity<InputState>,
    enterprise: bool,
    status: LinkStatus,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<AccountLinkEvent> for AccountLinkForm {}

impl AccountLinkForm {
    /// Creates an empty form for github.com.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let token = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Paste a personal access token")
                .masked(true)
        });
        let host = cx.new(|cx| InputState::new(window, cx).placeholder("github.example.com"));
        let subscriptions =
            vec![
                cx.subscribe_in(&token, window, |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter) {
                        this.link(window, cx);
                    }
                }),
            ];
        Self {
            token,
            host,
            enterprise: false,
            status: LinkStatus::Idle,
            _subscriptions: subscriptions,
        }
    }

    /// The host that the form links to, without a scheme.
    fn host_name(&self, cx: &App) -> String {
        if !self.enterprise {
            return DEFAULT_HOST.to_string();
        }
        let host = normalized_host(&self.host.read(cx).value());
        if host.is_empty() {
            DEFAULT_HOST.to_string()
        } else {
            host
        }
    }

    /// Whether a token check is running.
    fn is_verifying(&self) -> bool {
        matches!(self.status, LinkStatus::Verifying(_))
    }

    /// Checks the pasted token and links its account.
    fn link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_verifying() {
            return;
        }
        let token = self.token.read(cx).value().trim().to_string();
        if token.is_empty() {
            self.status = LinkStatus::Failed("Paste a token first.".into());
            cx.notify();
            return;
        }
        let host = self.host_name(cx);
        self.status = LinkStatus::Verifying(format!("Checking the token with {host}").into());
        cx.notify();
        let verify_host = host.clone();
        let task = cx.background_executor().spawn(async move {
            github::account_user(&verify_host, &token).map(|user| (user, token))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                this.finish_link(host, result, window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Links the account that the GitHub CLI is signed in to.
    fn link_with_cli(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_verifying() {
            return;
        }
        let host = self.host_name(cx);
        self.status = LinkStatus::Verifying("Reading the GitHub CLI login".into());
        cx.notify();
        let verify_host = host.clone();
        let task = cx.background_executor().spawn(async move {
            let Some(token) = github::github_cli_token(&verify_host) else {
                anyhow::bail!(
                    "The GitHub CLI is not signed in to {verify_host}. Run gh auth login first."
                );
            };
            github::account_user(&verify_host, &token).map(|user| (user, token))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                this.finish_link(host, result, window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Saves the account a token check found, or shows why the check failed.
    fn finish_link(
        &mut self,
        host: String,
        result: anyhow::Result<(AccountUser, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((user, token)) => {
                let account = Account {
                    host,
                    username: user.login,
                    email: user.email.unwrap_or_default(),
                    token,
                };
                self.status = LinkStatus::Linked(
                    format!("Linked {} on {}", account.username, account.host).into(),
                );
                self.token
                    .update(cx, |input, cx| input.set_value("", window, cx));
                save_account(account.clone(), cx);
                cx.emit(AccountLinkEvent::Linked(account));
            }
            Err(error) => {
                self.status = LinkStatus::Failed(error.to_string().into());
            }
        }
        cx.notify();
    }
}

/// Adds the account, or replaces the saved one for the same user on the
/// same host so a new token takes over.
fn save_account(account: Account, cx: &mut App) {
    GitSettings::update(cx, |settings| {
        settings.accounts.retain(|saved| {
            !(saved.host.eq_ignore_ascii_case(&account.host)
                && saved.username.eq_ignore_ascii_case(&account.username))
        });
        settings.accounts.push(account);
    });
}

/// Removes a scheme, a path, and spaces from a host the user typed.
fn normalized_host(value: &str) -> String {
    let value = value.trim();
    let value = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value);
    value
        .split('/')
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

impl Render for AccountLinkForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = liquid_colors(cx);
        let verifying = self.is_verifying();
        let host = self.host_name(cx);

        v_flex()
            .w_full()
            .gap_2()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(text_field(&self.token)))
                    .child(
                        button("account-link")
                            .primary()
                            .label("Link")
                            .icon(IconName::Github)
                            .loading(verifying)
                            .on_click(cx.listener(|this, _, window, cx| this.link(window, cx))),
                    ),
            )
            .when(self.enterprise, |form| form.child(text_field(&self.host)))
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .items_center()
                    .child(
                        button("account-create-token")
                            .ghost()
                            .xsmall()
                            .icon(IconName::ExternalLink)
                            .label("Create token")
                            .tooltip("Opens the token page with the scopes Kodem needs")
                            .on_click(move |_, _, cx| cx.open_url(&github::new_token_url(&host))),
                    )
                    .child(
                        button("account-use-cli")
                            .ghost()
                            .xsmall()
                            .icon(IconName::SquareTerminal)
                            .label("Use GitHub CLI")
                            .disabled_readable(verifying, cx)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.link_with_cli(window, cx);
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        // Clicking the label toggles the switch too.
                        h_flex()
                            .id("account-enterprise-toggle")
                            .flex_shrink_0()
                            .gap_2()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.enterprise = !this.enterprise;
                                cx.notify();
                            }))
                            .child(
                                Switch::new("account-enterprise", self.enterprise)
                                    .label("Enterprise")
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.enterprise = *checked;
                                        cx.notify();
                                    })),
                            )
                            .child(div().text_size(text_px(12.5)).child("Enterprise")),
                    ),
            )
            .child(match &self.status {
                LinkStatus::Idle => div()
                    .font_family(mono_font())
                    .text_size(text_px(11.))
                    .text_color(colors.text_tertiary)
                    .child("Scopes: repo, read:user, user:email. The token stays on this computer.")
                    .into_any_element(),
                LinkStatus::Verifying(message) => {
                    dither_loader(message.clone(), cx).px_0().into_any_element()
                }
                LinkStatus::Failed(message) => {
                    status_note(message.clone(), colors.danger).into_any_element()
                }
                LinkStatus::Linked(message) => {
                    status_note(message.clone(), colors.success).into_any_element()
                }
            })
    }
}

/// One card per linked account with its avatar, host, and a remove
/// button. Returns no cards when no account is linked.
pub fn linked_account_rows(cx: &App) -> Vec<Div> {
    let colors = liquid_colors(cx);
    GitSettings::get(cx)
        .accounts
        .iter()
        .enumerate()
        .map(|(ix, account)| {
            let username = account.username.clone();
            let host = account.host.clone();
            let has_token = !account.token.is_empty();
            list_card(false, cx)
                .child(author_avatar(&account.username, &account.email, 28.0, cx))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(text_px(12.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(account.username.clone()),
                        )
                        .child(
                            div()
                                .font_family(mono_font())
                                .text_size(text_px(11.))
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "{} · {}",
                                    account.host,
                                    if has_token { "linked" } else { "no token" }
                                )),
                        ),
                )
                .child(div().size(px(6.)).rounded_full().bg(if has_token {
                    colors.success
                } else {
                    colors.warning
                }))
                .child(
                    button(("remove-account", ix))
                        .ghost()
                        .xsmall()
                        .icon(icons::trash())
                        .tooltip("Remove account")
                        .on_click(move |_, window, cx| {
                            let username = username.clone();
                            let host = host.clone();
                            confirm_destructive_action(
                                "Remove Account?",
                                "The account and its token will be removed from Kodem.",
                                Some(format!("{username} at {host}").into()),
                                "Remove Account",
                                move |cx| remove_account(&host, &username, cx),
                                window,
                                cx,
                            );
                        }),
                )
        })
        .collect()
}

/// Removes the saved account of `username` on `host`.
fn remove_account(host: &str, username: &str, cx: &mut App) {
    GitSettings::update(cx, |settings| {
        settings
            .accounts
            .retain(|account| !(account.host == host && account.username == username));
    });
}
