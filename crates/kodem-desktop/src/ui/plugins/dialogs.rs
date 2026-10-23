//! The dialogs of the Plugins workspace: a custom MCP server from a form, a pasted
//! configuration or a web search, and a skill from GitHub, a folder or new instructions.
use super::*;

/// The pages of the custom server dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum CustomPage {
    Form,
    Paste,
    Web,
}

/// The custom server dialog's fields and what its searches found.
pub(in crate::ui) struct CustomServer {
    pub(in crate::ui) page: CustomPage,
    /// The installed name that the dialog edits; absent for a new server.
    editing: Option<String>,
    kind: mcp::Kind,
    name: Entity<TextInput>,
    /// The command line of a local server.
    command: Entity<TextInput>,
    /// The address of a remote server.
    url: Entity<TextInput>,
    /// Variables of a local server as `NAME=value` lines, or headers of a remote one as
    /// `Name: value` lines.
    pairs: Entity<TextInput>,
    paste: Entity<TextInput>,
    query: Entity<TextInput>,
    providers: Vec<Provider>,
    parsed: Option<Result<Vec<mcp::Server>, String>>,
    web: Load<Vec<catalog::WebResult>>,
    /// The configurations found in each result's README, by repository.
    readmes: HashMap<String, Load<Vec<mcp::Server>>>,
    running: bool,
}

/// The pages of the skill dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum SkillPage {
    GitHub,
    Folder,
    New,
}

/// The skill dialog's fields and the skills that its source holds.
pub(in crate::ui) struct AddSkill {
    page: SkillPage,
    address: Entity<TextInput>,
    found: Load<Vec<catalog::CatalogSkill>>,
    /// Names of the found skills that the installation takes.
    chosen: Vec<String>,
    name: Entity<TextInput>,
    description: Entity<TextInput>,
    body: Entity<TextInput>,
    providers: Vec<Provider>,
    running: bool,
}

impl CustomServer {
    /// Returns the installed name that the dialog edits; absent for a new server.
    pub(in crate::ui) fn editing(&self) -> Option<&str> {
        self.editing.as_deref()
    }
}

impl Shell {
    /// Opens the custom server dialog, for a new server or to edit an installed one.
    pub(in crate::ui) fn open_custom_server(
        &mut self,
        editing: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry = editing
            .as_ref()
            .and_then(|name| self.plugins.entries(name).into_iter().next().cloned());
        let providers = match &editing {
            Some(name) => self
                .plugins
                .entries(name)
                .iter()
                .map(|entry| entry.provider)
                .collect(),
            None => self.plugins.installed().to_vec(),
        };
        let (kind, command, url, pairs) = match entry.as_ref().map(|entry| &entry.server.transport)
        {
            Some(mcp::Transport::Stdio { command, args, env }) => (
                mcp::Kind::Stdio,
                command_line(command, args),
                String::new(),
                env.iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Some(mcp::Transport::Http { url, headers } | mcp::Transport::Sse { url, headers }) => (
                entry
                    .as_ref()
                    .map(|entry| entry.server.transport.kind())
                    .unwrap_or(mcp::Kind::Http),
                String::new(),
                url.clone(),
                headers
                    .iter()
                    .map(|(key, value)| format!("{key}: {value}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            None => (
                mcp::Kind::Stdio,
                String::new(),
                String::new(),
                String::new(),
            ),
        };
        let query = self.plugins.search.read(cx).text();
        let name = single_line_input("my-server", editing.clone().unwrap_or_default(), cx);
        let command = single_line_input("npx -y @scope/server --option value", command, cx);
        let url = single_line_input("https://example.com/mcp", url, cx);
        let pairs = multi_line_input("NAME=value, one per line", pairs, cx);
        let paste = multi_line_input(
            "{ \"mcpServers\": { … } }, [mcp_servers.name], claude mcp add …, or npx …",
            String::new(),
            cx,
        );
        let query = single_line_input("What should the server do?", query, cx);
        window.focus(&name.read(cx).focus_handle(cx), cx);
        self.plugins.custom = Some(CustomServer {
            page: CustomPage::Form,
            editing,
            kind,
            name,
            command,
            url,
            pairs,
            paste,
            query,
            providers,
            parsed: None,
            web: Load::Idle,
            readmes: HashMap::new(),
            running: false,
        });
        self.sync_pairs_hint(cx);
        self.panel = Panel::AddServer;
        cx.notify();
    }

    // Sets the hint of the pairs field for the chosen transport.
    fn sync_pairs_hint(&mut self, cx: &mut Context<Self>) {
        let Some(custom) = &self.plugins.custom else {
            return;
        };
        let hint = if custom.kind == mcp::Kind::Stdio {
            "API_KEY=value — one variable per line"
        } else {
            "Authorization: Bearer token — one header per line"
        };
        custom
            .pairs
            .update(cx, |input, cx| input.set_placeholder(hint, cx));
    }

    /// Searches the web for servers that match the dialog's query: GitHub's repositories.
    pub(in crate::ui) fn search_web_for(&mut self, cx: &mut Context<Self>) {
        let Some(custom) = &mut self.plugins.custom else {
            return;
        };
        let query = custom.query.read(cx).text().trim().to_owned();
        if query.is_empty() {
            return;
        }
        custom.web = Load::Loading;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { catalog::search_web(&query) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if let Some(custom) = &mut s.plugins.custom {
                    custom.web = match result {
                        Ok(found) => Load::Ready(found),
                        Err(error) => Load::Failed(format!("{error:#}")),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    // Reads the configurations of a web result's README.
    fn read_readme(&mut self, repository: String, cx: &mut Context<Self>) {
        let Some(custom) = &mut self.plugins.custom else {
            return;
        };
        custom.readmes.insert(repository.clone(), Load::Loading);
        cx.spawn(async move |this, cx| {
            let asked = repository.clone();
            let result = cx
                .background_executor()
                .spawn(async move { catalog::readme_configurations(&asked) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if let Some(custom) = &mut s.plugins.custom {
                    custom.readmes.insert(
                        repository,
                        match result {
                            Ok(found) => Load::Ready(found),
                            Err(error) => Load::Failed(format!("{error:#}")),
                        },
                    );
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    // Fills the form with a server that a paste or a search found, and shows the form.
    fn use_found_server(&mut self, server: mcp::Server, cx: &mut Context<Self>) {
        let Some(custom) = &mut self.plugins.custom else {
            return;
        };
        custom.page = CustomPage::Form;
        custom.kind = server.transport.kind();
        let (command, url, pairs) = match &server.transport {
            mcp::Transport::Stdio { command, args, env } => (
                command_line(command, args),
                String::new(),
                env.iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            mcp::Transport::Http { url, headers } | mcp::Transport::Sse { url, headers } => (
                String::new(),
                url.clone(),
                headers
                    .iter()
                    .map(|(key, value)| format!("{key}: {value}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        };
        let (name, command_field, url_field, pairs_field) = (
            custom.name.clone(),
            custom.command.clone(),
            custom.url.clone(),
            custom.pairs.clone(),
        );
        name.update(cx, |input, cx| input.set_text(server.name, cx));
        command_field.update(cx, |input, cx| input.set_text(command, cx));
        url_field.update(cx, |input, cx| input.set_text(url, cx));
        pairs_field.update(cx, |input, cx| input.set_text(pairs, cx));
        self.sync_pairs_hint(cx);
        cx.notify();
    }

    // Reads the server that the form describes.
    fn custom_server(&self, cx: &App) -> Result<mcp::Server, String> {
        let custom = self.plugins.custom.as_ref().ok_or_else(String::new)?;
        let name = custom.name.read(cx).text().trim().to_owned();
        mcp::validate_name(&name).map_err(|error| format!("{error:#}"))?;
        let pairs_text = custom.pairs.read(cx).text();
        let transport = match custom.kind {
            mcp::Kind::Stdio => {
                let words = mcp::command_words(custom.command.read(cx).text().trim());
                let mut words = words.into_iter();
                let command = words
                    .next()
                    .ok_or("Enter the command that starts the server")?;
                mcp::Transport::Stdio {
                    command,
                    args: words.collect(),
                    env: pairs(&pairs_text, '=')?,
                }
            }
            kind => {
                let url = custom.url.read(cx).text().trim().to_owned();
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    return Err("Enter the server's address, starting with https://".into());
                }
                let headers = pairs(&pairs_text, ':')?;
                if kind == mcp::Kind::Sse {
                    mcp::Transport::Sse { url, headers }
                } else {
                    mcp::Transport::Http { url, headers }
                }
            }
        };
        Ok(mcp::Server { name, transport })
    }

    // Installs the form's server for the chosen providers. An edit also removes it from the
    // providers that were unticked, and the earlier name after a rename.
    fn save_custom_server(&mut self, cx: &mut Context<Self>) {
        let server = match self.custom_server(cx) {
            Ok(server) => server,
            Err(error) => {
                if !error.is_empty() {
                    self.report_error(error, cx);
                }
                return;
            }
        };
        let Some(custom) = &mut self.plugins.custom else {
            return;
        };
        let kind = server.transport.kind();
        let providers: Vec<Provider> = custom
            .providers
            .iter()
            .copied()
            .filter(|provider| mcp::supports(*provider, kind))
            .collect();
        if providers.is_empty() {
            self.report_error(
                "Choose at least one provider that can reach this server",
                cx,
            );
            return;
        }
        custom.running = true;
        let editing = custom.editing.clone();
        let holders: Vec<Provider> = editing
            .as_ref()
            .map(|name| {
                self.plugins
                    .entries(name)
                    .iter()
                    .map(|entry| entry.provider)
                    .collect()
            })
            .unwrap_or_default();
        let count = providers.len();
        let name = server.name.clone();
        let done = match &editing {
            Some(_) => format!("Saved {name}"),
            None => format!("Installed {name} for {}", servers::provider_count(count)),
        };
        self.panel = Panel::None;
        self.run_plugin_change(
            done,
            move |accounts| {
                let mut failures = mcp::install(&accounts, &providers, &server);
                if let Some(earlier) = editing {
                    let dropped: Vec<Provider> = holders
                        .iter()
                        .copied()
                        .filter(|provider| earlier != server.name || !providers.contains(provider))
                        .collect();
                    failures.extend(mcp::remove(&accounts, &dropped, &earlier));
                }
                labelled(failures)
            },
            cx,
        );
        self.plugins.scope = Scope::Installed;
        self.plugins.detail = Some(Detail::Installed(name));
        self.plugins.form = None;
    }

    /// Builds the custom server dialog's body.
    pub(in crate::ui) fn custom_server_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(custom) = &self.plugins.custom else {
            return div().into_any_element();
        };
        let page = custom.page;
        let editing = custom.editing.is_some();
        let tabs = self.dialog_tabs(
            "plugins-custom-page",
            &[
                (CustomPage::Form, "pencil", "Form"),
                (CustomPage::Paste, "copy", "Paste configuration"),
                (CustomPage::Web, "globe", "Search the web"),
            ],
            page,
            |s, page| {
                if let Some(custom) = &mut s.plugins.custom {
                    custom.page = page;
                }
            },
            cx,
        );
        let body = match page {
            CustomPage::Form => self.custom_form(cx),
            CustomPage::Paste => self.custom_paste(cx),
            CustomPage::Web => self.custom_web(cx),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .when(!editing, |d| d.child(tabs))
            .child(body)
            .child(
                div()
                    .text_size(text_px(11.))
                    .line_height(relative(1.45))
                    .text_color(t.subtle)
                    .child("Values are written as they are into each CLI's configuration file. Claude Code, Codex and Grok Build can read a secret from your environment with ${NAME}; OpenCode uses {env:NAME}."),
            )
            .into_any_element()
    }

    /// Returns the actions of the custom server dialog.
    pub(in crate::ui) fn custom_server_actions(&self, cx: &mut Context<Self>) -> Div {
        let Some(custom) = &self.plugins.custom else {
            return div();
        };
        let form = custom.page == CustomPage::Form;
        let label = if custom.editing.is_some() {
            "Save".to_owned()
        } else {
            let kind = custom.kind;
            let count = custom
                .providers
                .iter()
                .filter(|provider| mcp::supports(**provider, kind))
                .count();
            format!("Install for {}", servers::provider_count(count))
        };
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .child(
                self.ghost("plugins-custom-cancel", "close", "Cancel")
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.panel = Panel::None;
                        cx.notify();
                    })),
            )
            .when(form, |d| {
                d.child(
                    self.primary("plugins-custom-save", "check", label)
                        .disabled(custom.running)
                        .on_click(cx.listener(|s, _, _, cx| s.save_custom_server(cx))),
                )
            })
    }

    // The form page: transport, name, command or address, variables or headers, providers.
    fn custom_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(custom) = &self.plugins.custom else {
            return div().into_any_element();
        };
        let kind = custom.kind;
        let mut kinds = div()
            .flex()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .border_1()
            .border_color(t.edge)
            .bg(t.inset);
        for (index, option) in mcp::Kind::ALL.into_iter().enumerate() {
            let picked = option == kind;
            kinds = kinds.child(
                self.control(("plugins-custom-kind", index))
                    .h(px(22.))
                    .px(px(10.))
                    .rounded(px(crate::theme::RADIUS_SMALL))
                    .overflow_hidden()
                    .border_color(gpui::transparent_black())
                    .bg(gpui::transparent_black())
                    .text_color(if picked { t.text } else { t.muted })
                    .when(picked, |c| {
                        c.bg(kit::raised_fill(t.control))
                            .border_color(t.edge)
                            .shadow(kit::raised_light(t))
                    })
                    .when(!picked, |c| c.hover(move |c| c.text_color(t.text)))
                    .child(option.label())
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Some(custom) = &mut s.plugins.custom {
                            custom.kind = option;
                        }
                        s.sync_pairs_hint(cx);
                        cx.notify();
                    })),
            );
        }
        let (target, target_label, target_detail, pairs_label, pairs_detail) = match kind {
            mcp::Kind::Stdio => (
                custom.command.clone(),
                "Command",
                "The program and its arguments, as you would type them in a terminal",
                "Environment",
                "Variables that the program starts with",
            ),
            _ => (
                custom.url.clone(),
                "Address",
                "The server's endpoint, usually ending in /mcp, or /sse for SSE",
                "Headers",
                "Sent with every request, such as an Authorization header",
            ),
        };
        let chosen = custom.providers.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(caps("Transport", t))
                    .child(kinds),
            )
            .child(self.form_field(
                "Name",
                "Letters, digits, hyphens and underscores; each CLI lists the server under it",
                custom.name.clone().into_any_element(),
                true,
            ))
            .child(self.form_field(target_label, target_detail, target.into_any_element(), true))
            .child(self.pairs_field(pairs_label, pairs_detail, custom.pairs.clone()))
            .child(div().pt(px(2.)).child(caps("Providers", t)))
            .child(self.provider_choices(
                "plugins-custom-provider",
                &chosen,
                move |provider| {
                    (!mcp::supports(provider, kind))
                        .then(|| format!("Cannot reach a server over {}", kind.label()))
                },
                |s, provider, on| {
                    if let Some(custom) = &mut s.plugins.custom {
                        custom.providers.retain(|known| *known != provider);
                        if on {
                            custom.providers.push(provider);
                        }
                    }
                },
                cx,
            ))
            .into_any_element()
    }

    // A field of several lines in a recessed frame.
    fn pairs_field(
        &self,
        label: &'static str,
        detail: &'static str,
        input: Entity<TextInput>,
    ) -> Div {
        let t = self.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .child(
                div()
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.text)
                    .child(label),
            )
            .child(
                div()
                    .text_size(text_px(11.5))
                    .text_color(t.subtle)
                    .child(detail),
            )
            .child(
                settings::input_frame(t)
                    .h_auto()
                    .min_h(px(64.))
                    .py(px(6.))
                    .items_start()
                    .border_color(t.border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(12.))
                            .child(input),
                    ),
            )
    }

    // The paste page: a configuration in any common form, and the servers read from it.
    fn custom_paste(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(custom) = &self.plugins.custom else {
            return div().into_any_element();
        };
        let paste = custom.paste.clone();
        let mut found = div().flex().flex_col().gap(px(6.));
        match &custom.parsed {
            Some(Ok(servers)) => {
                for (index, server) in servers.iter().enumerate() {
                    found = found.child(self.found_server_row(
                        ("plugins-parsed", index),
                        server.clone(),
                        cx,
                    ));
                }
            }
            Some(Err(error)) => {
                found = found.child(settings::error_block(
                    "Not a configuration",
                    error.clone(),
                    t,
                ));
            }
            None => {}
        }
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .text_size(text_px(12.))
                    .line_height(relative(1.45))
                    .text_color(t.muted)
                    .child("Paste what a server's README gives: a JSON block of mcpServers (Claude, Cursor, Antigravity), servers (VS Code) or mcp (OpenCode), a TOML table of Codex or Grok Build, a claude, codex or grok mcp add command, the command that starts it, or its address."),
            )
            .child(
                settings::input_frame(t)
                    .h_auto()
                    .min_h(px(140.))
                    .py(px(8.))
                    .items_start()
                    .border_color(t.border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(12.))
                            .child(paste.clone()),
                    ),
            )
            .child(
                div().flex().justify_end().child(
                    self.button("plugins-parse", "scan-eye", "Read configuration")
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let text = paste.read(cx).text();
                            if let Some(custom) = &mut s.plugins.custom {
                                let parsed = mcp::parse(&text).map_err(|error| format!("{error:#}"));
                                // A single server goes straight into the form.
                                if let Ok([server]) = parsed.as_deref() {
                                    let server = server.clone();
                                    custom.parsed = None;
                                    s.use_found_server(server, cx);
                                    return;
                                }
                                custom.parsed = Some(parsed);
                            }
                            cx.notify();
                        })),
                ),
            )
            .child(found)
            .into_any_element()
    }

    // A server that a paste or a README held, with the action that fills the form with it.
    fn found_server_row(
        &self,
        id: impl Into<ElementId>,
        server: mcp::Server,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let summary = server.transport.summary();
        let kind = server.transport.kind();
        div()
            .px(px(10.))
            .py(px(7.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .border_1()
            .border_color(t.border)
            .bg(t.inset)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(text_px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.text)
                            .child(server.name.clone())
                            .child(kit::chip(kind.label().to_uppercase(), t.muted)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(11.))
                            .text_color(t.subtle)
                            .child(summary),
                    ),
            )
            .child(
                self.button(id, "arrow-right", "Use").h(px(24.)).on_click(
                    cx.listener(move |s, _, _, cx| s.use_found_server(server.clone(), cx)),
                ),
            )
    }

    // The web page: a search of GitHub's repositories, and the configurations of their READMEs.
    fn custom_web(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(custom) = &self.plugins.custom else {
            return div().into_any_element();
        };
        let query = custom.query.clone();
        let mut results = div().flex().flex_col().gap(px(8.));
        match &custom.web {
            Load::Idle => {}
            Load::Loading => results = results.child(kit::loader_row("SEARCHING GITHUB", t)),
            Load::Failed(error) => {
                results =
                    results.child(settings::error_block("The search failed", error.clone(), t))
            }
            Load::Ready(found) if found.is_empty() => {
                results = results.child(kit::empty_hint("No repository matches", t))
            }
            Load::Ready(found) => {
                for (index, result) in found.iter().enumerate() {
                    let readme = custom.readmes.get(&result.name).cloned();
                    let repository = result.name.clone();
                    let url = result.url.clone();
                    let mut row = div()
                        .px(px(10.))
                        .py(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .rounded(px(crate::theme::RADIUS_CONTROL))
                        .border_1()
                        .border_color(t.border)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(icon("github").size(px(13.)).text_color(t.muted))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .text_size(text_px(12.5))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(t.text)
                                        .child(result.name.clone()),
                                )
                                .child(figure(format!("★ {}", result.stars), t.subtle))
                                .child(
                                    self.ghost(("plugins-web-open", index), "external-link", "")
                                        .w(px(26.))
                                        .px_0()
                                        .on_click(
                                            cx.listener(move |_, _, _, cx| cx.open_url(&url)),
                                        ),
                                )
                                .child(
                                    self.button(
                                        ("plugins-web-readme", index),
                                        "scan-eye",
                                        "Find configuration",
                                    )
                                    .h(px(24.))
                                    .disabled(matches!(readme, Some(Load::Loading)))
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| s.read_readme(repository.clone(), cx),
                                    )),
                                ),
                        )
                        .when(!result.description.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_size(text_px(11.5))
                                    .line_height(relative(1.4))
                                    .text_color(t.muted)
                                    .child(result.description.clone()),
                            )
                        });
                    match readme {
                        Some(Load::Loading) => row = row.child(kit::loader_row("READING README", t)),
                        Some(Load::Failed(error)) => {
                            row = row.child(
                                div()
                                    .text_size(text_px(11.5))
                                    .text_color(t.removed)
                                    .child(error),
                            )
                        }
                        Some(Load::Ready(servers)) if servers.is_empty() => {
                            row = row.child(
                                div()
                                    .text_size(text_px(11.5))
                                    .text_color(t.subtle)
                                    .child("The README holds no configuration that Kodem can read. Open the repository and paste one."),
                            )
                        }
                        Some(Load::Ready(servers)) => {
                            for (found, server) in servers.into_iter().enumerate() {
                                row = row.child(self.found_server_row(
                                    ("plugins-web-found", index * 100 + found),
                                    server,
                                    cx,
                                ));
                            }
                        }
                        _ => {}
                    }
                    results = results.child(row);
                }
            }
        }
        let web_query = query.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        settings::input_frame(t)
                            .flex_1()
                            .min_w_0()
                            .border_color(t.border)
                            .child(icon("search").size(px(12.)).text_color(t.subtle))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(12.))
                                    .child(query),
                            ),
                    )
                    .child(
                        self.button("plugins-web-search-run", "search", "Search GitHub")
                            .on_click(cx.listener(|s, _, _, cx| s.search_web_for(cx))),
                    )
                    .child(
                        self.ghost("plugins-web-browser", "external-link", "Web")
                            .tooltip(move |_, cx| {
                                cx.new(|_| Hint {
                                    text: "Search the whole web in your browser".into(),
                                    theme: t,
                                })
                                .into()
                            })
                            .on_click(cx.listener(move |_, _, _, cx| {
                                let terms = format!("{} MCP server", web_query.read(cx).text().trim());
                                let encoded: String = terms
                                    .bytes()
                                    .map(|byte| {
                                        if byte.is_ascii_alphanumeric() {
                                            (byte as char).to_string()
                                        } else {
                                            format!("%{byte:02X}")
                                        }
                                    })
                                    .collect();
                                cx.open_url(&format!("https://duckduckgo.com/?q={encoded}"));
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(text_px(11.5))
                    .line_height(relative(1.45))
                    .text_color(t.subtle)
                    .child("Finds MCP servers on GitHub, the most starred first. Find configuration reads the repository's README for a block that Kodem can install; check it before you install."),
            )
            .child(results)
            .into_any_element()
    }

    /// Returns a row of page tabs for a dialog.
    fn dialog_tabs<P: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        pages: &[(P, &'static str, &'static str)],
        current: P,
        choose: impl Fn(&mut Shell, P) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let mut strip = div()
            .flex()
            .items_end()
            .gap(px(2.))
            .border_b_1()
            .border_color(t.border);
        for (index, (page, glyph, label)) in pages.iter().copied().enumerate() {
            let chosen = page == current;
            let choose = choose.clone();
            strip = strip.child(
                div()
                    .id((id, index))
                    .relative()
                    .h(px(30.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if chosen { t.text } else { t.muted })
                    .when(!chosen, |tab| tab.hover(move |tab| tab.text_color(t.text)))
                    .child(icon(glyph).size(px(12.)).text_color(if chosen {
                        t.accent
                    } else {
                        t.subtle
                    }))
                    .child(label.to_uppercase())
                    .when(chosen, |tab| tab.child(kit::tab_indicator(t.accent)))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        choose(s, page);
                        cx.notify();
                    })),
            );
        }
        strip
    }

    /// Opens the skill dialog.
    pub(in crate::ui) fn open_add_skill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let address = single_line_input(
            "github.com/owner/repository/tree/main/skills/name",
            String::new(),
            cx,
        );
        let name = single_line_input("my-skill", String::new(), cx);
        let description = single_line_input(
            "What the skill does and when an agent should use it",
            String::new(),
            cx,
        );
        let body = multi_line_input("Instructions for the agent, in Markdown", String::new(), cx);
        window.focus(&address.read(cx).focus_handle(cx), cx);
        self.plugins.add_skill = Some(AddSkill {
            page: SkillPage::GitHub,
            address,
            found: Load::Idle,
            chosen: Vec::new(),
            name,
            description,
            body,
            providers: self.plugins.installed().to_vec(),
            running: false,
        });
        self.panel = Panel::AddSkill;
        cx.notify();
    }

    // Lists the skills of the GitHub address in the dialog.
    fn read_skill_address(&mut self, cx: &mut Context<Self>) {
        let Some(add) = &mut self.plugins.add_skill else {
            return;
        };
        let address = add.address.read(cx).text();
        let parts = match catalog::parse_github(&address) {
            Ok(parts) => parts,
            Err(error) => {
                add.found = Load::Failed(format!("{error:#}"));
                cx.notify();
                return;
            }
        };
        add.found = Load::Loading;
        cx.spawn(async move |this, cx| {
            let (owner, repository, reference, folder) = parts;
            let result = cx
                .background_executor()
                .spawn(async move {
                    catalog::read_skill_repository(&owner, &repository, &reference, &folder)
                })
                .await;
            let _ = this.update(cx, |s, cx| s.receive_found_skills(result, cx));
        })
        .detach();
        cx.notify();
    }

    // Asks for a folder and lists its skills.
    fn choose_skill_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        if let Some(add) = &mut self.plugins.add_skill {
            add.found = Load::Loading;
        }
        cx.spawn(async move |this, cx| {
            let folder = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(folder) = folder else {
                let _ = this.update(cx, |s, cx| {
                    if let Some(add) = &mut s.plugins.add_skill {
                        add.found = Load::Idle;
                    }
                    cx.notify();
                });
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { catalog::read_skill_folder(&folder) })
                .await;
            let _ = this.update(cx, |s, cx| s.receive_found_skills(result, cx));
        })
        .detach();
        cx.notify();
    }

    // Shows the skills that an address or a folder holds, all chosen.
    fn receive_found_skills(
        &mut self,
        result: anyhow::Result<Vec<catalog::CatalogSkill>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(add) = &mut self.plugins.add_skill {
            match result {
                Ok(found) => {
                    add.chosen = found.iter().map(|skill| skill.name.clone()).collect();
                    add.found = Load::Ready(found);
                }
                Err(error) => add.found = Load::Failed(format!("{error:#}")),
            }
            cx.notify();
        }
    }

    // Installs what the skill dialog holds: the chosen skills, or the new skill.
    fn save_add_skill(&mut self, cx: &mut Context<Self>) {
        let Some(add) = &mut self.plugins.add_skill else {
            return;
        };
        let providers = add.providers.clone();
        if providers.is_empty() {
            self.report_error("Choose at least one provider", cx);
            return;
        }
        let count = providers.len();
        match add.page {
            SkillPage::New => {
                let name = add.name.read(cx).text().trim().to_owned();
                let description = add.description.read(cx).text().trim().to_owned();
                let body = add.body.read(cx).text();
                if let Err(error) = skills::validate_name(&name) {
                    self.report_error(format!("{error:#}"), cx);
                    return;
                }
                if description.is_empty() {
                    self.report_error("Describe what the skill does and when to use it", cx);
                    return;
                }
                add.running = true;
                self.panel = Panel::None;
                self.run_plugin_change(
                    format!("Created {name} for {}", servers::provider_count(count)),
                    move |_| labelled(skills::create(&name, &description, &body, &providers)),
                    cx,
                );
            }
            SkillPage::GitHub | SkillPage::Folder => {
                let Load::Ready(found) = &add.found else {
                    return;
                };
                let chosen: Vec<catalog::CatalogSkill> = found
                    .iter()
                    .filter(|skill| add.chosen.contains(&skill.name))
                    .cloned()
                    .collect();
                if chosen.is_empty() {
                    self.report_error("Choose at least one skill", cx);
                    return;
                }
                add.running = true;
                let skills_count = chosen.len();
                self.panel = Panel::None;
                self.run_plugin_change(
                    format!(
                        "Installed {} for {}",
                        if skills_count == 1 {
                            chosen[0].name.clone()
                        } else {
                            format!("{skills_count} skills")
                        },
                        servers::provider_count(count)
                    ),
                    move |_| {
                        chosen
                            .iter()
                            .flat_map(|skill| {
                                labelled(skills::install(&skill.folder, &skill.name, &providers))
                            })
                            .collect()
                    },
                    cx,
                );
            }
        }
        self.plugins.scope = Scope::Installed;
        self.plugins.detail = None;
    }

    /// Builds the skill dialog's body.
    pub(in crate::ui) fn add_skill_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(add) = &self.plugins.add_skill else {
            return div().into_any_element();
        };
        let page = add.page;
        let tabs = self.dialog_tabs(
            "plugins-skill-page",
            &[
                (SkillPage::GitHub, "github", "From GitHub"),
                (SkillPage::Folder, "folder-open", "From a folder"),
                (SkillPage::New, "pencil", "New skill"),
            ],
            page,
            |s, page| {
                if let Some(add) = &mut s.plugins.add_skill {
                    if add.page != page {
                        add.found = Load::Idle;
                        add.chosen.clear();
                    }
                    add.page = page;
                }
            },
            cx,
        );
        let source = match page {
            SkillPage::GitHub => div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(6.))
                        .child(div().flex_1().min_w_0().child(self.form_field(
                            "Address",
                            "A repository, or the folder of one skill or of several",
                            add.address.clone().into_any_element(),
                            true,
                        )))
                        .child(
                            self.button("plugins-skill-read", "arrow-down-to-line", "Read")
                                .h(px(30.))
                                .on_click(cx.listener(|s, _, _, cx| s.read_skill_address(cx))),
                        ),
                )
                .child(self.found_skills(cx))
                .into_any_element(),
            SkillPage::Folder => div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(text_px(12.))
                                .line_height(relative(1.45))
                                .text_color(t.muted)
                                .child("A folder with a SKILL.md, or a folder of skill folders. Each skill is copied, so the original stays where it is."),
                        )
                        .child(
                            self.button("plugins-skill-folder", "folder-open", "Choose folder…")
                                .on_click(cx.listener(|s, _, _, cx| s.choose_skill_folder(cx))),
                        ),
                )
                .child(self.found_skills(cx))
                .into_any_element(),
            SkillPage::New => div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(self.form_field(
                    "Name",
                    "Lowercase words joined by hyphens, such as release-notes",
                    add.name.clone().into_any_element(),
                    true,
                ))
                .child(self.form_field(
                    "Description",
                    "Agents read it to decide when the skill applies",
                    add.description.clone().into_any_element(),
                    true,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .child(caps("Instructions", t))
                        .child(
                            settings::input_frame(t)
                                .h_auto()
                                .min_h(px(150.))
                                .py(px(8.))
                                .items_start()
                                .border_color(t.border)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .font_family(crate::theme::mono())
                                        .text_size(text_px(12.))
                                        .child(add.body.clone()),
                                ),
                        ),
                )
                .into_any_element(),
        };
        let chosen = add.providers.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(tabs)
            .child(source)
            .child(div().pt(px(2.)).child(caps("Providers", t)))
            .child(self.provider_choices(
                "plugins-add-skill-provider",
                &chosen,
                |_| None,
                |s, provider, on| {
                    if let Some(add) = &mut s.plugins.add_skill {
                        add.providers.retain(|known| *known != provider);
                        if on {
                            add.providers.push(provider);
                        }
                    }
                },
                cx,
            ))
            .into_any_element()
    }

    // The skills that the dialog's address or folder holds, each with a switch.
    fn found_skills(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(add) = &self.plugins.add_skill else {
            return div().into_any_element();
        };
        match &add.found {
            Load::Idle => div().into_any_element(),
            Load::Loading => kit::loader_row("READING SKILLS", t).into_any_element(),
            Load::Failed(error) => {
                settings::error_block("No skills found", error.clone(), t).into_any_element()
            }
            Load::Ready(found) => {
                let mut rows = div().flex().flex_col();
                for (index, skill) in found.iter().enumerate() {
                    let on = add.chosen.contains(&skill.name);
                    let name = skill.name.clone();
                    let exists = self
                        .plugins
                        .skills
                        .skills
                        .iter()
                        .any(|known| known.name == skill.name);
                    rows = rows.child(
                        div()
                            .px(px(10.))
                            .py(px(7.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .when(index > 0, |d| {
                                d.border_t_1().border_color(t.border.opacity(0.6))
                            })
                            .child(self.skill_tile(&skill.name, &skill.description, 26.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .text_size(text_px(12.5))
                                            .text_color(t.text)
                                            .child(skill.name.clone())
                                            .when(exists, |d| {
                                                d.child(kit::chip("REPLACES INSTALLED", t.warning))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .text_size(text_px(11.5))
                                            .text_color(t.muted)
                                            .child(skill.description.clone()),
                                    ),
                            )
                            .child(
                                controls::Switch::new(format!("plugins-found-skill-{index}"), on)
                                    .label(skill.name.clone())
                                    .on_click(cx.listener(move |s, enabled: &bool, _, cx| {
                                        if let Some(add) = &mut s.plugins.add_skill {
                                            add.chosen.retain(|known| *known != name);
                                            if *enabled {
                                                add.chosen.push(name.clone());
                                            }
                                        }
                                        cx.notify();
                                    })),
                            ),
                    );
                }
                kit::block(t)
                    .child(
                        kit::block_header("Skills found", t.accent, t)
                            .child(figure(found.len().to_string(), t.subtle)),
                    )
                    .child(rows)
                    .into_any_element()
            }
        }
    }

    /// Returns the actions of the skill dialog.
    pub(in crate::ui) fn add_skill_actions(&self, cx: &mut Context<Self>) -> Div {
        let Some(add) = &self.plugins.add_skill else {
            return div();
        };
        let ready = match add.page {
            SkillPage::New => true,
            _ => matches!(add.found, Load::Ready(_)) && !add.chosen.is_empty(),
        };
        let label = match add.page {
            SkillPage::New => format!(
                "Create for {}",
                servers::provider_count(add.providers.len())
            ),
            _ => format!(
                "Install for {}",
                servers::provider_count(add.providers.len())
            ),
        };
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .child(
                self.ghost("plugins-skill-cancel", "close", "Cancel")
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.panel = Panel::None;
                        cx.notify();
                    })),
            )
            .child(
                self.primary("plugins-skill-save", "check", label)
                    .disabled(!ready || add.running || add.providers.is_empty())
                    .on_click(cx.listener(|s, _, _, cx| s.save_add_skill(cx))),
            )
    }
}

/// Joins a command and its arguments into one line, quoting words with spaces or quotes.
fn command_line(command: &str, args: &[String]) -> String {
    std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .map(|word| {
            if word.is_empty() || word.contains([' ', '"', '\'', '\\', '$', '`']) {
                format!("'{}'", word.replace('\'', "'\\''"))
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads `NAME=value` or `Name: value` lines, split at `separator`, or at the other one when a
/// line lacks it. Blank lines and lines that start with `#` are left out.
fn pairs(text: &str, separator: char) -> Result<Vec<(String, String)>, String> {
    let other = if separator == '=' { ':' } else { '=' };
    let mut found = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once(separator)
            .or_else(|| line.split_once(other))
            .ok_or_else(|| format!("Write “{line}” as NAME{separator}value"))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("“{line}” has no name"));
        }
        found.push((key.to_owned(), value.trim().to_owned()));
    }
    Ok(found)
}
