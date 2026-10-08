use super::*;

pub(super) const COMPARISONS: [(Comparison, &str, &str); 5] = [
    (Comparison::WorkingTree, "All changes", "file"),
    (Comparison::Staged, "Staged", "check"),
    (Comparison::Unstaged, "Unstaged", "edit"),
    (Comparison::Baseline, "Since task", "branch"),
    (Comparison::Branch, "All branch changes", "branch"),
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
            .bg(if center { t.canvas } else { t.sidebar })
            .overflow_hidden()
            .when(!center, |panel| {
                panel.child(
                    div()
                        .h(px(36.))
                        .flex_shrink_0()
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
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
                            .px(px(7.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.text)
                            .accessibility_label("Choose changes comparison")
                            .child(icon("chevrons-up-down").size(px(12.)).text_color(t.subtle))
                            .on_click_with_bounds(cx.listener(
                                |s, bounds: &Bounds<Pixels>, _, cx| {
                                    s.menu_anchor = Some(bounds.top_right());
                                    s.panel = if s.panel == Panel::Comparison {
                                        Panel::None
                                    } else {
                                        Panel::Comparison
                                    };
                                    cx.notify();
                                },
                            )),
                        )
                        .child(figure(count.to_string(), t.subtle))
                        .child(div().flex_1())
                        .child(caps("Changes", t).mr(px(6.))),
                )
            });
        if self.new_diff.is_some() {
            panel = panel.child(
                div()
                    .px(px(12.))
                    .py(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .bg(t.accent_bg)
                    .border_b_1()
                    .border_color(t.accent.opacity(0.25))
                    .text_size(px(11.))
                    .text_color(t.accent)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child("New changes available. Your selected revision is preserved."),
                    )
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
        if let Some(snapshot) = &self.diff {
            for repo in &snapshot.repositories {
                if let Some(notice) = &repo.notice {
                    panel = panel.child(
                        div()
                            .px(px(12.))
                            .py(px(6.))
                            .flex()
                            .gap(px(8.))
                            .text_size(px(11.))
                            .text_color(t.muted)
                            .border_b_1()
                            .border_color(t.border)
                            .child(caps(&repo.label, t))
                            .child(div().flex_1().min_w_0().child(notice.clone())),
                    );
                }
            }
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
                            .gap(px(8.))
                            .child(caps("Clean", t).text_color(t.added))
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
                                .h(px(30.))
                                .px(px(14.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(caps(
                                    format!(
                                        "{} {}",
                                        snapshot.files.len(),
                                        if snapshot.files.len() == 1 {
                                            "file"
                                        } else {
                                            "files"
                                        }
                                    ),
                                    t,
                                ))
                                .child(div().flex_1())
                                .child(line_totals(added, removed, t)),
                        )
                        .child(files),
                );
            }
            if let Some(file) = snapshot.files.get(self.file) {
                let file_index = self.file;
                if let Some(root) = &file.repository {
                    let label = format!("Repository: {}", root.display());
                    let hint = label.clone();
                    panel = panel.child(
                        div()
                            .id("diff-repository")
                            .h(px(24.))
                            .flex_shrink_0()
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .gap_2()
                            .bg(t.inset)
                            .border_b_1()
                            .border_color(t.border)
                            .font_family(crate::theme::MONO)
                            .text_size(px(10.5))
                            .text_color(t.muted)
                            .child(icon("branch").size(px(12.)))
                            .child(div().min_w_0().text_ellipsis().child(label))
                            .tooltip(move |_, cx| {
                                cx.new(|_| Hint {
                                    text: hint.clone(),
                                    theme: t,
                                })
                                .into()
                            }),
                    );
                }
                panel = panel.child(
                    div()
                        .h(px(36.))
                        .flex_shrink_0()
                        .pl(px(12.))
                        .pr(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .bg(if center { t.sidebar } else { t.canvas })
                        .border_b_1()
                        .border_color(t.border)
                        .child(icon("file").size(px(13.)).text_color(t.subtle).mr(px(6.)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis_start()
                                .font_family(crate::theme::MONO)
                                .text_size(px(11.5))
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
                            self.icon_button(
                                "diff-wrap",
                                "wrap-text",
                                if self.settings.wrap_diff {
                                    "Disable word wrap"
                                } else {
                                    "Enable word wrap"
                                },
                            )
                            .when(self.settings.wrap_diff, |button| {
                                button.bg(t.selected).text_color(t.text)
                            })
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.settings.wrap_diff = !s.settings.wrap_diff;
                                s.save_settings();
                                cx.notify();
                            })),
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
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .bg(t.inset)
                            .border_b_1()
                            .border_color(t.border)
                            .child(
                                div()
                                    .w_1_2()
                                    .pl(px(12.))
                                    .child(caps("Before", t).text_color(t.removed)),
                            )
                            .child(
                                div()
                                    .w_1_2()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .pl(px(12.))
                                    .border_l_1()
                                    .border_color(t.border)
                                    .child(caps("After", t).text_color(t.added)),
                            ),
                    );
                }
                if let Some(notice) = &file.notice {
                    panel = panel.child(
                        div()
                            .px(px(12.))
                            .py(px(8.))
                            .text_size(px(12.))
                            .text_color(t.muted)
                            .border_b_1()
                            .border_color(t.border)
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
                            self.settings.wrap_diff,
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
                        view.configure(
                            self.split,
                            self.settings.wrap_diff,
                            t,
                            self.settings.light,
                            cx,
                        )
                    });
                    panel = panel.child(view.clone());
                }
                panel = panel.child(
                    div()
                        .h(px(26.))
                        .flex_shrink_0()
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .bg(t.sidebar)
                        .border_t_1()
                        .border_color(t.border)
                        .child(caps(
                            format!("Rev {}", &snapshot.id[..8.min(snapshot.id.len())]),
                            t,
                        ))
                        .child(div().flex_1())
                        .child(
                            div()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(px(11.))
                                .text_color(t.subtle)
                                .child("Select to copy · drag line numbers to comment"),
                        ),
                );
            }
        } else {
            panel = panel.child(
                div()
                    .p(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .text_color(t.muted)
                    .child(dither::pulse("review-loading", t.muted, cx.reduce_motion()))
                    .child("Reading repository changes…"),
            );
        }
        motion::reveal("review-enter", panel)
    }
}
