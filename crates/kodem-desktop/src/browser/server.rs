//! The private socket through which the MCP servers of tasks reach the window's browser.
//!
//! One thread accepts connections for the process, and each connection gets a thread that waits
//! for its reply, so a slow page never blocks the window. The window that opened last receives
//! the jobs.
use super::{Job, LOAD_LIMIT, Outcome, Status, wait_for_load};
use futures::channel::mpsc::UnboundedSender;
use kodem_core::{
    browser::{self, BrowserReply, BrowserRequest},
    ipc, platform,
};
use std::{
    fs,
    sync::{Mutex, Once, mpsc},
    time::Duration,
};

/// How long a command that may navigate waits for the navigation to start.
const NAVIGATION_START: Duration = Duration::from_millis(250);

// The window that runs the jobs.
static WINDOW: Mutex<Option<UnboundedSender<Job>>> = Mutex::new(None);
static LISTENER: Once = Once::new();

/// Sends the browser jobs of agents to `jobs`, and starts the socket the first time.
pub fn serve(jobs: UnboundedSender<Job>) {
    if let Ok(mut window) = WINDOW.lock() {
        *window = Some(jobs);
    }
    LISTENER.call_once(|| {
        std::thread::spawn(|| {
            if let Err(error) = listen() {
                eprintln!("The browser of the window cannot serve agents: {error:#}");
            }
        });
    });
}

fn listen() -> anyhow::Result<()> {
    let root = ipc::data_dir()?;
    ipc::prepare(&root)?;
    let endpoint = browser::socket(&root);
    // Another Kodem window of this data directory serves agents already.
    if ipc::UnixStream::connect(&endpoint).is_ok() {
        return Ok(());
    }
    match fs::remove_file(&endpoint) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let listener = ipc::UnixListener::bind(&endpoint)?;
    platform::restrict(&endpoint, 0o600)?;
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        // Reject connections from any other local account even if filesystem permissions change.
        if !ipc::same_user(&stream) {
            continue;
        }
        std::thread::spawn(move || answer(stream));
    }
    Ok(())
}

// Reads one request, has the window run it, and writes the reply.
fn answer(mut stream: ipc::UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(15)));
    let reply = match ipc::read_frame::<BrowserRequest>(&mut stream) {
        Ok(request) => run(request),
        Err(error) => BrowserReply::Error(format!("{error:#}")),
    };
    let _ = ipc::write_frame(&mut stream, &reply);
}

fn run(request: BrowserRequest) -> BrowserReply {
    let wait = request.command.wait();
    // The first try waits for a page that loads; the second runs on whatever the page is then.
    for patient in [true, false] {
        let (reply, outcome) = mpsc::channel();
        let job = Job {
            task_id: request.task_id.clone(),
            command: request.command.clone(),
            patient,
            reply,
        };
        let sent = WINDOW
            .lock()
            .ok()
            .and_then(|window| window.as_ref()?.unbounded_send(job).ok());
        if sent.is_none() {
            return BrowserReply::Error(
                "The Kodem window is not open, so its browser cannot be used.".into(),
            );
        }
        match outcome.recv_timeout(wait) {
            Ok(Outcome::Done { reply, settle }) => {
                return match settle {
                    Some(status) => settled(reply, &status),
                    None => reply,
                };
            }
            Ok(Outcome::Loading(status)) => {
                wait_for_load(&status, LOAD_LIMIT);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return BrowserReply::Error(format!(
                    "The browser did not answer in {} s. The page may be busy.",
                    wait.as_secs()
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return BrowserReply::Error(
                    "The Kodem window closed before the command finished.".into(),
                );
            }
        }
    }
    BrowserReply::Error("The page kept loading.".into())
}

// Waits for the navigation that a command may have started, and adds where the page is now.
fn settled(reply: BrowserReply, status: &Mutex<Status>) -> BrowserReply {
    let BrowserReply::Text(text) = reply else {
        return reply;
    };
    std::thread::sleep(NAVIGATION_START);
    wait_for_load(status, LOAD_LIMIT);
    let Ok(mut status) = status.lock() else {
        return BrowserReply::Text(text);
    };
    let mut text = format!("{text}\nPage: {}\nURL: {}", status.title, status.url);
    if status.loading.is_some() {
        // A page that never loads must not hold the commands that follow.
        status.loading = None;
        text.push_str(&format!(
            "\nThe page did not finish loading in {} s. If it is a local server, check that it runs.",
            LOAD_LIMIT.as_secs()
        ));
    }
    BrowserReply::Text(text)
}
