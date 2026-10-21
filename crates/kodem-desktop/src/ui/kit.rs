//! Shared Liquid-style pieces: dithered control textures, mono labels with dotted rules, tinted
//! chips, dot-matrix meters and empty states. Every screen composes these, so the dot pitch,
//! tone steps and label type stay the same across the workspace.
use super::dither::{DitherBar, DottedRule};
use super::*;
use dither::{Dither, DitherField, Edge};

/// A layer that takes the shape of the surface it covers. [`Layers::layer`] measures the
/// surface and builds the layer for it.
pub(super) trait Fit {
    /// Builds the layer for `corners`, the corners inside the surface's border in window pixels.
    fn fit(self, corners: Corners<Pixels>) -> AnyElement;
}

impl<F: FnOnce(Corners<Pixels>) -> AnyElement> Fit for F {
    fn fit(self, corners: Corners<Pixels>) -> AnyElement {
        self(corners)
    }
}

/// Adds layers that fill a surface inside its border and round as it does, at any radius,
/// border and size.
pub(super) trait Layers: Sized {
    /// Adds `layer` under the surface's content. A [`Button`] builds it from its final style;
    /// a plain element reads the radius and border set before this call.
    fn layer(self, layer: impl Fit + 'static) -> Self;
}

impl Layers for Button {
    fn layer(self, layer: impl Fit + 'static) -> Self {
        self.fitted(move |corners| layer.fit(corners))
    }
}

impl Layers for Div {
    fn layer(mut self, layer: impl Fit + 'static) -> Self {
        let corners = inner_corners(self.style());
        self.child(layer.fit(corners))
    }
}

impl Layers for Stateful<Div> {
    fn layer(mut self, layer: impl Fit + 'static) -> Self {
        let corners = inner_corners(self.style());
        self.child(layer.fit(corners))
    }
}

// The corners inside the border of an element as its style sets them so far. Lengths in rems
// take GPUI's default rem size; the interface sets its lengths in pixels.
fn inner_corners(style: &StyleRefinement) -> Corners<Pixels> {
    crate::components::inner_corners(style, gpui::px(16.))
}

/// Static dots that shade one edge of a control surface. Add it with [`Layers::layer`].
pub(super) fn shade(color: Hsla, from: Edge) -> Dither {
    Dither::new(DitherField::Fade {
        from,
        start: 0.4,
        end: -0.1,
    })
    .color(color)
    .grid(2., 1.)
}

/// Dots that light one edge of a control only while the pointer is over it. Add it with
/// [`Layers::layer`].
pub(super) fn glint(color: Hsla, from: Edge) -> Dither {
    Dither::new(DitherField::Fade {
        from,
        start: 0.75,
        end: -0.05,
    })
    .color(color)
    .grid(2., 1.)
    .hover_only()
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

/// Returns the body of a glass control: two rings around a flat face, with the corners of the
/// control. Add it with [`Layers::layer`] before the control's content.
///
/// The outer ring is a bezel, light at the top and the bottom and darkest across the middle. The
/// inner ring catches a bright highlight at the top and the light that the glass bends back at
/// the bottom. The face carries the dot texture: a sheen that thins out from the top, a shade
/// along the bottom and more light under the pointer. The control must belong to [`GLASS`] and
/// have a one-point border, which the body covers.
pub(super) fn glass_body(tone: Hsla) -> impl Fit {
    move |inner: Corners<Pixels>| glass_layers(tone, inner).into_any_element()
}

// The rings and the face of a glass control whose corners inside its border are `inner`.
fn glass_layers(tone: Hsla, inner: Corners<Pixels>) -> Div {
    // The body also covers the control's one-point edge, which only keyboard focus colors.
    let edge = gpui::px(1.);
    let ring_width = crate::theme::px(GLASS_RING);
    let within = |depth: Pixels| {
        inner.map(|radius| {
            if *radius > Pixels::ZERO {
                (*radius + edge - depth).max(Pixels::ZERO)
            } else {
                Pixels::ZERO
            }
        })
    };
    let face_radius = within(ring_width * 2.);
    let bezel = ring(
        within(Pixels::ZERO),
        toward(tone, 0., 0.42),
        toward(tone, 1., 0.3),
        toward(tone, 1., 0.1),
    );
    let lens = ring(
        within(ring_width),
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
        .map(|face| round(face, face_radius))
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
    div()
        .absolute()
        .top(-edge)
        .left(-edge)
        .right(-edge)
        .bottom(-edge)
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

/// Rounds each corner of an element to its own radius.
pub(super) fn round<E: Styled>(element: E, corners: Corners<Pixels>) -> E {
    element
        .rounded_tl(corners.top_left)
        .rounded_tr(corners.top_right)
        .rounded_br(corners.bottom_right)
        .rounded_bl(corners.bottom_left)
}

// A rounded layer that fills its parent: `middle` across its center, blending into `top` at its
// top edge and into `bottom` at its bottom edge. Each blend fades to clear at the middle, so the
// two meet without a seam. Both cover the whole layer and take its corners, so the ring keeps
// its shape at any radius and height.
fn ring(corners: Corners<Pixels>, middle: Hsla, top: Hsla, bottom: Hsla) -> Div {
    let clear = middle.opacity(0.);
    let layer = || round(div().absolute().size_full(), corners);
    layer()
        .bg(middle)
        .child(layer().bg(linear_gradient(
            180.,
            linear_color_stop(top, 0.),
            linear_color_stop(clear, 0.5),
        )))
        .child(layer().bg(linear_gradient(
            180.,
            linear_color_stop(clear, 0.5),
            linear_color_stop(bottom, 1.),
        )))
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
fn face_dots(
    from: Edge,
    start: f32,
    color: Hsla,
    corners: Corners<Pixels>,
    hover_only: bool,
) -> Div {
    let dots = Dither::new(DitherField::Fade {
        from,
        start,
        end: -0.3,
    })
    .color(color)
    .grid(2., 1.)
    .toned()
    .fitted(corners, Size::new(true, true));
    if hover_only {
        dots.hover_only().layer()
    } else {
        dots.layer()
    }
}

/// Moves a color's lightness the share `amount` of the way to `target` and keeps its hue,
/// saturation and alpha.
pub(super) fn toward(mut color: Hsla, target: f32, amount: f32) -> Hsla {
    color.lightness += (target - color.lightness) * amount;
    color
}

/// Returns the light of a raised plate: a highlight inside its top edge and, while nothing is
/// attached below it, a darker line inside its bottom edge and a tight contact shadow.
pub(super) fn plate_light(theme: Theme, grounded: bool) -> Vec<BoxShadow> {
    let mut light = vec![BoxShadow {
        inset: true,
        color: theme.highlight,
        offset: point(px(0.), px(1.)),
        blur_radius: px(0.),
        spread_radius: px(0.),
    }];
    if grounded {
        light.extend([
            BoxShadow {
                inset: true,
                color: gpui::black().opacity(if theme.light { 0.03 } else { 0.18 }),
                offset: point(px(0.), px(-1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
            },
            BoxShadow {
                inset: false,
                color: theme.shadow,
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
            },
        ]);
    }
    light
}

/// Returns the light of a recessed surface, such as a field, a content block or a pressed
/// control: a soft shadow that the edge above casts inside it and a faint highlight on the lip
/// below it.
pub(super) fn recessed_light(theme: Theme) -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            inset: true,
            color: gpui::black().opacity(if theme.light { 0.07 } else { 0.32 }),
            offset: point(px(0.), px(1.)),
            blur_radius: px(3.),
            spread_radius: px(0.),
        },
        BoxShadow {
            inset: false,
            color: theme.highlight.opacity(if theme.light { 0.8 } else { 1. }),
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
        },
    ]
}

/// One slice of a recessed well under a raised plate. Consecutive rows draw consecutive slices,
/// so the well reads as one surface: the first slice takes the shadow that the plate casts into
/// it, and the last closes it with rounded corners.
pub(super) fn well(theme: Theme, first: bool, last: bool) -> Div {
    let cast = gpui::black().opacity(if theme.light { 0.1 } else { 0.4 });
    div()
        .px(px(4.))
        .py(px(1.))
        .bg(theme.inset)
        .border_l_1()
        .border_r_1()
        .border_color(theme.border)
        .when(first, |well| {
            well.pt(px(5.)).shadow(vec![BoxShadow {
                inset: true,
                color: cast,
                offset: point(px(0.), px(3.)),
                blur_radius: px(5.),
                spread_radius: px(0.),
            }])
        })
        .when(last, |well| {
            well.pb(px(5.))
                .border_b_1()
                .rounded_b(px(crate::theme::RADIUS_SURFACE))
        })
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
        .layer(
            Dither::new(DitherField::Fade {
                from: Edge::Bottom,
                start: 1.,
                end: 0.4,
            })
            .color(tone)
            .grid(1., 1.),
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

/// A compact tinted badge with a mono label, for states, modes and identities. Dots in its
/// tone shade its bottom edge, as they shade a message.
pub(super) fn chip(label: impl Into<SharedString>, tone: Hsla) -> Div {
    div()
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4.))
        .h(px(18.))
        .px(px(6.))
        .rounded(px(crate::theme::RADIUS_SMALL))
        .border_1()
        .border_color(tone.opacity(0.36))
        .bg(tint_fill(tone, 0.08))
        .shadow(tint_light(tone))
        .layer(shade(tone.opacity(0.25), Edge::Bottom))
        .text_color(tone)
        .font_family(crate::theme::mono())
        .text_size(text_px(10.5))
        .line_height(relative(1.))
        .child(label.into())
}

/// The mark of a provider whose support in Kodem is in beta.
pub(super) fn beta_chip(theme: Theme) -> Div {
    chip("BETA", theme.warning)
}

/// Returns the fill of a tinted surface with slight depth: the tint a little stronger at the top
/// than at the bottom, as if lit from above. `strength` is the mean alpha of the tint.
pub(super) fn tint_fill(tone: Hsla, strength: f32) -> Background {
    linear_gradient(
        180.,
        linear_color_stop(tone.opacity(strength * 1.5), 0.),
        linear_color_stop(tone.opacity(strength * 0.6), 1.),
    )
}

/// Returns the light of a tinted surface with slight depth: a lighter line of its tone inside
/// its top edge, a deeper line inside its bottom edge and a tight shadow of the deep tone below.
/// The lines sit one point inside the edge, so a hairline border does not hide them.
pub(super) fn tint_light(tone: Hsla) -> Vec<BoxShadow> {
    let line = |color: Hsla, y: f32| BoxShadow {
        inset: true,
        color,
        offset: point(px(0.), gpui::px(y)),
        blur_radius: px(0.),
        spread_radius: px(0.),
    };
    vec![
        line(toward(tone, 1., 0.5).opacity(0.38), 2.),
        line(toward(tone, 0., 0.5).opacity(0.3), -2.),
        BoxShadow {
            inset: false,
            color: toward(tone, 0., 0.7).opacity(0.36),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
        },
    ]
}

/// Lights a toggle or ghost button that is on: the tint of its tone with the depth of a chosen
/// surface, and its label in the tone.
pub(super) fn lit<E: Styled>(element: E, tone: Hsla) -> E {
    element
        .bg(tint_fill(tone, 0.08))
        .shadow(tint_light(tone))
        .text_color(tone)
}

/// Returns the fill of a neutral raised surface with the same slight depth as `tint_fill`: the
/// fill a little lighter at the top than at the bottom.
pub(super) fn raised_fill(fill: Hsla) -> Background {
    linear_gradient(
        180.,
        linear_color_stop(toward(fill, 1., 0.07), 0.),
        linear_color_stop(toward(fill, 0., 0.07), 1.),
    )
}

/// Returns the light of a neutral raised surface with the same slight depth as `tint_light`: the
/// theme highlight inside its top edge, a darker line inside its bottom edge and a tight shadow
/// below. The lines sit one point inside the edge, so a hairline border does not hide them.
pub(super) fn raised_light(theme: Theme) -> Vec<BoxShadow> {
    let line = |color: Hsla, y: f32| BoxShadow {
        inset: true,
        color,
        offset: point(px(0.), gpui::px(y)),
        blur_radius: px(0.),
        spread_radius: px(0.),
    };
    vec![
        line(theme.highlight, 2.),
        line(
            gpui::black().opacity(if theme.light { 0.06 } else { 0.28 }),
            -2.,
        ),
        BoxShadow {
            inset: false,
            color: theme.shadow,
            offset: point(px(0.), px(1.)),
            blur_radius: px(1.5),
            spread_radius: px(0.),
        },
    ]
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
        // A key stands a little: a light line under its top edge and a heavier bottom edge.
        .shadow(vec![
            BoxShadow {
                inset: true,
                color: gpui::white().opacity(if theme.light { 0.7 } else { 0.06 }),
                offset: point(px(0.), gpui::px(2.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
            },
            BoxShadow {
                inset: false,
                color: theme.edge,
                offset: point(px(0.), gpui::px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
            },
        ])
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
    .grid(2., 1.);
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
        .bg(tint_fill(tone, 0.08))
        .shadow(tint_light(tone))
        .layer(if animated { fill.animated() } else { fill })
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

/// Share of a glass surface's tone that covers the background field, and the blur of the
/// field that shows through, in points.
const GLASS_FILL: f32 = 0.25;
pub(super) const GLASS_BLUR: f32 = 6.;

/// Share of the tone of a glass surface that holds text over the conversation: the composer,
/// menus, dialogs and floating cards. Half as much shows through as through the side panels, so
/// the text reads clearly over what lies behind it.
pub(super) const SURFACE_GLASS_FILL: f32 = 0.625;

/// Share of its tone that a part of a glass surface takes, such as a dialog's header and
/// navigation, so that the glass shows through it without a blur of its own.
pub(super) const PANE_SHARE: f32 = 0.5;

/// Gives a surface glass in `tone`: the share `fill` of the tone over a blur of what lies
/// behind it. GPUI's renderers on macOS (Metal), Linux (wgpu) and Windows (DirectX) all draw the
/// blur, clipped to the surface's rounded rectangle.
pub(super) fn glass<E: Styled>(surface: E, tone: Hsla, fill: f32) -> E {
    surface.bg(tone.opacity(fill)).backdrop_blur(px(GLASS_BLUR))
}

/// Gives a panel at a side of the window its glass, in the panel tone.
pub(super) fn panel_glass<E: Styled>(panel: E, theme: Theme) -> E {
    glass(panel, theme.sidebar, GLASS_FILL)
}

/// The face of a floating menu or dialog: glass in `tone` with corners of `radius` points, a
/// hairline edge, the floating light and faint dots along its bottom edge. Place it in a
/// relatively positioned wrapper with the bounds of the surface, after its dithered shadow and
/// before its content, which then needs no fill of its own. Parts of the content that need a
/// fill take a share of their tone, so that the glass shows through them without a second blur.
pub(super) fn floating_face(tone: Hsla, radius: f32, theme: Theme) -> Div {
    glass(div(), tone, SURFACE_GLASS_FILL)
        .absolute()
        .inset_0()
        .rounded(px(radius))
        .border_1()
        .border_color(theme.edge)
        .shadow(theme.floating())
        .layer(dither::edge_shade(theme.dot, 0.18))
}

/// A card surface: raised fill, hairline edge, the light of a grounded plate and a dithered
/// shade along its bottom edge.
pub(super) fn card(theme: Theme) -> Div {
    div()
        .relative()
        .overflow_hidden()
        .rounded(px(crate::theme::RADIUS_SURFACE))
        .border_1()
        .border_color(theme.border)
        .bg(theme.raised)
        .shadow(plate_light(theme, true))
        .layer(dither::edge_shade(theme.dot, 0.22))
}

/// A content block: a recessed inset surface with a hairline edge for code, output, a diff or a
/// table.
pub(super) fn block(theme: Theme) -> Div {
    div()
        .w_full()
        .min_w_0()
        .rounded(px(crate::theme::RADIUS_CONTROL))
        .border_1()
        .border_color(theme.border)
        .bg(theme.inset)
        .shadow(recessed_light(theme))
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
