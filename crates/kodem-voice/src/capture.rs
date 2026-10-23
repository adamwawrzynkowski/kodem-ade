//! Records a microphone as 16 kHz mono samples, with a running loudness level for a live
//! waveform.
use crate::SAMPLE_RATE;
use anyhow::{Context, Result, anyhow, bail};
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};

/// How many loudness levels the recorder keeps, newest last.
const LEVELS: usize = 256;
/// The audio that one loudness level covers: 20 milliseconds.
const LEVEL_SAMPLES: usize = SAMPLE_RATE / 50;
/// How long the recorder waits for the system to start the microphone.
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// A microphone that the system offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    /// The system's identifier of the device, which stays the same across launches.
    pub id: String,
    /// The name that the system shows for the device.
    pub name: String,
}

/// Lists the microphones that the system offers, in its order.
pub fn input_devices() -> Vec<InputDevice> {
    let Ok(devices) = cpal::default_host().input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            let name = device
                .description()
                .map(|description| description.name().to_owned())
                .unwrap_or_else(|_| id.clone());
            Some(InputDevice { id, name })
        })
        .collect()
}

/// A running recording of a microphone. Dropping it stops the microphone.
pub struct Recorder {
    shared: Arc<Shared>,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct Shared {
    samples: Mutex<Vec<f32>>,
    levels: Mutex<VecDeque<f32>>,
    failure: Mutex<Option<String>>,
    // The input volume as the bits of an f32 multiplier.
    gain: AtomicU32,
}

impl Recorder {
    /// Starts recording the microphone with the identifier `device`, or the system's default
    /// microphone when `device` is `None` or no longer connected. `gain` multiplies the signal:
    /// 1 keeps the microphone's own volume. The microphone runs on its own thread, which owns
    /// the system stream; this returns once the stream plays or fails to start.
    pub fn start(device: Option<&str>, gain: f32) -> Result<Self> {
        let shared = Arc::new(Shared::default());
        shared.gain.store(gain.to_bits(), Ordering::Relaxed);
        let (stop, stopped) = mpsc::channel::<()>();
        let (started, start) = mpsc::channel::<Result<()>>();
        let thread_shared = shared.clone();
        let device = device.map(str::to_owned);
        let thread = std::thread::Builder::new()
            .name("kodem-voice-capture".into())
            .spawn(
                move || match open_stream(device.as_deref(), thread_shared) {
                    Ok(stream) => {
                        let _ = started.send(Ok(()));
                        // The stream records until the recorder asks it to stop or goes away.
                        let _ = stopped.recv();
                        drop(stream);
                    }
                    Err(error) => {
                        let _ = started.send(Err(error));
                    }
                },
            )
            .context("Could not start the microphone thread")?;
        let mut recorder = Self {
            shared,
            stop: Some(stop),
            thread: Some(thread),
        };
        match start.recv_timeout(START_TIMEOUT) {
            Ok(Ok(())) => Ok(recorder),
            Ok(Err(error)) => {
                recorder.halt();
                Err(error)
            }
            Err(_) => {
                recorder.halt();
                bail!("The microphone did not start. Check that no other app holds it.")
            }
        }
    }

    /// Returns the loudness of the last `count` stretches of 20 milliseconds, oldest first, from
    /// 0 for silence to 1. A recording shorter than `count` stretches starts with silence.
    pub fn levels(&self, count: usize) -> Vec<f32> {
        let levels = lock(&self.shared.levels);
        let mut recent = vec![0.; count.saturating_sub(levels.len())];
        recent.extend(levels.iter().skip(levels.len().saturating_sub(count)));
        recent
    }

    /// Changes the input volume while the microphone records.
    pub fn set_gain(&self, gain: f32) {
        self.shared.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    /// Returns how long the recording runs.
    pub fn duration(&self) -> Duration {
        let samples = lock(&self.shared.samples).len();
        Duration::from_secs_f64(samples as f64 / SAMPLE_RATE as f64)
    }

    /// Returns the error that stopped the microphone while it recorded, if one did.
    pub fn failure(&self) -> Option<String> {
        lock(&self.shared.failure).clone()
    }

    /// Stops the microphone and returns the recording as 16 kHz mono samples.
    pub fn finish(mut self) -> Vec<f32> {
        self.halt();
        std::mem::take(&mut *lock(&self.shared.samples))
    }

    fn halt(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.halt();
    }
}

/// Opens and plays an input device in its own sample format.
fn open_stream(id: Option<&str>, shared: Arc<Shared>) -> Result<Stream> {
    let host = cpal::default_host();
    let chosen = id
        .and_then(|id| id.parse().ok())
        .and_then(|id| host.device_by_id(&id));
    let device = chosen
        .or_else(|| host.default_input_device())
        .context("This computer has no microphone. Connect one and try again.")?;
    let supported = device
        .default_input_config()
        .map_err(|error| anyhow!("The microphone cannot record: {error}"))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, shared),
        SampleFormat::I16 => build::<i16>(&device, &config, shared),
        SampleFormat::U16 => build::<u16>(&device, &config, shared),
        SampleFormat::I32 => build::<i32>(&device, &config, shared),
        SampleFormat::I8 => build::<i8>(&device, &config, shared),
        SampleFormat::U8 => build::<u8>(&device, &config, shared),
        SampleFormat::F64 => build::<f64>(&device, &config, shared),
        other => bail!("The microphone records {other} samples, which Kodem cannot read."),
    }?;
    stream
        .play()
        .map_err(|error| anyhow!("The microphone did not start: {error}"))?;
    Ok(stream)
}

/// Builds an input stream that mixes each frame to mono, resamples it to 16 kHz and keeps it.
fn build<T>(device: &cpal::Device, config: &StreamConfig, shared: Arc<Shared>) -> Result<Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels).max(1);
    let mut resampler = Resampler::new(config.sample_rate as f64);
    let mut level = Level::default();
    let mut mono = Vec::new();
    let errors = shared.clone();
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                let gain = f32::from_bits(shared.gain.load(Ordering::Relaxed));
                mono.clear();
                for frame in data.chunks(channels) {
                    let sum: f32 = frame.iter().map(|&sample| f32::from_sample(sample)).sum();
                    let sample = (sum / frame.len() as f32 * gain).clamp(-1., 1.);
                    resampler.push(sample, &mut mono);
                }
                for &sample in &mono {
                    if let Some(loudness) = level.push(sample) {
                        let mut levels = lock(&shared.levels);
                        if levels.len() == LEVELS {
                            levels.pop_front();
                        }
                        levels.push_back(loudness);
                    }
                }
                lock(&shared.samples).extend_from_slice(&mono);
            },
            move |error| {
                let mut failure = lock(&errors.failure);
                if failure.is_none() {
                    *failure = Some(format!("The microphone stopped: {error}"));
                }
            },
            None,
        )
        .map_err(|error| anyhow!("The microphone cannot record: {error}"))
}

/// Converts a stream to 16 kHz. Lowering the rate averages the samples that fall into each new
/// sample, which also filters out the sounds above the new rate's limit.
struct Resampler {
    // New samples per source sample.
    step: f64,
    phase: f64,
    sum: f32,
    count: u32,
    last: f32,
}

impl Resampler {
    fn new(rate: f64) -> Self {
        Self {
            step: SAMPLE_RATE as f64 / rate.max(1.),
            phase: 0.,
            sum: 0.,
            count: 0,
            last: 0.,
        }
    }

    fn push(&mut self, sample: f32, out: &mut Vec<f32>) {
        self.sum += sample;
        self.count += 1;
        self.phase += self.step;
        while self.phase >= 1. {
            self.phase -= 1.;
            if self.count > 0 {
                self.last = self.sum / self.count as f32;
                self.sum = 0.;
                self.count = 0;
            }
            // A source slower than 16 kHz repeats its sample.
            out.push(self.last);
        }
    }
}

/// Measures the loudness of each stretch of 20 milliseconds.
#[derive(Default)]
struct Level {
    squares: f32,
    count: usize,
}

impl Level {
    fn push(&mut self, sample: f32) -> Option<f32> {
        self.squares += sample * sample;
        self.count += 1;
        if self.count < LEVEL_SAMPLES {
            return None;
        }
        let rms = (self.squares / self.count as f32).sqrt();
        self.squares = 0.;
        self.count = 0;
        // Speech sits far below full scale, so the level rises with the decibels from -60 dB.
        let decibels = 20. * rms.max(1e-6).log10();
        Some(((decibels + 60.) / 60.).clamp(0., 1.))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
