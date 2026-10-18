//! Accounts preferences: every provider's signed-in identities with plan, limit windows and the
//! provider CLI's own sign-in.
//!
//! An added account is a separate configuration directory of the provider CLI. The desktop never
//! sees credentials; it shows what the CLI reports and asks the host to start the CLI's sign-in.
use super::*;

#[derive(Default)]
pub(super) struct AccountsView {
    /// The provider whose accounts are shown.
    pub provider: Provider,
    adding: bool,
    renaming: Option<String>,
    removing: Option<String>,
}

impl Shell {
    // Returns the name typed into the shared field, when it can name an account.
    fn account_name_input(&self, cx: &App) -> Option<String> {
        let name = self.aux.read(cx).text().trim().to_owned();
        (!name.is_empty() && name.chars().count() < 80).then_some(name)
    }

    fn begin_account_name(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.aux.update(cx, |input, cx| input.set_text(text, cx));
        window.focus(&self.aux.read(cx).focus_handle(cx), cx);
    }

    /// Starts the provider's sign-in for an account that was just added.
    pub(super) fn account_added(&mut self, account: &Account, cx: &mut Context<Self>) {
        self.accounts.adding = false;
        self.accounts.provider = account.provider;
        self.send(core::Request::SignInAccount {
            provider: account.provider,
            id: account.id.clone(),
        });
        let tone = self.theme.accent;
        self.notify_user(
            format!("{} added. Finish sign-in in Terminal.", account.name),
            tone,
            cx,
        );
        self.send(core::Request::Hello);
    }

    // What the provider's terms ask of someone who keeps several accounts. Shown where accounts
    // are added, because an account suspension is the cost of getting this wrong.
    fn account_terms(&self, provider: Provider) -> AnyElement {
        let t = self.theme;
        let company = match provider {
            Provider::Claude => "Anthropic",
            Provider::Codex => "OpenAI",
            Provider::OpenCode => "A provider that you connect to OpenCode",
        };
        kit::block(t)
            .child(kit::block_header("Provider terms", t.warning, t))
            .child(
                div()
                    .px(px(12.))
                    .py(px(9.))
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .text_size(text_px(11.5))
                    .text_color(t.muted)
                    .child(
                        "Add only accounts that are your own, for example a personal and a work account. Do not share an account with anyone.",
                    )
                    .child(format!(
                        "Do not use another account to continue after a usage limit. {company} can suspend accounts that get around its limits, so a started task stays on its account until the limit resets.",
                    ))
                    .child(format!(
                        "Sign-in runs in {}'s own CLI. Kodem never reads, stores or passes on its credentials.",
                        provider.label()
                    )),
            )
            .into_any_element()
    }

    /// Builds the accounts section: a provider rail, one card per account, and the add form.
    pub(super) fn account_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let provider = self.accounts.provider;
        let mut rail = div()
            .h(px(34.))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(t.border);
        for (index, candidate) in Provider::ALL.into_iter().enumerate() {
            let chosen = candidate == provider;
            let count = self.workspace.as_ref().map_or(0, |workspace| {
                workspace
                    .accounts
                    .iter()
                    .filter(|state| state.account.provider == candidate)
                    .count()
            });
            rail = rail.child(
                div()
                    .id(("account-provider", index))
                    .relative()
                    .h_full()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .cursor_pointer()
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.5))
                    .text_color(if chosen { t.text } else { t.muted })
                    .when(!chosen, |cell| cell.hover(move |cell| cell.bg(t.selected)))
                    .child(icon(candidate.icon()).size(px(13.)))
                    .child(candidate.label())
                    .when(candidate.beta(), |cell| cell.child(kit::beta_chip(t)))
                    .child(figure(count.to_string(), t.subtle))
                    .when(chosen, |cell| cell.child(kit::tab_indicator(t.accent)))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.accounts = AccountsView {
                            provider: candidate,
                            ..Default::default()
                        };
                        cx.notify();
                    })),
            );
        }
        let accounts: Vec<AccountState> = self
            .workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .accounts
                    .iter()
                    .filter(|state| state.account.provider == provider)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(self.section_title(
                "Accounts",
                "Stay signed in to several accounts of one provider and choose one per task. Each account is its own configuration directory of the provider's CLI, which keeps the sign-in. Kodem never reads or stores credentials.",
            ))
            .child(rail)
            .child(self.account_terms(provider));
        for state in &accounts {
            body = body.child(self.account_card(state, cx));
        }
        if accounts.is_empty() {
            body = body.child(kit::loader_row("Reading accounts from the local host", t));
        }
        if provider.home_variable().is_none() {
            return body
                .child(
                    div()
                        .text_size(text_px(12.))
                        .text_color(t.muted)
                        .child(format!(
                            "{} has one account, which its own CLI manages.",
                            provider.label()
                        )),
                )
                .into_any_element();
        }
        if self.accounts.adding {
            let ready = self.account_name_input(cx).is_some();
            body = body.child(
                kit::card(t)
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(kit::section_header("New account", None, t).relative())
                    .child(
                        settings::input_frame(t)
                            .child(div().flex_1().min_w_0().child(self.aux.clone())),
                    )
                    .child(
                        div()
                            .relative()
                            .text_size(text_px(11.5))
                            .text_color(t.muted)
                            .child(format!(
                                "Name it after how you use it, for example Work or Personal. Terminal then opens `{}` for this account.",
                                provider.login_command()
                            )),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .gap(px(6.))
                            .child(
                                self.primary("account-add-confirm", "log-in", "Add and sign in")
                                    .disabled(!ready)
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        if let Some(name) = s.account_name_input(cx) {
                                            s.send(core::Request::AddAccount { provider, name });
                                        }
                                    })),
                            )
                            .child(
                                self.ghost("account-add-cancel", "close", "Cancel")
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.accounts.adding = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        } else {
            body = body.child(
                div()
                    .flex()
                    .child(self.button("account-add", "plus", "Add account").on_click(
                        cx.listener(|s, _, window, cx| {
                            s.accounts.adding = true;
                            s.accounts.renaming = None;
                            s.begin_account_name(String::new(), window, cx);
                            cx.notify();
                        }),
                    )),
            );
        }
        body.into_any_element()
    }

    fn account_card(&self, state: &AccountState, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let account = &state.account;
        let status = &state.status;
        let provider = account.provider;
        let id = account.id.clone();
        let tone = self.account_tone(account);
        let preferences = self.provider_preferences(provider);
        let is_default = preferences.account.as_deref().unwrap_or(DEFAULT_ACCOUNT) == account.id;
        let renaming = self.accounts.renaming.as_ref() == Some(&account.id);
        let removing = self.accounts.removing.as_ref() == Some(&account.id);
        let email = status.email.as_deref().filter(|text| !text.is_empty());
        let organization = status
            .organization
            .as_deref()
            .filter(|text| !text.is_empty());
        // What follows the address on the identity line, or stands in its place.
        let identity = match (email, organization) {
            (Some(_), Some(organization)) => format!("· {organization}"),
            (Some(_), None) => String::new(),
            (None, Some(organization)) => organization.to_owned(),
            (None, None) => match status.signed_in {
                Some(false) => "Not signed in".to_owned(),
                _ => "Identity not reported yet".to_owned(),
            },
        };
        let directory = account.home.as_ref().map_or_else(
            || "The CLI's own directory".to_owned(),
            |home| home.display().to_string(),
        );
        let mut header = div()
            .relative()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(kit::icon_tile(
                provider.icon(),
                tone,
                32.,
                status.signing_in,
            ));
        header = if renaming {
            header.child(
                settings::input_frame(t)
                    .flex_1()
                    .min_w_0()
                    .child(div().flex_1().min_w_0().child(self.aux.clone())),
            )
        } else {
            header.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(text_px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_ellipsis()
                                    .child(account.name.clone()),
                            )
                            .when(is_default, |d| d.child(kit::chip("NEW TASKS", t.accent)))
                            .when_some(status.plan.as_ref(), |d, plan| {
                                d.child(kit::chip(usage::plan_name(plan).to_uppercase(), t.muted))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(7.))
                            .font_family(crate::theme::mono())
                            .text_size(text_px(11.))
                            .text_color(t.subtle)
                            .when_some(email, |line, email| {
                                line.child(self.censored_email(provider, &account.id, email, cx))
                            })
                            .child(div().min_w_0().text_ellipsis().child(identity)),
                    ),
            )
        };
        header = header.child(match status.signed_in {
            _ if status.signing_in => kit::chip("SIGNING IN", t.warning),
            Some(true) => kit::chip("SIGNED IN", t.added),
            Some(false) => kit::chip("SIGNED OUT", t.removed),
            None => kit::chip("UNKNOWN", t.subtle),
        });
        // The account that new tasks use is the chosen one among the cards.
        let mut card = kit::card(t)
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .when(is_default, |card| {
                card.layer(settings::chosen_layer(t.accent))
            })
            .child(header);
        if status.signing_in {
            card = card.child(
                kit::loader_row(
                    "Waiting for the sign-in that opened in Terminal and your browser",
                    t,
                )
                .relative(),
            );
        }
        if let Some(usage) = &status.usage {
            let mut windows = div().relative().flex().gap(px(16.));
            for window in [usage.primary.as_ref(), usage.secondary.as_ref()]
                .into_iter()
                .flatten()
            {
                windows = windows.child(limit_column(
                    format!("{} limit", usage::window_short(window.window_duration_mins)),
                    Some(window.used_percent),
                    usage::reset_time(window.resets_at),
                    t,
                ));
            }
            if let Some(spend) = &usage.spend {
                windows = windows.child(limit_column(
                    "Usage credits".into(),
                    spend.used_percent,
                    usage::spend_amounts(spend),
                    t,
                ));
            }
            card = card.child(windows);
            if usage::limit_reached(usage) {
                card = card.child(
                    div()
                        .relative()
                        .text_size(text_px(11.5))
                        .text_color(t.removed)
                        .child("This account reached a usage limit. Move its tasks to another account from the composer, or wait for the reset."),
                );
            }
        }
        if let Some(error) = &status.error {
            card = card.child(settings::error_block("Account error", error.clone(), t).relative());
        }
        let mut actions = div()
            .relative()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.));
        if renaming {
            let color = account.color;
            let rename_id = id.clone();
            actions = actions
                .child(
                    self.primary(("account-rename-save", 0usize), "check", "Save name")
                        .disabled(self.account_name_input(cx).is_none())
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if let Some(name) = s.account_name_input(cx) {
                                s.send(core::Request::EditAccount {
                                    provider,
                                    id: rename_id.clone(),
                                    name,
                                    color,
                                });
                                s.accounts.renaming = None;
                                cx.notify();
                            }
                        })),
                )
                .child(
                    self.ghost("account-rename-cancel", "close", "Cancel")
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.accounts.renaming = None;
                            cx.notify();
                        })),
                );
        } else if removing {
            let remove_id = id.clone();
            actions = actions
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(text_px(11.5))
                        .text_color(t.muted)
                        .child("Remove this account and delete its configuration directory?"),
                )
                .child(
                    self.tinted("account-remove-confirm", "trash-2", "Remove", t.removed)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.send(core::Request::RemoveAccount {
                                provider,
                                id: remove_id.clone(),
                            });
                            s.accounts.removing = None;
                            cx.notify();
                        })),
                )
                .child(
                    self.ghost("account-remove-cancel", "close", "Keep")
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.accounts.removing = None;
                            cx.notify();
                        })),
                );
        } else {
            let key = SharedString::from(format!("{provider:?}:{id}"));
            let element = |name: &'static str| (ElementId::from(name), key.clone());
            let sign_id = id.clone();
            actions = actions.child(if status.signed_in == Some(true) {
                self.ghost(element("account-sign-out"), "log-out", "Sign out")
                    .on_click(cx.listener(move |s, _, _, _| {
                        s.send(core::Request::SignOutAccount {
                            provider,
                            id: sign_id.clone(),
                        })
                    }))
            } else {
                self.primary(element("account-sign-in"), "log-in", "Sign in")
                    .disabled(status.signing_in)
                    .on_click(cx.listener(move |s, _, _, _| {
                        s.send(core::Request::SignInAccount {
                            provider,
                            id: sign_id.clone(),
                        })
                    }))
            });
            if !is_default {
                let default_id = (!account.is_default()).then(|| id.clone());
                actions = actions.child(
                    self.ghost(element("account-default"), "star", "Use for new tasks")
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let mut preferences = s.provider_preferences(provider);
                            preferences.account = default_id.clone();
                            s.save_agent_preferences(preferences);
                            cx.notify();
                        })),
                );
            }
            let refresh_id = id.clone();
            let rename_id = id.clone();
            let name = account.name.clone();
            actions = actions
                .child(
                    self.icon_button(
                        element("account-refresh"),
                        "refresh",
                        "Check sign-in and limits",
                    )
                    .on_click(cx.listener(move |s, _, _, _| {
                        s.send(core::Request::RefreshAccount {
                            provider,
                            id: refresh_id.clone(),
                        })
                    })),
                )
                .child(
                    self.icon_button(element("account-rename"), "pencil", "Rename account")
                        .on_click(cx.listener(move |s, _, window, cx| {
                            s.accounts.renaming = Some(rename_id.clone());
                            s.accounts.adding = false;
                            s.begin_account_name(name.clone(), window, cx);
                            cx.notify();
                        })),
                );
            if !account.is_default() {
                let remove_id = id.clone();
                actions = actions.child(
                    self.icon_button(element("account-remove"), "trash-2", "Remove account")
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.accounts.removing = Some(remove_id.clone());
                            cx.notify();
                        })),
                );
            }
            actions = actions.child(div().flex_1());
            let light = t.light;
            for (index, color) in ACCOUNT_COLORS.into_iter().enumerate() {
                let chosen = account.color == Some(color);
                let color_id = id.clone();
                let name = account.name.clone();
                actions = actions.child(
                    self.swatch(
                        (ElementId::from(("account-color", index)), key.clone()),
                        crate::theme::accent_color(color, light),
                        color.label(),
                        chosen,
                        false,
                    )
                    .size(px(18.))
                    .on_click(cx.listener(move |s, _, _, _| {
                        s.send(core::Request::EditAccount {
                            provider,
                            id: color_id.clone(),
                            name: name.clone(),
                            color: Some(color),
                        })
                    })),
                );
            }
        }
        card.child(
            div()
                .relative()
                .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
        )
        .child(actions)
        .child(
            div()
                .relative()
                .font_family(crate::theme::mono())
                .text_size(text_px(10.5))
                .text_color(t.subtle)
                .text_ellipsis()
                .child(directory),
        )
        .into_any_element()
    }
}

// A short palette that keeps accounts apart at the size of a five-point dot.
const ACCOUNT_COLORS: [AccentColor; 6] = [
    AccentColor::Blue,
    AccentColor::Green,
    AccentColor::Purple,
    AccentColor::Pink,
    AccentColor::Gold,
    AccentColor::Teal,
];

// One limit of an account on its settings card: the used share, a meter and a caption. A limit
// without a reported share shows its caption alone.
fn limit_column(label: String, used_percent: Option<i32>, caption: String, t: Theme) -> Div {
    let column = div().flex_1().min_w_0().flex().flex_col().gap(px(4.));
    let caption = div()
        .font_family(crate::theme::mono())
        .text_size(text_px(10.5))
        .text_color(t.subtle)
        .child(caption);
    let Some(used) = used_percent.map(|percent| percent.clamp(0, 100)) else {
        return column
            .child(caps(label, t).text_color(t.muted))
            .child(caption);
    };
    let fraction = used as f32 / 100.;
    column
        .child(kit::leader(
            label,
            format!("{used}%"),
            kit::usage_tone(fraction, t),
            t,
        ))
        .child(
            div()
                .relative()
                .w_full()
                .h(px(6.))
                .child(dither::DitherBar::meter(
                    fraction,
                    kit::usage_tone(fraction, t),
                    t.subtle.opacity(0.55),
                )),
        )
        .child(caption)
}
