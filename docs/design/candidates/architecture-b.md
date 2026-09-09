# Candidate B: durable local supervisor

## Problem

Kodem ADE must keep agent runs alive when its window closes, recover after a crash, and present incompatible agents through one native Rust UI. Provider sessions have different lifecycles, streaming events are transient, and Git files can change during selection. The frontend is GPUI-CE 0.2.2 with no HTML, CSS, or WebView. `gpui-kit` main depends on `gpui-pre` 0.3.1, so it remains excluded until a compile probe proves one GPUI type graph ([gpui-kit manifest](https://github.com/longbridge/gpui-kit/blob/main/Cargo.toml), [GPUI-CE 0.2.2](https://docs.rs/crate/gpui-ce/0.2.2)).

## Usage (caller's view)

A user opens a repository and starts a built-in or external agent. Closing the window leaves the run in the supervisor. Reopening restores its transcript, status, and layout. The resizable sidebar groups repositories and sessions and exposes theme, density, grouping, and collapsed sections.

The Diff tab shows staged, unstaged, untracked, and conflicted changes. Dragging a gutter selects one side of an immutable snapshot. "Add to feedback" inserts a chat chip. If the worktree changes, the chip becomes stale rather than moving to new lines.

Skills and MCP views say whether each item is configured, discovered, connected, or available to this session. Missing provider support appears as unavailable, never as an empty successful inventory.

```rust
let ade = SupervisorClient::connect_or_start().await?;
let repo = ade.open_repo(path).await?;
let session = ade.create_session(repo.id, AgentKind::Codex).await?;
let events = ade.subscribe(session.id, EventSeq::ZERO).await?;

let diff = ade.capture_diff(repo.id).await?;
let range = diff.range(drag_start, drag_end)?;
session.send(CommandId::new(), Draft::new(text).with(range)).await?;
```

## Shape

The supervisor is the sole session writer. GPUI windows are disposable clients that receive a state snapshot and ordered events after a cursor. Both ADE processes are Rust; provider executables remain child integrations. The UI folds events into GPUI entities and renders native components. Theme tokens, split constraints, virtualized chat rows, Markdown, code spans, and the diff gutter live in `ui`; no foreign GPUI component crate crosses that boundary.

```rust
pub struct SessionId(Uuid);
pub struct CommandId(Uuid);
pub struct SnapshotId(Blake3Hash);
pub struct EventSeq(u64);

pub enum SessionPhase { Idle, Running, AwaitingApproval, Interrupted,
    DeliveryUnknown, Failed }
pub enum ChatEvent {
    TextDelta { block: BlockId, append: String },
    ReasoningDelta { block: BlockId, append: String },
    ToolUpdated(ToolRun),
    ApprovalRequested(Approval),
    TurnFinished { usage: Option<Usage> },
}
pub enum DiffLayer { Staged, Unstaged, Untracked, Conflict }
pub enum Side { Old, New }
pub struct DiffSelection {
    snapshot: SnapshotId, file: FileKey, layer: DiffLayer, side: Side,
    old_blob: Option<BlobId>, new_blob: Option<BlobId>,
    first: DiffLineId, last: DiffLineId,
}
pub struct AgentCapabilities {
    resume: Support, fork: Support, steer: Support, approvals: Support,
    skills: Support, mcp: Support, usage: Support,
}

impl EventSeq {
    /// Starts replay at the first retained event.
    pub const ZERO: Self = Self(0);
}

impl SupervisorClient {
    /// Connects to the user-scoped daemon, starting the matching binary if absent.
    pub async fn connect_or_start() -> Result<Self, ConnectError>;
    /// Opens one canonical Git worktree and returns its durable identity.
    pub async fn open_repo(&self, path: PathBuf) -> Result<RepoView, OpenError>;
    /// Creates a provider session using its negotiated capabilities.
    pub async fn create_session(&self, repo: RepoId, agent: AgentKind)
        -> Result<SessionHandle, StartError>;
    /// Replays events after `after`, then continues with live ordered events.
    pub async fn subscribe(&self, id: SessionId, after: EventSeq)
        -> Result<EventStream, SubscribeError>;
    /// Captures an immutable observed view of every Git change layer.
    pub async fn capture_diff(&self, repo: RepoId)
        -> Result<DiffDocument, SnapshotError>;
}

impl DiffDocument {
    /// Validates a same-file, same-side gutter drag against this snapshot.
    pub fn range(&self, start: DiffLineId, end: DiffLineId)
        -> Result<DiffSelection, SelectionError>;
}

impl SessionHandle {
    /// Journals and submits a draft once for the supplied command identity.
    pub async fn send(&self, id: CommandId, draft: Draft)
        -> Result<(), SendError>;
}

trait AgentDriver: Send {
    fn capabilities(&self) -> AgentCapabilities;
    fn start(&mut self, request: StartRequest) -> BoxFuture<'_, Result<NativeSession, DriverError>>;
    fn submit(&mut self, command: AcceptedCommand, sink: EventSink)
        -> BoxFuture<'_, Result<ProviderReceipt, DriverError>>;
    fn recover(&mut self, checkpoint: ProviderCheckpoint, sink: EventSink)
        -> BoxFuture<'_, Result<Recovery, DriverError>>;
    fn inventory(&mut self) -> BoxFuture<'_, Result<InventorySnapshot, DriverError>>;
}
```

Adapters parse wire data into domain events. Codex uses a version-matched app-server schema and initialized JSONL connection with start, resume, fork, and turn events ([Codex app-server](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md)). Claude Code uses print mode, stream-JSON input/output, and `--resume` ([Claude Code CLI](https://code.claude.com/docs/en/cli-reference)). Pi uses JSONL RPC and durable entry cursors ([Pi RPC](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md)). OpenCode uses a spawned loopback server, OpenAPI schema, and SSE ([OpenCode server](https://dev.opencode.ai/docs/server/)). External agents negotiate a versioned sidecar protocol.

SQLite WAL stores commands, events, provider checkpoints, inventories, and Git blobs. `CommandId` is unique. A user-scoped lock and socket permissions prevent duplicate daemon writers; a version handshake rejects incompatible clients. The daemon commits events before publishing them. A crash after prompt send but before provider receipt yields `DeliveryUnknown`; recovery never resubmits it automatically. On restart, `Running` becomes `Interrupted` unless its driver can reconcile without replaying work. UI reconnect replays after `EventSeq`.

Git capture reads HEAD, index stages, status, and worktree contents twice. It accepts consecutive identical manifests, otherwise retries and returns `WorkspaceBusy`. This detects ordinary concurrent edits; it is not an atomic filesystem transaction. The manifest hashes each displayed blob and separates staged, unstaged, untracked, and conflict documents. Binary and submodule entries stay visible but have no selectable lines. Selections refer only to stored blobs and line IDs. Feedback includes the captured excerpt and blob identities, plus staleness against a fresh capture.

Inventory retains source and observation time. Direct MCP discovery follows negotiated capabilities and paginates `tools/list`, `resources/list`, and `prompts/list`; list-change notifications invalidate the cache ([MCP schema](https://modelcontextprotocol.io/specification/2025-11-25/schema)).

```text
crates/
  domain          IDs, events, capabilities, diff selections
  supervisor      command journal, recovery, IPC, process ownership
  adapters        codex | claude | pi | opencode | sidecar
  git_snapshot    observed capture, diff parsing, immutable blob store
  inventory       skill and MCP observations with provenance
  protocol        versioned local request/event wire types
  desktop         GPUI-CE shell, atomic preferences, sidebar, chat, diff
```

This interface is deep: five client operations hide process supervision, protocol drift, replay, deduplication, and Git races. Provider wire types, SQLite rows, and GPUI entities remain private.

## Synthesis decision

Selected as the base, revised to an on-demand local host with fewer crates and candidate A's capability and review rules. See [the synthesized architecture](../architecture.md).

## Tradeoffs accepted

- We accept a daemon, IPC versioning, and migration work in exchange for durable runs and a replaceable UI.
- We accept duplicate Git reads and snapshot storage in exchange for feedback that cannot silently change meaning.
- We accept capability gaps in exchange for honest adapters that do not invent cross-provider parity.

## Alternatives considered

A single GPUI process is easier to ship, but window lifetime becomes run lifetime and views inherit recovery. A daemon storing only provider IDs is smaller, but clients rebuild transcripts and cannot distinguish an unsent prompt from a duplicate. Dynamic Rust adapter libraries expose ABI and dependency coupling that a versioned sidecar hides.

## Open questions and risks

- Should the first release install an always-on daemon, or start it on demand with an idle shutdown?
- Is two-pass Git stability sufficient while external editors write, or must supported filesystems use copy-on-write snapshots?
- Which inventory states can each pinned agent version prove without scanning its private config format?
- What database retention limit keeps raw tool output and diff blobs useful without unbounded growth?

## Next implementation step

Build a headless supervisor spike with SQLite replay, one fake sidecar, and the two-pass Git snapshot contract before adding GPUI or real agents.
