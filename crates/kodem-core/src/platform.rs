//! Operating-system differences shared by the desktop and the local host.
use anyhow::{Context, Result};
use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// Returns the desktop identity of this build's channel. Linux desktop entries, window classes
/// and notifications use it to find Kodem's name and icon.
pub fn app_id() -> &'static str {
    if cfg!(debug_assertions) {
        return "dev.kodem.ade.debug";
    }
    match option_env!("KODEM_RELEASE_CHANNEL") {
        Some("production") => "dev.kodem.ade",
        Some("nightly") => "dev.kodem.ade.nightly",
        _ => "dev.kodem.ade.debug",
    }
}

/// Returns the home directory of the signed-in user.
pub fn home_dir() -> Result<PathBuf> {
    std::env::home_dir()
        .filter(|home| !home.as_os_str().is_empty())
        .context("The home directory is unavailable")
}

/// Returns the folder for per-user application data: Application Support on macOS, the XDG
/// data folder on Linux, and Local AppData on Windows.
pub fn data_home() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Ok(home_dir()?.join("Library/Application Support"))
    }
    #[cfg(windows)]
    {
        match std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            Some(path) if path.is_absolute() => Ok(path),
            _ => Ok(home_dir()?.join("AppData").join("Local")),
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        match std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
            Some(path) if path.is_absolute() => Ok(path),
            _ => Ok(home_dir()?.join(".local/share")),
        }
    }
}

/// Removes the `\\?\` prefix that `fs::canonicalize` adds on Windows when the path has a
/// classic drive or network form. Git and process working directories do not accept the
/// prefixed form everywhere. Other platforms return the path unchanged.
pub fn plain_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        const CLASSIC_LIMIT: usize = 260;
        if let Some(text) = path.to_str() {
            if let Some(share) = text.strip_prefix(r"\\?\UNC\")
                && share.len() + 2 < CLASSIC_LIMIT
            {
                return PathBuf::from(format!(r"\\{share}"));
            }
            if let Some(drive) = text.strip_prefix(r"\\?\")
                && drive.as_bytes().get(1) == Some(&b':')
                && drive.len() < CLASSIC_LIMIT
            {
                return PathBuf::from(drive);
            }
        }
    }
    path
}

/// Converts a path that Git printed into an operating-system string. Git prints UTF-8 paths
/// on Windows, and the platform's own bytes elsewhere.
pub fn os_string(bytes: &[u8]) -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(bytes.to_vec())
    }
    #[cfg(not(unix))]
    {
        OsString::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Returns the bytes that Git uses for an operating-system string, with `/` separators.
pub fn os_bytes(value: &OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        value.as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        value.to_string_lossy().replace('\\', "/").into_bytes()
    }
}

/// Limits a file or folder to the given Unix mode. On Windows the access list that the item
/// inherits from the user's profile folder already keeps other accounts out.
pub fn restrict(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// Tells whether the current user owns a file. Windows has no Unix owner; its per-user
/// folders carry the restriction instead.
pub fn owned_by_current_user(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.uid() == unsafe { libc::geteuid() }
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        true
    }
}

/// Opens a file without waiting on a FIFO or device that has no writer.
pub fn open_nonblocking(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    options.open(path)
}

/// Creates a new file that only its owner can read and write. It fails if the file exists.
pub fn create_private(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Keeps a console program that a windowed process starts from opening a console window.
pub fn hide_console(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Starts a child in its own process group, so that it outlives the terminal and signals of
/// the process that started it. On Windows the child also gets no console window.
pub fn detach(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    command
}

/// Returns the user's interactive shell with the arguments that start it for a terminal.
pub fn interactive_shell() -> (OsString, &'static [&'static str]) {
    #[cfg(windows)]
    {
        ("powershell.exe".into(), &["-NoLogo"])
    }
    #[cfg(not(windows))]
    {
        (login_shell(std::env::var_os("SHELL")), &["-l"])
    }
}

/// Builds a command that runs one command line through the platform's shell, as a login
/// shell where the platform has one.
pub fn shell_command(line: &str) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", line]);
        command
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/bin/zsh");
        command.args(["-lc", line]);
        command
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let mut command = Command::new(login_shell(std::env::var_os("SHELL")));
        command.args(["-lc", line]);
        command
    }
}

/// Uses the configured absolute shell, or the platform's default shell.
#[cfg(not(windows))]
fn login_shell(configured: Option<OsString>) -> OsString {
    if let Some(shell) = configured.filter(|shell| Path::new(shell).is_absolute()) {
        return shell;
    }
    let fallbacks: &[&str] = if cfg!(target_os = "macos") {
        &["/bin/zsh"]
    } else {
        &["/bin/bash", "/bin/sh"]
    };
    fallbacks
        .iter()
        .find(|shell| Path::new(shell).is_file())
        .unwrap_or(&fallbacks[0])
        .into()
}

/// Returns the file name of an executable on this platform, such as `kodem-host.exe`.
pub fn executable_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// Returns the folders where command-line tools are installed but that a desktop session's
/// PATH can omit.
pub fn tool_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(home) = home_dir() {
        directories.push(home.join(".local").join("bin"));
    }
    #[cfg(target_os = "macos")]
    directories.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    #[cfg(windows)]
    if let Some(app_data) = std::env::var_os("APPDATA") {
        directories.push(PathBuf::from(app_data).join("npm"));
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    directories.extend(["/usr/local/bin", "/usr/bin"].map(PathBuf::from));
    directories
}

/// Lists the files that can run a program, from the given folders first and then from PATH.
/// Windows also accepts the `.cmd` launchers that package managers install.
pub fn program_candidates(name: &str, directories: &[PathBuf]) -> Vec<PathBuf> {
    let names: &[String] = &if cfg!(windows) {
        vec![executable_name(name), format!("{name}.cmd")]
    } else {
        vec![name.to_owned()]
    };
    let path = std::env::var_os("PATH");
    directories
        .iter()
        .cloned()
        .chain(path.iter().flat_map(std::env::split_paths))
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .collect()
}

/// Finds the first existing file that can run a program. See [`program_candidates`].
pub fn find_program(name: &str, directories: &[PathBuf]) -> Option<PathBuf> {
    program_candidates(name, directories)
        .into_iter()
        .find(|path| path.is_file())
}
