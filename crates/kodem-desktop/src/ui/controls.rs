//! Interactive controls drawn in the dithered style: a stepped slider and an on/off switch.
//!
//! Both are ports of Liquid's controls to GPUI-CE and Kodem's own `Theme`.
use super::dither::{Dither, DitherBar, DitherField, Edge};
use super::*;
use std::rc::Rc;

/// The most steps that still get tick marks under the track.
const MAX_TICKS: f32 = 24.;
/// Height of the dot-matrix track: three rows of three-point cells.
const TRACK_HEIGHT: f32 = 9.;
const SLIDER_HEIGHT: f32 = 24.;
const THUMB_WIDTH: f32 = 8.;
const THUMB_HEIGHT: f32 = 16.;

type SliderCallback = Rc<dyn Fn(&f32, &mut Window, &mut App)>;
type ReleaseCallback = Rc<dyn Fn(&(), &mut Window, &mut App)>;
type SwitchCallback = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

// Drag state that lives across renders of one slider.
#[derive(Default)]
struct SliderDrag {
    dragging: bool,
    last_value: Option<f32>,
}

/// A stepped slider with a dot-matrix track. The filled part of the track uses the accent color.
///
/// Drag the thumb, click anywhere on the track, or use the arrow, Home and End keys.
#[derive(IntoElement)]
pub(super) struct Slider {
    id: SharedString,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    disabled: bool,
    label: SharedString,
    on_change: SliderCallback,
    on_release: Option<ReleaseCallback>,
}

impl Slider {
    /// Creates a slider. `on_change` receives each new stepped value.
    pub(super) fn new(
        id: impl Into<SharedString>,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
        on_change: impl Fn(&f32, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            value,
            min,
            max: max.max(min),
            step: step.max(f32::EPSILON),
            disabled: false,
            label: SharedString::default(),
            on_change: Rc::new(on_change),
            on_release: None,
        }
    }

    /// Stops the slider from changing.
    pub(super) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Sets the name exposed to accessibility clients.
    pub(super) fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Calls `handler` when the user settles on a value: at the end of a drag, and after each
    /// change from the keyboard. A setting that must not change during a drag applies there.
    pub(super) fn on_release(
        mut self,
        handler: impl Fn(&(), &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_release = Some(Rc::new(handler));
        self
    }
}

// Rounds a value to the nearest step inside the range.
fn snap_value(value: f32, min: f32, max: f32, step: f32) -> f32 {
    let steps = ((value - min) / step).round();
    (min + steps * step).clamp(min, max)
}

// Returns the share of the range that a value covers, from 0 to 1.
fn value_fraction(value: f32, min: f32, max: f32) -> f32 {
    if max <= min {
        return 0.;
    }
    ((value - min) / (max - min)).clamp(0., 1.)
}

// Returns the stepped value under a horizontal pointer position. The thumb's center follows
// the pointer, so the ends of the range sit half a thumb inside the track.
fn value_at(x: Pixels, bounds: Bounds<Pixels>, min: f32, max: f32, step: f32) -> f32 {
    let thumb_width = f32::from(px(THUMB_WIDTH));
    let travel = (f32::from(bounds.size.width) - thumb_width).max(1.);
    let offset = f32::from(x) - f32::from(bounds.origin.x) - thumb_width / 2.;
    let fraction = (offset / travel).clamp(0., 1.);
    snap_value(min + fraction * (max - min), min, max, step)
}

// Sends a value once, so that a drag does not repeat the same value.
fn emit_value(
    drag: &Entity<SliderDrag>,
    value: f32,
    on_change: &SliderCallback,
    window: &mut Window,
    cx: &mut App,
) {
    let changed = drag.update(cx, |drag, _| {
        if drag.last_value == Some(value) {
            false
        } else {
            drag.last_value = Some(value);
            true
        }
    });
    if changed {
        on_change(&value, window, cx);
    }
}

fn paint_thumb(
    bounds: Bounds<Pixels>,
    fraction: f32,
    ticks: Option<usize>,
    theme: Theme,
    window: &mut Window,
) {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let scale = window.scale_factor();
    let snap = |value: f32| (value * scale).round() / scale;
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    // The bounds are window pixels, so the parts of the slider are measured in them too.
    let (thumb_width, thumb_height) = (f32::from(px(THUMB_WIDTH)), f32::from(px(THUMB_HEIGHT)));
    let track_height = f32::from(px(TRACK_HEIGHT));
    if let Some(ticks) = ticks.filter(|ticks| *ticks > 1) {
        let tick_top = snap(top + (height + track_height) / 2. + 2.);
        for tick in 0..ticks {
            let x =
                left + thumb_width / 2. + tick as f32 / (ticks - 1) as f32 * (width - thumb_width)
                    - 0.5;
            window.paint_quad(fill(
                Bounds::new(
                    point(gpui::px(snap(x)), gpui::px(tick_top)),
                    size(gpui::px(1.), gpui::px(2.)),
                ),
                theme.subtle.opacity(0.7),
            ));
        }
    }
    let thumb = Bounds::new(
        point(
            gpui::px(snap(left + fraction * (width - thumb_width))),
            gpui::px(snap(top + (height - thumb_height) / 2.)),
        ),
        size(gpui::px(thumb_width), gpui::px(thumb_height)),
    );
    window.paint_quad(
        fill(thumb, theme.text)
            .corner_radii(px(2.5))
            .border_widths(gpui::px(1.))
            .border_color(theme.accent),
    );
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let drag = window.use_keyed_state(
            ElementId::Name(SharedString::from(format!("{}-drag", self.id))),
            cx,
            |_, _| SliderDrag::default(),
        );
        let focus = window
            .use_keyed_state(
                ElementId::Name(SharedString::from(format!("{}-focus", self.id))),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        let (min, max, step, value) = (self.min, self.max, self.step, self.value);
        let fraction = value_fraction(value, min, max);
        let steps = (max - min) / step;
        let ticks = (steps <= MAX_TICKS).then(|| steps.round() as usize + 1);
        let disabled = self.disabled;
        let on_change = self.on_change;
        let on_release = self.on_release;
        let key_change = on_change.clone();
        let key_release = on_release.clone();
        let press_focus = focus.clone();
        div()
            .id(ElementId::Name(self.id.clone()))
            .relative()
            .w_full()
            .min_w(px(80.))
            .h(px(SLIDER_HEIGHT))
            .role(Role::Slider)
            .aria_label(self.label)
            .aria_min_numeric_value(min as f64)
            .aria_max_numeric_value(max as f64)
            .aria_numeric_value(value as f64)
            .aria_numeric_value_step(step as f64)
            .opacity(if disabled { 0.45 } else { 1. })
            .when(!disabled, |slider| {
                slider.track_focus(&focus).cursor_pointer().on_key_down(
                    move |event: &KeyDownEvent, window, cx| {
                        let next = match event.keystroke.key.as_str() {
                            "left" | "down" => value - step,
                            "right" | "up" => value + step,
                            "home" => min,
                            "end" => max,
                            _ => return,
                        };
                        cx.stop_propagation();
                        let next = snap_value(next, min, max, step);
                        if next != value {
                            key_change(&next, window, cx);
                            if let Some(release) = &key_release {
                                release(&(), window, cx);
                            }
                        }
                    },
                )
            })
            .child(
                div()
                    .absolute()
                    .left(px(THUMB_WIDTH / 2.))
                    .right(px(THUMB_WIDTH / 2.))
                    .top(px((SLIDER_HEIGHT - TRACK_HEIGHT) / 2.))
                    .h(px(TRACK_HEIGHT))
                    .child(DitherBar::meter(
                        fraction,
                        theme.accent,
                        theme.subtle.opacity(0.7),
                    )),
            )
            .child(
                canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                    move |bounds, hitbox: Hitbox, window, _| {
                        paint_thumb(bounds, fraction, ticks, theme, window);
                        if disabled {
                            return;
                        }
                        let down_drag = drag.clone();
                        let down_change = on_change.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if !phase.bubble()
                                || event.button != MouseButton::Left
                                || !hitbox.is_hovered(window)
                            {
                                return;
                            }
                            window.focus(&press_focus, cx);
                            down_drag.update(cx, |drag, _| {
                                drag.dragging = true;
                                drag.last_value = Some(value);
                            });
                            let next = value_at(event.position.x, bounds, min, max, step);
                            emit_value(&down_drag, next, &down_change, window, cx);
                            cx.stop_propagation();
                        });
                        let move_drag = drag.clone();
                        let move_change = on_change.clone();
                        let move_release = on_release.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if !phase.bubble() || !move_drag.read(cx).dragging {
                                return;
                            }
                            if !event.dragging() {
                                move_drag.update(cx, |drag, _| drag.dragging = false);
                                if let Some(release) = &move_release {
                                    release(&(), window, cx);
                                }
                                return;
                            }
                            let next = value_at(event.position.x, bounds, min, max, step);
                            emit_value(&move_drag, next, &move_change, window, cx);
                        });
                        let up_drag = drag.clone();
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                            if phase.bubble() && up_drag.read(cx).dragging {
                                up_drag.update(cx, |drag, _| drag.dragging = false);
                                if let Some(release) = &on_release {
                                    release(&(), window, cx);
                                }
                            }
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
    }
}

/// Time the switch thumb takes to slide to the other side.
const SWITCH_DURATION: Duration = Duration::from_millis(160);

// Remembers the last value of one switch so that the thumb slides only when the value changes,
// not when the switch first appears.
#[derive(Default)]
struct SwitchMotion {
    previous: Option<bool>,
    sequence: usize,
}

impl SwitchMotion {
    // Records the current value and returns the animation sequence, which is zero until the
    // value changes for the first time.
    fn observe(&mut self, checked: bool) -> usize {
        if self.previous.is_some_and(|previous| previous != checked) {
            self.sequence = self.sequence.wrapping_add(1);
        }
        self.previous = Some(checked);
        self.sequence
    }
}

/// An on/off switch with a dot-matrix track. The track lights up in the accent color when the
/// switch is on, and the square thumb slides across.
#[derive(IntoElement)]
pub(super) struct Switch {
    id: SharedString,
    checked: bool,
    disabled: bool,
    label: SharedString,
    on_click: Option<SwitchCallback>,
}

impl Switch {
    /// Creates a switch with the given state.
    pub(super) fn new(id: impl Into<SharedString>, checked: bool) -> Self {
        Self {
            id: id.into(),
            checked,
            disabled: false,
            label: SharedString::default(),
            on_click: None,
        }
    }

    /// Stops the switch from changing.
    pub(super) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Sets the name exposed to accessibility clients.
    pub(super) fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Calls `handler` with the new value when the user toggles the switch.
    pub(super) fn on_click(
        mut self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let checked = self.checked;
        let (track_width, track_height, thumb_size) = (36., 20., 14.);
        // The thumb is positioned inside the one-point border, so the free space is measured
        // from the inner size of the track.
        let inner_width = track_width - 2.;
        let inner_height = track_height - 2.;
        let inset = (inner_height - thumb_size) / 2.;
        let travel = inner_width - thumb_size - inset * 2.;
        let motion = window.use_keyed_state(
            ElementId::Name(SharedString::from(format!("{}-motion", self.id))),
            cx,
            |_, _| SwitchMotion::default(),
        );
        let sequence = motion.update(cx, |motion, _| motion.observe(checked));
        // On: lit dots trail behind the thumb and fade toward it. Off: a quiet, sparse texture.
        let (field, tone) = if checked {
            (
                DitherField::Fade {
                    from: Edge::Left,
                    start: 0.95,
                    end: 0.05,
                },
                theme.accent,
            )
        } else {
            (
                DitherField::Fade {
                    from: Edge::Left,
                    start: 0.25,
                    end: 0.25,
                },
                theme.subtle.opacity(0.6),
            )
        };
        let thumb = div()
            .absolute()
            .top(px(inset))
            .size(px(thumb_size))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(3.))
            .bg(if checked { theme.text } else { theme.muted })
            .when(checked, |thumb| {
                // A small lit pixel in the thumb, like a status LED.
                thumb.child(div().size(px(4.)).rounded(px(1.)).bg(theme.accent))
            });
        let thumb = if sequence == 0 {
            thumb
                .left(px(inset + if checked { travel } else { 0. }))
                .into_any_element()
        } else {
            thumb
                .with_animation(
                    ElementId::NamedInteger(
                        SharedString::from(format!("{}-thumb", self.id)),
                        sequence as u64,
                    ),
                    Animation::new(SWITCH_DURATION).with_easing(|p| 1. - (1. - p).powi(3)),
                    move |thumb, delta| {
                        let progress = if checked { delta } else { 1. - delta };
                        thumb.left(px(inset + travel * progress))
                    },
                )
                .into_any_element()
        };
        let disabled = self.disabled;
        let focus = window
            .use_keyed_state(
                ElementId::Name(SharedString::from(format!("{}-focus", self.id))),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        div()
            .id(ElementId::Name(self.id.clone()))
            .relative()
            .flex_shrink_0()
            .w(px(track_width))
            .h(px(track_height))
            .rounded(px(5.))
            .overflow_hidden()
            .border_1()
            .border_color(if checked {
                theme.accent.opacity(0.65)
            } else {
                theme.edge
            })
            .bg(if checked {
                theme.accent.opacity(0.18)
            } else {
                theme.inset
            })
            .role(Role::Switch)
            .aria_label(self.label)
            .aria_toggled(if checked {
                Toggled::True
            } else {
                Toggled::False
            })
            .opacity(if disabled { 0.5 } else { 1. })
            .child(
                Dither::new(field)
                    .color(tone)
                    .grid(2., 1.)
                    .corner_radius(4.)
                    .layer(),
            )
            .child(thumb)
            .when_some(self.on_click.filter(|_| !disabled), |switch, on_click| {
                let key_click = on_click.clone();
                switch
                    .track_focus(&focus)
                    .cursor_pointer()
                    .focus_visible(move |style| style.border_color(theme.text))
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                            cx.stop_propagation();
                            key_click(&!checked, window, cx);
                        }
                    })
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        on_click(&!checked, window, cx);
                    })
            })
    }
}
