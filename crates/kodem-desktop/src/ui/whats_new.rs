//! The What's new dialog: a board of this release's highlights around a version card, which the
//! app draws itself from `changelog/highlights.toml`, and every commit of the release in a folded
//! list below it. Figures use a dot-matrix face; each card forms out of dither in reading order.
use super::*;
use crate::release_notes::{self, Chip, Highlight, Row, RowState, Tone, Visual};
use crate::updater::{Channel, VERSION};
use dither::{BarSegment, Dither, DitherBar, DitherField, DottedRule, Edge};
use std::hash::{Hash, Hasher};

/// Width of the dialog that holds the board.
pub(super) const WIDTH: f32 = 1040.;
/// Height of the dialog's scrolling body at most.
pub(super) const BODY_HEIGHT: f32 = 820.;
/// Columns of the board, and of a board in a window too narrow for them.
const COLUMNS: usize = 4;
const NARROW_COLUMNS: usize = 2;
/// Window width below which the board takes its narrow columns.
const NARROW_WINDOW: f32 = 900.;
/// Space between the cards.
const GAP: f32 = 12.;
/// Pitch and dot of the dot-matrix face, in points.
const MATRIX_CELL: f32 = 3.;
const MATRIX_DOT: f32 = 2.;
/// Rows of a glyph of the dot-matrix face.
const MATRIX_ROWS: usize = 7;

impl Shell {
    /// Builds the body of the What's new dialog: the board of highlights around the version card,
    /// then the commits of this release, folded while the board has highlights.
    pub(super) fn whats_new(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let columns = if window.viewport_size().width < px(NARROW_WINDOW) {
            NARROW_COLUMNS
        } else {
            COLUMNS
        };
        let highlights = release_notes::HIGHLIGHTS;
        let spans: Vec<usize> = highlights
            .iter()
            .map(|highlight| if highlight.wide { 2 } else { 1 })
            .collect();
        let places = arrange(&spans, columns);
        // Cards arrive in reading order, wherever their highlight stands in the file.
        let mut order: Vec<usize> = (0..places.len()).collect();
        order.sort_by_key(|&card| (places[card].row, places[card].column));
        let mut board = div().w_full().grid().grid_cols(columns as u16).gap(px(GAP));
        for (beat, &card) in order.iter().enumerate() {
            let place = places[card];
            let surface = if card == 0 {
                version_card(theme)
            } else {
                highlight_card(&highlights[card - 1], theme)
            }
            .col_span(place.span as u16)
            .col_start(place.column as i16 + 1)
            .row_start(place.row as i16 + 1);
            board = board.child(motion::assemble(
                ("whats-new-card", card),
                surface,
                beat,
                theme.raised,
                crate::theme::RADIUS_SURFACE,
            ));
        }
        let changes = release_notes::changes();
        // Without highlights the commits are the news, so they show unfolded.
        let open = self.whats_new_changes || highlights.is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(board)
            .when(!changes.is_empty(), |body| {
                body.child(
                    div()
                        .id("whats-new-changes")
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .h(px(28.))
                        .px(px(6.))
                        .rounded(px(crate::theme::RADIUS_CONTROL))
                        .when(!highlights.is_empty(), |header| {
                            header
                                .cursor_pointer()
                                .hover(move |s| s.bg(theme.control_hover))
                                .child(
                                    icon("chevron-right")
                                        .size(px(11.))
                                        .text_color(theme.subtle)
                                        .rotate(if open {
                                            std::f32::consts::FRAC_PI_2
                                        } else {
                                            0.
                                        }),
                                )
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.whats_new_changes = !s.whats_new_changes;
                                    cx.notify();
                                }))
                        })
                        .child(caps("All changes", theme))
                        .child(figure(changes.len().to_string(), theme.muted))
                        .child(
                            div()
                                .flex_1()
                                .child(DottedRule::horizontal(theme.subtle.opacity(0.45))),
                        )
                        .when(!release_notes::PREVIOUS.is_empty(), |header| {
                            header.child(figure(
                                format!("since {}", release_notes::PREVIOUS),
                                theme.subtle,
                            ))
                        }),
                )
                .when(open, |body| {
                    body.child(div().flex().flex_col().gap(px(7.)).px(px(6.)).children(
                        changes.into_iter().map(|change| {
                            div()
                                .flex()
                                .items_start()
                                .gap(px(9.))
                                .child(div().pt(px(6.)).child(kit::marker(theme.accent)))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(text_px(13.))
                                        .line_height(relative(1.45))
                                        .child(change),
                                )
                        }),
                    ))
                })
            })
    }
}

/// Where a card lies on the board, in cells from its top leading corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Place {
    row: usize,
    column: usize,
    span: usize,
}

/// Lays out the version card and the highlights, whose widths in columns are `spans`, on a board
/// of `columns` columns. The version card is the first place. It takes two columns in the middle
/// of the second row when the highlights can fill the first row and both of its sides, and the
/// top leading corner otherwise. Each highlight takes the first place in reading order where it
/// fits. Then a card widens into each cell that stays empty beside it, so no row has a gap.
fn arrange(spans: &[usize], columns: usize) -> Vec<Place> {
    let version = 2.min(columns);
    let total: usize = spans.iter().map(|span| (*span).clamp(1, columns)).sum();
    let narrow = spans.iter().filter(|span| **span <= 1).count();
    let sides = columns - version;
    let centered = sides >= 2 && narrow >= sides && total >= columns + sides;
    let mut places = vec![Place {
        row: usize::from(centered),
        column: if centered { sides / 2 } else { 0 },
        span: version,
    }];
    for &span in spans {
        let span = span.clamp(1, columns);
        let mut row = 0;
        let place = loop {
            if let Some(column) = (0..=columns - span)
                .find(|&column| (column..column + span).all(|cell| !taken(&places, row, cell)))
            {
                break Place { row, column, span };
            }
            row += 1;
        };
        places.push(place);
    }
    let rows = places.iter().map(|place| place.row + 1).max().unwrap_or(0);
    for row in 0..rows {
        for column in 0..columns {
            if taken(&places, row, column) {
                continue;
            }
            // The card before the gap widens into it, or else the nearest card after it.
            if let Some(before) = places
                .iter_mut()
                .find(|place| place.row == row && place.column + place.span == column)
            {
                before.span += 1;
            } else if let Some(after) = places
                .iter_mut()
                .filter(|place| place.row == row && place.column > column)
                .min_by_key(|place| place.column)
            {
                after.span += after.column - column;
                after.column = column;
            }
        }
    }
    places
}

// Tells whether a card covers a cell.
fn taken(places: &[Place], row: usize, column: usize) -> bool {
    places.iter().any(|place| {
        place.row == row && (place.column..place.column + place.span).contains(&column)
    })
}

/// Returns the theme color that a highlight names.
fn color(tone: Tone, theme: Theme) -> Hsla {
    match tone {
        Tone::Accent => theme.accent,
        Tone::Green => theme.added,
        Tone::Red => theme.removed,
        Tone::Amber => theme.warning,
        Tone::Blue => theme.info,
        Tone::Muted => theme.muted,
    }
}

/// The version card: the build type's scene behind the mark, the version in the dot-matrix face,
/// the headline and how many commits the release holds.
fn version_card(theme: Theme) -> Div {
    let channel = Channel::current();
    let tone = channel_art::channel_tone(theme);
    let count = release_notes::changes().len();
    let summary = match (count, release_notes::PREVIOUS) {
        (0, _) => None,
        (count, "") => Some(format!("{count} changes")),
        (count, previous) => Some(format!("{count} changes since {previous}")),
    };
    kit::card(theme)
        .flex()
        .flex_col()
        .min_h(px(250.))
        .child(channel_art::card_art(theme))
        .child(
            div()
                .relative()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .px(px(28.))
                .py(px(24.))
                .child(icon("kodem").size(px(26.)).text_color(theme.text))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            icon(channel_art::channel_glyph())
                                .size(px(11.))
                                .text_color(tone),
                        )
                        .child(caps(channel.app_name(), theme).text_color(tone)),
                )
                .child(matrix(VERSION, theme.text, theme))
                .when_some(release_notes::HEADLINE, |card, headline| {
                    card.child(
                        div()
                            .max_w(px(380.))
                            .text_center()
                            .text_size(text_px(14.))
                            .line_height(relative(1.45))
                            .text_color(theme.muted)
                            .child(headline),
                    )
                })
                .when_some(summary, |card, summary| {
                    card.child(figure(summary, theme.subtle))
                }),
        )
}

/// A highlight's card: its live picture above its title and text, or beside them when the card is
/// wide and the picture compact.
fn highlight_card(highlight: &'static Highlight, theme: Theme) -> Div {
    let tone = color(highlight.tone, theme);
    let words = div()
        .relative()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(
            div()
                .text_size(text_px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .line_height(relative(1.25))
                .text_color(theme.text)
                .child(highlight.title),
        )
        .child(
            div()
                .text_size(text_px(12.5))
                .line_height(relative(1.45))
                .text_color(theme.muted)
                .child(highlight.text),
        );
    let card = kit::card(theme)
        .flex()
        .min_h(px(212.))
        .p(px(16.))
        .pb(px(18.));
    if matches!(highlight.visual, Visual::None) {
        return card.flex_col().justify_end().child(words);
    }
    let picture = div()
        .relative()
        .flex()
        .flex_col()
        .justify_center()
        .child(visual(&highlight.visual, tone, theme));
    // Above the words, a compact picture would stand alone in a wide empty band.
    let beside = highlight.wide
        && matches!(
            highlight.visual,
            Visual::Figure { .. } | Visual::Glyph(_) | Visual::Keys { .. }
        );
    if beside {
        // The words keep at least half of the card; a long caption wraps instead of
        // squeezing them into a narrow column.
        card.items_center()
            .gap(px(24.))
            .child(picture.min_w_0())
            .child(words.flex_1().min_w(relative(0.5)))
    } else {
        card.flex_col()
            .justify_end()
            .gap(px(16.))
            .child(picture.flex_1().min_h(px(92.)))
            .child(words)
    }
}

/// Draws the picture of a highlight in its tone.
fn visual(visual: &'static Visual, tone: Hsla, theme: Theme) -> AnyElement {
    match visual {
        Visual::None => div().into_any_element(),
        Visual::List {
            title: Some(title),
            badge,
            items,
        } => tree(title, *badge, items, tone, theme).into_any_element(),
        Visual::List {
            title: None, items, ..
        } => div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(items.iter().map(|row| plate_row(row, theme)))
            .into_any_element(),
        Visual::Chips(chips) => capsules(chips, theme).into_any_element(),
        Visual::Prompt { text, chips } => div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .when(!chips.is_empty(), |prompt| {
                prompt.child(capsules(chips, theme))
            })
            .child(composer(text, theme))
            .into_any_element(),
        Visual::Bars { label, bars, focus } => {
            let most = bars.iter().map(|bar| bar.value).fold(0., f32::max);
            let labelled = bars.iter().any(|bar| bar.label.is_some());
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .when_some(*label, |chart, label| {
                    chart.child(kit::section_header(label, None, theme))
                })
                .child(div().h(px(68.)).flex().items_end().gap(px(8.)).children(
                    bars.iter().enumerate().map(|(index, bar)| {
                        let lit = focus.is_none_or(|focus| focus == index);
                        let shade = if lit { tone } else { theme.subtle.opacity(0.7) };
                        div()
                            .relative()
                            .flex_1()
                            .h(px((68. * bar.value / most).max(2.)))
                            .rounded_t(px(2.))
                            .overflow_hidden()
                            .border_t_2()
                            .border_color(shade)
                            .child(
                                Dither::new(DitherField::Fade {
                                    from: Edge::Bottom,
                                    start: 1.,
                                    end: 0.45,
                                })
                                .color(shade.opacity(if lit { 0.85 } else { 0.5 }))
                                .grid(3., 2.)
                                .layer(),
                            )
                    }),
                ))
                .child(DottedRule::horizontal(theme.subtle.opacity(0.45)))
                .when(labelled, |chart| {
                    chart.child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .children(bars.iter().enumerate().map(|(index, bar)| {
                                let lit = focus.is_none_or(|focus| focus == index);
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_center()
                                    .text_ellipsis()
                                    .child(figure(
                                        bar.label.unwrap_or_default(),
                                        if lit { theme.text } else { theme.subtle },
                                    ))
                            })),
                    )
                })
                .into_any_element()
        }
        Visual::Figure {
            value,
            label,
            detail,
        } => div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(matrix(
                value,
                if tone == theme.muted {
                    theme.text
                } else {
                    tone
                },
                theme,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .min_w_0()
                    .child(
                        div()
                            .text_size(text_px(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(*label),
                    )
                    .when_some(*detail, |caption, detail| {
                        caption.child(
                            div()
                                .text_size(text_px(12.))
                                .text_color(theme.muted)
                                .child(detail),
                        )
                    }),
            )
            .into_any_element(),
        Visual::Meter { segments, caption } => {
            let parts: Vec<BarSegment> = segments
                .iter()
                .map(|segment| BarSegment {
                    weight: segment.weight,
                    color: color(segment.tone, theme),
                })
                .collect();
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(px(10.))
                        .child(DitherBar::new(parts, theme.subtle.opacity(0.3))),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x(px(12.))
                        .gap_y(px(4.))
                        .children(segments.iter().map(|segment| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(kit::marker(color(segment.tone, theme)))
                                .child(
                                    div()
                                        .text_size(text_px(12.))
                                        .text_color(theme.muted)
                                        .child(segment.label),
                                )
                        })),
                )
                .when_some(*caption, |meter, caption| {
                    meter.child(
                        div()
                            .text_size(text_px(12.))
                            .text_color(theme.subtle)
                            .child(caption),
                    )
                })
                .into_any_element()
        }
        Visual::Keys { keys, label } => div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().flex().gap(px(6.)).children(keys.iter().map(|key| {
                div()
                    .min_w(px(34.))
                    .h(px(34.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .border_1()
                    .border_b_2()
                    .border_color(theme.edge)
                    .bg(kit::raised_fill(theme.control))
                    .shadow(kit::raised_light(theme))
                    .font_family(crate::theme::mono())
                    .text_size(text_px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(kit::shortcut(*key))
            })))
            .when_some(*label, |keys, label| {
                keys.child(
                    div()
                        .text_size(text_px(12.))
                        .text_color(theme.muted)
                        .child(label),
                )
            })
            .into_any_element(),
        Visual::Glyph(glyph) => div()
            .relative()
            .size(px(96.))
            .flex()
            .items_center()
            .justify_center()
            .child(
                Dither::new(DitherField::Glow {
                    x: 0.5,
                    y: 0.5,
                    radius: 0.5,
                    strength: 0.75,
                })
                .color(tone.opacity(0.55))
                .grid(3., 2.)
                .toned()
                .layer(),
            )
            .child(kit::icon_tile(glyph, tone, 52., false))
            .into_any_element(),
    }
}

/// Rows as a tree under a header: a chevron, the title and its badge, then the rows along a
/// dotted rule.
fn tree(
    title: &'static str,
    badge: Option<&'static str>,
    rows: &'static [Row],
    tone: Hsla,
    theme: Theme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .h(px(30.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .border_1()
                .border_color(theme.border)
                .bg(kit::raised_fill(theme.control))
                .shadow(kit::raised_light(theme))
                .child(icon("chevron-down").size(px(11.)).text_color(theme.subtle))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_size(text_px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(title),
                )
                .when_some(badge, |header, badge| header.child(kit::chip(badge, tone))),
        )
        .child(
            div()
                .relative()
                .ml(px(14.))
                .pl(px(14.))
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(2.))
                        .bottom(px(2.))
                        .child(DottedRule::vertical(theme.subtle.opacity(0.5))),
                )
                .children(rows.iter().map(|row| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(row_mark(row, theme))
                        .child(row_text(row, theme))
                })),
        )
}

/// A row as a small raised plate: its mark, its text and its tag in its tone.
fn plate_row(row: &'static Row, theme: Theme) -> Div {
    div()
        .h(px(28.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(crate::theme::RADIUS_CONTROL))
        .border_1()
        .border_color(theme.border)
        .bg(kit::raised_fill(theme.control))
        .shadow(kit::raised_light(theme))
        .child(row_mark(row, theme))
        .child(row_text(row, theme))
        .when_some(row.tag, |plate, tag| {
            plate.child(figure(tag, color(row.tone, theme)))
        })
}

// The mark at the start of a row: a check, an open ring, a dithered marker or a faded ring.
fn row_mark(row: &Row, theme: Theme) -> AnyElement {
    match row.state {
        RowState::Done => icon("check")
            .size(px(11.))
            .text_color(theme.added)
            .into_any_element(),
        RowState::Dot => kit::marker(color(row.tone, theme)).into_any_element(),
        RowState::Open | RowState::Faded => div()
            .size(px(9.))
            .flex_shrink_0()
            .rounded_full()
            .border_1()
            .border_color(if row.state == RowState::Open {
                theme.muted
            } else {
                theme.subtle.opacity(0.6)
            })
            .into_any_element(),
    }
}

fn row_text(row: &'static Row, theme: Theme) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .text_ellipsis()
        .text_size(text_px(12.5))
        .text_color(if row.state == RowState::Faded {
            theme.subtle
        } else {
            theme.text
        })
        .child(row.text)
}

/// Capsules in a wrapping row: an icon, a label and a detail or a diffstat, as above the composer.
fn capsules(chips: &'static [Chip], theme: Theme) -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap(px(6.))
        .children(chips.iter().map(|chip| {
            let tone = color(chip.tone, theme);
            div()
                .h(px(26.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .border_1()
                .border_color(theme.edge)
                .bg(kit::raised_fill(theme.control))
                .shadow(kit::raised_light(theme))
                .when_some(chip.icon, |capsule, glyph| {
                    capsule.child(icon(glyph).size(px(12.)).text_color(tone))
                })
                .child(
                    div()
                        .text_size(text_px(12.))
                        .text_color(theme.text)
                        .child(chip.text),
                )
                .when_some(chip.detail, |capsule, detail| {
                    capsule.child(figure(detail, tone))
                })
                .when(chip.added.is_some() || chip.removed.is_some(), |capsule| {
                    capsule.child(line_totals(
                        chip.added.unwrap_or(0),
                        chip.removed.unwrap_or(0),
                        theme,
                    ))
                })
        }))
}

/// A still composer: a recessed field with the draft and the glass send key.
fn composer(text: &'static str, theme: Theme) -> Div {
    div()
        .h(px(42.))
        .pl(px(8.))
        .pr(px(6.))
        .flex()
        .items_center()
        .gap(px(9.))
        .rounded(px(crate::theme::RADIUS_SURFACE))
        .border_1()
        .border_color(theme.edge)
        .bg(theme.inset)
        .shadow(kit::recessed_light(theme))
        .child(icon("plus").size(px(13.)).text_color(theme.muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .text_size(text_px(13.))
                .text_color(theme.text)
                .child(text),
        )
        .child(
            div()
                .relative()
                .size(px(28.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .border_1()
                .border_color(gpui::transparent_black())
                .overflow_hidden()
                .group(kit::GLASS)
                .layer(kit::glass_body(theme.accent))
                .shadow(kit::glass_glow(theme.accent, theme, kit::Pose::Rest))
                .text_color(theme.on_accent)
                .child(kit::engraved("arrow-up", theme.accent, 13.)),
        )
}

/// Text in a 5×7 dot-matrix face, like a segment display: each pixel of a glyph is a block of
/// 2×2 dots, lit in `lit` over faint unlit dots. The sprite is baked once per text and color.
fn matrix(text: &'static str, lit: Hsla, theme: Theme) -> impl IntoElement {
    const BLOCK: usize = 2;
    let unlit = theme.subtle.opacity(0.16);
    // Each pixel column of the text: the glyph and its column, or a space between glyphs.
    let mut columns: Vec<Option<([u8; MATRIX_ROWS], usize)>> = Vec::new();
    for (index, character) in text.chars().enumerate() {
        if index > 0 {
            columns.push(None);
        }
        let (width, rows) = glyph(character);
        columns.extend((0..width).map(|column| Some((rows, column))));
    }
    let width = (columns.len() * BLOCK) as f32 * MATRIX_CELL;
    let height = (MATRIX_ROWS * BLOCK) as f32 * MATRIX_CELL;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    for color in [lit, unlit] {
        let rgba = hsla_to_rgba(color);
        for value in [rgba.red, rgba.green, rgba.blue, rgba.alpha] {
            value.to_bits().hash(&mut hasher);
        }
    }
    let digest = hasher.finish();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let scale = crate::theme::ui_scale();
            dither::paint_scene(
                bounds,
                MATRIX_CELL * scale,
                MATRIX_DOT * scale,
                digest,
                &|column, row| {
                    let (column, row) = (column as usize / BLOCK, row as usize / BLOCK);
                    let (rows, x) = (*columns.get(column)?)?;
                    let bits = *rows.get(row)?;
                    Some(if bits & (0b10000 >> x) != 0 {
                        lit
                    } else {
                        unlit
                    })
                },
                window,
                cx,
            );
        },
    )
    .flex_shrink_0()
    // Half a point more than the grid, so rounding never drops its last column.
    .w(px(width + 0.5))
    .h(px(height + 0.5))
}

/// Returns the width in pixels and the rows of a glyph of the dot-matrix face. A row's bits read
/// from 0b10000, its leading pixel. build.rs allows only the characters drawn here.
fn glyph(character: char) -> (usize, [u8; MATRIX_ROWS]) {
    match character {
        '0' => (
            5,
            [
                0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
            ],
        ),
        '1' => (
            5,
            [
                0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
            ],
        ),
        '2' => (
            5,
            [
                0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
            ],
        ),
        '3' => (
            5,
            [
                0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
            ],
        ),
        '4' => (
            5,
            [
                0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
            ],
        ),
        '5' => (
            5,
            [
                0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
            ],
        ),
        '6' => (
            5,
            [
                0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
            ],
        ),
        '7' => (
            5,
            [
                0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
            ],
        ),
        '8' => (
            5,
            [
                0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
            ],
        ),
        '9' => (
            5,
            [
                0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
            ],
        ),
        '%' => (
            5,
            [
                0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011,
            ],
        ),
        '+' => (5, [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0]),
        '-' => (5, [0, 0, 0, 0b11111, 0, 0, 0]),
        'x' => (5, [0, 0, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001]),
        '/' => (
            5,
            [
                0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
            ],
        ),
        '.' => (2, [0, 0, 0, 0, 0, 0b11000, 0b11000]),
        ',' => (2, [0, 0, 0, 0, 0b11000, 0b01000, 0b10000]),
        ':' => (2, [0, 0b11000, 0b11000, 0, 0b11000, 0b11000, 0]),
        _ => (3, [0; MATRIX_ROWS]),
    }
}
