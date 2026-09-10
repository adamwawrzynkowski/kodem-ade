//! Native macOS desktop for Kodem's durable local host.
mod bridge;
mod clipboard;
mod components;
mod icons;
mod markdown;
mod syntax;
mod theme;
mod tool_output;
mod ui;
mod window_state;
use gpui::{prelude::*, *};
use std::borrow::Cow;
fn main() {
    // Foreground presentation is handled by the desktop process, even for host-sent alerts.
    #[cfg(target_os = "macos")]
    let _notification_presentation =
        kodem_core::notification_presentation::NotificationPresentation::install();
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(|cx| {
            if let Err(error) = cx.text_system().add_fonts(vec![
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/IBMPlexSans-Regular.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/IBMPlexSans-Medium.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/IBMPlexSans-SemiBold.ttf"
                )),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/JetBrainsMono.ttf")),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/Manrope-ExtraBold.ttf"
                )),
            ]) {
                eprintln!("Could not load bundled fonts: {error}");
            }
            ui::init(cx);
            let bounds = window_state::load(cx);
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(bounds),
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(12.), px(12.))),
                    }),
                    window_min_size: Some(size(px(850.), px(600.))),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| ui::Shell::new(window, cx)),
            );
            if let Err(error) = &opened {
                eprintln!("Could not open Kodem: {error}");
                cx.quit();
            }
            cx.activate(true);
        });
}
