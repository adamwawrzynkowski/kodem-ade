//! The Git section of Kodem's Preferences. Its pages came from the settings window of the
//! standalone Liquid app: General, Remote, Diffs, Display, Accounts, AI and Local Ignore. The
//! section draws one page at a time with the rows, switches, sliders and segmented controls of
//! the other preference sections. Every change saves at once.
mod accounts;
mod ai;
mod diffs;
mod display;
mod general;
mod local_ignore;
mod remote;

use std::rc::Rc;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, ElementId, Entity, FontWeight, Hsla, IntoElement, Render,
    SharedString, Subscription, Window, div,
};
use kodem_git::ai::{CodexModel, OllamaModel};
use kodem_git::settings::Settings;

use crate::components::Button;
use crate::theme::{RADIUS_CONTROL, RADIUS_SMALL, Theme, px, text_px};
use crate::ui::controls::{Slider, Switch};
use crate::ui::dither::{Dither, DitherField, DottedRule, Edge};
use crate::ui::git::account_link::AccountLinkForm;
use crate::ui::git::components::mono_font;
use crate::ui::git::kit::{ActiveTheme as _, InputEvent, InputState, TextareaState, h_flex};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::theme::liquid_colors;
use crate::ui::settings::{self as preferences, GROUP_GAP, SLIDER_COLUMN, chosen_layer, note};
use crate::ui::{figure, fx};

/// One page of the Git section of Preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitSettingsPage {
    /// Auto-fetch, repository defaults, confirmations and the welcome tour.
    General,
    /// Pull, fetch, push, merge and commit signing.
    Remote,
    /// The diff view and the commit editor.
    Diffs,
    /// What the commit list shows.
    Display,
    /// Linked GitHub accounts.
    Accounts,
    /// The AI provider, its models and prompts.
    Ai,
    /// Files that the Changes view hides in the active repository.
    LocalIgnore,
}

impl GitSettingsPage {
    /// The pages in the order of the tab row.
    pub const ALL: [GitSettingsPage; 7] = [
        GitSettingsPage::General,
        GitSettingsPage::Remote,
        GitSettingsPage::Diffs,
        GitSettingsPage::Display,
        GitSettingsPage::Accounts,
        GitSettingsPage::Ai,
        GitSettingsPage::LocalIgnore,
    ];

    /// Returns the name of the page in the tab row.
    pub fn label(self) -> &'static str {
        match self {
            GitSettingsPage::General => "General",
            GitSettingsPage::Remote => "Remote",
            GitSettingsPage::Diffs => "Diffs",
            GitSettingsPage::Display => "Display",
            GitSettingsPage::Accounts => "Accounts",
            GitSettingsPage::Ai => "AI",
            GitSettingsPage::LocalIgnore => "Local Ignore",
        }
    }
}

/// The pages of the Git section of Preferences. It draws the open page; the section above it
/// shows the tab row that chooses the page.
pub struct GitSettingsView {
    page: GitSettingsPage,
    gpg_key: Entity<InputState>,
    ollama_url: Entity<InputState>,
    commit_prompt: Entity<TextareaState>,
    review_prompt: Entity<TextareaState>,
    explain_prompt: Entity<TextareaState>,
    pull_request_review_prompt: Entity<TextareaState>,
    account_link: Entity<AccountLinkForm>,
    ignore_path: Entity<InputState>,
    ai_status: Option<String>,
    ai_models: Vec<OllamaModel>,
    codex_models: Vec<CodexModel>,
    ai_loading: bool,
    _subscriptions: Vec<Subscription>,
}

impl GitSettingsView {
    /// Creates the pages with their fields filled from the saved settings, open on General.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let s = GitSettings::get(cx).clone();
        let gpg_key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("GPG Key ID")
                .default_value(s.gpg_key_id.clone())
        });
        let ollama_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("http://localhost:11434")
                .default_value(s.ollama_base_url.clone())
        });
        let commit_prompt = prompt_field(
            "Use the built-in commit prompt",
            &s.ai_commit_prompt,
            window,
            cx,
        );
        let review_prompt = prompt_field(
            "Use the built-in review prompt",
            &s.ai_review_prompt,
            window,
            cx,
        );
        let explain_prompt = prompt_field(
            "Use the built-in explanation prompt",
            &s.ai_explain_prompt,
            window,
            cx,
        );
        let pull_request_review_prompt = prompt_field(
            "Use the built-in pull request prompt",
            &s.ai_pull_request_review_prompt,
            window,
            cx,
        );
        let account_link = cx.new(|cx| AccountLinkForm::new(window, cx));
        let ignore_path = cx.new(|cx| InputState::new(window, cx).placeholder("path/to/file"));

        let subscriptions = vec![
            save_on_change(&gpg_key, |s, value| s.gpg_key_id = value, cx),
            save_on_change(&ollama_url, |s, value| s.ollama_base_url = value, cx),
            save_on_change(&commit_prompt, |s, value| s.ai_commit_prompt = value, cx),
            save_on_change(&review_prompt, |s, value| s.ai_review_prompt = value, cx),
            save_on_change(&explain_prompt, |s, value| s.ai_explain_prompt = value, cx),
            save_on_change(
                &pull_request_review_prompt,
                |s, value| s.ai_pull_request_review_prompt = value,
                cx,
            ),
        ];

        Self {
            page: GitSettingsPage::General,
            gpg_key,
            ollama_url,
            commit_prompt,
            review_prompt,
            explain_prompt,
            pull_request_review_prompt,
            account_link,
            ignore_path,
            ai_status: None,
            ai_models: Vec::new(),
            codex_models: Vec::new(),
            ai_loading: false,
            _subscriptions: subscriptions,
        }
    }

    /// Returns the open page.
    pub fn page(&self) -> GitSettingsPage {
        self.page
    }

    /// Opens a page. The AI page checks its provider the first time it opens.
    pub fn set_page(&mut self, page: GitSettingsPage, cx: &mut Context<Self>) {
        self.page = page;
        if page == GitSettingsPage::Ai && self.ai_status.is_none() {
            self.refresh_ai_status(cx);
        }
        cx.notify();
    }
}

// Creates a multi-line field for a custom prompt.
fn prompt_field(
    placeholder: &'static str,
    value: &str,
    window: &mut Window,
    cx: &mut Context<GitSettingsView>,
) -> Entity<TextareaState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line()
            .placeholder(placeholder)
            .default_value(value.to_string())
    })
}

// Saves the text of a field into the settings each time it changes.
fn save_on_change(
    state: &Entity<InputState>,
    apply: fn(&mut Settings, String),
    cx: &mut Context<GitSettingsView>,
) -> Subscription {
    cx.subscribe(state, move |_, state, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
            let value = state.read(cx).value().to_string();
            GitSettings::update(cx, |s| apply(s, value));
        }
    })
}

impl Render for GitSettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.page {
            GitSettingsPage::General => self.render_general(cx),
            GitSettingsPage::Remote => self.render_remote(cx),
            GitSettingsPage::Diffs => self.render_diffs(cx),
            GitSettingsPage::Display => self.render_display(cx),
            GitSettingsPage::Accounts => self.render_accounts(cx),
            GitSettingsPage::Ai => self.render_ai(cx),
            GitSettingsPage::LocalIgnore => self.render_local_ignore(cx),
        }
    }
}

// ---------------------------------------------------------------------------
// Rows shared by the pages, by the Git onboarding and by the account form.
// ---------------------------------------------------------------------------

// Returns the column of one page: its groups with the gap of a preferences section.
fn page() -> Div {
    div().w_full().flex().flex_col().gap(px(GROUP_GAP))
}

/// Starts a group of settings: a section header that its rows follow.
fn section(label: &'static str, cx: &App) -> Div {
    preferences::setting_group(label, None, cx.theme().kodem)
}

/// A bordered card row for lists inside settings, such as accounts and
/// installed models.
pub(crate) fn list_card(selected: bool, cx: &App) -> Div {
    let colors = liquid_colors(cx);
    h_flex()
        .w_full()
        .px_3()
        .py_2()
        .gap_3()
        .items_center()
        .rounded(px(8.))
        .border_1()
        .border_color(if selected {
            cx.theme().primary.opacity(0.55)
        } else {
            cx.theme().border
        })
        .bg(if selected {
            cx.theme().primary.opacity(0.06)
        } else {
            colors.panel_secondary
        })
}

/// A status note with a dithered edge, used for provider and account
/// state messages.
pub(crate) fn status_note(message: impl Into<SharedString>, tone: Hsla) -> Div {
    let message: SharedString = message.into();
    h_flex()
        .relative()
        .w_full()
        .pl_3p5()
        .pr_3()
        .py_2()
        .rounded(px(7.))
        .overflow_hidden()
        .border_1()
        .border_color(tone.opacity(0.28))
        .bg(tone.opacity(0.07))
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(5.))
                .child(
                    Dither::new(DitherField::Fade {
                        from: Edge::Left,
                        start: 1.0,
                        end: 0.3,
                    })
                    .color(tone)
                    .grid(1.0, 1.0)
                    .layer(),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(mono_font())
                .text_size(text_px(11.5))
                .text_color(tone)
                .child(message),
        )
}

/// Returns one setting: its label over its description, beside its control and above a
/// dotted rule.
pub(crate) fn setting_row(
    label: &str,
    description: &str,
    control: impl IntoElement,
    cx: &App,
) -> AnyElement {
    preferences::setting_row(
        label,
        description,
        control.into_any_element(),
        cx.theme().kodem,
    )
}

/// Returns a setting whose control needs the full width, such as a prompt: its label and
/// description over the control, above a dotted rule.
fn wide_setting_row(
    label: &str,
    description: &str,
    control: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let t = cx.theme().kodem;
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .py(px(11.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_size(text_px(13.)).child(label.to_owned()))
                        .child(note(description.to_owned(), t)),
                )
                .child(control),
        )
        .child(DottedRule::horizontal(t.subtle.opacity(0.4)))
        .into_any_element()
}

/// Returns a setting with a switch. `on_toggle` receives the new state.
pub(crate) fn toggle_row(
    id: &'static str,
    label: &'static str,
    description: &'static str,
    checked: bool,
    on_toggle: impl Fn(bool, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    setting_row(
        label,
        description,
        Switch::new(id, checked)
            .label(label)
            .on_click(move |value: &bool, _, cx| on_toggle(*value, cx)),
        cx,
    )
}

/// Returns a setting with a slider beside the readout of its value.
fn slider_row(
    label: &'static str,
    description: &'static str,
    readout: String,
    slider: Slider,
    cx: &App,
) -> AnyElement {
    let t = cx.theme().kodem;
    setting_row(
        label,
        description,
        div()
            .w(px(SLIDER_COLUMN))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().flex_1().min_w_0().child(slider.label(label)))
            .child(
                div()
                    .w(px(44.))
                    .flex()
                    .justify_end()
                    .child(figure(readout, t.text)),
            ),
        cx,
    )
}

/// Returns a segmented control: a bordered group with a cell for each option. The chosen cell
/// carries the accent tint, the dithered wash and the accent edge. `on_select` receives the
/// option of the cell that the user selects.
pub(crate) fn segmented<T, L>(
    id: &'static str,
    options: impl IntoIterator<Item = (T, L)>,
    current: &T,
    on_select: impl Fn(&T, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement
where
    T: Clone + PartialEq + 'static,
    L: Into<SharedString>,
{
    let t = cx.theme().kodem;
    let on_select = Rc::new(on_select);
    let mut group = div()
        .flex()
        .flex_wrap()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(RADIUS_CONTROL))
        .border_1()
        .border_color(t.edge)
        .bg(t.inset);
    for (index, (value, label)) in options.into_iter().enumerate() {
        let chosen = value == *current;
        let on_select = on_select.clone();
        group = group.child(
            segment((id, index), t)
                .text_color(if chosen { t.text } else { t.muted })
                .aria_selected(chosen)
                .when(chosen, |cell| {
                    cell.cursor_default()
                        .child(chosen_layer(t.accent, RADIUS_SMALL))
                })
                .child(label.into())
                .when(!chosen, |cell| {
                    cell.hover(move |cell| cell.bg(t.selected).text_color(t.text))
                        .on_press(move |position, cx| fx::ripple(position, t.muted, cx))
                        .on_click(move |_, window, cx| on_select(&value, window, cx))
                }),
        );
    }
    group.into_any_element()
}

// Returns one cell of a segmented control: Kodem's control geometry and mono label, without a
// surface of its own.
fn segment(id: impl Into<ElementId>, theme: Theme) -> Button {
    Button::new(id)
        .relative()
        .h(px(22.))
        .px(px(10.))
        .gap(px(5.))
        .rounded(px(RADIUS_SMALL))
        .overflow_hidden()
        .border_1()
        .border_color(gpui::transparent_black())
        .bg(gpui::transparent_black())
        .font_family(crate::theme::mono())
        .text_size(text_px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .styles(|s| s.disabled(|s| s.opacity(0.4).cursor_default()))
        // Only keyboard focus draws the edge; a click must not leave a control outlined.
        .focus_visible(move |s| s.border_color(theme.accent.opacity(0.7)))
}
