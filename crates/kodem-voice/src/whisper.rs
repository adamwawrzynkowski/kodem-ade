//! Whisper speech recognition with whisper.cpp. It runs on the GPU through Metal on macOS and
//! through Vulkan on Linux and Windows on x64 when a graphics driver offers it, and on the CPU
//! otherwise.
use crate::SAMPLE_RATE;
use anyhow::{Context, Result, anyhow, ensure};
use std::{
    ffi::c_void,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

/// The loudness of speech: a stretch of 20 milliseconds above -44 dBFS. Whisper writes words
/// into silence, and the turbo model's own silence probability does not catch it.
const SPEECH_LEVEL: f32 = 0.006;
const LEVEL_STRETCH: usize = SAMPLE_RATE / 50;
/// A recording holds speech when 200 milliseconds of it are as loud as speech.
const SPEECH_STRETCHES: usize = 10;
/// Whisper needs at least a second of audio; a shorter recording is padded with silence.
const MIN_SAMPLES: usize = SAMPLE_RATE + SAMPLE_RATE / 10;
/// The most CPU threads that a transcription uses. More threads than this gain little.
const MAX_THREADS: usize = 8;

/// A loaded Whisper model that turns 16 kHz mono speech into text, in the language it hears.
pub struct Transcriber {
    state: WhisperState,
}

impl Transcriber {
    /// Loads a multilingual whisper.cpp model file onto the GPU when this computer has one that
    /// whisper.cpp can use. A first pass over silence prepares the GPU's programs, so that the
    /// first recording returns as fast as the next.
    pub fn load(path: &Path) -> Result<Self> {
        ensure_gpu_runtime()?;
        // whisper.cpp writes its progress to the console unless a hook takes it.
        whisper_rs::install_logging_hooks();
        let path = path
            .to_str()
            .context("The speech model's path is not valid text")?;
        let context = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|error| anyhow!("Could not load the speech model: {error}"))?;
        ensure!(
            context.is_multilingual(),
            "The speech model hears only English. Kodem needs a multilingual model."
        );
        let state = context
            .create_state()
            .map_err(|error| anyhow!("Could not prepare the speech model: {error}"))?;
        let mut transcriber = Self { state };
        if gpu_present() {
            transcriber.run(&[0.; MIN_SAMPLES], &AtomicBool::new(false))?;
        }
        Ok(transcriber)
    }

    /// Transcribes 16 kHz mono speech of any length. Silence returns an empty text. Setting
    /// `cancel` stops a running transcription with an error.
    pub fn transcribe(&mut self, samples: &[f32], cancel: &AtomicBool) -> Result<String> {
        if !speaks(samples) {
            return Ok(String::new());
        }
        if samples.len() >= MIN_SAMPLES {
            return self.run(samples, cancel);
        }
        let mut padded = samples.to_vec();
        padded.resize(MIN_SAMPLES, 0.);
        self.run(&padded, cancel)
    }

    /// Runs the model over the samples and joins the text of its segments.
    fn run(&mut self, samples: &[f32], cancel: &AtomicBool) -> Result<String> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let threads = std::thread::available_parallelism().map_or(4, |count| count.get());
        params.set_n_threads(threads.min(MAX_THREADS) as i32);
        params.set_language(Some("auto"));
        // Each recording stands alone, and its text holds no captions such as "[Music]".
        params.set_no_context(true);
        params.set_suppress_nst(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        // whisper-rs's closure form of this callback reads its closure as the wrong type, so the
        // flag travels as raw user data.
        // SAFETY: the callback only reads the flag, which outlives this call to `full`.
        unsafe {
            params.set_abort_callback(Some(should_stop));
            params.set_abort_callback_user_data(cancel as *const AtomicBool as *mut c_void);
        }
        self.state
            .full(params, samples)
            .map_err(|error| anyhow!("Could not transcribe the recording: {error}"))?;
        let mut text = String::new();
        for segment in self.state.as_iter() {
            if let Ok(words) = segment.to_str_lossy() {
                text.push_str(&words);
            }
        }
        Ok(text.trim().to_owned())
    }
}

/// Tells whisper.cpp to stop once the flag behind `flag` is set.
unsafe extern "C" fn should_stop(flag: *mut c_void) -> bool {
    // SAFETY: `run` passes a pointer to an AtomicBool that outlives the transcription.
    unsafe { (*(flag as *const AtomicBool)).load(Ordering::Relaxed) }
}

/// Reports whether a recording holds enough loud audio to be speech.
fn speaks(samples: &[f32]) -> bool {
    let threshold = SPEECH_LEVEL * SPEECH_LEVEL * LEVEL_STRETCH as f32;
    samples
        .chunks(LEVEL_STRETCH)
        .filter(|stretch| stretch.iter().map(|s| s * s).sum::<f32>() >= threshold)
        .nth(SPEECH_STRETCHES - 1)
        .is_some()
}

/// Reports whether whisper.cpp runs on a GPU here: whether ggml registered a GPU, as whisper.cpp
/// checks when it picks one. The registry leaves out a Vulkan backend that could not start.
/// whisper-rs's Vulkan device list starts Vulkan again instead, and on a driver without Vulkan
/// 1.2 that throws a C++ exception, which ends the process.
fn gpu_present() -> bool {
    use whisper_rs::whisper_rs_sys::{
        ggml_backend_dev_count, ggml_backend_dev_get, ggml_backend_dev_type,
        ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_GPU as GPU,
        ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_IGPU as IGPU,
    };
    // SAFETY: the registry is built once with every backend's errors caught, and each index
    // below the count names one of its devices.
    unsafe {
        (0..ggml_backend_dev_count()).any(|index| {
            matches!(
                ggml_backend_dev_type(ggml_backend_dev_get(index)),
                GPU | IGPU
            )
        })
    }
}

/// Checks that Windows has the Vulkan loader, which every graphics driver with Vulkan installs.
/// Kodem loads it only when whisper.cpp first calls it, so that Kodem starts without it; a call
/// without it would end the process.
fn ensure_gpu_runtime() -> Result<()> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        use windows::{
            Win32::System::LibraryLoader::{LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
            core::w,
        };
        // SAFETY: the name is a constant, and loading the loader runs no code of Kodem's.
        let loaded =
            unsafe { LoadLibraryExW(w!("vulkan-1.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) };
        ensure!(
            loaded.is_ok(),
            "Voice input needs a graphics driver with Vulkan. Update the graphics driver of this PC and try again."
        );
    }
    Ok(())
}
