//! Host-owned PTYs and explicit, revision-bound project checks.
use crate::Shared;
use anyhow::{Context, Result, ensure};
use kodem_core::{git, *};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
struct Screen {
    parser: vt100::Parser,
    running: bool,
    exit_code: Option<u32>,
}
pub(super) struct Terminal {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    screen: Arc<Mutex<Screen>>,
}
impl Terminal {
    /// Starts one owned login shell in the selected task checkout.
    pub(super) fn start(cwd: &Path) -> Result<Self> {
        let pair = portable_pty::native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut command = CommandBuilder::new(shell);
        command.arg("-l");
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        let mut child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let screen = Arc::new(Mutex::new(Screen {
            parser: vt100::Parser::new(24, 100, 5000),
            running: true,
            exit_code: None,
        }));
        let output = screen.clone();
        std::thread::spawn(move || {
            let mut bytes = [0; 16384];
            while let Ok(n) = reader.read(&mut bytes) {
                if n == 0 {
                    break;
                }
                output.lock().unwrap().parser.process(&bytes[..n]);
            }
        });
        let status = screen.clone();
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code());
            let mut s = status.lock().unwrap();
            s.running = false;
            s.exit_code = code;
        });
        Ok(Self {
            master: pair.master,
            writer,
            killer,
            screen,
        })
    }
    /// Captures the terminal screen and process state.
    pub(super) fn view(&self) -> TerminalView {
        let s = self.screen.lock().unwrap();
        TerminalView {
            text: s.parser.screen().contents(),
            running: s.running,
            exit_code: s.exit_code,
        }
    }
    /// Reports whether the owned shell is still alive.
    pub(super) fn running(&self) -> bool {
        self.screen.lock().unwrap().running
    }
    /// Writes explicit user input to the task PTY.
    pub(super) fn write(&mut self, data: &[u8]) -> Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }
    /// Resizes the PTY and its terminal parser together.
    pub(super) fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        ensure!(
            (5..=200).contains(&rows) && (20..=400).contains(&cols),
            "Invalid terminal dimensions"
        );
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        self.screen
            .lock()
            .unwrap()
            .parser
            .screen_mut()
            .set_size(rows, cols);
        Ok(())
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.killer.kill();
    }
}
/// Runs the requested check and records its revision, output, and exit code.
pub(super) fn run_check(host: Shared, task_id: String, command: String) -> Result<()> {
    ensure!(
        !command.trim().is_empty() && command.len() < 8192,
        "Enter a build or check command"
    );
    let cwd = {
        let mut h = host.lock().unwrap();
        ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
        ensure!(
            !h.checks.contains(&task_id),
            "A check is already running for this task"
        );
        let task = h.store.task(&task_id)?;
        h.checks.insert(task_id.clone());
        task.cwd
    };
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            let revision = git::head(&cwd).unwrap_or_else(|| "unborn HEAD".into());
            let before = git::command(
                &cwd,
                &["status".as_ref(), "--porcelain=v1".as_ref(), "-z".as_ref()],
            )?;
            let mut child = Command::new("/bin/zsh")
                .args(["-lc", &command])
                .current_dir(&cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            let stdout = child.stdout.take().context("Missing check stdout")?;
            let stderr = child.stderr.take().context("Missing check stderr")?;
            fn drain(mut reader: impl Read) -> String {
                let mut kept = Vec::new();
                let mut bytes = [0; 8192];
                while let Ok(n) = reader.read(&mut bytes) {
                    if n == 0 {
                        break;
                    }
                    let take = n.min((512 * 1024usize).saturating_sub(kept.len()));
                    kept.extend_from_slice(&bytes[..take]);
                }
                String::from_utf8_lossy(&kept).into()
            }
            let out = std::thread::spawn(move || drain(stdout));
            let err = std::thread::spawn(move || drain(stderr));
            let code = child.wait()?;
            let text = format!(
                "$ {command}\nDirectory: {}\nHEAD before: {revision}\nWorking status digest: {}\nEnvironment: {} / {}\nExit: {}\nOutput limited to 512 KiB per stream.\n\n{}\n{}",
                cwd.display(),
                git::digest(&before),
                std::env::consts::OS,
                std::env::consts::ARCH,
                code.code().map_or("signal".into(), |c| c.to_string()),
                out.join().unwrap_or_default(),
                err.join().unwrap_or_default()
            );
            host.lock().unwrap().store.message(&Message {
                seq: 0,
                id: new_id(),
                task_id: task_id.clone(),
                kind: MessageKind::Check,
                attachments: Vec::new(),
                feedback: Vec::new(),
                text,
                complete: true,
                created: now(),
            })?;
            Ok(())
        })();
        let mut h = host.lock().unwrap();
        h.checks.remove(&task_id);
        if let Err(e) = result {
            let _ = h.store.message(&Message {
                seq: 0,
                id: new_id(),
                task_id,
                kind: MessageKind::Error,
                attachments: Vec::new(),
                feedback: Vec::new(),
                text: format!("Check failed to run: {e:#}"),
                complete: true,
                created: now(),
            });
        }
    });
    Ok(())
}
