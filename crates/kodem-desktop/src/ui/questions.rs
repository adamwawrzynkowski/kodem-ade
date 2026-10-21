//! Pending provider requests live inside the composer, separate from the prompt draft:
//! permission and plan approvals with their decisions, and questions with their answers.
//! An answer can carry files, folders and images, which the host copies when it is sent.
use super::*;
use composition::PendingAttachment;
use serde_json::{Value, json};

/// Most height of the pending requests above the draft. Taller requests scroll.
const PANEL_HEIGHT: f32 = 420.;
/// Method of OpenCode's questions, whose choice fields take only the values of their options.
const OPENCODE_QUESTION: &str = "opencode/requestUserInput";

pub(super) struct AnswerDraft {
    input: Entity<TextInput>,
    selected: Vec<String>,
    /// Files, folders and images to send with the answer. Their cards describe them where they
    /// are; the host copies them when the answer is sent.
    attachments: Vec<PendingAttachment>,
    /// Pasted images staged in private files, which must last until the host has copied them.
    images: Vec<tempfile::TempPath>,
    /// Pasted images that are still being staged.
    staging: usize,
    _subscriptions: Vec<Subscription>,
}

impl AnswerDraft {
    fn new(input: Entity<TextInput>, subscriptions: Vec<Subscription>) -> Self {
        Self {
            input,
            selected: Vec::new(),
            attachments: Vec::new(),
            images: Vec::new(),
            staging: 0,
            _subscriptions: subscriptions,
        }
    }

    // Chosen options come first. The typed text is the answer while no option is chosen, and a
    // note beside the options where the question takes one.
    fn answers(&self, extras: bool, cx: &App) -> Vec<String> {
        let text = self.input.read(cx).text();
        let text = (!text.trim().is_empty()).then_some(text);
        if self.selected.is_empty() {
            text.into_iter().collect()
        } else if extras {
            self.selected.iter().cloned().chain(text).collect()
        } else {
            self.selected.clone()
        }
    }

    // An attachment answers a question on its own, as a screenshot can.
    fn answered(&self, extras: bool, cx: &App) -> bool {
        !self.answers(extras, cx).is_empty() || (extras && !self.attachments.is_empty())
    }

    fn is_empty(&self, cx: &App) -> bool {
        self.selected.is_empty() && self.attachments.is_empty() && self.input.read(cx).is_empty()
    }
}

// Key of the feedback field that belongs to an approval request.
const FEEDBACK: &str = "decline-feedback";

fn decision_label(decision: &str) -> &str {
    match decision {
        core::SWITCH_TO_AUTO_REVIEW => "Switch to AI auto-approve",
        "accept" => "Allow once",
        "acceptForSession" => "Allow for session",
        "decline" => "Decline",
        "declineAlways" => "Always decline",
        "cancel" => "Stop turn",
        other => other,
    }
}

// Reports whether a question takes a note and attachments beside its chosen options.
fn takes_extras(request: &InputRequest, question: &Value) -> bool {
    request.method != OPENCODE_QUESTION
        || question["options"]
            .as_array()
            .is_none_or(|options| options.is_empty())
}

impl Shell {
    fn feedback_draft(
        &mut self,
        key: (String, String),
        cx: &mut Context<Self>,
    ) -> &mut AnswerDraft {
        self.question_inputs.entry(key).or_insert_with(|| {
            let input = cx.new(|cx| {
                TextInput::new(
                    "Optional: tell the agent what to do instead, then decline",
                    cx,
                )
            });
            let subscription = cx.observe(&input, |_, _, cx| cx.notify());
            AnswerDraft::new(input, vec![subscription])
        })
    }

    // The draft of one answer. Its field submits like the composer: Return moves on or sends,
    // and pasted files and images attach to the answer.
    fn question_draft(
        &mut self,
        key: (String, String),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> &mut AnswerDraft {
        self.question_inputs.entry(key.clone()).or_insert_with(|| {
            let input = cx.new(|cx| TextInput::composer("", cx));
            let entered = key.clone();
            let pasted_into = key;
            let subscriptions = vec![
                cx.observe(&input, |_, _, cx| cx.notify()),
                cx.subscribe_in(&input, window, move |s, _, _: &SubmitInput, window, cx| {
                    s.answer_entered(&entered, window, cx)
                }),
                cx.subscribe(
                    &input,
                    move |s, _, pasted: &crate::clipboard::PastedAttachments, cx| {
                        s.paste_answer_attachments(&pasted_into, pasted, cx)
                    },
                ),
            ];
            AnswerDraft::new(input, subscriptions)
        })
    }

    // Returns the pending request whose drafts are keyed by `request_key`, the JSON text of its
    // identity.
    fn pending_request(&self, request_key: &str) -> Option<InputRequest> {
        let id: Value = serde_json::from_str(request_key).ok()?;
        self.view
            .as_ref()?
            .requests
            .iter()
            .find(|request| request.id == id)
            .cloned()
    }

    fn question_answered(&self, request: &InputRequest, question: &Value, cx: &App) -> bool {
        question["id"].as_str().is_some_and(|id| {
            self.question_inputs
                .get(&(request.id.to_string(), id.to_owned()))
                .is_some_and(|draft| draft.answered(takes_extras(request, question), cx))
        })
    }

    // Every question has an answer and no pasted image is still being staged.
    fn answers_ready(&self, request: &InputRequest, cx: &App) -> bool {
        let request_key = request.id.to_string();
        request
            .questions()
            .is_some_and(|questions| !questions.is_empty())
            && request
                .questions()
                .into_iter()
                .flatten()
                .all(|question| self.question_answered(request, question, cx))
            && self
                .question_inputs
                .iter()
                .all(|((key, _), draft)| key != &request_key || draft.staging == 0)
    }

    // Return in an answer sends the answers once all are given, and otherwise moves to the next
    // question that still needs one.
    fn answer_entered(
        &mut self,
        key: &(String, String),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.pending_request(&key.0) else {
            return;
        };
        if self.answers_ready(&request, cx) {
            self.submit_answers(&request.task_id, &request.id, cx);
            return;
        }
        let questions = request.questions().cloned().unwrap_or_default();
        let start = questions
            .iter()
            .position(|question| question["id"] == key.1.as_str())
            .unwrap_or(0);
        let next = (1..=questions.len())
            .map(|offset| &questions[(start + offset) % questions.len()])
            .find(|question| !self.question_answered(&request, question, cx));
        if let Some(draft) = next
            .and_then(|question| question["id"].as_str())
            .and_then(|id| self.question_inputs.get(&(key.0.clone(), id.to_owned())))
        {
            window.focus(&draft.input.focus_handle(cx), cx);
        }
    }

    // Adds files, folders and staged images to an answer. A path that is already attached is
    // not added twice.
    fn attach_to_answer(
        &mut self,
        key: &(String, String),
        paths: Vec<PathBuf>,
        images: Vec<tempfile::TempPath>,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        if !self.selected_runtime().files {
            self.report_error("Attachments are unavailable for the selected provider. Refresh its connection or choose another provider.", cx);
            return;
        }
        let Some(draft) = self.question_inputs.get_mut(key) else {
            return;
        };
        for path in paths {
            if !draft
                .attachments
                .iter()
                .any(|attachment| attachment.source.path() == path)
            {
                draft
                    .attachments
                    .push(PendingAttachment::new(AttachmentSource::File(path)));
            }
        }
        draft.images.extend(images);
        cx.notify();
    }

    // Attaches copied files to an answer, and stages pasted images in private files first.
    fn paste_answer_attachments(
        &mut self,
        key: &(String, String),
        pasted: &crate::clipboard::PastedAttachments,
        cx: &mut Context<Self>,
    ) {
        let extras = self.pending_request(&key.0).is_some_and(|request| {
            request.questions().into_iter().flatten().any(|question| {
                question["id"] == key.1.as_str() && takes_extras(&request, question)
            })
        });
        if !extras {
            self.report_error(
                "This question takes only one of its options, so it cannot take attachments.",
                cx,
            );
            return;
        }
        if pasted.images.is_empty() {
            self.attach_to_answer(key, pasted.paths.clone(), Vec::new(), cx);
            return;
        }
        if !self.accepts_images() {
            self.report_error("The selected provider and model do not support image attachments. Choose another model before pasting an image.", cx);
            return;
        }
        let Some(draft) = self.question_inputs.get_mut(key) else {
            return;
        };
        draft.staging += 1;
        let pasted = pasted.clone();
        let key = key.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::clipboard::stage_images(pasted.images)
                        .map(|images| (pasted.paths, images))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                // An answer that was sent or left meanwhile drops the staged images.
                let Some(draft) = s.question_inputs.get_mut(&key) else {
                    return;
                };
                draft.staging -= 1;
                match result {
                    Ok((mut paths, images)) => {
                        paths.extend(images.iter().map(|image| image.to_path_buf()));
                        s.attach_to_answer(&key, paths, images, cx);
                    }
                    Err(error) => s.report_error(format!("Could not paste image: {error:#}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Opens the native picker of files and folders for one answer.
    fn choose_answer_attachments(&mut self, key: (String, String), cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Attach files, folders or images to the answer".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let _ = this.update(cx, |s, cx| s.attach_to_answer(&key, paths, Vec::new(), cx));
            }
        })
        .detach();
    }

    /// Renders every pending approval and question above the prompt within its container.
    pub(super) fn question_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            .max_h(px(PANEL_HEIGHT))
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
                self.question_card(&request, window, cx)
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
                    } else if request.is_kodem_tool_approval() {
                        "Approval setting needed"
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
                        &crate::markdown::blocks(&plan),
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
                            .map(str::to_owned)
                            .or_else(|| request.tool_arguments())
                            .unwrap_or_else(|| {
                                "File changes; inspect the matching item in the transcript.".into()
                            }),
                    ),
            );
        }
        // Codex takes no message with a declined Kodem tool call.
        let can_decline = decisions.iter().any(|decision| decision == "decline")
            && !request.is_kodem_tool_approval();
        let feedback_key = (request_key.clone(), FEEDBACK.to_owned());
        if can_decline {
            let input = self.feedback_draft(feedback_key.clone(), cx).input.clone();
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
                core::SWITCH_TO_AUTO_REVIEW => self.primary(id, "shield", label),
                "accept" if request.is_kodem_tool_approval() => self.button(id, "check", label),
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

    // One request's questions: a progress readout, then each question with its options, its
    // field and its attachments, and the send action.
    fn question_card(
        &mut self,
        request: &InputRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let Some(questions) = request.questions().cloned() else {
            return div().into_any_element();
        };
        let request_key = request.id.to_string();
        let sending = self.answering.contains(&request_key);
        let files = self.selected_runtime().files;
        let mut group = div().flex().flex_col();
        for (index, question) in questions.iter().enumerate() {
            if index > 0 {
                group = group.child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)));
            }
            group = group
                .child(self.question_field(request, question, index, sending, files, window, cx));
        }
        let answered = questions
            .iter()
            .filter(|question| self.question_answered(request, question, cx))
            .count();
        let ready = self.answers_ready(request, cx);
        let task_id = request.task_id.clone();
        let id = request.id.clone();
        let hint = |keys: &'static str, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(kit::keycap(keys, t))
                .child(label)
        };
        kit::block(t)
            .flex_shrink_0()
            .border_color(t.accent.opacity(0.3))
            .child(
                kit::block_header("Answer needed", t.accent, t)
                    .pr(px(10.))
                    .bg(t.accent.opacity(0.07))
                    .border_color(t.accent.opacity(0.22))
                    .map(|header| {
                        if questions.len() > 1 {
                            header
                                .child(figure(
                                    format!("{answered}/{} ANSWERED", questions.len()),
                                    if answered == questions.len() {
                                        t.accent
                                    } else {
                                        t.muted
                                    },
                                ))
                                .child(kit::meter(
                                    Some(answered as f32 / questions.len() as f32),
                                    t.accent,
                                    44.,
                                    t,
                                ))
                        } else {
                            header.child(kit::chip("1 QUESTION", t.accent))
                        }
                    }),
            )
            .child(group)
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .child(
                div()
                    .p(px(8.))
                    .pl(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(10.))
                            .font_family(crate::theme::mono())
                            .text_size(text_px(10.5))
                            .text_color(t.subtle)
                            .child(hint("⏎", if ready { "Send" } else { "Next question" }))
                            .child(hint("⇧⏎", "New line"))
                            .when(files, |hints| {
                                hints.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.))
                                        .child(icon("paperclip").size(px(11.)))
                                        .child("Drop or paste files on a question"),
                                )
                            }),
                    )
                    .child(
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

    // One question: its number, which turns into a check once it has an answer, its options,
    // the field for the user's own words and the files attached to the answer. Files dropped on
    // the question attach to its answer.
    #[allow(clippy::too_many_arguments)]
    fn question_field(
        &mut self,
        request: &InputRequest,
        question: &Value,
        index: usize,
        sending: bool,
        files: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let Some(id) = question["id"].as_str() else {
            return div().into_any_element();
        };
        let request_key = request.id.to_string();
        let key = (request_key.clone(), id.to_owned());
        let multiple = question["multiSelect"].as_bool().unwrap_or(false);
        let extras = takes_extras(request, question);
        let attachable = extras && files;
        let options = question["options"]
            .as_array()
            .filter(|options| !options.is_empty())
            .cloned()
            .unwrap_or_default();
        let draft = self.question_draft(key.clone(), window, cx);
        let input = draft.input.clone();
        let selected = draft.selected.clone();
        let cards: Vec<Attachment> = draft
            .attachments
            .iter()
            .map(|attachment| attachment.card.clone())
            .collect();
        let staging = draft.staging;
        let answered = draft.answered(extras, cx);
        let empty = draft.is_empty(cx);
        // The field asks for the answer while no option is chosen, and for a note beside them.
        let shows_field = selected.is_empty() || extras;
        let placeholder = if options.is_empty() {
            "Type your answer…"
        } else if selected.is_empty() {
            "Or type your own answer…"
        } else {
            "Add a note for the agent (optional)…"
        };
        input.update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        let focused = input.read(cx).focus_handle(cx).is_focused(window);

        let step = kit::chip(
            format!("{:02}", index + 1),
            if answered { t.accent } else { t.subtle },
        )
        .when(answered, |chip| chip.child(icon("check").size(px(10.))));
        let clear_key = key.clone();
        let mut title = div()
            .flex()
            .items_start()
            .gap(px(8.))
            .child(div().pt(px(1.)).child(step))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .when_some(question["header"].as_str(), |column, header| {
                        column.child(caps(header, t).text_color(t.accent))
                    })
                    .child(
                        div()
                            .text_size(text_px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                question["question"]
                                    .as_str()
                                    .unwrap_or("Question")
                                    .to_owned(),
                            ),
                    ),
            )
            .when(multiple, |row| row.child(kit::chip("CHOOSE ANY", t.subtle)));
        if !empty && !sending {
            title = title.child(
                self.icon_button(
                    SharedString::from(format!("question-clear:{request_key}:{id}")),
                    "undo-2",
                    "Clear answer",
                )
                .size(px(20.))
                .on_click(cx.listener(move |s, _, _, cx| {
                    if let Some(draft) = s.question_inputs.get_mut(&clear_key) {
                        draft.selected.clear();
                        draft.attachments.clear();
                        draft.images.clear();
                        let input = draft.input.clone();
                        input.update(cx, |input, cx| input.set_text(String::new(), cx));
                    }
                    cx.notify();
                })),
            );
        }
        let drop_key = key.clone();
        let mut field = div()
            .id(SharedString::from(format!("question:{request_key}:{id}")))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .when(attachable && !sending, |field| {
                field
                    .drag_over::<ExternalPaths>(move |style, _, _, _| {
                        style.bg(t.accent.opacity(0.08))
                    })
                    .on_drop(cx.listener(move |s, paths: &ExternalPaths, _, cx| {
                        s.attach_to_answer(&drop_key, paths.paths().to_vec(), Vec::new(), cx)
                    }))
            })
            .child(title);
        if !options.is_empty() {
            let mut list = div().flex().flex_col().gap(px(2.));
            for (option_index, option) in options.iter().enumerate() {
                let Some(label) = option["label"].as_str() else {
                    continue;
                };
                list = list.child(self.answer_option(
                    &key,
                    option_index,
                    label,
                    option["description"].as_str().filter(|d| !d.is_empty()),
                    selected.iter().any(|chosen| chosen == label),
                    multiple,
                    sending,
                    cx,
                ));
            }
            field = field.child(list);
            // A choice field of OpenCode takes no words beside its options.
            if !shows_field {
                let answer_key = key.clone();
                let focus_input = input.clone();
                field = field.child(
                    div().flex().child(
                        self.ghost(
                            SharedString::from(format!("question-custom:{request_key}:{id}")),
                            "pencil",
                            "Write my own answer",
                        )
                        .disabled(sending)
                        .on_click(cx.listener(move |s, _, window, cx| {
                            if let Some(draft) = s.question_inputs.get_mut(&answer_key) {
                                draft.selected.clear();
                            }
                            window.focus(&focus_input.focus_handle(cx), cx);
                            cx.notify();
                        })),
                    ),
                );
            }
        }
        if shows_field {
            let pick_key = key.clone();
            field = field.child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(6.))
                    .pl(px(10.))
                    .pr(px(3.))
                    .py(px(3.))
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .border_1()
                    .border_color(if focused {
                        t.accent.opacity(0.6)
                    } else {
                        t.edge
                    })
                    .bg(t.canvas)
                    .shadow(kit::recessed_light(t))
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "question-input:{request_key}:{id}"
                            )))
                            .flex_1()
                            .min_w_0()
                            .max_h(px(100.))
                            .overflow_y_scroll()
                            .py(px(4.))
                            .child(if sending {
                                div().child(input.read(cx).text()).into_any_element()
                            } else {
                                input.into_any_element()
                            }),
                    )
                    .when(attachable, |row| {
                        row.child(
                            self.icon_button(
                                SharedString::from(format!("question-attach:{request_key}:{id}")),
                                "paperclip",
                                "Attach files, folders or images",
                            )
                            .size(px(22.))
                            .disabled(sending)
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    s.choose_answer_attachments(pick_key.clone(), cx)
                                },
                            )),
                        )
                    }),
            );
        }
        if !cards.is_empty() || staging > 0 {
            let mut tray = div().flex().flex_wrap().items_center().gap_2();
            for (attachment_index, card) in cards.iter().enumerate() {
                let remove_key = key.clone();
                tray = tray.child(
                    self.attachment_card(card, cx).child(
                        self.icon_button(
                            SharedString::from(format!(
                                "question-detach:{request_key}:{id}:{attachment_index}"
                            )),
                            "close",
                            "Remove attachment",
                        )
                        .size(px(20.))
                        .disabled(sending)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if let Some(draft) = s.question_inputs.get_mut(&remove_key)
                                && attachment_index < draft.attachments.len()
                            {
                                let removed = draft.attachments.remove(attachment_index);
                                draft
                                    .images
                                    .retain(|image| **image != *removed.source.path());
                            }
                            cx.notify();
                        })),
                    ),
                );
            }
            if staging > 0 {
                tray = tray.child(kit::loader_row("Preparing pasted images", t));
            }
            field = field.child(tray);
        }
        field.into_any_element()
    }

    // One option of a question: a radio or check mark, its label and description. A chosen
    // option is a selected row; choosing a single choice again clears it.
    #[allow(clippy::too_many_arguments)]
    fn answer_option(
        &self,
        key: &(String, String),
        index: usize,
        label: &str,
        description: Option<&str>,
        chosen: bool,
        multiple: bool,
        sending: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let t = self.theme;
        let label = label.to_owned();
        let answer_key = key.clone();
        let mut caption = div()
            .flex()
            .flex_col()
            .gap_1()
            .min_w_0()
            .font_family(crate::theme::sans())
            .font_weight(FontWeight::NORMAL)
            .text_size(text_px(12.5))
            .child(label.clone());
        if let Some(description) = description {
            caption = caption.child(
                div()
                    .text_size(text_px(11.))
                    .text_color(t.muted)
                    .child(description.to_owned()),
            );
        }
        self.control(SharedString::from(format!(
            "question-option:{}:{}:{index}",
            key.0, key.1
        )))
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
                .layer(dither::row_wash(t.accent, true, false))
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
                if let Some(position) = draft.selected.iter().position(|chosen| chosen == &label) {
                    draft.selected.remove(position);
                } else if multiple {
                    draft.selected.push(label.clone());
                } else {
                    draft.selected = vec![label.clone()];
                }
            }
            cx.notify();
        }))
    }

    fn submit_answers(&mut self, task_id: &str, id: &Value, cx: &mut Context<Self>) {
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
        if !self.answers_ready(request, cx) {
            return;
        }
        let Some(questions) = request.questions() else {
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
            let extras = takes_extras(request, question);
            let mut answer = json!({"answers": draft.answers(extras, cx)});
            if extras && !draft.attachments.is_empty() {
                answer["attachments"] = json!(
                    draft
                        .attachments
                        .iter()
                        .map(|attachment| attachment.source.path())
                        .collect::<Vec<_>>()
                );
            }
            answers.insert(question_id.to_owned(), answer);
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
