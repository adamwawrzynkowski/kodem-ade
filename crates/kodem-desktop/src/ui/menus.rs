use super::*;
impl Shell {
    /// Builds contextual task, sort, and model menus without replacing the workspace.
    pub(super) fn menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut width = 284.;
        let mut menu = div().flex().flex_col().flex_shrink_0().gap_1();
        match self.panel {
            Panel::Usage => {
                width = 320.;
                menu = menu.child(self.usage_panel(cx));
            }
            Panel::Projects | Panel::ChatProjects => {
                let chat_picker = self.panel == Panel::ChatProjects;
                width = if chat_picker {
                    320.
                } else {
                    (self.settings.sidebar_width - 24.).max(220.)
                };
                if !chat_picker {
                    menu = menu.child(
                        self.ghost("all-projects", "folder-open", "All projects")
                            .w_full()
                            .justify_start()
                            .when(self.sidebar_project.is_none(), |b| {
                                b.bg(t.selected).text_color(t.text)
                            })
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.sidebar_project = None;
                                s.sidebar_dirty = true;
                                s.panel = Panel::None;
                                cx.notify();
                            })),
                    );
                }
                if let Some(workspace) = self.workspace.clone() {
                    let mut projects = div().id("project-choices");
                    for project in workspace.projects {
                        let selected = if chat_picker {
                            self.selected_project.as_ref() == Some(&project.id)
                        } else {
                            self.sidebar_project.as_ref() == Some(&project.id)
                        };
                        let glyph = sidebar::project_icon(&project, t);
                        let id = project.id.clone();
                        projects = projects.child(
                            self.control((
                                ElementId::from("project-choice"),
                                SharedString::from(id.clone()),
                            ))
                            .border_0()
                            .shadow(Vec::new())
                            .bg(hsla(0., 0., 0., 0.))
                            .text_color(t.muted)
                            .hover(move |b| b.bg(t.selected))
                            .w_full()
                            .h(px(32.))
                            .justify_start()
                            .when(selected, |b| b.bg(t.selected).text_color(t.text))
                            .child(glyph)
                            .child(div().flex_1().min_w_0().text_ellipsis().child(project.name))
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    if !chat_picker {
                                        s.sidebar_project = Some(id.clone());
                                        s.sidebar_dirty = true;
                                    }
                                    s.selected_project = Some(id.clone());
                                    s.panel = Panel::None;
                                    cx.notify();
                                },
                            )),
                        );
                    }
                    menu = menu.child(projects);
                }
                menu = menu
                    .child(div().h(px(1.)).my_1().bg(t.border))
                    .child(
                        self.ghost("picker-add-project", "plus", "Add project…")
                            .justify_start()
                            .on_click(
                                cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx)),
                            ),
                    )
                    .child(
                        self.ghost("picker-manage-projects", "settings", "Customize projects…")
                            .justify_start()
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.settings_section = 5;
                                s.panel = Panel::Settings;
                                cx.notify();
                            })),
                    );
            }

            Panel::Comparison => {
                width = 212.;
                for (i, (comparison, label, glyph)) in review::COMPARISONS.into_iter().enumerate() {
                    if comparison == Comparison::Branch
                        && !self
                            .new_diff
                            .as_ref()
                            .or(self.diff.as_ref())
                            .and_then(|diff| diff.repositories.first())
                            .is_some_and(|repo| repo.branch_base.is_some())
                    {
                        continue;
                    }
                    let selected = self.comparison == comparison;
                    menu = menu.child(
                        self.ghost(("comparison-option", i), glyph, label)
                            .w_full()
                            .h(px(28.))
                            .justify_start()
                            .when(selected, |b| b.bg(t.selected).text_color(t.text))
                            .child(div().flex_1())
                            .when(selected, |b| b.child(icon("check").size(px(12.))))
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.panel = Panel::None;
                                if s.comparison != comparison || s.historical_diff {
                                    s.comparison = comparison.clone();
                                    s.diff_view = None;
                                    s.diff = None;
                                    s.new_diff = None;
                                    s.reviewing_file = false;
                                    s.refresh_diff();
                                }
                                cx.notify();
                            })),
                    );
                }
            }
            Panel::Sort => {
                width = 210.;
                menu = menu.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_size(px(11.))
                        .text_color(t.subtle)
                        .child("THREAD LIST"),
                );
                for (i, (sort, label)) in [
                    (Sort::Recent, "Recent activity"),
                    (Sort::Attention, "Needs attention"),
                    (Sort::Name, "Name"),
                    (Sort::Created, "Date created"),
                ]
                .into_iter()
                .enumerate()
                {
                    let chosen = self.settings.sort == sort;
                    menu = menu.child(
                        self.ghost(
                            ("sort-option", i),
                            if chosen { "check" } else { "chat" },
                            label,
                        )
                        .w_full()
                        .justify_start()
                        .when(chosen, |b| b.bg(t.selected).text_color(t.text))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.settings.sort = sort;
                            s.sidebar_dirty = true;
                            s.panel = Panel::None;
                            s.save_settings();
                            cx.notify();
                        })),
                    );
                }
                menu = menu.child(div().h(px(1.)).my_1().bg(t.border)).child(
                    self.ghost(
                        "group-projects",
                        if self.settings.group_tasks_by_project {
                            "check"
                        } else {
                            "folder-closed"
                        },
                        "Group by project",
                    )
                    .w_full()
                    .justify_start()
                    .accessibility_label(if self.settings.group_tasks_by_project {
                        "Disable project grouping"
                    } else {
                        "Enable project grouping"
                    })
                    .when(self.settings.group_tasks_by_project, |b| {
                        b.bg(t.selected).text_color(t.text)
                    })
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.settings.group_tasks_by_project = !s.settings.group_tasks_by_project;
                        s.sidebar_dirty = true;
                        s.save_settings();
                        cx.notify();
                    })),
                );
            }
            Panel::ThreadMenu => {
                if let Some(context) = &self.thread_menu {
                    width = 236.;
                    if let Some(task) = self
                        .workspace
                        .as_ref()
                        .and_then(|workspace| {
                            workspace
                                .tasks
                                .iter()
                                .find(|task| task.id == context.task_id)
                        })
                        .cloned()
                    {
                        let open_id = task.id.clone();
                        menu = menu.child(
                            self.ghost("thread-open", "chat", "Open")
                                .w_full()
                                .justify_start()
                                .on_click(cx.listener(move |s, _, _, cx| {
                                    if s.selected.as_ref() != Some(&open_id) {
                                        s.select_task(open_id.clone(), cx);
                                    }
                                    s.panel = Panel::None;
                                    cx.notify();
                                })),
                        );
                        let settled = task.is_settled();
                        let settle_id = task.id.clone();
                        let busy = task.state.active() || task.state == RunState::DeliveryUnknown;
                        menu = menu.child(
                            self.ghost(
                                "thread-settle",
                                if settled { "arrow-up" } else { "check" },
                                if settled { "Unsettle" } else { "Settle" },
                            )
                            .w_full()
                            .justify_start()
                            .disabled(busy)
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    s.send(core::Request::EditTask {
                                        task_id: settle_id.clone(),
                                        edit: TaskEdit::Settle(!settled),
                                    });
                                    s.panel = Panel::None;
                                    cx.notify();
                                },
                            )),
                        );
                        let archive_id = task.id.clone();
                        let archived = task.archived;
                        menu = menu.child(
                            self.ghost(
                                "thread-archive",
                                "archive",
                                if archived { "Unarchive" } else { "Archive" },
                            )
                            .w_full()
                            .justify_start()
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    s.send(core::Request::EditTask {
                                        task_id: archive_id.clone(),
                                        edit: TaskEdit::Archive(!archived),
                                    });
                                    s.panel = Panel::None;
                                    cx.notify();
                                },
                            )),
                        );
                        menu = menu.child(div().h(px(1.)).my_1().bg(t.border)).child(
                            self.ghost("thread-delete", "close", "Delete…")
                                .w_full()
                                .justify_start()
                                .text_color(t.removed)
                                .disabled(busy)
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.error = None;
                                    s.panel = Panel::DeleteTask;
                                    cx.notify();
                                })),
                        );
                    }
                }
            }
            Panel::TaskMenu => {
                width = 236.;
                if let Some(view) = self.view.clone() {
                    let id = view.task.id.clone();
                    let title = view.task.title.clone();
                    menu = menu.child(
                        self.ghost("menu-rename", "edit", "Rename task")
                            .w_full()
                            .justify_start()
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.aux.update(cx, |i, cx| i.set_text(title.clone(), cx));
                                s.panel = Panel::Rename;
                                cx.notify();
                            })),
                    );
                    let pinned = view.task.pinned;
                    let pin_id = id.clone();
                    menu = menu.child(
                        self.ghost(
                            "menu-pin",
                            "pin",
                            if pinned { "Unpin task" } else { "Pin task" },
                        )
                        .w_full()
                        .justify_start()
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.send(core::Request::EditTask {
                                task_id: pin_id.clone(),
                                edit: TaskEdit::Pin(!pinned),
                            });
                            s.panel = Panel::None;
                            cx.notify();
                        })),
                    );
                    menu = menu
                        .child(div().h(px(1.)).my_1().bg(t.border))
                        .child(
                            self.ghost("menu-check", "check-square", "Run a check")
                                .w_full()
                                .justify_start()
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.aux.update(cx, |i, cx| i.set_text(String::new(), cx));
                                    s.panel = Panel::Check;
                                    cx.notify();
                                })),
                        )
                        .child(
                            self.ghost("menu-resources", "code", "Skills & MCP")
                                .w_full()
                                .justify_start()
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.panel = Panel::Resources;
                                    s.send(core::Request::Resources {
                                        provider: s.composer_preferences().provider,
                                        project_id: s.selected_project.clone().or_else(|| {
                                            s.workspace
                                                .as_ref()?
                                                .projects
                                                .first()
                                                .map(|p| p.id.clone())
                                        }),
                                        task_id: s.selected.clone(),
                                    });
                                    cx.notify();
                                })),
                        );
                    let archived = view.task.archived;
                    menu = menu.child(div().h(px(1.)).my_1().bg(t.border)).child(
                        self.ghost(
                            "menu-archive",
                            "archive",
                            if archived {
                                "Restore task"
                            } else {
                                "Archive task"
                            },
                        )
                        .w_full()
                        .justify_start()
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.send(core::Request::EditTask {
                                task_id: id.clone(),
                                edit: TaskEdit::Archive(!archived),
                            });
                            s.panel = Panel::None;
                            cx.notify();
                        })),
                    );
                }
            }
            Panel::Models | Panel::Reasoning | Panel::Access => {
                width = if self.panel == Panel::Models {
                    460.
                } else {
                    348.
                };
                menu = menu.child(self.model_options(cx));
            }
            _ => {}
        }
        let viewport = window.viewport_size();
        // Measure content at its natural height. Only the window limits scrolling;
        // percentage caps inside the anchored layout can clip even short menus.
        let menu = div()
            .id("context-menu")
            .w(px(width).min((viewport.width - px(16.)).max(px(0.))))
            .max_h((viewport.height - px(16.)).max(px(0.)))
            .overflow_y_scroll()
            .p_2()
            .rounded(px(10.))
            .bg(t.raised)
            .border_1()
            .border_color(t.border)
            .shadow(t.elevation())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(menu)
            .with_animation(
                ("menu-enter", self.panel.clone() as usize),
                Animation::new(Duration::from_millis(130)),
                |d, p| d.opacity(p),
            );
        let thread_context = (self.panel == Panel::ThreadMenu)
            .then_some(self.thread_menu.as_ref())
            .flatten();
        let position = thread_context
            .map(|context| context.position)
            .or(self.menu_anchor)
            .unwrap_or(point(px(24.), px(120.)));
        div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.panel = Panel::None;
                    cx.notify();
                }),
            )
            .child(
                anchored()
                    .position(position)
                    .anchor(if thread_context.is_some() {
                        Anchor::TopLeft
                    } else {
                        Anchor::BottomRight
                    })
                    .offset(point(
                        px(0.),
                        px(if thread_context.is_some() { 0. } else { -8. }),
                    ))
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .into_any_element()
    }
}
