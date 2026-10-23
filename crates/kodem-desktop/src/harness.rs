//! The installed and the newest release of each provider's CLI, and updates of a CLI that run
//! outside the UI thread and report their progress.
//!
//! Each CLI is compared with the channel that its own update follows: Claude Code's configured
//! release channel, Codex's latest GitHub release, which Kodem installs as a package,
//! OpenCode's update service, the Antigravity CLI's published changelog, and the release that
//! Grok Build's own update check names.
use anyhow::{Context, Result, bail, ensure};
use kodem_core::{
    Provider, git, platform,
    providers::{antigravity, claude, codex, grok, opencode},
};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// How long the update command of a CLI may run before Kodem stops it.
const UPDATE_LIMIT: Duration = Duration::from_secs(15 * 60);
/// The first OpenCode 1 release whose `upgrade --major` replaces it with OpenCode 2.
const OPENCODE_MAJOR_UPGRADE: Version = Version::new(1, 19, 0);
/// The Codex package that matches this computer, as named in Codex's GitHub releases.
#[cfg(target_os = "macos")]
const CODEX_SYSTEM: &str = "apple-darwin";
#[cfg(windows)]
const CODEX_SYSTEM: &str = "pc-windows-msvc";
#[cfg(not(any(target_os = "macos", windows)))]
const CODEX_SYSTEM: &str = "unknown-linux-musl";

/// What an update of a provider's CLI reports while it runs and when it ends.
pub enum Event {
    /// The update's current step, and the share of it done when the step can measure it.
    Progress {
        provider: Provider,
        text: String,
        fraction: Option<f32>,
    },
    /// The update ended with the release now installed, or with why it failed.
    Done {
        provider: Provider,
        result: Result<Version, String>,
    },
}

#[derive(Deserialize)]
struct Package {
    version: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

/// Reads the release of each installed provider CLI, the same CLI that Kodem runs. A CLI that
/// is missing or does not report a version is left out.
pub fn installed() -> Vec<(Provider, Version)> {
    Provider::ALL
        .into_iter()
        .filter_map(|provider| {
            installed_of(provider)
                .ok()
                .map(|version| (provider, version))
        })
        .collect()
}

/// Reads the newest stable release of each provider's CLI. A provider whose release cannot be
/// read is left out.
pub fn latest() -> Vec<(Provider, Version)> {
    Provider::ALL
        .into_iter()
        .filter_map(|provider| latest_of(provider).ok().map(|version| (provider, version)))
        .collect()
}

/// Starts the update of a provider's CLI on its own thread. It sends progress while it runs,
/// then `Event::Done` with the release that the CLI reports afterwards.
pub fn update(provider: Provider, latest: Version, events: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let progress = |text: String, fraction: Option<f32>| {
            let _ = events.send(Event::Progress {
                provider,
                text,
                fraction,
            });
        };
        let result = match provider {
            Provider::Claude => run_update(claude::executable(), &["update"], &progress),
            Provider::OpenCode => update_opencode(&latest, &progress),
            Provider::Codex => update_codex(&progress).map(|()| String::new()),
            Provider::Antigravity => antigravity::executable()
                .and_then(|executable| run_update(executable, &["update"], &progress)),
            Provider::Grok => grok::executable()
                .and_then(|executable| run_update(executable, &["update"], &progress)),
        }
        .and_then(|last| {
            progress("Checking the installed version…".into(), None);
            let installed = installed_of(provider)?;
            if installed < latest {
                bail!("{} is still {installed}. {last}", provider.label());
            }
            Ok(installed)
        })
        .map_err(|error| format!("{error:#}"));
        let _ = events.send(Event::Done { provider, result });
    });
}

/// Reads the release from the version text that a provider's CLI reports, such as
/// `codex-cli 0.47.0`, `2.1.300 (Claude Code)`, `opencode v2.0.32`, `1.4.3` (Antigravity) or
/// `1.0.62` (Grok Build).
fn parse_installed(reported: &str) -> Option<Version> {
    reported
        .split_whitespace()
        .find_map(|word| Version::parse(word.trim_start_matches('v')).ok())
}

// Runs a provider's CLI to read its release.
fn installed_of(provider: Provider) -> Result<Version> {
    let reported = match provider {
        Provider::Claude => claude::version()?,
        Provider::Codex => codex::version()?,
        Provider::OpenCode => opencode::version()?,
        Provider::Antigravity => antigravity::version()?,
        Provider::Grok => grok::version()?,
    };
    parse_installed(&reported)
        .with_context(|| format!("{} reported no release: {reported}", provider.label()))
}

// Reads the newest release in the channel that a provider's own update installs.
fn latest_of(provider: Provider) -> Result<Version> {
    let version = match provider {
        Provider::Claude => {
            let channel = claude_channel();
            String::from_utf8(fetch(&format!(
                "https://downloads.claude.ai/claude-code-releases/{channel}"
            ))?)?
            .trim()
            .to_owned()
        }
        Provider::Codex => {
            serde_json::from_slice::<Package>(&fetch(
                "https://registry.npmjs.org/@openai/codex/latest",
            )?)?
            .version
        }
        Provider::OpenCode => {
            serde_json::from_slice::<Package>(&fetch(
                "https://opencode.ai/update/api/latest/cli/npm",
            )?)?
            .version
        }
        // The changelog that `agy changelog` reads lists the newest release first, as
        // `## 1.4.4`.
        Provider::Antigravity => String::from_utf8(fetch(
            "https://raw.githubusercontent.com/google-antigravity/antigravity-cli/refs/heads/main/CHANGELOG.md",
        )?)?
        .lines()
        .find_map(|line| line.strip_prefix("## "))
        .map(|heading| heading.trim().to_owned())
        .context("The Antigravity CLI changelog names no release")?,
        Provider::Grok => grok_latest()?,
    };
    let version = Version::parse(&version)?;
    ensure!(
        version.pre.is_empty(),
        "The newest {} release is a prerelease",
        provider.label()
    );
    Ok(version)
}

// The newest release in the channel that `grok update` follows, as the CLI's own check reads it.
fn grok_latest() -> Result<String> {
    let mut command = Command::new(grok::executable()?);
    let output = platform::hide_console(&mut command)
        .args(["update", "--check", "--json"])
        .env("GROK_DISABLE_AUTOUPDATER", "1")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("Grok Build did not check for updates")?;
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("Grok Build reported its update check unreadably")?;
    if let Some(error) = report["error"].as_str() {
        bail!("Grok Build could not check for updates: {error}");
    }
    report["latestVersion"]
        .as_str()
        .map(str::to_owned)
        .context("Grok Build named no newest release")
}

// The release channel that `claude update` follows: `autoUpdatesChannel` of the user's Claude
// Code settings, `latest` by default.
fn claude_channel() -> &'static str {
    let channel = platform::home_dir()
        .ok()
        .and_then(|home| fs::read(home.join(".claude").join("settings.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|settings| settings["autoUpdatesChannel"].as_str().map(str::to_owned));
    if channel.as_deref() == Some("stable") {
        "stable"
    } else {
        "latest"
    }
}

/// Returns the system's curl with the options of every request: HTTPS only, bounded in time
/// and size, and without the user's curl configuration.
pub(crate) fn curl(url: &str, accept: &str, limit: Duration, max_bytes: u64) -> Command {
    let mut command = Command::new(crate::updater::curl_program());
    platform::hide_console(&mut command)
        .args([
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "15",
            "--user-agent",
            "Kodem-Updater",
        ])
        .arg("--max-time")
        .arg(limit.as_secs().to_string())
        .arg("--max-filesize")
        .arg(max_bytes.to_string())
        .arg("--header")
        .arg(format!("Accept: {accept}"))
        .arg(url)
        .stdin(Stdio::null());
    command
}

/// Reads a small document over HTTPS.
pub(crate) fn fetch(url: &str) -> Result<Vec<u8>> {
    let output = curl(
        url,
        "application/json, text/plain",
        Duration::from_secs(30),
        4 * 1024 * 1024,
    )
    .stderr(Stdio::null())
    .output()?;
    ensure!(output.status.success(), "Cannot read {url}");
    Ok(output.stdout)
}

// Runs a CLI's own update command with Kodem's tool PATH and reports each line it prints. It
// returns the last line, which explains a run that installed nothing.
fn run_update(
    executable: impl AsRef<Path>,
    arguments: &[&str],
    progress: &dyn Fn(String, Option<f32>),
) -> Result<String> {
    progress("Starting the update…".into(), None);
    let mut command = Command::new(executable.as_ref());
    platform::hide_console(&mut command)
        .args(arguments)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Ok(path) = git::search_path() {
        command.env("PATH", path);
    }
    let mut child = command.spawn().context("The update did not start")?;
    let (lines, received) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        forward_lines(stdout, lines.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        forward_lines(stderr, lines);
    }
    let deadline = Instant::now() + UPDATE_LIMIT;
    let mut last = String::new();
    loop {
        match received.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => {
                progress(line.clone(), None);
                last = line;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                stop(&mut child);
                bail!("The update did not finish in 15 minutes and was stopped.");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    let status = child.wait()?;
    ensure!(
        status.success(),
        "{}",
        if last.is_empty() {
            format!("The update failed with {status}.")
        } else {
            last
        }
    );
    Ok(last)
}

// Sends the readable lines of a stream as they arrive. A spinner that redraws its line with a
// carriage return counts as one line per redraw.
fn forward_lines(stream: impl Read + Send + 'static, lines: mpsc::Sender<String>) {
    thread::spawn(move || {
        for chunk in BufReader::new(stream).split(b'\n').map_while(Result::ok) {
            let chunk = String::from_utf8_lossy(&chunk).into_owned();
            for part in chunk.split('\r') {
                let line = plain(part);
                if !line.is_empty() && lines.send(line).is_err() {
                    return;
                }
            }
        }
    });
}

// Removes terminal escape sequences and the surrounding space from a line of output.
fn plain(line: &str) -> String {
    let mut text = String::with_capacity(line.len());
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            // A control sequence ends at its first letter; other escapes take one character.
            if characters.next() == Some('[') {
                for next in characters.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if !character.is_control() {
            text.push(character);
        }
    }
    text.trim().to_owned()
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

// Updates OpenCode with its own `upgrade` command. OpenCode 1 refuses to upgrade to an
// OpenCode 2 release by version and installs OpenCode 2 only with `upgrade --major`, which
// OpenCode 1.19.0 introduced, so an older OpenCode 1 first upgrades to the last OpenCode 1
// release.
fn update_opencode(latest: &Version, progress: &dyn Fn(String, Option<f32>)) -> Result<String> {
    let installed = installed_of(Provider::OpenCode)?;
    if installed.major != 1 || latest.major < 2 {
        return run_update(
            opencode::executable()?,
            &["upgrade", &latest.to_string()],
            progress,
        );
    }
    if installed < OPENCODE_MAJOR_UPGRADE {
        // `opencode-ai` is the package of OpenCode 1; OpenCode 2 is published as another one.
        let last = serde_json::from_slice::<Package>(&fetch(
            "https://registry.npmjs.org/opencode-ai/latest",
        )?)?
        .version;
        let output = run_update(opencode::executable()?, &["upgrade", &last], progress)?;
        progress("Checking the installed version…".into(), None);
        let installed = installed_of(Provider::OpenCode)?;
        if installed < OPENCODE_MAJOR_UPGRADE {
            bail!("OpenCode is still {installed}. {output}");
        }
    }
    run_update(opencode::executable()?, &["upgrade", "--major"], progress)
}

// Installs the latest Codex release from GitHub into Kodem's runtimes folder, where Kodem
// picks the newest package. The download is checked against the digest that GitHub reports.
fn update_codex(progress: &dyn Fn(String, Option<f32>)) -> Result<()> {
    progress("Reading the latest Codex release…".into(), None);
    let target = format!("{}-{CODEX_SYSTEM}", std::env::consts::ARCH);
    let name = format!("codex-package-{target}.tar.gz");
    let release: GithubRelease = serde_json::from_slice(&fetch(
        "https://api.github.com/repos/openai/codex/releases/latest",
    )?)?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == name)
        .with_context(|| format!("The latest Codex release has no {name}"))?;
    let digest = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
        .context("The latest Codex release has no SHA-256 digest")?
        .to_ascii_lowercase();
    let runtimes = codex::runtimes_dir()?;
    fs::create_dir_all(&runtimes)?;
    // The stage stays on the runtimes' filesystem, so the final move is a rename.
    let stage = tempfile::Builder::new()
        .prefix(".install.")
        .tempdir_in(&runtimes)?;
    let archive = stage.path().join("codex.tar.gz");
    download(&asset, &archive, &digest, progress)?;
    progress("Unpacking Codex…".into(), None);
    let unpacked = stage.path().join("package");
    fs::create_dir(&unpacked)?;
    let tar = tar_program()?;
    let output = platform::hide_console(&mut Command::new(tar))
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&unpacked)
        .stdin(Stdio::null())
        .output()?;
    ensure!(output.status.success(), "The Codex package did not unpack");
    let package = if unpacked.join("codex-package.json").is_file() {
        unpacked
    } else {
        unpacked.join(format!("codex-package-{target}"))
    };
    let manifest: Package = serde_json::from_slice(&fs::read(package.join("codex-package.json"))?)?;
    let version = Version::parse(&manifest.version)?;
    let destination = runtimes.join(version.to_string());
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::rename(&package, &destination)?;
    Ok(())
}

// Downloads a release asset to `path`, reporting the bytes received, and checks its digest.
fn download(
    asset: &GithubAsset,
    path: &Path,
    digest: &str,
    progress: &dyn Fn(String, Option<f32>),
) -> Result<()> {
    let total = asset.size.max(1);
    let mut child = curl(
        &asset.browser_download_url,
        "application/octet-stream",
        Duration::from_secs(30 * 60),
        1024 * 1024 * 1024,
    )
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()?;
    let mut stdout = child.stdout.take().context("The download has no output")?;
    let mut file = File::create(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut received = 0u64;
    let mut reported = Instant::now();
    loop {
        let read = match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                stop(&mut child);
                return Err(error.into());
            }
        };
        file.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        received += read as u64;
        if reported.elapsed() >= Duration::from_millis(100) {
            reported = Instant::now();
            progress(
                format!(
                    "Downloading Codex: {} of {} MB",
                    received / 1_000_000,
                    total / 1_000_000
                ),
                Some(received as f32 / total as f32),
            );
        }
    }
    ensure!(child.wait()?.success(), "The Codex download failed");
    file.flush()?;
    ensure!(
        received == asset.size,
        "The Codex download is incomplete: {received} of {} bytes",
        asset.size
    );
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ensure!(
        actual == digest,
        "The Codex download does not match its digest"
    );
    progress("Downloaded Codex".into(), Some(1.));
    Ok(())
}

/// Returns the system's own `tar`: bsdtar on macOS and Windows reads `.tar.gz` archives.
pub(crate) fn tar_program() -> Result<PathBuf> {
    #[cfg(windows)]
    let candidates = vec![
        PathBuf::from(std::env::var_os("SystemRoot").context("SystemRoot is unavailable")?)
            .join("System32")
            .join("tar.exe"),
    ];
    #[cfg(not(windows))]
    let candidates = vec![PathBuf::from("/usr/bin/tar"), PathBuf::from("/bin/tar")];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .context("The system tar program is missing")
}
