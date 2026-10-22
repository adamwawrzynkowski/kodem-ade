//! The AI page: the Kodem agent that runs AI features, and the switches and prompts of commit
//! messages, code review, diff explanations and pull request reviews.
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Div, Entity, div};

use super::{GitSettingsView, page, section, setting_row, toggle_row, wide_setting_row};
use crate::theme::{px, text_px};
use crate::ui::git::ai::AiAgentState;
use crate::ui::git::components::mono_font;
use crate::ui::git::kit::{ActiveTheme as _, Textarea, TextareaState};
use crate::ui::git::settings::GitSettings;

impl GitSettingsView {
    /// Returns the AI page.
    pub(super) fn render_ai(&mut self, cx: &mut Context<Self>) -> Div {
        let s = GitSettings::get(cx).clone();
        let agent = AiAgentState::get(cx);
        let choice = format!(
            "{} · {} · {}",
            agent.agent.provider.label(),
            agent.model,
            agent.effort
        );

        page()
            .child(
                section("Agent", cx).child(setting_row(
                    "Provider, model and effort",
                    "Choose them with the buttons beside each AI action. Kodem's own tasks keep their own choices.",
                    div()
                        .font_family(mono_font())
                        .text_size(text_px(12.))
                        .text_color(cx.theme().muted_foreground)
                        .child(choice),
                    cx,
                )),
            )
            .child(
                section("Commit Message Generation", cx)
                    .child(toggle_row(
                        "ai-commit",
                        "AI commit messages",
                        "Generate a commit title from staged changes",
                        s.ai_commit_messages,
                        |v, cx| GitSettings::update(cx, |s| s.ai_commit_messages = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "ai-description",
                        "Generate description",
                        "Enable the Generate description AI action",
                        s.ai_generate_description,
                        |v, cx| GitSettings::update(cx, |s| s.ai_generate_description = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "ai-whole-file",
                        "Include complete files",
                        "Give the model more context from staged file contents",
                        s.ai_include_whole_file,
                        |v, cx| GitSettings::update(cx, |s| s.ai_include_whole_file = v),
                        cx,
                    ))
                    .child(prompt_row(&self.commit_prompt, cx)),
            )
            .child(
                section("Code Review", cx)
                    .child(toggle_row(
                        "ai-review",
                        "AI code review",
                        "Review staged changes before commit",
                        s.ai_code_review,
                        |v, cx| GitSettings::update(cx, |s| s.ai_code_review = v),
                        cx,
                    ))
                    .child(prompt_row(&self.review_prompt, cx)),
            )
            .child(
                section("Diff Explanation", cx)
                    .child(toggle_row(
                        "ai-explain",
                        "AI diff explanations",
                        "Explain a selected change or commit",
                        s.ai_explain_diffs,
                        |v, cx| GitSettings::update(cx, |s| s.ai_explain_diffs = v),
                        cx,
                    ))
                    .child(prompt_row(&self.explain_prompt, cx)),
            )
            .child(
                section("Pull Request Review", cx)
                    .child(toggle_row(
                        "ai-pr-review",
                        "AI pull request review",
                        "Review a GitHub pull request diff",
                        s.ai_pull_request_review,
                        |v, cx| GitSettings::update(cx, |s| s.ai_pull_request_review = v),
                        cx,
                    ))
                    .child(prompt_row(&self.pull_request_review_prompt, cx)),
            )
    }
}

// Returns the custom prompt row of an AI feature.
fn prompt_row(state: &Entity<TextareaState>, cx: &App) -> AnyElement {
    wide_setting_row(
        "Custom prompt",
        "Leave this empty to use the built-in prompt",
        Textarea::new(state).h(px(88.)),
        cx,
    )
}
