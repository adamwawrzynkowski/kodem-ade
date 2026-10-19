// Added by Kodem in October 2026 to gpui-ce 0.2.2.

//! Scroll chaining: nested scroll containers share one wheel gesture.
//!
//! Every scroll container under the pointer sees each scroll wheel event, the innermost one
//! first. Each axis of the event goes to the innermost container that can still move along it,
//! and its ancestors see that axis as taken. A container at its start or end leaves the axis to
//! the container around it.
//!
//! The container that takes an axis owns that axis for the rest of the gesture, while the
//! pointer stays over it: a flick that reaches the end of an inner list does not carry on into
//! the page around it, and an inner list that scrolls under the pointer does not catch a gesture
//! that scrolls its page. A gesture ends when the platform reports a new touch, or when the wheel
//! rests for [`GESTURE_TIMEOUT`] with no fingers on the touchpad.

use crate::{Along, Axis, Pixels, Point, TouchPhase, Window};
use scheduler::Instant;
use std::{rc::Rc, time::Duration};

/// How long the wheel rests before its next event starts a new gesture, unless fingers stay on
/// the touchpad. Mouse wheels and some touchpads report no touch phases, so only this ends their
/// gestures.
const GESTURE_TIMEOUT: Duration = Duration::from_millis(300);

/// The axes of a scroll wheel event, in the order that containers take them.
const AXES: [Axis; 2] = [Axis::Horizontal, Axis::Vertical];

/// Identifies a scroll container across frames, so that a gesture stays with the container that
/// took it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollContainerId(usize);

impl ScrollContainerId {
    /// The container whose scroll state is `state`.
    pub(crate) fn of<T: ?Sized>(state: &Rc<T>) -> Self {
        Self(Rc::as_ptr(state).cast::<()>().addr())
    }
}

/// Which containers own the current gesture, and which axes of the current event are taken.
#[derive(Default)]
pub(crate) struct ScrollChain {
    /// The container that owns each axis of the current gesture.
    owners: Point<Option<ScrollContainerId>>,
    /// Whether the owner of each axis is under the pointer for the current event.
    owners_under_pointer: Point<bool>,
    /// Whether a container has taken each axis of the current event.
    taken: Point<bool>,
    /// When the previous scroll wheel event came.
    last_event: Option<Instant>,
    /// Whether fingers are on the touchpad, between the touch phases that start and end a touch.
    touching: bool,
}

impl ScrollChain {
    /// Prepares for a new scroll wheel event, and ends the gesture when this event starts a new
    /// one. `precise` tells whether the event comes from a touchpad rather than a mouse wheel.
    pub(crate) fn begin_event(&mut self, phase: TouchPhase, precise: bool) {
        let now = Instant::now();
        // A mouse wheel turns, so no fingers rest on a touchpad, even if its touch never ended.
        if !precise {
            self.touching = false;
        }
        let rested = !self.touching
            && self
                .last_event
                .is_none_or(|last| now.duration_since(last) > GESTURE_TIMEOUT);
        if phase == TouchPhase::Started || rested {
            self.owners = Point::default();
        }
        match phase {
            TouchPhase::Started => self.touching = true,
            TouchPhase::Ended | TouchPhase::Cancelled => self.touching = false,
            TouchPhase::Moved => {}
        }
        self.last_event = Some(now);
        self.owners_under_pointer = Point::default();
        self.taken = Point::default();
    }
}

/// Reports whether a scroll offset, which lies in `-max..=0`, can move by `delta`.
pub(crate) fn offset_can_move(offset: Pixels, delta: Pixels, max: Pixels) -> bool {
    // Compared as floats: `Pixels` orders -0 below 0, and an offset at the start can be -0.
    let (offset, delta, max) = (f32::from(offset), f32::from(delta), f32::from(max));
    if delta > 0.0 {
        offset < 0.0
    } else if delta < 0.0 {
        offset > -max
    } else {
        false
    }
}

impl ScrollChain {
    /// Notes that `container` is under the pointer for the current event.
    fn note(&mut self, container: ScrollContainerId) {
        for axis in AXES {
            if self.owners.along(axis) == Some(container) {
                self.owners_under_pointer = self.owners_under_pointer.apply_along(axis, |_| true);
            }
        }
    }

    /// Takes the axes of `delta` that go to `container` for the current event, and returns the
    /// delta along them.
    fn take(
        &mut self,
        container: ScrollContainerId,
        delta: Point<Pixels>,
        can_move: Point<bool>,
    ) -> Point<Pixels> {
        let mut taken = Point::default();
        for axis in AXES {
            let axis_delta = delta.along(axis);
            if axis_delta == Pixels::ZERO || self.taken.along(axis) {
                continue;
            }
            let takes = match self.owners.along(axis) {
                Some(owner) if self.owners_under_pointer.along(axis) => owner == container,
                _ => can_move.along(axis),
            };
            if takes {
                taken = taken.apply_along(axis, |_| axis_delta);
                self.taken = self.taken.apply_along(axis, |_| true);
                self.owners = self.owners.apply_along(axis, |_| Some(container));
            }
        }
        taken
    }
}

impl Window {
    /// Notes that `container` is under the pointer for the current scroll wheel event. A scroll
    /// container calls this in the capture phase, so that the containers inside it know whether
    /// it owns the gesture.
    pub fn note_scroll_container(&mut self, container: ScrollContainerId) {
        self.scroll_chain.note(container);
    }

    /// Takes the axes of `delta` that `container` scrolls along for the current scroll wheel
    /// event, and returns the delta to scroll by: zero along an axis that another container
    /// takes. A scroll container calls this in the bubble phase with the delta along the axes it
    /// scrolls, and with `can_move` telling along which of them it is not yet at its end.
    pub fn take_scroll_delta(
        &mut self,
        container: ScrollContainerId,
        delta: Point<Pixels>,
        can_move: Point<bool>,
    ) -> Point<Pixels> {
        self.scroll_chain.take(container, delta, can_move)
    }
}
