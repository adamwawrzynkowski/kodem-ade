//! Small native animations shared by activity indicators and panel entrances.
use super::*;

pub(super) const COMPOSER_RADIUS: f32 = 16.;

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

/// Animates a nine-pixel working box and softly pulses other active glyphs.
pub(super) fn active_icon(
    id: impl Into<ElementId>,
    glyph: &'static str,
    color: Hsla,
    spin: bool,
    reduce_motion: bool,
) -> AnyElement {
    if spin || glyph == "loader" {
        let frame = move |phase: f32| {
            div()
                .size(px(14.))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .children((0..3).map(move |row| {
                    div().flex().gap(px(1.)).children((0..3).map(move |col| {
                        let seed = dot_seed(col, row);
                        div()
                            .size(px(4.))
                            .rounded(px(0.8))
                            .bg(color.opacity(0.2 + 0.8 * dot_opacity(seed, phase * 60.)))
                    }))
                }))
        };
        return if reduce_motion {
            frame(0.).into_any_element()
        } else {
            div()
                .with_animation(
                    id,
                    Animation::new(Duration::from_secs(60)).repeat(),
                    move |d, phase| d.child(frame(phase)),
                )
                .into_any_element()
        };
    }
    if reduce_motion {
        return icon(glyph)
            .size(px(14.))
            .text_color(color)
            .into_any_element();
    }
    svg()
        .path(glyph)
        .size(px(14.))
        .flex_shrink_0()
        .text_color(color)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(1800)).repeat(),
            move |svg, phase| {
                let scale = 0.9 + 0.1 * (phase * std::f32::consts::TAU).cos();
                svg.with_transformation(Transformation::scale(size(scale, scale)))
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

/// Fades independently staggered dots to half or full brightness, holds, then fades out.
pub(super) fn chat_dither(light: bool, reduce_motion: bool) -> AnyElement {
    let layer = div().absolute().inset_0();
    if reduce_motion {
        return layer.child(dither_frame(light, 0.)).into_any_element();
    }
    layer
        .with_animation(
            "chat-background-dither",
            Animation::new(Duration::from_secs(120))
                .with_easing(|phase| phase)
                .repeat(),
            move |layer, phase| layer.child(dither_frame(light, phase * 120.)),
        )
        .into_any_element()
}

fn dot_opacity(seed: u32, seconds: f32) -> f32 {
    // Each period divides the animation duration, so its loop is seamless.
    let period = [3., 4., 5., 6.][seed as usize % 4];
    let offset = (seed >> 8) as f32 / (u32::MAX >> 8) as f32 * period;
    let age = (seconds + offset).rem_euclid(period);
    let ramp = if age < 0.5 {
        age * 2.
    } else if age < 1.5 {
        1.
    } else if age < 2. {
        (2. - age) * 2.
    } else {
        0.
    };
    let peak = if seed & 4 == 0 { 0.5 } else { 1. };
    peak * ramp * ramp * (3. - 2. * ramp)
}

fn dot_seed(x: u32, y: u32) -> u32 {
    let mut n = x.wrapping_mul(0x9e3779b9) ^ y.wrapping_mul(0x85ebca6b);
    n = (n ^ (n >> 16)).wrapping_mul(0x7feb352d);
    n = (n ^ (n >> 15)).wrapping_mul(0x846ca68b);
    n ^ (n >> 16)
}

fn dither_frame(light: bool, seconds: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let tint = rgb_to_hsla(if light { rgb(0x000000) } else { rgb(0xffffff) });
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                // Dots never overlap: reserve one draw order for the entire grid instead
                // of inserting thousands of tiny rectangles into GPUI's overlap tree.
                window.paint_layer(bounds, |window| {
                    for y in 0..(f32::from(bounds.size.height) / 8.).ceil() as u32 {
                        for x in 0..(f32::from(bounds.size.width) / 8.).ceil() as u32 {
                            let seed = dot_seed(x, y);
                            if (seed >> 16).is_multiple_of(4) {
                                continue;
                            }
                            let alpha =
                                dot_opacity(seed, seconds) * if light { 0.09 } else { 0.14 };
                            if alpha <= 0. {
                                continue;
                            }
                            let origin = bounds.origin
                                + point(px(x as f32 * 8. + 2.), px(y as f32 * 8. + 2.));
                            window.paint_quad(gpui::fill(
                                Bounds::new(origin, size(px(1.5), px(1.5))),
                                tint.opacity(alpha),
                            ));
                        }
                    }
                });
            });
        },
    )
    .absolute()
    .inset_0()
}

/// Adds the app icon's corner grain in a project's saved color.
pub(super) fn project_dither(mut color: Hsla, radius: Pixels) -> impl IntoElement {
    color.saturation = (color.saturation * 1.18).min(1.);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            let radius = f32::from(radius);
            let data = rounded_project_dither(width, height, radius);
            // SVGs support a rounded clip path; GPUI content masks are rectangular.
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let _ = window.paint_svg(
                    bounds,
                    format!("thread-dither:{width}:{height}:{radius}").into(),
                    Some(data.as_bytes()),
                    Default::default(),
                    color.opacity(0.82),
                    cx,
                );
            });
        },
    )
    .absolute()
    .inset_0()
}

fn rounded_project_dither(width: f32, height: f32, radius: f32) -> String {
    let edge = width.min(200.);
    let grain = include_str!("../../../../assets/app/corner-warm.svg").replacen(
        "<svg ",
        &format!("<svg width=\"{edge}\" height=\"{edge}\" "),
        1,
    );
    // Match the tile's inner border edge, including both bottom corners.
    let inner_width = (width - 2.).max(0.);
    let inner_height = (height - 2.).max(0.);
    let inner_radius = (radius - 1.).max(0.);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<defs><clipPath id="tile-clip"><rect x="1" y="1" width="{inner_width}" height="{inner_height}" rx="{inner_radius}"/></clipPath></defs>
<g clip-path="url(#tile-clip)">{grain}</g></svg>"#
    )
}

/// Paints fine light and dark grain along the button bevel without covering its icon.
pub(super) fn button_dither() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            // The one-pixel grain points are separated by a two-pixel pitch.
            window.paint_layer(bounds, |window| {
                for y in (1..f32::from(bounds.size.height) as u32 - 1).step_by(2) {
                    for x in (1..f32::from(bounds.size.width) as u32 - 1).step_by(2) {
                        let edge = (x as f32)
                            .min(y as f32)
                            .min(f32::from(bounds.size.width) - x as f32)
                            .min(f32::from(bounds.size.height) - y as f32);
                        if edge > 5. || dot_seed(x, y).is_multiple_of(3) {
                            continue;
                        }
                        let color = if x + y
                            < (f32::from(bounds.size.width + bounds.size.height) / 2.) as u32
                        {
                            rgb(0xffffff)
                        } else {
                            rgb(0x000000)
                        };
                        window.paint_quad(gpui::fill(
                            Bounds::new(
                                bounds.origin + point(px(x as f32), px(y as f32)),
                                size(px(1.), px(1.)),
                            ),
                            rgb_to_hsla(color).opacity((1. - edge / 6.) * 0.27),
                        ));
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
                    top_left: if band == 0 {
                        px(COMPOSER_RADIUS)
                    } else {
                        px(0.)
                    },
                    top_right: if band == 0 {
                        px(COMPOSER_RADIUS)
                    } else {
                        px(0.)
                    },
                    bottom_left: if band == bands - 1 {
                        px(COMPOSER_RADIUS)
                    } else {
                        px(0.)
                    },
                    bottom_right: if band == bands - 1 {
                        px(COMPOSER_RADIUS)
                    } else {
                        px(0.)
                    },
                };
                window.paint_backdrop_filter(
                    region,
                    corners,
                    &[Filter::Blur(px(6. + band as f32 * 2.))],
                );
            }
            window.paint_quad(
                fill(bounds, theme.raised.opacity(0.76)).corner_radii(px(COMPOSER_RADIUS)),
            );
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
pub(super) fn window_texture(light: bool, accent: Hsla) -> impl IntoElement {
    corner_texture(light, Some(accent))
}

/// Keeps the gray corner lighting visible above file-list and diff backgrounds.
pub(super) fn cool_corner(light: bool) -> impl IntoElement {
    corner_texture(light, None)
}

fn corner_texture(light: bool, accent: Option<Hsla>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let viewport = window.viewport_size();
            let edge = px(440.).min(viewport.width).min(viewport.height);
            let (asset, origin, color) = if let Some(accent) = accent {
                ("corner-warm", point(px(0.), px(0.)), accent)
            } else {
                (
                    "corner-cool",
                    point(viewport.width - edge, viewport.height - edge),
                    rgb_to_hsla(rgb(0x9da4ae)),
                )
            };
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let _ = window.paint_svg(
                    Bounds::new(origin, size(edge, edge)),
                    asset.into(),
                    None,
                    Default::default(),
                    color.opacity(if light { 0.18 } else { 0.48 }),
                    cx,
                );
            });
        },
    )
    .absolute()
    .inset_0()
}
