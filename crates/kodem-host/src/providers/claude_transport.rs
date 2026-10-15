//! Bounded stream-json transport for one Claude Code process and its control protocol.
use anyhow::{Context, Result, bail, ensure};
use kodem_core::{Account, CompactRule};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

/// A tool result can embed a whole file or image; larger lines are dropped, not fatal.
const LINE_LIMIT: u64 = 32 * 1024 * 1024;
const DIAGNOSTIC_LIMIT: usize = 4096;

/// Resolves the native CLI, including the installer location absent from GUI PATH values.
pub(super) fn executable() -> PathBuf {
    if let Some(path) = std::env::var_os("KODEM_CLAUDE_PATH") {
        return PathBuf::from(path);
    }
    kodem_core::platform::find_program("claude", &kodem_core::platform::tool_directories())
        .unwrap_or_else(|| PathBuf::from("claude"))
}

/// How a process relates to a conversation stored by the CLI.
pub(super) enum Conversation {
    /// Starts a conversation under an identity chosen by the host.
    New(String),
    /// Continues a conversation the CLI already stored.
    Resume(String),
    /// Answers control requests only and stores nothing.
    Ephemeral,
}

/// Everything fixed at process start. Changing the account or conversation needs a new process.
pub(super) struct Launch<'a> {
    pub(super) cwd: &'a Path,
    pub(super) account: &'a Account,
    pub(super) model: Option<&'a str>,
    pub(super) effort: Option<&'a str>,
    pub(super) access: &'a str,
    pub(super) conversation: Conversation,
    /// Skips MCP servers for short account reads that never run a turn.
    pub(super) light: bool,
    /// When the CLI compacts by itself; absent leaves its own configuration in effect.
    pub(super) compact: Option<CompactRule>,
    /// Turns on Artifacts, which the CLI keeps off in a session that an application embeds.
    pub(super) artifacts: bool,
    /// Standing instructions added to the CLI's own system prompt.
    pub(super) instructions: Option<&'a str>,
    /// Tools the session may use without asking.
    pub(super) allowed_tools: &'a [&'a str],
    /// Tools the session starts without.
    pub(super) disallowed_tools: &'a [&'a str],
    /// MCP servers that Kodem adds to those of the user, as the CLI's JSON configuration.
    pub(super) mcp: Option<&'a str>,
}

pub(super) struct Transport {
    child: Child,
    stdin: ChildStdin,
    incoming: Receiver<Result<Value, String>>,
    deferred: VecDeque<Value>,
    diagnostics: Arc<Mutex<String>>,
    next_id: u64,
}

/// A control request the CLI answered with an error, which proves it was not applied.
#[derive(Debug)]
pub(super) struct Rejected(pub(super) String);
impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Rejected {}

impl Transport {
    /// Starts the CLI under one account and completes the control handshake without a model call.
    pub(super) fn start(launch: &Launch) -> Result<(Self, Value)> {
        let mut command = Command::new(executable());
        launch.account.apply(&mut command);
        command.current_dir(launch.cwd).args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--permission-prompt-tool",
            "stdio",
            "--replay-user-messages",
            "--forward-subagent-text",
            "--thinking-display",
            "summarized",
            // Print mode can otherwise start in a mode the user never selected.
            "--permission-mode",
            launch.access,
        ]);
        if let Some(model) = launch.model {
            command.args(["--model", model]);
        }
        if let Some(effort) = launch.effort {
            command.args(["--effort", effort]);
        }
        match &launch.conversation {
            Conversation::New(id) => command.args(["--session-id", id]),
            Conversation::Resume(id) => command.args(["--resume", id]),
            Conversation::Ephemeral => command.arg("--no-session-persistence"),
        };
        // The option takes several values. Only options follow it, and input arrives on the
        // stream, so the CLI reads nothing else as a configuration.
        if let Some(mcp) = launch.mcp {
            command.args(["--mcp-config", mcp]);
        }
        if launch.light {
            command.arg("--strict-mcp-config");
        }
        if let Some(instructions) = launch.instructions {
            command.args(["--append-system-prompt", instructions]);
        }
        // Each rule is one argument, because a rule such as `Bash(git push:*)` holds
        // punctuation of its own. Only options follow, so the CLI reads no other argument as a
        // tool name.
        if !launch.allowed_tools.is_empty() {
            command.arg("--allowedTools").args(launch.allowed_tools);
        }
        if !launch.disallowed_tools.is_empty() {
            command
                .arg("--disallowedTools")
                .args(launch.disallowed_tools);
        }
        // The CLI reads both variables at start: a share of its context window in percent, and
        // a switch that turns off automatic compaction while `/compact` keeps working.
        match launch.compact {
            Some(rule) if rule.enabled => {
                command
                    .env(
                        "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE",
                        rule.threshold.to_string(),
                    )
                    .env_remove("DISABLE_AUTO_COMPACT");
            }
            Some(_) => {
                command
                    .env("DISABLE_AUTO_COMPACT", "1")
                    .env_remove("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE");
            }
            None => {}
        }
        // Print mode starts without Artifacts unless this variable is set. The `/design` and
        // `/slides` commands publish Artifacts, so the CLI lists them only when it is set.
        if launch.artifacts {
            command.env("CLAUDE_CODE_ARTIFACT", "1");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(
                "Could not start Claude Code. Install the claude CLI and sign in from Providers settings",
            )?;
        let stdin = child.stdin.take().context("Missing Claude Code input")?;
        let stdout = child.stdout.take().context("Missing Claude Code output")?;
        let stderr = child
            .stderr
            .take()
            .context("Missing Claude Code diagnostics")?;
        let diagnostics = Arc::new(Mutex::new(String::new()));
        let kept = diagnostics.clone();
        // Keep only a short tail: it explains a failed start and is never written to logs.
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = Vec::new();
            while reader
                .by_ref()
                .take(64 * 1024)
                .read_until(b'\n', &mut line)
                .is_ok_and(|read| read > 0)
            {
                let mut tail = kept.lock().unwrap();
                tail.push_str(&String::from_utf8_lossy(&line));
                if tail.len() > DIAGNOSTIC_LIMIT {
                    let mut cut = tail.len() - DIAGNOSTIC_LIMIT;
                    while !tail.is_char_boundary(cut) {
                        cut += 1;
                    }
                    tail.drain(..cut);
                }
                line.clear();
            }
        });
        let (sender, incoming) = mpsc::sync_channel(256);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                let result = reader
                    .by_ref()
                    .take(LINE_LIMIT + 1)
                    .read_until(b'\n', &mut bytes);
                let value = match result {
                    Ok(0) => Err("Claude Code closed its connection".into()),
                    Ok(_) if bytes.len() as u64 > LINE_LIMIT && !bytes.ends_with(b"\n") => {
                        // Skip the rest of an oversized event and keep the session alive.
                        let mut rest = Vec::new();
                        loop {
                            rest.clear();
                            match reader
                                .by_ref()
                                .take(1024 * 1024)
                                .read_until(b'\n', &mut rest)
                            {
                                Ok(0) => break,
                                Ok(_) if rest.ends_with(b"\n") => break,
                                Ok(_) => {}
                                Err(_) => break,
                            }
                        }
                        continue;
                    }
                    Ok(_) if bytes.iter().all(u8::is_ascii_whitespace) => continue,
                    Ok(_) => serde_json::from_slice(&bytes)
                        .map_err(|error| format!("Invalid Claude Code JSON: {error}")),
                    Err(error) => Err(error.to_string()),
                };
                let failed = value.is_err();
                if sender.send(value).is_err() || failed {
                    break;
                }
            }
        });
        let mut transport = Self {
            child,
            stdin,
            incoming,
            deferred: VecDeque::new(),
            diagnostics,
            next_id: 1,
        };
        let initialized = transport
            .call(json!({"subtype":"initialize","hooks":null}))
            .map_err(|error| transport.startup_error(error))?;
        Ok((transport, initialized))
    }

    // A failed start reports the CLI's own reason, such as a conversation it cannot find.
    fn startup_error(&mut self, error: anyhow::Error) -> anyhow::Error {
        let reported = self
            .deferred
            .iter()
            .find(|value| value["type"] == "result" && value["is_error"] == true)
            .and_then(|value| value["errors"][0].as_str().map(str::to_owned));
        let tail = self.diagnostics.lock().unwrap().clone();
        let reason = reported.or_else(|| {
            tail.lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().chars().take(300).collect())
        });
        match reason {
            Some(reason) => anyhow::anyhow!("Claude Code did not start: {reason}"),
            None => error.context("Claude Code did not start"),
        }
    }

    /// Writes one protocol line; a failure leaves delivery of that line unknown.
    pub(super) fn send(&mut self, value: &Value) -> Result<()> {
        serde_json::to_writer(&mut self.stdin, value)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    /// Sends a control request whose answer the session owner does not need to wait for.
    pub(super) fn request(&mut self, request: Value) -> Result<String> {
        let id = format!("kodem-{}", self.next_id);
        self.next_id += 1;
        self.send(&json!({"type":"control_request","request_id":id,"request":request}))?;
        Ok(id)
    }

    /// Returns one deferred or incoming protocol message within the requested interval.
    pub(super) fn poll(&mut self, timeout: Duration) -> Result<Option<Value>> {
        if let Some(value) = self.deferred.pop_front() {
            return Ok(Some(value));
        }
        self.receive(timeout)
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Value>> {
        match self.incoming.recv_timeout(timeout) {
            Ok(Ok(value)) => Ok(Some(value)),
            Ok(Err(error)) => bail!(error),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("Claude Code disconnected"),
        }
    }

    /// Completes a short control request while keeping every other event for the session owner.
    pub(super) fn call(&mut self, request: Value) -> Result<Value> {
        let subtype = request["subtype"].as_str().unwrap_or("request").to_owned();
        let id = self.request(request)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            ensure!(
                Instant::now() < deadline,
                "Claude Code did not answer {subtype} in time"
            );
            let Some(value) = self.receive(Duration::from_millis(100))? else {
                continue;
            };
            if value["type"] == "control_response" && value["response"]["request_id"] == id.as_str()
            {
                let response = &value["response"];
                if response["subtype"] == "success" {
                    return Ok(response["response"].clone());
                }
                return Err(anyhow::Error::new(Rejected(format!(
                    "Claude Code {subtype}: {}",
                    response["error"].as_str().unwrap_or("request was rejected")
                ))));
            }
            ensure!(
                self.deferred.len() < 8192,
                "Claude Code produced too many events while answering {subtype}"
            );
            self.deferred.push_back(value);
        }
    }

    /// Answers a request from the CLI with the user's explicitly selected outcome.
    pub(super) fn respond(&mut self, request_id: &str, response: Value) -> Result<()> {
        self.send(&json!({"type":"control_response","response":{
            "subtype":"success","request_id":request_id,"response":response}}))
    }

    /// Refuses a CLI request for a capability the host does not implement.
    pub(super) fn reject(&mut self, request_id: &str, message: &str) -> Result<()> {
        self.send(&json!({"type":"control_response","response":{
            "subtype":"error","request_id":request_id,"error":message}}))
    }

    /// Ends the process before another one may own its conversation.
    pub(super) fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.stop();
    }
}
