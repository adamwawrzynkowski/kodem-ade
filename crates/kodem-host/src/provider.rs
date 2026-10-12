//! Provider routing and durable, per-task conversation ownership.
use crate::{Shared, accounts};
#[path = "providers/claude.rs"]
mod claude;
#[path = "providers/codex.rs"]
mod codex;
use anyhow::{Context, Result, ensure};
use kodem_core::*;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

pub(super) enum Control {
    Close,
    Wake,
    Interrupt,
    /// Runs the compaction that `begin_compaction` recorded for this task.
    Compact,
    Answer {
        id: Value,
        response: Value,
    },
    Query {
        method: &'static str,
        params: Value,
        reply: mpsc::SyncSender<std::result::Result<Value, String>>,
    },
}

/// State a task returns to when its host-requested compaction ends.
pub(crate) struct Compaction {
    state: RunState,
    error: Option<String>,
    updated: u64,
}

pub(crate) fn prompt_for(message: &OutgoingMessage) -> Result<String> {
    let mut prompt = message.text.clone();
    for attachment in &message.attachments {
        if let AttachmentKind::Skill { source } = &attachment.kind {
            prompt.push_str(&format!(
                "\n\nUse the skill at {}. Resolve its relative resources from the original skill directory.\nImmutable copy of the selected SKILL.md: {}",
                serde_json::to_string(source)?,
                serde_json::to_string(&attachment.path)?
            ));
        }
        if attachment.kind == AttachmentKind::File {
            prompt.push_str(&format!(
                "\n\nAttached file: {}\nImmutable local copy: {}",
                serde_json::to_string(&attachment.name)?,
                serde_json::to_string(&attachment.path)?
            ));
        }
    }
    if !message.feedback.is_empty() {
        prompt.push_str("\n\nReview context refers to captured content. Check current files before applying it:\n");
        for feedback in &message.feedback {
            if let Some(root) = &feedback.repository {
                prompt.push_str(&format!(
                    "\nRepository checkout: {}\n",
                    serde_json::to_string(root)?
                ));
            }
            let coordinates = if feedback.first_row.is_some() {
                format!(
                    "Displayed diff rows {:?}–{:?}; old source {:?}, new source {:?}",
                    feedback.first_row, feedback.last_row, feedback.old_range, feedback.new_range
                )
            } else {
                format!(
                    "Source {:?} lines {}–{}",
                    feedback.side, feedback.start, feedback.end
                )
            };
            prompt.push_str(&format!(
                "\nFile: {}\nSnapshot: {}\n{}\nComment: {}\nCaptured excerpt:\n{}\n",
                serde_json::to_string(&feedback.label)?,
                feedback.snapshot_id,
                coordinates,
                feedback.comment,
                feedback.excerpt
            ));
        }
    }
    ensure!(
        prompt.len() <= 2 * 1024 * 1024,
        "Prompt and context exceed 2 MiB; send a smaller batch"
    );
    Ok(prompt)
}

/// Durably accepts a new input without making the window an execution owner.
pub(super) fn submit(
    host: Shared,
    task_id: String,
    id: String,
    text: String,
    feedback_ids: Vec<String>,
    attachment_ids: Vec<String>,
    mode: SendMode,
) -> Result<()> {
    ensure!(text.len() <= 1024 * 1024, "Prompt exceeds 1 MiB");
    ensure!(
        !text.trim().is_empty() || !feedback_ids.is_empty() || !attachment_ids.is_empty(),
        "Write a prompt or attach context"
    );
    ensure!(
        !id.is_empty() && id.len() <= 128,
        "Invalid submission identity"
    );
    {
        let h = host.lock().unwrap();
        ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
        if h.store
            .matches_submission(&task_id, &id, &text, &feedback_ids, &attachment_ids, mode)?
        {
            return Ok(());
        }
        let mut task = h.store.task(&task_id)?;
        validate_preferences(&h, &preferences(&task))?;
        ensure!(
            mode != SendMode::Steer
                || h.runtimes
                    .get(&task.provider)
                    .is_some_and(|runtime| runtime.steering),
            "This provider does not support steering. Queue the message instead."
        );
        ensure!(
            task.state != RunState::DeliveryUnknown,
            "Delivery is unknown. Reconcile before sending another message."
        );
        ensure!(
            h.store.outgoing(&task_id)?.len() < 64,
            "Outbox is full; send or remove existing messages first"
        );
        ensure!(
            !h.store
                .tasks()?
                .iter()
                .any(|t| t.id != task.id && t.cwd == task.cwd && t.state.active()),
            "Another task is writing to this checkout. Use a separate task worktree."
        );
        // A compaction has no turn to join, so input sent meanwhile starts the next turn.
        let steers =
            task.state.active() && mode == SendMode::Steer && !h.compactions.contains_key(&task_id);
        let expected_turn = if steers {
            Some(task.active_turn.clone().context("The turn is still starting. Queue this message or wait until it begins before steering.")?)
        } else {
            None
        };
        let attachments = attachment_ids
            .iter()
            .map(|id| {
                task.attachments
                    .iter()
                    .find(|a| &a.id == id)
                    .cloned()
                    .context("Attachment is no longer in this draft")
            })
            .collect::<Result<Vec<_>>>()?;
        let feedback = feedback_ids
            .iter()
            .map(|id| {
                task.feedback
                    .iter()
                    .find(|f| &f.id == id)
                    .cloned()
                    .context("Context is no longer in this draft")
            })
            .collect::<Result<Vec<_>>>()?;
        validate_attachments(&h, &task, &attachments)?;
        let message = OutgoingMessage {
            id,
            text: text.clone(),
            attachments,
            feedback,
            created: now(),
            state: DeliveryState::Queued,
            error: None,
            mode,
            expected_turn,
        };
        let prompt = prompt_for(&message)?;
        if task.draft == text {
            task.draft.clear();
        }
        task.attachments.retain(|a| !attachment_ids.contains(&a.id));
        task.feedback.retain(|f| !feedback_ids.contains(&f.id));
        if !task.state.active() {
            task.state = RunState::Idle;
            task.error = None;
        }
        task.updated = now();
        task.last_started = Some(task.updated);
        h.store.accept_outgoing(&task, &message, &prompt)?;
    }
    ensure_owner(host, task_id);
    Ok(())
}

/// Promotes queued input to steering and wakes its existing provider session.
pub(super) fn steer_queued(host: Shared, task_id: &str, id: &str) -> Result<()> {
    let h = host.lock().unwrap();
    let task = h.store.task(task_id)?;
    ensure!(
        h.runtimes
            .get(&task.provider)
            .is_some_and(|runtime| runtime.steering),
        "This provider does not support steering"
    );
    ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
    ensure!(
        !h.compactions.contains_key(task_id),
        "Steer after the context compaction finishes"
    );
    let sender = h
        .sessions
        .get(task_id)
        .context("No live provider session to steer")?;
    h.store.steer_outgoing(task_id, id)?;
    sender.send(Control::Wake)?;
    Ok(())
}

/// Explicitly resumes retained queued input after a failure or a host restart.
pub(super) fn resume_queue(host: Shared, task_id: &str) -> Result<()> {
    {
        let h = host.lock().unwrap();
        let mut task = h.store.task(task_id)?;
        ensure!(!task.state.active(), "This task is already running");
        ensure!(
            task.state != RunState::DeliveryUnknown
                && !h.store.has_uncertain_submission(task_id)?,
            "Reconcile uncertain delivery before resuming the queue"
        );
        ensure!(
            h.store
                .outgoing(task_id)?
                .iter()
                .any(|m| m.state == DeliveryState::Queued && m.expected_turn.is_none()),
            "No queued messages to resume"
        );
        task.state = RunState::Idle;
        task.error = None;
        h.save_task_with_notification(&task)?;
    }
    ensure_owner(host, task_id.into());
    Ok(())
}

/// Starts a compaction of an idle task's conversation through its provider session.
///
/// A running turn is neither interrupted nor queued behind: the request is refused until the
/// turn ends. The result arrives with the task: its activity, context usage and transcript.
pub(super) fn compact(host: Shared, task_id: &str) -> Result<()> {
    {
        let mut h = host.lock().unwrap();
        ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
        let mut task = h.store.task(task_id)?;
        ensure!(
            !task.state.active() && !h.compactions.contains_key(task_id),
            "Compact after this turn finishes"
        );
        ensure!(
            task.state != RunState::DeliveryUnknown
                && !h.store.has_uncertain_submission(task_id)?,
            "Reconcile uncertain delivery before compacting"
        );
        ensure!(
            task.provider_thread.is_some(),
            "There is nothing to compact yet. Send a message first."
        );
        validate_preferences(&h, &preferences(&task))?;
        begin_compaction(&mut h, &mut task)?;
    }
    ensure_owner(host.clone(), task_id.into());
    let sent = host
        .lock()
        .unwrap()
        .sessions
        .get(task_id)
        .is_some_and(|session| session.send(Control::Compact).is_ok());
    if !sent {
        end_compaction(&host, task_id, None)?;
        anyhow::bail!("The provider session ended. Try again.");
    }
    Ok(())
}

/// Marks a host-requested compaction as running and remembers the state the task returns to.
///
/// The task counts as working meanwhile, so queued input waits and nothing else starts a turn.
pub(crate) fn begin_compaction(host: &mut crate::Host, task: &mut Task) -> Result<()> {
    host.compactions.insert(
        task.id.clone(),
        Compaction {
            state: task.state.clone(),
            error: task.error.take(),
            updated: task.updated,
        },
    );
    task.state = RunState::Running;
    task.active_turn = None;
    host.store.save_task(task)
}

/// Returns a task to the state it had before its compaction, without a completion alert.
///
/// A failure leaves a notice in the transcript. It never fails the task or its queued input.
pub(crate) fn end_compaction(host: &Shared, task_id: &str, failure: Option<String>) -> Result<()> {
    let mut h = host.lock().unwrap();
    let Some(previous) = h.compactions.remove(task_id) else {
        return Ok(());
    };
    h.activities.remove(task_id);
    let mut task = h.store.task(task_id)?;
    task.state = previous.state;
    task.error = previous.error;
    task.updated = previous.updated;
    task.active_turn = None;
    // The reading a compaction leaves behind must not start the next one.
    if let Some(usage) = &task.context_usage {
        h.compacted.insert(task_id.into(), usage.reported);
    }
    if let Some(failure) = failure {
        h.store.message(&Message {
            seq: 0,
            id: format!("compact:{}", new_id()),
            task_id: task_id.into(),
            kind: MessageKind::Notice,
            text: format!("Context was not compacted: {failure}"),
            attachments: Vec::new(),
            feedback: Vec::new(),
            file_edits: Vec::new(),
            tool: None,
            complete: true,
            created: now(),
        })?;
    }
    h.store.save_task(&task)
}

/// Returns the task when it is idle with input waiting to start its next turn.
pub(crate) fn waiting_turn(host: &crate::Host, task_id: &str) -> Result<Option<Task>> {
    let task = host.store.task(task_id)?;
    let waiting = matches!(task.state, RunState::Idle | RunState::Completed)
        && !host.store.has_uncertain_submission(task_id)?
        && host.store.outgoing(task_id)?.iter().any(|message| {
            message.state == DeliveryState::Queued && message.expected_turn.is_none()
        });
    Ok(waiting.then_some(task))
}

/// Begins a compaction when the next queued turn would start at or above the task's threshold.
///
/// This is the check between turns. The provider's own limit covers a turn that is running.
/// A usage reading is acted on once, and the one a compaction leaves behind never starts another.
pub(crate) fn begin_due_compaction(host: &Shared, task_id: &str) -> Result<bool> {
    let mut h = host.lock().unwrap();
    let Some(mut task) = waiting_turn(&h, task_id)? else {
        return Ok(false);
    };
    let Some((used, window, reported)) = task.context_usage.as_ref().and_then(|usage| {
        Some((
            usage.used,
            usage.window.filter(|window| *window > 0)?,
            usage.reported,
        ))
    }) else {
        return Ok(false);
    };
    if task.provider_thread.is_none() || h.compacted.get(task_id) == Some(&reported) {
        return Ok(false);
    }
    let rule = compact_rule(&h, &task)?;
    if !rule.enabled || used.saturating_mul(100) < window.saturating_mul(u64::from(rule.threshold))
    {
        return Ok(false);
    }
    begin_compaction(&mut h, &mut task)?;
    Ok(true)
}

fn ensure_owner(host: Shared, task_id: String) {
    let mut h = host.lock().unwrap();
    if let Some(sender) = h.sessions.get(&task_id) {
        // A disconnected owner still owns cleanup. Let it persist failure before any restart.
        let _ = sender.send(Control::Wake);
        return;
    }
    let (tx, rx) = mpsc::channel();
    h.sessions.insert(task_id.clone(), tx);
    drop(h);
    std::thread::spawn(move || {
        let provider = host
            .lock()
            .unwrap()
            .store
            .task(&task_id)
            .map(|task| task.provider);
        let result = provider.and_then(|provider| match provider {
            Provider::Codex => codex::run(host.clone(), &task_id, rx),
            Provider::Claude => claude::run(host.clone(), &task_id, rx),
        });
        if let Err(error) = result {
            let mut h = host.lock().unwrap();
            let compaction = h.compactions.remove(&task_id);
            if let Ok(mut task) = h.store.task(&task_id) {
                // A compaction carries no user input, so its failure is never an uncertain delivery.
                if let Some(previous) = compaction {
                    task.state = previous.state;
                    task.updated = previous.updated;
                    task.active_turn = None;
                    let _ = h.store.save_task(&task);
                }
                let unknown = task.active_turn.is_some()
                    || task.subagents.iter().any(|a| a.state.active())
                    || h.store.has_uncertain_submission(&task_id).unwrap_or(true);
                if unknown || task.state.active() || task.state == RunState::Idle {
                    task.state = if unknown {
                        RunState::DeliveryUnknown
                    } else {
                        RunState::Failed
                    };
                    let _ = h.store.settle_pending(&task_id, unknown);
                }
                for agent in &mut task.subagents {
                    if agent.state.active() {
                        agent.state = RunState::DeliveryUnknown;
                        agent.active_turn = None;
                    }
                }
                task.pending_completion = None;
                task.error = Some(format!(
                    "{error:#}. Queued messages are retained; no uncertain input was resent."
                ));
                let _ = h.save_task_with_notification(&task);
            }
        }
        let mut h = host.lock().unwrap();
        h.activities.remove(&task_id);
        h.sessions.remove(&task_id);
        h.requests.remove(&task_id);
        h.compactions.remove(&task_id);
    });
}

/// Claims the next eligible message while excluding concurrent checkout writers.
pub(crate) fn claim_next(host: &Shared, task_id: &str) -> Result<Option<OutgoingMessage>> {
    let mut h = host.lock().unwrap();
    let mut task = h.store.task(task_id)?;
    if h.store.has_uncertain_submission(task_id)? || task.state == RunState::DeliveryUnknown {
        return Ok(None);
    }
    let outgoing = h.store.outgoing(task_id)?;
    for message in outgoing
        .iter()
        .filter(|m| m.state == DeliveryState::Queued && m.expected_turn.is_some())
    {
        if message.expected_turn != task.active_turn {
            h.store.fail_outgoing(task_id, &message.id, false, "The selected turn finished before this steering message could be delivered. Restore it to send again.")?;
        }
    }
    let message = if task.state.active() {
        outgoing.into_iter().find(|m| {
            m.state == DeliveryState::Queued
                && m.expected_turn.is_some()
                && m.expected_turn == task.active_turn
        })
    } else if matches!(task.state, RunState::Idle | RunState::Completed) {
        outgoing
            .into_iter()
            .find(|m| m.state == DeliveryState::Queued && m.expected_turn.is_none())
    } else {
        None
    };
    let Some(message) = message else {
        return Ok(None);
    };
    validate_preferences(&h, &preferences(&task))?;
    validate_attachments(&h, &task, &message.attachments)?;
    if message.expected_turn.is_none() {
        if h.store
            .tasks()?
            .iter()
            .any(|other| other.id != task.id && other.cwd == task.cwd && other.state.active())
        {
            task.state = RunState::Interrupted;
            task.error = Some("Queue paused because another task is using this checkout. Resume after it finishes.".into());
            h.save_task_with_notification(&task)?;
            return Ok(None);
        }
        h.activities.remove(task_id);
        task.state = RunState::Starting;
        task.branch_merged = false;
        task.active_turn = None;
        task.pending_completion = None;
        task.error = None;
        task.updated = now();
        task.last_started = Some(task.updated);
    }
    h.store.claim_outgoing(&task, &message)?;
    Ok(Some(message))
}

/// Rules of a review session. Claude Code also runs it without its file-editing tools and with
/// the commands that publish denied. The approval rule of the task decides everything else.
const REVIEW_RULES: &str = "\
You are reviewing code in Kodem's Review workspace. This session is a local, read-only review.

Rules:
- Do not change anything. Do not edit, create, move or delete files. Do not run a command that writes to the working tree, the index, a branch, a tag or the Git configuration, such as commit, add, checkout, switch, restore, reset, stash, merge, rebase, cherry-pick, apply, fetch or pull.
- Do not publish anything. Do not push. Do not create, edit or submit a pull request, a pull-request comment, a review, an issue or any other remote content, with `gh`, `git`, an API call or any other tool. The review stays on this machine.
- Use read-only commands only, such as `git diff`, `git log`, `git show`, `git blame`, file reads and searches.
- When a fix is needed, describe it and do not apply it. The user sends the findings to another task that applies fixes.";

/// How a review reports, so that the findings can be handed to the task that fixes them.
const REVIEW_REPORT: &str = "\
Check correctness, regressions, error handling, security, performance and consistency with the conventions of this codebase. Do not report a style preference that the codebase does not follow.

Report the findings as a numbered list, most severe first. Give each finding: its severity (critical, major, minor or nit), its location as `path:line`, what is wrong, why it matters, and a concrete fix. End with a short verdict. If there are no findings, say so.";

/// Instructions of a conversation that has no project.
const CHAT_RULES: &str = "\
This is a general conversation in Kodem's Chat workspace. It has no project: the working directory is an empty scratch folder that belongs to this chat alone. Answer questions, explain, compare, brainstorm and write as a helpful assistant. Search the web when an answer depends on current or external information, and name the sources you used. Do not assume that a repository exists, and do not create files unless the user asks for one.";

/// Returns the standing instructions of a task's provider session, which its kind decides.
///
/// They are given when a provider process starts, so they also hold after a resume.
pub(crate) fn instructions(task: &Task) -> Option<String> {
    match task.kind {
        TaskKind::Coding => None,
        TaskKind::Chat => Some(CHAT_RULES.into()),
        TaskKind::Review => {
            let scope = match (&task.review_scope, &task.baseline) {
                (Some(scope), Some(merge_base)) => format!(
                    "The working directory is a temporary detached worktree at the reviewed commit {head}. The changes under review are those of {target} compared with `{base}`: run `git diff {merge_base}...HEAD`. Review only those changes, and read the surrounding code as needed to judge them.",
                    head = scope.head,
                    target = scope.target.label(),
                    base = scope.base,
                ),
                _ => "Review the changes that the user names.".into(),
            };
            Some(format!("{REVIEW_RULES}\n\n{scope}\n\n{REVIEW_REPORT}"))
        }
    }
}

/// Returns the options owned by this task rather than another provider's defaults.
pub(crate) fn preferences(task: &Task) -> AgentPreferences {
    AgentPreferences {
        provider: task.provider,
        account: task.account.clone(),
        provider_access: task.provider_access.clone(),
        model: task.model.clone(),
        effort: task.effort.clone(),
        access: task.access,
    }
}

/// Returns the model that runs a task: its own choice, or the provider's default for new tasks.
pub(crate) fn effective_model(host: &crate::Host, task: &Task) -> Option<String> {
    task.model.clone().or_else(|| {
        host.runtimes
            .get(&task.provider)
            .and_then(|runtime| runtime.default_model.clone())
    })
}

/// Returns the automatic compaction rule for a task's provider and the model that runs it.
pub(crate) fn compact_rule(host: &crate::Host, task: &Task) -> Result<CompactRule> {
    Ok(host
        .store
        .settings()?
        .auto_compact
        .rule(task.provider, effective_model(host, task).as_deref()))
}

/// Removes the "default" and "recommended" markers a provider adds to a model name.
///
/// The result is empty when the name was only such a marker and names no model.
pub(crate) fn model_name(name: &str) -> String {
    let marked = |text: &str| {
        let text = text.to_lowercase();
        text.contains("default") || text.contains("recommended")
    };
    let mut plain = String::new();
    for (index, part) in name.split('(').enumerate() {
        // After the first split every part starts inside a parenthesis.
        let (inside, outside) = match (index, part.split_once(')')) {
            (0, _) => ("", part),
            (_, Some((inside, outside))) => (inside, outside),
            (_, None) => (part, ""),
        };
        if !inside.is_empty() && !marked(inside) {
            plain.push_str(&format!("({inside})"));
        }
        plain.push_str(outside);
    }
    plain
        .split_whitespace()
        .filter(|word| !marked(word))
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches([' ', '-', '·', ':', ','])
        .to_owned()
}

/// Picks the model new tasks start with, which is always one of `models`.
///
/// The first model whose identity and name contain every wanted part wins, then the
/// provider's own default, then the first one listed.
pub(crate) fn default_model(
    models: &[Model],
    wanted: &[&str],
    own: Option<&str>,
) -> Option<String> {
    models
        .iter()
        .find(|model| {
            let text = format!("{} {}", model.id, model.name).to_lowercase();
            wanted.iter().all(|part| text.contains(part))
        })
        .or_else(|| models.iter().find(|model| Some(model.id.as_str()) == own))
        .or(models.first())
        .map(|model| model.id.clone())
}

/// Checks selected options against the provider's discovered catalog before persistence or delivery.
pub(crate) fn validate_preferences(host: &crate::Host, selection: &AgentPreferences) -> Result<()> {
    // An account identity is only meaningful for the provider it was registered with.
    accounts::resolve(host, selection.provider, selection.account.as_deref())?;
    let runtime = host.runtimes.get(&selection.provider);
    // An absent model runs as the provider's default, so reasoning is checked against that one.
    let model_id = selection
        .model
        .as_ref()
        .or_else(|| runtime.and_then(|runtime| runtime.default_model.as_ref()));
    if let Some(model_id) = model_id {
        let model = runtime.and_then(|runtime| runtime.models.iter().find(|model| &model.id == model_id))
            .context("The selected model is unavailable. Refresh this provider and choose an available model.")?;
        if let Some(effort) = &selection.effort {
            ensure!(
                model.efforts.contains(effort),
                "The selected model does not support that reasoning level"
            );
        }
    } else {
        ensure!(
            selection.effort.is_none(),
            "Choose a model before selecting reasoning"
        );
    }
    if let Some(access) = &selection.provider_access {
        ensure!(
            runtime
                .is_some_and(|runtime| runtime.access_modes.iter().any(|mode| &mode.id == access)),
            "The selected provider access mode is unavailable. Refresh its capabilities."
        );
    }
    Ok(())
}

/// Allows restoring previously accepted options while their provider catalog is reconnecting.
pub(crate) fn validate_saved_selection(
    host: &crate::Host,
    selection: &AgentPreferences,
) -> Result<()> {
    if host
        .runtimes
        .get(&selection.provider)
        .is_none_or(|runtime| runtime.models.is_empty())
    {
        let settings = host.store.settings()?;
        if settings.provider_preferences.get(&selection.provider) == Some(selection)
            || &settings.agent_defaults == selection
        {
            return Ok(());
        }
    }
    validate_preferences(host, selection)
}

/// Rejects unsupported context without consuming the user's draft or attachments.
pub(crate) fn validate_attachments(
    host: &crate::Host,
    task: &Task,
    attachments: &[Attachment],
) -> Result<()> {
    if attachments.is_empty() {
        return Ok(());
    }
    let runtime = host
        .runtimes
        .get(&task.provider)
        .context("Refresh the selected provider before attaching files")?;
    let supports_images = task.model.as_ref().map_or(runtime.images, |id| {
        runtime
            .models
            .iter()
            .find(|model| &model.id == id)
            .is_some_and(|model| model.images)
    });
    for attachment in attachments {
        if attachment.kind == AttachmentKind::Image {
            ensure!(
                supports_images,
                "{} with the selected model does not support images. Remove the image or choose another model.",
                task.provider.label()
            );
        } else {
            ensure!(
                runtime.files,
                "{} does not support file context in this session",
                task.provider.label()
            );
        }
    }
    Ok(())
}

/// Refreshes one provider's catalog through the given account without touching other providers.
pub(super) fn discover(
    host: Shared,
    provider: Provider,
    account: Option<String>,
    cwd: Option<PathBuf>,
) -> Result<()> {
    // The registry lookup must release the host before a provider process is started.
    let account = accounts::resolve(&host.lock().unwrap(), provider, account.as_deref());
    let result = account.and_then(|account| match provider {
        Provider::Codex => codex::discover(host.clone(), &account, cwd),
        Provider::Claude => claude::discover(host.clone(), &account, cwd),
    });
    if let Err(error) = &result {
        let mut h = host.lock().unwrap();
        let runtime = h.runtimes.entry(provider).or_default();
        runtime.error = Some(format!("{error:#}"));
    }
    result
}

/// Returns the capabilities known before a provider's CLI has been asked for its catalog.
pub(super) fn initial_runtime(provider: Provider) -> RuntimeInfo {
    match provider {
        Provider::Claude => claude::initial_runtime(),
        Provider::Codex => RuntimeInfo::default(),
    }
}

/// Reconciles only against a provider that can prove the exact submission identity.
pub(super) fn reconcile(host: &Shared, task_id: &str) -> Result<()> {
    let provider = host.lock().unwrap().store.task(task_id)?.provider;
    match provider {
        Provider::Codex => codex::reconcile(host, task_id),
        Provider::Claude => claude::reconcile(host, task_id),
    }
}

/// Resolves the CLI that signs an account in or out.
pub(crate) fn executable(provider: Provider) -> Result<PathBuf> {
    match provider {
        Provider::Codex => kodem_core::providers::codex::executable(),
        Provider::Claude => Ok(claude::executable()),
    }
}

/// Generates a short task title under the task's account for providers without their own.
pub(crate) fn generate_title(account: &Account, input: &str) -> Result<String> {
    match account.provider {
        Provider::Claude => claude::generate_title(account, input),
        Provider::Codex => anyhow::bail!("This provider titles its conversations itself"),
    }
}

/// Reads an account's sign-in state, and its quotas when requested, without a model turn.
pub(crate) fn account_status(
    host: &Shared,
    account: &Account,
    usage: bool,
) -> Result<AccountStatus> {
    match account.provider {
        Provider::Codex => codex::account_status(host, account, usage),
        Provider::Claude => claude::account_status(host, account, usage),
    }
}

/// Routes quota refresh and credit actions to the account that owns them.
pub(super) fn account_usage(
    host: &Shared,
    provider: Provider,
    account: Option<String>,
    key: Option<&str>,
) -> Result<Response> {
    let account = accounts::resolve(&host.lock().unwrap(), provider, account.as_deref())?;
    match provider {
        Provider::Codex => codex::account_usage(host, &account, key),
        Provider::Claude => claude::account_usage(host, &account, key),
    }
}

/// Resolves the task's immutable integration and account before loading commands or resources.
fn scoped_account(
    host: &Shared,
    provider: Provider,
    account: Option<String>,
    task_id: &Option<String>,
) -> Result<Account> {
    let h = host.lock().unwrap();
    if let Some(task_id) = task_id {
        accounts::for_task(&h, &h.store.task(task_id)?)
    } else {
        accounts::resolve(&h, provider, account.as_deref())
    }
}

/// Loads supported composer operations for the selected integration and checkout.
pub(super) fn composer_catalog(
    host: &Shared,
    provider: Provider,
    account: Option<String>,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<ComposerCatalog> {
    let account = scoped_account(host, provider, account, &task_id)?;
    let mut catalog = match account.provider {
        Provider::Codex => codex::composer_catalog(host, &account, task_id, project_id)?,
        Provider::Claude => claude::composer_catalog(host, &account, task_id, project_id)?,
    };
    if !catalog.commands.contains(&ComposerCommand::Provider) {
        catalog.commands.insert(0, ComposerCommand::Provider);
    }
    Ok(catalog)
}

/// Reads configuration and skill provenance from the selected provider.
pub(super) fn resources(
    host: &Shared,
    provider: Provider,
    account: Option<String>,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<Vec<Resource>> {
    let account = scoped_account(host, provider, account, &task_id)?;
    match account.provider {
        Provider::Codex => codex::resources(host, &account, task_id, project_id),
        Provider::Claude => claude::resources(host, &account, task_id, project_id),
    }
}

/// Captures the final checkout before publishing completion or making another queued turn eligible.
pub(crate) fn complete_turn(
    host: &Shared,
    task_id: &str,
    state: RunState,
    error: Option<String>,
) -> Result<()> {
    let task = host.lock().unwrap().store.task(task_id)?;
    // A chat has no checkout to capture.
    let capture = (task.state.active() && task.kind.has_project()).then(|| {
        kodem_core::git::capture(
            &task,
            if task.baseline.is_some() {
                Comparison::Baseline
            } else {
                Comparison::WorkingTree
            },
        )
    });
    // Only a coding task follows the branch of its checkout. A review keeps the name of the
    // branch it examines, and its worktree is detached.
    let branch = (task.state.active() && task.kind == TaskKind::Coding)
        .then(|| kodem_core::git::branch(&task.cwd));
    let mut h = host.lock().unwrap();
    let mut task = h.store.task(task_id)?;
    if let Some(capture) = capture {
        task.review = Some(match capture {
            Ok(mut snapshot) => {
                let summary = snapshot.summary();
                let stored = h.store.save_snapshot(&mut snapshot);
                TaskReview {
                    snapshot_id: stored.as_ref().ok().map(|_| snapshot.id.clone()),
                    added: summary.added,
                    removed: summary.removed,
                    omitted_files: summary.omitted_files,
                    captured: snapshot.captured,
                    error: stored.err().map(|error| error.to_string()),
                }
            }
            Err(error) => TaskReview {
                snapshot_id: None,
                added: 0,
                removed: 0,
                omitted_files: 0,
                captured: now(),
                error: Some(error.to_string()),
            },
        });
    }
    if let Some(branch) = branch {
        task.branch = branch;
    }
    h.store.release_completion_messages(task_id)?;
    task.pending_completion = None;
    task.active_turn = None;
    task.state = state;
    task.error = error;
    task.updated = now();
    h.requests.remove(task_id);
    h.activities.remove(task_id);
    h.save_task_with_notification(&task)
}
