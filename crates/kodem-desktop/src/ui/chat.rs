use super::*;
use chrono::Timelike;

/// Name of the effect anchor on the composer card, where chips for sent work start.
pub(super) const COMPOSER_ANCHOR: &str = "composer";

// The part of the local day that a start screen greets.
#[derive(Clone, Copy)]
enum DayPart {
    Morning,
    Afternoon,
    Evening,
    Night,
}

impl DayPart {
    // Returns the part of the day at the local time now.
    fn now() -> Self {
        match chrono::Local::now().hour() {
            5..12 => Self::Morning,
            12..17 => Self::Afternoon,
            17..22 => Self::Evening,
            _ => Self::Night,
        }
    }

    // Returns the greeting that leads a start screen.
    fn greeting(self) -> &'static str {
        match self {
            Self::Morning => "Good morning",
            Self::Afternoon => "Good afternoon",
            Self::Evening => "Good evening",
            Self::Night => "Still up",
        }
    }
}

impl Shell {
    /// Builds the conversation with a measured floating composer and a clear review viewport.
    pub(super) fn chat(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sync_slash(cx);
        self.sync_context_arrivals(window, cx);
        let history = self.message_history(window, cx);
        let cover = self.chat_cover();
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
                self.settings.click_waves,
                self.settings.dot_screen,
                self.settings.wallpaper.clone(),
            ));
        // The snapshot is taken before the view is borrowed for the rest of the frame.
        let latest_shown = self.latest_wanted();
        self.latest_shown = latest_shown;
        if let Some(view) = &self.view {
            // The order of the rows is set when they change, not on every frame.
            self.chat_selection.update(cx, |doc, _| {
                doc.set_scroll(self.chat_scroll.clone(), px(8.))
            });
            panel = panel.child(self.chat_selection.clone());
            let state = &view.task.state;
            let center_diff =
                self.reviewing_file && self.settings.diff_placement == DiffPlacement::Conversation;
            if self.settings.skater && !center_diff {
                let (avatar, tone) = self.task_project().map_or_else(
                    || (SkaterAvatar::default(), t.accent),
                    |project| {
                        (
                            project.avatar.unwrap_or_default(),
                            sidebar::project_tint(project, t),
                        )
                    },
                );
                skater::cue(
                    skater::Cue {
                        key: view.task.id.clone().into(),
                        avatar,
                        tone,
                        working: view.task.state.active(),
                        following: !latest_shown,
                        covered: self.panel != Panel::None || self.slash.open(),
                    },
                    cx,
                );
            }
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
                .on_scroll_wheel(cx.listener(|s, e: &ScrollWheelEvent, window, cx| {
                    let delta = e.delta.pixel_delta(px(20.));
                    s.nudge_composer((f32::from(delta.y) * 0.015).clamp(-0.45, 0.45), cx);
                    // The wheel scrolls the conversation anywhere in the chat, also beside and
                    // below the list, unless a scroll container under the pointer takes it. The
                    // list handles the wheel over itself, and the composer scrolls its own draft.
                    let over_composer = fx::anchor_bounds(COMPOSER_ANCHOR, cx)
                        .is_some_and(|bounds| bounds.contains(&e.position));
                    if over_composer || s.chat_scroll.viewport_bounds().contains(&e.position) {
                        return;
                    }
                    let taken = window.take_scroll_delta(
                        s.chat_scroll.scroll_container_id(),
                        point(px(0.), delta.y),
                        point(false, true),
                    );
                    if taken.y != px(0.) {
                        s.chat_scroll.scroll_by(-taken.y);
                        cx.notify();
                    }
                }));
            if center_diff {
                timeline = timeline
                    .child(self.review(cx, true))
                    .child(div().h(gpui::px(self.composer_height)).flex_shrink_0());
            } else {
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
                    let skater_room = if self.settings.skater && !center_diff {
                        skater::ROOM
                    } else {
                        0.
                    };
                    let list_end = gpui::px(cover) + px(8. + skater_room);
                    timeline = timeline.child(
                        div()
                            .w_full()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .justify_center()
                            // The conversation runs on behind the composer's glass box and
                            // ends at the box's bottom edge, so its hints below stay clear.
                            .pb(gpui::px(self.composer_tail))
                            .child(
                                div()
                                    .relative()
                                    .w_full()
                                    .min_w_0()
                                    .h_full()
                                    // A secondary press over the conversation opens the menu of
                                    // its text selection at the pointer.
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(|s, event: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            s.selection_menu = Some(event.position);
                                            s.panel = Panel::SelectionMenu;
                                            cx.notify();
                                        }),
                                    )
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
                                        // At its end the last message rests above the box,
                                        // and above the lane of the composer's skater.
                                        .pb(list_end)
                                        // At its start the first message rests below the
                                        // control for earlier messages.
                                        .when(view.before.is_some(), |list| {
                                            list.pt(px(EARLIER_ROOM))
                                        }),
                                    )
                                    // The history rail and its preview lie over the list,
                                    // above the box.
                                    .child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .right_0()
                                            .bottom(gpui::px(cover))
                                            .children(history),
                                    ),
                            ),
                    );
                }
                timeline = timeline.child(dither::fade(t.canvas, 14., false).top_0());
                // The control for earlier messages floats centered over the top of the
                // conversation, clear of the header, so it reads as part of the transcript.
                if let Some(before) = view.before {
                    let id = view.task.id.clone();
                    timeline = timeline.child(
                        div()
                            .absolute()
                            .top(px(EARLIER_INSET))
                            .left_0()
                            .right_0()
                            .flex()
                            .justify_center()
                            .child(self.earlier_messages("older").on_click(cx.listener(
                                move |s, _, _, _| {
                                    s.send(core::Request::Task {
                                        task_id: id.clone(),
                                        before: Some(before),
                                    })
                                },
                            ))),
                    );
                }
            }
            let bounds_entity = cx.entity();
            // The jump shows only while the user has scrolled away from the end, centered over
            // the composer, and in the accent once something arrived below.
            let latest = latest_shown.then(|| {
                let unseen = self.unseen_below;
                let jump = if unseen {
                    // The tint is faint, so the conversation behind it is blurred like glass.
                    self.tinted("jump-latest", "arrow-down", "New below", t.accent)
                        .backdrop_blur(px(kit::GLASS_BLUR))
                } else {
                    self.button("jump-latest", "arrow-down", "Latest")
                };
                jump.h(px(24.))
                    .accessibility_label("Scroll to the latest message")
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.follow_latest();
                        cx.notify();
                    }))
            });
            let floating = div().absolute().bottom_0().w_full().child(
                div()
                    .relative()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .relative()
                            .h(px(COMPOSER_LEAD))
                            .flex_shrink_0()
                            .flex()
                            .justify_center()
                            .items_start()
                            .pt(px(1.))
                            .children(latest),
                    )
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
                                    // The box laid out before this point. Its spring offset is
                                    // left out, so the conversation stays still while it bounces.
                                    if let Some(editor) = fx::anchor_bounds(COMPOSER_ANCHOR, cx) {
                                        let rest = editor.bottom() - px(s.composer_spring.offset);
                                        let tail = f32::from(bounds.bottom() - rest).ceil().max(0.);
                                        if (s.composer_tail - tail).abs() > 0.5 {
                                            s.composer_tail = tail;
                                            cx.notify();
                                        }
                                    }
                                    // The list laid out before this point, and its layout
                                    // resumes following when the user scrolls back to the end.
                                    if s.chat_scroll.is_following_tail() {
                                        s.unseen_below = false;
                                    }
                                    if s.latest_shown != s.latest_wanted() {
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
        } else if self.selected.is_some() {
            // A task whose conversation is loading shows its composer where the conversation
            // keeps it, so the start screen does not flash between two tasks.
            panel = panel.child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.docked_composer(false, window, cx)),
            );
        } else {
            let launch = self
                .launch
                .as_ref()
                .map(|launch| (launch.from, launch.vanished()));
            // While the composer launches, an empty box of its size keeps the rest in place.
            let composer = match launch {
                Some((from, _)) => div()
                    .h(gpui::px(f32::from(from.size.height)) + px(METER_TAB_HEIGHT - 1.))
                    .into_any_element(),
                None => self.composer(true, window, cx),
            };
            let column = div()
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
                .child(composer)
                .child(self.shortcut_hints());
            panel = panel.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .id("new-task-scroll")
                    .overflow_y_scroll()
                    // A secondary press on the background around the start screen opens the
                    // background's menu at the pointer. The composer keeps its own press.
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|s, event: &MouseDownEvent, _, cx| {
                            let over_composer = fx::anchor_bounds(COMPOSER_ANCHOR, cx)
                                .is_some_and(|bounds| bounds.contains(&event.position));
                            if over_composer {
                                return;
                            }
                            cx.stop_propagation();
                            s.backdrop_menu = Some(event.position);
                            s.panel = Panel::BackdropMenu;
                            cx.notify();
                        }),
                    )
                    .child(motion::vanish(
                        column,
                        launch.map_or(0., |(_, vanished)| vanished),
                        t.canvas,
                    )),
            );
            if launch.is_some() {
                panel = panel.child(self.docked_composer(true, window, cx));
            }
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

    /// Returns the composer where a conversation keeps it: over the bottom of its parent, below
    /// the room for the jump to the latest message. The parent must be positioned.
    fn docked_composer(&mut self, empty: bool, window: &mut Window, cx: &mut Context<Self>) -> Div {
        div()
            .absolute()
            .bottom_0()
            .w_full()
            .flex()
            .flex_col()
            .child(div().h(px(COMPOSER_LEAD)).flex_shrink_0())
            .child(div().w_full().child(self.composer(empty, window, cx)))
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
            .map(usage::session_cost);
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
            .when(task.kind.has_project(), |d| {
                let count = self
                    .task_project()
                    .map_or(0, |project| project.commands.len());
                d.child(
                    self.raised("task-commands")
                        .h(px(24.))
                        .px(px(7.))
                        .gap(px(6.))
                        // While its menu is open, the button takes the depth of a chosen surface.
                        .when(self.panel == Panel::ProjectCommands, |b| {
                            b.bg(kit::tint_fill(t.accent, 0.1))
                                .shadow(kit::tint_light(t.accent))
                                .border_color(t.accent.opacity(0.36))
                        })
                        .accessibility_label("Run a project command")
                        .child(icon("play").size(px(12.)).text_color(t.muted))
                        .when(count > 0, |b| b.child(figure(count.to_string(), t.muted)))
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: "Run a project command in the terminal".into(),
                                theme: t,
                            })
                            .into()
                        })
                        .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                            s.menu_anchor = Some(*bounds);
                            s.panel = Panel::ProjectCommands;
                            cx.notify();
                        })),
                )
            })
            .when_some(changes, |d, summary| {
                let files = summary.files;
                let omitted = summary.omitted_files;
                d.child(
                    self.raised("task-changes")
                        .h(px(24.))
                        .px(px(7.))
                        .gap(px(7.))
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

    // The heading of a start screen: a greeting, then what the workspace starts, both in words
    // for the time of day. Agentic Coding and Review name the project the work happens in, as a
    // control that changes it.
    fn empty_chat_greeting(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let part = DayPart::now();
        let greeting = format!("{}, {}", part.greeting(), self.greeting_name);
        let (label, lead, tail) = match self.mode {
            TaskKind::Coding => {
                let (lead, tail) = match part {
                    DayPart::Morning => ("What should we start in", "?"),
                    DayPart::Afternoon => ("What should we build in", "?"),
                    DayPart::Evening => ("What should we finish in", "tonight?"),
                    DayPart::Night => ("What should we fix in", "before bed?"),
                };
                (greeting, lead, tail)
            }
            TaskKind::Chat => (
                greeting,
                match part {
                    DayPart::Morning => "What is on your mind this morning?",
                    DayPart::Afternoon => "What is on your mind?",
                    DayPart::Evening => "What is on your mind this evening?",
                    DayPart::Night => "What is keeping you up?",
                },
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
            // A raised tinted plate: lit along its top, deeper along its bottom, dotted where
            // its light fades. It sinks into the heading while pressed.
            self.control("greeting-project")
                .h_auto()
                .pl(px(10.))
                .pr(px(8.))
                .py(px(2.))
                .gap(px(8.))
                .overflow_hidden()
                .layer(kit::shade(t.accent.opacity(0.3), dither::Edge::Bottom))
                .layer(kit::glint(t.accent.opacity(0.55), dither::Edge::Bottom))
                .border_color(t.accent.opacity(0.45))
                .bg(kit::tint_fill(t.accent, 0.14))
                .shadow(kit::tint_light(t.accent))
                .hover(move |b| {
                    b.bg(kit::tint_fill(t.accent, 0.22))
                        .border_color(t.accent.opacity(0.7))
                })
                .active(move |b| b.bg(t.accent.opacity(0.1)).shadow(kit::recessed_light(t)))
                .on_press(move |position, cx| fx::ripple(position, t.accent, cx))
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
                .child(kit::engraved("chevrons-up-down", t.accent, 16.))
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
        let fast_mode = selected_model.and_then(|model| model.fast.clone());
        let fast = preferences.fast;
        let options_locked = self.agent_options_locked();
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
        // A start screen's composer that launches takes the place of a conversation's composer
        // and changes into it: `docking` goes from 0, the start screen's, to 1, the docked one.
        let launch = self.launch.as_ref().filter(|_| empty).map(|launch| {
            let progress = launch.progress();
            (progress, launch.from.origin, launch.box_width(progress))
        });
        let docked = !empty || launch.is_some();
        let docking = if empty {
            launch.map_or(0., |(progress, ..)| progress)
        } else {
            1.
        };
        let mut wrapper = div()
            .relative()
            .w_full()
            .flex_shrink_0()
            .max_w(px(900.))
            .mx_auto()
            .when(docked, |d| d.px(px(20.)).pb(px(10.)));
        if launch.is_some() {
            // The box ends as wide as the docked composer, which is measured here every frame.
            let entity = cx.entity();
            wrapper = wrapper.child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |s, cx| {
                            let width = f32::from(bounds.size.width - px(40.));
                            if let Some(launch) = &mut s.launch
                                && launch.width != Some(width)
                            {
                                launch.width = Some(width);
                                cx.notify();
                            }
                        })
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        }
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
                let task_id = task.id.clone();
                let id = attachment.id.clone();
                files = files.child(motion::context_reveal(
                    self.attachment_card(attachment, cx).child(
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
                        })),
                    ),
                    self.context_arrivals.get(&attachment.id).copied().flatten(),
                    cx.reduce_motion(),
                ));
            }
            attachments = attachments.child(files);
        } else {
            // A new task's attachments are not imported yet, but their cards already show what
            // the conversation will: the preview of an image, the size of a file.
            let mut files = div().flex().flex_wrap().flex_shrink_0().gap_2();
            for (index, pending) in self.pending_attachments.iter().enumerate() {
                files = files.child(motion::context_reveal(
                    self.attachment_card(&pending.card, cx).child(
                        self.icon_button(
                            ("remove-pending-file", index),
                            "close",
                            "Remove attachment",
                        )
                        .size(px(20.))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let pending = s.pending_attachments.remove(index);
                            s.clipboard_images.remove(pending.source.path());
                            cx.notify();
                        })),
                    ),
                    self.context_arrivals
                        .get(&pending.card.id)
                        .copied()
                        .flatten(),
                    cx.reduce_motion(),
                ));
            }
            attachments = attachments.child(files);
        }
        // Long pasted texts are files of the message, after its attachments.
        // The draft shows its inline code as the message will, in the current theme.
        self.input
            .update(cx, |input, _| input.set_live_code(Some(t)));
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
        // A conversation's plan sits in a tab on the composer's leading corner.
        let plan_tab = if empty { None } else { self.plan_tab(cx) };
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
            // The meter tab continues this corner upward, and the plan tab the other one.
            .rounded_tr(px(0.))
            .when(plan_tab.is_some(), |editor| editor.rounded_tl(px(0.)))
            // Glass like the side panels, but denser, because the draft is read over the
            // conversation behind it. The meter tab on its edge is the same glass.
            .map(|editor| kit::glass(editor, t.raised, kit::SURFACE_GLASS_FILL))
            // Lit along its top edge, like the floating surfaces it stands with.
            .shadow(kit::plate_light(t, false))
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
                    .child(self.question_panel(cx))
                    // The next messages are a row of the composer, right above the draft.
                    .child(self.outgoing_tray(cx))
            })
            .when(has_context, |d| {
                d.child(div().px(px(12.)).pt(px(10.)).child(attachments))
            });
        // A new review names what it examines in a row above the draft.
        if empty && mode == TaskKind::Review && docking < 1. {
            editor = editor.child(launch_fold(self.review_pick_status(), docking));
        }
        // A new task says where it will work in a row above the draft.
        if empty && mode == TaskKind::Coding && docking < 1. {
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
            editor = editor.child(launch_fold(status, docking));
        }
        editor = editor.child(
            div()
                .id("composer-editor")
                .px(px(14.))
                .pt(px(6. + 6. * docking))
                .pb(px(6.))
                .min_h(px(92. - 36. * docking))
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
            // An energy toggle for the model's fast mode. On, it carries the accent; it never
            // takes the warning color, though it uses more of the account's usage.
            .when_some(fast_mode, |d, fast_mode| {
                let hint = fast_hint(fast, &fast_mode.detail);
                d.child(
                    self.chip_button("composer-fast")
                        .w(px(24.))
                        .px_0()
                        .disabled(options_locked)
                        .when(fast, |b| kit::lit(b, t.accent))
                        .accessibility_label(if fast {
                            "Fast mode: on. Uses more usage. Click to turn it off"
                        } else {
                            "Fast mode: off. Click to turn it on; it uses more usage"
                        })
                        .child(icon("zap").size(px(13.)))
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: hint.clone(),
                                theme: t,
                            })
                            .into()
                        })
                        .on_click(cx.listener(|s, _, _, cx| s.toggle_fast(cx))),
                )
            })
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
            || self.launch.is_some()
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
        // The whole tab is the meters' control, so its hover fills the tab's own shape: while
        // the pointer is over it, while its panel is open and while it has keyboard focus.
        let lit = self.meter_tab_hovered || self.panel == Panel::Usage;
        let focus = self.meter_tab_focus.clone();
        let tab = self
            .usage_meters(cx)
            .when(!empty, |tab| {
                tab.child(skater::ground(skater::Part::Ledge {
                    leading: false,
                    foot: px(METER_TAB_FOOT),
                    slant: px(METER_TAB_SLANT),
                }))
            })
            .absolute()
            .right_0()
            .top(px(1. - METER_TAB_HEIGHT))
            .h(px(METER_TAB_HEIGHT))
            .pl(px(METER_TAB_FOOT + METER_TAB_SLANT + 12.))
            .pr(px(12.))
            .track_focus(&self.meter_tab_focus)
            .on_hover(cx.listener(|s, hovered: &bool, _, cx| {
                if s.meter_tab_hovered != *hovered {
                    s.meter_tab_hovered = *hovered;
                    cx.notify();
                }
            }))
            .layer(move |_: Corners<Pixels>| {
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let lit =
                            lit || (focus.is_focused(window) && window.last_input_was_keyboard());
                        paint_edge_tab(
                            bounds,
                            TabCorner::Trailing,
                            t.raised,
                            lit.then_some(t.selected),
                            tab_edge,
                            window,
                        )
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .into_any_element()
            });
        // The plan tab mirrors the meter tab and takes at most half of the edge, so the two
        // never meet. Its current step shortens first.
        let plan_tab = plan_tab.map(|plan| {
            let lit = self.plan_tab_hovered || self.panel == Panel::Plan;
            let focus = self.plan_tab_focus.clone();
            plan.absolute()
                .left_0()
                .top(px(1. - METER_TAB_HEIGHT))
                .h(px(METER_TAB_HEIGHT))
                .max_w(relative(0.5))
                .pl(px(12.))
                .pr(px(METER_TAB_FOOT + METER_TAB_SLANT + 12.))
                .child(skater::ground(skater::Part::Ledge {
                    leading: true,
                    foot: px(METER_TAB_FOOT),
                    slant: px(METER_TAB_SLANT),
                }))
                .track_focus(&self.plan_tab_focus)
                .on_hover(cx.listener(|s, hovered: &bool, _, cx| {
                    if s.plan_tab_hovered != *hovered {
                        s.plan_tab_hovered = *hovered;
                        cx.notify();
                    }
                }))
                .layer(move |_: Corners<Pixels>| {
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let lit = lit
                                || (focus.is_focused(window) && window.last_input_was_keyboard());
                            paint_edge_tab(
                                bounds,
                                TabCorner::Leading,
                                t.raised,
                                lit.then_some(t.selected),
                                tab_edge,
                                window,
                            )
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .into_any_element()
                })
        });
        let composer_box = div()
            .relative()
            .mt(px(METER_TAB_HEIGHT - 1.))
            .top(px(self.composer_spring.offset))
            // The conversation scrolls behind the box. The box holds the pointer, so the
            // wheel over it scrolls the draft and not the conversation under it.
            .occlude()
            .child(dither::shadow(t, motion::COMPOSER_RADIUS, 4.))
            .child(editor)
            .child(tab)
            .children(plan_tab)
            // A conversation's composer is the ground that its skater rides on.
            .when(!empty, |composer| {
                composer.child(skater::ground(skater::Part::Edge))
            });
        wrapper = wrapper.child(match launch {
            // The box widens or narrows to the docked width while it travels there.
            Some((progress, from, width)) => motion::morph(
                composer_box.w(gpui::px(width)).flex_shrink_0(),
                from,
                progress,
            ),
            None => composer_box.into_any_element(),
        });
        if docked {
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
            wrapper = wrapper.child(footer.opacity(docking));
        }
        wrapper
            .child(self.slash_menu(empty, window, cx))
            .into_any_element()
    }

    /// Returns the control that loads an earlier page of a conversation. It is a raised pill
    /// over blurred glass, the counterpart of the jump to the latest message.
    pub(super) fn earlier_messages(&self, id: &'static str) -> Button {
        self.button(id, "arrow-up", "Earlier messages")
            .h(px(24.))
            .backdrop_blur(px(kit::GLASS_BLUR))
            .accessibility_label("Load earlier messages")
    }

    // A composer chip that opens one of the agent menus. It behaves as a ghost button: a plate
    // under the pointer and a sinking press. While its menu is open, it takes the accent tint
    // with the depth of a chosen surface.
    fn chip_button(&self, id: &'static str) -> Button {
        let t = self.theme;
        self.control(id)
            .h(px(24.))
            .px(px(7.))
            .gap(px(6.))
            .overflow_hidden()
            .layer(kit::glint(t.dot, dither::Edge::Bottom))
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .text_color(t.muted)
            .hover(move |s| {
                s.bg(t.selected)
                    .text_color(t.text)
                    .shadow(kit::plate_light(t, false))
            })
            .active(move |s| s.bg(t.control_active).shadow(kit::recessed_light(t)))
            .on_press(move |position, cx| fx::ripple(position, t.muted, cx))
            .styles(move |s| {
                s.selected(|s| {
                    s.bg(kit::tint_fill(t.accent, 0.08))
                        .shadow(kit::tint_light(t.accent))
                        .text_color(t.text)
                })
            })
    }
}

// Explains the fast mode toggle: its state, the provider's description of the speed-up, and
// its cost in usage.
fn fast_hint(on: bool, detail: &str) -> String {
    let (state, action) = if on {
        ("Fast mode on", "off")
    } else {
        ("Fast mode off", "on")
    };
    let detail = detail.trim().trim_end_matches('.');
    let detail = if detail.is_empty() {
        "Faster output from the same model"
    } else {
        detail
    };
    format!(
        "{state} · {detail}. Fast mode uses more of your plan's usage. Click to turn it {action}."
    )
}

/// Height of the strip above the floating composer that holds the jump-to-latest button. The
/// conversation runs beneath it.
pub(super) const COMPOSER_LEAD: f32 = 28.;

/// Distance of the control for earlier messages from the top of the conversation, and the room
/// above the first message that keeps the control clear of it at the start of the list.
const EARLIER_INSET: f32 = 12.;
const EARLIER_ROOM: f32 = EARLIER_INSET + 24. + 8.;

/// Height of the meter tab on the composer's top edge, and the run of its slanted side.
const METER_TAB_HEIGHT: f32 = 24.;
const METER_TAB_SLANT: f32 = 14.;
/// Room before the slant in which the tab's foot curves out of the composer's edge, and how far
/// from each corner of the slant its rounding starts.
const METER_TAB_FOOT: f32 = 7.;
const METER_TAB_ROUNDING: f32 = 6.;
/// Radius of the circle whose edge blurs along a tab's slant. Across the slant's length the edge
/// strays from a straight line by under a tenth of a point.
const METER_TAB_SLANT_BLUR_RADIUS: f32 = 2048.;

/// The corner of the composer's top edge that a tab continues.
#[derive(Clone, Copy)]
enum TabCorner {
    /// The plan tab's corner.
    Leading,
    /// The meter tab's corner.
    Trailing,
}

// Paints a tab on the composer's top edge: a surface with a rounded outer corner that
// continues the composer's corner, and a slanted inner side. The slant curves out of the
// composer's edge at its foot and into the top edge at its head. The bottom side has no edge,
// because it joins the composer. A leading tab is the trailing one mirrored.
fn paint_edge_tab(
    bounds: Bounds<Pixels>,
    corner: TabCorner,
    surface: Hsla,
    hover: Option<Hsla>,
    edge: Hsla,
    window: &mut Window,
) {
    let radius = px(motion::COMPOSER_RADIUS);
    // The edge runs through pixel centers, which keeps a one-pixel stroke sharp. The stroke
    // stays one window pixel wide at every interface scale.
    let half = gpui::px(0.5);
    // `inner` is where the foot starts, where the composer's own edge ends, and `outer` is
    // the rounded side. `toward` moves a point by a distance in the direction of `outer`.
    let (inner, outer, sign) = match corner {
        TabCorner::Trailing => (bounds.left(), bounds.right() - half, 1.),
        TabCorner::Leading => (bounds.right(), bounds.left() + half, -1.),
    };
    let toward = |from: Pixels, distance: Pixels| from + distance * sign;
    let (top, bottom) = (bounds.top() + half, bounds.bottom());
    let foot = point(toward(inner, px(METER_TAB_FOOT)), bottom);
    let head = point(toward(foot.x, px(METER_TAB_SLANT)), top);
    // A step of the rounding along the slant.
    let rise = f32::from(bottom - top);
    let slant = f32::from(px(METER_TAB_SLANT));
    let rounding = f32::from(px(METER_TAB_ROUNDING));
    let length = (slant * slant + rise * rise).sqrt();
    let along = point(
        gpui::px(sign * slant / length * rounding),
        gpui::px(-rise / length * rounding),
    );
    // `base` is where the foot starts: on the composer's edge for the stroke, so that the two
    // lines join, and at the tab's bottom for the fill, so that it covers that edge.
    let outline = |builder: &mut PathBuilder, base: Pixels| {
        builder.move_to(point(inner, base));
        builder.curve_to(foot + along, point(foot.x, base));
        builder.line_to(head - along);
        builder.curve_to(point(toward(head.x, px(METER_TAB_ROUNDING)), top), head);
        builder.line_to(point(toward(outer, -radius), top));
        builder.curve_to(point(outer, top + radius), point(outer, top));
        builder.line_to(point(outer, bottom));
    };
    // The tab is glass like the composer: the field behind it is blurred, then tinted. A backdrop
    // is a rounded rectangle, so two of them cover the tab. The first is a circle so large that
    // its edge runs straight along the slant, clipped to the tab short of its outer corner. The
    // second is a column at the outer side whose rounded corner continues the composer's. Each
    // blur adds a pass, so the tab keeps to these two. The blur stops above the composer's edge.
    let seam_top = bottom - gpui::px(1.);
    let filters = [Filter::Blur(px(kit::GLASS_BLUR))];
    let circle = px(METER_TAB_SLANT_BLUR_RADIUS);
    // The circle's center lies inside the tab, square to the slant's middle, so that its edge
    // touches the slant there.
    let middle = point((foot.x + head.x) / 2., (foot.y + head.y) / 2.);
    let center = point(
        middle.x + circle * (sign * rise / length),
        middle.y + circle * (slant / length),
    );
    let disc = Bounds::new(
        point(center.x - circle, center.y - circle),
        size(circle * 2., circle * 2.),
    );
    let column = radius * 2.;
    let (slant_clip, slant_corners, outer_blur, outer_corners) = match corner {
        TabCorner::Trailing => (
            Bounds::from_corners(
                point(bounds.left(), bounds.top()),
                point(bounds.right() - column, seam_top),
            ),
            Corners {
                top_left: circle,
                ..Corners::default()
            },
            Bounds::from_corners(
                point(bounds.right() - column, bounds.top()),
                point(bounds.right(), seam_top),
            ),
            Corners {
                top_right: radius,
                ..Corners::default()
            },
        ),
        TabCorner::Leading => (
            Bounds::from_corners(
                point(bounds.left() + column, bounds.top()),
                point(bounds.right(), seam_top),
            ),
            Corners {
                top_right: circle,
                ..Corners::default()
            },
            Bounds::from_corners(
                point(bounds.left(), bounds.top()),
                point(bounds.left() + column, seam_top),
            ),
            Corners {
                top_left: radius,
                ..Corners::default()
            },
        ),
    };
    window.with_content_mask(Some(ContentMask { bounds: slant_clip }), |window| {
        window.paint_backdrop_filter(disc, slant_corners, &filters);
    });
    window.paint_backdrop_filter(outer_blur, outer_corners, &filters);
    // The composer's edge runs under the tab's bottom row. A see-through fill would show it as
    // a line across the tab, so that row stays solid in the surface tone and the tab's own
    // stroke is the only line there. The row stops short of the outer stroke. A hover tone lies
    // over the whole surface in the same shape.
    let seam = Bounds::new(
        point(
            match corner {
                TabCorner::Trailing => bounds.left(),
                TabCorner::Leading => bounds.left() + gpui::px(1.),
            },
            seam_top,
        ),
        size(bounds.size.width - gpui::px(1.), gpui::px(1.)),
    );
    window.paint_quad(fill(seam, surface));
    for tone in [Some(surface.opacity(kit::SURFACE_GLASS_FILL)), hover]
        .into_iter()
        .flatten()
    {
        let mut body = PathBuilder::fill();
        outline(&mut body, bottom);
        body.close();
        if let Ok(path) = body.build() {
            window.paint_path(path, tone);
        }
        window.paint_quad(fill(seam, tone));
    }
    let mut stroke = PathBuilder::stroke(gpui::px(1.));
    outline(&mut stroke, bottom - half);
    if let Ok(path) = stroke.build() {
        window.paint_path(path, edge);
    }
}

// Folds a row of the start screen's composer away while the composer docks. `docking` goes from
// 0, the row in full, to 1, folded.
fn launch_fold(row: impl IntoElement, docking: f32) -> AnyElement {
    if docking <= 0. {
        return row.into_any_element();
    }
    div()
        .max_h(px(40. * (1. - docking)))
        .overflow_hidden()
        .opacity(1. - docking)
        .child(row)
        .into_any_element()
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
