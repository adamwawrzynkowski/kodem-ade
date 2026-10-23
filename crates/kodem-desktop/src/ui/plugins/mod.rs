//! The Plugins workspace: the MCP servers and skills of every provider's CLI, the catalogs that
//! offer more, and their installation for the providers that the user picks.
//!
//! Like the Git workspace, it covers the task workspace that was open. Its sidebar filters by
//! provider and its page has a tab for MCP servers and one for skills. Every read and write of
//! the CLIs' files runs on a background thread (`crate::plugins`); after a change the inventory
//! is read again, so the page always shows what the files hold.
mod dialogs;
mod directory;
mod look;
mod servers;
mod skill_pages;

use super::*;
use crate::plugins::{self as backend, catalog, mcp, skills};

pub(in crate::ui) use dialogs::{AddSkill, CustomServer};
use directory::DirectoryState;
use look::{CARD_HEIGHT, CardContent, Category, FEATURED_HEIGHT};
use std::collections::{HashSet, VecDeque};

/// The tabs of the Plugins page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum Tab {
    #[default]
    Servers,
    Skills,
}

/// What a tab lists: what the catalogs offer, or what is installed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum Scope {
    #[default]
    Discover,
    Installed,
}

/// Where Discover's skills come from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum Source {
    /// The skills.sh directory.
    #[default]
    Directory,
    /// A skill repository, as an index of `catalog::SKILL_SOURCES`.
    Repository(usize),
}

/// The state of something that loads in the background.
#[derive(Clone, Debug, Default)]
pub(in crate::ui) enum Load<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

/// What the page shows in place of its list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Detail {
    /// A server of the MCP Registry, by its registry name.
    Catalog(String),
    /// An installed server that the registry does not describe, by its name in the CLIs.
    Installed(String),
    /// An installed skill, by name.
    Skill(String),
    /// A skill of a repository, by the index of its source and its name.
    CatalogSkill(usize, String),
    /// A skill of the skills.sh directory, by its id.
    DirectorySkill(String),
}

/// The installation form of a catalog server or skill. It opens with every installed provider
/// chosen.
pub(in crate::ui) struct InstallForm {
    /// The detail that the form belongs to.
    pub(in crate::ui) detail: Detail,
    /// The chosen way to run a catalog server, as an index of its launches.
    pub(in crate::ui) launch: usize,
    pub(in crate::ui) name: Entity<TextInput>,
    /// One field per variable or header of the chosen launch.
    pub(in crate::ui) inputs: Vec<(catalog::Input, Entity<TextInput>)>,
    pub(in crate::ui) providers: Vec<Provider>,
    pub(in crate::ui) running: bool,
}

/// The servers of the MCP Registry, and how their last refresh went.
#[derive(Default)]
pub(in crate::ui) struct RegistryState {
    pub(in crate::ui) servers: Vec<catalog::CatalogServer>,
    pub(in crate::ui) reading: bool,
    pub(in crate::ui) error: Option<String>,
    /// When the shown list was last complete and up to date, in seconds since the Unix epoch;
    /// zero while the first read is unfinished.
    pub(in crate::ui) read_at: u64,
    /// The cursor of the next page of an unfinished first read, and when that read began.
    pub(in crate::ui) cursor: Option<String>,
    pub(in crate::ui) started_at: u64,
    /// Servers that the registry's own search found for a query the list does not match.
    pub(in crate::ui) found: Option<(String, Load<Vec<catalog::CatalogServer>>)>,
}

/// The state of the Plugins workspace.
pub(in crate::ui) struct Plugins {
    pub(in crate::ui) open: bool,
    pub(in crate::ui) tab: Tab,
    pub(in crate::ui) scope: Scope,
    /// The provider that the lists show, or every provider.
    pub(in crate::ui) provider: Option<Provider>,
    /// The providers whose CLI is on this computer; unknown until the first scan.
    pub(in crate::ui) installed: Option<Vec<Provider>>,
    pub(in crate::ui) servers: mcp::Inventory,
    pub(in crate::ui) skills: skills::Inventory,
    pub(in crate::ui) scanning: bool,
    pub(in crate::ui) registry: RegistryState,
    /// The skill source that Discover shows.
    pub(in crate::ui) source: Source,
    pub(in crate::ui) directory: DirectoryState,
    pub(in crate::ui) catalog_skills: HashMap<usize, Load<Vec<catalog::CatalogSkill>>>,
    pub(in crate::ui) detail: Option<Detail>,
    pub(in crate::ui) form: Option<InstallForm>,
    /// Changes that run in the background.
    pub(in crate::ui) changes: usize,
    pub(in crate::ui) search: Entity<TextInput>,
    /// The rows of the open list and the scroll state of the list. The rows are built again
    /// only when `rows_key`, which names everything they show, changes.
    pub(in crate::ui) rows: std::rc::Rc<Vec<GridRow>>,
    pub(in crate::ui) rows_key: String,
    /// The pages of the open list, shown in the bar under it: the page, the pages and the
    /// cards; absent for a list of one page.
    pub(in crate::ui) pager: Option<(usize, usize, usize)>,
    /// Counts the reads of the inventory, whose results the rows depend on.
    pub(in crate::ui) scan_revision: u64,
    pub(in crate::ui) feed: ListState,
    /// The kinds of the rows that `feed` measured, which decides whether it keeps its scroll.
    pub(in crate::ui) feed_shape: String,
    /// What the open list shows; a change starts it at its top.
    pub(in crate::ui) feed_key: String,
    /// The page of the open list, and the list it belongs to; another list starts at its
    /// first page.
    pub(in crate::ui) page: usize,
    pub(in crate::ui) page_for: String,
    /// The category that Discover shows.
    pub(in crate::ui) category: Category,
    /// The category of each registry server, by index, kept beside the list, and how many
    /// servers each category holds, counted when the rows are built.
    pub(in crate::ui) categories: Vec<Category>,
    pub(in crate::ui) category_counts: Vec<usize>,
    /// Icons and avatars, by address, and the downloads that wait or run.
    pub(in crate::ui) images: HashMap<String, Load<PathBuf>>,
    pub(in crate::ui) image_queue: VecDeque<String>,
    pub(in crate::ui) image_jobs: usize,
    pub(in crate::ui) detail_scroll: ScrollHandle,
    /// The texts of skill files that a detail page shows, by folder.
    pub(in crate::ui) manifests: HashMap<PathBuf, Load<(String, Vec<String>)>>,
    pub(in crate::ui) custom: Option<CustomServer>,
    pub(in crate::ui) add_skill: Option<AddSkill>,
}

impl Plugins {
    /// Creates the closed workspace with its search field.
    pub(in crate::ui) fn new(cx: &mut Context<Shell>) -> Self {
        let search = cx.new(|cx| {
            let mut input = TextInput::new("Search…", cx);
            input.set_single_line(true);
            input
        });
        Self {
            open: false,
            tab: Tab::default(),
            scope: Scope::default(),
            provider: None,
            installed: None,
            servers: mcp::Inventory::default(),
            skills: skills::Inventory::default(),
            scanning: false,
            registry: RegistryState::default(),
            source: Source::default(),
            directory: DirectoryState::default(),
            catalog_skills: HashMap::new(),
            detail: None,
            form: None,
            changes: 0,
            search,
            rows: std::rc::Rc::new(Vec::new()),
            rows_key: String::new(),
            pager: None,
            scan_revision: 0,
            feed: ListState::new(0, ListAlignment::Top, px(400.)),
            feed_shape: String::new(),
            feed_key: String::new(),
            page: 0,
            page_for: String::new(),
            category: Category::All,
            categories: Vec::new(),
            category_counts: Vec::new(),
            images: HashMap::new(),
            image_queue: VecDeque::new(),
            image_jobs: 0,
            detail_scroll: ScrollHandle::new(),
            manifests: HashMap::new(),
            custom: None,
            add_skill: None,
        }
    }

    /// Returns the installed providers, or none before the first scan.
    pub(in crate::ui) fn installed(&self) -> &[Provider] {
        self.installed.as_deref().unwrap_or_default()
    }

    /// Returns the installed entries of a server name, one per provider that holds it.
    pub(in crate::ui) fn entries(&self, name: &str) -> Vec<&mcp::Entry> {
        self.servers
            .entries
            .iter()
            .filter(|entry| entry.server.name == name)
            .collect()
    }

    /// Returns the installed server names, each once, in order.
    pub(in crate::ui) fn server_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for entry in &self.servers.entries {
            if !names.contains(&entry.server.name) {
                names.push(entry.server.name.clone());
            }
        }
        names.sort_by_key(|name| name.to_lowercase());
        names
    }

    /// Returns the installed name of a catalog server: the name of an entry with the same
    /// package or address.
    pub(in crate::ui) fn installed_name(&self, server: &catalog::CatalogServer) -> Option<String> {
        self.servers
            .entries
            .iter()
            .find(|entry| same_server(server, &entry.server.transport))
            .map(|entry| entry.server.name.clone())
    }

    /// Returns the catalog server that an installed name stands for.
    pub(in crate::ui) fn catalog_of(&self, name: &str) -> Option<&catalog::CatalogServer> {
        let entry = self.entries(name).into_iter().next()?;
        self.registry
            .servers
            .iter()
            .find(|server| same_server(server, &entry.server.transport))
    }

    /// Returns the words of the search field, in lowercase.
    pub(in crate::ui) fn query(&self, cx: &App) -> Vec<String> {
        self.search
            .read(cx)
            .text()
            .split_whitespace()
            .map(str::to_lowercase)
            .collect()
    }
}

/// Returns the category of each server, found from its name, title and description.
fn categorize(servers: &[catalog::CatalogServer]) -> Vec<Category> {
    servers
        .iter()
        .map(|server| {
            Category::of(&format!(
                "{} {} {}",
                server.name, server.title, server.description
            ))
        })
        .collect()
}

/// Names the state of a search's results, for the key of the rows that show them.
fn found_mark<T>(found: &Option<(String, Load<Vec<T>>)>) -> String {
    match found {
        Some((query, Load::Ready(list))) => format!("{query}:{}", list.len()),
        Some((query, Load::Failed(error))) => format!("{query}:!{error}"),
        Some((query, _)) => format!("{query}:…"),
        None => String::new(),
    }
}

/// Reports whether the Plugins workspace is open. Background reads check it before each request,
/// so nothing is downloaded while another workspace shows.
fn still_open(this: &WeakEntity<Shell>, cx: &mut AsyncApp) -> bool {
    this.update(cx, |s, _| s.plugins.open).unwrap_or(false)
}

/// Reports whether an installed transport runs a catalog server: the same package, or the same
/// address.
fn same_server(server: &catalog::CatalogServer, transport: &mcp::Transport) -> bool {
    server
        .launches
        .iter()
        .any(|launch| match (launch, transport) {
            (catalog::Launch::Remote { url, .. }, mcp::Transport::Http { url: installed, .. })
            | (catalog::Launch::Remote { url, .. }, mcp::Transport::Sse { url: installed, .. }) => {
                url.trim_end_matches('/') == installed.trim_end_matches('/')
            }
            (catalog::Launch::Package { identifier, .. }, mcp::Transport::Stdio { args, .. }) => {
                args.iter().any(|arg| {
                    arg == identifier
                        || arg
                            .strip_prefix(identifier.as_str())
                            .is_some_and(|rest| rest.starts_with(['@', '=', ':']))
                })
            }
            _ => false,
        })
}

impl Shell {
    /// Opens the Plugins workspace over the task workspace. The first opening reads the
    /// inventory and the registry.
    pub(in crate::ui) fn open_plugins(&mut self, cx: &mut Context<Self>) {
        if self.plugins.open || self.creating_task {
            return;
        }
        if self.git_open {
            self.close_git(cx);
        }
        self.plugins.open = true;
        self.panel = Panel::None;
        self.search_visible = false;
        self.scan_plugins(cx);
        // The kept list shows at once; the registry is asked only when the list is unfinished
        // or old.
        self.load_registry(false, cx);
        if self.plugins.tab == Tab::Skills && self.plugins.scope == Scope::Discover {
            self.load_skill_discover(false, cx);
        }
        cx.notify();
    }

    /// Returns to the task workspace that the Plugins workspace covered.
    pub(in crate::ui) fn close_plugins(&mut self, cx: &mut Context<Self>) {
        if !self.plugins.open {
            return;
        }
        self.plugins.open = false;
        // Queued downloads wait for cards that are no longer drawn; the cards ask again when
        // they show.
        let plugins = &mut self.plugins;
        for url in plugins.image_queue.drain(..) {
            plugins.images.remove(&url);
        }
        let directory = &mut plugins.directory;
        for skill in directory.description_queue.drain(..) {
            directory.asked.remove(&skill.id);
        }
        if matches!(&directory.found, Some((_, Load::Loading))) {
            directory.found = None;
        }
        if matches!(self.panel, Panel::AddServer | Panel::AddSkill) {
            self.panel = Panel::None;
        }
        cx.notify();
    }

    /// Returns every provider account, whose configuration folders a change writes.
    fn plugin_accounts(&self) -> Vec<Account> {
        self.workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .accounts
                    .iter()
                    .map(|state| state.account.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Reads which CLIs are installed and what their configurations hold.
    pub(in crate::ui) fn scan_plugins(&mut self, cx: &mut Context<Self>) {
        if self.plugins.scanning {
            return;
        }
        self.plugins.scanning = true;
        let accounts = self.plugin_accounts();
        cx.spawn(async move |this, cx| {
            let (installed, servers, skills) = cx
                .background_executor()
                .spawn(async move {
                    (
                        backend::installed_providers(),
                        mcp::read(&accounts),
                        skills::read(),
                    )
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                let plugins = &mut s.plugins;
                plugins.scanning = false;
                plugins.installed = Some(installed);
                plugins.servers = servers;
                plugins.skills = skills;
                plugins.scan_revision += 1;
                // A detail of something that is gone returns to the list.
                let gone = match &plugins.detail {
                    Some(Detail::Installed(name)) => plugins.entries(name).is_empty(),
                    Some(Detail::Skill(name)) => !plugins
                        .skills
                        .skills
                        .iter()
                        .any(|skill| &skill.name == name),
                    _ => false,
                };
                if gone {
                    plugins.detail = None;
                    plugins.form = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Brings the registry's list up to date. The list that the last start kept shows at once.
    /// An unfinished first read continues from its last page; a complete list asks only for
    /// the servers that changed since it was read, when it is older than `catalog::FRESH_FOR`
    /// or when `check` asks for it. The list is kept on disk after each step.
    pub(in crate::ui) fn load_registry(&mut self, check: bool, cx: &mut Context<Self>) {
        if self.plugins.registry.reading {
            return;
        }
        let registry = &self.plugins.registry;
        let complete =
            !registry.servers.is_empty() && registry.cursor.is_none() && registry.read_at > 0;
        let fresh = catalog::now().saturating_sub(registry.read_at) < catalog::FRESH_FOR.as_secs();
        if complete && fresh && !check {
            return;
        }
        self.plugins.registry.reading = true;
        self.plugins.registry.error = None;
        let in_memory = !self.plugins.registry.servers.is_empty();
        cx.spawn(async move |this, cx| {
            if !in_memory {
                let cached = cx
                    .background_executor()
                    .spawn(async move {
                        catalog::cached_registry().map(|cached| {
                            let categories = categorize(&cached.servers);
                            (cached, categories)
                        })
                    })
                    .await;
                if let Some((cached, categories)) = cached {
                    let _ = this.update(cx, |s, cx| {
                        s.plugins.categories = categories;
                        let registry = &mut s.plugins.registry;
                        registry.servers = cached.servers;
                        registry.read_at = cached.read_at;
                        registry.cursor = cached.cursor;
                        registry.started_at = cached.started_at;
                        cx.notify();
                    });
                }
            }
            let Ok(mut list) = this.update(cx, |s, _| {
                let registry = &s.plugins.registry;
                catalog::Registry {
                    servers: registry.servers.clone(),
                    read_at: registry.read_at,
                    cursor: registry.cursor.clone(),
                    started_at: registry.started_at,
                }
            }) else {
                return;
            };
            let complete = !list.servers.is_empty() && list.cursor.is_none() && list.read_at > 0;
            let fresh = catalog::now().saturating_sub(list.read_at) < catalog::FRESH_FOR.as_secs();
            let failure = if complete && fresh && !check {
                None
            } else if complete {
                Self::update_registry(&this, &mut list, cx).await
            } else {
                Self::read_whole_registry(&this, &mut list, cx).await
            };
            let _ = this.update(cx, |s, cx| {
                let registry = &mut s.plugins.registry;
                registry.reading = false;
                registry.error = failure;
                cx.notify();
            });
        })
        .detach();
    }

    // Reads every page of the registry, from the cursor of an unfinished read or from the
    // start. Each page's new servers are added to the shown list as they arrive; the list is
    // kept on disk with the cursor of the next page every `SAVE_EVERY` pages and when the read
    // stops, so a read that stops continues later. Returns why it stopped early.
    async fn read_whole_registry(
        this: &WeakEntity<Self>,
        list: &mut catalog::Registry,
        cx: &mut AsyncApp,
    ) -> Option<String> {
        /// Pages read between two copies on disk, since a copy writes the whole list.
        const SAVE_EVERY: usize = 10;
        if list.cursor.is_none() {
            list.servers.clear();
            list.started_at = catalog::now();
            let _ = this.update(cx, |s, _| {
                s.plugins.registry.servers.clear();
                s.plugins.categories.clear();
            });
        }
        let mut names: HashSet<String> = list
            .servers
            .iter()
            .map(|server| server.name.clone())
            .collect();
        let mut failure = None;
        for step in 1..=catalog::PAGE_LIMIT {
            // A closed workspace stops the read; the kept cursor continues it on the next opening.
            if !still_open(this, cx) {
                break;
            }
            let after = list.cursor.clone();
            let page = cx
                .background_executor()
                .spawn(async move { catalog::registry_page(after.as_deref(), None) })
                .await;
            let (changes, next) = match page {
                Ok(page) => page,
                Err(error) => {
                    failure = Some(format!("{error:#}"));
                    break;
                }
            };
            let fresh: Vec<catalog::CatalogServer> = changes
                .into_iter()
                .filter_map(|(_, entry)| entry)
                .filter(|entry| names.insert(entry.name.clone()))
                .collect();
            list.servers.extend(fresh.iter().cloned());
            list.cursor = next;
            if list.cursor.is_none() {
                // Complete: the list holds every server as of the read's start.
                list.read_at = list.started_at;
            }
            let (read_at, cursor, started_at) =
                (list.read_at, list.cursor.clone(), list.started_at);
            let categories = categorize(&fresh);
            let _ = this.update(cx, |s, cx| {
                s.plugins.categories.extend(categories);
                let registry = &mut s.plugins.registry;
                registry.servers.extend(fresh);
                registry.read_at = read_at;
                registry.cursor = cursor;
                registry.started_at = started_at;
                cx.notify();
            });
            if list.cursor.is_none() {
                break;
            }
            if step % SAVE_EVERY == 0 {
                Self::save_registry_copy(list, cx).await;
            }
        }
        Self::save_registry_copy(list, cx).await;
        failure
    }

    // Keeps a copy of the registry's list on disk. The list itself goes to a background thread
    // for the write and comes back, so the main thread copies nothing.
    async fn save_registry_copy(list: &mut catalog::Registry, cx: &mut AsyncApp) {
        let kept = std::mem::take(list);
        *list = cx
            .background_executor()
            .spawn(async move {
                let _ = catalog::save_registry(&kept);
                kept
            })
            .await;
    }

    // Asks the registry for the servers that changed since the list was read, with a margin
    // for clocks that differ, and applies them. Returns why it failed; the list then stays as
    // it was.
    async fn update_registry(
        this: &WeakEntity<Self>,
        list: &mut catalog::Registry,
        cx: &mut AsyncApp,
    ) -> Option<String> {
        let started = catalog::now();
        let since = list.read_at.saturating_sub(10 * 60);
        let mut changes = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..catalog::PAGE_LIMIT {
            // A closed workspace drops the update; the list stays old and the next opening asks
            // again.
            if !still_open(this, cx) {
                return None;
            }
            let after = cursor.clone();
            let page = cx
                .background_executor()
                .spawn(async move { catalog::registry_page(after.as_deref(), Some(since)) })
                .await;
            match page {
                Ok((found, next)) => {
                    changes.extend(found);
                    cursor = next;
                }
                Err(error) => return Some(format!("{error:#}")),
            }
            if cursor.is_none() {
                break;
            }
        }
        // Merging sorts the whole list, which runs on a background thread.
        let servers = std::mem::take(&mut list.servers);
        let (servers, categories) = cx
            .background_executor()
            .spawn(async move {
                let mut servers = servers;
                catalog::merge_changes(&mut servers, changes);
                let categories = categorize(&servers);
                (servers, categories)
            })
            .await;
        list.servers = servers;
        list.read_at = started;
        Self::save_registry_copy(list, cx).await;
        let shown = list.clone();
        let _ = this.update(cx, |s, cx| {
            s.plugins.categories = categories;
            let registry = &mut s.plugins.registry;
            registry.servers = shown.servers;
            registry.read_at = shown.read_at;
            cx.notify();
        });
        None
    }

    /// Asks the registry's own search for `query`, for servers that the list does not hold.
    fn search_registry(&mut self, query: String, cx: &mut Context<Self>) {
        self.plugins.registry.found = Some((query.clone(), Load::Loading));
        cx.spawn(async move |this, cx| {
            let asked = query.clone();
            let result = cx
                .background_executor()
                .spawn(async move { catalog::search_registry(&asked) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.plugins
                    .registry
                    .found
                    .as_ref()
                    .is_some_and(|(asked, _)| *asked == query)
                {
                    s.plugins.registry.found = Some((
                        query,
                        match result {
                            Ok(servers) => Load::Ready(servers),
                            Err(error) => Load::Failed(format!("{error:#}")),
                        },
                    ));
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Reads what Discover's skill source shows: the directory, brought up to date, or a
    /// repository, once. `again` reads it even when it is up to date or read.
    pub(in crate::ui) fn load_skill_discover(&mut self, again: bool, cx: &mut Context<Self>) {
        match self.plugins.source {
            Source::Directory => {
                self.load_directory(again, cx);
                self.directory_query_changed(cx);
            }
            Source::Repository(index) => {
                if again {
                    self.plugins.catalog_skills.remove(&index);
                }
                self.load_skill_source(index, cx);
            }
        }
    }

    /// Reads the skills of a repository source, once.
    pub(in crate::ui) fn load_skill_source(&mut self, index: usize, cx: &mut Context<Self>) {
        if matches!(
            self.plugins.catalog_skills.get(&index),
            Some(Load::Loading | Load::Ready(_))
        ) {
            return;
        }
        let Some(source) = catalog::SKILL_SOURCES.get(index).copied() else {
            return;
        };
        self.plugins.catalog_skills.insert(index, Load::Loading);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    catalog::read_skill_repository(
                        source.owner,
                        source.repository,
                        "",
                        source.folder,
                    )
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.plugins.catalog_skills.insert(
                    index,
                    match result {
                        Ok(skills) => Load::Ready(skills),
                        Err(error) => Load::Failed(format!("{error:#}")),
                    },
                );
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Reads a skill folder's `SKILL.md` and file list for its detail page, once.
    pub(in crate::ui) fn load_manifest(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        if self.plugins.manifests.contains_key(&folder) {
            return;
        }
        self.plugins.manifests.insert(folder.clone(), Load::Loading);
        cx.spawn(async move |this, cx| {
            let read = folder.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    skills::read_manifest(&read).map(|text| (text, skills::files(&read, 200)))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.plugins.manifests.insert(
                    folder,
                    match result {
                        Ok(read) => Load::Ready(read),
                        Err(error) => Load::Failed(format!("{error:#}")),
                    },
                );
                cx.notify();
            });
        })
        .detach();
    }

    /// Runs a change of the CLIs' files in the background, then reads the inventory again.
    /// `done` names what succeeded; each failure names its provider.
    pub(in crate::ui) fn run_plugin_change(
        &mut self,
        done: String,
        change: impl FnOnce(Vec<Account>) -> Vec<String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.plugins.changes += 1;
        let accounts = self.plugin_accounts();
        cx.spawn(async move |this, cx| {
            let failures = cx
                .background_executor()
                .spawn(async move { change(accounts) })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.plugins.changes = s.plugins.changes.saturating_sub(1);
                if let Some(form) = &mut s.plugins.form {
                    form.running = false;
                }
                if failures.is_empty() {
                    let tone = s.theme.added;
                    s.notify_user(done, tone, cx);
                } else {
                    s.report_error(failures.join("\n"), cx);
                }
                s.scan_plugins(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Opens a detail page in place of the list. A catalog detail or a skill gets its
    /// installation form.
    pub(in crate::ui) fn open_plugin_detail(&mut self, detail: Detail, cx: &mut Context<Self>) {
        self.plugins.detail_scroll = ScrollHandle::new();
        self.plugins.form = None;
        match &detail {
            Detail::Catalog(name) => {
                if let Some(server) = self
                    .plugins
                    .registry
                    .servers
                    .iter()
                    .chain(match &self.plugins.registry.found {
                        Some((_, Load::Ready(found))) => found.as_slice(),
                        _ => &[],
                    })
                    .find(|server| &server.name == name)
                    .cloned()
                {
                    self.plugins.form = Some(self.server_form(&detail, &server, 0, cx));
                }
            }
            Detail::CatalogSkill(source, name) => {
                if let Some(Load::Ready(found)) = self.plugins.catalog_skills.get(source)
                    && let Some(skill) = found.iter().find(|skill| &skill.name == name).cloned()
                {
                    let form = self.skill_form(&detail, &skill.name, cx);
                    self.plugins.form = Some(form);
                    self.load_manifest(skill.folder, cx);
                }
            }
            Detail::DirectorySkill(id) => {
                if let Some(skill) = self.plugins.directory.skill(id).cloned() {
                    let form = self.skill_form(&detail, &directory::install_name(&skill), cx);
                    self.plugins.form = Some(form);
                    if let Some(Load::Ready(found)) = self.plugins.directory.downloads.get(id) {
                        let folder = found.folder.clone();
                        self.load_manifest(folder, cx);
                    }
                    self.load_directory_skill(skill, cx);
                }
            }
            Detail::Skill(name) => {
                if let Some(copy) = self
                    .plugins
                    .skills
                    .skills
                    .iter()
                    .find(|skill| &skill.name == name)
                    .and_then(|skill| skill.copies.first())
                {
                    self.load_manifest(copy.path.clone(), cx);
                }
            }
            Detail::Installed(_) => {}
        }
        self.plugins.detail = Some(detail);
        cx.notify();
    }

    /// Returns from a detail page to the list.
    pub(in crate::ui) fn close_plugin_detail(&mut self, cx: &mut Context<Self>) {
        self.plugins.detail = None;
        self.plugins.form = None;
        cx.notify();
    }

    /// Builds the installation form of a catalog server for one of its launches. Fields keep
    /// what was typed in a form that the same server had open.
    pub(in crate::ui) fn server_form(
        &mut self,
        detail: &Detail,
        server: &catalog::CatalogServer,
        launch: usize,
        cx: &mut Context<Self>,
    ) -> InstallForm {
        let earlier = self
            .plugins
            .form
            .take()
            .filter(|form| &form.detail == detail);
        let installed_name = self.plugins.installed_name(server);
        let name_text = earlier
            .as_ref()
            .map(|form| form.name.read(cx).text())
            .or(installed_name)
            .unwrap_or_else(|| mcp::suggested_name(&server.name));
        let name = single_line_input("Server name", name_text, cx);
        let inputs = server
            .launches
            .get(launch)
            .map(|launch| launch.inputs().to_vec())
            .unwrap_or_default()
            .into_iter()
            .map(|input| {
                let typed = earlier
                    .as_ref()
                    .and_then(|form| {
                        form.inputs
                            .iter()
                            .find(|(known, _)| known.name == input.name)
                    })
                    .map(|(_, field)| field.read(cx).text())
                    .unwrap_or_default();
                let field = single_line_input(&input_hint(&input), typed, cx);
                if input.secret {
                    field.update(cx, |field, cx| field.set_masked(true, cx));
                }
                (input, field)
            })
            .collect();
        let providers = earlier
            .as_ref()
            .map(|form| form.providers.clone())
            .unwrap_or_else(|| self.plugins.installed().to_vec());
        InstallForm {
            detail: detail.clone(),
            launch,
            name,
            inputs,
            providers,
            running: false,
        }
    }

    /// Builds the installation form of a catalog skill.
    pub(in crate::ui) fn skill_form(
        &mut self,
        detail: &Detail,
        name: &str,
        cx: &mut Context<Self>,
    ) -> InstallForm {
        InstallForm {
            detail: detail.clone(),
            launch: 0,
            name: single_line_input("Skill name", name.to_owned(), cx),
            inputs: Vec::new(),
            providers: self.plugins.installed().to_vec(),
            running: false,
        }
    }

    /// Builds the title bar's content while the Plugins workspace is open.
    pub(in crate::ui) fn plugins_title(&self) -> Div {
        let t = self.theme;
        let installed = self.plugins.installed().len();
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .min_w_0()
            .child(icon("puzzle").size(px(14.)).text_color(t.accent))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(text_px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .child("Plugins"),
            )
            .child(figure("/", t.subtle))
            .child(
                div()
                    .min_w_0()
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.))
                    .text_color(t.muted)
                    .text_ellipsis()
                    .child(match self.plugins.tab {
                        Tab::Servers => "MCP servers",
                        Tab::Skills => "Skills",
                    }),
            )
            .when(self.plugins.installed.is_some(), |d| {
                d.child(kit::chip(
                    format!(
                        "{installed} {}",
                        if installed == 1 {
                            "PROVIDER"
                        } else {
                            "PROVIDERS"
                        }
                    ),
                    t.info,
                ))
            })
            .when(self.plugins.changes > 0 || self.plugins.scanning, |d| {
                d.child(dither::pulse(t.accent, false))
            })
    }

    /// Builds the sidebar of the Plugins workspace: the providers that filter the lists, with
    /// what each holds, and where Kodem writes.
    pub(in crate::ui) fn plugins_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        if self.reveals.sidebar <= 0. {
            return div().into_any_element();
        }
        let chosen = self.plugins.provider;
        let tab = self.plugins.tab;
        let installed = self.plugins.installed().to_vec();
        let mut providers = div().flex().flex_col().gap(px(2.));
        providers = providers.child(
            self.plugin_nav_row(
                ("plugins-provider-all", 0usize),
                chosen.is_none(),
                icon("layers").size(px(14.)).into_any_element(),
                "All providers".into(),
                match tab {
                    Tab::Servers => self.plugins.server_names().len(),
                    Tab::Skills => self.plugins.skills.skills.len(),
                },
                None,
            )
            .on_click(cx.listener(|s, _, _, cx| {
                s.plugins.provider = None;
                cx.notify();
            })),
        );
        for (index, provider) in Provider::ALL.into_iter().enumerate() {
            let present = installed.contains(&provider);
            let count = match tab {
                Tab::Servers => self
                    .plugins
                    .servers
                    .entries
                    .iter()
                    .filter(|entry| entry.provider == provider)
                    .count(),
                Tab::Skills => self
                    .plugins
                    .skills
                    .skills
                    .iter()
                    .filter(|skill| {
                        self.plugins.skills.reach(skill, provider) != skills::Reach::Missing
                    })
                    .count(),
            };
            let note = if self.plugins.installed.is_some() && !present {
                Some("NOT INSTALLED")
            } else if provider.beta() {
                Some("BETA")
            } else {
                None
            };
            providers = providers.child(
                self.plugin_nav_row(
                    ("plugins-provider", index + 1),
                    chosen == Some(provider),
                    icon(provider.icon()).size(px(14.)).into_any_element(),
                    provider.label().into(),
                    count,
                    note,
                )
                .when(self.plugins.installed.is_some() && !present, |row| {
                    row.opacity(0.55)
                })
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.plugins.provider = if s.plugins.provider == Some(provider) {
                        None
                    } else {
                        Some(provider)
                    };
                    cx.notify();
                })),
            );
        }
        let mut sources = div().flex().flex_col().gap(px(2.));
        if tab == Tab::Skills {
            let showing =
                self.plugins.scope == Scope::Discover && self.plugins.source == Source::Directory;
            sources = sources.child(
                self.plugin_nav_row(
                    "plugins-source-directory",
                    showing,
                    icon("globe").size(px(14.)).into_any_element(),
                    "skills.sh".into(),
                    self.plugins.directory.list.skills.len(),
                    Some("DIRECTORY"),
                )
                .on_click(cx.listener(|s, _, _, cx| {
                    s.plugins.scope = Scope::Discover;
                    s.plugins.source = Source::Directory;
                    s.plugins.detail = None;
                    s.plugins.form = None;
                    s.load_skill_discover(false, cx);
                    cx.notify();
                })),
            );
            for (index, source) in catalog::SKILL_SOURCES.iter().enumerate() {
                let count = match self.plugins.catalog_skills.get(&index) {
                    Some(Load::Ready(found)) => found.len(),
                    _ => 0,
                };
                let showing = self.plugins.scope == Scope::Discover
                    && self.plugins.source == Source::Repository(index);
                sources = sources.child(
                    self.plugin_nav_row(
                        ("plugins-source", index),
                        showing,
                        icon("github").size(px(14.)).into_any_element(),
                        format!("{}/{}", source.owner, source.repository),
                        count,
                        Some(source.label),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.plugins.scope = Scope::Discover;
                        s.plugins.source = Source::Repository(index);
                        s.plugins.detail = None;
                        s.plugins.form = None;
                        s.load_skill_discover(false, cx);
                        cx.notify();
                    })),
                );
            }
        }
        let footnote = match tab {
            Tab::Servers => {
                "Kodem writes each CLI's own configuration, for every account of the provider. Grok Build also loads Claude Code's servers. A running task sees a change from its next session."
            }
            Tab::Skills => {
                "Skills are copied into each provider's skill folder. OpenCode and Grok Build also read Claude Code's folder; Codex, OpenCode and Grok Build read ~/.agents/skills."
            }
        };
        div()
            .id("plugins-sidebar")
            .relative()
            .w(px(self.settings.sidebar_width))
            .flex_shrink_0()
            .h_full()
            .map(|panel| kit::panel_glass(panel, t))
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_size(text_px(12.))
            .child(
                div()
                    .id("plugins-sidebar-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(10.))
                    .pt(px(12.))
                    .pb(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(div().px(px(4.)).child(kit::section_header(
                                "Providers",
                                Some(installed.len()),
                                t,
                            )))
                            .child(providers),
                    )
                    .when(tab == Tab::Skills, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(div().px(px(4.)).child(kit::section_header(
                                    "Skill sources",
                                    Some(catalog::SKILL_SOURCES.len() + 1),
                                    t,
                                )))
                                .child(sources),
                        )
                    }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(14.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(t.border)
                    .text_size(text_px(11.))
                    .line_height(relative(1.45))
                    .text_color(t.subtle)
                    .child(footnote),
            )
            .into_any_element()
    }

    // A row of the Plugins sidebar: a glyph, a label, a count and a note. The chosen row takes
    // the accent edge of a selected row.
    fn plugin_nav_row(
        &self,
        id: impl Into<ElementId>,
        chosen: bool,
        glyph: AnyElement,
        label: String,
        count: usize,
        note: Option<&'static str>,
    ) -> Button {
        let t = self.theme;
        self.control(id)
            .h(px(30.))
            .px(px(8.))
            .gap(px(8.))
            .justify_start()
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .font_family(crate::theme::sans())
            .text_size(text_px(12.5))
            .font_weight(FontWeight::NORMAL)
            .text_color(if chosen { t.text } else { t.muted })
            .aria_selected(chosen)
            .when(chosen, |row| {
                row.overflow_hidden()
                    .bg(t.accent.opacity(0.1))
                    .layer(dither::row_wash(t.accent, true, false))
                    .child(kit::selection_edge(t.accent))
            })
            .when(!chosen, |row| {
                row.hover(move |row| row.bg(t.selected).text_color(t.text))
            })
            .child(glyph)
            .child(div().flex_1().min_w_0().text_ellipsis().child(label))
            .when_some(note, |row, note| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .font_family(crate::theme::mono())
                        .text_size(text_px(9.5))
                        .text_color(t.subtle)
                        .child(note),
                )
            })
            .when(count > 0, |row| {
                row.child(figure(
                    count.to_string(),
                    if chosen { t.accent } else { t.subtle },
                ))
            })
    }

    /// Builds the page of the Plugins workspace: its tabs, then the open detail, or the tools of
    /// the open tab over its grid of cards.
    pub(in crate::ui) fn plugins_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let body = match (&self.plugins.detail, self.plugins.tab) {
            (Some(_), Tab::Servers) => self.server_detail(window, cx),
            (Some(_), Tab::Skills) => self.skill_detail(window, cx),
            (None, _) => self.plugin_feed(cx),
        };
        div()
            .id("plugins-page")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.canvas)
            .child(self.plugin_tabs(cx))
            .when(self.plugins.detail.is_none(), |page| {
                page.child(self.plugin_toolbar(cx))
            })
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    // The grid of the open tab in a list that measures only the rows in view, with the bar of
    // its pages pinned under it. The rows are built again only when what they show changes,
    // since a list of the whole registry takes long to filter and each scroll renders again.
    // The list keeps its scroll while its rows keep their kinds, and starts at its top when it
    // shows something else.
    fn plugin_feed(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let columns = self.plugin_columns();
        let listing = format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{}",
            self.plugins.tab,
            self.plugins.scope,
            self.plugins.provider,
            self.plugins.category,
            self.plugins.source,
            self.plugins.query(cx).join(" "),
        );
        if listing != self.plugins.page_for {
            self.plugins.page = 0;
            self.plugins.page_for = listing.clone();
        }
        let key = format!("{listing}|{}|{columns}", self.plugins.page);
        let rows_key = format!("{key}|{}", self.rows_revision());
        if rows_key != self.plugins.rows_key {
            let mut rows = match self.plugins.tab {
                Tab::Servers => self.server_rows(columns, cx),
                Tab::Skills => self.skill_rows(columns, cx),
            };
            self.plugins.pager = rows.iter().find_map(|row| match row {
                GridRow::Pager { page, pages, total } => Some((*page, *pages, *total)),
                _ => None,
            });
            rows.retain(|row| !matches!(row, GridRow::Pager { .. }));
            if self.plugins.tab == Tab::Servers {
                self.plugins.category_counts = self.category_counts();
            }
            let shape: String = rows.iter().map(GridRow::kind).collect();
            let feed = &mut self.plugins.feed;
            if key != self.plugins.feed_key || feed.item_count() > rows.len() {
                feed.reset(rows.len());
            } else if shape != self.plugins.feed_shape {
                // Rows that arrive below keep the scroll; a change above measures again.
                let kept = shape
                    .chars()
                    .zip(self.plugins.feed_shape.chars())
                    .take_while(|(new, old)| new == old)
                    .count()
                    .min(feed.item_count());
                feed.splice(kept..feed.item_count(), rows.len() - kept);
            }
            self.plugins.feed_key = key;
            self.plugins.feed_shape = shape;
            self.plugins.rows = std::rc::Rc::new(rows);
            self.plugins.rows_key = rows_key;
        }
        let entity = cx.entity();
        let feed = list(self.plugins.feed.clone(), move |index, _, cx| {
            entity.update(cx, |s, cx| {
                let rows = s.plugins.rows.clone();
                match rows.get(index) {
                    Some(row) => s.grid_row(index, row, columns, cx),
                    None => div().into_any_element(),
                }
            })
        })
        .size_full();
        let pager = self
            .plugins
            .pager
            .map(|(page, pages, total)| self.plugin_pager(page, pages, total, cx));
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_1().min_h_0().child(feed))
            .when_some(pager, |d, pager| d.child(pager))
            .into_any_element()
    }

    // Names everything that the rows of a list show besides its tab, filters, page and width:
    // the lists, how their reads go and the inventory. The minute is part of it, for the age
    // of a list in a section's note.
    fn rows_revision(&self) -> String {
        let registry = &self.plugins.registry;
        let directory = &self.plugins.directory;
        let repository = match self.plugins.source {
            Source::Repository(index) => match self.plugins.catalog_skills.get(&index) {
                Some(Load::Ready(found)) => format!("{}", found.len()),
                Some(Load::Failed(error)) => format!("!{error}"),
                Some(_) => "…".into(),
                None => String::new(),
            },
            Source::Directory => String::new(),
        };
        format!(
            "{}|{}|{}|{:?}|{}|{}|{}|{}|{}|{}|{:?}|{}|{}|{}|{}",
            registry.servers.len(),
            registry.read_at,
            registry.reading,
            registry.error,
            found_mark(&registry.found),
            directory.list.skills.len(),
            directory.list.seeds,
            directory.list.read_at,
            directory.reading,
            directory.described,
            directory.error,
            found_mark(&directory.found),
            repository,
            self.plugins.scan_revision,
            catalog::now() / 60,
        )
    }

    // Builds one row of the grid, centered in the content's width.
    fn grid_row(
        &mut self,
        index: usize,
        row: &GridRow,
        columns: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let content = match row {
            GridRow::Banner => match self.plugins.tab {
                Tab::Servers => self.servers_banner(),
                Tab::Skills => self.skills_banner(),
            }
            .into_any_element(),
            GridRow::Section { label, count, note } => self
                .plugin_section(
                    label,
                    *count,
                    note.clone()
                        .map(|note| figure(note, t.subtle).into_any_element()),
                )
                .when(
                    self.plugins.registry.reading && *label == "All servers",
                    |row| {
                        row.child(kit::loader_row(
                            format!("READING · {}", self.plugins.registry.servers.len()),
                            t,
                        ))
                    },
                )
                .when_some(
                    self.plugins
                        .registry
                        .error
                        .clone()
                        .filter(|_| *label == "All servers"),
                    |row, error| {
                        row.child(
                            self.tinted("plugins-registry-retry", "refresh", "Retry", t.removed)
                                .h(px(22.))
                                .tooltip(move |_, cx| {
                                    cx.new(|_| Hint {
                                        text: error.clone(),
                                        theme: t,
                                    })
                                    .into()
                                })
                                .on_click(cx.listener(|s, _, _, cx| s.load_registry(true, cx))),
                        )
                    },
                )
                .into_any_element(),
            GridRow::Cards { cards, featured } => {
                let built: Vec<AnyElement> = cards
                    .iter()
                    .enumerate()
                    .map(|(at, card)| {
                        let id = index * 8 + at;
                        match card {
                            Card::Catalog(server) => {
                                self.catalog_card(id, *server, false, *featured, cx)
                            }
                            Card::Found(server) => {
                                self.catalog_card(id, *server, true, *featured, cx)
                            }
                            Card::Installed(name) => self.installed_card(id, name, cx),
                            Card::Skill(skill) => self.installed_skill_card(id, *skill, cx),
                            Card::CatalogSkill(skill) => self.catalog_skill_card(id, *skill, cx),
                            Card::Directory(_) | Card::DirectoryFound(_) => {
                                self.directory_card(id, card, cx)
                            }
                        }
                    })
                    .collect();
                self.card_row(built, columns)
                    .h(px(if *featured {
                        FEATURED_HEIGHT
                    } else {
                        CARD_HEIGHT
                    } + look::GAP))
                    .pb(px(look::GAP))
                    .into_any_element()
            }
            GridRow::SearchRegistry(query) => self.search_registry_row(index, query.clone(), cx),
            GridRow::Message {
                title,
                detail,
                error,
            } => {
                let mut message = div().w_full().h(px(240.)).child(kit::empty_state(
                    title.clone(),
                    detail.clone(),
                    dither::DitherField::Glow {
                        x: 0.5,
                        y: 0.5,
                        radius: 0.7,
                        strength: 0.8,
                    },
                    t,
                ));
                if *error {
                    message = message.child(
                        div().flex().justify_center().child(
                            self.button("plugins-message-retry", "refresh", "Try again")
                                .on_click(cx.listener(|s, _, _, cx| match s.plugins.tab {
                                    Tab::Servers => s.load_registry(true, cx),
                                    Tab::Skills => s.load_skill_discover(true, cx),
                                })),
                        ),
                    );
                }
                message.into_any_element()
            }
            GridRow::Loading(label) => div()
                .h(px(120.))
                .flex()
                .items_center()
                .justify_center()
                .child(kit::loader_row(label.clone(), t))
                .into_any_element(),
            GridRow::Pager { .. } => div().into_any_element(),
            GridRow::Space(height) => div().h(px(*height)).into_any_element(),
        };
        self.centered(content).into_any_element()
    }

    // The tab strip of the Plugins page, with the open tab's indicator, centered with the
    // content.
    // The bar of a long list's pages, pinned under it: the previous and next pages around which
    // cards show.
    fn plugin_pager(
        &self,
        page: usize,
        pages: usize,
        total: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let first = page * PAGE_SIZE + 1;
        let last = ((page + 1) * PAGE_SIZE).min(total);
        div()
            .flex_shrink_0()
            .w_full()
            .h(px(44.))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .bg(t.sidebar.opacity(kit::PANE_SHARE))
            .border_t_1()
            .border_color(t.border)
            .child(
                self.ghost("plugins-page-previous", "chevron-left", "Previous")
                    .h(px(28.))
                    .disabled(page == 0)
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.plugins.page = s.plugins.page.saturating_sub(1);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.))
                    .text_color(t.subtle)
                    .child(format!(
                        "{first}–{last} OF {total} · PAGE {} / {pages}",
                        page + 1
                    )),
            )
            .child(
                self.ghost("plugins-page-next", "chevron-right", "Next")
                    .h(px(28.))
                    .disabled(page + 1 >= pages)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.plugins.page = (page + 1).min(pages - 1);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn plugin_tabs(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut strip = div().h(px(42.)).flex().items_end().gap(px(4.));
        for (index, (tab, glyph, label, count)) in [
            (
                Tab::Servers,
                "plug",
                "MCP servers",
                self.plugins.server_names().len(),
            ),
            (
                Tab::Skills,
                "book",
                "Skills",
                self.plugins.skills.skills.len(),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let chosen = self.plugins.tab == tab;
            strip = strip.child(
                div()
                    .id(("plugins-tab", index))
                    .relative()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .cursor_pointer()
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if chosen { t.text } else { t.muted })
                    .when(!chosen, |tab| tab.hover(move |tab| tab.text_color(t.text)))
                    .child(icon(glyph).size(px(13.)).text_color(if chosen {
                        t.accent
                    } else {
                        t.subtle
                    }))
                    .child(label.to_uppercase())
                    .when(count > 0, |tab| {
                        tab.child(figure(
                            count.to_string(),
                            if chosen { t.accent } else { t.subtle },
                        ))
                    })
                    .when(chosen, |tab| tab.child(kit::tab_indicator(t.accent)))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if s.plugins.tab != tab {
                            s.plugins.tab = tab;
                            s.plugins.detail = None;
                            s.plugins.form = None;
                            if tab == Tab::Skills && s.plugins.scope == Scope::Discover {
                                s.load_skill_discover(false, cx);
                            }
                            cx.notify();
                        }
                    })),
            );
        }
        div()
            .flex_shrink_0()
            .bg(t.sidebar.opacity(kit::PANE_SHARE))
            .border_b_1()
            .border_color(t.border)
            .child(self.centered(strip))
    }

    // The tools above the grid, on one line: the search field, Discover or Installed, and the
    // tab's own actions; under them the categories of Discover.
    fn plugin_toolbar(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let tab = self.plugins.tab;
        let installed_count = match tab {
            Tab::Servers => self.plugins.server_names().len(),
            Tab::Skills => self.plugins.skills.skills.len(),
        };
        let scope = self.plugins.scope;
        let mut scopes = div()
            .flex()
            .flex_shrink_0()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .border_1()
            .border_color(t.edge)
            .bg(t.inset);
        for (index, (value, label)) in [
            (Scope::Discover, "Discover".to_owned()),
            (Scope::Installed, format!("Installed {installed_count}")),
        ]
        .into_iter()
        .enumerate()
        {
            let chosen = value == scope;
            scopes = scopes.child(
                self.control(("plugins-scope", index))
                    .h(px(24.))
                    .px(px(10.))
                    .rounded(px(crate::theme::RADIUS_SMALL))
                    .overflow_hidden()
                    .border_color(gpui::transparent_black())
                    .bg(gpui::transparent_black())
                    .text_color(if chosen { t.text } else { t.muted })
                    .when(chosen, |c| {
                        c.bg(kit::raised_fill(t.control))
                            .border_color(t.edge)
                            .shadow(kit::raised_light(t))
                    })
                    .when(!chosen, |c| c.hover(move |c| c.text_color(t.text)))
                    .child(label)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.plugins.scope = value;
                        if value == Scope::Discover && s.plugins.tab == Tab::Skills {
                            s.load_skill_discover(false, cx);
                        }
                        cx.notify();
                    })),
            );
        }
        let narrow = self.plugins_content_width() < 620.;
        let (add_label, add_hint) = match tab {
            Tab::Servers => (
                "Add custom",
                "Add a server from a form, a pasted configuration or a web search",
            ),
            Tab::Skills => (
                "Add skill",
                "Add a skill from GitHub or a folder, or write a new one",
            ),
        };
        let tools = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                settings::input_frame(t)
                    .flex_1()
                    .min_w(px(120.))
                    .h(px(32.))
                    .border_color(t.border)
                    .child(icon("search").size(px(13.)).text_color(t.subtle))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(12.))
                            .child(self.plugins.search.clone()),
                    )
                    .child(kit::keycap("⌘F", t)),
            )
            .child(scopes)
            .child(
                self.button("plugins-add", "plus", if narrow { "" } else { add_label })
                    .h(px(30.))
                    .flex_shrink_0()
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint {
                            text: add_hint.into(),
                            theme: t,
                        })
                        .into()
                    })
                    .on_click(cx.listener(move |s, _, w, cx| match tab {
                        Tab::Servers => s.open_custom_server(None, w, cx),
                        Tab::Skills => s.open_add_skill(w, cx),
                    })),
            )
            .child(
                self.icon_button(
                    "plugins-refresh",
                    "refresh",
                    "Read the configurations and catalogs again",
                )
                .h(px(30.))
                .w(px(30.))
                .flex_shrink_0()
                .disabled(self.plugins.scanning)
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.scan_plugins(cx);
                    match s.plugins.tab {
                        Tab::Servers => s.load_registry(true, cx),
                        Tab::Skills => s.load_skill_discover(true, cx),
                    }
                })),
            );
        // The categories wrap under the tools, each in the look of the tabs above.
        let categories = (tab == Tab::Servers && scope == Scope::Discover).then(|| {
            let counts = &self.plugins.category_counts;
            let mut strip = div().w_full().flex().flex_wrap().gap(px(2.));
            for (index, category) in Category::ALL.into_iter().enumerate() {
                let chosen = self.plugins.category == category;
                let count = counts.get(index).copied().unwrap_or_default();
                if category != Category::All && count == 0 {
                    continue;
                }
                strip = strip.child(
                    div()
                        .id(("plugins-category", index))
                        .relative()
                        .flex_shrink_0()
                        .h(px(30.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .cursor_pointer()
                        .font_family(crate::theme::mono())
                        .text_size(text_px(10.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if chosen { t.text } else { t.muted })
                        .when(!chosen, |tab| tab.hover(move |tab| tab.text_color(t.text)))
                        .child(icon(category.glyph()).size(px(12.)).text_color(if chosen {
                            category.tone(t)
                        } else {
                            t.subtle
                        }))
                        .child(category.label().to_uppercase())
                        .child(figure(
                            count.to_string(),
                            if chosen { t.accent } else { t.subtle },
                        ))
                        .when(chosen, |tab| tab.child(kit::tab_indicator(t.accent)))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.plugins.category = category;
                            cx.notify();
                        })),
                );
            }
            strip
        });
        div()
            .flex_shrink_0()
            .pt(px(14.))
            .pb(px(10.))
            .border_b_1()
            .border_color(t.border.opacity(0.6))
            .child(
                self.centered(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(tools)
                        .when_some(categories, |d, strip| d.child(strip)),
                ),
            )
    }

    /// Returns the category of each registry server, by index, kept beside the list.
    pub(in crate::ui) fn server_categories(&self) -> &[Category] {
        &self.plugins.categories
    }

    /// Returns the category of a registry server: the kept one for a server of the list, by
    /// index, else one found from its text.
    pub(in crate::ui) fn server_category(
        &self,
        server: &catalog::CatalogServer,
        index: Option<usize>,
    ) -> Category {
        index
            .and_then(|index| self.plugins.categories.get(index).copied())
            .unwrap_or_else(|| categorize(std::slice::from_ref(server))[0])
    }

    // Returns how many servers of the registry each category holds, in the order of
    // `Category::ALL`.
    fn category_counts(&self) -> Vec<usize> {
        let mut counts = vec![0; Category::ALL.len()];
        counts[0] = self.plugins.registry.servers.len();
        for category in &self.plugins.categories {
            if let Some(index) = Category::ALL.iter().position(|known| known == category) {
                counts[index] += 1;
            }
        }
        counts
    }

    /// Returns the logos of the providers that hold something, lit for those that use it and
    /// faint for the others. `reach` says how each installed provider has it.
    pub(in crate::ui) fn provider_marks(&self, reach: &[(Provider, Mark)]) -> Div {
        let t = self.theme;
        let mut marks = div().flex().flex_shrink_0().items_center().gap(px(3.));
        for provider in self.plugins.installed() {
            let mark = reach
                .iter()
                .find(|(known, _)| known == provider)
                .map(|(_, mark)| *mark)
                .unwrap_or(Mark::Missing);
            let (tone, border, fill) = match mark {
                Mark::On => (t.text, t.edge, t.raised),
                Mark::Off => (t.muted, t.border, gpui::transparent_black()),
                Mark::Shared => (t.muted, t.border, gpui::transparent_black()),
                Mark::Missing => (
                    t.subtle.opacity(0.45),
                    gpui::transparent_black(),
                    gpui::transparent_black(),
                ),
            };
            marks = marks.child(
                div()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(crate::theme::RADIUS_SMALL))
                    .border_1()
                    .border_color(border)
                    .bg(fill)
                    .when(mark == Mark::Shared, |d| d.border_dashed())
                    .child(icon(provider.icon()).size(px(11.)).text_color(tone)),
            );
        }
        marks
    }

    /// Builds the choice of providers for an installation: a switch for each installed provider,
    /// on by default. `unsupported` gives the reason a provider cannot take it.
    pub(in crate::ui) fn provider_choices(
        &self,
        id: &'static str,
        chosen: &[Provider],
        unsupported: impl Fn(Provider) -> Option<String>,
        toggle: impl Fn(&mut Shell, Provider, bool) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let installed = self.plugins.installed().to_vec();
        let mut rows = div().flex().flex_col();
        if installed.is_empty() {
            return rows.child(
                div()
                    .py(px(8.))
                    .text_size(text_px(12.))
                    .text_color(t.subtle)
                    .child("No provider CLI is installed. Install one from Settings → Providers."),
            );
        }
        for (index, provider) in installed.into_iter().enumerate() {
            let reason = unsupported(provider);
            let on = reason.is_none() && chosen.contains(&provider);
            let toggle = toggle.clone();
            rows = rows.child(
                div()
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(t.border.opacity(0.6))
                    .child(kit::icon_tile(
                        provider.icon(),
                        if on { t.accent } else { t.subtle },
                        24.,
                        false,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_size(text_px(12.5))
                                    .text_color(if reason.is_some() { t.subtle } else { t.text })
                                    .child(provider.label())
                                    .when(provider.beta(), |d| d.child(kit::beta_chip(t))),
                            )
                            .when_some(reason.clone(), |d, reason| {
                                d.child(
                                    div()
                                        .text_size(text_px(11.))
                                        .text_color(t.subtle)
                                        .child(reason),
                                )
                            }),
                    )
                    .child(
                        controls::Switch::new(format!("{id}-{index}"), on)
                            .label(provider.label())
                            .disabled(reason.is_some())
                            .on_click(cx.listener(move |s, enabled: &bool, _, cx| {
                                toggle(s, provider, *enabled);
                                cx.notify();
                            })),
                    ),
            );
        }
        rows
    }
}

/// Returns failures that name their provider as lines of a report.
pub(in crate::ui) fn labelled(failures: Vec<(Provider, String)>) -> Vec<String> {
    failures
        .into_iter()
        .map(|(provider, error)| format!("{}: {error}", provider.label()))
        .collect()
}

/// A row of a tab's grid.
#[derive(Clone, Debug)]
pub(in crate::ui) enum GridRow {
    /// The banner at the top of the tab.
    Banner,
    /// The header of a section.
    Section {
        label: String,
        count: Option<usize>,
        note: Option<String>,
    },
    /// One row of cards.
    Cards { cards: Vec<Card>, featured: bool },
    /// The offer to search the registry for the query.
    SearchRegistry(String),
    /// An empty or failed list, with a retry when it failed.
    Message {
        title: String,
        detail: String,
        error: bool,
    },
    /// Something that loads.
    Loading(String),
    /// The pages of a list longer than `PAGE_SIZE`, with the shown page counted from zero.
    Pager {
        page: usize,
        pages: usize,
        total: usize,
    },
    /// Free space, in points.
    Space(f32),
}

impl GridRow {
    /// Returns a letter for the row's kind, which decides its height.
    fn kind(&self) -> char {
        match self {
            Self::Banner => 'B',
            Self::Section { .. } => 'S',
            Self::Cards { featured: true, .. } => 'F',
            Self::Cards { .. } => 'C',
            Self::SearchRegistry(_) => 'R',
            Self::Message { .. } => 'M',
            Self::Loading(_) => 'L',
            Self::Pager { .. } => 'P',
            Self::Space(_) => '_',
        }
    }
}

/// A card of the grid.
#[derive(Clone, Debug)]
pub(in crate::ui) enum Card {
    /// A server of the kept registry list, by index.
    Catalog(usize),
    /// A server that the registry's search found, by index.
    Found(usize),
    /// An installed server, by name.
    Installed(String),
    /// An installed skill, by index.
    Skill(usize),
    /// A skill of the open source, by index.
    CatalogSkill(usize),
    /// A skill of the kept skills.sh list, by index.
    Directory(usize),
    /// A skill that the skills.sh search found, by index.
    DirectoryFound(usize),
}

/// Most cards on one page of a list, which bounds the rows that a render builds.
pub(in crate::ui) const PAGE_SIZE: usize = 500;

/// Adds the rows of one page of `cards` to `rows`, with the pager under them when the cards
/// fill more than one page. A page past the last shows the last.
pub(in crate::ui) fn paged_rows(
    cards: Vec<Card>,
    page: usize,
    columns: usize,
    rows: &mut Vec<GridRow>,
) {
    let total = cards.len();
    let pages = total.div_ceil(PAGE_SIZE).max(1);
    let page = page.min(pages - 1);
    let shown: Vec<Card> = cards
        .into_iter()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
        .collect();
    rows.extend(card_rows(shown, columns, false));
    if pages > 1 {
        rows.push(GridRow::Pager { page, pages, total });
    }
}

/// Splits cards into rows of `columns`.
pub(in crate::ui) fn card_rows(cards: Vec<Card>, columns: usize, featured: bool) -> Vec<GridRow> {
    cards
        .chunks(columns.max(1))
        .map(|chunk| GridRow::Cards {
            cards: chunk.to_vec(),
            featured,
        })
        .collect()
}

/// How a provider has a server or a skill, for its mark in a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Mark {
    /// Installed and in use.
    On,
    /// Installed and switched off.
    Off,
    /// Read from a folder that another provider owns.
    Shared,
    /// Not installed for the provider.
    Missing,
}

/// Returns a one-line input that holds `text`.
pub(in crate::ui) fn single_line_input(
    placeholder: &str,
    text: String,
    cx: &mut Context<Shell>,
) -> Entity<TextInput> {
    let input = cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx);
        input.set_single_line(true);
        input
    });
    if !text.is_empty() {
        input.update(cx, |input, cx| input.set_text(text, cx));
    }
    input
}

/// Returns an input of several lines that holds `text`.
pub(in crate::ui) fn multi_line_input(
    placeholder: &str,
    text: String,
    cx: &mut Context<Shell>,
) -> Entity<TextInput> {
    let input = cx.new(|cx| TextInput::new(placeholder, cx));
    if !text.is_empty() {
        input.update(cx, |input, cx| input.set_text(text, cx));
    }
    input
}

/// Returns the placeholder of a variable's field: what it is for, else its name.
fn input_hint(input: &catalog::Input) -> String {
    if input.secret {
        "Secret value".into()
    } else if !input.default.is_empty() {
        input.default.clone()
    } else {
        "Value".into()
    }
}

/// Returns a label of how long ago a time in seconds since the Unix epoch was.
pub(in crate::ui) fn age(seconds: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let elapsed = now.saturating_sub(seconds);
    match elapsed {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", elapsed / 60),
        3600..86400 => format!("{} h ago", elapsed / 3600),
        _ => format!("{} d ago", elapsed / 86400),
    }
}
