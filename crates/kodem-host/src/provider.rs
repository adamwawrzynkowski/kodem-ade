//! Per-task Codex ownership, durable delivery, and normalized provider events.
use crate::Shared;
use anyhow::{Context, Result, ensure};
use kodem_core::{runtime::Codex, *};
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

fn prompt_for(message: &OutgoingMessage) -> Result<String> {
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
        h.store.accept_outgoing(&task, &message, &prompt)?;
    }
    ensure_owner(host, task_id);
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
        if let Err(error) = run(host.clone(), &task_id, rx) {
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

fn run(host: Shared, task_id: &str, controls: mpsc::Receiver<Control>) -> Result<()> {
    let cwd = host.lock().unwrap().store.task(task_id)?.cwd;
    let mut items: HashMap<String, Message> = HashMap::new();
    let event_host = host.clone();
    let event_task = task_id.to_owned();
    let persistence_error = Arc::new(Mutex::new(None));
    let callback_error = persistence_error.clone();
    let mut event = move |value: Value| {
        if let Err(error) = ingest(&event_host, &event_task, value, &mut items) {
            *callback_error.lock().unwrap() =
                Some(format!("Could not persist provider event: {error}"));
        }
    };
    let mut codex = Codex::start(&cwd, &mut event)?;
    let task = host.lock().unwrap().store.task(task_id)?;
    let mut params = json!({"cwd":cwd,"approvalPolicy":task.access.approval_policy(),"approvalsReviewer":task.access.approvals_reviewer(),"sandbox":task.access.sandbox()});
    if let Some(model) = &task.model {
        params["model"] = json!(model);
    }
    let method = if let Some(thread) = &task.provider_thread {
        params["threadId"] = json!(thread);
        params["excludeTurns"] = json!(true);
        "thread/resume"
    } else {
        params["historyMode"] = json!("paginated");
        "thread/start"
    };
    let result = codex.call(method, params, &mut event)?;
    let thread = result["thread"]["id"]
        .as_str()
        .context("Codex omitted thread ID")?
        .to_owned();
    {
        let h = host.lock().unwrap();
        let mut task = h.store.task(task_id)?;
        task.provider_thread = Some(thread.clone());
        h.save_task_with_notification(&task)?;
    }
    crate::titles::start(host.clone(), task_id)?;
    let mut branch_refresh = Instant::now();
    loop {
        if branch_refresh.elapsed() >= Duration::from_secs(5) {
            branch_refresh = Instant::now();
            if host.lock().unwrap().store.task(task_id)?.state.active() {
                let branch = kodem_core::git::branch(&cwd);
                let h = host.lock().unwrap();
                let mut task = h.store.task(task_id)?;
                if task.branch != branch {
                    task.branch = branch;
                    h.save_task_with_notification(&task)?;
                }
            }
        }
        if let Some(error) = persistence_error.lock().unwrap().take() {
            anyhow::bail!(error);
        }
        while let Ok(control) = controls.try_recv() {
            match control {
                Control::Close => return Ok(()),
                Control::Wake => {}
                Control::Query {
                    method,
                    params,
                    reply,
                } => {
                    let result = codex
                        .call(method, params, &mut event)
                        .map_err(|e| format!("{e:#}"));
                    let _ = reply.send(result);
                }
                Control::Interrupt => {
                    let task = host.lock().unwrap().store.task(task_id)?;
                    if let Some(turn) = task.active_turn {
                        codex.call(
                            "turn/interrupt",
                            json!({"threadId":thread,"turnId":turn}),
                            &mut event,
                        )?;
                    }
                }
                Control::Answer { id, response } => {
                    let request = {
                        let h = host.lock().unwrap();
                        let Some(request) = h
                            .requests
                            .get(task_id)
                            .and_then(|r| r.iter().find(|r| r.id == id))
                            .cloned()
                        else {
                            continue;
                        };
                        request
                    };
                    let answer = (request.method == "item/tool/requestUserInput")
                        .then(|| crate::answer_text(&request, &response));
                    codex.answer(id.clone(), response)?;
                    let mut h = host.lock().unwrap();
                    if let Some(requests) = h.requests.get_mut(task_id) {
                        requests.retain(|r| r.id != id);
                    }
                    let mut task = h.store.task(task_id)?;
                    if let Some(text) = answer {
                        h.store.message(&Message {
                            seq: 0,
                            id: format!(
                                "{}:{}",
                                task.active_turn.as_deref().unwrap_or("unknown"),
                                request.answer_submission_id()
                            ),
                            task_id: task_id.into(),
                            kind: MessageKind::User,
                            text,
                            attachments: Vec::new(),
                            feedback: Vec::new(),
                            complete: true,
                            created: now(),
                        })?;
                    }
                    if task.state == RunState::Waiting
                        && task.active_turn.is_some()
                        && h.requests.get(task_id).is_none_or(|r| r.is_empty())
                    {
                        task.state = RunState::Running;
                        h.save_task_with_notification(&task)?;
                    }
                }
            }
        }
        if let Some(error) = persistence_error.lock().unwrap().take() {
            anyhow::bail!(error);
        }
        dispatch_next(&host, task_id, &thread, &mut codex, &mut event)?;
        codex.poll(Duration::from_millis(40), &mut event)?;
    }
}

fn dispatch_next(
    host: &Shared,
    task_id: &str,
    thread: &str,
    codex: &mut Codex,
    event: &mut impl FnMut(Value),
) -> Result<()> {
    let message = {
        let mut h = host.lock().unwrap();
        let mut task = h.store.task(task_id)?;
        if h.store.has_uncertain_submission(task_id)? || task.state == RunState::DeliveryUnknown {
            return Ok(());
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
            return Ok(());
        };
        if message.expected_turn.is_none() {
            if h.store
                .tasks()?
                .iter()
                .any(|other| other.id != task.id && other.cwd == task.cwd && other.state.active())
            {
                task.state = RunState::Interrupted;
                task.error = Some("Queue paused because another task is using this checkout. Resume after it finishes.".into());
                h.save_task_with_notification(&task)?;
                return Ok(());
            }
            h.activities.remove(task_id);
            task.state = RunState::Starting;
            task.active_turn = None;
            task.error = None;
            task.updated = now();
        }
        h.store.claim_outgoing(&task, &message)?;
        message
    };
    let prompt = prompt_for(&message)?;
    let mut input = Vec::new();
    if !prompt.trim().is_empty() {
        input.push(json!({"type":"text","text":prompt,"text_elements":[]}));
    }
    for attachment in &message.attachments {
        if attachment.kind == AttachmentKind::Image {
            input.push(json!({"type":"localImage","path":attachment.path}));
        }
    }
    let mut params = json!({"threadId":thread,"clientUserMessageId":message.id,"input":input});
    let method = if let Some(expected_turn) = &message.expected_turn {
        params["expectedTurnId"] = json!(expected_turn);
        "turn/steer"
    } else {
        let task = host.lock().unwrap().store.task(task_id)?;
        params["approvalPolicy"] = json!(task.access.approval_policy());
        params["approvalsReviewer"] = json!(task.access.approvals_reviewer());
        params["sandboxPolicy"] = task.access.turn_policy(&task.cwd);
        if let Some(model) = task.model {
            params["model"] = json!(model);
        }
        if let Some(effort) = task.effort {
            params["effort"] = json!(effort);
        }
        "turn/start"
    };
    match codex.call(method, params, event) {
        Ok(reply) => {
            let turn = if message.expected_turn.is_some() {
                reply["turnId"].as_str()
            } else {
                reply["turn"]["id"].as_str()
            }
            .context("Codex omitted delivery acknowledgement")?;
            let h = host.lock().unwrap();
            h.store.confirm_submission(&message.id, turn)?;
            let mut task = h.store.task(task_id)?;
            // Notifications may have completed the turn while call waited for its response.
            if message.expected_turn.is_none() && task.state == RunState::Starting {
                task.state = RunState::Running;
                task.active_turn = Some(turn.into());
                h.save_task_with_notification(&task)?;
            }
        }
        Err(error) => {
            let rejected = error.is::<kodem_core::runtime::Rejected>();
            let h = host.lock().unwrap();
            h.store
                .fail_outgoing(task_id, &message.id, !rejected, &format!("{error:#}"))?;
            if !rejected || message.expected_turn.is_none() {
                let mut task = h.store.task(task_id)?;
                task.state = if rejected {
                    RunState::Failed
                } else {
                    RunState::DeliveryUnknown
                };
                task.error = Some(format!("{error:#}"));
                h.save_task_with_notification(&task)?;
            }
            if !rejected {
                return Err(error);
            }
        }
    }
    Ok(())
}
fn ingest(
    host: &Shared,
    task_id: &str,
    value: Value,
    items: &mut HashMap<String, Message>,
) -> Result<()> {
    let method = value["method"].as_str().unwrap_or_default();
    let p = &value["params"];
    // Capture before publishing completion so another queued turn cannot change this checkout first.
    let observed_branch = if matches!(method, "turn/started" | "turn/completed") {
        let task = host.lock().unwrap().store.task(task_id)?;
        task.state
            .active()
            .then(|| kodem_core::git::branch(&task.cwd))
    } else {
        None
    };
    let completed_review = if method == "turn/completed" {
        let task = host.lock().unwrap().store.task(task_id)?;
        task.state.active().then(|| {
            kodem_core::git::capture(
                &task,
                if task.baseline.is_some() {
                    Comparison::Baseline
                } else {
                    Comparison::WorkingTree
                },
            )
        })
    } else {
        None
    };
    let mut h = host.lock().unwrap();
    let mut task = h.store.task(task_id)?;
    if value.get("id").is_some() {
        if h.requests
            .get(task_id)
            .is_some_and(|requests| requests.iter().any(|request| request.id == value["id"]))
        {
            return Ok(());
        }
        h.requests
            .entry(task_id.into())
            .or_default()
            .push(InputRequest {
                id: value["id"].clone(),
                task_id: task_id.into(),
                method: method.into(),
                params: p.clone(),
            });
        task.state = RunState::Waiting;
        h.store.save_task(&task)?;
        h.notify_task_state(&task);
        return Ok(());
    }
    if let Some(branch) = observed_branch {
        task.branch = branch;
    }
    update_activities(h.activities.entry(task_id.into()).or_default(), method, p);
    match method {
        "thread/tokenUsage/updated" => {
            if task
                .provider_thread
                .as_deref()
                .is_some_and(|id| p["threadId"].as_str() != Some(id))
            {
                return Ok(());
            }
            if let Some(used) = p["tokenUsage"]["last"]["totalTokens"].as_u64() {
                task.context_usage = Some(ContextUsage {
                    used,
                    window: p["tokenUsage"]["modelContextWindow"]
                        .as_u64()
                        .filter(|v| *v > 0),
                    reported: now(),
                });
                h.save_task_with_notification(&task)?;
            }
        }
        "account/rateLimits/updated" => {
            merge_usage(&mut h.runtime.usage, &p["rateLimits"]);
        }
        "turn/started" => {
            task.active_turn = p["turn"]["id"].as_str().map(str::to_owned);
            task.state = RunState::Running;
            task.updated = now();
            h.save_task_with_notification(&task)?;
        }
        "turn/completed" => {
            if let Some(review) = completed_review {
                task.review = Some(match review {
                    Ok(mut snapshot) => {
                        let stored = h.store.save_snapshot(&mut snapshot);
                        TaskReview {
                            snapshot_id: stored.as_ref().ok().map(|_| snapshot.id.clone()),
                            added: snapshot
                                .files
                                .iter()
                                .flat_map(|f| &f.lines)
                                .filter(|l| l.kind == LineKind::Added)
                                .count(),
                            removed: snapshot
                                .files
                                .iter()
                                .flat_map(|f| &f.lines)
                                .filter(|l| l.kind == LineKind::Removed)
                                .count(),
                            omitted_files: snapshot
                                .files
                                .iter()
                                .filter(|f| f.notice.is_some())
                                .count(),
                            captured: snapshot.captured,
                            error: stored.err().map(|e| e.to_string()),
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
            task.active_turn = None;
            task.state = match p["turn"]["status"].as_str() {
                Some("completed") => RunState::Completed,
                Some("interrupted") => RunState::Interrupted,
                _ => RunState::Failed,
            };
            task.error = p["turn"]["error"]["message"].as_str().map(str::to_owned);
            task.updated = now();
            h.requests.remove(task_id);
            h.save_task_with_notification(&task)?;
            items.clear();
        }
        "serverRequest/resolved" => {
            if let Some(requests) = h.requests.get_mut(task_id) {
                requests.retain(|r| r.id != p["requestId"]);
            }
            if task.state == RunState::Waiting
                && task.active_turn.is_some()
                && h.requests.get(task_id).is_none_or(|r| r.is_empty())
            {
                task.state = RunState::Running;
                h.save_task_with_notification(&task)?;
            }
        }
        "item/agentMessage/delta" | "item/commandExecution/outputDelta" => {
            let id = p["itemId"]
                .as_str()
                .context("Missing item identity")?
                .to_owned();
            let key = format!("{}:{id}", p["turnId"].as_str().unwrap_or("unknown"));
            let item = items.entry(key.clone()).or_insert(Message {
                seq: 0,
                id: key,
                task_id: task_id.into(),
                attachments: Vec::new(),
                feedback: Vec::new(),
                kind: if method.contains("agentMessage") {
                    MessageKind::Assistant
                } else {
                    MessageKind::Tool
                },
                text: String::new(),
                complete: false,
                created: now(),
            });
            let delta = p["delta"].as_str().unwrap_or_default();
            append_bounded(&mut item.text, delta);
            h.store.message(item)?;
        }
        "item/started" | "item/completed" => {
            let item = &p["item"];
            let Some(id) = item["id"].as_str() else {
                return Ok(());
            };
            let kind = item["type"].as_str().unwrap_or_default();
            if kind == "userMessage" || kind == "reasoning" {
                return Ok(());
            }
            let key = format!("{}:{id}", p["turnId"].as_str().unwrap_or("unknown"));
            let (message_kind, text) = match kind {
                "agentMessage" => (
                    MessageKind::Assistant,
                    item["text"].as_str().unwrap_or_default().to_owned(),
                ),
                "plan" => (
                    MessageKind::Notice,
                    item["text"].as_str().unwrap_or_default().to_owned(),
                ),
                "commandExecution" => (
                    MessageKind::Tool,
                    format!(
                        "$ {}\n{}{}",
                        item["command"].as_str().unwrap_or("Command"),
                        item["aggregatedOutput"].as_str().unwrap_or_default(),
                        item["exitCode"]
                            .as_i64()
                            .map(|c| format!("\nExit {c}"))
                            .unwrap_or_default()
                    ),
                ),
                "fileChange" => (
                    MessageKind::Tool,
                    format!(
                        "File changes · {}\n{}",
                        item["status"].as_str().unwrap_or("in progress"),
                        item["changes"]
                            .as_array()
                            .map(|changes| changes
                                .iter()
                                .filter_map(|c| c["path"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n"))
                            .unwrap_or_default()
                    ),
                ),
                _ => (
                    MessageKind::Tool,
                    format!(
                        "{kind} · {}",
                        item["status"]
                            .as_str()
                            .unwrap_or(if method.ends_with("completed") {
                                "completed"
                            } else {
                                "in progress"
                            })
                    ),
                ),
            };
            let mut message = Message {
                seq: 0,
                id: key.clone(),
                task_id: task_id.into(),
                kind: message_kind,
                attachments: Vec::new(),
                feedback: Vec::new(),
                text: String::new(),
                complete: method.ends_with("completed"),
                created: now(),
            };
            append_bounded(&mut message.text, &text);
            h.store.message(&message)?;
            if message.complete
                && let Some(request) = InputRequest::from_async_message(
                    task_id,
                    p["turnId"].as_str().unwrap_or("unknown"),
                    item,
                )?
                && !task
                    .questions
                    .iter()
                    .any(|pending| pending.id == request.id)
                && !h
                    .store
                    .has_submission(task_id, &request.answer_submission_id())?
            {
                task.questions.push(request);
                h.store.save_task(&task)?;
                h.notify_input_needed(&task);
            }
            if message.complete {
                items.remove(&key);
            } else {
                items.insert(key, message);
            }
        }
        "error" if p["willRetry"].as_bool() != Some(true) => {
            let text = p["error"]["message"]
                .as_str()
                .unwrap_or("Codex reported an error");
            h.store.message(&Message {
                seq: 0,
                id: new_id(),
                task_id: task_id.into(),
                kind: MessageKind::Error,
                attachments: Vec::new(),
                feedback: Vec::new(),
                text: text.into(),
                complete: true,
                created: now(),
            })?;
        }
        _ => {}
    }
    Ok(())
}
// Track item identities, not inferred prose. Reasoning content is never copied into activity labels.
fn update_activities(activities: &mut Vec<Activity>, method: &str, params: &Value) {
    if matches!(method, "turn/started" | "turn/completed") {
        activities.clear();
        return;
    }
    if method == "error" {
        let previous = activities
            .iter()
            .find(|a| a.kind == ActivityKind::Reconnecting)
            .map(|a| a.started);
        activities.retain(|a| a.kind != ActivityKind::Reconnecting);
        if params["willRetry"].as_bool() == Some(true) {
            activities.push(Activity {
                id: format!("retry:{}", params["turnId"].as_str().unwrap_or("unknown")),
                kind: ActivityKind::Reconnecting,
                detail: params["error"]["message"]
                    .as_str()
                    .unwrap_or("Retrying connection")
                    .chars()
                    .take(240)
                    .collect(),
                started: previous.unwrap_or_else(now),
            });
        }
        return;
    }
    // A resumed stream clears the retry indicator even if the existing item only emits deltas.
    if method.starts_with("item/") {
        activities.retain(|a| a.kind != ActivityKind::Reconnecting);
    }
    if !matches!(method, "item/started" | "item/completed") {
        return;
    }
    let item = &params["item"];
    let Some(id) = item["id"].as_str() else {
        return;
    };
    let key = format!("{}:{id}", params["turnId"].as_str().unwrap_or("unknown"));
    if method == "item/completed" {
        activities.retain(|a| a.id != key);
        return;
    }
    if activities.iter().any(|a| a.id == key) {
        return;
    }
    let (kind, detail) = match item["type"].as_str().unwrap_or_default() {
        "userMessage" | "functionCallOutput" | "subAgentActivity" | "exitedReviewMode" => return,
        "reasoning" => (ActivityKind::Reasoning, String::new()),
        "agentMessage" => (ActivityKind::Responding, String::new()),
        "commandExecution" => (
            command_activity(&item["commandActions"]),
            item["command"].as_str().unwrap_or_default().into(),
        ),
        "fileChange" => (
            ActivityKind::EditingFiles,
            item["changes"]
                .as_array()
                .map(|files| format!("{} files", files.len()))
                .unwrap_or_default(),
        ),
        "webSearch" => (
            ActivityKind::Searching,
            item["query"].as_str().unwrap_or_default().into(),
        ),
        "collabAgentToolCall" => (
            ActivityKind::Delegating,
            item["tool"].as_str().unwrap_or_default().into(),
        ),
        "enteredReviewMode" => (ActivityKind::Reviewing, String::new()),
        "imageView" => (
            ActivityKind::ViewingImage,
            item["path"].as_str().unwrap_or_default().into(),
        ),
        "imageGeneration" => (ActivityKind::GeneratingImage, String::new()),
        "sleep" => (
            ActivityKind::WaitingTimer,
            item["durationMs"]
                .as_u64()
                .map(|ms| format!("{} seconds", ms / 1000))
                .unwrap_or_default(),
        ),
        "dynamicToolCall" => (
            ActivityKind::Tool,
            item["tool"].as_str().unwrap_or_default().into(),
        ),
        "plan" => (ActivityKind::Planning, String::new()),
        "contextCompaction" => (ActivityKind::Compacting, String::new()),
        "mcpToolCall" => (
            ActivityKind::Tool,
            format!(
                "{} / {}",
                item["server"].as_str().unwrap_or("MCP"),
                item["tool"].as_str().unwrap_or("tool")
            ),
        ),
        _ => (ActivityKind::Tool, String::new()),
    };
    let detail = detail
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(100)
        .collect();
    activities.push(Activity {
        id: key,
        kind,
        detail,
        started: now(),
    });
}

// Use the provider's parsed command actions rather than guessing from command text.
fn command_activity(actions: &Value) -> ActivityKind {
    let Some(actions) = actions.as_array().filter(|a| !a.is_empty()) else {
        return ActivityKind::Command;
    };
    if actions
        .iter()
        .all(|a| matches!(a["type"].as_str(), Some("read" | "listFiles")))
    {
        ActivityKind::ReadingFiles
    } else if actions
        .iter()
        .all(|a| matches!(a["type"].as_str(), Some("read" | "listFiles" | "search")))
    {
        ActivityKind::SearchingFiles
    } else {
        ActivityKind::Command
    }
}

fn append_bounded(target: &mut String, text: &str) {
    const LIMIT: usize = 1024 * 1024;
    if target.len() >= LIMIT {
        return;
    }
    let mut take = text.len().min(LIMIT - target.len());
    while !text.is_char_boundary(take) {
        take -= 1
    }
    target.push_str(&text[..take]);
    if take < text.len() {
        target.push_str("\n[Output truncated at 1 MiB. Full output remains with the provider.]");
    }
}

enum Catalog {
    Session(mpsc::Sender<Control>),
    Standalone(Codex),
}
impl Catalog {
    fn open(host: &Shared, cwd: &std::path::Path) -> Result<Self> {
        let h = host.lock().unwrap();
        let session = h.store.tasks()?.iter().find_map(|task| {
            (task.cwd == cwd)
                .then(|| h.sessions.get(&task.id).cloned())
                .flatten()
        });
        drop(h);
        match session {
            Some(session) => Ok(Self::Session(session)),
            None => Ok(Self::Standalone(Codex::start(cwd, |_| {})?)),
        }
    }
    fn call(&mut self, method: &'static str, params: Value) -> Result<Value> {
        match self {
            Self::Standalone(codex) => codex.call(method, params, &mut |_| {}),
            Self::Session(session) => {
                let (reply, result) = mpsc::sync_channel(1);
                session.send(Control::Query {
                    method,
                    params,
                    reply,
                })?;
                result
                    .recv_timeout(Duration::from_secs(95))?
                    .map_err(anyhow::Error::msg)
            }
        }
    }
}

/// Reads account and model capabilities from the pinned provider runtime.
pub(super) fn discover(host: Shared, cwd: Option<PathBuf>) -> Result<()> {
    let cwd = cwd.unwrap_or(std::env::current_dir()?);
    let mut codex = Catalog::open(&host, &cwd)?;
    let account = codex.call("account/read", json!({"refreshToken":false}))?;
    let mut models = Vec::new();
    let mut cursor = Value::Null;
    loop {
        let result = codex.call("model/list", json!({"includeHidden":false,"cursor":cursor}))?;
        if let Some(data) = result["data"].as_array() {
            for model in data {
                if let Some(id) = model["model"].as_str() {
                    models.push(Model {
                        id: id.into(),
                        name: model["displayName"].as_str().unwrap_or(id).into(),
                        efforts: model["supportedReasoningEfforts"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|e| {
                                        e["reasoningEffort"].as_str().map(str::to_owned)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        default_effort: model["defaultReasoningEffort"]
                            .as_str()
                            .unwrap_or_default()
                            .into(),
                    });
                }
            }
        }
        cursor = result["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    let mut h = host.lock().unwrap();
    h.runtime = RuntimeInfo {
        version: Some(kodem_core::runtime::version()?),
        authenticated: Some(
            !account["account"].is_null() || account["requiresOpenaiAuth"] == false,
        ),
        models,
        usage: h.runtime.usage.clone(),
        error: None,
    };
    Ok(())
}
/// Reports instruction, skill, and MCP provenance through the existing provider owner when present.
pub(super) fn resources(host: &Shared, task_id: Option<String>) -> Result<Vec<Resource>> {
    let cwd = if let Some(id) = task_id {
        host.lock().unwrap().store.task(&id)?.cwd
    } else {
        std::env::current_dir()?
    };
    let mut result = Vec::new();
    let mut parent = Some(cwd.as_path());
    while let Some(path) = parent {
        let file = path.join("AGENTS.md");
        if file.is_file() {
            result.push(Resource {
                name: "AGENTS.md".into(),
                source: file.display().to_string(),
                scope: "Project instructions".into(),
                status: "Discovered file".into(),
                detail: std::fs::read_to_string(&file)
                    .unwrap_or_default()
                    .chars()
                    .take(2000)
                    .collect(),
            });
        }
        parent = path.parent();
    }
    let mut codex = Catalog::open(host, &cwd)?;
    let skills = codex.call("skills/list", json!({"cwds":[cwd],"forceReload":false}))?;
    if let Some(data) = skills["data"].as_array() {
        for group in data {
            if let Some(skills) = group["skills"].as_array() {
                for skill in skills {
                    result.push(Resource {
                        name: skill["name"].as_str().unwrap_or("Skill").into(),
                        source: skill["path"].as_str().unwrap_or_default().into(),
                        scope: skill["scope"].as_str().unwrap_or("Provider").into(),
                        status: if skill["enabled"] == true {
                            "Provider reports enabled"
                        } else {
                            "Provider reports disabled"
                        }
                        .into(),
                        detail: skill["description"].as_str().unwrap_or_default().into(),
                    });
                }
            }
        }
    }
    let mut cursor = Value::Null;
    loop {
        let status = codex.call(
            "mcpServerStatus/list",
            json!({"cursor":cursor,"detail":"toolsAndAuthOnly"}),
        )?;
        if let Some(data) = status["data"].as_array() {
            for server in data {
                result.push(Resource {
                    name: server["name"].as_str().unwrap_or("MCP server").into(),
                    source: "Codex app-server inventory".into(),
                    scope: "Provider-owned MCP".into(),
                    status: format!(
                        "Auth: {}",
                        server["authStatus"].as_str().unwrap_or("unknown")
                    ),
                    detail: format!(
                        "{} reported tools. This inventory is not a tool execution check.",
                        server["tools"].as_object().map_or(0, |t| t.len())
                    ),
                });
            }
        }
        cursor = status["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    Ok(result)
}

/// Resolves uncertain delivery only from matching provider message or turn identity.
pub(super) fn reconcile(host: &Shared, task_id: &str) -> Result<()> {
    let (task, submission) = {
        let h = host.lock().unwrap();
        ensure!(
            !h.sessions.contains_key(task_id),
            "The task still has a live transport owner"
        );
        (
            h.store.task(task_id)?,
            h.store
                .latest_submission(task_id)?
                .context("No recorded outgoing submission to reconcile")?,
        )
    };
    let thread=task.provider_thread.as_ref().context("The provider never returned a thread identity. No prompt can be reconciled automatically; retain this task and create a new one.")?;
    let mut codex = Codex::start(&task.cwd, |_| {})?;
    let resumed=codex.call("thread/resume",json!({"threadId":thread,"excludeTurns":true,"initialTurnsPage":{"limit":20,"sortDirection":"desc","itemsView":"notLoaded"}}),&mut |_|{})?;
    let mut page = resumed["initialTurnsPage"].clone();
    let mut identified: Option<(Value, Vec<Value>)> = None;
    for _ in 0..50 {
        for turn in page["data"].as_array().into_iter().flatten() {
            let Some(turn_id) = turn["id"].as_str() else {
                continue;
            };
            if submission.receipt.as_ref().is_some_and(|id| id != turn_id) {
                continue;
            }
            let mut matching = submission.receipt.as_deref() == Some(turn_id);
            let mut items = Vec::new();
            let mut cursor = Value::Null;
            for _ in 0..100 {
                let result=codex.call("thread/items/list",json!({"threadId":thread,"turnId":turn_id,"limit":10,"sortDirection":"asc","cursor":cursor}),&mut |_|{})?;
                for wrapped in result["data"].as_array().into_iter().flatten() {
                    let item = &wrapped["item"];
                    matching |= item["type"] == "userMessage"
                        && item["clientId"].as_str() == Some(&submission.id);
                    items.push(item.clone());
                }
                cursor = result["nextCursor"].clone();
                if cursor.is_null() {
                    break;
                }
            }
            ensure!(
                cursor.is_null(),
                "History page limit reached; delivery remains unknown"
            );
            if matching {
                identified = Some((turn.clone(), items));
                break;
            }
        }
        if identified.is_some() {
            break;
        }
        let cursor = &page["nextCursor"];
        if cursor.is_null() {
            break;
        }
        page=codex.call("thread/turns/list",json!({"threadId":thread,"cursor":cursor,"limit":20,"sortDirection":"desc","itemsView":"notLoaded"}),&mut |_|{})?;
    }
    let (turn,items)=identified.context("The provider history does not identify this exact submission. Delivery remains unknown; no text was resent.")?;
    let latest = codex.call(
        "thread/read",
        json!({"threadId":thread,"includeTurns":false}),
        &mut |_| {},
    )?;
    ensure!(
        latest["thread"]["status"]["type"] != "active" && turn["status"] != "inProgress",
        "The provider still reports active work. Reconcile again after it settles."
    );
    ensure!(
        matches!(
            turn["status"].as_str(),
            Some("completed" | "interrupted" | "failed")
        ),
        "Provider outcome is not terminal"
    );
    let mut normalized = HashMap::new();
    for item in items {
        ingest(
            host,
            task_id,
            json!({"method":"item/completed","params":{"threadId":thread,"turnId":turn["id"],"item":item}}),
            &mut normalized,
        )?;
    }
    ingest(
        host,
        task_id,
        json!({"method":"turn/completed","params":{"threadId":thread,"turn":turn}}),
        &mut normalized,
    )?;
    let h = host.lock().unwrap();
    h.store.confirm_submission(
        &submission.id,
        turn["id"].as_str().context("Missing turn identity")?,
    )?;
    let mut current = h.store.task(task_id)?;
    if submission.outgoing.is_none() && current.draft == submission.prompt {
        current.draft.clear();
    }
    current
        .feedback
        .retain(|f| !submission.feedback.contains(&f.id));
    current.error = None;
    if h.store.has_uncertain_submission(task_id)? {
        current.state = RunState::DeliveryUnknown;
        current.error = Some(
            "Other outgoing input still has uncertain delivery. Reconcile again before resuming."
                .into(),
        );
    }
    h.save_task_with_notification(&current)?;
    Ok(())
}

/// Refreshes account limits without starting a model turn or changing authentication.
pub(super) fn refresh_usage(host: &Shared) -> Result<()> {
    let mut catalog = Catalog::open(host, &std::env::current_dir()?)?;
    let response = catalog.call("account/rateLimits/read", json!({}))?;
    let value = response["rateLimitsByLimitId"]
        .get("codex")
        .unwrap_or(&response["rateLimits"]);
    let mut usage: UsageLimits = serde_json::from_value(value.clone())?;
    usage.reported = now();
    host.lock().unwrap().runtime.usage = Some(usage);
    Ok(())
}
// Rolling notifications are sparse: omitted windows and nullable account metadata retain their last observation.
fn merge_usage(current: &mut Option<UsageLimits>, value: &Value) {
    let Ok(mut incoming) = serde_json::from_value::<UsageLimits>(value.clone()) else {
        return;
    };
    incoming.reported = now();
    if let Some(old) = current {
        if old.limit_id.is_some()
            && incoming.limit_id.is_some()
            && old.limit_id != incoming.limit_id
        {
            return;
        }
        if incoming.primary.is_none() {
            incoming.primary = old.primary.clone();
        }
        if incoming.secondary.is_none() {
            incoming.secondary = old.secondary.clone();
        }
        if incoming.plan_type.is_none() {
            incoming.plan_type = old.plan_type.clone();
        }
        if incoming.limit_id.is_none() {
            incoming.limit_id = old.limit_id.clone();
        }
    }
    *current = Some(incoming);
}
