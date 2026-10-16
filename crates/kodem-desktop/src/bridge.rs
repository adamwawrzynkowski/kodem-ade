//! Blocking host transport kept outside GPUI's foreground executor.
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use kodem_core::{Request, Response, ipc};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// Shortest time between two watch requests, which bounds how often the window is updated.
const WATCH_SPACING: Duration = Duration::from_millis(200);
/// Wait before the next watch request after one failed.
const WATCH_RETRY: Duration = Duration::from_millis(600);
pub enum Command {
    Flush(mpsc::Sender<()>),
    PrepareUpdate {
        settings: Option<Box<kodem_core::Settings>>,
        draft: Option<(String, String)>,
        done: mpsc::Sender<Result<(), String>>,
    },
    ResumeAfterUpdate,
    Request(Request),
    ImportAttachments {
        task_id: String,
        sources: Vec<kodem_core::AttachmentSource>,
        images: Vec<tempfile::TempPath>,
    },
    /// The selected task, whether its terminal and a subagent are shown, and whether the
    /// window already holds the conversation of the task, which a watcher then keeps current.
    Poll(Option<String>, bool, Option<String>, bool),
}
pub struct Reply {
    pub request: Request,
    pub poll_complete: bool,
    pub connected: bool,
    pub result: Result<Response, String>,
}
/// Follows the conversation of the task that the window shows.
///
/// A watch request waits on the host until the task changes, so it has its own connection and
/// thread: the ordered worker must stay free for Send and Stop. A request in flight cannot be
/// cancelled. When the window turns to another task, the thread of the earlier one ends as soon
/// as its request returns, and its answer is dropped.
struct Watcher {
    task: Option<String>,
    generation: Arc<AtomicU64>,
}
impl Watcher {
    fn new() -> Self {
        Self {
            task: None,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
    /// Follows `task`, or nothing. The same task as before keeps its watcher and its cursor.
    fn follow(&mut self, root: &Path, task: Option<String>, replies: &UnboundedSender<Reply>) {
        if self.task == task {
            return;
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.task = task.clone();
        let Some(task_id) = task else {
            return;
        };
        let (root, current, replies) = (root.to_owned(), self.generation.clone(), replies.clone());
        std::thread::spawn(move || watch(&root, task_id, generation, &current, &replies));
    }
    /// Stops following without a host directory, as before an update or after a lost host.
    fn stop(&mut self) {
        if self.task.take().is_some() {
            self.generation.fetch_add(1, Ordering::SeqCst);
        }
    }
}
impl Drop for Watcher {
    // The watcher thread must not outlive the worker of a closed window.
    fn drop(&mut self) {
        self.stop();
    }
}

// Asks the host for changes of one task until another generation takes over. The first request
// has no cursor and is answered at once with the newest page; each later one carries the cursor
// of the answer before it. A failure is not reported: the ordered worker shows a lost host and
// reconnects, and the watcher only waits.
fn watch(
    root: &Path,
    task_id: String,
    generation: u64,
    current: &AtomicU64,
    replies: &UnboundedSender<Reply>,
) {
    let mut after = None;
    while current.load(Ordering::SeqCst) == generation {
        let started = Instant::now();
        let request = Request::Watch {
            task_id: task_id.clone(),
            after: after.clone(),
        };
        match ipc::request(root, &request) {
            Ok(Response::Watched(start)) => {
                if current.load(Ordering::SeqCst) != generation {
                    return;
                }
                let (view, through) = start.into_parts();
                after = Some(through);
                let reply = Reply {
                    request,
                    poll_complete: false,
                    connected: true,
                    result: Ok(Response::Task(view)),
                };
                if replies.unbounded_send(reply).is_err() {
                    return;
                }
            }
            _ => std::thread::sleep(WATCH_RETRY),
        }
        std::thread::sleep(WATCH_SPACING.saturating_sub(started.elapsed()));
    }
}

/// Creates ordered interaction and repository workers for this window.
///
/// Replies arrive on an asynchronous channel, so the window applies each one as it comes.
pub fn start() -> (mpsc::Sender<Command>, UnboundedReceiver<Reply>) {
    start_at(ipc::data_dir().map_err(|e| format!("{e:#}")))
}
fn start_at(root: Result<PathBuf, String>) -> (mpsc::Sender<Command>, UnboundedReceiver<Reply>) {
    let (tx, commands) = mpsc::channel();
    let (replies, rx) = unbounded();
    let (repository_tx, repository_rx) = mpsc::channel::<(PathBuf, Request)>();
    let repository_replies = replies.clone();
    std::thread::spawn(move || {
        while let Ok((root, request)) = repository_rx.recv() {
            let result = ipc::request(&root, &request).map_err(|e| format!("{e:#}"));
            if repository_replies
                .unbounded_send(Reply {
                    request,
                    poll_complete: false,
                    connected: true,
                    result,
                })
                .is_err()
            {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        let mut paused = false;
        let mut watcher = Watcher::new();
        // The host answered the handshake and nothing failed since. The handshake reads the
        // whole workspace, so it runs only to start or check a host, not before each request.
        let mut verified = false;
        while let Ok(command) = commands.recv() {
            let poll = matches!(command, Command::Poll(..));
            // Each poll names the conversation to follow: the selected task once the window
            // holds its first page, and nothing before that.
            let watched = match &command {
                Command::Poll(task, _, _, viewed) => Some(task.clone().filter(|_| *viewed)),
                _ => None,
            };
            // Keep clipboard files alive until the host has copied them, even if the window closes.
            let _clipboard_images;
            let requests = match command {
                Command::PrepareUpdate {
                    settings,
                    draft,
                    done,
                } => {
                    let result = root.as_ref().map_err(Clone::clone).and_then(|root| {
                        if settings.is_some() || draft.is_some() {
                            ipc::connect_or_start(root).map_err(|e| format!("{e:#}"))?;
                        }
                        if let Some(settings) = settings {
                            ipc::request(root, &Request::SaveSettings(settings))
                                .map_err(|e| format!("Could not save settings: {e:#}"))?;
                        }
                        if let Some((task_id, text)) = draft {
                            ipc::request(
                                root,
                                &Request::EditTask {
                                    task_id,
                                    edit: kodem_core::TaskEdit::Draft(text),
                                },
                            )
                            .map_err(|e| format!("Could not save draft: {e:#}"))?;
                        }
                        if !ipc::socket(root).exists() {
                            return Ok(());
                        }
                        match ipc::request(root, &Request::Shutdown)
                            .map_err(|e| format!("{e:#}"))?
                        {
                            Response::Ok => Ok(()),
                            _ => Err("The host did not accept update shutdown".into()),
                        }
                    });
                    paused = result.is_ok();
                    if paused {
                        watcher.stop();
                        verified = false;
                    }
                    let _ = done.send(result);
                    continue;
                }
                Command::ResumeAfterUpdate => {
                    paused = false;
                    continue;
                }
                Command::Flush(done) => {
                    let _ = done.send(());
                    continue;
                }
                Command::Request(r) => vec![r],
                Command::ImportAttachments {
                    task_id,
                    sources,
                    images,
                } => {
                    _clipboard_images = images;
                    vec![Request::ImportAttachments { task_id, sources }]
                }
                Command::Poll(task, terminal, agent, viewed) => {
                    let mut r = vec![Request::Hello];
                    if let Some(task_id) = task {
                        // The watcher keeps a conversation current once the window holds it.
                        if !viewed {
                            r.push(Request::Task {
                                task_id: task_id.clone(),
                                before: None,
                            });
                        }
                        if let Some(thread_id) = agent {
                            r.push(Request::SubagentThread {
                                task_id: task_id.clone(),
                                thread_id,
                                before: None,
                            });
                        }
                        if terminal {
                            r.push(Request::Terminal { task_id });
                        }
                    }
                    r
                }
            };
            if paused {
                let count = requests.len();
                for (index, request) in requests.into_iter().enumerate() {
                    let _ = replies.unbounded_send(Reply {
                        request,
                        poll_complete: poll && index + 1 == count,
                        connected: false,
                        result: Err("Kodem is restarting to install an update".into()),
                    });
                }
                continue;
            }
            // Keep this worker alive after a failed handshake. Polls and the explicit
            // refresh action reconnect; rejected mutations are reported, never replayed.
            let connection = root.as_ref().map_err(Clone::clone).and_then(|root| {
                if !verified {
                    ipc::connect_or_start(root).map_err(|e| format!("{e:#}"))?;
                    verified = true;
                }
                Ok(root)
            });
            let root = match connection {
                Ok(root) => root,
                Err(error) => {
                    watcher.stop();
                    if replies
                        .unbounded_send(Reply {
                            request: requests.into_iter().next().unwrap(),
                            poll_complete: poll,
                            connected: false,
                            result: Err(error),
                        })
                        .is_err()
                    {
                        return;
                    }
                    continue;
                }
            };
            if let Some(task) = watched {
                watcher.follow(root, task, &replies);
            }
            let request_count = requests.len();
            for (index, request) in requests.into_iter().enumerate() {
                // Preserve capture/reply order without making conversation actions wait for Git.
                if matches!(
                    request,
                    Request::CaptureDiff { .. }
                        | Request::TaskChanges { .. }
                        | Request::Files { .. }
                        | Request::ReadFile { .. }
                ) {
                    let _ = repository_tx.send((root.clone(), request));
                    continue;
                }
                // Account operations and catalog discovery do not depend on task mutations.
                // Provider latency must not block Send, Stop, or conversation polling. Listing
                // review targets and starting a review can wait for GitHub and a fetch. Reading
                // and saving an edited file must not wait behind a capture of the changes.
                if matches!(
                    request,
                    Request::ComposerCatalog { .. }
                        | Request::ReadEditableFile { .. }
                        | Request::SaveFile { .. }
                        | Request::ConnectRuntime { .. }
                        | Request::RefreshUsage { .. }
                        | Request::ConsumeUsageReset { .. }
                        | Request::ReviewTargets { .. }
                        | Request::CreateReview { .. }
                        | Request::InspectTools
                        | Request::InstallTool { .. }
                ) {
                    let root = root.clone();
                    let replies = replies.clone();
                    std::thread::spawn(move || {
                        let result = ipc::request(&root, &request).map_err(|e| format!("{e:#}"));
                        let _ = replies.unbounded_send(Reply {
                            request,
                            poll_complete: false,
                            connected: true,
                            result,
                        });
                    });
                    continue;
                }
                let mut result = ipc::request(root, &request);
                if result.as_ref().is_err_and(ipc::unreachable) {
                    // The host stopped since the handshake and never received the request, so
                    // it goes once more to a host that runs.
                    result =
                        ipc::connect_or_start(root).and_then(|()| ipc::request(root, &request));
                }
                // A failure or a host of another protocol is checked by the next handshake.
                verified = match &result {
                    Ok(Response::Workspace(workspace)) => workspace.protocol == ipc::PROTOCOL,
                    Ok(_) => true,
                    Err(_) => false,
                };
                let result = result.map_err(|e| format!("{e:#}"));
                let connected = result.is_ok() || !matches!(request, Request::Hello);
                if replies
                    .unbounded_send(Reply {
                        request,
                        poll_complete: poll && (index + 1 == request_count || !connected),
                        connected,
                        result,
                    })
                    .is_err()
                {
                    return;
                }
                if !connected {
                    break;
                }
            }
        }
    });
    (tx, rx)
}
