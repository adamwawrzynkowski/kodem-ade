//! Shared colors, type, geometry and depth for the native workspace.
//!
//! The visual language is Liquid's: cool graphite or paper surfaces, one phosphor accent that
//! stays vivid at low dot coverage, mono labels, hairline edges, and dot-matrix dithering in
//! place of gradients, glows and blurred glass.
use gpui::*;
/// Proportional interface and prose typeface: a bundled humanist sans-serif. It is a different
/// design from the mono face, so that prose and code are easy to tell apart.
pub const SANS: &str = "Fira Sans";
/// Code, metadata, counters, button and instrument labels.
pub const MONO: &str = "IBM Plex Mono";
/// Padding on all four sides of user-message bubbles.
pub const MESSAGE_PADDING: f32 = 11.;
/// Corner radius for user-message bubbles.
pub const MESSAGE_RADIUS: f32 = 8.;
/// Horizontal padding for inline blocks; their height follows the font size.
pub const INLINE_PADDING: f32 = 3.;
/// Corner radius for compact inline text blocks.
pub const INLINE_RADIUS: f32 = 2.;
/// Corner radius for chips, badges and keycaps.
pub const RADIUS_SMALL: f32 = 4.;
/// Corner radius for buttons, rows and fields.
pub const RADIUS_CONTROL: f32 = 6.;
/// Corner radius for code blocks, cards and the composer.
pub const RADIUS_SURFACE: f32 = 8.;
/// Corner radius for menus and dialogs.
pub const RADIUS_FLOATING: f32 = 10.;
/// Letter spacing for uppercase monospace labels.
pub const LABEL_TRACKING: f32 = 0.4;
#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    /// Deepest layer, visible behind cards and in gutters.
    pub backdrop: Hsla,
    /// Main content surface.
    pub canvas: Hsla,
    /// Title bar, rails and the sidebar.
    pub sidebar: Hsla,
    /// Raised rows and headers inside a panel.
    pub header: Hsla,
    /// Cards, popovers, dialogs and the composer.
    pub raised: Hsla,
    /// Recessed wells: code, fields and terminal output.
    pub inset: Hsla,
    /// Resting, hovered and pressed fills of a neutral control.
    pub control: Hsla,
    pub control_hover: Hsla,
    pub control_active: Hsla,
    /// Hairline separator.
    pub border: Hsla,
    /// A stronger edge for controls, floating surfaces and focused fields.
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
    pub warning: Hsla,
    pub info: Hsla,
    pub selected: Hsla,
    pub shadow: Hsla,
    /// Dither ink used for shadows and scrims.
    pub ink: Hsla,
    /// Neutral dot color for dithered textures on a surface.
    pub dot: Hsla,
    /// One-pixel top highlight that gives raised controls a machined edge.
    pub highlight: Hsla,
    pub light: bool,
}
impl Global for Theme {}
impl Theme {
    /// Returns the Liquid graphite or paper palette with the default accent.
    pub fn new(light: bool) -> Self {
        let c = |hex: u32| rgb_to_hsla(rgb(hex));
        let white = |alpha: f32| c(0xffffff).opacity(alpha);
        let black = |alpha: f32| c(0x000000).opacity(alpha);
        if light {
            let added = c(0x1f7a45);
            let removed = c(0xc23236);
            let accent = c(0xba520c);
            Self {
                backdrop: c(0xe7e7e4),
                canvas: c(0xfafaf8),
                sidebar: c(0xefefec),
                header: c(0xf3f3f0),
                raised: c(0xffffff),
                inset: c(0xf1f1ee),
                control: c(0xffffff),
                control_hover: c(0xf0f0ec),
                control_active: c(0xe6e6e1),
                border: black(0.09),
                edge: black(0.19),
                text: c(0x17181b),
                muted: c(0x5f5f5e),
                subtle: c(0x8c8c8b),
                accent,
                accent_bg: accent.opacity(0.12),
                on_accent: c(0xffffff),
                added,
                added_bg: added.opacity(0.08),
                removed,
                removed_bg: removed.opacity(0.08),
                warning: c(0x9e6a00),
                info: c(0x245bd1),
                selected: black(0.06),
                shadow: black(0.07),
                ink: black(0.3),
                dot: black(0.2),
                highlight: white(0.85),
                light,
            }
        } else {
            let added = c(0x5fd68f);
            let removed = c(0xff6b6e);
            let accent = c(0xff9447);
            Self {
                backdrop: c(0x0b0c0e),
                canvas: c(0x141518),
                sidebar: c(0x101113),
                header: c(0x18191d),
                raised: c(0x1b1c20),
                inset: c(0x0e0f11),
                control: c(0x232429),
                control_hover: c(0x2a2b31),
                control_active: c(0x32333a),
                border: white(0.08),
                edge: white(0.15),
                text: c(0xececef),
                muted: c(0xaaabac),
                subtle: c(0x777779),
                accent,
                accent_bg: accent.opacity(0.14),
                on_accent: c(0x08090a),
                added,
                added_bg: added.opacity(0.11),
                removed,
                removed_bg: removed.opacity(0.11),
                warning: c(0xf2bc4b),
                info: c(0x6ba6ff),
                selected: white(0.07),
                shadow: black(0.3),
                ink: black(0.72),
                dot: white(0.22),
                highlight: white(0.045),
                light,
            }
        }
    }
    /// Applies the saved accent to shared controls, formatting, and selected surfaces.
    pub fn with_accent(light: bool, accent: kodem_core::AccentColor) -> Self {
        let mut theme = Self::new(light);
        theme.accent = accent_color(accent, light);
        theme.accent_bg = theme.accent.opacity(if light { 0.12 } else { 0.14 });
        theme
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

/// Returns a named accent as a phosphor tone: vivid on graphite, deep on paper.
///
/// The saved palette supplies the hue. Saturation and lightness move into the band that keeps
/// dithered dots readable at low coverage in the selected appearance.
pub fn accent_color(accent: kodem_core::AccentColor, light: bool) -> Hsla {
    if accent == kodem_core::AccentColor::Orange {
        return Theme::new(light).accent;
    }
    let mut color = rgb_to_hsla(rgb(accent.rgb(light)));
    if light {
        color.saturation = (color.saturation * 1.3).clamp(0.6, 0.9);
        color.lightness = color.lightness.clamp(0.3, 0.46);
    } else {
        color.saturation = (color.saturation * 1.5).clamp(0.82, 1.);
        color.lightness = color.lightness.clamp(0.64, 0.74);
    }
    color
}
