//! Reads the conversations that the Antigravity CLI stored on this computer, for an import into
//! Kodem.
//!
//! The CLI keeps an index of its conversations, with their titles and folders, and the steps of
//! each in a file of its own. A conversation that a Kodem task ran names the task's session
//! folder among its folders.
use super::*;
use crate::imports::{Stored, title};
use kodem_core::providers::antigravity::Summary;

// The opening of a stored conversation: when it began and its first prompt.
fn opening(conversation: &str) -> (Option<u64>, Option<String>) {
    let Ok(file) =
        antigravity::transcript(conversation).and_then(|path| Ok(std::fs::File::open(path)?))
    else {
        return (None, None);
    };
    let mut created = None;
    for line in BufReader::new(file.take(LINE_LIMIT)).lines().take(50) {
        let Ok(step) = line.map(|line| serde_json::from_str::<Value>(&line)) else {
            break;
        };
        let Ok(step) = step else {
            continue;
        };
        if created.is_none() {
            created = step["created_at"].as_str().and_then(unix_millis);
        }
        if step["type"] == "USER_INPUT" {
            let text = transcript::request_text(step["content"].as_str().unwrap_or_default());
            if !text.is_empty() {
                return (created, Some(title(text)));
            }
        }
    }
    (created, None)
}

fn stored(summary: Summary) -> Stored {
    let kodem = summary
        .folders
        .iter()
        .any(|folder| is_session_folder(folder));
    // The CLI orders the folders by path; the conversation's own is the first that is not a
    // session folder of Kodem's.
    let cwd = summary
        .folders
        .iter()
        .find(|folder| !is_session_folder(folder))
        .cloned()
        .unwrap_or_default();
    let (created, opening) = opening(&summary.conversation);
    let title = Some(summary.title.trim().to_owned())
        .filter(|title| !title.is_empty())
        .or(opening)
        .unwrap_or_default();
    Stored {
        thread: summary.conversation,
        title,
        cwd,
        created: created.unwrap_or(summary.updated),
        updated: summary.updated,
        // The CLI does not store the model of a conversation.
        model: None,
        kodem,
    }
}

/// Lists the account's top-level conversations. The CLI has one account, so every account
/// lists the same ones.
pub(crate) fn stored_threads(_account: &Account) -> Result<Vec<Stored>> {
    Ok(antigravity::summaries()?
        .into_iter()
        .filter(|summary| !summary.folders.is_empty())
        .map(stored)
        .collect())
}

/// Reads one stored conversation for an import.
pub(crate) fn stored_thread(_account: &Account, conversation: &str) -> Result<Stored> {
    ensure!(
        antigravity::valid_conversation(conversation),
        "\"{conversation}\" is not an Antigravity conversation"
    );
    let summary = antigravity::summary(conversation)?
        .context("The Antigravity CLI no longer has this conversation")?;
    ensure!(
        summary.parent.is_none(),
        "This conversation belongs to an agent of another conversation. Import that one instead."
    );
    Ok(stored(summary))
}

/// Writes the stored conversation that `task` continues, with its subagents, into the task's
/// transcript.
pub(crate) fn import_history(host: &Shared, task: &Task, _account: &Account) -> Result<()> {
    let conversation = task
        .provider_thread
        .as_deref()
        .context("The imported task has no Antigravity conversation")?;
    let mut transcript = Transcript::new(&task.id, &task.cwd, true);
    transcript.set_main(conversation);
    transcript.replay(host, conversation)?;
    transcript.finish(host)?;
    if let Some(used) = transcript.used {
        let h = host.lock().unwrap();
        let mut current = h.store.task(&task.id)?;
        current.context_usage = Some(ContextUsage {
            used,
            window: None,
            reported: now(),
            cost: None,
        });
        h.store.save_task(&current)?;
    }
    Ok(())
}
