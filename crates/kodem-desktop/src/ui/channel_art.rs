//! The art behind the sidebar's end of the title bar and the badge of the build type. Each build
//! type has its own still scene of dots: a scope trace that stops on a breakpoint for Debug, a
//! night sky with a crescent moon for Nightly and a sunrise for Release. The motif of a scene
//! sits near the trailing end of the section, away from the wordmark, and only sparse dots lie
//! behind the text. A scene is baked once into a sprite, so it costs one draw per frame.
use super::*;
use crate::updater::{Channel, VERSION, version_label};
use dither::{DottedRule, Edge};
use std::hash::{Hash, Hasher};

/// Pitch of a scene's grid, in window pixels.
const CELL: f32 = 2.;
/// Size of a scene's dots, in window pixels.
const DOT: f32 = 1.;
/// Distance from the section's trailing edge to the center of a scene's motif, in window pixels.
const MOTIF: f32 = 52.;
/// Top and bottom of the band that holds the wordmark and the badge, in window pixels.
const TEXT_BAND: (f32, f32) = (10., 30.);
/// Height of the scope's baseline, in window pixels.
const SCOPE_BASE: f32 = 32.;

/// Returns the color of a build type: amber for Debug, blue for Nightly and the accent for
/// Release.
fn tone(channel: Channel, theme: Theme) -> Hsla {
    match channel {
        Channel::Debug => theme.warning,
        Channel::Nightly => theme.info,
        Channel::Production => theme.accent,
    }
}

/// Returns the glyph that stands for a build type in its badge.
fn glyph(channel: Channel) -> &'static str {
    match channel {
        Channel::Debug => "wrench",
        Channel::Nightly => "moon",
        Channel::Production => "sun",
    }
}

/// Returns the art of this build type for the sidebar's end of the title bar.
///
/// `width` is the width of the open section in window pixels. The art keeps that width while the
/// section slides, so its sprite is baked once. Add it as the first child of a relatively
/// positioned section that clips its content.
pub(super) fn titlebar_art(width: f32, theme: Theme) -> impl IntoElement {
    let channel = Channel::current();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let scene = Scene::new(
                channel,
                theme,
                bounds.size,
                Stage::title_bar(f32::from(bounds.size.width), f32::from(bounds.size.height)),
            );
            dither::paint_scene(
                bounds,
                CELL,
                DOT,
                scene.digest(),
                &|column, row| scene.dot(column, row),
                window,
                cx,
            );
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .h_full()
    .w(gpui::px(width))
}

/// Returns the scene of this build type for the version card of What's new: the title bar's
/// motif at twice its size in the card's trailing corner, with sparse dots behind the text. Add
/// it as the first child of the relatively positioned card, which clips it.
pub(super) fn card_art(theme: Theme) -> impl IntoElement {
    let channel = Channel::current();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let scene = Scene::new(
                channel,
                theme,
                bounds.size,
                Stage::card(f32::from(bounds.size.width), f32::from(bounds.size.height)),
            );
            dither::paint_scene(
                bounds,
                CELL,
                DOT,
                scene.digest(),
                &|column, row| scene.dot(column, row),
                window,
                cx,
            );
        },
    )
    .absolute()
    .inset_0()
}

/// Returns the color of this build type: amber for Debug, blue for Nightly and the accent for
/// Release.
pub(super) fn channel_tone(theme: Theme) -> Hsla {
    tone(Channel::current(), theme)
}

/// Returns the glyph of this build type: a wrench, a moon or a sun.
pub(super) fn channel_glyph() -> &'static str {
    glyph(Channel::current())
}

/// Returns the badge of this build type: a tinted chip with a plate that names the type beside
/// its glyph, a dotted rule and the version.
pub(super) fn channel_badge(theme: Theme) -> Div {
    let channel = Channel::current();
    let tone = tone(channel, theme);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(18.))
        .rounded(px(crate::theme::RADIUS_SMALL))
        .border_1()
        .border_color(tone.opacity(0.36))
        .bg(kit::tint_fill(tone, 0.08))
        .shadow(kit::tint_light(tone))
        .font_family(crate::theme::mono())
        .text_size(text_px(10.))
        .line_height(relative(1.))
        .child(
            div()
                .relative()
                .h_full()
                .flex()
                .items_center()
                .gap(px(4.))
                .pl(px(5.))
                .pr(px(6.))
                .rounded_l(px(crate::theme::RADIUS_SMALL - 1.))
                .bg(kit::tint_fill(tone, 0.12))
                .layer(kit::shade(tone.opacity(0.5), Edge::Bottom))
                .child(icon(glyph(channel)).size(px(10.)).text_color(tone))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .letter_spacing(text_px(crate::theme::LABEL_TRACKING))
                        .text_color(tone)
                        .child(channel.label().to_uppercase()),
                ),
        )
        .child(DottedRule::vertical(tone.opacity(0.45)))
        .child(
            div()
                .px(px(6.))
                .text_color(theme.muted)
                .child(version_label(VERSION)),
        )
}

/// Where a scene puts its motif and its text, in window pixels.
#[derive(Clone, Copy)]
struct Stage {
    /// Window pixels per unit of the motif's shapes: 1 in the title bar.
    unit: f32,
    /// The horizontal center of the motif.
    motif: f32,
    /// The height of the moon's center.
    moon: f32,
    /// The height of the sun's center, below the bottom edge.
    horizon: f32,
    /// The height of the scope's baseline.
    baseline: f32,
    /// The box of the text as left, top, right and bottom: only sparse dots lie in it.
    text: [f32; 4],
    /// The centers of the four-pointed sparkles.
    sparkles: [(f32, f32); 3],
    /// The length of a sparkle's arms, in cells.
    arm: i32,
}

impl Stage {
    // The sidebar's end of the title bar: the motif stands before the trailing edge, after the
    // wordmark and the badge.
    fn title_bar(width: f32, height: f32) -> Self {
        let motif = width - MOTIF;
        Self {
            unit: 1.,
            motif,
            moon: 15.,
            horizon: height + 3.,
            baseline: SCOPE_BASE,
            text: [f32::MIN, TEXT_BAND.0, motif - 14., TEXT_BAND.1],
            sparkles: [(motif - 40., 5.), (motif + 30., 35.), (150., 5.)],
            arm: 2,
        }
    }

    // The version card of What's new: the motif at twice its size in the trailing corner, and
    // the text in the middle.
    fn card(width: f32, height: f32) -> Self {
        let unit = 2.;
        let motif = width - MOTIF;
        Self {
            unit,
            motif,
            moon: 46.,
            horizon: height + 3. * unit,
            baseline: height - 14.,
            text: [width * 0.12, height * 0.14, width * 0.88, height - 16.],
            sparkles: [
                (width * 0.07, 24.),
                (motif - 70., 22.),
                (width * 0.95, height * 0.6),
            ],
            arm: 3,
        }
    }
}

/// The colors and size of one scene, in window pixels.
#[derive(Clone, Copy)]
struct Scene {
    channel: Channel,
    width: f32,
    height: f32,
    stage: Stage,
    /// The diffuse light of the scene: glows, nebulae, rays and haze.
    tone: Hsla,
    /// The lit core of the motif: the moon, the sun or the trace.
    bright: Hsla,
    /// Stars, and the breakpoint of the scope.
    spark: Hsla,
}

impl Scene {
    fn new(channel: Channel, theme: Theme, size: Size<Pixels>, stage: Stage) -> Self {
        let tone = tone(channel, theme);
        Self {
            channel,
            width: f32::from(size.width),
            height: f32::from(size.height),
            stage,
            tone,
            // The core is lighter than the glow on a dark surface and deeper on a light one.
            bright: if theme.light {
                kit::toward(tone, 0.3, 0.35)
            } else {
                kit::toward(tone, 0.92, 0.55)
            },
            spark: if channel == Channel::Debug {
                theme.removed
            } else {
                theme.text
            },
        }
    }

    // Digests everything that decides the scene's dots and colors.
    fn digest(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.channel.label().hash(&mut hasher);
        let stage = self.stage;
        let mut values = vec![
            self.width,
            self.height,
            stage.unit,
            stage.motif,
            stage.moon,
            stage.horizon,
            stage.baseline,
        ];
        values.extend(stage.text);
        values.extend(stage.sparkles.iter().flat_map(|(x, y)| [*x, *y]));
        for value in values {
            value.to_bits().hash(&mut hasher);
        }
        stage.arm.hash(&mut hasher);
        for color in [self.tone, self.bright, self.spark] {
            let rgba = hsla_to_rgba(color);
            for value in [rgba.red, rgba.green, rgba.blue, rgba.alpha] {
                value.to_bits().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    // Returns the color of one lit cell of the grid.
    fn dot(&self, column: i32, row: i32) -> Option<Hsla> {
        let x = (column as f32 + 0.5) * CELL;
        let y = (row as f32 + 0.5) * CELL;
        match self.channel {
            Channel::Debug => self.scope(x, y, column, row),
            Channel::Nightly => self.sky(x, y, column, row),
            Channel::Production => self.sunrise(x, y, column, row),
        }
    }

    // Returns whether a point lies in the box of the text.
    fn behind_text(&self, x: f32, y: f32) -> bool {
        let [left, top, right, bottom] = self.stage.text;
        x > left && x < right && y > top && y < bottom
    }

    // Dithers a diffuse density in the tone, with fewer dots behind the text.
    fn glow(&self, density: f32, x: f32, y: f32, column: i32, row: i32) -> Option<Hsla> {
        let density = if self.behind_text(x, y) {
            density * 0.3
        } else {
            density
        };
        (density > dither::cell_threshold(column, row))
            .then(|| self.tone.opacity(dither::tone(density) * 0.7))
    }

    // Returns a star: a few four-pointed sparkles at fixed places and single dots at random cells
    // away from the text.
    fn star(&self, x: f32, y: f32, column: i32, row: i32) -> Option<Hsla> {
        for (sparkle_x, sparkle_y) in self.stage.sparkles {
            let across = (column - (sparkle_x / CELL) as i32).abs();
            let down = (row - (sparkle_y / CELL) as i32).abs();
            if across.min(down) == 0 && across.max(down) <= self.stage.arm {
                return Some(self.spark.opacity(match across + down {
                    0 => 1.,
                    1 => 0.7,
                    _ => 0.35,
                }));
            }
        }
        if self.behind_text(x, y) || dither::hash(column, row, 5) < 0.985 {
            return None;
        }
        let bright = dither::hash(column, row, 6) > 0.7;
        Some(self.spark.opacity(if bright { 0.9 } else { 0.45 }))
    }

    // Nightly: a crescent moon with a halo, a drifting nebula and stars.
    fn sky(&self, x: f32, y: f32, column: i32, row: i32) -> Option<Hsla> {
        let unit = self.stage.unit;
        let (moon_x, moon_y, radius) = (self.stage.motif, self.stage.moon, 8. * unit);
        let moon = distance(x, y, moon_x, moon_y);
        if moon <= radius {
            // The crescent is the disc less a disc beside it; the dark side keeps a faint shine.
            if distance(x, y, moon_x + 3.5 * unit, moon_y - 2.5 * unit) > radius - unit {
                return Some(self.bright);
            }
            return self.glow(0.2, x, y, column, row);
        }
        if let Some(star) = self.star(x, y, column, row) {
            return Some(star);
        }
        let halo = 0.65 * (1. - (moon - radius) / (20. * unit)).max(0.).powf(1.6);
        let cloud = dither::smoothstep(
            0.45,
            0.8,
            dither::fractal_noise(x * 0.03 / unit, y * 0.08 / unit, 21),
        );
        let nebula = cloud
            * 0.8
            * ellipse(
                x,
                y,
                moon_x - 50. * unit,
                moon_y - 3. * unit,
                190. * unit,
                40. * unit,
            );
        self.glow(halo.max(nebula), x, y, column, row)
    }

    // Release: a striped sun on the horizon with a corona, rays and banks of haze.
    fn sunrise(&self, x: f32, y: f32, column: i32, row: i32) -> Option<Hsla> {
        let unit = self.stage.unit;
        let (sun_x, sun_y, radius) = (self.stage.motif, self.stage.horizon, 15. * unit);
        let sun = distance(x, y, sun_x, sun_y);
        let top = sun_y - radius;
        if sun <= radius {
            // Every other row of the lower half is cut out, as on a retro sunset.
            let band = ((y - top) / (CELL * unit)) as i32;
            if band >= 3 && band % 2 == 1 {
                return None;
            }
            let depth = ((y - top) / (self.height - top)).clamp(0., 1.);
            return Some(kit::toward(self.bright, self.tone.lightness, depth));
        }
        let corona = 0.85 * (1. - (sun - radius) / (30. * unit)).max(0.).powf(1.5);
        let angle = (x - sun_x).atan2(sun_y - y);
        let rays = 0.55
            * dither::smoothstep(0.6, 0.95, (angle * 16.).cos())
            * dither::smoothstep(radius + 2. * unit, radius + 8. * unit, sun)
            * (1. - (sun - radius) / (120. * unit)).max(0.);
        let horizon = 0.5
            * dither::smoothstep(self.height - 10. * unit, self.height, y)
            * (1. - (x - sun_x).abs() / (200. * unit)).max(0.);
        let strata =
            0.5 * dither::smoothstep(
                0.55,
                0.8,
                dither::fractal_noise(x * 0.02 / unit, y * 0.22 / unit, 9),
            ) * ellipse(x, y, sun_x - 30. * unit, sun_y, 220. * unit, 50. * unit);
        self.glow(corona.max(rays).max(horizon).max(strata), x, y, column, row)
    }

    // Returns the height of the scope's trace: flat at the wordmark, then a low wave that rises
    // toward the motif and ends in a spike and a short dip, like a beat on a monitor.
    fn trace(&self, x: f32) -> f32 {
        let (unit, motif) = (self.stage.unit, self.stage.motif);
        let wave = self.rise(x) * 3.5 * unit * (x * 0.2 / unit).sin();
        let spike = 13. * unit * (-((x - motif) / (2.5 * unit)).powi(2)).exp();
        let dip = 5. * unit * (-((x - motif - 5. * unit) / (2. * unit)).powi(2)).exp();
        self.stage.baseline - wave - spike + dip
    }

    // Returns how far the scope's wave has risen at `x`: none before the motif's reach, full near
    // it, and less again after it.
    fn rise(&self, x: f32) -> f32 {
        let (unit, motif) = (self.stage.unit, self.stage.motif);
        dither::smoothstep(motif - 110. * unit, motif - 20. * unit, x)
            * (1. - 0.6 * dither::smoothstep(motif, self.width, x))
    }

    // Debug: an oscilloscope trace on a ruled baseline that a breakpoint stops at its spike.
    fn scope(&self, x: f32, y: f32, column: i32, row: i32) -> Option<Hsla> {
        let (unit, motif, baseline) = (self.stage.unit, self.stage.motif, self.stage.baseline);
        let breakpoint = distance(x, y, motif, self.trace(motif));
        if breakpoint <= 3. * unit {
            return Some(self.spark);
        }
        if breakpoint <= 5. * unit {
            return None;
        }
        let (left, right) = (self.trace(x - CELL / 2.), self.trace(x + CELL / 2.));
        if y >= left.min(right) - 0.9 * unit && y <= left.max(right) + 0.9 * unit {
            let beam = 0.35 + 0.65 * dither::smoothstep(80. * unit, motif, x);
            return Some(self.bright.opacity(beam));
        }
        if column == (motif / CELL) as i32 && row % 2 == 0 {
            return Some(self.spark.opacity(0.35));
        }
        if row == (baseline / CELL) as i32 && column % 2 == 0 {
            return Some(self.tone.opacity(0.22));
        }
        if column % (8. * unit) as i32 == 0 && y > baseline + 2. * unit {
            return Some(self.tone.opacity(0.3));
        }
        let phosphor = 0.55 * (-(y - self.trace(x)).abs() / (3.5 * unit)).exp() * self.rise(x);
        let haze = 0.4
            * (1. - distance(x, y, motif, baseline - 12. * unit) / (70. * unit))
                .max(0.)
                .powf(1.5);
        self.glow(phosphor.max(haze), x, y, column, row)
    }
}

fn distance(x: f32, y: f32, center_x: f32, center_y: f32) -> f32 {
    ((x - center_x).powi(2) + (y - center_y).powi(2)).sqrt()
}

// Returns 1 at the center of an ellipse with radii `width` and `height`, falling to 0 at its rim.
fn ellipse(x: f32, y: f32, center_x: f32, center_y: f32, width: f32, height: f32) -> f32 {
    let (across, down) = ((x - center_x) / width, (y - center_y) / height);
    (1. - (across * across + down * down).sqrt()).max(0.)
}
