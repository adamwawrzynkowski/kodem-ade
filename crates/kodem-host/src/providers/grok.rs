//! Grok Build sessions over the CLI's agent mode: ACP, JSON-RPC over standard input and output.
//!
//! One `grok agent stdio` process per task owner holds the task's session. Its account is fixed
//! when it starts; the model, the reasoning level, plan mode and always-approve follow the task
//! through ACP and the CLI's `x.ai/` extensions between turns. The task's standing instructions
//! and Kodem's MCP server are given when the session is created; a new process loads the same
//! session. The CLI asks the client before a tool call that its permission mode does not allow,
//! asks its questions and presents its plan through extension requests, and takes a message
//! during a turn as an interjection.
#[path = "grok_import.rs"]
mod import;
#[path = "grok_transcript.rs"]
mod transcript;
use crate::{
    Shared, accounts,
    orchestrator::{self, Input, Signal},
    provider,
};
use anyhow::{Context, Result, bail, ensure};
pub(crate) use import::{import_history, stored_thread, stored_threads};
use kodem_core::{platform, providers::grok, *};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime},
};
use transcript::Transcript;

/// Approval rules that Kodem offers for Grok Build, in the order a mode switch cycles through.
const ACCESS_MODES: [(&str, &str, &str); 4] = [
    (
        "default",
        "Ask before changes",
        "Asks before file edits, commands and tools that Grok's own permission rules do not already allow.",
    ),
    (
        "acceptEdits",
        "Accept edits",
        "Edits files in the working directory without asking. Still asks before commands and other tools.",
    ),
    (
        "plan",
        "Plan",
        "Grok's plan mode: explores and writes a plan, changes no project file, and asks you to approve the plan.",
    ),
    (
        "bypassPermissions",
        "Always approve",
        "Runs every tool call without asking. Grok's deny rules and hooks still apply. Use only in a checkout you can restore.",
    ),
];
/// The approval rule of a task that chose none.
const DEFAULT_ACCESS: &str = "default";
/// Reasoning levels in the order the picker lists them.
const EFFORT_ORDER: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];
/// Commands that a review or a task that reads its parent's checkout must not run.
const BLOCKED_COMMANDS: [&str; 11] = [
    "git push",
    "git commit",
    "gh pr comment",
    "gh pr review",
    "gh pr edit",
    "gh pr merge",
    "gh pr close",
    "gh pr create",
    "gh issue",
    "gh api",
    "git reset",
];
/// Slash commands of the CLI whose work Kodem's own controls do, so the composer does not
/// offer them: the session, its model and mode, its compaction and the sign-in.
const OWN_COMMANDS: [&str; 22] = [
    "compact",
    "always-approve",
    "auto",
    "plan",
    "model",
    "m",
    "effort",
    "context-window",
    "new",
    "clear",
    "resume",
    "fork",
    "rewind",
    "undo",
    "delete",
    "rename",
    "title",
    "quit",
    "exit",
    "home",
    "login",
    "logout",
];
/// A line of the protocol can embed a whole file; larger lines are dropped, not fatal.
const LINE_LIMIT: u64 = 32 * 1024 * 1024;
const DIAGNOSTIC_LIMIT: usize = 4096;
/// How long a stopped process may take to save its session before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);
/// How long a request that reads the account, the catalog or the usage may take.
const READ_LIMIT: Duration = Duration::from_secs(45);
/// How long the CLI may take to open or load a session.
const SESSION_LIMIT: Duration = Duration::from_secs(120);
/// How long a stored session must be quiet before a reconciliation reads it.
const QUIET: Duration = Duration::from_secs(15);
/// Ended turns whose late updates are recognized. Only the latest ones still send any.
const ENDED_TURNS: usize = 8;

/// Returns the capabilities that do not depend on the CLI, so saved choices validate early.
pub(super) fn initial_runtime() -> RuntimeInfo {
    RuntimeInfo {
        access_modes: ACCESS_MODES
            .iter()
            .map(|(id, name, description)| ProviderAccess {
                id: (*id).into(),
                name: (*name).into(),
                description: (*description).into(),
            })
            .collect(),
        steering: true,
        images: true,
        files: true,
        account_usage: true,
        usage_resets: false,
        ..RuntimeInfo::default()
    }
}

fn access_of(task: &Task) -> &str {
    task.provider_access.as_deref().unwrap_or(DEFAULT_ACCESS)
}

// A review, or a task that reads its parent's checkout, must not change files.
fn read_only(task: &Task) -> bool {
    task.kind == TaskKind::Review || task.shares_checkout()
}

/// Returns the CLI's home folder of an account.
fn home_of(account: &Account) -> Result<PathBuf> {
    grok::home(account.home.as_deref())
}

// The line of a CLI's diagnostics that explains a failure: the latest error, because the CLI
// also logs errors that it recovers from, such as an MCP server that did not start.
fn last_error(diagnostics: &str) -> String {
    diagnostics
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| {
            line.contains("ERROR") || line.starts_with("error:") || line.starts_with("Error:")
        })
        .or_else(|| {
            diagnostics
                .lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty())
        })
        .unwrap_or("no reason given")
        .chars()
        .take(400)
        .collect()
}

// The message of a JSON-RPC error, with its detail.
fn error_text(error: &Value) -> String {
    let message = error["message"]
        .as_str()
        .unwrap_or("Grok Build reported an error");
    // The CLI puts the cause of an internal error in the data, as `{"kind","message"}`.
    let data = match &error["data"] {
        Value::String(data) => serde_json::from_str(data).unwrap_or_else(|_| error["data"].clone()),
        data => data.clone(),
    };
    if let Some(cause) = data["message"].as_str() {
        return cause.to_owned();
    }
    match &error["data"] {
        Value::String(data) if !data.is_empty() => format!("{message}: {data}"),
        Value::Null => message.to_owned(),
        data => format!("{message}: {data}"),
    }
}

/// One running `grok agent stdio` process and its messages.
struct Connection {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: mpsc::Receiver<std::result::Result<Value, String>>,
    /// Messages that arrived while a call waited for its answer.
    held: VecDeque<Value>,
    diagnostics: Arc<Mutex<String>>,
    next: u64,
    account: Account,
    /// The automatic compaction rule that the process started with, for a task's session.
    compact: Option<CompactRule>,
    /// The CLI's answer to `initialize`, with its models.
    initialized: Value,
}

impl Connection {
    /// Starts the CLI in agent mode for an account and initializes the protocol. `compact` is
    /// the automatic compaction rule of a task's session; a probe keeps the CLI's own.
    fn start(cwd: &Path, account: &Account, compact: Option<CompactRule>) -> Result<Self> {
        let mut command = Command::new(grok::executable()?);
        platform::hide_console(&mut command);
        account.apply(&mut command);
        // The CLI reads the share of its context window at which it compacts, in percent, at
        // start. It has no switch that turns automatic compaction off, so a rule that is off
        // lets it compact only at a full window.
        if let Some(rule) = compact {
            let threshold = if rule.enabled { rule.threshold } else { 100 };
            command.env("GROK_AUTO_COMPACT_THRESHOLD_PERCENT", threshold.to_string());
        }
        command
            .current_dir(cwd)
            .args(["agent", "--no-leader", "stdio"]);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(
                "Could not start Grok Build. Install it and sign in from Providers settings",
            )?;
        let stdin = child.stdin.take().context("Missing Grok Build input")?;
        let stdout = child.stdout.take().context("Missing Grok Build output")?;
        let stderr = child
            .stderr
            .take()
            .context("Missing Grok Build diagnostics")?;
        let diagnostics = Arc::new(Mutex::new(String::new()));
        let kept = diagnostics.clone();
        // Keep only a short tail: it explains a failure and is never written to logs.
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = Vec::new();
            while reader
                .by_ref()
                .take(64 * 1024)
                .read_until(b'\n', &mut line)
                .is_ok_and(|read| read > 0)
            {
                let mut tail = kept.lock().unwrap();
                tail.push_str(&String::from_utf8_lossy(&line));
                if tail.len() > DIAGNOSTIC_LIMIT {
                    let mut cut = tail.len() - DIAGNOSTIC_LIMIT;
                    while !tail.is_char_boundary(cut) {
                        cut += 1;
                    }
                    tail.drain(..cut);
                }
                line.clear();
            }
        });
        let (sender, messages) = mpsc::sync_channel(1024);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                let result = reader
                    .by_ref()
                    .take(LINE_LIMIT + 1)
                    .read_until(b'\n', &mut bytes);
                let value = match result {
                    Ok(0) => Err("Grok Build ended its session".into()),
                    Ok(_) if bytes.len() as u64 > LINE_LIMIT && !bytes.ends_with(b"\n") => {
                        // Skip the rest of an oversized message and keep the session alive.
                        let mut rest = Vec::new();
                        loop {
                            rest.clear();
                            match reader
                                .by_ref()
                                .take(1024 * 1024)
                                .read_until(b'\n', &mut rest)
                            {
                                Ok(0) => break,
                                Ok(_) if rest.ends_with(b"\n") => break,
                                Ok(_) => {}
                                Err(_) => break,
                            }
                        }
                        continue;
                    }
                    Ok(_) => match serde_json::from_slice::<Value>(&bytes) {
                        Ok(value) if value.is_object() => Ok(value),
                        _ => continue,
                    },
                    Err(error) => Err(error.to_string()),
                };
                let failed = value.is_err();
                if sender.send(value).is_err() || failed {
                    break;
                }
            }
        });
        let mut connection = Self {
            child,
            stdin: Some(stdin),
            messages,
            held: VecDeque::new(),
            diagnostics,
            next: 0,
            account: account.clone(),
            compact,
            initialized: Value::Null,
        };
        connection.initialized = connection.call(
            "initialize",
            json!({"protocolVersion":1,
                "clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},
                    "terminal":false},
                "clientInfo":{"name":"kodem","version":env!("CARGO_PKG_VERSION")}}),
            READ_LIMIT,
            false,
        )?;
        Ok(connection)
    }

    fn send(&mut self, value: &Value) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("Grok Build no longer reads input")?;
        serde_json::to_writer(&mut *stdin, value)?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        Ok(())
    }

    /// Writes a request and returns its identity; its answer arrives as a message.
    fn request(&mut self, method: &str, params: Value) -> Result<u64> {
        self.next += 1;
        let id = self.next;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        Ok(id)
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(&json!({"jsonrpc":"2.0","method":method,"params":params}))
    }

    fn respond(&mut self, id: &Value, result: Value) -> Result<()> {
        self.send(&json!({"jsonrpc":"2.0","id":id,"result":result}))
    }

    fn reject(&mut self, id: &Value, message: &str) -> Result<()> {
        self.send(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":message}}))
    }

    /// Takes the next message, first those that a call held.
    fn next_message(&mut self, wait: Duration) -> Result<Option<Value>> {
        if let Some(message) = self.held.pop_front() {
            return Ok(Some(message));
        }
        match self.messages.recv_timeout(wait) {
            Ok(Ok(message)) => Ok(Some(message)),
            Ok(Err(error)) => bail!("{error}"),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("Grok Build ended its session"),
        }
    }

    /// Sends a request and waits for its answer. Other messages wait for the owner, except the
    /// updates that a session load replays when `replay` is set: they hold the stored
    /// conversation, which Kodem already shows.
    fn call(
        &mut self,
        method: &str,
        params: Value,
        limit: Duration,
        replay: bool,
    ) -> Result<Value> {
        let id = self.request(method, params)?;
        let deadline = Instant::now() + limit;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            ensure!(!left.is_zero(), "Grok Build did not answer {method}");
            let message = match self
                .messages
                .recv_timeout(left.min(Duration::from_millis(200)))
            {
                Ok(Ok(message)) => message,
                Ok(Err(error)) => bail!("{error}{}", self.reason_suffix()),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("Grok Build ended its session{}", self.reason_suffix())
                }
            };
            if message["id"] == id && message.get("method").is_none() {
                if let Some(error) = message.get("error") {
                    bail!("{}", error_text(error));
                }
                return Ok(message["result"].clone());
            }
            let update = message["params"]["sessionId"].is_string() && message.get("id").is_none();
            if replay && update {
                continue;
            }
            self.held.push_back(message);
        }
    }

    // The CLI's own explanation of a failure, from its diagnostics.
    fn reason(&self) -> Option<String> {
        let tail = self.diagnostics.lock().unwrap().clone();
        let line = last_error(&tail);
        (line != "no reason given").then_some(line)
    }

    fn reason_suffix(&self) -> String {
        self.reason()
            .map(|reason| format!(": {reason}"))
            .unwrap_or_default()
    }

    fn running(&mut self) -> bool {
        self.child.try_wait().is_ok_and(|status| status.is_none())
    }

    /// Ends the process: closing the input lets the CLI save its sessions and exit.
    fn stop(&mut self) {
        self.stdin = None;
        let deadline = Instant::now() + STOP_GRACE;
        while self.running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Builds the catalog from the models that the CLI reported when it initialized.
fn runtime(initialized: &Value) -> RuntimeInfo {
    let state = &initialized["_meta"]["modelState"];
    let models: Vec<Model> = state["availableModels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let id = model["modelId"].as_str()?;
            let meta = &model["_meta"];
            let listed = meta["reasoningEfforts"].as_array();
            let mut efforts: Vec<String> = listed
                .into_iter()
                .flatten()
                .filter(|_| meta["supportsReasoningEffort"] == true)
                .filter_map(|effort| effort["value"].as_str().map(str::to_owned))
                .collect();
            efforts.sort_by_key(|effort| EFFORT_ORDER.iter().position(|known| known == effort));
            let default_effort = listed
                .into_iter()
                .flatten()
                .find(|effort| effort["default"] == true)
                .and_then(|effort| effort["value"].as_str())
                .or_else(|| meta["reasoningEffort"].as_str())
                .filter(|effort| efforts.iter().any(|known| known == effort))
                .or_else(|| {
                    efforts
                        .iter()
                        .map(String::as_str)
                        .find(|effort| *effort == "high")
                })
                .or_else(|| efforts.last().map(String::as_str))
                .unwrap_or_default()
                .to_owned();
            let name = provider::model_name(model["name"].as_str().unwrap_or(id));
            Some(Model {
                id: id.to_owned(),
                name: if name.is_empty() { id.to_owned() } else { name },
                efforts,
                default_effort,
                images: true,
                effort_models: Default::default(),
                fast: None,
            })
        })
        .collect();
    let own = state["currentModelId"].as_str();
    RuntimeInfo {
        default_model: own
            .filter(|own| models.iter().any(|model| model.id == *own))
            .map(str::to_owned)
            .or_else(|| models.first().map(|model| model.id.clone())),
        version: initialized["_meta"]["agentVersion"]
            .as_str()
            .map(|version| format!("Grok Build {version}")),
        models,
        ..initial_runtime()
    }
}

// The context window of a model, as the CLI reported it.
fn window_of(initialized: &Value, model: Option<&str>) -> Option<u64> {
    initialized["_meta"]["modelState"]["availableModels"]
        .as_array()?
        .iter()
        .find(|known| known["modelId"].as_str() == model)?["_meta"]["totalContextTokens"]
        .as_u64()
        .filter(|window| *window > 0)
}

fn publish(host: &Shared, initialized: &Value) {
    let info = runtime(initialized);
    if info.models.is_empty() {
        return;
    }
    host.lock().unwrap().runtimes.insert(Provider::Grok, info);
}

// The sign-in state that `x.ai/auth/check_subscription` reports.
fn identity(subscription: &Value) -> AccountStatus {
    let meta = &subscription["meta"];
    let signed_in = subscription["authenticated"] == true;
    AccountStatus {
        signed_in: Some(signed_in),
        email: meta["email"]
            .as_str()
            .filter(|_| signed_in)
            .map(str::to_owned),
        plan: meta["subscription_tier"]
            .as_str()
            .filter(|_| signed_in)
            .map(str::to_owned),
        organization: meta["team_name"].as_str().map(str::to_owned),
        ..AccountStatus::default()
    }
}

// An amount of the billing report, which counts in cents.
fn cents(value: &Value) -> Option<f64> {
    value["val"].as_f64()
}

/// Builds the account's limits from `x.ai/billing`. The account's allowance is one window of
/// the billing period, weekly or monthly.
fn usage_report(billing: &Value) -> UsageLimits {
    let config = &billing["config"];
    let period = &config["currentPeriod"];
    let start = period["start"]
        .as_str()
        .or_else(|| config["billingPeriodStart"].as_str())
        .and_then(unix_seconds);
    let end = period["end"]
        .as_str()
        .or_else(|| config["billingPeriodEnd"].as_str())
        .and_then(unix_seconds);
    let limit = cents(&config["monthlyLimit"]).filter(|limit| *limit > 0.0);
    let percent = config["creditUsagePercent"].as_f64().or_else(|| {
        let used = cents(&config["used"])?;
        Some(used / limit? * 100.0)
    });
    let primary = percent.map(|percent| UsageWindow {
        used_percent: percent.clamp(0.0, 100.0).round() as i32,
        window_duration_mins: start
            .zip(end)
            .filter(|(start, end)| end > start)
            .map(|(start, end)| (end - start) / 60),
        resets_at: end,
        name: None,
    });
    let reached = primary
        .as_ref()
        .is_some_and(|window| window.used_percent >= 100)
        .then(|| "credits".to_owned());
    let cap = cents(&config["onDemandCap"]).filter(|cap| *cap > 0.0);
    let spend = cap.map(|cap| {
        let used = cents(&config["onDemandUsed"]).unwrap_or(0.0);
        UsageSpend {
            used: Some(used),
            limit: Some(cap),
            used_percent: Some((used / cap * 100.0).clamp(0.0, 100.0).round() as i32),
            currency: Some("USD".into()),
        }
    });
    let balance = cents(&config["prepaidBalance"]).filter(|balance| *balance > 0.0);
    UsageLimits {
        plan_type: billing["subscription_tier"].as_str().map(str::to_owned),
        primary,
        credits: balance.map(|balance| UsageCredits {
            has_credits: true,
            unlimited: false,
            balance: Some(format!("${:.2}", balance / 100.0)),
        }),
        rate_limit_reached_type: reached,
        spend,
        reported: now(),
        ..UsageLimits::default()
    }
}

/// What a probe of the CLI read: its catalog, the account's sign-in and its usage.
struct Probe {
    initialized: Value,
    status: AccountStatus,
}

// Starts the CLI for an account without a session and reads its sign-in, and its usage when
// requested and signed in.
fn probe(account: &Account, cwd: &Path, usage: bool) -> Result<Probe> {
    let mut connection = Connection::start(cwd, account, None)?;
    let subscription = connection.call(
        "_x.ai/auth/check_subscription",
        json!({}),
        READ_LIMIT,
        false,
    )?;
    let mut status = identity(&subscription);
    if usage && status.signed_in == Some(true) {
        match connection.call("_x.ai/billing", json!({}), READ_LIMIT, false) {
            Ok(billing) => status.usage = Some(usage_report(&billing)),
            Err(error) => status.error = Some(format!("{error:#}")),
        }
    }
    Ok(Probe {
        initialized: connection.initialized.clone(),
        status,
    })
}

/// Reads the models of the account and its sign-in, without a session.
pub(super) fn discover(host: Shared, account: &Account, cwd: Option<PathBuf>) -> Result<()> {
    let cwd = match cwd {
        Some(cwd) => cwd,
        None => home_of(account)?,
    };
    let probe = probe(account, &cwd, false)?;
    publish(&host, &probe.initialized);
    let mut h = host.lock().unwrap();
    let mut status = probe.status;
    if let Some(known) = h.accounts.get(&(Provider::Grok, account.id.clone())) {
        status.usage = known.usage.clone();
    }
    accounts::observe(&mut h, account, status);
    Ok(())
}

/// Reads whether the account is signed in, and its usage when requested.
pub(super) fn account_status(
    host: &Shared,
    account: &Account,
    usage: bool,
) -> Result<AccountStatus> {
    let probe = probe(account, &home_of(account)?, usage)?;
    publish(host, &probe.initialized);
    Ok(probe.status)
}

/// Reads the account's usage again; the CLI has no banked resets to redeem.
pub(super) fn account_usage(
    host: &Shared,
    account: &Account,
    idempotency_key: Option<&str>,
) -> Result<Response> {
    ensure!(
        idempotency_key.is_none(),
        "Grok Build does not offer banked usage resets"
    );
    let mut connection = Connection::start(&home_of(account)?, account, None)?;
    let billing = connection.call("_x.ai/billing", json!({}), READ_LIMIT, false)?;
    let usage = usage_report(&billing);
    accounts::set_usage(
        &mut host.lock().unwrap(),
        Provider::Grok,
        &account.id,
        usage.clone(),
    );
    Ok(Response::Usage {
        provider: Provider::Grok,
        account: account.id.clone(),
        usage,
    })
}

fn location(host: &Shared, task_id: Option<String>, project_id: Option<String>) -> Result<PathBuf> {
    let h = host.lock().unwrap();
    if let Some(id) = task_id {
        return Ok(h.store.task(&id)?.cwd);
    }
    if let Some(id) = project_id {
        let project = h
            .store
            .projects()?
            .into_iter()
            .find(|project| project.id == id)
            .context("Project is no longer available")?;
        return Ok(project.path);
    }
    platform::home_dir()
}

// The skills that the CLI finds for a folder, as the composer attaches them.
fn skills(connection: &mut Connection, cwd: &Path) -> Result<Vec<ComposerSkill>> {
    let listed = connection.call("_x.ai/skills/list", json!({"cwd":cwd}), READ_LIMIT, false)?;
    let mut skills: Vec<ComposerSkill> = listed["result"]["skills"]
        .as_array()
        .or_else(|| listed["skills"].as_array())
        .into_iter()
        .flatten()
        .take(1000)
        .filter(|skill| skill["enabled"] != false)
        .filter_map(|skill| {
            let path = PathBuf::from(skill["path"].as_str()?);
            path.is_file().then(|| ComposerSkill {
                name: skill["name"].as_str().unwrap_or_default().to_owned(),
                description: skill["description"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(600)
                    .collect(),
                path,
            })
        })
        .filter(|skill| !skill.name.is_empty())
        .collect();
    skills.sort_by_key(|skill| skill.name.to_lowercase());
    Ok(skills)
}

// The CLI's own slash commands for a folder, without skills and without the commands whose
// work Kodem's controls do.
fn commands(connection: &mut Connection, cwd: &Path) -> Result<Vec<ProviderCommand>> {
    let listed = connection.call("_x.ai/commands/list", json!({"cwd":cwd}), READ_LIMIT, false)?;
    Ok(listed["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .take(1000)
        .filter(|command| command["_meta"]["path"].is_null())
        .filter_map(|command| {
            let name = command["name"].as_str()?;
            (!OWN_COMMANDS.contains(&name)).then(|| ProviderCommand {
                name: name.to_owned(),
                description: command["description"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(600)
                    .collect(),
                input_hint: command["input"]["hint"]
                    .as_str()
                    .filter(|hint| !hint.is_empty())
                    .map(str::to_owned),
            })
        })
        .collect())
}

/// Returns the CLI's commands and skills for the checkout.
pub(super) fn composer_catalog(
    host: &Shared,
    account: &Account,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<ComposerCatalog> {
    // Only a session that exists can be compacted.
    let started = match &task_id {
        Some(id) => host
            .lock()
            .unwrap()
            .store
            .task(id)?
            .provider_thread
            .is_some(),
        None => false,
    };
    let cwd = location(host, task_id, project_id)?;
    let mut catalog = ComposerCatalog {
        commands: vec![
            ComposerCommand::Provider,
            ComposerCommand::New,
            ComposerCommand::Model,
            ComposerCommand::Permissions,
            ComposerCommand::Skills,
            ComposerCommand::Mcp,
            ComposerCommand::Status,
        ],
        ..ComposerCatalog::default()
    };
    if started {
        catalog.commands.push(ComposerCommand::Compact);
    }
    let mut connection = Connection::start(&cwd, account, None)?;
    match commands(&mut connection, &cwd) {
        Ok(commands) => catalog.provider_commands = commands,
        Err(error) => catalog.errors.push(format!("{error:#}")),
    }
    match skills(&mut connection, &cwd) {
        Ok(skills) => catalog.skills = skills,
        Err(error) => catalog.errors.push(format!("{error:#}")),
    }
    Ok(catalog)
}

// Reads the first part of an instruction file for the inventory.
fn excerpt(path: &Path) -> String {
    let mut detail = String::new();
    let _ = std::fs::File::open(path).and_then(|file| file.take(8192).read_to_string(&mut detail));
    detail.chars().take(2000).collect()
}

// The names of the MCP servers that a configuration file of the CLI defines, from its
// `[mcp_servers.<name>]` tables.
fn configured_servers(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let table = line
                .trim()
                .strip_prefix("[mcp_servers.")?
                .strip_suffix(']')?;
            let name = table.split('.').next()?.trim_matches('"');
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

/// Lists instruction files, rules, skills and the MCP servers of the CLI's configuration.
pub(super) fn resources(
    host: &Shared,
    account: &Account,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<Vec<Resource>> {
    let cwd = location(host, task_id, project_id)?;
    let home = home_of(account)?;
    let mut resources = Vec::new();
    let mut files: Vec<(PathBuf, &str)> = Vec::new();
    let names = ["AGENTS.md", "AGENT.md", "CLAUDE.md", "CLAUDE.local.md"];
    for folder in cwd.ancestors() {
        for name in names {
            files.push((folder.join(name), "Project instructions"));
        }
    }
    files.push((home.join("AGENTS.md"), "User instructions"));
    for (folder, scope) in [
        (cwd.join(".grok").join("rules"), "Project rule"),
        (home.join("rules"), "User rule"),
    ] {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut rules: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
            .collect();
        rules.sort();
        files.extend(rules.into_iter().take(200).map(|path| (path, scope)));
    }
    for (path, scope) in files {
        if path.is_file() {
            resources.push(Resource {
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                source: path.display().to_string(),
                scope: scope.into(),
                status: "Discovered file".into(),
                detail: excerpt(&path),
            });
        }
    }
    let listed = Connection::start(&cwd, account, None)
        .and_then(|mut connection| skills(&mut connection, &cwd));
    match listed {
        Ok(skills) => {
            for skill in skills {
                resources.push(Resource {
                    name: skill.name,
                    source: skill.path.display().to_string(),
                    scope: "Grok skill".into(),
                    status: "Discovered file".into(),
                    detail: skill.description,
                });
            }
        }
        Err(error) => resources.push(Resource {
            name: "Skills".into(),
            source: "Grok Build".into(),
            scope: "Grok skill".into(),
            status: "Unavailable".into(),
            detail: format!("{error:#}"),
        }),
    }
    let mut configs = vec![(home.join("config.toml"), "User MCP")];
    configs.extend(
        cwd.ancestors()
            .map(|folder| (folder.join(".grok").join("config.toml"), "Project MCP")),
    );
    for (path, scope) in configs {
        for name in configured_servers(&path) {
            resources.push(Resource {
                name,
                source: path.display().to_string(),
                scope: scope.into(),
                status: "Configured".into(),
                detail: "This inventory is not a tool execution check.".into(),
            });
        }
    }
    Ok(resources)
}

/// Reports a command that a review must not run, also as one part of a command list.
fn blocked(command: &str) -> bool {
    command
        .split(['&', '|', ';', '\n'])
        .map(str::trim)
        .any(|part| {
            BLOCKED_COMMANDS.iter().any(|blocked| {
                part.strip_prefix(blocked)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
            })
        })
}

/// Reports a path inside one of the folders.
fn inside(path: &str, folders: &[PathBuf]) -> bool {
    let path = Path::new(path);
    path.is_absolute() && folders.iter().any(|folder| path.starts_with(folder))
}

/// What Kodem does with one tool call that the CLI asks about.
enum Verdict {
    Allow,
    Deny(String),
    Ask,
}

/// Decides on one tool call by the task's approval rule and kind.
fn verdict(task: &Task, call: &Value, roots: &[PathBuf]) -> Verdict {
    let kind = transcript::tool_kind(call);
    let name = transcript::tool_name(call);
    let input = &call["rawInput"];
    let category = transcript::category(kind);
    let access = access_of(task);
    let review = read_only(task);
    if category == ToolCategory::Edit {
        if review {
            return Verdict::Deny("This task reviews changes and must not edit files.".into());
        }
        if access == "acceptEdits" && inside(transcript::call_path(input), roots) {
            return Verdict::Allow;
        }
    }
    if category == ToolCategory::Command {
        let command = transcript::call_command(input);
        if review && blocked(command) {
            return Verdict::Deny(format!("A review must not run `{command}`."));
        }
    }
    // Kodem's own tools start tasks that the user sees and can stop.
    if name == "use_tool"
        && input["tool_name"]
            .as_str()
            .is_some_and(|tool| tool.starts_with(&format!("{}__", crate::mcp::SERVER)))
    {
        return Verdict::Allow;
    }
    // Searching and reading pages is what a chat is for.
    if category == ToolCategory::Fetch && task.kind == TaskKind::Chat {
        return Verdict::Allow;
    }
    if access == "bypassPermissions" && !review {
        return Verdict::Allow;
    }
    Verdict::Ask
}

// The option of a permission request with one of the kinds, such as `allow_once`.
fn option<'a>(options: &'a [Value], kinds: &[&str]) -> Option<&'a Value> {
    kinds.iter().find_map(|kind| {
        options
            .iter()
            .find(|option| option["kind"].as_str() == Some(*kind))
    })
}

// The answer that selects an option of a permission request, or cancels it when it has none.
fn select(options: &[Value], kinds: &[&str], followup: Option<&str>) -> Value {
    let Some(id) = option(options, kinds).and_then(|option| option["optionId"].as_str()) else {
        return json!({"outcome":{"outcome":"cancelled"}});
    };
    let mut answer = json!({"outcome":{"outcome":"selected","optionId":id}});
    if let Some(followup) = followup {
        answer["_meta"] = json!({"followup_message":followup});
    }
    answer
}

/// What a request of the CLI that waits for the user is about.
enum AskKind {
    /// A tool call, with the options that the CLI offers for it.
    Permission { options: Vec<Value> },
    /// The questions of an `ask_user_question` call.
    Question { questions: Vec<Value> },
    /// A plan that `exit_plan_mode` presents.
    Plan,
}

/// A request of the CLI that waits for the user.
struct Ask {
    kind: AskKind,
    /// The JSON-RPC identity that the CLI waits on.
    rpc: Value,
    /// The subagent session that asked; absent for the task's own.
    agent: Option<String>,
}

/// The answer of the CLI's question request from the user's answers in Kodem. A typed answer
/// that names no option is the free text of the question.
fn question_answer(questions: &[Value], response: &Value) -> Value {
    let mut answers = serde_json::Map::new();
    let mut annotations = serde_json::Map::new();
    for (index, question) in questions.iter().enumerate() {
        let Some(text) = question["question"].as_str() else {
            continue;
        };
        let labels: Vec<&str> = question["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|option| option["label"].as_str())
            .collect();
        let chosen: Vec<&str> = response["answers"][index.to_string()]["answers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|answer| !answer.is_empty())
            .collect();
        if chosen.is_empty() {
            continue;
        }
        let mut picked: Vec<&str> = chosen
            .iter()
            .copied()
            .filter(|answer| labels.contains(answer))
            .collect();
        let typed: Vec<&str> = chosen
            .iter()
            .copied()
            .filter(|answer| !labels.contains(answer))
            .collect();
        if !typed.is_empty() {
            annotations.insert(text.to_owned(), json!({"notes":typed.join("\n")}));
            if picked.is_empty() {
                picked.push("Other");
            }
        }
        answers.insert(text.to_owned(), json!(picked));
    }
    let mut answer = json!({"outcome":"accepted","answers":answers});
    if !annotations.is_empty() {
        answer["annotations"] = Value::Object(annotations);
    }
    answer
}

/// Resolves uncertain delivery from the session that the CLI stored.
///
/// A stored prompt holds no identity of Kodem's, so the proof is the latest stored prompt that
/// starts with the exact text of the submission.
pub(super) fn reconcile(host: &Shared, task_id: &str) -> Result<()> {
    let (task, account, submission) = {
        let h = host.lock().unwrap();
        ensure!(
            !h.sessions.contains_key(task_id),
            "The task still has a live transport owner"
        );
        let task = h.store.task(task_id)?;
        ensure!(
            task.state == RunState::DeliveryUnknown || h.store.has_uncertain_submission(task_id)?,
            "This task has no uncertain delivery to reconcile"
        );
        let account = accounts::for_task(&h, &task)?;
        (
            task,
            account,
            h.store
                .latest_submission(task_id)?
                .context("No recorded outgoing submission to reconcile")?,
        )
    };
    let session = task.provider_thread.clone().context("Grok Build never confirmed a session for this task, so the prompt cannot be reconciled automatically. Retain this task and create a new one.")?;
    let home = home_of(&account)?;
    let summary =
        grok::summary(&home, &session)?.context("Grok Build no longer has this session")?;
    let path = summary.updates();
    let idle = std::fs::metadata(&path)?
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok());
    ensure!(
        idle.is_some_and(|idle| idle >= QUIET),
        "Grok Build may still be writing this session. Reconcile again in a moment."
    );
    let updates = transcript::read_updates(&path, usize::MAX);
    let prompt = submission.prompt.trim();
    let receipt = updates
        .iter()
        .rposition(|line| {
            let update = &line["params"]["update"];
            update["sessionUpdate"] == "user_message_chunk"
                && transcript::user_text(update).starts_with(prompt)
        })
        .context("The Grok Build session does not contain this exact submission. Delivery remains unknown; no text was resent.")?;
    let finished = updates[receipt + 1..]
        .iter()
        .rev()
        .find(|line| line["params"]["update"]["sessionUpdate"] == "turn_completed")
        .is_some_and(|line| line["params"]["update"]["stop_reason"] == "end_turn");
    let mut transcript = Transcript::new(task_id, &task.cwd, &home, true);
    transcript.set_main(&session);
    transcript.replay(host, &path, Some(receipt))?;
    transcript.finish(host)?;
    {
        let h = host.lock().unwrap();
        h.store
            .confirm_submission(&submission.id, &format!("grok:{session}:{receipt}"))?;
        let mut current = h.store.task(task_id)?;
        if submission.outgoing.is_none() && current.draft == submission.prompt {
            current.draft.clear();
        }
        current
            .feedback
            .retain(|feedback| !submission.feedback.contains(&feedback.id));
        for agent in &mut current.subagents {
            if agent.state.active() || agent.state == RunState::DeliveryUnknown {
                agent.state = RunState::Interrupted;
                agent.active_turn = None;
            }
        }
        h.store.save_task(&current)?;
    }
    let (state, error) = if finished {
        (RunState::Completed, None)
    } else {
        (
            RunState::Interrupted,
            Some("The connection to Grok Build was lost during this turn. The output it stored was restored; send a message to continue.".to_owned()),
        )
    };
    provider::complete_turn(host, task_id, state, error)?;
    let h = host.lock().unwrap();
    if h.store.has_uncertain_submission(task_id)? {
        let mut current = h.store.task(task_id)?;
        current.state = RunState::DeliveryUnknown;
        current.error = Some(
            "Other outgoing input still has uncertain delivery. Reconcile again before resuming."
                .into(),
        );
        h.save_task_with_notification(&current)?;
    }
    Ok(())
}

/// The choices that the open session runs with, as Kodem last set them.
#[derive(Default)]
struct Applied {
    model: Option<String>,
    effort: Option<String>,
    plan: Option<bool>,
    yolo: Option<bool>,
}

struct Owner<'a> {
    host: &'a Shared,
    task_id: &'a str,
    cwd: PathBuf,
    home: PathBuf,
    connection: Option<Connection>,
    /// The CLI's session of the task in the running process.
    session: Option<String>,
    /// The CLI took a prompt of this session, so later processes can load it.
    stored: bool,
    applied: Applied,
    transcript: Transcript,
    /// A turn of the task runs, whether a submission or the CLI itself started it.
    running: bool,
    /// The `session/prompt` request of the running turn.
    prompt: Option<u64>,
    /// The CLI's identity of the running turn, once it reported one.
    turn: Option<String>,
    /// The CLI's identities of the latest turns that ended, so a late update starts nothing.
    ended: VecDeque<String>,
    /// The submission sent for the running turn until the CLI reports that it took it.
    sent: Option<(String, String)>,
    /// Messages sent during a turn until the CLI reports that it took them, by submission.
    steers: HashMap<String, String>,
    follow_ups: u64,
    interrupting: bool,
    asks: HashMap<String, Ask>,
    /// The `x.ai/compact_conversation` request of a compaction that the host asked for.
    compacting: Option<u64>,
    /// The approval rule that a task takes when its approved plan ends plan mode.
    after_plan: Option<&'static str>,
}

/// Owns one resumable Grok Build session and keeps controls responsive while it works.
pub(super) fn run(
    host: Shared,
    task_id: &str,
    controls: mpsc::Receiver<provider::Control>,
) -> Result<()> {
    let (task, account) = {
        let h = host.lock().unwrap();
        let task = h.store.task(task_id)?;
        let account = accounts::for_task(&h, &task)?;
        (task, account)
    };
    let home = home_of(&account)?;
    let mut owner = Owner {
        host: &host,
        task_id,
        cwd: task.cwd.clone(),
        transcript: Transcript::new(task_id, &task.cwd, &home, false),
        home,
        connection: None,
        session: None,
        stored: task.provider_thread.is_some(),
        applied: Applied::default(),
        running: false,
        prompt: None,
        turn: None,
        ended: VecDeque::new(),
        sent: None,
        steers: HashMap::new(),
        follow_ups: 0,
        interrupting: false,
        asks: HashMap::new(),
        compacting: None,
        after_plan: None,
    };
    if let Some(session) = &task.provider_thread {
        owner.transcript.set_main(session);
    }
    // Each turn reports its own cost, which adds to what the task's earlier turns cost.
    owner.transcript.cost = task
        .context_usage
        .as_ref()
        .and_then(|usage| usage.cost.as_ref())
        .filter(|cost| cost.currency == "USD")
        .map(|cost| cost.amount);
    owner.serve(&controls, &task)
}

impl Owner<'_> {
    fn serve(&mut self, controls: &mpsc::Receiver<provider::Control>, task: &Task) -> Result<()> {
        let mut branch_refresh = Instant::now();
        loop {
            while let Ok(control) = controls.try_recv() {
                match control {
                    provider::Control::Close => return Ok(()),
                    provider::Control::Wake => {}
                    provider::Control::Interrupt => self.interrupt()?,
                    provider::Control::Compact => self.compact()?,
                    provider::Control::Answer { id, response } => self.answer(id, response)?,
                    provider::Control::Query { reply, .. } => {
                        let _ =
                            reply.send(Err("This operation is not supported by Grok Build".into()));
                    }
                }
            }
            self.receive()?;
            self.dispatch()?;
            self.transcript.write_due(self.host)?;
            if branch_refresh.elapsed() >= Duration::from_secs(5) {
                branch_refresh = Instant::now();
                // Only a coding task follows the branch of its checkout.
                if self.running && task.kind == TaskKind::Coding {
                    let branch = kodem_core::git::branch(&task.cwd);
                    let h = self.host.lock().unwrap();
                    let mut task = h.store.task(self.task_id)?;
                    if task.branch != branch {
                        task.branch = branch;
                        h.save_task_with_notification(&task)?;
                    }
                }
            }
        }
    }

    fn task(&self) -> Result<Task> {
        self.host.lock().unwrap().store.task(self.task_id)
    }

    // Takes the next message of the process, or learns that it ended.
    fn receive(&mut self) -> Result<()> {
        let Some(connection) = &mut self.connection else {
            std::thread::sleep(Duration::from_millis(40));
            return Ok(());
        };
        let message = match connection.next_message(Duration::from_millis(40)) {
            Ok(Some(message)) => message,
            Ok(None) => return Ok(()),
            Err(_) => return self.ended(),
        };
        let method = message["method"].as_str().map(str::to_owned);
        match (method, message.get("id").cloned()) {
            (Some(method), Some(id)) => self.request(&method, id, &message["params"]),
            (Some(method), None) => self.notification(&method, &message["params"]),
            (None, Some(id)) => self.response(&id, &message),
            (None, None) => Ok(()),
        }
    }

    // The process ended. During a turn whose prompt the CLI took, the turn failed; before that,
    // the prompt's delivery is unknown and the owner ends, so the host marks it so.
    fn ended(&mut self) -> Result<()> {
        let reason = self.connection.as_ref().and_then(Connection::reason);
        self.connection = None;
        self.session = None;
        if self.compacting.take().is_some() {
            provider::end_compaction(
                self.host,
                self.task_id,
                Some(reason.clone().unwrap_or_else(|| "Grok Build ended".into())),
            )?;
        }
        // A message for the turn that the CLI did not answer may or may not be in it.
        if !self.steers.is_empty() {
            bail!(
                "Grok Build ended before it answered a message{}",
                reason
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            );
        }
        if !self.running {
            // Agents that worked in the background after the turn ended with the process.
            self.transcript
                .stop_agents(self.host, RunState::Interrupted)?;
            return self.agents_changed();
        }
        if self.sent.is_some() && !self.interrupting {
            bail!(
                "Grok Build ended before it took the message{}",
                reason
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            );
        }
        let outcome = if self.interrupting {
            TurnOutcome {
                state: RunState::Interrupted,
                error: None,
            }
        } else {
            TurnOutcome {
                state: RunState::Failed,
                error: Some(reason.unwrap_or_else(|| "Grok Build ended during the turn".into())),
            }
        };
        self.end_turn(outcome)
    }

    // A notification of the CLI: a session update, a turn that ended, or a message it took.
    fn notification(&mut self, method: &str, params: &Value) -> Result<()> {
        match method {
            "session/update" | "_x.ai/session_notification" | "_x.ai/session/update" => {
                let main = params["sessionId"].as_str() == self.session.as_deref();
                if main && let Some(prompt) = params["_meta"]["promptId"].as_str() {
                    self.prompt_seen(prompt)?;
                }
                self.transcript.update(self.host, params)?;
                self.reported()?;
                self.agents_changed()?;
            }
            "_x.ai/queue/changed" => {
                if params["sessionId"].as_str() == self.session.as_deref()
                    && let Some(prompt) = params["runningPromptId"].as_str()
                {
                    self.prompt_seen(prompt)?;
                }
            }
            "_x.ai/session/interjection" => {
                if let Some(submission) = params["interjectionId"]
                    .as_str()
                    .and_then(|id| self.steers.remove(id).map(|_| id.to_owned()))
                {
                    orchestrator::dispatch(
                        self.host,
                        self.task_id,
                        Input::Signal(Signal::Delivered {
                            turn: format!("grok:{submission}"),
                            submission,
                            steers: true,
                        }),
                    )?;
                }
            }
            "_x.ai/session/prompt_complete" => {
                let prompt = params["promptId"].as_str();
                let ours = params["sessionId"].as_str() == self.session.as_deref()
                    && (self.turn.is_none() || prompt == self.turn.as_deref());
                if self.running && ours {
                    let reason = params["stopReason"].as_str().unwrap_or_default();
                    let outcome = self.outcome(reason, None);
                    self.end_turn(outcome)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // The CLI works on one of the task's turns: the prompt that Kodem sent, or a turn that it
    // started by itself, such as one that a message left after the turn it was sent to.
    fn prompt_seen(&mut self, prompt: &str) -> Result<()> {
        if self.ended.iter().any(|ended| ended == prompt) {
            return Ok(());
        }
        if self.running {
            if self.turn.is_none() {
                self.turn = Some(prompt.to_owned());
                self.delivered()?;
            }
            return Ok(());
        }
        if self.compacting.is_some() {
            return Ok(());
        }
        self.running = true;
        self.turn = Some(prompt.to_owned());
        self.follow_ups += 1;
        // This local identity links steering to the turn; it is never evidence for
        // reconciliation.
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::TurnStarted {
                turn: Some(format!("grok:follow-up:{}", self.follow_ups)),
                branch: None,
            }),
        )?;
        Ok(())
    }

    // The CLI took the prompt of the running turn.
    fn delivered(&mut self) -> Result<()> {
        let Some((submission, turn)) = self.sent.take() else {
            return Ok(());
        };
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::Delivered {
                submission,
                turn,
                steers: false,
            }),
        )?;
        if !self.stored
            && let Some(session) = &self.session
        {
            self.stored = true;
            let h = self.host.lock().unwrap();
            let mut task = h.store.task(self.task_id)?;
            task.provider_thread = Some(session.clone());
            h.store.save_task(&task)?;
        }
        Ok(())
    }

    // The answer of the CLI to one of Kodem's requests.
    fn response(&mut self, id: &Value, message: &Value) -> Result<()> {
        let id = id.as_u64();
        let error = message.get("error").map(error_text);
        if id.is_some() && id == self.compacting {
            self.compacting = None;
            let failure = error.map(|error| format!("Grok Build could not compact: {error}"));
            return provider::end_compaction(self.host, self.task_id, failure);
        }
        if id.is_some() && id == self.prompt {
            self.prompt = None;
            if !self.running {
                return Ok(());
            }
            // The turn's end arrives as a notification first; the answer ends a turn that
            // reported none. An error before the CLI echoed the prompt refused it.
            if let Some(error) = &error
                && let Some((submission, _)) = self.sent.take()
            {
                orchestrator::dispatch(
                    self.host,
                    self.task_id,
                    Input::Signal(Signal::DeliveryFailed {
                        submission,
                        steers: false,
                        unknown: false,
                        error: error.clone(),
                    }),
                )?;
            }
            let reason = message["result"]["stopReason"].as_str().unwrap_or_default();
            let outcome = self.outcome(reason, error);
            return self.end_turn(outcome);
        }
        let Some(submission) = self.steer_request(id) else {
            return Ok(());
        };
        // The CLI queued the message for the running turn. An interjection that misses the turn
        // becomes a turn of its own, without the notification that names it, so this answer is
        // the proof of delivery.
        let signal = match error {
            None => Signal::Delivered {
                turn: format!("grok:{submission}"),
                submission,
                steers: true,
            },
            Some(error) => Signal::DeliveryFailed {
                submission,
                steers: true,
                unknown: false,
                error,
            },
        };
        orchestrator::dispatch(self.host, self.task_id, Input::Signal(signal)).map(drop)
    }

    // The submission of an `x.ai/interject` request, which the request's identity names.
    fn steer_request(&mut self, id: Option<u64>) -> Option<String> {
        let id = id?.to_string();
        let submission = self
            .steers
            .iter()
            .find(|(_, request)| **request == id)
            .map(|(submission, _)| submission.clone())?;
        self.steers.remove(&submission);
        Some(submission)
    }

    fn outcome(&self, reason: &str, error: Option<String>) -> TurnOutcome {
        if self.interrupting || reason == "cancelled" {
            return TurnOutcome {
                state: RunState::Interrupted,
                error: None,
            };
        }
        if let Some(error) = error {
            return TurnOutcome {
                state: RunState::Failed,
                error: Some(error.chars().take(4000).collect()),
            };
        }
        match reason {
            "end_turn" | "" => TurnOutcome {
                state: RunState::Completed,
                error: None,
            },
            "max_turn_requests" => TurnOutcome {
                state: RunState::Interrupted,
                error: Some("Grok Build reached its turn limit".into()),
            },
            "max_tokens" => TurnOutcome {
                state: RunState::Interrupted,
                error: Some("Grok Build reached its output limit".into()),
            },
            "refusal" => TurnOutcome {
                state: RunState::Failed,
                error: Some("The model refused this request".into()),
            },
            // The CLI ends a rate-limited turn without detail, but its last retry update said why.
            "rate_limit" => TurnOutcome {
                state: RunState::Failed,
                error: Some(self.transcript.failure.clone().unwrap_or_else(|| {
                    "xAI rate limited this account. Wait a moment and send again, or check the \
                     plan's usage."
                        .into()
                })),
            },
            other => TurnOutcome {
                state: RunState::Failed,
                error: Some(format!("Grok Build ended the turn: {other}")),
            },
        }
    }

    fn end_turn(&mut self, outcome: TurnOutcome) -> Result<()> {
        self.transcript.failure = None;
        // A turn that ends before the CLI reported its prompt still held it.
        self.delivered()?;
        self.transcript.finish(self.host)?;
        self.reported()?;
        // Agents that run in the background report their own end; a stopped turn stops them.
        if outcome.state != RunState::Completed {
            self.transcript
                .stop_agents(self.host, RunState::Interrupted)?;
        }
        self.withdraw_asks("The turn ended.")?;
        self.name_task(None)?;
        if let Some(turn) = self.turn.take() {
            if self.ended.len() == ENDED_TURNS {
                self.ended.pop_front();
            }
            self.ended.push_back(turn);
        }
        self.running = false;
        self.sent = None;
        self.prompt = None;
        self.interrupting = false;
        // The end of the turn releases its outcome itself when no agent works.
        self.transcript.agents_stopped = false;
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::TurnEnded { outcome }),
        )
        .map(drop)
    }

    // Releases the held outcome of an ended turn once its last background agent stopped. The
    // CLI may also continue by itself after an agent reports, which starts a follow-up turn.
    fn agents_changed(&mut self) -> Result<()> {
        if !std::mem::take(&mut self.transcript.agents_stopped) {
            return Ok(());
        }
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::ChildrenChanged),
        )
        .map(drop)
    }

    // Names a task that has no title yet: by the title that the CLI gave the session, else by
    // the first line of the first prompt after a turn.
    fn name_task(&self, title: Option<&str>) -> Result<()> {
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        if !task.title_pending {
            return Ok(());
        }
        let title = match title {
            Some(title) => Some(title.chars().take(200).collect()),
            None => {
                let (messages, _) = h.store.messages(self.task_id, None)?;
                messages
                    .iter()
                    .find(|message| message.kind == MessageKind::User)
                    .and_then(|message| message.text.lines().find(|line| !line.trim().is_empty()))
                    .map(|line| line.trim().chars().take(80).collect())
            }
        };
        if let Some(title) = title {
            task.title = title;
            task.title_pending = false;
            h.save_task_with_notification(&task)?;
        }
        Ok(())
    }

    // Saves what the latest update reported: the context, the cost, the plan, the title and a
    // switch of mode.
    fn reported(&mut self) -> Result<()> {
        if let Some(title) = self.transcript.title.take() {
            self.name_task(Some(&title))?;
        }
        if let Some(mode) = self.transcript.mode.take() {
            self.mode_changed(&mode)?;
        }
        let plan = self.transcript.plan.take();
        let usage = std::mem::take(&mut self.transcript.usage_changed);
        if plan.is_none() && !usage {
            return Ok(());
        }
        let window = self.connection.as_ref().and_then(|connection| {
            window_of(&connection.initialized, self.applied.model.as_deref())
        });
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        if let Some(plan) = plan {
            task.plan = plan;
        }
        if usage {
            let previous = task.context_usage.take();
            let cost = self.transcript.cost.map(|amount| UsageCost {
                amount,
                currency: "USD".into(),
            });
            task.context_usage = match self.transcript.used {
                Some(used) => Some(ContextUsage {
                    used,
                    window: window.or(previous.as_ref().and_then(|usage| usage.window)),
                    reported: now(),
                    cost: cost.or(previous.and_then(|usage| usage.cost)),
                }),
                None => previous.map(|mut usage| {
                    usage.cost = cost.or(usage.cost.take());
                    usage
                }),
            };
        }
        h.store.save_task(&task)
    }

    // A switch of mode inside the session, such as an approved plan, becomes the task's rule.
    fn mode_changed(&mut self, mode: &str) -> Result<()> {
        let plan = mode == "plan";
        self.applied.plan = Some(plan);
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        let access = access_of(&task);
        let next = match (plan, access == "plan") {
            (true, false) => "plan",
            (false, true) => self.after_plan.take().unwrap_or(DEFAULT_ACCESS),
            _ => return Ok(()),
        };
        task.provider_access = Some(next.to_owned());
        h.store.save_task(&task)
    }

    fn dispatch(&mut self) -> Result<()> {
        let task = self.task()?;
        if self.running {
            // Only input bound to the running turn is eligible while the task is active.
            if task.active_turn.is_some()
                && self.session.is_some()
                && let Some(message) = provider::claim_next(self.host, self.task_id)?
            {
                self.steer(message)?;
            }
            return Ok(());
        }
        if self.compacting.is_some() || !matches!(task.state, RunState::Idle | RunState::Completed)
        {
            return Ok(());
        }
        let queued = self
            .host
            .lock()
            .unwrap()
            .store
            .outgoing(self.task_id)?
            .iter()
            .any(|message| message.state == DeliveryState::Queued);
        if !queued {
            return Ok(());
        }
        self.prepare(&task)?;
        // The check between turns. The CLI's own rule covers a turn that is running.
        if provider::begin_due_compaction(self.host, self.task_id)? {
            return self.compact();
        }
        if let Some(message) = provider::claim_next(self.host, self.task_id)? {
            self.send(message)?;
        }
        Ok(())
    }

    // Brings the process and its session in line with the task's choices before the session
    // receives input. A process for another account or with another compaction rule ends
    // first, and the next one loads the same session.
    fn prepare(&mut self, task: &Task) -> Result<()> {
        let (account, compact) = {
            let h = self.host.lock().unwrap();
            (
                accounts::for_task(&h, task)?,
                provider::compact_rule(&h, task)?,
            )
        };
        let current = self.connection.as_mut().is_some_and(|connection| {
            connection.running()
                && connection.account == account
                && connection.compact == Some(compact)
        });
        if !current {
            self.connection = None;
            self.session = None;
            self.home = home_of(&account)?;
            let connection = Connection::start(&self.cwd, &account, Some(compact))?;
            publish(self.host, &connection.initialized);
            self.connection = Some(connection);
        }
        if self.session.is_none() {
            self.open(task)?;
        }
        self.configure(task)
    }

    // Opens the task's session in the process: loads the stored one, or creates it with the
    // task's standing instructions.
    fn open(&mut self, task: &Task) -> Result<()> {
        let yolo = access_of(task) == "bypassPermissions" && !read_only(task);
        let delegated = task
            .lineage
            .as_ref()
            .is_some_and(|lineage| lineage.delegated);
        // A task that another task delegated has no Kodem tools.
        let servers = if delegated {
            json!([])
        } else {
            let (command, arguments) = crate::mcp::command(self.task_id)?;
            json!([{"name":crate::mcp::SERVER,"command":command,"args":arguments,"env":[]}])
        };
        let connection = self
            .connection
            .as_mut()
            .context("Grok Build is not running")?;
        let stored = task.provider_thread.clone().filter(|_| self.stored);
        let opened = match &stored {
            Some(session) => connection.call(
                "session/load",
                json!({"sessionId":session,"cwd":self.cwd,"mcpServers":servers,
                    "_meta":{"yoloMode":yolo,"autoMode":false}}),
                SESSION_LIMIT,
                true,
            )?,
            None => connection.call(
                "session/new",
                json!({"cwd":self.cwd,"mcpServers":servers,
                    "_meta":{"rules":provider::instructions(task),"yoloMode":yolo,"autoMode":false}}),
                SESSION_LIMIT,
                false,
            )?,
        };
        let session = match stored {
            Some(session) => session,
            None => opened["sessionId"]
                .as_str()
                .context("Grok Build opened no session")?
                .to_owned(),
        };
        let option = |id: &str| {
            opened["configOptions"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|option| option["id"] == id)
                .and_then(|option| option["currentValue"].as_str())
                .map(str::to_owned)
        };
        self.applied = Applied {
            model: option("model"),
            effort: option("reasoning_effort"),
            // A loaded session keeps the mode it was saved in.
            plan: None,
            yolo: Some(yolo),
        };
        self.transcript.set_main(&session);
        self.session = Some(session);
        Ok(())
    }

    // Applies the choices that the CLI can change in place, before the next turn starts.
    fn configure(&mut self, task: &Task) -> Result<()> {
        let (model, effort) = {
            let h = self.host.lock().unwrap();
            let model = provider::effective_model(&h, task);
            let known = h.runtimes.get(&Provider::Grok).and_then(|runtime| {
                runtime
                    .models
                    .iter()
                    .find(|known| Some(&known.id) == model.as_ref())
                    .cloned()
            });
            let effort = known
                .filter(|known| !known.efforts.is_empty())
                .map(|known| {
                    task.effort
                        .clone()
                        .filter(|effort| known.efforts.contains(effort))
                        .unwrap_or(known.default_effort)
                });
            (model, effort)
        };
        let session = self.session.clone().context("Grok Build has no session")?;
        let connection = self
            .connection
            .as_mut()
            .context("Grok Build is not running")?;
        if let Some(model) = model.filter(|model| self.applied.model.as_ref() != Some(model)) {
            connection.call(
                "session/set_config_option",
                json!({"sessionId":session,"configId":"model","value":model}),
                READ_LIMIT,
                false,
            )?;
            self.applied.model = Some(model);
        }
        if let Some(effort) = effort.filter(|effort| self.applied.effort.as_ref() != Some(effort)) {
            connection.call(
                "session/set_config_option",
                json!({"sessionId":session,"configId":"reasoning_effort","value":effort}),
                READ_LIMIT,
                false,
            )?;
            self.applied.effort = Some(effort);
        }
        let plan = access_of(task) == "plan";
        if self.applied.plan != Some(plan) {
            connection.call(
                "session/set_mode",
                json!({"sessionId":session,"modeId":if plan { "plan" } else { "default" }}),
                READ_LIMIT,
                false,
            )?;
            self.applied.plan = Some(plan);
        }
        let yolo = access_of(task) == "bypassPermissions" && !read_only(task);
        if self.applied.yolo != Some(yolo) {
            connection.notify(
                "_x.ai/yolo_mode_changed",
                json!({"yolo_mode":yolo,"permission_mode":if yolo { "always-approve" } else { "ask" }}),
            )?;
            self.applied.yolo = Some(yolo);
        }
        Ok(())
    }

    // Runs the CLI's compaction for the compaction recorded on the host.
    fn compact(&mut self) -> Result<()> {
        if !self
            .host
            .lock()
            .unwrap()
            .compactions
            .contains_key(self.task_id)
        {
            return Ok(());
        }
        // The CLI continued by itself in the meantime; that turn owns the task now.
        if self.running {
            self.host.lock().unwrap().compactions.remove(self.task_id);
            return Ok(());
        }
        let task = self.task()?;
        if task.provider_thread.is_none() {
            return provider::end_compaction(
                self.host,
                self.task_id,
                Some("the session has no conversation to compact yet.".into()),
            );
        }
        self.prepare(&task)?;
        let session = self.session.clone().context("Grok Build has no session")?;
        let id = self
            .connection
            .as_mut()
            .context("Grok Build is not running")?
            .request("_x.ai/compact_conversation", json!({"sessionId":session}))?;
        self.compacting = Some(id);
        Ok(())
    }

    // The text of a prompt: the composed prompt, and the local copy of each image, which the
    // agent views with its file tool because the CLI takes text prompts only.
    fn text(message: &OutgoingMessage) -> Result<String> {
        let mut text = provider::prompt_for(message)?;
        for attachment in &message.attachments {
            if attachment.kind == AttachmentKind::Image {
                text.push_str(&format!(
                    "\n\nAttached image: {}\nView it with your file tool at its local path: {}",
                    serde_json::to_string(&attachment.name)?,
                    serde_json::to_string(&attachment.path)?
                ));
            }
        }
        Ok(text)
    }

    fn refuse(
        &self,
        message: &OutgoingMessage,
        steers: bool,
        unknown: bool,
        error: String,
    ) -> Result<()> {
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::DeliveryFailed {
                submission: message.id.clone(),
                steers,
                unknown,
                error,
            }),
        )?;
        Ok(())
    }

    fn send(&mut self, message: OutgoingMessage) -> Result<()> {
        let text = match Self::text(&message) {
            Ok(text) => text,
            // Nothing reached the CLI, so the message is refused, not uncertain.
            Err(error) => return self.refuse(&message, false, false, error.to_string()),
        };
        let session = self.session.clone().context("Grok Build has no session")?;
        let written = self
            .connection
            .as_mut()
            .context("Grok Build is not running")?
            .request(
                "session/prompt",
                json!({"sessionId":session,"prompt":[{"type":"text","text":text}]}),
            );
        let id = match written {
            Ok(id) => id,
            Err(error) => {
                self.connection = None;
                self.session = None;
                return self.refuse(&message, false, true, format!("{error:#}"));
            }
        };
        let turn = format!("grok:{}", message.id);
        self.running = true;
        self.interrupting = false;
        self.prompt = Some(id);
        self.turn = None;
        self.sent = Some((message.id.clone(), turn.clone()));
        self.transcript.working(self.host, "");
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::TurnStarted {
                turn: Some(turn),
                branch: None,
            }),
        )?;
        Ok(())
    }

    // Sends a message during a turn as an interjection, which the CLI gives the model at its
    // next step.
    fn steer(&mut self, message: OutgoingMessage) -> Result<()> {
        let text = match Self::text(&message) {
            Ok(text) => text,
            Err(error) => return self.refuse(&message, true, false, error.to_string()),
        };
        let session = self.session.clone().context("Grok Build has no session")?;
        let written = self
            .connection
            .as_mut()
            .context("Grok Build is not running")?
            .request(
                "_x.ai/interject",
                json!({"sessionId":session,"text":text,"interjectionId":message.id}),
            );
        match written {
            Ok(id) => {
                self.steers.insert(message.id.clone(), id.to_string());
                Ok(())
            }
            Err(error) => self.refuse(&message, true, true, format!("{error:#}")),
        }
    }

    fn interrupt(&mut self) -> Result<()> {
        // The CLI's compaction has no cancellation, so Stop ends the process, and the next
        // turn loads the session in a new one.
        if self.compacting.take().is_some() {
            self.connection = None;
            self.session = None;
            return provider::end_compaction(
                self.host,
                self.task_id,
                Some("it was stopped.".into()),
            );
        }
        if !self.running {
            return Ok(());
        }
        self.interrupting = true;
        self.withdraw_asks("The user stopped this turn.")?;
        let session = self.session.clone();
        match (&mut self.connection, session) {
            (Some(connection), Some(session)) => {
                connection.notify("session/cancel", json!({"sessionId":session}))?;
            }
            _ => {
                return self.end_turn(TurnOutcome {
                    state: RunState::Interrupted,
                    error: None,
                });
            }
        }
        Ok(())
    }

    // Answers every request that waits for the user as cancelled, and removes the requests.
    fn withdraw_asks(&mut self, reason: &str) -> Result<()> {
        if self.asks.is_empty() {
            return Ok(());
        }
        let asks: Vec<Ask> = self.asks.drain().map(|(_, ask)| ask).collect();
        if let Some(connection) = &mut self.connection {
            for ask in &asks {
                let answer = match ask.kind {
                    AskKind::Permission { .. } => json!({"outcome":{"outcome":"cancelled"}}),
                    AskKind::Question { .. } => json!({"outcome":"cancelled"}),
                    AskKind::Plan => json!({"outcome":"cancelled","feedback":reason}),
                };
                connection.respond(&ask.rpc, answer)?;
            }
        }
        let mut h = self.host.lock().unwrap();
        h.requests.remove(self.task_id);
        let mut task = h.store.task(self.task_id)?;
        let mut changed = false;
        for agent in &mut task.subagents {
            if agent.state == RunState::Waiting {
                agent.state = RunState::Running;
                changed = true;
            }
        }
        if task.state == RunState::Waiting {
            task.state = RunState::Running;
            h.save_task_with_notification(&task)?;
        } else if changed {
            h.store.save_task(&task)?;
        }
        Ok(())
    }

    // A request of the CLI: a tool call to approve, questions, or a plan to approve.
    fn request(&mut self, method: &str, id: Value, params: &Value) -> Result<()> {
        let session = params["sessionId"].as_str().unwrap_or_default();
        let requester = params["toolCall"]["_meta"]["x.ai/requestingSessionId"]
            .as_str()
            .unwrap_or(session);
        let agent = (Some(requester) != self.session.as_deref()).then(|| requester.to_owned());
        let known = matches!(
            method,
            "session/request_permission" | "_x.ai/ask_user_question" | "_x.ai/exit_plan_mode"
        );
        if self.interrupting || !known {
            let connection = self
                .connection
                .as_mut()
                .context("Grok Build is not running")?;
            return match method {
                "session/request_permission" => {
                    connection.respond(&id, json!({"outcome":{"outcome":"cancelled"}}))
                }
                "_x.ai/ask_user_question" => {
                    connection.respond(&id, json!({"outcome":"cancelled"}))
                }
                "_x.ai/exit_plan_mode" => connection.respond(
                    &id,
                    json!({"outcome":"cancelled","feedback":"The user stopped this turn."}),
                ),
                _ => connection.reject(&id, "Kodem does not implement this request"),
            };
        }
        match method {
            "session/request_permission" => self.permission(id, params, agent),
            "_x.ai/ask_user_question" => self.question(id, params, agent),
            _ => self.plan_approval(id, params, agent),
        }
    }

    fn permission(&mut self, id: Value, params: &Value, agent: Option<String>) -> Result<()> {
        let call = &params["toolCall"];
        let options = params["options"].as_array().cloned().unwrap_or_default();
        let task = self.task()?;
        let connection = self
            .connection
            .as_mut()
            .context("Grok Build is not running")?;
        match verdict(&task, call, self.transcript.roots()) {
            Verdict::Allow => {
                return connection
                    .respond(&id, select(&options, &["allow_once", "allow_always"], None));
            }
            Verdict::Deny(reason) => {
                return connection.respond(
                    &id,
                    select(&options, &["reject_once", "reject_always"], Some(&reason)),
                );
            }
            Verdict::Ask => {}
        }
        let name = transcript::tool_name(call);
        let input = &call["rawInput"];
        let subject = match transcript::category(transcript::tool_kind(call)) {
            ToolCategory::Command => transcript::call_command(input).to_owned(),
            ToolCategory::Edit => transcript::call_path(input).to_owned(),
            _ => transcript::describe(name, input, self.transcript.roots()).0,
        };
        let label = call["_meta"]["x.ai/tool"]["label"].as_str().unwrap_or(name);
        let (title, _) = transcript::describe(name, input, self.transcript.roots());
        let reason = match transcript::category(transcript::tool_kind(call)) {
            ToolCategory::Edit => format!("Grok wants to change {title}"),
            ToolCategory::Command => "Grok wants to run a command".into(),
            ToolCategory::Mcp => format!("Grok wants to use the tool {title}"),
            ToolCategory::Agent => "Grok wants to start a subagent".into(),
            ToolCategory::Fetch => format!("Grok wants to use {label}: {title}"),
            _ => format!("Grok wants to use {label}"),
        };
        let mut available = vec!["accept"];
        let mut labels = json!({"accept":"Allow once","decline":"Deny","cancel":"Deny and stop"});
        if let Some(always) = option(&options, &["allow_always"]) {
            available.push("acceptForSession");
            labels["acceptForSession"] = always["name"].clone();
        }
        available.extend(["decline", "cancel"]);
        let mut request = json!({"reason":reason,"command":subject,"tool":name,
            "availableDecisions":available,"decisionLabels":labels,
            "toolUseId":call["toolCallId"]});
        if let Some(agent) = &agent {
            request["threadId"] = json!(agent);
        }
        self.wait_for_user(
            "grok/requestApproval",
            request,
            Ask {
                kind: AskKind::Permission { options },
                rpc: id,
                agent,
            },
        )
    }

    fn question(&mut self, id: Value, params: &Value, agent: Option<String>) -> Result<()> {
        let questions: Vec<Value> = params["questions"].as_array().cloned().unwrap_or_default();
        let listed: Vec<Value> = questions
            .iter()
            .enumerate()
            .filter_map(|(index, question)| {
                let text = question["question"].as_str()?;
                Some(json!({"id":index.to_string(),"question":text,
                    "multiSelect":question["multiSelect"] == true || question["multi_select"] == true,
                    "options":question["options"].as_array().map(|options| options.iter()
                        .map(|option| json!({"label":option["label"],
                            "description":option["description"].as_str().unwrap_or_default()}))
                        .collect::<Vec<_>>())}))
            })
            .collect();
        if listed.is_empty() {
            return self
                .connection
                .as_mut()
                .context("Grok Build is not running")?
                .respond(&id, json!({"outcome":"cancelled"}));
        }
        let mut request = json!({"questions":listed});
        if let Some(agent) = &agent {
            request["threadId"] = json!(agent);
        }
        self.wait_for_user(
            "grok/requestUserInput",
            request,
            Ask {
                kind: AskKind::Question { questions },
                rpc: id,
                agent,
            },
        )
    }

    fn plan_approval(&mut self, id: Value, params: &Value, agent: Option<String>) -> Result<()> {
        let plan = params["planContent"].as_str().unwrap_or_default();
        let mut request = json!({"reason":"Grok proposes a plan","plan":plan,
            "command":if plan.trim().is_empty() { "Plan" } else { plan },
            "tool":"exit_plan_mode",
            "availableDecisions":["accept","acceptForSession","decline","cancel"],
            "decisionLabels":{"accept":"Approve and ask before edits",
                "acceptForSession":"Approve and accept edits",
                "decline":"Keep planning","cancel":"Stop"},
            "toolUseId":params["toolCallId"]});
        if let Some(agent) = &agent {
            request["threadId"] = json!(agent);
        }
        self.wait_for_user(
            "grok/plan/requestApproval",
            request,
            Ask {
                kind: AskKind::Plan,
                rpc: id,
                agent,
            },
        )
    }

    // Records a request that waits for the user and moves the task to waiting.
    fn wait_for_user(&mut self, method: &str, params: Value, ask: Ask) -> Result<()> {
        let key = new_id();
        {
            let mut h = self.host.lock().unwrap();
            h.requests
                .entry(self.task_id.into())
                .or_default()
                .push(InputRequest {
                    id: json!(key),
                    task_id: self.task_id.into(),
                    method: method.into(),
                    params,
                });
            let mut task = h.store.task(self.task_id)?;
            if let Some(known) = task
                .subagents
                .iter_mut()
                .find(|known| Some(&known.thread_id) == ask.agent.as_ref())
            {
                known.state = RunState::Waiting;
            }
            task.state = RunState::Waiting;
            h.save_task_with_notification(&task)?;
        }
        self.asks.insert(key, ask);
        Ok(())
    }

    fn answer(&mut self, id: Value, response: Value) -> Result<()> {
        let Some(key) = id.as_str().map(str::to_owned) else {
            return Ok(());
        };
        let Some(ask) = self.asks.remove(&key) else {
            return Ok(());
        };
        let request = {
            let mut h = self.host.lock().unwrap();
            h.requests.get_mut(self.task_id).and_then(|requests| {
                let position = requests.iter().position(|request| request.id == id)?;
                Some(requests.remove(position))
            })
        };
        // The desktop sends text typed beside a decline as `message`.
        let feedback = response["message"]
            .as_str()
            .map(str::trim)
            .filter(|feedback| !feedback.is_empty());
        let decision = response["decision"].as_str();
        let mut stop = false;
        let answer = match &ask.kind {
            AskKind::Question { questions } => question_answer(questions, &response),
            AskKind::Permission { options } => match decision {
                Some("accept") => select(options, &["allow_once"], None),
                Some("acceptForSession") => select(options, &["allow_always", "allow_once"], None),
                Some("decline") => select(
                    options,
                    &["reject_once", "reject_always"],
                    Some(feedback.unwrap_or("The user declined this action.")),
                ),
                Some("cancel") => {
                    stop = true;
                    select(
                        options,
                        &["reject_once", "reject_always"],
                        Some("The user stopped this turn."),
                    )
                }
                _ => bail!("Unsupported decision for this Grok Build request"),
            },
            AskKind::Plan => match decision {
                Some("accept") => {
                    self.after_plan = Some(DEFAULT_ACCESS);
                    json!({"outcome":"approved"})
                }
                Some("acceptForSession") => {
                    self.after_plan = Some("acceptEdits");
                    json!({"outcome":"approved"})
                }
                Some("decline") => json!({"outcome":"cancelled","feedback":feedback.unwrap_or(
                    "The user wants to keep planning. Revise the plan before presenting it again.")}),
                Some("cancel") => {
                    stop = true;
                    json!({"outcome":"cancelled","feedback":"The user stopped this turn."})
                }
                _ => bail!("Unsupported decision for this Grok Build request"),
            },
        };
        if let Some(connection) = &mut self.connection {
            connection.respond(&ask.rpc, answer)?;
        }
        self.resolve_request(&key, request.as_ref(), &ask, &response)?;
        if stop {
            self.interrupt()?;
        }
        Ok(())
    }

    // Lets the task continue after an answer, and writes the user's answers to questions.
    fn resolve_request(
        &mut self,
        key: &str,
        request: Option<&InputRequest>,
        ask: &Ask,
        response: &Value,
    ) -> Result<()> {
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        if let (Some(request), AskKind::Question { .. }) = (request, &ask.kind) {
            let transcript = task
                .subagents
                .iter()
                .find(|known| Some(&known.thread_id) == ask.agent.as_ref())
                .map_or_else(
                    || self.task_id.to_owned(),
                    |known| known.transcript_id(self.task_id),
                );
            h.store.message(&Message {
                seq: 0,
                transfer: None,
                id: format!("grok:answer:{key}"),
                task_id: transcript,
                kind: MessageKind::User,
                attachments: Vec::new(),
                feedback: Vec::new(),
                file_edits: Vec::new(),
                tool: None,
                text: crate::answer_text(request, response),
                complete: true,
                created: now(),
            })?;
        }
        let mut changed = false;
        if let Some(known) = task
            .subagents
            .iter_mut()
            .find(|known| Some(&known.thread_id) == ask.agent.as_ref())
            && known.state == RunState::Waiting
        {
            known.state = RunState::Running;
            changed = true;
        }
        if task.state == RunState::Waiting
            && h.requests
                .get(self.task_id)
                .is_none_or(|requests| requests.is_empty())
        {
            task.state = RunState::Running;
            h.save_task_with_notification(&task)?;
        } else if changed {
            h.store.save_task(&task)?;
        }
        Ok(())
    }
}
