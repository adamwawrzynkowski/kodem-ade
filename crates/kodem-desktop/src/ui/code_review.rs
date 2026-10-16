//! The Review workspace: the picker of a pull request or a branch, the request that starts a
//! local read-only review, and the hand-over of its findings to Agentic Coding.
use super::*;

/// Height of the list of review targets, in points. It is fixed, so the start screen does not
/// move while the lists load.
const LIST_HEIGHT: f32 = 264.;

/// Most text of a review that is handed to the task that fixes its findings.
const FINDINGS_LIMIT: usize = 256 * 1024;

#[derive(Clone, Copy, Default, PartialEq)]
enum ReviewTab {
    #[default]
    PullRequests,
    Branches,
}

/// What the start screen of the Review workspace lists and what the user picked on it.
#[derive(Default)]
pub(super) struct ReviewSetup {
    tab: ReviewTab,
    /// The user chose a tab, so an empty pull request list no longer moves them to branches.
    tab_chosen: bool,
    targets: Option<ReviewTargets>,
    /// Project whose targets are being listed.
    loading: Option<String>,
    /// Project whose listing failed, with the reason.
    error: Option<(String, String)>,
    pub(super) pick: Option<ReviewTarget>,
}

// The request that starts a review, as the conversation shows it. The session's own
// instructions hold the rules of a review, so this names only what to review.
fn review_request(target: &ReviewTarget, base: &str, notes: &str) -> String {
    let mut text = match target {
        ReviewTarget::PullRequest {
            number,
            title,
            head,
            base,
            ..
        } => format!("Review pull request #{number}: {title}\n`{head}` into `{base}`"),
        ReviewTarget::Branch { name, .. } => format!("Review branch `{name}` against `{base}`"),
    };
    let notes = notes.trim();
    if !notes.is_empty() {
        text.push_str("\n\n");
        text.push_str(notes);
    }
    text
}

// Reports that a review has written something to hand over.
fn has_findings(messages: &[Message]) -> bool {
    messages.iter().any(|message| {
        message.kind == MessageKind::Assistant
            && message.complete
            && !message.text.trim().is_empty()
    })
}

// Returns what the reviewer wrote, in order. When it exceeds the limit the end is kept, because
// the report ends a review.
fn review_findings(messages: &[Message]) -> Option<String> {
    let mut kept = Vec::new();
    let mut size = 0;
    for text in messages
        .iter()
        .rev()
        .filter(|message| message.kind == MessageKind::Assistant && message.complete)
        .map(|message| message.text.trim())
        .filter(|text| !text.is_empty())
    {
        if size + text.len() > FINDINGS_LIMIT && !kept.is_empty() {
            break;
        }
        size += text.len();
        kept.push(text);
    }
    if kept.is_empty() {
        return None;
    }
    kept.reverse();
    let mut text = kept.join("\n\n");
    if text.len() > FINDINGS_LIMIT {
        let start = text.ceil_char_boundary(text.len() - FINDINGS_LIMIT);
        text.drain(..start);
    }
    Some(text)
}

// The first message of the task that fixes a review's findings.
fn fix_request(scope: &ReviewScope, findings: &str) -> String {
    let subject = match &scope.target {
        ReviewTarget::PullRequest { number, title, .. } => {
            format!("pull request #{number} ({title})")
        }
        ReviewTarget::Branch { name, .. } => format!("branch `{name}`"),
    };
    let head = scope.head.get(..10).unwrap_or(&scope.head);
    format!(
        "Fix the findings of a local code review of {subject}.\n\n\
         This checkout is a new branch that starts at the reviewed commit {head}. The review \
         compared it with `{base}`. Check each finding against the code before you change \
         anything. Fix the findings that are valid, keep the changes within what they need, and \
         say which findings you skipped and why. Leave the result as local changes: do not \
         push, and do not comment on the pull request.\n\n\
         ## Review findings\n\n{findings}",
        base = scope.base,
    )
}

impl Shell {
    /// Returns the project whose pull requests and branches the start screen lists.
    pub(super) fn review_project(&self) -> Option<&Project> {
        let workspace = self.workspace.as_ref()?;
        workspace
            .projects
            .iter()
            .find(|project| Some(&project.id) == self.selected_project.as_ref())
            .or(workspace.projects.first())
    }

    /// Asks the host for the review targets of the start screen's project.
    ///
    /// Without `force` a project is listed once: neither a loaded list nor a failed listing is
    /// asked for again, so a render may call this on every frame.
    pub(super) fn load_review_targets(&mut self, force: bool) {
        let Some(project) = self.review_project().map(|project| project.id.clone()) else {
            return;
        };
        if self.review.loading.as_ref() == Some(&project) {
            return;
        }
        let listed = self
            .review
            .targets
            .as_ref()
            .is_some_and(|targets| targets.project_id == project);
        let failed = self
            .review
            .error
            .as_ref()
            .is_some_and(|(failed, _)| failed == &project);
        if !force && (listed || failed) {
            return;
        }
        if !listed {
            self.review.targets = None;
            self.review.pick = None;
        }
        self.review.error = None;
        self.review.loading = Some(project.clone());
        self.send(core::Request::ReviewTargets {
            project_id: project,
        });
    }

    /// Installs the review targets of the start screen's project. A pick that is no longer
    /// listed is dropped.
    pub(super) fn receive_review_targets(&mut self, targets: ReviewTargets) {
        if self.review.loading.as_ref() == Some(&targets.project_id) {
            self.review.loading = None;
        }
        if self.review_project().map(|project| &project.id) != Some(&targets.project_id) {
            return;
        }
        let listed = match &self.review.pick {
            Some(ReviewTarget::PullRequest { number, .. }) => targets
                .pull_requests
                .iter()
                .any(|pull_request| pull_request.number == *number),
            Some(ReviewTarget::Branch { reference, .. }) => targets
                .branches
                .iter()
                .any(|branch| &branch.reference == reference),
            None => true,
        };
        if !listed {
            self.review.pick = None;
        }
        // A project without pull requests, or without GitHub CLI, opens on its branches.
        if !self.review.tab_chosen
            && targets.pull_requests.is_empty()
            && !targets.branches.is_empty()
        {
            self.review.tab = ReviewTab::Branches;
        }
        self.review.error = None;
        self.review.targets = Some(targets);
    }

    /// Records why a project's review targets could not be listed. The picker shows the reason.
    pub(super) fn review_targets_failed(&mut self, request: &core::Request, error: String) {
        let core::Request::ReviewTargets { project_id } = request else {
            return;
        };
        if self.review.loading.as_ref() == Some(project_id) {
            self.review.loading = None;
        }
        self.review.error = Some((project_id.clone(), error));
    }

    /// Starts a review of the picked target. The composer's text goes with it as focus notes.
    pub(super) fn start_review(&mut self, cx: &mut Context<Self>) {
        if self.creating_task || self.selected.is_some() || self.pasting_images > 0 {
            return;
        }
        let Some(project) = self.review_project().map(|project| project.id.clone()) else {
            return;
        };
        let Some(target) = self.review.pick.clone() else {
            let tone = self.theme.warning;
            self.notify_user("Pick a pull request or a branch to review", tone, cx);
            return;
        };
        let base = self
            .review
            .targets
            .as_ref()
            .and_then(|targets| targets.default_branch.clone())
            .unwrap_or_default();
        let input = self.input.read(cx);
        self.new_submission_origin = input.text_bounds().map(|bounds| bounds.origin);
        self.empty_draft = review_request(&target, &base, &input.text());
        self.creating_task = true;
        let mut preferences = self.composer_preferences();
        // A review has no plan mode: it starts with the approval rule alone.
        if self.planning() {
            preferences.provider_access = self.approval_choice();
        }
        self.send(core::Request::CreateReview {
            project_id: project,
            target,
            preferences,
        });
        cx.notify();
    }

    /// Starts a task in Agentic Coding that fixes the findings of the open review.
    ///
    /// The task works in a new worktree whose branch starts at the reviewed commit, so the fixes
    /// apply to the reviewed code and the project's checkout stays as it is.
    pub(super) fn fix_review_findings(&mut self, cx: &mut Context<Self>) {
        if self.creating_task {
            return;
        }
        let request = self.view.as_ref().and_then(|view| {
            let scope = view.task.review_scope.as_ref()?;
            let findings = review_findings(&view.messages)?;
            let project = self
                .workspace
                .as_ref()?
                .projects
                .iter()
                .find(|project| project.id == view.task.project_id)?;
            Some((
                fix_request(scope, &findings),
                core::Request::CreateTask {
                    project_id: project.id.clone(),
                    worktree: true,
                    preferences: self.starting_preferences(Some(project), None),
                    base: Some(scope.head.clone()),
                },
            ))
        });
        let Some((prompt, request)) = request else {
            let tone = self.theme.warning;
            self.notify_user("This review has no findings to send yet", tone, cx);
            return;
        };
        self.launch_draft = Some(prompt);
        self.new_submission_origin = None;
        self.creating_task = true;
        self.send(request);
        cx.notify();
    }

    /// Returns the bar that offers to fix the findings of the open review, once it has some.
    pub(super) fn review_actions(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.theme;
        let view = self.view.as_ref()?;
        let task = &view.task;
        if task.kind != TaskKind::Review
            || task.state.active()
            || task.needs_input()
            || !has_findings(&view.messages)
        {
            return None;
        }
        Some(
            chat::notice(t, t.added, "Findings")
                .child(div().flex_1().child(
                    "The review is ready. Send its findings to a new task that checks each one and fixes the valid ones.",
                ))
                .child(
                    self.primary("fix-findings", "wrench", "Fix in Agentic Coding")
                        .disabled(self.creating_task)
                        .tooltip(move |_, cx| {
                            cx.new(|_| Hint {
                                text: "Starts a task in a new worktree whose branch begins at the reviewed commit. Your checkout stays as it is.".into(),
                                theme: t,
                            })
                            .into()
                        })
                        .on_click(cx.listener(|s, _, _, cx| s.fix_review_findings(cx))),
                )
                .into_any_element(),
        )
    }

    /// Returns the composer row that names what the next review examines.
    pub(super) fn review_pick_status(&self) -> AnyElement {
        let t = self.theme;
        let row = div()
            .px(px(14.))
            .pt(px(10.))
            .h(px(30.))
            .flex()
            .items_center()
            .gap(px(8.));
        if self.creating_task {
            return row
                .child(kit::loader_row("Preparing the review worktree…", t))
                .into_any_element();
        }
        match &self.review.pick {
            Some(target) => {
                let (glyph, detail) = match target {
                    ReviewTarget::PullRequest { title, .. } => ("git-pull-request", title.clone()),
                    ReviewTarget::Branch { .. } => ("branch", String::new()),
                };
                row.child(icon(glyph).size(px(13.)).text_color(t.accent))
                    .child(kit::chip(target.label(), t.accent))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_size(text_px(12.))
                            .text_color(t.muted)
                            .child(detail),
                    )
            }
            None => row
                .child(icon("git-pull-request").size(px(13.)).text_color(t.subtle))
                .child(caps("Pick a pull request or a branch above", t)),
        }
        .into_any_element()
    }

    // One tab of the picker, with the count of what it lists.
    fn review_tab(
        &self,
        tab: ReviewTab,
        label: &'static str,
        glyph: &'static str,
        count: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let chosen = self.review.tab == tab;
        div()
            .id(("review-tab", tab as usize))
            .relative()
            .h_full()
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(7.))
            .cursor_pointer()
            .font_family(crate::theme::mono())
            .text_size(text_px(11.5))
            .text_color(if chosen { t.text } else { t.muted })
            .when(!chosen, |cell| cell.hover(move |cell| cell.bg(t.selected)))
            .child(icon(glyph).size(px(13.)))
            .child(label)
            .when_some(count, |cell, count| {
                cell.child(figure(count.to_string(), t.subtle))
            })
            .when(chosen, |cell| cell.child(kit::tab_indicator(t.accent)))
            .on_click(cx.listener(move |s, _, _, cx| {
                s.review.tab = tab;
                s.review.tab_chosen = true;
                cx.notify();
            }))
            .into_any_element()
    }

    // The frame of one target row. A picked row carries the accent tint, wash and edge.
    fn review_row(&self, id: impl Into<ElementId>, chosen: bool) -> Stateful<Div> {
        let t = self.theme;
        let radius = crate::theme::RADIUS_CONTROL;
        div()
            .id(id)
            .relative()
            .overflow_hidden()
            .w_full()
            .flex_shrink_0()
            .px(px(8.))
            .py(px(6.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .rounded(px(radius))
            .cursor_pointer()
            .when(chosen, |row| {
                row.bg(t.accent.opacity(0.1))
                    .layer(dither::row_wash(t.accent, true, false))
                    .child(kit::selection_edge(t.accent))
            })
            .when(!chosen, |row| row.hover(move |row| row.bg(t.selected)))
    }

    // Picks a target, or clears the pick when it is picked again.
    fn pick_review_target(&mut self, target: ReviewTarget, cx: &mut Context<Self>) {
        self.review.pick = (self.review.pick.as_ref() != Some(&target)).then_some(target);
        cx.notify();
    }

    fn pull_request_row(
        &self,
        pull_request: &PullRequestSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let chosen = matches!(
            &self.review.pick,
            Some(ReviewTarget::PullRequest { number, .. }) if *number == pull_request.number
        );
        let target = ReviewTarget::PullRequest {
            number: pull_request.number,
            title: pull_request.title.clone(),
            head: pull_request.head.clone(),
            base: pull_request.base.clone(),
            url: pull_request.url.clone(),
        };
        self.review_row(
            ("review-pull-request", pull_request.number as usize),
            chosen,
        )
        .child(
            div()
                .relative()
                .h(px(20.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    icon("git-pull-request")
                        .size(px(13.))
                        .text_color(if chosen { t.accent } else { t.muted }),
                )
                .child(figure(format!("#{}", pull_request.number), t.muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(text_px(13.))
                        .text_color(t.text)
                        .text_ellipsis()
                        .child(pull_request.title.clone()),
                )
                .when(pull_request.draft, |line| {
                    line.child(kit::chip("DRAFT", t.subtle))
                })
                .child(kit::diffstat(
                    pull_request.additions as usize,
                    pull_request.deletions as usize,
                    t,
                )),
        )
        .child(
            div()
                .relative()
                .h(px(16.))
                .pl(px(21.))
                .flex()
                .items_center()
                .gap(px(6.))
                .font_family(crate::theme::mono())
                .text_size(text_px(10.5))
                .text_color(t.subtle)
                .child(
                    div()
                        .flex_shrink(1.)
                        .min_w_0()
                        .text_ellipsis()
                        .child(pull_request.head.clone()),
                )
                .child(icon("arrow-right").size(px(10.)))
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w(relative(0.3))
                        .text_ellipsis()
                        .child(pull_request.base.clone()),
                )
                .child(div().flex_1())
                .when(!pull_request.author.is_empty(), |line| {
                    line.child(
                        div()
                            .flex_shrink_0()
                            .max_w(relative(0.3))
                            .text_ellipsis()
                            .child(format!("@{}", pull_request.author)),
                    )
                })
                .when(pull_request.updated > 0, |line| {
                    line.child(figure(sidebar::elapsed(pull_request.updated), t.subtle))
                }),
        )
        .on_click(cx.listener(move |s, _, _, cx| s.pick_review_target(target.clone(), cx)))
        .into_any_element()
    }

    fn branch_row(
        &self,
        index: usize,
        branch: &BranchSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let chosen = matches!(
            &self.review.pick,
            Some(ReviewTarget::Branch { reference, .. }) if reference == &branch.reference
        );
        let target = ReviewTarget::Branch {
            name: branch.name.clone(),
            reference: branch.reference.clone(),
        };
        self.review_row(("review-branch", index), chosen)
            .child(
                div()
                    .relative()
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icon("branch").size(px(13.)).text_color(if chosen {
                        t.accent
                    } else {
                        t.muted
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(12.))
                            .text_color(t.text)
                            .text_ellipsis()
                            .child(branch.name.clone()),
                    )
                    .when(branch.current, |line| {
                        line.child(kit::chip("CHECKED OUT", t.accent))
                    })
                    .when(branch.remote, |line| {
                        line.child(kit::chip("REMOTE", t.subtle))
                    })
                    .child(figure(sidebar::elapsed(branch.updated), t.subtle)),
            )
            .child(
                div()
                    .relative()
                    .h(px(16.))
                    .pl(px(21.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .font_family(crate::theme::mono())
                    .text_size(text_px(10.5))
                    .text_color(t.subtle)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(branch.subject.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .max_w(relative(0.3))
                            .text_ellipsis()
                            .child(branch.author.clone()),
                    )
                    .child(figure(branch.commit.clone(), t.subtle)),
            )
            .on_click(cx.listener(move |s, _, _, cx| s.pick_review_target(target.clone(), cx)))
            .into_any_element()
    }

    // A message in place of the list: why it is empty, with an optional way out.
    fn review_note(&self, title: &'static str, detail: String, action: Option<Button>) -> Div {
        let t = self.theme;
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .px(px(24.))
            .child(caps(title, t).text_color(t.muted))
            .child(
                div()
                    .max_w(px(460.))
                    .text_center()
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.))
                    .line_height(relative(1.5))
                    .text_color(t.subtle)
                    .child(detail),
            )
            .children(action)
    }

    /// Builds the picker of the Review start screen: the pull requests and the branches of the
    /// chosen project, with a filter and the branch they are compared with.
    pub(super) fn review_picker(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.load_review_targets(false);
        let t = self.theme;
        let Some(project) = self.review_project().cloned() else {
            return kit::block(t)
                .h(px(LIST_HEIGHT))
                .child(
                    self.review_note(
                        "No repository",
                        "Open a repository to review its pull requests and branches.".into(),
                        Some(
                            self.button("review-open-project", "folder-open", "Open repository")
                                .on_click(
                                    cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx)),
                                ),
                        ),
                    ),
                )
                .into_any_element();
        };
        let targets = self
            .review
            .targets
            .as_ref()
            .filter(|targets| targets.project_id == project.id);
        let tab = self.review.tab;
        let query = self.review_filter.read(cx).text().trim().to_lowercase();
        let matches = |fields: &[&str]| {
            query.is_empty()
                || fields
                    .iter()
                    .any(|field| field.to_lowercase().contains(&query))
        };
        let pull_requests: Vec<&PullRequestSummary> = targets
            .map(|targets| {
                targets
                    .pull_requests
                    .iter()
                    .filter(|pull_request| {
                        matches(&[
                            &pull_request.title,
                            &pull_request.head,
                            &pull_request.author,
                            &format!("#{}", pull_request.number),
                        ])
                    })
                    .collect()
            })
            .unwrap_or_default();
        let branches: Vec<(usize, &BranchSummary)> = targets
            .map(|targets| {
                targets
                    .branches
                    .iter()
                    .enumerate()
                    .filter(|(_, branch)| matches(&[&branch.name, &branch.subject, &branch.author]))
                    .collect()
            })
            .unwrap_or_default();
        let header = div()
            .h(px(34.))
            .flex_shrink_0()
            .pr(px(4.))
            .flex()
            .items_center()
            .bg(t.header)
            .border_b_1()
            .border_color(t.border)
            .child(self.review_tab(
                ReviewTab::PullRequests,
                "Pull requests",
                "git-pull-request",
                targets.map(|targets| targets.pull_requests.len()),
                cx,
            ))
            .child(self.review_tab(
                ReviewTab::Branches,
                "Branches",
                "branch",
                targets.map(|targets| targets.branches.len()),
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(8.))
                    .px(px(8.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
            )
            .child(
                div()
                    .w(px(168.))
                    .flex_shrink(1.)
                    .min_w(px(72.))
                    .h(px(24.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .bg(t.inset)
                    .border_1()
                    .border_color(t.border)
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.5))
                    .child(icon("search").size(px(12.)).text_color(t.subtle))
                    .child(div().flex_1().min_w_0().child(self.review_filter.clone())),
            )
            .child(
                self.icon_button(
                    "review-refresh",
                    "refresh",
                    "Read pull requests and branches again",
                )
                .disabled(self.review.loading.is_some())
                .on_click(cx.listener(|s, _, _, cx| {
                    s.load_review_targets(true);
                    cx.notify();
                })),
            );
        let mut list = div()
            .id("review-targets")
            .h(px(LIST_HEIGHT))
            .overflow_y_scroll()
            .p(px(4.))
            .flex()
            .flex_col()
            .gap(px(1.));
        let failure = self
            .review
            .error
            .as_ref()
            .filter(|(failed, _)| failed == &project.id)
            .map(|(_, reason)| reason.clone());
        list = match (targets, failure) {
            (_, Some(reason)) => list.child(
                self.review_note(
                    "Could not read this repository",
                    reason,
                    Some(
                        self.button("review-retry", "refresh", "Try again")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.load_review_targets(true);
                                cx.notify();
                            })),
                    ),
                ),
            ),
            (None, None) => list.child(
                div()
                    .p(px(12.))
                    .child(kit::loader_row("Reading pull requests and branches…", t)),
            ),
            (Some(targets), None) if tab == ReviewTab::PullRequests => {
                if let Some(reason) = &targets.pull_request_error {
                    list.child(
                        self.review_note(
                            "Pull requests are unavailable",
                            format!("{reason}. Branches need no GitHub CLI."),
                            Some(
                                self.button("review-show-branches", "branch", "Show branches")
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.review.tab = ReviewTab::Branches;
                                        s.review.tab_chosen = true;
                                        cx.notify();
                                    })),
                            ),
                        ),
                    )
                } else if pull_requests.is_empty() {
                    list.child(self.review_note(
                        if query.is_empty() {
                            "No open pull requests"
                        } else {
                            "No pull request matches"
                        },
                        format!("{} has nothing to list here.", project.name),
                        None,
                    ))
                } else {
                    list.children(
                        pull_requests
                            .iter()
                            .map(|pull_request| self.pull_request_row(pull_request, cx)),
                    )
                }
            }
            (Some(targets), None) => {
                if targets.default_branch.is_none() {
                    list.child(self.review_note(
                        "No default branch",
                        "A branch is reviewed against the default branch, and this repository has none."
                            .into(),
                        None,
                    ))
                } else if branches.is_empty() {
                    list.child(self.review_note(
                        if query.is_empty() {
                            "No other branches"
                        } else {
                            "No branch matches"
                        },
                        format!("{} has nothing to list here.", project.name),
                        None,
                    ))
                } else {
                    list.children(
                        branches
                            .iter()
                            .map(|(index, branch)| self.branch_row(*index, branch, cx)),
                    )
                }
            }
        };
        // The footer says what a review compares, and what it leaves out.
        let unsaved = targets.is_some_and(|targets| {
            targets.dirty
                && matches!(
                    &self.review.pick,
                    Some(ReviewTarget::Branch { reference, .. })
                        if targets.branches.iter().any(|branch| {
                            branch.current && &branch.reference == reference
                        })
                )
        });
        let mut footer = div()
            .h(px(26.))
            .flex_shrink_0()
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .bg(t.header);
        footer = if unsaved {
            footer.child(kit::marker(t.warning)).child(
                caps(
                    "Uncommitted changes of the checkout are not part of the review",
                    t,
                )
                .text_color(t.warning),
            )
        } else {
            let shown = match tab {
                ReviewTab::PullRequests => pull_requests.len(),
                ReviewTab::Branches => branches.len(),
            };
            footer
                .when_some(
                    targets
                        .and_then(|targets| targets.default_branch.clone())
                        .filter(|_| tab == ReviewTab::Branches),
                    |footer, base| {
                        footer
                            .child(caps("Compared with", t))
                            .child(kit::chip(base, t.info))
                    },
                )
                .when(tab == ReviewTab::PullRequests, |footer| {
                    footer.child(caps("Fetched when the review starts", t))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(8.))
                        .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                )
                .child(figure(format!("{shown} listed"), t.subtle))
        };
        kit::block(t)
            .mb(px(14.))
            .child(header)
            .child(list)
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .child(footer)
            .into_any_element()
    }
}
