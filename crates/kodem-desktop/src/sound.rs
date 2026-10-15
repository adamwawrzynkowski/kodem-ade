//! Plays the bundled notification sounds in the desktop process, without a notification.
use anyhow::Result;
use kodem_core::NotificationSound;

/// Plays a notification sound now, in place of a preview that is still playing.
#[cfg(target_os = "macos")]
pub fn preview(sound: NotificationSound) -> Result<()> {
    use anyhow::{Context, ensure};
    use objc2::{AnyThread, rc::Retained};
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;
    use std::cell::RefCell;

    thread_local! {
        // The player must outlive this call, and the next preview stops it.
        static PLAYING: RefCell<Option<Retained<NSSound>>> = const { RefCell::new(None) };
    }
    // The app bundle carries the same files for Notification Center.
    let audio: &[u8] = match sound {
        NotificationSound::Chime => include_bytes!("../../../assets/sounds/kodem-chime.wav"),
        NotificationSound::Bell => include_bytes!("../../../assets/sounds/kodem-bell.wav"),
        NotificationSound::Glass => include_bytes!("../../../assets/sounds/kodem-glass.wav"),
        NotificationSound::Pulse => include_bytes!("../../../assets/sounds/kodem-pulse.wav"),
        NotificationSound::Ascend => include_bytes!("../../../assets/sounds/kodem-ascend.wav"),
    };
    let player = NSSound::initWithData(NSSound::alloc(), &NSData::with_bytes(audio))
        .context("macOS could not read the sound")?;
    PLAYING.with_borrow_mut(|playing| {
        if let Some(previous) = playing.take() {
            previous.stop();
        }
        ensure!(player.play(), "macOS could not play the sound");
        *playing = Some(player);
        Ok(())
    })
}

/// Plays a notification sound now, in place of a preview that is still playing.
#[cfg(not(target_os = "macos"))]
pub fn preview(sound: NotificationSound) -> Result<()> {
    kodem_core::sound::play(sound)
}
