//! Bounded, user-scoped local transport. Blocking operations belong on background threads.
use crate::{model::*, platform};
use anyhow::{Context, Result, ensure};
#[cfg(unix)]
pub use std::os::unix::net::{UnixListener, UnixStream};
use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
// Windows 10 1803 and newer provide Unix domain sockets that the standard library does not wrap.
#[cfg(windows)]
pub use uds_windows::{UnixListener, UnixStream};
pub const PROTOCOL: u32 = 39;
pub const MAX_PACKET: u64 = 32 * 1024 * 1024;
/// Returns the data directories of every Kodem build on this computer: the release and nightly
/// builds share one, and Debug has its own.
pub fn install_dirs() -> Result<Vec<PathBuf>> {
    let home = platform::data_home()?;
    Ok(vec![home.join("Kodem"), home.join("Kodem Debug")])
}
/// Resolves the data directory, isolating Debug's host and store unless explicitly overridden.
pub fn data_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("KODEM_DATA_DIR") {
        return Ok(path.into());
    }
    let directory = if cfg!(debug_assertions) {
        "Kodem Debug"
    } else {
        "Kodem"
    };
    Ok(platform::data_home()?.join(directory))
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
        platform::owned_by_current_user(&meta),
        "Kodem data directory belongs to another user"
    );
    platform::restrict(root, 0o700)?;
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
/// Tells whether the connected process runs as the same user as this process.
#[cfg(any(target_os = "macos", target_os = "freebsd"))]
pub fn same_user(stream: &UnixStream) -> bool {
    use std::os::unix::io::AsRawFd;
    let (mut uid, mut gid) = (0, 0);
    let read = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    read == 0 && uid == unsafe { libc::geteuid() }
}
/// Tells whether the connected process runs as the same user as this process.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn same_user(stream: &UnixStream) -> bool {
    use std::os::unix::io::AsRawFd;
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let read = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut credentials).cast(),
            &mut length,
        )
    };
    read == 0 && credentials.uid == unsafe { libc::geteuid() }
}
/// Tells whether the connected process runs as the same user as this process.
///
/// Windows reports no peer account for a Unix socket. The socket lives in the user's Local
/// AppData folder, whose access list admits only that user.
#[cfg(windows)]
pub fn same_user(_stream: &UnixStream) -> bool {
    true
}
/// The error of a request that found no host listening, so the host never received it.
#[derive(Debug)]
pub struct Unreachable;
impl std::fmt::Display for Unreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Kodem host is unavailable")
    }
}
/// Reports whether `error` came from a request that never reached the host.
pub fn unreachable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Unreachable>().is_some()
}
/// Sends a typed operation to the local host without retrying mutations.
pub fn request(root: &Path, request: &Request) -> Result<Response> {
    let mut stream = UnixStream::connect(socket(root)).context(Unreachable)?;
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
    connect_with_host(
        root,
        &env::current_exe()?.with_file_name(platform::executable_name("kodem-host")),
    )
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
                platform::detach(
                    Command::new(host)
                        .env("KODEM_DATA_DIR", root)
                        .stdin(Stdio::null())
                        .stdout(Stdio::from(log.try_clone()?))
                        .stderr(Stdio::from(log.try_clone()?)),
                )
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
