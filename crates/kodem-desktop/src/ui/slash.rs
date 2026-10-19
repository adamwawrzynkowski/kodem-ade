//! Completions anchored to the native prompt editor: runtime-backed slash commands and skills,
//! and `@` mentions of files in the task's checkout.
use super::*;
use crate::components::text_input::CompletionInput;
use std::{ops::Range, time::Instant};

#[derive(Clone, PartialEq, Eq)]
enum Choice {
    Command(ComposerCommand),
    ProviderCommand(ProviderCommand),
    Skill(ComposerSkill),
    /// A file of the task's checkout, inserted as an `@` mention.
    Path(String),
    Attach,
    Files,
    Diff,
    Retry,
}
impl Choice {
    fn labels(&self) -> (&str, &str, &'static str, &'static str) {
        match self {
            Self::ProviderCommand(command) => (
                &command.name,
                &command.description,
                "code",
                "Provider command",
            ),
            Self::Command(command) => match command {
                ComposerCommand::Provider => (
                    "provider",
                    "Choose a provider and model",
                    "settings-2",
                    "Provider",
                ),
                ComposerCommand::New => ("new", "Start a new conversation", "plus", "Provider"),
                ComposerCommand::Model => (
                    "model",
                    "Choose model and reasoning effort",
                    "bot",
                    "Provider",
                ),
                ComposerCommand::Permissions => (
                    "permissions",
                    "Choose access and approval mode",
                    "shield",
                    "Provider",
                ),
                ComposerCommand::Skills => (
                    "skills",
                    "Browse skills, instructions and MCP",
                    "code",
                    "Provider",
                ),
                ComposerCommand::Mcp => {
                    ("mcp", "Inspect connected MCP servers", "code", "Provider")
                }
                ComposerCommand::Compact => (
                    "compact",
                    "Summarize the conversation to free context",
                    "layers",
                    "Provider",
                ),
                ComposerCommand::Status => (
                    "status",
                    "View runtime and account status",
                    "bot",
                    "Provider",
                ),
                ComposerCommand::Undo => (
                    "undo",
                    "Restore the files to before the last turn",
                    "undo-2",
                    "Provider",
                ),
                ComposerCommand::Fork => (
                    "fork",
                    "Continue from here in a new task with its own worktree",
                    "git-pull-request",
                    "Provider",
                ),
            },
            Self::Skill(skill) => (&skill.name, &skill.description, "code", "Skill"),
            Self::Path(path) => (
                path.rsplit('/').next().unwrap_or(path),
                path,
                "file-text",
                "File",
            ),
            Self::Attach => (
                "attach",
                "Add files, folders or images",
                "paperclip",
                "Context",
            ),
            Self::Files => ("mention", "Choose a project file", "file", "Context"),
            Self::Diff => ("diff", "Open the current changes", "file-text", "Context"),
            Self::Retry => ("retry", "Reload commands and skills", "refresh", "Catalog"),
        }
    }
}

#[derive(PartialEq, Eq)]
struct CatalogContext {
    task_id: Option<String>,
    project_id: Option<String>,
    provider: Provider,
    account: Option<String>,
    model: Option<String>,
}

#[derive(Default)]
pub(super) struct SlashPicker {
    catalog: ComposerCatalog,
    context: Option<CatalogContext>,
    loading: bool,
    request_id: Option<String>,
    token: Option<(Range<usize>, String)>,
    /// The token is an `@` file mention, not a slash command.
    mention: bool,
    files_requested: bool,
    files_loading: bool,
    dismissed: Option<(Range<usize>, String)>,
    choices: Vec<Choice>,
    selected: usize,
    selection_touched: bool,
    scroll: ScrollHandle,
    opened: Option<Instant>,
    closing: Option<Instant>,
}
impl SlashPicker {
    /// Reports whether the menu shows and is not closing.
    pub(super) fn open(&self) -> bool {
        self.opened.is_some() && self.closing.is_none()
    }
}
impl Shell {
    fn catalog_context(&self) -> CatalogContext {
        CatalogContext {
            task_id: self.selected.clone(),
            // A new chat has no project; its catalog comes from the user's own configuration.
            project_id: self.composer_project().map(|project| project.id.clone()),
            provider: self.composer_preferences().provider,
            account: self.composer_preferences().account,
            model: self.composer_preferences().model,
        }
    }

    /// Synchronizes completions with the caret without treating paths or IME text as commands.
    pub(super) fn sync_slash(&mut self, cx: &mut Context<Self>) {
        let slash = self.input.read(cx).slash_query();
        // Mentions list the open task's files, so they need a task with a checkout.
        let mention = slash.is_none() && self.view.is_some() && self.has_checkout();
        let token = slash.or_else(|| {
            mention
                .then(|| self.input.read(cx).mention_query())
                .flatten()
        });
        let mention = mention && token.is_some();
        if self.panel != Panel::None || token.is_none() || token == self.slash.dismissed {
            self.close_slash(cx);
            if token.is_none() {
                self.slash.dismissed = None;
            }
            return;
        }
        if mention != self.slash.mention {
            self.slash.mention = mention;
            self.slash.token = None;
            self.slash.choices.clear();
        }
        if mention {
            // One request per task; an empty checkout must not ask again on every frame.
            if self.files.is_empty()
                && !self.slash.files_requested
                && let Some(task_id) = self.selected.clone()
            {
                self.slash.files_requested = true;
                self.slash.files_loading = true;
                self.send(core::Request::Files { task_id });
            }
        } else if self.slash.context.as_ref() != Some(&self.catalog_context()) {
            let context = self.catalog_context();
            self.slash.catalog = ComposerCatalog::default();
            self.slash.token = None;
            self.slash.choices.clear();
            self.slash.context = Some(context);
            self.load_slash_catalog();
        }
        if token != self.slash.token || !self.slash.open() {
            self.slash.token = token;
            self.slash.dismissed = None;
            self.slash.selected = 0;
            self.slash.selection_touched = false;
            self.slash.scroll.set_offset(point(px(0.), px(0.)));
            if !self.slash.open() {
                self.slash.opened = Some(Instant::now());
            }
            self.slash.closing = None;
            self.filter_slash();
        }
        self.input
            .update(cx, |input, _| input.set_completion_open(true));
    }

    fn load_slash_catalog(&mut self) {
        let CatalogContext {
            task_id,
            project_id,
            provider,
            account,
            model,
        } = self.catalog_context();
        self.slash.loading = true;
        let request_id = new_id();
        self.slash.request_id = Some(request_id.clone());
        self.send(core::Request::ComposerCatalog {
            request_id,
            task_id,
            project_id,
            provider,
            account,
            model,
        });
    }

    /// Installs a catalog only for the task and project that requested it.
    pub(super) fn receive_slash_catalog(
        &mut self,
        request: &core::Request,
        catalog: ComposerCatalog,
    ) {
        if let core::Request::ComposerCatalog {
            request_id,
            task_id,
            project_id,
            provider,
            account,
            model,
        } = request
            && self.slash.request_id.as_ref() == Some(request_id)
            && self.catalog_context()
                == (CatalogContext {
                    task_id: task_id.clone(),
                    project_id: project_id.clone(),
                    provider: *provider,
                    account: account.clone(),
                    model: model.clone(),
                })
        {
            self.slash.loading = false;
            self.slash.catalog = catalog;
            self.filter_slash();
        }
    }

    /// Lists the task's files again after its file list arrived.
    pub(super) fn files_loaded(&mut self) {
        self.slash.files_loading = false;
        if self.slash.mention {
            self.filter_slash();
        }
    }

    // Ranks files for a mention: a file name that starts with the query, then a file name that
    // contains it, then a path that contains it.
    fn filter_mentions(&mut self, query: &str) {
        let mut ranked: Vec<(u8, String)> = self
            .files
            .iter()
            .filter_map(|path| {
                let path = String::from_utf8_lossy(path).into_owned();
                let lower = path.to_lowercase();
                let name = lower.rsplit('/').next().unwrap_or(&lower);
                let rank = if name.starts_with(query) {
                    0
                } else if name.contains(query) {
                    1
                } else if lower.contains(query) {
                    2
                } else {
                    return None;
                };
                Some((rank, path))
            })
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.len().cmp(&b.1.len())));
        self.slash.choices = ranked
            .into_iter()
            .take(40)
            .map(|(_, path)| Choice::Path(path))
            .collect();
        self.slash.selected = self
            .slash
            .selected
            .min(self.slash.choices.len().saturating_sub(1));
    }

    fn filter_slash(&mut self) {
        let query = self
            .slash
            .token
            .as_ref()
            .map_or("", |(_, query)| query)
            .to_lowercase();
        if self.slash.mention {
            self.filter_mentions(&query);
            return;
        }
        let mut choices: Vec<_> = self
            .slash
            .catalog
            .commands
            .iter()
            .copied()
            .map(Choice::Command)
            .chain(self.slash.catalog.skills.iter().cloned().map(Choice::Skill))
            .chain(
                self.slash
                    .catalog
                    .provider_commands
                    .iter()
                    // Kodem's own Compact command and its rules replace the CLI's commands.
                    .filter(|command| {
                        !self
                            .slash
                            .catalog
                            .commands
                            .contains(&ComposerCommand::Compact)
                            || !matches!(command.name.as_str(), "compact" | "autocompact")
                    })
                    .cloned()
                    .map(Choice::ProviderCommand),
            )
            .chain(self.selected_runtime().files.then_some(Choice::Attach))
            .collect();
        // Files and changes belong to a checkout, which a chat does not have.
        if self.view.is_some() && self.has_checkout() {
            choices.extend([Choice::Files, Choice::Diff]);
        }
        choices.retain(|choice| {
            let (name, detail, _, category) = choice.labels();
            name.to_lowercase().contains(&query)
                || detail.to_lowercase().contains(&query)
                || category.to_lowercase().contains(&query)
        });
        if !self.slash.catalog.errors.is_empty() {
            choices.push(Choice::Retry);
        }
        let previous = self
            .slash
            .selection_touched
            .then(|| self.slash.choices.get(self.slash.selected))
            .flatten();
        let selected = previous
            .and_then(|previous| choices.iter().position(|choice| choice == previous))
            .unwrap_or(0);
        self.slash.choices = choices;
        self.slash.selected = selected;
        self.slash.selected = self
            .slash
            .selected
            .min(self.slash.choices.len().saturating_sub(1));
    }

    /// Retains the last menu during its exit animation while disabling completion key routing.
    pub(super) fn close_slash(&mut self, cx: &mut Context<Self>) {
        if self.slash.open() {
            self.slash.closing = Some(Instant::now());
            self.slash.dismissed = self.slash.token.clone();
        }
        self.input
            .update(cx, |input, _| input.set_completion_open(false));
    }

    /// Moves the highlighted row into view or accepts/dismisses the current completion.
    pub(super) fn slash_key(&mut self, key: &CompletionInput, cx: &mut Context<Self>) {
        if !self.slash.open() {
            return;
        }
        match key {
            CompletionInput::Dismiss => self.close_slash(cx),
            CompletionInput::Accept => self.accept_slash(self.slash.selected, cx),
            CompletionInput::Previous | CompletionInput::Next => {
                let len = self.slash.choices.len();
                if len > 0 {
                    self.slash.selection_touched = true;
                    self.slash.selected = if matches!(key, CompletionInput::Previous) {
                        (self.slash.selected + len - 1) % len
                    } else {
                        (self.slash.selected + 1) % len
                    };
                    self.slash.scroll.scroll_to_item(self.slash.selected);
                }
            }
        }
        cx.notify();
    }

    fn accept_slash(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = if self.slash.mention {
            self.input.read(cx).mention_query()
        } else {
            self.input.read(cx).slash_query()
        };
        if !self.slash.open() || current != self.slash.token {
            return;
        }
        let Some(choice) = self.slash.choices.get(index).cloned() else {
            return;
        };
        if matches!(choice, Choice::Retry) {
            if !self.slash.loading {
                self.load_slash_catalog();
            }
            cx.notify();
            return;
        }
        let Some((range, _)) = self.slash.token.clone() else {
            return;
        };
        self.close_slash(cx);
        let replacement = match &choice {
            Choice::ProviderCommand(command) => format!("/{} ", command.name),
            Choice::Path(path) => format!("@{path} "),
            _ => String::new(),
        };
        self.input.update(cx, |input, cx| {
            input.complete_slash(range, &replacement, cx)
        });
        self.menu_anchor = self
            .input
            .read(cx)
            .text_bounds()
            .map(|bounds| Bounds::new(bounds.origin, Size::default()));
        match choice {
            Choice::ProviderCommand(_) | Choice::Path(_) => {}
            Choice::Skill(skill) => self.attach_skill(skill.path, cx),
            Choice::Attach => self.choose_attachments(cx),
            Choice::Files => {
                self.panel = Panel::Files;
                if let Some(task_id) = self.selected.clone() {
                    self.send(core::Request::Files { task_id });
                }
            }
            Choice::Diff => {
                self.show_pane(PanePage::Changes);
                self.settings.show_review = true;
                self.save_settings();
            }
            Choice::Command(command) => match command {
                ComposerCommand::New => self.open_empty_chat(cx),
                ComposerCommand::Provider | ComposerCommand::Model => self.panel = Panel::Models,
                ComposerCommand::Permissions => self.panel = Panel::Access,
                ComposerCommand::Skills | ComposerCommand::Mcp => {
                    self.panel = Panel::Resources;
                    self.send(core::Request::Resources {
                        provider: self.composer_preferences().provider,
                        account: self.composer_preferences().account,
                        task_id: self.selected.clone(),
                        project_id: self.catalog_context().project_id,
                    });
                }
                ComposerCommand::Compact => self.compact_task(cx),
                ComposerCommand::Undo => {
                    if let Some(task_id) = self.selected.clone() {
                        self.send(core::Request::UndoTurn { task_id });
                    }
                }
                ComposerCommand::Fork => {
                    if let Some(task_id) = self.selected.clone() {
                        self.send(core::Request::ForkTask { task_id });
                    }
                }
                ComposerCommand::Status => {
                    self.open_settings(settings::PROVIDERS);
                }
            },
            Choice::Retry => unreachable!(),
        }
        cx.notify();
    }

    /// Builds a scrollable completion menu with matching blur and slide entrance/exit curves.
    pub(super) fn slash_menu(
        &mut self,
        empty: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(opened) = self.slash.opened else {
            return div().into_any_element();
        };
        let phase = if cx.reduce_motion() {
            if self.slash.closing.is_some() { 0. } else { 1. }
        } else if let Some(closing) = self.slash.closing {
            1. - (closing.elapsed().as_secs_f32() / 0.14).min(1.)
        } else {
            (opened.elapsed().as_secs_f32() / 0.20).min(1.)
        };
        if phase <= 0. && self.slash.closing.is_some() {
            self.slash.opened = None;
            return div().into_any_element();
        }
        if phase < 1. || self.slash.closing.is_some() {
            window.request_animation_frame();
        }
        let t = self.theme;
        let available = self
            .input
            .read(cx)
            .text_bounds()
            .map_or(260., |bounds| {
                // The measured room is in window pixels; the limits are in points.
                let room = if empty {
                    window.viewport_size().height - bounds.bottom()
                } else {
                    bounds.top()
                };
                f32::from(room) / crate::theme::ui_scale() - 120.
            })
            .clamp(96., 260.);
        let mut rows = div()
            .id("slash-rows")
            .max_h(px(available))
            .overflow_y_scroll()
            .track_scroll(&self.slash.scroll)
            .flex()
            .flex_col()
            .p_1();
        for (index, choice) in self.slash.choices.iter().enumerate() {
            let (name, detail, glyph, category) = choice.labels();
            let chosen = index == self.slash.selected;
            let path = matches!(choice, Choice::Path(_));
            rows = rows.child(
                div()
                    .id(("slash-choice", index))
                    .relative()
                    .overflow_hidden()
                    .flex_shrink_0()
                    .h(px(44.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .cursor_pointer()
                    .when(chosen, |d| {
                        d.bg(t.accent.opacity(0.1))
                            .layer(dither::row_wash(t.accent, true, false))
                            .child(kit::selection_edge(t.accent))
                    })
                    .when(!chosen, |d| d.hover(move |d| d.bg(t.selected)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, _, _, cx| {
                            cx.stop_propagation();
                            s.accept_slash(index, cx);
                        }),
                    )
                    .child(icon(glyph).size(px(14.)).text_color(if chosen {
                        t.accent
                    } else {
                        t.muted
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(12.))
                                    .child(if path {
                                        name.to_owned()
                                    } else {
                                        format!("/{name}")
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(text_px(11.))
                                    .text_color(t.muted)
                                    .text_ellipsis()
                                    .when(path, |d| {
                                        d.font_family(crate::theme::mono())
                                            .text_size(text_px(10.5))
                                            .text_color(t.subtle)
                                    })
                                    .child(detail.to_owned()),
                            ),
                    )
                    .child(kit::chip(category.to_uppercase(), t.subtle)),
            );
        }
        if self.slash.choices.is_empty() {
            rows = rows.child(if self.slash.mention && self.slash.files_loading {
                div()
                    .p_3()
                    .child(kit::loader_row("Reading the task's files…", t))
            } else {
                kit::empty_hint(
                    if self.slash.mention {
                        "No matching files"
                    } else {
                        "No matching commands or skills"
                    },
                    t,
                )
                .py(px(12.))
            });
        }
        let menu = div()
            .id("slash-picker")
            .occlude()
            .rounded(px(crate::theme::RADIUS_FLOATING))
            .border_1()
            .border_color(t.edge)
            .bg(t.raised)
            .shadow(t.floating())
            .text_color(t.text)
            .overflow_hidden()
            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                s.close_slash(cx);
                cx.notify();
            }))
            .child(
                div()
                    .px(px(12.))
                    .pt(px(8.))
                    .pb(px(2.))
                    .child(kit::section_header(
                        if self.slash.mention {
                            "Files"
                        } else {
                            "Commands and skills"
                        },
                        Some(self.slash.choices.len()),
                        t,
                    )),
            )
            .child(rows)
            .when(!self.slash.catalog.errors.is_empty(), |d| {
                d.child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(11.))
                            .text_color(t.removed)
                            .max_h(px(48.))
                            .overflow_hidden()
                            .child(self.slash.catalog.errors.join("\n")),
                    )
            })
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .child(if self.slash.loading && !self.slash.mention {
                div()
                    .h(px(30.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .bg(t.header)
                    .child(kit::loader_row("Loading commands and skills…", t))
            } else {
                let mut hints = div()
                    .h(px(30.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .bg(t.header);
                for (keys, label) in [("↑↓", "Navigate"), ("⏎", "Select"), ("esc", "Dismiss")]
                {
                    hints = hints
                        .child(kit::keycap(keys, t))
                        .child(caps(label, t).mr(px(9.)));
                }
                hints
            });
        let p = if cx.reduce_motion() {
            1.
        } else {
            1. - (1. - phase).powi(3)
        };
        let layer = div()
            .absolute()
            .left(px(if empty { 0. } else { 20. }))
            .right(px(if empty { 0. } else { 20. }))
            .when(empty, |d| d.top_full().mt(px(8. + (1. - p) * 6.)))
            .when(!empty, |d| d.bottom_full().mb(px(8. - (1. - p) * 6.)))
            .opacity(crate::components::streaming_text::reveal_opacity(p))
            .child(
                div()
                    .relative()
                    .child(dither::shadow(t, crate::theme::RADIUS_FLOATING, 5.))
                    .child(menu),
            );
        // The menu overhangs what comes after the composer, such as the shortcut hints of a
        // start screen, so it paints after them. An open panel still paints over it.
        if self.panel == Panel::None {
            deferred(layer).into_any_element()
        } else {
            layer.into_any_element()
        }
    }
}
