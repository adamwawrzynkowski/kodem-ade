use super::*;

fn presentation(kind: ActivityKind) -> (&'static str, &'static str, bool) {
    match kind {
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
    /// Shows every provider-reported activity, or the task's accurate fallback state.
    pub(super) fn activity_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = &self.view else {
            return div().into_any_element();
        };
        if !view.task.state.active() && self.submitted_draft.is_none() {
            return div().into_any_element();
        }
        let t = self.theme;
        let mut rows = div()
            .w_full()
            .max_w(px(940.))
            .mx_auto()
            .px_6()
            .py_1()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(11.));
        let waiting = view.task.state == RunState::Waiting;
        if waiting || view.activities.is_empty() {
            let (label, glyph, spin) = if waiting {
                ("Waiting for your input", "shield", false)
            } else if view.task.state == RunState::Starting || self.submitted_draft.is_some() {
                ("Starting agent…", "loader", true)
            } else {
                ("Agent working…", "loader", true)
            };
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(motion::active_icon(
                        "agent-state-icon",
                        glyph,
                        t.accent,
                        spin && !waiting,
                    ))
                    .child(motion::gleam("agent-state-label", label, t))
                    .when(waiting, |d| {
                        d.child(div().flex_1()).child(
                            self.icon_button("activity-respond", "arrow-right", "Respond to agent")
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
                        .gap_2()
                        .child(motion::active_icon(
                            (
                                ElementId::from("activity-icon"),
                                SharedString::from(activity.id.clone()),
                            ),
                            glyph,
                            t.accent,
                            spin,
                        ))
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
            .overflow_y_scroll()
            .flex_shrink_0()
            .flex()
            .justify_center()
            .child(rows)
            .into_any_element()
    }
}
