//! A vertical list that builds only the rows in view, for histories and diffs with many
//! thousands of rows. The owning view builds the rows after its own render, during layout.
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Context, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, Render, ScrollHandle, ScrollStrategy,
    Size, StyleRefinement, Styled, Window, div, point,
};

/// Rows built beyond each edge of the view, so that a short scroll shows no gap.
const OVERSCAN: usize = 8;

type RenderRows<V> =
    Rc<dyn Fn(&mut V, Range<usize>, &mut Window, &mut Context<V>) -> Vec<AnyElement>>;

#[derive(Default)]
struct HandleState {
    /// The top of each row and the end of the last one.
    tops: Rc<Vec<Pixels>>,
    /// A scroll request that waits for the rows to be measured.
    pending: Option<(usize, ScrollStrategy)>,
}

/// The scroll position of a virtual list.
#[derive(Clone, Default)]
pub struct VirtualListScrollHandle {
    base: ScrollHandle,
    state: Rc<RefCell<HandleState>>,
}

impl VirtualListScrollHandle {
    /// Creates a handle at the top of the list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the scroll offset; it is negative when the list scrolled down.
    pub fn offset(&self) -> Point<Pixels> {
        self.base.offset()
    }

    /// Moves the list to an offset.
    pub fn set_offset(&self, offset: Point<Pixels>) {
        self.base.set_offset(offset);
    }

    /// Returns the largest scroll offset.
    pub fn max_offset(&self) -> Point<Pixels> {
        self.base.max_offset()
    }

    /// Returns the bounds of the list's view.
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.base.bounds()
    }

    /// Scrolls so that a row shows at the place that `strategy` names.
    pub fn scroll_to_item(&self, index: usize, strategy: ScrollStrategy) {
        let tops = self.state.borrow().tops.clone();
        if index + 1 >= tops.len() {
            self.state.borrow_mut().pending = Some((index, strategy));
            return;
        }
        let top = tops[index];
        let bottom = tops[index + 1];
        let view = self.base.bounds().size.height;
        let current = -self.base.offset().y;
        let target = match strategy {
            ScrollStrategy::Top => top,
            ScrollStrategy::Center => top - (view - (bottom - top)) / 2.,
            ScrollStrategy::Bottom => bottom - view,
            _ => {
                if top < current {
                    top
                } else if bottom > current + view {
                    bottom - view
                } else {
                    current
                }
            }
        };
        let max = tops.last().copied().unwrap_or_default() - view;
        let target = target.min(max).max(Pixels::ZERO);
        self.base.set_offset(point(self.base.offset().x, -target));
    }
}

/// Creates a virtual list whose rows `render` builds from `view`. `sizes` gives each row's
/// height.
pub fn v_virtual_list<V: Render, E: IntoElement>(
    view: Entity<V>,
    id: impl Into<ElementId>,
    sizes: Rc<Vec<Size<Pixels>>>,
    render: impl Fn(&mut V, Range<usize>, &mut Window, &mut Context<V>) -> Vec<E> + 'static,
) -> VirtualList<V> {
    VirtualList {
        id: id.into(),
        view,
        sizes,
        render: Rc::new(move |this, range, window, cx| {
            render(this, range, window, cx)
                .into_iter()
                .map(IntoElement::into_any_element)
                .collect()
        }),
        handle: VirtualListScrollHandle::new(),
        style: StyleRefinement::default(),
    }
}

/// A vertical list that builds only the rows in view.
pub struct VirtualList<V: 'static> {
    id: ElementId,
    view: Entity<V>,
    sizes: Rc<Vec<Size<Pixels>>>,
    render: RenderRows<V>,
    handle: VirtualListScrollHandle,
    style: StyleRefinement,
}

impl<V: 'static> VirtualList<V> {
    /// Keeps the scroll position in `handle`.
    pub fn track_scroll(mut self, handle: &VirtualListScrollHandle) -> Self {
        self.handle = handle.clone();
        self
    }
}

impl<V: 'static> Styled for VirtualList<V> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<V: Render> IntoElement for VirtualList<V> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<V: Render> Element for VirtualList<V> {
    type RequestLayoutState = AnyElement;
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
    ) -> (LayoutId, AnyElement) {
        let mut tops = Vec::with_capacity(self.sizes.len() + 1);
        let mut total = Pixels::ZERO;
        for size in self.sizes.iter() {
            tops.push(total);
            total += size.height;
        }
        tops.push(total);
        let tops = Rc::new(tops);
        let pending = {
            let mut state = self.handle.state.borrow_mut();
            state.tops = tops.clone();
            state.pending.take()
        };
        if let Some((index, strategy)) = pending {
            self.handle.scroll_to_item(index, strategy);
        }
        let view_height = match self.handle.base.bounds().size.height {
            height if height > Pixels::ZERO => height,
            _ => window.viewport_size().height,
        };
        let scrolled = -self.handle.base.offset().y;
        let count = self.sizes.len();
        // The first row whose bottom is below the top of the view, and the first row whose
        // top is below its bottom.
        let first = tops[1..].partition_point(|bottom| *bottom <= scrolled);
        let last = tops[..count].partition_point(|top| *top < scrolled + view_height);
        let range = first.saturating_sub(OVERSCAN)..(last + OVERSCAN).min(count);
        let render = self.render.clone();
        let rows = {
            let range = range.clone();
            self.view
                .update(cx, |this, cx| render(this, range, window, cx))
        };
        let rows = range.zip(rows).map(|(index, row)| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(tops[index])
                .h(self.sizes[index].height)
                .child(row)
        });
        let mut list = div()
            .id(self.id.clone())
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.handle.base)
            .child(div().relative().w_full().h(total).children(rows));
        list.style().refine(&self.style);
        let mut element = list.into_any_element();
        let layout_id = element.request_layout(window, cx);
        (layout_id, element)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        element: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        element.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        element: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        element.paint(window, cx);
    }
}
