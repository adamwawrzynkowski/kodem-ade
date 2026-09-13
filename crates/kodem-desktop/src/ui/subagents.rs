//! Child conversations stay attached to the composer and open in a focused popup.
use super::*;

pub(super) struct AgentPopup {
    pub(super) thread_id: String,
    pub(super) error: Option<String>,
    pub(super) view: Option<SubagentView>,
    pub(super) scroll: ListState,
    pub(super) selection: Entity<crate::components::text_document::TextDocument>,
    focus: FocusHandle,
}

fn glyph(index: usize) -> &'static str {
    const ICONS: [&str; 12] = [
        "bot",
        "code",
        "search",
        "book",
        "cube",
        "branch",
        "square-terminal",
        "file-text",
        "check-square",
        "shield",
        "folder",
        "settings-2",
    ];
    ICONS[index % ICONS.len()]
}

fn state_label(state: &RunState) -> &'static str {
    match state {
        RunState::Starting => "Starting",
        RunState::Running => "Working",
        RunState::Waiting => "Needs input",
        RunState::Completed => "Finished",
        RunState::Interrupted => "Stopped",
        RunState::Failed => "Failed",
        RunState::DeliveryUnknown => "Status unknown",
        RunState::Idle => "Idle",
    }
}

impl Shell {
    /// Keeps child questions and approvals inside their own conversation.
    pub(super) fn request_in_current_thread(&self, request: &InputRequest) -> bool {
        let thread = request.params["threadId"].as_str();
        if let Some(popup) = &self.agent_popup {
            return thread == Some(&popup.thread_id);
        }
        self.view.as_ref().is_none_or(|view| {
            !view
                .task
                .subagents
                .iter()
                .any(|agent| Some(agent.thread_id.as_str()) == thread)
        })
    }

    fn open_agent(&mut self, thread_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = &self.view else { return };
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.send(core::Request::SubagentThread {
            task_id: view.task.id.clone(),
            thread_id: thread_id.clone(),
            before: None,
        });
        self.agent_popup = Some(AgentPopup {
            thread_id,
            error: None,
            view: None,
            scroll: ListState::new(0, ListAlignment::Bottom, px(600.)),
            selection: cx.new(|_| Default::default()),
            focus,
        });
        cx.notify();
    }

    fn close_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.agent_popup = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Opens the identified descendant using its persistent name, glyph and live status.
    pub(super) fn agent_link(&self, thread_id: &str, cx: &mut Context<Self>) -> AnyElement {
        let Some((index, agent)) = self.view.as_ref().and_then(|view| {
            view.task
                .subagents
                .iter()
                .enumerate()
                .find(|(_, a)| a.thread_id == thread_id)
        }) else {
            return div().child("Agent thread unavailable").into_any_element();
        };
        let id = agent.thread_id.clone();
        let t = self.theme;
        self.ghost(
            (
                ElementId::from("agent-link"),
                SharedString::from(id.clone()),
            ),
            glyph(index),
            agent.name.clone(),
        )
        .h(px(32.))
        .child(if agent.state.active() {
            motion::active_icon(
                (
                    ElementId::from("agent-progress"),
                    SharedString::from(id.clone()),
                ),
                "loader",
                t.accent,
                true,
                cx.reduce_motion(),
            )
        } else {
            div().into_any_element()
        })
        .child(
            div()
                .text_size(px(11.))
                .text_color(t.muted)
                .child(state_label(&agent.state)),
        )
        .child(icon("external-link").size(px(12.)))
        .on_click(cx.listener(move |s, _, window, cx| s.open_agent(id.clone(), window, cx)))
        .into_any_element()
    }

    /// Renders child controls as a connected section of the existing prompt container.
    pub(super) fn subagents_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = &self.view else {
            return div().into_any_element();
        };
        if view.task.subagents.is_empty() {
            return div().into_any_element();
        }
        let active = view
            .task
            .subagents
            .iter()
            .filter(|agent| agent.state.active())
            .count();
        div()
            .id("composer-agents")
            .px_3()
            .py_2()
            .max_h(px(156.))
            .overflow_y_scroll()
            .border_b_1()
            .border_color(self.theme.border)
            .bg(self.theme.sidebar)
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(self.theme.muted)
                    .child(if active > 0 {
                        format!("Agents · {active} working")
                    } else {
                        "Agents".into()
                    }),
            )
            .child(
                div().flex().flex_wrap().gap_1().children(
                    view.task
                        .subagents
                        .iter()
                        .map(|agent| self.agent_link(&agent.thread_id, cx)),
                ),
            )
            .into_any_element()
    }

    /// Merges tail updates and earlier pages without changing the main conversation scroll.
    pub(super) fn update_agent_popup(
        &mut self,
        mut view: SubagentView,
        older: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = &mut self.agent_popup else {
            return;
        };
        if self.selected.as_deref() != Some(&view.task_id)
            || popup.thread_id != view.agent.thread_id
        {
            return;
        }
        let follow = !older && popup.scroll.is_scrolled_to_end().unwrap_or(true);
        let mut anchor = popup.scroll.logical_scroll_top();
        let history_completed = popup
            .view
            .as_ref()
            .is_some_and(|old| !old.agent.history_loaded && view.agent.history_loaded);
        let mut reordered = history_completed;
        if let Some(previous) = popup.view.as_ref().filter(|_| !history_completed) {
            let anchor_id = previous.messages.get(anchor.item_ix).map(|m| &m.id);
            let mut messages: HashMap<String, Message> = previous
                .messages
                .iter()
                .cloned()
                .map(|m| (m.id.clone(), m))
                .collect();
            reordered |= view.messages.iter().any(|message| {
                messages
                    .get(&message.id)
                    .is_some_and(|previous| previous.seq != message.seq)
            });
            messages.extend(view.messages.into_iter().map(|m| (m.id.clone(), m)));
            view.messages = messages.into_values().collect();
            view.messages.sort_by_key(|m| m.seq);
            if !older && !previous.messages.is_empty() {
                view.before = previous.before;
            }
            if let Some(index) =
                anchor_id.and_then(|id| view.messages.iter().position(|m| &m.id == id))
            {
                anchor.item_ix = index;
            }
        }
        let changed = popup.view.as_ref().is_none_or(|old| {
            old.messages.len() != view.messages.len()
                || old.messages.iter().zip(&view.messages).any(|(a, b)| {
                    a.id != b.id
                        || a.seq != b.seq
                        || a.text != b.text
                        || a.complete != b.complete
                        || a.file_edits != b.file_edits
                })
        });
        if reordered {
            popup.selection = cx.new(|_| Default::default());
        }
        if changed {
            popup.scroll.reset(view.messages.len());
            if follow || history_completed {
                popup.scroll.scroll_to_end();
            } else {
                popup.scroll.scroll_to(anchor);
            }
        }
        popup.error = None;
        popup.view = Some(view);
    }

    /// Renders an almost fullscreen child thread with isolated focus and the task stop control.
    pub(super) fn render_agent_popup(&mut self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let popup = self.agent_popup.as_ref()?;
        let thread_id = popup.thread_id.clone();
        let focus = popup.focus.clone();
        let scroll = popup.scroll.clone();
        let selection = popup.selection.clone();
        selection.update(cx, |doc, _| {
            doc.set_viewport(
                scroll.clone(),
                popup
                    .view
                    .as_ref()
                    .map(|view| view.messages.iter().map(|message| message.seq).collect())
                    .unwrap_or_default(),
                px(0.),
            )
        });
        let conversation = Conversation::Subagent(thread_id.clone());
        let before = popup.view.as_ref().and_then(|view| view.before);
        let loaded = popup.view.is_some();
        let load_error = popup.error.clone();
        let (index, agent) = self
            .view
            .as_ref()?
            .task
            .subagents
            .iter()
            .enumerate()
            .find(|(_, agent)| agent.thread_id == thread_id)?;
        let name = agent.name.clone();
        let state = agent.state.clone();
        let history_loading = !agent.history_loaded;
        let task = self.view.as_ref()?.task.clone();
        let t = self.theme;
        let mut header = div()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(t.border)
            .child(icon(glyph(index)))
            .child(name)
            .child(div().text_color(t.muted).child(state_label(&state)))
            .when(state.active(), |d| {
                d.child(motion::active_icon(
                    "popup-agent-progress",
                    "loader",
                    t.accent,
                    true,
                    cx.reduce_motion(),
                ))
            })
            .when(history_loading, |d| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.muted)
                        .child("Loading history…"),
                )
            })
            .child(div().flex_1());
        if task.state.active() {
            let task_id = task.id.clone();
            header = header.child(self.ghost("stop-agent-task", "stop", "Stop task").on_click(
                cx.listener(move |s, _, _, _| {
                    s.send(core::Request::Interrupt {
                        task_id: task_id.clone(),
                    })
                }),
            ));
        }
        header = header.child(
            self.icon_button("close-agent", "close", "Close agent thread")
                .on_click(cx.listener(|s, _, window, cx| s.close_agent(window, cx))),
        );
        let entity = cx.entity();
        let mut body = div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(selection);
        if let Some(error) = load_error {
            body = body.child(div().p_3().text_color(t.removed).child(error));
        }
        if let Some(before) = before {
            body = body.child(
                self.ghost("agent-earlier", "arrow-up", "Earlier messages")
                    .on_click(cx.listener(move |s, _, _, _| {
                        s.send(core::Request::SubagentThread {
                            task_id: task.id.clone(),
                            thread_id: thread_id.clone(),
                            before: Some(before),
                        })
                    })),
            );
        }
        body = if loaded {
            body.child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .justify_center()
                    .child(
                        list(scroll, move |index, _, cx| {
                            entity.update(cx, |s, cx| {
                                s.conversation_message(&conversation, index, cx)
                            })
                        })
                        .w_full()
                        .max_w(px(940.))
                        .min_w_0()
                        .h_full()
                        .py_3(),
                    ),
            )
        } else {
            body.child(
                div()
                    .p_5()
                    .text_color(t.muted)
                    .child("Loading agent thread…"),
            )
        };
        body = body.child(self.question_panel(cx));
        let requests: Vec<_> = self
            .view
            .as_ref()
            .into_iter()
            .flat_map(|v| &v.requests)
            .filter(|r| self.request_in_current_thread(r) && r.method.ends_with("requestApproval"))
            .cloned()
            .collect();
        for (index, request) in requests.iter().enumerate() {
            let permission = div()
                .p_3()
                .border_t_1()
                .border_color(t.border)
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    request.params["reason"]
                        .as_str()
                        .unwrap_or("Agent needs permission")
                        .to_owned(),
                )
                .child(self.readonly(
                    format!("agent-approval:{}", request.id),
                    serde_json::to_string_pretty(&request.params).unwrap_or_default(),
                    cx,
                ));
            let decisions = request.params["availableDecisions"]
                .as_array()
                .cloned()
                .unwrap_or_else(|| {
                    vec![
                        serde_json::json!("accept"),
                        serde_json::json!("decline"),
                        serde_json::json!("cancel"),
                    ]
                });
            let mut buttons = div().flex().gap_2();
            for (choice, decision) in decisions.into_iter().enumerate() {
                let Some(decision) = decision.as_str().map(str::to_owned) else {
                    continue;
                };
                let request = request.clone();
                let label = match decision.as_str() {
                    "accept" => "Allow once",
                    "acceptForSession" => "Allow for session",
                    "decline" => "Decline",
                    "cancel" => "Cancel turn",
                    _ => &decision,
                }
                .to_owned();
                buttons = buttons.child(
                    self.ghost(("agent-permission", index * 16 + choice), "shield", label)
                        .on_click(cx.listener(move |s, _, _, _| {
                            s.send(core::Request::Answer {
                                task_id: request.task_id.clone(),
                                id: request.id.clone(),
                                response: serde_json::json!({"decision":decision}),
                            })
                        })),
                );
            }
            body = body.child(permission.child(buttons));
        }
        Some(
            div()
                .id("agent-popup")
                .track_focus(&focus)
                .key_context("Workspace")
                .tab_group()
                .size_full()
                .p(px(16.))
                .pt(px(44.))
                .bg(t.canvas)
                .text_color(t.text)
                .font_family("IBM Plex Sans")
                .text_size(px(self.settings.font_size - 1.))
                .line_height(relative(1.5))
                .on_action(cx.listener(|s, _: &Cancel, window, cx| s.close_agent(window, cx)))
                .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
                .child(
                    div()
                        .size_full()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .rounded(px(12.))
                        .border_1()
                        .border_color(t.border)
                        .shadow(t.elevation())
                        .bg(t.canvas)
                        .overflow_hidden()
                        .child(header)
                        .child(body),
                ),
        )
    }
}
