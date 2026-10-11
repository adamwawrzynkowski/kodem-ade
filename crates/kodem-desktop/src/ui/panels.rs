use super::*;

// Returns the dotted rule that separates the content of a dialog from its actions.
fn footer_rule(theme: Theme) -> Div {
    div()
        .mt(px(2.))
        .child(dither::DottedRule::horizontal(theme.subtle.opacity(0.4)))
}

// Returns the action row of a dialog. Its buttons align to the trailing edge.
fn footer_actions() -> Div {
    div().flex().items_center().justify_end().gap(px(6.))
}

impl Shell {
    /// Builds the active workspace dialog.
    pub(super) fn panel(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let title = match self.panel {
            Panel::CustomizeProject => "Customize project",
            Panel::RemoveProject => "Remove project?",
            Panel::Settings => "Settings",
            Panel::Resources => "Skills, instructions & MCP",
            Panel::Files => "Files",
            Panel::Check => "Run a project check",
            Panel::Rename => "Rename task",
            Panel::DeleteTask => "Delete thread?",
            Panel::None
            | Panel::Usage
            | Panel::Models
            | Panel::Reasoning
            | Panel::Access
            | Panel::Projects
            | Panel::ChatProjects
            | Panel::TaskMenu
            | Panel::ThreadMenu
            | Panel::Sort
            | Panel::Palette
            | Panel::Comparison => "",
        };
        let mut content = div()
            .id("panel-body")
            .min_h_0()
            .max_h(px(650.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(18.));
        match self.panel {
            Panel::DeleteTask => {
                if let Some(task) = self
                    .thread_menu
                    .as_ref()
                    .and_then(|context| {
                        self.workspace
                            .as_ref()?
                            .tasks
                            .iter()
                            .find(|task| task.id == context.task_id)
                    })
                    .cloned()
                {
                    content = content
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(task.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .line_height(relative(1.45))
                                .text_color(theme.muted)
                                .child("Delete this thread and its local conversation history? This cannot be undone. Project files and the worktree stay on disk."),
                        );
                    if let Some(error) = &self.error {
                        content =
                            content.child(settings::error_block("Error", error.clone(), theme));
                    }
                    let id = task.id.clone();
                    let deleting = self.deleting_task.as_ref() == Some(&id);
                    content = content.child(footer_rule(theme)).child(
                        footer_actions()
                            .child(
                                self.ghost("cancel-delete-thread", "close", "Cancel")
                                    .disabled(deleting)
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.panel = Panel::None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                self.tinted(
                                    "confirm-delete-thread",
                                    "trash-2",
                                    if deleting {
                                        "Deleting…"
                                    } else {
                                        "Delete thread"
                                    },
                                    theme.removed,
                                )
                                .disabled(deleting)
                                .on_click(cx.listener(
                                    move |s, _, _, cx| {
                                        if s.deleting_task.is_some() {
                                            return;
                                        }
                                        s.deleting_task = Some(id.clone());
                                        s.send(core::Request::DeleteTask {
                                            task_id: id.clone(),
                                        });
                                        cx.notify();
                                    },
                                )),
                            ),
                    );
                }
            }
            Panel::CustomizeProject => {
                content = content.child(self.project_editor(cx));
            }
            Panel::RemoveProject => {
                if let Some(project) = self.editing_project.clone() {
                    content = content
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(project.name),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .line_height(relative(1.45))
                                .text_color(theme.muted)
                                .child("Remove this project from Kodem? Files and chat history are kept. Open the repository again to restore it."),
                        )
                        .when_some(self.error.clone(), |d, error| {
                            d.child(settings::error_block("Error", error, theme))
                        })
                        .child(footer_rule(theme))
                        .child(
                            footer_actions().child(
                                self.tinted(
                                    "remove-project-confirm",
                                    "trash-2",
                                    "Remove project",
                                    theme.removed,
                                )
                                .on_click(cx.listener(move |s, _, _, _| {
                                    s.send(core::Request::RemoveProject {
                                        project_id: project.id.clone(),
                                    })
                                })),
                            ),
                        );
                }
            }
            Panel::Settings => {
                content = content.p_0().gap_0().child(self.settings_panel(cx));
            }
            Panel::Resources => {
                content = content.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(kit::section_header(
                            "Context available to this task",
                            Some(self.resources.len()),
                            theme,
                        )))
                        .child(
                            self.icon_button("refresh-resources", "refresh", "Refresh catalog")
                                .on_click(cx.listener(|s, _, _, _| {
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
                                    })
                                })),
                        ),
                );
                if self.resources.is_empty() {
                    content = content.child(kit::empty_hint(
                        "No catalog entries loaded. Refresh to read the local runtime's skills and configuration.",
                        theme,
                    ));
                }
                for (i, resource) in self.resources.clone().into_iter().enumerate() {
                    let source = resource.source.clone();
                    content = content.child(
                        div()
                            .flex()
                            .gap_3()
                            .child(kit::icon_tile("code", theme.accent, 28., false))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(resource.name),
                                            )
                                            .child(kit::chip(
                                                resource.scope.to_uppercase(),
                                                theme.muted,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .line_height(relative(1.4))
                                            .text_color(theme.muted)
                                            .child(resource.detail),
                                    )
                                    .child(
                                        div()
                                            .font_family(crate::theme::MONO)
                                            .text_size(px(10.5))
                                            .text_color(theme.subtle)
                                            .text_ellipsis_start()
                                            .child(format!(
                                                "{} · {}",
                                                resource.status, resource.source
                                            )),
                                    ),
                            )
                            .when(source.ends_with("SKILL.md"), |d| {
                                d.child(
                                    self.icon_button(
                                        ("add-skill", i),
                                        "plus",
                                        "Add skill reference to message",
                                    )
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| {
                                            s.attach_skill(source.clone().into(), cx);
                                            s.panel = Panel::None;
                                            cx.notify();
                                        },
                                    )),
                                )
                            }),
                    );
                    content =
                        content.child(dither::DottedRule::horizontal(theme.subtle.opacity(0.4)));
                }
            }
            Panel::Files => {
                content = content.p_0().gap_0().child(self.file_browser(cx));
            }
            Panel::Check => {
                content = content
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(relative(1.45))
                            .text_color(theme.muted)
                            .child("Runs in this task's checkout. The transcript records the command, revision and exit status."),
                    )
                    .child(
                        settings::input_frame(theme)
                            .h_auto()
                            .min_h(px(44.))
                            .py(px(8.))
                            .items_start()
                            .font_family(crate::theme::MONO)
                            .child(div().text_color(theme.accent).child("$"))
                            .child(div().flex_1().min_w_0().child(self.aux.clone())),
                    )
                    .child(footer_rule(theme))
                    .child(
                        footer_actions().child(
                            self.primary("run-check", "check-square", "Run check").on_click(
                                cx.listener(|s, _, _, cx| {
                                    if let Some(task_id) = &s.selected {
                                        s.send(core::Request::RunCheck {
                                            task_id: task_id.clone(),
                                            command: s.aux.read(cx).text(),
                                        });
                                        s.panel = Panel::None;
                                        cx.notify();
                                    }
                                }),
                            ),
                        ),
                    );
            }
            Panel::Rename => {
                content = content
                    .child(kit::section_header("Title", None, theme))
                    .child(
                        settings::input_frame(theme)
                            .child(div().flex_1().min_w_0().child(self.aux.clone())),
                    )
                    .child(footer_rule(theme))
                    .child(
                        footer_actions().child(
                            self.primary("rename", "check", "Save title")
                                .on_click(cx.listener(|s, _, _, cx| {
                                    if let Some(task_id) = &s.selected {
                                        s.send(core::Request::EditTask {
                                            task_id: task_id.clone(),
                                            edit: TaskEdit::Rename(s.aux.read(cx).text()),
                                        });
                                        s.panel = Panel::None;
                                        cx.notify();
                                    }
                                })),
                        ),
                    );
            }
            Panel::None
            | Panel::Usage
            | Panel::Models
            | Panel::Reasoning
            | Panel::Access
            | Panel::Projects
            | Panel::ChatProjects
            | Panel::TaskMenu
            | Panel::ThreadMenu
            | Panel::Sort
            | Panel::Palette
            | Panel::Comparison => {}
        }
        let width = if self.panel == Panel::Files {
            1000.
        } else if self.panel == Panel::Settings {
            780.
        } else {
            600.
        };
        let danger = matches!(self.panel, Panel::DeleteTask | Panel::RemoveProject);
        let tone = if danger { theme.removed } else { theme.accent };
        let presentation = match self.panel {
            Panel::CustomizeProject => Some(("palette", "Name, accent and icon")),
            Panel::RemoveProject => Some(("trash-2", "Files and local history stay on disk")),
            Panel::Resources => Some(("code", "What the provider can use in this task")),
            Panel::Check => Some((
                "square-terminal",
                "Runs in the task's checkout and is recorded in the conversation",
            )),
            Panel::Rename => Some(("pencil", "A manual title replaces the generated one")),
            Panel::DeleteTask => Some(("trash-2", "Removes this task's local conversation")),
            _ => None,
        };
        let close = self
            .icon_button("close-panel", "close", "Close · Esc")
            .on_click(cx.listener(|s, _, _, cx| {
                s.panel = Panel::None;
                cx.notify();
            }));
        // Focused dialogs open under a glow header; the two workspaces keep a compact title bar.
        let header = match presentation {
            Some((glyph, detail)) => div()
                .relative()
                .flex_shrink_0()
                .border_b_1()
                .border_color(theme.border)
                .child(kit::glow_header(glyph, title, detail, tone, theme))
                .child(
                    div()
                        .absolute()
                        .top(px(8.))
                        .right(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(kit::keycap("esc", theme))
                        .child(close),
                ),
            None => div()
                .h(px(40.))
                .flex_shrink_0()
                .pl(px(16.))
                .pr(px(8.))
                .flex()
                .items_center()
                .gap(px(10.))
                .bg(theme.sidebar)
                .border_b_1()
                .border_color(theme.border)
                .child(kit::marker(tone))
                .child(
                    div()
                        .flex_1()
                        .font_family(crate::theme::MONO)
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.to_uppercase()),
                )
                .child(kit::keycap("esc", theme))
                .child(close),
        };
        let dialog = div()
            .relative()
            .w(px(width))
            .max_w_full()
            .max_h_full()
            .flex()
            .flex_col()
            .child(dither::shadow(theme, crate::theme::RADIUS_FLOATING, 7.))
            .child(
                div()
                    .w_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .bg(theme.canvas)
                    .border_1()
                    .border_color(theme.edge)
                    .rounded(px(crate::theme::RADIUS_FLOATING))
                    .shadow(theme.floating())
                    .overflow_hidden()
                    .child(header)
                    .child(content),
            );
        div()
            .absolute()
            .inset_0()
            // Dialogs must block clicks, hover, and scroll in the workspace behind them.
            .occlude()
            .child(dither::scrim(theme))
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.))
            .child(motion::floating(
                ("dialog-enter", self.panel.clone() as usize),
                dialog,
                theme,
            ))
            .into_any_element()
    }
    /// Builds the task terminal surface and forwards explicit keyboard input.
    pub(super) fn terminal_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let focus = self.terminal_focus.clone();
        let selected = self.selected.clone();
        let tx = self.tx.clone();
        let text = self
            .terminal
            .as_ref()
            .map(|t| t.text.clone())
            .unwrap_or_else(|| "Starting a shell in the task checkout…".into());
        let running = self.terminal.as_ref().is_some_and(|t| t.running);
        let rows: Vec<_> = text.lines().map(str::to_owned).collect();
        let content = div()
            .id("terminal-input")
            .track_focus(&focus)
            .key_context("Terminal")
            .h(px(230.))
            .w_full()
            .overflow_hidden()
            .font_family(crate::theme::MONO)
            .text_size(px(12.))
            .line_height(px(17.))
            .px(px(14.))
            .py(px(8.))
            .on_mouse_down(MouseButton::Left, move |_, w, cx| w.focus(&focus, cx))
            .on_key_down(move |event, w, cx| {
                let Some(task_id) = &selected else { return };
                let key = &event.keystroke;
                let bytes = if key.modifiers.platform && key.key == "v" {
                    cx.read_from_clipboard()
                        .and_then(|c| c.text())
                        .map(|s| s.into_bytes())
                } else if key.modifiers.control && key.key.len() == 1 {
                    Some(vec![key.key.as_bytes()[0] & 0x1f])
                } else {
                    match key.key.as_str() {
                        "enter" => Some(vec![b'\r']),
                        "backspace" => Some(vec![127]),
                        "tab" => Some(vec![9]),
                        "escape" => Some(vec![27]),
                        "up" => Some(b"\x1b[A".to_vec()),
                        "down" => Some(b"\x1b[B".to_vec()),
                        "right" => Some(b"\x1b[C".to_vec()),
                        "left" => Some(b"\x1b[D".to_vec()),
                        _ => key.key_char.as_ref().map(|s| s.as_bytes().to_vec()),
                    }
                };
                if let Some(data) = bytes {
                    let _ = tx.send(bridge::Command::Request(core::Request::TerminalInput {
                        task_id: task_id.clone(),
                        data,
                    }));
                    cx.stop_propagation();
                    w.prevent_default();
                }
            })
            .children(rows.into_iter().map(|line| div().h(px(17.)).child(line)));
        let cols = ((f32::from(window.viewport_size().width)
            - self.settings.sidebar_width
            - if self.show_review {
                self.settings.review_width
            } else {
                0.
            })
            / 7.2)
            .clamp(20., 400.) as u16;
        div()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.inset)
            .child(
                div()
                    .h(px(32.))
                    .pl(px(14.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .bg(theme.sidebar)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(caps("Terminal", theme))
                    .when(running, |d| {
                        d.child(kit::marker(theme.added))
                            .child(caps("Running", theme).text_color(theme.added))
                    })
                    .child(div().flex_1())
                    .child(
                        self.icon_button("resize-terminal", "panel-bottom", "Fit terminal to pane")
                            .on_click(cx.listener(move |s, _, _, _| {
                                if let Some(task_id) = &s.selected {
                                    s.send(core::Request::TerminalResize {
                                        task_id: task_id.clone(),
                                        rows: 12,
                                        cols,
                                    });
                                }
                            })),
                    )
                    .child(
                        self.icon_button("copy-terminal", "copy", "Copy terminal output")
                            .on_click(cx.listener(|s, _, _, cx| {
                                if let Some(t) = &s.terminal {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        t.text.clone(),
                                    ));
                                }
                            })),
                    )
                    .child(
                        self.icon_button("close-terminal", "close", "Hide terminal")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.show_terminal = false;
                                s.save_settings();
                                cx.notify();
                            })),
                    ),
            )
            .child(content)
            .with_animation(
                "terminal-enter",
                Animation::new(Duration::from_millis(150)),
                |d, p| d.opacity(p),
            )
            .into_any_element()
    }
}
