//! Motion of the Git workspace's surfaces: popovers, menus, toasts, panels and drag chips
//! dissolve, rise or spring into place. With reduced motion, GPUI shows the final frame.
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, ElementId, Hsla, IntoElement, Pixels, div,
};

use super::kit::ease_out_cubic;
use super::theme::ActiveTheme as _;
use crate::theme::{RADIUS_FLOATING, px};
use crate::ui::dither::{self, Dither, DitherField};
use crate::ui::fx::ease_out_back;
use crate::ui::kit;

const POPOVER_DURATION: Duration = Duration::from_millis(220);
const CONTEXT_MENU_DURATION: Duration = Duration::from_millis(180);
const FLOATING_SURFACE_DURATION: Duration = Duration::from_millis(260);
const CONTENT_DURATION: Duration = Duration::from_millis(200);
const TOAST_DURATION: Duration = Duration::from_millis(420);
const DETAIL_DURATION: Duration = Duration::from_millis(240);
const DRAG_LIFT_DURATION: Duration = Duration::from_millis(320);
const DROP_HINT_DURATION: Duration = Duration::from_millis(180);

/// The surface color, corner radius, and block size that a dissolve
/// covers. `fill` makes the surface take the full size of its parent.
#[derive(Clone, Copy)]
struct DissolveSurface {
    color: Hsla,
    corner_radius: f32,
    block: f32,
    fill: bool,
}

/// Slides content in by `offset` while a Bayer dissolve in the surface
/// color clears from the top. With animations off, GPUI jumps straight to
/// the final frame, so neither effect is drawn.
fn reveal(
    name: &'static str,
    sequence: u64,
    offset: Pixels,
    duration: Duration,
    surface: DissolveSurface,
    content: impl IntoElement + 'static,
) -> AnyElement {
    let offset: f32 = offset.into();
    div()
        .relative()
        .when(surface.fill, |container| container.size_full())
        .child(content)
        .with_animation(
            ElementId::NamedInteger(name.into(), sequence),
            Animation::new(duration).with_easing(ease_out_cubic),
            move |surface_div, delta| {
                let moved = surface_div.top(gpui::px(offset * (1.0 - delta)));
                if delta >= 1.0 {
                    return moved;
                }
                moved.child(
                    Dither::new(DitherField::Dissolve(delta))
                        .color(surface.color)
                        .grid(surface.block, surface.block)
                        .corner_radius(surface.corner_radius)
                        .layer(),
                )
            },
        )
        .into_any_element()
}

/// A popover body that dissolves in below its trigger. It is glass, as a menu is: the content
/// lies on a floating face over a dithered contact shadow and has no fill of its own.
pub(crate) fn popover_surface(
    name: &'static str,
    sequence: u64,
    padding: Pixels,
    content: impl IntoElement + 'static,
    cx: &App,
) -> AnyElement {
    let t = cx.theme().kodem;
    reveal(
        name,
        sequence,
        px(-6.0),
        POPOVER_DURATION,
        DissolveSurface {
            color: cx.theme().popover,
            corner_radius: f32::from(cx.theme().radius_lg),
            block: 3.0,
            fill: false,
        },
        div()
            .relative()
            .child(dither::shadow(t, RADIUS_FLOATING, 5.))
            .child(kit::floating_face(t.raised, RADIUS_FLOATING, t))
            .child(
                div()
                    .relative()
                    .p(padding)
                    .rounded(px(RADIUS_FLOATING))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .child(content),
            ),
    )
}

/// A context menu that dissolves in at the pointer.
pub(crate) fn context_menu_surface(
    sequence: u64,
    content: impl IntoElement + 'static,
    cx: &App,
) -> AnyElement {
    reveal(
        "context-menu-motion",
        sequence,
        px(-4.0),
        CONTEXT_MENU_DURATION,
        DissolveSurface {
            color: cx.theme().popover,
            corner_radius: f32::from(cx.theme().radius_lg),
            block: 3.0,
            fill: false,
        },
        content,
    )
}

/// A floating card that rises into place and dissolves in.
pub(crate) fn floating_surface(
    name: &'static str,
    sequence: u64,
    content: impl IntoElement + 'static,
    cx: &App,
) -> AnyElement {
    reveal(
        name,
        sequence,
        px(10.0),
        FLOATING_SURFACE_DURATION,
        DissolveSurface {
            color: cx.theme().popover,
            corner_radius: f32::from(cx.theme().radius_lg),
            block: 3.0,
            fill: false,
        },
        content,
    )
}

/// A main content view that dissolves in with large blocks when the user
/// changes tabs or repositories. Large blocks keep the dot count low on
/// full-size panels.
pub(crate) fn content_surface(
    name: &'static str,
    sequence: u64,
    content: impl IntoElement + 'static,
    cx: &App,
) -> AnyElement {
    reveal(
        name,
        sequence,
        px(0.0),
        CONTENT_DURATION,
        DissolveSurface {
            color: cx.theme().colors.list,
            corner_radius: 0.0,
            block: 6.0,
            fill: true,
        },
        div().size_full().child(content),
    )
}

/// Toast content that springs in from the left, overshoots a little, and
/// settles, fading in as it arrives.
pub(crate) fn toast_surface(sequence: u64, content: impl IntoElement + 'static) -> AnyElement {
    div()
        .relative()
        .child(content)
        .with_animation(
            ElementId::NamedInteger("toast-motion".into(), sequence),
            Animation::new(TOAST_DURATION),
            |toast, delta| {
                let remaining = 1.0 - ease_out_back(delta);
                toast
                    .left(px(-28.0 * remaining))
                    .opacity((delta * 2.5).min(1.0))
            },
        )
        .into_any_element()
}

/// A dragged chip that springs up from the row it left, overshoots a
/// little, and settles under the pointer.
pub(crate) fn drag_lift_surface(sequence: u64, content: impl IntoElement + 'static) -> AnyElement {
    div()
        .relative()
        .child(content)
        .with_animation(
            ElementId::NamedInteger("drag-lift-motion".into(), sequence),
            Animation::new(DRAG_LIFT_DURATION),
            |chip, delta| {
                let remaining = 1.0 - ease_out_back(delta);
                chip.top(px(12.0 * remaining))
                    .opacity((delta * 3.0).min(1.0))
            },
        )
        .into_any_element()
}

/// A short hint that slides in from the left each time `sequence`
/// changes, such as the drop target under a dragged chip.
pub(crate) fn drop_hint_surface(sequence: u64, content: impl IntoElement + 'static) -> AnyElement {
    div()
        .relative()
        .child(content)
        .with_animation(
            ElementId::NamedInteger("drop-hint-motion".into(), sequence),
            Animation::new(DROP_HINT_DURATION).with_easing(ease_out_cubic),
            |hint, delta| hint.left(px(-8.0 * (1.0 - delta))).opacity(delta),
        )
        .into_any_element()
}

/// A detail panel that rises a few pixels and dissolves in when the
/// selection changes.
pub(crate) fn detail_surface(
    name: &'static str,
    sequence: u64,
    content: impl IntoElement + 'static,
    cx: &App,
) -> AnyElement {
    reveal(
        name,
        sequence,
        px(8.0),
        DETAIL_DURATION,
        DissolveSurface {
            color: cx.theme().colors.list,
            corner_radius: 0.0,
            block: 4.0,
            fill: true,
        },
        div().size_full().child(content),
    )
}
