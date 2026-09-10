use super::*;

pub(super) const COMPARISONS: [(Comparison, &str, &str); 4] = [
    (Comparison::WorkingTree, "All changes", "file"),
    (Comparison::Staged, "Staged", "check"),
    (Comparison::Unstaged, "Unstaged", "edit"),
    (Comparison::Baseline, "Since task", "branch"),
];
impl Shell {
    /// Builds the file browser, snapshot diff and contextual feedback editor.
    pub(super) fn review(&mut self, cx: &mut Context<Self>, center: bool) -> AnyElement {
        let t = self.theme;
        let width = if center {
            (self.window_width - self.settings.sidebar_width - self.settings.review_width).max(360.)
        } else if self.window_width <= 1100. {
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
            .relative()
            .id(if center {
                "center-review"
            } else {
                "side-review"
            })
            .when(center, |d| d.flex_1().min_w_0().min_h_0())
            .when(!center, |d| d.w(px(width)).h_full().flex_shrink_0())
            .flex()
            .flex_col()
            .bg(t.inset)
            .child(motion::window_texture(self.settings.light))
            .overflow_hidden()
            .when(!center, |panel| {
                panel.child(
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
                            self.ghost(
                                "comparison-menu",
                                glyph,
                                if self.historical_diff {
                                    "Saved task diff"
                                } else {
                                    *label
                                },
                            )
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
                        .child(div().flex_1()),
                )
            });
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
                                s.panel = Panel::None;
                                if let Some(diff) = s.new_diff.take() {
                                    s.install_diff(diff);
                                }
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
            let files = self.changes_tree(&snapshot, cx);
            if !self.reviewing_file
                || (!center && self.settings.diff_placement == DiffPlacement::Conversation)
            {
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
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(px(12.))
                                .child(file.label.clone()),
                        )
                        .child(
                            self.icon_button(
                                "diff-mode",
                                "panel-right",
                                "Toggle unified or split diff",
                            )
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.split = !s.split;
                                s.save_settings();
                                cx.notify();
                            })),
                        )
                        .child(
                            self.icon_button("refresh-diff", "refresh", "Refresh changes")
                                .on_click(cx.listener(|s, _, _, _| s.refresh_diff())),
                        )
                        .child(
                            self.icon_button("close-diff", "close", "Close diff")
                                .on_click(cx.listener(|s, _, window, cx| {
                                    s.reviewing_file = false;
                                    s.diff_view = None;
                                    window.focus(&s.input.read(cx).focus_handle(cx), cx);
                                    cx.notify();
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
                let replace = self
                    .diff_view
                    .as_ref()
                    .is_none_or(|view| !view.read(cx).matches(&snapshot.id, self.file));
                if replace {
                    let view = cx.new(|cx| {
                        diff_view::DiffView::new(
                            &snapshot,
                            self.file,
                            self.split,
                            t,
                            self.settings.light,
                            cx,
                        )
                    });
                    let task_id = snapshot.task_id.clone();
                    let snapshot_id = snapshot.id.clone();
                    self.diff_subscription = Some(cx.subscribe(
                        &view,
                        move |shell, _, event: &diff_view::ContextRequested, cx| {
                            shell.send(core::Request::AddFeedbackRows {
                                task_id: task_id.clone(),
                                snapshot_id: snapshot_id.clone(),
                                file: file_index,
                                first_row: event.first_row,
                                last_row: event.last_row,
                                comment: event.comment.clone(),
                            });
                            cx.notify();
                        },
                    ));
                    self.diff_view = Some(view);
                }
                if let Some(view) = &self.diff_view {
                    view.update(cx, |view, cx| {
                        view.configure(self.split, t, self.settings.light, cx)
                    });
                    panel = panel.child(view.clone());
                }
                panel = panel.child(
                    div()
                        .h(px(24.))
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
                        .child("Select text to copy · Drag line numbers to add context"),
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
}
