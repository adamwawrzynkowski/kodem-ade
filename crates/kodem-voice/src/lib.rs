//! Voice input that runs on this computer: microphone capture and Whisper speech recognition.
//! Nothing here depends on GPUI or the network; the desktop downloads the model and runs these
//! blocking calls on background threads.
mod capture;
mod whisper;

pub use capture::{InputDevice, Recorder, input_devices};
pub use whisper::Transcriber;

/// The sample rate of the audio that the recorder keeps and the model hears.
pub const SAMPLE_RATE: usize = 16_000;
