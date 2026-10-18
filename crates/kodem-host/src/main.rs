//! On-demand local host. Window closure never drops provider or PTY ownership.
mod accounts;
mod attachments;
mod awake;
mod delegation;
mod imports;
mod mcp;
mod notifications;
mod orchestrator;
mod project_icons;
mod provider;
mod settlement;
mod terminal;
mod titles;
mod tools;
use anyhow::{Context, Result, ensure};
use kodem_core::{git, ipc, platform, providers::codex, storage::Store, *};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

struct Host {
    store: Store,
    notifications: notifications::Notifications,
    runtimes: BTreeMap<Provider, RuntimeInfo>,
    /// Last observed sign-in and quota state, keyed by provider and account identity.
    accounts: BTreeMap<(Provider, String), AccountStatus>,
    sessions: HashMap<String, mpsc::Sender<provider::Control>>,
    requests: HashMap<String, Vec<InputRequest>>,
    activities: HashMap<String, Vec<Activity>>,
    /// Host-requested compactions in flight, by task, with the state each task returns to.
    compactions: HashMap<String, provider::Compaction>,
    /// The usage reading (`ContextUsage::reported`) each task's last compaction left behind.
    compacted: HashMap<String, u64>,
    terminals: HashMap<String, terminal::Terminal>,
    checks: std::collections::HashSet<String>,
    /// Number of task views produced so far, which puts them in order for a client.
    views: std::cell::Cell<u64>,
    shutdown: bool,
    usage_refresh: Option<Instant>,
    usage_refreshing: bool,
    settlement_refresh: Option<Instant>,
    settlement_refreshing: bool,
}
type Shared = Arc<Mutex<Host>>;
#[cfg(target_os = "macos")]
unsafe extern "C" {
    /// Parks the calling thread to serve the main dispatch queue. It never returns.
    fn dispatch_main() -> !;
}
fn main() {
    // A provider starts this executable as the server of Kodem's tools for one task.
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|argument| argument == "mcp") {
        let code = match mcp::serve(&arguments) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Kodem tools: {error:#}");
                1
            }
        };
        std::process::exit(code);
    }
    // Notification Center calls its delegate on the main dispatch queue. The main thread
    // serves that queue, so the host can answer how to present an alert while Kodem is active.
    #[cfg(target_os = "macos")]
    {
        // The host keeps the stack size that it had on the main thread.
        std::thread::Builder::new()
            .name("host".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(serve)
            .expect("Could not start the Kodem host thread");
        unsafe { dispatch_main() }
    }
    #[cfg(not(target_os = "macos"))]
    serve()
}
/// Runs the host to completion, then ends the process with the host's result.
fn serve() {
    // A panic must end the process: the main thread would otherwise keep a dead host alive.
    let code = match std::panic::catch_unwind(run) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            eprintln!("Kodem host: {error:#}");
            1
        }
        Err(_) => 101,
    };
    std::process::exit(code)
}
fn run() -> Result<()> {
    let root = ipc::data_dir()?;
    ipc::prepare(&root)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("host.lock"))?;
    // Another host holds the lock and serves this data directory.
    if lock.try_lock().is_err() {
        return Ok(());
    }
    let endpoint = ipc::socket(&root);
    // A Windows socket file reads as a reparse point, so remove it without asking first.
    match fs::remove_file(&endpoint) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let listener = ipc::UnixListener::bind(&endpoint)?;
    platform::restrict(&endpoint, 0o600)?;
    listener.set_nonblocking(true)?;
    let store = Store::open(&root)?;
    store.recover()?;
    accounts::ensure_defaults(&store)?;
    // Claude Code's `default` entry was a stand-in for its own choice and is no longer listed.
    store.retire_model(Provider::Claude, "default")?;
    let version = codex::version();
    let info = RuntimeInfo {
        version: version.as_ref().ok().cloned(),
        error: version.err().map(|e| e.to_string()),
        ..Default::default()
    };
    let host = Arc::new(Mutex::new(Host {
        store,
        notifications: notifications::Notifications::start(),
        runtimes: BTreeMap::from([
            (Provider::Codex, info),
            (
                Provider::Claude,
                provider::initial_runtime(Provider::Claude),
            ),
            (
                Provider::OpenCode,
                provider::initial_runtime(Provider::OpenCode),
            ),
        ]),
        accounts: BTreeMap::new(),
        sessions: HashMap::new(),
        requests: HashMap::new(),
        activities: HashMap::new(),
        compactions: HashMap::new(),
        compacted: HashMap::new(),
        terminals: HashMap::new(),
        checks: Default::default(),
        views: Default::default(),
        shutdown: false,
        usage_refresh: None,
        usage_refreshing: false,
        settlement_refresh: None,
        settlement_refreshing: false,
    }));
    let mut activity = Instant::now();
    // Taken and released on this thread, which Windows requires.
    let mut awake = awake::Awake::default();
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // macOS inherits the listener's nonblocking flag on accepted sockets.
                // Framed workers must wait for the complete request and response.
                stream.set_nonblocking(false)?;
                // Reject connections from any other local account even if filesystem permissions change.
                ensure!(ipc::same_user(&stream), "Unexpected IPC peer");
                activity = Instant::now();
                let host = host.clone();
                let root = root.clone();
                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(15)));
                    let response = ipc::read_frame::<Request>(&mut stream)
                        .and_then(|request| handle(&host, &root, request))
                        .unwrap_or_else(|e| Response::Error(format!("{e:#}")));
                    let _ = ipc::write_frame(&mut stream, &response);
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        {
            let h = host.lock().unwrap();
            if h.shutdown {
                break;
            }
            if awake.due() {
                // A failed read releases the lock rather than stopping the host.
                let wanted = h.store.settings().is_ok_and(|s| s.prevent_sleep)
                    && h.store.working().unwrap_or(false);
                awake.set(wanted);
            }
            if activity.elapsed() > Duration::from_secs(300) {
                let active = !h.checks.is_empty()
                    || h.terminals.values().any(|t| t.running())
                    || h.store.tasks()?.iter().any(|t| t.state.active());
                if !active {
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = fs::remove_file(&endpoint);
    Ok(())
}
/// Longest time a watch request waits for a change before it answers with none.
const WATCH_WAIT: Duration = Duration::from_secs(10);
/// How often a waiting watch request looks at its task again.
const WATCH_INTERVAL: Duration = Duration::from_millis(50);

// The state of a task without transcript rows. Each call counts as one produced view.
fn task_state(h: &Host, id: &str) -> Result<TaskView> {
    let task = h.store.task(id)?;
    let mut requests = h.requests.get(id).cloned().unwrap_or_default();
    requests.extend(task.questions.iter().cloned());
    h.views.set(h.views.get() + 1);
    Ok(TaskView {
        activities: h.activities.get(id).cloned().unwrap_or_default(),
        task,
        outgoing: h.store.outgoing(id)?,
        messages: Vec::new(),
        before: None,
        requests,
        runs: h.store.runs(id)?,
        epoch: h.store.revision().0.to_owned(),
        serial: h.views.get(),
    })
}
fn task_view(h: &Host, id: &str, before: Option<u64>) -> Result<TaskView> {
    let mut view = task_state(h, id)?;
    (view.messages, view.before) = h.store.messages(id, before)?;
    Ok(view)
}
// Digest of everything in a view that is not a transcript row or the order of the view.
fn state_digest(view: &TaskView) -> Result<u64> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&(
        &view.task,
        &view.activities,
        &view.outgoing,
        &view.requests,
        &view.runs,
    ))?
    .hash(&mut hasher);
    Ok(hasher.finish())
}
/// Answers a watcher once the task differs from what its cursor describes.
///
/// Transcript rows are compared by the revisions of the store. The rest of the view (the task,
/// the outbox, activities, requests and runs) is compared by a digest, so a change that only lives in
/// host memory is seen without a notification from the code that made it. The host lock is
/// held only while the task is looked at.
fn watch(host: &Shared, task_id: &str, after: Option<Cursor>) -> Result<Response> {
    let until = Instant::now() + WATCH_WAIT;
    loop {
        {
            let h = host.lock().unwrap();
            ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
            let mut view = task_state(&h, task_id)?;
            let (epoch, rev) = h.store.revision();
            let through = Cursor {
                epoch: epoch.to_owned(),
                rev,
                state: state_digest(&view)?,
            };
            let Some(after) = after.as_ref().filter(|after| after.epoch == through.epoch) else {
                (view.messages, view.before) = h.store.messages(task_id, None)?;
                return Ok(Response::Watched(SubscriptionStart::Snapshot {
                    state: view,
                    through,
                }));
            };
            let Some(rows) = h.store.changed_messages(task_id, after.rev)? else {
                (view.messages, view.before) = h.store.messages(task_id, None)?;
                return Ok(Response::Watched(SubscriptionStart::ReplayGap {
                    current: view,
                    through,
                }));
            };
            if !rows.is_empty() || after.state != through.state || Instant::now() >= until {
                view.messages = rows;
                return Ok(Response::Watched(SubscriptionStart::Replay {
                    state: view,
                    through,
                }));
            }
        }
        std::thread::sleep(WATCH_INTERVAL);
    }
}
fn handle(host: &Shared, root: &std::path::Path, request: Request) -> Result<Response> {
    ensure!(
        !host.lock().unwrap().shutdown,
        "Host is restarting. Retry after reconnecting."
    );
    match request {
        Request::SubagentThread {
            task_id,
            thread_id,
            before,
        } => {
            let h = host.lock().unwrap();
            let task = h.store.task(&task_id)?;
            let agent = task
                .subagents
                .into_iter()
                .find(|a| a.thread_id == thread_id)
                .context("Subagent does not belong to this task")?;
            let (messages, before) = h
                .store
                .agent_messages(&agent.transcript_id(&task_id), before)?;
            Ok(Response::SubagentThread(SubagentView {
                task_id,
                agent,
                messages,
                before,
            }))
        }
        Request::RefreshUsage {
            provider,
            ref account,
        }
        | Request::ConsumeUsageReset {
            provider,
            ref account,
            ..
        } => {
            let account = account.clone();
            {
                let mut h = host.lock().unwrap();
                ensure!(
                    !h.usage_refreshing,
                    "Account usage is refreshing. Try again shortly."
                );
                h.usage_refreshing = true;
            }
            let key = match &request {
                Request::ConsumeUsageReset {
                    idempotency_key, ..
                } => Some(idempotency_key.as_str()),
                _ => None,
            };
            let result = provider::account_usage(host, provider, account, key);
            let mut h = host.lock().unwrap();
            h.usage_refreshing = false;
            h.usage_refresh = Some(Instant::now());
            result
        }
        Request::Hello => {
            let mut h = host.lock().unwrap();
            if !h.settlement_refreshing
                && h.settlement_refresh
                    .is_none_or(|last| last.elapsed() >= Duration::from_secs(30))
            {
                h.settlement_refresh = Some(Instant::now());
                h.settlement_refreshing = true;
                let settlement_host = host.clone();
                std::thread::spawn(move || {
                    let _ = settlement::refresh(&settlement_host);
                    settlement_host.lock().unwrap().settlement_refreshing = false;
                });
            }
            if !h.usage_refreshing
                && h.usage_refresh
                    .is_none_or(|last| last.elapsed() >= Duration::from_secs(120))
            {
                h.usage_refresh = Some(Instant::now());
                h.usage_refreshing = true;
                let usage_host = host.clone();
                // Sign-in state and quotas of every account refresh together, off the request path.
                std::thread::spawn(move || {
                    let _ = accounts::refresh_all(&usage_host);
                    usage_host.lock().unwrap().usage_refreshing = false;
                });
            }
            let projects = h.store.projects()?;
            let tasks = h
                .store
                .tasks()?
                .into_iter()
                .filter(|task| {
                    !task.kind.has_project() || projects.iter().any(|p| p.id == task.project_id)
                })
                .collect();
            let without_git = projects
                .iter()
                .filter(|p| !git::is_repository(&p.path))
                .map(|p| p.id.clone())
                .collect();
            Ok(Response::Workspace(Workspace {
                notification_delivery: h.notifications.status(),
                protocol: ipc::PROTOCOL,
                projects,
                without_git,
                tasks,
                settings: h.store.settings()?,
                runtimes: h.runtimes.clone(),
                accounts: accounts::states(&h)?,
            }))
        }
        Request::TestNotification { sound } => {
            host.lock().unwrap().notifications.test(sound);
            Ok(Response::Ok)
        }
        Request::Reconcile { task_id } => {
            provider::reconcile(host, &task_id)?;
            Ok(Response::Ok)
        }
        Request::ConnectRuntime { provider, account } => {
            provider::discover(host.clone(), provider, account, None)?;
            Ok(Response::Ok)
        }
        Request::AddAccount { provider, name } => Ok(Response::Account(accounts::add(
            host, root, provider, &name,
        )?)),
        Request::EditAccount {
            provider,
            id,
            name,
            color,
        } => {
            accounts::edit(host, provider, &id, &name, color)?;
            Ok(Response::Ok)
        }
        Request::RemoveAccount { provider, id } => {
            accounts::remove(host, root, provider, &id)?;
            Ok(Response::Ok)
        }
        Request::InspectTools => Ok(Response::Tools(tools::inspect())),
        Request::InstallTool { tool } => {
            tools::install(root, tool)?;
            Ok(Response::Ok)
        }
        Request::SignInAccount { provider, id } => {
            accounts::sign_in(host, root, provider, &id)?;
            Ok(Response::Ok)
        }
        Request::SignOutAccount { provider, id } => {
            accounts::sign_out(host, provider, &id)?;
            Ok(Response::Ok)
        }
        Request::RefreshAccount { provider, id } => {
            // Reading status starts the provider CLI, which can take seconds. The result arrives
            // with the next workspace reply instead of holding this request open.
            accounts::resolve(&host.lock().unwrap(), provider, Some(&id))?;
            let host = host.clone();
            std::thread::spawn(move || {
                let _ = accounts::refresh(&host, provider, &id);
            });
            Ok(Response::Ok)
        }
        Request::SetProjectDefaults {
            project_id,
            defaults,
        } => {
            let h = host.lock().unwrap();
            if let Some(defaults) = &defaults {
                provider::validate_saved_selection(&h, defaults)?;
            }
            h.store.set_project_defaults(&project_id, defaults)?;
            Ok(Response::Ok)
        }
        Request::SetAgentPreferences(preferences) => {
            let h = host.lock().unwrap();
            provider::validate_saved_selection(&h, &preferences)?;
            h.store.save_agent_preferences(preferences)?;
            Ok(Response::Ok)
        }
        Request::SetAccessChoice(preferences) => {
            let h = host.lock().unwrap();
            provider::validate_saved_selection(&h, &preferences)?;
            h.store.save_access_choice(preferences)?;
            Ok(Response::Ok)
        }
        Request::OpenProject {
            path,
            missing_repository,
        } => {
            let mut path = path.canonicalize().map(kodem_core::platform::plain_path)?;
            if !git::is_repository(&path) {
                match missing_repository {
                    MissingRepository::Ask => return Ok(Response::NoRepository(path)),
                    MissingRepository::Initialize => git::initialize(&path)?,
                    MissingRepository::OpenAnyway => {}
                }
            }
            if git::is_repository(&path) {
                path = git::repository(&path)?;
            }
            Ok(Response::Project(
                host.lock().unwrap().store.add_project(path)?,
            ))
        }
        Request::CustomizeProject {
            project_id,
            name,
            color,
            icon,
            commands,
        } => {
            let previous = host
                .lock()
                .unwrap()
                .store
                .projects()?
                .into_iter()
                .find(|p| p.id == project_id)
                .context("Project not found")?
                .icon;
            let imported = if let ProjectIcon::Image(source) = &icon {
                if icon != previous {
                    Some(project_icons::import(root, source)?)
                } else {
                    None
                }
            } else {
                None
            };
            let saved_icon = imported
                .as_ref()
                .map_or(icon, |path| ProjectIcon::Image(path.clone()));
            let result = host.lock().unwrap().store.customize_project(
                &project_id,
                &name,
                color,
                saved_icon,
                commands,
            );
            if result.is_err()
                && let Some(path) = imported
            {
                let _ = std::fs::remove_file(path);
            }
            result?;
            Ok(Response::Ok)
        }
        Request::RemoveProject { project_id } => {
            let h = host.lock().unwrap();
            for task in h
                .store
                .tasks()?
                .iter()
                .filter(|t| t.project_id == project_id)
            {
                ensure!(
                    !h.checks.contains(&task.id)
                        && !h.terminals.get(&task.id).is_some_and(|t| t.running()),
                    "Stop this project's checks and terminals before removing it"
                );
            }
            h.store.remove_project(&project_id)?;
            Ok(Response::Ok)
        }
        Request::CreateTask {
            project_id,
            worktree,
            preferences: defaults,
            base,
        } => {
            provider::validate_preferences(&host.lock().unwrap(), &defaults)?;
            ensure!(
                base.is_none() || worktree,
                "A task that starts from another commit needs its own worktree"
            );
            let p = project(host, &project_id)?;
            let id = new_id();
            let cwd = if worktree {
                git::worktree(&p.path, &root.join("worktrees"), &id, base.as_deref())?
            } else {
                p.path
            };
            let mut task = new_task(id, TaskKind::Coding, project_id, cwd, &defaults);
            task.owns_worktree = worktree;
            task.baseline = git::head(&task.cwd);
            task.repository_baselines = git::repository_baselines(&task.cwd)?;
            task.branch = git::branch(&task.cwd);
            host.lock()
                .unwrap()
                .store
                .save_task_preferences(&task, defaults)?;
            Ok(Response::Created(task))
        }
        Request::CreateChat {
            preferences: defaults,
        } => {
            provider::validate_preferences(&host.lock().unwrap(), &defaults)?;
            let id = new_id();
            // Each chat has a scratch directory of its own, so chats never share a checkout.
            let cwd = chat_directory(root, &id);
            fs::create_dir_all(&cwd)?;
            let mut task = new_task(id, TaskKind::Chat, String::new(), cwd, &defaults);
            task.title = "New chat".into();
            host.lock().unwrap().store.save_task(&task)?;
            Ok(Response::Created(task))
        }
        Request::ReviewTargets { project_id } => {
            let p = project(host, &project_id)?;
            ensure!(
                git::is_repository(&p.path),
                "This project has no Git repository, so it has no branches or pull requests to review"
            );
            Ok(Response::ReviewTargets(git::review_targets(
                &project_id,
                &p.path,
            )?))
        }
        Request::CreateReview {
            project_id,
            target,
            preferences: defaults,
        } => {
            provider::validate_preferences(&host.lock().unwrap(), &defaults)?;
            let p = project(host, &project_id)?;
            let id = new_id();
            let prepared = git::prepare_review(&p.path, &review_directory(root), &id, &target)?;
            let mut task = new_task(id, TaskKind::Review, project_id, prepared.cwd, &defaults);
            task.title = match &target {
                ReviewTarget::PullRequest { number, title, .. } => {
                    format!("Review PR #{number}: {title}")
                }
                ReviewTarget::Branch { name, .. } => format!("Review {name}"),
            };
            task.title_pending = false;
            task.owns_worktree = true;
            // The merge base is the task's baseline, so "Since task" shows the reviewed changes.
            task.baseline = Some(prepared.merge_base);
            task.branch = target.head().to_owned();
            task.review_scope = Some(ReviewScope {
                target,
                head: prepared.head,
                base: prepared.base,
            });
            // Only a coding task's options become the options of new tasks.
            let saved = host.lock().unwrap().store.save_task(&task);
            if let Err(error) = saved {
                let _ = git::discard_review(Some(&p.path), &task.cwd);
                return Err(error);
            }
            Ok(Response::Created(task))
        }
        Request::Watch { task_id, after } => watch(host, &task_id, after),
        Request::Agents { parent } => Ok(Response::Agents(delegation::agents(host, &parent)?)),
        Request::Delegate {
            parent,
            prompt,
            provider,
            model,
            write,
            wait_ms,
        } => Ok(Response::Delegated(delegation::delegate(
            host,
            root,
            &parent,
            &prompt,
            (provider, model.as_deref()),
            write,
            wait_ms,
        )?)),
        Request::DelegateStatus {
            parent,
            task_id,
            wait_ms,
        } => Ok(Response::Delegated(delegation::status(
            host, &parent, &task_id, wait_ms,
        )?)),
        Request::DelegateCancel { parent, task_id } => Ok(Response::Delegated(delegation::cancel(
            host, &parent, &task_id,
        )?)),
        Request::ForkTask { task_id } => {
            let (source, run, from) = {
                let h = host.lock().unwrap();
                let source = h.store.task(&task_id)?;
                ensure!(
                    !source.state.active() && !h.compactions.contains_key(&task_id),
                    "Fork after this turn finishes"
                );
                let run = provider::forkable_run(&h, &task_id)?.context(
                    "This task has no saved turn to fork from. Only a coding task saves its files after each turn.",
                )?;
                let mut from = provider::agent_of(&h, &source);
                from.task_id = Some(source.id.clone());
                (source, run, from)
            };
            let project = project(host, &source.project_id)?;
            let checkpoint = run.checkpoint.clone().unwrap_or_default();
            let id = new_id();
            // The worktree starts at the commit that the checkpoint was taken on and then
            // receives the files of the checkpoint, so changes that were not committed stay
            // uncommitted in the fork.
            let start = format!(
                "{}^",
                git::checkpoint::reference_of(&source.id, &checkpoint)?
            );
            let cwd = git::worktree(&project.path, &root.join("worktrees"), &id, Some(&start))?;
            git::checkpoint::apply(&cwd, &source.id, &checkpoint)?;
            let mut task = new_task(
                id,
                TaskKind::Coding,
                source.project_id.clone(),
                cwd,
                &provider::preferences(&source),
            );
            task.owns_worktree = true;
            task.baseline = git::head(&task.cwd);
            task.repository_baselines = git::repository_baselines(&task.cwd)?;
            task.branch = git::branch(&task.cwd);
            task.title = format!("Fork of {}", source.title);
            task.title_pending = false;
            task.handoff = Some(Handoff::Forked {
                from,
                turn: run.ordinal,
            });
            task.lineage = Some(Lineage {
                parent: source.id.clone(),
                turn: run.ordinal,
                delegated: false,
            });
            host.lock().unwrap().store.save_task(&task)?;
            Ok(Response::Created(task))
        }
        Request::ExternalThreads => Ok(Response::ExternalThreads(imports::discover(host)?)),
        Request::ImportThread {
            provider,
            account,
            thread,
        } => {
            let task = imports::import(host, provider, &account, &thread)?;
            Ok(Response::Imported { thread, task })
        }
        Request::UndoTurn { task_id } => {
            provider::undo_turn(host, &task_id)?;
            Ok(Response::Ok)
        }
        Request::Task { task_id, before } => Ok(Response::Task(task_view(
            &host.lock().unwrap(),
            &task_id,
            before,
        )?)),
        Request::EditTask { task_id, edit } => {
            // Another provider cannot continue the provider conversation of the task, so the
            // session that holds it ends first. A task that works refuses the edit below.
            if matches!(edit, TaskEdit::Provider(_)) {
                let _ = provider::close_session(host, &task_id);
            }
            let h = host.lock().unwrap();
            let mut t = h.store.task(&task_id)?;
            let preferences_changed = matches!(
                &edit,
                TaskEdit::Model { .. }
                    | TaskEdit::Access(_)
                    | TaskEdit::Provider(_)
                    | TaskEdit::Account(_)
                    | TaskEdit::ProviderAccess(_)
            );
            ensure!(
                !preferences_changed
                    || (t.state != RunState::DeliveryUnknown
                        && !h.store.has_uncertain_submission(&task_id)?),
                "Reconcile uncertain delivery before changing provider options"
            );
            let access_changed = matches!(&edit, TaskEdit::Access(_) | TaskEdit::ProviderAccess(_));
            let mut defaults = provider::preferences(&t);
            match edit {
                TaskEdit::Provider(preferences) => {
                    ensure!(
                        !h.sessions.contains_key(&task_id)
                            && !t.state.active()
                            && !h.compactions.contains_key(&task_id)
                            && t.state != RunState::DeliveryUnknown
                            && !h.store.has_uncertain_submission(&task_id)?
                            && h.store.outgoing(&task_id)?.is_empty(),
                        "Change the provider after this turn finishes and with no message queued"
                    );
                    provider::validate_saved_selection(&h, &preferences)?;
                    if t.provider_thread.is_some() {
                        // The account of a started conversation has rules of its own.
                        ensure!(
                            preferences.provider != t.provider
                                || preferences.account.as_deref().unwrap_or(DEFAULT_ACCOUNT)
                                    == t.account_id(),
                            "Use the account choice to change the account of a started task"
                        );
                        // The task leaves the conversation of its provider. The next message
                        // starts one with the new provider and carries the earlier turns.
                        if preferences.provider != t.provider {
                            let mut from = provider::agent_of(&h, &t);
                            // Two changes without a message between them hand over once, from
                            // the agent that ran the turns.
                            if let Some(Handoff::Switched { from: earlier }) = &t.handoff {
                                from = earlier.clone();
                            }
                            t.handoff = Some(Handoff::Switched { from });
                            t.provider_thread = None;
                        }
                    }
                    t.provider = preferences.provider;
                    t.account = preferences.account.clone();
                    t.provider_access = preferences.provider_access.clone();
                    t.model = preferences.model.clone();
                    t.effort = preferences.effort.clone();
                    t.access = preferences.access;
                    t.context_usage = None;
                    defaults = preferences;
                }
                TaskEdit::Account(account) => {
                    ensure!(
                        !t.state.active() && t.active_turn.is_none(),
                        "Change the account after this turn finishes"
                    );
                    // Codex and OpenCode keep thread history in per-account state, so a started
                    // thread cannot continue under another identity.
                    ensure!(
                        !t.provider.account_bound() || t.provider_thread.is_none(),
                        "{} cannot continue a started conversation under another account. Start a new task with that account.",
                        t.provider.label()
                    );
                    accounts::resolve(&h, t.provider, account.as_deref())?;
                    // Both providers' terms forbid getting around usage limits. Carrying one
                    // conversation to another account when its account is at a limit does that.
                    ensure!(
                        t.provider_thread.is_none()
                            || account.as_deref().unwrap_or(DEFAULT_ACCOUNT) == t.account_id()
                            || !accounts::limit_reached(&h, t.provider, t.account_id()),
                        "This task's account reached its usage limit. Moving the same conversation to another account would get around that limit, which the provider's terms do not allow. Wait for the reset, or use the provider's own options for more usage."
                    );
                    defaults.account = account.clone();
                    // The session owner reopens its provider process under the new account
                    // before the next turn; the environment of a running process is fixed.
                    t.account = account;
                    t.context_usage = None;
                }
                TaskEdit::ProviderAccess(access) => {
                    ensure!(!t.state.active(), "Change access after this turn finishes");
                    defaults.provider_access = Some(access.clone());
                    provider::validate_preferences(&h, &defaults)?;
                    t.provider_access = Some(access);
                }
                TaskEdit::Access(access) => {
                    ensure!(
                        t.provider == Provider::Codex,
                        "Select one of this provider's advertised access modes"
                    );
                    ensure!(!t.state.active(), "Change access after this turn finishes");
                    t.access = access;
                    defaults.access = access;
                }
                TaskEdit::Rename(title) => {
                    ensure!(
                        !title.trim().is_empty() && title.len() < 512,
                        "Use a title between 1 and 511 bytes"
                    );
                    t.title = title;
                    t.title_pending = false;
                }
                TaskEdit::Pin(pin) => t.pinned = pin,
                TaskEdit::Settle(settled) => {
                    ensure!(
                        !t.state.active() && t.state != RunState::DeliveryUnknown,
                        "Finish or reconcile this task before settling it"
                    );
                    t.settled = Some(settled);
                }
                TaskEdit::Archive(archive) => t.archived = archive,
                TaskEdit::Draft(draft) => {
                    ensure!(draft.len() <= 1024 * 1024, "Draft exceeds 1 MiB");
                    t.draft = draft
                }
                TaskEdit::Model { model, effort } => {
                    ensure!(!t.state.active(), "Change model after this turn finishes");
                    t.model = model;
                    t.effort = effort;
                    defaults.model = t.model.clone();
                    defaults.effort = t.effort.clone();
                    provider::validate_preferences(&h, &defaults)?;
                    t.context_usage = None;
                }
            }
            // An approval rule chosen in any workspace is the rule of that model everywhere.
            let rule = access_changed.then(|| AgentPreferences {
                model: provider::effective_model(&h, &t),
                ..provider::preferences(&t)
            });
            // Only a coding task's options become the options of the next new task. Of a chat
            // or a review, only the approval rule is remembered, for its model.
            if preferences_changed && t.kind == TaskKind::Coding {
                h.store.save_task_preferences(&t, defaults)?;
            } else {
                h.store.save_task(&t)?;
            }
            if let Some(rule) = rule {
                h.store.remember_access(&rule)?;
            }
            Ok(Response::Ok)
        }
        Request::DeleteTask { task_id } => {
            let (task, project) = {
                let mut h = host.lock().unwrap();
                ensure!(
                    !h.checks.contains(&task_id)
                        && !h
                            .terminals
                            .get(&task_id)
                            .is_some_and(|terminal| terminal.running()),
                    "Stop this task's check or terminal before deleting it"
                );
                let task = h.store.task(&task_id)?;
                let project = h
                    .store
                    .projects()?
                    .into_iter()
                    .find(|project| project.id == task.project_id);
                h.store.delete_task(&task_id)?;
                if let Some(session) = h.sessions.get(&task_id) {
                    let _ = session.send(provider::Control::Close);
                }
                h.requests.remove(&task_id);
                h.activities.remove(&task_id);
                h.compacted.remove(&task_id);
                h.terminals.remove(&task_id);
                (task, project)
            };
            discard_scratch(root, &task, project.as_ref());
            Ok(Response::Ok)
        }
        Request::TaskChanges { task_id } => {
            let task = host.lock().unwrap().store.task(&task_id)?;
            let comparison = if task.baseline.is_some() {
                Comparison::Baseline
            } else {
                Comparison::WorkingTree
            };
            let summary = git::capture(&task, comparison)?.summary();
            Ok(Response::TaskChanges { task_id, summary })
        }
        Request::SaveSettings(settings) => {
            host.lock().unwrap().store.save_settings(&settings)?;
            Ok(Response::Ok)
        }
        Request::Submit {
            task_id,
            submission_id,
            text,
            feedback_ids,
            attachment_ids,
            mode,
        } => {
            provider::submit(
                host.clone(),
                task_id.clone(),
                submission_id.clone(),
                text,
                feedback_ids,
                attachment_ids,
                mode,
            )?;
            Ok(Response::Accepted {
                task_id,
                submission_id,
            })
        }
        Request::ImportAttachments { task_id, sources } => {
            host.lock().unwrap().store.task(&task_id)?;
            let imported = attachments::import(root, sources)?;
            let registration = (|| {
                let h = host.lock().unwrap();
                let mut task = h.store.task(&task_id)?;
                provider::validate_attachments(&h, &task, &imported)?;
                ensure!(
                    task.attachments.len() + imported.len() <= 32,
                    "A prompt can contain up to 32 attachments"
                );
                ensure!(
                    task.attachments
                        .iter()
                        .chain(&imported)
                        .map(|a| a.bytes)
                        .sum::<u64>()
                        <= 128 * 1024 * 1024,
                    "Prompt attachments exceed 128 MiB"
                );
                task.attachments.extend(imported.clone());
                h.store.save_task(&task)
            })();
            if let Err(error) = registration {
                attachments::discard(root, &imported);
                return Err(error);
            }
            Ok(Response::Attachments {
                task_id,
                attachments: imported,
            })
        }
        Request::RemoveAttachment { task_id, id } => {
            let h = host.lock().unwrap();
            let mut task = h.store.task(&task_id)?;
            task.attachments.retain(|a| a.id != id);
            h.store.save_task(&task)?;
            Ok(Response::Ok)
        }
        Request::RestoreOutgoing { task_id, id } => {
            let h = host.lock().unwrap();
            h.store.restore_outgoing(&task_id, &id)?;
            Ok(Response::Task(task_view(&h, &task_id, None)?))
        }
        Request::CancelQueued { task_id, id } => {
            host.lock().unwrap().store.cancel_outgoing(&task_id, &id)?;
            Ok(Response::Ok)
        }
        Request::ResumeQueue { task_id } => {
            provider::resume_queue(host.clone(), &task_id)?;
            Ok(Response::Ok)
        }
        Request::SteerQueued { task_id, id } => {
            provider::steer_queued(host.clone(), &task_id, &id)?;
            let h = host.lock().unwrap();
            Ok(Response::Task(task_view(&h, &task_id, None)?))
        }
        Request::Interrupt { task_id } => {
            let h = host.lock().unwrap();
            let tx = h.sessions.get(&task_id).context(
                "No live provider session. Recovered tasks are not automatically replayed.",
            )?;
            tx.send(provider::Control::Interrupt)?;
            delegation::stop_children(&h, &task_id)?;
            Ok(Response::Ok)
        }
        Request::CompactTask { task_id } => {
            provider::compact(host.clone(), &task_id)?;
            Ok(Response::Ok)
        }
        Request::Answer {
            task_id,
            id,
            response,
        } => {
            let h = host.lock().unwrap();
            let task = h.store.task(&task_id)?;
            let request = h
                .requests
                .get(&task_id)
                .and_then(|r| r.iter().find(|r| r.id == id))
                .or_else(|| task.questions.iter().find(|r| r.id == id))
                .context("This request is no longer pending")?;
            validate_answer(request, &response)?;
            if request.is_async() {
                let submission_id = request.answer_submission_id();
                let text = answer_text(request, &response);
                drop(h);
                provider::submit(
                    host.clone(),
                    task_id.clone(),
                    submission_id,
                    text,
                    Vec::new(),
                    Vec::new(),
                    SendMode::Steer,
                )?;
                let h = host.lock().unwrap();
                let mut task = h.store.task(&task_id)?;
                task.questions.retain(|r| r.id != id);
                h.store.save_task(&task)?;
                return Ok(Response::Ok);
            }
            h.sessions
                .get(&task_id)
                .context("Provider disconnected")?
                .send(provider::Control::Answer { id, response })?;
            Ok(Response::Ok)
        }
        Request::CaptureDiff {
            task_id,
            comparison,
            previous,
        } => {
            let task = host.lock().unwrap().store.task(&task_id)?;
            let mut snapshot = git::capture(&task, comparison)?;
            host.lock()
                .unwrap()
                .store
                .save_refreshed_snapshot(&mut snapshot, previous.as_deref())?;
            Ok(Response::Diff(snapshot))
        }
        Request::LoadSnapshot { id } => {
            Ok(Response::Diff(host.lock().unwrap().store.snapshot(&id)?))
        }
        Request::AddFeedback {
            task_id,
            snapshot_id,
            file,
            side,
            start,
            end,
            comment,
        } => {
            let h = host.lock().unwrap();
            let snapshot = h.store.snapshot(&snapshot_id)?;
            ensure!(
                snapshot.task_id == task_id,
                "Snapshot belongs to another task"
            );
            let feedback = git::feedback(&snapshot, file, side, start, end, comment)?;
            let mut task = h.store.task(&task_id)?;
            task.feedback.push(feedback);
            ensure!(
                serde_json::to_vec(&task.feedback)?.len() <= 2 * 1024 * 1024,
                "Draft context exceeds 2 MiB; send or remove existing context first"
            );
            h.store.save_task(&task)?;
            Ok(Response::Ok)
        }
        Request::AddFeedbackRows {
            task_id,
            snapshot_id,
            file,
            first_row,
            last_row,
            comment,
        } => {
            let h = host.lock().unwrap();
            let snapshot = h.store.snapshot(&snapshot_id)?;
            ensure!(
                snapshot.task_id == task_id,
                "Snapshot belongs to another task"
            );
            let feedback = git::feedback_rows(&snapshot, file, first_row, last_row, comment)?;
            let mut task = h.store.task(&task_id)?;
            ensure!(
                task.feedback.len() < 64,
                "Send or remove existing context before adding more"
            );
            task.feedback.push(feedback);
            ensure!(
                serde_json::to_vec(&task.feedback)?.len() <= 2 * 1024 * 1024,
                "Draft context exceeds 2 MiB; send or remove existing context first"
            );
            h.store.save_task(&task)?;
            Ok(Response::Ok)
        }
        Request::RemoveFeedback { task_id, id } => {
            let h = host.lock().unwrap();
            let mut t = h.store.task(&task_id)?;
            t.feedback.retain(|f| f.id != id);
            h.store.save_task(&t)?;
            Ok(Response::Ok)
        }
        Request::StageFile {
            task_id,
            snapshot_id,
            file,
        } => {
            let (task, snapshot) = {
                let h = host.lock().unwrap();
                (h.store.task(&task_id)?, h.store.snapshot(&snapshot_id)?)
            };
            ensure!(
                task.kind != TaskKind::Review,
                "A review does not change the repository"
            );
            git::stage(&task, &snapshot, file)?;
            Ok(Response::Ok)
        }
        Request::ReadEditableFile {
            task_id,
            snapshot_id,
            file,
        } => {
            let (task, snapshot) = {
                let h = host.lock().unwrap();
                (h.store.task(&task_id)?, h.store.snapshot(&snapshot_id)?)
            };
            ensure!(
                task.kind != TaskKind::Review,
                "A review does not change the repository"
            );
            Ok(Response::EditableFile(git::editable_file(
                &task, &snapshot, file,
            )?))
        }
        Request::SaveFile {
            task_id,
            snapshot_id,
            repository,
            path,
            expected,
            text,
        } => {
            let (task, snapshot) = {
                let h = host.lock().unwrap();
                (h.store.task(&task_id)?, h.store.snapshot(&snapshot_id)?)
            };
            ensure!(
                task.kind != TaskKind::Review,
                "A review does not change the repository"
            );
            git::save_file(
                &task,
                &snapshot,
                repository.as_deref(),
                &path,
                &expected,
                &text,
            )?;
            Ok(Response::Ok)
        }
        Request::ComposerCatalog {
            provider,
            account,
            model: _,
            request_id: _,
            task_id,
            project_id,
        } => Ok(Response::ComposerCatalog(provider::composer_catalog(
            host, provider, account, task_id, project_id,
        )?)),
        Request::Resources {
            provider,
            account,
            task_id,
            project_id,
        } => Ok(Response::Resources(provider::resources(
            host, provider, account, task_id, project_id,
        )?)),
        Request::Files { task_id } => {
            let cwd = host.lock().unwrap().store.task(&task_id)?.cwd;
            Ok(Response::Files(git::files(&cwd)?))
        }
        Request::ReadFile { task_id, path } => {
            let cwd = host.lock().unwrap().store.task(&task_id)?.cwd;
            Ok(Response::Text(git::read_file(&cwd, &path)?))
        }
        Request::Terminal { task_id } => {
            let mut h = host.lock().unwrap();
            ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
            if !h.terminals.contains_key(&task_id) {
                let cwd = h.store.task(&task_id)?.cwd;
                h.terminals
                    .insert(task_id.clone(), terminal::Terminal::start(&cwd)?);
            }
            Ok(Response::Terminal(
                h.terminals.get(&task_id).unwrap().view(),
            ))
        }
        Request::TerminalInput { task_id, data } => {
            let mut h = host.lock().unwrap();
            ensure!(data.len() < 65536, "Terminal input too large");
            h.terminals
                .get_mut(&task_id)
                .context("Open the terminal first")?
                .write(&data)?;
            Ok(Response::Ok)
        }
        Request::TerminalResize {
            task_id,
            rows,
            cols,
        } => {
            host.lock()
                .unwrap()
                .terminals
                .get_mut(&task_id)
                .context("Open the terminal first")?
                .resize(rows, cols)?;
            Ok(Response::Ok)
        }
        Request::RunProjectCommand {
            task_id,
            command_id,
            rows,
            cols,
        } => {
            let mut h = host.lock().unwrap();
            ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
            let task = h.store.task(&task_id)?;
            let project = h
                .store
                .projects()?
                .into_iter()
                .find(|project| project.id == task.project_id)
                .context("Project not found")?;
            let command = project
                .commands
                .iter()
                .find(|command| command.id == command_id)
                .context("This command was removed from the project")?;
            let programs = command.programs(&task.cwd, &project.path);
            if let Some(missing) = programs.iter().find(|program| !program.exists()) {
                anyhow::bail!("{} was not found", missing.display());
            }
            let line = platform::program_sequence(&task.cwd, &programs)?;
            // A shell that exited cannot take the line, so a new one starts in its place.
            if !h.terminals.get(&task_id).is_some_and(|t| t.running()) {
                h.terminals
                    .insert(task_id.clone(), terminal::Terminal::start(&task.cwd)?);
            }
            let terminal = h.terminals.get_mut(&task_id).unwrap();
            terminal.resize(rows, cols)?;
            terminal.write(format!("{line}\r").as_bytes())?;
            Ok(Response::Ok)
        }
        Request::RunCheck { task_id, command } => {
            terminal::run_check(host.clone(), task_id, command)?;
            Ok(Response::Ok)
        }
        Request::Shutdown => {
            let mut h = host.lock().unwrap();
            ensure!(
                !h.store.tasks()?.iter().any(|t| t.state.active()) && h.checks.is_empty(),
                "Stop active tasks and checks before shutting down the host"
            );
            h.terminals.clear();
            h.shutdown = true;
            Ok(Response::Ok)
        }
    }
}
// Finds a registered project or reports that it was removed.
fn project(host: &Shared, id: &str) -> Result<Project> {
    host.lock()
        .unwrap()
        .store
        .projects()?
        .into_iter()
        .find(|project| project.id == id)
        .context("Project not found")
}

// Returns the scratch directory of one chat.
fn chat_directory(root: &std::path::Path, id: &str) -> std::path::PathBuf {
    root.join("chats").join(id)
}

// Returns the directory that holds the detached worktrees of reviews.
fn review_directory(root: &std::path::Path) -> std::path::PathBuf {
    root.join("reviews")
}

// Removes the directory that the host made for a deleted chat or review. Only a path the host
// derives from the task's identity is removed; a failure leaves the directory behind.
fn discard_scratch(root: &std::path::Path, task: &Task, project: Option<&Project>) {
    match task.kind {
        TaskKind::Coding => {
            let _ = git::checkpoint::discard(&task.cwd, &task.id);
        }
        TaskKind::Chat => {
            if task.cwd == chat_directory(root, &task.id) {
                let _ = fs::remove_dir_all(&task.cwd);
            }
        }
        TaskKind::Review => {
            if task.cwd == review_directory(root).join(&task.id) {
                let _ =
                    git::discard_review(project.map(|project| project.path.as_path()), &task.cwd);
            }
        }
    }
}

// Returns an idle task with the chosen agent options and nothing else decided.
fn new_task(
    id: String,
    kind: TaskKind,
    project_id: String,
    cwd: std::path::PathBuf,
    defaults: &AgentPreferences,
) -> Task {
    Task {
        subagents: Vec::new(),
        pending_completion: None,
        provider: defaults.provider,
        account: defaults.account.clone(),
        plan: Vec::new(),
        provider_access: defaults.provider_access.clone(),
        kind,
        review_scope: None,
        id,
        project_id,
        title: "New task".into(),
        title_pending: true,
        cwd,
        owns_worktree: false,
        baseline: None,
        repository_baselines: Vec::new(),
        branch: String::new(),
        provider_thread: None,
        active_turn: None,
        state: RunState::Idle,
        draft: String::new(),
        model: defaults.model.clone(),
        effort: defaults.effort.clone(),
        access: defaults.access,
        review: None,
        context_usage: None,
        pinned: false,
        settled: None,
        branch_merged: false,
        archived: false,
        created: now(),
        updated: now(),
        last_started: None,
        feedback: Vec::new(),
        attachments: Vec::new(),
        error: None,
        questions: Vec::new(),
        handoff: None,
        lineage: None,
    }
}

// Called only after validate_answer has checked all question IDs and answer strings.
fn answer_text(request: &InputRequest, response: &Value) -> String {
    fn literal(text: &str) -> String {
        let mut escaped = String::with_capacity(text.len());
        for character in text.chars() {
            if character.is_ascii_punctuation() {
                escaped.push('\\');
            }
            escaped.push(character);
        }
        escaped
    }
    request
        .questions()
        .unwrap()
        .iter()
        .map(|question| {
            let id = question["id"].as_str().unwrap();
            let quoted_question = question["question"]
                .as_str()
                .unwrap_or("Question")
                .lines()
                .map(|line| format!("> {}", literal(line)))
                .collect::<Vec<_>>()
                .join("\n");
            let answers = response["answers"][id]["answers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|answer| {
                    let text = answer.as_str().unwrap();
                    let escaped = text.lines().map(literal).collect::<Vec<_>>().join("  \n");
                    if !text.contains('\n') && text == text.trim() {
                        format!("**{escaped}**")
                    } else {
                        escaped
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            format!("{quoted_question}\n\n{answers}")
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

fn validate_answer(request: &InputRequest, response: &Value) -> Result<()> {
    if request.is_approval() {
        let decision = response["decision"]
            .as_str()
            .context("Choose an approval decision")?;
        ensure!(
            ["accept", "acceptForSession", "decline", "cancel"].contains(&decision),
            "Unsupported approval decision"
        );
        if let Some(available) = request.offered_decisions() {
            ensure!(
                available.iter().any(|offered| offered == decision),
                "Decision was not offered by this request"
            )
        }
    } else {
        let questions = request.questions().context("Malformed question")?;
        for question in questions {
            let id = question["id"]
                .as_str()
                .context("Missing question identity")?;
            ensure!(
                response["answers"][id]["answers"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty()
                        && a.iter()
                            .all(|s| { s.as_str().is_some_and(|text| !text.trim().is_empty()) })),
                "Answer every question before submitting"
            )
        }
    }
    Ok(())
}
