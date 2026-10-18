//! Composer intent, attachment cards, captured context, and the host-owned outgoing tray.
use super::*;
use std::{fs, io::Read, path::PathBuf};

/// Name of the effect anchor on the queue tray, where chips for queued messages land.
pub(super) const OUTBOX_ANCHOR: &str = "outbox";
/// Most height of the list of next messages in the composer: about three folded messages and
/// the start of a fourth. More messages scroll, so the draft keeps its place.
const OUTBOX_HEIGHT: f32 = 128.;

// Returns the name of the effect anchor on the tray row of an outgoing message.
fn outgoing_anchor(id: &str) -> String {
    format!("outgoing:{id}")
}

/// A file, folder or skill chosen on a start screen before its task exists. The host imports it
/// when the task is created; until then its card describes the source where it is.
pub(super) struct PendingAttachment {
    pub(super) source: AttachmentSource,
    pub(super) card: Attachment,
}

impl PendingAttachment {
    /// Reads what the card of `source` shows: its name, size and whether it is a folder or an
    /// image. A file that cannot be read shows as a file; the import reports the error.
    pub(super) fn new(source: AttachmentSource) -> Self {
        let path = source.path().to_path_buf();
        let meta = fs::metadata(&path).ok();
        let file = meta.as_ref().is_some_and(fs::Metadata::is_file);
        let kind = match &source {
            AttachmentSource::Skill(skill) => AttachmentKind::Skill {
                source: skill.clone(),
            },
            AttachmentSource::File(_) if meta.as_ref().is_some_and(fs::Metadata::is_dir) => {
                AttachmentKind::Folder
            }
            AttachmentSource::File(_) if file && is_image_file(&path) => AttachmentKind::Image,
            AttachmentSource::File(_) => AttachmentKind::File,
        };
        let card = Attachment {
            id: format!("pending:{source:?}"),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            bytes: meta.filter(|_| file).map_or(0, |meta| meta.len()),
            path,
            kind,
        };
        Self { source, card }
    }
}

// Reports whether the file at `path` starts with the header of an image the host imports as one.
fn is_image_file(path: &std::path::Path) -> bool {
    let mut header = Vec::with_capacity(AttachmentKind::IMAGE_HEADER_LEN);
    fs::File::open(path)
        .and_then(|file| {
            file.take(AttachmentKind::IMAGE_HEADER_LEN as u64)
                .read_to_end(&mut header)
        })
        .is_ok_and(|_| AttachmentKind::is_image_header(&header))
}

impl Shell {
    /// Tracks only newly added draft context and stops requesting frames once it settles.
    pub(super) fn sync_context_arrivals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut keys = HashSet::new();
        if let Some(view) = &self.view {
            keys.extend(view.task.attachments.iter().map(|a| a.id.clone()));
            keys.extend(view.task.feedback.iter().map(|f| f.id.clone()));
        } else {
            keys.extend(
                self.pending_attachments
                    .iter()
                    .map(|pending| pending.card.id.clone()),
            );
        }
        let initial = self.context_task != self.selected;
        self.context_task = self.selected.clone();
        if initial {
            self.context_arrivals.clear();
        }
        self.context_arrivals.retain(|key, _| keys.contains(key));
        let now = std::time::Instant::now();
        for key in keys {
            self.context_arrivals
                .entry(key)
                .or_insert_with(|| (!initial && !cx.reduce_motion()).then_some(now));
        }
        let mut active = false;
        for time in self.context_arrivals.values_mut() {
            if time.is_some_and(|time| {
                cx.reduce_motion() || time.elapsed() >= Duration::from_millis(320)
            }) {
                *time = None;
            }
            active |= time.is_some();
        }
        if active {
            window.request_animation_frame();
        }
        self.send_arrivals.retain(|_, arrival| !arrival.expired());
    }

    /// Reports whether the draft has text or selected context to submit.
    pub(super) fn has_composer_content(&self, cx: &App) -> bool {
        !self.input.read(cx).is_blank()
            || self.view.as_ref().map_or_else(
                || !self.pending_attachments.is_empty(),
                |v| !v.task.feedback.is_empty() || !v.task.attachments.is_empty(),
            )
    }

    /// Sends the same composition for keyboard and pointer submission.
    pub(super) fn submit_current(&mut self, cx: &mut Context<Self>) {
        self.submit_with(false, cx);
    }

    /// Sends the composition. While a turn runs, `alternate` delivers it the way that is not the
    /// configured default: steering instead of queueing, or the reverse.
    pub(super) fn submit_with(&mut self, alternate: bool, cx: &mut Context<Self>) {
        // A review starts from its picked target, so its start screen needs no text.
        let starting_review = self.mode == TaskKind::Review && self.selected.is_none();
        if self.agent_popup.is_some()
            || self.panel != Panel::None
            || self.pasting_images > 0
            || self.submitted_draft.is_some()
            || self.launch.is_some()
            || (!starting_review && !self.has_composer_content(cx))
        {
            return;
        }
        let mode = if self.selected_runtime().steering
            && self
                .view
                .as_ref()
                .is_none_or(|v| v.task.pending_completion.is_none())
        {
            match (self.settings.send_mode, alternate) {
                (SendMode::Queue, false) | (SendMode::Steer, true) => SendMode::Queue,
                (SendMode::Steer, false) | (SendMode::Queue, true) => SendMode::Steer,
            }
        } else {
            SendMode::Queue
        };
        let text = self.input.read(cx).text();
        let Some(view) = &self.view else {
            self.create_composer_task(cx);
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
            self.report_error("Check delivery history before sending another message.", cx);
            return;
        }
        // The first message starts the thread quietly; later ones send a wave across the
        // background.
        let starts_thread = view.messages.is_empty();
        let id = new_id();
        self.submitted_draft = Some(PendingSubmission {
            task_id: view.task.id.clone(),
            id: id.clone(),
            text: text.clone(),
            revision: self.draft_revision,
            origin: self
                .input
                .read(cx)
                .text_bounds()
                .map(|bounds| bounds.origin)
                .map(|origin| {
                    origin
                        - point(
                            px(crate::theme::MESSAGE_PADDING),
                            px(crate::theme::MESSAGE_PADDING),
                        )
                }),
            queued: view.task.state.active() && mode == SendMode::Queue,
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
            mode,
        });
        if !starts_thread {
            backdrop::spark_send(cx);
        }
        cx.notify();
    }

    /// Steers a queued message into the running turn. The message travels from its row in the
    /// tray into the conversation, like a message sent from the composer.
    fn steer_queued(&mut self, task_id: String, id: String, cx: &mut Context<Self>) {
        if !cx.reduce_motion() {
            let origin = fx::anchor_bounds(&outgoing_anchor(&id), cx).map(|bounds| bounds.origin);
            self.send_arrivals.retain(|_, arrival| !arrival.expired());
            self.send_arrivals
                .insert(id.clone(), motion::Arrival::new(origin));
            self.follow_latest();
        }
        self.send(core::Request::SteerQueued { task_id, id });
        cx.notify();
    }

    /// Reports whether a steered message is on its way into the conversation. It does not stop
    /// in the tray while its provider takes it, so its send transition plays in the conversation.
    fn steer_arriving(&self, message: &OutgoingMessage) -> bool {
        message.state == DeliveryState::Queued
            && message.expected_turn.is_some()
            && self
                .send_arrivals
                .get(&message.id)
                .is_some_and(|arrival| !arrival.queued && !arrival.expired())
    }

    /// Creates one task of the open workspace and submits its first message after the draft
    /// and attachments load.
    fn create_composer_task(&mut self, cx: &mut Context<Self>) {
        if self.creating_task || self.selected.is_some() || self.pasting_images > 0 {
            return;
        }
        let request = match self.mode {
            TaskKind::Review => {
                self.start_review(cx);
                return;
            }
            TaskKind::Chat => core::Request::CreateChat {
                preferences: self.composer_preferences(),
            },
            TaskKind::Coding => {
                let Some(project) = self.workspace.as_ref().and_then(|w| {
                    w.projects
                        .iter()
                        .find(|p| Some(&p.id) == self.selected_project.as_ref())
                        .or(w.projects.first())
                }) else {
                    return;
                };
                core::Request::CreateTask {
                    project_id: project.id.clone(),
                    worktree: self.composer_worktree(),
                    preferences: self.composer_preferences(),
                    base: None,
                }
            }
        };
        self.empty_draft = self.input.read(cx).text();
        self.creating_task = true;
        self.send(request);
        self.begin_launch(cx);
        cx.notify();
    }

    /// Moves the start screen's composer to the bottom of the chat while the host creates the
    /// task, and dissolves the rest of the start screen. The message is sent once the composer
    /// arrives. With reduced motion the task opens as soon as it exists.
    pub(super) fn begin_launch(&mut self, cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            return;
        }
        let Some(from) = fx::anchor_bounds(chat::COMPOSER_ANCHOR, cx) else {
            return;
        };
        self.launch = Some(motion::Launch::new(from));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(motion::LAUNCH).await;
            let _ = this.update(cx, |s, cx| {
                s.finish_launch(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Imports dropped or chosen paths through the host without blocking the UI.
    pub(super) fn attach_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.attach_sources(paths.into_iter().map(AttachmentSource::File).collect(), cx);
    }

    /// Routes copied files and prepares raw clipboard images for the existing attachment import.
    pub(super) fn paste_attachments(
        &mut self,
        pasted: &crate::clipboard::PastedAttachments,
        cx: &mut Context<Self>,
    ) {
        if pasted.images.is_empty() {
            self.attach_paths(pasted.paths.clone(), cx);
            return;
        }
        if !self.accepts_images() {
            self.report_error("The selected provider and model do not support image attachments. Choose another model before pasting an image.", cx);
            return;
        }
        let pasted = pasted.clone();
        let selected = self.selected.clone();
        let preferences = self.composer_preferences();
        self.pasting_images += 1;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                crate::clipboard::stage_images(pasted.images).map(|images| (pasted.paths, images))
            }).await;
            let _ = this.update(cx, |s, cx| {
                s.pasting_images -= 1;
                match result {
                    Ok((mut paths, images)) => {
                        if selected.is_none() && (s.selected.is_some() || s.composer_preferences() != preferences) {
                            s.report_error("Image paste cancelled because the task, provider or model changed. Paste again in the intended conversation.", cx);
                        } else {
                            for image in images {
                                let path = image.to_path_buf();
                                paths.push(path.clone());
                                s.clipboard_images.insert(path, image);
                            }
                            let sources = paths.into_iter().map(AttachmentSource::File).collect();
                            if let Some(task_id) = selected {
                                s.import_attachments(task_id, sources);
                            } else {
                                s.attach_sources(sources, cx);
                            }
                        }
                    }
                    Err(error) => s.report_error(format!("Could not paste image: {error:#}"), cx),
                }
                cx.notify();
            });
        }).detach();
    }

    /// Adds an explicit skill reference using the same durable attachment flow as files.
    pub(super) fn attach_skill(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.attach_sources(vec![AttachmentSource::Skill(path)], cx);
    }

    fn attach_sources(&mut self, sources: Vec<AttachmentSource>, cx: &mut Context<Self>) {
        if sources.is_empty() {
            return;
        }
        if !self.selected_runtime().files {
            self.report_error("Attachments are unavailable for the selected provider. Refresh its connection or choose another provider.", cx);
            return;
        }
        if let Some(task_id) = self.selected.clone() {
            self.import_attachments(task_id, sources);
        } else {
            for source in sources {
                if !self.pending_attachments.iter().any(|p| p.source == source) {
                    self.pending_attachments
                        .push(PendingAttachment::new(source));
                }
            }
        }
        cx.notify();
    }

    /// Tracks an asynchronous import for the task that requested it.
    pub(super) fn import_attachments(&mut self, task_id: String, sources: Vec<AttachmentSource>) {
        *self.importing_tasks.entry(task_id.clone()).or_default() += 1;
        let images = sources
            .iter()
            .filter_map(|source| self.clipboard_images.remove(source.path()))
            .collect();
        let _ = self.tx.send(bridge::Command::ImportAttachments {
            task_id,
            sources,
            images,
        });
    }

    /// Opens the native picker of files and folders used by the composer paperclip.
    pub(super) fn choose_attachments(&mut self, cx: &mut Context<Self>) {
        if !self.selected_runtime().files {
            self.report_error("Attachments are unavailable for the selected provider. Refresh its connection or choose another provider.", cx);
            return;
        }
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Attach files, folders or images".into()),
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

    /// Renders a file, image or folder card; an image shows its preview, which opens on click.
    /// The caller appends a removal action where the attachment can still leave the draft.
    pub(super) fn attachment_card(&self, attachment: &Attachment, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let skill_name = match &attachment.kind {
            AttachmentKind::Skill { source } => source
                .parent()
                .and_then(|p| p.file_name())
                .map(|name| name.to_string_lossy().into_owned()),
            _ => None,
        };
        let mut card = kit::block(t)
            .w_auto()
            .flex()
            .items_center()
            .gap(px(8.))
            .p(px(4.))
            .pr(px(6.))
            .flex_shrink_0()
            .max_w(px(280.));
        card = card.child(if attachment.kind == AttachmentKind::Image {
            let attachment = attachment.clone();
            self.control((
                ElementId::from("preview-attachment"),
                SharedString::from(attachment.id.clone()),
            ))
            .size(px(36.))
            .p_0()
            .rounded(px(crate::theme::RADIUS_SMALL))
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
            kit::icon_tile(
                if skill_name.is_some() {
                    "code"
                } else if attachment.kind == AttachmentKind::Folder {
                    "folder"
                } else {
                    "file"
                },
                if skill_name.is_some() {
                    t.info
                } else {
                    t.accent
                },
                28.,
                false,
            )
            .into_any_element()
        });
        card = card.child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .font_family(crate::theme::mono())
                        .text_size(text_px(11.5))
                        .text_ellipsis()
                        .child(
                            skill_name
                                .clone()
                                .unwrap_or_else(|| attachment.name.clone()),
                        ),
                )
                .child(caps(
                    if skill_name.is_some() {
                        "Skill".into()
                    } else if attachment.kind == AttachmentKind::Folder {
                        "Folder".into()
                    } else {
                        file_size(attachment.bytes)
                    },
                    t,
                )),
        );
        card
    }

    /// Shows a long message of the user as a file. The file unfolds below its card to show the
    /// text, which can be selected and copied.
    pub(super) fn long_text_card(
        &mut self,
        conversation: &Conversation,
        index: usize,
        key: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let unfolded = self.unfolded_texts.contains(key);
        let toggle_key = key.to_owned();
        let toggle_conversation = conversation.clone();
        let card = text_file_card(text, t).child(
            self.icon_button(
                (
                    ElementId::from("toggle-long-text"),
                    SharedString::from(key.to_owned()),
                ),
                if unfolded {
                    "chevron-up"
                } else {
                    "chevron-down"
                },
                if unfolded { "Fold text" } else { "Unfold text" },
            )
            .size(px(20.))
            .on_click(cx.listener(move |s, _, _, cx| {
                if !s.unfolded_texts.remove(&toggle_key) {
                    s.unfolded_texts.insert(toggle_key.clone());
                }
                s.remeasure_messages(&toggle_conversation, index..index + 1);
                cx.notify();
            })),
        );
        if !unfolded {
            return card.into_any_element();
        }
        let input = self.readonly_streaming(format!("long-text:{key}"), text, false, cx);
        input.update(cx, |input, _| input.set_wrap(true));
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(card)
            .child(
                kit::block(t).child(
                    div()
                        .id(SharedString::from(format!("long-text-scroll:{key}")))
                        .w_full()
                        .max_h(px(LONG_TEXT_HEIGHT))
                        .overflow_y_scroll()
                        .px(px(12.))
                        .py(px(10.))
                        .font_family(crate::theme::mono())
                        .text_size(code_px(Settings::CODE_FONT_SIZE))
                        .line_height(relative(1.5))
                        .child(input),
                ),
            )
            .into_any_element()
    }

    /// Shows a long text that was pasted into the composer as a file beside the attachments.
    /// It can move into the draft, or leave the message.
    pub(super) fn folded_text_card(
        &self,
        index: usize,
        text: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        text_file_card(text, self.theme)
            .child(
                self.icon_button(("unfold-pasted-text", index), "pencil", "Edit in the draft")
                    .size(px(20.))
                    .on_click(cx.listener(move |s, _, window, cx| {
                        s.input.update(cx, |input, cx| input.unfold(index, cx));
                        window.focus(&s.input.read(cx).focus_handle(cx), cx);
                    })),
            )
            .child(
                self.icon_button(("remove-pasted-text", index), "close", "Remove text")
                    .size(px(20.))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.input
                            .update(cx, |input, cx| input.remove_folded(index, cx));
                    })),
            )
            .into_any_element()
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
        // A captured selection is a block that has only its title row.
        let mut title = kit::block_title(
            div()
                .flex_shrink(1.)
                .min_w_0()
                .text_ellipsis_start()
                .font_family(crate::theme::mono())
                .text_size(text_px(11.))
                .text_color(t.text)
                .child(feedback.label.clone()),
            t.accent,
            t,
        )
        .h(px(32.))
        .border_b_0()
        .child(figure(range, t.muted));
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
        kit::block(t)
            .h(px(34.))
            .flex_shrink_0()
            .child(title)
            .into_any_element()
    }

    /// Displays durable queued work and recoverable delivery failures as a row of the composer.
    /// Its header keeps its place and its messages scroll, so the draft stays in reach.
    pub(super) fn outgoing_tray(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = self.view.as_ref().filter(|v| !v.outgoing.is_empty()) else {
            return div().into_any_element();
        };
        let starting = view.starting_submission().map(|message| message.id.clone());
        let outgoing: Vec<_> = view
            .outgoing
            .iter()
            .filter(|message| {
                message.state != DeliveryState::Pending
                    && starting.as_ref() != Some(&message.id)
                    && !self.steer_arriving(message)
            })
            .cloned()
            .collect();
        let task = view.task.clone();
        if outgoing.is_empty() {
            return div().into_any_element();
        }
        let t = self.theme;
        let paused = !task.state.active()
            && starting.is_none()
            && outgoing.iter().any(|m| m.state == DeliveryState::Queued);
        let task_id = task.id.clone();
        let count = outgoing.len();
        // The header keeps its place above the messages, which scroll under it.
        let header = div().px(px(8.)).pt(px(6.)).pb(px(4.)).child(
            // The tray header reads as a section header: label, count, state, dotted rule.
            div()
                .relative()
                .h(px(24.))
                .flex_shrink_0()
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(fx::anchor(OUTBOX_ANCHOR))
                .child(icon("arrow-down").size(px(12.)).text_color(t.subtle))
                .child(caps("Next messages", t).text_color(t.muted))
                .child(figure(count.to_string(), t.muted))
                .when(paused, |d| d.child(kit::chip("PAUSED", t.warning)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(8.))
                        .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                )
                .when(paused, |d| {
                    d.child(
                        self.ghost("resume-queue", "arrow-right", "Resume")
                            .h(px(24.))
                            .disabled(task.state == RunState::DeliveryUnknown)
                            .on_click(cx.listener(move |s, _, _, _| {
                                s.send(core::Request::ResumeQueue {
                                    task_id: task_id.clone(),
                                })
                            })),
                    )
                }),
        );
        let mut tray = div()
            .id("outgoing-tray")
            .relative()
            .max_h(px(OUTBOX_HEIGHT))
            .overflow_y_scroll()
            .px(px(8.))
            .pb(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.));
        for (index, outgoing) in outgoing.into_iter().enumerate() {
            let id = outgoing.id.clone();
            let task_id = task.id.clone();
            let restore_task = task_id.clone();
            let restore_id = id.clone();
            let steer_task = task_id.clone();
            let steer_id = id.clone();
            let expanded_key = format!("outgoing:{id}");
            let expanded = self.folded.contains(&expanded_key);
            let (label, tone) = match outgoing.state {
                DeliveryState::Queued => ("Queued", t.subtle),
                DeliveryState::Pending => ("Sending…", t.accent),
                DeliveryState::Failed => ("Not sent", t.removed),
                DeliveryState::Unknown => ("Delivery unknown", t.warning),
            };
            let mut row = kit::block(t)
                .relative()
                .flex_shrink_0()
                .child(fx::anchor(outgoing_anchor(&id)))
                .p(px(4.))
                .flex()
                .flex_col()
                .gap(px(4.))
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
                                (index + 1).to_string(),
                            )
                            .h(px(22.))
                            .text_size(text_px(10.5))
                            .accessibility_label(format!("{} · {label}", index + 1))
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    if !s.folded.remove(&expanded_key) {
                                        s.folded.insert(expanded_key.clone());
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .child(kit::chip(label.to_uppercase(), tone))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(text_px(12.))
                                .child(if outgoing.text.is_empty() {
                                    "Attached context".into()
                                } else {
                                    // One line of the message, so a long one is not laid out
                                    // whole for a label.
                                    first_line(&outgoing.text)
                                }),
                        )
                        .when(
                            outgoing.state == DeliveryState::Queued
                                && self.selected_runtime().steering,
                            |d| {
                                d.child(
                                    self.icon_button(
                                        ("steer-queued", index),
                                        "arrow-right",
                                        "Steer immediately",
                                    )
                                    .size(px(22.))
                                    .disabled(
                                        !task.state.active()
                                            || task.active_turn.is_none()
                                            || outgoing.expected_turn.is_some(),
                                    )
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| {
                                            s.steer_queued(
                                                steer_task.clone(),
                                                steer_id.clone(),
                                                cx,
                                            );
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
                                        ("restore-queued", index),
                                        "edit",
                                        "Restore to empty composer",
                                    )
                                    .size(px(22.))
                                    .disabled(
                                        !self.input.read(cx).is_empty()
                                            || !task.attachments.is_empty()
                                            || !task.feedback.is_empty()
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
                    d.child(
                        div()
                            .px(px(6.))
                            .font_family(crate::theme::mono())
                            .text_size(text_px(11.))
                            .text_color(t.removed)
                            .child(error),
                    )
                });
            if expanded {
                let text = self.readonly(
                    format!("outgoing-text:{}", outgoing.id),
                    outgoing.text.clone(),
                    cx,
                );
                row = row
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
                    .child(
                        div()
                            .px(px(6.))
                            .py(px(2.))
                            .text_size(text_px(12.))
                            .child(text),
                    );
                for attachment in &outgoing.attachments {
                    row = row.child(self.attachment_card(attachment, cx));
                }
                for feedback in &outgoing.feedback {
                    row = row.child(self.context_card(feedback, None, cx));
                }
            } else if !outgoing.attachments.is_empty() || !outgoing.feedback.is_empty() {
                row = row.child(div().px(px(6.)).child(figure(
                    format!(
                        "{} attachments · {} code selections",
                        outgoing.attachments.len(),
                        outgoing.feedback.len()
                    ),
                    t.subtle,
                )));
            }
            tray = tray.child(
                if let Some(arrival) = self.send_arrivals.get(&outgoing.id).filter(|a| a.queued) {
                    motion::send_transition(
                        row,
                        arrival,
                        cx.reduce_motion() || self.panel != Panel::None,
                    )
                } else {
                    row.into_any_element()
                },
            );
        }
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(header)
            .child(tray)
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .into_any_element()
    }
}

/// Most height of an unfolded long message of the user. A longer text scrolls inside it.
const LONG_TEXT_HEIGHT: f32 = 420.;
/// Characters of a text's first line that a label of one line shows.
const LONG_TEXT_TITLE: usize = 120;

// Returns the start of a text's first line that is not blank, for a label of one line.
fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .take(LONG_TEXT_TITLE)
        .collect()
}

// The card of a long text: a file icon, the text's first line and the text's size. Actions
// follow it.
fn text_file_card(text: &str, t: Theme) -> Div {
    let title = first_line(text);
    kit::block(t)
        .w_auto()
        .flex()
        .items_center()
        .gap(px(8.))
        .p(px(4.))
        .pr(px(6.))
        .flex_shrink_0()
        .max_w(px(320.))
        .child(kit::icon_tile("file-text", t.accent, 28., false))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .font_family(crate::theme::mono())
                        .text_size(text_px(11.5))
                        .text_ellipsis()
                        .child(title),
                )
                .child(caps(format!("Text · {}", file_size(text.len() as u64)), t)),
        )
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
