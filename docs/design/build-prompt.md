# Enhanced implementation prompt

This prompt records the accepted implementation brief. See [implementation and validation](../implementation/progress.md) for the current code, build results and remaining scope.

---

Build **Kodem ADE**, a polished native Agentic Development Environment for people who work with coding runtimes such as Codex, Claude Code, Pi and OpenCode.

Work in this repository. Use the researched requirements in `docs/design/product-spec.md`, the selected ownership model in `docs/design/architecture.md`, and the sequence in `docs/design/implementation-plan.md`. Read `docs/design/validation.md` before choosing dependencies. Apply `docs/design/design-guidance.md` throughout implementation and use `docs/design/research.md` when verifying competitor or protocol claims.

The product must complete a real task workflow: open a repository, run an agent, read formatted output, inspect exact Git changes, select changed lines through the gutter, send feedback, review the next revision, and recover the task later.

## Technology and first decision

Implement all Kodem application logic and UI in Rust. Use GPUI Community Edition for native rendering. Keep blocking I/O and process work off the UI thread. The product has no HTML, CSS, WebView, Electron, or embedded JavaScript application layer. External coding runtimes are separate installed tools.

The researched dependency state is a hard implementation input:

- `gpui-ce 0.2.2` with `gpui_ce_platform 0.1.0` built a native macOS probe.
- `gpui-kit 0.6.0` uses the `gpui-pre` family. The tested Kit Button failed GPUI-CE's `IntoElement` contract with E0277.
- Start by pinning and porting the required Kit/base components onto one CE dependency graph. Prove a real text input, focus/menu interaction structure and resizable pane compile together. Keep upstream license notices.
- If the port proves too large, report concrete affected APIs and options before changing the requested stack. Do not quietly replace CE or claim compatibility from a renamed dependency.

## Product and appearance

The user rejected the generated designs and cancelled image generation. Continue directly with native implementation; do not create another image set unless explicitly requested. Use supplied screenshot 1 for workspace organization: compact persistent navigation, readable coding conversation and a substantial adjacent changes pane.

Avoid generic dashboards, large page headings, repeated rounded cards, oversized controls/composer, decorative color washes and artificial empty space. Preserve legible text and recover space from padding and redundant metadata. Use one shared token and icon system. Give interactive controls subtle depth through restrained edges, contact shadows and pressed states; keep reading surfaces calm. Make the tool feel alive through responsive, informative interaction. Exact colors, fonts and dimensions remain proposed defaults, not user-approved choices from the rejected images.

Start with a distinctive lowercase `kodem` wordmark and simple vector mark. Proposed bundled fonts are IBM Plex Sans for interface/chat, JetBrains Mono for code, and licensed Manrope for the wordmark. Allow font, size, density and theme overrides. Prioritize readable text, visible focus, stable spacing and useful content width.

The left panel must be resizable, collapsible and persistent. Support project and task grouping, pins, archive, saved views, search, attention/activity/name/manual sorting, and optional branch/provider/time metadata. Default to compact readable rows; expose a second line on selected/expanded tasks and offer comfortable density. Keep rows stable during interaction. Selecting a task must not silently change the repository branch. Use compact tabs, breadcrumbs and local popovers. Keep code, chat, diffs and terminal output central; boards and standalone management screens are secondary.

## Chat and review

Build native streaming Markdown with tables, highlighted code, links, attachments and structured tool groups. Preserve reading position and task drafts. Include a useful turn rail with keyboard-accessible previews. Show real errors, pending approvals and questions clearly.

The composer supports files, images, skill mentions, commands and review selections. Differentiate queued follow-up, active-turn steering, cancellation and unknown delivery according to runtime capability.

Build actual Git review for staged, unstaged, working-tree, baseline and later commit/branch comparisons. Provide a changed-file tree, unified and split diffs, context expansion, wrapping and accurate metadata for non-text changes.

The feedback gutter is essential:

1. Dragging a dedicated gutter selects source lines; dragging on text selects text.
2. A selection records the captured snapshot, file, side, original paths, old/new coordinates and exact excerpt.
3. Wrapped rows map to source lines. Headers and collapsed context are not silently selected.
4. Deleted lines retain old-side identity. Added lines retain new-side identity.
5. Refresh never changes what an existing comment refers to. Keep historical attachments and mark stale content.
6. Support several comments in a persistent review queue and send them once as a structured feedback packet.
7. Revalidate current index/content before staging or applying patches. Preserve unrelated edits.

## Runtime and data architecture

Use an on-demand Rust host that owns active sessions independently of desktop windows. Keep the initial workspace to a core library, host and desktop, with adapters/Git/storage/catalog as modules until extraction is useful.

Start with Codex's version-matched app-server protocol. Generate or validate Rust wire types from the installed schema. Add Pi RPC, OpenCode HTTP/event streams, Claude's proven structured bridge and ACP adapters afterward.

Represent capabilities with support details and unavailable reasons. Keep runtime identity separate from model/provider identity. Do not promise Claude interactive approvals from plain print streaming, or Pi MCP support without a verified extension.

Persist task state, drafts, ordered transcript records, provider IDs, submissions and review comments. Store large content in bounded immutable blobs. Commit outgoing intent before send and provider acknowledgement separately. An ambiguous send becomes delivery unknown and must be reconciled, not blindly replayed.

Subscriptions return a snapshot through sequence N and events above N. Handle retention gaps explicitly. Separate concurrent writers with worktrees and coordinate shared Git metadata. A worktree is not a process sandbox.

## Skills, MCP and daily use

Show discovered, installed, enabled, runtime-effective and reachable resources separately, with provenance and observation time. Explain provider-specific precedence and shadowed skills. Read inventory without executing arbitrary discovered scripts.

Show MCP connection ownership, scope, transport, auth state and actual catalogs. Distinguish provider-owned connections from Kodem-owned connections. Credentials belong in the OS credential store.

Deliver the native terminal, file viewer, task inbox, check evidence and project configuration needed for daily use. Later phases add manual editing/LSP, PR/CI integrations, handoffs, checkpoints, multi-agent orchestration, automation and remote hosts. Preserve the explicit release scope; alpha is not full parity.

## Working and acceptance rules

Inspect current source, instructions and affected paths before editing. Preserve unrelated changes. Document public methods. Make the smallest coherent change for each phase, with a real operation behind every enabled control. Never display seeded demo values as live state.

Compile each code milestone. Use meaningful, permitted headless checks for protocol ordering, Git edge cases, recovery and persistence. Remove temporary test/probe files afterward. Review the full final diff and run whitespace checks.

The current working agreement forbids app control, app launch for interaction testing, screenshots and visual testing. Keep those checks pending for user-performed validation unless the user changes the restriction. A compile does not prove scrolling, gutter dragging, accessibility, provider reliability or visual fidelity. Never commit without explicit permission and never push.

Begin with the dependency compatibility decision, then implement the Codex task/chat/diff/feedback/recovery workflow before expanding provider count. Report each phase's actual changes, evidence, unresolved checks and next dependency. Do not replace implementation with another static mockup or a collection of inactive buttons.
