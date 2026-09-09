use super::*;
impl Shell {
    /// Builds the project tree, stable task rows, and workspace navigation.
    pub(super) fn sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        if self.settings.collapsed {
            return div()
                .w(px(54.))
                .flex_shrink_0()
                .h_full()
                .bg(t.sidebar)
                .flex()
                .flex_col()
                .items_center()
                .py_3()
                .gap_2()
                .child(
                    self.icon_button("rail-new", "edit", "New task · ⌘N")
                        .on_click(cx.listener(|s, _, w, cx| s.new_task(&NewTask, w, cx))),
                )
                .child(
                    self.icon_button("rail-open", "folder-open", "Open repository · ⌘O")
                        .on_click(cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx))),
                )
                .child(div().flex_1())
                .child(
                    self.icon_button("rail-settings", "settings", "Settings · ⌘,")
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.panel = Panel::Settings;
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        let mut panel = div()
            .id("sidebar")
            .w(px(self.settings.sidebar_width))
            .flex_shrink_0()
            .h_full()
            .bg(t.sidebar)
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_size(px(12.))
            .on_hover(cx.listener(|s, hovered, _, cx| {
                s.sidebar_hovered = *hovered;
                cx.notify();
            }))
            .child(
                div().px_2().pt_1().pb_0().child(
                    self.ghost("new-task", "edit", "New chat")
                        .w_full()
                        .h(px(28.))
                        .justify_start()
                        .text_color(t.text)
                        .child(div().flex_1())
                        .child(div().text_size(px(11.)).text_color(t.subtle).child("⌘N"))
                        .on_click(cx.listener(|s, _, w, cx| s.new_task(&NewTask, w, cx))),
                ),
            )
            .child(
                div().px_2().pb_2().child(
                    self.ghost("new-project", "folder-open", "New project")
                        .w_full()
                        .h(px(28.))
                        .justify_start()
                        .on_click(cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx))),
                ),
            )
            .when(self.search_visible, |d| {
                d.child(
                    div()
                        .mx_3()
                        .mb_2()
                        .h(px(28.))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_1()
                        .rounded(px(5.))
                        .bg(t.inset)
                        .border_1()
                        .border_color(t.border)
                        .child(icon("search").size(px(13.)).text_color(t.muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(12.))
                                .child(self.search.clone()),
                        )
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
                    .px_4()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(t.subtle)
                            .child(if self.show_archived {
                                "Archived"
                            } else {
                                "Projects"
                            }),
                    )
                    .child(
                        self.icon_button("search-tasks", "search", "Search tasks · ⌘F")
                            .on_click(cx.listener(|s, _, w, cx| {
                                s.search_visible = true;
                                w.focus(&s.search.read(cx).focus_handle(cx), cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        self.icon_button("sort-tasks", "settings-2", "Sort tasks")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.panel = Panel::Sort;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.icon_button("add-project", "plus", "Open repository")
                            .on_click(
                                cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx)),
                            ),
                    ),
            );
        let query = self.search.read(cx).text().to_lowercase();
        let mut list = div()
            .id("projects")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_2();
        if let Some(workspace) = self.workspace.clone() {
            for project in &workspace.projects {
                let id = project.id.clone();
                let add_id = id.clone();
                let collapsed = self.collapsed_projects.contains(&id) && query.is_empty();
                let count = workspace
                    .tasks
                    .iter()
                    .filter(|v| v.project_id == id && v.archived == self.show_archived)
                    .count();
                list = list.child(
                    div()
                        .id((ElementId::from("project"), SharedString::from(id.clone())))
                        .h(px(28.))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(move |d| d.bg(t.raised))
                        .text_color(t.text)
                        .child(
                            icon(if collapsed {
                                "chevron-right"
                            } else {
                                "chevron-down"
                            })
                            .size(px(12.))
                            .text_color(t.subtle),
                        )
                        .child(icon("folder-closed").size(px(15.)).text_color(t.muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .font_weight(FontWeight::NORMAL)
                                .child(project.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(t.subtle)
                                .child(count.to_string()),
                        )
                        .child(
                            self.icon_button(
                                (
                                    ElementId::from("project-add"),
                                    SharedString::from(id.clone()),
                                ),
                                "plus",
                                "New task in this project",
                            )
                            .w(px(22.))
                            .h(px(22.))
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    s.selected_project = Some(add_id.clone());
                                    s.panel = Panel::NewTask;
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                            )),
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if !s.collapsed_projects.remove(&id) {
                                s.collapsed_projects.insert(id.clone());
                            }
                            cx.notify();
                        })),
                );
                if collapsed {
                    continue;
                }
                let mut tasks: Vec<_> = workspace
                    .tasks
                    .iter()
                    .filter(|task| {
                        task.project_id == project.id
                            && task.archived == self.show_archived
                            && (query.is_empty()
                                || task.title.to_lowercase().contains(&query)
                                || task.branch.to_lowercase().contains(&query))
                    })
                    .collect();
                tasks.sort_by(|a, b| {
                    if self.sidebar_hovered {
                        return self
                            .task_order
                            .get(&a.id)
                            .copied()
                            .unwrap_or(usize::MAX)
                            .cmp(&self.task_order.get(&b.id).copied().unwrap_or(usize::MAX))
                            .then_with(|| a.id.cmp(&b.id));
                    }
                    b.pinned
                        .cmp(&a.pinned)
                        .then_with(|| match self.settings.sort {
                            Sort::Recent => b.updated.cmp(&a.updated),
                            Sort::Created => b.created.cmp(&a.created),
                            Sort::Name => a.title.cmp(&b.title),
                            Sort::Attention => rank(&a.state).cmp(&rank(&b.state)),
                        })
                        .then_with(|| a.id.cmp(&b.id))
                });
                if !self.sidebar_hovered {
                    for (index, task) in tasks.iter().enumerate() {
                        self.task_order.insert(task.id.clone(), index);
                    }
                }
                for task in tasks {
                    let id = task.id.clone();
                    let selected = self.selected.as_deref() == Some(&id);
                    let color = match task.state {
                        RunState::Running | RunState::Starting | RunState::Waiting => t.accent,
                        RunState::Failed | RunState::DeliveryUnknown => t.removed,
                        RunState::Completed => t.added,
                        _ => t.subtle,
                    };
                    let title = task.title.clone();
                    let hint = format!("{}\n{}", title, task.cwd.display());
                    let row = div()
                        .id((ElementId::from("task"), SharedString::from(id.clone())))
                        .ml_3()
                        .mb(px(2.))
                        .pl_3()
                        .pr_2()
                        .py(px(if self.settings.comfortable { 6. } else { 4. }))
                        .rounded(px(4.))
                        .border_l_0()
                        .border_color(if selected {
                            t.accent
                        } else {
                            hsla(0., 0., 0., 0.)
                        })
                        .cursor_pointer()
                        .when(selected, |d| d.bg(t.selected))
                        .hover(move |d| d.bg(t.raised))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    icon(if task.pinned { "pin" } else { "branch" })
                                        .size(px(13.))
                                        .text_color(if selected { t.text } else { t.subtle }),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(12.))
                                        .text_ellipsis()
                                        .child(if task.state.active() {
                                            motion::gleam(
                                                (
                                                    ElementId::from("task-progress"),
                                                    SharedString::from(task.id.clone()),
                                                ),
                                                title,
                                                t,
                                            )
                                        } else {
                                            div().text_ellipsis().child(title).into_any_element()
                                        }),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(if task.state.active() {
                                            color
                                        } else {
                                            t.subtle
                                        })
                                        .child(ago(task.updated)),
                                ),
                        )
                        .on_click(cx.listener(move |s, _, _, cx| s.select_task(id.clone(), cx)))
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: hint.clone(),
                                theme: t,
                            })
                            .into()
                        });
                    list = list.child(row);
                }
                list = list.child(div().h(px(6.)));
            }
            if workspace.projects.is_empty() {
                list = list.child(
                    div()
                        .px_4()
                        .py_3()
                        .text_size(px(12.))
                        .text_color(t.muted)
                        .child("Open a repository to keep its conversations and changes together."),
                );
            }
        }
        panel =
            panel.child(list).child(
                div()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(t.border)
                    .child(
                        self.ghost(
                            "archive-view",
                            "archive",
                            if self.show_archived {
                                "Active tasks"
                            } else {
                                "Archived tasks"
                            },
                        )
                        .h(px(26.))
                        .w_full()
                        .justify_start()
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.show_archived = !s.show_archived;
                            cx.notify();
                        })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .h(px(30.))
                            .child(self.ghost("settings", "settings", "Settings").on_click(
                                cx.listener(|s, _, _, cx| {
                                    s.panel = Panel::Settings;
                                    cx.notify();
                                }),
                            ))
                            .child(div().flex_1())
                            .child(div().size(px(5.)).rounded_full().bg(
                                if self.workspace.is_some() {
                                    t.added
                                } else {
                                    t.subtle
                                },
                            ))
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(t.subtle)
                                    .pr_2()
                                    .child("Local"),
                            ),
                    ),
            );
        panel
            .with_animation(
                "sidebar-enter",
                Animation::new(Duration::from_millis(150)),
                |d, p| d.opacity(p),
            )
            .into_any_element()
    }
}
fn rank(state: &RunState) -> u8 {
    match state {
        RunState::Waiting => 0,
        RunState::Failed | RunState::DeliveryUnknown => 1,
        RunState::Running | RunState::Starting => 2,
        RunState::Completed => 3,
        _ => 4,
    }
}
fn ago(updated: u64) -> String {
    let minutes = now().saturating_sub(updated) / 60_000;
    if minutes < 1 {
        "now".into()
    } else if minutes < 60 {
        format!("{minutes}m")
    } else if minutes < 1440 {
        format!("{}h", minutes / 60)
    } else {
        format!("{}d", minutes / 1440)
    }
}
