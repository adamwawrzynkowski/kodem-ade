use super::*;

/// Name of the effect anchor on the composer card, where chips for sent work start.
pub(super) const COMPOSER_ANCHOR: &str = "composer";

impl Shell {
    /// Builds the conversation with a measured floating composer and a clear review viewport.
    pub(super) fn chat(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sync_slash(cx);
        self.sync_context_arrivals(window, cx);
        let history = self.message_history(window, cx);
        let t = self.theme;
        let mut panel = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            // The surface and its glyph field are painted beneath the workspace, in the mood of
            // the open task. A press on the surface starts a wave there.
            .child(backdrop::surface(
                backdrop::Mood::of(self.view.as_ref().map(|view| &view.task)),
                // The conversation and the new-task column are centered at these widths.
                if self.view.is_some() { 900. } else { 780. },
                // The composer of a conversation floats over the bottom of the surface. The
                // composer of a new task is in the middle of it.
                if self.view.is_some() {
                    self.composer_height
                } else {
                    0.
                },
            ));
        if let Some(view) = &self.view {
            let orders = (0..self.chat_rows.count())
                .filter_map(|row| {
                    let (index, part) = self.chat_rows.locate(row)?;
                    Some((view.messages.get(index)?.seq, part))
                })
                .collect();
            self.chat_selection.update(cx, |doc, _| {
                doc.set_viewport(self.chat_scroll.clone(), orders, px(8.))
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
            if let Some(review_actions) = self.review_actions(cx) {
                panel = panel.child(review_actions);
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
                    // The wheel scrolls the conversation anywhere in the chat, also beside and
                    // below the list. The list handles the wheel over itself, and the composer
                    // scrolls its own draft.
                    let over_composer = fx::anchor_bounds(COMPOSER_ANCHOR, cx)
                        .is_some_and(|bounds| bounds.contains(&e.position));
                    if !over_composer && !s.chat_scroll.viewport_bounds().contains(&e.position) {
                        s.chat_scroll.scroll_by(-delta.y);
                        cx.notify();
                    }
                }));
            if center_diff {
                timeline = timeline
                    .child(self.review(cx, true))
                    .child(div().h(gpui::px(self.composer_height)).flex_shrink_0());
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
                            .pb(gpui::px(self.composer_height))
                            .child(kit::empty_state(
                                match self.mode {
                                    TaskKind::Coding => "This task is empty",
                                    TaskKind::Chat => "This chat is empty",
                                    TaskKind::Review => "This review has not started",
                                },
                                match self.mode {
                                    TaskKind::Coding => "Describe the change below. The conversation and its code review stay together in this task.",
                                    TaskKind::Chat => "Ask a question below. A chat has no project and can search the web.",
                                    TaskKind::Review => "Send a message below to start it. A review reads the code and changes nothing.",
                                },
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
                            // The conversation ends where the composer begins. Nothing covers
                            // the area around the composer, so the background shows down to
                            // the bottom of the chat.
                            .pb((gpui::px(self.composer_height) - px(COMPOSER_LEAD))
                                .max(gpui::px(0.)))
                            .child(
                                div()
                                    .relative()
                                    .w_full()
                                    .min_w_0()
                                    .h_full()
                                    .child(
                                        // The list spans the chat, so the wheel scrolls it
                                        // from the margins too. Each row centers its content
                                        // in the reading column.
                                        list(self.chat_scroll.clone(), move |index, _, cx| {
                                            let row = entity.update(cx, |s, cx| {
                                                s.conversation_row(&Conversation::Main, index, cx)
                                            });
                                            div()
                                                .w_full()
                                                .flex()
                                                .justify_center()
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .max_w(px(900.))
                                                        .min_w_0()
                                                        .child(row),
                                                )
                                                .into_any_element()
                                        })
                                        .w_full()
                                        .min_w_0()
                                        .h_full()
                                        .pb(px(8.)),
                                    )
                                    // The history rail and its preview lie over the list.
                                    .children(history),
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
                    .child(div().relative().h(px(COMPOSER_LEAD)).flex_shrink_0().when(
                        !self.follow,
                        |d| {
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
                        },
                    ))
                    .child(div().w_full().child(self.composer(false, window, cx)))
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
                            .when(self.mode == TaskKind::Review, |column| {
                                column.child(self.review_picker(cx))
                            })
                            .children(
                                self.account_notice(cx)
                                    .map(|notice| div().mx(px(-20.)).child(notice)),
                            )
                            .child(self.composer(true, window, cx))
                            .child(self.shortcut_hints()),
                    ),
            );
        }
        let terminal_open = self.reveals.terminal;
        if terminal_open > 0. && self.has_checkout() {
            let terminal = self.terminal_panel(window, cx);
            panel = panel.child(if terminal_open < 1. {
                // While the terminal slides it is clipped to its current height and grows up
                // from the bottom of the chat.
                div()
                    .w_full()
                    .flex_shrink_0()
                    .max_h(px(motion::TERMINAL_REACH * terminal_open))
                    .overflow_hidden()
                    .child(terminal)
                    .into_any_element()
            } else {
                terminal
            });
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
            // The header is chrome: it keeps a solid fill over the glyph field behind the chat.
            .bg(self.theme.canvas)
            .child(
                div()
                    .min_w_0()
                    .text_size(text_px(13.))
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
                        s.menu_anchor = Some(*bounds);
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

    // The heading of a start screen: a greeting, then what the workspace starts. Agentic Coding
    // and Review name the project the work happens in, as a control that changes it.
    fn empty_chat_greeting(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let (label, lead, tail) = match self.mode {
            TaskKind::Coding => (
                format!("Hello, {}", self.greeting_name),
                "What should we build in",
                "?",
            ),
            TaskKind::Chat => (
                format!("Hello, {}", self.greeting_name),
                "What is on your mind?",
                "",
            ),
            TaskKind::Review => (
                "Local review · read-only".to_owned(),
                "Review changes in",
                "",
            ),
        };
        let project = self.mode.has_project().then(|| {
            let name = self
                .composer_project()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "a project".into());
            self.control("greeting-project")
                .h_auto()
                .px(px(5.))
                .py(px(1.))
                .border_color(t.accent.opacity(0.35))
                .bg(t.accent_bg)
                .hover(move |b| b.bg(t.accent.opacity(0.22)))
                .font_family(crate::theme::sans())
                .text_size(text_px(24.))
                .line_height(relative(1.25))
                .font_weight(FontWeight::MEDIUM)
                .max_w_full()
                .min_w_0()
                .text_ellipsis()
                .text_color(t.accent)
                .accessibility_label("Choose project")
                .child(name)
                .child(icon("chevrons-up-down").size(px(16.)))
                .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                    if !s.creating_task {
                        s.menu_anchor = Some(*bounds);
                        s.panel = Panel::ChatProjects;
                        cx.notify();
                    }
                }))
        });
        div()
            .w_full()
            .mb(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(kit::section_header(label, None, t))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .gap(px(7.))
                    .text_size(text_px(24.))
                    .line_height(relative(1.25))
                    .font_weight(FontWeight::MEDIUM)
                    .child(lead)
                    .children(project)
                    .when(!tail.is_empty(), |heading| heading.child(tail)),
            )
            .into_any_element()
    }

    // The keys that are worth learning first, shown under the composer of a start screen.
    fn shortcut_hints(&self) -> AnyElement {
        let t = self.theme;
        let mut hints = div()
            .mt(px(18.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(16.))
            .gap_y(px(8.));
        let keys: &[(&str, &str)] = match self.mode {
            TaskKind::Coding => &[
                ("⌘K", "Commands and threads"),
                ("/", "Provider commands"),
                ("⇧⇥", "Plan or build"),
                ("⇧⌘M", "Model and account"),
                ("⌥⌘A", "Next task that needs you"),
            ],
            TaskKind::Chat => &[
                ("⏎", "Send"),
                ("⇧⏎", "New line"),
                ("⇧⌘M", "Model and account"),
                ("⌘K", "Commands and threads"),
                ("⌥⌘1", "Agentic Coding"),
            ],
            TaskKind::Review => &[
                ("⏎", "Start review"),
                ("⇧⌘M", "Model and account"),
                ("⌘K", "Commands and threads"),
                ("⌥⌘1", "Agentic Coding"),
            ],
        };
        for (keys, label) in keys {
            hints = hints.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(kit::keycap(*keys, t))
                    .child(caps(label, t)),
            );
        }
        // What the workspace does with a message, where that differs from a coding task.
        let rule = match self.mode {
            TaskKind::Coding => None,
            TaskKind::Chat => Some("A chat has no project. It can search the web and read pages."),
            TaskKind::Review => Some(
                "A review runs on this machine in a detached worktree. It changes no file, and it does not push or comment.",
            ),
        };
        div()
            .flex()
            .flex_col()
            .child(hints)
            .when_some(rule, |column, rule| {
                column.child(
                    div()
                        .mt(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(kit::marker(t.info))
                        .child(
                            div()
                                .font_family(crate::theme::mono())
                                .text_size(text_px(11.))
                                .text_color(t.subtle)
                                .child(rule),
                        ),
                )
            })
            .into_any_element()
    }

    fn composer(&mut self, empty: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let task = self.view.as_ref().map(|view| view.task.clone());
        let active = task.as_ref().is_some_and(|v| v.state.active());
        let unknown = task
            .as_ref()
            .is_some_and(|v| v.state == RunState::DeliveryUnknown);
        let mode = self.mode;
        // What a start screen needs before it can create its task.
        let ready = match mode {
            TaskKind::Coding => self.composer_project().is_some(),
            TaskKind::Chat => true,
            TaskKind::Review => self.review_project().is_some() && self.review.pick.is_some(),
        };
        // A review starts from its picked target; the draft only adds focus notes.
        let needs_content = !(empty && mode == TaskKind::Review);
        let preferences = self.composer_preferences();
        let runtime = self.selected_runtime();
        let selected_model = runtime
            .models
            .iter()
            .find(|model| preferences.model.as_ref() == Some(&model.id));
        let has_reasoning = selected_model.is_some_and(|model| !model.efforts.is_empty());
        let approval = self.approval_choice();
        let access_label = if preferences.provider == Provider::Codex {
            preferences.access.label().to_owned()
        } else {
            runtime
                .access_modes
                .iter()
                .find(|mode| approval.as_ref() == Some(&mode.id))
                .map(|mode| mode.name.clone())
                .unwrap_or_else(|| "Approvals".into())
        };
        let access_id = if preferences.provider == Provider::Codex {
            preferences.access.label().to_owned()
        } else {
            approval.clone().unwrap_or_default()
        };
        // Plan mode is a mode of work with its own chip; the approval rule is separate.
        let has_modes = model_picker::plan_mode(&runtime).is_some();
        let planning = self.planning();
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
                                .font_family(crate::theme::mono())
                                .text_size(text_px(11.))
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
        // Long pasted texts are files of the message, after its attachments.
        let folded: Vec<SharedString> = self.input.read(cx).folded().to_vec();
        if !folded.is_empty() {
            let mut texts = div().flex().flex_wrap().flex_shrink_0().gap_2();
            for (index, text) in folded.iter().enumerate() {
                texts = texts.child(self.folded_text_card(index, text, cx));
            }
            attachments = attachments.child(texts);
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
            || !folded.is_empty()
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
            // The meter tab continues this corner upward.
            .rounded_tr(px(0.))
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
                    // The next messages are a row of the composer, right above the draft.
                    .child(self.outgoing_tray(cx))
            })
            .when(has_context, |d| {
                d.child(div().px(px(12.)).pt(px(10.)).child(attachments))
            });
        // A new review names what it examines in a row above the draft.
        if empty && mode == TaskKind::Review {
            editor = editor.child(self.review_pick_status());
        }
        // A new task says where it will work in a row above the draft.
        if empty && mode == TaskKind::Coding {
            let worktree = self.composer_worktree();
            // A folder without Git has no worktrees, so its task always edits the folder.
            let has_git = self
                .composer_project()
                .is_none_or(|project| self.project_has_git(&project.id));
            let status = div().px(px(8.)).pt(px(6.)).flex().items_center().child(
                self.ghost(
                    "composer-worktree",
                    if has_git { "branch" } else { "folder" },
                    if !has_git {
                        "No Git repository"
                    } else if worktree {
                        "New worktree"
                    } else {
                        "Current checkout"
                    },
                )
                .disabled(!has_git)
                .when(worktree, |b| b.text_color(t.info))
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: if !has_git {
                            "This task edits the project folder. The folder has no Git repository, so its changes are not tracked.".into()
                        } else if worktree {
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
            );
            editor = editor.child(status);
        }
        editor = editor.child(
            div()
                .id("composer-editor")
                .px(px(14.))
                .pt(px(if empty { 6. } else { 12. }))
                .pb(px(6.))
                .min_h(px(if empty { 92. } else { 56. }))
                .max_h(px(180.))
                .overflow_y_scroll()
                .text_size(text_px(13.))
                .text_color(t.text)
                .child(self.input.clone()),
        );
        // The options wrap inside their own group. The send group is a separate, fixed column on
        // the trailing side, so a narrow composer never moves the send button.
        // The leading group holds what goes with the message and who runs it: attachments and
        // skills, a gap, then the model and its effort. The trailing group holds how it may act
        // and the send button. The space between them is whatever the composer has left.
        let agent_hint = match &account {
            Some((name, _, _)) => format!("{model} · {name}"),
            None => model.clone(),
        };
        let toolbar = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(2.))
            .gap_y(px(4.))
            .child(
                self.icon_button(
                    "composer-files",
                    "paperclip",
                    "Attach files, folders or images",
                )
                .on_click(cx.listener(|s, _, _, cx| s.choose_attachments(cx))),
            )
            .child(
                self.icon_button("composer-skills", "code", "Skills, instructions and MCP")
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.open_resources();
                        cx.notify();
                    })),
            )
            .child(div().w(px(8.)).flex_shrink_0())
            .child(
                self.chip_button("composer-model")
                    .selected(self.panel == Panel::Models)
                    .accessibility_label("Provider, account and model")
                    .child(fx::anchor(model_picker::AGENT_ANCHOR))
                    .child(icon(preferences.provider.icon()).size(px(13.)))
                    .max_w_full()
                    // The account is its color square beside the provider's logo; its name is
                    // in the tooltip and in the menu.
                    .when_some(account, |b, (_, tone, blocked)| {
                        b.child(
                            div()
                                .size(px(6.))
                                .rounded(px(1.))
                                .flex_shrink_0()
                                .bg(if blocked { t.removed } else { tone }),
                        )
                    })
                    .child(div().max_w(px(170.)).min_w_0().text_ellipsis().child(model))
                    .child(icon("chevron-down").size(px(11.)).text_color(t.subtle))
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint {
                            text: agent_hint.clone(),
                            theme: t,
                        })
                        .into()
                    })
                    .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                        s.menu_anchor = Some(*bounds);
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
                            s.menu_anchor = Some(*bounds);
                            s.panel = Panel::Reasoning;
                            cx.notify();
                        })),
                )
            });
        let mut rules = div()
            .flex()
            .items_center()
            .gap(px(2.))
            // A review has no mode of work to choose. Its approval rule is the usual one.
            .when(mode == TaskKind::Review, |d| {
                d.child(
                    div()
                        .id("composer-read-only")
                        .px(px(4.))
                        .child(kit::chip("READ ONLY", t.info))
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: "A review is told to change no file, and not to push or comment. Claude Code also runs it without its editing tools and with the commands that publish denied. The approval rule decides what else runs without asking. For Codex, only the Read only access enforces this in its sandbox.".into(),
                                theme: t,
                            })
                            .into()
                        }),
                )
            })
            .when(has_modes && mode != TaskKind::Review, |d| {
                d.child(
                    self.chip_button("composer-mode")
                        .selected(self.panel == Panel::Mode)
                        .accessibility_label("Mode: build or plan")
                        .when(planning, |b| b.text_color(t.info))
                        .child(icon(if planning { "list-checks" } else { "zap" }).size(px(13.)))
                        .child(if planning { "Plan" } else { "Build" })
                        .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                            s.menu_anchor = Some(*bounds);
                            s.panel = Panel::Mode;
                            cx.notify();
                        })),
                )
            })
            .child(
                self.chip_button("composer-access")
                    .selected(self.panel == Panel::Access)
                    .accessibility_label("Approvals")
                    // The rule waits for build mode while the agent only plans.
                    .when(planning, |b| b.text_color(t.subtle))
                    .child(fx::anchor(model_picker::ACCESS_ANCHOR))
                    .child(icon(model_picker::access_glyph(&access_id)).size(px(13.)))
                    .child(access_label)
                    .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                        s.menu_anchor = Some(*bounds);
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
            // A square icon chip: a list that takes the message at its end for queue, a wheel
            // for steer. The tooltip names the mode.
            rules = rules.child(
                self.chip_button("active-send-mode")
                    .w(px(24.))
                    .px_0()
                    .text_color(t.accent)
                    .accessibility_label(if steer {
                        "Send mode: steer. Click to queue instead"
                    } else {
                        "Send mode: queue. Click to steer instead"
                    })
                    .child(icon(if steer { "ship-wheel" } else { "list-end" }).size(px(14.)))
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint {
                            text: if steer {
                                "Steer · Enter adds your message to the running turn. ⌘Enter queues it instead. Click to switch.".into()
                            } else {
                                "Queue · Enter queues your message for the next turn. ⌘Enter steers the running turn instead. Click to switch.".into()
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
        // The trailing group wraps toward the trailing side, so the send button stays last and
        // on the right in a narrow composer.
        let mut actions = div()
            .min_w_0()
            .flex()
            .flex_wrap()
            .justify_end()
            .items_center()
            .gap(px(4.))
            .child(rules);
        let blocked = unknown
            || importing
            || self.creating_task
            || (empty && !ready)
            || self.submitted_draft.is_some()
            || (needs_content && !has_content);
        // While a turn runs the submit key stops it. Enter still queues or steers a draft, as the
        // send mode chip says.
        let stopping = active;
        let submit_label = if stopping {
            "Stop current turn"
        } else if empty {
            match mode {
                TaskKind::Coding => "Create task and send message · Enter",
                TaskKind::Chat => "Start chat and send message · Enter",
                TaskKind::Review => "Start review · Enter",
            }
        } else {
            "Send message · Enter"
        };
        let submit_glyph = if stopping { "stop" } else { "arrow-up" };
        let submit = if stopping {
            self.glass("composer-submit", submit_glyph, "", t.removed)
        } else {
            self.primary("composer-submit", submit_glyph, "")
                .disabled(blocked)
        };
        actions = actions.child(
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
                    if stopping {
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
        editor = editor.child(
            div()
                .pl(px(8.))
                .pr(px(8.))
                .pb(px(8.))
                .flex()
                .items_end()
                .gap(px(8.))
                .child(toolbar)
                .child(actions),
        );
        // The context and usage meters sit in a tab on the composer's top edge. The tab is
        // drawn after the composer, one point over its border, so the two read as one surface.
        let tab_edge = if focused {
            t.accent.opacity(0.6)
        } else {
            t.edge
        };
        let tab = div()
            .absolute()
            .right_0()
            .top(px(1. - METER_TAB_HEIGHT))
            .h(px(METER_TAB_HEIGHT))
            .pl(px(METER_TAB_FOOT + METER_TAB_SLANT + 6.))
            .pr(px(6.))
            .flex()
            .items_center()
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| paint_meter_tab(bounds, t.raised, tab_edge, window),
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(self.usage_meters(cx));
        wrapper = wrapper.child(
            div()
                .relative()
                .mt(px(METER_TAB_HEIGHT - 1.))
                .top(px(self.composer_spring.offset))
                .child(dither::shadow(t, motion::COMPOSER_RADIUS, 4.))
                .child(editor)
                .child(tab),
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
            } else if mode == TaskKind::Review {
                &[("⏎", "send"), ("⇧⏎", "newline")]
            } else if has_modes {
                &[("⏎", "send"), ("⇧⏎", "newline"), ("⇧⇥", "plan")]
            } else {
                &[("⏎", "send"), ("⇧⏎", "newline"), ("⇧⇥", "access")]
            };
            let mut footer = div()
                .pt(px(7.))
                .px(px(2.))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    icon(match mode {
                        TaskKind::Coding => "branch",
                        TaskKind::Chat | TaskKind::Review => mode.icon(),
                    })
                    .size(px(11.))
                    .text_color(t.subtle),
                )
                .child(caps(
                    match mode {
                        TaskKind::Chat => "No project",
                        TaskKind::Review => "Detached review worktree",
                        TaskKind::Coding if task.as_ref().is_some_and(|v| v.owns_worktree) => {
                            "Isolated worktree"
                        }
                        TaskKind::Coding => "Current checkout",
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

/// Height of the strip above the floating composer that holds the jump-to-latest button. The
/// conversation runs beneath it.
const COMPOSER_LEAD: f32 = 28.;

/// Height of the meter tab on the composer's top edge, and the run of its slanted side.
const METER_TAB_HEIGHT: f32 = 24.;
const METER_TAB_SLANT: f32 = 14.;
/// Room before the slant in which the tab's foot curves out of the composer's edge, and how far
/// from each corner of the slant its rounding starts.
const METER_TAB_FOOT: f32 = 7.;
const METER_TAB_ROUNDING: f32 = 6.;

// Paints the meter tab: a surface with a slanted leading side and a rounded trailing corner
// that continues the composer's corner. The slant curves out of the composer's edge at its
// foot and into the top edge at its head. The bottom side has no edge, because it joins the
// composer.
fn paint_meter_tab(bounds: Bounds<Pixels>, surface: Hsla, edge: Hsla, window: &mut Window) {
    let radius = px(motion::COMPOSER_RADIUS);
    // The edge runs through pixel centers, which keeps a one-pixel stroke sharp. The stroke
    // stays one window pixel wide at every interface scale.
    let half = gpui::px(0.5);
    // The foot starts at the tab's leading edge, where the composer's own edge ends.
    let (left, right) = (bounds.left(), bounds.right() - half);
    let (top, bottom) = (bounds.top() + half, bounds.bottom());
    let foot = point(left + px(METER_TAB_FOOT), bottom);
    let head = point(foot.x + px(METER_TAB_SLANT), top);
    // A step of the rounding along the slant.
    let rise = f32::from(bottom - top);
    let slant = f32::from(px(METER_TAB_SLANT));
    let rounding = f32::from(px(METER_TAB_ROUNDING));
    let length = (slant * slant + rise * rise).sqrt();
    let along = point(
        gpui::px(slant / length * rounding),
        gpui::px(-rise / length * rounding),
    );
    // `base` is where the foot starts: on the composer's edge for the stroke, so that the two
    // lines join, and at the tab's bottom for the fill, so that it covers that edge.
    let outline = |builder: &mut PathBuilder, base: Pixels| {
        builder.move_to(point(left, base));
        builder.curve_to(foot + along, point(foot.x, base));
        builder.line_to(head - along);
        builder.curve_to(point(head.x + px(METER_TAB_ROUNDING), top), head);
        builder.line_to(point(right - radius, top));
        builder.curve_to(point(right, top + radius), point(right, top));
        builder.line_to(point(right, bottom));
    };
    let mut body = PathBuilder::fill();
    outline(&mut body, bottom);
    body.close();
    if let Ok(path) = body.build() {
        window.paint_path(path, surface);
    }
    // The fill is thin where the foot leaves the composer's edge, and a second translucent line
    // over that edge would show as a brighter stretch. The composer's edge is covered under the
    // whole tab, so the tab's own stroke is the only line there.
    window.paint_quad(fill(
        Bounds::new(
            point(bounds.left(), bottom - gpui::px(1.)),
            size(bounds.size.width - gpui::px(1.), gpui::px(1.)),
        ),
        surface,
    ));
    let mut stroke = PathBuilder::stroke(gpui::px(1.));
    outline(&mut stroke, bottom - half);
    if let Ok(path) = stroke.build() {
        window.paint_path(path, edge);
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
pub(super) fn notice(t: Theme, color: Hsla, label: &str) -> Div {
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
        .text_size(text_px(12.))
        .child(kit::marker(color))
        .child(caps(label, t).text_color(color))
}
