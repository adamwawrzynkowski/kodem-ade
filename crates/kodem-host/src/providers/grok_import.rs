//! Reads the sessions that Grok Build stored on this computer, for an import into Kodem.
//!
//! The CLI keeps each session in a folder of its home, with a summary and every update of the
//! conversation. A session that Kodem started holds Kodem's standing instructions in its system
//! prompt.
use super::*;
use crate::imports::{Stored, title};
use kodem_core::providers::grok::Summary;

/// Lines of a stored session that the listing reads for its first prompt.
const OPENING_LINES: usize = 200;

// The first prompt of a stored session.
fn opening(summary: &Summary) -> Option<String> {
    transcript::read_updates(&summary.updates(), OPENING_LINES)
        .iter()
        .map(|line| &line["params"]["update"])
        .filter(|update| update["sessionUpdate"] == "user_message_chunk")
        .map(transcript::user_text)
        .find(|text| !text.is_empty())
        .map(title)
}

fn stored(summary: Summary) -> Stored {
    let title = Some(summary.title.trim().to_owned())
        .filter(|title| !title.is_empty())
        .or_else(|| opening(&summary))
        .unwrap_or_default();
    Stored {
        kodem: summary.kodem(),
        thread: summary.session,
        title,
        cwd: summary.cwd,
        created: summary.created,
        updated: summary.updated,
        model: summary.model,
    }
}

/// Lists the account's top-level sessions.
pub(crate) fn stored_threads(account: &Account) -> Result<Vec<Stored>> {
    Ok(grok::summaries(&home_of(account)?)?
        .into_iter()
        .filter(|summary| !summary.subagent && !summary.cwd.as_os_str().is_empty())
        .map(stored)
        .collect())
}

/// Reads one stored session for an import.
pub(crate) fn stored_thread(account: &Account, session: &str) -> Result<Stored> {
    let summary = grok::summary(&home_of(account)?, session)?
        .context("Grok Build no longer has this session")?;
    ensure!(
        !summary.subagent,
        "This session belongs to an agent of another session. Import that one instead."
    );
    Ok(stored(summary))
}

/// Writes the stored session that `task` continues, with its subagents, into the task's
/// transcript.
pub(crate) fn import_history(host: &Shared, task: &Task, account: &Account) -> Result<()> {
    let session = task
        .provider_thread
        .as_deref()
        .context("The imported task has no Grok Build session")?;
    let home = home_of(account)?;
    let summary =
        grok::summary(&home, session)?.context("Grok Build no longer has this session")?;
    let mut transcript = Transcript::new(&task.id, &task.cwd, &home, true);
    transcript.set_main(session);
    transcript.replay(host, &summary.updates(), None)?;
    // The subagents that the session started are sessions of their own.
    let agents: Vec<String> = transcript.agents().cloned().collect();
    for agent in agents {
        if let Some(stored) = grok::summary(&home, &agent)? {
            transcript.replay(host, &stored.updates(), None)?;
        }
    }
    // An agent that the stored session did not see end stopped with the CLI.
    transcript.stop_agents(host, RunState::Interrupted)?;
    transcript.finish(host)?;
    let h = host.lock().unwrap();
    let mut current = h.store.task(&task.id)?;
    if let Some(used) = transcript.used {
        current.context_usage = Some(ContextUsage {
            used,
            window: None,
            reported: now(),
            cost: transcript.cost.map(|amount| UsageCost {
                amount,
                currency: "USD".into(),
            }),
        });
    }
    current.plan = transcript.plan.take().unwrap_or(current.plan);
    h.store.save_task(&current)
}
