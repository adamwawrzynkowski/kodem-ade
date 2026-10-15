//! Dialogs, toasts and tooltips of the Git workspace. Dialogs and toasts live on a window
//! layer that the window root draws above everything else, so a dialog that the Git settings
//! open in Preferences shows as well as one that the Git workspace opens.
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Anchor, AnyElement, AnyView, App, FocusHandle, Global, KeyDownEvent, MouseButton,
    ParentElement, SharedString, StyleRefinement, Styled, Window, deferred, div,
};

use crate::theme::px;
use crate::ui::dither;
use crate::ui::git::theme::ActiveTheme;

/// Paint order of dialogs above the window's content and below popup menus.
const DIALOG_PRIORITY: usize = 20;
/// Paint order of toasts.
const NOTIFICATION_PRIORITY: usize = 10;
/// How long a toast stays before it leaves on its own.
const NOTIFICATION_LIFETIME: Duration = Duration::from_secs(5);

type DialogBuilder = Rc<dyn Fn(Dialog, &mut Window, &mut App) -> Dialog>;
type NotificationContent = Rc<dyn Fn(&mut NotificationFrame, &mut Window, &mut App) -> AnyElement>;

/// Returns a tooltip builder that shows `text` in Kodem's hint style.
pub fn tooltip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView {
    let text: SharedString = text.into();
    move |_, cx| {
        let theme = cx.theme().kodem;
        let text = text.to_string();
        cx.new(|_| crate::ui::Hint { text, theme }).into()
    }
}

/// Builds the tooltip of an element.
pub struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    /// Creates a tooltip with a line of text.
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }

    /// Returns the tooltip's view.
    pub fn build(self, window: &mut Window, cx: &mut App) -> AnyView {
        tooltip(self.text)(window, cx)
    }

    /// Returns a builder of the tooltip for an element's `tooltip`.
    pub fn into_builder(self) -> impl Fn(&mut Window, &mut App) -> AnyView {
        tooltip(self.text)
    }
}

/// The frame of a dialog: a surface of a set width in the middle of the window, over a scrim.
pub struct Dialog {
    style: StyleRefinement,
    children: Vec<AnyElement>,
    overlay_closable: bool,
}

impl Dialog {
    fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            children: Vec::new(),
            overlay_closable: true,
        }
    }

    /// Kept for the dialogs' builders: Git dialogs draw their own close control.
    pub fn close_button(self, _: bool) -> Self {
        self
    }

    /// Sets whether a press on the scrim closes the dialog.
    pub fn overlay_closable(mut self, closable: bool) -> Self {
        self.overlay_closable = closable;
        self
    }
}

impl Styled for Dialog {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The argument that a toast's content builder receives.
pub struct NotificationFrame;

/// A toast: a short message that slides in at a corner of the window and leaves on its own.
pub struct Notification {
    key: Option<SharedString>,
    content: Option<NotificationContent>,
    placement: Anchor,
}

impl Default for Notification {
    fn default() -> Self {
        Self::new()
    }
}

impl Notification {
    /// Creates an empty toast.
    pub fn new() -> Self {
        Self {
            key: None,
            content: None,
            placement: Anchor::BottomLeft,
        }
    }

    /// Sets what the toast shows.
    pub fn content(
        mut self,
        content: impl Fn(&mut NotificationFrame, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.content = Some(Rc::new(content));
        self
    }

    /// Names the toast by a type and a key. A new toast with the same name replaces it.
    pub fn id1<T: 'static>(mut self, key: impl Into<SharedString>) -> Self {
        self.key = Some(notification_key::<T>(key.into()));
        self
    }

    /// Sets the corner of the window where the toast shows.
    pub fn placement(mut self, placement: Anchor) -> Self {
        self.placement = placement;
        self
    }
}

fn notification_key<T: 'static>(key: SharedString) -> SharedString {
    SharedString::from(format!("{}:{key}", std::any::type_name::<T>()))
}

/// The builder and the focus of an open dialog, copied out of the overlay state for a draw.
type OpenDialog = (DialogBuilder, FocusHandle);
/// The id, the content and the corner of a toast, copied out of the overlay state for a draw.
type OpenNotification = (u64, NotificationContent, Anchor);

struct DialogEntry {
    builder: DialogBuilder,
    /// Holds keyboard focus while the dialog is open, so that Escape reaches it.
    focus: FocusHandle,
    /// Where focus was before the dialog opened; it returns there when the dialog closes.
    previous: Option<FocusHandle>,
}

struct NotificationEntry {
    id: u64,
    key: Option<SharedString>,
    content: NotificationContent,
    placement: Anchor,
}

/// The open dialogs and toasts of the window.
#[derive(Default)]
struct GitOverlays {
    dialogs: Vec<DialogEntry>,
    notifications: Vec<NotificationEntry>,
    next_id: u64,
}

impl Global for GitOverlays {}

fn layers(cx: &mut App) -> &mut GitOverlays {
    if !cx.has_global::<GitOverlays>() {
        cx.set_global(GitOverlays::default());
    }
    cx.global_mut::<GitOverlays>()
}

/// Opens and closes the Git workspace's dialogs and toasts on a window.
pub trait WindowExt {
    /// Opens a dialog above the window. `build` fills its frame each time it draws.
    fn open_dialog(
        &mut self,
        cx: &mut App,
        build: impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
    );

    /// Closes the topmost dialog.
    fn close_dialog(&mut self, cx: &mut App);

    /// Shows a toast. A toast with the same name leaves first.
    fn push_notification(&mut self, notification: Notification, cx: &mut App);

    /// Removes the toast with a type and a key.
    fn remove_notification1<T: 'static>(&mut self, key: impl Into<SharedString>, cx: &mut App);
}

impl WindowExt for Window {
    fn open_dialog(
        &mut self,
        cx: &mut App,
        build: impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
    ) {
        let focus = cx.focus_handle();
        let previous = self.focused(cx);
        self.focus(&focus, cx);
        layers(cx).dialogs.push(DialogEntry {
            builder: Rc::new(build),
            focus,
            previous,
        });
        self.refresh();
    }

    fn close_dialog(&mut self, cx: &mut App) {
        if let Some(previous) = layers(cx).dialogs.pop().and_then(|entry| entry.previous) {
            self.focus(&previous, cx);
        }
        self.refresh();
    }

    fn push_notification(&mut self, notification: Notification, cx: &mut App) {
        let Some(content) = notification.content else {
            return;
        };
        let state = layers(cx);
        if let Some(key) = &notification.key {
            state
                .notifications
                .retain(|entry| entry.key.as_ref() != Some(key));
        }
        state.next_id += 1;
        let id = state.next_id;
        state.notifications.push(NotificationEntry {
            id,
            key: notification.key,
            content,
            placement: notification.placement,
        });
        self.refresh();
        let handle = self.window_handle();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(NOTIFICATION_LIFETIME).await;
            let _ = handle.update(cx, |_, window, cx| {
                layers(cx).notifications.retain(|entry| entry.id != id);
                window.refresh();
            });
        })
        .detach();
    }

    fn remove_notification1<T: 'static>(&mut self, key: impl Into<SharedString>, cx: &mut App) {
        let key = notification_key::<T>(key.into());
        layers(cx)
            .notifications
            .retain(|entry| entry.key.as_ref() != Some(&key));
        self.refresh();
    }
}

/// Draws the open dialogs and toasts. The window root adds it as its last child.
pub fn overlays(window: &mut Window, cx: &mut App) -> AnyElement {
    let (dialogs, notifications): (Vec<OpenDialog>, Vec<OpenNotification>) = {
        let state = layers(cx);
        (
            state
                .dialogs
                .iter()
                .map(|entry| (entry.builder.clone(), entry.focus.clone()))
                .collect(),
            state
                .notifications
                .iter()
                .map(|entry| (entry.id, entry.content.clone(), entry.placement))
                .collect(),
        )
    };
    let theme = cx.theme();
    let t = theme.kodem;
    let count = dialogs.len();
    let mut layer = div().absolute().inset_0().size_full();
    for (index, (builder, focus)) in dialogs.into_iter().enumerate() {
        let dialog = builder(Dialog::new(), window, cx);
        let closable = dialog.overlay_closable;
        let mut frame = div()
            .relative()
            .max_w_full()
            .max_h_full()
            .flex()
            .flex_col()
            .bg(t.raised)
            .border_1()
            .border_color(t.edge)
            .rounded(px(crate::theme::RADIUS_FLOATING))
            .shadow(t.floating())
            .overflow_hidden();
        frame.style().refine(&dialog.style);
        let frame = frame
            .id(("git-dialog", index))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(dialog.children);
        let top = index + 1 == count;
        layer = layer.child(
            div()
                .id(("git-dialog-layer", index))
                .absolute()
                .inset_0()
                .occlude()
                .child(dither::scrim(t))
                .flex()
                .items_center()
                .justify_center()
                .p(px(24.))
                .when(top, |layer| {
                    layer
                        .track_focus(&focus)
                        .on_key_down(|event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                window.close_dialog(cx);
                            }
                        })
                        .when(closable, |layer| {
                            layer.on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.close_dialog(cx);
                            })
                        })
                })
                .child(
                    div()
                        .relative()
                        .child(dither::shadow(t, crate::theme::RADIUS_FLOATING, 7.))
                        .child(crate::ui::motion::floating(
                            ("git-dialog-enter", index),
                            frame,
                            t,
                        )),
                ),
        );
    }
    let mut corners: Vec<(Anchor, Vec<AnyElement>)> = Vec::new();
    for (id, content, placement) in notifications {
        // A click on a toast dismisses it before its time is up.
        let element = div()
            .id(("git-toast", id as usize))
            .cursor_pointer()
            .on_click(move |_, window, cx| {
                layers(cx).notifications.retain(|entry| entry.id != id);
                window.refresh();
            })
            .child(content(&mut NotificationFrame, window, cx))
            .into_any_element();
        match corners.iter_mut().find(|(corner, _)| *corner == placement) {
            Some((_, elements)) => elements.push(element),
            None => corners.push((placement, vec![element])),
        }
    }
    let toasts = corners.into_iter().map(|(placement, elements)| {
        let stack = div()
            .absolute()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(16.))
            .children(elements);
        match placement {
            Anchor::TopLeft => stack.top(px(40.)).left_0(),
            Anchor::TopRight => stack.top(px(40.)).right_0().items_end(),
            Anchor::BottomRight => stack.bottom_0().right_0().items_end(),
            _ => stack.bottom_0().left_0(),
        }
    });
    let toast_layer = div().absolute().inset_0().size_full().children(toasts);
    div()
        .absolute()
        .inset_0()
        .size_full()
        .child(deferred(toast_layer).with_priority(NOTIFICATION_PRIORITY))
        .when(count > 0, |root| {
            root.child(deferred(layer).with_priority(DIALOG_PRIORITY))
        })
        .into_any_element()
}
