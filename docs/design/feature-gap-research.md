# Kodem feature gap research

Researched 22 October 2026 against local Kodem commit `f59525b`. This is a feature comparison and proposed prioritization, not an implementation commitment.

Three research agents covered MonoCode/T3 Code/Synara, Codex/Claude Code, and Zuse/Factory/Cursor. The parent inspected Kodem's implementation, checked key public sources, and added Conductor and Superset comparisons. All investigation was read-only source and documentation research. No applications were controlled, screenshotted, or visually tested.

## Findings

Kodem already covers much of the local agent workspace. Its clearest gaps are completing the PR feedback loop, preparing runnable worktrees automatically, retaining explicit goals across turns, scheduling work, and accessing work remotely. Larger opportunities include cross-repository coordination, reusable execution environments, issue-driven workflows, and shared knowledge.

The most useful additions make existing agent work easier to start, verify, finish, and resume. Adding more provider names alone would address less of the observed workflow gap.

“Missing” below means no first-class Kodem implementation was found in the inspected models, host, relevant UI, and integration code. It does not mean an agent cannot perform a similar operation through shell commands, upstream provider tools, skills, or an externally configured MCP server. Public documentation establishes advertised/documented behavior; competitor runtime quality was not independently tested. Links to repository `main` can change after this research date.

## Existing Kodem capabilities that must not be counted as gaps

| Capability | Implementation evidence |
| --- | --- |
| Five provider integrations: Codex, Claude Code, OpenCode, Antigravity and Grok; the latter two are beta | [Provider integration](../implementation/providers.md), [provider routing](../../crates/kodem-host/src/provider.rs) |
| Local task worktrees, isolated delegated writers, cross-provider delegation and subagent inspection | [Task creation and dispatch](../../crates/kodem-host/src/main.rs), [delegation](../../crates/kodem-host/src/delegation.rs), [agent tools](../../crates/kodem-host/src/mcp.rs) |
| Task forks, per-turn file checkpoints and repeated last-turn undo | [Checkpoint implementation](../../crates/kodem-core/src/git/checkpoint.rs), [undo and fork eligibility](../../crates/kodem-host/src/provider.rs) |
| Provider handoff between turns, using the prior user/assistant history as context in a new provider conversation | [Provider integration](../implementation/providers.md), [task/provider operations](../../crates/kodem-host/src/provider.rs) |
| Queued input, steering, durable delivery tracking, async questions and continued local execution with the window closed | [Orchestrator](../../crates/kodem-host/src/orchestrator.rs), [host](../../crates/kodem-host/src/main.rs) |
| Embedded browser with agent tools, console output, screenshots and a visible agent cursor | [Browser contract](../../crates/kodem-core/src/browser.rs), [browser implementation](../../crates/kodem-desktop/src/browser), [agent tools](../../crates/kodem-host/src/mcp.rs) |
| Git history, branches, staging, commits, remote operations, GitHub PR listing and AI review | [Git engine](../../crates/kodem-git/src/git/repo.rs), [PR pane](../../crates/kodem-desktop/src/ui/git/pull_requests.rs) |
| Detached read-only review worktrees and a task to implement the review's findings | [Review workflow](../../crates/kodem-desktop/src/ui/review.rs), [review targets](../../crates/kodem-core/src/git/review.rs) |
| Text editing within working-tree diffs; read-only general file browsing | [Diff editing](../../crates/kodem-desktop/src/ui/file_edit.rs), [file browser](../../crates/kodem-desktop/src/ui/browser.rs) |
| Manual named project commands, sequential script steps and recorded checks | [Project commands](../../crates/kodem-desktop/src/ui/project_commands.rs), [check execution](../../crates/kodem-host/src/terminal.rs) |
| CLI-thread import, skill attachment/catalog discovery, MCP inventory, usage meters and compaction controls | [Imports](../../crates/kodem-host/src/imports.rs), [resource pane](../../crates/kodem-desktop/src/ui/panels.rs), [provider integration](../implementation/providers.md) |

The README's “Current scope” paragraph is stale: it calls extra providers and native editing unimplemented, and describes undo less completely than the current code. Its Git description also claims PR merging, while the inspected PR pane only lists PRs, opens GitHub, and runs AI review. This report follows the implementation in those cases.

## Highest-value additions

Priorities are product judgments based on reuse of Kodem's existing structure and the user workflow improved. They are not engineering estimates. “Moderate” and “large” describe relative scope; provider protocol support still needs implementation-time checks.

| Priority | Feature Kodem is missing or only partly supports | Competitor evidence | Concrete Kodem addition | Relative scope |
| --- | --- | --- | --- | --- |
| 1 | **A complete PR workflow tied to the coding task** | [Synara PR workflow](https://www.trysynara.com/docs/workflows/pull-requests), [Zuse PR actions](https://docs.zuse.sh/workspace/changes-pull-requests) | Link a task to a PR; create/update a draft; show current head, checks and review threads; turn failures/comments into captured fix context; expose ready/merge actions when authorized. | Moderate–large |
| 2 | **Automatic workspace setup and cleanup** | [Conductor scripts](https://www.conductor.build/docs/reference/scripts), [Codex local environments](https://learn.chatgpt.com/docs/environments/local-environment) | Repository-owned setup/archive configuration, selected ignored-file copying, visible setup status, and an option to start the agent only after successful setup. Reuse existing manual project actions. | Moderate |
| 3 | **Persistent goals with controlled continuation** | [Synara thread goals](https://www.trysynara.com/docs/features/thread-goals), [Codex long-running work](https://learn.chatgpt.com/docs/long-running-work) | Save an objective and completion criteria independently of the current plan; show pause/resume/blocked/achieved state; continue across clean turns and restarts while yielding to user input and approvals. | Moderate |
| 4 | **Scheduled tasks and a results inbox** | [Codex scheduled tasks](https://learn.chatgpt.com/docs/automations), [Synara automations](https://www.trysynara.com/docs/workflows/automations) | Save a prompt, repository, provider, permissions and cadence; create isolated runs; retain history and failures; collect results for review. State clearly whether execution needs the local computer awake. | Moderate–large |
| 5 | **Actionable review findings** | [Claude Desktop review](https://code.claude.com/docs/en/desktop#review-your-code) | Track findings individually with file/line, severity, fixed/dismissed state, per-finding fix actions and a recheck. Existing “Fix in Agentic Coding” transfers the report as a whole. | Moderate |
| 6 | **A process and port dashboard** | [Superset ports](https://docs.superset.sh/ports), [MonoCode changelog](https://github.com/hardbeat920/monocode/blob/main/CHANGELOG.md) | Show each workspace's servers, listening ports and resource use; open the right preview, identify ownership and stop a selected process/tool. | Moderate |

PR automation should build on the first row rather than arrive as an opaque “auto-fix” switch. Capture the failing check run or review comment against the current PR head, allow inspection, then offer bounded repair and rerun. Claude Desktop documents this integrated monitoring/repair behavior in its [PR monitoring workflow](https://code.claude.com/docs/en/desktop#monitor-pull-request-status).

The setup gap is specifically lifecycle behavior. Kodem already has manually launched script chains in `ProjectCommand`; new worktree creation does not run them automatically. Conductor additionally allocates a workspace port range. Superset discovers active ports but explicitly does not allocate ranges by default; those are different features.

## Larger platform opportunities

| Feature | Competitor evidence | Kodem's present boundary and possible addition |
| --- | --- | --- |
| **SSH hosts and mobile continuation** | [T3 remote access](https://github.com/pingdotgg/t3code/blob/main/docs/user/remote-access.md), [T3 clients](https://github.com/pingdotgg/t3code), [Codex remote connections](https://learn.chatgpt.com/docs/remote-connections) | Kodem's host uses a local socket; there is no remote-host registry, pairing or phone client. Add SSH hosts first, then authenticated remote steering, approval and review. T3 also documents resource-aware placement across machines. Codex availability can depend on rollout. |
| **Managed cloud execution** | [Factory Droid Computers](https://docs.factory.com/droid-computers/overview), [Zuse repository](https://github.com/swarajbachu/zuse) | Keeping Kodem's window closed preserves local execution; laptop sleep/power loss still affects it. Hosted computers would support work without the laptop. Factory documents retained machine state; Zuse's shipped cloud features remain invitation-gated in an alpha/beta product. |
| **Reusable environments and coordinated multi-repository work** | [Factory environments](https://docs.factory.com/environments/run-droids), [Cursor cloud environments](https://cursor.com/docs/cloud-agent/setup) | Kodem can include linked repositories in diffs and baselines, but a task has one owning project/cwd. Add explicit repository sets, repeatable setup and per-repository branch/PR outcomes. Factory uses separate agent instances from environment builds. |
| **Coordinator workspaces with shared memory and deliverables** | [Synara Hubs](https://www.trysynara.com/docs/features/hubs) | Delegation already exists. Missing is a durable coordinator spanning repositories, a persistent bounded worker queue, shared editable memory and a versioned deliverable library. Synara labels Hubs available in Stable 1.0.2. |
| **Milestones, success criteria and validation orchestration** | [Factory Missions](https://docs.factory.com/missions/overview) | Kodem's `Task.plan` is the current step list, while lineage identifies parent/child tasks. Add durable feature/milestone status, worker ownership and required verification before completion. Missions' ability to self-validate depends on scriptable project QA. |
| **Engineering-event automations** | [Cursor automations](https://cursor.com/docs/cloud-agent/automations), [Factory automations](https://docs.factory.com/software-factory/automations) | No native event-trigger/run-history model was found. Add CI completion, review comments, issue labels or tracker transitions as explicit run triggers. Factory inbound webhooks are Private Preview; do not equate them with its ordinary schedule/Slack/GitHub features. |
| **Issue-to-agent delegation** | [Factory Linear delegation](https://docs.factory.com/delegations/linear) | Add an issue as the task's origin, execute through an agreed environment and return progress/result links. Factory distinguishes assignment/mention-based delegation from a personal Linear connector; merely exposing Linear tools is less complete. |
| **External control through a supported CLI/API/MCP service** | [T3 outside agents](https://github.com/pingdotgg/t3code/blob/main/docs/user/outside-agents.md) | Kodem's task-scoped MCP server exposes delegation/status/cancel and browser tools. It is not a documented external authenticated service for listing projects, starting arbitrary tasks, posting follow-ups or reading results. Expose those operations with explicit access ceilings. |

Remote access and hosted compute should be separate roadmap items. An SSH connection to the user's workstation still requires that workstation to remain available; a managed environment changes where the work runs.

## Additional feature gaps worth considering

| Feature | Evidence and maturity | What is actually missing in Kodem |
| --- | --- | --- |
| **MCP connector setup and sign-in UI** | [Synara Connectors](https://www.trysynara.com/docs/features/connectors): Beta-only, starting 1.0.6-beta.1 | Add/remove a server, OAuth sign-in, inspect connection health and choose scope across providers. Kodem currently displays provider inventory; MCP use itself is already possible. |
| **More source-control hosts** | [T3 source control](https://github.com/pingdotgg/t3code/blob/main/docs/user/source-control.md) documents GitLab, Forgejo/Gitea, Bitbucket, Azure DevOps and GitCafe alongside GitHub. | Native PR/MR discovery and actions beyond GitHub. Ordinary Git remotes already work independently of hosting provider. |
| **PR inbox, stacks and guided review** | [Codex Code Review](https://learn.chatgpt.com/docs/code-review), [T3 review guides](https://github.com/pingdotgg/t3code/blob/main/docs/user/source-control.md#review-guides): T3 guides are opt-in beta; Codex GitLab review is preview. | Cross-repository assigned/authored/team views, viewed-file tracking, dependent PR navigation and an ordered reading guide explaining important decisions. |
| **Conversation time travel at a chosen turn** | [Claude checkpointing](https://code.claude.com/docs/en/checkpointing) offers code/conversation restore choices, with coverage limitations. | Kodem can undo successive latest turns and fork its latest saved state. Missing is selecting an arbitrary earlier turn, editing its prompt and branching there, plus explicit code-only versus context-only restore. |
| **Local ↔ worktree handoff** | [Codex worktrees](https://learn.chatgpt.com/docs/environments/git-worktrees) | Move an existing task and its changes between the main checkout and an isolated worktree. Kodem creates a new worktree for a fork; that is a related but different operation. |
| **Side questions and context diagnostics** | [Claude Desktop](https://code.claude.com/docs/en/desktop) documents side chats and categorized context inspection. | Ask a contextual question without steering or enlarging the main conversation; inspect which instructions/files/tools consume context. Kodem already has aggregate occupancy and compaction controls. |
| **Search within and across conversation content** | [Claude sessions](https://code.claude.com/docs/en/sessions) documents session search/resumption. | Kodem's inspected feed search matches task titles, branches and project names. Add message/tool-output matching and navigation to the matching turn. Backend SQL use of `LIKE` for migrations is not a search feature. |
| **Shared knowledge with provenance** | [Factory AutoWiki](https://docs.factory.com/software-factory/wiki/generate), [Synara Hubs](https://www.trysynara.com/docs/features/hubs) | Inspectable project knowledge with source links, revision history, updates and reuse across providers. Existing instruction files and provider memory are not absent; the gap is the shared Kodem experience. |
| **Repository readiness assessment** | [Factory readiness report](https://docs.factory.com/agent-readiness/readiness-report) includes historical scoring and remediation. | Detect missing deterministic build/test/setup instructions and offer concrete repairs before an unattended task. Could be delivered as a workflow/skill before building a dashboard. |
| **Point-at-page feedback** | [Superset browser](https://superset.sh/browser) documents selecting a page element and sending a note with element context and a cropped image. | Kodem already has an agent-controlled preview. Missing is the user's direct element-selection-to-composer flow. This row describes the feature only; no browser was operated for this research. |
| **iOS device/simulator workflow** | [Synara simulator](https://www.trysynara.com/docs/features/ios-simulator) documents live stream, input, accessibility and app install/launch; requires macOS and Xcode. | A native app development companion beyond the web preview. Useful if mobile developers are a target audience; narrow compared with PR/setup/automation work. |
| **Organization-owned automation and audit** | [Factory identity/access](https://docs.factory.com/enterprise/identity-and-access) documents service-account ownership and audit attribution. | Shared ownership, policy and audit history for unattended work. This is an enterprise direction, not a necessary addition for a personal local workspace. |

## Provider features that need integration checks, not a fresh implementation

Hooks, skills, custom agents, memory, MCP and session resume already exist in the upstream runtimes. Kodem has native integrations and resource discovery. Do not label these capabilities wholly absent without checking whether the selected CLI already loads and executes them.

Potential additions are an editor, a consistent provider-neutral presentation, explicit scope/trust controls, and useful visibility into results. [Claude subagents](https://code.claude.com/docs/en/sub-agents) and [Claude hooks](https://code.claude.com/docs/en/hooks-guide) document configuration already owned by the runtime. [Codex memory](https://learn.chatgpt.com/docs/customization/memories) is off by default; absence of a visible memory setting in Kodem does not prove the runtime cannot use memory.

[Claude agent teams](https://code.claude.com/docs/en/agent-teams) are experimental and disabled by default. Peer communication/shared task ownership are comparison ideas, but normal subagents should not be described as team orchestration, or vice versa. [MonoCode Monos](https://github.com/hardbeat920/monocode) are also explicitly experimental; their persistent cross-project memory and scheduled habits are useful directional evidence, not a baseline that all mature tools already meet.

Automatic account rotation after a rate limit is excluded from the recommendations. Kodem already supports multiple accounts and deliberately retains a started task's account at its limit; that is a documented product decision in [provider integration](../implementation/providers.md), not an overlooked account-management feature.

## Recommended sequence

1. **Finish one local task cleanly:** native PR association/draft delivery, current checks and review context, actionable findings, bounded repair.
2. **Make parallel worktrees runnable:** setup/archive lifecycle, selected ignored-file handling, process/port ownership and preview launch. Keep existing manual project actions.
3. **Make longer work explicit:** durable goals, pause/resume and completion criteria, then local scheduled runs with an inbox and failure history.
4. **Make the host accessible:** supported task-control CLI/API, SSH hosts and remote approvals. Treat mobile UI and hosted cloud execution as separate investments.
5. **Coordinate broader work:** shared repository sets, coordinator memory/artifacts, milestone validation and issue/event triggers.

This order is an inference about payoff for Kodem's present architecture. User segment, usage evidence and implementation constraints could change it. For a team/enterprise product, issue delegation and hosted environments would move earlier; for an individual local workspace, the first three steps offer the closest fit.

## Validation and limits

Kodem gap judgments were checked against current task/request/settings models, task creation, delegation/MCP tools, undo/fork behavior, project commands, resources UI, GitHub functions and PR pane. Positive capabilities were traced to implementation instead of relying on the roadmap. Absent first-class features are conclusions from the inspected surfaces and targeted repository searches, not proof that no external workaround exists.

No application behavior or competitor UI quality was verified. No compiled code was changed, so compilation was not required. The only delivered repository change is this research document; no commit or push was made.
