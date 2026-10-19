//! OpenCode sessions over the CLI's headless server and its event stream.
//!
//! One `opencode serve` process per task owns the conversation, an OpenCode session. Kodem
//! needs OpenCode 2, whose sessions keep their model, reasoning variant, agent, approval rules
//! and standing instructions. The account, Kodem's tools and automatic compaction are fixed
//! when a server starts. OpenCode support is in beta.
#[path = "opencode_import.rs"]
mod import;
#[path = "opencode_transcript.rs"]
mod transcript;
use crate::{
    Shared, accounts,
    orchestrator::{self, Input, Signal},
    provider,
};
use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
pub(crate) use import::{import_history, stored_thread, stored_threads};
use kodem_core::{
    providers::opencode::{Client, Refused, Server},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};
use transcript::Transcript;

/// Approval rules that Kodem offers for OpenCode, in the order a mode switch cycles through.
const ACCESS_MODES: [(&str, &str, &str); 4] = [
    (
        "ask",
        "Ask before changes",
        "Asks before file edits, commands and access to folders outside the project.",
    ),
    (
        "edits",
        "Accept edits",
        "Edits files without asking. Still asks before commands and access outside the project.",
    ),
    (
        "plan",
        "Plan",
        "OpenCode's plan agent explores and proposes a plan. It changes no project files.",
    ),
    (
        "allow",
        "Allow everything",
        "Runs edits, commands and access outside the project without asking. Use only in a checkout you can restore.",
    ),
];
/// The approval rule of a task that chose none.
const DEFAULT_ACCESS: &str = "ask";
/// Reasoning variants in the order the picker lists them.
const EFFORT_ORDER: [&str; 7] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];
/// The end of the identity OpenCode gives the fast mode of a model.
const FAST_SUFFIX: &str = "-fast";
/// Commands that a review or a task that reads its parent's checkout must not run. A denied
/// rule comes last, so it wins over the rule of the approval mode.
const BLOCKED_COMMANDS: [&str; 11] = [
    "git push*",
    "git commit*",
    "gh pr comment*",
    "gh pr review*",
    "gh pr edit*",
    "gh pr merge*",
    "gh pr close*",
    "gh pr create*",
    "gh issue*",
    "gh api*",
    "git reset*",
];
/// How long a slash command may take before its request is given up. Its run reports itself.
const COMMAND_LIMIT: Duration = Duration::from_secs(6 * 60 * 60);
/// How long after a turn OpenCode may still name the conversation before Kodem names it.
const TITLE_WAIT: Duration = Duration::from_secs(30);
/// The entry of a session's standing instructions that Kodem writes.
const INSTRUCTIONS: &str = "kodem";
/// Most messages that one page of a stored conversation returns.
const PAGE: usize = 200;
/// Most pages of one stored conversation that Kodem reads.
const PAGES: usize = 500;
const IMAGE_LIMIT: usize = 20 * 1024 * 1024;

/// How the request of a slash command ended: `Err` with OpenCode's refusal.
type CommandResult = std::result::Result<(), String>;

/// Returns the capabilities that do not depend on the server, so saved choices validate early.
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
        account_usage: false,
        usage_resets: false,
        ..RuntimeInfo::default()
    }
}

fn access_of(task: &Task) -> &str {
    task.provider_access.as_deref().unwrap_or(DEFAULT_ACCESS)
}

// The agent that runs a task's turns: OpenCode's plan agent in Plan mode, its build agent
// otherwise.
fn agent_of(task: &Task) -> &'static str {
    if access_of(task) == "plan" {
        "plan"
    } else {
        "build"
    }
}

fn rule(action: &str, resource: &str, effect: &str) -> Value {
    json!({"action":action,"resource":resource,"effect":effect})
}

// The approval rules of a task's session. OpenCode applies the last rule that matches.
fn rules(task: &Task) -> Vec<Value> {
    let mut rules = match access_of(task) {
        "allow" => vec![
            rule("edit", "*", "allow"),
            rule("shell", "*", "allow"),
            rule("external_directory", "*", "allow"),
        ],
        "edits" => vec![
            rule("edit", "*", "allow"),
            rule("shell", "*", "ask"),
            rule("external_directory", "*", "ask"),
        ],
        // The plan agent's own rules keep it from editing the project.
        "plan" => vec![
            rule("shell", "*", "ask"),
            rule("external_directory", "*", "ask"),
        ],
        _ => vec![
            rule("edit", "*", "ask"),
            rule("shell", "*", "ask"),
            rule("external_directory", "*", "ask"),
        ],
    };
    // A review, and a delegated task that reads its parent's checkout, change nothing.
    if task.kind == TaskKind::Review || task.shares_checkout() {
        rules.push(rule("edit", "*", "deny"));
        for command in BLOCKED_COMMANDS {
            rules.push(rule("shell", command, "deny"));
        }
    }
    rules
}

// The configuration that a server adds to the user's: no self-update, the compaction rule and
// Kodem's own tools.
fn environment(
    task: Option<&Task>,
    compact: Option<CompactRule>,
) -> Result<Vec<(&'static str, String)>> {
    let mut config = json!({"update":"disable"});
    if compact.is_some_and(|rule| !rule.enabled) {
        config["compaction"] = json!({"auto":false});
    }
    // A task that another task delegated starts no subagents of its own through Kodem.
    if let Some(task) = task
        && !task
            .lineage
            .as_ref()
            .is_some_and(|lineage| lineage.delegated)
    {
        let (command, arguments) = crate::mcp::command(&task.id)?;
        let mut line = vec![command.to_string_lossy().into_owned()];
        line.extend(arguments);
        config["mcp"] = json!({"servers":{crate::mcp::SERVER:{"type":"local","command":line}}});
    }
    Ok(vec![("OPENCODE_CONFIG_CONTENT", config.to_string())])
}

/// Returns OpenCode's reference to a model of the catalog, with the reasoning variant.
fn model_ref(id: &str, effort: Option<&str>) -> Result<Value> {
    let (provider, model) = id
        .split_once('/')
        .filter(|(provider, model)| !provider.is_empty() && !model.is_empty())
        .with_context(|| format!("\"{id}\" is not an OpenCode model. Choose a model again."))?;
    let mut reference = json!({"id":model,"providerID":provider});
    if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
        reference["variant"] = json!(effort);
    }
    Ok(reference)
}

// Starts a server and waits until it answers.
fn start(cwd: &Path, account: &Account, environment: &[(&str, String)]) -> Result<Server> {
    let server = Server::start(&std::env::current_exe()?, cwd, account, environment)?;
    server.ready(Duration::from_secs(30))?;
    Ok(server)
}

// A server that only reads the catalog, the account or stored conversations.
fn probe(cwd: &Path, account: &Account) -> Result<Server> {
    start(cwd, account, &environment(None, None)?)
}

// The folder of a probe that belongs to no task.
fn home() -> Result<PathBuf> {
    kodem_core::platform::home_dir()
}

// Opens the server's folder. OpenCode loads the commands and skills of a folder when it first
// opens it, and a listing before that is empty.
fn open_folder(client: &Client) {
    let _ = client.get(&client.located("/api/location"));
}

// The items of a listing. OpenCode wraps most of them in `data`.
fn items(listing: &Value) -> &[Value] {
    listing["data"]
        .as_array()
        .or_else(|| listing.as_array())
        .map_or(&[], Vec::as_slice)
}

/// The models of the providers that the account can use, and the context window of each.
struct Catalog {
    info: RuntimeInfo,
    windows: HashMap<String, u64>,
    connected: Vec<String>,
}

fn catalog(client: &Client) -> Result<Catalog> {
    let listed = client.get(&client.located("/api/model"))?;
    let providers = client
        .get(&client.located("/api/provider"))
        .unwrap_or_default();
    let provider_name = |id: &str| {
        items(&providers)
            .iter()
            .find(|provider| provider["id"] == id)
            .and_then(|provider| provider["name"].as_str())
            .unwrap_or(id)
            .to_owned()
    };
    let mut models = Vec::new();
    let mut windows = HashMap::new();
    let mut connected: Vec<String> = Vec::new();
    for model in items(&listed) {
        let (Some(provider_id), Some(model_id)) =
            (model["providerID"].as_str(), model["id"].as_str())
        else {
            continue;
        };
        if model["enabled"] == false || model["status"] == "deprecated" {
            continue;
        }
        let provider = provider_name(provider_id);
        if !connected.contains(&provider) {
            connected.push(provider.clone());
        }
        let id = format!("{provider_id}/{model_id}");
        if let Some(window) = model["limit"]["context"]
            .as_u64()
            .filter(|window| *window > 0)
        {
            windows.insert(id.clone(), window);
        }
        let mut efforts: Vec<String> = model["variants"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|variant| variant["id"].as_str())
            .map(str::to_owned)
            .collect();
        efforts.sort_by_key(|effort| {
            EFFORT_ORDER
                .iter()
                .position(|known| known == effort)
                .unwrap_or(EFFORT_ORDER.len())
        });
        let name = provider::model_name(model["name"].as_str().unwrap_or(model_id));
        models.push(Model {
            name: format!(
                "{} · {provider}",
                if name.is_empty() { model_id } else { &name }
            ),
            id,
            efforts,
            default_effort: String::new(),
            images: model["capabilities"]["input"]
                .as_array()
                .is_some_and(|input| input.iter().any(|kind| kind == "image")),
            effort_models: Default::default(),
            fast: None,
        });
    }
    fold_fast_models(&mut models);
    models.sort_by(|a, b| a.id.cmp(&b.id));
    // OpenCode's own choice for new sessions in this folder.
    let own = client
        .get(&client.located("/api/model/default"))
        .ok()
        .and_then(|default| {
            let model = &default["data"];
            Some(format!(
                "{}/{}",
                model["providerID"].as_str()?,
                model["id"].as_str()?
            ))
        });
    let version = client.get("/api/info").ok().and_then(|info| {
        info["version"]
            .as_str()
            .map(|version| format!("OpenCode {version}"))
    });
    Ok(Catalog {
        info: RuntimeInfo {
            default_model: provider::default_model(&models, &[], own.as_deref()),
            version,
            models,
            ..initial_runtime()
        },
        windows,
        connected,
    })
}

// OpenCode lists the fast mode of a model as a model of its own, named after the mode. It
// becomes the fast mode of the model it speeds up.
fn fold_fast_models(models: &mut Vec<Model>) {
    let fast: Vec<String> = models
        .iter()
        .filter_map(|model| {
            let base = model.id.strip_suffix(FAST_SUFFIX)?;
            models
                .iter()
                .any(|other| other.id == base)
                .then(|| model.id.clone())
        })
        .collect();
    for id in &fast {
        let base = &id[..id.len() - FAST_SUFFIX.len()];
        if let Some(model) = models.iter_mut().find(|model| model.id == base) {
            model.fast = Some(FastMode {
                id: Some(id.clone()),
                detail: String::new(),
            });
        }
    }
    models.retain(|model| !fast.contains(&model.id));
}

// Returns the catalog model that runs a task: its fast one while fast mode is on.
fn fast_model(host: &crate::Host, task: &Task) -> Option<String> {
    let model = provider::effective_model(host, task)?;
    if !task.fast {
        return Some(model);
    }
    host.runtimes
        .get(&Provider::OpenCode)
        .and_then(|runtime| runtime.models.iter().find(|known| known.id == model))
        .and_then(|known| known.fast.as_ref())
        .and_then(|fast| fast.id.clone())
        .or(Some(model))
}

// What the account can use: OpenCode needs no sign-in for its own free models, so an account
// counts as signed in while any provider offers a model.
fn status(connected: &[String]) -> AccountStatus {
    AccountStatus {
        signed_in: Some(!connected.is_empty()),
        organization: (!connected.is_empty()).then(|| connected.join(", ")),
        ..AccountStatus::default()
    }
}

fn publish(host: &Shared, account: &Account, catalog: &Catalog) {
    let mut info = catalog.info.clone();
    let mut h = host.lock().unwrap();
    if let Some(previous) = h.runtimes.get(&Provider::OpenCode) {
        for model in &mut info.models {
            if let Some(known) = previous.models.iter().find(|known| known.id == model.id) {
                model.default_effort = known.default_effort.clone();
            }
        }
    }
    h.runtimes.insert(Provider::OpenCode, info);
    accounts::observe(&mut h, account, status(&catalog.connected));
}

/// Reads the models of the account's providers, without a turn.
pub(super) fn discover(host: Shared, account: &Account, cwd: Option<PathBuf>) -> Result<()> {
    let cwd = match cwd {
        Some(cwd) => cwd,
        None => home()?,
    };
    let server = probe(&cwd, account)?;
    let catalog = catalog(server.client())?;
    drop(server);
    publish(&host, account, &catalog);
    Ok(())
}

/// Reads which providers the account can use. OpenCode reports no usage limits.
pub(super) fn account_status(
    _host: &Shared,
    account: &Account,
    _usage: bool,
) -> Result<AccountStatus> {
    let server =
        probe(&home()?, account).context("Could not run OpenCode. Install it to use it")?;
    Ok(status(&catalog(server.client())?.connected))
}

/// OpenCode reports no usage windows or credits.
pub(super) fn account_usage(
    _host: &Shared,
    _account: &Account,
    _idempotency_key: Option<&str>,
) -> Result<Response> {
    bail!("OpenCode reports no usage limits")
}

fn query(session: &mpsc::Sender<provider::Control>, path: &str) -> Result<Value> {
    let (reply, response) = mpsc::sync_channel(1);
    session.send(provider::Control::Query {
        method: "opencode/get",
        params: json!({"path":path}),
        reply,
    })?;
    response
        .recv_timeout(Duration::from_secs(35))?
        .map_err(anyhow::Error::msg)
}

fn location(
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
    Ok((home()?, None))
}

// Reads several resources of a folder from a live session, or from a server started for the
// reading.
fn read(
    account: &Account,
    cwd: &Path,
    owner: Option<mpsc::Sender<provider::Control>>,
    paths: &[&str],
) -> Result<Vec<Result<Value>>> {
    if let Some(owner) = owner {
        return Ok(paths.iter().map(|path| query(&owner, path)).collect());
    }
    let server = probe(cwd, account)?;
    let client = server.client();
    open_folder(client);
    Ok(paths
        .iter()
        .map(|path| client.get(&client.located(path)))
        .collect())
}

// The skills that OpenCode lists with a file, as the composer attaches them.
fn skills(listed: &Value) -> Vec<ComposerSkill> {
    let mut skills: Vec<ComposerSkill> = items(listed)
        .iter()
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
    skills
}

/// Returns OpenCode's slash commands and the skills it found for the checkout.
pub(super) fn composer_catalog(
    host: &Shared,
    account: &Account,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<ComposerCatalog> {
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
    let (cwd, owner) = location(host, task_id, project_id)?;
    let mut read = read(account, &cwd, owner, &["/api/command", "/api/skill"])?.into_iter();
    let (commands, listed) = (
        read.next().context("No command list")??,
        read.next().context("No skill list")?,
    );
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
        provider_commands: items(&commands)
            .iter()
            .take(1000)
            .filter_map(|command| {
                Some(ProviderCommand {
                    name: command["name"].as_str()?.to_owned(),
                    description: command["description"]
                        .as_str()
                        .unwrap_or_default()
                        .chars()
                        .take(600)
                        .collect(),
                    input_hint: None,
                })
            })
            .collect(),
        ..ComposerCatalog::default()
    };
    // Only a conversation that exists can be compacted.
    if started {
        catalog.commands.push(ComposerCommand::Compact);
    }
    match listed {
        Ok(listed) => catalog.skills = skills(&listed),
        Err(error) => catalog.errors.push(format!("{error:#}")),
    }
    Ok(catalog)
}

/// Lists instruction files, skills, agents and the MCP servers that OpenCode reports.
pub(super) fn resources(
    host: &Shared,
    account: &Account,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<Vec<Resource>> {
    use std::io::Read;
    let (cwd, owner) = location(host, task_id, project_id)?;
    let mut read = read(
        account,
        &cwd,
        owner,
        &["/api/config", "/api/skill", "/api/agent", "/api/mcp"],
    )?
    .into_iter();
    let mut next = || read.next().unwrap_or_else(|| Ok(Value::Null));
    let (config, listed, agents, servers) = (next(), next(), next(), next());
    let mut resources = Vec::new();
    let mut instructions: Vec<(PathBuf, &str)> = cwd
        .ancestors()
        .map(|directory| (directory.join("AGENTS.md"), "Project instructions"))
        .collect();
    // The folders of OpenCode's configuration hold the user's own instructions.
    for entry in config.iter().flat_map(|config| items(config).iter()) {
        let Some(path) = entry["path"].as_str().map(Path::new) else {
            continue;
        };
        let folder = if entry["type"] == "directory" {
            Some(path)
        } else {
            path.parent()
        };
        if let Some(folder) = folder {
            let file = folder.join("AGENTS.md");
            if !instructions.iter().any(|(known, _)| known == &file) {
                instructions.push((file, "User instructions"));
            }
        }
    }
    for (path, scope) in instructions {
        if path.is_file() {
            let mut detail = String::new();
            let _ = std::fs::File::open(&path)
                .and_then(|file| file.take(8192).read_to_string(&mut detail));
            resources.push(Resource {
                name: "AGENTS.md".into(),
                source: path.display().to_string(),
                scope: scope.into(),
                status: "Discovered file".into(),
                detail: detail.chars().take(2000).collect(),
            });
        }
    }
    for skill in listed.as_ref().map(skills).unwrap_or_default() {
        resources.push(Resource {
            name: skill.name,
            source: skill.path.display().to_string(),
            scope: "OpenCode skill".into(),
            status: "Discovered file".into(),
            detail: skill.description,
        });
    }
    for agent in agents.iter().flat_map(|agents| items(agents).iter()) {
        if agent["hidden"] == true {
            continue;
        }
        resources.push(Resource {
            name: agent["name"]
                .as_str()
                .or_else(|| agent["id"].as_str())
                .unwrap_or("Agent")
                .into(),
            source: "OpenCode".into(),
            scope: if agent["mode"] == "subagent" {
                "Subagent".into()
            } else {
                "Agent".into()
            },
            status: "Available".into(),
            detail: agent["description"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(2000)
                .collect(),
        });
    }
    match servers {
        Ok(servers) => {
            for server in items(&servers) {
                resources.push(Resource {
                    name: server["name"]
                        .as_str()
                        .or_else(|| server["id"].as_str())
                        .unwrap_or("MCP server")
                        .into(),
                    source: "OpenCode".into(),
                    scope: "Provider-owned MCP".into(),
                    status: format!(
                        "Status: {}",
                        server["status"]
                            .as_str()
                            .or_else(|| server["status"]["type"].as_str())
                            .unwrap_or("unknown")
                    ),
                    detail: server["error"]
                        .as_str()
                        .unwrap_or("This inventory is not a tool execution check.")
                        .chars()
                        .take(2000)
                        .collect(),
                });
            }
        }
        Err(error) => resources.push(Resource {
            name: "MCP servers".into(),
            source: "OpenCode".into(),
            scope: "Provider-owned MCP".into(),
            status: "Unavailable".into(),
            detail: format!("{error:#}"),
        }),
    }
    Ok(resources)
}

// Builds the text and the images of a prompt.
fn prompt_parts(message: &OutgoingMessage) -> Result<(String, Vec<Value>)> {
    let text = provider::prompt_for(message)?;
    let mut files = Vec::new();
    for attachment in &message.attachments {
        if attachment.kind != AttachmentKind::Image {
            continue;
        }
        let bytes = std::fs::read(&attachment.path).context("Could not read the attached image")?;
        ensure!(bytes.len() <= IMAGE_LIMIT, "Image exceeds 20 MiB");
        let media = image::guess_format(&bytes)
            .context("Unrecognized image format")?
            .to_mime_type();
        ensure!(
            matches!(
                media,
                "image/png" | "image/jpeg" | "image/gif" | "image/webp"
            ),
            "OpenCode accepts PNG, JPEG, GIF and WebP images"
        );
        files.push(
            json!({"name":attachment.name,"uri":format!("data:{media};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes))}),
        );
    }
    ensure!(
        !text.trim().is_empty() || !files.is_empty(),
        "Write a prompt or attach an image"
    );
    Ok((text, files))
}

/// Reads all stored messages of a session, oldest first.
fn messages(client: &Client, session: &str) -> Result<Vec<Value>> {
    let mut all = Vec::new();
    let mut path = format!("/api/session/{session}/message?order=asc&limit={PAGE}");
    for _ in 0..PAGES {
        let page = client.get(&path)?;
        let listed = items(&page);
        all.extend(listed.iter().cloned());
        let next = page["cursor"]["next"].as_str();
        match next {
            Some(cursor) if listed.len() >= PAGE => {
                path = format!("/api/session/{session}/message?cursor={cursor}&limit={PAGE}");
            }
            _ => break,
        }
    }
    Ok(all)
}

/// Applies stored messages of a session to a transcript. With `agents`, the sessions of its
/// agents follow, in their own transcripts.
fn replay(
    host: &Shared,
    client: &Client,
    transcript: &mut Transcript,
    session: &str,
    stored: &[Value],
    agents: bool,
) -> Result<()> {
    for message in stored {
        transcript.replay(host, session, message)?;
    }
    if !agents {
        return Ok(());
    }
    let children = client.get(&format!("/api/session?parentID={session}&limit={PAGE}"))?;
    for child in items(&children) {
        let Some(id) = child["id"].as_str() else {
            continue;
        };
        transcript.ingest(
            host,
            &json!({"type":"session.created","data":{"sessionID":id,"parentID":session,
                "title":child["title"]}}),
        )?;
        let stored = messages(client, id)?;
        replay(host, client, transcript, id, &stored, true)?;
        let state = match child["outcome"].as_str() {
            Some("failed") => RunState::Failed,
            Some("interrupted") => RunState::Interrupted,
            _ => RunState::Completed,
        };
        transcript.agent_finished(host, id, state)?;
    }
    Ok(())
}

/// Resolves uncertain delivery from the messages that OpenCode stored for the conversation.
///
/// Each prompt carries its submission identity in its metadata, so only that exact identity
/// counts as proof of receipt.
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
    let session = task.provider_thread.clone().context("OpenCode never confirmed a conversation for this task, so the prompt cannot be reconciled automatically. Retain this task and create a new one.")?;
    let server = probe(&task.cwd, &account)?;
    let client = server.client();
    let active = client.get("/api/session/active")?;
    ensure!(
        active["data"][&session].is_null(),
        "OpenCode is still working in this conversation. Reconcile again in a moment."
    );
    let stored = messages(client, &session)?;
    let position = stored
        .iter()
        .position(|message| {
            message["type"] == "user" && message["metadata"]["kodem"] == submission.id.as_str()
        })
        .context("The OpenCode conversation does not contain this exact submission. Delivery remains unknown; no text was resent.")?;
    let receipt = stored[position]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let later = &stored[position + 1..];
    let finished = later
        .iter()
        .rev()
        .find(|message| message["type"] == "idle")
        .is_some_and(|message| message["outcome"] == "succeeded");
    let mut transcript = Transcript::new(task_id, &task.cwd, false);
    transcript.set_main(&session);
    replay(host, client, &mut transcript, &session, later, false)?;
    transcript.finish(host)?;
    drop(server);
    {
        let h = host.lock().unwrap();
        h.store
            .confirm_submission(&submission.id, &format!("opencode:{receipt}"))?;
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
            Some("The connection to OpenCode was lost during this turn. The output it stored was restored; send a message to continue.".to_owned()),
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

enum AskKind {
    Permission,
    /// A form of questions, with OpenCode's fields.
    Question(Value),
}

/// A permission or a question of OpenCode that waits for the user.
struct Ask {
    kind: AskKind,
    /// The session that asked.
    session: String,
    /// The agent session that asked; absent for the task's own session.
    agent: Option<String>,
}

/// What the session of a task has, as Kodem last set it.
#[derive(Default)]
struct Applied {
    access: Option<String>,
    agent: Option<String>,
    model: Option<Value>,
    instructions: Option<String>,
}

struct Owner<'a> {
    host: &'a Shared,
    task_id: &'a str,
    server: Server,
    events: mpsc::Receiver<std::result::Result<Value, String>>,
    /// The account and the compaction rule that the server started with.
    account: Account,
    compact: CompactRule,
    /// OpenCode's session of the task, once it exists.
    session: Option<String>,
    applied: Applied,
    transcript: Transcript,
    /// OpenCode works on a turn or a compaction of the task.
    running: bool,
    /// OpenCode reported that the current run began.
    busy: bool,
    interrupting: bool,
    /// Steered prompts that OpenCode accepted but did not give the model yet. OpenCode starts
    /// another run for one that arrives after its run ended, and that run is part of the turn.
    undelivered: HashSet<String>,
    /// Names of OpenCode's commands, which a prompt runs when it starts with one.
    commands: Vec<String>,
    windows: HashMap<String, u64>,
    /// Cost of the session as OpenCode last reported it.
    cost: Option<f64>,
    asks: HashMap<String, Ask>,
    /// A compaction that the host asked for runs; `true` once OpenCode reported its summary.
    compacting: Option<bool>,
    /// The stored message of the compaction that runs.
    compaction: Option<String>,
    commanded: (mpsc::Sender<CommandResult>, mpsc::Receiver<CommandResult>),
    /// When Kodem names a conversation that OpenCode has not named.
    title_due: Option<Instant>,
}

/// Owns one resumable OpenCode conversation and keeps controls responsive while it works.
pub(super) fn run(
    host: Shared,
    task_id: &str,
    controls: mpsc::Receiver<provider::Control>,
) -> Result<()> {
    let (task, account, compact) = {
        let h = host.lock().unwrap();
        let task = h.store.task(task_id)?;
        let account = accounts::for_task(&h, &task)?;
        let compact = provider::compact_rule(&h, &task)?;
        (task, account, compact)
    };
    let server = start(
        &task.cwd,
        &account,
        &environment(Some(&task), Some(compact))?,
    )?;
    let events = server.client().events()?;
    let mut owner = Owner {
        host: &host,
        task_id,
        server,
        events,
        account,
        compact,
        session: None,
        applied: Applied::default(),
        transcript: Transcript::new(task_id, &task.cwd, false),
        running: false,
        busy: false,
        interrupting: false,
        undelivered: HashSet::new(),
        commands: Vec::new(),
        windows: HashMap::new(),
        cost: None,
        asks: HashMap::new(),
        compacting: None,
        compaction: None,
        commanded: mpsc::channel(),
        title_due: None,
    };
    owner.load();
    if let Some(session) = &task.provider_thread {
        owner.transcript.set_main(session);
    }
    let mut branch_refresh = Instant::now();
    loop {
        while let Ok(control) = controls.try_recv() {
            match control {
                provider::Control::Close => return Ok(()),
                provider::Control::Wake => {}
                provider::Control::Interrupt => owner.interrupt()?,
                provider::Control::Compact => owner.compact()?,
                provider::Control::Answer { id, response } => owner.answer(id, response)?,
                provider::Control::Query {
                    method,
                    params,
                    reply,
                } => {
                    let client = owner.client();
                    let result = match (method, params["path"].as_str()) {
                        ("opencode/get", Some(path)) => client
                            .get(&client.located(path))
                            .map_err(|error| format!("{error:#}")),
                        _ => Err("This operation is not supported by OpenCode".into()),
                    };
                    let _ = reply.send(result);
                }
            }
        }
        match owner.events.recv_timeout(Duration::from_millis(40)) {
            Ok(Ok(event)) => owner.event(&event)?,
            Ok(Err(error)) => bail!("{error}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("OpenCode ended its event stream"),
        }
        while let Ok(result) = owner.commanded.1.try_recv() {
            owner.command_ended(result)?;
        }
        owner.dispatch()?;
        owner.name_conversation()?;
        owner.transcript.write_due(&host)?;
        if branch_refresh.elapsed() >= Duration::from_secs(5) {
            branch_refresh = Instant::now();
            // Only a coding task follows the branch of its checkout.
            if owner.running && task.kind == TaskKind::Coding {
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
}

impl Owner<'_> {
    fn task(&self) -> Result<Task> {
        self.host.lock().unwrap().store.task(self.task_id)
    }

    fn client(&self) -> Client {
        self.server.client().clone()
    }

    // Reads the context windows of the models and the names of the commands. Both only
    // improve what Kodem shows, so a failure leaves them empty.
    fn load(&mut self) {
        let client = self.client();
        open_folder(&client);
        if let Ok(catalog) = catalog(&client) {
            publish(self.host, &self.account, &catalog);
            self.windows = catalog.windows;
        }
        self.commands = client
            .get(&client.located("/api/command"))
            .map(|commands| {
                items(&commands)
                    .iter()
                    .filter_map(|command| command["name"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
    }

    fn main(&self, session: Option<&str>) -> bool {
        session.is_some() && self.session.as_deref() == session
    }

    fn ours(&self, session: Option<&str>) -> bool {
        self.main(session) || session.is_some_and(|session| self.transcript.is_agent(session))
    }

    fn event(&mut self, event: &Value) -> Result<()> {
        self.transcript.ingest(self.host, event)?;
        let data = &event["data"];
        let session = data["sessionID"].as_str();
        match event["type"].as_str().unwrap_or_default() {
            "permission.asked" if self.ours(session) => self.permission(data)?,
            "form.created" if self.ours(data["form"]["sessionID"].as_str()) => {
                self.question(&data["form"])?;
            }
            "permission.replied" | "form.replied" | "form.cancelled" => {
                if let Some(id) = data["requestID"].as_str().or_else(|| data["id"].as_str())
                    && self.asks.remove(id).is_some()
                {
                    self.resolve_request(&json!(id), None)?;
                }
            }
            "session.inbox.delivered" if self.main(session) => {
                if let Some(id) = data["inboxID"].as_str() {
                    self.undelivered.remove(id);
                }
            }
            "session.execution.started" if self.main(session) => self.busy = true,
            kind @ ("session.execution.succeeded"
            | "session.execution.failed"
            | "session.execution.interrupted")
                if self.main(session) =>
            {
                self.execution_ended(kind, data)?;
            }
            "session.compaction.started" if self.main(session) => {
                self.compaction = data["inputID"].as_str().map(str::to_owned);
            }
            "session.compaction.ended" if self.main(session) => self.summarized(data)?,
            "session.renamed" if self.main(session) => self.renamed(data["title"].as_str())?,
            "session.usage.updated" if self.main(session) => {
                if let Some(cost) = data["cost"]
                    .as_f64()
                    .filter(|cost| cost.is_finite() && *cost >= 0.0)
                {
                    self.cost = Some(cost);
                }
            }
            _ => {}
        }
        Ok(())
    }

    // The summary of a compaction replaced the conversation. The context that the next request
    // starts with is the summary.
    fn summarized(&mut self, data: &Value) -> Result<()> {
        self.transcript
            .clear_activity(self.host, transcript::COMPACTING);
        if let Some(compacting) = &mut self.compacting {
            *compacting = true;
        }
        let stored = match (&self.session, self.compaction.take()) {
            (Some(session), Some(id)) => self
                .client()
                .get(&format!("/api/session/{session}/message/{id}"))
                .ok(),
            _ => None,
        };
        let tokens = stored
            .as_ref()
            .and_then(|stored| stored["data"]["tokens"]["output"].as_u64())
            .filter(|tokens| *tokens > 0);
        // Without a count, the summary's length gives an estimate of about four characters for
        // each token.
        let used = tokens.or_else(|| {
            data["text"]
                .as_str()
                .map(|text| (text.chars().count() / 4) as u64)
                .filter(|tokens| *tokens > 0)
        });
        if let Some(used) = used {
            self.transcript.used = Some(used);
            self.context_usage()?;
        }
        Ok(())
    }

    // The title that OpenCode chose.
    fn renamed(&mut self, title: Option<&str>) -> Result<()> {
        let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) else {
            return Ok(());
        };
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        if task.title_pending {
            task.title = title.chars().take(200).collect();
            task.title_pending = false;
            h.store.save_task(&task)?;
        }
        Ok(())
    }

    // Names a conversation that OpenCode did not name after its first turn by its first prompt.
    fn name_conversation(&mut self) -> Result<()> {
        if self.title_due.is_none_or(|due| Instant::now() < due) {
            return Ok(());
        }
        self.title_due = None;
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        if !task.title_pending {
            return Ok(());
        }
        let (messages, _) = h.store.messages(self.task_id, None)?;
        if let Some(prompt) = messages
            .iter()
            .find(|message| message.kind == MessageKind::User)
            .and_then(|message| message.text.lines().find(|line| !line.trim().is_empty()))
        {
            task.title = prompt.trim().chars().take(80).collect();
            task.title_pending = false;
            h.store.save_task(&task)?;
        }
        Ok(())
    }

    fn context_usage(&self) -> Result<()> {
        let h = self.host.lock().unwrap();
        let mut task = h.store.task(self.task_id)?;
        let model = self
            .transcript
            .model
            .clone()
            .or_else(|| provider::effective_model(&h, &task));
        let window = model.and_then(|model| self.windows.get(&model).copied());
        let cost = self.cost.map(|amount| UsageCost {
            amount,
            currency: "USD".into(),
        });
        let previous = task.context_usage.take();
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
        h.store.save_task(&task)
    }

    // A run of the task's session ended: a turn, part of a turn or a compaction.
    fn execution_ended(&mut self, kind: &str, data: &Value) -> Result<()> {
        if let Some(done) = self.compacting.take() {
            return self.compacted(done, kind, data);
        }
        if !self.running {
            return Ok(());
        }
        // A steered prompt that OpenCode took after its run ended starts the next run.
        if kind == "session.execution.succeeded" && !self.undelivered.is_empty() {
            self.busy = false;
            return Ok(());
        }
        let outcome = match kind {
            _ if self.interrupting || kind == "session.execution.interrupted" => TurnOutcome {
                state: RunState::Interrupted,
                error: None,
            },
            "session.execution.failed" => TurnOutcome {
                state: RunState::Failed,
                error: Some(
                    transcript::error_text(&data["error"])
                        .unwrap_or_else(|| "OpenCode reported a failure".into()),
                ),
            },
            _ => TurnOutcome {
                state: RunState::Completed,
                error: None,
            },
        };
        self.end_turn(outcome)
    }

    fn end_turn(&mut self, outcome: TurnOutcome) -> Result<()> {
        self.transcript.finish(self.host)?;
        self.context_usage()?;
        // OpenCode waits for its agents before its run ends, so an agent that still counts as
        // working has stopped.
        self.transcript.stop_agents(self.host)?;
        self.running = false;
        self.busy = false;
        self.interrupting = false;
        self.undelivered.clear();
        if self.task()?.title_pending {
            self.title_due = Some(Instant::now() + TITLE_WAIT);
        }
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::TurnEnded { outcome }),
        )?;
        Ok(())
    }

    // Ends a compaction the host requested. The task returns to its earlier state and no turn
    // completes, so nothing is captured, released or announced.
    fn compacted(&mut self, done: bool, kind: &str, data: &Value) -> Result<()> {
        let failure = if done && kind == "session.execution.succeeded" {
            None
        } else if self.interrupting || kind == "session.execution.interrupted" {
            Some("it was stopped.".to_owned())
        } else {
            Some(
                transcript::error_text(&data["error"])
                    .unwrap_or_else(|| "OpenCode reported no result".into()),
            )
        };
        self.running = false;
        self.busy = false;
        self.interrupting = false;
        self.transcript.finish(self.host)?;
        self.transcript
            .clear_activity(self.host, transcript::COMPACTING);
        self.context_usage()?;
        provider::end_compaction(self.host, self.task_id, failure)
    }

    // A slash command's request ended. A refused one started no run, so it ends the turn.
    fn command_ended(&mut self, result: CommandResult) -> Result<()> {
        match result {
            Err(error) if self.running && !self.busy => self.end_turn(TurnOutcome {
                state: RunState::Failed,
                error: Some(error),
            }),
            _ => Ok(()),
        }
    }

    fn dispatch(&mut self) -> Result<()> {
        let task = self.task()?;
        if self.running {
            // Only input bound to the running turn is eligible while the task is active.
            if task.active_turn.is_some()
                && self.compacting.is_none()
                && let Some(message) = provider::claim_next(self.host, self.task_id)?
            {
                self.send(message, true)?;
            }
            return Ok(());
        }
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
        // The check between turns. OpenCode's own limit covers a turn that is running.
        if provider::begin_due_compaction(self.host, self.task_id)? {
            return self.compact();
        }
        if let Some(message) = provider::claim_next(self.host, self.task_id)? {
            self.send(message, false)?;
        }
        Ok(())
    }

    // Brings the server and the session in line with the task's current choices before the
    // session receives input. The account and the compaction rule are fixed when a server
    // starts, so a change needs a new server for the same conversation.
    fn prepare(&mut self, task: &Task) -> Result<()> {
        let (account, compact, model) = {
            let h = self.host.lock().unwrap();
            (
                accounts::for_task(&h, task)?,
                provider::compact_rule(&h, task)?,
                fast_model(&h, task),
            )
        };
        if account != self.account
            || compact.enabled != self.compact.enabled
            || !self.server.running()
        {
            let server = start(
                &task.cwd,
                &account,
                &environment(Some(task), Some(compact))?,
            )?;
            self.events = server.client().events()?;
            self.server = server;
            self.account = account;
            self.compact = compact;
            self.load();
        }
        let model = model
            .map(|model| model_ref(&model, task.effort.as_deref()))
            .transpose()?;
        let client = self.client();
        let session = match (&self.session, &task.provider_thread) {
            (Some(session), _) => session.clone(),
            (None, Some(stored)) => {
                let info = client
                    .get(&format!("/api/session/{stored}"))
                    .context("OpenCode no longer has the conversation of this task under its account. Start a new task to continue.")?;
                // The rules and the instructions of the session are not known, so the task's
                // are set again.
                self.applied = Applied {
                    agent: info["data"]["agent"].as_str().map(str::to_owned),
                    model: Some(info["data"]["model"].clone()).filter(|model| !model.is_null()),
                    ..Applied::default()
                };
                stored.clone()
            }
            (None, None) => {
                // The tag tells Kodem's import that Kodem started this session.
                let mut body = json!({"location":{"directory":task.cwd},
                    "permissions":rules(task),"agent":agent_of(task),
                    "metadata":{"kodem":{"task":task.id}}});
                if let Some(model) = &model {
                    body["model"] = model.clone();
                }
                let created = client.post("/api/session", &body)?;
                let id = created["data"]["id"]
                    .as_str()
                    .context("OpenCode created a session without an identity")?
                    .to_owned();
                self.applied = Applied {
                    access: Some(access_of(task).to_owned()),
                    agent: Some(agent_of(task).to_owned()),
                    model: model.clone(),
                    instructions: None,
                };
                let h = self.host.lock().unwrap();
                let mut current = h.store.task(self.task_id)?;
                current.provider_thread = Some(id.clone());
                h.store.save_task(&current)?;
                id
            }
        };
        self.transcript.set_main(&session);
        self.session = Some(session.clone());
        if self.applied.access.as_deref() != Some(access_of(task)) {
            client.request(
                "PATCH",
                &format!("/api/session/{session}"),
                Some(&json!({"permissions":rules(task)})),
                kodem_core::providers::opencode::REQUEST_LIMIT,
            )?;
            self.applied.access = Some(access_of(task).to_owned());
        }
        if self.applied.agent.as_deref() != Some(agent_of(task)) {
            client.post(
                &format!("/api/session/{session}/agent"),
                &json!({"agent":agent_of(task)}),
            )?;
            self.applied.agent = Some(agent_of(task).to_owned());
        }
        if let Some(model) = model
            && self.applied.model.as_ref() != Some(&model)
        {
            client.post(
                &format!("/api/session/{session}/model"),
                &json!({"model":model}),
            )?;
            self.applied.model = Some(model);
        }
        let instructions = provider::instructions(task);
        if !instructions.is_empty() && self.applied.instructions.as_ref() != Some(&instructions) {
            client.request(
                "PUT",
                &format!("/api/experimental/session/{session}/instructions/entries/{INSTRUCTIONS}"),
                Some(&json!({"value":instructions})),
                kodem_core::providers::opencode::REQUEST_LIMIT,
            )?;
            self.applied.instructions = Some(instructions);
        }
        Ok(())
    }

    // Asks OpenCode to summarize the conversation for the compaction recorded on the host.
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
        // A turn runs; it owns the task now.
        if self.running {
            self.host.lock().unwrap().compactions.remove(self.task_id);
            return Ok(());
        }
        let task = self.task()?;
        self.prepare(&task)?;
        let session = self
            .session
            .clone()
            .context("There is nothing to compact yet")?;
        if let Err(error) = self
            .client()
            .post(&format!("/api/session/{session}/compact"), &json!({}))
        {
            return provider::end_compaction(self.host, self.task_id, Some(format!("{error:#}")));
        }
        self.compacting = Some(false);
        self.compaction = None;
        self.running = true;
        self.busy = false;
        self.interrupting = false;
        Ok(())
    }

    fn send(&mut self, message: OutgoingMessage, steer: bool) -> Result<()> {
        let (text, files) = match prompt_parts(&message) {
            Ok(parts) => parts,
            Err(error) => {
                // Nothing reached OpenCode, so the message is refused, not uncertain.
                return self.refused(&message, steer, error.to_string());
            }
        };
        let task = self.task()?;
        let session = self
            .session
            .clone()
            .context("The OpenCode session is not ready")?;
        let turn = if steer {
            task.active_turn.clone().unwrap_or_default()
        } else {
            format!("opencode:{}", message.id)
        };
        let client = self.client();
        // A prompt that starts a turn with one of OpenCode's commands runs that command.
        let command = (!steer)
            .then(|| message.text.trim().strip_prefix('/'))
            .flatten()
            .and_then(|line| {
                let (name, arguments) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
                self.commands
                    .iter()
                    .any(|known| known == name)
                    .then(|| (name.to_owned(), arguments.trim().to_owned()))
            });
        if let Some((name, arguments)) = command {
            let body = json!({"name":name,"text":arguments,"files":files,"delivery":"steer"});
            let sender = self.commanded.0.clone();
            // OpenCode may answer only when the command's run ends.
            std::thread::spawn(move || {
                let result = client
                    .request(
                        "POST",
                        &format!("/api/session/{session}/command"),
                        Some(&body),
                        COMMAND_LIMIT,
                    )
                    .map(drop)
                    .map_err(|error| format!("{error:#}"));
                let _ = sender.send(result);
            });
        } else {
            let body = json!({"text":text,"files":files,"metadata":{"kodem":message.id},
                "delivery":"steer"});
            match client.post(&format!("/api/session/{session}/prompt"), &body) {
                Ok(accepted) => {
                    if steer && let Some(id) = accepted["data"]["id"].as_str() {
                        self.undelivered.insert(id.to_owned());
                    }
                }
                // OpenCode refused the prompt, so it never reached the conversation.
                Err(error) if error.is::<Refused>() => {
                    return self.refused(&message, steer, format!("{error:#}"));
                }
                Err(error) => return Err(error),
            }
        }
        if !steer {
            self.running = true;
            self.busy = false;
            self.interrupting = false;
            self.undelivered.clear();
            // OpenCode accepted the prompt, so its turn starts. This local identity links
            // steering to the turn; the stored prompt's identity is the evidence.
            orchestrator::dispatch(
                self.host,
                self.task_id,
                Input::Signal(Signal::TurnStarted {
                    turn: Some(turn.clone()),
                    branch: None,
                }),
            )?;
        }
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::Delivered {
                submission: message.id.clone(),
                turn,
                steers: steer,
            }),
        )?;
        Ok(())
    }

    // Reports a message that did not reach OpenCode.
    fn refused(&mut self, message: &OutgoingMessage, steer: bool, error: String) -> Result<()> {
        orchestrator::dispatch(
            self.host,
            self.task_id,
            Input::Signal(Signal::DeliveryFailed {
                submission: message.id.clone(),
                steers: steer,
                unknown: false,
                error,
            }),
        )
        .map(drop)
    }

    // Records a request that waits for the user and moves the task to waiting.
    fn wait_for_user(&mut self, id: &str, method: &str, params: Value, ask: Ask) -> Result<()> {
        {
            let mut h = self.host.lock().unwrap();
            let requests = h.requests.entry(self.task_id.into()).or_default();
            if requests.iter().any(|known| known.id == id) {
                return Ok(());
            }
            requests.push(InputRequest {
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

    // The agent session among the task's sessions; `None` for the task's own.
    fn agent_session(&self, session: &str) -> Option<String> {
        (!self.main(Some(session))).then(|| session.to_owned())
    }

    fn permission(&mut self, data: &Value) -> Result<()> {
        let (Some(id), Some(session)) = (data["id"].as_str(), data["sessionID"].as_str()) else {
            return Ok(());
        };
        if self.interrupting {
            self.client().post(
                &format!("/api/session/{session}/permission/{id}/reply"),
                &json!({"decision":"reject","message":"The user stopped this turn."}),
            )?;
            return Ok(());
        }
        let agent = self.agent_session(session);
        let action = data["action"].as_str().unwrap_or("tool");
        let resources: Vec<&str> = data["resources"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let relative = |path: &str| {
            self.transcript
                .roots()
                .iter()
                .find_map(|root| Path::new(path).strip_prefix(root).ok())
                .map_or_else(|| path.to_owned(), |path| path.display().to_string())
        };
        let files = resources
            .iter()
            .map(|resource| relative(resource))
            .collect::<Vec<_>>()
            .join(", ");
        let reason = match action {
            "edit" => format!("OpenCode wants to change {files}"),
            "read" => format!("OpenCode wants to read {files}"),
            "shell" => "OpenCode wants to run a command".to_owned(),
            "external_directory" => "OpenCode wants to use a folder outside the project".to_owned(),
            "webfetch" => "OpenCode wants to read a web page".to_owned(),
            "websearch" => "OpenCode wants to search the web".to_owned(),
            "subagent" => "OpenCode wants to start an agent".to_owned(),
            other => format!("OpenCode wants to use {other}"),
        };
        let patches: Vec<&str> = data["metadata"]["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|file| file["patch"].as_str())
            .collect();
        let command = if patches.is_empty() {
            resources.join("\n")
        } else {
            patches.join("\n")
        };
        let save: Vec<&str> = data["save"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut available = vec!["accept"];
        let mut labels = json!({"accept":"Allow once","decline":"Deny","cancel":"Deny and stop"});
        if !save.is_empty() {
            // A rule for everything names the kind of action instead of its pattern.
            let scope = if save == ["*"] {
                match action {
                    "edit" => "all edits".to_owned(),
                    "shell" => "all commands".to_owned(),
                    "external_directory" => "all folders outside the project".to_owned(),
                    other => format!("all {other} requests"),
                }
            } else {
                save.join(", ").chars().take(60).collect()
            };
            available.push("acceptForSession");
            labels["acceptForSession"] = json!(format!("Always allow {scope}"));
        }
        available.extend(["decline", "cancel"]);
        let mut params = json!({"reason":reason,"command":command,"tool":action,
            "availableDecisions":available,"decisionLabels":labels});
        if let Some(agent) = &agent {
            params["threadId"] = json!(agent);
        }
        self.wait_for_user(
            id,
            "opencode/requestApproval",
            params,
            Ask {
                kind: AskKind::Permission,
                session: session.to_owned(),
                agent,
            },
        )
    }

    fn question(&mut self, form: &Value) -> Result<()> {
        let (Some(id), Some(session)) = (form["id"].as_str(), form["sessionID"].as_str()) else {
            return Ok(());
        };
        let agent = self.agent_session(session);
        let questions: Vec<Value> = form["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|field| field["hidden"] != true && field["type"] != "external")
            .filter_map(|field| {
                let key = field["key"].as_str()?;
                let text = field["description"]
                    .as_str()
                    .or_else(|| field["title"].as_str())?;
                let mut entry = json!({"id":key,"question":text,
                    "multiSelect":field["type"] == "multiselect"});
                if let Some(title) = field["title"].as_str()
                    && title != text
                {
                    entry["header"] = json!(title);
                }
                if let Some(options) = field["options"].as_array() {
                    entry["options"] = json!(
                        options
                            .iter()
                            .map(|option| json!({"label":option["label"],
                            "description":option["description"].as_str().unwrap_or_default()}))
                            .collect::<Vec<_>>()
                    );
                }
                Some(entry)
            })
            .collect();
        if questions.is_empty() {
            self.client().request(
                "DELETE",
                &format!("/api/session/{session}/form/{id}"),
                None,
                kodem_core::providers::opencode::REQUEST_LIMIT,
            )?;
            return Ok(());
        }
        let mut params = json!({"questions":questions});
        if let Some(agent) = &agent {
            params["threadId"] = json!(agent);
        }
        self.wait_for_user(
            id,
            "opencode/requestUserInput",
            params,
            Ask {
                kind: AskKind::Question(form["fields"].clone()),
                session: session.to_owned(),
                agent,
            },
        )
    }

    fn answer(&mut self, id: Value, response: Value) -> Result<()> {
        let Some(request) = id.as_str().map(str::to_owned) else {
            return Ok(());
        };
        let Some(ask) = self.asks.remove(&request) else {
            return Ok(());
        };
        let client = self.client();
        let session = &ask.session;
        match &ask.kind {
            AskKind::Question(fields) => {
                let mut answer = serde_json::Map::new();
                for field in fields.as_array().into_iter().flatten() {
                    let Some(key) = field["key"].as_str() else {
                        continue;
                    };
                    // The user picks labels; OpenCode takes the values behind them, or the
                    // user's own words.
                    let chosen: Vec<Value> = response["answers"][key]["answers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(|label| {
                            field["options"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .find(|option| option["label"] == label)
                                .map_or_else(|| json!(label), |option| option["value"].clone())
                        })
                        .collect();
                    let value = match field["type"].as_str() {
                        Some("multiselect") => json!(chosen),
                        _ => match chosen.into_iter().next() {
                            Some(value) => value,
                            None => continue,
                        },
                    };
                    answer.insert(key.to_owned(), value);
                }
                client.post(
                    &format!("/api/session/{session}/form/{request}/reply"),
                    &json!({"answer":answer}),
                )?;
            }
            AskKind::Permission => {
                // The desktop sends text typed beside a decline as `message`.
                let feedback = response["message"]
                    .as_str()
                    .map(str::trim)
                    .filter(|feedback| !feedback.is_empty());
                let reply = match response["decision"].as_str() {
                    Some("accept") => json!({"decision":"once"}),
                    Some("acceptForSession") => json!({"decision":"always"}),
                    Some("decline") => json!({"decision":"reject",
                        "message":feedback.unwrap_or("The user declined this action.")}),
                    Some("cancel") => {
                        self.interrupting = true;
                        json!({"decision":"reject","message":"The user stopped this turn."})
                    }
                    _ => bail!("Unsupported decision for this OpenCode request"),
                };
                client.post(
                    &format!("/api/session/{session}/permission/{request}/reply"),
                    &reply,
                )?;
                if self.interrupting
                    && let Some(main) = &self.session
                {
                    client.post(&format!("/api/session/{main}/interrupt"), &json!({}))?;
                }
            }
        }
        self.resolve_request(&id, Some((&ask, &response)))
    }

    // Removes a request that was answered or withdrawn and lets the task continue.
    fn resolve_request(&mut self, id: &Value, answered: Option<(&Ask, &Value)>) -> Result<()> {
        let mut h = self.host.lock().unwrap();
        let request = h.requests.get_mut(self.task_id).and_then(|requests| {
            let position = requests.iter().position(|request| &request.id == id)?;
            Some(requests.remove(position))
        });
        let mut task = h.store.task(self.task_id)?;
        if let (Some(request), Some((ask, response))) = (&request, answered)
            && matches!(ask.kind, AskKind::Question(_))
        {
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
                id: format!("opencode:answer:{}", id.as_str().unwrap_or_default()),
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
        let agent = request
            .as_ref()
            .and_then(|request| request.params["threadId"].as_str());
        if let Some(known) = task
            .subagents
            .iter_mut()
            .find(|known| Some(known.thread_id.as_str()) == agent)
            && known.state == RunState::Waiting
        {
            known.state = RunState::Running;
            h.store.save_task(&task)?;
        }
        if task.state == RunState::Waiting
            && h.requests
                .get(self.task_id)
                .is_none_or(|requests| requests.is_empty())
        {
            task.state = RunState::Running;
            h.save_task_with_notification(&task)?;
        }
        Ok(())
    }

    fn interrupt(&mut self) -> Result<()> {
        let active_agents = self
            .task()?
            .subagents
            .iter()
            .any(|agent| agent.state.active());
        if !self.running && !active_agents {
            return Ok(());
        }
        let client = self.client();
        let asks: Vec<(String, Ask)> = self.asks.drain().collect();
        for (id, ask) in asks {
            let session = &ask.session;
            match ask.kind {
                AskKind::Permission => client.post(
                    &format!("/api/session/{session}/permission/{id}/reply"),
                    &json!({"decision":"reject","message":"The user stopped this turn."}),
                )?,
                AskKind::Question(_) => client.request(
                    "DELETE",
                    &format!("/api/session/{session}/form/{id}"),
                    None,
                    kodem_core::providers::opencode::REQUEST_LIMIT,
                )?,
            };
        }
        {
            let mut h = self.host.lock().unwrap();
            h.requests.remove(self.task_id);
            let mut task = h.store.task(self.task_id)?;
            if task.state == RunState::Waiting {
                task.state = RunState::Running;
                h.save_task_with_notification(&task)?;
            }
        }
        self.interrupting = true;
        if let Some(session) = &self.session {
            // Steered prompts that OpenCode has not taken would start another run.
            for inbox in self.undelivered.drain() {
                let _ = client.request(
                    "DELETE",
                    &format!("/api/session/{session}/inbox/{inbox}"),
                    None,
                    kodem_core::providers::opencode::REQUEST_LIMIT,
                );
            }
            // Stopping the task's session stops the agents that work for it.
            client.post(&format!("/api/session/{session}/interrupt"), &json!({}))?;
        }
        if !self.running {
            self.transcript.stop_agents(self.host)?;
        }
        Ok(())
    }
}
