use super::*;
impl Shell {
    /// Builds contextual task, sort, and model menus without replacing the workspace.
    pub(super) fn menu(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut menu = div()
            .id("context-menu")
            .absolute()
            .w(px(284.))
            .p_2()
            .rounded(px(10.))
            .bg(t.raised)
            .border_1()
            .border_color(t.border)
            .shadow(t.elevation())
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        match self.panel {
            Panel::Projects => {
                menu = menu
                    .left(px(12.))
                    .top(px(if self.search_visible { 158. } else { 112. }))
                    .w(px((self.settings.sidebar_width - 24.).max(220.)));
                menu = menu.child(
                    self.ghost("all-projects", "folder-open", "All projects")
                        .w_full()
                        .justify_start()
                        .when(self.sidebar_project.is_none(), |b| {
                            b.bg(t.selected).text_color(t.text)
                        })
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.sidebar_project = None;
                            s.panel = Panel::None;
                            cx.notify();
                        })),
                );
                if let Some(workspace) = self.workspace.clone() {
                    let mut projects = div()
                        .id("project-choices")
                        .max_h(px(360.))
                        .overflow_y_scroll();
                    for project in workspace.projects {
                        let selected = self.sidebar_project.as_ref() == Some(&project.id);
                        let glyph = sidebar::project_icon(&project);
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
                                    s.sidebar_project = Some(id.clone());
                                    s.selected_project = Some(id.clone());
                                    s.panel = Panel::None;
                                    cx.notify();
                                },
                            )),
                        );
                    }
                    menu = menu.child(projects);
                }
            }

            Panel::Comparison => {
                let review_width = if self.window_width <= 1100. {
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
                menu = menu
                    .top(px(72.))
                    .right(px((review_width - 224.).max(8.)))
                    .w(px(212.));
                for (i, (comparison, label, glyph)) in review::COMPARISONS.into_iter().enumerate() {
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
                menu = menu
                    .top(px(177.))
                    .left(px((self.settings.sidebar_width - 210.).max(12.)))
                    .w(px(210.))
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .text_size(px(11.))
                            .text_color(t.subtle)
                            .child("SORT TASKS"),
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
                            s.panel = Panel::None;
                            s.save_settings();
                            cx.notify();
                        })),
                    );
                }
            }
            Panel::TaskMenu => {
                let right = if self.show_review && self.window_width > 1100. {
                    self.settings.review_width + 14.
                } else {
                    14.
                };
                menu = menu.top(px(87.)).right(px(right)).w(px(236.));
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
                let right =
                    if self.selected.is_some() && self.show_review && self.window_width > 1100. {
                        self.settings.review_width.min(self.window_width * 0.55) + 24.
                    } else {
                        24.
                    };
                menu = menu
                    .bottom(px(94.))
                    .right(px(right))
                    .w(px(348.))
                    .max_h(relative(0.8))
                    .overflow_y_scroll()
                    .child(self.model_options(cx));
            }
            _ => {}
        }
        div()
            .absolute()
            .inset_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.panel = Panel::None;
                    cx.notify();
                }),
            )
            .child(menu.with_animation(
                ("menu-enter", self.panel.clone() as usize),
                Animation::new(Duration::from_millis(130)),
                |d, p| d.opacity(p),
            ))
            .into_any_element()
    }
}
