//! Markdown documents in the Git workspace, such as AI explanations and reviews. Prose and code
//! are Kodem's read-only text inputs, so the text can be selected and copied.
use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, Entity, SharedString, Window, div, relative};

use crate::components::text_input::TextInput;
use crate::markdown::{self, Block, RichText};
use crate::theme::{code_px, px, text_px};
use crate::ui::dither::DottedRule;
use crate::ui::git::theme::ActiveTheme;

/// A rendered Markdown document.
#[derive(IntoElement)]
pub struct TextView {
    id: SharedString,
    text: SharedString,
}

impl TextView {
    /// Creates a document from Markdown text. `id` keeps its text selection across redraws.
    pub fn markdown(id: impl Into<SharedString>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
        }
    }

    /// Kept for the views' builders: the document's text is always selectable.
    pub fn selectable(self, _: bool) -> Self {
        self
    }
}

// Returns the read-only input that shows one block, kept by `key` across redraws.
fn readonly(key: String, text: &str, window: &mut Window, cx: &mut App) -> Entity<TextInput> {
    let initial = text.to_owned();
    let input = window.use_keyed_state(ElementId::Name(key.into()), cx, move |_, cx| {
        TextInput::readonly(initial, cx)
    });
    if !input.read(cx).shows(text) {
        let text = text.to_owned();
        input.update(cx, |input, cx| input.set_text(text, cx));
    }
    input
}

// Builds selectable prose from rich text.
fn prose(key: String, text: RichText, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme().kodem;
    let input = readonly(key, &text.text, window, cx);
    input.update(cx, |input, _| input.prose(&text.spans, theme));
    div()
        .w_full()
        .min_w_0()
        .line_height(relative(1.55))
        .child(input)
        .into_any_element()
}

// Builds the blocks of a document, or of a list item or quotation inside it.
fn blocks(key: &str, blocks: Vec<Block>, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme().kodem;
    let mut column = div().w_full().min_w_0().flex().flex_col().gap(px(8.));
    for (index, block) in blocks.into_iter().enumerate() {
        let key = format!("{key}.{index}");
        let element = match block {
            Block::Text(text) => prose(key, text, window, cx),
            Block::Heading { level, text } => div()
                .w_full()
                .pt(px(if level <= 2 { 6. } else { 2. }))
                .text_size(text_px(match level {
                    1 => 16.,
                    2 => 14.5,
                    _ => 13.,
                }))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(prose(key, text, window, cx))
                .into_any_element(),
            Block::Code { language, text } => {
                let input = readonly(key.clone(), &text, window, cx);
                input.update(cx, |input, _| input.code(&language, theme.id));
                div()
                    .id(SharedString::from(format!("{key}:scroll")))
                    .w_full()
                    .overflow_x_scroll()
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(crate::theme::RADIUS_SURFACE))
                    .bg(theme.inset)
                    .border_1()
                    .border_color(theme.border)
                    .font_family(crate::theme::mono())
                    .text_size(code_px(12.))
                    .line_height(relative(1.5))
                    .child(input)
                    .into_any_element()
            }
            Block::Quote { blocks: inner, .. } => div()
                .w_full()
                .pl(px(10.))
                .border_l_2()
                .border_color(theme.edge)
                .text_color(theme.muted)
                .child(self::blocks(&key, inner, window, cx))
                .into_any_element(),
            Block::List { start, items } => {
                let mut list = div().w_full().flex().flex_col().gap(px(4.));
                for (number, item) in items.into_iter().enumerate() {
                    let marker = match (start, item.checked) {
                        (_, Some(true)) => "☑".to_owned(),
                        (_, Some(false)) => "☐".to_owned(),
                        (Some(first), None) => format!("{}.", first + number as u64),
                        (None, None) => "•".to_owned(),
                    };
                    list = list.child(
                        div()
                            .w_full()
                            .flex()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .min_w(px(14.))
                                    .font_family(crate::theme::mono())
                                    .text_color(theme.subtle)
                                    .child(marker),
                            )
                            .child(div().flex_1().min_w_0().child(self::blocks(
                                &format!("{key}.{number}"),
                                item.blocks,
                                window,
                                cx,
                            ))),
                    );
                }
                list.into_any_element()
            }
            Block::Table { header, rows, .. } => {
                let mut table = div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .rounded(px(crate::theme::RADIUS_SURFACE))
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden();
                for (row_index, row) in std::iter::once(header).chain(rows).enumerate() {
                    let mut line = div()
                        .w_full()
                        .flex()
                        .when(row_index == 0, |line| {
                            line.bg(theme.header)
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                        })
                        .when(row_index > 0, |line| {
                            line.border_t_1().border_color(theme.border)
                        });
                    for (cell_index, cell) in row.into_iter().enumerate() {
                        line = line.child(div().flex_1().min_w_0().px(px(8.)).py(px(5.)).child(
                            prose(format!("{key}.{row_index}.{cell_index}"), cell, window, cx),
                        ));
                    }
                    table = table.child(line);
                }
                table.into_any_element()
            }
            Block::Rule => div()
                .w_full()
                .py(px(4.))
                .child(DottedRule::horizontal(theme.subtle.opacity(0.4)))
                .into_any_element(),
        };
        column = column.child(element);
    }
    column.into_any_element()
}

impl RenderOnce for TextView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The document takes the text size and color of the element around it.
        div().w_full().min_w_0().child(blocks(
            &format!("git-text-view:{}", self.id),
            markdown::blocks(&self.text).to_vec(),
            window,
            cx,
        ))
    }
}
