# Kodem implementation plan

This plan carries the completed research into native application implementation. The user cancelled image generation after rejecting the visual concepts. [Design guidance](design-guidance.md) records the lessons and replaces the images as the implementation reference. [Product requirements](product-spec.md) define behavior. [Research](research.md) explains competitor evidence. [Architecture](architecture.md) defines ownership.

## Delivery rules

Keep all Kodem backend and frontend code in Rust. Use GPUI Community Edition. Resolve GPUI Kit compatibility explicitly; never sneak in a WebView or a second GPUI type graph. Preserve unrelated files and repository changes. Summaries are required for public methods.

Inspect the affected source and conventions before changing code. Compile every code milestone. Current instructions forbid launching, controlling, screenshotting or visually testing apps. Use source checks and permitted headless domain checks; clearly mark interaction, accessibility and runtime gaps. Remove temporary tests and probe files before finishing. Do not commit without permission and never push.

Treat supported provider versions, Git snapshots and persisted schemas as contracts. Avoid a universal compatibility promise. No hidden API key requirement, invented usage number, or fake “connected” state.

Apply the design guidance in every UI phase: compact spacing without shrinking readable text, useful coding content before management panels, one consistent component system, and shallow depth that clarifies interaction. Do not generate another mockup set. Palette, fonts and dimensions remain proposed implementation defaults, not user-approved results from the rejected images.

## Phase 0: resolve native dependencies

Input: the proven CE build and failed Kit interoperability probe in [validation](validation.md).

1. Freeze the initial Rust toolchain, CE and platform versions and lock transitive packages.
2. Create an isolated, attributed Kit compatibility branch or vendor a minimal permitted subset. Port the base/input/focus layer, button, menu, popover, scroll/list and resizable split needed by the first workflow. Include Markdown/editor only after foundational components compile.
3. Ensure every component uses one CE `App`, `Context`, `Entity` and `IntoElement` family. Audit runtime initialization and asset loading.
4. Exclude Kit webview, HTML rendering and script-hosting paths from Kodem's application graph. Do not confuse native Markdown parsing with a web frontend.
5. Produce a native application binary containing an actual Kit-derived input and a resizable pane, and compile it. Record licenses and maintenance changes.
6. If the port has a large API gap, stop that implementation branch with a measured port inventory and cost. Compare CE-native components against the required port, then request a concrete stack decision. Do not mark the combined stack solved after a CE-only build.

Exit: one native type graph and a compiled control composition, or a documented stack decision required before dependent implementation. The current research stage reached the first result for CE and the incompatibility result for unmodified Kit.

## Phase 1: product shell and durable local host

Depends on Phase 0.

Build the native window, compact branded sidebar, bundled fonts, shared neutral theme tokens, restrained native depth, focus order, command palette and resizable panes. Follow supplied screenshot 1 for workspace organization and the design guidance for the improvements. Use aligned rows and local controls; avoid large headings, repeated cards and oversized composer controls. Preserve text size and recover space from padding and redundant metadata first. Establish task/project/worktree IDs and persisted settings. Introduce the selected local host ownership before provider execution, so a long run is not tied to a particular window.

Implement an on-demand host, user-scoped IPC, version handshake, local database migrations, bounded subscriptions, draft persistence and an explicit shutdown policy. Start without network remote access. Closing the last window and choosing Quit must have documented, different behavior. Do not auto-start on login.

Exit: native binary compiles; domain checks cover settings round-trips, schema migration, owner selection and draft isolation. The shell reads actual projects. Empty/loading/error states are present. Source review checks shared tokens, long-name overflow, compact/comfortable density, visible focus definitions and contrast calculations. Visual quality and input behavior remain pending under the current restriction.

## Phase 2: one real Codex workflow

Depends on Phase 1. This is the first end-to-end implementation priority.

Detect the installed executable and version; present installation/authentication status without reading secrets into logs. Generate the app-server JSON schema from the installed supported version, and produce Rust protocol types or explicit checked serializers. Do not generate a TypeScript backend.

Implement initialization, thread creation/resume, turn submission, item streaming, tool results, questions/approvals, interruption and terminal outcomes. Persist outgoing intent and provider acknowledgements separately. Record provider session identity and the selected environment.

Build the composer, native formatted transcript, tool folding and follow-scroll. Explicitly expose unsupported operations. Make process shutdown, lost transport and provider exit distinct states.

Exit: a controlled integration run, when execution is authorized, demonstrates prompt → stream → tool → outcome → persisted history → resume. Under current no-app-control instructions, compile and permitted protocol/domain checks can pass, but this live workflow gate remains pending. Never mark an illustrative transcript as a completed integration.

## Phase 3: Git review and precise feedback

Depends on Phase 2 for sending; diff capture can develop after Phase 1 in parallel.

Read Git machine formats, capture immutable bytes and index stages, and build one diff model for unified/split presentation. Support working tree, index and task baseline comparisons first. Include status for all file kinds even when a format has no text renderer.

Implement virtual rows, syntax spans, context expansion, file tree, independent text selection and gutter selection. Add exact feedback attachments, multi-comment review queue, stale-state detection and delivery deduplication.

Add safe file staging. Then implement partial hunk/line staging only after preserving old/new coordinates and fresh preconditions are proven. Never write a selected preview patch directly without index validation.

Exit: checks cover deleted lines, mixed hunks, renames, CRLF, missing final newline, binary metadata, modes, symlinks, non-UTF8 paths, submodule state, conflicts, wrapped rows and external edits. A user-performed drag check must verify pointer capture/autoscroll and text selection before interaction success is claimed.

## Phase 4: finish the daily-use alpha

Depends on Phases 2 and 3.

Complete sidebar appearance and sorting controls, scoped search, task drafts, unread states, history rail, per-task pane/scroll state and keyboard navigation. Add a native PTY terminal and read-only file viewer. Bind build/check evidence to the revision, environment, command and exit status.

Finish crash recovery, uncertain-send handling, snapshot retention, interruption messages and resource limits. Add provider-owned skill/MCP inventory with provenance. Configure logging and export with redaction. Inventory reads must not execute discovered extensions.

Exit: a user can complete the main Codex task/review/feedback loop and return the next day without losing drafts, comments or exact review references. Compilation is mandatory. Current automation restrictions leave the real interaction pass to the user.

## Phase 5: multi-provider and extension beta

Depends on durable Codex event semantics and review.

Add Pi RPC and OpenCode HTTP/SSE adapters. Develop Claude Code integration as a separate compatibility workstream; prove its interactive approval bridge in Rust or through a supported external protocol before exposing the approval UI. Evaluate a maintained ACP bridge for broader providers.

Add structured capability discovery: resume, fork, steering, approvals, images, model/effort selection, usage, skills, MCP and tools. Unsupported means unsupported with a reason. Reuse provider authentication where documented; do not borrow another client's credentials.

Complete source-aware skill selection and provider-specific precedence. Add explicit MCP configuration editing, health checks and scoped connection management with backups. Distinguish a provider connection from a Kodem connection. Pi extensions require their own compatibility record.

Exit: every advertised provider completes the supported workflow and recovery checks at a pinned version. A compatibility matrix names gaps. Adding provider two does not change provider one's behavior.

## Phase 6: worktrees, review delivery and power use

Depends on Phase 5 for cross-provider tasks; local orchestration can begin earlier.

Add task inbox/board, concurrency limits, managed worktree creation and cleanup, project setup commands, local services, port ownership and task handoffs. Setup scripts run only in the chosen environment under explicit project policy.

Add native editing with undo/redo, search/replace and incremental highlighting, then selected language-server features. Avoid building a whole editor before the chat/review workflow works.

Add GitHub PR and issue context, CI evidence, review import and draft creation. Keep publish, push and merge behind explicit user actions and policy. Additional hosts get their own support matrix rather than a generic promise.

Add checkpoint inspection and restoration with pre-restore backup, conflict-aware application and retained transcript branches. Report filesystem changes separately from irreversible external tool actions.

Exit: two simultaneous tasks keep separate checkouts; handoff preserves context; review statuses and check evidence stay tied to their revision. Worktree deletion refuses unsafe cleanup.

## Phase 7: reliability, accessibility and release

Cross-cutting from Phase 1; this phase closes release gates.

Profile long conversations, large diffs, rapid streaming, terminal floods, cold/warm startup, retained windows and history search. Measure app and external-runtime resource use separately. Fix observed bottlenecks before adding speculative caches.

Complete native keyboard menus, screen-reader labels and roles, IME, high-DPI text, font fallback, contrast calculations, light/dark themes, reduced motion and localization readiness. Runtime accessibility and visual checks require user authorization or user-performed evidence under the current agreement.

Build signed and notarized macOS artifacts, stable/beta update channels, rollback, crash-data opt-in and release notes. Audit licenses, dependency graph, credential storage, local IPC and data export/delete. Start Linux and Windows compile/packaging gates, then platform-specific runtime gates.

Exit: a repeatable release build and recorded acceptance evidence. Never label all platforms supported based only on Rust portability.

## Phase 8: close the remaining competitive gaps

Depends on stable local behavior.

Add scheduled tasks with timezone/DST semantics, overlap policy, offline/missed-run behavior, resource budgets, notifications and review output. Add dependencies between tasks only with cancellation and failure propagation.

Extend the existing host protocol to authenticated remote machines and SSH environments. Add device pairing, revocation, reconnect cursors and environment identity. Prefer a native Rust desktop remote client. A native mobile client needs its own GPUI/platform feasibility decision; do not promise a browser or HTML-based client under the user's stack constraint.

Add optional external-browser evidence exchange, advanced PR-host support, richer context indexing, repository maps, safe extension packaging and an external Kodem MCP interface. An external client must get scoped authority, not unrestricted access to all projects.

Exit: each capability ships with its own lifecycle and failure acceptance criteria. Remote/mobile/browser capability is explicitly deferred, so the first alpha is not advertised as complete competitor parity.

## Scope and effort

The release sequence is more useful than a fixed date. A planning allowance for one experienced Rust engineer with part-time design support is roughly 8–12 engineer-weeks for a credible Codex alpha after the dependency decision, another 6–10 for multi-provider beta and delivery, and 6–12 for release hardening and later parity. These are uncertain estimates, not a delivery commitment. GPUI Kit porting, Claude approvals, native editor work and platform accessibility can change them substantially.

Parallel work is possible after the domain contracts settle: native shell, runtime integration, and Git capture/review can have separate owners. They should share recorded schemas and fixtures rather than mutate the same branch or data files concurrently.

## Verification matrix

| Area | Required evidence | What compilation cannot establish |
| --- | --- | --- |
| Dependencies | Locked manifest, actual CE + selected component composition, build log | Render correctness, input quality |
| Runtime protocols | Versioned schemas, event replay, cancellation, unknown-delivery cases | Account access and real provider reliability |
| Git | Temporary real repositories and byte-accurate expected changes, fresh preconditions | Usability of gutter drag |
| Storage | Restart/replay, migration and interruption checks, retention references | Operating-system service lifecycle |
| Chat/diff UI | Native build, source inspection, user-performed interactions under current restriction | Scroll, focus, selection, accessibility and visual fidelity |
| Release | Platform builds, signatures, packaging/update checks | Untested platforms and devices |

Temporary tests must be removed after each implementation task under the user's current rule. Preserve their scenario and result in the validation notes so a later run can reconstruct them. Do not leave a fake permanent test suite to satisfy a checklist.

## Rollback and completion

Isolate dependency ports and preserve upstream notices. Database migrations need backup and version rejection, not optimistic loading of incompatible data. Provider updates must be reversible and never change a running task's adapter version. Git mutations need fresh preconditions and recovery records; never use a destructive reset as a general repair.

A phase is complete only when its listed behavior and evidence are present. If a required live or visual check is forbidden, state that gap plainly and keep that acceptance item open. This plan originated with the research and design package. Current implementation results and remaining acceptance gates are recorded in [implementation and validation](../implementation/progress.md).
