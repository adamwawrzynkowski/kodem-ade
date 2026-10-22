mod prompts;

use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use kodem_core::{Generation, GenerationEvent, Request, Response, ipc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::git::GitRepo;
use crate::settings::{AiAgent, Settings};

/// How long one status request waits for news of a running generation, in milliseconds.
const STATUS_WAIT_MS: u64 = 1_000;

/// The Kodem agent that runs AI requests, the folder they run in, and the custom prompts.
#[derive(Debug, Clone)]
pub struct AiConfig {
    pub agent: AiAgent,
    pub cwd: PathBuf,
    pub commit_prompt: String,
    pub review_prompt: String,
    pub explain_prompt: String,
    pub pull_request_review_prompt: String,
}

impl AiConfig {
    /// Returns the configuration of requests that `agent` runs in `cwd`, with the custom prompts
    /// of `settings`.
    pub fn new(settings: &Settings, agent: AiAgent, cwd: PathBuf) -> Self {
        Self {
            agent,
            cwd,
            commit_prompt: settings.ai_commit_prompt.clone(),
            review_prompt: settings.ai_review_prompt.clone(),
            explain_prompt: settings.ai_explain_prompt.clone(),
            pull_request_review_prompt: settings.ai_pull_request_review_prompt.clone(),
        }
    }
}

/// The changes that an AI request reads: the changed files, the diff stat, the diff and, when
/// requested, the full contents of the changed files.
#[derive(Debug, Clone)]
pub struct ChangeContext {
    pub files: String,
    pub stat: String,
    pub diff: String,
    pub file_contents: Vec<(String, String)>,
}

/// Which changes an AI request reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeScope {
    Staged,
    StagedAndUnstaged,
}

impl ChangeScope {
    /// Returns `StagedAndUnstaged` when `include_unstaged` is set, and `Staged` otherwise.
    pub fn from_include_unstaged(include_unstaged: bool) -> Self {
        if include_unstaged {
            Self::StagedAndUnstaged
        } else {
            Self::Staged
        }
    }

    /// Returns whether the scope includes unstaged and untracked changes.
    pub fn includes_unstaged(self) -> bool {
        self == Self::StagedAndUnstaged
    }
}

impl ChangeContext {
    /// Reads the changes of `repo` for an AI request. With `amend`, staged changes are compared to
    /// the parent of `HEAD`. With `include_whole_file`, it adds the contents of changed files up to
    /// 64,000 bytes, except files with a path part that starts with a dot. Fails when the scope has
    /// no changes.
    pub fn from_repo(
        repo: &GitRepo,
        amend: bool,
        include_whole_file: bool,
        scope: ChangeScope,
    ) -> Result<Self> {
        let base = staged_diff_base(repo, amend)?;
        let mut diff = run_staged_change_command(repo, base.as_deref(), &[])?;
        let mut stat = run_staged_change_command(repo, base.as_deref(), &["--stat"])?;
        let mut files = run_staged_change_command(repo, base.as_deref(), &["--name-status"])?;

        if scope.includes_unstaged() {
            append_output(&mut diff, &run_unstaged_change_command(repo, &[])?);
            append_output(&mut stat, &run_unstaged_change_command(repo, &["--stat"])?);
            append_output(
                &mut files,
                &run_unstaged_change_command(repo, &["--name-status"])?,
            );

            let untracked = collect_untracked_changes(repo)?;
            append_output(&mut diff, &untracked.diff);
            append_output(&mut stat, &untracked.stat);
            append_output(&mut files, &untracked.files);
        }

        if diff.trim().is_empty() {
            bail!(match scope {
                ChangeScope::Staged => "There are no staged changes for AI analysis.",
                ChangeScope::StagedAndUnstaged => "There are no changes for AI analysis.",
            });
        }
        let file_contents = if include_whole_file {
            load_change_files(repo, &files, scope)
        } else {
            Vec::new()
        };
        Ok(Self {
            files,
            stat,
            diff,
            file_contents,
        })
    }
}

fn staged_diff_base(repo: &GitRepo, amend: bool) -> Result<Option<String>> {
    if !amend {
        return Ok(None);
    }
    repo.amend_base_ref().map(Some)
}

fn run_staged_change_command(repo: &GitRepo, base: Option<&str>, extra: &[&str]) -> Result<String> {
    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    args.extend_from_slice(extra);
    args.push("--cached");
    if let Some(base) = base {
        args.push(base);
    }
    args.push("--");
    repo.run(&args)
}

fn run_unstaged_change_command(repo: &GitRepo, extra: &[&str]) -> Result<String> {
    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    args.extend_from_slice(extra);
    args.push("--");
    repo.run(&args)
}

#[derive(Default)]
struct UntrackedChanges {
    files: String,
    stat: String,
    diff: String,
}

fn collect_untracked_changes(repo: &GitRepo) -> Result<UntrackedChanges> {
    let paths = repo.run(&["ls-files", "--others", "--exclude-standard", "--"])?;
    let mut changes = UntrackedChanges::default();

    for path in paths.lines().filter(|path| !path.is_empty()) {
        let file_path = repo.path.join(path);
        let byte_count = std::fs::metadata(&file_path)?.len();
        let mut bytes = Vec::new();
        std::fs::File::open(file_path)?
            .take(64_001)
            .read_to_end(&mut bytes)?;
        append_output(&mut changes.files, &format!("A\t{path}"));
        append_output(
            &mut changes.stat,
            &format!("{path} | untracked ({byte_count} bytes)"),
        );
        append_output(
            &mut changes.diff,
            &render_untracked_diff(path, &bytes, byte_count),
        );
    }

    Ok(changes)
}

fn render_untracked_diff(path: &str, bytes: &[u8], byte_count: u64) -> String {
    let mut diff = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n"
    );
    let Ok(content) = std::str::from_utf8(bytes) else {
        diff.push_str(&format!("Binary file, {byte_count} bytes\n"));
        return diff;
    };
    if content.contains('\0') {
        diff.push_str(&format!("Binary file, {byte_count} bytes\n"));
        return diff;
    }
    if content.is_empty() {
        return diff;
    }

    diff.push_str(&format!("@@ -0,0 +1,{} @@\n", content.lines().count()));
    for line in content.split_inclusive('\n') {
        diff.push('+');
        diff.push_str(line);
    }
    if !content.ends_with('\n') {
        diff.push_str("\n\\ No newline at end of file\n");
    }
    if byte_count > bytes.len() as u64 {
        diff.push_str("[Untracked file content was truncated by Kodem]\n");
    }
    diff
}

fn append_output(target: &mut String, output: &str) {
    let output = output.trim_end();
    if output.is_empty() {
        return;
    }
    if !target.is_empty() && !target.ends_with('\n') {
        target.push('\n');
    }
    target.push_str(output);
    target.push('\n');
}

fn load_change_files(repo: &GitRepo, files: &str, scope: ChangeScope) -> Vec<(String, String)> {
    let paths = files
        .lines()
        .filter_map(|line| line.split('\t').next_back())
        .filter(|path| !path.is_empty() && !is_dotfile(path))
        .collect::<std::collections::BTreeSet<_>>();

    paths
        .into_iter()
        .filter_map(|path| {
            let content = match scope {
                ChangeScope::Staged => repo.run(&["show", &format!(":{path}")]).ok(),
                ChangeScope::StagedAndUnstaged => {
                    std::fs::read_to_string(repo.path.join(path)).ok()
                }
            }?;
            (content.len() <= 64_000).then(|| (path.to_string(), content))
        })
        .collect()
}

fn is_dotfile(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
}

/// A commit title and description that the AI wrote.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommitMessage {
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// The part of a commit message that the AI writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitGenerationTarget {
    Title,
    Description,
}

/// The headline and overview of an AI review.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewSummary {
    pub headline: String,
    pub overview: String,
}

/// How serious a review finding or pull request comment is.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSeverity {
    Critical,
    Warning,
    Info,
}

/// The kind of issue that a review finding reports.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCategory {
    Bug,
    Security,
    Improvement,
    Deletion,
    SensitiveData,
}

/// One issue that an AI review of local changes found.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    pub file: String,
    #[serde(default)]
    pub line_hint: Option<String>,
    pub severity: ReviewSeverity,
    pub category: ReviewCategory,
    pub title: String,
    pub impact: String,
    pub evidence: String,
    pub recommendation: String,
    #[serde(default)]
    pub code_snippet: Option<String>,
}

/// The AI review of local changes: a summary and the findings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodeReview {
    pub summary: ReviewSummary,
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
}

/// One comment that an AI pull request review places on a line or line range. `side` is `LEFT` for
/// deleted lines and `RIGHT` for added or context lines.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestComment {
    pub file: String,
    pub side: String,
    pub line: u32,
    #[serde(default)]
    pub start_line: Option<u32>,
    pub severity: ReviewSeverity,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub suggestion: Option<String>,
}

/// The AI review of a pull request: a summary and the line comments.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PullRequestReview {
    pub summary: ReviewSummary,
    #[serde(default)]
    pub comments: Vec<PullRequestComment>,
}

/// The pull request details and diff that an AI pull request review reads.
#[derive(Debug, Clone)]
pub struct PullRequestContext {
    pub title: String,
    pub author: String,
    pub base: String,
    pub head: String,
    pub diff: String,
}

/// A progress update from a running AI request: a status line, or new text of the model's thinking
/// or response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiProgress {
    Status(String),
    ThinkingDelta(String),
    ResponseDelta(String),
}

/// Runs AI requests with the configured Kodem agent through the Kodem host. All methods block
/// until the provider finishes; run them on a background thread.
#[derive(Debug, Clone)]
pub struct AiService {
    config: AiConfig,
}

impl AiService {
    /// Creates a service that uses `config`.
    pub fn new(config: AiConfig) -> Self {
        Self { config }
    }

    /// Writes the commit title or the description for the changes. When the response is not JSON,
    /// the first line becomes the title or all lines become the description.
    pub fn generate_commit(
        &self,
        context: &ChangeContext,
        target: CommitGenerationTarget,
    ) -> Result<CommitMessage> {
        let (system, user) = prompts::commit(&self.config, context, target);
        let mut ignore_progress = |_| {};
        let text = self.generate(&system, &user, Some(commit_schema()), &mut ignore_progress)?;
        if let Ok(message) = parse_json(&text, "commit message") {
            return Ok(message);
        }
        let lines = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        if lines.is_empty() {
            bail!("The AI returned an invalid commit message response.");
        }
        Ok(match target {
            CommitGenerationTarget::Title => CommitMessage {
                title: lines[0].trim_matches('`').trim_matches('"').to_string(),
                description: String::new(),
            },
            CommitGenerationTarget::Description => CommitMessage {
                title: String::new(),
                description: lines.join("\n"),
            },
        })
    }

    /// Reviews local changes and returns the findings. Sends progress updates while the provider
    /// runs.
    pub fn review_changes(
        &self,
        context: &ChangeContext,
        progress: &mut dyn FnMut(AiProgress),
    ) -> Result<CodeReview> {
        let (system, user) = prompts::review(&self.config, context);
        let text = self.generate(&system, &user, Some(review_schema()), progress)?;
        parse_json(&text, "code review")
    }

    /// Explains a diff in Markdown. `file_path` and `commit_message` add context to the prompt.
    /// Sends progress updates, including the response text as it arrives.
    pub fn explain_diff(
        &self,
        diff: &str,
        file_path: Option<&str>,
        commit_message: Option<&str>,
        progress: &mut dyn FnMut(AiProgress),
    ) -> Result<String> {
        let (system, user) = prompts::explain(&self.config, diff, file_path, commit_message);
        self.generate(&system, &user, None, progress)
    }

    /// Reviews a pull request and returns line comments. Sends progress updates while the provider
    /// runs.
    pub fn review_pull_request(
        &self,
        context: &PullRequestContext,
        progress: &mut dyn FnMut(AiProgress),
    ) -> Result<PullRequestReview> {
        let (system, user) = prompts::pull_request_review(&self.config, context);
        let text = self.generate(&system, &user, Some(pull_request_review_schema()), progress)?;
        parse_json(&text, "pull request review")
    }

    // Runs one request on the Kodem host with the configured agent and passes its progress on.
    fn generate(
        &self,
        system: &str,
        user: &str,
        schema: Option<Value>,
        progress: &mut dyn FnMut(AiProgress),
    ) -> Result<String> {
        let root = ipc::data_dir()?;
        let job = kodem_core::new_id();
        let agent = &self.config.agent;
        ipc::request(
            &root,
            &Request::Generate {
                job: job.clone(),
                generation: Generation {
                    provider: agent.provider,
                    account: agent.account.clone(),
                    model: agent.model.clone(),
                    effort: agent.effort.clone(),
                    cwd: self.config.cwd.clone(),
                    instructions: system.to_string(),
                    input: user.to_string(),
                    schema,
                },
            },
        )?;
        let mut after = 0;
        loop {
            let Response::Generation(status) = ipc::request(
                &root,
                &Request::GenerationStatus {
                    job: job.clone(),
                    after,
                    wait_ms: STATUS_WAIT_MS,
                },
            )?
            else {
                bail!("The Kodem host gave an unexpected answer.");
            };
            for event in status.events {
                progress(match event {
                    GenerationEvent::Status(status) => AiProgress::Status(status),
                    GenerationEvent::Thinking(text) => AiProgress::ThinkingDelta(text),
                    GenerationEvent::Response(text) => AiProgress::ResponseDelta(text),
                });
            }
            after = status.next;
            if status.done {
                return match status.result {
                    Some(result) => {
                        progress(AiProgress::Status(
                            "Kodem is formatting the result...".into(),
                        ));
                        Ok(result)
                    }
                    None => bail!(
                        "{}",
                        status
                            .error
                            .unwrap_or_else(|| "The AI request failed.".into())
                    ),
                };
            }
        }
    }
}

fn parse_json<T: for<'de> Deserialize<'de>>(response: &str, kind: &str) -> Result<T> {
    let trimmed = response.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim);
    if let Some(value) = unfenced
        && let Ok(parsed) = serde_json::from_str(value)
    {
        return Ok(parsed);
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}'))
        && let Ok(parsed) = serde_json::from_str(&trimmed[start..=end])
    {
        return Ok(parsed);
    }
    bail!("The AI returned an invalid {kind} response.")
}

fn commit_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["title", "description"],
        "properties": {
            "title": { "type": "string" },
            "description": { "type": "string" }
        }
    })
}

fn review_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "findings"],
        "properties": {
            "summary": summary_schema(),
            "findings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["file", "lineHint", "severity", "category", "title", "impact", "evidence", "recommendation", "codeSnippet"],
                    "properties": {
                        "file": { "type": "string" },
                        "lineHint": { "type": ["string", "null"] },
                        "severity": { "type": "string", "enum": ["critical", "warning", "info"] },
                        "category": { "type": "string", "enum": ["bug", "security", "improvement", "deletion", "sensitive_data"] },
                        "title": { "type": "string" },
                        "impact": { "type": "string" },
                        "evidence": { "type": "string" },
                        "recommendation": { "type": "string" },
                        "codeSnippet": { "type": ["string", "null"] }
                    }
                }
            }
        }
    })
}

fn pull_request_review_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "comments"],
        "properties": {
            "summary": summary_schema(),
            "comments": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["file", "side", "line", "startLine", "severity", "title", "body", "suggestion"],
                    "properties": {
                        "file": { "type": "string" },
                        "side": { "type": "string", "enum": ["LEFT", "RIGHT"] },
                        "line": { "type": "integer" },
                        "startLine": { "type": ["integer", "null"] },
                        "severity": { "type": "string", "enum": ["critical", "warning", "info"] },
                        "title": { "type": "string" },
                        "body": { "type": "string" },
                        "suggestion": { "type": ["string", "null"] }
                    }
                }
            }
        }
    })
}

fn summary_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["headline", "overview"],
        "properties": {
            "headline": { "type": "string" },
            "overview": { "type": "string" }
        }
    })
}
