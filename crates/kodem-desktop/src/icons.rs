//! Embedded native vector assets. No runtime asset directory is required.
use gpui::{prelude::*, *};
use std::borrow::Cow;
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "app-icon" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/app/AppIcon_1024x1024.png"
            ))),
            "wrap-text" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/wrap-text.svg"
            ))),
            "bell" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/bell.svg"
            ))),
            "chat-dither" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/app/chat-dither.svg"
            ))),
            "corner-warm" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/app/corner-warm.svg"
            ))),
            "corner-cool" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/app/corner-cool.svg"
            ))),
            "cube" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/cube.svg"
            ))),
            "book" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/book.svg"
            ))),
            "openai" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/openai.svg"
            ))),
            "lock-open" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/lock-open.svg"
            ))),
            "archive" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/archive.svg"
            ))),
            "arrow-down" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/arrow-down.svg"
            ))),
            "arrow-left" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/arrow-left.svg"
            ))),
            "arrow-right" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/arrow-right.svg"
            ))),
            "arrow-up" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/arrow-up.svg"
            ))),
            "bot" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/bot.svg"
            ))),
            "branch" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/branch.svg"
            ))),
            "chat" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/chat.svg"
            ))),
            "check-square" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/check-square.svg"
            ))),
            "check" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/check.svg"
            ))),
            "chevron-down" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/chevron-down.svg"
            ))),
            "chevron-left" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/chevron-left.svg"
            ))),
            "chevron-right" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/chevron-right.svg"
            ))),
            "chevrons-up-down" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/chevrons-up-down.svg"
            ))),
            "circle-check" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/circle-check.svg"
            ))),
            "circle-x" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/circle-x.svg"
            ))),
            "close" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/close.svg"
            ))),
            "code" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/code.svg"
            ))),
            "copy" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/copy.svg"
            ))),
            "edit" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/edit.svg"
            ))),
            "ellipsis" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/ellipsis.svg"
            ))),
            "external-link" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/external-link.svg"
            ))),
            "file-text" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/file-text.svg"
            ))),
            "file" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/file.svg"
            ))),
            "folder-closed" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/folder-closed.svg"
            ))),
            "folder-open" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/folder-open.svg"
            ))),
            "folder" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/folder.svg"
            ))),
            "keyboard" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/keyboard.svg"
            ))),
            "kodem" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/kodem.svg"
            ))),
            "minus" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/minus.svg"
            ))),
            "loader" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/loader.svg"
            ))),
            "moon" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/moon.svg"
            ))),
            "panel-bottom" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/panel-bottom.svg"
            ))),
            "panel-left" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/panel-left.svg"
            ))),
            "panel-right" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/panel-right.svg"
            ))),
            "paperclip" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/paperclip.svg"
            ))),
            "pin" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/pin.svg"
            ))),
            "plus" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/plus.svg"
            ))),
            "refresh" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/refresh.svg"
            ))),
            "search" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/search.svg"
            ))),
            "settings-2" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/settings-2.svg"
            ))),
            "settings" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/settings.svg"
            ))),
            "shield" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/shield.svg"
            ))),
            "square-terminal" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/square-terminal.svg"
            ))),
            "stop" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/stop.svg"
            ))),
            "sun" => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/sun.svg"
            ))),
            _ => None,
        })
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
