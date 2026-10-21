//! The dot screen of the chat background: an ordered-dither halftone over the top of the window.
//!
//! Square dots on a three-point pitch light in Bayer order where the screen is dense, so smooth
//! values turn into the regular patterns of print halftones. The screen is densest at the top of
//! the window and thins out toward its middle, and slow noise leaves open patches in it. Without
//! a wallpaper the dots take the theme's dot color. With one, each dot takes a shade of the
//! wallpaper's color beneath it, so the screen reads as a halftone of the image.
//!
//! The screen does not move. It is baked once into a sprite at device resolution, away from the
//! window, and baked again when the window's size, the theme or the wallpaper changes. While a
//! bake runs, the last one keeps showing; a resize therefore bakes only as often as bakes end.
use super::*;
use image::RgbaImage;

/// Pitch of the screen and size of its dots, in points.
const CELL: f32 = 3.;
const DOT: f32 = 1.5;
/// Density of the screen at the top of the window, from 0 to 1.
const DENSITY: f32 = 0.65;
/// Share of the window's height, from the top, over which the screen thins out to nothing.
const REACH: f32 = 0.7;
/// Size of the open patches in the screen, in points, and the seed of their noise.
const PATCH: (f32, f32) = (240., 170.);
const SEED: u32 = 0x5C4E;
/// Longest side of the wallpaper's sample that colors the dots, in pixels.
const SAMPLE: u32 = 480;
/// Share of the wallpaper's color that a dot keeps, and the alpha of that dot.
const SHADE: f32 = 0.7;
const SHADE_ALPHA: f32 = 0.9;

// What one bake shows: the window's size in device pixels, its scale, the theme's dot color and
// the wallpaper.
#[derive(Clone, PartialEq)]
struct Key {
    width: u32,
    height: u32,
    scale: f32,
    color: Hsla,
    wallpaper: Option<PathBuf>,
}

/// The dot screen of one window: its last bake and the wallpaper's sample that colors it.
#[derive(Default)]
pub(super) struct Screen {
    baked: Option<(Key, Arc<RenderImage>)>,
    baking: bool,
    /// The wallpaper's colors at a small size, by its path. The sample is absent when the
    /// wallpaper cannot be read.
    sample: Option<(PathBuf, Option<Arc<RgbaImage>>)>,
    sampling: bool,
}

impl Backdrop {
    /// Returns the dot screen's sprite for this frame, and starts a bake when what the sprite
    /// shows is out of date. Returns nothing while the screen is off.
    pub(super) fn screen(
        &mut self,
        on: bool,
        wallpaper: Option<&PathBuf>,
        color: Hsla,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        if !on {
            if let Some((_, sprite)) = self.screen.baked.take() {
                cx.drop_image(sprite, Some(window));
            }
            return None;
        }
        let scale = window.scale_factor();
        let viewport = window.viewport_size();
        let key = Key {
            width: (f32::from(viewport.width) * scale).round() as u32,
            height: (f32::from(viewport.height) * scale).round() as u32,
            scale,
            color,
            wallpaper: wallpaper.cloned(),
        };
        let current = self
            .screen
            .baked
            .as_ref()
            .is_some_and(|(baked, _)| *baked == key);
        if !current && !self.screen.baking {
            // A bake with a wallpaper waits for its sample.
            let sample = match wallpaper {
                Some(path) => self.wallpaper_sample(path, cx),
                None => Some(None),
            };
            if let Some(sample) = sample {
                self.screen.baking = true;
                cx.spawn(async move |this, cx| {
                    let baked = key.clone();
                    let sprite = cx
                        .background_executor()
                        .spawn(async move { bake(&baked, sample.as_deref()) })
                        .await;
                    let _ = this.update(cx, |backdrop, cx| {
                        backdrop.screen.baking = false;
                        if let Some((_, old)) = backdrop.screen.baked.replace((key, sprite)) {
                            cx.drop_image(old, None);
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        self.screen.baked.as_ref().map(|(_, sprite)| sprite.clone())
    }

    // Returns the sample of the wallpaper at `path`, which is absent when the wallpaper cannot be
    // read. Returns nothing while the sample loads, and starts loading it.
    fn wallpaper_sample(
        &mut self,
        path: &PathBuf,
        cx: &mut Context<Self>,
    ) -> Option<Option<Arc<RgbaImage>>> {
        if let Some((sampled, sample)) = &self.screen.sample
            && sampled == path
        {
            return Some(sample.clone());
        }
        if !self.screen.sampling {
            self.screen.sampling = true;
            let path = path.clone();
            cx.spawn(async move |this, cx| {
                let source = path.clone();
                let sample = cx
                    .background_executor()
                    .spawn(async move {
                        image::open(&source)
                            .ok()
                            .map(|image| Arc::new(image.thumbnail(SAMPLE, SAMPLE).to_rgba8()))
                    })
                    .await;
                let _ = this.update(cx, |backdrop, cx| {
                    backdrop.screen.sampling = false;
                    backdrop.screen.sample = Some((path, sample));
                    cx.notify();
                });
            })
            .detach();
        }
        None
    }
}

/// Paints the screen's sprite at the top-left corner of the window, clipped to `bounds`.
pub(super) fn paint(sprite: &Arc<RenderImage>, bounds: Bounds<Pixels>, window: &mut Window) {
    let scale = window.scale_factor();
    let size = sprite.size(0);
    let region = Bounds::new(
        point(px(0.), px(0.)),
        gpui::size(
            px(size.width.0 as f32 / scale),
            px(size.height.0 as f32 / scale),
        ),
    );
    let _ = window.paint_image(bounds, region, Corners::default(), sprite.clone(), 0, false);
}

// Where the wallpaper's sample lies under the window: scaled to cover it and centered, as the
// wallpaper is drawn.
struct Cover<'a> {
    sample: &'a RgbaImage,
    scale: f32,
    offset: (f32, f32),
}

impl<'a> Cover<'a> {
    fn new(sample: &'a RgbaImage, width: f32, height: f32) -> Self {
        let (sample_width, sample_height) = (sample.width() as f32, sample.height() as f32);
        let scale = (width / sample_width).max(height / sample_height);
        Self {
            sample,
            scale,
            offset: (
                (width - sample_width * scale) / 2.,
                (height - sample_height * scale) / 2.,
            ),
        }
    }

    // Returns the shade of the wallpaper's color at a point of the window, in points.
    fn shade(&self, x: f32, y: f32) -> Rgba {
        let column = ((x - self.offset.0) / self.scale).clamp(0., (self.sample.width() - 1) as f32);
        let row = ((y - self.offset.1) / self.scale).clamp(0., (self.sample.height() - 1) as f32);
        let [red, green, blue, _] = self.sample.get_pixel(column as u32, row as u32).0;
        let channel = |value: u8| value as f32 / 255. * SHADE;
        Rgba::new(channel(red), channel(green), channel(blue), SHADE_ALPHA)
    }
}

// Bakes the screen of `key` into straight-alpha BGRA, the layout of GPUI's sprite atlas. The
// rows below the screen's reach stay out of the sprite.
fn bake(key: &Key, sample: Option<&RgbaImage>) -> Arc<RenderImage> {
    let cell = (CELL * key.scale).round().max(1.) as u32;
    let dot = ((DOT * key.scale).round() as u32).clamp(1, cell);
    let inset = (cell - dot) / 2;
    let columns = key.width.div_ceil(cell).max(1);
    let rows = ((key.height as f32 * REACH) / cell as f32).ceil().max(1.) as u32;
    let width = (columns * cell) as usize;
    let mut data = vec![0u8; width * (rows * cell) as usize * 4];
    // The field is laid out in points, so it keeps its shape at every display scale.
    let pitch = cell as f32 / key.scale;
    let (window_width, window_height) =
        (key.width as f32 / key.scale, key.height as f32 / key.scale);
    let reach = window_height * REACH;
    let cover = sample.map(|sample| Cover::new(sample, window_width, window_height));
    let neutral = hsla_to_rgba(key.color);
    for row in 0..rows {
        let y = (row as f32 + 0.5) * pitch;
        let fade = 1. - smoothstep(0., reach, y);
        for column in 0..columns {
            let x = (column as f32 + 0.5) * pitch;
            let open = smoothstep(
                0.25,
                0.55,
                dither::fractal_noise(x / PATCH.0, y / PATCH.1, SEED),
            );
            if DENSITY * fade * open <= crate::bayer::threshold(column as i64, row as i64) {
                continue;
            }
            let color = cover.as_ref().map_or(neutral, |cover| cover.shade(x, y));
            let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
            let texel = [
                channel(color.blue),
                channel(color.green),
                channel(color.red),
                channel(color.alpha),
            ];
            for line in 0..dot {
                let start = (((row * cell + inset + line) as usize) * width
                    + (column * cell + inset) as usize)
                    * 4;
                for target in data[start..start + dot as usize * 4].chunks_exact_mut(4) {
                    target.copy_from_slice(&texel);
                }
            }
        }
    }
    let buffer = RgbaImage::from_raw(columns * cell, rows * cell, data)
        .expect("the screen's buffer matches its dimensions");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}
