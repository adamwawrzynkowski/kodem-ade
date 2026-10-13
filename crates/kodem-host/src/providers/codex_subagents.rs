//! Routes child lifecycle events before they can affect the main conversation.
use super::*;

fn status(value: &str) -> Option<RunState> {
    Some(match value {
        "pendingInit" => RunState::Starting,
        "running" | "inProgress" => RunState::Running,
        "completed" => RunState::Completed,
        "interrupted" | "shutdown" => RunState::Interrupted,
        "errored" | "failed" | "notFound" => RunState::Failed,
        _ => return None,
    })
}

fn register(task: &mut Task, thread: &str, parent: &str, name: Option<&str>) -> bool {
    if task.provider_thread.as_deref() == Some(thread) {
        return false;
    }
    if let Some(agent) = task.subagents.iter_mut().find(|a| a.thread_id == thread) {
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            agent.name = name.to_owned();
        }
        return false;
    }
    task.subagents.push(Subagent {
        history_loaded: false,
        thread_id: thread.into(),
        parent_thread_id: parent.into(),
        name: name
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Agent {}", task.subagents.len() + 1)),
        state: RunState::Starting,
        active_turn: None,
    });
    true
}

fn spawn_message(h: &crate::Host, task: &Task, thread: &str, parent: &str) -> Result<()> {
    let transcript = task
        .subagents
        .iter()
        .find(|a| a.thread_id == parent)
        .map_or_else(|| task.id.clone(), |a| a.transcript_id(&task.id));
    h.store.message(&Message {
        seq: 0,
        transfer: None,
        id: format!("spawn:{thread}"),
        task_id: transcript,
        kind: MessageKind::Subagent,
        text: thread.into(),
        complete: true,
        created: now(),
        attachments: Vec::new(),
        feedback: Vec::new(),
        file_edits: Vec::new(),
        tool: None,
    })
}

/// Registers descendants using protocol parent identities and collaboration items.
pub(super) fn observe(host: &Shared, task_id: &str, method: &str, p: &Value) -> Result<bool> {
    let h = host.lock().unwrap();
    let mut task = h.store.task(task_id)?;
    if method == "thread/started" {
        let thread = &p["thread"];
        let Some(id) = thread["id"].as_str() else {
            return Ok(true);
        };
        let source = &thread["source"]["subAgent"]["thread_spawn"];
        let parent = thread["parentThreadId"]
            .as_str()
            .or_else(|| source["parent_thread_id"].as_str());
        if let Some(parent) = parent {
            if task.provider_thread.as_deref() != Some(parent)
                && !task.subagents.iter().any(|a| a.thread_id == parent)
            {
                return Ok(true);
            }
            let name = thread["agentNickname"]
                .as_str()
                .or_else(|| source["agent_nickname"].as_str())
                .or_else(|| source["agent_path"].as_str());
            if register(&mut task, id, parent, name) {
                spawn_message(&h, &task, id, parent)?;
            }
        } else if task.provider_thread.is_none() && thread["source"].get("subAgent").is_none() {
            task.provider_thread = Some(id.into());
        }
        h.store.save_task(&task)?;
        return Ok(true);
    }
    let item = &p["item"];
    if !matches!(method, "item/started" | "item/completed")
        || !matches!(
            item["type"].as_str(),
            Some("collabAgentToolCall" | "subAgentActivity")
        )
    {
        return Ok(false);
    }
    let Some(parent) = p["threadId"].as_str() else {
        return Ok(true);
    };
    if task.provider_thread.as_deref() != Some(parent)
        && !task.subagents.iter().any(|a| a.thread_id == parent)
    {
        return Ok(true);
    }
    let activity = item["type"] == "subAgentActivity";
    let ids: Vec<&str> = if activity {
        item["agentThreadId"].as_str().into_iter().collect()
    } else {
        item["receiverThreadIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect()
    };
    for id in ids {
        let name = activity.then(|| item["agentPath"].as_str()).flatten();
        if register(&mut task, id, parent, name) {
            spawn_message(&h, &task, id, parent)?;
        }
        let Some(agent) = task.subagents.iter_mut().find(|a| a.thread_id == id) else {
            continue;
        };
        let state = if activity {
            match item["kind"].as_str() {
                Some("started") => Some(RunState::Running),
                Some("completed") => Some(RunState::Completed),
                Some("interrupted") => Some(RunState::Interrupted),
                _ => None,
            }
        } else {
            status(
                item["agentsStates"][id]["status"]
                    .as_str()
                    .unwrap_or_default(),
            )
        };
        if let Some(state) = state {
            let starts_work = activity && item["kind"] == "started"
                || matches!(
                    item["tool"].as_str(),
                    Some("spawnAgent" | "resumeAgent" | "sendInput" | "followupTask")
                );
            if state.active() && !agent.state.active() && !starts_work {
                continue;
            }
            agent.state = state;
            if !agent.state.active() {
                agent.active_turn = None;
            }
        }
    }
    h.store.save_task(&task)?;
    drop(h);
    orchestrator::dispatch(host, task_id, Input::Signal(Signal::ChildrenChanged))?;
    Ok(true)
}

/// Updates only the addressed child's state and leaves main-thread requests intact.
pub(super) fn transition(
    host: &Shared,
    task_id: &str,
    thread: &str,
    method: &str,
    p: &Value,
) -> Result<()> {
    let mut h = host.lock().unwrap();
    let mut task = h.store.task(task_id)?;
    let Some(agent) = task.subagents.iter_mut().find(|a| a.thread_id == thread) else {
        return Ok(());
    };
    match method {
        "turn/started" => {
            agent.state = RunState::Running;
            agent.active_turn = p["turn"]["id"].as_str().map(str::to_owned);
        }
        "turn/completed" => {
            // A delayed completion must not clear a newer child turn.
            if agent
                .active_turn
                .as_deref()
                .is_some_and(|id| p["turn"]["id"].as_str() != Some(id))
            {
                return Ok(());
            }
            agent.state = status(p["turn"]["status"].as_str().unwrap_or_default())
                .unwrap_or(RunState::Failed);
            agent.active_turn = None;
            if let Some(requests) = h.requests.get_mut(task_id) {
                requests.retain(|r| r.params["threadId"].as_str() != Some(thread));
            }
        }
        "thread/status/changed" => {
            match p["status"]["type"].as_str() {
                Some("active") => agent.state = RunState::Running,
                Some("systemError") => {
                    agent.state = RunState::Failed;
                    agent.active_turn = None;
                }
                // An idle status alone does not prove the outcome of a turn.
                _ => {}
            }
        }
        _ => {}
    }
    h.store.save_task(&task)?;
    drop(h);
    orchestrator::dispatch(host, task_id, Input::Signal(Signal::ChildrenChanged))?;
    Ok(())
}

/// Per-session subscriptions and incremental backfill cursors for child threads.
#[derive(Default)]
pub(super) struct Subscriptions {
    cursors: HashMap<String, Option<Value>>,
    positions: HashMap<String, u64>,
}
impl Subscriptions {
    /// Subscribes new children and fills one bounded transcript page between control polls.
    pub(super) fn poll(
        &mut self,
        host: &Shared,
        task_id: &str,
        codex: &mut Codex,
        event: &mut impl FnMut(Value),
    ) -> Result<()> {
        let task = host.lock().unwrap().store.task(task_id)?;
        for agent in &task.subagents {
            if !self.cursors.contains_key(&agent.thread_id) {
                // Resuming subscribes this app-server client without starting a new turn.
                codex.call(
                    "thread/resume",
                    json!({"threadId":agent.thread_id,"excludeTurns":true}),
                    event,
                )?;
                self.cursors
                    .insert(agent.thread_id.clone(), Some(Value::Null));
                self.positions.insert(agent.thread_id.clone(), 0);
                {
                    let h = host.lock().unwrap();
                    let mut current = h.store.task(task_id)?;
                    if let Some(agent) = current
                        .subagents
                        .iter_mut()
                        .find(|a| a.thread_id == agent.thread_id)
                    {
                        agent.history_loaded = false;
                    }
                    h.store.save_task(&current)?;
                }
                let latest = codex.call("thread/turns/list", json!({"threadId":agent.thread_id,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}), event)?;
                if let Some(turn) = latest["data"].as_array().and_then(|turns| turns.first()) {
                    let current = host.lock().unwrap().store.task(task_id)?;
                    if current
                        .subagents
                        .iter()
                        .find(|a| a.thread_id == agent.thread_id)
                        .is_some_and(|a| a.state.active() && a.active_turn.is_none())
                    {
                        event(
                            json!({"method":if turn["status"] == "inProgress" {"turn/started"} else {"turn/completed"},
                            "params":{"threadId":agent.thread_id,"turn":turn}}),
                        );
                    }
                }
                return Ok(());
            }
        }
        let Some((thread, cursor)) = self.cursors.iter().find_map(|(thread, cursor)| {
            cursor
                .as_ref()
                .map(|cursor| (thread.clone(), cursor.clone()))
        }) else {
            return Ok(());
        };
        let page = codex.call(
            "thread/items/list",
            json!({"threadId":thread,"cursor":cursor,"limit":100,"sortDirection":"asc"}),
            event,
        )?;
        let transcript = format!("{task_id}/agent/{thread}");
        for entry in page["data"].as_array().into_iter().flatten() {
            let Some(id) = entry["item"]["id"].as_str() else {
                continue;
            };
            let turn = entry["turnId"].as_str().unwrap_or("unknown");
            let position = self.positions.entry(thread.clone()).or_default();
            *position += 1;
            let message_id = if entry["item"]["type"] == "subAgentActivity" {
                format!(
                    "spawn:{}",
                    entry["item"]["agentThreadId"].as_str().unwrap_or_default()
                )
            } else {
                format!("{turn}:{id}")
            };
            host.lock()
                .unwrap()
                .store
                .order_agent_message(&transcript, &message_id, *position)?;
            if host
                .lock()
                .unwrap()
                .store
                .has_message(&transcript, &format!("{turn}:{id}"))?
            {
                continue;
            }
            // Never replay an old child status over newer live state.
            if matches!(
                entry["item"]["type"].as_str(),
                Some("collabAgentToolCall" | "subAgentActivity")
            ) {
                continue;
            }
            let current = host.lock().unwrap().store.task(task_id)?;
            let streaming = current
                .subagents
                .iter()
                .find(|agent| agent.thread_id == thread)
                .is_some_and(|agent| {
                    agent.state.active() && agent.active_turn.as_deref() == Some(turn)
                })
                && !matches!(
                    entry["item"]["status"].as_str(),
                    Some("completed" | "failed" | "interrupted")
                );
            event(
                json!({"method":if streaming {"item/started"} else {"item/completed"},
                "params":{"threadId":thread,"turnId":turn,"item":entry["item"]}}),
            );
        }
        if page["nextCursor"].is_null() {
            let h = host.lock().unwrap();
            let mut current = h.store.task(task_id)?;
            if let Some(agent) = current.subagents.iter_mut().find(|a| a.thread_id == thread) {
                agent.history_loaded = true;
            }
            h.store.save_task(&current)?;
        }
        self.cursors.insert(
            thread,
            (!page["nextCursor"].is_null()).then(|| page["nextCursor"].clone()),
        );
        Ok(())
    }
}
