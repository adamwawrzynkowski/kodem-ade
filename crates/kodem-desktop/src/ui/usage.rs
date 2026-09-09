//! Minimal occupancy meters whose details stay in hover and accessibility labels.
use super::*;
impl Shell {
    /// Shows the last reported context occupancy and account usage without estimating missing data.
    pub(super) fn usage_meters(&self) -> AnyElement {
        let t = self.theme;
        let context = self
            .view
            .as_ref()
            .and_then(|v| v.task.context_usage.as_ref());
        let fraction = context.and_then(|u| u.window.map(|w| u.used as f32 / w.max(1) as f32));
        let context_hint = context.map_or_else(|| "Context usage\nNot reported by the provider yet.".into(), |u| {
            let limit = u.window.map_or_else(|| "Capacity unavailable".into(), |w| format!("{} of {} tokens · {:.1}% used",u.used,w,u.used as f64/w.max(1) as f64*100.));
            format!("Context usage\n{limit}\nLast provider report {} ago. This is the last context size, not cumulative usage.",age(u.reported))
        });
        let usage = self
            .workspace
            .as_ref()
            .and_then(|w| w.runtime.usage.as_ref());
        let mut highest = None::<f32>;
        let mut hint = "Account usage limits".to_owned();
        if let Some(u) = usage {
            if let Some(plan) = &u.plan_type {
                hint.push_str(&format!("\nPlan: {plan}"));
            }
            for (label, window) in [
                ("Primary", u.primary.as_ref()),
                ("Secondary", u.secondary.as_ref()),
            ] {
                if let Some(window) = window {
                    let used = window.used_percent.clamp(0, 100) as f32;
                    highest = Some(highest.unwrap_or(0.).max(used / 100.));
                    let duration = window.window_duration_mins.map_or_else(
                        || label.into(),
                        |m| {
                            if m % 1440 == 0 {
                                format!("{} day window", m / 1440)
                            } else if m % 60 == 0 {
                                format!("{} hour window", m / 60)
                            } else {
                                format!("{m} minute window")
                            }
                        },
                    );
                    hint.push_str(&format!(
                        "\n{duration}: {}% used · {}% remaining",
                        used as u32,
                        100 - used as u32
                    ));
                    if let Some(reset) = window.resets_at {
                        let seconds = reset.saturating_sub(now() / 1000);
                        hint.push_str(&if seconds == 0 {
                            "\nReset time passed; awaiting a new provider report.".into()
                        } else {
                            format!("\nResets in {}h {}m", seconds / 3600, (seconds % 3600) / 60)
                        });
                    }
                }
            }
            if u.spend_control_reached == Some(true) || u.rate_limit_reached_type.is_some() {
                hint.push_str("\nProvider reports a usage or spending limit reached.");
                highest = Some(1.);
            }
            if highest.is_none() {
                hint.push_str("\nUsage windows unavailable for this account.");
            }
            hint.push_str(&format!(
                "\nLast provider report {} ago. Ring shows the fullest window.",
                age(u.reported)
            ));
        } else {
            hint.push_str("\nNot reported by the provider yet. Limits refresh automatically.");
        }
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_1()
            .child(meter(
                "context-meter",
                fraction,
                context_hint,
                rgb_to_hsla(rgb(0x55bdd7)),
                false,
                t,
            ))
            .child(meter(
                "limit-meter",
                highest,
                hint,
                if highest.is_some_and(|v| v >= 0.9) {
                    t.removed
                } else {
                    rgb_to_hsla(rgb(0xd7aa62))
                },
                true,
                t,
            ))
            .into_any_element()
    }
}
fn meter(
    id: &'static str,
    value: Option<f32>,
    hint: String,
    color: Hsla,
    ring: bool,
    t: Theme,
) -> AnyElement {
    div()
        .id(id)
        .size(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_default()
        .aria_label(hint.clone())
        .role(Role::Meter)
        .when_some(value, |d, v| {
            d.aria_numeric_value((v.clamp(0., 1.) * 100.) as f64)
                .aria_min_numeric_value(0.)
                .aria_max_numeric_value(100.)
        })
        .child(
            div()
                .relative()
                .size(px(13.))
                .rounded_full()
                .overflow_hidden()
                .border_1()
                .border_color(color.opacity(if value.is_some() { 0.7 } else { 0.4 }))
                .bg(color.opacity(0.1))
                .when_some(value, |d, v| {
                    d.child(
                        div()
                            .absolute()
                            .bottom_0()
                            .w_full()
                            .h(relative(v.clamp(0., 1.)))
                            .bg(color),
                    )
                })
                .when(ring, |d| {
                    d.child(div().absolute().inset(px(3.)).rounded_full().bg(t.canvas))
                })
                .when(value.is_none(), |d| {
                    d.child(div().absolute().inset(px(4.)).rounded_full().bg(t.subtle))
                }),
        )
        .tooltip(move |_, cx| {
            cx.new(|_| Hint {
                text: hint.clone(),
                theme: t,
            })
            .into()
        })
        .into_any_element()
}
fn age(reported: u64) -> String {
    let seconds = now().saturating_sub(reported) / 1000;
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m", seconds / 60)
    }
}
