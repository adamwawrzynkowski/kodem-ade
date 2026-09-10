use super::*;
use crate::markdown;
impl Shell {
    /// Builds the conversation with a measured floating composer and a clear review viewport.
    pub(super) fn chat(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut panel = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.canvas)
            .child(motion::window_texture(self.settings.light))
            .child(motion::chat_dither(self.settings.light, cx.reduce_motion()));
        if let Some(view) = self.view.clone() {
            self.chat_selection.update(cx, |doc, _| {
                doc.set_viewport(
                    self.chat_scroll.clone(),
                    view.messages.iter().map(|m| m.seq).collect(),
                    px(self.composer_height + 16.),
                )
            });
            panel = panel.child(self.chat_selection.clone());
            let state = &view.task.state;
            let state_color = match state {
                RunState::Completed => t.added,
                RunState::Failed | RunState::DeliveryUnknown => t.removed,
                RunState::Waiting | RunState::Running | RunState::Starting => t.accent,
                _ => t.muted,
            };
            let center_diff =
                self.reviewing_file && self.settings.diff_placement == DiffPlacement::Conversation;
            if !center_diff {
                panel = panel.child(
                    div()
                        .h(px(28.))
                        .flex_shrink_0()
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1())
                        .child(if state.active() {
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(px(10.))
                                .child(motion::active_icon(
                                    "task-state-icon",
                                    if state == &RunState::Waiting {
                                        "shield"
                                    } else {
                                        "loader"
                                    },
                                    state_color,
                                    state != &RunState::Waiting,
                                ))
                                .child(motion::gleam("task-state-label", state.label(), t))
                                .into_any_element()
                        } else {
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(px(10.))
                                .text_color(state_color)
                                .child(
                                    icon(match state {
                                        RunState::Completed => "check",
                                        RunState::Idle => "chat",
                                        RunState::Interrupted => "stop",
                                        RunState::DeliveryUnknown => "shield",
                                        _ => "circle-x",
                                    })
                                    .size(px(12.)),
                                )
                                .child(state.label())
                                .into_any_element()
                        })
                        .child(
                            self.icon_button("task-menu", "ellipsis", "Task actions")
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.panel = Panel::TaskMenu;
                                    cx.notify();
                                })),
                        ),
                );
            }
            if state == &RunState::DeliveryUnknown {
                let id = view.task.id.clone();
                panel = panel.child(
                    div()
                        .mx_5()
                        .mb_3()
                        .p_3()
                        .rounded(px(8.))
                        .bg(t.accent_bg)
                        .flex()
                        .items_center()
                        .gap_3()
                        .text_size(px(12.))
                        .child(icon("refresh").text_color(t.accent))
                        .child(div().flex_1().child(
                            "Delivery needs confirmation before another message can be sent.",
                        ))
                        .child(
                            self.button("reconcile", "refresh", "Check history")
                                .on_click(cx.listener(move |s, _, _, _| {
                                    s.send(core::Request::Reconcile {
                                        task_id: id.clone(),
                                    })
                                })),
                        ),
                );
            }
            if let Some(error) = &view.task.error {
                panel = panel.child(
                    div()
                        .mx_5()
                        .mb_2()
                        .p_3()
                        .rounded(px(7.))
                        .bg(t.removed_bg)
                        .text_size(px(12.))
                        .text_color(t.removed)
                        .child(error.clone()),
                );
            }
            if !view.requests.is_empty() {
                panel = panel.child(
                    div()
                        .mx_5()
                        .mb_3()
                        .px_3()
                        .py_2()
                        .rounded(px(8.))
                        .bg(t.accent_bg)
                        .border_1()
                        .border_color(t.accent)
                        .flex()
                        .items_center()
                        .gap_3()
                        .text_size(px(12.))
                        .child(icon("shield").text_color(t.accent))
                        .child(div().flex_1().child("Codex is waiting for your input"))
                        .child(self.primary("open-requests", "shield", "Respond").on_click(
                            cx.listener(|s, _, _, cx| {
                                s.panel = Panel::Questions;
                                cx.notify();
                            }),
                        )),
                );
            }
            let mut timeline = div()
                .id("conversation-stage")
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .on_scroll_wheel(cx.listener(|s, e: &ScrollWheelEvent, _, cx| {
                    let delta = e.delta.pixel_delta(px(20.));
                    s.nudge_composer((f32::from(delta.y) * 0.015).clamp(-0.45, 0.45), cx);
                }));
            if center_diff {
                timeline = timeline
                    .child(self.review(cx, true))
                    .child(div().h(px(self.composer_height)).flex_shrink_0());
            } else {
                if let Some(before) = view.before {
                    let id = view.task.id.clone();
                    timeline = timeline.child(
                        div().px_5().pb_2().child(
                            self.ghost("older", "arrow-up", "Earlier messages")
                                .on_click(cx.listener(move |s, _, _, _| {
                                    s.send(core::Request::Task {
                                        task_id: id.clone(),
                                        before: Some(before),
                                    })
                                })),
                        ),
                    );
                }
                if view.messages.is_empty() {
                    timeline=timeline.child(div().flex_1().px_8().pt_8().child(div().max_w(px(600.)).text_size(px(13.)).text_color(t.muted)
                    .child("Describe the change below. Your conversation and its code review stay together in this task.")));
                } else {
                    let entity = cx.entity();
                    timeline = timeline.child(
                        div()
                            .w_full()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .justify_center()
                            .child(
                                list(self.chat_scroll.clone(), move |index, window, cx| {
                                    entity.update(cx, |s, cx| s.message(index, window, cx))
                                })
                                .w_full()
                                .max_w(px(940.))
                                .min_w_0()
                                .h_full()
                                .pb(px(self.composer_height + 16.)),
                            ),
                    );
                }
            }
            let bounds_entity = cx.entity();
            let floating = div().absolute().bottom_0().w_full().child(
                div()
                    .relative()
                    .w_full()
                    .child(
                        div()
                            .px_5()
                            .h(px(if self.follow { 4. } else { 24. }))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_end()
                            .when(!self.follow, |d| {
                                d.child(
                                    self.ghost("jump-latest", "arrow-down", "Jump to latest")
                                        .on_click(cx.listener(|s, _, _, cx| {
                                            s.follow = true;
                                            s.chat_scroll.scroll_to_end();
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    .child(self.composer(false, cx))
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                bounds_entity.update(cx, |s, cx| {
                                    let height = f32::from(bounds.size.height).ceil();
                                    if (s.composer_height - height).abs() > 0.5 {
                                        s.composer_height = height;
                                        cx.notify();
                                    }
                                });
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
            );
            panel = panel.child(timeline.child(floating));
        } else {
            panel = panel.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .id("new-task-scroll")
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(840.))
                            .mx_auto()
                            .px_8()
                            .pt(px(92.))
                            .child(
                                div()
                                    .text_size(px(24.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .mb_2()
                                    .child("New task"),
                            )
                            .child(div().text_size(px(14.)).text_color(t.muted).mb_6().child(
                                "Choose a repository and describe what you want to work on.",
                            ))
                            .child(self.composer(true, cx))
                            .child(
                                div()
                                    .px_2()
                                    .pt_4()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_size(px(11.))
                                    .text_color(t.subtle)
                                    .child(icon("branch").size(px(13.)))
                                    .child(
                                        "An isolated worktree keeps this task's changes separate.",
                                    ),
                            ),
                    ),
            );
        }
        if self.show_terminal && self.selected.is_some() {
            panel = panel.child(self.terminal_panel(window, cx));
        }
        panel.into_any_element()
    }
    fn composer(&mut self, empty: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let view = self.view.clone();
        let active = view.as_ref().is_some_and(|v| v.task.state.active());
        let unknown = view
            .as_ref()
            .is_some_and(|v| v.task.state == RunState::DeliveryUnknown);
        let project = self
            .workspace
            .as_ref()
            .and_then(|w| {
                w.projects
                    .iter()
                    .find(|p| Some(&p.id) == self.selected_project.as_ref())
                    .or(w.projects.first())
            })
            .cloned();
        let model = view
            .as_ref()
            .and_then(|v| v.task.model.as_ref())
            .map(|id| {
                self.workspace
                    .as_ref()
                    .and_then(|w| w.runtime.models.iter().find(|m| &m.id == id))
                    .map_or_else(|| id.clone(), |m| m.name.clone())
            })
            .unwrap_or_else(|| "Codex default".into());
        let effort = view.as_ref().and_then(|v| v.task.effort.clone());
        let mut wrapper = div()
            .w_full()
            .flex_shrink_0()
            .max_w(px(940.))
            .mx_auto()
            .when(!empty, |d| d.px_5().pb_2());
        wrapper = wrapper.child(self.outgoing_tray(cx));
        let mut attachments = div()
            .id("composer-context")
            .max_h(px(160.))
            .flex_shrink_0()
            .restrict_scroll_to_axis()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2();
        if let Some(view) = &view {
            for feedback in &view.task.feedback {
                attachments =
                    attachments.child(self.context_card(feedback, Some(&view.task.id), cx));
            }
            let mut files = div().flex().flex_wrap().flex_shrink_0().gap_2();
            for attachment in &view.task.attachments {
                files = files.child(self.attachment_card(attachment, Some(&view.task.id), cx));
            }
            attachments = attachments.child(files);
        } else {
            for (index, source) in self.pending_attachments.iter().enumerate() {
                let skill = matches!(source, AttachmentSource::Skill(_));
                let path = source.path();
                attachments = attachments.child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2()
                        .p_2()
                        .bg(t.sidebar)
                        .rounded(px(7.))
                        .child(icon(if skill { "code" } else { "paperclip" }).text_color(t.accent))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(px(11.))
                                .child(
                                    (if skill {
                                        path.parent().and_then(|p| p.file_name())
                                    } else {
                                        path.file_name()
                                    })
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                                ),
                        )
                        .child(
                            self.icon_button(
                                ("remove-pending-file", index),
                                "close",
                                "Remove attachment",
                            )
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    let source = s.pending_attachments.remove(index);
                                    s.clipboard_images.remove(source.path());
                                    cx.notify();
                                },
                            )),
                        )
                        .when(skill, |d| {
                            d.child(div().text_size(px(10.)).text_color(t.muted).child("Skill"))
                        }),
                );
            }
        }
        let importing = self.pasting_images > 0
            || self
                .selected
                .as_ref()
                .is_some_and(|id| self.importing_tasks.get(id).copied().unwrap_or(0) > 0);
        if importing {
            attachments = attachments.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .child(motion::active_icon("import-icon", "loader", t.accent, true))
                    .child(motion::gleam("import-label", "Importing attachments…", t)),
            );
        }
        let has_context = importing
            || view.as_ref().map_or_else(
                || !self.pending_attachments.is_empty(),
                |v| !v.task.feedback.is_empty() || !v.task.attachments.is_empty(),
            );
        let mut editor = div()
            .id("composer-drop-target")
            .relative()
            .top(px(self.composer_spring.offset))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| s.nudge_composer(0.65, cx)),
            )
            .drag_over::<ExternalPaths>(move |style, _, _, _| {
                style.border_color(t.accent).bg(t.accent_bg)
            })
            .on_drop(cx.listener(|s, paths: &ExternalPaths, _, cx| {
                s.attach_paths(paths.paths().to_vec(), cx)
            }))
            .rounded(px(motion::COMPOSER_RADIUS))
            .border_1()
            .border_color(t.border)
            .shadow(t.elevation())
            .overflow_hidden()
            .child(motion::composer_backdrop(t))
            .when(!empty, |d| {
                d.child(self.activity_strip(cx))
                    .child(self.changed_files_strip(cx))
            })
            .when(has_context, |d| {
                d.child(div().px_3().pt_3().child(attachments))
            });
        if empty {
            let label = project
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Choose repository".into());
            editor = editor.child(
                div()
                    .px_3()
                    .pt_3()
                    .pb_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        self.ghost("composer-repo", "folder-closed", label)
                            .bg(t.sidebar)
                            .text_color(t.text)
                            .child(icon("chevron-down").size(px(12.)))
                            .on_click(cx.listener(|s, _, w, cx| {
                                if s.workspace.as_ref().is_none_or(|w| w.projects.is_empty()) {
                                    s.open_project(&OpenProject, w, cx);
                                } else {
                                    s.empty_draft = s.input.read(cx).text();
                                    s.panel = Panel::NewTask;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        self.ghost(
                            "composer-worktree",
                            "branch",
                            if self.new_worktree {
                                "New worktree"
                            } else {
                                "Current checkout"
                            },
                        )
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.new_worktree = !s.new_worktree;
                            cx.notify();
                        })),
                    ),
            );
        }
        editor = editor.child(
            div()
                .id("composer-editor")
                .px_3()
                .pt_3()
                .pb_2()
                .min_h(px(if empty { 86. } else { 58. }))
                .max_h(px(180.))
                .overflow_y_scroll()
                .text_size(px(self.settings.font_size - 1.))
                .text_color(t.text)
                .child(self.input.clone()),
        );
        let mut toolbar = div()
            .flex_wrap()
            .px_3()
            .pb_2()
            .flex()
            .items_center()
            .gap_1()
            .child(
                self.icon_button("composer-files", "paperclip", "Attach files or images")
                    .on_click(cx.listener(|s, _, _, cx| s.choose_attachments(cx))),
            )
            .child(
                self.icon_button("composer-skills", "code", "Skills, instructions and MCP")
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.panel = Panel::Resources;
                        s.send(core::Request::Resources {
                            task_id: s.selected.clone(),
                        });
                        cx.notify();
                    })),
            )
            .child(self.usage_meters())
            .child(div().flex_1())
            .child(
                self.ghost("composer-model", "bot", "")
                    .child(div().max_w(px(150.)).min_w_0().text_ellipsis().child(model))
                    .text_size(px(11.))
                    .child(icon("chevron-down").size(px(11.)))
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.panel = Panel::Models;
                        if s.workspace
                            .as_ref()
                            .is_none_or(|w| w.runtime.models.is_empty())
                        {
                            s.send(core::Request::ConnectRuntime);
                        }
                        cx.notify();
                    })),
            )
            .when_some(effort, |d, e| {
                d.child(div().w(px(1.)).h(px(14.)).mx_1().bg(t.border))
                    .child(
                        self.ghost("composer-effort", "settings-2", e)
                            .text_size(px(11.))
                            .child(icon("chevron-down").size(px(10.)))
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.panel = Panel::Reasoning;
                                cx.notify();
                            })),
                    )
            })
            .when_some(view.as_ref(), |d, view| {
                d.child(div().w(px(1.)).h(px(14.)).mx_1().bg(t.border))
                    .child(
                        self.ghost(
                            "composer-access",
                            match view.task.access {
                                AccessMode::FullAccess => "lock-open",
                                AccessMode::AutoReview => "bot",
                                _ => "shield",
                            },
                            view.task.access.label(),
                        )
                        .text_size(px(11.))
                        .child(icon("chevron-down").size(px(10.)))
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.panel = Panel::Access;
                            cx.notify();
                        })),
                    )
            });
        if empty {
            toolbar = toolbar.child(
                self.primary("create-from-composer", "plus", "Create task")
                    .h(px(30.))
                    .disabled(project.is_none() || self.start_after_create || importing)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Some(project) = &project {
                            s.empty_draft = s.input.read(cx).text();
                            s.send(core::Request::CreateTask {
                                project_id: project.id.clone(),
                                worktree: s.new_worktree,
                            });
                            s.status = "Creating task…".into();
                            cx.notify();
                        }
                    })),
            );
        }
        if !empty {
            if active {
                toolbar = toolbar.child(
                    self.ghost(
                        "active-send-mode",
                        if self.settings.send_mode == SendMode::Queue {
                            "arrow-down"
                        } else {
                            "arrow-right"
                        },
                        if self.settings.send_mode == SendMode::Queue {
                            "Queue"
                        } else {
                            "Steer"
                        },
                    )
                    .text_size(px(11.))
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.settings.send_mode = if s.settings.send_mode == SendMode::Queue {
                            SendMode::Steer
                        } else {
                            SendMode::Queue
                        };
                        s.save_settings();
                        cx.notify();
                    })),
                );
            }
            let disabled = !active
                && (unknown
                    || importing
                    || self.submitted_draft.is_some()
                    || !self.has_composer_content(cx));
            toolbar = toolbar.child(
                self.icon_button(
                    "composer-submit",
                    if active { "stop" } else { "arrow-up" },
                    if active {
                        "Stop current turn"
                    } else {
                        "Send message · Enter"
                    },
                )
                .relative()
                .overflow_hidden()
                .h(px(32.))
                .w(px(34.))
                .rounded(px(8.))
                .border_1()
                .border_color(if disabled {
                    t.border
                } else {
                    t.text.opacity(0.18)
                })
                .shadow(vec![
                    BoxShadow {
                        inset: true,
                        color: rgb_to_hsla(rgb(0xffffff)).opacity(0.22),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(0.),
                        spread_radius: px(0.),
                    },
                    BoxShadow {
                        inset: true,
                        color: t.shadow.opacity(0.35),
                        offset: point(px(0.), px(-2.)),
                        blur_radius: px(1.),
                        spread_radius: px(0.),
                    },
                    BoxShadow {
                        inset: false,
                        color: t.shadow,
                        offset: point(px(0.), px(2.)),
                        blur_radius: px(3.),
                        spread_radius: px(-1.),
                    },
                ])
                .child(motion::button_dither())
                .hover(move |b| {
                    b.bg(if disabled {
                        t.raised
                    } else {
                        t.accent.opacity(0.88)
                    })
                    .text_color(if disabled { t.subtle } else { t.inset })
                })
                .active(move |b| b.bg(t.accent).shadow(Vec::new()))
                .bg(if disabled { t.raised } else { t.accent })
                .text_color(if disabled { t.subtle } else { t.inset })
                .disabled(disabled)
                .when(disabled, |b| b.cursor_default())
                .on_click(cx.listener(|s, _, w, cx| {
                    if let Some(view) = s.view.as_ref().filter(|v| v.task.state.active()) {
                        s.send(core::Request::Interrupt {
                            task_id: view.task.id.clone(),
                        });
                    } else {
                        s.submit(&SendPrompt, w, cx);
                    }
                })),
            );
        }
        editor = editor.child(toolbar);
        wrapper = wrapper.child(editor);
        if !empty {
            wrapper = wrapper.child(
                div()
                    .pt_2()
                    .px_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(10.))
                    .text_color(t.subtle)
                    .child(icon("branch").size(px(12.)))
                    .child(if view.as_ref().is_some_and(|v| v.task.owns_worktree) {
                        "Isolated worktree"
                    } else {
                        "Current checkout"
                    })
                    .child(div().flex_1())
                    .child(if active {
                        if self.settings.send_mode == SendMode::Queue {
                            "Enter to queue · Shift+Enter for newline"
                        } else {
                            "Enter to steer · Shift+Enter for newline"
                        }
                    } else {
                        "Enter to send · Shift+Enter for newline"
                    }),
            );
        }
        wrapper.into_any_element()
    }
    fn message(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.selection_inputs = Some(Vec::new());
        let content = self.message_content(index, window, cx);
        let inputs = self.selection_inputs.take().unwrap_or_default();
        if let Some(message) = self.view.as_ref().and_then(|view| view.messages.get(index)) {
            let order = message.seq;
            self.chat_selection
                .update(cx, |doc, cx| doc.register(order, inputs, cx));
        }
        content
    }
    fn message_content(
        &mut self,
        index: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(message) = self
            .view
            .as_ref()
            .and_then(|v| v.messages.get(index))
            .cloned()
        else {
            return div().into_any_element();
        };
        let t = self.theme;
        let user = message.kind == MessageKind::User;
        if message.kind == MessageKind::Tool {
            return self.tool_calls(index, cx);
        }
        let message_active = self.view.as_ref().is_some_and(|v| {
            v.task.state == RunState::Running && v.activities.iter().any(|a| a.id == message.id)
        });
        let text = message.text.clone();
        let mut outer = div().w_full().px_5().py_2();
        let mut block = div().w_full();
        if user {
            outer = outer.flex().justify_end();
            let line_width = message
                .text
                .lines()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(1) as f32
                * (self.settings.font_size - 1.)
                * 0.55
                + 24.;
            block = block
                .w(px(line_width.max(
                    if message.attachments.is_empty() && message.feedback.is_empty() {
                        52.
                    } else {
                        380.
                    },
                )))
                .max_w(relative(0.86))
                .bg(t.raised)
                .shadow(t.elevation())
                .rounded(px(9.))
                .px_3()
                .py_2();
        } else if message.kind != MessageKind::Assistant {
            let (name, glyph, color) = match message.kind {
                MessageKind::Error => ("Error", "circle-x", t.removed),
                MessageKind::Check => ("Check result", "check-square", t.added),
                MessageKind::Tool => ("Tool output", "square-terminal", t.muted),
                _ => ("Activity", "chat", t.muted),
            };
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_1()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(if message_active {
                        motion::active_icon(
                            (
                                ElementId::from("message-icon"),
                                SharedString::from(message.id.clone()),
                            ),
                            glyph,
                            color,
                            false,
                        )
                    } else {
                        icon(glyph)
                            .size(px(15.))
                            .text_color(color)
                            .into_any_element()
                    })
                    .child(if message_active {
                        motion::gleam(
                            (
                                ElementId::from("message-label"),
                                SharedString::from(message.id.clone()),
                            ),
                            name,
                            t,
                        )
                    } else {
                        div().child(name).into_any_element()
                    })
                    .when(message_active, |d| {
                        d.child(
                            div()
                                .text_size(px(10.))
                                .text_color(t.accent)
                                .child("Working…"),
                        )
                    })
                    .child(div().flex_1()),
            );
        }
        if !message.attachments.is_empty() {
            let mut files = div().flex().flex_wrap().gap_2().mb_2();
            for attachment in &message.attachments {
                files = files.child(self.attachment_card(attachment, None, cx));
            }
            block = block.child(files);
        }
        for feedback in &message.feedback {
            block = block.child(self.context_card(feedback, None, cx));
        }
        block = block.child(div().text_size(px(self.settings.font_size - 1.)).child(
            self.message_blocks(
                &format!("message:{}", message.id),
                markdown::blocks(&message.text),
                cx,
            ),
        ));
        if !user {
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mt_1()
                    .child(
                        self.icon_button(
                            (
                                ElementId::from("copy-message"),
                                SharedString::from(message.id.clone()),
                            ),
                            "copy",
                            "Copy message",
                        )
                        .w(px(18.))
                        .h(px(18.))
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                        }),
                    )
                    .child(div().text_size(px(10.)).text_color(t.subtle).child(
                        if message_active {
                            motion::gleam(
                                (
                                    ElementId::from("streaming"),
                                    SharedString::from(message.id.clone()),
                                ),
                                if message.kind == MessageKind::Assistant {
                                    "Writing…"
                                } else {
                                    "In progress…"
                                },
                                t,
                            )
                        } else {
                            div().into_any_element()
                        },
                    )),
            );
        }
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(outer.child(block))
            .into_any_element()
    }
}
