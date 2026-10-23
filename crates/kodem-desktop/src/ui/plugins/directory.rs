//! The skills.sh directory in the Skills tab: its list of the most installed skills, kept
//! between starts, its search, the descriptions of the cards in view, and a skill's page and
//! installation from its GitHub repository.
use super::*;

/// How many descriptions download at once.
const DESCRIPTION_JOBS: usize = 4;

/// How long the search waits for more typing before it asks skills.sh.
const SEARCH_DELAY: Duration = Duration::from_millis(350);

/// The skills.sh directory, and how its reads went.
#[derive(Default)]
pub(in crate::ui) struct DirectoryState {
    pub(in crate::ui) list: catalog::Directory,
    pub(in crate::ui) reading: bool,
    pub(in crate::ui) error: Option<String>,
    /// Skills that the directory's search found for a query.
    pub(in crate::ui) found: Option<(String, Load<Vec<catalog::DirectorySkill>>)>,
    /// The skills whose description was asked for this session, and those that failed.
    pub(in crate::ui) asked: HashSet<String>,
    pub(in crate::ui) unreadable: HashSet<String>,
    pub(in crate::ui) description_queue: VecDeque<catalog::DirectorySkill>,
    pub(in crate::ui) description_jobs: usize,
    /// Whether descriptions arrived since the list was last kept.
    pub(in crate::ui) unsaved: bool,
    /// How many descriptions arrived this session, which a search's matches depend on.
    pub(in crate::ui) described: usize,
    /// Skills downloaded from their repositories for their pages, by id.
    pub(in crate::ui) downloads: HashMap<String, Load<catalog::CatalogSkill>>,
}

impl DirectoryState {
    /// Returns a skill of the list or of the search's results, by id.
    pub(in crate::ui) fn skill(&self, id: &str) -> Option<&catalog::DirectorySkill> {
        self.list
            .skills
            .iter()
            .find(|skill| skill.id == id)
            .or_else(|| match &self.found {
                Some((_, Load::Ready(found))) => found.iter().find(|skill| skill.id == id),
                _ => None,
            })
    }

    // Reports whether the list holds every seed word's skills.
    fn complete(&self) -> bool {
        !self.list.skills.is_empty() && self.list.seeds >= catalog::DIRECTORY_SEEDS.len()
    }
}

impl Shell {
    /// Brings the directory's list up to date. The list that the last start kept shows at once;
    /// skills.sh is asked when the list is unfinished, older than `catalog::FRESH_FOR`, or
    /// when `check` asks for it.
    pub(in crate::ui) fn load_directory(&mut self, check: bool, cx: &mut Context<Self>) {
        let directory = &self.plugins.directory;
        if directory.reading {
            return;
        }
        let fresh =
            catalog::now().saturating_sub(directory.list.read_at) < catalog::FRESH_FOR.as_secs();
        if directory.complete() && fresh && !check {
            return;
        }
        self.plugins.directory.reading = true;
        self.plugins.directory.error = None;
        let in_memory = !self.plugins.directory.list.skills.is_empty();
        cx.spawn(async move |this, cx| {
            if !in_memory {
                let cached = cx
                    .background_executor()
                    .spawn(async move { catalog::cached_directory() })
                    .await;
                if let Some(cached) = cached {
                    let _ = this.update(cx, |s, cx| {
                        s.plugins.directory.list = cached;
                        cx.notify();
                    });
                }
            }
            let Ok((list, complete)) = this.update(cx, |s, _| {
                (
                    s.plugins.directory.list.clone(),
                    s.plugins.directory.complete(),
                )
            }) else {
                return;
            };
            let fresh = catalog::now().saturating_sub(list.read_at) < catalog::FRESH_FOR.as_secs();
            let failure = if complete && fresh && !check {
                None
            } else {
                Self::read_directory(&this, list, complete, cx).await
            };
            let _ = this.update(cx, |s, cx| {
                s.plugins.directory.reading = false;
                s.plugins.directory.error = failure;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    // Searches each seed word and joins the skills found. A first read continues from the
    // word where an earlier one stopped, shows each word's skills as they arrive and keeps the
    // list after each. A refresh of a complete list reads every word again and replaces the
    // list at the end, so a failed refresh changes nothing. Returns why the read stopped early.
    async fn read_directory(
        this: &WeakEntity<Self>,
        list: catalog::Directory,
        refresh: bool,
        cx: &mut AsyncApp,
    ) -> Option<String> {
        let mut read = if refresh {
            catalog::Directory::default()
        } else {
            list
        };
        while let Some(seed) = catalog::DIRECTORY_SEEDS.get(read.seeds).copied() {
            // A closed workspace stops the read: a first read continues from the kept word on
            // the next opening, and a refresh starts again.
            if !still_open(this, cx) {
                return None;
            }
            let found = cx
                .background_executor()
                .spawn(async move { catalog::search_directory(seed) })
                .await;
            match found {
                Ok(found) => catalog::merge_directory(&mut read.skills, found),
                Err(error) => return Some(format!("{error:#}")),
            }
            read.seeds += 1;
            if read.seeds == catalog::DIRECTORY_SEEDS.len() {
                read.read_at = catalog::now();
            }
            if !refresh {
                Self::keep_directory(this, &mut read, cx).await;
            }
        }
        if refresh {
            Self::keep_directory(this, &mut read, cx).await;
        }
        None
    }

    // Shows a list that a read made and keeps it on disk. The descriptions that the shown list
    // holds and the read's list lacks go with it, since they arrive while the read runs.
    async fn keep_directory(
        this: &WeakEntity<Self>,
        read: &mut catalog::Directory,
        cx: &mut AsyncApp,
    ) {
        let mut shown = read.clone();
        let Ok(shown) = this.update(cx, |s, cx| {
            let known: HashMap<&str, &String> = s
                .plugins
                .directory
                .list
                .skills
                .iter()
                .filter_map(|known| Some((known.id.as_str(), known.description.as_ref()?)))
                .collect();
            for skill in shown
                .skills
                .iter_mut()
                .filter(|skill| skill.description.is_none())
            {
                skill.description = known.get(skill.id.as_str()).map(|text| (*text).clone());
            }
            s.plugins.directory.list = shown.clone();
            s.plugins.directory.unsaved = false;
            cx.notify();
            shown
        }) else {
            return;
        };
        *read = shown.clone();
        let _ = cx
            .background_executor()
            .spawn(async move { catalog::save_directory(&shown) })
            .await;
    }

    /// Follows the search field while Discover shows the directory: a query of two characters
    /// or more asks skills.sh once typing pauses, for the skills beyond the kept list.
    pub(in crate::ui) fn directory_query_changed(&mut self, cx: &mut Context<Self>) {
        let plugins = &self.plugins;
        if !plugins.open
            || plugins.tab != Tab::Skills
            || plugins.scope != Scope::Discover
            || plugins.source != Source::Directory
        {
            return;
        }
        let query = plugins.search.read(cx).text().trim().to_owned();
        if query.chars().count() < 2 {
            self.plugins.directory.found = None;
            return;
        }
        if matches!(&plugins.directory.found, Some((asked, Load::Loading | Load::Ready(_))) if *asked == query)
        {
            return;
        }
        self.plugins.directory.found = Some((query.clone(), Load::Loading));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DELAY).await;
            let current = |s: &Shell| {
                s.plugins
                    .directory
                    .found
                    .as_ref()
                    .is_some_and(|(asked, _)| *asked == query)
            };
            if !this
                .update(cx, |s, _| s.plugins.open && current(s))
                .unwrap_or(false)
            {
                return;
            }
            let asked = query.clone();
            let result = cx
                .background_executor()
                .spawn(async move { catalog::search_directory(&asked) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if current(s) {
                    s.plugins.directory.found = Some((
                        query.clone(),
                        match result {
                            Ok(found) => Load::Ready(found),
                            Err(error) => Load::Failed(format!("{error:#}")),
                        },
                    ));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Builds the rows of the directory in Discover: the most installed skills, or the skills
    /// of the kept list and of skills.sh's search that match the query.
    pub(in crate::ui) fn directory_rows(
        &self,
        words: &[String],
        columns: usize,
        rows: &mut Vec<GridRow>,
    ) {
        let directory = &self.plugins.directory;
        let list = &directory.list.skills;
        if words.is_empty() {
            let note = if directory.reading {
                Some("Reading skills.sh…".into())
            } else if directory.list.read_at > 0 {
                Some(format!(
                    "skills.sh · read {}",
                    super::age(directory.list.read_at)
                ))
            } else {
                None
            };
            rows.push(GridRow::Section {
                label: "Most installed".into(),
                count: (!list.is_empty()).then_some(list.len()),
                note,
            });
            if list.is_empty() {
                rows.push(match &directory.error {
                    Some(error) if !directory.reading => GridRow::Message {
                        title: "skills.sh could not be read".into(),
                        detail: error.clone(),
                        error: true,
                    },
                    _ => GridRow::Loading("READING THE SKILLS.SH DIRECTORY".into()),
                });
                return;
            }
            let cards = (0..list.len()).map(Card::Directory).collect();
            paged_rows(cards, self.plugins.page, columns, rows);
            return;
        }
        let mut matches: Vec<(u64, Card)> = list
            .iter()
            .enumerate()
            .filter(|(_, skill)| skill.matches(words))
            .map(|(index, skill)| (skill.installs, Card::Directory(index)))
            .collect();
        let (searching, failure) = match &directory.found {
            Some((_, Load::Ready(found))) => {
                matches.extend(
                    found
                        .iter()
                        .enumerate()
                        .filter(|(_, skill)| !list.iter().any(|known| known.id == skill.id))
                        .map(|(index, skill)| (skill.installs, Card::DirectoryFound(index))),
                );
                (false, None)
            }
            Some((_, Load::Failed(error))) => (false, Some(error.clone())),
            Some(_) => (true, None),
            None => (false, None),
        };
        matches.sort_by_key(|(installs, _)| std::cmp::Reverse(*installs));
        let note = if searching {
            Some("Searching skills.sh…".into())
        } else if failure.is_some() {
            Some("Only the kept list: skills.sh's search failed".into())
        } else {
            Some("The kept list and skills.sh's search".into())
        };
        rows.push(GridRow::Section {
            label: "Results".into(),
            count: Some(matches.len()),
            note,
        });
        if matches.is_empty() {
            rows.push(if searching {
                GridRow::Loading("SEARCHING SKILLS.SH".into())
            } else {
                GridRow::Message {
                    title: "No skill matches".into(),
                    detail: failure.unwrap_or_else(|| {
                        "Try other words, or add a skill from GitHub with Add skill.".into()
                    }),
                    error: false,
                }
            });
            return;
        }
        paged_rows(
            matches.into_iter().map(|(_, card)| card).collect(),
            self.plugins.page,
            columns,
            rows,
        );
    }

    /// A directory skill as a card: its publisher's avatar, its repository, its description
    /// once read, how often it was installed, and its installation.
    pub(in crate::ui) fn directory_card(
        &mut self,
        id: usize,
        card: &Card,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let directory = &self.plugins.directory;
        let skill = match card {
            Card::Directory(index) => directory.list.skills.get(*index).cloned(),
            Card::DirectoryFound(index) => match &directory.found {
                Some((_, Load::Ready(found))) => found.get(*index).cloned(),
                _ => None,
            },
            _ => None,
        };
        let Some(skill) = skill else {
            return div().flex_1().into_any_element();
        };
        let description = match &skill.description {
            Some(description) => description.clone(),
            None if directory.unreadable.contains(&skill.id) => {
                format!("A skill from {}.", skill.source)
            }
            None => {
                self.directory_description(&skill, cx);
                String::new()
            }
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
        let name = skill.name.clone();
        let open = skill.id.clone();
        let quick = skill.clone();
        let action = if installed {
            self.ghost(("plugins-directory-installed", id), "settings-2", "Manage")
                .h(px(26.))
                .into_any_element()
        } else {
            self.button(
                ("plugins-directory-install", id),
                "arrow-down-to-line",
                "Install",
            )
            .h(px(26.))
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: "Download and install for every installed provider".into(),
                    theme: t,
                })
                .into()
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                cx.stop_propagation();
                let providers = s.plugins.installed().to_vec();
                let name = install_name(&quick);
                s.install_directory_skill(quick.clone(), name, providers, cx);
            }))
            .into_any_element()
        };
        let glyph = look::skill_glyph(&skill.name, &description);
        let icon = self.plugin_icon(
            Some(catalog::avatar(skill.owner())),
            glyph,
            t.accent,
            38.,
            cx,
        );
        self.plugin_card(
            ("plugin-directory-skill", id),
            CardContent {
                icon,
                title: skill.name.clone(),
                subtitle: skill.source.clone(),
                description,
                chips: vec![(installs_label(skill.installs).into(), t.muted)],
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
                s.open_plugin_detail(Detail::DirectorySkill(open.clone()), cx)
            }
        }))
        .into_any_element()
    }

    // Asks for a skill's description once a session, a few downloads at a time.
    fn directory_description(&mut self, skill: &catalog::DirectorySkill, cx: &mut Context<Self>) {
        if !self.plugins.directory.asked.insert(skill.id.clone()) {
            return;
        }
        let directory = &mut self.plugins.directory;
        directory.description_queue.push_back(skill.clone());
        // Cards that scrolled away give up their place; they ask again when they show.
        while directory.description_queue.len() > look::QUEUE_LIMIT {
            if let Some(dropped) = directory.description_queue.pop_front() {
                directory.asked.remove(&dropped.id);
            }
        }
        self.pump_descriptions(cx);
    }

    // Starts description downloads while fewer than `DESCRIPTION_JOBS` run and the workspace is
    // open, newest request first. Once the queue is empty, the list is kept with the
    // descriptions that arrived.
    fn pump_descriptions(&mut self, cx: &mut Context<Self>) {
        let directory = &mut self.plugins.directory;
        if directory.description_jobs == 0
            && directory.description_queue.is_empty()
            && directory.unsaved
            && !directory.reading
        {
            directory.unsaved = false;
            let kept = directory.list.clone();
            cx.background_executor()
                .spawn(async move {
                    let _ = catalog::save_directory(&kept);
                })
                .detach();
            return;
        }
        while self.plugins.open && self.plugins.directory.description_jobs < DESCRIPTION_JOBS {
            let Some(skill) = self.plugins.directory.description_queue.pop_back() else {
                return;
            };
            self.plugins.directory.description_jobs += 1;
            cx.spawn(async move |this, cx| {
                let asked = skill.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { catalog::directory_description(&asked) })
                    .await;
                let _ = this.update(cx, |s, cx| {
                    let directory = &mut s.plugins.directory;
                    directory.description_jobs = directory.description_jobs.saturating_sub(1);
                    match result {
                        Ok(description) => {
                            let found = match &mut directory.found {
                                Some((_, Load::Ready(found))) => found.as_mut_slice(),
                                _ => &mut [],
                            };
                            for entry in directory.list.skills.iter_mut().chain(found.iter_mut()) {
                                if entry.id == skill.id {
                                    entry.description = Some(description.clone());
                                    directory.unsaved = true;
                                    directory.described += 1;
                                }
                            }
                        }
                        Err(_) => {
                            directory.unreadable.insert(skill.id.clone());
                        }
                    }
                    s.pump_descriptions(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Downloads a directory skill's repository for its page, once.
    pub(in crate::ui) fn load_directory_skill(
        &mut self,
        skill: catalog::DirectorySkill,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.plugins.directory.downloads.get(&skill.id),
            Some(Load::Loading | Load::Ready(_))
        ) {
            return;
        }
        let id = skill.id.clone();
        self.plugins
            .directory
            .downloads
            .insert(id.clone(), Load::Loading);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { catalog::read_directory_skill(&skill) })
                .await;
            let _ = this.update(cx, |s, cx| {
                let load = match result {
                    Ok(found) => {
                        s.load_manifest(found.folder.clone(), cx);
                        Load::Ready(found)
                    }
                    Err(error) => Load::Failed(format!("{error:#}")),
                };
                s.plugins.directory.downloads.insert(id, load);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    // Installs a directory skill for `providers` under `name`, from its downloaded repository,
    // or from a new download when its page did not download it.
    fn install_directory_skill(
        &mut self,
        skill: catalog::DirectorySkill,
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
        let downloaded = match self.plugins.directory.downloads.get(&skill.id) {
            Some(Load::Ready(found)) => Some(found.clone()),
            _ => None,
        };
        let count = providers.len();
        self.run_plugin_change(
            format!("Installed {name} for {}", servers::provider_count(count)),
            move |_| {
                let found = match downloaded {
                    Some(found) => found,
                    None => match catalog::read_directory_skill(&skill) {
                        Ok(found) => found,
                        Err(error) => return vec![format!("{error:#}")],
                    },
                };
                // The skill holds its unpacked repository until the copy is done.
                labelled(skills::install(&found.folder, &name, &providers))
            },
            cx,
        );
    }

    /// The page of a directory skill: its installation form and the `SKILL.md` of its
    /// repository, which downloads when the page opens.
    pub(in crate::ui) fn directory_skill_page(
        &mut self,
        skill: &catalog::DirectorySkill,
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
                s.install_directory_skill(target.clone(), name, providers, cx)
            },
            cx,
        );
        let main = match self.plugins.directory.downloads.get(&skill.id).cloned() {
            Some(Load::Ready(found)) => self.manifest_block(&found.folder, window, cx),
            Some(Load::Failed(error)) => kit::block(t)
                .p(px(16.))
                .text_size(text_px(12.))
                .text_color(t.removed)
                .child(error)
                .into_any_element(),
            _ => kit::block(t)
                .h(px(120.))
                .flex()
                .items_center()
                .justify_center()
                .child(kit::loader_row("DOWNLOADING THE SKILL'S REPOSITORY", t))
                .into_any_element(),
        };
        let site = skill.url();
        let repository = format!("https://github.com/{}", skill.source);
        let actions = div()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                self.button("plugins-directory-site", "globe", "skills.sh")
                    .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&site))),
            )
            .child(
                self.button("plugins-directory-github", "github", "Open on GitHub")
                    .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&repository))),
            );
        let mut chips: Vec<(SharedString, Hsla)> =
            vec![(installs_label(skill.installs).into(), t.muted)];
        if installed {
            chips.push(("INSTALLED".into(), t.added));
        }
        let description = skill.description.clone().unwrap_or_default();
        let glyph = look::skill_glyph(&skill.name, &description);
        let icon = self.plugin_icon(
            Some(catalog::avatar(skill.owner())),
            glyph,
            t.accent,
            64.,
            cx,
        );
        let hero = self
            .detail_hero(
                icon,
                skill.name.clone(),
                format!("{} · {}", skill.source, skill.skill_id),
                chips,
                actions,
                t.accent,
            )
            .when(!description.is_empty(), |hero| {
                hero.child(
                    div()
                        .relative()
                        .px(px(20.))
                        .pb(px(18.))
                        .text_size(text_px(13.))
                        .line_height(relative(1.55))
                        .text_color(t.muted)
                        .child(description),
                )
            });
        self.skill_page_layout(hero, main, side)
    }
}

/// Returns the folder name that a directory skill installs under: its name when that is a
/// valid folder name, else its slug.
pub(in crate::ui) fn install_name(skill: &catalog::DirectorySkill) -> String {
    if skills::validate_name(&skill.name).is_ok() {
        skill.name.clone()
    } else {
        skill.skill_id.clone()
    }
}

// Returns a short label of an install count, such as `1.4M INSTALLS`.
fn installs_label(installs: u64) -> String {
    let count = match installs {
        0..1_000 => installs.to_string(),
        1_000..1_000_000 => format!("{:.1}K", installs as f64 / 1_000.),
        _ => format!("{:.1}M", installs as f64 / 1_000_000.),
    };
    format!("{} INSTALLS", count.replace(".0", ""))
}
