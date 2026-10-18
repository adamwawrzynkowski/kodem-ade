//! Reads the conversations that Codex stored on this computer, for an import into Kodem.
//!
//! Codex's app-server lists and reads stored threads without loading them, so an import does
//! not depend on the files that Codex keeps them in.
use super::*;
use crate::imports::{Stored, title};
use anyhow::bail;

/// Most pages that one listing reads, of threads, turns or the items of one turn.
const PAGES: usize = 500;
/// Items of one page. A page arrives as one event, which Kodem reads up to 8 MiB.
const ITEMS: u64 = 20;

// Reads a thread of `thread/list` or `thread/read`. A thread without a folder gives `None`.
fn stored(thread: &Value) -> Option<Stored> {
    let id = thread["id"].as_str()?;
    let cwd = thread["cwd"].as_str().filter(|cwd| !cwd.is_empty())?;
    let text = thread["name"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| thread["preview"].as_str())
        .unwrap_or_default();
    let created = thread["createdAt"].as_u64().unwrap_or_default() * 1000;
    Some(Stored {
        thread: id.to_owned(),
        title: title(text),
        cwd: PathBuf::from(cwd),
        created,
        updated: thread["updatedAt"]
            .as_u64()
            .map_or(created, |seconds| seconds * 1000),
        model: thread["model"].as_str().map(str::to_owned),
        kodem: thread["originator"] == kodem_core::providers::codex::CLIENT_NAME,
    })
}

// Lists the threads of a running app-server. With `indexed`, Codex answers from its database of
// threads without reading their files again, which takes seconds for some hundred threads.
fn list(codex: &mut Codex, indexed: bool) -> Result<Vec<Stored>> {
    let mut found = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let page = codex.call(
            "thread/list",
            json!({"limit":100,"cursor":cursor,"useStateDbOnly":indexed}),
            &mut |_| {},
        )?;
        for thread in page["data"].as_array().into_iter().flatten() {
            // An agent's thread belongs to the conversation that started the agent.
            if thread["ephemeral"] == true || !thread["parentThreadId"].is_null() {
                continue;
            }
            found.extend(stored(thread));
        }
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    Ok(found)
}

/// Lists the conversations that Codex stored for an account. Without Codex there are none.
pub(crate) fn stored_threads(account: &Account) -> Result<Vec<Stored>> {
    if kodem_core::providers::codex::executable().is_err() {
        return Ok(Vec::new());
    }
    let mut codex = Codex::start(&kodem_core::platform::home_dir()?, account, |_| {})?;
    // A Codex release without the database listing reads the files instead.
    list(&mut codex, true).or_else(|_| list(&mut codex, false))
}

/// Reads one stored conversation of an account.
pub(crate) fn stored_thread(account: &Account, thread: &str) -> Result<Stored> {
    let mut codex = Codex::start(&kodem_core::platform::home_dir()?, account, |_| {})?;
    let read = codex.call(
        "thread/read",
        json!({"threadId":thread,"includeTurns":false}),
        &mut |_| {},
    )?;
    ensure!(
        read["thread"]["status"]["type"] != "active",
        "Codex is still working in this conversation. Import it again when it finishes."
    );
    stored(&read["thread"]).context("Codex recorded no folder for this conversation")
}

// Writes the items of one turn: the user's message as Kodem stores its own, and the others as
// their completion events.
fn import_turn(
    host: &Shared,
    codex: &mut Codex,
    task: &Task,
    thread: &str,
    turn: &str,
    items: &mut Streamed,
) -> Result<()> {
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let page = codex.call(
            "thread/items/list",
            json!({"threadId":thread,"turnId":turn,"limit":ITEMS,"sortDirection":"asc","cursor":cursor}),
            &mut |_| {},
        )?;
        for entry in page["data"].as_array().into_iter().flatten() {
            let mut item = entry["item"].clone();
            let Some(id) = item["id"].as_str() else {
                continue;
            };
            if item["type"] == "userMessage" {
                let text = item["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                if text.trim().is_empty() {
                    continue;
                }
                let mut message = Message {
                    seq: 0,
                    transfer: None,
                    id: format!("{turn}:{id}"),
                    task_id: task.id.clone(),
                    kind: MessageKind::User,
                    attachments: Vec::new(),
                    feedback: Vec::new(),
                    file_edits: Vec::new(),
                    tool: None,
                    text: String::new(),
                    complete: true,
                    created: entry["startedAtMs"].as_u64().unwrap_or_else(now),
                };
                append_bounded(&mut message.text, &text);
                host.lock().unwrap().store.message(&message)?;
                continue;
            }
            // A question of an earlier session is not asked again.
            if let Some(fields) = item.as_object_mut() {
                fields.remove("questions");
            }
            ingest(
                host,
                &task.id,
                json!({"method":"item/completed","params":{"threadId":thread,"turnId":turn,"item":item}}),
                items,
            )?;
        }
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            // The final answer waits apart until its turn ends, which it did long ago.
            return host
                .lock()
                .unwrap()
                .store
                .release_completion_messages(&task.id);
        }
    }
    bail!("This conversation has more items in one turn than an import reads")
}

/// Writes the stored history of the conversation that `task` continues into its transcript.
pub(crate) fn import_history(host: &Shared, task: &Task, account: &Account) -> Result<()> {
    let thread = task
        .provider_thread
        .as_deref()
        .context("The task names no Codex conversation")?;
    let mut codex = Codex::start(&task.cwd, account, |_| {})?;
    let mut items = Streamed::new();
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let page = codex.call(
            "thread/turns/list",
            json!({"threadId":thread,"cursor":cursor,"limit":ITEMS,"sortDirection":"asc","itemsView":"notLoaded"}),
            &mut |_| {},
        )?;
        for turn in page["data"].as_array().into_iter().flatten() {
            if let Some(turn) = turn["id"].as_str() {
                import_turn(host, &mut codex, task, thread, turn, &mut items)?;
            }
        }
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            if items.pending() {
                items.write(&host.lock().unwrap().store)?;
            }
            return Ok(());
        }
    }
    bail!("This conversation has more turns than an import reads")
}
