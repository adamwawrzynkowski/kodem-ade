use super::*;
impl Shell {
    fn titlebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let project = self.composer_project();
        let name = project.map(|p| p.name.clone()).unwrap_or_else(|| {
            if self.mode.has_project() {
                "Workspace".into()
            } else {
                self.mode.label().into()
            }
        });
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
        let scale = crate::theme::ui_scale();
        let mut header = div()
            .relative()
            // The title bar and the room for the window controls are window pixels: macOS
            // draws the controls at one size at every interface scale.
            .h(gpui::px(TITLE_BAR))
            .flex_shrink_0()
            .flex()
            .items_center()
            // Only the bar's empty areas move the window. Every control in it occludes the bar
            // under it, so these handlers and the system's hit test on Windows miss it there.
            .window_control_area(WindowControlArea::Drag)
            .when(cfg!(target_os = "macos"), |header| {
                // The app owns the drag of the title bar, so AppKit leaves the controls alone.
                header.on_mouse_down(MouseButton::Left, |event: &MouseDownEvent, window, _| {
                    if event.click_count == 2 {
                        window.titlebar_double_click();
                    } else {
                        window.start_window_move();
                    }
                })
            })
            .when(client_decorations(window), |header| {
                // A Linux compositor moves the window only when the app asks it to.
                header
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|s, event: &MouseDownEvent, window, _| {
                            if event.click_count == 2 {
                                window.zoom_window();
                            } else {
                                s.title_press = true;
                            }
                        }),
                    )
                    .on_mouse_move(cx.listener(|s, event: &MouseMoveEvent, window, _| {
                        // A press that ended away from the bar must not move the window later.
                        if s.title_press && event.dragging() {
                            window.start_window_move();
                        }
                        s.title_press = false;
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|s, _, _, _| s.title_press = false),
                    )
            })
            .bg(t.sidebar)
            .border_b_1()
            .border_color(t.border)
            .when(self.reveals.sidebar > 0., |d| {
                d.child(
                    div()
                        // The section ends above the sidebar's trailing edge. While the sidebar
                        // slides, it shrinks toward the room of the window controls.
                        .w(gpui::px(
                            WINDOW_CONTROLS
                                + ((workspaces::RAIL + 1. + self.settings.sidebar_width) * scale
                                    - WINDOW_CONTROLS)
                                    * self.reveals.sidebar,
                        ))
                        .overflow_hidden()
                        .h_full()
                        .flex_shrink_0()
                        .pl(gpui::px(WINDOW_CONTROLS))
                        .pr(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            // The mark and the name in the interface's own mono face.
                            div()
                                .id("wordmark")
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .gap(px(7.))
                                .child(
                                    div()
                                        .font_family(crate::theme::mono())
                                        .text_size(text_px(12.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(t.text)
                                        .child("Kodem"),
                                ),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .child(caps(crate::updater::version_label(), t).text_ellipsis()),
                        )
                        .child(div().flex_1())
                        .child(
                            self.icon_button("header-sidebar", "panel-left", "Toggle sidebar · ⌘B")
                                .occlude()
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
            .when(self.reveals.sidebar <= 0., |d| {
                d.pl(gpui::px(WINDOW_CONTROLS)).child(
                    self.icon_button("expand-sidebar", "panel-left", "Show sidebar · ⌘B")
                        .occlude()
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
                        icon(if self.mode.has_project() {
                            "folder-closed"
                        } else {
                            self.mode.icon()
                        })
                        .size(px(14.))
                        .text_color(t.accent)
                        .into_any_element()
                    }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(px(220.))
                    .text_size(text_px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_ellipsis()
                    .child(name),
            );
        if let Some(view) = self.view.as_ref().filter(|_| self.mode.has_project()) {
            let review = view.task.kind == TaskKind::Review;
            // A folder without Git has no branch to name.
            project_path = project_path
                .when(!view.task.branch.is_empty(), |d| {
                    d.child(figure("/", t.subtle))
                        .child(icon("branch").size(px(12.)).text_color(t.subtle))
                        .child(
                            div()
                                .min_w_0()
                                .font_family(crate::theme::mono())
                                .text_size(text_px(11.))
                                .text_color(t.muted)
                                .text_ellipsis()
                                .child(view.task.branch.clone()),
                        )
                })
                .when(view.task.owns_worktree && !review, |d| {
                    d.child(kit::chip("WORKTREE", t.info))
                })
                .when(review, |d| d.child(kit::chip("READ ONLY", t.info)));
        }
        let enabled = self.has_checkout();
        let readout = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(6.))
            .when(waiting > 0, |d| {
                d.child(
                    self.control("titlebar-waiting")
                        .occlude()
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
        if let Some(git) = self.git.clone().filter(|_| self.git_open) {
            let title = git.update(cx, |git, cx| git.render_title_bar(window, cx));
            header = header.child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .flex_1()
                    .px(px(14.))
                    .when(self.reveals.sidebar <= 0., |d| {
                        d.pl(gpui::px(WINDOW_CONTROLS)).child(
                            self.icon_button("expand-sidebar", "panel-left", "Show sidebar · ⌘B")
                                .occlude()
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.settings.collapsed = false;
                                    s.save_settings();
                                    cx.notify();
                                })),
                        )
                    })
                    .child(title),
            );
            return header
                .when(app_window_buttons(window), |header| {
                    header.child(self.window_buttons(window, cx))
                })
                .into_any_element();
        }
        header = header
            .child(project_path)
            .child(readout)
            .child(
                self.control("header-palette")
                    .occlude()
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
                            .occlude()
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
                        .occlude()
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
                            .occlude()
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
            )
            .when(app_window_buttons(window), |header| {
                header.child(self.window_buttons(window, cx))
            });
        header.into_any_element()
    }

    /// Returns the minimize, maximize and close buttons that Windows, and a Linux compositor
    /// without server-side decorations, leave to the app. On Windows the system handles their
    /// clicks through their window control areas, which also opens Snap Layouts.
    fn window_buttons(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let button = |id: &'static str, glyph: &'static str, area: WindowControlArea| {
            let hover = if area == WindowControlArea::Close {
                t.removed
            } else {
                t.control_hover
            };
            div()
                .id(id)
                .w(gpui::px(46.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(t.muted)
                // Over the bar's drag area, the button is the only control area that it hits.
                .occlude()
                .window_control_area(area)
                .hover(move |s| {
                    s.bg(hover).text_color(if area == WindowControlArea::Close {
                        gpui::white()
                    } else {
                        t.text
                    })
                })
                .child(icon(glyph).size(gpui::px(12.)))
        };
        div()
            .h_full()
            .flex()
            .flex_shrink_0()
            .child(
                button("window-minimize", "minus", WindowControlArea::Min)
                    .on_click(cx.listener(|_, _, window, _| window.minimize_window())),
            )
            .child(
                button(
                    "window-maximize",
                    if window.is_maximized() {
                        "copy"
                    } else {
                        "square"
                    },
                    WindowControlArea::Max,
                )
                .on_click(cx.listener(|_, _, window, _| window.zoom_window())),
            )
            .child(
                button("window-close", "close", WindowControlArea::Close)
                    .on_click(cx.listener(|_, _, window, _| window.remove_window())),
            )
            .into_any_element()
    }

    fn go_to_task(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(id) = self.feed_tasks(cx).get(index) {
            self.close_git(cx);
            self.select_task(id.clone(), cx);
        }
    }
}
impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The saved scale, text sizes and typefaces apply before any length is built. The
        // window root and the layers beside this view are cached, so a change redraws them.
        if crate::theme::apply_metrics(&self.settings) {
            // Every row of the lists has a new height.
            self.chat_scroll.remeasure();
            self.sidebar_scroll.remeasure();
            window.on_next_frame(|window, _| window.refresh());
        }
        // Spacing that is given in rems follows the interface scale with the rem.
        window.set_rem_size(gpui::px(BASE_REM) * crate::theme::ui_scale());
        // Widths that the layout compares with its breakpoints and saved pane sizes are in
        // points of the interface scale.
        self.window_width = f32::from(window.viewport_size().width) / crate::theme::ui_scale();
        backdrop::begin_frame(window, cx);
        self.sync_reveals(cx);
        self.sync_error_notices(cx);
        self.sync_censor();
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
            // The window root and the chat background read the theme and are cached or render
            // before this view, so a changed theme redraws everything once.
            window.on_next_frame(|window, _| window.refresh());
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
            .font_family(crate::theme::sans())
            .text_size(text_px(13.))
            .line_height(relative(1.5))
            .font_weight(FontWeight::NORMAL)
            .text_color(t.text)
            // No fill: the window root paints the backdrop color, and the chat surface with its
            // glyph field lies beneath this view.
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
                if s.has_checkout() {
                    s.show_terminal = !s.show_terminal;
                    s.save_settings();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|s, _: &ShowSettings, _, cx| {
                // The Git workspace's preferences are its own section.
                if s.git_open {
                    s.open_settings(settings::GIT);
                } else {
                    s.panel = Panel::Settings;
                }
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
            .on_action(cx.listener(|s, _: &ShowCoding, _, cx| s.set_mode(TaskKind::Coding, cx)))
            .on_action(cx.listener(|s, _: &ShowChat, _, cx| s.set_mode(TaskKind::Chat, cx)))
            .on_action(cx.listener(|s, _: &ShowReview, _, cx| s.set_mode(TaskKind::Review, cx)))
            .on_action(cx.listener(|s, _: &ShowGit, _, cx| s.open_git(cx)))
            .on_action(cx.listener(|s, _: &CloneRepository, window, cx| {
                s.open_git(cx);
                let git = s.git_workspace(window, cx);
                git.update(cx, |git, cx| git.clone_repository_dialog(window, cx));
            }))
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
                // In the Git workspace, search looks through the open repository's commits.
                if s.git_open
                    && let Some(git) = s.git.clone()
                {
                    git.update(cx, |git, cx| git.open_commit_search(cx));
                    return;
                }
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
                // The pointer is in window pixels; the saved widths are in points.
                let pointer = f32::from(event.position.x) / crate::theme::ui_scale();
                if s.dragging_sidebar {
                    s.settings.sidebar_width = (pointer - workspaces::RAIL - 1.).clamp(220., 420.);
                    cx.notify();
                }
                if s.dragging_review {
                    s.settings.review_width = (s.window_width - pointer).clamp(360., 900.);
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
        // The Git workspace exists once it is open, so that its parts can draw this frame.
        let git = if self.git_open {
            Some(self.git_workspace(window, cx))
        } else {
            None
        };
        root = root.child(self.titlebar(window, cx));
        let rail = self.workspace_rail(window, cx);
        let sidebar = match &git {
            Some(git) => self.git_sidebar(git, cx),
            None => self.sidebar(cx),
        };
        let sidebar_open = self.reveals.sidebar;
        // While the sidebar slides it is clipped to its current width, held at its trailing
        // edge so that it moves in from the side.
        let sidebar = if sidebar_open > 0. && sidebar_open < 1. {
            div()
                .w(px(self.settings.sidebar_width * sidebar_open))
                .h_full()
                .flex_shrink_0()
                .flex()
                .justify_end()
                .overflow_hidden()
                .child(sidebar)
                .into_any_element()
        } else {
            sidebar
        };
        let narrow_review = git.is_none()
            && self.show_review
            && self.has_checkout()
            && self.window_width <= 1100.
            && !(self.reviewing_file
                && self.settings.diff_placement == DiffPlacement::Conversation);
        // The content of a workspace that just opened clears out of a dissolve.
        let chat = match &git {
            Some(git) => motion::switched(
                "workspace-enter-git",
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(git.clone()),
                t.canvas,
                5.,
            ),
            None => motion::switched(
                ("workspace-enter", self.mode as usize),
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.chat(window, cx)),
                t.canvas,
                5.,
            ),
        };
        let mut body = div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(rail)
            // The rail and the sidebar are one surface, so a dotted rule separates them.
            .when(sidebar_open > 0., |d| {
                d.child(dither::DottedRule::vertical(t.subtle.opacity(0.5)))
                    .child(sidebar)
            })
            .child(div().w(px(1.)).flex_shrink_0().h_full().bg(t.border))
            .when(!narrow_review, |d| d.child(chat));
        let mut handles = Vec::new();
        if sidebar_open >= 1. {
            handles.push(
                self.resize_handle("sidebar-divider", self.dragging_sidebar)
                    .left(px(
                        workspaces::RAIL + 1. + self.settings.sidebar_width - RESIZE_REACH
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|s, _, _, cx| {
                            s.dragging_sidebar = true;
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        let review_open = self.reveals.review;
        if narrow_review {
            body = body
                .child(div().w(px(1.)).flex_shrink_0().h_full().bg(t.border))
                .child(self.review(cx, false));
        } else if git.is_none()
            && review_open > 0.
            && self.has_checkout()
            && self.window_width > 1100.
        {
            // The review pane is as wide as `review` makes it; the divider sits before it.
            let width = self.settings.review_width.min(self.window_width * 0.55);
            let review = self.review(cx, false);
            body = body
                .child(div().w(px(1.)).flex_shrink_0().h_full().bg(t.border))
                .child(if review_open < 1. {
                    // While the pane slides it is clipped to its current width; its leading
                    // edge moves in from the side of the window.
                    div()
                        .w(px(width * review_open))
                        .h_full()
                        .flex_shrink_0()
                        .overflow_hidden()
                        .child(review)
                        .into_any_element()
                } else {
                    review
                });
            if review_open >= 1. {
                handles.push(
                    self.resize_handle("review-divider", self.dragging_review)
                        .right(px(width - RESIZE_REACH))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|s, _, _, cx| {
                                s.dragging_review = true;
                                cx.stop_propagation();
                            }),
                        ),
                );
            }
        }
        // The handles come last, so they sit above the panes on both sides of a divider.
        root = root.child(body.relative().children(handles));
        root = root.children(self.floating_notices(cx));
        if self.panel == Panel::Palette {
            root = root.child(self.palette(window, cx));
        } else if matches!(
            self.panel,
            Panel::Models
                | Panel::Usage
                | Panel::Notifications
                | Panel::Reasoning
                | Panel::Access
                | Panel::Mode
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

/// Half the width of a pane divider's hit area, in points.
const RESIZE_REACH: f32 = 5.;

/// Width at the leading end of the title bar that the window controls take, in window pixels.
/// Only macOS puts its controls there; elsewhere this is the title bar's leading margin.
#[cfg(target_os = "macos")]
const WINDOW_CONTROLS: f32 = 80.;
#[cfg(not(target_os = "macos"))]
const WINDOW_CONTROLS: f32 = 12.;

/// Tells whether the app draws the window frame because the platform does not: a Linux
/// compositor without server-side decorations, such as GNOME on Wayland.
fn client_decorations(window: &Window) -> bool {
    cfg!(not(any(target_os = "macos", windows)))
        && matches!(window.window_decorations(), Decorations::Client { .. })
}

/// Tells whether the title bar carries the app's own minimize, maximize and close buttons.
fn app_window_buttons(window: &Window) -> bool {
    cfg!(windows) || client_decorations(window)
}

/// Width of the band along a self-drawn window edge that resizes the window, in pixels.
const RESIZE_BORDER: f32 = 5.;

/// Thin bands along the edges and corners of a window whose frame the app draws. Pressing one
/// asks the compositor to resize the window from that edge. Tiled and maximized edges have none.
fn resize_borders(window: &Window) -> Option<AnyElement> {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return None;
    };
    if cfg!(any(target_os = "macos", windows)) || window.is_maximized() {
        return None;
    }
    let band = |id: &'static str, edge: ResizeEdge, cursor: CursorStyle| {
        div()
            .id(id)
            .absolute()
            .occlude()
            .cursor(cursor)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                window.start_window_resize(edge);
            })
    };
    let reach = px(RESIZE_BORDER);
    let corner = px(RESIZE_BORDER * 3.);
    let mut borders = div().absolute().top_0().left_0().size_full();
    if !tiling.top {
        borders = borders.child(
            band("resize-top", ResizeEdge::Top, CursorStyle::ResizeUpDown)
                .top_0()
                .left_0()
                .right_0()
                .h(reach),
        );
    }
    if !tiling.bottom {
        borders = borders.child(
            band(
                "resize-bottom",
                ResizeEdge::Bottom,
                CursorStyle::ResizeUpDown,
            )
            .bottom_0()
            .left_0()
            .right_0()
            .h(reach),
        );
    }
    if !tiling.left {
        borders = borders.child(
            band(
                "resize-left",
                ResizeEdge::Left,
                CursorStyle::ResizeLeftRight,
            )
            .top_0()
            .bottom_0()
            .left_0()
            .w(reach),
        );
    }
    if !tiling.right {
        borders = borders.child(
            band(
                "resize-right",
                ResizeEdge::Right,
                CursorStyle::ResizeLeftRight,
            )
            .top_0()
            .bottom_0()
            .right_0()
            .w(reach),
        );
    }
    for (id, edge, cursor, open, top, left) in [
        (
            "resize-top-left",
            ResizeEdge::TopLeft,
            CursorStyle::ResizeUpLeftDownRight,
            !tiling.top && !tiling.left,
            true,
            true,
        ),
        (
            "resize-top-right",
            ResizeEdge::TopRight,
            CursorStyle::ResizeUpRightDownLeft,
            !tiling.top && !tiling.right,
            true,
            false,
        ),
        (
            "resize-bottom-left",
            ResizeEdge::BottomLeft,
            CursorStyle::ResizeUpRightDownLeft,
            !tiling.bottom && !tiling.left,
            false,
            true,
        ),
        (
            "resize-bottom-right",
            ResizeEdge::BottomRight,
            CursorStyle::ResizeUpLeftDownRight,
            !tiling.bottom && !tiling.right,
            false,
            false,
        ),
    ] {
        if open {
            let mut handle = band(id, edge, cursor).size(corner);
            handle = if top {
                handle.top_0()
            } else {
                handle.bottom_0()
            };
            handle = if left {
                handle.left_0()
            } else {
                handle.right_0()
            };
            borders = borders.child(handle);
        }
    }
    Some(borders.into_any_element())
}

/// Height of the title bar, in window pixels.
pub(super) const TITLE_BAR: f32 = 40.;

/// Size of a rem at an interface scale of 100%, in window pixels.
const BASE_REM: f32 = 16.;

impl Shell {
    // The hit area of a pane divider: wide enough to grab, with a line that lights up under
    // the pointer and while it is dragged.
    fn resize_handle(&self, id: &'static str, dragging: bool) -> Stateful<Div> {
        let t = self.theme;
        div()
            .id(id)
            .group(id)
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(RESIZE_REACH * 2. + 1.))
            .cursor_col_resize()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(RESIZE_REACH - 1.))
                    .w(px(3.))
                    .when(dragging, |line| line.bg(t.accent))
                    .group_hover(id, move |line| line.bg(t.accent.opacity(0.6))),
            )
    }
}

/// The window root: the chat background, the workspace above it, and the effect layer on top.
///
/// The workspace is a cached view. A frame that only advances an effect or the background
/// reuses the workspace's last paint instead of rendering it again.
pub struct Stage {
    backdrop: Entity<backdrop::Backdrop>,
    shell: Entity<Shell>,
    fx: Entity<fx::FxLayer>,
}

impl Stage {
    /// Creates the workspace and the effect layer of one window.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            backdrop: cx.new(backdrop::Backdrop::new),
            shell: cx.new(|cx| Shell::new(window, cx)),
            fx: cx.new(fx::FxLayer::new),
        }
    }
}

impl Render for Stage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        div()
            .relative()
            .size_full()
            .bg(theme.backdrop)
            // Cached like the workspace: the field renders on its own frames, not on every
            // frame of a scroll or of an effect.
            .child(
                AnyView::from(self.backdrop.clone()).cached(
                    StyleRefinement::default()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                ),
            )
            .child(AnyView::from(self.shell.clone()).cached(StyleRefinement::default().size_full()))
            // Dialogs and toasts of the Git workspace, which Preferences can open as well.
            .child(super::git::kit::overlays(window, cx))
            .child(self.fx.clone())
            .children(resize_borders(window))
    }
}

impl Shell {
    // The sidebar of the Git workspace: the open repository's branches, remotes, tags, stashes
    // and worktrees, at the width of the task sidebar.
    fn git_sidebar(
        &mut self,
        git: &Entity<git::GitWorkspace>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        if self.reveals.sidebar <= 0. {
            return div().into_any_element();
        }
        let content = match git.read(cx).sidebar() {
            // Cached: the sidebar renders again only when it is notified.
            Some(view) => view
                .cached(StyleRefinement::default().size_full())
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .p(px(16.))
                .child(kit::empty_hint("No repository is open", t))
                .child(
                    self.button("git-sidebar-open", "folder-open", "Open Repository")
                        .on_click(cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx))),
                )
                .into_any_element(),
        };
        div()
            .id("git-sidebar")
            .relative()
            .w(px(self.settings.sidebar_width))
            .flex_shrink_0()
            .h_full()
            .bg(t.sidebar)
            .overflow_hidden()
            .child(content)
            .into_any_element()
    }
}
