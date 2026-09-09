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

/// Adds visible, slowly drifting grain behind chat content, static under Reduce Motion.
pub(super) fn chat_dither(light: bool, reduce_motion: bool) -> AnyElement {
    let layer = div().absolute().inset_0();
    if reduce_motion {
        return layer.child(dither_frame(light, 0.)).into_any_element();
    }
    layer
        .with_animation(
            "chat-background-dither",
            Animation::new(Duration::from_secs(240))
                .with_easing(|phase| phase)
                .repeat(),
            move |layer, phase| layer.child(dither_frame(light, phase)),
        )
        .into_any_element()
}

fn dither_frame(light: bool, phase: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let tile = 256.;
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            // SVG bounds are pixel-snapped. Translate the cached sprite in the GPU
            // after snapping, retaining fractional device pixels between frames.
            let translation =
                point(px(-phase * tile), px(-phase * tile)).scale(window.scale_factor());
            let transform = TransformationMatrix::unit().translate(translation);
            let tint = if light { rgb(0x000000) } else { rgb(0xffffff) };
            let color = rgb_to_hsla(tint).opacity(if light { 0.08 } else { 0.14 });
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                // Only placement changes. Every tile reuses the same cached SVG raster.
                for y in 0..(height / tile).ceil() as usize + 2 {
                    for x in 0..(width / tile).ceil() as usize + 2 {
                        let origin = point(
                            bounds.left() + px(x as f32 * tile),
                            bounds.top() + px(y as f32 * tile),
                        );
                        let _ = window.paint_svg(
                            Bounds::new(origin, size(px(tile), px(tile))),
                            "chat-dither".into(),
                            None,
                            transform,
                            color,
                            cx,
                        );
                    }
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

/// Paints glass only inside the rounded prompt surface, beneath its input and toolbar.
pub(super) fn composer_backdrop(theme: Theme) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let bands = 4;
            let height = bounds.size.height / bands as f32;
            for band in 0..bands {
                let region = Bounds::new(
                    point(bounds.left(), bounds.top() + height * band as f32),
                    size(bounds.size.width, height),
                );
                let corners = Corners {
                    top_left: if band == 0 { px(16.) } else { px(0.) },
                    top_right: if band == 0 { px(16.) } else { px(0.) },
                    bottom_left: if band == bands - 1 { px(16.) } else { px(0.) },
                    bottom_right: if band == bands - 1 { px(16.) } else { px(0.) },
                };
                window.paint_backdrop_filter(
                    region,
                    corners,
                    &[Filter::Blur(px(6. + band as f32 * 2.))],
                );
            }
            window.paint_quad(fill(bounds, theme.raised.opacity(0.76)).corner_radii(px(16.)));
        },
    )
    .absolute()
    .inset_0()
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

/// Paints warm corner lighting behind a container's content, clipped to that container.
pub(super) fn window_texture(light: bool) -> impl IntoElement {
    corner_texture(light, true)
}

/// Keeps the gray corner lighting visible above file-list and diff backgrounds.
pub(super) fn cool_corner(light: bool) -> impl IntoElement {
    corner_texture(light, false)
}

fn corner_texture(light: bool, warm: bool) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let viewport = window.viewport_size();
            let edge = px(440.).min(viewport.width).min(viewport.height);
            let (asset, origin, color) = if warm {
                ("corner-warm", point(px(0.), px(0.)), rgb(0xd98137))
            } else {
                (
                    "corner-cool",
                    point(viewport.width - edge, viewport.height - edge),
                    rgb(0x9da4ae),
                )
            };
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let _ = window.paint_svg(
                    Bounds::new(origin, size(edge, edge)),
                    asset.into(),
                    None,
                    Default::default(),
                    rgb_to_hsla(color).opacity(if light { 0.18 } else { 0.48 }),
                    cx,
                );
            });
        },
    )
    .absolute()
    .inset_0()
}
