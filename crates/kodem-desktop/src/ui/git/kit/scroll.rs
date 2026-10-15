//! Scrolling for the Git workspace's lists. Kodem draws no scroll bars: content scrolls with
//! the platform's gestures and wheel, as in the rest of the window.
use gpui::{Div, ElementId, Stateful, prelude::*};

/// Lets a container scroll its overflowing content.
pub trait ScrollableElement: Sized {
    /// Scrolls the content vertically.
    fn overflow_y_scrollbar(self) -> Stateful<Div>;
}

impl ScrollableElement for Div {
    #[track_caller]
    fn overflow_y_scrollbar(self) -> Stateful<Div> {
        self.id(ElementId::CodeLocation(*std::panic::Location::caller()))
            .overflow_y_scroll()
    }
}

impl ScrollableElement for Stateful<Div> {
    fn overflow_y_scrollbar(self) -> Stateful<Div> {
        self.overflow_y_scroll()
    }
}
