//! Lets an agent of one task give work to another task, on any provider and model.
//!
//! The agent reaches these operations through the tools of `mcp`. A delegated task is an
//! ordinary task with a `Lineage` that names its parent, so it has its own conversation, its
//! own approvals, and a place in the task list. A task that may not write reads the checkout
//! of its parent while the parent works. One that may write gets a worktree of its own.
use crate::{Host, Shared, provider};
use anyhow::{Context, Result, bail, ensure};
use kodem_core::{git, *};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Most delegated tasks of one parent that work at the same time.
const CHILD_LIMIT: usize = 4;
/// Most text of a result that is returned to the parent's agent, in bytes.
const RESULT_LIMIT: usize = 64 * 1024;
/// Longest wait of one status request. The client's read timeout is longer.
const WAIT_LIMIT: Duration = Duration::from_secs(45);
const SUBAGENT_NOTE: &str = "You work as a subagent for another agent in Kodem. That agent sees only your last message, so end with one complete report of what you found or did.";

// A model name without case, spaces and punctuation, so that "GPT 6.1 Sol" finds "gpt-6.1-sol".
fn plain(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Lists the models that an agent of `parent` can give work to, reading a provider's catalog
/// first when the host has not read it yet. That read makes no model request.
pub(crate) fn agents(host: &Shared, parent: &str) -> Result<Vec<AgentOption>> {
    let cwd = host.lock().unwrap().store.task(parent)?.cwd;
    for provider in Provider::ALL {
        let known = host
            .lock()
            .unwrap()
            .runtimes
            .get(&provider)
            .is_some_and(|runtime| !runtime.models.is_empty());
        if !known {
            // A provider that is not installed or not signed in offers no model.
            let _ = provider::discover(host.clone(), provider, None, Some(cwd.clone()));
        }
    }
    let h = host.lock().unwrap();
    Ok(Provider::ALL
        .into_iter()
        .flat_map(|provider| {
            let runtime = h.runtimes.get(&provider).cloned().unwrap_or_default();
            let default = runtime.default_model.clone();
            runtime.models.into_iter().map(move |model| AgentOption {
                provider,
                default: default.as_deref() == Some(model.id.as_str()),
                model: model.id,
                name: model.name,
            })
        })
        .collect())
}

// Chooses the provider and model of a delegated task. A named model decides the provider.
fn choose(
    host: &Shared,
    parent: &Task,
    provider: Option<Provider>,
    model: Option<&str>,
) -> Result<(Provider, Option<String>)> {
    let Some(model) = model.filter(|model| !model.trim().is_empty()) else {
        return Ok((provider.unwrap_or(parent.provider), None));
    };
    let wanted = plain(model);
    let options: Vec<AgentOption> = agents(host, &parent.id)?
        .into_iter()
        .filter(|option| provider.is_none_or(|provider| option.provider == provider))
        .collect();
    let exact =
        |option: &&AgentOption| plain(&option.model) == wanted || plain(&option.name) == wanted;
    let partial = |option: &&AgentOption| {
        plain(&option.model).contains(&wanted) || plain(&option.name).contains(&wanted)
    };
    let found = options
        .iter()
        .find(exact)
        .or_else(|| options.iter().find(partial))
        .with_context(|| {
            format!(
                "No available model matches \"{model}\". Available: {}.",
                options
                    .iter()
                    .map(|option| option.model.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    Ok((found.provider, Some(found.model.clone())))
}

// The first line of a task text, as the title of the delegated task.
fn title(prompt: &str) -> String {
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Delegated task");
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    title
}

// The row that marks a transfer between a task and the task it delegated to.
fn transfer_row(host: &Host, transcript: &str, id: String, transfer: Transfer) -> Message {
    Message {
        seq: 0,
        text: provider::transfer_text(host, &transfer),
        transfer: Some(transfer),
        id,
        task_id: transcript.into(),
        kind: MessageKind::Notice,
        attachments: Vec::new(),
        feedback: Vec::new(),
        file_edits: Vec::new(),
        tool: None,
        complete: true,
        created: now(),
    }
}

/// Starts a task for an agent of `parent`, sends it `prompt`, and waits up to `wait_ms` for
/// its result.
pub(crate) fn delegate(
    host: &Shared,
    root: &Path,
    parent: &str,
    prompt: &str,
    choice: (Option<Provider>, Option<&str>),
    write: bool,
    wait_ms: u64,
) -> Result<Delegation> {
    let (child, text) = prepare(host, root, parent, prompt, choice, write)?;
    provider::submit(
        host.clone(),
        child.id.clone(),
        new_id(),
        text,
        Vec::new(),
        Vec::new(),
        SendMode::Queue,
    )?;
    status(host, parent, &child.id, wait_ms)
}

// Creates the delegated task and marks the transfer in both conversations. Returns the task
// and the message that starts it.
fn prepare(
    host: &Shared,
    root: &Path,
    parent: &str,
    prompt: &str,
    choice: (Option<Provider>, Option<&str>),
    write: bool,
) -> Result<(Task, String)> {
    ensure!(!prompt.trim().is_empty(), "Write the task for the subagent");
    let parent = {
        let h = host.lock().unwrap();
        ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
        let parent = h.store.task(parent)?;
        ensure!(
            !parent
                .lineage
                .as_ref()
                .is_some_and(|lineage| lineage.delegated),
            "A delegated task cannot delegate further"
        );
        let working = h
            .store
            .tasks()?
            .iter()
            .filter(|task| {
                task.state.active()
                    && task
                        .lineage
                        .as_ref()
                        .is_some_and(|lineage| lineage.delegated && lineage.parent == parent.id)
            })
            .count();
        ensure!(
            working < CHILD_LIMIT,
            "{CHILD_LIMIT} delegated tasks of this task already work. Wait for one to finish."
        );
        parent
    };
    let (provider, model) = choose(host, &parent, choice.0, choice.1)?;
    let preferences = AgentPreferences {
        provider,
        // Another provider's account is not this task's to choose: its default one is used.
        account: (provider == parent.provider)
            .then(|| parent.account.clone())
            .flatten(),
        provider_access: None,
        model,
        effort: None,
        access: if write {
            parent.access
        } else {
            AccessMode::ReadOnly
        },
    };
    provider::validate_preferences(&host.lock().unwrap(), &preferences)?;
    let id = new_id();
    let cwd = if write {
        ensure!(
            parent.kind == TaskKind::Coding,
            "Only a coding task can delegate work that changes files"
        );
        let project = crate::project(host, &parent.project_id)?;
        let head = git::head(&parent.cwd)
            .context("Create an initial commit before delegating work that changes files")?;
        // The worktree starts at the parent's commit and receives its present files, so the
        // subagent also sees work that the parent has not committed.
        let name = format!("delegate-{}", &id[..8]);
        git::checkpoint::capture(&parent.cwd, &parent.id, &name)?;
        let cwd = git::worktree(&project.path, &root.join("worktrees"), &id, Some(&head))?;
        git::checkpoint::apply(&cwd, &parent.id, &name)?;
        cwd
    } else {
        parent.cwd.clone()
    };
    let mut child = crate::new_task(
        id,
        parent.kind,
        parent.project_id.clone(),
        cwd,
        &preferences,
    );
    child.title = title(prompt);
    child.title_pending = false;
    child.owns_worktree = write;
    child.review_scope = parent.review_scope.clone();
    if write {
        child.baseline = git::head(&child.cwd);
        child.repository_baselines = git::repository_baselines(&child.cwd)?;
        child.branch = git::branch(&child.cwd);
    } else {
        child.baseline = parent.baseline.clone();
        child.branch = parent.branch.clone();
    }
    let mut text = format!("{}\n\n{SUBAGENT_NOTE}", prompt.trim());
    if write {
        text.push_str(&format!(
            " You have a worktree of your own on branch {}. Commit your work there: the other agent reads it from that branch.",
            child.branch
        ));
    } else {
        text.push_str(" You can read the files of the checkout. Do not change them.");
    }
    {
        let h = host.lock().unwrap();
        child.lineage = Some(Lineage {
            parent: parent.id.clone(),
            turn: h.store.latest_run(&parent.id)?.map_or(0, |run| run.ordinal),
            delegated: true,
        });
        h.store.save_task(&child)?;
        let mut to = provider::agent_of(&h, &child);
        to.task_id = Some(child.id.clone());
        let transfer = Transfer {
            kind: TransferKind::Delegated,
            from: provider::agent_of(&h, &parent),
            to,
        };
        // Both conversations show the transfer: where the work went, and where it came from.
        for transcript in [&parent.id, &child.id] {
            h.store.message(&transfer_row(
                &h,
                transcript,
                format!("delegated:{}", child.id),
                transfer.clone(),
            ))?;
        }
    }
    Ok((child, text))
}

// A task that the given parent delegated, or an error that names neither another task's
// content nor its existence.
fn child_of(host: &Host, parent: &str, task_id: &str) -> Result<Task> {
    host.store
        .task(task_id)
        .ok()
        .filter(|task| {
            task.lineage
                .as_ref()
                .is_some_and(|lineage| lineage.delegated && lineage.parent == parent)
        })
        .context("This task did not delegate a task with that task_id")
}

/// Reads a delegated task, waiting up to `wait_ms` until it stopped working.
///
/// The first read of a finished task writes the transfer of its result into the parent's
/// conversation.
pub(crate) fn status(
    host: &Shared,
    parent: &str,
    task_id: &str,
    wait_ms: u64,
) -> Result<Delegation> {
    let until = Instant::now() + Duration::from_millis(wait_ms).min(WAIT_LIMIT);
    loop {
        {
            let h = host.lock().unwrap();
            ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
            let child = child_of(&h, parent, task_id)?;
            let waiting = h.store.outgoing(task_id)?.iter().any(|message| {
                matches!(
                    message.state,
                    DeliveryState::Queued | DeliveryState::Pending
                )
            });
            let done = !child.state.active() && !waiting;
            if done || Instant::now() >= until {
                return report(&h, parent, &child, done);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

// The replies of the last turn of a task, newest kept when they exceed the limit.
fn result(host: &Host, task_id: &str) -> Result<Option<String>> {
    let first = host
        .store
        .latest_run(task_id)?
        .map_or(0, |run| run.first_seq);
    let mut replies: Vec<String> = Vec::new();
    let mut size = 0usize;
    let mut before = None;
    'pages: loop {
        let (messages, older) = host.store.messages(task_id, before)?;
        for message in messages.iter().rev() {
            if message.seq < first {
                break 'pages;
            }
            if message.kind != MessageKind::Assistant || message.text.trim().is_empty() {
                continue;
            }
            if size + message.text.len() > RESULT_LIMIT {
                break 'pages;
            }
            size += message.text.len();
            replies.push(message.text.trim().to_owned());
        }
        match older {
            Some(older) => before = Some(older),
            None => break,
        }
    }
    replies.reverse();
    Ok((!replies.is_empty()).then(|| replies.join("\n\n")))
}

fn report(host: &Host, parent: &str, child: &Task, done: bool) -> Result<Delegation> {
    let mut agent = provider::agent_of(host, child);
    if done {
        let id = format!("result:{}", child.id);
        if !host.store.has_message(parent, &id)? {
            let parent_task = host.store.task(parent)?;
            agent.task_id = Some(child.id.clone());
            let transfer = Transfer {
                kind: TransferKind::Result,
                from: agent.clone(),
                to: provider::agent_of(host, &parent_task),
            };
            host.store
                .message(&transfer_row(host, parent, id, transfer))?;
        }
    }
    Ok(Delegation {
        task_id: child.id.clone(),
        agent: provider::agent_name(host, &agent),
        state: child.state.clone(),
        done,
        needs_input: child.needs_input(),
        result: if done { result(host, &child.id)? } else { None },
        error: child.error.clone(),
        worktree: child
            .owns_worktree
            .then(|| (child.branch.clone(), child.cwd.clone())),
    })
}

/// Stops the running turn of a delegated task and returns its state shortly after.
pub(crate) fn cancel(host: &Shared, parent: &str, task_id: &str) -> Result<Delegation> {
    {
        let h = host.lock().unwrap();
        let child = child_of(&h, parent, task_id)?;
        if !child.state.active() {
            return report(&h, parent, &child, true);
        }
        let Some(session) = h.sessions.get(task_id) else {
            bail!("The task has no live provider session to stop");
        };
        session.send(provider::Control::Interrupt)?;
    }
    status(host, parent, task_id, 3000)
}

/// Stops the delegated tasks of a task that the user stopped, so that no work goes on unseen.
pub(crate) fn stop_children(host: &Host, parent: &str) -> Result<()> {
    for task in host.store.tasks()? {
        let delegated = task
            .lineage
            .as_ref()
            .is_some_and(|lineage| lineage.delegated && lineage.parent == parent);
        if delegated
            && task.state.active()
            && let Some(session) = host.sessions.get(&task.id)
        {
            let _ = session.send(provider::Control::Interrupt);
        }
    }
    Ok(())
}
