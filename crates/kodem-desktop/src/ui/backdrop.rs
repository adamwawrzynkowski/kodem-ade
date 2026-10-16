//! The glyph field behind the conversation.
//!
//! The chat lies on a grid of faint monospace letters and digits. Most cells are empty. Noise
//! drifts across the grid and lights runs of cells where it is dense, so short streaks of
//! characters gather into shapes, morph, thin out and pass on. Each row slides and each column
//! rises and falls on its own, so the shapes flow and shear. Lit characters twinkle and change
//! every second or two. Behind the glyphs lie a few wide, soft lights in the colors of the
//! theme that drift slowly at the sides and below the composer; a veil of the surface color
//! keeps them faint behind the reading column.
//!
//! A press on the chat surface and every sent message start a wave: a ring of characters in
//! the tone of the task that spreads from the press, or from the composer, in a shape of its
//! own. The ring is bent by a few slow lobes that turn while it spreads, so no two waves look
//! alike. The characters under its edge scramble, a fainter wake fills the ring behind the
//! edge, and the wave fades as it spreads. Presses on selectable text, in fields and on the
//! composer do not start one.
//!
//! The reading column stays calm: the drifting streaks keep a quarter of their strength behind
//! it, and waves half. The field follows the open task: quiet while idle; denser, faster and
//! tinted in the accent while the agent works; breathing in amber while the task waits for the
//! user; green after a finished turn and red after a failure. A change of mood blends, so
//! nothing jumps.
//!
//! The field covers the whole window. The panels at its sides are glass that blurs it; the calm
//! column and the composer belong to the chat.
//!
//! Each light is one blurred shape that the GPU draws. Each lit cell is one glyph sprite from
//! GPUI's atlas. The grid lies on whole device pixels,
//! so each character is rasterized once and every later frame reuses its sprite. A frame
//! computes a density per cell from a few hashes and paints only the lit cells. The field steps
//! 25 times a second at rest, 30 while the agent works and 60 while a wave spreads or a mood
//! blends. It is its own view beneath the cached workspace: a frame of the field repaints the
//! glyphs only and never renders the conversation again.
use super::*;
// The field is painted in window pixels over the measured chat surface.
use gpui::px;
use std::{cell::RefCell, f32::consts::TAU, rc::Rc, time::Instant};

/// Seconds in which the field closes most of the distance to a new mood.
const BLEND: f32 = 0.6;
/// Time between two frames at rest, while the agent works, and while a wave spreads or a mood
/// blends.
const CALM_FRAME: Duration = Duration::from_millis(40);
const BUSY_FRAME: Duration = Duration::from_millis(33);
const LIVELY_FRAME: Duration = Duration::from_millis(16);
/// Size of the characters, in points, and the height of a row as a multiple of that size.
const GLYPH_SIZE: f32 = 10.;
const ROW_SPAN: f32 = 1.6;
/// The characters of the field.
const CHARSET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// Strongest alpha of a drifting streak, of the wake inside a wave and of a wave's edge.
const STREAK_ALPHA: f32 = 0.16;
const WAKE_ALPHA: f32 = 0.3;
const EDGE_ALPHA: f32 = 0.78;
/// Share of those alphas on a light surface.
const LIGHT_ALPHA: f32 = 0.8;
/// Share of the streaks and of the waves that the reading column takes away, and how far the
/// fade into the column reaches past its sides and past the top of the composer, in points.
const VEIL: f32 = 0.75;
const WAVE_VEIL: f32 = 0.5;
const VEIL_REACH: f32 = 64.;
/// Size of the drifting shapes and of their finer detail, in points.
const SHAPE_SCALE: (f32, f32) = (150., 110.);
const DETAIL_SCALE: (f32, f32) = (64., 48.);
/// Cells per run of lit characters, about.
const RUN_LENGTH: f32 = 3.6;
/// Seconds that one wave lives, and the most waves at once. A new wave beyond that replaces
/// the oldest.
const WAVE_LIFE: f32 = 2.8;
const MAX_WAVES: usize = 8;
/// Width of a wave's edge and depth of its wake when it starts and when it ends, in points.
const EDGE_WIDTH: (f32, f32) = (24., 64.);
const WAKE_DEPTH: (f32, f32) = (40., 200.);
/// Most glyphs that one frame paints, a bound for very large displays.
const MAX_GLYPHS: usize = 12_000;
/// Strongest tint of a light behind the glyphs on dark and on light surfaces. The blur
/// spreads a light, so its center shows about a third of this.
const TINT_DARK: f32 = 0.34;
const TINT_LIGHT: f32 = 0.22;
/// Share of the lights that the veil over the reading column takes away.
const LIGHT_VEIL: f32 = 0.75;
/// Blur of the light veil's edges, in points, and how far the veil reaches past the column's
/// sides and past the top of the composer, in blurs. At that distance inside its edge it is
/// dense.
const LIGHT_VEIL_BLUR: f32 = 56.;
const LIGHT_VEIL_REACH: f32 = 1.6;
/// Share by which a light swells and shrinks, and the speed of that, in radians per second.
const SWELL: f32 = 0.07;
const SWELL_SPEED: f32 = 0.31;
/// How far rows and columns of the drifting streaks sway, and the scale of the sway, in
/// points. Each row slides on its own and each column rises and falls on its own, so the
/// shapes flow and shear instead of sliding as one sheet.
const SWAY: (f32, f32) = (70., 50.);
const SWAY_SCALE: (f32, f32) = (220., 260.);
/// Changes of character per cell and second at rest, and under a wave's edge.
const CHURN: f64 = 0.6;
/// Speed of the twinkle of a drifting streak's characters, in radians per second, and the
/// share of their alpha that it takes.
const TWINKLE: f64 = 2.2;
const TWINKLE_DEPTH: f32 = 0.3;
const SCRAMBLE: f64 = 14.;
/// Speed of the breath of a waiting task, in radians per second.
const BREATH_SPEED: f32 = 1.85;

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

/// What starts a wave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Spark {
    /// A press on the chat surface.
    Press,
    /// A sent message, from the composer.
    Send,
}

// How the field looks and moves in one mood. Every value blends when the mood changes.
#[derive(Clone, Copy, PartialEq)]
struct Field {
    /// The tone of waves and of tinted streaks.
    tone: Rgba,
    /// Share of the streaks painted in the tone instead of the neutral color.
    tint: f32,
    /// How much more of the surface the streaks cover, as a shift of their threshold.
    cover: f32,
    /// Speed of the drift and of the changes of character, as a multiple of the calm speed.
    speed: f32,
    /// Share of the strength of the streaks and the lights that rises and falls with the
    /// breath.
    breath: f32,
    /// The second tone of the lights behind the glyphs; the first is `tone`.
    second: Rgba,
    /// Strength of the lights, from 0 to 1.
    glow: f32,
    /// Speed of the lights' drift as a multiple of each light's own pace, and how far they
    /// drift as a multiple of each light's own drift.
    drift: f32,
    reach: f32,
}

impl Field {
    fn of(mood: Mood, theme: Theme) -> Self {
        let (tone, tint, cover, speed, breath) = match mood {
            Mood::Idle => (theme.accent, 0., 0., 1., 0.),
            Mood::Working => (theme.accent, 0.45, 0.1, 1.8, 0.),
            Mood::Waiting => (theme.warning, 0.35, 0.04, 0.8, 0.3),
            Mood::Finished => (theme.added, 0.25, 0., 0.8, 0.),
            Mood::Failed => (theme.removed, 0.3, -0.05, 0.5, 0.),
        };
        let (second, glow, drift, reach) = match mood {
            Mood::Idle => (theme.info, 0.55, 0.45, 1.),
            Mood::Working => (theme.info, 1., 1.6, 1.25),
            Mood::Waiting => (theme.accent, 0.85, 0.6, 1.),
            Mood::Finished => (theme.info, 0.65, 0.4, 1.),
            Mood::Failed => (theme.warning, 0.7, 0.25, 0.8),
        };
        Self {
            tone: hsla_to_rgba(tone),
            tint,
            cover,
            speed,
            breath,
            second: hsla_to_rgba(second),
            glow,
            drift,
            reach,
        }
    }

    // Moves every value the share `amount` of the way to `target`.
    fn toward(self, target: Self, amount: f32) -> Self {
        let amount = amount.clamp(0., 1.);
        Self {
            tone: mix_rgba(self.tone, target.tone, amount),
            tint: mix(self.tint, target.tint, amount),
            cover: mix(self.cover, target.cover, amount),
            speed: mix(self.speed, target.speed, amount),
            breath: mix(self.breath, target.breath, amount),
            second: mix_rgba(self.second, target.second, amount),
            glow: mix(self.glow, target.glow, amount),
            drift: mix(self.drift, target.drift, amount),
            reach: mix(self.reach, target.reach, amount),
        }
    }

    // Reports whether the field has settled in `target`, so that calmer frames are enough.
    fn close_to(self, target: Self) -> bool {
        near(self.tint, target.tint)
            && near(self.cover, target.cover)
            && near(self.speed, target.speed)
            && near(self.breath, target.breath)
            && near(self.glow, target.glow)
            && near(self.drift, target.drift)
            && near(self.reach, target.reach)
            && near(self.second.red, target.second.red)
            && near(self.second.green, target.second.green)
            && near(self.second.blue, target.second.blue)
            && near(self.second.alpha, target.second.alpha)
            && near(self.tone.red, target.tone.red)
            && near(self.tone.green, target.tone.green)
            && near(self.tone.blue, target.tone.blue)
            && near(self.tone.alpha, target.tone.alpha)
    }
}

fn mix(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

fn mix_rgba(from: Rgba, to: Rgba, amount: f32) -> Rgba {
    Rgba::new(
        mix(from.red, to.red, amount),
        mix(from.green, to.green, amount),
        mix(from.blue, to.blue, amount),
        mix(from.alpha, to.alpha, amount),
    )
}

fn near(from: f32, to: f32) -> bool {
    (from - to).abs() < 0.004
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

// One soft light behind the glyphs. Its place and its drift are shares of the surface's width and
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

// One wave as it was started. Where it is in a frame follows from its age.
#[derive(Clone, Copy)]
struct Wave {
    origin: Point<Pixels>,
    born: Instant,
    /// Strength from 0 to 1.
    strength: f32,
    /// Ratio of the vertical to the horizontal reach.
    stretch: f32,
    /// The lobes that bend the ring: their count, depth as a share of the radius, start angle
    /// and turn in radians per second.
    bends: [(f32, f32, f32, f32); 3],
}

// What the workspace last asked for, what the background last painted, and the running waves.
#[derive(Default)]
struct BackdropState {
    bounds: Option<Bounds<Pixels>>,
    mood: Mood,
    /// Width of the centered column that holds the text, in window pixels.
    column: f32,
    /// Height at the bottom of the surface that the composer covers, in window pixels.
    composer: f32,
    painted: Option<(Bounds<Pixels>, Mood, f32, f32)>,
    waves: Vec<Wave>,
    /// Counts the waves, so that each one takes a different shape.
    seed: u32,
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

/// Starts a wave of `kind` at `origin`, in window pixels. Nothing starts while reduced motion
/// is on.
pub(super) fn spark(origin: Point<Pixels>, kind: Spark, cx: &mut App) {
    if cx.reduce_motion() {
        return;
    }
    let state = shared(cx);
    let layer = {
        let mut state = state.borrow_mut();
        let seed = state.seed;
        state.seed = seed.wrapping_add(1);
        let (strength, stretch) = match kind {
            Spark::Press => (0.85, 1.),
            // A sent message spreads wider than tall, along the composer.
            Spark::Send => (1., 0.72),
        };
        if state.waves.len() >= MAX_WAVES {
            state.waves.remove(0);
        }
        state.waves.push(Wave {
            origin,
            born: Instant::now(),
            strength,
            stretch,
            bends: [bend(seed, 0), bend(seed, 1), bend(seed, 2)],
        });
        state.layer.clone()
    };
    if let Some(layer) = layer.and_then(|layer| layer.upgrade()) {
        layer.update(cx, |_, cx| cx.notify());
    }
}

/// Starts the wave of a sent message from the middle of the composer.
pub(super) fn spark_send(cx: &mut App) {
    if let Some(composer) = fx::anchor_bounds(chat::COMPOSER_ANCHOR, cx) {
        spark(composer.center(), Spark::Send, cx);
    }
}

// Returns one lobe of a wave's ring. The first lobe is the deepest, so the shape keeps one
// clear outline with finer bends on it.
fn bend(seed: u32, index: u32) -> (f32, f32, f32, f32) {
    let depth = [0.16, 0.09, 0.05][index as usize % 3];
    (
        (2 + hash(seed, index, 1) % 5) as f32,
        depth * (0.5 + unit(hash(seed, index, 2))),
        unit(hash(seed, index, 3)) * TAU,
        (unit(hash(seed, index, 4)) - 0.5) * 0.8,
    )
}

/// An invisible element that marks its parent as the surface the background fills, in `mood`.
/// The parent must be positioned and must not paint a background of its own. A press on the
/// surface that no text or field takes, outside the composer, starts a wave there.
///
/// `column` is the width of the centered column that holds the text, in points. The field is
/// calmer behind it. `composer` is the height at the bottom of the surface that the composer
/// covers, in window pixels; the calm column ends there and leaves the field around the
/// composer.
pub(super) fn surface(mood: Mood, column: f32, composer: f32) -> impl IntoElement {
    let column = column * crate::theme::ui_scale();
    canvas(
        move |bounds, window, cx| {
            let state = shared(cx);
            let mut state = state.borrow_mut();
            state.bounds = Some(bounds);
            state.mood = mood;
            state.column = column;
            state.composer = composer;
            window.insert_hitbox(bounds, HitboxBehavior::Normal)
        },
        |_, hitbox, window, _| {
            // The surface is painted first, so this listener runs after every element above
            // it. A text or a field that takes focus on the press prevents the default.
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || window.default_prevented()
                    || !hitbox.is_hovered(window)
                {
                    return;
                }
                let over_composer = fx::anchor_bounds(chat::COMPOSER_ANCHOR, cx)
                    .is_some_and(|bounds| bounds.contains(&event.position));
                if !over_composer {
                    spark(event.position, Spark::Press, cx);
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

// The characters of the field, shaped once for each typeface and size.
struct Glyphs {
    family: &'static str,
    size: Pixels,
    set: Rc<[(FontId, GlyphId)]>,
    /// Advance of one character, in window pixels.
    advance: f32,
}

impl Glyphs {
    fn shape(family: &'static str, size: Pixels, window: &mut Window) -> Self {
        let line = window.text_system().shape_line(
            CHARSET.into(),
            size,
            &[TextRun {
                len: CHARSET.len(),
                font: font(family),
                color: black(),
                background_color: None,
                underline: None,
                strikethrough: None,
                letter_spacing: None,
            }],
            None,
        );
        let set: Rc<[(FontId, GlyphId)]> = line
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(|glyph| (run.font_id, glyph.id)))
            .collect();
        let advance = f32::from(line.width) / set.len().max(1) as f32;
        Self {
            family,
            size,
            set,
            advance,
        }
    }
}

/// Paints the chat surface and its glyph field. Add one to the window root beneath the
/// workspace.
pub(super) struct Backdrop {
    field: Option<Field>,
    /// Seconds of drift so far. It grows faster in a livelier mood and never jumps, so the
    /// streaks keep their places when the speed changes.
    clock: f64,
    /// Seconds of the lights' drift so far, at their own speed.
    drift: f64,
    /// Phase of the breath, in radians.
    breath: f32,
    /// When the field started, for the scramble under waves, which keeps one speed.
    started: Instant,
    stepped: Instant,
    waiting: bool,
    glyphs: Option<Glyphs>,
    /// The cells of the last grid, kept while its size stays the same.
    cells: Option<(u32, u32, Rc<[Cell]>)>,
}

impl Backdrop {
    /// Creates the background and registers it to be woken when the chat surface changes.
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        shared(cx).borrow_mut().layer = Some(cx.entity().downgrade());
        let now = Instant::now();
        Self {
            field: None,
            clock: 0.,
            drift: 0.,
            breath: 0.,
            started: now,
            stepped: now,
            waiting: false,
            glyphs: None,
            cells: None,
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

    // Returns the characters in the current typeface and size, their size and their advance,
    // shaping them on a change.
    fn glyphs(&mut self, window: &mut Window) -> (Rc<[(FontId, GlyphId)]>, Pixels, f32) {
        let family = crate::theme::mono();
        let size = px(GLYPH_SIZE * crate::theme::ui_scale());
        let glyphs = self
            .glyphs
            .take()
            .filter(|glyphs| glyphs.family == family && glyphs.size == size)
            .unwrap_or_else(|| Glyphs::shape(family, size, window));
        let glyphs = self.glyphs.insert(glyphs);
        (glyphs.set.clone(), glyphs.size, glyphs.advance)
    }

    // Returns the grid over `bounds` for characters of `size` and `advance`.
    fn grid(&mut self, bounds: Bounds<Pixels>, size: Pixels, advance: f32, device: f32) -> Grid {
        let size = f32::from(size);
        let cell = (
            snap(advance, device).max(1. / device),
            snap(size * ROW_SPAN, device).max(1. / device),
        );
        let columns = (f32::from(bounds.size.width) / cell.0).ceil().max(0.) as u32 + 1;
        let rows = (f32::from(bounds.size.height) / cell.1).ceil().max(0.) as u32 + 1;
        let cells = match &self.cells {
            Some((kept_columns, kept_rows, cells))
                if (*kept_columns, *kept_rows) == (columns, rows) =>
            {
                cells.clone()
            }
            _ => {
                let cells: Rc<[Cell]> = (0..rows)
                    .flat_map(|row| (0..columns).map(move |column| Cell::at(column, row)))
                    .collect();
                self.cells = Some((columns, rows, cells.clone()));
                cells
            }
        };
        Grid {
            origin: (
                snap(f32::from(bounds.left()), device),
                snap(f32::from(bounds.top()), device),
            ),
            cell,
            baseline: snap(cell.1 / 2. + size * 0.35, device),
            columns,
            rows,
            cells,
        }
    }
}

impl Render for Backdrop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        // The field covers the whole window, beneath the panels at its sides, which blur it.
        // The calm column and the composer belong to the chat surface.
        let chat = bounds;
        let bounds = Bounds::new(point(Pixels::ZERO, Pixels::ZERO), window.viewport_size());
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
            self.drift += (field.drift * elapsed) as f64;
            self.breath = (self.breath + BREATH_SPEED * elapsed) % TAU;
            field
        };
        self.field = Some(field);
        let rings: Vec<Ring> = {
            let mut state = state.borrow_mut();
            state
                .waves
                .retain(|wave| now.duration_since(wave.born).as_secs_f32() < WAVE_LIFE);
            state
                .waves
                .iter()
                .map(|wave| Ring::of(wave, bounds, now))
                .collect()
        };
        // An unfocused window keeps moving. A hidden window is not rendered, so the field rests
        // until the window shows again.
        if !still {
            let delay = if !rings.is_empty() || !field.close_to(target) {
                LIVELY_FRAME
            } else if mood == Mood::Working {
                BUSY_FRAME
            } else {
                CALM_FRAME
            };
            self.schedule(delay, cx);
        }
        let (set, size, advance) = self.glyphs(window);
        let grid = self.grid(bounds, size, advance, window.scale_factor());
        let neutral = hsla_to_rgba(theme.muted);
        let frame = Frame {
            bounds,
            chat,
            column,
            composer,
            clock: self.clock,
            scramble: (now.duration_since(self.started).as_secs_f64() * SCRAMBLE) as u32,
            cover: field.cover,
            strength: 1. + field.breath * self.breath.sin(),
            alpha: if theme.light { LIGHT_ALPHA } else { 1. },
            tones: [
                rgb_to_hsla(field.tone),
                rgb_to_hsla(mix_rgba(neutral, field.tone, 0.4)),
                rgb_to_hsla(mix_rgba(neutral, field.tone, field.tint)),
            ],
            drift: self.drift,
            reach: field.reach,
            glow: field.glow
                * if theme.light { TINT_LIGHT } else { TINT_DARK }
                * (1. + field.breath * self.breath.sin()),
            lights: [rgb_to_hsla(field.tone), rgb_to_hsla(field.second)],
            surface: theme.canvas,
            grid,
            set,
            size,
            rings,
        };
        layer.child(
            canvas(|_, _, _| {}, move |_, _, window, _| paint(&frame, window))
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
        )
    }
}

// Where one wave is in a frame, in window pixels.
#[derive(Clone, Copy)]
struct Ring {
    center: (f32, f32),
    radius: f32,
    /// Width of the edge and depth of the wake behind it.
    width: f32,
    wake: f32,
    /// How much of the ring's cells light, and how brightly, in this frame. The ring stays
    /// dense while it fades and thins only near its end, so it dims before it dissolves.
    body: f32,
    fade: f32,
    stretch: f32,
    /// Lobes: count, depth and angle in this frame.
    bends: [(f32, f32, f32); 3],
    /// Distances from the center beyond which, and within which, the wave shows nothing.
    outer: f32,
    inner: f32,
}

// What a wave gives one cell: for its edge and for its wake, the density that lights the cell
// and the strength of its alpha.
#[derive(Clone, Copy, Default)]
struct Touch {
    edge: (f32, f32),
    wake: (f32, f32),
}

impl Touch {
    // Keeps the stronger of each value, where waves cross.
    fn max(self, other: Self) -> Self {
        Self {
            edge: (self.edge.0.max(other.edge.0), self.edge.1.max(other.edge.1)),
            wake: (self.wake.0.max(other.wake.0), self.wake.1.max(other.wake.1)),
        }
    }
}

impl Ring {
    fn of(wave: &Wave, bounds: Bounds<Pixels>, now: Instant) -> Self {
        let age = now.duration_since(wave.born).as_secs_f32();
        let progress = (age / WAVE_LIFE).clamp(0., 1.);
        let scale = crate::theme::ui_scale();
        let center = (f32::from(wave.origin.x), f32::from(wave.origin.y));
        // The wave reaches the farthest corner of the surface as it ends.
        let reach = [
            bounds.origin,
            bounds.top_right(),
            bounds.bottom_left(),
            bounds.bottom_right(),
        ]
        .iter()
        .map(|corner| {
            let x = f32::from(corner.x) - center.0;
            let y = (f32::from(corner.y) - center.1) / wave.stretch;
            (x * x + y * y).sqrt()
        })
        .fold(0., f32::max);
        let radius = reach * (1. - (1. - progress).powf(1.8));
        let width = mix(EDGE_WIDTH.0, EDGE_WIDTH.1, progress) * scale;
        let wake = mix(WAKE_DEPTH.0, WAKE_DEPTH.1, progress) * scale;
        let depth: f32 = wave.bends.iter().map(|bend| bend.1).sum();
        let start = smoothstep(0., 0.05, progress);
        Self {
            center,
            radius,
            width,
            wake,
            body: start * 1.25 * (1. - progress.powi(3)),
            fade: wave.strength * start * (1. - progress).powf(1.5),
            stretch: wave.stretch,
            bends: wave
                .bends
                .map(|(lobes, depth, start, turn)| (lobes, depth, start + turn * age)),
            outer: radius * (1. + depth) + width * 3.,
            // Four depths into the wake it is below the faintest alpha that shows.
            inner: (radius * (1. - depth) - wake * 4.).max(0.),
        }
    }

    // Returns what the wave gives a cell centered at a point.
    fn touch(&self, x: f32, y: f32) -> Touch {
        let dx = x - self.center.0;
        let dy = (y - self.center.1) / self.stretch;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance > self.outer || distance < self.inner {
            return Touch::default();
        }
        let angle = dy.atan2(dx);
        let shape: f32 = self
            .bends
            .iter()
            .map(|(lobes, depth, start)| depth * (lobes * angle + start).sin())
            .sum();
        let gap = distance - self.radius * (1. + shape);
        let edge = (-(gap / self.width).powi(2)).exp();
        let wake = if gap < 0. {
            (gap / self.wake).exp()
        } else {
            0.
        };
        Touch {
            edge: (edge * self.body, lift(edge) * self.fade),
            wake: (wake * self.body * 0.75, lift(wake) * self.fade),
        }
    }
}

// Everything that one frame of the field is painted from.
struct Frame {
    /// The window, which the field covers.
    bounds: Bounds<Pixels>,
    /// The chat surface, which holds the calm column and the composer.
    chat: Bounds<Pixels>,
    /// Width of the reading column, centered on the chat, in window pixels.
    column: f32,
    /// Height at the bottom of the chat that the composer covers, in window pixels.
    composer: f32,
    grid: Grid,
    clock: f64,
    /// Step of the scramble under the edges of waves.
    scramble: u32,
    cover: f32,
    /// Strength of the streaks, with the breath.
    strength: f32,
    /// Share of the alphas on the surface of the theme.
    alpha: f32,
    /// The tones of the layers: a wave's edge, a wave's wake and the drifting streaks.
    tones: [Hsla; 3],
    /// Seconds of the lights' drift, how far they drift, their strength with the breath and
    /// their two tones.
    drift: f64,
    reach: f32,
    glow: f32,
    lights: [Hsla; 2],
    surface: Hsla,
    set: Rc<[(FontId, GlyphId)]>,
    size: Pixels,
    rings: Vec<Ring>,
}

// The cells over the surface. They lie on whole device pixels, so every glyph reuses one
// sprite of the atlas.
#[derive(Clone)]
struct Grid {
    /// Top left corner of the first cell, the size of a cell and the baseline in a cell, in
    /// window pixels.
    origin: (f32, f32),
    cell: (f32, f32),
    baseline: f32,
    columns: u32,
    rows: u32,
    /// What each cell keeps from frame to frame, row by row.
    cells: Rc<[Cell]>,
}

// What one cell keeps from frame to frame.
#[derive(Clone, Copy)]
struct Cell {
    /// The density that lights the cell. Runs of cells along a row share a low threshold, and
    /// the Bayer threshold dithers their ends, so a rising density grows streaks of
    /// characters.
    threshold: f32,
    /// Its own pace of changes of character, as a multiple of the field's, and where it starts.
    pace: f32,
    offset: f32,
}

impl Cell {
    fn at(column: u32, row: u32) -> Self {
        let run = noise(column as f32 / RUN_LENGTH, row as f32 * 7.31);
        let seed = hash(column, row, 0);
        Self {
            threshold: 0.7 * run + 0.3 * crate::bayer::threshold(column as i64, row as i64),
            pace: 0.5 + unit(seed),
            offset: unit(seed >> 3) * 16.,
        }
    }
}

// Mixes three integers into a well-spread hash.
fn hash(a: u32, b: u32, c: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1)
        ^ b.wrapping_mul(0x85EB_CA77).rotate_left(13)
        ^ c.wrapping_mul(0xC2B2_AE3D).rotate_left(27);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

// Returns a hash as a share from 0 to 1.
fn unit(hash: u32) -> f32 {
    (hash >> 8) as f32 / (1u32 << 24) as f32
}

// Returns smooth value noise from 0 to 1.
fn noise(x: f32, y: f32) -> f32 {
    let (left, top) = (x.floor(), y.floor());
    let (column, row) = (left as i32 as u32, top as i32 as u32);
    let (across, down) = (x - left, y - top);
    let (across, down) = (
        across * across * (3. - 2. * across),
        down * down * (3. - 2. * down),
    );
    mix(
        mix(
            lattice(column, row),
            lattice(column.wrapping_add(1), row),
            across,
        ),
        mix(
            lattice(column, row.wrapping_add(1)),
            lattice(column.wrapping_add(1), row.wrapping_add(1)),
            across,
        ),
        down,
    )
}

// Returns the value of the noise at a point of its integer lattice.
fn lattice(column: u32, row: u32) -> f32 {
    unit(hash(column, row, 7))
}

// Rounds a length in window pixels to whole device pixels.
fn snap(value: f32, device: f32) -> f32 {
    (value * device).round() / device
}

// Returns the density of the drifting streaks at a point, in points of the surface, times the
// two factors of `gain`. The large shapes drift and morph into new ones, and a finer detail
// drifts against them. With a `floor`, a density that cannot exceed it with the finest detail
// returns 0 before the detail is sampled.
fn streaks(x: f32, y: f32, clock: f32, cover: f32, gain: (f32, f32), floor: Option<f32>) -> f32 {
    let shape = morph(
        x / SHAPE_SCALE.0 + clock * 0.15,
        y / SHAPE_SCALE.1 + clock * 0.04,
        clock * 0.2,
    );
    let density = |value: f32| smoothstep(0.5 - cover, 0.8 - cover, value) * gain.0 * gain.1;
    // The detail is at most 1.
    if floor.is_some_and(|floor| density(0.68 * shape + 0.32) <= floor) {
        return 0.;
    }
    let detail = noise(
        x / DETAIL_SCALE.0 - clock * 0.19,
        y / DETAIL_SCALE.1 + clock * 0.05 + 7.1,
    );
    density(0.68 * shape + 0.32 * detail)
}

// Returns value noise that blends from one field into the next as `z` grows, so shapes grow,
// split and fade instead of only sliding. The blend is rescaled to keep the contrast of a
// single field, so the coverage does not pulse between fields.
fn morph(x: f32, y: f32, z: f32) -> f32 {
    let layer = z.floor();
    let blend = z - layer;
    let blend = blend * blend * (3. - 2. * blend);
    let first = noise(x + layer * 17.3, y - layer * 9.1);
    let second = noise(x + (layer + 1.) * 17.3, y - (layer + 1.) * 9.1);
    let spread = (blend * blend + (1. - blend) * (1. - blend)).sqrt();
    (mix(first, second, blend) - 0.5) / spread + 0.5
}

// Returns the share of a layer's alpha that a cell of strength `value` shows. A faint value
// still shows its glyph a little, so the ends of streaks stay legible as characters; the far
// tail of a wave fades out entirely.
fn lift(value: f32) -> f32 {
    smoothstep(0.02, 0.12, value) * (0.4 + 0.6 * value.min(1.))
}

// One lit cell of a frame, painted after the glow.
struct Lit {
    origin: Point<Pixels>,
    /// The layer whose tone the cell takes: 0 for a wave's edge, 1 for its wake and 2 for the
    /// drifting streaks.
    layer: usize,
    alpha: f32,
    glyph: usize,
}

// Paints the surface, the glow and the lit cells of one frame, clipped to the chat surface.
fn paint(frame: &Frame, window: &mut Window) {
    let bounds = frame.bounds;
    window.paint_quad(fill(bounds, frame.surface));
    if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) || frame.set.is_empty() {
        return;
    }
    let lit = lay_out(frame);
    let set = frame.set.len();
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        paint_lights(frame, window);
        // The glyphs share one layer above the lights. GPUI then orders the layer once instead
        // of placing each of thousands of glyphs in its bounds tree on every frame.
        window.paint_layer(bounds, |window| {
            for cell in &lit {
                let (font_id, glyph_id) = frame.set[cell.glyph % set];
                window
                    .paint_glyph(
                        cell.origin,
                        font_id,
                        glyph_id,
                        frame.size,
                        frame.tones[cell.layer].opacity(cell.alpha),
                    )
                    .ok();
            }
        });
    });
}

// Returns the lit cells of one frame.
fn lay_out(frame: &Frame) -> Vec<Lit> {
    let Grid {
        origin,
        cell,
        baseline,
        columns,
        rows,
        ..
    } = frame.grid;
    let scale = crate::theme::ui_scale();
    let clock = frame.clock as f32;
    // The calm column: how much of it lies over each column and each row of cells.
    let middle = f32::from(frame.chat.center().x);
    let reach = VEIL_REACH * scale;
    let half = frame.column / 2.;
    let limit = f32::from(frame.chat.bottom()) - frame.composer;
    let across: Vec<f32> = (0..columns)
        .map(|column| {
            if frame.column <= 0. {
                return 0.;
            }
            let x = origin.0 + (column as f32 + 0.5) * cell.0;
            1. - smoothstep(half, half + reach, (x - middle).abs())
        })
        .collect();
    // The sway of each column, in points, which moves the streaks in it up and down.
    let rise: Vec<f32> = (0..columns)
        .map(|column| {
            let x = column as f32 * cell.0 / scale;
            (noise(x / SWAY_SCALE.1, clock * 0.11 + 40.) - 0.5) * 2. * SWAY.1
        })
        .collect();
    let mut lit = Vec::new();
    'rows: for row in 0..rows {
        let y = origin.1 + row as f32 * cell.1;
        let middle_y = y + cell.1 / 2.;
        let down = 1. - smoothstep(limit - reach / 2., limit + reach / 2., middle_y);
        let place_y = (y - origin.1) / scale;
        // The sway of the row, in points, which slides the streaks in it sideways.
        let slide = (noise(place_y / SWAY_SCALE.0 + 0.5, clock * 0.14) - 0.5) * 2. * SWAY.0;
        for column in 0..columns {
            let x = origin.0 + column as f32 * cell.0;
            let Some((layer, alpha, glyph)) = light(
                frame,
                (column, row),
                (
                    (x - origin.0) / scale + slide,
                    place_y + rise[column as usize],
                ),
                (x + cell.0 / 2., middle_y),
                across[column as usize] * down,
            ) else {
                continue;
            };
            lit.push(Lit {
                origin: point(px(x), px(y + baseline)),
                layer,
                alpha,
                glyph,
            });
            if lit.len() >= MAX_GLYPHS {
                break 'rows;
            }
        }
    }
    lit
}

// Paints the soft lights behind the glyphs and the veil that keeps them faint behind the
// reading column. Each light is one blurred shape that the GPU draws.
fn paint_lights(frame: &Frame, window: &mut Window) {
    for (index, light) in LIGHTS.iter().enumerate() {
        let (shape, blur, color) = placed(light, index, frame);
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
    if frame.column <= 0. {
        return;
    }
    let (shape, blur) = light_veil(frame);
    window.paint_drop_shadows(
        shape,
        Corners::default(),
        &[BoxShadow {
            color: frame.surface.opacity(LIGHT_VEIL),
            offset: point(px(0.), px(0.)),
            blur_radius: blur,
            spread_radius: px(0.),
            inset: false,
        }],
    );
}

// Returns a smooth sway from -1 to 1. A second, faster swing keeps the path from closing into
// a plain loop.
fn sway(clock: f64, pace: f32, start: f32) -> f32 {
    let turn = std::f64::consts::TAU;
    let slow = ((clock * pace as f64) % turn) as f32 + start;
    let fast = ((clock * pace as f64 * 2.3) % turn) as f32 + start * 1.7;
    0.75 * slow.sin() + 0.25 * fast.sin()
}

// Returns the veil over the reading column: the shape that is blurred, and the blur. It
// starts above the surface, so that only its sides and its lower end fade in view. It ends
// behind the composer, so that it is dense over the last message and the light around the
// composer stays.
fn light_veil(frame: &Frame) -> (Bounds<Pixels>, Pixels) {
    let bounds = frame.chat;
    let blur = px(LIGHT_VEIL_BLUR * crate::theme::ui_scale());
    let reach = blur * LIGHT_VEIL_REACH;
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
    let bounds = frame.bounds;
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let shorter = width.min(height);
    let clock = frame.drift;
    let swell = 1. + SWELL * sway(clock, SWELL_SPEED, index as f32 * 1.9);
    let center_x = f32::from(bounds.left())
        + width
            * (light.anchor.0
                + light.drift.0 * frame.reach * sway(clock, light.pace.0, light.start.0));
    let center_y = f32::from(bounds.top())
        + height
            * (light.anchor.1
                + light.drift.1 * frame.reach * sway(clock, light.pace.1, light.start.1));
    let half_width = light.half.0 * width * swell;
    let half_height = light.half.1 * shorter * swell;
    (
        Bounds::new(
            point(px(center_x - half_width), px(center_y - half_height)),
            size(px(half_width * 2.), px(half_height * 2.)),
        ),
        px(light.blur * shorter),
        frame.lights[light.tone].opacity((frame.glow * light.strength).clamp(0., 1.)),
    )
}

// Returns the layer, the alpha and the character index of one cell, or nothing when the cell
// is dark. `place` is where the streaks are sampled for the cell, in points, `center` the
// cell's center in window pixels, and `veil` how far it lies in the calm column.
fn light(
    frame: &Frame,
    (column, row): (u32, u32),
    place: (f32, f32),
    center: (f32, f32),
    veil: f32,
) -> Option<(usize, f32, usize)> {
    let cell = frame.grid.cells[(row * frame.grid.columns + column) as usize];
    let touch = frame.rings.iter().fold(Touch::default(), |touch, ring| {
        touch.max(ring.touch(center.0, center.1))
    });
    // A cell that neither a wave nor the strongest streak could light stays dark without
    // sampling the noise. The streaks are at most 1 before their gain.
    let gain = (frame.strength, 1. - VEIL * veil);
    let waves = touch.edge.0.max(touch.wake.0);
    if (1. * gain.0 * gain.1).max(waves) <= cell.threshold {
        return None;
    }
    let streak = streaks(
        place.0,
        place.1,
        frame.clock as f32,
        frame.cover,
        gain,
        (waves <= cell.threshold).then_some(cell.threshold),
    );
    let calm = 1. - WAVE_VEIL * veil;
    if streak.max(waves) <= cell.threshold {
        return None;
    }
    // The characters of a streak twinkle, each at its own pace.
    let twinkle = 1. - TWINKLE_DEPTH
        + TWINKLE_DEPTH
            * (frame.clock * TWINKLE * cell.pace as f64 + cell.offset as f64).sin() as f32;
    // A lit cell takes the tone of whatever is strongest at it.
    let (layer, alpha) = [
        EDGE_ALPHA * touch.edge.1 * calm,
        WAKE_ALPHA * touch.wake.1 * calm,
        STREAK_ALPHA * lift(streak) * twinkle,
    ]
    .into_iter()
    .enumerate()
    .fold((2, 0.), |best, candidate| {
        if candidate.1 > best.1 {
            candidate
        } else {
            best
        }
    });
    let alpha = alpha * frame.alpha;
    if alpha < 0.01 {
        return None;
    }
    // Each cell changes its character at its own pace; under an edge it scrambles.
    let step = if touch.edge.0 > 0.3 {
        frame.scramble.wrapping_add(0x5C2A_3B1D)
    } else {
        (frame.clock * CHURN * cell.pace as f64 + cell.offset as f64) as u32
    };
    Some((layer, alpha, hash(column, row, step) as usize))
}
