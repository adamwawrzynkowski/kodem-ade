use super::*;

pub(super) const COMPARISONS: [(Comparison, &str, &str); 5] = [
    (Comparison::WorkingTree, "All changes", "file"),
    (Comparison::Staged, "Staged", "check"),
    (Comparison::Unstaged, "Unstaged", "edit"),
    (Comparison::Baseline, "Since task", "branch"),
    (Comparison::Branch, "All branch changes", "branch"),
];

// The title of one side of a split diff: a marker and label in the side's color, then a rule.
fn side_header(label: &str, tone: Hsla, theme: Theme) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(kit::marker(tone))
        .child(caps(label, theme).text_color(tone))
        .child(
            div()
                .flex_1()
                .child(dither::DottedRule::horizontal(tone.opacity(0.35))),
        )
}

impl Shell {
    /// Returns the name of a comparison for the open task. The baseline of a review is the
    /// merge base of what it examines, so that comparison holds the reviewed changes.
    pub(super) fn comparison_label(
        &self,
        comparison: &Comparison,
        label: &'static str,
    ) -> &'static str {
        if self.mode == TaskKind::Review && *comparison == Comparison::Baseline {
            "Reviewed changes"
        } else {
            label
        }
    }

    /// Builds the file browser, snapshot diff and contextual feedback editor.
    pub(super) fn review(&mut self, cx: &mut Context<Self>, center: bool) -> AnyElement {
        let t = self.theme;
        // The workspace rail, the sidebar when it is open, and the rules between the panes.
        let leading = workspaces::RAIL
            + if self.settings.collapsed {
                2.
            } else {
                self.settings.sidebar_width + 3.
            };
        let width = if center {
            (self.window_width - leading - self.settings.review_width).max(360.)
        } else if self.window_width <= 1100. {
            self.window_width - leading
        } else {
            self.settings.review_width.min(self.window_width * 0.55)
        };
        let count = self.diff.as_ref().map_or(0, |d| d.files.len());
        let (comparison, label, glyph) = COMPARISONS
            .iter()
            .find(|(comparison, _, _)| *comparison == self.comparison)
            .unwrap();
        let label = self.comparison_label(comparison, label);
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
                                    label
                                },
                            )
                            .px(px(7.))
                            .text_size(text_px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.text)
                            .accessibility_label("Choose changes comparison")
                            .child(icon("chevrons-up-down").size(px(12.)).text_color(t.subtle))
                            .on_click_with_bounds(cx.listener(
                                |s, bounds: &Bounds<Pixels>, _, cx| {
                                    s.menu_anchor = Some(*bounds);
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
        // A folder without Git has nothing to compare its files with.
        if !self.tracks_changes() {
            return motion::reveal(
                "review-enter",
                panel.child(div().flex_1().child(kit::empty_state(
                    "No Git repository",
                    "This folder has no Git repository, so its changes are not tracked.",
                    dither::DitherField::Glow {
                        x: 0.5,
                        y: 0.55,
                        radius: 0.7,
                        strength: 0.85,
                    },
                    t,
                ))),
            );
        }
        if self.new_diff.is_some() {
            panel = panel.child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .bg(t.accent.opacity(0.06))
                    .child(
                        div()
                            .px(px(12.))
                            .py(px(5.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(kit::chip("NEW", t.accent))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(11.))
                                    .text_color(t.muted)
                                    .child(
                                        "New changes available. Your selected revision is preserved.",
                                    ),
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
                    )
                    .child(dither::DottedRule::horizontal(t.accent.opacity(0.5))),
            );
        }
        if let Some(snapshot) = &self.diff {
            for repo in &snapshot.repositories {
                if let Some(notice) = &repo.notice {
                    panel = panel.child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .px(px(12.))
                                    .py(px(6.))
                                    .flex()
                                    .gap(px(8.))
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(11.))
                                    .text_color(t.muted)
                                    .child(kit::chip(repo.label.to_uppercase(), t.warning))
                                    .child(div().flex_1().min_w_0().child(notice.clone())),
                            )
                            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                    );
                }
            }
            if snapshot.files.is_empty() {
                return motion::reveal(
                    "review-enter",
                    panel.child(
                        div().flex_1().child(kit::empty_state(
                            "No changes",
                            "This comparison is up to date.",
                            dither::DitherField::Glow {
                                x: 0.5,
                                y: 0.55,
                                radius: 0.7,
                                strength: 0.85,
                            }
                            .times(dither::DitherField::Flow {
                                scale: 0.18,
                                speed: 0.9,
                                bias: 0.12,
                            }),
                            t,
                        )),
                    ),
                );
            }
            let files = self.changes_tree(snapshot, cx);
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
                            // The totals read as a section header above the tree.
                            kit::section_header(
                                if snapshot.files.len() == 1 {
                                    "File"
                                } else {
                                    "Files"
                                },
                                Some(snapshot.files.len()),
                                t,
                            )
                            .h(px(30.))
                            .flex_shrink_0()
                            .px(px(14.))
                            .child(div().relative().w(px(60.)).h(px(6.)).flex_shrink_0().child(
                                dither::DitherBar::new(
                                    vec![
                                        dither::BarSegment {
                                            weight: added as f32,
                                            color: t.added,
                                        },
                                        dither::BarSegment {
                                            weight: removed as f32,
                                            color: t.removed,
                                        },
                                    ],
                                    t.subtle.opacity(0.55),
                                ),
                            ))
                            .child(line_totals(added, removed, t)),
                        )
                        .child(files),
                );
            }
            if let Some(file) = snapshot.files.get(self.file) {
                let file_index = self.file;
                if let Some(root) = &file.repository {
                    let hint = format!("Repository: {}", root.display());
                    panel = panel.child(
                        div()
                            .id("diff-repository")
                            .h(px(24.))
                            .flex_shrink_0()
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .bg(t.header)
                            .child(icon("branch").size(px(12.)).text_color(t.subtle))
                            .child(caps("Repository", t))
                            .child(
                                div()
                                    .flex_shrink(1.)
                                    .min_w_0()
                                    .text_ellipsis_start()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(10.5))
                                    .text_color(t.muted)
                                    .child(root.display().to_string()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(8.))
                                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                            )
                            .tooltip(move |_, cx| {
                                cx.new(|_| Hint {
                                    text: hint.clone(),
                                    theme: t,
                                })
                                .into()
                            }),
                    );
                }
                let (added, removed) =
                    file.lines
                        .iter()
                        .fold((0, 0), |(added, removed), line| match line.kind {
                            LineKind::Added => (added + 1, removed),
                            LineKind::Removed => (added, removed + 1),
                            _ => (added, removed),
                        });
                panel = panel.child(
                    kit::block_title(
                        div()
                            .flex_shrink(1.)
                            .min_w_0()
                            .text_ellipsis_start()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(11.5))
                            .text_color(t.text)
                            .child(file.label.clone()),
                        t.info,
                        t,
                    )
                    .h(px(34.))
                    .pr(px(6.))
                    .child(kit::diffstat(added, removed, t))
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
                            button.bg(t.accent.opacity(0.1)).text_color(t.accent)
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
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .bg(t.header)
                            .border_b_1()
                            .border_color(t.border)
                            .child(
                                div()
                                    .w_1_2()
                                    .px(px(12.))
                                    .child(side_header("Before", t.removed, t)),
                            )
                            .child(
                                div()
                                    .w_1_2()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .px(px(12.))
                                    .border_l_1()
                                    .border_color(t.border)
                                    .child(side_header("After", t.added, t)),
                            ),
                    );
                }
                if let Some(notice) = &file.notice {
                    panel = panel.child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .px(px(12.))
                                    .py(px(7.))
                                    .flex()
                                    .gap(px(8.))
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(11.))
                                    .text_color(t.muted)
                                    .child(kit::chip("NOTE", t.info))
                                    .child(div().flex_1().min_w_0().child(notice.clone())),
                            )
                            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                    );
                }
                let replace = self
                    .diff_view
                    .as_ref()
                    .is_none_or(|view| !view.read(cx).matches(&snapshot.id, self.file));
                if replace {
                    let view = cx.new(|cx| {
                        diff_view::DiffView::new(
                            snapshot,
                            self.file,
                            self.split,
                            self.settings.wrap_diff,
                            t,
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
                        view.configure(self.split, self.settings.wrap_diff, t, cx)
                    });
                    panel = panel.child(view.clone());
                }
                panel = panel
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
                    .child(
                        div()
                            .h(px(26.))
                            .flex_shrink_0()
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .bg(t.header)
                            .child(caps("Rev", t))
                            .child(figure(
                                snapshot.id[..8.min(snapshot.id.len())].to_owned(),
                                t.muted,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(8.))
                                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(10.5))
                                    .text_color(t.subtle)
                                    .child("Select to copy · drag line numbers to comment"),
                            ),
                    );
            }
        } else {
            panel = panel.child(
                div()
                    .p(px(16.))
                    .child(kit::loader_row("Reading repository changes…", t)),
            );
        }
        motion::reveal("review-enter", panel)
    }
}
