use super::*;
impl Shell {
    /// Builds contextual task, sort, and model menus without replacing the workspace.
    pub(super) fn menu(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut menu = div()
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
                                if s.comparison != comparison {
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
            Panel::Models => {
                let right =
                    if self.selected.is_some() && self.show_review && self.window_width > 1100. {
                        self.settings.review_width + 42.
                    } else {
                        36.
                    };
                menu = menu.bottom(px(106.)).right(px(right)).child(
                    div()
                        .px_2()
                        .py_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(11.))
                        .text_color(t.muted)
                        .child(icon("bot").size(px(14.)))
                        .child("MODEL & REASONING")
                        .child(div().flex_1())
                        .child(
                            self.icon_button(
                                "refresh-models",
                                "refresh",
                                "Refresh available models",
                            )
                            .on_click(
                                cx.listener(|s, _, _, _| s.send(core::Request::ConnectRuntime)),
                            ),
                        ),
                );
                let runtime = self
                    .workspace
                    .as_ref()
                    .map(|w| w.runtime.clone())
                    .unwrap_or_default();
                if let Some(view) = self.view.clone() {
                    let id = view.task.id.clone();
                    menu = menu.child(
                        self.ghost("default-model", "bot", "Codex default")
                            .w_full()
                            .justify_start()
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.send(core::Request::EditTask {
                                    task_id: id.clone(),
                                    edit: TaskEdit::Model {
                                        model: None,
                                        effort: None,
                                    },
                                });
                                s.panel = Panel::None;
                                cx.notify();
                            })),
                    );
                    let mut models = div().id("model-list").max_h(px(300.)).overflow_y_scroll();
                    for (i, model) in runtime.models.iter().enumerate() {
                        let chosen = view.task.model.as_deref() == Some(&model.id);
                        let id = view.task.id.clone();
                        let model_id = model.id.clone();
                        let effort = model.default_effort.clone();
                        models = models.child(
                            self.ghost(
                                ("model-option", i),
                                if chosen { "check" } else { "bot" },
                                model.name.clone(),
                            )
                            .w_full()
                            .justify_start()
                            .when(chosen, |b| b.bg(t.selected).text_color(t.text))
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    s.send(core::Request::EditTask {
                                        task_id: id.clone(),
                                        edit: TaskEdit::Model {
                                            model: Some(model_id.clone()),
                                            effort: Some(effort.clone()),
                                        },
                                    });
                                    cx.notify();
                                },
                            )),
                        );
                        if chosen {
                            for (j, effort) in model.efforts.iter().enumerate() {
                                let id = view.task.id.clone();
                                let model_id = model.id.clone();
                                let effort = effort.clone();
                                let chosen = view.task.effort.as_ref() == Some(&effort);
                                models = models.child(
                                    self.ghost(
                                        ("reasoning-option", i * 20 + j),
                                        if chosen { "check" } else { "chevron-right" },
                                        effort.clone(),
                                    )
                                    .ml_4()
                                    .w_full()
                                    .justify_start()
                                    .text_size(px(11.))
                                    .when(chosen, |b| b.text_color(t.accent))
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| {
                                            s.send(core::Request::EditTask {
                                                task_id: id.clone(),
                                                edit: TaskEdit::Model {
                                                    model: Some(model_id.clone()),
                                                    effort: Some(effort.clone()),
                                                },
                                            });
                                            s.panel = Panel::None;
                                            cx.notify();
                                        },
                                    )),
                                );
                            }
                        }
                    }
                    menu = menu.child(models);
                } else {
                    menu = menu.child(
                        div()
                            .p_2()
                            .text_size(px(12.))
                            .text_color(t.muted)
                            .child("Create a task to choose its model and reasoning effort."),
                    );
                }
                if runtime.models.is_empty() {
                    menu =
                        menu.child(div().p_2().text_size(px(11.)).text_color(t.subtle).child(
                            "No model catalog loaded. Refresh to query your Codex runtime.",
                        ));
                }
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
