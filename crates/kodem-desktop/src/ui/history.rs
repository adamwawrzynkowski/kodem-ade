//! The message history rail: one tick for each message the user sent, on the trailing edge of
//! the conversation. A tick previews its turn while the pointer is over it, and a click
//! scrolls the conversation to that message.
use super::*;
use crate::markdown::{self, Block};

/// Height of one tick's row, which is also the distance between two ticks.
const PITCH: f32 = 9.;
/// Thickness of a tick.
const TICK: f32 = 2.;
/// Width of the column that reacts to the pointer.
const REACH: f32 = 28.;
/// Distance between the rail and the trailing edge of the chat.
const INSET: f32 = 4.;
/// Width of a tick at rest, of the tick for the turn in view, and of the tick under the pointer.
const REST: f32 = 8.;
const CURRENT: f32 = 14.;
const PEAK: f32 = 22.;
/// How much of the pointer's widening reaches the ticks beside it, by distance.
const FALLOFF: [f32; 3] = [1., 0.6, 0.28];
/// Space the rail and the preview keep from the top and the bottom of the conversation.
const MARGIN: f32 = 20.;
const PREVIEW_MARGIN: f32 = 8.;
/// Width of the preview card and its distance from the rail.
const PREVIEW_WIDTH: f32 = 320.;
const PREVIEW_GAP: f32 = 8.;
/// Longest text that one preview section receives before its lines are clamped.
const PREVIEW_CHARS: usize = 220;
/// Bytes of a reply that are parsed for its preview.
const REPLY_BYTES: usize = 1600;

impl Shell {
    /// Builds the history rail of the open conversation and the preview of the tick under the
    /// pointer. The layer covers the conversation list and passes the pointer through outside
    /// its ticks. A conversation with fewer than two sent messages has no rail.
    pub(super) fn message_history(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let t = self.theme;
        let listed =
            !(self.reviewing_file && self.settings.diff_placement == DiffPlacement::Conversation);
        let turns: Vec<usize> = self
            .view
            .as_ref()
            .filter(|_| listed)
            .map(|view| {
                view.messages
                    .iter()
                    .enumerate()
                    .filter(|(_, message)| message.kind == MessageKind::User)
                    .map(|(index, _)| index)
                    .collect()
            })
            .unwrap_or_default();
        if turns.len() < 2 {
            self.history_hover = None;
            return None;
        }
        let view = self.view.as_ref()?;
        // The turn in view is the last sent message at or above the top of the list.
        // The list counts rows, and a long reply takes several.
        let top = self
            .chat_rows
            .locate(self.chat_scroll.logical_scroll_top().item_ix)
            .map_or(view.messages.len(), |(index, _)| index);
        let current = turns.iter().rposition(|index| *index <= top).unwrap_or(0);
        // The list's height comes from the frame before. A long conversation shows the ticks
        // around the turn in view.
        // The part of the list behind the composer's box holds no ticks.
        let height = (f32::from(self.chat_scroll.viewport_bounds().size.height)
            - self.chat_cover())
            / crate::theme::ui_scale();
        let capacity = if height > 0. {
            (((height - 2. * MARGIN) / PITCH).floor() as usize).max(1)
        } else {
            turns.len()
        };
        let first = if turns.len() > capacity {
            current
                .saturating_sub(capacity / 2)
                .min(turns.len() - capacity)
        } else {
            0
        };
        let shown = &turns[first..(first + capacity).min(turns.len())];
        let hovered = self.history_hover.and_then(|seq| {
            shown
                .iter()
                .position(|index| view.messages[*index].seq == seq)
        });
        // A tick that left the rail while the pointer was over it reports no change.
        if hovered.is_none() {
            self.history_hover = None;
        }
        let mut rail = div()
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(INSET))
            .w(px(REACH))
            .flex()
            .flex_col()
            .justify_center();
        for (slot, index) in shown.iter().enumerate() {
            let seq = view.messages[*index].seq;
            let swell = hovered
                .and_then(|hovered| FALLOFF.get(hovered.abs_diff(slot)))
                .copied();
            let in_view = first + slot == current;
            let rest = if in_view { CURRENT } else { REST };
            let width = swell.map_or(rest, |swell| (REST + (PEAK - REST) * swell).max(rest));
            let color = if hovered == Some(slot) {
                t.accent
            } else if in_view {
                t.text
            } else if swell.is_some() {
                t.muted
            } else {
                t.subtle
            };
            rail = rail.child(
                div()
                    .id(("history-tick", seq))
                    .h(px(PITCH))
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .cursor_pointer()
                    // A press on a tick does not reach the message beneath it; the wheel does.
                    .block_mouse_except_scroll()
                    .on_hover(cx.listener(move |s, hovered: &bool, _, cx| {
                        if *hovered {
                            s.history_hover = Some(seq);
                            cx.notify();
                        } else if s.history_hover == Some(seq) {
                            s.history_hover = None;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |s, _, _, cx| s.open_turn(seq, cx)))
                    .child(div().h(px(TICK)).w(px(width)).bg(color)),
            );
        }
        let mut layer = div().absolute().inset_0().child(rail);
        if let Some(slot) = hovered {
            // The preview takes the same share of the free height above it as its tick has in
            // the conversation, so it stays beside the tick and inside the conversation.
            let share = if height > 2. * PREVIEW_MARGIN {
                let tick = (height - shown.len() as f32 * PITCH) / 2. + (slot as f32 + 0.5) * PITCH;
                ((tick - PREVIEW_MARGIN) / (height - 2. * PREVIEW_MARGIN)).clamp(0., 1.)
            } else {
                0.5
            };
            let place = view.before.is_none().then_some((slot + 1, turns.len()));
            layer = layer.child(
                div()
                    .absolute()
                    .top(px(PREVIEW_MARGIN))
                    .bottom(px(PREVIEW_MARGIN))
                    .left(px(PREVIEW_MARGIN))
                    .right(px(INSET + REACH + PREVIEW_GAP))
                    .flex()
                    .flex_col()
                    .items_end()
                    .child(div().flex_basis(px(0.)).flex_grow(share * 100.))
                    .child(self.turn_preview(view, shown[slot], place, window, cx))
                    .child(div().flex_basis(px(0.)).flex_grow((1. - share) * 100.)),
            );
        }
        Some(layer.into_any_element())
    }

    // Scrolls the conversation so that the sent message with this sequence number is at the top
    // of the list.
    fn open_turn(&mut self, seq: u64, cx: &mut Context<Self>) {
        let Some(index) = self
            .view
            .as_ref()
            .and_then(|view| view.messages.iter().position(|message| message.seq == seq))
        else {
            return;
        };
        // An older turn stays in view while the agent writes: the jump stops the list from
        // following its end, and the list resumes when the target lies at the end.
        self.chat_scroll.scroll_to(ListOffset {
            item_ix: self.chat_rows.rows(index..index + 1).start,
            offset_in_item: px(0.),
        });
        cx.notify();
    }

    // The preview of one turn: the sent message, then the opening of the turn's last reply and
    // the paragraph after it as a quotation. `place` is the turn's number and the total when
    // the whole conversation is loaded.
    fn turn_preview(
        &self,
        view: &TaskView,
        index: usize,
        place: Option<(usize, usize)>,
        window: &Window,
        cx: &App,
    ) -> Div {
        let t = self.theme;
        let message = &view.messages[index];
        let turn = &view.messages[index + 1..];
        let turn = &turn[..turn
            .iter()
            .position(|message| message.kind == MessageKind::User)
            .unwrap_or(turn.len())];
        let last = index + 1 + turn.len() == view.messages.len();
        let reply = turn
            .iter()
            .rev()
            .find(|message| {
                message.kind == MessageKind::Assistant && !message.text.trim().is_empty()
            })
            .map(|message| reply_openings(&message.text))
            .unwrap_or_default();
        let sent = excerpt(&message.text);
        let files = message.attachments.len();
        let label = match place {
            Some((number, total)) => format!("Message {number} of {total}"),
            None => "Message".into(),
        };
        let line = |color: Hsla, size: f32| {
            div()
                .min_w_0()
                .text_size(text_px(size))
                .line_height(relative(1.4))
                .text_color(color)
                .line_clamp(2)
                .text_ellipsis()
        };
        let mut body = div()
            .relative()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(7.))
            .layer(dither::edge_shade(t.dot, 0.22))
            .child(if sent.is_empty() {
                line(t.muted, 13.).child(if files == 1 {
                    "1 attachment".to_string()
                } else {
                    format!("{files} attachments")
                })
            } else {
                line(t.text, 13.)
                    .font_weight(FontWeight::MEDIUM)
                    .child(sent)
            });
        let mut openings = reply.into_iter();
        match openings.next() {
            Some(opening) => body = body.child(line(t.muted, 12.5).child(opening)),
            None if last && view.task.state == RunState::Running => {
                body = body.child(div().text_size(text_px(12.5)).child(motion::gleam(
                    "Working…",
                    t,
                    cx.reduce_motion(),
                )));
            }
            None => {}
        }
        if let Some(next) = openings.next() {
            body = body.child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(
                        div()
                            .w(px(2.))
                            .flex_shrink_0()
                            .bg(dither::checker(t.subtle, 1., window)),
                    )
                    .child(line(t.subtle, 12.5).flex_1().child(next)),
            );
        }
        div()
            .relative()
            .w(px(PREVIEW_WIDTH))
            .max_w_full()
            .flex_shrink_0()
            .child(dither::shadow(t, crate::theme::RADIUS_SURFACE, 4.))
            .child(
                div()
                    .relative()
                    .overflow_hidden()
                    .font_family(crate::theme::sans())
                    .bg(t.raised)
                    .border_1()
                    .border_color(t.edge)
                    .rounded(px(crate::theme::RADIUS_SURFACE))
                    .child(
                        kit::block_header(label, t.accent, t)
                            .pr(px(10.))
                            .child(figure(sidebar::elapsed(message.created), t.subtle)),
                    )
                    .child(body),
            )
    }
}

// Returns the opening of a text on one line, cut to the length of a preview section.
fn excerpt(text: &str) -> String {
    let mut line = String::new();
    let mut length = 0;
    for word in text.split_whitespace() {
        if length > 0 {
            line.push(' ');
            length += 1;
        }
        for character in word.chars() {
            if length >= PREVIEW_CHARS {
                line.push('…');
                return line;
            }
            line.push(character);
            length += 1;
        }
    }
    line
}

// Returns the plain openings of a reply's first two blocks. Only the start of the reply is
// parsed, because a preview shows no more than that.
fn reply_openings(text: &str) -> Vec<String> {
    let mut end = text.len().min(REPLY_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    markdown::blocks(&text[..end])
        .iter()
        .filter_map(opening)
        .take(2)
        .collect()
}

// Returns the text that a block starts with. A divider and an empty block have none.
fn opening(block: &Block) -> Option<String> {
    let text = match block {
        Block::Text(text) | Block::Heading { text, .. } => excerpt(&text.text),
        Block::Code { text, .. } => excerpt(text.lines().find(|line| !line.trim().is_empty())?),
        Block::Quote { blocks, .. } => blocks.iter().find_map(opening)?,
        Block::List { items, .. } => excerpt(
            &items
                .iter()
                .filter_map(|item| item.blocks.iter().find_map(opening))
                .take(3)
                .collect::<Vec<_>>()
                .join(" · "),
        ),
        Block::Table { header, .. } => excerpt(
            &header
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>()
                .join(" · "),
        ),
        Block::Rule => return None,
    };
    (!text.is_empty()).then_some(text)
}
