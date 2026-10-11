//! Short motion effects drawn above the whole window: chips that fly between parts of the UI,
//! dithered shockwaves where they land, button ripples, and rows that crumble into falling
//! pixels.
//!
//! Views mark landing spots with [`anchor`], then start effects with the functions below. One
//! [`FxLayer`] paints every running effect, so only that layer renders on each animation frame.
//! Nothing runs while reduced motion is on.
use super::*;
use std::{cell::RefCell, f32::consts::TAU, rc::Rc, time::Instant};

/// Share of a flight spent popping up in place before it moves.
const POP_SHARE: f32 = 0.16;
/// Comet tail samples drawn behind a flying chip.
const TRAIL_SAMPLES: usize = 9;
/// Progress gap between two tail samples.
const TRAIL_SPACING: f32 = 0.035;
const CHIP_HEIGHT: f32 = 22.0;
/// Room a flying chip needs between the middle of its arc and a window
/// edge to stay fully in view.
const ARC_EDGE_MARGIN: f32 = CHIP_HEIGHT / 2.0;
const CHIP_TEXT_SIZE: f32 = 11.0;
const CHIP_MAX_CHARS: usize = 32;
const IMPACT_DURATION: Duration = Duration::from_millis(560);
const SOFT_LANDING_DURATION: Duration = Duration::from_millis(320);
/// Comet tail samples drawn behind a gliding chip.
const GLIDE_TRAIL_SAMPLES: usize = 3;
const RIPPLE_DURATION: Duration = Duration::from_millis(340);
const CRUMBLE_DURATION: Duration = Duration::from_millis(950);
/// Most cells a crumbling area breaks into.
const CRUMBLE_MAX_CELLS: f32 = 900.0;
/// Sparks thrown out by one impact.
const SPARK_COUNT: usize = 14;
/// Downward pull on sparks and crumbs, in pixels per second squared.
const GRAVITY: f32 = 1100.0;
/// Grid pitch of dithered fields, matching the rest of the UI.
const FIELD_PITCH: f32 = 2.0;

#[derive(Default)]
struct FxState {
    anchors: HashMap<SharedString, Bounds<Pixels>>,
    /// Anchors painted in the frame before the current one. An anchor that is in neither map
    /// left the screen, so effects do not land where it used to be.
    previous: HashMap<SharedString, Bounds<Pixels>>,
    effects: Vec<Effect>,
    layer: Option<WeakEntity<FxLayer>>,
    /// The window that shows the layer. Effects started in other windows,
    /// such as settings, are skipped because they would land in the wrong
    /// place.
    window: Option<AnyWindowHandle>,
    seed: u32,
}

impl FxState {
    fn bounds(&self, name: &str) -> Option<Bounds<Pixels>> {
        self.anchors
            .get(name)
            .or_else(|| self.previous.get(name))
            .copied()
    }
}

#[derive(Default)]
struct Fx(Rc<RefCell<FxState>>);

impl Global for Fx {}

/// Shared effect state. Reads go through the `RefCell`, so updating
/// anchors on every frame never notifies global observers.
fn shared(cx: &mut App) -> Rc<RefCell<FxState>> {
    if !cx.has_global::<Fx>() {
        cx.set_global(Fx::default());
    }
    cx.global::<Fx>().0.clone()
}

#[derive(Clone)]
struct Effect {
    start: Instant,
    duration: Duration,
    seed: u32,
    kind: EffectKind,
}

#[derive(Clone)]
enum EffectKind {
    Flight(Flight),
    Impact {
        bounds: Bounds<Pixels>,
        tone: Hsla,
    },
    /// The quiet landing of a glide: the target glows briefly.
    SoftLanding {
        bounds: Bounds<Pixels>,
        tone: Hsla,
    },
    Ripple {
        at: Point<Pixels>,
        tone: Hsla,
    },
    Crumble {
        bounds: Bounds<Pixels>,
        tone: Hsla,
    },
}

#[derive(Clone)]
struct Flight {
    from: Point<Pixels>,
    /// Where the chip lands when its anchor is gone.
    to: Point<Pixels>,
    /// The anchor the chip homes in on and hits when it lands.
    target: Option<SharedString>,
    label: SharedString,
    tone: Hsla,
    style: FlightStyle,
}

/// How lively a flight is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlightStyle {
    /// Pops up, arcs high, stretches, and lands with an impact. For rare,
    /// meaningful moments such as a commit or a push.
    Throw,
    /// A small, quick, low hop that lands with a soft glow. For frequent
    /// actions such as staging a file.
    Glide,
}

impl Effect {
    /// Progress from `0.0` to `1.0`, or a negative value before a delayed
    /// effect starts.
    fn progress(&self, now: Instant) -> f32 {
        if now < self.start {
            return -1.0;
        }
        let elapsed = now.duration_since(self.start).as_secs_f32();
        (elapsed / self.duration.as_secs_f32()).min(1.0)
    }
}

/// An invisible element that records its parent's bounds under `name`
/// on every frame, so effects can fly to it. The parent must be
/// positioned (`relative`) for the bounds to match it.
pub(super) fn anchor(name: impl Into<SharedString>) -> impl IntoElement {
    let name = name.into();
    canvas(
        move |bounds, _, cx| {
            shared(cx).borrow_mut().anchors.insert(name, bounds);
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The bounds `name` had on the last frame it was painted.
pub(super) fn anchor_bounds(name: &str, cx: &mut App) -> Option<Bounds<Pixels>> {
    shared(cx).borrow().bounds(name)
}

/// Starts a frame of anchor recording. Call it once before the window's content renders.
pub(super) fn begin_frame(cx: &mut App) {
    let state = shared(cx);
    let mut state = state.borrow_mut();
    state.previous = std::mem::take(&mut state.anchors);
}

/// Throws a chip with `label` from `from` to the `target` anchor, where
/// it lands with an impact. `delay` staggers several chips.
pub(super) fn fly_to(
    from: Point<Pixels>,
    target: &str,
    label: impl Into<SharedString>,
    tone: Hsla,
    delay: Duration,
    cx: &mut App,
) {
    let Some(to) = anchor_bounds(target, cx).map(|bounds| bounds.center()) else {
        return;
    };
    let flight = Flight {
        from,
        to,
        target: Some(target.into()),
        label: label.into(),
        tone,
        style: FlightStyle::Throw,
    };
    launch_flight(flight, delay, cx);
}

/// Moves a small chip with `label` in a quick, low hop from `from` to
/// the `target` anchor, where it lands with a soft glow. Made for
/// frequent actions, so it stays out of the way. The target may only
/// appear on the next frame, such as a list section that the same click
/// creates; until it is painted, the chip heads for `fallback`.
pub(super) fn glide_to(
    from: Point<Pixels>,
    target: &str,
    fallback: Point<Pixels>,
    label: impl Into<SharedString>,
    tone: Hsla,
    cx: &mut App,
) {
    let to = anchor_bounds(target, cx).map_or(fallback, |bounds| bounds.center());
    let flight = Flight {
        from,
        to,
        target: Some(target.into()),
        label: label.into(),
        tone,
        style: FlightStyle::Glide,
    };
    launch_flight(flight, Duration::ZERO, cx);
}

/// Throws a chip from the center of the `source` anchor to the `target`
/// anchor.
pub(super) fn fly_between(
    source: &str,
    target: &str,
    label: impl Into<SharedString>,
    tone: Hsla,
    delay: Duration,
    cx: &mut App,
) {
    let Some(from) = anchor_bounds(source, cx).map(|bounds| bounds.center()) else {
        return;
    };
    fly_to(from, target, label, tone, delay, cx);
}

/// A flash, an expanding frame, a dithered shockwave, and sparks around
/// the `target` anchor.
pub(super) fn impact(target: &str, tone: Hsla, cx: &mut App) {
    if let Some(bounds) = anchor_bounds(target, cx) {
        push(
            EffectKind::Impact { bounds, tone },
            IMPACT_DURATION,
            Duration::ZERO,
            cx,
        );
    }
}

/// A small dithered ring that spreads from a click.
pub(super) fn ripple(at: Point<Pixels>, tone: Hsla, cx: &mut App) {
    push(
        EffectKind::Ripple { at, tone },
        RIPPLE_DURATION,
        Duration::ZERO,
        cx,
    );
}

/// Breaks an area into pixels that hop and fall away, sweeping from left
/// to right. Used when content is discarded.
pub(super) fn crumble(bounds: Bounds<Pixels>, tone: Hsla, cx: &mut App) {
    push(
        EffectKind::Crumble { bounds, tone },
        CRUMBLE_DURATION,
        Duration::ZERO,
        cx,
    );
}

fn launch_flight(mut flight: Flight, delay: Duration, cx: &mut App) {
    let distance = (flight.to - flight.from).magnitude() as f32;
    let duration = flight_duration(distance, flight.style);
    flight.label = chip_label(&flight.label);
    push(EffectKind::Flight(flight), duration, delay, cx);
}

fn push(kind: EffectKind, duration: Duration, delay: Duration, cx: &mut App) {
    if cx.reduce_motion() {
        return;
    }
    let state = shared(cx);
    let active_window = cx.active_window();
    let layer = {
        let mut state = state.borrow_mut();
        if active_window.is_some() && state.window.is_some() && active_window != state.window {
            return;
        }
        state.seed = state.seed.wrapping_add(1);
        let seed = state.seed;
        state.effects.push(Effect {
            start: Instant::now() + delay,
            duration,
            seed,
            kind,
        });
        state.layer.clone()
    };
    if let Some(layer) = layer {
        layer.update(cx, |_, cx| cx.notify()).ok();
    }
}

/// Longer flights take longer, within limits that keep them snappy.
/// Glides are much quicker than throws.
fn flight_duration(distance: f32, style: FlightStyle) -> Duration {
    let millis = match style {
        FlightStyle::Throw => (380.0 + distance * 0.45).clamp(480.0, 900.0),
        FlightStyle::Glide => (200.0 + distance * 0.3).clamp(260.0, 420.0),
    };
    Duration::from_millis(millis as u64)
}

/// Shortens long labels so chips stay small.
fn chip_label(label: &str) -> SharedString {
    let label = label.lines().next().unwrap_or_default().trim();
    if label.chars().count() <= CHIP_MAX_CHARS {
        return SharedString::from(label.to_string());
    }
    let short: String = label.chars().take(CHIP_MAX_CHARS - 1).collect();
    SharedString::from(format!("{}…", short.trim_end()))
}

/// Paints every running effect. Add one to the window root, above the
/// content; it never takes mouse input. Keep it outside the content view, so
/// that a frame of an effect does not render the content again.
pub(super) struct FxLayer;

impl FxLayer {
    /// Creates the layer and registers it to be notified when effects
    /// start.
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        shared(cx).borrow_mut().layer = Some(cx.entity().downgrade());
        Self
    }
}

/// Theme colors used while painting effects.
#[derive(Clone, Copy)]
struct FxColors {
    surface: Hsla,
    text: Hsla,
}

impl Render for FxLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = shared(cx);
        let now = Instant::now();
        let (effects, anchors) = {
            let mut state = state.borrow_mut();
            state.window = Some(window.window_handle());
            let mut landed = Vec::new();
            state.effects.retain(|effect| {
                let done = effect.progress(now) >= 1.0;
                if done && let EffectKind::Flight(flight) = &effect.kind {
                    landed.push(flight.clone());
                }
                !done
            });
            for flight in landed {
                let Some(target) = flight.target else {
                    continue;
                };
                let bounds = state
                    .bounds(&target)
                    .unwrap_or_else(|| Bounds::centered_at(flight.to, size(px(24.), px(24.))));
                state.seed = state.seed.wrapping_add(1);
                let seed = state.seed;
                let (duration, kind) = match flight.style {
                    FlightStyle::Throw => (
                        IMPACT_DURATION,
                        EffectKind::Impact {
                            bounds,
                            tone: flight.tone,
                        },
                    ),
                    FlightStyle::Glide => (
                        SOFT_LANDING_DURATION,
                        EffectKind::SoftLanding {
                            bounds,
                            tone: flight.tone,
                        },
                    ),
                };
                state.effects.push(Effect {
                    start: now,
                    duration,
                    seed,
                    kind,
                });
            }
            let mut anchors = state.previous.clone();
            anchors.extend(state.anchors.clone());
            (state.effects.clone(), anchors)
        };
        let theme = cx
            .try_global::<Theme>()
            .copied()
            .unwrap_or(Theme::new(false));
        let colors = FxColors {
            surface: theme.raised,
            text: theme.text,
        };

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .when(!effects.is_empty(), |layer| {
                layer.child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, cx| {
                            let now = Instant::now();
                            // One layer for every effect: thousands of dots then share a single
                            // draw order instead of each being sorted against the scene.
                            let viewport = Bounds::new(Point::default(), window.viewport_size());
                            window.paint_layer(viewport, |window| {
                                for effect in &effects {
                                    paint_effect(effect, now, &anchors, colors, window, cx);
                                }
                            });
                            window.request_animation_frame();
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            })
    }
}

fn paint_effect(
    effect: &Effect,
    now: Instant,
    anchors: &HashMap<SharedString, Bounds<Pixels>>,
    colors: FxColors,
    window: &mut Window,
    cx: &mut App,
) {
    let progress = effect.progress(now);
    if progress < 0.0 {
        return;
    }
    match &effect.kind {
        EffectKind::Flight(flight) => {
            let to = flight
                .target
                .as_ref()
                .and_then(|target| anchors.get(target))
                .map_or(flight.to, |bounds| bounds.center());
            paint_flight(flight, to, progress, colors, window, cx);
        }
        EffectKind::Impact { bounds, tone } => {
            paint_impact(*bounds, *tone, progress, effect.seed, window);
        }
        EffectKind::SoftLanding { bounds, tone } => {
            paint_soft_landing(*bounds, *tone, progress, window);
        }
        EffectKind::Ripple { at, tone } => paint_ripple(*at, *tone, progress, window),
        EffectKind::Crumble { bounds, tone } => {
            let elapsed = progress * effect.duration.as_secs_f32();
            paint_crumble(*bounds, *tone, elapsed, effect.seed, window);
        }
    }
}

/// Where a flight is at `progress`, and how it is squashed: `(center,
/// scale_x, scale_y)`. The chip pops up in place, then accelerates along
/// an arc and shrinks as it is pulled into the target, stretching along
/// its direction of travel. The arc bends whichever way keeps it inside
/// a window `window_height` tall.
fn flight_pose(
    from: (f32, f32),
    to: (f32, f32),
    progress: f32,
    style: FlightStyle,
    window_height: f32,
) -> ((f32, f32), f32, f32) {
    if style == FlightStyle::Glide {
        return glide_pose(from, to, progress, window_height);
    }
    if progress < POP_SHARE {
        let scale = 0.35 + 0.65 * ease_out_back(progress / POP_SHARE);
        return (from, scale, scale);
    }
    let travel = (progress - POP_SHARE) / (1.0 - POP_SHARE);
    let along = travel * travel;
    let lift = (flight_distance(from, to) * 0.35).clamp(40.0, 160.0);
    let control = arc_control(from, to, lift, window_height);
    let center = quadratic(from, control, to, along);
    let scale = 1.0 - 0.45 * along;
    let speed = travel;
    let direction = quadratic_tangent(from, control, to, along);
    let stretch = 1.0 + 0.3 * speed;
    let squash = 1.0 - 0.18 * speed;
    if direction.0.abs() >= direction.1.abs() {
        (center, scale * stretch, scale * squash)
    } else {
        (center, scale * squash, scale * stretch)
    }
}

/// A glide eases in and out along a shallow arc at a steady small size,
/// with no pop and no stretch.
fn glide_pose(
    from: (f32, f32),
    to: (f32, f32),
    progress: f32,
    window_height: f32,
) -> ((f32, f32), f32, f32) {
    let along = ease_in_out_sine(progress);
    let lift = (flight_distance(from, to) * 0.12).clamp(8.0, 28.0);
    let control = arc_control(from, to, lift, window_height);
    let scale = 0.85 - 0.2 * along;
    (quadratic(from, control, to, along), scale, scale)
}

fn flight_distance(from: (f32, f32), to: (f32, f32)) -> f32 {
    ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt()
}

/// The control point that bends a flight into an arc, `lift` pixels
/// beyond its ends. The arc rises above both ends, unless that carries
/// the chip past the top of a window `window_height` tall; then it dips
/// below them instead, as long as that keeps more of it in view. A
/// flight that starts or ends outside the window is meant to cross its
/// edge, so it always rises.
fn arc_control(from: (f32, f32), to: (f32, f32), lift: f32, window_height: f32) -> (f32, f32) {
    let middle = (from.0 + to.0) / 2.0;
    let rising = from.1.min(to.1) - lift;
    let crosses_edge = from.1.min(to.1) < 0.0 || from.1.max(to.1) > window_height;
    let above_top = ARC_EDGE_MARGIN - quadratic_peak(from.1, rising, to.1);
    if crosses_edge || above_top <= 0.0 {
        return (middle, rising);
    }
    let dipping = from.1.max(to.1) + lift;
    let below_bottom = quadratic_peak(from.1, dipping, to.1) - (window_height - ARC_EDGE_MARGIN);
    if below_bottom < above_top {
        (middle, dipping)
    } else {
        (middle, rising)
    }
}

/// The farthest a quadratic curve from `a` to `c` reaches toward its
/// control `b`, which must lie beyond both ends.
fn quadratic_peak(a: f32, b: f32, c: f32) -> f32 {
    let t = (a - b) / (a - 2.0 * b + c);
    let u = 1.0 - t;
    u * u * a + 2.0 * u * t * b + t * t * c
}

fn quadratic(a: (f32, f32), b: (f32, f32), c: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    (
        u * u * a.0 + 2.0 * u * t * b.0 + t * t * c.0,
        u * u * a.1 + 2.0 * u * t * b.1 + t * t * c.1,
    )
}

fn quadratic_tangent(a: (f32, f32), b: (f32, f32), c: (f32, f32), t: f32) -> (f32, f32) {
    (
        2.0 * (1.0 - t) * (b.0 - a.0) + 2.0 * t * (c.0 - b.0),
        2.0 * (1.0 - t) * (b.1 - a.1) + 2.0 * t * (c.1 - b.1),
    )
}

fn paint_flight(
    flight: &Flight,
    to: Point<Pixels>,
    progress: f32,
    colors: FxColors,
    window: &mut Window,
    cx: &mut App,
) {
    let from = (f32::from(flight.from.x), f32::from(flight.from.y));
    let to = (f32::from(to.x), f32::from(to.y));
    let window_height = f32::from(window.viewport_size().height);
    let tone = flight.tone;

    // Comet tail: fading dither puffs at earlier points of the arc.
    let (samples, start, peak) = match flight.style {
        FlightStyle::Throw => (TRAIL_SAMPLES, POP_SHARE, 0.85),
        FlightStyle::Glide => (GLIDE_TRAIL_SAMPLES, 0.0, 0.45),
    };
    for sample in 1..=samples {
        let earlier = progress - sample as f32 * TRAIL_SPACING;
        if earlier <= start {
            break;
        }
        let ((x, y), scale, _) = flight_pose(from, to, earlier, flight.style, window_height);
        let fade = 1.0 - sample as f32 / (samples + 1) as f32;
        let radius = (2.0 + 6.0 * fade) * scale;
        let strength = peak * fade;
        paint_field(
            window,
            (x - radius, y - radius, x + radius, y + radius),
            tone.opacity(0.9),
            |px, py| {
                let distance = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
                strength * (1.0 - distance / radius).max(0.0)
            },
        );
    }

    let ((x, y), scale_x, scale_y) = flight_pose(from, to, progress, flight.style, window_height);
    let text_size = (CHIP_TEXT_SIZE * scale_x.min(scale_y)).max(5.0);
    let line = window.text_system().shape_line(
        flight.label.clone(),
        px(text_size),
        &[TextRun {
            len: flight.label.len(),
            font: font(crate::theme::MONO),
            color: colors.text,
            background_color: None,
            underline: None,
            strikethrough: None,
            letter_spacing: None,
        }],
        None,
    );
    let padding = 8.0 * scale_x;
    let bullet = 5.0 * scale_y.min(scale_x);
    let gap = 6.0 * scale_x;
    let width = f32::from(line.width) + padding * 2.0 + bullet + gap;
    let height = CHIP_HEIGHT * scale_y;
    let chip = Bounds::new(
        point(px(x - width / 2.0), px(y - height / 2.0)),
        size(px(width), px(height)),
    );
    window.paint_quad(
        fill(chip, colors.surface)
            .corner_radii(px(6.0 * scale_y))
            .border_widths(px(1.))
            .border_color(tone),
    );
    window.paint_quad(fill(chip, tone.opacity(0.12)).corner_radii(px(6.0 * scale_y)));
    let bullet_bounds = Bounds::new(
        point(px(x - width / 2.0 + padding), px(y - bullet / 2.0)),
        size(px(bullet), px(bullet)),
    );
    window.paint_quad(fill(bullet_bounds, tone).corner_radii(px(1.)));
    let text_origin = point(
        px(x - width / 2.0 + padding + bullet + gap),
        px(y - height / 2.0),
    );
    line.paint(text_origin, px(height), TextAlign::Left, None, window, cx)
        .ok();
}

fn paint_impact(bounds: Bounds<Pixels>, tone: Hsla, progress: f32, seed: u32, window: &mut Window) {
    let fade = 1.0 - progress;
    let eased = ease_out_cubic(progress);
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let right = left + f32::from(bounds.size.width);
    let bottom = top + f32::from(bounds.size.height);

    // The target lights up and an outline kicks outward from it.
    window.paint_quad(
        fill(bounds, tone.opacity(0.24 * fade * fade))
            .corner_radii(px(6.))
            .border_widths(px(1.))
            .border_color(tone.opacity(0.9 * fade)),
    );
    let spread = 2.0 + 10.0 * eased;
    window.paint_quad(
        fill(
            Bounds::new(
                point(px(left - spread), px(top - spread)),
                size(
                    px(right - left + spread * 2.0),
                    px(bottom - top + spread * 2.0),
                ),
            ),
            gpui::transparent_black(),
        )
        .corner_radii(px(6.0 + spread))
        .border_widths(px(1.))
        .border_color(tone.opacity(0.55 * fade)),
    );

    // A dithered shockwave that follows the target's outline.
    let wave = 4.0 + 44.0 * eased;
    let thickness = 2.0 + 6.0 * fade;
    let reach = wave + thickness;
    paint_field(
        window,
        (left - reach, top - reach, right + reach, bottom + reach),
        tone,
        |x, y| {
            let dx = (left - x).max(x - right).max(0.0);
            let dy = (top - y).max(y - bottom).max(0.0);
            let distance = (dx * dx + dy * dy).sqrt();
            0.95 * fade * (1.0 - (distance - wave).abs() / thickness).max(0.0)
        },
    );

    // Sparks fly out from the edge and fall, lighter than crumbs.
    let center = bounds.center();
    let (center_x, center_y) = (f32::from(center.x), f32::from(center.y));
    let (half_w, half_h) = ((right - left) / 2.0, (bottom - top) / 2.0);
    let time = progress * IMPACT_DURATION.as_secs_f32();
    for spark in 0..SPARK_COUNT {
        let jitter = hash(spark as i32, 0, seed);
        let angle = spark as f32 / SPARK_COUNT as f32 * TAU + (jitter - 0.5) * 0.5;
        let speed = 140.0 + 140.0 * hash(spark as i32, 1, seed);
        let (dir_x, dir_y) = (angle.cos(), angle.sin());
        let x = center_x + dir_x * half_w + dir_x * speed * time;
        let y = center_y + dir_y * half_h + dir_y * speed * time + 0.25 * GRAVITY * time * time;
        let side = (3.0 * fade + 1.0).round();
        let dot = Bounds::new(
            point(px(x.round()), px(y.round())),
            size(px(side), px(side)),
        );
        window.paint_quad(fill(dot, tone.opacity(fade)));
    }
}

fn paint_soft_landing(bounds: Bounds<Pixels>, tone: Hsla, progress: f32, window: &mut Window) {
    let fade = 1.0 - ease_out_cubic(progress);
    window.paint_quad(
        fill(bounds, tone.opacity(0.12 * fade))
            .corner_radii(px(6.))
            .border_widths(px(1.))
            .border_color(tone.opacity(0.45 * fade)),
    );
}

fn paint_ripple(at: Point<Pixels>, tone: Hsla, progress: f32, window: &mut Window) {
    let (x, y) = (f32::from(at.x), f32::from(at.y));
    let fade = 1.0 - progress;
    let radius = 3.0 + 24.0 * ease_out_cubic(progress);
    let thickness = 4.0;
    let reach = radius + thickness;
    paint_field(
        window,
        (x - reach, y - reach, x + reach, y + reach),
        tone,
        |px, py| {
            let distance = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
            0.8 * fade * (1.0 - (distance - radius).abs() / thickness).max(0.0)
        },
    );
}

fn paint_crumble(bounds: Bounds<Pixels>, tone: Hsla, elapsed: f32, seed: u32, window: &mut Window) {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let cell = (width * height / CRUMBLE_MAX_CELLS).sqrt().ceil().max(3.0);
    let columns = (width / cell).ceil() as i32;
    let rows = (height / cell).ceil() as i32;
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let life = 0.55;
    for row in 0..rows {
        for column in 0..columns {
            let x = left + column as f32 * cell;
            let y = top + row as f32 * cell;
            let sweep = column as f32 / columns.max(1) as f32;
            let start = 0.24 * (0.75 * sweep + 0.25 * hash(column, row, seed));
            let age = elapsed - start;
            if age < 0.0 {
                // Not broken off yet: a tinted ghost of the removed content.
                let ghost = Bounds::new(point(px(x), px(y)), size(px(cell - 1.0), px(cell - 1.0)));
                window.paint_quad(fill(ghost, tone.opacity(0.35)));
                continue;
            }
            let remaining = 1.0 - age / life;
            if remaining <= 0.0 || remaining <= dither::cell_threshold(column, row) {
                continue;
            }
            let drift = (hash(column, row, seed ^ 0x5bd1) - 0.5) * 90.0;
            let hop = 40.0 + 70.0 * hash(column, row, seed ^ 0x9e37);
            let dx = drift * age;
            let dy = -hop * age + 0.5 * GRAVITY * age * age;
            let crumb = Bounds::new(
                point(px((x + dx).round()), px((y + dy).round())),
                size(px(cell - 1.0), px(cell - 1.0)),
            );
            window.paint_quad(fill(crumb, tone.opacity(0.9)));
        }
    }
}

/// Paints a dithered field over `(left, top, right, bottom)`: a dot on
/// each 2px grid cell whose `intensity` beats its Bayer threshold. The
/// grid is fixed to the window, so moving fields shimmer through it like
/// on a low-resolution display.
fn paint_field(
    window: &mut Window,
    (left, top, right, bottom): (f32, f32, f32, f32),
    color: Hsla,
    intensity: impl Fn(f32, f32) -> f32,
) {
    let first_column = (left / FIELD_PITCH).floor() as i32;
    let last_column = (right / FIELD_PITCH).ceil() as i32;
    let first_row = (top / FIELD_PITCH).floor() as i32;
    let last_row = (bottom / FIELD_PITCH).ceil() as i32;
    for row in first_row..last_row {
        let y = row as f32 * FIELD_PITCH;
        for column in first_column..last_column {
            let x = column as f32 * FIELD_PITCH;
            let value = intensity(x + FIELD_PITCH / 2.0, y + FIELD_PITCH / 2.0);
            if value <= dither::cell_threshold(column, row) {
                continue;
            }
            let dot = Bounds::new(point(px(x), px(y)), size(px(1.), px(1.)));
            window.paint_quad(fill(dot, color));
        }
    }
}

/// Overshoots past `1.0`, then settles, like a spring.
pub(super) fn ease_out_back(t: f32) -> f32 {
    let overshoot = 1.70158;
    let t = t - 1.0;
    1.0 + (overshoot + 1.0) * t * t * t + overshoot * t * t
}

fn ease_in_out_sine(t: f32) -> f32 {
    0.5 - 0.5 * (t * std::f32::consts::PI).cos()
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// A stable pseudo-random value in `0.0..1.0` for a cell and seed.
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut value = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ seed.wrapping_mul(0xCB1A_B31F);
    value ^= value >> 13;
    value = value.wrapping_mul(0x85EB_CA6B);
    value ^= value >> 16;
    (value & 0xFFFF) as f32 / 65_536.0
}
