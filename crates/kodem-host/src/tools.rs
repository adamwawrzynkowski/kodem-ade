//! Support tools for the setup dialog: which are installed, and installers that run in a
//! terminal window the user watches, so that password prompts and progress stay visible.
//!
//! Each installer uses the tool's official source: Claude Code's install script, Codex's release
//! package from GitHub, OpenCode's install script (its npm package on Windows), the install
//! scripts of the Antigravity CLI and Grok Build, Apple's Command Line Tools or Homebrew on
//! macOS, winget on Windows, and the distribution's package manager on Linux.
use crate::accounts;
use anyhow::Result;
use kodem_core::{Tool, ToolVersion, git, platform, providers::codex};
use std::{ffi::OsStr, fs, path::Path, process::Command, time::Duration};

/// How long a version query may take before the tool counts as missing.
const VERSION_WAIT: Duration = Duration::from_secs(10);

/// The file type that the platform's terminal runs.
#[cfg(target_os = "macos")]
const EXTENSION: &str = "command";
#[cfg(windows)]
const EXTENSION: &str = "ps1";
#[cfg(not(any(target_os = "macos", windows)))]
const EXTENSION: &str = "sh";

/// The Codex package that matches this computer, as named in Codex's GitHub releases.
#[cfg(target_os = "macos")]
const CODEX_SYSTEM: &str = "apple-darwin";
#[cfg(windows)]
const CODEX_SYSTEM: &str = "pc-windows-msvc";
#[cfg(not(any(target_os = "macos", windows)))]
const CODEX_SYSTEM: &str = "unknown-linux-musl";

/// Reads the version of each support tool with the PATH that Kodem gives Git.
pub(super) fn inspect() -> Vec<ToolVersion> {
    let path = git::search_path().ok();
    Tool::SUPPORT
        .into_iter()
        .map(|tool| ToolVersion {
            tool,
            version: version(tool, path.as_deref()),
        })
        .collect()
}

// Runs a tool's version query and returns its first line. Provider CLIs report through their
// runtimes instead, so they have none here.
fn version(tool: Tool, path: Option<&OsStr>) -> Option<String> {
    let (program, arguments): (&str, &[&str]) = match tool {
        Tool::Git => ("git", &["--version"]),
        Tool::GitLfs => ("git", &["lfs", "version"]),
        Tool::GitHub => ("gh", &["--version"]),
        Tool::Claude | Tool::Codex | Tool::OpenCode | Tool::Antigravity | Tool::Grok => {
            return None;
        }
    };
    if program == "git" && !apple_git_ready(path) {
        return None;
    }
    let mut command = Command::new(program);
    command.args(arguments);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let output = accounts::output(command, VERSION_WAIT).ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
}

// macOS ships `/usr/bin/git` as a stub that opens Apple's installer when it runs without the
// Command Line Tools. Reading the version must not open that installer.
#[cfg(target_os = "macos")]
fn apple_git_ready(path: Option<&OsStr>) -> bool {
    let first = path
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|directory| directory.join("git"))
        .find(|candidate| candidate.is_file());
    if first.as_deref() != Some(Path::new("/usr/bin/git")) {
        return first.is_some();
    }
    Command::new("/usr/bin/xcode-select")
        .arg("-p")
        .output()
        .is_ok_and(|output| output.status.success())
}

// Only macOS replaces a missing Git with an installer stub.
#[cfg(not(target_os = "macos"))]
fn apple_git_ready(_path: Option<&OsStr>) -> bool {
    true
}

/// Writes the installer of a tool to Kodem's data directory and runs it in a new terminal
/// window. The result shows in that window; the setup dialog checks the tool again later.
pub(super) fn install(root: &Path, tool: Tool) -> Result<()> {
    let directory = root.join("installers");
    fs::create_dir_all(&directory)?;
    platform::restrict(&directory, 0o700)?;
    let name = format!("{tool:?}").to_lowercase();
    let path = directory.join(format!("install-{name}.{EXTENSION}"));
    fs::write(&path, script(tool)?)?;
    platform::restrict(&path, 0o700)?;
    accounts::open_terminal(&path, &path.display().to_string())
}

// The text that an installer prints after its last step succeeded.
fn finished(tool: Tool) -> String {
    if cfg!(target_os = "macos") && tool == Tool::Git {
        return "Apple's installer opened. Finish it, then return to Kodem and choose Check again."
            .into();
    }
    format!(
        "{} is installed. Return to Kodem and choose Check again.",
        tool.label()
    )
}

/// Builds a POSIX shell installer that runs its steps in order and stops at the first that
/// fails. Terminals on Linux close with the script, so it waits for Return there.
#[cfg(unix)]
fn script(tool: Tool) -> Result<String> {
    use accounts::quote;
    let label = tool.label();
    let mut script = format!(
        "#!/bin/sh\n# Generated by Kodem. Installs {label} for this user.\n\
         PATH=\"$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH\"\nexport PATH\n\
         echo {}\necho\n",
        quote(&format!("Installing {label}…"))
    );
    script.push_str(&format!(
        "if {}\nthen\n    echo\n    echo {}\nelse\n    echo\n    echo {}\nfi\n",
        steps(tool)?.join(" &&\n    "),
        quote(&finished(tool)),
        quote(&format!(
            "{label} was not installed. Read the messages above, then try again."
        ))
    ));
    if !cfg!(target_os = "macos") {
        script.push_str("printf 'Press Return to close this window. '\nread _\n");
    }
    Ok(script)
}

// The shell commands that install a tool on macOS or Linux.
#[cfg(unix)]
fn steps(tool: Tool) -> Result<Vec<String>> {
    use accounts::quote;
    Ok(match tool {
        Tool::Claude => vec![
            "script=$(mktemp)".into(),
            "curl -fsSL https://claude.ai/install.sh -o \"$script\"".into(),
            "bash \"$script\"".into(),
            "rm -f \"$script\"".into(),
        ],
        // The script installs into ~/.opencode/bin, where Kodem looks for the CLI.
        Tool::OpenCode => vec![
            "script=$(mktemp)".into(),
            "curl -fsSL https://opencode.ai/install -o \"$script\"".into(),
            "bash \"$script\"".into(),
            "rm -f \"$script\"".into(),
        ],
        // The script installs `agy` into ~/.local/bin, where Kodem looks for the CLI.
        Tool::Antigravity => vec![
            "script=$(mktemp)".into(),
            "curl -fsSL https://antigravity.google/cli/install.sh -o \"$script\"".into(),
            "bash \"$script\"".into(),
            "rm -f \"$script\"".into(),
        ],
        // The script installs `grok` into ~/.grok/bin, where Kodem looks for the CLI.
        Tool::Grok => vec![
            "script=$(mktemp)".into(),
            "curl -fsSL https://x.ai/cli/install.sh -o \"$script\"".into(),
            "bash \"$script\"".into(),
            "rm -f \"$script\"".into(),
        ],
        Tool::Codex => {
            let target = format!("{}-{CODEX_SYSTEM}", std::env::consts::ARCH);
            vec![
                format!("runtimes={}", quote(&codex::runtimes_dir()?.to_string_lossy())),
                "mkdir -p \"$runtimes\"".into(),
                // The download stays on the runtimes' filesystem, so the final move is a rename.
                "stage=$(mktemp -d \"$runtimes/.install.XXXXXX\")".into(),
                format!(
                    "curl -fL --proto '=https' --tlsv1.2 https://github.com/openai/codex/releases/latest/download/codex-package-{target}.tar.gz -o \"$stage/codex.tar.gz\""
                ),
                "mkdir \"$stage/package\"".into(),
                "tar -xzf \"$stage/codex.tar.gz\" -C \"$stage/package\"".into(),
                format!(
                    "{{ package=\"$stage/package\"; [ -f \"$package/codex-package.json\" ] || package=\"$stage/package/codex-package-{target}\"; }}"
                ),
                "version=$(sed -n 's/^[[:space:]]*\"version\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p' \"$package/codex-package.json\" | head -n 1)".into(),
                "[ -n \"$version\" ]".into(),
                "rm -rf \"${runtimes:?}/$version\"".into(),
                "mv \"$package\" \"$runtimes/$version\"".into(),
                "rm -rf \"$stage\"".into(),
                "echo \"Installed Codex $version in $runtimes/$version\"".into(),
            ]
        }
        #[cfg(target_os = "macos")]
        Tool::Git => vec!["xcode-select --install".into()],
        #[cfg(target_os = "macos")]
        Tool::GitLfs => homebrew("git-lfs", &["git lfs install"]),
        #[cfg(target_os = "macos")]
        Tool::GitHub => homebrew("gh", &[GITHUB_SIGN_IN]),
        #[cfg(not(target_os = "macos"))]
        Tool::Git => system_packages("git", "git")?,
        #[cfg(not(target_os = "macos"))]
        Tool::GitLfs => {
            let mut steps = system_packages("git-lfs", "git-lfs")?;
            steps.push("git lfs install".into());
            steps
        }
        #[cfg(not(target_os = "macos"))]
        Tool::GitHub => {
            let mut steps = system_packages("gh", "github-cli")?;
            steps.push(GITHUB_SIGN_IN.into());
            steps
        }
    })
}

/// Signs GitHub CLI in unless it already has an account. Reviews of pull requests need it.
#[cfg(unix)]
const GITHUB_SIGN_IN: &str = "{ gh auth status >/dev/null 2>&1 || gh auth login; }";

// Installs a Homebrew formula, then runs the steps that set it up.
#[cfg(target_os = "macos")]
fn homebrew(formula: &str, after: &[&str]) -> Vec<String> {
    let mut steps = vec![
        "{ command -v brew >/dev/null || { echo 'Install Homebrew from https://brew.sh first.'; false; }; }".into(),
        format!("brew install {formula}"),
    ];
    steps.extend(after.iter().map(|step| (*step).to_owned()));
    steps
}

// Installs a package with the distribution's package manager. Arch Linux and Alpine name some
// packages differently, so `renamed` is the name there.
#[cfg(not(any(target_os = "macos", windows)))]
fn system_packages(package: &str, renamed: &str) -> Result<Vec<String>> {
    use anyhow::Context;
    let manager = ["apt-get", "dnf", "pacman", "zypper", "apk"]
        .into_iter()
        .find(|name| platform::find_program(name, &[]).is_some())
        .context("Kodem found no supported package manager. Install it with your distribution's package manager.")?;
    Ok(match manager {
        "apt-get" => vec![
            "sudo apt-get update".into(),
            format!("sudo apt-get install -y {package}"),
        ],
        "dnf" => vec![format!("sudo dnf install -y {package}")],
        "pacman" => vec![format!("sudo pacman -S --needed --noconfirm {renamed}")],
        "zypper" => vec![format!("sudo zypper --non-interactive install {package}")],
        _ => vec![format!("sudo apk add {renamed}")],
    })
}

/// Builds a PowerShell installer that stops at the first failing step and waits for Enter.
#[cfg(windows)]
fn script(tool: Tool) -> Result<String> {
    let label = tool.label();
    let steps = steps(tool)?.join("\r\n    ");
    let finished = quote_powershell(&finished(tool));
    // The byte order mark makes Windows PowerShell read the paths in the script as UTF-8.
    Ok(format!(
        "\u{feff}# Generated by Kodem. Installs {label} for this user.\r\n\
         $ErrorActionPreference = 'Stop'\r\n\
         $ProgressPreference = 'SilentlyContinue'\r\n\
         # Runs a program and stops the installation when it fails.\r\n\
         function Invoke-Step {{\r\n\
         \x20   param([string]$Program, [string[]]$Arguments)\r\n\
         \x20   & $Program @Arguments\r\n\
         \x20   if ($LASTEXITCODE -ne 0) {{ throw \"$Program exited with code $LASTEXITCODE\" }}\r\n\
         }}\r\n\
         # Reads PATH again, so this script finds the programs that an installer just added.\r\n\
         function Update-Path {{\r\n\
         \x20   $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')\r\n\
         }}\r\n\
         # winget comes with App Installer on Windows 10 and 11.\r\n\
         function Assert-Winget {{\r\n\
         \x20   if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {{ throw 'winget is missing. Install App Installer from the Microsoft Store.' }}\r\n\
         }}\r\n\
         Write-Host 'Installing {label}...'\r\n\
         try {{\r\n\
         \x20   {steps}\r\n\
         \x20   Write-Host ''\r\n\
         \x20   Write-Host {finished}\r\n\
         }} catch {{\r\n\
         \x20   Write-Host ''\r\n\
         \x20   Write-Host \"{label} was not installed: $_\"\r\n\
         }}\r\n\
         Read-Host 'Press Enter to close this window'\r\n"
    ))
}

// The PowerShell statements that install a tool on Windows.
#[cfg(windows)]
fn steps(tool: Tool) -> Result<Vec<String>> {
    let winget = |id: &str| {
        format!(
            "Invoke-Step winget @('install', '--id', '{id}', '--exact', '--source', 'winget', '--accept-package-agreements', '--accept-source-agreements')"
        )
    };
    Ok(match tool {
        Tool::Claude => vec!["Invoke-RestMethod https://claude.ai/install.ps1 | Invoke-Expression".into()],
        // OpenCode's install script is for macOS and Linux. Its npm package installs
        // `opencode.cmd` into the npm folder, where Kodem looks for the CLI.
        Tool::OpenCode => vec![
            "if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { throw 'npm is missing. Install Node.js from https://nodejs.org first.' }".into(),
            "Invoke-Step npm @('install', '--global', 'opencode-ai')".into(),
        ],
        // The script installs `agy.exe` into %LOCALAPPDATA%\agy\bin, where Kodem looks for it.
        Tool::Antigravity => vec![
            "Invoke-RestMethod https://antigravity.google/cli/install.ps1 | Invoke-Expression".into(),
        ],
        // The script installs `grok.exe` into %USERPROFILE%\.grok\bin, where Kodem looks for it.
        Tool::Grok => {
            vec!["Invoke-RestMethod https://x.ai/cli/install.ps1 | Invoke-Expression".into()]
        }
        Tool::Codex => {
            let target = format!("{}-{CODEX_SYSTEM}", std::env::consts::ARCH);
            vec![
                format!(
                    "$runtimes = {}",
                    quote_powershell(&codex::runtimes_dir()?.to_string_lossy())
                ),
                "New-Item -ItemType Directory -Force -Path $runtimes | Out-Null".into(),
                // The download stays on the runtimes' volume, so the final move is a rename.
                "$stage = Join-Path $runtimes ('.install-' + [Guid]::NewGuid())".into(),
                "New-Item -ItemType Directory -Path (Join-Path $stage 'package') | Out-Null".into(),
                format!(
                    "Invoke-WebRequest -UseBasicParsing -Uri 'https://github.com/openai/codex/releases/latest/download/codex-package-{target}.tar.gz' -OutFile (Join-Path $stage 'codex.tar.gz')"
                ),
                "Invoke-Step (Join-Path $env:SystemRoot 'System32\\tar.exe') @('-xzf', (Join-Path $stage 'codex.tar.gz'), '-C', (Join-Path $stage 'package'))".into(),
                "$package = Join-Path $stage 'package'".into(),
                format!(
                    "if (-not (Test-Path -LiteralPath (Join-Path $package 'codex-package.json'))) {{ $package = Join-Path $package 'codex-package-{target}' }}"
                ),
                "$version = (Get-Content -Raw -LiteralPath (Join-Path $package 'codex-package.json') | ConvertFrom-Json).version".into(),
                "if (-not $version) { throw 'The Codex package has no version.' }".into(),
                "$destination = Join-Path $runtimes $version".into(),
                "if (Test-Path -LiteralPath $destination) { Remove-Item -LiteralPath $destination -Recurse -Force }".into(),
                "Move-Item -LiteralPath $package -Destination $destination".into(),
                "Remove-Item -LiteralPath $stage -Recurse -Force".into(),
                "Write-Host \"Installed Codex $version in $destination\"".into(),
            ]
        }
        Tool::Git => vec!["Assert-Winget".into(), winget("Git.Git")],
        Tool::GitLfs => vec![
            "Assert-Winget".into(),
            winget("GitHub.GitLFS"),
            "Update-Path".into(),
            "Invoke-Step git @('lfs', 'install')".into(),
        ],
        Tool::GitHub => vec![
            "Assert-Winget".into(),
            winget("GitHub.cli"),
            "Update-Path".into(),
            // Reviews of pull requests need a signed-in GitHub CLI. Windows PowerShell stops at
            // redirected error output of a program while errors stop the script.
            "$ErrorActionPreference = 'Continue'; & gh auth status *> $null; $signedIn = $LASTEXITCODE -eq 0; $ErrorActionPreference = 'Stop'".into(),
            "if (-not $signedIn) { Invoke-Step gh @('auth', 'login') }".into(),
        ],
    })
}

// Quotes text as one PowerShell string that expands nothing.
#[cfg(windows)]
fn quote_powershell(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}
