//! Maps OpenCode's session events and stored messages onto Kodem transcripts, tool calls and
//! subagents.
//!
//! A live session reports each text, reasoning and tool call of an assistant message as events:
//! a start, deltas or progress, and an end. A stored conversation keeps the same items as the
//! content of its messages. A subagent is a session whose `parentID` names the session that
//! started it. The same mapping serves a live session, the reconciliation of a lost turn and
//! the import of a stored conversation.
use crate::{Shared, provider::Streamed};
use anyhow::Result;
use kodem_core::*;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

const TEXT_LIMIT: usize = 1024 * 1024;
const DIFF_LINES: usize = 2000;
const DIFF_LINE_CHARS: usize = 2000;
/// The activity of a compaction that OpenCode runs, by its own limit or on request.
pub(super) const COMPACTING: &str = "opencode:compacting";
/// What OpenCode writes into the error of a tool call that the user declined.
const DECLINED: [&str; 3] = ["rejected", "denied", "declined"];

/// Chooses the icon and summary wording for an OpenCode tool.
pub(super) fn category(name: &str) -> ToolCategory {
    match name {
        "shell" | "bash" => ToolCategory::Command,
        "read" => ToolCategory::Read,
        "edit" | "write" | "patch" | "multiedit" | "apply_patch" => ToolCategory::Edit,
        "grep" | "glob" | "list" | "codesearch" | "lsp" => ToolCategory::Search,
        "webfetch" | "websearch" => ToolCategory::Fetch,
        "subagent" | "task" => ToolCategory::Agent,
        // OpenCode names the tools of an MCP server `<server>_<tool>`; built-in tools have no
        // underscore apart from `apply_patch`.
        _ if name.contains('_') => ToolCategory::Mcp,
        _ => ToolCategory::Other,
    }
}

// Questions have their own surface; a transcript row would repeat them.
fn silent(name: &str) -> bool {
    name == "question"
}

fn activity_kind(category: ToolCategory) -> ActivityKind {
    match category {
        ToolCategory::Command => ActivityKind::Command,
        ToolCategory::Read => ActivityKind::ReadingFiles,
        ToolCategory::Edit => ActivityKind::EditingFiles,
        ToolCategory::Search => ActivityKind::SearchingFiles,
        ToolCategory::Fetch => ActivityKind::Searching,
        ToolCategory::Agent => ActivityKind::Delegating,
        ToolCategory::Plan => ActivityKind::Planning,
        ToolCategory::Thinking => ActivityKind::Reasoning,
        ToolCategory::Mcp | ToolCategory::Other => ActivityKind::Tool,
    }
}

fn first_line(text: &str, limit: usize) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .chars()
        .take(limit)
        .collect()
}

fn bounded(text: &str) -> String {
    let mut take = text.len().min(TEXT_LIMIT);
    while !text.is_char_boundary(take) {
        take -= 1;
    }
    let mut kept = text[..take].to_owned();
    if take < text.len() {
        kept.push_str("\n[Output truncated at 1 MiB. Full output remains with the provider.]");
    }
    kept
}

// A thinking row is summarized by its first sentence, never by an invented label.
fn thought_title(text: &str) -> String {
    let line = first_line(text, 400);
    let sentence = line
        .split_inclusive(['.', '?', '!', ':'])
        .next()
        .unwrap_or_default()
        .trim()
        .trim_end_matches(['.', ':']);
    if sentence.is_empty() {
        "Thought".into()
    } else {
        sentence.chars().take(96).collect()
    }
}

fn text_field<'a>(input: &'a Value, names: &[&str]) -> &'a str {
    names
        .iter()
        .find_map(|name| input[*name].as_str())
        .unwrap_or_default()
}

// Returns `path` relative to the first checkout root that contains it.
fn relative(path: &str, roots: &[PathBuf]) -> String {
    roots
        .iter()
        .find_map(|root| Path::new(path).strip_prefix(root).ok())
        .filter(|path| !path.as_os_str().is_empty())
        .map_or_else(|| path.to_owned(), |path| path.display().to_string())
}

/// Returns a tool call's subject and its secondary context for the transcript summary.
pub(super) fn describe(name: &str, input: &Value, roots: &[PathBuf]) -> (String, String) {
    let (title, detail) = match name {
        "shell" | "bash" => (
            text_field(input, &["command"]).to_owned(),
            text_field(input, &["description"]).to_owned(),
        ),
        "read" => {
            let range = match (input["offset"].as_u64(), input["limit"].as_u64()) {
                (Some(offset), Some(limit)) => format!("lines {offset}–{}", offset + limit),
                (Some(offset), None) => format!("from line {offset}"),
                (None, Some(limit)) => format!("first {limit} lines"),
                (None, None) => String::new(),
            };
            (
                relative(text_field(input, &["path", "filePath"]), roots),
                range,
            )
        }
        "edit" | "write" | "patch" | "multiedit" => (
            relative(text_field(input, &["path", "filePath"]), roots),
            String::new(),
        ),
        "grep" | "glob" => (
            text_field(input, &["pattern"]).to_owned(),
            [
                input["path"].as_str().map(|path| relative(path, roots)),
                input["include"].as_str().map(str::to_owned),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
        ),
        "list" => (
            input["path"]
                .as_str()
                .map_or_else(|| ".".to_owned(), |path| relative(path, roots)),
            String::new(),
        ),
        "webfetch" => (text_field(input, &["url"]).to_owned(), String::new()),
        "websearch" | "codesearch" => (text_field(input, &["query"]).to_owned(), String::new()),
        "subagent" | "task" => (
            text_field(input, &["description"]).to_owned(),
            text_field(input, &["agent", "subagent_type"]).to_owned(),
        ),
        "skill" => (text_field(input, &["name", "id"]).to_owned(), String::new()),
        _ => (String::new(), String::new()),
    };
    let title = if title.trim().is_empty() {
        name.to_owned()
    } else {
        title
    };
    (first_line(&title, 400), first_line(&detail, 400))
}

fn diff_line(old: Option<u32>, new: Option<u32>, kind: LineKind, text: &str) -> DiffLine {
    DiffLine {
        old,
        new,
        kind,
        text: text.chars().take(DIFF_LINE_CHARS).collect(),
    }
}

// Reads the start and length of one side of a hunk header, such as `-12,3`.
fn hunk_side(text: &str) -> (u32, u32) {
    let mut parts = text[1..].split(',');
    let start = parts.next().and_then(|part| part.parse().ok()).unwrap_or(1);
    let length = parts.next().and_then(|part| part.parse().ok()).unwrap_or(1);
    (start, length)
}

/// Builds display lines and totals from a unified diff, as OpenCode's edit tools report it.
pub(super) fn unified_diff(patch: &str) -> (Vec<DiffLine>, EditedLines) {
    let mut lines = Vec::new();
    let mut totals = EditedLines::default();
    // Lines of the current hunk that are still to come, on the old and the new side.
    let (mut old_left, mut new_left) = (0u32, 0u32);
    let (mut old, mut new) = (0u32, 0u32);
    for line in patch.lines() {
        if old_left == 0 && new_left == 0 {
            // Between hunks only a hunk header matters; file headers and notes are skipped.
            let Some(header) = line.strip_prefix("@@ ") else {
                continue;
            };
            let mut sides = header.split_whitespace();
            let (Some(removed), Some(added)) = (sides.next(), sides.next()) else {
                continue;
            };
            (old, old_left) = hunk_side(removed);
            (new, new_left) = hunk_side(added);
            if lines.len() < DIFF_LINES {
                lines.push(diff_line(None, None, LineKind::Header, line));
            }
            continue;
        }
        let Some(marker) = line.chars().next() else {
            // An empty line is a context line whose own text is empty.
            old += 1;
            new += 1;
            old_left = old_left.saturating_sub(1);
            new_left = new_left.saturating_sub(1);
            if lines.len() < DIFF_LINES {
                lines.push(diff_line(
                    Some(old - 1),
                    Some(new - 1),
                    LineKind::Context,
                    "",
                ));
            }
            continue;
        };
        let text = &line[marker.len_utf8()..];
        let entry = match marker {
            '+' => {
                totals.added += 1;
                new += 1;
                new_left = new_left.saturating_sub(1);
                diff_line(None, Some(new - 1), LineKind::Added, text)
            }
            '-' => {
                totals.removed += 1;
                old += 1;
                old_left = old_left.saturating_sub(1);
                diff_line(Some(old - 1), None, LineKind::Removed, text)
            }
            ' ' => {
                old += 1;
                new += 1;
                old_left = old_left.saturating_sub(1);
                new_left = new_left.saturating_sub(1);
                diff_line(Some(old - 1), Some(new - 1), LineKind::Context, text)
            }
            // "\ No newline at end of file" describes the previous line.
            _ => continue,
        };
        if lines.len() < DIFF_LINES {
            lines.push(entry);
        }
    }
    (lines, totals)
}

// The lines of a file that a write created, as added lines.
fn created_file(content: &str) -> (Vec<DiffLine>, EditedLines) {
    let mut lines = Vec::new();
    let mut totals = EditedLines::default();
    for (index, line) in content.lines().enumerate() {
        totals.added += 1;
        if lines.len() < DIFF_LINES {
            lines.push(diff_line(
                None,
                Some(index as u32 + 1),
                LineKind::Added,
                line,
            ));
        }
    }
    (lines, totals)
}

/// Returns the reason of an error that OpenCode reports for a run, a step or a tool call.
pub(super) fn error_text(error: &Value) -> Option<String> {
    if error.is_null() {
        return None;
    }
    let message = error["message"]
        .as_str()
        .or_else(|| error["type"].as_str())
        .unwrap_or("OpenCode reported an error");
    Some(message.chars().take(4000).collect())
}

// The text of a tool result: OpenCode returns it as a list of content items.
fn content_text(content: &Value) -> String {
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "text")
        .filter_map(|item| item["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

// The context that a model request occupied: everything it read and everything it wrote.
fn occupancy(tokens: &Value) -> Option<u64> {
    let used: u64 = [
        &tokens["input"],
        &tokens["output"],
        &tokens["reasoning"],
        &tokens["cache"]["read"],
        &tokens["cache"]["write"],
    ]
    .into_iter()
    .filter_map(Value::as_u64)
    .sum();
    (used > 0).then_some(used)
}

/// How a tool call ended.
enum Ending<'a> {
    Success {
        content: &'a Value,
        metadata: &'a Value,
    },
    Failure {
        error: &'a Value,
    },
}

/// Streaming state of one task's conversation and the conversations of its subagents.
pub(super) struct Transcript {
    task_id: String,
    roots: Vec<PathBuf>,
    /// The session of the task. The others are the sessions of its agents.
    main: Option<String>,
    /// Transcript of each agent session, by session.
    agents: HashMap<String, String>,
    /// Open text and reasoning rows, by key.
    rows: Streamed,
    /// Tool calls that have not finished, by key, with their input.
    tools: HashMap<String, (Message, Value)>,
    /// Writes the user's text. A live session stores its own user rows; an import does not.
    prompts: bool,
    /// Context occupancy of the main session's latest finished request, in tokens.
    pub(super) used: Option<u64>,
    /// The model of the main session's latest request, as `provider/model`.
    pub(super) model: Option<String>,
}

impl Transcript {
    /// Starts with no open item. `prompts` writes the user's text as rows, for an import.
    pub(super) fn new(task_id: &str, cwd: &Path, prompts: bool) -> Self {
        let mut roots = vec![cwd.to_owned()];
        // OpenCode reports canonical paths, which differ from the stored checkout under symlinks.
        if let Ok(canonical) = cwd.canonicalize().map(kodem_core::platform::plain_path)
            && canonical != cwd
        {
            roots.push(canonical);
        }
        Self {
            task_id: task_id.into(),
            roots,
            main: None,
            agents: HashMap::new(),
            rows: Streamed::new(),
            tools: HashMap::new(),
            prompts,
            used: None,
            model: None,
        }
    }

    /// Names the session of the task, whose rows go into the task's own transcript.
    pub(super) fn set_main(&mut self, session: &str) {
        self.main = Some(session.to_owned());
    }

    /// Reports the session of an agent of this task.
    pub(super) fn is_agent(&self, session: &str) -> bool {
        self.agents.contains_key(session)
    }

    /// Returns the checkout roots used to shorten paths.
    pub(super) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    // Returns the transcript of a session, or `None` for a session of another task.
    fn destination(&self, session: &str) -> Option<String> {
        if self.main.as_deref() == Some(session) {
            return Some(self.task_id.clone());
        }
        self.agents.get(session).cloned()
    }

    fn main_session(&self, session: &str) -> bool {
        self.main.as_deref() == Some(session)
    }

    fn message(&self, transcript: &str, id: String, kind: MessageKind, created: u64) -> Message {
        Message {
            seq: 0,
            transfer: None,
            id,
            task_id: transcript.into(),
            kind,
            attachments: Vec::new(),
            feedback: Vec::new(),
            file_edits: Vec::new(),
            tool: None,
            text: String::new(),
            complete: false,
            created,
        }
    }

    fn activity(&self, host: &Shared, id: &str, kind: ActivityKind, detail: &str) {
        if self.prompts {
            return;
        }
        let mut h = host.lock().unwrap();
        let activities = h.activities.entry(self.task_id.clone()).or_default();
        if let Some(activity) = activities.iter_mut().find(|activity| activity.id == id) {
            activity.detail = first_line(detail, 100);
            return;
        }
        activities.push(Activity {
            id: id.into(),
            kind,
            detail: first_line(detail, 100),
            started: now(),
        });
    }

    /// Removes an activity of the task.
    pub(super) fn clear_activity(&self, host: &Shared, id: &str) {
        if let Some(activities) = host.lock().unwrap().activities.get_mut(&self.task_id) {
            activities.retain(|activity| activity.id != id);
        }
    }

    fn store(&self, host: &Shared, message: &Message) -> Result<()> {
        host.lock().unwrap().store.message(message)
    }

    /// Writes streamed text that waited for the write interval and that no later event followed.
    pub(super) fn write_due(&mut self, host: &Shared) -> Result<()> {
        if self.rows.due() {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        Ok(())
    }

    /// Applies one live event; events without transcript meaning are ignored.
    pub(super) fn ingest(&mut self, host: &Shared, event: &Value) -> Result<()> {
        let kind = event["type"].as_str().unwrap_or_default();
        // Every event but streamed text must see the rows of that text as they are now.
        if !kind.ends_with(".delta") && self.rows.pending() {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        let data = &event["data"];
        let created = event["created"].as_u64().unwrap_or_else(now);
        if kind == "session.created" {
            return self.session(host, data);
        }
        let Some(session) = data["sessionID"].as_str() else {
            return Ok(());
        };
        let message = data["assistantMessageID"].as_str().unwrap_or_default();
        let ordinal = data["ordinal"].as_u64().unwrap_or_default();
        let call = data["id"].as_str().unwrap_or_default();
        match kind {
            "session.text.started" | "session.reasoning.started" => {
                let thinking = kind == "session.reasoning.started";
                self.text(
                    host, session, message, ordinal, thinking, None, created, None,
                )?;
            }
            "session.text.delta" | "session.reasoning.delta" => {
                let thinking = kind == "session.reasoning.delta";
                if let Some(delta) = data["delta"].as_str() {
                    self.delta(host, &text_key(message, ordinal, thinking), delta)?;
                }
            }
            "session.text.ended" | "session.reasoning.ended" => {
                let thinking = kind == "session.reasoning.ended";
                let text = data["text"].as_str().unwrap_or_default();
                self.text(
                    host,
                    session,
                    message,
                    ordinal,
                    thinking,
                    Some(text),
                    created,
                    Some(created),
                )?;
            }
            "session.tool.input.started" => {
                let name = data["name"].as_str().unwrap_or("tool");
                self.tool_started(host, session, call, name, &Value::Null, created)?;
            }
            "session.tool.called" => {
                let name = self.tool_name(call);
                self.tool_started(host, session, call, &name, &data["input"], created)?;
            }
            "session.tool.progress" => {
                // A subagent's call names the session that the agent works in.
                if let Some(agent) = data["metadata"]["sessionID"].as_str() {
                    self.agent_of_call(host, session, call, agent)?;
                }
            }
            "session.tool.success" => self.tool_ended(
                host,
                session,
                call,
                Ending::Success {
                    content: &data["content"],
                    metadata: &data["metadata"],
                },
                created,
            )?,
            "session.tool.failed" => self.tool_ended(
                host,
                session,
                call,
                Ending::Failure {
                    error: &data["error"],
                },
                created,
            )?,
            "session.step.started" if self.main_session(session) => {
                let model = &data["model"];
                if let (Some(provider), Some(id)) =
                    (model["providerID"].as_str(), model["id"].as_str())
                {
                    self.model = Some(format!("{provider}/{id}"));
                }
            }
            "session.step.ended" | "session.step.failed" if self.main_session(session) => {
                if let Some(used) = occupancy(&data["tokens"]) {
                    self.used = Some(used);
                }
            }
            "session.compaction.started" if self.main_session(session) => {
                let id = data["inputID"].as_str().unwrap_or("latest");
                self.compacted(host, id, created)?;
                self.activity(host, COMPACTING, ActivityKind::Compacting, "");
            }
            "session.execution.succeeded" if self.is_agent(session) => {
                self.agent_finished(host, session, RunState::Completed)?;
            }
            "session.execution.failed" if self.is_agent(session) => {
                self.agent_finished(host, session, RunState::Failed)?;
            }
            "session.execution.interrupted" if self.is_agent(session) => {
                self.agent_finished(host, session, RunState::Interrupted)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Applies one stored message of a session, as the events that wrote it would.
    pub(super) fn replay(&mut self, host: &Shared, session: &str, message: &Value) -> Result<()> {
        let Some(id) = message["id"].as_str() else {
            return Ok(());
        };
        let created = message["time"]["created"].as_u64().unwrap_or_else(now);
        match message["type"].as_str() {
            // An agent's prompt is written from the call that started the agent.
            Some("user") if self.prompts && self.main_session(session) => {
                let text = message["text"].as_str().unwrap_or_default();
                if text.trim().is_empty() {
                    return Ok(());
                }
                let mut row = self.message(
                    &self.task_id,
                    format!("opencode:{id}"),
                    MessageKind::User,
                    created,
                );
                row.text = bounded(text);
                row.complete = true;
                self.store(host, &row)?;
            }
            Some("assistant") => {
                let (mut texts, mut thoughts) = (0, 0);
                for item in message["content"].as_array().into_iter().flatten() {
                    match item["type"].as_str() {
                        Some(kind @ ("text" | "reasoning")) => {
                            let thinking = kind == "reasoning";
                            let ordinal = if thinking { &mut thoughts } else { &mut texts };
                            let started = item["time"]["created"].as_u64().unwrap_or(created);
                            let ended = item["time"]["completed"].as_u64().unwrap_or(started);
                            self.text(
                                host,
                                session,
                                id,
                                *ordinal,
                                thinking,
                                Some(item["text"].as_str().unwrap_or_default()),
                                started,
                                Some(ended),
                            )?;
                            *ordinal += 1;
                        }
                        Some("tool") => self.replay_tool(host, session, item, created)?,
                        _ => {}
                    }
                }
                if self.main_session(session) {
                    let model = &message["model"];
                    if let (Some(provider), Some(model)) =
                        (model["providerID"].as_str(), model["id"].as_str())
                    {
                        self.model = Some(format!("{provider}/{model}"));
                    }
                    if let Some(used) = occupancy(&message["tokens"]) {
                        self.used = Some(used);
                    }
                }
            }
            Some("compaction")
                if self.main_session(session) && message["status"] == "completed" =>
            {
                self.compacted(host, id, created)?;
                if let Some(summary) = message["tokens"]["output"]
                    .as_u64()
                    .filter(|tokens| *tokens > 0)
                {
                    self.used = Some(summary);
                }
            }
            _ => {}
        }
        Ok(())
    }

    // Applies a stored tool call: its start, its input and its end when it ended.
    fn replay_tool(
        &mut self,
        host: &Shared,
        session: &str,
        item: &Value,
        created: u64,
    ) -> Result<()> {
        let call = item["id"].as_str().unwrap_or_default();
        let name = item["name"].as_str().unwrap_or("tool");
        let state = &item["state"];
        let started = item["time"]["created"].as_u64().unwrap_or(created);
        self.tool_started(host, session, call, name, &state["input"], started)?;
        if name == "subagent"
            && let Some(agent) = state["metadata"]["sessionID"].as_str()
        {
            self.agent_of_call(host, session, call, agent)?;
        }
        let ended = item["time"]["completed"].as_u64().unwrap_or(started);
        match state["status"].as_str() {
            Some("completed") => self.tool_ended(
                host,
                session,
                call,
                Ending::Success {
                    content: &state["content"],
                    metadata: &state["metadata"],
                },
                ended,
            ),
            Some("error") => self.tool_ended(
                host,
                session,
                call,
                Ending::Failure {
                    error: &state["error"],
                },
                ended,
            ),
            _ => Ok(()),
        }
    }

    /// Writes the notice of a compaction once.
    fn compacted(&mut self, host: &Shared, id: &str, created: u64) -> Result<()> {
        let mut notice = self.message(
            &self.task_id,
            format!("opencode:compact:{id}"),
            MessageKind::Notice,
            created,
        );
        notice.text = "Context compacted.".into();
        notice.complete = true;
        self.store(host, &notice)
    }

    // A session whose parent is the task's session or one of its agents is an agent.
    fn session(&mut self, host: &Shared, info: &Value) -> Result<()> {
        let (Some(id), Some(parent)) = (
            info["sessionID"].as_str().or_else(|| info["id"].as_str()),
            info["parentID"].as_str(),
        ) else {
            return Ok(());
        };
        if self.destination(parent).is_none() {
            return Ok(());
        }
        let name = info["title"].as_str();
        self.agent_started(host, id, parent, name, None)
    }

    // Registers the agent that a subagent call started, named by the call's description.
    fn agent_of_call(
        &mut self,
        host: &Shared,
        session: &str,
        call: &str,
        agent: &str,
    ) -> Result<()> {
        let input = self
            .tools
            .get(&tool_key(call))
            .map(|(_, input)| input.clone())
            .unwrap_or_default();
        self.agent_started(
            host,
            agent,
            session,
            input["description"].as_str(),
            input["prompt"].as_str(),
        )
    }

    // Registers an agent session once and links it from the transcript that started it.
    fn agent_started(
        &mut self,
        host: &Shared,
        session: &str,
        parent: &str,
        name: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<()> {
        let Some(parent_transcript) = self.destination(parent) else {
            return Ok(());
        };
        let transcript = format!("{}/agent/{session}", self.task_id);
        self.agents.insert(session.to_owned(), transcript.clone());
        let h = host.lock().unwrap();
        let mut task = h.store.task(&self.task_id)?;
        if !task
            .subagents
            .iter()
            .any(|known| known.thread_id == session)
        {
            let subagent = Subagent {
                // Agent items arrive as live events or with the stored conversation.
                history_loaded: true,
                thread_id: session.into(),
                parent_thread_id: parent.into(),
                name: name
                    .map(|name| first_line(name, 80))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("Agent {}", task.subagents.len() + 1)),
                state: RunState::Running,
                active_turn: None,
            };
            let mut link = self.message(
                &parent_transcript,
                format!("spawn:{session}"),
                MessageKind::Subagent,
                now(),
            );
            link.text = session.into();
            link.complete = true;
            h.store.message(&link)?;
            task.subagents.push(subagent);
            h.store.save_task(&task)?;
        }
        if let Some(prompt) = prompt.filter(|prompt| !prompt.trim().is_empty()) {
            let mut first = self.message(
                &transcript,
                format!("opencode:prompt:{session}"),
                MessageKind::User,
                now(),
            );
            first.text = bounded(prompt);
            first.complete = true;
            h.store.message(&first)?;
        }
        Ok(())
    }

    /// Records the outcome of an agent session that still worked.
    pub(super) fn agent_finished(
        &mut self,
        host: &Shared,
        session: &str,
        state: RunState,
    ) -> Result<()> {
        let h = host.lock().unwrap();
        let mut task = h.store.task(&self.task_id)?;
        let Some(known) = task
            .subagents
            .iter_mut()
            .find(|known| known.thread_id == session)
        else {
            return Ok(());
        };
        if !known.state.active() {
            return Ok(());
        }
        known.state = state;
        known.active_turn = None;
        h.store.save_task(&task)
    }

    // Opens or ends a text or reasoning row. `text` is the whole text when the item ended.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        host: &Shared,
        session: &str,
        message: &str,
        ordinal: u64,
        thinking: bool,
        text: Option<&str>,
        started: u64,
        ended: Option<u64>,
    ) -> Result<()> {
        let Some(transcript) = self.destination(session) else {
            return Ok(());
        };
        let main = self.main_session(session);
        let key = text_key(message, ordinal, thinking);
        let mut row = self.rows.rows.remove(&key).unwrap_or_else(|| {
            self.message(
                &transcript,
                key.clone(),
                if thinking {
                    MessageKind::Tool
                } else {
                    MessageKind::Assistant
                },
                started,
            )
        });
        if let Some(text) = text {
            row.text = bounded(text);
        }
        let finished = ended.is_some();
        if thinking {
            row.tool = Some(ToolCall {
                name: "Thinking".into(),
                category: ToolCategory::Thinking,
                title: thought_title(&row.text),
                status: if finished {
                    ToolStatus::Completed
                } else {
                    ToolStatus::Running
                },
                duration_ms: ended.map(|ended| ended.saturating_sub(row.created)),
                ..ToolCall::default()
            });
        }
        if finished {
            if main {
                self.clear_activity(host, &key);
            }
            row.complete = true;
            if !row.text.trim().is_empty() {
                self.store(host, &row)?;
            }
            return Ok(());
        }
        if main {
            let activity = if thinking {
                ActivityKind::Reasoning
            } else {
                ActivityKind::Responding
            };
            self.activity(host, &key, activity, "");
        }
        // An empty item is written once its first text arrives.
        self.rows.rows.insert(key, row);
        Ok(())
    }

    fn delta(&mut self, host: &Shared, key: &str, delta: &str) -> Result<()> {
        let Some(row) = self.rows.rows.get_mut(key) else {
            return Ok(());
        };
        if row.text.len() + delta.len() > TEXT_LIMIT {
            return Ok(());
        }
        row.text.push_str(delta);
        if let Some(tool) = &mut row.tool {
            tool.title = thought_title(&row.text);
        }
        if self.rows.changed(key) {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        Ok(())
    }

    // The name of a call that started earlier.
    fn tool_name(&self, call: &str) -> String {
        self.tools
            .get(&tool_key(call))
            .and_then(|(message, _)| message.tool.as_ref())
            .map_or_else(|| "tool".to_owned(), |tool| tool.name.clone())
    }

    // Opens a tool row, or describes it again once its input is known.
    fn tool_started(
        &mut self,
        host: &Shared,
        session: &str,
        call: &str,
        name: &str,
        input: &Value,
        started: u64,
    ) -> Result<()> {
        let Some(transcript) = self.destination(session) else {
            return Ok(());
        };
        let key = tool_key(call);
        let (mut message, known) = self.tools.remove(&key).unwrap_or_else(|| {
            (
                self.message(&transcript, key.clone(), MessageKind::Tool, started),
                Value::Null,
            )
        });
        let input = if input.is_null() {
            known
        } else {
            input.clone()
        };
        let (title, detail) = describe(name, &input, &self.roots);
        let call = ToolCall {
            name: name.into(),
            category: category(name),
            title: title.clone(),
            detail,
            ..ToolCall::default()
        };
        if self.main_session(session) {
            self.activity(host, &key, activity_kind(call.category), &title);
        }
        message.tool = Some(call);
        if !silent(name) {
            self.store(host, &message)?;
        }
        self.tools.insert(key, (message, input));
        Ok(())
    }

    // Ends a tool row with its output, its exit code and the changes it made.
    fn tool_ended(
        &mut self,
        host: &Shared,
        session: &str,
        call: &str,
        ending: Ending,
        ended: u64,
    ) -> Result<()> {
        let key = tool_key(call);
        if self.destination(session).is_none() {
            return Ok(());
        }
        if !self.tools.contains_key(&key) {
            self.tool_started(host, session, call, "tool", &Value::Null, ended)?;
        }
        let Some((mut message, input)) = self.tools.remove(&key) else {
            return Ok(());
        };
        if self.main_session(session) {
            self.clear_activity(host, &key);
        }
        let mut tool = message.tool.take().unwrap_or_default();
        tool.duration_ms = Some(ended.saturating_sub(message.created));
        match &ending {
            Ending::Success { content, metadata } => {
                tool.status = ToolStatus::Completed;
                message.text = bounded(&content_text(content));
                if tool.category == ToolCategory::Command {
                    tool.exit = metadata["exit"].as_i64();
                }
                if tool.category == ToolCategory::Edit {
                    let mut diff = Vec::new();
                    let mut edits = Vec::new();
                    for file in metadata["files"].as_array().into_iter().flatten() {
                        let (lines, mut totals) =
                            unified_diff(file["patch"].as_str().unwrap_or_default());
                        if let (Some(added), Some(removed)) =
                            (file["additions"].as_u64(), file["deletions"].as_u64())
                        {
                            totals = EditedLines {
                                added: added as usize,
                                removed: removed as usize,
                            };
                        }
                        diff.extend(
                            lines
                                .into_iter()
                                .take(DIFF_LINES - diff.len().min(DIFF_LINES)),
                        );
                        edits.push(FileEdit {
                            path: relative(file["file"].as_str().unwrap_or_default(), &self.roots),
                            lines: Some(totals),
                        });
                    }
                    // A file that a write created reports no patch.
                    if edits.is_empty() && tool.name == "write" {
                        let (lines, totals) =
                            created_file(input["content"].as_str().unwrap_or_default());
                        diff = lines;
                        edits.push(FileEdit {
                            path: tool.title.clone(),
                            lines: Some(totals),
                        });
                    }
                    tool.diff = diff;
                    message.file_edits = edits;
                }
            }
            Ending::Failure { error } => {
                let reason = error_text(error).unwrap_or_else(|| "The tool failed".into());
                let kind = error["type"].as_str().unwrap_or_default().to_lowercase();
                let lowered = reason.to_lowercase();
                tool.status = if kind == "aborted" {
                    ToolStatus::Failed
                } else if DECLINED
                    .iter()
                    .any(|phrase| kind.contains(phrase) || lowered.contains(phrase))
                {
                    ToolStatus::Declined
                } else {
                    ToolStatus::Failed
                };
                message.text = bounded(&reason);
            }
        }
        let hidden = silent(&tool.name);
        // A live session writes the user's answers when the user gives them; an import has
        // only the stored call.
        if self.prompts
            && tool.name == "question"
            && tool.status == ToolStatus::Completed
            && let Ending::Success { metadata, .. } = &ending
        {
            message.kind = MessageKind::User;
            message.id = format!("opencode:answer:{call}");
            message.text = answers(&input, metadata);
            message.complete = true;
            if !message.text.is_empty() {
                self.store(host, &message)?;
            }
            return Ok(());
        }
        message.tool = Some(tool);
        message.complete = true;
        if hidden {
            return Ok(());
        }
        self.store(host, &message)
    }

    /// Closes the open items of a session that stopped, such as the task's after its turn.
    /// A tool call that never reported its result ends as failed.
    pub(super) fn finish(&mut self, host: &Shared) -> Result<()> {
        if self.rows.pending() {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        let open: Vec<String> = self.rows.rows.keys().cloned().collect();
        for key in open {
            self.clear_activity(host, &key);
            if let Some(mut message) = self.rows.rows.remove(&key) {
                message.complete = true;
                if let Some(tool) = &mut message.tool {
                    tool.status = ToolStatus::Completed;
                }
                if !message.text.trim().is_empty() {
                    self.store(host, &message)?;
                }
            }
        }
        let tools: Vec<String> = self.tools.keys().cloned().collect();
        for key in tools {
            self.clear_activity(host, &key);
            if let Some((mut message, _)) = self.tools.remove(&key) {
                message.complete = true;
                let hidden = message.tool.as_ref().is_some_and(|tool| silent(&tool.name));
                if let Some(tool) = &mut message.tool
                    && tool.status == ToolStatus::Running
                {
                    tool.status = ToolStatus::Failed;
                }
                if !hidden {
                    self.store(host, &message)?;
                }
            }
        }
        Ok(())
    }

    /// Marks the agents that did not report an outcome as stopped.
    pub(super) fn stop_agents(&mut self, host: &Shared) -> Result<()> {
        let active: Vec<String> = host
            .lock()
            .unwrap()
            .store
            .task(&self.task_id)?
            .subagents
            .iter()
            .filter(|agent| agent.state.active())
            .map(|agent| agent.thread_id.clone())
            .collect();
        for agent in active {
            self.agent_finished(host, &agent, RunState::Interrupted)?;
        }
        Ok(())
    }
}

// The questions of a stored question call with the user's answers, as a live answer reads.
fn answers(input: &Value, metadata: &Value) -> String {
    input["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, question)| {
            let chosen: Vec<&str> = metadata["answers"][index]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let asked = question["question"].as_str()?;
            (!chosen.is_empty()).then(|| {
                let quoted = asked
                    .lines()
                    .map(|line| format!("> {line}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{quoted}\n\n**{}**", chosen.join(", "))
            })
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

// The row of a text or reasoning item of an assistant message.
fn text_key(message: &str, ordinal: u64, thinking: bool) -> String {
    let kind = if thinking { "reasoning" } else { "text" };
    format!("opencode:{message}:{kind}:{ordinal}")
}

// The row of a tool call.
fn tool_key(call: &str) -> String {
    format!("opencode:tool:{call}")
}
