use super::*;

pub(super) const COMPARISONS: [(Comparison, &str, &str); 4] = [
    (Comparison::WorkingTree, "All changes", "file"),
    (Comparison::Staged, "Staged", "check"),
    (Comparison::Unstaged, "Unstaged", "edit"),
    (Comparison::Baseline, "Since task", "branch"),
];
impl Shell {
    /// Builds the file browser, snapshot diff and contextual feedback editor.
    pub(super) fn review(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let width = if self.window_width <= 1100. {
            self.window_width
                - if self.settings.collapsed {
                    54.
                } else {
                    self.settings.sidebar_width
                }
                - 2.
        } else {
            self.settings.review_width.min(self.window_width * 0.55)
        };
        let count = self.diff.as_ref().map_or(0, |d| d.files.len());
        let (_, label, glyph) = COMPARISONS
            .iter()
            .find(|(comparison, _, _)| *comparison == self.comparison)
            .unwrap();
        let mut panel = div()
            .w(px(width))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(t.inset)
            .overflow_hidden()
            .child(
                div()
                    .h(px(34.))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(t.canvas)
                    .border_b_1()
                    .border_color(t.border)
                    .child(
                        self.ghost("comparison-menu", glyph, *label)
                            .h(px(26.))
                            .text_size(px(12.))
                            .text_color(t.text)
                            .accessibility_label("Choose changes comparison")
                            .child(icon("chevron-down").size(px(11.)))
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.panel = if s.panel == Panel::Comparison {
                                    Panel::None
                                } else {
                                    Panel::Comparison
                                };
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t.subtle)
                            .child(count.to_string()),
                    )
                    .child(div().flex_1())
                    .child(
                        self.icon_button("refresh-diff", "refresh", "Refresh changes")
                            .on_click(cx.listener(|s, _, _, _| s.refresh_diff())),
                    )
                    .child(
                        self.icon_button(
                            "diff-mode",
                            "panel-right",
                            if self.split {
                                "Use unified diff"
                            } else {
                                "Use split diff"
                            },
                        )
                        .text_size(px(11.))
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.split = !s.split;
                            cx.notify();
                        })),
                    )
                    .child(
                        self.icon_button("close-review", "close", "Hide changes · ⌘D")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.show_review = false;
                                cx.notify();
                            })),
                    ),
            );
        if self.new_diff.is_some() {
            panel = panel.child(
                div()
                    .px_3()
                    .py_2()
                    .bg(t.accent_bg)
                    .text_size(px(11.))
                    .text_color(t.accent)
                    .child("New changes available. Your selected revision is preserved.")
                    .child(
                        self.ghost("use-current", "refresh", "Load latest")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.selection = None;
                                s.panel = Panel::None;
                                s.diff = s.new_diff.take();
                                s.file = 0;
                                s.reviewing_file = false;
                                cx.notify();
                            })),
                    ),
            );
        }
        if let Some(snapshot) = self.diff.clone() {
            if snapshot.files.is_empty() {
                return motion::reveal(
                    "review-enter",
                    panel.child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(icon("circle-check").size(px(25.)).text_color(t.added))
                            .child(div().text_size(px(13.)).child("No changes"))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.muted)
                                    .child("This comparison is up to date."),
                            ),
                    ),
                );
            }
            let mut files = div()
                .id("changed-files")
                .flex_1()
                .min_h_0()
                .py_1()
                .overflow_y_scroll()
                .bg(t.canvas)
                .border_b_1()
                .border_color(t.border);
            for (i, file) in snapshot.files.iter().enumerate() {
                let (directory, name) = file.label.rsplit_once('/').unwrap_or(("", &file.label));
                let added = file
                    .lines
                    .iter()
                    .filter(|l| l.kind == LineKind::Added)
                    .count();
                let removed = file
                    .lines
                    .iter()
                    .filter(|l| l.kind == LineKind::Removed)
                    .count();
                files = files.child(
                    div()
                        .id(("diff-file", i))
                        .h(px(30.))
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .when(self.reviewing_file && self.file == i, |d| d.bg(t.selected))
                        .hover(move |d| d.bg(t.raised))
                        .child(
                            div()
                                .w(px(18.))
                                .h(px(20.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .text_size(px(10.))
                                .bg(if file.status.contains('A') || file.status.contains('?') {
                                    t.added_bg
                                } else {
                                    t.accent_bg
                                })
                                .text_color(
                                    if file.status.contains('A') || file.status.contains('?') {
                                        t.added
                                    } else {
                                        t.accent
                                    },
                                )
                                .child(if file.status.contains('?') {
                                    "A".to_owned()
                                } else {
                                    file.status.trim().to_owned()
                                }),
                        )
                        .child(icon("file").size(px(13.)).text_color(t.muted))
                        .child(
                            div()
                                .min_w_0()
                                .max_w(relative(0.72))
                                .text_ellipsis()
                                .text_size(px(12.))
                                .child(name.to_owned()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(px(11.))
                                .text_color(t.subtle)
                                .child(directory.to_owned()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.added)
                                .child(format!("+{added}")),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.removed)
                                .child(format!("−{removed}")),
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.file = i;
                            s.reviewing_file = true;
                            s.selection = None;
                            if s.panel == Panel::Feedback {
                                s.panel = Panel::None;
                            }
                            s.diff_scroll.scroll_to_item(0, ScrollStrategy::Top);
                            cx.notify();
                        })),
                );
            }
            if !self.reviewing_file {
                let added = snapshot
                    .files
                    .iter()
                    .flat_map(|f| &f.lines)
                    .filter(|l| l.kind == LineKind::Added)
                    .count();
                let removed = snapshot
                    .files
                    .iter()
                    .flat_map(|f| &f.lines)
                    .filter(|l| l.kind == LineKind::Removed)
                    .count();
                return motion::reveal(
                    "review-enter",
                    panel
                        .child(
                            div()
                                .h(px(32.))
                                .px_3()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(10.))
                                .text_color(t.subtle)
                                .child(format!("{} files", snapshot.files.len()))
                                .child(div().flex_1())
                                .child(div().text_color(t.added).child(format!("+{added}")))
                                .child(div().text_color(t.removed).child(format!("−{removed}"))),
                        )
                        .child(files),
                );
            }
            if let Some(file) = snapshot.files.get(self.file) {
                let task_id = snapshot.task_id.clone();
                let snapshot_id = snapshot.id.clone();
                let file_index = self.file;
                panel = panel.child(
                    div()
                        .h(px(34.))
                        .flex_shrink_0()
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .bg(t.sidebar)
                        .border_b_1()
                        .border_color(t.border)
                        .child(
                            self.icon_button(
                                "back-to-files",
                                "chevron-left",
                                "Back to changed files",
                            )
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.reviewing_file = false;
                                s.selection = None;
                                cx.notify();
                            })),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(px(12.))
                                .child(file.label.clone()),
                        )
                        .child(
                            self.icon_button("stage-file", "plus", "Stage this file")
                                .text_size(px(11.))
                                .disabled(self.comparison == Comparison::Staged)
                                .on_click(cx.listener(move |s, _, _, _| {
                                    s.send(core::Request::StageFile {
                                        task_id: task_id.clone(),
                                        snapshot_id: snapshot_id.clone(),
                                        file: file_index,
                                    })
                                })),
                        ),
                );
                if self.split {
                    panel = panel.child(
                        div()
                            .h(px(26.))
                            .flex()
                            .text_size(px(10.))
                            .text_color(t.subtle)
                            .bg(t.sidebar)
                            .child(div().w_1_2().pl_3().child("BEFORE"))
                            .child(div().w_1_2().pl_3().child("AFTER")),
                    );
                }
                if let Some(notice) = &file.notice {
                    panel = panel.child(
                        div()
                            .p_3()
                            .text_size(px(12.))
                            .text_color(t.muted)
                            .child(notice.clone()),
                    );
                }
                let entity = cx.entity();
                let text_width = file
                    .lines
                    .iter()
                    .map(|line| {
                        line.text
                            .chars()
                            .map(|ch| if ch == '\t' { 4 } else { 1 })
                            .sum::<usize>()
                    })
                    .max()
                    .unwrap_or(0) as f32
                    * 7.5;
                if self.split {
                    let mut columns = div().flex_1().min_h_0().flex().overflow_hidden();
                    for (side, id) in [(Side::Old, "diff-old"), (Side::New, "diff-new")] {
                        let entity = entity.clone();
                        columns = columns.child(
                            div()
                                .id(id)
                                .w_1_2()
                                .h_full()
                                .overflow_x_scroll()
                                .border_r_1()
                                .border_color(t.border)
                                .child(
                                    uniform_list(
                                        (id, 0usize),
                                        file.lines.len(),
                                        move |range, _, cx| {
                                            entity.update(cx, |s, cx| {
                                                range
                                                    .map(|row| {
                                                        s.diff_row(row, Some(side.clone()), cx)
                                                    })
                                                    .collect::<Vec<_>>()
                                            })
                                        },
                                    )
                                    .track_scroll(&self.diff_scroll)
                                    .w(px((text_width + 70.).max(width / 2.)))
                                    .h_full(),
                                ),
                        );
                    }
                    panel = panel.child(columns);
                } else {
                    panel = panel.child(
                        div()
                            .id("diff-horizontal")
                            .flex_1()
                            .min_h_0()
                            .overflow_x_scroll()
                            .child(
                                uniform_list(
                                    "diff-lines",
                                    file.lines.len(),
                                    move |range, _, cx| {
                                        entity.update(cx, |s, cx| {
                                            range
                                                .map(|row| s.diff_row(row, None, cx))
                                                .collect::<Vec<_>>()
                                        })
                                    },
                                )
                                .track_scroll(&self.diff_scroll)
                                .w(px((text_width + 112.).max(width)))
                                .h_full(),
                            ),
                    );
                }
                if let Some(selection) = self.selection.clone() {
                    let start = selection.start.min(selection.end);
                    let end = selection.start.max(selection.end);
                    let mut feedback = div()
                        .flex_shrink_0()
                        .p_3()
                        .bg(t.raised)
                        .border_t_1()
                        .border_color(t.border)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(12.))
                                .child(icon("chat").text_color(t.accent))
                                .child(format!(
                                    "{} · lines {start}–{end}",
                                    if selection.side == Side::Old {
                                        "Before"
                                    } else {
                                        "After"
                                    }
                                ))
                                .child(div().flex_1())
                                .child(
                                    self.icon_button(
                                        "cancel-selection",
                                        "close",
                                        "Clear line selection",
                                    )
                                    .on_click(cx.listener(
                                        |s, _, _, cx| {
                                            s.selection = None;
                                            s.panel = Panel::None;
                                            cx.notify();
                                        },
                                    )),
                                ),
                        );
                    if self.panel == Panel::Feedback {
                        let diff_id = snapshot.id.clone();
                        let task_id = snapshot.task_id.clone();
                        feedback = feedback
                            .child(
                                div()
                                    .p_2()
                                    .min_h(px(72.))
                                    .max_h(px(140.))
                                    .bg(t.inset)
                                    .border_1()
                                    .border_color(t.border)
                                    .rounded(px(7.))
                                    .child(self.aux.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(11.))
                                            .text_color(t.muted)
                                            .child("Queued with your next message"),
                                    )
                                    .child(
                                        self.primary("save-feedback", "chat", "Queue feedback")
                                            .disabled(self.aux.read(cx).text().trim().is_empty())
                                            .on_click(cx.listener(move |s, _, _, cx| {
                                                s.send(core::Request::AddFeedback {
                                                    task_id: task_id.clone(),
                                                    snapshot_id: diff_id.clone(),
                                                    file: file_index,
                                                    side: selection.side.clone(),
                                                    start,
                                                    end,
                                                    comment: s.aux.read(cx).text(),
                                                });
                                            })),
                                    ),
                            );
                    } else {
                        feedback = feedback.child(
                            self.ghost("add-comment", "plus", "Add feedback on these lines")
                                .on_click(cx.listener(|s, _, w, cx| {
                                    s.aux.update(cx, |i, cx| i.set_text(String::new(), cx));
                                    s.panel = Panel::Feedback;
                                    w.focus(&s.aux.read(cx).focus_handle(cx), cx);
                                    cx.notify();
                                })),
                        );
                    }
                    panel = panel.child(feedback);
                }
                panel = panel.child(
                    div()
                        .h(px(28.))
                        .flex_shrink_0()
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_t_1()
                        .border_color(t.border)
                        .text_size(px(10.))
                        .text_color(t.subtle)
                        .child(icon("shield").size(px(12.)))
                        .child(format!(
                            "Revision {}",
                            &snapshot.id[..8.min(snapshot.id.len())]
                        ))
                        .child(div().flex_1())
                        .child("Drag the gutter to add feedback"),
                );
            }
        } else {
            panel = panel.child(
                div()
                    .p_5()
                    .text_size(px(12.))
                    .text_color(t.muted)
                    .child("Reading repository changes…"),
            );
        }
        motion::reveal("review-enter", panel)
    }
    fn diff_row(
        &mut self,
        index: usize,
        forced_side: Option<Side>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((snapshot_id, line)) = self.diff.as_ref().and_then(|s| {
            s.files
                .get(self.file)
                .and_then(|f| f.lines.get(index))
                .map(|l| (s.id.clone(), l.clone()))
        }) else {
            return div().h(px(24.)).into_any_element();
        };
        let theme = self.theme;
        if line.kind == LineKind::Header {
            return div()
                .w_full()
                .h(px(24.))
                .px_3()
                .bg(theme.raised)
                .text_size(px(11.))
                .text_color(theme.muted)
                .text_ellipsis()
                .child(line.text)
                .into_any_element();
        }
        let kind = line.kind.clone();
        let color = match kind {
            LineKind::Added => theme.added,
            LineKind::Removed => theme.removed,
            _ => theme.text,
        };
        if (forced_side == Some(Side::Old) && kind == LineKind::Added)
            || (forced_side == Some(Side::New) && kind == LineKind::Removed)
        {
            return div().w_full().h(px(24.)).bg(theme.inset).into_any_element();
        }
        let side = forced_side.clone().unwrap_or_else(|| {
            if kind == LineKind::Removed {
                Side::Old
            } else {
                Side::New
            }
        });
        let number = if side == Side::Old {
            line.old
        } else {
            line.new
        };
        let selected = self.selection.as_ref().is_some_and(|s| {
            s.side == side
                && number.is_some_and(|n| n >= s.start.min(s.end) && n <= s.start.max(s.end))
        });
        let side_down = side.clone();
        let side_move = side.clone();
        let gutter = div()
            .id(("gutter", index))
            .w(px(16.))
            .h_full()
            .flex_shrink_0()
            .cursor_crosshair()
            .bg(if selected {
                theme.accent
            } else {
                theme.sidebar
            })
            .child(if selected { "▎" } else { " " })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |s, _, _, cx| {
                    if let Some(number) = number {
                        s.selection = Some(Selection {
                            side: side_down.clone(),
                            start: number,
                            end: number,
                            dragging: true,
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(move |s, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left)
                    && let Some(selection) = &mut s.selection
                    && selection.dragging
                    && selection.side == side_move
                    && let Some(number) = number
                {
                    selection.end = number;
                    cx.notify();
                }
            }));
        let input = self.readonly(
            format!("diff:{snapshot_id}:{}:{index}:{side:?}", self.file),
            line.text,
            cx,
        );
        let extension = self
            .diff
            .as_ref()
            .and_then(|d| d.files.get(self.file))
            .and_then(|f| f.label.rsplit('.').next())
            .unwrap_or("")
            .to_owned();
        input.update(cx, |i, _| i.code(&extension, self.settings.light));
        let marker = match kind {
            LineKind::Added => "+",
            LineKind::Removed => "−",
            _ => " ",
        };
        let numbers = format!(
            "{:>4} {:>4} {marker}",
            line.old.map(|n| n.to_string()).unwrap_or_default(),
            line.new.map(|n| n.to_string()).unwrap_or_default()
        );
        let mut row = div()
            .w_full()
            .h(px(24.))
            .flex()
            .items_center()
            .font_family("JetBrains Mono")
            .text_size(px(12.))
            .text_color(color)
            .bg(if selected {
                theme.selected
            } else {
                match kind {
                    LineKind::Added => theme.added_bg,
                    LineKind::Removed => theme.removed_bg,
                    _ => theme.inset,
                }
            })
            .child(gutter);
        row = row
            .child(
                div()
                    .w(px(if forced_side.is_some() { 50. } else { 86. }))
                    .text_color(theme.muted)
                    .whitespace_nowrap()
                    .flex_shrink_0()
                    .child(if forced_side.is_some() {
                        format!(
                            "{:>4} {marker}",
                            number.map(|n| n.to_string()).unwrap_or_default()
                        )
                    } else {
                        numbers
                    }),
            )
            .child(div().flex_1().min_w_0().overflow_hidden().child(input));
        row.into_any_element()
    }
}
