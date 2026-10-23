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

## Projects without Git (protocol 36)

Recorded on 14 October 2026. Before this change, Open Repository resolved the chosen folder with `git rev-parse --show-toplevel`. A folder without a repository inside the checkout of another one therefore opened the repository above it.

| Part | Where |
| --- | --- |
| Git stays in its folder | `kodem-core/src/git.rs`: `git_process` sets `GIT_CEILING_DIRECTORIES` to the parent of the folder it runs in, so no Git command, checkpoint or restore reaches a repository above a project |
| Detection and setup | `git::is_repository` (the folder has its own `.git`) and `git::initialize` (`git init`, no commit) |
| Opening | `Request::OpenProject` carries `MissingRepository`: `Ask` answers `Response::NoRepository` and adds nothing, `Initialize` runs `git init` first, `OpenAnyway` adds the folder as it is |
| The dialog | `ui/panels.rs`: `Panel::NoRepository`, "No Git repository here", with Cancel, Open anyway, and Initialize Git and open |
| The workspace | `Workspace.without_git` names the projects whose folder has no `.git`. The host checks this each time it sends the workspace, so a repository made later is noticed |
| Tasks in such a folder | No worktree (`git::worktree` and the composer refuse it), an empty branch (`git::branch`), no linked checkouts, no checkpoints, no review capture after a turn (`orchestrator::observe`), no review targets. The changes panel says "No Git repository" and does not poll. A project check records "no Git repository". The file list walks the folder, without hidden entries, up to 20,000 files |

Checked with a locked `cargo build` of the workspace and `cargo fmt --all -- --check`. Clippy with `-D warnings` reports the six earlier findings and no new one. One temporary test, removed afterward, used a repository with one commit and a folder without Git inside it, in a scratch directory. In the folder: `is_repository` is false, `repository` fails, there is no head, the branch is empty, there are no linked checkouts, a checkpoint fails and writes no reference into the repository above, a worktree is refused with the new message, and the file list holds the two visible files only. The repository above still resolves, has its branch and head, takes a checkpoint, lists its files, and makes a task worktree that has its branch, head, files and checkpoint. Through the real request handler: `Ask` answers with the canonical folder and adds no project; `OpenAnyway` adds it, and a second open returns the same project; a repository opens without a question; review targets and a capture fail with the new messages; `Initialize` makes the same project a repository of its own with a branch and no commit, and a capture then works. Passed.

Not checked, because app control is prohibited here: the dialog, the composer control and the changes panel on screen. A provider's own tools still run Git as the provider does, so an agent's `git status` in such a folder can still see a repository above it.

## Windows and Linux support

Recorded on 15 October 2026. Before this change the workspace compiled for macOS only: the IPC used Unix sockets from `std::os::unix`, data lived in `~/Library/Application Support`, Git paths were raw Unix bytes, processes used Unix process groups and `flock`, and the updater, sign-in, notifications and sounds called macOS tools.

| Part | Where |
| --- | --- |
| Platform layer | `kodem-core/src/platform.rs`: home and data folders (Application Support, `%LOCALAPPDATA%`, XDG data), Git path bytes with `/` separators on Windows, owner-only permissions, non-blocking opens, `detach` and `hide_console` for child processes, shells for the terminal and checks (PowerShell on Windows), executable names and tool lookup with `.cmd` launchers, `plain_path` after every `canonicalize` |
| IPC | `ipc.rs` re-exports `UnixStream` and `UnixListener` from `uds_windows` on Windows. The host locks with `File::try_lock`, checks the peer with `getpeereid` on macOS and `SO_PEERCRED` on Linux, and relies on the per-user folder's access list on Windows |
| Providers | Codex packages are validated per platform (`PACKAGE_FILES`), and the build installs them to `providers::codex::runtimes_dir`. Claude Code is found as `claude.exe` or `claude.cmd`. Sign-in opens `cmd.exe` in a new console on Windows and the first installed terminal emulator on Linux. Shared account configuration uses symbolic links, or junctions and hard links on Windows |
| Notifications and sounds | `notify-rust` on Windows and Linux, with the host playing Kodem's sound through `PlaySoundW` or `pw-play`, `paplay` or `aplay` (`kodem-core/src/sound.rs`). The "Show while Kodem is active" switch is macOS-only |
| Desktop | `secondary-` bindings for application commands, macOS-only Emacs and Option editing keys, Ctrl and Home/End editing on Windows and Linux, shortcut labels translated by `kit::shortcut`, Ctrl-Shift-V paste in the terminal. Windows draws minimize, maximize and close buttons with window control areas; Linux draws them, moves the window and adds resize edges only when the compositor leaves decorations to the app. Windows and Linux quit with the last window. The Windows executable embeds icon resource 1 and version information |
| Updater | Platform asset names, `release.json` verification, system `tar`, Authenticode signer pinning on Windows, PowerShell and Bash installers |

Checked with `cargo check` and `cargo clippy --all-targets -D warnings` for `aarch64-apple-darwin`, `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu` (the Windows and Linux checks used a scratch zig C toolchain and a stand-in resource compiler, so their native libraries were not linked), a locked macOS `cargo build`, and `cargo fmt --all -- --check`. Clippy reports the six earlier findings on each target and no new one. Temporary probes, removed afterward: the shortcut label translation for 15 labels; the Bash dependency helpers against the installed Codex package; the release reuse logic against a stubbed `gh` (new version, reuse of the commit's release, refusal of a draft); the Linux installer and uninstaller in a scratch home; and the Linux update installer replacing a package folder. Passed.

Not checked, because no Windows or Linux machine was available and app control is prohibited here: linking and running on Windows and Linux, the release shader compilation, Authenticode signing, the window buttons and resize edges, notifications, sounds, ConPTY terminals, sign-in windows, and an update cycle on either platform.

## Installing tools from the setup dialog (protocol 37)

Recorded on 15 October 2026. The setup dialog showed only an installation guide link for a missing Claude Code. It now lists Git, Git LFS and GitHub CLI below the providers and offers **Install** for every missing provider CLI or tool.

| Part | Where |
| --- | --- |
| Protocol | `Tool` and `ToolVersion` in `model.rs`; `Request::InspectTools` answers `Response::Tools`, `Request::InstallTool` opens the installer. The bridge sends both off the conversation queue |
| Detection | `kodem-host/src/tools.rs`: `git --version`, `git lfs version` and `gh --version` with `git::search_path`, which now also holds the Git, Git LFS and GitHub CLI folders under Program Files on Windows. On macOS, a `/usr/bin/git` without Command Line Tools counts as missing without being run, because running it opens Apple's installer |
| Installers | A shell script (`.command` on macOS, `.sh` on Linux) or a PowerShell script on Windows, written to `installers/` in the data directory and opened by `accounts::open_terminal`, which sign-in also uses. Each step stops the script when it fails and the window reports the result |
| Dialog | `ui/onboarding.rs`: a Tools group with INSTALLED, MISSING (Git), OPTIONAL and INSTALLING states, and **Install** or **Install again** buttons |

Checked with `cargo clippy --all-targets -D warnings` for macOS, Windows and Linux (six earlier findings, no new one), a locked macOS `cargo build` and `cargo fmt --all -- --check`. Temporary tests, removed afterward: every macOS installer script passed `sh -n`; the Codex installer ran in a scratch home, downloaded the latest package and installed 0.162.1, which the build's package validation accepted; detection on this Mac reported Git, Git LFS and GitHub CLI with their versions; the Claude Code install scripts and the Windows and Linux Codex package URLs answered HTTP 200. The first script check found a `;` that broke the step chain in the Codex installer; it was fixed before the run.

Not checked: the Windows PowerShell and Linux scripts did not run, the dialog was not viewed, and the Claude Code, Git, Git LFS and GitHub CLI installers were not run, because they change this system.

## Windows on Arm64

Recorded on 15 October 2026. `scripts/build-windows.sh` builds `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`: both for Nightly and Production, the build computer's own for Debug, or the list in `KODEM_WINDOWS_ARCH`. For each processor it builds, checks the PE machine type of both executables, signs, packages to `target/<channel>/windows-<processor>/` and publishes `Kodem-windows-<processor>.zip` into one release. `scripts/dependencies.sh` reads the processor from `PROCESSOR_ARCHITECTURE` and `PROCESSOR_ARCHITEW6432`, so an emulated x64 Git Bash on Arm still counts as Arm64. It requires the Visual Studio C++ tools of each processor, installs the matching rustup and Codex package, adds the Rust targets, and prefers the SDK's Arm64 tools on Arm. The updater and the Codex installer already name their downloads after the running processor. `scripts/licenses.py` accepts the Arm64 target.

Checked with `cargo clippy --all-targets -D warnings` for `aarch64-pc-windows-msvc` with a scratch zig C toolchain (six earlier findings, no new one), `bash -n`, a probe of the processor detection with faked Windows variables (x64, Arm64, and an emulated x64 shell on Arm64), and generated Arm64 license notices: 373 packages, one fewer than x64 (`raw-cpuid`). Temporary probes and toolchains were removed. Not checked: a build, signing or run on Windows.

## All platforms from the release commands

Recorded on 15 October 2026. Windows release builds need the Windows SDK's shader compiler and Linux builds need Linux's system libraries, so a Mac cannot build them. The macOS release commands now start the **Release packages** workflow for Windows (x64 and Arm64) and Linux, build macOS meanwhile, and publish one release with every archive (see [releases](releases.md#one-release-for-every-platform)).

Checked with `bash -n` on every script, a YAML parse of the workflow, and temporary probes under `/bin/bash` 3.2 with a stubbed `gh`, removed afterward: a normal run created one release with seven files (macOS, two Windows and one Linux archive, three checksum files); a downloaded archive that no longer matched its checksum stopped the command before any release existed; an upload whose GitHub digest differed left the release a draft; a CI build with `KODEM_RELEASE_VERSION` used the given version without calling `gh`, refused a malformed version and refused another commit. Not checked: a real workflow run, which needs the workflow on GitHub's default branch, and the Windows and Linux builds on hosted runners.

## What's new after an update

Recorded on 15 October 2026. `kodem_release_changes` in `scripts/github-release.sh` finds the channel's previous release (the newest tag below the release version, compared numerically), resolves its tag, and reads the commit subjects up to the release commit from GitHub's compare API, newest first, without merges, at most 100. A failure leaves the list empty and never stops a release. The build commands pass `KODEM_RELEASE_NOTES` and `KODEM_RELEASE_PREVIOUS`; `.github/workflows/release.yml` takes them as the `notes` and `previous` inputs. `crates/kodem-desktop/build.rs` embeds them, and `release_notes.rs` decides once per version, before the window opens, whether the notes are due: not in Debug, not on a new installation, and not without notes. A data directory without `release-notes.json` counts as an update when an earlier version left `window-state.json`. The version is recorded before the dialog shows; if recording fails, the dialog stays closed rather than showing at every launch. `Panel::WhatsNew` opens after the first workspace arrives, unless another dialog, such as the first-run setup, is open. The command palette lists **What's new** when the build has notes.

Checked with Clippy `-D warnings` on macOS (six earlier findings, no new one), a locked `cargo build`, `cargo fmt --all -- --check`, `bash -n`, and a YAML parse of the workflow. Temporary tests, removed afterward: in scratch data directories, a new installation showed nothing at two launches, an update from a version before the record showed once, an update without notes showed nothing, and an update from a recorded version showed once and recorded the new version; notes passed at build time were embedded trimmed, without blank lines, with their previous version. With a stubbed `gh`, the previous release was 0.1.10 for 0.1.11, 0.1.9 for 0.1.10, nightly 0.2.0 for nightly 0.2.1, and none for the first release of a channel; the compare filter dropped merges, kept only subjects and listed the newest first. Not checked: the dialog on screen and a real release.

## Public repository review and protection

Recorded on 15 October 2026. The repository was already public. Review of everything reachable from the pushed `main` and tags, plus the working tree:

| Area | Result |
| --- | --- |
| Credentials | gitleaks 8.30.1 found none in 151 commits or in the working tree. Greps for key, token and private-key formats found none |
| Personal data | 67 commits carry the author's work email. Two design documents named local home paths; they now describe the projects without paths, and the old text stays in history. Committed `.DS_Store` files (removed on 10 September) hold Finder view settings only. Emails elsewhere are upstream authors in license notices |
| Published releases | `BuildInfo.txt` of earlier releases names the build folder, and the binaries embed home-folder source paths. Builds now omit the folder and map those paths to `~` and `kodem` (`kodem_release_rustflags`); a release probe binary held no `/Users/` path |

Repository changes: `.github/CODEOWNERS`, `SECURITY.md` (private vulnerability reporting), `.github/workflows/check.yml` (format, license snapshot, and Clippy with warnings denied on macOS, Windows and Linux), `.github/dependabot.yml` for the actions, and actions pinned to full commit SHAs. The six earlier Clippy findings were fixed with Clippy's own suggestions, so the checks pass with `-D warnings`. `scripts/configure-github.sh` applies the GitHub settings and was run: the **Main** ruleset lets only the administrator and write roles push or merge (the owner added the write role so the collaborator account pushes; `configure-github.sh` keeps it), requires a code owner's approval, approval of the last push, resolved threads and the four checks, and forbids force pushes and deletion; the deploy-key bypass was removed. The **Release tags** ruleset reserves `v*` and `nightly/v*` for the owner and collaborators; a personal account's repository cannot make a collaborator an administrator, so the write role bypasses it. Secret scanning with push protection, Dependabot alerts and private vulnerability reporting are on. Actions allow GitHub's own actions and `Swatinem/rust-cache` only, require SHA pins, default to a read-only token, and need approval for every outside contributor's run. The release commands check before building that GitHub CLI's account can create release tags.

Checked by reading every setting back through the API, as the owner and as the collaborator account (both rulesets: always bypasses), `cargo clippy -D warnings` on macOS with no finding, `cargo fmt --all -- --check`, the license snapshot, `bash -n`, and YAML parses of the workflows.

## Linux release job: Codex package check

Recorded on 15 October 2026. The first coordinated release (nightly 0.1.37) failed in the Linux job with "The downloaded Codex package is incomplete or incompatible." The Codex 0.153.4 Linux package's binaries are static, but its bundled `zsh` needs glibc 2.38 and `libtinfo.so.6`; `ubuntu-22.04` has glibc 2.35, so the build's `zsh -f -c 'exit 0'` check failed. Kodem itself never starts that `zsh`, so the build now runs it on macOS only and checks only that it exists on Linux. Packaging builds in GitHub Actions (`KODEM_RELEASE_VERSION` set) no longer install a Codex runtime at all, because they never run the app. A failed package check now names the check. Checked with `bash -n`, the installed macOS package still validating, and the Linux package's `zsh` dependencies read with `llvm-objdump`. The fixed workflow run itself is not yet checked.

## Every platform builds in GitHub Actions

Recorded on 15 October 2026. macOS joined Windows and Linux in the **Release packages** workflow, which now plans the version, builds all platforms and stages one draft release (see [releases](releases.md#one-release-for-every-platform)). The Build Release commands on every platform run `scripts/release.sh`, which starts the workflow, follows it and publishes the draft from the user's account. Making the workflow publish by itself would need a bypass of the **Release tags** ruleset: GitHub refused the GitHub Actions app as a bypass actor in a personal account's repository, and a write deploy key in an Actions secret was not set up, because it widens who can create release tags; that stays the owner's decision. `build-macos.sh` no longer starts the workflow; with a release channel it packages for the workflow, or publishes macOS alone when run directly, and writes a `.sha256` file like the other platforms.

Checked with `bash -n`, a YAML parse of the workflow, the configuration script re-applied and read back, and temporary probes under `/bin/bash` 3.2 with stubbed `gh`, `git` and `cargo`, removed afterward: Plan chose 0.1.37 after 0.1.36 with its change list; staging uploaded eight files to a draft and published nothing; finishing published a draft whose files all had digests as a prerelease and refused one with a file without a digest; dispatch found its run by the request identifier. Not checked: a real workflow run.

The first all-platform run compiled every platform; the Windows job then stopped at the signing rule, because the repository has no signing certificate and does not allow unsigned releases. The Plan job now checks that before any build, and `build-windows.sh` checks it before compiling.

Windows releases are now unsigned by default: publicly trusted code-signing certificates keep their keys in hardware or a provider's cloud since June 2023, so a `.pfx` secret cannot hold one, and the owner chose to ship unsigned until a signing service is set up. `build-windows.sh` signs when an identity is configured and otherwise warns; `KODEM_REQUIRE_SIGNING=1` restores the refusal, and the Plan job checks it before any build.

The next run failed in the macOS job with "Refusing to replace an unrelated bundle". The build cache had restored `target/`, including the previous run's `target/nightly/Kodem Nightly.app`, from which the cache's pruning had removed `Info.plist`; the build script's guard then could not identify the bundle. Each build job now removes `target/<channel>`, which holds only Kodem's packages, after the cache restores and again after uploading, so packages never enter the cache. Checked with a YAML parse and the resulting step order.

## Glass primary buttons

Recorded on 15 October 2026. The user asked for the send button and similar buttons to take the depth of a 3D glass key, mixed with the dithering. Every primary button (`primary` in `ui/mod.rs`: send, sign in, save and other main actions) is now a glass key in the accent: a vertical gradient body, a darker bezel, inset rim light at the top and bottom, a contact shadow and a soft accent glow, with a toned dither sheen from the top edge, the existing dark dither shade at the bottom and the hover glint on top. Hover brightens the body and widens the glow; a press lights the body from below, swaps the top rim for a dark inner edge and tightens the glow. The composer's stop button, which replaces send while a turn runs with an empty draft, is the same glass key in the theme's red (`glass` in `ui/mod.rs`, which `primary` calls with the accent). The pieces are `sheen`, `glass_fill`, `glass_bezel` and `glass_depth` in `ui/kit.rs`; the rule is in the [dithered interface](../design/dithered-interface.md) document. The sheen is a vertical fade, so it is cached as a strip like the other control textures; the gradient and shadows are drawn by the GPU and add no work on the CPU.

Checked with `cargo build`, `cargo clippy -D warnings` (no findings) and `cargo fmt --check` on macOS. The look on real displays, in light and dark themes, is not checked under the no-visual-testing agreement.

The user found the first version far from the reference images and pointed at their doubled borders. The key is now built from nested layers instead of one gradient fill (`glass_body` in `ui/kit.rs`): a two-point bezel ring that is light, dark and light again from top to bottom, a two-point lens ring with a near-white highlight at its top and a bright refraction at its bottom, and a flat face that holds the sheen, the shade and the hover dots. Each ring is a rounded layer in its middle color with two half-height gradients that fade to clear, because a GPUI gradient has only two stops. The face brightens on hover and darkens on a press through the control's `glass` group; the lens dims on a press. The icon is engraved with a darker copy one point above it. The drop shadow and glow stay on the control (`glass_glow`). The body also covers the control's one-point edge, which stays transparent so that keyboard focus can still color it. Glass keys keep the corners of a control; a round submit key was tried and dropped at the user's request. They have 12 points of side padding so a label clears the rings.

Checked with `cargo build`, `cargo clippy -D warnings` (no findings) and `cargo fmt --check` on macOS. The look is not checked on a display under the no-visual-testing agreement.

## Depth for stretches of work

Recorded on 16 October 2026. The user found the foldable sections of agent work messy and asked for a little depth to make them easier to read. The parent tray of a stretch (`work_header` in `ui/tool_calls.rs`) is now a raised plate: the raised surface, a hairline edge, a highlight inside its top edge, the dithered bottom shade of a button and accent dots on hover. Its trays lie in a recessed well of the inset surface (`kit::well`) in place of the hairline rail. Each transcript row draws its own slice of the well, so `shown_trays` finds the first slice, which takes an inner shadow cast by the plate, and the last, which closes the well with rounded corners. Under a folded parent only runs with a failed call count, matching `tool_run`. While the well is open the plate squares its bottom corners onto it; a folded stretch is the plate alone, with a contact shadow (`kit::plate_light`). Height changes of the last slice are covered by the existing remeasure of the whole stretch when a row is appended, a tool row changes or the parent folds.

Checked with `cargo build`, `cargo clippy -D warnings` (no findings) and `cargo fmt --check` on macOS. The look is not checked on a display under the no-visual-testing agreement.

## Send mode as an icon chip

Recorded on 16 October 2026. At the user's request the composer's Queue/Steer chip shows only its arrow, down for queue and right for steer, in a square chip of the composer's chip height. The submit key shows the same arrow in the same mode. The mode name moved to the tooltip, which also says that a click switches the mode, and to the accessible label. Checked with `cargo build`, `cargo clippy -D warnings` and `cargo fmt --check` on macOS.

## Depth for badges and chosen tabs

Recorded on 16 October 2026. The user asked for a little more depth on status badges (FAILED, RUNNING and others), the workspace tabs on the left and the other badges. `kit::tint_fill` draws a tint that is half again as strong at the top as its mean and six tenths of it at the bottom. `kit::tint_light` adds a lighter line of the tone one point under the top edge, a deeper one above the bottom edge (both below a hairline border, so the border does not hide them) and a tight shadow of the deep tone. Both take only the tone, so `kit::chip` keeps its signature for its many callers. They apply to `kit::chip`, `kit::icon_tile`, the open workspace's tile in `ui/workspaces.rs` and `settings::chosen_layer`, which draws the chosen settings section, the chosen segment of a segmented control, the chosen footer action and the chosen account. `kit::keycap` gets a light line under its top edge and a heavier bottom edge in the theme's edge color. Selected rows of lists and menus are unchanged.

Checked with `cargo build`, `cargo clippy -D warnings` (no findings) and `cargo fmt --check` on macOS. The look is not checked on a display under the no-visual-testing agreement.

## Stop always takes the place of send

Recorded on 16 October 2026. While a turn ran with a draft, the composer showed a separate small stop button next to a queue or steer submit key. The user asked for stop to take the place of send. The submit key is now the red glass stop key for the whole turn, and the separate stop button is gone. A draft is still queued or steered with Enter (⌘Enter for the other mode), as the send mode chip and the key hints say. Checked with `cargo build`, `cargo clippy -D warnings` and `cargo fmt --check` on macOS.

## Queue and steer icons

Recorded on 16 October 2026. The user found the arrows of the send mode chip uninformative. Queue now shows Lucide `list-end`, a list with an arrow that turns into its end, and steer shows Lucide `ship-wheel`, at 14 points. Both come from `lucide-static` 1.55.0 with the geometry unchanged, written in the one-line form of the other control icons and listed in `assets/THIRD_PARTY.md`. Checked with `cargo build`, `cargo clippy -D warnings` and `cargo fmt --check` on macOS.

## Depth for agent links and flat buttons

Recorded on 16 October 2026. The user asked for the badge depth on the buttons that open agents and on the other flat buttons. `kit::raised_fill` draws a neutral fill four hundredths lighter at the top and darker at the bottom; `kit::raised_light` adds the theme highlight one point under the top edge, a darker line above the bottom edge and a tight shadow in the theme's shadow color. They apply to `agent_link` in `ui/subagents.rs` (the agent strip above the composer and the agent link in the conversation) and to the neutral `button` builder in `ui/mod.rs`. The `tinted` builder now uses `tint_fill` and `tint_light`. Hover keeps the depth on the hover fill; a press drops the light, so the button sinks flat. Ghost, icon and glass buttons and the buttons of the Git workspace's own kit are unchanged. Checked with `cargo build`, `cargo clippy -D warnings` (no findings) and `cargo fmt --check` on macOS. The look is not checked on a display under the no-visual-testing agreement.

## Agent link crash in debug builds

Recorded on 16 October 2026. The debug app aborted when the composer showed agent links. `agent_link` started from `ghost`, which sets a hover style, and set a second one for its raised fill; GPUI debug-asserts that an element sets its hover style once ("hover style already set"), so release builds were not affected. The link now starts from `control` with the ghost's glint, icon, label and ripple and sets hover and press once. A scan of the button builders (`ghost`, `button`, `icon_button`, `tinted`, `menu_row`, `primary`, `glass`) found no other chain that sets hover twice. Checked with `cargo build`, `cargo clippy -D warnings` and `cargo fmt --check` on macOS.

## Notices in the top right corner and a notification center

Recorded on 16 October 2026. The user reported that the error card at the bottom of the window covered the message box, and asked for notices to move, to fit the interface better, to animate, and for a notification center beside the settings and archive buttons.

- `ui/notices.rs` replaces the single error card and the single toast with a list of notices. Cards float in the top right corner under the title bar, newest first, at most three. They take the depth of a raised control (`kit::raised_fill`, `kit::raised_light`) with the floating shadow, a dithered wash of their tone from the leading edge and an icon tile. A confirmation (`notify_user`) is one compact line that leaves after 3.6 seconds and is not kept. An error is a card with a label, its text and a dismiss button.
- Errors go through `report_error`, which sets `self.error` (dialogs still show it inline) and records the error. Its card floats while it is the current error: dismissing it clears the error, and an error that code clears or replaces sends its card away. The same error reported again while its card floats does not stack.
- `motion::notice_enter` springs a card in from the right while a Bayer dissolve clears; `motion::notice_leave` slides it back and covers it with the dissolve in 220 ms. They replace `motion::toast`. Neither runs longer than its entrance, so no countdown bar redraws the window.
- The notification center is a menu (`Panel::Notifications`) that opens from a bell in the sidebar footer and on the rail when the sidebar is collapsed. It keeps up to 50 errors and task events: a finished turn, a request for input and a failure of a task that is not open, recorded where the thread row already gets its impact. Entries show their age; an entry of a task that still exists opens it. A badge on the bell counts the entries not yet shown, red when one is a failure. Clear removes the kept entries.

Checked with `cargo build`, `cargo clippy -D warnings` (no findings) and `cargo fmt --check` on macOS. The look and motion are not checked on a display under the no-visual-testing agreement. The Git workspace keeps its own notifications.

## Git workspace (Beta): Liquid merged into Kodem

Recorded on 16 October 2026. The user asked to merge the standalone Liquid Git client completely into Kodem as the fourth workspace of the rail, with all its features, design and elements. The Git tile takes the branch glyph that Review used and carries a BETA badge; Review now shows Lucide `scan-eye`.

- `crates/kodem-git` holds Liquid's engine unchanged in behaviour: Git commands and models, graph layout, cloning, GitHub requests, the AI service (Ollama or Codex CLI) and the Git settings (`git-settings.json` in the data folder, written privately, imported once from Liquid's settings). Child processes get Kodem's tool search path and no console on Windows; interactive rebase uses `:` as Git's no-op editor and a quoted sequence editor path, so it works with Git for Windows. `lfs_push` now names the remote that `git lfs push` requires, which Liquid omitted.
- `crates/kodem-desktop/src/ui/git/` holds the views: workspace, history, changes, diff, sidebar, branch drag and drop, dialogs, pull requests, accounts, clone, onboarding, remote actions and the Git section of Preferences. `ui/git/kit/` re-implements the `gpui-component` parts Liquid used (buttons, popup, context and dropdown menus, popovers, dialogs and toasts, inputs, split panes, virtual list, Markdown) on Kodem's surfaces, theme, dither and motion. `TextInput` gained single-line and masked modes; `dither` gained Liquid's Area, Ember and Pointer fields; `fx` gained `fly_away`.
- Kodem owns the theme, accent, typefaces, text sizes, window and density; Liquid's settings for them are gone. Repository cards are Kodem projects: opening a repository in Git lists it as a project, and Customize opens Kodem's project dialog (Liquid's card shape has no equivalent).

Checked with `cargo build --locked`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check` on macOS, and with clippy for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` through zig (no findings). Liquid's 53 engine tests ran once against the ported crate and passed before they were removed. License notices were refreshed; the package set is unchanged. Read-only parity audits compared every ported file with Liquid. The app was not run or viewed under the no-visual-testing agreement, so layout, motion and the look in each theme are unverified. Differences that remain: no scroll bars, no search in model pickers, toasts leave after a fixed five seconds or on click, avatars load through `curl`, the workspace and its auto-fetch start when Git first opens, and long one-line fields wrap.

## Glyph field behind the chat

Recorded on 16 October 2026. The user asked to replace the chat background with a field of random letters and digits that ripples in waves of different shapes, after the Claude desktop app, in Kodem's own look, animating on a press on the background and on every sent message, and cheap to draw.

- `ui/backdrop.rs` replaces the four blurred lights and their veil with a grid of monospace characters (the code typeface at 10 points, rows of 16). Value noise drifts over the grid; runs of cells along a row share a threshold and the Bayer threshold dithers their ends, so the lit cells read as streaks of text. A lit cell changes its character every few seconds.
- `backdrop::spark` starts a wave; at most eight run at once. The surface element inserts a hitbox and listens for a left press in the bubble phase, after every element above it. A press starts a wave unless a focusable element took it first (selectable message text and fields prevent the default) or it lands on the composer. `Shell::submit_with` and `Shell::create_composer_task` call `backdrop::spark_send`, which starts a wider wave from the middle of the composer. Reduced motion starts no waves and keeps the field still.
- A wave is a ring bent by three lobes of random count, depth and turn. Its edge is in the tone of the mood, scrambles the characters it passes and stays dense while it dims; a wake of character runs fills the ring behind it. The moods keep their meaning: idle, working (denser, faster, tinted), waiting (amber, breathing), finished (green) and failed (red).
- Readability: behind the reading column, down to the composer, streaks keep a quarter of their strength and waves half.

Performance, measured with a temporary test that walked the paint loop over a 1400 × 900 point chat (13,338 cells) and was removed afterwards: 5–23% of cells lit, depending on mood and time. An optimized build took 0.31–0.34 ms of CPU per frame at rest, 0.66 ms with one young wave and 1.6 ms with three overlapping waves. The debug build of this crate is unoptimized and took about ten times as long. The field steps at 25 frames a second at rest, 30 while the agent works and 60 only while a wave runs or a mood blends.

Checked with `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check` on macOS. The look and motion in the app are not checked under the no-visual-testing agreement. The glyph painting cost per lit cell (an atlas lookup per glyph in GPUI) was not measured, because it needs a window. Presses on buttons in the chat also start a wave behind the button, because GPUI does not expose which hitbox is topmost.

The user then asked for the glow of the earlier background behind the glyphs, for more movement at rest, and for thicker waves. `ui/backdrop.rs` paints the four drifting lights and their veil over the reading column again, unchanged from the light field, beneath the glyphs; each mood keeps its two light colors, strength, drift and breath. At rest the streaks move more: the large shapes drift about three times as fast and blend into new shapes every five seconds (`morph`), each row slides and each column rises and falls on its own (`SWAY`), characters change 2.4 times as often and twinkle. A temporary test counted about twice as many cells turning on or off per second as before. The edge of a wave is 24 points wide at the start and 64 at the end (earlier 10 and 40), about four rows thick. The same temporary test, since removed, measured 0.6 ms of CPU per frame at rest, 1.1 ms with one wave and 2.1 ms with three overlapping waves in an optimized build. Checked with `cargo build`, `cargo clippy -D warnings` (no findings) and `cargo fmt --check`. Not checked on a display.

## Keeping the computer awake while an agent works (protocol 38)

Recorded on 16 October 2026. The user asked to prevent sleep while an agent works, on by default, with a switch in Settings.

| Part | Where |
| --- | --- |
| Setting | `Settings.prevent_sleep`, true by default. The protocol number changes because a host from before this change would save settings without the new field |
| Check | The host's main loop asks every 2 seconds (`awake::Awake::due`). `Store::working` reads only `json_extract(json, '$.state')` and is true while a task is `Starting` or `Running`; a task that waits for the user does not keep the computer awake. The host holds the lock, so it works with the window closed, and a failed read releases the lock rather than stopping the host |
| macOS | `kodem-host/src/awake.rs`: an IOKit `PreventUserIdleSystemSleep` assertion named "A Kodem agent is working", the kind that `caffeinate -i` takes. The display can sleep, and the lid and Sleep still work. The system releases it if the host stops |
| Windows | `SetThreadExecutionState(ES_CONTINUOUS \| ES_SYSTEM_REQUIRED)`, taken and released on the host's main loop thread, which owns that state. `windows-sys` gains `Win32_System_Power` in the host |
| Linux | A `systemd-inhibit --what=idle:sleep --mode=block` process whose shell exits when the host's process is gone. Without systemd nothing is locked |
| Switch | Settings → Workspace → Tasks → "Keep the computer awake" (`AWAKE_LABEL` in `ui/settings.rs`) |

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. `awake.rs` passed Clippy with `-D warnings` for `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin` in a scratch crate; the first Windows run found the reason text unused there and it was fixed. Temporary probes, removed afterward with the scratch crate and the two Rust targets: on this Mac `pmset -g assertions` listed the assertion while it was held and not after release, and `Store::working` was true for `Running` and `Starting` and false for `Waiting`, `Completed` and an empty store. Not checked: the Windows and Linux locks on those systems, and the switch on a display.

## Performance of long text, many working agents and large diffs

Recorded on 16 October 2026. The user reported lag while scrolling long replies and long prompts, with many working agents and with large diffs, and asked for long sent and pasted text to show as a file that unfolds. Animations, their frame rates and their look are unchanged.

The cause: every animation step of a working agent, every step of a streamed reveal and every scroll event renders the workspace view again, and the chat list lays out every row in view on each frame. A long message was one row, so each frame built and measured all of its blocks and shaped all of its text again.

| Part | Where |
| --- | --- |
| Shaping | `TextInput` keeps its last two shapings (`Shaped`) and reuses them until the text, a style, the font or the width changes. Layout and prepaint no longer shape the same text again on every frame. Streamed text that is still revealing is shaped each time, as before |
| Read-only text | A read-only `TextInput` registers its 38 key actions only while it is focused; a press focuses it and redraws first. The mouse-move listener for drag selection exists only while a selection is being made |
| Markdown | `markdown::blocks` returns shared `Rc` blocks, and blocks render by reference. A block's text is copied only when it changed, and the copy buttons read the text when clicked |
| Chat rows | `conversation::ChatRows`: a reply, notice, error or check result of 3000 bytes or more takes one list row for each part of about 1200 bytes of its blocks. A title stays with the block after it. The first part has the heading, attachments and context, the last part the actions. Text selection orders rows by `(seq, part)`; the history rail, tool trays and transcript updates map messages to rows. Subagent popups keep one row a message |
| Long user text | A pasted text of 4000 bytes or 60 lines or more becomes a file card in the composer (`TextInput::folded`). The pencil moves it into the draft and the cross removes it; both can be undone. `text()` still returns the whole message, so drafts, sending and restoring are unchanged. A draft that leaves the composer with cards comes back with them; another long draft comes back as one card. A sent user message of that size shows as a file card that unfolds to its text, plain and in a scrolling box |
| Review panel | The changes tree and each file's line totals are built once per snapshot and folded folders (`ChangeTree`), not on every frame |
| Git workspace | A file row of the Changes list no longer reads the disk twice on every render; the submodule check runs when its menu opens. Queued message labels show one line |

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Three temporary tests passed and were removed: the parts of a long reply cover every block once and keep titles with their section; `ChatRows` maps messages to rows and back; and the long-text limits and the composition of a draft with its cards. Not checked: frame times in the running app and the look of the cards and split replies, under the no-visual-testing agreement. The Git workspace's Changes list still lays out every file row when it renders; it renders when its repository state changes, not on animation frames.

## The project of a new task is kept

Recorded on 16 October 2026. The user asked that the project chosen on the start screen be remembered and chosen again by default for a new task. `Settings.selected_project` holds it. Like `show_review`, the window copies its `selected_project` into each settings snapshot and reads it back from the first workspace reply, before the check that falls back to the first project when the saved one was removed. Settings are saved when the project picker, an added project or a project's "Choose…" defaults button sets it; creating a task already saves them. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A restart with a saved project is not checked in the running app.

## Every stretch of work sits on a plate

Recorded on 16 October 2026. The user reported that some work trays showed without the raised plate and recessed well. A stretch of one tray (one run of calls or one thought) had no parent, so it rendered as a flat line. Now its own summary line is the plate and its calls or its thought text hang in a well under it, so the label does not repeat. `plate` and `flat` in `ui/tool_calls.rs` set a summary line's surface and its only hover style; the parent plate of a longer stretch uses `plate` too. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. The look is not checked on a display under the no-visual-testing agreement.

## Surface layers take the shape of their surface

Recorded on 16 October 2026. The user reported that the corners of the dithered and tinted layers on buttons did not match the buttons, for example on a chosen font card in Settings, and asked for them to fit any shape and size. The layers took a fixed radius (mostly `RADIUS_CONTROL`) and filled the box inside the surface's border, so their corners stood one point inside the border's inner arc, and on a surface of another radius they did not match at all. GPUI clips children to rectangles only.

| Part | Where |
| --- | --- |
| Corners | `components::inner_corners` returns each radius less the wider border at that corner. A transparent border still takes layout space, so it counts |
| Layers | `kit::Layers::layer` adds a `kit::Fit` layer under the content. `Button::fitted` builds it when the button renders, from its final style, so a radius set after the layer still applies; a `Div` or `Stateful<Div>` reads the radius and border set before the call |
| Dots | `Dither::fitted` takes those corners in window pixels and the sides that the grid shares with its surface; painting bounds each corner to half of those sides, as GPUI does for quads, so pills and short controls keep their arcs. `Dither` is a `Fit` that covers its surface |
| Fitted pieces | `kit::shade`, `kit::glint`, `kit::glass_body` (rings and face in from the outer corners), `dither::row_wash` (the active flow now also keeps out of the right corners), `dither::edge_shade`, `settings::chosen_layer`, `kit::icon_tile`, `kit::marker`, the sidebar project tile and the Git workspace's `LiquidButton` shade and glint. The theme card footer now rounds its bottom corners to the card's inner corners |
| Glass rings | Both blends of a ring cover the whole ring with stops at the middle, instead of two half-height quads, so they keep the ring's corners at any height |

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary test passed and was removed: `inner_corners` for a bordered control, a borderless surface, mixed borders on one side, a square bordered box and `rounded_full`. The look is not checked on a display under the no-visual-testing agreement. Other dithered layers with a fixed radius on unbordered surfaces, such as Git workspace panels and loaders, are unchanged.

## The title bar moves the window only from its empty areas

Recorded on 16 October 2026. The user reported that holding the pointer down on a button or the command palette field in the title bar and moving it moved the window, and asked that only the bar's empty areas move it. On macOS AppKit moved the window from the whole transparent title bar strip, controls included, because GPUI leaves that to AppKit by default. On Windows the bar's `WindowControlArea::Drag` made the system's hit test answer "caption" over the bar's controls too, since their hitboxes did not hide the bar.

| Part | Where |
| --- | --- |
| macOS | `WindowOptions.app_owns_titlebar_drag` is on (`main.rs`), so AppKit no longer moves the window from the strip. A press on the bar itself calls `start_window_move`; a double click calls `titlebar_double_click`, which follows the system's double-click setting |
| Controls | Every control in the bar occludes it: the sidebar toggles, the waiting chip, the command palette field, the files, terminal and changes buttons, the window buttons of Windows and Linux and the Git workspace's fetch, pull and push group. GPUI's hit test stops at them, so the bar's handlers and its drag area miss the press there. Labels such as the project name, the branch and the running count stay draggable |
| Linux | The compositor move starts only while the button that pressed the bar is still down, so a press that ended over a control cannot move the window later |

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Moving the window is not checked in the running app under the no-visual-testing agreement, nor on Windows or Linux.

## The meter tab lights as a whole

Recorded on 16 October 2026. The user asked that the hover of the CTX and USE meters fill the whole tab on the composer's top edge. The meters were a small rounded button inside the tab, so only that rectangle lit. Now the tab itself is the meters' control (`usage_meters` returns the `Button`, and `chat.rs` gives it the tab's geometry). Its slanted surface is a layer under the meters, and `paint_meter_tab` lays the `selected` tone over the whole shape, the seam over the composer's edge included, while the pointer is over the tab (`Shell::meter_tab_hovered`, set by `on_hover`), while the usage panel is open and while the tab has keyboard focus (`Shell::meter_tab_focus`). A press no longer darkens the meters. The meters keep their place in the tab. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. The look is not checked on a display under the no-visual-testing agreement.

## The conversation leaves the user's scroll alone

Recorded on 16 October 2026. The user reported that the conversation jumped back to its end while they scrolled up, mostly when a thought or a tool tray changed, and that the jump to the latest message could show without any scrolling. They asked to respect the user's scroll and to move the jump to a better place.

The cause: every transcript update set `follow` from `ListState::is_scrolled_to_end`, then scrolled to the end when it was set. That check answers `None` while any row has no measured height, which is the usual state of a long conversation and of every row just remeasured. `unwrap_or(follow)` then kept an old `true` and pulled the reader back. A row that grew before layout could also read as away from the end, which hid the following and showed the jump.

| Part | Where |
| --- | --- |
| Following | `chat_scroll` uses GPUI's `FollowMode::Tail`. The list keeps its end in view during layout while it follows, stops following on any scroll up and resumes when the user scrolls back to the end. Transcript updates no longer scroll it: a following list stays at the end by itself, and any other keeps the same part of the same message at the top. A task that opens, a sent message, the jump and the Latest action call `Shell::follow_latest` |
| Trays | The trays that open or fold when a turn starts or ends only measure again |
| Jump | `Shell::latest_wanted` is true while the list does not follow its end. The jump sits centered above the composer. Once something arrives below while the user reads above, it reads "New below" in the accent (`Shell::unseen_below`). The list resumes following during layout, after the frame rendered, so the composer's measuring canvas asks for another frame when `latest_shown` no longer matches |
| History | A jump to an earlier turn stops the following through `scroll_to` |
| Agent threads | The subagent thread popup follows its end the same way; it used `is_scrolled_to_end().unwrap_or(true)`, which pulled the reader to the end after each update |

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Scrolling during a running turn is not checked in the running app under the no-visual-testing agreement.

## The composer shows inline code as the user types it

Recorded on 16 October 2026. The user asked for the look of inline code in messages, text between backticks, to show in the prompt while it is typed. `markdown::draft_code` reads the draft with the options that messages are read with (`markdown::OPTIONS`, now shared with `parse_blocks`), so the composer marks exactly the code that the sent message will show: a span counts once its closing backticks are typed, and code fences, escaped backticks and spans across lines do not count. `TextInput::set_live_code` turns this on; the composer sets it with the current theme as it renders. Shaping sets the code in the code face, draws its backticks in the subtle color and gives the code the dotted plate of message code through the existing bubble padding, so the text, the caret, selection and copying keep the typed backticks. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary test passed and was removed: one span, double backticks around a backtick, an unclosed span, a fence, escaped backticks, a span across lines, two spans on a line, and spans in a list and a quote. The look and typing next to a plate are not checked in the running app under the no-visual-testing agreement.

## The background covers the window and the side panels blur it

Recorded on 16 October 2026. The user asked for the glyph field to cover the whole window and for the panels at its sides to blur it slightly, on macOS, Linux and Windows.

| Part | Where |
| --- | --- |
| Field | `Backdrop` paints the field over the whole viewport (`Frame::bounds`). The chat surface (`Frame::chat`) still places the calm reading column, its veil and the composer's gap, and still takes the presses that start waves. Waves reach the window's corners. The streaks are now laid out from the window's corner, so they no longer move when the sidebar opens |
| Glass | `kit::glass` gives a surface its tone at `GLASS_FILL` over a `GLASS_BLUR` backdrop blur; `kit::panel_glass` uses it with `sidebar`, which fills a panel at 25% over a 6-point `backdrop_blur`, so three quarters of the blurred field shows through. It started at 86% over 14 points, which the user found hard to notice, then went to 72% over 8 points and 50% over 6 points as the user asked for more. The field's characters are about one glyph wide, so a wide blur smeared them away and left only the soft lights; at 6 points they stay soft shapes. It applies to the title bar, the workspace rail, the task and Git sidebars and the side review pane. The centered review keeps the chat's canvas |
| Platforms | `backdrop_blur` is GPUI-CE core API without platform gating. The Metal (macOS), wgpu (Linux) and DirectX (Windows) renderers each draw `BackdropFilters`, clipped to the panel's rounded rectangle. While a blurred panel is on screen, each frame renders to an offscreen scene texture and blurs from it, then copies it to the window; the field steps at 25 to 60 frames a second |

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Linux and Windows were not built for this change, which uses no platform-specific code. The look and the GPU cost are not checked in the running app under the no-visual-testing agreement. While the Git workspace is open there is no chat surface, so the window shows the plain background color behind its glass panels, as before.

## The composer is glass

Recorded on 16 October 2026. The user asked for the side panels' glass on the composer. The composer box takes `kit::glass` in `raised` at `COMPOSER_GLASS_FILL` (0.625), so half as much shows through as through the side panels' 0.25; the user asked for that once the conversation scrolled behind the box. The meter tab uses the same fill. The meter tab is a painted path, and a backdrop blur is a rounded rectangle, so `paint_meter_tab` blurs the flat top as one rectangle with the tab's rounded corner and follows the slant with `METER_TAB_SLANT_STEPS` (5) narrow steps, each as high as the slant at its middle, before it fills the shape at `COMPOSER_GLASS_FILL`. The composer's top edge runs under the tab's bottom row, so that row stays solid in `raised`; through a see-through fill the edge would show as a line across the tab. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. The look is not checked on a display under the no-visual-testing agreement.

## The conversation scrolls behind the composer's glass

Recorded on 16 October 2026. The user reported that the composer's glass blurred only the background field, because the conversation ended at the composer's top. The list now runs down to the bottom edge of the composer's box: its container keeps only `Shell::composer_tail` below it, the height of the hints and the margin under the box, measured at rest from the box's anchor in the floating composer's measuring canvas so that the spring of the box does not move the conversation. The list keeps the rest of the composer's height, `Shell::chat_cover`, as bottom padding, so its end still rests above the box and the messages before it scroll behind the box and its meter tab, which blur them. The hint row under the box stays clear. The box occludes what lies under it, so the wheel over it does not scroll the conversation. The history rail stays above the box and counts its ticks without the covered part. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. The look is not checked on a display under the no-visual-testing agreement.

## The conversation's text has a context menu

Recorded on 16 October 2026. The user asked for a right-click menu for selected text in the chat. A secondary press anywhere over the conversation list opens `Panel::SelectionMenu` at the pointer, in the shell's menu surface that the thread menu uses: a "Text" title, Copy and Select all as ghost rows with their `⌘C` and `⌘A` keycaps (named `Ctrl+C` and `Ctrl+A` on Windows and Linux). Copy is disabled while nothing is selected. Both rows act on `Shell::chat_selection` through the new public `TextDocument::selected_text`, `copy` and `select_all`, which the keyboard shortcuts now share. The press does not move the keyboard focus, so `⌘C` still copies after Select all from the menu. A pointer-opened menu, the thread menu or this one, opens at the pointer instead of above or below a control. The subagent popup's conversation has no such menu. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. The menu is not checked in the running app under the no-visual-testing agreement.

## Provider CLIs get Kodem's tool search path

Recorded on 16 October 2026. A Windows user reported that Kodem did not find Claude Code until Kodem had been closed and opened again. Kodem found the `claude` program itself, because `platform::tool_directories` is read on each discovery, but it started the CLI with the host's own PATH. A Windows process keeps the PATH it started with, so a host that was running while Git was installed, for example from the setup dialog, has no Git on its PATH. Claude Code on Windows needs Git for Windows for its shell and does not start without it; the host quits after five idle minutes, and the next host has the new PATH, which explains the report. This is inferred from the code and Claude Code's documented requirement; the user's error text was not available. `Account::apply`, which every provider CLI process goes through, now sets PATH to `git::search_path`, the PATH that Git, Git LFS and GitHub CLI already get: the process's PATH, then the tool folders and, on Windows, the folders of the Git, Git LFS and GitHub CLI installers. The Windows console that runs a sign-in or an installer gets the same PATH. `tool_directories` also lists `%LOCALAPPDATA%\Microsoft\WinGet\Links` on Windows, where WinGet links the programs of portable packages.

## Windows updates move the installed folder

Recorded on 16 October 2026. In a Windows test the update closed Kodem, did not install or restart it, and reported a broken package afterwards. The detached installer inherited the desktop's working directory, which is the installed folder when Kodem starts from its Start menu shortcut or from a previous update, and Windows refuses to move a folder that a process works in. The installer therefore blocked its own move for 60 seconds and exited without opening Kodem. `updater::install` now starts the installer in the folder above the installed one. The script moves folders with `[System.IO.Directory]::Move`, one rename that moves the whole folder or nothing, in place of `Move-Item`, which can copy and delete and so leave a folder without some of its files; this is the likely source of the broken package. When the installed folder stays in use, the script opens the untouched app again. A failed run keeps its log in `install.log` in the `.kodem-update-*` folder beside the installed one. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` for macOS and Windows x64 (no findings) and `cargo fmt --check`. An update was not run on Windows.

## The Adys theme

Recorded on 16 October 2026. The user asked for a special dark theme in violet and pink, named Adys. `ThemeId::Adys` follows Graphite in the picker and the command palette. Its palette in `theme.rs` has plum-black surfaces that step from the backdrop to the controls in one violet hue, lilac text, a hot pink accent (`#ff6ac1`), a violet info color (`#b69cff`), and mint, coral and gold for change, failure and attention. The chat background draws its lights from the accent and the info color, so it glows pink and violet. `syntax.rs` gives Adys its own code colors: pink keywords, violet functions, sky types, peach strings, orchid constants and blush parameters on lilac text with violet-gray comments. A saved theme is stored by name, so no saved settings change.

Checked with a script, not kept: against the canvas, sidebar, header, raised and inset surfaces, text is at 14.5:1 or more, muted text 8.8:1, subtle text 4.8:1, the accent 6.4:1 and the four status colors 5.6:1 or more; text on the accent is 7.3:1; every code color is 4.3:1 or more on the raised surface and higher on the canvas and code wells. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` and `cargo fmt --check`. The look of the theme is not checked on a display under the no-visual-testing agreement.

## Measured performance of the chat, the background, History and the host

Recorded on 16 October 2026. The user reported that scrolling long chats with many thoughts lagged, and asked to measure the heavy parts of the app and make them faster without changing their look. Temporary benchmarks ran the CPU work of each part on synthetic data in an optimized build: threads of up to 12,120 messages (12 MB of text), stretches of up to 600 tool rows with a thought every third row, 500 long replies, a 1728 × 1117 point window, 1000 commits and rows of up to 1 MB. GPUI's own layout, text shaping and drawing need a window and were not measured.

| Part | Cause | Change | Before | After |
| --- | --- | --- | --- | --- |
| Stretches of tool work | Every row of a stretch walked the whole stretch: its views, trays and running calls. Folded rows have no height, so the list renders every row of a stretch in view on each frame | `tool_calls::Stretch`: a frame builds each stretch once (`Shell::work_stretches`, emptied as the shell renders) and its rows read it. `tool_output` keeps 1024 views a generation instead of 512, so a long stretch no longer drops and rebuilds views each frame | 300 rows: 17 ms a frame; 600 rows: 270 ms | 0.11 ms; 0.24 ms |
| Rows of the conversation | Each transcript update read every long reply's Markdown again; past 384 long replies the cache could not hold them | `ChatRows::new` keeps the parts of a long reply whose text did not change, and the rows read their parts from `ChatRows::parts` | 500 long replies: 72 ms an update | 0.16 ms |
| Drag selection order | Built over every row on every frame | Set with the rows (`TextDocument::set_viewport`); a frame calls `set_scroll` | 0.15 ms a frame at 12,120 messages | none per frame |
| Code highlighting | Each streamed update of a code block highlighted all of it again | `syntax::Highlighted` keeps syntect's state after the last complete line, so an update highlights the lines it adds. A text that changed before its end, another language or another theme starts again | A streamed 400-line block: 1.49 s in all, up to 17 ms an update | 24 ms in all, about 0.15 ms an update |
| Glyph background | Every cell sampled three noise values | Cells that no wave and not even the strongest streak can light skip the noise, and the finer detail is skipped where it cannot light the cell | 0.65 ms a frame at rest | 0.50 ms |
| History graph | Each visible row computed its glow cell by cell on every paint | `history::glow_dots` keeps the dots of a row by its lanes, size and Bayer phase; segments beyond the glow's reach are skipped | 0.85 ms a frame for 40 rows | 0.014 ms |
| Transcript merge | Each watch reply put the whole transcript in a map, and the update compared every row's text three times | Rows in order are merged by search; the map remains for rows out of order. The rows are compared once | 12,120 messages: 4.0 ms and 3 × 0.8 ms a reply | 2.5 ms and 0.8 ms |
| Host row writes | A streamed row was serialized twice per write | Serialized once, then checked against the 4 MiB limit | 1 MB row: 2.0 ms a write | 1.1–1.7 ms (the commit's fsync varies) |

Every change keeps its result: a temporary test compared each against the code before it. The background's lit cells, places, layers, alphas and characters were identical over 2,400 frames in two window sizes, two moods and zero to three waves. History's glow dots were identical for five lane shapes in 16 rows. The incremental highlight was identical to a full highlight at each of 163 streamed steps in three themes, and after an edit and a change of language. The merge matched the map merge in 300 random rounds with duplicates and rows out of order.

Not changed: a wave's ring math, which costs most of a frame with three waves (2.4 ms), because a cheaper form would move some characters; History's activity summary (0.04 ms a frame) and lane width (0.015 ms); the Git Changes list, which builds every file row on each render; and the host's fsync per committed row.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. The benchmarks and comparison tests were removed. Frame times in the running app and the look are not checked under the no-visual-testing agreement.

## Title bar art and badge for each build type

Recorded on 16 October 2026. The user asked for art behind the sidebar's end of the title bar, like the starry sky of T3 Code, with its own scene for Debug, Nightly and Release builds, and for a badge of the build type in the style of the project. `ui/channel_art.rs` holds both. `titlebar_art` paints a still scene of dots through the new `dither::paint_scene`, which bakes it once into a cached sprite with a digest of the build type, the size and the colors. The art keeps the width of the open section, `(RAIL + 1 + sidebar_width) × scale`, and the section now clips it while it slides, so a slide does not bake new sprites. Debug is an amber scope trace that ends in a spike on a red breakpoint, Nightly a blue night sky with a crescent moon, a nebula and stars, and Release a striped sun rising in the accent with rays and haze. `channel_badge` replaces the plain version label: a tinted chip with the glyph and the uppercase type on a plate, a dotted rule and the version. `Channel::label` names the type (Debug, Nightly or Release) and replaces `updater::version_label`, which only the title bar used. The rules are in `docs/design/dithered-interface.md`.

Checked with a temporary test, not kept, that printed each scene as text at the default sidebar width: the motif sits between the badge and the sidebar button, the Debug trace stays flat under the wordmark, and 13 to 38 faint dots lie in the band of the wordmark and the badge. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Windows and Linux were not checked; the change uses no platform code. The look is not checked on a display under the no-visual-testing agreement.

## Profiled responsiveness: the glyph field, the scrim and regions of the shell

Recorded on 16 October 2026. The user reported that scrolling stayed laggy everywhere, the preferences included, with high CPU use. The Nightly 0.1.43 build that the user was running was sampled with macOS `sample`, passively, with no control of the app: once at rest (37% CPU) and four times while the user worked in it (71–94%). In every sample the main thread spent its time drawing frames: 20–26% placing primitives in GPUI's bounds tree, 13–18% painting the glyph field, 9–14% replaying the cached shell, 6–13% laying out conversation rows, 4–8% encoding Metal commands and up to 7% painting the dithered scrim of a dialog.

| Cause | Change |
| --- | --- |
| The glyph field painted 3,000–9,500 glyphs on each of its 25–60 frames a second, and GPUI placed each glyph in the bounds tree that orders primitives. They made that tree deep for everything painted and replayed after them | `backdrop::paint` paints the glyphs in one `paint_layer` above the lights. GPUI orders the layer once and the glyphs take its order, so they draw as before |
| A dialog's scrim covered the window with tiles of 32 cells, about 1,900 sprites a frame | Tiles of `dither::TILE_CELLS` are 128 cells, still a multiple of the Bayer period, so the pattern is the same; `paint_tiles` paints them in one layer |
| GPUI builds and lays out a view again when anything inside it is notified, and nearly the whole workspace was the shell. A scroll of the preferences, a hover, or a pulse in the sidebar built and laid out every visible conversation row again | `ui/region.rs` made the conversation list, the sidebar's feed and the preferences' content cached views of their own. Removed again: see [glass that blurs once](#glass-that-blurs-once) |

Not changed: the glass panels' backdrop blur and the glyph field's frame rate. Each frame of the field draws the whole window again, its glass included, so the field's 25 frames a second at rest are most of the remaining idle work; a lower rate would change its motion.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. The new build has not been profiled in the running app; the look, scrolling and every region's updates are not checked under the no-visual-testing agreement.

## Installers and the update helper

Recorded on 16 October 2026. Releases carry installers next to the update archives: `Kodem-macos-aarch64.dmg`, `Kodem-windows-<processor>-setup.exe` (NSIS, per user, Start menu and desktop shortcuts, uninstaller) and `Kodem-linux-x86_64-setup.run`. The package folders hold no install scripts. The `kodem-update` helper replaces the three updater scripts: it waits for the desktop to exit through its standard input, ends left-over `kodem-host.exe` processes on Windows, retries every folder move, rolls back and reopens. `scripts/build-windows.sh` runs in steps, so that the release workflow can sign with SignPath between them. See [releases](releases.md#installers-and-the-update-helper) for the design and the checks.

## Glass that blurs once

Recorded on 16 October 2026. The user reported that scrolling everywhere and typing still lagged in a debug build, and that the app had been much smoother before that day's commits. A 20-second `sample` of the debug build while the user scrolled and typed found its main thread idle 84% of the time. The GPU was the limit: the IORegistry's `accumulatedGPUTime` of each process showed the idle debug build using 15–17% of the GPU, the Nightly build 30–32% while it streamed a reply, and WindowServer 19%. In `gpui_ce_macos`, each backdrop blur ends the render pass, downsamples the whole window, runs two Gaussian passes over the whole half-size window and composites the result, whatever the size of the blurred shape. Since the glass of `43b4bb1` a frame had about eleven of them: the title bar, the rail, the sidebar, the review pane, the composer, and the meter tab's flat top and five steps along its slant. The glyph field, since `f8334e7`, draws a frame 25 to 60 times a second, and a scroll, a typed letter or the composer's spring draws one on every display frame.

| Cause | Change |
| --- | --- |
| Each glass panel beside the chat blurred the window again, though only the field lies behind the panels | `backdrop::paint` blurs the field once over the window, paints the chat surface over the blur and paints the field sharp again inside it. `kit::panel_glass` gives a panel its tone without a blur of its own. Four blurs become one; the composer and the meter tab keep theirs, which blur the conversation behind them |
| The regions of the previous change could not keep a frame: GPUI marks the views around a notified view as notified, and while a cached view renders, every cached view inside it renders too (`window.refreshing`). A scroll in a region rendered the shell, and the shell rendered every region | `ui/region.rs` and the region views are removed; the lists are part of the shell again |

What looks different: the one-pixel dotted rule between the rail and the sidebar shows the field blurred instead of sharp. The field follows the chat's bounds one frame late, as its calm column did before, so while a panel slides or is resized the edge between the blurred and the sharp field trails the panel's edge by one frame.

Not changed: the meter tab's six blurs and the composer's blur, because fewer steps along the slant would change its edge; the field's frame rate; and GPUI's blur, which would need a patched renderer to blur only around each shape.

Checked with a locked macOS `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and a debug build of the app bundle. The look is not checked under the no-visual-testing agreement.

## The GPU limits macOS release builds

Recorded on 16 October 2026. The user reported that every scroll view of the Nightly build lagged on macOS, with hovers and clicks up to three seconds late, on a Mac Studio (M1 Max, three 4K displays at 144 Hz) and on a MacBook Pro (M3 Pro), while the Debug build on the same Mac and the Nightly build in a Windows virtual machine were smooth. Data, settings (the interface scale was tried at 90, 100 and 110%) and the scroll utility were ruled out. Passive measurements of Nightly 0.1.44: `sample` found the main thread idle about 73% of the time while the user scrolled, with no blocking call; the IORegistry's `Device Utilization %` was 83–100% while scrolling and 92–97% at rest, with WindowServer at about 45% of the GPU, Kodem at 22–28% and another app at 16%.

A release build produces frames as fast as the display asks: on every refresh while the user scrolls or types, 25–60 a second for the glyph field, and, for one second after fast input, GPUI presents again on every refresh, which renders the whole scene on the GPU (`Window::present` calls `renderer.draw`). Each frame paid for every backdrop blur, and in `gpui_ce_macos` each blur ends the render pass and blurs the whole window. At 144 Hz in 4K the demand passed what the GPU could do, so frames and input queued. A Debug build runs Kodem's code unoptimized, draws fewer frames and stays under the limit.

`b1f03c2` made the side panels share one blur. `paint_meter_tab` now blurs only the tab's flat top; the five steps along its slant are gone, so the slant shows its tint over the unblurred conversation. A frame has three blurs instead of about eleven: the field under the panels, the composer and the tab.

Checked with a locked macOS `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. No release build was made, at the user's request, so the GPU use of the new build is not measured; the look is not checked under the no-visual-testing agreement.

## A blur that costs its region

Recorded on 16 October 2026. With three blurs a frame, Nightly still used about 25% of the GPU at rest on a 4K display. The user asked to rewrite the blur for the most performance. Kodem patches GPUI's macOS renderer: `vendor/gpui_ce_macos` is `gpui_ce_macos` 0.1.0, wired in through `[patch.crates-io]` and excluded from the workspace. Only `metal_renderer.rs`, `shaders.metal`, `gpui_macos.rs` and the new `region_blur.rs` differ from the published crate.

| Before | After |
| --- | --- |
| Each blur downsampled the whole window, ran two gaussian passes over the whole half-size window, and composited in a render pass of its own | `BlurJob` works out the half-res texels that the composite reads, grown by the kernel's reach, and the downsample and both gaussian passes run as compute kernels over those regions only |
| Every pixel of each gaussian pass evaluated `exp` for each of its 37 taps | `BlurKernel::weights` normalizes the weights once per blur; a texel adds mirrored taps |
| A blur broke the scene's render pass twice | The composite draws in the resumed scene pass. Consecutive blurs of a batch share the passes and one break when none reads pixels that an earlier one composites and they share a kernel |

The blurred values are the old ones. A temporary GPU harness (Swift, removed) ran the original shader's full-window passes and the new kernels on one 1003 × 701 image for six cases: the panel radius, a strip along the window's edge, a corner region, larger radii including spread taps, and a tiny shape. In each region the composite reads, at most 1 of 255 differed, in at most 9 of up to 184,000 channels, from weights normalized on the CPU.

A blur now costs its region, so `b1f03c2`'s single blur of the whole window is reverted: each side panel has its glass blur again, the field is painted once, and the one-frame lag of the blurred edge and the blurred dotted rule are gone. The meter tab keeps one blur, because the steps along its slant touch one another and would each break the render pass.

Third-party notices: `scripts/licenses.py` rejected the patched crate, which has no registry source, as a private workspace crate. It now accepts crates under `vendor/` as third-party, hashes their top-level files as notice inputs and marks them as modified in `Rust-THIRD-PARTY.txt`; each changed file states its change, as Apache-2.0 asks. The notices were refreshed and still cover 385 external packages.

Checked with a locked macOS `cargo build` and `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check`, rustfmt on the patched files, and a compile of the stitched shader with the system Metal framework, which built both compute pipelines. GPU use in the app is not measured until a release build runs; the look is not checked under the no-visual-testing agreement.

## Project commands

Recorded on 16 October 2026. The user asked for scripts such as `.command` or `.cmd` files that a project declares in its settings, as a tree of steps that run in order and reorder by drag and drop, and for a button left of the chat header's change totals that lists them with a run button and streams their output into Kodem's terminal.

`Project::commands` holds `ProjectCommand`s: an id, an optional name and the steps, which are paths. A step inside the project is saved relative to its root, so that `ProjectCommand::programs` resolves it in the task's own checkout, or in the project's root when the checkout does not hold it, such as a file that Git does not track. `Store::customize_project` saves them with the name, accent and icon, and rejects a command without steps, more than 32 steps, more than 64 commands, and names or paths with control characters.

The Customize project dialog shows the commands in `ui/project_commands.rs`: a card per command with its name field over a branch of step tiles. Add step and Add command open the file picker. A step tile drags to another place in any command; it lands before the tile it is dropped on, or at the end of a command when it is dropped on the command's header. A command's grip drags the whole command. The play button left of the change totals opens a menu of the open task's project commands; each run button sends `Request::RunProjectCommand`, opens the terminal and sizes it to its pane.

The host types one line into the task's terminal, which starts a new shell when none runs, so the output streams in the terminal as if the user typed it. `platform::program_sequence` builds the line: on macOS and Linux `cd` to the checkout and the steps joined by `&&`, quoted for POSIX shells and fish; a macOS `.app` runs through `open -W` so the next step waits until it quits, and a script without the executable bit runs through `/bin/sh`. On Windows, PowerShell 5.1 has no `&&`, so each step runs in the `if ($?)` branch of the one before; a `.exe` whose PE header names the GUI subsystem starts through `Start-Process -Wait`, the rest through `&`. The host checks that every step exists before it types the line.

Checked with a temporary test, not kept, that ran the generated line through zsh, bash and sh: the steps ran in order from the checkout with a quote in the path, a script without the executable bit ran, and a failing step stopped the chain with its exit code. The PE check read a GUI and a console header correctly. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Windows and Linux were not compiled; the platform branches use `cfg!`, so the macOS build compiles them. The PowerShell line, fish and the look are not checked in the running app under the no-visual-testing agreement.

## Editing files in the Changes pane

Recorded on 16 October 2026. The user asked for the Changes pane's diff to be editable as it is shown: typing into its lines changes the file, the green and red follow, and undo and save work as shortcuts and as buttons. A first version had a pencil button that switched to the whole file; the user rejected it, so the diff is now editable as soon as it opens and keeps its hunks.

When an editable file opens, `Shell::load_edit` asks the host for both sides (`Request::ReadEditableFile`) while the diff shows the capture. `git::editable_file` reads the old side that the snapshot's comparison uses: `HEAD` or the task's baseline or branch base through `git cat-file`, the index for unstaged changes, and nothing for a new file. It also reads the working-tree text and its SHA-256 digest. Staged changes, conflicts, binary files, deleted files, saved snapshots and review tasks stay read-only, and a file that cannot be read stays read-only without an error until its capture changes.

`ui/file_edit.rs` holds the edit: the lines with ids that stay with each line, the undo history (one step per change of lines; typing on one line within a second is one step), and a Myers line diff after the common start and end. Past 1,024 differing lines the middle shows as removed and then added. The rows are hunks with five lines of context and a header each, as in Git's capture. A line that a hunk has shown stays shown, so a line never leaves the diff while it is edited. Each line of the working-tree text is a `TextInput` in a document-row mode, which reports changes, Backspace at the start, Delete at the end, and Up and Down past its text as `RowEdit` events. A typed or pasted line break splits a row. Up and Down skip headers and removed lines. Old lines stay read-only. Line comments work while nothing is unsaved: the rows map to the snapshot's rows by their line numbers.

The title bar has Undo, Discard (undoable; shown while something is unsaved) and Save, and an UNSAVED or SAVING chip. ⌘Z and ⇧⌘Z (Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y elsewhere) undo and redo, and ⌘S saves. `Request::SaveFile` writes the text only when the file still has the digest that editing began with, so a change by the agent or another editor is not overwritten. CRLF files keep CRLF, and the final line break is kept. Edits with unsaved text live in the shell by task, repository and path, so a refresh, another file or another task does not lose them. A capture keeps the editing view. When it finds the file changed and nothing is unsaved, the file is read again: the same text keeps the edit and its undo history, and other text replaces it.

Checked with temporary tests, not kept: the line diff against a longest-common-subsequence count on 3,000 random pairs, the fallback, undo joining and redo, discard, save text with LF, CRLF and no final break, splitting typed text, hunk headers and context, lines staying shown after a change is undone, and on a real repository the old side for all changes, unstaged and staged, a save, a refused stale save, and refused paths through `..`, a folder, an unlinked repository and symbolic links. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not checked: typing, focus, IME and the look in the running app, under the no-visual-testing agreement. Tab does not insert a tab character; each line is highlighted on its own, as in the read-only diff; a hunk header has no function name.

### Faster loading, saving and typing

Recorded on 16 October 2026. The user reported a wait of 2–3 seconds before a file could be edited, slow saves, and edits that sometimes did not take.

| Cause | Change |
| --- | --- |
| The read and the save waited in the desktop's one queue of Git requests, behind the periodic capture of the changes, which runs a `git diff` for each changed file | `ReadEditableFile` and `SaveFile` run on their own thread, like the catalog and account requests |
| The host checked the file's repository with `git::repositories`, which runs three `git ls-files` over the working tree, ignored files included | The repository must be one that the snapshot captured. `SaveFile` names its snapshot |
| Until the text arrived, the diff was read-only. The editable view then replaced it, so a line that the user had clicked lost the caret | The open view takes the edit in place (`DiffView::attach`): the scroll position stays and the caret moves to the clicked line's editor. The same happens when a file changed on disk is read again |
| Every keystroke numbered each line of the changed middle through a hash map before the diff, and copied every line into a row | Lines compare as text and rows are made only for the lines shown. A keystroke in a debug build: 0.4 ms for 1,000 lines, 1.6 ms for 5,000 and 6 ms for 20,000, before 1.4, 5.5 and 21 ms |

Comments now name the capture and file of the view that they come from, which follows newer captures while the file is edited.

Checked with temporary tests, not kept: the line diff against a longest-common-subsequence count on 3,000 random pairs, the hunks, and the time of a keystroke. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. The waits in the running app are not measured under the no-visual-testing agreement.

## Browser in the side pane

Recorded on 17 October 2026. The user asked for a browser in the right pane next to the diff, with a choice between them when the pane opens, and for agents to use the same browser to open files and pages and to test their work. Research into other harnesses came first: Cursor, VS Code with Copilot, Playwright MCP and Chrome DevTools MCP give agents an accessibility-style snapshot with element refs, actions on those refs, console messages, screenshots and script evaluation, and they show the agent's page to the user. Kodem follows that model.

The title bar's pane button closes an open pane, and opens a closed one on `PanePage::Picker`: a list in the pane with Changes and Browser and their shortcuts. `PanePage` holds the choice; `Shell::toggle_pane` serves ⌘D (`ToggleReview`) and ⇧⌘B (`ToggleBrowser`). The pane headers have no switch between the pages. Paths that show changes, such as a task's diff badge or `/diff`, select Changes. The diff refreshes only while Changes is shown.

`browser::engine` (macOS) creates one `wry` WebKit view per task, the first time that the user or an agent opens a page, as a hidden subview of GPUI's view. The pane paints a `canvas`, whose paint places the task's view over its bounds; `Browsers::end_frame` at the end of each render hides every other view, and also the shown one while any panel, menu, image preview or subagent popup is open, while the pane slides, or in the Git workspace. A click anywhere in GPUI gives the keyboard back to GPUI's view, because AppKit does not move the first responder from a web view to a view that refuses it on click. A trusted pointer press in the page moves GPUI's focus to the workspace root, so that ⌘C, ⌘V, ⌘X, ⌘A and ⌘Z reach the page instead of a GPUI text field. File targets load with `loadFileURL:allowingReadAccessToURL:`, with read access to the task's folder when the file is in it.

Agents reach the browser through `kodem-host mcp`, which lists the `browser_*` tools on macOS only. Each call is one `kodem_core::browser::BrowserRequest` over the window's own socket, `browser-v1.sock` in the data directory, which `browser::server` serves. The socket accepts only processes of the same user, with `ipc::same_user`, which moved from the host to `kodem-core`. The page lives in the window, not the host, so the window serves the socket; with no window open, the tools say so. A job runs on the window's thread. A page command evaluates `browser/agent.js` (idempotent; it also runs as the page's initialization script) and a call with a random nonce. The script replies through a WebKit script message handler of Kodem's own (`Replies`), because wry's IPC channel drops the messages of `file:` pages, whose address `http::Uri` cannot hold. A command that does not start, such as `browser_evaluate` with a syntax error, fails at once. A command that arrives while its page loads waits for the load and runs again. Click, press, type and select wait 250 ms for a navigation to start and then for its load, and add the page's title and address to the reply. wry reports no failed loads, so a page counts as loading from the request for at most 15 seconds. Screenshots use `takeSnapshotWithConfiguration:`, one pixel per point, as PNG.

Checked with temporary tests, not kept: address and path parsing (localhost, bare hosts, `file:` addresses with escapes, relative files, folders with `index.html`, missing files); the socket round trip with a fake window, including the retry while a page loads and the note after a click; and the page script in a temporary headless WebKit program (a hidden `tao` window with `wry` and the same reply handler) on a `file:` page. The snapshot listed links, headings, merged inline text, form fields with values, a select with its options, a cursor-pointer card, a `contenteditable` field and a shadow-DOM button, and left out `display: none` and `aria-hidden` content. Typing, a checkbox, a select, Enter submitting a form, clicks, appending to `contenteditable`, `wait_for` on late text, a stale ref error, console and error capture with an accepted `confirm`, `evaluate` with a function, and a syntax error that does not start were all correct, and a click on a link to another file replied before the next page loaded. `kodem-host mcp` listed the tools and reported a missing window and a wrong history action as tool errors. Checked with locked `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check` on macOS, and `cargo clippy --workspace --all-targets -D warnings` for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` through zig (no findings). Not checked, under the no-visual-testing agreement: the page's placement, hiding and focus in the running app, screenshots, and Web Inspector.

`wry` adds eight packages to the notices: `wry`, `objc2-web-kit`, `objc2-exception-helper`, `dpi`, `cookie`, `dirs`, `dirs-sys` and `time-macros`. `dpi` 0.1.2 is Apache-2.0 AND MIT, with the MIT part for its copied `libm` code. Its MIT notice is in `LICENSE-LIBM-MIT`, which cargo-about does not read, so it produced the blank MIT template and the debug build stopped. `clarifications.json` now holds that file's text, identical to upstream at winit commit 587ade8, and `licenses.py refresh` updated the tracked notices and snapshot. `licenses.py check` and the debug build command passed.

### Keyboard and address field fixes

Recorded on 17 October 2026. The user reported that keys typed into the prompt could reach the page, and that a long address drew outside the address field.

| Cause | Change |
| --- | --- |
| The press that takes the keyboard back from the page was a hit-tested listener on the workspace root. GPUI skips such listeners where an occluding layer covers the press, as the composer's glass does, so a click into the prompt left the page as the first responder | The pane's paint registers a window-level mouse-down listener, which no layer hides. A change of the window's focus to an element other than the workspace root, by a click or a shortcut, also gives the keyboard back. A click on the page moves the focus to the root, so the page keeps the keyboard until then |
| `TextInput` wraps by default and has no horizontal scrolling, so a long address wrapped beyond the 24-point field | A single-line `TextInput` with wrapping off scrolls sideways to keep its caret in view, shows the start of its text while unfocused, and clips the text to its bounds. Hit testing and the input method use the same offset. The address field turns wrapping off; other inputs keep their behavior |

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the debug build command. Not checked in the running app under the no-visual-testing agreement.

### Web Inspector inside the pane

Recorded on 17 October 2026. The user reported that opening Web Inspector made the page fill the whole window, and that it stayed so. WebKit docks the inspector into the page's superview: it splits that view between the page and the inspector, and on detaching or closing it sets the page's frame to the superview's bounds. The page was a direct subview of GPUI's view, which covers the window, and the pane set the page's frame only when its own bounds changed.

Each page now lies in a host `NSView` of its own, a subview of GPUI's view. wry builds the page into the host through a window handle of the host (`HostHandle`), and the page fills the host and follows its size. The pane places and hides the host, comparing its native frame with the pane's on each paint, so a docked inspector stays inside the pane and the page returns to the pane's size when the inspector closes. Giving the keyboard back makes GPUI's view the first responder directly, because wry's `focus_parent` would now pick the host.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the debug build command. Not checked in the running app under the no-visual-testing agreement.

### Pages fit the pane's width

Recorded on 17 October 2026. The user asked that pages always fit the pane from left to right instead of being cut, as a site with a fixed minimum width and hidden horizontal overflow was.

The page script measures the width that the content needs after `DOMContentLoaded`, on load, 1.5 seconds later and after each resize: the right edge of the visible boxes, without fixed boxes, the content of boxes that clip or scroll horizontally (such as a news ticker) and boxes that start right of the view (such as a closed side menu). The document's scroll width is not used, because it counts boxes that the page hides off the view. A width wider than the view sets the page's content width, and the first measure of a document clears it. `Tab::fit` sets WebKit's page zoom to the pane's width divided by that width, between 30% and 100%, and again when the pane changes size. Page zoom lays the page out again at the wider CSS width, so a responsive page keeps its own size and a fixed layout shrinks to fit.

Checked in a temporary headless WebKit program, not kept, at 1,000 points: a fixed 1,300-pixel layout with hidden overflow and a 5,000-pixel ticker measured 1,300; a responsive page with the same ticker measured 1,000; an off-view menu and a 3,000-pixel fixed bar measured 1,000; after zooming the fixed layout to 1,000/1,300 its view was 1,300 CSS pixels and it fitted. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the debug build command. Not checked in the running app under the no-visual-testing agreement.

### The agent's cursor

Recorded on 17 October 2026. The user asked for an agent cursor with clicks, so that they see what the agent does in the page, also what it types.

The page script draws the cursor in a closed shadow tree of a `kodem-agent-cursor` element beside the body, with `pointer-events: none` and the highest z-index; it builds the element again if the page removes it. The outline and the width measure walk the body only, and hit tests ignore it. Before an action on an element, the cursor glides to the element's center in 420 ms and a frame outlines the element; a click shows a ring. `browser_type` types in at most 40 steps over about 1.4 seconds, and inserts the rest at once after 2 seconds, with a caption of the typed text (its end when long, dots for a password); it focuses the field again if the page moved the focus between steps. The caption names the element with the agent's `element` description when the tool call has one. Reading the page, the console, waiting and running a script show a caption without a move. The cursor fades after 2.5 seconds without an action, and keeps its place per site in `sessionStorage`. A hidden page acts at once, because WebKit slows its timers to about one a second, and reduced motion turns the animation off. A screenshot hides the cursor first: the script runs before WebKit draws the snapshot, which waits for the next screen update.

Checked in a temporary headless WebKit program, not kept: snapshot, typing into a field and a password, select, click, appending to `contenteditable`, Enter submitting the form, scroll and hover gave the same results as before, the snapshot did not show the cursor, the cursor element existed and `conceal` hid it. With animation, an action took about 420 ms and typing a field 0.7–0.85 s; one run in a hidden window took 8.8 s for 17 characters, which led to acting at once in hidden pages and to the two-second limit. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the debug build command. The look of the cursor is not checked in the running app under the no-visual-testing agreement.

### Google as the first page

Recorded on 17 October 2026. The user asked for google.com as the page that the browser opens by default. When the pane shows the browser of a task that has no page, it opens `browser::HOME` (`https://www.google.com/`) instead of showing the empty state. A page that the user or an agent opened is kept, and a task whose home page failed to open is not tried again on each frame (`Shell::browser_home`); it shows the error and the empty state. An agent that reads a task's page before anything opened one still gets the "no page is open" answer. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the debug build command.

### Glass on floating surfaces

Recorded on 17 October 2026. The user liked the glass of the composer and the side panels and asked for the other elements in their screenshots to take that look, with fainter dots and the dithered shadows outside kept: inline code, the button that opens an agent with its state chip, the Settings and Skills & MCP dialogs with their chosen rows and theme cards, the text, approval, mode, reasoning and model menus, and the preview of a sent message.

- `kit::floating_face` is the glass of a menu, dialog or card: five eighths of its tone (`kit::SURFACE_GLASS_FILL`, the composer's share, which moved there from `chat.rs`) over the six-point blur, a hairline edge, the floating light and faint dots along the bottom. It lies beneath the content, so the dots stay at the bottom of a menu that scrolls. `menus.rs`, the dialog in `panels.rs` and the preview in `history.rs` use it. A dialog's compact header, the Settings navigation, a theme card's name and the preview's header take half of their tone (`kit::PANE_SHARE`) without a blur of their own, because a blur over another one breaks the render pass.
- Floating notices are glass in the same share, and their wash is fainter.
- The button that opens an agent is glass in the control tone with faint dots along its bottom. In the composer's agent strip, which lies inside the composer's glass, it takes the share without the blur.
- `dither::row_wash` shades a selected row with faint dots along its bottom edge, as a message has, instead of grain from its leading edge. This changes every chosen row and chosen settings surface.
- Tinted chips carry faint dots of their tone along their bottom edge.
- Inline code is a faint fill in a hairline edge with fine dither over its lower half, in place of a plate of dither.

Each glass surface adds one blur of its own region while it is on screen. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. GPU use is not measured, and the look is not checked in the running app under the no-visual-testing agreement.

### Send transition for steered messages

Recorded on 17 October 2026. The user reported that the send transition of a new message did not play for a message steered from the queue or from the composer. "Steer immediately" in the tray only sent `SteerQueued`, so no `motion::Arrival` existed for the message. A message steered from the composer had an arrival, but it first showed as a queued row in the tray until its provider took it, and then moved into the conversation. Now `Shell::steer_queued` records an arrival that starts at the message's row in the tray (the `outgoing:{id}` anchor) and follows the end of the conversation. The tray leaves out a queued message that is bound to the running turn while its arrival plays (`Shell::steer_arriving`), so the message goes straight from the composer or the tray into the conversation. A message that its provider does not take within the arrival's three seconds shows in the tray again. Checked with a locked macOS `cargo build`, `cargo clippy -D warnings` (no findings) and `cargo fmt --check`. The transition is not checked in the running app under the no-visual-testing agreement.

### Plan tab

Recorded on 17 October 2026. The user asked for a plan view that shows the steps of an agent as it does them, for agents that always build such a plan and keep it current, placed with the composer and never making it tall.

- `provider::instructions` now returns instructions for every kind. Each ends with the plan rules, which name the provider's plan tool: `TodoWrite`, or `TaskCreate` and `TaskUpdate`, for Claude Code, and `update_plan` for Codex. A coding task had no instructions before.
- The Codex driver reads `turn/plan/updated` (in the 0.153.4 schema: `plan` of `{step, status}` with `pending`, `inProgress` and `completed`) from the main thread into `Task.plan`. Codex plans were not shown before.
- A subagent's `TodoWrite` no longer replaces the task's plan in the Claude Code driver.
- `orchestrator::apply` clears a plan whose steps are all done when a turn starts.
- The plan strip inside the composer is gone. A plan tab on the composer's top-left corner mirrors the meter tab (`paint_edge_tab` with `TabCorner`) and takes at most half of the edge: totals, a meter, a pulse and the current step, which shortens first. It opens `Panel::Plan`, a menu above the tab that numbers the steps, marks the current one and scrolls past 320 points. The tab shows while the task works, and afterwards while steps are unfinished.

Checked with temporary tests, not kept: `turn/plan/updated` through `ingest` on a scratch store replaced the plan from the main thread, skipped a step without text, ignored a subagent's and an unknown thread's report, and an empty report cleared it; a turn start kept an unfinished plan and cleared a finished one; the instructions of each kind and provider held the plan rules after the kind's rules; `PlanProgress` chose the step in progress, then the first pending one. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Whether agents follow the rules was not checked with a provider turn, and the look is not checked in the running app under the no-visual-testing agreement.

The user then reported that no model showed a plan, in build mode or in plan mode. The Debug sessions received the plan rules, but neither CLI offered its plan tool. Claude Code 2.1.296 enables `TodoWrite` and the `Task` tools for models after the 4.x family only when the launch opts in (`mz()` in the CLI: an older model, `CLAUDE_CODE_ENABLE_TODO_TOOLS`, or a plan tool named in `--allowedTools` or `--tools`). In a Codex session of the Debug app, the model searched `ALL_TOOLS` for `update_plan` and found nothing: Codex 0.162.1 offers it only with `tools.update_plan.enabled`. Kodem's sessions run Homebrew's Codex 0.162.1, not the 0.153.4 runtime. Now the Claude Code driver adds `PLAN_TOOLS` to `--allowedTools`, the Codex driver sets `tools.update_plan.enabled` in the thread configuration, and the Codex rules say that Plan mode refuses `update_plan`.

Checked with short probes on the cheapest models in an empty scratch folder, not kept: through `codex app-server` on 0.162.1 with an ephemeral, read-only thread, the model answered "missing" without the setting; with it, it called `update_plan` and the server sent `turn/plan/updated` with `{step, status}` and `inProgress`, the shape that the driver reads. Claude Code's `init` event for Opus 5.5 listed no plan tool without the opt-in, and `TaskCreate`, `TaskGet`, `TaskList` and `TaskUpdate` with it. A Haiku turn returned `{"task":{"id":"1",…}}` from `TaskCreate` and sent `taskId` and `status` to `TaskUpdate`, as `add_step` and `update_step` expect. Checked again with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the debug build command. Not checked in the running app under the no-visual-testing agreement.

The user then reported that Codex showed its plan and Claude Code did not. The Debug sessions of Claude Code started with the plan tools allowed and the rules appended, but Sonnet 5.5 never loaded `TaskCreate`: the task tools are deferred behind `ToolSearch`, their own description keeps them for complex work, and the rules named `TodoWrite` first, which this CLI does not offer. With the user's two-step prompt (search, then a short story) in an empty scratch folder, the first rules gave no plan in 1 of 1 runs, and a version that named the tools and the `ToolSearch` query in 1 of 2. Requiring a plan for every request of two or more steps gave a plan in 3 of 3, but no step in progress and both steps completed at once. `CLAUDE_PLAN_RULES` adds the order of calls, and gave in progress, the work, completed and the next step in progress in 3 of 3 Sonnet runs and in an Opus run in each of the default and plan permission modes. The Codex rules did not change. Checked again with `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the debug build command.

### Import of Claude Code and Codex threads

Recorded on 18 October 2026. The user asked for a way to find the threads of Claude Code and import them into Kodem from Settings → Workspace: a dialog that imports one thread, a set of chosen threads or all of them, and shows the progress. The user then asked for Codex threads too, on macOS, Windows and Linux.

- `Request::ExternalThreads` lists the conversations of each Claude Code and Codex account whose folder is the folder of a project, with `ExternalThreads` (threads, the number in other folders, and the accounts that could not be read). `Request::ImportThread` imports one and answers with `Response::Imported`. The protocol is 39. The bridge sends both requests on threads of their own, so Send and Stop do not wait for them.
- Claude Code: `claude_import.rs` reads the JSON Lines files under `projects` in the account's configuration directory. A listing takes the folder, the first time and the first prompt from the start of a file, and only short lines after that, for the `custom-title` and `ai-title` entries. A thread without a prompt of the user is not listed. A prompt is a `user` entry that is not meta, a compact summary, a tool result or a notice of the CLI (`origin.kind` other than `human`, `promptSource: system`, `<local-command-…>`, `<task-notification>`). A slash command shows as its name and arguments.
- Codex: `codex_import.rs` asks the account's app-server, so it does not depend on Codex's files. `thread/list` with `useStateDbOnly` answered for 520 threads in 0.04 s, against 3.9 s when Codex reads its files again. A release that refuses the field lists without it. Agent threads (`parentThreadId`) and ephemeral threads are left out.
- A folder matches a project by its canonical path. On Windows the comparison ignores letter case and the slash direction. Threads in Kodem's data folder and in the temporary folder that do not match a project are not counted as importable.
- The import (`imports::import`) creates a coding task in the project's own checkout. The task holds the provider's conversation as `provider_thread`, so the next message resumes it. The task has the account of the thread, the model of its latest answer when the catalog lists it, the thread's title and times, and `Idle` state. The task is stored before the history, a second import of the same conversation is refused, and a failed import deletes the task again. Afterwards the plan, questions, activities and requests are cleared, and agents that did not report are `Completed`.
- History: Claude Code entries replay through `Transcript`, as reconcile does. Prompts become user rows. `compact_boundary` becomes the compaction notice. The files of agents (`<session>/subagents/agent-<id>.jsonl`) replay into the agents' transcripts. `Transcript::at` dates the rows and tool durations with the times of the entries. `Transcript::conclude` closes open items. A file changed within the last 15 seconds is refused, because the CLI may still write it. Codex: `thread/turns/list` and `thread/items/list` per turn, ascending, 20 at a time. User messages become user rows. Other items go through `ingest` as `item/completed` without `questions`. After each turn the final answer moves from `/completion` into the conversation.
- The dialog (`ui/thread_import.rs`, `Panel::ImportThreads`) groups the threads by project. Each row shows the provider, the title, the age, the account (when the provider has more than one), and a state: waiting, importing, in Kodem with an Open button, or failed with the reason. "Choose all", "Import chosen" and "Import all" start a run. The runs import one thread after another, with a meter and "Importing n of m", and "Stop after this thread" ends the run early. The state is kept when the dialog closes.

Not imported: images and other attachments of prompts, older Claude Code agents stored in the main file (`isSidechain`), and the conversations of Codex agents, whose rows stay empty. A conversation that Kodem started and then replaced, such as after undoing a turn, can be listed as importable.

Checked with temporary tests, not kept, on a scratch store with the user's own stored conversations, read only: a listing found 59 Claude Code and 26 Codex threads in two projects, 541 in other folders and no errors, in 1.0 s in a debug build. Three Claude Code imports (one with one agent, one of 18 MB with five agents and a compaction) took 0.25–0.85 s. The imports had user, assistant and tool rows, no running or incomplete row, timed tools, agent rows in each agent's transcript, completed agents, `Idle` state, no activities and no rows left in `/completion`. Two Codex imports took 0.6–0.8 s with the same checks. A second import, the session that was still written and an identity with `../` were refused. Prompt parsing (text blocks, slash commands with and without arguments, notices, meta entries, tool results) and timestamps with fractions and offsets were correct. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the debug build command, and with `cargo clippy --workspace --all-targets -D warnings` for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` through zig (no findings). Not checked: a resumed turn of an imported thread, because it would add to the user's real conversations, and the dialog in the running app, under the no-visual-testing agreement.

### OpenCode provider (beta)

Recorded on 18 October 2026. The user asked for OpenCode support with the features that Claude Code and Codex have, marked as beta.

- `Provider::OpenCode` (label "OpenCode", `beta()`), with `Tool::OpenCode` and its installer (the official script on macOS and Linux, `npm install --global opencode-ai` on Windows). `account_bound()` keeps a started Codex or OpenCode conversation on its account in the host and the model picker. Added accounts set `XDG_DATA_HOME`.
- The driver, the transcript, reconciliation, the catalog, resources, the composer catalog and the import are in `providers/opencode*.rs`; [providers.md](providers.md) has the feature list and the evidence. Kodem's MCP tools accept `opencode` as a provider.
- OpenCode updated itself from 1.18.35 to 2.0.32 during the work. A probe server that was started by hand without `OPENCODE_DISABLE_AUTOUPDATE` is the likely cause. OpenCode 2 replaced the HTTP API, so the driver was rewritten for it and refuses 1.x. Kodem's servers now also set `update: disable`.
- `kodem-host opencode-guard` runs each server and stops it when the host ends, because a test showed that a killed host left `opencode serve` running.
- The desktop marks OpenCode with a BETA chip in the provider rows of Settings, the model picker, the setup dialog and the accounts tabs, and no longer lists it as coming soon. The provider row of Settings puts the sign-in state under the name, so three providers fit in one row. The model picker's provider column is 168 px wide, so the BETA chip fits beside "OpenCode".
- Settings → Workspace → Import threads lists and imports OpenCode sessions too. An imported question shows the user's answer.

Not available with OpenCode 2: the plan tab, because OpenCode 2 has no plan tool, and usage limits.

Checked with a temporary IPC client and scratch hosts, not kept, as [providers.md](providers.md) lists. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check`, the debug build command, and `cargo clippy --workspace --all-targets -D warnings` for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` through zig (no findings). The look of the BETA chips and the provider rows was not checked in the running app, under the no-visual-testing agreement.

### Default compaction threshold

Recorded on 18 October 2026. The user asked for a default compaction threshold of 60% for all providers. `CompactRule::default()` now has a threshold of 60 instead of 80. The settings store keeps the rule that it saved, so the new default applies where no rule is saved yet, such as a new installation; a saved rule, and the rules of providers and models, do not change. Checked with a locked macOS `cargo build` and `cargo fmt --check`.

### Import lists only threads of other apps

Recorded on 18 October 2026. The user reported that the import dialog listed conversations that Kodem runs, such as Claude Code threads of tasks in another Kodem build, and that the list was hard to read.

- A conversation is Kodem's, and is not listed or imported, when its provider records that Kodem started it, or when a task of any Kodem build on this computer holds it. Codex: the thread's `originator` is `kodem_ade` (`providers::codex::CLIENT_NAME`, the name that Kodem gives Codex). Claude Code: the session's entry point is `sdk-cli`, the print mode in which Kodem runs the CLI; the terminal, desktop and IDE apps and the apps built on the SDK record `cli`, `claude-desktop`, `claude-vscode`, `sdk-ts` or `sdk-py`, so other scripted `claude -p` runs are hidden too. OpenCode: Kodem now tags each session that it creates with `metadata.kodem`. The stores: `ipc::install_dirs` names the data folders of the release and Debug builds, and `storage::provider_threads` reads each one without changes. A direct import of such a conversation is refused.
- `ExternalThread` no longer has `task_id`; the dialog remembers the tasks that it imported itself.
- Sessions that OpenCode 1 left with a default title ("New session - …") are named by their first prompt.
- The dialog has tabs by provider with counts, a project filter when threads come from more than one project, groups by age (last 24 hours, 7 days, 30 days, older), and one line per thread: the choice, the provider, the title, the account when the provider has more than one, the project when the list mixes projects, and the age. The thread identity is not shown. One action imports the chosen threads, or every shown thread when none is chosen.

Checked with a temporary IPC client and a scratch host, not kept, with the user's projects `kodem-ade` and `liquid` and the stores of both builds read only: 30 threads in 1.3–1.8 s (5 Claude Code from SDK apps, 23 Codex from Codex Desktop, Synara, T3 Code, Zuse, Realm and the Codex SDK, 2 OpenCode), none held by a Kodem store and none from `kodem_ade` or `sdk-cli`; this session's own thread was not listed, and its direct import was refused. Before the change the dialog listed 56 threads for `kodem-ade` alone. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (the change has no platform code) and the debug build command, because the user's Debug app was running. The new list was not checked in the running app, under the no-visual-testing agreement.

### More depth in the interface

Recorded on 18 October 2026. The user asked for more depth in all interface elements, in the same look. The change is in the shared light of the theme and the kit, so every screen that uses them follows. The fourteenth revision of [dithered-interface.md](../design/dithered-interface.md) records the rules. It keeps the rule against blurred glows and gradients on surfaces: no soft drop shadows were added, and cards and messages keep a flat fill.

- Theme: the shadow tone is stronger (light themes 0.07 → 0.1, dark themes 0.3 → 0.4) and the top highlight of dark themes is a little brighter (0.045 → 0.06). `Theme::floating()` adds a dark line inside the bottom edge of menus, dialogs and popovers. The dithered shadow under floating surfaces is denser (peak cover 0.62 → 0.72).
- Kit: `raised_fill` lights its top and darkens its bottom more (4% → 7%) and `raised_light` has a deeper bottom line, for raised buttons; `tint_light` has stronger lines and shadow, for chips, icon tiles and chosen rows; `plate_light` on a grounded plate adds a dark line inside the bottom edge. The new `recessed_light` casts a soft shadow inside a well and lights the lip below it.
- Cards and user messages take the light of a grounded plate, and the composer the top highlight. Content blocks and text fields are recessed; a prose table is not.
- A pressed neutral or tinted button, and a button that opens an agent, sink into the surface with the recessed light instead of only losing their light. A ghost button shows the top highlight under the pointer.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. The look was not checked in the running app, under the no-visual-testing agreement.

### Browser tools under Workspace access

Recorded on 18 October 2026. The user reported that the browser does not work with the Workspace approval setting, and asked that Kodem tell the user that it needs another setting and offer to switch it, and that agents use Kodem's browser for browser work unless the user names another method.

- Cause: under Workspace access (approval policy `on-request`), Codex asks the client to approve each call of an MCP tool with `mcpServer/elicitation/request` (`_meta.codex_approval_kind` is `mcp_tool_call`). Kodem answered every request that it did not know with an error, so Codex reported "user rejected MCP tool call" for each `kodem` tool, the browser tools among them. Under AI auto-approve and Full access, Codex 0.162.1 runs the tools without asking.
- A Kodem tool approval is now an approval in the composer, "Approval setting needed", with the tool, its arguments and three decisions: "Switch to AI auto-approve", "Allow once" and "Decline". `InputRequest::is_kodem_tool_approval` recognizes it, only for the `kodem` server; other elicitations are still refused. The host turns the decision into Codex's elicitation answer (`providers::codex::kodem_tool_answer`).
- The switch saves AI auto-approve as the task's access and as the remembered approval rule of its model, as the access menu does. Codex keeps the approval rule of a running turn, so the answer also approves the tool for the session (`_meta.persist = "session"`, which Codex keeps per tool), and the `Codex` handle approves the later Kodem tool calls of that turn without asking again.
- The standing instructions of every provider session with Kodem's tools (`provider::instructions`) and the MCP server instructions now tell the agent to use Kodem's browser for browser work, unless the user names another browser or method. A delegated task has no Kodem tools and does not get the rule.

Checked with a direct `codex app-server` probe and a stub MCP server (Workspace refused the tool; AI auto-approve and Full access ran it; a session approval covered later calls of the same tool only), a temporary unit test of the request helpers, and a scratch host driven by a temporary IPC client: a Codex chat in Workspace access showed one approval, the switch set the task and the remembered rule to AI auto-approve, and both `browser_open` and the later `browser_snapshot` reached Kodem's tool server in the same turn. None of them were kept. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (the change has no platform code). The approval card was not checked in the running app, under the no-visual-testing agreement.

### Browser on Windows and Linux, modern pages and the keyboard

Recorded on 19 October 2026. The user asked that the browser work on Windows and Linux as it does on macOS, that sites show the pages that other browsers get instead of outdated ones, and that keys typed into the composer stop going to the page.

- Outdated pages: wry's WKWebView sent WebKit's user agent, which has no `Version/… Safari/…`, so Google and other sites served the page that they serve unknown browsers. The macOS page now sends Safari's user agent with the version of the installed Safari (`/Applications/Safari.app`), or the version that the macOS release ships when Safari cannot be read.
- Keyboard: a click in a frame of the page, the docked Web Inspector and focus that the page took in another way gave the page the keyboard without telling the window, so the composer kept its caret while the page took the keys. `browser/focus.js` now runs in every frame and reports the user's presses and the page's focus. The window moves its focus off its fields only when the platform confirms that the page has the keyboard (`Browsers::has_focus`: macOS's first responder, WebView2's focus events, the page's key events on X11), so a late message cannot take the composer's focus after the user clicked back into the window. A new page no longer takes the keyboard when it opens (`with_focused(false)`), which wry did on Windows and Linux.
- One engine: `browser/engine.rs` holds the pages, the commands, the fit and the screenshots of every platform. `browser/macos.rs`, `browser/windows.rs` and `browser/linux.rs` hold how a page joins the window, its message handler, file pages, the capture and the keyboard. History steps use wry's `can_go_back`/`go_back` on every platform. `unsupported.rs` is gone; `Browsers::unavailable` gives the reason that the pane shows when a computer cannot show pages.
- Windows: the page is a WebView2 child window. GPUI's DirectComposition target was topmost, above child windows, so `vendor/gpui_ce_windows` (`gpui_ce_windows` 0.1.0 through `[patch.crates-io]`) creates it behind them; only `directx_renderer.rs` differs, and states its change. The pages keep their data in `Browser` in Kodem's data directory, because WebView2's default folder beside an installed program cannot be written. Messages arrive over `chrome.webview` and `WebMessageReceived`, focus over the controller's `GotFocus`/`LostFocus`, which also cover frames, and `GotFocus` moves the page's popups to where the window is now. Screenshots use `CapturePreview`, scaled to one pixel per window pixel. File pages open as escaped `file:///C:/…` addresses; the address field reads them back.
- Linux: the page is a WebKitGTK X11 child window that wry builds from GPUI's XCB window id. wry embeds on X11 only, so `browser::prefer_x11`, first in `main`, moves the window to XWayland in a Wayland session that offers X11; the session's display waits in `KODEM_WAYLAND_DISPLAY`, and `ipc` gives it back to the host, so agents and terminals keep Wayland. GTK opens on X11 only, without setting the process locale, and the window runs GTK's waiting events every 8 ms. X11 sends a key to the window under the pointer, so the page takes key events only from the user's click on it until the window takes the keyboard back; otherwise its keys pass on to GPUI's window. Builds need `libgtk-3-dev` and `libwebkit2gtk-4.1-dev`, which `scripts/dependencies.sh`, the package requirements and both workflows now name.
- The host lists the browser tools on all three platforms. A Linux window without an X11 display answers them with the reason.
- Third-party notices: refreshed for macOS (393 packages). The Windows (381) and Linux (551) notices that builds generate pass the policy; `webview2-com`, `webview2-com-sys` and `webview2-com-macros` 0.39.1/0.8.1 ship no license file, so `licensing/clarifications.json` holds the MIT text of webview2-rs at `f55627c`.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`, and with `cargo clippy --all-targets -D warnings` cross-checks through zig: Linux (`x86_64-unknown-linux-gnu`, whole workspace, against stub pkg-config files for GTK and WebKitGTK) found nothing, and Windows (`x86_64-pc-windows-msvc`) found nothing in `kodem-desktop` and `kodem-core`. The whole Windows workspace stops at `clippy::large_enum_variant` on `Catalog` in `kodem-host/src/providers/codex.rs`, which the tree before this change reports too. Not run: any Windows or Linux build that links or starts, so WebView2, WebKitGTK, XWayland and the key handling on X11 are not checked at run time; the pages were not checked in the running app, under the no-visual-testing agreement.

### Work trail look and motion

Recorded on 19 October 2026. The user asked that the work trail (the parent tray of a stretch of work, its runs, thoughts and calls, and the running state) look better and fit the app, and that it animate when it appears, opens and folds.

- The parent plate reads `WORKED` (`WORKING` with a moving highlight and the pulse while a call runs), then a tally: the glyph of each kind of work and its count, instead of the long caps sentence. Its dotted shade and rule are lighter. The full sentence stays the plate's accessibility label.
- Runs and thoughts in the well are sentence-case lines with a glyph: the kind of a run's calls (`list-ordered` for a mixed run), or the thought's sparkles. They have no dotted leader. A running run names the call that it makes now with a moving highlight, in place of its time.
- Calls hang from a dotted rail under the chevron, with their glyphs under the run's glyph and their verbs in one column. A running call moves a highlight over its title (`Starting…` while it has none) instead of showing a `RUNNING` chip. An open call keeps a faint fill.
- Motion (`motion::Folds`, `motion::fold`): each foldable part compares its state with the frame before, so a click and a tray that follows its work both slide. The clip grows or shrinks to the content's measured height while the content fades and drifts, and inside a well a Bayer dissolve in the well's color clears from it. Chevrons turn with the fold. A row or a call that started in the last 700 ms grows into place the first time that it shows; a row that scrolls back into view does not play it again. A fold by hand measures its rows again when it ends, for rows out of view. A part counts as moving while its change runs, not by its openness: the first frame of a fold is still fully open, and it must ask for the next frame, or the fold waits for a pointer event and lands folded without its motion. Reduced motion shows every state at once.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (the change has no platform code). The look and the motion were not checked in the running app, under the no-visual-testing agreement.

### Composer skater

Recorded on 19 October 2026. The user asked for a cute pixel avatar for each project that skates on the top edge of the composer: idle while the task rests, skating with ollies over the tabs, tricks and rare falls while it works, falling off when the conversation scrolls away from its end and coming back at the end, and a grab-and-throw with the pointer. It is on by default and has a switch in Preferences.

- Model: `SkaterAvatar` has 12 avatars (Cat, Bunny, Bear, Fox, Frog, Duck, Penguin, Dino, Robot, Ghost, Slime, Alien). `Project::avatar` is optional in the JSON. `Store::add_project` picks a random one, and `Store::open` gives a random one to each project that has none, once (`assign_project_avatars`). `CustomizeProject` carries the avatar; without one the project keeps its own. `Settings::skater` turns the character on (default on).
- Picker: the project dialog has an Avatar row with a still portrait of each avatar in the project's accent. Preferences → Appearance → Interface has the "Composer skater" switch.
- Layer: `ui/skater` is its own view on the window root (`Stage`), between the workspace and the Git overlays, so the cached workspace does not render again for a frame of the character. The workspace clears the scene in each render (`skater::begin_frame`), names the avatar, the project's tint, the task's work state, whether the conversation shows its end and whether a menu covers the composer (`skater::cue`), and marks the composer's box and its plan and meter tabs as ground (`skater::ground`). A conversation keeps 40 points clear above the composer for the lane (`skater::ROOM`).
- Character: a simulation with gravity runs on the layer's frames. Modes: enter (a leap with a flip from under the window), idle (breath, blinks, looks around), doze after 14 s (sits, z's), ride (pushes, coasts, brakes to a stop), kick turn at each end, ollie up onto a tab over its whole height, a roll down or a trick off a tab, flat tricks (ollie, kickflip, shove-it, spin, grab, manual), a bail on 2–7 % of jumps (tumble, dizzy stars, get up, walk to the board, hop on), panic (a `!`, flailing fall out of the window while the conversation is scrolled away), held (a pendulum from the pointer) and flung (keeps the pointer's speed, bounces off the window's sides, falls out of the window and comes back after 1.4 s while the end shows). Moving modes use the display rate; idle and doze use 12 frames per second; hidden, covered and off use none. Reduced motion shows a still character without the pointer grip.
- Art (`ui/skater/art.rs`): 12×12 sprites per avatar, shared legs, arms, eyes and a board with four flip faces, painted cell by cell, mirrored, rotated and squeezed, in colors from the project's tint.

Checked with temporary tests that were not kept: the ground profile of the edge and both tabs; ten minutes of simulated frames with work, rest and a scroll away (12 runs with random seeds) reached every mode, kept the deck on the ground and in the lane, and reached the tops of both tabs; a grab, drag and throw left the window and came back; a store with a project saved without an avatar got one on open, kept it over later opens, kept it on a customization without an avatar and changed it on one with an avatar. The sprites were checked as a rendered sheet of the art data, not in the app. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (the change has no platform code). The character was not checked in the running app, under the no-visual-testing agreement.

### Workspace rail look and motion

Recorded on 19 October 2026. The user asked that the tiles of the workspace rail (Agentic Coding, Chat, Review and Git, with the selection edge and the BETA badge) look better and fit the app, and that they animate when they appear, open and fold.

- The tiles share one builder (`Shell::rail_tile`, `Shell::rail_cell`) with 8-point corners. The open workspace's tile keeps the accent tint and light, and its dots are a radial bloom under the glyph (`DitherField::Glow`) instead of a flat bottom shade. The other tiles take a neutral fill and a hairline edge under the pointer.
- The selection is a three-point glowing pill on the rail's leading edge, centered on the tile. `RailSlide` keeps the slot of the last frame; `Shell::sync_rail_slide`, called once per render, starts a slide when the slot changes, so a click, a shortcut or a task of another kind all slide the pill. It uses the stretching slide of the Git view tabs, which moved to `fx::stretch_span`.
- The tile that opens forms out of dither as before, and its glyph springs from 60% to full size. The tile that the selection left lets its tint fade over 280 ms.
- The attention marker is a round dot with a ring in the rail's tone. It springs in and sends out one fading ring when its state (waits, works, ready) changes. It draws no frames once it has settled.
- The Git tile's BETA badge is a small mono pill centered under the glyph and hanging from the tile's lower edge: outlined in the accent while Git is closed, filled while it is open, rising into place when that changes.
- `motion::stagger` drops parts into place one after another. The quick actions that the rail gains once the sidebar has folded, and the Git workspace's repositories, use it.

Reduced motion shows every state at once. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (the change has no platform code). The look and the motion were not checked in the running app, under the no-visual-testing agreement.

### Thread feed selection and fold motion

Recorded on 19 October 2026. The user asked that the thread feed in the sidebar get the motion of the workspace rail.

- Selection: `FeedSlide` keeps the task that the last frame marked; `Shell::sync_feed_slide` starts a slide when another task of the feed opens, from the edge of the row that was open, read from its `fx` anchor relative to a new anchor on the sidebar. `Shell::slide_edge` draws the edge over the sidebar on its way, with `fx::stretch_span`, toward the new row's anchor of the last frame, so it follows a scroll. The new row draws its own edge when the slide arrives; until then its tint and dots grow with the slide, and the row that was left fades its tint. A selection that changes workspace, or whose rows were not painted on the last frame, does not slide.
- Folds: `GroupFold` times a project group or the settled list between folded and open. Rows move one after another from the top, 35 ms apart up to the eighth row, each in 260 ms. Rows of a project group use `motion::row_enter` (grow, slide, dissolve), forward to open and backward to fold; the group joins `Settings.collapsed_projects` only when its last row has folded (`Shell::settle_folds`). The feed measures those rows again on each frame, as it does for new rows. The settled list is a uniform list whose rows keep their height, so they use the new `motion::drop_in`. A click during a fold turns it around.
- Chevrons turn with the fold. `Icon::rotate` turns an icon about its center, and `Shell::ghost_icon` builds a ghost button around a shaped icon.
- `motion::next_frame` keeps frames coming while a slide or a fold runs, so rows out of view still finish.

A search, which shows every group open, and reduced motion fold at once and do not slide. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (the change has no platform code). The look and the motion were not checked in the running app, under the no-visual-testing agreement.

### Fast mode and heavy reasoning levels

Recorded on 19 October 2026. The user asked that the reasoning slider show in red the levels that burn usage much faster and how much more usage they can take, and that the composer get a fast mode toggle for the harnesses that have one, with an energy icon after the model picker and before the reasoning chip, which says that fast mode uses more usage and is not red.

- Fast mode: `Model.fast` (`FastMode` with the provider's identity and description of the mode), `Task.fast`, `AgentPreferences.fast` and `TaskEdit::Fast`. The host saves the choice as the next task's option, like the model. The drivers apply it as `providers.md` describes: the `fastMode` flag setting for Claude Code, the `serviceTier` of `turn/start` for Codex, and the `-fast` model of OpenCode. A delegated task starts without it.
- Composer: a square `zap` chip between the model chip and the reasoning chip, shown only for a model with fast mode. On, it takes the accent tint. Its tooltip gives the state, the provider's description of the speed-up and that fast mode uses more of the plan's usage. It is locked while the agent options are (`Shell::agent_options_locked`, shared with the agent menu).
- Slider: `xhigh`, `max` and `ultra` are heavy levels (`effort_slider::heavy_usage`). The track has a red zone from halfway before the first heavy stop, heavy stops have red ticks, and a heavy selection turns the fill and the level name red and shows "Up to ~2×/~4×/~8× the usage of high". Neither CLI reports a usage figure for a level through its protocol, so the multipliers are Kodem's rough upper estimates.

Checked against the installed CLIs without a model call: Claude Code 2.1.296 lists `supportsFastMode` among the handshake's model fields and reads `fastMode` from the flag settings; Codex 0.162.1 `model/list` returns a `priority` tier named `Fast` for every model, and its schema types `serviceTier` of `turn/start` as a string or `null`; OpenCode 2.0.32 builds `<id>-<mode>` models from `experimental.modes`. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: a fast turn on any provider (it uses the account's extra usage), and the Linux and Windows lint (the change has no platform code). The look was not checked in the running app, under the no-visual-testing agreement.

### Nested scrolling chains the wheel

Recorded on 19 October 2026. The user reported that every inner scroll area also scrolled the area around it: the diff of an edit scrolled the chat, and the icon list of the project settings scrolled the whole dialog. The main area should scroll only once the inner area reaches its start or end. Sideways scrolling should follow the platform, Shift with the wheel on macOS.

The cause is in GPUI: each scroll container under the pointer applied the whole wheel delta and none of them took it, so all of them moved. GPUI's `list` also handled the wheel before the containers inside its items. A container that scrolls only sideways turned a plain vertical wheel into a sideways scroll, so a code block in the chat caught the wheel that was meant for the chat. The platforms already report Shift with the wheel as sideways input: AppKit on macOS, and GPUI's Windows and Linux backends.

Kodem now patches GPUI itself: `vendor/gpui_ce` is `gpui-ce` 0.2.2, wired in through `[patch.crates-io]` like the two platform crates. Only `gpui.rs`, `window.rs`, `style.rs`, `elements/div.rs`, `elements/list.rs` and the new `scroll_chain.rs` differ from the published crate. The examples and tests of the crate are not copied.

| Before | After |
| --- | --- |
| Every container under the pointer scrolled by the same delta | Each axis goes to the innermost container that can still move along it (`Window::take_scroll_delta`); the containers around it see the axis as taken |
| A flick that reached the end of an inner area carried on into the page | The container that took an axis keeps it for the gesture while the pointer is over it (`Window::note_scroll_container` in the capture phase). A gesture ends at a new touch, or after the wheel rests for 300 ms with no fingers on the touchpad |
| `list` saw the wheel before the scroll containers of its items | It registers its listener before its items paint |
| A container that scrolls one axis took the other axis's input unless it set `restrict_scroll_to_axis` | Input is never moved to another axis: a plain wheel scrolls vertically, Shift with the wheel, a tilt wheel or a sideways swipe scroll sideways |

The two wheel handlers of Kodem that scroll by hand take their delta the same way: the chat's stage, which scrolls the conversation beside and below the list (`ListState::scroll_container_id`), and the sideways scroll of unwrapped diff rows (`VirtualListScrollHandle::scroll_container_id`).

Temporary tests (removed) in a scratch crate on the patched GPUI checked the chain's rules and real dispatch through `VisualTestContext`. In a `list` an inner block took the wheel until its end, the list stayed during that gesture and took the next one, and scrolling back went to the block first. A sideways block in the list took Shift-like input only, and a plain wheel over it scrolled the list. Nested `div`s chained the same way, a resting finger kept the gesture, and a mouse wheel after a rest moved to the outer container. They found that `Pixels` orders -0 below 0, so the bounds check of a container compares floats. Third-party notices: refreshed, `gpui-ce` is marked as modified, 393 external packages. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and rustfmt on the patched files. Not run: the Linux and Windows lint (the patch has no platform code). The feel was not checked in the running app, under the no-visual-testing agreement.

### Provider CLI update card

Recorded on 19 October 2026. The user asked for a small blurred card on top of the window that offers updates of the providers' CLIs, after a screenshot with the title "Update Available: Claude v2.1.303", a close button and the actions Settings and Update, in Kodem's own components. After a first version, they reported that Update did not update the provider, that not every provider was found, that the update must show its progress in the card instead of opening a terminal, and that the card must show several updates at once.

Causes of the first version's failures:

- Discovery compared only runtimes that had already connected, so a provider without a connection had no installed version.
- Claude Code: the install script downloads the newest build and then runs `claude install` without a channel, which installs the default channel. Its `stable` was 2.1.292, older than the installed 2.1.296, so nothing changed.
- Codex: the installer did install 0.163.0 into the runtimes folder, but Codex 0.163.0 packages no longer carry `codex-resources/zsh` on macOS or Linux. `PACKAGE_FILES` and `scripts/dependencies.sh` required it, so Kodem rejected the package and kept Homebrew's 0.162.1. Kodem never starts that zsh. Both checks now leave it out; Linux still requires `bwrap`, and the Windows package still has `codex-command-runner.exe`.
- OpenCode: npm's `opencode-ai` and `opencode.ai/install` serve OpenCode 1 only, and Kodem needs OpenCode 2.

What the card does now:

- Discovery (`harness.rs`): each CLI that Kodem runs reports its version: `kodem_core::providers::claude::version` (the Claude executable lookup moved from the host into core), `codex::version` and `opencode::version`. The newest release comes from the channel that the CLI's own update installs: `downloads.claude.ai/claude-code-releases/<channel>` with `autoUpdatesChannel` of `~/.claude/settings.json` (`latest` by default), npm's `@openai/codex`, and OpenCode's update service `opencode.ai/update/api/latest/cli/npm`. It runs at start and every six hours, outside the UI thread.
- Updates run on their own threads, without a terminal. Claude Code runs `claude update`. OpenCode runs `opencode upgrade <version>`. Both use Kodem's tool PATH, and the card shows each line that the command prints. Codex downloads the latest GitHub release package with byte progress, checks its SHA-256 digest, unpacks it with the system `tar` and moves it into the runtimes folder. Afterwards Kodem reads the CLI's version again. A CLI that is still older fails with the command's last line. A finished update asks the host to read the provider's runtime again.
- Card (`ui/harness_updates.rs`): one glass card with the blur and depth of the notice cards, at the top of the floating notices column. It lists every provider with a newer release, or with an update that runs or ended. Each row has the provider's mark, which pulses while it updates, the installed and the newest release, the current step with a meter and a percentage when the step can measure it, then "Updated to …" or the error with Retry. Each row has its own Update button; "Update all" appears below the rows when two or more can update. The first version's Settings button was removed at the user's request. Close hides the offers and the finished rows until a newer release appears; it is hidden while an update runs.

A temporary test (removed) checked the version parsing, the output cleaning, and a streamed command with its last line and its failure. It read the installed releases (Claude 2.1.296, Codex 0.163.0 after the package fix, OpenCode 2.0.32) and the newest (Claude 2.1.303, Codex 0.163.0, OpenCode 2.0.32). It downloaded the macOS Codex package (133 MB, 14 progress steps) with its digest matching, and a wrong digest was refused. The Linux and Windows 0.163.0 packages were listed for their required files. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and `bash -n scripts/dependencies.sh`. Not run: a real `claude update` or `opencode upgrade`, because they would change the installed CLIs, and the Linux and Windows lint. The look was not checked in the running app, under the no-visual-testing agreement.

### Skater frame rate, sprite sort and filter-group textures

Recorded on 19 October 2026. The user asked to measure the running Nightly release build and to fix clear wins. Passive measurements on the Mac Studio (M1 Max, 4K displays at 144 Hz), while an agent worked in the open task: `top` showed about 38% CPU; `footprint` 478 MB, of which 189 MB IOAccelerator, 96 MB unmapped graphics memory and 94 MB IOSurface; the IORegistry's `accumulatedGPUTime` gave Kodem 55–56% of the GPU, and its `lastSubmittedTime` changed about 198 times a second. A `sample` of 8 seconds found the main thread in `Sim::step_ride` of the skater, 37% of its time waiting in `nextDrawable` for a free drawable, and the stable sort of monochrome sprites in `Scene::finish` as the largest self time of Kodem's code (about 5%).

| Cause | Change |
| --- | --- |
| A moving skater asked for a frame on every display refresh, and each frame draws the whole window again: the scene replays, sorts and goes to the GPU with its blurs, 144 times a second on these displays | `skater::pace` notifies the layer on the first refresh at least `SMOOTH_FRAME` (11 ms) after its last frame. On displays up to 90 Hz the motion still takes every refresh; above, every second refresh (72 frames a second at 144 Hz, 60 at 120 Hz). The frames stay on refreshes, so the motion does not judder |
| `Scene::finish` sorted the sprites, 112 bytes each, with a stable sort. A scratch benchmark of 9,000 glyph-field sprites and 3,000 text sprites took 0.51–0.60 ms stable and 0.35–0.39 ms unstable | `vendor/gpui_ce/src/scene.rs` sorts sprites with `sort_unstable_by_key`. Sprites with the same order and tile draw the same image in one layer |
| The Metal renderer created two full-window textures for content-filter groups on every resize, about 66 MB at 4K, and Kodem uses only backdrop filters | `vendor/gpui_ce_macos/src/metal_renderer.rs` creates them on the first scene with a content filter, and drops them on a resize |

What looks different: on displays faster than 90 Hz the skater moves at half the refresh rate.

Not changed: the glyph field's rates, the glass blurs, and the three drawables of the Metal layer (94 MB at 4K), whose reduction could stall frames. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and rustfmt on the patched files. Not run: the Linux and Windows lint (the changes have no platform code outside the macOS renderer). The new build was not measured, because restarting the app would end the session that ran in it; the look is not checked under the no-visual-testing agreement.

### Frames that copy, and a blink that waits

Recorded on 19 October 2026, after `bb131fb`, on the same running Nightly build. A second `sample` of 10 seconds, taken while an agent ran a tool, found the main thread in `Window::draw` 57% of the time: the shell rendered, laid out (taffy, 16%), prepainted and painted again, though nothing in it streamed. Every animated glyph inside the shell, such as the working pulse of a task row or a tool call, notifies the shell on each 80 ms step of `dither`'s clock, and GPUI renders a cached view and every view inside it again when it is notified. `footprint` grew from 478 to 525 MB over about 20 minutes, in the glyph atlas and malloc; nothing pointed to a leak.

| Cause | Change |
| --- | --- |
| For one second after input at 60 events a second or more, such as a moving pointer, GPUI presents the unchanged scene again on every display refresh, and the Metal renderer drew it again in full, glass blurs included | Each finished `Scene` takes a `generation`, unique across windows (`vendor/gpui_ce/src/scene.rs`). When the renderer is asked for the generation and size that it drew last, and that frame went through the offscreen target, it copies the target into the new drawable (`MetalRenderer::drawn_scene`). Any other drawing into the target, a resize or a change of transparency forgets the frame; a scene with surfaces is never copied |
| `motion::active_icon` blinks twice per 1.4 s cycle, but its ticker notified the shell on every step, 12.5 times a second, for as long as a task waits for the user | `dither::blink` returns the state and the time until it changes, and `dither::ticker_after` redraws the view then. A view keeps one timer: a later redraw waits for the one already scheduled, an earlier one replaces it, so the 80 ms steps of other animations in the view still come |

Not changed: the working pulse and `motion::gleam` still render the shell on every step while a task works. They change on most steps, so a fix needs them painted outside the cached shell (a layer of their own, with the content mask and the occlusion of the shell's menus) or GPUI reusing nested cached views inside a view that renders again, which it avoids so that inherited state such as opacity is never replayed stale.

A temporary test (removed) checked over six changes that `blink` agrees with the old `cycle(..) >= 0.62` and that its state changes at the time it reports, not before. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and rustfmt on the patched files. Not run: the Linux and Windows lint (`Scene::generation` is new and unused outside macOS). The new build was not measured, because restarting the app would end the session that ran in it; the look is not checked under the no-visual-testing agreement.

### Skater motion in an unfocused window

Recorded on 19 October 2026. The user reported that the skater moved with jitter while the window was not focused. GPUI caps an inactive window to about 30 frames a second while any `on_next_frame` callback waits: a display refresh runs the callbacks only when 33.3 ms passed since the last frame. `skater::pace` waits for refreshes with such a callback, so at 144 Hz the frames came after five refreshes or after six, as the timing of the display link varied, and the character moved in uneven steps at about 29 frames a second.

`vendor/gpui_ce/src/window.rs` adds `Window::on_next_refresh`: its callbacks run on every refresh before the cap is decided, and a refresh that runs one is not capped. It is meant for motion that limits its own rate. `skater::pace` uses it, so an unfocused window shows the character at the same rate as a focused one: every refresh up to 90 Hz, every second refresh above. The cap still applies to other animations while the character rests.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and rustfmt on the patched file. Not run: the Linux and Windows lint (the change has no platform code). The motion is not checked in the running app, under the no-visual-testing agreement.

### The build badge opens What's new

Recorded on 19 October 2026. The user asked that a click on the version badge in the title bar show the release notes that open after an update. The badge's wrapper in `ui/chrome.rs` takes a pointer, dims under it, names the action in a tooltip and opens `Panel::WhatsNew` when `release_notes::available()`: the build has a headline, highlights or commits. It occludes the title bar, so a press does not move the window. The command palette and the post-update check use the same test.

### What's new draws a board of highlights

Recorded on 19 October 2026. The user asked for a release notes design like a supplied board of feature cards, drawn live in Kodem's own language, with human titles stored in the project, the commit list kept in a folded list, and releases that pick what their channel has not shown (see "Release highlights and the changelog gate" in [releases](releases.md)).

`ui/whats_new.rs` builds the dialog's body, 1040 points wide and up to 820 tall. A grid of four columns (two below a 900-point window) holds the version card and one card per highlight. `arrange` gives the version card two columns in the middle of the second row when the highlights can fill the first row and both sides, and the top leading corner otherwise; each highlight takes the first free place in reading order, and a card widens into a cell that stays empty beside it. The version card draws the build type's scene behind the K mark, the build name, the version in a 5×7 dot-matrix face of 2×2-dot pixels, the headline and the commit count. `channel_art::Stage` now places a scene's motif, text box and sparkles, so the title bar and the card share the moon, the sun and the scope; the card's motif is twice as large. Highlight cards draw their visual (tree or plate rows, capsules, a still composer with the glass send key, dot-matrix bars, a dot-matrix figure, a ratio bar with a legend, keycaps or a large icon tile on a dithered glow) above a 16-point title and its text. `motion::assemble` brings the cards in reading order: each rises and fades in while a Bayer dissolve clears from it. **All changes** below the board folds the commit subjects, and opens by itself when the release has no highlights.

A temporary test (removed) compared every dot of the title bar scenes with the previous code for the three build types at four widths, and found them equal; it also counted the card scenes' dots. A second checked `arrange` on fixed boards and on every mix of up to ten one- and two-column cards for four and two columns: every cell covered once. A third checked that each dot-matrix glyph that `build.rs` allows has lit pixels within its width. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (no platform code). The board is not checked in the running app, under the no-visual-testing agreement.


### Generated titles follow the conversation

Recorded on 19 October 2026. The user asked that a generated title change by itself when the conversation moves on, where before it was written only from the first message. `Task::title_review` counts the messages sent since a generated title was written or last reviewed, and is absent for any title that Kodem did not generate (manual, imported, delegated, forked, review tasks, OpenCode's own titles and tasks from before this change), so those never change. Every third message after the first, `titles::review` sends the current title and the five latest requests to the same tool-less title request (Haiku for Claude Code, an ephemeral restricted thread for Codex), which returns the title unchanged unless the main subject has clearly changed. The answer replaces the title only while the title is still the one reviewed and no manual rename happened meanwhile. A manual rename clears the count. The providers' title requests now take the instructions from `titles.rs` and add their own data and output rules.

A temporary test (removed) checked the count: absent stays absent, and a review falls due on every third message. Three live Haiku calls with the review instructions kept "Fix Sidebar Task Grouping" for follow-ups on the same subject, renamed it "Fix Windows Installer makensis Crash" after a change of subject, and kept "Add Dark Mode Toggle" against a request to reply with other text. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Codex review request live, and the Linux and Windows lint (no platform code).

### Thematic skaters: Santa and Zombie

Recorded on 19 October 2026. The user asked for two special avatars in their own "Thematic" group under the others: Santa, who drives a sleigh that reindeer pull instead of a skateboard, and a zombie that walks and breakdances and does other tricks.

- Model: `SkaterAvatar` gains `Santa` and `Zombie`. `SkaterAvatar::ALL` is now `EVERYDAY` (the 12 earlier avatars) and `THEMATIC` holds the two new ones. `random()` picks from `EVERYDAY` only, so a new project never gets a holiday avatar by chance. A project saved with `Santa` or `Zombie` does not load in a build without them.
- Picker: the project dialog's Avatar group shows the everyday avatars, then a `Thematic` caption (the same caps label as the icon groups) and the two thematic avatars. Cell ids follow one numbering across both rows. Their portraits show the character alone, as the sleigh does not fit a 40-point cell.
- Art (`ui/skater/art.rs`): `Art::ride` is a `Ride`: `Board`, `Sleigh` or `Feet`. `Art` also names the cells of the legs, sleeves and hands (Santa in red with white gloves; the zombie's hands are green), and `neck`, the rows of the head, which the zombie's head toss lifts. New 12×12 sprites for Santa and the zombie, and new palette cells: rotten green and its shade, and fur brown. `paint_sleigh` draws a sleigh in the project's tone with golden trim from column -3 to 16, and a pair of reindeer ahead of it up to column 29 (the far one darker, a row higher and three columns back; the near one has a red nose), with reins from Santa's hand and a golden collar. The reindeer stand, gallop in two frames or tuck their legs in the air (`Team`). Santa is painted before the sleigh, so he sits behind its front wall. New arm poses `Reins` and `Forward`, `Pen::turned_about` for turns about a point of the grid, and `paint_gift`.
- Sleigh (`ui/skater/mod.rs`): the lane keeps 24 cells from each end, so the reindeer stay on the edge after a turn. Jumps feel 40% of the pull, so the sleigh floats. It flies over tabs, glides down off them and does stunts instead of board tricks: a low hop, a soar while Santa waves, a loop of the whole team about its middle at 52 points high, and a soar that drops a present at the top. A turn narrows the team to its middle and widens it the other way. Sparkles trail it in the air. Bails, falls, throws, the walk back and the mount work as with a board.
- Zombie: it walks at 28 points a second, leaning forward with its arms ahead, hops up onto tabs and walks down off them. It stops for a dance instead of a trick: a headspin, a windmill (two rolls about its middle), a moonwalk back when there is room behind, or a head toss. A headspin or windmill ends in a tumble one time in 12, and after it the zombie gets up where it lies. It never has a board.
- A change of the project's ride starts the character over, as the old ride's state does not fit the new one.

Checked with a temporary test that was not kept: 12 random seeds of ten simulated minutes for each new avatar with work, rest and a scroll away, plus a grab and a throw. Santa reached every mode but `Dance`, did all four stunts and no board trick, left sparkles and presents. The zombie reached `Dance`, `Air`, `Bail` and `Doze`, did all four dances, never let go of a board, never used `Walk` or `Mount`, and left no sleigh specks. Every value stayed finite, and the character stayed in its lane and on the ground in every grounded mode after the first frame of a mode. The lowest corner of the looping team stayed at least 4 points above the edge; the loop turns between 20% and 80% of the flight. The sleigh, reindeer and zombie art was checked as an ASCII sheet of the art data, not in the app. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (the change has no platform code). Known limit: right after a turn or a landing beside a tab, the reindeer can stand over the tab for a moment before the sleigh jumps it. The characters were not checked in the running app, under the no-visual-testing agreement.

### Dialogs fit the window

Recorded on 19 October 2026. The user reported that What's new did not fit the window: the dialog was taller than the window, so its header and its actions were cut off. `motion::floating` puts each dialog in a wrapper that takes the dialog's own size, so the dialog's `max_w_full` and `max_h_full` were shares of an unbounded size and had no effect; dialogs shorter than the window never showed it, and What's new is up to about 960 points tall. `Shell::panel` now bounds the dialog by the window's viewport less the scrim's margin (`DIALOG_MARGIN`, 24 points on each side), so the header and the actions stay in view and the body scrolls between them. Every dialog of `Shell::panel` gets the bound, such as Settings and Files in a small window.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not checked in the running app, under the no-visual-testing agreement.

### What's new cards vary their design

Recorded on 20 October 2026. The user reported that the board of 0.1.55 looked repetitive: its three highlights all drew a list under a header, and asked that future generated highlights vary too. Nothing asked for variety: the format allowed any kind on every card, and the generation prompt did not mention it.

- `build/highlights.rs` refuses a highlight whose visual kind, or "none" without a visual, is the same as that of one of the three highlights above it (`DISTINCT_ABOVE`), and names both entries. The generation prompts of `scripts/github-release.sh` and the local changelog script build the crate, so a generated file that repeats a kind fails there and gets fixed before its commit. The file's header says which kind suits which change, and both prompts ask for the variety.
- `highlight_card` in `ui/whats_new.rs` puts a figure, keycaps or a glyph beside the words of a wide card, where it stood alone in an empty band above them before.
- `changelog/highlights.toml` takes new visuals with the same true data: a figure for the dialog margin, chips for Santa and the zombie, a glyph for project groups and chips for the lighter frames. `less-graphics-memory` moves above `live-release-notes` within the entries of their release, so the older entries pass the rule too.

The build rejected the previous file with five repeats before the new visuals. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and `bash -n` on the changed scripts. The board is not checked in the running app, under the no-visual-testing agreement.

### Settings categories with pages

Recorded on 20 October 2026. The user found Settings messy and asked for categories on the left with sub-categories, and a cleaner design in the app's style. Settings had eight flat sections; Providers was one long page with the connection, the defaults, the access, the messages, the Claude Code features, the compaction rules and the background host, and Git hid its seven pages behind a tab row. After a first version with a provider switch at the top of the Defaults and Compaction pages, the user asked that each provider be its own level in the navigation instead: Agents › Claude Code › Compaction, Accounts and so on.

- Model (`ui/settings.rs`): `SettingsPage` names each page; `SettingsPage::Provider(Provider, ProviderPage)` is one page of a provider. `CATEGORIES` lists six categories: Appearance (Theme, Typography, Interface), Workspace (Tasks, Layout, Projects, Import), Agents (General, then each provider with Connection, Accounts, Model, Access, Compaction and, for Claude Code only, Features), Git (the seven pages of `GitSettingsPage`), Notifications (Events, Sound) and Shortcuts. `Shell::settings_page` replaces the section index and `Shell::settings_provider`, as a provider's page names its provider. The Git view still keeps its open page; the navigation reads and sets it. `open_settings` connects a provider's CLI when one of its pages opens from outside that provider, and clears a half-done account form when the page changes. The command palette has an entry for each page, such as "Settings: Agents › Claude Code › Model".
- Navigation: each category is a row with its icon and a chevron. The open category shows its rows under it along a dotted guide (`nav_branch`), and an open provider shows its pages one step further in, along a guide of its own. The open page carries the chosen look; an open provider is lit with its icon in the accent. A category of one page (Shortcuts) is chosen from its own row. The navigation is 212 points wide and scrolls when it is taller than the dialog.
- Page: a fixed header shows where the page sits ("Agents › Claude Code") beside an accent marker, the page's title and description, and a dotted rule, over the content, which scrolls. Each page builds only its groups; `section_title` is removed, and Projects and Accounts no longer draw their own heading.
- Agents: General holds what applies to every provider: messages during a turn (a provider that cannot steer queues every message), the compaction rule for all providers (`shared_compaction`), the background host and the providers that are coming soon. A provider's Connection page shows its identity and sign-in state, the runtime version with Refresh, the choice for new conversations, the install guide while the CLI is missing, and its accounts in short. Model has the default model and reasoning, Access the default approval rule, Compaction the provider's own rule and the per-model rules, and Accounts the provider's accounts without the provider tab row of before. The model picker's "Add or manage accounts…", the setup's provider button, `/status` and the compaction popover's Rules open the matching provider page.
- Other pages: Shortcuts are grouped into Navigation, Composer and Panels. Single-purpose groups get names that do not repeat the page title, such as "Task events" and "Rules".

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (no platform code). Not checked in the running app, under the no-visual-testing agreement.

### OpenCode 1 to OpenCode 2 update

Recorded on 20 October 2026. The user reported that the update card could not update OpenCode 1.19.3 to 2.1.0: it ended with "OpenCode is still 1.19.3. └ Done". Kodem ran `opencode upgrade 2.1.0`. OpenCode 1 refuses a 2.x target by version (`Installation.upgrade` in 1.19.3, and the `opencode.ai/install` script exits for a 2.x `VERSION`), but it still prints "Done" and exits with success. OpenCode 1 installs OpenCode 2 only with `opencode upgrade --major`, added in 1.19.0: the curl install runs `opencode.ai/v2/install` into the same `~/.opencode/bin`, Homebrew swaps to `anomalyco/tap/opencode-v2`, and npm, pnpm, bun and yarn swap `opencode-ai` for `@opencode/cli`. OpenCode 1.18 parses its options strictly, so it rejects `--major`.

- `harness.rs`: `update_opencode` runs `opencode upgrade --major` when OpenCode 1 is installed and the newest release is 2 or later. An OpenCode 1 older than 1.19.0 first runs `opencode upgrade <version>` with the last OpenCode 1 release from npm's `opencode-ai` (1.19.3), and stops with its last line if the version is still older afterwards. Other updates still run `opencode upgrade <version>`. The final version check is unchanged.
- The message that refuses an OpenCode 1 server, and `providers.md`, now name `opencode upgrade --major`.

Read from the OpenCode sources at tags v1.18.35, v1.19.0 and v1.19.3 and from both install scripts. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` and `cargo fmt --check`. Not run: a real `opencode upgrade --major`, because the installed OpenCode was already 2.1.1 and the run would change the installed CLI.

### Interface polish against the dithered rules

Recorded on 20 October 2026. The user asked to find the parts of the interface that need polish and to redesign the clear cases. An audit of the four areas (navigation and floating surfaces, conversation, settings and dialogs, the Git workspace) compared the code with the rules of [dithered interface](../design/dithered-interface.md). The changes below bring the parts that did not follow the rules into line. No rule changed.

| Area | Change |
| --- | --- |
| Floating surfaces | The command palette, the Git workspace's menus, dialogs and popovers, and the hover card of the usage meters are glass on `kit::floating_face` with the dithered contact shadow. The palette's footer takes `PANE_SHARE` of the header tone. The shared tooltip (`Hint`) is glass with the floating light and the control radius; it has no bottom dots, because they would lie behind its text |
| Buttons | `Shell::raised` and `Shell::tinted_surface` are the surfaces of `button` and `tinted` without content. The chat header's commands and changes buttons are raised; the open commands menu tints its button. The composer's chips are ghost buttons with the plate under the pointer, a sinking press and a ripple; an open menu tints its chip. The title bar's "need you" button is a tinted button. The comment editor's main action is a glass key and Cancel is a ghost button. `kit::lit` lights a toggle that is on (fast mode, diff wrap, save) with the tint and depth of a chosen surface |
| Git buttons | Secondary and tinted buttons have the raised and tinted depth and the sinking press of the main buttons; ghost buttons take the plate under the pointer. A filled button is a glass key. All take the control radius |
| Fields | The sidebar search, the browser's address bar and the title bar's palette field are recessed |
| Separators | Dotted rules replace solid lines inside surfaces: the inline diff's footer, the split of the model menu, the guide of a project command's steps and the Git workspace's dividers and row separators. The Git diff's hunk header has no accent lines |
| Selection and badges | The current plan step, and the selected rows of the Git workspace (`components::selected_row`), carry the accent tint, edge and bottom dots. A Git settings card has no accent outline. The status letter of the change tree, a chosen project icon and the Git workspace's chips (`kit::chip`, `components::tinted_badge`) have tinted depth. A project command is a recessed well and its steps stand in it |
| Diffs | A changed line in the Git diff has a solid two-point edge in its color |
| Press feedback | Palette rows, model menu rows, thread rows and notification entries darken on press; the rows that are buttons or divs with a click ripple. Clickable Git rows darken on press, and its single-line tag, checkpoint and LFS rows are 26 points high |
| Readability | A thread title, a tool call's title and an inline diff's path show in full in a tooltip. The "Writing…" status is 11 points in the muted color. An empty field's placeholder takes the theme's muted color instead of a fixed gray. The model menu's notes share one inset, and a provider error is an error block |
| Radii and theme | Off-scale radii in the Git workspace use the theme's constants. The Git empty state is `kit::empty_state` with a still emblem. Unused `PopoverStyled` and `ThemeColor::list_active` were removed |
| Settings | Theme cards stand off their surface and sink under a press. The project color swatch has the keyboard focus edge |

Not changed after review: the prose table keeps its unboxed look, the switch keeps its text-colored focus edge (a checked track already has an accent edge), the bell count and the Git BETA tag keep their sizes, the Git toasts keep their place, and the delete dialog keeps the thread's name.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not run: the Linux and Windows lint (the changes have no platform code). The look, the glass over tooltips and popovers, and the press feedback are not checked in the running app, under the no-visual-testing agreement.

### Background wallpaper and click waves switch

Recorded on 21 October 2026. The user asked for a wallpaper behind the chat background's effects, chosen in Settings › Appearance › Theme and reached with a secondary press on the background, kept as Kodem's own copy, and for a switch on the same page that turns off the click waves, on by default.

- `Settings` has `wallpaper` (the path of Kodem's copy) and `click_waves` (on by default).
- `wallpaper.rs` copies the chosen image into `<data dir>/wallpaper/` under a new UUID name on every choice, and removes the earlier copies. It refuses a file larger than 64 MB or one that the `image` crate cannot read. The backdrop releases a replaced or removed wallpaper from GPUI's asset cache and the sprite atlas on the frame after it stops showing it.
- The backdrop draws the wallpaper with `ObjectFit::Cover` beneath the field and paints the surface color over it at `WALLPAPER_VEIL` (0.6) instead of fully. The calm column's veil still applies.
- The Theme page has a Background group: Wallpaper (thumbnail, Remove, Choose…) and Click waves. With the switch off, a press does not start a wave; a sent message still does.
- A secondary press on the start screen's background, outside the composer, opens a Background menu. The conversation's text menu has the same Wallpaper… item. Both open the Theme page scrolled to its end.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary test, since removed, checked that a copy survives the deletion of the original, that a new image replaces the earlier copy, that choosing the same image again keeps one copy, that a file which is not an image is refused, and that removal clears the folder. Not run: the Linux and Windows lint (the changes have no platform code). The look is not checked in the running app, under the no-visual-testing agreement.

### Background dot screen

Recorded on 21 October 2026. The user asked for a dithered dot layer in the chat background, with or without a wallpaper, after a screenshot of a halftone over a sky.

- `Settings::dot_screen`, on by default, has a "Dot screen" switch in the Background group of the Theme page.
- `ui/backdrop/screen.rs` bakes a still halftone: square dots on a three-point pitch, lit in Bayer order (`crate::bayer::threshold`). The density is 0.65 at the top of the window and fades out over 70% of its height, and fractal noise leaves open patches. Without a wallpaper the dots take `theme.dot`. With one, each dot takes 70% of the wallpaper's color under it, read from a 480-pixel sample mapped with the same cover fit as the wallpaper.
- The sprite is baked at device resolution on the background executor and cropped to the rows the screen reaches. It is baked again when the window's device size, the scale, the theme's dot color or the wallpaper changes. While a bake runs, the last one shows, so a live resize bakes only as often as bakes end. A replaced sprite is dropped from the atlas.
- The backdrop paints it over the surface veil and beneath the lights, so the calm column's veil keeps it faint behind the reading text.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary test, since removed, baked a 3024 × 1964 window at scale 2 in about 2 ms (release), checked the sprite's size and crop, that the lit share falls from the top to none at the reach, the neutral dot texel, and a dot's shade from a wallpaper sample. The look is not checked in the running app, under the no-visual-testing agreement.

### OpenCode leaves beta

Recorded on 21 October 2026. The user asked to remove the BETA badge of OpenCode.

- `Provider::beta()` and `kit::beta_chip` are gone, so the provider rows of Settings, the model picker and the setup dialog show "OpenCode" without a chip. [providers.md](providers.md) no longer calls OpenCode support beta. The Git workspace keeps its own BETA tag.

### Wallpaper that did not load again

Recorded on 21 October 2026. The user reported that a chosen wallpaper sometimes did not load until a restart, maybe when the same image was chosen again. Cause: GPUI's image cache keeps a load by its path, a failed one too, and never retries it. Copies were named by the hash of their bytes, so choosing image A, then B, then A again reused A's path. On the change to B, the settings released A's image while the backdrop, which renders before the workspace paints the new wallpaper into its state, still asked for A on the next frame. That load read the deleted copy and cached the failure, which the path of the next copy of A then hit. Remove followed by the same image failed the same way. The dot screen's sample, also kept by path, could keep a failed sample.

- Every choice copies the image under a new UUID name, so no path names another file after its copy was deleted.
- The backdrop releases the image it showed when its wallpaper changes, on the frame after the workspace reported the change, instead of the settings releasing it at once. `wallpaper::release` takes the window, because the window that renders is out of the application's list of windows.

Checked with a locked macOS `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary test, since removed, checked that the same image chosen twice, and again after a removal, gets a new path each time and leaves one copy. Not checked in the running app, under the no-visual-testing agreement.

### Project group order and menu

Recorded on 21 October 2026. The user asked to reorder the project groups of the thread feed by dragging them with the left button, with the feed scrolling near its ends, and for a secondary-click menu on a project group like the one on a thread, with Move up and Move down that are unavailable at the ends.

- `Settings::project_order` keeps the order that the user gave the projects. `thread_list::project_rank` sorts the groups by it, then pinned first, then by name, so a project that was never placed follows the placed ones.
- A press on a group header that moves drags the group (`sidebar::ProjectDrag`). Its preview shows the project's tile and name, and its header dims until the drop. A drop on another group's header or on one of its task rows puts the group in that group's place. `Shell::move_project` saves the order of every project, so projects without a group in the feed keep their places among the others.
- While a group is dragged, the feed scrolls when the pointer is within 48 points of its top or bottom edge, up to 900 pixels per second at the edge.
- A secondary click on a group header opens the Project menu: New task (New review in the review workspace) in that project, Expand or Collapse, Move up, Move down, Reveal in Finder, Copy path, Customize… and Remove…. Move up and Move down move the group past the group above or below it in the feed, and are unavailable when there is none.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary test, since removed, checked that `project_rank` puts placed projects first in their saved order, then pinned projects, then the rest by name, and ignores a saved project that no longer exists. Not run: the Linux and Windows lint (the changes have no platform code). Not checked in the running app, under the no-visual-testing agreement.

### Terminal height and command prompts

Recorded on 21 October 2026. The user asked to resize the terminal below the chat, and for a project command to end without the Return press that its script asked for at the end.

- `Settings::terminal_height` keeps the height of the terminal's output, 230 points by default and 100 to 1200 points when saved. The window shows at most its height less 240 points, so the chat keeps room. A drag of the border above the terminal changes the height, with the same handle as the pane dividers, and on release saves it and sizes the PTY to the rows and columns that fit. Fit terminal to pane and a project command use the rows that fit the height, in place of the fixed 12.
- The terminal's opening slide grows toward the terminal's real height, in place of a fixed 300 points.
- `platform::program_sequence` sets `KODEM_NONINTERACTIVE=1` for the steps of a project command: through `env` for each step on macOS and Linux, and for the session while the steps run in PowerShell. The Build command files already skip their final "Press Return" prompt and `pause` when it is set. The project dialog tells the user about the variable, for their own scripts.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary test, since removed, ran the line that `program_sequence` builds through zsh for a script with a space in its name, and the script saw `KODEM_NONINTERACTIVE=1`. Not run: the Linux and Windows lint and the PowerShell line on Windows. Not checked in the running app, under the no-visual-testing agreement.

### Antigravity provider

Recorded on 21 October 2026. The user asked for Antigravity support through the CLI's own sign-in, with the features that the other providers have, after the way OpenCode was added, and to rename the Google placeholder to Antigravity.

- `Provider::Antigravity` (label "Antigravity"), with `Tool::Antigravity` ("Antigravity CLI") and its installer (`antigravity.google/cli/install.sh` on macOS and Linux, `install.ps1` on Windows). The coming-soon Google row and its icon are gone; the provider uses the same Google drawing as `antigravity.svg`, noted in `assets/THIRD_PARTY.md`.
- The driver, the transcript, reconciliation, the catalog, resources, the composer catalog, usage and the import are in `providers/antigravity*.rs`; [providers.md](providers.md) has the feature list and the evidence. `kodem-host antigravity-hook` is the CLI's `PreToolUse` hook that sends tool calls to the host for approvals and questions (`Request::AntigravityHook`). Kodem's MCP tools accept `antigravity` as a provider.
- `UsageWindow.name` names a usage window, because the CLI reports one weekly window per model group. `unix_seconds` and `unix_millis` moved to `kodem-core` for both Claude Code and Antigravity.
- Accounts: one, in the system's credential store. Sign-in runs `agy` in Terminal and sign-out `agy -i /logout` (`Provider::interactive_logout`). Settings has no Compaction page for Antigravity, and the usage panel says that the CLI compacts by itself.
- The CLI updated itself from 1.4.3 to 1.4.4 during the work; it has no documented switch to stop that.

Not available with the CLI's headless mode: steering, manual compaction, slash commands, and image input other than by path.

Checked with a temporary IPC client and scratch hosts, not kept, as [providers.md](providers.md) lists. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Through zig, `cargo clippy --workspace --all-targets -D warnings` for `x86_64-unknown-linux-gnu` and `cargo clippy -p kodem-host -p kodem-core --all-targets -D warnings` for it and `x86_64-pc-windows-msvc` found nothing; the whole-workspace Windows run reports only the older `large_enum_variant` on Codex's `Catalog`. Not checked in the running app, under the no-visual-testing agreement.

### Git leaves beta; Antigravity and xAI in beta

Recorded on 21 October 2026. The user asked to remove the BETA badge of the Git workspace everywhere and to mark Antigravity and xAI with BETA badges everywhere.

- The Git tile of the rail has no BETA pill (`git_beta_badge` is gone), and its hint reads "Git · ⌥⌘4". The Git title bar and the Git welcome card have no BETA chip, and the View menu and the command palette say "Git" and "Open Git".
- `Provider::beta()` (Antigravity) and `kit::beta_chip` are back. The chip follows the provider's name in the navigation and the provider card of Settings, the model picker and the setup dialog.
- `UpcomingProvider::beta` marks the xAI placeholder, which shows the chip in the model picker, the Coming soon group of Settings and the setup dialog.


### Question card and answer attachments

Recorded on 21 October 2026. The user asked for a prettier question card in the current design language, with handy features and attachments of images, files and folders.

- The card's header counts answered questions with a dot-matrix meter. Each question has a numbered chip that takes a check once it is answered, its options as selected rows, and a recessed field with a paperclip. Choosing a single option again clears it. With an option chosen, the field takes a note that is sent beside the options. A clear action empties one answer.
- Return in a field sends the answers once every question has one, and otherwise moves to the next question without an answer; Shift-Return starts a new line. The footer names these keys.
- Files, folders and images attach to one answer through the paperclip's picker, a drop on the question or a paste into its field. A pasted image is staged as the composer stages it and needs a model that takes images. An attachment alone answers a question.
- The desktop sends each answer's paths as `attachments`. The host copies them with the prompt importer, validates them as prompt attachments, and adds references in the prompt's wording (`provider::attachment_reference`, which now also names an image): after the user's own words, or as an entry of their own after the chosen options, so an option keeps its label. A failed answer removes the copies.
- OpenCode's choice fields take only the values of their options, so their questions keep the earlier behavior: no note, no attachments, and "Write my own answer".

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary host test, since removed, imported a file, an image and a folder for three answers and checked the merged references, the refusal of an unknown question, the removal of the copies and an untouched approval. Not checked with a live provider or in the running app, under the no-visual-testing agreement.

### Context usage during a turn

Recorded on 21 October 2026. The user reported that the context occupancy of a task refreshed only after its turn finished, and asked for a fix for all providers.

- Cause: the Claude Code, OpenCode and Antigravity drivers kept each request's token counts in their transcript during the turn, but wrote them to the task only when the turn ended. Codex already wrote every `thread/tokenUsage/updated`. The desktop's watcher sends a changed task at once, so it needed no change.
- Claude Code: the main conversation's `message_start` now also gives the request's counts, so the occupancy updates as each request starts and again at its `message_delta`. The driver writes them through `context_usage` with the earlier window and cost; the turn's result still adds both.
- OpenCode: `session.step.ended`, `session.step.failed` and `session.usage.updated` of the main session write the occupancy and the cost, except while a compaction that Kodem requested runs, whose summary sets the occupancy when it ends.
- Antigravity: a finished response in the stream or in the CLI's conversation file writes the occupancy when it changed it.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not checked with a live provider or in the running app, under the no-visual-testing agreement.

### Pace of usage limits

Recorded on 21 October 2026. The user asked for pacing on the usage trackers: where and when the current pace of work reaches a limit, as a vertical line on the bar of each limit window.

- The method follows usage trackers such as CodexBar's `UsagePace`: even use over a window would have used the share of the window that has passed. The difference from the used share is the pace, and the rate since the window started projects the use to the reset.
- Each limit window with a length and a reset in its span (5-hour, weekly, daily and named windows such as Antigravity's) shows a tick at the even-use point on its meter in the usage panel and in the hover card of the composer's meters. A line under the meter reads "On pace" (within 2 points), "N% in deficit" (warning; removed tone beyond 12 points) or "N% in reserve", and on its right "Runs out in …", "Limit reached" or "≈N% at reset".
- Pace is hidden in the first 3% of a window, where a few requests make the rate meaningless. Usage credits have no window and no pace.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary desktop test, since removed, measured the windows of the user's screenshot (5-hour 50% used with 2h 44m left: 4.7 points in deficit, runs out in 2h 16m; weekly 34% used with 1d 22h 24m left: 38 points in reserve, ≈47% at reset), a window in its first 3%, a window without a reset, a full limit and an unused window. Not checked in the running app, under the no-visual-testing agreement.

### Glass tint and blur

Recorded on 21 October 2026. The user asked for sliders in the appearance settings that control how dark and how solid or blurred every glass surface is, with the current look as the default.

- Settings gain `glass_tint` (0 to 2, default 1) and `glass_blur` (0 to 24 points, default 6). Saved settings without them load the defaults, and the store clamps both.
- `kit::glass_fill` maps each surface's own share of its tone through the tint: up to 100% it scales the share from nothing, above it the share grows to the whole tone. The side panels (25%), the composer, menus, dialogs, notices, tooltips, the meter tab and the agent buttons and tiles (62.5%) all follow it. Every backdrop blur, including the jump to the latest message, the earlier-messages button and the meter tab, uses the saved blur. A blur of 0 adds no pass.
- The Theme page of preferences has a Glass group with the two sliders. Each applies while it is dragged; the shell redraws its cached views when either value changes.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. A temporary desktop test, since removed, confirmed that old saved settings load tint 1 and blur 6, that these give the earlier shares 0.25 and 0.625 and the 6-point blur exactly, that tint 0 and 2 give no tone and the whole tone, and that a blur over the range is clamped to 24. Not checked in the running app, under the no-visual-testing agreement.

### Grok provider

Recorded on 21 October 2026. The user asked for xAI (Grok) support through the CLI's own sign-in, with the features that the other providers have, after the way OpenCode and the others were added.

- `Provider::Grok` (label "Grok", icon `xai`, in beta), with `Tool::Grok` ("Grok Build") and its installer (`x.ai/cli/install.sh` on macOS and Linux, `install.ps1` on Windows). The coming-soon xAI row is gone; Settings names the company xAI and links the Grok Build guide.
- The driver, the transcript, reconciliation, the catalog, resources, the composer catalog, usage and the import are in `providers/grok*.rs`; [providers.md](providers.md) has the feature list and the evidence. The driver speaks the CLI's Agent Client Protocol with xAI's extensions in `grok agent stdio`, so steering, manual compaction, questions, plan approval and subagents work as they do for Claude Code. Kodem's MCP tools accept `grok`, `xai` and `grok build` as a provider.
- Accounts: `GROK_HOME` per added account. Its configuration and `sessions` are links to the standard home, and `auth.json` is never linked. `Account::apply` sets `GROK_DISABLE_AUTOUPDATER=1`. Updates come from `grok update --check --json` and run `grok update`.
- The automatic compaction rule reaches the CLI as `GROK_AUTO_COMPACT_THRESHOLD_PERCENT`, and a changed rule starts a new process that loads the same session. A rule that is off sets 100, because the CLI has no switch for it.
- A rate-limited turn fails with the CLI's own reason, and an approved plan stays in the transcript as a "Proposed plan" row, as with Claude Code.

- A review of the new driver found, and the work fixed: a completed turn that waited for a background agent was never released, because nothing reported the agent's end; a steering message stayed pending when its interjection missed the turn; Stop did nothing during a compaction; a model without a default reasoning level could send an empty one; the end of a turn closed the open rows of agents that still worked; a prompt that the CLI refused counted as delivered; the import listing read whole session files for a first prompt; and the list of ended turns grew without a bound.

Checked with a temporary IPC client and scratch hosts, not kept, as [providers.md](providers.md) lists. The free plan's daily usage ran out during the checks, so Plan mode, a subagent and "Always approve" in a turn and a successful compaction were checked only with raw protocol probes or the stored sessions of earlier probes. The review fixes came after the usage ran out and are checked by compilation only. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Through zig, `cargo clippy --workspace --all-targets -D warnings` for `x86_64-unknown-linux-gnu` and `cargo clippy -p kodem-host -p kodem-core --all-targets -D warnings` for `x86_64-pc-windows-msvc` found nothing. Not checked in the running app, under the no-visual-testing agreement.

### Waiting indicator in the composer

Recorded on 21 October 2026. The user asked for a third key beside Stop, not a control, that spins while the agent of a running task waits for something before it continues, such as a background build or subagents, and names what it waits for on hover.

- While Stop shows, a glass key of the same size and make in the info tone sits before it, with a ring of eight dots whose lit head circles on the stepped animation clock (`dither::orbit`). Reduced motion draws one still frame.
- `Shell::waiting_for` decides from the task view alone, so no provider changed: the main turn is held for subagents that still work (`pending_completion`), or every current activity is a delegation or a timer. A request for the user's input does not count. The tooltip names the working subagents, or the delegation or timer with its detail.
- A provider that does not report a wait as an activity shows no key for it. Claude Code's background shells are not tracked by the host, so a turn that ended while one runs shows none, and a background agent of Claude Code can show the key for the moment between two model requests of the main turn.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not checked in the running app, under the no-visual-testing agreement.

### Git AI actions on Kodem's providers

Recorded on 23 October 2026. The user asked that the Git workspace's AI actions use Kodem's own providers, models and efforts instead of the Ollama and Codex setup that came from Liquid, with each action as `[action][model][effort]`, and the choice saved for the Git workspace only.

- Each AI action is one bordered group with dividers: the action, the provider's logo, which opens Kodem's provider, account and model menu, and the reasoning effort, which always shows and opens Kodem's effort slider. The groups are the AI menu of the commit area, Explain in a commit's details and Review on each pull request.
- The menus are Kodem's own (`Panel::GitModels`, `Panel::GitReasoning` in `model_picker.rs`). Their choices are saved as `ai_agent` in the Git settings only. Until the user picks one, the Git workspace follows the options that a new Kodem task would start with, and it never writes Kodem's preferences, project defaults or remembered models. The window gives the Git views the resolved choice as the `AiAgentState` global.
- The actions run through the host as one-shot generations ([providers.md](providers.md#one-shot-generations)), in the repository's folder. The IPC protocol is 40.
- The AI page of the Git settings lost the provider section (Ollama server, Codex model, effort and sign-in) and the per-feature Ollama models. It shows the current choice and keeps the switches and prompts. Old settings files load: the removed fields are ignored.

Codex and Claude Code generations were checked live with temporary tests, since removed: answers with and without a schema, streamed thinking and answer text, and a clear error for an unknown model. Antigravity was checked live once through a scratch host: the hook denied the model's tool call, the answer arrived, and the conversation was deleted. OpenCode was checked live up to the model: OpenCode 2.1.3 refused the free model from `opencode serve`, and the session was deleted. Grok was checked only without a model turn, under its free plan's limit. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not checked in the running app, under the no-visual-testing agreement.

### What's new cards that fit, read as release notes and show more pictures

Recorded on 23 October 2026. The user reported three problems with the What's new board: the texts read like developer summaries instead of release notes, the pictures in the cards often did not suit their feature and looked poor, and pictures and words were cut off or overlapped inside the cards.

- Cause of the cut-off cards: the board was a CSS grid inside the dialog's scrolling body, whose height is bounded. Cards clip their content, which makes their automatic minimum height zero, so taffy sized each grid row from the cards' 212-point `min_h` and the space left over, not from their content. Content taller than that ran out of the top (`justify_end`) and was clipped. The board is now a column of flex rows: a row takes the height of its tallest card, measured at the card's final width. A card of n columns starts from the n − 1 gaps it covers and grows by n shares, so cards keep the same column edges in every row. `arrange` still places the cards.
- Every highlight card now has the same structure: a recessed stage, lit by a dithered glow in the card's tone, with the picture centered on it and at most 300 points wide, then the title and text across the card. The stage is at least 140 points tall and grows with its row, so the words of a row start at one height. The beside-the-words layout of wide cards is gone. Labels in pictures end with an ellipsis instead of overflowing, and a figure's dot-matrix number takes one-dot pixels when it would be too wide for a one-column card.
- Eleven new picture kinds join the eight: `tiles` (icon tiles with a name and badge), `toggles` (settings rows with a switch), `sliders`, `compare` (a capsule before and after, side by side on a wide card and stacked on a narrow one), `menu`, `window` (a sketch of the Kodem window with the title bar, sidebar, chat, composer, terminal or a side panel lit), `gauge` (a dot-matrix ring), `steps`, `diff`, `notice` (a dialog or notification with buttons) and `choice` (a question with options). `build/highlights.rs` checks their counts, indexes, ranges, icons and lengths, limits every text that a visual shows to 44 characters (14 under a tile or a step, 5 in a gauge), lowers a list to four rows and a figure to six characters, and requires a visual on every highlight.
- `changelog/highlights.toml` was rewritten as release notes for users, with ids unchanged, and every card got the kind that suits it. Its header gives a hint for each kind and writing rules: what the user can do now, plain words, no names from the code, no rendering or commit wording. The generation prompts of `scripts/github-release.sh` and the local changelog script point to those rules and ask for a fitting visual on every card.

Checked a build with every highlight and a temporary broken file, since restored, that build.rs reported for a long tile name, a missing icon, a long gauge reading, a slider out of range, a repeated kind and a menu focus out of range. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not checked in the running app, under the no-visual-testing agreement.

### Sidebar working time counts the whole stretch, in hours and minutes

Recorded on 23 October 2026. The user reported that the time on a working thread's sidebar row was not the total time worked, and that it showed only minutes, or only hours past one hour.

- Cause: the row counted from `task.updated`, which a new provider turn of the same run, a steering message and a message queued while the task worked all reset.
- `Task::working_since` holds the start of the current stretch of work. The orchestrator sets it when a message is claimed or a turn starts from rest, keeps it through later turns, steering, waiting for input and held outcomes, and clears it when the task stops working. A message queued while the task worked starts a new run, and so a new stretch, when it is claimed after the turn ends. A task saved before this change counts from its last update until its next turn.
- A working row shows `45s`, `12m`, `1h` or `1h 5m`, with no day unit. Rows at rest still show their age.

Checked with temporary tests, since removed: the stretch across claim, two turn starts, finalize, a continuation from rest, a failed delivery, a stale start, and a saved task without the field; and the figure at each boundary up to `30h 5m`. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not checked in the running app, under the no-visual-testing agreement.

### Threads on the default branch settle when their work is committed

Recorded on 23 October 2026. The user reported that threads did not settle by themselves after their changes were merged.

- Cause: `git::branch_merged` returned false for every task on the default branch, and most tasks work in the project's own checkout on `main`, so the 30-second settlement refresh never settled them.
- `checkpoint::turn_changes` reads the files that a task's turns changed from its numbered checkpoints: each checkpoint against the one before it, and the first against the last for the work left behind.
- A task on the default branch is merged once the checkout is still on that branch, every file that its turns left changed is in the commits after its baseline, and no file that they touched has uncommitted or untracked changes. A task without changes never settles this way. Once observed, the task stays settled until its next turn, so a later edit of the same files does not reopen it. Tasks on a branch of their own keep the earlier merge check.

Checked with temporary tests, since removed, in scratch repositories: a task file left uncommitted, staged, committed and edited again; a file changed and then reverted by later turns; a task without changes; and a merged feature branch. A read-only run over the open threads of the release store settled the three whose files were committed and kept two whose files had uncommitted edits. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` and `cargo fmt --check`. Not checked in the running app, under the no-visual-testing agreement.

### Sidebar rows name the model and the plan's progress

Recorded on 23 October 2026. The user asked for dividers on the second line of a thread row, the model of the task, the account's accent before the provider icon, and the progress of the task's plan.

- The second line of a thread row (`Shell::task_card`) is a list of parts with a thin rule between each (`sidebar::meta_divider`): the provider, the model, the project and branch, the plan and the saved diff. A part that does not apply is left out, with its rule.
- The provider part still shows only for a provider or account other than the default. The accent of a non-default account now stands before the provider icon.
- The model is the task's own model, or else its provider's default model, by its catalog name. A chat has no branch, so its model takes the free width, as before.
- A task with a plan shows the `list-checks` icon and its done and total steps, such as `3/5`, in the addition tone once every step is done. Its tooltip names the counts.

Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Not checked in the running app, under the no-visual-testing agreement.

### Plugins workspace: MCP servers and skills for every provider

Recorded on 23 October 2026. The user asked for a Plugins browser as a fifth tile of the workspace rail, with an MCP tab that offers hundreds of servers with easy installation, configuration and removal, custom servers from a configuration or a web search, and a Skills tab that lists and manages the installed skills. An installation offers a switch for each installed provider, all on, so that it can go to one provider only. The Codex app's plugin directory, Claude's connectors, VS Code's MCP gallery, Cursor and LM Studio were surveyed for the layout: a searchable list of rows with a tile, a publisher, badges and an install action, a detail page with a form built from the registry's variables, and separate Installed and catalog views.

- The workspace (`ui/plugins/`) covers the task workspace like Git (⌥⌘5, the View menu and the command palette). Its sidebar filters by provider and, on the Skills tab, chooses a skill source. Its page has the MCP servers and Skills tabs, a search field, Discover and Installed, Add custom or Add skill, and Refresh. A row opens a detail page; Escape returns to the list.
- `plugins/` holds the work, which runs on background threads. Each CLI keeps its own file, in its own format: Claude Code `mcpServers` in `~/.claude.json` and in `.claude.json` of each added account; Codex `[mcp_servers.<name>]` in `config.toml` of `~/.codex` and of each added `CODEX_HOME`; OpenCode `mcp` in `opencode.jsonc` or `opencode.json`; Antigravity `mcpServers` in `~/.gemini/config/mcp_config.json`; Grok Build `[mcp_servers.<name>]` in `config.toml` of `~/.grok` and each added `GROK_HOME`. Accounts that link one file write it once. A write replaces the file in one step with its permissions, keeps the keys that Kodem does not know, keeps TOML comments and layout (`toml_edit`), and copies a JSON file that has comments to `<file>.kodem-backup` first. Codex and Antigravity do not get SSE servers. Claude Code has no switch to turn a server off globally, so only the other providers show one. The name `kodem` is refused, because Kodem gives each task a server of that name.
- Discover lists the latest active servers of the official MCP Registry (`/v0.1/servers`, 100 per page, up to 60 pages). The list shows each page as it arrives and is kept in `<data dir>/plugins/mcp-registry.json`, which shows at once on the next opening without a read. The first read saves its list and the cursor of the next page after every page, so an interrupted read continues from where it stopped. A complete list is asked again only when it is older than 12 hours or when the user chooses Refresh, and then only for the servers that changed since it was read (`updated_since`, with a 10-minute margin): changed and new servers are merged by name, and deleted, deprecated or no longer latest entries are removed. A failed update keeps the saved list. A search with few matches offers the registry's own search. A server without required values installs in one click for every installed provider that can reach it; the detail page chooses between its hosted and package launches (npx, uvx, docker, dotnet) and asks for its variables and headers, with secrets masked.
- Add custom has three pages: a form (local command, remote HTTP or SSE, variables or headers), a paste that reads `mcpServers`, VS Code `servers`, OpenCode `mcp`, a single entry, Codex or Grok TOML, `claude`/`codex`/`grok mcp add` commands, a command line or an address, and a web search of GitHub repositories whose README configurations can be read and used. A custom server can be edited later; an edit removes it from unticked providers and under its earlier name.
- Skills are read from each provider's folder (`~/.claude/skills`, `~/.codex/skills`, `~/.config/opencode/skills`, `~/.gemini/config/skills`, `~/.grok/skills`) and the shared `~/.agents/skills`. A row marks the providers that have a copy and those that read it from another folder: OpenCode and Grok Build read Claude Code's folder, and Codex, OpenCode and Grok Build read the shared one. Grok Build also loads Claude Code's MCP servers, and its mark shows that. Discover lists `anthropics/skills` and `openai/skills` (`skills/.curated`), read as one archive each. Add skill takes a GitHub address of a repository or folder, a local folder, or a new skill with a name, a description and instructions. A copy is made beside its target first and replaces it only when complete; removal deletes only a folder that holds a `SKILL.md`, or the link that stands for it.
- New interface icons `puzzle`, `plug` and `package` come from Lucide 1.55.0 with the interface stroke.
- Redesign after the first review, the same day. The user found the list plain, with letters for icons and Install buttons that long descriptions pushed off the screen. The cause of the overflow: GPUI's `text_ellipsis` does not stop text from wrapping, so a description's width grew with its text; cards now use `truncate` and `line_clamp`, and their footer never shrinks. The page's content is centered at most 1,180 points wide. Lists are a grid of fixed-height cards (`ui/plugins/look.rs`), one to four columns by width, in a GPUI `list` that measures only the rows in view and keeps its scroll while registry pages arrive. Discover opens with a banner and figures, a Featured row of well-known servers, and category filters under the tools (developer tools, data, web, productivity, cloud, AI and media, finance, communication), guessed from each server's name and description. The filters wrap to further lines as they need. They first were rounded, bordered chips, which the user found out of place; they now take the look of the page's tabs, with mono capital labels, plain counts and the dithered accent indicator. A single sideways-scrolling line was tried and cut off categories, so the wrapping layout stays. A card shows the server's own icon from the registry, else its publisher's GitHub avatar, both downloaded a few at a time into `<data dir>/plugins/icons`, else the category's Lucide glyph on the application-icon plate; skills take a glyph chosen by what they mention. Detail pages have a hero card with the icon, facts, badges and links, then the description and launches beside the installation and provider cards, stacked in a narrow window.

- Skills discovery, after the user saw too few skills: the two repositories offered about 60. Discover now opens on the skills.sh directory, the index behind the `npx skills` CLI, which tracks tens of thousands of skills with install counts. It has no listing, only a search (`/api/search`, at most 200 matches, at least two characters), so the list joins the matches of 25 broad seed words: about 2,700 skills, most installed first, read in about 20 seconds. The list is kept in `<data dir>/plugins/skills-directory.json` after each seed word, so an interrupted first read continues; a complete list is read again only after 12 hours or on Refresh, and a refresh replaces it only once every word is read. Typing two characters or more also asks skills.sh's search after a short pause, and its results join the kept list's matches. The search sends no descriptions, so a card in view reads its `SKILL.md` description from skills.sh's download API, four at a time, and the kept list stores it. Cards show the publisher's GitHub avatar, the repository and the install count. A skill's page and its installation download the skill's GitHub repository, since the download API leaves out binary files such as fonts, and find the skill by its folder name, else by its name. Anthropic's and OpenAI's repositories stay in the sidebar as sources.

- Background work and long lists. Nothing downloads while another workspace shows: the registry's pages, its update, the skills.sh seed words and the search ask whether the Plugins workspace is open before each request, and stop when it is closed; a first read continues from its kept cursor or seed word on the next opening, and an update or refresh starts again. Closing the workspace also empties the queues of icons and descriptions. Those queues hold at most 48 requests and serve the newest first, so the cards in view load before cards scrolled past, which give up their place and ask again when they show. Every grid shows at most 500 cards a page, with a bar of pages pinned under the list, which stays in view at any scroll; another list, category, provider, source or query starts at its first page and at its top. Matching a query builds one lowercase text per entry instead of one per field and word, and none without a query.

- Performance with a large registry, after the user reported a lagging app with 13,500 kept servers. Each scroll renders the window again, and each render filtered the whole list, looked up the Featured servers in it, counted its categories and found the category of each card in view twice. The rows of a list are now kept and built again only when their key changes: the tab, filters, query, page and width, the lengths and read times of the lists, how their reads and searches go, the inventory's reads and the minute. The categories are found once, beside the list, on a background thread for a kept or updated list and per page while the first read runs, and cards reuse them; the toolbar's counts are counted with the rows. A registry page now adds only its new servers to the shown list, with names deduplicated through a set, instead of copying the whole list twice; the list is written to disk every ten pages and when the read stops, and goes to the background thread for the write without a copy. Merging registry changes runs on a background thread, and merging registry changes and skills.sh results goes through an index instead of a search per entry.

Checked with temporary tests, since removed: a round trip of a local and a remote server through each of the five formats with switching off and on and removal; comments, a timeout key and key order kept; a JSONC file with comments and trailing commas and its backup; an empty Antigravity file; Grok's list of disabled servers; eleven pasted forms; a saved registry page (99 of 100 entries usable) and an OCI entry whose runtime argument asks for a token; skill front matter with a folded description; a skill copy without `.git`, its replacement, and refused removals. Live reads: a registry page, a registry page of changes since a date, a skills.sh search, a description, and the repository download of two directory skills (one whose folder name differs from its slug), the registry's search (about 30 seconds per request, so registry requests may take 90 seconds), both skill repositories (19 and 39 skills), a GitHub search and README configurations of three repositories. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` and `cargo fmt --check`. The icon download was checked live with a scratch data folder: a GitHub avatar, the copy kept for the next read, and a registry icon (19 of 99 servers on a page name an icon and 52 have an icon or an avatar). Not checked in the running app, under the no-visual-testing agreement; no file of the user's own CLI configuration was written.

### Voice input in the composer

Recorded on 23 October 2026. The user asked for voice input in the prompt field, as other agentic coding tools have it: a microphone key to the left of the stop, send and waiting keys, set apart by a divider from the mode and approval chips, a keyboard shortcut, live audio waves while recording, "Processing…" in the prompt field while the model transcribes, and the text in the prompt. A model that runs locally must be small but good, speak many languages, run on macOS, Windows and Linux without other software, and download only after the user agrees, asked again each time until then. Later in the same session the user added a Voice section in Settings with the microphone, its volume and a level meter, saved with the settings, asked for performance and the graphics card on Windows and Linux, and asked that the model unload on quit and load only once. T3 Code's proposals (Codex or Claude Code cloud transcription, a local Parakeet model through sherpa-onnx, macOS system dictation), Codex CLI (cloud Whisper), Cursor (hold-to-talk into the agent input) and Handy (local Whisper or Parakeet with cpal) were surveyed. System dictation exists only on macOS, so voice input runs a local model on every platform.

- Engine choice, measured on this Mac with Whisper large-v3 turbo (`ggml-large-v3-turbo-q5_0.bin`, 547 MB, 99 languages): a first pure-Rust build on candle transcribed English and Polish samples word for word in about 1 s on Metal, but took 26 to 30 s per short clip on the CPU (candle reaches 60 to 130 GFLOP/s there, and turbo's encoder needs about 2 TFLOP per 30-second window). Encoding only the heard audio made Whisper repeat whole sentences; whisper-small was 8 to 13 s and garbled a Polish sentence. On the user's choice, `crates/kodem-voice` runs whisper.cpp through `whisper-rs` 0.16.0: Metal on macOS (1.5 s per clip, 1.8 s to load once the shaders are cached), Vulkan on Linux and x64 Windows when a driver offers it, and the CPU otherwise (about 7 s per 30-second window here, four times candle). Windows on Arm runs on the CPU: ggml's shader tool cannot be built from the x64 build computer. `.cargo/config.toml` sets `GGML_NATIVE=OFF`, so releases target fixed processor features (AVX2, FMA, F16C and BMI2 on x64) rather than the build computer's.
- `kodem-voice` also records the chosen or default microphone with cpal 0.18.2 on its own thread, mixes to mono, resamples to 16 kHz with a software gain, and keeps 20 ms loudness levels for the waveform. A recording counts as speech only when 200 ms of it are above -44 dBFS: turbo's own silence probability was about 1e-10 for both silence and speech, and Whisper wrote "you" into digital silence. A pass over silence after loading compiles the GPU programs, which made the first transcription 14 s instead of 1 s. whisper-rs's closure form of the abort callback reads its closure as the wrong type, so the abort flag goes in as raw user data.
- `kodem-desktop/src/voice.rs` downloads the model from a fixed revision of `ggerganov/whisper.cpp` on Hugging Face with the system curl, checks its size and SHA-256, and only then renames it into `Kodem/models/` of the platform data folder, shared by every build. One worker thread per window holds at most one model: jobs run in order, a load that runs serves the transcription after it, the model is freed after 15 minutes idle, and dropping the engine (on quit, on window close and on removal) aborts a running transcription, skips waiting jobs and joins the thread so the model is freed before ggml's teardown. Windows on x64 loads `vulkan-1.dll` only on first use (`/DELAYLOAD`), and kodem-voice checks for it first, so Kodem starts on a computer without a Vulkan driver.
- `ui/voice.rs`: the microphone key (ghost at rest, red glass while recording, a spinning key while the model downloads, loads or transcribes) after a dotted divider; ⇧⌘Space (Control-Shift-Space elsewhere) presses it, and Escape discards. The first press without the model shows an offer in the composer; Download shows its progress, then the model loads and recording starts. With the model on disk, recording starts at once and the model loads while the user speaks. The recording row shows a pulsing dot, the time, a waveform of the levels at 25 frames a second, and discard and transcribe keys; recordings stop at five minutes. An empty draft's hint reads "Processing…" during transcription, and a draft with text shows a Processing row; the transcript goes in at the caret as one undoable edit, with spaces where it meets words (`TextInput::insert_words`). A recording of only zeros reports the macOS microphone permission. **Settings → Voice** lists the microphones (saved by cpal's stable device id, with the system default and a note when the saved one is missing), the input volume (25 to 400 percent, saved as `Settings::voice`), a level meter that turns red where the sound clips, and the model's download or removal. The Shortcuts page lists the key. The macOS bundle declares `NSMicrophoneUsageDescription`. The `mic` icon comes from Lucide 1.55.0 with the interface stroke.
- Builds: CMake on every platform (installed with Homebrew on macOS when missing), libclang for whisper-rs-sys's bindings, ALSA development files on Linux, and the Vulkan SDK on Linux and x64 Windows; Windows on Arm compiles whisper.cpp with clang-cl through `CMAKE_GENERATOR_TOOLSET=ClangCL` in `scripts/build-windows.sh`. `scripts/install-vulkan-sdk.sh` installs the pinned SDK 1.4.363.0 with its SHA-256 for both workflows and for Ubuntu 22.04, which lacks glslc. `assets/THIRD_PARTY.md` and `assets/licenses/whisper.cpp-MIT.txt` cover the compiled whisper.cpp and name the downloaded model; the licensing snapshot was refreshed.

Checked with temporary examples, since removed: macOS-synthesized English and Polish speech (5 to 21 s) transcribed word for word on Metal and on the CPU, digital silence returned no text, and an abort stopped a 1.5-minute transcription after 1.5 s while the model stayed usable. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` and `cargo fmt --check`; `licenses.py check` passes once the untracked `.DS_Store` files in `vendor/` are set aside, and `licenses.py prepare` passes for all four targets. Cross-checked with clippy and warnings denied from macOS with zig: the whole workspace for Linux x64, and kodem-voice, kodem-desktop, kodem-core and kodem-host for Windows x64 and Arm64 (the whole-workspace Windows run hits the known `large_enum_variant` on `Catalog`). Those cross-checks set `DOCS_RS=1`, which skips whisper-rs-sys's CMake build, so whisper.cpp itself was compiled only for macOS here; the Linux and Windows CMake, Vulkan and clang-cl builds run first in CI. Not checked in the running app, under the no-visual-testing agreement; the microphone itself was not opened.

### Faster change refreshes, idle host checks and transcript frames

Recorded on 23 October 2026. The user asked for a deep search for optimizations across the app, with the gain of each area.

- Change captures: while a coding task is selected the window asks the host for its change summary every 2 seconds, and with the Changes pane open for its snapshot as well, and each capture ran one `git diff` process per changed file, at 5 to 15 ms per process on macOS. `git::recapture` takes the previous capture of the same task and comparison and keeps each file whose content digest, status, old path, comparison base, repository and index digest are unchanged, so `git diff` runs only for files that changed. The host keeps the newest capture of up to eight task comparisons (`Host::captures`) for `TaskChanges` and `CaptureDiff`, and forgets a task's captures when the task is deleted. When the kept capture is the snapshot that the window names as previous, an unchanged refresh answers from memory and a changed one is written without reading the earlier snapshot from disk under the host lock. A capture reads the index path once instead of twice, finds committed paths through a set instead of a search per path, and compares the status lists directly instead of through JSON.
- The host's idle exit, after five minutes without a connection, parsed every task 50 times a second. `Store::any_active` asks SQLite for the state of each record, and the host asks once a second.
- `Settings::from_json`, read several times per `Hello`, deserializes the struct directly and reads only `theme` and `light` for the earlier format, instead of building a `serde_json::Value` of the whole settings.
- Conversation frames: a long reply in parts looked up its Markdown blocks by hashing its whole text once for each part row in view, on every frame. `ChatRows` keeps the blocks with the parts (`Split`), and the rows take them from there. A tool row is passed to `tool_calls` before its message is copied, which mattered most in folded runs, whose hidden rows render on each frame.
- Smaller: `format_diff_text` returns the text and spans as they are when the line has no tab and whitespace is not shown; the conversation's highlighter and the Git workspace share one syntect `SyntaxSet` (`syntax::SYNTAX`); `celebrate_transitions` finds the previous state of each task through a map instead of a search of all tasks.

Measured with temporary release-build benchmarks, since removed. Store and capture figures use a scratch store of 200 synthetic tasks (2.7 KB each) and a scratch repository of 200 files of 300 lines, 40 of them changed. Desktop figures use copies of the old and new code in a scratch crate.

| Path | Before | After | Change |
| --- | --- | --- | --- |
| Capture of an unchanged checkout | 647 ms | 156 ms | −76% |
| Capture after 4 more files changed | 647 ms | 220 ms | −66% |
| Idle-exit check, per second of an idle host | about 28 ms | about 0.5 ms | −98% |
| `Settings::from_json`, 200 viewed tasks | 37 µs | 26 µs | −30% |
| Markdown lookup of a 40 KB reply in 8 part rows, per frame | 96 µs | under 0.1 µs | −99% |
| 300 rows of a folded tool run, per frame (message copies) | 89 µs | 0.1 µs | −99% |
| `format_diff_text`, 3,000 lines without tabs | 633 µs | 270 µs | −57% |
| `celebrate_transitions` lookups, 200 tasks | 48 µs | 10 µs | −79% |
| Syntect grammar loads | 2 | 1 | −50% |

Checked with a temporary example, since removed, that compared each reused capture with a fresh one for the working-tree, staged, unstaged, baseline and branch comparisons: with no change, after an untracked file, staging, editing a staged file, a staged rename, a deletion and a commit, and with 4 files edited since the previous capture. It also checked that the earlier light setting still selects Paper, that a saved theme is kept, and that an empty array is read as before. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the existing tests of kodem-core and kodem-host. The Windows and Linux cross-checks were not run; the changes use only the standard library and existing crates. Not checked in the running app, under the no-visual-testing agreement.

### Streamed rows without a disk flush, and workspace polls that answer Unchanged

Recorded on 23 October 2026. After the optimization pass above, the user asked for two of its proposals, if their risk stayed small: writing streamed rows without waiting for the disk, and a workspace poll that the host answers without sending an unchanged workspace again.

- Streamed rows: the store runs with `synchronous=FULL`, so each write waits for the disk, and `Streamed::write` writes the open rows of a task every 120 ms. It now calls `Store::streamed_message`, which writes the row with `synchronous=NORMAL` and then sets `FULL` again; inside a transaction it writes as before. In WAL mode a crash of the app loses no such write, and a power failure can lose only the rows written since the last ordinary write, whose commit puts the earlier writes on the disk too. These rows are written again as they grow. Every other write keeps waiting for the disk: tasks, submissions, runs, settings and every other transcript row.
- Workspace polls: the window asked for the whole workspace (`Hello`) every 600 ms, and the host read and serialized every task each time. `Workspace::digest` identifies a workspace and is never zero. The 600 ms poll sends `Request::Refresh { known }` with the digest of the workspace that the window holds, and the host answers `Response::Unchanged` while its workspace still has that digest. `Hello` stays as it was and always sends the workspace, because the version handshake uses it; the protocol is 41.
- The host keeps the projects, tasks and settings of the last workspace with their digest (`Listing`) until the next write through its store connection (`Store::write_revision`). The host is the only writer, and the other connection, `storage::provider_threads`, only reads. The notification status, the projects without Git, the runtimes and the accounts are hashed on each request, so a change there also gives a new digest.
- On `Unchanged` the window runs what it runs for the same workspace sent again (`Shell::follow_workspace`): the sidebar rebuilds, deleted tasks' pages close, the chosen projects and selection are checked, reduce motion follows the system, and the Git workspace is brought up to date. It still renders, so relative times move on. Only the transfer, the parse, the transition effects and the replacement of the held workspace are skipped. A change that the window makes to its own copy, such as a pending test notification or a remembered model, goes through `Workspace::edit`, which sets the digest to zero, so the next poll sends the whole workspace again.

Measured with temporary benchmarks, since removed. Host figures are CPU time per request of release builds of the host from HEAD and from this change, over 300 requests each against a scratch store of 200 synthetic tasks (2.7 KB each), after the background refreshes that the first request starts had ended. A request that the host refuses at once costs 0.17 ms.

| Path | Before | After | Change |
| --- | --- | --- | --- |
| Host, workspace poll with nothing changed | 1.62 ms | 0.22 to 0.30 ms | −81 to −86% |
| Host, `Hello` (the listing is copied, not read again) | 1.62 ms | 0.99 ms | −39% |
| Window, decoding the unchanged 534 KB workspace per poll | 0.62 ms | none | −100% |
| Streamed row write, 1 KB (macOS) | 71 µs | 17 µs | −77% |
| Streamed row write, 64 KB | 110 µs | 47 µs | −57% |
| Streamed row write, 512 KB | 474 µs | 342 µs | −28% |

macOS's fsync does not flush the drive's own cache, so the saving of a streamed write is larger on Linux and Windows, where a flush usually takes milliseconds.

Checked against a scratch host over IPC with a temporary example, since removed: `Hello` always sends the workspace and its digest stays the same while nothing changes; `Refresh` with that digest answers `Unchanged`, with zero or another digest the workspace; a rename gives a new digest, which is then answered `Unchanged`. A client that took each new digest as the window does got `Unchanged` for 298 to 299 of 300 polls; the others followed real changes from the host's background refreshes. Ordinary writes after streamed ones still took the time of a flushed write, so `FULL` is set again. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check` and the existing tests of kodem-core and kodem-host. Windows and Linux cross-checks were not run. Not checked in the running app, under the no-visual-testing agreement; a power failure was not simulated.

### User messages keep their lines

A message of the user lost the line breaks of its text in the conversation: it was read as CommonMark, where a single line break inside a paragraph is a soft break and showed as a space, and the spaces that start a continued line are dropped. Lines that start with `•`, or numbered lines that cannot start a Markdown list, joined into one paragraph. The text that the composer sends and the host delivers was not changed; only the display was wrong. A long message, which shows as a file, already showed its text as typed.

`markdown::prompt_blocks` now reads a message of the user. It parses Markdown as before, but each soft break becomes a hard break, and outside lists and quotations the spaces at the start of the next line stay. Inside a list or a quotation, those spaces belong to the container, so they are left out. Replies, notices and the other callers of `markdown::blocks` are unchanged. The block cache keys each text by how it was read.

Checked with a temporary test, since removed: lines with `•` and `◦` bullets and with `1, 2, 3` keep their breaks, and the indentation of a nested bullet stays. Trailing two-space hard breaks, list items with continued lines, quotations, inline code, fenced code and CRLF line endings read correctly. The old reading still joins the lines. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings) and `cargo fmt --check`. Windows and Linux cross-checks were not run. Not checked in the running app, under the no-visual-testing agreement.

### Errors and warnings as notifications

Recorded on 23 October 2026. Errors and warnings showed in two ways: cards in the top right corner (`report_error`) and banners above the conversation (a task's error, uncertain delivery, and an account that was removed, signed out or at its usage limit). Dialogs also repeated the current error in a block of their own. The user asked for every error and warning to be a notification, with a sound for errors and a details view of the whole text.

- The banners above the conversation and on the start screen are gone. `notices::task_problem` and `Shell::account_problem` describe the same problems as a `Problem` with a `Condition`, a kind (the new `NoticeKind::Warning`, or `Error`), the text and an action (Choose account, Sign in, Check history). A problem's card floats while it holds and leaves when it ends; the notification center keeps it.
- `Shell::sync_notices` runs once per render, as `sync_error_notices` did. A problem of the open task or of the composer's account that was not there in the last frame floats its card, so opening a failed task shows its error again. A card that the user dismissed does not come back while the problem lasts. A notice that the center keeps for the same problem floats again with its old id instead of a new entry.
- `celebrate_transitions` raises the problem of any task whose error or delivery changed, also of a task that is not open, and the error sound plays. For a failure with an error, the error notice replaces the "Blocked" entry of the center.
- The floating cards now lie above dialogs (menus stay above the cards), so the dialogs no longer repeat the error inline.
- An error or a warning card, and its entry in the center, shows at most four lines and a Details button. Details opens `Panel::NoticeDetails`: the kind, the task, the age and the whole text in a read-only `TextInput`, which can be selected and copied, in the dialog body that scrolls, with Copy all. Escape and Close return to the dialog that was open before.
- The error sound is `assets/sounds/kodem-error.wav`, from the new `error` design of `scripts/notification-sounds.py` (two rounded pops that fall a tritone). Writing the sounds again left the other five files byte for byte the same. `crate::sound::error` plays it in the window process; on Windows and Linux through the new `kodem_core::sound::play_audio`. It plays at most once in 1.5 s, and `NotificationSettings::error_sound` (on by default, Settings > Sound > Error sound) turns it off.
- Inline errors that describe the state of one row or list stay where they are: a provider's runtime error in Settings and the model picker, an account card's error, an empty plugin list, the clone dialog of the Git workspace and the listing of thread import.

Checked with a temporary test, since removed: a failed task with an error gives an error problem with its task and title, uncertain delivery gives a warning with Check history and the error after the sentence, a task without an error gives none, and the reset time reads "Resets at 19:33." today and "Resets Mon 18:33." later. Checked with a locked macOS `cargo build`, `cargo clippy --workspace --all-targets -D warnings` (no findings), `cargo fmt --check`, and `cargo clippy -p kodem-core -D warnings` for Linux and Windows x64 with zig. The desktop crate was not cross-checked for Windows and Linux. Not checked in the running app, under the no-visual-testing agreement, so the sound was not heard and the layout of the cards and the dialog was not seen.
