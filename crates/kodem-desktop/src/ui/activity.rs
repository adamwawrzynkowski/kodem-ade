use super::*;

fn presentation(kind: ActivityKind) -> (&'static str, &'static str, bool) {
    match kind {
        ActivityKind::Reconnecting => ("Reconnecting…", "refresh", true),
        ActivityKind::ReadingFiles => ("Reading files…", "file-text", false),
        ActivityKind::SearchingFiles => ("Searching files…", "search", false),
        ActivityKind::Delegating => ("Coordinating agents…", "bot", false),
        ActivityKind::Reviewing => ("Reviewing changes…", "check-square", false),
        ActivityKind::ViewingImage => ("Inspecting image…", "file", false),
        ActivityKind::GeneratingImage => ("Generating image…", "loader", true),
        ActivityKind::WaitingTimer => ("Waiting…", "loader", false),
        ActivityKind::Reasoning => ("Reasoning…", "sparkles", false),
        ActivityKind::Responding => ("Writing response…", "edit", false),
        ActivityKind::Command => ("Running command…", "loader", true),
        ActivityKind::EditingFiles => ("Editing files…", "file-text", false),
        ActivityKind::Searching => ("Searching the web…", "globe", false),
        ActivityKind::Tool => ("Running tool…", "loader", true),
        ActivityKind::Planning => ("Planning…", "list-checks", false),
        ActivityKind::Compacting => ("Compacting context…", "loader", true),
    }
}
impl Shell {
    fn has_activity_strip(&self) -> bool {
        self.view.as_ref().is_some_and(|view| {
            view.task.state.active()
                || view.starting_submission().is_some()
                || self.submitted_draft.is_some()
        })
    }

    /// Shows every provider-reported activity, or the task's accurate fallback state.
    pub(super) fn activity_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = &self.view else {
            return div().into_any_element();
        };
        if !self.has_activity_strip() {
            return div().into_any_element();
        }
        let t = self.theme;
        let mut rows = div()
            .w_full()
            .px(px(14.))
            .pt(px(9.))
            .pb(px(7.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .text_size(px(12.));
        let waiting = view.task.state == RunState::Waiting;
        if waiting || view.activities.is_empty() {
            let (label, glyph, spin) = if waiting {
                ("Waiting for your input", "shield", false)
            } else if view.task.pending_completion.is_some() {
                ("Subagents working…", "loader", true)
            } else if self.submitted_draft.is_some() {
                ("Recording message…", "loader", true)
            } else if view.task.state == RunState::Starting || view.starting_submission().is_some()
            {
                ("Starting agent…", "loader", true)
            } else {
                ("Agent working…", "loader", true)
            };
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .min_h(px(20.))
                    .gap_2()
                    .child(
                        div()
                            .size(px(16.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(motion::active_icon(
                                glyph,
                                t.accent,
                                spin && !waiting,
                                cx.reduce_motion(),
                            )),
                    )
                    .child(motion::gleam(label, t, cx.reduce_motion())),
            );
        } else {
            for activity in &view.activities {
                let (label, glyph, spin) = presentation(activity.kind);
                let seconds = now().saturating_sub(activity.started) / 1000;
                rows = rows.child(
                    div()
                        .flex()
                        .items_center()
                        .min_h(px(20.))
                        .gap_2()
                        .child(
                            div()
                                .size(px(16.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(motion::active_icon(
                                    glyph,
                                    t.accent,
                                    spin,
                                    cx.reduce_motion(),
                                )),
                        )
                        .child(motion::gleam(label, t, cx.reduce_motion()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .font_family(crate::theme::MONO)
                                .text_size(px(11.))
                                .text_color(t.subtle)
                                .child(activity.detail.clone()),
                        )
                        .child(figure(
                            format!("{}:{:02}", seconds / 60, seconds % 60),
                            t.subtle,
                        )),
                );
            }
        }
        // The strip is the composer's first row unless the subagents strip is above it. The
        // scanner then follows the inside of the composer's rounded leading corner.
        let scanner_radii = Corners {
            top_left: if view.task.subagents.is_empty() {
                motion::COMPOSER_RADIUS - 1.
            } else {
                0.
            },
            ..Corners::default()
        };
        div()
            .relative()
            .w_full()
            .flex_shrink_0()
            .bg(t.inset)
            .when(!waiting, |d| {
                d.child(dither::scanner(t.accent, scanner_radii))
            })
            .child(
                div()
                    .w_full()
                    .max_h(px(104.))
                    .id("agent-activities")
                    .overflow_y_scroll()
                    .flex()
                    .justify_center()
                    .child(rows),
            )
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .into_any_element()
    }
    /// Opens the review pane on the task's changes since its baseline.
    pub(super) fn open_task_changes(&mut self, cx: &mut Context<Self>) {
        self.show_review = true;
        self.comparison = if self
            .view
            .as_ref()
            .is_some_and(|v| v.task.baseline.is_some())
        {
            Comparison::Baseline
        } else {
            Comparison::WorkingTree
        };
        self.historical_diff = false;
        self.reviewing_file = false;
        self.diff = None;
        self.new_diff = None;
        self.diff_view = None;
        self.save_settings();
        self.refresh_diff();
        cx.notify();
    }

    /// Shows the agent's current step list as a progress meter that expands into the steps.
    pub(super) fn plan_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = self.view.as_ref().filter(|view| !view.task.plan.is_empty()) else {
            return div().into_any_element();
        };
        let t = self.theme;
        let plan = &view.task.plan;
        let done = plan
            .iter()
            .filter(|step| step.status == PlanStatus::Done)
            .count();
        let current = plan
            .iter()
            .find(|step| step.status == PlanStatus::Active)
            .or_else(|| plan.iter().find(|step| step.status == PlanStatus::Pending));
        let expanded = self.plan_expanded;
        let mut strip =
            div().w_full().flex_shrink_0().bg(t.inset).child(
                self.control("plan-toggle")
                    .w_full()
                    .h(px(30.))
                    .px(px(14.))
                    .gap(px(8.))
                    .rounded_none()
                    .justify_start()
                    .border_0()
                    .bg(gpui::transparent_black())
                    .hover(move |s| s.bg(t.selected))
                    .accessibility_label(if expanded {
                        "Collapse plan"
                    } else {
                        "Expand plan"
                    })
                    // The strip reads as a section header: chevron, label, totals, dotted rule.
                    .child(
                        icon(if expanded {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        })
                        .size(px(12.))
                        .text_color(t.subtle),
                    )
                    .child(caps("Plan", t).text_color(t.muted))
                    .child(figure(format!("{done}/{}", plan.len()), t.muted))
                    .child(div().relative().w(px(72.)).h(px(6.)).flex_shrink_0().child(
                        dither::DitherBar::meter(
                            done as f32 / plan.len() as f32,
                            if done == plan.len() {
                                t.added
                            } else {
                                t.accent
                            },
                            t.subtle.opacity(0.55),
                        ),
                    ))
                    .child(
                        div()
                            .flex_shrink(1.)
                            .min_w_0()
                            .text_ellipsis()
                            .font_family(crate::theme::SANS)
                            .font_weight(FontWeight::NORMAL)
                            .text_size(px(12.))
                            .text_color(t.muted)
                            .child(current.map_or_else(
                                || "All steps done".to_owned(),
                                |step| step.text.clone(),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(8.))
                            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                    )
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.plan_expanded = !s.plan_expanded;
                        cx.notify();
                    })),
            );
        if expanded {
            let mut steps = div()
                .id("plan-steps")
                .max_h(px(180.))
                .overflow_y_scroll()
                .px(px(14.))
                .pb(px(8.))
                .flex()
                .flex_col()
                .gap(px(4.));
            for step in plan {
                let (marker, color) = match step.status {
                    PlanStatus::Done => (
                        icon("check")
                            .size(px(12.))
                            .text_color(t.added)
                            .into_any_element(),
                        t.subtle,
                    ),
                    PlanStatus::Active => (dither::pulse(t.accent, cx.reduce_motion()), t.text),
                    PlanStatus::Pending => (
                        div()
                            .size(px(8.))
                            .rounded(px(2.))
                            .border_1()
                            .border_color(t.subtle)
                            .into_any_element(),
                        t.muted,
                    ),
                };
                steps = steps.child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(8.))
                        .text_size(px(12.))
                        .child(
                            div()
                                .size(px(16.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(marker),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(color)
                                .when(step.status == PlanStatus::Done, |d| d.line_through())
                                .child(step.text.clone()),
                        ),
                );
            }
            strip = strip.child(steps);
        }
        strip
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .into_any_element()
    }
}
