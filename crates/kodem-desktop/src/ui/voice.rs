//! Voice input in the composer: the microphone key and its shortcut, the offer to download the
//! speech model, the download, the recording with its live waveform, and the transcript that
//! lands in the draft. The Voice page of Preferences chooses the microphone and its volume.
use super::settings::{SLIDER_COLUMN, SettingsPage, setting_group};
use super::*;
use crate::voice::{self as engine, Event};
use std::{
    f32::consts::TAU,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// The longest recording. A recording that reaches it stops and is transcribed.
const MAX_RECORDING: Duration = Duration::from_secs(5 * 60);
/// How often the composer redraws the waveform while the microphone records.
const WAVE_FRAME: Duration = Duration::from_millis(40);
/// How often the composer checks the download and the worker otherwise.
const POLL: Duration = Duration::from_millis(100);
/// The most levels that a waveform shows.
const WAVE_LEVELS: usize = 200;
/// Width of one bar of the waveform and the space after it, in points.
const WAVE_BAR: f32 = 2.;
const WAVE_GAP: f32 = 2.;
/// The levels of the last half second, which the Voice page's meter shows the loudest of.
const METER_LEVELS: usize = 25;
/// A recording of only zeros: the system gives Kodem no sound from the microphone.
#[cfg(target_os = "macos")]
const SILENT_MICROPHONE: &str = "Kodem hears nothing from the microphone. Allow Kodem in System Settings › Privacy & Security › Microphone, then try again.";
#[cfg(not(target_os = "macos"))]
const SILENT_MICROPHONE: &str =
    "Kodem hears nothing from the microphone. Check the microphone in Settings › Voice.";

/// Where voice input is.
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    /// The composer offers to download the speech model.
    Offer,
    Downloading {
        received: u64,
        total: u64,
    },
    /// The model loads after its download; the microphone starts once it is ready.
    Loading,
    Recording,
    /// The draft waits for the transcript.
    Processing,
}

/// The state of voice input: the microphone, the worker that holds the model, and the meter of
/// the Voice page.
pub(super) struct VoiceInput {
    phase: Phase,
    recorder: Option<kodem_voice::Recorder>,
    engine: Option<engine::Engine>,
    events: mpsc::Sender<Event>,
    received: mpsc::Receiver<Event>,
    /// The worker holds the model, so a transcription starts without loading it.
    loaded: bool,
    /// Cancels the running download.
    download: Option<Arc<AtomicBool>>,
    /// The download started from the composer, so the microphone starts once it is done.
    record_after_download: bool,
    /// A loop drains the events and redraws while voice input works.
    polling: bool,
    /// The microphone of the Voice page's level meter, while it runs.
    test: Option<kodem_voice::Recorder>,
    /// The microphones that the Voice page lists, read when the page opens.
    devices: Vec<kodem_voice::InputDevice>,
}

impl Default for VoiceInput {
    fn default() -> Self {
        let (events, received) = mpsc::channel();
        Self {
            phase: Phase::Idle,
            recorder: None,
            engine: None,
            events,
            received,
            loaded: false,
            download: None,
            record_after_download: false,
            polling: false,
            test: None,
            devices: Vec::new(),
        }
    }
}

impl VoiceInput {
    /// Reports whether the draft waits for a transcript.
    pub(super) fn processing(&self) -> bool {
        self.phase == Phase::Processing
    }

    /// Stops the microphones, cancels a download and frees the model, before the app quits.
    pub(super) fn shutdown(&mut self) {
        self.recorder = None;
        self.test = None;
        if let Some(cancel) = self.download.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.engine = None;
    }

    /// Reads the microphones that the system offers, for the Voice page.
    pub(super) fn refresh_devices(&mut self) {
        self.devices = kodem_voice::input_devices();
    }

    // Reports whether voice input works on something that the window must follow.
    fn busy(&self) -> bool {
        !matches!(self.phase, Phase::Idle | Phase::Offer) || self.test.is_some()
    }
}

impl Shell {
    /// Starts voice input, or stops the recording and transcribes it. Without the speech model,
    /// the composer first offers to download it; a second press closes the offer.
    pub(super) fn toggle_voice(&mut self, cx: &mut Context<Self>) {
        match self.voice.phase {
            Phase::Idle if engine::model_ready() => self.start_recording(cx),
            Phase::Idle => self.voice.phase = Phase::Offer,
            Phase::Offer => self.voice.phase = Phase::Idle,
            Phase::Recording => self.finish_recording(cx),
            Phase::Downloading { .. } | Phase::Loading | Phase::Processing => {}
        }
        cx.notify();
    }

    /// Leaves voice input without a transcript: it discards a recording, closes the offer and
    /// cancels a download. Returns whether there was anything to leave.
    pub(super) fn cancel_voice(&mut self, cx: &mut Context<Self>) -> bool {
        match self.voice.phase {
            Phase::Offer | Phase::Recording | Phase::Downloading { .. } | Phase::Loading => {
                if let Some(cancel) = self.voice.download.take() {
                    cancel.store(true, Ordering::Relaxed);
                }
                self.voice.recorder = None;
                self.voice.phase = Phase::Idle;
                cx.notify();
                true
            }
            Phase::Idle | Phase::Processing => false,
        }
    }

    // Starts the download that the user accepted. With `record`, the microphone starts once the
    // model is downloaded and loaded.
    fn download_model(&mut self, record: bool, cx: &mut Context<Self>) {
        self.voice.record_after_download = record;
        self.voice.phase = Phase::Downloading {
            received: 0,
            total: engine::MODEL_SIZE,
        };
        self.voice.download = Some(engine::start_download(self.voice.events.clone()));
        self.poll_voice(cx);
        cx.notify();
    }

    // Starts the microphone. The model loads while the user speaks, unless the worker holds it.
    fn start_recording(&mut self, cx: &mut Context<Self>) {
        // The Voice page's meter holds the microphone too.
        self.voice.test = None;
        let voice = &self.settings.voice;
        match kodem_voice::Recorder::start(voice.microphone.as_deref(), voice.gain()) {
            Ok(recorder) => {
                self.voice.recorder = Some(recorder);
                self.voice.phase = Phase::Recording;
                if !self.voice.loaded {
                    self.voice_engine().load();
                }
                self.poll_voice(cx);
            }
            Err(error) => {
                self.voice.phase = Phase::Idle;
                self.report_error(format!("{error:#}"), cx);
            }
        }
        cx.notify();
    }

    // Stops the microphone and hands the recording to the worker.
    fn finish_recording(&mut self, cx: &mut Context<Self>) {
        let Some(recorder) = self.voice.recorder.take() else {
            return;
        };
        let samples = recorder.finish();
        if !samples.is_empty() && samples.iter().all(|sample| *sample == 0.) {
            self.voice.phase = Phase::Idle;
            self.report_error(SILENT_MICROPHONE, cx);
            return;
        }
        self.voice.phase = Phase::Processing;
        self.sync_placeholder(cx);
        self.voice_engine().transcribe(samples);
        cx.notify();
    }

    // Returns the worker, starting it on first use.
    fn voice_engine(&mut self) -> &engine::Engine {
        let events = self.voice.events.clone();
        self.voice
            .engine
            .get_or_insert_with(|| engine::Engine::new(events))
    }

    // Keeps a loop that drains the events of the download and the worker, stops a recording that
    // fails or runs too long, and redraws the waveform. It ends when voice input rests.
    fn poll_voice(&mut self, cx: &mut Context<Self>) {
        if self.voice.polling {
            return;
        }
        self.voice.polling = true;
        cx.spawn(async move |this, cx| {
            loop {
                let delay = this.update(cx, |s, _| s.voice_frame()).unwrap_or(POLL);
                cx.background_executor().timer(delay).await;
                let busy = this.update(cx, |s, cx| {
                    let changed = s.receive_voice_events(cx);
                    s.check_microphones(cx);
                    if changed || s.voice_frame() == WAVE_FRAME {
                        cx.notify();
                    }
                    s.voice.polling = s.voice.busy();
                    s.voice.polling
                });
                if !matches!(busy, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    // Returns how soon the next check is due: a frame of the waveform while a microphone records.
    fn voice_frame(&self) -> Duration {
        if self.voice.recorder.is_some() || self.voice.test.is_some() {
            WAVE_FRAME
        } else {
            POLL
        }
    }

    // Applies what the download and the worker reported. Returns whether anything changed.
    fn receive_voice_events(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        while let Ok(event) = self.voice.received.try_recv() {
            changed = true;
            match event {
                Event::Downloading { received, total } => {
                    if matches!(self.voice.phase, Phase::Downloading { .. }) {
                        self.voice.phase = Phase::Downloading { received, total };
                    }
                }
                Event::Downloaded => {
                    self.voice.download = None;
                    if !matches!(self.voice.phase, Phase::Downloading { .. }) {
                        continue;
                    }
                    if self.voice.record_after_download {
                        self.voice.phase = Phase::Loading;
                        self.voice_engine().load();
                    } else {
                        self.voice.phase = Phase::Idle;
                        self.notify_user("Speech model downloaded", self.theme.accent, cx);
                    }
                }
                Event::Loaded => {
                    self.voice.loaded = true;
                    if self.voice.phase == Phase::Loading {
                        self.start_recording(cx);
                    }
                }
                Event::Transcribed(text) => {
                    self.voice.loaded = true;
                    if self.voice.phase == Phase::Processing {
                        self.voice.phase = Phase::Idle;
                        self.sync_placeholder(cx);
                        if text.is_empty() {
                            self.notify_user("No speech heard", self.theme.subtle, cx);
                        } else {
                            self.input
                                .update(cx, |input, cx| input.insert_words(&text, cx));
                        }
                    }
                }
                Event::Failed(error) => {
                    self.voice.download = None;
                    self.voice.recorder = None;
                    if self.voice.phase != Phase::Idle {
                        self.voice.phase = Phase::Idle;
                        self.sync_placeholder(cx);
                    }
                    self.report_error(error, cx);
                }
            }
        }
        changed
    }

    // Stops a recording whose microphone failed or that reached its longest length, and the
    // Voice page's meter once that page is no longer open.
    fn check_microphones(&mut self, cx: &mut Context<Self>) {
        if let Some(recorder) = &self.voice.recorder {
            if let Some(failure) = recorder.failure() {
                self.voice.recorder = None;
                self.voice.phase = Phase::Idle;
                self.report_error(failure, cx);
            } else if recorder.duration() >= MAX_RECORDING {
                self.finish_recording(cx);
            }
        }
        let page_open = self.panel == Panel::Settings && self.settings_page == SettingsPage::Voice;
        if let Some(test) = &self.voice.test
            && (!page_open || test.failure().is_some())
        {
            let failure = test.failure();
            self.voice.test = None;
            if let Some(failure) = failure {
                self.report_error(failure, cx);
            }
            cx.notify();
        }
    }

    /// Returns the composer's microphone key: a ghost key at rest, a red glass key while it
    /// records, and a spinning key while the model downloads, loads or transcribes.
    pub(super) fn voice_key(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let working = match self.voice.phase {
            Phase::Downloading { received, total } => Some(format!(
                "Downloading the speech model · {}%",
                percent(received, total)
            )),
            Phase::Loading => Some("Loading the speech model".to_owned()),
            Phase::Processing => Some("Transcribing on this computer".to_owned()),
            Phase::Idle | Phase::Offer | Phase::Recording => None,
        };
        if let Some(hint) = working {
            return div()
                .id("composer-voice")
                .relative()
                .overflow_hidden()
                .w(px(28.))
                .h(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .border_1()
                .border_color(gpui::transparent_black())
                .layer(kit::glass_body(t.info))
                .shadow(kit::glass_glow(t.info, t, kit::Pose::Rest))
                .child(dither::orbit(t.on_accent, cx.reduce_motion()))
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: hint.clone(),
                        theme: t,
                    })
                    .into()
                })
                .into_any_element();
        }
        let key = if self.voice.phase == Phase::Recording {
            const LABEL: &str = "Stop and transcribe · ⇧⌘Space";
            self.glass("composer-voice", "mic", "", t.removed)
                .accessibility_label(kit::shortcut(LABEL))
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: LABEL.into(),
                        theme: t,
                    })
                    .into()
                })
        } else {
            self.icon_button("composer-voice", "mic", "Voice input · ⇧⌘Space")
                .selected(self.voice.phase == Phase::Offer)
        };
        key.w(px(28.))
            .h(px(28.))
            .px_0()
            .on_click(cx.listener(|s, _, _, cx| s.toggle_voice(cx)))
            .into_any_element()
    }

    /// Returns the composer's voice row under the draft: the offer to download the model, the
    /// download, the recording with its waveform, or "Processing…" beside a draft with text.
    /// An empty draft shows "Processing…" as its hint instead.
    pub(super) fn voice_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.theme;
        let row = div()
            .mx(px(8.))
            .mb(px(6.))
            .px(px(10.))
            .min_h(px(34.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .border_1()
            .border_color(t.edge)
            .bg(t.inset);
        let row = match self.voice.phase {
            Phase::Idle => return None,
            Phase::Processing if self.input.read(cx).is_empty() => return None,
            Phase::Offer => row
                .py(px(8.))
                .child(icon("mic").size(px(14.)).text_color(t.accent))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(text_px(12.))
                        .line_height(relative(1.4))
                        .text_color(t.muted)
                        .child(format!(
                            "Voice input runs {}, a speech model for 99 languages, on this computer, so your voice never leaves it. It is a one-time download of {}.",
                            engine::MODEL_NAME,
                            megabytes(engine::MODEL_SIZE)
                        )),
                )
                .child(
                    self.button("voice-download", "arrow-down", "Download")
                        .flex_shrink_0()
                        .on_click(cx.listener(|s, _, _, cx| s.download_model(true, cx))),
                )
                .child(
                    self.ghost("voice-decline", "close", "Not now")
                        .flex_shrink_0()
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.cancel_voice(cx);
                        })),
                ),
            Phase::Downloading { received, total } => row
                .child(caption("Downloading speech model", t))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(40.))
                        .h(px(3.))
                        .child(dither::DitherBar::meter(
                            received as f32 / total.max(1) as f32,
                            t.accent,
                            t.accent.opacity(0.25),
                        )),
                )
                .child(figure(
                    format!("{} / {}", megabytes(received), megabytes(total)),
                    t,
                ))
                .child(
                    self.icon_button("voice-cancel-download", "close", "Cancel download · Esc")
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.cancel_voice(cx);
                        })),
                ),
            Phase::Loading => row
                .child(div().size(px(14.)).child(dither::orbit(t.accent, cx.reduce_motion())))
                .child(caption("Loading speech model", t)),
            Phase::Recording => {
                let (levels, elapsed) = self
                    .voice
                    .recorder
                    .as_ref()
                    .map(|recorder| (recorder.levels(WAVE_LEVELS), recorder.duration()))
                    .unwrap_or_default();
                let glow = if cx.reduce_motion() {
                    1.
                } else {
                    0.6 + 0.4 * (dither::cycle(Duration::from_millis(1200)) * TAU).cos()
                };
                row.child(
                    div()
                        .size(px(8.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(t.removed.opacity(glow)),
                )
                .child(figure(clock(elapsed), t))
                .child(waveform(levels, t.accent).flex_1().min_w(px(40.)).h(px(22.)))
                .when(!self.voice.loaded, |row| row.child(caption("Loading model", t)))
                .child(
                    self.icon_button("voice-discard", "close", "Discard recording · Esc")
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.cancel_voice(cx);
                        })),
                )
                .child(
                    self.icon_button("voice-finish", "check", "Transcribe · ⇧⌘Space")
                        .text_color(t.accent)
                        .on_click(cx.listener(|s, _, _, cx| s.finish_recording(cx))),
                )
            }
            Phase::Processing => row
                .child(div().size(px(14.)).child(dither::orbit(t.accent, cx.reduce_motion())))
                .child(
                    div()
                        .text_size(text_px(12.))
                        .text_color(t.muted)
                        .child("Processing…"),
                ),
        };
        Some(row.into_any_element())
    }

    /// Builds the Voice page: the microphone, its volume with a live level meter, and the
    /// speech model.
    pub(super) fn voice_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let voice = &self.settings.voice;
        let connected = voice
            .microphone
            .as_ref()
            .is_none_or(|id| self.voice.devices.iter().any(|device| &device.id == id));
        let mut microphones = div().flex().flex_col().gap(px(2.)).pt(px(6.)).child(
            self.choice_row("voice-microphone-default", voice.microphone.is_none())
                .child(icon("mic").size(px(13.)))
                .child("System default")
                .on_click(cx.listener(|s, _, _, cx| s.choose_microphone(None, cx))),
        );
        for (index, device) in self.voice.devices.iter().enumerate() {
            let id = device.id.clone();
            microphones = microphones.child(
                self.choice_row(
                    ("voice-microphone", index),
                    voice.microphone.as_ref() == Some(&device.id),
                )
                .child(icon("mic").size(px(13.)))
                .child(div().min_w_0().text_ellipsis().child(device.name.clone()))
                .on_click(
                    cx.listener(move |s, _, _, cx| s.choose_microphone(Some(id.clone()), cx)),
                ),
            );
        }
        if !connected {
            microphones = microphones.child(
                self.choice_row("voice-microphone-missing", true)
                    .child(icon("triangle-alert").size(px(13.)))
                    .child("Chosen microphone is not connected. The system default records."),
            );
        }
        let (quietest, loudest) = VoiceSettings::INPUT_GAIN_RANGE;
        let gain = (voice.gain() * 100.).round();
        let level = self
            .voice
            .test
            .as_ref()
            .map(|test| test.levels(METER_LEVELS).into_iter().fold(0f32, f32::max))
            .unwrap_or(0.);
        let testing = self.voice.test.is_some();
        let meter = div()
            .w(px(SLIDER_COLUMN))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().flex_1().h(px(6.)).child(dither::DitherBar::meter(
                level,
                if level > 0.95 { t.removed } else { t.accent },
                t.edge,
            )))
            .child(
                self.button(
                    "voice-test",
                    if testing { "stop" } else { "mic" },
                    if testing { "Stop" } else { "Test" },
                )
                .on_click(cx.listener(|s, _, _, cx| s.toggle_level_test(cx))),
            );
        let model = if let Phase::Downloading { received, total } = self.voice.phase {
            figure(format!("{}%", percent(received, total)), t).into_any_element()
        } else if engine::model_ready() {
            self.tinted("voice-remove-model", "trash-2", "Remove", t.removed)
                .disabled(self.voice.busy())
                .on_click(cx.listener(|s, _, _, cx| s.remove_speech_model(cx)))
                .into_any_element()
        } else {
            self.button("voice-download-model", "arrow-down", "Download")
                .on_click(cx.listener(|s, _, _, cx| s.download_model(false, cx)))
                .into_any_element()
        };
        page_body()
            .child(
                setting_group("Microphone", None, t)
                    .child(microphones)
                    .child(div().h(px(6.)))
                    .child(
                        self.setting_row(
                            "Refresh",
                            "Read the microphones again after connecting one.",
                            self.button("voice-refresh", "refresh", "Refresh")
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.voice.refresh_devices();
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ),
                    ),
            )
            .child(
                setting_group("Level", None, t)
                    .child(self.setting_row(
                        "Input volume",
                        "Raises or lowers the microphone's signal before Kodem hears it.",
                        self.slider_control(
                            controls::Slider::new(
                                "voice-input-gain",
                                gain,
                                quietest * 100.,
                                loudest * 100.,
                                5.,
                                cx.listener(move |s, value: &f32, _, cx| {
                                    let gain = (value / 100.).clamp(quietest, loudest);
                                    s.settings.voice.input_gain = gain;
                                    if let Some(test) = &s.voice.test {
                                        test.set_gain(gain);
                                    }
                                    s.save_settings();
                                    cx.notify();
                                }),
                            )
                            .label("Input volume"),
                            format!("{gain}%"),
                            t.text,
                        ),
                    ))
                    .child(self.setting_row(
                        "Test microphone",
                        "Speak while the meter runs. Speech should fill about half of it, and the meter turns red where the sound clips.",
                        meter.into_any_element(),
                    )),
            )
            .child(
                setting_group("Speech model", None, t).child(self.setting_row(
                    engine::MODEL_NAME,
                    &format!(
                        "Transcribes 99 languages on this computer, on its GPU where it has one. {} on disk.",
                        megabytes(engine::MODEL_SIZE)
                    ),
                    model,
                )),
            )
            .into_any_element()
    }

    // Records with another microphone from now on, and restarts a running meter with it.
    fn choose_microphone(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.settings.voice.microphone = id;
        self.save_settings();
        if self.voice.test.take().is_some() {
            self.toggle_level_test(cx);
        }
        cx.notify();
    }

    // Starts or stops the Voice page's level meter.
    fn toggle_level_test(&mut self, cx: &mut Context<Self>) {
        if self.voice.test.take().is_some() {
            cx.notify();
            return;
        }
        // The meter waits while voice input records.
        if self.voice.recorder.is_some() {
            return;
        }
        let voice = &self.settings.voice;
        match kodem_voice::Recorder::start(voice.microphone.as_deref(), voice.gain()) {
            Ok(test) => {
                self.voice.test = Some(test);
                self.poll_voice(cx);
            }
            Err(error) => self.report_error(format!("{error:#}"), cx),
        }
        cx.notify();
    }

    // Deletes the downloaded model. The next use offers the download again.
    fn remove_speech_model(&mut self, cx: &mut Context<Self>) {
        // The worker frees its copy with the engine.
        self.voice.engine = None;
        self.voice.loaded = false;
        if let Err(error) = engine::remove_model() {
            self.report_error(format!("{error:#}"), cx);
        }
        cx.notify();
    }
}

// Returns the column of the Voice page's groups.
fn page_body() -> Div {
    div().flex().flex_col().gap(px(super::settings::GROUP_GAP))
}

/// Draws the loudness levels as bars from the right edge leftward, newest on the right, each
/// as tall as its level and centered on the middle line.
fn waveform(levels: Vec<f32>, color: Hsla) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            let count = ((width + WAVE_GAP) / (WAVE_BAR + WAVE_GAP)).floor() as usize;
            let right = f32::from(bounds.origin.x) + width;
            let middle = f32::from(bounds.origin.y) + height / 2.;
            for (index, level) in levels.iter().rev().take(count).enumerate() {
                let bar = (level * height).max(2.).round();
                let x = right - index as f32 * (WAVE_BAR + WAVE_GAP) - WAVE_BAR;
                window.paint_quad(
                    fill(
                        Bounds::new(
                            point(gpui::px(x.round()), gpui::px((middle - bar / 2.).round())),
                            size(gpui::px(WAVE_BAR), gpui::px(bar)),
                        ),
                        color.opacity(0.35 + 0.65 * level),
                    )
                    .corner_radii(px(1.)),
                );
            }
        },
    )
}

// A small mono caption in the voice row.
fn caption(text: &'static str, theme: Theme) -> Div {
    div()
        .flex_shrink_0()
        .font_family(crate::theme::mono())
        .text_size(text_px(11.))
        .text_color(theme.subtle)
        .child(text.to_uppercase())
}

// A mono figure that keeps its width as it counts.
fn figure(text: String, theme: Theme) -> Div {
    div()
        .flex_shrink_0()
        .font_family(crate::theme::mono())
        .text_size(text_px(11.))
        .text_color(theme.text)
        .child(text)
}

// Writes a length of time as minutes and seconds.
fn clock(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

// Writes a byte count in megabytes, as file sizes are usually shown.
fn megabytes(bytes: u64) -> String {
    format!("{} MB", bytes / 1_000_000)
}

fn percent(received: u64, total: u64) -> u64 {
    received * 100 / total.max(1)
}
