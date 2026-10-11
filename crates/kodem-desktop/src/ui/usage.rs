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
    /// the usage panel, which holds the compaction and limit actions.
    pub(super) fn usage_meters(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let meter = |id: &'static str| {
            Button::new(id)
                .h(px(26.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .cursor_pointer()
                .hover(move |s| s.bg(t.selected))
                .focus_visible(move |s| s.bg(t.selected))
                .active(move |s| s.bg(t.control_active))
                .selected(self.panel == Panel::Usage)
                .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                    s.panel = Panel::Usage;
                    s.menu_anchor = Some(bounds.top_right());
                    cx.notify();
                }))
        };
        div()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                meter("context-meter")
                    .accessibility_label("Context usage and compaction")
                    .child(caps("Ctx", t))
                    .child(kit::meter(
                        fraction,
                        fraction.map_or(t.info, |value| context_tone(value, rule, t)),
                        30.,
                        t,
                    ))
                    .tooltip(move |_, cx| {
                        cx.new(|_| ContextHint {
                            usage: context.clone(),
                            rule,
                            theme: t,
                        })
                        .into()
                    }),
            )
            .child(
                meter("limit-meter")
                    .accessibility_label("Account usage, credits and banked resets")
                    .child(caps("Use", t))
                    .child(kit::meter(
                        highest,
                        kit::usage_tone(highest.unwrap_or(0.), t),
                        30.,
                        t,
                    ))
                    .tooltip(move |_, cx| {
                        cx.new(|_| UsageHint {
                            usage: usage.clone(),
                            theme: t,
                        })
                        .into()
                    }),
            )
            .into_any_element()
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
        for window in usage
            .into_iter()
            .flat_map(|usage| [usage.primary.as_ref(), usage.secondary.as_ref()])
            .flatten()
        {
            let fraction = window.used_percent.clamp(0, 100) as f32 / 100.;
            windows = windows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .child(caps(window_short(window.window_duration_mins), t))
                    .child(kit::meter(
                        Some(fraction),
                        kit::usage_tone(fraction, t),
                        36.,
                        t,
                    )),
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
                    .text_size(px(11.))
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
                s.menu_anchor = Some(bounds.top_right());
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

    // The open task's context: occupancy against the compaction threshold, and the manual action.
    fn context_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.theme;
        let view = self.view.as_ref()?;
        let usage = view.task.context_usage.as_ref();
        let rule = self.compact_rule();
        let fraction = usage.and_then(context_fraction);
        let busy = view.task.state.active() || view.task.state == RunState::DeliveryUnknown;
        let started = view.task.provider_thread.is_some();
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(kit::section_header("Context", None, t));
        if let Some(usage) = usage {
            section = section.child(context_readout(usage, rule, t));
        } else {
            section = section.child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.muted)
                    .child("The provider has not reported context usage for this task yet."),
            );
        }
        let over = fraction.is_some_and(|value| value * 100. >= rule.threshold as f32);
        Some(
            section
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
                        .child(self.ghost("compact-rules", "settings-2", "Rules").on_click(
                            cx.listener(|s, _, _, cx| {
                                s.open_settings(settings::PROVIDERS);
                                cx.notify();
                            }),
                        )),
                )
                .when(busy, |section| {
                    section.child(
                        div()
                            .text_size(px(11.))
                            .text_color(t.subtle)
                            .child("Compact now is available when the current turn finishes."),
                    )
                })
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
        let identity = state
            .and_then(|state| {
                state
                    .status
                    .email
                    .clone()
                    .or_else(|| state.status.organization.clone())
            })
            .unwrap_or_else(|| provider.label().to_owned());
        let tone = state.map_or(t.accent, |state| self.account_tone(&state.account));
        let plan = self
            .account_usage()
            .and_then(|usage| usage.plan_type.as_deref())
            .map(plan_name);
        let header = div()
            .relative()
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .overflow_hidden()
            .child(kit::glow_header(
                provider.icon(),
                account_name,
                identity,
                tone,
                t,
            ))
            .when_some(plan, |header, plan| {
                header.child(
                    div()
                        .absolute()
                        .top(px(10.))
                        .right(px(10.))
                        .child(kit::chip(plan.to_uppercase(), tone)),
                )
            });
        let mut panel = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(header)
            .when_some(self.context_section(cx), |panel, section| {
                panel.child(div().px(px(8.)).child(section))
            });
        if let Some(usage) = self
            .view
            .as_ref()
            .and_then(|view| view.task.context_usage.as_ref())
            && let Some(cost) = &usage.cost
        {
            panel = panel.child(div().px(px(8.)).child(kit::leader(
                "Session cost",
                format!("{:.4} {}", cost.amount, cost.currency),
                t.text,
                t,
            )));
        }
        if !runtime.account_usage {
            return panel
                .child(
                    div()
                        .px(px(8.))
                        .pb(px(6.))
                        .text_size(px(11.5))
                        .text_color(t.muted)
                        .child(
                            "Account quotas are unavailable through this provider's CLI. Session cost and context appear when the provider reports them.",
                        ),
                )
                .into_any_element();
        }
        panel = panel.child(
            div()
                .px(px(8.))
                .child(usage_details(self.account_usage(), t)),
        );
        let mut controls = div().flex().items_center().gap(px(6.)).child(
            self.button(
                "refresh-account-usage",
                "refresh",
                if pending { "Working…" } else { "Refresh" },
            )
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
            })),
        );
        if runtime.usage_resets {
            controls = controls.child(
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
                })),
            );
        }
        panel
            .child(
                div()
                    .px(px(8.))
                    .pb(px(6.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(controls)
                    .when(runtime.usage_resets, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.subtle)
                                .child("A reset uses one banked reset on eligible usage limits."),
                        )
                    })
                    .when_some(actions.message, |d, message| {
                        d.child(div().text_size(px(11.5)).text_color(t.text).child(message))
                    }),
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
                actions.message = Some("Usage updated.".into());
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

// A hover card: the surface of a menu at the size of a tooltip.
fn hint_card(theme: Theme) -> Div {
    div()
        .w(px(304.))
        .p(px(12.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .font_family(crate::theme::SANS)
        .text_size(px(12.))
        .text_color(theme.text)
        .bg(theme.raised)
        .border_1()
        .border_color(theme.edge)
        .rounded(px(crate::theme::RADIUS_SURFACE))
        .shadow(theme.floating())
}

// One measured share: a mono label and its figure, a dot-matrix meter with an optional
// threshold tick, and a caption line.
fn meter_block(
    label: &str,
    figure_text: String,
    fraction: f32,
    tone: Hsla,
    tick: Option<f32>,
    caption: (String, String),
    t: Theme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(caps(label, t).text_color(t.muted))
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
                            .w(px(1.))
                            .h(px(10.))
                            .bg(t.text),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .gap(px(8.))
                .child(figure(caption.0, t.subtle))
                .child(figure(caption.1, t.subtle)),
        )
}

// Context occupancy with the point where automatic compaction starts marked on the meter.
fn context_readout(usage: &ContextUsage, rule: CompactRule, t: Theme) -> Div {
    let capacity = usage.window.filter(|window| *window > 0);
    let compaction = if rule.enabled {
        format!("AUTO-COMPACT AT {}%", rule.threshold)
    } else {
        "AUTO-COMPACT OFF".to_owned()
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
        "Window",
        format!("{:.0}% USED", fraction * 100.),
        fraction,
        context_tone(fraction, rule, t),
        rule.enabled.then_some(rule.threshold as f32 / 100.),
        (
            format!(
                "{} / {}",
                number(&usage.used.to_string()),
                number(&capacity.to_string())
            ),
            compaction,
        ),
        t,
    )
}

struct ContextHint {
    usage: Option<ContextUsage>,
    rule: CompactRule,
    theme: Theme,
}
impl Render for ContextHint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let card = hint_card(t).child(kit::section_header("Context", None, t));
        match &self.usage {
            Some(usage) => card
                .child(context_readout(usage, self.rule, t))
                .child(figure(
                    format!(
                        "UPDATED {} AGO · CLICK TO COMPACT",
                        age(usage.reported).to_uppercase()
                    ),
                    t.subtle,
                )),
            None => card.child(
                div()
                    .text_color(t.muted)
                    .child("Context usage has not been reported yet."),
            ),
        }
    }
}

struct UsageHint {
    usage: Option<UsageLimits>,
    theme: Theme,
}
impl Render for UsageHint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        hint_card(self.theme).child(usage_details(self.usage.as_ref(), self.theme))
    }
}

fn usage_details(usage: Option<&UsageLimits>, t: Theme) -> AnyElement {
    let mut details = div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(kit::section_header("Limits", None, t));
    let Some(usage) = usage else {
        return details
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.muted)
                    .child("Usage has not been reported yet."),
            )
            .into_any_element();
    };
    for (label, window) in [
        ("Primary window", &usage.primary),
        ("Secondary window", &usage.secondary),
    ] {
        if let Some(window) = window {
            let used = window.used_percent.clamp(0, 100);
            let progress = used as f32 / 100.;
            details = details.child(meter_block(
                &window_name(label, window.window_duration_mins),
                format!("{used}% USED"),
                progress,
                kit::usage_tone(progress, t),
                None,
                (
                    format!("{}% LEFT", 100 - used),
                    reset_time(window.resets_at).to_uppercase(),
                ),
                t,
            ));
        }
    }
    if usage.primary.is_none() && usage.secondary.is_none() {
        details = details.child(
            div()
                .text_size(px(11.5))
                .text_color(t.muted)
                .child("Usage windows unavailable."),
        );
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
    details
        .child(figure(
            format!("UPDATED {} AGO", age(usage.reported).to_uppercase()),
            t.subtle,
        ))
        .into_any_element()
}

/// Reports that the provider refused further use of this account until a reset.
pub(super) fn limit_reached(usage: &UsageLimits) -> bool {
    usage.limit_reached()
}

/// Returns the fullest limit window as a fraction, or full when a limit was reached.
pub(super) fn highest_usage(usage: &UsageLimits) -> Option<f32> {
    if limit_reached(usage) {
        return Some(1.);
    }
    [usage.primary.as_ref(), usage.secondary.as_ref()]
        .into_iter()
        .flatten()
        .map(|w| w.used_percent.clamp(0, 100) as f32 / 100.)
        .reduce(f32::max)
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
            Some(balance) => format!("{} credits", number(balance)),
            None if c.has_credits => "Available · Balance not reported".into(),
            None => "No credits available".into(),
        },
        None => "Not reported".into(),
    }
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

fn duration(seconds: u64) -> String {
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
