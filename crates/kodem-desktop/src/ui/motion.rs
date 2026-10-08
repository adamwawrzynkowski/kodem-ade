//! Small native animations shared by activity indicators and panel entrances.
use super::*;
use crate::components::streaming_text::reveal_opacity;

pub(super) const COMPOSER_RADIUS: f32 = 8.;

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
                        // Three brightness steps keep the scan reading as phosphor, not a gradient.
                        let intensity =
                            ((1. - distance.abs() / 0.24).max(0.).powi(2) * 3.).round() / 3.;
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

/// Shows the dithered working pulse, or blinks another active glyph in two steps.
pub(super) fn active_icon(
    id: impl Into<ElementId>,
    glyph: &'static str,
    color: Hsla,
    spin: bool,
    reduce_motion: bool,
) -> AnyElement {
    if spin || glyph == "loader" {
        return dither::pulse(id, color, reduce_motion);
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
            Animation::new(Duration::from_millis(1400)).repeat(),
            move |svg, phase| svg.opacity(if phase < 0.62 { 1. } else { 0.4 }),
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

/// Shares one send transition across rerenders without replaying it when a row is recycled.
#[derive(Clone)]
pub(super) struct Arrival {
    created: std::time::Instant,
    started: std::rc::Rc<std::cell::Cell<Option<std::time::Instant>>>,
    origin: Option<Point<Pixels>>,
    pub queued: bool,
}
impl Arrival {
    /// Captures the composer's actual screen position until the accepted row is laid out.
    pub(super) fn new(origin: Option<Point<Pixels>>) -> Self {
        Self {
            created: std::time::Instant::now(),
            started: Default::default(),
            origin,
            queued: false,
        }
    }
    /// Bounds retained transition state, including submissions that never become visible.
    pub(super) fn expired(&self) -> bool {
        self.created.elapsed() > Duration::from_secs(3)
    }
}

/// Paints a message at its interpolated position while keeping its final layout space stable.
pub(super) fn send_transition<E: IntoElement + Styled + ParentElement>(
    element: E,
    arrival: &Arrival,
    reduce: bool,
) -> AnyElement {
    if reduce || arrival.expired() {
        return element.into_any_element();
    }
    let started = arrival.started.get().unwrap_or_else(|| {
        let now = std::time::Instant::now();
        arrival.started.set(Some(now));
        now
    });
    let p = (started.elapsed().as_secs_f32() / 0.42).min(1.);
    if p >= 1. {
        return element.into_any_element();
    }
    TravelingElement {
        child: Some(
            element
                .relative()
                .opacity(reveal_opacity(p))
                .child(arrival_gleam(p))
                .into_any_element(),
        ),
        origin: arrival.origin,
        progress: p,
    }
    .into_any_element()
}

struct TravelingElement {
    child: Option<AnyElement>,
    origin: Option<Point<Pixels>>,
    progress: f32,
}
impl IntoElement for TravelingElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for TravelingElement {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.as_mut().unwrap().request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        let source = self
            .origin
            .unwrap_or(bounds.origin + point(px(0.), px(16.)));
        let offset = (source - bounds.origin) * (1. - self.progress).powi(3);
        // Paint over the composer and escape the transcript/outbox scroll mask while in flight.
        // The child keeps its final layout slot, so neighboring messages do not move each frame.
        window.defer_draw(
            self.child.take().unwrap(),
            window.element_offset() + offset,
            0,
            None,
        );
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        window.request_animation_frame();
    }
}

/// Reveals newly added context with the streamed text's stepped fade and gentle lift.
pub(super) fn context_reveal(
    element: impl IntoElement,
    added: Option<std::time::Instant>,
    reduce: bool,
) -> AnyElement {
    let frame = div().relative().flex_shrink_0().min_w_0().child(element);
    let Some(added) = added.filter(|time| !reduce && time.elapsed() < Duration::from_millis(320))
    else {
        return frame.into_any_element();
    };
    let p = (added.elapsed().as_secs_f32() / 0.32).min(1.);
    frame
        .top(px(6. * (1. - p).powi(3)))
        .opacity(reveal_opacity(p))
        .child(arrival_gleam(p))
        .into_any_element()
}

// A brief luminance pulse matches the streamed glyph highlight across images and context cards.
fn arrival_gleam(progress: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let color = if window.text_style().color.lightness > 0.5 {
                rgb(0xffffff)
            } else {
                rgb(0x000000)
            };
            let alpha = (std::f32::consts::PI * progress).sin().powi(2) * 0.045;
            window.paint_quad(fill(bounds, rgb_to_hsla(color).opacity(alpha)).corner_radii(px(7.)));
        },
    )
    .absolute()
    .inset_0()
}
