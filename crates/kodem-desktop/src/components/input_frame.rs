// Adapted from GPUI Kit gpui-base 0.6.0. Apache-2.0; see assets/licenses.
use super::{StateStyle, StyledExt as _};
use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, Interactivity, IntoElement, ParentElement,
    Refineable as _, RenderOnce, Role, SharedString, StatefulInteractiveElement, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _,
};

/// The foundational input frame.
///
/// It intentionally owns only input semantics, interaction forwarding, and
/// normal children. Applications remain responsible for all presentation.
#[derive(IntoElement)]
pub struct InputBase {
    base: gpui::Stateful<Div>,
    style: StyleRefinement,
    semantic_styles: InputStyles,
    children: Vec<AnyElement>,
    focused: bool,
    disabled: bool,
    role: super::RoleOverride,
}

impl InputBase {
    /// Creates a native editable input frame.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id),
            style: StyleRefinement::default(),
            semantic_styles: InputStyles::default(),
            children: Vec::new(),
            focused: false,
            disabled: false,
            role: super::RoleOverride::Implicit,
        }
    }
    /// Sets the accessibility role.
    pub fn role(mut self, role: impl Into<super::RoleOverride>) -> Self {
        self.role = role.into();
        self
    }

    /// Sets the focus state.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Sets the disabled state.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Configures state-dependent appearance.
    pub fn styles(mut self, build: impl FnOnce(InputStyles) -> InputStyles) -> Self {
        self.semantic_styles = build(self.semantic_styles);
        self
    }

    /// Sets the spoken input label.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.base = self.base.aria_label(label.into());
        self
    }

    fn resolved_style(&self) -> StyleRefinement {
        super::resolve_style(
            &self.style,
            [
                self.focused.then_some(&self.semantic_styles.focused),
                self.disabled.then_some(&self.semantic_styles.disabled),
            ]
            .into_iter()
            .flatten(),
        )
    }
}

#[derive(Default)]
pub struct InputStyles {
    focused: StyleRefinement,
    disabled: StyleRefinement,
}

impl InputStyles {
    /// Sets the focus state.
    pub fn focused(mut self, build: impl FnOnce(StateStyle) -> StateStyle) -> Self {
        self.focused
            .refine(&build(StateStyle::default()).into_refinement());
        self
    }

    /// Sets the disabled state.
    pub fn disabled(mut self, build: impl FnOnce(StateStyle) -> StateStyle) -> Self {
        self.disabled
            .refine(&build(StateStyle::default()).into_refinement());
        self
    }
}

impl Styled for InputBase {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for InputBase {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl InteractiveElement for InputBase {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for InputBase {}

impl RenderOnce for InputBase {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let style = self.resolved_style();
        self.base
            .when_some(self.role.resolve(|| Role::TextInput), |this, role| {
                this.role(role)
            })
            .children(self.children)
            .refine_style(&style)
    }
}
