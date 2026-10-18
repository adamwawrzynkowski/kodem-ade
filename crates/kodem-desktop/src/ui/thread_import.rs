//! The dialog that imports the conversations of Claude Code and Codex into Kodem: the threads
//! that the host found in the folders of projects, the user's choice among them, and the
//! progress of the imports, which run one after another.
use super::*;

/// Tallest list of threads, in points. A longer list scrolls.
const LIST_HEIGHT: f32 = 380.;
/// Width of the progress meter, in points.
const METER_WIDTH: f32 = 160.;

/// What the import dialog found, chose and imported.
#[derive(Default)]
pub(super) struct ThreadImport {
    /// The threads that the host found. It is absent until the first listing answers.
    found: Option<ExternalThreads>,
    loading: bool,
    /// Why the threads could not be listed.
    error: Option<String>,
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
}

impl ThreadImport {
    // Reports whether an import runs or waits.
    fn running(&self) -> bool {
        self.current.is_some() || !self.queue.is_empty()
    }

    // Returns the threads that can still be imported.
    fn available(&self) -> impl Iterator<Item = &ExternalThread> {
        self.found
            .iter()
            .flat_map(|found| found.threads.iter())
            .filter(|thread| thread.task_id.is_none())
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
                // A choice stays only while its thread can still be imported.
                state.chosen.retain(|key| {
                    found
                        .threads
                        .iter()
                        .any(|thread| thread.task_id.is_none() && &thread.key() == key)
                });
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
                if let Some(thread) = state
                    .found
                    .iter_mut()
                    .flat_map(|found| found.threads.iter_mut())
                    .find(|thread| thread.key() == key)
                {
                    thread.task_id = Some(task.id);
                }
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

    // Returns the name of a project for a group of threads.
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

    // Returns one thread's row: its choice box or its state, provider, title and age.
    fn import_row(
        &self,
        index: usize,
        thread: &ExternalThread,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let key = thread.key();
        let running = self.thread_import.running();
        let chosen = self.thread_import.chosen.contains(&key);
        let importing = self.thread_import.current.as_ref() == Some(&key);
        let waiting = self
            .thread_import
            .queue
            .iter()
            .any(|queued| queued.key() == key);
        let outcome = self.thread_import.outcomes.get(&key);
        let failure = outcome.and_then(|outcome| outcome.clone());
        let imported = thread.task_id.clone();
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
        let status = if importing {
            Some(("IMPORTING", t.accent))
        } else if waiting {
            Some(("WAITING", t.subtle))
        } else if imported.is_some() {
            Some(("IN KODEM", t.added))
        } else if failure.is_some() {
            Some(("FAILED", t.removed))
        } else {
            None
        };
        let account = self.import_account_name(thread);
        let row = div()
            .id(("import-thread", index))
            .relative()
            .overflow_hidden()
            .w_full()
            .flex_shrink_0()
            .px(px(8.))
            .py(px(6.))
            .flex()
            .gap(px(8.))
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
            .child(
                div()
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .child(icon(glyph).size(px(13.)).text_color(tone)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .h(px(20.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
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
                            .when_some(status, |line, (label, tone)| {
                                line.child(kit::chip(label, tone))
                            })
                            .child(figure(sidebar::elapsed(thread.updated), t.subtle)),
                    )
                    .child(
                        div()
                            .h(px(16.))
                            .pl(px(21.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .font_family(crate::theme::mono())
                            .text_size(text_px(10.5))
                            .text_color(t.subtle)
                            .child(thread.provider.label())
                            .when_some(account, |line, account| {
                                line.child("·").child(
                                    div()
                                        .flex_shrink_0()
                                        .max_w(relative(0.3))
                                        .text_ellipsis()
                                        .child(account),
                                )
                            })
                            .child("·")
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(thread.thread.clone()),
                            ),
                    )
                    .when_some(failure, |details, failure| {
                        details.child(
                            div()
                                .pl(px(21.))
                                .text_size(text_px(11.5))
                                .line_height(relative(1.4))
                                .text_color(t.removed)
                                .child(failure),
                        )
                    }),
            )
            .when_some(imported, |row, task_id| {
                row.child(
                    div().h(px(20.)).flex().items_center().child(
                        self.ghost(("open-imported-thread", index), "arrow-right", "Open")
                            .h(px(20.))
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.open_imported_thread(task_id.clone(), cx)
                            })),
                    ),
                )
            });
        row.into_any_element()
    }

    /// Builds the content of the import dialog: the threads by project, the choice and the
    /// progress of the imports.
    pub(super) fn thread_import(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let state = &self.thread_import;
        let running = state.running();
        let mut content = div().flex().flex_col().gap(px(12.)).child(
            div()
                .text_size(text_px(13.))
                .line_height(relative(1.45))
                .text_color(t.muted)
                .child("Kodem lists the conversations that Claude Code and Codex stored on this computer in the folders of your projects. An imported thread shows its history and continues the same conversation."),
        );
        if let Some(error) = &state.error {
            content = content.child(settings::error_block("Listing failed", error.clone(), t));
        }
        for error in state.found.iter().flat_map(|found| found.errors.iter()) {
            content = content.child(settings::error_block("Account not read", error.clone(), t));
        }
        let threads: Vec<&ExternalThread> = state
            .found
            .iter()
            .flat_map(|found| found.threads.iter())
            .collect();
        let available = state.available().count();
        let mut list = div()
            .id("import-thread-list")
            .max_h(px(LIST_HEIGHT))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.))
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
                    "No threads of Claude Code or Codex were found in the folders of your projects.",
                    t,
                )
            });
        }
        // Threads are grouped by project, in the order of each project's latest thread.
        let mut projects: Vec<&str> = Vec::new();
        for thread in &threads {
            if !projects.contains(&thread.project_id.as_str()) {
                projects.push(&thread.project_id);
            }
        }
        for (group, project) in projects.into_iter().enumerate() {
            list = list.child(
                div()
                    .when(group > 0, |header| header.pt(px(8.)))
                    .px(px(4.))
                    .pb(px(2.))
                    .child(kit::section_header(
                        self.import_project_name(project),
                        Some(
                            threads
                                .iter()
                                .filter(|thread| thread.project_id == project)
                                .count(),
                        ),
                        t,
                    )),
            );
            for (index, thread) in threads.iter().enumerate() {
                if thread.project_id == project {
                    list = list.child(self.import_row(index, thread, cx));
                }
            }
        }
        content = content.child(kit::block(t).child(list));
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
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(kit::meter(
                        Some(finished as f32 / state.total as f32),
                        if failed > 0 { t.warning } else { t.accent },
                        METER_WIDTH,
                        t,
                    ))
                    .child(figure(label, if failed > 0 { t.warning } else { t.muted })),
            );
        }
        let chosen: Vec<ExternalThread> = state
            .available()
            .filter(|thread| state.chosen.contains(&thread.key()))
            .cloned()
            .collect();
        let all: Vec<ExternalThread> = state.available().cloned().collect();
        let every_chosen = available > 0 && chosen.len() == available;
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
                        "Choose all"
                    },
                )
                .disabled(running || available == 0)
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
            )
            .child(div().flex_1());
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
            let count = all.len();
            actions = actions
                .child(
                    self.button(
                        "import-all-threads",
                        "arrow-down-to-line",
                        format!("Import all ({count})"),
                    )
                    .disabled(count == 0)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.import_threads(all.clone());
                        cx.notify();
                    })),
                )
                .child(
                    self.primary(
                        "import-chosen-threads",
                        "arrow-down-to-line",
                        format!("Import chosen ({})", chosen.len()),
                    )
                    .disabled(chosen.is_empty())
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.import_threads(chosen.clone());
                        cx.notify();
                    })),
                );
        }
        content
            .child(
                div()
                    .mt(px(2.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
            )
            .child(actions)
            .into_any_element()
    }
}
