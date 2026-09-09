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
