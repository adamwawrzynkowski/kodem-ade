//! A surface that opens below its trigger, such as the Fetch, Pull and Push options or the
//! notification center. Its owner keeps the open state; the popover reports the user's changes.
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    Anchor, AnyElement, App, Bounds, ElementId, FocusHandle, KeyDownEvent, MouseDownEvent, Pixels,
    StyleRefinement, Styled, Window, anchored, canvas, deferred, div, point,
};

use crate::theme::px;

/// Paint order of popovers: above the content, below dialogs and menus.
const POPOVER_PRIORITY: usize = 15;

type ContentBuilder = Rc<dyn Fn(&mut PopoverFrame, &mut Window, &mut App) -> AnyElement>;
type OpenChange = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

/// The argument that a popover's content builder receives.
pub struct PopoverFrame;

/// A trigger with a surface that opens below it.
#[derive(IntoElement)]
pub struct Popover {
    id: ElementId,
    style: StyleRefinement,
    open: bool,
    trigger: Option<AnyElement>,
    content: Option<ContentBuilder>,
    on_open_change: Option<OpenChange>,
    focus: Option<FocusHandle>,
    beside: bool,
}

impl Popover {
    /// Creates a closed popover.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            open: false,
            trigger: None,
            content: None,
            on_open_change: None,
            focus: None,
            beside: false,
        }
    }

    /// Opens the surface beside the trigger, toward the window's trailing edge and aligned with
    /// the trigger's bottom, as in a rail along the window's leading edge.
    pub fn beside(mut self) -> Self {
        self.beside = true;
        self
    }

    /// Kept for the views' builders: the content draws its own surface.
    pub fn appearance(self, _: bool) -> Self {
        self
    }

    /// Sets whether the surface is open.
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Sets the element that opens and closes the surface.
    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    /// Sets what the surface shows.
    pub fn content<E: IntoElement>(
        mut self,
        content: impl Fn(&mut PopoverFrame, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.content = Some(Rc::new(move |frame, window, cx| {
            content(frame, window, cx).into_any_element()
        }));
        self
    }

    /// Calls `handler` with the state that the user asks for: a click on the trigger toggles
    /// it, and a press outside the surface or Escape closes it.
    pub fn on_open_change(
        mut self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(handler));
        self
    }

    /// Moves keyboard focus to `focus` when the surface opens, such as to a search field in it.
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }
}

impl Styled for Popover {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Popover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let trigger_bounds = window.use_keyed_state(
            ElementId::Name(format!("git-popover-trigger-{:?}", self.id).into()),
            cx,
            |_, _| None::<Bounds<Pixels>>,
        );
        let open = self.open;
        // Focus moves into the surface once, when it opens.
        let was_open = window.use_keyed_state(
            ElementId::Name(format!("git-popover-open-{:?}", self.id).into()),
            cx,
            |_, _| false,
        );
        // Without a field to focus, the surface takes focus itself, so that Escape reaches it.
        let own_focus = window.use_keyed_state(
            ElementId::Name(format!("git-popover-focus-{:?}", self.id).into()),
            cx,
            |_, cx| cx.focus_handle(),
        );
        let surface_focus = self.focus.is_none().then(|| own_focus.read(cx).clone());
        if open
            && !*was_open.read(cx)
            && let Some(focus) = self.focus.as_ref().or(surface_focus.as_ref())
        {
            window.focus(focus, cx);
        }
        was_open.update(cx, |state, _| *state = open);
        let toggle = self.on_open_change.clone();
        let mut container = div().id(self.id.clone()).relative();
        container.style().refine(&self.style);
        let surface = if open {
            let bounds = *trigger_bounds.read(cx);
            match (bounds, self.content.clone()) {
                (Some(bounds), Some(content)) => {
                    let close = self.on_open_change.clone();
                    let escape = self.on_open_change.clone();
                    let content = content(&mut PopoverFrame, window, cx);
                    let body = div()
                        .id("git-popover-content")
                        .when_some(surface_focus.clone(), |body, focus| {
                            body.track_focus(&focus)
                        })
                        .occlude()
                        .on_mouse_down_out(move |event: &MouseDownEvent, window, cx| {
                            // A press on the trigger toggles the popover there.
                            if bounds.contains(&event.position) {
                                return;
                            }
                            if let Some(close) = &close {
                                close(&false, window, cx);
                            }
                        })
                        .on_key_down(move |event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                if let Some(escape) = &escape {
                                    escape(&false, window, cx);
                                }
                            }
                        })
                        .child(content);
                    let (position, anchor) = if self.beside {
                        (
                            point(bounds.right() + px(8.), bounds.bottom()),
                            Anchor::BottomLeft,
                        )
                    } else {
                        (
                            point(bounds.left(), bounds.bottom() + px(6.)),
                            Anchor::TopLeft,
                        )
                    };
                    Some(
                        deferred(
                            anchored()
                                .position(position)
                                .anchor(anchor)
                                .snap_to_window_with_margin(px(8.))
                                .child(body),
                        )
                        .with_priority(POPOVER_PRIORITY),
                    )
                }
                _ => None,
            }
        } else {
            None
        };
        let measure = trigger_bounds.clone();
        let trigger = div()
            .id("git-popover-trigger")
            .relative()
            .when_some(self.trigger, |trigger, element| trigger.child(element))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        measure.update(cx, |state, _| *state = Some(bounds));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .on_click(move |_, window, cx| {
                if let Some(toggle) = &toggle {
                    toggle(&!open, window, cx);
                }
            });
        container.child(trigger).children(surface)
    }
}
