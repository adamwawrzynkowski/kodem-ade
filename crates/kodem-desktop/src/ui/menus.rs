use super::*;

// The rule between two groups of menu items.
fn separator(theme: Theme) -> Div {
    div()
        .my(px(3.))
        .mx(px(4.))
        .child(dither::DottedRule::horizontal(theme.subtle.opacity(0.4)))
}

// The title of a menu: a mono label, an optional count and a dotted rule.
fn menu_title(label: &str, count: Option<usize>, theme: Theme) -> Div {
    div()
        .px(px(8.))
        .pt(px(6.))
        .pb(px(4.))
        .child(kit::section_header(label, count, theme))
}

impl Shell {
    // A menu item: an icon and a mono label. The chosen item of a set carries the accent tint,
    // wash, edge and a trailing check in place of a grey fill.
    fn menu_row(
        &self,
        id: impl Into<ElementId>,
        glyph: &'static str,
        label: impl Into<SharedString>,
        chosen: bool,
    ) -> Button {
        let t = self.theme;
        self.control(id)
            .w_full()
            .justify_start()
            .overflow_hidden()
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .text_color(if chosen { t.text } else { t.muted })
            .when(chosen, |row| {
                row.bg(t.accent.opacity(0.1))
                    .child(dither::row_wash(
                        t.accent,
                        crate::theme::RADIUS_CONTROL,
                        true,
                        false,
                    ))
                    .child(kit::selection_edge(t.accent))
            })
            .when(!chosen, |row| {
                row.child(kit::glint(t.dot, dither::Edge::Bottom))
                    .hover(move |row| row.bg(t.selected).text_color(t.text))
            })
            .active(move |row| row.bg(t.control_active))
            .on_press(move |position, cx| fx::ripple(position, t.muted, cx))
            .child(
                icon(glyph)
                    .size(px(13.))
                    .text_color(if chosen { t.accent } else { t.muted }),
            )
            .child(label.into())
            .when(chosen, |row| {
                row.child(div().flex_1())
                    .child(icon("check").size(px(12.)).text_color(t.accent))
            })
    }

    /// Builds contextual task, sort, and model menus without replacing the workspace.
    pub(super) fn menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut width = 284.;
        let mut menu = div().flex().flex_col().flex_shrink_0().gap(px(1.));
        match self.panel {
            Panel::Usage => {
                width = 320.;
                menu = menu.child(self.usage_panel(cx));
            }
            Panel::Notifications => {
                width = 340.;
                menu = menu.child(self.notification_center(cx));
            }
            Panel::Projects | Panel::ChatProjects => {
                let chat_picker = self.panel == Panel::ChatProjects;
                width = if chat_picker {
                    320.
                } else {
                    (self.settings.sidebar_width - 24.).max(220.)
                };
                menu = menu.child(menu_title(
                    "Projects",
                    self.workspace.as_ref().map(|w| w.projects.len()),
                    t,
                ));
                if !chat_picker {
                    menu = menu.child(
                        self.menu_row(
                            "all-projects",
                            "folder-open",
                            "All projects",
                            self.sidebar_project.is_none(),
                        )
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
                            .border_color(gpui::transparent_black())
                            .bg(gpui::transparent_black())
                            .text_color(if selected { t.text } else { t.muted })
                            .w_full()
                            .h(px(32.))
                            .justify_start()
                            .overflow_hidden()
                            .when(selected, |b| {
                                b.bg(t.accent.opacity(0.1))
                                    .child(dither::row_wash(
                                        t.accent,
                                        crate::theme::RADIUS_CONTROL,
                                        true,
                                        false,
                                    ))
                                    .child(kit::selection_edge(t.accent))
                            })
                            .when(!selected, |b| {
                                b.hover(move |b| b.bg(t.selected).text_color(t.text))
                            })
                            .child(glyph)
                            .child(div().flex_1().min_w_0().text_ellipsis().child(project.name))
                            .when(selected, |b| {
                                b.child(icon("check").size(px(12.)).text_color(t.accent))
                            })
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    if !chat_picker {
                                        s.sidebar_project = Some(id.clone());
                                        s.sidebar_dirty = true;
                                    }
                                    if chat_picker && s.selected_project.as_ref() != Some(&id) {
                                        // The next task starts from the chosen project's defaults.
                                        s.new_preferences = None;
                                    }
                                    s.selected_project = Some(id.clone());
                                    s.panel = Panel::None;
                                    if chat_picker && s.selected_runtime().models.is_empty() {
                                        s.connect_runtime();
                                    }
                                    cx.notify();
                                },
                            )),
                        );
                    }
                    menu = menu.child(projects);
                }
                menu = menu
                    .child(separator(t))
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
                                s.open_settings(settings::PROJECTS);
                                cx.notify();
                            })),
                    );
            }

            Panel::Comparison => {
                width = 212.;
                menu = menu.child(menu_title("Compare", None, t));
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
                    let label = self.comparison_label(&comparison, label);
                    menu = menu.child(
                        self.menu_row(("comparison-option", i), glyph, label, selected)
                            .h(px(28.))
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
                menu = menu.child(menu_title("Thread list", None, t));
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
                        self.menu_row(("sort-option", i), "chat", label, chosen)
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.settings.sort = sort;
                                s.sidebar_dirty = true;
                                s.panel = Panel::None;
                                s.save_settings();
                                cx.notify();
                            })),
                    );
                }
                menu = menu.child(separator(t)).child(
                    self.menu_row(
                        "group-projects",
                        "folder-closed",
                        "Group by project",
                        self.settings.group_tasks_by_project,
                    )
                    .accessibility_label(if self.settings.group_tasks_by_project {
                        "Disable project grouping"
                    } else {
                        "Enable project grouping"
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
                        menu = menu.child(menu_title("Thread", None, t)).child(
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
                        menu = menu.child(separator(t)).child(
                            // Built from the bare control: `ghost` already sets a hover style,
                            // and GPUI allows one hover style per element.
                            self.control("thread-delete")
                                .w_full()
                                .justify_start()
                                .border_color(gpui::transparent_black())
                                .bg(gpui::transparent_black())
                                .text_color(t.removed)
                                .child(icon("trash-2").size(px(13.)))
                                .child("Delete…")
                                .hover(move |b| b.bg(t.removed.opacity(0.1)))
                                .active(move |b| b.bg(t.removed.opacity(0.16)))
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
                    menu = menu
                        .child(menu_title(view.task.kind.noun(), None, t))
                        .child(
                            self.ghost("menu-rename", "edit", "Rename")
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
                        self.ghost("menu-pin", "pin", if pinned { "Unpin" } else { "Pin" })
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
                    // A check runs a command in the task's checkout, which a chat does not have.
                    menu = menu
                        .child(separator(t))
                        .when(view.task.kind.has_project(), |menu| {
                            menu.child(
                                self.ghost("menu-check", "check-square", "Run a check")
                                    .w_full()
                                    .justify_start()
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.aux.update(cx, |i, cx| i.set_text(String::new(), cx));
                                        s.panel = Panel::Check;
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            self.ghost("menu-resources", "code", "Skills & MCP")
                                .w_full()
                                .justify_start()
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.panel = Panel::Resources;
                                    s.send(core::Request::Resources {
                                        provider: s.composer_preferences().provider,
                                        account: s.composer_preferences().account,
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
                    menu = menu.child(separator(t)).child(
                        self.ghost(
                            "menu-archive",
                            "archive",
                            if archived { "Restore" } else { "Archive" },
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
            Panel::Models | Panel::Reasoning | Panel::Access | Panel::Mode => {
                width = if self.panel == Panel::Models {
                    580.
                } else {
                    360.
                };
                menu = menu.child(self.model_options(cx));
            }
            _ => {}
        }
        let viewport = window.viewport_size();
        // Measure content at its natural height. Only the window limits scrolling;
        // percentage caps inside the anchored layout can clip even short menus.
        let thread_context = (self.panel == Panel::ThreadMenu)
            .then_some(self.thread_menu.as_ref())
            .flatten();
        // A menu opens above its trigger, as the composer's menus do, unless the trigger sits in
        // the upper half of the window. There it opens below, so that it never climbs into the
        // title bar and under the window controls.
        let trigger = self
            .menu_anchor
            .unwrap_or(Bounds::new(point(px(24.), px(120.)), Size::default()));
        let upward = thread_context.is_none() && trigger.top() > viewport.height / 2.;
        let (position, anchor, offset) = match thread_context {
            Some(context) => (context.position, Anchor::TopLeft, px(0.)),
            None if upward => (trigger.top_right(), Anchor::BottomRight, px(-8.)),
            None => (trigger.bottom_left(), Anchor::TopLeft, px(6.)),
        };
        let menu = motion::popover(
            ("menu-enter", self.panel.clone() as usize),
            div()
                .relative()
                .child(dither::shadow(t, crate::theme::RADIUS_FLOATING, 5.))
                .child(
                    div()
                        .id("context-menu")
                        .w(px(width).min((viewport.width - px(16.)).max(px(0.))))
                        .max_h((viewport.height - px(16.)).max(px(0.)))
                        .overflow_y_scroll()
                        .p(px(4.))
                        .rounded(px(crate::theme::RADIUS_FLOATING))
                        .bg(t.raised)
                        .border_1()
                        .border_color(t.edge)
                        .shadow(t.floating())
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(menu),
                ),
            t,
            upward,
        );
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
                    .anchor(anchor)
                    .offset(point(px(0.), offset))
                    // The top margin is the title bar: no menu may cover the window controls.
                    .snap_to_window_with_margin(Edges {
                        top: gpui::px(chrome::TITLE_BAR) + px(4.),
                        right: px(8.),
                        bottom: px(8.),
                        left: px(8.),
                    })
                    .child(menu),
            )
            .into_any_element()
    }
}
