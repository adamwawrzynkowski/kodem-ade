//! Text fields of the Git workspace: one-line fields for names, paths and searches, and
//! multi-line fields for commit descriptions and prompts. Each wraps Kodem's text input, so
//! editing, selection, IME and undo behave as everywhere else in the window.
use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, RenderOnce, SharedString, StyleRefinement, Styled, Subscription, Window, div,
};

use super::{Sizable, Size};
use crate::components::text_input::{SubmitInput, TextInput};
use crate::theme::{px, text_px};
use crate::ui::git::theme::ActiveTheme;

/// What a text field reports to its owner.
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// The text changed.
    Change,
    /// The user pressed Return.
    PressEnter,
}

/// The state of a one-line text field.
pub struct InputState {
    input: Entity<TextInput>,
    /// Settings from the builder that wait for the first draw, where the input can change.
    pending: Pending,
    last_text: String,
    multi_line: bool,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
struct Pending {
    placeholder: Option<SharedString>,
    value: Option<SharedString>,
    masked: Option<bool>,
    multi_line: bool,
}

impl EventEmitter<InputEvent> for InputState {}

impl Focusable for InputState {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl InputState {
    /// Creates an empty one-line field.
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut input = TextInput::new("", cx);
            input.set_single_line(true);
            input
        });
        let subscriptions = vec![
            cx.observe(&input, |this, input, cx| {
                let text = input.read(cx).text();
                if text != this.last_text {
                    this.last_text = text;
                    cx.emit(InputEvent::Change);
                }
                cx.notify();
            }),
            cx.subscribe(&input, |_, _, _: &SubmitInput, cx| {
                cx.emit(InputEvent::PressEnter);
            }),
        ];
        Self {
            input,
            pending: Pending::default(),
            last_text: String::new(),
            multi_line: false,
            _subscriptions: subscriptions,
        }
    }

    /// Sets the hint that shows while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.pending.placeholder = Some(placeholder.into());
        self
    }

    /// Sets the first text of the field.
    pub fn default_value(mut self, value: impl Into<SharedString>) -> Self {
        let value = value.into();
        self.last_text = value.to_string();
        self.pending.value = Some(value);
        self
    }

    /// Makes the field hold several lines: Return starts a new line.
    pub fn multi_line(mut self) -> Self {
        self.multi_line = true;
        self.pending.multi_line = true;
        self
    }

    /// Hides the text, for tokens and passwords.
    pub fn masked(mut self, masked: bool) -> Self {
        self.pending.masked = Some(masked);
        self
    }

    /// Returns the text.
    pub fn value(&self) -> SharedString {
        match &self.pending.value {
            Some(value) => value.clone(),
            None => SharedString::from(self.last_text.clone()),
        }
    }

    /// Replaces the text.
    pub fn set_value(
        &mut self,
        value: impl Into<SharedString>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value: SharedString = value.into();
        self.pending.value = None;
        self.last_text = value.to_string();
        self.input
            .update(cx, |input, cx| input.set_text(value.to_string(), cx));
        cx.notify();
    }

    // Applies the builder's settings to the input before it draws.
    fn apply_pending(&mut self, cx: &mut Context<Self>) {
        let pending = std::mem::take(&mut self.pending);
        if pending.placeholder.is_none()
            && pending.value.is_none()
            && pending.masked.is_none()
            && !pending.multi_line
        {
            return;
        }
        self.input.update(cx, |input, cx| {
            if pending.multi_line {
                input.set_single_line(false);
            }
            if let Some(placeholder) = &pending.placeholder {
                input.set_placeholder(placeholder, cx);
            }
            if let Some(value) = &pending.value {
                input.set_text(value.to_string(), cx);
            }
            if let Some(masked) = pending.masked {
                input.set_masked(masked, cx);
            }
        });
    }
}

/// The state of a multi-line text field: an [`InputState`] built with
/// [`InputState::multi_line`].
pub type TextareaState = InputState;

/// A text field on an inset well with the focus edge.
#[derive(IntoElement)]
pub struct Input {
    state: Entity<InputState>,
    style: StyleRefinement,
    size: Size,
    prefix: Option<AnyElement>,
    font_family: Option<SharedString>,
}

impl Input {
    /// Creates a field for a state.
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
            size: Size::Medium,
            prefix: None,
            font_family: None,
        }
    }

    /// Sets the typeface of the text.
    pub fn font_family(mut self, family: impl Into<SharedString>) -> Self {
        self.font_family = Some(family.into());
        self
    }

    /// Shows an element before the text, such as a search icon.
    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.prefix = Some(prefix.into_any_element());
        self
    }

    /// Kept for the views' builders: the field draws no clear button.
    pub fn cleanable(self) -> Self {
        self
    }
}

impl Sizable for Input {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let t = theme.kodem;
        let (input, multi_line) = self.state.update(cx, |state, cx| {
            state.apply_pending(cx);
            (state.input.clone(), state.multi_line)
        });
        let focused = input.read(cx).focus_handle(cx).contains_focused(window, cx);
        let (height, text) = match self.size {
            Size::XSmall => (24., 11.),
            Size::Small => (28., 11.5),
            Size::Medium | Size::Points(_) => (30., 12.),
        };
        let mut field = div()
            .relative()
            .w_full()
            .flex()
            .gap(px(6.))
            .px(px(8.))
            .text_size(text_px(text))
            .text_color(t.text)
            .when_some(self.font_family, |field, family| field.font_family(family))
            .when(!multi_line, |field| field.min_h(px(height)).items_center())
            .when(multi_line, |field| {
                field.items_start().py(px(4.)).overflow_hidden()
            })
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .bg(t.inset)
            .border_1()
            .border_color(if focused {
                t.accent.opacity(0.6)
            } else {
                t.edge
            });
        field.style().refine(&self.style);
        field.children(self.prefix).child(if multi_line {
            // A field of a set height scrolls its lines instead of growing past it.
            div()
                .id(("git-input-lines", self.state.entity_id().as_u64() as usize))
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_y_scroll()
                .child(input)
                .into_any_element()
        } else {
            div().flex_1().min_w_0().child(input).into_any_element()
        })
    }
}

/// A multi-line text field.
pub struct Textarea;

impl Textarea {
    /// Creates a field for a multi-line state.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(state: &Entity<InputState>) -> Input {
        Input::new(state)
    }
}
