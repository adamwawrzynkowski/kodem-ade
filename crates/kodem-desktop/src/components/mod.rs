//! A narrow GPUI Kit/base compatibility port on the GPUI-CE type graph.
#![allow(dead_code)] // Preserve the attributed upstream component builder contracts.
mod button;
mod inline_bubbles;
mod input_frame;
pub(crate) mod streaming_text;
pub mod text_document;
pub mod text_input;
pub use button::Button;
use gpui::{AbsoluteLength, Corners, Pixels, Refineable, Role, StyleRefinement, prelude::*};
pub use input_frame::InputBase;

#[derive(Default)]
pub struct StateStyle(StyleRefinement);
impl StateStyle {
    /// Returns the style contributed by this state.
    pub fn into_refinement(self) -> StyleRefinement {
        self.0
    }
}
impl Styled for StateStyle {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.0
    }
}
#[derive(Clone, Copy)]
pub enum RoleOverride {
    Implicit,
    Explicit(Role),
}
impl RoleOverride {
    /// Resolves the default or caller-supplied accessibility role.
    pub fn resolve(self, default: impl FnOnce() -> Role) -> Option<Role> {
        Some(match self {
            Self::Implicit => default(),
            Self::Explicit(role) => role,
        })
    }
}
impl From<Role> for RoleOverride {
    fn from(role: Role) -> Self {
        Self::Explicit(role)
    }
}
pub trait Selectable: Sized {
    /// Sets the selected visual state.
    fn selected(self, selected: bool) -> Self;
    /// Returns the selected visual state.
    fn is_selected(&self) -> bool;
}
pub trait StyledExt: Styled + Sized {
    /// Applies a resolved Kit state style to a CE element.
    fn refine_style(mut self, style: &StyleRefinement) -> Self {
        self.style().refine(style);
        self
    }
}
impl<T: Styled> StyledExt for T {}
fn resolve_style<'a>(
    base: &StyleRefinement,
    states: impl IntoIterator<Item = &'a StyleRefinement>,
) -> StyleRefinement {
    let mut style = base.clone();
    for state in states {
        style.refine(state);
    }
    style
}

/// Returns the corners inside the border of an element with this style: each radius less the
/// wider of the two borders that meet at it. A layer that fills the element inside its border
/// takes these corners. Radii larger than half a side stay as set; painting clamps them.
pub(crate) fn inner_corners(style: &StyleRefinement, rem_size: Pixels) -> Corners<Pixels> {
    let length = |length: Option<AbsoluteLength>| {
        length.map_or(Pixels::ZERO, |length| length.to_pixels(rem_size))
    };
    let radii = &style.corner_radii;
    let borders = &style.border_widths;
    let (top, right, bottom, left) = (
        length(borders.top),
        length(borders.right),
        length(borders.bottom),
        length(borders.left),
    );
    let inset = |radius: Option<AbsoluteLength>, first: Pixels, second: Pixels| {
        (length(radius) - first.max(second)).max(Pixels::ZERO)
    };
    Corners {
        top_left: inset(radii.top_left, top, left),
        top_right: inset(radii.top_right, top, right),
        bottom_right: inset(radii.bottom_right, bottom, right),
        bottom_left: inset(radii.bottom_left, bottom, left),
    }
}
