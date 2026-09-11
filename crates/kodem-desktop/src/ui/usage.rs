//! Account usage details and provider-backed reset actions beside the composer.
use super::*;

#[derive(Default)]
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
                    .size(px(18.))
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
                    .child(meter(fraction, rgb_to_hsla(rgb(0x55bdd7)), false))
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
                    .size(px(24.))
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
                    .child(meter(highest, usage_color(highest.unwrap_or(0.), t), true))
                    .tooltip(move |_, cx| {
                        cx.new(|_| UsageHint {
                            usage: usage.clone(),
                            theme: t,
                        })
                        .into()
                    })
                    .on_click(cx.listener(|s, event: &ClickEvent, _, cx| {
                        s.panel = Panel::Usage;
                        s.composer_menu_anchor = Some(event.position());
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn account_usage(&self) -> Option<&UsageLimits> {
        self.workspace
            .as_ref()
            .and_then(|w| w.runtime.usage.as_ref())
    }

    /// Builds the account details and explicit refresh and reset actions.
    pub(super) fn usage_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let available = self
            .account_usage()
            .and_then(|u| u.banked_resets.as_ref())
            .is_some_and(|r| r.available_count > 0);
        let retry = self.usage_actions.retry_key.is_some();
        let pending = self.usage_actions.pending;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(usage_details(self.account_usage(), t))
            .child(div().h(px(1.)).bg(t.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        self.ghost(
                            "refresh-account-usage",
                            "refresh",
                            if pending { "Working…" } else { "Refresh" },
                        )
                        .disabled(pending)
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.usage_actions.pending = true;
                            s.usage_actions.message = None;
                            s.send(core::Request::RefreshUsage);
                            cx.notify();
                        })),
                    )
                    .child(
                        self.control("consume-usage-reset")
                            .child(if retry {
                                "Retry reset"
                            } else {
                                "Use one reset"
                            })
                            .disabled(pending || (!available && !retry))
                            .on_click(cx.listener(|s, _, _, cx| {
                                if s.usage_actions.pending {
                                    return;
                                }
                                let key = s
                                    .usage_actions
                                    .retry_key
                                    .get_or_insert_with(core::new_id)
                                    .clone();
                                s.usage_actions.pending = true;
                                s.usage_actions.message = None;
                                s.send(core::Request::ConsumeUsageReset {
                                    idempotency_key: key,
                                });
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .child("Uses one banked reset on eligible usage limits."),
            )
            .when_some(self.usage_actions.message.clone(), |d, message| {
                d.child(div().text_size(px(12.)).text_color(t.text).child(message))
            })
            .into_any_element()
    }

    /// Applies account responses while preserving the redemption key after uncertain failures.
    pub(super) fn apply_usage_reply(
        &mut self,
        result: Result<Response, String>,
        cx: &mut Context<Self>,
    ) {
        self.usage_actions.pending = false;
        match result {
            Ok(Response::Usage(usage)) => {
                if let Some(workspace) = &mut self.workspace {
                    workspace.runtime.usage = Some(usage);
                }
                self.usage_actions.message = Some("Usage updated.".into());
            }
            Ok(Response::UsageReset {
                outcome,
                usage,
                refresh_error,
            }) => {
                self.usage_actions.retry_key = None;
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
                        workspace.runtime.usage = Some(usage);
                    } else if let Some(usage) = &mut workspace.runtime.usage {
                        // The old inventory is no longer safe to use for another redemption.
                        usage.banked_resets = None;
                    }
                }
                self.usage_actions.message = Some(match refresh_error {
                    Some(error) => {
                        format!("{message} Could not refresh usage: {error}. Press Refresh.")
                    }
                    None => message.into(),
                });
            }
            Err(error) | Ok(Response::Error(error)) => {
                self.usage_actions.message = Some(if self.usage_actions.retry_key.is_some() {
                    format!(
                        "{error}\nRetry reset checks the same attempt without using a second reset."
                    )
                } else {
                    error
                });
            }
            _ => {
                self.usage_actions.message =
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
                        .rounded(px(2.))
                        .bg(color.opacity(0.15))
                        .child(
                            div()
                                .h_full()
                                .w(relative(value.clamp(0., 1.) as f32))
                                .rounded(px(2.))
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
            .font_family("IBM Plex Sans")
            .bg(t.raised)
            .border_1()
            .border_color(t.border)
            .rounded(px(8.))
            .shadow(t.elevation())
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
            .font_family("IBM Plex Sans")
            .bg(self.theme.raised)
            .border_1()
            .border_color(self.theme.border)
            .rounded(px(8.))
            .shadow(self.theme.elevation())
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
                            .rounded(px(2.))
                            .bg(color.opacity(0.15))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(progress))
                                    .rounded(px(2.))
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

fn meter(value: Option<f32>, color: Hsla, ring: bool) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = bounds.center();
            let radius = px(5.3);
            if let Some(path) = radial_path(center, radius, 1., ring) {
                window.paint_path(path, color.opacity(0.18));
            }
            if let Some(progress) = value {
                if let Some(path) = radial_path(center, radius, progress, ring) {
                    window.paint_path(path, color);
                }
            } else {
                let dot = Bounds::new(center - point(px(0.8), px(0.8)), size(px(1.6), px(1.6)));
                window.paint_quad(fill(dot, color.opacity(0.5)).corner_radii(px(0.8)));
            }
        },
    )
    .size(px(14.))
    .into_any_element()
}

// Clockwise from twelve o'clock. Two half arcs also make a full circle well-defined.
fn radial_path(
    center: Point<Pixels>,
    radius: Pixels,
    progress: f32,
    ring: bool,
) -> Option<gpui::Path<Pixels>> {
    let progress = progress.clamp(0., 1.);
    if progress <= 0. {
        return None;
    }
    let mut path = if ring {
        PathBuilder::stroke(px(1.8))
    } else {
        PathBuilder::fill()
    };
    let start = point(center.x, center.y - radius);
    if ring {
        path.move_to(start);
    } else {
        path.move_to(center);
        path.line_to(start);
    }
    let pieces = (progress * 2.).ceil() as usize;
    for part in 1..=pieces {
        let angle = -std::f32::consts::FRAC_PI_2
            + std::f32::consts::TAU * progress * part as f32 / pieces as f32;
        let end = point(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        );
        path.arc_to(point(radius, radius), px(0.), false, true, end);
    }
    if !ring || progress == 1. {
        path.close();
    }
    path.build().ok()
}
