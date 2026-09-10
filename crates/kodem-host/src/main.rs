//! On-demand local host. Window closure never drops provider or PTY ownership.
mod attachments;
mod provider;
mod terminal;
mod titles;
use anyhow::{Context, Result, ensure};
use kodem_core::{git, ipc, runtime, storage::Store, *};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    os::unix::{fs::PermissionsExt, io::AsRawFd, net::UnixListener},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

struct Host {
    store: Store,
    runtime: RuntimeInfo,
    sessions: HashMap<String, mpsc::Sender<provider::Control>>,
    requests: HashMap<String, Vec<InputRequest>>,
    activities: HashMap<String, Vec<Activity>>,
    terminals: HashMap<String, terminal::Terminal>,
    checks: std::collections::HashSet<String>,
    shutdown: bool,
    usage_refresh: Option<Instant>,
    usage_refreshing: bool,
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
    let version = runtime::version();
    let info = RuntimeInfo {
        version: version.as_ref().ok().cloned(),
        error: version.err().map(|e| e.to_string()),
        ..Default::default()
    };
    let host = Arc::new(Mutex::new(Host {
        store,
        runtime: info,
        sessions: HashMap::new(),
        requests: HashMap::new(),
        activities: HashMap::new(),
        terminals: HashMap::new(),
        checks: Default::default(),
        shutdown: false,
        usage_refresh: None,
        usage_refreshing: false,
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
            let active = h.store.tasks()?.iter().any(|t| t.state.active())
                || !h.checks.is_empty()
                || h.terminals.values().any(|t| t.running());
            if !active && activity.elapsed() > Duration::from_secs(300) {
                break;
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
    Ok(TaskView {
        activities: h.activities.get(id).cloned().unwrap_or_default(),
        task: h.store.task(id)?,
        outgoing: h.store.outgoing(id)?,
        messages,
        before,
        requests: h.requests.get(id).cloned().unwrap_or_default(),
    })
}
fn handle(host: &Shared, root: &std::path::Path, request: Request) -> Result<Response> {
    ensure!(
        !host.lock().unwrap().shutdown,
        "Host is restarting. Retry after reconnecting."
    );
    match request {
        Request::Hello => {
            let mut h = host.lock().unwrap();
            if !h.usage_refreshing
                && h.usage_refresh
                    .is_none_or(|last| last.elapsed() >= Duration::from_secs(120))
            {
                h.usage_refresh = Some(Instant::now());
                h.usage_refreshing = true;
                let usage_host = host.clone();
                std::thread::spawn(move || {
                    let _ = provider::refresh_usage(&usage_host);
                    usage_host.lock().unwrap().usage_refreshing = false;
                });
            }
            Ok(Response::Workspace(Workspace {
                protocol: ipc::PROTOCOL,
                projects: h.store.projects()?,
                tasks: h.store.tasks()?,
                settings: h.store.settings()?,
                runtime: h.runtime.clone(),
            }))
        }
        Request::Reconcile { task_id } => {
            provider::reconcile(host, &task_id)?;
            Ok(Response::Ok)
        }
        Request::ConnectRuntime => {
            provider::discover(host.clone(), None)?;
            Ok(Response::Ok)
        }
        Request::OpenProject { path } => {
            let path = git::repository(&path)?;
            Ok(Response::Project(
                host.lock().unwrap().store.add_project(path)?,
            ))
        }
        Request::CreateTask {
            project_id,
            worktree,
        } => {
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
            let branch = git::branch(&cwd);
            let defaults = host.lock().unwrap().store.settings()?.agent_defaults;
            let task = Task {
                id,
                project_id,
                title: "New task".into(),
                title_pending: true,
                cwd,
                owns_worktree: worktree,
                baseline,
                branch,
                provider_thread: None,
                active_turn: None,
                state: RunState::Idle,
                draft: String::new(),
                model: defaults.model,
                effort: defaults.effort,
                access: defaults.access,
                review: None,
                context_usage: None,
                pinned: false,
                archived: false,
                created: now(),
                updated: now(),
                feedback: Vec::new(),
                attachments: Vec::new(),
                error: None,
            };
            host.lock().unwrap().store.save_task(&task)?;
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
            let preferences_changed = matches!(&edit, TaskEdit::Model { .. } | TaskEdit::Access(_));
            let mut defaults = h.store.settings()?.agent_defaults;
            match edit {
                TaskEdit::Access(access) => {
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
                }
            }
            if preferences_changed {
                h.store.save_task_preferences(&t, defaults)?;
            } else {
                h.store.save_task(&t)?;
            }
            Ok(Response::Ok)
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
        Request::Interrupt { task_id } => {
            let h = host.lock().unwrap();
            let tx = h.sessions.get(&task_id).context(
                "No live provider session. Recovered tasks are not automatically replayed.",
            )?;
            tx.send(provider::Control::Interrupt)?;
            Ok(Response::Ok)
        }
        Request::Answer {
            task_id,
            id,
            response,
        } => {
            let h = host.lock().unwrap();
            let request = h
                .requests
                .get(&task_id)
                .and_then(|r| r.iter().find(|r| r.id == id))
                .context("This request is no longer pending")?;
            validate_answer(request, &response)?;
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
        Request::Resources { task_id } => {
            Ok(Response::Resources(provider::resources(host, task_id)?))
        }
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
                    .is_some_and(|a| !a.is_empty() && a.iter().all(|s| s.is_string())),
                "Answer every question before submitting"
            )
        }
    }
    Ok(())
}
