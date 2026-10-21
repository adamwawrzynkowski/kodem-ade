//! The dialog that imports the conversations of Claude Code, Codex, OpenCode, Antigravity and Grok
//! into Kodem: the threads that the host found in the folders of projects, filters by provider and
//! project, the user's choice among them, and the progress of the imports, which run one after
//! another.
use super::*;

/// Tallest list of threads, in points. A longer list scrolls.
const LIST_HEIGHT: f32 = 360.;
/// Width of the progress meter, in points.
const METER_WIDTH: f32 = 140.;
/// Height of one thread row, in points.
const ROW_HEIGHT: f32 = 30.;
/// Age groups of the list: the label and the oldest age in hours that the group holds.
const AGES: [(&str, u64); 4] = [
    ("Last 24 hours", 24),
    ("Last 7 days", 24 * 7),
    ("Last 30 days", 24 * 30),
    ("Older", u64::MAX),
];

/// What the import dialog found, chose and imported.
#[derive(Default)]
pub(super) struct ThreadImport {
    /// The threads that the host found. It is absent until the first listing answers.
    found: Option<ExternalThreads>,
    loading: bool,
    /// Why the threads could not be listed.
    error: Option<String>,
    /// The provider whose threads the list shows; all providers when absent.
    provider: Option<Provider>,
    /// The project whose threads the list shows; all projects when absent.
    project: Option<String>,
    /// Keys of the threads that the user chose.
    chosen: HashSet<String>,
    /// Threads that wait for their import, in order.
    queue: VecDeque<ExternalThread>,
    /// Key of the thread that the host imports now.
    current: Option<String>,
    /// Number of threads of the latest run of imports.
    total: usize,
    /// The outcome of each thread of the latest run, by key: no error or the error.
    outcomes: HashMap<String, Option<String>>,
    /// The task of each thread that this dialog imported, by key.
    imported: HashMap<String, String>,
}

impl ThreadImport {
    // Reports whether an import runs or waits.
    fn running(&self) -> bool {
        self.current.is_some() || !self.queue.is_empty()
    }

    // Returns every thread that the host found.
    fn threads(&self) -> impl Iterator<Item = &ExternalThread> {
        self.found.iter().flat_map(|found| found.threads.iter())
    }

    // Reports whether the filters show a thread.
    fn shows(&self, thread: &ExternalThread) -> bool {
        self.provider
            .is_none_or(|provider| thread.provider == provider)
            && self
                .project
                .as_ref()
                .is_none_or(|project| &thread.project_id == project)
    }

    // Returns the threads that the filters show.
    fn shown(&self) -> impl Iterator<Item = &ExternalThread> {
        self.threads().filter(|thread| self.shows(thread))
    }

    // Returns the shown threads that can still be imported.
    fn available(&self) -> impl Iterator<Item = &ExternalThread> {
        self.shown()
            .filter(|thread| !self.imported.contains_key(&thread.key()))
    }
}

impl Shell {
    /// Opens the import dialog and lists the threads again, unless an import runs.
    pub(super) fn open_thread_import(&mut self) {
        self.panel = Panel::ImportThreads;
        if !self.thread_import.running() {
            self.list_external_threads();
        }
    }

    // Asks the host for the threads that can be imported.
    fn list_external_threads(&mut self) {
        self.thread_import.loading = true;
        self.thread_import.error = None;
        self.send(core::Request::ExternalThreads);
    }

    /// Applies the host's answer to a listing of threads.
    pub(super) fn receive_external_threads(&mut self, result: Result<Response, String>) {
        let state = &mut self.thread_import;
        state.loading = false;
        match result {
            Ok(Response::ExternalThreads(found)) => {
                // A choice stays only while its thread is still listed.
                state
                    .chosen
                    .retain(|key| found.threads.iter().any(|thread| &thread.key() == key));
                // A filter without threads would show an empty list.
                if state.provider.is_some_and(|provider| {
                    !found
                        .threads
                        .iter()
                        .any(|thread| thread.provider == provider)
                }) {
                    state.provider = None;
                }
                if state.project.as_ref().is_some_and(|project| {
                    !found
                        .threads
                        .iter()
                        .any(|thread| &thread.project_id == project)
                }) {
                    state.project = None;
                }
                state.found = Some(found);
            }
            Ok(Response::Error(error)) | Err(error) => state.error = Some(error),
            Ok(_) => state.error = Some("The host gave an unexpected answer".into()),
        }
    }

    /// Records the outcome of one import and starts the next one.
    pub(super) fn receive_imported_thread(
        &mut self,
        key: String,
        result: Result<Response, String>,
    ) {
        let state = &mut self.thread_import;
        let outcome = match result {
            Ok(Response::Imported { task, .. }) => {
                state.imported.insert(key.clone(), task.id);
                state.chosen.remove(&key);
                None
            }
            Ok(Response::Error(error)) | Err(error) => Some(error),
            Ok(_) => Some("The host gave an unexpected answer".into()),
        };
        state.outcomes.insert(key.clone(), outcome);
        if state.current.as_ref() == Some(&key) {
            state.current = None;
        }
        // Each import adds a task, which the sidebar lists from the workspace.
        self.send(core::Request::Hello);
        self.import_next_thread();
    }

    // Starts a run of imports of `threads`, one after another.
    fn import_threads(&mut self, threads: Vec<ExternalThread>) {
        let state = &mut self.thread_import;
        if state.running() || threads.is_empty() {
            return;
        }
        state.total = threads.len();
        state.outcomes.clear();
        state.queue = threads.into();
        self.import_next_thread();
    }

    // Sends the next waiting import to the host.
    fn import_next_thread(&mut self) {
        let state = &mut self.thread_import;
        if state.current.is_some() {
            return;
        }
        let Some(thread) = state.queue.pop_front() else {
            return;
        };
        state.current = Some(thread.key());
        self.send(core::Request::ImportThread {
            provider: thread.provider,
            account: thread.account,
            thread: thread.thread,
        });
    }

    // Opens an imported thread's task in Agentic Coding.
    fn open_imported_thread(&mut self, task_id: String, cx: &mut Context<Self>) {
        self.panel = Panel::None;
        if self.mode != TaskKind::Coding {
            self.enter_mode(TaskKind::Coding, cx);
        }
        self.select_task(task_id, cx);
    }

    // Returns the name of the project of a thread.
    fn import_project_name(&self, project_id: &str) -> String {
        self.workspace
            .as_ref()
            .and_then(|workspace| {
                workspace
                    .projects
                    .iter()
                    .find(|project| project.id == project_id)
            })
            .map_or_else(|| "Project".to_owned(), |project| project.name.clone())
    }

    // Returns the account's name when the provider has more than one account.
    fn import_account_name(&self, thread: &ExternalThread) -> Option<String> {
        let workspace = self.workspace.as_ref()?;
        let accounts = workspace
            .accounts
            .iter()
            .filter(|state| state.account.provider == thread.provider)
            .count();
        (accounts > 1)
            .then(|| workspace.account(thread.provider, Some(&thread.account)))
            .flatten()
            .map(|state| state.account.name.clone())
    }

    // Returns the tabs that filter the list by provider, each with its number of threads.
    fn import_provider_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let state = &self.thread_import;
        let mut tabs = div()
            .h(px(32.))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(t.border);
        let options = std::iter::once(None).chain(Provider::ALL.into_iter().map(Some));
        for (index, provider) in options.enumerate() {
            let count = state
                .threads()
                .filter(|thread| provider.is_none_or(|provider| thread.provider == provider))
                .count();
            // A provider without threads has nothing to show.
            if provider.is_some() && count == 0 {
                continue;
            }
            let chosen = state.provider == provider;
            tabs = tabs.child(
                div()
                    .id(("import-provider", index))
                    .relative()
                    .h_full()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .cursor_pointer()
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.5))
                    .text_color(if chosen { t.text } else { t.muted })
                    .when(!chosen, |tab| tab.hover(move |tab| tab.bg(t.selected)))
                    .when_some(provider, |tab, provider| {
                        tab.child(icon(provider.icon()).size(px(13.)))
                    })
                    .child(provider.map_or("All", Provider::label))
                    .child(figure(count.to_string(), t.subtle))
                    .when(chosen, |tab| tab.child(kit::tab_indicator(t.accent)))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.thread_import.provider = provider;
                        cx.notify();
                    })),
            );
        }
        tabs.into_any_element()
    }

    // Returns the choice of project, shown when threads come from more than one project.
    fn import_project_filter(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mut projects: Vec<String> = Vec::new();
        for thread in self.thread_import.threads() {
            if !projects.contains(&thread.project_id) {
                projects.push(thread.project_id.clone());
            }
        }
        if projects.len() < 2 {
            return None;
        }
        let options = std::iter::once((None, "All projects".to_owned())).chain(
            projects
                .into_iter()
                .map(|project| (Some(project.clone()), self.import_project_name(&project))),
        );
        Some(self.segmented(
            "import-project",
            options,
            &self.thread_import.project.clone(),
            |s, project, cx| {
                s.thread_import.project = project;
                cx.notify();
            },
            cx,
        ))
    }

    // Returns one thread's row: its choice box or its state, provider, title, project and age.
    fn import_row(
        &self,
        index: usize,
        thread: &ExternalThread,
        project: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let state = &self.thread_import;
        let key = thread.key();
        let running = state.running();
        let chosen = state.chosen.contains(&key);
        let importing = state.current.as_ref() == Some(&key);
        let waiting = state.queue.iter().any(|queued| queued.key() == key);
        let failure = state.outcomes.get(&key).and_then(|outcome| outcome.clone());
        let imported = state.imported.get(&key).cloned();
        let choosable = imported.is_none() && !running;
        let (glyph, tone) = if imported.is_some() {
            ("circle-check", t.added)
        } else if failure.is_some() {
            ("circle-x", t.removed)
        } else if importing || waiting {
            ("loader", t.accent)
        } else if chosen {
            ("check-square", t.accent)
        } else {
            ("square", t.subtle)
        };
        let account = self.import_account_name(thread);
        let line = div()
            .h(px(ROW_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icon(glyph).size(px(13.)).text_color(tone))
            .child(
                icon(thread.provider.icon())
                    .size(px(13.))
                    .text_color(t.muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(text_px(13.))
                    .text_color(if imported.is_some() { t.muted } else { t.text })
                    .text_ellipsis()
                    .child(thread.title.clone()),
            )
            .when_some(account, |line, account| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(120.))
                        .text_ellipsis()
                        .child(figure(account, t.subtle)),
                )
            })
            .when(project, |line| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(140.))
                        .text_ellipsis()
                        .child(figure(
                            self.import_project_name(&thread.project_id),
                            t.subtle,
                        )),
                )
            })
            .child(
                div()
                    .w(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .justify_end()
                    .child(figure(sidebar::elapsed(thread.updated), t.subtle)),
            )
            .when(importing, |line| {
                line.child(kit::chip("IMPORTING", t.accent))
            })
            .when_some(imported, |line, task_id| {
                line.child(
                    self.ghost(("open-imported-thread", index), "arrow-right", "Open")
                        .h(px(22.))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.open_imported_thread(task_id.clone(), cx)
                        })),
                )
            });
        div()
            .id(("import-thread", index))
            .relative()
            .overflow_hidden()
            .w_full()
            .flex_shrink_0()
            .px(px(8.))
            .flex()
            .flex_col()
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .aria_selected(chosen)
            .when(chosen, |row| {
                row.bg(t.accent.opacity(0.1))
                    .layer(dither::row_wash(t.accent, true, false))
                    .child(kit::selection_edge(t.accent))
            })
            .when(choosable, |row| {
                row.cursor_pointer()
                    .when(!chosen, |row| row.hover(move |row| row.bg(t.selected)))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        let chosen = &mut s.thread_import.chosen;
                        if !chosen.remove(&key) {
                            chosen.insert(key.clone());
                        }
                        cx.notify();
                    }))
            })
            .child(line)
            .when_some(failure, |row, failure| {
                row.child(
                    div()
                        .pl(px(42.))
                        .pb(px(6.))
                        .text_size(text_px(11.5))
                        .line_height(relative(1.4))
                        .text_color(t.removed)
                        .child(failure),
                )
            })
            .into_any_element()
    }

    /// Builds the content of the import dialog: the filters and the threads by age.
    pub(super) fn thread_import(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let state = &self.thread_import;
        let mut content = div().flex().flex_col().gap(px(10.)).child(
            div()
                .text_size(text_px(13.))
                .line_height(relative(1.45))
                .text_color(t.muted)
                .child("Conversations that you had in Claude Code, Codex, OpenCode, Antigravity and Grok, in the folders of your projects. An imported thread keeps its history and continues in Kodem."),
        );
        if let Some(error) = &state.error {
            content = content.child(settings::error_block("Listing failed", error.clone(), t));
        }
        for error in state.found.iter().flat_map(|found| found.errors.iter()) {
            content = content.child(settings::error_block("Account not read", error.clone(), t));
        }
        let projects = self.import_project_filter(cx);
        // The project shows on each row only while the list mixes projects.
        let mixed = projects.is_some() && state.project.is_none();
        if let Some(projects) = projects {
            content = content.child(projects);
        }
        let threads: Vec<(usize, &ExternalThread)> = state
            .threads()
            .enumerate()
            .filter(|(_, thread)| state.shows(thread))
            .collect();
        let mut list = div()
            .id("import-thread-list")
            .max_h(px(LIST_HEIGHT))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(1.))
            .p(px(4.));
        if threads.is_empty() {
            list = list.child(if state.loading {
                div()
                    .py(px(24.))
                    .flex()
                    .justify_center()
                    .child(kit::loader_row("Looking for threads…", t))
            } else {
                kit::empty_hint(
                    "No conversations of Claude Code, Codex, OpenCode, Antigravity or Grok were found in the folders of your projects.",
                    t,
                )
            });
        }
        // The host lists the most recent threads first, so each age group is in order.
        let mut younger = 0;
        for (group, (label, hours)) in AGES.into_iter().enumerate() {
            let range = younger..hours.saturating_mul(3_600_000);
            younger = range.end;
            let members: Vec<&(usize, &ExternalThread)> = threads
                .iter()
                .filter(|(_, thread)| range.contains(&now().saturating_sub(thread.updated)))
                .collect();
            if members.is_empty() {
                continue;
            }
            list = list.child(
                div()
                    .when(group > 0, |header| header.pt(px(6.)))
                    .px(px(8.))
                    .pb(px(2.))
                    .child(kit::section_header(label, Some(members.len()), t)),
            );
            for (index, thread) in members {
                list = list.child(self.import_row(*index, thread, mixed, cx));
            }
        }
        content = content.child(
            kit::block(t)
                .child(self.import_provider_tabs(cx))
                .child(list),
        );
        if let Some(found) = state.found.as_ref().filter(|found| found.elsewhere > 0) {
            content = content.child(settings::note(
                format!(
                    "{} more {} in folders that are not projects. Add a folder as a project to import its threads.",
                    found.elsewhere,
                    if found.elsewhere == 1 { "thread is" } else { "threads are" }
                ),
                t,
            ));
        }
        content.into_any_element()
    }

    /// Returns the actions of the thread import: look again, choose threads and import them,
    /// with the progress of a run between them.
    pub(super) fn thread_import_actions(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let state = &self.thread_import;
        let running = state.running();
        let chosen: Vec<ExternalThread> = state
            .available()
            .filter(|thread| state.chosen.contains(&thread.key()))
            .cloned()
            .collect();
        let available: Vec<ExternalThread> = state.available().cloned().collect();
        let every_chosen = !available.is_empty() && chosen.len() == available.len();
        let mut actions = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                self.icon_button(
                    "refresh-import-threads",
                    "refresh",
                    "Look for threads again",
                )
                .disabled(running || state.loading)
                .on_click(cx.listener(|s, _, _, cx| {
                    s.list_external_threads();
                    cx.notify();
                })),
            )
            .child(
                self.ghost(
                    "choose-all-import-threads",
                    if every_chosen {
                        "square"
                    } else {
                        "check-square"
                    },
                    if every_chosen {
                        "Clear choice"
                    } else {
                        "Choose all shown"
                    },
                )
                .disabled(running || available.is_empty())
                .on_click(cx.listener(move |s, _, _, cx| {
                    let keys: Vec<String> = s
                        .thread_import
                        .available()
                        .map(ExternalThread::key)
                        .collect();
                    let chosen = &mut s.thread_import.chosen;
                    if every_chosen {
                        chosen.clear();
                    } else {
                        chosen.extend(keys);
                    }
                    cx.notify();
                })),
            );
        // The progress of a run, or of the last run, sits between the choice and the action.
        if state.total > 0 {
            let finished = state.outcomes.len();
            let failed = state
                .outcomes
                .values()
                .filter(|outcome| outcome.is_some())
                .count();
            let label = if running {
                format!(
                    "Importing {} of {}",
                    (finished + 1).min(state.total),
                    state.total
                )
            } else if failed > 0 {
                format!(
                    "Imported {} of {} · {failed} failed",
                    finished - failed,
                    state.total
                )
            } else {
                format!("Imported {finished} of {}", state.total)
            };
            let tone = if failed > 0 { t.warning } else { t.accent };
            actions = actions
                .child(div().w(px(8.)))
                .child(kit::meter(
                    Some(finished as f32 / state.total as f32),
                    tone,
                    METER_WIDTH,
                    t,
                ))
                .child(figure(label, if failed > 0 { t.warning } else { t.muted }));
        }
        actions = actions.child(div().flex_1());
        if running {
            actions = actions.child(
                self.ghost("stop-import-threads", "stop", "Stop after this thread")
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.thread_import.total -= s.thread_import.queue.len();
                        s.thread_import.queue.clear();
                        cx.notify();
                    })),
            );
        } else {
            // One action: the chosen threads, or every shown thread when none is chosen.
            let (label, threads) = if chosen.is_empty() {
                (format!("Import all shown ({})", available.len()), available)
            } else {
                (format!("Import chosen ({})", chosen.len()), chosen)
            };
            actions = actions.child(
                self.primary("import-threads", "arrow-down-to-line", label)
                    .disabled(threads.is_empty())
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.import_threads(threads.clone());
                        cx.notify();
                    })),
            );
        }
        actions
    }
}
