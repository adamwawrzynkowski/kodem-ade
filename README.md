# Kodem ADE

A native Rust and GPUI coding workspace. This first implementation connects task conversations, Git review, captured feedback and a terminal through a durable local host. Codex is the supported provider, pinned to CLI 0.153.4.

## Build on macOS

Requires Apple Silicon and macOS 14 or newer. Double-click either file in Finder:

| Command | Result |
| --- | --- |
| [Build Debug.command](Build%20Debug.command) | Builds `target/debug/Kodem Debug.app` |
| [Build Release.command](Build%20Release.command) | Builds `target/release/Kodem.app` and installs `/Applications/Kodem.app` |

Both commands check Apple Command Line Tools, Git, Rust 1.97.1, rustfmt, Clippy and the complete pinned Codex runtime package, including `codex-code-mode-host`, bundled ripgrep and zsh. They install missing prerequisites and fetch the locked Cargo dependencies. SQLite is bundled; Homebrew, Node and Python are not required. First builds need an internet connection. Apple tools and installation into a protected Applications directory can require an administrator password. If Apple only offers its interactive tools installer, finish that installer and rerun the command.

The scripts format-check, compile, package and verify the app signature. They do not launch or stop the app. Release installation verifies a staged bundle before replacement and retains any previous matching installation as `Kodem.previous.<timestamp>-<pid>.app`. It refuses to replace an unrelated application. A running Kodem no longer blocks installation: its previous bundle is kept intact, and the command reports that a restart is required to load the installed build. Finish active work, quit the old app, then open `/Applications/Kodem.app`. You do not need to rebuild after quitting. Installed executables and metadata are compared against the packaged output before success is reported.

The same commands work from Terminal:

```sh
./'Build Debug.command'
./'Build Release.command'
```

Set `KODEM_NONINTERACTIVE=1` to skip the final Return prompt. Normal incremental builds reuse `target/aarch64-apple-darwin/`; the app bundles remain in `target/debug/` and `target/release/`. The scripts explicitly choose the output directory and Apple Silicon target, so Cargo environment overrides cannot make packaging pick up an older executable. Each bundle includes a timestamped build ID and `Contents/Resources/BuildInfo.txt`. Command output is saved to `target/build-debug.log` or `target/build-release.log`. These are local, ad hoc signed bundles; notarization and automatic updates are not implemented.

## Start a task

Open the app, choose a repository, then create a task. An isolated Git worktree is the default; a repository needs an initial commit for that option. You can also choose its current checkout. Selecting a task never checks out another branch in your original working directory.

Kodem uses Codex's own authentication. If you have not signed in, run this in Terminal:

```sh
~/.local/share/kodem/runtimes/0.153.4/bin/codex login
```

Use **Settings → Codex** to refresh the account connection. Choose the model, reasoning effort and access from their separate composer menus. The effort slider supports dragging and arrow keys. Access defaults to Workspace; Read only restricts execution, and Full access removes sandbox and approval prompts. These controls apply to the next turn and cannot change during active execution. Compose a prompt and press Return (or Command-Return); Shift-Return inserts a newline. Commands and file changes stream into the transcript. Approval and question requests wait for your response. Stop interrupts the active turn. A transport failure with uncertain delivery requires explicit history reconciliation; Kodem does not silently resend your prompt.

The sidebar shows project icons, task status and elapsed time, the task branch and its provider logo. Filter by project or search titles, projects and branches. Completed tasks collect under Settled. A task's +/− badge opens its saved completion snapshot; later checkout edits do not change it. Older tasks show no historical total until another turn completes.

Two small composer meters show context occupancy in cyan and account limits in an amber ring. Hover for token counts, usage windows, reset estimates and report age. They use provider-reported data and remain unknown until it is available.

The Changes dropdown offers working-tree, staged, unstaged and task-baseline comparisons. Selecting a file opens its diff above the composer by default; **Settings → Workspace → Open file diffs in** can use the right panel instead. Drag through code to select and copy text across lines. Drag the line numbers to select a range and add a comment below it. The resulting composer block retains the original snapshot, both old/new ranges, excerpt and comment even after files change. Whole-file staging checks that both file contents and the index still match the capture.

Drop files or images into the composer, or use the paperclip. Images show thumbnails; files show name and size. Imports create stable private copies (up to 32 files, 32 MiB each, 128 MiB total). Submitted messages retain their attachment and code-context blocks.

**Settings → Codex → Messages during a turn** selects Queue or Steer. Queue waits for successful completion; Steer adds input to the running turn without restarting it. The composer also exposes the current choice while work runs. The tray above it shows upcoming messages; expand an entry to inspect its content, cancel it, or restore it to an empty composer for editing. Failures and interruptions pause the queue until Resume. Unknown delivery requires Check history before resuming; uncertain messages are never replayed automatically.

The Files dialog provides read-only text previews. The terminal starts a shell in the task's checkout. Run check records the command, checkout revision, working-tree digest, output and exit code in the conversation. Skills & MCP shows provider-reported inventory and discovered instruction files.

## Current scope

The redesigned UI uses an integrated titlebar, project tree, floating composer with progressive backdrop blur and subtle spring motion, contextual review, tabbed preferences and a split file browser. It includes configurable sidebar widths, task search and sorting, pin/archive/rename actions, light and dark colors, density and font settings, paged chat history, Markdown blocks, code highlighting, unified/split diffs, a foldable Changes tree, compact context headers, a feedback queue and a PTY terminal screen. Fonts are bundled.

This is an initial Codex implementation. The broader [product specification](docs/design/product-spec.md) remains the roadmap. Native file editing, other providers, PR delivery, MCP configuration editing, scheduling and remote access are not implemented. Diff context expansion, intraline highlighting, partial staging, full terminal emulation controls and accessibility acceptance also remain open.

The earlier redesign was inspected using actual native GPUI Metal frames and app-local input events in an isolated project. Dark/light themes, narrow windows, file browsing, task creation, search, focus restoration, feedback and staging were exercised. This does not establish a complete accessibility or IME pass, or a real authenticated provider turn. The later conversation workflow changes were checked without visual testing or a real authenticated provider turn. See [implementation and validation](docs/implementation/progress.md) for the evidence and remaining gates.

## Source and data

| Crate | Responsibility |
| --- | --- |
| `kodem-core` | Typed local protocol, SQLite storage, Git snapshots, Codex transport |
| `kodem-host` | Task runtime ownership, delivery recovery, PTYs and check execution |
| `kodem-desktop` | Native GPUI-CE workspace and shared controls |

The host starts on demand. Closing the window keeps active tasks and terminals alive. Settings includes an explicit host shutdown action. An unused host with no active task, check or terminal exits after five minutes.

Data lives in `~/Library/Application Support/Kodem/`. Debug and release use the same store by default. `KODEM_DATA_DIR` selects an isolated store; `KODEM_CODEX_PATH` overrides the runtime path but must point into the complete supported Codex package. Database upgrades reject newer schemas, back up existing older schemas and apply changes transactionally.

GPUI-CE 0.2.2 and its platform package 0.1.0 share one native type graph. The component layer contains a small, attributed port of GPUI Kit/base 0.6.0's Button and InputBase. It is not the full Kit library. See [third-party notices](assets/THIRD_PARTY.md).

The accepted [architecture](docs/design/architecture.md), [design guidance](docs/design/design-guidance.md), [implementation plan](docs/design/implementation-plan.md) and [research](docs/design/research.md) remain available. [Design validation](docs/design/validation.md) records the earlier dependency probe separately from this implementation.

The bundled app icon comes from `assets/app/AppIcon_1024x1024.png`. Both build commands generate a complete macOS iconset from that source. The window carries subtle orange top-left and gray bottom-right dithered shading.

The desktop retries failed local connections. An older idle host accepts an upgrade through its guarded shutdown request; active tasks and checks postpone that upgrade. A newer host is never downgraded by an older app. Settings distinguish an unavailable host from an unavailable runtime, and rejected operations are not replayed during reconnect.
