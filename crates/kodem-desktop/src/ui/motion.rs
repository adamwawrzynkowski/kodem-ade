//! Small native animations shared by activity indicators and panel entrances.
use super::*;
use crate::components::streaming_text::reveal_opacity;
use std::{cell::Cell, rc::Rc};

pub(super) const COMPOSER_RADIUS: f32 = 8.;

/// Applies a moving highlight to text while keeping a single stable text layout.
///
/// The highlight advances on the stepped clock, so it redraws the window twelve times a second
/// instead of on every display frame.
pub(super) fn gleam(
    label: impl Into<SharedString>,
    theme: Theme,
    reduce_motion: bool,
) -> AnyElement {
    let label = label.into();
    if reduce_motion {
        return div()
            .min_w_0()
            .text_ellipsis()
            .text_color(theme.muted)
            .child(label)
            .into_any_element();
    }
    let phase = dither::cycle(Duration::from_millis(1900));
    let count = label.chars().count().max(1) as f32;
    let highlights = label
        .char_indices()
        .enumerate()
        .map(|(n, (offset, ch))| {
            let distance = n as f32 / count - (phase * 1.6 - 0.3);
            // Three brightness steps keep the scan reading as phosphor, not a gradient.
            let intensity = ((1. - distance.abs() / 0.24).max(0.).powi(2) * 3.).round() / 3.;
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
    div()
        .relative()
        .min_w_0()
        .text_ellipsis()
        .child(StyledText::new(label).with_highlights(highlights))
        .child(dither::ticker())
        .into_any_element()
}

/// Shows the dithered working pulse, or blinks another active glyph in two steps.
pub(super) fn active_icon(
    glyph: &'static str,
    color: Hsla,
    spin: bool,
    reduce_motion: bool,
) -> AnyElement {
    if spin || glyph == "loader" {
        return dither::pulse(color, reduce_motion);
    }
    if reduce_motion {
        return icon(glyph)
            .size(px(14.))
            .text_color(color)
            .into_any_element();
    }
    // The glyph changes twice per cycle, so the view redraws only then.
    let (dim, change) = dither::blink(Duration::from_millis(1400), 0.62);
    div()
        .relative()
        .flex_shrink_0()
        .child(
            icon(glyph)
                .size(px(14.))
                .text_color(color)
                .opacity(if dim { 0.4 } else { 1. }),
        )
        .child(dither::ticker_after(change))
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

/// Starts fast and settles into place.
pub(super) fn ease_out_cubic(progress: f32) -> f32 {
    1. - (1. - progress).powi(3)
}

/// An invisible element that asks for another frame, for motion that a view steps itself.
pub(super) fn next_frame() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |_, _, window, _| window.request_animation_frame(),
    )
    .absolute()
    .size(px(1.))
}

/// Shows a part at `openness` of its way in, from 0, hidden a little above its place, to 1, in
/// place. Its layout keeps its full size, so it suits rows of a uniform list.
pub(super) fn drop_in(element: impl IntoElement, openness: f32) -> AnyElement {
    if openness >= 1. {
        return element.into_any_element();
    }
    let eased = ease_out_cubic(openness);
    div()
        .relative()
        .top(px(-6. * (1. - eased)))
        .opacity(eased)
        .child(element)
        .child(next_frame())
        .into_any_element()
}

/// Brings in a part that a surface just gained: it fades in and drops into place `order` beats
/// after the first part, so a group of parts arrives in sequence. The part must be relatively
/// positioned. It plays again when the part leaves and comes back.
pub(super) fn stagger<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    element: E,
    order: usize,
) -> AnyElement {
    let delay = order as f32 * 0.04;
    let span = 0.22;
    let total = delay + span;
    element
        .with_animation(
            id,
            Animation::new(Duration::from_secs_f32(total))
                .with_easing(move |p| ease_out_cubic(((p * total - delay) / span).clamp(0., 1.))),
            |part, delta| part.top(px(-6. * (1. - delta))).opacity(delta),
        )
        .into_any_element()
}

/// Slides content in by `offset` while a Bayer dissolve in the surface color clears from the top.
///
/// With reduced motion GPUI jumps to the final frame, so neither effect is drawn. `block` is the
/// dissolve's dot size; large blocks keep the dot count low on full-size panels.
pub(super) fn dissolve(
    id: impl Into<ElementId>,
    element: impl IntoElement,
    color: Hsla,
    radius: f32,
    offset: f32,
    block: f32,
    millis: u64,
) -> AnyElement {
    div()
        .relative()
        .child(element)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(millis)).with_easing(ease_out_cubic),
            move |surface, delta| {
                let moved = surface.top(px(offset * (1. - delta)));
                if delta >= 1. {
                    return moved;
                }
                moved.child(
                    dither::Dither::new(dither::DitherField::Dissolve(delta))
                        .color(color)
                        .grid(block, block)
                        .corner_radius(radius)
                        .layer(),
                )
            },
        )
        .into_any_element()
}

/// Clears a Bayer dissolve in `color` from over a surface whose content was replaced in place,
/// such as a pane after its workspace changed. The surface keeps the layout its caller gave it.
///
/// A new `id` plays the dissolve again. `block` is the dot size; large blocks keep the dot
/// count low on wide panes.
pub(super) fn switched(
    id: impl Into<ElementId>,
    surface: Div,
    color: Hsla,
    block: f32,
) -> AnyElement {
    surface
        .with_animation(
            id,
            Animation::new(Duration::from_millis(240)).with_easing(ease_out_cubic),
            move |surface, delta| {
                if delta >= 1. {
                    return surface;
                }
                surface.child(
                    dither::Dither::new(dither::DitherField::Dissolve(delta))
                        .color(color)
                        .grid(block, block)
                        .layer(),
                )
            },
        )
        .into_any_element()
}

/// A menu that dissolves in beside its trigger; `upward` menus rise into place.
pub(super) fn popover(
    id: impl Into<ElementId>,
    element: impl IntoElement,
    theme: Theme,
    upward: bool,
) -> AnyElement {
    dissolve(
        id,
        element,
        theme.raised,
        crate::theme::RADIUS_FLOATING,
        if upward { 6. } else { -6. },
        3.,
        200,
    )
}

/// A dialog or card that rises into place and dissolves in.
pub(super) fn floating(
    id: impl Into<ElementId>,
    element: impl IntoElement,
    theme: Theme,
) -> AnyElement {
    dissolve(
        id,
        element,
        theme.raised,
        crate::theme::RADIUS_FLOATING,
        10.,
        3.,
        260,
    )
}

/// How long a floating notice takes to leave.
pub(super) const NOTICE_LEAVE: Duration = Duration::from_millis(220);

/// A floating notice that springs in from the right, overshoots a little and settles, fading in
/// while a Bayer dissolve in its surface color clears from it. `radius` is the card's corner.
pub(super) fn notice_enter(
    id: impl Into<ElementId>,
    element: impl IntoElement,
    theme: Theme,
    radius: f32,
) -> AnyElement {
    div()
        .relative()
        .child(element)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(420)),
            move |notice, delta| {
                let remaining = 1. - fx::ease_out_back(delta);
                let notice = notice
                    .left(px(32. * remaining))
                    .opacity((delta * 2.5).min(1.));
                // The dissolve clears in the first part of the spring, before it settles.
                let cleared = (delta * 1.6).min(1.);
                if cleared >= 1. {
                    return notice;
                }
                notice.child(
                    dither::Dither::new(dither::DitherField::Dissolve(cleared))
                        .color(theme.raised)
                        .grid(3., 3.)
                        .corner_radius(radius)
                        .layer(),
                )
            },
        )
        .into_any_element()
}

/// A floating notice that slides back to the right and fades out while a Bayer dissolve in its
/// surface color covers it. It takes `NOTICE_LEAVE`.
pub(super) fn notice_leave(
    id: impl Into<ElementId>,
    element: impl IntoElement,
    theme: Theme,
    radius: f32,
) -> AnyElement {
    div()
        .relative()
        .child(element)
        .with_animation(
            id,
            Animation::new(NOTICE_LEAVE).with_easing(quadratic),
            move |notice, delta| {
                notice.left(px(36. * delta)).opacity(1. - delta).child(
                    dither::Dither::new(dither::DitherField::Dissolve(1. - delta))
                        .color(theme.raised)
                        .grid(3., 3.)
                        .corner_radius(radius)
                        .layer(),
                )
            },
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

/// A height no smaller than the terminal panel, which its opening grows toward.
pub(super) const TERMINAL_REACH: f32 = 300.;

/// How far each side panel is open, from 0 to 1. The values follow what the settings ask for,
/// so opening and closing a panel slides instead of jumping.
#[derive(Default)]
pub(super) struct Reveals {
    pub sidebar: f32,
    pub review: f32,
    pub terminal: f32,
    running: bool,
    started: bool,
}

impl Reveals {
    fn values(&mut self) -> [&mut f32; 3] {
        [&mut self.sidebar, &mut self.review, &mut self.terminal]
    }
}

impl Shell {
    // Returns how far the sidebar, the changes panel and the terminal should be open.
    fn reveal_targets(&self) -> [f32; 3] {
        let open = |shown: bool| if shown { 1. } else { 0. };
        // A chat has no checkout, so it has no changes panel and no terminal.
        let task = self.has_checkout();
        [
            open(!self.settings.collapsed),
            // In a narrow window the changes panel replaces the chat and does not slide.
            open(self.show_review && task && self.window_width > 1100.),
            open(self.show_terminal && task),
        ]
    }

    /// Moves the side panels toward the state the settings ask for. Call it once per render.
    ///
    /// Panels take their state at once until the workspace has loaded, and with reduced motion.
    pub(super) fn sync_reveals(&mut self, cx: &mut Context<Self>) {
        let targets = self.reveal_targets();
        if cx.reduce_motion() || !self.reveals.started {
            for (value, target) in self.reveals.values().into_iter().zip(targets) {
                *value = target;
            }
            self.reveals.started = self.workspace.is_some();
            return;
        }
        let settled = self
            .reveals
            .values()
            .into_iter()
            .zip(targets)
            .all(|(value, target)| *value == target);
        if settled || self.reveals.running {
            return;
        }
        self.reveals.running = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let moving = this
                    .update(cx, |s, cx| {
                        let targets = s.reveal_targets();
                        let mut moving = false;
                        for (value, target) in s.reveals.values().into_iter().zip(targets) {
                            let remaining = target - *value;
                            if remaining.abs() < 0.004 {
                                *value = target;
                            } else {
                                // Each step covers a share of what is left: fast, then settling.
                                *value += remaining * 0.26;
                                moving = true;
                            }
                        }
                        s.reveals.running = moving;
                        cx.notify();
                        moving
                    })
                    .unwrap_or(false);
                if !moving {
                    break;
                }
            }
        })
        .detach();
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

/// How long the composer of a start screen takes to move to the bottom of the conversation.
pub(super) const LAUNCH: Duration = Duration::from_millis(420);
/// Share of the launch in which the rest of the start screen dissolves away.
const LAUNCH_VANISH: f32 = 0.6;

/// The start screen's composer on its way to the bottom of the new conversation. The message
/// is sent when it arrives there and the task exists.
pub(super) struct Launch {
    started: std::time::Instant,
    /// The composer's box on the start screen when the message was sent, in window pixels.
    pub from: Bounds<Pixels>,
    /// The task that the host created for the message, once it answered.
    pub task: Option<String>,
    /// Width of the composer's box at the bottom of the conversation, in window pixels. It is
    /// measured on each frame and unknown before the first.
    pub width: Option<f32>,
}
impl Launch {
    /// Starts a launch from the composer's box at `from`.
    pub(super) fn new(from: Bounds<Pixels>) -> Self {
        Self {
            started: std::time::Instant::now(),
            from,
            task: None,
            width: None,
        }
    }
    /// Returns how far the composer has moved, from 0 to 1, eased in and out.
    pub(super) fn progress(&self) -> f32 {
        ease_in_out_cubic((self.started.elapsed().as_secs_f32() / LAUNCH.as_secs_f32()).min(1.))
    }
    /// Returns how far the rest of the start screen has dissolved, from 0 to 1.
    pub(super) fn vanished(&self) -> f32 {
        let linear = self.started.elapsed().as_secs_f32() / (LAUNCH.as_secs_f32() * LAUNCH_VANISH);
        ease_out_cubic(linear.min(1.))
    }
    /// Reports whether the composer has reached the bottom of the conversation.
    pub(super) fn arrived(&self) -> bool {
        self.started.elapsed() >= LAUNCH
    }
    /// Returns the width of the composer's box at `progress`, in window pixels.
    pub(super) fn box_width(&self, progress: f32) -> f32 {
        let from = f32::from(self.from.size.width);
        from + (self.width.unwrap_or(from) - from) * progress
    }
}

fn ease_in_out_cubic(progress: f32) -> f32 {
    if progress < 0.5 {
        4. * progress.powi(3)
    } else {
        1. - (2. - 2. * progress).powi(3) / 2.
    }
}

/// Covers content that leaves with a Bayer dissolve in `color` while it fades out. `progress`
/// goes from 0, untouched, to 1, gone.
pub(super) fn vanish(element: impl IntoElement, progress: f32, color: Hsla) -> AnyElement {
    let frame = div().relative().child(element);
    if progress <= 0. {
        return frame.into_any_element();
    }
    frame
        // The fade trails the dots, so the content reads as eaten by the pattern.
        .opacity(1. - progress.powi(2))
        .child(
            dither::Dither::new(dither::DitherField::Dissolve(1. - progress))
                .color(color)
                .grid(3., 3.)
                .layer(),
        )
        .into_any_element()
}

/// Paints a laid-out element on its way from `from` to the place its layout gives it.
/// `progress` goes from 0, at `from`, to 1, in place. The element's layout does not move, so the
/// content around it stays still.
pub(super) fn morph(element: impl IntoElement, from: Point<Pixels>, progress: f32) -> AnyElement {
    Morph {
        child: element.into_any_element(),
        from,
        progress,
    }
    .into_any_element()
}

struct Morph {
    child: AnyElement,
    from: Point<Pixels>,
    progress: f32,
}
impl IntoElement for Morph {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Morph {
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
        (self.child.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let offset = (self.from - bounds.origin) * (1. - self.progress);
        window.with_element_offset(offset, |window| {
            self.child.prepaint(window, cx);
        });
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
        if self.progress < 1. {
            window.request_animation_frame();
        }
    }
}

/// How long a new row of the thread feed takes to open.
const ROW_ENTER: Duration = Duration::from_millis(360);
/// Height that an opening row of the thread feed grows toward. It is above the height of a task
/// card, so the row's own height stops the growth.
const ROW_REACH: f32 = 64.;

/// A row of the thread feed that appears once, when its task was just created.
pub(super) struct Entrance {
    created: std::time::Instant,
    shown: std::cell::Cell<Option<std::time::Instant>>,
}
impl Entrance {
    /// Starts waiting for the row to show.
    pub(super) fn new() -> Self {
        Self {
            created: std::time::Instant::now(),
            shown: Default::default(),
        }
    }
    /// Returns how far the row has opened, from 0 to 1. The opening starts the first time it is
    /// asked for, which is when the row shows.
    pub(super) fn progress(&self) -> f32 {
        let shown = self.shown.get().unwrap_or_else(|| {
            let now = std::time::Instant::now();
            self.shown.set(Some(now));
            now
        });
        (shown.elapsed().as_secs_f32() / ROW_ENTER.as_secs_f32()).min(1.)
    }
    /// Reports whether the row has opened, or never showed within a few seconds.
    pub(super) fn done(&self) -> bool {
        match self.shown.get() {
            Some(shown) => shown.elapsed() >= ROW_ENTER,
            None => self.created.elapsed() > Duration::from_secs(3),
        }
    }
}

/// Opens a new row: it grows to its height, slides in from the leading side and clears out of
/// a Bayer dissolve in `color`, the surface behind it.
pub(super) fn row_enter(element: impl IntoElement, progress: f32, color: Hsla) -> AnyElement {
    if progress >= 1. {
        return element.into_any_element();
    }
    let eased = ease_out_cubic(progress);
    div()
        .relative()
        .max_h(px(ROW_REACH * eased))
        .overflow_hidden()
        .child(
            div()
                .relative()
                .left(px(-14. * (1. - eased)))
                .opacity(reveal_opacity(progress))
                .child(element),
        )
        .child(
            dither::Dither::new(dither::DitherField::Dissolve(eased))
                .color(color)
                .grid(3., 3.)
                .layer(),
        )
        .child(
            canvas(
                |_, _, _| {},
                |_, _, window, _| window.request_animation_frame(),
            )
            .absolute()
            .size(px(1.)),
        )
        .into_any_element()
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

/// How long a tray of the work trail takes to open or fold.
pub(super) const FOLD: Duration = Duration::from_millis(240);

/// The foldable parts of the work trail that the last frame drew, and those that this frame
/// draws. A part that changes between open and folded from one frame to the next animates the
/// change, so a click and a tray that follows its work both slide. A part that no frame drew
/// shows its state at once, unless it is new content that grows into place.
#[derive(Default)]
pub(super) struct Folds {
    drawn: HashMap<String, FoldSlot>,
    drawing: HashMap<String, FoldSlot>,
}

struct FoldSlot {
    open: bool,
    /// When the running change started, and how far open the part was then.
    change: Option<(std::time::Instant, f32)>,
    /// The height of the open content, as the last frame laid it out.
    height: Rc<Cell<Option<Pixels>>>,
}

impl FoldSlot {
    fn openness(&self, now: std::time::Instant) -> f32 {
        let target = if self.open { 1. } else { 0. };
        let Some((started, from)) = self.change else {
            return target;
        };
        let progress = (now.duration_since(started).as_secs_f32() / FOLD.as_secs_f32()).min(1.);
        from + (target - from) * ease_out_cubic(progress)
    }
}

/// How far one foldable part is open in this frame, from 0, folded, to 1, open.
pub(super) struct Fold {
    pub openness: f32,
    open: bool,
    /// A change is running. Its first frame is still at the openness that it leaves from.
    moving: bool,
    height: Rc<Cell<Option<Pixels>>>,
}

impl Fold {
    /// Reports whether the part's content shows in this frame: it is open, opening or folding.
    pub(super) fn shown(&self) -> bool {
        self.open || self.openness > 0.
    }
}

impl Folds {
    /// Starts a frame: the parts that the last frame drew become the ones to compare with.
    pub(super) fn begin_frame(&mut self) {
        self.drawn = std::mem::take(&mut self.drawing);
    }

    /// Returns how far the part `key` is open in this frame, where `open` is its state. A
    /// `fresh` part that the last frame did not draw grows open from folded. With `reduce`,
    /// every part takes its state at once.
    pub(super) fn slot(&mut self, key: &str, open: bool, fresh: bool, reduce: bool) -> Fold {
        let now = std::time::Instant::now();
        let mut slot = self
            .drawing
            .remove(key)
            .or_else(|| self.drawn.remove(key))
            .unwrap_or_else(|| FoldSlot {
                open,
                change: (fresh && open && !reduce).then_some((now, 0.)),
                height: Rc::default(),
            });
        if slot.open != open {
            let from = slot.openness(now);
            slot.open = open;
            slot.change = (!reduce).then_some((now, from));
        }
        if reduce
            || slot
                .change
                .is_some_and(|(started, _)| now.duration_since(started) >= FOLD)
        {
            slot.change = None;
        }
        let fold = Fold {
            openness: slot.openness(now),
            open,
            moving: slot.change.is_some(),
            height: slot.height.clone(),
        };
        self.drawing.insert(key.to_owned(), slot);
        fold
    }
}

/// Shows foldable content at the share of its height that `fold` is open. The content keeps
/// its own layout, which is measured on every frame, so a later fold knows the height to leave
/// from. While it moves, the content fades and drifts with the clip, and a Bayer dissolve in
/// `color`, the opaque surface behind it, clears from it. Content over the backdrop takes no
/// dissolve, which would cover the backdrop's art.
pub(super) fn fold(content: impl IntoElement, fold: &Fold, color: Option<Hsla>) -> AnyElement {
    let height = fold.height.clone();
    let measured = div()
        .relative()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .child(content)
        .child(
            canvas(
                move |bounds, _, _| height.set(Some(bounds.size.height)),
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
    if !fold.moving && fold.openness >= 1. {
        return measured.into_any_element();
    }
    let openness = fold.openness.max(0.);
    // Before the first measure the clip stays shut; the next frame knows the height.
    let shown = fold.height.get().map_or(px(0.), |height| height * openness);
    div()
        .relative()
        .w_full()
        .min_w_0()
        .h(shown)
        .overflow_hidden()
        .child(
            measured
                .top(px(-6. * (1. - openness)))
                .opacity(reveal_opacity(openness)),
        )
        .when_some(color, |clip, color| {
            clip.child(
                dither::Dither::new(dither::DitherField::Dissolve(openness))
                    .color(color)
                    .grid(3., 3.)
                    .layer(),
            )
        })
        .child(
            canvas(
                |_, _, _| {},
                |_, _, window, _| window.request_animation_frame(),
            )
            .absolute()
            .size(px(1.)),
        )
        .into_any_element()
}
