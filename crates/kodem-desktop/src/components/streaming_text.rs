//! Bounded arrival times for the newest graphemes in a streamed text block.
use gpui::*;
use std::{
    collections::VecDeque,
    ops::Range,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

const SETTLE: Duration = Duration::from_millis(280);
const MAX_LETTERS: usize = 96;

#[derive(Default)]
pub(super) struct StreamingText {
    letters: VecDeque<(Range<usize>, Instant)>,
}

pub(super) struct RevealingGlyph {
    pub origin: Point<Pixels>,
    pub font: FontId,
    pub id: GlyphId,
    pub font_size: Pixels,
    pub color: Hsla,
    pub progress: f32,
}

/// Shared blur curve for arriving text, composer context and sent messages.
pub(crate) fn reveal_blur(progress: f32) -> Pixels {
    px(2.4 * (1. - progress).powi(2))
}

/// Shared opacity curve for progressive content reveals.
pub(crate) fn reveal_opacity(progress: f32) -> f32 {
    1. - (1. - progress).powi(3)
}

/// Paints only arriving glyphs, sharing at most eight blur layers per text line.
pub(super) fn paint_glyphs(glyphs: Vec<RevealingGlyph>, window: &mut Window) {
    let mut groups: [Vec<RevealingGlyph>; 8] = Default::default();
    for glyph in glyphs {
        if glyph.progress > 0. {
            groups[((glyph.progress * 8.) as usize).min(7)].push(glyph);
        }
    }
    for group in groups.iter().filter(|group| !group.is_empty()) {
        let mut bounds = Bounds::default();
        let mut progress = 0.;
        for (index, glyph) in group.iter().enumerate() {
            let region = Bounds::new(
                glyph.origin - point(glyph.font_size, glyph.font_size * 1.5),
                size(glyph.font_size * 3., glyph.font_size * 3.),
            );
            bounds = if index == 0 {
                region
            } else {
                bounds.union(&region)
            };
            progress += glyph.progress;
        }
        if !bounds.intersects(&window.content_mask().bounds) {
            continue;
        }
        let blur = reveal_blur(progress / group.len() as f32);
        window.with_filter_layer(
            bounds,
            Corners::default(),
            &[Filter::Blur(blur)],
            |window| {
                for glyph in group {
                    let p = glyph.progress;
                    let gleam = (std::f32::consts::PI * p).sin().powi(2) * 0.35;
                    let mut color = glyph.color;
                    let target = if color.lightness > 0.5 { 1. } else { 0. };
                    color.lightness += (target - color.lightness) * gleam;
                    color.alpha *= reveal_opacity(p);
                    let _ = window.paint_glyph(
                        glyph.origin,
                        glyph.font,
                        glyph.id,
                        glyph.font_size,
                        color,
                    );
                }
            },
        );
    }
}

impl StreamingText {
    /// Records appended graphemes, preserving arrival times across subsequent chunks.
    pub(super) fn append(&mut self, old: &str, new: &str, now: Instant) {
        self.prune(now);
        if !new.starts_with(old) {
            self.letters.clear();
            return;
        }
        // Segment the whole string so a combining mark or emoji continuation never
        // starts its own animation in the middle of an existing grapheme.
        let tail = new
            .grapheme_indices(true)
            .rev()
            .take_while(|(start, letter)| start + letter.len() > old.len())
            .take(MAX_LETTERS)
            .collect::<Vec<_>>();
        let first = tail.last().map_or(new.len(), |(start, _)| *start);
        self.letters.retain(|(range, _)| range.end <= first);
        let start = self
            .letters
            .back()
            .map_or(now, |(_, time)| (*time).max(now));
        let step = Duration::from_secs_f32(0.008_f32.min(0.12 / tail.len().max(1) as f32));
        for (index, (offset, letter)) in tail.into_iter().rev().enumerate() {
            let time = (start + step * index as u32).min(now + Duration::from_millis(120));
            self.letters
                .push_back((offset..offset + letter.len(), time));
        }
        while self.letters.len() > MAX_LETTERS {
            self.letters.pop_front();
        }
    }

    /// Tracks an arbitrary composer edit, shifting unchanged ranges and revealing inserted graphemes.
    pub(super) fn replace(
        &mut self,
        new: &str,
        replaced: Range<usize>,
        inserted: usize,
        now: Instant,
    ) {
        self.prune(now);
        let delta = inserted as isize - replaced.len() as isize;
        self.letters.retain_mut(|(range, _)| {
            if range.end <= replaced.start {
                return true;
            }
            if range.start < replaced.end {
                return false;
            }
            range.start = range.start.saturating_add_signed(delta);
            range.end = range.end.saturating_add_signed(delta);
            true
        });
        let end = replaced.start + inserted;
        let letters: Vec<_> = new
            .grapheme_indices(true)
            .filter(|(start, letter)| *start < end && start + letter.len() > replaced.start)
            .take(MAX_LETTERS)
            .collect();
        let step = Duration::from_secs_f32(0.008_f32.min(0.12 / letters.len().max(1) as f32));
        for (index, (start, letter)) in letters.into_iter().enumerate() {
            let range = start..start + letter.len();
            self.letters
                .retain(|(old, _)| old.end <= range.start || old.start >= range.end);
            self.letters.push_back((range, now + step * index as u32));
        }
        self.letters
            .make_contiguous()
            .sort_by_key(|(range, _)| range.start);
        while self.letters.len() > MAX_LETTERS {
            self.letters.pop_front();
        }
    }

    /// Returns the byte spans painted by the reveal overlay, including edits in the middle of text.
    pub(super) fn ranges(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.letters.iter().map(|(range, _)| range.clone())
    }

    /// Drops settled letters so idle text no longer requests animation frames.
    pub(super) fn prune(&mut self, now: Instant) {
        self.letters
            .retain(|(_, time)| now.saturating_duration_since(*time) < SETTLE);
    }

    /// Returns the first byte still painted by the animated overlay.
    pub(super) fn start(&self) -> Option<usize> {
        self.letters.front().map(|(range, _)| range.start)
    }

    /// Returns a letter's reveal progress without dividing multibyte graphemes.
    pub(super) fn progress(&self, byte: usize, now: Instant) -> Option<f32> {
        self.letters
            .iter()
            .find(|(range, _)| range.contains(&byte))
            .map(|(_, time)| {
                (now.saturating_duration_since(*time).as_secs_f32() / SETTLE.as_secs_f32()).min(1.)
            })
    }
}
