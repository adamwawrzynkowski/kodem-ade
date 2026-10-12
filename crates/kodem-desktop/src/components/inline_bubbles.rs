//! Display-only inline padding with source byte coordinates preserved for selection and copying.
use crate::theme::{INLINE_PADDING, INLINE_RADIUS, Theme};
use gpui::*;
use std::{
    ops::{Deref, Range},
    sync::Arc,
};

const SPACER: &str = "\u{a0}";

#[derive(Default)]
pub(super) struct PaddingMap {
    insertions: Vec<(usize, Range<usize>)>,
    /// Displayed range of each bubble, and whether a click on it opens something.
    bubbles: Vec<(Range<usize>, bool)>,
}
impl PaddingMap {
    /// Converts a displayed byte position to the original text, excluding inserted padding.
    pub(super) fn source_index(&self, display: usize) -> usize {
        let mut removed = 0;
        for (source, range) in &self.insertions {
            if display < range.start {
                break;
            }
            if display < range.end {
                return *source;
            }
            removed += range.len();
        }
        display - removed
    }
    fn display_index(&self, source: usize) -> usize {
        source
            + self
                .insertions
                .iter()
                .take_while(|(index, _)| *index <= source)
                .map(|(_, range)| range.len())
                .sum::<usize>()
    }
}

/// Inserts measured, nonbreaking padding without changing the text owned by the input.
///
/// Each bubble is its source range and whether a click on it opens something.
pub(super) fn pad_text(
    text: &str,
    runs: Vec<TextRun>,
    bubbles: &[(Range<usize>, bool)],
    padding_run: TextRun,
) -> (String, Vec<TextRun>, Arc<PaddingMap>) {
    let mut events = bubbles
        .iter()
        .flat_map(|(range, opens)| [(range.start, true, *opens), (range.end, false, *opens)])
        .peekable();
    let mut displayed = String::new();
    let mut displayed_runs = Vec::new();
    let mut map = PaddingMap::default();
    let mut source = 0;
    let mut bubble_start = 0;
    for mut run in runs {
        // CoreText inherits attributes when the next run is appended. An omitted
        // value leaves the spacer's kerning on the following content.
        run.letter_spacing = Some(run.letter_spacing.unwrap_or(px(0.)));
        let end = source + run.len;
        while let Some(&(index, opening, opens)) = events.peek() {
            if index > end {
                break;
            }
            if index > source {
                displayed.push_str(&text[source..index]);
                displayed_runs.push(TextRun {
                    len: index - source,
                    ..run.clone()
                });
            }
            let start = displayed.len();
            displayed.push_str(SPACER);
            map.insertions.push((index, start..displayed.len()));
            displayed_runs.push(padding_run.clone());
            if opening {
                bubble_start = start;
            } else {
                map.bubbles.push((bubble_start..displayed.len(), opens));
            }
            source = index;
            events.next();
        }
        if source < end {
            displayed.push_str(&text[source..end]);
            displayed_runs.push(TextRun {
                len: end - source,
                ..run
            });
        }
        source = end;
    }
    (displayed, displayed_runs, Arc::new(map))
}

pub(super) struct TextLine {
    line: WrappedLine,
    map: Arc<PaddingMap>,
    display_start: usize,
    source_start: usize,
    paint_runs: Arc<Vec<TextRun>>,
}
impl Deref for TextLine {
    type Target = WrappedLine;
    fn deref(&self) -> &Self::Target {
        &self.line
    }
}
impl TextLine {
    /// Associates a shaped display line with the source bytes it represents.
    pub(super) fn new(
        line: WrappedLine,
        map: Arc<PaddingMap>,
        display_start: usize,
        paint_runs: Arc<Vec<TextRun>>,
    ) -> Self {
        let source_start = map.source_index(display_start);
        Self {
            line,
            map,
            display_start,
            source_start,
            paint_runs,
        }
    }
    /// Reuses shaped glyph positions for the transient streaming overlay.
    pub(super) fn streaming_glyphs(
        &self,
        origin: Point<Pixels>,
        height: Pixels,
        streaming: &super::streaming_text::StreamingText,
        now: std::time::Instant,
        visible: Bounds<Pixels>,
    ) -> Vec<super::streaming_text::RevealingGlyph> {
        let Some(start) = streaming.start() else {
            return Vec::new();
        };
        if self.source_start + self.len() <= start {
            return Vec::new();
        }
        let layout = &self.line.unwrapped_layout;
        let baseline = (height - layout.ascent - layout.descent) / 2. + layout.ascent;
        let mut wraps = self.line.wrap_boundaries().iter().peekable();
        let mut row = 0;
        let mut row_x = px(0.);
        let mut glyphs = Vec::new();
        for (run_ix, run) in layout.runs.iter().enumerate() {
            for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
                if wraps
                    .peek()
                    .is_some_and(|wrap| wrap.run_ix == run_ix && wrap.glyph_ix == glyph_ix)
                {
                    wraps.next();
                    row += 1;
                    row_x = glyph.position.x;
                }
                let position = origin
                    + point(
                        glyph.position.x - row_x,
                        height * row + baseline + glyph.position.y,
                    );
                if !Bounds::new(
                    position - point(layout.font_size, layout.font_size * 1.5),
                    size(layout.font_size * 3., layout.font_size * 3.),
                )
                .intersects(&visible)
                {
                    continue;
                }
                let source = self.map.source_index(self.display_start + glyph.index);
                // GPUI paints color emoji independently of the text foreground alpha.
                if glyph.is_emoji {
                    continue;
                }
                let Some(progress) = streaming.progress(source, now) else {
                    continue;
                };
                let mut end = 0;
                let Some(style) = self.paint_runs.iter().find(|run| {
                    end += run.len;
                    source < end
                }) else {
                    continue;
                };
                glyphs.push(super::streaming_text::RevealingGlyph {
                    origin: position,
                    font: run.font_id,
                    id: glyph.id,
                    font_size: layout.font_size,
                    color: style.color,
                    progress,
                });
            }
        }
        glyphs
    }
    /// Returns this line's original byte length without visual padding.
    pub(super) fn len(&self) -> usize {
        self.map.source_index(self.display_start + self.line.len()) - self.source_start
    }
    /// Locates an original byte offset in the padded layout.
    pub(super) fn position_for_index(
        &self,
        source: usize,
        height: Pixels,
    ) -> Option<Point<Pixels>> {
        let display = self
            .map
            .display_index(self.source_start + source)
            .saturating_sub(self.display_start)
            .min(self.line.len());
        self.line.position_for_index(display, height)
    }
    /// Maps pointer hits, including bubble padding, back to original byte offsets.
    pub(super) fn closest_index_for_position(
        &self,
        position: Point<Pixels>,
        height: Pixels,
    ) -> Result<usize, usize> {
        let source = |index| self.map.source_index(self.display_start + index) - self.source_start;
        self.line
            .closest_index_for_position(position, height)
            .map(source)
            .map_err(source)
    }
    /// Paints a padded, rounded background for each visible fragment of an inline bubble.
    pub(super) fn paint_bubbles(
        &self,
        origin: Point<Pixels>,
        height: Pixels,
        theme: Theme,
        window: &mut Window,
    ) {
        let layout = &self.line.unwrapped_layout;
        let mut start = 0;
        let ends = self
            .line
            .wrap_boundaries()
            .iter()
            .map(|boundary| layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index)
            .chain([self.line.len()]);
        for (row, end) in ends.enumerate() {
            let row_x = layout.x_for_index(start);
            let row_range = self.display_start + start..self.display_start + end;
            for (bubble, opens) in &self.map.bubbles {
                let a = bubble.start.max(row_range.start);
                let b = bubble.end.min(row_range.end);
                if a >= b || self.map.source_index(a) == self.map.source_index(b) {
                    continue;
                }
                let left = layout.x_for_index(a - self.display_start)
                    - row_x
                    - if a > bubble.start {
                        px(INLINE_PADDING)
                    } else {
                        px(0.)
                    };
                let right = layout.x_for_index(b - self.display_start) - row_x
                    + if b < bubble.end {
                        px(INLINE_PADDING)
                    } else {
                        px(0.)
                    };
                // The plate is a little taller than the letters.
                let block_height = self.line.font_size() + px(3.);
                let bounds = Bounds::new(
                    origin + point(left, height * row + (height - block_height) / 2.),
                    size(right - left, block_height),
                );
                // Inline code is drawn in the interface's own language: a plate of fine
                // dither instead of a flat fill, square at the corners. The pattern is drawn in
                // the shader, so a paragraph of many spans stays cheap.
                let cell = window.scale_factor();
                window.paint_quad(
                    fill(bounds, checkerboard(theme.dot.opacity(0.8), cell))
                        .corner_radii(Corners::all(px(INLINE_RADIUS))),
                );
                // Only a plate that opens something sits on a dotted rule, so that the rule
                // says that a click works and a line of plain code spans stays quiet.
                if *opens {
                    let baseline = Bounds::new(
                        point(bounds.left(), bounds.bottom() - px(1.)),
                        size(bounds.size.width, px(1.)),
                    );
                    window.paint_quad(fill(
                        baseline,
                        checkerboard(theme.accent.opacity(0.7), cell),
                    ));
                }
            }
            start = end;
        }
    }
}
