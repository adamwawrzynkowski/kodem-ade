//! Bounded JSON-RPC transport for the native Devin ACP subprocess.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

pub(super) struct Transport {
    child: Child,
    stdin: ChildStdin,
    incoming: Receiver<Result<Value, String>>,
    deferred: VecDeque<Value>,
    next_id: u64,
}

/// Resolves the native CLI, including the installer location absent from GUI PATH values.
pub(super) fn executable() -> PathBuf {
    if let Some(path) = std::env::var_os("KODEM_DEVIN_PATH") {
        return PathBuf::from(path);
    }
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/bin/devin"));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/devin"),
        PathBuf::from("/usr/local/bin/devin"),
    ]);
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("devin"))
}

impl Transport {
    /// Starts ACP with file and terminal execution retained by the provider itself.
    pub(super) fn start(cwd: &Path) -> Result<(Self, Value)> {
        let mut child = Command::new(executable())
            .arg("acp")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start Devin CLI. Install it and sign in from Providers settings")?;
        let stdin = child.stdin.take().context("Missing Devin input")?;
        let stdout = child.stdout.take().context("Missing Devin output")?;
        let stderr = child.stderr.take().context("Missing Devin diagnostics")?;
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
        });
        let (sender, incoming) = mpsc::sync_channel(256);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                let result = reader
                    .by_ref()
                    .take(8 * 1024 * 1024 + 1)
                    .read_until(b'\n', &mut bytes);
                let value = match result {
                    Ok(0) => Err("Devin transport closed".into()),
                    Ok(_) if bytes.len() > 8 * 1024 * 1024 => {
                        Err("Devin event exceeded 8 MiB".into())
                    }
                    Ok(_) => serde_json::from_slice(&bytes)
                        .map_err(|error| format!("Invalid Devin JSON: {error}")),
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
            next_id: 1,
        };
        let initialized = transport.call("initialize", json!({
            "protocolVersion":1,
            "clientInfo":{"name":"kodem_ade","title":"Kodem ADE","version":env!("CARGO_PKG_VERSION")},
            "clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false}
        }))?;
        ensure!(
            initialized["protocolVersion"] == 1,
            "Devin selected an unsupported ACP protocol version"
        );
        Ok((transport, initialized))
    }

    fn write(&mut self, value: &Value) -> Result<()> {
        serde_json::to_writer(&mut self.stdin, value)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    /// Sends a request without blocking the task owner's cancellation and permission controls.
    pub(super) fn request(&mut self, method: &str, params: Value) -> Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        Ok(id)
    }

    /// Sends a notification, which has no JSON-RPC response.
    pub(super) fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.write(&json!({"jsonrpc":"2.0","method":method,"params":params}))
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
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("Devin transport disconnected"),
        }
    }

    /// Completes a short setup request while retaining notifications for the session owner.
    pub(super) fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.request(method, params)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            ensure!(Instant::now() < deadline, "Devin {method} timed out");
            let Some(value) = self.receive(Duration::from_millis(100))? else {
                continue;
            };
            if value.get("method").is_none() && value["id"].as_u64() == Some(id) {
                if let Some(error) = value.get("error") {
                    bail!("Devin {method}: {error}");
                }
                return value
                    .get("result")
                    .cloned()
                    .context("Devin response has no result");
            }
            if value.get("method").is_some() && value.get("id").is_some() {
                self.reject(value["id"].clone())?;
            } else {
                ensure!(
                    self.deferred.len() < 4096,
                    "Devin setup produced too many deferred events"
                );
                self.deferred.push_back(value);
            }
        }
    }

    /// Replies to a provider request with the user's explicitly selected outcome.
    pub(super) fn answer(&mut self, id: Value, result: Value) -> Result<()> {
        self.write(&json!({"jsonrpc":"2.0","id":id,"result":result}))
    }

    /// Rejects a client capability the application did not advertise or implement.
    pub(super) fn reject(&mut self, id: Value) -> Result<()> {
        self.write(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Kodem does not implement this ACP client method"}}))
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
