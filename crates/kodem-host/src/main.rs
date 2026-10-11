//! On-demand local host. Window closure never drops provider or PTY ownership.
mod accounts;
mod attachments;
mod notifications;
mod project_icons;
mod provider;
mod settlement;
mod terminal;
mod titles;
use anyhow::{Context, Result, ensure};
use kodem_core::{git, ipc, providers::codex, storage::Store, *};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    os::unix::{fs::PermissionsExt, io::AsRawFd, net::UnixListener},
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
    shutdown: bool,
    usage_refresh: Option<Instant>,
    usage_refreshing: bool,
    settlement_refresh: Option<Instant>,
    settlement_refreshing: bool,
}
type Shared = Arc<Mutex<Host>>;
fn main() {
    if let Err(error) = run() {
        eprintln!("Kodem host: {error:#}");
        std::process::exit(1)
    }
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
    if fs2_lock(&lock).is_err() {
        return Ok(());
    }
    let endpoint = ipc::socket(&root);
    if endpoint.exists() {
        fs::remove_file(&endpoint)?;
    }
    let listener = UnixListener::bind(&endpoint)?;
    fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
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
        ]),
        accounts: BTreeMap::new(),
        sessions: HashMap::new(),
        requests: HashMap::new(),
        activities: HashMap::new(),
        compactions: HashMap::new(),
        compacted: HashMap::new(),
        terminals: HashMap::new(),
        checks: Default::default(),
        shutdown: false,
        usage_refresh: None,
        usage_refreshing: false,
        settlement_refresh: None,
        settlement_refreshing: false,
    }));
    let mut activity = Instant::now();
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // macOS inherits the listener's nonblocking flag on accepted sockets.
                // Framed workers must wait for the complete request and response.
                stream.set_nonblocking(false)?;
                // Reject connections from any other local account even if filesystem permissions change.
                let (mut uid, mut gid) = (0, 0);
                ensure!(
                    unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } == 0
                        && uid == unsafe { libc::geteuid() },
                    "Unexpected IPC peer"
                );
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
fn fs2_lock(file: &fs::File) -> Result<()> {
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    ensure!(result == 0, "Host already running");
    Ok(())
}
fn task_view(h: &Host, id: &str, before: Option<u64>) -> Result<TaskView> {
    let (messages, before) = h.store.messages(id, before)?;
    let task = h.store.task(id)?;
    let mut requests = h.requests.get(id).cloned().unwrap_or_default();
    requests.extend(task.questions.iter().cloned());
    Ok(TaskView {
        activities: h.activities.get(id).cloned().unwrap_or_default(),
        task,
        outgoing: h.store.outgoing(id)?,
        messages,
        before,
        requests,
    })
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
                .filter(|task| projects.iter().any(|p| p.id == task.project_id))
                .collect();
            Ok(Response::Workspace(Workspace {
                notification_delivery: h.notifications.status(),
                protocol: ipc::PROTOCOL,
                projects,
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
        Request::OpenProject { path } => {
            let path = git::repository(&path)?;
            Ok(Response::Project(
                host.lock().unwrap().store.add_project(path)?,
            ))
        }
        Request::CustomizeProject {
            project_id,
            name,
            color,
            icon,
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
            let result =
                host.lock()
                    .unwrap()
                    .store
                    .customize_project(&project_id, &name, color, saved_icon);
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
        } => {
            provider::validate_preferences(&host.lock().unwrap(), &defaults)?;
            let p = host
                .lock()
                .unwrap()
                .store
                .projects()?
                .into_iter()
                .find(|p| p.id == project_id)
                .context("Project not found")?;
            let id = new_id();
            let cwd = if worktree {
                git::worktree(&p.path, &root.join("worktrees"), &id)?
            } else {
                p.path
            };
            let baseline = git::head(&cwd);
            let repository_baselines = git::repository_baselines(&cwd)?;
            let branch = git::branch(&cwd);
            let task = Task {
                subagents: Vec::new(),
                pending_completion: None,
                provider: defaults.provider,
                account: defaults.account.clone(),
                plan: Vec::new(),
                provider_access: defaults.provider_access.clone(),
                id,
                project_id,
                title: "New task".into(),
                title_pending: true,
                cwd,
                owns_worktree: worktree,
                baseline,
                repository_baselines,
                branch,
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
            };
            host.lock()
                .unwrap()
                .store
                .save_task_preferences(&task, defaults)?;
            Ok(Response::Created(task))
        }
        Request::Task { task_id, before } => Ok(Response::Task(task_view(
            &host.lock().unwrap(),
            &task_id,
            before,
        )?)),
        Request::EditTask { task_id, edit } => {
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
            let mut defaults = provider::preferences(&t);
            match edit {
                TaskEdit::Provider(preferences) => {
                    ensure!(
                        t.provider_thread.is_none()
                            && !h.sessions.contains_key(&task_id)
                            && !t.state.active()
                            && t.state != RunState::DeliveryUnknown
                            && h.store.outgoing(&task_id)?.is_empty(),
                        "Start a new conversation to change providers after this task has started"
                    );
                    provider::validate_saved_selection(&h, &preferences)?;
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
                    // Codex keeps thread history in per-account state, so a started thread cannot
                    // continue under another identity.
                    ensure!(
                        t.provider != Provider::Codex || t.provider_thread.is_none(),
                        "Codex cannot continue a started conversation under another account. Start a new task with that account."
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
            if preferences_changed {
                h.store.save_task_preferences(&t, defaults)?;
            } else {
                h.store.save_task(&t)?;
            }
            Ok(Response::Ok)
        }
        Request::DeleteTask { task_id } => {
            let mut h = host.lock().unwrap();
            ensure!(
                !h.checks.contains(&task_id)
                    && !h
                        .terminals
                        .get(&task_id)
                        .is_some_and(|terminal| terminal.running()),
                "Stop this task's check or terminal before deleting it"
            );
            h.store.delete_task(&task_id)?;
            if let Some(session) = h.sessions.get(&task_id) {
                let _ = session.send(provider::Control::Close);
            }
            h.requests.remove(&task_id);
            h.activities.remove(&task_id);
            h.compacted.remove(&task_id);
            h.terminals.remove(&task_id);
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
                attachments::discard(&imported);
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
            git::stage(&task, &snapshot, file)?;
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
    request.params["questions"]
        .as_array()
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
    if request.method.ends_with("requestApproval") {
        let decision = response["decision"]
            .as_str()
            .context("Choose an approval decision")?;
        ensure!(
            ["accept", "acceptForSession", "decline", "cancel"].contains(&decision),
            "Unsupported approval decision"
        );
        if let Some(available) = request.params["availableDecisions"].as_array() {
            ensure!(
                available.iter().any(|v| v.as_str() == Some(decision)),
                "Decision was not offered by this request"
            )
        }
    } else {
        let questions = request.params["questions"]
            .as_array()
            .context("Malformed question")?;
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
