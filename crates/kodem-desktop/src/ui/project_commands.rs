//! Project commands: their tree in the project dialog, where a drag reorders commands and
//! steps, and the menu in the chat header that runs one in the task's terminal.
use super::*;
use std::path::Path;

/// What a drag in the command tree carries: a whole command, or one step of a command.
#[derive(Clone)]
pub(super) struct CommandDrag {
    command: usize,
    step: Option<usize>,
    label: SharedString,
    glyph: &'static str,
    theme: Theme,
}

impl Render for CommandDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(26.))
            .px(px(9.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .bg(t.raised)
            .border_1()
            .border_color(t.accent.opacity(0.6))
            .shadow(t.floating())
            .font_family(crate::theme::mono())
            .text_size(text_px(11.5))
            .text_color(t.text)
            .child(icon(self.glyph).size(px(13.)).text_color(t.accent))
            .child(self.label.clone())
    }
}

// Returns the glyph of a step by its kind: an app, a script or another file.
fn step_glyph(step: &Path) -> &'static str {
    let extension = step
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    match extension.as_deref() {
        Some("app" | "exe") => "cube",
        Some("command" | "sh" | "bash" | "zsh" | "cmd" | "bat" | "ps1") => "square-terminal",
        _ => "file",
    }
}

// Returns the field that names a command in the project dialog.
fn name_input(name: &str, cx: &mut Context<Shell>) -> Entity<TextInput> {
    let name = name.to_owned();
    cx.new(|cx| {
        let mut input = TextInput::new("Name, or the first step's file name", cx);
        input.set_text(name, cx);
        input
    })
}

impl Shell {
    /// Opens the project dialog with the project's name and command names in their fields.
    pub(super) fn open_project_editor(&mut self, project: Project, cx: &mut Context<Self>) {
        self.aux
            .update(cx, |input, cx| input.set_text(project.name.clone(), cx));
        self.icon_filter
            .update(cx, |input, cx| input.set_text(String::new(), cx));
        self.command_names = project
            .commands
            .iter()
            .map(|command| (command.id.clone(), name_input(&command.name, cx)))
            .collect();
        self.editing_project = Some(project);
        self.error = None;
        self.panel = Panel::CustomizeProject;
        cx.notify();
    }

    /// Returns the edited commands with the names in their fields.
    pub(super) fn edited_commands(&self, project: &Project, cx: &App) -> Vec<ProjectCommand> {
        project
            .commands
            .iter()
            .cloned()
            .map(|mut command| {
                if let Some(input) = self.command_names.get(&command.id) {
                    command.name = input.read(cx).text().trim().to_owned();
                }
                command
            })
            .collect()
    }

    /// Builds the command tree of the project dialog: each command over the branch of its steps.
    pub(super) fn command_editor(&self, project: &Project, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut tree = div().flex().flex_col().gap(px(8.));
        for (c, command) in project.commands.iter().enumerate() {
            tree = tree.child(self.command_card(c, command, cx));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().mt(px(6.)).child(kit::section_header(
                "Commands",
                Some(project.commands.len()),
                t,
            )))
            .child(
                div()
                    .text_size(text_px(12.))
                    .text_color(t.muted)
                    .child("Scripts and apps that run in the task's terminal from the chat header. Steps run in order, and each starts when the one before it succeeds. Each step sees KODEM_NONINTERACTIVE=1, so a script can end without a closing prompt. Drag a command or a step to move it."),
            )
            .child(tree)
            .child(
                div().flex().child(
                    self.button("project-command-add", "plus", "Add command…")
                        .disabled(project.commands.len() >= 64)
                        .on_click(cx.listener(|s, _, _, cx| s.add_project_command(cx))),
                ),
            )
            .into_any_element()
    }

    // Returns one command: its name field over the branch of its steps. The grip drags the
    // whole command; the header takes a dropped command, or a dropped step at the end.
    fn command_card(
        &self,
        c: usize,
        command: &ProjectCommand,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = self.theme;
        let drag = CommandDrag {
            command: c,
            step: None,
            label: command.label().into(),
            glyph: "play",
            theme: t,
        };
        let mut header = div()
            .id("command-header")
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .drag_over::<CommandDrag>(move |style, _, _, _| style.bg(t.accent.opacity(0.1)))
            .on_drop(cx.listener(move |s, drag: &CommandDrag, _, cx| {
                s.drop_command_item(drag, c, None, cx)
            }))
            .child(
                div()
                    .id("command-grip")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(22.))
                    .cursor_grab()
                    .text_color(t.subtle)
                    .hover(move |grip| grip.text_color(t.text))
                    .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .child(icon("chevrons-up-down").size(px(12.))),
            );
        if let Some(input) = self.command_names.get(&command.id) {
            header = header.child(
                settings::input_frame(t)
                    .flex_1()
                    .min_w_0()
                    .h(px(26.))
                    .border_color(t.border)
                    .child(icon("play").size(px(12.)).text_color(t.subtle))
                    .child(div().flex_1().min_w_0().child(input.clone())),
            );
        }
        let id = command.id.clone();
        header = header.child(
            self.icon_button("command-remove", "trash-2", "Remove command")
                .on_click(cx.listener(move |s, _, _, cx| {
                    if let Some(project) = &mut s.editing_project {
                        project.commands.retain(|command| command.id != id);
                    }
                    s.command_names.remove(&id);
                    cx.notify();
                })),
        );
        // The steps hang from a dotted guide, as the branches of the settings navigation do.
        let mut branch = div()
            .relative()
            .ml(px(10.))
            .pl(px(13.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(2.))
                    .bottom(px(2.))
                    .child(dither::DottedRule::vertical(t.subtle.opacity(0.45))),
            );
        for (s, step) in command.steps.iter().enumerate() {
            branch = branch.child(self.step_tile(c, s, step, cx));
        }
        branch = branch.child(
            div().flex().child(
                self.ghost("command-add-step", "plus", "Add step…")
                    .disabled(command.steps.len() >= ProjectCommand::MAX_STEPS)
                    .on_click(cx.listener(move |s, _, _, cx| s.choose_command_steps(c, cx))),
            ),
        );
        div()
            .id(("project-command", c))
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(8.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .bg(t.inset)
            .shadow(kit::recessed_light(t))
            .border_1()
            .border_color(t.border)
            .child(header)
            .child(branch)
    }

    // Returns one step as a tile that drags to another place in any command.
    fn step_tile(
        &self,
        c: usize,
        s: usize,
        step: &Path,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = self.theme;
        let glyph = step_glyph(step);
        let label = step_label(step);
        let drag = CommandDrag {
            command: c,
            step: Some(s),
            label: label.clone().into(),
            glyph,
            theme: t,
        };
        div()
            .id(("command-step", s))
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(30.))
            .pl(px(8.))
            .pr(px(2.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            // A step stands in the recessed command like a key, so it reads as something to drag.
            .bg(kit::raised_fill(t.raised))
            .shadow(kit::raised_light(t))
            .border_1()
            .border_color(t.border)
            .cursor_grab()
            .hover(move |tile| tile.border_color(t.edge))
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .drag_over::<CommandDrag>(move |style, _, _, _| {
                style
                    .border_color(t.accent.opacity(0.7))
                    .bg(t.accent.opacity(0.1))
            })
            .on_drop(cx.listener(move |shell, drag: &CommandDrag, _, cx| {
                shell.drop_command_item(drag, c, Some(s), cx)
            }))
            .child(figure((s + 1).to_string(), t.subtle))
            .child(icon(glyph).size(px(13.)).text_color(t.muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(text_px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(10.5))
                            .text_color(t.subtle)
                            .text_ellipsis_start()
                            .child(step.display().to_string()),
                    ),
            )
            .child(
                self.icon_button("step-remove", "close", "Remove step")
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        if let Some(steps) = shell
                            .editing_project
                            .as_mut()
                            .and_then(|project| project.commands.get_mut(c))
                            .map(|command| &mut command.steps)
                            .filter(|steps| s < steps.len())
                        {
                            steps.remove(s);
                        }
                        cx.notify();
                    })),
            )
    }

    // Moves a dragged command before the command at `command`, or a dragged step to `step` of
    // that command, or to its end when `step` is absent.
    fn drop_command_item(
        &mut self,
        drag: &CommandDrag,
        command: usize,
        step: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let Some(commands) = self
            .editing_project
            .as_mut()
            .map(|project| &mut project.commands)
        else {
            return;
        };
        if command >= commands.len() {
            return;
        }
        match drag.step {
            None => {
                if drag.command < commands.len() && drag.command != command {
                    let moved = commands.remove(drag.command);
                    commands.insert(command, moved);
                }
            }
            Some(from) => {
                let Some(moved) = commands
                    .get_mut(drag.command)
                    .filter(|source| from < source.steps.len())
                    .map(|source| source.steps.remove(from))
                else {
                    return;
                };
                let steps = &mut commands[command].steps;
                let at = step.unwrap_or(steps.len()).min(steps.len());
                steps.insert(at, moved);
            }
        }
        cx.notify();
    }

    // Adds an empty command and asks for its first steps.
    fn add_project_command(&mut self, cx: &mut Context<Self>) {
        let Some(project) = &mut self.editing_project else {
            return;
        };
        let id = new_id();
        project.commands.push(ProjectCommand {
            id: id.clone(),
            name: String::new(),
            steps: Vec::new(),
        });
        let index = project.commands.len() - 1;
        let input = name_input("", cx);
        self.command_names.insert(id, input);
        self.choose_command_steps(index, cx);
        cx.notify();
    }

    // Asks for scripts or apps and adds them to the end of a command's steps. A file inside the
    // project is kept relative to its root.
    fn choose_command_steps(&mut self, command: usize, cx: &mut Context<Self>) {
        let Some(project_id) = self
            .editing_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            return;
        };
        let selected = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add step".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = selected.await {
                let _ = this.update(cx, |s, cx| {
                    if s.panel != Panel::CustomizeProject {
                        return;
                    }
                    let Some(project) = s
                        .editing_project
                        .as_mut()
                        .filter(|project| project.id == project_id)
                    else {
                        return;
                    };
                    let root = &project.path;
                    if let Some(target) = project.commands.get_mut(command) {
                        let room = ProjectCommand::MAX_STEPS.saturating_sub(target.steps.len());
                        target
                            .steps
                            .extend(paths.into_iter().take(room).map(|path| {
                                path.strip_prefix(root)
                                    .map_or_else(|_| path.clone(), Path::to_path_buf)
                            }));
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    /// Returns the project of the open task.
    pub(super) fn task_project(&self) -> Option<&Project> {
        let task = &self.view.as_ref()?.task;
        self.workspace
            .as_ref()?
            .projects
            .iter()
            .find(|project| project.id == task.project_id)
    }

    /// Builds the menu of the open task's project commands, each with its run button.
    pub(super) fn command_menu(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let project = self.task_project().cloned();
        let commands = project
            .as_ref()
            .map(|project| project.commands.clone())
            .unwrap_or_default();
        let mut menu = div().flex().flex_col();
        if commands.is_empty() {
            menu = menu.child(div().px(px(8.)).py(px(4.)).child(kit::empty_hint(
                "No commands yet. Add scripts or apps to this project to run them here.",
                t,
            )));
        }
        for (i, command) in commands.into_iter().enumerate() {
            let steps = command
                .steps
                .iter()
                .map(|step| step_label(step))
                .collect::<Vec<_>>()
                .join(" → ");
            let id = command.id.clone();
            menu = menu.child(
                div()
                    .id(("project-command-row", i))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(8.))
                    .pr(px(2.))
                    .py(px(3.))
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .hover(move |row| row.bg(t.selected))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(text_px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_ellipsis()
                                    .child(command.label()),
                            )
                            .child(
                                div()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(10.5))
                                    .text_color(t.subtle)
                                    .text_ellipsis()
                                    .child(steps),
                            ),
                    )
                    .child(
                        self.icon_button(("run-project-command", i), "play", "Run in terminal")
                            .text_color(t.accent)
                            .on_click(cx.listener(move |s, _, window, cx| {
                                s.run_project_command(id.clone(), window, cx)
                            })),
                    ),
            );
        }
        if let Some(project) = project {
            menu = menu.child(menus::separator(t)).child(
                self.ghost("edit-project-commands", "settings-2", "Edit commands…")
                    .w_full()
                    .justify_start()
                    .on_click(
                        cx.listener(move |s, _, _, cx| s.open_project_editor(project.clone(), cx)),
                    ),
            );
        }
        menu
    }

    // Runs a command in the open task's terminal and shows the terminal.
    fn run_project_command(&mut self, command_id: String, window: &Window, cx: &mut Context<Self>) {
        let Some(task_id) = self.selected.clone() else {
            return;
        };
        self.send(core::Request::RunProjectCommand {
            task_id,
            command_id,
            rows: self.terminal_rows(window),
            cols: self.terminal_columns(window),
        });
        self.show_terminal = true;
        self.save_settings();
        self.panel = Panel::None;
        cx.notify();
    }
}
