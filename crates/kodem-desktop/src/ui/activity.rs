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
        ActivityKind::Reasoning => ("Reasoning…", "bot", false),
        ActivityKind::Responding => ("Writing response…", "edit", false),
        ActivityKind::Command => ("Running command…", "loader", true),
        ActivityKind::EditingFiles => ("Editing files…", "file-text", false),
        ActivityKind::Searching => ("Searching the web…", "search", false),
        ActivityKind::Tool => ("Running tool…", "loader", true),
        ActivityKind::Planning => ("Planning…", "check-square", false),
        ActivityKind::Compacting => ("Compacting context…", "loader", true),
    }
}
impl Shell {
    fn has_activity_strip(&self) -> bool {
        self.view
            .as_ref()
            .is_some_and(|view| view.task.state.active() || self.submitted_draft.is_some())
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
            .px_4()
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(11.));
        let waiting = view.task.state == RunState::Waiting;
        if waiting || view.activities.is_empty() {
            let (label, glyph, spin) = if waiting {
                ("Waiting for your input", "shield", false)
            } else if self.submitted_draft.is_some() {
                ("Recording message…", "loader", true)
            } else if view.task.state == RunState::Starting {
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
                                "agent-state-icon",
                                glyph,
                                t.accent,
                                spin && !waiting,
                            )),
                    )
                    .child(motion::gleam("agent-state-label", label, t))
                    .when(waiting, |d| {
                        d.child(div().flex_1()).child(
                            self.icon_button("activity-respond", "arrow-right", "Respond to agent")
                                .size(px(20.))
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.panel = Panel::Questions;
                                    cx.notify();
                                })),
                        )
                    }),
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
                                    (
                                        ElementId::from("activity-icon"),
                                        SharedString::from(activity.id.clone()),
                                    ),
                                    glyph,
                                    t.accent,
                                    spin,
                                )),
                        )
                        .child(motion::gleam(
                            (
                                ElementId::from("activity-label"),
                                SharedString::from(activity.id.clone()),
                            ),
                            label,
                            t,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_color(t.subtle)
                                .child(activity.detail.clone()),
                        )
                        .child(div().text_size(px(10.)).text_color(t.subtle).child(format!(
                            "{}:{:02}",
                            seconds / 60,
                            seconds % 60
                        ))),
                );
            }
        }
        div()
            .w_full()
            .max_h(px(104.))
            .id("agent-activities")
            .rounded_t(px(motion::COMPOSER_RADIUS - 1.))
            .bg(t.raised)
            .border_b_1()
            .border_color(t.border)
            .overflow_y_scroll()
            .flex_shrink_0()
            .flex()
            .justify_center()
            .child(rows)
            .into_any_element()
    }
    /// Shows live task-baseline totals as a connected section of the prompt.
    pub(super) fn changed_files_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(summary) = self
            .task_changes
            .as_ref()
            .filter(|summary| summary.files > 0)
        else {
            return div().into_any_element();
        };
        let t = self.theme;
        let scope = if self
            .view
            .as_ref()
            .is_some_and(|view| view.task.baseline.is_some())
        {
            "Changes since this task's baseline."
        } else {
            "Current working-tree changes."
        };
        let hint = if summary.omitted_files > 0 {
            format!(
                "{scope} Line totals exclude {} binary, large, or metadata-only files.",
                summary.omitted_files
            )
        } else {
            format!("{scope} Click to review.")
        };
        div()
            .w_full()
            .when(!self.has_activity_strip(), |d| {
                d.rounded_t(px(motion::COMPOSER_RADIUS - 1.))
            })
            .bg(t.raised)
            .border_b_1()
            .border_color(t.border)
            .px_4()
            .py_2()
            .child(
                self.ghost(
                    "composer-changes",
                    "file-text",
                    format!(
                        "{} Changed {}",
                        summary.files,
                        if summary.files == 1 { "File" } else { "Files" }
                    ),
                )
                .w_full()
                .h(px(20.))
                .px_0()
                .gap_2()
                .justify_start()
                .text_size(px(11.))
                .when(summary.omitted_files > 0, |button| button.child("~"))
                .child(
                    div()
                        .text_color(t.added)
                        .child(format!("+{}", summary.added)),
                )
                .child(
                    div()
                        .text_color(t.removed)
                        .child(format!("−{}", summary.removed)),
                )
                .child(div().flex_1())
                .child(icon("chevron-right").size(px(12.)))
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: hint.clone(),
                        theme: t,
                    })
                    .into()
                })
                .on_click(cx.listener(|s, _, _, cx| {
                    s.show_review = true;
                    s.comparison = if s.view.as_ref().is_some_and(|v| v.task.baseline.is_some()) {
                        Comparison::Baseline
                    } else {
                        Comparison::WorkingTree
                    };
                    s.historical_diff = false;
                    s.reviewing_file = false;
                    s.diff = None;
                    s.new_diff = None;
                    s.diff_view = None;
                    s.save_settings();
                    s.refresh_diff();
                    cx.notify();
                })),
            )
            .into_any_element()
    }
}
