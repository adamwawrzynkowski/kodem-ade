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
                .child(motion::window_texture(self.settings.light))
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
            .child(motion::window_texture(self.settings.light))
            .on_hover(cx.listener(|s, hovered, _, cx| {
                s.sidebar_hovered = *hovered;
                cx.notify();
            }))
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
                        self.icon_button("sort-tasks", "settings-2", "Sort tasks")
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
        let mut feed = div()
            .id("task-feed")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .restrict_scroll_to_axis()
            .px_2()
            .pb_2();
        if let Some(workspace) = self.workspace.clone() {
            let mut tasks: Vec<_> = workspace
                .tasks
                .iter()
                .filter(|task| {
                    let project = workspace.projects.iter().find(|p| p.id == task.project_id);
                    task.archived == self.show_archived
                        && self
                            .sidebar_project
                            .as_ref()
                            .is_none_or(|id| id == &task.project_id)
                        && (query.is_empty()
                            || task.title.to_lowercase().contains(&query)
                            || task.branch.to_lowercase().contains(&query)
                            || project.is_some_and(|p| p.name.to_lowercase().contains(&query)))
                })
                .collect();
            tasks.sort_by(|a, b| {
                if self.sidebar_hovered {
                    return self
                        .task_order
                        .get(&a.id)
                        .unwrap_or(&usize::MAX)
                        .cmp(self.task_order.get(&b.id).unwrap_or(&usize::MAX))
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
                for (i, task) in tasks.iter().enumerate() {
                    self.task_order.insert(task.id.clone(), i);
                }
            }
            let (open, settled): (Vec<_>, Vec<_>) = tasks.into_iter().partition(|task| {
                task.state != RunState::Completed
                    || task.pinned
                    || self.selected.as_deref() == Some(&task.id)
            });
            let empty = open.is_empty() && settled.is_empty();
            for task in open {
                if let Some(project) = workspace.projects.iter().find(|p| p.id == task.project_id) {
                    feed = feed.child(self.task_card(task, project, cx));
                }
            }
            if !settled.is_empty() {
                feed = feed.child(
                    self.ghost(
                        "settled-tasks",
                        if self.settled_collapsed {
                            "chevron-right"
                        } else {
                            "chevron-down"
                        },
                        "Settled",
                    )
                    .w_full()
                    .justify_start()
                    .mt_2()
                    .mb_1()
                    .text_size(px(11.))
                    .child(div().flex_1())
                    .child(div().text_color(t.subtle).child(settled.len().to_string()))
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.settled_collapsed = !s.settled_collapsed;
                        cx.notify();
                    })),
                );
                if !self.settled_collapsed || !query.is_empty() {
                    for task in settled {
                        if let Some(project) =
                            workspace.projects.iter().find(|p| p.id == task.project_id)
                        {
                            feed = feed.child(self.task_card(task, project, cx));
                        }
                    }
                }
            }
            if empty {
                feed = feed.child(
                    div()
                        .px_3()
                        .py_5()
                        .text_size(px(12.))
                        .text_color(t.muted)
                        .child(if workspace.projects.is_empty() {
                            "Open a repository to start a task."
                        } else if !query.is_empty() {
                            "No matching tasks."
                        } else {
                            "No tasks here yet. Start a new task above."
                        }),
                );
            }
        }
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
                            cx.notify();
                        })),
                )
                .child(
                    self.ghost("settings", "settings", "Settings")
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.panel = Panel::Settings;
                            cx.notify();
                        })),
                )
                .child(div().flex_1())
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
    fn task_card(
        &self,
        task: &core::Task,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let selected = self.selected.as_deref() == Some(&task.id);
        let surface = t.raised;
        let title_color = if selected {
            t.text
        } else {
            t.text.opacity(0.94)
        };
        let metadata = t.muted;
        let mut hover_surface = surface;
        hover_surface.lightness = (surface.lightness + 0.016).min(1.);
        let id = task.id.clone();
        let status_color = match task.state {
            RunState::Starting | RunState::Running => t.accent,
            RunState::Waiting => t.accent,
            RunState::Failed | RunState::DeliveryUnknown => t.removed,
            RunState::Completed => t.muted,
            _ => t.muted,
        };
        let status = if task.state.active() {
            format!(
                "{} {}",
                if task.state == RunState::Running {
                    "Working"
                } else {
                    task.state.label()
                },
                elapsed(task.updated)
            )
        } else {
            task.state.label().into()
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
            .w_full()
            .mb(px(6.))
            .px(px(10.))
            .py(px(5.))
            .line_height(relative(1.25))
            .rounded(px(9.))
            .bg(surface)
            .border_1()
            .border_color(t.muted.opacity(0.12))
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
            .child(
                div()
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .text_color(metadata)
                    .child(project_icon(project))
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
                    .child(if task.state.active() {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_color(status_color)
                            .child(motion::active_icon(
                                (
                                    ElementId::from("sidebar-status"),
                                    SharedString::from(id.clone()),
                                ),
                                if task.state == RunState::Waiting {
                                    "shield"
                                } else {
                                    "loader"
                                },
                                status_color,
                                task.state != RunState::Waiting,
                            ))
                            .child(motion::gleam(
                                (
                                    ElementId::from("sidebar-status-label"),
                                    SharedString::from(id.clone()),
                                ),
                                status,
                                t,
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
            .on_click(cx.listener(move |s, _, _, cx| s.select_task(id.clone(), cx)))
            .into_any_element()
    }
}
/// Returns a stable, distinct project glyph and tint without filesystem work during rendering.
pub(super) fn project_icon(project: &Project) -> AnyElement {
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
    let tint = [
        0xb5a28c, 0xa7a0af, 0x9ca58f, 0xc49c75, 0x91aaa3, 0xb3999e, 0x9ba4b5, 0xb0aa8f,
    ][(hash >> 8) as usize % 8];
    div()
        .size(px(18.))
        .rounded(px(4.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(tint).opacity(0.08))
        .child(icon(glyph).size(px(15.)).text_color(rgb(tint)))
        .into_any_element()
}
fn rank(state: &RunState) -> u8 {
    match state {
        RunState::Waiting => 0,
        RunState::Failed | RunState::DeliveryUnknown => 1,
        RunState::Running | RunState::Starting => 2,
        RunState::Idle => 3,
        _ => 4,
    }
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
