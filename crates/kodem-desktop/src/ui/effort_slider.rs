//! A discrete reasoning control with a continuously animated thumb and fill.
use super::*;

/// Emitted when a pointer gesture or keyboard step selects a supported effort.
pub(super) struct EffortSelected(pub String);
/// Keeps pointer preview separate from the effort committed to the provider.
pub(super) struct EffortSlider {
    options: Vec<String>,
    confirmed: String,
    selected: usize,
    visual: f32,
    target: f32,
    velocity: f32,
    animating: bool,
    dragging: bool,
    bounds: Option<Bounds<Pixels>>,
    focus: FocusHandle,
    theme: Theme,
    disabled: bool,
}
impl EventEmitter<EffortSelected> for EffortSlider {}
impl EffortSlider {
    /// Creates a slider from the efforts advertised by the current model.
    pub(super) fn new(
        options: Vec<String>,
        selected: String,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let index = options.iter().position(|v| v == &selected).unwrap_or(0);
        Self {
            visual: index as f32,
            target: index as f32,
            selected: index,
            confirmed: selected,
            options,
            velocity: 0.,
            animating: false,
            dragging: false,
            bounds: None,
            focus: cx.focus_handle(),
            theme,
            disabled: false,
        }
    }
    /// Synchronizes provider confirmation without interrupting a pointer preview.
    pub(super) fn configure(
        &mut self,
        selected: &str,
        theme: Theme,
        disabled: bool,
        cx: &mut Context<Self>,
    ) {
        self.theme = theme;
        self.disabled = disabled;
        if disabled && self.dragging {
            self.dragging = false;
            self.selected = self.options.iter().position(|v| v == selected).unwrap_or(0);
            self.target = self.selected as f32;
            self.animate(cx);
        }
        if self.confirmed != selected {
            self.confirmed = selected.into();
            if !self.dragging {
                self.selected = self.options.iter().position(|v| v == selected).unwrap_or(0);
                self.target = self.selected as f32;
                self.animate(cx);
            }
        }
    }
    fn animate(&mut self, cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            self.visual = self.target;
            self.velocity = 0.;
            cx.notify();
            return;
        }
        if self.animating {
            return;
        }
        self.animating = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep = this
                    .update(cx, |s, cx| {
                        s.velocity = (s.velocity + (s.target - s.visual) * 0.22) * 0.62;
                        s.visual += s.velocity;
                        let keep = !cx.reduce_motion()
                            && ((s.target - s.visual).abs() > 0.002 || s.velocity.abs() > 0.002);
                        if !keep {
                            s.visual = s.target;
                            s.velocity = 0.;
                            s.animating = false;
                        }
                        cx.notify();
                        keep
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }
    fn preview(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(bounds) = self.bounds else {
            return;
        };
        let width = (f32::from(bounds.size.width) - 28.).max(1.);
        self.target = ((f32::from(x - bounds.left()) - 14.) / width).clamp(0., 1.)
            * self.options.len().saturating_sub(1) as f32;
        self.selected = self.target.round() as usize;
        self.animate(cx);
        cx.notify();
    }
    fn commit(&mut self, cx: &mut Context<Self>) {
        self.dragging = false;
        self.target = self.selected as f32;
        self.animate(cx);
        if let Some(value) = self.options.get(self.selected) {
            cx.emit(EffortSelected(value.clone()));
        }
    }
}
impl Render for EffortSlider {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let count = self.options.len().saturating_sub(1).max(1) as f32;
        let progress = (self.visual / count).clamp(0., 1.);
        let entity = cx.entity();
        let blue: Hsla = rgb_to_hsla(rgb(0x328be8));
        div()
            .id("effort-slider")
            .track_focus(&self.focus)
            .tab_index(0)
            .role(Role::Slider)
            .aria_label("Reasoning effort")
            .aria_value(self.options.get(self.selected).cloned().unwrap_or_default())
            .aria_min_numeric_value(0.)
            .aria_max_numeric_value(self.options.len().saturating_sub(1) as f64)
            .aria_numeric_value(self.selected as f64)
            .aria_numeric_value_step(1.)
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .opacity(if self.disabled { 0.45 } else { 1. })
            .on_key_down(cx.listener(|s, event: &KeyDownEvent, _, cx| {
                if s.disabled {
                    return;
                }
                let next = match event.keystroke.key.as_str() {
                    "left" | "down" => s.selected.saturating_sub(1),
                    "right" | "up" => (s.selected + 1).min(s.options.len().saturating_sub(1)),
                    "home" => 0,
                    "end" => s.options.len().saturating_sub(1),
                    _ => return,
                };
                cx.stop_propagation();
                s.selected = next;
                s.commit(cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .child(icon("settings-2").size(px(13.)))
                    .child(div().ml_2().child("Reasoning effort"))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_color(t.text)
                            .child(self.options.get(self.selected).cloned().unwrap_or_default()),
                    ),
            )
            .child(
                div()
                    .id("effort-track")
                    .relative()
                    .h(px(32.))
                    .w_full()
                    .rounded(px(16.))
                    .bg(t.selected)
                    .border_1()
                    .border_color(t.border)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|s, e: &MouseDownEvent, w, cx| {
                            if s.disabled {
                                return;
                            }
                            w.focus(&s.focus, cx);
                            s.dragging = true;
                            s.preview(e.position.x, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, _, cx| {
                        if s.dragging {
                            s.preview(e.position.x, cx);
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|s, e: &MouseUpEvent, _, cx| {
                            if s.dragging {
                                s.preview(e.position.x, cx);
                                s.commit(cx);
                            }
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|s, e: &MouseUpEvent, _, cx| {
                            if s.dragging {
                                s.preview(e.position.x, cx);
                                s.commit(cx);
                            }
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                entity.update(cx, |s, _| s.bounds = Some(bounds));
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .mx(px(14.))
                            .child(
                                div()
                                    .absolute()
                                    .left(px(-14.))
                                    .top_0()
                                    .bottom_0()
                                    .w(relative(progress))
                                    .min_w(px(28.))
                                    .rounded(px(16.))
                                    .bg(blue),
                            )
                            .children((0..self.options.len()).map(|i| {
                                div()
                                    .absolute()
                                    .left(relative(i as f32 / count))
                                    .top(px(13.))
                                    .ml(px(-2.))
                                    .size(px(4.))
                                    .rounded_full()
                                    .bg(t.text.opacity(0.4))
                            }))
                            .child(
                                div()
                                    .absolute()
                                    .left(relative(progress))
                                    .top(px(2.))
                                    .ml(px(-14.))
                                    .w(px(28.))
                                    .h(px(26.))
                                    .rounded_full()
                                    .bg(rgb(0xffffff))
                                    .shadow(t.elevation()),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(9.))
                    .text_color(t.subtle)
                    .child(self.options.first().cloned().unwrap_or_default())
                    .child(self.options.last().cloned().unwrap_or_default()),
            )
    }
}
