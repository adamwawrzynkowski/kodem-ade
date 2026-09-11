//! Native Devin ACP sessions with durable input and provider-reported capabilities.
#[path = "devin_transport.rs"]
mod transport;
use crate::{Shared, provider};
use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use kodem_core::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
use transport::Transport;

struct Session {
    transport: Transport,
    id: String,
    initialized: Value,
    config: Vec<Value>,
    default_model: Option<String>,
    commands: Vec<ProviderCommand>,
}

impl Session {
    fn open(cwd: &Path, existing: Option<&str>) -> Result<Self> {
        let (mut transport, initialized) = Transport::start(cwd)?;
        let result = if let Some(id) = existing {
            ensure!(
                initialized["agentCapabilities"]["loadSession"] == true,
                "This Devin CLI cannot resume saved sessions"
            );
            transport.call(
                "session/load",
                json!({"sessionId":id,"cwd":cwd,"mcpServers":[]}),
            )?
        } else {
            transport.call("session/new", json!({"cwd":cwd,"mcpServers":[]}))?
        };
        let id = existing
            .map(str::to_owned)
            .or_else(|| result["sessionId"].as_str().map(str::to_owned))
            .context("Devin omitted session identity")?;
        let mut session = Self {
            transport,
            id,
            initialized,
            config: result["configOptions"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            default_model: None,
            commands: Vec::new(),
        };
        // Loading history replays prior messages. Kodem already stores them; only restore metadata here.
        while let Some(value) = session.transport.poll(Duration::ZERO)? {
            session.metadata(&value);
        }
        session.default_model = session
            .config_option("model")
            .and_then(|option| option["currentValue"].as_str())
            .map(str::to_owned);
        Ok(session)
    }

    fn metadata(&mut self, value: &Value) -> bool {
        if value["method"] != "session/update" || value["params"]["sessionId"] != self.id {
            return false;
        }
        let update = &value["params"]["update"];
        match update["sessionUpdate"].as_str() {
            Some("config_option_update") => {
                if let Some(options) = update["configOptions"].as_array() {
                    self.config = options.clone();
                }
                true
            }
            Some("available_commands_update") => {
                self.commands = update["availableCommands"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|command| {
                        Some(ProviderCommand {
                            name: command["name"].as_str()?.to_owned(),
                            description: command["description"]
                                .as_str()
                                .unwrap_or_default()
                                .to_owned(),
                            input_hint: command["input"]["hint"].as_str().map(str::to_owned),
                        })
                    })
                    .collect();
                true
            }
            _ => false,
        }
    }

    fn config_option(&self, category: &str) -> Option<&Value> {
        self.config.iter().find(|option| {
            option["type"] == "select"
                && (option["category"] == category || option["id"] == category)
        })
    }

    fn set_option(&mut self, category: &str, selected: &str) -> Result<()> {
        let option = self
            .config_option(category)
            .context(format!("Devin does not advertise a {category} selector"))?;
        if option["currentValue"] == selected {
            return Ok(());
        }
        ensure!(
            option_values(option)
                .iter()
                .any(|value| value["value"] == selected),
            "Devin does not currently support {category} '{selected}'. Refresh Providers settings"
        );
        let config_id = option["id"]
            .as_str()
            .context("Devin config option has no identity")?
            .to_owned();
        let result = self.transport.call(
            "session/set_config_option",
            json!({"sessionId":self.id,"configId":config_id,"value":selected}),
        )?;
        while let Some(value) = self.transport.poll(Duration::ZERO)? {
            self.metadata(&value);
        }
        self.config = result["configOptions"]
            .as_array()
            .cloned()
            .context("Devin omitted updated configuration")?;
        Ok(())
    }

    fn configure(&mut self, task: &Task) -> Result<()> {
        if let Some(model) = task.model.as_ref().or(self.default_model.as_ref()).cloned() {
            self.set_option("model", &model)?;
        }
        if let Some(effort) = &task.effort {
            self.set_option("thought_level", effort)?;
        }
        if let Some(access) = &task.provider_access {
            self.set_option("mode", access)?;
        }
        Ok(())
    }

    fn runtime(&self) -> RuntimeInfo {
        let images = self.initialized["agentCapabilities"]["promptCapabilities"]["image"] == true;
        let current_model = self
            .config_option("model")
            .and_then(|option| option["currentValue"].as_str());
        let reasoning = self.config_option("thought_level");
        let models: Vec<Model> = self
            .config_option("model")
            .map(option_values)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|option| {
                let id = option["value"].as_str()?;
                let effort = (Some(id) == current_model).then_some(reasoning).flatten();
                Some(Model {
                    id: id.to_owned(),
                    name: option["name"].as_str().unwrap_or(id).to_owned(),
                    images: images
                        && option["_meta"]["cognition.ai/supportsImages"]
                            .as_bool()
                            .unwrap_or(false),
                    efforts: effort
                        .map(option_values)
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|option| option["value"].as_str().map(str::to_owned))
                        .collect(),
                    default_effort: effort
                        .and_then(|option| option["currentValue"].as_str())
                        .unwrap_or_default()
                        .to_owned(),
                    effort_models: BTreeMap::new(),
                })
            })
            .collect();
        RuntimeInfo {
            default_model: self.default_model.clone(),
            images: models.iter().any(|model| {
                Some(model.id.as_str()) == self.default_model.as_deref() && model.images
            }),
            models,
            files: self.initialized["agentCapabilities"]["promptCapabilities"]["embeddedContext"]
                == true,
            access_modes: self
                .config_option("mode")
                .map(option_values)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|option| {
                    Some(ProviderAccess {
                        id: option["value"].as_str()?.to_owned(),
                        name: option["name"].as_str().unwrap_or_default().to_owned(),
                        description: option["description"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    })
                })
                .collect(),
            ..RuntimeInfo::default()
        }
    }

    fn publish_capabilities(&self, host: &Shared) {
        let mut h = host.lock().unwrap();
        let mut info = self.runtime();
        if let Some(previous) = h.runtimes.get(&Provider::Devin) {
            info.authenticated = previous.authenticated;
            info.version = previous.version.clone();
            let current_model = self
                .config_option("model")
                .and_then(|option| option["currentValue"].as_str());
            for model in &mut info.models {
                if Some(model.id.as_str()) != current_model
                    && let Some(known) = previous.models.iter().find(|known| known.id == model.id)
                {
                    model.efforts = known.efforts.clone();
                    model.default_effort = known.default_effort.clone();
                    model.effort_models = known.effort_models.clone();
                }
            }
        }
        h.runtimes.insert(Provider::Devin, info);
    }

    fn catalog(&self) -> ComposerCatalog {
        let mut commands = vec![
            ComposerCommand::Provider,
            ComposerCommand::New,
            ComposerCommand::Status,
        ];
        if self.config_option("model").is_some() {
            commands.push(ComposerCommand::Model);
        }
        if self.config_option("mode").is_some() {
            commands.push(ComposerCommand::Permissions);
        }
        ComposerCatalog {
            commands,
            provider_commands: self.commands.clone(),
            ..ComposerCatalog::default()
        }
    }

    fn delete(&mut self) -> Result<()> {
        ensure!(
            self.initialized["agentCapabilities"]["sessionCapabilities"]["delete"].is_object(),
            "Devin does not support deleting temporary discovery sessions"
        );
        self.transport
            .call("session/delete", json!({"sessionId":self.id}))?;
        Ok(())
    }
}

fn option_values(option: &Value) -> Vec<&Value> {
    option["options"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|value| {
            if value["options"].is_array() {
                value["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
            } else {
                vec![value]
            }
        })
        .collect()
}

/// Reads selectable models and access modes from ACP, removing its empty discovery session.
pub(super) fn discover(host: Shared, cwd: Option<PathBuf>) -> Result<()> {
    let cwd = cwd.unwrap_or(std::env::current_dir()?);
    let mut session = Session::open(&cwd, None)?;
    let mut info = session.runtime();
    let deleted = session.delete();
    let version = Command::new(transport::executable())
        .arg("version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    ensure!(version.status.success(), "Could not read Devin CLI version");
    info.version = Some(String::from_utf8_lossy(&version.stdout).trim().to_owned());
    let status = Command::new(transport::executable())
        .args(["auth", "status"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    // The CLI status output can include account details; retain only its documented sign-in marker.
    let output = String::from_utf8_lossy(&status.stdout);
    info.authenticated = if output.starts_with("Logged in") {
        Some(true)
    } else if output.contains("Not logged in") || output.contains("not logged in") {
        Some(false)
    } else {
        None
    };
    deleted.context("Could not delete Devin's empty discovery session")?;
    host.lock().unwrap().runtimes.insert(Provider::Devin, info);
    Ok(())
}

struct Prompt {
    request: u64,
    submission: String,
    turn: String,
    observed: bool,
    cancelled: bool,
}

struct Transcript {
    messages: HashMap<String, Message>,
    tools: HashMap<String, Value>,
    assistant: Option<String>,
    sequence: u64,
}

impl Transcript {
    fn new() -> Self {
        Self {
            messages: HashMap::new(),
            tools: HashMap::new(),
            assistant: None,
            sequence: 0,
        }
    }

    fn finish_message(&mut self, host: &Shared) -> Result<()> {
        if let Some(id) = self.assistant.take()
            && let Some(message) = self.messages.get_mut(&id)
        {
            message.complete = true;
            host.lock().unwrap().store.message(message)?;
        }
        Ok(())
    }

    fn finish(&mut self, host: &Shared) -> Result<()> {
        let h = host.lock().unwrap();
        for message in self.messages.values_mut() {
            message.complete = true;
            h.store.message(message)?;
        }
        self.messages.clear();
        self.tools.clear();
        self.assistant = None;
        Ok(())
    }
}

/// Owns one resumable ACP session and pumps long-running prompts without blocking cancellation.
pub(super) fn run(
    host: Shared,
    task_id: &str,
    controls: mpsc::Receiver<provider::Control>,
) -> Result<()> {
    let task = host.lock().unwrap().store.task(task_id)?;
    let mut session = Session::open(&task.cwd, task.provider_thread.as_deref())?;
    if task.provider_thread.is_some()
        && let Some(default) = host
            .lock()
            .unwrap()
            .runtimes
            .get(&Provider::Devin)
            .and_then(|runtime| runtime.default_model.clone())
    {
        session.default_model = Some(default);
    }
    {
        let h = host.lock().unwrap();
        let mut current = h.store.task(task_id)?;
        current.provider_thread = Some(session.id.clone());
        h.save_task_with_notification(&current)?;
    }
    session.publish_capabilities(&host);
    crate::titles::start(host.clone(), task_id)?;
    let mut pending: Option<Prompt> = None;
    let mut transcript = Transcript::new();
    let mut branch_refresh = Instant::now();
    loop {
        while let Ok(control) = controls.try_recv() {
            match control {
                provider::Control::Close => return Ok(()),
                provider::Control::Wake => {}
                provider::Control::Interrupt => {
                    if let Some(prompt) = pending.as_mut() {
                        prompt.cancelled = true;
                        cancel(&host, task_id, &mut session)?;
                    }
                }
                provider::Control::Answer { id, response } => {
                    if response["decision"] == "cancel"
                        && let Some(prompt) = pending.as_mut()
                    {
                        prompt.cancelled = true;
                    }
                    answer(&host, task_id, &mut session, id, response)?
                }
                provider::Control::Query {
                    method,
                    params: _,
                    reply,
                } => {
                    let result = if method == "kodem/catalog" {
                        serde_json::to_value(session.catalog()).map_err(|error| error.to_string())
                    } else {
                        Err("This operation is not supported by Devin ACP".into())
                    };
                    let _ = reply.send(result);
                }
            }
        }
        if let Some(value) = session.transport.poll(Duration::from_millis(40))? {
            if session.metadata(&value) {
                session.publish_capabilities(&host);
                if pending.is_some()
                    && value["params"]["update"]["sessionUpdate"] == "config_option_update"
                {
                    sync_selection(&host, task_id, &session)?;
                }
            } else if value["method"] == "session/update"
                && value["params"]["sessionId"] == session.id
            {
                if let Some(prompt) = pending.as_mut() {
                    prompt.observed = true;
                    ingest(
                        &host,
                        task_id,
                        &prompt.turn,
                        &value["params"]["update"],
                        &mut transcript,
                    )?;
                }
            } else if value.get("method").is_some() && value.get("id").is_some() {
                if value["method"] == "session/request_permission"
                    && value["params"]["sessionId"] == session.id
                    && pending.is_some()
                {
                    if pending.as_ref().is_some_and(|prompt| prompt.cancelled) {
                        session.transport.answer(
                            value["id"].clone(),
                            json!({"outcome":{"outcome":"cancelled"}}),
                        )?;
                    } else {
                        permission(&host, task_id, &mut session, &value)?;
                    }
                } else {
                    session.transport.reject(value["id"].clone())?;
                }
            } else if pending
                .as_ref()
                .is_some_and(|prompt| value["id"].as_u64() == Some(prompt.request))
            {
                let prompt = pending.take().unwrap();
                finish_prompt(&host, task_id, prompt, &value, &mut transcript)?;
            }
        }
        if pending.is_none() {
            let task = host.lock().unwrap().store.task(task_id)?;
            if matches!(task.state, RunState::Idle | RunState::Completed) {
                session.configure(&task)?;
                session.publish_capabilities(&host);
                if let Some(message) = provider::claim_next(&host, task_id)? {
                    let input = match input(&session, &task, &message) {
                        Ok(input) => input,
                        Err(error) => {
                            host.lock().unwrap().store.fail_outgoing(
                                task_id,
                                &message.id,
                                false,
                                &error.to_string(),
                            )?;
                            provider::complete_turn(
                                &host,
                                task_id,
                                RunState::Failed,
                                Some(error.to_string()),
                            )?;
                            continue;
                        }
                    };
                    // This local identity links transcript items; it is never evidence for reconciliation.
                    let turn = format!("devin:{}", message.id);
                    let request = session.transport.request(
                        "session/prompt",
                        json!({"sessionId":session.id,"prompt":input}),
                    )?;
                    pending = Some(Prompt {
                        request,
                        submission: message.id,
                        turn: turn.clone(),
                        observed: false,
                        cancelled: false,
                    });
                    let h = host.lock().unwrap();
                    let mut task = h.store.task(task_id)?;
                    task.active_turn = Some(turn);
                    task.state = RunState::Running;
                    h.save_task_with_notification(&task)?;
                }
            }
        }
        if pending.is_some() && branch_refresh.elapsed() >= Duration::from_secs(5) {
            branch_refresh = Instant::now();
            let branch = kodem_core::git::branch(&task.cwd);
            let h = host.lock().unwrap();
            let mut task = h.store.task(task_id)?;
            if task.branch != branch {
                task.branch = branch;
                h.save_task_with_notification(&task)?;
            }
        }
    }
}

// Native mode/model slash commands update the same task choices shown by the composer.
fn sync_selection(host: &Shared, task_id: &str, session: &Session) -> Result<()> {
    let h = host.lock().unwrap();
    let mut task = h.store.task(task_id)?;
    if let Some(model) = session
        .config_option("model")
        .and_then(|option| option["currentValue"].as_str())
    {
        if task.model.as_deref().or(session.default_model.as_deref()) != Some(model) {
            task.model = Some(model.to_owned());
            if let Some(usage) = &mut task.context_usage {
                usage.window = None;
            }
        }
    }
    task.effort = session
        .config_option("thought_level")
        .and_then(|option| option["currentValue"].as_str())
        .map(str::to_owned);
    task.provider_access = session
        .config_option("mode")
        .and_then(|option| option["currentValue"].as_str())
        .map(str::to_owned);
    h.store.save_task(&task)
}

fn input(session: &Session, task: &Task, message: &OutgoingMessage) -> Result<Vec<Value>> {
    ensure!(
        message.expected_turn.is_none(),
        "Devin ACP does not support steering an active turn"
    );
    let mut input = vec![json!({"type":"text","text":provider::prompt_for(message)?})];
    for attachment in &message.attachments {
        if attachment.kind == AttachmentKind::Image {
            let runtime = session.runtime();
            let model = task.model.as_deref().or_else(|| {
                session
                    .config_option("model")
                    .and_then(|option| option["currentValue"].as_str())
            });
            ensure!(
                runtime
                    .models
                    .iter()
                    .any(|entry| Some(entry.id.as_str()) == model && entry.images),
                "The selected Devin model does not advertise image support"
            );
            let bytes =
                std::fs::read(&attachment.path).context("Could not read the attached image")?;
            ensure!(bytes.len() <= 20 * 1024 * 1024, "Image exceeds 20 MiB");
            let format = image::guess_format(&bytes).context("Unrecognized image format")?;
            input.push(json!({"type":"image","mimeType":format.to_mime_type(),"data":base64::engine::general_purpose::STANDARD.encode(bytes)}));
        }
    }
    Ok(input)
}

fn finish_prompt(
    host: &Shared,
    task_id: &str,
    prompt: Prompt,
    value: &Value,
    transcript: &mut Transcript,
) -> Result<()> {
    if let Some(error) = value.get("error") {
        let rejected =
            !prompt.observed && matches!(error["code"].as_i64(), Some(-32601 | -32602 | -32002));
        host.lock().unwrap().store.fail_outgoing(
            task_id,
            &prompt.submission,
            !rejected,
            &error.to_string(),
        )?;
        if !rejected {
            bail!("Devin prompt failed without a confirmed outcome: {error}");
        }
        transcript.finish(host)?;
        return provider::complete_turn(
            host,
            task_id,
            RunState::Failed,
            Some(format!("Devin rejected the prompt: {error}")),
        );
    }
    let stop = value["result"]["stopReason"]
        .as_str()
        .context("Devin omitted the prompt outcome; delivery remains unknown")?;
    let (state, error) = match stop {
        "end_turn" => (RunState::Completed, None),
        "cancelled" => (RunState::Interrupted, None),
        "max_tokens" => (
            RunState::Interrupted,
            Some("Devin reached the model output token limit".into()),
        ),
        "max_turn_requests" => (
            RunState::Interrupted,
            Some("Devin reached its model request limit".into()),
        ),
        "refusal" => (
            RunState::Failed,
            Some("Devin declined to continue this request".into()),
        ),
        _ => bail!("Unrecognized Devin prompt outcome '{stop}'; delivery remains unknown"),
    };
    transcript.finish(host)?;
    // Only the response to this exact outstanding RPC confirms delivery, never a text match.
    host.lock()
        .unwrap()
        .store
        .confirm_submission(&prompt.submission, &prompt.turn)?;
    provider::complete_turn(host, task_id, state, error)
}

fn permission(host: &Shared, task_id: &str, session: &mut Session, value: &Value) -> Result<()> {
    let mut choices = serde_json::Map::new();
    let mut labels = serde_json::Map::new();
    for option in value["params"]["options"].as_array().into_iter().flatten() {
        let decision = match option["kind"].as_str() {
            Some("allow_once") => "accept",
            Some("allow_always") => "acceptForSession",
            Some("reject_once") => "decline",
            Some("reject_always") => "declineAlways",
            _ => continue,
        };
        if let Some(id) = option["optionId"].as_str() {
            choices.insert(decision.into(), json!(id));
            labels.insert(
                decision.into(),
                json!(option["name"].as_str().unwrap_or(decision)),
            );
        }
    }
    if choices.is_empty() {
        return session.transport.answer(
            value["id"].clone(),
            json!({"outcome":{"outcome":"cancelled"}}),
        );
    }
    let mut available: Vec<Value> = choices.keys().map(|key| json!(key)).collect();
    available.push(json!("cancel"));
    let call = &value["params"]["toolCall"];
    let command = call["rawInput"]["command"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if call["rawInput"].is_null() {
                call["title"]
                    .as_str()
                    .unwrap_or("Inspect the tool operation before allowing it.")
                    .to_owned()
            } else {
                call["rawInput"].to_string().chars().take(4000).collect()
            }
        });
    let mut h = host.lock().unwrap();
    let requests = h.requests.entry(task_id.into()).or_default();
    if requests.iter().any(|request| request.id == value["id"]) {
        return Ok(());
    }
    requests.push(InputRequest {
        id: value["id"].clone(), task_id: task_id.into(), method: "devin/requestApproval".into(),
        params: json!({"reason":call["title"].as_str().unwrap_or("Devin requests permission"),"command":command,"availableDecisions":available,"providerOptions":choices,"decisionLabels":labels}),
    });
    let mut task = h.store.task(task_id)?;
    task.state = RunState::Waiting;
    h.save_task_with_notification(&task)
}

fn answer(
    host: &Shared,
    task_id: &str,
    session: &mut Session,
    id: Value,
    response: Value,
) -> Result<()> {
    let request = host
        .lock()
        .unwrap()
        .requests
        .get(task_id)
        .and_then(|requests| requests.iter().find(|request| request.id == id))
        .cloned();
    let Some(request) = request else {
        return Ok(());
    };
    let decision = response["decision"]
        .as_str()
        .context("Missing permission decision")?;
    let outcome = if decision == "cancel" {
        json!({"outcome":"cancelled"})
    } else {
        let option = request.params["providerOptions"][decision]
            .as_str()
            .context("Devin did not offer this permission choice")?;
        json!({"outcome":"selected","optionId":option})
    };
    session
        .transport
        .answer(id.clone(), json!({"outcome":outcome}))?;
    {
        let mut h = host.lock().unwrap();
        if let Some(requests) = h.requests.get_mut(task_id) {
            requests.retain(|request| request.id != id);
        }
        let mut task = h.store.task(task_id)?;
        if task.state == RunState::Waiting
            && h.requests
                .get(task_id)
                .is_none_or(|requests| requests.is_empty())
        {
            task.state = RunState::Running;
            h.save_task_with_notification(&task)?;
        }
    }
    if decision == "cancel" {
        cancel(host, task_id, session)?;
    }
    Ok(())
}

fn cancel(host: &Shared, task_id: &str, session: &mut Session) -> Result<()> {
    let requests = host
        .lock()
        .unwrap()
        .requests
        .get(task_id)
        .cloned()
        .unwrap_or_default();
    for request in requests {
        session
            .transport
            .answer(request.id, json!({"outcome":{"outcome":"cancelled"}}))?;
    }
    host.lock().unwrap().requests.remove(task_id);
    session
        .transport
        .notify("session/cancel", json!({"sessionId":session.id}))
}

fn message(task_id: &str, id: String, kind: MessageKind) -> Message {
    Message {
        seq: 0,
        id,
        task_id: task_id.into(),
        kind,
        attachments: Vec::new(),
        feedback: Vec::new(),
        file_edits: Vec::new(),
        text: String::new(),
        complete: false,
        created: now(),
    }
}

fn append_bounded(target: &mut String, text: &str) {
    const LIMIT: usize = 1024 * 1024;
    if target.len() >= LIMIT {
        return;
    }
    let mut length = text.len().min(LIMIT - target.len());
    while !text.is_char_boundary(length) {
        length -= 1;
    }
    target.push_str(&text[..length]);
}

fn ingest(
    host: &Shared,
    task_id: &str,
    turn: &str,
    update: &Value,
    transcript: &mut Transcript,
) -> Result<()> {
    match update["sessionUpdate"].as_str() {
        Some("agent_message_chunk") if update["content"]["type"] == "text" => {
            let explicit = update["messageId"]
                .as_str()
                .map(|id| format!("{turn}:message:{id}"));
            if explicit.is_some() && explicit != transcript.assistant {
                transcript.finish_message(host)?;
            }
            let id = transcript
                .assistant
                .get_or_insert_with(|| {
                    transcript.sequence += 1;
                    explicit.unwrap_or_else(|| format!("{turn}:message:{}", transcript.sequence))
                })
                .clone();
            let item = transcript
                .messages
                .entry(id.clone())
                .or_insert_with(|| message(task_id, id, MessageKind::Assistant));
            append_bounded(
                &mut item.text,
                update["content"]["text"].as_str().unwrap_or_default(),
            );
            let mut h = host.lock().unwrap();
            h.store.message(item)?;
            let activities = h.activities.entry(task_id.into()).or_default();
            activities.retain(|activity| activity.id != "devin-thinking");
        }
        Some("agent_thought_chunk") => {
            let mut h = host.lock().unwrap();
            let activities = h.activities.entry(task_id.into()).or_default();
            if !activities
                .iter()
                .any(|activity| activity.id == "devin-thinking")
            {
                activities.push(Activity {
                    id: "devin-thinking".into(),
                    kind: ActivityKind::Reasoning,
                    detail: String::new(),
                    started: now(),
                });
            }
        }
        Some("tool_call" | "tool_call_update") => {
            transcript.finish_message(host)?;
            let Some(tool_id) = update["toolCallId"].as_str() else {
                return Ok(());
            };
            let key = format!("{turn}:tool:{tool_id}");
            let previous = transcript
                .tools
                .entry(key.clone())
                .or_insert_with(|| json!({}));
            if let (Some(current), Some(incoming)) = (previous.as_object_mut(), update.as_object())
            {
                for (field, value) in incoming {
                    if !value.is_null() {
                        current.insert(field.clone(), value.clone());
                    }
                }
            }
            let mut item = message(task_id, key.clone(), MessageKind::Tool);
            let title = previous["title"].as_str().unwrap_or("Tool operation");
            append_bounded(&mut item.text, title);
            for content in previous["content"].as_array().into_iter().flatten() {
                if content["type"] == "content" && content["content"]["type"] == "text" {
                    append_bounded(&mut item.text, "\n");
                    append_bounded(
                        &mut item.text,
                        content["content"]["text"].as_str().unwrap_or_default(),
                    );
                } else if content["type"] == "diff"
                    && let Some(path) = content["path"].as_str()
                {
                    append_bounded(&mut item.text, "\n");
                    append_bounded(&mut item.text, path);
                    if previous["status"] == "completed" {
                        item.file_edits.push(FileEdit {
                            path: path.into(),
                            lines: None,
                        });
                    }
                }
            }
            item.complete = matches!(previous["status"].as_str(), Some("completed" | "failed"));
            let kind = match previous["kind"].as_str() {
                Some("read") => ActivityKind::ReadingFiles,
                Some("edit" | "delete" | "move") => ActivityKind::EditingFiles,
                Some("search") => ActivityKind::Searching,
                Some("execute") => ActivityKind::Command,
                Some("think") => ActivityKind::Reasoning,
                Some("fetch") => ActivityKind::Searching,
                _ => ActivityKind::Tool,
            };
            let mut h = host.lock().unwrap();
            h.store.message(&item)?;
            let activities = h.activities.entry(task_id.into()).or_default();
            activities.retain(|activity| activity.id != key && activity.id != "devin-thinking");
            if !item.complete {
                activities.push(Activity {
                    id: key.clone(),
                    kind,
                    detail: title.chars().take(500).collect(),
                    started: now(),
                });
            }
            transcript.messages.insert(key, item);
        }
        Some("plan") => {
            let mut item = message(task_id, format!("{turn}:plan"), MessageKind::Notice);
            for entry in update["entries"].as_array().into_iter().flatten() {
                append_bounded(
                    &mut item.text,
                    &format!(
                        "{} · {}\n",
                        entry["status"].as_str().unwrap_or("pending"),
                        entry["content"].as_str().unwrap_or_default()
                    ),
                );
            }
            item.complete = true;
            host.lock().unwrap().store.message(&item)?;
        }
        Some("usage_update") => {
            if let Some(used) = update["used"].as_u64() {
                let h = host.lock().unwrap();
                let mut task = h.store.task(task_id)?;
                task.context_usage = Some(ContextUsage {
                    used,
                    window: update["size"].as_u64().filter(|size| *size > 0),
                    reported: now(),
                    cost: update["cost"]["amount"]
                        .as_f64()
                        .filter(|amount| amount.is_finite() && *amount >= 0.0)
                        .zip(update["cost"]["currency"].as_str())
                        .map(|(amount, currency)| UsageCost {
                            amount,
                            currency: currency.chars().take(12).collect(),
                        })
                        .or_else(|| {
                            task.context_usage
                                .as_ref()
                                .and_then(|usage| usage.cost.clone())
                        }),
                });
                h.save_task_with_notification(&task)?;
            }
        }
        Some("session_info_update") => {
            if let Some(title) = update["title"]
                .as_str()
                .filter(|title| !title.trim().is_empty())
            {
                let h = host.lock().unwrap();
                let mut task = h.store.task(task_id)?;
                if task.title_pending {
                    task.title = title.chars().take(120).collect();
                    task.title_pending = false;
                    h.store.save_task(&task)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn catalog_location(
    host: &Shared,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<(PathBuf, Option<mpsc::Sender<provider::Control>>)> {
    let h = host.lock().unwrap();
    if let Some(id) = task_id {
        let task = h.store.task(&id)?;
        return Ok((task.cwd, h.sessions.get(&id).cloned()));
    }
    if let Some(id) = project_id {
        let project = h
            .store
            .projects()?
            .into_iter()
            .find(|project| project.id == id)
            .context("Project is no longer available")?;
        return Ok((project.path, None));
    }
    Ok((std::env::current_dir()?, None))
}

/// Returns the selected session's advertised native commands and local provider skills.
pub(super) fn composer_catalog(
    host: &Shared,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<ComposerCatalog> {
    let (cwd, owner) = catalog_location(host, task_id, project_id)?;
    let mut catalog = if let Some(owner) = owner {
        let (reply, response) = mpsc::sync_channel(1);
        owner.send(provider::Control::Query {
            method: "kodem/catalog",
            params: Value::Null,
            reply,
        })?;
        let value = response
            .recv_timeout(Duration::from_secs(35))?
            .map_err(anyhow::Error::msg)?;
        serde_json::from_value(value)?
    } else {
        let mut session = Session::open(&cwd, None)?;
        let catalog = session.catalog();
        session
            .delete()
            .context("Could not delete Devin's empty command discovery session")?;
        catalog
    };
    match skills(&cwd) {
        Ok(skills) => catalog.skills = skills,
        Err(error) => catalog.errors.push(format!("Skills: {error:#}")),
    }
    Ok(catalog)
}

fn skills(cwd: &Path) -> Result<Vec<ComposerSkill>> {
    let output = Command::new(transport::executable())
        .args(["skills", "list", "--json"])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    ensure!(output.status.success(), "Could not read Devin skills");
    let entries: Value = serde_json::from_slice(&output.stdout)?;
    let mut skills = Vec::new();
    for skill in entries
        .as_array()
        .context("Devin returned an invalid skill inventory")?
    {
        let Some(directory) = skill["base_dir"]
            .as_str()
            .filter(|directory| !directory.is_empty())
        else {
            continue;
        };
        let path = Path::new(directory).join("SKILL.md");
        if path.is_file()
            && let Some(name) = skill["name"].as_str()
        {
            skills.push(ComposerSkill {
                name: name.into(),
                description: skill["description"].as_str().unwrap_or_default().into(),
                path,
            });
        }
    }
    Ok(skills)
}

/// Lists provider-owned skills and instructions without representing absent MCP introspection as success.
pub(super) fn resources(
    host: &Shared,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<Vec<Resource>> {
    let (cwd, _) = catalog_location(host, task_id, project_id)?;
    let mut resources = Vec::new();
    for directory in cwd.ancestors() {
        let path = directory.join("AGENTS.md");
        if path.is_file() {
            resources.push(Resource {
                name: "AGENTS.md".into(),
                source: path.display().to_string(),
                scope: "Project instructions".into(),
                status: "Discovered file".into(),
                detail: std::fs::read_to_string(path)?.chars().take(2000).collect(),
            });
        }
    }
    for skill in skills(&cwd)? {
        resources.push(Resource {
            name: skill.name,
            source: skill.path.display().to_string(),
            scope: "Devin skill".into(),
            status: "Provider reports available".into(),
            detail: skill.description,
        });
    }
    resources.push(Resource {
        name: "MCP servers".into(),
        source: "Devin ACP".into(),
        scope: "Provider-owned MCP".into(),
        status: "Managed by Devin CLI".into(),
        detail: "Use /mcp in the conversation to inspect the provider's current server status."
            .into(),
    });
    Ok(resources)
}
