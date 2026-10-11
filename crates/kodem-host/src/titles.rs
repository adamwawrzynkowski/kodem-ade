//! Background model-generated titles, independent of conversation delivery.
use crate::{Shared, accounts, provider};
use anyhow::Result;
use kodem_core::{Provider, providers::codex::Codex};

fn first_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("New conversation")
        .chars()
        .take(80)
        .collect()
}

/// Generates a short title for a new task without delaying its first turn.
pub(super) fn start(host: Shared, task_id: &str) -> Result<()> {
    let (task, account, fallback, input) = {
        let h = host.lock().unwrap();
        let task = h.store.task(task_id)?;
        if !task.title_pending {
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
        // Title generation cannot fail or replay the user's actual turn.
        let title = match task.provider {
            Provider::Codex => {
                match Codex::generate_title(&task.cwd, &account, task.model.as_deref(), &input) {
                    Ok(title) => title,
                    Err(_) => return,
                }
            }
            // Claude Code has no later title source, so a failed generation keeps the prompt line.
            _ => provider::generate_title(&account, &input).unwrap_or(fallback),
        };
        let h = host.lock().unwrap();
        if let Ok(mut current) = h.store.task(&task.id)
            && current.title_pending
        {
            current.title = title;
            current.title_pending = false;
            let _ = h.store.save_task(&current);
        }
    });
    Ok(())
}
