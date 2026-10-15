use super::{AiConfig, ChangeContext, CommitGenerationTarget, PullRequestContext};

const COMMIT_LIMIT: usize = 16_000;
const EXPLAIN_LIMIT: usize = 16_000;
const REVIEW_LIMIT: usize = 16_000;

/// Builds the system and user prompts that ask for a commit title or description. A custom commit
/// prompt replaces the default system prompt. The diff leaves out dotfiles and is cut at 16,000
/// characters.
pub(super) fn commit(
    config: &AiConfig,
    context: &ChangeContext,
    target: CommitGenerationTarget,
) -> (String, String) {
    let task = commit_task(target);
    let system = custom_or(
        &config.commit_prompt,
        &format!(
            "You are a senior developer. Write one part of a Git commit message for all supplied changes. Return JSON with title and description. {task}"
        ),
    );
    let mut user = format!(
        "STAT:\n{}\n\nFILES CHANGED:\n{}\n\nDIFF:\n{}",
        context.stat,
        context.files,
        clean_diff_for_prompt(&context.diff, COMMIT_LIMIT)
    );
    if !context.file_contents.is_empty() {
        user.push_str("\n\nFULL FILE CONTENTS:");
        for (path, contents) in &context.file_contents {
            user.push_str(&format!("\n\n--- {path} ---\n{contents}"));
        }
    }
    (system, user)
}

fn commit_task(target: CommitGenerationTarget) -> &'static str {
    match target {
        CommitGenerationTarget::Title => {
            "Write the commit title. The title must use an imperative verb, cover the complete change, have no trailing period, and use at most 72 characters. Return an empty description."
        }
        CommitGenerationTarget::Description => {
            "Write the commit description. The description must use one to three clear sentences that explain the important changes. Return an empty title."
        }
    }
}

/// Builds the system and user prompts for a review of local changes. A custom review prompt
/// replaces the default system prompt. The diff leaves out dotfiles and is cut at 16,000
/// characters.
pub(super) fn review(config: &AiConfig, context: &ChangeContext) -> (String, String) {
    let system = custom_or(
        &config.review_prompt,
        "You are a strict code reviewer and security auditor. Review only issues that the diff proves. Return JSON with summary and findings. Each finding needs file, lineHint, severity, category, title, impact, evidence, recommendation, and codeSnippet. Severity is critical, warning, or info. Category is bug, security, improvement, deletion, or sensitive_data. Check for data loss, real secrets, security regressions, crashes, logic errors, race conditions, and missing error handling. Do not report style preferences. Return an empty findings array when there is no actionable issue.",
    );
    let user = format!(
        "STAT:\n{}\n\nFILES CHANGED:\n{}\n\nDIFF:\n{}",
        context.stat,
        context.files,
        clean_diff_for_prompt(&context.diff, REVIEW_LIMIT)
    );
    (system, user)
}

/// Builds the system and user prompts that ask for a Markdown explanation of a diff. A custom
/// explain prompt replaces the default system prompt. The diff leaves out dotfiles and is cut at
/// 16,000 characters.
pub(super) fn explain(
    config: &AiConfig,
    diff: &str,
    file_path: Option<&str>,
    commit_message: Option<&str>,
) -> (String, String) {
    let system = custom_or(
        &config.explain_prompt,
        "You are a senior developer. Explain the supplied diff in Markdown. Use ## Summary first and ## Important Changes second. Each important change must be a bullet with an explanation. Add ## Risks or ## Notes only for concrete facts. Do not repeat the diff.",
    );
    let mut parts = Vec::new();
    if let Some(message) = commit_message {
        parts.push(format!("COMMIT MESSAGE:\n{message}"));
    }
    if let Some(path) = file_path {
        parts.push(format!("FILE: {path}"));
    }
    parts.push(format!(
        "DIFF:\n{}",
        clean_diff_for_prompt(diff, EXPLAIN_LIMIT)
    ));
    (system, parts.join("\n\n"))
}

/// Builds the system and user prompts for a pull request review. A custom pull request review
/// prompt replaces the default system prompt. The diff leaves out dotfiles and is cut at 16,000
/// characters.
pub(super) fn pull_request_review(
    config: &AiConfig,
    context: &PullRequestContext,
) -> (String, String) {
    let system = custom_or(
        &config.pull_request_review_prompt,
        "You are a senior engineer who reviews a GitHub pull request. Return JSON with summary and comments. Each comment needs the exact file, side, line, startLine, severity, title, body, and suggestion. Use RIGHT for added or context lines and LEFT for deleted lines. Only report actionable issues that the diff proves. Do not report style preferences. Return an empty comments array when there is no actionable issue.",
    );
    let user = format!(
        "TITLE: {}\nAUTHOR: {}\nBASE: {}\nHEAD: {}\n\nDIFF:\n{}",
        context.title,
        context.author,
        context.base,
        context.head,
        clean_diff_for_prompt(&context.diff, REVIEW_LIMIT)
    );
    (system, user)
}

fn custom_or(custom: &str, default: &str) -> String {
    let custom = custom.trim();
    if custom.is_empty() {
        default.to_string()
    } else {
        custom.to_string()
    }
}

fn bounded(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut output = value.chars().take(limit).collect::<String>();
    output.push_str("\n\n[Diff was truncated by Kodem]");
    output
}

fn clean_diff_for_prompt(value: &str, limit: usize) -> String {
    let mut output = String::new();
    let mut include = true;
    for line in value.lines() {
        if let Some(header) = line.strip_prefix("diff --git ") {
            let path = header
                .split_whitespace()
                .next_back()
                .unwrap_or_default()
                .trim_start_matches("b/");
            include = !path.split('/').any(|component| component.starts_with('.'));
        }
        if include {
            output.push_str(line);
            output.push('\n');
        }
    }
    bounded(output.trim_end(), limit)
}
