//! The overview ruler of a diff: a dithered strip beside the text that maps every row of the
//! file, with removed lines on the left, added lines on the right, and a frame around the
//! visible part. Pressing the ruler scrolls there, and dragging it keeps scrolling.
use std::cell::Cell;
use std::rc::Rc;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    Bounds, Context, DispatchPhase, Div, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Stateful, WeakEntity, Window, canvas, div, fill, point, size,
};

use crate::theme::px;
use crate::ui::dither::cell_threshold;

/// Width of the overview ruler next to the diff text.
const OVERVIEW_RULER_WIDTH: f32 = 20.0;

/// What one diff row shows, for the overview ruler and the file stats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowMark {
    Context,
    Added,
    Removed,
    Hunk,
}

/// Colors for the overview ruler.
#[derive(Clone, Copy)]
pub(super) struct RulerPalette {
    pub(super) added: Hsla,
    pub(super) removed: Hsla,
    pub(super) hunk: Hsla,
    pub(super) context: Hsla,
    pub(super) viewport: Hsla,
    pub(super) background: Hsla,
    pub(super) border: Hsla,
}

/// How far the ruled content scrolls, in pixels.
#[derive(Clone, Copy)]
pub(super) struct ScrollMetrics {
    /// Distance from the top of the content to the top of the viewport.
    pub(super) offset: f32,
    /// Largest offset.
    pub(super) max: f32,
    /// Height of the viewport.
    pub(super) viewport: f32,
}

impl ScrollMetrics {
    /// The visible part of the content as `(start, length)` fractions of its full height.
    fn viewport_fraction(self) -> Option<(f32, f32)> {
        let total = self.max + self.viewport;
        (total > 0.0 && self.viewport > 0.0).then(|| (self.offset / total, self.viewport / total))
    }
}

/// A view that shows an overview ruler beside the content it scrolls.
pub(super) trait RulerScroll: Sized + 'static {
    /// Returns the state of the view's ruler.
    fn ruler(&self) -> &OverviewRuler;
    /// Returns how far the ruled content scrolls.
    fn scroll_metrics(&self) -> ScrollMetrics;
    /// Scrolls the ruled content so its top sits `offset` pixels above the viewport.
    fn set_scroll_offset(&mut self, offset: f32, cx: &mut Context<Self>);
}

/// Bounds and drag state of an overview ruler.
#[derive(Default)]
pub(super) struct OverviewRuler {
    /// Bounds of the ruler from the last paint, used to map clicks to rows.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// While the ruler is dragged, how far the pointer sits from the center of the viewport
    /// frame, so grabbing the frame does not jump.
    drag: Rc<Cell<Option<f32>>>,
}

/// Renders the ruler of `view`, with one mark for each row of its content in order.
pub(super) fn render_ruler<V: RulerScroll>(
    view: &V,
    marks: Rc<[RowMark]>,
    palette: RulerPalette,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let viewport = view.scroll_metrics().viewport_fraction();
    let bounds = view.ruler().bounds.clone();
    let drag = view.ruler().drag.clone();
    let view = cx.entity().downgrade();
    div()
        .id("diff-overview-ruler")
        .flex_shrink_0()
        .w(px(OVERVIEW_RULER_WIDTH))
        .h_full()
        .border_l_1()
        .border_color(palette.border)
        .bg(palette.background)
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|view, event: &MouseDownEvent, _, cx| {
                begin_drag(view, event.position.y, cx);
            }),
        )
        .child(
            canvas(
                move |paint_bounds, _, _| bounds.set(paint_bounds),
                move |paint_bounds, _, window, _| {
                    paint(paint_bounds, &marks, viewport, palette, window);
                    track_drag(drag, view, window);
                },
            )
            .size_full(),
        )
}

/// Starts a ruler drag. Pressing inside the viewport frame grabs it where it was pressed;
/// pressing elsewhere centers the frame there.
fn begin_drag<V: RulerScroll>(view: &mut V, y: Pixels, cx: &mut Context<V>) {
    let ruler = view.ruler();
    let bounds = ruler.bounds.get();
    let height = f32::from(bounds.size.height);
    let pointer = f32::from(y) - f32::from(bounds.origin.y);
    let anchor = view
        .scroll_metrics()
        .viewport_fraction()
        .map(|(start, length)| (start * height, length * height))
        .filter(|(top, length)| pointer >= *top && pointer <= top + length)
        .map_or(0.0, |(top, length)| pointer - (top + length / 2.0));
    ruler.drag.set(Some(anchor));
    scroll_to_position(view, y, cx);
}

/// Scrolls so the viewport frame centers on a ruler position, keeping the grab offset of the
/// current drag.
fn scroll_to_position<V: RulerScroll>(view: &mut V, y: Pixels, cx: &mut Context<V>) {
    let ruler = view.ruler();
    let bounds = ruler.bounds.get();
    let height = f32::from(bounds.size.height);
    let metrics = view.scroll_metrics();
    if height <= 0.0 || metrics.max + metrics.viewport <= 0.0 {
        return;
    }
    let anchor = ruler.drag.get().unwrap_or(0.0);
    let center = (f32::from(y) - anchor - f32::from(bounds.origin.y)) / height;
    let offset = (center * (metrics.max + metrics.viewport) - metrics.viewport / 2.0)
        .clamp(0.0, metrics.max.max(0.0));
    view.set_scroll_offset(offset, cx);
}

/// Follows an active ruler drag with window-wide listeners, so the drag keeps scrolling while
/// the pointer is outside the ruler.
fn track_drag<V: RulerScroll>(
    drag: Rc<Cell<Option<f32>>>,
    view: WeakEntity<V>,
    window: &mut Window,
) {
    let move_drag = drag.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
        if phase != DispatchPhase::Bubble || move_drag.get().is_none() {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            // The button was released outside the window.
            move_drag.set(None);
            return;
        }
        _ = view.update(cx, |view, cx| {
            scroll_to_position(view, event.position.y, cx);
        });
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, _| {
        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
            drag.set(None);
        }
    });
}

fn paint(
    bounds: Bounds<Pixels>,
    marks: &[RowMark],
    viewport: Option<(f32, f32)>,
    palette: RulerPalette,
    window: &mut Window,
) {
    let cell = 3.0;
    let dot = 2.0;
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let columns = (width / cell).floor() as i32;
    let rows = (height / cell).floor() as i32;
    if marks.is_empty() || columns < 2 || rows <= 0 {
        return;
    }
    let scale = window.scale_factor();
    let left = ((f32::from(bounds.origin.x) + (width - columns as f32 * cell) / 2.0) * scale)
        .round()
        / scale;
    let top = (f32::from(bounds.origin.y) * scale).round() / scale;
    let half = columns / 2;

    for row in 0..rows {
        let start = row as usize * marks.len() / rows as usize;
        let end = ((row as usize + 1) * marks.len() / rows as usize)
            .max(start + 1)
            .min(marks.len());
        let bin = &marks[start.min(marks.len() - 1)..end];
        let added = bin.contains(&RowMark::Added);
        let removed = bin.contains(&RowMark::Removed);
        let hunk = bin.contains(&RowMark::Hunk);
        for column in 0..columns {
            let color = if column < half && removed {
                Some(palette.removed)
            } else if column >= half && added {
                Some(palette.added)
            } else if hunk && !added && !removed {
                Some(palette.hunk.opacity(0.6))
            } else if column == half && cell_threshold(column, row) < 0.5 {
                Some(palette.context)
            } else {
                None
            };
            if let Some(color) = color {
                let origin = point(
                    gpui::px(left + column as f32 * cell),
                    gpui::px(top + row as f32 * cell),
                );
                window.paint_quad(fill(
                    Bounds::new(origin, size(gpui::px(dot), gpui::px(dot))),
                    color,
                ));
            }
        }
    }

    if let Some((start, length)) = viewport {
        let frame = Bounds::new(
            point(
                bounds.origin.x,
                bounds.origin.y + gpui::px(height * start.clamp(0.0, 1.0)),
            ),
            size(
                bounds.size.width,
                gpui::px((height * length).clamp(6.0, height)),
            ),
        );
        window.paint_quad(
            fill(frame, palette.viewport.opacity(0.07))
                .border_widths(gpui::px(1.))
                .border_color(palette.viewport.opacity(0.28)),
        );
    }
}
