//! Bounded, user-scoped local transport. Blocking operations belong on background threads.
use crate::model::*;
use anyhow::{Context, Result, ensure};
use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub const PROTOCOL: u32 = 21;
pub const MAX_PACKET: u64 = 32 * 1024 * 1024;
/// Resolves the user's data directory, with an explicit override for isolated installations.
pub fn data_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("KODEM_DATA_DIR") {
        return Ok(path.into());
    }
    Ok(
        PathBuf::from(env::var_os("HOME").context("HOME is unavailable")?)
            .join("Library/Application Support/Kodem"),
    )
}
/// Returns the private local host endpoint.
pub fn socket(root: &Path) -> PathBuf {
    root.join("host-v1.sock")
}
/// Creates the private host directory and rejects an endpoint owned by another user.
pub fn prepare(root: &Path) -> Result<()> {
    fs::create_dir_all(root)?;
    let meta = fs::symlink_metadata(root)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "Kodem data path must be a directory"
    );
    ensure!(
        meta.uid() == unsafe { libc::geteuid() },
        "Kodem data directory belongs to another user"
    );
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
/// Reads one size-limited JSON frame, including its newline delimiter.
pub fn read_frame<T: serde::de::DeserializeOwned>(stream: &mut UnixStream) -> Result<T> {
    use std::io::Read;
    let mut line = String::new();
    BufReader::new(stream)
        .take(MAX_PACKET + 1)
        .read_line(&mut line)?;
    ensure!(
        !line.is_empty() && line.len() as u64 <= MAX_PACKET && line.ends_with('\n'),
        "Incomplete or oversized IPC frame"
    );
    Ok(serde_json::from_str(&line)?)
}
/// Writes and flushes one complete local request or response.
pub fn write_frame<T: serde::Serialize>(stream: &mut UnixStream, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    ensure!((bytes.len() as u64) < MAX_PACKET, "IPC payload too large");
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}
/// Sends a typed operation to the local host without retrying mutations.
pub fn request(root: &Path, request: &Request) -> Result<Response> {
    let mut stream = UnixStream::connect(socket(root)).context("Kodem host is unavailable")?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    write_frame(&mut stream, request)?;
    let reply = read_frame(&mut stream)?;
    if let Response::Error(error) = reply {
        anyhow::bail!(error)
    }
    Ok(reply)
}
/// Connects to the packaged protocol, upgrading an idle older host without retrying mutations.
pub fn connect_or_start(root: &Path) -> Result<()> {
    connect_with_host(root, &env::current_exe()?.with_file_name("kodem-host"))
}
fn connect_with_host(root: &Path, host: &Path) -> Result<()> {
    prepare(root)?;
    if let Ok(Response::Workspace(ws)) = request(root, &Request::Hello) {
        if ws.protocol == PROTOCOL {
            return Ok(());
        }
        ensure!(
            ws.protocol < PROTOCOL,
            "A newer Kodem host is running. Open the latest installed app."
        );
        ensure!(
            host.is_file(),
            "Missing packaged kodem-host executable. Rebuild the app bundle."
        );
        // The host makes the final idle decision under its state lock. Never kill it.
        match request(root, &Request::Shutdown) {
            Ok(Response::Ok) => {}
            Ok(_) => anyhow::bail!("The older host did not accept the upgrade. Kodem will retry."),
            Err(error) => anyhow::bail!(
                "Waiting to upgrade the local host: {error:#}. Kodem will retry automatically."
            ),
        }
        let until = Instant::now() + Duration::from_secs(10);
        while UnixStream::connect(socket(root)).is_ok() {
            ensure!(
                Instant::now() < until,
                "Waiting for the older host to exit. Kodem will retry automatically."
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    ensure!(
        host.is_file(),
        "Missing packaged kodem-host executable. Rebuild the app bundle."
    );
    let log = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(root.join("host.log"))?;
    use std::os::unix::process::CommandExt;
    let mut child = None;
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        if let Ok(Response::Workspace(ws)) = request(root, &Request::Hello) {
            ensure!(
                ws.protocol == PROTOCOL,
                "Another Kodem version started its host. Open the latest installed app."
            );
            return Ok(());
        }
        // Concurrent windows can race for the host lock. A losing starter exits cleanly;
        // retry only that startup, never a user's operation.
        if child
            .as_mut()
            .is_none_or(|child: &mut std::process::Child| {
                child.try_wait().is_ok_and(|status| status.is_some())
            })
        {
            child = Some(
                Command::new(host)
                    .env("KODEM_DATA_DIR", root)
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(log.try_clone()?))
                    .stderr(Stdio::from(log.try_clone()?))
                    .process_group(0)
                    .spawn()?,
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!(
        "Host did not start. See {}",
        root.join("host.log").display()
    )
}
