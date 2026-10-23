//! The first-run tour of the Git workspace. It fills the workspace's content area until the
//! user finishes it, and walks through what the commit list shows, a GitHub account, Git
//! defaults, and the first repository. Every choice saves right away, so the workspace changes
//! while the user picks.
use std::time::Duration;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Context, Div, ElementId, Entity, EventEmitter,
    FontWeight, Hsla, IntoElement, Render, SharedString, Subscription, Window, div, relative,
};
use kodem_git::settings::{DateFormat, PullStrategy};

use crate::theme::{RADIUS_SURFACE, px, text_px};
use crate::ui::dither::{Dither, DitherField, Edge};
use crate::ui::fx;
use crate::ui::git::account_link::{AccountLinkEvent, AccountLinkForm, linked_account_rows};
use crate::ui::git::components::{button, dither_icon_tile, mono_font, mono_label};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, Icon, IconName, ScrollableElement as _, Tooltip,
    ease_out_cubic, h_flex, v_flex,
};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::settings_view::{segmented, setting_row, toggle_row};
use crate::ui::git::theme;
use crate::ui::git::workspace::welcome_card;
use crate::updater::{VERSION, version_label};

const STEP_DURATION: Duration = Duration::from_millis(360);
const TIDE_DURATION: Duration = Duration::from_millis(900);
/// Share of the content height that the tide can fill.
const TIDE_HEIGHT: f32 = 0.5;
const COLUMN_WIDTH: f32 = 660.0;
/// Width of the progress pill of the current step and of the others.
const ACTIVE_PILL_WIDTH: f32 = 28.0;
const PILL_WIDTH: f32 = 7.0;
/// How far a new step slides in from the side.
const STEP_SLIDE: f32 = 32.0;
const ACCOUNTS_ANCHOR: &str = "onboarding-accounts";

/// What the user picked on the last step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnboardingExit {
    /// Open a repository from disk.
    OpenRepository,
    /// Clone a repository.
    CloneRepository,
    /// Go to the welcome screen of the Git workspace.
    Done,
}

/// Sent when the user finishes the tour. The receiver marks onboarding
/// as completed.
pub enum OnboardingEvent {
    /// The user left the last step with this choice.
    Finished(OnboardingExit),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Welcome,
    Look,
    Account,
    Workflow,
    Ready,
}

impl Step {
    const ALL: [Step; 5] = [
        Step::Welcome,
        Step::Look,
        Step::Account,
        Step::Workflow,
        Step::Ready,
    ];

    fn label(self) -> &'static str {
        match self {
            Step::Welcome => "Welcome",
            Step::Look => "Look",
            Step::Account => "Account",
            Step::Workflow => "Workflow",
            Step::Ready => "Ready",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Step::Welcome => "Welcome to Git in Kodem",
            Step::Look => "Make it yours",
            Step::Account => "Link GitHub",
            Step::Workflow => "Your Git defaults",
            Step::Ready => "You are ready",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Step::Welcome => {
                "A minute of setup: the commit list, a GitHub account, and how Git should \
                 behave. You can change all of it later in Preferences under Git."
            }
            Step::Look => {
                "Choose what the commit list shows. The theme and accent follow Kodem's \
                 Appearance preferences."
            }
            Step::Account => {
                "Browse and clone your repositories, read pull requests, and push over HTTPS \
                 without a separate login."
            }
            Step::Workflow => {
                "How pulls join your work, how often Kodem fetches, and when it asks first."
            }
            Step::Ready => "Open a repository from disk, or clone one from your account.",
        }
    }

    /// The fill level of the background tide. It rises with each step.
    fn tide_level(self) -> f32 {
        match self {
            Step::Welcome => 0.2,
            Step::Look => 0.35,
            Step::Account => 0.5,
            Step::Workflow => 0.65,
            Step::Ready => 0.8,
        }
    }
}

/// The first-run tour shown in place of the Git workspace's content.
pub struct OnboardingView {
    step: usize,
    previous: usize,
    /// Restarts the step transition on every change.
    sequence: u64,
    link_form: Entity<AccountLinkForm>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OnboardingEvent> for OnboardingView {}

impl OnboardingView {
    /// Creates the tour at its first step.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let link_form = cx.new(|cx| AccountLinkForm::new(window, cx));
        let subscriptions = vec![cx.subscribe(&link_form, |_, _, event, cx| {
            let AccountLinkEvent::Linked(_) = event;
            fx::impact(
                ACCOUNTS_ANCHOR,
                crate::ui::git::theme::liquid_colors(cx).success,
                cx,
            );
            cx.notify();
        })];
        Self {
            step: 0,
            previous: 0,
            sequence: 0,
            link_form,
            _subscriptions: subscriptions,
        }
    }

    fn current(&self) -> Step {
        Step::ALL[self.step]
    }

    fn go_to(&mut self, step: usize, cx: &mut Context<Self>) {
        let step = step.min(Step::ALL.len() - 1);
        if step == self.step {
            return;
        }
        self.previous = self.step;
        self.step = step;
        self.sequence += 1;
        fx::impact(&step_anchor(step), cx.theme().primary, cx);
        cx.notify();
    }

    fn finish(&mut self, exit: OnboardingExit, cx: &mut Context<Self>) {
        cx.emit(OnboardingEvent::Finished(exit));
    }

    fn render_step(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.current() {
            Step::Welcome => welcome_step(cx).into_any_element(),
            Step::Look => look_step(cx).into_any_element(),
            Step::Account => self.account_step(cx).into_any_element(),
            Step::Workflow => workflow_step(cx).into_any_element(),
            Step::Ready => self.ready_step(cx).into_any_element(),
        }
    }

    fn account_step(&self, cx: &mut Context<Self>) -> Div {
        let rows = linked_account_rows(cx);
        v_flex()
            .w_full()
            .gap_3()
            .child(
                v_flex()
                    .relative()
                    .w_full()
                    .gap_1()
                    .child(fx::anchor(ACCOUNTS_ANCHOR))
                    .when(rows.is_empty(), |list| {
                        list.child(
                            h_flex()
                                .w_full()
                                .gap_3()
                                .child(dither_icon_tile(
                                    IconName::Github,
                                    cx.theme().primary,
                                    36.0,
                                    true,
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(text_px(12.5))
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            "Create a token with the button below, or reuse the \
                                             login of the GitHub CLI. You can skip this step.",
                                        ),
                                ),
                        )
                    })
                    .children(rows),
            )
            .child(self.link_form.clone())
    }

    fn ready_step(&self, cx: &mut Context<Self>) -> Div {
        h_flex()
            .w_full()
            .gap_3()
            .child(
                welcome_card(
                    "onboarding-open",
                    Icon::new(IconName::FolderOpen),
                    "Open Repository",
                    SharedString::from("Choose a folder on disk"),
                    Some("⌘O"),
                    DitherField::Ember {
                        scale: 0.16,
                        speed: 2.4,
                    },
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.finish(OnboardingExit::OpenRepository, cx);
                })),
            )
            .child(
                welcome_card(
                    "onboarding-clone",
                    icons::pull(),
                    "Clone Repository",
                    SharedString::from("From GitHub or any URL"),
                    Some("⇧⌘O"),
                    DitherField::Flow {
                        scale: 0.1,
                        speed: 1.6,
                        bias: -0.18,
                    }
                    .times(DitherField::Fade {
                        from: Edge::Bottom,
                        start: 1.0,
                        end: 0.2,
                    }),
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.finish(OnboardingExit::CloneRepository, cx);
                })),
            )
            .child(
                welcome_card(
                    "onboarding-done",
                    icons::kodem_mark(),
                    "Start Empty",
                    SharedString::from("Go to the welcome screen"),
                    None,
                    DitherField::Glow {
                        x: 0.2,
                        y: 1.0,
                        radius: 0.9,
                        strength: 0.8,
                    }
                    .times(DitherField::Flow {
                        scale: 0.2,
                        speed: 1.0,
                        bias: 0.0,
                    }),
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| this.finish(OnboardingExit::Done, cx))),
            )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> Div {
        let step = self.current();
        let last = self.step + 1 == Step::ALL.len();
        let skip = step == Step::Account && GitSettings::get(cx).accounts.is_empty();
        let next = self.step + 1;
        let back = self.step.saturating_sub(1);

        h_flex()
            .w_full()
            .items_center()
            .child(
                div().w(px(140.)).child(
                    button("onboarding-back")
                        .ghost()
                        .icon(IconName::ArrowLeft)
                        .label("Back")
                        .disabled_readable(self.step == 0, cx)
                        .on_click(cx.listener(move |this, _, _, cx| this.go_to(back, cx))),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .justify_center()
                    .child(self.render_progress(cx)),
            )
            .child(h_flex().w(px(140.)).justify_end().child(if last {
                button("onboarding-finish")
                    .primary()
                    .icon(IconName::Check)
                    .label("Finish")
                    .on_click(cx.listener(|this, _, _, cx| this.finish(OnboardingExit::Done, cx)))
            } else if skip {
                button("onboarding-next")
                    .outline()
                    .label("Skip for now")
                    .on_click(cx.listener(move |this, _, _, cx| this.go_to(next, cx)))
            } else {
                button("onboarding-next")
                    .primary()
                    .icon(IconName::ArrowRight)
                    .label(if self.step == 0 {
                        "Get Started"
                    } else {
                        "Continue"
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.go_to(next, cx)))
            }))
    }

    /// Step pills. The current pill stretches open like a worm while the
    /// one it left shrinks back; clicking a visited pill goes back to it.
    fn render_progress(&self, cx: &mut Context<Self>) -> AnyElement {
        let step = self.step;
        let previous = self.previous;
        let accent = cx.theme().primary;
        let idle = cx.theme().muted_foreground.opacity(0.3);
        let pills = Step::ALL
            .iter()
            .enumerate()
            .map(|(ix, item)| {
                let label = item.label();
                div()
                    .id(("onboarding-pill", ix))
                    .py_2()
                    .when(ix < step, |pill| {
                        pill.cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| this.go_to(ix, cx)))
                    })
                    .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
            })
            .collect::<Vec<_>>();

        h_flex()
            .gap_1p5()
            .items_center()
            .children(pills.into_iter().enumerate().map(move |(ix, hit)| {
                hit.child(
                    div()
                        .relative()
                        .h(px(PILL_WIDTH))
                        .child(fx::anchor(step_anchor(ix)))
                        .with_animation(
                            ElementId::NamedInteger(
                                SharedString::from(format!("onboarding-pill-{ix}")),
                                self.sequence,
                            ),
                            Animation::new(STEP_DURATION),
                            move |pill, delta| {
                                pill_frame(pill, ix, step, previous, delta, accent, idle)
                            },
                        ),
                )
            }))
            .into_any_element()
    }
}

/// One progress pill at `delta` of a step change.
fn pill_frame(
    pill: Div,
    ix: usize,
    step: usize,
    previous: usize,
    delta: f32,
    accent: Hsla,
    idle: Hsla,
) -> Div {
    let open = fx::ease_out_back(delta);
    let width = if ix == step {
        PILL_WIDTH + (ACTIVE_PILL_WIDTH - PILL_WIDTH) * open
    } else if ix == previous {
        ACTIVE_PILL_WIDTH - (ACTIVE_PILL_WIDTH - PILL_WIDTH) * ease_out_cubic(delta)
    } else {
        PILL_WIDTH
    };
    let pill = pill
        .w(px(width.max(PILL_WIDTH * 0.6)))
        .rounded(px(2.))
        .overflow_hidden();
    if ix == step {
        pill.bg(accent.opacity(0.25)).child(
            Dither::new(DitherField::Flow {
                scale: 0.3,
                speed: 4.0,
                bias: 0.2,
            })
            .color(accent)
            .grid(1.0, 1.0)
            .animated()
            .layer(),
        )
    } else if ix < step {
        pill.bg(accent.opacity(0.7))
    } else {
        pill.bg(idle)
    }
}

fn step_anchor(ix: usize) -> SharedString {
    SharedString::from(format!("onboarding-step-{ix}"))
}

/// The step body. It slides in from the side it came from while a
/// coarse Bayer dissolve in the card color clears.
fn step_motion(
    sequence: u64,
    forward: bool,
    surface: Hsla,
    content: impl IntoElement + 'static,
) -> AnyElement {
    let direction = if forward { 1.0 } else { -1.0 };
    div()
        .relative()
        .w_full()
        .child(content)
        .with_animation(
            ElementId::NamedInteger("onboarding-step-motion".into(), sequence),
            Animation::new(STEP_DURATION).with_easing(ease_out_cubic),
            move |body, delta| {
                let moved = body
                    .left(px(STEP_SLIDE * direction * (1.0 - delta)))
                    .opacity(0.35 + 0.65 * delta);
                if delta >= 1.0 {
                    return moved;
                }
                moved.child(
                    Dither::new(DitherField::Dissolve(delta))
                        .color(surface)
                        .grid(4.0, 4.0)
                        .layer(),
                )
            },
        )
        .into_any_element()
}

/// The background tide in the lower half of the content. It rises from
/// the level of the previous step to the level of the current one.
fn tide(sequence: u64, from: f32, to: f32, accent: Hsla) -> AnyElement {
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(relative(TIDE_HEIGHT))
        .with_animation(
            ElementId::NamedInteger("onboarding-tide".into(), sequence),
            Animation::new(TIDE_DURATION).with_easing(ease_out_cubic),
            move |layer, delta| {
                layer.child(
                    Dither::new(DitherField::Wave {
                        level: from + (to - from) * delta,
                        amplitude: 0.035,
                        speed: 1.2,
                    })
                    .color(accent.opacity(0.55))
                    .grid(4.0, 3.0)
                    .toned()
                    .animated()
                    .layer(),
                )
            },
        )
        .into_any_element()
}

fn feature_tile(icon: impl Into<Icon>, title: &'static str, detail: &'static str, cx: &App) -> Div {
    let colors = theme::liquid_colors(cx);
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .p_3()
        .rounded(px(RADIUS_SURFACE))
        .border_1()
        .border_color(cx.theme().border)
        .bg(colors.panel_secondary)
        .child(dither_icon_tile(icon, cx.theme().primary, 32.0, true))
        .child(
            div()
                .text_size(text_px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_size(text_px(11.5))
                .text_color(colors.text_tertiary)
                .child(detail),
        )
}

fn welcome_step(cx: &App) -> Div {
    let colors = theme::liquid_colors(cx);
    v_flex()
        .w_full()
        .gap_4()
        .child(
            h_flex()
                .gap_3()
                .items_end()
                .child(
                    icons::kodem_mark()
                        .size(px(48.))
                        .text_color(cx.theme().primary),
                )
                .child(
                    div()
                        .font_family(mono_font())
                        .text_size(text_px(52.))
                        .line_height(text_px(56.))
                        .font_weight(FontWeight::BOLD)
                        .child("git"),
                )
                .child(
                    div()
                        .pb_2()
                        .font_family(mono_font())
                        .text_size(text_px(11.))
                        .text_color(colors.text_tertiary)
                        .child(format!("kodem v{}", version_label(VERSION))),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .gap_3()
                .items_start()
                .child(feature_tile(
                    icons::git_branch(),
                    "Living history",
                    "A lane graph that glows along the branch you are on.",
                    cx,
                ))
                .child(feature_tile(
                    icons::file_text(),
                    "Precise changes",
                    "Stage whole files, hunks, or single lines.",
                    cx,
                ))
                .child(feature_tile(
                    IconName::Github,
                    "Pull requests",
                    "Read and review GitHub pull requests in place.",
                    cx,
                )),
        )
}

fn look_step(cx: &App) -> Div {
    let settings = GitSettings::get(cx);
    v_flex()
        .w_full()
        .child(toggle_row(
            "onboarding-avatars",
            "Author avatars",
            "Gravatar images next to commits",
            settings.show_avatars,
            |v, cx| GitSettings::update(cx, |s| s.show_avatars = v),
            cx,
        ))
        .child(toggle_row(
            "onboarding-badges",
            "Branch badges",
            "Branch and tag labels on commits",
            settings.show_branch_badges,
            |v, cx| GitSettings::update(cx, |s| s.show_branch_badges = v),
            cx,
        ))
        .child(toggle_row(
            "onboarding-stats",
            "Commit stats",
            "Additions and deletions of each commit",
            settings.show_commit_stats_badges,
            |v, cx| GitSettings::update(cx, |s| s.show_commit_stats_badges = v),
            cx,
        ))
        .child(setting_row(
            "Dates",
            "How commit dates read in the list",
            segmented(
                "onboarding-date-format",
                DateFormat::ALL.map(|format| (format, format.label())),
                &settings.date_format,
                |format, _, cx| GitSettings::update(cx, |s| s.date_format = *format),
                cx,
            ),
            cx,
        ))
}

fn workflow_step(cx: &App) -> Div {
    let settings = GitSettings::get(cx);
    let auto_fetch = match settings.auto_fetch_interval {
        0 => 0,
        1..=5 => 5,
        6..=10 => 10,
        _ => 30,
    };

    v_flex()
        .w_full()
        .child(setting_row(
            "Pull strategy",
            "How pulled commits join your branch",
            segmented(
                "onboarding-pull",
                PullStrategy::ALL.map(|strategy| (strategy, strategy.label())),
                &settings.pull_strategy,
                |strategy, _, cx| GitSettings::update(cx, |s| s.pull_strategy = *strategy),
                cx,
            ),
            cx,
        ))
        .child(setting_row(
            "Auto-fetch",
            "Fetch remotes in the background",
            segmented(
                "onboarding-fetch",
                [(0u32, "Off"), (5, "5 min"), (10, "10 min"), (30, "30 min")],
                &auto_fetch,
                |interval, _, cx| GitSettings::update(cx, |s| s.auto_fetch_interval = *interval),
                cx,
            ),
            cx,
        ))
        .child(toggle_row(
            "onboarding-confirm",
            "Confirm destructive actions",
            "Ask before discard, reset, and force push",
            settings.confirm_destructive_actions,
            |v, cx| GitSettings::update(cx, |s| s.confirm_destructive_actions = v),
            cx,
        ))
        .child(toggle_row(
            "onboarding-restore",
            "Restore tabs on launch",
            "Reopen the repositories from the last session",
            settings.restore_tabs_on_launch,
            |v, cx| GitSettings::update(cx, |s| s.restore_tabs_on_launch = v),
            cx,
        ))
}

impl Render for OnboardingView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::liquid_colors(cx);
        let accent = cx.theme().primary;
        let step = self.current();
        let total = Step::ALL.len();
        let forward = self.step >= self.previous;
        let from_level = Step::ALL[self.previous].tide_level();
        let body = self.render_step(cx);
        let footer = self.render_footer(cx);

        let card = v_flex()
            .relative()
            .w_full()
            .rounded(px(RADIUS_SURFACE))
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .bg(colors.panel)
            .child(
                Dither::new(
                    DitherField::Glow {
                        x: 1.0,
                        y: 0.0,
                        radius: 0.55,
                        strength: 0.8,
                    }
                    .times(DitherField::Flow {
                        scale: 0.1,
                        speed: 1.0,
                        bias: 0.1,
                    }),
                )
                .color(accent.opacity(0.35))
                .grid(3.0, 2.0)
                .animated()
                .corner_radius(RADIUS_SURFACE)
                .layer(),
            )
            .child(div().relative().w_full().p_6().child(step_motion(
                self.sequence,
                forward,
                colors.panel,
                body,
            )));

        div()
            .relative()
            .size_full()
            .bg(colors.window_backdrop)
            .child(tide(self.sequence, from_level, step.tide_level(), accent))
            .child(
                v_flex()
                    .relative()
                    .size_full()
                    .child(
                        h_flex()
                            .w_full()
                            .h(px(40.))
                            .flex_shrink_0()
                            .justify_center()
                            .child(mono_label("Git · Setup", cx)),
                    )
                    .child(
                        v_flex()
                            .id("onboarding-scroll")
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .items_center()
                            .overflow_y_scrollbar()
                            .child(
                                v_flex()
                                    .w(px(COLUMN_WIDTH))
                                    .max_w_full()
                                    .px_6()
                                    .py_10()
                                    .gap_5()
                                    .child(
                                        v_flex()
                                            .gap_1()
                                            .child(mono_label(
                                                format!(
                                                    "{:02} / {:02} · {}",
                                                    self.step + 1,
                                                    total,
                                                    step.label().to_uppercase()
                                                ),
                                                cx,
                                            ))
                                            .child(
                                                div()
                                                    .text_size(text_px(24.))
                                                    .font_weight(FontWeight::BOLD)
                                                    .child(step.title()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(text_px(12.5))
                                                    .text_color(colors.text_secondary)
                                                    .child(step.detail()),
                                            ),
                                    )
                                    .child(card)
                                    .child(footer),
                            ),
                    ),
            )
    }
}
