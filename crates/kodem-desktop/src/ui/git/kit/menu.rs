//! Popup menus of the Git workspace: right-click menus on rows, dropdown menus on buttons and
//! the menus that a dropped branch or commit opens. A menu takes focus while it is open, so the
//! arrow keys, Return and Escape reach it.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    Anchor, AnyElement, App, Bounds, ClickEvent, Context, DismissEvent, Element, ElementId, Entity,
    EventEmitter, FocusHandle, Focusable, GlobalElementId, Hitbox, HitboxBehavior,
    InspectorElementId, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement,
    Pixels, Point, ScrollHandle, SharedString, StyleRefinement, Styled, Subscription, Window,
    anchored, canvas, deferred, div, point,
};

use super::button::LiquidButton;
use super::{Icon, IconName, Sizable as _};
use crate::theme::{px, text_px};
use crate::ui::dither::{self, DottedRule};
use crate::ui::fx;
use crate::ui::git::motion;
use crate::ui::git::theme::ActiveTheme;
use crate::ui::kit;

/// Paint order of menus above other deferred layers.
const CONTEXT_MENU_PRIORITY: usize = 100;
type MenuBuilder = dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu;
type ItemHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type ItemRender = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// One entry of a popup menu.
pub struct PopupMenuItem {
    content: ItemContent,
    icon: Option<Icon>,
    disabled: bool,
    checked: bool,
    handler: Option<ItemHandler>,
}

enum ItemContent {
    Label(SharedString),
    Element(ItemRender),
}

impl PopupMenuItem {
    /// Creates an item with a label.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            content: ItemContent::Label(label.into()),
            icon: None,
            disabled: false,
            checked: false,
            handler: None,
        }
    }

    /// Creates an item that draws its own content.
    pub fn element<E: IntoElement>(render: impl Fn(&mut Window, &mut App) -> E + 'static) -> Self {
        Self {
            content: ItemContent::Element(Rc::new(move |window, cx| {
                render(window, cx).into_any_element()
            })),
            icon: None,
            disabled: false,
            checked: false,
            handler: None,
        }
    }

    /// Sets the icon before the label.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Stops the item from reacting to the user.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Shows a check after the label.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Calls `handler` when the user chooses the item. The menu closes first.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Some(Rc::new(handler));
        self
    }
}

enum Entry {
    Item(PopupMenuItem),
    Label(SharedString),
    Separator,
    Submenu {
        label: SharedString,
        icon: Option<Icon>,
        menu: Entity<PopupMenu>,
    },
}

impl Entry {
    // Reports whether the keyboard can move onto the entry.
    fn selectable(&self) -> bool {
        match self {
            Entry::Item(item) => !item.disabled && item.handler.is_some(),
            Entry::Submenu { .. } => true,
            Entry::Label(_) | Entry::Separator => false,
        }
    }
}

/// A list of actions that floats above the window.
pub struct PopupMenu {
    focus_handle: FocusHandle,
    entries: Vec<Entry>,
    min_width: Option<Pixels>,
    max_height: Option<Pixels>,
    action_context: Option<FocusHandle>,
    /// The entry under the keyboard selection or the pointer.
    highlighted: Option<usize>,
    /// The entry whose submenu is open.
    open_submenu: Option<usize>,
    /// Top of the open submenu in window coordinates: the height where it opened.
    submenu_top: Pixels,
    /// Where the menu was painted, so that a press inside an open submenu does not close the
    /// menu that opened it.
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DismissEvent> for PopupMenu {}

impl Focusable for PopupMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl PopupMenu {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            entries: Vec::new(),
            min_width: None,
            max_height: None,
            action_context: None,
            highlighted: None,
            open_submenu: None,
            submenu_top: Pixels::ZERO,
            bounds: Rc::default(),
            scroll: ScrollHandle::new(),
            _subscriptions: Vec::new(),
        }
    }

    /// Creates a menu that `build` fills.
    pub fn build(
        window: &mut Window,
        cx: &mut App,
        build: impl FnOnce(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu,
    ) -> Entity<PopupMenu> {
        cx.new(|cx| {
            let menu = PopupMenu::new(cx);
            build(menu, window, cx)
        })
    }

    /// Returns focus to `handle` when the menu closes.
    pub fn action_context(mut self, handle: FocusHandle) -> Self {
        self.action_context = Some(handle);
        self
    }

    /// Sets the narrowest width of the menu.
    pub fn min_w(mut self, width: impl Into<Pixels>) -> Self {
        self.min_width = Some(width.into());
        self
    }

    /// Sets the tallest height of the menu; longer menus scroll.
    pub fn max_h(mut self, height: impl Into<Pixels>) -> Self {
        self.max_height = Some(height.into());
        self
    }

    /// Adds an item.
    pub fn item(mut self, item: PopupMenuItem) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }

    /// Adds a quiet line of text that cannot be chosen.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.entries.push(Entry::Label(label.into()));
        self
    }

    /// Adds a rule between two groups. A rule at the top or after another rule is dropped.
    pub fn separator(mut self) -> Self {
        if !matches!(self.entries.last(), None | Some(Entry::Separator)) {
            self.entries.push(Entry::Separator);
        }
        self
    }

    /// Adds an item that opens a nested menu beside this one.
    pub fn submenu(
        self,
        label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.submenu_with_icon(None, label, window, cx, build)
    }

    /// Adds an item with an icon that opens a nested menu beside this one.
    pub fn submenu_with_icon(
        mut self,
        icon: Option<Icon>,
        label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        let menu = PopupMenu::build(window, cx, build);
        // Choosing an item of the nested menu closes the whole chain.
        self._subscriptions
            .push(cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
                this.open_submenu = None;
                cx.emit(DismissEvent);
            }));
        self.entries.push(Entry::Submenu {
            label: label.into(),
            icon,
            menu,
        });
        self
    }

    /// Reports whether the menu has nothing to show.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // Reports whether a window point lies on this menu or on one of its open submenus.
    fn contains(&self, position: Point<Pixels>, cx: &App) -> bool {
        if self
            .bounds
            .get()
            .is_some_and(|bounds| bounds.contains(&position))
        {
            return true;
        }
        self.open_submenu
            .and_then(|index| match self.entries.get(index) {
                Some(Entry::Submenu { menu, .. }) => Some(menu.clone()),
                _ => None,
            })
            .is_some_and(|menu| menu.read(cx).contains(position, cx))
    }

    // Closes the menu and gives focus back to where it was.
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = &self.action_context {
            window.focus(handle, cx);
        }
        cx.emit(DismissEvent);
    }

    // Runs the action of an entry: an item runs its handler, a submenu opens.
    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        match self.entries.get(index) {
            Some(Entry::Item(item)) if !item.disabled => {
                let Some(handler) = item.handler.clone() else {
                    return;
                };
                self.dismiss(window, cx);
                handler(&ClickEvent::default(), window, cx);
            }
            Some(Entry::Submenu { menu, .. }) => {
                if self.open_submenu != Some(index) {
                    self.submenu_top = window.mouse_position().y - px(14.);
                }
                self.open_submenu = Some(index);
                let focus = menu.read(cx).focus_handle.clone();
                window.focus(&focus, cx);
                cx.notify();
            }
            _ => {}
        }
    }

    // Moves the keyboard selection by `step` selectable entries, wrapping at the ends.
    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        let selectable: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.selectable())
            .map(|(index, _)| index)
            .collect();
        if selectable.is_empty() {
            return;
        }
        let current = self
            .highlighted
            .and_then(|index| selectable.iter().position(|candidate| *candidate == index));
        let next = match current {
            Some(position) => {
                (position as isize + step).rem_euclid(selectable.len() as isize) as usize
            }
            None if step < 0 => selectable.len() - 1,
            None => 0,
        };
        self.highlighted = Some(selectable[next]);
        self.scroll.scroll_to_item(selectable[next]);
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.dismiss(window, cx),
            "down" => self.step(1, cx),
            "up" => self.step(-1, cx),
            "enter" | "space" => {
                if let Some(index) = self.highlighted {
                    self.activate(index, window, cx);
                }
            }
            "right" => {
                if let Some(index) = self
                    .highlighted
                    .filter(|index| matches!(self.entries.get(*index), Some(Entry::Submenu { .. })))
                {
                    self.activate(index, window, cx);
                }
            }
            "left" => {
                if self.open_submenu.take().is_some() {
                    window.focus(&self.focus_handle, cx);
                    cx.notify();
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_entry(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let t = theme.kodem;
        let highlighted = self.highlighted == Some(index);
        let row = |id: ElementId| {
            div()
                .id(id)
                .relative()
                .w_full()
                .min_h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .overflow_hidden()
                .font_family(crate::theme::mono())
                .text_size(text_px(11.5))
                .font_weight(gpui::FontWeight::MEDIUM)
        };
        match &self.entries[index] {
            Entry::Separator => div()
                .my(px(3.))
                .mx(px(4.))
                .child(DottedRule::horizontal(t.subtle.opacity(0.4)))
                .into_any_element(),
            Entry::Label(label) => div()
                .px(px(8.))
                .py(px(5.))
                .max_w(px(360.))
                .text_size(text_px(11.5))
                .line_height(gpui::relative(1.45))
                .text_color(t.subtle)
                .child(label.clone())
                .into_any_element(),
            Entry::Submenu { label, icon, menu } => {
                let open = self.open_submenu == Some(index);
                let menu = menu.clone();
                let submenu = open.then(|| {
                    let bounds = self.bounds.get().unwrap_or_default();
                    deferred(
                        anchored()
                            .position(point(bounds.right() - px(4.), self.submenu_top))
                            .anchor(Anchor::TopLeft)
                            .snap_to_window_with_margin(px(8.))
                            .child(menu.clone()),
                    )
                    .with_priority(CONTEXT_MENU_PRIORITY + 1)
                });
                row(("git-menu-submenu", index).into())
                    .text_color(if open || highlighted { t.text } else { t.muted })
                    .when(open || highlighted, |row| row.bg(t.selected))
                    .hover(move |row| row.bg(t.selected).text_color(t.text))
                    .cursor_pointer()
                    .child(kit::glint(t.dot, dither::Edge::Bottom))
                    .children(icon.clone().map(|icon| icon.size(px(13.))))
                    .child(div().flex_1().whitespace_nowrap().child(label.clone()))
                    .child(Icon::new(IconName::ChevronRight).size(px(12.)))
                    .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                        if *hovered {
                            if this.open_submenu != Some(index) {
                                this.submenu_top = window.mouse_position().y - px(14.);
                            }
                            this.highlighted = Some(index);
                            this.open_submenu = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate(index, window, cx);
                    }))
                    .children(submenu)
                    .into_any_element()
            }
            Entry::Item(item) => {
                let disabled = item.disabled;
                let interactive = !disabled && item.handler.is_some();
                let checked = item.checked;
                let icon = item.icon.clone();
                let content = match &item.content {
                    ItemContent::Label(label) => div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_nowrap()
                        .child(label.clone())
                        .into_any_element(),
                    ItemContent::Element(render) => div()
                        .flex_1()
                        .min_w_0()
                        .child(render(window, cx))
                        .into_any_element(),
                };
                let tone = if disabled { t.subtle } else { t.muted };
                row(("git-menu-item", index).into())
                    .text_color(if highlighted && interactive {
                        t.text
                    } else {
                        tone
                    })
                    .when(highlighted && interactive, |row| row.bg(t.selected))
                    .when(interactive, |row| {
                        row.cursor_pointer()
                            .child(kit::glint(t.dot, dither::Edge::Bottom))
                            .hover(move |row| row.bg(t.selected).text_color(t.text))
                            .active(move |row| row.bg(t.control_active))
                            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                                fx::ripple(event.position, t.muted, cx);
                            })
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                if *hovered {
                                    this.highlighted = Some(index);
                                    this.open_submenu = None;
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.activate(index, window, cx);
                            }))
                    })
                    .children(icon.map(|icon| {
                        icon.size(px(13.))
                            .text_color(if disabled { t.subtle } else { t.muted })
                    }))
                    .child(content)
                    .when(checked, |row| {
                        row.child(Icon::new(IconName::Check).xsmall().text_color(t.accent))
                    })
                    .into_any_element()
            }
        }
    }
}

impl Render for PopupMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let t = theme.kodem;
        let entries: Vec<AnyElement> = (0..self.entries.len())
            .map(|index| self.render_entry(index, window, cx))
            .collect();
        let bounds = self.bounds.clone();
        let viewport = window.viewport_size();
        let max_height = self
            .max_height
            .unwrap_or(viewport.height)
            .min((viewport.height - px(16.)).max(px(0.)));
        div()
            .id("git-popup-menu")
            .track_focus(&self.focus_handle)
            .key_context("GitPopupMenu")
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                if !this.contains(event.position, cx) {
                    this.dismiss(window, cx);
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .relative()
            .child(dither::shadow(t, crate::theme::RADIUS_FLOATING, 5.))
            .child(
                div()
                    .id("git-popup-menu-body")
                    .track_scroll(&self.scroll)
                    .overflow_y_scroll()
                    .max_h(max_height)
                    .when_some(self.min_width, |menu, width| menu.min_w(width))
                    .max_w(px(420.))
                    .p(px(4.))
                    .flex()
                    .flex_col()
                    .rounded(px(crate::theme::RADIUS_FLOATING))
                    .bg(t.raised)
                    .border_1()
                    .border_color(t.edge)
                    .shadow(t.floating())
                    .text_color(t.text)
                    .children(entries),
            )
            .child(
                canvas(
                    move |measured, _, _| bounds.set(Some(measured)),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
    }
}

/// Floats `menu` above the window at `position` and dissolves it in. The
/// menu takes focus so keys reach it. `sequence` restarts the animation.
pub fn popup_menu_layer(
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    anchor: Anchor,
    sequence: u64,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    if !menu.focus_handle(cx).contains_focused(window, cx) {
        let focus = menu.focus_handle(cx);
        window.focus(&focus, cx);
    }
    deferred(
        anchored().child(
            div()
                .w(window.bounds().size.width)
                .h(window.bounds().size.height)
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(
                    anchored()
                        .position(position)
                        .snap_to_window_with_margin(px(8.0))
                        .anchor(anchor)
                        .child(motion::context_menu_surface(sequence, menu, cx)),
                ),
        ),
    )
    .with_priority(CONTEXT_MENU_PRIORITY)
    .into_any()
}

/// Opens a popup menu where the user right-clicks an element.
pub trait ContextMenuExt: InteractiveElement + ParentElement + Styled {
    /// Builds the menu with `builder` each time the user right-clicks the element.
    #[track_caller]
    fn context_menu(
        mut self,
        builder: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> ContextMenu<Self>
    where
        Self: Sized,
    {
        let caller = std::panic::Location::caller();
        let id = self
            .interactivity()
            .element_id
            .clone()
            .map(|id| ElementId::Name(format!("context-menu-{id:?}").into()))
            .unwrap_or_else(|| ElementId::CodeLocation(*caller));
        ContextMenu::new(id, self).menu(builder)
    }
}

impl<E: InteractiveElement + ParentElement + Styled> ContextMenuExt for E {}

/// An element with a right-click menu.
pub struct ContextMenu<E: ParentElement + Styled + Sized> {
    id: ElementId,
    element: Option<E>,
    menu: Option<Rc<MenuBuilder>>,
    ignored_style: StyleRefinement,
    anchor: Anchor,
}

impl<E: ParentElement + Styled> ContextMenu<E> {
    fn new(id: impl Into<ElementId>, element: E) -> Self {
        Self {
            id: id.into(),
            element: Some(element),
            menu: None,
            ignored_style: StyleRefinement::default(),
            anchor: Anchor::TopLeft,
        }
    }

    fn menu<F>(mut self, builder: F) -> Self
    where
        F: Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    {
        self.menu = Some(Rc::new(builder));
        self
    }

    fn with_element_state<R>(
        &mut self,
        id: &GlobalElementId,
        window: &mut Window,
        cx: &mut App,
        operation: impl FnOnce(&mut Self, &mut ContextMenuState, &mut Window, &mut App) -> R,
    ) -> R {
        window.with_optional_element_state::<ContextMenuState, _>(
            Some(id),
            |element_state, window| {
                let mut element_state = element_state.flatten().unwrap_or_default();
                let result = operation(self, &mut element_state, window, cx);
                (result, Some(element_state))
            },
        )
    }
}

impl<E: ParentElement + Styled> ParentElement for ContextMenu<E> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        if let Some(element) = &mut self.element {
            element.extend(elements);
        }
    }
}

impl<E: ParentElement + Styled> Styled for ContextMenu<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        if let Some(element) = &mut self.element {
            element.style()
        } else {
            &mut self.ignored_style
        }
    }
}

impl<E: ParentElement + Styled + IntoElement + 'static> IntoElement for ContextMenu<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct ContextMenuSharedState {
    menu_view: Option<Entity<PopupMenu>>,
    open: bool,
    position: Point<Pixels>,
    animation_sequence: u64,
    previous_focus_handle: Option<FocusHandle>,
    subscription: Option<Subscription>,
}

/// Element state of a right-click menu: the element it wraps and the open menu.
pub struct ContextMenuState {
    element: Option<AnyElement>,
    shared_state: Rc<RefCell<ContextMenuSharedState>>,
}

impl Default for ContextMenuState {
    fn default() -> Self {
        Self {
            element: None,
            shared_state: Rc::new(RefCell::new(ContextMenuSharedState {
                menu_view: None,
                open: false,
                position: Default::default(),
                animation_sequence: 0,
                previous_focus_handle: None,
                subscription: None,
            })),
        }
    }
}

impl<E: ParentElement + Styled + IntoElement + 'static> Element for ContextMenu<E> {
    type RequestLayoutState = ContextMenuState;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let anchor = self.anchor;

        self.with_element_state(id.unwrap(), window, cx, |this, state, window, cx| {
            let (position, open, animation_sequence) = {
                let shared_state = state.shared_state.borrow();
                (
                    shared_state.position,
                    shared_state.open,
                    shared_state.animation_sequence,
                )
            };
            let menu_view = state.shared_state.borrow().menu_view.clone();
            let mut menu_element = None;
            if open {
                let has_menu_item = menu_view
                    .as_ref()
                    .map(|menu| !menu.read(cx).is_empty())
                    .unwrap_or(false);

                if has_menu_item && let Some(menu) = menu_view {
                    menu_element = Some(popup_menu_layer(
                        menu,
                        position,
                        anchor,
                        animation_sequence,
                        window,
                        cx,
                    ));
                }
            }

            let mut element = this
                .element
                .take()
                .expect("The context menu element must exist.")
                .children(menu_element)
                .into_any_element();
            let layout_id = element.request_layout(window, cx);

            (
                layout_id,
                ContextMenuState {
                    element: Some(element),
                    shared_state: state.shared_state.clone(),
                },
            )
        })
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(element) = &mut request_layout.element {
            element.prepaint(window, cx);
        }
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(element) = &mut request_layout.element {
            element.paint(window, cx);
        }

        let builder = self.menu.clone();
        let shared_state = request_layout.shared_state.clone();
        let hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase.bubble() && event.button == MouseButton::Right && hitbox.is_hovered(window) {
                let previous_focus_handle = {
                    let shared_state = shared_state.borrow();
                    if shared_state.open {
                        shared_state.previous_focus_handle.clone()
                    } else {
                        window.focused(cx)
                    }
                };

                {
                    let mut shared_state = shared_state.borrow_mut();
                    shared_state.menu_view = None;
                    shared_state.subscription = None;
                    shared_state.position = event.position;
                    shared_state.open = true;
                    shared_state.previous_focus_handle = previous_focus_handle.clone();
                    shared_state.animation_sequence =
                        shared_state.animation_sequence.wrapping_add(1);
                }

                window.defer(cx, {
                    let shared_state = shared_state.clone();
                    let builder = builder.clone();
                    move |window, cx| {
                        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
                            let menu = match previous_focus_handle.clone() {
                                Some(handle) => menu.action_context(handle),
                                None => menu,
                            };
                            let Some(build) = &builder else {
                                return menu;
                            };
                            build(menu, window, cx)
                        });

                        let subscription = window.subscribe(&menu, cx, {
                            let shared_state = shared_state.clone();
                            move |_, _: &DismissEvent, window, _| {
                                shared_state.borrow_mut().open = false;
                                window.refresh();
                            }
                        });

                        {
                            let mut state = shared_state.borrow_mut();
                            state.menu_view = Some(menu);
                            state.subscription = Some(subscription);
                            window.refresh();
                        }
                    }
                });
            }
        });
    }
}

/// Open state of a dropdown menu, kept across renders of its trigger.
#[derive(Default)]
struct DropdownState {
    menu: Option<Entity<PopupMenu>>,
    sequence: u64,
    trigger: Option<Bounds<Pixels>>,
    _subscription: Option<Subscription>,
}

/// A button that opens a popup menu when the user clicks it.
#[derive(IntoElement)]
pub struct DropdownMenuElement {
    button: LiquidButton,
    anchor: Anchor,
    builder: Rc<MenuBuilder>,
}

impl DropdownMenuElement {
    /// Wraps a button so that a click opens the menu that `builder` fills.
    pub(super) fn new(button: LiquidButton, anchor: Anchor, builder: Rc<MenuBuilder>) -> Self {
        Self {
            button,
            anchor,
            builder,
        }
    }
}

impl RenderOnce for DropdownMenuElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut button = self.button;
        let key =
            ElementId::Name(format!("git-dropdown-{:?}", button.interactivity().element_id).into());
        let state = window.use_keyed_state(key, cx, |_, _| DropdownState::default());
        let builder = self.builder;
        let open_state = state.clone();
        let previous = button.take_on_click();
        button.set_on_click(Rc::new(move |event, window, cx| {
            if let Some(previous) = &previous {
                previous(event, window, cx);
            }
            let previous_focus = window.focused(cx);
            let builder = builder.clone();
            let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
                let menu = match previous_focus {
                    Some(handle) => menu.action_context(handle),
                    None => menu,
                };
                builder(menu, window, cx)
            });
            let dismiss_state = open_state.clone();
            let subscription =
                window.subscribe(&menu, cx, move |_, _: &DismissEvent, window, cx| {
                    dismiss_state.update(cx, |state, _| state.menu = None);
                    window.refresh();
                });
            open_state.update(cx, |state, _| {
                state.menu = Some(menu);
                state.sequence = state.sequence.wrapping_add(1);
                state._subscription = Some(subscription);
            });
            window.refresh();
        }));
        let (menu, sequence, trigger) = {
            let state = state.read(cx);
            (state.menu.clone(), state.sequence, state.trigger)
        };
        let layer = menu.zip(trigger).map(|(menu, trigger)| {
            let (position, anchor) = match self.anchor {
                Anchor::TopRight => (trigger.bottom_right(), Anchor::TopRight),
                Anchor::BottomLeft => (trigger.origin, Anchor::BottomLeft),
                Anchor::BottomRight => (trigger.top_right(), Anchor::BottomRight),
                _ => (trigger.bottom_left(), Anchor::TopLeft),
            };
            let offset = if matches!(anchor, Anchor::BottomLeft | Anchor::BottomRight) {
                px(-4.)
            } else {
                px(4.)
            };
            popup_menu_layer(
                menu,
                point(position.x, position.y + offset),
                anchor,
                sequence,
                window,
                cx,
            )
        });
        let measure = state.clone();
        div()
            .relative()
            .flex_shrink_0()
            .child(button)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        measure.update(cx, |state, _| state.trigger = Some(bounds));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .children(layer)
    }
}
