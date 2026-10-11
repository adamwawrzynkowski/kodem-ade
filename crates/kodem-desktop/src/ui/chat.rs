use super::*;

/// Name of the effect anchor on the composer card, where chips for sent work start.
pub(super) const COMPOSER_ANCHOR: &str = "composer";

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
                panel = panel.child(self.chat_header(view, cx));
            }
            if state == &RunState::DeliveryUnknown {
                let id = view.task.id.clone();
                panel = panel.child(
                    notice(t, t.warning, "Delivery")
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
            if let Some(account_notice) = self.account_notice(cx) {
                panel = panel.child(account_notice);
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
                    timeline =
                        timeline.child(div().px_5().pb_2().child(
                            self.earlier_messages("older").on_click(cx.listener(
                                move |s, _, _, _| {
                                    s.send(core::Request::Task {
                                        task_id: id.clone(),
                                        before: Some(before),
                                    })
                                },
                            )),
                        ));
                }
                if view.messages.is_empty() {
                    timeline = timeline.child(
                        div()
                            .flex_1()
                            .pb(px(self.composer_height))
                            .child(kit::empty_state(
                                "This task is empty",
                                "Describe the change below. The conversation and its code review stay together in this task.",
                                dither::DitherField::Wave {
                                    level: 0.5,
                                    amplitude: 0.08,
                                    speed: 1.2,
                                },
                                t,
                            )),
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
                            .pt(px(48.))
                            .pb(px(32.))
                            .child(self.empty_chat_greeting(cx))
                            .children(
                                self.account_notice(cx)
                                    .map(|notice| div().mx(px(-20.)).child(notice)),
                            )
                            .child(self.composer(true, window, cx))
                            .child(self.shortcut_hints()),
                    ),
            );
        }
        if self.show_terminal && self.selected.is_some() {
            panel = panel.child(self.terminal_panel(window, cx));
        }
        panel.into_any_element()
    }

    // The task's title, state, live change totals and actions.
    fn chat_header(&self, view: &TaskView, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let task = &view.task;
        let changes = self
            .task_changes
            .as_ref()
            .filter(|summary| summary.files > 0);
        let cost = task
            .context_usage
            .as_ref()
            .and_then(|usage| usage.cost.as_ref())
            .map(|cost| {
                if cost.currency.eq_ignore_ascii_case("usd") {
                    format!("${:.2}", cost.amount)
                } else {
                    format!("{:.2} {}", cost.amount, cost.currency)
                }
            });
        div()
            .h(px(38.))
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
                    .child(task.title.clone()),
            )
            .child(state_chip(task, t))
            .child(div().flex_1())
            .when_some(cost, |d, cost| {
                d.child(div().id("task-cost").child(figure(cost, t.subtle)).tooltip(
                    move |_, cx| {
                        cx.new(|_| Hint {
                            text: "Session cost reported by the provider".into(),
                            theme: t,
                        })
                        .into()
                    },
                ))
            })
            .when_some(changes, |d, summary| {
                let files = summary.files;
                let omitted = summary.omitted_files;
                d.child(
                    self.control("task-changes")
                        .h(px(24.))
                        .px(px(7.))
                        .gap(px(7.))
                        .bg(gpui::transparent_black())
                        .border_color(t.border)
                        .hover(move |s| s.bg(t.selected).border_color(t.accent.opacity(0.5)))
                        .accessibility_label("Review this task's changes")
                        .child(icon("file-text").size(px(12.)).text_color(t.muted))
                        .child(figure(files.to_string(), t.muted))
                        .when(omitted > 0, |b| b.child(figure("~", t.subtle)))
                        .child(kit::diffstat(summary.added, summary.removed, t))
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: format!(
                                    "{files} changed {} in this task. Click to review.",
                                    if files == 1 { "file" } else { "files" }
                                ),
                                theme: t,
                            })
                            .into()
                        })
                        .on_click(cx.listener(|s, _, _, cx| s.open_task_changes(cx))),
                )
            })
            .child(
                self.icon_button("task-menu", "ellipsis", "Task actions")
                    .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                        s.menu_anchor = Some(bounds.top_right());
                        s.panel = Panel::TaskMenu;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    // Tells the user when the task's account cannot run a turn, with the action that fixes it.
    fn account_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.theme;
        let Some(state) = self.composer_account() else {
            // The host keeps a removed account's tasks blocked instead of switching identity.
            let preferences = self.composer_preferences();
            self.workspace.as_ref()?;
            preferences.account.as_ref()?;
            return Some(
                notice(t, t.removed, "Account")
                    .child(div().flex_1().child(format!(
                        "The {} account of this task was removed. Choose another account to continue.",
                        preferences.provider.label()
                    )))
                    .child(
                        self.button("notice-choose-account", "users", "Choose account")
                            .on_click(cx.listener(|s, _, window, cx| {
                                s.open_agent_menu(Panel::Models, window, cx)
                            })),
                    )
                    .into_any_element(),
            );
        };
        let provider = state.account.provider;
        let id = state.account.id.clone();
        let name = state.account.name.clone();
        if state.status.signed_in == Some(false) {
            return Some(
                notice(t, t.warning, "Account")
                    .child(
                        div()
                            .flex_1()
                            .child(format!("{name} is signed out of {}.", provider.label())),
                    )
                    .child(
                        self.primary("notice-sign-in", "log-in", "Sign in")
                            .disabled(state.status.signing_in)
                            .on_click(cx.listener(move |s, _, _, _| {
                                s.send(core::Request::SignInAccount {
                                    provider,
                                    id: id.clone(),
                                })
                            })),
                    )
                    .into_any_element(),
            );
        }
        let usage = state
            .status
            .usage
            .as_ref()
            .filter(|usage| usage::limit_reached(usage))?;
        let reset = [usage.primary.as_ref(), usage.secondary.as_ref()]
            .into_iter()
            .flatten()
            .filter(|window| window.used_percent >= 100)
            .filter_map(|window| window.resets_at)
            .min();
        // The notice names the reset. It does not offer another account: using one to continue
        // past a limit is what the providers' terms forbid.
        Some(
            notice(t, t.removed, "Limit")
                .child(div().flex_1().child(format!(
                    "{name} reached its usage limit. {}",
                    usage::reset_time(reset)
                )))
                .into_any_element(),
        )
    }

    fn empty_chat_greeting(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let name = self
            .composer_project()
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
                    .h(px(140.))
                    .rounded(px(crate::theme::RADIUS_SURFACE))
                    .overflow_hidden()
                    .border_1()
                    .border_color(t.border)
                    .bg(t.backdrop)
                    .child(dither::hero(t.accent, t.dot)),
            )
            .child(kit::section_header(
                format!("Hello, {}", self.greeting_name),
                None,
                t,
            ))
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
                            .px(px(5.))
                            .py(px(1.))
                            .border_color(t.accent.opacity(0.35))
                            .bg(t.accent_bg)
                            .hover(move |b| b.bg(t.accent.opacity(0.22)))
                            .font_family(crate::theme::SANS)
                            .text_size(px(24.))
                            .line_height(relative(1.25))
                            .font_weight(FontWeight::MEDIUM)
                            .max_w_full()
                            .min_w_0()
                            .text_ellipsis()
                            .text_color(t.accent)
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

    // The keys that are worth learning first, shown under the composer of a new task.
    fn shortcut_hints(&self) -> AnyElement {
        let t = self.theme;
        let mut hints = div()
            .mt(px(18.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(16.))
            .gap_y(px(8.));
        for (keys, label) in [
            ("⌘K", "Commands and threads"),
            ("/", "Provider commands"),
            ("⇧⇥", "Cycle access"),
            ("⇧⌘M", "Model and account"),
            ("⌥⌘A", "Next task that needs you"),
        ] {
            hints = hints.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(kit::keycap(keys, t))
                    .child(caps(label, t)),
            );
        }
        hints.into_any_element()
    }

    fn composer(&mut self, empty: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let task = self.view.as_ref().map(|view| view.task.clone());
        let active = task.as_ref().is_some_and(|v| v.state.active());
        let unknown = task
            .as_ref()
            .is_some_and(|v| v.state == RunState::DeliveryUnknown);
        let has_project = self.composer_project().is_some();
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
                .or_else(|| {
                    runtime
                        .access_modes
                        .iter()
                        .find(|mode| mode.id == "default")
                })
                .or(runtime.access_modes.first())
                .map(|mode| mode.name.clone())
                .unwrap_or_else(|| "Access".into())
        };
        let access_id = if preferences.provider == Provider::Codex {
            preferences.access.label().to_owned()
        } else {
            preferences.provider_access.clone().unwrap_or_default()
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
            .unwrap_or_else(|| preferences.provider.label().to_owned());
        let effort = preferences
            .effort
            .clone()
            .unwrap_or_else(|| "Effort".into());
        let account = self
            .composer_account()
            .filter(|_| preferences.provider.home_variable().is_some())
            .map(|state| {
                (
                    state.account.name.clone(),
                    self.account_tone(&state.account),
                    state.status.signed_in == Some(false)
                        || state
                            .status
                            .usage
                            .as_ref()
                            .is_some_and(usage::limit_reached),
                )
            });
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
                    kit::block(t)
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2()
                        .pl(px(8.))
                        .pr(px(2.))
                        .py(px(2.))
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
                                .font_family(crate::theme::MONO)
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
                        .when(skill, |d| d.child(kit::chip("SKILL", t.info))),
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
            attachments = attachments.child(kit::loader_row("Importing attachments", t));
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
                t.accent.opacity(0.6)
            } else {
                t.edge
            })
            .overflow_hidden()
            .child(fx::anchor(COMPOSER_ANCHOR))
            .when(!empty, |d| {
                d.child(self.subagents_strip(cx))
                    .child(self.activity_strip(cx))
                    .child(self.plan_strip(cx))
                    .child(self.question_panel(cx))
            })
            .when(has_context, |d| {
                d.child(div().px(px(12.)).pt(px(10.)).child(attachments))
            });
        if empty {
            let worktree = self.composer_worktree();
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
                            if worktree {
                                "New worktree"
                            } else {
                                "Current checkout"
                            },
                        )
                        .when(worktree, |b| b.text_color(t.info))
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: if worktree {
                                    "This task works in its own Git worktree and branch.".into()
                                } else {
                                    "This task edits the repository's current checkout.".into()
                                },
                                theme: t,
                            })
                            .into()
                        })
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
                        s.open_resources();
                        cx.notify();
                    })),
            )
            .child(self.usage_meters(cx))
            .child(div().flex_1())
            .child(
                self.chip_button("composer-model")
                    .selected(self.panel == Panel::Models)
                    .accessibility_label("Provider, account and model")
                    .child(fx::anchor(model_picker::AGENT_ANCHOR))
                    .child(icon(preferences.provider.icon()).size(px(13.)))
                    .child(div().max_w(px(150.)).min_w_0().text_ellipsis().child(model))
                    .when_some(account, |b, (name, tone, blocked)| {
                        b.child(
                            div()
                                .h(px(12.))
                                .child(dither::DottedRule::vertical(t.subtle.opacity(0.7))),
                        )
                        .child(
                            div()
                                .size(px(6.))
                                .rounded(px(1.))
                                .flex_shrink_0()
                                .bg(if blocked { t.removed } else { tone }),
                        )
                        .child(
                            div()
                                .max_w(px(96.))
                                .min_w_0()
                                .text_ellipsis()
                                .when(blocked, |d| d.text_color(t.removed))
                                .child(name),
                        )
                    })
                    .child(icon("chevron-down").size(px(11.)).text_color(t.subtle))
                    .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                        s.menu_anchor = Some(bounds.top_right());
                        s.panel = Panel::Models;
                        if s.selected_runtime().models.is_empty() {
                            s.connect_runtime();
                        }
                        cx.notify();
                    })),
            )
            .when(has_reasoning, |d| {
                d.child(
                    self.chip_button("composer-effort")
                        .selected(self.panel == Panel::Reasoning)
                        .accessibility_label("Reasoning effort")
                        .child(icon("gauge").size(px(13.)))
                        .child(effort)
                        .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                            s.menu_anchor = Some(bounds.top_right());
                            s.panel = Panel::Reasoning;
                            cx.notify();
                        })),
                )
            })
            .child(
                self.chip_button("composer-access")
                    .selected(self.panel == Panel::Access)
                    .accessibility_label("Access mode")
                    .child(fx::anchor(model_picker::ACCESS_ANCHOR))
                    .child(icon(model_picker::access_glyph(&access_id)).size(px(13.)))
                    .child(access_label)
                    .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                        s.menu_anchor = Some(bounds.top_right());
                        s.panel = Panel::Access;
                        cx.notify();
                    })),
            );
        let steering = active
            && runtime.steering
            && task.as_ref().is_none_or(|t| t.pending_completion.is_none());
        let has_content = self.has_composer_content(cx);
        let steer = steering && self.settings.send_mode == SendMode::Steer;
        if steering {
            toolbar = toolbar.child(
                self.chip_button("active-send-mode")
                    .text_color(t.accent)
                    .child(icon(if steer { "arrow-right" } else { "arrow-down" }).size(px(13.)))
                    .child(if steer { "Steer" } else { "Queue" })
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint {
                            text: if steer {
                                "Enter adds your message to the running turn. ⌘Enter queues it instead.".into()
                            } else {
                                "Enter queues your message for the next turn. ⌘Enter steers the running turn instead.".into()
                            },
                            theme: t,
                        })
                        .into()
                    })
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
        let blocked = unknown
            || importing
            || self.creating_task
            || (empty && !has_project)
            || self.submitted_draft.is_some()
            || !has_content;
        // While a turn runs, an empty draft offers to stop it and a draft offers to send it.
        let stop_only = active && !has_content;
        if active && has_content {
            toolbar = toolbar.child(
                self.icon_button("composer-stop", "stop", "Stop current turn")
                    .text_color(t.removed)
                    .on_click(cx.listener(|s, _, _, _| {
                        if let Some(task_id) = &s.selected {
                            s.send(core::Request::Interrupt {
                                task_id: task_id.clone(),
                            });
                        }
                    })),
            );
        }
        let submit_label = if stop_only {
            "Stop current turn"
        } else if active {
            if steer {
                "Steer the running turn · Enter"
            } else {
                "Queue for the next turn · Enter"
            }
        } else if empty {
            "Create task and send message · Enter"
        } else {
            "Send message · Enter"
        };
        let submit_glyph = if stop_only {
            "stop"
        } else if active && !steer {
            "arrow-down"
        } else if active {
            "arrow-right"
        } else {
            "arrow-up"
        };
        let submit = if stop_only {
            self.tinted("composer-submit", submit_glyph, "", t.removed)
        } else {
            self.primary("composer-submit", submit_glyph, "")
                .disabled(blocked)
        };
        toolbar = toolbar.child(div().w(px(4.))).child(
            submit
                .w(px(28.))
                .h(px(28.))
                .px_0()
                .accessibility_label(submit_label)
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: submit_label.into(),
                        theme: t,
                    })
                    .into()
                })
                .on_click(cx.listener(move |s, _, w, cx| {
                    if stop_only {
                        if let Some(task_id) = &s.selected {
                            s.send(core::Request::Interrupt {
                                task_id: task_id.clone(),
                            });
                        }
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
            let hint: &[(&str, &str)] = if steering {
                if steer {
                    &[("⏎", "steer"), ("⌘⏎", "queue"), ("⇧⏎", "newline")]
                } else {
                    &[("⏎", "queue"), ("⌘⏎", "steer"), ("⇧⏎", "newline")]
                }
            } else if active {
                &[("⏎", "queue"), ("⇧⏎", "newline")]
            } else {
                &[("⏎", "send"), ("⇧⏎", "newline"), ("⇧⇥", "access")]
            };
            let mut footer = div()
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
                .child(div().flex_1());
            for (keys, label) in hint {
                footer = footer
                    .child(kit::keycap(*keys, t).ml(px(4.)))
                    .child(caps(label, t));
            }
            wrapper = wrapper.child(footer);
        }
        wrapper
            .child(self.slash_menu(empty, window, cx))
            .into_any_element()
    }

    /// Returns the control that loads an earlier page of a conversation. It reads as a section
    /// header: an arrow, a mono label, then a dotted rule.
    pub(super) fn earlier_messages(&self, id: &'static str) -> Button {
        let t = self.theme;
        self.control(id)
            .w_full()
            .px(px(6.))
            .gap(px(8.))
            .justify_start()
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .hover(move |s| s.bg(t.selected))
            .accessibility_label("Earlier messages")
            .child(icon("arrow-up").size(px(12.)).text_color(t.subtle))
            .child(caps("Earlier messages", t).text_color(t.muted))
            .child(
                div()
                    .flex_1()
                    .min_w(px(8.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
            )
    }

    // A flat composer chip that opens one of the agent menus.
    fn chip_button(&self, id: &'static str) -> Button {
        let t = self.theme;
        self.control(id)
            .h(px(24.))
            .px(px(7.))
            .gap(px(6.))
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .text_color(t.muted)
            .hover(move |s| s.bg(t.selected).text_color(t.text))
            .styles(move |s| s.selected(|s| s.bg(t.accent.opacity(0.12)).text_color(t.text)))
    }
}

/// Returns a tinted state chip for the conversation header.
fn state_chip(task: &core::Task, t: Theme) -> Div {
    let (label, color) = if task.needs_input() {
        ("Needs input", t.warning)
    } else {
        match task.state {
            RunState::Starting | RunState::Running => (task.state.label(), t.accent),
            RunState::Failed | RunState::DeliveryUnknown => (task.state.label(), t.removed),
            RunState::Completed => (task.state.label(), t.added),
            _ => (task.state.label(), t.subtle),
        }
    };
    kit::chip(label.to_uppercase(), color)
}

/// Returns an inline notice card with a dithered marker and an uppercase source label.
fn notice(t: Theme, color: Hsla, label: &str) -> Div {
    kit::block(t)
        .w_auto()
        .flex_shrink_0()
        .mx(px(20.))
        .mb(px(8.))
        .pl(px(12.))
        .pr(px(8.))
        .py(px(7.))
        .bg(color.opacity(0.06))
        .border_color(color.opacity(0.3))
        .flex()
        .items_center()
        .gap(px(10.))
        .text_size(px(12.))
        .child(kit::marker(color))
        .child(caps(label, t).text_color(color))
}
