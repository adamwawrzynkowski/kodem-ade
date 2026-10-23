//! Plays the bundled notification sounds in the desktop process, without a notification.
use anyhow::Result;
use kodem_core::NotificationSound;

/// The sound that plays with an error notice.
const ERROR: &[u8] = include_bytes!("../../../assets/sounds/kodem-error.wav");

/// Plays a notification sound now, in place of a preview that is still playing.
pub fn preview(sound: NotificationSound) -> Result<()> {
    play(audio(sound))
}

// Returns the WAV data of a notification sound. The app bundle carries the same files for
// Notification Center.
#[cfg(target_os = "macos")]
fn audio(sound: NotificationSound) -> &'static [u8] {
    match sound {
        NotificationSound::Chime => include_bytes!("../../../assets/sounds/kodem-chime.wav"),
        NotificationSound::Bell => include_bytes!("../../../assets/sounds/kodem-bell.wav"),
        NotificationSound::Glass => include_bytes!("../../../assets/sounds/kodem-glass.wav"),
        NotificationSound::Pulse => include_bytes!("../../../assets/sounds/kodem-pulse.wav"),
        NotificationSound::Ascend => include_bytes!("../../../assets/sounds/kodem-ascend.wav"),
    }
}

// Returns the WAV data of a notification sound.
#[cfg(not(target_os = "macos"))]
fn audio(sound: NotificationSound) -> &'static [u8] {
    kodem_core::sound::audio(sound)
}

/// Plays the error sound now, in place of a sound that is still playing.
pub fn error() -> Result<()> {
    play(ERROR)
}

// Starts a bundled WAV sound and keeps its player until the next sound replaces it.
#[cfg(target_os = "macos")]
fn play(audio: &'static [u8]) -> Result<()> {
    use anyhow::{Context, ensure};
    use objc2::{AnyThread, rc::Retained};
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;
    use std::cell::RefCell;

    thread_local! {
        // The player must outlive this call, and the next sound stops it.
        static PLAYING: RefCell<Option<Retained<NSSound>>> = const { RefCell::new(None) };
    }
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

// Starts a bundled WAV sound, which replaces a sound that is still playing.
#[cfg(not(target_os = "macos"))]
fn play(audio: &'static [u8]) -> Result<()> {
    kodem_core::sound::play_audio(audio)
}
