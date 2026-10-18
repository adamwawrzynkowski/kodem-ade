//! OpenCode's headless server: its process, local HTTP requests and its event stream.
//!
//! `opencode serve` listens on a loopback port that the system chooses and serves the API of
//! OpenCode 2 under `/api`. Every request carries the password of that one process, so another
//! local account cannot use the server. A session knows its folder; other requests name it with
//! [`Client::located`]. Events arrive as server-sent events on `/api/event`.
use crate::{model::*, platform};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

/// The user name that OpenCode expects with the server password.
const USER: &str = "opencode";
/// Longest wait for a new server to report its port.
const START_LIMIT: Duration = Duration::from_secs(45);
/// Longest wait for the answer of an ordinary request.
pub const REQUEST_LIMIT: Duration = Duration::from_secs(90);
/// Largest answer that a request reads, and largest event.
const BODY_LIMIT: usize = 64 * 1024 * 1024;
/// The line on which a new server reports its address.
const LISTENING: &str = "listening on http://";
/// What Kodem says when the installed CLI does not serve the API of OpenCode 2.
const UPDATE: &str =
    "Kodem supports OpenCode 2 or later. Update OpenCode with `opencode upgrade`, then try again.";
/// The argument that starts Kodem's host as the guard of one server.
pub const GUARD: &str = "opencode-guard";

/// Finds the OpenCode CLI: `KODEM_OPENCODE_PATH`, then the folder of its own installer, then
/// the folders where package managers put programs, then PATH.
pub fn executable() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("KODEM_OPENCODE_PATH") {
        return Ok(PathBuf::from(path));
    }
    let mut directories = Vec::new();
    if let Ok(home) = platform::home_dir() {
        directories.push(home.join(".opencode").join("bin"));
    }
    directories.extend(platform::tool_directories());
    platform::find_program("opencode", &directories)
        .context("OpenCode is not installed. Install it from Settings → Providers.")
}

/// Runs the CLI to read its version, without starting a server.
pub fn version() -> Result<String> {
    let mut command = Command::new(executable()?);
    platform::hide_console(&mut command);
    let output = command
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("OpenCode did not start")?;
    ensure!(output.status.success(), "OpenCode did not report a version");
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    ensure!(!version.is_empty(), "OpenCode did not report a version");
    Ok(version)
}

// Encodes a value for a query string. Only unreserved characters stay as they are.
fn encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

// Encodes the bytes of a password for HTTP basic authentication.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let value = chunk.iter().enumerate().fold(0u32, |value, (index, byte)| {
            value | u32::from(*byte) << (16 - 8 * index)
        });
        for index in 0..4 {
            if index <= chunk.len() {
                encoded.push(ALPHABET[(value >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

// Reads the body of a chunked answer up to the limit.
fn read_chunked(reader: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let mut size = String::new();
        reader.read_line(&mut size)?;
        let size = usize::from_str_radix(size.trim().split(';').next().unwrap_or_default(), 16)
            .context("OpenCode sent a malformed chunk")?;
        if size == 0 {
            return Ok(body);
        }
        ensure!(
            body.len() + size <= BODY_LIMIT,
            "OpenCode's answer exceeds 64 MiB"
        );
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..])?;
        let mut end = [0; 2];
        reader.read_exact(&mut end)?;
    }
}

// Reads the status line and the headers of an answer. Header names are lowercase.
fn read_head(reader: &mut impl BufRead) -> Result<(u16, Vec<(String, String)>)> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let status = line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .context("OpenCode sent no HTTP status")?;
    let mut headers = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line)?;
        let header = line.trim_end();
        if header.is_empty() {
            return Ok((status, headers));
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
        }
    }
}

// Returns OpenCode's own words for a refused request.
fn refusal(status: u16, body: &[u8]) -> String {
    let parsed: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let message = [
        &parsed["data"]["message"],
        &parsed["error"]["message"],
        &parsed["message"],
        &parsed["error"],
    ]
    .into_iter()
    .find_map(Value::as_str)
    .map(str::to_owned)
    .unwrap_or_else(|| String::from_utf8_lossy(body).chars().take(400).collect());
    format!("OpenCode answered {status}: {message}")
}

/// An answer that refused a request, which proves that the server did not act on it, unlike a
/// connection that broke.
#[derive(Debug)]
pub struct Refused(pub String);
impl std::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for Refused {}

// A connection that the server did not accept, so no request reached it.
#[derive(Debug)]
struct NotListening;
impl std::fmt::Display for NotListening {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OpenCode's server is not reachable")
    }
}
impl std::error::Error for NotListening {}

/// Sends requests to one server. It is cheap to copy into another thread.
#[derive(Clone)]
pub struct Client {
    port: u16,
    authorization: String,
    directory: String,
}

impl Client {
    // Opens a connection and writes one request.
    fn open(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        accept: &str,
    ) -> Result<TcpStream> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port))
            .map_err(|_| anyhow::Error::new(NotListening))?;
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: {}\r\nAccept: {accept}\r\nConnection: close\r\n",
            self.port, self.authorization
        );
        if let Some(body) = body {
            request.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        request.push_str("\r\n");
        stream.write_all(request.as_bytes())?;
        if let Some(body) = body {
            stream.write_all(body)?;
        }
        stream.flush()?;
        Ok(stream)
    }

    /// Sends a request and returns its JSON answer, `null` for an empty one. A status other
    /// than success is an error with OpenCode's message.
    pub fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        limit: Duration,
    ) -> Result<Value> {
        let body = body.map(serde_json::to_vec).transpose()?;
        let stream = self.open(method, path, body.as_deref(), "application/json")?;
        stream.set_read_timeout(Some(limit))?;
        let mut reader = BufReader::new(stream);
        let (status, headers) = read_head(&mut reader)?;
        let header = |name: &str| {
            headers
                .iter()
                .find(|(known, _)| known == name)
                .map(|(_, value)| value.as_str())
        };
        let content = if header("transfer-encoding").is_some_and(|value| value.contains("chunked"))
        {
            read_chunked(&mut reader)?
        } else if let Some(length) = header("content-length").and_then(|value| value.parse().ok()) {
            ensure!(length <= BODY_LIMIT, "OpenCode's answer exceeds 64 MiB");
            let mut content = vec![0; length];
            reader.read_exact(&mut content)?;
            content
        } else {
            let mut content = Vec::new();
            reader
                .take(BODY_LIMIT as u64 + 1)
                .read_to_end(&mut content)?;
            content
        };
        if !(200..300).contains(&status) {
            return Err(anyhow::Error::new(Refused(refusal(status, &content))));
        }
        if content.iter().all(u8::is_ascii_whitespace) {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&content).context("OpenCode sent an answer that is not JSON")
    }

    /// Returns `path` with the server's folder as its location, for a request that reads the
    /// configuration, the catalog or the requests of one folder.
    pub fn located(&self, path: &str) -> String {
        let separator = if path.contains('?') { '&' } else { '?' };
        format!(
            "{path}{separator}location%5Bdirectory%5D={}",
            self.directory
        )
    }

    /// Reads a resource.
    pub fn get(&self, path: &str) -> Result<Value> {
        self.request("GET", path, None, REQUEST_LIMIT)
    }

    /// Sends a JSON body to a resource.
    pub fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.request("POST", path, Some(body), REQUEST_LIMIT)
    }

    /// Follows the server's events on a thread of its own. The receiver ends with an error
    /// when the stream ends.
    pub fn events(&self) -> Result<Receiver<Result<Value, String>>> {
        let stream = self.open("GET", "/api/event", None, "text/event-stream")?;
        let control = stream.try_clone()?;
        let mut reader = BufReader::new(stream);
        let (status, headers) = read_head(&mut reader)?;
        ensure!(
            status == 200,
            "OpenCode refused its event stream with status {status}"
        );
        let chunked = headers
            .iter()
            .any(|(name, value)| name == "transfer-encoding" && value.contains("chunked"));
        let (sender, receiver) = mpsc::sync_channel(4096);
        std::thread::spawn(move || {
            let error = follow(reader, chunked, &sender).err().map_or_else(
                || "OpenCode ended its event stream".to_owned(),
                |e| format!("{e:#}"),
            );
            let _ = sender.send(Err(error));
            let _ = control.shutdown(Shutdown::Both);
        });
        Ok(receiver)
    }
}

// Reads events until the stream ends or nobody receives them any more.
fn follow(
    mut reader: BufReader<TcpStream>,
    chunked: bool,
    sender: &mpsc::SyncSender<Result<Value, String>>,
) -> Result<()> {
    let mut pending = Vec::new();
    let mut data = Vec::new();
    loop {
        let read = if chunked {
            let mut size = String::new();
            if reader.read_line(&mut size)? == 0 {
                return Ok(());
            }
            let size = usize::from_str_radix(size.trim().split(';').next().unwrap_or_default(), 16)
                .context("OpenCode sent a malformed chunk")?;
            if size == 0 {
                return Ok(());
            }
            let start = pending.len();
            pending.resize(start + size, 0);
            reader.read_exact(&mut pending[start..])?;
            let mut end = [0; 2];
            reader.read_exact(&mut end)?;
            size
        } else {
            let buffer = reader.fill_buf()?;
            let size = buffer.len();
            pending.extend_from_slice(buffer);
            reader.consume(size);
            size
        };
        if read == 0 {
            return Ok(());
        }
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
            let line = line.strip_suffix(b"\n").unwrap_or(&line);
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() {
                if !data.is_empty() {
                    let event = serde_json::from_slice(&data).map_err(|error| {
                        format!("OpenCode sent an event that is not JSON: {error}")
                    });
                    data.clear();
                    if sender.send(event).is_err() {
                        return Ok(());
                    }
                }
            } else if let Some(value) = line.strip_prefix(b"data:") {
                if !data.is_empty() {
                    data.push(b'\n');
                }
                data.extend_from_slice(value.strip_prefix(b" ").unwrap_or(value));
                ensure!(
                    data.len() <= BODY_LIMIT,
                    "OpenCode sent an event over 64 MiB"
                );
            }
        }
        ensure!(
            pending.len() <= BODY_LIMIT,
            "OpenCode sent a line over 64 MiB"
        );
    }
}

/// Runs a server as a child of this process and stops it when standard input closes.
///
/// `opencode serve` does not end when the program that started it ends. Kodem's host holds the
/// other end of the guard's standard input, so the server stops also when the host stops
/// without cleanup. `arguments` are the server's program and its arguments.
pub fn guard(arguments: &[String]) -> Result<i32> {
    let (program, rest) = arguments
        .split_first()
        .context("The guard needs the program of the server")?;
    let mut child = Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .spawn()
        .context("OpenCode did not start")?;
    let id = child.id();
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
        stop_guarded(id);
    });
    Ok(child.wait()?.code().unwrap_or(1))
}

// Stops a guarded server with the processes that it started.
fn stop_guarded(id: u32) {
    // The guard leads the process group of the server, so this also ends the guard.
    #[cfg(unix)]
    {
        let _ = id;
        unsafe {
            libc::kill(0, libc::SIGTERM);
        }
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("taskkill");
        platform::hide_console(&mut command);
        let _ = command
            .args(["/PID", &id.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// One `opencode serve` process under its guard. Dropping it stops the process and the
/// processes it started.
pub struct Server {
    child: Child,
    client: Client,
}

impl Server {
    /// Starts a server in `cwd` for `account`, with more environment for this process only.
    /// `guard` is the program that runs [`guard`] with its [`GUARD`] argument.
    pub fn start(
        guard: &Path,
        cwd: &Path,
        account: &Account,
        environment: &[(&str, String)],
    ) -> Result<Self> {
        let password = uuid::Uuid::new_v4().simple().to_string();
        let mut command = Command::new(guard);
        account.apply(&mut command);
        platform::detach(&mut command);
        command
            .current_dir(cwd)
            .arg(GUARD)
            .arg(executable()?)
            .args(["serve", "--hostname", "127.0.0.1", "--port", "0"])
            .env("OPENCODE_SERVER_PASSWORD", &password)
            .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
            // The guard stops the server when this end closes.
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in environment {
            command.env(name, value);
        }
        let mut child = command.spawn().context("OpenCode did not start")?;
        let stdout = child.stdout.take().context("Missing OpenCode output")?;
        let stderr = child.stderr.take().context("Missing OpenCode errors")?;
        let (sender, receiver) = mpsc::channel();
        // The server reports its address on standard output; anything else is drained.
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if let Some(address) = line.split(LISTENING).nth(1) {
                    let port = address
                        .trim()
                        .rsplit(':')
                        .next()
                        .and_then(|port| port.trim_end_matches('/').parse::<u16>().ok());
                    let _ = sender.send(port);
                }
                line.clear();
            }
        });
        let last_error = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let errors = last_error.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    *errors.lock().unwrap() = line.chars().take(400).collect();
                }
            }
        });
        let port = match receiver.recv_timeout(START_LIMIT) {
            Ok(Some(port)) => port,
            result => {
                let _ = child.kill();
                let _ = child.wait();
                let reason = last_error.lock().unwrap().clone();
                if result == Err(RecvTimeoutError::Timeout) {
                    bail!("OpenCode's server did not start within 45 seconds. {reason}");
                }
                bail!("OpenCode's server stopped before it listened. {reason}");
            }
        };
        let directory = cwd
            .to_str()
            .context("The folder of this task is not valid UTF-8, which OpenCode needs")?;
        Ok(Self {
            child,
            client: Client {
                port,
                authorization: format!("Basic {}", base64(format!("{USER}:{password}").as_bytes())),
                directory: encode(directory),
            },
        })
    }

    /// Returns the client of this server.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Reports whether the process still runs.
    pub fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Waits until the server answers, for at most `limit`, and returns what it reports about
    /// itself. A server that answers but not as OpenCode 2 does is an older version.
    pub fn ready(&self, limit: Duration) -> Result<Value> {
        let deadline = Instant::now() + limit;
        loop {
            match self.client.get("/api/info") {
                Ok(info) => {
                    let major = info["version"]
                        .as_str()
                        .and_then(|version| version.split('.').next())
                        .and_then(|major| major.parse::<u64>().ok());
                    ensure!(major.is_some_and(|major| major >= 2), UPDATE);
                    return Ok(info);
                }
                // An older server refuses the route or answers with its web page.
                Err(error)
                    if error.is::<Refused>()
                        || error.downcast_ref::<serde_json::Error>().is_some() =>
                {
                    bail!(UPDATE)
                }
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // The server starts language servers and MCP servers of its own, which stop with it.
        #[cfg(unix)]
        if let Ok(pid) = i32::try_from(self.child.id()) {
            // The process leads its own group, so the group has the same identity.
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
            }
        }
        #[cfg(windows)]
        {
            let mut command = Command::new("taskkill");
            platform::hide_console(&mut command);
            let _ = command
                .args(["/PID", &self.child.id().to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
