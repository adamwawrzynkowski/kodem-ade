use super::*;
impl Shell {
    /// Builds the conversation with a measured floating composer and a clear review viewport.
    pub(super) fn chat(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sync_slash(cx);
        self.sync_context_arrivals(window, cx);
        let t = self.theme;
        let mut panel = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.canvas);
        if let Some(view) = &self.view {
            self.chat_selection.update(cx, |doc, _| {
                doc.set_viewport(
                    self.chat_scroll.clone(),
                    view.messages.iter().map(|m| m.seq).collect(),
                    px(self.composer_height + 8.),
                )
            });
            panel = panel.child(self.chat_selection.clone());
            let state = &view.task.state;
            let center_diff =
                self.reviewing_file && self.settings.diff_placement == DiffPlacement::Conversation;
            if !center_diff {
                panel = panel.child(
                    div()
                        .h(px(36.))
                        .flex_shrink_0()
                        .pl(px(20.))
                        .pr(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .min_w_0()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_ellipsis()
                                .child(view.task.title.clone()),
                        )
                        .child(state_caps(&view.task, t))
                        .child(div().flex_1())
                        .child(
                            self.icon_button("task-menu", "ellipsis", "Task actions")
                                .on_click_with_bounds(cx.listener(
                                    |s, bounds: &Bounds<Pixels>, _, cx| {
                                        s.menu_anchor = Some(bounds.top_right());
                                        s.panel = Panel::TaskMenu;
                                        cx.notify();
                                    },
                                )),
                        ),
                );
            }
            if state == &RunState::DeliveryUnknown {
                let id = view.task.id.clone();
                panel = panel.child(
                    notice(t, t.accent, "Delivery")
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
                    notice(t, t.removed, "Error").child(div().flex_1().child(error.clone())),
                );
            }
            if view
                .requests
                .iter()
                .any(|r| self.request_in_current_thread(r) && r.method.ends_with("requestApproval"))
            {
                panel = panel.child(
                    notice(t, t.accent, "Approval")
                        .child(
                            div()
                                .flex_1()
                                .child("The provider needs your permission to continue."),
                        )
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
                    timeline = timeline.child(
                        div()
                            .flex_1()
                            .px(px(20.))
                            .pt(px(24.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(caps("Empty task", t))
                            .child(
                                div()
                                    .max_w(px(560.))
                                    .text_size(px(13.))
                                    .text_color(t.muted)
                                    .child("Describe the change below. The conversation and its code review stay together in this task."),
                            ),
                    );
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
                                list(self.chat_scroll.clone(), move |index, _, cx| {
                                    entity.update(cx, |s, cx| {
                                        s.conversation_message(&Conversation::Main, index, cx)
                                    })
                                })
                                .w_full()
                                .max_w(px(900.))
                                .min_w_0()
                                .h_full()
                                .pb(px(self.composer_height + 8.)),
                            ),
                    );
                }
                timeline = timeline.child(dither::fade(t.canvas, 14., false).top_0());
            }
            let bounds_entity = cx.entity();
            let floating = div().absolute().bottom_0().w_full().child(
                div()
                    .relative()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .relative()
                            .h(px(28.))
                            .flex_shrink_0()
                            .child(dither::fade(t.canvas, 28., true).top_0())
                            .when(!self.follow, |d| {
                                d.child(
                                    div().absolute().right(px(20.)).top(px(2.)).child(
                                        self.button("jump-latest", "arrow-down", "Latest")
                                            .h(px(24.))
                                            .on_click(cx.listener(|s, _, _, cx| {
                                                s.follow = true;
                                                s.chat_scroll.scroll_to_end();
                                                cx.notify();
                                            })),
                                    ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .w_full()
                            .bg(t.canvas)
                            .child(self.composer(false, window, cx)),
                    )
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
                            .max_w(px(780.))
                            .mx_auto()
                            .px(px(32.))
                            .pt(px(56.))
                            .pb(px(32.))
                            .child(self.empty_chat_greeting(cx))
                            .child(self.composer(true, window, cx)),
                    ),
            );
        }
        if self.show_terminal && self.selected.is_some() {
            panel = panel.child(self.terminal_panel(window, cx));
        }
        panel.into_any_element()
    }
    fn empty_chat_greeting(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let project = self.workspace.as_ref().and_then(|w| {
            w.projects
                .iter()
                .find(|p| Some(&p.id) == self.selected_project.as_ref())
                .or(w.projects.first())
        });
        let name = project
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "a project".into());
        div()
            .w_full()
            .mb(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(132.))
                    .child(dither::field(t.accent.opacity(0.8), cx.reduce_motion())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(caps(format!("Hello, {}", self.greeting_name), t))
                    .child(div().flex_1().h(px(1.)).bg(t.border))
                    .child(caps("New task", t)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .gap(px(7.))
                    .text_size(px(24.))
                    .line_height(relative(1.25))
                    .font_weight(FontWeight::MEDIUM)
                    .child("What should we build in")
                    .child(
                        self.control("greeting-project")
                            .h_auto()
                            .px(px(4.))
                            .py(px(1.))
                            .border_0()
                            .shadow(Vec::new())
                            .bg(t.accent_bg)
                            .hover(move |b| b.bg(t.selected))
                            .text_size(px(24.))
                            .line_height(relative(1.25))
                            .font_weight(FontWeight::MEDIUM)
                            .max_w_full()
                            .min_w_0()
                            .text_ellipsis()
                            .text_color(t.accent)
                            .cursor_pointer()
                            .accessibility_label("Choose project")
                            .child(name)
                            .child(icon("chevrons-up-down").size(px(16.)))
                            .on_click_with_bounds(cx.listener(
                                |s, bounds: &Bounds<Pixels>, _, cx| {
                                    if !s.creating_task {
                                        s.menu_anchor = Some(bounds.top_right());
                                        s.panel = Panel::ChatProjects;
                                        cx.notify();
                                    }
                                },
                            )),
                    )
                    .child("?"),
            )
            .into_any_element()
    }
    fn composer(&mut self, empty: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let task = self.view.as_ref().map(|view| view.task.clone());
        let active = task.as_ref().is_some_and(|v| v.state.active());
        let unknown = task
            .as_ref()
            .is_some_and(|v| v.state == RunState::DeliveryUnknown);
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
        let preferences = self.composer_preferences();
        let runtime = self.selected_runtime();
        let selected_model = runtime
            .models
            .iter()
            .find(|model| preferences.model.as_ref() == Some(&model.id));
        let has_reasoning = selected_model.is_some_and(|model| !model.efforts.is_empty());
        let access_label = if preferences.provider == Provider::Codex {
            preferences.access.label().to_owned()
        } else {
            runtime
                .access_modes
                .iter()
                .find(|mode| preferences.provider_access.as_ref() == Some(&mode.id))
                .map(|mode| mode.name.clone())
                .unwrap_or_else(|| "Provider default".into())
        };
        let model = preferences
            .model
            .as_ref()
            .map(|id| {
                runtime
                    .models
                    .iter()
                    .find(|m| &m.id == id)
                    .map_or_else(|| id.clone(), |m| m.name.clone())
            })
            .unwrap_or_else(|| format!("{} default", preferences.provider.label()));
        let effort = preferences
            .effort
            .clone()
            .unwrap_or_else(|| "Reasoning".into());
        let mut wrapper = div()
            .relative()
            .w_full()
            .flex_shrink_0()
            .max_w(px(900.))
            .mx_auto()
            .when(!empty, |d| d.px(px(20.)).pb(px(10.)));
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
        if let Some(task) = &task {
            for feedback in &task.feedback {
                attachments = attachments.child(motion::context_reveal(
                    self.context_card(feedback, Some(&task.id), cx),
                    self.context_arrivals.get(&feedback.id).copied().flatten(),
                    cx.reduce_motion(),
                ));
            }
            let mut files = div().flex().flex_wrap().flex_shrink_0().gap_2();
            for attachment in &task.attachments {
                files = files.child(motion::context_reveal(
                    self.attachment_card(attachment, Some(&task.id), cx),
                    self.context_arrivals.get(&attachment.id).copied().flatten(),
                    cx.reduce_motion(),
                ));
            }
            attachments = attachments.child(files);
        } else {
            for (index, source) in self.pending_attachments.iter().enumerate() {
                let skill = matches!(source, AttachmentSource::Skill(_));
                let path = source.path();
                attachments = attachments.child(motion::context_reveal(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2()
                        .pl(px(8.))
                        .pr(px(2.))
                        .py(px(2.))
                        .bg(t.inset)
                        .border_1()
                        .border_color(t.border)
                        .rounded(px(crate::theme::RADIUS_CONTROL))
                        .child(
                            icon(if skill { "code" } else { "paperclip" })
                                .size(px(13.))
                                .text_color(t.accent),
                        )
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
                        .when(skill, |d| d.child(caps("Skill", t))),
                    self.context_arrivals
                        .get(&format!("pending:{source:?}"))
                        .copied()
                        .flatten(),
                    cx.reduce_motion(),
                ));
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
                    .child(motion::active_icon(
                        "import-icon",
                        "loader",
                        t.accent,
                        true,
                        cx.reduce_motion(),
                    ))
                    .child(motion::gleam("import-label", "Importing attachments…", t)),
            );
        }
        let has_context = importing
            || task.as_ref().map_or_else(
                || !self.pending_attachments.is_empty(),
                |v| !v.feedback.is_empty() || !v.attachments.is_empty(),
            );
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        let mut editor = div()
            .id("composer-drop-target")
            .relative()
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
            .bg(t.raised)
            .border_1()
            .border_color(if focused {
                t.accent.opacity(0.55)
            } else {
                t.edge
            })
            .shadow(t.elevation())
            .overflow_hidden()
            .when(!empty, |d| {
                d.child(self.subagents_strip(cx))
                    .child(self.activity_strip(cx))
                    .child(self.changed_files_strip(cx))
                    .child(self.question_panel(cx))
            })
            .when(has_context, |d| {
                d.child(div().px(px(12.)).pt(px(10.)).child(attachments))
            });
        if empty {
            editor = editor.child(
                div()
                    .px(px(8.))
                    .pt(px(8.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        self.ghost(
                            "composer-worktree",
                            "branch",
                            if self.composer_worktree() {
                                "New worktree"
                            } else {
                                "Current checkout"
                            },
                        )
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.new_worktree_override = Some(!s.composer_worktree());
                            cx.notify();
                        })),
                    ),
            );
        }
        editor = editor.child(
            div()
                .id("composer-editor")
                .px(px(14.))
                .pt(px(12.))
                .pb(px(6.))
                .min_h(px(if empty { 92. } else { 56. }))
                .max_h(px(180.))
                .overflow_y_scroll()
                .text_size(px(self.settings.font_size - 1.))
                .text_color(t.text)
                .child(self.input.clone()),
        );
        let mut toolbar = div()
            .flex_wrap()
            .pl(px(8.))
            .pr(px(8.))
            .pb(px(8.))
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                self.icon_button("composer-files", "paperclip", "Attach files or images")
                    .on_click(cx.listener(|s, _, _, cx| s.choose_attachments(cx))),
            )
            .child(
                self.icon_button("composer-skills", "code", "Skills, instructions and MCP")
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.panel = Panel::Resources;
                        s.send(core::Request::Resources {
                            provider: s.composer_preferences().provider,
                            project_id: s.selected_project.clone().or_else(|| {
                                s.workspace.as_ref()?.projects.first().map(|p| p.id.clone())
                            }),
                            task_id: s.selected.clone(),
                        });
                        cx.notify();
                    })),
            )
            .child(self.usage_meters(cx))
            .child(div().flex_1())
            .child(
                self.ghost("composer-model", preferences.provider.icon(), "")
                    .px(px(7.))
                    .child(div().max_w(px(160.)).min_w_0().text_ellipsis().child(model))
                    .text_size(px(12.))
                    .child(icon("chevron-down").size(px(11.)).text_color(t.subtle))
                    .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                        s.menu_anchor = Some(bounds.top_right());
                        s.panel = Panel::Models;
                        if s.selected_runtime().models.is_empty() {
                            s.send(core::Request::ConnectRuntime {
                                provider: s.composer_preferences().provider,
                            });
                        }
                        cx.notify();
                    })),
            )
            .when(has_reasoning, |d| {
                d.child(div().w(px(1.)).h(px(12.)).mx(px(2.)).bg(t.border))
                    .child(
                        self.ghost("composer-effort", "settings-2", effort)
                            .px(px(7.))
                            .text_size(px(12.))
                            .child(icon("chevron-down").size(px(11.)).text_color(t.subtle))
                            .on_click_with_bounds(cx.listener(
                                |s, bounds: &Bounds<Pixels>, _, cx| {
                                    s.menu_anchor = Some(bounds.top_right());
                                    s.panel = Panel::Reasoning;
                                    cx.notify();
                                },
                            )),
                    )
            })
            .map(|d| {
                d.child(div().w(px(1.)).h(px(12.)).mx(px(2.)).bg(t.border))
                    .child(
                        self.ghost(
                            "composer-access",
                            match preferences.access {
                                AccessMode::FullAccess => "lock-open",
                                AccessMode::AutoReview => "bot",
                                _ => "shield",
                            },
                            access_label,
                        )
                        .px(px(7.))
                        .text_size(px(12.))
                        .child(icon("chevron-down").size(px(11.)).text_color(t.subtle))
                        .on_click_with_bounds(cx.listener(
                            |s, bounds: &Bounds<Pixels>, _, cx| {
                                s.menu_anchor = Some(bounds.top_right());
                                s.panel = Panel::Access;
                                cx.notify();
                            },
                        )),
                    )
            });
        if active
            && runtime.steering
            && task.as_ref().is_none_or(|t| t.pending_completion.is_none())
        {
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
                .px(px(7.))
                .text_size(px(12.))
                .text_color(t.accent)
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
                || self.creating_task
                || (empty && project.is_none())
                || self.submitted_draft.is_some()
                || !self.has_composer_content(cx));
        let submit_label = if active {
            "Stop current turn"
        } else if empty {
            "Create task and send message · Enter"
        } else {
            "Send message · Enter"
        };
        let mut hover = t.accent;
        hover.lightness = (hover.lightness + 0.05).min(1.);
        toolbar = toolbar.child(div().w(px(6.))).child(
            // This control owns its hover style; icon_button already installs one.
            self.control("composer-submit")
                .px_0()
                .child(
                    icon(if active {
                        "stop"
                    } else if empty {
                        "plus"
                    } else {
                        "arrow-up"
                    })
                    .size(px(15.)),
                )
                .accessibility_label(submit_label)
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: submit_label.into(),
                        theme: t,
                    })
                    .into()
                })
                .h(px(28.))
                .w(px(28.))
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .map(|b| {
                    if active {
                        b.bg(t.accent_bg)
                            .border_color(t.accent.opacity(0.6))
                            .text_color(t.accent)
                            .shadow(Vec::new())
                            .hover(move |b| b.bg(t.selected))
                    } else if disabled {
                        b.bg(t.inset)
                            .border_color(t.border)
                            .text_color(t.subtle)
                            .shadow(Vec::new())
                            .cursor_default()
                    } else {
                        b.bg(t.accent)
                            .border_color(t.accent)
                            .text_color(t.on_accent)
                            .shadow(vec![BoxShadow {
                                inset: true,
                                color: rgb_to_hsla(rgb(0xffffff)).opacity(0.25),
                                offset: point(px(0.), px(1.)),
                                blur_radius: px(0.),
                                spread_radius: px(0.),
                            }])
                            .hover(move |b| b.bg(hover))
                            .active(move |b| b.shadow(Vec::new()))
                    }
                })
                .disabled(disabled)
                .on_click(cx.listener(move |s, _, w, cx| {
                    if let Some(view) = s.view.as_ref().filter(|v| v.task.state.active()) {
                        s.send(core::Request::Interrupt {
                            task_id: view.task.id.clone(),
                        });
                    } else {
                        s.submit(&SendPrompt, w, cx);
                    }
                })),
        );
        editor = editor.child(toolbar);
        wrapper = wrapper.child(
            div()
                .relative()
                .top(px(self.composer_spring.offset))
                .child(dither::shadow(t, motion::COMPOSER_RADIUS, 4.))
                .child(editor),
        );
        if !empty {
            wrapper = wrapper.child(
                div()
                    .pt(px(7.))
                    .px(px(2.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon("branch").size(px(11.)).text_color(t.subtle))
                    .child(caps(
                        if task.as_ref().is_some_and(|v| v.owns_worktree) {
                            "Isolated worktree"
                        } else {
                            "Current checkout"
                        },
                        t,
                    ))
                    .child(div().flex_1())
                    .child(caps(
                        if active {
                            if self.settings.send_mode == SendMode::Queue
                                || task
                                    .as_ref()
                                    .is_some_and(|t| t.pending_completion.is_some())
                            {
                                "⏎ queue · ⇧⏎ newline"
                            } else {
                                "⏎ steer · ⇧⏎ newline"
                            }
                        } else {
                            "⏎ send · ⇧⏎ newline"
                        },
                        t,
                    )),
            );
        }
        wrapper
            .child(self.slash_menu(empty, window, cx))
            .into_any_element()
    }
}

/// Returns a compact uppercase state readout for the conversation header.
fn state_caps(task: &core::Task, t: Theme) -> AnyElement {
    let (label, color) = if task.needs_input() {
        ("Needs input", t.accent)
    } else {
        match task.state {
            RunState::Starting | RunState::Running => (task.state.label(), t.accent),
            RunState::Failed | RunState::DeliveryUnknown => (task.state.label(), t.removed),
            _ => (task.state.label(), t.subtle),
        }
    };
    div()
        .flex_shrink_0()
        .px(px(5.))
        .py(px(2.))
        .rounded(px(crate::theme::RADIUS_SMALL))
        .border_1()
        .border_color(color.opacity(0.35))
        .child(caps(label, t).text_color(color))
        .into_any_element()
}

/// Returns an inline notice row with a colored rule and an uppercase source label.
fn notice(t: Theme, color: Hsla, label: &str) -> Div {
    div()
        .mx(px(20.))
        .mb(px(8.))
        .pl(px(12.))
        .pr(px(8.))
        .py(px(7.))
        .rounded(px(crate::theme::RADIUS_CONTROL))
        .bg(color.opacity(0.08))
        .border_1()
        .border_color(color.opacity(0.25))
        .flex()
        .items_center()
        .gap(px(10.))
        .text_size(px(12.))
        .child(caps(label, t).text_color(color))
}
