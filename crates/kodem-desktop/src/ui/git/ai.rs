//! AI in the Git workspace: the agent that its actions run with and the buttons that choose it,
//! the dialog that shows an explanation, a code review or a pull request review, with live
//! model activity while the request runs, and the runner that streams a background AI task's
//! progress into that dialog.
use futures::{StreamExt as _, channel::mpsc};
use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    Action, App, ClipboardItem, Context, Div, ElementId, Entity, FontWeight, Global, Hsla,
    IntoElement, Render, RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
};
use kodem_git::ai::{
    AiProgress, CodeReview, PullRequestComment, PullRequestReview, ReviewFinding, ReviewSeverity,
};
use kodem_git::settings::AiAgent;

use crate::components::text_input::TextInput;
use crate::theme::{RADIUS_SMALL, RADIUS_SURFACE, px, text_px};
use crate::ui::dither::{Dither, DitherField, Edge};
use crate::ui::git::components::{button, dialog_header, mono_font, mono_label};
use crate::ui::git::contextual_actions::{divider, open_liquid_dialog};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, Icon, IconName, ScrollableElement as _, Sizable as _,
    Size, TextView, WindowExt as _, h_flex, v_flex,
};
use crate::ui::git::motion;
use crate::ui::git::syntax::DiffSyntaxHighlighter;
use crate::ui::git::theme;
use crate::ui::{fx, kit};

const MAX_LIVE_TEXT_BYTES: usize = 8_192;

/// The agent that the Git workspace's AI actions run with, as the window resolves it from the
/// Git settings and Kodem's provider catalogs, with the names its buttons show. The window
/// keeps it current.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiAgentState {
    /// The provider, account, model and effort that the next action runs with.
    pub agent: AiAgent,
    /// The display name of the model.
    pub model: SharedString,
    /// The name of the effort in use: the chosen one, the model's default, or "Default".
    pub effort: SharedString,
}

impl Global for AiAgentState {}

impl AiAgentState {
    /// Returns the agent of the Git workspace's AI actions.
    pub fn get(cx: &App) -> Self {
        cx.try_global::<Self>().cloned().unwrap_or_default()
    }
}

/// Asks the window to open the agent menu of the Git workspace's AI actions beside the control
/// whose effect anchor is `anchor`: the provider and model menu, or with `effort` the reasoning
/// effort menu.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = kodem_git, no_json)]
pub struct ChooseAiAgent {
    pub anchor: SharedString,
    pub effort: bool,
}

/// Returns an AI action joined with its agent in one bordered container, with dividers between
/// the parts: the action, the provider's logo, which opens the provider and model menu, and the
/// reasoning effort, which opens the effort menu. Both menus choose for every AI action of the
/// Git workspace. `action` is a ghost button of `size`; `id` makes the identities and anchors of
/// the agent buttons unique.
pub fn ai_action_group(
    id: impl Into<SharedString>,
    action: impl IntoElement,
    size: Size,
    cx: &App,
) -> Div {
    let id: SharedString = id.into();
    let state = AiAgentState::get(cx);
    let model_anchor: SharedString = format!("git-ai-model:{id}").into();
    let effort_anchor: SharedString = format!("git-ai-effort:{id}").into();
    let model_hint = format!(
        "{} · {}. Choose the provider and model of Git AI actions.",
        state.agent.provider.label(),
        state.model
    );
    let effort_hint = format!(
        "Reasoning effort: {}. Choose the effort of Git AI actions.",
        state.effort
    );
    h_flex()
        .flex_shrink_0()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .overflow_hidden()
        .child(action)
        .child(group_divider(cx))
        .child(agent_button(
            model_anchor,
            Icon::named(state.agent.provider.icon()),
            None,
            model_hint,
            false,
            size,
        ))
        .child(group_divider(cx))
        .child(agent_button(
            effort_anchor,
            Icon::named("gauge"),
            Some(state.effort),
            effort_hint,
            true,
            size,
        ))
}

// Returns a button of an AI action group that asks the window to open an agent menu beside it.
fn agent_button(
    anchor: SharedString,
    icon: Icon,
    label: Option<SharedString>,
    hint: String,
    effort: bool,
    size: Size,
) -> Div {
    let request = ChooseAiAgent {
        anchor: anchor.clone(),
        effort,
    };
    div()
        .relative()
        .child(
            button(ElementId::Name(anchor.clone()))
                .ghost()
                .with_size(size)
                .icon(icon)
                .when_some(label, |button, label| button.label(label))
                .tooltip(hint)
                .on_click(move |_, window, cx| {
                    // A row that holds the group, such as a pull request's, keeps its own click.
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(request.clone()), cx);
                }),
        )
        .child(fx::anchor(anchor))
}

// Returns the line between two parts of an AI action group.
fn group_divider(cx: &App) -> Div {
    div().w(px(1.)).h(px(14.)).bg(cx.theme().border)
}

/// What an AI result dialog shows.
#[derive(Debug, Clone)]
pub enum AiResult {
    Loading,
    Error(String),
    Explanation(String),
    CodeReview(CodeReview),
    PullRequestReview(PullRequestReview),
}

/// A dialog that shows the live activity of an AI request and then its result.
pub struct AiResultDialogView {
    title: SharedString,
    detail: SharedString,
    result: AiResult,
    activity: AiActivity,
}

struct AiActivity {
    status: SharedString,
    thinking: String,
    response: String,
    status_sequence: u64,
    thinking_sequence: u64,
    response_sequence: u64,
}

impl Default for AiActivity {
    fn default() -> Self {
        Self {
            status: "Preparing the AI request...".into(),
            thinking: String::new(),
            response: String::new(),
            status_sequence: 0,
            thinking_sequence: 0,
            response_sequence: 0,
        }
    }
}

impl AiResultDialogView {
    /// Creates a dialog that waits for its result.
    pub fn new(title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            result: AiResult::Loading,
            activity: AiActivity::default(),
        }
    }

    /// Shows the result of the request.
    pub fn set_result(&mut self, result: AiResult, cx: &mut Context<Self>) {
        self.result = result;
        cx.notify();
    }

    /// Shows the progress of the running request: its status and the streamed thinking and
    /// response text. Progress that arrives after the result is ignored.
    pub fn set_progress(&mut self, progress: AiProgress, cx: &mut Context<Self>) {
        if !matches!(self.result, AiResult::Loading) {
            return;
        }
        match progress {
            AiProgress::Status(status) => {
                if self.activity.status != status.as_str() {
                    self.activity.status = status.into();
                    self.activity.status_sequence += 1;
                }
            }
            AiProgress::ThinkingDelta(delta) => {
                if self.activity.thinking.is_empty() {
                    self.activity.thinking_sequence += 1;
                }
                append_live_text(&mut self.activity.thinking, &delta);
            }
            AiProgress::ResponseDelta(delta) => {
                if self.activity.response.is_empty() {
                    self.activity.response_sequence += 1;
                }
                append_live_text(&mut self.activity.response, &delta);
            }
        }
        cx.notify();
    }

    fn severity_color(severity: &ReviewSeverity, cx: &App) -> Hsla {
        let colors = theme::liquid_colors(cx);
        match severity {
            ReviewSeverity::Critical => colors.danger,
            ReviewSeverity::Warning => colors.warning,
            ReviewSeverity::Info => cx.theme().primary,
        }
    }

    fn render_code_review(&self, review: &CodeReview, cx: &App) -> Div {
        v_flex()
            .w_full()
            .gap_3()
            .child(summary_card(
                "code-review-summary",
                &review.summary.headline,
                &review.summary.overview,
                cx,
            ))
            .when(review.findings.is_empty(), |this| {
                this.child(empty_result(
                    "code-review-empty",
                    "No actionable issues were found.",
                    cx,
                ))
            })
            .children(review.findings.iter().enumerate().map(|(ix, finding)| {
                let color = Self::severity_color(&finding.severity, cx);
                let finding_to_copy = code_review_finding_copy_text(finding);
                v_flex()
                    .w_full()
                    .gap_2()
                    .p_3()
                    .rounded(px(RADIUS_SURFACE))
                    .border_1()
                    .border_color(color.opacity(0.35))
                    .bg(color.opacity(0.08))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_start()
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded(px(RADIUS_SMALL))
                                    .bg(color.opacity(0.18))
                                    .font_family(mono_font())
                                    .text_size(text_px(10.5))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(color)
                                    .child(selectable_plain_text(
                                        format!("review-severity-{ix}"),
                                        format!("{:?}", finding.severity),
                                    )),
                            )
                            .child(
                                selectable_plain_text(
                                    format!("review-title-{ix}"),
                                    finding.title.clone(),
                                )
                                .flex_1()
                                .text_size(text_px(12.5))
                                .font_weight(FontWeight::SEMIBOLD),
                            )
                            .child(
                                button(("copy-review-finding", ix))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Copy)
                                    .tooltip("Copy finding")
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            finding_to_copy.clone(),
                                        ));
                                    }),
                            ),
                    )
                    .child(
                        selectable_plain_text(
                            format!("review-location-{ix}"),
                            match &finding.line_hint {
                                Some(line) => format!("{} · {line}", finding.file),
                                None => finding.file.clone(),
                            },
                        )
                        .font_family(mono_font())
                        .text_size(text_px(11.5))
                        .text_color(cx.theme().muted_foreground),
                    )
                    .child(detail_block(
                        "review-impact",
                        ix,
                        "Impact",
                        &finding.impact,
                        cx,
                    ))
                    .child(detail_block(
                        "review-evidence",
                        ix,
                        "Evidence",
                        &finding.evidence,
                        cx,
                    ))
                    .child(detail_block(
                        "review-recommendation",
                        ix,
                        "Recommendation",
                        &finding.recommendation,
                        cx,
                    ))
                    .when_some(finding.code_snippet.clone(), |this, snippet| {
                        this.child(code_block(
                            format!("review-code-{ix}"),
                            ("copy-review-code", ix),
                            &finding.file,
                            &snippet,
                        ))
                    })
            }))
    }

    fn render_pull_request_review(&self, review: &PullRequestReview, cx: &App) -> Div {
        v_flex()
            .w_full()
            .gap_3()
            .child(summary_card(
                "pull-request-review-summary",
                &review.summary.headline,
                &review.summary.overview,
                cx,
            ))
            .when(review.comments.is_empty(), |this| {
                this.child(empty_result(
                    "pull-request-review-empty",
                    "No actionable issues were found.",
                    cx,
                ))
            })
            .children(review.comments.iter().enumerate().map(|(ix, comment)| {
                let color = Self::severity_color(&comment.severity, cx);
                let comment_to_copy = pull_request_comment_copy_text(comment);
                v_flex()
                    .w_full()
                    .gap_2()
                    .p_3()
                    .rounded(px(RADIUS_SURFACE))
                    .border_1()
                    .border_color(color.opacity(0.35))
                    .bg(color.opacity(0.08))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_start()
                            .child(
                                selectable_plain_text(
                                    format!("pull-request-review-title-{ix}"),
                                    comment.title.clone(),
                                )
                                .flex_1()
                                .text_size(text_px(12.5))
                                .font_weight(FontWeight::SEMIBOLD),
                            )
                            .child(
                                selectable_plain_text(
                                    format!("pull-request-review-location-{ix}"),
                                    format!("{}:{} {}", comment.file, comment.line, comment.side),
                                )
                                .text_size(text_px(11.5))
                                .font_family(mono_font())
                                .text_color(color),
                            )
                            .child(
                                button(("copy-pull-request-review-comment", ix))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Copy)
                                    .tooltip("Copy review comment")
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            comment_to_copy.clone(),
                                        ));
                                    }),
                            ),
                    )
                    .child(
                        div().text_size(text_px(11.5)).child(
                            TextView::markdown(
                                format!("pull-request-review-body-{ix}"),
                                comment.body.clone(),
                            )
                            .selectable(true),
                        ),
                    )
                    .when_some(comment.suggestion.clone(), |this, suggestion| {
                        this.child(code_block(
                            format!("pull-request-review-code-{ix}"),
                            ("copy-pr-review-code", ix),
                            &comment.file,
                            &suggestion,
                        ))
                    })
            }))
    }

    fn render_live_activity(&self, cx: &App) -> Div {
        let status = self.activity.status.clone();
        let thinking = self.activity.thinking.clone();
        let response = self.activity.response.clone();
        let status_sequence = self.activity.status_sequence;
        let thinking_sequence = self.activity.thinking_sequence;
        let response_sequence = self.activity.response_sequence;

        v_flex().w_full().items_center().py_2().child(
            v_flex()
                .w_full()
                .max_w(px(560.))
                .gap_3()
                .child(motion::floating_surface(
                    "ai-progress-status-motion",
                    status_sequence,
                    div()
                        .relative()
                        .w_full()
                        .overflow_hidden()
                        .rounded(px(RADIUS_SURFACE))
                        .border_1()
                        .border_color(cx.theme().primary.opacity(0.3))
                        .bg(cx.theme().primary.opacity(0.05))
                        .child(
                            Dither::new(
                                DitherField::Flow {
                                    scale: 0.16,
                                    speed: 3.5,
                                    bias: -0.2,
                                }
                                .times(DitherField::Fade {
                                    from: Edge::Right,
                                    start: 1.0,
                                    end: 0.0,
                                }),
                            )
                            .color(cx.theme().primary.opacity(0.45))
                            .grid(3.0, 2.0)
                            .animated()
                            .corner_radius(RADIUS_SURFACE)
                            .layer(),
                        )
                        .child(
                            h_flex()
                                .relative()
                                .w_full()
                                .gap_2()
                                .p_3()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(text_px(12.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(status),
                                )
                                .child(kit::chip("LIVE", cx.theme().primary)),
                        ),
                    cx,
                ))
                .when(!thinking.is_empty(), |this| {
                    this.child(motion::floating_surface(
                        "ai-progress-thinking-motion",
                        thinking_sequence,
                        live_text_card("ai-live-thinking", "Model activity", thinking, cx),
                        cx,
                    ))
                })
                .when(!response.is_empty(), |this| {
                    this.child(motion::floating_surface(
                        "ai-progress-response-motion",
                        response_sequence,
                        live_text_card("ai-live-response", "Draft response", response, cx),
                        cx,
                    ))
                })
                .when(
                    self.activity.thinking.is_empty() && self.activity.response.is_empty(),
                    |this| {
                        this.child(
                            div()
                                .w_full()
                                .px_3()
                                .text_size(text_px(11.5))
                                .text_color(cx.theme().muted_foreground)
                                .child("Live model activity will appear here."),
                        )
                    },
                ),
        )
    }
}

impl Render for AiResultDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::liquid_colors(cx);
        let body = match &self.result {
            AiResult::Loading => self.render_live_activity(cx).into_any_element(),
            AiResult::Error(error) => v_flex()
                .w_full()
                .p_3()
                .rounded(px(RADIUS_SURFACE))
                .bg(colors.danger.opacity(0.12))
                .text_size(text_px(11.5))
                .text_color(colors.danger)
                .child(selectable_plain_text("ai-result-error", error.clone()))
                .into_any_element(),
            AiResult::Explanation(markdown) => div()
                .w_full()
                .text_size(text_px(11.5))
                .child(
                    TextView::markdown("ai-result-explanation", markdown.clone()).selectable(true),
                )
                .into_any_element(),
            AiResult::CodeReview(review) => self.render_code_review(review, cx).into_any_element(),
            AiResult::PullRequestReview(review) => self
                .render_pull_request_review(review, cx)
                .into_any_element(),
        };

        v_flex()
            .w_full()
            .max_h(px(680.))
            .overflow_hidden()
            .child(dialog_header(
                icons::sparkles(),
                selectable_plain_text("ai-result-title", self.title.clone()),
                selectable_plain_text("ai-result-detail", self.detail.clone()),
                cx.theme().primary,
                cx,
            ))
            .child(divider(cx))
            .child(
                div()
                    .id("ai-result-scroll")
                    .w_full()
                    .max_h(px(520.))
                    .overflow_y_scrollbar()
                    .p_4()
                    .child(body),
            )
            .child(divider(cx))
            .child(
                h_flex().w_full().justify_end().px_4().py_3().child(
                    button("close-ai-result")
                        .outline()
                        .icon(IconName::Close)
                        .label("Close")
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                ),
            )
    }
}

/// Opens a dialog that waits for an AI result, and returns it for [`run_ai_result_task`].
pub fn open_ai_result_dialog(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<AiResultDialogView> {
    let view = cx.new(|_| AiResultDialogView::new(title, detail));
    open_liquid_dialog(view.clone(), px(680.), window, cx);
    view
}

/// Runs an AI operation on a background thread, streams its progress into `dialog`, and shows
/// the result that `map_result` makes, or the error.
pub fn run_ai_result_task<C, T, F, M>(
    dialog: Entity<AiResultDialogView>,
    operation: F,
    map_result: M,
    cx: &mut Context<C>,
) where
    C: 'static,
    T: Send + 'static,
    F: FnOnce(&mut dyn FnMut(AiProgress)) -> anyhow::Result<T> + Send + 'static,
    M: FnOnce(T) -> AiResult + 'static,
{
    let (sender, mut receiver) = mpsc::unbounded();
    let task = cx.background_executor().spawn(async move {
        let mut report = move |progress| {
            let _ = sender.unbounded_send(progress);
        };
        operation(&mut report)
    });
    cx.spawn(async move |_, cx| {
        while let Some(progress) = receiver.next().await {
            dialog.update(cx, |dialog, cx| dialog.set_progress(progress, cx));
        }
        let result = task.await;
        dialog.update(cx, |dialog, cx| {
            dialog.set_result(
                result
                    .map(map_result)
                    .unwrap_or_else(|error| AiResult::Error(error.to_string())),
                cx,
            );
        });
    })
    .detach();
}

fn summary_card(id_prefix: &'static str, headline: &str, overview: &str, cx: &App) -> Div {
    v_flex()
        .w_full()
        .gap_1()
        .p_3()
        .rounded(px(RADIUS_SURFACE))
        .border_1()
        .border_color(cx.theme().border)
        .bg(theme::liquid_colors(cx).panel_secondary)
        .child(
            selectable_plain_text(format!("{id_prefix}-headline"), headline.to_string())
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(
            div()
                .text_size(text_px(11.5))
                .text_color(cx.theme().muted_foreground)
                .child(
                    TextView::markdown(format!("{id_prefix}-overview"), overview.to_string())
                        .selectable(true),
                ),
        )
}

fn empty_result(id: &'static str, message: &str, cx: &App) -> Div {
    div()
        .w_full()
        .p_4()
        .text_size(text_px(11.5))
        .text_color(cx.theme().muted_foreground)
        .child(selectable_plain_text(id, message.to_string()))
}

fn detail_block(id_prefix: &'static str, ix: usize, label: &str, value: &str, cx: &App) -> Div {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .font_family(mono_font())
                .text_size(text_px(10.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::liquid_colors(cx).text_tertiary)
                .child(selectable_plain_text(
                    format!("{id_prefix}-{ix}-label"),
                    label.to_uppercase(),
                )),
        )
        .child(
            div().text_size(text_px(11.5)).child(
                TextView::markdown(format!("{id_prefix}-{ix}-value"), value.to_string())
                    .selectable(true),
            ),
        )
}

/// A fenced code block in the language of `path`, with a button that copies the code.
fn code_block(text_id: String, button_id: (&'static str, usize), path: &str, code: &str) -> Div {
    let language = DiffSyntaxHighlighter::detect(path, code.lines().next())
        .as_ref()
        .map(DiffSyntaxHighlighter::language)
        .unwrap_or_else(|| "Code".into());
    let code_to_copy = code.to_string();
    div()
        .relative()
        .w_full()
        .child(TextView::markdown(text_id, markdown_code_block(&language, code)).selectable(true))
        .child(
            div().absolute().top(px(4.)).right(px(4.)).child(
                button(button_id)
                    .ghost()
                    .xsmall()
                    .icon(IconName::Copy)
                    .tooltip("Copy code")
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(code_to_copy.clone()));
                    }),
            ),
        )
}

fn live_text_card(id: &'static str, label: &str, value: String, cx: &App) -> Div {
    v_flex()
        .w_full()
        .gap_1p5()
        .p_3()
        .rounded(px(RADIUS_SURFACE))
        .border_1()
        .border_color(cx.theme().border)
        .bg(theme::liquid_colors(cx).panel_secondary)
        .child(
            h_flex()
                .gap_1p5()
                .text_color(cx.theme().primary)
                .child(icons::sparkles().xsmall())
                .child(mono_label(label.to_string(), cx).text_color(cx.theme().primary)),
        )
        .child(
            div()
                .max_h(px(190.))
                .overflow_y_scrollbar()
                .text_size(text_px(11.5))
                .text_color(cx.theme().muted_foreground)
                .child(TextView::markdown(id, live_text_preview(&value)).selectable(true)),
        )
}

/// Plain text that the user can select and copy, in the text style of its parent.
fn selectable_plain_text(
    id: impl Into<SharedString>,
    value: impl Into<SharedString>,
) -> SelectableText {
    SelectableText {
        id: id.into(),
        value: value.into(),
        style: StyleRefinement::default(),
    }
}

/// Selectable read-only text. Its id keeps the selection across redraws.
#[derive(IntoElement)]
struct SelectableText {
    id: SharedString,
    value: SharedString,
    style: StyleRefinement,
}

impl Styled for SelectableText {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SelectableText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let initial = self.value.to_string();
        let input = window.use_keyed_state(
            ElementId::Name(format!("git-ai-text:{}", self.id).into()),
            cx,
            move |_, cx| TextInput::readonly(initial, cx),
        );
        if input.read(cx).text() != self.value.as_ref() {
            input.update(cx, |input, cx| input.set_text(self.value.to_string(), cx));
        }
        let mut text = div().min_w_0().child(input);
        text.style().refine(&self.style);
        text
    }
}

fn markdown_code_block(language: &str, code: &str) -> String {
    let longest_run = code
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest_run.saturating_add(1).max(3));
    let language = if language == "Code" {
        String::new()
    } else {
        language.to_ascii_lowercase().replace(' ', "")
    };
    let trailing_newline = if code.ends_with('\n') { "" } else { "\n" };
    format!("{fence}{language}\n{code}{trailing_newline}{fence}")
}

fn code_review_finding_copy_text(finding: &ReviewFinding) -> String {
    let location = match &finding.line_hint {
        Some(line) => format!("{} · {line}", finding.file),
        None => finding.file.clone(),
    };
    let mut output = format!(
        "{:?}: {}\n{}\n\nImpact\n{}\n\nEvidence\n{}\n\nRecommendation\n{}",
        finding.severity,
        finding.title,
        location,
        finding.impact,
        finding.evidence,
        finding.recommendation,
    );
    if let Some(code) = &finding.code_snippet {
        output.push_str("\n\nCode\n");
        output.push_str(code);
    }
    output
}

fn pull_request_comment_copy_text(comment: &PullRequestComment) -> String {
    let mut output = format!(
        "{:?}: {}\n{}:{} {}\n\n{}",
        comment.severity, comment.title, comment.file, comment.line, comment.side, comment.body,
    );
    if let Some(suggestion) = &comment.suggestion {
        output.push_str("\n\nSuggestion\n");
        output.push_str(suggestion);
    }
    output
}

fn live_text_preview(value: &str) -> String {
    const MAX_PREVIEW_BYTES: usize = 2_048;
    if value.len() <= MAX_PREVIEW_BYTES {
        return value.to_string();
    }
    let mut start = value.len() - MAX_PREVIEW_BYTES;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    format!("\u{2026}{}", &value[start..])
}

fn append_live_text(target: &mut String, delta: &str) {
    target.push_str(delta);
    if target.len() <= MAX_LIVE_TEXT_BYTES {
        return;
    }
    let mut start = target.len() - MAX_LIVE_TEXT_BYTES;
    while !target.is_char_boundary(start) {
        start += 1;
    }
    target.drain(..start);
    target.insert(0, '\u{2026}');
}
