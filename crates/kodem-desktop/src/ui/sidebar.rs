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
                .py(px(10.))
                .gap(px(4.))
                .child(
                    self.icon_button("rail-new", "edit", "New task · ⌘N")
                        .on_click(cx.listener(|s, _, w, cx| s.new_task(&NewTask, w, cx))),
                )
                .child(
                    self.icon_button("rail-search", "search", "Search tasks · ⌘F")
                        .on_click(cx.listener(|s, _, w, cx| s.open_search(w, cx))),
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
        let search = if self.search_visible {
            div()
                .flex_1()
                .min_w_0()
                .h(px(28.))
                .pl(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(crate::theme::RADIUS_CONTROL))
                .bg(t.inset)
                .border_1()
                .border_color(t.edge)
                .child(icon("search").size(px(13.)).text_color(t.muted))
                .child(div().flex_1().min_w_0().child(self.search.clone()))
                .child(
                    self.icon_button("close-search", "close", "Close search · Esc")
                        .h(px(24.))
                        .w(px(24.))
                        .on_click(cx.listener(|s, _, w, cx| {
                            s.search_visible = false;
                            s.search.update(cx, |i, cx| i.set_text(String::new(), cx));
                            s.sidebar_dirty = true;
                            w.focus(&s.focus, cx);
                            cx.notify();
                        })),
                )
                .into_any_element()
        } else {
            self.control("search-tasks")
                .flex_1()
                .min_w_0()
                .h(px(28.))
                .justify_start()
                .px(px(8.))
                .bg(t.inset)
                .shadow(Vec::new())
                .text_color(t.subtle)
                .hover(move |b| b.border_color(t.edge).text_color(t.muted))
                .child(icon("search").size(px(13.)))
                .child(div().flex_1().child("Search"))
                .child(caps("⌘F", t))
                .on_click(cx.listener(|s, _, w, cx| s.open_search(w, cx)))
                .into_any_element()
        };
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
            .child(
                div()
                    .px(px(10.))
                    .pt(px(10.))
                    .pb(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(search)
                    .child(
                        self.icon_button("new-task", "edit", "New task · ⌘N")
                            .h(px(28.))
                            .w(px(28.))
                            .on_click(cx.listener(|s, _, w, cx| s.new_task(&NewTask, w, cx))),
                    ),
            )
            .child(
                div()
                    .px(px(10.))
                    .pb(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        self.ghost("project-filter", "folder-closed", "")
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .px(px(6.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(project_label),
                            )
                            .child(icon("chevrons-up-down").size(px(12.)))
                            .on_click_with_bounds(cx.listener(
                                |s, bounds: &Bounds<Pixels>, _, cx| {
                                    s.menu_anchor = Some(bounds.top_right());
                                    s.panel = Panel::Projects;
                                    cx.notify();
                                },
                            )),
                    )
                    .child(
                        self.icon_button("sort-tasks", "settings-2", "Thread list settings")
                            .on_click_with_bounds(cx.listener(
                                |s, bounds: &Bounds<Pixels>, _, cx| {
                                    s.menu_anchor = Some(bounds.top_right());
                                    s.panel = Panel::Sort;
                                    cx.notify();
                                },
                            )),
                    )
                    .child(
                        self.icon_button("add-project", "plus", "Open repository · ⌘O")
                            .on_click(
                                cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx)),
                            ),
                    ),
            )
            .child(div().mx(px(10.)).h(px(1.)).bg(t.border));
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
                        expanded_settled_projects: &self.expanded_settled_projects,
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
            // List positions rows at its left edge; horizontal gutters belong inside each row.
            div()
                .w_full()
                .px(px(6.))
                .child(entity.update(cx, |s, cx| s.sidebar_row(index, cx)))
                .into_any_element()
        })
        .flex_1()
        .min_h_0()
        .w_full()
        .pt(px(6.))
        .pb_2();
        let connected = self.host_connected && self.workspace.is_some();
        panel = panel.child(feed).child(
            div()
                .h(px(40.))
                .flex_shrink_0()
                .px(px(10.))
                .border_t_1()
                .border_color(t.border)
                .flex()
                .items_center()
                .gap(px(2.))
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
                    .px(px(6.))
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.panel = Panel::Settings;
                        cx.notify();
                    })),
                )
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
                .child(div().flex_1())
                .child(self.update_control(false, cx))
                .child(
                    div()
                        .id("host-status")
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .pl(px(6.))
                        .child(
                            div()
                                .size(px(5.))
                                .bg(if connected { t.added } else { t.subtle }),
                        )
                        .child(caps(if connected { "Host" } else { "Offline" }, t))
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: if connected {
                                    "Local host connected".into()
                                } else {
                                    "Local host unavailable; reconnecting".into()
                                },
                                theme: t,
                            })
                            .into()
                        }),
                ),
        );
        motion::reveal("sidebar-enter", panel)
    }
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.collapsed = false;
        self.search_visible = true;
        window.focus(&self.search.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    fn sidebar_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        use crate::thread_list::Row;
        let Some(row) = self.sidebar_rows.get(index) else {
            return div().into_any_element();
        };
        let t = self.theme;
        match row {
            Row::Task {
                index,
                project,
                settled,
                ..
            } => {
                let Some(workspace) = &self.workspace else {
                    return div().into_any_element();
                };
                let card =
                    self.task_card(&workspace.tasks[*index], &workspace.projects[*project], cx);
                // List measures the root's height without its margins; keep the gap inside the row.
                div()
                    .pb(px(2.))
                    .when(*settled, |row| {
                        row.pl(px(if self.settings.group_tasks_by_project {
                            22.
                        } else {
                            12.
                        }))
                    })
                    .child(card)
                    .into_any_element()
            }
            Row::Project { index, id, count } => {
                let Some(project) = self.workspace.as_ref().and_then(|w| w.projects.get(*index))
                else {
                    return div().into_any_element();
                };
                let collapsed = self.settings.collapsed_projects.contains(id)
                    && self.search.read(cx).text().is_empty();
                let id = id.clone();
                div()
                    .pt(px(4.))
                    .child(
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
                        .h(px(28.))
                        .px(px(6.))
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
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.text)
                                .child(project.name.clone()),
                        )
                        .child(figure(count.to_string(), t.subtle))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if !s.settings.collapsed_projects.remove(&id) {
                                s.settings.collapsed_projects.insert(id.clone());
                            }
                            s.sidebar_dirty = true;
                            s.save_settings();
                            cx.notify();
                        })),
                    )
                    .into_any_element()
            }
            Row::Settled { project, count } => {
                let collapsed = project.as_ref().map_or(self.settled_collapsed, |id| {
                    !self.expanded_settled_projects.contains(id)
                });
                let project = project.clone();
                self.control(SharedString::from(format!(
                    "settled:{}",
                    project.as_deref().unwrap_or("all")
                )))
                .border_0()
                .shadow(Vec::new())
                .bg(hsla(0., 0., 0., 0.))
                .hover(move |b| b.bg(t.selected))
                .w_full()
                .h(px(26.))
                .px(px(6.))
                .mt(px(4.))
                .justify_start()
                .text_color(t.subtle)
                .when(project.is_some(), |row| row.pl(px(18.)))
                .child(
                    icon(if collapsed && self.search.read(cx).text().is_empty() {
                        "chevron-right"
                    } else {
                        "chevron-down"
                    })
                    .size(px(12.)),
                )
                .child(caps("Settled", t))
                .child(div().flex_1().h(px(1.)).mx(px(4.)).bg(t.border))
                .child(figure(count.to_string(), t.subtle))
                .on_click(cx.listener(move |s, _, _, cx| {
                    if let Some(id) = &project {
                        if !s.expanded_settled_projects.remove(id) {
                            s.expanded_settled_projects.insert(id.clone());
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
                .px(px(10.))
                .py(px(18.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(caps("Nothing here", t))
                .child(div().text_size(px(12.)).text_color(t.muted).child(*label))
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
        let radius = crate::theme::RADIUS_CONTROL;
        let id = task.id.clone();
        let menu_id = id.clone();
        let needs_input = task.needs_input();
        let active = task.state.active() && !needs_input;
        let failed = matches!(task.state, RunState::Failed | RunState::DeliveryUnknown);
        let leading = if needs_input {
            motion::active_icon(
                (
                    ElementId::from("sidebar-status"),
                    SharedString::from(id.clone()),
                ),
                "shield",
                t.accent,
                false,
                cx.reduce_motion(),
            )
        } else if active {
            dither::pulse(
                (
                    ElementId::from("sidebar-status"),
                    SharedString::from(id.clone()),
                ),
                t.accent,
                cx.reduce_motion(),
            )
        } else if failed {
            icon("circle-x")
                .size(px(13.))
                .text_color(t.removed)
                .into_any_element()
        } else {
            project_icon(project, t)
        };
        let status = if needs_input {
            motion::gleam(
                (
                    ElementId::from("sidebar-status-label"),
                    SharedString::from(id.clone()),
                ),
                "Input",
                t,
            )
        } else if active {
            figure(elapsed(task.updated), t.accent).into_any_element()
        } else if failed {
            caps(task.state.label(), t)
                .text_color(t.removed)
                .into_any_element()
        } else {
            figure(elapsed(task.updated), t.subtle).into_any_element()
        };
        let review_hint = task.review.as_ref().map_or_else(|| "No saved task diff. A snapshot will be recorded when the next turn finishes.".to_owned(), |r| {
            if let Some(error) = &r.error { format!("Could not capture task diff: {error}") }
            else { format!("Saved task diff · {} ago\n+{} added · −{} removed\n{} files omitted from inline counts. Later checkout changes do not affect these totals.", elapsed(r.captured),r.added,r.removed,r.omitted_files) }
        });
        let mut meta = div()
            .h(px(16.))
            .pl(px(21.))
            .flex()
            .items_center()
            .gap(px(6.))
            .font_family(crate::theme::MONO)
            .text_size(px(10.5))
            .text_color(if selected { t.muted } else { t.subtle })
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(relative(0.45))
                    .text_ellipsis()
                    .child(project.name.clone()),
            )
            .child(div().flex_shrink_0().child("/"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(task.branch.clone()),
            );
        if let Some(review) = task.review.as_ref().filter(|r| r.error.is_none()) {
            let snapshot = review.snapshot_id.clone();
            let task_id = id.clone();
            meta = meta.child(
                self.control((ElementId::from("task-diff"), SharedString::from(id.clone())))
                    .border_0()
                    .shadow(Vec::new())
                    .bg(hsla(0., 0., 0., 0.))
                    .hover(move |b| b.bg(t.raised))
                    .h(px(16.))
                    .px(px(3.))
                    .rounded(px(crate::theme::RADIUS_SMALL))
                    .disabled(snapshot.is_none())
                    .accessibility_label("Open saved task diff")
                    .when(review.omitted_files > 0, |b| b.child(figure("~", t.subtle)))
                    .child(line_totals(review.added, review.removed, t))
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
                    .px(px(3.))
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
        let provider = task.provider;
        meta = meta.child(
            div()
                .id((
                    ElementId::from("task-provider"),
                    SharedString::from(id.clone()),
                ))
                .child(icon(provider.icon()).size(px(12.)))
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: provider.label().into(),
                        theme: t,
                    })
                    .into()
                }),
        );
        let mut hover = t.selected;
        hover.alpha *= 0.6;
        div()
            .id((ElementId::from("task"), SharedString::from(id.clone())))
            .relative()
            .overflow_hidden()
            .w_full()
            .px(px(8.))
            .py(px(if self.settings.comfortable { 9. } else { 6. }))
            .flex()
            .flex_col()
            .gap(px(2.))
            .rounded(px(radius))
            .border_1()
            .border_color(if selected {
                t.border
            } else {
                hsla(0., 0., 0., 0.)
            })
            .when(selected, |row| row.bg(t.selected))
            .cursor_pointer()
            .when(!selected, |row| row.hover(move |d| d.bg(hover)))
            .when(selected || active, |row| {
                row.child(dither::row_wash(
                    SharedString::from(format!("thread-wash:{id}")),
                    if active { t.accent } else { t.text },
                    radius,
                    selected,
                    active,
                    cx.reduce_motion(),
                ))
            })
            .child(
                div()
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .size(px(14.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(leading),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .font_weight(if selected {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if selected || active || needs_input {
                                t.text
                            } else {
                                t.text.opacity(0.88)
                            })
                            .text_ellipsis()
                            .child(task.title.clone()),
                    )
                    .when(task.pinned, |d| {
                        d.child(icon("pin").size(px(11.)).text_color(t.accent))
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(t.accent)
                            .child(status),
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
            .size(px(16.))
            .rounded(px(crate::theme::RADIUS_SMALL))
            .flex_shrink_0()
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                icon("file")
                    .size(px(16.))
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
        .size(px(16.))
        .rounded(px(crate::theme::RADIUS_SMALL))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(tint.opacity(0.12))
        .child(icon(glyph).size(px(12.)).text_color(tint))
        .into_any_element()
}

/// Returns readable theme variants for named colors and preserves legacy custom RGB colors.
pub(super) fn project_tint(project: &Project, theme: Theme) -> Hsla {
    project.color.map_or(theme.accent, |color| {
        AccentColor::ALL
            .into_iter()
            .find(|accent| accent.rgb(false) == color)
            .map_or_else(
                || rgb_to_hsla(rgb(color)),
                |accent| crate::theme::accent_color(accent, theme.light),
            )
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
