//! The Git workspace's view of Kodem's theme. The workspace's views name colors by their role in
//! a Git client: the accent is `primary`, secondary text is `muted_foreground`, and list rows have
//! a `list_hover` fill. Every color here derives from the open Kodem theme, so the Git workspace
//! follows the theme, the accent and light or dark appearance like the rest of the window.
use std::ops::Deref;

use gpui::{App, Hsla, Pixels, rgb, rgb_to_hsla};

use crate::theme::Theme;

/// Colors of the Git workspace by role, derived from the Kodem theme.
#[derive(Debug, Clone, Copy)]
pub struct ThemeColor {
    /// Deepest layer of the window.
    pub background: Hsla,
    pub foreground: Hsla,
    /// Hairline edge of panels, rows and controls.
    pub border: Hsla,
    /// Stronger edge of raised controls and fields.
    pub input: Hsla,
    /// The accent.
    pub primary: Hsla,
    /// Text on an accent fill.
    pub primary_foreground: Hsla,
    /// Neutral fill of quiet surfaces and badges.
    pub muted: Hsla,
    /// Secondary text.
    pub muted_foreground: Hsla,
    /// Menus, popovers and dialogs.
    pub popover: Hsla,
    /// The main content surface behind lists.
    pub list: Hsla,
    pub list_hover: Hsla,
    pub sidebar: Hsla,
    pub title_bar: Hsla,
    pub button: Hsla,
    pub button_hover: Hsla,
    pub button_active: Hsla,
    pub danger: Hsla,
    pub info: Hsla,
}

/// Fills that the theme also exposes under their token names.
#[derive(Debug, Clone, Copy)]
pub struct ThemeTokens {
    pub secondary: Hsla,
}

/// The Git workspace's theme: role colors, the Git palette and the corner radii.
#[derive(Clone, Copy)]
pub struct GitTheme {
    pub colors: ThemeColor,
    pub tokens: ThemeTokens,
    /// Corner radius of controls and rows.
    pub radius: Pixels,
    /// Corner radius of menus, popovers and dialogs.
    pub radius_lg: Pixels,
    /// The Kodem theme that the colors derive from.
    pub kodem: Theme,
}

impl Deref for GitTheme {
    type Target = ThemeColor;

    fn deref(&self) -> &ThemeColor {
        &self.colors
    }
}

impl GitTheme {
    /// Derives the Git workspace's colors from a Kodem theme.
    pub fn new(t: Theme) -> Self {
        Self {
            colors: ThemeColor {
                background: t.backdrop,
                foreground: t.text,
                border: t.border,
                input: t.edge,
                primary: t.accent,
                primary_foreground: t.on_accent,
                muted: t.selected,
                muted_foreground: t.muted,
                popover: t.raised,
                list: t.canvas,
                list_hover: t.selected,
                sidebar: t.sidebar,
                title_bar: t.sidebar,
                button: t.control,
                button_hover: t.control_hover,
                button_active: t.control_active,
                danger: t.removed,
                info: t.info,
            },
            tokens: ThemeTokens {
                secondary: t.control,
            },
            radius: crate::theme::px(crate::theme::RADIUS_CONTROL),
            radius_lg: crate::theme::px(crate::theme::RADIUS_FLOATING),
            kodem: t,
        }
    }

    /// Reports whether the open theme is dark.
    pub fn is_dark(&self) -> bool {
        !self.kodem.light
    }

    /// Returns the surfaces, text tones and status colors that the workspace layers.
    pub fn liquid(&self) -> LiquidColors {
        let t = self.kodem;
        LiquidColors {
            window_backdrop: t.backdrop,
            panel: t.canvas,
            panel_secondary: t.header,
            elevated: t.raised,
            text_secondary: t.muted,
            text_tertiary: t.subtle,
            separator: t.border,
            ink: t.dot,
            success: t.added,
            danger: t.removed,
            warning: t.warning,
            info: t.info,
            addition_fill: t.added_bg,
            deletion_fill: t.removed_bg,
        }
    }
}

/// Reads the Git workspace's theme from the open Kodem theme.
pub trait ActiveTheme {
    /// Returns the Git workspace's colors for the open Kodem theme.
    fn theme(&self) -> GitTheme;
}

impl ActiveTheme for App {
    fn theme(&self) -> GitTheme {
        GitTheme::new(self.try_global::<Theme>().copied().unwrap_or_default())
    }
}

/// Surfaces, text tones and status colors of the Git workspace.
#[derive(Debug, Clone, Copy)]
pub struct LiquidColors {
    /// Deepest layer, visible behind cards and in gutters.
    pub window_backdrop: Hsla,
    /// Main content surface.
    pub panel: Hsla,
    /// Raised rows and headers inside a panel.
    pub panel_secondary: Hsla,
    /// Cards, popovers, and dialogs.
    pub elevated: Hsla,
    pub text_secondary: Hsla,
    pub text_tertiary: Hsla,
    /// Hairline separator.
    pub separator: Hsla,
    /// Neutral dot color for dithered textures.
    pub ink: Hsla,
    pub success: Hsla,
    pub danger: Hsla,
    pub warning: Hsla,
    pub info: Hsla,
    pub addition_fill: Hsla,
    pub deletion_fill: Hsla,
}

/// Returns the surfaces, text tones and status colors of the open theme.
pub fn liquid_colors(cx: &App) -> LiquidColors {
    cx.theme().liquid()
}

fn hex(value: u32) -> Hsla {
    rgb_to_hsla(rgb(value))
}

/// Graph lane colors. Lane 0, the first parent line, uses the theme's text tone; the others
/// are vivid phosphor tones that keep their color at low dot coverage.
pub fn graph_colors(cx: &App) -> Vec<Hsla> {
    let theme = cx.theme();
    let first = theme.kodem.text;
    if theme.is_dark() {
        vec![
            first,
            hex(0x6CF0A0),
            hex(0x6BA6FF),
            hex(0xFF9447),
            hex(0xA98BFF),
            hex(0xFF77A6),
            hex(0x5CD8E3),
            hex(0xF5D35C),
        ]
    } else {
        vec![
            first,
            hex(0x1F8048),
            hex(0x245BD1),
            hex(0xBA520C),
            hex(0x5B3FC4),
            hex(0xBB2F62),
            hex(0x13707E),
            hex(0x8F6C00),
        ]
    }
}
