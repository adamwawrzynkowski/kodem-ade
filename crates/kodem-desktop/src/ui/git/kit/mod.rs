//! Building blocks of the Git workspace's views, drawn with Kodem's surfaces, dither and
//! motion. The views came from the standalone Liquid app, which built on a component library
//! that Kodem's GPUI type graph cannot load; these blocks keep the shape of the calls the views
//! make, so the views read the same while every element is Kodem's own.
mod button;
mod dialog;
mod icon;
mod input;
mod menu;
mod popover;
mod resizable;
mod scroll;
mod text_view;
mod virtual_list;

pub use button::{ButtonVariants, DropdownMenu, LiquidButton};
pub use dialog::{Notification, Tooltip, WindowExt, overlays};
pub use icon::{Icon, IconName};
pub use input::{Input, InputEvent, InputState, Textarea, TextareaState};
pub use menu::{ContextMenuExt, PopupMenu, PopupMenuItem, popup_menu_layer};
pub use popover::Popover;
pub use resizable::{ResizableState, h_resizable, resizable_panel, v_resizable};
pub use scroll::ScrollableElement;
pub use text_view::TextView;
pub use virtual_list::{VirtualListScrollHandle, v_virtual_list};

pub use crate::components::Selectable;
pub use crate::ui::git::theme::ActiveTheme;

use gpui::{Div, ParentElement, Pixels, Styled, div};

/// Returns a row that lays out its children horizontally and centers them vertically.
pub fn h_flex() -> Div {
    div().flex().flex_row().items_center()
}

/// Returns a column that lays out its children vertically.
pub fn v_flex() -> Div {
    div().flex().flex_col()
}

/// The size of a control or an icon.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Size {
    XSmall,
    Small,
    #[default]
    Medium,
    /// A size in points.
    Points(Pixels),
}

impl From<Pixels> for Size {
    fn from(size: Pixels) -> Self {
        Size::Points(size)
    }
}

/// An element that comes in several sizes.
pub trait Sizable: Sized {
    /// Sets the size.
    fn with_size(self, size: impl Into<Size>) -> Self;

    /// Uses the extra small size.
    fn xsmall(self) -> Self {
        self.with_size(Size::XSmall)
    }

    /// Uses the small size.
    fn small(self) -> Self {
        self.with_size(Size::Small)
    }
}

/// An element that can be disabled.
pub trait Disableable: Sized {
    /// Stops the element from reacting to the user.
    fn disabled(self, disabled: bool) -> Self;
}

/// Eases a transition out with a cubic curve.
pub fn ease_out_cubic(progress: f32) -> f32 {
    1. - (1. - progress).powi(3)
}

/// Styles a container as a floating surface: the popover fill, a hairline edge, rounded corners
/// and the floating shadow.
pub trait PopoverStyled: Styled + ParentElement + Sized {
    /// Applies the popover surface style.
    fn popover_style(self, cx: &gpui::App) -> Self {
        let theme = cx.theme();
        self.bg(theme.popover)
            .border_1()
            .border_color(theme.kodem.edge)
            .rounded(theme.radius_lg)
            .shadow(theme.kodem.floating())
    }
}

impl<T: Styled + ParentElement> PopoverStyled for T {}
