//! Vector icons of the Git workspace, drawn from Kodem's embedded icon set.
use gpui::{
    App, Hsla, IntoElement, Pixels, RenderOnce, SharedString, StyleRefinement, Styled, Window,
    prelude::*, svg,
};

use super::{Sizable, Size};
use crate::theme::px;

/// Icons that the Git workspace names by role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    ArrowLeft,
    ArrowRight,
    Bell,
    Check,
    ChevronDown,
    ChevronRight,
    ChevronUp,
    Close,
    Copy,
    ExternalLink,
    EyeOff,
    Folder,
    FolderOpen,
    Github,
    Globe,
    Info,
    Minus,
    Palette,
    Plus,
    Search,
    Settings,
    SquareTerminal,
    Star,
    TriangleAlert,
    Undo,
}

impl IconName {
    /// Returns the name of the icon in Kodem's embedded icon set.
    pub fn path(self) -> &'static str {
        match self {
            Self::ArrowLeft => "arrow-left",
            Self::ArrowRight => "arrow-right",
            Self::Bell => "bell",
            Self::Check => "check",
            Self::ChevronDown => "chevron-down",
            Self::ChevronRight => "chevron-right",
            Self::ChevronUp => "chevron-up",
            Self::Close => "close",
            Self::Copy => "copy",
            Self::ExternalLink => "external-link",
            Self::EyeOff => "eye-off",
            Self::Folder => "folder",
            Self::FolderOpen => "folder-open",
            Self::Github => "github",
            Self::Globe => "globe",
            Self::Info => "info",
            Self::Minus => "minus",
            Self::Palette => "palette",
            Self::Plus => "plus",
            Self::Search => "search",
            Self::Settings => "settings",
            Self::SquareTerminal => "square-terminal",
            Self::Star => "star",
            Self::TriangleAlert => "triangle-alert",
            Self::Undo => "undo-2",
        }
    }
}

/// A vector icon whose color follows the surrounding text unless it is set.
#[derive(IntoElement, Clone)]
pub struct Icon {
    path: SharedString,
    style: StyleRefinement,
    size: Option<Size>,
}

impl Icon {
    /// Creates an icon from a role name.
    pub fn new(name: IconName) -> Self {
        Self::named(name.path())
    }

    /// Creates an icon from its name in Kodem's embedded icon set, such as `git-branch`.
    pub fn named(path: &'static str) -> Self {
        Self {
            path: SharedString::new_static(path),
            style: StyleRefinement::default(),
            size: None,
        }
    }

    /// Sets the icon color.
    pub fn text_color(mut self, color: impl Into<Hsla>) -> Self {
        self.style.text.color = Some(color.into());
        self
    }

    /// Sets the width and the height of the icon.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(Size::Points(size.into()));
        self
    }
}

impl From<IconName> for Icon {
    fn from(name: IconName) -> Self {
        Icon::new(name)
    }
}

impl Sizable for Icon {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = Some(size.into());
        self
    }
}

impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let side = match self.size.unwrap_or_default() {
            Size::XSmall => px(12.),
            Size::Small => px(14.),
            Size::Medium => px(16.),
            Size::Points(size) => size,
        };
        let mut image = svg()
            .path(self.path)
            .flex_shrink_0()
            .size(side)
            .text_color(window.text_style().color);
        image.style().refine(&self.style);
        image
    }
}
