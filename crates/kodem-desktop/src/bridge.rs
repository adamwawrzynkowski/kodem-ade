//! Blocking host transport kept outside GPUI's foreground executor.
use kodem_core::{Request, Response, ipc};
use std::sync::mpsc;
pub enum Command {
    Request(Request),
    Poll(Option<String>, bool),
}
pub struct Reply {
    pub request: Request,
    pub result: Result<Response, String>,
}
/// Creates one ordered background transport owner for this window.
pub fn start() -> (mpsc::Sender<Command>, mpsc::Receiver<Reply>) {
    let (tx, commands) = mpsc::channel();
    let (replies, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let root = match ipc::data_dir().and_then(|root| {
            ipc::connect_or_start(&root)?;
            Ok(root)
        }) {
            Ok(root) => root,
            Err(e) => {
                let _ = replies.send(Reply {
                    request: Request::Hello,
                    result: Err(format!("{e:#}")),
                });
                return;
            }
        };
        while let Ok(command) = commands.recv() {
            let requests = match command {
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
            for request in requests {
                let result = ipc::request(&root, &request).map_err(|e| format!("{e:#}"));
                if replies.send(Reply { request, result }).is_err() {
                    return;
                }
            }
        }
    });
    (tx, rx)
}
