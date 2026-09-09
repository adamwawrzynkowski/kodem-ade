# Kodem ADE

A native Rust and GPUI coding workspace. This first implementation connects task conversations, Git review, captured feedback and a terminal through a durable local host. Codex is the supported provider, pinned to CLI 0.153.4.

## Build on macOS

Requires Apple Silicon and macOS 14 or newer. Double-click either file in Finder:

| Command | Result |
| --- | --- |
| [Build Debug.command](Build%20Debug.command) | Builds `target/debug/Kodem Debug.app` |
| [Build Release.command](Build%20Release.command) | Builds `target/release/Kodem.app` and installs `/Applications/Kodem.app` |

Both commands check Apple Command Line Tools, Git, Rust 1.97.1, rustfmt, Clippy and the complete pinned Codex runtime package, including `codex-code-mode-host`, bundled ripgrep and zsh. They install missing prerequisites and fetch the locked Cargo dependencies. SQLite is bundled; Homebrew, Node and Python are not required. First builds need an internet connection. Apple tools and installation into a protected Applications directory can require an administrator password. If Apple only offers its interactive tools installer, finish that installer and rerun the command.

The scripts format-check, compile, package and verify the app signature. They leave the app closed. Release installation verifies a staged bundle before replacement and retains any previous matching installation as `Kodem.previous.<timestamp>.app`. It refuses to replace an unrelated application or a running installed Kodem process.

The same commands work from Terminal:

```sh
./'Build Debug.command'
./'Build Release.command'
```

Set `KODEM_NONINTERACTIVE=1` to skip the final Return prompt. Normal incremental builds reuse `target/`. These are local, ad hoc signed bundles; notarization and automatic updates are not implemented.

## Start a task

Open the app, choose a repository, then create a task. An isolated Git worktree is the default; a repository needs an initial commit for that option. You can also choose its current checkout. Selecting a task never checks out another branch in your original working directory.

Kodem uses Codex's own authentication. If you have not signed in, run this in Terminal:

```sh
~/.local/share/kodem/runtimes/0.153.4/bin/codex login
```

Use **Settings → Codex** to refresh the account connection. Choose the model and reasoning effort beside the composer. Compose a prompt and press Command-Return. Commands and file changes stream into the transcript. Approval and question requests wait for your response. Stop interrupts the active turn. A transport failure with uncertain delivery requires explicit history reconciliation; Kodem does not silently resend your prompt.

The Changes pane offers working-tree, staged, unstaged and task-baseline comparisons. Drag its narrow gutter to select source lines and add feedback. Queued feedback retains the original snapshot, path bytes, side, range and excerpt even after files change. Whole-file staging checks that both file contents and the index still match the capture.

The Files dialog provides read-only text previews. The terminal starts a shell in the task's checkout. Run check records the command, checkout revision, working-tree digest, output and exit code in the conversation. Skills & MCP shows provider-reported inventory and discovered instruction files.

## Current scope

The redesigned UI uses an integrated titlebar, project tree, docked composer, contextual review, tabbed preferences and a split file browser. It includes configurable sidebar widths, task search and sorting, pin/archive/rename actions, light and dark colors, density and font settings, paged chat history, Markdown blocks, code highlighting, unified/split diffs, a feedback queue and a PTY terminal screen. Fonts are bundled.

This is an initial Codex implementation. The broader [product specification](docs/design/product-spec.md) remains the roadmap. Native file editing, other providers, PR delivery, MCP configuration editing, scheduling and remote access are not implemented. Diff context expansion, intraline highlighting, partial staging, multi-line text selection across diff rows, full terminal emulation controls and accessibility acceptance also remain open.

The redesign was inspected using actual native GPUI Metal frames and app-local input events in an isolated project. Dark/light themes, narrow windows, file browsing, task creation, search, focus restoration, feedback and staging were exercised. This does not establish a complete accessibility or IME pass, or a real authenticated provider turn. See [implementation and validation](docs/implementation/progress.md) for the evidence and remaining gates.

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
