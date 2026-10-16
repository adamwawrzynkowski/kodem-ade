//! Shared colors, type, geometry and depth for the native workspace.
//!
//! The visual language is Liquid's: layered surfaces in the colors of a named theme, one accent
//! that stays vivid at low dot coverage, mono labels, hairline edges, and dot-matrix dithering
//! for the shading of controls and surfaces.
use gpui::*;
use kodem_core::{AccentColor, CodeFont, Settings, TextFont, ThemeId};
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

// The saved interface scale, the two text scales and the two typefaces. Every length and font
// of the interface reads them, so they are process-wide instead of an argument of each builder.
static UI_SCALE: AtomicU32 = AtomicU32::new(1f32.to_bits());
static TEXT_SCALE: AtomicU32 = AtomicU32::new(1f32.to_bits());
static CODE_SCALE: AtomicU32 = AtomicU32::new(1f32.to_bits());
static TEXT_FONT: AtomicU8 = AtomicU8::new(0);
static CODE_FONT: AtomicU8 = AtomicU8::new(0);

/// Applies the saved interface scale, text sizes and typefaces to every length and font that is
/// built afterwards. Returns whether one of them changed.
pub fn apply_metrics(settings: &Settings) -> bool {
    let (smallest, largest) = Settings::UI_SCALE_RANGE;
    let scale = settings.ui_scale.clamp(smallest, largest);
    let (smallest, largest) = Settings::FONT_SIZE_RANGE;
    let text = settings.font_size.clamp(smallest, largest) / Settings::FONT_SIZE;
    let (smallest, largest) = Settings::CODE_FONT_SIZE_RANGE;
    let code = settings.code_font_size.clamp(smallest, largest) / Settings::CODE_FONT_SIZE;
    let text_font = TextFont::ALL
        .iter()
        .position(|font| *font == settings.text_font)
        .unwrap_or(0) as u8;
    let code_font = CodeFont::ALL
        .iter()
        .position(|font| *font == settings.code_font)
        .unwrap_or(0) as u8;
    let mut changed = UI_SCALE.swap(scale.to_bits(), Ordering::Relaxed) != scale.to_bits();
    changed |= TEXT_SCALE.swap(text.to_bits(), Ordering::Relaxed) != text.to_bits();
    changed |= CODE_SCALE.swap(code.to_bits(), Ordering::Relaxed) != code.to_bits();
    changed |= TEXT_FONT.swap(text_font, Ordering::Relaxed) != text_font;
    changed |= CODE_FONT.swap(code_font, Ordering::Relaxed) != code_font;
    changed
}

/// Returns the interface scale: 1 at 100%.
pub fn ui_scale() -> f32 {
    f32::from_bits(UI_SCALE.load(Ordering::Relaxed))
}

/// Returns a length of the interface at the interface scale: spacing, sizes of controls and
/// icons, and corner radii. Lengths that come from a measured layout or from the pointer are
/// window pixels already and use `gpui::px`.
pub fn px(points: f32) -> Pixels {
    gpui::px(points * ui_scale())
}

/// Returns a size for interface and prose text. It follows the text size setting and not the
/// interface scale.
pub fn text_px(points: f32) -> Pixels {
    gpui::px(points * f32::from_bits(TEXT_SCALE.load(Ordering::Relaxed)))
}

/// Returns a size for code text. It follows the code size setting and not the interface scale.
pub fn code_px(points: f32) -> Pixels {
    gpui::px(points * f32::from_bits(CODE_SCALE.load(Ordering::Relaxed)))
}

/// Returns the proportional interface and prose typeface. The default is a bundled humanist
/// sans-serif of a different design from the mono face, so that prose and code are easy to
/// tell apart.
pub fn sans() -> &'static str {
    TextFont::ALL
        .get(TEXT_FONT.load(Ordering::Relaxed) as usize)
        .copied()
        .unwrap_or_default()
        .family()
}

/// Returns the typeface of code, metadata, counters, button and instrument labels.
pub fn mono() -> &'static str {
    CodeFont::ALL
        .get(CODE_FONT.load(Ordering::Relaxed) as usize)
        .copied()
        .unwrap_or_default()
        .family()
}
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
    pub id: ThemeId,
}
impl Global for Theme {}
impl Default for Theme {
    fn default() -> Self {
        Self::new(ThemeId::default())
    }
}

// The colors that give a theme its character. The edges, tints and dither inks follow from
// them and from whether the theme is light.
struct Palette {
    backdrop: u32,
    canvas: u32,
    sidebar: u32,
    header: u32,
    raised: u32,
    inset: u32,
    /// Resting, hovered and pressed fills of a neutral control.
    control: [u32; 3],
    text: u32,
    muted: u32,
    subtle: u32,
    accent: u32,
    on_accent: u32,
    added: u32,
    removed: u32,
    warning: u32,
    info: u32,
}

// Returns the palette of a theme. The semantic colors of the classic themes are their own
// greens, reds and yellows; a few are deepened or lifted so that they stay readable as text.
fn palette(id: ThemeId) -> Palette {
    match id {
        ThemeId::Graphite => Palette {
            backdrop: 0x0b0c0e,
            canvas: 0x141518,
            sidebar: 0x101113,
            header: 0x18191d,
            raised: 0x1b1c20,
            inset: 0x0e0f11,
            control: [0x232429, 0x2a2b31, 0x32333a],
            text: 0xececef,
            muted: 0xaaabac,
            subtle: 0x777779,
            accent: PHOSPHOR_DARK,
            on_accent: 0x08090a,
            added: 0x5fd68f,
            removed: 0xff6b6e,
            warning: 0xf2bc4b,
            info: 0x6ba6ff,
        },
        // Kodem's own night theme: plum-black surfaces, lilac text, a hot pink accent and a
        // violet light. Mint, coral and gold carry the states without leaving the dusk.
        ThemeId::Adys => Palette {
            backdrop: 0x0d0912,
            canvas: 0x17111f,
            sidebar: 0x120d19,
            header: 0x1c1526,
            raised: 0x221a2e,
            inset: 0x110c17,
            control: [0x2a2038, 0x342843, 0x3e304f],
            text: 0xf4ecf8,
            muted: 0xc6b6d4,
            subtle: 0x9583ab,
            accent: 0xff6ac1,
            on_accent: 0x1a0a1e,
            added: 0x6ef2b8,
            removed: 0xff5f6d,
            warning: 0xf9cb76,
            info: 0xb69cff,
        },
        ThemeId::Monokai => Palette {
            backdrop: 0x171814,
            canvas: 0x272822,
            sidebar: 0x1e1f1c,
            header: 0x2d2e27,
            raised: 0x31322b,
            inset: 0x1c1d19,
            control: [0x3a3b33, 0x44453b, 0x4f5045],
            text: 0xf8f8f2,
            muted: 0xbdbcae,
            subtle: 0x8a8773,
            accent: 0xfd971f,
            on_accent: 0x1c1d19,
            added: 0xa6e22e,
            removed: 0xf92672,
            warning: 0xe6db74,
            info: 0x66d9ef,
        },
        ThemeId::Dracula => Palette {
            backdrop: 0x191a21,
            canvas: 0x282a36,
            sidebar: 0x21222c,
            header: 0x2d2f3d,
            raised: 0x313342,
            inset: 0x1e1f29,
            control: [0x3a3c4e, 0x44475a, 0x4f5268],
            text: 0xf8f8f2,
            muted: 0xbcc0d6,
            subtle: 0x7b88b8,
            accent: 0xbd93f9,
            on_accent: 0x191a21,
            added: 0x50fa7b,
            removed: 0xff5555,
            warning: 0xf1fa8c,
            info: 0x8be9fd,
        },
        ThemeId::OneDark => Palette {
            backdrop: 0x1b1d23,
            canvas: 0x282c34,
            sidebar: 0x21252b,
            header: 0x2c313a,
            raised: 0x30353f,
            inset: 0x1e2127,
            control: [0x383d48, 0x414754, 0x4b5261],
            text: 0xd7dae0,
            muted: 0xabb2bf,
            subtle: 0x7f848e,
            accent: 0x61afef,
            on_accent: 0x1b1d23,
            added: 0x98c379,
            removed: 0xe06c75,
            warning: 0xe5c07b,
            info: 0x56b6c2,
        },
        ThemeId::Nord => Palette {
            backdrop: 0x222730,
            canvas: 0x2e3440,
            sidebar: 0x272c36,
            header: 0x333a47,
            raised: 0x3b4252,
            inset: 0x262b35,
            control: [0x434c5e, 0x4c566a, 0x566179],
            text: 0xeceff4,
            muted: 0xc2cad8,
            subtle: 0x8c98b0,
            accent: 0x88c0d0,
            on_accent: 0x222730,
            added: 0xa3be8c,
            removed: 0xd0707a,
            warning: 0xebcb8b,
            info: 0x81a1c1,
        },
        ThemeId::TokyoNight => Palette {
            backdrop: 0x13131a,
            canvas: 0x1a1b26,
            sidebar: 0x16161e,
            header: 0x1f2130,
            raised: 0x24283b,
            inset: 0x15151d,
            control: [0x292e42, 0x323851, 0x3b4261],
            text: 0xc0caf5,
            muted: 0xa9b1d6,
            subtle: 0x737aa2,
            accent: 0x7aa2f7,
            on_accent: 0x13131a,
            added: 0x9ece6a,
            removed: 0xf7768e,
            warning: 0xe0af68,
            info: 0x7dcfff,
        },
        ThemeId::GruvboxDark => Palette {
            backdrop: 0x161819,
            canvas: 0x282828,
            sidebar: 0x1d2021,
            header: 0x2e2c2b,
            raised: 0x32302f,
            inset: 0x1c1e1f,
            control: [0x3c3836, 0x47423f, 0x504945],
            text: 0xebdbb2,
            muted: 0xbdae93,
            subtle: 0x928374,
            accent: 0xfe8019,
            on_accent: 0x1d2021,
            added: 0xb8bb26,
            removed: 0xfb4934,
            warning: 0xfabd2f,
            info: 0x83a598,
        },
        ThemeId::Mocha => Palette {
            backdrop: 0x11111b,
            canvas: 0x1e1e2e,
            sidebar: 0x181825,
            header: 0x232334,
            raised: 0x27273a,
            inset: 0x161622,
            control: [0x313244, 0x3b3c50, 0x45475a],
            text: 0xcdd6f4,
            muted: 0xa6adc8,
            subtle: 0x7f849c,
            accent: 0xcba6f7,
            on_accent: 0x11111b,
            added: 0xa6e3a1,
            removed: 0xf38ba8,
            warning: 0xf9e2af,
            info: 0x89b4fa,
        },
        ThemeId::SolarizedDark => Palette {
            backdrop: 0x001f27,
            canvas: 0x002b36,
            sidebar: 0x00252f,
            header: 0x04313c,
            raised: 0x073642,
            inset: 0x00232c,
            control: [0x0b4050, 0x114a5b, 0x175466],
            text: 0xb4c2c3,
            muted: 0x93a1a1,
            subtle: 0x6c8289,
            accent: 0x3a9ee4,
            on_accent: 0x001f27,
            added: 0x9bb000,
            removed: 0xe9605d,
            warning: 0xc99a00,
            info: 0x2aa198,
        },
        ThemeId::Paper => Palette {
            backdrop: 0xe7e7e4,
            canvas: 0xfafaf8,
            sidebar: 0xefefec,
            header: 0xf3f3f0,
            raised: 0xffffff,
            inset: 0xf1f1ee,
            control: [0xffffff, 0xf0f0ec, 0xe6e6e1],
            text: 0x17181b,
            muted: 0x5f5f5e,
            subtle: 0x8c8c8b,
            accent: PHOSPHOR_LIGHT,
            on_accent: 0xffffff,
            added: 0x1f7a45,
            removed: 0xc23236,
            warning: 0x9e6a00,
            info: 0x245bd1,
        },
        ThemeId::Latte => Palette {
            backdrop: 0xdce0e8,
            canvas: 0xeff1f5,
            sidebar: 0xe6e9ef,
            header: 0xe9ecf1,
            raised: 0xf8f9fb,
            inset: 0xe6e9ef,
            control: [0xf8f9fb, 0xe1e5ec, 0xd3d7e0],
            text: 0x4c4f69,
            muted: 0x5c5f77,
            subtle: 0x7c7f93,
            accent: 0x8839ef,
            on_accent: 0xffffff,
            added: 0x2f8a1e,
            removed: 0xd20f39,
            warning: 0xa5650a,
            info: 0x1e66f5,
        },
        ThemeId::SolarizedLight => Palette {
            backdrop: 0xeee8d5,
            canvas: 0xfdf6e3,
            sidebar: 0xf4edd9,
            header: 0xf8f1dd,
            raised: 0xfffbee,
            inset: 0xf5eedb,
            control: [0xfffbee, 0xf0e9d4, 0xe6dfc9],
            text: 0x435459,
            muted: 0x586e75,
            subtle: 0x74878c,
            accent: 0x1f78b8,
            on_accent: 0xfdf6e3,
            added: 0x6c7c00,
            removed: 0xd12b28,
            warning: 0x9a7400,
            info: 0x1e857d,
        },
        ThemeId::GruvboxLight => Palette {
            backdrop: 0xebdbb2,
            canvas: 0xfbf1c7,
            sidebar: 0xf2e5bc,
            header: 0xf6ebc1,
            raised: 0xfdf6d8,
            inset: 0xf3e7be,
            control: [0xfdf6d8, 0xeee0b7, 0xe3d4a8],
            text: 0x3c3836,
            muted: 0x5a514b,
            subtle: 0x7c6f64,
            accent: 0xaf3a03,
            on_accent: 0xfbf1c7,
            added: 0x79740e,
            removed: 0x9d0006,
            warning: 0xa16a0c,
            info: 0x076678,
        },
        ThemeId::OneLight => Palette {
            backdrop: 0xe5e5e6,
            canvas: 0xfafafa,
            sidebar: 0xf0f0f1,
            header: 0xf4f4f5,
            raised: 0xffffff,
            inset: 0xf2f2f3,
            control: [0xffffff, 0xededee, 0xe2e2e4],
            text: 0x383a42,
            muted: 0x5f6168,
            subtle: 0x82848b,
            accent: 0x4078f2,
            on_accent: 0xffffff,
            added: 0x3f8a3e,
            removed: 0xca3b2e,
            warning: 0x986801,
            info: 0x0184bc,
        },
    }
}
impl Theme {
    /// Returns the palette of a named theme with the theme's own accent.
    pub fn new(id: ThemeId) -> Self {
        let c = |hex: u32| rgb_to_hsla(rgb(hex));
        let white = |alpha: f32| c(0xffffff).opacity(alpha);
        let black = |alpha: f32| c(0x000000).opacity(alpha);
        let light = id.light();
        let palette = palette(id);
        let (accent, added, removed) = (c(palette.accent), c(palette.added), c(palette.removed));
        let surfaces = Self {
            backdrop: c(palette.backdrop),
            canvas: c(palette.canvas),
            sidebar: c(palette.sidebar),
            header: c(palette.header),
            raised: c(palette.raised),
            inset: c(palette.inset),
            control: c(palette.control[0]),
            control_hover: c(palette.control[1]),
            control_active: c(palette.control[2]),
            border: black(0.09),
            edge: black(0.19),
            text: c(palette.text),
            muted: c(palette.muted),
            subtle: c(palette.subtle),
            accent,
            accent_bg: accent.opacity(accent_tint(light)),
            on_accent: c(palette.on_accent),
            added,
            added_bg: added.opacity(0.08),
            removed,
            removed_bg: removed.opacity(0.08),
            warning: c(palette.warning),
            info: c(palette.info),
            selected: black(0.06),
            shadow: black(0.07),
            ink: black(0.3),
            dot: black(0.2),
            highlight: white(0.85),
            light,
            id,
        };
        if light {
            surfaces
        } else {
            Self {
                border: white(0.08),
                edge: white(0.15),
                added_bg: added.opacity(0.11),
                removed_bg: removed.opacity(0.11),
                selected: white(0.07),
                shadow: black(0.3),
                ink: black(0.72),
                dot: white(0.22),
                highlight: white(0.045),
                ..surfaces
            }
        }
    }
    /// Applies the saved accent to shared controls, formatting, and selected surfaces. The
    /// default accent stands for the theme's own.
    pub fn with_accent(id: ThemeId, accent: AccentColor) -> Self {
        let mut theme = Self::new(id);
        if accent != AccentColor::default() {
            theme.accent = accent_color(accent, theme.light);
            theme.accent_bg = theme.accent.opacity(accent_tint(theme.light));
        }
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

/// Kodem's own orange: vivid on dark surfaces, deep on light ones.
const PHOSPHOR_DARK: u32 = 0xff9447;
const PHOSPHOR_LIGHT: u32 = 0xba520c;

// Returns the strength of the accent tint behind selected and highlighted content.
fn accent_tint(light: bool) -> f32 {
    if light { 0.12 } else { 0.14 }
}

/// Returns a named accent as a phosphor tone: vivid on dark surfaces, deep on light ones.
///
/// The saved palette supplies the hue. Saturation and lightness move into the band that keeps
/// dithered dots readable at low coverage in the selected appearance.
pub fn accent_color(accent: AccentColor, light: bool) -> Hsla {
    if accent == AccentColor::Orange {
        return rgb_to_hsla(rgb(if light { PHOSPHOR_LIGHT } else { PHOSPHOR_DARK }));
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
