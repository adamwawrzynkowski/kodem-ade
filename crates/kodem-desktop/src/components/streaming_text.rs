//! Bounded arrival times for the newest graphemes in a streamed text block.
use gpui::*;
use std::{
    collections::VecDeque,
    ops::Range,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

/// How long a streamed letter takes to form, and a typed one. Typing must feel immediate, so its
/// letters form in less than half the time.
const SETTLE: Duration = Duration::from_millis(260);
const TYPED_SETTLE: Duration = Duration::from_millis(110);
/// Most letters that form at the same time.
const MAX_LETTERS: usize = 96;
/// Most streamed letters that wait for their turn. A block that arrives whole forms from its
/// start to its end in a sweep of this many letters; older letters are there at once.
const MAX_QUEUED: usize = MAX_LETTERS * 6;
/// Longest time by which the last letter of a small chunk starts after the chunk arrived.
const LEAD: Duration = Duration::from_millis(120);

pub(super) struct StreamingText {
    letters: VecDeque<(Range<usize>, Instant)>,
    settle: Duration,
}

impl Default for StreamingText {
    fn default() -> Self {
        Self {
            letters: VecDeque::new(),
            settle: SETTLE,
        }
    }
}

pub(super) struct RevealingGlyph {
    pub origin: Point<Pixels>,
    pub font: FontId,
    pub id: GlyphId,
    pub font_size: Pixels,
    pub color: Hsla,
    pub progress: f32,
}
impl RevealingGlyph {
    fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(
            self.origin - point(self.font_size, self.font_size * 1.5),
            size(self.font_size * 3., self.font_size * 3.),
        )
    }
}

/// Shared stepped opacity curve for progressive content reveals.
///
/// Four discrete levels give arriving text and context a palette-limited, phosphor-like fade.
pub(crate) fn reveal_opacity(progress: f32) -> f32 {
    ((1. - (1. - progress).powi(3)) * 4.).ceil() / 4.
}

/// Share of a letter that is there at once when it starts to form. A typed letter must show
/// under the key press; a streamed one may start from almost nothing.
const TYPED_START: f32 = 0.34;
const STREAMED_START: f32 = 0.1;
/// Highest density of the loose dots around a forming letter.
const SPARK: f32 = 0.2;

/// Paints arriving glyphs as they form out of dither.
///
/// A glyph is drawn only inside the grid cells whose Bayer threshold its progress has passed,
/// so its pixels appear in dither order until the whole letter is there. Loose dots flicker in
/// the letter's cell while it forms and are gone when it has settled. GPUI has no custom
/// shaders, so the mask is a set of clip rectangles; lit cells of a row merge into runs, and
/// everything shares one layer.
///
/// `typed` selects the finer grid and the faster start of the prompt field.
pub(super) fn paint_glyphs(glyphs: Vec<RevealingGlyph>, typed: bool, window: &mut Window) {
    if glyphs.is_empty() {
        return;
    }
    let visible = window.content_mask().bounds;
    let scale = window.scale_factor();
    // Cells are whole device pixels: one point for typing, about one and a half for the many
    // letters of a streamed reply.
    let cell = if typed {
        scale.round().max(1.) / scale
    } else {
        (scale * 1.5).ceil() / scale
    };
    let start = if typed { TYPED_START } else { STREAMED_START };
    window.paint_layer(visible, |window| {
        for glyph in glyphs {
            // A typed letter at progress zero already shows its first share of dots.
            if glyph.progress < 0. || !glyph.bounds().intersects(&visible) {
                continue;
            }
            let progress = glyph.progress.min(1.);
            let eased = progress * progress * (3. - 2. * progress);
            let density = start + (1. - start) * eased;
            // The letter starts a little brighter than its final color and cools as it forms.
            let mut color = glyph.color;
            let target = if color.lightness > 0.5 { 1. } else { 0. };
            color.lightness += (target - color.lightness) * (1. - eased) * 0.4;
            let alpha = color.alpha;
            color.alpha = alpha * (0.55 + 0.45 * eased);
            if density >= 0.985 {
                let _ =
                    window.paint_glyph(glyph.origin, glyph.font, glyph.id, glyph.font_size, color);
                continue;
            }
            // The cell box of the letter, on a grid fixed to the window.
            let size = f32::from(glyph.font_size);
            let (x, y) = (f32::from(glyph.origin.x), f32::from(glyph.origin.y));
            let columns =
                ((x - size * 0.12) / cell).floor() as i64..((x + size * 1.1) / cell).ceil() as i64;
            let rows =
                ((y - size * 1.05) / cell).floor() as i64..((y + size * 0.36) / cell).ceil() as i64;
            let spark = SPARK * (std::f32::consts::PI * progress).sin().powi(2);
            let mut dot = glyph.color;
            dot.alpha = alpha * 0.5;
            for row in rows {
                let top = px(row as f32 * cell);
                let mut run: Option<i64> = None;
                for column in columns.start..=columns.end {
                    let level = crate::bayer::threshold(column, row);
                    let lit = column < columns.end && density > level;
                    match (lit, run) {
                        (true, None) => run = Some(column),
                        (false, Some(first)) => {
                            let mask = Bounds::new(
                                point(px(first as f32 * cell), top),
                                gpui::size(px((column - first) as f32 * cell), px(cell)),
                            );
                            window.with_content_mask(
                                Some(ContentMask { bounds: mask }),
                                |window| {
                                    let _ = window.paint_glyph(
                                        glyph.origin,
                                        glyph.font,
                                        glyph.id,
                                        glyph.font_size,
                                        color,
                                    );
                                },
                            );
                            run = None;
                        }
                        _ => {}
                    }
                    // A loose dot sits in a cell the letter has not reached yet. Its pattern
                    // is the letter's shifted, so the two do not light the same cells.
                    if !lit
                        && column < columns.end
                        && spark > crate::bayer::threshold(column + 3, row + 5)
                    {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(px(column as f32 * cell), top),
                                gpui::size(px(cell), px(cell)),
                            ),
                            dot,
                        ));
                    }
                }
            }
        }
    });
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
            .take(MAX_QUEUED)
            .collect::<Vec<_>>();
        let first = tail.last().map_or(new.len(), |(start, _)| *start);
        self.letters.retain(|(range, _)| range.end <= first);
        if tail.is_empty() {
            return;
        }
        let count = tail.len() as u32;
        // A large chunk takes longer than a small one, so that its letters start one after
        // another and no more than MAX_LETTERS of them form at the same time.
        let lead = (SETTLE / MAX_LETTERS as u32 * count).max(LEAD);
        let step = Duration::from_millis(8).min(lead / count);
        let mut start = self
            .letters
            .back()
            .map_or(now, |(_, time)| (*time + step).max(now));
        // Letters that still wait must not hold the new ones back: when the queue is longer
        // than the lead, every letter moves earlier and the oldest ones are there at once.
        let late = (start + step * (count - 1)).saturating_duration_since(now + lead);
        if !late.is_zero() {
            for (_, time) in &mut self.letters {
                *time = time.checked_sub(late).unwrap_or(*time);
            }
            start = start.checked_sub(late).unwrap_or(start);
        }
        for (index, (offset, letter)) in tail.into_iter().rev().enumerate() {
            self.letters
                .push_back((offset..offset + letter.len(), start + step * index as u32));
        }
        while self.letters.len() > MAX_QUEUED {
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
        self.settle = TYPED_SETTLE;
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
        let step = Duration::from_secs_f32(0.002_f32.min(0.03 / letters.len().max(1) as f32));
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

    /// Returns the byte spans painted by the reveal overlay, including edits in the middle of
    /// text. Letters that touch share one span.
    pub(super) fn ranges(&self) -> Vec<Range<usize>> {
        let mut spans: Vec<Range<usize>> = Vec::new();
        for (range, _) in &self.letters {
            match spans.last_mut() {
                Some(last) if last.end == range.start => last.end = range.end,
                _ => spans.push(range.clone()),
            }
        }
        spans
    }

    /// Drops settled letters so idle text no longer requests animation frames.
    pub(super) fn prune(&mut self, now: Instant) {
        self.letters
            .retain(|(_, time)| now.saturating_duration_since(*time) < self.settle);
    }

    /// Reports letters that the user typed, which form faster than streamed ones.
    pub(super) fn typed(&self) -> bool {
        self.settle < SETTLE
    }

    /// Returns the first byte still painted by the animated overlay.
    pub(super) fn start(&self) -> Option<usize> {
        self.letters.front().map(|(range, _)| range.start)
    }

    /// Returns a letter's reveal progress without dividing multibyte graphemes. A letter whose
    /// turn has not come yet reports a negative progress and stays hidden.
    pub(super) fn progress(&self, byte: usize, now: Instant) -> Option<f32> {
        let index = self.letters.partition_point(|(range, _)| range.end <= byte);
        self.letters
            .get(index)
            .filter(|(range, _)| range.contains(&byte))
            .map(|(_, time)| {
                if now < *time {
                    return -1.;
                }
                (now.saturating_duration_since(*time).as_secs_f32() / self.settle.as_secs_f32())
                    .min(1.)
            })
    }
}
