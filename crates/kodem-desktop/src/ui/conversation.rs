//! Shared message presentation for the main conversation and child thread popups.
use super::*;
use crate::markdown;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Conversation {
    Main,
    Subagent(String),
}

impl Shell {
    /// Returns only the messages owned by the requested conversation.
    pub(super) fn conversation_messages(&self, conversation: &Conversation) -> Option<&[Message]> {
        match conversation {
            Conversation::Main => self.view.as_ref().map(|view| view.messages.as_slice()),
            Conversation::Subagent(thread) => self
                .agent_popup
                .as_ref()
                .filter(|popup| &popup.thread_id == thread)
                .and_then(|popup| popup.view.as_ref())
                .map(|view| view.messages.as_slice()),
        }
    }

    /// Resolves the viewport used by expansion controls without moving another thread.
    pub(super) fn conversation_scroll(&self, conversation: &Conversation) -> Option<&ListState> {
        match conversation {
            Conversation::Main => Some(&self.chat_scroll),
            Conversation::Subagent(thread) => self
                .agent_popup
                .as_ref()
                .filter(|popup| &popup.thread_id == thread)
                .map(|popup| &popup.scroll),
        }
    }

    /// Reports activity in this message's thread, independent of the main task's state.
    pub(super) fn conversation_message_active(
        &self,
        conversation: &Conversation,
        message: &Message,
    ) -> bool {
        match conversation {
            Conversation::Main => self.view.as_ref().is_some_and(|view| {
                view.task.state == RunState::Running
                    && view
                        .activities
                        .iter()
                        .any(|activity| activity.id == message.id)
            }),
            Conversation::Subagent(thread) => self
                .agent_popup
                .as_ref()
                .filter(|popup| &popup.thread_id == thread)
                .and_then(|popup| popup.view.as_ref())
                .is_some_and(|view| {
                    view.agent.state.active()
                        && !message.complete
                        && view.agent.active_turn.as_deref().is_none_or(|turn| {
                            message
                                .id
                                .strip_prefix(turn)
                                .is_some_and(|suffix| suffix.starts_with(':'))
                        })
                }),
        }
    }

    /// Renders shared message styling and registers selectable blocks in the owning thread.
    pub(super) fn conversation_message(
        &mut self,
        conversation: &Conversation,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.selection_inputs = Some(Vec::new());
        let content = self.message_content(conversation, index, cx);
        let inputs = self.selection_inputs.take().unwrap_or_default();
        if let Some(message) = self
            .conversation_messages(conversation)
            .and_then(|messages| messages.get(index))
        {
            let order = message.seq;
            let selection = match conversation {
                Conversation::Main => Some(&self.chat_selection),
                Conversation::Subagent(thread) => self
                    .agent_popup
                    .as_ref()
                    .filter(|popup| &popup.thread_id == thread)
                    .map(|popup| &popup.selection),
            };
            if let Some(selection) = selection {
                selection.update(cx, |doc, cx| doc.register(order, inputs, cx));
            }
        }
        content
    }
    fn message_content(
        &mut self,
        conversation: &Conversation,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(message) = self
            .conversation_messages(conversation)
            .and_then(|messages| messages.get(index))
            .cloned()
        else {
            return div().into_any_element();
        };
        if message.kind == MessageKind::Subagent {
            return div()
                .px_5()
                .py_2()
                .child(self.agent_link(&message.text, cx))
                .into_any_element();
        }
        let t = self.theme;
        let user = message.kind == MessageKind::User;
        if message.kind == MessageKind::Tool {
            return self.tool_calls(conversation, index, cx);
        }
        // A one-line provider notice reads as a divider in the conversation, not as a message.
        if message.kind == MessageKind::Notice
            && message.attachments.is_empty()
            && !message.text.trim().contains('\n')
            && message.text.trim().chars().count() <= 96
        {
            return div()
                .w_full()
                .px_5()
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .flex_1()
                        .child(dither::DottedRule::horizontal(t.subtle.opacity(0.45))),
                )
                .child(caps(message.text.trim(), t))
                .child(
                    div()
                        .flex_1()
                        .child(dither::DottedRule::horizontal(t.subtle.opacity(0.45))),
                )
                .into_any_element();
        }
        let message_active = self.conversation_message_active(conversation, &message);
        let message_key = format!("{}:{}", message.task_id, message.id);
        let text = message.text.clone();
        let mut outer = div().w_full().px_5().py_2();
        let mut block = div().w_full();
        if user {
            outer = outer.flex().justify_end();
            block = block
                .relative()
                .overflow_hidden()
                .w_auto()
                .min_w_0()
                .max_w(relative(0.82))
                .flex()
                .flex_col()
                .gap_2()
                .bg(t.raised)
                .border_1()
                .border_color(t.edge)
                .rounded(px(crate::theme::MESSAGE_RADIUS))
                .p(px(crate::theme::MESSAGE_PADDING))
                .child(
                    dither::Dither::new(dither::DitherField::Fade {
                        from: dither::Edge::Bottom,
                        start: 0.3,
                        end: -0.25,
                    })
                    .color(t.dot)
                    .grid(2., 1.)
                    .corner_radius(crate::theme::MESSAGE_RADIUS)
                    .layer(),
                );
        } else if message.kind != MessageKind::Assistant {
            let (name, glyph, color) = match message.kind {
                MessageKind::Error => ("Error", "circle-x", t.removed),
                MessageKind::Check => ("Check result", "check-square", t.added),
                MessageKind::Tool => ("Tool output", "square-terminal", t.muted),
                _ => ("Notice", "info", t.info),
            };
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .mb(px(4.))
                    .text_size(px(11.))
                    .child(if message_active {
                        motion::active_icon(glyph, color, false, cx.reduce_motion())
                    } else {
                        icon(glyph)
                            .size(px(13.))
                            .text_color(color)
                            .into_any_element()
                    })
                    .child(caps(name, t).text_color(color))
                    .when(message_active, |d| {
                        d.child(caps("Working", t).text_color(t.accent))
                    })
                    .child(
                        div()
                            .flex_1()
                            .child(dither::DottedRule::horizontal(color.opacity(0.35))),
                    ),
            );
        }
        if !message.attachments.is_empty() {
            let mut files = div()
                .flex()
                .flex_wrap()
                .gap_2()
                .when(!user, |files| files.mb_2());
            for attachment in &message.attachments {
                files = files.child(self.attachment_card(attachment, None, cx));
            }
            block = block.child(files);
        }
        for feedback in &message.feedback {
            block = block.child(self.context_card(feedback, None, cx));
        }
        if !user || !message.text.is_empty() {
            block = block.child(
                div()
                    .relative()
                    .text_size(px(self.settings.font_size - 1.))
                    .child(
                        self.message_blocks(
                            &format!("message:{message_key}"),
                            markdown::blocks(&message.text),
                            message.kind == MessageKind::Assistant
                                && !message.complete
                                && match conversation {
                                    Conversation::Main => self
                                        .view
                                        .as_ref()
                                        .is_some_and(|view| view.task.state == RunState::Running),
                                    Conversation::Subagent(_) => message_active,
                                },
                            cx,
                        ),
                    ),
            );
        }
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
                                SharedString::from(message_key.clone()),
                            ),
                            "copy",
                            "Copy message as Markdown",
                        )
                        .w(px(20.))
                        .h(px(20.))
                        .text_color(t.subtle)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            let tone = s.theme.accent;
                            s.notify_user("Copied", tone, cx);
                        })),
                    )
                    .child(div().text_size(px(10.)).text_color(t.subtle).child(
                        if message_active {
                            motion::gleam(
                                if message.kind == MessageKind::Assistant {
                                    "Writing…"
                                } else {
                                    "In progress…"
                                },
                                t,
                                cx.reduce_motion(),
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
            .child(
                outer.child(
                    if *conversation == Conversation::Main
                        && let Some(arrival) = message
                            .id
                            .strip_prefix("submission-")
                            .and_then(|id| self.send_arrivals.get(id))
                            .filter(|a| !a.queued)
                    {
                        motion::send_transition(
                            block,
                            arrival,
                            cx.reduce_motion() || self.panel != Panel::None,
                        )
                    } else {
                        block.into_any_element()
                    },
                ),
            )
            .into_any_element()
    }
}
