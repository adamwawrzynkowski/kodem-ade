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

    /// Names what the agent of a running task waits for before it continues, one item a line:
    /// subagents that still work after its turn ended, or waits that are its only activities.
    /// A request for the user's input is not a wait of this kind; it has its own surfaces.
    pub(super) fn waiting_for(&self) -> Option<String> {
        let view = self.view.as_ref()?;
        let task = &view.task;
        if !task.state.active() || task.state == RunState::Waiting {
            return None;
        }
        let held = task.pending_completion.is_some();
        let waits = !view.activities.is_empty()
            && view.activities.iter().all(|activity| {
                matches!(
                    activity.kind,
                    ActivityKind::Delegating | ActivityKind::WaitingTimer
                )
            });
        if !held && !waits {
            return None;
        }
        let agents: Vec<&str> = task
            .subagents
            .iter()
            .filter(|agent| agent.state.active())
            .map(|agent| agent.name.as_str())
            .collect();
        let mut lines = Vec::new();
        if !agents.is_empty() {
            lines.push(if agents.len() == 1 {
                "Waiting for 1 subagent".to_owned()
            } else {
                format!("Waiting for {} subagents", agents.len())
            });
            lines.extend(agents.iter().map(|name| format!("· {name}")));
        }
        for activity in &view.activities {
            let line = match activity.kind {
                ActivityKind::WaitingTimer if activity.detail.is_empty() => {
                    "Waiting for a timer".to_owned()
                }
                ActivityKind::WaitingTimer => format!("Waiting for a timer · {}", activity.detail),
                // The subagents above already name the delegation.
                ActivityKind::Delegating if !agents.is_empty() => continue,
                ActivityKind::Delegating if activity.detail.is_empty() => {
                    "Waiting for an agent".to_owned()
                }
                ActivityKind::Delegating => format!("Waiting for an agent · {}", activity.detail),
                _ => continue,
            };
            lines.push(line);
        }
        if lines.is_empty() {
            lines.push("Waiting for background work to finish".to_owned());
        }
        Some(lines.join("\n"))
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
            .text_size(text_px(12.));
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
                                .font_family(crate::theme::mono())
                                .text_size(text_px(11.))
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
        // scanner then follows the inside of the composer's rounded leading corner, which the
        // plan tab squares.
        let scanner_radii = Corners {
            top_left: if view.task.subagents.is_empty() && self.shown_plan().is_none() {
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
        self.show_pane(PanePage::Changes);
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

    /// Returns the agent's plan while the composer shows it: while the task works, and after
    /// that while steps are left unfinished. A finished plan of an idle task is history.
    pub(super) fn shown_plan(&self) -> Option<&[PlanStep]> {
        let view = self.view.as_ref()?;
        let plan = &view.task.plan;
        let unfinished = plan.iter().any(|step| step.status != PlanStatus::Done);
        (!plan.is_empty() && (view.task.state.active() || unfinished)).then_some(plan.as_slice())
    }

    /// Shows the agent's plan in one line: totals, a progress meter and the current step. It
    /// opens every step above it. The caller places the control and paints its surface.
    pub(super) fn plan_tab(&self, cx: &mut Context<Self>) -> Option<Button> {
        let plan = self.shown_plan()?;
        let t = self.theme;
        let progress = PlanProgress::of(plan);
        let working = self
            .view
            .as_ref()
            .is_some_and(|view| view.task.state.active());
        let current = progress.current.map(|index| plan[index].clone());
        let hint = current.as_ref().map_or_else(
            || "Every step of the plan is done.".to_owned(),
            |step| step.text.clone(),
        );
        Some(
            Button::new("plan-tab")
                .flex()
                .items_center()
                .gap(px(6.))
                .min_w_0()
                .overflow_hidden()
                .cursor_pointer()
                .selected(self.panel == Panel::Plan)
                .accessibility_label("Plan steps")
                .child(caps("Plan", t))
                .child(figure(format!("{}/{}", progress.done, plan.len()), t.muted))
                .child(kit::meter(
                    Some(progress.fraction()),
                    progress.tone(t),
                    30.,
                    t,
                ))
                .when_some(current, |tab, step| {
                    tab.child(div().w(px(2.)).flex_shrink_0())
                        .when(step.status == PlanStatus::Active, |tab| {
                            tab.child(dither::pulse(t.accent, cx.reduce_motion() || !working))
                        })
                        .child(
                            div()
                                .flex_shrink(1.)
                                .min_w_0()
                                .text_ellipsis()
                                .font_family(crate::theme::sans())
                                .text_size(text_px(12.))
                                .text_color(t.muted)
                                .child(step.text),
                        )
                })
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: hint.clone(),
                        theme: t,
                    })
                    .into()
                })
                .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                    s.panel = Panel::Plan;
                    s.menu_anchor = Some(*bounds);
                    cx.notify();
                })),
        )
    }

    /// Lists every step of the agent's plan with its state, for the menu above the plan tab.
    pub(super) fn plan_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let plan = self
            .view
            .as_ref()
            .map_or(&[][..], |view| view.task.plan.as_slice());
        let progress = PlanProgress::of(plan);
        let working = self
            .view
            .as_ref()
            .is_some_and(|view| view.task.state.active());
        let header = div()
            .px(px(8.))
            .pt(px(6.))
            .pb(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(caps("Plan", t))
            .child(figure(format!("{}/{}", progress.done, plan.len()), t.muted))
            .child(kit::meter(
                Some(progress.fraction()),
                progress.tone(t),
                48.,
                t,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(8.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.45))),
            );
        if plan.is_empty() {
            return div()
                .flex()
                .flex_col()
                .child(header)
                .child(
                    div()
                        .px(px(8.))
                        .pb(px(8.))
                        .text_size(text_px(12.))
                        .text_color(t.muted)
                        .child("The agent has no plan."),
                )
                .into_any_element();
        }
        // The list scrolls inside its own bound, so a long plan never grows past it.
        let mut steps = div()
            .id("plan-steps")
            .max_h(px(PLAN_LIST_HEIGHT))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(1.));
        // Rows keep their height and the list scrolls. A row that clips its overflow has no
        // content minimum, so without this the current row alone absorbs the overflow and
        // collapses under the next row.
        for (index, step) in plan.iter().enumerate() {
            let current = progress.current == Some(index);
            let marker = match step.status {
                PlanStatus::Done => icon("check")
                    .size(px(12.))
                    .text_color(t.added)
                    .into_any_element(),
                PlanStatus::Active => dither::pulse(t.accent, cx.reduce_motion() || !working),
                PlanStatus::Pending => div()
                    .size(px(9.))
                    .rounded(px(2.))
                    .border_1()
                    .border_color(t.subtle)
                    .into_any_element(),
            };
            steps = steps.child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .px(px(8.))
                    .py(px(5.))
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .text_size(text_px(12.))
                    .line_height(text_px(17.))
                    .when(current, |row| {
                        row.overflow_hidden()
                            .bg(t.accent.opacity(0.1))
                            .layer(dither::row_wash(t.accent, true, false))
                            .child(kit::selection_edge(t.accent))
                    })
                    .child(
                        div()
                            .w(px(18.))
                            .flex_shrink_0()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(11.))
                            .text_color(t.subtle)
                            .child(format!("{:02}", index + 1)),
                    )
                    .child(
                        div()
                            .size(px(17.))
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
                            .text_color(match step.status {
                                PlanStatus::Done => t.subtle,
                                PlanStatus::Active => t.text,
                                PlanStatus::Pending => t.muted,
                            })
                            .when(step.status == PlanStatus::Done, |d| d.line_through())
                            .child(step.text.clone()),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .pb(px(2.))
            .child(header)
            .child(steps)
            .into_any_element()
    }
}

/// Height at which the plan menu's step list stops growing and scrolls.
const PLAN_LIST_HEIGHT: f32 = 320.;

/// How far the agent is through its plan.
struct PlanProgress {
    done: usize,
    total: usize,
    /// The step in progress, or else the first one still pending.
    current: Option<usize>,
}

impl PlanProgress {
    fn of(plan: &[PlanStep]) -> Self {
        let position = |status| plan.iter().position(|step| step.status == status);
        Self {
            done: plan
                .iter()
                .filter(|step| step.status == PlanStatus::Done)
                .count(),
            total: plan.len(),
            current: position(PlanStatus::Active).or_else(|| position(PlanStatus::Pending)),
        }
    }

    fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.
        } else {
            self.done as f32 / self.total as f32
        }
    }

    // The meter turns to the addition tone once every step is done.
    fn tone(&self, t: Theme) -> Hsla {
        if self.total > 0 && self.done == self.total {
            t.added
        } else {
            t.accent
        }
    }
}
