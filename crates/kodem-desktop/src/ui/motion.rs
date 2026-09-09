//! Small native animations shared by activity indicators and panel entrances.
use super::*;

/// Applies a moving highlight to text while keeping a single stable text layout.
pub(super) fn gleam(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: Theme,
) -> AnyElement {
    let label = label.into();
    div()
        .min_w_0()
        .text_ellipsis()
        .with_animation(
            id,
            Animation::new(Duration::from_millis(1900)).repeat(),
            move |d, phase| {
                let count = label.chars().count().max(1) as f32;
                let highlights = label
                    .char_indices()
                    .enumerate()
                    .map(|(n, (offset, ch))| {
                        let distance = n as f32 / count - (phase * 1.6 - 0.3);
                        let intensity = (1. - distance.abs() / 0.24).max(0.).powi(2);
                        let mut color = theme.muted;
                        color.lightness += (theme.text.lightness - color.lightness) * intensity;
                        (
                            offset..offset + ch.len_utf8(),
                            HighlightStyle {
                                color: Some(color),
                                ..Default::default()
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                d.child(StyledText::new(label.clone()).with_highlights(highlights))
            },
        )
        .into_any_element()
}

/// Rotates execution indicators and softly pulses the other active glyphs.
pub(super) fn active_icon(
    id: impl Into<ElementId>,
    glyph: &'static str,
    color: Hsla,
    spin: bool,
) -> AnyElement {
    svg()
        .path(glyph)
        .size(px(14.))
        .flex_shrink_0()
        .text_color(color)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(if spin { 1400 } else { 1800 })).repeat(),
            move |svg, phase| {
                if spin {
                    svg.with_transformation(Transformation::rotate(percentage(phase)))
                } else {
                    let scale = 0.9 + 0.1 * (phase * std::f32::consts::TAU).cos();
                    svg.with_transformation(Transformation::scale(size(scale, scale)))
                }
            },
        )
        .into_any_element()
}

/// Fades new panels into place without keeping invisible controls alive after dismissal.
pub(super) fn reveal<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    element: E,
) -> AnyElement {
    element
        .with_animation(
            id,
            Animation::new(Duration::from_millis(170)).with_easing(|p| 1. - (1. - p).powi(3)),
            |d, phase| d.opacity(phase),
        )
        .into_any_element()
}

/// Reads the native accessibility preference without changing system settings.
pub(super) fn system_reduce_motion() -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Paints a soft blur ramp above the floating composer without adding pointer hitboxes.
pub(super) fn composer_backdrop(theme: Theme) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let ramp = px(36.).min(bounds.size.height);
            for i in 0..6 {
                let fraction = (i + 1) as f32 / 6.;
                let band = Bounds::new(
                    point(bounds.left(), bounds.top() + ramp * (i as f32 / 6.)),
                    size(bounds.size.width, ramp / 6.),
                );
                window.paint_backdrop_filter(
                    band,
                    Corners::default(),
                    &[Filter::Blur(px(8. * fraction * fraction))],
                );
                window.paint_quad(fill(band, theme.canvas.opacity(0.48 * fraction * fraction)));
            }
            let body = Bounds::new(
                point(bounds.left(), bounds.top() + ramp),
                size(bounds.size.width, (bounds.size.height - ramp).max(px(0.))),
            );
            window.paint_backdrop_filter(body, Corners::default(), &[Filter::Blur(px(8.))]);
            window.paint_quad(fill(body, theme.canvas.opacity(0.48)));
        },
    )
    .absolute()
    .top(px(-36.))
    .bottom_0()
    .w_full()
}

#[derive(Default)]
pub(super) struct ComposerSpring {
    pub offset: f32,
    velocity: f32,
    running: bool,
}
impl Shell {
    /// Adds a bounded spring impulse and stops rendering animation frames once it settles.
    pub(super) fn nudge_composer(&mut self, impulse: f32, cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            return;
        }
        self.composer_spring.velocity = (self.composer_spring.velocity + impulse).clamp(-0.8, 0.8);
        if self.composer_spring.running {
            return;
        }
        self.composer_spring.running = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep = this
                    .update(cx, |s, cx| {
                        let spring = &mut s.composer_spring;
                        spring.velocity = (spring.velocity - spring.offset * 0.16) * 0.72;
                        spring.offset = (spring.offset + spring.velocity).clamp(-2., 2.);
                        let keep = !cx.reduce_motion()
                            && (spring.offset.abs() > 0.01 || spring.velocity.abs() > 0.01);
                        if !keep {
                            spring.offset = 0.;
                            spring.velocity = 0.;
                            spring.running = false;
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
}

/// Paints cached dithered corner lighting without hitboxes, layout space or animation work.
pub(super) fn window_texture(light: bool) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let edge = px(440.).min(bounds.size.width).min(bounds.size.height);
            let corners = [
                ("corner-warm", bounds.origin, rgb(0xd98137)),
                (
                    "corner-cool",
                    point(bounds.right() - edge, bounds.bottom() - edge),
                    rgb(0x9da4ae),
                ),
            ];
            for (asset, origin, color) in corners {
                let _ = window.paint_svg(
                    Bounds::new(origin, size(edge, edge)),
                    asset.into(),
                    None,
                    Default::default(),
                    rgb_to_hsla(color).opacity(if light { 0.18 } else { 0.48 }),
                    cx,
                );
            }
        },
    )
    .absolute()
    .inset_0()
}
