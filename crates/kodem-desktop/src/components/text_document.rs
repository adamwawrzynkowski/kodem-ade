//! Coordinates selection across the independently laid out blocks of a transcript.
use super::text_input::{SelectionInput, TextInput};
use gpui::{prelude::*, *};
use std::time::Duration;
use std::{collections::BTreeMap, ops::Range};

struct Block {
    input: Entity<TextInput>,
    _subscription: Subscription,
}

#[derive(Clone)]
struct Position {
    input: EntityId,
    byte: usize,
}

/// Reading order of the blocks of one list row: a message's sequence number, then the part of
/// the message that the row shows.
pub type RowOrder = (u64, usize);

#[derive(Default)]
pub struct TextDocument {
    groups: BTreeMap<RowOrder, Vec<Block>>,
    anchor: Option<Position>,
    head: Option<Position>,
    dragging: bool,
    scroll: Option<ListState>,
    orders: Vec<RowOrder>,
    bottom_inset: Pixels,
    pointer: Option<Point<Pixels>>,
    scrolling: bool,
}

impl TextDocument {
    /// Shares the virtual transcript's current order and viewport for drag selection.
    pub fn set_viewport(&mut self, scroll: ListState, orders: Vec<RowOrder>, bottom_inset: Pixels) {
        self.scroll = Some(scroll);
        self.orders = orders;
        self.bottom_inset = bottom_inset;
    }

    fn extend_at(&mut self, pointer: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(scroll) = &self.scroll else {
            return;
        };
        let viewport = scroll.viewport_bounds();
        let mut closest = None;
        let mut distance = f32::MAX;
        for (index, order) in self.orders.iter().enumerate() {
            let Some(bounds) = scroll.bounds_for_item(index) else {
                continue;
            };
            if bounds.bottom() < viewport.top()
                || bounds.top() > viewport.bottom() - self.bottom_inset
            {
                continue;
            }
            for block in self.groups.get(order).into_iter().flatten() {
                let input = block.input.read(cx);
                let Some(bounds) = input.text_bounds() else {
                    continue;
                };
                let dy = (bounds.top() - pointer.y)
                    .max(pointer.y - bounds.bottom())
                    .max(px(0.));
                let dx = (bounds.left() - pointer.x)
                    .max(pointer.x - bounds.right())
                    .max(px(0.));
                let delta = f32::from(dy) * 1000. + f32::from(dx);
                if delta < distance {
                    distance = delta;
                    closest = Some(Position {
                        input: block.input.entity_id(),
                        byte: input.byte_at(pointer),
                    });
                }
            }
        }
        if let Some(head) = closest {
            self.head = Some(head);
            self.sync(cx);
        }
    }

    fn start_scroll(&mut self, cx: &mut Context<Self>) {
        if self.scrolling {
            return;
        }
        self.scrolling = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(32))
                    .await;
                if !this
                    .update(cx, |doc, cx| {
                        if !doc.dragging {
                            doc.scrolling = false;
                            return false;
                        }
                        if let Some(pointer) = doc.pointer
                            && let Some(scroll) = &doc.scroll
                        {
                            let bounds = scroll.viewport_bounds();
                            let delta = if pointer.y < bounds.top() + px(20.) {
                                -12.
                            } else if pointer.y > bounds.bottom() - doc.bottom_inset - px(20.) {
                                12.
                            } else {
                                0.
                            };
                            if delta != 0. {
                                scroll.scroll_by(px(delta));
                                doc.extend_at(pointer, cx);
                                cx.notify();
                            }
                        }
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// Registers a row's blocks in reading order, preserving selection on re-render.
    pub fn register(
        &mut self,
        order: RowOrder,
        inputs: Vec<Entity<TextInput>>,
        cx: &mut Context<Self>,
    ) {
        if self.groups.get(&order).is_none_or(|blocks| {
            !blocks
                .iter()
                .map(|b| b.input.entity_id())
                .eq(inputs.iter().map(Entity::entity_id))
        }) {
            let blocks = inputs
                .into_iter()
                .map(|input| {
                    input.update(cx, |input, _| input.delegate_selection());
                    let id = input.entity_id();
                    let subscription = cx.subscribe(&input, move |doc, _, event, cx| {
                        doc.selection_event(id, event, cx)
                    });
                    Block {
                        input,
                        _subscription: subscription,
                    }
                })
                .collect();
            self.groups.insert(order, blocks);
        }
        self.sync(cx);
    }

    fn selection_event(&mut self, id: EntityId, event: &SelectionInput, cx: &mut Context<Self>) {
        match event {
            SelectionInput::Begin { range, extend } => {
                if !extend || self.anchor.is_none() {
                    self.anchor = Some(Position {
                        input: id,
                        byte: range.start,
                    });
                }
                self.head = Some(Position {
                    input: id,
                    byte: range.end,
                });
                self.dragging = true;
                self.pointer = None;
                self.start_scroll(cx);
            }
            SelectionInput::Move { byte } if self.dragging => {
                self.head = Some(Position {
                    input: id,
                    byte: *byte,
                })
            }
            SelectionInput::End => self.dragging = false,
            SelectionInput::Copy => self.copy(cx),
            SelectionInput::SelectAll => self.select_all(cx),
            _ => {}
        }
        self.sync(cx);
    }

    /// Returns the selected text of all blocks, one block per line, or `None` without a
    /// selection.
    pub fn selected_text(&self, cx: &App) -> Option<String> {
        let mut selected = Vec::new();
        let endpoints = self.endpoints()?;
        for (index, block) in self.groups.values().flatten().enumerate() {
            let text = block.input.read(cx).text();
            let range = selection_range(index, text.len(), endpoints.0, endpoints.1);
            let range = text.floor_char_boundary(range.start)..text.floor_char_boundary(range.end);
            if !range.is_empty() {
                selected.push(text[range].to_owned());
            }
        }
        (!selected.is_empty()).then(|| selected.join("\n"))
    }

    /// Writes the selected text to the clipboard. Without a selection the clipboard keeps its
    /// content.
    pub fn copy(&self, cx: &mut App) {
        if let Some(text) = self.selected_text(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Selects the text of every registered block.
    pub fn select_all(&mut self, cx: &mut App) {
        self.anchor = self.groups.values().flatten().next().map(|b| Position {
            input: b.input.entity_id(),
            byte: 0,
        });
        self.head = self.groups.values().flatten().last().map(|b| Position {
            input: b.input.entity_id(),
            byte: b.input.read(cx).text().len(),
        });
        self.sync(cx);
    }

    fn endpoints(&self) -> Option<((usize, usize), (usize, usize))> {
        let point = |p: &Position| {
            self.groups
                .values()
                .flatten()
                .position(|b| b.input.entity_id() == p.input)
                .map(|i| (i, p.byte))
        };
        self.anchor
            .as_ref()
            .and_then(point)
            .zip(self.head.as_ref().and_then(point))
    }

    fn sync(&self, cx: &mut App) {
        let endpoints = self.endpoints();
        for (index, block) in self.groups.values().flatten().enumerate() {
            let range = endpoints.map_or(0..0, |(a, b)| {
                selection_range(index, block.input.read(cx).text_len(), a, b)
            });
            block
                .input
                .update(cx, |input, cx| input.set_selection(range, cx));
        }
    }
}

fn selection_range(index: usize, len: usize, a: (usize, usize), b: (usize, usize)) -> Range<usize> {
    let (start, end) = if a <= b { (a, b) } else { (b, a) };
    if index < start.0 || index > end.0 {
        return 0..0;
    }
    let from = if index == start.0 {
        start.1.min(len)
    } else {
        0
    };
    let to = if index == end.0 { end.1.min(len) } else { len };
    from..to
}

impl Render for TextDocument {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let pointer_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble || !pointer_entity.read(cx).dragging {
                        return;
                    }
                    pointer_entity.update(cx, |doc, cx| {
                        if event.pressed_button != Some(MouseButton::Left) {
                            doc.dragging = false;
                            return;
                        }
                        doc.pointer = Some(event.position);
                        doc.extend_at(event.position, cx);
                    });
                });
                let release_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        release_entity.update(cx, |doc, _| doc.dragging = false);
                    }
                });
            },
        )
        .absolute()
        .inset_0()
    }
}
