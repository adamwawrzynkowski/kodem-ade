use super::*;
impl Shell {
    fn titlebar(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .when(self.reveals.sidebar > 0., |d| {
                d.child(
                    div()
                        // The section ends above the sidebar's trailing edge. While the sidebar
                        // slides, it shrinks toward the room of the window controls.
                        .w(px(WINDOW_CONTROLS
                            + (workspaces::RAIL + 1. + self.settings.sidebar_width
                                - WINDOW_CONTROLS)
                                * self.reveals.sidebar))
                        .overflow_hidden()
                        .h_full()
                        .flex_shrink_0()
                        .pl(px(WINDOW_CONTROLS))
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
                                        .font_family(crate::theme::MONO)
                                        .text_size(px(12.5))
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
                d.pl(px(WINDOW_CONTROLS)).child(
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
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_ellipsis()
                    .child(name),
            );
        if let Some(view) = self.view.as_ref().filter(|_| self.mode.has_project()) {
            let review = view.task.kind == TaskKind::Review;
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
        if let Some(id) = self.feed_tasks(cx).get(index) {
            self.select_task(id.clone(), cx);
        }
    }
}
impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window_width = f32::from(window.viewport_size().width);
        backdrop::begin_frame(window, cx);
        self.sync_reveals(cx);
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
            .font_family(crate::theme::SANS)
            .text_size(px(self.settings.font_size - 1.))
            .line_height(relative(1.5))
            .font_weight(FontWeight::NORMAL)
            .text_color(t.text)
            // No fill: the window root paints the backdrop color, and the chat surface with its
            // dot field lies beneath this view.
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
            .on_action(cx.listener(|s, _: &ShowCoding, _, cx| s.set_mode(TaskKind::Coding, cx)))
            .on_action(cx.listener(|s, _: &ShowChat, _, cx| s.set_mode(TaskKind::Chat, cx)))
            .on_action(cx.listener(|s, _: &ShowReview, _, cx| s.set_mode(TaskKind::Review, cx)))
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
                    s.settings.sidebar_width =
                        (f32::from(event.position.x) - workspaces::RAIL - 1.).clamp(220., 420.);
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
        let rail = self.workspace_rail(cx);
        let sidebar = self.sidebar(cx);
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
        let narrow_review = self.show_review
            && self.has_checkout()
            && self.window_width <= 1100.
            && !(self.reviewing_file
                && self.settings.diff_placement == DiffPlacement::Conversation);
        // The content of a workspace that just opened clears out of a dissolve.
        let chat = motion::switched(
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
        );
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
        } else if review_open > 0. && self.has_checkout() && self.window_width > 1100. {
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

/// Width at the leading end of the title bar that the window controls take, in points.
const WINDOW_CONTROLS: f32 = 80.;

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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx
            .try_global::<Theme>()
            .copied()
            .unwrap_or(Theme::new(false));
        div()
            .relative()
            .size_full()
            .bg(theme.backdrop)
            // Cached like the workspace: the field renders on its own steps, not on every
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
            .child(self.fx.clone())
    }
}
