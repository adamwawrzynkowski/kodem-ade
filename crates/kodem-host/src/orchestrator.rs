//! Decides the lifecycle of a task's main turn without I/O and commits each step in one transaction.
//!
//! A provider driver keeps its own session loop and protocol. It reports what it observed as a
//! `Signal`, at the moment its protocol proves it. `decide` turns a signal or a host `Command`
//! into events from the stored task and from facts gathered under the host lock. `apply` is the
//! only code that changes a task for those events, and `Store::commit` stores the task and the
//! ledger changes of one step together. An effect that needs no provider transport runs after
//! the commit; a message to send returns to the session loop that owns the transport.
use crate::{Host, Shared, provider};
use anyhow::{Result, bail};
use kodem_core::{
    storage::{Commit, LedgerChange},
    *,
};
use std::collections::VecDeque;

const STEER_EXPIRED: &str = "The selected turn finished before this steering message could be delivered. Restore it to send again.";
const CHECKOUT_BUSY: &str =
    "Queue paused because another task is using this checkout. Resume after it finishes.";

/// What a provider driver observed about the main turn of a task.
pub(crate) enum Signal {
    /// The provider accepted a claimed message as part of the given turn.
    Delivered {
        submission: String,
        turn: String,
        steers: bool,
    },
    /// The provider did not accept a claimed message. `unknown` means it may still have arrived.
    DeliveryFailed {
        submission: String,
        steers: bool,
        unknown: bool,
        error: String,
    },
    /// The provider reported that the main turn began.
    TurnStarted {
        turn: Option<String>,
        /// Branch of the checkout, when the task follows it.
        branch: Option<String>,
    },
    /// The provider reported the outcome of the current main turn.
    TurnEnded { outcome: TurnOutcome },
    /// A subagent changed state, which can let a held outcome complete.
    ChildrenChanged,
}

/// A step the host asks for.
pub(crate) enum Command {
    /// Claims the next queued message that may be sent now.
    Claim,
    /// Publishes a turn outcome with the checkout observations made after the turn.
    Finalize {
        outcome: TurnOutcome,
        review: Option<TaskReview>,
        branch: Option<String>,
        /// Name of the checkpoint that holds the files as the turn left them.
        checkpoint: Option<String>,
    },
}

/// One input of the decision.
pub(crate) enum Input {
    Signal(Signal),
    Command(Command),
}

/// State outside the task that a decision depends on, read under the host lock.
///
/// The queue fields are gathered only for `Command::Claim`.
#[derive(Default)]
pub(crate) struct Facts {
    /// Queued, in-flight and recoverable failed input in acceptance order.
    pub(crate) outgoing: Vec<OutgoingMessage>,
    /// A delivery of this task is not confirmed yet.
    pub(crate) uncertain: bool,
    /// Another active task writes to the checkout of this task.
    pub(crate) checkout_busy: bool,
    /// Why the next message cannot be sent with the options of the task.
    pub(crate) refusal: Option<String>,
    /// The host runs a compaction of this task, whose provider turn is not a run.
    pub(crate) compacting: bool,
    /// Earlier conversation for a provider conversation that has not seen it, when the task
    /// waits for such a handoff and the next message starts a turn.
    pub(crate) context: Option<String>,
    /// Identity for a run that this step creates.
    pub(crate) fresh: String,
    pub(crate) now: u64,
}

/// A fact about the lifecycle of a task that one step stores.
pub(crate) enum Event {
    /// A queued steering message outlived the turn it was meant for.
    SteerExpired { submission: String },
    /// The queue stopped because another task writes to the same checkout.
    QueuePaused,
    /// A queued message was claimed to start the next turn.
    TurnClaimed { message: OutgoingMessage, at: u64 },
    /// A queued message was claimed to join the running turn.
    SteerClaimed { message: OutgoingMessage },
    /// The provider accepted a message. `begins` marks the proof that its turn runs.
    Delivered {
        submission: String,
        turn: String,
        begins: bool,
    },
    /// A message was not accepted. `state` is the state the task falls to, if any.
    DeliveryFailed {
        submission: String,
        unknown: bool,
        error: String,
        state: Option<RunState>,
    },
    /// The main turn began.
    TurnStarted {
        turn: Option<String>,
        branch: Option<String>,
        at: u64,
    },
    /// The main turn ended; its outcome waits until the subagents of the task stopped.
    OutcomeHeld { outcome: TurnOutcome },
    /// No subagent works any more, so the held outcome can be published.
    OutcomeReleased,
    /// The turn outcome was published with the checkout observations.
    Finalized {
        outcome: TurnOutcome,
        review: Option<TaskReview>,
        branch: Option<String>,
        checkpoint: Option<String>,
        at: u64,
    },
}

/// Work that follows a committed step.
pub(crate) enum Effect {
    /// Send the claimed message through the provider session of the task.
    Send(OutgoingMessage),
    /// Observe the checkout, then finalize the turn with this outcome.
    Capture(TurnOutcome),
}

/// Result of one decision.
#[derive(Default)]
pub(crate) struct Decision {
    pub(crate) events: Vec<Event>,
    pub(crate) effects: Vec<Effect>,
    /// Error reported to the caller after the events were committed.
    pub(crate) refusal: Option<String>,
}

/// Returns the queued message that may be sent in the current state of the task.
///
/// A running turn takes only messages bound to it. A task at rest takes the oldest unbound one.
pub(crate) fn next_message<'a>(
    task: &Task,
    outgoing: &'a [OutgoingMessage],
) -> Option<&'a OutgoingMessage> {
    if task.state.active() {
        outgoing.iter().find(|message| {
            message.state == DeliveryState::Queued
                && message.expected_turn.is_some()
                && message.expected_turn == task.active_turn
        })
    } else if matches!(task.state, RunState::Idle | RunState::Completed) {
        outgoing.iter().find(|message| {
            message.state == DeliveryState::Queued && message.expected_turn.is_none()
        })
    } else {
        None
    }
}

/// Decides the events and effects of one input. It reads nothing but its arguments.
pub(crate) fn decide(task: &Task, facts: &Facts, input: &Input) -> Decision {
    match input {
        Input::Command(Command::Claim) => claim(task, facts),
        Input::Command(Command::Finalize {
            outcome,
            review,
            branch,
            checkpoint,
        }) => Decision {
            events: vec![Event::Finalized {
                outcome: outcome.clone(),
                review: review.clone(),
                branch: branch.clone(),
                checkpoint: checkpoint.clone(),
                at: facts.now,
            }],
            ..Decision::default()
        },
        Input::Signal(Signal::Delivered {
            submission,
            turn,
            steers,
        }) => Decision {
            events: vec![Event::Delivered {
                submission: submission.clone(),
                turn: turn.clone(),
                // The provider may have reported the turn, or even its end, before this proof.
                begins: !steers && task.state == RunState::Starting,
            }],
            ..Decision::default()
        },
        Input::Signal(Signal::DeliveryFailed {
            submission,
            steers,
            unknown,
            error,
        }) => Decision {
            events: vec![Event::DeliveryFailed {
                submission: submission.clone(),
                unknown: *unknown,
                error: error.clone(),
                // A refused steering message leaves its running turn as it is.
                state: (*unknown || !steers).then_some(if *unknown {
                    RunState::DeliveryUnknown
                } else {
                    RunState::Failed
                }),
            }],
            ..Decision::default()
        },
        Input::Signal(Signal::TurnStarted { turn, branch }) => Decision {
            events: vec![Event::TurnStarted {
                turn: turn.clone(),
                branch: branch.clone(),
                at: facts.now,
            }],
            ..Decision::default()
        },
        Input::Signal(Signal::TurnEnded { outcome }) => {
            let mut decision = Decision {
                events: vec![Event::OutcomeHeld {
                    outcome: outcome.clone(),
                }],
                ..Decision::default()
            };
            release(task, outcome, &mut decision);
            decision
        }
        Input::Signal(Signal::ChildrenChanged) => {
            let mut decision = Decision::default();
            if let Some(outcome) = &task.pending_completion {
                release(task, outcome, &mut decision);
            }
            decision
        }
    }
}

// Only the main turn completes a task, and only after every subagent stopped working.
fn release(task: &Task, outcome: &TurnOutcome, decision: &mut Decision) {
    if task.subagents.iter().any(|agent| agent.state.active()) {
        return;
    }
    decision.events.push(Event::OutcomeReleased);
    decision.effects.push(Effect::Capture(outcome.clone()));
}

fn claim(task: &Task, facts: &Facts) -> Decision {
    let mut decision = Decision::default();
    // Nothing is sent, and no steering message expires, while a delivery is unconfirmed.
    if facts.uncertain || task.state == RunState::DeliveryUnknown {
        return decision;
    }
    for message in &facts.outgoing {
        if message.state == DeliveryState::Queued
            && message.expected_turn.is_some()
            && message.expected_turn != task.active_turn
        {
            decision.events.push(Event::SteerExpired {
                submission: message.id.clone(),
            });
        }
    }
    let Some(message) = next_message(task, &facts.outgoing) else {
        return decision;
    };
    if facts.refusal.is_some() {
        decision.refusal = facts.refusal.clone();
        return decision;
    }
    if message.expected_turn.is_some() {
        decision.events.push(Event::SteerClaimed {
            message: message.clone(),
        });
    } else if facts.checkout_busy {
        decision.events.push(Event::QueuePaused);
        return decision;
    } else {
        decision.events.push(Event::TurnClaimed {
            message: message.clone(),
            at: facts.now,
        });
    }
    // The provider receives the earlier conversation with the message. The transcript keeps
    // the message as the user wrote it.
    let mut sent = message.clone();
    if let (Some(context), None) = (&facts.context, &message.expected_turn) {
        sent.text = format!("{context}\n\n{}", message.text);
    }
    decision.effects.push(Effect::Send(sent));
    decision
}

/// Changes a task for one event. It reads nothing but its arguments.
pub(crate) fn apply(task: &mut Task, event: &Event) {
    match event {
        Event::SteerExpired { .. } | Event::SteerClaimed { .. } => {}
        Event::QueuePaused => {
            task.state = RunState::Interrupted;
            task.error = Some(CHECKOUT_BUSY.into());
        }
        Event::TurnClaimed { at, .. } => {
            task.state = RunState::Starting;
            task.branch_merged = false;
            task.active_turn = None;
            task.pending_completion = None;
            task.error = None;
            // The claimed message carries the context that a waiting handoff asked for.
            task.handoff = None;
            task.updated = *at;
            task.last_started = Some(*at);
            task.working_since = Some(*at);
        }
        Event::Delivered { turn, begins, .. } => {
            if *begins {
                task.state = RunState::Running;
                task.active_turn = Some(turn.clone());
            }
        }
        Event::DeliveryFailed { error, state, .. } => {
            if let Some(state) = state {
                task.state = state.clone();
                task.error = Some(error.clone());
            }
        }
        Event::TurnStarted { turn, branch, at } => {
            if let Some(branch) = branch {
                task.branch = branch.clone();
            }
            // A turn that follows another of the same stretch of work keeps its start.
            if !task.state.active() || task.working_since.is_none() {
                task.working_since = Some(*at);
            }
            task.active_turn = turn.clone();
            task.state = RunState::Running;
            // An error belongs to the turn before this one, and so does a finished plan.
            task.error = None;
            if task.plan.iter().all(|step| step.status == PlanStatus::Done) {
                task.plan.clear();
            }
            task.updated = *at;
        }
        Event::OutcomeHeld { outcome } => {
            task.active_turn = None;
            task.pending_completion = Some(outcome.clone());
            if task.subagents.iter().any(|agent| agent.state.active()) {
                task.state = RunState::Running;
            }
        }
        Event::OutcomeReleased => {
            task.pending_completion = None;
        }
        Event::Finalized {
            outcome,
            review,
            branch,
            at,
            ..
        } => {
            if let Some(review) = review {
                task.review = Some(review.clone());
            }
            if let Some(branch) = branch {
                task.branch = branch.clone();
            }
            task.pending_completion = None;
            task.active_turn = None;
            task.state = outcome.state.clone();
            task.error = outcome.error.clone();
            task.updated = *at;
        }
    }
    if !task.state.active() {
        task.working_since = None;
    }
}

/// Changes the newest run of a task for one event, after `apply` changed the task.
///
/// Returns an open run that the event closed in favour of a new one. It reads nothing but its
/// arguments.
pub(crate) fn apply_run(
    run: &mut Option<Run>,
    task: &Task,
    facts: &Facts,
    event: &Event,
) -> Option<Run> {
    let open = run.as_mut().filter(|run| run.ended.is_none());
    match (event, open) {
        (Event::TurnClaimed { message, at }, _) => {
            return begin_run(
                run,
                task,
                facts,
                RunOrigin::User,
                vec![message.id.clone()],
                RunState::Starting,
                *at,
            );
        }
        // A turn that nobody claimed is the provider continuing by itself.
        (Event::TurnStarted { turn, at, .. }, None) => {
            if !facts.compacting {
                let closed = begin_run(
                    run,
                    task,
                    facts,
                    RunOrigin::Continuation,
                    Vec::new(),
                    RunState::Running,
                    *at,
                );
                if let Some(run) = run {
                    run.turn = turn.clone();
                }
                return closed;
            }
        }
        (Event::TurnStarted { turn, .. }, Some(open)) => {
            if turn.is_some() {
                open.turn = turn.clone();
            }
            open.state = RunState::Running;
        }
        (Event::SteerClaimed { message }, Some(open)) => open.submissions.push(message.id.clone()),
        (Event::Delivered { turn, begins, .. }, Some(open)) => {
            if open.turn.is_none() {
                open.turn = Some(turn.clone());
            }
            if *begins {
                open.state = RunState::Running;
            }
        }
        (
            Event::DeliveryFailed {
                state: Some(state), ..
            },
            Some(open),
        ) => {
            open.state = state.clone();
            // An uncertain delivery leaves the run open: reconciliation can still prove it.
            if *state == RunState::Failed {
                open.ended = Some(facts.now);
            }
        }
        (
            Event::Finalized {
                outcome,
                checkpoint,
                at,
                ..
            },
            Some(open),
        ) => {
            if checkpoint.is_some() {
                open.checkpoint = checkpoint.clone();
            }
            open.state = outcome.state.clone();
            open.provider_thread = task.provider_thread.clone();
            open.ended = Some(*at);
        }
        _ => {}
    }
    None
}

// Starts the next run of a task. An earlier run that is still open ends here, without an
// outcome of its own, and is returned to be stored.
fn begin_run(
    run: &mut Option<Run>,
    task: &Task,
    facts: &Facts,
    origin: RunOrigin,
    submissions: Vec<String>,
    state: RunState,
    at: u64,
) -> Option<Run> {
    let previous = run.take();
    let ordinal = previous.as_ref().map_or(1, |run| run.ordinal + 1);
    *run = Some(Run {
        id: facts.fresh.clone(),
        task_id: task.id.clone(),
        ordinal,
        origin,
        submissions,
        state,
        provider: task.provider,
        provider_thread: task.provider_thread.clone(),
        turn: None,
        first_seq: 0,
        started: at,
        ended: None,
        checkpoint: None,
        undone: None,
    });
    previous.filter(|run| run.ended.is_none()).map(|mut run| {
        if run.state.active() {
            run.state = RunState::Interrupted;
        }
        run.ended = Some(at);
        run
    })
}

// The ledger change that an event stores with the task, if it has one.
fn ledger(event: &Event) -> Option<LedgerChange> {
    match event {
        Event::SteerExpired { submission } => Some(LedgerChange::Fail {
            id: submission.clone(),
            unknown: false,
            error: STEER_EXPIRED.into(),
        }),
        Event::TurnClaimed { message, .. } | Event::SteerClaimed { message } => {
            Some(LedgerChange::Claim(message.clone()))
        }
        Event::Delivered {
            submission, turn, ..
        } => Some(LedgerChange::Confirm {
            id: submission.clone(),
            turn: turn.clone(),
        }),
        Event::DeliveryFailed {
            submission,
            unknown,
            error,
            ..
        } => Some(LedgerChange::Fail {
            id: submission.clone(),
            unknown: *unknown,
            error: error.clone(),
        }),
        Event::QueuePaused
        | Event::TurnStarted { .. }
        | Event::OutcomeHeld { .. }
        | Event::OutcomeReleased
        | Event::Finalized { .. } => None,
    }
}

// Activities and pending requests live in host memory; they follow the committed events.
fn forget(host: &mut Host, task: &Task, event: &Event) {
    match event {
        Event::TurnClaimed { .. } => {
            host.activities.remove(&task.id);
        }
        Event::OutcomeHeld { .. } => {
            host.activities.remove(&task.id);
            if let Some(requests) = host.requests.get_mut(&task.id) {
                // Only requests of the main conversation end with its turn. A request without a
                // provider conversation is kept: its driver withdraws it by itself.
                requests.retain(|request| {
                    request.params["threadId"]
                        .as_str()
                        .is_none_or(|thread| Some(thread) != task.provider_thread.as_deref())
                });
            }
        }
        Event::Finalized { .. } => {
            host.requests.remove(&task.id);
            host.activities.remove(&task.id);
        }
        Event::SteerExpired { .. }
        | Event::QueuePaused
        | Event::SteerClaimed { .. }
        | Event::Delivered { .. }
        | Event::DeliveryFailed { .. }
        | Event::TurnStarted { .. }
        | Event::OutcomeReleased => {}
    }
}

// Reads what a decision needs from outside the task. Only a claim looks at the queue.
fn facts(host: &Host, task: &Task, input: &Input) -> Result<Facts> {
    let mut facts = Facts {
        compacting: host.compactions.contains_key(&task.id),
        fresh: new_id(),
        now: now(),
        ..Facts::default()
    };
    if !matches!(input, Input::Command(Command::Claim)) {
        return Ok(facts);
    }
    facts.uncertain = host.store.has_uncertain_submission(&task.id)?;
    if facts.uncertain || task.state == RunState::DeliveryUnknown {
        return Ok(facts);
    }
    facts.outgoing = host.store.outgoing(&task.id)?;
    let Some(message) = next_message(task, &facts.outgoing) else {
        return Ok(facts);
    };
    let valid = provider::validate_preferences(host, &provider::preferences(task))
        .and_then(|()| provider::validate_attachments(host, task, &message.attachments));
    if let Err(error) = valid {
        facts.refusal = Some(format!("{error:#}"));
    } else if message.expected_turn.is_none() {
        if task.handoff.is_some() {
            facts.context = provider::handoff_context(host, task, message)?;
        }
        // A delegated task that cannot write is no second writer of its parent's checkout.
        facts.checkout_busy = !task.shares_checkout()
            && host.store.tasks()?.iter().any(|other| {
                other.id != task.id
                    && other.cwd == task.cwd
                    && other.state.active()
                    && !other.shares_checkout()
            });
    }
    Ok(facts)
}

/// Decides one input for a task and commits its events, then returns the effects to carry out.
///
/// The caller holds the host lock, so inputs of all tasks are decided one at a time. The user
/// is alerted when the state of the task changed.
pub(crate) fn orchestrate(host: &mut Host, task_id: &str, input: Input) -> Result<Vec<Effect>> {
    let mut task = host.store.task(task_id)?;
    let facts = facts(host, &task, &input)?;
    let decision = decide(&task, &facts, &input);
    if !decision.events.is_empty() {
        let previous = task.state.clone();
        let handoff = task.handoff.clone();
        let stored = host.store.latest_run(task_id)?;
        let mut run = stored.clone();
        let mut changes = Vec::new();
        let mut runs = Vec::new();
        for event in &decision.events {
            apply(&mut task, event);
            runs.extend(apply_run(&mut run, &task, &facts, event));
            changes.extend(ledger(event));
        }
        if run != stored {
            runs.extend(run);
        }
        // A handoff that this step consumed is shown where it happened.
        let messages: Vec<Message> = handoff
            .filter(|_| task.handoff.is_none())
            .map(|handoff| provider::handoff_notice(host, &task, &handoff, &facts.fresh, facts.now))
            .into_iter()
            .collect();
        host.store.commit(&Commit {
            task: &task,
            ledger: &changes,
            runs: &runs,
            messages: &messages,
            release_completion: decision
                .events
                .iter()
                .any(|event| matches!(event, Event::Finalized { .. })),
        })?;
        for event in &decision.events {
            forget(host, &task, event);
        }
        if previous != task.state {
            host.notify_task_state(&task);
        }
    }
    if let Some(refusal) = decision.refusal {
        bail!(refusal);
    }
    Ok(decision.effects)
}

/// Observes the checkout after a turn and returns the command that publishes its outcome.
///
/// The capture and the snapshot file are written without the host lock.
pub(crate) fn observe(host: &Shared, task_id: &str, outcome: TurnOutcome) -> Result<Command> {
    let (task, root, run) = {
        let h = host.lock().unwrap();
        (
            h.store.task(task_id)?,
            h.store.root().to_owned(),
            h.store.latest_run(task_id)?,
        )
    };
    // A coding task keeps the files of each turn, so that the user can undo a later one. A
    // failed checkpoint only means that the turn after this one cannot be undone.
    let checkpoint = run
        .filter(|run| run.ended.is_none() && task.state.active() && task.kind == TaskKind::Coding)
        .and_then(|run| {
            let name = run.ordinal.to_string();
            kodem_core::git::checkpoint::capture(&task.cwd, &task.id, &name)
                .ok()
                .map(|_| name)
        });
    // A chat has no checkout to capture, and a folder without Git has no changes to compare.
    let review = (task.state.active()
        && task.kind.has_project()
        && kodem_core::git::is_repository(&task.cwd))
    .then(|| {
        let capture = kodem_core::git::capture(
            &task,
            if task.baseline.is_some() {
                Comparison::Baseline
            } else {
                Comparison::WorkingTree
            },
        );
        match capture {
            Ok(mut snapshot) => {
                let summary = snapshot.summary();
                let stored = kodem_core::storage::Store::write_snapshot(&root, &mut snapshot);
                TaskReview {
                    snapshot_id: stored.as_ref().ok().map(|_| snapshot.id.clone()),
                    added: summary.added,
                    removed: summary.removed,
                    omitted_files: summary.omitted_files,
                    captured: snapshot.captured,
                    error: stored.err().map(|error| error.to_string()),
                }
            }
            Err(error) => TaskReview {
                snapshot_id: None,
                added: 0,
                removed: 0,
                omitted_files: 0,
                captured: now(),
                error: Some(error.to_string()),
            },
        }
    });
    // Only a coding task follows the branch of its checkout. A review keeps the name of the
    // branch it examines, and its worktree is detached.
    let branch = (task.state.active() && task.kind == TaskKind::Coding)
        .then(|| kodem_core::git::branch(&task.cwd));
    Ok(Command::Finalize {
        outcome,
        review,
        branch,
        checkpoint,
    })
}

// Saves the files of a coding task before its first recorded turn changes them, so that this
// turn can be undone too. Later turns start from the checkpoint of the turn before them.
fn save_baseline(host: &Shared, task_id: &str) -> Result<()> {
    let (task, run) = {
        let h = host.lock().unwrap();
        (h.store.task(task_id)?, h.store.latest_run(task_id)?)
    };
    if task.kind == TaskKind::Coding && run.is_some_and(|run| run.ordinal == 1) {
        let _ = kodem_core::git::checkpoint::capture(&task.cwd, &task.id, "0");
    }
    Ok(())
}

/// Commits one input and carries out the effects that need no provider transport.
///
/// Returns the message that the session loop of the task has to send, if the input claimed one.
pub(crate) fn dispatch(
    host: &Shared,
    task_id: &str,
    input: Input,
) -> Result<Option<OutgoingMessage>> {
    let mut inputs = VecDeque::from([input]);
    let mut send = None;
    while let Some(input) = inputs.pop_front() {
        let effects = orchestrate(&mut host.lock().unwrap(), task_id, input)?;
        for effect in effects {
            match effect {
                Effect::Send(message) => {
                    if message.expected_turn.is_none() {
                        save_baseline(host, task_id)?;
                    }
                    send = Some(message);
                }
                Effect::Capture(outcome) => {
                    inputs.push_back(Input::Command(observe(host, task_id, outcome)?));
                }
            }
        }
    }
    Ok(send)
}
