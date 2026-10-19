//! A pixel character that skates on the top edge of a conversation's composer.
//!
//! The workspace describes the scene each time it renders: [`begin_frame`] clears it, [`cue`]
//! names the character and the state of the open task, and [`ground`] markers inside the composer
//! record the edge and the tabs that the character rides on. One [`SkaterLayer`] above the
//! workspace runs the character on its own frames, so the workspace does not render again while
//! it moves.
//!
//! The character idles while the agent rests and skates, jumps the tabs and does tricks while it
//! works. It falls off when the conversation scrolls away from its end and leaps back once the
//! end shows again. The pointer can pick it up and throw it.
//!
//! Two avatars ride their own way. Santa drives a sleigh that reindeer pull, which flies over the
//! tabs, soars, loops and drops presents. The zombie has no board: it shambles along the edge and
//! stops to breakdance or to toss its head.
mod art;

use super::*;
// The character moves in window pixels, between measured bounds.
use art::{Arms, Deck, Eyes, Legs, Palette, Pen, Pose, Ride, Team};
use gpui::px;
use std::{
    cell::{Cell, RefCell},
    f32::consts::{PI, TAU},
    rc::Rc,
    time::Instant,
};

pub(super) use art::portrait;

/// Height that a conversation keeps clear above its composer for the character, in points.
pub(super) const ROOM: f32 = 40.;

/// Size of one cell of the art at an interface scale of 1, in window pixels.
const PIXEL: f32 = 2.;
/// Downward pull, in points per second squared.
const GRAVITY: f32 = 900.;
/// Pull on a character that hangs from the pointer. It is stronger, so the swing is quick.
const HANG_GRAVITY: f32 = 2600.;
/// Speed that pushing keeps up, in points per second.
const CRUISE: f32 = 86.;
const PUSH: f32 = 150.;
const ROLL_FRICTION: f32 = 6.;
const BRAKE: f32 = 140.;
const WALK: f32 = 34.;
/// Speed of a walking zombie, and of its glide back in a moonwalk.
const SHAMBLE: f32 = 28.;
const MOONWALK: f32 = 30.;
/// Height of a trick on flat ground, and of a plain ollie.
const TRICK_LIFT: f32 = 25.;
const OLLIE_LIFT: f32 = 17.;
/// Height of a sleigh's soar and of its loop, which needs room under the reindeer.
const SOAR_LIFT: f32 = 34.;
const LOOP_LIFT: f32 = 52.;
/// Share of the pull that a flying sleigh feels, so it floats.
const SLEIGH_FLOAT: f32 = 0.4;
/// Height that a jump onto a tab clears above it.
const STEP_CLEARANCE: f32 = 13.;
/// A difference in height ahead that counts as a step, in points.
const STEP: f32 = 5.;
/// Seconds of the squat before a jump, of the squash after a landing and of a turn.
const CROUCH: f32 = 0.1;
const LANDING: f32 = 0.14;
const TURN: f32 = 0.42;
/// Seconds that the character idles before it sits down and dozes.
const DOZE_AFTER: f32 = 14.;
/// Frames per second of the art while the character idles. Its motion follows the display's
/// refreshes.
const IDLE_RATE: f32 = 12.;
/// Shortest time between two frames of the character's motion. Each frame draws the whole
/// window again, so on a display faster than 90 Hz the motion takes every second refresh.
const SMOOTH_FRAME: Duration = Duration::from_millis(11);

/// What the workspace shows of the character this frame.
#[derive(Clone, PartialEq)]
pub(super) struct Cue {
    /// The open task. Another task starts the character over.
    pub key: SharedString,
    pub avatar: SkaterAvatar,
    pub tone: Hsla,
    /// The task's agent is working, so the character skates.
    pub working: bool,
    /// The conversation shows its end. Away from it the character falls off.
    pub following: bool,
    /// A menu or dialog lies over the composer, so the character pauses unseen.
    pub covered: bool,
}

/// A part of the composer that the character rides on.
#[derive(Clone, Copy)]
pub(super) enum Part {
    /// The composer's box, whose top edge is the ground.
    Edge,
    /// A tab that rises from the edge at its leading or trailing end. `foot` is the run before
    /// its slanted side starts and `slant` the run of that side.
    Ledge {
        leading: bool,
        foot: Pixels,
        slant: Pixels,
    },
}

/// A tab on the edge, in window pixels.
#[derive(Clone, Copy)]
struct Ledge {
    left: f32,
    right: f32,
    top: f32,
    leading: bool,
    foot: f32,
    slant: f32,
}

impl Ledge {
    // Returns the top of the tab at `x`, or `base` beside it.
    fn height(&self, x: f32, base: f32) -> f32 {
        if x < self.left || x > self.right {
            return base;
        }
        // `low` is where the slant leaves the edge and `high` where it reaches the tab's top.
        let (low, high) = if self.leading {
            let low = self.right - self.foot;
            (low, low - self.slant)
        } else {
            let low = self.left + self.foot;
            (low, low + self.slant)
        };
        if (high - low).abs() < 0.5 {
            return if (x - low) * (high - low).signum() >= 0. {
                self.top
            } else {
                base
            };
        }
        let rise = ((x - low) / (high - low)).clamp(0., 1.);
        base + (self.top - base) * rise
    }
}

/// The ground under the character: the composer's edge with its tabs, in window pixels.
#[derive(Clone, Copy)]
struct Ground {
    left: f32,
    right: f32,
    base: f32,
    ledges: [Option<Ledge>; 2],
}

impl Ground {
    // Returns the top of the ground at `x`.
    fn height(&self, x: f32) -> f32 {
        self.ledges
            .iter()
            .flatten()
            .fold(self.base, |y, ledge| y.min(ledge.height(x, self.base)))
    }
}

/// The scene as the workspace described it in its last render.
#[derive(Default)]
struct Scene {
    cue: Option<Cue>,
    edge: Option<Bounds<Pixels>>,
    ledges: Vec<Ledge>,
}

impl Scene {
    fn ground(&self) -> Option<Ground> {
        let edge = self.edge?;
        let mut ledges = [None; 2];
        for (slot, ledge) in ledges.iter_mut().zip(&self.ledges) {
            *slot = Some(*ledge);
        }
        Some(Ground {
            left: f32::from(edge.left()),
            right: f32::from(edge.right()),
            base: f32::from(edge.top()),
            ledges,
        })
    }
}

#[derive(Default)]
struct Shared(Rc<RefCell<Scene>>);

impl Global for Shared {}

fn shared(cx: &mut App) -> Rc<RefCell<Scene>> {
    if !cx.has_global::<Shared>() {
        cx.set_global(Shared::default());
    }
    cx.global::<Shared>().0.clone()
}

/// Clears the scene. Call it once before the workspace renders.
pub(super) fn begin_frame(cx: &mut App) {
    let scene = shared(cx);
    let mut scene = scene.borrow_mut();
    scene.cue = None;
    scene.edge = None;
    scene.ledges.clear();
}

/// Shows the character on the composer of this frame.
pub(super) fn cue(cue: Cue, cx: &mut App) {
    shared(cx).borrow_mut().cue = Some(cue);
}

/// An invisible element that records its parent's bounds as `part` of the ground on every
/// frame. The parent must be positioned (`relative` or `absolute`).
pub(super) fn ground(part: Part) -> impl IntoElement {
    canvas(
        move |bounds, _, cx| {
            let scene = shared(cx);
            let mut scene = scene.borrow_mut();
            match part {
                Part::Edge => scene.edge = Some(bounds),
                Part::Ledge {
                    leading,
                    foot,
                    slant,
                } => scene.ledges.push(Ledge {
                    left: f32::from(bounds.left()),
                    right: f32::from(bounds.right()),
                    top: f32::from(bounds.top()),
                    leading,
                    foot: f32::from(foot),
                    slant: f32::from(slant),
                }),
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// Off the screen, waiting to come back.
    Hidden,
    /// Leaping up from below the window onto the edge.
    Enter,
    Idle,
    /// Sitting on the board after a long idle.
    Doze,
    /// Standing up from a doze.
    Wake,
    Ride,
    /// A kick turn at an end of the edge.
    Turn,
    /// The squat before a jump.
    Crouch,
    Air,
    /// The squash after a landing.
    Land,
    /// Tumbling after a failed landing.
    Bail,
    Dizzy,
    /// Getting up after a bail.
    Recover,
    /// Walking back to the board.
    Walk,
    /// Hopping back onto the board.
    Mount,
    /// Falling off because the conversation scrolled away.
    Panic,
    /// Hanging from the pointer.
    Held,
    /// Thrown by the pointer.
    Flung,
    /// A zombie's dance on the spot.
    Dance,
}

impl Mode {
    // Reports a mode in which the character is on the edge or near it, so it falls off when the
    // conversation scrolls away.
    fn on_stage(self) -> bool {
        !matches!(self, Self::Hidden | Self::Panic | Self::Held | Self::Flung)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Trick {
    Ollie,
    Kickflip,
    Shuvit,
    /// The character spins about itself with the board.
    Spin,
    /// The front hand grabs the board.
    Grab,
    /// A sleigh's long, floating flight while Santa waves.
    Soar,
    /// A sleigh's loop about the middle of its team.
    Loop,
    /// A soar in which Santa drops a present at the top.
    Gift,
}

/// A zombie's dance.
#[derive(Clone, Copy, PartialEq)]
enum Dance {
    Headspin,
    Windmill,
    /// Faces ahead and glides back.
    Moonwalk,
    /// Throws its head up and catches it on its neck.
    HeadToss,
}

impl Dance {
    // Returns the seconds that the dance lasts.
    fn length(self) -> f32 {
        match self {
            Self::Headspin => 1.8,
            Self::Windmill => 1.6,
            Self::Moonwalk => 2.,
            Self::HeadToss => 1.4,
        }
    }
}

impl Ride {
    // Returns the cells from the middle of the grid that the ride keeps from the ends of the edge.
    // The reindeer run far ahead of the sleigh, and either end can be ahead after a turn.
    fn margin(self) -> f32 {
        match self {
            Self::Board => 9.,
            Self::Sleigh => 24.,
            Self::Feet => 6.,
        }
    }

    // Returns the cells from the middle to where the ride touches the ground on each side.
    fn base(self) -> f32 {
        match self {
            Self::Board => 4.,
            Self::Sleigh => 8.,
            Self::Feet => 2.,
        }
    }

    // Returns the cells from the middle to the front of the ride, which meets a step first.
    fn front(self) -> f32 {
        match self {
            Self::Board => 4.,
            Self::Sleigh => 23.,
            Self::Feet => 3.,
        }
    }

    // Returns the rows from the feet line down to the ground.
    fn depth(self) -> f32 {
        match self {
            Self::Board | Self::Sleigh => art::DECK_DEPTH,
            Self::Feet => 0.,
        }
    }

    // Returns the points of flat edge ahead that a trick needs.
    fn room(self) -> f32 {
        match self {
            Self::Board => 70.,
            Self::Sleigh => 110.,
            Self::Feet => 0.,
        }
    }
}

/// A moving thing: the character, at the middle of its feet line, or the board, at the middle of
/// its deck's top.
#[derive(Clone, Copy, Default)]
struct Body {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    angle: f32,
    spin: f32,
}

impl Body {
    fn fly(&mut self, gravity: f32, dt: f32) {
        self.vy += gravity * dt;
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.angle += self.spin * dt;
    }
}

#[derive(Clone, Copy)]
enum Speck {
    Dust,
    Snore,
    /// A twinkle behind a flying sleigh.
    Sparkle,
    /// A present that Santa drops.
    Gift,
}

#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    kind: Speck,
}

/// The pointer's hold on the character.
struct Grip {
    /// Where the pointer is. The character hangs from it by the scruff.
    pivot: Point<f32>,
    /// The pointer's place and speed at the last step, for its acceleration.
    last_pivot: Point<f32>,
    last_speed: f32,
    /// The swing's angle from hanging straight down, and its speed.
    swing: f32,
    swing_speed: f32,
    /// Recent pointer places, for the speed of a throw.
    samples: VecDeque<(Instant, Point<f32>)>,
}

/// How often the layer needs to paint again.
#[derive(Clone, Copy, PartialEq)]
enum Cadence {
    /// Nothing shows and nothing waits for time to pass.
    Rest,
    /// The character shows and stands still.
    Still,
    /// The art changes a few times per second.
    Slow,
    /// The character moves.
    Smooth,
}

/// The character's state between frames.
struct Sim {
    mode: Mode,
    /// Seconds in the current mode.
    clock: f32,
    /// Seconds since the layer started, for blinks and breaths.
    time: f32,
    me: Body,
    board: Body,
    /// What carries the character, from its avatar.
    ride: Ride,
    /// The board is under the feet. Otherwise it moves on its own.
    carried: bool,
    facing: f32,
    trick: Trick,
    dance: Dance,
    /// Seconds before the next sparkle behind a flying sleigh.
    trail: f32,
    /// Expected seconds in the air of the current jump.
    air: f32,
    /// Height that the next jump reaches and the rise of the ground where it lands.
    lift: f32,
    rise: f32,
    /// The current jump ends in a bail.
    fail: bool,
    /// Seconds left of a manual, or of cheering after a trick.
    manual: f32,
    cheer: f32,
    /// Seconds into the current push, below zero while coasting.
    push: f32,
    next_trick: f32,
    next_look: f32,
    next_blink: f32,
    /// Seconds before the next decision about a drop ahead.
    drop_wait: f32,
    bounced: bool,
    /// Seconds hidden before the character may come back.
    respawn: f32,
    key: Option<SharedString>,
    avatar: SkaterAvatar,
    tone: Hsla,
    grip: Option<Grip>,
    particles: Vec<Particle>,
    rng: u32,
    last: Option<Instant>,
    /// The interface scale. Distances and speeds follow it.
    scale: f32,
}

impl Sim {
    fn new() -> Self {
        Self {
            mode: Mode::Hidden,
            clock: 0.,
            time: 0.,
            me: Body::default(),
            board: Body::default(),
            ride: Ride::Board,
            carried: true,
            facing: 1.,
            trick: Trick::Ollie,
            dance: Dance::Headspin,
            trail: 0.,
            air: 0.5,
            lift: 0.,
            rise: 0.,
            fail: false,
            manual: 0.,
            cheer: 0.,
            push: -1.,
            next_trick: 3.,
            next_look: 4.,
            next_blink: 2.,
            drop_wait: 0.,
            bounced: false,
            respawn: 0.3,
            key: None,
            avatar: SkaterAvatar::default(),
            tone: hsla(0., 0., 0.6, 1.),
            grip: None,
            particles: Vec::new(),
            rng: (uuid::Uuid::new_v4().as_u128() as u32) | 1,
            last: None,
            scale: 1.,
        }
    }

    // Returns a number from 0 to 1.
    fn random(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.random()
    }

    fn pixel(&self) -> f32 {
        PIXEL * self.scale
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.clock = 0.;
    }

    // Returns where the deck's top rests at `x` and the tilt of the board there. The board lies
    // on the ground under both of its wheels.
    fn rest(&self, ground: &Ground, x: f32) -> (f32, f32) {
        let reach = self.ride.base() * self.pixel();
        let (left, right) = (ground.height(x - reach), ground.height(x + reach));
        (
            (left + right) / 2. - self.ride.depth() * self.pixel(),
            (right - left).atan2(2. * reach),
        )
    }

    // Returns the ends of the edge that the middle of the board stays between.
    fn lane(&self, ground: &Ground) -> (f32, f32) {
        let margin = self.ride.margin() * self.pixel();
        (ground.left + margin, ground.right - margin)
    }

    // Returns the pull on the character in a jump. A sleigh flies, so it floats.
    fn air_gravity(&self) -> f32 {
        let float = if self.ride == Ride::Sleigh {
            SLEIGH_FLOAT
        } else {
            1.
        };
        GRAVITY * self.scale * float
    }

    fn hide(&mut self, respawn: f32) {
        self.set_mode(Mode::Hidden);
        self.respawn = respawn;
        self.carried = true;
        self.grip = None;
        self.me.angle = 0.;
        self.me.spin = 0.;
    }

    // Advances the character to `now` and reports how soon it needs another frame. `Rest`
    // means that nothing shows.
    fn advance(
        &mut self,
        now: Instant,
        cue: Option<&Cue>,
        ground: Option<Ground>,
        viewport: Size<Pixels>,
        reduce_motion: bool,
    ) -> Cadence {
        let dt = self
            .last
            .map_or(0., |last| now.duration_since(last).as_secs_f32())
            .min(0.05);
        self.last = Some(now);
        self.scale = crate::theme::ui_scale();
        let (Some(cue), Some(ground)) = (cue, ground) else {
            self.key = None;
            self.hide(0.3);
            return Cadence::Rest;
        };
        if cue.covered {
            // The clock stops while the character is unseen.
            self.last = None;
            return Cadence::Rest;
        }
        self.avatar = cue.avatar;
        self.tone = cue.tone;
        let ride = art::art(cue.avatar).ride;
        // Another ride starts the character over, as the old one's state does not fit it.
        if self.key.as_ref() != Some(&cue.key) || self.ride != ride {
            self.key = Some(cue.key.clone());
            self.ride = ride;
            self.hide(0.3);
        }
        let (low, high) = self.lane(&ground);
        if high - low < 24. * self.pixel() {
            self.hide(0.3);
            return Cadence::Rest;
        }
        if reduce_motion {
            return self.stand_still(cue, &ground);
        }
        self.time += dt;
        self.clock += dt;
        self.next_blink -= dt;
        if self.next_blink < -0.12 {
            self.next_blink = self.range(1.8, 4.8);
        }
        self.step_particles(dt);
        if !cue.following && self.mode.on_stage() {
            self.panic();
        }
        let view = (f32::from(viewport.width), f32::from(viewport.height));
        match self.mode {
            Mode::Hidden => {
                if !cue.following {
                    return Cadence::Rest;
                }
                if self.clock < self.respawn {
                    return Cadence::Slow;
                }
                self.enter(&ground, view.1);
            }
            Mode::Enter => self.step_enter(&ground, dt),
            Mode::Idle | Mode::Doze | Mode::Wake => self.step_idle(cue, &ground, dt),
            Mode::Ride | Mode::Turn | Mode::Crouch | Mode::Land => self.step_ride(cue, &ground, dt),
            Mode::Air => self.step_air(&ground, dt),
            Mode::Bail | Mode::Dizzy | Mode::Recover | Mode::Walk | Mode::Mount => {
                self.step_bail(&ground, dt)
            }
            Mode::Panic | Mode::Flung => self.step_fall(view, dt),
            Mode::Held => self.step_held(dt),
            Mode::Dance => self.step_dance(cue, &ground, dt),
        }
        if !self.carried {
            self.step_board(&ground, view, dt);
        }
        if self.ride == Ride::Sleigh && self.carried && matches!(self.mode, Mode::Enter | Mode::Air)
        {
            self.trail -= dt;
            if self.trail <= 0. {
                self.trail = 0.05;
                self.sparkle();
            }
        }
        // A rising z is slow enough for the idle rate. Other specks need the display rate.
        let dusty = self
            .particles
            .iter()
            .any(|particle| !matches!(particle.kind, Speck::Snore));
        match self.mode {
            Mode::Hidden => Cadence::Slow,
            Mode::Idle | Mode::Doze if !dusty => Cadence::Slow,
            _ => Cadence::Smooth,
        }
    }

    // Keeps the character standing on the edge without motion, for reduced motion.
    fn stand_still(&mut self, cue: &Cue, ground: &Ground) -> Cadence {
        if !cue.following {
            self.hide(0.);
            return Cadence::Rest;
        }
        let (low, high) = self.lane(ground);
        if self.mode != Mode::Idle {
            self.set_mode(Mode::Idle);
            self.me.x = low + (high - low) * 0.3;
            self.facing = 1.;
        }
        self.me.x = self.me.x.clamp(low, high);
        let (y, tilt) = self.rest(ground, self.me.x);
        self.me.y = y;
        self.me.angle = tilt;
        self.me.vx = 0.;
        self.carried = true;
        self.grip = None;
        self.next_blink = 1.;
        self.particles.clear();
        Cadence::Still
    }

    fn enter(&mut self, ground: &Ground, bottom: f32) {
        let (low, high) = self.lane(ground);
        let x = self.range(low + (high - low) * 0.15, high - (high - low) * 0.15);
        self.facing = if x < (low + high) / 2. { 1. } else { -1. };
        let (floor, _) = self.rest(ground, x);
        let apex = floor - 30. * self.scale;
        let start = bottom + 16. * self.pixel();
        let gravity = self.air_gravity();
        let launch = (2. * gravity * (start - apex).max(1.)).sqrt();
        let flight = launch / gravity + (2. * (floor - apex) / gravity).sqrt();
        self.me = Body {
            x,
            y: start,
            vx: 0.,
            vy: -launch,
            angle: 0.,
            // A skater flips once on the way up and down. A sleigh and a zombie rise upright.
            spin: if self.ride == Ride::Board {
                self.facing * TAU / flight
            } else {
                0.
            },
        };
        self.carried = true;
        self.fail = false;
        self.cheer = 0.;
        self.manual = 0.;
        self.push = -1.;
        self.particles.clear();
        self.next_trick = self.range(2., 4.);
        self.set_mode(Mode::Enter);
    }

    fn step_enter(&mut self, ground: &Ground, dt: f32) {
        self.me.fly(self.air_gravity(), dt);
        let (floor, tilt) = self.rest(ground, self.me.x);
        if self.me.vy > 0. && self.me.y >= floor {
            self.me.y = floor;
            self.me.vy = 0.;
            self.me.spin = 0.;
            self.me.angle = tilt;
            self.dust(self.me.x, floor + self.ride.depth() * self.pixel(), 5);
            self.cheer = 0.5;
            self.set_mode(Mode::Land);
        }
    }

    fn step_idle(&mut self, cue: &Cue, ground: &Ground, dt: f32) {
        let (low, high) = self.lane(ground);
        self.me.x = self.me.x.clamp(low, high);
        let (y, tilt) = self.rest(ground, self.me.x);
        self.me.y = y;
        self.me.angle = tilt;
        self.me.vx = 0.;
        match self.mode {
            Mode::Idle => {
                if cue.working {
                    self.start_ride();
                    return;
                }
                // The character looks around now and then.
                self.next_look -= dt;
                if self.next_look <= 0. {
                    self.facing = -self.facing;
                    self.next_look = self.range(3., 7.);
                }
                if self.clock >= DOZE_AFTER {
                    self.set_mode(Mode::Doze);
                }
            }
            Mode::Doze => {
                if cue.working {
                    self.set_mode(Mode::Wake);
                    return;
                }
                // A z rises now and then.
                if (self.clock / 1.6).floor() != ((self.clock - dt) / 1.6).floor() {
                    let pixel = self.pixel();
                    self.particles.push(Particle {
                        x: self.me.x + self.facing * 4. * pixel,
                        y: self.me.y - 12. * pixel,
                        vx: self.facing * 5. * self.scale,
                        vy: -12. * self.scale,
                        age: 0.,
                        life: 2.2,
                        kind: Speck::Snore,
                    });
                }
            }
            _ => {
                if self.clock >= 0.45 {
                    if cue.working {
                        self.start_ride();
                    } else {
                        self.set_mode(Mode::Idle);
                    }
                }
            }
        }
    }

    fn start_ride(&mut self) {
        self.push = 0.;
        self.next_trick = self.range(1.5, 3.5);
        self.set_mode(Mode::Ride);
    }

    fn step_ride(&mut self, cue: &Cue, ground: &Ground, dt: f32) {
        self.cheer = (self.cheer - dt).max(0.);
        match self.mode {
            Mode::Turn => {
                let half = TURN / 2.;
                if self.clock >= half && self.clock - dt < half {
                    self.facing = -self.facing;
                }
                self.me.vx = 0.;
                if self.clock >= TURN {
                    if cue.working {
                        self.me.vx = self.facing * 45. * self.scale;
                        self.push = 0.;
                        self.set_mode(Mode::Ride);
                    } else {
                        self.set_mode(Mode::Idle);
                    }
                }
            }
            Mode::Crouch => {
                if self.clock >= CROUCH {
                    self.take_off();
                    return;
                }
            }
            Mode::Land => {
                if self.clock >= LANDING {
                    self.set_mode(Mode::Ride);
                }
            }
            _ => self.cruise(cue, ground, dt),
        }
        if !matches!(
            self.mode,
            Mode::Ride | Mode::Turn | Mode::Crouch | Mode::Land
        ) {
            return;
        }
        let (low, high) = self.lane(ground);
        self.me.x = (self.me.x + self.me.vx * dt).clamp(low, high);
        let (y, tilt) = self.rest(ground, self.me.x);
        self.me.y = y;
        self.me.angle = tilt;
    }

    // Decides how the character rolls on: pushing, braking, turning at an end, jumping a step
    // or doing a trick.
    fn cruise(&mut self, cue: &Cue, ground: &Ground, dt: f32) {
        let scale = self.scale;
        let pixel = self.pixel();
        let (low, high) = self.lane(ground);
        let speed = self.me.vx.abs();
        self.manual = (self.manual - dt).max(0.);
        self.drop_wait = (self.drop_wait - dt).max(0.);
        if self.ride == Ride::Feet {
            // A zombie shambles at one pace and stops at once.
            if !cue.working {
                self.me.vx = 0.;
                self.set_mode(Mode::Idle);
                return;
            }
            self.push = -1.;
            self.me.vx = self.facing * SHAMBLE * scale;
        } else if cue.working {
            // Push while slow, then coast.
            if self.push >= 0. {
                self.push += dt;
                if self.push >= 0.9 || speed > CRUISE * scale * 1.05 {
                    self.push = -1.;
                }
            } else if speed < CRUISE * scale * 0.7 && self.manual <= 0. {
                self.push = 0.;
            }
            let accel = if self.push >= 0. {
                PUSH * scale
            } else {
                -ROLL_FRICTION * scale
            };
            self.me.vx = self.facing * (speed + accel * dt).max(0.);
        } else {
            self.push = -1.;
            self.manual = 0.;
            // The board does not stop on a slant.
            let on_slope = self.me.angle.abs() > 0.05;
            let slowed = (speed - BRAKE * scale * dt).max(if on_slope { 20. * scale } else { 0. });
            if slowed < 4. * scale {
                self.me.vx = 0.;
                self.set_mode(Mode::Idle);
                return;
            }
            self.me.vx = self.facing * slowed;
        }
        let end = if self.facing > 0. { high } else { low };
        if (end - self.me.x) * self.facing <= 1. {
            self.me.x = end;
            self.manual = 0.;
            self.push = -1.;
            self.set_mode(Mode::Turn);
            return;
        }
        let speed = self.me.vx.abs();
        let front = self.ride.front();
        let here = ground.height(self.me.x);
        let ahead = ground.height(self.me.x + self.facing * (front * pixel + speed * 0.15));
        // Above zero the ground rises ahead.
        let step = here - ahead;
        if step > STEP * scale {
            if !cue.working {
                self.me.vx = 0.;
                self.set_mode(Mode::Idle);
                return;
            }
            // The jump clears the whole tab, not only the start of its slant.
            let top = (1..=24 + front as i32)
                .map(|i| ground.height(self.me.x + self.facing * i as f32 * pixel))
                .fold(here, f32::min);
            let rise = here - top;
            self.manual = 0.;
            self.me.vx = self.facing * speed.max(80. * scale);
            self.plan_jump(Trick::Ollie, rise + STEP_CLEARANCE * scale, rise, 0.03);
            return;
        }
        // Off a tab a skater rolls down its slant or jumps it with a trick, a sleigh rolls or
        // glides down, and a zombie walks down.
        if step < -STEP * scale && self.drop_wait <= 0. && cue.working && self.ride != Ride::Feet {
            self.drop_wait = 1.2;
            if self.random() < 0.55 {
                let trick = if self.ride == Ride::Sleigh {
                    Trick::Soar
                } else {
                    self.pick_trick()
                };
                self.plan_jump(trick, OLLIE_LIFT * scale, step, 0.07);
                return;
            }
        }
        if !cue.working {
            return;
        }
        self.next_trick -= dt;
        let room = (end - self.me.x) * self.facing;
        let flat = (1..=4).all(|i| {
            (ground.height(self.me.x + self.facing * i as f32 * 14. * scale) - here).abs() < 1.
        });
        if self.next_trick <= 0. && room > self.ride.room() * scale && flat && self.manual <= 0. {
            self.next_trick = self.range(2.2, 5.5);
            if self.ride == Ride::Feet {
                self.start_dance(high - low - room);
            } else if self.ride == Ride::Board && self.random() < 0.15 {
                self.manual = self.range(0.8, 1.4);
                self.push = -1.;
            } else {
                let trick = self.pick_trick();
                let (lift, risk) = match trick {
                    Trick::Ollie => (OLLIE_LIFT, 0.02),
                    Trick::Soar | Trick::Gift => (SOAR_LIFT, 0.03),
                    Trick::Loop => (LOOP_LIFT, 0.05),
                    _ => (TRICK_LIFT, 0.07),
                };
                self.plan_jump(trick, lift * scale, 0., risk);
            }
        }
    }

    fn pick_trick(&mut self) -> Trick {
        const TRICKS: [Trick; 7] = [
            Trick::Ollie,
            Trick::Kickflip,
            Trick::Kickflip,
            Trick::Shuvit,
            Trick::Shuvit,
            Trick::Spin,
            Trick::Grab,
        ];
        const STUNTS: [Trick; 5] = [
            Trick::Ollie,
            Trick::Soar,
            Trick::Soar,
            Trick::Loop,
            Trick::Gift,
        ];
        let tricks: &[Trick] = if self.ride == Ride::Sleigh {
            &STUNTS
        } else {
            &TRICKS
        };
        tricks[(self.random() * tricks.len() as f32) as usize % tricks.len()]
    }

    // Stops to dance with `behind` points of edge behind, which a moonwalk needs.
    fn start_dance(&mut self, behind: f32) {
        const DANCES: [Dance; 4] = [
            Dance::Headspin,
            Dance::Windmill,
            Dance::Moonwalk,
            Dance::HeadToss,
        ];
        let mut dance = DANCES[(self.random() * DANCES.len() as f32) as usize % DANCES.len()];
        if dance == Dance::Moonwalk && behind < MOONWALK * self.scale * dance.length() {
            dance = Dance::Headspin;
        }
        self.dance = dance;
        // A spin on the floor ends in a tumble now and then.
        self.fail = matches!(dance, Dance::Headspin | Dance::Windmill) && self.random() < 0.08;
        self.me.vx = 0.;
        self.push = -1.;
        self.set_mode(Mode::Dance);
    }

    fn step_dance(&mut self, cue: &Cue, ground: &Ground, dt: f32) {
        let (low, high) = self.lane(ground);
        if self.dance == Dance::Moonwalk {
            self.me.x -= self.facing * MOONWALK * self.scale * dt;
        }
        self.me.x = self.me.x.clamp(low, high);
        let (y, tilt) = self.rest(ground, self.me.x);
        self.me.y = y;
        self.me.angle = tilt;
        self.me.vx = 0.;
        if self.clock < self.dance.length() {
            return;
        }
        if self.fail {
            self.bail();
            return;
        }
        self.cheer = 0.6;
        self.set_mode(if cue.working { Mode::Ride } else { Mode::Idle });
    }

    // Squats for a jump that reaches `lift` and lands on ground that is `rise` higher. A share
    // of `risk` of the jumps end in a bail.
    fn plan_jump(&mut self, trick: Trick, lift: f32, rise: f32, risk: f32) {
        self.trick = trick;
        self.lift = lift;
        self.rise = rise;
        self.fail = self.random() < risk;
        self.push = -1.;
        self.set_mode(Mode::Crouch);
    }

    fn take_off(&mut self) {
        let gravity = self.air_gravity();
        let launch = (2. * gravity * self.lift.max(1.)).sqrt();
        self.me.vy = -launch;
        self.air = launch / gravity + (2. * (self.lift - self.rise).max(1.) / gravity).sqrt();
        self.set_mode(Mode::Air);
    }

    fn step_air(&mut self, ground: &Ground, dt: f32) {
        let (low, high) = self.lane(ground);
        let rising = self.me.vy < 0.;
        self.me.fly(self.air_gravity(), dt);
        if rising && self.me.vy >= 0. && self.trick == Trick::Gift {
            self.drop_gift();
        }
        if self.me.x < low || self.me.x > high {
            self.me.x = self.me.x.clamp(low, high);
            self.me.vx = 0.;
        }
        let (floor, tilt) = self.rest(ground, self.me.x);
        if self.me.vy > 0. && self.me.y >= floor {
            self.me.y = floor;
            self.me.vy = 0.;
            self.me.angle = tilt;
            if self.fail {
                self.bail();
                return;
            }
            self.dust(self.me.x, floor + self.ride.depth() * self.pixel(), 4);
            self.cheer = if self.trick == Trick::Ollie { 0. } else { 0.6 };
            self.set_mode(Mode::Land);
        }
    }

    // Lets the board or the sleigh go on its own as `board`. A character on its feet has none.
    fn let_go(&mut self, board: Body) {
        if self.ride == Ride::Feet {
            return;
        }
        self.carried = false;
        self.board = board;
    }

    fn bail(&mut self) {
        let scale = self.scale;
        self.let_go(Body {
            x: self.me.x,
            y: self.me.y,
            vx: self.me.vx * 1.3 + self.facing * 20. * scale,
            vy: -110. * scale,
            angle: self.me.angle,
            spin: -self.facing * 7.,
        });
        self.me.vy = -150. * scale;
        self.me.vx *= 0.7;
        self.me.spin = self.facing * 10.;
        self.bounced = false;
        self.cheer = 0.;
        self.manual = 0.;
        self.set_mode(Mode::Bail);
    }

    // Tumbles after a bail, lies dizzy, gets up and walks back to the board.
    fn step_bail(&mut self, ground: &Ground, dt: f32) {
        let scale = self.scale;
        let pixel = self.pixel();
        let (low, high) = self.lane(ground);
        // A lying character turns about its feet, so its middle rests this far above them.
        let lying_lift = 6. * pixel;
        let lying = -self.facing * PI / 2.;
        match self.mode {
            Mode::Bail => {
                self.me.fly(GRAVITY * scale, dt);
                if self.me.x < low || self.me.x > high {
                    self.me.x = self.me.x.clamp(low, high);
                    self.me.vx = -self.me.vx * 0.3;
                }
                let floor = ground.height(self.me.x) - lying_lift;
                if self.me.vy >= 0. && self.me.y >= floor {
                    self.me.y = floor;
                    if !self.bounced && self.me.vy > 60. * scale {
                        self.bounced = true;
                        self.me.vy = -self.me.vy * 0.3;
                        self.me.spin *= 0.5;
                        self.dust(self.me.x, floor + lying_lift, 4);
                    } else {
                        self.me.vy = 0.;
                        self.me.spin = 0.;
                        self.me.angle += wrap(lying - self.me.angle) * (dt * 12.).min(1.);
                        let slowed = (self.me.vx.abs() - 300. * scale * dt).max(0.);
                        self.me.vx = self.me.vx.signum() * slowed;
                        if slowed <= 0. {
                            self.me.angle = lying;
                            self.set_mode(Mode::Dizzy);
                        }
                    }
                }
            }
            Mode::Dizzy => {
                self.me.y = ground.height(self.me.x) - lying_lift;
                if self.clock >= 1.4 {
                    self.set_mode(Mode::Recover);
                }
            }
            Mode::Recover => {
                let share = (self.clock / 0.4).min(1.);
                self.me.angle = lying * (1. - share);
                self.me.y = ground.height(self.me.x) - lying_lift * (1. - share);
                if share >= 1. {
                    self.me.angle = 0.;
                    // A zombie has no board to walk back to.
                    self.set_mode(if self.ride == Ride::Feet {
                        Mode::Idle
                    } else {
                        Mode::Walk
                    });
                }
            }
            Mode::Walk => {
                let gap = self.board.x - self.me.x;
                let settled = self.board.vy == 0. && self.board.vx.abs() < 6. * scale;
                if gap.abs() > 2. * pixel {
                    self.facing = gap.signum();
                    self.me.x += self.facing * (WALK * scale * dt).min(gap.abs());
                } else if settled {
                    self.me.vy = -170. * scale;
                    self.me.vx = 0.;
                    self.set_mode(Mode::Mount);
                }
                self.me.y = ground.height(self.me.x);
            }
            _ => {
                self.me.fly(GRAVITY * scale, dt);
                self.me.x += (self.board.x - self.me.x) * (dt * 10.).min(1.);
                let (floor, _) = self.rest(ground, self.me.x);
                if self.me.vy > 0. && self.me.y >= floor {
                    let (floor, tilt) = self.rest(ground, self.board.x);
                    self.me = Body {
                        x: self.board.x,
                        y: floor,
                        angle: tilt,
                        ..Body::default()
                    };
                    self.carried = true;
                    self.dust(self.me.x, floor + self.ride.depth() * pixel, 3);
                    self.set_mode(Mode::Land);
                }
            }
        }
    }

    // Moves the board while it is off the feet: it flies, lands and rolls to a stop on the
    // edge, or falls out of the window with the character.
    fn step_board(&mut self, ground: &Ground, view: (f32, f32), dt: f32) {
        let scale = self.scale;
        let pixel = self.pixel();
        let mut board = self.board;
        if matches!(self.mode, Mode::Panic | Mode::Flung | Mode::Held) {
            board.fly(GRAVITY * scale, dt);
            if self.mode == Mode::Flung {
                bounce(&mut board, view, 7. * pixel);
            }
            self.board = board;
            return;
        }
        let (low, high) = self.lane(ground);
        let (floor, tilt) = self.rest(ground, board.x);
        if board.vy != 0. || board.y < floor - 0.5 {
            board.fly(GRAVITY * scale, dt);
            if board.vy > 0. && board.y >= floor {
                board.y = floor;
                if board.vy > 120. * scale {
                    board.vy = -board.vy * 0.3;
                    board.spin *= 0.5;
                } else {
                    board.vy = 0.;
                    board.spin = 0.;
                    board.angle = tilt;
                }
            }
        } else {
            board.y = floor;
            board.angle = tilt;
            let slowed = (board.vx.abs() - 70. * scale * dt).max(0.);
            board.vx = board.vx.signum() * slowed;
            board.x += board.vx * dt;
        }
        if board.x < low {
            board.x = low;
            board.vx = board.vx.abs() * 0.4;
        } else if board.x > high {
            board.x = high;
            board.vx = -board.vx.abs() * 0.4;
        }
        self.board = board;
    }

    // Startles and falls off the edge, out of the bottom of the window.
    fn panic(&mut self) {
        let scale = self.scale;
        let spin = self.range(-9., 9.);
        if self.carried {
            let drift = self.range(-40., 40.) * scale;
            self.let_go(Body {
                x: self.me.x,
                y: self.me.y,
                vx: self.me.vx * 0.5 + drift,
                vy: -140. * scale,
                angle: self.me.angle,
                spin,
            });
        } else {
            self.board.vy = -120. * scale;
            self.board.spin = spin;
        }
        self.me.vy = -240. * scale;
        self.me.vx = self.facing * 30. * scale;
        self.me.spin = 0.;
        self.cheer = 0.;
        self.manual = 0.;
        self.set_mode(Mode::Panic);
    }

    fn step_fall(&mut self, view: (f32, f32), dt: f32) {
        let pixel = self.pixel();
        if self.mode == Mode::Panic && self.clock > 0.3 && self.me.spin == 0. {
            self.me.spin = -self.facing * 3.;
        }
        self.me.fly(GRAVITY * self.scale, dt);
        let flung = self.mode == Mode::Flung;
        if flung {
            bounce(&mut self.me, view, 7. * pixel);
        }
        let gone = self.me.y - 16. * pixel > view.1
            && (self.carried || self.board.y - 4. * pixel > view.1);
        if gone {
            self.hide(if flung { 1.4 } else { 0.5 });
        }
    }

    fn grab(&mut self, at: Point<Pixels>, now: Instant) {
        let pivot = point(f32::from(at.x), f32::from(at.y));
        self.grip = Some(Grip {
            pivot,
            last_pivot: pivot,
            last_speed: 0.,
            swing: 0.,
            swing_speed: 0.,
            samples: VecDeque::from([(now, pivot)]),
        });
        self.cheer = 0.;
        self.manual = 0.;
        self.push = -1.;
        self.set_mode(Mode::Held);
    }

    fn drag(&mut self, at: Point<Pixels>, now: Instant) {
        let Some(grip) = &mut self.grip else {
            return;
        };
        grip.pivot = point(f32::from(at.x), f32::from(at.y));
        grip.samples.push_back((now, grip.pivot));
        while grip.samples.len() > 2
            && grip
                .samples
                .front()
                .is_some_and(|(at, _)| now.duration_since(*at) > Duration::from_millis(100))
        {
            grip.samples.pop_front();
        }
    }

    // Lets go of the character, which flies on at the pointer's last speed.
    fn release(&mut self, now: Instant) {
        let Some(grip) = self.grip.take() else {
            return;
        };
        let scale = self.scale;
        let (mut vx, mut vy) = (0., 0.);
        if let (Some((first_at, first)), Some((last_at, last))) =
            (grip.samples.front(), grip.samples.back())
        {
            let elapsed = last_at.duration_since(*first_at).as_secs_f32();
            let fresh = now.duration_since(*last_at) < Duration::from_millis(100);
            if elapsed > 0.008 && fresh {
                vx = (last.x - first.x) / elapsed;
                vy = (last.y - first.y) / elapsed;
            }
        }
        let fastest = 2800. * scale;
        let speed = (vx * vx + vy * vy).sqrt();
        if speed > fastest {
            vx *= fastest / speed;
            vy *= fastest / speed;
        }
        self.me.vx = vx;
        self.me.vy = vy;
        self.me.spin = (-grip.swing_speed * 0.6 + vx * 0.005).clamp(-16., 16.);
        if self.carried {
            let drift = self.range(-30., 30.) * scale;
            self.let_go(Body {
                x: self.me.x,
                y: self.me.y,
                vx: vx * 0.85 + drift,
                vy: vy * 0.85 - 60. * scale,
                angle: self.me.angle,
                spin: -self.me.spin * 0.8 - self.facing * 4.,
            });
        }
        self.set_mode(Mode::Flung);
    }

    // Swings the character from the pointer like a pendulum that the pointer's motion pushes.
    fn step_held(&mut self, dt: f32) {
        let reach = art::SCRUFF * self.pixel();
        let pull = HANG_GRAVITY * self.scale / reach;
        let Some(grip) = self.grip.as_mut() else {
            self.set_mode(Mode::Flung);
            return;
        };
        if dt > 0. {
            let speed = (grip.pivot.x - grip.last_pivot.x) / dt;
            let accel = ((speed - grip.last_speed) / dt).clamp(-40000., 40000.);
            grip.last_pivot = grip.pivot;
            grip.last_speed = speed;
            grip.swing_speed += (-pull * grip.swing.sin()
                - 5. * grip.swing_speed
                - accel / reach * grip.swing.cos())
                * dt;
            grip.swing = (grip.swing + grip.swing_speed * dt).clamp(-1.5, 1.5);
        }
        self.me.x = grip.pivot.x + reach * grip.swing.sin();
        self.me.y = grip.pivot.y + reach * grip.swing.cos();
        self.me.angle = -grip.swing;
        self.me.vx = 0.;
        self.me.vy = 0.;
    }

    fn dust(&mut self, x: f32, y: f32, count: usize) {
        let scale = self.scale;
        let pixel = self.pixel();
        for i in 0..count {
            let side = if i % 2 == 0 { 1. } else { -1. };
            let vx = side * self.range(15., 45.) * scale;
            let vy = -self.range(8., 30.) * scale;
            let life = self.range(0.3, 0.55);
            self.particles.push(Particle {
                x: x + side * 3. * pixel,
                y: y - pixel * 0.5,
                vx,
                vy,
                age: 0.,
                life,
                kind: Speck::Dust,
            });
        }
    }

    // Leaves a twinkle behind the sleigh.
    fn sparkle(&mut self) {
        let scale = self.scale;
        let pixel = self.pixel();
        let vy = self.range(-12., 12.) * scale;
        let drift = self.range(5., 20.) * scale;
        self.particles.push(Particle {
            x: self.me.x - self.facing * 8. * pixel,
            y: self.me.y + pixel,
            vx: -self.facing * drift,
            vy,
            age: 0.,
            life: 0.7,
            kind: Speck::Sparkle,
        });
    }

    // Drops a present out of the back of the sleigh.
    fn drop_gift(&mut self) {
        let scale = self.scale;
        let pixel = self.pixel();
        self.particles.push(Particle {
            x: self.me.x - self.facing * 2. * pixel,
            y: self.me.y - 8. * pixel,
            vx: self.me.vx * 0.3 - self.facing * 25. * scale,
            vy: -70. * scale,
            age: 0.,
            life: 1.6,
            kind: Speck::Gift,
        });
    }

    fn step_particles(&mut self, dt: f32) {
        let scale = self.scale;
        for particle in &mut self.particles {
            particle.age += dt;
            particle.x += particle.vx * dt;
            particle.y += particle.vy * dt;
            match particle.kind {
                Speck::Dust => particle.vy += 60. * scale * dt,
                Speck::Snore => particle.x += (particle.age * 4.).sin() * 6. * scale * dt,
                Speck::Sparkle => {}
                Speck::Gift => particle.vy += 500. * scale * dt,
            }
        }
        self.particles
            .retain(|particle| particle.age < particle.life);
    }

    // Returns the area that the pointer can pick the character up by.
    fn bounds(&self) -> Bounds<Pixels> {
        let pixel = self.pixel();
        let (sin, cos) = self.me.angle.sin_cos();
        // The middle of the character, seven rows above its feet.
        let (x, y) = (self.me.x + 7. * pixel * sin, self.me.y - 7. * pixel * cos);
        Bounds::centered_at(point(px(x), px(y)), size(px(16. * pixel), px(18. * pixel)))
    }

    // Returns how the character and the board look in this frame.
    fn figure(&self, snap: f32) -> Figure {
        let pixel = self.pixel();
        let ride = self.ride;
        let base = Pen {
            x: self.me.x,
            y: self.me.y,
            facing: self.facing,
            angle: self.me.angle,
            squeeze: 1.,
            pixel,
            snap,
        };
        let mut body = base;
        let mut board = if self.carried {
            base
        } else {
            Pen {
                x: self.board.x,
                y: self.board.y,
                angle: self.board.angle,
                ..base
            }
        };
        let beat = |rate: f32| (self.time * rate) as i64 % 2 == 1;
        let flail = if beat(10.) { Arms::Up } else { Arms::Out };
        // Where a skater balances, Santa holds the reins and a zombie reaches ahead.
        let steady = match ride {
            Ride::Board => Arms::Balance,
            Ride::Sleigh => Arms::Reins,
            Ride::Feet => Arms::Forward,
        };
        let mut pose = Pose {
            dy: 0,
            legs: Legs::Stand,
            arms: Arms::Down,
            eyes: if self.next_blink < 0. {
                Eyes::Closed
            } else {
                Eyes::Open
            },
            head: 0.,
        };
        let mut face = Deck::Up;
        let (mut stars, mut bang, mut sweat) = (false, false, false);
        let cheering = self.cheer > 0.;
        match self.mode {
            Mode::Hidden | Mode::Recover => {}
            Mode::Enter => {
                pose.arms = if ride == Ride::Feet {
                    Arms::Forward
                } else {
                    Arms::Up
                };
                pose.eyes = if self.me.vy > 0. {
                    Eyes::Happy
                } else {
                    Eyes::Wide
                };
            }
            Mode::Idle => pose.dy = i32::from(beat(1.2)),
            Mode::Doze => {
                pose.dy = 2;
                pose.legs = Legs::Sit;
                pose.eyes = Eyes::Closed;
            }
            Mode::Wake => {
                pose.arms = Arms::Up;
                pose.eyes = Eyes::Happy;
                if self.clock < 0.2 {
                    pose.dy = 1;
                    pose.legs = Legs::Crouch;
                }
            }
            Mode::Ride => {
                pose.arms = steady;
                if ride == Ride::Feet && self.me.vx != 0. {
                    // A zombie leans into its shamble.
                    pose.legs = Legs::Walk(beat(5.5));
                    body.angle = self.me.angle + self.facing * 0.1;
                }
                if ride == Ride::Board && self.push >= 0. {
                    pose.legs = Legs::Push((self.push * 3.2) as i64 % 2 == 1);
                }
                if self.manual > 0. {
                    // The nose lifts about the back wheels and the rider leans back.
                    let lift = -0.32 * self.facing;
                    board.angle = self.me.angle + lift;
                    board.y -= 4. * pixel * lift.abs().sin();
                    body.angle = self.me.angle + lift * 0.4;
                    body.y = board.y;
                    pose.arms = Arms::Out;
                }
                if cheering {
                    pose.arms = Arms::Up;
                    pose.eyes = Eyes::Happy;
                }
            }
            Mode::Turn => match ride {
                Ride::Board => {
                    let lift = -0.4 * self.facing * (PI * self.clock / TURN).sin();
                    board.angle = self.me.angle + lift;
                    body.angle = self.me.angle + lift * 0.5;
                    pose.arms = Arms::Out;
                }
                Ride::Sleigh => {
                    // The team swings about, narrowing to its middle and widening the other way.
                    board.squeeze = (PI * self.clock / TURN).cos().abs().max(0.08);
                    pose.arms = Arms::Reins;
                }
                Ride::Feet => pose.arms = Arms::Forward,
            },
            Mode::Crouch => {
                // Santa sits, so the sleigh does not squat.
                if ride != Ride::Sleigh {
                    pose.dy = 1;
                    pose.legs = Legs::Crouch;
                }
                pose.arms = steady;
            }
            Mode::Land => {
                if self.clock < 0.08 && ride != Ride::Sleigh {
                    pose.dy = 1;
                    pose.legs = Legs::Crouch;
                }
                pose.arms = steady;
                if cheering {
                    pose.arms = Arms::Up;
                    pose.eyes = Eyes::Happy;
                }
            }
            Mode::Air => {
                let share = (self.clock / self.air.max(0.1)).clamp(0., 1.);
                // The trick plays between the pop and the catch.
                let trick = ((share - 0.12) / 0.7).clamp(0., 1.);
                pose.arms = steady;
                if ride == Ride::Feet {
                    // A zombie hops with loose legs and lifted arms.
                    pose.legs = Legs::Dangle(false);
                    pose.arms = Arms::Up;
                } else {
                    match self.trick {
                        Trick::Ollie => {
                            if ride == Ride::Board {
                                pose.arms = Arms::Up;
                            }
                            if share < 0.35 {
                                let lift = -0.35 * self.facing * (1. - share / 0.35);
                                board.angle = lift;
                                body.angle = lift * 0.3;
                            }
                        }
                        Trick::Kickflip => {
                            face = Deck::of_flip(trick);
                            board.y += 2. * pixel * (PI * trick).sin();
                        }
                        Trick::Shuvit => {
                            board.squeeze = (PI * trick).cos();
                            board.y += pixel * (PI * trick).sin();
                            pose.arms = Arms::Out;
                        }
                        Trick::Spin => {
                            body.squeeze = (TAU * trick).cos();
                            board.squeeze = body.squeeze;
                            pose.arms = Arms::Up;
                        }
                        Trick::Grab => {
                            if (0.15..0.85).contains(&share) {
                                pose.dy = 1;
                                pose.legs = Legs::Crouch;
                                pose.arms = Arms::Grab;
                                pose.eyes = Eyes::Happy;
                            }
                        }
                        Trick::Soar => {
                            if (0.2..0.8).contains(&share) {
                                pose.arms = Arms::Up;
                                pose.eyes = Eyes::Happy;
                            }
                        }
                        Trick::Gift => {
                            // Santa lets go of the reins to drop the present at the top.
                            if (0.35..0.65).contains(&share) {
                                pose.arms = Arms::Up;
                            }
                        }
                        Trick::Loop => {
                            // The whole team loops about its middle, nose up first, while it is
                            // high enough that the reindeer clear the edge.
                            let turn = ((share - 0.2) / 0.6).clamp(0., 1.);
                            board = board.turned_about(13., 10., -self.facing * TAU * turn);
                            pose.eyes = Eyes::Happy;
                        }
                    }
                }
            }
            Mode::Bail => {
                pose.legs = Legs::Dangle(beat(10.));
                pose.arms = flail;
                pose.eyes = Eyes::Wide;
            }
            Mode::Dizzy => {
                pose.eyes = Eyes::Closed;
                stars = true;
            }
            Mode::Walk => {
                let stride = beat(5.5);
                pose.legs = Legs::Walk(stride);
                pose.arms = if stride { Arms::Balance } else { Arms::Down };
            }
            Mode::Mount => pose.arms = Arms::Up,
            Mode::Panic => {
                pose.eyes = Eyes::Wide;
                pose.legs = Legs::Dangle(beat(9.));
                sweat = true;
                if self.clock < 0.3 {
                    bang = true;
                    pose.arms = Arms::Up;
                } else {
                    pose.arms = flail;
                }
            }
            Mode::Held => {
                pose.eyes = Eyes::Wide;
                pose.legs = Legs::Dangle(beat(6.));
                pose.arms = Arms::Up;
                sweat = true;
            }
            Mode::Flung => {
                pose.eyes = Eyes::Wide;
                pose.legs = Legs::Dangle(beat(9.));
                pose.arms = flail;
                bang = self.clock < 0.35;
            }
            Mode::Dance => {
                let share = (self.clock / self.dance.length()).min(1.);
                pose.eyes = Eyes::Happy;
                match self.dance {
                    Dance::Headspin => {
                        // On its head with the legs apart, turning about itself.
                        body = base.turned_about(6., 7., PI);
                        body.squeeze = (self.time * 16.).cos();
                        pose.legs = Legs::Dangle(true);
                        pose.arms = Arms::Out;
                    }
                    Dance::Windmill => {
                        // Two rolls about its middle on the floor, the legs flying.
                        body = base.turned_about(6., 7., self.facing * TAU * 2. * share);
                        pose.legs = Legs::Dangle(beat(8.));
                        pose.arms = Arms::Out;
                    }
                    Dance::Moonwalk => {
                        pose.legs = Legs::Walk(beat(4.));
                        pose.arms = if beat(2.) { Arms::Balance } else { Arms::Down };
                    }
                    Dance::HeadToss => {
                        let toss = ((share - 0.15) / 0.7).clamp(0., 1.);
                        let flying = toss > 0. && toss < 1.;
                        pose.head = 40. * toss * (1. - toss);
                        pose.arms = if flying { Arms::Up } else { Arms::Forward };
                        if flying {
                            pose.eyes = Eyes::Wide;
                        }
                    }
                }
            }
        }
        // A ghost bobs while it hovers.
        if art::art(self.avatar).floats && pose.dy == 0 && (self.time * 2.5).sin() > 0. {
            pose.dy = -1;
        }
        // Santa sits in the sleigh and moves with it.
        if ride == Ride::Sleigh && self.carried {
            body = board;
        }
        // The reindeer gallop while the sleigh moves on the ground and tuck their legs in the air.
        let (speed, flying) = if self.carried {
            (
                self.me.vx,
                matches!(
                    self.mode,
                    Mode::Enter | Mode::Air | Mode::Panic | Mode::Held | Mode::Flung
                ),
            )
        } else {
            (
                self.board.vx,
                self.board.vy != 0. || matches!(self.mode, Mode::Panic | Mode::Held | Mode::Flung),
            )
        };
        let team = if flying {
            Team::Fly
        } else if speed.abs() > 4. * self.scale {
            Team::Gallop(beat(8.))
        } else {
            Team::Stand
        };
        Figure {
            body,
            board,
            pose,
            face,
            team,
            stars,
            bang,
            sweat,
        }
    }
}

// Keeps a thrown thing inside the window's sides and below its top, bouncing it off them.
fn bounce(body: &mut Body, view: (f32, f32), margin: f32) {
    if body.x < margin {
        body.x = margin;
        body.vx = body.vx.abs() * 0.55;
        body.spin = -body.spin * 0.7;
    } else if body.x > view.0 - margin {
        body.x = view.0 - margin;
        body.vx = -body.vx.abs() * 0.55;
        body.spin = -body.spin * 0.7;
    }
    if body.y < margin * 2. && body.vy < 0. {
        body.y = margin * 2.;
        body.vy = body.vy.abs() * 0.45;
    }
}

// Returns an angle between -π and π.
fn wrap(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

/// How the character and the board look in one frame.
struct Figure {
    body: Pen,
    board: Pen,
    pose: Pose,
    face: Deck,
    team: Team,
    /// Stars circle the head.
    stars: bool,
    /// A `!` shows above the head.
    bang: bool,
    /// A drop of sweat flies off the head.
    sweat: bool,
}

/// What the layer paints in one frame.
struct Frame {
    cadence: Cadence,
    /// The character to paint, or nothing while it is off the screen.
    figure: Option<Figure>,
    particles: Vec<Particle>,
    avatar: SkaterAvatar,
    colors: Palette,
    time: f32,
    hitbox: Option<Hitbox>,
    held: bool,
}

/// Paints the skater above the workspace and lets the pointer pick it up. Add one to the window
/// root, above the workspace. Keep it outside the workspace view, so that a frame of the
/// character does not render the workspace again.
pub(super) struct SkaterLayer {
    sim: Rc<RefCell<Sim>>,
    /// A timer for the next slow frame is running.
    waiting: Rc<Cell<bool>>,
    /// The next smooth frame waits for a display refresh.
    pacing: Rc<Cell<bool>>,
}

impl SkaterLayer {
    /// Creates the layer with the character off the screen.
    pub(super) fn new(_: &mut Context<Self>) -> Self {
        Self {
            sim: Rc::new(RefCell::new(Sim::new())),
            waiting: Rc::new(Cell::new(false)),
            pacing: Rc::new(Cell::new(false)),
        }
    }
}

// Notifies the layer `view` on the first display refresh at least `SMOOTH_FRAME` after `since`,
// so the motion keeps to whole refreshes without drawing a frame on each one. Refreshes come at
// the display's rate in an unfocused window too, so the motion stays as smooth there.
fn pace(view: EntityId, pacing: Rc<Cell<bool>>, since: Instant, window: &Window) {
    window.on_next_refresh(move |window, cx| {
        if since.elapsed() < SMOOTH_FRAME {
            pace(view, pacing, since, window);
            return;
        }
        pacing.set(false);
        cx.notify(view);
    });
}

impl Render for SkaterLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let sim = self.sim.clone();
        let input = self.sim.clone();
        let waiting = self.waiting.clone();
        let pacing = self.pacing.clone();
        canvas(
            move |_, window, cx| {
                let scene = shared(cx);
                let (cue, ground) = {
                    let scene = scene.borrow();
                    (scene.cue.clone(), scene.ground())
                };
                let mut sim = sim.borrow_mut();
                let cadence = sim.advance(
                    Instant::now(),
                    cue.as_ref(),
                    ground,
                    window.viewport_size(),
                    cx.reduce_motion(),
                );
                if cadence == Cadence::Rest {
                    return None;
                }
                let shown = sim.mode != Mode::Hidden;
                let hitbox = (shown && cadence != Cadence::Still)
                    .then(|| window.insert_hitbox(sim.bounds(), HitboxBehavior::BlockMouse));
                Some(Frame {
                    cadence,
                    figure: shown.then(|| sim.figure(window.scale_factor())),
                    particles: sim.particles.clone(),
                    avatar: sim.avatar,
                    colors: Palette::new(sim.tone),
                    time: sim.time,
                    hitbox,
                    held: sim.mode == Mode::Held,
                })
            },
            move |_, frame, window, cx| {
                let Some(frame) = frame else {
                    return;
                };
                let viewport = Bounds::new(Point::default(), window.viewport_size());
                window.paint_layer(viewport, |window| paint_frame(&frame, window));
                let view = window.current_view();
                if let Some(hitbox) = &frame.hitbox {
                    window.set_cursor_style(
                        if frame.held {
                            CursorStyle::ClosedHand
                        } else {
                            CursorStyle::OpenHand
                        },
                        hitbox,
                    );
                    let sim = input.clone();
                    let hitbox = hitbox.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble
                            && event.button == MouseButton::Left
                            && hitbox.is_hovered(window)
                        {
                            sim.borrow_mut().grab(event.position, Instant::now());
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify(view);
                        }
                    });
                }
                if frame.held {
                    window.set_window_cursor_style(CursorStyle::ClosedHand);
                    let sim = input.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        let mut sim = sim.borrow_mut();
                        // A button released outside the window ends the hold too.
                        if event.pressed_button == Some(MouseButton::Left) {
                            sim.drag(event.position, Instant::now());
                        } else {
                            sim.release(Instant::now());
                        }
                        cx.notify(view);
                    });
                    let sim = input.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                            sim.borrow_mut().release(Instant::now());
                            cx.stop_propagation();
                            cx.notify(view);
                        }
                    });
                }
                match frame.cadence {
                    Cadence::Smooth => {
                        if !pacing.replace(true) {
                            pace(view, pacing.clone(), Instant::now(), window);
                        }
                    }
                    Cadence::Slow => {
                        if !waiting.replace(true) {
                            let waiting = waiting.clone();
                            cx.spawn(async move |cx| {
                                cx.background_executor()
                                    .timer(Duration::from_secs_f32(1. / IDLE_RATE))
                                    .await;
                                cx.update(|cx| {
                                    waiting.set(false);
                                    cx.notify(view);
                                });
                            })
                            .detach();
                        }
                    }
                    Cadence::Rest | Cadence::Still => {}
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

// Paints the ride, the character, what floats around its head and the specks.
fn paint_frame(frame: &Frame, window: &mut Window) {
    let colors = &frame.colors;
    if let Some(figure) = &frame.figure {
        let pixel = figure.body.pixel;
        let art = art::art(frame.avatar);
        match art.ride {
            Ride::Board => {
                art::paint_board(&figure.board, figure.face, colors, window);
                art::paint_character(&figure.body, &art, figure.pose, colors, window);
            }
            // Santa sits behind the sleigh's front wall.
            Ride::Sleigh => {
                art::paint_character(&figure.body, &art, figure.pose, colors, window);
                art::paint_sleigh(&figure.board, figure.team, colors, window);
            }
            Ride::Feet => art::paint_character(&figure.body, &art, figure.pose, colors, window),
        }
        let (head_x, head_y) = figure.body.point(6., 3.);
        if figure.stars {
            for i in 0..3 {
                let turn = frame.time * 5. + i as f32 * TAU / 3.;
                art::paint_star(
                    head_x + turn.cos() * 5. * pixel,
                    head_y - 5. * pixel + turn.sin() * 1.5 * pixel,
                    pixel,
                    1.,
                    colors,
                    window,
                );
            }
        }
        if figure.bang {
            let (x, y) = figure.body.point(8., 0.);
            art::paint_bang(x, y - pixel, pixel, colors, window);
        }
        if figure.sweat {
            let (x, y) = figure.body.point(12.5, 1.5 + (frame.time * 6.).fract());
            for row in [0., 1.] {
                window.paint_quad(fill(
                    Bounds::new(
                        point(px(x - pixel / 2.), px(y + row * pixel)),
                        size(px(pixel), px(pixel)),
                    ),
                    colors.sweat,
                ));
            }
        }
    }
    for particle in &frame.particles {
        let fade = 1. - particle.age / particle.life;
        let pixel = PIXEL * crate::theme::ui_scale();
        match particle.kind {
            Speck::Dust => {
                let dot = pixel * 0.75;
                window.paint_quad(fill(
                    Bounds::new(
                        point(px(particle.x - dot / 2.), px(particle.y - dot / 2.)),
                        size(px(dot), px(dot)),
                    ),
                    colors.dust.opacity(fade * 0.8),
                ));
            }
            Speck::Snore => art::paint_snore(
                particle.x,
                particle.y,
                pixel,
                (fade * 2.).min(1.),
                colors,
                window,
            ),
            Speck::Sparkle => {
                art::paint_star(particle.x, particle.y, pixel * 0.6, fade, colors, window)
            }
            Speck::Gift => art::paint_gift(
                particle.x,
                particle.y,
                pixel,
                (fade * 3.).min(1.),
                colors,
                window,
            ),
        }
    }
}
