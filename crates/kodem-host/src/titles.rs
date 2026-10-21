//! Background model-generated titles, independent of conversation delivery.
use crate::{Shared, accounts, provider};
use anyhow::Result;
use kodem_core::{Account, MessageKind, Provider, Task, providers::codex::Codex};

const NEW: &str = "Generate a concise thread title summarizing the supplied request. Use 3 to 7 words, at most 64 characters, in the request's language. Describe the task, not a reply.";
const REVIEW: &str = "Review the title of an ongoing thread. The input holds its current title and the user's latest requests, oldest first. Return the current title exactly while it still describes the thread. Only when the main subject has clearly changed, so that the current title would mislead, return a new concise title of 3 to 7 words, at most 64 characters, in the requests' language. Describe the work, not a reply.";
// Messages sent after a title was generated or reviewed before the next review.
const REVIEW_INTERVAL: u32 = 3;
// Latest requests that a review reads.
const REVIEW_REQUESTS: usize = 5;

fn first_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("New conversation")
        .chars()
        .take(80)
        .collect()
}

// Title generation cannot fail or replay the user's actual turn.
fn generate(task: &Task, account: &Account, instructions: &str, input: &str) -> Result<String> {
    match task.provider {
        Provider::Codex => Codex::generate_title(
            &task.cwd,
            account,
            task.model.as_deref(),
            instructions,
            input,
        ),
        _ => provider::generate_title(account, instructions, input),
    }
}

/// Generates a short title for a new task without delaying its first turn.
pub(super) fn start(host: Shared, task_id: &str) -> Result<()> {
    let (task, account, fallback, input) = {
        let h = host.lock().unwrap();
        let task = h.store.task(task_id)?;
        // OpenCode, the Antigravity CLI and Grok Build name their conversations themselves; their
        // session owners name one that they did not.
        if !task.title_pending
            || matches!(
                task.provider,
                Provider::OpenCode | Provider::Antigravity | Provider::Grok
            )
        {
            return Ok(());
        }
        let Some(message) = h.store.outgoing(task_id)?.into_iter().next() else {
            return Ok(());
        };
        let input = serde_json::json!({
            "request": message.text.chars().take(6000).collect::<String>(),
            "attachments": message.attachments.iter().take(20).map(|a| &a.name).collect::<Vec<_>>(),
            "review_comments": message.feedback.iter().take(10).map(|f| f.comment.chars().take(300).collect::<String>()).collect::<Vec<_>>()
        });
        let account = accounts::for_task(&h, &task)?;
        (task, account, first_line(&message.text), input.to_string())
    };
    std::thread::spawn(move || {
        let title = match generate(&task, &account, NEW, &input) {
            Ok(title) => title,
            // Claude Code has no later title source, so a failed generation keeps the prompt line.
            Err(_) if task.provider != Provider::Codex => fallback,
            Err(_) => return,
        };
        let h = host.lock().unwrap();
        if let Ok(mut current) = h.store.task(&task.id)
            && current.title_pending
        {
            current.title = title;
            current.title_pending = false;
            current.title_review = Some(0);
            let _ = h.store.save_task(&current);
        }
    });
    Ok(())
}

/// Counts a sent message toward the next review of a generated title and reports whether that
/// review is due. The caller saves the task.
pub(super) fn count(task: &mut Task) -> bool {
    let Some(sent) = task.title_review else {
        return false;
    };
    let due = sent + 1 >= REVIEW_INTERVAL;
    task.title_review = Some(if due { 0 } else { sent + 1 });
    due
}

/// Replaces a generated title in the background when the conversation has clearly moved to
/// another subject, and keeps it otherwise.
pub(super) fn review(host: Shared, task_id: &str) -> Result<()> {
    let (task, account, input) = {
        let h = host.lock().unwrap();
        let task = h.store.task(task_id)?;
        if task.title_review.is_none()
            || matches!(
                task.provider,
                Provider::OpenCode | Provider::Antigravity | Provider::Grok
            )
        {
            return Ok(());
        }
        let (messages, _) = h.store.messages(task_id, None)?;
        let mut requests: Vec<(String, String)> = messages
            .into_iter()
            .filter(|message| message.kind == MessageKind::User)
            .map(|message| (message.id, message.text))
            .collect();
        // Queued input reaches the transcript only when its turn starts.
        for message in h.store.outgoing(task_id)? {
            let id = format!("submission-{}", message.id);
            if !requests.iter().any(|(known, _)| *known == id) {
                requests.push((id, message.text));
            }
        }
        let skip = requests.len().saturating_sub(REVIEW_REQUESTS);
        let input = serde_json::json!({
            "current_title": task.title,
            "recent_requests": requests
                .iter()
                .skip(skip)
                .map(|(_, text)| text.chars().take(1200).collect::<String>())
                .collect::<Vec<_>>()
        });
        let account = accounts::for_task(&h, &task)?;
        (task, account, input.to_string())
    };
    std::thread::spawn(move || {
        let Ok(title) = generate(&task, &account, REVIEW, &input) else {
            return;
        };
        if title.to_lowercase() == task.title.trim().to_lowercase() {
            return;
        }
        let h = host.lock().unwrap();
        // A manual rename or another review since this one started keeps its title.
        if let Ok(mut current) = h.store.task(&task.id)
            && current.title_review.is_some()
            && current.title == task.title
        {
            current.title = title;
            let _ = h.store.save_task(&current);
        }
    });
    Ok(())
}
