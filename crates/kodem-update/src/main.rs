//! Installs a downloaded Kodem update after the desktop exits, then opens the new version.
//!
//! `kodem-update DESTINATION PACKAGE STAGE`
//!
//! The desktop verifies the download, copies this program into the update's stage folder and
//! starts it from there, so that no file of the installed folder is in use while it moves. The
//! desktop keeps this program's standard input open until it exits: the end of that input tells
//! that the desktop is gone. The program then replaces DESTINATION with PACKAGE by renaming
//! folders, puts the previous folder back when a step fails, and opens whichever version is
//! installed in the end. Its output goes to `install.log` in STAGE.
#![cfg_attr(windows, windows_subsystem = "windows")]
use anyhow::{Context, Result, anyhow, bail, ensure};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// How long the desktop may take to save its state and exit.
const EXIT_LIMIT: Duration = Duration::from_secs(120);
/// How long a move may fail before the update gives up. Windows refuses to rename a folder while
/// a program in it runs or another process, such as an antivirus scan, holds one of its files.
const MOVE_LIMIT: Duration = Duration::from_secs(60);

/// Files that every package of this platform holds.
#[cfg(target_os = "macos")]
const PACKAGE_FILES: [&str; 3] = [
    "Contents/Info.plist",
    "Contents/MacOS/kodem",
    "Contents/MacOS/kodem-host",
];
#[cfg(windows)]
const PACKAGE_FILES: [&str; 3] = ["release.json", "kodem.exe", "kodem-host.exe"];
#[cfg(not(any(target_os = "macos", windows)))]
const PACKAGE_FILES: [&str; 3] = ["release.json", "kodem", "kodem-host"];

fn main() -> ExitCode {
    let arguments: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [destination, package, stage] = arguments.as_slice() else {
        eprintln!("Usage: kodem-update DESTINATION PACKAGE STAGE");
        return ExitCode::from(2);
    };
    match update(destination, package, stage) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Replaces the installed folder once the desktop has exited, and opens the result.
fn update(destination: &Path, package: &Path, stage: &Path) -> Result<()> {
    let backup = stage.join("previous");
    ensure!(
        destination.is_dir() && stage.is_dir() && !backup.exists(),
        "Invalid update arguments"
    );
    for file in PACKAGE_FILES {
        ensure!(package.join(file).is_file(), "The update is missing {file}");
    }
    // The desktop quits only after it sees this file.
    fs::File::create(stage.join("ready")).context("Could not write to the update folder")?;
    wait_for_desktop()?;
    #[cfg(windows)]
    windows::release(destination);
    if let Err(error) = rename(destination, &backup) {
        // Nothing changed, so the installed version opens again as it was.
        let _ = launch(destination);
        return Err(
            error.context("The installed app is still in use; the update was not installed")
        );
    }
    if let Err(error) = replace(package, destination) {
        eprintln!("Update failed: {error:#}");
        if destination.exists() {
            rename(destination, &stage.join("failed"))?;
        }
        rename(&backup, destination)?;
        launch(destination)?;
        bail!("Restored the previous app");
    }
    println!("Installed {}", destination.display());
    #[cfg(windows)]
    windows::adopt(&backup, destination);
    let _ = fs::remove_dir_all(&backup);
    finish(stage);
    Ok(())
}

/// Waits until the desktop exits, which closes the other end of standard input.
fn wait_for_desktop() -> Result<()> {
    let (done, exited) = mpsc::channel();
    thread::spawn(move || {
        // The desktop never writes, so the read ends only when its end of the pipe closes.
        let _ = io::copy(&mut io::stdin().lock(), &mut io::sink());
        let _ = done.send(());
    });
    exited
        .recv_timeout(EXIT_LIMIT)
        .map_err(|_| anyhow!("The desktop did not exit; the update was not installed"))
}

/// Moves the new package into the installed folder's place and opens it.
fn replace(package: &Path, destination: &Path) -> Result<()> {
    rename(package, destination)?;
    #[cfg(target_os = "macos")]
    run(Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(destination))?;
    launch(destination)
}

/// Renames a folder, which moves all of it or nothing. Windows refuses while the folder is in
/// use, so there it retries until `MOVE_LIMIT`.
fn rename(from: &Path, to: &Path) -> Result<()> {
    let until = Instant::now() + MOVE_LIMIT;
    let mut reported = false;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if cfg!(windows)
                    && error.kind() != io::ErrorKind::NotFound
                    && Instant::now() < until =>
            {
                if !reported {
                    eprintln!("Waiting to move {}: {error}", from.display());
                    reported = true;
                }
                thread::sleep(Duration::from_millis(250));
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Could not move {} to {}", from.display(), to.display())
                });
            }
        }
    }
}

/// Opens the app installed in a folder, outside this program's process group.
fn launch(installed: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        run(Command::new("/usr/bin/open")
            .arg("-n")
            .arg(installed)
            .stdin(Stdio::null()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let program = installed.join(PACKAGE_FILES[1]);
        let mut command = Command::new(&program);
        // The app works outside its own folder, so its working directory never keeps the next
        // update from moving that folder.
        command
            .current_dir(installed.parent().unwrap_or(installed))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        detach(&mut command);
        command
            .spawn()
            .with_context(|| format!("Could not open {}", program.display()))?;
        Ok(())
    }
}

/// Starts a child in its own process group, so that it outlives this program.
#[cfg(not(target_os = "macos"))]
fn detach(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
}

#[cfg(target_os = "macos")]
fn run(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    ensure!(status.success(), "{command:?} failed with {status}");
    Ok(())
}

/// Removes the stage folder. On Windows this program and its log stay in use until it exits, so
/// the folder keeps them with a mark that lets the next start of Kodem remove it.
fn finish(stage: &Path) {
    #[cfg(windows)]
    {
        let _ = fs::remove_dir(stage.join("unpacked"));
        // updater::FINISHED in the desktop names the same file.
        let _ = fs::File::create(stage.join("finished"));
    }
    #[cfg(not(windows))]
    {
        let _ = fs::remove_dir_all(stage);
    }
}

#[cfg(windows)]
mod windows {
    use anyhow::{Result, bail, ensure};
    use std::{
        fs,
        path::Path,
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegOpenKeyExW,
                RegSetValueExW,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
            },
        },
    };

    /// How long Kodem's own programs may take to exit by themselves.
    const EXIT_GRACE: Duration = Duration::from_secs(15);
    /// The uninstaller that `scripts/windows-installer.nsi` writes into the installed folder.
    const UNINSTALLER: &str = "Uninstall.exe";

    /// Waits until no Kodem program of the installed folder runs. After `EXIT_GRACE` it ends
    /// the host processes that remain: the desktop stopped the host before it exited, and the
    /// tool servers that providers start from `kodem-host.exe` hold no state.
    pub fn release(folder: &Path) {
        let until = Instant::now() + EXIT_GRACE;
        loop {
            let running = programs_in(folder);
            if running.is_empty() {
                return;
            }
            if Instant::now() >= until {
                for (process, name) in running {
                    if name.eq_ignore_ascii_case("kodem-host.exe") {
                        eprintln!("Ending {name} (process {process}), which did not exit");
                        terminate(process);
                    } else {
                        eprintln!(
                            "{name} (process {process}) still runs from the installed folder"
                        );
                    }
                }
                return;
            }
            thread::sleep(Duration::from_millis(250));
        }
    }

    /// Carries what the installer added into the new folder: the uninstaller, which release
    /// archives do not hold, and the version that Settings shows under Installed apps.
    pub fn adopt(previous: &Path, installed: &Path) {
        let uninstaller = previous.join(UNINSTALLER);
        if uninstaller.is_file()
            && !installed.join(UNINSTALLER).exists()
            && let Err(error) = fs::copy(&uninstaller, installed.join(UNINSTALLER))
        {
            eprintln!("Could not keep the uninstaller: {error}");
        }
        if let Err(error) = record_version(installed) {
            eprintln!("Could not record the version for Installed apps: {error:#}");
        }
    }

    /// Lists the Kodem programs that run from a folder, by process ID and file name.
    fn programs_in(folder: &Path) -> Vec<(u32, String)> {
        let prefix = format!("{}\\", folder.display()).to_lowercase();
        let mut found = Vec::new();
        // SAFETY: the snapshot handle is checked and closed, and the entry carries its size.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return found;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snapshot, &mut entry) != 0;
            while more {
                let name = text(&entry.szExeFile);
                if (name.eq_ignore_ascii_case("kodem.exe")
                    || name.eq_ignore_ascii_case("kodem-host.exe"))
                    && image(entry.th32ProcessID)
                        .is_some_and(|path| path.to_lowercase().starts_with(&prefix))
                {
                    found.push((entry.th32ProcessID, name));
                }
                more = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        found
    }

    /// Returns the full path of a process's program.
    fn image(process: u32) -> Option<String> {
        let mut buffer = vec![0u16; 32_768];
        let mut size = buffer.len() as u32;
        // SAFETY: the buffer holds `size` characters, and the handle is checked and closed.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process);
            if handle.is_null() {
                return None;
            }
            let read = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut size,
            ) != 0;
            CloseHandle(handle);
            read.then(|| String::from_utf16_lossy(&buffer[..size as usize]))
        }
    }

    fn terminate(process: u32) {
        // SAFETY: the handle is checked and closed.
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, process);
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }

    /// Writes the installed version into the installer's entry under Installed apps. An
    /// unpacked archive has no entry and is left alone.
    fn record_version(installed: &Path) -> Result<()> {
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(installed.join("release.json"))?)?;
        let (Some(identifier), Some(version)) = (
            manifest["identifier"].as_str(),
            manifest["version"].as_str(),
        ) else {
            bail!("release.json is incomplete");
        };
        let name = wide(&format!(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{identifier}"
        ));
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: the strings end with NUL, and the key is closed after use.
        unsafe {
            if RegOpenKeyExW(HKEY_CURRENT_USER, name.as_ptr(), 0, KEY_SET_VALUE, &mut key)
                != ERROR_SUCCESS
            {
                return Ok(());
            }
            let value = wide(version);
            let result = RegSetValueExW(
                key,
                wide("DisplayVersion").as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * size_of::<u16>()) as u32,
            );
            RegCloseKey(key);
            ensure!(result == ERROR_SUCCESS, "Windows error {result}");
        }
        Ok(())
    }

    /// Reads a NUL-terminated UTF-16 string.
    fn text(characters: &[u16]) -> String {
        let end = characters
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(characters.len());
        String::from_utf16_lossy(&characters[..end])
    }

    /// Encodes a string as NUL-terminated UTF-16.
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }
}
