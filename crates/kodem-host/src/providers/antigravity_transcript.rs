//! Maps the Antigravity CLI's stream events and stored steps onto Kodem transcripts, tool calls
//! and subagents.
//!
//! A live conversation reports its steps as `step_update` events: the text of a response as it
//! grows, the start and the end of each tool call, and the subagents that a call started. The
//! CLI also stores every step in the conversation's `transcript_full.jsonl`, with the thinking
//! and the full arguments that the stream leaves out. A stored response step holds the text, the
//! thinking and the tool calls of one model request, and the result of each call follows as a
//! step of its own. Both sources number steps alike, so both write the same rows. A subagent is
//! a conversation of its own that only the stored file reports, so its file is followed.
use crate::{Shared, provider::Streamed};
use anyhow::Result;
use kodem_core::{providers::antigravity, *};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const TEXT_LIMIT: usize = 1024 * 1024;
const DIFF_LINES: usize = 2000;
const DIFF_LINE_CHARS: usize = 2000;
/// Longest stored step that is read, in bytes. A longer line is skipped.
const LINE_LIMIT: u64 = 32 * 1024 * 1024;
/// How long a reserved thinking row waits for the stored step that holds its text.
const THINKING_WAIT: Duration = Duration::from_secs(15);
/// The activity between the end of a step and the start of the next one, while the model works.
const WORKING: &str = "antigravity:working";
/// The task list artifact, which holds the plan.
pub(super) const TASK_LIST: &str = "task.md";
/// Words of a tool error that mean the call was refused, not that it failed.
const DECLINED: [&str; 3] = ["declined", "denied", "rejected"];

/// Chooses the icon and summary wording for an Antigravity tool.
pub(super) fn category(name: &str) -> ToolCategory {
    match name {
        "run_command" | "send_command_input" | "command_status" => ToolCategory::Command,
        "read_file" | "view_file" | "view_file_outline" | "view_code_item" | "list_dir"
        | "read_resource" | "list_resources" => ToolCategory::Read,
        "grep_search" | "find_by_name" | "codebase_search" => ToolCategory::Search,
        "search_web" | "read_url_content" | "read_browser_page" | "open_browser_url" => {
            ToolCategory::Fetch
        }
        "invoke_subagent" | "define_subagent" | "manage_subagents" | "send_message"
        | "browser_subagent" => ToolCategory::Agent,
        "call_mcp_tool" => ToolCategory::Mcp,
        _ if edits(name) => ToolCategory::Edit,
        _ => ToolCategory::Other,
    }
}

/// Reports a tool that changes files.
pub(super) fn edits(name: &str) -> bool {
    matches!(
        name,
        "write_file"
            | "write_to_file"
            | "edit_file"
            | "replace_file_content"
            | "multi_replace_file_content"
            | "sed_file"
            | "notebook_edit"
            | "delete_file"
            | "move_file"
    )
}

// Questions and the plan have their own surfaces; a transcript row would repeat them. The plan
// is the task list artifact in the CLI's folder of the conversation, outside the project, so
// the call's title is its absolute path.
fn silent(call: &ToolCall) -> bool {
    call.name == "ask_question"
        || (edits(&call.name)
            && Path::new(&call.title).file_name() == Some(TASK_LIST.as_ref())
            && antigravity::data_dir()
                .is_ok_and(|data| Path::new(&call.title).starts_with(data.join("brain"))))
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

/// The argument names under which the CLI's tools name the file they work on.
const PATH_FIELDS: [&str; 6] = [
    "Path",
    "TargetFile",
    "AbsolutePath",
    "File",
    "DirectoryPath",
    "path",
];

/// Returns the file that a tool call works on, as the CLI named it.
pub(super) fn call_path(input: &Value) -> &str {
    text_field(input, &PATH_FIELDS)
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
    let summary = text_field(input, &["toolSummary"]).to_owned();
    let (title, detail) = match name {
        "run_command" => (text_field(input, &["CommandLine"]).to_owned(), summary),
        "send_command_input" | "command_status" => (
            text_field(input, &["Input", "CommandId"]).to_owned(),
            summary,
        ),
        "grep_search" | "find_by_name" | "codebase_search" => (
            text_field(input, &["Query", "Pattern", "SearchQuery"]).to_owned(),
            [
                text_field(input, &["SearchPath", "SearchDirectory", "DirectoryPath"]),
                text_field(input, &["Includes", "Extensions"]),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .map(|part| relative(part, roots))
            .collect::<Vec<_>>()
            .join(" · "),
        ),
        "search_web" => (text_field(input, &["query", "Query"]).to_owned(), summary),
        "read_url_content" | "open_browser_url" | "read_browser_page" => (
            text_field(input, &["Url", "url", "PageId"]).to_owned(),
            summary,
        ),
        "invoke_subagent" => (
            text_field(input, &["Role", "Prompt"]).to_owned(),
            text_field(input, &["TypeName"]).to_owned(),
        ),
        "send_message" => (
            text_field(input, &["Message"]).to_owned(),
            text_field(input, &["Recipient"]).to_owned(),
        ),
        "call_mcp_tool" => (
            format!(
                "{}/{}",
                text_field(input, &["ServerName"]),
                text_field(input, &["ToolName"])
            ),
            summary,
        ),
        _ if !call_path(input).is_empty() => (relative(call_path(input), roots), summary),
        _ => (summary, String::new()),
    };
    let title = if title.trim().is_empty() || title == "/" {
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

// The lines that one replacement removes and adds. The CLI reports the replaced text, not its
// position in the file, so the lines carry no numbers.
fn replacement(old: &str, new: &str, lines: &mut Vec<DiffLine>, totals: &mut EditedLines) {
    for line in old.lines() {
        totals.removed += 1;
        if lines.len() < DIFF_LINES {
            lines.push(diff_line(None, None, LineKind::Removed, line));
        }
    }
    for line in new.lines() {
        totals.added += 1;
        if lines.len() < DIFF_LINES {
            lines.push(diff_line(None, None, LineKind::Added, line));
        }
    }
}

/// Builds the display lines and totals of a call that changes a file from its arguments.
pub(super) fn edit_diff(name: &str, input: &Value) -> Option<(Vec<DiffLine>, EditedLines)> {
    let mut lines = Vec::new();
    let mut totals = EditedLines::default();
    match name {
        "write_file" | "write_to_file" => {
            let content = text_field(input, &["Content", "CodeContent"]);
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
        "edit_file" | "replace_file_content" => replacement(
            text_field(input, &["Target", "TargetContent"]),
            text_field(input, &["Replacement", "ReplacementContent"]),
            &mut lines,
            &mut totals,
        ),
        "multi_replace_file_content" => {
            for chunk in input["ReplacementChunks"].as_array().into_iter().flatten() {
                replacement(
                    text_field(chunk, &["TargetContent", "Target"]),
                    text_field(chunk, &["ReplacementContent", "Replacement"]),
                    &mut lines,
                    &mut totals,
                );
            }
        }
        _ => return None,
    }
    Some((lines, totals))
}

/// Removes the lines with which the CLI opens a stored tool result: when the call was made and
/// when it ended.
fn result_text(content: &str) -> &str {
    let mut rest = content;
    for prefix in ["Created At:", "Completed At:"] {
        if rest.starts_with(prefix) {
            rest = rest.split_once('\n').map_or("", |(_, rest)| rest);
        }
    }
    rest
}

// The exit code in the result of a command, as `Command exited with code 0.`.
fn exit_code(output: &str) -> Option<i64> {
    let rest = output.split("exited with code ").nth(1)?;
    rest.split(|character: char| !(character.is_ascii_digit() || character == '-'))
        .next()?
        .parse()
        .ok()
}

/// Returns the user's text of a stored prompt, without the context the CLI wraps around it.
pub(super) fn request_text(content: &str) -> &str {
    content
        .split_once("<USER_REQUEST>")
        .and_then(|(_, rest)| rest.split_once("</USER_REQUEST>"))
        .map_or(content, |(request, _)| request)
        .trim()
}

/// Returns the conversation that a stored `invoke_subagent` result names.
fn created_agent(content: &str) -> Option<&str> {
    let rest = content.split_once("\"conversationId\"")?.1;
    let start = rest.find('"')? + 1;
    let id = rest[start..].split('"').next()?;
    antigravity::valid_conversation(id).then_some(id)
}

// The context that a model request occupied: what it read, cached or not, and what it wrote.
fn occupancy(usage: &Value) -> u64 {
    [
        &usage["input_tokens"],
        &usage["cache_read_tokens"],
        &usage["output_tokens"],
    ]
    .into_iter()
    .filter_map(Value::as_u64)
    .sum()
}

/// How a tool call ended.
enum Ending<'a> {
    Success(&'a str),
    Failure(&'a str),
}

/// What the CLI writes before the reason of a call that a hook refused.
const HOOK_DENIAL: &str = "tool call denied by pre-tool hook: ";

// The time between the `Created At:` and `Completed At:` lines of a stored tool result.
fn stored_duration(content: &str) -> Option<u64> {
    let stamp = |prefix: &str| {
        content
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .and_then(|stamp| unix_millis(stamp.trim()))
    };
    Some(stamp("Completed At:")?.saturating_sub(stamp("Created At:")?))
}

/// What Kodem follows of a conversation's stored file.
#[derive(Default)]
struct Stored {
    /// Bytes of the file that were read.
    offset: u64,
    /// The step of the latest stored response, with the tool calls it made in order. The result
    /// of the n-th call is the step that follows the response by n + 1.
    calls: Option<(u64, Vec<Value>)>,
}

/// Streaming state of one task's conversation and the conversations of its subagents.
pub(super) struct Transcript {
    task_id: String,
    roots: Vec<PathBuf>,
    /// The conversation of the task. The others are its subagents.
    main: Option<String>,
    /// Transcript of each subagent conversation, by conversation.
    agents: HashMap<String, String>,
    /// Open text rows, by key.
    rows: Streamed,
    /// Tool calls that have not finished, by key.
    tools: HashMap<String, Message>,
    /// Full arguments of tool calls, by key: the stream reports only a part of them.
    arguments: HashMap<String, Value>,
    /// Reserved thinking rows of the task's conversation, by key, until the stored step holds
    /// their text.
    thoughts: HashMap<String, Instant>,
    stored: HashMap<String, Stored>,
    /// Writes the user's text and applies every stored step of the task's conversation, for an
    /// import or a reconciliation. A live session stores its own user rows.
    replaying: bool,
    /// Context occupancy of the task's latest finished request, in tokens.
    pub(super) used: Option<u64>,
}

impl Transcript {
    /// Starts with no open item. `replaying` writes the user's text and every stored step.
    pub(super) fn new(task_id: &str, cwd: &Path, replaying: bool) -> Self {
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
            main: None,
            agents: HashMap::new(),
            rows: Streamed::new(),
            tools: HashMap::new(),
            arguments: HashMap::new(),
            thoughts: HashMap::new(),
            stored: HashMap::new(),
            replaying,
            used: None,
        }
    }

    /// Names the conversation of the task, whose rows go into the task's own transcript. A
    /// live session reads its stored file from the end, because the stream reports the rest.
    pub(super) fn set_main(&mut self, conversation: &str) {
        if self.main.as_deref() == Some(conversation) {
            return;
        }
        self.main = Some(conversation.to_owned());
        if !self.replaying {
            let offset = antigravity::transcript(conversation)
                .ok()
                .and_then(|path| std::fs::metadata(path).ok())
                .map_or(0, |metadata| metadata.len());
            self.stored.insert(
                conversation.to_owned(),
                Stored {
                    offset,
                    calls: None,
                },
            );
        }
    }

    /// Reports a subagent conversation of this task.
    pub(super) fn is_agent(&self, conversation: &str) -> bool {
        self.agents.contains_key(conversation)
    }

    /// Returns the checkout roots used to shorten paths.
    pub(super) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    // Returns the transcript of a conversation, or `None` for a conversation of another task.
    fn destination(&self, conversation: &str) -> Option<String> {
        if self.main.as_deref() == Some(conversation) {
            return Some(self.task_id.clone());
        }
        self.agents.get(conversation).cloned()
    }

    fn is_main(&self, conversation: &str) -> bool {
        self.main.as_deref() == Some(conversation)
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
        if self.replaying {
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

    /// Shows that the model works on the task's next step.
    pub(super) fn working(&self, host: &Shared) {
        self.activity(host, WORKING, ActivityKind::Reasoning, "");
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

    /// Applies one `step_update` of the stream. Steps without transcript meaning are ignored.
    pub(super) fn update(&mut self, host: &Shared, update: &Value) -> Result<()> {
        let Some(conversation) = update["conversation_id"].as_str() else {
            return Ok(());
        };
        let Some(step) = update["step_index"].as_u64() else {
            return Ok(());
        };
        let done = update["state"] != "ACTIVE";
        // Every update but streamed text must see the rows of that text as they are now.
        if (done || update["step_type"] != "agent_response") && self.rows.pending() {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        let created = now();
        match update["step_type"].as_str().unwrap_or_default() {
            "agent_response" => {
                if self.is_main(conversation) {
                    self.clear_activity(host, WORKING);
                    self.reserve_thought(host, conversation, step, created)?;
                }
                if let Some(delta) = update["text_delta"].as_str() {
                    self.delta(host, conversation, step, delta, created)?;
                }
                if done {
                    self.text_ended(host, conversation, step)?;
                    if self.is_main(conversation) {
                        let usage = &update["usage"];
                        let used = occupancy(usage);
                        if used > 0 {
                            self.used = Some(used);
                        }
                        // A request that did not think leaves nothing for its reserved row.
                        if usage["thinking_tokens"].as_u64() == Some(0) {
                            self.drop_thought(host, &thought_key(conversation, step))?;
                        }
                    }
                }
            }
            "tool" | "subagent" => {
                let info = &update["tool_info"];
                let name = info["name"]
                    .as_str()
                    .or_else(|| update["tool_name"].as_str())
                    .unwrap_or("tool");
                if self.is_main(conversation) {
                    self.clear_activity(host, WORKING);
                }
                let key = tool_key(conversation, step);
                if !self.tools.contains_key(&key) {
                    self.tool_started(
                        host,
                        conversation,
                        step,
                        name,
                        &info["parameters"],
                        created,
                    )?;
                }
                for agent in update["subagent_info"]["subagents"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    if let Some(id) = agent["conversation_id"].as_str() {
                        self.agent_started(
                            host,
                            id,
                            conversation,
                            agent["role"].as_str(),
                            agent["initial_prompt"].as_str(),
                        )?;
                    }
                }
                let duration = update["duration_seconds"]
                    .as_f64()
                    .map(|seconds| (seconds * 1000.0) as u64);
                match update["state"].as_str() {
                    Some("DONE") => {
                        let output = info["output"].as_str().unwrap_or_default();
                        let ending = Ending::Success(output);
                        self.tool_ended(host, conversation, step, ending, duration)?;
                    }
                    Some("ERROR") => {
                        let error = info["error"]["message"]
                            .as_str()
                            .unwrap_or("The tool failed");
                        let ending = Ending::Failure(error);
                        self.tool_ended(host, conversation, step, ending, duration)?;
                    }
                    _ => {}
                }
                if done && self.is_main(conversation) {
                    self.working(host);
                }
            }
            "user_input" if done && self.is_main(conversation) => self.working(host),
            _ => {}
        }
        Ok(())
    }

    /// Records the full arguments of a tool call, as the approval hook received them. A row
    /// that is open shows them at once.
    pub(super) fn arguments(
        &mut self,
        host: &Shared,
        conversation: &str,
        step: u64,
        name: &str,
        arguments: &Value,
    ) -> Result<()> {
        if self.destination(conversation).is_none() {
            return Ok(());
        }
        let key = tool_key(conversation, step);
        self.arguments.insert(key.clone(), arguments.clone());
        if self.tools.contains_key(&key) {
            self.tool_started(host, conversation, step, name, arguments, now())?;
        }
        Ok(())
    }

    // Reserves the thinking row of a response before its text, so the thinking reads first. The
    // stored step fills it in.
    fn reserve_thought(
        &mut self,
        host: &Shared,
        conversation: &str,
        step: u64,
        created: u64,
    ) -> Result<()> {
        let key = thought_key(conversation, step);
        if self.thoughts.contains_key(&key) || self.replaying {
            return Ok(());
        }
        self.thoughts.insert(key.clone(), Instant::now());
        let mut row = self.message(&self.task_id, key, MessageKind::Tool, created);
        row.tool = Some(ToolCall {
            name: "Thinking".into(),
            category: ToolCategory::Thinking,
            title: "Thought".into(),
            ..ToolCall::default()
        });
        self.store(host, &row)
    }

    // Removes a reserved thinking row that no thinking filled.
    fn drop_thought(&mut self, host: &Shared, key: &str) -> Result<()> {
        if self.thoughts.remove(key).is_some() {
            host.lock()
                .unwrap()
                .store
                .remove_message(&self.task_id, key)?;
        }
        Ok(())
    }

    fn delta(
        &mut self,
        host: &Shared,
        conversation: &str,
        step: u64,
        delta: &str,
        created: u64,
    ) -> Result<()> {
        let Some(transcript) = self.destination(conversation) else {
            return Ok(());
        };
        let key = text_key(conversation, step);
        if !self.rows.rows.contains_key(&key) {
            if delta.trim().is_empty() {
                return Ok(());
            }
            let row = self.message(&transcript, key.clone(), MessageKind::Assistant, created);
            self.rows.rows.insert(key.clone(), row);
            if self.is_main(conversation) {
                self.activity(host, &key, ActivityKind::Responding, "");
            }
        }
        let Some(row) = self.rows.rows.get_mut(&key) else {
            return Ok(());
        };
        if row.text.len() + delta.len() > TEXT_LIMIT {
            return Ok(());
        }
        row.text.push_str(delta);
        if self.rows.changed(&key) {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        Ok(())
    }

    // Ends the text row of a response.
    fn text_ended(&mut self, host: &Shared, conversation: &str, step: u64) -> Result<()> {
        let key = text_key(conversation, step);
        self.clear_activity(host, &key);
        if let Some(mut row) = self.rows.rows.remove(&key) {
            row.text = bounded(row.text.trim_end());
            row.complete = true;
            if !row.text.is_empty() {
                self.store(host, &row)?;
            }
        }
        Ok(())
    }

    // Writes a finished text or thinking row of a stored response.
    fn stored_text(
        &mut self,
        host: &Shared,
        conversation: &str,
        key: String,
        text: &str,
        thinking: bool,
        created: u64,
    ) -> Result<()> {
        let Some(transcript) = self.destination(conversation) else {
            return Ok(());
        };
        if thinking {
            self.thoughts.remove(&key);
        }
        if text.trim().is_empty() {
            return Ok(());
        }
        let kind = if thinking {
            MessageKind::Tool
        } else {
            MessageKind::Assistant
        };
        let mut row = self.message(&transcript, key, kind, created);
        row.text = bounded(text.trim());
        row.complete = true;
        if thinking {
            row.tool = Some(ToolCall {
                name: "Thinking".into(),
                category: ToolCategory::Thinking,
                title: thought_title(&row.text),
                status: ToolStatus::Completed,
                ..ToolCall::default()
            });
        }
        self.store(host, &row)
    }

    // Opens a tool row, or describes it again once its full arguments are known.
    fn tool_started(
        &mut self,
        host: &Shared,
        conversation: &str,
        step: u64,
        name: &str,
        input: &Value,
        started: u64,
    ) -> Result<()> {
        let Some(transcript) = self.destination(conversation) else {
            return Ok(());
        };
        let key = tool_key(conversation, step);
        let input = self
            .arguments
            .get(&key)
            .cloned()
            .unwrap_or_else(|| input.clone());
        let mut message = self
            .tools
            .remove(&key)
            .unwrap_or_else(|| self.message(&transcript, key.clone(), MessageKind::Tool, started));
        let (title, detail) = describe(name, &input, &self.roots);
        let call = ToolCall {
            name: name.into(),
            category: category(name),
            title: title.clone(),
            detail,
            ..ToolCall::default()
        };
        if self.is_main(conversation) {
            self.activity(host, &key, activity_kind(call.category), &title);
        }
        let hidden = silent(&call);
        message.tool = Some(call);
        if !hidden {
            self.store(host, &message)?;
        }
        self.tools.insert(key, message);
        Ok(())
    }

    // Ends a tool row with its output, its exit code and the changes it made.
    fn tool_ended(
        &mut self,
        host: &Shared,
        conversation: &str,
        step: u64,
        ending: Ending,
        duration: Option<u64>,
    ) -> Result<()> {
        let key = tool_key(conversation, step);
        let Some(mut message) = self.tools.remove(&key) else {
            return Ok(());
        };
        if self.is_main(conversation) {
            self.clear_activity(host, &key);
        }
        let mut tool = message.tool.take().unwrap_or_default();
        tool.duration_ms = duration.or_else(|| Some(now().saturating_sub(message.created)));
        let input = self.arguments.remove(&key).unwrap_or_default();
        match ending {
            Ending::Success(output) => {
                tool.status = ToolStatus::Completed;
                message.text = bounded(output.trim());
                if tool.category == ToolCategory::Command {
                    tool.exit = exit_code(output);
                }
                if tool.category == ToolCategory::Edit
                    && let Some((diff, totals)) = edit_diff(&tool.name, &input)
                {
                    tool.diff = diff;
                    message.file_edits = vec![FileEdit {
                        path: tool.title.clone(),
                        lines: Some(totals),
                    }];
                }
            }
            Ending::Failure(error) => {
                let error = error.strip_prefix(HOOK_DENIAL).unwrap_or(error);
                let lowered = error.to_lowercase();
                tool.status = if DECLINED.iter().any(|phrase| lowered.contains(phrase)) {
                    ToolStatus::Declined
                } else {
                    ToolStatus::Failed
                };
                message.text = bounded(error);
            }
        }
        let hidden = silent(&tool);
        message.tool = Some(tool);
        message.complete = true;
        if hidden {
            return Ok(());
        }
        self.store(host, &message)
    }

    // Registers a subagent conversation once and links it from the transcript that started it.
    fn agent_started(
        &mut self,
        host: &Shared,
        conversation: &str,
        parent: &str,
        name: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<()> {
        let Some(parent_transcript) = self.destination(parent) else {
            return Ok(());
        };
        if !antigravity::valid_conversation(conversation) || self.is_agent(conversation) {
            return Ok(());
        }
        let transcript = format!("{}/agent/{conversation}", self.task_id);
        self.agents
            .insert(conversation.to_owned(), transcript.clone());
        self.stored.entry(conversation.to_owned()).or_default();
        let h = host.lock().unwrap();
        let mut task = h.store.task(&self.task_id)?;
        if !task
            .subagents
            .iter()
            .any(|known| known.thread_id == conversation)
        {
            let subagent = Subagent {
                // The agent's steps arrive from its stored file.
                history_loaded: true,
                thread_id: conversation.into(),
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
                format!("spawn:{conversation}"),
                MessageKind::Subagent,
                now(),
            );
            link.text = conversation.into();
            link.complete = true;
            h.store.message(&link)?;
            task.subagents.push(subagent);
            h.store.save_task(&task)?;
        }
        if let Some(prompt) = prompt.filter(|prompt| !prompt.trim().is_empty()) {
            let mut first = self.message(
                &transcript,
                format!("antigravity:prompt:{conversation}"),
                MessageKind::User,
                now(),
            );
            first.text = bounded(prompt);
            first.complete = true;
            h.store.message(&first)?;
        }
        Ok(())
    }

    // Records the state of a subagent; one that reports new steps works again.
    fn agent_state(&mut self, host: &Shared, conversation: &str, state: RunState) -> Result<()> {
        let h = host.lock().unwrap();
        let mut task = h.store.task(&self.task_id)?;
        let Some(known) = task
            .subagents
            .iter_mut()
            .find(|known| known.thread_id == conversation)
        else {
            return Ok(());
        };
        if known.state == state || known.state == RunState::Waiting {
            return Ok(());
        }
        known.state = state;
        known.active_turn = None;
        h.store.save_task(&task)
    }

    /// Reads the steps that the CLI stored since the last call: the thinking of the task's
    /// conversation and every step of its subagents.
    pub(super) fn follow(&mut self, host: &Shared) -> Result<()> {
        let conversations: Vec<String> = self.stored.keys().cloned().collect();
        for conversation in conversations {
            let Ok(path) = antigravity::transcript(&conversation) else {
                continue;
            };
            let Some(mut stored) = self.stored.remove(&conversation) else {
                continue;
            };
            let steps = read_steps(&path, &mut stored.offset);
            self.stored.insert(conversation.clone(), stored);
            for step in &steps {
                self.stored_step(host, &conversation, step)?;
            }
            // A subagent whose latest response made no call has answered and waits.
            if self.is_agent(&conversation)
                && let Some(last) = steps.last()
            {
                let answered = last["type"] == "PLANNER_RESPONSE"
                    && last["tool_calls"].as_array().is_none_or(Vec::is_empty);
                let state = if answered {
                    RunState::Completed
                } else {
                    RunState::Running
                };
                self.agent_state(host, &conversation, state)?;
            }
        }
        let expired: Vec<String> = self
            .thoughts
            .iter()
            .filter(|(_, reserved)| reserved.elapsed() >= THINKING_WAIT)
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired {
            self.drop_thought(host, &key)?;
        }
        Ok(())
    }

    /// Applies the whole stored file of a conversation, and of the subagents it started, as a
    /// replay of a stored conversation.
    pub(super) fn replay(&mut self, host: &Shared, conversation: &str) -> Result<()> {
        let path = antigravity::transcript(conversation)?;
        let mut stored = Stored::default();
        let steps = read_steps(&path, &mut stored.offset);
        self.stored.insert(conversation.to_owned(), stored);
        for step in &steps {
            self.stored_step(host, conversation, step)?;
        }
        let agents: Vec<String> = self
            .agents
            .keys()
            .filter(|agent| {
                self.stored
                    .get(*agent)
                    .is_some_and(|stored| stored.offset == 0)
            })
            .cloned()
            .collect();
        for agent in agents {
            self.replay(host, &agent)?;
            self.agent_state(host, &agent, RunState::Completed)?;
        }
        Ok(())
    }

    /// Applies the stored steps of the task's conversation after the step `after`, for a
    /// reconciliation of a lost turn.
    pub(super) fn replay_after(
        &mut self,
        host: &Shared,
        conversation: &str,
        after: u64,
    ) -> Result<()> {
        let path = antigravity::transcript(conversation)?;
        let mut stored = Stored::default();
        let steps = read_steps(&path, &mut stored.offset);
        self.stored.insert(conversation.to_owned(), stored);
        for step in steps.iter().filter(|step| {
            step["step_index"]
                .as_u64()
                .is_some_and(|index| index > after)
        }) {
            self.stored_step(host, conversation, step)?;
        }
        Ok(())
    }

    // Applies one stored step of a conversation. The stream already reports the steps of the
    // task's conversation in a live session, so only their thinking is taken from the file.
    fn stored_step(&mut self, host: &Shared, conversation: &str, step: &Value) -> Result<()> {
        let Some(index) = step["step_index"].as_u64() else {
            return Ok(());
        };
        let created = step["created_at"]
            .as_str()
            .and_then(unix_millis)
            .unwrap_or_else(now);
        let live = self.is_main(conversation) && !self.replaying;
        match step["type"].as_str().unwrap_or_default() {
            "USER_INPUT" if self.replaying && self.is_main(conversation) => {
                let text = request_text(step["content"].as_str().unwrap_or_default());
                if !text.is_empty() {
                    let mut row = self.message(
                        &self.task_id,
                        format!("antigravity:{conversation}:{index}:user"),
                        MessageKind::User,
                        created,
                    );
                    row.text = bounded(text);
                    row.complete = true;
                    self.store(host, &row)?;
                }
            }
            "PLANNER_RESPONSE" => {
                let thinking = step["thinking"].as_str().unwrap_or_default();
                let key = thought_key(conversation, index);
                if thinking.trim().is_empty() {
                    self.drop_thought(host, &key)?;
                } else if !live || self.thoughts.contains_key(&key) {
                    self.stored_text(host, conversation, key, thinking, true, created)?;
                }
                let calls = step["tool_calls"].as_array().cloned().unwrap_or_default();
                if live {
                    // The stream reported the text and the calls; the file adds the arguments of
                    // the calls that still run.
                    for (offset, call) in calls.iter().enumerate() {
                        let key = tool_key(conversation, index + 1 + offset as u64);
                        if self.tools.contains_key(&key) {
                            self.arguments
                                .entry(key)
                                .or_insert_with(|| call["args"].clone());
                        }
                    }
                    return Ok(());
                }
                if self.is_main(conversation) {
                    let used = occupancy(step);
                    if used > 0 {
                        self.used = Some(used);
                    }
                }
                let text = step["content"].as_str().unwrap_or_default();
                self.stored_text(
                    host,
                    conversation,
                    text_key(conversation, index),
                    text,
                    false,
                    created,
                )?;
                for (offset, call) in calls.iter().enumerate() {
                    let at = index + 1 + offset as u64;
                    let name = call["name"].as_str().unwrap_or("tool");
                    self.arguments
                        .insert(tool_key(conversation, at), call["args"].clone());
                    self.tool_started(host, conversation, at, name, &call["args"], created)?;
                }
                if let Some(stored) = self.stored.get_mut(conversation) {
                    stored.calls = (!calls.is_empty()).then_some((index, calls));
                }
            }
            "USER_INPUT" | "SYSTEM_MESSAGE" | "CHECKPOINT" => {}
            _ if !live => {
                let call = self.stored.get(conversation).and_then(|stored| {
                    let (response, calls) = stored.calls.as_ref()?;
                    let position = index.checked_sub(response + 1)?;
                    calls.get(position as usize).cloned()
                });
                let Some(call) = call else {
                    return Ok(());
                };
                let content = result_text(step["content"].as_str().unwrap_or_default());
                if call["name"] == "invoke_subagent"
                    && let Some(agent) = created_agent(content)
                {
                    self.agent_started(
                        host,
                        agent,
                        conversation,
                        call["args"]["Role"].as_str(),
                        call["args"]["Prompt"].as_str(),
                    )?;
                }
                let raw = step["content"].as_str().unwrap_or_default();
                let ending = if step["status"] == "ERROR" {
                    Ending::Failure(step["error"].as_str().unwrap_or("The tool failed"))
                } else {
                    Ending::Success(content)
                };
                self.tool_ended(host, conversation, index, ending, stored_duration(raw))?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Closes the open items of the task, such as after its turn. A tool call that never
    /// reported its result ends as failed.
    pub(super) fn finish(&mut self, host: &Shared) -> Result<()> {
        if self.rows.pending() {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        self.clear_activity(host, WORKING);
        let open: Vec<String> = self.rows.rows.keys().cloned().collect();
        for key in open {
            self.clear_activity(host, &key);
            if let Some(mut message) = self.rows.rows.remove(&key) {
                message.complete = true;
                if !message.text.trim().is_empty() {
                    self.store(host, &message)?;
                }
            }
        }
        let tools: Vec<String> = self.tools.keys().cloned().collect();
        for key in tools {
            self.clear_activity(host, &key);
            if let Some(mut message) = self.tools.remove(&key) {
                message.complete = true;
                let hidden = message.tool.as_ref().is_some_and(silent);
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

    /// Records an outcome for the subagents that still count as working.
    pub(super) fn stop_agents(&mut self, host: &Shared, state: RunState) -> Result<()> {
        let active: Vec<String> = host
            .lock()
            .unwrap()
            .store
            .task(&self.task_id)?
            .subagents
            .iter()
            .filter(|agent| agent.state.active() && self.is_agent(&agent.thread_id))
            .map(|agent| agent.thread_id.clone())
            .collect();
        for agent in active {
            self.agent_state(host, &agent, state.clone())?;
        }
        Ok(())
    }

    /// Writes a notice into the task's transcript, such as the actions the CLI refused.
    pub(super) fn notice(&self, host: &Shared, id: String, text: String) -> Result<()> {
        let mut row = self.message(&self.task_id, id, MessageKind::Notice, now());
        row.text = text;
        row.complete = true;
        self.store(host, &row)
    }
}

/// Reads the complete steps of a stored file from `offset` and moves `offset` past them. A
/// line that is still being written stays for the next read; an unreadable one is skipped.
fn read_steps(path: &Path, offset: &mut u64) -> Vec<Value> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    if file.seek(SeekFrom::Start(*offset)).is_err() {
        return Vec::new();
    }
    let mut reader = BufReader::new(file);
    let mut steps = Vec::new();
    loop {
        let mut bytes = Vec::new();
        let Ok(read) = reader
            .by_ref()
            .take(LINE_LIMIT + 1)
            .read_until(b'\n', &mut bytes)
        else {
            break;
        };
        if read == 0 || !bytes.ends_with(b"\n") {
            if read as u64 > LINE_LIMIT {
                *offset += read as u64;
            }
            break;
        }
        *offset += read as u64;
        if let Ok(step) = serde_json::from_slice::<Value>(&bytes) {
            steps.push(step);
        }
    }
    steps
}

// The row of a response's text.
fn text_key(conversation: &str, step: u64) -> String {
    format!("antigravity:{conversation}:{step}:text")
}

// The row of a response's thinking.
fn thought_key(conversation: &str, step: u64) -> String {
    format!("antigravity:{conversation}:{step}:thinking")
}

// The row of a tool call, numbered by the step that holds its result.
fn tool_key(conversation: &str, step: u64) -> String {
    format!("antigravity:{conversation}:{step}:tool")
}
