//! Maps Grok Build's ACP session updates onto Kodem transcripts, tool calls and subagents.
//!
//! A live session reports its conversation as `session/update` notifications: chunks of the
//! response and of its reasoning, each tool call as it starts and as it changes, and the todo
//! list. The CLI's own notifications add the subagents that a call started. The CLI stores the
//! same updates in the session's `updates.jsonl`, with consecutive chunks merged, so an import
//! replays them through the same code. A subagent runs in a session of its own, whose updates
//! arrive in the same stream and are stored in its own folder.
use crate::{Shared, provider::Streamed};
use anyhow::Result;
use kodem_core::{providers::grok, *};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};

const TEXT_LIMIT: usize = 1024 * 1024;
const DIFF_LINES: usize = 2000;
const DIFF_LINE_CHARS: usize = 2000;
/// Longest stored update that is read, in bytes. A longer line is skipped.
const LINE_LIMIT: u64 = 32 * 1024 * 1024;
/// The activity between the end of a step and the start of the next one, while the model works.
const WORKING: &str = "grok:working";
/// Words of a tool error that mean the call was refused, not that it failed.
const DECLINED: [&str; 4] = ["declined", "denied", "rejected", "user chose not"];
/// Updates that the CLI stores, so a stored session splits its text where the live one did.
const BOUNDARIES: [&str; 8] = [
    "tool_call",
    "tool_call_update",
    "plan",
    "user_message_chunk",
    "subagent_spawned",
    "subagent_finished",
    "turn_completed",
    "background_tasks",
];

/// Returns the CLI's name of the tool that an update is about.
pub(super) fn tool_name(update: &Value) -> &str {
    update["_meta"]["x.ai/tool"]["name"]
        .as_str()
        .or_else(|| update["title"].as_str())
        .unwrap_or("tool")
}

/// Returns the CLI's kind of the tool that an update is about, such as `execute` or `write`.
pub(super) fn tool_kind(update: &Value) -> &str {
    update["_meta"]["x.ai/tool"]["kind"]
        .as_str()
        .unwrap_or_default()
}

/// Chooses the icon and summary wording for a Grok Build tool.
pub(super) fn category(kind: &str) -> ToolCategory {
    match kind {
        "execute"
        | "monitor"
        | "background_task_action"
        | "wait_tasks_action"
        | "kill_task_action" => ToolCategory::Command,
        "read" | "list_dir" | "list" | "lsp" | "memory_search" | "memory_get" | "skill" => {
            ToolCategory::Read
        }
        "search" | "search_tool" => ToolCategory::Search,
        "edit" | "write" | "delete" | "move" => ToolCategory::Edit,
        "web_search" | "web_fetch" => ToolCategory::Fetch,
        "task" | "active_agent_message" => ToolCategory::Agent,
        "plan" | "goal_update" | "enter_plan" | "exit_plan" => ToolCategory::Plan,
        "use_tool" => ToolCategory::Mcp,
        _ => ToolCategory::Other,
    }
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
        .find_map(|name| input[*name].as_str().filter(|text| !text.is_empty()))
        .unwrap_or_default()
}

/// The argument names under which the CLI's tools name the file or folder they work on.
const PATH_FIELDS: [&str; 4] = ["file_path", "target_file", "target_directory", "path"];

/// Returns the file that a tool call works on, as the CLI named it.
pub(super) fn call_path(input: &Value) -> &str {
    text_field(input, &PATH_FIELDS)
}

/// Returns the command that a tool call runs.
pub(super) fn call_command(input: &Value) -> &str {
    text_field(input, &["command"])
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
        "run_terminal_command" | "monitor" => (
            call_command(input).to_owned(),
            text_field(input, &["description"]).to_owned(),
        ),
        "grep" => (
            text_field(input, &["pattern"]).to_owned(),
            [text_field(input, &["path"]), text_field(input, &["glob"])]
                .into_iter()
                .filter(|part| !part.is_empty())
                .map(|part| relative(part, roots))
                .collect::<Vec<_>>()
                .join(" · "),
        ),
        "spawn_subagent" => (
            text_field(input, &["description"]).to_owned(),
            text_field(input, &["prompt"]).to_owned(),
        ),
        "use_tool" => (
            text_field(input, &["tool_name"]).to_owned(),
            input["tool_input"].to_string(),
        ),
        "web_fetch" => (text_field(input, &["url"]).to_owned(), String::new()),
        "todo_write" => ("Plan".into(), String::new()),
        _ if !call_path(input).is_empty() => (relative(call_path(input), roots), String::new()),
        _ => (
            text_field(
                input,
                &["query", "description", "prompt", "tool_name", "task_id"],
            )
            .to_owned(),
            String::new(),
        ),
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

// The lines that one replacement removes and adds, numbered from where each side starts.
fn replacement(
    old: &str,
    old_start: Option<u32>,
    new: &str,
    new_start: Option<u32>,
    lines: &mut Vec<DiffLine>,
    totals: &mut EditedLines,
) {
    for (index, line) in old.lines().enumerate() {
        totals.removed += 1;
        if lines.len() < DIFF_LINES {
            let number = old_start.map(|start| start + index as u32);
            lines.push(diff_line(number, None, LineKind::Removed, line));
        }
    }
    for (index, line) in new.lines().enumerate() {
        totals.added += 1;
        if lines.len() < DIFF_LINES {
            let number = new_start.map(|start| start + index as u32);
            lines.push(diff_line(None, number, LineKind::Added, line));
        }
    }
}

/// Builds the display lines, the totals and the path of the changes that an update reports.
/// The CLI reports each change with the lines where it starts; without them, a change from an
/// empty file starts at its first line.
pub(super) fn edit_diff(update: &Value) -> Option<(Vec<DiffLine>, EditedLines, String)> {
    let mut lines = Vec::new();
    let mut totals = EditedLines::default();
    let mut path = None;
    for item in update["content"].as_array().into_iter().flatten() {
        if item["type"] != "diff" {
            continue;
        }
        path = path.or_else(|| item["path"].as_str().map(str::to_owned));
        let details = item["_meta"]["details"].as_array();
        match details {
            Some(details) if !details.is_empty() => {
                for detail in details {
                    let number = |name: &str| detail[name].as_u64().map(|line| line as u32);
                    replacement(
                        detail["old_string"].as_str().unwrap_or_default(),
                        number("old_line"),
                        detail["new_string"].as_str().unwrap_or_default(),
                        number("new_line"),
                        &mut lines,
                        &mut totals,
                    );
                }
            }
            _ => {
                let old = item["oldText"].as_str().unwrap_or_default();
                let start = old.is_empty().then_some(1);
                replacement(
                    old,
                    start,
                    item["newText"].as_str().unwrap_or_default(),
                    start,
                    &mut lines,
                    &mut totals,
                );
            }
        }
    }
    Some((lines, totals, path?))
}

// Returns the first text of `value` under one of the names, looking one level into objects,
// such as `rawOutput.Content.content`.
fn nested_text<'a>(value: &'a Value, names: &[&str]) -> Option<&'a str> {
    let direct = names
        .iter()
        .find_map(|name| value[*name].as_str().filter(|text| !text.is_empty()));
    direct.or_else(|| {
        value.as_object()?.values().find_map(|inner| {
            names
                .iter()
                .find_map(|name| inner[*name].as_str().filter(|text| !text.is_empty()))
        })
    })
}

/// Returns the output that a finished tool call reports.
pub(super) fn output_text(update: &Value) -> String {
    let raw = &update["rawOutput"];
    // A command reports what it printed, after a line with its exit status.
    if raw["type"] == "Bash" {
        let text = raw["output_for_prompt"].as_str().unwrap_or_default();
        return text
            .lines()
            .filter(|line| !line.starts_with("exit: "))
            .collect::<Vec<_>>()
            .join("\n");
    }
    let content: Vec<&str> = update["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "content")
        .filter_map(|item| item["content"]["text"].as_str())
        .filter(|text| !text.trim().is_empty())
        .collect();
    if !content.is_empty() {
        return content.join("\n");
    }
    nested_text(
        raw,
        &[
            "tool_output_for_prompt_concise",
            "tool_output_for_prompt",
            "output_for_prompt",
            "content",
            "message",
            "text",
            "error",
        ],
    )
    .unwrap_or_default()
    .to_owned()
}

/// The steps of a todo list, as the CLI reports them in a `plan` update.
pub(super) fn plan_steps(entries: &Value) -> Vec<PlanStep> {
    entries
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let text = entry["content"].as_str()?.trim();
            (!text.is_empty()).then(|| PlanStep {
                text: text.chars().take(2000).collect(),
                status: match entry["status"].as_str() {
                    Some("completed") => PlanStatus::Done,
                    Some("in_progress") => PlanStatus::Active,
                    _ => PlanStatus::Pending,
                },
            })
        })
        .take(200)
        .collect()
}

/// Returns the user's own text of a stored prompt, without the wrapper of a message that the
/// user sent during a turn.
pub(super) fn user_text(update: &Value) -> &str {
    let content = &update["content"];
    content["_meta"]["displayText"]
        .as_str()
        .or_else(|| content["text"].as_str())
        .unwrap_or_default()
        .trim()
}

/// A tool call that has not ended.
struct Open {
    message: Message,
    /// The call's arguments, the fullest that the CLI reported.
    input: Value,
    /// The call has a surface of its own or changes only the CLI's files, so it has no row.
    hidden: bool,
}

/// The open text and thinking rows of one session.
#[derive(Default)]
struct Segment {
    /// The turn that the rows belong to.
    prompt: String,
    /// Counts the rows of the turn, so that each row has a key of its own.
    index: u64,
    /// The kind of chunk that the open row holds.
    open: Option<&'static str>,
}

/// Streaming state of one task's session and the sessions of its subagents.
pub(super) struct Transcript {
    task_id: String,
    roots: Vec<PathBuf>,
    /// Folders whose files hold the CLI's own state, such as the plan file of plan mode.
    homes: Vec<PathBuf>,
    /// The session of the task. The others are its subagents.
    main: Option<String>,
    /// Transcript of each subagent session, by session.
    agents: HashMap<String, String>,
    /// Open text and thinking rows, by key.
    rows: Streamed,
    segments: HashMap<String, Segment>,
    /// Tool calls that have not ended, by key.
    tools: HashMap<String, Open>,
    /// Writes the user's prompts, for an import or a reconciliation. A live session stores its
    /// own user rows.
    replaying: bool,
    /// Context occupancy of the task's session, in tokens.
    pub(super) used: Option<u64>,
    pub(super) usage_changed: bool,
    /// What the task's session cost so far, in US dollars. Each turn reports its own cost.
    pub(super) cost: Option<f64>,
    /// A title that the CLI gave the task's session.
    pub(super) title: Option<String>,
    /// The task's todo list, when it changed.
    pub(super) plan: Option<Vec<PlanStep>>,
    /// The mode that the task's session switched to, such as `default` after a plan.
    pub(super) mode: Option<String>,
    /// Why the CLI gave up on the model's request after its retries, such as a usage limit.
    pub(super) failure: Option<String>,
    /// A subagent stopped working, which can release the outcome of a turn that waits for it.
    pub(super) agents_stopped: bool,
}

impl Transcript {
    /// Starts with no open item. `replaying` writes the user's prompts.
    pub(super) fn new(task_id: &str, cwd: &Path, home: &Path, replaying: bool) -> Self {
        let mut roots = vec![cwd.to_owned()];
        // The CLI reports canonical paths, which differ from the stored checkout under symlinks.
        if let Ok(canonical) = cwd.canonicalize().map(kodem_core::platform::plain_path)
            && canonical != cwd
        {
            roots.push(canonical);
        }
        let mut homes = vec![home.to_owned()];
        if let Ok(canonical) = home.canonicalize().map(kodem_core::platform::plain_path)
            && canonical != home
        {
            homes.push(canonical);
        }
        Self {
            task_id: task_id.into(),
            roots,
            homes,
            main: None,
            agents: HashMap::new(),
            rows: Streamed::new(),
            segments: HashMap::new(),
            tools: HashMap::new(),
            replaying,
            used: None,
            usage_changed: false,
            cost: None,
            title: None,
            plan: None,
            mode: None,
            failure: None,
            agents_stopped: false,
        }
    }

    /// Names the session of the task, whose rows go into the task's own transcript.
    pub(super) fn set_main(&mut self, session: &str) {
        self.main = Some(session.to_owned());
    }

    /// Returns the checkout roots used to shorten paths.
    pub(super) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Returns the subagent sessions of the task.
    pub(super) fn agents(&self) -> impl Iterator<Item = &String> {
        self.agents.keys()
    }

    // Returns the transcript of a session, or `None` for a session of another task.
    fn destination(&self, session: &str) -> Option<String> {
        if self.main.as_deref() == Some(session) {
            return Some(self.task_id.clone());
        }
        self.agents.get(session).cloned()
    }

    fn is_main(&self, session: &str) -> bool {
        self.main.as_deref() == Some(session)
    }

    // Calls that change the CLI's own files, such as the plan of plan mode, and calls that
    // have their own surface: the questions, the plan approval and the todo list.
    fn silent(&self, kind: &str, path: &str) -> bool {
        matches!(kind, "ask_user" | "exit_plan" | "enter_plan" | "plan")
            || (category(kind) == ToolCategory::Edit
                && self
                    .homes
                    .iter()
                    .any(|home| Path::new(path).starts_with(home)))
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

    fn clear_activity(&self, host: &Shared, id: &str) {
        if let Some(activities) = host.lock().unwrap().activities.get_mut(&self.task_id) {
            activities.retain(|activity| activity.id != id);
        }
    }

    /// Shows that the model works on the task's next step, with what it waits for.
    pub(super) fn working(&self, host: &Shared, detail: &str) {
        self.activity(host, WORKING, ActivityKind::Reasoning, detail);
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

    /// Applies one session update, live or stored. Updates without transcript meaning are
    /// ignored.
    pub(super) fn update(&mut self, host: &Shared, params: &Value) -> Result<()> {
        let Some(session) = params["sessionId"].as_str() else {
            return Ok(());
        };
        let update = &params["update"];
        let meta = &params["_meta"];
        let kind = update["sessionUpdate"].as_str().unwrap_or_default();
        match kind {
            "subagent_spawned" => {
                if let Some(child) = update["child_session_id"].as_str() {
                    let parent = update["parent_session_id"].as_str().unwrap_or(session);
                    let name = update["description"]
                        .as_str()
                        .or_else(|| update["subagent_type"].as_str());
                    self.agent_started(host, child, parent, name)?;
                }
            }
            "subagent_finished" => {
                if let Some(child) = update["child_session_id"].as_str() {
                    let state = match update["status"].as_str() {
                        Some("completed") => RunState::Completed,
                        Some("cancelled" | "killed" | "interrupted") => RunState::Interrupted,
                        _ => RunState::Failed,
                    };
                    self.close_session(host, child)?;
                    self.agent_state(host, child, state)?;
                }
            }
            _ => {}
        }
        let Some(transcript) = self.destination(session) else {
            return Ok(());
        };
        let main = self.is_main(session);
        if main && let Some(used) = meta["totalTokens"].as_u64().filter(|used| *used > 0) {
            self.usage_changed |= self.used != Some(used);
            self.used = Some(used);
        }
        if let Some(prompt) = meta["promptId"].as_str() {
            let segment = self.segments.entry(session.to_owned()).or_default();
            if segment.prompt != prompt {
                self.end_segment(host, session)?;
                let segment = self.segments.entry(session.to_owned()).or_default();
                segment.prompt = prompt.to_owned();
                segment.index = 0;
            }
        }
        if BOUNDARIES.contains(&kind) {
            self.end_segment(host, session)?;
        }
        let created = meta["agentTimestampMs"].as_u64().unwrap_or_else(now);
        match kind {
            "agent_message_chunk" | "agent_thought_chunk" => {
                let thinking = kind == "agent_thought_chunk";
                let text = update["content"]["text"].as_str().unwrap_or_default();
                if main {
                    self.clear_activity(host, WORKING);
                }
                self.chunk(host, session, &transcript, thinking, text, created)?;
            }
            "user_message_chunk" if self.replaying || !main => {
                let text = user_text(update);
                if !text.is_empty() {
                    let id = meta["eventId"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(new_id);
                    let mut row = self.message(
                        &transcript,
                        format!("grok:{session}:{id}:user"),
                        MessageKind::User,
                        created,
                    );
                    row.text = bounded(text);
                    row.complete = true;
                    self.store(host, &row)?;
                }
            }
            "tool_call" => {
                if main {
                    self.clear_activity(host, WORKING);
                }
                self.tool_started(host, session, &transcript, update, created)?;
            }
            "tool_call_update" => self.tool_changed(host, session, update)?,
            "plan" if main => self.plan = Some(plan_steps(&update["entries"])),
            "session_info_update" if main => {
                if let Some(title) = update["title"].as_str().filter(|title| !title.is_empty()) {
                    self.title = Some(title.to_owned());
                }
            }
            "current_mode_update" if main => {
                self.mode = update["currentModeId"].as_str().map(str::to_owned);
            }
            "turn_completed" if main => {
                let ticks = update["usage"]["costUsdTicks"].as_f64();
                if let Some(ticks) = ticks.filter(|ticks| ticks.is_finite() && *ticks >= 0.0) {
                    self.cost = Some(self.cost.unwrap_or(0.0) + ticks / 1e10);
                    self.usage_changed = true;
                }
            }
            "retry_state" if main && !self.replaying && update["type"] == "exhausted" => {
                self.failure = update["reason"].as_str().map(str::to_owned);
            }
            "retry_state" if main && !self.replaying => {
                let attempt = update["attempt"].as_u64().unwrap_or(1);
                let reason = update["reason"].as_str().unwrap_or("The request failed");
                self.working(
                    host,
                    &format!("Retrying, attempt {attempt}: {}", first_line(reason, 80)),
                );
            }
            _ => {}
        }
        Ok(())
    }

    // Adds a chunk to the open text or thinking row of a session, which a chunk of the other
    // kind ends.
    fn chunk(
        &mut self,
        host: &Shared,
        session: &str,
        transcript: &str,
        thinking: bool,
        text: &str,
        created: u64,
    ) -> Result<()> {
        let label = if thinking { "thinking" } else { "text" };
        let open = self.segments.get(session).and_then(|segment| segment.open);
        if open.is_some_and(|open| open != label) {
            self.end_segment(host, session)?;
        }
        let segment = self.segments.entry(session.to_owned()).or_default();
        let key = format!(
            "grok:{session}:{}:{}:{label}",
            if segment.prompt.is_empty() {
                "turn"
            } else {
                &segment.prompt
            },
            segment.index
        );
        if segment.open.is_none() {
            if text.trim().is_empty() {
                return Ok(());
            }
            segment.open = Some(label);
            let mut row = self.message(
                transcript,
                key.clone(),
                if thinking {
                    MessageKind::Tool
                } else {
                    MessageKind::Assistant
                },
                created,
            );
            if thinking {
                row.tool = Some(ToolCall {
                    name: "Thinking".into(),
                    category: ToolCategory::Thinking,
                    title: "Thought".into(),
                    ..ToolCall::default()
                });
            }
            self.rows.rows.insert(key.clone(), row);
            if self.is_main(session) {
                let kind = if thinking {
                    ActivityKind::Reasoning
                } else {
                    ActivityKind::Responding
                };
                self.activity(host, &key, kind, "");
            }
        }
        let Some(row) = self.rows.rows.get_mut(&key) else {
            return Ok(());
        };
        if row.text.len() + text.len() > TEXT_LIMIT {
            return Ok(());
        }
        row.text.push_str(text);
        if let Some(tool) = &mut row.tool {
            tool.title = thought_title(&row.text);
        }
        if self.rows.changed(&key) {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        Ok(())
    }

    // Ends the open text or thinking row of a session.
    fn end_segment(&mut self, host: &Shared, session: &str) -> Result<()> {
        let Some(segment) = self.segments.get_mut(session) else {
            return Ok(());
        };
        let Some(label) = segment.open.take() else {
            return Ok(());
        };
        let key = format!(
            "grok:{session}:{}:{}:{label}",
            if segment.prompt.is_empty() {
                "turn"
            } else {
                &segment.prompt
            },
            segment.index
        );
        segment.index += 1;
        self.clear_activity(host, &key);
        if let Some(mut row) = self.rows.rows.remove(&key) {
            row.text = bounded(row.text.trim());
            row.complete = true;
            if let Some(tool) = &mut row.tool {
                tool.title = thought_title(&row.text);
                tool.status = ToolStatus::Completed;
            }
            if !row.text.is_empty() {
                self.store(host, &row)?;
            } else {
                host.lock()
                    .unwrap()
                    .store
                    .remove_message(&row.task_id, &key)?;
            }
        }
        if self.is_main(session) {
            self.working(host, "");
        }
        Ok(())
    }

    // Opens a tool row.
    fn tool_started(
        &mut self,
        host: &Shared,
        session: &str,
        transcript: &str,
        update: &Value,
        created: u64,
    ) -> Result<()> {
        let Some(id) = update["toolCallId"].as_str() else {
            return Ok(());
        };
        let key = format!("grok:{session}:{id}");
        if self.tools.contains_key(&key) {
            return self.tool_changed(host, session, update);
        }
        let name = tool_name(update).to_owned();
        let kind = tool_kind(update);
        let input = update["rawInput"].clone();
        let (title, detail) = describe(&name, &input, &self.roots);
        let call = ToolCall {
            name: name.clone(),
            category: category(kind),
            title: title.clone(),
            detail,
            ..ToolCall::default()
        };
        let hidden = self.silent(kind, call_path(&input));
        if self.is_main(session) && !hidden {
            self.activity(host, &key, activity_kind(call.category), &title);
        }
        let mut message = self.message(transcript, key.clone(), MessageKind::Tool, created);
        message.tool = Some(call);
        if !hidden {
            self.store(host, &message)?;
        }
        self.tools.insert(
            key,
            Open {
                message,
                input,
                hidden,
            },
        );
        if update["status"].is_string() {
            self.tool_changed(host, session, update)?;
        }
        Ok(())
    }

    // Applies a change of a tool call: fuller arguments, the diff of an edit, or its end.
    fn tool_changed(&mut self, host: &Shared, session: &str, update: &Value) -> Result<()> {
        let Some(id) = update["toolCallId"].as_str() else {
            return Ok(());
        };
        let key = format!("grok:{session}:{id}");
        let Some(mut open) = self.tools.remove(&key) else {
            return Ok(());
        };
        let message = &mut open.message;
        let mut tool = message.tool.take().unwrap_or_default();
        if let Some(kind) = update["_meta"]["x.ai/tool"]["kind"].as_str() {
            tool.category = category(kind);
            open.hidden |= self.silent(kind, call_path(&update["rawInput"]));
        }
        if update["rawInput"].is_object() {
            open.input = update["rawInput"].clone();
            let input = &open.input;
            let (title, detail) = describe(&tool.name, input, &self.roots);
            tool.title = title;
            if !detail.is_empty() {
                tool.detail = detail;
            }
        }
        if tool.category == ToolCategory::Edit
            && let Some((diff, totals, path)) = edit_diff(update)
        {
            tool.diff = diff;
            message.file_edits = vec![FileEdit {
                path: relative(&path, &self.roots),
                lines: Some(totals),
            }];
        }
        let status = update["status"].as_str();
        let ended = matches!(status, Some("completed" | "failed"));
        if ended {
            if self.is_main(session) {
                self.clear_activity(host, &key);
            }
            tool.duration_ms = Some(now().saturating_sub(message.created));
            let output = output_text(update);
            if status == Some("failed") {
                let lowered = output.to_lowercase();
                tool.status = if DECLINED.iter().any(|phrase| lowered.contains(phrase)) {
                    ToolStatus::Declined
                } else {
                    ToolStatus::Failed
                };
            } else {
                tool.status = ToolStatus::Completed;
            }
            if tool.category == ToolCategory::Command {
                tool.exit = update["rawOutput"]["exit_code"].as_i64();
            }
            message.text = bounded(output.trim());
            // An approved plan stays in the transcript, as the plan of other providers does.
            if let Some(plan) = update["rawOutput"]["PlanReady"]["plan_content"].as_str() {
                tool.title = "Proposed plan".into();
                message.text = bounded(plan.trim());
                open.hidden = false;
            }
            message.complete = true;
        }
        message.tool = Some(tool);
        if !open.hidden {
            self.store(host, &open.message)?;
        }
        if !ended {
            self.tools.insert(key, open);
        }
        Ok(())
    }

    // Registers a subagent session once and links it from the transcript that started it.
    fn agent_started(
        &mut self,
        host: &Shared,
        session: &str,
        parent: &str,
        name: Option<&str>,
    ) -> Result<()> {
        let Some(parent_transcript) = self.destination(parent) else {
            return Ok(());
        };
        if !grok::valid_session(session) || self.agents.contains_key(session) {
            return Ok(());
        }
        let transcript = format!("{}/agent/{session}", self.task_id);
        self.agents.insert(session.to_owned(), transcript);
        let h = host.lock().unwrap();
        let mut task = h.store.task(&self.task_id)?;
        if task
            .subagents
            .iter()
            .any(|known| known.thread_id == session)
        {
            return Ok(());
        }
        let subagent = Subagent {
            // The agent's updates arrive in the same stream as the task's.
            history_loaded: true,
            thread_id: session.into(),
            parent_thread_id: parent.into(),
            name: name
                .map(|name| first_line(name, 80))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("Agent {}", task.subagents.len() + 1)),
            state: if self.replaying {
                RunState::Completed
            } else {
                RunState::Running
            },
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
        h.store.save_task(&task)
    }

    // Records the state of a subagent.
    fn agent_state(&mut self, host: &Shared, session: &str, state: RunState) -> Result<()> {
        let h = host.lock().unwrap();
        let mut task = h.store.task(&self.task_id)?;
        let Some(known) = task
            .subagents
            .iter_mut()
            .find(|known| known.thread_id == session)
        else {
            return Ok(());
        };
        if known.state == state {
            return Ok(());
        }
        self.agents_stopped |= known.state.active() && !state.active();
        known.state = state;
        known.active_turn = None;
        h.store.save_task(&task)
    }

    /// Applies the stored updates of a session after the line `after`, or all of them.
    pub(super) fn replay(
        &mut self,
        host: &Shared,
        path: &Path,
        after: Option<usize>,
    ) -> Result<()> {
        for (index, line) in read_updates(path, usize::MAX).iter().enumerate() {
            if after.is_some_and(|after| index <= after) {
                continue;
            }
            self.update(host, &line["params"])?;
        }
        Ok(())
    }

    /// Closes the open items of the task's own session, such as after its turn, and leaves those
    /// of agents that still work. A tool call that never reported its end ends as failed.
    pub(super) fn finish(&mut self, host: &Shared) -> Result<()> {
        if let Some(main) = self.main.clone() {
            self.close_session(host, &main)?;
        }
        if self.rows.pending() {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        self.clear_activity(host, WORKING);
        Ok(())
    }

    // Ends the open text and thinking row of a session and closes its open tool calls.
    fn close_session(&mut self, host: &Shared, session: &str) -> Result<()> {
        self.end_segment(host, session)?;
        let prefix = format!("grok:{session}:");
        let tools: Vec<String> = self
            .tools
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in tools {
            self.clear_activity(host, &key);
            if let Some(mut open) = self.tools.remove(&key) {
                open.message.complete = true;
                if let Some(tool) = &mut open.message.tool
                    && tool.status == ToolStatus::Running
                {
                    tool.status = ToolStatus::Failed;
                }
                if !open.hidden {
                    self.store(host, &open.message)?;
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
            .filter(|agent| agent.state.active() && self.agents.contains_key(&agent.thread_id))
            .map(|agent| agent.thread_id.clone())
            .collect();
        for agent in active {
            self.agent_state(host, &agent, state.clone())?;
            self.close_session(host, &agent)?;
        }
        if self.rows.pending() {
            self.rows.write(&host.lock().unwrap().store)?;
        }
        Ok(())
    }
}

/// Reads up to `lines` stored updates of a session, oldest first. A line that is still being written or
/// that cannot be read is skipped.
pub(super) fn read_updates(path: &Path, lines: usize) -> Vec<Value> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut reader = BufReader::new(file);
    let mut updates = Vec::new();
    for _ in 0..lines {
        let mut bytes = Vec::new();
        let Ok(read) = reader
            .by_ref()
            .take(LINE_LIMIT + 1)
            .read_until(b'\n', &mut bytes)
        else {
            break;
        };
        if read == 0 {
            break;
        }
        if !bytes.ends_with(b"\n") {
            if read as u64 > LINE_LIMIT {
                continue;
            }
            break;
        }
        if let Ok(update) = serde_json::from_slice::<Value>(&bytes) {
            updates.push(update);
        }
    }
    updates
}
