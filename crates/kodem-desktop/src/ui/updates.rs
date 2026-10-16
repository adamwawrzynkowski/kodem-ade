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

    /// Renders the update action or byte progress in the sidebar footer.
    pub(super) fn update_control(&self, collapsed: bool, cx: &mut Context<Self>) -> AnyElement {
        let (label, glyph, help, disabled) = match &self.update_state {
            State::Idle | State::Checking => return div().into_any_element(),
            State::Available(release) => ("Update".to_owned(), "arrow-down", format!("Download Kodem {}", release.version), false),
            State::Downloading { received, total } => {
                let percent = if *total == 0 { 0 } else { received.saturating_mul(100) / total };
                (format!("{percent}%"), "arrow-down", if *total == 0 { "Starting download…".into() } else if received == total { "Verifying update…".into() } else { format!("Downloading update: {received} of {total} bytes") }, true)
            }
            State::Ready(_) => (String::new(), "refresh", "Restart and install update. Finish active tasks and checks first; terminals will close.".into(), false),
            State::Installing => (String::new(), "refresh", "Installing update…".into(), true),
            State::Failed(error) => ("Retry".into(), "refresh", error.clone(), false),
        };
        let theme = self.theme;
        let downloading = matches!(self.update_state, State::Downloading { .. });
        self.control("app-update")
            .when((!collapsed || downloading) && !label.is_empty(), |b| {
                b.child(label)
            })
            .when(!collapsed || !downloading, |b| b.child(icon(glyph)))
            .border_0()
            .bg(hsla(0., 0., 0., 0.))
            .hover(move |s| s.bg(theme.selected))
            .flex_shrink_0()
            .px_2()
            .text_color(self.theme.accent)
            .disabled(disabled)
            .accessibility_label(help.clone())
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: help.clone(),
                    theme,
                })
                .into()
            })
            .on_click(cx.listener(|s, _, _, cx| s.activate_update(cx)))
            .into_any_element()
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
