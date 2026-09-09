# Candidate A: one desktop owner

Research and design sketch, 9 September 2026. No application implementation.

## Usage, written first

Open a project, start an isolated task, review its changes, and send precise feedback. GPUI views submit user intent and subscribe to immutable results. They never coordinate process handshakes or database transactions.

```rust
// Caller sketches; names below describe proposed Rust interfaces.
let task = workspace.start_task(StartTask {
    project, runtime, checkout: CheckoutChoice::NewWorktree, prompt,
}).await?;
chat.observe(workspace.watch_task(task));

let snapshot = workspace.diff(task, DiffBase::WorkingTree).await?;
let selection = snapshot.select_gutter_rows(gutter_drag)?;
workspace.send_feedback(task, selection, "Preserve cancellation ownership").await?;

// Resolve only the exact request presented in this session.
workspace.answer_approval(task, approval_id, ApprovalDecision::AllowOnce).await?;
```

## Problem

Kodem needs native, responsive chat and Git review around several independently evolving coding runtimes. One application owns its local records and presentation. External agents remain external processes with their own authentication, permissions, and history. The main risks are mismatched capabilities, ambiguous recovery, and feedback attached to the wrong revision.

## Shape

Use one Rust executable, GPUI-CE on the UI thread, asynchronous workers for processes and Git, and SQLite in WAL mode behind one writer task. Each runtime session owns its transport, request IDs, cancellation, and bounded mailbox. Each concurrent task gets a separate worktree by default. Repository actors serialize shared Git metadata operations; external Git writers still require lock/error handling and revision revalidation.

```rust
struct TaskId(Uuid);
struct RuntimeId(Uuid);
struct ApprovalId(Uuid);
struct SnapshotId(ContentDigest);
struct WorktreeId(Uuid);

enum TaskState {
    Idle,
    Running { turn: TurnId },
    Waiting { turn: TurnId, approval: ApprovalId },
    Interrupted { recovery: RecoveryChoice },
    Finished { outcome: Outcome },
}

enum FeatureSupport<T> { Available(T), Unavailable(Reason) }
struct RuntimeCapabilities {
    resume: FeatureSupport<ResumeMode>,
    steering: FeatureSupport<SteeringMode>,
    approvals: FeatureSupport<ApprovalMode>,
    attachments: SupportedContent,
    skills: FeatureSupport<SkillDiscovery>,
    mcp: FeatureSupport<McpDiscovery>,
}

enum DiffLine {
    Context { old: LineNumber, new: LineNumber, text: TextRef },
    Removed { old: LineNumber, text: TextRef },
    Added { new: LineNumber, text: TextRef },
}
struct ReviewSelection {
    snapshot: SnapshotId,
    file: FileDeltaId,
    rows: NonEmptyRange<DiffRowId>,
}

impl Workspace {
    /// Creates task records and checkout ownership before starting an agent.
    async fn start_task(&self, request: StartTask) -> Result<TaskId>;
    /// Subscribes to persisted state and subsequent ordered updates.
    fn watch_task(&self, task: TaskId) -> TaskSubscription;
    /// Captures an immutable diff with exact base and target identities.
    async fn diff(&self, task: TaskId, base: DiffBase) -> Result<DiffSnapshot>;
    /// Resolves stored rows into a revision-labelled feedback attachment.
    async fn send_feedback(&self, task: TaskId, selection: ReviewSelection,
                           message: &str) -> Result<SubmissionId>;
    /// Answers a pending request once, without carrying approval to later requests.
    async fn answer_approval(&self, task: TaskId, request: ApprovalId,
                             decision: ApprovalDecision) -> Result<()>;
}

trait RuntimeAdapter {
    /// Establishes transport and reports verified capabilities for this connection.
    async fn connect(&self, launch: RuntimeLaunch) -> Result<RuntimeConnection>;
}
impl DiffSnapshot {
    /// Maps gutter hit targets to valid content rows, excluding headers and folds.
    fn select_gutter_rows(&self, drag: GutterDrag) -> Result<ReviewSelection>;
}
```

Signatures are non-executable sketches; bodies are intentionally unimplemented. IDs and validated ranges have private constructors. Boundary parsers reject malformed wire messages; typed internal events carry no transport types, per boundary-discipline. Capabilities determine UI availability and preserve provider-specific controls. A single generic "supports chat" flag would conceal material differences.

## Module map

| Module | Knowledge it owns |
| --- | --- |
| `ui` | GPUI entities, typography, pane layout, virtual chat/diff rows, keyboard and gutter input |
| `workspace` | Tasks, durable command submission, session ownership, local queries and recovery |
| `runtimes` | Process supervision and separate Codex, Claude, Pi, OpenCode, ACP parsers |
| `git` | Worktrees, comparisons, immutable diffs, selection anchors, staged operations |
| `catalog` | Skill/MCP source scope, discovery precedence, runtime activation and health |
| `storage` | SQLite migrations, writer task, transcript chunks, indexed queries, attachment files |

Start as modules in one crate. Extract crates only when a compile or dependency boundary pays for itself. The Workspace API hides persistence and runtime lifecycle in one call, per minimize-reader-load. It is not a global mutable container; transport and Git state retain distinct owners.

## Verified adapter directions

- Codex app-server offers JSONL stdio, initialization, thread/turn operations, streamed items, steering and approvals. Generate schemas from the installed version; keep experimental capabilities disabled initially. [Codex app-server](https://learn.chatgpt.com/docs/app-server)
- Claude exposes subprocess JSON and streaming JSON, session IDs, explicit resume, and initialization metadata. Rust can use the CLI without embedding a TypeScript SDK. Interactive approval hosting needs a separate proven bridge; plain `-p` is insufficient. [Programmatic Claude](https://code.claude.com/docs/en/headless), [SDK language support](https://platform.claude.com/docs/en/agent-sdk/overview)
- Pi has JSONL stdin/stdout RPC, correlated commands, asynchronous events, and distinct steering/follow-up behavior. Preserve that distinction. [Pi RPC](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md)
- OpenCode exposes HTTP/OpenAPI, SSE events, session status, asynchronous prompts and permission responses. Supervise a loopback server with authentication. [OpenCode server](https://opencode.ai/docs/server/)
- ACP negotiates versions and capabilities; omitted capabilities are unsupported. Advertise filesystem and terminal operations only when implemented. ACP is another adapter, not a claim of universal feature parity. [ACP initialization](https://agentclientprotocol.com/protocol/v1/initialization)

## Recovery and review invariants

Persist submission intent before sending. Record acknowledgement separately. A disconnect between those states is "delivery unknown" until provider history reconciles it; never blindly resend a command. Approval decisions bind to session, turn and request. Drain stdout independently of rendering. Large output goes to bounded chunks with transcript paging. Completed records become visible after their database transaction commits.

Store provider session IDs alongside the local transcript, without rewriting provider history. On restart, reconcile interrupted sessions where supported; otherwise preserve evidence and offer an explicit new turn. No promise that work continues after application exit. Migrations use recoverable backups; secrets stay in OS credential storage.

Diff anchors include snapshot, file identity, old/new coordinates and content. Removed lines have no invented new-side line number. Gutter dragging captures pointer movement independently of text selection, supports autoscroll and keyboard extension, and maps wrapped rows back to original lines. Folded context is not silently selected. A drag stays within one file; multiple selections form an attachment collection. Diff refresh preserves the old snapshot until the selection is resolved. Staging requires fresh revision validation; historical feedback remains labelled with its original revision.

## Tradeoffs and alternatives

We accept application-exit interruption for simpler installation, ownership and debugging. A separate daemon hides process lifetime from the UI but introduces IPC compatibility, service upgrades and another failure boundary. An embedded universal agent loop offers consistent events but duplicates provider behavior and weakens compatibility. Neither is the initial shape.

## Open risks and next step

Can GPUI-CE and gpui-kit share one GPUI dependency? Parent research found different package families; no compatibility is assumed. Can each runtime provide interactive approval and reconciliation contracts at its supported version? These are implementation gates.

First compile the dependency integration, then prove one Codex turn through persistence into native chat. No visual testing or application control is authorized. Compilation alone will not establish interaction quality.

## Synthesis decision

The final comparison selected candidate B's on-demand host and retained this candidate's session ownership, capability and review rules. See [the synthesized architecture](../architecture.md).
