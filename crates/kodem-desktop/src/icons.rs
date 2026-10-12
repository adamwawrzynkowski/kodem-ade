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
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(embedded_icons!(
            path,
            "archive",
            "arrow-down",
            "arrow-left",
            "arrow-right",
            "arrow-up",
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
            "chevrons-up-down",
            "circle-check",
            "circle-x",
            "claude",
            "close",
            "code",
            "command",
            "copy",
            "cube",
            "edit",
            "ellipsis",
            "external-link",
            "eye",
            "file",
            "file-text",
            "folder",
            "folder-closed",
            "folder-open",
            "gauge",
            "git-pull-request",
            "globe",
            "info",
            "keyboard",
            "kodem",
            "layers",
            "list-checks",
            "loader",
            "lock-open",
            "log-in",
            "log-out",
            "minus",
            "moon",
            "openai",
            "palette",
            "panel-bottom",
            "panel-left",
            "panel-right",
            "paperclip",
            "pencil",
            "pin",
            "plus",
            "refresh",
            "search",
            "settings",
            "settings-2",
            "shield",
            "sparkles",
            "square-terminal",
            "star",
            "star-fill",
            "stop",
            "sun",
            "trash-2",
            "undo-2",
            "user",
            "users",
            "wrap-text",
            "wrench",
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
        image
    }
}
/// Returns a compact vector icon using the surrounding text color.
pub fn icon(name: &'static str) -> Icon {
    Icon {
        name,
        style: StyleRefinement::default(),
    }
    .size(px(16.))
    .flex_shrink_0()
}
