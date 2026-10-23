//! The What's new dialog: a board of this release's highlights around a version card, which the
//! app draws itself from `changelog/highlights.toml`, and every commit of the release in a folded
//! list below it. Each highlight shows a live picture on a lit stage above its words. Figures use
//! a dot-matrix face; each card forms out of dither in reading order.
use super::*;
use crate::release_notes::{
    self, Area, Change, Chip, DiffLine, Highlight, MenuItem, Row, RowState, Slider, Tile, Toggle,
    Tone, Visual,
};
use crate::updater::{Channel, VERSION, version_label};
use dither::{BarSegment, Dither, DitherBar, DitherField, DottedRule, Edge};
use std::f32::consts::TAU;
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
/// Space between a card's edge and its stage.
const BEZEL: f32 = 6.;
/// Height of a stage at least. A taller picture makes its stage, and so its row, taller.
const STAGE_HEIGHT: f32 = 140.;
/// Width of a picture at most, so that a wide stage centers it instead of stretching it.
const PICTURE_WIDTH: f32 = 300.;
/// Pitch and dot of the dot-matrix face, in points.
const MATRIX_CELL: f32 = 3.;
const MATRIX_DOT: f32 = 2.;
/// Rows of a glyph of the dot-matrix face.
const MATRIX_ROWS: usize = 7;
/// Width of a figure's dot-matrix number on a card of one column at most, in points, before its
/// pixels take one dot.
const FIGURE_WIDTH: f32 = 132.;
/// Side of a gauge's ring, in points.
const GAUGE_SIZE: f32 = 84.;

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
        // Each row is a flex row, which takes the height of its tallest card. A grid in the
        // scrolling body sized its rows from the cards' minimum height, so pictures and words
        // ran out of the clipped cards.
        let rows = places.iter().map(|place| place.row + 1).max().unwrap_or(0);
        let mut lines: Vec<Vec<AnyElement>> = (0..rows).map(|_| Vec::new()).collect();
        for (beat, &card) in order.iter().enumerate() {
            let place = places[card];
            let surface = if card == 0 {
                version_card(theme)
            } else {
                highlight_card(&highlights[card - 1], place.span >= 2, theme)
            };
            // A card of n columns grows by n shares and starts from the n - 1 gaps that it
            // covers, so its edges line up with the columns of every other row.
            let surface = surface
                .min_w_0()
                .flex_shrink_0()
                .flex_basis(px((place.span - 1) as f32 * GAP))
                .flex_grow(place.span as f32);
            lines[place.row].push(motion::assemble(
                ("whats-new-card", card),
                surface,
                beat,
                theme.raised,
                crate::theme::RADIUS_SURFACE,
            ));
        }
        let board = div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .children(lines.into_iter().map(|cards| {
                div()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .gap(px(GAP))
                    .children(cards)
            }));
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
                                format!("since {}", version_label(release_notes::PREVIOUS)),
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
        (count, previous) => Some(format!("{count} changes since {}", version_label(previous))),
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
                .child(matrix(version_label(VERSION), theme.text, 2, theme))
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

/// A highlight's card: its picture on a stage, then its title and text across the card's width.
/// A `roomy` card spans two columns or more, so its picture can lay out in a row.
fn highlight_card(highlight: &'static Highlight, roomy: bool, theme: Theme) -> Div {
    let tone = color(highlight.tone, theme);
    kit::card(theme)
        .flex()
        .flex_col()
        .p(px(BEZEL))
        .child(stage(&highlight.visual, roomy, tone, theme))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .px(px(10.))
                .pt(px(12.))
                .pb(px(10.))
                .child(
                    div()
                        .text_size(text_px(15.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .line_height(relative(1.25))
                        .text_color(theme.text)
                        .child(highlight.title),
                )
                .child(
                    div()
                        .text_size(text_px(12.5))
                        .line_height(relative(1.5))
                        .text_color(theme.muted)
                        .child(highlight.text),
                ),
        )
}

/// The recessed stage of a picture, lit by a glow of the card's tone, with the picture in its
/// middle. It grows with its row, so the words of the cards of a row start at the same height.
fn stage(visual: &'static Visual, roomy: bool, tone: Hsla, theme: Theme) -> Div {
    div()
        .relative()
        .flex_grow(1.)
        .min_h(px(STAGE_HEIGHT))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .p(px(16.))
        .rounded(px(crate::theme::RADIUS_SURFACE - 2.))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(theme.inset)
        .shadow(kit::recessed_light(theme))
        .child(
            Dither::new(DitherField::Glow {
                x: 0.5,
                y: 0.45,
                radius: 0.85,
                strength: 0.5,
            })
            .color(tone.opacity(0.32))
            .grid(3., 1.5)
            .toned()
            .layer(),
        )
        .child(
            div()
                .relative()
                .w_full()
                .max_w(px(PICTURE_WIDTH))
                .flex()
                .flex_col()
                .child(picture(visual, roomy, tone, theme)),
        )
}

/// Draws the picture of a highlight in its tone, for a roomy card or a card of one column.
fn picture(visual: &'static Visual, roomy: bool, tone: Hsla, theme: Theme) -> Div {
    match visual {
        Visual::List {
            title: Some(title),
            badge,
            items,
        } => tree(title, *badge, items, tone, theme),
        Visual::List {
            title: None, items, ..
        } => div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .children(items.iter().map(|row| plate_row(row, theme))),
        Visual::Chips(chips) => capsules(chips, theme).justify_center(),
        Visual::Prompt { text, chips } => div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .when(!chips.is_empty(), |prompt| {
                prompt.child(capsules(chips, theme))
            })
            .child(composer(text, theme)),
        Visual::Bars { label, bars, focus } => bar_chart(*label, bars, *focus, tone, theme),
        Visual::Figure {
            value,
            label,
            detail,
        } => {
            let lit = if tone == theme.muted {
                theme.text
            } else {
                tone
            };
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(12.))
                .child(matrix(value, lit, figure_block(value, roomy), theme))
                .child(
                    div()
                        .max_w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_center()
                                .text_size(text_px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(*label),
                        )
                        .when_some(*detail, |caption, detail| {
                            caption.child(
                                div()
                                    .text_center()
                                    .text_size(text_px(11.5))
                                    .text_color(theme.muted)
                                    .child(detail),
                            )
                        }),
                )
        }
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
        }
        Visual::Keys { keys, label } => div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(6.))
                    .children(keys.iter().map(|key| keycap(key, theme))),
            )
            .when_some(*label, |keys, label| keys.child(caption(label, theme))),
        Visual::Glyph(glyph) => div().flex().justify_center().child(
            div()
                .relative()
                .size(px(100.))
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
                .child(kit::icon_tile(glyph, tone, 56., false)),
        ),
        Visual::Tiles(tiles) => div()
            .flex()
            .flex_wrap()
            .justify_center()
            .gap_x(px(14.))
            .gap_y(px(10.))
            .children(tiles.iter().map(|item| tile(item, theme))),
        Visual::Toggles(rows) => div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .children(rows.iter().map(|row| toggle(row, tone, theme))),
        Visual::Sliders(rows) => div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .children(rows.iter().map(|row| slider(row, tone, theme))),
        Visual::Compare {
            before,
            after,
            label,
        } => div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.))
            .child(
                // A card of one column stacks the two capsules, which would wrap in a row.
                div()
                    .max_w_full()
                    .flex()
                    .when(!roomy, |pair| pair.flex_col())
                    .items_center()
                    .gap(px(if roomy { 10. } else { 6. }))
                    .child(capsule(before, Look::Before, theme))
                    .child(
                        icon(if roomy { "arrow-right" } else { "arrow-down" })
                            .size(px(14.))
                            .text_color(color(after.tone, theme)),
                    )
                    .child(capsule(after, Look::After, theme)),
            )
            .when_some(*label, |compare, label| {
                compare.child(caption(label, theme))
            }),
        Visual::Menu { items, focus } => div()
            .flex()
            .justify_center()
            .child(menu(items, *focus, tone, theme)),
        Visual::Window { area, label } => div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .child(window_sketch(*area, tone, theme))
            .when_some(*label, |sketch, label| sketch.child(caption(label, theme))),
        Visual::Gauge { value, text, label } => div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .child(gauge(*value, *text, tone, theme))
            .child(caption(label, theme)),
        Visual::Steps { items, current } => steps(items, *current, tone, theme),
        Visual::Diff { file, lines } => diff(*file, lines, theme),
        Visual::Notice {
            icon: glyph,
            title,
            text,
            actions,
        } => div()
            .flex()
            .justify_center()
            .child(notice(glyph, title, *text, actions, tone, theme)),
        Visual::Choice {
            question,
            options,
            chosen,
        } => choice(question, options, *chosen, tone, theme),
    }
}

/// A short centered caption under a picture.
fn caption(text: &'static str, theme: Theme) -> Div {
    div()
        .max_w_full()
        .text_center()
        .text_size(text_px(12.))
        .line_height(relative(1.35))
        .text_color(theme.muted)
        .child(text)
}

/// A neutral raised plate, as of a control at rest.
fn plate(theme: Theme) -> Div {
    div()
        .rounded(px(crate::theme::RADIUS_CONTROL))
        .border_1()
        .border_color(theme.border)
        .bg(kit::raised_fill(theme.control))
        .shadow(kit::raised_light(theme))
}

/// One line of text that ends with an ellipsis when its space runs out.
fn label(text: &'static str, size: f32, tint: Hsla) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_size(text_px(size))
        .text_color(tint)
        .child(text)
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
        .gap(px(7.))
        .child(
            plate(theme)
                .h(px(28.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(icon("chevron-down").size(px(11.)).text_color(theme.subtle))
                .child(
                    label(title, 12.5, theme.text)
                        .flex_1()
                        .font_weight(FontWeight::MEDIUM),
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
                .gap(px(6.))
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
                        .h(px(18.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(row_mark(row, theme))
                        .child(row_text(row, theme))
                        .when_some(row.tag, |line, tag| {
                            line.child(figure(tag, color(row.tone, theme)))
                        })
                })),
        )
}

/// A row as a small raised plate: its mark, its text and its tag in its tone.
fn plate_row(row: &'static Row, theme: Theme) -> Div {
    plate(theme)
        .h(px(28.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(8.))
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
    label(
        row.text,
        12.5,
        if row.state == RowState::Faded {
            theme.subtle
        } else {
            theme.text
        },
    )
    .flex_1()
}

/// How a capsule shows a thing: as it is, as it was before a change, or as the change left it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Plain,
    Before,
    After,
}

/// Capsules in a wrapping row, as above the composer.
fn capsules(chips: &'static [Chip], theme: Theme) -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap(px(6.))
        .children(chips.iter().map(|chip| capsule(chip, Look::Plain, theme)))
}

/// A capsule: an icon, a label and a detail or a diffstat. Before a change it is faded and its
/// detail struck through; after it, it is lit in its tone.
fn capsule(chip: &'static Chip, look: Look, theme: Theme) -> Div {
    let tone = color(chip.tone, theme);
    let before = look == Look::Before;
    let surface = div()
        .h(px(28.))
        .max_w_full()
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(crate::theme::RADIUS_CONTROL))
        .border_1();
    let surface = if look == Look::After {
        kit::lit(surface.border_color(tone.opacity(0.45)), tone)
    } else {
        surface
            .border_color(theme.edge)
            .bg(kit::raised_fill(theme.control))
            .shadow(kit::raised_light(theme))
    };
    surface
        .when_some(chip.icon, |capsule, glyph| {
            capsule.child(icon(glyph).size(px(13.)).text_color(if before {
                theme.subtle
            } else {
                tone
            }))
        })
        .child(label(
            chip.text,
            12.5,
            if before { theme.muted } else { theme.text },
        ))
        .when_some(chip.detail, |capsule, detail| {
            capsule.child(
                figure(detail, if before { theme.subtle } else { tone })
                    .when(before, |detail| detail.line_through()),
            )
        })
        .when(chip.added.is_some() || chip.removed.is_some(), |capsule| {
            capsule.child(line_totals(
                chip.added.unwrap_or(0),
                chip.removed.unwrap_or(0),
                theme,
            ))
        })
}

/// A still composer: a recessed field with the draft and the glass send key.
fn composer(text: &'static str, theme: Theme) -> Div {
    div()
        .h(px(42.))
        .pl(px(10.))
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
        .child(label(text, 13., theme.text).flex_1())
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

/// Dot-matrix bars on a dotted base line, the focused one lit, with their labels below.
fn bar_chart(
    label: Option<&'static str>,
    bars: &'static [release_notes::Bar],
    focus: Option<usize>,
    tone: Hsla,
    theme: Theme,
) -> Div {
    const HEIGHT: f32 = 60.;
    let most = bars.iter().map(|bar| bar.value).fold(0., f32::max);
    let labelled = bars.iter().any(|bar| bar.label.is_some());
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .when_some(label, |chart, label| {
            chart.child(kit::section_header(label, None, theme))
        })
        .child(div().h(px(HEIGHT)).flex().items_end().gap(px(8.)).children(
            bars.iter().enumerate().map(|(index, bar)| {
                let lit = focus.is_none_or(|focus| focus == index);
                let shade = if lit { tone } else { theme.subtle.opacity(0.7) };
                div()
                    .relative()
                    .flex_1()
                    .h(px((HEIGHT * bar.value / most).max(2.)))
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
                            .overflow_hidden()
                            .flex()
                            .justify_center()
                            .child(figure(
                                bar.label.unwrap_or_default(),
                                if lit { theme.text } else { theme.subtle },
                            ))
                    })),
            )
        })
}

/// A keycap of a shortcut picture: a raised key with a heavier bottom edge.
fn keycap(key: &'static str, theme: Theme) -> Div {
    div()
        .min_w(px(38.))
        .h(px(38.))
        .px(px(9.))
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
        .text_size(text_px(15.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text)
        .child(kit::shortcut(key))
}

/// A named tile: a large icon tile, its name and an optional badge in its tone.
fn tile(item: &'static Tile, theme: Theme) -> Div {
    let tone = color(item.tone, theme);
    div()
        .w(px(78.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(7.))
        .child(kit::icon_tile(item.icon, tone, 44., false))
        .child(
            label(item.text, 12.5, theme.text)
                .max_w_full()
                .font_weight(FontWeight::MEDIUM),
        )
        .when_some(item.badge, |tile, badge| tile.child(kit::chip(badge, tone)))
}

/// A settings row with a switch.
fn toggle(row: &'static Toggle, tone: Hsla, theme: Theme) -> Div {
    plate(theme)
        .h(px(30.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(label(row.text, 12.5, theme.text).flex_1())
        .when_some(row.detail, |line, detail| {
            line.child(figure(detail, theme.subtle))
        })
        .child(switch(row.on, tone, theme))
}

/// A still switch, drawn as the settings draw theirs: lit dots behind the thumb when on.
fn switch(on: bool, tone: Hsla, theme: Theme) -> Div {
    const WIDTH: f32 = 28.;
    const HEIGHT: f32 = 16.;
    const THUMB: f32 = 10.;
    // The thumb sits inside the one-point border.
    let inset = (HEIGHT - 2. - THUMB) / 2.;
    div()
        .relative()
        .flex_shrink_0()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .rounded(px(4.))
        .overflow_hidden()
        .border_1()
        .border_color(if on { tone.opacity(0.65) } else { theme.edge })
        .bg(theme.inset)
        .child(
            Dither::new(DitherField::Fade {
                from: Edge::Left,
                start: if on { 0.95 } else { 0.25 },
                end: if on { 0.05 } else { 0.25 },
            })
            .color(if on { tone } else { theme.subtle.opacity(0.6) })
            .grid(2., 1.)
            .layer(),
        )
        .child(
            div()
                .absolute()
                .top(px(inset))
                .left(px(if on {
                    WIDTH - 2. - THUMB - inset
                } else {
                    inset
                }))
                .size(px(THUMB))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(2.5))
                .bg(if on { theme.text } else { theme.muted })
                .when(on, |thumb| {
                    thumb.child(div().size(px(3.)).rounded(px(1.)).bg(tone))
                }),
        )
}

/// A settings row with a slider: its name and reading above a dot-matrix track and its thumb.
fn slider(row: &'static Slider, tone: Hsla, theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(label(row.text, 12., theme.text).flex_1())
                .when_some(row.readout, |line, readout| {
                    line.child(figure(readout, tone))
                }),
        )
        .child(
            // The track is inset by half a thumb, so the thumb stays inside at both ends.
            div()
                .relative()
                .h(px(14.))
                .mx(px(4.))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(4.))
                        .h(px(6.))
                        .child(DitherBar::meter(
                            row.value,
                            tone,
                            theme.subtle.opacity(0.55),
                        )),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(relative(row.value))
                        .ml(px(-4.))
                        .w(px(8.))
                        .h(px(14.))
                        .rounded(px(2.5))
                        .border_1()
                        .border_color(tone)
                        .bg(theme.text),
                ),
        )
}

/// A floating menu with one row under the pointer, lit as a chosen menu row.
fn menu(items: &'static [MenuItem], focus: Option<usize>, tone: Hsla, theme: Theme) -> Div {
    div()
        .w_full()
        .max_w(px(232.))
        .flex()
        .flex_col()
        .gap(px(1.))
        .p(px(4.))
        .rounded(px(crate::theme::RADIUS_SURFACE))
        .border_1()
        .border_color(theme.edge)
        .bg(theme.raised)
        .shadow(theme.floating())
        .children(items.iter().enumerate().map(|(index, item)| {
            let chosen = focus == Some(index);
            div()
                .relative()
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(crate::theme::RADIUS_SMALL))
                .overflow_hidden()
                .when(chosen, |row| {
                    row.bg(tone.opacity(0.12))
                        .layer(dither::row_wash(tone, true, false))
                        .child(kit::selection_edge(tone))
                })
                .when_some(item.icon, |row, glyph| {
                    row.child(icon(glyph).size(px(13.)).text_color(if chosen {
                        tone
                    } else {
                        theme.muted
                    }))
                })
                .child(
                    label(
                        item.text,
                        12.5,
                        if chosen { theme.text } else { theme.muted },
                    )
                    .flex_1(),
                )
                .when_some(item.keys, |row, keys| row.child(kit::keycap(keys, theme)))
        }))
}

/// A sketch of the Kodem window: the title bar, the sidebar, the chat with the composer, and the
/// terminal or a side panel when they are the lit area.
fn window_sketch(area: Area, tone: Hsla, theme: Theme) -> Div {
    let line = theme.subtle.opacity(0.45);
    let title_bar = area_surface(area == Area::Titlebar, tone, theme)
        .h(px(13.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(3.))
        .px(px(6.))
        .border_b_1()
        .when(area != Area::Titlebar, |bar| {
            bar.border_color(theme.border).bg(theme.header)
        })
        .children((0..3).map(|_| div().size(px(4.)).rounded_full().bg(line)))
        .child(div().flex_1())
        .when(area == Area::Titlebar, |bar| {
            bar.child(div().h(px(6.)).w(px(22.)).rounded(px(2.)).bg(tone))
        });
    let sidebar = area_surface(area == Area::Sidebar, tone, theme)
        .w(relative(0.28))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(5.))
        .p(px(6.))
        .border_r_1()
        .when(area != Area::Sidebar, |side| {
            side.border_color(theme.border).bg(theme.sidebar)
        })
        .children([0.8, 0.55, 0.7, 0.45].map(|width| sketch_line(width, line)));
    let chat = area_surface(false, tone, theme)
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .gap(px(5.))
        .p(px(7.))
        .when(area == Area::Chat, |chat| {
            chat.child(
                Dither::new(DitherField::Flow {
                    scale: 0.22,
                    speed: 0.,
                    bias: -0.05,
                })
                .color(tone.opacity(0.55))
                .grid(3., 1.5)
                .toned()
                .layer(),
            )
        })
        .child(sketch_line(0.7, line))
        .child(
            div().flex().justify_end().child(
                div()
                    .w(relative(0.45))
                    .h(px(8.))
                    .rounded(px(3.))
                    .bg(theme.control),
            ),
        )
        .child(sketch_line(0.55, line));
    let composer = area_surface(area == Area::Composer, tone, theme)
        .mx(px(7.))
        .mb(px(6.))
        .h(px(13.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_end()
        .px(px(3.))
        .rounded(px(3.))
        .border_1()
        .when(area != Area::Composer, |field| {
            field.border_color(theme.edge).bg(theme.inset)
        })
        .child(
            div()
                .size(px(7.))
                .rounded(px(2.))
                .bg(if area == Area::Composer { tone } else { line }),
        );
    let main = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(chat)
        .child(composer)
        .when(area == Area::Terminal, |main| {
            main.child(
                area_surface(true, tone, theme)
                    .h(px(24.))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .px(px(7.))
                    .pt(px(7.))
                    .border_t_1()
                    // The grip on the terminal's top edge, which the pointer drags.
                    .child(
                        div()
                            .absolute()
                            .top(px(-3.))
                            .left_0()
                            .right_0()
                            .flex()
                            .justify_center()
                            .child(div().w(px(18.)).h(px(5.)).rounded(px(2.)).bg(tone)),
                    )
                    .child(sketch_line(0.5, tone.opacity(0.75)))
                    .child(sketch_line(0.3, line)),
            )
        });
    div()
        .w_full()
        .max_w(px(250.))
        .h(px(100.))
        .flex()
        .flex_col()
        .rounded(px(6.))
        .overflow_hidden()
        .border_1()
        .border_color(theme.edge)
        .bg(theme.canvas)
        .shadow(kit::raised_light(theme))
        .child(title_bar)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(sidebar)
                .child(main)
                .when(area == Area::Panel, |body| {
                    body.child(
                        area_surface(true, tone, theme)
                            .w(relative(0.3))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .p(px(6.))
                            .border_l_1()
                            .children([0.75, 0.5, 0.65].map(|width| sketch_line(width, line))),
                    )
                }),
        )
}

/// A part of the window sketch. The lit part takes the tone: a tint, a tinted edge and dots that
/// rise from its bottom, under the content that the caller adds.
fn area_surface(lit: bool, tone: Hsla, theme: Theme) -> Div {
    let part = div().relative();
    if !lit {
        return part;
    }
    part.border_color(tone.opacity(0.7))
        .bg(tone.opacity(0.1))
        .child(
            Dither::new(DitherField::Fade {
                from: Edge::Bottom,
                start: 0.45,
                end: 0.,
            })
            .color(tone.opacity(0.5))
            .grid(2., 1.)
            .layer(),
        )
        .text_color(theme.text)
}

/// A line of placeholder text in a sketch, `width` of its parent wide.
fn sketch_line(width: f32, shade: Hsla) -> Div {
    div()
        .relative()
        .flex_shrink_0()
        .w(relative(width))
        .h(px(3.))
        .rounded(px(1.5))
        .bg(shade)
}

/// A dot-matrix ring filled clockwise from the top to `value`, with an optional reading in it.
fn gauge(value: f32, reading: Option<&'static str>, tone: Hsla, theme: Theme) -> Div {
    // Width of the ring, in dots.
    const BAND: f32 = 4.;
    let unlit = theme.subtle.opacity(0.18);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "gauge".hash(&mut hasher);
    value.to_bits().hash(&mut hasher);
    for shade in [tone, unlit] {
        let rgba = hsla_to_rgba(shade);
        for part in [rgba.red, rgba.green, rgba.blue, rgba.alpha] {
            part.to_bits().hash(&mut hasher);
        }
    }
    let digest = hasher.finish();
    let ring = canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let scale = crate::theme::ui_scale();
            let cell = MATRIX_CELL * scale;
            // The same count of cells as the scene's grid takes.
            let cells = (f32::from(bounds.size.width) / cell).floor();
            let center = (cells - 1.) / 2.;
            let outer = cells / 2.;
            dither::paint_scene(
                bounds,
                cell,
                MATRIX_DOT * scale,
                digest,
                &|column, row| {
                    let (dx, dy) = (column as f32 - center, row as f32 - center);
                    let distance = (dx * dx + dy * dy).sqrt();
                    if distance > outer - 0.5 || distance < outer - 0.5 - BAND {
                        return None;
                    }
                    let turn = dx.atan2(-dy).rem_euclid(TAU) / TAU;
                    Some(if turn <= value { tone } else { unlit })
                },
                window,
                cx,
            );
        },
    )
    .absolute()
    .inset_0();
    div()
        .relative()
        .flex_shrink_0()
        // Half a point more than the grid, so rounding never drops its last column.
        .size(px(GAUGE_SIZE + 0.5))
        .flex()
        .items_center()
        .justify_center()
        .child(ring)
        .when_some(reading, |gauge, reading| {
            gauge.child(
                div()
                    .relative()
                    .font_family(crate::theme::mono())
                    .text_size(text_px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(reading),
            )
        })
}

/// Steps along a dotted line: done steps checked, the current one lit, the rest open.
fn steps(items: &'static [&'static str], current: usize, tone: Hsla, theme: Theme) -> Div {
    let last = items.len().saturating_sub(1);
    div()
        .w_full()
        .flex()
        .children(items.iter().enumerate().map(|(index, step)| {
            let done = index < current;
            let now = index == current;
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .w_full()
                        .h(px(16.))
                        .flex()
                        .items_center()
                        .child(step_link(index > 0, index <= current, tone, theme))
                        .child(step_mark(done, now, tone, theme))
                        .child(step_link(index < last, index < current, tone, theme)),
                )
                .child(
                    label(
                        step,
                        11.5,
                        if now {
                            theme.text
                        } else if done {
                            theme.muted
                        } else {
                            theme.subtle
                        },
                    )
                    .max_w_full()
                    .px(px(2.)),
                )
        }))
}

// Half of the dotted line between two steps, lit where the steps reached it.
fn step_link(shown: bool, lit: bool, tone: Hsla, theme: Theme) -> Div {
    div().flex_1().when(shown, |link| {
        link.child(DottedRule::horizontal(if lit {
            tone.opacity(0.8)
        } else {
            theme.subtle.opacity(0.45)
        }))
    })
}

// The mark of a step: a check when done, a lit dot when current, an open ring otherwise.
fn step_mark(done: bool, now: bool, tone: Hsla, theme: Theme) -> Div {
    let mark = div()
        .size(px(16.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .bg(theme.inset);
    if done {
        mark.border_color(tone.opacity(0.6))
            .bg(tone.opacity(0.18))
            .child(icon("check").size(px(9.)).text_color(tone))
    } else if now {
        mark.border_color(tone)
            .shadow(kit::tint_light(tone))
            .child(div().size(px(6.)).rounded_full().bg(tone))
    } else {
        mark.border_color(theme.subtle.opacity(0.6))
    }
}

/// Lines of a change in a recessed block, under the file's name and its line counts.
fn diff(file: Option<&'static str>, lines: &'static [DiffLine], theme: Theme) -> Div {
    let added = lines
        .iter()
        .filter(|line| line.change == Change::Added)
        .count();
    let removed = lines
        .iter()
        .filter(|line| line.change == Change::Removed)
        .count();
    kit::block(theme)
        .flex()
        .flex_col()
        .when_some(file, |block, file| {
            block.child(
                div()
                    .h(px(24.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(icon("file").size(px(11.)).text_color(theme.subtle))
                    .child(
                        label(file, 11., theme.muted)
                            .flex_1()
                            .font_family(crate::theme::mono()),
                    )
                    .child(line_totals(added, removed, theme)),
            )
        })
        .child(
            div()
                .py(px(4.))
                .flex()
                .flex_col()
                .children(lines.iter().map(|line| {
                    let (sign, tint, fill) = match line.change {
                        Change::Added => ("+", theme.added, theme.added.opacity(0.1)),
                        Change::Removed => ("−", theme.removed, theme.removed.opacity(0.1)),
                        Change::Same => (" ", theme.subtle, gpui::transparent_black()),
                    };
                    div()
                        .h(px(18.))
                        .flex()
                        .items_center()
                        .bg(fill)
                        .font_family(crate::theme::mono())
                        .child(
                            div()
                                .w(px(18.))
                                .flex_shrink_0()
                                .text_center()
                                .text_size(text_px(11.))
                                .text_color(tint)
                                .child(sign),
                        )
                        .child(
                            label(
                                line.text,
                                11.,
                                if line.change == Change::Same {
                                    theme.muted
                                } else {
                                    theme.text
                                },
                            )
                            .flex_1()
                            .pr(px(8.)),
                        )
                })),
        )
}

/// A floating dialog or notification: its icon, title and text, and its buttons with the last
/// one lit as the main action.
fn notice(
    glyph: &'static str,
    title: &'static str,
    text: Option<&'static str>,
    actions: &'static [&'static str],
    tone: Hsla,
    theme: Theme,
) -> Div {
    let main = actions.len().saturating_sub(1);
    div()
        .w_full()
        .max_w(px(272.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .p(px(11.))
        .rounded(px(crate::theme::RADIUS_SURFACE))
        .border_1()
        .border_color(theme.edge)
        .bg(theme.raised)
        .shadow(theme.floating())
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(10.))
                .child(kit::icon_tile(glyph, tone, 28., false))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(label(title, 12.5, theme.text).font_weight(FontWeight::SEMIBOLD))
                        .when_some(text, |words, text| {
                            words.child(
                                div()
                                    .text_size(text_px(11.5))
                                    .line_height(relative(1.4))
                                    .text_color(theme.muted)
                                    .line_clamp(2)
                                    .text_ellipsis()
                                    .child(text),
                            )
                        }),
                ),
        )
        .when(!actions.is_empty(), |notice| {
            notice.child(div().flex().justify_end().gap(px(6.)).children(
                actions.iter().enumerate().map(|(index, action)| {
                    let button = div()
                        .h(px(24.))
                        .min_w_0()
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .rounded(px(crate::theme::RADIUS_CONTROL))
                        .border_1()
                        .text_size(text_px(11.5));
                    if index == main {
                        kit::lit(button.border_color(tone.opacity(0.45)), tone)
                            .child(label(action, 11.5, tone))
                    } else {
                        button
                            .border_color(theme.edge)
                            .bg(kit::raised_fill(theme.control))
                            .shadow(kit::raised_light(theme))
                            .child(label(action, 11.5, theme.muted))
                    }
                }),
            ))
        })
}

/// A question with its options as radio rows, the chosen one lit.
fn choice(
    question: &'static str,
    options: &'static [&'static str],
    chosen: Option<usize>,
    tone: Hsla,
    theme: Theme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .pb(px(2.))
                .text_size(text_px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .line_height(relative(1.35))
                .text_color(theme.text)
                .line_clamp(2)
                .text_ellipsis()
                .child(question),
        )
        .children(options.iter().enumerate().map(|(index, option)| {
            let on = chosen == Some(index);
            let row = div()
                .h(px(26.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .border_1();
            let row = if on {
                kit::lit(row.border_color(tone.opacity(0.45)), tone)
            } else {
                row.border_color(theme.border)
                    .bg(kit::raised_fill(theme.control))
                    .shadow(kit::raised_light(theme))
            };
            row.child(
                div()
                    .size(px(11.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(if on { tone } else { theme.subtle.opacity(0.7) })
                    .when(on, |radio| {
                        radio.child(div().size(px(5.)).rounded_full().bg(tone))
                    }),
            )
            .child(label(option, 12., if on { theme.text } else { theme.muted }).flex_1())
        }))
}

/// Returns the pixel block of a figure's dot-matrix number: two dots a pixel, or one when the
/// number would be wider than the stage of its card allows.
fn figure_block(text: &str, roomy: bool) -> usize {
    let room = if roomy { PICTURE_WIDTH } else { FIGURE_WIDTH };
    if matrix_columns(text) as f32 * 2. * MATRIX_CELL <= room {
        2
    } else {
        1
    }
}

// The pixel columns of a text in the dot-matrix face, with one between glyphs.
fn matrix_columns(text: &str) -> usize {
    text.chars()
        .map(|character| glyph(character).0)
        .sum::<usize>()
        + text.chars().count().saturating_sub(1)
}

/// Text in a 5×7 dot-matrix face, like a segment display: each pixel of a glyph is a block of
/// `block`×`block` dots, lit in `lit` over faint unlit dots. The sprite is baked once per text,
/// block and color.
fn matrix(text: &'static str, lit: Hsla, block: usize, theme: Theme) -> impl IntoElement {
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
    let width = (columns.len() * block) as f32 * MATRIX_CELL;
    let height = (MATRIX_ROWS * block) as f32 * MATRIX_CELL;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    block.hash(&mut hasher);
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
                    let (column, row) = (column as usize / block, row as usize / block);
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
