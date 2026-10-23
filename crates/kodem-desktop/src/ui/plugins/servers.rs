//! The MCP servers tab: the registry's servers and the installed ones, a server's detail with
//! its installation form, and what each provider holds of it.
use super::*;

/// Registry names of servers that Discover features while no search or category narrows it,
/// in their order. A name matches when it holds one of these.
const FEATURED: [&str; 12] = [
    "github/github-mcp-server",
    "microsoft/playwright",
    "upstash/context7",
    "notion",
    "linear",
    "sentry",
    "figma",
    "stripe",
    "supabase",
    "cloudflare",
    "atlassian",
    "vercel",
];

impl Shell {
    /// Builds the rows of the servers tab: in Discover, the banner, the featured servers and
    /// every server of the open category; in Installed, the installed servers.
    pub(in crate::ui) fn server_rows(
        &mut self,
        columns: usize,
        cx: &mut Context<Self>,
    ) -> Vec<GridRow> {
        let words = self.plugins.query(cx);
        let provider = self.plugins.provider;
        let mut rows = Vec::new();
        match self.plugins.scope {
            Scope::Discover => {
                let category = self.plugins.category;
                let categories = self.server_categories();
                let registry = &self.plugins.registry;
                let reachable = |server: &catalog::CatalogServer| {
                    provider.is_none_or(|provider| {
                        server.launches.iter().any(|launch| match launch {
                            catalog::Launch::Remote {
                                kind: catalog::RemoteKind::Sse,
                                ..
                            } => mcp::supports(provider, mcp::Kind::Sse),
                            _ => true,
                        })
                    })
                };
                let mut cards: Vec<Card> = registry
                    .servers
                    .iter()
                    .enumerate()
                    .filter(|(index, server)| {
                        (category == Category::All
                            || categories.get(*index).copied().unwrap_or(Category::Other)
                                == category)
                            && server.matches(&words)
                            && reachable(server)
                    })
                    .map(|(index, _)| Card::Catalog(index))
                    .collect();
                // The registry's own search adds the servers that the kept list lacks.
                if let Some((query, Load::Ready(found))) = &registry.found
                    && *query == words.join(" ")
                {
                    for (index, server) in found.iter().enumerate() {
                        let known = registry
                            .servers
                            .iter()
                            .any(|known| known.name == server.name);
                        if !known && reachable(server) {
                            cards.push(Card::Found(index));
                        }
                    }
                }
                let browsing = words.is_empty() && category == Category::All;
                if browsing {
                    rows.push(GridRow::Space(14.));
                    rows.push(GridRow::Banner);
                    let featured: Vec<Card> = FEATURED
                        .iter()
                        .filter_map(|wanted| {
                            // The publisher's own entry names it in its namespace; among the
                            // others the shortest name is the likeliest original.
                            registry
                                .servers
                                .iter()
                                .enumerate()
                                .filter(|(_, server)| {
                                    server.name.contains(wanted) && reachable(server)
                                })
                                .min_by_key(|(_, server)| {
                                    let namespace =
                                        server.name.split('/').next().unwrap_or_default();
                                    (
                                        !namespace
                                            .contains(wanted.split('/').next().unwrap_or(wanted)),
                                        server.name.len(),
                                    )
                                })
                                .map(|(index, _)| index)
                        })
                        .take(columns * 2)
                        .map(Card::Catalog)
                        .collect();
                    if featured.len() >= columns {
                        let shown = featured.len() / columns * columns;
                        rows.push(GridRow::Section {
                            label: "Featured".into(),
                            count: None,
                            note: Some("Popular servers from the registry".into()),
                        });
                        rows.extend(card_rows(featured[..shown].to_vec(), columns, true));
                    }
                } else {
                    rows.push(GridRow::Space(6.));
                }
                let label = match (browsing, category) {
                    (true, _) | (false, Category::All) => "All servers".to_owned(),
                    (false, category) => category.label().to_owned(),
                };
                let note = (registry.read_at > 0 && !registry.reading)
                    .then(|| format!("MCP Registry · read {}", super::age(registry.read_at)));
                rows.push(GridRow::Section {
                    label: if words.is_empty() {
                        label
                    } else {
                        "Results".into()
                    },
                    count: Some(cards.len()),
                    note,
                });
                if cards.is_empty() {
                    rows.push(if registry.reading {
                        GridRow::Loading("READING THE MCP REGISTRY".into())
                    } else if let Some(error) = registry.error.clone().filter(|_| registry.servers.is_empty()) {
                        GridRow::Message {
                            title: "The MCP Registry did not answer".into(),
                            detail: error,
                            error: true,
                        }
                    } else {
                        GridRow::Message {
                            title: "No server matches".into(),
                            detail: "Try other words or another category, or add a custom server from its configuration.".into(),
                            error: false,
                        }
                    });
                }
                let few = cards.len() < 8;
                paged_rows(cards, self.plugins.page, columns, &mut rows);
                let answered = matches!(
                    &registry.found,
                    Some((query, Load::Ready(_) | Load::Failed(_))) if *query == words.join(" ")
                );
                if !words.is_empty() && few && !answered {
                    rows.push(GridRow::SearchRegistry(words.join(" ")));
                }
            }
            Scope::Installed => {
                let mut cards = Vec::new();
                for name in self.plugins.server_names() {
                    let entries = self.plugins.entries(&name);
                    if provider.is_some_and(|provider| {
                        !entries.iter().any(|entry| entry.provider == provider)
                    }) {
                        continue;
                    }
                    let haystack = format!(
                        "{} {}",
                        name.to_lowercase(),
                        entries
                            .first()
                            .map(|entry| entry.server.transport.summary().to_lowercase())
                            .unwrap_or_default()
                    );
                    if words.iter().all(|word| haystack.contains(word)) {
                        cards.push(Card::Installed(name));
                    }
                }
                rows.push(GridRow::Space(6.));
                rows.push(GridRow::Section {
                    label: "Installed servers".into(),
                    count: Some(cards.len()),
                    note: Some("From the configuration of every provider".into()),
                });
                if cards.is_empty() {
                    rows.push(GridRow::Message {
                        title: "No MCP server is installed".into(),
                        detail: "Install one from Discover, or add a custom server.".into(),
                        error: false,
                    });
                }
                paged_rows(cards, self.plugins.page, columns, &mut rows);
            }
        }
        rows.push(GridRow::Space(24.));
        rows
    }

    /// Builds the banner of the servers tab.
    pub(in crate::ui) fn servers_banner(&self) -> Div {
        let registry = &self.plugins.registry;
        let detail = if registry.servers.is_empty() && registry.reading {
            "Reading the official MCP Registry…".to_owned()
        } else {
            format!(
                "{} servers from the official MCP Registry, hosted or run on this computer. Install one for every provider at once, or only for those you pick.",
                registry.servers.len()
            )
        };
        self.plugin_banner(
            "plug",
            "MCP servers",
            detail,
            vec![
                (registry.servers.len().to_string(), "In registry"),
                (self.plugins.server_names().len().to_string(), "Installed"),
                (self.plugins.installed().len().to_string(), "Providers"),
            ],
        )
    }

    /// Returns the icon of a registry server: its own image or its publisher's avatar, else the
    /// glyph of its category.
    pub(in crate::ui) fn server_icon(
        &mut self,
        server: &catalog::CatalogServer,
        category: Category,
        size: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tone = category.tone(self.theme);
        self.plugin_icon(server.image(), category.glyph(), tone, size, cx)
    }

    /// A server of the registry as a card: its icon, name, publisher and description, how it
    /// runs, which providers have it, and its installation.
    pub(in crate::ui) fn catalog_card(
        &mut self,
        id: usize,
        index: usize,
        found: bool,
        featured: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let server = if found {
            match &self.plugins.registry.found {
                Some((_, Load::Ready(found))) => found.get(index).cloned(),
                _ => None,
            }
        } else {
            self.plugins.registry.servers.get(index).cloned()
        };
        let Some(server) = server else {
            return div().flex_1().into_any_element();
        };
        let category = self.server_category(&server, (!found).then_some(index));
        let installed_name = self.plugins.installed_name(&server);
        let marks: Vec<(Provider, Mark)> = installed_name
            .as_ref()
            .map(|name| self.server_marks(name))
            .unwrap_or_default();
        let mut chips: Vec<(SharedString, Hsla)> = Vec::new();
        for launch in &server.launches {
            let kind = match launch {
                catalog::Launch::Remote { .. } => "HOSTED",
                catalog::Launch::Package { registry, .. } => match registry.as_str() {
                    "oci" => "DOCKER",
                    "pypi" => "PYTHON",
                    "nuget" => ".NET",
                    _ => "NPM",
                },
            };
            if !chips.iter().any(|(known, _)| known.as_ref() == kind) {
                chips.push((kind.into(), t.muted));
            }
        }
        let needs_input = server
            .launches
            .first()
            .is_some_and(|launch| launch.inputs().iter().any(|input| input.required));
        let name = server.name.clone();
        let quick = server.clone();
        let action = match &installed_name {
            Some(_) => self
                .ghost(("plugins-manage", id), "settings-2", "Manage")
                .h(px(26.))
                .into_any_element(),
            None => self
                .button(("plugins-install", id), "arrow-down-to-line", "Install")
                .h(px(26.))
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: if needs_input {
                            "Install · asks for the values it needs".into()
                        } else {
                            "Install for every installed provider".into()
                        },
                        theme: t,
                    })
                    .into()
                })
                .on_click(cx.listener(move |s, _, _, cx| {
                    cx.stop_propagation();
                    if needs_input {
                        s.open_plugin_detail(Detail::Catalog(quick.name.clone()), cx);
                    } else {
                        s.quick_install(&quick, cx);
                    }
                }))
                .into_any_element(),
        };
        let icon = self.server_icon(&server, category, if featured { 44. } else { 38. }, cx);
        self.plugin_card(
            ("plugin-server", id),
            CardContent {
                icon,
                title: server.display_name(),
                subtitle: server.publisher(),
                description: server.description.clone(),
                chips,
                marks,
                action,
                installed: installed_name.is_some(),
                tone: category.tone(t),
                featured,
            },
        )
        .on_click(
            cx.listener(move |s, _, _, cx| s.open_plugin_detail(Detail::Catalog(name.clone()), cx)),
        )
        .into_any_element()
    }

    /// An installed server as a card: its name, how it starts, and each provider's state of it.
    pub(in crate::ui) fn installed_card(
        &mut self,
        id: usize,
        name: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let entries = self.plugins.entries(name);
        let Some(first) = entries.first() else {
            return div().flex_1().into_any_element();
        };
        let summary = first.server.transport.summary();
        let kind = first.server.transport.kind();
        let catalog_server = self.plugins.catalog_of(name).cloned();
        let marks = self.server_marks(name);
        let (title, description, icon, tone) = match &catalog_server {
            Some(server) => {
                let tone = Category::of(&format!("{} {}", server.name, server.description)).tone(t);
                (
                    server.display_name(),
                    server.description.clone(),
                    self.server_icon(server, self.server_category(server, None), 38., cx),
                    tone,
                )
            }
            None => {
                let category = Category::of(&format!("{name} {summary}"));
                let tone = category.tone(t);
                let glyph = match kind {
                    mcp::Kind::Stdio => category.glyph(),
                    _ => "projects/server",
                };
                (
                    name.to_owned(),
                    summary.clone(),
                    sidebar::glyph_tile(glyph, tone, 38., 19.),
                    tone,
                )
            }
        };
        let detail = match &catalog_server {
            Some(server) => Detail::Catalog(server.name.clone()),
            None => Detail::Installed(name.to_owned()),
        };
        let chip = match kind {
            mcp::Kind::Stdio => "LOCAL",
            mcp::Kind::Http => "HTTP",
            mcp::Kind::Sse => "SSE",
        };
        let open = detail.clone();
        self.plugin_card(
            ("plugin-installed", id),
            CardContent {
                icon,
                title,
                subtitle: if catalog_server.is_some() {
                    name.to_owned()
                } else {
                    "Custom server".into()
                },
                description,
                chips: vec![(chip.into(), t.muted)],
                marks,
                action: self
                    .ghost(("plugins-installed-manage", id), "settings-2", "Manage")
                    .h(px(26.))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        cx.stop_propagation();
                        s.open_plugin_detail(open.clone(), cx)
                    }))
                    .into_any_element(),
                installed: true,
                tone,
                featured: false,
            },
        )
        .on_click(cx.listener(move |s, _, _, cx| s.open_plugin_detail(detail.clone(), cx)))
        .into_any_element()
    }

    /// The card after a few matches that asks the registry's own search, or the web.
    pub(in crate::ui) fn search_registry_row(
        &mut self,
        index: usize,
        query: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let searching = matches!(
            &self.plugins.registry.found,
            Some((asked, Load::Loading)) if *asked == query
        );
        let asked = query.clone();
        div()
            .id(("plugin-search-registry", index))
            .w_full()
            .mb(px(look::GAP))
            .p(px(14.))
            .flex()
            .items_center()
            .gap(px(14.))
            .rounded(px(crate::theme::RADIUS_SURFACE))
            .border_1()
            .border_dashed()
            .border_color(t.accent.opacity(0.4))
            .bg(kit::tint_fill(t.accent, 0.05))
            .child(kit::icon_tile("globe", t.accent, 38., searching))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .truncate()
                            .text_size(text_px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.text)
                            .child(format!("Not what you need? Look further for “{query}”")),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(text_px(12.))
                            .text_color(t.muted)
                            .child("The registry's own search, or GitHub's repositories with their README configurations."),
                    ),
            )
            .child(if searching {
                kit::loader_row("SEARCHING", t).into_any_element()
            } else {
                self.button(("plugins-search-registry", index), "search", "Search registry")
                    .flex_shrink_0()
                    .on_click(cx.listener(move |s, _, _, cx| s.search_registry(asked.clone(), cx)))
                    .into_any_element()
            })
            .child(
                self.ghost(("plugins-web-search", index), "github", "Search GitHub")
                    .flex_shrink_0()
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.open_custom_server(None, w, cx);
                        if let Some(custom) = &mut s.plugins.custom {
                            custom.page = dialogs::CustomPage::Web;
                        }
                        s.search_web_for(cx);
                    })),
            )
            .into_any_element()
    }

    /// Returns each installed provider's mark for an installed server name.
    pub(in crate::ui) fn server_marks(&self, name: &str) -> Vec<(Provider, Mark)> {
        let mut marks: Vec<(Provider, Mark)> = self
            .plugins
            .entries(name)
            .into_iter()
            .map(|entry| {
                (
                    entry.provider,
                    if entry.enabled { Mark::On } else { Mark::Off },
                )
            })
            .collect();
        // Grok Build also loads the servers of Claude Code's configuration.
        if marks
            .iter()
            .any(|(provider, _)| *provider == Provider::Claude)
            && !marks
                .iter()
                .any(|(provider, _)| *provider == Provider::Grok)
        {
            marks.push((Provider::Grok, Mark::Shared));
        }
        marks
    }

    // Installs a catalog server that needs no values, under its suggested name, for every
    // installed provider that can reach it.
    fn quick_install(&mut self, server: &catalog::CatalogServer, cx: &mut Context<Self>) {
        let Some(launch) = server.launches.first() else {
            return;
        };
        let transport = launch.transport(&[]);
        let installed = self.plugins.installed().to_vec();
        let providers: Vec<Provider> = installed
            .into_iter()
            .filter(|provider| mcp::supports(*provider, transport.kind()))
            .collect();
        if providers.is_empty() {
            self.report_error("No installed provider can reach this server", cx);
            return;
        }
        let name = self.unique_server_name(&mcp::suggested_name(&server.name));
        let target = mcp::Server {
            name: name.clone(),
            transport,
        };
        let count = providers.len();
        self.run_plugin_change(
            format!("Installed {name} for {}", provider_count(count)),
            move |accounts| labelled(mcp::install(&accounts, &providers, &target)),
            cx,
        );
    }

    // Returns `name`, or `name-2`, `name-3`… when another installed server already has it.
    fn unique_server_name(&self, name: &str) -> String {
        let taken = self.plugins.server_names();
        if !taken.iter().any(|known| known == name) {
            return name.to_owned();
        }
        (2..)
            .map(|number| format!("{name}-{number}"))
            .find(|candidate| !taken.contains(candidate))
            .unwrap_or_else(|| name.to_owned())
    }

    /// Builds the detail page of the open server: a hero with its icon and links, then its
    /// description and how it runs beside its installation and what each provider holds.
    pub(in crate::ui) fn server_detail(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let detail = self.plugins.detail.clone();
        let catalog_server = match &detail {
            Some(Detail::Catalog(name)) => self
                .plugins
                .registry
                .servers
                .iter()
                .chain(match &self.plugins.registry.found {
                    Some((_, Load::Ready(found))) => found.as_slice(),
                    _ => &[],
                })
                .find(|server| &server.name == name)
                .cloned(),
            _ => None,
        };
        let installed_name = match (&detail, &catalog_server) {
            (Some(Detail::Installed(name)), _) => Some(name.clone()),
            (_, Some(server)) => self.plugins.installed_name(server),
            _ => None,
        };
        let entry = installed_name
            .as_ref()
            .and_then(|name| self.plugins.entries(name).into_iter().next().cloned());
        let category = match (&catalog_server, &entry) {
            (Some(server), _) => Category::of(&format!(
                "{} {} {}",
                server.name, server.title, server.description
            )),
            (None, Some(entry)) => Category::of(&format!(
                "{} {}",
                entry.server.name,
                entry.server.transport.summary()
            )),
            _ => Category::Other,
        };
        let tone = category.tone(t);
        let title = catalog_server
            .as_ref()
            .map(|server| server.display_name())
            .or_else(|| installed_name.clone())
            .unwrap_or_default();
        let facts = match &catalog_server {
            Some(server) if !server.version.is_empty() => {
                format!(
                    "{} · v{} · {}",
                    server.publisher(),
                    server.version,
                    server.name
                )
            }
            Some(server) => format!("{} · {}", server.publisher(), server.name),
            None => "Custom server · added by you".into(),
        };
        let icon = match &catalog_server {
            Some(server) => self.server_icon(server, self.server_category(server, None), 64., cx),
            None => sidebar::glyph_tile(category.glyph(), tone, 64., 30.),
        };
        let mut chips: Vec<(SharedString, Hsla)> =
            vec![(category.label().to_uppercase().into(), tone)];
        if let Some(server) = &catalog_server {
            for launch in &server.launches {
                let label: SharedString = launch.label().to_uppercase().into();
                if !chips.iter().any(|(known, _)| *known == label) {
                    chips.push((label, t.muted));
                }
            }
        }
        if installed_name.is_some() {
            chips.push(("INSTALLED".into(), t.added));
        }
        let mut actions = div().flex().items_center().gap(px(6.));
        if let Some(server) = &catalog_server {
            for (index, (glyph, label, url)) in [
                ("github", "Repository", server.repository.clone()),
                ("globe", "Website", server.website.clone()),
            ]
            .into_iter()
            .enumerate()
            {
                if url.starts_with("https://") || url.starts_with("http://") {
                    actions = actions.child(
                        self.button(("plugins-server-link", index), glyph, label)
                            .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url))),
                    );
                }
            }
        }
        let wide = self.plugins_content_width() >= 860.;
        let mut main = div().flex_1().min_w_0().flex().flex_col().gap(px(14.));
        if let Some(server) = &catalog_server {
            main = main.child(
                self.detail_card(
                    "About",
                    tone,
                    div()
                        .text_size(text_px(13.))
                        .line_height(relative(1.55))
                        .text_color(t.text)
                        .child(if server.description.is_empty() {
                            "The publisher gave no description.".to_owned()
                        } else {
                            server.description.clone()
                        }),
                ),
            );
            main = main.child(self.launches_card(server));
        }
        if let Some(entry) = &entry
            && catalog_server.is_none()
        {
            main = main.child(self.transport_block(&entry.server.transport));
        }
        let mut side = div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(14.))
            .when(wide, |side| side.w(px(380.)).flex_shrink_0())
            .when(!wide, |side| side.w_full());
        if let Some(server) = &catalog_server {
            side = side.child(self.install_block(server, installed_name.is_some(), window, cx));
        }
        if let Some(name) = &installed_name {
            side = side.child(self.installed_block(name, catalog_server.is_none(), cx));
        }
        let body = if wide {
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(px(14.))
                .child(main)
                .child(side)
        } else {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(side)
                .child(main)
        };
        let page = div()
            .w_full()
            .pt(px(16.))
            .pb(px(32.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(self.detail_hero(icon, title, facts, chips, actions, tone))
            .child(body);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.detail_bar("MCP servers", div(), cx))
            .child(
                div()
                    .id("plugins-server-detail")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.plugins.detail_scroll)
                    .child(self.centered(page)),
            )
            .into_any_element()
    }

    // The ways that a registry server runs: each launch with its command or address and what it
    // asks for.
    fn launches_card(&self, server: &catalog::CatalogServer) -> Div {
        let t = self.theme;
        let mut rows = div().flex().flex_col().gap(px(10.));
        for launch in &server.launches {
            let (glyph, line) = match launch {
                catalog::Launch::Package { command, args, .. } => {
                    ("square-terminal", format!("{command} {}", args.join(" ")))
                }
                catalog::Launch::Remote { url, .. } => ("globe", url.clone()),
            };
            let inputs = launch.inputs();
            rows =
                rows.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(icon(glyph).size(px(13.)).text_color(t.accent))
                                .child(caps(launch.label(), t).text_color(t.text)),
                        )
                        .child(
                            kit::block(t).child(
                                div()
                                    .px(px(10.))
                                    .py(px(8.))
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(11.5))
                                    .text_color(t.text)
                                    .child(line),
                            ),
                        )
                        .when(!inputs.is_empty(), |d| {
                            d.child(div().flex().flex_wrap().gap(px(5.)).children(
                                inputs.iter().map(|input| {
                                    kit::chip(
                                        input.name.clone(),
                                        if input.required { t.warning } else { t.muted },
                                    )
                                }),
                            ))
                        }),
                );
        }
        self.detail_card("How it runs", t.accent, rows)
    }

    /// Builds the bar above a detail page: the way back to the list, and the page's links.
    pub(in crate::ui) fn detail_bar(
        &self,
        back: &'static str,
        links: Div,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        div()
            .flex_shrink_0()
            .h(px(42.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(t.border.opacity(0.6))
            .child(
                self.ghost("plugins-back", "arrow-left", back)
                    .on_click(cx.listener(|s, _, _, cx| s.close_plugin_detail(cx))),
            )
            .child(div().flex_1())
            .child(links)
    }

    // What each provider holds of an installed server: its files, its switch and its removal.
    fn installed_block(&mut self, name: &str, custom: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let entries: Vec<mcp::Entry> = self.plugins.entries(name).into_iter().cloned().collect();
        let busy = self.plugins.changes > 0;
        let mut rows = div().flex().flex_col();
        for (index, entry) in entries.iter().enumerate() {
            let provider = entry.provider;
            let files = entry
                .files
                .iter()
                .map(|file| backend::display_path(file))
                .collect::<Vec<_>>()
                .join(" · ");
            let remove_name = name.to_owned();
            let switch_name = name.to_owned();
            rows = rows.child(
                div()
                    .px(px(12.))
                    .py(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .when(index > 0, |d| {
                        d.border_t_1().border_color(t.border.opacity(0.6))
                    })
                    .child(kit::icon_tile(
                        provider.icon(),
                        if entry.enabled { t.accent } else { t.subtle },
                        26.,
                        false,
                    ))
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
                                    .child(provider.label())
                                    .when(!entry.enabled, |d| d.child(kit::chip("OFF", t.subtle))),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .text_ellipsis_start()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(10.5))
                                    .text_color(t.subtle)
                                    .child(files),
                            ),
                    )
                    .when(mcp::switchable(provider), |d| {
                        d.child(
                            controls::Switch::new(
                                format!("plugins-server-on-{index}"),
                                entry.enabled,
                            )
                            .label(format!("Use {name} in {}", provider.label()))
                            .disabled(busy)
                            .on_click(cx.listener(
                                move |s, enabled: &bool, _, cx| {
                                    let name = switch_name.clone();
                                    let enabled = *enabled;
                                    s.run_plugin_change(
                                        format!(
                                            "{} {name} in {}",
                                            if enabled { "Turned on" } else { "Turned off" },
                                            provider.label()
                                        ),
                                        move |accounts| {
                                            mcp::set_enabled(&accounts, provider, &name, enabled)
                                                .err()
                                                .map(|error| {
                                                    vec![format!("{}: {error:#}", provider.label())]
                                                })
                                                .unwrap_or_default()
                                        },
                                        cx,
                                    );
                                },
                            )),
                        )
                    })
                    .child(
                        self.icon_button(
                            ("plugins-server-remove", index),
                            "trash-2",
                            "Remove for this provider",
                        )
                        .disabled(busy)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let name = remove_name.clone();
                            s.run_plugin_change(
                                format!("Removed {name} from {}", provider.label()),
                                move |accounts| {
                                    labelled(mcp::remove(&accounts, &[provider], &name))
                                },
                                cx,
                            );
                        })),
                    ),
            );
        }
        let all: Vec<Provider> = entries.iter().map(|entry| entry.provider).collect();
        let remove_all = name.to_owned();
        let edit_name = name.to_owned();
        let actions = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .when(custom, |d| {
                d.child(
                    self.button("plugins-server-edit", "pencil", "Edit")
                        .disabled(busy)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.open_custom_server(Some(edit_name.clone()), w, cx)
                        })),
                )
            })
            .child(
                self.tinted(
                    "plugins-server-remove-all",
                    "trash-2",
                    "Remove everywhere",
                    t.removed,
                )
                .disabled(busy)
                .on_click(cx.listener(move |s, _, _, cx| {
                    let name = remove_all.clone();
                    let providers = all.clone();
                    s.run_plugin_change(
                        format!("Removed {name}"),
                        move |accounts| labelled(mcp::remove(&accounts, &providers, &name)),
                        cx,
                    );
                })),
            );
        self.detail_card(
            &format!("Installed for {}", provider_count(entries.len())),
            t.added,
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(kit::block(t).child(rows))
                .child(actions),
        )
        .into_any_element()
    }

    // The configuration of a server as a readout: how it starts, its arguments, and the names
    // of its variables or headers. Values stay hidden, because they often hold secrets.
    fn transport_block(&self, transport: &mcp::Transport) -> Div {
        let t = self.theme;
        let mut lines: Vec<(&'static str, String)> =
            vec![("Transport", transport.kind().label().into())];
        match transport {
            mcp::Transport::Stdio { command, args, env } => {
                lines.push(("Command", command.clone()));
                if !args.is_empty() {
                    lines.push(("Arguments", args.join(" ")));
                }
                if !env.is_empty() {
                    lines.push((
                        "Environment",
                        env.iter()
                            .map(|(key, _)| format!("{key}=••••"))
                            .collect::<Vec<_>>()
                            .join("  "),
                    ));
                }
            }
            mcp::Transport::Http { url, headers } | mcp::Transport::Sse { url, headers } => {
                lines.push(("Address", url.clone()));
                if !headers.is_empty() {
                    lines.push((
                        "Headers",
                        headers
                            .iter()
                            .map(|(key, _)| format!("{key}: ••••"))
                            .collect::<Vec<_>>()
                            .join("  "),
                    ));
                }
            }
        }
        let mut body = div().px(px(12.)).py(px(10.)).flex().flex_col().gap(px(6.));
        for (label, value) in lines {
            body = body.child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(div().w(px(92.)).flex_shrink_0().child(caps(label, t)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(11.5))
                            .text_color(t.text)
                            .child(value),
                    ),
            );
        }
        kit::block(t)
            .child(kit::block_header("Configuration", t.accent, t))
            .child(body)
    }

    // The installation form of a catalog server: how it runs, its name, the values it asks for
    // and the providers that receive it.
    fn install_block(
        &mut self,
        server: &catalog::CatalogServer,
        installed: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let Some(form) = self.plugins.form.as_ref() else {
            return div().into_any_element();
        };
        let launch_index = form.launch.min(server.launches.len().saturating_sub(1));
        let Some(launch) = server.launches.get(launch_index).cloned() else {
            return div().into_any_element();
        };
        let chosen = form.providers.clone();
        let running = form.running;
        let name_field = form.name.clone();
        let inputs: Vec<(catalog::Input, Entity<TextInput>)> = form.inputs.clone();
        let kind = launch.transport(&[]).kind();
        let mut launches = div()
            .flex()
            .flex_wrap()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .border_1()
            .border_color(t.edge)
            .bg(t.inset);
        for (index, option) in server.launches.iter().enumerate() {
            let picked = index == launch_index;
            let server_for_click = server.clone();
            launches = launches.child(
                self.control(("plugins-launch", index))
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
                        let Some(detail) = s.plugins.detail.clone() else {
                            return;
                        };
                        let form = s.server_form(&detail, &server_for_click, index, cx);
                        s.plugins.form = Some(form);
                        cx.notify();
                    })),
            );
        }
        let mut fields = div().flex().flex_col().gap(px(10.)).child(self.form_field(
            "Name",
            "The name under which every CLI lists the server",
            name_field.into_any_element(),
            false,
        ));
        for (input, field) in &inputs {
            let detail = if input.description.is_empty() {
                input.name.clone()
            } else {
                input.description.clone()
            };
            fields = fields.child(self.form_field_owned(
                input.name.clone(),
                detail,
                field.clone().into_any_element(),
                input.required,
            ));
        }
        let count = chosen
            .iter()
            .filter(|provider| mcp::supports(**provider, kind))
            .count();
        let target = server.clone();
        let body = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .when(server.launches.len() > 1, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(caps("Runs as", t))
                        .child(launches),
                )
            })
            .child(fields)
            .child(div().pt(px(4.)).child(caps("Providers", t)))
            .child(self.provider_choices(
                "plugins-server-provider",
                &chosen,
                move |provider| {
                    (!mcp::supports(provider, kind))
                        .then(|| format!("Cannot reach a server over {}", kind.label()))
                },
                |s, provider, on| {
                    if let Some(form) = &mut s.plugins.form {
                        form.providers.retain(|known| *known != provider);
                        if on {
                            form.providers.push(provider);
                        }
                    }
                },
                cx,
            ))
            .child(
                div()
                    .pt(px(6.))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .when(running, |d| d.child(kit::loader_row("INSTALLING", t)))
                    .child(
                        self.primary(
                            "plugins-server-install",
                            "arrow-down-to-line",
                            format!("Install for {}", provider_count(count)),
                        )
                        .disabled(running || count == 0)
                        .on_click(
                            cx.listener(move |s, _, _, cx| s.install_catalog_server(&target, cx)),
                        ),
                    ),
            );
        self.detail_card(
            if installed {
                "Install again"
            } else {
                "Install"
            },
            t.accent,
            body,
        )
        .into_any_element()
    }

    // Installs the open catalog server with the form's choices.
    fn install_catalog_server(&mut self, server: &catalog::CatalogServer, cx: &mut Context<Self>) {
        let Some(form) = self.plugins.form.as_mut() else {
            return;
        };
        let Some(launch) = server.launches.get(form.launch) else {
            return;
        };
        let name = form.name.read(cx).text().trim().to_owned();
        if let Err(error) = mcp::validate_name(&name) {
            self.report_error(format!("{error:#}"), cx);
            return;
        }
        let values: Vec<(String, String)> = form
            .inputs
            .iter()
            .map(|(input, field)| (input.name.clone(), field.read(cx).text()))
            .collect();
        if let Some((input, _)) = form
            .inputs
            .iter()
            .zip(&values)
            .map(|((input, _), (_, value))| (input, value))
            .find(|(input, value)| {
                input.required && value.trim().is_empty() && input.default.is_empty()
            })
        {
            let message = format!("Enter a value for {}", input.name);
            self.report_error(message, cx);
            return;
        }
        let transport = launch.transport(&values);
        let providers: Vec<Provider> = form
            .providers
            .iter()
            .copied()
            .filter(|provider| mcp::supports(*provider, transport.kind()))
            .collect();
        form.running = true;
        let count = providers.len();
        let target = mcp::Server {
            name: name.clone(),
            transport,
        };
        self.run_plugin_change(
            format!("Installed {name} for {}", provider_count(count)),
            move |accounts| labelled(mcp::install(&accounts, &providers, &target)),
            cx,
        );
    }

    /// Builds one field of a form: its label, a mark when it is required, a line that explains
    /// it, and its input.
    pub(in crate::ui) fn form_field(
        &self,
        label: &'static str,
        detail: &'static str,
        input: AnyElement,
        required: bool,
    ) -> Div {
        self.form_field_owned(label.into(), detail.into(), input, required)
    }

    /// Builds one field of a form whose label and explanation are computed.
    pub(in crate::ui) fn form_field_owned(
        &self,
        label: String,
        detail: String,
        input: AnyElement,
        required: bool,
    ) -> Div {
        let t = self.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.text)
                            .child(label),
                    )
                    .when(required, |d| d.child(kit::chip("REQUIRED", t.warning))),
            )
            .child(
                div()
                    .text_size(text_px(11.5))
                    .line_height(relative(1.4))
                    .text_color(t.subtle)
                    .child(detail),
            )
            .child(
                settings::input_frame(t).border_color(t.border).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(crate::theme::mono())
                        .text_size(text_px(12.))
                        .child(input),
                ),
            )
    }
}

/// Returns "1 provider" or "N providers".
pub(in crate::ui) fn provider_count(count: usize) -> String {
    if count == 1 {
        "1 provider".into()
    } else {
        format!("{count} providers")
    }
}
