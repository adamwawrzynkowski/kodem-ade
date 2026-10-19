//! The skater's pixel art: a sprite for each avatar, the shared legs, arms and board, and the pen
//! that paints them mirrored, rotated and squeezed.
//!
//! The art lives on one grid of cells that faces right. Columns run from 0 to 11 with the middle
//! at 6. Rows 0 to 11 hold an avatar's sprite, rows 12 and 13 its legs, and rows 13 to 15 the
//! board, whose deck top is row 14: the line that the feet stand on.
//!
//! Santa rides a sleigh instead, which spans columns -3 to 16 with a pair of reindeer ahead of it
//! up to column 29, and the zombie has nothing under its feet.
use super::*;

/// Column of the middle of the grid.
const MIDDLE: f32 = 6.;
/// Row of the line that the feet stand on, which is also the deck's top.
pub(super) const FEET: f32 = 14.;
/// Rows from the feet line to the bottom of the wheels.
pub(super) const DECK_DEPTH: f32 = 2.;
/// Rows from the feet line to the eyes, where a held character is picked up.
pub(super) const SCRUFF: f32 = 11.;

/// What carries an avatar along the edge.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Ride {
    /// A skateboard under the feet.
    Board,
    /// A sleigh that a pair of reindeer pull. It flies over tabs and does stunts in the air.
    Sleigh,
    /// Nothing: the character walks and dances on the edge.
    Feet,
}

/// How an avatar is drawn on the grid.
pub(super) struct Art {
    rows: [&'static str; 12],
    /// The rows above this one make up the head, which can leave the body.
    neck: usize,
    /// Rows that the sprite sits lower, for a character without legs that rests on the deck.
    drop: i32,
    legs: bool,
    arms: bool,
    /// The back shoulder's column, the front shoulder's column and the row of both.
    shoulders: (i32, i32, i32),
    /// The cells of the legs, the sleeves and the hands.
    leg: u8,
    sleeve: u8,
    hand: u8,
    /// The cell that a closed eye shows.
    lid: u8,
    /// The eyes glow instead of being dark.
    glow: bool,
    /// The character hovers over the deck and bobs there.
    pub floats: bool,
    pub ride: Ride,
}

/// Returns the art of an avatar.
pub(super) fn art(avatar: SkaterAvatar) -> Art {
    let standard = Art {
        rows: CAT,
        neck: 9,
        drop: 0,
        legs: true,
        arms: true,
        shoulders: (2, 9, 9),
        leg: b'd',
        sleeve: b'd',
        hand: b'b',
        lid: b'b',
        glow: false,
        floats: false,
        ride: Ride::Board,
    };
    match avatar {
        SkaterAvatar::Cat => standard,
        SkaterAvatar::Bunny => Art {
            rows: BUNNY,
            ..standard
        },
        SkaterAvatar::Bear => Art {
            rows: BEAR,
            ..standard
        },
        SkaterAvatar::Fox => Art {
            rows: FOX,
            ..standard
        },
        SkaterAvatar::Frog => Art {
            rows: FROG,
            ..standard
        },
        SkaterAvatar::Duck => Art {
            rows: DUCK,
            ..standard
        },
        SkaterAvatar::Penguin => Art {
            rows: PENGUIN,
            shoulders: (1, 10, 8),
            lid: b'c',
            ..standard
        },
        SkaterAvatar::Dino => Art {
            rows: DINO,
            ..standard
        },
        SkaterAvatar::Robot => Art {
            rows: ROBOT,
            lid: b'k',
            glow: true,
            ..standard
        },
        SkaterAvatar::Ghost => Art {
            rows: GHOST,
            legs: false,
            shoulders: (0, 11, 6),
            lid: b'l',
            floats: true,
            ..standard
        },
        SkaterAvatar::Slime => Art {
            rows: SLIME,
            drop: 2,
            legs: false,
            arms: false,
            ..standard
        },
        SkaterAvatar::Alien => Art {
            rows: ALIEN,
            ..standard
        },
        SkaterAvatar::Santa => Art {
            rows: SANTA,
            leg: b'r',
            sleeve: b'r',
            hand: b'w',
            lid: b'c',
            ride: Ride::Sleigh,
            ..standard
        },
        SkaterAvatar::Zombie => Art {
            rows: ZOMBIE,
            hand: b'g',
            lid: b'g',
            ride: Ride::Feet,
            ..standard
        },
    }
}

// Cells: `o` outline, `b` body, `l` light, `d` shade, `c` cream, `w` white, `p` blush, `n` mouth,
// `y` yellow, `r` red, `k` screen, `g` and `h` rotten green and its shade, `u` fur brown, and `e`
// over `f` for the top and bottom of each eye.
const CAT: [&str; 12] = [
    ".o........o.",
    ".oo......oo.",
    ".opoooooopo.",
    "obbbbbbbbbbo",
    "obbbbbbbbbbo",
    "obbbbebbebbo",
    "obpbbfbbfpbo",
    ".obbbbbnbbo.",
    "..oooooooo..",
    ".d.obbbbo...",
    "d..obccbo...",
    ".ddobbbbo...",
];
const BUNNY: [&str; 12] = [
    "..obo..obo..",
    "..opo..opo..",
    ".oopoooopoo.",
    "obbbbbbbbbbo",
    "obbbbbbbbbbo",
    "obbbbebbebbo",
    "obpbbfbbfpbo",
    ".obbbbbpbbo.",
    "..oooooooo..",
    "...obbbbo...",
    "..wobccbo...",
    "...obbbbo...",
];
const BEAR: [&str; 12] = [
    "............",
    ".oo......oo.",
    "obdoooooodbo",
    "obbbbbbbbbbo",
    "obbbbbbbbbbo",
    "obbbbebbebbo",
    "obpbbfbbfpbo",
    ".obbbbcncbo.",
    "..oooooooo..",
    "...obbbbo...",
    "...obccbo...",
    "...obbbbo...",
];
const FOX: [&str; 12] = [
    ".o........o.",
    ".oo......oo.",
    ".odoooooodo.",
    "obbbbbbbbbbo",
    "obbbbbbbbbbo",
    "obbbbebbebbo",
    "occbbfbbfcco",
    ".occcccncco.",
    "..oooooooo..",
    "dd.obbbbo...",
    "cddobccbo...",
    ".ddobbbbo...",
];
const FROG: [&str; 12] = [
    "............",
    ".ooo....ooo.",
    "owewoooowewo",
    "owfwbbbbwfwo",
    "obbbbbbbbbbo",
    "obpbbbbbbpbo",
    "obbnbbbbnbbo",
    ".obbnnnnbbo.",
    "..oooooooo..",
    "...obbbbo...",
    "...obccbo...",
    "...obbbbo...",
];
const DUCK: [&str; 12] = [
    ".....o......",
    "...oooooo...",
    "..obbbbbbo..",
    ".obbbbbbbbo.",
    ".obbbbbebbo.",
    ".obpbbbfbyyy",
    ".obbbbbbbyy.",
    "..obbbbbbo..",
    "...oooooo...",
    "...obbbbo...",
    "...obccbo...",
    "...obbbbo...",
];
const PENGUIN: [&str; 12] = [
    "....oooo....",
    "..oobbbboo..",
    ".obbbbbbbbo.",
    ".obccccccbo.",
    ".obceccecbo.",
    ".obcfccfcbo.",
    ".obpcyycpbo.",
    ".obbccccbbo.",
    "..obccccbo..",
    "..obccccbo..",
    "..obccccbo..",
    "...oooooo...",
];
const DINO: [&str; 12] = [
    "...ooooo....",
    "..obbbbbo...",
    ".obbbbbbooo.",
    "dobbbbbebbbo",
    ".obbbbbfbbbo",
    "dobbbbpbbbbo",
    ".obbbbbbwnwo",
    "..obbbbboooo",
    "...obbbbo...",
    "...obccbo...",
    ".ddobccbo...",
    "dd.obbbbo...",
];
const ROBOT: [&str; 12] = [
    ".....r......",
    ".....o......",
    ".oooooooooo.",
    "obbbbbbbbbbo",
    "obkkkkkkkkbo",
    "obkkkekkekbo",
    "obkkkfkkfkbo",
    "obbbbbbbbbbo",
    ".oooooooooo.",
    "...obbbbo...",
    "...obrlbo...",
    "...obbbbo...",
];
const GHOST: [&str; 12] = [
    "....oooo....",
    "..oolllloo..",
    ".ollllllllo.",
    ".oleelleelo.",
    ".olffllfflo.",
    "olpllnnllplo",
    "ollllllllllo",
    "ollllllllllo",
    "ollllllllllo",
    "ollllllllllo",
    "obllbllbllbo",
    "oo.oo..oo.oo",
];
const SLIME: [&str; 12] = [
    "............",
    "............",
    "............",
    "....oooo....",
    "..oobbbboo..",
    ".obwlbbbbbo.",
    ".oblbbbbbbo.",
    "obbbbebbebbo",
    "obbbbfbbfbbo",
    "obpbbnnbbpbo",
    "obbbbbbbbbbo",
    ".oooooooooo.",
];
const ALIEN: [&str; 12] = [
    ".l........l.",
    "..o......o..",
    "...oooooo...",
    "..obbbbbbo..",
    ".obbbbbbbbo.",
    ".obeebbeebo.",
    ".obffbbffbo.",
    "..obbnbbbo..",
    "...oooooo...",
    "....obbo....",
    "...obbbbo...",
    "...obbbbo...",
];
const SANTA: [&str; 12] = [
    "....oooo....",
    ".woorrrroo..",
    "wworrrrrrro.",
    "owwwwwwwwwwo",
    "occcccccccco",
    "occcceccecco",
    "ocpccfccfpco",
    "owwwwwwwwwwo",
    ".owwwwwnwwo.",
    "..owwwwwwo..",
    "..orrrrrro..",
    "..ooooyooo..",
];
const ZOMBIE: [&str; 12] = [
    "..o.o..o.o..",
    ".oooooooooo.",
    "ohgggggggggo",
    "ohgggggogggo",
    "ohgggggggggo",
    "oggggeggeggo",
    "ohgggfggfggo",
    ".ogggnwnwno.",
    "..oooooooo..",
    "...obbdbo...",
    "...obgbbo...",
    "...obbbdo...",
];

/// Column of the near reindeer's first cell. The far one runs three columns behind and a row
/// higher.
const REINDEER_COL: i32 = 17;
/// The sleigh from column -3 and row 6. Santa sits in it behind its front wall.
const SLEIGH: [&str; 10] = [
    ".o..................",
    "obo.................",
    "obo...............oo",
    "obo..............obo",
    "oyyyyyyyyyyyyyyyyyo.",
    "obbbbbbbbbbbbbbbbbo.",
    "obbbbyybbbbbbyybbo..",
    ".obbbbbbbbbbbbbbo...",
    "...o...o....o...o..o",
    "oyyyyyyyyyyyyyyyyyo.",
];
/// A reindeer from row 5, without its legs, with a red nose.
const REINDEER: [&str; 8] = [
    "........c.c..",
    "........ccc..",
    "........ouuo.",
    "........uueuu",
    "........ouuur",
    "couuuuuuuuuo.",
    "ouuuuuuuuuo..",
    ".uucccccuu...",
];
/// A reindeer's legs from row 13 for each stride.
const HOOVES_STAND: [&str; 3] = ["..o.o...o.o..", "..o.o...o.o..", "..o.o...o.o.."];
const HOOVES_REACH: [&str; 3] = [".oo......oo..", "o..........o.", "o...........o"];
const HOOVES_GATHER: [&str; 3] = ["...oo...oo...", "....o..o.....", "....o..o....."];
const HOOVES_FLY: [&str; 3] = ["oo.......ooo.", ".............", "............."];
/// The reins from Santa's front hand to the near reindeer's head, and the traces from the sleigh
/// to it.
const REINS: [(i32, i32); 16] = [
    (12, 9),
    (13, 9),
    (14, 9),
    (15, 9),
    (16, 9),
    (17, 9),
    (18, 9),
    (19, 9),
    (20, 9),
    (21, 9),
    (22, 9),
    (23, 9),
    (24, 9),
    (15, 12),
    (16, 12),
    (17, 12),
];

/// How the legs stand. The flag picks one of two frames of a stepping pose.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Legs {
    Stand,
    /// Bent: only the feet show under a sprite that sits a row lower.
    Crouch,
    /// One foot pushes on the ground behind the board.
    Push(bool),
    Walk(bool),
    /// Hanging free, as in the air or in a hand.
    Dangle(bool),
    /// Stretched forward over the deck under a sprite that sits two rows lower.
    Sit,
}

/// Where the arms reach.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Arms {
    Down,
    Balance,
    Up,
    Out,
    /// The front hand holds the board.
    Grab,
    /// The front hand holds the reins ahead.
    Reins,
    /// Both arms reach ahead, as a zombie's do.
    Forward,
}

/// What the eyes show.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Eyes {
    Open,
    Closed,
    Wide,
    Happy,
}

/// One frame of the character.
#[derive(Clone, Copy)]
pub(super) struct Pose {
    /// Rows that the sprite sits lower than standing.
    pub dy: i32,
    pub legs: Legs,
    pub arms: Arms,
    pub eyes: Eyes,
    /// Rows that the head rises off the body.
    pub head: f32,
}

/// How the reindeer move their legs.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Team {
    Stand,
    /// The flag picks the reaching or the gathered frame of a gallop.
    Gallop(bool),
    /// Legs tucked, in the air.
    Fly,
}

/// The face of the board that the viewer sees while it flips about its length.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Deck {
    /// Wheels down.
    Up,
    /// The underside with its trucks.
    Bottom,
    /// Wheels up.
    Down,
    /// The grip on the top face.
    Top,
}

impl Deck {
    /// Returns the face shown at a share of one flip, from 0 to 1.
    pub(super) fn of_flip(share: f32) -> Self {
        match ((share.rem_euclid(1.) * 4.) as u8).min(3) {
            0 => Self::Up,
            1 => Self::Bottom,
            2 => Self::Down,
            _ => Self::Top,
        }
    }
}

/// The colors of a character in a project's tone.
#[derive(Clone, Copy)]
pub(super) struct Palette {
    outline: Hsla,
    body: Hsla,
    light: Hsla,
    shade: Hsla,
    cream: Hsla,
    white: Hsla,
    blush: Hsla,
    yellow: Hsla,
    red: Hsla,
    green: Hsla,
    moss: Hsla,
    brown: Hsla,
    screen: Hsla,
    eye: Hsla,
    glow: Hsla,
    deck: Hsla,
    grip: Hsla,
    wheel: Hsla,
    pub dust: Hsla,
    pub sweat: Hsla,
}

impl Palette {
    /// Derives the character's colors from a project's tone.
    pub(super) fn new(tone: Hsla) -> Self {
        // Each color keeps the tone's hue and sets its own saturation and lightness.
        let of = |saturation: f32, lightness: f32| {
            let mut color = tone;
            color.saturation = saturation;
            color.lightness = lightness.clamp(0.06, 0.94);
            color.alpha = 1.;
            color
        };
        let saturation = tone.saturation;
        let lightness = tone.lightness.clamp(0.4, 0.72);
        let shade_of = |lightness: f32| of(saturation, lightness);
        Self {
            outline: of((saturation * 0.6).min(0.5), 0.13),
            body: shade_of(lightness),
            light: shade_of(lightness + 0.15),
            shade: shade_of(lightness - 0.17),
            cream: hsla(0.11, 0.5, 0.9, 1.),
            white: hsla(0., 0., 0.98, 1.),
            blush: hsla(0.97, 0.8, 0.74, 1.),
            yellow: hsla(0.12, 0.9, 0.6, 1.),
            red: hsla(0.0, 0.8, 0.6, 1.),
            green: hsla(0.27, 0.35, 0.56, 1.),
            moss: hsla(0.27, 0.3, 0.42, 1.),
            brown: hsla(0.07, 0.42, 0.4, 1.),
            screen: of(0.35, 0.14),
            eye: of(0.3, 0.09),
            glow: hsla(0.5, 0.95, 0.72, 1.),
            deck: shade_of(lightness - 0.3),
            grip: hsla(0., 0., 0.16, 1.),
            wheel: hsla(0.12, 0.25, 0.86, 1.),
            dust: hsla(0., 0., 0.72, 1.),
            sweat: hsla(0.56, 0.8, 0.78, 1.),
        }
    }

    /// Returns the color of a sprite cell with the eyes in `eyes`, or nothing for an empty cell.
    fn cell(&self, cell: u8, eyes: Eyes, art: &Art) -> Option<Hsla> {
        let eye = if art.glow { self.glow } else { self.eye };
        let lid = self.fill(art.lid).unwrap_or(self.body);
        Some(match cell {
            b'e' => match eyes {
                Eyes::Open | Eyes::Happy => eye,
                Eyes::Closed => lid,
                Eyes::Wide => self.white,
            },
            b'f' => match eyes {
                Eyes::Happy => lid,
                _ => eye,
            },
            _ => return self.fill(cell),
        })
    }

    // Returns the color of a sprite cell other than an eye, or nothing for an empty cell.
    fn fill(&self, cell: u8) -> Option<Hsla> {
        Some(match cell {
            b'o' | b'n' => self.outline,
            b'b' => self.body,
            b'l' => self.light,
            b'd' => self.shade,
            b'c' => self.cream,
            b'w' => self.white,
            b'p' => self.blush,
            b'y' => self.yellow,
            b'r' => self.red,
            b'g' => self.green,
            b'h' => self.moss,
            b'u' => self.brown,
            b'k' => self.screen,
            _ => return None,
        })
    }

    // Returns the color of a cell of the sleigh or a reindeer, whose eyes stay open.
    fn ride_cell(&self, cell: u8) -> Option<Hsla> {
        match cell {
            b'e' => Some(self.eye),
            _ => self.fill(cell),
        }
    }
}

/// Paints grid cells at a place on the window, mirrored to the facing, rotated about the feet
/// and squeezed across, as for a spin.
#[derive(Clone, Copy)]
pub(super) struct Pen {
    /// The middle of the feet line, in window pixels.
    pub x: f32,
    pub y: f32,
    /// 1 faces right and -1 left.
    pub facing: f32,
    /// Clockwise turn in radians.
    pub angle: f32,
    /// Width share from -1 to 1. A negative share mirrors the art.
    pub squeeze: f32,
    /// Size of one cell in window pixels.
    pub pixel: f32,
    /// Window pixels per device pixel, for cells that line up with the screen.
    pub snap: f32,
}

impl Pen {
    /// Returns where a point of the grid lands on the window.
    pub(super) fn point(&self, col: f32, row: f32) -> (f32, f32) {
        let local_x = (col - MIDDLE) * self.pixel * self.facing * self.squeeze;
        let local_y = (row - FEET) * self.pixel;
        let (sin, cos) = self.angle.sin_cos();
        (
            self.x + local_x * cos - local_y * sin,
            self.y + local_x * sin + local_y * cos,
        )
    }

    /// Returns the pen turned clockwise by `angle` about the point `col`, `row` of the grid,
    /// which stays where it is.
    pub(super) fn turned_about(&self, col: f32, row: f32, angle: f32) -> Self {
        let (x, y) = self.point(col, row);
        let mut pen = Self {
            angle: self.angle + angle,
            ..*self
        };
        let (moved_x, moved_y) = pen.point(col, row);
        pen.x += x - moved_x;
        pen.y += y - moved_y;
        pen
    }

    /// Paints the cell at `col`, `row` of the grid.
    pub(super) fn cell(&self, col: f32, row: f32, color: Hsla, window: &mut Window) {
        let (x, y) = self.point(col + 0.5, row + 0.5);
        let rotated = self.angle.abs() > 0.01;
        // A turned cell grows a little, so that its neighbors leave no gaps between them.
        let grow = if rotated { self.pixel * 0.35 } else { 0. };
        let width = self.pixel * self.squeeze.abs().max(0.15) + grow;
        let height = self.pixel + grow;
        let (mut left, mut top) = (x - width / 2., y - height / 2.);
        if !rotated {
            left = (left * self.snap).round() / self.snap;
            top = (top * self.snap).round() / self.snap;
        }
        window.paint_quad(fill(
            Bounds::new(point(px(left), px(top)), size(px(width), px(height))),
            color,
        ));
    }
}

/// Paints the character in `pose`.
pub(super) fn paint_character(
    pen: &Pen,
    art: &Art,
    pose: Pose,
    colors: &Palette,
    window: &mut Window,
) {
    if art.legs {
        let (leg, outline) = (colors.fill(art.leg).unwrap_or(colors.shade), colors.outline);
        for &(col, row, foot) in legs(pose.legs) {
            pen.cell(
                f32::from(col),
                f32::from(row),
                if foot { outline } else { leg },
                window,
            );
        }
    }
    let dy = art.drop + pose.dy;
    for (row, line) in art.rows.iter().enumerate() {
        let lift = if row < art.neck { pose.head } else { 0. };
        for (col, cell) in line.bytes().enumerate() {
            if let Some(color) = colors.cell(cell, pose.eyes, art) {
                pen.cell(col as f32, row as f32 + dy as f32 - lift, color, window);
            }
        }
    }
    if art.arms {
        let (back, front, shoulder) = art.shoulders;
        let shoulder = shoulder + dy;
        let sleeve = colors.fill(art.sleeve).unwrap_or(colors.shade);
        let hand_color = colors.fill(art.hand).unwrap_or(colors.body);
        let (back_arm, front_arm) = arms(pose.arms);
        // Reaching ahead, the back arm shows from the front shoulder a row lower, as the body
        // hides its root.
        let (back, back_side, back_drop) = if pose.arms == Arms::Forward {
            (front, 1, 1)
        } else {
            (back, -1, 0)
        };
        for (base, reach, side, drop) in [
            (back, back_arm, back_side, back_drop),
            (front, front_arm, 1, 0),
        ] {
            for (i, &(out, down)) in reach.iter().enumerate() {
                let hand = i + 1 == reach.len();
                pen.cell(
                    (base + out * side) as f32,
                    (shoulder + down + drop) as f32,
                    if hand { hand_color } else { sleeve },
                    window,
                );
            }
        }
    }
}

// Returns the leg cells of a pose: column, row, and whether the cell is a foot.
fn legs(legs: Legs) -> &'static [(u8, u8, bool)] {
    match legs {
        Legs::Stand => &[
            (4, 12, false),
            (7, 12, false),
            (4, 13, true),
            (5, 13, true),
            (7, 13, true),
            (8, 13, true),
        ],
        Legs::Crouch => &[(4, 13, true), (5, 13, true), (7, 13, true), (8, 13, true)],
        Legs::Push(false) => &[
            (7, 12, false),
            (7, 13, true),
            (8, 13, true),
            (4, 12, false),
            (3, 13, false),
            (3, 14, false),
            (2, 15, true),
            (3, 15, true),
        ],
        Legs::Push(true) => &[
            (7, 12, false),
            (7, 13, true),
            (8, 13, true),
            (4, 12, false),
            (3, 13, false),
            (2, 14, false),
            (0, 15, true),
            (1, 15, true),
        ],
        Legs::Walk(false) => &[
            (3, 12, false),
            (8, 12, false),
            (2, 13, true),
            (3, 13, true),
            (8, 13, true),
            (9, 13, true),
        ],
        Legs::Walk(true) => &[
            (5, 12, false),
            (6, 12, false),
            (5, 13, true),
            (6, 13, true),
            (7, 13, true),
        ],
        Legs::Dangle(false) => &[
            (4, 12, false),
            (7, 12, false),
            (4, 13, false),
            (7, 13, false),
            (4, 14, true),
            (7, 14, true),
        ],
        Legs::Dangle(true) => &[
            (4, 12, false),
            (7, 12, false),
            (3, 13, false),
            (8, 13, false),
            (3, 14, true),
            (8, 14, true),
        ],
        Legs::Sit => &[(8, 13, false), (9, 13, false), (10, 13, true)],
    }
}

// The cells of one arm, from the shoulder out, as steps away from the body and down from the
// shoulder.
type Reach = &'static [(i32, i32)];

// Returns the cells of the back and the front arm.
fn arms(arms: Arms) -> (Reach, Reach) {
    match arms {
        Arms::Down => (&[(0, 0), (0, 1)], &[(0, 0), (0, 1)]),
        Arms::Balance => (&[(0, 0), (1, 1)], &[(0, 0), (1, 1)]),
        Arms::Up => (&[(0, -1), (1, -2)], &[(0, -1), (1, -2)]),
        Arms::Out => (&[(0, 0), (1, 0), (2, -1)], &[(0, 0), (1, 0), (2, -1)]),
        Arms::Grab => (&[(0, -1), (1, -2)], &[(0, 0), (0, 1), (0, 2), (0, 3)]),
        Arms::Reins => (&[(0, 0), (0, 1)], &[(0, 0), (1, 0), (2, 0)]),
        Arms::Forward => (&[(1, 0), (2, 0), (3, 0)], &[(0, 0), (1, 0), (2, 0), (3, 0)]),
    }
}

/// Paints the board with `face` toward the viewer.
pub(super) fn paint_board(pen: &Pen, face: Deck, colors: &Palette, window: &mut Window) {
    let deck = colors.deck;
    let wheel = colors.wheel;
    let mut cells: Vec<(i32, i32, Hsla)> = Vec::with_capacity(48);
    match face {
        Deck::Up | Deck::Down => {
            // Upside down, the rows swap about the deck.
            let (tails, wheels) = if face == Deck::Up { (13, 15) } else { (15, 13) };
            cells.extend([(-1, tails, deck), (12, tails, deck)]);
            cells.extend((0..12).map(|col| (col, 14, deck)));
            cells.extend([1, 2, 9, 10].map(|col| (col, wheels, wheel)));
        }
        Deck::Bottom => {
            for row in 13..=15 {
                let tone = if row == 14 { colors.body } else { deck };
                cells.extend((0..12).map(|col| (col, row, tone)));
            }
            cells.extend([2, 9].map(|col| (col, 14, wheel)));
        }
        Deck::Top => {
            for row in 13..=15 {
                cells.extend((0..12).map(|col| (col, row, colors.grip)));
            }
            cells.extend([(0, 14, deck), (11, 14, deck)]);
        }
    }
    for (col, row, color) in cells {
        pen.cell(col as f32, row as f32, color, window);
    }
}

/// Paints the sleigh with its reindeer, whose legs move as `team` says.
pub(super) fn paint_sleigh(pen: &Pen, team: Team, colors: &Palette, window: &mut Window) {
    let hooves = match team {
        Team::Stand => &HOOVES_STAND,
        Team::Gallop(false) => &HOOVES_REACH,
        Team::Gallop(true) => &HOOVES_GATHER,
        Team::Fly => &HOOVES_FLY,
    };
    // The far reindeer runs beside the near one, in shadow.
    for (col, row, far) in [(REINDEER_COL - 3, 4, true), (REINDEER_COL, 5, false)] {
        let color = |cell: u8| {
            colors.ride_cell(cell).map(|mut color| {
                if far {
                    color.lightness *= 0.72;
                }
                color
            })
        };
        paint_rows(pen, &REINDEER, col, row, color, window);
        paint_rows(pen, hooves, col, row + 8, color, window);
    }
    paint_rows(pen, &SLEIGH, -3, 6, |cell| colors.ride_cell(cell), window);
    for (col, row) in REINS {
        pen.cell(col as f32, row as f32, colors.outline, window);
    }
    // A golden collar holds the near reindeer's reins.
    for row in [10., 11.] {
        pen.cell((REINDEER_COL + 8) as f32, row, colors.yellow, window);
    }
}

// Paints rows of cells whose first cell sits at `col`, `row` of the grid.
fn paint_rows(
    pen: &Pen,
    rows: &[&str],
    col: i32,
    row: i32,
    color: impl Fn(u8) -> Option<Hsla>,
    window: &mut Window,
) {
    for (dy, line) in rows.iter().enumerate() {
        for (dx, cell) in line.bytes().enumerate() {
            if let Some(color) = color(cell) {
                pen.cell(
                    (col + dx as i32) as f32,
                    (row + dy as i32) as f32,
                    color,
                    window,
                );
            }
        }
    }
}

/// Paints a small present centered on a point of the window.
pub(super) fn paint_gift(
    x: f32,
    y: f32,
    pixel: f32,
    alpha: f32,
    colors: &Palette,
    window: &mut Window,
) {
    const GIFT: [&str; 4] = [".y.", "ryr", "yyy", "ryr"];
    let dot = pixel * 0.75;
    for (row, line) in GIFT.iter().enumerate() {
        for (col, cell) in line.bytes().enumerate() {
            if let Some(color) = colors.fill(cell) {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            px(x + (col as f32 - 1.5) * dot),
                            px(y + (row as f32 - 2.) * dot),
                        ),
                        size(px(dot), px(dot)),
                    ),
                    color.opacity(alpha),
                ));
            }
        }
    }
}

/// Paints a small `!` with its foot at a point of the window.
pub(super) fn paint_bang(x: f32, y: f32, pixel: f32, colors: &Palette, window: &mut Window) {
    for row in [-5., -4., -3., -1.] {
        window.paint_quad(fill(
            Bounds::new(
                point(px(x - pixel / 2.), px(y + row * pixel)),
                size(px(pixel), px(pixel)),
            ),
            colors.yellow,
        ));
    }
}

/// Paints a small star centered on a point of the window.
pub(super) fn paint_star(
    x: f32,
    y: f32,
    pixel: f32,
    alpha: f32,
    colors: &Palette,
    window: &mut Window,
) {
    let dot = pixel * 0.75;
    for (dx, dy, share) in [
        (0., 0., 1.),
        (-1., 0., 0.6),
        (1., 0., 0.6),
        (0., -1., 0.6),
        (0., 1., 0.6),
    ] {
        window.paint_quad(fill(
            Bounds::new(
                point(px(x + dx * dot - dot / 2.), px(y + dy * dot - dot / 2.)),
                size(px(dot), px(dot)),
            ),
            colors.yellow.opacity(share * alpha),
        ));
    }
}

/// Paints a small `z` centered on a point of the window.
pub(super) fn paint_snore(
    x: f32,
    y: f32,
    pixel: f32,
    alpha: f32,
    colors: &Palette,
    window: &mut Window,
) {
    const Z: [&str; 4] = ["####", "..#.", ".#..", "####"];
    let dot = pixel * 0.6;
    for (row, line) in Z.iter().enumerate() {
        for (col, cell) in line.bytes().enumerate() {
            if cell == b'#' {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            px(x + (col as f32 - 2.) * dot),
                            px(y + (row as f32 - 2.) * dot),
                        ),
                        size(px(dot), px(dot)),
                    ),
                    colors.light.opacity(alpha),
                ));
            }
        }
    }
}

/// Returns a still portrait of an avatar, sized to its parent, for pickers. A skater stands on its
/// board. The others stand alone, as their rides do not fit.
pub(in crate::ui) fn portrait(avatar: SkaterAvatar, tone: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            // The grid spans 14 columns with the board's tails and 16 rows.
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            let pixel = (width / 14.).min(height / 16.).floor().max(1.);
            let pen = Pen {
                x: f32::from(bounds.origin.x) + width / 2.,
                y: f32::from(bounds.origin.y) + (height - 16. * pixel) / 2. + FEET * pixel,
                facing: 1.,
                angle: 0.,
                squeeze: 1.,
                pixel,
                snap: window.scale_factor(),
            };
            let colors = Palette::new(tone);
            let art = art(avatar);
            if art.ride == Ride::Board {
                paint_board(&pen, Deck::Up, &colors, window);
            }
            paint_character(
                &pen,
                &art,
                Pose {
                    dy: 0,
                    legs: Legs::Stand,
                    arms: if art.ride == Ride::Feet {
                        Arms::Forward
                    } else {
                        Arms::Down
                    },
                    eyes: Eyes::Open,
                    head: 0.,
                },
                &colors,
                window,
            );
        },
    )
    .size_full()
}
