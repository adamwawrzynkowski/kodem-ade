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


## Picker, typography and density corrections

Access now includes AI auto-approve using the pinned runtime's native `approvalsReviewer: auto_review`. The option preserves the workspace sandbox and on-request policy. Other modes explicitly reset the reviewer to user on thread start/resume and every new turn. Protocol 5 prevents an older host from accepting this new access enum. The locally generated 0.153.4 schema confirms the field and reviewer values; no authenticated approval turn was run.

The reasoning slider paints its track, fill, markers and circular thumb from one coordinate system shared with pointer input. The fill extends under the thumb and reaches the final endpoint. Model, effort and access menus omit redundant headings. Usage indicators now draw clockwise sectors and arcs rather than bottom-filled rectangles. Temporary numerical checks passed all slider stops at five widths, quarter/half/three-quarter circular geometry, and AI reviewer/sandbox mappings. Temporary test sources were removed.

Task cards have tighter spacing, subdued project glyph colors, darker surfaces, gradient shading and stronger selected edges. The earlier sidebar, raised and border colors are restored. IBM Plex Sans replaces the system UI font, picker/sidebar captions are more readable, the prompt text is slightly larger, and Kodem uses 16px bold Manrope. Code stays in JetBrains Mono.

Progressive native blur now lives inside the rounded prompt field with its input and toolbar. The surrounding floating container and context tray have no backdrop filter. The duplicate task-title chip, diff back button and diff staging button are removed; close, refresh and split controls remain. No application was launched, controlled or visually tested.

Final Clippy with warnings denied, formatting, shell syntax and git diff --check passed. Debug build `20260909T230219Z-5e3d101-dirty` is in `target/debug/Kodem Debug.app`. Release `20260909T230335Z-5e3d101-dirty` is installed at `/Applications/Kodem.app`; desktop, host, plist, icon and build metadata match the release package, and strict signature verification passed. The previous bundle is retained at `/Applications/Kodem.previous.20260910-010336-988.app`. No visual or live AI approval test was run. No commit or push was made.


## Darker task surfaces, startup transport and generated titles

The wordmark now embeds a static Manrope ExtraBold weight-800 instance. The original variable font defaulted to weight 200 in the native renderer despite the requested bold style. Dark task cards use opaque charcoal gradients, inset shading and shadows; selection uses a slightly brighter surface without an accent border. Warm dithering paints behind sidebar content. Gray bottom-right dithering paints above the workspace background and file/diff surfaces, before menus, so populated Changes views cannot cover it.

Scoped local provider logs showed initial WebSocket HTTP 503 failures followed by a successful HTTP fallback. The pinned runtime rejects overriding its reserved built-in OpenAI provider, so Kodem registers a process-local HTTP provider alias and uses it for OpenAI thread start/resume. Custom provider choices and explicit OpenAI base URLs remain respected. No global Codex configuration or credentials were modified. Retryable error notifications update one live reconnect activity; terminal errors still persist as transcript errors. Protocol 6 includes the new activity variant.

New tasks keep their placeholder while a separate ephemeral, restricted model request generates a short title from the first request and attachment/context labels. The request has a JSON output schema, disables shell, code-mode, app, plugin, MCP and delegation tools, and cannot delay or replay the main conversation turn. A persisted pending flag prevents generated results from replacing a manual rename. Existing task titles are preserved.

A temporary isolated host check passed repeated retry updates, absence of persisted retry errors, clearing on resumed stream events, and preservation of terminal errors. An authenticated nonvisual title request through the actual new runtime adapter returned “Fix Reconnect Notices and Darken Task Cards” successfully. Temporary verification sources and executable examples were removed. This does not constitute app visual or pointer-interaction testing.

Final workspace Clippy with warnings denied, formatting and git diff --check passed. Debug build `20260909T232120Z-5e3d101-dirty` is at `target/debug/Kodem Debug.app`. A concurrent release command held the build lock when this session attempted release; that command then completed successfully and installed release `20260909T232208Z-5e3d101-dirty` at `/Applications/Kodem.app`. Installed desktop, host, plist, icon, build metadata and third-party notices match the release package byte-for-byte. Strict signature verification passed. The previous release is retained at `/Applications/Kodem.previous.20260910-012209-17102.app`. No app was launched, controlled or visually tested. No commit or push was made.


## Thread cards and drifting chat texture

Thread cards use a single opaque surface from the app's existing dark/light palette, rounded corners, a restrained shared edge and a shallow contact shadow. The project, title and metadata share that surface. Internal colored bands and full-card gradients are removed. Titles use medium weight and selection changes the surface tone without an accent outline. Existing task selection, saved-diff actions, live statuses and provider tooltips are preserved.

A deterministic 256px SVG dither tile covers the chat background at low opacity. The native animation moves the repeated tile by one full period over four minutes, so the repeat has no jump. The cached raster is reused across tiles; the texture has no pointer handlers and paints before conversation content. Reduce Motion renders a static tile layer. Cards and message surfaces remain opaque over the background.

Workspace Clippy with warnings denied and source formatting passed. No app visual testing, control or screenshot capture was performed. Computer History status remained unavailable with error -1743; the supplied screenshots and current source were used.

Final debug `20260909T232756Z-5e3d101-dirty` and release `20260909T232841Z-5e3d101-dirty` builds passed. The release is installed at `/Applications/Kodem.app`; installed desktop/host executables, plist, icon and build metadata match the release package. Strict signature verification, formatting and git diff --check passed. The SVG asset parses successfully. The previous release is preserved at `/Applications/Kodem.previous.20260910-012842-24501.app`. Visual appearance and live animation were not tested, per the user's restriction. No commit or push was made.


## Chat texture visibility

Increased chat dither opacity from 4% to 14% in dark mode and from 3% to 8% in light mode. The tile, coverage, four-minute motion loop and Reduce Motion behavior are unchanged. Verification is limited to source checks and compilation; no app visual testing was performed.

Debug `20260909T232941Z-5e3d101-dirty` and release `20260909T233024Z-5e3d101-dirty` compiled and passed package signature checks. Installed release executables and build metadata match the package. The build detected an older loaded app/host and reported that reopening is required. The previous bundle remains at `/Applications/Kodem.previous.20260910-013024-26448.app`. Formatting and git diff --check passed. No app control, visual tests, commit or push.


## Session restoration, smooth motion and release replacement

Chat dithering now moves through the GPU sprite transformation instead of moving pixel-snapped SVG bounds. A temporary numerical check reproduced repeated positions in 57 of 59 frame transitions at Retina scale with the previous path. The replacement retains fractional device-pixel movement at both 1x and 2x, without changing the tile, opacity, loop duration or Reduce Motion behavior.

Window placement is saved atomically by the desktop and restored before the host connects, including the normal restore rectangle for maximized/fullscreen windows. Saved placement is recovered onto a connected display. Sidebar/review widths, panel visibility and split-diff choice are restored; selecting a task no longer silently changes the saved review-panel visibility on narrow windows. Quit drains queued settings requests. Accepted model/effort/access changes persist task choices and defaults for new threads together, and stale desktop settings cannot overwrite those defaults. Model discovery begins at workspace startup, and accepted picker changes immediately reload the task rather than waiting for the next poll.

The virtual transcript list now owns one centered 940px column, with the same horizontal padding as the composer. Git capture already honors repository, nested, local and configured global ignore rules; tracked files remain visible as in Git itself. A temporary repository verified exclusions, negation and tracked ignored files in working-tree, unstaged and task-baseline comparisons. No files were untracked or deleted from the user's repository.

Release installation keeps a hidden rollback bundle only until the replacement passes verification, then removes it. Successful installs also remove legacy generated timestamp/PID backups after checking the exact filename shape and Kodem bundle identifier. Unrelated applications and symlinks are excluded. A running app is not stopped; reopening is still required to load a newly installed binary.

Temporary nonvisual checks passed database reopen, task/default preferences, stale layout saves, independent access defaults, older settings defaults, all three saved window modes, malformed placement handling, GPU fractional translation, and Git ignore behavior. Temporary verification sources and fixture directories were removed. App visual testing remains prohibited; appearance and pointer interactions have not been tested.

Final workspace Clippy with warnings denied, formatting, shell syntax and git diff --check passed. Debug build `20260909T235227Z-5e3d101-dirty` and release `20260909T235311Z-5e3d101-dirty` compiled and passed package verification. The release is installed at `/Applications/Kodem.app`; desktop, host, plist, icon and build metadata match the release package byte-for-byte, and strict signature verification passed. No generated legacy backups, rollback copies or staging copies remain in Applications. No app was launched, controlled or visually tested. No commit or push was made.


## Composer-contained context and skill chips

Draft suggestions, files and skills now appear inside the rounded prompt container, above the text editor. The container grows with selected context and reserves its measured height in the transcript; the context region scrolls after 160px. Captured suggestions still show only their title and source range. The outgoing queue remains above the prompt because those messages have already been submitted.

One rightmost control switches between Send while idle and Stop during active work. Empty Send is disabled and visually subdued, with the same content predicate used by keyboard submission. Enter continues to queue or steer active-turn input, and the footer reflects that choice.

The skill catalog adds a typed skill attachment instead of editing prompt text. It uses the existing bounded import, persistence, removal, queue and restore flows. Skill chips show the skill name and type; the host keeps an immutable file copy and the original skill path for resolving sibling resources. Ordinary uploads of a file named SKILL.md remain ordinary file attachments. Protocol 8 carries the typed import source and skill attachment metadata.

A temporary nonvisual test passed explicit skill versus ordinary file importing, frozen content after source changes, model-payload skill invocation, queue persistence across database reopen, restoration without adding editor text, invalid skill rejection, and deletion of imported copies without deleting the source. Verification code, executable and fixture files were removed afterward. Workspace Clippy with warnings denied passed. Computer History status remained unavailable with error -1743; the current code and supplied screenshot were used. No app was launched, controlled or visually tested.

Debug `20260910T000118Z-125448d-dirty` and release `20260910T000209Z-125448d-dirty` builds passed. Release is installed at `/Applications/Kodem.app`. Installed executables, plist, icon and build metadata match the release package; strict signature verification passed. There are no generated backup, staging or rollback copies in Applications. Formatting and git diff --check passed. No app visual testing, commit or push was performed.


## Workspaces: Agentic Coding, Chat and Review

Recorded on 12 October 2026. A rail of three icons on the window's leading edge swaps the window between workspaces. Tasks have a kind (`TaskKind`); the thread feed, keyboard navigation and the start screen follow the open workspace, and a task always opens in the workspace of its kind. Each workspace keeps its open task and the draft of its start screen. Protocol 27 adds `CreateChat`, `ReviewTargets`, `CreateReview`, the `base` commit of `CreateTask`, and the saved workspace in `Settings.mode`. The task kinds and their session rules are in [provider integration](providers.md#task-kinds-and-session-rules); the rail's visual rules are in [dithered interface](../design/dithered-interface.md).

The collapsed sidebar no longer has a rail of its own: it closes completely, and the workspace rail holds its buttons. The title bar's leading section and the sidebar's resize handle account for the rail's width.

Checks that ran:

- Workspace `cargo check`, `cargo fmt --all -- --check` and Clippy. Clippy reports the six warnings that the code had before this change and no new one.
- A temporary unit test, removed afterward, ran the review helpers against a temporary repository with a bare remote: branch listing without the default branch and without a duplicate remote-tracking branch, the uncommitted-changes flag, a branch review, a pull request review fetched through `refs/pull/<n>/head`, removal of the temporary references, an unchanged checkout and index in the project, a fix worktree that starts at the reviewed commit, worktree removal, and refusal of an unknown branch, a branch without new commits, a foreign remote and a base name that is an option.
- Claude Code 2.1.296 completed its control handshake, without a model turn, with `--append-system-prompt`, `--allowedTools` and the `--disallowedTools` rules that a review uses.
- The JSON schema of Codex 0.153.4's app-server lists `developerInstructions` on `thread/start` and `thread/resume`, and `web_search` with the value `live` in its configuration.

Not checked: the appearance and pointer behavior of the rail and the Review start screen, because app visual testing is prohibited; a real provider turn of a chat or a review; `gh pr list` against a repository on GitHub (in the test repository it reported that no remote points to GitHub, which the picker shows); and whether Codex accepts a thread in a directory that is not a Git repository. No commit or push was made.

## Approval rule in a review, remembered for each model

Recorded on 12 October 2026. A review no longer pins its access. Its composer has the approval menu of Agentic Coding beside **READ ONLY**, and the provider session runs with the rule of the task; `provider::pin_review_access` and the refusal of access edits for a review are removed. Claude Code keeps the denied tools of a review. Codex has no such list: only the Read only access enforces a review in its sandbox. A review has no Build or Plan mode, and the desktop never starts one in a plan mode.

The approval rule is remembered for each model in `Settings.access_choices` and applies to new tasks with that model in every workspace and project. Only an explicit choice records it: the new request `SetAccessChoice` for a draft or for Settings, and an access edit of a task of any kind. Protocol 28 adds the request and the saved field. No rule is recorded for a model until one is chosen after this change, so until then a new task starts with the project default or the last used options as before.

Checked with `cargo check`, `cargo fmt --all -- --check` and Clippy, which reports the six warnings that the code had before this change and no new one. A temporary unit test, removed afterward, checked the store: an ordinary save records no rule, an explicit choice records it for its model only, a plan mode and a missing model are not recorded, a window layout save keeps the rules, older settings parse, and a retired model loses its rule.

Not checked: the appearance of the composer, because app visual testing is prohibited, and a real provider turn of a review under another approval rule. No commit or push was made.

## Folder attachments, alerts while Kodem is active, and new sounds

Recorded on 12 October 2026. A dropped, pasted or picked folder is now an attachment. The host does not copy it: `AttachmentKind::Folder` keeps the resolved path of the folder, with no size, and the prompt tells the agent to read it in place. The paperclip picker accepts folders. `attachments::discard` removes a private copy by the identity of its attachment, so it can never remove a path of the user. Files keep their private copies and their limits.

Alerts were delivered quietly while Kodem was the active application. macOS then asks the delegate of the application how to present the alert, and it calls the delegate on the main dispatch queue. The host posts the alerts, and its main thread ran the accept loop and never served that queue, so a question that went to the host got no answer. The host now runs on its own thread, with the stack size of the main thread, and the main thread calls `dispatch_main`. A panic or a result of the host thread ends the process. The sender records its choice in the `userInfo` of the notification, and the delegate of either process obeys it: banner, list and sound, or the list only. `NotificationSettings.while_focused` (on by default) is that choice for task alerts. A test notification always shows.

The five sounds keep their names and are new audio from `scripts/notification-sounds.py`, which also reports their levels. The old files peaked between -4 and -8 dBFS with fundamentals near 440 to 660 Hz. The new ones peak at -1 dBFS, their loudest 200 ms are between -8 and -12 dBFS (before: -12 to -20), and their fundamentals are between 780 and 2100 Hz. In Settings, choosing a sound plays it and **Play** plays the selected sound again. The desktop plays the embedded file with `NSSound`, so no notification is sent. Protocol 29 adds the folder kind and the new setting.

Checked with a locked `cargo build` of the workspace, `cargo fmt --all -- --check` and Clippy, which reports the six warnings that the code had before this change and no new one. Temporary tests, removed afterward, checked that a folder import makes no copy and resolves the path, that `discard` and a failed batch leave the folder and remove only private copies, that the prompt names the folder, that the foreground mark is read back as written and is absent by default, and that the host binary binds, lets a second starter exit, accepts `Shutdown` and exits with success from its thread. The level of each sound file was measured.

Not checked: whether macOS shows the banner while Kodem is active, the sound of the new files, and the appearance of Settings and of the folder card, because app visual testing is prohibited and no sound was played. The queue of the delegate was read from the disassembly of UserNotifications on this macOS version, not from Apple's documentation. The tracked licensing snapshot was stale before this change and was not refreshed; no external package was added. No commit or push was made.

## Shorter, softer notification sounds

Recorded on 12 October 2026. The user did not like any of the five sounds and asked for sounds that are pleasant and not too long. The sounds keep their names and files and are new audio from `scripts/notification-sounds.py`.

The earlier sounds were plain sine partials with fundamentals between 780 and 2100 Hz, attacks of 1.5 to 4 ms and a reverb of four combs. They stayed within 40 dB of their loudest moment for 0.47 to 1.64 s. The new ones have fundamentals between 440 and 1480 Hz, attacks of 4 to 8 ms, overtones that fade before the body of a note, and a room of eight damped combs with a tail of about 0.4 s. They are quiet after 0.37 to 0.78 s.

| Sound | Design | Length | Quiet after |
| --- | --- | --- | --- |
| Chime (default) | Two electric-piano notes that rise a fifth, D5 to A5 (`tine`) | 0.85 s | 0.74 s |
| Bell | One small bell on F#5 with a low hum and a slow shimmer | 0.95 s | 0.63 s |
| Glass | B5 and F#6 struck 45 ms apart | 0.60 s | 0.44 s |
| Pulse | Two rounded pops on E5, the second softer (`pop`) | 0.50 s | 0.38 s |
| Ascend | A quick A major arpeggio on a wooden bar, A4 to A5 | 0.90 s | 0.78 s |

The files are stereo: the dry sound is in the middle and only the tail of the room differs between the ears. The peak stays at -1 dBFS and the loudest 200 ms are between -10 and -12 dBFS, so the sounds are about as loud as before. `levels` now also reports when a sound becomes quiet.

Checked with `cargo check` of the workspace, which embeds the files, and `cargo fmt --all -- --check`. A temporary script, removed afterward, measured each file: no energy of note above 3 kHz (0.13% at most, before up to 5.7%), a largest step between two samples of 0.18 of full scale (before up to 0.39), and a start and an end at zero.

Not checked: the sound of the files. No sound was played, so whether they are pleasant is a judgment for the user. The designs follow measured properties only.

## Conversation typography and message actions

Recorded on 13 October 2026. The user reported that the conversation looked plain and messy, with two screenshots, and asked for a redesign that takes ideas from other agentic coding tools. The screenshots showed five causes, and each has one change.

| Cause | Change |
| --- | --- |
| A paragraph or a list item with inline code started three points to the right of the others. `TextInput` moved its text by `INLINE_PADDING` when it held a plate. | `bubble_padding` is removed. All prose starts on one edge. A plate that continues on the next line hangs three points into the margin; every container of prose has at least 11 points of padding there. |
| Every block had 8 points above it, so a title, its section and the next section were one even column. | `block_gap` in `ui/message_content.rs` gives the space between two blocks from their kinds: 6 after a title or before the list that a sentence introduces, 10 between paragraphs and lists, 12 around code, tables and quotations, 18 or 24 above a title. The content of a list item is tighter. |
| Agents title sections with a paragraph of bold text, which looked the same as a bold start of a list item. | `promote_titles` in `markdown.rs` turns such a paragraph into a heading of level 3 when it is at most 72 characters, is not a sentence, and a block that is not a heading follows it. Headings now use their line height of 1.3; `message_prose` set 1.55 over it before. |
| Every span of inline code had a plate and a dotted line below it, so a line with three paths was mostly dotted lines. | Only a file reference keeps the dotted line, in the accent. Plain code keeps its plate. |
| Every reply had a copy icon below it, also a one-line remark between two runs of tool calls. | The reply that ends a turn keeps the row. A reply that work follows shows the icon under the pointer, in the right padding of its row. A message of the user has 18 points above it. |

A list with several paragraphs in an item has 10 points between its items and other lists have 5. A list inside a list has open bullets. When rows are added, the row before them is measured again, because a reply loses its row of actions when work follows it.

The ideas from other tools are from general knowledge of the Codex app, Claude Code, Cursor and the agent panel of Zed: less emphasis on steps than on the final answer, actions that do not take room on every message, quiet inline code, and space that groups a section. Four web searches on 13 October 2026 found release notes and forum threads but no description of these layouts, so no source is cited for them.

Checked with a locked `cargo check` of the workspace, `cargo fmt --all -- --check` and Clippy, which reports the six warnings that the code had before this change and no new one. Two temporary tests, removed afterward, checked which paragraphs of bold text become titles and which reply ends a turn.

Not checked: the appearance of any of this, because app visual testing is prohibited. The sizes of the gaps, the place of the copy icon under the pointer and the look of a hanging plate are untested judgments. The chat background still draws its dots behind the text in the lower part of the conversation; it was not changed. No commit or push was made.

## Orchestration, phase 0: streamed text is written at a bounded rate

Recorded on 13 October 2026. This is the first phase of the plan that adopts the lifecycle model of T3 Code's Orchestrator V2 (pure decision, one transactional commit, runs, durable requests, a change cursor). The user chose to keep the task and message rows as the stored truth, without an event log.

Before this change each streamed delta stored the whole row of its item under the host lock, with `synchronous=FULL`. A long answer or a long command output therefore cost one growing write for each delta.

`provider::Streamed` holds the open items of one session and the set of items whose text changed. Changed rows are written together at most once in 120 ms (`STREAM_WRITE_INTERVAL`). Three rules keep the stored rows correct:

| Rule | Codex | Claude Code |
| --- | --- | --- |
| The first delta of an idle session is written at once. | `Streamed::changed` in `ingest` | `Streamed::changed` in `Transcript::stream` |
| Every event that is not a delta sees current rows. | `ingest` writes pending rows first (`appends_text`) | `Owner::event` calls `Transcript::write_pending` for every event that is not a `stream_event` |
| Text that no later event follows still appears. | The loop of `run` writes rows that are due after each poll | The loop of `run` calls `Transcript::write_due` |

The Codex session loop and its event callback share the open items through `Rc<RefCell<..>>`. Both run on the owner thread and never at the same time.

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six findings that the code had before this change (four collapsible `if`, two needless borrows) and no new one. Two temporary tests, removed afterward, ran the real `ingest` of each driver against a store in a temporary directory:

| Driver | Input | Result |
| --- | --- | --- |
| Codex | 500 `item/commandExecution/outputDelta` events in 803 ms | 7 row writes in place of 500; the final row holds all text after the loop write; a pending row is written before the next event that is not a delta |
| Claude Code | 300 `text_delta` events in 395 ms | 4 row writes in place of 300; `write_pending` stores all text and the row stays open; `content_block_stop` stores one complete row |

Not checked: a live turn of either provider, and the conversation while text streams, because app control is prohibited. The desktop reads rows every 600 ms, so a delay of at most 160 ms (the interval and one poll of 40 ms) is below what it shows. A session owner that fails between two writes loses at most that much streamed text of open items; reconciliation restores items from the provider history as before. No commit or push was made.

## Orchestration, phase 1: lifecycle decisions for the Codex main turn

Recorded on 13 October 2026. `kodem-host/src/orchestrator.rs` now decides the lifecycle of a task's main turn. [Provider integration](providers.md#lifecycle-decisions) describes the parts.

| Before | Now |
| --- | --- |
| `provider::claim_next` read the queue, failed expired steering messages, checked options and the checkout, and claimed a message in separate writes. | `Command::Claim`: `decide` returns `SteerExpired`, then `SteerClaimed`, `TurnClaimed` or `QueuePaused`. One `Store::commit` stores them. |
| `codex::dispatch_next` confirmed or failed a delivery and changed the task itself. | `Signal::Delivered` and `Signal::DeliveryFailed`. |
| `codex::ingest` set the task to `Running` for `turn/started`. | `Signal::TurnStarted`, decided under the lock that `ingest` already holds. |
| `codex_subagents::complete_main` and `finish_if_ready` held the outcome and released it. | `Signal::TurnEnded` and `Signal::ChildrenChanged`; both functions are removed. |
| `provider::complete_turn` released the final answer, deleted its staging rows and saved the task in separate writes, and wrote the snapshot file under the host lock. | `orchestrator::observe` captures the checkout and writes the snapshot file without the lock (`Store::write_snapshot`). `Command::Finalize` stores the rest in one transaction. |

`Store::claim_outgoing` is removed; `Store::commit` holds its statements. The check of the turn identity for `turn/completed` stays in the Codex driver, before the check for a compaction, so the order of those two checks did not change. Claude Code calls the same `claim_next` and `complete_turn` as before.

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. Three temporary tests, removed afterward:

| Check | Cases and result |
| --- | --- |
| `decide` and `apply`, no store | A task at rest claims its oldest unbound message: `Starting`, `updated` and `last_started` set to the time of the claim, `branch_merged` and `error` cleared. `Delivered` after `turn/started` changes nothing; `Delivered` first sets `Running` and the turn and leaves `updated`. `turn/completed` before `Delivered`: the outcome is released, `Finalize` sets `Completed`, and the late `Delivered` leaves `Completed`. A turn that ends while a subagent works is held with the task `Running`; `ChildrenChanged` releases it only after the subagent stopped, and releases nothing without a held outcome. A steering message of an earlier turn expires while the one of the running turn is claimed and an unbound message waits. A turn that is still starting accepts nothing. A busy checkout pauses the queue as `Interrupted` and sends nothing. Unsupported options refuse the claim after the expired message was recorded. Nothing is claimed or expired while a delivery is uncertain or the task is `DeliveryUnknown`, `Failed` or `Interrupted`. A refused steering message leaves the turn; a refused start gives `Failed`; an uncertain delivery gives `DeliveryUnknown`. All passed. |
| `Store::commit` and the whole loop on a store in a temporary directory | A claim of a message above the 2 MiB transcript limit fails and leaves the task row and both ledger rows unchanged. Then claim, `TurnStarted`, `Delivered`, `TurnEnded`: the ledger row is `pending` and then acknowledged with the turn, the staged final answer moves into the transcript after the user message, the staging transcript is empty, and activities are cleared. A coding task outside a repository still completes through `complete_turn`; its review holds the capture error. Passed. |
| The real `codex::ingest` | `turn/started` gives `Running`. A `turn/completed` of another turn is ignored. The main turn ends while a subagent works: the outcome is held and the final answer stays staged. The subagent's `turn/completed` completes the task and publishes the answer. Passed. |

Not checked: a live Codex or Claude Code turn, a host-requested compaction, and the alerts, because app control and authenticated turns are prohibited here. The Claude Code driver was not changed in this phase, so its use of the two shared functions is covered only by the store check above. No commit or push was made.

## Orchestration, phase 2: the window watches its conversation (protocol 30)

Recorded on 13 October 2026. Before this change the desktop asked every 600 ms for the whole workspace and for the newest 100 messages of the open task, and merged them by `seq`. A change to a row older than those 100 never reached an open window.

`Request::Watch { task_id, after }` waits on the host until the task differs from what the cursor `after` describes. The answer is a `SubscriptionStart`, the shape that [architecture](../design/architecture.md) names:

| Answer | When | Transcript rows in it |
| --- | --- | --- |
| `Snapshot` | No cursor, or a cursor of another host process | The newest page |
| `Replay` | The cursor is known | Only the rows written after it, in `seq` order; none when only other state changed or the wait of 10 s ended |
| `ReplayGap` | The cursor is older than the 1024 writes the store retains, or more than 100 rows or 4 MiB changed | The newest page |

A `Cursor` holds the identity of the host process (`Store::revision`), the revision of its newest transcript write, and a digest of the rest of the view: the task, the outbox, activities and requests.

| Part | Where | Rule |
| --- | --- | --- |
| Transcript changes | `Store::message` records each write; `Store::changed_messages` reads them | A row that is written again while it streams keeps one entry. The list lives in memory, so it adds no table and no schema version. |
| Other changes | `watch` in `kodem-host/src/main.rs` | A waiting request looks at the task every 50 ms and compares the digest. No code that changes a task, an activity or a request has to notify anything, so a change cannot be missed by a forgotten notification. The host lock is held only for each look. |
| Order of views | `TaskView.epoch` and `TaskView.serial` | The host numbers the views it produces. The watcher and the explicit requests use separate connections, so `merge_transcript` in the desktop lets the view with the larger number win, and an older answer only adds rows that the window does not hold. |
| Desktop | `Watcher` in `kodem-desktop/src/bridge.rs` | One thread and connection per followed task, apart from the ordered worker, so Send and Stop never wait for it. It asks again at most every 200 ms. A request in flight cannot be cancelled: when the window turns to another task, the old thread ends when its request returns, after at most 10 s, and its answer is dropped. |
| Start and fallback | `Command::Poll(.., viewed)` | The poll still sends `Hello` every 600 ms, which also drives settlement and usage refresh. It requests the task itself only while the window does not hold its first page. From then on the watcher follows the task. Changed rows that arrive while the window has no page are dropped. |

This plan first named a condition variable and a notification from each place that changes a task. The digest replaced it, because about 80 places change a task directly and a place without the notification would leave the window stale for the whole wait.

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. Three temporary tests, removed afterward:

| Check | Cases and result |
| --- | --- |
| `watch` on a store in a temporary directory, 150 stored rows | No cursor: the newest 100 rows at once. A row older than that page changes after 150 ms: the answer holds exactly that row and arrives 150 to 400 ms after the request. An activity that exists only in host memory, and a change of the task row, each wake the watcher with no rows. 300 writes of one row and one new row: two rows, in order. 120 changed rows: `ReplayGap` with the newest page. 1100 writes of another task: `ReplayGap`. A cursor of another host: `Snapshot`. A write of another task does not wake the watcher; it answers no change after 10 s. A waiting watcher ends with an error when the host shuts down. An unknown task is an error. All passed. |
| `Watcher` against a real `kodem-host` process on a scratch data directory, without `Hello`, so no account or provider was read | First answer without a cursor. No answer while nothing changes. A draft edit arrives in under 400 ms with no rows. The row that `RunCheck` writes arrives as the only row of its answer. Following the same task again starts no new watcher. After the window turns away, a change is not delivered. After it turns back, the first answer is whole again. A stopped watcher stays quiet, and a host that shut down is not reported as an error. Passed. The scratch host and its directory were removed. |
| `merge_transcript`, no host | Changed rows join the page and bring the newer state. An answer with a smaller number does not put a shorter row or the older state back, and still adds a row the window lacks. An outdated older page extends the history and keeps the state. A view of a restarted host is not outdated. Passed. |

The first run of the bridge check found only an error of the check itself. The check of `watch` showed that an upsert that updates a row still advances SQLite's `AUTOINCREMENT` counter, so `seq` values of new rows have gaps. Nothing depends on `seq` being dense.

Not checked: the window itself. Whether the conversation follows a running task smoothly, whether scrolling and the follow-latest behaviour are unchanged at five updates a second in place of 1.6, and the change from one task to another, need a run by the user, because app control is prohibited. An older host is replaced through the existing protocol check: a desktop of protocol 30 shuts down an idle host of protocol 29 and starts its own. No commit or push was made.

## Orchestration, phase 3: runs (protocol 31)

Recorded on 13 October 2026. A task had no record of its turns: only `Task.active_turn`, and the desktop read turn boundaries from the order of messages. `Run` in `kodem-core/src/model.rs` is now one counted turn of a task, from the claim of its first message to its published outcome.

| Rule | Where |
| --- | --- |
| A claimed message starts a run (`RunOrigin::User`). A message that steers the running turn joins that run. | `orchestrator::apply_run` for `TurnClaimed` and `SteerClaimed` |
| A provider turn that nobody claimed starts a run of its own (`RunOrigin::Continuation`). | `apply_run` for `TurnStarted` without an open run |
| The provider turn of a compaction that the host asked for is not a run. | `Facts.compacting` |
| A run ends with the published outcome, or as `Failed` when its start was refused. An uncertain delivery leaves it open as `DeliveryUnknown`, also through `Store::recover`, because reconciliation can still prove the outcome. | `apply_run` for `Finalized` and `DeliveryFailed` |
| An open run that a new claim finds ends without an outcome of its own. | `begin_run` |

Runs are rows of a new table, `runs(id, task_id, ordinal, json)`, created with `CREATE TABLE IF NOT EXISTS`. The schema version did not change, so an older binary still opens the database and ignores the table. `Store::commit` stores the runs of a step with the task and the ledger. `TaskView.runs` carries them to the desktop, and they are part of the digest that wakes a watcher.

This phase differs from the plan in two points, both stated to the user before the work. A message has no stored `run_id`: `Run.first_seq` is the `seq` of the first transcript row of the run, and the rows of the task's own transcript from that row up to the first row of the next run belong to it. The store sets it when it first saves the run: the row of the submission that started it, which can be older than the claim when the message was accepted at rest, or the next row that the store will write. The desktop's grouping of work is unchanged: it shows the stretch between two messages of the user, which is a rule of presentation, and a steering message would otherwise change how the replies around it look. Nothing reads `TaskView.runs` in the desktop yet; checkpoints and transfers will.

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. One temporary test, removed afterward, drove the real orchestrator against a store in a temporary directory: a message accepted at rest gives a run whose first row is that message; `turn/started` and the acknowledgement record the provider turn; a steering message joins the run, which then holds two submissions; the outcome ends it as `Completed`. Of two queued messages the second is recorded only at its claim, and the three runs have rising ordinals and first rows equal to their submission rows. A turn that nobody claimed gives a `Continuation` run whose first row is the next row written. A compaction gives no run. A refused start ends its run as `Failed`. A run with an unconfirmed delivery stays open through `Store::recover` and ends with the outcome that arrives later. An open run ends as `Interrupted` when the next claim starts a run. The task view carries the runs and deleting the task removes them. All passed. The first two runs of the check failed on steps that the check itself had left out.

Not checked: a live turn of either provider. Claude Code's own continuation after a background agent does not start a run yet, because that driver reports no `TurnStarted` signal before phase 4. No commit or push was made.

## Orchestration, phase 4: Claude Code reports lifecycle signals

Recorded on 13 October 2026. The Claude Code driver changed the task itself at four places. Each is now a signal to the orchestrator, described in [provider integration](providers.md#lifecycle-decisions).

| Place in `providers/claude.rs` | Before | Now |
| --- | --- | --- |
| `send`, a message that starts a turn | Set `Running` and a local turn identity | `Signal::TurnStarted` |
| `send`, a message that cannot be built | `fail_outgoing`, then `complete_turn` as `Failed`, which also captured the checkout | `Signal::DeliveryFailed` without uncertainty. No capture, because no turn ran. |
| `receipt`, the echo of a message identity | `confirm_submission` | `Signal::Delivered` |
| `follow_up`, the CLI continues by itself | Set `Running`, a local turn identity, cleared the error | `Signal::TurnStarted`, which starts a `Continuation` run when no run is open |
| `settle`, the outcome while an agent works | Wrote `pending_completion` and `Running` | `Signal::TurnEnded`; the orchestrator holds the outcome |
| `settle`, the outcome when nothing works | `complete_turn` | `Signal::TurnEnded`; the orchestrator releases, captures and finalizes |

Two rules of the orchestrator changed with this, for both providers. `TurnStarted` clears `Task.error`, because an error belongs to the turn before it; the Claude Code driver did that for a continuation and the Codex driver did not. A held outcome withdraws only the requests of the main provider conversation, and a request that names no conversation is kept, so a Claude Code agent's request stays answerable while its outcome is held.

`provider::complete_turn` stays for Claude Code's reconciliation, which publishes an outcome it read from the CLI's stored session. The states of subagents are still written by both drivers; the plan named `ChildStarted` and `ChildEnded` signals for them, and they were left out because the rule they protect, that only the main turn completes a task, is already decided in one place.

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. One temporary test, removed afterward, sent the signals in the order the driver sends them, against a store in a temporary directory: a continuation after a failed turn runs, clears the error and is a `Continuation` run; a written message starts the turn and the echo only records the receipt; a steering message joins the run; a result while an agent works is held with the task `Running`, the agent's request stays, and the outcome that the user's stop changed is the one published; a message that cannot be built fails its start, leaves the message recoverable and not uncertain, and ends its run as `Failed`. All passed.

Not checked: the driver itself. Which signal it sends at which moment is verified by reading only, because a turn needs the CLI and an authenticated model call. A run by the user of one Claude Code turn, one with a steering message and one with a background agent covers it. No commit or push was made.

## Orchestration, phase 5: requests are read through one interface

Recorded on 13 October 2026. The desktop and the host's request handler read a pending request by the provider's wire fields: `request.method.ends_with("requestApproval")` and `request.params["availableDecisions"]`, `["decisionLabels"]`, `["reason"]`, `["command"]`, `["tool"]`, `["plan"]`, `["planFilePath"]`, `["questions"]` and `["threadId"]`, at 18 places in `ui/questions.rs`, `ui/subagents.rs` and `kodem-host/src/main.rs`.

`InputRequest` in `kodem-core/src/model.rs` now has one method for each: `is_approval`, `conversation`, `offered_decisions`, `decisions`, `decision_label`, `reason`, `command`, `tool`, `plan`, `plan_path`, `questions` and `details`. Those 18 places call them, and no file outside the two drivers reads `method` or `params`. `decisions` holds the fallback of accept, decline and cancel that two places in the desktop each wrote out. Every method returns what the expression it replaces returned, so nothing that a user sees changed. The wire format did not change, so the protocol number stays 31.

This is less than the plan named for this phase, on purpose:

| Planned | Done | Reason |
| --- | --- | --- |
| A new typed request stored in `Task.requests`, with `Host.requests` removed | No | Neither provider can answer a request after its process is gone: Codex needs the live JSON-RPC identity, Claude Code the pending control request. A stored request could only be shown as expired. After a host restart the task already says that the host stopped before completion was confirmed. The change would have rewritten how both drivers ask and how the user approves a command, which cannot be checked here without live turns. |
| `ResponseCapability` and `Store::recover` marking requests | No | It follows from the row above. |
| `rollback`, `fork` and `delegation` flags on `RuntimeInfo` | Not yet | Each flag is added in the phase that reads it. |
| The desktop stops reading provider wire fields | Yes | As described above. A question is still a JSON object with `id`, `question` and `options`. |

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. A search finds no `params[` and no `.method` in `kodem-desktop` or in `kodem-host/src/main.rs`. No temporary test was written, because each method is the expression it replaces.

Not checked: the approval and question panels on screen. No commit or push was made.

## Orchestration, phase 6: checkpoints and undo (protocol 32)

Recorded on 13 October 2026. A coding task now keeps the files of its checkout as each turn left them, and the composer command `/undo` restores them to before the last turn.

| Part | Where | Rule |
| --- | --- | --- |
| Checkpoint | `kodem-core/src/git/checkpoint.rs` | `capture` builds a tree of every file that Git does not ignore, tracked or not, in a copy of the index, and commits it under `refs/kodem/checkpoints/<task>/<name>`. The user's index, branch and commits do not change. The copy keeps Git's record of unchanged files, so only changed files are read. |
| When | `orchestrator::observe` and `save_baseline` | After each turn of a `Coding` task, under the ordinal of its run (`Run.checkpoint`), without the host lock. Before the first recorded turn, the state before it is saved as `0`. A failed checkpoint does not fail the turn; it only means that the turn after it cannot be undone. |
| Restore | `checkpoint::restore` | First saves the present state as `restore-<time>`, so the restore can be undone by hand. Then removes the files that did not exist in the checkpoint and makes all others equal to it. A commit made after the checkpoint stays on the branch, and its changes show as undone in the working tree. |
| Undo | `Request::UndoTurn`, `provider::undo_turn` | Restores the checkpoint of the last turn before it that is not undone, or `0`. Marks the run as undone (`Run.undone`); its rows stay in the transcript and the desktop dims them. Refused while a turn or a compaction runs, while a delivery is uncertain, while messages are queued, and while another task writes to the checkout. |
| Conversation | `Task.handoff`, `provider::handoff_context` | The provider conversation still holds the undone turn. The task therefore leaves it: the session owner is closed, `provider_thread` is cleared, and the next message starts a new conversation. That message carries the messages of the user and the replies of the turns that are not undone, newest kept up to 200 KiB, inside `<earlier-conversation>`, with the reason. The transcript keeps the message as the user wrote it and shows a "Context handoff" divider. |
| Removal | `discard_scratch` | Deleting the task deletes its checkpoint references. |

The plan named a rollback of the provider's own conversation (Codex `thread/rollback`, Claude Code resume at a message) before the files. Neither could be proven here without authenticated turns, and a provider that rolls back wrongly would leave the conversation and the files apart. A new conversation with the remaining messages as context works the same way for both providers and is the mechanism that a change of provider needs in phase 7. Its cost is that the agent loses its tool history and reasoning from the earlier turns.

Not part of a checkpoint: files that Git ignores, and other repositories that the checkout links to. A large file that Git does not ignore is stored again each time it changes.

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. Two temporary tests on real repositories in a temporary directory, removed afterward:

| Check | Cases and result |
| --- | --- |
| `checkpoint` | A repository without a commit can be saved. A checkpoint holds a modified, a staged, and an untracked file and no ignored file, and its parent is the current commit. Taking it leaves `HEAD`, the staged diff, the status and the branch list as they were, and no temporary index. After a turn that rewrites, deletes, adds an untracked file and commits another, restoring the first checkpoint gives exactly its tree: content back, deleted file back, both later files gone, the ignored file untouched, `HEAD` unchanged. The saved earlier state equals the second checkpoint, and restoring it brings the later state back. A name with a path, a missing checkpoint and a directory that is no repository are errors. Discarding one task's checkpoints leaves another's. Passed. |
| The whole flow through the orchestrator | Nothing can be undone before a turn ran. Two turns leave checkpoints `0`, `1` and `2`. Undo restores the files of turn 1, clears the provider conversation, marks run 2 as undone and writes a notice of at most 96 characters. The next message is sent with the first turn's request and answer as context, without the undone turn and without a copy of itself; the transcript holds the message as written and the handoff divider. Only the rows of the undone turn written before the undo count as undone. A later message is sent as written. Three more undos go back to the last turn that still counts and then to the state before the first, where the working tree is clean; a fourth is refused. With every turn undone the next message has no context. A queued message and a running turn refuse the undo. Passed. |

Not checked: the close of a live provider session before the undo, a new provider conversation receiving the context in a real turn, the `/undo` entry in the composer menu, and the dimmed rows, because app control and authenticated turns are prohibited here. No commit or push was made.

## Orchestration, phase 7: change of provider, fork, and transfer rows (protocol 33)

Recorded on 13 October 2026. The user asked that the conversation shows when one agent hands its context to another, as a divider such as "Context handoff, Claude Opus 5.5 to GPT-6.1-Sol" with the logos of both.

| Part | Where | Rule |
| --- | --- | --- |
| Transfer row | `Message.transfer` (`Transfer`, `AgentRef`, `TransferKind`) | A `Notice` row that names the kind of transfer and both agents by provider and model. `text` holds the same statement in words (`provider::transfer_text`), so a client that does not draw the row still shows a divider. The host writes the row in the commit that claims the first message after the transfer, so a transfer never happens without its row. |
| The row on screen | `Shell::transfer_row` in `ui/conversation.rs` | The existing divider, with a label for the kind, then logo and model of the source in the accent color, an arrow, and logo and model of the receiver. The model is named as the provider's catalog names it. |
| Change of provider | `TaskEdit::Provider` in `kodem-host/src/main.rs` | Allowed for a started task between turns, with no message queued and no uncertain delivery. The session owner ends first (`provider::close_session`). `provider_thread` is cleared and `Task.handoff` is `Switched` from the agent that ran the turns. A second change before any message still hands over from that agent. The same provider with another account is refused here, because the account of a started task has rules of its own. |
| Context | `provider::handoff_context` | As for undo in phase 6, with a reason for each kind. The message that starts the turn is never inside its own context. |
| Fork | `Request::ForkTask`, composer command `/fork` | A new coding task in a worktree of its own. The worktree starts at the commit that the last checkpoint of the source was taken on and then receives the files of that checkpoint (`checkpoint::apply`), so work that was not committed stays uncommitted in the fork. `Task.handoff` is `Forked` and `Task.lineage` names the source and its turn. The first message of the fork carries the conversation of the source up to that turn. |
| Desktop | `ui/model_picker.rs` | The provider list is no longer locked for a started task. |

Less than the plan named, on purpose. A change back to an earlier provider does not resume that provider's old conversation with only the turns in between; every change starts a new conversation with the whole remaining conversation. A fork does not use a provider's own fork operation. Both would need provider behaviour that cannot be proven here, and the general path is correct for both providers; its cost is more context tokens on the first message. A fork starts from the last turn only, as undo does.

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. One temporary test, removed afterward, ran the real request handler against a store and a Git repository in a temporary directory: a queued message refuses the change of provider; without it the task has the new provider, no provider conversation and a waiting handoff from Codex; two more changes keep that source; the next message is sent with both earlier turns as context and the reason that names Codex; the transcript holds a row with the transfer and the text "Context handoff · Codex → Claude Code"; the runs record their providers; a later message is sent as written. A fork has its own worktree on the same commit, the modified file as an uncommitted change and the untracked file as untracked, a branch `kodem/…`, and the lineage of turn 4. A later turn of the source changes neither the files nor the context of the fork, whose first message carries the source's conversation up to turn 4 and writes a "Forked context" row with the source task. The fork saves its own checkpoints. A task that works refuses both a fork and a change of provider. All passed. During this phase one of my edits removed `resources` and `complete_turn` from `provider.rs`; the compiler reported it, both were restored, and the function list was compared with the committed file.

Not checked: the transfer row on screen, the provider list for a started task, `/fork` in the composer menu, a real turn of the new provider that receives the context, and the close of a live session before the change, because app control and authenticated turns are prohibited here. No commit or push was made.

## Orchestration, phase 8: subagents on another provider (protocol 34)

Recorded on 13 October 2026. The user asked a Claude Code task to "spawn subagent using gpt 6.1 sol" and the agent answered that its own Agent tool only accepts Claude models. That was correct before this phase. [Provider integration](providers.md#subagents-on-another-provider) describes what exists now.

| Part | Where |
| --- | --- |
| The tool server | `kodem-host/src/mcp.rs`; `main` runs it for the argument `mcp` before anything else |
| The operations | `kodem-host/src/delegation.rs`: `agents`, `delegate`, `status`, `cancel`, `stop_children` |
| Requests | `Request::Agents`, `Delegate`, `DelegateStatus`, `DelegateCancel`; `Response::Agents`, `Delegated` |
| Injection | `Session::open` in `providers/claude.rs`, `open` in `providers/codex.rs`, `Codex::start_with` in `kodem-core` |
| One writer for each checkout | `Task::shares_checkout` in `provider::submit`, in the claim facts of the orchestrator and in `undo_turn` |

Less than the plan named, on purpose. The parent's agent reads a result by calling a tool while its own turn runs; a result that arrives after that turn ended does not start a new turn of the parent. A delegated task cannot delegate, so Stop reaches one level. The plan's per-session token is left out: the host socket already accepts only the same user, and the server process is started by a provider that this user runs.

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. Three temporary tests, removed afterward:

| Check | Cases and result |
| --- | --- |
| Delegation on a store and a Git repository in a temporary directory, with two catalogs given to the host | The models of both providers are offered. "gpt 6.1 sol" and "GPT-6.1-Sol" find `gpt-6.1-sol` on Codex, "haiku" finds Claude Code, "luna" finds the one model that contains it, a provider alone gives its default, nothing gives the parent's provider, an unknown model is an error that lists the models, and a model of the other provider than the one named is an error. A reading task has the parent's checkout, read-only access, the first line of the task as its title, the lineage, and a "Delegated task · Opus 5.5 → GPT-6.1-Sol" row in both conversations. It is claimed while its parent works in the same checkout; a task that can write is still paused there; the parent is claimed while the reading task works. While it works the answer says so. After its turn the answer holds its reply, and the parent's conversation holds one "Subagent result · GPT-6.1-Sol → Opus 5.5" row also after a second read. Another task cannot read it, and it cannot delegate. A writing task has a worktree and branch of its own that hold the parent's uncommitted file, the parent's access, and no share in the parent's checkout, which is unchanged. Passed. |
| The tool server as a process, against a real `kodem-host` on a scratch data directory | `initialize` answers with the client's protocol version, the server name and instructions. `tools/list` names the four tools. A status of an unknown task, a delegation without a task text, an unknown provider and an unknown tool are tool results marked as errors with their reason. An unknown method is error -32601. `ping` answers. The server ends when its input closes. Passed. |
| The installed CLIs, Claude Code 2.1.296 and Codex 0.162.1, started with the configuration that Kodem passes | Claude Code reports the server `kodem` as connected with 4 tools through `mcp_status`. Codex lists `delegate_task`, `list_agents`, `task_cancel` and `task_status` through `mcpServerStatus/list`. No model request was made; both CLIs ran under the default account in a scratch directory. Passed. |

Not checked, because each needs an authenticated turn: that an agent calls `delegate_task` when asked for a subagent on another model, a delegated turn from start to result, whether Codex asks for approval before it calls a tool of this server, a reading Claude Code task being held to the blocked tools, and Stop reaching a working delegated task. The transfer rows and the delegated tasks in the task list are not checked on screen. No commit or push was made.

## Appearance: themes, typefaces, text sizes, interface scale and the light field (protocol 35)

Recorded on 14 October 2026. [Dithered interface](../design/dithered-interface.md) describes the look; this section records what changed and how it was checked.

| Part | Where |
| --- | --- |
| Saved choices | `kodem-core/src/model.rs`: `ThemeId`, `TextFont`, `CodeFont`, and `Settings.theme`, `text_font`, `code_font`, `code_font_size` and `ui_scale`. `Settings::from_json` turns the earlier light switch into Paper. `Store::write_settings` bounds the sizes and the scale |
| Palettes | `theme.rs`: one `Palette` for each of fourteen themes; `Theme::new` derives edges, tints and dither inks from it |
| Code colors | `syntax.rs`: a color theme built from eight colors for each classic theme; Graphite, Paper and both Solarized themes use the themes that syntect bundles |
| Scale and text sizes | `theme.rs`: `apply_metrics`, `px`, `text_px`, `code_px`, `sans`, `mono`. In `ui`, `px` is a length at the interface scale; a length from a measured layout or the pointer uses `gpui::px` |
| Settings page | `ui/settings.rs`: `theme_choices`, `text_font_choices`, `code_font_choices`, `scale_slider`. `Slider::on_release` applies the scale at the end of a drag |
| Chat background | `ui/backdrop.rs`: four blurred lights and a veil over the reading column |

The protocol number changes because a host from before this change would save settings without the new fields.

Less than a full zoom, on purpose. The title bar keeps its height and the room for the window controls, because macOS draws them at one size. Dot pitches of dithered textures and the flying chips of the effect layer keep their size. The bundle gained Inter, JetBrains Mono and Fira Code as static files from their release archives, with their licence texts and rows in `assets/THIRD_PARTY.md`. The pickers list the bundled typefaces, the system font, and the typefaces from a fixed list that this Mac has.

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. Temporary tests, removed afterward:

| Check | Cases and result |
| --- | --- |
| Settings | A saved light switch selects Paper, a dark one Graphite, and a saved theme wins over the switch. A store bounds the scale to 0.6–1.4, the text size to 12–22 and the code size to 10–20, and refuses a scale that is not a number |
| Palettes | Every theme, against its canvas, sidebar, raised and inset surfaces: text at 6.5:1 or more, muted text at 4.5:1 or more, subtle text at 3:1 or more in the twelve classic themes (Paper keeps its 2.9:1), and the accent and the four status colors at 3:1 or more. Graphite and Paper keep their earlier colors |
| Code colors | Every theme colors a Rust sample completely with four or more colors. Monokai gives the expected color to a comment, a declaration, a function name, a number, a string and a type |
| Scale | At 140% a length of 10 is 14 and text sizes stay; a text size of 21 and a code size of 18 scale their own text only |
| Chat background | All themes, five states and six window sizes over 160 moments each, with the blur computed as the shader does: behind messages the tint stays below 6% on dark themes and 4% on light ones, text loses at most 15% of its contrast and stays at 6.7:1 or more, muted text at 4.8:1 or more. Each light is short enough for the shader's four samples |

Not checked, because app control and screenshots are prohibited here: every screen at a scale other than 100%, the look of each theme and typeface, and the motion of the background.
