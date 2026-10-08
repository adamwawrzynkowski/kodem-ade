//! Ordered (Bayer) dithering: the workspace's shading, depth and working-state language.
//!
//! Patterns are anchored to the window's cell grid, so an animated density field changes which
//! cells are lit without the pattern itself crawling or flickering. One-point cells are texture
//! (shadows, scrims, fades, selection grain); two- and three-point cells are signal (work in
//! progress, the wordmark and the new-task field). Motion is reserved for signal.
use super::*;
use std::{cell::RefCell, f32::consts::TAU, sync::Arc};

const BAYER: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];
/// Cells per side of a cached texture tile; a multiple of the Bayer period.
const TILE_CELLS: u32 = 32;

/// Returns the threshold of a window-grid cell in the open interval (0, 1).
fn threshold(x: i64, y: i64) -> f32 {
    (BAYER[y.rem_euclid(8) as usize][x.rem_euclid(8) as usize] as f32 + 0.5) / 64.
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
    .h(px(height))
}

/// Draws a one-point dotted rule on alternating window pixels.
pub(super) fn rule(color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint_cells(window, bounds, 1., color, |_, _| 0.5),
    )
    .w_full()
    .h(px(1.))
}

/// Casts a dithered contact shadow below and right of its parent's rounded bounds.
///
/// Place it as the first child of a relatively positioned wrapper that has the same bounds as
/// the floating surface, so the surface paints above it.
pub(super) fn shadow(theme: Theme, radius: f32, depth: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint_shadow(window, bounds, radius, depth, theme.ink),
    )
    .absolute()
    .inset_0()
}

// Visits only the uncovered band around the surface, never its interior.
fn paint_shadow(window: &mut Window, surface: Bounds<Pixels>, radius: f32, depth: f32, ink: Hsla) {
    let (left, top) = (f32::from(surface.left()), f32::from(surface.top()));
    let (width, height) = (
        f32::from(surface.size.width),
        f32::from(surface.size.height),
    );
    let (offset_x, offset_y) = (depth * 0.4, depth);
    let spread = depth * 1.5;
    let region = Bounds::new(
        point(px(left + offset_x - spread), px(top + offset_y - spread)),
        size(px(width + spread * 2.), px(height + spread * 2.)),
    );
    let x0 = left.floor() as i64;
    let x1 = (left + width + offset_x + spread).ceil() as i64;
    let y0 = top.floor() as i64;
    let y1 = (top + height + offset_y + spread).ceil() as i64;
    let cast = |x: f32, y: f32| -> f32 {
        // Distance outside the offset rounded rectangle, in points.
        let (sx, sy) = (x - left - offset_x, y - top - offset_y);
        let qx = (sx - width / 2.).abs() - (width / 2. - radius);
        let qy = (sy - height / 2.).abs() - (height / 2. - radius);
        let outside = qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - radius;
        0.62 * (1. - smoothstep(0., spread, outside))
    };
    window.paint_layer(region, |window| {
        for y in y0..y1 {
            let cy = y as f32 + 0.5;
            let beside = cy < top + height - radius;
            let mut run: Option<i64> = None;
            let start = if beside {
                (left + width).floor() as i64 - 1
            } else {
                x0
            };
            for x in start..=x1 {
                let cx = x as f32 + 0.5;
                let lit = x < x1
                    && !inside_rounded(cx - left, cy - top, width, height, radius)
                    && cast(cx, cy) > threshold(x, y);
                match (lit, run) {
                    (true, None) => run = Some(x),
                    (false, Some(first)) => {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(px(first as f32), px(y as f32)),
                                size(px((x - first) as f32), px(1.)),
                            ),
                            ink,
                        ));
                        run = None;
                    }
                    _ => {}
                }
            }
        }
    });
}

/// Draws a 4×4 working glyph that fills and empties in Bayer order.
pub(super) fn pulse(id: impl Into<ElementId>, color: Hsla, reduce_motion: bool) -> AnyElement {
    let frame = move |level: f32| {
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                const ORDER: [[u8; 4]; 4] =
                    [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
                let pitch = f32::from(bounds.size.width.min(bounds.size.height)) / 4.;
                let dot = (pitch - 0.75).max(1.);
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
            },
        )
        .size(px(13.))
        .flex_shrink_0()
    };
    if reduce_motion {
        return frame(0.5).into_any_element();
    }
    div()
        .size(px(13.))
        .flex_shrink_0()
        .with_animation(
            id,
            Animation::new(Duration::from_millis(1600)).repeat(),
            move |d, phase| d.child(frame(0.5 - 0.5 * (phase * TAU).cos())),
        )
        .into_any_element()
}

/// Sweeps a dithered signal packet across a thin band while an agent works.
pub(super) fn scanner(id: impl Into<ElementId>, color: Hsla, reduce_motion: bool) -> AnyElement {
    let frame = move |phase: f32| {
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let width = f32::from(bounds.size.width).max(1.);
                let center = (phase * 1.5 - 0.25) * width;
                let reach = width * 0.16;
                paint_cells(window, bounds, 2., color, |x, _| {
                    let d = (x - center) / reach;
                    0.06 + 0.94 * (-d * d).exp()
                });
            },
        )
        .absolute()
        .inset_0()
    };
    let band = div().absolute().top_0().left_0().right_0().h(px(4.));
    if reduce_motion {
        return band.child(frame(0.5)).into_any_element();
    }
    band.with_animation(
        id,
        Animation::new(Duration::from_millis(2200)).repeat(),
        move |d, phase| d.child(frame(phase)),
    )
    .into_any_element()
}

/// Shades a rounded row with fine grain fading to the right, sweeping it while work runs.
pub(super) fn row_wash(
    id: impl Into<ElementId>,
    color: Hsla,
    radius: f32,
    selected: bool,
    active: bool,
    reduce_motion: bool,
) -> AnyElement {
    let frame = move |phase: Option<f32>| {
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let width = f32::from(bounds.size.width).max(1.);
                let height = f32::from(bounds.size.height);
                if selected {
                    paint_cells(window, bounds, 1., color.opacity(0.13), |x, y| {
                        if !inside_rounded(x, y, width, height, radius) {
                            return 0.;
                        }
                        let t = x / width;
                        0.55 * (1. - smoothstep(0., 0.72, t))
                    });
                }
                if let Some(phase) = phase {
                    let center = (phase * 1.6 - 0.3) * width;
                    paint_cells(window, bounds, 2., color.opacity(0.22), |x, y| {
                        if !inside_rounded(x, y, width, height, radius) {
                            return 0.;
                        }
                        let d = (x - center) / (width * 0.2);
                        0.7 * (-d * d).exp()
                    });
                }
            },
        )
        .absolute()
        .inset_0()
    };
    let layer = div().absolute().inset_0();
    if !active || reduce_motion {
        return layer.child(frame(None)).into_any_element();
    }
    layer
        .with_animation(
            id,
            Animation::new(Duration::from_millis(2600)).repeat(),
            move |d, phase| d.child(frame(Some(phase))),
        )
        .into_any_element()
}

/// Five-by-seven dot-matrix glyphs for the wordmark.
const WORDMARK: [[u8; 7]; 5] = [
    // K
    [
        0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
    ],
    // O
    [
        0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
    ],
    // D
    [
        0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
    ],
    // E
    [
        0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
    ],
    // M
    [
        0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
    ],
];
const WORDMARK_PITCH: f32 = 2.;

/// Renders the dot-matrix wordmark; a scan of accent runs through it while any task works.
pub(super) fn wordmark(theme: Theme, working: bool, reduce_motion: bool) -> AnyElement {
    let columns = WORDMARK.len() * 6 - 1;
    let frame = move |phase: Option<f32>| {
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let sweep = phase.map(|p| p * (columns as f32 + 16.) - 8.);
                window.paint_layer(bounds, |window| {
                    for (letter, rows) in WORDMARK.iter().enumerate() {
                        for (row, bits) in rows.iter().enumerate() {
                            for col in 0..5 {
                                if bits & (0b10000 >> col) == 0 {
                                    continue;
                                }
                                let x = letter * 6 + col;
                                let glow = sweep
                                    .map_or(0., |s| (1. - ((x as f32 - s).abs() / 4.)).max(0.));
                                let mut color = theme.text;
                                if glow > 0. {
                                    color = theme.accent;
                                    color.alpha = 0.55 + 0.45 * glow;
                                }
                                window.paint_quad(fill(
                                    Bounds::new(
                                        bounds.origin
                                            + point(
                                                px(x as f32 * WORDMARK_PITCH),
                                                px(row as f32 * WORDMARK_PITCH),
                                            ),
                                        size(px(WORDMARK_PITCH - 0.4), px(WORDMARK_PITCH - 0.4)),
                                    ),
                                    color,
                                ));
                            }
                        }
                    }
                });
            },
        )
        .w(px(columns as f32 * WORDMARK_PITCH))
        .h(px(7. * WORDMARK_PITCH))
        .flex_shrink_0()
    };
    if !working || reduce_motion {
        return frame(None).into_any_element();
    }
    div()
        .flex_shrink_0()
        .with_animation(
            "wordmark-scan",
            Animation::new(Duration::from_millis(2400)).repeat(),
            move |d, phase| d.child(frame(Some(phase))),
        )
        .into_any_element()
}

/// Paints a slow 1-bit plasma for the new-task screen, dissolving into the canvas at its edges.
pub(super) fn field(color: Hsla, reduce_motion: bool) -> AnyElement {
    let frame = move |phase: f32| {
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let width = f32::from(bounds.size.width).max(1.);
                let height = f32::from(bounds.size.height).max(1.);
                let a = phase * TAU;
                paint_cells(window, bounds, 3., color, |x, y| {
                    let (u, v) = (x / width, y / height);
                    let r = ((u - 0.5) * (u - 0.5) * 4. + (v - 0.5) * (v - 0.5)).sqrt();
                    let wave = (u * 9. + 2. * a).sin()
                        + 0.8 * (v * 5.5 - a).sin()
                        + 0.6 * ((u + v) * 6. + a).sin()
                        + 0.5 * (r * 14. - 3. * a).sin();
                    let level = (0.5 + wave / 5.8).clamp(0., 1.).powf(1.7);
                    let edge = smoothstep(0., 0.3, u)
                        * smoothstep(0., 0.3, 1. - u)
                        * smoothstep(0., 0.4, v)
                        * smoothstep(0., 0.4, 1. - v);
                    level * edge
                });
            },
        )
        .absolute()
        .inset_0()
    };
    let layer = div().absolute().inset_0();
    if reduce_motion {
        return layer.child(frame(0.)).into_any_element();
    }
    layer
        .with_animation(
            "new-task-field",
            Animation::new(Duration::from_secs(24))
                .with_easing(|p| p)
                .repeat(),
            move |d, phase| d.child(frame(phase)),
        )
        .into_any_element()
}

/// Reveals a floating surface like a CRT unrolling, led by a dithered scan line.
pub(super) fn unroll(
    id: impl Into<ElementId>,
    element: impl IntoElement,
    theme: Theme,
    upward: bool,
) -> AnyElement {
    Unroll {
        child: Some(element.into_any_element()),
        progress: 0.,
        upward,
        beam: theme.accent,
    }
    .with_animation(
        id,
        Animation::new(Duration::from_millis(190)).with_easing(|p| 1. - (1. - p).powi(3)),
        |mut unroll, progress| {
            unroll.progress = progress;
            unroll
        },
    )
    .into_any_element()
}

struct Unroll {
    child: Option<AnyElement>,
    progress: f32,
    upward: bool,
    beam: Hsla,
}
impl IntoElement for Unroll {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Unroll {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let child = self.child.as_mut().expect("unroll child is laid out once");
        (child.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = &mut self.child {
            child.prepaint(window, cx);
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(child) = &mut self.child else {
            return;
        };
        if self.progress >= 1. {
            child.paint(window, cx);
            return;
        }
        // Extend the clip past the surface so its dithered shadow unrolls with it.
        let margin = px(24.);
        let reach = (bounds.size.height + margin) * self.progress;
        let visible = if self.upward {
            Bounds::from_corners(
                point(bounds.left() - margin, bounds.bottom() + margin - reach),
                point(bounds.right() + margin, bounds.bottom() + margin),
            )
        } else {
            Bounds::from_corners(
                point(bounds.left() - margin, bounds.top() - margin),
                point(bounds.right() + margin, bounds.top() + reach),
            )
        };
        window.with_content_mask(Some(ContentMask { bounds: visible }), |window| {
            child.paint(window, cx);
        });
        // Keep the beam on the surface while the clip edge travels through the shadow margin.
        let edge = if self.upward {
            visible.top()
        } else {
            visible.bottom() - px(3.)
        }
        .clamp(bounds.top(), (bounds.bottom() - px(3.)).max(bounds.top()));
        let beam = Bounds::new(point(bounds.left(), edge), size(bounds.size.width, px(3.)));
        let strength = 1. - self.progress;
        paint_cells(
            window,
            beam,
            1.,
            self.beam.opacity(0.9 * strength),
            |_, y| {
                if y < 1.5 { 0.85 } else { 0.4 }
            },
        );
    }
}
