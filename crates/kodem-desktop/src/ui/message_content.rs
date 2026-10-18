//! Shared Markdown blocks for chat, with selectable prose and copyable code.
use super::*;
use crate::markdown::{Block, Callout, RichText};

/// Line height of running prose and of a title, as a share of the text size.
const PROSE_LEADING: f32 = 1.55;
const TITLE_LEADING: f32 = 1.3;

// Returns the space between two blocks that follow one another. Blocks that belong together
// sit close: a title and its section, a sentence and the list that it introduces. A title
// starts a section, so the most space is above it. `nested` is the content of a list item,
// which is set tighter than the message around it.
fn block_gap(above: &Block, below: &Block, nested: bool) -> f32 {
    match (above, below) {
        (Block::Heading { level, .. }, _) => {
            if *level <= 2 {
                10.
            } else {
                6.
            }
        }
        (_, Block::Heading { level, .. }) => {
            if nested {
                12.
            } else if *level <= 2 {
                24.
            } else {
                18.
            }
        }
        (Block::Rule, _) | (_, Block::Rule) => 14.,
        (Block::Text(_), Block::List { .. }) => {
            if nested {
                4.
            } else {
                6.
            }
        }
        (Block::Text(_) | Block::List { .. }, Block::Text(_) | Block::List { .. }) => {
            if nested {
                6.
            } else {
                10.
            }
        }
        // Code, a table or a quotation next to anything else.
        _ => {
            if nested {
                8.
            } else {
                12.
            }
        }
    }
}

impl Shell {
    /// Builds nested Markdown content with stable text-selection identities.
    pub(super) fn message_blocks(
        &mut self,
        key: &str,
        blocks: &[Block],
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.blocks_at(key, blocks, 0, 0, streaming, cx)
    }

    /// Builds the part of a message's blocks that starts at block `first`. Keys and the
    /// space above the part are those of the whole message, so the parts read as one message.
    pub(super) fn message_part(
        &mut self,
        key: &str,
        blocks: &[Block],
        part: std::ops::Range<usize>,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let first = part.start;
        let content = self.blocks_at(key, &blocks[part], first, 0, streaming, cx);
        match first.checked_sub(1).and_then(|above| blocks.get(above)) {
            Some(above) => div()
                .w_full()
                .pt(px(block_gap(above, &blocks[first], false)))
                .child(content)
                .into_any_element(),
            None => content,
        }
    }

    // Builds the blocks of a message, or of a list item at `depth` lists inside the message.
    // `first` is the index of the first block among its siblings, which names its key.
    fn blocks_at(
        &mut self,
        key: &str,
        blocks: &[Block],
        first: usize,
        depth: usize,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let mut content = div().w_full().min_w_0().flex().flex_col();
        for (offset, block) in blocks.iter().enumerate() {
            if offset > 0 {
                content = content.child(
                    div()
                        .h(px(block_gap(&blocks[offset - 1], block, depth > 0)))
                        .flex_shrink_0(),
                );
            }
            let index = first + offset;
            let key = format!("{key}:{index}");
            let element = match block {
                Block::Text(text) => self.message_prose(&key, text, PROSE_LEADING, streaming, cx),
                Block::Heading { level, text } => {
                    let level = *level;
                    let extra = match level {
                        1 => 8.,
                        2 => 5.,
                        3 => 2.,
                        _ => 0.,
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_size(text_px(Settings::FONT_SIZE + extra))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(self.message_prose(
                                    &key,
                                    text,
                                    TITLE_LEADING,
                                    streaming,
                                    cx,
                                )),
                        )
                        .when(level <= 2, |heading| {
                            heading.child(dither::DottedRule::horizontal(t.subtle.opacity(0.45)))
                        })
                        .into_any_element()
                }
                Block::Code { language, text } => {
                    self.message_code(&key, language, text, None, streaming, cx)
                }
                Block::Quote {
                    callout: None,
                    blocks,
                } => div()
                    .relative()
                    .pl(px(14.))
                    .py(px(2.))
                    .text_color(t.muted)
                    .child(
                        // A dot-matrix bar in place of a solid quote edge.
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(px(4.))
                            .child(
                                dither::Dither::new(dither::DitherField::Fade {
                                    from: dither::Edge::Top,
                                    start: 0.75,
                                    end: 0.75,
                                })
                                .color(t.subtle)
                                .grid(2., 1.),
                            ),
                    )
                    .child(self.blocks_at(&key, blocks, 0, depth, streaming, cx))
                    .into_any_element(),
                Block::Quote {
                    callout: Some(callout),
                    blocks,
                } => {
                    let tone = match callout {
                        Callout::Note => t.info,
                        Callout::Tip => t.added,
                        Callout::Important => t.accent,
                        Callout::Warning => t.warning,
                        Callout::Caution => t.removed,
                    };
                    kit::block(t)
                        .border_color(tone.opacity(0.3))
                        .bg(tone.opacity(0.05))
                        .child(
                            kit::block_header(callout.label(), tone, t)
                                .bg(tone.opacity(0.07))
                                .border_color(tone.opacity(0.22)),
                        )
                        .child(
                            div()
                                .px(px(12.))
                                .py(px(9.))
                                .child(self.blocks_at(&key, blocks, 0, depth, streaming, cx)),
                        )
                        .into_any_element()
                }
                Block::Rule => {
                    dither::DottedRule::horizontal(t.subtle.opacity(0.6)).into_any_element()
                }
                Block::List { start, items } => {
                    let start = *start;
                    // A list whose items hold several paragraphs is loose: its items stand
                    // further apart, so that each reads as one unit.
                    let loose = items.iter().any(|item| {
                        item.blocks
                            .iter()
                            .filter(|block| matches!(block, Block::Text(_)))
                            .count()
                            > 1
                    });
                    let mut list = div()
                        .flex()
                        .flex_col()
                        .gap(px(if loose { 10. } else { 5. }));
                    for (index, item) in items.iter().enumerate() {
                        let marker = match item.checked {
                            Some(done) => icon(if done { "check-square" } else { "stop" })
                                .size(px(14.))
                                .mt(px(3.))
                                .text_color(if done { t.added } else { t.subtle })
                                .into_any_element(),
                            None => match start {
                                Some(start) => div()
                                    .pt(px(2.))
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(11.))
                                    .text_color(t.subtle)
                                    .child(format!("{}.", start.saturating_add(index as u64)))
                                    .into_any_element(),
                                // A square pixel bullet, vertically centered on the first line.
                                None if depth == 0 => div()
                                    .mt(gpui::px(
                                        f32::from(text_px(Settings::FONT_SIZE)) * 0.775 - 2.,
                                    ))
                                    .size(px(4.))
                                    .bg(t.subtle)
                                    .into_any_element(),
                                // A list inside a list has an open bullet.
                                None => div()
                                    .mt(gpui::px(
                                        f32::from(text_px(Settings::FONT_SIZE)) * 0.775 - 2.5,
                                    ))
                                    .size(px(5.))
                                    .border_1()
                                    .border_color(t.subtle)
                                    .into_any_element(),
                            },
                        };
                        list = list.child(
                            div()
                                .flex()
                                .items_start()
                                .gap_2()
                                .child(
                                    div()
                                        .min_w(px(20.))
                                        .flex_shrink_0()
                                        .flex()
                                        .justify_end()
                                        .child(marker),
                                )
                                .child(div().flex_1().min_w_0().child(self.blocks_at(
                                    &format!("{key}:item:{index}"),
                                    &item.blocks,
                                    0,
                                    depth + 1,
                                    streaming,
                                    cx,
                                ))),
                        );
                    }
                    list.into_any_element()
                }
                Block::Table {
                    alignments,
                    header,
                    rows,
                } => {
                    let widths: Vec<f32> = (0..alignments.len())
                        .map(|col| {
                            std::iter::once(header)
                                .chain(rows.iter())
                                .filter_map(|row| row.get(col))
                                .map(|cell| {
                                    cell.text
                                        .lines()
                                        .map(|line| line.chars().count())
                                        .max()
                                        .unwrap_or(0)
                                })
                                .max()
                                .unwrap_or(0) as f32
                                * f32::from(text_px(12.))
                                * 0.55
                                + 24.
                        })
                        .map(|width| width.clamp(120., 300.))
                        .collect();
                    let mut table = div()
                        // The widths follow the text size, so they are window pixels.
                        .w(gpui::px(widths.iter().sum()))
                        .min_w_full()
                        .flex()
                        .flex_col();
                    for (row_index, row) in std::iter::once(header).chain(rows).enumerate() {
                        // Rows are separated by dotted rules; only the header has a solid edge.
                        if row_index > 1 {
                            table =
                                table.child(dither::DottedRule::horizontal(t.subtle.opacity(0.45)));
                        }
                        let mut line = div().flex().when(row_index == 0, |d| {
                            d.bg(t.header)
                                .font_family(crate::theme::mono())
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.muted)
                                .border_b_1()
                                .border_color(t.border)
                        });
                        for (col, cell) in row.iter().enumerate() {
                            line = line.child(
                                div()
                                    .w(gpui::px(widths.get(col).copied().unwrap_or(120.)))
                                    .flex_shrink_0()
                                    .flex_grow(1.)
                                    .px_3()
                                    .py(px(if row_index == 0 { 5. } else { 7. }))
                                    .when(row_index == 0, |d| d.text_size(text_px(11.)))
                                    .child(self.message_prose(
                                        &format!("{key}:{row_index}:{col}"),
                                        cell,
                                        PROSE_LEADING,
                                        streaming,
                                        cx,
                                    )),
                            );
                        }
                        table = table.child(line);
                    }
                    // A prose table sits on the page and is not recessed into it.
                    kit::block(t)
                        .bg(gpui::transparent_black())
                        .shadow(Vec::new())
                        .child(
                            div()
                                .id(SharedString::from(format!("table:{key}")))
                                .w_full()
                                .overflow_x_scroll()
                                .child(table),
                        )
                        .into_any_element()
                }
            };
            content = content.child(element);
        }
        content.into_any_element()
    }

    // Builds selectable prose. `leading` is its line height as a share of its text size.
    fn message_prose(
        &mut self,
        key: &str,
        text: &RichText,
        leading: f32,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = self.readonly_streaming(key.to_owned(), &text.text, streaming, cx);
        input.update(cx, |input, _| input.prose(&text.spans, self.theme));
        div()
            .w_full()
            .min_w_0()
            .line_height(relative(leading))
            .child(input)
            .into_any_element()
    }

    /// Builds a literal, syntax-colored block with a copy action and optional output height limit.
    pub(super) fn message_code(
        &mut self,
        key: &str,
        language: &str,
        text: &str,
        max_height: Option<f32>,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let input = self.readonly_streaming(format!("code:{key}"), text, streaming, cx);
        input.update(cx, |input, _| input.code(language, self.theme.id));
        let width = text
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f32
            * f32::from(code_px(Settings::CODE_FONT_SIZE))
            * 0.64
            + 24.;
        let copied = input.clone();
        let lines = text.lines().count().max(1);
        kit::block(t)
            .child(
                kit::block_header(
                    if language.is_empty() {
                        "Text"
                    } else {
                        language
                    },
                    t.muted,
                    t,
                )
                .child(figure(
                    format!("{lines} {}", if lines == 1 { "line" } else { "lines" }),
                    t.subtle,
                ))
                .child(
                    self.icon_button(
                        SharedString::from(format!("copy-code:{key}")),
                        "copy",
                        "Copy block",
                    )
                    .w(px(22.))
                    .h(px(22.))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        let text = copied.read(cx).text();
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        let tone = s.theme.accent;
                        s.notify_user("Copied", tone, cx);
                    })),
                ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("code-scroll:{key}")))
                    .w_full()
                    .overflow_x_scroll()
                    .when_some(max_height, |d, height| {
                        d.max_h(px(height)).overflow_y_scroll()
                    })
                    .px(px(12.))
                    .py(px(10.))
                    .font_family(crate::theme::mono())
                    .text_size(code_px(Settings::CODE_FONT_SIZE))
                    .line_height(relative(1.5))
                    .child(div().min_w_full().w(gpui::px(width)).child(input)),
            )
            .into_any_element()
    }
}
