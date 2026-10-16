// Added by Kodem in October 2026 to gpui_ce_macos 0.1.0.

//! Blurs that work on the region around each blurred shape.
//!
//! A blur downsamples its backdrop to half resolution, blurs it with a separable gaussian and
//! composites it into a rounded rectangle. Each pass covers only the half-res texels that the
//! composite reads, grown by the kernel's reach, so a blur costs in proportion to its shape
//! instead of to the window. The values in that region are the ones that whole-window passes
//! would produce there.
//!
//! Consecutive blurs of one batch share the passes and one break of the render pass when none of
//! them reads pixels that an earlier one composites, and when they share a kernel: their regions
//! may overlap, and the same kernel over the same backdrop writes the same values there.

use gpui::{Bounds, Corners, ScaledPixels};
use std::f32::consts::PI;

/// The most taps on each side of a gaussian's center.
pub(crate) const MAX_TAPS: usize = 32;

/// A gaussian at half resolution: its sigma, its taps on each side of the center and the
/// spacing between taps, in half-res texels.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct BlurKernel {
    pub sigma: f32,
    pub tap_count: f32,
    pub tap_step: f32,
}

impl BlurKernel {
    /// Returns the kernel of a blur radius in device pixels, or `None` when it blurs nothing.
    ///
    /// Sigma is halved because the blur runs at half resolution. The kernel spans ±3σ; when that
    /// needs more than [`MAX_TAPS`] taps, the taps spread apart rather than truncate the gaussian.
    pub fn new(blur_radius: f32) -> Option<Self> {
        let sigma = (blur_radius * 0.5).max(0.0);
        if sigma <= 0.0 {
            return None;
        }
        let ideal_taps = (3.0 * sigma).ceil();
        let tap_count = ideal_taps.clamp(1.0, MAX_TAPS as f32);
        let tap_step = (ideal_taps / tap_count).max(1.0);
        Some(Self {
            sigma,
            tap_count,
            tap_step,
        })
    }

    /// Returns how far, in half-res texels, the taps of a texel sample from it. A tap between two
    /// texels also reads the farther one.
    pub fn reach(&self) -> i32 {
        (self.tap_count * self.tap_step).ceil() as i32 + 1
    }

    /// Returns the weights of the taps from the center outwards, normalized over both sides.
    pub fn weights(&self) -> [f32; MAX_TAPS + 1] {
        let taps = self.tap_count as usize;
        let mut weights = [0.0; MAX_TAPS + 1];
        let mut sum = 0.0;
        for (index, weight) in weights.iter_mut().enumerate().take(taps + 1) {
            let x = index as f32 * self.tap_step;
            *weight = (-(x * x) / (2.0 * self.sigma * self.sigma)).exp()
                / ((2.0 * PI).sqrt() * self.sigma);
            sum += if index == 0 { *weight } else { 2.0 * *weight };
        }
        let sum: f32 = sum.max(1e-5);
        for weight in &mut weights[..=taps] {
            *weight /= sum;
        }
        weights
    }
}

/// A rectangle of texels: columns `x0..x1` and rows `y0..y1`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct TexelRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl TexelRect {
    /// Returns the rectangle grown by `dx` columns and `dy` rows on each side.
    pub fn grow(self, dx: i32, dy: i32) -> Self {
        Self {
            x0: self.x0 - dx,
            y0: self.y0 - dy,
            x1: self.x1 + dx,
            y1: self.y1 + dy,
        }
    }

    /// Returns the part of the rectangle within a texture of `width` × `height` texels.
    pub fn within(self, width: i32, height: i32) -> Self {
        Self {
            x0: self.x0.clamp(0, width),
            y0: self.y0.clamp(0, height),
            x1: self.x1.clamp(0, width),
            y1: self.y1.clamp(0, height),
        }
    }

    /// Reports whether the rectangle holds no texel.
    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    /// Reports whether the two rectangles share a texel.
    pub fn intersects(&self, other: &Self) -> bool {
        self.x0 < other.x1 && other.x0 < self.x1 && self.y0 < other.y1 && other.y0 < self.y1
    }

    /// Returns the number of columns and rows.
    pub fn size(&self) -> [i32; 2] {
        [self.x1 - self.x0, self.y1 - self.y0]
    }
}

/// One blur: the regions of its passes and how it composites.
pub(crate) struct BlurJob {
    pub kernel: BlurKernel,
    /// The rounded rectangle that the composite draws, in device pixels.
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: Bounds<ScaledPixels>,
    pub corner_radii: Corners<ScaledPixels>,
    pub opacity: f32,
    pub clip_rounded: bool,
    /// The full-res pixels that the composite covers.
    pub composite: TexelRect,
    /// The half-res texels that the downsample, the horizontal pass and the vertical pass write.
    pub downsampled: TexelRect,
    pub horizontal: TexelRect,
    pub vertical: TexelRect,
}

impl BlurJob {
    /// Returns the blur of `bounds` with `blur_radius` in device pixels, for a viewport of
    /// `viewport` pixels and half-res textures of `half` texels, or `None` when it shows nothing.
    ///
    /// A backdrop (`clip_rounded`) composites within its rounded rectangle. A content blur bleeds
    /// about three radii past its bounds, so its composite covers a dilated rectangle.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        bounds: Bounds<ScaledPixels>,
        content_mask: Bounds<ScaledPixels>,
        corner_radii: Corners<ScaledPixels>,
        blur_radius: f32,
        opacity: f32,
        clip_rounded: bool,
        viewport: [i32; 2],
        half: [i32; 2],
    ) -> Option<Self> {
        let kernel = BlurKernel::new(blur_radius)?;
        let bounds = if clip_rounded {
            bounds
        } else {
            bounds.dilate(ScaledPixels(3.0 * blur_radius))
        };
        let area = bounds.intersect(&content_mask);
        let composite = TexelRect {
            x0: area.origin.x.0.floor() as i32,
            y0: area.origin.y.0.floor() as i32,
            x1: (area.origin.x.0 + area.size.width.0).ceil() as i32,
            y1: (area.origin.y.0 + area.size.height.0).ceil() as i32,
        }
        .within(viewport[0], viewport[1]);
        if composite.is_empty() {
            return None;
        }
        // A pixel samples the half-res texture between the two texels nearest to half its
        // position, so the composite reads one texel beyond each side of the halved rectangle.
        let reach = kernel.reach();
        let vertical = TexelRect {
            x0: composite.x0.div_euclid(2) - 1,
            y0: composite.y0.div_euclid(2) - 1,
            x1: (composite.x1 + 1).div_euclid(2) + 1,
            y1: (composite.y1 + 1).div_euclid(2) + 1,
        }
        .within(half[0], half[1]);
        let horizontal = vertical.grow(0, reach).within(half[0], half[1]);
        let downsampled = horizontal.grow(reach, 0).within(half[0], half[1]);
        Some(Self {
            kernel,
            bounds,
            content_mask,
            corner_radii,
            opacity,
            clip_rounded,
            composite,
            downsampled,
            horizontal,
            vertical,
        })
    }

    /// Returns the full-res pixels that the downsample reads: each half-res texel averages a 2×2
    /// block.
    pub fn source(&self) -> TexelRect {
        TexelRect {
            x0: self.downsampled.x0 * 2,
            y0: self.downsampled.y0 * 2,
            x1: self.downsampled.x1 * 2,
            y1: self.downsampled.y1 * 2,
        }
    }

    /// Reports whether this blur can share the passes of `earlier`, which come before it in the
    /// same batch: the same kernel, and a backdrop that `earlier` does not composite into.
    pub fn joins(&self, earlier: &[BlurJob]) -> bool {
        let source = self.source();
        earlier
            .iter()
            .all(|job| job.kernel == self.kernel && !job.composite.intersects(&source))
    }
}
