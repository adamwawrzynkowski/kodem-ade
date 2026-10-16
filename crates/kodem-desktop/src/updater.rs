//! GitHub channel discovery and verified updates, run outside the UI thread. macOS installs an
//! app bundle; Windows and Linux replace the folder that holds the executables. The
//! `kodem-update` helper of the downloaded version moves the folders after the desktop exits.
use anyhow::{Context, Result, bail, ensure};
use kodem_core::platform;
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
/// The archive format of this platform's release asset.
#[cfg(any(target_os = "macos", windows))]
const ARCHIVE: &str = "zip";
#[cfg(not(any(target_os = "macos", windows)))]
const ARCHIVE: &str = "tar.gz";

/// Names the release asset for this platform and processor, such as `Kodem-windows-x86_64.zip`.
fn asset_name() -> String {
    let system = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    };
    format!("Kodem-{system}-{}.{ARCHIVE}", std::env::consts::ARCH)
}
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
    /// Returns the name of the build type, as the title bar's badge shows it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Debug => "Debug",
            Self::Nightly => "Nightly",
            Self::Production => "Release",
        }
    }
    /// Returns the application name of the channel, as windows and installed files show it.
    pub fn app_name(self) -> &'static str {
        match self {
            Self::Debug => "Kodem Debug",
            Self::Nightly => "Kodem Nightly",
            Self::Production => "Kodem",
        }
    }
    /// Returns the name of the top folder in the channel's release archive: the app bundle on
    /// macOS, the install folder on Windows, and a lowercase folder on Linux.
    fn package_name(self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            Self::Debug => "Kodem Debug.app",
            #[cfg(target_os = "macos")]
            Self::Nightly => "Kodem Nightly.app",
            #[cfg(target_os = "macos")]
            Self::Production => "Kodem.app",
            #[cfg(windows)]
            channel => channel.app_name(),
            #[cfg(not(any(target_os = "macos", windows)))]
            Self::Debug => "kodem-debug",
            #[cfg(not(any(target_os = "macos", windows)))]
            Self::Nightly => "kodem-nightly",
            #[cfg(not(any(target_os = "macos", windows)))]
            Self::Production => "kodem",
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
        .find(|a| a.name == asset_name() && a.state == "uploaded")?;
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
        if let Ok(output) = platform::hide_console(&mut Command::new(gh))
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
    let mut child = platform::hide_console(&mut Command::new(curl_program()))
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
/// Returns the curl that the system ships: macOS and Windows 10 and later include one.
fn curl_program() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join("System32").join("curl.exe"))
            .filter(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from("curl.exe"))
    }
    #[cfg(not(windows))]
    {
        ["/usr/bin/curl", "/bin/curl"]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from("curl"))
    }
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
    let destination = installed_package(channel)?;
    let directory = tempfile::Builder::new()
        .prefix(".kodem-update-")
        .tempdir_in(
            destination
                .parent()
                .context("Missing application directory")?,
        )
        .context("Move Kodem to a writable folder before updating")?;
    platform::restrict(directory.path(), 0o700)?;
    let archive = directory.path().join(format!("update.{ARCHIVE}"));
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
    drop(file);
    let unpacked = directory.path().join("unpacked");
    extract(&archive, &unpacked, channel)?;
    let bundle = unpacked.join(channel.package_name());
    verify_package(&bundle, channel, &release.version.to_string())?;
    fs::remove_file(archive)?;
    Ok(PreparedUpdate {
        directory,
        destination,
        bundle,
    })
}

/// Checks that the archive holds only regular files and folders of the channel's package,
/// then extracts it.
#[cfg(target_os = "macos")]
fn extract(archive: &Path, unpacked: &Path, channel: Channel) -> Result<()> {
    // Our bundles contain regular files and directories only. Reject links and traversal before extraction.
    let listing = Command::new("/usr/bin/unzip")
        .args(["-Z", "-1"])
        .arg(archive)
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
            name.starts_with(&format!("{}/", channel.package_name()))
                || name.starts_with("__MACOSX/"),
            "Unexpected update archive contents"
        );
    }
    let listing = Command::new("/usr/bin/unzip")
        .args(["-Z", "-l"])
        .arg(archive)
        .output()?;
    ensure!(
        listing.status.success()
            && !String::from_utf8_lossy(&listing.stdout)
                .lines()
                .any(|line| line.starts_with('l')),
        "Update archive contains symbolic links"
    );
    run(Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(archive)
        .arg(unpacked))
}

/// Checks that the archive holds only regular files and folders of the channel's package,
/// then extracts it with the system's `tar`, which reads ZIP archives on Windows.
#[cfg(not(target_os = "macos"))]
fn extract(archive: &Path, unpacked: &Path, channel: Channel) -> Result<()> {
    let tar = tar_program()?;
    let listing = platform::hide_console(&mut Command::new(&tar))
        .arg("-tf")
        .arg(archive)
        .output()?;
    ensure!(
        listing.status.success(),
        "Update is not a readable {ARCHIVE} archive"
    );
    let root = format!("{}/", channel.package_name());
    for name in std::str::from_utf8(&listing.stdout)?.lines() {
        ensure!(
            !name.starts_with('/')
                && !name.contains(['\\', ':'])
                && !name.split('/').any(|part| part == ".."),
            "Unsafe update archive path"
        );
        ensure!(
            name.starts_with(&root),
            "Unexpected update archive contents"
        );
    }
    // A verbose listing marks symbolic and hard links like `ls -l` does.
    let listing = platform::hide_console(&mut Command::new(&tar))
        .arg("-tvf")
        .arg(archive)
        .output()?;
    ensure!(
        listing.status.success()
            && !String::from_utf8_lossy(&listing.stdout)
                .lines()
                .any(|line| line.starts_with(['l', 'h'])),
        "Update archive contains links"
    );
    fs::create_dir(unpacked)?;
    run(platform::hide_console(&mut Command::new(&tar))
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(unpacked))
}

/// Returns the system's own `tar`. On Windows that is bsdtar, which reads ZIP archives; a Git
/// installation can put a GNU tar that does not earlier on PATH.
#[cfg(not(target_os = "macos"))]
fn tar_program() -> Result<PathBuf> {
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

/// Resolves the installed app bundle that this process runs from.
#[cfg(target_os = "macos")]
fn installed_package(channel: Channel) -> Result<PathBuf> {
    let executable = std::env::current_exe()?
        .canonicalize()
        .map(kodem_core::platform::plain_path)?;
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
    Ok(destination)
}

/// Resolves the installed package folder that this process runs from.
#[cfg(not(target_os = "macos"))]
fn installed_package(channel: Channel) -> Result<PathBuf> {
    let executable = std::env::current_exe()?
        .canonicalize()
        .map(kodem_core::platform::plain_path)?;
    let destination = executable
        .parent()
        .context("Run the installed Kodem package to update")?
        .to_path_buf();
    ensure!(
        manifest(&destination).is_ok_and(|manifest| manifest.identifier == channel.bundle_id()),
        "Run the installed Kodem package to update. Its release.json must match this channel."
    );
    Ok(destination)
}

#[cfg(target_os = "macos")]
fn plist(bundle: &Path, key: &str) -> Result<String> {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print {key}")])
        .arg(bundle.join("Contents/Info.plist"))
        .output()?;
    ensure!(output.status.success(), "Update bundle is missing {key}");
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

/// Release metadata that the build commands write next to the Windows and Linux executables,
/// in place of a macOS bundle's Info.plist.
#[cfg(not(target_os = "macos"))]
#[derive(Deserialize)]
struct Manifest {
    identifier: String,
    version: String,
    channel: String,
    repository: String,
}

#[cfg(not(target_os = "macos"))]
fn manifest(package: &Path) -> Result<Manifest> {
    let bytes =
        fs::read(package.join("release.json")).context("Update package is missing release.json")?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(target_os = "macos")]
fn verify_package(bundle: &Path, channel: Channel, version: &str) -> Result<()> {
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
    for executable in ["kodem", "kodem-host", "kodem-update"] {
        ensure!(
            bundle.join("Contents/MacOS").join(executable).is_file(),
            "Update is missing {executable}"
        );
    }
    run(Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle))
}

#[cfg(not(target_os = "macos"))]
fn verify_package(package: &Path, channel: Channel, version: &str) -> Result<()> {
    let manifest = manifest(package)?;
    for (key, value, expected) in [
        ("identifier", &manifest.identifier, channel.bundle_id()),
        ("version", &manifest.version, version),
        ("channel", &manifest.channel, channel.name()),
        ("repository", &manifest.repository, REPOSITORY),
    ] {
        ensure!(value == expected, "Update package has an unexpected {key}");
    }
    for executable in ["kodem", "kodem-host", "kodem-update"] {
        let path = package.join(platform::executable_name(executable));
        ensure!(path.is_file(), "Update is missing {executable}");
        #[cfg(windows)]
        verify_signer(&path)?;
    }
    Ok(())
}

/// Requires an update to carry a valid Authenticode signature from the publisher that signed
/// the running app. An unsigned or locally signed app accepts the download on its GitHub
/// digest alone. The publisher is the certificate's subject, which stays the same when the
/// certificate is renewed.
#[cfg(windows)]
fn verify_signer(executable: &Path) -> Result<()> {
    let Some(expected) = signer(&std::env::current_exe()?)? else {
        return Ok(());
    };
    ensure!(
        signer(executable)?.as_deref() == Some(expected.as_str()),
        "The update is not signed by the publisher of the installed app"
    );
    Ok(())
}

/// Returns the subject of the certificate behind a valid Authenticode signature. A valid
/// signature chains to a certificate authority that Windows trusts.
#[cfg(windows)]
fn signer(executable: &Path) -> Result<Option<String>> {
    // The path travels in the environment, so it is never parsed as PowerShell code.
    let output = platform::hide_console(&mut Command::new("powershell.exe"))
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$s = Get-AuthenticodeSignature -LiteralPath $env:KODEM_SIGNED_FILE; \
             if ($s.Status -eq 'Valid') { $s.SignerCertificate.Subject }",
        ])
        .env("KODEM_SIGNED_FILE", executable)
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        output.status.success(),
        "Windows could not read the update's signature"
    );
    let subject = String::from_utf8(output.stdout)?.trim().to_owned();
    Ok((!subject.is_empty()).then_some(subject))
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

/// The file with which `kodem-update` marks a stage folder that it finished with. Windows keeps
/// the running helper and its log in that folder until the helper exits.
const FINISHED: &str = "finished";

/// Starts the `kodem-update` helper of the downloaded version from the stage folder and waits
/// until it is ready, before the desktop may exit. The helper waits for that exit by reading
/// its standard input, which stays open until this process ends.
///
/// The new version's helper runs, so a fixed helper installs its own release. Its contract
/// stays the same across versions: the arguments are the installed folder, the new package and
/// the stage, the end of standard input starts the installation, and `ready` in the stage
/// means the desktop may exit.
pub fn install(prepared: PreparedUpdate) -> Result<()> {
    ensure_unchanged(&prepared.destination)?;
    let stage = prepared.directory.path();
    let name = platform::executable_name("kodem-update");
    // A helper inside the installed folder or the new package would keep Windows from moving
    // that folder.
    let helper = stage.join(&name);
    fs::copy(package_helper(&prepared.bundle, &name), &helper)
        .context("The update has no update helper. Retry the download.")?;
    let log = File::create(stage.join("install.log"))?;
    // The helper must not start in the installed folder, where the app usually starts: Windows
    // refuses to move a folder that a process uses as its working directory.
    let neutral = prepared
        .destination
        .parent()
        .context("Missing application directory")?;
    let mut child = platform::detach(
        Command::new(&helper)
            .current_dir(neutral)
            .arg(&prepared.destination)
            .arg(&prepared.bundle)
            .arg(stage)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log)),
    )
    .spawn()?;
    let ready = stage.join("ready");
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        if ready.is_file() {
            // The pipe closes when this process exits, which is the helper's signal to start.
            std::mem::forget(child.stdin.take());
            // The helper now owns cleanup and rollback after the parent exits.
            let _ = prepared.directory.keep();
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            bail!("Update installer failed to prepare. Retry the download.");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    bail!("Update installer did not become ready. Retry the download.")
}

/// Returns the path of the update helper in a verified package.
fn package_helper(package: &Path, name: &str) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        package.join("Contents/MacOS").join(name)
    }
    #[cfg(not(target_os = "macos"))]
    {
        package.join(name)
    }
}

/// Removes the stage folders that `kodem-update` finished with beside the installed app. A
/// folder that is still in use stays until a later call.
pub fn remove_finished_updates() {
    let channel = Channel::current();
    if channel == Channel::Debug {
        return;
    }
    let Some(parent) = installed_package(channel)
        .ok()
        .and_then(|destination| destination.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".kodem-update-")
            && path.join(FINISHED).is_file()
        {
            let _ = remove_stage(&path);
        }
    }
}

/// Removes a stage folder, its mark last, so that a folder still in use keeps the mark.
fn remove_stage(stage: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(stage)? {
        let entry = entry?;
        if entry.file_name() == FINISHED {
            continue;
        }
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    fs::remove_dir_all(stage)
}

/// Refuses to replace an installed app that changed after the download started.
fn ensure_unchanged(destination: &Path) -> Result<()> {
    let channel = Channel::current();
    #[cfg(target_os = "macos")]
    let unchanged = plist(destination, "CFBundleIdentifier")? == channel.bundle_id()
        && plist(destination, "CFBundleShortVersionString")? == VERSION;
    #[cfg(not(target_os = "macos"))]
    let unchanged = manifest(destination).is_ok_and(|manifest| {
        manifest.identifier == channel.bundle_id() && manifest.version == VERSION
    });
    ensure!(
        unchanged,
        "The installed app changed after this download. Reopen it before updating."
    );
    Ok(())
}
