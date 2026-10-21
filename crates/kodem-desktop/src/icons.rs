//! Embedded native vector assets. No runtime asset directory is required.
use gpui::{prelude::*, *};
use std::borrow::Cow;
// Expands each icon name into an arm that embeds `assets/icons/<name>.svg`.
macro_rules! embedded_icons {
    ($path:expr, $($name:literal),+ $(,)?) => {
        match $path {
            "app-icon" => Some(Cow::Borrowed(
                include_bytes!("../../../assets/app/AppIcon_1024x1024.png").as_slice(),
            )),
            $($name => Some(Cow::Borrowed(
                include_bytes!(concat!("../../../assets/icons/", $name, ".svg")).as_slice(),
            )),)+
            _ => None,
        }
    };
}
// Expands the shared project glyph table into a lookup that embeds
// `assets/icons/projects/<asset>.svg` for each row.
macro_rules! embedded_project_glyphs {
    ($($group:literal { $($variant:ident => ($asset:literal, $label:literal),)+ })+) => {
        fn project_glyph(asset: &str) -> Option<&'static [u8]> {
            match asset {
                $($($asset => Some(
                    include_bytes!(concat!("../../../assets/icons/projects/", $asset, ".svg"))
                        .as_slice(),
                ),)+)+
                _ => None,
            }
        }
    };
}
kodem_core::with_project_icons!(embedded_project_glyphs);
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(asset) = path.strip_prefix("projects/") {
            return Ok(project_glyph(asset).map(Cow::Borrowed));
        }
        Ok(embedded_icons!(
            path,
            "antigravity",
            "archive",
            "arrow-down",
            "arrow-down-to-line",
            "arrow-left",
            "arrow-right",
            "arrow-up",
            "arrow-up-from-line",
            "bell",
            "book",
            "bot",
            "branch",
            "chat",
            "check",
            "check-square",
            "chevron-down",
            "chevron-left",
            "chevron-right",
            "chevron-up",
            "chevrons-up-down",
            "circle-check",
            "circle-x",
            "claude",
            "clock",
            "close",
            "code",
            "command",
            "copy",
            "cube",
            "edit",
            "ellipsis",
            "external-link",
            "eye",
            "eye-off",
            "file",
            "file-text",
            "folder",
            "folder-closed",
            "folder-open",
            "gauge",
            "git-branch",
            "git-commit-horizontal",
            "git-merge",
            "git-pull-request",
            "github",
            "globe",
            "info",
            "keyboard",
            "kodem",
            "layers",
            "list-checks",
            "list-end",
            "list-ordered",
            "loader",
            "lock-open",
            "log-in",
            "log-out",
            "minus",
            "moon",
            "openai",
            "opencode",
            "palette",
            "panel-bottom",
            "panel-left",
            "panel-right",
            "paperclip",
            "pencil",
            "pi",
            "pilcrow",
            "pin",
            "play",
            "plus",
            "refresh",
            "refresh-cw",
            "scan-eye",
            "search",
            "settings",
            "settings-2",
            "shield",
            "ship-wheel",
            "sparkles",
            "square",
            "square-terminal",
            "star",
            "star-fill",
            "stop",
            "sun",
            "tag",
            "trash-2",
            "triangle-alert",
            "undo-2",
            "user",
            "users",
            "wrap-text",
            "wrench",
            "xai",
            "zap",
        ))
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
/// A vector icon whose color follows its surrounding control unless overridden.
#[derive(IntoElement)]
pub struct Icon {
    name: &'static str,
    style: StyleRefinement,
    rotation: f32,
}
impl Icon {
    /// Turns the icon about its center by `angle`, in radians, clockwise.
    pub fn rotate(mut self, angle: f32) -> Self {
        self.rotation = angle;
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
        let mut image = svg().path(self.name).text_color(window.text_style().color);
        image.style().refine(&self.style);
        if self.rotation != 0. {
            image = image.with_transformation(Transformation::rotate(radians(self.rotation)));
        }
        image
    }
}
/// Returns a compact vector icon using the surrounding text color.
pub fn icon(name: &'static str) -> Icon {
    Icon {
        name,
        style: StyleRefinement::default(),
        rotation: 0.,
    }
    .size(px(16.))
    .flex_shrink_0()
}
