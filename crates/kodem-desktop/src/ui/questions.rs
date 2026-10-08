//! Pending provider questions live inside the composer, separate from the prompt draft.
use super::*;

pub(super) struct AnswerDraft {
    input: Entity<TextInput>,
    selected: Option<String>,
    _subscription: Subscription,
}

impl AnswerDraft {
    fn answer(&self, cx: &App) -> String {
        self.selected
            .clone()
            .unwrap_or_else(|| self.input.read(cx).text())
    }
}

impl Shell {
    /// Renders all pending question groups above the prompt within its existing container.
    pub(super) fn question_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let requests: Vec<_> = self
            .view
            .as_ref()
            .map(|v| {
                v.requests
                    .iter()
                    .filter(|r| self.request_in_current_thread(r))
                    .filter(|r| r.method == "item/tool/requestUserInput" || r.is_async())
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
            .max_h(px(300.))
            .overflow_y_scroll()
            .restrict_scroll_to_axis()
            .px(px(14.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .border_b_1()
            .border_color(t.border)
            .bg(t.inset)
            .text_color(t.text)
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.))
                    .child(div().size(px(6.)).bg(t.accent))
                    .child(caps("Answer needed", t).text_color(t.accent))
                    .child(div().flex_1().h(px(1.)).bg(t.border)),
            );
        for request in requests {
            let Some(questions) = request.params["questions"].as_array() else {
                continue;
            };
            let request_key = request.id.to_string();
            let sending = self.answering.contains(&request_key);
            let mut group = div().flex().flex_col().flex_shrink_0().gap_3();
            let mut ready = !questions.is_empty();
            for question in questions {
                let Some(id) = question["id"].as_str() else {
                    ready = false;
                    continue;
                };
                let key = (request_key.clone(), id.to_owned());
                let draft = self.question_inputs.entry(key.clone()).or_insert_with(|| {
                    let input = cx.new(|cx| TextInput::new("Type your own answer…", cx));
                    let subscription = cx.observe(&input, |_, _, cx| cx.notify());
                    AnswerDraft {
                        input,
                        selected: None,
                        _subscription: subscription,
                    }
                });
                let input = draft.input.clone();
                let selected = draft.selected.clone();
                ready &= !draft.answer(cx).trim().is_empty();
                let mut field = div().flex().flex_col().gap_2().child(
                    div().font_weight(FontWeight::SEMIBOLD).child(
                        question["question"]
                            .as_str()
                            .unwrap_or("Question")
                            .to_owned(),
                    ),
                );
                if let Some(options) = question["options"].as_array().filter(|o| !o.is_empty()) {
                    for (index, option) in options.iter().enumerate() {
                        let Some(label) = option["label"].as_str() else {
                            continue;
                        };
                        let label = label.to_owned();
                        let chosen = selected.as_ref() == Some(&label);
                        let answer_key = key.clone();
                        let option_id = SharedString::from(format!(
                            "question-option:{}:{id}:{index}",
                            request_key
                        ));
                        let mut caption = div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .min_w_0()
                            .child(label.clone());
                        if let Some(description) =
                            option["description"].as_str().filter(|d| !d.is_empty())
                        {
                            caption = caption.child(
                                div()
                                    .text_size(px(11.))
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
                                .bg(if chosen { t.accent_bg } else { t.raised })
                                .border_color(if chosen {
                                    t.accent.opacity(0.7)
                                } else {
                                    t.border
                                })
                                .hover(move |b| b.border_color(t.edge))
                                .child(
                                    div()
                                        .mt(px(1.))
                                        .size(px(12.))
                                        .flex_shrink_0()
                                        .rounded(px(2.))
                                        .border_1()
                                        .border_color(if chosen { t.accent } else { t.edge })
                                        .p(px(2.))
                                        .when(chosen, |d| d.child(div().size_full().bg(t.accent))),
                                )
                                .child(caption)
                                .on_click(cx.listener(move |s, _, _, cx| {
                                    if let Some(draft) = s.question_inputs.get_mut(&answer_key) {
                                        draft.selected = Some(label.clone());
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                    let answer_key = key.clone();
                    let focus_input = input.clone();
                    field = field.child(
                        self.button(
                            SharedString::from(format!("question-custom:{}:{id}", request_key)),
                            if selected.is_none() { "check" } else { "chat" },
                            "Custom answer",
                        )
                        .disabled(sending)
                        .on_click(cx.listener(move |s, _, window, cx| {
                            if let Some(draft) = s.question_inputs.get_mut(&answer_key) {
                                draft.selected = None;
                            }
                            window.focus(&focus_input.focus_handle(cx), cx);
                            cx.notify();
                        })),
                    );
                }
                if selected.is_none() {
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
            group = group.child(
                div().flex().justify_end().child(
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
            );
            panel = panel.child(group);
        }
        panel.into_any_element()
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
        let Some(questions) = request.params["questions"]
            .as_array()
            .filter(|q| !q.is_empty())
        else {
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
            let answer = draft.answer(cx);
            if answer.trim().is_empty() {
                return;
            }
            answers.insert(
                question_id.to_owned(),
                serde_json::json!({"answers":[answer]}),
            );
        }
        self.answering.insert(key);
        self.send(core::Request::Answer {
            task_id: task_id.to_owned(),
            id: id.clone(),
            response: serde_json::json!({"answers":answers}),
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
