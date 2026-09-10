//! A narrow GPUI Kit/base compatibility port on the GPUI-CE type graph.
#![allow(dead_code)] // Preserve the attributed upstream component builder contracts.
mod button;
mod inline_bubbles;
mod input_frame;
pub(crate) mod streaming_text;
pub mod text_document;
pub mod text_input;
pub use button::Button;
use gpui::{Refineable, Role, StyleRefinement, prelude::*};
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
