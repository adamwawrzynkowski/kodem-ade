//! Maps Claude Code stream events onto Kodem transcripts, tool calls, plans and subagents.
use crate::{Shared, provider::Streamed};
use anyhow::Result;
use kodem_core::*;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

const TEXT_LIMIT: usize = 1024 * 1024;
const DIFF_LINES: usize = 2000;
const DIFF_LINE_CHARS: usize = 2000;
const COMPACTING: &str = "claude:compacting";
const RETRYING: &str = "claude:retry";

/// Chooses the icon and summary wording for a Claude Code tool.
pub(super) fn category(name: &str) -> ToolCategory {
    match name {
        "Bash" | "BashOutput" | "KillShell" | "KillBash" | "PowerShell" | "Monitor" | "REPL" => {
            ToolCategory::Command
        }
        "Read" | "NotebookRead" => ToolCategory::Read,
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => ToolCategory::Edit,
        "Grep" | "Glob" | "LS" | "ToolSearch" => ToolCategory::Search,
        "WebFetch" | "WebSearch" => ToolCategory::Fetch,
        "Task" | "Agent" => ToolCategory::Agent,
        "TodoWrite" | "EnterPlanMode" | "ExitPlanMode" | "TaskCreate" | "TaskUpdate"
        | "TaskList" | "TaskGet" => ToolCategory::Plan,
        _ if name.starts_with("mcp__") => ToolCategory::Mcp,
        _ => ToolCategory::Other,
    }
}

// Step lists and questions have their own surfaces; a transcript row would repeat them.
fn silent(name: &str) -> bool {
    matches!(
        name,
        "TodoWrite" | "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet" | "AskUserQuestion"
    )
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

fn bounded(text: &str, limit: usize) -> String {
    let mut take = text.len().min(limit);
    while !text.is_char_boundary(take) {
        take -= 1;
    }
    text[..take].to_owned()
}

fn append_bounded(target: &mut String, text: &str) {
    if target.len() >= TEXT_LIMIT {
        return;
    }
    let mut take = text.len().min(TEXT_LIMIT - target.len());
    while !text.is_char_boundary(take) {
        take -= 1;
    }
    target.push_str(&text[..take]);
    if take < text.len() {
        target.push_str("\n[Output truncated at 1 MiB. Full output remains with the provider.]");
    }
}

fn text_field<'a>(input: &'a Value, names: &[&str]) -> &'a str {
    names
        .iter()
        .find_map(|name| input[*name].as_str())
        .unwrap_or_default()
}

/// Returns a tool call's subject and its secondary context for the transcript summary.
pub(super) fn describe(name: &str, input: &Value, roots: &[PathBuf]) -> (String, String) {
    let relative = |path: &str| {
        roots
            .iter()
            .find_map(|root| Path::new(path).strip_prefix(root).ok())
            .filter(|path| !path.as_os_str().is_empty())
            .map_or_else(|| path.to_owned(), |path| path.display().to_string())
    };
    let (title, detail) = match name {
        "Bash" | "PowerShell" | "Monitor" | "REPL" => (
            text_field(input, &["command", "code"]).to_owned(),
            text_field(input, &["description"]).to_owned(),
        ),
        "BashOutput" | "KillShell" | "KillBash" => (
            text_field(input, &["bash_id", "shell_id", "task_id"]).to_owned(),
            String::new(),
        ),
        "Read" | "NotebookRead" => {
            let range = match (input["offset"].as_u64(), input["limit"].as_u64()) {
                (Some(offset), Some(limit)) => format!("lines {offset}–{}", offset + limit),
                (Some(offset), None) => format!("from line {offset}"),
                (None, Some(limit)) => format!("first {limit} lines"),
                (None, None) => String::new(),
            };
            (
                relative(text_field(input, &["file_path", "notebook_path"])),
                range,
            )
        }
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => (
            relative(text_field(input, &["file_path", "notebook_path"])),
            String::new(),
        ),
        "Grep" | "Glob" => (
            text_field(input, &["pattern"]).to_owned(),
            [
                input["path"].as_str().map(relative),
                input["glob"].as_str().map(str::to_owned),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
        ),
        "LS" => (relative(text_field(input, &["path"])), String::new()),
        "ToolSearch" | "WebSearch" => (text_field(input, &["query"]).to_owned(), String::new()),
        "WebFetch" => (
            text_field(input, &["url"]).to_owned(),
            first_line(text_field(input, &["prompt"]), 200),
        ),
        "Task" | "Agent" => (
            text_field(input, &["description"]).to_owned(),
            text_field(input, &["subagent_type"]).to_owned(),
        ),
        "Skill" => (
            text_field(input, &["skill", "command"]).to_owned(),
            text_field(input, &["args"]).to_owned(),
        ),
        "EnterPlanMode" => ("Enter plan mode".into(), String::new()),
        "ExitPlanMode" => ("Proposed plan".into(), String::new()),
        _ if name.starts_with("mcp__") => {
            let mut parts = name["mcp__".len()..].splitn(2, "__");
            let server = parts.next().unwrap_or_default();
            (parts.next().unwrap_or(server).to_owned(), server.to_owned())
        }
        _ => (
            input
                .as_object()
                .and_then(|fields| fields.values().find_map(Value::as_str))
                .unwrap_or_default()
                .to_owned(),
            String::new(),
        ),
    };
    (bounded(&title, 4000), bounded(&detail, 1000))
}

/// Renders a tool input for an approval prompt: the command, the path, or the raw arguments.
pub(super) fn render_input(name: &str, input: &Value, roots: &[PathBuf]) -> String {
    let (title, _) = describe(name, input, roots);
    if !title.is_empty() && !matches!(category(name), ToolCategory::Other | ToolCategory::Mcp) {
        return title;
    }
    if input.as_object().is_none_or(|fields| fields.is_empty()) {
        return name.to_owned();
    }
    serde_json::to_string_pretty(input)
        .unwrap_or_default()
        .chars()
        .take(4000)
        .collect()
}

fn result_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_owned();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| match block["type"].as_str() {
            Some("text") => block["text"].as_str().map(str::to_owned),
            Some("image") => Some("[image]".into()),
            Some("tool_reference") => block["tool_name"].as_str().map(str::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn diff_line(old: Option<u32>, new: Option<u32>, kind: LineKind, text: &str) -> DiffLine {
    DiffLine {
        old,
        new,
        kind,
        text: text.chars().take(DIFF_LINE_CHARS).collect(),
    }
}

// Builds display lines and totals from the CLI's structured patch; a created file has no hunks.
fn edit_diff(result: &Value) -> (Vec<DiffLine>, EditedLines) {
    let mut lines = Vec::new();
    let mut totals = EditedLines::default();
    let hunks = result["structuredPatch"].as_array();
    if hunks.is_none_or(|hunks| hunks.is_empty()) {
        if result["type"] == "create"
            && let Some(content) = result["content"].as_str()
        {
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
        }
        return (lines, totals);
    }
    for hunk in hunks.into_iter().flatten() {
        let mut old = hunk["oldStart"].as_u64().unwrap_or(1) as u32;
        let mut new = hunk["newStart"].as_u64().unwrap_or(1) as u32;
        if lines.len() < DIFF_LINES {
            lines.push(diff_line(
                None,
                None,
                LineKind::Header,
                &format!(
                    "@@ -{old},{} +{new},{} @@",
                    hunk["oldLines"].as_u64().unwrap_or(0),
                    hunk["newLines"].as_u64().unwrap_or(0)
                ),
            ));
        }
        for line in hunk["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let Some(marker) = line.chars().next() else {
                continue;
            };
            let text = &line[marker.len_utf8()..];
            let entry = match marker {
                '+' => {
                    totals.added += 1;
                    new += 1;
                    diff_line(None, Some(new - 1), LineKind::Added, text)
                }
                '-' => {
                    totals.removed += 1;
                    old += 1;
                    diff_line(Some(old - 1), None, LineKind::Removed, text)
                }
                ' ' => {
                    old += 1;
                    new += 1;
                    diff_line(Some(old - 1), Some(new - 1), LineKind::Context, text)
                }
                // "\ No newline at end of file" describes the previous line.
                _ => continue,
            };
            if lines.len() < DIFF_LINES {
                lines.push(entry);
            }
        }
    }
    (lines, totals)
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

fn plan_status(status: &str) -> PlanStatus {
    match status {
        "in_progress" => PlanStatus::Active,
        "completed" => PlanStatus::Done,
        _ => PlanStatus::Pending,
    }
}

fn agent_state(status: &str) -> Option<RunState> {
    Some(match status {
        "completed" => RunState::Completed,
        "failed" | "error" => RunState::Failed,
        "killed" | "stopped" | "cancelled" => RunState::Interrupted,
        _ => return None,
    })
}

struct Open {
    index: u64,
    id: String,
    thinking: bool,
}

#[derive(Default)]
struct Scope {
    message: Option<String>,
    open: Option<Open>,
    /// Next block position per provider message, for blocks that arrive without stream events.
    blocks: HashMap<String, u64>,
}

struct Running {
    message: Message,
    started: u64,
    main: bool,
    input: Value,
}

/// Streaming state of one task's conversation and the conversations of its subagents.
pub(super) struct Transcript {
    task_id: String,
    roots: Vec<PathBuf>,
    scopes: HashMap<String, Scope>,
    /// Open text and thinking items, whose rows are written at a bounded rate.
    messages: Streamed,
    tools: HashMap<String, Running>,
    /// Launch calls of background agents, by agent, kept open until the agent reports.
    background: HashMap<String, Running>,
    /// Tool calls that already received their result in the current turn.
    finished: HashSet<String>,
    declined: HashSet<String>,
    /// Subagents already registered on the task, so their events need no further lookup.
    known: HashSet<String>,
    /// Transcript that holds each subagent launch, by the launching tool call.
    launches: HashMap<String, String>,
    /// Subagent identity by the tool call that launched it.
    agents: HashMap<String, String>,
    /// Provider step identities aligned with `Task.plan`, for incremental step updates.
    steps: Vec<String>,
    /// Token counts of the main conversation's latest model request.
    pub(super) usage: Option<Value>,
    pub(super) model: Option<String>,
    /// A background agent reported during this turn, so the CLI may continue on its own.
    pub(super) agent_reported: bool,
}

impl Transcript {
    /// Starts with no open items; identities come from the provider, so replays are idempotent.
    pub(super) fn new(task_id: &str, cwd: &Path) -> Self {
        let mut roots = vec![cwd.to_owned()];
        // The CLI reports canonical paths, which differ from the stored checkout under symlinks.
        if let Ok(canonical) = cwd.canonicalize().map(kodem_core::platform::plain_path)
            && canonical != cwd
        {
            roots.push(canonical);
        }
        Self {
            task_id: task_id.into(),
            roots,
            scopes: HashMap::new(),
            messages: Streamed::new(),
            tools: HashMap::new(),
            background: HashMap::new(),
            finished: HashSet::new(),
            declined: HashSet::new(),
            known: HashSet::new(),
            launches: HashMap::new(),
            agents: HashMap::new(),
            steps: Vec::new(),
            usage: None,
            model: None,
            agent_reported: false,
        }
    }

    /// Returns the checkout roots used to shorten paths in summaries and prompts.
    pub(super) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Remembers a refusal so the tool's result is shown as declined rather than failed.
    pub(super) fn decline(&mut self, tool_use_id: &str) {
        self.declined.insert(tool_use_id.to_owned());
    }

    fn message(&self, transcript: &str, id: String, kind: MessageKind) -> Message {
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
            created: now(),
        }
    }

    fn child_transcript(&self, agent: &str) -> String {
        format!("{}/agent/{agent}", self.task_id)
    }

    // Subagent events name their agent; older shapes only name the launching tool call.
    fn agent_of(&self, event: &Value) -> Option<String> {
        event["agent_id"].as_str().map(str::to_owned).or_else(|| {
            let parent = event["parent_tool_use_id"].as_str()?;
            Some(
                self.agents
                    .get(parent)
                    .cloned()
                    .unwrap_or_else(|| parent.to_owned()),
            )
        })
    }

    fn destination(&mut self, host: &Shared, event: &Value) -> Result<(String, String)> {
        let Some(agent) = self.agent_of(event) else {
            return Ok((String::new(), self.task_id.clone()));
        };
        self.agent_started(
            host,
            &agent,
            event["parent_tool_use_id"].as_str(),
            event["task_description"]
                .as_str()
                .or_else(|| event["subagent_type"].as_str()),
            None,
        )?;
        Ok((agent.clone(), self.child_transcript(&agent)))
    }

    fn activity(&self, host: &Shared, id: &str, kind: ActivityKind, detail: &str) {
        let mut h = host.lock().unwrap();
        let activities = h.activities.entry(self.task_id.clone()).or_default();
        activities.retain(|activity| activity.id != RETRYING);
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

    fn clear_activity(&self, host: &Shared, id: &str) {
        if let Some(activities) = host.lock().unwrap().activities.get_mut(&self.task_id) {
            activities.retain(|activity| activity.id != id);
        }
    }

    fn store(&self, host: &Shared, message: &Message) -> Result<()> {
        host.lock().unwrap().store.message(message)
    }

    /// Writes the rows of streamed text that changed, so the next event sees them as they are.
    pub(super) fn write_pending(&mut self, host: &Shared) -> Result<()> {
        if self.messages.pending() {
            self.messages.write(&host.lock().unwrap().store)?;
        }
        Ok(())
    }

    /// Writes streamed text that waited for the write interval and that no later event followed.
    pub(super) fn write_due(&mut self, host: &Shared) -> Result<()> {
        if self.messages.due() {
            self.messages.write(&host.lock().unwrap().store)?;
        }
        Ok(())
    }

    /// Applies one conversation event; events without transcript meaning are ignored.
    pub(super) fn ingest(&mut self, host: &Shared, event: &Value) -> Result<()> {
        match event["type"].as_str() {
            Some("stream_event") => self.stream(host, event),
            Some("assistant") => self.assistant(host, event),
            Some("user") => self.user(host, event),
            Some("system") => self.system(host, event),
            _ => Ok(()),
        }
    }

    fn stream(&mut self, host: &Shared, event: &Value) -> Result<()> {
        let (scope_key, transcript) = self.destination(host, event)?;
        let main = scope_key.is_empty();
        let inner = &event["event"];
        let index = inner["index"].as_u64().unwrap_or(0);
        match inner["type"].as_str() {
            Some("message_start") => {
                let scope = self.scopes.entry(scope_key).or_default();
                scope.message = inner["message"]["id"].as_str().map(str::to_owned);
                scope.open = None;
                if main {
                    self.model = inner["message"]["model"].as_str().map(str::to_owned);
                    self.clear_activity(host, RETRYING);
                }
            }
            Some("content_block_start") => {
                let block = &inner["content_block"];
                let provider_message = self
                    .scopes
                    .entry(scope_key.clone())
                    .or_default()
                    .message
                    .clone()
                    .unwrap_or_else(|| "unknown".into());
                let id = format!("claude:{provider_message}:{index}");
                let open = match block["type"].as_str() {
                    Some("text") => {
                        let message = self.message(&transcript, id.clone(), MessageKind::Assistant);
                        self.messages.rows.insert(id.clone(), message);
                        if main {
                            self.activity(host, &id, ActivityKind::Responding, "");
                        }
                        Some(Open {
                            index,
                            id,
                            thinking: false,
                        })
                    }
                    Some("thinking") => {
                        let mut message = self.message(&transcript, id.clone(), MessageKind::Tool);
                        message.tool = Some(ToolCall {
                            name: "Thinking".into(),
                            category: ToolCategory::Thinking,
                            title: "Thinking".into(),
                            ..ToolCall::default()
                        });
                        self.messages.rows.insert(id.clone(), message);
                        if main {
                            self.activity(host, &id, ActivityKind::Reasoning, "");
                        }
                        Some(Open {
                            index,
                            id,
                            thinking: true,
                        })
                    }
                    Some("tool_use") => {
                        if let (Some(tool), Some(name)) =
                            (block["id"].as_str(), block["name"].as_str())
                        {
                            self.tool(host, &transcript, main, tool, name, &block["input"])?;
                        }
                        None
                    }
                    _ => None,
                };
                self.scopes.entry(scope_key).or_default().open = open;
            }
            Some("content_block_delta") => {
                let delta = &inner["delta"];
                let text = match delta["type"].as_str() {
                    Some("text_delta") => delta["text"].as_str(),
                    Some("thinking_delta") => delta["thinking"].as_str(),
                    _ => None,
                };
                let open = self
                    .scopes
                    .get(&scope_key)
                    .and_then(|scope| scope.open.as_ref())
                    .filter(|open| open.index == index)
                    .map(|open| open.id.clone());
                if let (Some(text), Some(id)) = (text.filter(|text| !text.is_empty()), open)
                    && let Some(message) = self.messages.rows.get_mut(&id)
                {
                    append_bounded(&mut message.text, text);
                    if self.messages.changed(&id) {
                        self.messages.write(&host.lock().unwrap().store)?;
                    }
                }
            }
            Some("content_block_stop") => {
                let open = self
                    .scopes
                    .get_mut(&scope_key)
                    .and_then(|scope| scope.open.take_if(|open| open.index == index));
                if let Some(open) = open {
                    self.close(host, &open.id)?;
                }
            }
            Some("message_delta") if main && inner["usage"].is_object() => {
                self.usage = Some(inner["usage"].clone());
            }
            _ => {}
        }
        Ok(())
    }

    // Completes a streamed text or thinking item; one that never produced text leaves no row.
    fn close(&mut self, host: &Shared, id: &str) -> Result<()> {
        self.clear_activity(host, id);
        let Some(mut message) = self.messages.rows.remove(id) else {
            return Ok(());
        };
        if message.text.trim().is_empty() {
            return Ok(());
        }
        message.complete = true;
        if let Some(tool) = &mut message.tool {
            tool.status = ToolStatus::Completed;
            tool.title = thought_title(&message.text);
        }
        self.store(host, &message)
    }

    fn assistant(&mut self, host: &Shared, event: &Value) -> Result<()> {
        let (scope_key, transcript) = self.destination(host, event)?;
        let main = scope_key.is_empty();
        let provider_message = event["message"]["id"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned();
        for block in event["message"]["content"].as_array().into_iter().flatten() {
            let kind = block["type"].as_str().unwrap_or_default();
            if kind == "tool_use" {
                if let (Some(tool), Some(name)) = (block["id"].as_str(), block["name"].as_str()) {
                    self.tool(host, &transcript, main, tool, name, &block["input"])?;
                }
                self.advance(&scope_key, &provider_message, event);
                continue;
            }
            let thinking = kind == "thinking";
            let Some(text) = block[if thinking { "thinking" } else { "text" }].as_str() else {
                continue;
            };
            if !thinking && kind != "text" {
                continue;
            }
            let duration = event["thinking_duration_ms"].as_u64();
            let streaming = self
                .scopes
                .get(&scope_key)
                .filter(|scope| scope.message.as_deref() == Some(&provider_message));
            let streamed = streaming
                .and_then(|scope| scope.open.as_ref())
                .filter(|open| open.thinking == thinking)
                .map(|open| open.id.clone());
            // Stream events own this provider message; a block that already closed is stored.
            if streaming.is_some() && streamed.is_none() {
                continue;
            }
            if let Some(id) = streamed {
                // The complete block replaces deltas, which may have been truncated or lost.
                if let Some(message) = self.messages.rows.get_mut(&id) {
                    message.text.clear();
                    append_bounded(&mut message.text, text);
                    if let Some(tool) = &mut message.tool {
                        tool.duration_ms = duration.or(tool.duration_ms);
                    }
                    if !message.text.trim().is_empty() {
                        let message = message.clone();
                        self.store(host, &message)?;
                    }
                }
                continue;
            }
            let position = self.advance(&scope_key, &provider_message, event);
            if text.trim().is_empty() {
                continue;
            }
            let mut message = self.message(
                &transcript,
                format!("claude:{provider_message}:{position}"),
                if thinking {
                    MessageKind::Tool
                } else {
                    MessageKind::Assistant
                },
            );
            if thinking {
                message.tool = Some(ToolCall {
                    name: "Thinking".into(),
                    category: ToolCategory::Thinking,
                    title: thought_title(text),
                    status: ToolStatus::Completed,
                    duration_ms: duration,
                    ..ToolCall::default()
                });
            }
            append_bounded(&mut message.text, text);
            message.complete = true;
            self.store(host, &message)?;
        }
        Ok(())
    }

    // Returns this block's position within its provider message and moves to the next one.
    fn advance(&mut self, scope_key: &str, provider_message: &str, event: &Value) -> u64 {
        let next = self
            .scopes
            .entry(scope_key.to_owned())
            .or_default()
            .blocks
            .entry(provider_message.to_owned())
            .or_default();
        let position = event["apiBlockIndex"].as_u64().unwrap_or(*next);
        *next = position + 1;
        position
    }

    // Creates or updates a tool call. Input arrives empty first and complete shortly after.
    fn tool(
        &mut self,
        host: &Shared,
        transcript: &str,
        main: bool,
        tool_use_id: &str,
        name: &str,
        input: &Value,
    ) -> Result<()> {
        let id = format!("claude:tool:{tool_use_id}");
        let (title, detail) = describe(name, input, &self.roots);
        if !self.tools.contains_key(tool_use_id) {
            // A repeated announcement of a finished call must not reopen it as running.
            if self.finished.contains(tool_use_id) {
                return Ok(());
            }
            let mut message = self.message(transcript, id.clone(), MessageKind::Tool);
            message.tool = Some(ToolCall {
                name: name.into(),
                category: category(name),
                ..ToolCall::default()
            });
            self.tools.insert(
                tool_use_id.to_owned(),
                Running {
                    message,
                    started: now(),
                    main,
                    input: Value::Null,
                },
            );
            if category(name) == ToolCategory::Agent {
                self.launches
                    .insert(tool_use_id.to_owned(), transcript.to_owned());
            }
        }
        let complete = input.as_object().is_some_and(|fields| !fields.is_empty());
        let running = self.tools.get_mut(tool_use_id).unwrap();
        if complete {
            running.input = input.clone();
        }
        if let Some(tool) = &mut running.message.tool {
            tool.title = title.clone();
            tool.detail = detail;
        }
        if name == "ExitPlanMode"
            && let Some(plan) = input["plan"].as_str()
        {
            running.message.text = bounded(plan, TEXT_LIMIT);
        }
        let message = running.message.clone();
        if main {
            self.activity(host, &id, activity_kind(category(name)), &title);
        }
        if complete && name == "TodoWrite" {
            self.replace_plan(host, input)?;
        }
        if complete && name == "TaskUpdate" {
            self.update_step(host, input)?;
        }
        if silent(name) {
            return Ok(());
        }
        self.store(host, &message)
    }

    fn user(&mut self, host: &Shared, event: &Value) -> Result<()> {
        // Replays acknowledge host input and synthetic messages carry context for the model.
        if event["isReplay"] == true || event["isSynthetic"] == true {
            return Ok(());
        }
        for block in event["message"]["content"].as_array().into_iter().flatten() {
            if block["type"] == "tool_result"
                && let Some(tool_use_id) = block["tool_use_id"].as_str()
            {
                self.result(host, event, block, tool_use_id)?;
            }
        }
        Ok(())
    }

    fn result(
        &mut self,
        host: &Shared,
        event: &Value,
        block: &Value,
        tool_use_id: &str,
    ) -> Result<()> {
        let Some(mut running) = self.tools.remove(tool_use_id) else {
            return Ok(());
        };
        self.finished.insert(tool_use_id.to_owned());
        let structured = &event["tool_use_result"];
        let meta = event["tool_result_meta"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|meta| meta["id"] == tool_use_id);
        let refused = self.declined.remove(tool_use_id)
            || meta.is_some_and(|meta| {
                meta["non_execution_kind"] == "user-rejected"
                    || meta["permission_decision"]["decision"] == "reject"
            });
        let failed = block["is_error"] == true;
        let output = result_text(&block["content"]);
        let name = running
            .message
            .tool
            .as_ref()
            .map(|tool| tool.name.clone())
            .unwrap_or_default();
        if !failed && name == "TaskCreate" {
            self.add_step(host, &running.input, structured)?;
        }
        if category(&name) == ToolCategory::Agent && !refused {
            let agent = structured["agentId"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| self.agents.get(tool_use_id).cloned());
            if let Some(agent) = agent {
                self.agent_started(
                    host,
                    &agent,
                    Some(tool_use_id),
                    running.input["description"].as_str(),
                    running.input["prompt"].as_str(),
                )?;
                if structured["isAsync"] == true {
                    // A background launch returns at once with internal metadata. The call stays
                    // open and receives the agent's own report when it finishes.
                    self.background.insert(agent, running);
                    return Ok(());
                }
                self.agent_finished(
                    host,
                    &agent,
                    if failed {
                        RunState::Failed
                    } else {
                        RunState::Completed
                    },
                    None,
                )?;
            }
        }
        self.clear_activity(host, &running.message.id);
        if silent(&name) {
            return Ok(());
        }
        let message = &mut running.message;
        if let Some(tool) = &mut message.tool {
            tool.status = if refused {
                ToolStatus::Declined
            } else if failed {
                ToolStatus::Failed
            } else {
                ToolStatus::Completed
            };
            tool.duration_ms = Some(now().saturating_sub(running.started));
            if tool.category == ToolCategory::Command && failed && !refused {
                tool.exit = output
                    .strip_prefix("Exit code ")
                    .and_then(|rest| rest.lines().next())
                    .and_then(|code| code.trim().parse().ok());
            }
            if tool.category == ToolCategory::Edit && !failed && !refused {
                let (diff, lines) = edit_diff(structured);
                tool.diff = diff;
                message.file_edits = vec![FileEdit {
                    path: tool.title.clone(),
                    lines: Some(lines),
                }];
            }
        }
        // The plan stays visible as the item's content; the CLI's acknowledgement adds nothing.
        if name != "ExitPlanMode" || message.text.is_empty() {
            message.text.clear();
            append_bounded(&mut message.text, &output);
        }
        message.complete = true;
        let message = message.clone();
        self.store(host, &message)
    }

    fn system(&mut self, host: &Shared, event: &Value) -> Result<()> {
        match event["subtype"].as_str() {
            Some("status") => {
                if event["status"] == "compacting" {
                    self.activity(host, COMPACTING, ActivityKind::Compacting, "");
                } else if event.get("status").is_some() {
                    self.clear_activity(host, COMPACTING);
                }
            }
            Some("compact_boundary") => {
                self.clear_activity(host, COMPACTING);
                let metadata = &event["compact_metadata"];
                let mut message = self.message(
                    &self.task_id,
                    format!(
                        "claude:compact:{}",
                        event["uuid"].as_str().unwrap_or("boundary")
                    ),
                    MessageKind::Notice,
                );
                message.text = match (
                    metadata["pre_tokens"].as_u64(),
                    metadata["post_tokens"].as_u64(),
                ) {
                    (Some(before), Some(after)) => {
                        format!("Context compacted from {before} to {after} tokens.")
                    }
                    _ => "Context compacted.".into(),
                };
                message.complete = true;
                self.store(host, &message)?;
            }
            Some("api_retry") => {
                let detail = match (event["attempt"].as_u64(), event["max_retries"].as_u64()) {
                    (Some(attempt), Some(limit)) => {
                        format!("Retrying the request, attempt {attempt} of {limit}")
                    }
                    _ => "Retrying the request".into(),
                };
                let mut h = host.lock().unwrap();
                let activities = h.activities.entry(self.task_id.clone()).or_default();
                let started = activities
                    .iter()
                    .find(|activity| activity.id == RETRYING)
                    .map_or_else(now, |activity| activity.started);
                activities.retain(|activity| activity.id != RETRYING);
                activities.push(Activity {
                    id: RETRYING.into(),
                    kind: ActivityKind::Reconnecting,
                    detail,
                    started,
                });
            }
            Some("task_started") => {
                if let Some(agent) = event["task_id"].as_str()
                    && (event["task_type"] == "local_agent" || event["subagent_type"].is_string())
                {
                    self.agent_started(
                        host,
                        agent,
                        event["tool_use_id"].as_str(),
                        event["description"]
                            .as_str()
                            .or_else(|| event["subagent_type"].as_str()),
                        event["prompt"].as_str(),
                    )?;
                }
            }
            Some("task_updated" | "task_notification") => {
                let status = event["status"]
                    .as_str()
                    .or_else(|| event["patch"]["status"].as_str())
                    .unwrap_or_default();
                if let (Some(agent), Some(state)) = (event["task_id"].as_str(), agent_state(status))
                {
                    self.agent_finished(host, agent, state, event["summary"].as_str())?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // Registers a subagent once and links it from the transcript that launched it.
    fn agent_started(
        &mut self,
        host: &Shared,
        agent: &str,
        tool_use_id: Option<&str>,
        name: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<()> {
        if let Some(tool_use_id) = tool_use_id {
            self.agents
                .entry(tool_use_id.to_owned())
                .or_insert_with(|| agent.to_owned());
        }
        if !self.known.insert(agent.to_owned()) {
            return Ok(());
        }
        let h = host.lock().unwrap();
        let mut task = h.store.task(&self.task_id)?;
        if task.subagents.iter().any(|known| known.thread_id == agent) {
            return Ok(());
        }
        let parent = tool_use_id
            .and_then(|tool_use_id| self.launches.get(tool_use_id))
            .cloned()
            .unwrap_or_else(|| self.task_id.clone());
        let parent_thread = parent
            .strip_prefix(&format!("{}/agent/", self.task_id))
            .map(str::to_owned)
            .or_else(|| task.provider_thread.clone())
            .unwrap_or_default();
        let subagent = Subagent {
            // Child items arrive only as live events; there is no history to fill in later.
            history_loaded: true,
            thread_id: agent.into(),
            parent_thread_id: parent_thread,
            name: name
                .map(|name| first_line(name, 80))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("Agent {}", task.subagents.len() + 1)),
            state: RunState::Running,
            active_turn: tool_use_id.map(str::to_owned),
        };
        let mut link = self.message(&parent, format!("spawn:{agent}"), MessageKind::Subagent);
        link.text = agent.into();
        link.complete = true;
        h.store.message(&link)?;
        if let Some(prompt) = prompt.filter(|prompt| !prompt.trim().is_empty()) {
            let mut first = self.message(
                &subagent.transcript_id(&self.task_id),
                format!("claude:prompt:{agent}"),
                MessageKind::User,
            );
            append_bounded(&mut first.text, prompt);
            first.complete = true;
            h.store.message(&first)?;
        }
        task.subagents.push(subagent);
        h.store.save_task(&task)
    }

    // Records an agent's outcome. Its report may arrive after the outcome, in a second event.
    fn agent_finished(
        &mut self,
        host: &Shared,
        agent: &str,
        state: RunState,
        report: Option<&str>,
    ) -> Result<()> {
        if let Some(running) = self.background.get_mut(agent) {
            let message = &mut running.message;
            if let Some(tool) = &mut message.tool {
                tool.status = if state == RunState::Completed {
                    ToolStatus::Completed
                } else {
                    ToolStatus::Failed
                };
                tool.duration_ms = Some(now().saturating_sub(running.started));
            }
            if let Some(report) = report {
                message.text.clear();
                append_bounded(&mut message.text, report);
            }
            message.complete = true;
            let message = message.clone();
            self.clear_activity(host, &message.id);
            self.store(host, &message)?;
            if report.is_some() || state == RunState::Interrupted {
                self.background.remove(agent);
            }
        }
        let transcript = self.child_transcript(agent);
        let open: Vec<String> = self
            .tools
            .iter()
            .filter(|(_, running)| running.message.task_id == transcript)
            .map(|(id, _)| id.clone())
            .collect();
        for id in open {
            if let Some(mut running) = self.tools.remove(&id) {
                self.settle(&mut running.message);
                self.store(host, &running.message)?;
            }
        }
        let h = host.lock().unwrap();
        let mut task = h.store.task(&self.task_id)?;
        let Some(known) = task
            .subagents
            .iter_mut()
            .find(|known| known.thread_id == agent)
        else {
            return Ok(());
        };
        if !known.state.active() {
            return Ok(());
        }
        known.state = state;
        known.active_turn = None;
        self.agent_reported = true;
        h.store.save_task(&task)
    }

    // An item that never received its result ends as failed rather than running forever.
    fn settle(&self, message: &mut Message) {
        message.complete = true;
        if let Some(tool) = &mut message.tool
            && tool.status == ToolStatus::Running
        {
            tool.status = if tool.category == ToolCategory::Thinking {
                ToolStatus::Completed
            } else {
                ToolStatus::Failed
            };
        }
    }

    /// Closes the main conversation's open items when its turn ends, leaving running agents alone.
    pub(super) fn finish(&mut self, host: &Shared) -> Result<()> {
        let main = self.task_id.clone();
        let open: Vec<String> = self
            .messages
            .rows
            .iter()
            .filter(|(_, message)| message.task_id == main)
            .map(|(id, _)| id.clone())
            .collect();
        for id in open {
            self.close(host, &id)?;
        }
        if let Some(scope) = self.scopes.get_mut("") {
            scope.open = None;
        }
        let tools: Vec<String> = self
            .tools
            .iter()
            .filter(|(_, running)| running.main)
            .map(|(id, _)| id.clone())
            .collect();
        for id in tools {
            if let Some(mut running) = self.tools.remove(&id) {
                let hidden = running
                    .message
                    .tool
                    .as_ref()
                    .is_some_and(|tool| silent(&tool.name));
                self.settle(&mut running.message);
                self.clear_activity(host, &running.message.id);
                if !hidden {
                    self.store(host, &running.message)?;
                }
            }
        }
        self.finished.clear();
        self.agent_reported = false;
        Ok(())
    }

    /// Marks agents that did not report an outcome after their conversation was stopped.
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
            self.agent_finished(host, &agent, RunState::Interrupted, None)?;
        }
        Ok(())
    }

    fn save_plan(&self, host: &Shared, plan: Vec<PlanStep>) -> Result<()> {
        let h = host.lock().unwrap();
        let mut task = h.store.task(&self.task_id)?;
        task.plan = plan;
        h.store.save_task(&task)
    }

    fn replace_plan(&mut self, host: &Shared, input: &Value) -> Result<()> {
        self.steps.clear();
        let plan = input["todos"]
            .as_array()
            .into_iter()
            .flatten()
            .take(200)
            .filter_map(|todo| {
                Some(PlanStep {
                    text: bounded(todo["content"].as_str()?, 2000),
                    status: plan_status(todo["status"].as_str().unwrap_or_default()),
                })
            })
            .collect();
        self.save_plan(host, plan)
    }

    fn add_step(&mut self, host: &Shared, input: &Value, result: &Value) -> Result<()> {
        let Some(subject) = input["subject"].as_str() else {
            return Ok(());
        };
        let identity = [&result["task"]["id"], &result["id"], &result["taskId"]]
            .into_iter()
            .find_map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| value.as_u64().map(|id| id.to_string()))
            })
            .unwrap_or_default();
        let mut plan = host.lock().unwrap().store.task(&self.task_id)?.plan;
        if plan.len() != self.steps.len() {
            // The stored list came from another source; start a list this session can address.
            plan.clear();
            self.steps.clear();
        }
        if plan.len() >= 200 {
            return Ok(());
        }
        plan.push(PlanStep {
            text: bounded(subject, 2000),
            status: PlanStatus::Pending,
        });
        self.steps.push(identity);
        self.save_plan(host, plan)
    }

    fn update_step(&mut self, host: &Shared, input: &Value) -> Result<()> {
        let identity = input["taskId"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| input["taskId"].as_u64().map(|id| id.to_string()));
        let Some(position) =
            identity.and_then(|identity| self.steps.iter().position(|step| step == &identity))
        else {
            return Ok(());
        };
        let mut plan = host.lock().unwrap().store.task(&self.task_id)?.plan;
        if plan.len() != self.steps.len() {
            return Ok(());
        }
        if input["status"] == "deleted" {
            plan.remove(position);
            self.steps.remove(position);
        } else {
            if let Some(status) = input["status"].as_str() {
                plan[position].status = plan_status(status);
            }
            if let Some(subject) = input["subject"].as_str() {
                plan[position].text = bounded(subject, 2000);
            }
        }
        self.save_plan(host, plan)
    }
}
