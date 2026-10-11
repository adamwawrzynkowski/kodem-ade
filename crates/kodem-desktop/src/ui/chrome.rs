use super::*;
impl Shell {
    fn titlebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let project = self.composer_project();
        let name = project
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Workspace".into());
        let (running, waiting) = self.workspace.as_ref().map_or((0, 0), |w| {
            w.tasks
                .iter()
                .filter(|task| !task.archived)
                .fold((0, 0), |(running, waiting), task| {
                    if task.needs_input() {
                        (running, waiting + 1)
                    } else if task.state.active() {
                        (running + 1, waiting)
                    } else {
                        (running, waiting)
                    }
                })
        });
        let mut header = div()
            .relative()
            .h(px(40.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .window_control_area(WindowControlArea::Drag)
            .bg(t.sidebar)
            .border_b_1()
            .border_color(t.border)
            .when(!self.settings.collapsed, |d| {
                d.child(
                    div()
                        .w(px(self.settings.sidebar_width))
                        .h_full()
                        .flex_shrink_0()
                        .pl(px(80.))
                        .pr(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(div().id("wordmark").flex_shrink_0().child(dither::wordmark(
                            t,
                            running > 0,
                            cx.reduce_motion(),
                        )))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .child(caps(crate::updater::version_label(), t).text_ellipsis()),
                        )
                        .child(div().flex_1())
                        .child(
                            self.icon_button("header-sidebar", "panel-left", "Toggle sidebar · ⌘B")
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.settings.collapsed = true;
                                    s.save_settings();
                                    cx.notify();
                                })),
                        ),
                )
                .child(dither::DottedRule::vertical(t.subtle.opacity(0.5)))
            });
        let mut project_path = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .min_w_0()
            .flex_1()
            .px(px(14.))
            .when(self.settings.collapsed, |d| {
                d.pl(px(80.)).child(
                    self.icon_button("expand-sidebar", "panel-left", "Show sidebar · ⌘B")
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.settings.collapsed = false;
                            s.save_settings();
                            cx.notify();
                        })),
                )
            })
            .child(
                project
                    .map(|project| sidebar::project_icon(project, t))
                    .unwrap_or_else(|| {
                        icon("folder-closed")
                            .size(px(14.))
                            .text_color(t.accent)
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(px(220.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_ellipsis()
                    .child(name),
            );
        if let Some(view) = &self.view {
            project_path = project_path
                .child(figure("/", t.subtle))
                .child(icon("branch").size(px(12.)).text_color(t.subtle))
                .child(
                    div()
                        .min_w_0()
                        .font_family(crate::theme::MONO)
                        .text_size(px(11.))
                        .text_color(t.muted)
                        .text_ellipsis()
                        .child(view.task.branch.clone()),
                )
                .when(view.task.owns_worktree, |d| {
                    d.child(kit::chip("WORKTREE", t.info))
                });
        }
        let enabled = self.selected.is_some();
        let readout = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(6.))
            .when(waiting > 0, |d| {
                d.child(
                    self.control("titlebar-waiting")
                        .h(px(22.))
                        .px(px(7.))
                        .gap(px(6.))
                        .bg(t.warning.opacity(0.09))
                        .border_color(t.warning.opacity(0.45))
                        .text_color(t.warning)
                        .hover(move |s| s.bg(t.warning.opacity(0.16)))
                        .child(kit::marker(t.warning))
                        .child(format!("{waiting} NEED YOU"))
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: "Open the next task that waits for you · ⌥⌘A".into(),
                                theme: t,
                            })
                            .into()
                        })
                        .on_click(cx.listener(|s, _, _, cx| s.select_next_attention(cx))),
                )
            })
            .when(running > 0, |d| {
                d.child(
                    div()
                        .id("titlebar-running")
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(dither::pulse(t.accent, cx.reduce_motion()))
                        .child(caps(format!("{running} running"), t).text_color(t.muted))
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: "Tasks with an active agent turn".into(),
                                theme: t,
                            })
                            .into()
                        }),
                )
            });
        header = header
            .child(project_path)
            .child(readout)
            .child(
                self.control("header-palette")
                    .h(px(24.))
                    .w(px(220.))
                    .flex_shrink(1.)
                    .min_w(px(28.))
                    .overflow_hidden()
                    .px(px(8.))
                    .gap(px(7.))
                    .justify_start()
                    .bg(t.inset)
                    .border_color(t.border)
                    .text_color(t.subtle)
                    .hover(move |s| s.border_color(t.accent.opacity(0.5)).text_color(t.muted))
                    .accessibility_label("Command palette")
                    .child(icon("search").size(px(12.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child("Threads and commands"),
                    )
                    .child(kit::keycap("⌘K", t))
                    .on_click(cx.listener(|s, _, w, cx| s.open_palette(w, cx))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .px(px(10.))
                    .child(
                        self.icon_button("header-files", "file-text", "Browse project files")
                            .disabled(!enabled)
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.panel = Panel::Files;
                                if let Some(task_id) = &s.selected {
                                    s.send(core::Request::Files {
                                        task_id: task_id.clone(),
                                    });
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        self.icon_button(
                            "header-terminal",
                            "square-terminal",
                            "Toggle terminal · ⌃`",
                        )
                        .disabled(!enabled)
                        .when(self.show_terminal, |b| {
                            b.bg(t.accent.opacity(0.14)).text_color(t.accent)
                        })
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.show_terminal = !s.show_terminal;
                            s.save_settings();
                            cx.notify();
                        })),
                    )
                    .child(
                        self.icon_button("header-review", "panel-right", "Toggle changes · ⌘D")
                            .disabled(!enabled)
                            .when(self.show_review && enabled, |b| {
                                b.bg(t.accent.opacity(0.14)).text_color(t.accent)
                            })
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.show_review = !s.show_review;
                                s.save_settings();
                                cx.notify();
                            })),
                    ),
            );
        header.into_any_element()
    }

    // Floating notices above the workspace: a persistent error card and a short confirmation.
    fn notices(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut stack = div()
            .absolute()
            .bottom(px(18.))
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.));
        if let Some(error) = &self.error {
            stack = stack.child(motion::toast(
                "error-toast",
                div()
                    .relative()
                    .max_w(px(620.))
                    .child(dither::shadow(t, crate::theme::RADIUS_SURFACE, 5.))
                    .child(
                        div()
                            .id("error-notice")
                            .occlude()
                            .flex()
                            .items_start()
                            .gap(px(10.))
                            .pl(px(12.))
                            .pr(px(6.))
                            .py(px(8.))
                            .rounded(px(crate::theme::RADIUS_SURFACE))
                            .bg(t.raised)
                            .border_1()
                            .border_color(t.removed.opacity(0.5))
                            .text_size(px(12.))
                            .child(div().pt(px(3.)).child(kit::marker(t.removed)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(caps("Error", t).text_color(t.removed))
                                    .child(div().text_color(t.text).child(error.clone())),
                            )
                            .child(
                                self.icon_button("dismiss-error", "close", "Dismiss error")
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.error = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            ));
        }
        if let Some(toast) = &self.toast {
            stack = stack.child(motion::toast(
                ("toast", toast.sequence as usize),
                div()
                    .relative()
                    .child(dither::shadow(t, crate::theme::RADIUS_CONTROL, 4.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .h(px(30.))
                            .px(px(12.))
                            .rounded(px(crate::theme::RADIUS_CONTROL))
                            .bg(t.raised)
                            .border_1()
                            .border_color(toast.tone.opacity(0.5))
                            .font_family(crate::theme::MONO)
                            .text_size(px(11.5))
                            .child(kit::marker(toast.tone))
                            .child(toast.text.clone()),
                    ),
            ));
        }
        stack.into_any_element()
    }

    fn go_to_task(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(id) = self.feed_tasks().get(index) {
            self.select_task(id.clone(), cx);
        }
    }
}
impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window_width = f32::from(window.viewport_size().width);
        fx::begin_frame(cx);
        dither::begin_frame();
        if let Some(preview) = self.render_image_preview(window, cx) {
            return preview;
        }
        if let Some(popup) = self.render_agent_popup(cx) {
            return popup;
        }
        if cx.try_global::<Theme>() != Some(&self.theme) {
            cx.set_global(self.theme);
        }
        let t = self.theme;
        let mut root = div()
            .id("workspace")
            .relative()
            .track_focus(&self.focus)
            .key_context("Workspace")
            .tab_group()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family(crate::theme::SANS)
            .text_size(px(self.settings.font_size - 1.))
            .line_height(relative(1.5))
            .font_weight(FontWeight::NORMAL)
            .text_color(t.text)
            .bg(t.backdrop)
            .on_action(cx.listener(Self::open_project))
            .on_action(cx.listener(Self::new_task))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(|s, _: &ToggleSidebar, _, cx| {
                s.settings.collapsed = !s.settings.collapsed;
                s.save_settings();
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &ToggleReview, _, cx| {
                s.show_review = !s.show_review;
                s.save_settings();
                if !s.show_review && s.panel == Panel::Comparison {
                    s.panel = Panel::None;
                }
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &ToggleTerminal, _, cx| {
                if s.selected.is_some() {
                    s.show_terminal = !s.show_terminal;
                    s.save_settings();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|s, _: &ShowSettings, _, cx| {
                s.panel = Panel::Settings;
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &CommandPalette, w, cx| {
                if s.panel == Panel::Palette {
                    s.panel = Panel::None;
                    w.focus(&s.focus, cx);
                    cx.notify();
                } else {
                    s.open_palette(w, cx);
                }
            }))
            .on_action(cx.listener(|s, _: &NextTask, _, cx| s.select_adjacent_task(1, cx)))
            .on_action(cx.listener(|s, _: &PreviousTask, _, cx| s.select_adjacent_task(-1, cx)))
            .on_action(cx.listener(|s, _: &NextAttention, _, cx| s.select_next_attention(cx)))
            .on_action(
                cx.listener(|s, _: &ChooseModel, w, cx| s.open_agent_menu(Panel::Models, w, cx)),
            )
            .on_action(
                cx.listener(|s, _: &ChooseEffort, w, cx| {
                    s.open_agent_menu(Panel::Reasoning, w, cx)
                }),
            )
            .on_action(
                cx.listener(|s, _: &ChooseAccess, w, cx| s.open_agent_menu(Panel::Access, w, cx)),
            )
            .on_action(cx.listener(|s, _: &CycleAccess, _, cx| s.cycle_access(cx)))
            .on_action(cx.listener(|s, _: &GoToTask1, _, cx| s.go_to_task(0, cx)))
            .on_action(cx.listener(|s, _: &GoToTask2, _, cx| s.go_to_task(1, cx)))
            .on_action(cx.listener(|s, _: &GoToTask3, _, cx| s.go_to_task(2, cx)))
            .on_action(cx.listener(|s, _: &GoToTask4, _, cx| s.go_to_task(3, cx)))
            .on_action(cx.listener(|s, _: &GoToTask5, _, cx| s.go_to_task(4, cx)))
            .on_action(cx.listener(|s, _: &GoToTask6, _, cx| s.go_to_task(5, cx)))
            .on_action(cx.listener(|s, _: &GoToTask7, _, cx| s.go_to_task(6, cx)))
            .on_action(cx.listener(|s, _: &GoToTask8, _, cx| s.go_to_task(7, cx)))
            .on_action(cx.listener(|s, _: &GoToTask9, _, cx| s.go_to_task(8, cx)))
            .on_action(cx.listener(|s, _: &Search, w, cx| {
                s.settings.collapsed = false;
                s.search_visible = true;
                w.focus(&s.search.read(cx).focus_handle(cx), cx);
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &Latest, _, cx| {
                s.follow = true;
                s.chat_scroll.scroll_to_end();
                cx.notify();
            }))
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .on_mouse_move(cx.listener(|s, event: &MouseMoveEvent, _, cx| {
                if s.dragging_sidebar {
                    s.settings.sidebar_width = f32::from(event.position.x).clamp(220., 420.);
                    cx.notify();
                }
                if s.dragging_review {
                    s.settings.review_width =
                        (s.window_width - f32::from(event.position.x)).clamp(360., 900.);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    if s.dragging_sidebar || s.dragging_review {
                        s.save_settings();
                    }
                    s.dragging_sidebar = false;
                    s.dragging_review = false;
                    cx.notify();
                }),
            );
        root = root.child(self.titlebar(cx));
        let sidebar = self.sidebar(cx);
        let narrow_review = self.show_review
            && self.selected.is_some()
            && self.window_width <= 1100.
            && !(self.reviewing_file
                && self.settings.diff_placement == DiffPlacement::Conversation);
        let chat = self.chat(window, cx);
        let mut body = div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(sidebar)
            .child(
                div()
                    .id("sidebar-divider")
                    .w(px(1.))
                    .flex_shrink_0()
                    .h_full()
                    .cursor_col_resize()
                    .bg(if self.dragging_sidebar {
                        t.accent
                    } else {
                        t.border
                    })
                    .hover(move |d| d.bg(t.accent.opacity(0.6)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|s, _, _, _| s.dragging_sidebar = true),
                    ),
            )
            .when(!narrow_review, |d| d.child(chat));
        if self.show_review
            && self.selected.is_some()
            && (self.window_width > 1100. || narrow_review)
        {
            body = body
                .child(
                    div()
                        .id("review-divider")
                        .w(px(1.))
                        .flex_shrink_0()
                        .h_full()
                        .cursor_col_resize()
                        .bg(if self.dragging_review {
                            t.accent
                        } else {
                            t.border
                        })
                        .hover(move |d| d.bg(t.accent.opacity(0.6)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|s, _, _, _| s.dragging_review = s.window_width > 1100.),
                        ),
                )
                .child(self.review(cx, false));
        }
        root = root.child(body);
        if self.error.is_some() || self.toast.is_some() {
            root = root.child(self.notices(cx));
        }
        if self.panel == Panel::Palette {
            root = root.child(self.palette(window, cx));
        } else if matches!(
            self.panel,
            Panel::Models
                | Panel::Usage
                | Panel::Reasoning
                | Panel::Access
                | Panel::Projects
                | Panel::ChatProjects
                | Panel::TaskMenu
                | Panel::ThreadMenu
                | Panel::Sort
                | Panel::Comparison
        ) {
            root = root.child(self.menu(window, cx));
        } else if self.panel != Panel::None {
            root = root.child(self.panel(window, cx));
        }
        root
    }
}

/// The window root: the workspace beneath the effect layer.
///
/// The workspace is a cached view. A frame that only advances an effect reuses the workspace's
/// last paint instead of rendering it again.
pub struct Stage {
    shell: Entity<Shell>,
    fx: Entity<fx::FxLayer>,
}

impl Stage {
    /// Creates the workspace and the effect layer of one window.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            shell: cx.new(|cx| Shell::new(window, cx)),
            fx: cx.new(fx::FxLayer::new),
        }
    }
}

impl Render for Stage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .child(AnyView::from(self.shell.clone()).cached(StyleRefinement::default().size_full()))
            .child(self.fx.clone())
    }
}
