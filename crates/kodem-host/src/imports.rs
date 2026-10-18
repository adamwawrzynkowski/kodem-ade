//! Finds the conversations that Claude Code and Codex stored outside Kodem, and imports them as
//! tasks that continue those conversations.
use crate::{Shared, accounts, new_task, provider};
use anyhow::{Context, Result, ensure};
use kodem_core::{git, platform, *};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// A conversation as the provider stored it, before it is matched with a project.
pub(crate) struct Stored {
    pub(crate) thread: String,
    pub(crate) title: String,
    pub(crate) cwd: PathBuf,
    /// Wall-clock milliseconds of the first and the last change of the conversation.
    pub(crate) created: u64,
    pub(crate) updated: u64,
    /// The model of the conversation's latest turn, when the provider recorded it.
    pub(crate) model: Option<String>,
}

/// Longest title of an imported conversation, in characters.
const TITLE_LIMIT: usize = 80;

/// Returns the first line of a conversation's opening text as its title.
pub(crate) fn title(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if line.chars().count() <= TITLE_LIMIT {
        return line.to_owned();
    }
    let mut title: String = line.chars().take(TITLE_LIMIT - 1).collect();
    title.push('…');
    title
}

/// Returns a comparable form of a folder: its canonical path when it exists, and on Windows
/// without letter case or forward slashes, because the file system ignores both.
fn folder_key(path: &Path) -> String {
    let path = path
        .canonicalize()
        .map(platform::plain_path)
        .unwrap_or_else(|_| path.to_owned());
    let text = path.to_string_lossy();
    let text = text.trim_end_matches(['/', '\\']);
    if cfg!(windows) {
        text.replace('/', "\\").to_lowercase()
    } else {
        text.to_owned()
    }
}

/// Matches the folders of conversations with the folders of projects, reading each folder once.
struct Folders {
    projects: Vec<(String, String)>,
    /// Folders whose conversations are Kodem's own: its worktrees, chats, reviews and the
    /// temporary folder that title generation runs in.
    own: Vec<String>,
    known: HashMap<PathBuf, Option<String>>,
}

impl Folders {
    fn new(projects: &[Project], root: &Path) -> Self {
        Self {
            projects: projects
                .iter()
                .map(|project| (folder_key(&project.path), project.id.clone()))
                .collect(),
            own: [root.to_owned(), std::env::temp_dir()]
                .iter()
                .map(|path| folder_key(path))
                .collect(),
            known: HashMap::new(),
        }
    }

    /// Returns the project of `cwd`, or `None` for a folder that no project holds.
    fn project(&mut self, cwd: &Path) -> Option<String> {
        if let Some(known) = self.known.get(cwd) {
            return known.clone();
        }
        let key = folder_key(cwd);
        let project = self
            .projects
            .iter()
            .find(|(folder, _)| *folder == key)
            .map(|(_, id)| id.clone());
        self.known.insert(cwd.to_owned(), project.clone());
        project
    }

    /// Reports a folder of Kodem's own, whose conversations are not counted as importable.
    fn own(&self, cwd: &Path) -> bool {
        let key = folder_key(cwd);
        let separator = if cfg!(windows) { '\\' } else { '/' };
        self.own.iter().any(|own| {
            key.strip_prefix(own.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(separator))
        })
    }
}

/// Lists the stored conversations of every Claude Code and Codex account.
pub(crate) fn discover(host: &Shared) -> Result<ExternalThreads> {
    let (projects, tasks, accounts, root) = {
        let h = host.lock().unwrap();
        (
            h.store.projects()?,
            h.store.tasks()?,
            h.store.settings()?.accounts,
            h.store.root().to_owned(),
        )
    };
    let mut folders = Folders::new(&projects, &root);
    let mut found = ExternalThreads::default();
    for account in accounts {
        let stored = match provider::stored_threads(account.provider, &account) {
            Ok(stored) => stored,
            Err(error) => {
                found.errors.push(format!(
                    "{} · {}: {error:#}",
                    account.provider.label(),
                    account.name
                ));
                continue;
            }
        };
        for thread in stored {
            let Some(project_id) = folders.project(&thread.cwd) else {
                if !folders.own(&thread.cwd) {
                    found.elsewhere += 1;
                }
                continue;
            };
            let task_id = tasks
                .iter()
                .find(|task| {
                    task.provider == account.provider
                        && task.provider_thread.as_deref() == Some(&thread.thread)
                })
                .map(|task| task.id.clone());
            found.threads.push(ExternalThread {
                provider: account.provider,
                account: account.id.clone(),
                thread: thread.thread,
                title: thread.title,
                project_id,
                cwd: thread.cwd,
                created: thread.created,
                updated: thread.updated,
                task_id,
            });
        }
    }
    found
        .threads
        .sort_by_key(|thread| std::cmp::Reverse(thread.updated));
    Ok(found)
}

// Returns options for the imported task: the provider's last options, with the model of the
// conversation when the provider still offers it.
fn preferences(
    h: &crate::Host,
    account: &Account,
    model: Option<String>,
) -> Result<AgentPreferences> {
    let mut preferences = h
        .store
        .settings()?
        .provider_preferences
        .get(&account.provider)
        .cloned()
        .unwrap_or_else(|| AgentPreferences {
            provider: account.provider,
            ..AgentPreferences::default()
        });
    preferences.account = (!account.is_default()).then(|| account.id.clone());
    let models = h
        .runtimes
        .get(&account.provider)
        .map(|runtime| runtime.models.as_slice())
        .unwrap_or_default();
    if let Some(model) = model.and_then(|id| models.iter().find(|known| known.id == id))
        && preferences.model.as_deref() != Some(&model.id)
    {
        preferences.effort = preferences
            .effort
            .filter(|effort| model.efforts.contains(effort));
        preferences.model = Some(model.id.clone());
    }
    Ok(preferences)
}

/// Creates a task that holds the stored conversation `thread` of `account` with its history.
///
/// The task works in the project's own checkout, as the conversation did. A failed import
/// removes the task again, so a later import starts clean.
pub(crate) fn import(
    host: &Shared,
    provider: Provider,
    account: &str,
    thread: &str,
) -> Result<Task> {
    let account = accounts::resolve(&host.lock().unwrap(), provider, Some(account))?;
    let stored = provider::stored_thread(provider, &account, thread)?;
    let project = {
        let h = host.lock().unwrap();
        let projects = h.store.projects()?;
        let mut folders = Folders::new(&projects, h.store.root());
        let id = folders.project(&stored.cwd).with_context(|| {
            format!(
                "No project holds {}. Add the folder as a project to import this conversation.",
                stored.cwd.display()
            )
        })?;
        projects
            .into_iter()
            .find(|project| project.id == id)
            .context("Project is no longer available")?
    };
    let mut task = {
        let h = host.lock().unwrap();
        let preferences = preferences(&h, &account, stored.model.clone())?;
        new_task(
            new_id(),
            TaskKind::Coding,
            project.id.clone(),
            project.path.clone(),
            &preferences,
        )
    };
    task.baseline = git::head(&task.cwd);
    task.repository_baselines = git::repository_baselines(&task.cwd)?;
    task.branch = git::branch(&task.cwd);
    task.title = if stored.title.is_empty() {
        "Imported conversation".into()
    } else {
        stored.title
    };
    task.title_pending = false;
    task.provider_thread = Some(stored.thread);
    task.created = stored.created;
    task.updated = stored.updated;
    task.last_started = Some(stored.updated);
    {
        let h = host.lock().unwrap();
        ensure!(
            !h.store
                .tasks()?
                .iter()
                .any(|known| known.provider == provider
                    && known.provider_thread == task.provider_thread),
            "This conversation is already in Kodem"
        );
        h.store.save_task(&task)?;
    }
    let replayed = provider::import_history(host, &task, &account);
    let mut h = host.lock().unwrap();
    // The replay leaves no live work: activities, requests and subagent states are history.
    h.activities.remove(&task.id);
    h.requests.remove(&task.id);
    if let Err(error) = replayed {
        h.store.delete_task(&task.id)?;
        return Err(error);
    }
    let mut task = h.store.task(&task.id)?;
    for agent in &mut task.subagents {
        if agent.state.active() || agent.state == RunState::DeliveryUnknown {
            agent.state = RunState::Completed;
            agent.active_turn = None;
        }
    }
    // A step list or a question of an earlier session is not current work.
    task.plan.clear();
    task.questions.clear();
    task.state = RunState::Idle;
    task.active_turn = None;
    task.error = None;
    h.store.save_task(&task)?;
    Ok(task)
}
