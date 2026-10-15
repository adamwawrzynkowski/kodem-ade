//! Panes that the user resizes by dragging the divider between them, such as the file list
//! beside a diff. A pane with a set size keeps it while the window resizes; the others share
//! what is left.
use std::ops::Range;
use std::rc::Rc;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Axis, Bounds, CursorStyle, ElementId, Entity, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, RenderOnce, Styled, Window, canvas, div,
};

use crate::theme::px;
use crate::ui::git::theme::ActiveTheme;

/// Half the width of a divider's grab area, in points.
const REACH: f32 = 3.;

type ResizeCallback = Rc<dyn Fn(&Entity<ResizableState>, &mut Window, &mut App)>;

/// Sizes of the panes of one group, kept across renders.
#[derive(Default)]
pub struct ResizableState {
    /// The measured size of each pane along the group's axis.
    sizes: Vec<Pixels>,
    /// Sizes that the user dragged, which replace the panes' own sizes.
    dragged: Vec<Option<Pixels>>,
    drag: Option<Drag>,
}

#[derive(Clone, Copy)]
struct Drag {
    /// The pane that the drag resizes.
    pane: usize,
    /// Whether the pane lies after the divider, so that it grows as the pointer moves back.
    after: bool,
    start_pointer: Pixels,
    start_size: Pixels,
}

impl ResizableState {
    /// Returns the measured size of each pane along the group's axis.
    pub fn sizes(&self) -> &Vec<Pixels> {
        &self.sizes
    }
}

/// One pane of a resizable group.
pub struct ResizablePanel {
    size: Option<Pixels>,
    range: Range<Pixels>,
    children: Vec<AnyElement>,
}

/// Creates a pane.
pub fn resizable_panel() -> ResizablePanel {
    ResizablePanel {
        size: None,
        range: Pixels::ZERO..Pixels::MAX,
        children: Vec::new(),
    }
}

impl ResizablePanel {
    /// Gives the pane a size along the group's axis. The divider beside it changes this size.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Limits the pane's size along the group's axis.
    pub fn size_range(mut self, range: Range<Pixels>) -> Self {
        self.range = range;
        self
    }

    /// Kept for the views' builders: a pane with a size never grows on its own.
    pub fn flex_none(self) -> Self {
        self
    }
}

impl ParentElement for ResizablePanel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// A row or column of resizable panes.
#[derive(IntoElement)]
pub struct ResizableGroup {
    id: ElementId,
    axis: Axis,
    state: Option<Entity<ResizableState>>,
    on_resize: Option<ResizeCallback>,
    panels: Vec<ResizablePanel>,
}

/// Creates a row of panes with vertical dividers.
pub fn h_resizable(id: impl Into<ElementId>) -> ResizableGroup {
    ResizableGroup::new(id, Axis::Horizontal)
}

/// Creates a column of panes with horizontal dividers.
pub fn v_resizable(id: impl Into<ElementId>) -> ResizableGroup {
    ResizableGroup::new(id, Axis::Vertical)
}

impl ResizableGroup {
    fn new(id: impl Into<ElementId>, axis: Axis) -> Self {
        Self {
            id: id.into(),
            axis,
            state: None,
            on_resize: None,
            panels: Vec::new(),
        }
    }

    /// Keeps the pane sizes in `state`.
    pub fn with_state(mut self, state: &Entity<ResizableState>) -> Self {
        self.state = Some(state.clone());
        self
    }

    /// Calls `handler` when the user finishes dragging a divider.
    pub fn on_resize(
        mut self,
        handler: impl Fn(&Entity<ResizableState>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_resize = Some(Rc::new(handler));
        self
    }

    /// Adds a pane.
    pub fn child(mut self, panel: ResizablePanel) -> Self {
        self.panels.push(panel);
        self
    }
}

// Returns the length of a bounds along an axis.
fn length(bounds: Bounds<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => bounds.size.width,
        Axis::Vertical => bounds.size.height,
    }
}

// Returns the coordinate of a point along an axis.
fn along(position: gpui::Point<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Horizontal => position.x,
        Axis::Vertical => position.y,
    }
}

impl RenderOnce for ResizableGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.unwrap_or_else(|| {
            window.use_keyed_state(self.id.clone(), cx, |_, _| ResizableState::default())
        });
        let axis = self.axis;
        let count = self.panels.len();
        let ranges: Vec<Range<Pixels>> = self
            .panels
            .iter()
            .map(|panel| panel.range.clone())
            .collect();
        let fixed: Vec<Option<Pixels>> = {
            let state = state.read(cx);
            self.panels
                .iter()
                .enumerate()
                .map(|(index, panel)| {
                    let dragged = state.dragged.get(index).copied().flatten();
                    dragged.or(panel.size).map(|size| {
                        size.clamp(panel.range.start, panel.range.end.max(panel.range.start))
                    })
                })
                .collect()
        };
        let dragging = state.read(cx).drag.is_some();
        let theme = cx.theme();
        let line = theme.kodem.border;
        let accent = theme.kodem.accent;
        let mut group = div()
            .id(self.id.clone())
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .when(axis == Axis::Horizontal, |group| group.flex_row())
            .when(axis == Axis::Vertical, |group| group.flex_col());
        for (index, panel) in self.panels.into_iter().enumerate() {
            let measure = state.clone();
            let mut pane = div()
                .relative()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .children(panel.children)
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            measure.update(cx, |state, _| {
                                if state.sizes.len() != count {
                                    state.sizes.resize(count, Pixels::ZERO);
                                }
                                state.sizes[index] = length(bounds, axis);
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                );
            pane = match (fixed[index], axis) {
                (Some(size), Axis::Horizontal) => pane.flex_shrink_0().w(size).h_full(),
                (Some(size), Axis::Vertical) => pane.flex_shrink_0().h(size).w_full(),
                (None, Axis::Horizontal) => pane.flex_1().h_full(),
                (None, Axis::Vertical) => pane.flex_1().w_full(),
            };
            if index > 0 {
                // The divider resizes the pane with a size beside it, or the pane before it.
                let (pane_index, after) = if fixed[index].is_some() && fixed[index - 1].is_none() {
                    (index, true)
                } else {
                    (index - 1, false)
                };
                let start = state.clone();
                let handle = div()
                    .id(("git-resize-handle", index))
                    .group("git-resize-handle")
                    .relative()
                    .flex_shrink_0()
                    .when(axis == Axis::Horizontal, |handle| {
                        handle
                            .w(gpui::px(1.))
                            .h_full()
                            .cursor(CursorStyle::ResizeLeftRight)
                    })
                    .when(axis == Axis::Vertical, |handle| {
                        handle
                            .h(gpui::px(1.))
                            .w_full()
                            .cursor(CursorStyle::ResizeUpDown)
                    })
                    .bg(line)
                    .child(
                        div()
                            .id(("git-resize-reach", index))
                            .absolute()
                            .when(axis == Axis::Horizontal, |reach| {
                                reach
                                    .top_0()
                                    .bottom_0()
                                    .left(px(-REACH))
                                    .w(px(REACH * 2.) + gpui::px(1.))
                                    .cursor(CursorStyle::ResizeLeftRight)
                            })
                            .when(axis == Axis::Vertical, |reach| {
                                reach
                                    .left_0()
                                    .right_0()
                                    .top(px(-REACH))
                                    .h(px(REACH * 2.) + gpui::px(1.))
                                    .cursor(CursorStyle::ResizeUpDown)
                            })
                            .group_hover("git-resize-handle", move |reach| {
                                reach.bg(accent.opacity(0.35))
                            })
                            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                                cx.stop_propagation();
                                start.update(cx, |state, cx| {
                                    let size =
                                        state.sizes.get(pane_index).copied().unwrap_or_default();
                                    state.drag = Some(Drag {
                                        pane: pane_index,
                                        after,
                                        start_pointer: along(event.position, axis),
                                        start_size: size,
                                    });
                                    cx.notify();
                                });
                            }),
                    );
                group = group.child(handle);
            }
            group = group.child(pane);
        }
        let follow = state.clone();
        let on_resize = self.on_resize.clone();
        group.when(dragging, |group| {
            group.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let moved = follow.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, _, window, cx| {
                            moved.update(cx, |state, cx| {
                                let Some(drag) = state.drag else {
                                    return;
                                };
                                let travel = along(event.position, axis) - drag.start_pointer;
                                let size = if drag.after {
                                    drag.start_size - travel
                                } else {
                                    drag.start_size + travel
                                };
                                let range = &ranges[drag.pane];
                                let size = size.clamp(range.start, range.end.max(range.start));
                                if state.dragged.len() != count {
                                    state.dragged.resize(count, None);
                                }
                                state.dragged[drag.pane] = Some(size);
                                cx.notify();
                            });
                            window.refresh();
                        });
                        let released = follow.clone();
                        let on_resize = on_resize.clone();
                        window.on_mouse_event(move |_: &MouseUpEvent, _, window, cx| {
                            let ended = released.update(cx, |state, cx| {
                                cx.notify();
                                state.drag.take().is_some()
                            });
                            if ended && let Some(on_resize) = &on_resize {
                                on_resize(&released, window, cx);
                            }
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
        })
    }
}
