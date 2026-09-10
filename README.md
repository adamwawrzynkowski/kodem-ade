# Kodem ADE

A native Rust and GPUI coding workspace. This first implementation connects task conversations, Git review, captured feedback and a terminal through a durable local host. Codex is the supported provider. Kodem requires CLI 0.153.4 or newer, with no upper version limit.

## Build on macOS

Requires Apple Silicon and macOS 14 or newer. Double-click one of these files in Finder:

| Command | Result |
| --- | --- |
| [Build Debug.command](Build%20Debug.command) | Builds `target/debug/Kodem Debug.app`, local only |
| [Build Release Nightly.command](Build%20Release%20Nightly.command) | Builds `target/nightly/Kodem Nightly.app` and publishes a GitHub prerelease |
| [Build Release Production.command](Build%20Release%20Production.command) | Builds `target/production/Kodem.app` and publishes the latest stable GitHub release |

All commands check Apple Command Line Tools, Git, Rust 1.97.1, rustfmt, Clippy and a complete Codex runtime package at or above the minimum version. They reuse the newest valid installed Codex package, or install the minimum version when none is available. They install missing prerequisites, fetch locked Cargo dependencies, format-check, compile, package and verify signatures. They do not launch or stop the app. Debug keeps its current local workflow. Release commands keep local bundles and publish a ZIP for installation and updates; they do not replace an app in `/Applications` during publication.

Release commands require GitHub CLI with repository write access, a clean checkout, and a commit already available on GitHub. They never commit or push source. Run `gh auth login` before publishing. Each channel starts at the workspace version and increments its patch version on subsequent releases. Set `KODEM_VERSION=X.X.X` to choose a newer version explicitly. Nightly uses `nightly/vX.X.X` tags and GitHub prereleases; Production uses `vX.X.X` and marks the release as latest. Uploads remain drafts until GitHub's SHA-256 digest matches the archive.

```sh
./'Build Debug.command'
./'Build Release Nightly.command'
./'Build Release Production.command'
```

Set `KODEM_NONINTERACTIVE=1` to skip the final Return prompt. Incremental builds reuse `target/aarch64-apple-darwin/`. Build metadata is embedded in the executable and `Contents/Info.plist`; `Contents/Resources/BuildInfo.txt` includes the build ID. Logs are saved under `target/build-debug.log`, `target/build-nightly.log` and `target/build-production.log`. Bundles use ad hoc signatures; Developer ID signing and notarization are not configured.

## Updates

Install the released app in a writable Applications folder. Nightly and Production check their own GitHub release channels at startup and every 30 minutes. Debug does not make update requests. The titlebar shows `Kodem X.X.X` or `Kodem Nightly X.X.X`, with the version in a subtle rounded badge.

When an update is available, **Update ↓** appears beside Settings. Click it to download and view progress. Once the archive's size, SHA-256 digest, bundle identity, channel, version and code signature have been checked, the control becomes a reload icon. Click to save state, stop the idle host, install and reopen. Active tasks and checks block restart; terminal sessions close when the update is applied. Failed downloads can be retried. Installation failures restore the previous bundle and retain a log.

The release repository is private. Users need GitHub CLI installed and signed in with read access to `adamwawrzynkowski/kodem-ade`; credentials are never bundled. Public releases work without a token if the repository becomes public. The Codex runtime remains a separate prerequisite installed by the build commands. See [release implementation](docs/implementation/releases.md) for the channel and installation contract.

## Start a task

Open the app, choose a repository, then create a task. Tasks use the current checkout by default. Choose **Settings → Workspace → Default task location** to default to a new worktree instead; a repository needs an initial commit for that option. Each new task starts with the saved preference, and you can override it in the composer for that task. Selecting a task never checks out another branch in your original working directory.

Kodem uses Codex's own authentication. If you have not signed in, run this in Terminal:

```sh
codex login
```

If Codex is installed only by Kodem's build setup, use `~/.local/share/kodem/runtimes/0.153.4/bin/codex login` instead.

Use **Settings → Codex** to refresh the account connection. Choose the model, reasoning effort and access from their separate composer menus. The effort slider supports dragging and arrow keys. Access defaults to Workspace; AI auto-approve routes permission requests to Codex’s risk-based AI reviewer while retaining the workspace sandbox. Read only restricts execution, and Full access removes sandbox and approval prompts. These controls apply to the next turn and cannot change during active execution. Compose a prompt and press Return (or Command-Return); Shift-Return inserts a newline. Commands and file changes stream into the transcript. Approval requests wait for your response. Model questions appear inside the composer with selectable options, descriptions, and a custom-answer field. Answer every question and select **Send answers**; each new request triggers the configured attention notification. Async questions remain available after the model finishes or the app restarts; answers reach the active turn immediately or start the next turn. Stop interrupts the active turn. A transport failure with uncertain delivery requires explicit history reconciliation; Kodem does not silently resend your prompt.

The sidebar shows project icons, task status and elapsed time, the task branch and its provider logo. New threads receive a short model-generated title in the background; manual renames take precedence. Filter by project or search titles, projects and branches. Completed tasks collect under Settled. A task's +/− badge opens its saved completion snapshot; later checkout edits do not change it. Older tasks show no historical total until another turn completes.

Two small composer meters show context occupancy in cyan and account limits in an amber ring. Hover for token counts, usage windows, reset estimates and report age. They use provider-reported data and remain unknown until it is available.

Chat supports headings, bold and italic text, inline code, strikethrough, nested bullet and numbered lists, task lists, quotes, dividers, tables and syntax-colored code blocks. Text remains selectable; Command-click a web link to open it. Tool groups expand into compact call rows. Open a call to inspect its command, status and output. Long output starts with a preview of up to 12 lines; **Show full output** expands it inside a scrollable block. Separate copy controls retain access to the full captured output and original tool transcript.

The Changes dropdown offers working-tree, staged, unstaged and task-baseline comparisons. Live comparisons refresh automatically while the Changes view or an open diff is visible. Selected diff text or lines retain their snapshot until **Load latest** is chosen; saved completion snapshots stay fixed. Selecting a file opens its diff above the composer by default; **Settings → Workspace → Open file diffs in** can use the right panel instead. The diff toolbar keeps only file identity, split/unified, refresh and close controls. Drag through code to select and copy text across lines. Drag the line numbers to select a range and add a comment below it. The resulting composer block retains the original snapshot, both old/new ranges, excerpt and comment even after files change.

Paste copied files or images with Command-V, drop them into the composer, or use the paperclip. Images show thumbnails; files show name and size. Click an image thumbnail in a draft, queued message or sent message to open a window-sized preview. The preview fits the whole image on opening and has icon-only zoom and close controls. Scroll to inspect an enlarged image, and press Escape to close it. Imports create stable private copies (up to 32 files, 32 MiB each, 128 MiB total). Submitted messages retain their attachment and code-context blocks.

**Settings → Codex → Messages during a turn** selects Queue or Steer. Queue waits for successful completion; Steer adds input to the running turn without restarting it. The composer also exposes the current choice while work runs. The tray above it shows upcoming messages; expand an entry to inspect its content, cancel it, or restore it to an empty composer for editing. Failures and interruptions pause the queue until Resume. Unknown delivery requires Check history before resuming; uncertain messages are never replayed automatically.

**Settings → Notifications** controls alerts for tasks that need input or attention, finish, fail, or are interrupted. Choose Chime, Bell, Glass, Pulse, or Ascend, turn sounds off, and use **Test notification** to send a preview. The host sends alerts after task state changes, including while the window is closed. Repeated events and stored history do not replay alerts. Settings reports submission or permission errors; macOS notification and Focus settings control presentation.

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

Data lives in `~/Library/Application Support/Kodem/`. Debug and release use the same store by default. `KODEM_DATA_DIR` selects an isolated store. Database upgrades reject newer schemas, back up existing older schemas and apply changes transactionally.

`KODEM_CODEX_PATH` overrides the runtime path and must point into a complete Codex package meeting the minimum in `codex-min-version`. Otherwise Kodem selects the newest valid package from PATH, common installation locations and `~/.local/share/kodem/runtimes/*/`. Selection happens at runtime, so upgrading Codex does not require rebuilding Kodem. The installer and application share the same minimum requirement. Future incompatible app-server protocol changes may still require adapter updates.

GPUI-CE 0.2.2 and its platform package 0.1.0 share one native type graph. The component layer contains a small, attributed port of GPUI Kit/base 0.6.0's Button and InputBase. It is not the full Kit library. See [third-party notices](assets/THIRD_PARTY.md).

The accepted [architecture](docs/design/architecture.md), [design guidance](docs/design/design-guidance.md), [implementation plan](docs/design/implementation-plan.md) and [research](docs/design/research.md) remain available. [Design validation](docs/design/validation.md) records the earlier dependency probe separately from this implementation.

The bundled app icon comes from `assets/app/AppIcon_1024x1024.png`. All build commands generate a complete macOS iconset from that source. The window uses the saved accent for its top-left dithering and gray for its bottom-right shading. Settings → Appearance offers Orange, Blue, Green, Purple, Pink and Teal. Orange remains the default; the choice also colors controls, inline formatting and working indicators.

Settings → Projects lets you rename projects and choose their accent and icon, including your own image. Imported images are stored as small PNG copies, so moving or deleting the original does not remove the project icon. New projects follow the app accent; existing saved project colors are retained. Removing a project hides it and its tasks while keeping files and local history. Reopen the repository to restore it. Active or unresolved work must finish before removal.

New Task opens an empty chat with the app icon and a greeting using the local account's first name. Click the project name to switch or add a repository, and choose New worktree or Current checkout inside the composer. Choose a model, reasoning effort and access before creating the task. The compact + button and Enter both create the task and immediately submit the first message. Working indicators use a nine-pixel dither box, with a static pattern when reduced motion is enabled.

The desktop retries failed local connections. An older idle host accepts an upgrade through its guarded shutdown request; active tasks and checks postpone that upgrade. A newer host is never downgraded by an older app. Settings distinguish an unavailable host from an unavailable runtime, and rejected operations are not replayed during reconnect.

The interface uses bundled IBM Plex Sans, with bold Manrope for the Kodem wordmark and JetBrains Mono for code. Task cards use compact spacing, subdued project colors and shaded surfaces. Model, reasoning and access pickers omit redundant headings. Context fills as a clockwise sector; account usage fills as a clockwise ring. Progressive backdrop blur is confined to the rounded prompt field, beneath its input and toolbar.

Kodem uses HTTP streaming for its OpenAI sessions to avoid repeated WebSocket connection failures at thread startup. Retryable provider errors appear as transient reconnect status; terminal errors remain in the transcript. Custom provider selections are preserved.

## License

Kodem is proprietary software. See [LICENSE](LICENSE). Third-party components retain their own licenses; see [third-party notices](assets/THIRD_PARTY.md) and [licensing maintenance](licensing/README.md). Builds verify license notices and regenerate stale notices under `target/` using the existing license policy. The pinned scanner is installed when needed. Generated notices are bundled without changing tracked files; unresolved license review failures still stop the build.
