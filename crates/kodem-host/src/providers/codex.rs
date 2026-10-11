//! Codex integration: app-server sessions, discovery and protocol events.
use super::*;
use kodem_core::providers::codex::Codex;
#[path = "codex_subagents.rs"]
mod subagents;

/// A limit no context reaches, for a task whose rule turns automatic compaction off.
const NO_COMPACT_LIMIT: u64 = 1_000_000_000_000;

// Codex compacts at a token count, so the task's percentage needs the model's context window.
// The window is known once any task of that model has reported its usage.
fn compact_limit(host: &crate::Host, task: &Task) -> Result<Option<u64>> {
    let rule = compact_rule(host, task)?;
    if !rule.enabled {
        return Ok(Some(NO_COMPACT_LIMIT));
    }
    let window = match task.context_usage.as_ref().and_then(|usage| usage.window) {
        Some(window) => Some(window),
        None => {
            let model = effective_model(host, task);
            host.store
                .tasks()?
                .iter()
                .filter(|other| {
                    other.provider == Provider::Codex && effective_model(host, other) == model
                })
                .find_map(|other| other.context_usage.as_ref()?.window)
        }
    };
    Ok(window.map(|window| window.saturating_mul(u64::from(rule.threshold)) / 100))
}

// Starts an app-server process and opens the task's thread in it. Codex reads thread
// configuration, which holds the compaction limit, only here: a changed limit needs a new process.
fn open(
    host: &Shared,
    task_id: &str,
    cwd: &std::path::Path,
    account: &Account,
    event: &mut impl FnMut(Value),
) -> Result<(Codex, String, Option<u64>)> {
    let mut codex = Codex::start(cwd, account, &mut *event)?;
    let known = host
        .lock()
        .unwrap()
        .runtimes
        .get(&Provider::Codex)
        .is_some_and(|runtime| !runtime.models.is_empty());
    // A task without a model runs the catalog's default, so the catalog must be read first.
    if !known {
        let info = read_runtime(|method, params| codex.call(method, params, &mut *event))?;
        host.lock().unwrap().runtimes.insert(Provider::Codex, info);
    }
    let (task, model, limit) = {
        let h = host.lock().unwrap();
        let task = h.store.task(task_id)?;
        let model = effective_model(&h, &task);
        let limit = compact_limit(&h, &task)?;
        (task, model, limit)
    };
    let mut params = json!({"cwd":cwd,"approvalPolicy":task.access.approval_policy(),"approvalsReviewer":task.access.approvals_reviewer(),"sandbox":task.access.sandbox(),"config":{"features.multi_agent":true}});
    if let Some(limit) = limit {
        params["config"]["model_auto_compact_token_limit"] = json!(limit);
    }
    if let Some(model) = model {
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
    let result = codex.call(method, params, event)?;
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
    Ok((codex, thread, limit))
}

// Asks Codex to summarize the thread for the compaction recorded on the host. Codex runs it as
// a turn of its own, which `ingest` ends without completing a user turn.
fn compact(
    host: &Shared,
    task_id: &str,
    thread: &str,
    codex: &mut Codex,
    event: &mut impl FnMut(Value),
) -> Result<()> {
    if !host.lock().unwrap().compactions.contains_key(task_id) {
        return Ok(());
    }
    let Err(error) = codex.call("thread/compact/start", json!({"threadId":thread}), event) else {
        return Ok(());
    };
    let rejected = error.is::<kodem_core::providers::codex::Rejected>();
    end_compaction(host, task_id, Some(format!("{error:#}")))?;
    if rejected { Ok(()) } else { Err(error) }
}

pub(crate) fn run(host: Shared, task_id: &str, controls: mpsc::Receiver<Control>) -> Result<()> {
    let (cwd, account) = {
        let h = host.lock().unwrap();
        let task = h.store.task(task_id)?;
        let account = accounts::for_task(&h, &task)?;
        (task.cwd, account)
    };
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
    let (mut codex, thread, mut limit) = open(&host, task_id, &cwd, &account, &mut event)?;
    crate::titles::start(host.clone(), task_id)?;
    let mut branch_refresh = Instant::now();
    let mut children = subagents::Subscriptions::default();
    let mut stopping = false;
    let mut interrupted = std::collections::HashSet::new();
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
                Control::Compact => compact(&host, task_id, &thread, &mut codex, &mut event)?,
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
                    stopping = true;
                    interrupted.clear();
                    let h = host.lock().unwrap();
                    let mut task = h.store.task(task_id)?;
                    if let Some(outcome) = &mut task.pending_completion {
                        outcome.state = RunState::Interrupted;
                        h.store.save_task(&task)?;
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
                            task_id: task
                                .subagents
                                .iter()
                                .find(|agent| {
                                    request.params["threadId"].as_str() == Some(&agent.thread_id)
                                })
                                .map_or_else(
                                    || task_id.to_owned(),
                                    |agent| agent.transcript_id(task_id),
                                ),
                            kind: MessageKind::User,
                            text,
                            attachments: Vec::new(),
                            feedback: Vec::new(),
                            file_edits: Vec::new(),
                            tool: None,
                            complete: true,
                            created: now(),
                        })?;
                    }
                    if let Some(agent) = task
                        .subagents
                        .iter_mut()
                        .find(|agent| request.params["threadId"].as_str() == Some(&agent.thread_id))
                    {
                        if agent.state == RunState::Waiting {
                            agent.state = RunState::Running;
                        }
                        h.store.save_task(&task)?;
                    }
                    if task.state == RunState::Waiting
                        && (task.active_turn.is_some()
                            || task.subagents.iter().any(|a| a.state.active()))
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
        children.poll(&host, task_id, &mut codex, &mut event)?;
        if stopping {
            let task = host.lock().unwrap().store.task(task_id)?;
            if !task.state.active() {
                stopping = false;
                interrupted.clear();
            } else {
                let targets = task
                    .subagents
                    .iter()
                    .filter(|agent| agent.state.active())
                    .filter_map(|agent| {
                        agent
                            .active_turn
                            .clone()
                            .map(|turn| (agent.thread_id.clone(), turn))
                    })
                    .chain(task.active_turn.clone().map(|turn| (thread.clone(), turn)));
                for (thread_id, turn_id) in targets {
                    if interrupted.insert((thread_id.clone(), turn_id.clone())) {
                        if let Err(error) = codex.call(
                            "turn/interrupt",
                            json!({"threadId":thread_id,"turnId":turn_id}),
                            &mut event,
                        ) {
                            let h = host.lock().unwrap();
                            let mut task = h.store.task(task_id)?;
                            task.error = Some(format!("Could not stop agent turn: {error}"));
                            h.store.save_task(&task)?;
                        }
                    }
                }
            }
        }
        let waiting = waiting_turn(&host.lock().unwrap(), task_id)?;
        if let Some(task) = waiting {
            // The rule, the model or the known window changed since this process opened the thread.
            let wanted = compact_limit(&host.lock().unwrap(), &task)?;
            if wanted != limit {
                drop(codex);
                (codex, _, limit) = open(&host, task_id, &cwd, &account, &mut event)?;
                children = subagents::Subscriptions::default();
            }
            if begin_due_compaction(&host, task_id)? {
                compact(&host, task_id, &thread, &mut codex, &mut event)?;
            }
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
    let Some(message) = claim_next(host, task_id)? else {
        return Ok(());
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
        let model = effective_model(&host.lock().unwrap(), &task);
        if let Some(model) = model {
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
            let rejected = error.is::<kodem_core::providers::codex::Rejected>();
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
    if subagents::observe(host, task_id, method, p)? {
        return Ok(());
    }
    let task = host.lock().unwrap().store.task(task_id)?;
    let event_thread = p["threadId"].as_str();
    let child = task
        .subagents
        .iter()
        .find(|agent| Some(agent.thread_id.as_str()) == event_thread);
    // Unknown threads must never mutate this task's lifecycle or transcript.
    if event_thread.is_some() && event_thread != task.provider_thread.as_deref() && child.is_none()
    {
        return Ok(());
    }
    let transcript_id =
        child.map_or_else(|| task_id.to_owned(), |agent| agent.transcript_id(task_id));
    let child_thread = child.map(|agent| agent.thread_id.clone());
    if child_thread.is_some()
        && matches!(
            method,
            "turn/started" | "turn/completed" | "thread/status/changed"
        )
    {
        subagents::transition(host, task_id, event_thread.unwrap(), method, p)?;
        return Ok(());
    }
    // Complete the checkout capture before another queued turn becomes eligible.
    if method == "turn/completed" {
        if task
            .active_turn
            .as_deref()
            .is_some_and(|turn| p["turn"]["id"].as_str() != Some(turn))
        {
            return Ok(());
        }
        let state = match p["turn"]["status"].as_str() {
            Some("completed") => RunState::Completed,
            Some("interrupted") => RunState::Interrupted,
            _ => RunState::Failed,
        };
        // A compaction the host asked for is a turn without user input: nothing completes.
        let compacting = host.lock().unwrap().compactions.contains_key(task_id);
        if compacting {
            let failure = match state {
                RunState::Completed => None,
                RunState::Interrupted => Some("it was stopped.".to_owned()),
                _ => Some(
                    p["turn"]["error"]["message"]
                        .as_str()
                        .unwrap_or("Codex reported an error")
                        .to_owned(),
                ),
            };
            return end_compaction(host, task_id, failure);
        }
        subagents::complete_main(
            host,
            task_id,
            TurnOutcome {
                state,
                error: p["turn"]["error"]["message"].as_str().map(str::to_owned),
            },
        )?;
        items.retain(|_, message| {
            message.task_id != task_id && message.task_id != format!("{task_id}/completion")
        });
        return Ok(());
    }
    let observed_branch = if method == "turn/started" {
        let task = host.lock().unwrap().store.task(task_id)?;
        task.state
            .active()
            .then(|| kodem_core::git::branch(&task.cwd))
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
        if let Some(thread) = &child_thread {
            if let Some(agent) = task
                .subagents
                .iter_mut()
                .find(|agent| &agent.thread_id == thread)
            {
                agent.state = RunState::Waiting;
            }
        }
        task.state = RunState::Waiting;
        h.store.save_task(&task)?;
        h.notify_task_state(&task);
        return Ok(());
    }
    if let Some(branch) = observed_branch {
        task.branch = branch;
    }
    if child_thread.is_none() {
        update_activities(h.activities.entry(task_id.into()).or_default(), method, p);
    }
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
                // Codex reports the smaller context while its compaction item is still open,
                // which is the only moment both sizes are known. Only the first report counts.
                let compaction = h.activities.get(task_id).and_then(|activities| {
                    activities
                        .iter()
                        .find(|activity| activity.kind == ActivityKind::Compacting)
                });
                if let (Some(compaction), Some(previous)) = (compaction, &task.context_usage)
                    && !h.store.has_message(task_id, &compaction.id)?
                {
                    h.store.message(&compaction_notice(
                        task_id,
                        &compaction.id,
                        format!("Context compacted from {} to {used} tokens.", previous.used),
                    ))?;
                }
                task.context_usage = Some(ContextUsage {
                    cost: None,
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
            // Quotas belong to the account this session's process was started under.
            let account = task.account_id().to_owned();
            merge_usage(
                accounts::usage_mut(&mut h, Provider::Codex, &account),
                &p["rateLimits"],
            );
        }
        "turn/started" => {
            task.active_turn = p["turn"]["id"].as_str().map(str::to_owned);
            task.state = RunState::Running;
            task.updated = now();
            h.save_task_with_notification(&task)?;
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
            let cache_key = format!("{transcript_id}:{key}");
            let item = items.entry(cache_key).or_insert(Message {
                seq: 0,
                id: key,
                task_id: if child_thread.is_none() && method.contains("agentMessage") {
                    format!("{task_id}/completion")
                } else {
                    transcript_id.clone()
                },
                attachments: Vec::new(),
                feedback: Vec::new(),
                file_edits: Vec::new(),
                tool: None,
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
            if child_thread.is_none() && (kind == "userMessage" || kind == "reasoning") {
                return Ok(());
            }
            let key = format!("{}:{id}", p["turnId"].as_str().unwrap_or("unknown"));
            if kind == "contextCompaction" {
                // The usage report normally wrote the notice with both sizes already.
                if method == "item/completed" && !h.store.has_message(&transcript_id, &key)? {
                    h.store.message(&compaction_notice(
                        &transcript_id,
                        &key,
                        "Context compacted.".into(),
                    ))?;
                }
                return Ok(());
            }
            let (message_kind, text) = match kind {
                "userMessage" => (
                    MessageKind::User,
                    item["content"]
                        .as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default(),
                ),
                "reasoning" => (
                    MessageKind::Notice,
                    item["summary"]
                        .as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default(),
                ),
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
                _ if child_thread.is_some() => {
                    (MessageKind::Tool, serde_json::to_string_pretty(item)?)
                }
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
            let destination = if child_thread.is_none()
                && kind == "agentMessage"
                && item["phase"] != "commentary"
            {
                format!("{task_id}/completion")
            } else {
                transcript_id.clone()
            };
            let mut message = Message {
                seq: 0,
                id: key.clone(),
                task_id: destination,
                kind: message_kind,
                attachments: Vec::new(),
                feedback: Vec::new(),
                file_edits: Vec::new(),
                tool: None,
                text: String::new(),
                complete: method.ends_with("completed"),
                created: now(),
            };
            if kind == "fileChange" && item["status"] == "completed" {
                message.file_edits = completed_file_edits(item);
            }
            append_bounded(&mut message.text, &text);
            if child_thread.is_none() && kind == "agentMessage" && item["phase"] == "commentary" {
                h.store
                    .remove_message(&format!("{task_id}/completion"), &message.id)?;
            }
            h.store.message(&message)?;
            if child_thread.is_none()
                && message.complete
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
            let cache_key = format!("{transcript_id}:{key}");
            if message.complete {
                items.remove(&cache_key);
            } else {
                items.insert(cache_key, message);
            }
        }
        "error" if p["willRetry"].as_bool() != Some(true) => {
            let text = p["error"]["message"]
                .as_str()
                .unwrap_or("Codex reported an error");
            h.store.message(&Message {
                seq: 0,
                id: new_id(),
                task_id: transcript_id.clone(),
                kind: MessageKind::Error,
                attachments: Vec::new(),
                feedback: Vec::new(),
                file_edits: Vec::new(),
                tool: None,
                text: text.into(),
                complete: true,
                created: now(),
            })?;
        }
        _ => {}
    }
    Ok(())
}
fn compaction_notice(transcript: &str, id: &str, text: String) -> Message {
    Message {
        seq: 0,
        id: id.into(),
        task_id: transcript.into(),
        kind: MessageKind::Notice,
        attachments: Vec::new(),
        feedback: Vec::new(),
        file_edits: Vec::new(),
        tool: None,
        text,
        complete: true,
        created: now(),
    }
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

// Preserve edit totals before the display transcript is truncated. Add/delete diffs contain
// full file contents; update diffs contain unified hunks, including header-like source lines.
fn completed_file_edits(item: &Value) -> Vec<FileEdit> {
    item["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|change| {
            let path = change["path"].as_str()?.to_owned();
            let lines = change["diff"].as_str().and_then(|diff| {
                let mut lines = EditedLines::default();
                match change["kind"]["type"].as_str()? {
                    "add" => lines.added = diff.lines().count(),
                    "delete" => lines.removed = diff.lines().count(),
                    "update" => {
                        let mut in_hunk = false;
                        for line in diff.lines() {
                            if line.starts_with("@@ ") {
                                in_hunk = true;
                            } else if line.starts_with("diff --git ") {
                                in_hunk = false;
                            } else if in_hunk {
                                match line.as_bytes().first() {
                                    Some(b'+') => lines.added += 1,
                                    Some(b'-') => lines.removed += 1,
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => return None,
                }
                Some(lines)
            });
            Some(FileEdit { path, lines })
        })
        .collect()
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
    // A live session answers only for the checkout and account its process was started with.
    fn open(host: &Shared, cwd: &std::path::Path, account: &Account) -> Result<Self> {
        let h = host.lock().unwrap();
        let session = h.store.tasks()?.iter().find_map(|task| {
            (task.provider == Provider::Codex && task.cwd == cwd && task.account_id() == account.id)
                .then(|| h.sessions.get(&task.id).cloned())
                .flatten()
        });
        drop(h);
        match session {
            Some(session) => Ok(Self::Session(session)),
            None => Ok(Self::Standalone(Codex::start(cwd, account, |_| {})?)),
        }
    }
    // Account-level reads do not depend on a checkout; any session of the account can answer.
    fn for_account(host: &Shared, account: &Account) -> Result<Self> {
        let h = host.lock().unwrap();
        let session = h.store.tasks()?.iter().find_map(|task| {
            (task.provider == Provider::Codex && task.account_id() == account.id)
                .then(|| h.sessions.get(&task.id).cloned())
                .flatten()
        });
        drop(h);
        match session {
            Some(session) => Ok(Self::Session(session)),
            None => Ok(Self::Standalone(Codex::start(
                &std::env::current_dir()?,
                account,
                |_| {},
            )?)),
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

// Reads the model catalog through any connection to the runtime. New tasks start with 6.1 Sol
// when the runtime lists it, and with the runtime's own default otherwise.
fn read_runtime(mut call: impl FnMut(&'static str, Value) -> Result<Value>) -> Result<RuntimeInfo> {
    let mut models = Vec::new();
    let mut own_default = None;
    let mut cursor = Value::Null;
    loop {
        let result = call("model/list", json!({"includeHidden":false,"cursor":cursor}))?;
        if let Some(data) = result["data"].as_array() {
            for model in data {
                if let Some(id) = model["model"].as_str() {
                    // The installed schema defaults omitted inputModalities to text and image.
                    let images = model["inputModalities"]
                        .as_array()
                        .is_none_or(|modes| modes.iter().any(|mode| mode == "image"));
                    if model["isDefault"] == true {
                        own_default = Some(id.to_owned());
                    }
                    let name = model_name(model["displayName"].as_str().unwrap_or(id));
                    models.push(Model {
                        images,
                        effort_models: Default::default(),
                        id: id.into(),
                        name: if name.is_empty() { id.into() } else { name },
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
    let default_model = default_model(&models, &["6.1", "sol"], own_default.as_deref());
    Ok(RuntimeInfo {
        version: Some(kodem_core::providers::codex::version()?),
        images: models
            .iter()
            .find(|model| Some(&model.id) == default_model.as_ref())
            .is_some_and(|model| model.images),
        models,
        default_model,
        error: None,
        steering: true,
        files: true,
        account_usage: true,
        usage_resets: true,
        access_modes: Vec::new(),
    })
}

/// Reads account and model capabilities from the selected provider runtime.
pub(crate) fn discover(host: Shared, account: &Account, cwd: Option<PathBuf>) -> Result<()> {
    let cwd = cwd.unwrap_or(std::env::current_dir()?);
    let mut codex = Catalog::open(&host, &cwd, account)?;
    let identity = codex.call("account/read", json!({"refreshToken":false}))?;
    let info = read_runtime(|method, params| codex.call(method, params))?;
    let mut h = host.lock().unwrap();
    accounts::observe(&mut h, account, identity_status(&identity));
    h.runtimes.insert(Provider::Codex, info);
    Ok(())
}
/// Reports instruction, skill, and MCP provenance through the existing provider owner when present.
pub(crate) fn resources(
    host: &Shared,
    account: &Account,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<Vec<Resource>> {
    let cwd = if let Some(id) = task_id {
        host.lock().unwrap().store.task(&id)?.cwd
    } else if let Some(id) = project_id {
        host.lock()
            .unwrap()
            .store
            .projects()?
            .into_iter()
            .find(|p| p.id == id)
            .context("Project is no longer available")?
            .path
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
    let mut codex = Catalog::open(host, &cwd, account)?;
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
pub(crate) fn reconcile(host: &Shared, task_id: &str) -> Result<()> {
    let (task, account, submission) = {
        let h = host.lock().unwrap();
        ensure!(
            !h.sessions.contains_key(task_id),
            "The task still has a live transport owner"
        );
        let task = h.store.task(task_id)?;
        let account = accounts::for_task(&h, &task)?;
        (
            task,
            account,
            h.store
                .latest_submission(task_id)?
                .context("No recorded outgoing submission to reconcile")?,
        )
    };
    let thread=task.provider_thread.as_ref().context("The provider never returned a thread identity. No prompt can be reconciled automatically; retain this task and create a new one.")?;
    let mut codex = Codex::start(&task.cwd, &account, |_| {})?;
    for agent in &task.subagents {
        let child = codex.call(
            "thread/read",
            json!({"threadId":agent.thread_id,"includeTurns":false}),
            &mut |_| {},
        )?;
        ensure!(
            child["thread"]["status"]["type"] != "active",
            "A subagent is still working. Reconcile after it settles."
        );
        let latest = codex.call("thread/turns/list", json!({"threadId":agent.thread_id,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}), &mut |_| {})?;
        if let Some(turn) = latest["data"].as_array().and_then(|turns| turns.first()) {
            ensure!(
                turn["status"] != "inProgress",
                "A subagent turn is still active"
            );
            subagents::transition(
                host,
                task_id,
                &agent.thread_id,
                "turn/completed",
                &json!({"turn":turn}),
            )?;
        } else {
            ensure!(
                !agent.state.active() && agent.state != RunState::DeliveryUnknown,
                "Subagent outcome is unavailable; delivery remains unknown"
            );
        }
    }

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

fn identity_status(identity: &Value) -> AccountStatus {
    let account = &identity["account"];
    AccountStatus {
        signed_in: Some(!account.is_null() || identity["requiresOpenaiAuth"] == false),
        email: account["email"].as_str().map(str::to_owned),
        plan: account["planType"].as_str().map(str::to_owned),
        ..AccountStatus::default()
    }
}

/// Reads an account's sign-in state, and its quotas when requested, without a model turn.
///
/// The quick form asks only `codex login status`, which is enough to notice a finished sign-in.
pub(crate) fn account_status(
    host: &Shared,
    account: &Account,
    usage: bool,
) -> Result<AccountStatus> {
    if !usage {
        let mut command = std::process::Command::new(kodem_core::providers::codex::executable()?);
        command.args(["login", "status"]);
        account.apply(&mut command);
        let result = accounts::output(command, Duration::from_secs(20))?;
        return Ok(AccountStatus {
            signed_in: Some(result.status.success()),
            ..AccountStatus::default()
        });
    }
    let mut catalog = Catalog::for_account(host, account)?;
    let mut status = identity_status(&catalog.call("account/read", json!({"refreshToken":false}))?);
    if status.signed_in == Some(true) {
        match read_usage(host, &mut catalog, account) {
            Ok(usage) => status.usage = Some(usage),
            Err(error) => status.error = Some(format!("{error:#}")),
        }
    }
    Ok(status)
}

fn read_usage(host: &Shared, catalog: &mut Catalog, account: &Account) -> Result<UsageLimits> {
    let response = catalog.call("account/rateLimits/read", json!({}))?;
    let value = response["rateLimitsByLimitId"]
        .get("codex")
        .unwrap_or(&response["rateLimits"]);
    let mut usage: UsageLimits = serde_json::from_value(value.clone())?;
    usage.banked_resets = serde_json::from_value(response["rateLimitResetCredits"].clone())?;
    usage.reported = now();
    accounts::set_usage(
        &mut host.lock().unwrap(),
        Provider::Codex,
        &account.id,
        usage.clone(),
    );
    Ok(usage)
}

/// Reads one account's usage or redeems one banked reset, then reads the authoritative balance.
pub(crate) fn account_usage(
    host: &Shared,
    account: &Account,
    idempotency_key: Option<&str>,
) -> Result<Response> {
    let mut catalog = Catalog::for_account(host, account)?;
    let Some(key) = idempotency_key else {
        return Ok(Response::Usage {
            provider: Provider::Codex,
            account: account.id.clone(),
            usage: read_usage(host, &mut catalog, account)?,
        });
    };
    ensure!(
        !key.trim().is_empty(),
        "A reset attempt needs an idempotency key"
    );
    let response = catalog.call(
        "account/rateLimitResetCredit/consume",
        json!({"idempotencyKey": key}),
    )?;
    let outcome = serde_json::from_value(response["outcome"].clone())?;
    // A failed refresh must not turn a confirmed redemption into an ambiguous failure.
    let (usage, refresh_error) = match read_usage(host, &mut catalog, account) {
        Ok(usage) => (Some(usage), None),
        Err(error) => {
            if let Some(usage) =
                accounts::usage_mut(&mut host.lock().unwrap(), Provider::Codex, &account.id)
            {
                usage.banked_resets = None;
            }
            (None, Some(format!("{error:#}")))
        }
    };
    Ok(Response::UsageReset {
        provider: Provider::Codex,
        account: account.id.clone(),
        outcome,
        usage,
        refresh_error,
    })
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
        if incoming.credits.is_none() {
            incoming.credits = old.credits.clone();
        }
        // Reset inventory only arrives on account/rateLimits/read, not rolling events.
        incoming.banked_resets = old.banked_resets.clone();
    }
    *current = Some(incoming);
}

/// Loads enabled skills for the selected task or project without coupling them to MCP health.
pub(crate) fn composer_catalog(
    host: &Shared,
    account: &Account,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<ComposerCatalog> {
    let (cwd, started) = {
        let h = host.lock().unwrap();
        if let Some(id) = task_id {
            let task = h.store.task(&id)?;
            (task.cwd, task.provider_thread.is_some())
        } else if let Some(id) = project_id {
            let project = h
                .store
                .projects()?
                .into_iter()
                .find(|p| p.id == id)
                .context("Project is no longer available")?;
            (project.path, false)
        } else {
            anyhow::bail!("Select a project to load its Codex skills");
        }
    };
    let mut result = ComposerCatalog::default();
    match kodem_core::providers::codex::composer_commands() {
        Ok(commands) => result.commands = commands,
        Err(error) => result.errors.push(format!("Commands: {error:#}")),
    }
    // Only a conversation that exists can be compacted.
    if !started {
        result
            .commands
            .retain(|command| *command != ComposerCommand::Compact);
    }
    let skills = Catalog::open(host, &cwd, account)
        .and_then(|mut codex| codex.call("skills/list", json!({"cwds":[cwd],"forceReload":true})));
    match skills {
        Ok(value) => {
            for group in value["data"].as_array().into_iter().flatten() {
                for skill in group["skills"].as_array().into_iter().flatten() {
                    if skill["enabled"] != true {
                        continue;
                    }
                    let (Some(name), Some(path)) = (skill["name"].as_str(), skill["path"].as_str())
                    else {
                        continue;
                    };
                    result.skills.push(ComposerSkill {
                        name: name.into(),
                        path: path.into(),
                        description: skill["description"].as_str().unwrap_or_default().into(),
                    });
                }
                for error in group["errors"].as_array().into_iter().flatten() {
                    result.errors.push(
                        error["message"]
                            .as_str()
                            .unwrap_or("Could not load a skill")
                            .into(),
                    );
                }
            }
        }
        Err(error) => result.errors.push(format!("Skills: {error:#}")),
    }
    result.skills.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.path.cmp(&b.path))
    });
    result.skills.dedup_by(|a, b| a.path == b.path);
    Ok(result)
}
