//! Voice input's speech model: where it lives, its verified download, and the worker thread that
//! loads it and transcribes recordings. Nothing here touches the UI; it reports through events.
use anyhow::{Context, Result, bail, ensure};
use kodem_core::platform;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// The speech model as the interface names it.
pub const MODEL_NAME: &str = "Whisper large-v3 turbo";
/// The size of the model file, in bytes.
pub const MODEL_SIZE: u64 = 574_041_195;
const MODEL_FILE: &str = "ggml-large-v3-turbo-q5_0.bin";
// A fixed revision of whisper.cpp's model repository, so the file matches its digest.
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin";
const MODEL_SHA256: &str = "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2";
/// The worker frees the loaded model, about a gigabyte, after this long without a recording.
const IDLE_UNLOAD: Duration = Duration::from_secs(15 * 60);

/// What the download and the worker report to the window.
pub enum Event {
    Downloading {
        received: u64,
        total: u64,
    },
    Downloaded,
    /// The worker holds the model and transcribes without loading it first.
    Loaded,
    Transcribed(String),
    Failed(String),
}

/// Returns where the speech model lives. Every build of Kodem on this computer shares it.
pub fn model_path() -> Result<PathBuf> {
    Ok(platform::data_home()?
        .join("Kodem")
        .join("models")
        .join(MODEL_FILE))
}

/// Reports whether the speech model is downloaded. A download writes the file only once its
/// digest matches, so a file of the right size is the verified model.
pub fn model_ready() -> bool {
    model_path()
        .ok()
        .and_then(|path| fs::metadata(path).ok())
        .is_some_and(|meta| meta.is_file() && meta.len() == MODEL_SIZE)
}

/// Removes the downloaded speech model.
pub fn remove_model() -> Result<()> {
    let path = model_path()?;
    if path.exists() {
        fs::remove_file(&path).context("Could not remove the speech model")?;
    }
    Ok(())
}

/// Downloads the speech model with the system's curl, reports its progress, and checks its size
/// and digest before it takes the model's place. Setting `cancel` stops it.
pub fn download(progress: &mpsc::Sender<Event>, cancel: &AtomicBool) -> Result<()> {
    let path = model_path()?;
    let folder = path.parent().context("Missing model folder")?;
    fs::create_dir_all(folder).context("Could not create the model folder")?;
    let partial = path.with_extension("part");
    let mut file = File::create(&partial).context("Could not write the speech model")?;
    let mut child = platform::hide_console(&mut Command::new(crate::updater::curl_program()))
        .args([
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "15",
            // A download that stalls for a minute fails instead of waiting forever.
            "--speed-limit",
            "1",
            "--speed-time",
            "60",
            "--user-agent",
            "Kodem",
            MODEL_URL,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Could not start curl to download the speech model")?;
    let result = (|| -> Result<()> {
        let mut output = child.stdout.take().context("Download stream unavailable")?;
        let mut hash = Sha256::new();
        let mut received = 0;
        let mut last = Instant::now();
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("Download cancelled");
            }
            let count = output.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            received += count as u64;
            ensure!(
                received <= MODEL_SIZE,
                "The downloaded speech model is larger than expected"
            );
            file.write_all(&buffer[..count])?;
            hash.update(&buffer[..count]);
            if last.elapsed() >= Duration::from_millis(100) {
                let _ = progress.send(Event::Downloading {
                    received,
                    total: MODEL_SIZE,
                });
                last = Instant::now();
            }
        }
        ensure!(
            child.wait()?.success(),
            "The speech model did not download. Check your connection and try again."
        );
        ensure!(
            received == MODEL_SIZE
                && format!("{:x}", hash.finalize()).eq_ignore_ascii_case(MODEL_SHA256),
            "The downloaded speech model is damaged. Try again."
        );
        file.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        drop(file);
        let _ = fs::remove_file(&partial);
        return result;
    }
    drop(file);
    fs::rename(&partial, &path).context("Could not save the speech model")?;
    Ok(())
}

/// Starts the download on its own thread and returns the flag that cancels it.
pub fn start_download(events: mpsc::Sender<Event>) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        let event = match download(&events, &flag) {
            Ok(()) => Event::Downloaded,
            // The window already left the download.
            Err(_) if flag.load(Ordering::Relaxed) => return,
            Err(error) => Event::Failed(format!("{error:#}")),
        };
        let _ = events.send(event);
    });
    cancel
}

enum Job {
    Load,
    Transcribe(Vec<f32>),
}

/// The thread that owns the one loaded model, so loading and transcribing never block the
/// window. Jobs run in order: a transcription that follows a load uses the model that the load
/// brought, and a loaded model is never loaded again.
pub struct Engine {
    jobs: Option<mpsc::Sender<Job>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Starts the worker thread. It reports to `events` and ends when the engine is dropped.
    pub fn new(events: mpsc::Sender<Event>) -> Self {
        let (jobs, queue) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("kodem-voice".into())
            .spawn(move || work(queue, events, &flag))
            .ok();
        Self {
            jobs: Some(jobs),
            stop,
            thread,
        }
    }

    /// Loads the model unless the worker holds it, and reports [`Event::Loaded`].
    pub fn load(&self) {
        self.send(Job::Load);
    }

    /// Transcribes a recording of 16 kHz mono samples, loading the model first if needed.
    pub fn transcribe(&self, samples: Vec<f32>) {
        self.send(Job::Transcribe(samples));
    }

    fn send(&self, job: Job) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }
}

impl Drop for Engine {
    /// Frees the model before the window or the app goes: a running transcription stops, the
    /// worker skips the jobs that wait, and this returns once it has ended. A load that runs
    /// finishes first, which takes a second or two.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        drop(self.jobs.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// Runs the jobs in order and frees the model after a while without one, or once `stop` is set.
fn work(queue: mpsc::Receiver<Job>, events: mpsc::Sender<Event>, stop: &AtomicBool) {
    let mut transcriber: Option<kodem_voice::Transcriber> = None;
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let job = match queue.recv_timeout(IDLE_UNLOAD) {
            Ok(job) => job,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                transcriber = None;
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if transcriber.is_none() {
            match model_path().and_then(|path| kodem_voice::Transcriber::load(&path)) {
                Ok(loaded) => transcriber = Some(loaded),
                Err(error) => {
                    let _ = events.send(Event::Failed(format!("{error:#}")));
                    continue;
                }
            }
        }
        let Some(model) = transcriber.as_mut() else {
            continue;
        };
        let event = match job {
            Job::Load => Event::Loaded,
            Job::Transcribe(samples) => match model.transcribe(&samples, stop) {
                Ok(text) => Event::Transcribed(text),
                Err(error) => Event::Failed(format!("{error:#}")),
            },
        };
        let _ = events.send(event);
    }
}
