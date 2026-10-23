//! Notices: cards that float in the window's top right corner and the notification center that
//! keeps errors, warnings and task events. An error or a warning opens its whole text in the
//! details dialog.
use super::*;
use std::time::Instant;

/// How long a confirmation floats before it leaves.
const CONFIRMATION_FLOAT: Duration = Duration::from_millis(3600);
/// The most cards that float at once. A new card sends the oldest away.
const MAX_FLOATING: usize = 3;
/// The most notices that the notification center keeps.
const MAX_KEPT: usize = 50;
/// The width of an error card.
const CARD_WIDTH: f32 = 360.;
/// The most lines of a notice's text that its card and its entry show. Details shows the rest.
const TEXT_LINES: usize = 4;
/// The shortest time between two error sounds, so a burst of errors plays one sound.
const ERROR_SOUND_GAP: Duration = Duration::from_millis(1500);

/// What a notice reports. It sets the notice's label, glyph and tone, how long its card floats
/// and whether the notification center keeps it.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum NoticeKind {
    /// A short confirmation in a tone of its caller. It floats briefly and is not kept.
    Confirmation(Hsla),
    /// An error. Its card floats while the error is current.
    Error,
    /// A problem that blocks work until the user acts or it passes, such as a usage limit.
    Warning,
    /// A task finished its turn.
    Finished,
    /// A task waits for an answer or an approval.
    NeedsInput,
    /// A task failed or its delivery is uncertain.
    Blocked,
}

impl NoticeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Confirmation(_) => "Done",
            Self::Error => "Error",
            Self::Warning => "Warning",
            Self::Finished => "Finished",
            Self::NeedsInput => "Needs you",
            Self::Blocked => "Blocked",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Confirmation(_) => "check",
            Self::Error | Self::Warning => "triangle-alert",
            Self::Finished => "circle-check",
            Self::NeedsInput => "chat",
            Self::Blocked => "circle-x",
        }
    }

    /// Returns the tone of the notice in a theme.
    pub(super) fn tone(self, t: Theme) -> Hsla {
        match self {
            Self::Confirmation(tone) => tone,
            Self::Error | Self::Blocked => t.removed,
            Self::Warning => t.warning,
            Self::Finished => t.added,
            Self::NeedsInput => t.warning,
        }
    }

    fn kept(self) -> bool {
        !matches!(self, Self::Confirmation(_))
    }

    /// Reports whether the notice is an error or a warning, whose details can be opened.
    fn problem(self) -> bool {
        matches!(self, Self::Error | Self::Warning)
    }
}

/// A problem that a notice reports while it holds. Its card leaves when the problem ends.
#[derive(Clone, PartialEq)]
pub(super) enum Condition {
    /// A task stopped with an error.
    TaskError { task: String, error: String },
    /// A task's delivery needs confirmation before another message can be sent.
    Delivery(String),
    /// The account of the open task was removed.
    AccountRemoved(Provider),
    /// The account of the composer is signed out.
    SignedOut(Provider, String),
    /// The account of the composer reached its usage limit.
    Limit(Provider, String),
}

/// The action that a notice offers beside its details.
#[derive(Clone)]
pub(super) enum NoticeAction {
    /// Opens the agent menu to choose another account.
    ChooseAccount,
    /// Signs an account in.
    SignIn { provider: Provider, id: String },
    /// Checks the delivery history of a task.
    CheckHistory(String),
}

/// A problem of a task or of the composer's account, as a notice reports it.
pub(super) struct Problem {
    pub(super) condition: Condition,
    kind: NoticeKind,
    text: String,
    /// The task and its title.
    task: Option<(String, String)>,
    action: Option<NoticeAction>,
}

/// Where a notice's card is in its float over the workspace.
#[derive(Clone, Copy, PartialEq)]
enum Float {
    Shown,
    Leaving,
    Gone,
}

/// One notice: a card that floats over the workspace, an entry of the notification center, or both.
struct Notice {
    id: u64,
    kind: NoticeKind,
    text: SharedString,
    /// The task that the notice is about. Its entry opens the task.
    task: Option<String>,
    /// A line above the text, such as the title of the notice's task.
    title: Option<SharedString>,
    /// The problem that keeps the card floating.
    condition: Option<Condition>,
    action: Option<NoticeAction>,
    at: Instant,
    float: Float,
}

impl Notice {
    fn new(kind: NoticeKind, text: SharedString) -> Self {
        Self {
            id: 0,
            kind,
            text,
            task: None,
            title: None,
            condition: None,
            action: None,
            at: Instant::now(),
            float: Float::Gone,
        }
    }
}

/// The notice that the details dialog shows, and the dialog that it returns to.
struct Details {
    kind: NoticeKind,
    title: Option<SharedString>,
    text: SharedString,
    at: Instant,
    /// The selectable text of the notice.
    log: Entity<TextInput>,
    back: Panel,
}

/// The notices of the window, oldest first, and what the notification center has shown.
#[derive(Default)]
pub(super) struct Notices {
    list: Vec<Notice>,
    sequence: u64,
    /// The newest notice when the notification center last opened.
    seen: u64,
    /// The newest notice that had been seen before the center last opened. Entries after it
    /// show as new.
    seen_before: u64,
    /// The problems of the open task and of the composer in the last frame. A problem that
    /// appears floats its card again.
    shown: Vec<Condition>,
    /// When the error sound last played.
    sounded: Option<Instant>,
    details: Option<Details>,
}

impl Notices {
    /// Counts the kept notices that the notification center has not shown, and reports whether
    /// any of them is a failure.
    fn unread(&self) -> (usize, bool) {
        self.list
            .iter()
            .filter(|notice| notice.kind.kept() && notice.id > self.seen)
            .fold((0, false), |(count, failed), notice| {
                (
                    count + 1,
                    failed || matches!(notice.kind, NoticeKind::Error | NoticeKind::Blocked),
                )
            })
    }

    // Drops the notices whose card has gone and that the center does not keep, then the oldest
    // kept notices beyond the limit whose card has gone.
    fn prune(&mut self) {
        self.list
            .retain(|notice| notice.float != Float::Gone || notice.kind.kept());
        let mut excess = self
            .list
            .iter()
            .filter(|notice| notice.kind.kept())
            .count()
            .saturating_sub(MAX_KEPT);
        self.list.retain(|notice| {
            if excess > 0 && notice.kind.kept() && notice.float == Float::Gone {
                excess -= 1;
                return false;
            }
            true
        });
    }
}

/// Returns the problem of a task: uncertain delivery, or the error that stopped it.
pub(super) fn task_problem(task: &core::Task) -> Option<Problem> {
    let (condition, kind, text, action) = if task.state == RunState::DeliveryUnknown {
        let text = match &task.error {
            Some(error) => format!(
                "Delivery needs confirmation before another message can be sent.\n\n{error}"
            ),
            None => "Delivery needs confirmation before another message can be sent.".to_owned(),
        };
        (
            Condition::Delivery(task.id.clone()),
            NoticeKind::Warning,
            text,
            Some(NoticeAction::CheckHistory(task.id.clone())),
        )
    } else {
        let error = task.error.clone()?;
        (
            Condition::TaskError {
                task: task.id.clone(),
                error: error.clone(),
            },
            NoticeKind::Error,
            error,
            None,
        )
    };
    Some(Problem {
        condition,
        kind,
        text,
        task: Some((task.id.clone(), task.title.clone())),
        action,
    })
}

// Names the local time when a usage limit resets.
fn reset_clock(reset: Option<u64>) -> String {
    let Some(at) = reset.and_then(|reset| chrono::DateTime::from_timestamp(reset as i64, 0)) else {
        return "Reset time unavailable.".to_owned();
    };
    let at = at.with_timezone(&chrono::Local);
    if at.date_naive() == chrono::Local::now().date_naive() {
        format!("Resets at {}.", at.format("%H:%M"))
    } else {
        format!("Resets {}.", at.format("%a %H:%M"))
    }
}

impl Shell {
    /// Shows a short confirmation that floats for a few seconds and is not kept.
    pub(super) fn notify_user(
        &mut self,
        text: impl Into<SharedString>,
        tone: Hsla,
        cx: &mut Context<Self>,
    ) {
        self.push_notice(
            Notice::new(NoticeKind::Confirmation(tone), text.into()),
            true,
            cx,
        );
    }

    /// Reports an error. Its card floats until it is dismissed, resolved or replaced, the
    /// notification center keeps it and the error sound plays.
    pub(super) fn report_error(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let text = text.into();
        // The same error reported again while its card floats does not stack a second card.
        let floating = self.notices.list.iter().any(|notice| {
            notice.kind == NoticeKind::Error
                && notice.condition.is_none()
                && notice.float == Float::Shown
                && notice.text.as_ref() == text
        });
        if !floating {
            self.push_notice(
                Notice::new(NoticeKind::Error, text.clone().into()),
                true,
                cx,
            );
            self.sound_error();
        }
        self.error = Some(text);
        cx.notify();
    }

    /// Records an event of a task in the notification center. It does not float: the task's
    /// row in the sidebar marks the event.
    pub(super) fn record_task_event(
        &mut self,
        kind: NoticeKind,
        task_id: String,
        title: String,
        cx: &mut Context<Self>,
    ) {
        self.push_notice(
            Notice {
                task: Some(task_id),
                ..Notice::new(kind, title.into())
            },
            false,
            cx,
        );
    }

    /// Floats the card of a problem while it holds, unless its card already floats. A notice
    /// that the center keeps for the same problem floats again instead of a new one. `fresh`
    /// marks a problem that has just happened: an error then plays the error sound.
    pub(super) fn raise_problem(&mut self, problem: Problem, fresh: bool, cx: &mut Context<Self>) {
        let kept = self
            .notices
            .list
            .iter()
            .rposition(|notice| notice.condition.as_ref() == Some(&problem.condition));
        match kept {
            Some(index) if self.notices.list[index].float == Float::Shown => return,
            Some(index) => {
                // The notice moves to the end of the list, where the newest card floats, and
                // keeps its id, so the center does not count it as new again.
                let mut notice = self.notices.list.remove(index);
                notice.float = Float::Shown;
                notice.action = problem.action;
                self.notices.list.push(notice);
                self.limit_floating(cx);
                cx.notify();
            }
            None => {
                let (task, title) = problem.task.unzip();
                self.push_notice(
                    Notice {
                        task,
                        title: title.map(SharedString::from),
                        condition: Some(problem.condition),
                        action: problem.action,
                        ..Notice::new(problem.kind, problem.text.into())
                    },
                    true,
                    cx,
                );
            }
        }
        if fresh && problem.kind == NoticeKind::Error {
            self.sound_error();
        }
    }

    // Plays the error sound, unless the user turned it off or it played a moment ago.
    fn sound_error(&mut self) {
        if !self.settings.notifications.error_sound
            || self
                .notices
                .sounded
                .is_some_and(|at| at.elapsed() < ERROR_SOUND_GAP)
        {
            return;
        }
        self.notices.sounded = Some(Instant::now());
        if let Err(error) = crate::sound::error() {
            eprintln!("Could not play the error sound: {error:#}");
        }
    }

    fn push_notice(&mut self, mut notice: Notice, float: bool, cx: &mut Context<Self>) {
        self.notices.sequence += 1;
        let id = self.notices.sequence;
        let kind = notice.kind;
        notice.id = id;
        notice.float = if float { Float::Shown } else { Float::Gone };
        self.notices.list.push(notice);
        self.limit_floating(cx);
        self.notices.prune();
        if float && !kind.kept() {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(CONFIRMATION_FLOAT).await;
                let _ = this.update(cx, |s, cx| s.dismiss_notice(id, cx));
            })
            .detach();
        }
        cx.notify();
    }

    // Sends the oldest floating cards away while more than the limit float.
    fn limit_floating(&mut self, cx: &mut Context<Self>) {
        let floating: Vec<u64> = self
            .notices
            .list
            .iter()
            .filter(|notice| notice.float == Float::Shown)
            .map(|notice| notice.id)
            .collect();
        for oldest in &floating[..floating.len().saturating_sub(MAX_FLOATING)] {
            self.dismiss_notice(*oldest, cx);
        }
    }

    /// Sends a floating card away: it plays its leave, then stops floating. Dismissing an error
    /// also clears it from the dialogs that show it.
    fn dismiss_notice(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(notice) = self
            .notices
            .list
            .iter_mut()
            .find(|notice| notice.id == id && notice.float == Float::Shown)
        else {
            return;
        };
        notice.float = Float::Leaving;
        if notice.kind == NoticeKind::Error
            && notice.condition.is_none()
            && self.error.as_deref() == Some(notice.text.as_ref())
        {
            self.error = None;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(motion::NOTICE_LEAVE).await;
            let _ = this.update(cx, |s, cx| {
                // A card that floated again while it left stays.
                if let Some(notice) = s
                    .notices
                    .list
                    .iter_mut()
                    .find(|notice| notice.id == id && notice.float == Float::Leaving)
                {
                    notice.float = Float::Gone;
                }
                s.notices.prune();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Floats the cards of the problems that appeared in the open task or the composer's
    /// account, and sends away the cards whose error or problem was resolved or replaced.
    /// Call it once per render.
    pub(super) fn sync_notices(&mut self, cx: &mut Context<Self>) {
        let problems = self.open_problems();
        let shown: Vec<Condition> = problems
            .iter()
            .map(|problem| problem.condition.clone())
            .collect();
        for problem in problems {
            if !self.notices.shown.contains(&problem.condition) {
                self.raise_problem(problem, false, cx);
                continue;
            }
            // A problem that holds can change its words, such as the reset time of a limit.
            if let Some(notice) = self.notices.list.iter_mut().find(|notice| {
                notice.float == Float::Shown
                    && notice.condition.as_ref() == Some(&problem.condition)
            }) && notice.text.as_ref() != problem.text
            {
                notice.text = problem.text.into();
            }
        }
        self.notices.shown = shown;
        let stale: Vec<u64> = self
            .notices
            .list
            .iter()
            .filter(|notice| notice.float == Float::Shown)
            .filter(|notice| match &notice.condition {
                Some(condition) => !self.problem_holds(condition),
                None => {
                    notice.kind == NoticeKind::Error
                        && self.error.as_deref() != Some(notice.text.as_ref())
                }
            })
            .map(|notice| notice.id)
            .collect();
        for id in stale {
            self.dismiss_notice(id, cx);
        }
    }

    // Returns the problems of the open task and of the composer's account.
    fn open_problems(&self) -> Vec<Problem> {
        let open = self.view.as_ref().and_then(|view| {
            self.workspace
                .as_ref()?
                .tasks
                .iter()
                .find(|task| task.id == view.task.id)
        });
        open.and_then(task_problem)
            .into_iter()
            .chain(self.account_problem())
            .collect()
    }

    // Reports whether a problem still holds. A task's problem holds while the task has it; an
    // account's problem holds while the composer shows that account.
    fn problem_holds(&self, condition: &Condition) -> bool {
        let task = |id: &str| {
            self.workspace
                .as_ref()
                .and_then(|workspace| workspace.tasks.iter().find(|task| task.id == id))
        };
        match condition {
            Condition::TaskError { task: id, error } => task(id).is_some_and(|task| {
                task.state != RunState::DeliveryUnknown && task.error.as_ref() == Some(error)
            }),
            Condition::Delivery(id) => {
                task(id).is_some_and(|task| task.state == RunState::DeliveryUnknown)
            }
            Condition::AccountRemoved(_) | Condition::SignedOut(..) | Condition::Limit(..) => {
                self.notices.shown.contains(condition)
            }
        }
    }

    // Returns the problem of the account that the composer uses, when it cannot run a turn.
    fn account_problem(&self) -> Option<Problem> {
        let Some(state) = self.composer_account() else {
            // The host keeps a removed account's tasks blocked instead of switching identity.
            let preferences = self.composer_preferences();
            self.workspace.as_ref()?;
            preferences.account.as_ref()?;
            return Some(Problem {
                condition: Condition::AccountRemoved(preferences.provider),
                kind: NoticeKind::Error,
                text: format!(
                    "The {} account of this task was removed. Choose another account to continue.",
                    preferences.provider.label()
                ),
                task: None,
                action: Some(NoticeAction::ChooseAccount),
            });
        };
        let provider = state.account.provider;
        let id = state.account.id.clone();
        let name = &state.account.name;
        if state.status.signed_in == Some(false) {
            return Some(Problem {
                condition: Condition::SignedOut(provider, id.clone()),
                kind: NoticeKind::Warning,
                text: format!("{name} is signed out of {}.", provider.label()),
                task: None,
                action: Some(NoticeAction::SignIn { provider, id }),
            });
        }
        let usage = state
            .status
            .usage
            .as_ref()
            .filter(|usage| usage::limit_reached(usage))?;
        let reset = [usage.primary.as_ref(), usage.secondary.as_ref()]
            .into_iter()
            .flatten()
            .filter(|window| window.used_percent >= 100)
            .filter_map(|window| window.resets_at)
            .min();
        // The notice names the reset. It does not offer another account: using one to continue
        // past a limit is what the providers' terms forbid.
        Some(Problem {
            condition: Condition::Limit(provider, id),
            kind: NoticeKind::Warning,
            text: format!("{name} reached its usage limit. {}", reset_clock(reset)),
            task: None,
            action: None,
        })
    }

    /// Opens the details dialog of a notice: its whole text, which can be selected, scrolled
    /// and copied. Closing it returns to the dialog that was open.
    fn open_notice_details(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(notice) = self.notices.list.iter().find(|notice| notice.id == id) else {
            return;
        };
        let text = notice.text.clone();
        let log = cx.new(|cx| TextInput::readonly(text.to_string(), cx));
        let back = match self.panel {
            Panel::CustomizeProject
            | Panel::RemoveProject
            | Panel::NoRepository
            | Panel::Settings
            | Panel::Setup
            | Panel::Resources
            | Panel::Files
            | Panel::Check
            | Panel::Rename
            | Panel::DeleteTask
            | Panel::ImportThreads
            | Panel::Quit
            | Panel::AddServer
            | Panel::AddSkill
            | Panel::WhatsNew => self.panel.clone(),
            _ => Panel::None,
        };
        self.notices.details = Some(Details {
            kind: notice.kind,
            title: notice.title.clone(),
            text,
            at: notice.at,
            log,
            back,
        });
        self.panel = Panel::NoticeDetails;
        self.menu_anchor = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Closes the details dialog and returns to the dialog that was open before it.
    pub(super) fn close_notice_details(&mut self, cx: &mut Context<Self>) {
        self.panel = self
            .notices
            .details
            .take()
            .map_or(Panel::None, |details| details.back);
        cx.notify();
    }

    /// Returns the title of the details dialog.
    pub(super) fn notice_details_title(&self) -> &'static str {
        match self.notices.details.as_ref().map(|details| details.kind) {
            Some(NoticeKind::Warning) => "Warning details",
            _ => "Error details",
        }
    }

    /// Returns the tone of the details dialog: the tone of its notice.
    pub(super) fn notice_details_tone(&self) -> Hsla {
        self.notices
            .details
            .as_ref()
            .map_or(self.theme.removed, |details| details.kind.tone(self.theme))
    }

    /// Returns the content of the details dialog: what the notice is about, then its whole text
    /// in a selectable block.
    pub(super) fn notice_details(&self) -> Div {
        let t = self.theme;
        let Some(details) = &self.notices.details else {
            return div();
        };
        let tone = details.kind.tone(t);
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(kit::chip(details.kind.label().to_uppercase(), tone))
                    .when_some(details.title.clone(), |line, title| {
                        line.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(text_px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.text)
                                .child(title),
                        )
                    })
                    .when(details.title.is_none(), |line| line.child(div().flex_1()))
                    .child(figure(
                        format!("{} ago", usage::duration(details.at.elapsed().as_secs())),
                        t.subtle,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(crate::theme::RADIUS_SURFACE))
                    .bg(t.inset)
                    .border_1()
                    .border_color(tone.opacity(0.3))
                    .font_family(crate::theme::mono())
                    .text_size(code_px(12.))
                    .line_height(relative(1.55))
                    .text_color(t.text)
                    .child(details.log.clone()),
            )
    }

    /// Returns the actions of the details dialog: close, and copy the whole text.
    pub(super) fn notice_details_actions(&self, cx: &mut Context<Self>) -> Div {
        let text = self
            .notices
            .details
            .as_ref()
            .map(|details| details.text.clone())
            .unwrap_or_default();
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .child(
                self.ghost("notice-details-close", "close", "Close")
                    .on_click(cx.listener(|s, _, _, cx| s.close_notice_details(cx))),
            )
            .child(
                self.primary("notice-details-copy", "copy", "Copy all")
                    .on_click(cx.listener(move |s, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                        s.notify_user("Copied to the clipboard", s.theme.accent, cx);
                    })),
            )
    }

    // Returns the buttons of a notice: its details for an error or a warning, then its action.
    fn notice_buttons(&self, notice: &Notice, place: &'static str, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let id = notice.id;
        let action = notice.action.clone().map(|action| {
            let key = (SharedString::from(format!("{place}-action")), id as usize);
            match action {
                NoticeAction::ChooseAccount => self
                    .button(key, "users", "Choose account")
                    .on_click(cx.listener(|s, _, window, cx| {
                        cx.stop_propagation();
                        s.open_agent_menu(Panel::Models, window, cx)
                    })),
                NoticeAction::SignIn { provider, id } => {
                    let signing_in = self
                        .composer_account()
                        .is_some_and(|state| state.status.signing_in);
                    self.primary(key, "log-in", "Sign in")
                        .disabled(signing_in)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            cx.stop_propagation();
                            s.send(core::Request::SignInAccount {
                                provider,
                                id: id.clone(),
                            })
                        }))
                }
                NoticeAction::CheckHistory(task_id) => self
                    .button(key, "refresh", "Check history")
                    .on_click(cx.listener(move |s, _, _, cx| {
                        cx.stop_propagation();
                        s.send(core::Request::Reconcile {
                            task_id: task_id.clone(),
                        })
                    })),
            }
            .h(px(24.))
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .when(notice.kind.problem(), |row| {
                row.child(
                    self.ghost(
                        (SharedString::from(format!("{place}-details")), id as usize),
                        "file-text",
                        "Details",
                    )
                    .h(px(24.))
                    .px(px(8.))
                    .border_color(t.border)
                    .on_click(cx.listener(move |s, _, window, cx| {
                        cx.stop_propagation();
                        s.open_notice_details(id, window, cx);
                    })),
                )
            })
            .children(action)
    }

    /// Opens the notification center beside its bell and marks every kept notice as seen.
    fn open_notifications(&mut self, bounds: Bounds<Pixels>) {
        self.notices.seen_before = self.notices.seen;
        self.notices.seen = self.notices.sequence;
        self.panel = Panel::Notifications;
        self.menu_anchor = Some(bounds);
    }

    /// Returns the bell that opens the notification center. While the center is closed, a badge
    /// counts the kept notices it has not shown, in red when one of them is a failure.
    pub(super) fn notification_bell(&self, id: &'static str, cx: &mut Context<Self>) -> Button {
        let t = self.theme;
        let open = self.panel == Panel::Notifications;
        let (unread, failed) = if open {
            (0, false)
        } else {
            self.notices.unread()
        };
        let tone = if failed { t.removed } else { t.accent };
        self.icon_button(id, "bell", "Notifications")
            .when(unread > 0, |bell| {
                bell.accessibility_label(format!("Notifications, {unread} new"))
                    .child(
                        div()
                            .absolute()
                            .top(px(3.))
                            .right(px(2.))
                            .min_w(px(12.))
                            .h(px(12.))
                            .px(px(2.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(3.))
                            .bg(tone)
                            .shadow(kit::tint_light(tone))
                            .text_color(t.on_accent)
                            .font_family(crate::theme::mono())
                            .text_size(text_px(8.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(relative(1.))
                            .child(if unread > 9 {
                                "9+".to_owned()
                            } else {
                                unread.to_string()
                            }),
                    )
            })
            .when(open, |bell| {
                bell.bg(t.accent.opacity(0.1)).text_color(t.accent)
            })
            .on_click_with_bounds(cx.listener(|s, bounds: &Bounds<Pixels>, _, cx| {
                s.open_notifications(*bounds);
                cx.notify();
            }))
    }

    /// Returns the floating cards in the window's top right corner: the provider update card
    /// first, then the notices newest first, or nothing while no card floats.
    pub(super) fn floating_notices(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let update = self.harness_update_card(cx);
        let cards: Vec<AnyElement> = self
            .notices
            .list
            .iter()
            .rev()
            .filter(|notice| notice.float != Float::Gone)
            .map(|notice| self.notice_card(notice, cx))
            .collect();
        if update.is_none() && cards.is_empty() {
            return None;
        }
        Some(
            div()
                .absolute()
                .top(gpui::px(chrome::TITLE_BAR) + px(8.))
                .right(px(14.))
                .flex()
                .flex_col()
                .items_end()
                .gap(px(8.))
                .children(update)
                .children(cards)
                .into_any_element(),
        )
    }

    // A floating card. A confirmation is one compact line; a kept notice is a card with its
    // label, its title, its text, its buttons and a dismiss button. Both are glass that stands a
    // little off the window with the depth of a raised control and carry a faint dithered wash
    // of their tone along the leading edge.
    fn notice_card(&self, notice: &Notice, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let tone = notice.kind.tone(t);
        let id = notice.id;
        let compact = !notice.kind.kept();
        let radius = if compact {
            crate::theme::RADIUS_CONTROL
        } else {
            crate::theme::RADIUS_SURFACE
        };
        let mut light = kit::raised_light(t);
        light.extend(t.floating());
        let surface = kit::glass(div(), t.raised, kit::SURFACE_GLASS_FILL)
            .id(("notice-card", id as usize))
            .occlude()
            .relative()
            .overflow_hidden()
            .flex()
            .rounded(px(radius))
            .border_1()
            .border_color(tone.opacity(0.4))
            .shadow(light)
            .layer(kit::shade(tone.opacity(0.15), dither::Edge::Left));
        let card = if compact {
            surface
                .items_center()
                .gap(px(8.))
                .h(px(32.))
                .pl(px(6.))
                .pr(px(12.))
                .child(kit::icon_tile(notice.kind.glyph(), tone, 20., false))
                .child(
                    div()
                        .font_family(crate::theme::mono())
                        .text_size(text_px(11.5))
                        .text_color(t.text)
                        .child(notice.text.clone()),
                )
        } else {
            let buttons = (notice.kind.problem() || notice.action.is_some())
                .then(|| self.notice_buttons(notice, "notice-card", cx).pt(px(4.)));
            surface
                .w(px(CARD_WIDTH))
                .items_start()
                .gap(px(10.))
                .pl(px(10.))
                .pr(px(4.))
                .py(px(9.))
                .child(kit::icon_tile(notice.kind.glyph(), tone, 24., false))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pt(px(1.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(caps(notice.kind.label(), t).text_color(tone))
                        .when_some(notice.title.clone(), |column, title| {
                            column.child(
                                div()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .text_size(text_px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.text)
                                    .child(title),
                            )
                        })
                        .child(
                            div()
                                .text_size(text_px(12.))
                                .text_color(if notice.title.is_some() {
                                    t.muted
                                } else {
                                    t.text
                                })
                                .line_clamp(TEXT_LINES)
                                .child(notice.text.clone()),
                        )
                        .children(buttons),
                )
                .child(
                    self.icon_button(("dismiss-notice", id as usize), "close", "Dismiss")
                        .size(px(22.))
                        .on_click(cx.listener(move |s, _, _, cx| s.dismiss_notice(id, cx))),
                )
        };
        let card = div()
            .relative()
            .child(dither::shadow(t, radius, 5.))
            .child(card);
        if notice.float == Float::Leaving {
            motion::notice_leave(("notice-leave", id as usize), card, t, radius)
        } else {
            motion::notice_enter(("notice-enter", id as usize), card, t, radius)
        }
    }

    /// Returns the content of the notification center: kept notices newest first, each with
    /// its age. An entry about a task that still exists opens it.
    pub(super) fn notification_center(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let kept: Vec<&Notice> = self
            .notices
            .list
            .iter()
            .rev()
            .filter(|notice| notice.kind.kept())
            .collect();
        let header = kit::section_header("Notifications", Some(kept.len()), t).when(
            !kept.is_empty(),
            |header| {
                header.child(
                    self.ghost("notices-clear", "trash-2", "Clear")
                        .h(px(20.))
                        .px(px(6.))
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.notices.list.retain(|notice| notice.float != Float::Gone);
                            cx.notify();
                        })),
                )
            },
        );
        let mut center = div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(div().px(px(8.)).pt(px(6.)).pb(px(4.)).child(header));
        if kept.is_empty() {
            return center.child(kit::empty_hint("No notifications yet", t));
        }
        for notice in kept {
            let tone = notice.kind.tone(t);
            let new = notice.id > self.notices.seen_before;
            let task = notice.task.clone().filter(|id| {
                self.workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.tasks.iter().any(|task| &task.id == id))
            });
            let buttons = notice
                .kind
                .problem()
                .then(|| self.notice_buttons(notice, "notice-entry", cx).pt(px(4.)));
            center = center.child(
                div()
                    .id(("notice-entry", notice.id as usize))
                    .relative()
                    .overflow_hidden()
                    .flex()
                    .items_start()
                    .gap(px(9.))
                    .px(px(8.))
                    .py(px(7.))
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .when(new, |entry| {
                        entry
                            .bg(tone.opacity(0.06))
                            .child(kit::selection_edge(tone))
                    })
                    .child(kit::icon_tile(notice.kind.glyph(), tone, 20., false))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(caps(notice.kind.label(), t).text_color(tone))
                                    .child(div().flex_1())
                                    .child(figure(
                                        usage::duration(notice.at.elapsed().as_secs()),
                                        t.subtle,
                                    )),
                            )
                            .when_some(notice.title.clone(), |column, title| {
                                column.child(
                                    div()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .text_size(text_px(12.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(t.text)
                                        .child(title),
                                )
                            })
                            .child(
                                div()
                                    .text_size(text_px(12.))
                                    .text_color(if notice.title.is_some() {
                                        t.muted
                                    } else {
                                        t.text
                                    })
                                    .line_clamp(TEXT_LINES)
                                    .child(notice.text.clone()),
                            )
                            .children(buttons),
                    )
                    .when_some(task, |entry, task| {
                        entry
                            .cursor_pointer()
                            .hover(move |entry| entry.bg(t.selected))
                            .active(move |entry| entry.bg(t.control_active))
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.panel = Panel::None;
                                s.close_git(cx);
                                s.close_plugins(cx);
                                s.select_task(task.clone(), cx);
                                cx.notify();
                            }))
                    }),
            );
        }
        center
    }
}
