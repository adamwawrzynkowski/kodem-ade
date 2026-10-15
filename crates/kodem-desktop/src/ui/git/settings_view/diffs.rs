//! The Diffs page: how the diff view lays out changes and how the commit editor checks titles.
//! The size of diff text follows Kodem's code size in Appearance.
use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{App, Context, Div, Hsla, div};

use super::{GitSettingsView, page, section, segmented, setting_row, slider_row, toggle_row};
use crate::theme::{code_px, mono, px};
use crate::ui::controls::Slider;
use crate::ui::dither::{Dither, DitherField, Edge};
use crate::ui::git::kit::{ActiveTheme as _, h_flex, v_flex};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::theme::liquid_colors;
use crate::ui::settings::note;

/// Size of diff text at Kodem's default code size, in points.
const DIFF_TEXT_SIZE: f32 = 12.;
/// Space above and below the text of one diff line, in points.
const DIFF_LINE_PADDING: f32 = 9.;

impl GitSettingsView {
    /// Returns the Diffs page.
    pub(super) fn render_diffs(&mut self, cx: &mut Context<Self>) -> Div {
        let s = GitSettings::get(cx).clone();
        let colors = liquid_colors(cx);

        page()
            .child(
                section("Preview", cx)
                    .child(
                        v_flex()
                            .mt(px(10.))
                            .w_full()
                            .rounded(px(8.))
                            .overflow_hidden()
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().colors.list)
                            .child(preview_line(
                                12,
                                "+",
                                "let result = calculate()",
                                colors.success,
                                colors.addition_fill,
                                cx,
                            ))
                            .child(preview_line(
                                13,
                                "-",
                                "let old = legacy()",
                                colors.danger,
                                colors.deletion_fill,
                                cx,
                            ))
                            .child(preview_line(
                                14,
                                " ",
                                "return result",
                                cx.theme().foreground,
                                gpui::transparent_black(),
                                cx,
                            )),
                    )
                    .child(div().pt(px(8.)).child(note(
                        "Diff text uses the code font and code size from Appearance.",
                        cx.theme().kodem,
                    ))),
            )
            .child(
                section("Diff Display", cx)
                    .child(slider_row(
                        "Tab width",
                        "Spaces per tab character",
                        s.tab_width.to_string(),
                        Slider::new(
                            "tab-width",
                            s.tab_width as f32,
                            2.0,
                            8.0,
                            1.0,
                            |value: &f32, _, cx| {
                                GitSettings::update(cx, |s| s.tab_width = *value as u32)
                            },
                        ),
                        cx,
                    ))
                    .child(slider_row(
                        "Context lines",
                        "Lines shown around each change",
                        s.diff_context_lines.to_string(),
                        Slider::new(
                            "context-lines",
                            s.diff_context_lines as f32,
                            1.0,
                            20.0,
                            1.0,
                            |value: &f32, _, cx| {
                                GitSettings::update(cx, |s| s.diff_context_lines = *value as u32)
                            },
                        ),
                        cx,
                    ))
                    .child(toggle_row(
                        "line-numbers",
                        "Show line numbers",
                        "Display line numbers in diff view",
                        s.show_line_numbers,
                        |v, cx| GitSettings::update(cx, |s| s.show_line_numbers = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "word-wrap",
                        "Word wrap",
                        "Wrap long lines instead of scrolling",
                        s.diff_word_wrap,
                        |v, cx| GitSettings::update(cx, |s| s.diff_word_wrap = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "whitespace",
                        "Show whitespace",
                        "Visualize spaces and tab characters",
                        s.show_whitespace,
                        |v, cx| GitSettings::update(cx, |s| s.show_whitespace = v),
                        cx,
                    )),
            )
            .child(section("Diff Panel Position", cx).child(setting_row(
                "Diff panel",
                "Choose where file changes open",
                segmented(
                    "diff-panel",
                    [(false, "Right"), (true, "Bottom")],
                    &s.diff_panel_bottom,
                    |bottom, _, cx| GitSettings::update(cx, |s| s.diff_panel_bottom = *bottom),
                    cx,
                ),
                cx,
            )))
            .child(section("Commit Editor", cx).child(slider_row(
                "Title character limit",
                "Warn when the commit title gets longer",
                s.commit_title_limit.to_string(),
                Slider::new(
                    "title-limit",
                    s.commit_title_limit as f32,
                    50.0,
                    120.0,
                    1.0,
                    |value: &f32, _, cx| {
                        GitSettings::update(cx, |s| s.commit_title_limit = *value as u32)
                    },
                ),
                cx,
            )))
    }
}

// Returns one line of the diff preview. It matches the diff view: a tinted gutter column and a
// dithered marker on changed lines.
fn preview_line(
    number: u32,
    sign: &'static str,
    text: &'static str,
    color: Hsla,
    background: Hsla,
    cx: &App,
) -> Div {
    let colors = liquid_colors(cx);
    let changed = sign != " ";
    let (gutter_background, number_color) = if changed {
        (color.opacity(0.13), color.opacity(0.85))
    } else {
        (colors.panel_secondary, colors.text_tertiary.opacity(0.8))
    };
    h_flex()
        .relative()
        .w_full()
        .h(code_px(DIFF_TEXT_SIZE) + px(DIFF_LINE_PADDING))
        .items_center()
        .bg(background)
        .font_family(mono())
        .text_size(code_px(DIFF_TEXT_SIZE))
        .when(changed, |line| {
            line.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(3.))
                    .child(
                        Dither::new(DitherField::Fade {
                            from: Edge::Left,
                            start: 0.5,
                            end: 0.5,
                        })
                        .color(color)
                        .grid(1.0, 1.0)
                        .layer(),
                    ),
            )
        })
        .child(
            div()
                .w(px(42.))
                .h_full()
                .flex()
                .items_center()
                .justify_end()
                .pr_1p5()
                .bg(gutter_background)
                .border_r_1()
                .border_color(colors.separator)
                .text_color(number_color)
                .child(number.to_string()),
        )
        .child(div().w(px(18.)).text_center().text_color(color).child(sign))
        .child(div().text_color(color).child(text))
}
