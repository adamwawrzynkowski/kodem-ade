//! Per-task Codex ownership, durable delivery, and normalized provider events.
use crate::Shared;
use anyhow::{Context, Result, ensure};
use kodem_core::{runtime::Codex, *};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

pub(super) enum Control {
    Submit {
        id: String,
        text: String,
        feedback: Vec<String>,
    },
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

/// Records outgoing intent before assigning it to the task's sole provider worker.
pub(super) fn submit(
    host: Shared,
    task_id: String,
    id: String,
    text: String,
    feedback: Vec<String>,
) -> Result<()> {
    ensure!(text.len() <= 1024 * 1024, "Prompt exceeds 1 MiB");
    ensure!(
        !text.trim().is_empty() || !feedback.is_empty(),
        "Write a prompt or attach feedback"
    );
    let mut h = host.lock().unwrap();
    let mut task = h.store.task(&task_id)?;
    ensure!(
        !task.state.active(),
        "This task is running. Wait for completion or stop it before sending another turn."
    );
    ensure!(
        task.state != RunState::DeliveryUnknown,
        "Delivery is unknown. Inspect the provider history before making a new task; this submission will not be replayed."
    );
    ensure!(
        !h.store
            .tasks()?
            .iter()
            .any(|t| t.id != task.id && t.cwd == task.cwd && t.state.active()),
        "Another task is writing to this checkout. Use a separate task worktree."
    );
    let mut prompt = text.clone();
    if !feedback.is_empty() {
        let selected: Vec<_> = feedback
            .iter()
            .map(|id| {
                task.feedback
                    .iter()
                    .find(|f| &f.id == id)
                    .context("Review comment is no longer queued")
            })
            .collect::<Result<_>>()?;
        prompt.push_str("\n\nReview feedback refers to captured content. Check current files before applying it:\n");
        prompt.push_str(&serde_json::to_string_pretty(&selected)?);
    }
    ensure!(
        prompt.len() <= 2 * 1024 * 1024,
        "Prompt and attached feedback exceed 2 MiB; send a smaller review batch"
    );
    task.draft = text.clone();
    if !h
        .store
        .begin_submission(&id, &task_id, &prompt, &feedback)?
    {
        return Ok(());
    }
    task.state = RunState::Starting;
    task.error = None;
    task.updated = now();
    if task.title == "New task" {
        task.title = text
            .lines()
            .next()
            .unwrap_or("Review feedback")
            .chars()
            .take(90)
            .collect();
    }
    h.store.save_task(&task)?;
    h.store.message(&Message {
        seq: 0,
        id: format!("submission-{id}"),
        task_id: task_id.clone(),
        kind: MessageKind::User,
        text: prompt.clone(),
        complete: true,
        created: now(),
    })?;
    let control = Control::Submit {
        id,
        text: prompt,
        feedback,
    };
    if let Some(sender) = h.sessions.get(&task_id) {
        sender.send(control)?;
        return Ok(());
    }
    let (tx, rx) = mpsc::channel();
    tx.send(control)?;
    h.sessions.insert(task_id.clone(), tx);
    drop(h);
    std::thread::spawn(move || {
        let delivery_attempted = Arc::new(AtomicBool::new(false));
        if let Err(e) = run(host.clone(), &task_id, rx, delivery_attempted.clone()) {
            let h = host.lock().unwrap();
            if let Ok(mut task) = h.store.task(&task_id) {
                if task.state.active() {
                    let unknown = delivery_attempted.load(Ordering::SeqCst);
                    task.state = if unknown {
                        RunState::DeliveryUnknown
                    } else {
                        RunState::Failed
                    };
                    let _ = h.store.settle_pending(&task_id, unknown);
                    task.error = Some(format!("{e:#}. No prompt was automatically resent."));
                }

                let _ = h.store.save_task(&task);
            }
        }
        let mut h = host.lock().unwrap();
        h.sessions.remove(&task_id);
        h.requests.remove(&task_id);
    });
    Ok(())
}
fn run(
    host: Shared,
    task_id: &str,
    controls: mpsc::Receiver<Control>,
    delivery_attempted: Arc<AtomicBool>,
) -> Result<()> {
    let cwd = host.lock().unwrap().store.task(task_id)?.cwd;
    let mut items: HashMap<String, Message> = HashMap::new();
    let event_host = host.clone();
    let event_task = task_id.to_owned();
    let event_delivery = delivery_attempted.clone();
    let mut event = move |value: Value| {
        if value["method"] == "turn/completed" {
            event_delivery.store(false, Ordering::SeqCst);
        }

        if let Err(e) = ingest(&event_host, &event_task, value, &mut items) {
            let h = event_host.lock().unwrap();
            if let Ok(mut task) = h.store.task(&event_task) {
                task.error = Some(format!("Could not persist provider event: {e}"));
                let _ = h.store.save_task(&task);
            }
        }
    };
    let mut codex = Codex::start(&cwd, &mut event)?;
    let task = host.lock().unwrap().store.task(task_id)?;
    let mut params = json!({"cwd":cwd,"approvalPolicy":"on-request","sandbox":"workspace-write"});
    if let Some(model) = &task.model {
        params["model"] = json!(model)
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
        h.store.save_task(&task)?;
    }
    loop {
        while let Ok(control) = controls.try_recv() {
            match control {
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
                Control::Submit { id, text, feedback } => {
                    let task = host.lock().unwrap().store.task(task_id)?;
                    let mut params = json!({"threadId":thread,"clientUserMessageId":id,"input":[{"type":"text","text":text}]});
                    if let Some(model) = task.model {
                        params["model"] = json!(model)
                    }
                    if let Some(effort) = task.effort {
                        params["effort"] = json!(effort)
                    }
                    delivery_attempted.store(true, Ordering::SeqCst);
                    match codex.call("turn/start", params, &mut event) {
                        Ok(reply) => {
                            let turn = reply["turn"]["id"]
                                .as_str()
                                .context("Codex omitted turn acknowledgement")?
                                .to_owned();
                            let h = host.lock().unwrap();
                            h.store.confirm_submission(&id, &turn)?;
                            let mut task = h.store.task(task_id)?;
                            // Completion can arrive before the request response. Do not resurrect a finished turn.
                            if task.state == RunState::Starting {
                                task.state = RunState::Running;
                                task.active_turn = Some(turn);
                            }
                            task.draft.clear();
                            task.feedback.retain(|f| !feedback.contains(&f.id));
                            h.store.save_task(&task)?;
                        }
                        Err(error) => {
                            let rejected = error.is::<kodem_core::runtime::Rejected>();
                            if rejected {
                                delivery_attempted.store(false, Ordering::SeqCst);
                            }
                            host.lock()
                                .unwrap()
                                .store
                                .delivery(&id, if rejected { "failed" } else { "unknown" })?;
                            return Err(error);
                        }
                    }
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
                    {
                        let h = host.lock().unwrap();
                        if !h
                            .requests
                            .get(task_id)
                            .is_some_and(|r| r.iter().any(|r| r.id == id))
                        {
                            continue;
                        }
                    }
                    codex.answer(id.clone(), response)?;
                    let mut h = host.lock().unwrap();
                    if let Some(requests) = h.requests.get_mut(task_id) {
                        requests.retain(|r| r.id != id);
                    }
                    let mut task = h.store.task(task_id)?;
                    if task.state == RunState::Waiting
                        && task.active_turn.is_some()
                        && h.requests.get(task_id).is_none_or(|r| r.is_empty())
                    {
                        task.state = RunState::Running;
                        h.store.save_task(&task)?;
                    }
                }
            }
        }
        codex.poll(Duration::from_millis(40), &mut event)?;
    }
}
fn ingest(
    host: &Shared,
    task_id: &str,
    value: Value,
    items: &mut HashMap<String, Message>,
) -> Result<()> {
    let method = value["method"].as_str().unwrap_or_default();
    let p = &value["params"];
    let mut h = host.lock().unwrap();
    let mut task = h.store.task(task_id)?;
    if value.get("id").is_some() {
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
        return Ok(());
    }
    match method {
        "turn/started" => {
            task.active_turn = p["turn"]["id"].as_str().map(str::to_owned);
            task.state = RunState::Running;
            task.updated = now();
            h.store.save_task(&task)?;
        }
        "turn/completed" => {
            task.active_turn = None;
            task.state = match p["turn"]["status"].as_str() {
                Some("completed") => RunState::Completed,
                Some("interrupted") => RunState::Interrupted,
                _ => RunState::Failed,
            };
            task.error = p["turn"]["error"]["message"].as_str().map(str::to_owned);
            task.updated = now();
            h.requests.remove(task_id);
            h.store.save_task(&task)?;
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
                h.store.save_task(&task)?;
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
                text: String::new(),
                complete: method.ends_with("completed"),
                created: now(),
            };
            append_bounded(&mut message.text, &text);
            h.store.message(&message)?;
            if message.complete {
                items.remove(&key);
            } else {
                items.insert(key, message);
            }
        }
        "error" => {
            let text = p["error"]["message"]
                .as_str()
                .unwrap_or("Codex reported an error");
            h.store.message(&Message {
                seq: 0,
                id: new_id(),
                task_id: task_id.into(),
                kind: MessageKind::Error,
                text: text.into(),
                complete: true,
                created: now(),
            })?;
        }
        _ => {}
    }
    Ok(())
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
    if current.draft == submission.prompt {
        current.draft.clear();
    }
    current
        .feedback
        .retain(|f| !submission.feedback.contains(&f.id));
    current.error = None;
    h.store.save_task(&current)?;
    Ok(())
}
