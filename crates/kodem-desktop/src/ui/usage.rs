//! Account usage details and provider-backed reset actions beside the composer.
use super::*;

#[derive(Clone, Default)]
pub(super) struct UsageActions {
    pending: bool,
    // Retain this key after an uncertain response so retry cannot consume a second reset.
    retry_key: Option<String>,
    message: Option<String>,
}

impl Shell {
    /// Shows context occupancy and the account's fullest limit as dot-matrix meters. Both open
    /// the usage panel, which holds the compaction and limit actions. The caller gives the
    /// control its surface and its hover and focus feedback.
    pub(super) fn usage_meters(&self, cx: &mut Context<Self>) -> Button {
        let t = self.theme;
        let context = self
            .view
            .as_ref()
            .and_then(|v| v.task.context_usage.as_ref())
            .cloned();
        let fraction = context.as_ref().and_then(context_fraction);
        let rule = self.compact_rule();
        let usage = self.account_usage().cloned();
        let highest = usage.as_ref().and_then(highest_usage);
        // One control for both meters: they open the same panel.
        Button::new("usage-meters")
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .selected(self.panel == Panel::Usage)
            .accessibility_label("Context usage, compaction and account limits")
            .child(caps("Ctx", t))
            .child(kit::meter(
                fraction,
                fraction.map_or(t.info, |value| context_tone(value, rule, t)),
                30.,
                t,
            ))
            .child(div().w(px(4.)))
            .child(caps("Use", t))
            .child(kit::meter(
                highest,
                kit::usage_tone(highest.unwrap_or(0.), t),
                30.,
                t,
            ))
            .tooltip(move |_, cx| {
                cx.new(|_| MetersHint {
                    context: context.clone(),
                    rule,
                    usage: usage.clone(),
                    theme: t,
                })
                .into()
            })
            .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                s.panel = Panel::Usage;
                s.menu_anchor = Some(*bounds);
                cx.notify();
            }))
    }

    /// Shows the composer account's identity and limit windows above the sidebar footer.
    pub(super) fn sidebar_usage(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(state) = self.composer_account() else {
            return div().into_any_element();
        };
        let tone = self.account_tone(&state.account);
        let name = state.account.name.clone();
        let provider = state.account.provider;
        let usage = state.status.usage.as_ref();
        let limited = usage.is_some_and(limit_reached);
        let mut windows = div().flex().items_center().gap(px(10.));
        let meters = usage.map(limit_meters).unwrap_or_default();
        // Credits without a cap have no share to draw, so the row names the amount spent.
        let spent = usage
            .filter(|_| meters.is_empty())
            .and_then(|usage| usage.spend.as_ref())
            .and_then(|spend| Some(money(spend.used?, spend.currency.as_deref())));
        for (label, fraction) in meters {
            windows = windows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .child(caps(label, t))
                    .child(kit::meter(
                        Some(fraction),
                        kit::usage_tone(fraction, t),
                        36.,
                        t,
                    )),
            );
        }
        if let Some(spent) = spent {
            windows = windows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .child(caps(CREDITS_SHORT, t))
                    .child(figure(spent, t.muted)),
            );
        }
        let signed_out = state.status.signed_in == Some(false);
        self.control("sidebar-usage")
            .w_full()
            .h(px(30.))
            .px(px(12.))
            .gap(px(7.))
            .rounded_none()
            .justify_start()
            .border_0()
            .border_t_1()
            .border_color(t.border)
            .bg(gpui::transparent_black())
            .hover(move |s| s.bg(t.selected))
            .accessibility_label("Account usage")
            .child(icon(provider.icon()).size(px(12.)).text_color(t.muted))
            .child(div().size(px(6.)).rounded(px(1.)).flex_shrink_0().bg(tone))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .text_size(text_px(11.))
                    .text_color(t.muted)
                    .child(name),
            )
            .map(|row| {
                if signed_out {
                    row.child(kit::chip("SIGNED OUT", t.removed))
                } else if limited {
                    row.child(kit::chip("LIMIT", t.removed))
                } else {
                    row.child(windows)
                }
            })
            .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                s.panel = Panel::Usage;
                s.menu_anchor = Some(*bounds);
                cx.notify();
            }))
            .into_any_element()
    }

    fn account_usage(&self) -> Option<&UsageLimits> {
        self.composer_account()
            .and_then(|state| state.status.usage.as_ref())
    }

    /// Returns the automatic compaction rule of the composer's provider and model.
    pub(super) fn compact_rule(&self) -> CompactRule {
        let preferences = self.composer_preferences();
        self.settings
            .auto_compact
            .rule(preferences.provider, preferences.model.as_deref())
    }

    // The open task's context as a block: occupancy against the compaction threshold and the
    // session cost, with the manual action in its footer.
    fn context_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.theme;
        let view = self.view.as_ref()?;
        let usage = view.task.context_usage.as_ref();
        let rule = self.compact_rule();
        let fraction = usage.and_then(context_fraction);
        let busy = view.task.state.active() || view.task.state == RunState::DeliveryUnknown;
        let started = view.task.provider_thread.is_some();
        let over = fraction.is_some_and(|value| value * 100. >= rule.threshold as f32);
        let tone = fraction.map_or(t.info, |value| context_tone(value, rule, t));
        let body = match usage {
            Some(usage) => div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(context_readout(usage, rule, t))
                .when_some(usage.cost.as_ref(), |body, cost| {
                    body.child(kit::leader("Session cost", session_cost(cost), t.text, t))
                }),
            None => note(
                "The provider has not reported context usage for this task yet.",
                t,
            ),
        };
        // The Antigravity CLI compacts by its own rule and offers no command for it.
        let footer = if view.task.provider == Provider::Antigravity {
            block_footer(t).child(caption(
                "The Antigravity CLI compacts the context by itself, so Kodem has no compaction rule or command for it.",
                t,
            ))
        } else {
            block_footer(t)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            self.button("compact-now", "layers", "Compact now")
                                .disabled(busy || !started)
                                .when(over && !busy, |b| {
                                    b.border_color(t.warning.opacity(0.5)).text_color(t.warning)
                                })
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.panel = Panel::None;
                                    s.compact_task(cx);
                                })),
                        )
                        .child(div().flex_1())
                        .child(self.ghost("compact-rules", "settings-2", "Rules").on_click(
                            cx.listener(|s, _, _, cx| {
                                s.open_provider_settings(settings::ProviderPage::Compaction);
                                cx.notify();
                            }),
                        )),
                )
                .when(busy, |footer| {
                    footer.child(caption(
                        "Compact now is available when the current turn finishes.",
                        t,
                    ))
                })
        };
        Some(
            kit::block(t)
                .child(kit::block_header("Context", tone, t))
                .child(div().p(px(10.)).child(body))
                .child(footer)
                .into_any_element(),
        )
    }

    /// Builds the context and account details with their explicit actions.
    pub(super) fn usage_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let preferences = self.composer_preferences();
        let provider = preferences.provider;
        let account = preferences
            .account
            .unwrap_or_else(|| DEFAULT_ACCOUNT.into());
        let runtime = self.selected_runtime();
        let actions = self
            .usage_actions
            .get(&(provider, account.clone()))
            .cloned()
            .unwrap_or_default();
        let available = self
            .account_usage()
            .and_then(|u| u.banked_resets.as_ref())
            .is_some_and(|r| r.available_count > 0);
        let retry = actions.retry_key.is_some();
        let pending = actions.pending;
        let state = self.composer_account();
        let account_name = state.map_or_else(
            || provider.label().to_owned(),
            |state| state.account.name.clone(),
        );
        let identity = match state.and_then(|state| Some((state, state.status.email.as_deref()?))) {
            Some((state, email)) => self
                .censored_email(provider, &state.account.id, email, cx)
                .into_any_element(),
            None => state
                .and_then(|state| state.status.organization.clone())
                .unwrap_or_else(|| provider.label().to_owned())
                .into_any_element(),
        };
        let tone = state.map_or(t.accent, |state| self.account_tone(&state.account));
        let plan = self
            .account_usage()
            .and_then(|usage| usage.plan_type.as_deref())
            .map(plan_name);
        // The plan chip sits over the header, level with the icon tile; the padding keeps a long
        // identity clear of it.
        let header = div()
            .relative()
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .overflow_hidden()
            .child(
                kit::glow_header(provider.icon(), account_name, identity, tone, t)
                    .when(plan.is_some(), |header| header.pr(px(72.))),
            )
            .when_some(plan, |header, plan| {
                header.child(
                    div()
                        .absolute()
                        .top(px(25.))
                        .right(px(14.))
                        .child(kit::chip(plan.to_uppercase(), tone)),
                )
            });
        let panel = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .pb(px(2.))
            .child(header)
            .when_some(self.context_section(cx), |panel, section| {
                panel.child(section)
            });
        if !runtime.account_usage {
            return panel
                .child(div().px(px(8.)).pb(px(4.)).child(note(
                    "Account quotas are unavailable through this provider's CLI. Session cost and context appear when the provider reports them.",
                    t,
                )))
                .into_any_element();
        }
        let usage = self.account_usage();
        let limits_tone = usage
            .and_then(highest_usage)
            .map_or(t.accent, |value| kit::usage_tone(value, t));
        let updated = if pending {
            Some("Updating…".to_owned())
        } else {
            usage.map(|usage| format!("{} ago", age(usage.reported)))
        };
        let refresh = self
            .icon_button("refresh-account-usage", "refresh", "Refresh usage")
            .size(px(22.))
            .disabled(pending)
            .on_click(cx.listener({
                let account = account.clone();
                move |s, _, _, cx| {
                    let actions = s
                        .usage_actions
                        .entry((provider, account.clone()))
                        .or_default();
                    actions.pending = true;
                    actions.message = None;
                    s.send(core::Request::RefreshUsage {
                        provider,
                        account: Some(account.clone()),
                    });
                    cx.notify();
                }
            }));
        let reset = runtime.usage_resets.then(|| {
            self.tinted(
                "consume-usage-reset",
                "zap",
                if retry {
                    "Retry reset"
                } else {
                    "Use one reset"
                },
                t.warning,
            )
            .disabled(pending || (!available && !retry))
            .on_click(cx.listener(move |s, _, _, cx| {
                let actions = s
                    .usage_actions
                    .entry((provider, account.clone()))
                    .or_default();
                if actions.pending {
                    return;
                }
                let key = actions.retry_key.get_or_insert_with(core::new_id).clone();
                actions.pending = true;
                actions.message = None;
                s.send(core::Request::ConsumeUsageReset {
                    provider,
                    account: Some(account.clone()),
                    idempotency_key: key,
                });
                cx.notify();
            }))
        });
        let footer = (reset.is_some() || actions.message.is_some()).then(|| {
            block_footer(t)
                .when_some(reset, |footer, reset| {
                    footer.child(
                        div().flex().items_center().gap(px(8.)).child(reset).child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(caption("Uses one banked reset on eligible limits.", t)),
                        ),
                    )
                })
                .when_some(actions.message, |footer, message| {
                    footer.child(
                        div()
                            .text_size(text_px(11.5))
                            .text_color(t.text)
                            .child(message),
                    )
                })
        });
        panel
            .child(
                kit::block(t)
                    .child(
                        kit::block_header("Limits", limits_tone, t)
                            .when_some(updated, |header, updated| {
                                header.child(figure(updated, t.subtle))
                            })
                            .child(refresh),
                    )
                    .child(div().p(px(10.)).child(usage_details(usage, t)))
                    .when_some(footer, |block, footer| block.child(footer)),
            )
            .into_any_element()
    }

    /// Applies account responses while preserving the redemption key after uncertain failures.
    pub(super) fn apply_usage_reply(
        &mut self,
        provider: Provider,
        account: String,
        result: Result<Response, String>,
        cx: &mut Context<Self>,
    ) {
        let status = self.workspace.as_mut().and_then(|workspace| {
            workspace
                .edit()
                .accounts
                .iter_mut()
                .find(|state| state.account.provider == provider && state.account.id == account)
                .map(|state| &mut state.status)
        });
        let actions = self.usage_actions.entry((provider, account)).or_default();
        actions.pending = false;
        match result {
            Ok(Response::Usage { usage, .. }) => {
                if let Some(status) = status {
                    status.usage = Some(usage);
                }
            }
            Ok(Response::UsageReset {
                outcome,
                usage,
                refresh_error,
                ..
            }) => {
                actions.retry_key = None;
                let message = match outcome {
                    UsageResetOutcome::Reset => "One banked reset used.",
                    UsageResetOutcome::AlreadyRedeemed => "This reset was already applied.",
                    UsageResetOutcome::NothingToReset => {
                        "No usage limits are eligible for a reset. No reset was used."
                    }
                    UsageResetOutcome::NoCredit => "No banked resets are available.",
                };
                if let Some(status) = status {
                    if let Some(usage) = usage {
                        status.usage = Some(usage);
                    } else if let Some(usage) = &mut status.usage {
                        // The old inventory is no longer safe to use for another redemption.
                        usage.banked_resets = None;
                    }
                }
                actions.message = Some(match refresh_error {
                    Some(error) => {
                        format!("{message} Could not refresh usage: {error}. Press Refresh.")
                    }
                    None => message.into(),
                });
            }
            Err(error) | Ok(Response::Error(error)) => {
                actions.message = Some(if actions.retry_key.is_some() {
                    format!(
                        "{error}\nRetry reset checks the same attempt without using a second reset."
                    )
                } else {
                    error
                });
            }
            _ => {
                actions.message =
                    Some("Unexpected account response. Refresh to check usage.".into())
            }
        }
        cx.notify();
    }
}

// Returns the used share of the context window, when the provider reported its size.
fn context_fraction(usage: &ContextUsage) -> Option<f32> {
    usage
        .window
        .filter(|window| *window > 0)
        .map(|window| usage.used as f32 / window as f32)
}

// The content of a hover card, which `hint_surface` places on the glass of a menu.
fn hint_card(theme: Theme) -> Div {
    div()
        .relative()
        .w(px(304.))
        .p(px(12.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .font_family(crate::theme::sans())
        .text_size(text_px(12.))
        .text_color(theme.text)
        .border_1()
        .border_color(gpui::transparent_black())
        .rounded(px(crate::theme::RADIUS_SURFACE))
}

// A hover card: the glass, light and dithered shadow of a menu at the size of a tooltip.
fn hint_surface(card: Div, theme: Theme) -> Div {
    div()
        .relative()
        .child(dither::shadow(theme, crate::theme::RADIUS_SURFACE, 4.))
        .child(kit::floating_face(
            theme.raised,
            crate::theme::RADIUS_SURFACE,
            theme,
        ))
        .child(card)
}

// A paragraph that stands in for a readout the provider has not reported.
fn note(text: &'static str, t: Theme) -> Div {
    div()
        .text_size(text_px(11.5))
        .line_height(text_px(16.))
        .text_color(t.muted)
        .child(text)
}

// A secondary line under an action or a meter.
fn caption(text: &'static str, t: Theme) -> Div {
    div()
        .text_size(text_px(11.))
        .line_height(text_px(15.))
        .text_color(t.subtle)
        .child(text)
}

// The footer of a usage block: its actions on the header tone, below a hairline.
fn block_footer(t: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .px(px(8.))
        .py(px(7.))
        .bg(t.header)
        .border_t_1()
        .border_color(t.border)
}

// One measured share: a label and its figure, a dot-matrix meter with an optional threshold
// tick, and a caption line with an optional second figure on its right.
fn meter_block(
    label: &str,
    figure_text: String,
    fraction: f32,
    tone: Hsla,
    tick: Option<f32>,
    caption: (String, Option<String>),
    t: Theme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .min_w_0()
                        .text_ellipsis()
                        .text_size(text_px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.text)
                        .child(label.to_owned()),
                )
                .child(figure(figure_text, tone).font_weight(FontWeight::SEMIBOLD)),
        )
        .child(
            div()
                .relative()
                .child(wide_meter(fraction, tone, t))
                .when_some(tick, |meter, tick| {
                    meter.child(
                        div()
                            .absolute()
                            .top(px(-2.))
                            .left(relative(tick.clamp(0., 1.)))
                            .w(px(2.))
                            .h(px(10.))
                            .rounded(px(1.))
                            .bg(t.muted),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .gap(px(8.))
                .child(figure(caption.0, t.subtle))
                .when_some(caption.1, |line, right| line.child(figure(right, t.subtle))),
        )
}

// Context occupancy with the point where automatic compaction starts marked on the meter.
fn context_readout(usage: &ContextUsage, rule: CompactRule, t: Theme) -> Div {
    let capacity = usage.window.filter(|window| *window > 0);
    let compaction = if rule.enabled {
        format!("Auto-compact at {}%", rule.threshold)
    } else {
        "Auto-compact off".to_owned()
    };
    let Some((fraction, capacity)) = context_fraction(usage).zip(capacity) else {
        return div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(kit::leader(
                "Tokens used",
                number(&usage.used.to_string()),
                t.text,
                t,
            ))
            .child(figure(compaction, t.subtle));
    };
    meter_block(
        "Context window",
        format!("{:.0}% used", fraction * 100.),
        fraction,
        context_tone(fraction, rule, t),
        rule.enabled.then_some(rule.threshold as f32 / 100.),
        (
            format!(
                "{} / {}",
                number(&usage.used.to_string()),
                number(&capacity.to_string())
            ),
            Some(compaction),
        ),
        t,
    )
}

// The hover card of the composer's meters: the context window, then the account's limits.
struct MetersHint {
    context: Option<ContextUsage>,
    rule: CompactRule,
    usage: Option<UsageLimits>,
    theme: Theme,
}
impl Render for MetersHint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let card = hint_card(t).child(kit::section_header("Context", None, t));
        let card = match &self.context {
            Some(context) => card.child(context_readout(context, self.rule, t)),
            None => card.child(
                div()
                    .text_color(t.muted)
                    .child("Context usage has not been reported yet."),
            ),
        };
        let card = card
            .child(kit::section_header("Limits", None, t))
            .child(usage_details(self.usage.as_ref(), t))
            .when_some(self.usage.as_ref(), |card, usage| {
                card.child(figure(
                    format!("UPDATED {} AGO", age(usage.reported).to_uppercase()),
                    t.subtle,
                ))
            })
            .child(figure("CLICK FOR COMPACTION AND LIMIT ACTIONS", t.subtle));
        hint_surface(card, t)
    }
}

// The account's limit windows, credits and banked resets, under a header the caller supplies.
fn usage_details(usage: Option<&UsageLimits>, t: Theme) -> AnyElement {
    let Some(usage) = usage else {
        return note("Usage has not been reported yet.", t).into_any_element();
    };
    let mut details = div().flex().flex_col().gap(px(12.));
    let seconds = now() / 1000;
    for (label, window) in [
        ("Primary window", &usage.primary),
        ("Secondary window", &usage.secondary),
    ] {
        if let Some(window) = window {
            let used = window.used_percent.clamp(0, 100);
            let progress = used as f32 / 100.;
            let title = match &window.name {
                Some(name) => format!(
                    "{name} · {}",
                    window_name(label, window.window_duration_mins).to_lowercase()
                ),
                None => window_name(label, window.window_duration_mins),
            };
            let pace = pace(window, seconds);
            details = details.child(
                meter_block(
                    &title,
                    format!("{used}% used"),
                    progress,
                    kit::usage_tone(progress, t),
                    pace.as_ref().map(|pace| pace.expected),
                    (
                        reset_time(window.resets_at),
                        Some(format!("{}% left", 100 - used)),
                    ),
                    t,
                )
                .when_some(pace, |block, pace| block.child(pace_line(&pace, t))),
            );
        }
    }
    if let Some(spend) = usage.spend.as_ref() {
        details = details.child(spend_readout(spend, t));
    }
    if usage.primary.is_none() && usage.secondary.is_none() && usage.spend.is_none() {
        details = details.child(note("Usage windows unavailable.", t));
    }
    // Credits and banked resets are shown only when the provider reports them.
    if let Some(credits) = usage.credits.as_ref() {
        details = details.child(kit::leader(
            "Credits",
            credit_balance(Some(credits)),
            t.text,
            t,
        ));
    }
    if let Some(resets) = usage.banked_resets.as_ref() {
        details = details.child(kit::leader(
            "Banked resets",
            format!("{} available", resets.available_count.max(0)),
            t.text,
            t,
        ));
    }
    if limit_reached(usage) {
        details = details.child(
            div()
                .flex()
                .items_center()
                .gap(px(7.))
                .child(kit::marker(t.removed))
                .child(caps("A usage or spending limit has been reached", t).text_color(t.removed)),
        );
    }
    details.into_any_element()
}

// Pace is hidden in the first 3% of a window, where a few requests make the rate meaningless.
const PACE_START: f32 = 0.03;
// Use within this many percentage points of the even rate counts as on pace.
const ON_PACE: f32 = 2.;
// Use this many points beyond the even rate is far ahead of it.
const FAR_AHEAD: f32 = 12.;

// A limit window's use against an even rate over the whole window, and where the current rate
// takes it by the reset.
struct Pace {
    // Share of the window that has passed, which is where even use would stand by now.
    expected: f32,
    // Percentage points used beyond even use; negative below it.
    delta: f32,
    // Seconds until the current rate reaches the limit, when that comes before the reset.
    runs_out: Option<u64>,
    // Used percentage at the reset at the current rate.
    projected: f32,
}

// Measures the pace of a window from its length, its reset and its used share, at `now` in Unix
// seconds. A window without a length or a reset in its span has no pace.
fn pace(window: &UsageWindow, now: u64) -> Option<Pace> {
    let length = window.window_duration_mins.filter(|minutes| *minutes > 0)? * 60;
    let left = window
        .resets_at?
        .checked_sub(now)
        .filter(|left| (1..=length).contains(left))?;
    let elapsed = length - left;
    let expected = elapsed as f32 / length as f32;
    if expected < PACE_START {
        return None;
    }
    let used = window.used_percent.clamp(0, 100) as f32;
    // Percent per second since the window started.
    let rate = used / elapsed as f32;
    let runs_out = if used >= 100. {
        Some(0)
    } else {
        Some((100. - used) / rate)
            .filter(|seconds| seconds.is_finite() && *seconds < left as f32)
            .map(|seconds| seconds as u64)
    };
    Some(Pace {
        expected,
        delta: used - expected * 100.,
        runs_out,
        projected: used + rate * left as f32,
    })
}

// How the use compares with even use, and when the current rate reaches the limit or how much
// of it the window uses by its reset.
fn pace_line(pace: &Pace, t: Theme) -> Div {
    let points = pace.delta.abs().round();
    let (state, tone) = if pace.delta.abs() <= ON_PACE {
        ("On pace".to_owned(), t.subtle)
    } else if pace.delta > 0. {
        (
            format!("{points:.0}% in deficit"),
            if pace.delta > FAR_AHEAD {
                t.removed
            } else {
                t.warning
            },
        )
    } else {
        (format!("{points:.0}% in reserve"), t.subtle)
    };
    let outlook = match pace.runs_out {
        Some(0) => "Limit reached".to_owned(),
        Some(seconds) => format!("Runs out in {}", duration(seconds)),
        None => format!("≈{:.0}% at reset", pace.projected),
    };
    div()
        .flex()
        .justify_between()
        .gap(px(8.))
        .child(figure(state, tone))
        .child(figure(outlook, tone))
}

/// Reports that the provider refused further use of this account until a reset.
pub(super) fn limit_reached(usage: &UsageLimits) -> bool {
    usage.limit_reached()
}

/// Returns the fullest limit of the account as a fraction, or full when a limit was reached.
pub(super) fn highest_usage(usage: &UsageLimits) -> Option<f32> {
    if limit_reached(usage) {
        return Some(1.);
    }
    limit_meters(usage)
        .into_iter()
        .map(|(_, fraction)| fraction)
        .reduce(f32::max)
}

/// Caption of the usage credits where a limit window shows its length.
const CREDITS_SHORT: &str = "cred";

/// Returns the limits that can stop the account, each as a short caption and its used share.
///
/// An account without plan windows pays for usage with credits, so their cap stands in.
pub(super) fn limit_meters(usage: &UsageLimits) -> Vec<(String, f32)> {
    let mut meters: Vec<(String, f32)> = [usage.primary.as_ref(), usage.secondary.as_ref()]
        .into_iter()
        .flatten()
        .map(|window| {
            (
                window_caption(window),
                window.used_percent.clamp(0, 100) as f32 / 100.,
            )
        })
        .collect();
    if meters.is_empty()
        && let Some(percent) = usage.spend.as_ref().and_then(|spend| spend.used_percent)
    {
        meters.push((CREDITS_SHORT.into(), percent.clamp(0, 100) as f32 / 100.));
    }
    meters
}

/// Describes the usage credits of an account as the amount spent and its cap, when it has one.
pub(super) fn spend_amounts(spend: &UsageSpend) -> String {
    let currency = spend.currency.as_deref();
    match (spend.used, spend.limit) {
        (Some(used), Some(limit)) => {
            format!("{} / {}", money(used, currency), money(limit, currency))
        }
        (Some(used), None) => format!("{} spent", money(used, currency)),
        (None, Some(limit)) => format!("{} cap", money(limit, currency)),
        (None, None) => "Amounts not reported".into(),
    }
}

// Usage credits against their cap, or the amount alone when the provider reports no share.
fn spend_readout(spend: &UsageSpend, t: Theme) -> Div {
    let Some(percent) = spend.used_percent else {
        return kit::leader("Usage credits", spend_amounts(spend), t.text, t);
    };
    let used = percent.clamp(0, 100);
    let progress = used as f32 / 100.;
    meter_block(
        "Usage credits",
        format!("{used}% used"),
        progress,
        kit::usage_tone(progress, t),
        None,
        (spend_amounts(spend), Some(format!("{}% left", 100 - used))),
        t,
    )
}

// Formats an amount given in the smallest unit of its currency, such as cents.
fn money(minor: f64, currency: Option<&str>) -> String {
    let amount = number(&format!("{:.2}", minor / 100.));
    match currency {
        None | Some("USD") => format!("${amount}"),
        Some(currency) => format!("{amount} {currency}"),
    }
}

/// Formats the session cost a provider reported in whole currency units, to the cent.
pub(super) fn session_cost(cost: &UsageCost) -> String {
    let amount = number(&format!("{:.2}", cost.amount));
    if cost.currency.eq_ignore_ascii_case("usd") {
        format!("${amount}")
    } else {
        format!("{amount} {}", cost.currency)
    }
}

// Context is calm until it nears the point where it is compacted.
fn context_tone(fraction: f32, rule: CompactRule, t: Theme) -> Hsla {
    let limit = if rule.enabled {
        rule.threshold as f32 / 100.
    } else {
        0.9
    };
    if fraction >= limit {
        t.removed
    } else if fraction >= limit - 0.15 {
        t.warning
    } else {
        t.info
    }
}

// A dot-matrix meter that fills the width of a details card.
fn wide_meter(fraction: f32, color: Hsla, t: Theme) -> Div {
    div()
        .relative()
        .w_full()
        .h(px(6.))
        .child(dither::DitherBar::meter(
            fraction,
            color,
            t.subtle.opacity(0.55),
        ))
}

/// Returns the short caption of a limit window: the first word of its name, such as `Gemini`,
/// where a provider names its windows, else its length.
pub(super) fn window_caption(window: &UsageWindow) -> String {
    window
        .name
        .as_deref()
        .and_then(|name| name.split_whitespace().next())
        .map_or_else(|| window_short(window.window_duration_mins), str::to_owned)
}

/// Returns a two- or three-character caption for a limit window, such as `5H` or `7D`.
pub(super) fn window_short(minutes: Option<u64>) -> String {
    match minutes {
        Some(m) if m > 0 && m % 1440 == 0 => format!("{}d", m / 1440),
        Some(m) if m > 0 && m % 60 == 0 => format!("{}h", m / 60),
        Some(m) if m > 0 => format!("{m}m"),
        _ => "use".into(),
    }
}

fn credit_balance(credits: Option<&UsageCredits>) -> String {
    match credits {
        Some(c) if c.unlimited => "Unlimited".into(),
        Some(c) => match c.balance.as_deref().filter(|v| !v.is_empty()) {
            Some(balance) => format!("{} credits", credit_amount(balance)),
            None if c.has_credits => "Available · Balance not reported".into(),
            None => "No credits available".into(),
        },
        None => "Not reported".into(),
    }
}

// Rounds a decimal credit balance to the hundredth and drops the zeros a whole amount would
// show, so `2500.0000000000` reads `2,500`. A balance that is not a number stays as reported.
fn credit_amount(balance: &str) -> String {
    let Ok(value) = balance.parse::<f64>() else {
        return number(balance);
    };
    let rounded = format!("{value:.2}");
    number(rounded.trim_end_matches('0').trim_end_matches('.'))
}

/// Returns a provider plan identifier as a display name.
pub(super) fn plan_name(plan: &str) -> String {
    match plan {
        "prolite" => "Pro Lite".into(),
        "pro" => "Pro".into(),
        "plus" => "Plus".into(),
        "free" => "Free".into(),
        "team" => "Team".into(),
        "business" => "Business".into(),
        "enterprise" => "Enterprise".into(),
        "edu" => "Edu".into(),
        _ => plan.replace('_', " "),
    }
}

fn window_name(fallback: &str, minutes: Option<u64>) -> String {
    match minutes {
        Some(10080) => "Weekly limit".into(),
        Some(1440) => "Daily limit".into(),
        Some(m) if m > 0 && m % 1440 == 0 => format!("{}-day limit", m / 1440),
        Some(m) if m > 0 && m % 60 == 0 => format!("{}-hour limit", m / 60),
        Some(m) if m > 0 => format!("{m}-minute limit"),
        _ => fallback.into(),
    }
}

/// Describes when a limit window resets, from its Unix-second timestamp.
pub(super) fn reset_time(reset: Option<u64>) -> String {
    match reset {
        None => "Reset time unavailable".into(),
        Some(reset) => {
            let seconds = reset.saturating_sub(now() / 1000);
            if seconds == 0 {
                "Awaiting updated limits".into()
            } else {
                format!("Resets in {}", duration(seconds))
            }
        }
    }
}

/// Names a span of seconds compactly, such as `12s`, `4m` or `1h 12m`.
pub(super) fn duration(seconds: u64) -> String {
    if seconds >= 86400 {
        format!(
            "{}d {}h {}m",
            seconds / 86400,
            seconds % 86400 / 3600,
            seconds % 3600 / 60
        )
    } else if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn age(reported: u64) -> String {
    duration(now().saturating_sub(reported) / 1000)
}

// Preserve the provider's decimal precision; only group plain numeric strings.
fn number(value: &str) -> String {
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(w, f)| (w, Some(f)));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_some_and(|f| !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return value.into();
    }
    let mut result = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            result.push(',');
        }
        result.push(digit);
    }
    if let Some(fraction) = fraction {
        result.push('.');
        result.push_str(fraction);
    }
    result
}

impl Shell {
    /// Asks the provider to summarize the selected task's conversation and free its context.
    pub(super) fn compact_task(&mut self, cx: &mut Context<Self>) {
        let Some(task_id) = self.selected.clone() else {
            return;
        };
        self.send(core::Request::CompactTask { task_id });
        cx.notify();
    }
}
