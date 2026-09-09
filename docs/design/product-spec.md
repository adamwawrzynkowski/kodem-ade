# Kodem product specification

Proposed product direction, 9 September 2026. Requirements below are planned behavior, not implemented capabilities.

## Product promise

Kodem is a native desktop workspace for directing coding agents and deciding whether their changes are ready. A user can move from a task to its conversation, exact code changes, feedback, and verification without losing context.

The first target is macOS on Apple Silicon. Keep domain and service code portable. Linux and Windows remain named milestones with native build gates, not implied support from a dependency description. No web frontend, HTML, CSS, Electron, WebView, or JavaScript application layer. External coding runtimes remain separate installed tools.

The minimum useful release completes this loop: open repository, choose an installed Codex runtime, create a task, watch structured output, inspect actual Git changes, drag the diff gutter to select lines, add feedback, send it, review the next revision, restart and recover the task.

## Reference interpretation

| Supplied image | Keep | Improve |
| --- | --- | --- |
| 1, Zuse workspace | Persistent project hierarchy and adjacent changes | Fewer simultaneous controls, clearer task state, wider readable paths, independently resizable panes |
| 2, T3 conversation | Quiet conversation and docked composer | Use the available width more deliberately, increase secondary-text contrast, show contextual review |
| 3, Synara workspace | Project-aware threads and contextual inspector | Avoid an always-open inspector competing with chat; make it task-sensitive and collapsible |
| 4, full diff | File headers, context expansion, syntax color | Stable revision identity, explicit old/new sides, dedicated feedback gutter, large-file virtualization |
| 5, changed-file card | Compact tree inside the final answer | Bind the tree to a revision; separate agent completion from check evidence |
| 6, turn preview | Quick navigation through long history | Keyboard access, descriptive landmarks, no hover-only functionality |
| 7, task rows | Two-line titles and branch/provider metadata | User-chosen grouping, stable attention sorting, configurable metadata, readable full titles |

## Visual system

Use one coherent native coding workspace. Follow supplied screenshot 1 for its project/task hierarchy on the left, formatted coding conversation in the center, and substantial changes or code pane on the right. The user rejected the generated concepts and cancelled further image generation. [Design guidance](design-guidance.md) records the lessons that must inform implementation: compact readable content, restrained native depth, and consistent controls. Theme customization remains a product feature.

The values below are tentative starting points for implementation, not approved choices from the rejected images. Refine them through native implementation and user feedback. Create density through spacing and information hierarchy before reducing font sizes.

| Token | Proposed value or rule |
| --- | --- |
| Charcoal canvas | `#131414` |
| Sidebar | `#1C1D1E` |
| Elevated toolbar/control | `#242628` |
| Main text | `#EDEDEB` |
| Secondary text | `#B8B8B2`, subject to contrast calculation for each background |
| Accent | `#D99059`; small warm-orange highlights on active state and send; Git colors stay semantic |
| Add/remove | Muted green and coral, always paired with +/− and line semantics |
| Interface | Bundled IBM Plex Sans, 14 px default |
| Conversation | IBM Plex Sans, 15 px default, 1.4 line height, 70–100 character reading width where space permits |
| Code | JetBrains Mono, 14 px default, configurable ligatures |
| Wordmark | Manrope semibold, custom vector k mark, text “kodem”, 23–25 px |
| Spacing | 4 px base; 6, 8, 12 and 16 px common gaps; compact 28–32 px toolbars and task rows |
| Corners | 4–7 px controls and composer, 10 px outer window; reserve cards for content that needs a boundary |
| Depth | Shallow edge highlight/contact shadow on interactive controls; inset inputs; quiet reading surfaces; avoid blanket gradients and shadows |
| Motion | Brief 100–160 ms state transitions; obey reduced motion |
| Accessibility | Target 4.5:1 normal text and 3:1 large text/control boundaries; visible focus and meaningful accessibility tree |

The implementation must bundle actual font assets and notices. Final contrast, glyph coverage, and native text shaping need validation. Implement the interface directly in native components; no generated images are part of the design deliverable or implementation dependency.

## Layout and navigation, N01–N09

At 1440 px wide, start with a 224 px sidebar, flexible conversation, and optional 480 px review pane. Keep small task tabs and repository breadcrumbs instead of large page headings. Code, conversation and terminal output use most of the window. The review pane gets priority when a user starts review. At narrower widths, collapse optional inspectors before squeezing code into unreadable columns.

- N01. Logo and wordmark stay at the top. New task and search are obvious. Settings stay at the bottom.
- N02. Resize the sidebar from 220 to 420 px, persist width per window, support collapse to a narrow rail, and expose a keyboard-accessible width control.
- N03. Projects contain tasks. Offer task-centric saved views across projects. Keep repository, task, agent session, branch, and worktree distinct.
- N04. Sort by attention, recent activity, name, created time, or manual order. Group by project, task state, provider, or date. Manual order is active only in its selected scope.
- N05. Do not reorder the row under a pointer or move the selected task during interaction. Apply pending activity sorting after the interaction or refresh.
- N06. Compact density is the default. Show a readable task title and small state marker; selected or expanded rows can add a branch line. A comfortable two-line mode and provider/time/PR metadata remain configurable. Use full-title tooltips and a wider sidebar before shrinking fonts.
- N07. Pin projects or tasks. Rename, archive, restore, mute, and reveal their local location. Archive does not delete a worktree or transcript.
- N08. Search tasks, messages, paths and commands with visible scope and matched text. Keyboard navigation and a command palette work throughout.
- N09. A single click selects a row. Changing branches is a separate explicit operation with dirty-state handling. Task selection never silently checks out a branch.

## Conversation and composer, C01–C13

- C01. Native Markdown includes paragraphs, lists, tables, links, quotes, inline code, highlighted blocks, images and file attachments. Untrusted raw HTML is displayed safely or omitted, never executed.
- C02. Stream into the current block without reparsing the whole transcript. Incomplete Markdown remains readable. Huge tables scroll horizontally.
- C03. Distinguish user input, assistant messages, tool activity, approvals, questions, errors, check results, and final outcomes as typed items.
- C04. Fold successful routine tool groups by default, while keeping failures and pending input visible. Do not fabricate hidden reasoning.
- C05. Preserve reading position while new output arrives. Follow the end only if the user was already following. Provide a jump-to-latest control.
- C06. A turn rail and searchable history show meaningful user prompts, completions, failures and checkpoints. Hover previews also open with keyboard focus.
- C07. Composer accepts multiline input, paste, native IME, file/image drops, @files/folders/symbols, skill references, slash commands, and diff selections.
- C08. Persist drafts and attachment metadata per task. Show submission state as draft, queued, acknowledged, failed, or delivery unknown.
- C09. Separate “send next” from “steer now” where supported. Never simulate steering by silently terminating and restarting a run.
- C10. Discover provider/model/effort options dynamically. Preserve provider-specific controls. An unavailable option explains the reason.
- C11. Editing an earlier prompt creates an explicit conversation fork when supported. Otherwise offer a new task with a handoff. Original history remains accessible.
- C12. Show native approval or question forms with request scope and lifetime. Stop/cancel status follows provider acknowledgement.
- C13. Changed-file cards open the exact referenced revision. Source links can open the file and line in Kodem or the configured external editor.

## Git review and feedback, G01–G15

- G01. Show working tree, staged, unstaged, commit, branch comparison, merge-base comparison, task baseline, and last-turn changes. Label each comparison.
- G02. Include modified, added, deleted, renamed, copied, mode-only, untracked, binary, symlink, submodule and conflicted entries. Unsupported visual formats get accurate metadata rather than an empty diff.
- G03. Unified and split modes share one immutable diff document. Include syntax and intraline changes, context expansion, wrapping, whitespace visibility and ignore-whitespace comparison.
- G04. A dedicated 20–24 px feedback gutter sits beside line numbers. Dragging there selects code rows. Dragging on text selects text. Staging is a separate action.
- G05. Pointer down records snapshot, file, side and line identity; pointer capture keeps the drag active; pointer up finalizes it. Escape cancels. Support upward/downward drags, edge autoscroll and keyboard range extension.
- G06. Map wrapped visual rows to source lines. Headers, spacer rows and folded regions are not source lines. Expand hidden context before selecting it. Clamp one drag to one file and one side; add several selections for multi-file feedback.
- G07. Removed lines attach old-side coordinates, added lines attach new-side coordinates, context can retain both. Zero-length sides have no invented line zero.
- G08. Feedback contains repository/worktree identity, comparison, captured revision/digests, old/new paths, side, line ranges, exact excerpt and user comment. Store original path bytes separately from display labels.
- G09. Live refresh never moves a selection silently. Keep its old snapshot visible; mark it stale if current content differs. Offer explicit reattachment and show ambiguous mappings.
- G10. Batch comments in a review queue, edit/remove them, and send once to the agent. Preserve the original queue if delivery fails. Deduplicate by submission ID.
- G11. Track viewed files and unresolved comments per snapshot. A materially changed file becomes unreviewed for the new snapshot.
- G12. Offer file, hunk and selected-line staging only when patch construction is proven for that change. Recheck index/worktree identity immediately before applying. Unsupported changes use explicit whole-file operations.
- G13. Checkpoint restoration previews affected tracked/untracked files, index changes, and user edits. Save a recoverable pre-restore state. Conversation navigation alone never modifies Git.
- G14. Conflicts expose base/ours/theirs and final result when available. Never interpret an ordinary two-way patch as a complete conflict resolution.
- G15. Git status remains current after both success and partial failure. Paths with spaces, newlines and non-UTF8 bytes are handled with NUL-delimited machine formats and typed path storage.

The first alpha must complete G01 for working tree/staged/unstaged/task baseline, G02 accurate file classification, G03 text modes, G04–G11, and safe file staging. Advanced line staging, merge UI and checkpoint restore follow after snapshot correctness.

## Skills, instructions and MCP, X01–X10

- X01. Discover documented project and personal skill locations and provider-reported catalogs. Show source path, scope, provider compatibility and content digest.
- X02. Separate discovered files, installed packages, enabled resources, runtime-effective resources, and currently reachable tools. A plugin cache is not an installed capability.
- X03. Resolve naming collisions according to the selected provider's actual rules. Show shadowed items and the winner. Do not invent a global precedence.
- X04. Preview the instruction and skill files that affect a task, including provenance and parsing problems. “Add to prompt” shows what context will be sent.
- X05. Support selection of a skill without loading every skill body into every prompt. Show known size or an explicitly approximate token estimate.
- X06. MCP connection inventory shows server owner, scope, transport, status, auth state, observed time and tool/resource/prompt catalogs.
- X07. Support stdio and current Streamable HTTP where the runtime permits; negotiate protocol versions, handle pagination and list-change events. Mark older SSE transport as compatibility support when implemented.
- X08. Distinguish provider-owned MCP from Kodem-owned MCP. Do not start duplicate copies or assume a green Kodem connection means the agent can call it.
- X09. Store Kodem-owned secrets in the OS credential store. Show references, never raw values in ordinary logs. Installation, configuration mutation and tool execution are separate user actions.
- X10. Pi MCP/approval/subagent extensions remain unavailable until their specific extension bridge is installed and verified. A generic provider icon cannot promise these features.

## Workspace capabilities, W01–W12

| ID | Requirement | Target |
| --- | --- | --- |
| W01 | Explicit task worktree or current checkout; ownership visible; separate concurrent writers by default | Alpha |
| W02 | Native terminal with PTY, scrollback limits, selection, copy, links, task cwd and process status | Alpha |
| W03 | Read-only file viewer and native search; manual editing, undo/redo, diagnostics and LSP as a distinct workstream | Viewer alpha; editing beta |
| W04 | Task inbox for needs-input, failures and review; optional board and concurrency limits | Beta |
| W05 | Build/check actions stored per project with command, environment, revision and evidence | Alpha |
| W06 | PR/issue context, CI links, review comments and host capability differences | Beta |
| W07 | Handoff packet with task intent, files, review selections, executed checks and remaining risks | Beta |
| W08 | Import supported local histories read-only, then explicit copy/import with provenance; never rewrite provider files | Beta |
| W09 | Costs, tokens, context and limits only when reported; distinguish actual, estimate, unavailable | Beta |
| W10 | Scheduling, dependency-based tasks, concurrency/overlap rules, missed-run policy and review inbox | Later parity |
| W11 | Remote Rust supervisor connection, SSH/project environments, then native mobile feasibility | Later parity |
| W12 | Optional external-browser handoff and returned evidence; no embedded web rendering | Later, disabled under current no-control agreement |

## Reliability and data

Use durable local storage for tasks, transcripts, drafts, settings, comments and immutable review content. Keep payload blobs outside the main relational rows, with quotas and retention. Retention cannot delete content referenced by unsent feedback or retained checkpoints.

A saved transcript is different from a live run. Closing a window, quitting Kodem, losing a provider connection, and host sleep are separate lifecycle events. Every one needs explicit behavior. Never label a task recovered just because text loaded from disk.

Permissions distinguish what Kodem may do, what the provider may do, and what an external tool may do. Inspecting a repository does not execute project scripts, install extensions, or start MCP processes. Respect repository instructions and explicit session restrictions.

## Performance targets

Targets below require measurement on an agreed baseline machine and release build; none are achieved claims.

| Workload | Initial budget |
| --- | --- |
| UI interaction | p95 frame work under 16.7 ms on 60 Hz display; never do Git/network/process waits on UI thread |
| Visible feedback | Local key/input acknowledgement p95 below 50 ms |
| Search | Task/message search p95 below 100 ms over a representative indexed 10,000-task corpus |
| Large transcript | 50,000 events with bounded visible rows and paged bodies; no full-history clone per token |
| Diff | 100,000 changed text lines without constructing a native element per hidden line |
| Startup | Usable local shell below 1.5 seconds warm on the agreed macOS baseline |
| Memory | Provisional shell idle RSS below 200 MiB; measure and report external runtime RSS separately |
| Process output | Bounded UI queues; durable spill or explicit truncation metadata for large output |

## Acceptance examples

1. An added 6-line range is selected by gutter drag, attached, sent and reopened after restart with exactly the same text and revision. Text selection still works independently.
2. An external edit during selection marks the attachment stale and never changes what the comment refers to.
3. A provider disconnect after submission but before acknowledgement yields delivery unknown. Reconnect does not submit the prompt twice.
4. A task can use a personal skill while another uses a project override. The UI explains the different effective sources.
5. Switching tasks preserves each draft, scroll anchor, review queue and pane state.
6. Partial staging refuses a stale index and preserves unrelated edits.
7. Large output does not freeze the composer or exhaust the in-memory event queue.
8. A dark/light theme switch preserves contrast and focus state with font scaling.
9. Completion can say compilation passed while interaction validation remains pending.
10. No model, balance, tool count or integration claim is displayed as real unless runtime evidence supplies it.

Current user instructions forbid app control and visual testing. Implementation must record those interaction and accessibility checks as pending until the user performs them or changes that restriction. Compilation is mandatory at every code milestone. Temporary checks and probe files must be removed.
