//! Shared message presentation for the main conversation and child thread popups.
use super::*;
use crate::components::text_input::is_long_text;
use crate::markdown::{self, Block};
use std::{ops::Range, rc::Rc};

/// How long after its creation a message that came whole still counts as arriving, in
/// milliseconds.
const WHOLE_ARRIVAL_MS: u64 = 1500;

/// Space above a message of the user, which starts a turn.
const TURN_GAP: f32 = 18.;

/// Name of the hover group of a reply, which shows the reply's actions under the pointer.
const REPLY_GROUP: &str = "reply";

/// Size in bytes from which a message of the agent can take more than one row of the main
/// conversation. The list lays out only the rows in view, so a long reply costs its part in
/// view instead of all of it.
const SPLIT_BYTES: usize = 3000;
/// Text in bytes that one row of a split message holds before the next block starts a row.
const PART_BYTES: usize = 1200;
/// Weight of a block beside its text, for its spacing and structure.
const BLOCK_WEIGHT: usize = 48;

/// The ranges of a long message's blocks that show as one row each.
type Parts = Rc<[Range<usize>]>;

/// Rows of the main conversation list. A long message of the agent takes one row for each part
/// of its blocks; every other message takes one row.
#[derive(Default)]
pub(super) struct ChatRows {
    /// The first row of each message, then the number of rows.
    starts: Vec<usize>,
    /// The parts of each message that takes several rows.
    parts: Vec<Option<Parts>>,
}

impl ChatRows {
    /// Lays out the rows of a transcript. `previous` is the transcript that the rows were laid
    /// out for before, with those rows: a long message whose text did not change keeps its
    /// parts instead of being read again.
    pub(super) fn new(messages: &[Message], previous: Option<(&[Message], &ChatRows)>) -> Self {
        let kept: HashMap<&str, (&Message, &Parts)> = previous
            .map(|(old_messages, old_rows)| {
                old_messages
                    .iter()
                    .zip(&old_rows.parts)
                    .filter_map(|(message, parts)| {
                        Some((message.id.as_str(), (message, parts.as_ref()?)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut starts = Vec::with_capacity(messages.len() + 1);
        let mut parts = Vec::with_capacity(messages.len());
        let mut row = 0;
        for message in messages {
            starts.push(row);
            let split = splits(message).then(|| {
                kept.get(message.id.as_str())
                    .filter(|(old, _)| old.kind == message.kind && old.text == message.text)
                    .map(|(_, parts)| Rc::clone(parts))
                    .unwrap_or_else(|| split_parts(&markdown::blocks(&message.text)).into())
            });
            row += split.as_ref().map_or(1, |split| split.len());
            parts.push(split);
        }
        starts.push(row);
        Self { starts, parts }
    }

    /// Returns the parts of a message that takes several rows, or nothing for a message that
    /// takes one row.
    pub(super) fn parts(&self, message: usize) -> Option<&[Range<usize>]> {
        self.parts.get(message)?.as_deref()
    }

    /// Returns the number of rows.
    pub(super) fn count(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    /// Returns the rows of the messages in `messages`.
    pub(super) fn rows(&self, messages: Range<usize>) -> Range<usize> {
        let row = |message: usize| {
            self.starts
                .get(message)
                .copied()
                .unwrap_or_else(|| self.count())
        };
        row(messages.start)..row(messages.end.max(messages.start))
    }

    /// Returns the message of a row and the part of the message that the row shows. A row
    /// after the last one belongs to no message.
    pub(super) fn locate(&self, row: usize) -> Option<(usize, usize)> {
        if row >= self.count() {
            return None;
        }
        let message = self.starts.partition_point(|start| *start <= row) - 1;
        Some((message, row - self.starts[message]))
    }
}

// Reports whether a message is shown as Markdown blocks that can take several rows: a reply,
// notice, error or check result that is not a divider.
fn splits(message: &Message) -> bool {
    !matches!(
        message.kind,
        MessageKind::User | MessageKind::Tool | MessageKind::Subagent
    ) && message.transfer.is_none()
        && message.text.len() >= SPLIT_BYTES
}

// Groups blocks into parts of about `PART_BYTES` each. A title stays with the block after it.
fn split_parts(blocks: &[Block]) -> Vec<Range<usize>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut weight = 0;
    for (index, block) in blocks.iter().enumerate() {
        weight += block_bytes(block) + BLOCK_WEIGHT;
        let next = index + 1;
        if weight >= PART_BYTES && next < blocks.len() && !matches!(block, Block::Heading { .. }) {
            parts.push(start..next);
            start = next;
            weight = 0;
        }
    }
    parts.push(start..blocks.len());
    parts
}

// Returns the size of the text that a block shows, in bytes.
fn block_bytes(block: &Block) -> usize {
    match block {
        Block::Text(text) | Block::Heading { text, .. } => text.text.len(),
        Block::Code { text, .. } => text.len(),
        Block::Quote { blocks, .. } => blocks.iter().map(block_bytes).sum(),
        Block::List { items, .. } => items
            .iter()
            .flat_map(|item| &item.blocks)
            .map(block_bytes)
            .sum(),
        Block::Table { header, rows, .. } => std::iter::once(header)
            .chain(rows)
            .flatten()
            .map(|cell| cell.text.len())
            .sum(),
        Block::Rule => 0,
    }
}

// Reports whether a reply is the last word of its turn: no work and no reply of the agent
// follows it before the next message of the user.
fn closes_turn(transcript: &[Message], index: usize) -> bool {
    transcript
        .iter()
        .skip(index + 1)
        .take_while(|message| message.kind != MessageKind::User)
        .all(|message| {
            !matches!(
                message.kind,
                MessageKind::Assistant | MessageKind::Tool | MessageKind::Subagent
            )
        })
}

#[derive(Clone, PartialEq, Eq, Hash)]
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

    /// Reports that this conversation's thread works on a turn, which can still add messages.
    pub(super) fn conversation_working(&self, conversation: &Conversation) -> bool {
        match conversation {
            Conversation::Main => self
                .view
                .as_ref()
                .is_some_and(|view| view.task.state.active()),
            Conversation::Subagent(thread) => self
                .agent_popup
                .as_ref()
                .filter(|popup| &popup.thread_id == thread)
                .and_then(|popup| popup.view.as_ref())
                .is_some_and(|view| view.agent.state.active()),
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

    /// Reports whether the text of an agent's message arrives now, so that its letters form out
    /// of dither. A message that came whole counts for a moment after its creation.
    pub(super) fn message_arriving(&self, conversation: &Conversation, message: &Message) -> bool {
        if message.kind == MessageKind::User {
            return false;
        }
        if message.complete {
            return now().saturating_sub(message.created) < WHOLE_ARRIVAL_MS;
        }
        match conversation {
            Conversation::Main => self
                .view
                .as_ref()
                .is_some_and(|view| view.task.state == RunState::Running),
            Conversation::Subagent(_) => self.conversation_message_active(conversation, message),
        }
    }

    /// Renders the rows of the conversation's list: one message, or one part of a long message
    /// of the main conversation.
    pub(super) fn conversation_row(
        &mut self,
        conversation: &Conversation,
        row: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let located = match conversation {
            Conversation::Main => self.chat_rows.locate(row),
            Conversation::Subagent(_) => Some((row, 0)),
        };
        match located {
            Some((index, part)) => self.conversation_message(conversation, index, part, cx),
            None => div().into_any_element(),
        }
    }

    /// Measures the rows of the messages in `messages` again, after their height changed.
    pub(super) fn remeasure_messages(&self, conversation: &Conversation, messages: Range<usize>) {
        let rows = match conversation {
            Conversation::Main => self.chat_rows.rows(messages),
            Conversation::Subagent(_) => messages,
        };
        if let Some(scroll) = self.conversation_scroll(conversation) {
            scroll.remeasure_items(rows);
        }
    }

    /// Renders shared message styling and registers selectable blocks in the owning thread.
    /// `part` is the part of a long message that the row shows.
    fn conversation_message(
        &mut self,
        conversation: &Conversation,
        index: usize,
        part: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.selection_inputs = Some(Vec::new());
        let content = self.message_content(conversation, index, part, cx);
        let inputs = self.selection_inputs.take().unwrap_or_default();
        if let Some(message) = self
            .conversation_messages(conversation)
            .and_then(|messages| messages.get(index))
        {
            let order = (message.seq, part);
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
        part: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = self.message_body(conversation, index, part, cx);
        if self.message_undone(conversation, index) {
            // A turn that the user undid stays readable, but it no longer counts.
            return div()
                .w_full()
                .opacity(0.45)
                .child(content)
                .into_any_element();
        }
        content
    }

    // Names one agent of a transfer: the logo of its provider and its model.
    fn transfer_agent(&self, agent: &core::AgentRef, color: Hsla) -> Div {
        let name = self.model_name(&AgentPreferences {
            provider: agent.provider,
            model: agent.model.clone(),
            ..AgentPreferences::default()
        });
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(5.))
            .text_size(text_px(12.))
            .text_color(color)
            .child(icon(agent.provider.icon()).size(px(12.)))
            .child(name)
    }

    // A transfer of the conversation from one agent to another reads as a divider that names
    // both: where the context came from, and who continues with it.
    fn transfer_row(&self, transfer: &core::Transfer) -> AnyElement {
        let t = self.theme;
        div()
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
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.))
                    .child(icon("refresh").size(px(12.)).text_color(t.subtle))
                    .child(caps(transfer.kind.label(), t))
                    .child(self.transfer_agent(&transfer.from, t.accent))
                    .child(icon("arrow-right").size(px(12.)).text_color(t.subtle))
                    .child(self.transfer_agent(&transfer.to, t.text)),
            )
            .child(
                div()
                    .flex_1()
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.45))),
            )
            .into_any_element()
    }

    // Reports a row of a turn that the user undid. Rows written after the undo are not part
    // of that turn, also before the next turn begins.
    fn message_undone(&self, conversation: &Conversation, index: usize) -> bool {
        if *conversation != Conversation::Main {
            return false;
        }
        self.view.as_ref().is_some_and(|view| {
            view.messages.get(index).is_some_and(|message| {
                core::run_at(&view.runs, message.seq)
                    .and_then(|run| run.undone)
                    .is_some_and(|undone| message.created < undone)
            })
        })
    }

    fn message_body(
        &mut self,
        conversation: &Conversation,
        index: usize,
        part: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((message, closes_turn)) =
            self.conversation_messages(conversation)
                .and_then(|messages| {
                    messages
                        .get(index)
                        .map(|message| (message.clone(), closes_turn(messages, index)))
                })
        else {
            return div().into_any_element();
        };
        if message.kind == MessageKind::Subagent {
            return div()
                .px_5()
                .py_2()
                .child(self.agent_link(&message.text, false, cx))
                .into_any_element();
        }
        let t = self.theme;
        let user = message.kind == MessageKind::User;
        if message.kind == MessageKind::Tool {
            return self.tool_calls(conversation, index, cx);
        }
        if let Some(transfer) = &message.transfer {
            return self.transfer_row(transfer);
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
        // A long message of the agent shows in parts, one row of the list each. The first part
        // has the message's heading and context, the last its actions.
        let blocks = markdown::blocks(&message.text);
        let parts = match conversation {
            Conversation::Main => self.chat_rows.parts(index).map(<[_]>::to_vec),
            Conversation::Subagent(_) => None,
        };
        let shown = match &parts {
            Some(parts) => match parts.get(part) {
                Some(range) => range.clone(),
                None => return div().into_any_element(),
            },
            None => 0..blocks.len(),
        };
        let first = shown.start == 0;
        let last = shown.end == blocks.len();
        let message_active = self.conversation_message_active(conversation, &message);
        let message_key = format!("{}:{}", message.task_id, message.id);
        let mut outer = div()
            .w_full()
            .px_5()
            .when(first, |outer| outer.pt_2())
            .when(last, |outer| outer.pb_2());
        let mut block = div().w_full();
        if user {
            outer = outer.pt(px(TURN_GAP)).flex().justify_end();
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
                .layer(dither::edge_shade(t.dot, 0.3));
        } else if message.kind != MessageKind::Assistant && first {
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
                    .text_size(text_px(11.))
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
        if first && !message.attachments.is_empty() {
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
        if first {
            for feedback in &message.feedback {
                block = block.child(self.context_card(feedback, None, cx));
            }
        }
        if user && is_long_text(&message.text) {
            // A long message of the user shows as a file that unfolds, like the text that was
            // pasted into the composer for it.
            block = block.child(self.long_text_card(
                conversation,
                index,
                &message_key,
                &message.text,
                cx,
            ));
        } else if !user || !message.text.is_empty() {
            let arriving = self.message_arriving(conversation, &message);
            let key = format!("message:{message_key}");
            block = block.child(div().relative().text_size(text_px(13.)).child(
                if parts.is_some() {
                    self.message_part(&key, &blocks, shown, arriving, cx)
                } else {
                    self.message_blocks(&key, &blocks, arriving, cx)
                },
            ));
        }
        if !user {
            let copy_conversation = conversation.clone();
            let copy_id = message.id.clone();
            let copy = self
                .icon_button(
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
                    // The message is read when it is copied, so a long reply is not copied on
                    // every frame.
                    let Some(text) = s
                        .conversation_messages(&copy_conversation)
                        .and_then(|messages| messages.iter().find(|m| m.id == copy_id))
                        .map(|message| message.text.clone())
                    else {
                        return;
                    };
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    let tone = s.theme.accent;
                    s.notify_user("Copied", tone, cx);
                }));
            // The reply that ends a turn, and every notice, error and check result, keeps its
            // actions in a row below it. A reply that work follows is a step of that work: its
            // action shows under the pointer, in the padding beside the reply, and takes no
            // room.
            if message_active || closes_turn || message.kind != MessageKind::Assistant {
                if last {
                    block =
                        block.child(div().flex().items_center().gap_2().mt_1().child(copy).when(
                            message_active,
                            |row| {
                                row.child(div().text_size(text_px(10.)).text_color(t.subtle).child(
                                    motion::gleam(
                                        if message.kind == MessageKind::Assistant {
                                            "Writing…"
                                        } else {
                                            "In progress…"
                                        },
                                        t,
                                        cx.reduce_motion(),
                                    ),
                                ))
                            },
                        ));
                }
            } else if first {
                outer = outer.relative().group(REPLY_GROUP).child(
                    div()
                        .absolute()
                        .top(px(8.))
                        .right_0()
                        .invisible()
                        .group_hover(REPLY_GROUP, |action| action.visible())
                        .child(copy),
                );
            }
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
