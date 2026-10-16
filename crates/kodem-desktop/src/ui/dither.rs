//! Ordered (Bayer) dithering: the workspace's shading, depth and working-state language.
//!
//! Two layers share one 8×8 Bayer threshold. Fine one-point cells anchored to the window grid
//! are texture: shadows, scrims and fades where content passes under chrome. Dot-matrix fields
//! (`Dither`) are drawn as separate square dots on a two- or three-point pitch, so smooth values
//! turn into the regular patterns of retro displays and print halftones. They shade surfaces,
//! mark selection and report work.
//!
//! Static fields are baked into cached sprites, so they cost one draw call per frame. Animated
//! fields advance on a stepped clock of about twelve frames per second. The steps are
//! part of the look and keep the redraw cost low. With reduced motion, fields render one frame.
use super::*;
// This module paints in window pixels: the dot pitch belongs to the texture and stays the same
// at every interface scale. Only the sizes of whole elements and their corners follow the scale.
use gpui::px;
use std::{cell::RefCell, f32::consts::TAU, rc::Rc, sync::Arc, time::Instant};

/// Cells per side of a cached texture tile; a multiple of the Bayer period.
const TILE_CELLS: u32 = 32;

/// Returns the threshold of a window-grid cell in the open interval (0, 1).
fn threshold(x: i64, y: i64) -> f32 {
    crate::bayer::threshold(x, y)
}

/// Returns the Bayer threshold of one dot cell, for effects that paint their own fields.
pub(super) fn cell_threshold(column: i32, row: i32) -> f32 {
    threshold(column as i64, row as i64)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// Paints the cells inside `bounds` whose density exceeds their threshold.
///
/// `density` receives cell centers relative to the top-left of `bounds`. Adjacent lit cells in a
/// row become one quad, and the whole field shares a single draw order.
fn paint_cells(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    cell: f32,
    color: Hsla,
    density: impl Fn(f32, f32) -> f32,
) {
    let (left, top) = (f32::from(bounds.left()), f32::from(bounds.top()));
    let (right, bottom) = (f32::from(bounds.right()), f32::from(bounds.bottom()));
    if right <= left || bottom <= top || color.alpha <= 0. {
        return;
    }
    let (x0, x1) = ((left / cell).floor() as i64, (right / cell).ceil() as i64);
    let (y0, y1) = ((top / cell).floor() as i64, (bottom / cell).ceil() as i64);
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_layer(bounds, |window| {
            for y in y0..y1 {
                let cy = (y as f32 + 0.5) * cell - top;
                let mut run: Option<i64> = None;
                for x in x0..=x1 {
                    let lit =
                        x < x1 && density((x as f32 + 0.5) * cell - left, cy) > threshold(x, y);
                    match (lit, run) {
                        (true, None) => run = Some(x),
                        (false, Some(start)) => {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(px(start as f32 * cell), px(y as f32 * cell)),
                                    size(px((x - start) as f32 * cell), px(cell)),
                                ),
                                color,
                            ));
                            run = None;
                        }
                        _ => {}
                    }
                }
            }
        });
    });
}

/// Fills small bounds with a flat dithered density during an element's own paint.
pub(super) fn paint_flat(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    cell: f32,
    color: Hsla,
    density: f32,
) {
    paint_cells(window, bounds, cell, color, |_, _| density);
}

/// Returns whether a point lies inside a rounded rectangle anchored at the origin.
fn inside_rounded(x: f32, y: f32, width: f32, height: f32, radius: f32) -> bool {
    if x < 0. || y < 0. || x > width || y > height {
        return false;
    }
    let dx = (radius - x.min(width - x)).max(0.);
    let dy = (radius - y.min(height - y)).max(0.);
    dx * dx + dy * dy <= radius * radius
}

/// Cells per side of a strip sprite; a multiple of the Bayer period.
const STRIP_CELLS: u32 = 64;
/// Bytes of new sprites one frame may bake. Fields beyond it are painted as quads for that frame.
const FRAME_BUDGET: usize = 2 << 20;
/// Largest sprite worth caching, in bytes.
const SPRITE_LIMIT: usize = 4 << 20;
/// Bytes of cached sprites kept before the least recently used ones are dropped.
const CACHE_LIMIT: usize = 48 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SpriteKey {
    /// Digest of everything that decides which dots are lit and in which color.
    content: u64,
    columns: u32,
    rows: u32,
    /// Cell and dot size in device pixels.
    cell: u32,
    dot: u32,
}

struct Sprite {
    image: Arc<RenderImage>,
    bytes: usize,
    used: u64,
}

// Static dot fields baked at device resolution. A cached field costs one sprite per frame
// instead of one quad per dot.
#[derive(Default)]
struct Bakery {
    sprites: std::collections::HashMap<SpriteKey, Sprite>,
    bytes: usize,
    frame: u64,
    spent: usize,
}

thread_local! {
    static BAKERY: RefCell<Bakery> = RefCell::default();
}

/// Starts a frame of sprite baking. Call it once before the window's content renders.
pub(super) fn begin_frame() {
    BAKERY.with(|bakery| {
        let mut bakery = bakery.borrow_mut();
        bakery.frame += 1;
        bakery.spent = 0;
    });
}

// Returns straight-alpha BGRA, the layout of GPUI's sprite atlas.
fn texel(color: Hsla) -> [u8; 4] {
    let rgba = hsla_to_rgba(color);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    [
        channel(rgba.blue),
        channel(rgba.green),
        channel(rgba.red),
        channel(rgba.alpha),
    ]
}

fn bake(key: SpriteKey, source: &dyn Fn(i32, i32) -> Option<Hsla>) -> Arc<RenderImage> {
    let (cell, dot) = (key.cell as usize, key.dot as usize);
    let (width, height) = (key.columns as usize * cell, key.rows as usize * cell);
    let inset = (cell - dot) / 2;
    let mut data = vec![0u8; width * height * 4];
    let mut line = vec![0u8; width * 4];
    for row in 0..key.rows as usize {
        line.fill(0);
        let mut lit = false;
        for column in 0..key.columns as usize {
            let Some(color) = source(column as i32, row as i32) else {
                continue;
            };
            let pixel = texel(color);
            if pixel[3] == 0 {
                continue;
            }
            lit = true;
            let start = (column * cell + inset) * 4;
            for target in line[start..start + dot * 4].chunks_exact_mut(4) {
                target.copy_from_slice(&pixel);
            }
        }
        if !lit {
            continue;
        }
        for y in 0..dot {
            let start = (row * cell + inset + y) * width * 4;
            data[start..start + width * 4].copy_from_slice(&line);
        }
    }
    let buffer = image::RgbaImage::from_raw(width as u32, height as u32, data)
        .expect("sprite buffer matches its dimensions");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

// Returns the cached sprite for `key`, baking it when this frame's budget allows.
fn sprite(
    window: &mut Window,
    cx: &mut App,
    key: SpriteKey,
    source: &dyn Fn(i32, i32) -> Option<Hsla>,
) -> Option<Arc<RenderImage>> {
    let bytes = (key.columns * key.cell) as usize * (key.rows * key.cell) as usize * 4;
    BAKERY.with(|bakery| {
        let mut bakery = bakery.borrow_mut();
        let frame = bakery.frame;
        if let Some(sprite) = bakery.sprites.get_mut(&key) {
            sprite.used = frame;
            return Some(sprite.image.clone());
        }
        if bytes == 0
            || bytes > SPRITE_LIMIT
            || (bakery.spent > 0 && bakery.spent + bytes > FRAME_BUDGET)
        {
            return None;
        }
        let image = bake(key, source);
        bakery.spent += bytes;
        bakery.bytes += bytes;
        bakery.sprites.insert(
            key,
            Sprite {
                image: image.clone(),
                bytes,
                used: frame,
            },
        );
        if bakery.bytes > CACHE_LIMIT {
            // Sprites of this and the previous frame may still be on screen.
            let mut idle: Vec<(u64, SpriteKey)> = bakery
                .sprites
                .iter()
                .filter(|(_, sprite)| sprite.used + 1 < frame)
                .map(|(key, sprite)| (sprite.used, *key))
                .collect();
            idle.sort_unstable_by_key(|(used, _)| *used);
            for (_, key) in idle {
                if bakery.bytes <= CACHE_LIMIT / 4 * 3 {
                    break;
                }
                if let Some(sprite) = bakery.sprites.remove(&key) {
                    bakery.bytes -= sprite.bytes;
                    cx.drop_image(sprite.image, Some(window));
                }
            }
        }
        Some(image)
    })
}

/// A grid of square dots in window coordinates.
#[derive(Clone, Copy)]
struct Grid {
    left: f32,
    top: f32,
    columns: u32,
    rows: u32,
    cell: f32,
    dot: f32,
}

impl Grid {
    fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(
            point(px(self.left), px(self.top)),
            size(
                px(self.columns as f32 * self.cell),
                px(self.rows as f32 * self.cell),
            ),
        )
    }
}

// Paints the dots that `source` lights. With `content`, the grid is drawn from a cached sprite of
// `tile` cells that repeats across the grid; a strip of a field that varies along one axis serves
// every size along the other. Without it, or while the sprite cannot be baked, each dot is a quad.
fn paint_dots(
    window: &mut Window,
    cx: &mut App,
    grid: Grid,
    tile: (u32, u32),
    content: Option<u64>,
    radii: Corners<f32>,
    source: &dyn Fn(i32, i32) -> Option<Hsla>,
) {
    if grid.columns == 0 || grid.rows == 0 {
        return;
    }
    let bounds = grid.bounds();
    if !window.content_mask().bounds.intersects(&bounds) {
        return;
    }
    let scale = window.scale_factor();
    let device_cell = (grid.cell * scale).round();
    // A sprite is sharp only when a cell covers whole device pixels.
    if let Some(content) = content
        && device_cell >= 1.
        && (grid.cell * scale - device_cell).abs() < 0.01
    {
        let key = SpriteKey {
            content,
            columns: tile.0,
            rows: tile.1,
            cell: device_cell as u32,
            dot: ((grid.dot * scale).round() as u32).clamp(1, device_cell as u32),
        };
        if let Some(image) = sprite(window, cx, key, source) {
            paint_sprite(window, image, bounds, tile, grid.cell, radii);
            return;
        }
    }
    paint_quads(window, grid, radii, source);
}

// Repeats a sprite across `bounds` and rounds the corners that coincide with the grid's corners.
fn paint_sprite(
    window: &mut Window,
    image: Arc<RenderImage>,
    bounds: Bounds<Pixels>,
    tile: (u32, u32),
    cell: f32,
    radii: Corners<f32>,
) {
    let (tile_width, tile_height) = (tile.0 as f32 * cell, tile.1 as f32 * cell);
    let (left, top) = (f32::from(bounds.left()), f32::from(bounds.top()));
    let (right, bottom) = (f32::from(bounds.right()), f32::from(bounds.bottom()));
    let mut y = top;
    while y < bottom {
        let mut x = left;
        while x < right {
            let region = Bounds::new(point(px(x), px(y)), size(px(tile_width), px(tile_height)));
            let (first_column, last_column) = (x <= left, x + tile_width >= right);
            let (first_row, last_row) = (y <= top, y + tile_height >= bottom);
            let corner = |rounded: bool, radius: f32| px(if rounded { radius } else { 0. });
            let tile_radii = Corners {
                top_left: corner(first_column && first_row, radii.top_left),
                top_right: corner(last_column && first_row, radii.top_right),
                bottom_right: corner(last_column && last_row, radii.bottom_right),
                bottom_left: corner(first_column && last_row, radii.bottom_left),
            };
            let _ = window.paint_image(
                region.intersect(&bounds),
                region,
                tile_radii,
                image.clone(),
                0,
                false,
            );
            x += tile_width;
        }
        y += tile_height;
    }
}

// Paints each lit dot as a quad in one layer. Solid cells merge into runs.
fn paint_quads(
    window: &mut Window,
    grid: Grid,
    radii: Corners<f32>,
    source: &dyn Fn(i32, i32) -> Option<Hsla>,
) {
    let bounds = grid.bounds();
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let cell = grid.cell;
    let dot = grid.dot.clamp(0.5, cell);
    let inset = (cell - dot) / 2.;
    let solid = dot >= cell;
    let columns = grid.columns as i32;
    window.paint_layer(bounds, |window| {
        for row in 0..grid.rows as i32 {
            let y = grid.top + row as f32 * cell;
            let mut run: Option<(i32, Hsla)> = None;
            for column in 0..=columns {
                let lit = if column < columns
                    && !outside_rounded_corner(
                        (column as f32 + 0.5) * cell,
                        (row as f32 + 0.5) * cell,
                        width,
                        height,
                        radii,
                    ) {
                    source(column, row)
                } else {
                    None
                };
                if !solid {
                    if let Some(color) = lit {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(px(grid.left + column as f32 * cell + inset), px(y + inset)),
                                size(px(dot), px(dot)),
                            ),
                            color,
                        ));
                    }
                    continue;
                }
                if let (Some(color), Some((_, current))) = (lit, run)
                    && color == current
                {
                    continue;
                }
                if let Some((start, color)) = run {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(px(grid.left + start as f32 * cell), px(y)),
                            size(px((column - start) as f32 * cell), px(cell)),
                        ),
                        color,
                    ));
                }
                run = lit.map(|color| (column, color));
            }
        }
    });
}

// Starts a digest of the values that decide a sprite's content.
fn digest(color: Hsla, values: &[f32]) -> std::collections::hash_map::DefaultHasher {
    use std::hash::Hash;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    texel(color).hash(&mut hasher);
    for value in values {
        value.to_bits().hash(&mut hasher);
    }
    hasher
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Pattern {
    /// A flat density from 0 to 64.
    Uniform(u8),
    /// A vertical ramp `cells` tall that is dense at the top when `falling`.
    Ramp { cells: u16, falling: bool },
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TileKey {
    pattern: Pattern,
    color: [u8; 4],
    cell: u32,
    scale: u32,
}
thread_local! {
    static TILES: RefCell<std::collections::HashMap<TileKey, Arc<RenderImage>>> =
        Default::default();
}

// Bakes a straight-alpha BGRA tile at device resolution so large areas cost a few sprites.
fn tile(key: TileKey) -> Arc<RenderImage> {
    TILES.with(|tiles| {
        let mut tiles = tiles.borrow_mut();
        if let Some(image) = tiles.get(&key) {
            return image.clone();
        }
        if tiles.len() > 256 {
            tiles.clear();
        }
        let device_cell =
            ((key.cell as f32 / 100. * key.scale as f32 / 100.).round() as u32).max(1);
        let rows = match key.pattern {
            Pattern::Uniform(_) => TILE_CELLS,
            Pattern::Ramp { cells, .. } => cells as u32,
        };
        let (width, height) = (TILE_CELLS * device_cell, rows * device_cell);
        // GPUI's sprite atlas stores straight alpha, matching its decoded images and SVGs.
        let [r, g, b, a] = key.color;
        let pixel = [b, g, r, a];
        let mut data = vec![0u8; (width * height * 4) as usize];
        for cy in 0..rows {
            let density = match key.pattern {
                Pattern::Uniform(level) => level as f32 / 64.,
                Pattern::Ramp { cells, falling } => {
                    let t = (cy as f32 + 0.5) / cells as f32;
                    let t = if falling { 1. - t } else { t };
                    t * t * (3. - 2. * t)
                }
            };
            for cx in 0..TILE_CELLS {
                if density <= threshold(cx as i64, cy as i64) {
                    continue;
                }
                for dy in 0..device_cell {
                    let row = ((cy * device_cell + dy) * width + cx * device_cell) as usize * 4;
                    for dx in 0..device_cell as usize {
                        data[row + dx * 4..row + dx * 4 + 4].copy_from_slice(&pixel);
                    }
                }
            }
        }
        let buffer = image::RgbaImage::from_raw(width, height, data)
            .expect("tile buffer matches its dimensions");
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
        tiles.insert(key, image.clone());
        image
    })
}

// Repeats a cached tile horizontally across `bounds`, anchored to the window grid.
fn paint_tiles(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    cell: f32,
    color: Hsla,
    pattern: Pattern,
) {
    let rgba = hsla_to_rgba(color);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    let key = TileKey {
        pattern,
        color: [
            channel(rgba.red),
            channel(rgba.green),
            channel(rgba.blue),
            channel(rgba.alpha),
        ],
        cell: (cell * 100.).round() as u32,
        scale: (window.scale_factor() * 100.).round() as u32,
    };
    if key.color[3] == 0 {
        return;
    }
    let image = tile(key);
    let tile_width = TILE_CELLS as f32 * cell;
    let tile_height = match pattern {
        Pattern::Uniform(_) => tile_width,
        Pattern::Ramp { cells, .. } => cells as f32 * cell,
    };
    let left = (f32::from(bounds.left()) / tile_width).floor() * tile_width;
    let top = match pattern {
        Pattern::Uniform(_) => (f32::from(bounds.top()) / tile_height).floor() * tile_height,
        Pattern::Ramp { .. } => f32::from(bounds.top()),
    };
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let mut y = top;
        while y < f32::from(bounds.bottom()) {
            let mut x = left;
            while x < f32::from(bounds.right()) {
                let region =
                    Bounds::new(point(px(x), px(y)), size(px(tile_width), px(tile_height)));
                let _ =
                    window.paint_image(region, region, Corners::default(), image.clone(), 0, false);
                x += tile_width;
            }
            y += tile_height;
        }
    });
}

/// Covers the window with a fine screen-door scrim beneath a modal dialog.
pub(super) fn scrim(theme: Theme) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.paint_quad(fill(bounds, theme.canvas.opacity(0.42)));
            paint_tiles(window, bounds, 1., theme.ink, Pattern::Uniform(32));
        },
    )
    .absolute()
    .inset_0()
}

/// Shades the area it occupies from solid `color` at one edge to clear at the other.
///
/// Used where content slides beneath fixed chrome, in place of a gradient or blur.
pub(super) fn fade(color: Hsla, height: f32, solid_at_bottom: bool) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let cells = (f32::from(bounds.size.height).max(1.)).ceil() as u16;
            paint_tiles(
                window,
                bounds,
                1.,
                color,
                Pattern::Ramp {
                    cells,
                    falling: !solid_at_bottom,
                },
            );
        },
    )
    .absolute()
    .left_0()
    .right_0()
    .h(crate::theme::px(height))
}

/// Casts a dithered contact shadow below and right of its parent's rounded bounds.
///
/// Place it as the first child of a relatively positioned wrapper that has the same bounds as
/// the floating surface, so the surface paints above it.
pub(super) fn shadow(theme: Theme, radius: f32, depth: f32) -> impl IntoElement {
    let scale = crate::theme::ui_scale();
    let (radius, depth) = (radius * scale, depth * scale);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| paint_shadow(window, cx, bounds, radius, depth, theme.ink),
    )
    .absolute()
    .inset_0()
}

// Covers only the uncovered bands around the surface: beside its right edge and below it.
fn paint_shadow(
    window: &mut Window,
    cx: &mut App,
    surface: Bounds<Pixels>,
    radius: f32,
    depth: f32,
    ink: Hsla,
) {
    use std::hash::{Hash, Hasher};
    let (left, top) = (
        f32::from(surface.left()).floor(),
        f32::from(surface.top()).floor(),
    );
    let (width, height) = (
        f32::from(surface.size.width).round(),
        f32::from(surface.size.height).round(),
    );
    if width < 1. || height < 1. {
        return;
    }
    let (offset_x, offset_y) = (depth * 0.4, depth);
    let spread = depth * 1.5;
    let right = (width + offset_x + spread).ceil();
    let bottom = (height + offset_y + spread).ceil();
    // The surface hides everything left of its right edge above this row.
    let split = (height - radius).floor().max(0.);
    let beside = (width - 1.).max(0.);
    let bands = [
        (0u8, beside, 0., right - beside, split),
        (1u8, 0., split, right, bottom - split),
    ];
    for (band, x0, y0, columns, rows) in bands {
        let source = |column: i32, row: i32| -> Option<Hsla> {
            // Cell positions relative to the surface, so both bands share one pattern.
            let (cell_x, cell_y) = (x0 as i32 + column, y0 as i32 + row);
            let (x, y) = (cell_x as f32 + 0.5, cell_y as f32 + 0.5);
            if inside_rounded(x, y, width, height, radius) {
                return None;
            }
            // Distance outside the offset rounded rectangle, in points.
            let (sx, sy) = (x - offset_x, y - offset_y);
            let qx = (sx - width / 2.).abs() - (width / 2. - radius);
            let qy = (sy - height / 2.).abs() - (height / 2. - radius);
            let outside = qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - radius;
            let cast = 0.62 * (1. - smoothstep(0., spread, outside));
            (cast > threshold(cell_x as i64, cell_y as i64)).then_some(ink)
        };
        let mut content = digest(ink, &[width, height, radius, depth]);
        band.hash(&mut content);
        let grid = Grid {
            left: left + x0,
            top: top + y0,
            columns: columns.max(0.) as u32,
            rows: rows.max(0.) as u32,
            cell: 1.,
            dot: 1.,
        };
        paint_dots(
            window,
            cx,
            grid,
            (grid.columns, grid.rows),
            Some(content.finish()),
            Corners::default(),
            &source,
        );
    }
}

/// Time between steps of animated dot-matrix fields.
const FRAME: Duration = Duration::from_millis(80);

thread_local! {
    // Views that already wait for the next step, so each schedules one timer.
    static PENDING: RefCell<HashSet<EntityId>> = RefCell::default();
}

// Returns the stepped animation time in seconds. Every animated element reads this clock, so
// they advance together and the window redraws at most once per step.
fn clock_seconds() -> f32 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let elapsed = START.get_or_init(Instant::now).elapsed();
    let steps = elapsed.as_millis() / FRAME.as_millis();
    steps as f32 * FRAME.as_secs_f32()
}

/// Returns the phase of a repeating cycle on the stepped clock, from 0 to 1.
pub(super) fn cycle(period: Duration) -> f32 {
    (clock_seconds() / period.as_secs_f32()).fract()
}

// Schedules one redraw of the view that is being painted, at the next step of the clock.
//
// The next step is scheduled from a paint, so the clock runs in every window that draws, focused
// or not, and rests while the window is hidden and the system asks for no frames.
fn schedule_step(window: &Window, cx: &mut App) {
    let view = window.current_view();
    if !PENDING.with(|pending| pending.borrow_mut().insert(view)) {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor().timer(FRAME).await;
        cx.update(|cx| {
            PENDING.with(|pending| pending.borrow_mut().remove(&view));
            cx.notify(view);
        });
    })
    .detach();
}

/// An invisible element that redraws its view on the next clock step.
///
/// Add it beside content whose look comes from [`cycle`]. An unfocused window keeps stepping; a
/// hidden window is not painted, so it schedules nothing until it shows again.
pub(super) fn ticker() -> impl IntoElement {
    canvas(|_, _, _| {}, |_, _, window, cx| schedule_step(window, cx))
        .absolute()
        .size(px(1.))
}

/// Draws a 4×4 working glyph that fills and empties in Bayer order.
pub(super) fn pulse(color: Hsla, reduce_motion: bool) -> AnyElement {
    let level = if reduce_motion {
        0.5
    } else {
        0.5 - 0.5 * (cycle(Duration::from_millis(1600)) * TAU).cos()
    };
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            const ORDER: [[u8; 4]; 4] =
                [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
            let pitch = f32::from(bounds.size.width.min(bounds.size.height)) / 4.;
            let dot = (pitch - 0.75).max(1.);
            window.paint_layer(bounds, |window| {
                for (row, values) in ORDER.iter().enumerate() {
                    for (col, value) in values.iter().enumerate() {
                        let lit = level > (*value as f32 + 0.5) / 16.;
                        window.paint_quad(fill(
                            Bounds::new(
                                bounds.origin
                                    + point(px(col as f32 * pitch), px(row as f32 * pitch)),
                                size(px(dot), px(dot)),
                            ),
                            color.opacity(if lit { 1. } else { 0.16 }),
                        ));
                    }
                }
            });
            if !reduce_motion {
                schedule_step(window, cx);
            }
        },
    )
    .size(crate::theme::px(13.))
    .flex_shrink_0()
    .into_any_element()
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut value = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ seed.wrapping_mul(0xCB1A_B31F);
    value ^= value >> 13;
    value = value.wrapping_mul(0x5BD1_E995);
    value ^= value >> 15;
    (value & 0xFFFF) as f32 / 65_535.
}

fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let sx = fx * fx * (3. - 2. * fx);
    let sy = fy * fy * (3. - 2. * fy);
    let (ix, iy) = (x0 as i32, y0 as i32);
    let top = lerp(hash(ix, iy, seed), hash(ix + 1, iy, seed), sx);
    let bottom = lerp(hash(ix, iy + 1, seed), hash(ix + 1, iy + 1, seed), sx);
    lerp(top, bottom, sy)
}

// Two octaves of value noise, normalized to 0..1.
fn fractal_noise(x: f32, y: f32, seed: u32) -> f32 {
    value_noise(x, y, seed) * 0.65
        + value_noise(x * 2.03 + 17., y * 2.03 + 5., seed ^ 0x9E37) * 0.35
}

/// The edge where a fade starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    fn distance(self, u: f32, v: f32) -> f32 {
        match self {
            Edge::Left => u,
            Edge::Right => 1. - u,
            Edge::Top => v,
            Edge::Bottom => 1. - v,
        }
    }
}

/// One dot cell of a field's grid, with the values a field needs.
#[derive(Debug, Clone, Copy)]
pub(super) struct Sample {
    /// Horizontal position, 0 at the left edge and 1 at the right.
    pub u: f32,
    /// Vertical position, 0 at the top edge and 1 at the bottom.
    pub v: f32,
    pub column: i32,
    pub row: i32,
    pub rows: i32,
    /// Width divided by height.
    pub aspect: f32,
    /// Seconds on the stepped animation clock.
    pub time: f32,
    /// Pointer position in `u, v` space while the pointer is over the grid.
    pub pointer: Option<(f32, f32)>,
}

/// A function that gives the dot density at each cell.
#[derive(Clone)]
pub(super) enum DitherField {
    /// Density goes from `start` at `from` to `end` at the opposite edge.
    Fade { from: Edge, start: f32, end: f32 },
    /// Density peaks at `x, y` (in 0..1 space) and falls off with distance.
    Glow {
        x: f32,
        y: f32,
        radius: f32,
        strength: f32,
    },
    /// Slowly drifting noise, like light on moving water.
    Flow { scale: f32, speed: f32, bias: f32 },
    /// A liquid fill with a moving wave surface. `level` is measured from the bottom edge.
    Wave {
        level: f32,
        amplitude: f32,
        speed: f32,
    },
    /// A packet of light that travels from left to right and repeats.
    Sweep { width: f32, speed: f32, floor: f32 },
    /// A dithered area chart. Values are normalized to `0..1`.
    Area { values: Rc<[f32]> },
    /// Flickering flames that rise from the bottom edge.
    Ember { scale: f32, speed: f32 },
    /// A glow that follows the pointer while it is over the grid.
    Pointer { radius: f32, strength: f32 },
    /// Covers the surface while progress goes from 0 to 1, then reveals it from the top.
    Dissolve(f32),
    /// Multiplies two fields.
    Product(Rc<DitherField>, Rc<DitherField>),
}

impl DitherField {
    /// Multiplies this field by another field.
    pub(super) fn times(self, other: DitherField) -> Self {
        DitherField::Product(Rc::new(self), Rc::new(other))
    }

    /// Returns the density of one cell.
    pub(super) fn intensity(&self, sample: &Sample) -> f32 {
        match self {
            DitherField::Fade { from, start, end } => {
                lerp(*start, *end, from.distance(sample.u, sample.v))
            }
            DitherField::Glow {
                x,
                y,
                radius,
                strength,
            } => {
                let dx = (sample.u - x) * sample.aspect;
                let dy = sample.v - y;
                let distance = (dx * dx + dy * dy).sqrt() / radius.max(0.001);
                strength * (1. - distance).max(0.).powf(1.4)
            }
            DitherField::Flow { scale, speed, bias } => {
                let x = sample.column as f32 * scale + sample.time * speed;
                let y = sample.row as f32 * scale - sample.time * speed * 0.55;
                (fractal_noise(x, y, 7) + bias).clamp(0., 1.)
            }
            DitherField::Wave {
                level,
                amplitude,
                speed,
            } => {
                let phase = sample.u * TAU;
                let surface = 1. - level
                    + amplitude
                        * (0.6 * (phase * 1.3 + sample.time * speed).sin()
                            + 0.4 * (phase * 2.9 - sample.time * speed * 1.4).sin());
                if sample.v < surface {
                    return 0.;
                }
                let cell = 1. / sample.rows.max(1) as f32;
                if sample.v - surface < cell {
                    return 1.;
                }
                let depth = ((sample.v - surface) / (1. - surface).max(0.001)).clamp(0., 1.);
                let shimmer = fractal_noise(
                    sample.column as f32 * 0.18 + sample.time * 0.8,
                    sample.row as f32 * 0.3,
                    11,
                );
                (0.28 + depth * 0.55 + (shimmer - 0.5) * 0.3).clamp(0., 1.)
            }
            DitherField::Sweep {
                width,
                speed,
                floor,
            } => {
                let center = (sample.time * speed).fract() * (1. + width * 4.) - width * 2.;
                let distance = (sample.u - center) / width.max(0.001);
                floor + (1. - floor) * (-distance * distance).exp()
            }
            DitherField::Area { values } => {
                if values.is_empty() {
                    return 0.;
                }
                let position = sample.u * (values.len().saturating_sub(1)) as f32;
                let left = position.floor() as usize;
                let right = (left + 1).min(values.len() - 1);
                let value = lerp(values[left], values[right], position - left as f32);
                let top = 1. - value.clamp(0., 1.);
                if sample.v < top {
                    return 0.;
                }
                let cell = 1. / sample.rows.max(1) as f32;
                if sample.v - top < cell {
                    return 1.;
                }
                let depth = (sample.v - top) / (1. - top).max(0.001);
                0.16 + depth * 0.62
            }
            DitherField::Ember { scale, speed } => {
                let x = sample.column as f32 * scale;
                let y = sample.row as f32 * scale + sample.time * speed;
                let flame = fractal_noise(x, y, 3);
                let rise = sample.v.powf(1.6);
                (flame * rise * 1.55 - 0.08).clamp(0., 1.)
            }
            DitherField::Pointer { radius, strength } => {
                let Some((x, y)) = sample.pointer else {
                    return 0.;
                };
                let dx = (sample.u - x) * sample.aspect;
                let dy = sample.v - y;
                let distance = (dx * dx + dy * dy).sqrt() / radius.max(0.001);
                strength * (1. - distance).max(0.)
            }
            DitherField::Dissolve(progress) => {
                ((1. - progress) * 1.6 - sample.v * 0.6).clamp(0., 1.)
            }
            DitherField::Product(left, right) => left.intensity(sample) * right.intensity(sample),
        }
    }

    // Reports a field that varies along one axis only, so a strip of it serves any length
    // along the other axis.
    fn axis(&self) -> Option<Axis> {
        match self {
            DitherField::Fade {
                from: Edge::Top | Edge::Bottom,
                ..
            } => Some(Axis::Vertical),
            DitherField::Fade { .. } => Some(Axis::Horizontal),
            DitherField::Product(left, right) => {
                let axis = left.axis()?;
                (right.axis() == Some(axis)).then_some(axis)
            }
            _ => None,
        }
    }

    // Reports a field whose dots move with the animation clock.
    fn timed(&self) -> bool {
        match self {
            DitherField::Flow { .. }
            | DitherField::Wave { .. }
            | DitherField::Sweep { .. }
            | DitherField::Ember { .. } => true,
            DitherField::Product(left, right) => left.timed() || right.timed(),
            _ => false,
        }
    }

    // Reports a field that looks the same on every frame, so its dots can be cached.
    fn cacheable(&self) -> bool {
        match self {
            // The pointer moves without the clock, so its glow is drawn on every frame.
            DitherField::Dissolve(_) | DitherField::Pointer { .. } => false,
            DitherField::Product(left, right) => left.cacheable() && right.cacheable(),
            _ => true,
        }
    }

    fn digest(&self, hasher: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        let mut values = |tag: u8, values: &[f32]| {
            tag.hash(hasher);
            for value in values {
                value.to_bits().hash(hasher);
            }
        };
        match self {
            DitherField::Fade { from, start, end } => values(*from as u8, &[*start, *end]),
            DitherField::Glow {
                x,
                y,
                radius,
                strength,
            } => values(8, &[*x, *y, *radius, *strength]),
            DitherField::Flow { scale, speed, bias } => values(9, &[*scale, *speed, *bias]),
            DitherField::Wave {
                level,
                amplitude,
                speed,
            } => values(10, &[*level, *amplitude, *speed]),
            DitherField::Sweep {
                width,
                speed,
                floor,
            } => values(11, &[*width, *speed, *floor]),
            DitherField::Area { values: points } => values(12, points),
            DitherField::Ember { scale, speed } => values(15, &[*scale, *speed]),
            DitherField::Pointer { radius, strength } => values(16, &[*radius, *strength]),
            DitherField::Dissolve(progress) => values(13, &[*progress]),
            DitherField::Product(left, right) => {
                values(14, &[]);
                left.digest(hasher);
                right.digest(hasher);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone)]
struct PaintSpec {
    field: DitherField,
    color: Hsla,
    cell: f32,
    dot: f32,
    toned: bool,
    hover_only: bool,
    corner_radii: Corners<f32>,
    spans: Size<bool>,
    /// Whether the field advances with the clock on this frame.
    moving: bool,
    time: f32,
}

// Returns the dot alpha for one intensity. Three steps keep the limited palette of retro displays.
fn tone(intensity: f32) -> f32 {
    if intensity > 0.82 {
        1.
    } else if intensity > 0.5 {
        0.72
    } else {
        0.46
    }
}

fn outside_rounded_corner(x: f32, y: f32, width: f32, height: f32, radii: Corners<f32>) -> bool {
    let left = x < width / 2.;
    let (top, bottom) = if left {
        (radii.top_left, radii.bottom_left)
    } else {
        (radii.top_right, radii.bottom_right)
    };
    // A grid shorter than its radius keeps the top corner's arc over its whole height.
    let (radius, corner_y) = if y < top {
        (top, top)
    } else if y > height - bottom {
        (bottom, height - bottom)
    } else {
        return false;
    };
    let corner_x = if left { radius } else { width - radius };
    if (x < corner_x) != left {
        return false;
    }
    let (dx, dy) = (x - corner_x, y - corner_y);
    dx * dx + dy * dy > radius * radius
}

fn paint_grid(bounds: Bounds<Pixels>, spec: &PaintSpec, window: &mut Window, cx: &mut App) {
    use std::hash::{Hash, Hasher};
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let cell = spec.cell.max(1.);
    if width < cell || height < cell || spec.color.alpha <= 0. {
        return;
    }
    let mouse = window.mouse_position();
    let hovered = bounds.contains(&mouse);
    if spec.hover_only && !hovered {
        return;
    }
    // A corner is at most half of a side that the grid shares with its surface, as GPUI rounds a
    // quad, so a pill or a short control keeps its arc at any size.
    let limit = match (spec.spans.width, spec.spans.height) {
        (true, true) => width.min(height) / 2.,
        (true, false) => width / 2.,
        (false, true) => height / 2.,
        (false, false) => f32::INFINITY,
    };
    let radii = spec.corner_radii.map(|radius| radius.min(limit));
    let scale = window.scale_factor();
    let snap = |value: f32| (value * scale).round() / scale;
    let columns = (width / cell).floor() as i32;
    let rows = (height / cell).floor() as i32;
    let grid_width = columns as f32 * cell;
    let grid_height = rows as f32 * cell;
    let grid = Grid {
        left: snap(f32::from(bounds.origin.x) + (width - grid_width) / 2.),
        top: snap(f32::from(bounds.origin.y) + (height - grid_height) / 2.),
        columns: columns as u32,
        rows: rows as u32,
        cell,
        dot: spec.dot,
    };
    let aspect = grid_width / grid_height;
    let pointer = hovered.then(|| {
        (
            (f32::from(mouse.x) - grid.left) / grid_width,
            (f32::from(mouse.y) - grid.top) / grid_height,
        )
    });
    let source = |column: i32, row: i32| -> Option<Hsla> {
        let intensity = spec.field.intensity(&Sample {
            u: (column as f32 + 0.5) / columns as f32,
            v: (row as f32 + 0.5) / rows as f32,
            column,
            row,
            rows,
            aspect,
            time: spec.time,
            pointer,
        });
        if intensity <= threshold(column as i64, row as i64) {
            return None;
        }
        Some(if spec.toned {
            spec.color.opacity(tone(intensity))
        } else {
            spec.color
        })
    };
    if spec.moving || !spec.field.cacheable() {
        paint_dots(
            window,
            cx,
            grid,
            (grid.columns, grid.rows),
            None,
            radii,
            &source,
        );
        return;
    }
    let mut content = digest(spec.color, &[]);
    spec.toned.hash(&mut content);
    spec.field.digest(&mut content);
    let tile = match spec.field.axis() {
        Some(Axis::Vertical) => (STRIP_CELLS, grid.rows),
        Some(Axis::Horizontal) if grid.rows > STRIP_CELLS => (grid.columns, STRIP_CELLS),
        _ => (grid.columns, grid.rows),
    };
    paint_dots(
        window,
        cx,
        grid,
        tile,
        Some(content.finish()),
        radii,
        &source,
    );
}

/// A dot-matrix surface. It fills its parent, so the parent sets the size.
#[derive(IntoElement, Clone)]
pub(super) struct Dither {
    field: DitherField,
    color: Hsla,
    cell: f32,
    dot: f32,
    toned: bool,
    animated: bool,
    hover_only: bool,
    /// In window pixels, at the interface scale.
    corner_radii: Corners<f32>,
    spans: Size<bool>,
}

impl Dither {
    /// Creates a surface for a field. The default grid has a 3-point pitch with 2-point dots.
    pub(super) fn new(field: DitherField) -> Self {
        Self {
            field,
            color: gpui::white(),
            cell: 3.,
            dot: 2.,
            toned: false,
            animated: false,
            hover_only: false,
            corner_radii: Corners::default(),
            spans: Size::default(),
        }
    }

    /// Sets the dot color.
    pub(super) fn color(mut self, color: Hsla) -> Self {
        self.color = color;
        self
    }

    /// Sets the grid pitch and the dot size, in logical pixels.
    pub(super) fn grid(mut self, cell: f32, dot: f32) -> Self {
        self.cell = cell;
        self.dot = dot;
        self
    }

    /// Varies the dot alpha in three steps with the field intensity.
    pub(super) fn toned(mut self) -> Self {
        self.toned = true;
        self
    }

    /// Advances the field on the stepped clock while the window is drawn and motion is allowed.
    pub(super) fn animated(mut self) -> Self {
        self.animated = true;
        self
    }

    /// Paints the dots only while the pointer is over the surface.
    pub(super) fn hover_only(mut self) -> Self {
        self.hover_only = true;
        self
    }

    /// Skips dots outside rounded corners of this radius, in points. The corners belong to the
    /// surface that the dots shade, so they follow the interface scale.
    pub(super) fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radii = Corners::all(radius * crate::theme::ui_scale());
        self
    }

    /// Skips dots outside rounded corners, with one radius in points for each corner.
    pub(super) fn corner_radii(mut self, radii: Corners<f32>) -> Self {
        let scale = crate::theme::ui_scale();
        self.corner_radii = radii.map(|radius| radius * scale);
        self
    }

    /// Skips dots outside the corners of the surface that the grid lies on, as
    /// [`kit::Layers`] measures them, in window pixels. `spans` names the sides that the grid
    /// shares with the surface, which bound each corner to half of their length.
    pub(super) fn fitted(mut self, corners: Corners<Pixels>, spans: Size<bool>) -> Self {
        self.corner_radii = corners.map(|radius| f32::from(*radius));
        self.spans = spans;
        self
    }

    /// Wraps the surface in an absolute layer that covers its parent.
    ///
    /// Add the layer as the first child so that content paints on top.
    pub(super) fn layer(self) -> Div {
        div().absolute().inset_0().child(self)
    }
}

impl kit::Fit for Dither {
    fn fit(self, corners: Corners<Pixels>) -> AnyElement {
        self.fitted(corners, Size::new(true, true))
            .layer()
            .into_any_element()
    }
}

impl RenderOnce for Dither {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let moving = self.animated && self.field.timed() && !cx.reduce_motion();
        let spec = PaintSpec {
            field: self.field,
            color: self.color,
            cell: self.cell,
            dot: self.dot,
            toned: self.toned,
            hover_only: self.hover_only,
            corner_radii: self.corner_radii,
            spans: self.spans,
            moving,
            time: if moving { clock_seconds() } else { 0. },
        };
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                paint_grid(bounds, &spec, window, cx);
                // The view stack is only reliable during paint, so the next step is scheduled
                // here rather than in `render`.
                if moving {
                    schedule_step(window, cx);
                }
            },
        )
        .size_full()
    }
}

/// Returns a 50% checker texture with cells of `logical` pixels, drawn in the shader.
pub(super) fn checker(color: Hsla, logical: f32, window: &Window) -> Background {
    checkerboard(color, logical * window.scale_factor())
}

/// A one-pixel dotted rule that fills the width or height it is given.
#[derive(IntoElement)]
pub(super) struct DottedRule {
    color: Hsla,
    vertical: bool,
}

impl DottedRule {
    /// Creates a horizontal dotted rule.
    pub(super) fn horizontal(color: Hsla) -> Self {
        Self {
            color,
            vertical: false,
        }
    }

    /// Creates a vertical dotted rule that fills its parent's height.
    pub(super) fn vertical(color: Hsla) -> Self {
        Self {
            color,
            vertical: true,
        }
    }
}

impl RenderOnce for DottedRule {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let rule = div().flex_shrink_0().bg(checker(self.color, 1., window));
        if self.vertical {
            rule.w(px(1.)).h_full()
        } else {
            rule.h(px(1.)).w_full()
        }
    }
}

/// One colored part of a [`DitherBar`].
#[derive(Debug, Clone, Copy)]
pub(super) struct BarSegment {
    pub weight: f32,
    pub color: Hsla,
}

/// A dot-matrix ratio bar, like a memory usage bar on a retro display.
#[derive(IntoElement)]
pub(super) struct DitherBar {
    segments: Rc<[BarSegment]>,
    empty: Hsla,
    total: Option<f32>,
}

impl DitherBar {
    /// Creates a bar from weighted segments. `empty` colors the dots no segment covers.
    pub(super) fn new(segments: impl Into<Rc<[BarSegment]>>, empty: Hsla) -> Self {
        Self {
            segments: segments.into(),
            empty,
            total: None,
        }
    }

    /// Creates a meter: one segment that fills `fraction` of the bar.
    pub(super) fn meter(fraction: f32, color: Hsla, empty: Hsla) -> Self {
        Self {
            segments: Rc::from(vec![BarSegment {
                weight: fraction.clamp(0., 1.),
                color,
            }]),
            empty,
            total: Some(1.),
        }
    }
}

impl RenderOnce for DitherBar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (segments, empty, total) = (self.segments, self.empty, self.total);
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| paint_bar(bounds, &segments, empty, total, window, cx),
        )
        .size_full()
    }
}

fn paint_bar(
    bounds: Bounds<Pixels>,
    segments: &[BarSegment],
    empty: Hsla,
    total: Option<f32>,
    window: &mut Window,
    cx: &mut App,
) {
    use std::hash::{Hash, Hasher};
    let (cell, dot) = (3., 2.);
    let columns = (f32::from(bounds.size.width) / cell).floor() as i32;
    let rows = (f32::from(bounds.size.height) / cell).floor() as i32;
    if columns <= 0 || rows <= 0 {
        return;
    }
    let scale = window.scale_factor();
    let total = total.unwrap_or_else(|| segments.iter().map(|s| s.weight.max(0.)).sum());
    let source = |column: i32, row: i32| -> Option<Hsla> {
        let u = (column as f32 + 0.5) / columns as f32;
        let mut color = empty;
        let mut fade = 0.18;
        if total > 0. {
            let mut start = 0.;
            for segment in segments {
                let share = segment.weight.max(0.) / total;
                if share > 0. && u <= start + share {
                    color = segment.color;
                    // Dots thin out toward the end of each segment so the seams read as
                    // dithered transitions.
                    let local = ((u - start) / share).clamp(0., 1.);
                    fade = 1. - (local - 0.75).max(0.) * 2.2;
                    break;
                }
                start += share;
            }
        }
        (fade > threshold(column as i64, row as i64)).then_some(color)
    };
    let mut content = digest(empty, &[total]);
    for segment in segments {
        segment.weight.to_bits().hash(&mut content);
        texel(segment.color).hash(&mut content);
    }
    let grid = Grid {
        left: (f32::from(bounds.origin.x) * scale).round() / scale,
        top: (f32::from(bounds.origin.y) * scale).round() / scale,
        columns: columns as u32,
        rows: rows as u32,
        cell,
        dot,
    };
    paint_dots(
        window,
        cx,
        grid,
        (grid.columns, grid.rows),
        Some(content.finish()),
        Corners::default(),
        &source,
    );
}

/// Sweeps a dot-matrix packet across a thin band at the top of its parent while an agent works.
///
/// `radii` are the parent's corners that the band touches, so that no dot shows outside them.
pub(super) fn scanner(color: Hsla, radii: Corners<f32>) -> Div {
    div().absolute().top_0().left_0().right_0().h(px(4.)).child(
        Dither::new(DitherField::Sweep {
            width: 0.16,
            speed: 0.42,
            floor: 0.07,
        })
        .color(color)
        .grid(2., 1.)
        .corner_radii(radii)
        .animated(),
    )
}

/// Height of the dots that shade the bottom edge of a surface.
const EDGE_SHADE: f32 = 16.;
/// Width of the grain at the leading edge of a selected row.
const SELECTION_GRAIN: f32 = 64.;

/// Shades the bottom edge of a rounded surface with dots that thin out upward, from `density`.
///
/// The band has a fixed height, so the dots stay in the padding of the surface and do not
/// spread behind its text on a tall surface. Add it to a relatively positioned surface with
/// [`kit::Layers::layer`].
pub(super) fn edge_shade(color: Hsla, density: f32) -> impl kit::Fit {
    move |corners: Corners<Pixels>| {
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(crate::theme::px(EDGE_SHADE))
            .max_h_full()
            .child(
                Dither::new(DitherField::Fade {
                    from: Edge::Bottom,
                    start: density,
                    end: 0.,
                })
                .color(color)
                .grid(2., 1.)
                .fitted(
                    Corners {
                        bottom_left: corners.bottom_left,
                        bottom_right: corners.bottom_right,
                        ..Corners::default()
                    },
                    Size::new(true, false),
                ),
            )
            .into_any_element()
    }
}

/// Shades a rounded row: grain at its leading edge when selected, a moving flow while active.
/// Add it with [`kit::Layers::layer`].
pub(super) fn row_wash(color: Hsla, selected: bool, active: bool) -> impl kit::Fit {
    move |corners: Corners<Pixels>| row_wash_layer(color, corners, selected, active)
}

// The layer of `row_wash` for a row whose corners inside its border are `corners`.
fn row_wash_layer(
    color: Hsla,
    corners: Corners<Pixels>,
    selected: bool,
    active: bool,
) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .when(selected, |layer| {
            // The grain has a fixed width, so it thins out before the text of a wide row.
            layer.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(crate::theme::px(SELECTION_GRAIN))
                    .max_w_full()
                    .child(
                        Dither::new(DitherField::Fade {
                            from: Edge::Left,
                            start: 0.62,
                            end: 0.,
                        })
                        .color(color.opacity(0.3))
                        .grid(2., 1.)
                        .fitted(
                            Corners {
                                top_left: corners.top_left,
                                bottom_left: corners.bottom_left,
                                ..Corners::default()
                            },
                            Size::new(false, true),
                        ),
                    ),
            )
        })
        .when(active, |layer| {
            // The flow fades out toward the left, so only the trailing part of the row holds
            // dots. A fixed width keeps the moving field small on wide rows.
            layer.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(crate::theme::px(132.))
                    .max_w_full()
                    .child(
                        Dither::new(
                            DitherField::Flow {
                                scale: 0.2,
                                speed: 1.3,
                                bias: -0.08,
                            }
                            .times(DitherField::Fade {
                                from: Edge::Right,
                                start: 0.95,
                                end: 0.,
                            }),
                        )
                        .color(color.opacity(0.5))
                        .grid(2., 1.)
                        .fitted(
                            Corners {
                                top_right: corners.top_right,
                                bottom_right: corners.bottom_right,
                                ..Corners::default()
                            },
                            Size::new(false, true),
                        )
                        .animated(),
                    ),
            )
        })
        .into_any_element()
}
