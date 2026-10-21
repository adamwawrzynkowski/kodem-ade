//! Antigravity sessions over the CLI's headless stream-json mode.
//!
//! One `agy` process per task owner holds the conversation. Its model, reasoning, plan mode and
//! Kodem's additions are fixed when it starts, so a change starts a new process that continues
//! the same conversation with `--conversation`. Kodem adds its approval hook, its tools and the
//! task's standing instructions through a session folder that the process opens as one more
//! workspace with `--add-dir`: the CLI reads `.agents/hooks.json`, `.agents/mcp_config.json` and
//! `.agents/rules` of every workspace. The CLI asks nobody in headless mode, so its pre-tool hook
//! is where Kodem approves, refuses or asks the user about each tool call.
#[path = "antigravity_import.rs"]
mod import;
#[path = "antigravity_transcript.rs"]
mod transcript;
use crate::{
    Shared, accounts,
    orchestrator::{self, Input, Signal},
    provider,
};
use anyhow::{Context, Result, bail, ensure};
pub(crate) use import::{import_history, stored_thread, stored_threads};
use kodem_core::{ipc, platform, providers::antigravity, *};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime},
};
use transcript::Transcript;

/// Approval rules that Kodem offers for Antigravity, in the order a mode switch cycles through.
const ACCESS_MODES: [(&str, &str, &str); 4] = [
    (
        "ask",
        "Ask before changes",
        "Asks before file edits, commands, MCP tools, web access and reads outside the project.",
    ),
    (
        "edits",
        "Accept edits",
        "Edits files without asking. Still asks before commands, MCP tools, web access and reads outside the project.",
    ),
    (
        "plan",
        "Plan",
        "The CLI's plan mode: the agent explores and writes a plan. Kodem refuses edits of project files and asks before commands.",
    ),
    (
        "allow",
        "Allow everything",
        "Runs edits, commands, MCP tools and web access without asking. Use only in a checkout you can restore.",
    ),
];
/// The approval rule of a task that chose none.
const DEFAULT_ACCESS: &str = "ask";
/// Reasoning levels in the order the picker lists them. The CLI names a model's level at the
/// end of its identity, such as `gemini-3.8-flash-high`.
const EFFORT_ORDER: [&str; 6] = ["minimal", "low", "medium", "high", "xhigh", "max"];
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
/// The argument that starts Kodem's host as the pre-tool hook of one task's CLI.
pub(crate) const HOOK: &str = "antigravity-hook";
/// Longest time the CLI waits for the hook, in seconds: the user may take a while to answer.
const HOOK_LIMIT: u64 = 6 * 60 * 60;
/// How often the hook asks again while the user decides.
const HOOK_POLL: Duration = Duration::from_millis(400);
/// The folder under which each task's session folder lives.
const SESSION_FOLDER: &str = "kodem-antigravity";
/// The rule file of the session folder that holds the task's standing instructions.
const RULES: &str = "kodem.md";
/// A tool result can embed a whole file; larger lines are dropped, not fatal.
const LINE_LIMIT: u64 = 32 * 1024 * 1024;
const DIAGNOSTIC_LIMIT: usize = 4096;
/// How long a stopped process may take to save its conversation before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);
/// How long a command that reads the account, the catalog or the usage may take.
const READ_LIMIT: Duration = Duration::from_secs(45);
/// How often the stored files of the conversations and the task list are read.
const FOLLOW_INTERVAL: Duration = Duration::from_millis(300);

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
        // The CLI runs a message that arrives during a turn as the next turn.
        steering: false,
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

// Runs a short CLI command that starts no conversation, such as `agy models`.
fn read(account: &Account, cwd: &Path, arguments: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new(antigravity::executable()?);
    platform::hide_console(&mut command);
    account.apply(&mut command);
    command.current_dir(cwd).args(arguments);
    accounts::output(command, READ_LIMIT)
}

// Runs one of the read-only slash commands that print mode answers without a turn, and returns
// its data. The CLI asks for a sign-in in print mode, so only a signed-in account runs one.
fn slash(account: &Account, cwd: &Path, command: &str) -> Result<Value> {
    let output = read(account, cwd, &["-p", command, "--output-format", "json"])?;
    let report: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "The Antigravity CLI answered {command} unreadably: {}",
            first_error(&output.stderr)
        )
    })?;
    ensure!(
        report["status"] == "SUCCESS",
        "The Antigravity CLI could not answer {command}: {}",
        report["error"].as_str().unwrap_or("no reason given")
    );
    Ok(report["command"]["data"].clone())
}

// The first line of a CLI's diagnostics that explains a failure.
fn first_error(diagnostics: &[u8]) -> String {
    let text = String::from_utf8_lossy(diagnostics);
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with("error:") || line.starts_with("Error:"))
        .or_else(|| text.lines().map(str::trim).find(|line| !line.is_empty()))
        .unwrap_or("no reason given")
        .chars()
        .take(400)
        .collect()
}

/// The models that the account can use, or why it cannot use any.
enum Listing {
    Models(Vec<(String, String)>),
    SignedOut(String),
}

// Lists the models with `agy models`, which fails at once without a sign-in instead of
// starting one.
fn listing(account: &Account) -> Result<Listing> {
    let output = read(account, &platform::home_dir()?, &["models"])
        .context("Could not run the Antigravity CLI. Install it to use it")?;
    let text = String::from_utf8_lossy(&output.stdout);
    let models: Vec<(String, String)> = text
        .lines()
        .filter_map(|line| {
            let (id, name) = line.split_once('\t')?;
            Some((id.trim().to_owned(), name.trim().to_owned()))
        })
        .filter(|(id, _)| !id.is_empty())
        .collect();
    if output.status.success() && !models.is_empty() {
        return Ok(Listing::Models(models));
    }
    let reason = first_error(&output.stderr);
    if reason.to_lowercase().contains("sign in") || text.to_lowercase().contains("sign in") {
        return Ok(Listing::SignedOut(reason));
    }
    bail!("The Antigravity CLI listed no models: {reason}")
}

// Splits a model identity that ends with a reasoning level, such as `gemini-3.8-flash-high`.
fn effort_suffix(id: &str) -> Option<(&str, &str)> {
    EFFORT_ORDER.iter().find_map(|effort| {
        id.strip_suffix(effort)
            .and_then(|base| base.strip_suffix('-'))
            .filter(|base| !base.is_empty())
            .map(|base| (base, *effort))
    })
}

// A model's name without the reasoning level that the CLI adds in parentheses.
fn base_name(name: &str, effort: &str) -> String {
    let name = provider::model_name(name);
    let marker = format!("({effort})").to_lowercase();
    match name.to_lowercase().rfind(&marker) {
        Some(at) => name[..at].trim_end().to_owned(),
        None => name,
    }
}

/// Builds the catalog from the CLI's listing. The variants of one model at several reasoning
/// levels become one model with those levels.
fn models(listed: &[(String, String)], own: Option<&str>) -> Vec<Model> {
    let mut groups: HashMap<&str, Vec<(&str, &str, &str)>> = HashMap::new();
    for (id, name) in listed {
        if let Some((base, effort)) = effort_suffix(id) {
            groups
                .entry(base)
                .or_default()
                .push((id.as_str(), name.as_str(), effort));
        }
    }
    let mut models = Vec::new();
    let mut folded = HashSet::new();
    for (id, name) in listed {
        let group = effort_suffix(id).and_then(|(base, _)| {
            groups
                .get(base)
                .filter(|variants| variants.len() > 1)
                .map(|variants| (base, variants))
        });
        let Some((base, variants)) = group else {
            models.push(Model {
                id: id.clone(),
                name: provider::model_name(name),
                efforts: Vec::new(),
                default_effort: String::new(),
                images: true,
                effort_models: Default::default(),
                fast: None,
            });
            continue;
        };
        if !folded.insert(base) {
            continue;
        }
        let mut efforts: Vec<&str> = variants.iter().map(|(_, _, effort)| *effort).collect();
        efforts.sort_by_key(|effort| EFFORT_ORDER.iter().position(|known| known == effort));
        let default_effort = variants
            .iter()
            .find(|(variant, _, _)| Some(*variant) == own)
            .map(|(_, _, effort)| *effort)
            .or_else(|| efforts.iter().copied().find(|effort| *effort == "high"))
            .unwrap_or(efforts[efforts.len() - 1]);
        models.push(Model {
            id: base.to_owned(),
            name: base_name(name, effort_suffix(id).map_or("", |(_, effort)| effort)),
            efforts: efforts.iter().map(|effort| (*effort).to_owned()).collect(),
            default_effort: default_effort.to_owned(),
            images: true,
            effort_models: variants
                .iter()
                .map(|(variant, _, effort)| ((*effort).to_owned(), (*variant).to_owned()))
                .collect(),
            fast: None,
        });
    }
    models
}

/// Returns the CLI's identity of the model that runs a task, with its reasoning level.
fn cli_model(host: &crate::Host, task: &Task) -> Option<String> {
    let id = provider::effective_model(host, task)?;
    let known = host
        .runtimes
        .get(&Provider::Antigravity)
        .and_then(|runtime| runtime.models.iter().find(|model| model.id == id));
    let Some(model) = known.filter(|model| !model.effort_models.is_empty()) else {
        return Some(id);
    };
    let effort = task
        .effort
        .as_deref()
        .filter(|effort| model.effort_models.contains_key(*effort))
        .unwrap_or(&model.default_effort);
    model.effort_models.get(effort).cloned().or(Some(id))
}

/// The models of the account, the CLI's own choice among them, and its sign-in state.
struct Catalog {
    info: RuntimeInfo,
    status: AccountStatus,
}

fn catalog(account: &Account) -> Result<Catalog> {
    let listed = match listing(account)? {
        Listing::Models(listed) => listed,
        Listing::SignedOut(reason) => {
            return Ok(Catalog {
                info: RuntimeInfo {
                    error: Some(reason),
                    ..initial_runtime()
                },
                status: AccountStatus {
                    signed_in: Some(false),
                    ..AccountStatus::default()
                },
            });
        }
    };
    // The model that the CLI starts a conversation with.
    let own = slash(account, &platform::home_dir()?, "/model")
        .ok()
        .and_then(|data| data["id"].as_str().map(str::to_owned));
    let models = models(&listed, own.as_deref());
    let default_model = own
        .as_deref()
        .and_then(|own| {
            models
                .iter()
                .find(|model| model.id == own || model.effort_models.values().any(|id| id == own))
        })
        .or(models.first())
        .map(|model| model.id.clone());
    Ok(Catalog {
        info: RuntimeInfo {
            default_model,
            version: antigravity::version()
                .ok()
                .map(|version| format!("Antigravity CLI {version}")),
            models,
            ..initial_runtime()
        },
        status: AccountStatus {
            signed_in: Some(true),
            ..AccountStatus::default()
        },
    })
}

fn publish(host: &Shared, account: &Account, catalog: &Catalog) {
    let mut h = host.lock().unwrap();
    // A signed-out account lists no models; the last known catalog stays for saved choices.
    if catalog.status.signed_in == Some(true) {
        h.runtimes
            .insert(Provider::Antigravity, catalog.info.clone());
    } else {
        let runtime = h
            .runtimes
            .entry(Provider::Antigravity)
            .or_insert_with(initial_runtime);
        runtime.error = catalog.info.error.clone();
    }
    let mut status = catalog.status.clone();
    if let Some(known) = h.accounts.get(&(Provider::Antigravity, account.id.clone())) {
        status.usage = known.usage.clone();
    }
    accounts::observe(&mut h, account, status);
}

/// Reads the models of the account and the CLI's default, without a turn.
pub(super) fn discover(host: Shared, account: &Account, _cwd: Option<PathBuf>) -> Result<()> {
    let catalog = catalog(account)?;
    publish(&host, account, &catalog);
    Ok(())
}

// The share of a quota bucket that is used, from the share that remains.
fn used_percent(bucket: &Value) -> Option<i32> {
    let remaining = bucket["remaining_fraction"].as_f64()?;
    Some(((1.0 - remaining.clamp(0.0, 1.0)) * 100.0).round() as i32)
}

// The length of a quota window, by the CLI's name for it.
fn window_minutes(window: &str) -> Option<u64> {
    match window {
        "weekly" => Some(7 * 24 * 60),
        "daily" => Some(24 * 60),
        "hourly" => Some(60),
        _ => None,
    }
}

/// Builds the account's limits from `/usage` and `/credits`. The CLI shares one quota among
/// the models of a group: the first group is the primary window and the second the secondary.
fn usage_report(usage: &Value, credits: Option<&Value>) -> UsageLimits {
    let mut windows = usage["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|group| {
            // The bucket that is nearest to its limit is the one that stops the group.
            let bucket = group["buckets"]
                .as_array()?
                .iter()
                .max_by_key(|bucket| used_percent(bucket).unwrap_or(0))?;
            let window = bucket["window"].as_str().unwrap_or_default();
            Some((
                UsageWindow {
                    used_percent: used_percent(bucket)?,
                    window_duration_mins: window_minutes(window),
                    resets_at: bucket["reset_time"].as_str().and_then(unix_seconds),
                    name: group["name"].as_str().map(str::to_owned),
                },
                window.to_owned(),
            ))
        });
    let primary = windows.next();
    let secondary = windows.next();
    let reached = [&primary, &secondary]
        .into_iter()
        .flatten()
        .find(|(window, _)| window.used_percent >= 100)
        .map(|(_, name)| name.clone());
    UsageLimits {
        primary: primary.map(|(window, _)| window),
        secondary: secondary.map(|(window, _)| window),
        credits: credits.and_then(|credits| {
            let balance = credits["remaining_credits"].as_f64()?;
            Some(UsageCredits {
                has_credits: balance > 0.0,
                unlimited: false,
                balance: Some(credits["remaining_credits"].to_string()),
            })
        }),
        rate_limit_reached_type: reached,
        reported: now(),
        ..UsageLimits::default()
    }
}

fn read_usage(account: &Account) -> Result<UsageLimits> {
    let home = platform::home_dir()?;
    let usage = slash(account, &home, "/usage")?;
    let credits = slash(account, &home, "/credits").ok();
    Ok(usage_report(&usage, credits.as_ref()))
}

/// Reads whether the account is signed in, and its quotas when requested.
pub(super) fn account_status(
    _host: &Shared,
    account: &Account,
    usage: bool,
) -> Result<AccountStatus> {
    let signed_in = match listing(account)? {
        Listing::Models(_) => true,
        Listing::SignedOut(_) => false,
    };
    let mut status = AccountStatus {
        signed_in: Some(signed_in),
        ..AccountStatus::default()
    };
    if usage && signed_in {
        match read_usage(account) {
            Ok(usage) => status.usage = Some(usage),
            Err(error) => status.error = Some(format!("{error:#}")),
        }
    }
    Ok(status)
}

/// Reads the account's quotas again; the CLI has no banked resets to redeem.
pub(super) fn account_usage(
    host: &Shared,
    account: &Account,
    idempotency_key: Option<&str>,
) -> Result<Response> {
    ensure!(
        idempotency_key.is_none(),
        "The Antigravity CLI does not offer banked usage resets"
    );
    let usage = read_usage(account)?;
    accounts::set_usage(
        &mut host.lock().unwrap(),
        Provider::Antigravity,
        &account.id,
        usage.clone(),
    );
    Ok(Response::Usage {
        provider: Provider::Antigravity,
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
fn skills(account: &Account, cwd: &Path) -> Result<Vec<ComposerSkill>> {
    let listed = slash(account, cwd, "/skills")?;
    let mut skills: Vec<ComposerSkill> = listed["skills"]
        .as_array()
        .into_iter()
        .flatten()
        .take(1000)
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

/// Returns the skills that the CLI found for the checkout. Its slash commands cannot be sent
/// in a stream-json session, so none is offered, and it compacts only by itself.
pub(super) fn composer_catalog(
    host: &Shared,
    account: &Account,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<ComposerCatalog> {
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
    match skills(account, &cwd) {
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

/// Lists instruction files, skills, custom agents and the MCP servers of the CLI's
/// configuration files.
pub(super) fn resources(
    host: &Shared,
    account: &Account,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<Vec<Resource>> {
    let cwd = location(host, task_id, project_id)?;
    let home = platform::home_dir()?;
    let gemini = home.join(".gemini");
    let mut resources = Vec::new();
    let mut files: Vec<(PathBuf, &str)> = Vec::new();
    for folder in cwd.ancestors() {
        for name in ["AGENTS.md", "GEMINI.md"] {
            files.push((folder.join(name), "Project instructions"));
            files.push((folder.join(".agents").join(name), "Project instructions"));
        }
    }
    for folder in [gemini.clone(), gemini.join("config")] {
        for name in ["AGENTS.md", "GEMINI.md"] {
            files.push((folder.join(name), "User instructions"));
        }
    }
    for folder in [
        cwd.join(".agents").join("rules"),
        gemini.join("config").join("rules"),
        antigravity::data_dir()?.join("rules"),
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
        files.extend(rules.into_iter().take(200).map(|path| (path, "Rule")));
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
    match skills(account, &cwd) {
        Ok(skills) => {
            for skill in skills {
                resources.push(Resource {
                    name: skill.name,
                    source: skill.path.display().to_string(),
                    scope: "Antigravity skill".into(),
                    status: "Discovered file".into(),
                    detail: skill.description,
                });
            }
        }
        Err(error) => resources.push(Resource {
            name: "Skills".into(),
            source: "Antigravity CLI".into(),
            scope: "Antigravity skill".into(),
            status: "Unavailable".into(),
            detail: format!("{error:#}"),
        }),
    }
    if let Ok(agents) = slash(account, &cwd, "/agents") {
        for agent in agents["agents"].as_array().into_iter().flatten() {
            resources.push(Resource {
                name: agent["name"].as_str().unwrap_or("Agent").into(),
                source: agent["path"].as_str().unwrap_or("Antigravity CLI").into(),
                scope: "Custom agent".into(),
                status: "Available".into(),
                detail: agent["description"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(2000)
                    .collect(),
            });
        }
    }
    for (path, scope) in [
        (cwd.join(".agents").join("mcp_config.json"), "Project MCP"),
        (gemini.join("config").join("mcp_config.json"), "User MCP"),
    ] {
        let Some(config) = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        else {
            continue;
        };
        for (name, server) in config["mcpServers"].as_object().into_iter().flatten() {
            resources.push(Resource {
                name: name.clone(),
                source: path.display().to_string(),
                scope: scope.into(),
                status: if server["disabled"] == true {
                    "Disabled".into()
                } else {
                    "Configured".into()
                },
                detail: "This inventory is not a tool execution check.".into(),
            });
        }
    }
    Ok(resources)
}

/// Returns the folder that Kodem adds to a task's workspaces.
///
/// The CLI orders the workspaces of a conversation by path, and a subagent works in the first,
/// so the folder must sort after the task's own folder. The temporary folder does on macOS and
/// Linux, where it is outside the home folder; Kodem's data folder is the other candidate.
fn session_folder(task_id: &str, cwd: &Path) -> Result<PathBuf> {
    let candidates = [std::env::temp_dir(), ipc::data_dir()?];
    let folder = |root: &PathBuf| root.join(SESSION_FOLDER).join(task_id);
    Ok(candidates
        .iter()
        .map(folder)
        .find(|candidate| candidate.as_os_str() > cwd.as_os_str())
        .unwrap_or_else(|| folder(&candidates[0])))
}

/// Reports a folder of a conversation that is the session folder of a Kodem task.
pub(super) fn is_session_folder(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == SESSION_FOLDER)
}

// Quotes one word of the hook's command line, which the CLI runs with a shell.
#[cfg(unix)]
fn shell_word(text: &str) -> String {
    accounts::quote(text)
}
#[cfg(windows)]
fn shell_word(text: &str) -> String {
    format!("\"{text}\"")
}

/// Returns the command line that runs the pre-tool hook for one task of this host.
fn hook_command(task_id: &str) -> Result<String> {
    let root = ipc::data_dir()?;
    Ok([
        std::env::current_exe()?.to_string_lossy().into_owned(),
        HOOK.to_owned(),
        "--root".to_owned(),
        root.to_str()
            .context("Kodem data path is not UTF-8")?
            .to_owned(),
        "--task".to_owned(),
        task_id.to_owned(),
    ]
    .iter()
    .map(|word| shell_word(word))
    .collect::<Vec<_>>()
    .join(" "))
}

/// Everything fixed when a process starts. A change needs a new process.
#[derive(Clone, PartialEq)]
struct Launch {
    model: Option<String>,
    plan: bool,
    /// The task's standing instructions, as a rule of the session folder.
    rules: String,
    /// Kodem's own tools, which a delegated task does not get.
    tools: bool,
}

impl Launch {
    fn of(host: &crate::Host, task: &Task) -> Self {
        Self {
            model: cli_model(host, task),
            plan: access_of(task) == "plan",
            rules: provider::instructions(task),
            tools: !task
                .lineage
                .as_ref()
                .is_some_and(|lineage| lineage.delegated),
        }
    }
}

// Writes the session folder: the approval hook, Kodem's tools and the standing instructions.
fn write_session_folder(folder: &Path, task_id: &str, launch: &Launch) -> Result<()> {
    let agents = folder.join(".agents");
    let rules = agents.join("rules");
    std::fs::create_dir_all(&rules)?;
    for path in [folder.parent().unwrap_or(folder), folder, &agents, &rules] {
        platform::restrict(path, 0o700)?;
    }
    let hooks = json!({"kodem":{"PreToolUse":[{"matcher":"*","hooks":[
        {"type":"command","command":hook_command(task_id)?,"timeout":HOOK_LIMIT}]}]}});
    std::fs::write(
        agents.join("hooks.json"),
        serde_json::to_vec_pretty(&hooks)?,
    )?;
    let servers = if launch.tools {
        let (command, arguments) = crate::mcp::command(task_id)?;
        json!({crate::mcp::SERVER:{"command":command,"args":arguments}})
    } else {
        json!({})
    };
    std::fs::write(
        agents.join("mcp_config.json"),
        serde_json::to_vec_pretty(&json!({"mcpServers":servers}))?,
    )?;
    let rule = rules.join(RULES);
    if launch.rules.trim().is_empty() {
        let _ = std::fs::remove_file(rule);
    } else {
        std::fs::write(
            rule,
            format!(
                "---\ntrigger: always_on\ndescription: Kodem's rules for this session\n---\n{}\n",
                launch.rules
            ),
        )?;
    }
    Ok(())
}

/// One running `agy` process and its event stream.
struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    events: mpsc::Receiver<std::result::Result<Value, String>>,
    diagnostics: Arc<Mutex<String>>,
    launch: Launch,
}

impl Process {
    fn start(
        cwd: &Path,
        account: &Account,
        launch: Launch,
        conversation: Option<&str>,
        folder: &Path,
    ) -> Result<Self> {
        let mut command = Command::new(antigravity::executable()?);
        platform::hide_console(&mut command);
        account.apply(&mut command);
        command.current_dir(cwd).args([
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
        ]);
        if let Some(model) = &launch.model {
            command.args(["--model", model]);
        }
        if launch.plan {
            command.args(["--mode", "plan"]);
        }
        if let Some(conversation) = conversation {
            command.args(["--conversation", conversation]);
        }
        command.arg("--add-dir").arg(folder);
        // The prompts arrive on the stream. An empty prompt attached to the option keeps the
        // CLI from taking the next argument as one.
        command.arg("-p=");
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start the Antigravity CLI. Install it and sign in from Providers settings")?;
        let stdin = child
            .stdin
            .take()
            .context("Missing Antigravity CLI input")?;
        let stdout = child
            .stdout
            .take()
            .context("Missing Antigravity CLI output")?;
        let stderr = child
            .stderr
            .take()
            .context("Missing Antigravity CLI diagnostics")?;
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
        let (sender, events) = mpsc::sync_channel(256);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                let result = reader
                    .by_ref()
                    .take(LINE_LIMIT + 1)
                    .read_until(b'\n', &mut bytes);
                let value = match result {
                    Ok(0) => Err("The Antigravity CLI ended its session".into()),
                    Ok(_) if bytes.len() as u64 > LINE_LIMIT && !bytes.ends_with(b"\n") => {
                        // Skip the rest of an oversized event and keep the session alive.
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
                    // The CLI writes notices that are not events, such as warnings.
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
        Ok(Self {
            child,
            stdin: Some(stdin),
            events,
            diagnostics,
            launch,
        })
    }

    /// Writes one prompt line; a failure leaves the delivery of that prompt unknown.
    fn send(&mut self, value: &Value) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("The Antigravity CLI no longer reads input")?;
        serde_json::to_writer(&mut *stdin, value)?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        Ok(())
    }

    /// The CLI's own explanation of a failure, from its diagnostics.
    fn reason(&self) -> Option<String> {
        let tail = self.diagnostics.lock().unwrap().clone();
        let line = first_error(tail.as_bytes());
        (line != "no reason given").then_some(line)
    }

    /// Cancels the turn as Ctrl+C does. The CLI reports the turn as interrupted and ends.
    fn interrupt(&mut self) {
        #[cfg(unix)]
        if let Ok(id) = libc::pid_t::try_from(self.child.id()) {
            // SAFETY: the process is a child of this one that was not waited for, so its
            // identity still names it.
            unsafe {
                libc::kill(id, libc::SIGINT);
            }
            return;
        }
        let _ = self.child.kill();
    }

    fn running(&mut self) -> bool {
        self.child.try_wait().is_ok_and(|status| status.is_none())
    }

    /// Ends the session: closing the input lets the CLI save the conversation and exit.
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

impl Drop for Process {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Runs the pre-tool hook for the CLI of one task: reads the call from standard input, asks the
/// host, and prints the decision. It asks again while the user decides.
pub(crate) fn serve_hook(arguments: &[String]) -> Result<()> {
    let option = |name: &str| {
        arguments
            .iter()
            .position(|argument| argument == name)
            .and_then(|index| arguments.get(index + 1))
            .cloned()
            .with_context(|| format!("Missing {name}"))
    };
    let root = PathBuf::from(option("--root")?);
    let task_id = option("--task")?;
    let mut input = Vec::new();
    std::io::stdin()
        .lock()
        .take(LINE_LIMIT)
        .read_to_end(&mut input)?;
    let mut payload: Value = serde_json::from_slice(&input).context("Unreadable hook input")?;
    let mut ticket: Option<String> = None;
    let decision = loop {
        let request = Request::AntigravityHook {
            task_id: task_id.clone(),
            payload: std::mem::take(&mut payload),
            ticket: ticket.clone(),
        };
        let answer = match ipc::request(&root, &request) {
            Ok(Response::Text(text)) => serde_json::from_str::<Value>(&text).ok(),
            Ok(_) => None,
            Err(error) => {
                break json!({"decision":"deny",
                    "reason":format!("Kodem could not decide on this call: {error:#}")});
            }
        };
        match answer {
            Some(answer) if answer["ticket"].is_string() => {
                ticket = answer["ticket"].as_str().map(str::to_owned);
                std::thread::sleep(HOOK_POLL);
            }
            Some(answer) if answer["decision"].is_string() => break answer,
            _ => {
                break json!({"decision":"deny","reason":"Kodem gave no decision on this call."});
            }
        }
    };
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, &decision)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

/// Answers a hook of a task's CLI through the session owner that runs it.
pub(crate) fn hook(
    host: &Shared,
    task_id: &str,
    payload: Value,
    ticket: Option<String>,
) -> Result<String> {
    let owner = host.lock().unwrap().sessions.get(task_id).cloned();
    let Some(owner) = owner else {
        return Ok(
            json!({"decision":"deny","reason":"Kodem no longer runs this task."}).to_string(),
        );
    };
    let (reply, response) = mpsc::sync_channel(1);
    owner.send(provider::Control::Query {
        method: "antigravity/hook",
        params: json!({"payload":payload,"ticket":ticket}),
        reply,
    })?;
    let decision = response
        .recv_timeout(Duration::from_secs(60))
        .context("The Antigravity session did not answer")?
        .map_err(anyhow::Error::msg)?;
    Ok(decision.to_string())
}

/// Resolves uncertain delivery from the conversation that the CLI stored.
///
/// A stored prompt holds no identity of Kodem's, so the proof is the latest stored prompt
/// that starts with the exact text of the submission.
pub(super) fn reconcile(host: &Shared, task_id: &str) -> Result<()> {
    let (task, submission) = {
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
        (
            task,
            h.store
                .latest_submission(task_id)?
                .context("No recorded outgoing submission to reconcile")?,
        )
    };
    let conversation = task.provider_thread.clone().context("The Antigravity CLI never confirmed a conversation for this task, so the prompt cannot be reconciled automatically. Retain this task and create a new one.")?;
    ensure!(
        !antigravity::summary(&conversation)?.is_some_and(|summary| summary.running()),
        "The Antigravity CLI is still working in this conversation. Reconcile again in a moment."
    );
    let stored = std::fs::read_to_string(antigravity::transcript(&conversation)?)
        .context("The Antigravity CLI no longer has this conversation")?;
    let steps: Vec<Value> = stored
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let prompt = submission.prompt.trim();
    let receipt = steps
        .iter()
        .rev()
        .find(|step| {
            step["type"] == "USER_INPUT"
                && transcript::request_text(step["content"].as_str().unwrap_or_default())
                    .starts_with(prompt)
        })
        .and_then(|step| step["step_index"].as_u64())
        .context("The Antigravity conversation does not contain this exact submission. Delivery remains unknown; no text was resent.")?;
    // A turn ended with a response that made no further call.
    let finished = steps
        .iter()
        .rev()
        .find(|step| {
            step["step_index"]
                .as_u64()
                .is_some_and(|index| index > receipt)
        })
        .is_some_and(|step| {
            step["type"] == "PLANNER_RESPONSE"
                && step["tool_calls"].as_array().is_none_or(Vec::is_empty)
        });
    let mut transcript = Transcript::new(task_id, &task.cwd, true);
    transcript.set_main(&conversation);
    transcript.replay_after(host, &conversation, receipt)?;
    transcript.finish(host)?;
    {
        let h = host.lock().unwrap();
        h.store
            .confirm_submission(&submission.id, &format!("antigravity:{receipt}"))?;
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
            Some("The connection to the Antigravity CLI was lost during this turn. The output it stored was restored; send a message to continue.".to_owned()),
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

/// What a request that waits for the user is about.
enum AskKind {
    /// A tool call, with the rule that "Always allow" adds for the session.
    Permission { rule: Option<String> },
    /// The questions of an `ask_question` call.
    Question,
}

/// A request of the CLI that waits for the user.
struct Ask {
    kind: AskKind,
    /// The subagent conversation that asked; absent for the task's own.
    agent: Option<String>,
}

/// What Kodem does with one tool call.
enum Verdict {
    /// Runs it without asking.
    Allow,
    /// Leaves it to the CLI's own rules.
    Defer,
    Deny(String),
    /// Asks the user, with the reason, the call's subject, and the rule that "Always allow"
    /// adds for the session.
    Ask {
        reason: String,
        subject: String,
        rule: Option<(String, String)>,
    },
    Question,
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

/// Decides on one tool call by the task's approval mode and kind.
fn verdict(task: &Task, call: &Value, roots: &[PathBuf], artifacts: Option<&Path>) -> Verdict {
    let name = call["name"].as_str().unwrap_or_default();
    let arguments = &call["args"];
    let access = access_of(task);
    let everything = access == "allow";
    let review = task.kind == TaskKind::Review || task.shares_checkout();
    let path = transcript::call_path(arguments);
    let in_artifacts = artifacts.is_some_and(|folder| Path::new(path).starts_with(folder));
    let category = transcript::category(name);
    match name {
        "ask_question" => Verdict::Question,
        // The CLI's own artifacts, such as the task list and the plan, change no project file.
        _ if transcript::edits(name) && in_artifacts => Verdict::Allow,
        _ if transcript::edits(name) => {
            if review {
                return Verdict::Deny("This task reviews changes and must not edit files.".into());
            }
            match access {
                "plan" => Verdict::Deny("Plan mode: do not change project files. Write the plan, then end your turn; the user switches to another mode to carry it out.".into()),
                "allow" => Verdict::Allow,
                "edits" if inside(path, roots) => Verdict::Allow,
                _ => Verdict::Ask {
                    reason: format!("Antigravity wants to change {}", transcript::describe(name, arguments, roots).0),
                    subject: path.to_owned(),
                    rule: Some(("edit:*".into(), "all edits".into())),
                },
            }
        }
        "run_command" | "send_command_input" => {
            let command = arguments["CommandLine"]
                .as_str()
                .or_else(|| arguments["Input"].as_str())
                .unwrap_or_default();
            if review && blocked(command) {
                return Verdict::Deny(format!("A review must not run `{command}`."));
            }
            if everything {
                return Verdict::Allow;
            }
            let program = command.split_whitespace().next().unwrap_or_default();
            Verdict::Ask {
                reason: "Antigravity wants to run a command".into(),
                subject: command.to_owned(),
                rule: (!program.is_empty()).then(|| {
                    (
                        format!("command:{program}"),
                        format!("`{program}` commands"),
                    )
                }),
            }
        }
        "call_mcp_tool" => {
            let server = arguments["ServerName"].as_str().unwrap_or_default();
            let tool = arguments["ToolName"].as_str().unwrap_or_default();
            if server == crate::mcp::SERVER || everything {
                return Verdict::Allow;
            }
            Verdict::Ask {
                reason: format!(
                    "Antigravity wants to use the tool {tool} of the MCP server {server}"
                ),
                subject: arguments["Arguments"].to_string(),
                rule: Some((format!("mcp:{server}/{tool}"), format!("{server}/{tool}"))),
            }
        }
        "search_web" | "read_url_content" => {
            if everything || task.kind == TaskKind::Chat {
                return Verdict::Allow;
            }
            Verdict::Ask {
                reason: if name == "search_web" {
                    "Antigravity wants to search the web".into()
                } else {
                    "Antigravity wants to read a web page".into()
                },
                subject: transcript::describe(name, arguments, roots).0,
                rule: Some(("web:*".into(), "web access".into())),
            }
        }
        _ if name.starts_with("browser_")
            || matches!(
                name,
                "open_browser_url"
                    | "click_browser_pixel"
                    | "execute_browser_javascript"
                    | "capture_browser_screenshot"
            ) =>
        {
            if everything {
                return Verdict::Allow;
            }
            Verdict::Ask {
                reason: "Antigravity wants to use its browser".into(),
                subject: transcript::describe(name, arguments, roots).0,
                rule: Some(("browser:*".into(), "browser use".into())),
            }
        }
        _ if matches!(category, ToolCategory::Read | ToolCategory::Search) => {
            let outside = !path.is_empty() && !inside(path, roots) && !in_artifacts;
            if !outside || everything {
                return Verdict::Allow;
            }
            Verdict::Ask {
                reason: format!("Antigravity wants to read {path}, outside the project"),
                subject: path.to_owned(),
                rule: Some(("read:*".into(), "reads outside the project".into())),
            }
        }
        // Subagents' own calls come through this hook too.
        _ if category == ToolCategory::Agent => Verdict::Allow,
        _ => Verdict::Defer,
    }
}

/// The answers of the user to the questions of an `ask_question` call, as the model reads them.
fn answers(questions: &[Value], response: &Value) -> String {
    let mut text = String::from("Kodem asked the user these questions, so do not ask them again. ");
    let lines: Vec<String> = questions
        .iter()
        .filter_map(|question| {
            let id = question["id"].as_str()?;
            let chosen: Vec<&str> = response["answers"][id]["answers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            Some(format!(
                "\n- {}: {}",
                question["question"].as_str().unwrap_or_default(),
                if chosen.is_empty() {
                    "no answer".to_owned()
                } else {
                    chosen.join(", ")
                }
            ))
        })
        .collect();
    text.push_str("The user answered:");
    text.extend(lines);
    text
}

/// The task's plan from the task list artifact, a Markdown checklist.
fn plan_of(list: &str) -> Vec<PlanStep> {
    list.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let rest = line
                .strip_prefix("- [")
                .or_else(|| line.strip_prefix("* ["))?;
            let (mark, text) = rest.split_once(']')?;
            let status = match mark.trim() {
                "x" | "X" => PlanStatus::Done,
                "/" | "-" | "~" => PlanStatus::Active,
                "" => PlanStatus::Pending,
                _ => return None,
            };
            let text = text.trim();
            (!text.is_empty()).then(|| PlanStep {
                text: text.chars().take(2000).collect(),
                status,
            })
        })
        .take(200)
        .collect()
}

struct Owner<'a> {
    host: &'a Shared,
    task_id: &'a str,
    cwd: PathBuf,
    account: Account,
    folder: PathBuf,
    process: Option<Process>,
    /// The CLI's conversation of the task, once it exists.
    conversation: Option<String>,
    transcript: Transcript,
    /// A turn of the task runs.
    running: bool,
    /// The submission sent for the running turn until the CLI reports that it took it.
    sent: Option<(String, String)>,
    interrupting: bool,
    asks: HashMap<String, Ask>,
    /// Decisions that the user made and the hook has not read yet, by ticket.
    decided: HashMap<String, Value>,
    /// Rules that "Always allow" added for this session.
    allowed: HashSet<String>,
    /// When the task list artifact was last read.
    listed: Option<SystemTime>,
    followed: Instant,
}

/// Owns one resumable Antigravity conversation and keeps controls responsive while it works.
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
    let mut owner = Owner {
        host: &host,
        task_id,
        cwd: task.cwd.clone(),
        account,
        folder: session_folder(task_id, &task.cwd)?,
        process: None,
        conversation: task.provider_thread.clone(),
        transcript: Transcript::new(task_id, &task.cwd, false),
        running: false,
        sent: None,
        interrupting: false,
        asks: HashMap::new(),
        decided: HashMap::new(),
        allowed: HashSet::new(),
        listed: None,
        followed: Instant::now(),
    };
    if let Some(conversation) = &task.provider_thread {
        owner.transcript.set_main(conversation);
    }
    let result = owner.serve(&controls, &task);
    owner.process = None;
    let _ = std::fs::remove_dir_all(&owner.folder);
    result
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
                    provider::Control::Compact => provider::end_compaction(
                        self.host,
                        self.task_id,
                        Some("the Antigravity CLI compacts its context by itself and has no command for it in a Kodem session.".into()),
                    )?,
                    provider::Control::Answer { id, response } => self.answer(id, response)?,
                    provider::Control::Query {
                        method,
                        params,
                        reply,
                    } => {
                        let result = match method {
                            "antigravity/hook" => self
                                .hook(&params["payload"], params["ticket"].as_str())
                                .map_err(|error| format!("{error:#}")),
                            _ => Err("This operation is not supported by Antigravity".into()),
                        };
                        let _ = reply.send(result);
                    }
                }
            }
            self.receive()?;
            self.dispatch()?;
            self.transcript.write_due(self.host)?;
            if self.followed.elapsed() >= FOLLOW_INTERVAL {
                self.followed = Instant::now();
                self.transcript.follow(self.host)?;
                self.read_plan()?;
            }
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

    // Takes the events of the process, or learns that it ended.
    fn receive(&mut self) -> Result<()> {
        let Some(process) = &self.process else {
            std::thread::sleep(Duration::from_millis(40));
            return Ok(());
        };
        let event = match process.events.recv_timeout(Duration::from_millis(40)) {
            Ok(Ok(event)) => event,
            Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => return self.ended(),
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(()),
        };
        match event["event"].as_str().unwrap_or_default() {
            "init" => {
                if let Some(conversation) = event["conversation_id"].as_str() {
                    self.started(conversation)?;
                }
            }
            "step_update" => {
                let update = &event["step_update"];
                let ours = update["conversation_id"].as_str() == self.conversation.as_deref();
                if ours && update["step_type"] == "user_input" && update["state"] == "DONE" {
                    self.delivered()?;
                }
                self.transcript.update(self.host, update)?;
            }
            "result" => self.result(&event["result"])?,
            _ => {}
        }
        Ok(())
    }

    // The CLI named the conversation that it opened.
    fn started(&mut self, conversation: &str) -> Result<()> {
        if self.conversation.as_deref() == Some(conversation)
            || !antigravity::valid_conversation(conversation)
        {
            return Ok(());
        }
        self.conversation = Some(conversation.to_owned());
        self.transcript.set_main(conversation);
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        task.provider_thread = Some(conversation.to_owned());
        h.store.save_task(&task)
    }

    // The CLI took the prompt of the running turn into the conversation.
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
        Ok(())
    }

    // The process ended. During a turn whose prompt the CLI took, the turn failed; before that,
    // the prompt's delivery is unknown and the owner ends, so the host marks it so.
    fn ended(&mut self) -> Result<()> {
        let reason = self.process.as_ref().and_then(Process::reason);
        self.process = None;
        if !self.running {
            return Ok(());
        }
        if self.sent.is_some() && !self.interrupting {
            bail!(
                "The Antigravity CLI ended before it took the message{}",
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
                error: Some(
                    reason.unwrap_or_else(|| "The Antigravity CLI ended during the turn".into()),
                ),
            }
        };
        self.end_turn(outcome)
    }

    // The CLI reported the end of a turn.
    fn result(&mut self, result: &Value) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        // A turn that ends before the CLI reported the prompt still holds it.
        self.delivered()?;
        let refused: Vec<&str> = result["denied_actions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|action| {
                action["display_name"]
                    .as_str()
                    .or_else(|| action["action"].as_str())
            })
            .collect();
        if !refused.is_empty() {
            self.transcript.notice(
                self.host,
                format!("antigravity:refused:{}", new_id()),
                format!(
                    "The Antigravity CLI refused these actions by its own rules: {}. Allow them in its settings (`permissions.allow`) to let the agent use them.",
                    refused.join(", ")
                ),
            )?;
        }
        let error = result["error"].as_str().unwrap_or_default();
        let outcome = match result["status"].as_str() {
            _ if self.interrupting => TurnOutcome {
                state: RunState::Interrupted,
                error: None,
            },
            Some("SUCCESS") => TurnOutcome {
                state: RunState::Completed,
                error: None,
            },
            Some("CANCELED" | "INTERRUPTED") => TurnOutcome {
                state: RunState::Interrupted,
                error: None,
            },
            _ if error == "interrupted" => TurnOutcome {
                state: RunState::Interrupted,
                error: None,
            },
            _ => TurnOutcome {
                state: RunState::Failed,
                error: Some(if error.is_empty() {
                    "The Antigravity CLI reported a failure".into()
                } else {
                    error.chars().take(4000).collect()
                }),
            },
        };
        self.end_turn(outcome)
    }

    fn end_turn(&mut self, outcome: TurnOutcome) -> Result<()> {
        self.transcript.follow(self.host)?;
        self.read_plan()?;
        self.transcript.finish(self.host)?;
        self.context_usage()?;
        let agents = if outcome.state == RunState::Completed {
            RunState::Completed
        } else {
            RunState::Interrupted
        };
        self.transcript.stop_agents(self.host, agents)?;
        self.withdraw_asks("The turn ended.")?;
        self.name_conversation()?;
        self.running = false;
        self.sent = None;
        self.interrupting = false;
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::TurnEnded { outcome }),
        )?;
        Ok(())
    }

    // Names a task that has no title yet: by the title that the CLI gave the conversation, else
    // by the first line of the first prompt.
    fn name_conversation(&self) -> Result<()> {
        if !self.task()?.title_pending {
            return Ok(());
        }
        let own = self
            .conversation
            .as_deref()
            .and_then(|conversation| antigravity::summary(conversation).ok().flatten())
            .map(|summary| summary.title.trim().to_owned())
            .filter(|title| !title.is_empty());
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        let title = match own {
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
            h.store.save_task(&task)?;
        }
        Ok(())
    }

    fn context_usage(&self) -> Result<()> {
        let Some(used) = self.transcript.used else {
            return Ok(());
        };
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        let previous = task.context_usage.take();
        task.context_usage = Some(ContextUsage {
            used,
            window: previous.as_ref().and_then(|usage| usage.window),
            reported: now(),
            cost: previous.and_then(|usage| usage.cost),
        });
        h.store.save_task(&task)
    }

    // Reads the task list artifact of the conversation when it changed and makes it the plan.
    fn read_plan(&mut self) -> Result<()> {
        let Some(conversation) = &self.conversation else {
            return Ok(());
        };
        let path = antigravity::data_dir()?
            .join("brain")
            .join(conversation)
            .join(transcript::TASK_LIST);
        let Ok(changed) = std::fs::metadata(&path).and_then(|metadata| metadata.modified()) else {
            return Ok(());
        };
        if self.listed == Some(changed) {
            return Ok(());
        }
        self.listed = Some(changed);
        let list = std::fs::read_to_string(&path).unwrap_or_default();
        let plan = plan_of(&list);
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        if task.plan != plan {
            task.plan = plan;
            h.store.save_task(&task)?;
        }
        Ok(())
    }

    fn dispatch(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }
        let task = self.task()?;
        if !matches!(task.state, RunState::Idle | RunState::Completed) {
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
        if let Some(message) = provider::claim_next(self.host, self.task_id)? {
            self.send(message)?;
        }
        Ok(())
    }

    // Starts a process with the task's current choices before it receives input. A process
    // whose choices differ ends first; the next one continues the same conversation.
    fn prepare(&mut self, task: &Task) -> Result<()> {
        let (launch, account) = {
            let h = self.host.lock().unwrap();
            (Launch::of(&h, task), accounts::for_task(&h, task)?)
        };
        // The system may clear old files of its temporary folder, and a process whose session
        // folder lost its hook would run without Kodem's approvals.
        let hooked = self.folder.join(".agents").join("hooks.json").is_file();
        let current = self.process.as_mut().is_some_and(|process| {
            process.running() && process.launch == launch && account == self.account && hooked
        });
        if current {
            return Ok(());
        }
        self.process = None;
        write_session_folder(&self.folder, self.task_id, &launch)?;
        self.process = Some(Process::start(
            &self.cwd,
            &account,
            launch,
            self.conversation.as_deref(),
            &self.folder,
        )?);
        self.account = account;
        Ok(())
    }

    // The text of a prompt: the composed prompt, and the local copy of each image, which the
    // agent views with its file tool because the stream takes text only.
    fn prompt(message: &OutgoingMessage) -> Result<String> {
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

    fn send(&mut self, message: OutgoingMessage) -> Result<()> {
        let text = match Self::prompt(&message) {
            Ok(text) => text,
            Err(error) => {
                // Nothing reached the CLI, so the message is refused, not uncertain.
                orchestrator::dispatch(
                    self.host,
                    self.task_id,
                    Input::Signal(Signal::DeliveryFailed {
                        submission: message.id.clone(),
                        steers: false,
                        unknown: false,
                        error: error.to_string(),
                    }),
                )?;
                return Ok(());
            }
        };
        let turn = format!("antigravity:{}", message.id);
        let written = self
            .process
            .as_mut()
            .context("The Antigravity CLI is not running")?
            .send(&json!({"event":"user","message":{"content":text}}));
        if let Err(error) = written {
            orchestrator::dispatch(
                self.host,
                self.task_id,
                Input::Signal(Signal::DeliveryFailed {
                    submission: message.id.clone(),
                    steers: false,
                    unknown: true,
                    error: format!("{error:#}"),
                }),
            )?;
            self.process = None;
            return Ok(());
        }
        self.running = true;
        self.interrupting = false;
        self.sent = Some((message.id.clone(), turn.clone()));
        self.transcript.working(self.host);
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

    fn interrupt(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        self.interrupting = true;
        self.withdraw_asks("The user stopped this turn.")?;
        match &mut self.process {
            Some(process) => process.interrupt(),
            None => {
                return self.end_turn(TurnOutcome {
                    state: RunState::Interrupted,
                    error: None,
                });
            }
        }
        Ok(())
    }

    // Refuses every call that waits for the user, and removes the requests.
    fn withdraw_asks(&mut self, reason: &str) -> Result<()> {
        if self.asks.is_empty() {
            return Ok(());
        }
        for (ticket, _) in self.asks.drain() {
            self.decided
                .insert(ticket, json!({"decision":"deny","reason":reason}));
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

    // Answers the pre-tool hook: at once, or with a ticket while the user decides.
    fn hook(&mut self, payload: &Value, ticket: Option<&str>) -> Result<Value> {
        if let Some(ticket) = ticket {
            if let Some(decision) = self.decided.remove(ticket) {
                return Ok(decision);
            }
            if self.asks.contains_key(ticket) {
                return Ok(json!({"ticket":ticket}));
            }
            return Ok(json!({"decision":"deny","reason":"The request was withdrawn."}));
        }
        let call = &payload["toolCall"];
        let name = call["name"].as_str().unwrap_or_default();
        let conversation = payload["conversationId"].as_str().unwrap_or_default();
        if let Some(step) = payload["stepIdx"].as_u64() {
            self.transcript
                .arguments(self.host, conversation, step, name, &call["args"])?;
        }
        if self.interrupting {
            return Ok(json!({"decision":"deny","reason":"The user stopped this turn."}));
        }
        let task = self.task()?;
        let artifacts = payload["artifactDirectoryPath"].as_str().map(Path::new);
        // Kodem's own session folder holds no project file either.
        let mut roots = self.transcript.roots().to_vec();
        roots.push(self.folder.clone());
        // The hook belongs to this task's process, so a conversation that is not the task's
        // own is one of its subagents, even before the stream reports it.
        let agent =
            (Some(conversation) != self.conversation.as_deref()).then(|| conversation.to_owned());
        Ok(match verdict(&task, call, &roots, artifacts) {
            Verdict::Allow => json!({"decision":"auto_approve"}),
            Verdict::Defer => json!({"decision":"allow"}),
            Verdict::Deny(reason) => json!({"decision":"deny","reason":reason}),
            Verdict::Ask {
                reason,
                subject,
                rule,
            } => {
                if rule
                    .as_ref()
                    .is_some_and(|(rule, _)| self.allowed.contains(rule))
                {
                    return Ok(json!({"decision":"auto_approve"}));
                }
                let ticket = new_id();
                let mut available = vec!["accept"];
                let mut labels =
                    json!({"accept":"Allow once","decline":"Deny","cancel":"Deny and stop"});
                if let Some((_, scope)) = &rule {
                    available.push("acceptForSession");
                    labels["acceptForSession"] = json!(format!("Always allow {scope}"));
                }
                available.extend(["decline", "cancel"]);
                let mut params = json!({"reason":reason,"command":subject,"tool":name,
                    "availableDecisions":available,"decisionLabels":labels});
                if let Some(agent) = &agent {
                    params["threadId"] = json!(agent);
                }
                self.wait_for_user(
                    &ticket,
                    "antigravity/requestApproval",
                    params,
                    Ask {
                        kind: AskKind::Permission {
                            rule: rule.map(|(rule, _)| rule),
                        },
                        agent,
                    },
                )?;
                json!({"ticket":ticket})
            }
            Verdict::Question => {
                let questions: Vec<Value> = call["args"]["questions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .filter_map(|(index, question)| {
                        let text = question["question"].as_str()?;
                        let mut entry = json!({"id":index.to_string(),"question":text,
                            "multiSelect":question["is_multi_select"] == true});
                        if let Some(options) = question["options"].as_array() {
                            entry["options"] = json!(
                                options
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(|label| json!({"label":label,"description":""}))
                                    .collect::<Vec<_>>()
                            );
                        }
                        Some(entry)
                    })
                    .collect();
                if questions.is_empty() {
                    return Ok(json!({"decision":"allow"}));
                }
                let ticket = new_id();
                let mut params = json!({"questions":questions});
                if let Some(agent) = &agent {
                    params["threadId"] = json!(agent);
                }
                self.wait_for_user(
                    &ticket,
                    "antigravity/requestUserInput",
                    params,
                    Ask {
                        kind: AskKind::Question,
                        agent,
                    },
                )?;
                json!({"ticket":ticket})
            }
        })
    }

    // Records a request that waits for the user and moves the task to waiting.
    fn wait_for_user(&mut self, id: &str, method: &str, params: Value, ask: Ask) -> Result<()> {
        {
            let mut h = self.host.lock().unwrap();
            h.requests
                .entry(self.task_id.into())
                .or_default()
                .push(InputRequest {
                    id: json!(id),
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
        self.asks.insert(id.to_owned(), ask);
        Ok(())
    }

    fn answer(&mut self, id: Value, response: Value) -> Result<()> {
        let Some(ticket) = id.as_str().map(str::to_owned) else {
            return Ok(());
        };
        let Some(ask) = self.asks.remove(&ticket) else {
            return Ok(());
        };
        let request = {
            let mut h = self.host.lock().unwrap();
            h.requests.get_mut(self.task_id).and_then(|requests| {
                let position = requests.iter().position(|request| request.id == id)?;
                Some(requests.remove(position))
            })
        };
        let mut stop = false;
        let decision = match &ask.kind {
            AskKind::Question => {
                let questions = request
                    .as_ref()
                    .and_then(InputRequest::questions)
                    .cloned()
                    .unwrap_or_default();
                json!({"decision":"deny","reason":answers(&questions, &response)})
            }
            AskKind::Permission { rule } => {
                // The desktop sends text typed beside a decline as `message`.
                let feedback = response["message"]
                    .as_str()
                    .map(str::trim)
                    .filter(|feedback| !feedback.is_empty());
                match response["decision"].as_str() {
                    Some("accept") => json!({"decision":"auto_approve"}),
                    Some("acceptForSession") => {
                        if let Some(rule) = rule {
                            self.allowed.insert(rule.clone());
                        }
                        json!({"decision":"auto_approve"})
                    }
                    Some("decline") => json!({"decision":"deny",
                        "reason":format!("The user declined this action.{}",
                            feedback.map(|feedback| format!(" {feedback}")).unwrap_or_default())}),
                    Some("cancel") => {
                        stop = true;
                        json!({"decision":"deny","reason":"The user stopped this turn."})
                    }
                    _ => bail!("Unsupported decision for this Antigravity request"),
                }
            }
        };
        self.decided.insert(ticket.clone(), decision);
        self.resolve_request(&ticket, request.as_ref(), &ask, &response)?;
        if stop {
            self.interrupt()?;
        }
        Ok(())
    }

    // Lets the task continue after an answer, and writes the user's answers to questions.
    fn resolve_request(
        &mut self,
        ticket: &str,
        request: Option<&InputRequest>,
        ask: &Ask,
        response: &Value,
    ) -> Result<()> {
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        if let (Some(request), AskKind::Question) = (request, &ask.kind) {
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
                id: format!("antigravity:answer:{ticket}"),
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
