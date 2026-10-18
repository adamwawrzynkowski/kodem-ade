//! Provider routing and durable, per-task conversation ownership.
use crate::{
    Shared, accounts,
    orchestrator::{self, Command, Input, Signal},
};
#[path = "providers/claude.rs"]
mod claude;
#[path = "providers/codex.rs"]
mod codex;
#[path = "providers/opencode.rs"]
mod opencode;
use anyhow::{Context, Result, ensure};
use kodem_core::{
    git,
    storage::{Commit, Store},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

/// Shortest time between two writes of one task's streamed transcript rows.
const STREAM_WRITE_INTERVAL: Duration = Duration::from_millis(120);

/// Open transcript items that still receive streamed text, by a key their driver chooses.
///
/// A write stores the whole row under the host lock, so one write for each delta costs more as
/// an item grows. Changed rows are written together instead, at most once per interval. A
/// driver also writes them before any event that is not a delta, so such an event sees current
/// rows, and from its session loop, so text that nothing follows still appears.
pub(crate) struct Streamed {
    pub(crate) rows: HashMap<String, Message>,
    unwritten: HashSet<String>,
    written: Instant,
}
impl Streamed {
    /// Starts with no open item. The first changed row is written at once.
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        Self {
            rows: HashMap::new(),
            unwritten: HashSet::new(),
            written: now.checked_sub(STREAM_WRITE_INTERVAL).unwrap_or(now),
        }
    }
    /// Records that the text of an item changed and returns whether changed rows are due.
    pub(crate) fn changed(&mut self, key: &str) -> bool {
        if !self.unwritten.contains(key) {
            self.unwritten.insert(key.to_owned());
        }
        self.due()
    }
    /// Returns whether a changed row waited for the whole write interval.
    pub(crate) fn due(&self) -> bool {
        self.pending() && self.written.elapsed() >= STREAM_WRITE_INTERVAL
    }
    /// Returns whether the text of any item changed since the last write.
    pub(crate) fn pending(&self) -> bool {
        !self.unwritten.is_empty()
    }
    /// Writes every changed row that is still open.
    pub(crate) fn write(&mut self, store: &Store) -> Result<()> {
        for key in self.unwritten.drain() {
            if let Some(message) = self.rows.get(&key) {
                store.message(message)?;
            }
        }
        self.written = Instant::now();
        Ok(())
    }
}

pub(super) enum Control {
    Close,
    Wake,
    Interrupt,
    /// Runs the compaction that `begin_compaction` recorded for this task.
    Compact,
    Answer {
        id: Value,
        response: Value,
    },
    Query {
        method: &'static str,
        params: Value,
        reply: mpsc::SyncSender<std::result::Result<Value, String>>,
    },
}

/// State a task returns to when its host-requested compaction ends.
pub(crate) struct Compaction {
    state: RunState,
    error: Option<String>,
    updated: u64,
}

pub(crate) fn prompt_for(message: &OutgoingMessage) -> Result<String> {
    let mut prompt = message.text.clone();
    for attachment in &message.attachments {
        if let AttachmentKind::Skill { source } = &attachment.kind {
            prompt.push_str(&format!(
                "\n\nUse the skill at {}. Resolve its relative resources from the original skill directory.\nImmutable copy of the selected SKILL.md: {}",
                serde_json::to_string(source)?,
                serde_json::to_string(&attachment.path)?
            ));
        }
        if attachment.kind == AttachmentKind::File {
            prompt.push_str(&format!(
                "\n\nAttached file: {}\nImmutable local copy: {}",
                serde_json::to_string(&attachment.name)?,
                serde_json::to_string(&attachment.path)?
            ));
        }
        if attachment.kind == AttachmentKind::Folder {
            prompt.push_str(&format!(
                "\n\nAttached folder: {}\nRead it in place at its local path: {}",
                serde_json::to_string(&attachment.name)?,
                serde_json::to_string(&attachment.path)?
            ));
        }
    }
    if !message.feedback.is_empty() {
        prompt.push_str("\n\nReview context refers to captured content. Check current files before applying it:\n");
        for feedback in &message.feedback {
            if let Some(root) = &feedback.repository {
                prompt.push_str(&format!(
                    "\nRepository checkout: {}\n",
                    serde_json::to_string(root)?
                ));
            }
            let coordinates = if feedback.first_row.is_some() {
                format!(
                    "Displayed diff rows {:?}–{:?}; old source {:?}, new source {:?}",
                    feedback.first_row, feedback.last_row, feedback.old_range, feedback.new_range
                )
            } else {
                format!(
                    "Source {:?} lines {}–{}",
                    feedback.side, feedback.start, feedback.end
                )
            };
            prompt.push_str(&format!(
                "\nFile: {}\nSnapshot: {}\n{}\nComment: {}\nCaptured excerpt:\n{}\n",
                serde_json::to_string(&feedback.label)?,
                feedback.snapshot_id,
                coordinates,
                feedback.comment,
                feedback.excerpt
            ));
        }
    }
    ensure!(
        prompt.len() <= 2 * 1024 * 1024,
        "Prompt and context exceed 2 MiB; send a smaller batch"
    );
    Ok(prompt)
}

/// Durably accepts a new input without making the window an execution owner.
pub(super) fn submit(
    host: Shared,
    task_id: String,
    id: String,
    text: String,
    feedback_ids: Vec<String>,
    attachment_ids: Vec<String>,
    mode: SendMode,
) -> Result<()> {
    ensure!(text.len() <= 1024 * 1024, "Prompt exceeds 1 MiB");
    ensure!(
        !text.trim().is_empty() || !feedback_ids.is_empty() || !attachment_ids.is_empty(),
        "Write a prompt or attach context"
    );
    ensure!(
        !id.is_empty() && id.len() <= 128,
        "Invalid submission identity"
    );
    {
        let h = host.lock().unwrap();
        ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
        if h.store
            .matches_submission(&task_id, &id, &text, &feedback_ids, &attachment_ids, mode)?
        {
            return Ok(());
        }
        let mut task = h.store.task(&task_id)?;
        validate_preferences(&h, &preferences(&task))?;
        ensure!(
            mode != SendMode::Steer
                || h.runtimes
                    .get(&task.provider)
                    .is_some_and(|runtime| runtime.steering),
            "This provider does not support steering. Queue the message instead."
        );
        ensure!(
            task.state != RunState::DeliveryUnknown,
            "Delivery is unknown. Reconcile before sending another message."
        );
        ensure!(
            h.store.outgoing(&task_id)?.len() < 64,
            "Outbox is full; send or remove existing messages first"
        );
        // A delegated task that cannot write reads the checkout of its parent while the
        // parent works, so it is no second writer.
        ensure!(
            task.shares_checkout()
                || !h.store.tasks()?.iter().any(|t| {
                    t.id != task.id && t.cwd == task.cwd && t.state.active() && !t.shares_checkout()
                }),
            "Another task is writing to this checkout. Use a separate task worktree."
        );
        // A compaction has no turn to join, so input sent meanwhile starts the next turn.
        let steers =
            task.state.active() && mode == SendMode::Steer && !h.compactions.contains_key(&task_id);
        let expected_turn = if steers {
            Some(task.active_turn.clone().context("The turn is still starting. Queue this message or wait until it begins before steering.")?)
        } else {
            None
        };
        let attachments = attachment_ids
            .iter()
            .map(|id| {
                task.attachments
                    .iter()
                    .find(|a| &a.id == id)
                    .cloned()
                    .context("Attachment is no longer in this draft")
            })
            .collect::<Result<Vec<_>>>()?;
        let feedback = feedback_ids
            .iter()
            .map(|id| {
                task.feedback
                    .iter()
                    .find(|f| &f.id == id)
                    .cloned()
                    .context("Context is no longer in this draft")
            })
            .collect::<Result<Vec<_>>>()?;
        validate_attachments(&h, &task, &attachments)?;
        let message = OutgoingMessage {
            id,
            text: text.clone(),
            attachments,
            feedback,
            created: now(),
            state: DeliveryState::Queued,
            error: None,
            mode,
            expected_turn,
        };
        let prompt = prompt_for(&message)?;
        if task.draft == text {
            task.draft.clear();
        }
        task.attachments.retain(|a| !attachment_ids.contains(&a.id));
        task.feedback.retain(|f| !feedback_ids.contains(&f.id));
        if !task.state.active() {
            task.state = RunState::Idle;
            task.error = None;
        }
        task.updated = now();
        task.last_started = Some(task.updated);
        h.store.accept_outgoing(&task, &message, &prompt)?;
    }
    ensure_owner(host, task_id);
    Ok(())
}

/// Promotes queued input to steering and wakes its existing provider session.
pub(super) fn steer_queued(host: Shared, task_id: &str, id: &str) -> Result<()> {
    let h = host.lock().unwrap();
    let task = h.store.task(task_id)?;
    ensure!(
        h.runtimes
            .get(&task.provider)
            .is_some_and(|runtime| runtime.steering),
        "This provider does not support steering"
    );
    ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
    ensure!(
        !h.compactions.contains_key(task_id),
        "Steer after the context compaction finishes"
    );
    let sender = h
        .sessions
        .get(task_id)
        .context("No live provider session to steer")?;
    h.store.steer_outgoing(task_id, id)?;
    sender.send(Control::Wake)?;
    Ok(())
}

/// Explicitly resumes retained queued input after a failure or a host restart.
pub(super) fn resume_queue(host: Shared, task_id: &str) -> Result<()> {
    {
        let h = host.lock().unwrap();
        let mut task = h.store.task(task_id)?;
        ensure!(!task.state.active(), "This task is already running");
        ensure!(
            task.state != RunState::DeliveryUnknown
                && !h.store.has_uncertain_submission(task_id)?,
            "Reconcile uncertain delivery before resuming the queue"
        );
        ensure!(
            h.store
                .outgoing(task_id)?
                .iter()
                .any(|m| m.state == DeliveryState::Queued && m.expected_turn.is_none()),
            "No queued messages to resume"
        );
        task.state = RunState::Idle;
        task.error = None;
        h.save_task_with_notification(&task)?;
    }
    ensure_owner(host, task_id.into());
    Ok(())
}

/// Starts a compaction of an idle task's conversation through its provider session.
///
/// A running turn is neither interrupted nor queued behind: the request is refused until the
/// turn ends. The result arrives with the task: its activity, context usage and transcript.
pub(super) fn compact(host: Shared, task_id: &str) -> Result<()> {
    {
        let mut h = host.lock().unwrap();
        ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
        let mut task = h.store.task(task_id)?;
        ensure!(
            !task.state.active() && !h.compactions.contains_key(task_id),
            "Compact after this turn finishes"
        );
        ensure!(
            task.state != RunState::DeliveryUnknown
                && !h.store.has_uncertain_submission(task_id)?,
            "Reconcile uncertain delivery before compacting"
        );
        ensure!(
            task.provider_thread.is_some(),
            "There is nothing to compact yet. Send a message first."
        );
        validate_preferences(&h, &preferences(&task))?;
        begin_compaction(&mut h, &mut task)?;
    }
    ensure_owner(host.clone(), task_id.into());
    let sent = host
        .lock()
        .unwrap()
        .sessions
        .get(task_id)
        .is_some_and(|session| session.send(Control::Compact).is_ok());
    if !sent {
        end_compaction(&host, task_id, None)?;
        anyhow::bail!("The provider session ended. Try again.");
    }
    Ok(())
}

/// Marks a host-requested compaction as running and remembers the state the task returns to.
///
/// The task counts as working meanwhile, so queued input waits and nothing else starts a turn.
pub(crate) fn begin_compaction(host: &mut crate::Host, task: &mut Task) -> Result<()> {
    host.compactions.insert(
        task.id.clone(),
        Compaction {
            state: task.state.clone(),
            error: task.error.take(),
            updated: task.updated,
        },
    );
    task.state = RunState::Running;
    task.active_turn = None;
    host.store.save_task(task)
}

/// Returns a task to the state it had before its compaction, without a completion alert.
///
/// A failure leaves a notice in the transcript. It never fails the task or its queued input.
pub(crate) fn end_compaction(host: &Shared, task_id: &str, failure: Option<String>) -> Result<()> {
    let mut h = host.lock().unwrap();
    let Some(previous) = h.compactions.remove(task_id) else {
        return Ok(());
    };
    h.activities.remove(task_id);
    let mut task = h.store.task(task_id)?;
    task.state = previous.state;
    task.error = previous.error;
    task.updated = previous.updated;
    task.active_turn = None;
    // The reading a compaction leaves behind must not start the next one.
    if let Some(usage) = &task.context_usage {
        h.compacted.insert(task_id.into(), usage.reported);
    }
    if let Some(failure) = failure {
        h.store.message(&Message {
            seq: 0,
            transfer: None,
            id: format!("compact:{}", new_id()),
            task_id: task_id.into(),
            kind: MessageKind::Notice,
            text: format!("Context was not compacted: {failure}"),
            attachments: Vec::new(),
            feedback: Vec::new(),
            file_edits: Vec::new(),
            tool: None,
            complete: true,
            created: now(),
        })?;
    }
    h.store.save_task(&task)
}

/// Returns the task when it is idle with input waiting to start its next turn.
pub(crate) fn waiting_turn(host: &crate::Host, task_id: &str) -> Result<Option<Task>> {
    let task = host.store.task(task_id)?;
    let waiting = matches!(task.state, RunState::Idle | RunState::Completed)
        && !host.store.has_uncertain_submission(task_id)?
        && host.store.outgoing(task_id)?.iter().any(|message| {
            message.state == DeliveryState::Queued && message.expected_turn.is_none()
        });
    Ok(waiting.then_some(task))
}

/// Begins a compaction when the next queued turn would start at or above the task's threshold.
///
/// This is the check between turns. The provider's own limit covers a turn that is running.
/// A usage reading is acted on once, and the one a compaction leaves behind never starts another.
pub(crate) fn begin_due_compaction(host: &Shared, task_id: &str) -> Result<bool> {
    let mut h = host.lock().unwrap();
    let Some(mut task) = waiting_turn(&h, task_id)? else {
        return Ok(false);
    };
    let Some((used, window, reported)) = task.context_usage.as_ref().and_then(|usage| {
        Some((
            usage.used,
            usage.window.filter(|window| *window > 0)?,
            usage.reported,
        ))
    }) else {
        return Ok(false);
    };
    if task.provider_thread.is_none() || h.compacted.get(task_id) == Some(&reported) {
        return Ok(false);
    }
    let rule = compact_rule(&h, &task)?;
    if !rule.enabled || used.saturating_mul(100) < window.saturating_mul(u64::from(rule.threshold))
    {
        return Ok(false);
    }
    begin_compaction(&mut h, &mut task)?;
    Ok(true)
}

fn ensure_owner(host: Shared, task_id: String) {
    let mut h = host.lock().unwrap();
    if let Some(sender) = h.sessions.get(&task_id) {
        // A disconnected owner still owns cleanup. Let it persist failure before any restart.
        let _ = sender.send(Control::Wake);
        return;
    }
    let (tx, rx) = mpsc::channel();
    h.sessions.insert(task_id.clone(), tx);
    drop(h);
    std::thread::spawn(move || {
        let provider = host
            .lock()
            .unwrap()
            .store
            .task(&task_id)
            .map(|task| task.provider);
        let result = provider.and_then(|provider| match provider {
            Provider::Codex => codex::run(host.clone(), &task_id, rx),
            Provider::Claude => claude::run(host.clone(), &task_id, rx),
            Provider::OpenCode => opencode::run(host.clone(), &task_id, rx),
        });
        if let Err(error) = result {
            let mut h = host.lock().unwrap();
            let compaction = h.compactions.remove(&task_id);
            if let Ok(mut task) = h.store.task(&task_id) {
                // A compaction carries no user input, so its failure is never an uncertain delivery.
                if let Some(previous) = compaction {
                    task.state = previous.state;
                    task.updated = previous.updated;
                    task.active_turn = None;
                    let _ = h.store.save_task(&task);
                }
                let unknown = task.active_turn.is_some()
                    || task.subagents.iter().any(|a| a.state.active())
                    || h.store.has_uncertain_submission(&task_id).unwrap_or(true);
                if unknown || task.state.active() || task.state == RunState::Idle {
                    task.state = if unknown {
                        RunState::DeliveryUnknown
                    } else {
                        RunState::Failed
                    };
                    let _ = h.store.settle_pending(&task_id, unknown);
                }
                for agent in &mut task.subagents {
                    if agent.state.active() {
                        agent.state = RunState::DeliveryUnknown;
                        agent.active_turn = None;
                    }
                }
                task.pending_completion = None;
                task.error = Some(format!(
                    "{error:#}. Queued messages are retained; no uncertain input was resent."
                ));
                let _ = h.save_task_with_notification(&task);
            }
        }
        let mut h = host.lock().unwrap();
        h.activities.remove(&task_id);
        h.sessions.remove(&task_id);
        h.requests.remove(&task_id);
        h.compactions.remove(&task_id);
    });
}

/// Claims the next eligible message while excluding concurrent checkout writers.
pub(crate) fn claim_next(host: &Shared, task_id: &str) -> Result<Option<OutgoingMessage>> {
    orchestrator::dispatch(host, task_id, Input::Command(Command::Claim))
}

/// Rules of a review session. Claude Code also runs it without its file-editing tools and with
/// the commands that publish denied. The approval rule of the task decides everything else.
const REVIEW_RULES: &str = "\
You are reviewing code in Kodem's Review workspace. This session is a local, read-only review.

Rules:
- Do not change anything. Do not edit, create, move or delete files. Do not run a command that writes to the working tree, the index, a branch, a tag or the Git configuration, such as commit, add, checkout, switch, restore, reset, stash, merge, rebase, cherry-pick, apply, fetch or pull.
- Do not publish anything. Do not push. Do not create, edit or submit a pull request, a pull-request comment, a review, an issue or any other remote content, with `gh`, `git`, an API call or any other tool. The review stays on this machine.
- Use read-only commands only, such as `git diff`, `git log`, `git show`, `git blame`, file reads and searches.
- When a fix is needed, describe it and do not apply it. The user sends the findings to another task that applies fixes.";

/// How a review reports, so that the findings can be handed to the task that fixes them.
const REVIEW_REPORT: &str = "\
Check correctness, regressions, error handling, security, performance and consistency with the conventions of this codebase. Do not report a style preference that the codebase does not follow.

Report the findings as a numbered list, most severe first. Give each finding: its severity (critical, major, minor or nit), its location as `path:line`, what is wrong, why it matters, and a concrete fix. End with a short verdict. If there are no findings, say so.";

/// Instructions of a conversation that has no project.
const CHAT_RULES: &str = "\
This is a general conversation in Kodem's Chat workspace. It has no project: the working directory is an empty scratch folder that belongs to this chat alone. Answer questions, explain, compare, brainstorm and write as a helpful assistant. Search the web when an answer depends on current or external information, and name the sources you used. Do not assume that a repository exists, and do not create files unless the user asks for one.";

/// How a Codex session keeps the plan that Kodem shows beside the composer.
const CODEX_PLAN_RULES: &str = "\
Kodem shows your plan to the user beside the prompt and updates it live while you work. Keep it accurate with the update_plan tool. In Plan mode update_plan is not available: propose the plan there instead.

Rules:
- When the work takes more than one step, write the plan before you start: short steps, in the order you will do them. Skip the plan only for a single quick step, such as a short answer.
- Mark a step in progress when you start it, and completed as soon as it is done, before you start the next step. Keep only one step in progress. Do not mark several steps completed at the end.
- When the work changes, change the plan: add, remove or rewrite steps, so that it always shows what you actually do.
- Before you end your turn, mark every finished step completed. Leave a step pending only when it is not done, and say why.";

/// How a Claude Code session keeps the plan. Its task tools are deferred and their own
/// description reserves them for complex work, so the rules name how to load them, override
/// that advice and spell out the order of calls.
const CLAUDE_PLAN_RULES: &str = "\
Kodem shows your plan to the user beside the prompt and updates it live while you work. You keep it with the TaskCreate and TaskUpdate tools. They are deferred tools: load them with ToolSearch, query `select:TaskCreate,TaskUpdate`, together with the first other tools you need. If you have TodoWrite instead, use it.

Rules:
- Every request with two or more steps gets a plan, also a short one, such as a search followed by an answer. This replaces any advice to keep a task list only for complex work. Skip the plan only when one step answers the request.
- Write the plan before any other work: one TaskCreate call for each step, short, in the order you will do them.
- Work through the steps in this order of calls: TaskUpdate the step to in_progress, do the step, TaskUpdate it to completed, then the next step. The user watches each step change, so never skip in_progress, and never mark two steps in one go. A step that ends in your reply, such as writing the answer, is in_progress while you write it and completed in your last tool call before the reply.
- When the work changes, change the plan: add, remove or rewrite steps, so that it always shows what you actually do.
- Before you end your turn, mark every finished step completed. Leave a step pending only when it is not done, and say why.";

/// How an OpenCode session keeps the plan, with its todo tool.
const OPENCODE_PLAN_RULES: &str = "\
Kodem shows your plan to the user beside the prompt and updates it live while you work. Keep it accurate with the todowrite tool.

Rules:
- Every request with two or more steps gets a plan, also a short one, such as a search followed by an answer. Skip the plan only when one step answers the request.
- Write the plan before any other work: short steps, in the order you will do them.
- Mark a step in_progress when you start it, and completed as soon as it is done, before you start the next step. Keep only one step in_progress. Do not mark several steps completed at the end.
- When the work changes, change the plan: add, remove or rewrite steps, so that it always shows what you actually do.
- Before you end your turn, mark every finished step completed. Leave a step pending only when it is not done, and say why.";

/// Returns the standing instructions of a task's provider session: the plan rules, and the
/// rules that its kind adds.
///
/// They are given when a provider process starts, so they also hold after a resume.
pub(crate) fn instructions(task: &Task) -> String {
    let plan = match task.provider {
        Provider::Claude => CLAUDE_PLAN_RULES,
        Provider::Codex => CODEX_PLAN_RULES,
        Provider::OpenCode => OPENCODE_PLAN_RULES,
    };
    let rules = match task.kind {
        TaskKind::Coding => None,
        TaskKind::Chat => Some(CHAT_RULES.into()),
        TaskKind::Review => {
            let scope = match (&task.review_scope, &task.baseline) {
                (Some(scope), Some(merge_base)) => format!(
                    "The working directory is a temporary detached worktree at the reviewed commit {head}. The changes under review are those of {target} compared with `{base}`: run `git diff {merge_base}...HEAD`. Review only those changes, and read the surrounding code as needed to judge them.",
                    head = scope.head,
                    target = scope.target.label(),
                    base = scope.base,
                ),
                _ => "Review the changes that the user names.".into(),
            };
            Some(format!("{REVIEW_RULES}\n\n{scope}\n\n{REVIEW_REPORT}"))
        }
    };
    match rules {
        Some(rules) => format!("{rules}\n\n{plan}"),
        None => plan.into(),
    }
}

/// Returns the options owned by this task rather than another provider's defaults.
pub(crate) fn preferences(task: &Task) -> AgentPreferences {
    AgentPreferences {
        provider: task.provider,
        account: task.account.clone(),
        provider_access: task.provider_access.clone(),
        model: task.model.clone(),
        effort: task.effort.clone(),
        access: task.access,
    }
}

/// Returns the model that runs a task: its own choice, or the provider's default for new tasks.
pub(crate) fn effective_model(host: &crate::Host, task: &Task) -> Option<String> {
    task.model.clone().or_else(|| {
        host.runtimes
            .get(&task.provider)
            .and_then(|runtime| runtime.default_model.clone())
    })
}

/// Returns the automatic compaction rule for a task's provider and the model that runs it.
pub(crate) fn compact_rule(host: &crate::Host, task: &Task) -> Result<CompactRule> {
    Ok(host
        .store
        .settings()?
        .auto_compact
        .rule(task.provider, effective_model(host, task).as_deref()))
}

/// Removes the "default" and "recommended" markers a provider adds to a model name.
///
/// The result is empty when the name was only such a marker and names no model.
pub(crate) fn model_name(name: &str) -> String {
    let marked = |text: &str| {
        let text = text.to_lowercase();
        text.contains("default") || text.contains("recommended")
    };
    let mut plain = String::new();
    for (index, part) in name.split('(').enumerate() {
        // After the first split every part starts inside a parenthesis.
        let (inside, outside) = match (index, part.split_once(')')) {
            (0, _) => ("", part),
            (_, Some((inside, outside))) => (inside, outside),
            (_, None) => (part, ""),
        };
        if !inside.is_empty() && !marked(inside) {
            plain.push_str(&format!("({inside})"));
        }
        plain.push_str(outside);
    }
    plain
        .split_whitespace()
        .filter(|word| !marked(word))
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches([' ', '-', '·', ':', ','])
        .to_owned()
}

/// Picks the model new tasks start with, which is always one of `models`.
///
/// The first model whose identity and name contain every wanted part wins, then the
/// provider's own default, then the first one listed.
pub(crate) fn default_model(
    models: &[Model],
    wanted: &[&str],
    own: Option<&str>,
) -> Option<String> {
    models
        .iter()
        .find(|model| {
            let text = format!("{} {}", model.id, model.name).to_lowercase();
            wanted.iter().all(|part| text.contains(part))
        })
        .or_else(|| models.iter().find(|model| Some(model.id.as_str()) == own))
        .or(models.first())
        .map(|model| model.id.clone())
}

/// Checks selected options against the provider's discovered catalog before persistence or delivery.
pub(crate) fn validate_preferences(host: &crate::Host, selection: &AgentPreferences) -> Result<()> {
    // An account identity is only meaningful for the provider it was registered with.
    accounts::resolve(host, selection.provider, selection.account.as_deref())?;
    let runtime = host.runtimes.get(&selection.provider);
    // An absent model runs as the provider's default, so reasoning is checked against that one.
    let model_id = selection
        .model
        .as_ref()
        .or_else(|| runtime.and_then(|runtime| runtime.default_model.as_ref()));
    if let Some(model_id) = model_id {
        let model = runtime.and_then(|runtime| runtime.models.iter().find(|model| &model.id == model_id))
            .context("The selected model is unavailable. Refresh this provider and choose an available model.")?;
        if let Some(effort) = &selection.effort {
            ensure!(
                model.efforts.contains(effort),
                "The selected model does not support that reasoning level"
            );
        }
    } else {
        ensure!(
            selection.effort.is_none(),
            "Choose a model before selecting reasoning"
        );
    }
    if let Some(access) = &selection.provider_access {
        ensure!(
            runtime
                .is_some_and(|runtime| runtime.access_modes.iter().any(|mode| &mode.id == access)),
            "The selected provider access mode is unavailable. Refresh its capabilities."
        );
    }
    Ok(())
}

/// Allows restoring previously accepted options while their provider catalog is reconnecting.
pub(crate) fn validate_saved_selection(
    host: &crate::Host,
    selection: &AgentPreferences,
) -> Result<()> {
    if host
        .runtimes
        .get(&selection.provider)
        .is_none_or(|runtime| runtime.models.is_empty())
    {
        let settings = host.store.settings()?;
        if settings.provider_preferences.get(&selection.provider) == Some(selection)
            || &settings.agent_defaults == selection
        {
            return Ok(());
        }
    }
    validate_preferences(host, selection)
}

/// Rejects unsupported context without consuming the user's draft or attachments.
pub(crate) fn validate_attachments(
    host: &crate::Host,
    task: &Task,
    attachments: &[Attachment],
) -> Result<()> {
    if attachments.is_empty() {
        return Ok(());
    }
    let runtime = host
        .runtimes
        .get(&task.provider)
        .context("Refresh the selected provider before attaching files")?;
    let supports_images = task.model.as_ref().map_or(runtime.images, |id| {
        runtime
            .models
            .iter()
            .find(|model| &model.id == id)
            .is_some_and(|model| model.images)
    });
    for attachment in attachments {
        if attachment.kind == AttachmentKind::Image {
            ensure!(
                supports_images,
                "{} with the selected model does not support images. Remove the image or choose another model.",
                task.provider.label()
            );
        } else {
            ensure!(
                runtime.files,
                "{} does not support file context in this session",
                task.provider.label()
            );
        }
    }
    Ok(())
}

/// Refreshes one provider's catalog through the given account without touching other providers.
pub(super) fn discover(
    host: Shared,
    provider: Provider,
    account: Option<String>,
    cwd: Option<PathBuf>,
) -> Result<()> {
    // The registry lookup must release the host before a provider process is started.
    let account = accounts::resolve(&host.lock().unwrap(), provider, account.as_deref());
    let result = account.and_then(|account| match provider {
        Provider::Codex => codex::discover(host.clone(), &account, cwd),
        Provider::Claude => claude::discover(host.clone(), &account, cwd),
        Provider::OpenCode => opencode::discover(host.clone(), &account, cwd),
    });
    if let Err(error) = &result {
        let mut h = host.lock().unwrap();
        let runtime = h.runtimes.entry(provider).or_default();
        runtime.error = Some(format!("{error:#}"));
    }
    result
}

/// Returns the capabilities known before a provider's CLI has been asked for its catalog.
pub(super) fn initial_runtime(provider: Provider) -> RuntimeInfo {
    match provider {
        Provider::Claude => claude::initial_runtime(),
        Provider::Codex => RuntimeInfo::default(),
        Provider::OpenCode => opencode::initial_runtime(),
    }
}

/// Reconciles only against a provider that can prove the exact submission identity.
pub(super) fn reconcile(host: &Shared, task_id: &str) -> Result<()> {
    let provider = host.lock().unwrap().store.task(task_id)?.provider;
    match provider {
        Provider::Codex => codex::reconcile(host, task_id),
        Provider::Claude => claude::reconcile(host, task_id),
        Provider::OpenCode => opencode::reconcile(host, task_id),
    }
}

/// Lists the conversations that an account's CLI stored on this computer.
pub(crate) fn stored_threads(
    provider: Provider,
    account: &Account,
) -> Result<Vec<crate::imports::Stored>> {
    match provider {
        Provider::Codex => codex::stored_threads(account),
        Provider::Claude => claude::stored_threads(account),
        Provider::OpenCode => opencode::stored_threads(account),
    }
}

/// Reads one stored conversation of an account for an import.
pub(crate) fn stored_thread(
    provider: Provider,
    account: &Account,
    thread: &str,
) -> Result<crate::imports::Stored> {
    match provider {
        Provider::Codex => codex::stored_thread(account, thread),
        Provider::Claude => claude::stored_thread(account, thread),
        Provider::OpenCode => opencode::stored_thread(account, thread),
    }
}

/// Writes the history of the stored conversation that `task` continues into its transcript.
pub(crate) fn import_history(host: &Shared, task: &Task, account: &Account) -> Result<()> {
    match task.provider {
        Provider::Codex => codex::import_history(host, task, account),
        Provider::Claude => claude::import_history(host, task, account),
        Provider::OpenCode => opencode::import_history(host, task, account),
    }
}

/// Resolves the CLI that signs an account in or out.
pub(crate) fn executable(provider: Provider) -> Result<PathBuf> {
    match provider {
        Provider::Codex => kodem_core::providers::codex::executable(),
        Provider::Claude => Ok(claude::executable()),
        Provider::OpenCode => kodem_core::providers::opencode::executable(),
    }
}

/// Generates a short task title under the task's account for providers without their own.
pub(crate) fn generate_title(account: &Account, input: &str) -> Result<String> {
    match account.provider {
        Provider::Claude => claude::generate_title(account, input),
        Provider::Codex | Provider::OpenCode => {
            anyhow::bail!("This provider titles its conversations itself")
        }
    }
}

/// Reads an account's sign-in state, and its quotas when requested, without a model turn.
pub(crate) fn account_status(
    host: &Shared,
    account: &Account,
    usage: bool,
) -> Result<AccountStatus> {
    match account.provider {
        Provider::Codex => codex::account_status(host, account, usage),
        Provider::Claude => claude::account_status(host, account, usage),
        Provider::OpenCode => opencode::account_status(host, account, usage),
    }
}

/// Routes quota refresh and credit actions to the account that owns them.
pub(super) fn account_usage(
    host: &Shared,
    provider: Provider,
    account: Option<String>,
    key: Option<&str>,
) -> Result<Response> {
    let account = accounts::resolve(&host.lock().unwrap(), provider, account.as_deref())?;
    match provider {
        Provider::Codex => codex::account_usage(host, &account, key),
        Provider::Claude => claude::account_usage(host, &account, key),
        Provider::OpenCode => opencode::account_usage(host, &account, key),
    }
}

/// Resolves the task's immutable integration and account before loading commands or resources.
fn scoped_account(
    host: &Shared,
    provider: Provider,
    account: Option<String>,
    task_id: &Option<String>,
) -> Result<Account> {
    let h = host.lock().unwrap();
    if let Some(task_id) = task_id {
        accounts::for_task(&h, &h.store.task(task_id)?)
    } else {
        accounts::resolve(&h, provider, account.as_deref())
    }
}

/// Loads supported composer operations for the selected integration and checkout.
pub(super) fn composer_catalog(
    host: &Shared,
    provider: Provider,
    account: Option<String>,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<ComposerCatalog> {
    let account = scoped_account(host, provider, account, &task_id)?;
    let (undoable, forkable) = match &task_id {
        Some(task_id) => {
            let h = host.lock().unwrap();
            (
                undoable_run(&h, task_id)?.is_some(),
                forkable_run(&h, task_id)?.is_some(),
            )
        }
        None => (false, false),
    };
    let mut catalog = match account.provider {
        Provider::Codex => codex::composer_catalog(host, &account, task_id, project_id)?,
        Provider::Claude => claude::composer_catalog(host, &account, task_id, project_id)?,
        Provider::OpenCode => opencode::composer_catalog(host, &account, task_id, project_id)?,
    };
    if !catalog.commands.contains(&ComposerCommand::Provider) {
        catalog.commands.insert(0, ComposerCommand::Provider);
    }
    // Only a turn whose files were saved before it began can be undone.
    if undoable {
        catalog.commands.push(ComposerCommand::Undo);
    }
    if forkable {
        catalog.commands.push(ComposerCommand::Fork);
    }
    Ok(catalog)
}

/// Most earlier conversation that a handoff gives to a provider conversation, in bytes.
const HANDOFF_LIMIT: usize = 200 * 1024;

// The last turn of a coding task that is not undone, with the turn whose checkpoint holds the
// files from before it: the last turn before it that is not undone either, or zero for the
// state before the first recorded turn. `None` when no such checkpoint was saved.
fn undoable_run(host: &crate::Host, task_id: &str) -> Result<Option<(Run, u32)>> {
    let task = host.store.task(task_id)?;
    if task.kind != TaskKind::Coding {
        return Ok(None);
    }
    let runs = host.store.runs(task_id)?;
    let Some(index) = runs.iter().rposition(|run| run.undone.is_none()) else {
        return Ok(None);
    };
    let before = runs[..index].iter().rfind(|run| run.undone.is_none());
    if before.is_some_and(|run| run.checkpoint.is_none()) {
        return Ok(None);
    }
    Ok(Some((
        runs[index].clone(),
        before.map_or(0, |run| run.ordinal),
    )))
}

/// Returns the last turn of a coding task that is not undone, when its files were saved: the
/// state that a fork of the task starts from.
pub(crate) fn forkable_run(host: &crate::Host, task_id: &str) -> Result<Option<Run>> {
    let task = host.store.task(task_id)?;
    if task.kind != TaskKind::Coding {
        return Ok(None);
    }
    Ok(host
        .store
        .runs(task_id)?
        .into_iter()
        .rfind(|run| run.undone.is_none())
        .filter(|run| run.checkpoint.is_some()))
}

/// Restores the files of a coding task to the checkpoint from before its last turn.
///
/// The turn stays in the transcript, marked as undone. The provider conversation still holds
/// it, so the task leaves that conversation: the next message starts a new one and carries the
/// conversation that remains as context. The state that the restore replaced is saved as a
/// checkpoint of its own.
pub(super) fn undo_turn(host: &Shared, task_id: &str) -> Result<()> {
    let (task, run, before) = {
        let h = host.lock().unwrap();
        ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
        let task = h.store.task(task_id)?;
        ensure!(
            task.kind == TaskKind::Coding,
            "Only a coding task saves its files after each turn"
        );
        ensure!(
            !task.state.active() && !h.compactions.contains_key(task_id),
            "Undo after this turn finishes"
        );
        ensure!(
            task.state != RunState::DeliveryUnknown
                && !h.store.has_uncertain_submission(task_id)?,
            "Reconcile uncertain delivery before undoing a turn"
        );
        ensure!(
            !h.store
                .outgoing(task_id)?
                .iter()
                .any(|message| message.state == DeliveryState::Queued),
            "Cancel queued messages before undoing a turn"
        );
        ensure!(
            !h.store.tasks()?.iter().any(|other| other.id != task.id
                && other.cwd == task.cwd
                && other.state.active()
                && !other.shares_checkout()),
            "Another task is writing to this checkout. Undo after it finishes."
        );
        let (run, before) = undoable_run(&h, task_id)?
            .context("There is no turn to undo: no files were saved before the last turn")?;
        (task, run, before)
    };
    close_session(host, task_id)?;
    let backup = git::checkpoint::restore(&task.cwd, task_id, &before.to_string())
        .context("The files could not be restored to the checkpoint before this turn")?;
    let h = host.lock().unwrap();
    let mut task = h.store.task(task_id)?;
    let mut run = run;
    run.undone = Some(now());
    task.provider_thread = None;
    task.context_usage = None;
    task.handoff = Some(Handoff::Restored { turn: before });
    let name = backup.rsplit('/').next().unwrap_or_default();
    let notice = Message {
        seq: 0,
        transfer: None,
        id: format!("undo:{}", run.id),
        task_id: task_id.into(),
        kind: MessageKind::Notice,
        text: format!(
            "Turn {} undone · files restored · earlier state kept as {name}",
            run.ordinal
        ),
        attachments: Vec::new(),
        feedback: Vec::new(),
        file_edits: Vec::new(),
        tool: None,
        complete: true,
        created: now(),
    };
    h.store.commit(&Commit {
        task: &task,
        ledger: &[],
        runs: &[run],
        messages: &[notice],
        release_completion: false,
    })
}

/// Ends the provider session of a task that does no work, and waits until it is gone.
///
/// The session owner holds the provider conversation. It must end before the task leaves that
/// conversation, or the next message would still reach it. Input that was accepted meanwhile
/// gets a new owner.
pub(crate) fn close_session(host: &Shared, task_id: &str) -> Result<()> {
    {
        let h = host.lock().unwrap();
        let task = h.store.task(task_id)?;
        ensure!(
            !task.state.active() && !h.compactions.contains_key(task_id),
            "Wait until this turn finishes"
        );
        let Some(session) = h.sessions.get(task_id) else {
            return Ok(());
        };
        let _ = session.send(Control::Close);
    }
    let until = Instant::now() + Duration::from_secs(10);
    while host.lock().unwrap().sessions.contains_key(task_id) {
        ensure!(
            Instant::now() < until,
            "The provider session did not stop. Try again."
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let queued = host
        .lock()
        .unwrap()
        .store
        .outgoing(task_id)?
        .iter()
        .any(|message| message.state == DeliveryState::Queued);
    if queued {
        ensure_owner(host.clone(), task_id.into());
    }
    Ok(())
}

/// Returns the agent that runs a task now: its provider and the model that its turns use.
pub(crate) fn agent_of(host: &crate::Host, task: &Task) -> AgentRef {
    AgentRef {
        provider: task.provider,
        model: effective_model(host, task),
        task_id: None,
    }
}

/// Returns the name to show for an agent: its model as the provider lists it, or the provider.
pub(crate) fn agent_name(host: &crate::Host, agent: &AgentRef) -> String {
    let Some(id) = &agent.model else {
        return agent.provider.label().to_owned();
    };
    host.runtimes
        .get(&agent.provider)
        .and_then(|runtime| runtime.models.iter().find(|model| &model.id == id))
        .map_or_else(|| id.clone(), |model| model.name.clone())
}

/// Builds what a provider conversation that has not seen a task needs to continue it: the
/// reason, and the messages of the user and the replies of the turns that are not undone.
///
/// The newest messages are kept when the conversation exceeds the limit. Tool calls, their
/// output and reasoning are left out: the files hold their result. A fork reads the
/// conversation of the task it came from, up to the turn it was forked at.
pub(crate) fn handoff_context(
    host: &crate::Host,
    task: &Task,
    message: &OutgoingMessage,
) -> Result<Option<String>> {
    let Some(handoff) = &task.handoff else {
        return Ok(None);
    };
    let (source, last_turn) = match handoff {
        Handoff::Forked { from, turn } => {
            (from.task_id.as_deref().unwrap_or(&task.id), Some(*turn))
        }
        Handoff::Restored { .. } | Handoff::Switched { .. } => (task.id.as_str(), None),
    };
    let runs = host.store.runs(source)?;
    let mut parts: Vec<String> = Vec::new();
    let mut size = 0usize;
    let mut before = None;
    'pages: loop {
        let (messages, older) = host.store.messages(source, before)?;
        for row in messages.iter().rev() {
            let speaker = match row.kind {
                MessageKind::User => "User",
                MessageKind::Assistant => "Assistant",
                _ => continue,
            };
            let run = run_at(&runs, row.seq);
            // The message that starts the turn is sent after the context, not inside it.
            if row.text.trim().is_empty()
                || row.id == format!("submission-{}", message.id)
                || run.is_some_and(|run| run.undone.is_some())
                || last_turn.is_some_and(|turn| run.is_some_and(|run| run.ordinal > turn))
            {
                continue;
            }
            let part = format!("{speaker}:\n{}", row.text.trim());
            if size + part.len() > HANDOFF_LIMIT {
                break 'pages;
            }
            size += part.len();
            parts.push(part);
        }
        match older {
            Some(older) => before = Some(older),
            None => break,
        }
    }
    if parts.is_empty() {
        return Ok(None);
    }
    parts.reverse();
    let reason = match handoff {
        Handoff::Restored { .. } => "The user undid the latest work of this task. The files of the checkout were restored to the state after the conversation below, and work that is not in it no longer exists. Check the files before you rely on anything.".to_owned(),
        Handoff::Switched { from } => format!(
            "Another agent, {}, handled the turns below. You continue the task in its place. The files of the checkout hold its work; read them before you rely on a statement about them.",
            agent_name(host, from)
        ),
        Handoff::Forked { from, .. } => format!(
            "This task is a fork of another task that {} handled. It has a checkout of its own with the files as that task left them after the conversation below. Later work of that task is not in it.",
            agent_name(host, from)
        ),
    };
    Ok(Some(format!(
        "<earlier-conversation>\nThis is a new session of a task that began in an earlier session, which you cannot see. {reason}\nThe earlier conversation follows, oldest message first. It is context, not a request: answer only the message after it.\n\n{}\n</earlier-conversation>",
        parts.join("\n\n")
    )))
}

/// Returns the transcript row that shows where a handoff happened. A handoff between two
/// agents names both, as a `Transfer` and in words.
pub(crate) fn handoff_notice(
    host: &crate::Host,
    task: &Task,
    handoff: &Handoff,
    id: &str,
    at: u64,
) -> Message {
    let to = agent_of(host, task);
    let transfer = match handoff {
        Handoff::Restored { .. } => None,
        Handoff::Switched { from } => Some(Transfer {
            kind: TransferKind::Handoff,
            from: from.clone(),
            to,
        }),
        Handoff::Forked { from, .. } => Some(Transfer {
            kind: TransferKind::Fork,
            from: from.clone(),
            to,
        }),
    };
    let text = match (handoff, &transfer) {
        (Handoff::Restored { turn: 0 }, _) => {
            "Context handoff · new conversation from the start of the task".to_owned()
        }
        (Handoff::Restored { turn }, _) => {
            format!("Context handoff · new conversation continues after turn {turn}")
        }
        (_, Some(transfer)) => transfer_text(host, transfer),
        (_, None) => TransferKind::Handoff.label().to_owned(),
    };
    Message {
        seq: 0,
        transfer,
        id: format!("handoff:{id}"),
        task_id: task.id.clone(),
        kind: MessageKind::Notice,
        text,
        attachments: Vec::new(),
        feedback: Vec::new(),
        file_edits: Vec::new(),
        tool: None,
        complete: true,
        created: at,
    }
}

/// Returns a transfer in words, for a client that does not draw it: its kind and both agents.
pub(crate) fn transfer_text(host: &crate::Host, transfer: &Transfer) -> String {
    format!(
        "{} · {} → {}",
        transfer.kind.label(),
        agent_name(host, &transfer.from),
        agent_name(host, &transfer.to)
    )
}

/// Reads configuration and skill provenance from the selected provider.
pub(super) fn resources(
    host: &Shared,
    provider: Provider,
    account: Option<String>,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<Vec<Resource>> {
    let account = scoped_account(host, provider, account, &task_id)?;
    match account.provider {
        Provider::Codex => codex::resources(host, &account, task_id, project_id),
        Provider::Claude => claude::resources(host, &account, task_id, project_id),
        Provider::OpenCode => opencode::resources(host, &account, task_id, project_id),
    }
}

/// Captures the final checkout before publishing completion or making another queued turn eligible.
pub(crate) fn complete_turn(
    host: &Shared,
    task_id: &str,
    state: RunState,
    error: Option<String>,
) -> Result<()> {
    let command = orchestrator::observe(host, task_id, TurnOutcome { state, error })?;
    orchestrator::dispatch(host, task_id, Input::Command(command))?;
    Ok(())
}
