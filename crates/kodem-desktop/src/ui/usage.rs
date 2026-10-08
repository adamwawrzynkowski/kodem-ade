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
    /// Shows context occupancy and opens account usage from the quota ring.
    pub(super) fn usage_meters(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let context = self
            .view
            .as_ref()
            .and_then(|v| v.task.context_usage.as_ref())
            .cloned();
        let fraction = context.as_ref().and_then(|u| {
            u.window
                .filter(|w| *w > 0)
                .map(|w| u.used as f32 / w as f32)
        });
        let context_hint = context.as_ref().map_or_else(|| "Context usage\nNot reported by the provider yet.".into(), |u| {
            let limit = u.window.filter(|w| *w > 0).map_or_else(|| format!("{} tokens used · Capacity unavailable", number(&u.used.to_string())), |w| format!("{} of {} tokens · {:.1}% used", number(&u.used.to_string()), number(&w.to_string()), u.used as f64 / w as f64 * 100.));
            format!("Context usage\n{limit}\nUpdated {} ago. Last context size, not cumulative usage.", age(u.reported))
        });
        let usage = self.account_usage().cloned();
        let highest = usage.as_ref().and_then(highest_usage);
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_1()
            .child(
                div()
                    .id("context-meter")
                    .w(px(14.))
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .aria_label(context_hint.clone())
                    .role(Role::Meter)
                    .when_some(fraction, |d, value| {
                        d.aria_numeric_value((value.clamp(0., 1.) * 100.) as f64)
                            .aria_min_numeric_value(0.)
                            .aria_max_numeric_value(100.)
                    })
                    .child(meter(fraction, rgb_to_hsla(rgb(0x55bdd7))))
                    .tooltip(move |_, cx| {
                        cx.new(|_| ContextHint {
                            usage: context.clone(),
                            theme: t,
                        })
                        .into()
                    }),
            )
            .child(
                Button::new("limit-meter")
                    .w(px(16.))
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(t.selected))
                    .focus(move |s| s.bg(t.selected))
                    .active(move |s| s.bg(t.inset))
                    .accessibility_label("Account usage, credits and banked resets")
                    .selected(self.panel == Panel::Usage)
                    .child(meter(highest, usage_color(highest.unwrap_or(0.), t)))
                    .tooltip(move |_, cx| {
                        cx.new(|_| UsageHint {
                            usage: usage.clone(),
                            theme: t,
                        })
                        .into()
                    })
                    .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                        s.panel = Panel::Usage;
                        s.menu_anchor = Some(bounds.top_right());
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn account_usage(&self) -> Option<&UsageLimits> {
        self.workspace
            .as_ref()
            .and_then(|w| w.runtimes.get(&self.composer_preferences().provider))
            .and_then(|runtime| runtime.usage.as_ref())
    }

    /// Builds the account details and explicit refresh and reset actions.
    pub(super) fn usage_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let provider = self.composer_preferences().provider;
        let runtime = self.selected_runtime();
        let actions = self
            .usage_actions
            .get(&provider)
            .cloned()
            .unwrap_or_default();
        let available = self
            .account_usage()
            .and_then(|u| u.banked_resets.as_ref())
            .is_some_and(|r| r.available_count > 0);
        let retry = actions.retry_key.is_some();
        let pending = actions.pending;
        let mut panel = div().flex().flex_col().gap_2().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(icon(provider.icon()).size(px(15.)))
                .child(format!("{} usage", provider.label())),
        );
        if let Some(usage) = self
            .view
            .as_ref()
            .and_then(|view| view.task.context_usage.as_ref())
            && let Some(cost) = &usage.cost
        {
            panel = panel
                .child(div().text_size(px(13.)).child(format!(
                    "Session cost: {:.4} {}",
                    cost.amount, cost.currency
                )))
                .child(div().text_size(px(11.)).text_color(t.muted).child(format!(
                    "Reported by {} · updated {} ago",
                    provider.label(),
                    age(usage.reported)
                )));
        }
        if !runtime.account_usage {
            return panel.child(div().text_size(px(12.)).text_color(t.muted).child("Account quotas are unavailable through this provider's CLI. Session cost and context appear when the provider reports them.")).into_any_element();
        }
        panel = panel
            .child(usage_details(self.account_usage(), t))
            .child(div().h(px(1.)).bg(t.border));
        let mut controls = div().flex().items_center().gap_2().child(
            self.ghost(
                "refresh-account-usage",
                "refresh",
                if pending { "Working…" } else { "Refresh" },
            )
            .disabled(pending)
            .on_click(cx.listener(move |s, _, _, cx| {
                let actions = s.usage_actions.entry(provider).or_default();
                actions.pending = true;
                actions.message = None;
                s.send(core::Request::RefreshUsage { provider });
                cx.notify();
            })),
        );
        if runtime.usage_resets {
            controls = controls.child(
                self.control("consume-usage-reset")
                    .child(if retry {
                        "Retry reset"
                    } else {
                        "Use one reset"
                    })
                    .disabled(pending || (!available && !retry))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        let actions = s.usage_actions.entry(provider).or_default();
                        if actions.pending {
                            return;
                        }
                        let key = actions.retry_key.get_or_insert_with(core::new_id).clone();
                        actions.pending = true;
                        actions.message = None;
                        s.send(core::Request::ConsumeUsageReset {
                            provider,
                            idempotency_key: key,
                        });
                        cx.notify();
                    })),
            );
            panel = panel.child(
                div()
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .child("Uses one banked reset on eligible usage limits."),
            );
        }
        panel
            .child(controls)
            .when_some(actions.message, |d, message| {
                d.child(div().text_size(px(12.)).text_color(t.text).child(message))
            })
            .into_any_element()
    }

    /// Applies account responses while preserving the redemption key after uncertain failures.
    pub(super) fn apply_usage_reply(
        &mut self,
        provider: Provider,
        result: Result<Response, String>,
        cx: &mut Context<Self>,
    ) {
        let actions = self.usage_actions.entry(provider).or_default();
        actions.pending = false;
        match result {
            Ok(Response::Usage { usage, .. }) => {
                if let Some(workspace) = &mut self.workspace {
                    workspace.runtimes.entry(provider).or_default().usage = Some(usage);
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
                if let Some(workspace) = &mut self.workspace {
                    if let Some(usage) = usage {
                        workspace.runtimes.entry(provider).or_default().usage = Some(usage);
                    } else if let Some(usage) =
                        &mut workspace.runtimes.entry(provider).or_default().usage
                    {
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

struct ContextHint {
    usage: Option<ContextUsage>,
    theme: Theme,
}
impl Render for ContextHint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let capacity = self
            .usage
            .as_ref()
            .and_then(|u| u.window)
            .filter(|w| *w > 0);
        let fraction = self
            .usage
            .as_ref()
            .zip(capacity)
            .map(|(u, w)| u.used as f64 / w as f64);
        let color = rgb_to_hsla(rgb(0x55bdd7));
        let mut details = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_1()
            .text_size(px(12.))
            .text_color(t.text)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Context usage"),
                    )
                    .when_some(fraction, |d, value| {
                        d.child(
                            div()
                                .text_color(t.muted)
                                .child(format!("{:.1}% used", value * 100.)),
                        )
                    }),
            )
            .when_some(fraction, |d, value| {
                d.child(
                    div()
                        .h(px(4.))
                        .w_full()
                        .rounded(px(1.))
                        .bg(color.opacity(0.15))
                        .child(
                            div()
                                .h_full()
                                .w(relative(value.clamp(0., 1.) as f32))
                                .rounded(px(1.))
                                .bg(color),
                        ),
                )
            });
        if let Some(usage) = &self.usage {
            details = details
                .child(detail_row(
                    "Tokens used",
                    number(&usage.used.to_string()),
                    t,
                ))
                .child(detail_row(
                    "Context limit",
                    capacity.map_or_else(|| "Not reported".into(), |w| number(&w.to_string())),
                    t,
                ))
                .when_some(capacity, |d, w| {
                    d.child(detail_row(
                        "Tokens remaining",
                        number(&w.saturating_sub(usage.used).to_string()),
                        t,
                    ))
                })
                .child(div().h(px(1.)).bg(t.border))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_size(px(11.))
                        .text_color(t.muted)
                        .child(format!("Updated {} ago", age(usage.reported)))
                        .child("Last reported context size."),
                );
        } else {
            details = details.child(
                div()
                    .text_color(t.muted)
                    .child("Context usage has not been reported yet."),
            );
        }
        div()
            .w(px(304.))
            .p_3()
            .font_family(crate::theme::SANS)
            .bg(t.raised)
            .border_1()
            .border_color(t.edge)
            .rounded(px(crate::theme::RADIUS_SURFACE))
            .shadow(t.floating())
            .child(details)
    }
}

struct UsageHint {
    usage: Option<UsageLimits>,
    theme: Theme,
}
impl Render for UsageHint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(304.))
            .p_3()
            .font_family(crate::theme::SANS)
            .bg(self.theme.raised)
            .border_1()
            .border_color(self.theme.edge)
            .rounded(px(crate::theme::RADIUS_SURFACE))
            .shadow(self.theme.floating())
            .child(usage_details(self.usage.as_ref(), self.theme))
            .child(
                div()
                    .mt_2()
                    .text_size(px(11.))
                    .text_color(self.theme.muted)
                    .child("Click to refresh usage or use a banked reset."),
            )
    }
}

fn usage_details(usage: Option<&UsageLimits>, t: Theme) -> AnyElement {
    let mut details = div()
        .flex()
        .flex_col()
        .gap_3()
        .p_1()
        .text_size(px(12.))
        .text_color(t.text)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Account usage"),
                )
                .when_some(usage.and_then(|u| u.plan_type.as_deref()), |d, plan| {
                    d.child(div().text_color(t.muted).child(plan_name(plan)))
                }),
        );
    let Some(usage) = usage else {
        return details
            .child(
                div()
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
            let color = usage_color(progress, t);
            details = details.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(detail_row(
                        &window_name(label, window.window_duration_mins),
                        format!("{}% remaining", 100 - used),
                        t,
                    ))
                    .child(
                        div()
                            .h(px(4.))
                            .w_full()
                            .rounded(px(1.))
                            .bg(color.opacity(0.15))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(progress))
                                    .rounded(px(1.))
                                    .bg(color),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap_2()
                            .text_size(px(11.))
                            .text_color(t.muted)
                            .child(format!("{used}% used"))
                            .child(reset_time(window.resets_at)),
                    ),
            );
        }
    }
    if usage.primary.is_none() && usage.secondary.is_none() {
        details = details.child(
            div()
                .text_color(t.muted)
                .child("Usage windows unavailable."),
        );
    }
    details = details
        .child(div().h(px(1.)).bg(t.border))
        .child(detail_row(
            "Credit balance",
            credit_balance(usage.credits.as_ref()),
            t,
        ))
        .child(detail_row(
            "Banked resets",
            usage.banked_resets.as_ref().map_or_else(
                || "Not reported".into(),
                |r| format!("{} available", r.available_count.max(0)),
            ),
            t,
        ));
    if usage.spend_control_reached == Some(true) || usage.rate_limit_reached_type.is_some() {
        details = details.child(
            div()
                .text_color(t.removed)
                .child("A usage or spending limit has been reached."),
        );
    }
    details
        .child(div().text_size(px(11.)).text_color(t.muted).child(format!(
            "Updated {} ago · Ring shows highest usage",
            age(usage.reported)
        )))
        .into_any_element()
}

fn detail_row(label: &str, value: String, t: Theme) -> AnyElement {
    div()
        .flex()
        .justify_between()
        .items_start()
        .gap_3()
        .child(
            div()
                .flex_shrink_0()
                .text_color(t.muted)
                .child(label.to_owned()),
        )
        .child(div().text_right().child(value))
        .into_any_element()
}

fn highest_usage(usage: &UsageLimits) -> Option<f32> {
    if usage.spend_control_reached == Some(true) || usage.rate_limit_reached_type.is_some() {
        return Some(1.);
    }
    [usage.primary.as_ref(), usage.secondary.as_ref()]
        .into_iter()
        .flatten()
        .map(|w| w.used_percent.clamp(0, 100) as f32 / 100.)
        .reduce(f32::max)
}

fn usage_color(fraction: f32, t: Theme) -> Hsla {
    if fraction >= 0.9 {
        t.removed
    } else {
        rgb_to_hsla(rgb(0xd7aa62))
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

fn plan_name(plan: &str) -> String {
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

fn reset_time(reset: Option<u64>) -> String {
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

// A segmented pixel level meter, filled from the bottom; unknown values show only the frame.
fn meter(value: Option<f32>, color: Hsla) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            const SEGMENTS: usize = 6;
            let width = px(6.);
            let height = f32::from(bounds.size.height);
            let pitch = height / SEGMENTS as f32;
            let left = bounds.center().x - width / 2.;
            for segment in 0..SEGMENTS {
                // Segment zero is the bottom of the meter.
                let lit = value
                    .is_some_and(|v| v.clamp(0., 1.) * SEGMENTS as f32 > segment as f32 + 0.25);
                let top = bounds.top() + px(height - (segment + 1) as f32 * pitch);
                window.paint_quad(fill(
                    Bounds::new(point(left, top), size(width, px(pitch - 0.75))),
                    if lit {
                        color
                    } else {
                        color.opacity(if value.is_some() { 0.2 } else { 0.12 })
                    },
                ));
            }
        },
    )
    .w(px(8.))
    .h(px(14.))
    .into_any_element()
}
