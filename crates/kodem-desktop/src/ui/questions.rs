//! Pending provider requests live inside the composer, separate from the prompt draft:
//! permission and plan approvals with their decisions, and questions with their answers.
use super::*;
use serde_json::json;

pub(super) struct AnswerDraft {
    input: Entity<TextInput>,
    selected: Vec<String>,
    _subscription: Subscription,
}

impl AnswerDraft {
    // Chosen options win over typed text; a typed answer is a single entry.
    fn answers(&self, cx: &App) -> Vec<String> {
        if self.selected.is_empty() {
            let text = self.input.read(cx).text();
            if text.trim().is_empty() {
                Vec::new()
            } else {
                vec![text]
            }
        } else {
            self.selected.clone()
        }
    }
}

// Key of the feedback field that belongs to an approval request.
const FEEDBACK: &str = "decline-feedback";

fn decision_label(decision: &str) -> &str {
    match decision {
        "accept" => "Allow once",
        "acceptForSession" => "Allow for session",
        "decline" => "Decline",
        "declineAlways" => "Always decline",
        "cancel" => "Stop turn",
        other => other,
    }
}

impl Shell {
    fn answer_draft(
        &mut self,
        key: (String, String),
        placeholder: &str,
        cx: &mut Context<Self>,
    ) -> &mut AnswerDraft {
        self.question_inputs.entry(key).or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new(placeholder, cx));
            let subscription = cx.observe(&input, |_, _, cx| cx.notify());
            AnswerDraft {
                input,
                selected: Vec::new(),
                _subscription: subscription,
            }
        })
    }

    /// Renders every pending approval and question above the prompt within its container.
    pub(super) fn question_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let requests: Vec<_> = self
            .view
            .as_ref()
            .map(|v| {
                v.requests
                    .iter()
                    .filter(|r| self.request_in_current_thread(r))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if requests.is_empty() {
            return div().into_any_element();
        }
        let t = self.theme;
        let mut panel = div()
            .id("composer-questions")
            .max_h(px(380.))
            .overflow_y_scroll()
            .restrict_scroll_to_axis()
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .text_color(t.text)
            .text_size(text_px(12.));
        for request in requests {
            panel = panel.child(if request.is_approval() {
                self.approval_card(&request, cx)
            } else {
                self.question_card(&request, cx)
            });
        }
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(panel)
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .into_any_element()
    }

    // A permission or plan review with the provider's own decisions.
    fn approval_card(&mut self, request: &InputRequest, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let request_key = request.id.to_string();
        let sending = self.answering.contains(&request_key);
        // A plan review without plan text points at the file the provider wrote it to.
        let plan = request.plan().map(|plan| {
            if plan.trim().is_empty() {
                request.plan_path().map_or_else(
                    || "The provider did not include the plan text.".to_owned(),
                    |path| format!("The plan is in `{path}`."),
                )
            } else {
                plan.to_owned()
            }
        });
        let decisions: Vec<String> = request.decisions();
        // The request is one block: a tinted header, what is asked, then the decisions.
        let mut card = kit::block(t)
            .flex_shrink_0()
            .border_color(t.warning.opacity(0.3))
            .child(
                kit::block_header(
                    if plan.is_some() {
                        "Plan ready for review"
                    } else {
                        "Permission requested"
                    },
                    t.warning,
                    t,
                )
                .pr(px(10.))
                .bg(t.warning.opacity(0.07))
                .border_color(t.warning.opacity(0.22))
                .when_some(request.tool(), |header, tool| {
                    header.child(kit::chip(tool.to_owned(), t.muted))
                }),
            );
        if let Some(reason) = request.reason().filter(|reason| !reason.is_empty()) {
            card = card
                .child(div().px(px(12.)).py(px(8.)).child(reason.to_owned()))
                .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)));
        }
        if let Some(plan) = plan {
            card = card.child(
                div()
                    .id(SharedString::from(format!("plan-review:{request_key}")))
                    .max_h(px(220.))
                    .overflow_y_scroll()
                    .px(px(12.))
                    .py(px(10.))
                    .child(self.message_blocks(
                        &format!("plan-review:{request_key}"),
                        crate::markdown::blocks(&plan),
                        false,
                        cx,
                    )),
            );
        } else {
            card = card.child(
                div()
                    .id(SharedString::from(format!(
                        "approval-command:{request_key}"
                    )))
                    .max_h(px(140.))
                    .overflow_y_scroll()
                    .px(px(12.))
                    .py(px(8.))
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.5))
                    .child(
                        request
                            .command()
                            .unwrap_or("File changes; inspect the matching item in the transcript.")
                            .to_owned(),
                    ),
            );
        }
        let can_decline = decisions.iter().any(|decision| decision == "decline");
        let feedback_key = (request_key.clone(), FEEDBACK.to_owned());
        if can_decline {
            let input = self
                .answer_draft(
                    feedback_key.clone(),
                    "Optional: tell the agent what to do instead, then decline",
                    cx,
                )
                .input
                .clone();
            card = card
                .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
                .child(
                    div().px(px(8.)).pt(px(8.)).child(
                        div()
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(crate::theme::RADIUS_CONTROL))
                            .border_1()
                            .border_color(t.edge)
                            .bg(t.canvas)
                            .child(input),
                    ),
                );
        } else {
            card = card.child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)));
        }
        let mut row = div()
            .p(px(8.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.));
        for decision in decisions {
            let label = request
                .decision_label(&decision)
                .unwrap_or(decision_label(&decision))
                .to_owned();
            let id = SharedString::from(format!("decision:{request_key}:{decision}"));
            let button = match decision.as_str() {
                "accept" => self.primary(id, "check", label),
                "acceptForSession" => self.button(id, "shield", label),
                "cancel" => self.tinted(id, "stop", label, t.removed),
                _ => self.ghost(id, "close", label),
            };
            let request_id = request.id.clone();
            let task_id = request.task_id.clone();
            let feedback_key = feedback_key.clone();
            row = row.child(
                button
                    .disabled(sending)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        let feedback = s
                            .question_inputs
                            .get(&feedback_key)
                            .map(|draft| draft.input.read(cx).text())
                            .filter(|text| decision == "decline" && !text.trim().is_empty());
                        let response = match feedback {
                            Some(message) => json!({"decision": decision, "message": message}),
                            None => json!({"decision": decision}),
                        };
                        s.answering.insert(request_id.to_string());
                        s.send(core::Request::Answer {
                            task_id: task_id.clone(),
                            id: request_id.clone(),
                            response,
                        });
                        cx.notify();
                    })),
            );
        }
        if sending {
            row = row.child(kit::loader_row("Sending your decision", t));
        }
        card.child(row).into_any_element()
    }

    // One request's questions, each with its options and a free-text answer.
    fn question_card(&mut self, request: &InputRequest, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(questions) = request.questions().cloned() else {
            return div().into_any_element();
        };
        let request_key = request.id.to_string();
        let sending = self.answering.contains(&request_key);
        let mut group = div().p(px(10.)).flex().flex_col().gap_3();
        let mut ready = !questions.is_empty();
        for question in &questions {
            let Some(id) = question["id"].as_str() else {
                ready = false;
                continue;
            };
            let key = (request_key.clone(), id.to_owned());
            let multiple = question["multiSelect"].as_bool().unwrap_or(false);
            let draft = self.answer_draft(key.clone(), "Type your own answer…", cx);
            let input = draft.input.clone();
            let selected = draft.selected.clone();
            ready &= !draft.answers(cx).is_empty();
            let mut field = div().flex().flex_col().gap_2().child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when_some(question["header"].as_str(), |row, header| {
                        row.child(kit::chip(header.to_uppercase(), t.accent))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(text_px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                question["question"]
                                    .as_str()
                                    .unwrap_or("Question")
                                    .to_owned(),
                            ),
                    )
                    .when(multiple, |row| row.child(kit::chip("CHOOSE ANY", t.subtle))),
            );
            let options = question["options"]
                .as_array()
                .filter(|options| !options.is_empty());
            if let Some(options) = options {
                for (index, option) in options.iter().enumerate() {
                    let Some(label) = option["label"].as_str() else {
                        continue;
                    };
                    let label = label.to_owned();
                    let chosen = selected.contains(&label);
                    let answer_key = key.clone();
                    let option_id =
                        SharedString::from(format!("question-option:{}:{id}:{index}", request_key));
                    let mut caption = div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .min_w_0()
                        .font_family(crate::theme::sans())
                        .font_weight(FontWeight::NORMAL)
                        .text_size(text_px(12.5))
                        .child(label.clone());
                    if let Some(description) =
                        option["description"].as_str().filter(|d| !d.is_empty())
                    {
                        caption = caption.child(
                            div()
                                .text_size(text_px(11.))
                                .text_color(t.muted)
                                .child(description.to_owned()),
                        );
                    }
                    field = field.child(
                        self.control(option_id)
                            .w_full()
                            .h_auto()
                            .py(px(8.))
                            .px(px(10.))
                            .gap(px(10.))
                            .items_start()
                            .justify_start()
                            .disabled(sending)
                            .overflow_hidden()
                            .bg(gpui::transparent_black())
                            .border_color(gpui::transparent_black())
                            // A chosen option is a selected row, not an outlined card.
                            .when(chosen, |b| {
                                b.bg(t.accent.opacity(0.1))
                                    .child(dither::row_wash(
                                        t.accent,
                                        crate::theme::RADIUS_CONTROL,
                                        true,
                                        false,
                                    ))
                                    .child(kit::selection_edge(t.accent))
                            })
                            .when(!chosen, |b| b.hover(move |b| b.bg(t.selected)))
                            .child(
                                div()
                                    .mt(px(1.))
                                    .size(px(12.))
                                    .flex_shrink_0()
                                    .rounded(px(if multiple { 2. } else { 6. }))
                                    .border_1()
                                    .border_color(if chosen { t.accent } else { t.edge })
                                    .p(px(2.))
                                    .when(chosen, |d| {
                                        d.child(
                                            div()
                                                .size_full()
                                                .rounded(px(if multiple { 1. } else { 4. }))
                                                .bg(t.accent),
                                        )
                                    }),
                            )
                            .child(caption)
                            .on_click(cx.listener(move |s, _, _, cx| {
                                if let Some(draft) = s.question_inputs.get_mut(&answer_key) {
                                    if !multiple {
                                        draft.selected = vec![label.clone()];
                                    } else if let Some(position) =
                                        draft.selected.iter().position(|chosen| chosen == &label)
                                    {
                                        draft.selected.remove(position);
                                    } else {
                                        draft.selected.push(label.clone());
                                    }
                                }
                                cx.notify();
                            })),
                    );
                }
                if !selected.is_empty() {
                    let answer_key = key.clone();
                    let focus_input = input.clone();
                    field = field.child(
                        div().flex().child(
                            self.ghost(
                                SharedString::from(format!("question-custom:{}:{id}", request_key)),
                                "pencil",
                                "Write my own answer",
                            )
                            .disabled(sending)
                            .on_click(cx.listener(
                                move |s, _, window, cx| {
                                    if let Some(draft) = s.question_inputs.get_mut(&answer_key) {
                                        draft.selected.clear();
                                    }
                                    window.focus(&focus_input.focus_handle(cx), cx);
                                    cx.notify();
                                },
                            )),
                        ),
                    );
                }
            }
            if selected.is_empty() {
                field = field.child(
                    div()
                        .id(SharedString::from(format!(
                            "question-input:{}:{id}",
                            request_key
                        )))
                        .max_h(px(100.))
                        .overflow_y_scroll()
                        .px(px(10.))
                        .py(px(7.))
                        .rounded(px(crate::theme::RADIUS_CONTROL))
                        .border_1()
                        .border_color(t.edge)
                        .bg(t.canvas)
                        .child(if sending {
                            div().child(input.read(cx).text()).into_any_element()
                        } else {
                            input.into_any_element()
                        }),
                );
            }
            group = group.child(field);
        }
        let task_id = request.task_id.clone();
        let id = request.id.clone();
        kit::block(t)
            .flex_shrink_0()
            .border_color(t.accent.opacity(0.3))
            .child(
                kit::block_header("Answer needed", t.accent, t)
                    .pr(px(10.))
                    .bg(t.accent.opacity(0.07))
                    .border_color(t.accent.opacity(0.22))
                    .child(kit::chip(
                        format!(
                            "{} {}",
                            questions.len(),
                            if questions.len() == 1 {
                                "QUESTION"
                            } else {
                                "QUESTIONS"
                            }
                        ),
                        t.accent,
                    )),
            )
            .child(group)
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .child(
                div().p(px(8.)).flex().justify_end().child(
                    self.primary(
                        SharedString::from(format!("send-answers:{request_key}")),
                        "arrow-up",
                        if sending {
                            "Sending answers…"
                        } else {
                            "Send answers"
                        },
                    )
                    .disabled(sending || !ready)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.submit_answers(&task_id, &id, cx);
                    })),
                ),
            )
            .into_any_element()
    }

    fn submit_answers(&mut self, task_id: &str, id: &serde_json::Value, cx: &mut Context<Self>) {
        let key = id.to_string();
        if self.answering.contains(&key) {
            return;
        }
        let Some(request) = self
            .view
            .as_ref()
            .filter(|v| v.task.id == task_id)
            .and_then(|v| v.requests.iter().find(|r| &r.id == id))
        else {
            return;
        };
        let Some(questions) = request.questions().filter(|q| !q.is_empty()) else {
            return;
        };
        let mut answers = serde_json::Map::new();
        for question in questions {
            let Some(question_id) = question["id"].as_str() else {
                return;
            };
            let Some(draft) = self
                .question_inputs
                .get(&(key.clone(), question_id.to_owned()))
            else {
                return;
            };
            let answer = draft.answers(cx);
            if answer.is_empty() {
                return;
            }
            answers.insert(question_id.to_owned(), json!({"answers": answer}));
        }
        self.answering.insert(key);
        self.send(core::Request::Answer {
            task_id: task_id.to_owned(),
            id: id.clone(),
            response: json!({"answers": answers}),
        });
        cx.notify();
    }

    /// Re-enables a rejected answer while preserving the user's choices for a retry.
    pub(super) fn answer_failed(&mut self, request: &core::Request) {
        if let core::Request::Answer { task_id, id, .. } = request
            && self.selected.as_ref() == Some(task_id)
        {
            self.answering.remove(&id.to_string());
        }
    }
}
