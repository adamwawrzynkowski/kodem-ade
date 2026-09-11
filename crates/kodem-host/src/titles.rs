//! Background model-generated titles, independent of conversation delivery.
use crate::Shared;
use anyhow::Result;
use kodem_core::providers::codex::Codex;

/// Generates a short title for a new task without delaying its first turn.
pub(super) fn start(host: Shared, task_id: &str) -> Result<()> {
    let (task, input) = {
        let h = host.lock().unwrap();
        let task = h.store.task(task_id)?;
        if !task.title_pending {
            return Ok(());
        }
        let Some(message) = h.store.outgoing(task_id)?.into_iter().next() else {
            return Ok(());
        };
        if task.provider != kodem_core::Provider::Codex {
            let mut task = task;
            task.title = message
                .text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("New conversation")
                .chars()
                .take(80)
                .collect();
            task.title_pending = false;
            h.store.save_task(&task)?;
            return Ok(());
        }
        let input = serde_json::json!({
            "request": message.text.chars().take(6000).collect::<String>(),
            "attachments": message.attachments.iter().take(20).map(|a| &a.name).collect::<Vec<_>>(),
            "review_comments": message.feedback.iter().take(10).map(|f| f.comment.chars().take(300).collect::<String>()).collect::<Vec<_>>()
        });
        (task, input.to_string())
    };
    std::thread::spawn(move || {
        let Ok(title) = Codex::generate_title(&task.cwd, task.model.as_deref(), &input) else {
            // Title generation cannot fail or replay the user's actual turn.
            return;
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
