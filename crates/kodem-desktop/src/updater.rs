//! GitHub channel discovery and verified macOS updates, run outside the UI thread.
use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const REPOSITORY: &str = "adamwawrzynkowski/kodem-ade";
const ASSET_NAME: &str = "Kodem-macos-aarch64.zip";
pub const VERSION: &str = env!("KODEM_APP_VERSION");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Debug,
    Nightly,
    Production,
}
impl Channel {
    /// Returns the channel embedded by the build command; development builds stay local.
    pub fn current() -> Self {
        if cfg!(debug_assertions) {
            return Self::Debug;
        }
        match env!("KODEM_RELEASE_CHANNEL") {
            "nightly" => Self::Nightly,
            "production" => Self::Production,
            _ => Self::Debug,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Nightly => "nightly",
            Self::Production => "production",
        }
    }
    fn bundle_name(self) -> &'static str {
        match self {
            Self::Debug => "Kodem Debug.app",
            Self::Nightly => "Kodem Nightly.app",
            Self::Production => "Kodem.app",
        }
    }
    fn bundle_id(self) -> &'static str {
        match self {
            Self::Debug => "dev.kodem.ade.debug",
            Self::Nightly => "dev.kodem.ade.nightly",
            Self::Production => "dev.kodem.ade",
        }
    }
}

/// Formats the compact titlebar badge from the installed build metadata.
pub fn version_label() -> String {
    if Channel::current() == Channel::Nightly {
        format!("Nightly {VERSION}")
    } else {
        VERSION.into()
    }
}

pub enum State {
    Idle,
    Checking,
    Available(Release),
    Downloading { received: u64, total: u64 },
    Ready(PreparedUpdate),
    Installing,
    Failed(String),
}
#[derive(Clone)]
pub struct Release {
    pub version: Version,
    asset_id: u64,
    size: u64,
    digest: String,
}
pub struct PreparedUpdate {
    directory: tempfile::TempDir,
    destination: PathBuf,
    bundle: PathBuf,
}
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}
#[derive(Deserialize)]
struct GithubAsset {
    id: u64,
    name: String,
    size: u64,
    digest: Option<String>,
    state: String,
}

/// Checks only the installed release channel, ignoring drafts and incomplete uploads.
pub fn check() -> Result<Option<Release>> {
    let channel = Channel::current();
    if channel == Channel::Debug {
        return Ok(None);
    }
    let current = Version::parse(VERSION)?;
    let token = github_token();
    let mut newest = None;
    // Bound each request and the total history scan. Stable discovery is independent of nightly volume.
    if channel == Channel::Production {
        let bytes = api("releases/latest", token.as_deref())?;
        return Ok(select_release(
            serde_json::from_slice(&bytes)?,
            channel,
            &current,
        ));
    }
    for page in 1..=20 {
        let bytes = api(
            &format!("releases?per_page=100&page={page}"),
            token.as_deref(),
        )?;
        let releases: Vec<GithubRelease> = serde_json::from_slice(&bytes)?;
        let last = releases.len() < 100;
        for release in releases {
            if let Some(release) = select_release(release, channel, &current)
                && newest
                    .as_ref()
                    .is_none_or(|old: &Release| release.version > old.version)
            {
                newest = Some(release);
            }
        }
        if last {
            break;
        }
    }
    Ok(newest)
}
fn select_release(release: GithubRelease, channel: Channel, current: &Version) -> Option<Release> {
    if release.draft
        || channel == Channel::Debug
        || release.prerelease != (channel == Channel::Nightly)
    {
        return None;
    }
    let prefix = if channel == Channel::Nightly {
        "nightly/v"
    } else {
        "v"
    };
    let version = Version::parse(release.tag_name.strip_prefix(prefix)?).ok()?;
    if version <= *current || !version.pre.is_empty() || !version.build.is_empty() {
        return None;
    }
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == ASSET_NAME && a.state == "uploaded")?;
    let digest = asset.digest?.strip_prefix("sha256:")?.to_owned();
    if digest.len() != 64
        || !digest.bytes().all(|b| b.is_ascii_hexdigit())
        || asset.size == 0
        || asset.size > 1024 * 1024 * 1024
    {
        return None;
    }
    Some(Release {
        version,
        asset_id: asset.id,
        size: asset.size,
        digest,
    })
}
fn github_token() -> Option<String> {
    for key in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(token) = std::env::var(key)
            && valid_token(&token)
        {
            return Some(token);
        }
    }
    // Finder does not inherit a shell's Homebrew PATH.
    for gh in ["gh", "/opt/homebrew/bin/gh", "/usr/local/bin/gh"] {
        if let Ok(output) = Command::new(gh)
            .args(["auth", "token", "--hostname", "github.com"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            && output.status.success()
            && let Ok(token) = String::from_utf8(output.stdout)
        {
            let token = token.trim().to_owned();
            if valid_token(&token) {
                return Some(token);
            }
        }
    }
    None
}
fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn curl(endpoint: &str, token: Option<&str>, binary: bool) -> Result<std::process::Child> {
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "--disable",
            "--config",
            "-",
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
            "--max-time",
            if binary { "1800" } else { "30" },
            "--max-filesize",
            if binary { "1073741824" } else { "4194304" },
            "--user-agent",
            "Kodem-Updater",
            "--header",
            "X-GitHub-Api-Version: 2022-11-28",
            "--header",
            if binary {
                "Accept: application/octet-stream"
            } else {
                "Accept: application/vnd.github+json"
            },
        ])
        .arg(format!(
            "https://api.github.com/repos/{REPOSITORY}/{endpoint}"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    // Keep credentials out of process arguments, logs, app bundles and disk.
    if let Some(mut input) = child.stdin.take()
        && let Some(token) = token
        && let Err(error) = writeln!(input, "header = \"Authorization: Bearer {token}\"")
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.into());
    }
    Ok(child)
}
fn api(endpoint: &str, token: Option<&str>) -> Result<Vec<u8>> {
    let output = curl(endpoint, token, false)?.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Cannot read GitHub releases. Check your connection and run gh auth login with access to {REPOSITORY}. A stable release may not exist yet."
    );
    Ok(output.stdout)
}

/// Downloads and verifies an update, retaining a private stage beside the installed app.
pub fn download(release: &Release, progress: &mpsc::Sender<State>) -> Result<PreparedUpdate> {
    let channel = Channel::current();
    ensure!(
        channel != Channel::Debug,
        "Debug builds cannot install updates"
    );
    let executable = std::env::current_exe()?.canonicalize()?;
    let destination = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("Run the installed macOS app to update")?
        .to_path_buf();
    ensure!(
        destination.extension().is_some_and(|e| e == "app"),
        "Run the installed macOS app to update"
    );
    ensure!(
        plist(&destination, "CFBundleIdentifier")? == channel.bundle_id(),
        "Installed app identity does not match this update channel"
    );
    let directory = tempfile::Builder::new()
        .prefix(".kodem-update-")
        .tempdir_in(
            destination
                .parent()
                .context("Missing application directory")?,
        )
        .context("Move Kodem to a writable Applications folder before updating")?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
    let archive = directory.path().join("update.zip");
    let mut file = File::create(&archive)?;
    let token = github_token();
    let mut child = curl(
        &format!("releases/assets/{}", release.asset_id),
        token.as_deref(),
        true,
    )?;
    let result = (|| -> Result<()> {
        let mut output = child.stdout.take().context("Download stream unavailable")?;
        let mut hash = Sha256::new();
        let mut received = 0;
        let mut last = Instant::now();
        let mut buffer = [0u8; 128 * 1024];
        loop {
            let count = output.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            received += count as u64;
            ensure!(
                received <= release.size,
                "Downloaded archive exceeds its published size"
            );
            file.write_all(&buffer[..count])?;
            hash.update(&buffer[..count]);
            if last.elapsed() >= Duration::from_millis(100) {
                let _ = progress.send(State::Downloading {
                    received,
                    total: release.size,
                });
                last = Instant::now();
            }
        }
        ensure!(
            child.wait()?.success(),
            "Download failed. Check your network and GitHub access, then retry."
        );
        ensure!(
            received == release.size
                && format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&release.digest),
            "Update checksum or size does not match GitHub. Retry the download."
        );
        file.sync_all()?;
        let _ = progress.send(State::Downloading {
            received,
            total: release.size,
        });
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result?;
    // Our bundles contain regular files and directories only. Reject links and traversal before extraction.
    let listing = Command::new("/usr/bin/unzip")
        .args(["-Z", "-1"])
        .arg(&archive)
        .output()?;
    ensure!(
        listing.status.success(),
        "Update is not a readable ZIP archive"
    );
    for name in std::str::from_utf8(&listing.stdout)?.lines() {
        ensure!(
            !name.starts_with('/') && !name.split('/').any(|part| part == ".."),
            "Unsafe update archive path"
        );
        ensure!(
            name.starts_with(&format!("{}/", channel.bundle_name()))
                || name.starts_with("__MACOSX/"),
            "Unexpected update archive contents"
        );
    }
    let listing = Command::new("/usr/bin/unzip")
        .args(["-Z", "-l"])
        .arg(&archive)
        .output()?;
    ensure!(
        listing.status.success()
            && !String::from_utf8_lossy(&listing.stdout)
                .lines()
                .any(|line| line.starts_with('l')),
        "Update archive contains symbolic links"
    );
    let unpacked = directory.path().join("unpacked");
    run(Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(&archive)
        .arg(&unpacked))?;
    let bundle = unpacked.join(channel.bundle_name());
    verify_bundle(&bundle, channel, &release.version.to_string())?;
    fs::remove_file(archive)?;
    Ok(PreparedUpdate {
        directory,
        destination,
        bundle,
    })
}
fn plist(bundle: &Path, key: &str) -> Result<String> {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print {key}")])
        .arg(bundle.join("Contents/Info.plist"))
        .output()?;
    ensure!(output.status.success(), "Update bundle is missing {key}");
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn verify_bundle(bundle: &Path, channel: Channel, version: &str) -> Result<()> {
    for (key, expected) in [
        ("CFBundleIdentifier", channel.bundle_id()),
        ("CFBundleShortVersionString", version),
        ("KodemReleaseChannel", channel.name()),
        ("KodemReleaseRepository", REPOSITORY),
    ] {
        ensure!(
            plist(bundle, key)? == expected,
            "Update bundle has an unexpected {key}"
        );
    }
    for executable in ["kodem", "kodem-host"] {
        ensure!(
            bundle.join("Contents/MacOS").join(executable).is_file(),
            "Update is missing {executable}"
        );
    }
    run(Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle))
}
fn run(command: &mut Command) -> Result<()> {
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "Update verification failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

/// Starts the detached installer and waits for its readiness before allowing desktop exit.
pub fn install(prepared: PreparedUpdate) -> Result<()> {
    ensure!(
        plist(&prepared.destination, "CFBundleIdentifier")? == Channel::current().bundle_id()
            && plist(&prepared.destination, "CFBundleShortVersionString")? == VERSION,
        "The installed app changed after this download. Reopen it before updating."
    );
    let script = prepared.directory.path().join("install.sh");
    fs::write(&script, include_str!("../../../scripts/install-update.sh"))?;
    let log = File::create(prepared.directory.path().join("install.log"))?;
    use std::os::unix::process::CommandExt;
    let mut helper = Command::new("/bin/bash")
        .arg(&script)
        .arg(std::process::id().to_string())
        .arg(&prepared.destination)
        .arg(&prepared.bundle)
        .arg(prepared.directory.path())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .process_group(0)
        .spawn()?;
    let ready = prepared.directory.path().join("ready");
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        if ready.is_file() {
            // The helper now owns cleanup and rollback after the parent exits.
            let _ = prepared.directory.keep();
            return Ok(());
        }
        if helper.try_wait()?.is_some() {
            bail!("Update installer failed to prepare. Retry the download.");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = helper.kill();
    let _ = helper.wait();
    bail!("Update installer did not become ready. Retry the download.")
}
