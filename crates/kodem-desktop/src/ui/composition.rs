//! Composer intent, attachment cards, captured context, and the host-owned outgoing tray.
use super::*;
use std::path::PathBuf;

impl Shell {
    /// Reports whether the draft has text or selected context to submit.
    pub(super) fn has_composer_content(&self, cx: &App) -> bool {
        !self.input.read(cx).text().trim().is_empty()
            || self.view.as_ref().map_or_else(
                || !self.pending_attachments.is_empty(),
                |v| !v.task.feedback.is_empty() || !v.task.attachments.is_empty(),
            )
    }

    /// Sends the same composition for keyboard and pointer submission.
    pub(super) fn submit_current(&mut self, cx: &mut Context<Self>) {
        if self.panel != Panel::None
            || self.submitted_draft.is_some()
            || !self.has_composer_content(cx)
        {
            return;
        }
        let text = self.input.read(cx).text();
        let Some(view) = &self.view else {
            if self.start_after_create {
                return;
            }
            if let Some(project) = self.workspace.as_ref().and_then(|w| {
                w.projects
                    .iter()
                    .find(|p| Some(&p.id) == self.selected_project.as_ref())
                    .or(w.projects.first())
            }) {
                self.empty_draft = text;
                self.start_after_create = true;
                self.send(core::Request::CreateTask {
                    project_id: project.id.clone(),
                    worktree: self.new_worktree,
                });
                cx.notify();
            }
            return;
        };
        if self
            .importing_tasks
            .get(&view.task.id)
            .copied()
            .unwrap_or(0)
            > 0
        {
            return;
        }
        if view.task.state == RunState::DeliveryUnknown {
            self.error = Some("Check delivery history before sending another message.".into());
            cx.notify();
            return;
        }
        let id = new_id();
        self.submitted_draft = Some(PendingSubmission {
            task_id: view.task.id.clone(),
            id: id.clone(),
            text: text.clone(),
            revision: self.draft_revision,
        });
        self.send(core::Request::EditTask {
            task_id: view.task.id.clone(),
            edit: TaskEdit::Draft(text.clone()),
        });
        self.send(core::Request::Submit {
            task_id: view.task.id.clone(),
            submission_id: id,
            text,
            feedback_ids: view.task.feedback.iter().map(|f| f.id.clone()).collect(),
            attachment_ids: view.task.attachments.iter().map(|a| a.id.clone()).collect(),
            mode: self.settings.send_mode,
        });
        cx.notify();
    }

    /// Imports dropped or chosen paths through the host without blocking the UI.
    pub(super) fn attach_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.attach_sources(paths.into_iter().map(AttachmentSource::File).collect(), cx);
    }

    /// Adds an explicit skill reference using the same durable attachment flow as files.
    pub(super) fn attach_skill(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.attach_sources(vec![AttachmentSource::Skill(path)], cx);
    }

    fn attach_sources(&mut self, sources: Vec<AttachmentSource>, cx: &mut Context<Self>) {
        if sources.is_empty() {
            return;
        }
        if let Some(task_id) = self.selected.clone() {
            self.import_attachments(task_id, sources);
        } else {
            for source in sources {
                if !self.pending_attachments.contains(&source) {
                    self.pending_attachments.push(source);
                }
            }
        }
        cx.notify();
    }

    /// Tracks an asynchronous import for the task that requested it.
    pub(super) fn import_attachments(&mut self, task_id: String, sources: Vec<AttachmentSource>) {
        *self.importing_tasks.entry(task_id.clone()).or_default() += 1;
        self.send(core::Request::ImportAttachments { task_id, sources });
    }

    /// Opens the native multiple-file picker used by the composer paperclip.
    pub(super) fn choose_attachments(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach files or images".into()),
        });
        let selected = self.selected.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let _ = this.update(cx, |s, cx| {
                    if selected == s.selected {
                        s.attach_paths(paths, cx);
                    } else if let Some(task_id) = selected {
                        s.import_attachments(
                            task_id,
                            paths.into_iter().map(AttachmentSource::File).collect(),
                        );
                    }
                });
            }
        })
        .detach();
    }

    /// Renders a stable imported file or image, with an optional draft removal action.
    pub(super) fn attachment_card(
        &self,
        attachment: &Attachment,
        task_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let skill_name = match &attachment.kind {
            AttachmentKind::Skill { source } => source
                .parent()
                .and_then(|p| p.file_name())
                .map(|name| name.to_string_lossy().into_owned()),
            _ => None,
        };
        let mut card = div()
            .flex()
            .items_center()
            .gap_2()
            .p_2()
            .rounded(px(8.))
            .bg(t.sidebar)
            .shadow(t.elevation())
            .border_1()
            .border_color(t.border)
            .flex_shrink_0()
            .min_w_0()
            .max_w(px(280.));
        card = card.child(if attachment.kind == AttachmentKind::Image {
            let attachment = attachment.clone();
            self.control((
                ElementId::from("preview-attachment"),
                SharedString::from(attachment.id.clone()),
            ))
            .size(px(42.))
            .p_0()
            .rounded(px(5.))
            .overflow_hidden()
            .accessibility_label(format!("Preview {}", attachment.name))
            .hover(move |s| s.border_color(t.accent))
            .child(
                img(attachment.path.clone())
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .on_click(cx.listener(move |s, _, window, cx| {
                s.open_image_preview(&attachment, window, cx);
            }))
            .into_any_element()
        } else {
            div()
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    icon(if skill_name.is_some() { "code" } else { "file" }).text_color(t.accent),
                )
                .into_any_element()
        });
        card = card.child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div().text_size(px(11.)).text_ellipsis().child(
                        skill_name
                            .clone()
                            .unwrap_or_else(|| attachment.name.clone()),
                    ),
                )
                .child(div().text_size(px(10.)).text_color(t.muted).child(
                    if skill_name.is_some() {
                        "Skill".into()
                    } else {
                        file_size(attachment.bytes)
                    },
                )),
        );
        if let Some(task_id) = task_id {
            let task_id = task_id.to_owned();
            let id = attachment.id.clone();
            card = card.child(
                self.icon_button(
                    (
                        ElementId::from("remove-attachment"),
                        SharedString::from(id.clone()),
                    ),
                    "close",
                    "Remove attachment",
                )
                .size(px(20.))
                .on_click(cx.listener(move |s, _, _, _| {
                    s.send(core::Request::RemoveAttachment {
                        task_id: task_id.clone(),
                        id: id.clone(),
                    });
                    s.send(core::Request::Task {
                        task_id: task_id.clone(),
                        before: None,
                    });
                })),
            );
        }
        card.into_any_element()
    }

    /// Shows only the path and captured range while retaining the full context in the payload.
    pub(super) fn context_card(
        &mut self,
        feedback: &Feedback,
        task_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let range = if feedback.first_row.is_some() {
            match (feedback.old_range, feedback.new_range) {
                (Some((a, b)), Some((c, d))) => format!("−{a}–{b}  +{c}–{d}"),
                (Some((a, b)), None) => format!("Before {a}–{b}"),
                (None, Some((a, b))) => format!("After {a}–{b}"),
                _ => "Captured diff selection".into(),
            }
        } else {
            format!(
                "{} lines {}–{}",
                if feedback.side == Side::Old {
                    "Before"
                } else {
                    "After"
                },
                feedback.start,
                feedback.end
            )
        };
        let mut title = div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(34.))
            .text_size(px(11.))
            .child(icon("code").size(px(13.)).text_color(t.accent))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(feedback.label.clone()),
            )
            .child(div().flex_shrink_0().text_color(t.muted).child(range));
        if let Some(task_id) = task_id {
            let task_id = task_id.to_owned();
            let id = feedback.id.clone();
            title = title.child(
                self.icon_button(
                    (
                        ElementId::from("remove-context"),
                        SharedString::from(id.clone()),
                    ),
                    "close",
                    "Remove code context",
                )
                .size(px(20.))
                .on_click(cx.listener(move |s, _, _, _| {
                    s.send(core::Request::RemoveFeedback {
                        task_id: task_id.clone(),
                        id: id.clone(),
                    });
                    s.send(core::Request::Task {
                        task_id: task_id.clone(),
                        before: None,
                    });
                })),
            );
        }
        div()
            .w_full()
            .min_w_0()
            .h(px(36.))
            .flex_shrink_0()
            .rounded(px(9.))
            .border_1()
            .border_color(t.border)
            .bg(t.sidebar)
            .shadow(t.elevation())
            .overflow_hidden()
            .child(title)
            .into_any_element()
    }

    /// Displays durable queued work and recoverable delivery failures above the composer.
    pub(super) fn outgoing_tray(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = self.view.clone().filter(|v| !v.outgoing.is_empty()) else {
            return div().into_any_element();
        };
        let t = self.theme;
        let paused = !view.task.state.active()
            && view
                .outgoing
                .iter()
                .any(|m| m.state == DeliveryState::Queued);
        let task_id = view.task.id.clone();
        let mut tray = div()
            .id("outgoing-tray")
            .max_h(px(220.))
            .overflow_y_scroll()
            .mb_2()
            .rounded(px(9.))
            .border_1()
            .border_color(t.border)
            .bg(t.sidebar)
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .child(icon("arrow-down").size(px(13.)))
                    .child(if paused {
                        "Queue paused"
                    } else {
                        "Next messages"
                    })
                    .child(div().flex_1())
                    .when(paused, |d| {
                        d.child(
                            self.ghost("resume-queue", "arrow-right", "Resume")
                                .h(px(24.))
                                .disabled(view.task.state == RunState::DeliveryUnknown)
                                .on_click(cx.listener(move |s, _, _, _| {
                                    s.send(core::Request::ResumeQueue {
                                        task_id: task_id.clone(),
                                    })
                                })),
                        )
                    }),
            );
        for (index, outgoing) in view.outgoing.iter().enumerate() {
            let id = outgoing.id.clone();
            let task_id = view.task.id.clone();
            let restore_task = task_id.clone();
            let restore_id = id.clone();
            let expanded_key = format!("outgoing:{id}");
            let expanded = self.folded.contains(&expanded_key);
            let label = match outgoing.state {
                DeliveryState::Queued => "Queued",
                DeliveryState::Pending => "Sending…",
                DeliveryState::Failed => "Not sent",
                DeliveryState::Unknown => "Delivery unknown",
            };
            let mut row = div()
                .p_2()
                .rounded(px(6.))
                .bg(t.raised)
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            self.ghost(
                                ("outgoing-expand", index),
                                if expanded {
                                    "chevron-down"
                                } else {
                                    "chevron-right"
                                },
                                format!("{} · {label}", index + 1),
                            )
                            .h(px(23.))
                            .text_size(px(10.))
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    if !s.folded.remove(&expanded_key) {
                                        s.folded.insert(expanded_key.clone());
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(px(12.))
                                .child(if outgoing.text.is_empty() {
                                    "Attached context".into()
                                } else {
                                    outgoing.text.clone()
                                }),
                        )
                        .when(
                            matches!(
                                outgoing.state,
                                DeliveryState::Queued | DeliveryState::Failed
                            ),
                            |d| {
                                d.child(
                                    self.icon_button(
                                        ("restore-queued", index),
                                        "edit",
                                        "Restore to empty composer",
                                    )
                                    .size(px(22.))
                                    .disabled(
                                        !self.input.read(cx).text().is_empty()
                                            || !view.task.attachments.is_empty()
                                            || !view.task.feedback.is_empty()
                                            || self.submitted_draft.is_some(),
                                    )
                                    .on_click(cx.listener(
                                        move |s, _, _, _| {
                                            s.send(core::Request::EditTask {
                                                task_id: restore_task.clone(),
                                                edit: TaskEdit::Draft(String::new()),
                                            });
                                            s.send(core::Request::RestoreOutgoing {
                                                task_id: restore_task.clone(),
                                                id: restore_id.clone(),
                                            });
                                        },
                                    )),
                                )
                            },
                        )
                        .when(
                            matches!(
                                outgoing.state,
                                DeliveryState::Queued | DeliveryState::Failed
                            ),
                            |d| {
                                d.child(
                                    self.icon_button(
                                        ("cancel-queued", index),
                                        "close",
                                        "Remove this outgoing message",
                                    )
                                    .size(px(22.))
                                    .on_click(cx.listener(
                                        move |s, _, _, _| {
                                            s.send(core::Request::CancelQueued {
                                                task_id: task_id.clone(),
                                                id: id.clone(),
                                            });
                                            s.send(core::Request::Task {
                                                task_id: task_id.clone(),
                                                before: None,
                                            });
                                        },
                                    )),
                                )
                            },
                        ),
                )
                .when_some(outgoing.error.clone(), |d, error| {
                    d.child(div().text_size(px(11.)).text_color(t.removed).child(error))
                });
            if expanded {
                let text = self.readonly(
                    format!("outgoing-text:{}", outgoing.id),
                    outgoing.text.clone(),
                    cx,
                );
                row = row.child(div().text_size(px(12.)).child(text));
                for attachment in &outgoing.attachments {
                    row = row.child(self.attachment_card(attachment, None, cx));
                }
                for feedback in &outgoing.feedback {
                    row = row.child(self.context_card(feedback, None, cx));
                }
            } else if !outgoing.attachments.is_empty() || !outgoing.feedback.is_empty() {
                row = row.child(div().text_size(px(10.)).text_color(t.muted).child(format!(
                    "{} attachments · {} code selections",
                    outgoing.attachments.len(),
                    outgoing.feedback.len()
                )));
            }
            tray = tray.child(row);
        }
        tray.into_any_element()
    }
}

fn file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024. * 1024.))
    }
}
