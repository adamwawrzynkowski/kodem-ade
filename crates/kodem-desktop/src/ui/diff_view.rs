//! Virtualized diff document with shared text selection and snapshot-backed comments.
use super::*;
use crate::components::text_input::{SelectionInput, TextInput};
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Position {
    row: usize,
    byte: usize,
}
#[derive(Clone)]
struct TextSelection {
    anchor: Position,
    head: Position,
    column: u8,
}
fn selection_bytes(
    lines: &[DiffLine],
    selection: Option<&TextSelection>,
    row: usize,
    column: u8,
) -> Range<usize> {
    let Some(selection) = selection else {
        return 0..0;
    };
    if selection.column != column
        || matches!(
            (column, &lines[row].kind),
            (1, LineKind::Added) | (2, LineKind::Removed)
        )
    {
        return 0..0;
    }
    let (a, b) = if selection.anchor <= selection.head {
        (selection.anchor, selection.head)
    } else {
        (selection.head, selection.anchor)
    };
    if row < a.row || row > b.row {
        return 0..0;
    }
    (if row == a.row { a.byte } else { 0 })..(if row == b.row {
        b.byte
    } else {
        lines[row].text.len()
    })
}
fn selected_text(lines: &[DiffLine], selection: &TextSelection) -> String {
    let a = selection.anchor.row.min(selection.head.row);
    let b = selection.anchor.row.max(selection.head.row);
    (a..=b)
        .filter(|&row| {
            !matches!(
                (selection.column, &lines[row].kind),
                (1, LineKind::Added) | (2, LineKind::Removed)
            )
        })
        .map(|row| {
            lines[row].text[selection_bytes(lines, Some(selection), row, selection.column)]
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Text,
    Gutter,
}
/// A comment ready to attach to the current composer.
pub(super) struct ContextRequested {
    pub first_row: usize,
    pub last_row: usize,
    pub comment: String,
}
/// Owns one immutable diff document, its selection, and its virtual viewport.
pub(super) struct DiffView {
    snapshot: String,
    file: usize,
    lines: Vec<DiffLine>,
    extension: String,
    split: bool,
    theme: Theme,
    light: bool,
    list: ListState,
    horizontal: ScrollHandle,
    text: Option<TextSelection>,
    gutter: Option<(usize, usize)>,
    comment_open: bool,
    pending_comment: bool,
    comment: Entity<TextInput>,
    drag: Option<Drag>,
    pointer: Option<Point<Pixels>>,
    bounds: Option<Bounds<Pixels>>,
    rows: HashMap<(usize, u8), Entity<TextInput>>,
    subscriptions: HashMap<(usize, u8), Subscription>,
    autoscroll_running: bool,
    focus: FocusHandle,
    text_width: f32,
}
impl EventEmitter<ContextRequested> for DiffView {}
impl DiffView {
    /// Creates a virtual document without constructing inputs for off-screen rows.
    pub(super) fn new(
        snapshot: &DiffSnapshot,
        file: usize,
        split: bool,
        theme: Theme,
        light: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let delta = &snapshot.files[file];
        let text_width = delta
            .lines
            .iter()
            .map(|l| {
                l.text
                    .chars()
                    .map(|c| if c == '\t' { 4 } else { 1 })
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(0) as f32
            * 7.5;
        let comment = cx.new(|cx| TextInput::new("Add context for the agent…", cx));
        Self {
            snapshot: snapshot.id.clone(),
            file,
            lines: delta.lines.clone(),
            extension: delta.label.rsplit('.').next().unwrap_or("").into(),
            split,
            theme,
            light,
            list: ListState::new(delta.lines.len(), ListAlignment::Top, px(240.))
                .with_uniform_item_height(px(24.)),
            horizontal: ScrollHandle::new(),
            text: None,
            gutter: None,
            comment_open: false,
            pending_comment: false,
            comment,
            drag: None,
            pointer: None,
            bounds: None,
            rows: HashMap::new(),
            subscriptions: HashMap::new(),
            autoscroll_running: false,
            focus: cx.focus_handle(),
            text_width,
        }
    }
    /// Identifies the immutable file currently displayed.
    pub(super) fn matches(&self, snapshot: &str, file: usize) -> bool {
        self.snapshot == snapshot && self.file == file
    }
    /// Applies presentation preferences while preserving document selection and scroll.
    pub(super) fn configure(
        &mut self,
        split: bool,
        theme: Theme,
        light: bool,
        cx: &mut Context<Self>,
    ) {
        let changed = self.split != split || self.light != light;
        if self.split != split {
            self.text = None;
            self.drag = None;
        }
        self.split = split;
        self.theme = theme;
        self.light = light;
        if changed {
            self.sync_selection(cx);
        }
    }
    /// Enables retry after the host rejects a context attachment.
    pub(super) fn context_failed(&mut self, cx: &mut Context<Self>) {
        self.pending_comment = false;
        cx.notify();
    }
    /// Reports whether a refresh must preserve the selected immutable snapshot.
    pub(super) fn has_selection(&self) -> bool {
        self.text
            .as_ref()
            .is_some_and(|selection| selection.anchor != selection.head)
            || self.gutter.is_some()
    }
    /// Clears a document selection while retaining its viewport and captured content.
    pub(super) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if self.pending_comment {
            return;
        }
        self.text = None;
        self.gutter = None;
        self.drag = None;
        self.comment_open = false;
        self.reset_list();
        self.sync_selection(cx);
    }
    fn range(&self) -> Option<(usize, usize)> {
        self.gutter.map(|(a, b)| (a.min(b), a.max(b)))
    }
    fn comment_index(&self) -> Option<usize> {
        self.comment_open
            .then(|| self.range().map(|(_, b)| b + 1))
            .flatten()
    }
    fn actual_row(&self, item: usize) -> usize {
        item - usize::from(self.comment_index().is_some_and(|i| item > i))
    }
    fn reset_list(&self) {
        let top = self.list.logical_scroll_top();
        self.list
            .reset_with_uniform_height(self.lines.len() + usize::from(self.comment_open), px(24.));
        self.list.scroll_to(top);
    }
    fn visible(&self, row: usize, column: u8) -> bool {
        !matches!(
            (column, &self.lines[row].kind),
            (1, LineKind::Added) | (2, LineKind::Removed)
        )
    }
    fn selected_bytes(&self, row: usize, column: u8) -> Range<usize> {
        selection_bytes(&self.lines, self.text.as_ref(), row, column)
    }
    fn sync_selection(&mut self, cx: &mut Context<Self>) {
        for (&(row, column), input) in &self.rows {
            let range = self.selected_bytes(row, column);
            input.update(cx, |input, cx| input.set_selection(range, cx));
        }
        cx.notify();
    }
    fn copy(&self, cx: &mut Context<Self>) {
        let Some(selection) = &self.text else {
            return;
        };
        let text = selected_text(&self.lines, selection);
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
    fn selection_event(
        &mut self,
        row: usize,
        column: u8,
        event: &SelectionInput,
        cx: &mut Context<Self>,
    ) {
        match event {
            SelectionInput::Begin { range, extend } => {
                if self.pending_comment {
                    return;
                }
                if self.comment_open {
                    self.comment_open = false;
                    self.reset_list();
                }
                self.gutter = None;
                let point = Position {
                    row,
                    byte: range.end,
                };
                if *extend && self.text.as_ref().is_some_and(|s| s.column == column) {
                    self.text.as_mut().unwrap().head = point;
                } else {
                    self.text = Some(TextSelection {
                        anchor: Position {
                            row,
                            byte: range.start,
                        },
                        head: point,
                        column,
                    });
                }
                self.drag = Some(Drag::Text);
                self.start_autoscroll(cx);
            }
            SelectionInput::Move { byte } if self.drag == Some(Drag::Text) => {
                if let Some(selection) = &mut self.text
                    && selection.column == column
                {
                    selection.head = Position { row, byte: *byte };
                }
            }
            SelectionInput::Copy => self.copy(cx),
            SelectionInput::SelectAll => {
                if let Some(last) = self.lines.len().checked_sub(1) {
                    self.text = Some(TextSelection {
                        anchor: Position { row: 0, byte: 0 },
                        head: Position {
                            row: last,
                            byte: self.lines[last].text.len(),
                        },
                        column,
                    });
                }
            }
            _ => {}
        }
        self.sync_selection(cx);
    }
    fn row_input(&mut self, row: usize, column: u8, cx: &mut Context<Self>) -> Entity<TextInput> {
        if let Some(input) = self.rows.get(&(row, column)) {
            return input.clone();
        }
        let text = self.lines[row].text.clone();
        let input = cx.new(|cx| {
            let mut input = TextInput::readonly(text, cx);
            input.delegate_selection();
            input.code(&self.extension, self.light);
            input
        });
        self.subscriptions.insert(
            (row, column),
            cx.subscribe(&input, move |view, _, event, cx| {
                view.selection_event(row, column, event, cx)
            }),
        );
        input.update(cx, |input, cx| {
            input.set_selection(self.selected_bytes(row, column), cx)
        });
        self.rows.insert((row, column), input.clone());
        input
    }
    fn gutter_begin(
        &mut self,
        row: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_comment || self.lines[row].kind == LineKind::Header {
            return;
        }
        if self.comment_open {
            self.comment_open = false;
            self.reset_list();
        }
        self.text = None;
        self.gutter = Some(if event.modifiers.shift {
            (self.gutter.map_or(row, |s| s.0), row)
        } else {
            (row, row)
        });
        self.drag = Some(Drag::Gutter);
        self.pointer = Some(event.position);
        self.start_autoscroll(cx);
        window.focus(&self.focus, cx);
        cx.stop_propagation();
        self.sync_selection(cx);
    }
    fn finish_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.drag.take() == Some(Drag::Gutter) && self.gutter.is_some() {
            self.comment_open = true;
            self.reset_list();
            if let (Some(index), Some(bounds)) = (self.comment_index(), self.bounds) {
                let top = self.list.logical_scroll_top();
                let comment_y =
                    (index as f32 - top.item_ix as f32) * 24. - f32::from(top.offset_in_item);
                // The newly inserted row has not been measured yet. Reveal its actual
                // editor height, rather than the ordinary 24px row estimate.
                if comment_y < 0. || comment_y + 164. > f32::from(bounds.size.height) {
                    let preceding = ((f32::from(bounds.size.height) - 164.).max(0.) / 24.) as usize;
                    self.list.scroll_to(ListOffset {
                        item_ix: index.saturating_sub(preceding),
                        offset_in_item: px(0.),
                    });
                }
            }
            window.focus(&self.comment.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
    fn pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pointer = Some(event.position);
        if event.pressed_button != Some(MouseButton::Left) {
            if self.drag.is_some() {
                self.finish_drag(window, cx);
            }
            return;
        }
        self.extend_at(event.position, cx);
    }
    fn extend_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.drag else {
            return;
        };
        let Some(bounds) = self.bounds else {
            return;
        };
        let top = self.list.logical_scroll_top();
        let distance = f32::from(position.y - bounds.top() + top.offset_in_item).max(0.);
        let row = (top.item_ix + (distance / 24.) as usize).min(self.lines.len().saturating_sub(1));
        if self.lines.is_empty() {
            return;
        }
        if drag == Drag::Gutter {
            if let Some(selection) = &mut self.gutter {
                selection.1 = row;
            }
        } else if let Some(selection) = &self.text {
            let column = selection.column;
            // Use the painted row's coordinates. List estimates can differ from
            // the text hitbox at partial rows and were overwriting child selections.
            let row = self
                .rows
                .iter()
                .find_map(|(&(index, side), input)| {
                    if side != column || index.abs_diff(row) > 2 {
                        return None;
                    }
                    input
                        .read(cx)
                        .text_bounds()
                        .filter(|b| position.y >= b.top() && position.y < b.bottom())
                        .map(|_| index)
                })
                .unwrap_or(row);
            if self.visible(row, column) {
                let byte = self
                    .rows
                    .get(&(row, column))
                    .map(|input| {
                        let input = input.read(cx);
                        let position = input
                            .text_bounds()
                            .map_or(position, |b| point(position.x, b.top() + px(9.)));
                        input.byte_at(position)
                    })
                    .unwrap_or(self.lines[row].text.len());
                self.text.as_mut().unwrap().head = Position { row, byte };
            }
        }
        self.sync_selection(cx);
    }
    fn start_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.autoscroll_running {
            return;
        }
        self.autoscroll_running = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(32))
                    .await;
                if this
                    .update(cx, |view, cx| {
                        if view.drag.is_none() {
                            view.autoscroll_running = false;
                            return false;
                        }
                        view.autoscroll(cx);
                        true
                    })
                    .ok()
                    != Some(true)
                {
                    break;
                }
            }
        })
        .detach();
    }
    fn autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.drag.is_none() {
            return;
        }
        let (Some(pointer), Some(bounds)) = (self.pointer, self.bounds) else {
            return;
        };
        let distance = if pointer.y < bounds.top() + px(20.) {
            -px(12.)
        } else if pointer.y > bounds.bottom() - px(20.) {
            px(12.)
        } else {
            return;
        };
        self.list.scroll_by(distance);
        self.extend_at(pointer, cx);
    }
    fn cell(&mut self, row: usize, column: u8, cx: &mut Context<Self>) -> AnyElement {
        let line = self.lines[row].clone();
        let t = self.theme;
        if !self.visible(row, column) {
            return div().h(px(24.)).flex_1().bg(t.inset).into_any_element();
        }
        let input = self.row_input(row, column, cx);
        input.update(cx, |input, _| input.code(&self.extension, self.light));
        let selected = self.range().is_some_and(|(a, b)| row >= a && row <= b);
        let marker = match line.kind {
            LineKind::Added => "+",
            LineKind::Removed => "−",
            _ => " ",
        };
        let numbers = if column == 0 {
            format!(
                "{:>4} {:>4} {marker}",
                line.old.map(|n| n.to_string()).unwrap_or_default(),
                line.new.map(|n| n.to_string()).unwrap_or_default()
            )
        } else {
            format!(
                "{:>4} {marker}",
                if column == 1 { line.old } else { line.new }
                    .map(|n| n.to_string())
                    .unwrap_or_default()
            )
        };
        div()
            .id(("diff-cell", row * 3 + column as usize))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| window.focus(&view.focus, cx)),
            )
            .flex()
            .items_center()
            .h(px(24.))
            // Virtual rows are layout roots: auto width uses the input's 400px intrinsic fallback.
            .when(column == 0, |d| d.w_full())
            .flex_1()
            .min_w_0()
            .font_family("JetBrains Mono")
            .text_size(px(12.))
            .bg(if selected {
                t.selected
            } else {
                match line.kind {
                    LineKind::Added => t.added_bg,
                    LineKind::Removed => t.removed_bg,
                    LineKind::Header => t.raised,
                    _ => t.inset,
                }
            })
            .child(
                div()
                    .id(("line-gutter", row * 3 + column as usize))
                    .w(px(if column == 0 { 102. } else { 66. }))
                    .h_full()
                    .flex_shrink_0()
                    .pl_2()
                    .border_l_2()
                    .border_color(if selected { t.accent } else { t.border })
                    .text_color(if selected { t.accent } else { t.muted })
                    .cursor_crosshair()
                    .whitespace_nowrap()
                    .child(numbers)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, event, window, cx| {
                            view.gutter_begin(row, event, window, cx)
                        }),
                    ),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .text_color(t.text)
                    .child(input),
            )
            .into_any_element()
    }
    fn render_row(&mut self, item: usize, cx: &mut Context<Self>) -> AnyElement {
        if self.comment_index() == Some(item) {
            return self.comment_editor(cx);
        }
        let row = self.actual_row(item);
        if row >= self.lines.len() {
            return div().h(px(24.)).into_any_element();
        }
        if self.split {
            div()
                .w_full()
                .flex()
                .h(px(24.))
                .child(self.cell(row, 1, cx))
                .child(self.cell(row, 2, cx))
                .into_any_element()
        } else {
            self.cell(row, 0, cx)
        }
    }
    fn comment_editor(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let range = self.range().unwrap();
        let label = format!("{} selected lines", range.1 - range.0 + 1);
        div()
            .w(self.bounds.map_or(px(360.), |bounds| bounds.size.width))
            .ml(-self.horizontal.offset().x)
            .h(px(164.))
            .p_3()
            .bg(t.selected)
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_size(px(11.)).text_color(t.muted).child(label))
            .child(
                div()
                    .id("diff-comment-input")
                    .p_2()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .rounded(px(7.))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.inset)
                    .child(self.comment.clone()),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel-diff-comment")
                            .disabled(self.pending_comment)
                            .h(px(28.))
                            .px_2()
                            .child(icon("close"))
                            .child("Cancel")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.gutter = None;
                                view.comment_open = false;
                                view.reset_list();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("attach-diff-comment")
                            .disabled(self.pending_comment)
                            .h(px(28.))
                            .px_2()
                            .gap_1()
                            .rounded(px(6.))
                            .bg(t.accent)
                            .text_color(t.canvas)
                            .child(icon("plus"))
                            .child("Add context")
                            .on_click(cx.listener(|view, _, _, cx| {
                                if !view.pending_comment
                                    && let Some((first_row, last_row)) = view.range()
                                {
                                    view.pending_comment = true;
                                    cx.emit(ContextRequested {
                                        first_row,
                                        last_row,
                                        comment: view.comment.read(cx).text(),
                                    });
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .into_any_element()
    }
}
impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let top = self.list.logical_scroll_top().item_ix;
        self.rows.retain(|(row, _), input| {
            row.abs_diff(top) < 256 || input.read(cx).focus_handle(cx).is_focused(window)
        });
        self.subscriptions
            .retain(|key, _| self.rows.contains_key(key));
        let entity = cx.entity();
        let bounds_entity = entity.clone();
        let pointer_entity = entity.clone();
        let width = if self.split {
            (self.text_width + 66.) * 2.
        } else {
            self.text_width + 102.
        };
        div()
            .id("diff-document")
            .w_full()
            .min_w_0()
            .key_context("DiffDocument")
            .track_focus(&self.focus)
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .on_action(
                cx.listener(|view, _: &crate::components::text_input::Copy, _, cx| view.copy(cx)),
            )
            .on_action(cx.listener(
                |view, _: &crate::components::text_input::SelectAll, _, cx| {
                    let column = view
                        .text
                        .as_ref()
                        .map_or(if view.split { 2 } else { 0 }, |selection| selection.column);
                    view.selection_event(0, column, &SelectionInput::SelectAll, cx);
                },
            ))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| view.finish_drag(window, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| view.finish_drag(window, cx)),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        bounds_entity.update(cx, |view, _| view.bounds = Some(bounds));
                    },
                    move |_, _, window, _| {
                        let entity = pointer_entity.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble && entity.read(cx).drag.is_some() {
                                entity.update(cx, |view, cx| view.pointer_move(event, window, cx));
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .id("diff-horizontal")
                    .size_full()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.horizontal)
                    .child(
                        list(self.list.clone(), move |row, _, cx| {
                            entity.update(cx, |view, cx| view.render_row(row, cx))
                        })
                        .w_full()
                        .min_w(px(width))
                        .h_full(),
                    ),
            )
    }
}
