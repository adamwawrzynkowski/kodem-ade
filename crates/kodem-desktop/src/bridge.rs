//! Blocking host transport kept outside GPUI's foreground executor.
use kodem_core::{Request, Response, ipc};
use std::sync::mpsc;
pub enum Command {
    Flush(mpsc::Sender<()>),
    Request(Request),
    Poll(Option<String>, bool),
}
pub struct Reply {
    pub request: Request,
    pub connected: bool,
    pub result: Result<Response, String>,
}
/// Creates one ordered background transport owner for this window.
pub fn start() -> (mpsc::Sender<Command>, mpsc::Receiver<Reply>) {
    start_at(ipc::data_dir().map_err(|e| format!("{e:#}")))
}
fn start_at(
    root: Result<std::path::PathBuf, String>,
) -> (mpsc::Sender<Command>, mpsc::Receiver<Reply>) {
    let (tx, commands) = mpsc::channel();
    let (replies, rx) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(command) = commands.recv() {
            let requests = match command {
                Command::Flush(done) => {
                    let _ = done.send(());
                    continue;
                }
                Command::Request(r) => vec![r],
                Command::Poll(task, terminal) => {
                    let mut r = vec![Request::Hello];
                    if let Some(task_id) = task {
                        r.push(Request::Task {
                            task_id: task_id.clone(),
                            before: None,
                        });
                        if terminal {
                            r.push(Request::Terminal { task_id });
                        }
                    }
                    r
                }
            };
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
            for request in requests {
                let result = ipc::request(root, &request).map_err(|e| format!("{e:#}"));
                let connected = result.is_ok() || !matches!(request, Request::Hello);
                if replies
                    .send(Reply {
                        request,
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
