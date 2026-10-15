//! Plays the bundled notification sounds on platforms whose notifications cannot carry them.
use crate::model::NotificationSound;
use anyhow::Result;

/// Returns the WAV data of a bundled notification sound.
pub fn audio(sound: NotificationSound) -> &'static [u8] {
    match sound {
        NotificationSound::Chime => include_bytes!("../../../assets/sounds/kodem-chime.wav"),
        NotificationSound::Bell => include_bytes!("../../../assets/sounds/kodem-bell.wav"),
        NotificationSound::Glass => include_bytes!("../../../assets/sounds/kodem-glass.wav"),
        NotificationSound::Pulse => include_bytes!("../../../assets/sounds/kodem-pulse.wav"),
        NotificationSound::Ascend => include_bytes!("../../../assets/sounds/kodem-ascend.wav"),
    }
}

/// Starts a notification sound and returns without waiting for it. A new sound replaces one
/// that is still playing.
#[cfg(windows)]
pub fn play(sound: NotificationSound) -> Result<()> {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    // The data is static, so it outlives the asynchronous playback that reads it.
    let played = unsafe {
        PlaySoundW(
            audio(sound).as_ptr().cast(),
            std::ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        )
    };
    anyhow::ensure!(played != 0, "Windows could not play the sound");
    Ok(())
}

/// Starts a notification sound and returns without waiting for it. A new sound replaces one
/// that is still playing.
#[cfg(not(windows))]
pub fn play(sound: NotificationSound) -> Result<()> {
    use anyhow::Context;
    use std::{
        io::Write,
        process::{Child, Command, Stdio},
        sync::Mutex,
    };
    static PLAYING: Mutex<Option<(Child, tempfile::TempPath)>> = Mutex::new(None);
    // PipeWire, PulseAudio and ALSA each install their own player.
    let player = ["pw-play", "paplay", "aplay"]
        .into_iter()
        .find_map(|name| crate::platform::find_program(name, &[]))
        .context("Install PipeWire, PulseAudio or ALSA utilities to play notification sounds")?;
    let mut file = tempfile::Builder::new()
        .prefix("kodem-sound-")
        .suffix(".wav")
        .tempfile()?;
    file.write_all(audio(sound))?;
    file.flush()?;
    let path = file.into_temp_path();
    let child = Command::new(player)
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Could not start the sound player")?;
    let mut playing = PLAYING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((mut previous, _file)) = playing.replace((child, path)) {
        let _ = previous.kill();
        let _ = previous.wait();
    }
    Ok(())
}
