//! The Git workspace's button: a mono label on a dithered surface. It keeps one builder for
//! every variant, so a button also works as a popover trigger and as a dropdown menu anchor.
use std::rc::Rc;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    Anchor, AnyElement, App, Background, BoxShadow, ClickEvent, Div, ElementId, Hsla,
    Interactivity, IntoElement, RenderOnce, SharedString, Stateful, StyleRefinement, Window, div,
};

use super::menu::{DropdownMenuElement, PopupMenu};
use super::{Disableable, Icon, IconName, Selectable, Sizable, Size, h_flex};
use crate::theme::{RADIUS_CONTROL, Theme, px, text_px};
use crate::ui::dither::{Dither, DitherField, Edge};
use crate::ui::fx;
use crate::ui::git::theme::ActiveTheme;
use crate::ui::kit::{self, Layers as _, Pose};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The role of a button, which picks its colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Default,
    Primary,
    Danger,
    Ghost,
}

/// Builders for the role of a button.
pub trait ButtonVariants: Sized {
    /// Sets the role.
    fn with_variant(self, variant: ButtonVariant) -> Self;

    /// The main action: a solid accent fill.
    fn primary(self) -> Self {
        self.with_variant(ButtonVariant::Primary)
    }

    /// A destructive action.
    fn danger(self) -> Self {
        self.with_variant(ButtonVariant::Danger)
    }

    /// No surface until the pointer is over it.
    fn ghost(self) -> Self {
        self.with_variant(ButtonVariant::Ghost)
    }
}

/// Dimensions for one button size, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ButtonMetrics {
    height: f32,
    padding: f32,
    text: f32,
    icon: f32,
    gap: f32,
}

fn metrics(size: Size) -> ButtonMetrics {
    match size {
        Size::XSmall => ButtonMetrics {
            height: 22.0,
            padding: 7.0,
            text: 11.0,
            icon: 12.0,
            gap: 4.0,
        },
        Size::Small => ButtonMetrics {
            height: 26.0,
            padding: 9.0,
            text: 11.5,
            icon: 13.0,
            gap: 5.0,
        },
        Size::Medium | Size::Points(_) => ButtonMetrics {
            height: 30.0,
            padding: 12.0,
            text: 12.5,
            icon: 14.0,
            gap: 6.0,
        },
    }
}

/// How a button paints, resolved from its variant and state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    /// A raised neutral control.
    Secondary,
    /// No surface until the pointer is over it.
    Ghost,
    /// A glass key in a tone color.
    Filled,
    /// A tinted surface and border in a tone color.
    Tinted,
}

/// A button with a mono label and a dithered surface.
#[derive(IntoElement)]
pub struct LiquidButton {
    base: Stateful<Div>,
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<Icon>,
    children: Vec<AnyElement>,
    variant: ButtonVariant,
    tone: Option<Hsla>,
    outline: bool,
    size: Size,
    disabled: bool,
    readable_disabled: bool,
    selected: bool,
    loading: bool,
    dropdown_caret: bool,
    tooltip: Option<SharedString>,
    on_click: Option<ClickHandler>,
}

impl LiquidButton {
    /// Creates a secondary button.
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            base: div().id(id.clone()),
            id,
            label: None,
            icon: None,
            children: Vec::new(),
            variant: ButtonVariant::Default,
            tone: None,
            outline: false,
            size: Size::Medium,
            disabled: false,
            readable_disabled: false,
            selected: false,
            loading: false,
            dropdown_caret: false,
            tooltip: None,
            on_click: None,
        }
    }

    /// Sets the label.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the icon shown before the label.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Draws a tinted surface instead of a solid fill.
    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    /// Tints the button with a custom color, for actions that carry
    /// their own meaning such as rebase commands.
    pub fn tone(mut self, color: Hsla) -> Self {
        self.tone = Some(color);
        self
    }

    /// Shows a moving dither in place of the icon and ignores clicks.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Adds a down chevron after the label, for menu triggers.
    pub fn dropdown_caret(mut self, caret: bool) -> Self {
        self.dropdown_caret = caret;
        self
    }

    /// Shows a tooltip while the pointer rests on the button.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Calls `handler` when the user clicks the button.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Disables the button with a label that stays readable. Use it for
    /// main actions such as Commit, so the user can still read the action.
    pub fn disabled_readable(mut self, disabled: bool, _: &App) -> Self {
        self.disabled = disabled;
        self.readable_disabled = disabled;
        self
    }

    // Returns the tone color of filled and tinted buttons.
    fn tone_color(&self, cx: &App) -> Option<Hsla> {
        if let Some(tone) = self.tone {
            return Some(tone);
        }
        let theme = cx.theme();
        let colors = theme.liquid();
        match self.variant {
            ButtonVariant::Primary => Some(theme.primary),
            ButtonVariant::Danger => Some(colors.danger),
            _ => None,
        }
    }

    fn look(&self, tone: Option<Hsla>) -> Look {
        match (self.variant, tone) {
            (ButtonVariant::Ghost, None) => Look::Ghost,
            (_, Some(_)) if self.outline || self.tone.is_some() => Look::Tinted,
            (_, Some(_)) => Look::Filled,
            _ => Look::Secondary,
        }
    }

    /// Takes the click handler, so that a wrapper can call it after its own work.
    pub(super) fn take_on_click(&mut self) -> Option<ClickHandler> {
        self.on_click.take()
    }

    /// Replaces the click handler.
    pub(super) fn set_on_click(&mut self, handler: ClickHandler) {
        self.on_click = Some(handler);
    }
}

impl ButtonVariants for LiquidButton {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Sizable for LiquidButton {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Selectable for LiquidButton {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Disableable for LiquidButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for LiquidButton {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for LiquidButton {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for LiquidButton {}

impl ParentElement for LiquidButton {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// Opens a popup menu below an element when the user clicks it.
pub trait DropdownMenu: Sized {
    /// Opens the menu that `builder` fills below the element's leading edge.
    fn dropdown_menu(
        self,
        builder: impl Fn(PopupMenu, &mut Window, &mut gpui::Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> DropdownMenuElement {
        self.dropdown_menu_with_anchor(Anchor::TopLeft, builder)
    }

    /// Opens the menu that `builder` fills at the corner of the element that `anchor` names.
    fn dropdown_menu_with_anchor(
        self,
        anchor: Anchor,
        builder: impl Fn(PopupMenu, &mut Window, &mut gpui::Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> DropdownMenuElement;
}

impl DropdownMenu for LiquidButton {
    fn dropdown_menu_with_anchor(
        self,
        anchor: Anchor,
        builder: impl Fn(PopupMenu, &mut Window, &mut gpui::Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> DropdownMenuElement {
        DropdownMenuElement::new(self, anchor, Rc::new(builder))
    }
}

/// How a button stands off its surface.
#[derive(Clone, Copy, PartialEq)]
enum Lift {
    /// Flat on its surface.
    Flat,
    /// A neutral control with the depth of a badge.
    Raised,
    /// A control tinted in a tone with the depth of a badge.
    Tinted(Hsla),
    /// No surface at rest and a light line under its top edge under the pointer.
    Ghost,
}

// Returns the light of a button with `lift`, under the pointer when `hovered`.
fn lift_light(lift: Lift, theme: Theme, hovered: bool) -> Vec<BoxShadow> {
    match lift {
        Lift::Flat => Vec::new(),
        Lift::Raised => kit::raised_light(theme),
        Lift::Tinted(tone) => kit::tint_light(tone),
        Lift::Ghost if hovered => kit::plate_light(theme, false),
        Lift::Ghost => Vec::new(),
    }
}

/// Colors for one painted state.
#[derive(Clone, Copy)]
struct Surface {
    bg: Background,
    border: Hsla,
    fg: Hsla,
    hover_bg: Background,
    hover_border: Hsla,
    hover_fg: Hsla,
    active_bg: Hsla,
    /// The depth of the surface. A press puts its light out and sinks the surface.
    lift: Lift,
    /// Static dots that shade the bottom of the surface.
    shade: Option<Hsla>,
    /// Dots that appear while the pointer is over the button.
    glint: Option<(Hsla, Edge)>,
}

fn surface(look: Look, tone: Option<Hsla>, cx: &App) -> Surface {
    let theme = cx.theme();
    let colors = theme.liquid();
    let accent = theme.primary;
    match look {
        Look::Secondary => Surface {
            bg: kit::raised_fill(theme.button),
            border: theme.input,
            fg: theme.foreground,
            hover_bg: kit::raised_fill(theme.button_hover),
            hover_border: accent.opacity(0.5),
            hover_fg: theme.foreground,
            active_bg: theme.button_active,
            lift: Lift::Raised,
            shade: Some(colors.ink.opacity(0.9)),
            glint: Some((accent.opacity(0.5), Edge::Bottom)),
        },
        Look::Ghost => Surface {
            bg: gpui::transparent_black().into(),
            border: gpui::transparent_black(),
            fg: colors.text_secondary,
            hover_bg: theme.list_hover.into(),
            hover_border: gpui::transparent_black(),
            hover_fg: theme.foreground,
            active_bg: theme.button_active,
            lift: Lift::Ghost,
            shade: None,
            glint: Some((colors.ink, Edge::Bottom)),
        },
        // The glass body draws the face, its dots and its rings, so the surface stays clear.
        Look::Filled => Surface {
            bg: gpui::transparent_black().into(),
            border: gpui::transparent_black(),
            fg: theme.primary_foreground,
            hover_bg: gpui::transparent_black().into(),
            hover_border: gpui::transparent_black(),
            hover_fg: theme.primary_foreground,
            active_bg: gpui::transparent_black(),
            lift: Lift::Flat,
            shade: None,
            glint: None,
        },
        Look::Tinted => {
            let tone = tone.unwrap_or(accent);
            Surface {
                bg: kit::tint_fill(tone, 0.09),
                border: tone.opacity(0.45),
                fg: tone,
                hover_bg: kit::tint_fill(tone, 0.16),
                hover_border: tone.opacity(0.7),
                hover_fg: tone,
                active_bg: tone.opacity(0.22),
                lift: Lift::Tinted(tone),
                shade: Some(tone.opacity(0.3)),
                glint: Some((tone.opacity(0.6), Edge::Bottom)),
            }
        }
    }
}

impl RenderOnce for LiquidButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.liquid();
        let accent = theme.primary;
        let metrics = metrics(self.size);
        let tone = self.tone_color(cx);
        let look = self.look(tone);
        let mut paint = surface(look, tone, cx);
        if self.selected && !self.disabled {
            paint = Surface {
                bg: kit::tint_fill(accent, 0.16),
                border: accent.opacity(0.6),
                fg: accent,
                hover_bg: kit::tint_fill(accent, 0.22),
                hover_border: accent.opacity(0.75),
                hover_fg: accent,
                active_bg: accent.opacity(0.26),
                lift: Lift::Tinted(accent),
                shade: Some(accent.opacity(0.4)),
                glint: None,
            };
        }
        if self.disabled {
            let fg = if self.readable_disabled {
                colors.text_secondary
            } else {
                colors.text_tertiary
            };
            paint = Surface {
                bg: if look == Look::Filled {
                    tone.unwrap_or(accent).opacity(0.14).into()
                } else if look == Look::Ghost {
                    gpui::transparent_black().into()
                } else {
                    theme.muted.into()
                },
                border: if look == Look::Ghost {
                    gpui::transparent_black()
                } else {
                    theme.border
                },
                fg,
                hover_bg: gpui::transparent_black().into(),
                hover_border: gpui::transparent_black(),
                hover_fg: fg,
                active_bg: gpui::transparent_black(),
                lift: Lift::Flat,
                shade: None,
                glint: None,
            };
        }

        let interactive = !self.disabled && !self.loading;
        let icon_only = self.label.is_none() && self.children.is_empty();
        // The main action is a glass key in its tone, unless it is selected or disabled.
        let glass = (look == Look::Filled && !self.selected && !self.disabled)
            .then(|| tone.unwrap_or(accent));
        let kodem = theme.kodem;

        let mut base = self.base;
        let instance_style = base.style().clone();
        base = base
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .h(px(metrics.height))
            .when(icon_only, |button| button.w(px(metrics.height)))
            .when(!icon_only, |button| button.px(px(metrics.padding)))
            .rounded(px(RADIUS_CONTROL))
            .overflow_hidden()
            .border_1()
            .border_color(paint.border)
            .bg(paint.bg)
            .shadow(lift_light(paint.lift, kodem, false))
            .text_color(paint.fg)
            .font_family(crate::theme::mono())
            .text_size(text_px(metrics.text))
            .font_weight(if look == Look::Filled {
                gpui::FontWeight::SEMIBOLD
            } else {
                gpui::FontWeight::MEDIUM
            })
            .when(interactive && glass.is_none(), |button| {
                let hover = paint;
                button
                    .cursor_pointer()
                    .hover(move |style| {
                        style
                            .bg(hover.hover_bg)
                            .border_color(hover.hover_border)
                            .text_color(hover.hover_fg)
                            .shadow(lift_light(hover.lift, kodem, true))
                    })
                    // A pressed button sinks into the surface: its light goes out and the edge
                    // above casts a shadow into it.
                    .active(move |style| {
                        style.bg(hover.active_bg).shadow(kit::recessed_light(kodem))
                    })
            })
            .when_some(glass, |button, tone| {
                button
                    .group(kit::GLASS)
                    .shadow(kit::glass_glow(tone, kodem, Pose::Rest))
                    .when(interactive, |button| {
                        button
                            .cursor_pointer()
                            .hover(move |style| {
                                style.shadow(kit::glass_glow(tone, kodem, Pose::Hover))
                            })
                            .active(move |style| {
                                style.shadow(kit::glass_glow(tone, kodem, Pose::Pressed))
                            })
                    })
            });
        // Caller styles (width, alignment, colors) win over the defaults.
        base.style().refine(&instance_style);

        if let Some(tone) = glass {
            base = base.layer(kit::glass_body(tone));
        }

        if let Some(shade) = paint.shade {
            base = base.layer(
                Dither::new(DitherField::Fade {
                    from: Edge::Bottom,
                    start: 0.4,
                    end: -0.1,
                })
                .color(shade)
                .grid(2.0, 1.0),
            );
        }
        if let Some((glint, from)) = paint.glint.filter(|_| interactive) {
            base = base.layer(
                Dither::new(DitherField::Fade {
                    from,
                    start: 0.75,
                    end: -0.05,
                })
                .color(glint)
                .grid(2.0, 1.0)
                .hover_only(),
            );
        }

        let icon = if self.loading {
            Some(
                div()
                    .relative()
                    .size(px(metrics.icon - 2.0))
                    .child(
                        Dither::new(DitherField::Flow {
                            scale: 0.5,
                            speed: 8.0,
                            bias: 0.0,
                        })
                        .color(paint.fg)
                        .grid(2.0, 1.5)
                        .animated()
                        .layer(),
                    )
                    .into_any_element(),
            )
        } else {
            self.icon.map(|icon| match glass {
                Some(tone) => engraved(icon, tone, paint.fg, metrics.icon).into_any_element(),
                None => icon
                    .size(px(metrics.icon))
                    .text_color(paint.fg)
                    .into_any_element(),
            })
        };

        let content = h_flex()
            .relative()
            .w_full()
            .h_full()
            .items_center()
            .justify_center()
            .gap(px(metrics.gap))
            .children(icon)
            .when_some(self.label, |content, label| {
                content.child(
                    div()
                        .flex_none()
                        .line_height(text_px(metrics.text + 2.0))
                        .whitespace_nowrap()
                        .child(label),
                )
            })
            .children(self.children)
            .when(self.dropdown_caret, |content| {
                content.child(
                    Icon::new(IconName::ChevronDown)
                        .size(px(metrics.icon - 2.0))
                        .text_color(paint.fg.opacity(0.7)),
                )
            });
        // Keep the caller's alignment, such as `justify_start`, for the label.
        let content = match instance_style.justify_content {
            Some(justify) => {
                let mut content = content;
                content.style().justify_content = Some(justify);
                content
            }
            None => content,
        };

        let id = self.id;
        // A glass key ripples in its tone, as the main action of the window does.
        let ripple_tone = glass.unwrap_or(paint.fg);
        base.child(content)
            .when(interactive, |button| {
                // A dithered ring spreads from the press point.
                button.on_mouse_down(gpui::MouseButton::Left, move |event, _, cx| {
                    fx::ripple(event.position, ripple_tone, cx);
                })
            })
            .when_some(self.tooltip, |button, tooltip| {
                button.tooltip(super::dialog::tooltip(tooltip))
            })
            .when_some(self.on_click.filter(|_| interactive), |button, on_click| {
                button.on_click(move |event, window, cx| on_click(event, window, cx))
            })
            .debug_selector(move || format!("git-button-{id:?}"))
    }
}

// Returns `icon` engraved into a glass face of `tone`: a darker copy one point above it reads as
// the edge of a cut.
fn engraved(icon: Icon, tone: Hsla, color: Hsla, size: f32) -> Div {
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .child(
            icon.clone()
                .size(px(size))
                .absolute()
                .top(gpui::px(-1.))
                .left_0()
                .text_color(kit::toward(tone, 0., 0.55).opacity(0.75)),
        )
        .child(
            icon.size(px(size))
                .absolute()
                .top_0()
                .left_0()
                .text_color(color),
        )
}
