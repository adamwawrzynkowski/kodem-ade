//! Native desktop for Kodem's durable local host on macOS, Windows and Linux.
// A release build opens no console window next to the app on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod bayer;
mod bridge;
mod browser;
mod clipboard;
mod components;
mod harness;
mod icons;
mod markdown;
mod release_notes;
mod sound;
mod syntax;
mod theme;
mod thread_list;
mod tool_output;
mod ui;
mod updater;
mod wallpaper;
mod window_state;
use gpui::{prelude::*, *};
use std::borrow::Cow;
fn main() {
    // Linux's browser needs the window on X11, which a Wayland session offers through XWayland.
    #[cfg(not(any(target_os = "macos", windows)))]
    browser::prefer_x11();
    // macOS can ask this process how to present a host-sent alert while Kodem is active.
    #[cfg(target_os = "macos")]
    let _notification_presentation =
        kodem_core::notification_presentation::NotificationPresentation::install();
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(|cx| {
            if let Err(error) = cx.text_system().add_fonts(vec![
                Cow::Borrowed(include_bytes!("../../../assets/fonts/FiraSans-Regular.ttf")),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/FiraSans-Medium.ttf")),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/FiraSans-SemiBold.ttf"
                )),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/FiraSans-Bold.ttf")),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/FiraSans-Italic.ttf")),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/FiraSans-BoldItalic.ttf"
                )),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/Inter-Regular.ttf")),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/Inter-Medium.ttf")),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf")),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/Inter-Bold.ttf")),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/Inter-Italic.ttf")),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/Inter-BoldItalic.ttf")),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/IBMPlexMono-Regular.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/IBMPlexMono-Medium.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/IBMPlexMono-SemiBold.ttf"
                )),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/IBMPlexMono-Bold.ttf")),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/JetBrainsMono-Regular.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/JetBrainsMono-Medium.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/JetBrainsMono-SemiBold.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/JetBrainsMono-Bold.ttf"
                )),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/FiraCode-Regular.ttf")),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/FiraCode-Medium.ttf")),
                Cow::Borrowed(include_bytes!(
                    "../../../assets/fonts/FiraCode-SemiBold.ttf"
                )),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/FiraCode-Bold.ttf")),
            ]) {
                eprintln!("Could not load bundled fonts: {error}");
            }
            ui::init(cx);
            // Kodem ends with its last window. A windowless macOS app would stay as a
            // background activity, which keeps an update from relaunching it.
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = window_state::load(cx);
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(bounds),
                    titlebar: Some(TitlebarOptions {
                        // The taskbar and window switchers of Windows and Linux show the title.
                        title: (!cfg!(target_os = "macos"))
                            .then(|| updater::Channel::current().app_name().into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(12.), px(12.))),
                    }),
                    // The title bar moves the window from its empty areas only, so AppKit must
                    // not move it from the whole strip, the title bar's controls included.
                    app_owns_titlebar_drag: true,
                    // Linux finds the window's desktop entry and icon by this identity.
                    app_id: Some(kodem_core::platform::app_id().into()),
                    // Linux keeps the compositor's frame where it draws one.
                    window_decorations: Some(WindowDecorations::Server),
                    window_min_size: Some(size(px(850.), px(600.))),
                    // X11 shows this icon; Wayland takes it from the desktop entry.
                    #[cfg(not(any(target_os = "macos", windows)))]
                    icon: window_icon(),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| ui::Stage::new(window, cx)),
            );
            if let Err(error) = &opened {
                eprintln!("Could not open Kodem: {error}");
                cx.quit();
            }
            cx.activate(true);
        });
}

/// Decodes the bundled app icon at the size that X11 window managers show.
#[cfg(not(any(target_os = "macos", windows)))]
fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    let icon = image::load_from_memory(include_bytes!("../../../assets/app/AppIcon_1024x1024.png"))
        .ok()?;
    Some(std::sync::Arc::new(icon.thumbnail(256, 256).to_rgba8()))
}
