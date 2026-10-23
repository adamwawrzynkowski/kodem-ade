//! Presentation and user actions for the installed release channel.
use super::*;
use crate::updater::{self, Channel, State};

impl Shell {
    /// Starts a background check for the installed channel.
    pub(super) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        self.last_update_check = std::time::Instant::now();
        if Channel::current() == Channel::Debug {
            return;
        }
        self.update_state = State::Checking;
        let tx = self.update_tx.clone();
        std::thread::spawn(move || {
            updater::remove_finished_updates();
            let state = match updater::check() {
                Ok(Some(release)) => State::Available(release),
                Ok(None) => State::Idle,
                Err(error) => State::Failed(format!("{error:#}")),
            };
            let _ = tx.send(state);
        });
        cx.notify();
    }

    /// Renders the update action or download progress in the sidebar footer or the rail.
    pub(super) fn update_control(&self, collapsed: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some((look, help)) = update_look(&self.update_state) else {
            return div().into_any_element();
        };
        let theme = self.theme;
        let reduce_motion = cx.reduce_motion();
        let tone = if matches!(look, Look::Failed) {
            theme.warning
        } else {
            theme.accent
        };
        let downloading = matches!(look, Look::Downloading { .. });
        let busy = downloading || matches!(look, Look::Installing);
        // A pending update draws the eye with a glint that crosses the control now and then.
        let glint = (matches!(look, Look::Available | Look::Ready) && !reduce_motion).then(|| {
            let (resting, change) = dither::blink(GLINT_PERIOD, GLINT_SPAN);
            (!resting)
                .then(|| dither::cycle(GLINT_PERIOD) / GLINT_SPAN)
                .ok_or(change)
        });
        let glyph = match &look {
            Look::Downloading { .. } | Look::Installing => dither::orbit(tone, reduce_motion),
            Look::Available => {
                // The arrow dips while the glint passes over it.
                let dip = match glint {
                    Some(Ok(travel)) => ((travel * std::f32::consts::PI).sin() * 2.).round(),
                    _ => 0.,
                };
                div()
                    .relative()
                    .top(px(dip))
                    .child(icon("arrow-down").size(px(13.)))
                    .into_any_element()
            }
            Look::Ready | Look::Failed => icon("refresh").size(px(13.)).into_any_element(),
        };
        let label: SharedString = match &look {
            Look::Available => "Update".into(),
            Look::Downloading { fraction, .. } => {
                format!("{}%", (fraction * 100.).floor() as u32).into()
            }
            Look::Ready => "Restart".into(),
            Look::Installing => "Installing".into(),
            Look::Failed => "Retry".into(),
        };
        // The rail has room for the figure of a download, and for an icon otherwise.
        let show_label = !collapsed || downloading;
        let mut control = self
            .tinted_surface("app-update", tone)
            .flex_shrink_0()
            .when(!collapsed || !downloading, |b| b.child(glyph))
            .when(show_label, |b| {
                // The figure keeps one width as it counts, so the control does not jitter.
                b.child(div().when(downloading, |d| d.min_w(px(28.))).child(label))
            })
            .when(!show_label, |b| b.w(px(26.)).px_0())
            .when(matches!(look, Look::Ready), |b| {
                b.bg(kit::tint_fill(tone, 0.15))
                    .border_color(tone.opacity(0.7))
            })
            .disabled(busy)
            // Work in progress stays legible; only the cursor shows that it takes no click.
            .styles(|s| s.disabled(|s| s.opacity(1.)));
        match glint {
            Some(Ok(travel)) => {
                control = control
                    .layer(
                        dither::Dither::new(dither::DitherField::Glow {
                            x: travel * 1.8 - 0.4,
                            y: 0.5,
                            radius: 0.9,
                            strength: 0.85,
                        })
                        .color(tone.opacity(0.6))
                        .grid(2., 1.),
                    )
                    .child(dither::ticker());
            }
            Some(Err(change)) => control = control.child(dither::ticker_after(change)),
            None => {}
        }
        if let Look::Downloading {
            fraction,
            indeterminate,
        } = look
        {
            // A dotted progress line along the bottom edge; it sweeps while the size is unknown.
            let strip = div()
                .absolute()
                .left(px(5.))
                .right(px(5.))
                .bottom(px(2.))
                .h(px(3.));
            control = control.child(if indeterminate {
                strip.child(
                    dither::Dither::new(dither::DitherField::Sweep {
                        width: 0.2,
                        speed: 0.6,
                        floor: 0.15,
                    })
                    .color(tone)
                    .grid(3., 2.)
                    .animated(),
                )
            } else {
                strip.child(dither::DitherBar::meter(fraction, tone, tone.opacity(0.3)))
            });
        }
        let control = control
            .accessibility_label(help.clone())
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: help.clone(),
                    theme,
                })
                .into()
            })
            .on_click(cx.listener(|s, _, _, cx| s.activate_update(cx)));
        // Each new look dissolves into place once.
        let place = if collapsed { "rail" } else { "footer" };
        motion::dissolve(
            SharedString::from(format!("app-update-{place}-{}", look.key())),
            control,
            theme.sidebar,
            crate::theme::RADIUS_CONTROL,
            4.,
            2.,
            320,
        )
    }

    fn activate_update(&mut self, cx: &mut Context<Self>) {
        let state = std::mem::replace(&mut self.update_state, State::Idle);
        match state {
            State::Available(release) => {
                self.update_state = State::Downloading {
                    received: 0,
                    total: 0,
                };
                let tx = self.update_tx.clone();
                std::thread::spawn(move || {
                    let result = updater::download(&release, &tx);
                    let _ = tx.send(match result {
                        Ok(prepared) => State::Ready(prepared),
                        Err(error) => State::Failed(format!("{error:#}")),
                    });
                });
            }
            State::Ready(prepared) => {
                // Save the latest values and require acknowledgements before host shutdown.
                let draft = if self.submitted_draft.is_none() {
                    self.selected
                        .clone()
                        .map(|task_id| (task_id, self.input.read(cx).text()))
                } else {
                    None
                };
                self.update_state = State::Installing;
                let (done, rx) = mpsc::channel();
                let _ = self.tx.send(bridge::Command::PrepareUpdate {
                    settings: self
                        .workspace
                        .as_ref()
                        .map(|_| Box::new(self.settings_snapshot())),
                    draft,
                    done,
                });
                let bridge = self.tx.clone();
                cx.spawn(async move |this, cx| {
                    let result =
                        cx.background_executor()
                            .spawn(async move {
                                match rx.recv_timeout(Duration::from_secs(130)) {
                                    Ok(Ok(())) => updater::install(prepared)
                                        .map_err(|e| (format!("{e:#}"), None)),
                                    Ok(Err(error)) => Err((error, Some(prepared))),
                                    Err(error) => Err((
                                        format!("Could not prepare update: {error}"),
                                        Some(prepared),
                                    )),
                                }
                            })
                            .await;
                    let _ = this.update(cx, |s, cx| match result {
                        Ok(()) => cx.quit(),
                        Err((error, prepared)) => {
                            let _ = bridge.send(bridge::Command::ResumeAfterUpdate);
                            s.report_error(error.clone(), cx);
                            s.update_state =
                                prepared.map_or_else(|| State::Failed(error), State::Ready);
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            State::Failed(_) => self.check_for_updates(cx),
            other => self.update_state = other,
        }
        cx.notify();
    }
}

/// How often a pending update glints, and the share of that period the glint takes to cross.
const GLINT_PERIOD: Duration = Duration::from_secs(5);
const GLINT_SPAN: f32 = 0.28;

/// What the update control shows for one updater state.
enum Look {
    Available,
    /// `fraction` of the download is done; `indeterminate` while starting or verifying.
    Downloading {
        fraction: f32,
        indeterminate: bool,
    },
    Ready,
    Installing,
    Failed,
}

impl Look {
    // Names the look in the entrance key, so that each new look plays the entrance once.
    fn key(&self) -> &'static str {
        match self {
            Look::Available => "available",
            Look::Downloading { .. } => "downloading",
            Look::Ready => "ready",
            Look::Installing => "installing",
            Look::Failed => "failed",
        }
    }
}

// The look and the hint for an updater state, or `None` when there is nothing to show.
fn update_look(state: &State) -> Option<(Look, String)> {
    let megabytes = |bytes: u64| bytes as f64 / 1_048_576.;
    Some(match state {
        State::Idle | State::Checking => return None,
        State::Available(release) => (
            Look::Available,
            format!(
                "Download Kodem {}",
                updater::version_label(&release.version.to_string())
            ),
        ),
        State::Downloading { received, total } => {
            let (fraction, indeterminate, help) = if *total == 0 {
                (0., true, "Starting download…".to_owned())
            } else if received == total {
                (1., true, "Verifying update…".to_owned())
            } else {
                (
                    *received as f32 / *total as f32,
                    false,
                    format!(
                        "Downloading update: {:.1} of {:.1} MB",
                        megabytes(*received),
                        megabytes(*total)
                    ),
                )
            };
            (
                Look::Downloading {
                    fraction,
                    indeterminate,
                },
                help,
            )
        }
        State::Ready(_) => (
            Look::Ready,
            "Restart and install update. Finish active tasks and checks first; terminals will close."
                .to_owned(),
        ),
        State::Installing => (Look::Installing, "Installing update…".to_owned()),
        State::Failed(error) => (Look::Failed, error.clone()),
    })
}
