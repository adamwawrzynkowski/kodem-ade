//! The light field behind the conversation.
//!
//! The chat lies on a few wide, soft lights in the colors of the theme. They rest at the sides
//! of the surface and below it, where the composer is, and drift slowly around those places.
//! The reading column keeps close to the plain surface color: a light has no edge, no dots and
//! no other detail that could compete with text, and a veil of the surface color lies over the
//! column above the composer, so that a light that drifts behind messages loses most of its
//! tint there.
//!
//! Its color and movement follow the open task: quiet and slow while idle, brighter and
//! livelier in the accent while the agent works, breathing in amber while the task waits for
//! the user, calm green after a finished turn, and red after a failure. A change of mood blends
//! the colors and the movement, so nothing jumps.
//!
//! Each light is one blurred shape that the GPU draws, so a frame costs a few shapes and no work
//! per pixel on the CPU. The field moves sixty times a second while the agent works or a mood
//! blends in, and thirty times a second otherwise; the drift is slow, so both read as
//! continuous, and a long task does not redraw the window at the full rate of a fast display.
//! The field is its own view beneath the cached workspace: a frame of the field repaints the
//! lights only and never renders the conversation again.
use super::*;
// The field is painted in window pixels over the measured chat surface.
use gpui::px;
use std::{cell::RefCell, f32::consts::TAU, rc::Rc, time::Instant};

/// Seconds in which the field closes most of the distance to a new mood.
const BLEND: f32 = 0.6;
/// Time between two frames of a calm field, and of a field that is lively or blends.
const CALM_FRAME: Duration = Duration::from_millis(33);
const LIVELY_FRAME: Duration = Duration::from_millis(16);
/// Strongest tint of a light on dark and on light surfaces. The blur spreads a light, so its
/// center shows about a third of this.
const TINT_DARK: f32 = 0.34;
const TINT_LIGHT: f32 = 0.22;
/// Share of the lights that the veil over the reading column takes away.
const VEIL: f32 = 0.75;
/// Blur of the veil's edges, in points, and how far the veil reaches past the column's sides
/// and past the top of the composer, in blurs. At that distance inside its edge it is dense.
const VEIL_BLUR: f32 = 56.;
const VEIL_REACH: f32 = 1.6;
/// Speed of the breath of a waiting task, in radians per second.
const BREATH_SPEED: f32 = 1.85;
/// Share by which a light swells and shrinks, and the speed of that, in radians per second.
const SWELL: f32 = 0.07;
const SWELL_SPEED: f32 = 0.31;

/// What the open task is doing, as the background shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Mood {
    #[default]
    Idle,
    Working,
    Waiting,
    Finished,
    Failed,
}

impl Mood {
    /// Returns the mood for the open task, or the idle mood when no task is open.
    pub(super) fn of(task: Option<&core::Task>) -> Self {
        let Some(task) = task else {
            return Self::Idle;
        };
        if task.needs_input() {
            return Self::Waiting;
        }
        match task.state {
            RunState::Starting | RunState::Running => Self::Working,
            RunState::Failed | RunState::DeliveryUnknown => Self::Failed,
            RunState::Completed => Self::Finished,
            RunState::Idle | RunState::Interrupted | RunState::Waiting => Self::Idle,
        }
    }
}

// One light of the field. Its place and its drift are shares of the surface's width and
// height. Its height and its blur are shares of the shorter side, so that the blur stays wide
// against the height and the shape has no visible edge.
struct Light {
    /// Where the light rests.
    anchor: (f32, f32),
    /// The farthest that it drifts from there along each axis.
    drift: (f32, f32),
    /// Speeds of the drift along the two axes, in radians per second, and where it starts.
    pace: (f32, f32),
    start: (f32, f32),
    /// Half of the width, as a share of the surface's width, and half of the height.
    half: (f32, f32),
    blur: f32,
    /// Strength as a share of the field's.
    strength: f32,
    /// Which of the mood's two tones the light has.
    tone: usize,
}

// A light at each side, a wide glow below the composer, and one faint light that wanders
// along the bottom. The speeds have no common period, so the field never repeats.
const LIGHTS: [Light; 4] = [
    Light {
        anchor: (-0.03, 0.68),
        drift: (0.05, 0.13),
        pace: (0.18, 0.117),
        start: (0., 1.3),
        half: (0.12, 0.17),
        blur: 0.17,
        strength: 1.,
        tone: 0,
    },
    Light {
        anchor: (1.03, 0.36),
        drift: (0.05, 0.15),
        pace: (0.133, 0.155),
        start: (2.1, 0.4),
        half: (0.11, 0.16),
        blur: 0.18,
        strength: 0.9,
        tone: 1,
    },
    Light {
        anchor: (0.5, 1.05),
        drift: (0.2, 0.025),
        pace: (0.098, 0.21),
        start: (4., 2.6),
        half: (0.34, 0.06),
        blur: 0.13,
        strength: 0.85,
        tone: 0,
    },
    Light {
        anchor: (0.5, 0.86),
        drift: (0.36, 0.07),
        pace: (0.075, 0.142),
        start: (5.2, 0.9),
        half: (0.09, 0.1),
        blur: 0.14,
        strength: 0.5,
        tone: 1,
    },
];

// How the field looks and moves in one mood. Every value blends when the mood changes.
#[derive(Clone, Copy, PartialEq)]
struct Field {
    /// The two tones of the lights.
    tones: [Rgba; 2],
    /// Strength of the lights, from 0 to 1.
    strength: f32,
    /// Speed of the drift as a multiple of each light's own pace.
    speed: f32,
    /// How far the lights drift, as a multiple of each light's own drift.
    reach: f32,
    /// Share of the strength that rises and falls with the breath.
    breath: f32,
}

impl Field {
    fn of(mood: Mood, theme: Theme) -> Self {
        let (first, second, strength, speed, reach, breath) = match mood {
            Mood::Idle => (theme.accent, theme.info, 0.55, 0.45, 1., 0.),
            Mood::Working => (theme.accent, theme.info, 1., 1.6, 1.25, 0.),
            Mood::Waiting => (theme.warning, theme.accent, 0.85, 0.6, 1., 0.3),
            Mood::Finished => (theme.added, theme.info, 0.65, 0.4, 1., 0.),
            Mood::Failed => (theme.removed, theme.warning, 0.7, 0.25, 0.8, 0.),
        };
        Self {
            tones: [hsla_to_rgba(first), hsla_to_rgba(second)],
            strength,
            speed,
            reach,
            breath,
        }
    }

    // Moves every value the share `amount` of the way to `target`.
    fn toward(self, target: Self, amount: f32) -> Self {
        let amount = amount.clamp(0., 1.);
        let mix = |from: f32, to: f32| from + (to - from) * amount;
        let blend = |from: Rgba, to: Rgba| {
            let mut color = from;
            color.red = mix(from.red, to.red);
            color.green = mix(from.green, to.green);
            color.blue = mix(from.blue, to.blue);
            color.alpha = mix(from.alpha, to.alpha);
            color
        };
        Self {
            tones: [
                blend(self.tones[0], target.tones[0]),
                blend(self.tones[1], target.tones[1]),
            ],
            strength: mix(self.strength, target.strength),
            speed: mix(self.speed, target.speed),
            reach: mix(self.reach, target.reach),
            breath: mix(self.breath, target.breath),
        }
    }

    // Reports whether the field has settled in `target`, so that calm frames are enough.
    fn close_to(self, target: Self) -> bool {
        let near = |from: f32, to: f32| (from - to).abs() < 0.004;
        let alike = |from: Rgba, to: Rgba| {
            near(from.red, to.red)
                && near(from.green, to.green)
                && near(from.blue, to.blue)
                && near(from.alpha, to.alpha)
        };
        near(self.strength, target.strength)
            && near(self.reach, target.reach)
            && near(self.breath, target.breath)
            && alike(self.tones[0], target.tones[0])
            && alike(self.tones[1], target.tones[1])
    }
}

// What the workspace last asked for, and what the background last painted.
#[derive(Default)]
struct BackdropState {
    bounds: Option<Bounds<Pixels>>,
    mood: Mood,
    /// Width of the centered column that holds the text, in window pixels.
    column: f32,
    /// Height at the bottom of the surface that the composer covers, in window pixels.
    composer: f32,
    painted: Option<(Bounds<Pixels>, Mood, f32, f32)>,
    layer: Option<WeakEntity<Backdrop>>,
}

#[derive(Default)]
struct Shared(Rc<RefCell<BackdropState>>);

impl Global for Shared {}

// Shared state behind a `RefCell`, so recording the surface on every frame never notifies
// global observers.
fn shared(cx: &mut App) -> Rc<RefCell<BackdropState>> {
    if !cx.has_global::<Shared>() {
        cx.set_global(Shared::default());
    }
    cx.global::<Shared>().0.clone()
}

/// Starts a frame of the workspace. The chat area is unknown until the conversation paints its
/// surface again. On the next frame the background redraws when what it shows is out of date.
pub(super) fn begin_frame(window: &mut Window, cx: &mut App) {
    shared(cx).borrow_mut().bounds = None;
    window.on_next_frame(|_, cx| {
        let state = shared(cx);
        let (stale, layer) = {
            let state = state.borrow();
            (
                state.painted
                    != state
                        .bounds
                        .map(|bounds| (bounds, state.mood, state.column, state.composer)),
                state.layer.clone(),
            )
        };
        if stale && let Some(layer) = layer.and_then(|layer| layer.upgrade()) {
            layer.update(cx, |_, cx| cx.notify());
        }
    });
}

/// An invisible element that marks its parent as the surface the background fills, in `mood`.
/// The parent must be positioned and must not paint a background of its own.
///
/// `column` is the width of the centered column that holds the text, in points. A veil of the
/// surface color keeps the lights faint behind it. `composer` is the height at the bottom of
/// the surface that the composer covers, in window pixels; the veil ends there and leaves the
/// glow around the composer.
pub(super) fn surface(mood: Mood, column: f32, composer: f32) -> impl IntoElement {
    let column = column * crate::theme::ui_scale();
    canvas(
        move |bounds, _, cx| {
            let state = shared(cx);
            let mut state = state.borrow_mut();
            state.bounds = Some(bounds);
            state.mood = mood;
            state.column = column;
            state.composer = composer;
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// Paints the chat surface and its lights. Add one to the window root beneath the workspace.
pub(super) struct Backdrop {
    field: Option<Field>,
    /// Seconds of drift so far. It grows faster in a livelier mood and never jumps, so the
    /// lights keep their places when the speed changes.
    clock: f64,
    /// Phase of the breath, in radians.
    breath: f32,
    stepped: Instant,
    waiting: bool,
}

impl Backdrop {
    /// Creates the background and registers it to be woken when the chat surface changes.
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        shared(cx).borrow_mut().layer = Some(cx.entity().downgrade());
        Self {
            field: None,
            clock: 0.,
            breath: 0.,
            stepped: Instant::now(),
            waiting: false,
        }
    }

    // Schedules the next frame once, however often the view renders before it.
    fn schedule(&mut self, delay: Duration, cx: &mut Context<Self>) {
        if self.waiting {
            return;
        }
        self.waiting = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |backdrop, cx| {
                backdrop.waiting = false;
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for Backdrop {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let state = shared(cx);
        let (bounds, mood, column, composer) = {
            let state = state.borrow();
            (state.bounds, state.mood, state.column, state.composer)
        };
        let layer = div().absolute().top_0().left_0().size_full();
        let Some(bounds) = bounds else {
            state.borrow_mut().painted = None;
            return layer;
        };
        state.borrow_mut().painted = Some((bounds, mood, column, composer));
        let target = Field::of(mood, theme);
        let now = Instant::now();
        // A long pause, such as a hidden window, must not turn into one large jump.
        let elapsed = now.duration_since(self.stepped).as_secs_f32().min(0.25);
        self.stepped = now;
        let still = cx.reduce_motion();
        let field = if still {
            target
        } else {
            // The same share of the remaining distance per second, whatever the frame rate.
            let field = self
                .field
                .unwrap_or(target)
                .toward(target, 1. - (-elapsed / BLEND).exp());
            self.clock += (field.speed * elapsed) as f64;
            self.breath = (self.breath + BREATH_SPEED * elapsed) % TAU;
            field
        };
        self.field = Some(field);
        // An unfocused window keeps moving. A hidden window is not rendered, so the field rests
        // until the window shows again.
        if !still {
            let lively = mood == Mood::Working || !field.close_to(target);
            self.schedule(if lively { LIVELY_FRAME } else { CALM_FRAME }, cx);
        }
        let frame = Frame {
            bounds,
            column,
            composer,
            field,
            clock: self.clock,
            breath: self.breath,
            tint: if theme.light { TINT_LIGHT } else { TINT_DARK },
            surface: theme.canvas,
        };
        layer.child(
            canvas(|_, _, _| {}, move |_, _, window, _| paint(frame, window))
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
        )
    }
}

// Everything that one frame of the field is painted from.
#[derive(Clone, Copy)]
struct Frame {
    bounds: Bounds<Pixels>,
    /// Width of the reading column, in window pixels.
    column: f32,
    /// Height at the bottom of the surface that the composer covers, in window pixels.
    composer: f32,
    field: Field,
    clock: f64,
    breath: f32,
    /// Strongest tint of a light on the surface of the theme.
    tint: f32,
    /// The color of the chat surface.
    surface: Hsla,
}

// Returns a smooth sway from -1 to 1. A second, faster swing keeps the path from closing into
// a plain loop.
fn sway(clock: f64, pace: f32, start: f32) -> f32 {
    let turn = std::f64::consts::TAU;
    let slow = ((clock * pace as f64) % turn) as f32 + start;
    let fast = ((clock * pace as f64 * 2.3) % turn) as f32 + start * 1.7;
    0.75 * slow.sin() + 0.25 * fast.sin()
}

// Returns the strength of the whole field in one frame, before each light's own share.
fn field_strength(frame: &Frame) -> f32 {
    frame.field.strength * frame.tint * (1. + frame.field.breath * frame.breath.sin())
}

// Returns the veil over the reading column: the shape that is blurred, and the blur. It
// starts above the surface, so that only its sides and its lower end fade in view. It ends
// behind the composer, so that it is dense over the last message and the glow around the
// composer stays.
fn veil(frame: &Frame) -> (Bounds<Pixels>, Pixels) {
    let bounds = frame.bounds;
    let blur = px(VEIL_BLUR * crate::theme::ui_scale());
    let reach = blur * VEIL_REACH;
    let half = px(frame.column / 2.) + reach;
    (
        Bounds::from_corners(
            point(bounds.center().x - half, bounds.top() - blur * 4.),
            point(
                bounds.center().x + half,
                bounds.bottom() - px(frame.composer) + reach,
            ),
        ),
        blur,
    )
}

// Returns where one light is in a frame: the shape that is blurred, the blur and the color.
fn placed(light: &Light, index: usize, frame: &Frame) -> (Bounds<Pixels>, Pixels, Hsla) {
    let Frame {
        bounds,
        field,
        clock,
        ..
    } = *frame;
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let shorter = width.min(height);
    let swell = 1. + SWELL * sway(clock, SWELL_SPEED, index as f32 * 1.9);
    let center_x = f32::from(bounds.left())
        + width
            * (light.anchor.0
                + light.drift.0 * field.reach * sway(clock, light.pace.0, light.start.0));
    let center_y = f32::from(bounds.top())
        + height
            * (light.anchor.1
                + light.drift.1 * field.reach * sway(clock, light.pace.1, light.start.1));
    let half_width = light.half.0 * width * swell;
    let half_height = light.half.1 * shorter * swell;
    (
        Bounds::new(
            point(px(center_x - half_width), px(center_y - half_height)),
            size(px(half_width * 2.), px(half_height * 2.)),
        ),
        px(light.blur * shorter),
        rgb_to_hsla(field.tones[light.tone])
            .opacity((field_strength(frame) * light.strength).clamp(0., 1.)),
    )
}

// Paints the surface, the lights of one frame and the veil, clipped to the chat surface.
fn paint(frame: Frame, window: &mut Window) {
    let bounds = frame.bounds;
    window.paint_quad(fill(bounds, frame.surface));
    if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
        return;
    }
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for (index, light) in LIGHTS.iter().enumerate() {
            let (shape, blur, color) = placed(light, index, &frame);
            if color.alpha <= 0.002 {
                continue;
            }
            window.paint_drop_shadows(
                shape,
                Corners::all(shape.size.width.min(shape.size.height) / 2.),
                &[BoxShadow {
                    color,
                    offset: point(px(0.), px(0.)),
                    blur_radius: blur,
                    spread_radius: px(0.),
                    inset: false,
                }],
            );
        }
        let (shape, blur) = veil(&frame);
        window.paint_drop_shadows(
            shape,
            Corners::default(),
            &[BoxShadow {
                color: frame.surface.opacity(VEIL),
                offset: point(px(0.), px(0.)),
                blur_radius: blur,
                spread_radius: px(0.),
                inset: false,
            }],
        );
    });
}
