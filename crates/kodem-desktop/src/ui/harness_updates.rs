//! The card that offers newer releases of the providers' CLIs and shows each update's progress,
//! floating above the notices.
use super::*;
use crate::harness::{self, Event};
use semver::Version;
use std::time::Instant;

/// How often Kodem reads the installed and the newest release of each provider's CLI.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// How long a finished update stays in the card.
const UPDATED_STAYS: Duration = Duration::from_secs(4);
/// The width of the update card.
const CARD_WIDTH: f32 = 380.;
/// The width of an update's progress meter.
const METER_WIDTH: f32 = 120.;

/// Where the update of one provider's CLI is.
enum Job {
    /// It runs, at its last reported step.
    Running { text: String, fraction: Option<f32> },
    /// It installed this release.
    Updated(Version),
    /// It failed for this reason.
    Failed(String),
}

/// The installed and the newest release of each provider's CLI, the updates that run or ended,
/// and the releases whose offer was dismissed.
pub(super) struct HarnessUpdates {
    installed: Vec<(Provider, Version)>,
    latest: Vec<(Provider, Version)>,
    dismissed: Vec<(Provider, Version)>,
    jobs: Vec<(Provider, Job)>,
    checked: Option<Instant>,
    sender: mpsc::Sender<Event>,
    events: mpsc::Receiver<Event>,
}

impl Default for HarnessUpdates {
    fn default() -> Self {
        let (sender, events) = mpsc::channel();
        Self {
            installed: Vec::new(),
            latest: Vec::new(),
            dismissed: Vec::new(),
            jobs: Vec::new(),
            checked: None,
            sender,
            events,
        }
    }
}

impl HarnessUpdates {
    /// Reports whether the releases should be read again.
    pub(super) fn due(&self) -> bool {
        self.checked
            .is_none_or(|checked| checked.elapsed() >= CHECK_INTERVAL)
    }

    fn installed(&self, provider: Provider) -> Option<&Version> {
        self.installed
            .iter()
            .find(|(known, _)| *known == provider)
            .map(|(_, version)| version)
    }

    fn latest(&self, provider: Provider) -> Option<&Version> {
        self.latest
            .iter()
            .find(|(known, _)| *known == provider)
            .map(|(_, version)| version)
    }

    fn job(&self, provider: Provider) -> Option<&Job> {
        self.jobs
            .iter()
            .find(|(known, _)| *known == provider)
            .map(|(_, job)| job)
    }

    fn set_job(&mut self, provider: Provider, job: Job) {
        self.jobs.retain(|(known, _)| *known != provider);
        self.jobs.push((provider, job));
    }

    fn running(&self) -> bool {
        self.jobs
            .iter()
            .any(|(_, job)| matches!(job, Job::Running { .. }))
    }

    // Returns the newest release when it is newer than the installed one and its offer was not
    // dismissed.
    fn offer(&self, provider: Provider) -> Option<&Version> {
        let latest = self.latest(provider)?;
        let dismissed = self
            .dismissed
            .iter()
            .any(|(known, version)| *known == provider && version == latest);
        (latest > self.installed(provider)? && !dismissed).then_some(latest)
    }

    // The providers that the card lists, in the order of `Provider::ALL`: those with an offer
    // and those whose update runs or ended.
    fn listed(&self) -> Vec<Provider> {
        Provider::ALL
            .into_iter()
            .filter(|provider| self.offer(*provider).is_some() || self.job(*provider).is_some())
            .collect()
    }
}

impl Shell {
    /// Starts a background read of the installed and the newest release of each provider's CLI.
    pub(super) fn check_harness_updates(&mut self, cx: &mut Context<Self>) {
        self.harness_updates.checked = Some(Instant::now());
        cx.spawn(async move |this, cx| {
            let (installed, latest) = cx
                .background_executor()
                .spawn(async move { (harness::installed(), harness::latest()) })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.harness_updates.installed = installed;
                s.harness_updates.latest = latest;
                cx.notify();
            });
        })
        .detach();
    }

    /// Applies the progress and the results that the running updates sent. A finished update
    /// stores the release it installed, asks the host to read the provider's runtime again, and
    /// leaves the card after a few seconds.
    pub(super) fn receive_harness_events(&mut self, cx: &mut Context<Self>) {
        while let Ok(event) = self.harness_updates.events.try_recv() {
            match event {
                Event::Progress {
                    provider,
                    text,
                    fraction,
                } => self
                    .harness_updates
                    .set_job(provider, Job::Running { text, fraction }),
                Event::Done {
                    provider,
                    result: Ok(version),
                } => {
                    let updates = &mut self.harness_updates;
                    updates.installed.retain(|(known, _)| *known != provider);
                    updates.installed.push((provider, version.clone()));
                    updates.set_job(provider, Job::Updated(version));
                    self.send(core::Request::ConnectRuntime {
                        provider,
                        account: None,
                    });
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(UPDATED_STAYS).await;
                        let _ = this.update(cx, |s, cx| {
                            s.harness_updates.jobs.retain(|(known, job)| {
                                *known != provider || !matches!(job, Job::Updated(_))
                            });
                            cx.notify();
                        });
                    })
                    .detach();
                }
                Event::Done {
                    provider,
                    result: Err(error),
                } => self.harness_updates.set_job(provider, Job::Failed(error)),
            }
            cx.notify();
        }
    }

    // Starts the update of a provider's CLI to its newest release.
    fn start_harness_update(&mut self, provider: Provider, cx: &mut Context<Self>) {
        let updates = &mut self.harness_updates;
        let Some(latest) = updates.latest(provider).cloned() else {
            return;
        };
        if matches!(updates.job(provider), Some(Job::Running { .. })) {
            return;
        }
        updates.set_job(
            provider,
            Job::Running {
                text: "Starting the update…".into(),
                fraction: None,
            },
        );
        harness::update(provider, latest, updates.sender.clone());
        cx.notify();
    }

    // Hides the offers and the ended updates of the card. Offers return when a newer release
    // than the dismissed one appears.
    fn dismiss_harness_updates(&mut self, cx: &mut Context<Self>) {
        let updates = &mut self.harness_updates;
        for provider in Provider::ALL {
            if let Some(latest) = updates.offer(provider).cloned() {
                updates.dismissed.push((provider, latest));
            }
        }
        updates
            .jobs
            .retain(|(_, job)| matches!(job, Job::Running { .. }));
        cx.notify();
    }

    /// Returns the card that lists every provider CLI with a newer release or an update that
    /// runs or ended, or nothing while there is none. It is glass like a notice card. Each row
    /// has its provider's mark, its releases, its update's progress and its own update button;
    /// the card also updates every listed CLI at once, or hides.
    pub(super) fn harness_update_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let updates = &self.harness_updates;
        let listed = updates.listed();
        if listed.is_empty() {
            return None;
        }
        let t = self.theme;
        let tone = t.accent;
        let radius = crate::theme::RADIUS_SURFACE;
        let running = updates.running();
        let startable: Vec<Provider> = listed
            .iter()
            .copied()
            .filter(|provider| {
                !matches!(
                    updates.job(*provider),
                    Some(Job::Running { .. } | Job::Updated(_))
                )
            })
            .collect();
        let title = if running {
            "Updating"
        } else if startable.is_empty() {
            "Updated"
        } else {
            "Updates available"
        };
        let mut light = kit::raised_light(t);
        light.extend(t.floating());
        let header = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .pr(px(4.))
            .child(caps(title, t).text_color(tone))
            .child(figure(listed.len().to_string(), t.muted))
            .child(div().flex_1())
            .when(!running, |header| {
                header.child(
                    self.icon_button("harness-updates-dismiss", "close", "Dismiss")
                        .size(px(22.))
                        .on_click(cx.listener(|s, _, _, cx| s.dismiss_harness_updates(cx))),
                )
            });
        let rows: Vec<AnyElement> = listed
            .iter()
            .map(|provider| self.harness_update_row(*provider, cx))
            .collect();
        // Several updates that can start get one action for all of them.
        let update_all = (startable.len() > 1).then(|| {
            div().pr(px(6.)).flex().justify_end().child(
                self.primary("harness-updates-all", "arrow-down", "Update all")
                    .on_click(cx.listener(move |s, _, _, cx| {
                        for provider in &startable {
                            s.start_harness_update(*provider, cx);
                        }
                    })),
            )
        });
        let card = kit::glass(div(), t.raised, kit::SURFACE_GLASS_FILL)
            .id("harness-updates")
            .occlude()
            .relative()
            .overflow_hidden()
            .w(px(CARD_WIDTH))
            .flex()
            .flex_col()
            .gap(px(8.))
            .pl(px(10.))
            .pr(px(4.))
            .pt(px(7.))
            .pb(px(10.))
            .rounded(px(radius))
            .border_1()
            .border_color(tone.opacity(0.4))
            .shadow(light)
            .layer(kit::shade(tone.opacity(0.15), dither::Edge::Left))
            .child(header)
            .children(rows)
            .children(update_all);
        let card = div()
            .relative()
            .child(dither::shadow(t, radius, 5.))
            .child(card);
        Some(motion::notice_enter(
            "harness-updates-enter",
            card,
            t,
            radius,
        ))
    }

    // Returns the button that starts or retries a provider's update, or nothing while it runs
    // or after it installed.
    fn harness_update_button(&self, provider: Provider, cx: &mut Context<Self>) -> Option<Button> {
        let (text, glyph) = match self.harness_updates.job(provider) {
            None => ("Update", "arrow-down"),
            Some(Job::Failed(_)) => ("Retry", "refresh"),
            Some(Job::Running { .. } | Job::Updated(_)) => return None,
        };
        Some(
            self.primary(("harness-update", provider as usize), glyph, text)
                .on_click(cx.listener(move |s, _, _, cx| s.start_harness_update(provider, cx))),
        )
    }

    // One provider of the update card: its mark, which pulses while its update runs, the
    // installed and the newest release, the update's step, progress or result, and its button.
    fn harness_update_row(&self, provider: Provider, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let updates = &self.harness_updates;
        let label = provider.label();
        let job = updates.job(provider);
        let releases = match (job, updates.installed(provider), updates.latest(provider)) {
            (Some(Job::Updated(version)), _, _) => version.to_string(),
            (_, Some(installed), Some(latest)) => format!("{installed} → {latest}"),
            _ => String::new(),
        };
        let (detail, detail_tone) = match job {
            Some(Job::Running { text, .. }) => (text.clone(), t.muted),
            Some(Job::Updated(version)) => (format!("Updated to {version}"), t.added),
            Some(Job::Failed(error)) => (error.clone(), t.removed),
            None => ("A newer release is available.".to_owned(), t.muted),
        };
        let tile_tone = match job {
            Some(Job::Updated(_)) => t.added,
            Some(Job::Failed(_)) => t.removed,
            _ => t.accent,
        };
        let running = matches!(job, Some(Job::Running { .. }));
        let fraction = match job {
            Some(Job::Running { fraction, .. }) => *fraction,
            _ => None,
        };
        let action = self.harness_update_button(provider, cx);
        div()
            .flex()
            .items_start()
            .gap(px(10.))
            .pr(px(6.))
            .child(kit::icon_tile(provider.icon(), tile_tone, 24., running))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(text_px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.text)
                                    .child(label),
                            )
                            .child(figure(releases, t.muted)),
                    )
                    .when(running, |column| {
                        column.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(kit::meter(fraction, t.accent, METER_WIDTH, t))
                                .when_some(fraction, |line, fraction| {
                                    line.child(figure(format!("{:.0}%", fraction * 100.), t.muted))
                                }),
                        )
                    })
                    .child(
                        div()
                            .text_size(text_px(12.))
                            .text_color(detail_tone)
                            .line_clamp(2)
                            .child(detail),
                    ),
            )
            .children(action)
            .into_any_element()
    }
}
