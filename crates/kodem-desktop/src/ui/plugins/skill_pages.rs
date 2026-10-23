//! The Skills tab: the installed skills and the skills of the repositories, a skill's detail
//! with its `SKILL.md`, and which providers reach it.
use super::*;

impl Shell {
    /// Builds the rows of the skills tab: in Discover, the banner and the skills of the open
    /// source; in Installed, the installed skills.
    pub(in crate::ui) fn skill_rows(
        &mut self,
        columns: usize,
        cx: &mut Context<Self>,
    ) -> Vec<GridRow> {
        let words = self.plugins.query(cx);
        let provider = self.plugins.provider;
        let matches = |name: &str, description: &str| {
            let haystack = format!("{} {}", name.to_lowercase(), description.to_lowercase());
            words.iter().all(|word| haystack.contains(word))
        };
        let mut rows = Vec::new();
        match self.plugins.scope {
            Scope::Installed => {
                let inventory = &self.plugins.skills;
                let cards: Vec<Card> = inventory
                    .skills
                    .iter()
                    .enumerate()
                    .filter(|(_, skill)| {
                        provider.is_none_or(|provider| {
                            inventory.reach(skill, provider) != skills::Reach::Missing
                        }) && matches(&skill.name, &skill.description)
                    })
                    .map(|(index, _)| Card::Skill(index))
                    .collect();
                rows.push(GridRow::Space(6.));
                rows.push(GridRow::Section {
                    label: "Installed skills".into(),
                    count: Some(cards.len()),
                    note: Some("From every provider's skill folder".into()),
                });
                if cards.is_empty() {
                    rows.push(GridRow::Message {
                        title: "No skill matches".into(),
                        detail: "Install one from Discover, or add a skill from GitHub, a folder or your own instructions.".into(),
                        error: false,
                    });
                }
                paged_rows(cards, self.plugins.page, columns, &mut rows);
            }
            Scope::Discover => {
                if words.is_empty() {
                    rows.push(GridRow::Space(14.));
                    rows.push(GridRow::Banner);
                } else {
                    rows.push(GridRow::Space(6.));
                }
                let source = match self.plugins.source {
                    Source::Directory => {
                        self.directory_rows(&words, columns, &mut rows);
                        rows.push(GridRow::Space(24.));
                        return rows;
                    }
                    Source::Repository(index) => index,
                };
                let label = catalog::SKILL_SOURCES
                    .get(source)
                    .map(|source| {
                        format!("{} · {}/{}", source.label, source.owner, source.repository)
                    })
                    .unwrap_or_default();
                match self.plugins.catalog_skills.get(&source) {
                    Some(Load::Ready(found)) => {
                        let cards: Vec<Card> = found
                            .iter()
                            .enumerate()
                            .filter(|(_, skill)| matches(&skill.name, &skill.description))
                            .map(|(index, _)| Card::CatalogSkill(index))
                            .collect();
                        rows.push(GridRow::Section {
                            label,
                            count: Some(cards.len()),
                            note: Some("Other sources are in the sidebar".into()),
                        });
                        if cards.is_empty() {
                            rows.push(GridRow::Message {
                                title: "No skill matches".into(),
                                detail: "Try other words, or choose another source in the sidebar."
                                    .into(),
                                error: false,
                            });
                        }
                        paged_rows(cards, self.plugins.page, columns, &mut rows);
                    }
                    Some(Load::Failed(error)) => {
                        rows.push(GridRow::Section {
                            label,
                            count: None,
                            note: None,
                        });
                        rows.push(GridRow::Message {
                            title: "The skills could not be read".into(),
                            detail: error.clone(),
                            error: true,
                        });
                    }
                    _ => {
                        rows.push(GridRow::Section {
                            label,
                            count: None,
                            note: None,
                        });
                        rows.push(GridRow::Loading(
                            "DOWNLOADING THE REPOSITORY'S SKILLS".into(),
                        ));
                    }
                }
            }
        }
        rows.push(GridRow::Space(24.));
        rows
    }

    /// Builds the banner of the skills tab.
    pub(in crate::ui) fn skills_banner(&self) -> Div {
        let shared = self
            .plugins
            .skills
            .skills
            .iter()
            .filter(|skill| skill.copies.len() > 1)
            .count();
        self.plugin_banner(
            "book",
            "Skills",
            "Instructions, scripts and resources that agents load when a task needs them. Browse thousands of skills from the skills.sh directory or Anthropic's and OpenAI's collections, install them for any of your providers, or bring your own.".into(),
            vec![
                (self.plugins.skills.skills.len().to_string(), "Installed"),
                (shared.to_string(), "In several folders"),
                (self.plugins.installed().len().to_string(), "Providers"),
            ],
        )
    }

    /// Returns each installed provider's mark for a skill name.
    pub(in crate::ui) fn skill_marks(&self, name: &str) -> Vec<(Provider, Mark)> {
        let inventory = &self.plugins.skills;
        let Some(skill) = inventory.skills.iter().find(|skill| skill.name == name) else {
            return Vec::new();
        };
        self.plugins
            .installed()
            .iter()
            .map(|provider| {
                (
                    *provider,
                    match inventory.reach(skill, *provider) {
                        skills::Reach::Own => Mark::On,
                        skills::Reach::Shared(_) => Mark::Shared,
                        skills::Reach::Missing => Mark::Missing,
                    },
                )
            })
            .collect()
    }

    /// Returns the tile of a skill: its glyph, in the tone of the source it came from.
    pub(in crate::ui) fn skill_tile(&self, name: &str, description: &str, size: f32) -> AnyElement {
        let t = self.theme;
        let glyph = look::skill_glyph(name, description);
        let tone = match glyph {
            "projects/file-text" | "projects/presentation" | "projects/table" => t.info,
            "projects/palette" | "projects/wand-sparkles" => t.removed,
            "projects/flask-conical" | "projects/shield" => t.warning,
            "projects/cloud" | "projects/database" | "projects/globe" => t.added,
            _ => t.accent,
        };
        sidebar::glyph_tile(glyph, tone, size, (size * 0.5).round())
    }

    /// An installed skill as a card: its name, its folders, its description and who reaches it.
    pub(in crate::ui) fn installed_skill_card(
        &mut self,
        id: usize,
        at: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let Some(skill) = self.plugins.skills.skills.get(at).cloned() else {
            return div().flex_1().into_any_element();
        };
        let folders = skill
            .copies
            .iter()
            .map(|copy| self.plugins.skills.folders[copy.folder].label())
            .collect::<Vec<_>>()
            .join(" · ");
        let marks = self.skill_marks(&skill.name);
        let name = skill.name.clone();
        let open = skill.name.clone();
        let mut chips: Vec<(SharedString, Hsla)> = Vec::new();
        if skill.copies.len() > 1 {
            chips.push((format!("{} COPIES", skill.copies.len()).into(), t.muted));
        }
        self.plugin_card(
            ("plugin-skill", id),
            CardContent {
                icon: self.skill_tile(&skill.name, &skill.description, 38.),
                title: skill.name.clone(),
                subtitle: folders,
                description: skill.description.clone(),
                chips,
                marks,
                action: self
                    .ghost(("plugins-skill-manage", id), "settings-2", "Manage")
                    .h(px(26.))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        cx.stop_propagation();
                        s.open_plugin_detail(Detail::Skill(open.clone()), cx)
                    }))
                    .into_any_element(),
                installed: true,
                tone: t.accent,
                featured: false,
            },
        )
        .on_click(
            cx.listener(move |s, _, _, cx| s.open_plugin_detail(Detail::Skill(name.clone()), cx)),
        )
        .into_any_element()
    }

    /// A skill of a repository as a card: who publishes it, its description, who has it already,
    /// and its installation.
    pub(in crate::ui) fn catalog_skill_card(
        &mut self,
        id: usize,
        at: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let Source::Repository(source) = self.plugins.source else {
            return div().flex_1().into_any_element();
        };
        let Some(skill) = (match self.plugins.catalog_skills.get(&source) {
            Some(Load::Ready(found)) => found.get(at).cloned(),
            _ => None,
        }) else {
            return div().flex_1().into_any_element();
        };
        let installed = self
            .plugins
            .skills
            .skills
            .iter()
            .any(|known| known.name == skill.name);
        let marks = if installed {
            self.skill_marks(&skill.name)
        } else {
            Vec::new()
        };
        let publisher = catalog::SKILL_SOURCES
            .get(source)
            .map(|source| source.label)
            .unwrap_or_default();
        let name = skill.name.clone();
        let quick = skill.clone();
        let action = if installed {
            self.ghost(("plugins-skill-installed", id), "settings-2", "Manage")
                .h(px(26.))
                .into_any_element()
        } else {
            self.button(
                ("plugins-skill-install", id),
                "arrow-down-to-line",
                "Install",
            )
            .h(px(26.))
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: "Install for every installed provider".into(),
                    theme: t,
                })
                .into()
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                cx.stop_propagation();
                let providers = s.plugins.installed().to_vec();
                s.install_catalog_skill(quick.clone(), quick.name.clone(), providers, cx);
            }))
            .into_any_element()
        };
        self.plugin_card(
            ("plugin-catalog-skill", id),
            CardContent {
                icon: self.skill_tile(&skill.name, &skill.description, 38.),
                title: skill.name.clone(),
                subtitle: format!("{publisher} · {}", skill.path),
                description: skill.description.clone(),
                chips: vec![(publisher.to_uppercase().into(), t.muted)],
                marks,
                action,
                installed,
                tone: t.accent,
                featured: false,
            },
        )
        .on_click(cx.listener(move |s, _, _, cx| {
            if installed {
                s.plugins.scope = Scope::Installed;
                s.open_plugin_detail(Detail::Skill(name.clone()), cx)
            } else {
                s.open_plugin_detail(Detail::CatalogSkill(source, name.clone()), cx)
            }
        }))
        .into_any_element()
    }

    // Copies a repository's skill into the folders of `providers` under `name`.
    fn install_catalog_skill(
        &mut self,
        skill: catalog::CatalogSkill,
        name: String,
        providers: Vec<Provider>,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = skills::validate_name(&name) {
            self.report_error(format!("{error:#}"), cx);
            return;
        }
        if providers.is_empty() {
            self.report_error("Choose at least one provider", cx);
            return;
        }
        if let Some(form) = &mut self.plugins.form {
            form.running = true;
        }
        let count = providers.len();
        self.run_plugin_change(
            format!("Installed {name} for {}", servers::provider_count(count)),
            // The skill holds its unpacked repository until the copy is done.
            move |_| labelled(skills::install(&skill.folder, &name, &providers)),
            cx,
        );
    }

    /// Builds the detail page of the open skill.
    pub(in crate::ui) fn skill_detail(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut links = div().flex().items_center().gap(px(2.));
        let page = match self.plugins.detail.clone() {
            Some(Detail::Skill(name)) => {
                let Some(skill) = self
                    .plugins
                    .skills
                    .skills
                    .iter()
                    .find(|skill| skill.name == name)
                    .cloned()
                else {
                    return div().into_any_element();
                };
                if let Some(copy) = skill.copies.first() {
                    let reveal = copy.path.clone();
                    links = links.child(
                        self.button("plugins-skill-reveal", "folder-open", reveal_label())
                            .on_click(cx.listener(move |_, _, _, cx| cx.reveal_path(&reveal))),
                    );
                }
                self.installed_skill_page(&skill, links, window, cx)
            }
            Some(Detail::CatalogSkill(source, name)) => {
                let Some(skill) = (match self.plugins.catalog_skills.get(&source) {
                    Some(Load::Ready(found)) => {
                        found.iter().find(|skill| skill.name == name).cloned()
                    }
                    _ => None,
                }) else {
                    return div().into_any_element();
                };
                let url = skill.url.clone();
                links = links.child(
                    self.button("plugins-skill-github", "github", "Open on GitHub")
                        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url))),
                );
                self.catalog_skill_page(source, &skill, links, window, cx)
            }
            Some(Detail::DirectorySkill(id)) => {
                let Some(skill) = self.plugins.directory.skill(&id).cloned() else {
                    return div().into_any_element();
                };
                self.directory_skill_page(&skill, window, cx)
            }
            _ => div().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.detail_bar("Skills", div(), cx))
            .child(
                div()
                    .id("plugins-skill-detail")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.plugins.detail_scroll)
                    .child(self.centered(page)),
            )
            .into_any_element()
    }

    // Lays out a skill's page: its hero, then its `SKILL.md` beside the card of its providers,
    // or under it in a narrow window.
    pub(in crate::ui) fn skill_page_layout(
        &self,
        hero: Div,
        main: AnyElement,
        side: AnyElement,
    ) -> AnyElement {
        let wide = self.plugins_content_width() >= 860.;
        let body = if wide {
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(px(14.))
                .child(div().flex_1().min_w_0().child(main))
                .child(div().w(px(380.)).flex_shrink_0().child(side))
        } else {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(side)
                .child(main)
        };
        div()
            .w_full()
            .pt(px(16.))
            .pb(px(32.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(hero)
            .child(body)
            .into_any_element()
    }

    // The page of an installed skill: who reaches it, with a switch per provider, its removal,
    // and its `SKILL.md` and files.
    fn installed_skill_page(
        &mut self,
        skill: &skills::Skill,
        actions: Div,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let busy = self.plugins.changes > 0;
        let inventory = self.plugins.skills.clone();
        let mut rows = div().flex().flex_col();
        for (index, provider) in self.plugins.installed().to_vec().into_iter().enumerate() {
            let reach = inventory.reach(skill, provider);
            let (detail, on) = match reach {
                skills::Reach::Own => (
                    inventory
                        .own_copy(skill, provider)
                        .map(|copy| backend::display_path(&copy.path))
                        .unwrap_or_default(),
                    true,
                ),
                skills::Reach::Shared(folder) => {
                    (format!("Reads it from the {folder} folder"), true)
                }
                skills::Reach::Missing => ("Not installed".to_owned(), false),
            };
            let shared = matches!(reach, skills::Reach::Shared(_));
            let target = skill.clone();
            let toggled = inventory.clone();
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
                        if on { t.accent } else { t.subtle },
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
                                    .when(shared, |d| d.child(kit::chip("SHARED", t.info))),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .text_ellipsis_start()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(10.5))
                                    .text_color(t.subtle)
                                    .child(detail),
                            ),
                    )
                    .child(
                        controls::Switch::new(format!("plugins-skill-on-{index}"), on)
                            .label(format!("{} for {}", skill.name, provider.label()))
                            // A skill that a provider reads from another's folder goes with that
                            // folder's copy.
                            .disabled(busy || shared)
                            .on_click(cx.listener(move |s, enabled: &bool, _, cx| {
                                let skill = target.clone();
                                let inventory = toggled.clone();
                                let name = skill.name.clone();
                                if *enabled {
                                    let Some(source) =
                                        skill.copies.first().map(|copy| copy.path.clone())
                                    else {
                                        return;
                                    };
                                    s.run_plugin_change(
                                        format!("Installed {name} for {}", provider.label()),
                                        move |_| {
                                            labelled(skills::install(&source, &name, &[provider]))
                                        },
                                        cx,
                                    );
                                } else {
                                    s.run_plugin_change(
                                        format!("Removed {name} from {}", provider.label()),
                                        move |_| {
                                            labelled(skills::remove(
                                                &inventory,
                                                &skill,
                                                &[provider],
                                            ))
                                        },
                                        cx,
                                    );
                                }
                            })),
                    ),
            );
        }
        let everywhere = skill.clone();
        let folder = skill.copies.first().map(|copy| copy.path.clone());
        let folders = skill
            .copies
            .iter()
            .map(|copy| backend::display_path(&copy.path))
            .collect::<Vec<_>>()
            .join(" · ");
        let remove = self
            .tinted(
                "plugins-skill-remove-all",
                "trash-2",
                "Remove everywhere",
                t.removed,
            )
            .disabled(busy)
            .on_click(cx.listener(move |s, _, _, cx| {
                let skill = everywhere.clone();
                let name = skill.name.clone();
                s.run_plugin_change(
                    format!("Removed {name}"),
                    move |_| {
                        skills::remove_everywhere(&skill)
                            .err()
                            .map(|error| vec![format!("{error:#}")])
                            .unwrap_or_default()
                    },
                    cx,
                );
            }));
        let side = self
            .detail_card(
                "Available to",
                t.added,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(kit::block(t).child(rows))
                    .child(div().flex().justify_end().child(remove)),
            )
            .into_any_element();
        let main = match folder {
            Some(folder) => self.manifest_block(&folder, window, cx),
            None => div().into_any_element(),
        };
        let mut chips: Vec<(SharedString, Hsla)> = vec![("INSTALLED".into(), t.added)];
        if skill.copies.len() > 1 {
            chips.push((format!("{} COPIES", skill.copies.len()).into(), t.muted));
        }
        let hero = self.detail_hero(
            self.skill_tile(&skill.name, &skill.description, 64.),
            skill.name.clone(),
            folders,
            chips,
            actions,
            t.accent,
        );
        let hero = hero.when(!skill.description.is_empty(), |hero| {
            hero.child(
                div()
                    .relative()
                    .px(px(20.))
                    .pb(px(18.))
                    .text_size(text_px(13.))
                    .line_height(relative(1.55))
                    .text_color(t.muted)
                    .child(skill.description.clone()),
            )
        });
        self.skill_page_layout(hero, main, side)
    }

    // The page of a repository's skill: its installation form and its `SKILL.md`.
    fn catalog_skill_page(
        &mut self,
        source: usize,
        skill: &catalog::CatalogSkill,
        actions: Div,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let installed = self
            .plugins
            .skills
            .skills
            .iter()
            .any(|known| known.name == skill.name);
        let target = skill.clone();
        let side = self.skill_install_card(
            installed,
            move |s, name, providers, cx| {
                s.install_catalog_skill(target.clone(), name, providers, cx)
            },
            cx,
        );
        let publisher = catalog::SKILL_SOURCES
            .get(source)
            .map(|source| source.label)
            .unwrap_or_default();
        let main = self.manifest_block(&skill.folder, window, cx);
        let mut chips: Vec<(SharedString, Hsla)> = vec![(publisher.to_uppercase().into(), t.muted)];
        if installed {
            chips.push(("INSTALLED".into(), t.added));
        }
        let hero = self
            .detail_hero(
                self.skill_tile(&skill.name, &skill.description, 64.),
                skill.name.clone(),
                format!("{publisher} · {}", skill.path),
                chips,
                actions,
                t.accent,
            )
            .when(!skill.description.is_empty(), |hero| {
                hero.child(
                    div()
                        .relative()
                        .px(px(20.))
                        .pb(px(18.))
                        .text_size(text_px(13.))
                        .line_height(relative(1.55))
                        .text_color(t.muted)
                        .child(skill.description.clone()),
                )
            });
        self.skill_page_layout(hero, main, side)
    }

    /// The installation card of a skill's page: the name of its folder and a switch per
    /// installed provider. Its button hands the name and the chosen providers to `install`.
    pub(in crate::ui) fn skill_install_card(
        &mut self,
        installed: bool,
        install: impl Fn(&mut Shell, String, Vec<Provider>, &mut Context<Shell>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let Some(form) = self.plugins.form.as_ref() else {
            return div().into_any_element();
        };
        let chosen = form.providers.clone();
        let running = form.running;
        let name_field = form.name.clone();
        let form_body = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(self.form_field(
                "Name",
                "The folder of the skill in each provider's skill folder",
                name_field.clone().into_any_element(),
                false,
            ))
            .child(div().pt(px(4.)).child(caps("Providers", t)))
            .child(self.provider_choices(
                "plugins-skill-provider",
                &chosen,
                |_| None,
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
                            "plugins-skill-install-form",
                            "arrow-down-to-line",
                            format!("Install for {}", servers::provider_count(chosen.len())),
                        )
                        .disabled(running || chosen.is_empty())
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let name = name_field.read(cx).text().trim().to_owned();
                            let providers = s
                                .plugins
                                .form
                                .as_ref()
                                .map(|form| form.providers.clone())
                                .unwrap_or_default();
                            install(s, name, providers, cx);
                        })),
                    ),
            );
        self.detail_card(
            if installed {
                "Install again"
            } else {
                "Install"
            },
            t.accent,
            form_body,
        )
        .into_any_element()
    }

    // The `SKILL.md` of a skill folder as selectable text, with the folder's other files.
    pub(in crate::ui) fn manifest_block(
        &mut self,
        folder: &std::path::Path,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        match self.plugins.manifests.get(folder).cloned() {
            Some(Load::Ready((text, files))) => {
                let key = format!("plugins-manifest:{}", folder.display());
                let viewer = self.readonly(key, text, cx);
                // Markdown colors, with lines wrapped to the card: prose has long lines.
                viewer.update(cx, |viewer, _| {
                    viewer.code("md", self.theme.id);
                    viewer.set_wrap(true);
                });
                let others: Vec<String> = files
                    .into_iter()
                    .filter(|file| file != "SKILL.md")
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        kit::block(t)
                            .child(kit::block_header("SKILL.md", t.accent, t))
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .px(px(12.))
                                    .py(px(10.))
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(11.5))
                                    .child(viewer),
                            ),
                    )
                    .when(!others.is_empty(), |d| {
                        let count = others.len();
                        let mut list = div().px(px(12.)).py(px(8.)).flex().flex_col().gap(px(3.));
                        for file in others.into_iter().take(60) {
                            list = list.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(7.))
                                    .child(icon("file").size(px(12.)).text_color(t.subtle))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .text_ellipsis_start()
                                            .font_family(crate::theme::mono())
                                            .text_size(text_px(11.))
                                            .text_color(t.muted)
                                            .child(file),
                                    ),
                            );
                        }
                        d.child(
                            kit::block(t)
                                .child(
                                    kit::block_header("Files", t.muted, t)
                                        .child(figure(count.to_string(), t.subtle)),
                                )
                                .child(list),
                        )
                    })
                    .into_any_element()
            }
            Some(Load::Failed(error)) => {
                settings::error_block("SKILL.md could not be read", error, t).into_any_element()
            }
            _ => kit::loader_row("Reading SKILL.md…", t).into_any_element(),
        }
    }
}

/// Returns the name of the action that shows a folder in the system's file manager.
fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Show in Finder"
    } else if cfg!(windows) {
        "Show in Explorer"
    } else {
        "Show folder"
    }
}
