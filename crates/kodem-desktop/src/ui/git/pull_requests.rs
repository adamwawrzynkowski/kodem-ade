//! The Pull Requests tab: the open pull requests of the repository's GitHub remote, each with a
//! link to GitHub and an AI review.

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{App, Context, Entity, IntoElement, Render, SharedString, Window, div};
use kodem_git::ai::{AiConfig, AiService, PullRequestContext};
use kodem_git::github::{self, PullRequest};

use crate::theme::{RADIUS_CONTROL, RADIUS_SURFACE, px, text_px};
use crate::ui::dither::DitherField;
use crate::ui::git::ai::{AiResult, open_ai_result_dialog, run_ai_result_task};
use crate::ui::git::components::{
    button, dither_empty_state, dither_icon_tile, dither_loader, mono_font, section_header,
};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, Disableable as _, ScrollableElement as _, Sizable as _,
    h_flex, v_flex,
};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::state::RepositoryState;
use crate::ui::git::theme;
use crate::ui::kit::chip;

/// Lists the open pull requests of the repository's GitHub remote.
pub struct PullRequestsView {
    state: Entity<RepositoryState>,
    /// The pull requests from the last load.
    pub prs: Vec<PullRequest>,
    loading: bool,
    error: Option<String>,
    loaded_once: bool,
}

impl PullRequestsView {
    /// Creates the view for a repository. The list loads on first use.
    pub fn new(state: Entity<RepositoryState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            prs: Vec::new(),
            loading: false,
            error: None,
            loaded_once: false,
        }
    }

    /// The `owner/name` of the first GitHub remote.
    fn slug(&self, cx: &App) -> Option<String> {
        self.state
            .read(cx)
            .remotes
            .iter()
            .find_map(|r| github::github_slug(&r.fetch_url))
    }

    /// The token of the first linked GitHub account, if any.
    fn token(cx: &App) -> Option<String> {
        GitSettings::get(cx)
            .accounts
            .iter()
            .find(|a| a.host.contains("github"))
            .map(|a| a.token.clone())
            .filter(|t| !t.is_empty())
    }

    /// Fetches the open pull requests from GitHub.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let Some(slug) = self.slug(cx) else {
            self.error = Some("No GitHub remote found.".into());
            self.loaded_once = true;
            cx.notify();
            return;
        };
        let token = Self::token(cx);
        self.loading = true;
        self.error = None;
        self.loaded_once = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let task = cx
                .background_executor()
                .spawn(async move { github::list_pull_requests(&slug, token.as_deref()) });
            let result = task.await;
            this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(prs) => this.prs = prs,
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Loads the pull requests the first time the tab is shown.
    pub fn ensure_loaded(&mut self, cx: &mut Context<Self>) {
        if !self.loaded_once {
            self.load(cx);
        }
    }

    /// Asks the AI to review a pull request's diff and shows the result in a dialog.
    fn review_pull_request(
        &mut self,
        pull_request: PullRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slug) = self.slug(cx) else {
            return;
        };
        let token = Self::token(cx);
        let settings = GitSettings::get(cx).clone();
        let detail = format!(
            "#{} · {} → {}",
            pull_request.number, pull_request.head.name, pull_request.base.name
        );
        let dialog = open_ai_result_dialog("AI Pull Request Review", detail, window, cx);
        run_ai_result_task(
            dialog,
            move |progress| {
                let diff = github::pull_request_diff(&slug, pull_request.number, token.as_deref())?;
                let context = PullRequestContext {
                    title: pull_request.title,
                    author: pull_request.user.login,
                    base: pull_request.base.name,
                    head: pull_request.head.name,
                    diff,
                };
                AiService::new(AiConfig::from(&settings)).review_pull_request(&context, progress)
            },
            AiResult::PullRequestReview,
            cx,
        );
    }
}

impl Render for PullRequestsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let loading = self.loading;
        let error = self.error.clone();
        let prs = self.prs.clone();
        let ai_review_enabled = GitSettings::get(cx).ai_pull_request_review;
        let colors = theme::liquid_colors(cx);
        let has_slug = self.slug(cx).is_some();

        v_flex()
            .size_full()
            .bg(cx.theme().colors.list)
            .child(
                h_flex()
                    .w_full()
                    .h(px(38.))
                    .flex_shrink_0()
                    .px_3()
                    .gap_2()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().flex_1().min_w_0().child(section_header(
                        "Open pull requests",
                        Some(prs.len()),
                        cx,
                    )))
                    .child(
                        button("refresh-prs")
                            .ghost()
                            .xsmall()
                            .icon(icons::refresh())
                            .tooltip("Refresh")
                            .loading(loading)
                            .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
                    ),
            )
            .when(loading && prs.is_empty(), |this| {
                this.child(dither_loader("Loading pull requests", cx))
            })
            .when_some(error, |this, error| {
                this.child(
                    div()
                        .m_3()
                        .px_3()
                        .py_2()
                        .rounded(px(RADIUS_CONTROL))
                        .border_1()
                        .border_color(colors.danger.opacity(0.3))
                        .bg(colors.danger.opacity(0.08))
                        .font_family(mono_font())
                        .text_size(text_px(11.5))
                        .text_color(colors.danger)
                        .child(error),
                )
            })
            .when(!prs.is_empty(), |this| {
                this.child(
                    v_flex()
                        .id("pr-list")
                        .flex_1()
                        .min_h_0()
                        .p_2()
                        .gap_1()
                        .overflow_y_scrollbar()
                        .children(prs.into_iter().enumerate().map(|(ix, pr)| {
                            let url = pr.html_url.clone();
                            let pr_for_review = pr.clone();
                            let tone = if pr.draft {
                                cx.theme().muted_foreground
                            } else {
                                colors.success
                            };
                            h_flex()
                                .id(("pr", ix))
                                .w_full()
                                .px_3()
                                .py_2p5()
                                .gap_3()
                                .items_center()
                                .rounded(px(RADIUS_SURFACE))
                                .border_1()
                                .border_color(cx.theme().border)
                                .bg(colors.panel_secondary)
                                .cursor_pointer()
                                .hover(|s| s.border_color(cx.theme().primary.opacity(0.45)))
                                .active(|s| s.bg(cx.theme().button_active))
                                .on_click(move |_, _, cx| {
                                    cx.open_url(&url);
                                })
                                .child(dither_icon_tile(
                                    icons::git_pull_request(),
                                    tone,
                                    30.0,
                                    false,
                                ))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_0p5()
                                        .child(
                                            h_flex()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .text_size(text_px(12.5))
                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                        .overflow_hidden()
                                                        .whitespace_nowrap()
                                                        .child(SharedString::from(
                                                            pr.title.clone(),
                                                        )),
                                                )
                                                .when(pr.draft, |this| {
                                                    this.child(chip(
                                                        "DRAFT",
                                                        cx.theme().muted_foreground,
                                                    ))
                                                }),
                                        )
                                        .child(
                                            h_flex()
                                                .gap_2()
                                                .font_family(mono_font())
                                                .text_size(text_px(11.))
                                                .text_color(colors.text_tertiary)
                                                .child(format!("#{}", pr.number))
                                                .child(SharedString::from(pr.user.login.clone()))
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .whitespace_nowrap()
                                                        .child(format!(
                                                            "{} → {}",
                                                            pr.head.name, pr.base.name
                                                        )),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .font_family(mono_font())
                                        .text_size(text_px(11.))
                                        .text_color(colors.text_tertiary)
                                        .child(SharedString::from(
                                            pr.updated_at.chars().take(10).collect::<String>(),
                                        )),
                                )
                                .child(
                                    button(("ai-review-pr", ix))
                                        .outline()
                                        .xsmall()
                                        .icon(icons::sparkles())
                                        .label("Review")
                                        .disabled(!ai_review_enabled)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.review_pull_request(
                                                pr_for_review.clone(),
                                                window,
                                                cx,
                                            )
                                        })),
                                )
                        })),
                )
            })
            .when(
                !loading && self.prs.is_empty() && self.error.is_none(),
                |this| {
                    this.child(div().flex_1().min_h_0().child(dither_empty_state(
                        "No open pull requests",
                        if has_slug {
                            "New pull requests on GitHub will appear here."
                        } else {
                            "This repository does not have a GitHub remote."
                        },
                        DitherField::Ember {
                            scale: 0.18,
                            speed: 1.6,
                        },
                        cx,
                    )))
                },
            )
    }
}
