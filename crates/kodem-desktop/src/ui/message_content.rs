//! Shared Markdown blocks for chat, with selectable prose and copyable code.
use super::*;
use crate::markdown::{Block, RichText};

impl Shell {
    /// Builds nested Markdown content with stable text-selection identities.
    pub(super) fn message_blocks(
        &mut self,
        key: &str,
        blocks: Vec<Block>,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let mut content = div().w_full().min_w_0().flex().flex_col().gap(px(8.));
        for (index, block) in blocks.into_iter().enumerate() {
            let key = format!("{key}:{index}");
            let element = match block {
                Block::Text(text) => self.message_prose(&key, text, streaming, cx),
                Block::Heading { level, text } => {
                    let extra = match level {
                        1 => 8.,
                        2 => 5.,
                        3 => 2.,
                        _ => 0.,
                    };
                    div()
                        .mt(px(if level <= 2 { 10. } else { 6. }))
                        .text_size(px(self.settings.font_size + extra))
                        .line_height(relative(1.3))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.message_prose(&key, text, streaming, cx))
                        .into_any_element()
                }
                Block::Code { language, text } => {
                    self.message_code(&key, &language, &text, None, streaming, cx)
                }
                Block::Quote(blocks) => div()
                    .border_l_2()
                    .border_color(t.edge)
                    .pl(px(12.))
                    .py(px(2.))
                    .text_color(t.muted)
                    .child(self.message_blocks(&key, blocks, streaming, cx))
                    .into_any_element(),
                Block::Rule => div()
                    .my(px(8.))
                    .child(dither::rule(t.edge))
                    .into_any_element(),
                Block::List { start, items } => {
                    let mut list = div().flex().flex_col().gap(px(6.));
                    for (index, item) in items.into_iter().enumerate() {
                        let marker = match item.checked {
                            Some(done) => icon(if done { "check-square" } else { "stop" })
                                .size(px(14.))
                                .mt(px(3.))
                                .text_color(if done { t.added } else { t.subtle })
                                .into_any_element(),
                            None => match start {
                                Some(start) => div()
                                    .pt(px(2.))
                                    .font_family(crate::theme::MONO)
                                    .text_size(px(self.settings.font_size - 3.))
                                    .text_color(t.subtle)
                                    .child(format!("{}.", start.saturating_add(index as u64)))
                                    .into_any_element(),
                                // A square pixel bullet, vertically centered on the first line.
                                None => div()
                                    .mt(px(self.settings.font_size * 0.775 - 2.))
                                    .size(px(4.))
                                    .bg(t.subtle)
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
                                .child(div().flex_1().min_w_0().child(self.message_blocks(
                                    &format!("{key}:item:{index}"),
                                    item.blocks,
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
                            std::iter::once(&header)
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
                                * (self.settings.font_size - 2.)
                                * 0.55
                                + 24.
                        })
                        .map(|width| width.clamp(120., 300.))
                        .collect();
                    let mut table = div()
                        .w(px(widths.iter().sum()))
                        .min_w_full()
                        .flex()
                        .flex_col();
                    for (row_index, row) in std::iter::once(header).chain(rows).enumerate() {
                        let mut line = div()
                            .flex()
                            .when(row_index == 0, |d| {
                                d.bg(t.inset)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .border_b_1()
                                    .border_color(t.edge)
                            })
                            .when(row_index > 1, |d| d.border_t_1().border_color(t.border));
                        for (col, cell) in row.into_iter().enumerate() {
                            line = line.child(
                                div()
                                    .w(px(widths.get(col).copied().unwrap_or(120.)))
                                    .flex_shrink_0()
                                    .flex_grow(1.)
                                    .px_3()
                                    .py_2()
                                    .when(col > 0, |d| d.border_l_1().border_color(t.border))
                                    .child(self.message_prose(
                                        &format!("{key}:{row_index}:{col}"),
                                        cell,
                                        streaming,
                                        cx,
                                    )),
                            );
                        }
                        table = table.child(line);
                    }
                    div()
                        .id(SharedString::from(format!("table:{key}")))
                        .w_full()
                        .overflow_x_scroll()
                        .border_1()
                        .border_color(t.border)
                        .rounded(px(crate::theme::RADIUS_CONTROL))
                        .child(table)
                        .into_any_element()
                }
            };
            content = content.child(element);
        }
        content.into_any_element()
    }

    fn message_prose(
        &mut self,
        key: &str,
        text: RichText,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = self.readonly_streaming(key.to_owned(), text.text, streaming, cx);
        input.update(cx, |input, _| input.prose(text.spans, self.theme));
        div()
            .w_full()
            .min_w_0()
            .line_height(relative(1.55))
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
        let input = self.readonly_streaming(format!("code:{key}"), text.to_owned(), streaming, cx);
        input.update(cx, |input, _| input.code(language, self.settings.light));
        let width = text
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f32
            * (self.settings.font_size - 2.)
            * 0.64
            + 24.;
        let copied = text.to_owned();
        div()
            .w_full()
            .min_w_0()
            .rounded(px(crate::theme::RADIUS_SURFACE))
            .border_1()
            .border_color(t.border)
            .bg(t.inset)
            .overflow_hidden()
            .child(
                div()
                    .pl(px(12.))
                    .pr(px(4.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(t.border)
                    .child(caps(
                        if language.is_empty() {
                            "Text"
                        } else {
                            language
                        },
                        t,
                    ))
                    .child(div().flex_1())
                    .child(
                        self.icon_button(
                            SharedString::from(format!("copy-code:{key}")),
                            "copy",
                            "Copy block",
                        )
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))
                        }),
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
                    .font_family(crate::theme::MONO)
                    .text_size(px(self.settings.font_size - 2.))
                    .line_height(relative(1.5))
                    .child(div().min_w_full().w(px(width)).child(input)),
            )
            .into_any_element()
    }
}
