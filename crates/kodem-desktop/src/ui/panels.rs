use super::*;
use serde_json::json;
impl Shell {
    /// Builds the active workspace dialog.
    pub(super) fn panel(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let title = match self.panel {
            Panel::NewTask => "New task",
            Panel::Settings => "Settings",
            Panel::Resources => "Skills, instructions & MCP",
            Panel::Files => "Files",
            Panel::Check => "Run a project check",
            Panel::Rename => "Rename task",
            Panel::Questions => "Codex needs your input",
            Panel::None
            | Panel::Models
            | Panel::Reasoning
            | Panel::Access
            | Panel::Projects
            | Panel::TaskMenu
            | Panel::Sort
            | Panel::Comparison => "",
        };
        let mut content = div()
            .id("panel-body")
            .min_h_0()
            .max_h(px(650.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .p_4();
        match self.panel {
            Panel::NewTask => {
                content = content.child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme.muted)
                        .child("Choose where this task will work."),
                );
                let projects = self
                    .workspace
                    .as_ref()
                    .map(|w| w.projects.clone())
                    .unwrap_or_default();
                if self.selected_project.is_none() {
                    self.selected_project = projects.first().map(|p| p.id.clone());
                }
                for (i, project) in projects.iter().enumerate() {
                    let id = project.id.clone();
                    let chosen = self.selected_project.as_ref() == Some(&id);
                    content = content.child(
                        div()
                            .id(("choose-project", i))
                            .p_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(if chosen { theme.accent } else { theme.border })
                            .bg(if chosen { theme.accent_bg } else { theme.inset })
                            .cursor_pointer()
                            .child(icon("folder").text_color(theme.accent))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(project.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme.muted)
                                            .text_ellipsis()
                                            .child(project.path.display().to_string()),
                                    ),
                            )
                            .when(chosen, |d| d.child(icon("check").text_color(theme.accent)))
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.selected_project = Some(id.clone());
                                cx.notify();
                            })),
                    );
                }
                content = content
                    .child(
                        self.ghost("panel-open", "plus", "Open another repository")
                            .justify_start()
                            .on_click(
                                cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx)),
                            ),
                    )
                    .child(
                        div()
                            .mt_3()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Checkout"),
                    );
                for (i, (isolated, title, detail)) in [
                    (
                        true,
                        "New worktree",
                        "A separate branch and directory for this task.",
                    ),
                    (
                        false,
                        "Current checkout",
                        "Work on the current branch, including its uncommitted edits.",
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    content = content.child(
                        div()
                            .id(("checkout", i))
                            .p_3()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(if self.new_worktree == isolated {
                                theme.accent
                            } else {
                                theme.border
                            })
                            .cursor_pointer()
                            .flex()
                            .gap_3()
                            .child(
                                icon(if self.new_worktree == isolated {
                                    "circle-check"
                                } else {
                                    "branch"
                                })
                                .text_color(theme.muted),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(div().text_size(px(13.)).child(title))
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme.muted)
                                            .child(detail),
                                    ),
                            )
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.new_worktree = isolated;
                                cx.notify();
                            })),
                    );
                }
                content = content.child(
                    div().mt_2().flex().justify_end().child(
                        self.primary("create-task", "plus", "Create task")
                            .disabled(projects.is_empty())
                            .on_click(cx.listener(|s, _, _, _| {
                                if let Some(project_id) = &s.selected_project {
                                    s.send(core::Request::CreateTask {
                                        project_id: project_id.clone(),
                                        worktree: s.new_worktree,
                                    });
                                }
                            })),
                    ),
                );
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
                        .text_size(px(12.))
                        .text_color(theme.muted)
                        .child(icon("code"))
                        .child("Context available to this task")
                        .child(div().flex_1())
                        .child(
                            self.icon_button("refresh-resources", "refresh", "Refresh catalog")
                                .on_click(cx.listener(|s, _, _, _| {
                                    s.send(core::Request::Resources {
                                        task_id: s.selected.clone(),
                                    })
                                })),
                        ),
                );
                if self.resources.is_empty() {
                    content = content.child(div().p_4().text_size(px(12.)).text_color(theme.muted).child("No catalog entries loaded. Refresh to read the local runtime's skills and configuration."));
                }
                for (i, resource) in self.resources.clone().into_iter().enumerate() {
                    let source = resource.source.clone();
                    content = content.child(
                        div()
                            .py_3()
                            .border_b_1()
                            .border_color(theme.border)
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(30.))
                                    .rounded(px(7.))
                                    .bg(theme.raised)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon("code").text_color(theme.accent)),
                            )
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
                                            .child(
                                                div()
                                                    .px_2()
                                                    .rounded(px(4.))
                                                    .bg(theme.raised)
                                                    .text_size(px(10.))
                                                    .text_color(theme.muted)
                                                    .child(resource.scope),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme.muted)
                                            .child(resource.detail),
                                    )
                                    .child(
                                        div().text_size(px(10.)).text_color(theme.subtle).child(
                                            format!("{} · {}", resource.status, resource.source),
                                        ),
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
                                            let mut text = s.input.read(cx).text();
                                            text.push_str(&format!("\nUse the skill at {source}."));
                                            s.input.update(cx, |i, cx| i.set_text(text, cx));
                                            s.panel = Panel::None;
                                            cx.notify();
                                        },
                                    )),
                                )
                            }),
                    );
                }
            }
            Panel::Files => {
                content = content.p_0().gap_0().child(self.file_browser(cx));
            }
            Panel::Check => {
                content=content.child("Runs in this task's checkout. The transcript records the command, revision and exit status.").child(div().p_2().min_h(px(48.)).bg(theme.inset).border_1().border_color(theme.border).rounded(px(7.)).child(self.aux.clone())).child(self.primary("run-check","check-square","Run check").on_click(cx.listener(|s,_,_,cx|{if let Some(task_id)=&s.selected{s.send(core::Request::RunCheck{task_id:task_id.clone(),command:s.aux.read(cx).text()});s.panel=Panel::None;cx.notify();}})));
            }
            Panel::Rename => {
                content = content
                    .child(
                        div()
                            .p_2()
                            .min_h(px(48.))
                            .bg(theme.inset)
                            .border_1()
                            .border_color(theme.border)
                            .rounded(px(7.))
                            .child(self.aux.clone()),
                    )
                    .child(
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
                    );
            }
            Panel::Questions => {
                if let Some(view) = self.view.clone() {
                    for (i, request) in view.requests.into_iter().enumerate() {
                        content = content.child(div().font_weight(FontWeight::SEMIBOLD).child(
                            if request.method.ends_with("requestApproval") {
                                "Permission requested"
                            } else {
                                "Your input"
                            },
                        ));
                        if request.method.ends_with("requestApproval") {
                            content=content.child(request.params["reason"].as_str().unwrap_or("Codex requests permission for this operation").to_owned()).child(div().font_family("JetBrains Mono").text_size(px(12.)).child(request.params["command"].as_str().unwrap_or("File changes; inspect the corresponding file-change item in the transcript.").to_owned()));
                            let decisions = request.params["availableDecisions"]
                                .as_array()
                                .cloned()
                                .unwrap_or_else(|| {
                                    vec![json!("accept"), json!("decline"), json!("cancel")]
                                });
                            let mut row = div().flex().gap_2();
                            for (j, decision) in decisions.into_iter().enumerate() {
                                let Some(decision) = decision.as_str().map(str::to_owned) else {
                                    continue;
                                };
                                let id = request.id.clone();
                                let task_id = request.task_id.clone();
                                row = row.child(
                                    self.button(
                                        ("decision", i * 10 + j),
                                        if decision.starts_with("accept") {
                                            "shield"
                                        } else {
                                            "close"
                                        },
                                        match decision.as_str() {
                                            "accept" => "Allow once",
                                            "acceptForSession" => "Allow for session",
                                            "decline" => "Decline",
                                            "cancel" => "Cancel turn",
                                            _ => &decision,
                                        },
                                    )
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| {
                                            s.send(core::Request::Answer {
                                                task_id: task_id.clone(),
                                                id: id.clone(),
                                                response: json!({"decision":decision}),
                                            });
                                            s.panel = Panel::None;
                                            cx.notify();
                                        },
                                    )),
                                );
                            }
                            content = content.child(row);
                        } else if let Some(questions) = request.params["questions"].as_array() {
                            for (q, question) in questions.iter().enumerate() {
                                let id = question["id"].as_str().unwrap_or_default();
                                let key = format!("{}:{id}", request.id);
                                let input = self
                                    .question_inputs
                                    .entry(key)
                                    .or_insert_with(|| {
                                        cx.new(|cx| TextInput::new("Type your answer…", cx))
                                    })
                                    .clone();
                                content = content.child(
                                    question["question"]
                                        .as_str()
                                        .unwrap_or("Question")
                                        .to_owned(),
                                );
                                if let Some(options) = question["options"].as_array() {
                                    for (j, option) in options.iter().enumerate() {
                                        let label =
                                            option["label"].as_str().unwrap_or_default().to_owned();
                                        let chosen = input.read(cx).text() == label;
                                        let answer = input.clone();
                                        content =
                                            content.child(
                                                self.button(
                                                    ("answer", i * 1000 + q * 100 + j),
                                                    if chosen { "check" } else { "chat" },
                                                    label.clone(),
                                                )
                                                .on_click(cx.listener(move |_, _, _, cx| {
                                                    answer.update(cx, |input, cx| {
                                                        input.set_text(label.clone(), cx)
                                                    });
                                                    cx.notify();
                                                })),
                                            );
                                    }
                                }
                                content = content.child(input);
                            }
                            let id = request.id.clone();
                            let task_id = request.task_id.clone();
                            let keys: Vec<String> = questions
                                .iter()
                                .filter_map(|q| q["id"].as_str().map(str::to_owned))
                                .collect();
                            content = content.child(
                                self.primary(("submit-answers", i), "check", "Submit answers")
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        let mut answers = serde_json::Map::new();
                                        for key in &keys {
                                            if let Some(input) =
                                                s.question_inputs.get(&format!("{id}:{key}"))
                                            {
                                                let answer = input.read(cx).text();
                                                if !answer.trim().is_empty() {
                                                    answers.insert(
                                                        key.clone(),
                                                        json!({"answers":[answer]}),
                                                    );
                                                }
                                            }
                                        }
                                        s.send(core::Request::Answer {
                                            task_id: task_id.clone(),
                                            id: id.clone(),
                                            response: json!({"answers":answers}),
                                        });
                                        s.panel = Panel::None;
                                        cx.notify();
                                    })),
                            );
                        }
                    }
                }
            }
            Panel::None
            | Panel::Models
            | Panel::Reasoning
            | Panel::Access
            | Panel::Projects
            | Panel::TaskMenu
            | Panel::Sort
            | Panel::Comparison => {}
        }
        div()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000090))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(if self.panel == Panel::Files {
                        1000.
                    } else if self.panel == Panel::Settings {
                        760.
                    } else {
                        620.
                    }))
                    .max_w_full()
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .bg(theme.canvas)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(12.))
                    .shadow(theme.elevation())
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(48.))
                            .flex_shrink_0()
                            .px_4()
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                self.icon_button("close-panel", "close", "Close · Esc")
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.panel = Panel::None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(content)
                    .with_animation(
                        ("dialog-enter", self.panel.clone() as usize),
                        Animation::new(Duration::from_millis(170)),
                        |d, p| d.opacity(p).mt(px((1. - p) * 8.)),
                    ),
            )
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
            .font_family("JetBrains Mono")
            .text_size(px(12.))
            .line_height(px(17.))
            .p_2()
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
                    .h(px(36.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .child(
                        icon("square-terminal")
                            .size(px(14.))
                            .text_color(theme.muted),
                    )
                    .child(if running {
                        "Terminal · running"
                    } else {
                        "Terminal"
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
