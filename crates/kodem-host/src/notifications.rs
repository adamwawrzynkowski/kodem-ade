//! Host-owned notifications continue after the desktop window closes.
use crate::Host;
use anyhow::Result;
use kodem_core::{NotificationDelivery, NotificationSettings, NotificationSound, RunState, Task};
use std::sync::{Arc, Mutex, mpsc};

struct Alert {
    title: String,
    body: String,
    sound: Option<NotificationSound>,
    /// Shows a banner and plays the sound while Kodem is the active application. Only macOS
    /// asks the host how to present an alert; elsewhere the banner always shows.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    foreground: bool,
}

pub(super) struct Notifications {
    sender: mpsc::Sender<Alert>,
    status: Arc<Mutex<NotificationDelivery>>,
}

impl Notifications {
    /// Starts one delivery worker so authorization and system callbacks never block tasks.
    pub(super) fn start() -> Self {
        let (sender, receiver) = mpsc::channel::<Alert>();
        let status = Arc::new(Mutex::new(NotificationDelivery::Idle));
        let worker_status = status.clone();
        std::thread::spawn(move || {
            // Initialize lazily: opening Kodem alone must not request notification permission.
            #[cfg(target_os = "macos")]
            let mut delivery = None;
            for alert in receiver {
                #[cfg(target_os = "macos")]
                let result = objc2::rc::autoreleasepool(|_| {
                    if delivery.is_none() {
                        delivery = Some(native::Delivery::new()?);
                    }
                    delivery.as_ref().unwrap().send(&alert)
                });
                #[cfg(not(target_os = "macos"))]
                let result = desktop::send(&alert);
                *worker_status.lock().unwrap() = match result {
                    Ok(()) => NotificationDelivery::Submitted,
                    Err(error) => {
                        eprintln!("Kodem notification: {error:#}");
                        NotificationDelivery::Failed(format!("{error:#}"))
                    }
                };
            }
        });
        Self { sender, status }
    }

    /// Returns the most recent delivery result for the settings panel.
    pub(super) fn status(&self) -> NotificationDelivery {
        self.status.lock().unwrap().clone()
    }

    /// Sends an explicit preview even when automatic task notifications are disabled. The
    /// preview always shows, because the user sends it from the focused settings window.
    pub(super) fn test(&self, sound: Option<NotificationSound>) {
        self.enqueue(Alert {
            title: "Kodem · Test notification".into(),
            body: "Your task notifications will appear here.".into(),
            sound,
            foreground: true,
        });
    }

    fn enqueue(&self, alert: Alert) {
        *self.status.lock().unwrap() = NotificationDelivery::Pending;
        if self.sender.send(alert).is_err() {
            *self.status.lock().unwrap() =
                NotificationDelivery::Failed("Notification worker is unavailable".into());
        }
    }
}

impl Host {
    /// Persists a provider transition before queuing its alert; repeated states stay silent.
    pub(super) fn save_task_with_notification(&self, task: &Task) -> Result<()> {
        let previous = self.store.task(&task.id)?.state;
        self.store.save_task(task)?;
        if previous != task.state {
            self.notify_task_state(task);
        }
        Ok(())
    }

    /// Queues the current task alert, including each distinct pending input request.
    pub(super) fn notify_task_state(&self, task: &Task) {
        self.notify_task(task, &task.state);
    }

    /// Alerts for async questions without claiming that the provider stopped working.
    pub(super) fn notify_input_needed(&self, task: &Task) {
        self.notify_task(task, &RunState::Waiting);
    }

    fn notify_task(&self, task: &Task, state: &RunState) {
        // A delegated task reports its outcome to the agent of its parent, whose turn alerts the
        // user. Its own approvals still alert, because only the user can answer them.
        let delegated = task
            .lineage
            .as_ref()
            .is_some_and(|lineage| lineage.delegated);
        if delegated && !matches!(state, RunState::Waiting | RunState::DeliveryUnknown) {
            return;
        }
        // A notification preference failure must never fail an already persisted turn.
        match self.store.settings() {
            Ok(settings) => {
                if let Some(title) = alert_title(&settings.notifications, state) {
                    self.notifications.enqueue(Alert {
                        title: format!("Kodem · {title}"),
                        body: task.title.clone(),
                        sound: settings
                            .notifications
                            .play_sound
                            .then_some(settings.notifications.sound),
                        foreground: settings.notifications.while_focused,
                    });
                }
            }
            Err(error) => eprintln!("Could not read notification preferences: {error:#}"),
        }
    }
}

fn alert_title(settings: &NotificationSettings, state: &RunState) -> Option<&'static str> {
    if !settings.enabled {
        return None;
    }
    match state {
        RunState::Waiting if settings.attention => Some("Task needs your input"),
        RunState::DeliveryUnknown if settings.attention => Some("Task needs attention"),
        RunState::Completed if settings.completed => Some("Task finished"),
        RunState::Failed if settings.failed => Some("Task failed"),
        RunState::Interrupted if settings.interrupted => Some("Task interrupted"),
        _ => None,
    }
}

/// Desktop notifications through the Windows toast service or the freedesktop.org notification
/// service. Neither plays Kodem's own sounds, so the host plays the sound itself.
#[cfg(not(target_os = "macos"))]
mod desktop {
    use super::Alert;
    use anyhow::Result;

    /// Shows one alert and starts its sound.
    pub(super) fn send(alert: &Alert) -> Result<()> {
        let mut notification = notify_rust::Notification::new();
        notification.summary(&alert.title).body(&alert.body);
        #[cfg(not(windows))]
        {
            // The Linux package carries its icon next to the executables, outside icon themes.
            let icon = std::env::current_exe()
                .ok()
                .and_then(|executable| Some(executable.parent()?.join("kodem.png")))
                .filter(|icon| icon.is_file());
            let app_id = kodem_core::platform::app_id();
            notification
                .appname("Kodem")
                .hint(notify_rust::Hint::DesktopEntry(app_id.into()));
            match icon {
                Some(icon) => notification.icon(&icon.to_string_lossy()),
                None => notification.icon(app_id),
            };
        }
        notification.show().map_err(|error| {
            anyhow::anyhow!("The system did not accept the notification: {error}")
        })?;
        if let Some(sound) = alert.sound {
            kodem_core::sound::play(sound)?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::Alert;
    use anyhow::{Context, Result, ensure};
    use block2::RcBlock;
    use kodem_core::notification_presentation::NotificationPresentation;
    use objc2::{rc::Retained, runtime::Bool};
    use objc2_foundation::{NSBundle, NSError, NSString};
    use objc2_user_notifications::*;
    use std::{sync::mpsc, time::Duration};

    pub(super) struct Delivery {
        center: Retained<UNUserNotificationCenter>,
        _presentation: NotificationPresentation,
    }

    impl Delivery {
        /// Creates the native service only when running from Kodem's app bundle.
        pub(super) fn new() -> Result<Self> {
            ensure!(
                NSBundle::mainBundle().bundleIdentifier().is_some(),
                "Notifications require the bundled Kodem app. Build it with scripts/build-macos.sh debug."
            );
            let center = UNUserNotificationCenter::currentNotificationCenter();
            let presentation = NotificationPresentation::install()
                .context("Could not install notification presentation")?;
            Ok(Self {
                center,
                _presentation: presentation,
            })
        }

        /// Requests permission and waits for macOS to accept the notification request.
        pub(super) fn send(&self, alert: &Alert) -> Result<()> {
            let (tx, rx) = mpsc::channel();
            self.center.requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
                &RcBlock::new(move |granted: Bool, error: *mut NSError| {
                    let result = native_result(error).and_then(|()| {
                        ensure!(granted.as_bool(),
                            "Notifications are disabled. Allow Kodem in System Settings > Notifications.");
                        Ok(())
                    });
                    let _ = tx.send(result);
                }),
            );
            rx.recv_timeout(Duration::from_secs(60)).context(
                "Notification permission is still pending. Respond to macOS, then test again.",
            )??;
            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str(&alert.title));
            content.setBody(&NSString::from_str(&alert.body));
            NotificationPresentation::set_foreground(&content, alert.foreground);
            if let Some(sound) = alert.sound {
                let name = NSString::from_str(sound.filename());
                ensure!(
                    NSBundle::mainBundle()
                        .pathForResource_ofType(Some(&name), None)
                        .is_some(),
                    "The notification sound is missing. Rebuild the Kodem app bundle."
                );
                content.setSound(Some(&UNNotificationSound::soundNamed(&name)));
            }
            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &NSString::from_str(&kodem_core::new_id()),
                &content,
                None,
            );
            let (tx, rx) = mpsc::channel();
            self.center.addNotificationRequest_withCompletionHandler(
                &request,
                Some(&RcBlock::new(move |error: *mut NSError| {
                    let _ = tx.send(native_result(error));
                })),
            );
            rx.recv_timeout(Duration::from_secs(10))
                .context("macOS did not confirm the notification. Try again.")?
        }
    }

    fn native_result(error: *mut NSError) -> Result<()> {
        // Framework callback pointers remain valid for the duration of the callback.
        if let Some(error) = unsafe { error.as_ref() } {
            if unsafe { UNErrorDomain }.is_some_and(|domain| &*error.domain() == domain)
                && error.code() == UNErrorCode::NotificationsNotAllowed.0
            {
                anyhow::bail!(
                    "macOS did not authorize notifications. Allow Kodem in System Settings > Notifications, then test again."
                );
            }
            anyhow::bail!("{}", error.localizedDescription());
        }
        Ok(())
    }
}
