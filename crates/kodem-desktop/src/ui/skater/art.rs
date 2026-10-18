//! The skater's pixel art: a sprite for each avatar, the shared legs, arms and board, and the pen
//! that paints them mirrored, rotated and squeezed.
//!
//! The art lives on one grid of cells that faces right. Columns run from 0 to 11 with the middle
//! at 6. Rows 0 to 11 hold an avatar's sprite, rows 12 and 13 its legs, and rows 13 to 15 the
//! board, whose deck top is row 14: the line that the feet stand on.
use super::*;

/// Column of the middle of the grid.
const MIDDLE: f32 = 6.;
/// Row of the line that the feet stand on, which is also the deck's top.
pub(super) const FEET: f32 = 14.;
/// Rows from the feet line to the bottom of the wheels.
pub(super) const DECK_DEPTH: f32 = 2.;
/// Rows from the feet line to the eyes, where a held character is picked up.
pub(super) const SCRUFF: f32 = 11.;

/// How an avatar is drawn on the grid.
pub(super) struct Art {
    rows: [&'static str; 12],
    /// Rows that the sprite sits lower, for a character without legs that rests on the deck.
    drop: i32,
    legs: bool,
    arms: bool,
    /// The back shoulder's column, the front shoulder's column and the row of both.
    shoulders: (i32, i32, i32),
    /// The cell that a closed eye shows.
    lid: u8,
    /// The eyes glow instead of being dark.
    glow: bool,
    /// The character hovers over the deck and bobs there.
    pub floats: bool,
}

/// Returns the art of an avatar.
pub(super) fn art(avatar: SkaterAvatar) -> Art {
    let standard = Art {
        rows: CAT,
        drop: 0,
        legs: true,
        arms: true,
        shoulders: (2, 9, 9),
        lid: b'b',
        glow: false,
        floats: false,
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
    }
}

// Cells: `o` outline, `b` body, `l` light, `d` shade, `c` cream, `w` white, `p` blush, `n` mouth,
// `y` yellow, `r` red, `k` screen, and `e` over `f` for the top and bottom of each eye.
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
            b'k' => self.screen,
            _ => return None,
        })
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
        let (shade, outline) = (colors.shade, colors.outline);
        for &(col, row, foot) in legs(pose.legs) {
            pen.cell(
                f32::from(col),
                f32::from(row),
                if foot { outline } else { shade },
                window,
            );
        }
    }
    let dy = art.drop + pose.dy;
    for (row, line) in art.rows.iter().enumerate() {
        for (col, cell) in line.bytes().enumerate() {
            if let Some(color) = colors.cell(cell, pose.eyes, art) {
                pen.cell(col as f32, row as f32 + dy as f32, color, window);
            }
        }
    }
    if art.arms {
        let (back, front, shoulder) = art.shoulders;
        let shoulder = shoulder + dy;
        let (back_arm, front_arm) = arms(pose.arms);
        for (base, reach, side) in [(back, back_arm, -1), (front, front_arm, 1)] {
            for (i, &(out, down)) in reach.iter().enumerate() {
                let hand = i + 1 == reach.len();
                pen.cell(
                    (base + out * side) as f32,
                    (shoulder + down) as f32,
                    if hand { colors.body } else { colors.shade },
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
pub(super) fn paint_star(x: f32, y: f32, pixel: f32, colors: &Palette, window: &mut Window) {
    let dot = pixel * 0.75;
    for (dx, dy, alpha) in [
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
            colors.yellow.opacity(alpha),
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

/// Returns a still portrait of an avatar on its board, sized to its parent, for pickers.
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
            paint_board(&pen, Deck::Up, &colors, window);
            paint_character(
                &pen,
                &art(avatar),
                Pose {
                    dy: 0,
                    legs: Legs::Stand,
                    arms: Arms::Down,
                    eyes: Eyes::Open,
                },
                &colors,
                window,
            );
        },
    )
    .size_full()
}
