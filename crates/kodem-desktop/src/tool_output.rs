//! Presentation of the host's saved tool transcripts, without changing their source text.
//!
//! Providers that report structured tool calls fill `Message::tool`. Older records and the
//! other providers use text conventions. [`view`] turns both into one shape for the transcript.
use kodem_core::{DiffLine, EditedLines, Message, ToolCategory, ToolStatus};
use std::{collections::HashSet, rc::Rc};

/// One tool call, ready to summarize and expand in the transcript.
pub struct ToolView {
    pub category: ToolCategory,
    /// The provider's tool name, kept for the copyable record.
    pub name: String,
    pub title: String,
    pub detail: String,
    pub status: ToolStatus,
    pub exit: Option<i64>,
    pub duration_ms: Option<u64>,
    /// Captured output with terminal escape sequences removed. A standalone JSON value is
    /// pretty-printed.
    pub output: String,
    /// Syntax of `output`, or empty for plain text.
    pub language: &'static str,
    pub diff: Vec<DiffLine>,
}

/// Tool views kept per generation before the older generation is dropped.
const CACHED_VIEWS: usize = 512;

// What a cached view was built from. A view is rebuilt when any of it changes.
#[derive(PartialEq)]
struct Source {
    length: usize,
    complete: bool,
    status: Option<ToolStatus>,
    duration_ms: Option<u64>,
    diff_lines: usize,
}

#[derive(Default)]
struct ViewCache {
    recent: std::collections::HashMap<String, (Source, Rc<ToolView>)>,
    older: std::collections::HashMap<String, (Source, Rc<ToolView>)>,
}

thread_local! {
    static VIEW_CACHE: std::cell::RefCell<ViewCache> = std::cell::RefCell::default();
}

/// Reads a tool message into its display form, from structured data when the provider sent it.
///
/// The transcript asks for every call of a visible run on every frame, so views are kept until
/// their message changes.
pub fn view(message: &Message) -> Rc<ToolView> {
    let key = format!("{}:{}", message.task_id, message.id);
    let source = Source {
        length: message.text.len(),
        complete: message.complete,
        status: message.tool.as_ref().map(|tool| tool.status),
        duration_ms: message.tool.as_ref().and_then(|tool| tool.duration_ms),
        diff_lines: message.tool.as_ref().map_or(0, |tool| tool.diff.len()),
    };
    VIEW_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((cached, view)) = cache.recent.get(&key)
            && *cached == source
        {
            return view.clone();
        }
        let view = match cache.older.remove(&key) {
            Some((cached, view)) if cached == source => view,
            _ => Rc::new(build_view(message)),
        };
        if cache.recent.len() >= CACHED_VIEWS {
            cache.older = std::mem::take(&mut cache.recent);
        }
        cache.recent.insert(key, (source, view.clone()));
        view
    })
}

// Pretty-prints a standalone structured response, while leaving logs and shell output literal.
fn structured(output: String) -> (String, &'static str) {
    if output.len() <= 64 * 1024
        && output.trim_start().starts_with(['{', '['])
        && let Ok(value @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) =
            serde_json::from_str::<serde_json::Value>(&output)
    {
        return (
            serde_json::to_string_pretty(&value).unwrap_or(output),
            "json",
        );
    }
    (output, "")
}

fn build_view(message: &Message) -> ToolView {
    if let Some(tool) = &message.tool {
        let (output, language) = structured(display_text(&message.text));
        return ToolView {
            category: tool.category,
            name: tool.name.clone(),
            title: tool.title.clone(),
            detail: tool.detail.clone(),
            status: tool.status,
            exit: tool.exit,
            duration_ms: tool.duration_ms,
            output,
            language,
            diff: tool.diff.clone(),
        };
    }
    let record = display_record(&message.text, message.complete);
    let output = parse(&record, message.complete);
    let failed = output.exit.is_some_and(|code| code != 0)
        || output.status.is_some_and(|status| status == "failed");
    let status = if output.status == Some("declined") {
        ToolStatus::Declined
    } else if failed {
        ToolStatus::Failed
    } else if message.complete {
        ToolStatus::Completed
    } else {
        ToolStatus::Running
    };
    let (category, title) = match (&output.kind, output.title) {
        (ToolKind::Command, command) => (ToolCategory::Command, command),
        (ToolKind::Files, _) => (ToolCategory::Edit, "File changes"),
        (ToolKind::Other, "mcpToolCall") => (ToolCategory::Mcp, "MCP tool"),
        (ToolKind::Other, "dynamicToolCall") => (ToolCategory::Other, "Tool call"),
        (ToolKind::Other, "collabAgentToolCall") => (ToolCategory::Agent, "Agent activity"),
        (ToolKind::Other, "webSearch") => (ToolCategory::Fetch, "Web search"),
        (ToolKind::Other, "imageView") => (ToolCategory::Read, "View image"),
        (ToolKind::Other, "imageGeneration") => (ToolCategory::Other, "Generate image"),
        (ToolKind::Other, "contextCompaction") => (ToolCategory::Other, "Compact context"),
        (ToolKind::Other, title) => (ToolCategory::Other, title),
    };
    let (body, language) = structured(display_text(output.body));
    ToolView {
        category,
        name: output.title.chars().take(80).collect(),
        title: title.chars().take(400).collect(),
        detail: String::new(),
        status,
        exit: output.exit,
        duration_ms: None,
        output: body,
        language,
        diff: Vec::new(),
    }
}

/// Returns the full copyable record of a tool call: its subject, then its output.
pub fn record(message: &Message) -> String {
    match &message.tool {
        Some(tool) if message.text.is_empty() => format!("{}: {}", tool.name, tool.title),
        Some(tool) => format!("{}: {}\n{}", tool.name, tool.title, message.text),
        None => message.text.clone(),
    }
}

/// Counts the calls of a group by what they did, for a one-line summary.
pub fn summary(views: &[Rc<ToolView>]) -> String {
    let count = |category: ToolCategory| views.iter().filter(|v| v.category == category).count();
    let plural = |count: usize, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    let mut parts = Vec::new();
    let thoughts = count(ToolCategory::Thinking);
    let commands = count(ToolCategory::Command);
    let reads = count(ToolCategory::Read);
    let searches = count(ToolCategory::Search);
    let fetches = count(ToolCategory::Fetch);
    let agents = count(ToolCategory::Agent);
    let other = views.len()
        - thoughts
        - commands
        - reads
        - searches
        - fetches
        - agents
        - count(ToolCategory::Edit);
    if commands > 0 {
        parts.push(format!("ran {}", plural(commands, "command", "commands")));
    }
    if reads > 0 {
        parts.push(format!("read {}", plural(reads, "file", "files")));
    }
    if searches > 0 {
        parts.push(plural(searches, "search", "searches"));
    }
    if fetches > 0 {
        parts.push(plural(fetches, "web lookup", "web lookups"));
    }
    if agents > 0 {
        parts.push(plural(agents, "agent call", "agent calls"));
    }
    if other > 0 {
        parts.push(plural(other, "tool call", "tool calls"));
    }
    if parts.is_empty() && thoughts > 0 {
        parts.push("thought".into());
    }
    let mut text = parts.join(" · ");
    if let Some(first) = text.get(..1) {
        text = first.to_uppercase() + &text[1..];
    }
    text
}

/// Formats a duration for a status readout: milliseconds, seconds, then minutes and seconds.
pub fn duration(milliseconds: u64) -> String {
    if milliseconds < 1000 {
        format!("{milliseconds}ms")
    } else if milliseconds < 60_000 {
        format!("{:.1}s", milliseconds as f64 / 1000.)
    } else {
        format!(
            "{}m {}s",
            milliseconds / 60_000,
            milliseconds % 60_000 / 1000
        )
    }
}

#[derive(Default)]
pub struct EditSummary {
    pub files: usize,
    pub lines: Option<EditedLines>,
}

/// Counts distinct edited paths and sums recorded changes across successful calls in a group.
pub fn edit_summary(messages: &[Message]) -> EditSummary {
    let mut paths = HashSet::new();
    let mut lines = Some(EditedLines::default());
    for message in messages {
        if let Some(tool) = &message.tool {
            if tool.category != ToolCategory::Edit || tool.status != ToolStatus::Completed {
                continue;
            }
        } else {
            let output = parse(&message.text, message.complete);
            if !message.complete
                || output.kind != ToolKind::Files
                || output.status != Some("completed")
            {
                continue;
            }
        }
        if message.file_edits.is_empty() && message.tool.is_none() {
            // Older transcripts saved only paths. Show their file count without inventing line totals.
            let output = parse(&message.text, message.complete);
            for path in output.body.lines().filter(|path| !path.is_empty()) {
                paths.insert(path);
                lines = None;
            }
        } else {
            for edit in &message.file_edits {
                paths.insert(&edit.path);
                lines = lines.zip(edit.lines).map(|(mut total, edit)| {
                    total.added += edit.added;
                    total.removed += edit.removed;
                    total
                });
            }
        }
    }
    EditSummary {
        files: paths.len(),
        lines,
    }
}

#[derive(Debug, PartialEq)]
pub enum ToolKind {
    Command,
    Files,
    Other,
}
#[derive(Debug)]
pub struct ToolOutput<'a> {
    pub kind: ToolKind,
    pub title: &'a str,
    pub body: &'a str,
    pub status: Option<&'a str>,
    pub exit: Option<i64>,
}
/// Gives saved structured provider items the same heading and body as regular tool records.
pub fn display_record(text: &str, complete: bool) -> std::borrow::Cow<'_, str> {
    #[derive(serde::Deserialize)]
    struct Record {
        #[serde(rename = "type")]
        kind: String,
        status: Option<String>,
    }
    if text.trim_start().starts_with('{')
        && let Ok(record) = serde_json::from_str::<Record>(text)
        && !record.kind.is_empty()
    {
        return format!(
            "{} · {}\n{text}",
            record.kind,
            record
                .status
                .as_deref()
                .unwrap_or(if complete { "completed" } else { "in progress" })
        )
        .into();
    }
    text.into()
}

/// Separates the host's command, file-change and generic activity records for display.
pub fn parse(text: &str, complete: bool) -> ToolOutput<'_> {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    if let Some(command) = first.strip_prefix("$ ") {
        let trimmed = rest.trim_end_matches('\n');
        let (body, last) = trimmed.rsplit_once('\n').unwrap_or(("", trimmed));
        let exit = complete
            .then_some(last)
            .and_then(|last| last.strip_prefix("Exit "))
            .and_then(|code| code.parse().ok());
        return ToolOutput {
            kind: ToolKind::Command,
            title: command,
            body: if exit.is_some() { body } else { rest },
            status: None,
            exit,
        };
    }
    if let Some(status) = first.strip_prefix("File changes · ") {
        return ToolOutput {
            kind: ToolKind::Files,
            title: "File changes",
            body: rest,
            status: Some(status),
            exit: None,
        };
    }
    let (title, status) = first
        .split_once(" · ")
        .map_or((first, None), |(title, status)| (title, Some(status)));
    ToolOutput {
        kind: ToolKind::Other,
        title: if title.is_empty() {
            "Tool activity"
        } else {
            title
        },
        body: rest,
        status,
        exit: None,
    }
}
/// Removes terminal escape sequences from display text; copying the original remains available.
pub fn display_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.next() {
                Some('[') => {
                    for ch in chars.by_ref() {
                        if ('@'..='~').contains(&ch) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => result.push('\n'),
            ch if !ch.is_control() || ch == '\n' || ch == '\t' => result.push(ch),
            _ => {}
        }
    }
    result
}
/// Returns a bounded UTF-8 preview and reports whether more captured output is available.
pub fn preview(text: &str) -> (String, bool) {
    let mut end = 0;
    let mut lines = 0;
    for (characters, (index, ch)) in text.char_indices().enumerate() {
        if characters == 1800 || lines == 12 {
            break;
        }
        end = index + ch.len_utf8();
        if ch == '\n' {
            lines += 1;
        }
    }
    (
        text[..end].trim_end_matches('\n').to_owned(),
        end < text.len(),
    )
}
