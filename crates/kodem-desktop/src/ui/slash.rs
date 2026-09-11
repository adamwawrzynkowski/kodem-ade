//! Runtime-backed slash completions anchored to the native prompt editor.
use super::*;
use crate::components::text_input::CompletionInput;
use std::{ops::Range, time::Instant};

#[derive(Clone, PartialEq, Eq)]
enum Choice {
    Command(ComposerCommand),
    ProviderCommand(ProviderCommand),
    Skill(ComposerSkill),
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
                ComposerCommand::Status => (
                    "status",
                    "View runtime and account status",
                    "bot",
                    "Provider",
                ),
            },
            Self::Skill(skill) => (&skill.name, &skill.description, "code", "Skill"),
            Self::Attach => ("attach", "Add files or images", "paperclip", "Context"),
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
    model: Option<String>,
}

#[derive(Default)]
pub(super) struct SlashPicker {
    catalog: ComposerCatalog,
    context: Option<CatalogContext>,
    loading: bool,
    request_id: Option<String>,
    token: Option<(Range<usize>, String)>,
    dismissed: Option<(Range<usize>, String)>,
    choices: Vec<Choice>,
    selected: usize,
    selection_touched: bool,
    scroll: ScrollHandle,
    opened: Option<Instant>,
    closing: Option<Instant>,
}
impl SlashPicker {
    fn open(&self) -> bool {
        self.opened.is_some() && self.closing.is_none()
    }
}
impl Shell {
    fn catalog_context(&self) -> CatalogContext {
        CatalogContext {
            task_id: self.selected.clone(),
            project_id: self.selected_project.clone().or_else(|| {
                self.workspace
                    .as_ref()?
                    .projects
                    .first()
                    .map(|p| p.id.clone())
            }),
            provider: self.composer_preferences().provider,
            model: self.composer_preferences().model,
        }
    }

    /// Synchronizes completions with the caret without treating paths or IME text as commands.
    pub(super) fn sync_slash(&mut self, cx: &mut Context<Self>) {
        let token = self.input.read(cx).slash_query();
        if self.panel != Panel::None || token.is_none() || token == self.slash.dismissed {
            self.close_slash(cx);
            if token.is_none() {
                self.slash.dismissed = None;
            }
            return;
        }
        let context = self.catalog_context();
        if self.slash.context.as_ref() != Some(&context) {
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
            model,
        } = request
            && self.slash.request_id.as_ref() == Some(request_id)
            && self.catalog_context()
                == (CatalogContext {
                    task_id: task_id.clone(),
                    project_id: project_id.clone(),
                    provider: *provider,
                    model: model.clone(),
                })
        {
            self.slash.loading = false;
            self.slash.catalog = catalog;
            self.filter_slash();
        }
    }

    fn filter_slash(&mut self) {
        let query = self
            .slash
            .token
            .as_ref()
            .map_or("", |(_, query)| query)
            .to_lowercase();
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
                    .cloned()
                    .map(Choice::ProviderCommand),
            )
            .chain(self.selected_runtime().files.then_some(Choice::Attach))
            .collect();
        if self.view.is_some() {
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
        if !self.slash.open() || self.input.read(cx).slash_query() != self.slash.token {
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
            _ => String::new(),
        };
        self.input.update(cx, |input, cx| {
            input.complete_slash(range, &replacement, cx)
        });
        self.menu_anchor = self
            .input
            .read(cx)
            .text_bounds()
            .map(|bounds| bounds.origin);
        match choice {
            Choice::ProviderCommand(_) => {}
            Choice::Skill(skill) => self.attach_skill(skill.path, cx),
            Choice::Attach => self.choose_attachments(cx),
            Choice::Files => {
                self.panel = Panel::Files;
                if let Some(task_id) = self.selected.clone() {
                    self.send(core::Request::Files { task_id });
                }
            }
            Choice::Diff => {
                self.show_review = true;
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
                        task_id: self.selected.clone(),
                        project_id: self.catalog_context().project_id,
                    });
                }
                ComposerCommand::Status => {
                    self.panel = Panel::Settings;
                    self.settings_section = 2;
                    self.settings_provider = self.composer_preferences().provider;
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
                if empty {
                    f32::from(window.viewport_size().height - bounds.bottom()) - 120.
                } else {
                    f32::from(bounds.top()) - 120.
                }
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
            rows = rows.child(
                div()
                    .id(("slash-choice", index))
                    .flex_shrink_0()
                    .h(px(48.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(index == self.slash.selected, |d| d.bg(t.accent_bg))
                    .hover(|d| d.bg(t.accent_bg))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, _, _, cx| {
                            cx.stop_propagation();
                            s.accept_slash(index, cx);
                        }),
                    )
                    .child(icon(glyph).size(px(16.)).text_color(t.accent))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_size(px(12.)).child(format!("/{name}")))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t.muted)
                                    .text_ellipsis()
                                    .child(detail.to_owned()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(t.subtle)
                            .child(category),
                    ),
            );
        }
        if self.slash.choices.is_empty() {
            rows = rows.child(
                div()
                    .p_3()
                    .text_color(t.muted)
                    .child("No matching commands or skills"),
            );
        }
        let menu = div()
            .id("slash-picker")
            .occlude()
            .rounded(px(10.))
            .border_1()
            .border_color(t.border)
            .bg(t.raised)
            .shadow(t.elevation())
            .text_color(t.text)
            .overflow_hidden()
            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                s.close_slash(cx);
                cx.notify();
            }))
            .child(rows)
            .when(!self.slash.catalog.errors.is_empty(), |d| {
                d.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_size(px(11.))
                        .text_color(t.removed)
                        .max_h(px(48.))
                        .overflow_hidden()
                        .child(self.slash.catalog.errors.join("\n")),
                )
            })
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(10.))
                    .text_color(t.muted)
                    .child(if self.slash.loading {
                        "Loading provider commands and skills…"
                    } else {
                        "↑ ↓ navigate · Enter / Tab select · Esc dismiss"
                    }),
            );
        let p = if cx.reduce_motion() {
            1.
        } else {
            1. - (1. - phase).powi(3)
        };
        div()
            .absolute()
            .left(px(if empty { 0. } else { 20. }))
            .right(px(if empty { 0. } else { 20. }))
            .when(empty, |d| d.top_full().mt(px(8. + (1. - p) * 6.)))
            .when(!empty, |d| d.bottom_full().mb(px(8. - (1. - p) * 6.)))
            .opacity(p)
            .filter(vec![Filter::Blur(px(2.4 * (1. - p).powi(2)))])
            .child(menu)
            .into_any_element()
    }
}
