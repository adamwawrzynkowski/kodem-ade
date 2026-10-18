//! Reads the conversations that Claude Code stored on this computer, for an import into Kodem.
//!
//! The CLI keeps one JSON Lines file per conversation under `projects` in its configuration
//! directory, and the conversations of its agents beside it. The format is internal to the CLI,
//! so an entry that cannot be read is skipped.
use super::*;
use crate::imports::{Stored, title};
use std::{fs, time::UNIX_EPOCH};

/// Longest stored entry that is read, in bytes. A longer line is skipped.
const LINE_LIMIT: u64 = 32 * 1024 * 1024;
/// Longest entry that a listing reads after the start of a conversation. Titles are short.
const SHORT_LINE: usize = 4096;
/// Longest prompt that an import keeps, in bytes, within the store's limit for one row.
const PROMPT_LIMIT: usize = 1024 * 1024;
/// A conversation that changed more recently may still be written by the CLI.
const SETTLED: Duration = Duration::from_secs(15);
/// Entries that the CLI writes as user text for its own output and its notices.
const GENERATED: [&str; 4] = [
    "<local-command-stdout>",
    "<local-command-stderr>",
    "<local-command-caveat>",
    "<task-notification>",
];

/// The entry point that Claude Code records for a session in print mode, which is how Kodem
/// runs it. Claude Code's own terminal, desktop and IDE apps and the apps built on its SDK
/// record other entry points.
const PRINT_MODE: &str = "sdk-cli";

// Calls `each` with every line of a stored file.
fn lines(path: &Path, mut each: impl FnMut(&[u8]) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::new(fs::File::open(path)?);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .by_ref()
            .take(LINE_LIMIT + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(());
        }
        if line.last() != Some(&b'\n') && read as u64 > LINE_LIMIT {
            reader.skip_until(b'\n')?;
            continue;
        }
        each(&line)?;
    }
}

// Returns the text between the first `<tag>` and `</tag>` of `text`.
fn tagged<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let start = text.find(&format!("<{tag}>"))? + tag.len() + 2;
    let end = text[start..].find(&format!("</{tag}>"))? + start;
    Some(text[start..end].trim())
}

// Returns what the user wrote in a stored entry, or `None` for an entry that the CLI wrote.
fn prompt(entry: &Value) -> Option<String> {
    if entry["type"] != "user"
        || entry["isSidechain"] == true
        || entry["isMeta"] == true
        || entry["isCompactSummary"] == true
        || entry["isVisibleInTranscriptOnly"] == true
        || entry["promptSource"] == "system"
        || !entry["toolUseResult"].is_null()
        || entry["origin"]["kind"]
            .as_str()
            .is_some_and(|kind| kind != "human")
    {
        return None;
    }
    let content = &entry["message"]["content"];
    let text = match content.as_str() {
        Some(text) => text.to_owned(),
        None => content
            .as_array()?
            .iter()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    };
    let text = text.trim();
    if GENERATED.iter().any(|tag| text.starts_with(tag)) {
        return None;
    }
    // A slash command is stored as tags around its name and its arguments.
    if let Some(name) = tagged(text, "command-name") {
        let arguments = tagged(text, "command-args").unwrap_or_default();
        return Some(format!("{name} {arguments}").trim_end().to_owned());
    }
    (!text.is_empty()).then(|| text.to_owned())
}

// Returns the wall-clock milliseconds of a stored entry.
fn milliseconds(entry: &Value) -> Option<u64> {
    let stamp = entry["timestamp"].as_str()?;
    let fraction = stamp
        .split_once('.')
        .map(|(_, rest)| {
            rest.chars()
                .take_while(char::is_ascii_digit)
                .chain(std::iter::repeat('0'))
                .take(3)
                .collect::<String>()
        })
        .and_then(|digits| digits.parse::<u64>().ok())
        .unwrap_or(0);
    Some(unix_seconds(stamp)? * 1000 + fraction)
}

// Returns the wall-clock milliseconds of a file's last change.
fn modified(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64))
}

// Reads a conversation's folder, times, title and opening prompt, and whether Kodem ran it. A
// conversation without a prompt of the user, such as one that only listed commands, gives
// `None`. With `model`, every entry is read to find the model of the latest answer.
fn summary(path: &Path, session: &str, model: bool) -> Result<Option<Stored>> {
    let mut cwd = None;
    let mut created = None;
    let mut opening = None;
    let mut named = None;
    let mut generated = None;
    let mut latest = None;
    let mut entrypoint = None;
    lines(path, |line| {
        let started = cwd.is_some() && created.is_some() && opening.is_some();
        if started && !model && line.len() > SHORT_LINE {
            return Ok(());
        }
        let Ok(entry) = serde_json::from_slice::<Value>(line) else {
            return Ok(());
        };
        let text = |field: &str| {
            entry[field]
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        match entry["type"].as_str() {
            Some("custom-title") => named = text("customTitle").or(named.take()),
            Some("ai-title") => generated = text("aiTitle").or(generated.take()),
            Some("assistant") if entry["isSidechain"] != true => {
                latest = entry["message"]["model"]
                    .as_str()
                    .filter(|model| !model.starts_with('<'))
                    .map(str::to_owned)
                    .or(latest.take());
            }
            _ => {}
        }
        if entry["isSidechain"] == true {
            return Ok(());
        }
        if cwd.is_none() {
            cwd = entry["cwd"].as_str().map(PathBuf::from);
        }
        if created.is_none() {
            created = milliseconds(&entry);
        }
        if entrypoint.is_none() {
            entrypoint = text("entrypoint");
        }
        if opening.is_none() {
            opening = prompt(&entry);
        }
        Ok(())
    })?;
    let (Some(cwd), Some(opening)) = (cwd, opening) else {
        return Ok(None);
    };
    let updated = modified(path)?;
    Ok(Some(Stored {
        thread: session.to_owned(),
        title: title(named.or(generated).as_deref().unwrap_or(&opening)),
        cwd,
        created: created.unwrap_or(updated),
        updated,
        model: latest,
        kodem: entrypoint.as_deref() == Some(PRINT_MODE),
    }))
}

/// Lists the conversations that Claude Code stored under an account's configuration directory.
pub(crate) fn stored_threads(account: &Account) -> Result<Vec<Stored>> {
    let projects = config_home(account)?.join("projects");
    // An account that never ran a conversation has no folder for them.
    let Ok(folders) = fs::read_dir(&projects) else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    for folder in folders.flatten() {
        let Ok(files) = fs::read_dir(folder.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let Some(session) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|_| {
                    path.extension()
                        .is_some_and(|extension| extension == "jsonl")
                })
                .filter(|stem| valid_session(stem))
            else {
                continue;
            };
            // One unreadable conversation must not hide the others.
            if let Ok(Some(stored)) = summary(&path, session, false) {
                found.push(stored);
            }
        }
    }
    Ok(found)
}

/// Reads one stored conversation of an account, with the model of its latest answer.
pub(crate) fn stored_thread(account: &Account, session: &str) -> Result<Stored> {
    let path = session_file(account, session)?
        .context("Claude Code has no stored conversation with this identity under this account")?;
    summary(&path, session, true)?.context("This conversation has no message of yours to import")
}

// Applies one stored entry to the transcript. `agent` names the agent whose conversation the
// entry belongs to; the CLI stores those in files of their own.
fn replay(
    host: &Shared,
    transcript: &mut Transcript,
    task_id: &str,
    entry: &Value,
    agent: Option<&str>,
) -> Result<()> {
    if agent.is_none() && entry["isSidechain"] == true {
        return Ok(());
    }
    let created = milliseconds(entry);
    if let Some(time) = created {
        transcript.at(time);
    }
    let routed = |mut event: Value| {
        if let Some(agent) = agent {
            event["agent_id"] = json!(agent);
        }
        event
    };
    let results = entry["message"]["content"]
        .as_array()
        .is_some_and(|blocks| blocks.iter().any(|block| block["type"] == "tool_result"));
    match entry["type"].as_str() {
        Some("assistant") => transcript.ingest(
            host,
            &routed(json!({"type":"assistant","message":entry["message"],
                "parent_tool_use_id":null,"apiBlockIndex":entry["apiBlockIndex"],
                "thinking_duration_ms":entry["thinkingDurationMs"]})),
        ),
        Some("user") if results => transcript.ingest(
            host,
            &routed(json!({"type":"user","message":entry["message"],
                "parent_tool_use_id":null,"tool_use_result":entry["toolUseResult"]})),
        ),
        // An agent's first prompt is already stored with the call that started the agent.
        Some("user") if agent.is_none() => {
            let (Some(text), Some(uuid)) = (prompt(entry), entry["uuid"].as_str()) else {
                return Ok(());
            };
            let mut end = text.len().min(PROMPT_LIMIT);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            host.lock().unwrap().store.message(&Message {
                seq: 0,
                transfer: None,
                id: format!("claude:user:{uuid}"),
                task_id: task_id.to_owned(),
                kind: MessageKind::User,
                attachments: Vec::new(),
                feedback: Vec::new(),
                file_edits: Vec::new(),
                tool: None,
                text: text[..end].to_owned(),
                complete: true,
                created: created.unwrap_or_else(now),
            })
        }
        Some("system") if agent.is_none() && entry["subtype"] == "compact_boundary" => {
            let metadata = &entry["compactMetadata"];
            transcript.ingest(
                host,
                &json!({"type":"system","subtype":"compact_boundary","uuid":entry["uuid"],
                    "compact_metadata":{"pre_tokens":metadata["preTokens"],
                    "post_tokens":metadata["postTokens"]}}),
            )
        }
        _ => Ok(()),
    }
}

/// Writes the stored history of the conversation that `task` continues into its transcript,
/// with the conversations of its agents.
pub(crate) fn import_history(host: &Shared, task: &Task, account: &Account) -> Result<()> {
    let session = task
        .provider_thread
        .as_deref()
        .context("The task names no Claude Code conversation")?;
    let path = session_file(account, session)?
        .context("Claude Code has no stored conversation with this identity under this account")?;
    let idle = SystemTime::now()
        .duration_since(fs::metadata(&path)?.modified()?)
        .unwrap_or_default();
    ensure!(
        idle >= SETTLED,
        "Claude Code may still be writing this conversation. Import it again in a moment."
    );
    let mut transcript = Transcript::new(&task.id, &task.cwd);
    lines(&path, |line| match serde_json::from_slice::<Value>(line) {
        Ok(entry) => replay(host, &mut transcript, &task.id, &entry, None),
        Err(_) => Ok(()),
    })?;
    let agents: Vec<String> = host
        .lock()
        .unwrap()
        .store
        .task(&task.id)?
        .subagents
        .into_iter()
        .map(|agent| agent.thread_id)
        .filter(|agent| valid_session(agent))
        .collect();
    let folder = path.with_extension("").join("subagents");
    for agent in agents {
        let file = folder.join(format!("agent-{agent}.jsonl"));
        if !file.is_file() {
            continue;
        }
        lines(&file, |line| match serde_json::from_slice::<Value>(line) {
            Ok(entry) => replay(host, &mut transcript, &task.id, &entry, Some(&agent)),
            Err(_) => Ok(()),
        })?;
    }
    transcript.conclude(host)
}
