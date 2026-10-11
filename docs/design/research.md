# Kodem ADE research

Research date: 9 September 2026. This is a product and source review, not hands-on testing. Current official documentation and repository files support the claims below. Marketing claims are not performance measurements. The supplied screenshots establish reference layouts only.

## Recommendation

Build Kodem around the complete task: intent, context, agent execution, changed code, feedback, and verification. Use a polished native shell with an excellent chat and review loop. A generic chat window with a provider menu would enter an already crowded market.

Ship Codex first, then add providers through explicit capability contracts. Make accurate diff feedback and reliable recovery the main engineering investments. A light theme, font controls, worktrees, and MCP support are expected capabilities. They do not by themselves distinguish Kodem.

## What the named products establish

| Product | Current documented capabilities | What Kodem should learn | Qualification |
| --- | --- | --- | --- |
| Codex | Rich app-server integration exposes history, streamed events, approvals, thread and turn operations, skills, and MCP status. The surrounding app documentation covers review, worktrees, terminal work, and scheduled tasks. | Use the structured runtime; preserve turn boundaries, pending requests, and provider-owned identity. | The installed CLI labels app-server experimental. Its current docs also warn about production support. Freeze a supported version range. [App-server](https://learn.chatgpt.com/docs/app-server), [developer workflows](https://learn.chatgpt.com/docs/developers) |
| Claude Code | Skills, instructions, hooks, plugins, MCP, subagents, agent teams, and worktree isolation. Programmatic JSON streaming and session resume are documented. | Rich tool presentation, reusable workflows, and separate controls for planning, execution, and permission handling. | A streaming print command does not establish a complete interactive approval bridge. [Extension overview](https://code.claude.com/docs/en/features-overview), [worktrees](https://code.claude.com/docs/en/worktrees), [programmatic use](https://code.claude.com/docs/en/headless) |
| Pi | Minimal agent runtime with skills, extensions, prompt templates, configurable providers, session trees, branching, compaction, and JSONL RPC. | Make conversation history navigable; preserve steering versus queued follow-up semantics. | Built-in MCP, subagents, and permission popups are explicitly absent. Extensions can add them. Do not show universal support. [README](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/README.md), [RPC](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md) |
| OpenCode | Headless HTTP service with OpenAPI, event streams, session operations, provider configuration, permissions, skills, and MCP. | Build a real typed adapter around the service and expose connection health. | It starts a separate service; authentication and process ownership must be explicit. [Server](https://opencode.ai/docs/server/), [skills](https://opencode.ai/docs/skills/), [MCP](https://opencode.ai/docs/mcp-servers/) |
| Synara | Project/thread organization, several local runtimes, review, terminal, file editing, split views, browser previews, managed worktrees, handoffs, automations, and typography/density settings. External MCP can create scoped tasks. | Task context should survive a handoff. Configuration and worktree isolation belong in the core workflow. | README describes development-build capabilities and calls the project early stage. External MCP access is different from provider MCP tools. [Repository](https://github.com/Emanuele-web04/synara), [external MCP](https://github.com/Emanuele-web04/synara/blob/main/docs/external-mcp.md) |
| T3 Code | Desktop, web, and mobile control of several installed agents; remote/background service operation; source-control host integrations and PR workflows. | Fast task switching, readable conversation, and continuity between machines are strong expectations. | It uses Electron and a server. Its UI technology is not the proposed Kodem implementation. Host feature coverage differs. [Repository](https://github.com/pingdotgg/t3code), [source control](https://github.com/pingdotgg/t3code/blob/main/docs/user/source-control.md) |
| Zuse | Persistent projects and provider sessions, streamed tools/edits, seven listed providers, workspaces, review and PR lifecycle, browser tools, mobile/remote access and Serve. | Keep project context, execution, review, and recovery in one place. | Its docs explicitly preserve provider differences. Mechanical Git actions and agent-mediated actions have different ownership. [Providers](https://docs.zuse.sh/providers), [review](https://docs.zuse.sh/workspace/changes-pull-requests), [documentation index](https://docs.zuse.sh/llms.txt) |

Synara here means trysynara.com and Emanuele-web04/synara, not the unrelated business-automation or Roblox products. Pi's older badlogic/pi-mono links redirect to earendil-works/pi. T3 Code means pingdotgg/t3code, not create-t3-app. Some Zuse Markdown URLs failed in the web reader; direct HTTP retrieval succeeded.

## Additional products worth learning from

| Product | Relevant evidence | Kodem implication |
| --- | --- | --- |
| Zed | Distinguishes its own agent, ACP external agents, and terminal threads. External runtimes own authentication and much of their configuration. [Agents](https://zed.dev/docs/ai/agents), [external agents](https://zed.dev/docs/ai/external-agents) | Separate the agent runtime from the model provider. A terminal fallback must not pretend to provide structured chat or native tool control. |
| Cursor | Current agent documentation includes checkpoint restoration, planning, review, and an agent workflow alongside editor work. [Agent overview](https://cursor.com/docs/agent/overview) | Users will expect to inspect and redirect work without abandoning the code context. Preserve checkpoint scope and side effects explicitly. |
| Conductor | Stores local turn checkpoints in private Git refs and documents that restoration can remove later conversation and code changes, including user changes. Support varies by agent. [Checkpoints](https://www.conductor.build/docs/reference/checkpoints), [agent modes](https://www.conductor.build/docs/concepts/agent-modes) | Kodem should preview restoration and preserve history. A checkpoint is a snapshot, not proof of agent authorship or a universal undo. |
| Aider | A repository map supplies selected structural code context under a token budget. [Repository map](https://aider.chat/docs/repomap.html) | Context selection needs explainable scope and size. Full LSP indexing can come later; explicit files and symbols must work first. |
| OpenHands | Documents configurable execution sandboxes separate from the main service. [Sandbox overview](https://docs.openhands.dev/openhands/usage/sandboxes/overview) | Execution environment is a first-class choice. A Git worktree isolates files but does not sandbox processes or network access. |

## Competitive baseline and differentiation

| Capability | Baseline Kodem must reach | Proposed improvement | Release |
| --- | --- | --- | --- |
| Navigation | Projects, tasks, search, unread/running state | Stable attention sorting, saved views, optional metadata, two-line titles, persistent pane widths | Private alpha |
| Conversation | Streamed Markdown, tools, code, attachments, resume | Follow-scroll that respects reading, meaningful turn rail, durable drafts and explicit delivery state | Private alpha |
| Review | Changed tree, unified/split diffs, revision choices | Drag the feedback gutter; attach immutable old/new line references; batch feedback without selecting text | Private alpha |
| Multi-provider | More than one runtime | Explain support per runtime/version; preserve native options and histories | Beta |
| Context | Files, instructions, skills, MCP | Distinguish discovered, installed, enabled, effective, and reachable; show why an item wins or is unavailable | Alpha inventory, beta editing |
| Concurrent work | Worktrees and multiple sessions | Visible ownership, resource limits, collision detection and review inbox | Beta |
| Recovery | Transcript persistence and resume | Reconcile uncertain sends; retain drafts and pending review; never blindly replay mutations | Private alpha |
| Delivery | Git and PR status | Tie verification to revision and environment; expose unverified checks | Alpha local review, beta PR |
| Remote work | Desktop continuity, increasingly mobile control | Plan native client plus Rust host only when local foundations are reliable | Later parity |
| Automation | Reusable scheduled runs | Explicit execution scope, durable run records, overlap limits and review output | Later parity |

These are product choices inferred from the evidence and the user's workflow. This review does not establish that every competitor lacks them.

## Native stack findings

At research time GPUI-CE main was `ae0af3e862eb21533776b07d44df304b6f0e9264`. GPUI Kit main was `d2304b9063b902fc7ac19b97a7cb5bf3e650b4f5`. Published packages inspected locally were `gpui-ce 0.2.2`, `gpui_ce_platform 0.1.0`, and `gpui-kit 0.6.0`.

GPUI Kit's workspace aliases `gpui` to package `gpui-pre 0.3.1`. GPUI-CE's workspace aliases it to package `gpui-ce 0.2.2`. Cargo package identity matters: using the same import name does not make entities, contexts, or rendering traits compatible. A Cargo alias alone cannot bridge the two type systems. [Kit manifest at inspected revision](https://github.com/longbridge/gpui-kit/blob/d2304b9063b902fc7ac19b97a7cb5bf3e650b4f5/Cargo.toml), [CE manifest at inspected revision](https://github.com/gpui-ce/gpui-ce/blob/ae0af3e862eb21533776b07d44df304b6f0e9264/Cargo.toml)

GPUI Kit offers useful controls, native rich content, virtual lists, editor components, and resizable docking. It also offers webview and script-hosting packages, which Kodem must not include. The user wants all Kodem application logic and UI in Rust. Rust layout methods that resemble web terminology still build native GPUI elements and do not introduce HTML or CSS. [GPUI Kit](https://github.com/longbridge/gpui-kit), [GPUI-CE](https://github.com/gpui-ce/gpui-ce)

The implementation gate is therefore a small, pinned compatibility branch that ports only the needed Kit/base components onto CE and compiles them with the application. A native CE-only probe compiled successfully; an unmodified Kit Button failed CE's IntoElement bound with E0277. The Kit requirement resolved to gpui-pre 0.3.4 in that probe. If the port cost is too high, report it with concrete compiler failures. Do not quietly substitute gpui-pre for the requested CE stack. Native CE controls provide a fallback to evaluate with the user; the current stage does not claim that Kit integration works.

Provider programs are external tools and may themselves use other languages. Kodem must not embed their JavaScript SDKs into its backend. Prefer structured subprocess or network protocols. Claude approval integration remains a separate proof obligation.

## Typography and accessibility

Recommended families are IBM Plex Sans for interface and conversation, JetBrains Mono for code, and Manrope for the wordmark. Bundle licensed static font files with their notices, verify embedding rights at the selected revision, and allow users to override them. Their published sources include license information. [IBM Plex](https://github.com/IBM/plex), [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono), [Manrope license](https://github.com/google/fonts/blob/main/ofl/manrope/OFL.txt)

The screenshots are useful for placement, but their tiny secondary labels and broad low-contrast areas should not define the final tokens. Specify measurable contrast, keyboard routes, accessible names, selection state, and font scaling. Native rendering does not automatically provide correct accessibility.

## Evidence limits and ongoing tracking

No competitor application was launched, controlled, visually tested, or screenshotted. No latency, memory, reliability, or usability benchmark was run against competitors. Generated mockups were rejected and removed; the user cancelled further image generation. The supplied screenshots remain interaction and workspace references. Compilation results and remaining stack risks are in [validation.md](validation.md).

Before each provider release, check the installed CLI version, protocol schema, supported actions, extension discovery rules, and upstream changelog. Before changing UI dependencies, inspect both pinned manifests and compile the component seam. Record date, version, changed capabilities, and affected acceptance criteria. Do not run a background monitoring service or update providers automatically.

The first competitor refresh should prioritize Codex app-server, Claude approval transport, Pi extension behavior, ACP capabilities, GPUI-CE, and GPUI Kit. Price and model names should come from runtime metadata where available; this plan does not hard-code a supposedly permanent best model.

## Interface survey, 12 October 2026

The user asked for a second survey of agentic coding tools before a redesign, with attention to accounts, transcripts and attention handling. This is a review of official documentation, changelogs, repositories and issue trackers. Pages were read through a summarizing fetcher, so exact strings may differ. No product was used hands-on.

| Product | What its documentation shows | What Kodem took |
| --- | --- | --- |
| T3 Code | Sidebar sections Pinned, Active, Snoozed and Settled. The send button changes between interrupt, steer and queue. Provider "instances" bind a thread to a `CLAUDE_CONFIG_DIR`, and a Codex "shadow home" keeps its own sign-in while sharing sessions, so a thread can continue under another account. A usage page shows limits per account. Streaming one event per token made it slower than the CLI. ([composer](https://github.com/pingdotgg/t3code/blob/main/docs/user/composer.md), [thread sidebar](https://github.com/pingdotgg/t3code/blob/main/docs/user/thread-sidebar.md), [Codex providers](https://github.com/pingdotgg/t3code/blob/main/docs/user/providers-codex.md), [issue 695](https://github.com/pingdotgg/t3code/issues/695)) | One configuration directory per account with shared session storage. A send button that offers stop on an empty draft and steer or queue on a draft, with Command-Return for the other one. |
| Synara | Named accounts per provider, shown as tabs of the model picker. Adjacent tool activity folds into one live line. Subagent launches share one card. ([provider accounts](https://trysynara.com/docs/features/provider-accounts), [changelog](https://github.com/Emanuele-web04/synara/blob/main/CHANGELOG.md)) | Accounts inside the model menu. The folded work trail with one live line. |
| MonoCode | Named accounts switched from a usage footer. A project remembers its account. A limit notice offers another saved account. Settled turns fold into a work trail and errors stay visible. ([changelog](https://github.com/hardbeat920/monocode/blob/main/CHANGELOG.md)) | The usage strip in the sidebar footer. A per-project default. The limit notice with a switch action. Failed calls that stay visible in a folded run. |
| Superset | Account cards with email, plan, and session and weekly meters. Running agents keep their account. ([usage](https://docs.superset.sh/usage)) | The account cards in Settings. |
| Claude Code desktop and agent view | Sessions group as needs input, working, ready for review and completed. Permission modes cycle with Shift-Tab. Plan approval has explicit choices. ([desktop](https://code.claude.com/docs/en/desktop), [agent view](https://code.claude.com/docs/en/agent-view), [permission modes](https://code.claude.com/docs/en/permission-modes)) | Feed sections Needs you, Working and Ready for review. Shift-Tab cycles access modes that still ask or plan. Plan review inside the composer. |
| Codex app | An activity view lists unread, running and waiting chats, with a shortcut for the next chat that needs attention. The review pane scopes changes by unstaged, staged, commit, branch and last turn. ([notifications](https://learn.chatgpt.com/docs/notifications), [commands](https://learn.chatgpt.com/docs/reference/commands)) | Unread results. Option-Command-A for the next task that needs the user. Command-1 to Command-9. |
| Factory | Sessions that wait for the user sort first and are bold. ([release notes](https://docs.factory.com/docs/changelog/release-notes)) | The same ordering, as sections. |
| Zed | Approval offers once, deny, and always for a tool or pattern. Messages sent during generation queue by default with a send-now action. ([tool permissions](https://zed.dev/docs/ai/tool-permissions), [agent panel](https://zed.dev/docs/ai/agent-panel)) | Decisions from the provider shown as labelled buttons. A decline can carry feedback. |
| Warp and Roo Code | A task-list chip that expands, and a context bar that changes color near its limit. ([task lists](https://docs.warp.dev/agents/capabilities/task-lists/), [context condensing](https://roocodeinc.github.io/Roo-Code/features/intelligent-context-condensing)) | The plan strip above the composer. Context meter tones. |
| Vibe Kanban and Sculptor | A Command-K palette whose sections follow the current context. ([command bar](https://vibekanban.com/docs/workspaces/command-bar.md)) | The command palette. |

Account handling has two constraints that shaped the design. Anthropic documents one `CLAUDE_CONFIG_DIR` per account as the way to stay signed in to several accounts, and its terms forbid third-party applications from collecting, storing or relaying claude.ai credentials. ([authentication](https://code.claude.com/docs/en/authentication), [legal and compliance](https://code.claude.com/docs/en/legal-and-compliance)) Tools that swap one credential between accounts report forced sign-outs, because refresh tokens are single-use. ([claude-swap](https://github.com/realiti4/claude-swap), [clauth](https://docs.rs/crate/clauth/0.7.3)) Kodem therefore never reads credentials. It starts the provider's own CLI with a separate configuration directory per account, and the CLI performs the sign-in. Kodem suggests another account when a limit is reached. It does not rotate accounts on its own.

Not adopted in this change: provider handoff inside one conversation, side chats, checkpoints and rewind, pull request and CI controls, per-hunk staging, scheduled tasks, and a kanban view.
