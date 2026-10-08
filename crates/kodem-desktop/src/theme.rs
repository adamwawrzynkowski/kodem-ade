//! Shared colors, type, geometry and depth for the native workspace.
//!
//! The visual language is "ink on graphite": quiet neutral surfaces, one restrained accent,
//! hairline edges, and ordered dithering in place of gradients, glows and blurred glass.
use gpui::*;
/// Proportional interface and prose typeface.
pub const SANS: &str = "IBM Plex Sans";
/// Code, metadata, counters and instrument labels.
pub const MONO: &str = "JetBrains Mono";
/// Padding on all four sides of user-message bubbles.
pub const MESSAGE_PADDING: f32 = 11.;
/// Corner radius for user-message bubbles.
pub const MESSAGE_RADIUS: f32 = 7.;
/// Horizontal padding for inline blocks; their height follows the font size.
pub const INLINE_PADDING: f32 = 3.;
/// Corner radius for compact inline text blocks.
pub const INLINE_RADIUS: f32 = 3.;
/// Corner radius for chips, badges and keycaps.
pub const RADIUS_SMALL: f32 = 3.;
/// Corner radius for buttons, rows and fields.
pub const RADIUS_CONTROL: f32 = 5.;
/// Corner radius for code blocks, cards and the composer.
pub const RADIUS_SURFACE: f32 = 7.;
/// Corner radius for menus and dialogs.
pub const RADIUS_FLOATING: f32 = 9.;
/// Letter spacing for uppercase monospace labels.
pub const LABEL_TRACKING: f32 = 0.7;
#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    pub canvas: Hsla,
    pub sidebar: Hsla,
    pub raised: Hsla,
    pub inset: Hsla,
    pub border: Hsla,
    /// A stronger edge for floating surfaces and focused fields.
    pub edge: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub subtle: Hsla,
    pub accent: Hsla,
    pub accent_bg: Hsla,
    /// Text placed on an accent fill.
    pub on_accent: Hsla,
    pub added: Hsla,
    pub added_bg: Hsla,
    pub removed: Hsla,
    pub removed_bg: Hsla,
    pub selected: Hsla,
    pub shadow: Hsla,
    /// Dither ink used for shadows, scrims and shading.
    pub ink: Hsla,
    /// One-pixel top highlight that gives raised controls a machined edge.
    pub highlight: Hsla,
    pub light: bool,
}
impl Global for Theme {}
impl Theme {
    /// Returns the coordinated dark or light workspace palette.
    pub fn new(light: bool) -> Self {
        let c = |hex: u32| rgb_to_hsla(rgb(hex));
        if light {
            Self {
                canvas: c(0xf6f5f1),
                sidebar: c(0xedebe6),
                raised: c(0xfdfcfa),
                inset: c(0xf0eee9),
                border: c(0xdcd9d1),
                edge: c(0xc4c0b6),
                text: c(0x1b1a17),
                muted: c(0x5c5952),
                subtle: c(0x89857c),
                accent: c(0xc0581f),
                accent_bg: c(0xf6e6da),
                on_accent: c(0xfffaf5),
                added: c(0x2b7849),
                added_bg: c(0xe4f0e7),
                removed: c(0xb5404a),
                removed_bg: c(0xf8e6e7),
                selected: c(0xe3e0d8),
                shadow: rgb_to_hsla(rgba(0x1b1a1712)),
                ink: c(0x1b1a17).opacity(0.3),
                highlight: c(0xffffff).opacity(0.85),
                light,
            }
        } else {
            Self {
                canvas: c(0x0e0e0d),
                sidebar: c(0x131312),
                raised: c(0x1b1a19),
                inset: c(0x0a0a09),
                border: c(0x262523),
                edge: c(0x36342f),
                text: c(0xe9e6df),
                muted: c(0xa29e95),
                subtle: c(0x726f68),
                accent: c(0xec7a3c),
                accent_bg: c(0x2a1a10),
                on_accent: c(0x140b05),
                added: c(0x6cba84),
                added_bg: c(0x13231a),
                removed: c(0xe3727b),
                removed_bg: c(0x2b1619),
                selected: c(0x242320),
                shadow: rgb_to_hsla(rgba(0x0000004d)),
                ink: c(0x000000).opacity(0.72),
                highlight: c(0xffffff).opacity(0.045),
                light,
            }
        }
    }
    /// Applies the saved accent to shared controls, formatting, and selected surfaces.
    pub fn with_accent(light: bool, accent: kodem_core::AccentColor) -> Self {
        let mut theme = Self::new(light);
        if accent != kodem_core::AccentColor::Orange {
            theme.accent = accent_color(accent, light);
            theme.accent_bg = theme.accent;
            theme.accent_bg.lightness = if light { 0.92 } else { 0.12 };
            theme.accent_bg.saturation *= 0.45;
            theme.on_accent = if light { theme.raised } else { theme.inset };
        }
        theme
    }
    /// Returns the one-pixel machined edge used by raised controls; no blurred shadow.
    pub fn elevation(self) -> Vec<BoxShadow> {
        vec![BoxShadow {
            inset: true,
            color: self.highlight,
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
        }]
    }
    /// Returns a tight ambient shadow for floating menus and dialogs, paired with a dither shadow.
    pub fn floating(self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                inset: true,
                color: self.highlight,
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
            },
            BoxShadow {
                inset: false,
                color: self.shadow,
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
            },
        ]
    }
}

/// Returns a named accent as signal color against the neutral surfaces of either appearance.
pub fn accent_color(accent: kodem_core::AccentColor, light: bool) -> Hsla {
    if accent == kodem_core::AccentColor::Orange {
        return Theme::new(light).accent;
    }
    let mut color = rgb_to_hsla(rgb(accent.rgb(light)));
    color.saturation = (color.saturation * 1.12).min(1.);
    if !light {
        color.lightness = color.lightness.max(0.6);
    }
    color
}
