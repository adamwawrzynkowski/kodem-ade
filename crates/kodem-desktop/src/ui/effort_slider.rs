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
        self.target = slider_progress(f32::from(bounds.size.width), f32::from(x - bounds.left()))
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
        let stop_count = self.options.len();
        let entity = cx.entity();
        let accent = t.accent;
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
                    .justify_center()
                    .child(
                        div()
                            .font_family(crate::theme::MONO)
                            .text_size(px(12.))
                            .letter_spacing(px(crate::theme::LABEL_TRACKING))
                            .text_color(t.text)
                            .child(
                                self.options
                                    .get(self.selected)
                                    .cloned()
                                    .unwrap_or_default()
                                    .to_uppercase(),
                            ),
                    ),
            )
            .child(
                div()
                    .id("effort-track")
                    .relative()
                    .h(px(30.))
                    .w_full()
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
                            move |bounds, _, window, _| {
                                let center = bounds.left()
                                    + px(slider_center(f32::from(bounds.size.width), progress));
                                // An instrument fader: dithered empty track, solid level, machined cap.
                                let track = Bounds::new(
                                    point(bounds.left(), bounds.center().y - px(5.)),
                                    size(bounds.size.width, px(10.)),
                                );
                                window.paint_quad(
                                    fill(track, t.inset)
                                        .corner_radii(px(2.))
                                        .border_widths(px(1.))
                                        .border_color(t.border),
                                );
                                super::dither::paint_flat(
                                    window,
                                    track.inset(px(2.)),
                                    1.,
                                    t.text.opacity(0.22),
                                    0.5,
                                );
                                // Fill ends at the cap's center, including at endpoints.
                                let filled = Bounds::new(
                                    track.origin,
                                    size(center - track.left(), track.size.height),
                                );
                                window.paint_quad(fill(filled, accent).corner_radii(px(2.)));
                                for i in 0..stop_count {
                                    let x = bounds.left()
                                        + px(slider_center(
                                            f32::from(bounds.size.width),
                                            i as f32 / count,
                                        ));
                                    let tick = Bounds::new(
                                        point(x - px(0.5), bounds.center().y + px(8.)),
                                        size(px(1.), px(5.)),
                                    );
                                    window.paint_quad(fill(tick, t.subtle));
                                }
                                let thumb = Bounds::new(
                                    point(center - px(4.), bounds.center().y - px(11.)),
                                    size(px(8.), px(22.)),
                                );
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            thumb.origin + point(px(1.), px(2.)),
                                            thumb.size,
                                        ),
                                        t.ink,
                                    )
                                    .corner_radii(px(2.)),
                                );
                                window.paint_quad(
                                    fill(thumb, t.text)
                                        .corner_radii(px(2.))
                                        .border_widths(px(1.))
                                        .border_color(t.canvas),
                                );
                                let grip = Bounds::new(
                                    point(center - px(2.), bounds.center().y - px(0.5)),
                                    size(px(4.), px(1.)),
                                );
                                window.paint_quad(fill(grip, t.canvas));
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(super::caps(
                        self.options.first().cloned().unwrap_or_default(),
                        t,
                    ))
                    .child(super::caps(
                        self.options.last().cloned().unwrap_or_default(),
                        t,
                    )),
            )
    }
}

// Shared pointer and paint geometry keeps the thumb, fill and stop positions aligned.
fn slider_center(width: f32, progress: f32) -> f32 {
    let inset = 14_f32.min(width * 0.5);
    inset + (width - 2. * inset).max(0.) * progress.clamp(0., 1.)
}
fn slider_progress(width: f32, x: f32) -> f32 {
    let inset = 14_f32.min(width * 0.5);
    ((x - inset) / (width - 2. * inset).max(1.)).clamp(0., 1.)
}
