//! The reactive dithered background behind the conversation.
//!
//! The chat lies on a dithered gradient: empty at the top, a full grid of dots at the bottom,
//! and between them a band in which the dots thin out in Bayer order. Every column of dots
//! starts its band at its own height, so the edge of the gradient is a row of streaks of
//! different lengths. The streaks rise and fall, and two swells cross along the edge in
//! opposite directions, a row of dots at a time. A band that moves changes the share of dots
//! at every height, so the dots of the whole gradient come and go with the swells.
//!
//! Its color and movement follow the open task: low, neutral and slow while idle, higher and
//! livelier in the accent while the agent works, breathing in amber while the task waits for
//! the user, calm green after a finished turn, and red after a failure.
//!
//! Each column is drawn from cached sprites, so a step costs a few sprites per column and no
//! per-dot work. The gradient is its own view beneath the cached workspace: a step repaints the
//! gradient only and never renders the conversation again.
use super::*;
use std::{cell::RefCell, f32::consts::TAU, rc::Rc, sync::Arc, time::Instant};

/// Pitch and size of a dot, in points.
const CELL: f32 = 3.;
const DOT: f32 = 1.5;
/// Rows over which a column goes from no dots to all dots.
const BAND: i32 = 176;
/// Rows of the sprite that fills a column below its band.
const FULL: i32 = 128;
/// Seconds a change of mood takes to blend in.
const BLEND: f32 = 0.9;
/// How much lower the gradient starts behind the reading column, as a share of the height, so
/// that most text has few dots behind it, and the distance beside the column over which the
/// gradient returns to its own height.
const QUIET_DROP: f32 = 0.1;
const FALLOFF: f32 = 150.;
/// Longest streak of a column above the edge of the gradient, in rows. Most columns are far
/// shorter; a few reach this.
const STREAK: f32 = 54.;
/// Height of the second swell as a share of the first, and its speed as a share of the speed
/// of the streaks.
const COUNTER_SWELL: f32 = 0.6;
const COUNTER_SPEED: f32 = 0.4;

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

    // Time between two steps of the gradient. A working task moves on the shared twelve-frame
    // clock; quieter moods step less often.
    fn cadence(self) -> Duration {
        Duration::from_millis(match self {
            Self::Working => 80,
            Self::Waiting => 100,
            Self::Idle | Self::Finished | Self::Failed => 110,
        })
    }

    // The color of the dots. It is fixed per mood, so its sprites are baked once; a change of
    // mood cross-fades two gradients instead of blending the color.
    fn tone(self, theme: Theme) -> Hsla {
        match self {
            Self::Idle => theme.dot.opacity(0.154),
            Self::Working => theme.accent.opacity(0.084),
            Self::Waiting => theme.warning.opacity(0.08),
            Self::Finished => theme.added.opacity(0.063),
            Self::Failed => theme.removed.opacity(0.07),
        }
    }
}

// Where the gradient stands and how it moves in one mood. Every value blends when the mood
// changes.
#[derive(Clone, Copy, PartialEq)]
struct Field {
    /// Where the band starts, as a share of the height from the top.
    start: f32,
    /// Height of the swell that runs along the edge, in rows. A second, longer swell of a
    /// smaller height runs against it.
    wave: f32,
    /// Share of a streak's length that rises and falls.
    flicker: f32,
    /// Speeds of the swell and of the streaks, in radians per second. The second swell moves
    /// with the clock of the streaks.
    speed: [f32; 2],
    /// Share of the height by which the whole gradient rises and falls.
    breath: f32,
}

impl Field {
    fn of(mood: Mood) -> Self {
        let (start, wave, flicker, speed, breath) = match mood {
            // The gradient fills about three quarters of the height from the bottom.
            Mood::Idle => (0.27, 7., 0.3, [0.35, 0.7], 0.012),
            Mood::Working => (0.18, 12., 0.6, [0.9, 2.2], 0.015),
            Mood::Waiting => (0.24, 7., 0.3, [0.3, 0.7], 0.035),
            Mood::Finished => (0.26, 6., 0.25, [0.28, 0.6], 0.01),
            Mood::Failed => (0.26, 4., 0.18, [0.18, 0.45], 0.012),
        };
        Self {
            start,
            wave,
            flicker,
            speed,
            breath,
        }
    }

    // Moves every value the share `amount` of the way to `target`.
    fn toward(self, target: Self, amount: f32) -> Self {
        let mix = |from: f32, to: f32| from + (to - from) * amount.clamp(0., 1.);
        Self {
            start: mix(self.start, target.start),
            wave: mix(self.wave, target.wave),
            flicker: mix(self.flicker, target.flicker),
            speed: [
                mix(self.speed[0], target.speed[0]),
                mix(self.speed[1], target.speed[1]),
            ],
            breath: mix(self.breath, target.breath),
        }
    }

    fn close_to(self, target: Self) -> bool {
        (self.start - target.start).abs() < 0.002
            && (self.wave - target.wave).abs() < 0.05
            && (self.flicker - target.flicker).abs() < 0.01
            && (self.breath - target.breath).abs() < 0.002
    }
}

// What the workspace last asked for, and what the background last painted.
#[derive(Default)]
struct BackdropState {
    bounds: Option<Bounds<Pixels>>,
    mood: Mood,
    /// Width of the centered column that holds the text.
    column: f32,
    painted: Option<(Bounds<Pixels>, Mood, f32)>,
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
                        .map(|bounds| (bounds, state.mood, state.column)),
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
/// `column` is the width of the centered column that holds the text. The gradient starts lower
/// behind it and returns to its own height toward the sides.
pub(super) fn surface(mood: Mood, column: f32) -> impl IntoElement {
    canvas(
        move |bounds, _, cx| {
            let state = shared(cx);
            let mut state = state.borrow_mut();
            state.bounds = Some(bounds);
            state.mood = mood;
            state.column = column;
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// Paints the chat surface and its dither. Add one to the window root beneath the workspace.
pub(super) struct Backdrop {
    field: Option<Field>,
    /// The mood whose color is shown, and the mood it fades from with the progress of the fade.
    shown: Mood,
    fading: Option<(Mood, f32)>,
    /// Phases of the wave and of the breath, in radians, and the clock of the streaks, which
    /// only grows so that streaks with different periods never jump.
    phase: [f32; 2],
    clock: f64,
    stepped: Instant,
    waiting: bool,
}

impl Backdrop {
    /// Creates the background and registers it to be woken when the chat surface changes.
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        shared(cx).borrow_mut().layer = Some(cx.entity().downgrade());
        Self {
            field: None,
            shown: Mood::Idle,
            fading: None,
            phase: [0.; 2],
            clock: 0.,
            stepped: Instant::now(),
            waiting: false,
        }
    }

    // Schedules the next step once, however often the view renders before it.
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
        let theme = cx
            .try_global::<Theme>()
            .copied()
            .unwrap_or(Theme::new(false));
        let state = shared(cx);
        let (bounds, mood, column) = {
            let state = state.borrow();
            (state.bounds, state.mood, state.column)
        };
        let layer = div().absolute().top_0().left_0().size_full();
        let Some(bounds) = bounds else {
            state.borrow_mut().painted = None;
            return layer;
        };
        state.borrow_mut().painted = Some((bounds, mood, column));
        let target = Field::of(mood);
        let now = Instant::now();
        // A long pause, such as a hidden window, must not turn into one large jump.
        let elapsed = now.duration_since(self.stepped).as_secs_f32().min(0.25);
        self.stepped = now;
        let still = cx.reduce_motion();
        if mood != self.shown {
            self.fading = (!still && self.field.is_some()).then_some((self.shown, 0.));
            self.shown = mood;
        }
        let field = if still {
            self.fading = None;
            target
        } else {
            let field = self
                .field
                .unwrap_or(target)
                .toward(target, elapsed / BLEND * 3.);
            self.phase[0] = (self.phase[0] + field.speed[0] * elapsed) % TAU;
            self.phase[1] = (self.phase[1] + 2.1 * elapsed) % TAU;
            self.clock += (field.speed[1] * elapsed) as f64;
            if let Some((_, progress)) = &mut self.fading {
                *progress += elapsed / BLEND;
            }
            if self.fading.is_some_and(|(_, progress)| progress >= 1.) {
                self.fading = None;
            }
            field
        };
        self.field = Some(field);
        // An unfocused window keeps its cadence. A hidden window is not rendered, so the gradient
        // rests until the window shows again.
        if !still {
            let delay = if self.fading.is_none() && field.close_to(target) {
                mood.cadence()
            } else {
                Mood::Working.cadence()
            };
            self.schedule(delay, cx);
        }
        // The clock is reduced before it becomes single precision; periods stay near one.
        let motion = Motion {
            wave: self.phase[0],
            breath: self.phase[1],
            clock: (self.clock % (TAU as f64 * 4096.)) as f32,
        };
        let shape = Shape {
            bounds,
            reading: column,
        };
        let fill_color = theme.canvas;
        let pass = move |tone: Hsla, opacity: f32| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .opacity(opacity)
                .child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, cx| paint(shape, field, motion, tone, window, cx),
                    )
                    .size_full(),
                )
        };
        let layer = layer.child(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| window.paint_quad(fill(bounds, fill_color)),
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
        // During a change of mood the old color fades out over the same gradient as the new one
        // fades in.
        match self.fading {
            Some((from, progress)) => {
                let shown = progress * progress * (3. - 2. * progress);
                layer
                    .child(pass(from.tone(theme), 1. - shown))
                    .child(pass(self.shown.tone(theme), shown))
            }
            None => layer.child(pass(self.shown.tone(theme), 1.)),
        }
    }
}

// Where the gradient is drawn: the surface and the width of its reading column.
#[derive(Clone, Copy)]
struct Shape {
    bounds: Bounds<Pixels>,
    reading: f32,
}

// Where the gradient is in its movement: the phases of the wave and of the breath, and the
// clock of the streaks.
#[derive(Clone, Copy)]
struct Motion {
    wave: f32,
    breath: f32,
    clock: f32,
}

// Returns a stable pseudo-random value from 0 to 1 for one column of dots.
fn noise(column: i32, seed: u32) -> f32 {
    let mut value = (column as u32).wrapping_mul(0x8DA6_B343) ^ seed.wrapping_mul(0xCB1A_B31F);
    value ^= value >> 13;
    value = value.wrapping_mul(0x5BD1_E995);
    value ^= value >> 15;
    (value & 0xFFFF) as f32 / 65_535.
}

// Paints the gradient: for every column of dots, the band in which it fills in Bayer order,
// then the full column below. A column's band is one of sixty-four cached sprites, chosen by
// the column's place in the Bayer matrix and by the row its band starts on, so that the dots
// stay on one fixed grid while the band moves a row at a time.
fn paint(
    shape: Shape,
    field: Field,
    motion: Motion,
    tone: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let Shape { bounds, reading } = shape;
    let scale = window.scale_factor();
    let snap = |value: f32| (value * scale).round() / scale;
    let (left, top) = (
        snap(f32::from(bounds.left())),
        snap(f32::from(bounds.top())),
    );
    let width = f32::from(bounds.size.width);
    let columns = (width / CELL).ceil() as i32;
    let rows = (f32::from(bounds.size.height) / CELL).ceil() as i32;
    if columns <= 0 || rows <= 0 {
        return;
    }
    let height = rows as f32;
    let edge = height * (field.start + field.breath * motion.breath.sin());
    let half = reading.min(width) / 2.;
    dither::renew_budget();
    let mut bands: [Option<Option<Arc<RenderImage>>>; 64] = [const { None }; 64];
    let full = dither::column_sprite(
        window,
        cx,
        tone,
        FULL as u32,
        (CELL, DOT),
        (1, 0, 0),
        &|_| true,
    );
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_layer(bounds, |window| {
            for column in 0..columns {
                let x = left + column as f32 * CELL;
                // The gradient starts lower behind the text and returns to its height beside it.
                let center = (column as f32 + 0.5) * CELL;
                let outside = (((center - width / 2.).abs() - half) / FALLOFF).clamp(0., 1.);
                let quiet = 1. - outside * outside * (3. - 2. * outside);
                // A column's streak: short for most columns, long for a few.
                let reach = noise(column, 1);
                let streak = 4. + 9. * noise(column, 2) + STREAK * reach * reach * reach * reach;
                let sway =
                    (motion.clock * (0.6 + 0.8 * noise(column, 3)) + noise(column, 4) * TAU).sin();
                let wave = field.wave
                    * ((column as f32 * 0.07 + motion.wave).sin()
                        + COUNTER_SWELL
                            * (column as f32 * 0.023 - motion.clock * COUNTER_SPEED).sin())
                    + 5. * (column as f32 * 0.021 + 1.3).sin();
                let start = (edge + height * QUIET_DROP * quiet + wave
                    - streak * (1. + field.flicker * sway))
                    .round() as i32;
                if start >= rows {
                    continue;
                }
                // The row the band starts on decides which rows of the Bayer matrix its rows
                // meet. Eight starts times eight columns of the matrix cover every case.
                let turn = start.rem_euclid(8);
                let slot = (column.rem_euclid(8) * 8 + turn) as usize;
                let band = bands[slot].get_or_insert_with(|| {
                    dither::column_sprite(
                        window,
                        cx,
                        tone,
                        BAND as u32,
                        (CELL, DOT),
                        (0, column.rem_euclid(8) as u8, turn as u8),
                        &|row| {
                            let share = (row as f32 + 0.5) / BAND as f32;
                            share > dither::cell_threshold(column.rem_euclid(8), row + turn)
                        },
                    )
                });
                if let Some(band) = band
                    && start + BAND > 0
                {
                    let area = Bounds::new(
                        point(px(x), px(top + start as f32 * CELL)),
                        size(px(CELL), px(BAND as f32 * CELL)),
                    );
                    let _ =
                        window.paint_image(area, area, Corners::default(), band.clone(), 0, false);
                }
                let Some(full) = &full else {
                    continue;
                };
                let mut row = (start + BAND).max(0);
                while row < rows {
                    let area = Bounds::new(
                        point(px(x), px(top + row as f32 * CELL)),
                        size(px(CELL), px(FULL as f32 * CELL)),
                    );
                    let _ =
                        window.paint_image(area, area, Corners::default(), full.clone(), 0, false);
                    row += FULL;
                }
            }
        });
    });
}
