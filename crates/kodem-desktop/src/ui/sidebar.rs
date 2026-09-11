use super::*;
impl Shell {
    /// Builds a project-filtered task feed with status, branch and saved review metadata.
    pub(super) fn sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        if self.settings.collapsed {
            return div()
                .relative()
                .w(px(54.))
                .flex_shrink_0()
                .h_full()
                .bg(t.sidebar)
                .flex()
                .flex_col()
                .items_center()
                .py_3()
                .gap_2()
                .child(motion::window_texture(
                    self.settings.light,
                    self.theme.accent,
                ))
                .child(
                    self.icon_button("rail-new", "edit", "New task · ⌘N")
                        .on_click(cx.listener(|s, _, w, cx| s.new_task(&NewTask, w, cx))),
                )
                .child(
                    self.icon_button("rail-open", "folder-open", "Open repository · ⌘O")
                        .on_click(cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx))),
                )
                .child(div().flex_1())
                .child(self.update_control(true, cx))
                .child(
                    self.icon_button("rail-settings", "settings", "Settings · ⌘,")
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.panel = Panel::Settings;
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        let project_label = self
            .workspace
            .as_ref()
            .and_then(|w| {
                w.projects
                    .iter()
                    .find(|p| Some(&p.id) == self.sidebar_project.as_ref())
            })
            .map_or_else(|| "All projects".to_owned(), |p| p.name.clone());
        let mut panel = div()
            .id("sidebar")
            .relative()
            .w(px(self.settings.sidebar_width))
            .flex_shrink_0()
            .h_full()
            .bg(t.sidebar)
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_size(px(12.))
            .child(motion::window_texture(
                self.settings.light,
                self.theme.accent,
            ))
            .child(
                div()
                    .px_3()
                    .pt_2()
                    .pb_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        self.ghost("search-tasks", "search", "Search")
                            .flex_1()
                            .justify_start()
                            .h(px(32.))
                            .on_click(cx.listener(|s, _, w, cx| {
                                s.search_visible = true;
                                w.focus(&s.search.read(cx).focus_handle(cx), cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        self.icon_button("new-task", "edit", "New task · ⌘N")
                            .on_click(cx.listener(|s, _, w, cx| s.new_task(&NewTask, w, cx))),
                    ),
            )
            .when(self.search_visible, |d| {
                d.child(
                    div()
                        .mx_3()
                        .mb_2()
                        .px_2()
                        .py_1()
                        .flex()
                        .items_center()
                        .rounded(px(7.))
                        .bg(t.inset)
                        .border_1()
                        .border_color(t.border)
                        .child(div().flex_1().min_w_0().child(self.search.clone()))
                        .child(
                            self.icon_button("close-search", "close", "Close search")
                                .on_click(cx.listener(|s, _, w, cx| {
                                    s.search_visible = false;
                                    s.search.update(cx, |i, cx| i.set_text(String::new(), cx));
                                    w.focus(&s.focus, cx);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                div()
                    .px_3()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        self.ghost("project-filter", "folder-closed", "")
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .h(px(32.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(project_label),
                            )
                            .child(icon("chevron-down").size(px(12.)))
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.panel = Panel::Projects;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.icon_button("sort-tasks", "settings-2", "Thread list settings")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.panel = Panel::Sort;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.icon_button("add-project", "plus", "Open repository · ⌘O")
                            .on_click(
                                cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx)),
                            ),
                    ),
            );
        let query = self.search.read(cx).text().to_lowercase();
        if self.sidebar_dirty {
            if let Some(workspace) = &self.workspace {
                let rows = crate::thread_list::rows(
                    workspace,
                    &self.settings,
                    crate::thread_list::Filter {
                        project: self.sidebar_project.as_deref(),
                        query: &query,
                        archived: self.show_archived,
                        settled_collapsed: self.settled_collapsed,
                        settled_projects: &self.settled_projects_collapsed,
                    },
                );
                let changed = rows.len() != self.sidebar_rows.len()
                    || rows
                        .iter()
                        .zip(&self.sidebar_rows)
                        .any(|(a, b)| !a.same_item(b));
                if changed {
                    let mut anchor = self.sidebar_scroll.logical_scroll_top();
                    if let Some(previous) = self.sidebar_rows.get(anchor.item_ix) {
                        anchor.item_ix = rows
                            .iter()
                            .position(|row| row.same_item(previous))
                            .unwrap_or(anchor.item_ix.min(rows.len().saturating_sub(1)));
                    }
                    self.sidebar_scroll.reset(rows.len());
                    self.sidebar_scroll.scroll_to(anchor);
                }
                self.sidebar_rows = rows;
            }
            self.sidebar_dirty = false;
        }
        let entity = cx.entity();
        let feed = list(self.sidebar_scroll.clone(), move |index, _, cx| {
            entity.update(cx, |s, cx| s.sidebar_row(index, cx))
        })
        .flex_1()
        .min_h_0()
        .w_full()
        .px_2()
        .pb_2();
        panel = panel.child(feed).child(
            div()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(t.border)
                .flex()
                .items_center()
                .gap_2()
                .child(
                    self.icon_button("archive-view", "archive", "Toggle archived tasks")
                        .when(self.show_archived, |b| {
                            b.bg(t.selected).text_color(t.accent)
                        })
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.show_archived = !s.show_archived;
                            s.sidebar_dirty = true;
                            cx.notify();
                        })),
                )
                .child(
                    self.ghost(
                        "settings",
                        "settings",
                        if self.settings.sidebar_width < 280.
                            && !matches!(
                                self.update_state,
                                crate::updater::State::Idle | crate::updater::State::Checking
                            )
                        {
                            ""
                        } else {
                            "Settings"
                        },
                    )
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.panel = Panel::Settings;
                        cx.notify();
                    })),
                )
                .child(div().flex_1())
                .child(self.update_control(false, cx))
                .child(
                    div()
                        .size(px(5.))
                        .rounded_full()
                        .bg(if self.workspace.is_some() {
                            t.added
                        } else {
                            t.subtle
                        }),
                ),
        );
        motion::reveal("sidebar-enter", panel)
    }
    fn sidebar_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        use crate::thread_list::Row;
        let Some(row) = self.sidebar_rows.get(index) else {
            return div().into_any_element();
        };
        let t = self.theme;
        match row {
            Row::Task { index, project, .. } => {
                let Some(workspace) = &self.workspace else {
                    return div().into_any_element();
                };
                let card =
                    self.task_card(&workspace.tasks[*index], &workspace.projects[*project], cx);
                if self.settings.group_tasks_by_project {
                    div().pl_2().child(card).into_any_element()
                } else {
                    card
                }
            }
            Row::Project { index, id, count } => {
                let Some(project) = self.workspace.as_ref().and_then(|w| w.projects.get(*index))
                else {
                    return div().into_any_element();
                };
                let collapsed = self.settings.collapsed_projects.contains(id)
                    && self.search.read(cx).text().is_empty();
                let id = id.clone();
                self.ghost(
                    SharedString::from(format!("project-group:{id}")),
                    if collapsed {
                        "chevron-right"
                    } else {
                        "chevron-down"
                    },
                    "",
                )
                .w_full()
                .h(px(34.))
                .justify_start()
                .min_w_0()
                .accessibility_label(format!(
                    "{} project {}",
                    if collapsed { "Expand" } else { "Collapse" },
                    project.name
                ))
                .child(project_icon(project, t))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(t.text)
                        .child(project.name.clone()),
                )
                .child(div().text_color(t.subtle).child(count.to_string()))
                .on_click(cx.listener(move |s, _, _, cx| {
                    if !s.settings.collapsed_projects.remove(&id) {
                        s.settings.collapsed_projects.insert(id.clone());
                    }
                    s.sidebar_dirty = true;
                    s.save_settings();
                    cx.notify();
                }))
                .into_any_element()
            }
            Row::Settled { project, count } => {
                let collapsed = project.as_ref().map_or(self.settled_collapsed, |id| {
                    self.settled_projects_collapsed.contains(id)
                });
                let project = project.clone();
                self.ghost(
                    SharedString::from(format!("settled:{}", project.as_deref().unwrap_or("all"))),
                    if collapsed && self.search.read(cx).text().is_empty() {
                        "chevron-right"
                    } else {
                        "chevron-down"
                    },
                    "Settled",
                )
                .w_full()
                .h(px(32.))
                .justify_start()
                .text_size(px(11.))
                .child(div().flex_1())
                .child(div().text_color(t.subtle).child(count.to_string()))
                .on_click(cx.listener(move |s, _, _, cx| {
                    if let Some(id) = &project {
                        if !s.settled_projects_collapsed.remove(id) {
                            s.settled_projects_collapsed.insert(id.clone());
                        }
                    } else {
                        s.settled_collapsed = !s.settled_collapsed;
                    }
                    s.sidebar_dirty = true;
                    cx.notify();
                }))
                .into_any_element()
            }
            Row::Empty(label) => div()
                .px_3()
                .py_5()
                .text_size(px(12.))
                .text_color(t.muted)
                .child(*label)
                .into_any_element(),
        }
    }
    fn task_card(
        &self,
        task: &core::Task,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let selected = self.selected.as_deref() == Some(&task.id);
        let tint = project_tint(project, self.theme);
        let radius = px(9.);
        let mut surface = if self.settings.light {
            t.inset
        } else {
            t.raised
        };
        if selected {
            surface = t.selected;
        }
        let title_color = t.text.opacity(0.94);
        let metadata = t.muted;
        let mut hover_surface = surface;
        hover_surface.lightness = (surface.lightness + 0.016).min(1.);
        let id = task.id.clone();
        let menu_id = id.clone();
        let needs_input = task.needs_input();
        let status_color = if needs_input {
            t.accent
        } else {
            match task.state {
                RunState::Starting | RunState::Running => t.accent,
                RunState::Waiting => t.accent,
                RunState::Failed | RunState::DeliveryUnknown => t.removed,
                RunState::Completed => t.muted,
                _ => t.muted,
            }
        };
        let status = if needs_input {
            "Needs input"
        } else {
            task.state.label()
        };
        let mut meta = div()
            .h(px(20.))
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(11.))
            .text_color(metadata)
            .child(icon("branch").size(px(11.)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(task.branch.clone()),
            );
        let review_hint = task.review.as_ref().map_or_else(|| "No saved task diff. A snapshot will be recorded when the next turn finishes.".to_owned(), |r| {
            if let Some(error) = &r.error { format!("Could not capture task diff: {error}") }
            else { format!("Saved task diff · {} ago\n+{} added · −{} removed\n{} files omitted from inline counts. Later checkout changes do not affect these totals.", elapsed(r.captured),r.added,r.removed,r.omitted_files) }
        });
        if let Some(review) = task.review.as_ref().filter(|r| r.error.is_none()) {
            let snapshot = review.snapshot_id.clone();
            let task_id = id.clone();
            meta = meta.child(
                self.ghost(
                    (ElementId::from("task-diff"), SharedString::from(id.clone())),
                    "code",
                    "",
                )
                .h(px(20.))
                .px_1()
                .gap_1()
                .disabled(snapshot.is_none())
                .accessibility_label("Open saved task diff")
                .when(review.omitted_files > 0, |b| b.child("~"))
                .child(
                    div()
                        .text_color(t.added)
                        .child(format!("+{}", review.added)),
                )
                .child(
                    div()
                        .text_color(t.removed)
                        .child(format!("−{}", review.removed)),
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    cx.stop_propagation();
                    s.select_task(task_id.clone(), cx);
                    s.show_review = true;
                    s.save_settings();
                    if let Some(id) = &snapshot {
                        s.historical_diff = true;
                        s.new_diff = None;
                        s.send(core::Request::LoadSnapshot { id: id.clone() });
                    }
                }))
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: review_hint.clone(),
                        theme: t,
                    })
                    .into()
                }),
            );
        } else {
            meta = meta.child(
                div()
                    .id((
                        ElementId::from("missing-task-diff"),
                        SharedString::from(id.clone()),
                    ))
                    .px_1()
                    .child("—")
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint {
                            text: review_hint.clone(),
                            theme: t,
                        })
                        .into()
                    }),
            );
        }
        meta = meta.child(
            div()
                .id((
                    ElementId::from("task-provider"),
                    SharedString::from(id.clone()),
                ))
                .child(icon("openai").size(px(14.)).text_color(t.muted))
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: "Codex · OpenAI".into(),
                        theme: t,
                    })
                    .into()
                }),
        );
        div()
            .id((ElementId::from("task"), SharedString::from(id.clone())))
            .relative()
            .overflow_hidden()
            .w_full()
            .mb(px(6.))
            .px(px(10.))
            .py(px(5.))
            .line_height(relative(1.25))
            .rounded(radius)
            .bg(surface)
            .border_1()
            .border_color(if selected {
                t.accent.opacity(0.7)
            } else {
                t.muted.opacity(0.12)
            })
            .shadow(vec![
                BoxShadow {
                    inset: true,
                    color: t.text.opacity(0.035),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(0.),
                    spread_radius: px(0.),
                },
                BoxShadow {
                    inset: false,
                    color: t.shadow,
                    offset: point(px(0.), px(2.)),
                    blur_radius: px(2.),
                    spread_radius: px(-1.),
                },
            ])
            .cursor_pointer()
            .hover(move |d| d.bg(hover_surface))
            .child(motion::project_dither(tint, radius))
            .when(selected, |row| {
                row.child(motion::selection_dither(t.accent)).child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(12.))
                        .bottom(px(12.))
                        .w(px(3.))
                        .rounded(px(2.))
                        .bg(t.accent),
                )
            })
            .child(
                div()
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .text_color(metadata)
                    .child(project_icon(project, self.theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(project.name.clone()),
                    )
                    .when(task.pinned, |d| {
                        d.child(icon("pin").size(px(10.)).text_color(t.accent))
                    })
                    .child(if task.state.active() || needs_input {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_color(status_color)
                            .when(needs_input, |d| {
                                d.child(motion::gleam(
                                    (
                                        ElementId::from("sidebar-status-label"),
                                        SharedString::from(id.clone()),
                                    ),
                                    status,
                                    t,
                                ))
                            })
                            .when(task.state.active() && !needs_input, |d| {
                                d.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_color(t.muted)
                                        .child(elapsed(task.updated)),
                                )
                            })
                            .child(motion::active_icon(
                                (
                                    ElementId::from("sidebar-status"),
                                    SharedString::from(id.clone()),
                                ),
                                if needs_input { "shield" } else { "loader" },
                                status_color,
                                task.state.active() && !needs_input,
                                cx.reduce_motion(),
                            ))
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(11.))
                            .text_color(status_color)
                            .child(format!("{} · {}", status, ago(task.updated)))
                            .into_any_element()
                    }),
            )
            .child(
                div().h(px(25.)).flex().items_center().child(
                    div()
                        .min_w_0()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(title_color)
                        .text_ellipsis()
                        .child(task.title.clone()),
                ),
            )
            .child(meta)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |s, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let viewport = window.viewport_size();
                    s.thread_menu = Some(ThreadMenu {
                        task_id: menu_id.clone(),
                        position: point(
                            event
                                .position
                                .x
                                .clamp(px(8.), (viewport.width - px(244.)).max(px(8.))),
                            event
                                .position
                                .y
                                .clamp(px(8.), (viewport.height - px(192.)).max(px(8.))),
                        ),
                    });
                    s.panel = Panel::ThreadMenu;
                    window.focus(&s.focus, cx);
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |s, _, _, cx| s.select_task(id.clone(), cx)))
            .into_any_element()
    }
}
/// Returns a stable, distinct project glyph and tint without filesystem work during rendering.
pub(super) fn project_icon(project: &Project, theme: Theme) -> AnyElement {
    if let ProjectIcon::Image(path) = &project.icon {
        return img(path.clone())
            .size(px(18.))
            .rounded(px(4.))
            .flex_shrink_0()
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                icon("file")
                    .size(px(18.))
                    .text_color(theme.accent)
                    .into_any_element()
            })
            .into_any_element();
    }
    let hash = project
        .path
        .to_string_lossy()
        .bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let name = project.name.to_lowercase();
    let glyph = if name.contains("unity") {
        "cube"
    } else if name.contains("kodem") {
        "code"
    } else if name.contains("liquid") {
        "branch"
    } else if name.contains("quill") {
        "book"
    } else {
        [
            "code",
            "cube",
            "book",
            "branch",
            "square-terminal",
            "folder-open",
            "settings-2",
            "file-text",
        ][hash as usize % 8]
    };
    let glyph = project.icon.glyph().unwrap_or(glyph);
    let tint = project_tint(project, theme);
    div()
        .size(px(18.))
        .rounded(px(4.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(tint.opacity(0.08))
        .child(icon(glyph).size(px(15.)).text_color(tint))
        .into_any_element()
}

// Use readable theme variants for named colors and preserve legacy custom RGB colors.
fn project_tint(project: &Project, theme: Theme) -> Hsla {
    project.color.map_or(theme.accent, |color| {
        let color = AccentColor::ALL
            .into_iter()
            .find(|accent| accent.rgb(false) == color)
            .map_or(color, |accent| accent.rgb(theme.canvas.lightness > 0.5));
        rgb_to_hsla(rgb(color))
    })
}

fn elapsed(time: u64) -> String {
    let s = now().saturating_sub(time) / 1000;
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86400 {
        format!("{}h", s / 3600)
    } else {
        format!("{}d", s / 86400)
    }
}
fn ago(time: u64) -> String {
    elapsed(time)
}
