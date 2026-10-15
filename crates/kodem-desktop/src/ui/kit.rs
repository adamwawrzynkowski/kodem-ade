//! Shared Liquid-style pieces: dithered control textures, mono labels with dotted rules, tinted
//! chips, dot-matrix meters and empty states. Every screen composes these, so the dot pitch,
//! tone steps and label type stay the same across the workspace.
use super::dither::{DitherBar, DottedRule};
use super::*;
use dither::{Dither, DitherField, Edge};

/// Static dots that shade one edge of a control surface. Add it before the control's content.
pub(super) fn shade(color: Hsla, from: Edge) -> Div {
    Dither::new(DitherField::Fade {
        from,
        start: 0.4,
        end: -0.1,
    })
    .color(color)
    .grid(2., 1.)
    .corner_radius(crate::theme::RADIUS_CONTROL)
    .layer()
}

/// Dots that light one edge of a control only while the pointer is over it.
pub(super) fn glint(color: Hsla, from: Edge) -> Div {
    Dither::new(DitherField::Fade {
        from,
        start: 0.75,
        end: -0.05,
    })
    .color(color)
    .grid(2., 1.)
    .corner_radius(crate::theme::RADIUS_CONTROL)
    .hover_only()
    .layer()
}

/// The group that lights the layers of a glass control under the pointer and on a press. The
/// control sets it on itself.
pub(super) const GLASS: &str = "glass";

/// Width of each of the two rings of a glass control.
const GLASS_RING: f32 = 2.;

/// Where the light falls on a glass control.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pose {
    Rest,
    Hover,
    Pressed,
}

/// Returns the body of a glass control with corners of `radius`: two rings around a flat face.
///
/// The outer ring is a bezel, light at the top and the bottom and darkest across the middle. The
/// inner ring catches a bright highlight at the top and the light that the glass bends back at
/// the bottom. The face carries the dot texture: a sheen that thins out from the top, a shade
/// along the bottom and more light under the pointer. Add it as the first child of the control,
/// which must belong to [`GLASS`].
pub(super) fn glass_body(tone: Hsla, radius: f32) -> Div {
    let face_radius = (radius - GLASS_RING * 2.).max(0.);
    let bezel = ring(
        radius,
        toward(tone, 0., 0.42),
        toward(tone, 1., 0.3),
        toward(tone, 1., 0.1),
    );
    let lens = ring(
        radius - GLASS_RING,
        toward(tone, 1., 0.22),
        toward(tone, 1., 0.85),
        toward(tone, 1., 0.5),
    )
    .id("glass-lens")
    .group_active(GLASS, |s| s.opacity(0.55));
    let face = div()
        .id("glass-face")
        .absolute()
        .size_full()
        .rounded(px(face_radius))
        .bg(tone)
        .group_hover(GLASS, move |s| s.bg(toward(tone, 1., 0.1)))
        .group_active(GLASS, move |s| s.bg(toward(tone, 0., 0.12)))
        .child(face_dots(
            Edge::Top,
            0.6,
            gpui::white().opacity(0.32),
            face_radius,
            false,
        ))
        .child(face_dots(
            Edge::Bottom,
            0.4,
            gpui::black().opacity(0.18),
            face_radius,
            false,
        ))
        .child(face_dots(
            Edge::Top,
            0.75,
            gpui::white().opacity(0.3),
            face_radius,
            true,
        ));
    // The body also covers the control's one-point edge, which only keyboard focus colors.
    let edge = gpui::px(-1.);
    div()
        .absolute()
        .top(edge)
        .left(edge)
        .right(edge)
        .bottom(edge)
        .child(bezel)
        .child(inset(GLASS_RING).child(lens))
        .child(inset(GLASS_RING * 2.).child(face))
}

/// Returns the drop shadows of a glass control: a tight contact shadow and a soft glow of its
/// tone below it. The glow grows under the pointer and tightens on a press.
pub(super) fn glass_glow(tone: Hsla, theme: Theme, pose: Pose) -> Vec<BoxShadow> {
    let shadow = |color: Hsla, y: f32, blur: f32, spread: f32| BoxShadow {
        inset: false,
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
    };
    let glow = tone.opacity(if theme.light { 0.32 } else { 0.42 });
    let contact = gpui::black().opacity(if theme.light { 0.2 } else { 0.45 });
    match pose {
        Pose::Rest => vec![shadow(contact, 1., 1.5, 0.), shadow(glow, 4., 10., -1.)],
        Pose::Hover => vec![
            shadow(contact, 1., 1.5, 0.),
            shadow(glow.opacity(1.3), 4., 13., 0.),
        ],
        Pose::Pressed => vec![
            shadow(contact, 0.5, 1., 0.),
            shadow(glow.opacity(0.6), 1., 5., -2.),
        ],
    }
}

/// Returns an icon engraved into a glass face: a darker copy of the glyph one point above it
/// reads as the edge of a cut.
pub(super) fn engraved(glyph: &'static str, tone: Hsla, size: f32) -> Div {
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .child(
            icon(glyph)
                .size(px(size))
                .absolute()
                .top(gpui::px(-1.))
                .left_0()
                .text_color(toward(tone, 0., 0.55).opacity(0.75)),
        )
        .child(icon(glyph).size(px(size)).absolute().top_0().left_0())
}

// A rounded layer that fills its parent: `middle` across its center, blending into `top` at its
// top edge and into `bottom` at its bottom edge. Each half fades to clear over the middle color,
// so the halves meet without a seam.
fn ring(radius: f32, middle: Hsla, top: Hsla, bottom: Hsla) -> Div {
    let clear = middle.opacity(0.);
    let radius = px(radius.max(0.));
    div()
        .absolute()
        .size_full()
        .rounded(radius)
        .bg(middle)
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(relative(0.5))
                .rounded_t(radius)
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(top, 0.),
                    linear_color_stop(clear, 1.),
                )),
        )
        .child(
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .h(relative(0.5))
                .rounded_b(radius)
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(clear, 0.),
                    linear_color_stop(bottom, 1.),
                )),
        )
}

// An absolute layer inset from every edge of its parent by `points`.
fn inset(points: f32) -> Div {
    div()
        .absolute()
        .top(px(points))
        .left(px(points))
        .right(px(points))
        .bottom(px(points))
}

// Static dots that fade from one edge of a glass face, or dots that light it under the pointer.
fn face_dots(from: Edge, start: f32, color: Hsla, radius: f32, hover_only: bool) -> Div {
    let dots = Dither::new(DitherField::Fade {
        from,
        start,
        end: -0.3,
    })
    .color(color)
    .grid(2., 1.)
    .toned()
    .corner_radius(radius);
    if hover_only {
        dots.hover_only().layer()
    } else {
        dots.layer()
    }
}

// Moves a color's lightness the share `amount` of the way to `target` and keeps its hue,
// saturation and alpha.
fn toward(mut color: Hsla, target: f32, amount: f32) -> Hsla {
    color.lightness += (target - color.lightness) * amount;
    color
}

/// A section header: an uppercase mono label, an optional count, and a dotted rule that fills
/// the remaining width.
pub(super) fn section_header(label: impl AsRef<str>, count: Option<usize>, theme: Theme) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(caps(label, theme))
        .when_some(count, |header, count| {
            header.child(figure(count.to_string(), theme.muted))
        })
        .child(
            div()
                .flex_1()
                .child(DottedRule::horizontal(theme.subtle.opacity(0.45))),
        )
}

/// A small square filled with dots in a tone, used as the marker of a status group.
pub(super) fn marker(tone: Hsla) -> Div {
    div()
        .relative()
        .size(px(9.))
        .flex_shrink_0()
        .rounded(px(2.))
        .overflow_hidden()
        .border_1()
        .border_color(tone.opacity(0.7))
        .child(
            Dither::new(DitherField::Fade {
                from: Edge::Bottom,
                start: 1.,
                end: 0.4,
            })
            .color(tone)
            .grid(1., 1.)
            .layer(),
        )
}

/// A section header for a status group: a dithered marker, a label and count in the tone color,
/// and a dotted rule in the same tone.
pub(super) fn status_header(label: impl AsRef<str>, count: usize, tone: Hsla, theme: Theme) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(marker(tone))
        .child(caps(label, theme).text_color(tone))
        .child(figure(count.to_string(), tone).font_weight(FontWeight::SEMIBOLD))
        .child(
            div()
                .flex_1()
                .child(DottedRule::horizontal(tone.opacity(0.35))),
        )
}

/// A compact tinted badge with a mono label, for states, modes and identities.
pub(super) fn chip(label: impl Into<SharedString>, tone: Hsla) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4.))
        .h(px(18.))
        .px(px(6.))
        .rounded(px(crate::theme::RADIUS_SMALL))
        .border_1()
        .border_color(tone.opacity(0.36))
        .bg(tone.opacity(0.08))
        .text_color(tone)
        .font_family(crate::theme::mono())
        .text_size(text_px(10.5))
        .line_height(relative(1.))
        .child(label.into())
}

/// Names the keys of the shortcuts in a label the way this platform does. Labels are written
/// with macOS symbols, so `⇧⌘M` reads `Ctrl+Shift+M` on Windows and Linux.
pub(super) fn shortcut(label: impl Into<SharedString>) -> SharedString {
    let label = label.into();
    if cfg!(target_os = "macos") || !label.contains(['⌘', '⌥', '⇧', '⌃']) {
        return label;
    }
    let mut text = String::with_capacity(label.len() + 16);
    let mut chars = label.chars().peekable();
    while let Some(&next) = chars.peek() {
        if !matches!(next, '⌘' | '⌥' | '⇧' | '⌃') {
            text.push(next);
            chars.next();
            continue;
        }
        // One chord: modifier symbols, spaced or not, then the key up to the next space.
        let (mut control, mut alt, mut shift) = (false, false, false);
        while let Some(&symbol) = chars.peek() {
            match symbol {
                '⌘' | '⌃' => control = true,
                '⌥' => alt = true,
                '⇧' => shift = true,
                ' ' => {}
                _ => break,
            }
            chars.next();
        }
        let mut key = String::new();
        while let Some(&character) = chars.peek() {
            if character == ' ' {
                break;
            }
            key.push(character);
            chars.next();
        }
        for (held, name) in [(control, "Ctrl+"), (alt, "Alt+"), (shift, "Shift+")] {
            if held {
                text.push_str(name);
            }
        }
        text.push_str(match key.as_str() {
            "⇥" => "Tab",
            "Return" | "⏎" | "↩" => "Enter",
            key => key,
        });
    }
    text.into()
}

/// A keycap that names a keyboard shortcut next to the action it triggers.
pub(super) fn keycap(keys: impl Into<SharedString>, theme: Theme) -> Div {
    div()
        .flex_shrink_0()
        .h(px(16.))
        .px(px(4.))
        .flex()
        .items_center()
        .rounded(px(3.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.inset)
        .text_color(theme.subtle)
        .font_family(crate::theme::mono())
        .text_size(text_px(10.))
        .line_height(relative(1.))
        .child(shortcut(keys))
}

/// Number of blocks in a micro diffstat.
const DIFFSTAT_BLOCKS: usize = 5;

// Splits the diffstat blocks into added and removed blocks, keeping both sides visible.
fn diffstat_blocks(added: usize, removed: usize) -> (usize, usize) {
    let total = added + removed;
    if total == 0 {
        return (0, 0);
    }
    let filled = DIFFSTAT_BLOCKS.min(total);
    let mut green = ((added as f32 / total as f32) * filled as f32).round() as usize;
    if added > 0 {
        green = green.max(1);
    }
    green = green.min(filled);
    let red = if removed > 0 {
        (filled - green).max(1).min(filled)
    } else {
        0
    };
    (green.min(filled - red), red)
}

/// Mono "+N −N" counts with a five-block micro diffstat.
pub(super) fn diffstat(added: usize, removed: usize, theme: Theme) -> Div {
    let (green, red) = diffstat_blocks(added, removed);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .when(added > 0, |d| {
            d.child(figure(format!("+{added}"), theme.added))
        })
        .when(removed > 0, |d| {
            d.child(figure(format!("−{removed}"), theme.removed))
        })
        .child(
            div()
                .flex()
                .gap(px(1.5))
                .children((0..DIFFSTAT_BLOCKS).map(|index| {
                    let color = if index < green {
                        theme.added
                    } else if index < green + red {
                        theme.removed
                    } else {
                        theme.subtle.opacity(0.3)
                    };
                    div().size(px(5.)).rounded(px(1.)).bg(color)
                })),
        )
}

/// A dot-matrix meter of a fixed size. An unknown value renders as an empty bar.
pub(super) fn meter(fraction: Option<f32>, color: Hsla, width: f32, theme: Theme) -> Div {
    div()
        .relative()
        .flex_shrink_0()
        .w(px(width))
        .h(px(6.))
        .child(DitherBar::meter(
            fraction.unwrap_or(0.),
            color,
            theme.subtle.opacity(0.55),
        ))
}

/// Returns the meter color for a used fraction: calm, then warning, then critical.
pub(super) fn usage_tone(fraction: f32, theme: Theme) -> Hsla {
    if fraction >= 0.9 {
        theme.removed
    } else if fraction >= 0.7 {
        theme.warning
    } else {
        theme.accent
    }
}

/// A square icon tile with a dithered fill rising from the bottom, for list rows and headers.
pub(super) fn icon_tile(glyph: &'static str, tone: Hsla, size: f32, animated: bool) -> Div {
    let radius = (size * 0.27).round();
    let fill = Dither::new(DitherField::Fade {
        from: Edge::Bottom,
        start: 0.8,
        end: 0.,
    })
    .color(tone.opacity(0.55))
    .grid(2., 1.)
    .corner_radius(radius);
    div()
        .relative()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .overflow_hidden()
        .border_1()
        .border_color(tone.opacity(0.35))
        .bg(tone.opacity(0.08))
        .child(if animated { fill.animated() } else { fill }.layer())
        .child(
            div()
                .relative()
                .child(icon(glyph).size(px((size * 0.5).round())).text_color(tone)),
        )
}

/// The header of a dialog or card: an icon tile, a title and a mono detail line over a dithered
/// glow in the tone color.
pub(super) fn glow_header(
    glyph: &'static str,
    title: impl Into<SharedString>,
    detail: impl IntoElement,
    tone: Hsla,
    theme: Theme,
) -> Div {
    div()
        .relative()
        .w_full()
        .overflow_hidden()
        .child(
            Dither::new(
                DitherField::Glow {
                    x: 1.,
                    y: 0.,
                    radius: 0.62,
                    strength: 0.95,
                }
                .times(DitherField::Fade {
                    from: Edge::Top,
                    start: 1.,
                    end: 0.35,
                }),
            )
            .color(tone.opacity(0.5))
            .grid(3., 2.)
            .toned()
            .layer(),
        )
        .child(
            div()
                .relative()
                .w_full()
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(16.))
                .pt(px(16.))
                .pb(px(12.))
                .child(icon_tile(glyph, tone, 36., false))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(text_px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text)
                                .text_ellipsis()
                                .child(title.into()),
                        )
                        .child(
                            div()
                                .font_family(crate::theme::mono())
                                .text_size(text_px(11.))
                                .text_color(theme.subtle)
                                .text_ellipsis()
                                .child(detail),
                        ),
                ),
        )
}

/// An empty or loading state with a dithered emblem above a title and a detail line.
///
/// The emblem is still. A moving one would render the whole workspace again on every step for
/// as long as the empty state is on screen.
pub(super) fn empty_state(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    field: DitherField,
    theme: Theme,
) -> Div {
    let detail: SharedString = detail.into();
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(12.))
        .p(px(24.))
        .child(
            div().relative().w(px(168.)).h(px(72.)).child(
                Dither::new(field)
                    .color(theme.accent.opacity(0.85))
                    .grid(4., 3.)
                    .toned()
                    .corner_radius(8.)
                    .layer(),
            ),
        )
        .child(
            div()
                .text_size(text_px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(title.into()),
        )
        .when(!detail.is_empty(), |state| {
            state.child(
                div()
                    .max_w(px(340.))
                    .text_center()
                    .text_size(text_px(12.))
                    .text_color(theme.subtle)
                    .child(detail),
            )
        })
}

/// A compact empty state for narrow panels such as the sidebar.
pub(super) fn empty_hint(text: impl Into<SharedString>, theme: Theme) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.))
        .py(px(24.))
        .child(
            div().relative().w(px(84.)).h(px(28.)).child(
                Dither::new(
                    DitherField::Glow {
                        x: 0.5,
                        y: 0.5,
                        radius: 0.55,
                        strength: 0.8,
                    }
                    .times(DitherField::Flow {
                        scale: 0.25,
                        speed: 1.5,
                        bias: 0.1,
                    }),
                )
                .color(theme.subtle)
                .grid(3., 2.)
                .layer(),
            ),
        )
        .child(
            div()
                .font_family(crate::theme::mono())
                .text_size(text_px(11.))
                .text_color(theme.subtle)
                .text_center()
                .child(text.into()),
        )
}

/// A short animated strip with a mono label that shows that work is in progress.
pub(super) fn loader_row(label: impl Into<SharedString>, theme: Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(
            div().relative().w(px(48.)).h(px(9.)).flex_shrink_0().child(
                Dither::new(DitherField::Flow {
                    scale: 0.22,
                    speed: 6.,
                    bias: -0.12,
                })
                .color(theme.accent)
                .grid(3., 2.)
                .animated()
                .layer(),
            ),
        )
        .child(
            div()
                .font_family(crate::theme::mono())
                .text_size(text_px(11.))
                .text_color(theme.muted)
                .child(label.into()),
        )
}

/// The selected-tab indicator: a two-point accent line with a dithered glow that fades upward,
/// like phosphor bleeding on an old display. Place it in a relatively positioned tab cell.
pub(super) fn tab_indicator(accent: Hsla) -> Div {
    div()
        .absolute()
        .bottom_0()
        .left(px(4.))
        .right(px(4.))
        .h(px(9.))
        .child(
            Dither::new(DitherField::Fade {
                from: Edge::Bottom,
                start: 0.7,
                end: 0.,
            })
            .color(accent.opacity(0.5))
            .grid(2., 1.)
            .layer(),
        )
        .child(
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .h(px(2.))
                .bg(accent),
        )
}

/// A card surface: raised fill, hairline edge and a dithered shade along its bottom edge.
pub(super) fn card(theme: Theme) -> Div {
    div()
        .relative()
        .overflow_hidden()
        .rounded(px(crate::theme::RADIUS_SURFACE))
        .border_1()
        .border_color(theme.border)
        .bg(theme.raised)
        .child(dither::edge_shade(
            theme.dot,
            0.22,
            crate::theme::RADIUS_SURFACE,
        ))
}

/// A content block: an inset surface with a hairline edge for code, output, a diff or a table.
pub(super) fn block(theme: Theme) -> Div {
    div()
        .w_full()
        .min_w_0()
        .rounded(px(crate::theme::RADIUS_CONTROL))
        .border_1()
        .border_color(theme.border)
        .bg(theme.inset)
        .overflow_hidden()
}

/// The header of a content block: a dithered marker and a mono label in `tone`, then a dotted
/// rule that fills the width. Dots in the tone shade its leading edge. Add counts and actions
/// as further children; they sit after the rule.
pub(super) fn block_header(label: impl AsRef<str>, tone: Hsla, theme: Theme) -> Div {
    block_title(caps(label, theme).text_color(tone), tone, theme)
}

/// A block header whose title is any element, such as a file path that keeps its case.
pub(super) fn block_title(title: impl IntoElement, tone: Hsla, theme: Theme) -> Div {
    div()
        .relative()
        .h(px(26.))
        .flex_shrink_0()
        .pl(px(10.))
        .pr(px(4.))
        .flex()
        .items_center()
        .gap(px(8.))
        .bg(theme.header)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(px(96.))
                .child(
                    Dither::new(DitherField::Fade {
                        from: Edge::Left,
                        start: 0.5,
                        end: -0.1,
                    })
                    .color(tone.opacity(0.4))
                    .grid(2., 1.),
                ),
        )
        .child(marker(tone))
        .child(title)
        .child(
            div()
                .flex_1()
                .min_w(px(8.))
                .child(DottedRule::horizontal(theme.subtle.opacity(0.4))),
        )
}

/// A key and its value joined by a dotted leader, for readouts in popovers and cards.
pub(super) fn leader(
    label: impl AsRef<str>,
    value: impl Into<SharedString>,
    tone: Hsla,
    theme: Theme,
) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(caps(label, theme).text_color(theme.muted))
        .child(
            div()
                .flex_1()
                .min_w(px(12.))
                .child(DottedRule::horizontal(theme.subtle.opacity(0.4))),
        )
        .child(figure(value, tone))
}

/// The accent edge of a selected row: a two-point bar on its leading side.
pub(super) fn selection_edge(tone: Hsla) -> Div {
    div()
        .absolute()
        .left_0()
        .top(px(4.))
        .bottom(px(4.))
        .w(px(2.))
        .rounded(px(1.))
        .bg(tone)
}
