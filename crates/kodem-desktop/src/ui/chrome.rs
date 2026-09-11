use super::*;
impl Shell {
    fn titlebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let sidebar_width = if self.settings.collapsed {
            96.
        } else {
            self.settings.sidebar_width.max(300.) + 1.
        };
        let project = self.workspace.as_ref().and_then(|w| {
            let id = self
                .view
                .as_ref()
                .map(|v| &v.task.project_id)
                .or(self.selected_project.as_ref());
            w.projects.iter().find(|p| Some(&p.id) == id)
        });
        let name = project
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Workspace".into());
        let mut header = div()
            .relative()
            .h(px(38.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .window_control_area(WindowControlArea::Drag)
            .bg(t.sidebar)
            .border_b_1()
            .border_color(t.border)
            .child(motion::window_texture(
                self.settings.light,
                self.theme.accent,
            ))
            .child(
                div()
                    .w(px(sidebar_width))
                    .h_full()
                    .flex_shrink_0()
                    .pl(px(82.))
                    .pr_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(!self.settings.collapsed, |d| {
                        d.child(
                            div()
                                .font_family("Manrope ExtraBold")
                                .text_size(px(16.))
                                .font_weight(FontWeight::EXTRA_BOLD)
                                .child("Kodem"),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .px_2()
                                .py(px(2.))
                                .rounded(px(6.))
                                .bg(t.selected.opacity(0.55))
                                .text_color(t.muted)
                                .text_size(px(11.))
                                .text_ellipsis()
                                .child(crate::updater::version_label()),
                        )
                        .child(div().flex_1())
                        .child(
                            self.icon_button("header-sidebar", "panel-left", "Toggle sidebar · ⌘B")
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.settings.collapsed = true;
                                    s.save_settings();
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(div().w(px(1.)).h_full().bg(t.border));
        let mut project_path = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .flex_1()
            .px_4()
            .when(self.settings.collapsed, |d| {
                d.child(
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
                            .text_color(t.accent)
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_ellipsis()
                    .child(name),
            );
        if let Some(view) = &self.view {
            project_path = project_path
                .child(icon("chevron-right").size(px(12.)).text_color(t.subtle))
                .child(icon("branch").size(px(13.)).text_color(t.muted))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.muted)
                        .text_ellipsis()
                        .child(view.task.branch.clone()),
                );
        }
        let enabled = self.selected.is_some();
        header = header.child(project_path).child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_3()
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
                    self.icon_button("header-terminal", "square-terminal", "Toggle terminal")
                        .disabled(!enabled)
                        .when(self.show_terminal, |b| b.bg(t.selected).text_color(t.text))
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.show_terminal = !s.show_terminal;
                            s.save_settings();
                            cx.notify();
                        })),
                )
                .child(div().w(px(1.)).h(px(16.)).mx_2().bg(t.border))
                .child(
                    self.icon_button("header-review", "panel-right", "Toggle changes · ⌘D")
                        .disabled(!enabled)
                        .when(self.show_review && enabled, |b| {
                            b.bg(t.selected).text_color(t.text)
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
}
impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window_width = f32::from(window.viewport_size().width);
        if let Some(preview) = self.render_image_preview(window, cx) {
            return preview;
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
            .font_family("IBM Plex Sans")
            .text_size(px(self.settings.font_size - 1.))
            .line_height(relative(1.5))
            .font_weight(FontWeight::NORMAL)
            .text_color(t.text)
            .bg(t.canvas)
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
            .on_action(cx.listener(|s, _: &ShowSettings, _, cx| {
                s.panel = Panel::Settings;
                cx.notify();
            }))
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
        if let Some(error) = &self.error {
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_2()
                    .bg(t.removed_bg)
                    .text_size(px(12.))
                    .child(icon("circle-x").text_color(t.removed))
                    .child(div().flex_1().child(error.clone()))
                    .child(
                        self.icon_button("dismiss-error", "close", "Dismiss error")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.error = None;
                                cx.notify();
                            })),
                    ),
            );
        }
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
                    .bg(t.border)
                    .hover(move |d| d.bg(t.accent))
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
                        .bg(t.border)
                        .hover(move |d| d.bg(t.accent))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|s, _, _, _| s.dragging_review = s.window_width > 1100.),
                        ),
                )
                .child(self.review(cx, false));
        }
        root = root
            .child(body)
            .child(motion::cool_corner(self.settings.light));
        if matches!(
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
            root = root.child(self.menu(cx));
        } else if self.panel != Panel::None {
            root = root.child(self.panel(window, cx));
        }
        root
    }
}
