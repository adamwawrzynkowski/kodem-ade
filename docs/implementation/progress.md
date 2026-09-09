# Implementation and validation

Native Codex implementation, 9 September 2026. The accepted [architecture](../design/architecture.md) defines the crate boundaries. The broader [implementation plan](../design/implementation-plan.md) still contains future product phases; this record does not mark those phases complete.

## Implemented

| Area | Current artifact |
| --- | --- |
| Native dependencies | Rust 1.97.1, GPUI-CE 0.2.2, platform 0.1.0, one GPUI type graph, a small attributed Kit/base port, bundled fonts |
| Core and persistence | Three crates; host-owned SQLite with WAL; transactional migrations and backups; isolated task drafts; bounded transcript pages; durable outgoing submissions; content-addressed snapshot blobs |
| Host and Codex | Private Unix socket and peer UID check; single host lock; one provider worker per task; pinned CLI protocol; streaming, explicit approvals/questions, model discovery, interruption and identity-based delivery reconciliation |
| Task workspace | Repository registration; optional isolated worktrees; project/task navigation; search, sort, pin, archive and rename; ordering held while hovering; saved density, widths, font size and theme |
| Chat | Native Markdown blocks; selectable text; syntax colors; folded tools; paged history retained across polls; follow-latest behavior; task drafts retained until acknowledgement |
| Review | Immutable real Git captures; working-tree, staged, unstaged and baseline comparisons; unified/split rows; horizontal scrolling; source-side gutter feedback; stale-selection retention; guarded whole-file staging |
| Supporting tools | Read-only file preview; task PTY screen and keyboard input; explicit check command with revision/output/exit evidence; instruction and provider skill/MCP inventory |
| Packaging | Executable Finder commands; dependency installation; locked debug/release builds; app/host bundling; signature verification; staged release installation in Applications with backup |

Codex 0.153.4's local CLI and generated protocol definitions were inspected for the implemented requests. The transport follows the [official app-server documentation](https://learn.chatgpt.com/docs/app-server). Paginated history and client message identity are version-specific. Unknown delivery is never resolved by comparing prompt text alone, and failed reconciliation never resends the submission.

## Initial implementation verification

The verification machine is Apple Silicon arm64 running macOS 27.0 with Apple Command Line Tools and Rust 1.97.1. No global Xcode selection or Rust default was changed. The initial implementation was not opened or controlled; the authorized redesign pass below supersedes that restriction.

Nine temporary Rust tests passed through `cargo test --workspace --locked -- --test-threads=1`:

1. Captured old/new CRLF excerpts, valid and invalid feedback ranges, snapshot reload, preservation of unrelated edits, whole-file staging and stale-content rejection.
2. Filenames containing newlines, rename identity, binary metadata and symlink changes. The macOS filesystem rejected an invalid-UTF8 filename fixture, so that filesystem case remains unverified; Git paths are represented as bytes in the protocol.
3. Staged additions before the first commit, and committed changes relative to a task baseline. This exposed an initial-commit capture bug that was corrected before the passing run.
4. Independent drafts, submission deduplication, crash recovery, paged transcript persistence and reopening the database.
5. Isolated worktree creation without switching the original checkout or changing its dirty file.
6. Mock Codex framing, preserved approval request identity, streamed events and explicit rejection classification.
7. Schema-1 backup and migration, reopening schema 2, and rejection of a newer schema.
8. Earlier chat pages retained across polling, overlapping-page deduplication and final-message replacement.
9. Provider startup failure, explicit turn rejection, completion arriving before acknowledgement, idle transport disconnect, matching/mismatching reconciliation receipts and preservation of a newer unsent draft.

The Codex tests used temporary local protocol fixtures. They did not send an authenticated provider turn or allow a model to change a repository. The UI history test exercised a pure merge function without starting GPUI. Temporary test source, fixtures and logs were removed after recording results, as required by the workspace agreement.

The dependency check also exercised a missing private Codex runtime with a real download from OpenAI's release archive. It extracted and verified `codex-cli 0.153.4` in a temporary directory, then removed that directory. A separate build installed the compatible runtime under `~/.local/share/kodem/runtimes/0.153.4/`. Existing global Codex was preserved. Missing Rust and Apple tools installation paths were inspected but were not forced on this already configured machine.

Initial implementation verification passed:

| Check | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed for the workspace |
| `Build Debug.command` with `KODEM_NONINTERACTIVE=1` | Exit 0; `target/debug/Kodem Debug.app` built and verified |
| `Build Release.command` with `KODEM_NONINTERACTIVE=1` | Exit 0; `target/release/Kodem.app` built and installed in `/Applications/Kodem.app` |
| Installed signature and plist | `codesign --verify --deep --strict` and `plutil -lint` passed |
| Installed app and host executables | Both Mach-O arm64; SHA-256 hashes match the corresponding local release bundle files |
| Native dependency graph | `gpui-ce` is present; `gpui-pre` is absent from the resolved graph |
| Packaged host process check | Real private socket, single-host lock, Git capture/feedback, check execution, restart persistence and explicit shutdown passed in an isolated temporary directory |
| Repository audit | 36 local document links, code fences, public method summaries and temporary test removal passed; `git diff --check` passed |

The packaged host check executed only the non-GUI host and an explicit local `printf` check. A version-only fixture rejected any attempt to start a provider. Temporary repositories and host data were removed afterward.

The first release install exposed an empty-array expansion error under macOS Bash 3.2 with `set -u`. The installer now uses an explicit privilege helper. Both final Finder command files were executed through their shell entry points after that correction; Finder itself was not controlled.

Initial installed desktop SHA-256: `6bbbd1d5dc58b4588ad8d4f8e0df1bb31cbc71f175a322539c98c8bbe73580c9`.
Initial installed host SHA-256: `5df39fc84cf8bdd3b5175abf278877a50683f1c5dd1b55a318ceff72635a39d1`.


## Whole redesign verification

The user rejected the first interface and explicitly authorized native visual testing on 9 September 2026. The [redesign record](ui-redesign.md) documents the new layout, method and corrections.

| Area | Confirmed result |
| --- | --- |
| Actual native rendering | Inspected empty/populated workspaces, dark/light settings, files, code review, inline feedback, menus, terminal and task creation through GPUI Metal scene exports |
| Native input dispatch | File clicks, tool disclosure, search text, repeated sidebar shortcuts, theme choices, Escape, narrow Changes navigation, task creation and gutter selection passed app-local event assertions |
| Feedback and Git | Selected new-side lines 12–13, typed a comment, queued it through the host, and staged only the intended fixture file through the Stage control |
| Terminal | App-local typing executed a temporary printf command and its output appeared in the terminal pane |
| Catalog transport | Fixed inherited nonblocking mode on accepted macOS sockets; the real 20,890-byte, 53-entry catalog arrived complete and rendered after previously stopping at 8,192 bytes |
| Model menu | Read and rendered the runtime's actual model catalog; no model turn was submitted |
| Responsive layout | Inspected 1512 × 982 and 950 × 700 windows; narrow views can switch between conversation and Changes |
| Cleanup | Temporary visual feature, capture driver, assertions, fixture repositories, databases, logs and screenshots removed before the normal builds |

The source fixes include explicit icon color inheritance, default workspace focus and focus restoration, independently scrollable split columns with shared vertical position, non-wrapping line-number gutters, and removal of redundant workspace refreshes after terminal input acknowledgements. Provider execution, persisted snapshots and Git guards retain their existing ownership.

Desktop-level automation was unavailable because macOS denied Accessibility and Screen Recording access. These checks exported Kodem's own native scene and dispatched events inside its real window. They are neither browser mockups nor proof of OS accessibility, system traffic-light rendering or real IME composition.

## Open acceptance gates

The user explicitly authorized visual testing for the whole redesign. Native rendering and the app-local interaction paths recorded below were tested. OS accessibility automation and desktop capture were unavailable. A complete keyboard/accessibility review, IME composition, cross-row text selection, long-history scrolling and sustained performance still need acceptance testing.

No end-to-end Codex turn through the Kodem UI, live approval round-trip, external MCP tool execution, long-running reconnect or performance benchmark was performed. The later runtime repair was checked with an ephemeral authenticated CLI turn that executed a terminal command and read a fixture file. Inventory is labeled as provider-reported capability, not a successful tool execution check. Unknown-delivery reconciliation remains unresolved when provider history cannot prove identity or terminal state.

The component compatibility work covers Button, InputBase and supporting state contracts. It does not establish compatibility for the complete GPUI Kit library. Markdown styling is partial; tables use text columns and links display their URL. The file viewer is read-only. Full terminal scrollback controls, ANSI color presentation, editor undo/redo, diff context expansion, intraline marks, partial staging, keyboard range extension and drag autoscroll are future work. Secret-input masking is not implemented in provider question fields.

Other providers, native editing/LSP, PR publishing, scheduling, remote access, snapshot retention controls, a complete accessibility pass, notarization and automatic updates remain in the product plan. Only Apple Silicon macOS compilation and local packaging were verified. No Linux, Windows or Intel support is claimed.

Cargo reports a future-compatibility warning in transitive `block` 0.1.6. It compiles with the pinned toolchain. A future dependency/toolchain update must address that upstream warning.

## Ownership and rollback

The data directory is private to the current user. The host owns provider and terminal lifetimes independently of the window. It exits after five idle minutes when no task, check or terminal is active. Settings offers explicit host shutdown. Provider errors are retained in task state; raw provider stderr is drained without persisting potentially sensitive output.

Git writes are limited to explicitly requested whole-file staging and creation of new worktrees. Staging uses an index lock, a temporary index, fresh content/index checks and atomic replacement. The original checkout is never switched when selecting a task. Archiving does not delete a worktree.

Release installation only replaces a bundle with Kodem's identifier. It verifies a staging copy first and keeps the prior installation as a timestamped backup. Task data is outside the app bundle. A schema downgrade requires the retained pre-migration database backup; replacing only the app does not downgrade the store.

Initial Cargo.lock SHA-256: `f5114a352aceab2946dc9eb443f94f935ad0cbe4842a461536bbeef35d27f3f7`.


## Redesign delivery checks

The normal, instrumentation-free builds passed `cargo clippy --workspace --all-targets --locked -- -D warnings`, `Build Debug.command`, and `Build Release.command`. Both Finder command files were executed through their shell entry points with `KODEM_NONINTERACTIVE=1`; Finder itself was not automated. Dependency checks found the pinned Rust/Codex toolchains. The earlier missing-dependency installation evidence remains above.

The release was installed in `/Applications/Kodem.app`, its signature and plist verified, and both executable hashes matched the local release bundle. The previous application is retained at `/Applications/Kodem.previous.20260909-125608.app`. The old debug window and idle host were stopped, the installed release opened, and its new host returned the same persisted task identity, completed state and draft digest. One user task was preserved; no visual fixture entered the user store.

Final source review passed formatting, shell syntax, document links, public-method summaries and `git diff --check`. Temporary source, fixture data, captures and verification logs were removed. No commit or push was made.

Redesign kodem SHA-256: `f003ed1b870950866797b46b2415f2699dc778c820eced4313a90b477e2ae7bf`.

Redesign kodem-host SHA-256: `418aacf27231db1a2e434162336be6b343b7ad5d794828cf3c0a2f7f8ea2550d`.

Redesign Cargo.lock SHA-256: `f5114a352aceab2946dc9eb443f94f935ad0cbe4842a461536bbeef35d27f3f7`.

## Compact reference follow-up

The later screenshot comparison supersedes the first redesign's visual acceptance. The correction uses a 38 px titlebar, a small conversation tab, single-line task rows, system typography, a darker palette, content-sized user bubbles, grouped tool disclosures and a shallow composer. Changes defaults to the full-height file list and opens a selected file on demand. Search can be opened with its icon or Cmd-F and closed with Escape. A refresh retains file selection by path rather than by list index.

The build installer previously preserved only the Codex executable, which omitted codex-code-mode-host and broke tools. It now downloads and installs the complete official 0.153.4 package, including its metadata, tool host, ripgrep and zsh. Package checks ran successfully after repairing the installed private runtime. The runtime adapter also rejects incomplete packages before starting a provider session.

Visual testing stopped immediately at the user's request. Temporary instrumentation, fixture repositories, data, captures and logs were removed. Remaining verification is compilation and source review; the final visual result is not claimed accepted. The application is not launched as part of the final build/install checks.

Both double-click command entry points completed successfully with KODEM_NONINTERACTIVE=1. Formatting, shell syntax, Clippy with warnings denied, and git diff --check passed. The installed /Applications/Kodem.app passed strict signature verification, and its two executable hashes matched target/release/Kodem.app. The previous app is retained at /Applications/Kodem.previous.20260909-224759.app. No user task database was edited, and the installed app was not launched.

Current desktop SHA-256: `52b799cd6cc684bdab6fbebd5613dbda1f25195c9cddbdac593c2dad799b8bd6`.
Current host SHA-256: `7e78048487e83a7737fa8c6dc932b831f5424e7e71ab0853c9d836cf9cd0c58f`.

## Activity and animation delivery

The accepted compact UI now includes typed live activity, elapsed times, moving text highlights, animated work icons and short panel entrances. Text-button helpers require icons, and compact toolbar actions use accessible icon controls. Activity is transient host state, with an optional protocol field for compatibility. The desktop follows the native Reduce Motion preference.

Two temporary non-visual reducer tests passed, including concurrent completion, duplicates, turn cleanup and parsed-command classification. The pinned Codex schema confirmed the event names and fields. Temporary tests and generated schemas were removed. Final formatting, Clippy with warnings denied, shell syntax and diff checks passed. Both double-click build commands completed; the release was installed in /Applications/Kodem.app with the prior bundle retained at /Applications/Kodem.previous.20260909-230830.app. Its signature and installed executable hashes were verified. No visual testing, app launch or provider turn was performed for this change.

Installed desktop SHA-256: `48a8c0e6c69cff6811d967ddb3804ef09613e9639a9172bdc3d3a5be942718b8`.
Installed host SHA-256: `ebac0b05093c64b55c345f029ed50d19a85d46b05440bd3f4662616240a51133`.

## Conversation, diff selection, and outgoing messages

Changed files now open in the conversation area above the composer by default. Settings can move the diff to the right panel. One virtualized diff document owns character selection across rows, copy/select-all, gutter context ranges, autoscroll and an inline comment editor. Captured context preserves mixed removed/added rows and appears as a structured composer/message block. The former separate feedback dialog and row-local selection state were removed.

The composer accepts dropped files/images and picker imports, displays removable attachment cards, sends on Return or Command-Return, and inserts newlines with Shift-Return. Its IME guard suppresses submission during marked composition. Queue/Steer is configurable in Codex settings and exposed during active work. The host stores immutable input in its existing submission ledger; the tray supports inspection, cancellation, restoration and explicit resume after a pause. Steering calls the pinned `turn/steer` endpoint with its active-turn precondition.

Protocol 2 prevents the new desktop from reusing an incompatible old host. Schema 3 adds frozen submission payloads with a pre-migration backup. Imported attachment copies remain with retained messages. Serialized outbox, context and transcript budgets keep TaskView within the IPC frame limit. New persisted fields default when loading older data.

Validation used no app launch, visual test, screenshots, or real authenticated provider turn. An isolated real host with a simulated Codex JSONL transport passed file/image import and immutability, mixed feedback, FIFO queue dispatch, cancellation/restoration, duplicate identity checks, successful and rejected steering, preservation of newer drafts, completion-before-acknowledgement, uncertain-delivery recovery with later queued records, schema-2 migration/defaults, and size-limit rejection without consuming drafts. Eight checks of the actual text-selection functions passed forward/reverse, multiline, UTF-8/emoji, split-side, and empty-selection cases. Mixed Unicode diff-row extraction and invalid range checks also passed. Temporary fixtures, generated schemas and harness artifacts were removed.

Final `cargo clippy --workspace --all-targets --locked -- -D warnings`, formatting, shell syntax and `git diff --check` passed. Both double-click command entry points compiled and packaged their ARM64 bundles; strict signature verification passed for each. The release command then exited at its running-process installation check because the installed background host was still running. `/Applications/Kodem.app` was not replaced. The new release is ready at `target/release/Kodem.app`; stop the idle installed host and rerun `Build Release.command` to install it. No user task database was modified by verification, and no commit or push was made.

Release desktop SHA-256: `ff0802f854087da2cadc79d1816c1735fa892d78614687cafc6fd0063d5f84ad`.
Release host SHA-256: `7623e5fc92f6757a9f4a4e18e4f6f86925946b9d43ef2be6a70488bdf35aa5c5`.


## Release installation correction

The release command reproduced a successful compile/package followed by exit 1 at the running-process check. That left the installed application unchanged and the wrapper described the result as a build failure. The script also copied from a fixed profile directory even though Cargo could use an overridden target/output directory.

Builds now explicitly select the Apple Silicon target and repository target directory, then package executables from that exact location. Bundles carry a UTC build ID and BuildInfo.txt. The wrapper preserves the build command exit status through tee and retains a per-configuration log. Build and installation success are reported separately.

Release installation moves the previous bundle intact to a unique backup and promotes the verified staging bundle even if an older process is still running. It compares the installed desktop, host and plist to the package, and reports that running processes need a restart. No task, app or host is terminated or launched. Failed promotion restores the previous installation; cleanup also restores it if an interruption leaves the destination absent.

The original command failure was reproduced without app control. The corrected release command succeeded with conflicting CARGO_TARGET_DIR and CARGO_BUILD_TARGET overrides, and the installed desktop, host and build identity matched the package. An isolated test of the actual installation block passed simulated-running-process installation and failed-promotion rollback. Its temporary files were removed. Strict installed signature verification and shell syntax checks passed. The installed release build ID is 20260909T214412Z-33a2613-dirty, with the previous app retained at /Applications/Kodem.previous.20260909-234413-9948.app. No visual testing was performed.

The debug command also completed successfully with the explicit target directory. Both command files remain executable; final shell syntax, signature checks and git diff --check passed. Persistent command logs are intentional build diagnostics, not temporary test files.

## Floating composer and review refinement

The composer is a measured bottom overlay with native progressive backdrop blur. Its bounded spring responds to scroll, click and typing input, stops when settled and respects Reduce Motion. Transcript bottom padding and the center diff's reserved space follow the actual composer height. Shared surface colors, contact shadows and highlights add depth to the composer, attachments, menus, controls, message bubbles and selected navigation rows.

Context cards now render only file/range headers. The composer context tray scrolls at a bounded height and keeps cards from shrinking. The full captured excerpt and comment remain in task and outgoing message payloads. Changes navigation is a folder-first foldable tree with file-type icons, counts and change totals. Leaves retain snapshot indices, including duplicate display labels. Refresh, split and close controls live in the diff toolbar.

Diff rows now declare their width explicitly. A temporary nonvisual Taffy sizing check reproduced the previous 400px text fallback, which made unified rows 502px wide regardless of their available space. The corrected unified and split rows used the full 360px, 900px and 1800px widths. Horizontal scrolling now uses GPUI's axis restriction so vertical wheel input is not remapped onto the horizontal container.

The model popover has a spring-animated discrete effort slider with pointer and keyboard input and persisted per-task access choices. Workspace remains the default for existing and new tasks. The host rejects access changes during active execution and sends explicit sandbox/approval policies on thread creation/resume and each new turn. Protocol 3 rejects incompatible older hosts. The pinned runtime's generated JSON schema confirmed the policy shapes.

Workspace Clippy with warnings denied and formatting passed. Temporary checks of the actual folder construction code passed hierarchy, folding, totals, stable file indices and duplicate labels. Core checks passed all access policy mappings, task edit serialization, older-task defaults and permission round trips. Temporary verification sources, binaries and generated schemas were removed. These checks do not establish visual fidelity, pointer interaction acceptance or a real authenticated provider turn; no app was launched, controlled or screenshotted.

Both final command entry points compiled, packaged and verified their ARM64 bundles. Release build `20260909T220906Z-372257c-dirty` was installed at `/Applications/Kodem.app`; the previous bundle is retained at `/Applications/Kodem.previous.20260910-000906-43512.app`. Installed desktop, host and Info.plist bytes match the release package, and strict signature verification passed. Final debug build `20260909T221006Z-372257c-dirty` is at `target/debug/Kodem Debug.app`. Formatting and `git diff --check` passed. No commit or push was made.


## Task feed, usage and host reconnection

The left sidebar now presents compact task cards with project-specific glyphs and colors, live status and elapsed time, branch, provider logo and a saved completion diff badge. Project filtering, text search and a collapsible Settled section replace the nested task list. Completion captures a persistent snapshot before publishing completion; later checkout edits and repeated completion notifications cannot overwrite that snapshot. Older tasks retain an unknown historical total until new work completes. Branch updates are observed outside the host lock while a task is active and retained when it settles.

The composer has separate model, reasoning and access menus. Two minimal occupancy meters expose context tokens and account limit windows in hover details. Context reads the provider's last token usage, not its cumulative total. Account limits refresh in a bounded background query and accept sparse provider updates without erasing known windows. Missing telemetry is shown as unknown.

The desktop previously exited its connection worker on the first host mismatch. It now retries handshakes, asks an older idle host to shut down through its existing guarded operation and rejects downgrade attempts. Active tasks and checks postpone upgrades. Rejected user operations are reported without replay. Runtime settings now distinguish missing connection data from an unavailable runtime.

Temporary isolated checks passed completion snapshot persistence, repeated completion notifications, branch capture, last-context selection, sparse usage updates, idle legacy-host replacement, busy-host refusal and connection-worker recovery without mutation replay. Verification sources and fixtures were removed. No authenticated provider turn or app visual testing was performed.

The supplied app PNG is preserved byte-for-byte. Both bundle configurations generate all standard macOS icon sizes and include AppIcon.icns. Cached SVG masks supply static dithered orange top-left and gray bottom-right window lighting without pointer hitboxes or a particle animation loop.

Final workspace Clippy with warnings denied, formatting, shell syntax and git diff --check passed. Both command entry points built ARM64 bundles with the supplied icon. Iconset verification found all ten standard representations. Release `20260909T224645Z-372257c-dirty` is installed at `/Applications/Kodem.app`; installed executables, plist, icon and build metadata match the release package, and strict signature verification passed. Debug `20260909T224542Z-372257c-dirty` is at `target/debug/Kodem Debug.app`. The previous release is preserved at `/Applications/Kodem.previous.20260910-004646-81702.app`. Running processes were not stopped or launched. The existing `block 0.1.6` dependency still emits Cargo's future-incompatibility notice; both builds succeeded. No commit or push was made.
