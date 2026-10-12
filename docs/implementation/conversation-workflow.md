# Conversation and review workflow

## Problem

The composer rejected messages during active work, diff text selection stopped at each row, and review comments occupied a separate editor below the diff. The new workflow keeps message composition available during review and gives the host ownership of queued delivery.

## Usage

Selecting a changed file opens its diff above the composer. Settings can move the diff to the right panel. Dragging code selects text across rows; dragging line numbers selects captured context and opens a comment editor below the range. Adding that comment creates a removable context header inside the prompt container, with a bounded scrollable context area. The header shows the file and captured range; the full excerpt and comment stay in the model payload. Changes navigation groups files in foldable folders.

Dropping files imports stable attachment copies. Adding a skill from the catalog creates a removable Skill chip in the same draft context area, preserving its original resource directory and an immutable copy of SKILL.md without inserting text into the editor. Enter sends the current composition; Shift+Enter inserts a newline. The composer has one primary action: Send when idle, Stop while work is active. Empty Send is disabled; text or selected context enables it. While work is active, Enter submits using the selected Queue or Steer preference. Queued messages appear above the composer and can be cancelled. Interrupted or failed work pauses the queue until explicitly resumed.

The composer floats over the transcript with measured bottom padding and a bounded spring response to scrolling, clicking and typing. Progressive native blur is clipped inside the rounded input surface; it covers only the prompt container and its draft context, not queued messages, the activity area or surrounding conversation. The center diff reserves the measured composer height so its final rows remain reachable. Reduce Motion disables spring animation.

The model menu lists only runtime-advertised models. The separate reasoning menu contains its fluid slider with pointer and keyboard input. The access menu contains only permission choices. Access is persisted per task and defaults to Workspace for older tasks. Read only, Workspace, AI auto-approve and Full access map to explicit sandbox and approval policies at thread creation/resume and every new turn. Settings are locked during an active turn. Queued turns use the task's settings at dispatch; steering continues with the active turn's settings. AI auto-approve uses `approvalsReviewer: auto_review` on thread creation/resume and every new turn; other modes explicitly restore `user`. It retains `workspace-write` and `on-request`, so AI review can deny escalations. Protocol 5 includes this access mode as well as saved task review and provider usage metadata. Idle older hosts upgrade through guarded shutdown; active work delays replacement. Connection polling recovers from failed handshakes without replaying rejected operations.

## Shape

The existing submissions ledger stores immutable outgoing content and delivery state. `Submit` names attachment and feedback identities; the host validates and freezes them before acceptance. `TaskView.outgoing` projects that ledger into the tray. The provider worker advances queued work after its current request acknowledgement settles, keeping notifications free of recursive dispatch. Steering uses `turn/steer` with an expected turn identity. Unknown delivery is retained for reconciliation and is never automatically replayed.

Draft attachments belong to `Task`; submitted attachments and feedback belong to each message. The host copies imported files outside the UI thread, and images use the provider's local-image input. The desktop owns placement, selection, and rendering. One diff document owns character selection across virtual rows, preserving syntax colors and bounded rendering. Gutter context refers to immutable snapshot row indices so mixed additions and deletions retain their meaning.

## Synthesis decision

Two independent sketches were compared. The existing-ledger design is the persistence base: it avoids a second queue in task JSON and centralizes atomic acceptance, delivery and recovery. The document-owned selection design supplies the UI: one selection owner, virtual rows, inline comments, and composer-specific submit events. The available agent model was used for both sketches; the skill's external default runners were unavailable.

## Tradeoffs and alternatives

- A schema migration adds transactional delivery state to the existing ledger. A separate task queue would require coordinating two durable records for each dispatch.
- Attachment copies remain available for queued input and retained conversation history. References to original source paths would change meaning when files move or change. A folder is the exception: it can exceed every attachment limit, so it is attached by reference and the agent reads it in place.
- A dedicated diff selection owner adds UI code while retaining virtualization. Shaping a full multi-megabyte diff on each render would simplify selection at a substantial rendering cost.
- Failed, interrupted, and unknown outcomes pause queued execution. This requires an explicit resume action and preserves control over subsequent work.

## Verification

Use source review, temporary nonvisual checks, formatting, Clippy, and debug/release compilation and packaging. Do not launch, control, or screenshot the application. Remove temporary verification files before delivery. The release installer keeps a previous-version backup and verifies installed bytes. It can replace the installed bundle while a previous process runs, and reports the restart needed to load new code.
