//! Provider routing and durable, per-task conversation ownership.
use crate::Shared;
#[path = "providers/codex.rs"]
mod codex;
#[path = "providers/devin.rs"]
mod devin;
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
        let expected_turn = if task.state.active() && mode == SendMode::Steer {
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
            Provider::Devin => devin::run(host.clone(), &task_id, rx),
        });
        if let Err(error) = result {
            let h = host.lock().unwrap();
            if let Ok(mut task) = h.store.task(&task_id) {
                let unknown = task.active_turn.is_some()
                    || h.store.has_uncertain_submission(&task_id).unwrap_or(true);
                if unknown || task.state.active() || task.state == RunState::Idle {
                    task.state = if unknown {
                        RunState::DeliveryUnknown
                    } else {
                        RunState::Failed
                    };
                    let _ = h.store.settle_pending(&task_id, unknown);
                }
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
        task.error = None;
        task.updated = now();
        task.last_started = Some(task.updated);
    }
    h.store.claim_outgoing(&task, &message)?;
    Ok(Some(message))
}

/// Returns the options owned by this task rather than another provider's defaults.
pub(crate) fn preferences(task: &Task) -> AgentPreferences {
    AgentPreferences {
        provider: task.provider,
        provider_access: task.provider_access.clone(),
        model: task.model.clone(),
        effort: task.effort.clone(),
        access: task.access,
    }
}

/// Checks selected options against the provider's discovered catalog before persistence or delivery.
pub(crate) fn validate_preferences(host: &crate::Host, selection: &AgentPreferences) -> Result<()> {
    let runtime = host.runtimes.get(&selection.provider);
    if let Some(model_id) = &selection.model {
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
            selection.provider == Provider::Devin
                && runtime.is_some_and(|runtime| runtime
                    .access_modes
                    .iter()
                    .any(|mode| &mode.id == access)),
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

/// Refreshes one provider without overwriting another integration's account or catalog.
pub(super) fn discover(host: Shared, provider: Provider, cwd: Option<PathBuf>) -> Result<()> {
    let result = match provider {
        Provider::Codex => codex::discover(host.clone(), cwd),
        Provider::Devin => devin::discover(host.clone(), cwd),
    };
    if let Err(error) = &result {
        let mut h = host.lock().unwrap();
        let runtime = h.runtimes.entry(provider).or_default();
        runtime.error = Some(format!("{error:#}"));
    }
    result
}

/// Reconciles only against a provider that can prove the exact submission identity.
pub(super) fn reconcile(host: &Shared, task_id: &str) -> Result<()> {
    let provider = host.lock().unwrap().store.task(task_id)?.provider;
    match provider {
        Provider::Codex => codex::reconcile(host, task_id),
        Provider::Devin => anyhow::bail!(
            "Devin ACP cannot verify the exact receipt of an uncertain prompt. Inspect the session with devin --resume; this task will not resend it automatically."
        ),
    }
}

/// Refreshes supported account quotas without starting a model turn.
pub(super) fn refresh_usage(host: &Shared) -> Result<()> {
    let supported = host
        .lock()
        .unwrap()
        .runtimes
        .get(&Provider::Codex)
        .is_some_and(|runtime| runtime.account_usage);
    if supported {
        codex::refresh_usage(host)
    } else {
        Ok(())
    }
}

/// Routes quota refresh and credit actions to the account that owns them.
pub(super) fn account_usage(
    host: &Shared,
    provider: Provider,
    key: Option<&str>,
) -> Result<Response> {
    match provider {
        Provider::Codex => codex::account_usage(host, key),
        Provider::Devin => anyhow::bail!(
            "Devin ACP does not expose account quotas or banked resets. Session context is shown when the provider reports it."
        ),
    }
}

/// Resolves the task's immutable integration before loading commands or resources.
fn scoped_provider(
    host: &Shared,
    provider: Provider,
    task_id: &Option<String>,
) -> Result<Provider> {
    if let Some(task_id) = task_id {
        Ok(host.lock().unwrap().store.task(task_id)?.provider)
    } else {
        Ok(provider)
    }
}

/// Loads supported composer operations for the selected integration and checkout.
pub(super) fn composer_catalog(
    host: &Shared,
    provider: Provider,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<ComposerCatalog> {
    let provider = scoped_provider(host, provider, &task_id)?;
    let mut catalog = match provider {
        Provider::Codex => codex::composer_catalog(host, task_id, project_id)?,
        Provider::Devin => devin::composer_catalog(host, task_id, project_id)?,
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
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<Vec<Resource>> {
    match scoped_provider(host, provider, &task_id)? {
        Provider::Codex => codex::resources(host, task_id, project_id),
        Provider::Devin => devin::resources(host, task_id, project_id),
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
    let capture = task.state.active().then(|| {
        kodem_core::git::capture(
            &task,
            if task.baseline.is_some() {
                Comparison::Baseline
            } else {
                Comparison::WorkingTree
            },
        )
    });
    let branch = task
        .state
        .active()
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
    task.active_turn = None;
    task.state = state;
    task.error = error;
    task.updated = now();
    h.requests.remove(task_id);
    h.activities.remove(task_id);
    h.save_task_with_notification(&task)
}
