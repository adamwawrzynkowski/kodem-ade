//! Reads the conversations that OpenCode stored on this computer, for an import into Kodem.
//!
//! OpenCode keeps its sessions in its data folder, which the account selects, and serves them
//! through its server. A probe server lists them for every project and reads one for an import.
use super::*;
use crate::imports::{Stored, title};
use kodem_core::now;

/// Titles that OpenCode 1 gave sessions before naming them; sessions that it migrated keep them.
const UNTITLED: [&str; 2] = ["New session - ", "Child session - "];
/// Most sessions that a listing reads.
const LISTING_LIMIT: usize = 5000;
/// A conversation that changed more recently may still be written by OpenCode.
const SETTLED: u64 = 15_000;

// The model of a stored session, as `provider/model`.
fn model(session: &Value) -> Option<String> {
    let model = &session["model"];
    Some(format!(
        "{}/{}",
        model["providerID"].as_str()?,
        model["id"].as_str()?
    ))
}

// The first text that the user wrote in a session, or `None` for a session without a prompt.
fn opening(client: &Client, session: &str) -> Result<Option<String>> {
    let first = client.get(&format!(
        "/api/session/{session}/message?type=user&order=asc&limit=1"
    ))?;
    Ok(items(&first)
        .first()
        .and_then(|message| message["text"].as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned))
}

// Describes a stored session. A session that OpenCode did not title is named by its first
// prompt, and one without a prompt gives `None`.
fn stored(client: &Client, session: &Value) -> Result<Option<Stored>> {
    let (Some(id), Some(directory)) = (
        session["id"].as_str(),
        session["location"]["directory"].as_str(),
    ) else {
        return Ok(None);
    };
    let named = session["title"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .filter(|name| !UNTITLED.iter().any(|untitled| name.starts_with(untitled)));
    let name = match named {
        Some(name) => name.to_owned(),
        None => match opening(client, id)? {
            Some(prompt) => prompt,
            None => return Ok(None),
        },
    };
    let updated = session["time"]["updated"].as_u64().unwrap_or_default();
    Ok(Some(Stored {
        thread: id.to_owned(),
        title: title(&name),
        cwd: PathBuf::from(directory),
        created: session["time"]["created"].as_u64().unwrap_or(updated),
        updated,
        model: model(session),
        kodem: !session["metadata"]["kodem"].is_null(),
    }))
}

/// Lists the conversations that OpenCode stored for an account, in every project.
pub(crate) fn stored_threads(account: &Account) -> Result<Vec<Stored>> {
    let server = probe(&home()?, account)?;
    let client = server.client();
    let mut found = Vec::new();
    let mut path = format!("/api/session?parentID=null&limit={PAGE}");
    loop {
        let page = client.get(&path)?;
        let sessions = items(&page);
        for session in sessions {
            // One unreadable conversation must not hide the others.
            if let Ok(Some(thread)) = stored(client, session) {
                found.push(thread);
            }
        }
        match page["cursor"]["next"].as_str() {
            Some(cursor) if sessions.len() >= PAGE && found.len() < LISTING_LIMIT => {
                path = format!("/api/session?parentID=null&limit={PAGE}&cursor={cursor}");
            }
            _ => break,
        }
    }
    Ok(found)
}

// Accepts only the characters of OpenCode's session identities, which go into request paths.
fn valid(session: &str) -> bool {
    !session.is_empty()
        && session
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Reads one stored conversation of an account, with the model of its latest answer.
pub(crate) fn stored_thread(account: &Account, session: &str) -> Result<Stored> {
    ensure!(valid(session), "This is not an OpenCode conversation");
    let server = probe(&home()?, account)?;
    let client = server.client();
    let info = client
        .get(&format!("/api/session/{session}"))
        .context("OpenCode has no stored conversation with this identity under this account")?;
    let info = &info["data"];
    ensure!(
        info["parentID"].is_null(),
        "This conversation belongs to an agent. Import the conversation that started it."
    );
    stored(client, info)?.context("This conversation has no message of yours to import")
}

/// Writes the stored history of the conversation that `task` continues into its transcript,
/// with the conversations of its agents.
pub(crate) fn import_history(host: &Shared, task: &Task, account: &Account) -> Result<()> {
    let session = task
        .provider_thread
        .as_deref()
        .context("The task names no OpenCode conversation")?;
    ensure!(valid(session), "This is not an OpenCode conversation");
    let server = probe(&task.cwd, account)?;
    let client = server.client();
    let info = client
        .get(&format!("/api/session/{session}"))
        .context("OpenCode has no stored conversation with this identity under this account")?;
    let info = &info["data"];
    let updated = info["time"]["updated"].as_u64().unwrap_or_default();
    let active = client.get("/api/session/active")?;
    ensure!(
        active["data"][session].is_null() && now().saturating_sub(updated) >= SETTLED,
        "OpenCode may still be writing this conversation. Import it again in a moment."
    );
    let stored = messages(client, session)?;
    let mut transcript = Transcript::new(&task.id, &task.cwd, true);
    transcript.set_main(session);
    replay(host, client, &mut transcript, session, &stored, true)?;
    transcript.finish(host)?;
    if let Some(used) = transcript.used {
        let h = host.lock().unwrap();
        let mut current = h.store.task(&task.id)?;
        current.context_usage = Some(ContextUsage {
            used,
            window: None,
            reported: now(),
            cost: info["cost"]
                .as_f64()
                .filter(|cost| cost.is_finite() && *cost > 0.0)
                .map(|amount| UsageCost {
                    amount,
                    currency: "USD".into(),
                }),
        });
        h.store.save_task(&current)?;
    }
    Ok(())
}
