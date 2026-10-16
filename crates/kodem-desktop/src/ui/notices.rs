//! Notices: cards that float in the window's top right corner and the notification center that
//! keeps errors and task events.
use super::*;
use std::time::Instant;

/// How long a confirmation floats before it leaves.
const CONFIRMATION_FLOAT: Duration = Duration::from_millis(3600);
/// The most cards that float at once. A new card sends the oldest away.
const MAX_FLOATING: usize = 3;
/// The most notices that the notification center keeps.
const MAX_KEPT: usize = 50;
/// The width of an error card.
const CARD_WIDTH: f32 = 340.;

/// What a notice reports. It sets the notice's label, glyph and tone, how long its card floats
/// and whether the notification center keeps it.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum NoticeKind {
    /// A short confirmation in a tone of its caller. It floats briefly and is not kept.
    Confirmation(Hsla),
    /// An error. Its card floats while it is the current error.
    Error,
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
            Self::Finished => "Finished",
            Self::NeedsInput => "Needs you",
            Self::Blocked => "Blocked",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Confirmation(_) => "check",
            Self::Error => "triangle-alert",
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
            Self::Finished => t.added,
            Self::NeedsInput => t.warning,
        }
    }

    fn kept(self) -> bool {
        !matches!(self, Self::Confirmation(_))
    }
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
    at: Instant,
    float: Float,
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

impl Shell {
    /// Shows a short confirmation that floats for a few seconds and is not kept.
    pub(super) fn notify_user(
        &mut self,
        text: impl Into<SharedString>,
        tone: Hsla,
        cx: &mut Context<Self>,
    ) {
        self.push_notice(NoticeKind::Confirmation(tone), text.into(), None, true, cx);
    }

    /// Reports an error. Dialogs show it inline, its card floats until it is dismissed, resolved
    /// or replaced, and the notification center keeps it.
    pub(super) fn report_error(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let text = text.into();
        // The same error reported again while its card floats does not stack a second card.
        let floating = self.notices.list.iter().any(|notice| {
            notice.kind == NoticeKind::Error
                && notice.float == Float::Shown
                && notice.text.as_ref() == text
        });
        if !floating {
            self.push_notice(NoticeKind::Error, text.clone().into(), None, true, cx);
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
        self.push_notice(kind, title.into(), Some(task_id), false, cx);
    }

    fn push_notice(
        &mut self,
        kind: NoticeKind,
        text: SharedString,
        task: Option<String>,
        float: bool,
        cx: &mut Context<Self>,
    ) {
        self.notices.sequence += 1;
        let id = self.notices.sequence;
        self.notices.list.push(Notice {
            id,
            kind,
            text,
            task,
            at: Instant::now(),
            float: if float { Float::Shown } else { Float::Gone },
        });
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
        if notice.kind == NoticeKind::Error && self.error.as_deref() == Some(notice.text.as_ref()) {
            self.error = None;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(motion::NOTICE_LEAVE).await;
            let _ = this.update(cx, |s, cx| {
                if let Some(notice) = s.notices.list.iter_mut().find(|notice| notice.id == id) {
                    notice.float = Float::Gone;
                }
                s.notices.prune();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Sends away the error cards whose error was resolved or replaced, so only the current
    /// error floats. Call it once per render.
    pub(super) fn sync_error_notices(&mut self, cx: &mut Context<Self>) {
        let stale: Vec<u64> = self
            .notices
            .list
            .iter()
            .filter(|notice| {
                notice.kind == NoticeKind::Error
                    && notice.float == Float::Shown
                    && self.error.as_deref() != Some(notice.text.as_ref())
            })
            .map(|notice| notice.id)
            .collect();
        for id in stale {
            self.dismiss_notice(id, cx);
        }
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

    /// Returns the floating cards in the window's top right corner, newest first, or nothing
    /// while no card floats.
    pub(super) fn floating_notices(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mut cards = self
            .notices
            .list
            .iter()
            .rev()
            .filter(|notice| notice.float != Float::Gone)
            .peekable();
        cards.peek()?;
        let cards: Vec<AnyElement> = cards.map(|notice| self.notice_card(notice, cx)).collect();
        Some(
            div()
                .absolute()
                .top(gpui::px(chrome::TITLE_BAR) + px(8.))
                .right(px(14.))
                .flex()
                .flex_col()
                .items_end()
                .gap(px(8.))
                .children(cards)
                .into_any_element(),
        )
    }

    // A floating card. A confirmation is one compact line; an error is a card with its label,
    // its text and a dismiss button. Both are glass that stands a little off the window with the
    // depth of a raised control and carry a faint dithered wash of their tone along the leading
    // edge.
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
                        .child(
                            div()
                                .text_size(text_px(12.))
                                .text_color(t.text)
                                .child(notice.text.clone()),
                        ),
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
                            .child(
                                div()
                                    .text_size(text_px(12.))
                                    .text_color(t.text)
                                    .child(notice.text.clone()),
                            ),
                    )
                    .when_some(task, |entry, task| {
                        entry
                            .cursor_pointer()
                            .hover(move |entry| entry.bg(t.selected))
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.panel = Panel::None;
                                s.close_git(cx);
                                s.select_task(task.clone(), cx);
                                cx.notify();
                            }))
                    }),
            );
        }
        center
    }
}
