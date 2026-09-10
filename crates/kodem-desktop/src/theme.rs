//! Shared colors and depth for the native workspace.
use gpui::*;
/// Padding on all four sides of user-message bubbles.
pub const MESSAGE_PADDING: f32 = 12.;
/// Corner radius for user-message bubbles.
pub const MESSAGE_RADIUS: f32 = 9.;
/// Horizontal padding for inline blocks; their height follows the font size.
pub const INLINE_PADDING: f32 = 3.;
/// Corner radius for compact inline text blocks.
pub const INLINE_RADIUS: f32 = 4.;
#[derive(Clone, Copy)]
pub struct Theme {
    pub canvas: Hsla,
    pub sidebar: Hsla,
    pub raised: Hsla,
    pub inset: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub subtle: Hsla,
    pub accent: Hsla,
    pub accent_bg: Hsla,
    pub added: Hsla,
    pub added_bg: Hsla,
    pub removed: Hsla,
    pub removed_bg: Hsla,
    pub selected: Hsla,
    pub shadow: Hsla,
}
impl Theme {
    /// Returns the coordinated dark or light workspace palette.
    pub fn new(light: bool) -> Self {
        if light {
            Self {
                canvas: rgb_to_hsla(rgb(0xfafafa)),
                sidebar: rgb_to_hsla(rgb(0xf0f1f3)),
                raised: rgb_to_hsla(rgb(0xffffff)),
                inset: rgb_to_hsla(rgb(0xf5f6f8)),
                border: rgb_to_hsla(rgb(0xdfe1e5)),
                text: rgb_to_hsla(rgb(0x252932)),
                muted: rgb_to_hsla(rgb(0x626977)),
                subtle: rgb_to_hsla(rgb(0x7b8290)),
                accent: rgb_to_hsla(rgb(0x956324)),
                accent_bg: rgb_to_hsla(rgb(0xf4eadb)),
                added: rgb_to_hsla(rgb(0x267348)),
                added_bg: rgb_to_hsla(rgb(0xeaf5ee)),
                removed: rgb_to_hsla(rgb(0xb5464c)),
                removed_bg: rgb_to_hsla(rgb(0xfbecee)),
                selected: rgb_to_hsla(rgb(0xc7cbc7)),
                shadow: rgb_to_hsla(rgba(0x17202d14)),
            }
        } else {
            Self {
                canvas: rgb_to_hsla(rgb(0x0b0d0c)),
                sidebar: rgb_to_hsla(rgb(0x101211)),
                raised: rgb_to_hsla(rgb(0x1b1e1c)),
                inset: rgb_to_hsla(rgb(0x0c0e0d)),
                border: rgb_to_hsla(rgb(0x202320)),
                text: rgb_to_hsla(rgb(0xc7cbc7)),
                muted: rgb_to_hsla(rgb(0x939a93)),
                subtle: rgb_to_hsla(rgb(0x6e776f)),
                accent: rgb_to_hsla(rgb(0xcb864c)),
                accent_bg: rgb_to_hsla(rgb(0x302217)),
                added: rgb_to_hsla(rgb(0x58ae79)),
                added_bg: rgb_to_hsla(rgb(0x15291d)),
                removed: rgb_to_hsla(rgb(0xd36975)),
                removed_bg: rgb_to_hsla(rgb(0x2d1b20)),
                selected: rgb_to_hsla(rgb(0x252b26)),
                shadow: rgb_to_hsla(rgba(0x00000038)),
            }
        }
    }
    /// Returns a shallow contact shadow for a floating input or panel.
    pub fn elevation(self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                inset: true,
                color: self.text.opacity(0.045),
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
            },
            BoxShadow {
                inset: false,
                color: self.shadow,
                offset: point(px(0.), px(2.)),
                blur_radius: px(4.),
                spread_radius: px(-1.),
            },
            BoxShadow {
                inset: false,
                color: self.shadow,
                offset: point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(-5.),
            },
        ]
    }
}
