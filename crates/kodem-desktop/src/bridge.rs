//! Blocking host transport kept outside GPUI's foreground executor.
use kodem_core::{Request, Response, ipc};
use std::sync::mpsc;
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
    Poll(Option<String>, bool, Option<String>),
}
pub struct Reply {
    pub request: Request,
    pub poll_complete: bool,
    pub connected: bool,
    pub result: Result<Response, String>,
}
/// Creates ordered interaction and repository workers for this window.
pub fn start() -> (mpsc::Sender<Command>, mpsc::Receiver<Reply>) {
    start_at(ipc::data_dir().map_err(|e| format!("{e:#}")))
}
fn start_at(
    root: Result<std::path::PathBuf, String>,
) -> (mpsc::Sender<Command>, mpsc::Receiver<Reply>) {
    let (tx, commands) = mpsc::channel();
    let (replies, rx) = mpsc::channel();
    let (repository_tx, repository_rx) = mpsc::channel::<(std::path::PathBuf, Request)>();
    let repository_replies = replies.clone();
    std::thread::spawn(move || {
        while let Ok((root, request)) = repository_rx.recv() {
            let result = ipc::request(&root, &request).map_err(|e| format!("{e:#}"));
            if repository_replies
                .send(Reply {
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
        while let Ok(command) = commands.recv() {
            let poll = matches!(command, Command::Poll(..));
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
                Command::Poll(task, terminal, agent) => {
                    let mut r = vec![Request::Hello];
                    if let Some(task_id) = task {
                        r.push(Request::Task {
                            task_id: task_id.clone(),
                            before: None,
                        });
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
                    let _ = replies.send(Reply {
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
                ipc::connect_or_start(root).map_err(|e| format!("{e:#}"))?;
                Ok(root)
            });
            let root = match connection {
                Ok(root) => root,
                Err(error) => {
                    if replies
                        .send(Reply {
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
                // review targets and starting a review can wait for GitHub and a fetch.
                if matches!(
                    request,
                    Request::ComposerCatalog { .. }
                        | Request::ConnectRuntime { .. }
                        | Request::RefreshUsage { .. }
                        | Request::ConsumeUsageReset { .. }
                        | Request::ReviewTargets { .. }
                        | Request::CreateReview { .. }
                ) {
                    let root = root.clone();
                    let replies = replies.clone();
                    std::thread::spawn(move || {
                        let result = ipc::request(&root, &request).map_err(|e| format!("{e:#}"));
                        let _ = replies.send(Reply {
                            request,
                            poll_complete: false,
                            connected: true,
                            result,
                        });
                    });
                    continue;
                }
                let result = ipc::request(root, &request).map_err(|e| format!("{e:#}"));
                let connected = result.is_ok() || !matches!(request, Request::Hello);
                if replies
                    .send(Reply {
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
