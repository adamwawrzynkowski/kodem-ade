mod prompts;

use std::env;
use std::io::{BufRead as _, BufReader, Read as _, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::git::GitRepo;
use crate::settings::{AiProvider, Settings};

/// The AI provider, models and custom prompts, taken from the settings.
#[derive(Debug, Clone)]
pub struct AiConfig {
    pub provider: AiProvider,
    pub ollama_base_url: String,
    pub commit_model: String,
    pub review_model: String,
    pub explain_model: String,
    pub codex_model: String,
    pub codex_reasoning_effort: String,
    pub commit_prompt: String,
    pub review_prompt: String,
    pub explain_prompt: String,
    pub pull_request_review_prompt: String,
}

impl From<&Settings> for AiConfig {
    fn from(settings: &Settings) -> Self {
        Self {
            provider: settings.ai_provider,
            ollama_base_url: settings.ollama_base_url.clone(),
            commit_model: settings.ai_commit_model.clone(),
            review_model: settings.ai_review_model.clone(),
            explain_model: settings.ai_explain_model.clone(),
            codex_model: settings.codex_model.clone(),
            codex_reasoning_effort: settings.codex_reasoning_effort.clone(),
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

/// Runs AI requests with the configured provider. All methods block until the provider finishes;
/// run them on a background thread.
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
        let text = self.generate(
            Feature::Commit,
            &system,
            &user,
            Some(commit_schema()),
            &mut ignore_progress,
        )?;
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
        let text = self.generate(
            Feature::Review,
            &system,
            &user,
            Some(review_schema()),
            progress,
        )?;
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
        self.generate(Feature::Explain, &system, &user, None, progress)
    }

    /// Reviews a pull request and returns line comments. Sends progress updates while the provider
    /// runs.
    pub fn review_pull_request(
        &self,
        context: &PullRequestContext,
        progress: &mut dyn FnMut(AiProgress),
    ) -> Result<PullRequestReview> {
        let (system, user) = prompts::pull_request_review(&self.config, context);
        let text = self.generate(
            Feature::PullRequestReview,
            &system,
            &user,
            Some(pull_request_review_schema()),
            progress,
        )?;
        parse_json(&text, "pull request review")
    }

    fn generate(
        &self,
        feature: Feature,
        system: &str,
        user: &str,
        schema: Option<Value>,
        progress: &mut dyn FnMut(AiProgress),
    ) -> Result<String> {
        match self.config.provider {
            AiProvider::Ollama => self.generate_with_ollama(
                feature.model(&self.config),
                system,
                user,
                schema,
                progress,
            ),
            AiProvider::Codex => self.generate_with_codex(system, user, schema, progress),
        }
    }

    fn generate_with_ollama(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: Option<Value>,
        progress: &mut dyn FnMut(AiProgress),
    ) -> Result<String> {
        if model.trim().is_empty() {
            bail!("Select an Ollama model in AI settings.");
        }
        let url = format!(
            "{}/api/generate",
            self.config.ollama_base_url.trim_end_matches('/')
        );
        let structured = schema.is_some();
        let mut body = json!({
            "model": model,
            "system": system,
            "prompt": user,
            "stream": true,
            "think": true,
            "options": { "temperature": 0.15 }
        });
        if let Some(schema) = schema {
            body["format"] = schema;
        }
        progress(AiProgress::Status(format!("Ollama is loading {model}...")));
        let mut child = command("curl")
            .args([
                "-sS",
                "--fail-with-body",
                "--max-time",
                "300",
                "-H",
                "Content-Type: application/json",
                "--data-binary",
                "@-",
                &url,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Kodem could not start curl")?;
        child
            .stdin
            .take()
            .expect("curl stdin must be piped")
            .write_all(body.to_string().as_bytes())?;
        let stdout = child.stdout.take().context("curl did not open stdout")?;
        let mut stderr = child.stderr.take().context("curl did not open stderr")?;
        let stderr_task = std::thread::spawn(move || {
            let mut output = String::new();
            let _ = stderr.read_to_string(&mut output);
            output
        });
        let mut response = String::new();
        let mut stream_error = None;
        let mut response_started = false;
        progress(AiProgress::Status(
            "Ollama is analyzing the request...".into(),
        ));
        for line in BufReader::new(stdout).lines() {
            let chunk = match line
                .context("Kodem could not read the Ollama response")
                .and_then(|line| {
                    serde_json::from_str::<OllamaGenerateChunk>(&line)
                        .context("Ollama returned an invalid response chunk")
                }) {
                Ok(chunk) => chunk,
                Err(error) => {
                    stream_error = Some(error);
                    let _ = child.kill();
                    break;
                }
            };
            if let Some(error) = chunk.error {
                stream_error = Some(anyhow::anyhow!(error));
                let _ = child.kill();
                break;
            }
            if !chunk.thinking.is_empty() {
                progress(AiProgress::ThinkingDelta(chunk.thinking));
            }
            if !chunk.response.is_empty() {
                response.push_str(&chunk.response);
                if structured {
                    if !response_started {
                        progress(AiProgress::Status(
                            "Ollama is writing the structured result...".into(),
                        ));
                    }
                } else {
                    progress(AiProgress::ResponseDelta(chunk.response));
                }
                response_started = true;
            }
        }
        let status = child.wait()?;
        let error_output = stderr_task.join().unwrap_or_default();
        if let Some(error) = stream_error {
            return Err(error);
        }
        if !status.success() {
            bail!("Ollama request failed: {}", error_output.trim());
        }
        if response.trim().is_empty() {
            bail!("Ollama returned an empty response.");
        }
        progress(AiProgress::Status(
            "Kodem is formatting the result...".into(),
        ));
        Ok(response)
    }

    fn generate_with_codex(
        &self,
        system: &str,
        user: &str,
        schema: Option<Value>,
        progress: &mut dyn FnMut(AiProgress),
    ) -> Result<String> {
        if !codex_available() {
            bail!("Codex CLI is not installed. Install it, then sign in with `codex login`.");
        }
        let directory = temporary_directory("kodem-codex")?;
        let output_path = directory.join("response.txt");
        let schema_path = directory.join("schema.json");
        let structured = schema.is_some();
        let mut process = command("codex");
        process.args([
            "exec",
            "--skip-git-repo-check",
            "--ephemeral",
            "--sandbox",
            "read-only",
            "--color",
            "never",
            "--json",
            "--cd",
        ]);
        process.arg(&directory);
        if !self.config.codex_model.trim().is_empty() {
            process.arg("--model").arg(&self.config.codex_model);
        }
        if !self.config.codex_reasoning_effort.trim().is_empty() {
            process.arg("--config").arg(format!(
                "model_reasoning_effort=\"{}\"",
                self.config.codex_reasoning_effort
            ));
        }
        if let Some(schema) = schema {
            std::fs::write(&schema_path, serde_json::to_vec_pretty(&schema)?)?;
            process.arg("--output-schema").arg(&schema_path);
        }
        process.arg("--output-last-message").arg(&output_path);
        process.arg("-");
        process
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = process.spawn().context("Kodem could not start Codex CLI")?;
        progress(AiProgress::Status("Codex is starting the task...".into()));
        let prompt = format!("SYSTEM INSTRUCTIONS:\n{system}\n\nUSER REQUEST:\n{user}");
        child
            .stdin
            .take()
            .expect("Codex stdin must be piped")
            .write_all(prompt.as_bytes())?;
        let stdout = child.stdout.take().context("Codex did not open stdout")?;
        let mut stderr = child.stderr.take().context("Codex did not open stderr")?;
        let stderr_task = std::thread::spawn(move || {
            let mut output = String::new();
            let _ = stderr.read_to_string(&mut output);
            output
        });
        let mut last_message = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Some(event) = parse_codex_stream_event(&line, structured) else {
                continue;
            };
            if let Some(message) = event.final_message {
                last_message = message;
            }
            for update in event.progress {
                progress(update);
            }
        }
        let status = child.wait()?;
        let error = stderr_task.join().unwrap_or_default();
        let response = std::fs::read_to_string(&output_path).unwrap_or(last_message);
        let _ = std::fs::remove_dir_all(&directory);
        if !status.success() {
            bail!("Codex request failed: {}", concise_error(&error));
        }
        if response.trim().is_empty() {
            bail!("Codex returned an empty response.");
        }
        Ok(response)
    }
}

#[derive(Debug, Clone, Copy)]
enum Feature {
    Commit,
    Review,
    Explain,
    PullRequestReview,
}

impl Feature {
    fn model(self, config: &AiConfig) -> &str {
        match self {
            Self::Commit => &config.commit_model,
            Self::Review | Self::PullRequestReview => &config.review_model,
            Self::Explain => &config.explain_model,
        }
    }
}

#[derive(Deserialize)]
struct OllamaGenerateChunk {
    #[serde(default)]
    response: String,
    #[serde(default)]
    thinking: String,
    #[serde(default)]
    error: Option<String>,
}

struct CodexStreamEvent {
    progress: Vec<AiProgress>,
    final_message: Option<String>,
}

fn parse_codex_stream_event(line: &str, structured: bool) -> Option<CodexStreamEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    let event_type = value.get("type")?.as_str()?;
    let item_type = value.pointer("/item/type").and_then(Value::as_str);
    let item_text = value
        .pointer("/item/text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string);
    let mut event = CodexStreamEvent {
        progress: Vec::new(),
        final_message: None,
    };

    match (event_type, item_type) {
        ("turn.started", _) => event.progress.push(AiProgress::Status(
            "Codex is analyzing the request...".into(),
        )),
        ("item.started", Some("reasoning")) => event
            .progress
            .push(AiProgress::Status("Codex is reasoning...".into())),
        ("item.started", Some("agent_message")) => event.progress.push(AiProgress::Status(
            "Codex is writing the response...".into(),
        )),
        ("item.started", Some("command_execution")) => event.progress.push(AiProgress::Status(
            "Codex is checking the supplied context...".into(),
        )),
        ("item.completed", Some("reasoning")) => {
            if let Some(text) = item_text {
                event.progress.push(AiProgress::ThinkingDelta(text));
            }
        }
        ("item.completed", Some("agent_message")) => {
            if let Some(text) = item_text {
                if !structured {
                    event.progress.push(AiProgress::ResponseDelta(text.clone()));
                }
                event.final_message = Some(text);
            }
        }
        ("turn.completed", _) => event.progress.push(AiProgress::Status(
            "Kodem is formatting the result...".into(),
        )),
        _ => {}
    }

    (!event.progress.is_empty() || event.final_message.is_some()).then_some(event)
}

/// A model installed in Ollama, with its size in bytes.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

/// Returns the models installed in the Ollama server at `base_url`, sorted by name. Fails when the
/// server does not answer within 5 seconds.
pub fn ollama_models(base_url: &str) -> Result<Vec<OllamaModel>> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let output = command("curl")
        .args(["-sS", "--fail-with-body", "--max-time", "5", &url])
        .output()
        .context("Kodem could not start curl")?;
    if !output.status.success() {
        bail!("Ollama is not available at {base_url}.");
    }
    let mut response: OllamaTagsResponse =
        serde_json::from_slice(&output.stdout).context("Ollama returned an invalid model list")?;
    response.models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(response.models)
}

/// Downloads a model into the Ollama server at `base_url`. Blocks until the download finishes, for
/// up to 30 minutes.
pub fn pull_ollama_model(base_url: &str, model: &str) -> Result<()> {
    if model.trim().is_empty() {
        bail!("Enter a model name first.");
    }
    let url = format!("{}/api/pull", base_url.trim_end_matches('/'));
    let body = json!({ "name": model, "stream": false }).to_string();
    let mut child = command("curl")
        .args([
            "-sS",
            "--fail-with-body",
            "--max-time",
            "1800",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
            &url,
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("curl stdin must be piped")
        .write_all(body.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "Ollama could not install the model: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Returns whether the Codex CLI is installed and runs.
pub fn codex_available() -> bool {
    command("codex")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Returns the sign-in status that `codex login status` prints. Fails when the CLI is missing or
/// reports an error.
pub fn codex_status() -> Result<String> {
    let output = command("codex")
        .args(["login", "status"])
        .output()
        .context("Codex CLI is not installed")?;
    let text = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    if !output.status.success() {
        bail!("{}", concise_error(&String::from_utf8_lossy(text)));
    }
    Ok(String::from_utf8_lossy(text).trim().to_string())
}

/// A model that the Codex CLI offers, with its supported and default reasoning efforts.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModel {
    pub model: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub supported_reasoning_efforts: Vec<CodexReasoningEffort>,
    pub default_reasoning_effort: String,
    #[serde(default)]
    pub is_default: bool,
}

/// One reasoning effort that a Codex model supports.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexReasoningEffort {
    pub reasoning_effort: String,
}

/// The signed-in Codex account and the models it can use.
pub struct CodexInfo {
    pub account: String,
    pub models: Vec<CodexModel>,
}

/// Reads the Codex account and the visible models from the Codex App Server. Starts the server,
/// waits up to 8 seconds for each answer, then stops it.
pub fn codex_info() -> Result<CodexInfo> {
    let child = command("codex")
        .args(["app-server", "--listen", "stdio://"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Codex CLI is not installed")?;
    read_codex_info(child)
}

fn read_codex_info(mut child: Child) -> Result<CodexInfo> {
    let result = read_codex_info_from_transport(&mut child);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn read_codex_info_from_transport(child: &mut Child) -> Result<CodexInfo> {
    let stdout = child.stdout.take().context("Codex did not open stdout")?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });
    let mut stdin = child.stdin.take().context("Codex did not open stdin")?;
    send_codex_request(
        &mut stdin,
        json!({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "Kodem", "title": "Kodem", "version": env!("CARGO_PKG_VERSION")}}}),
    )?;
    let initialize = receive_codex_response(&receiver, 1)?;
    require_codex_result(&initialize, "Codex initialization")?;

    send_codex_request(&mut stdin, json!({"method": "initialized", "params": {}}))?;
    send_codex_request(
        &mut stdin,
        json!({"id": 2, "method": "account/read", "params": {"refreshToken": true}}),
    )?;
    let account = receive_codex_response(&receiver, 2)?;
    require_codex_result(&account, "Codex account check")?;

    send_codex_request(
        &mut stdin,
        json!({"id": 3, "method": "model/list", "params": {"limit": 100, "includeHidden": false}}),
    )?;
    let models = receive_codex_response(&receiver, 3)?;
    require_codex_result(&models, "Codex model list")?;
    let models = models
        .pointer("/result/data")
        .cloned()
        .context("Codex model list did not contain data")
        .and_then(|data| {
            serde_json::from_value(data).context("Codex returned an invalid model list")
        })?;

    Ok(CodexInfo {
        account: parse_codex_account(&account),
        models,
    })
}

fn send_codex_request(stdin: &mut impl Write, value: Value) -> Result<()> {
    writeln!(stdin, "{value}")?;
    stdin.flush()?;
    Ok(())
}

fn receive_codex_response(
    receiver: &std::sync::mpsc::Receiver<String>,
    request_id: u64,
) -> Result<Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let line = receiver.recv_timeout(remaining).with_context(|| {
            format!("Codex App Server closed before request {request_id} completed")
        })?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("id").and_then(Value::as_u64) == Some(request_id) {
            return Ok(value);
        }
    }
}

fn require_codex_result<'a>(value: &'a Value, operation: &str) -> Result<&'a Value> {
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("The request failed.");
        bail!("{operation} failed: {message}");
    }
    value
        .get("result")
        .with_context(|| format!("{operation} did not return a result"))
}

fn parse_codex_account(value: &Value) -> String {
    let Some(account) = value
        .pointer("/result/account")
        .filter(|account| !account.is_null())
    else {
        return "Codex is not signed in.".into();
    };
    let email = account
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or("ChatGPT account");
    let plan = account
        .get("planType")
        .and_then(Value::as_str)
        .unwrap_or("unknown plan");
    format!("Signed in as {email} · {plan}")
}

/// Runs `codex login`, which blocks until the sign-in finishes, then returns the new sign-in
/// status.
pub fn codex_login() -> Result<String> {
    let output = command("codex")
        .arg("login")
        .output()
        .context("Codex CLI is not installed")?;
    if !output.status.success() {
        bail!(
            "Codex sign-in failed: {}",
            concise_error(&String::from_utf8_lossy(&output.stderr))
        );
    }
    codex_status()
}

/// Signs out of the Codex CLI.
pub fn codex_logout() -> Result<()> {
    let output = command("codex")
        .arg("logout")
        .output()
        .context("Codex CLI is not installed")?;
    if !output.status.success() {
        bail!(
            "Codex sign-out failed: {}",
            concise_error(&String::from_utf8_lossy(&output.stderr))
        );
    }
    Ok(())
}

fn command(program: &str) -> Command {
    crate::process::command(program)
}

fn temporary_directory(prefix: &str) -> Result<PathBuf> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

fn concise_error(value: &str) -> String {
    value
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("The command failed.")
        .trim()
        .to_string()
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
