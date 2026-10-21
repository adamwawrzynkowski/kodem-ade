use super::git::settings_view::GitSettingsPage;
use super::*;
use crate::icons::Icon;

/// One page of Preferences. The navigation lists it under its category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SettingsPage {
    #[default]
    Theme,
    Typography,
    Interface,
    Tasks,
    Layout,
    Projects,
    Import,
    /// What applies to every provider.
    Agents,
    /// One page of a provider. The navigation lists it under the provider.
    Provider(Provider, ProviderPage),
    /// The Git section. Its view keeps which of its own pages is open.
    Git,
    Events,
    Sound,
    Shortcuts,
}

impl SettingsPage {
    /// Returns the name of the page in the navigation and over its content.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Typography => "Typography",
            Self::Interface => "Interface",
            Self::Tasks => "Tasks",
            Self::Layout => "Layout",
            Self::Projects => "Projects",
            Self::Import => "Import",
            Self::Agents => "General",
            Self::Provider(_, page) => page.label(),
            Self::Git => "Git",
            Self::Events => "Events",
            Self::Sound => "Sound",
            Self::Shortcuts => "Keyboard shortcuts",
        }
    }

    // Returns the description under the page's title.
    fn detail(self) -> &'static str {
        match self {
            Self::Theme => {
                "Choose the colors of the workspace and its accent, and the background behind the chat."
            }
            Self::Typography => "Set the typefaces and the sizes of text and code.",
            Self::Interface => "Set the size of the controls and the spacing of the task list.",
            Self::Tasks => "Choose where new tasks work and how the task list sorts them.",
            Self::Layout => "Choose where file diffs open and how wide the sidebar is.",
            Self::Projects => {
                "Customize how repositories appear, and which provider, account and model their new tasks start with."
            }
            Self::Import => "Bring conversations from other coding agents into Kodem.",
            Self::Agents => {
                "Set what applies to every provider: messages during a turn, the shared compaction rule and the background host."
            }
            Self::Provider(_, page) => page.detail(),
            Self::Git => {
                "Choose how the Git workspace syncs with remotes, shows diffs and history, and uses AI."
            }
            Self::Events => {
                "Choose the task events that send a notification, even after closing the window."
            }
            Self::Sound => "Choose the sound of a notification and send a test.",
            Self::Shortcuts => "Move around the workspace without leaving the keyboard.",
        }
    }

    /// Returns the category that lists the page.
    pub(super) fn category(self) -> &'static SettingsCategory {
        let index = match self {
            Self::Theme | Self::Typography | Self::Interface => 0,
            Self::Tasks | Self::Layout | Self::Projects | Self::Import => 1,
            Self::Agents | Self::Provider(..) => 2,
            Self::Git => 3,
            Self::Events | Self::Sound => 4,
            Self::Shortcuts => 5,
        };
        &CATEGORIES[index]
    }

    /// Returns the provider whose page this is.
    pub(super) fn provider(self) -> Option<Provider> {
        match self {
            Self::Provider(provider, _) => Some(provider),
            _ => None,
        }
    }

    /// Returns the path of the page in the navigation, such as "Agents › Claude Code › Model".
    /// A category of one page is the whole path.
    pub(super) fn trail(self) -> String {
        let category = self.category();
        match self {
            Self::Provider(provider, page) => format!(
                "{} › {} › {}",
                category.label,
                provider.label(),
                page.label()
            ),
            _ if category.pages.len() == 1 && self != Self::Agents => category.label.to_owned(),
            _ => format!("{} › {}", category.label, self.label()),
        }
    }

    // Returns where the page sits, over its title: its category and its provider.
    fn section(self) -> String {
        match self {
            Self::Provider(provider, _) => {
                format!("{} › {}", self.category().label, provider.label())
            }
            _ => self.category().label.to_owned(),
        }
    }
}

/// One page of a provider in Preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderPage {
    Connection,
    Accounts,
    Model,
    Access,
    Compaction,
    /// The Claude Code features that the CLI keeps off in a session that an app starts.
    Features,
}

impl ProviderPage {
    /// The pages in navigation order. [`ProviderPage::pages`] lists those of one provider.
    const ALL: [ProviderPage; 6] = [
        ProviderPage::Connection,
        ProviderPage::Accounts,
        ProviderPage::Model,
        ProviderPage::Access,
        ProviderPage::Compaction,
        ProviderPage::Features,
    ];

    /// Returns the pages of a provider in navigation order.
    pub(super) fn pages(provider: Provider) -> impl Iterator<Item = ProviderPage> {
        Self::ALL
            .into_iter()
            .filter(move |page| *page != Self::Features || provider == Provider::Claude)
    }

    // Returns the name of the page in the navigation and over its content.
    fn label(self) -> &'static str {
        match self {
            Self::Connection => "Connection",
            Self::Accounts => "Accounts",
            Self::Model => "Model",
            Self::Access => "Access",
            Self::Compaction => "Compaction",
            Self::Features => "Features",
        }
    }

    // Returns the description under the page's title.
    fn detail(self) -> &'static str {
        match self {
            Self::Connection => {
                "See the provider's local CLI, refresh it and choose it for new conversations."
            }
            Self::Accounts => {
                "Stay signed in to several accounts of one provider and choose one per task. Each account is its own configuration directory of the provider's CLI, which keeps the sign-in. Kodem never reads or stores credentials."
            }
            Self::Model => "Choose the model and reasoning that new conversations start with.",
            Self::Access => "Choose the approval rule that new conversations start with.",
            Self::Compaction => {
                "Choose when Kodem asks this provider to summarize a task's context."
            }
            Self::Features => {
                "Turn on the Claude Code features that the CLI keeps off in sessions that an app starts."
            }
        }
    }
}

/// One category of Preferences: its icon and name in the navigation, and its pages.
pub(super) struct SettingsCategory {
    pub(super) icon: &'static str,
    pub(super) label: &'static str,
    /// The pages under the category. Agents also lists the pages of each provider, and Git
    /// the pages of its own view.
    pub(super) pages: &'static [SettingsPage],
}

impl SettingsCategory {
    /// Returns every page of the category with the pages of each provider, in navigation
    /// order.
    pub(super) fn all_pages(&self) -> Vec<SettingsPage> {
        let mut pages = self.pages.to_vec();
        if self.pages.contains(&SettingsPage::Agents) {
            for provider in Provider::ALL {
                pages.extend(
                    ProviderPage::pages(provider)
                        .map(|page| SettingsPage::Provider(provider, page)),
                );
            }
        }
        pages
    }
}

/// The categories of Preferences in navigation order.
pub(super) static CATEGORIES: [SettingsCategory; 6] = [
    SettingsCategory {
        icon: "sun",
        label: "Appearance",
        pages: &[
            SettingsPage::Theme,
            SettingsPage::Typography,
            SettingsPage::Interface,
        ],
    },
    SettingsCategory {
        icon: "folder",
        label: "Workspace",
        pages: &[
            SettingsPage::Tasks,
            SettingsPage::Layout,
            SettingsPage::Projects,
            SettingsPage::Import,
        ],
    },
    SettingsCategory {
        icon: "bot",
        label: "Agents",
        pages: &[SettingsPage::Agents],
    },
    SettingsCategory {
        icon: "git-pull-request",
        label: "Git",
        pages: &[SettingsPage::Git],
    },
    SettingsCategory {
        icon: "bell",
        label: "Notifications",
        pages: &[SettingsPage::Events, SettingsPage::Sound],
    },
    SettingsCategory {
        icon: "keyboard",
        label: "Shortcuts",
        pages: &[SettingsPage::Shortcuts],
    },
];

// A row of the navigation opens a page of Preferences, or a page of the Git section.
#[derive(Clone, Copy)]
enum NavTarget {
    Page(SettingsPage),
    Git(GitSettingsPage),
}

impl NavTarget {
    // Returns the element id of the row that opens this target.
    fn id(self) -> ElementId {
        ElementId::Name(match self {
            Self::Page(page) => format!("settings-page:{page:?}").into(),
            Self::Git(page) => format!("settings-git-page:{page:?}").into(),
        })
    }
}

// One row of the navigation and the rows under it, which show while it is open.
struct NavRow {
    target: NavTarget,
    glyph: Option<&'static str>,
    label: &'static str,
    chosen: bool,
    open: bool,
    children: Vec<NavRow>,
}

impl NavRow {
    // Returns a row without rows under it.
    fn leaf(target: NavTarget, label: &'static str, chosen: bool) -> Self {
        Self {
            target,
            glyph: None,
            label,
            chosen,
            open: false,
            children: Vec::new(),
        }
    }
}

/// Width of the navigation of Preferences.
const NAVIGATION_WIDTH: f32 = 212.;
/// Space between the edges of the content of a Preferences page and its column.
const PAGE_INSET: f32 = 28.;

/// The key chords of the workspace in the groups of the Shortcuts page. A slash separates the
/// chords that do the same thing.
const SHORTCUTS: [(&str, &[(&str, &str)]); 3] = [
    (
        "Navigation",
        &[
            ("Command palette", "⌘ K"),
            ("New task", "⌘ N"),
            ("Open repository", "⌘ O"),
            ("Clone repository into Git", "⇧ ⌘ O"),
            ("Search tasks, or commits in Git", "⌘ F"),
            ("Next or previous task", "⌃ Tab / ⌃ ⇧ Tab / ⇧ ⌘ ] / ⇧ ⌘ ["),
            ("Go to a task in the feed", "⌘ 1 – ⌘ 9"),
            ("Next task that needs you", "⌥ ⌘ A"),
            (
                "Agentic Coding, Chat, Review or Git",
                "⌥ ⌘ 1 / ⌥ ⌘ 2 / ⌥ ⌘ 3 / ⌥ ⌘ 4",
            ),
            ("Settings", "⌘ ,"),
            ("Close dialog or menu", "Esc"),
        ],
    ),
    (
        "Composer",
        &[
            ("Send message", "Return"),
            ("Send the other way during a turn", "⌘ Return"),
            ("New line in prompt", "⇧ Return"),
            ("Provider, account and model", "⇧ ⌘ M"),
            ("Reasoning effort", "⇧ ⌘ E"),
            ("Access mode", "⇧ ⌘ A"),
            ("Switch build and plan, or cycle Codex access", "⇧ Tab"),
        ],
    ),
    (
        "Panels",
        &[
            ("Toggle sidebar", "⌘ B"),
            ("Toggle changes", "⌘ D"),
            ("Toggle browser", "⇧ ⌘ B"),
            ("Toggle terminal", "⌃ `"),
            ("Copy diff selection", "⌘ C"),
        ],
    ),
];

// Returns the column of one Preferences page: its groups with the gap between groups.
fn page_column() -> Div {
    div().flex().flex_col().gap(px(GROUP_GAP))
}

/// Space between the groups of one preferences section.
pub(super) const GROUP_GAP: f32 = 18.;
/// Width of the control column that holds a slider and its readout.
pub(super) const SLIDER_COLUMN: f32 = 220.;
/// Widest control column of a settings row. A wider segmented control wraps.
const CONTROL_COLUMN: f32 = 340.;
/// Step of a compaction threshold slider, in percent.
const THRESHOLD_STEP: f32 = 5.;
/// Step of the interface scale slider, in percent.
const SCALE_STEP: f32 = 5.;
/// Cards in one row of the theme picker and of a font picker.
const PICKER_COLUMNS: u16 = 3;
/// Space between the cards of a picker.
const PICKER_GAP: f32 = 8.;
/// Height of the miniature workspace in a theme card.
const THEME_PREVIEW: f32 = 86.;
/// Name of the switch that turns on Artifacts in Claude Code sessions.
const ARTIFACTS_LABEL: &str = "Artifacts, /design and /slides";
/// What the Artifacts switch does and what it needs.
const ARTIFACTS_DETAIL: &str = "Claude Code keeps Artifacts off in sessions that an app starts. Turn this on so that /design and /slides publish to claude.ai. This needs a claude.ai sign-in on a Pro, Max, Team or Enterprise plan. Each task applies the change with its next message.";
/// Name of the switch that keeps the computer awake while an agent works.
const AWAKE_LABEL: &str = "Keep the computer awake";
/// What the switch that keeps the computer awake does.
const AWAKE_DETAIL: &str = "While an agent works, the computer does not go to sleep on its own, also with the window closed. The display can still turn off. Closing the lid still puts a laptop to sleep.";
/// Name of the wallpaper setting.
const WALLPAPER_LABEL: &str = "Wallpaper";
/// What the wallpaper setting shows and where Kodem keeps the image.
const WALLPAPER_DETAIL: &str = "An image behind the background effects of the chat. Kodem keeps its own copy, so the original file can move or go.";
/// Name of the switch that starts a wave where a press lands on the chat background.
const CLICK_WAVES_LABEL: &str = "Click waves";
/// What the click waves switch does.
const CLICK_WAVES_DETAIL: &str =
    "A click on the chat background starts a wave of characters there.";
/// Name of the switch that lays the dot screen over the chat background.
const DOT_SCREEN_LABEL: &str = "Dot screen";
/// What the dot screen switch shows.
const DOT_SCREEN_DETAIL: &str = "A dithered dot screen over the top of the chat background. With a wallpaper, the dots take its colors.";
/// Size of the wallpaper's thumbnail beside its buttons.
const WALLPAPER_THUMBNAIL: (f32, f32) = (48., 30.);
/// Name of the switch that shows the skater on the composer.
const SKATER_LABEL: &str = "Composer skater";
/// What the skater switch shows.
const SKATER_DETAIL: &str = "A pixel avatar of the project skates on the composer while its agent works and waits there while it rests. Scroll up and it falls off. Drag it to throw it. Choose the avatar in the project's settings.";

// One place where a compaction rule is stored: the shared rule, a provider or one of its models.
#[derive(Clone)]
enum CompactScope {
    All,
    Provider(Provider),
    Model(Provider, String),
}

impl CompactScope {
    // Returns a stable name for the element ids of this scope's controls.
    fn key(&self) -> String {
        match self {
            Self::All => "all".to_owned(),
            Self::Provider(provider) => provider.icon().to_owned(),
            Self::Model(provider, model) => AutoCompact::model_key(*provider, model),
        }
    }

    // Returns the rule stored for this scope, when the scope has a rule of its own.
    fn stored<'a>(&self, rules: &'a mut AutoCompact) -> Option<&'a mut CompactRule> {
        match self {
            Self::All => Some(&mut rules.default),
            Self::Provider(provider) => rules.providers.get_mut(provider),
            Self::Model(provider, model) => rules
                .models
                .get_mut(&AutoCompact::model_key(*provider, model)),
        }
    }
}

/// The look of a chosen surface as one layer: the accent tint with slight depth, faint dots
/// along its bottom edge and the accent edge. Add it to a relatively positioned surface with
/// [`kit::Layers::layer`].
pub(super) fn chosen_layer(tone: Hsla) -> impl kit::Fit {
    move |corners: Corners<Pixels>| {
        kit::round(div().absolute().inset_0(), corners)
            .bg(kit::tint_fill(tone, 0.1))
            .shadow(kit::tint_light(tone))
            .layer(dither::row_wash(tone, true, false))
            .child(kit::selection_edge(tone))
            .into_any_element()
    }
}

/// The frame of a text field in a dialog or a form: a recessed inset surface with the focus
/// edge. Add the input as a child that fills the width.
pub(super) fn input_frame(theme: Theme) -> Div {
    div()
        .relative()
        .h(px(30.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(crate::theme::RADIUS_CONTROL))
        .bg(theme.inset)
        .shadow(kit::recessed_light(theme))
        .border_1()
        .border_color(theme.accent.opacity(0.5))
}

/// A failure report as a content block: a header in the failure tone over the message.
pub(super) fn error_block(label: &str, message: impl Into<SharedString>, theme: Theme) -> Div {
    kit::block(theme)
        .border_color(theme.removed.opacity(0.3))
        .bg(theme.removed.opacity(0.05))
        .child(
            kit::block_header(label, theme.removed, theme)
                .bg(theme.removed.opacity(0.07))
                .border_color(theme.removed.opacity(0.22)),
        )
        .child(
            div()
                .px(px(12.))
                .py(px(9.))
                .text_size(text_px(12.))
                .line_height(relative(1.45))
                .text_color(theme.text)
                .child(message.into()),
        )
}

/// Starts a group of settings: a section header that its rows follow without a gap.
pub(super) fn setting_group(label: &str, count: Option<usize>, theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .child(kit::section_header(label, count, theme))
}

/// The system that shows Kodem's notifications, as the interface names it.
#[cfg(target_os = "macos")]
pub(super) const NOTIFIER: &str = "macOS";
#[cfg(windows)]
pub(super) const NOTIFIER: &str = "Windows";
#[cfg(not(any(target_os = "macos", windows)))]
pub(super) const NOTIFIER: &str = "desktop";

/// Where the user changes how the system presents Kodem's notifications.
#[cfg(target_os = "macos")]
const NOTIFICATION_CONTROL: &str =
    "macOS controls notification permission, banners, sounds, and Focus settings.";
#[cfg(windows)]
const NOTIFICATION_CONTROL: &str = "Windows controls notification banners and Do Not Disturb in Settings > System > Notifications. Kodem plays its own sound.";
#[cfg(not(any(target_os = "macos", windows)))]
const NOTIFICATION_CONTROL: &str = "Your desktop's notification service controls banners and Do Not Disturb. Kodem plays its own sound through PipeWire, PulseAudio or ALSA.";

/// Returns one setting: its label over its description, beside its control and above a dotted
/// rule.
pub(super) fn setting_row(
    label: &str,
    detail: &str,
    control: AnyElement,
    theme: Theme,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .py(px(11.))
                .flex()
                .items_center()
                .gap(px(16.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_size(text_px(13.)).child(label.to_owned()))
                        .child(note(detail.to_owned(), theme)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(CONTROL_COLUMN))
                        .flex()
                        .justify_end()
                        .child(control),
                ),
        )
        .child(dither::DottedRule::horizontal(theme.subtle.opacity(0.4)))
        .into_any_element()
}

/// Returns a short explanation in prose below a group header or between rows.
pub(super) fn note(text: impl Into<SharedString>, theme: Theme) -> Div {
    div()
        .text_size(text_px(12.))
        .line_height(relative(1.45))
        .text_color(theme.muted)
        .child(text.into())
}

// Returns the readout of a compaction rule: its threshold, or that it is off.
fn rule_label(rule: CompactRule) -> String {
    if rule.enabled {
        format!("{}%", rule.threshold)
    } else {
        "OFF".to_owned()
    }
}

// Returns the tone of a rule readout: accent for a rule set at this level, quiet for one that
// is inherited or off.
fn rule_tone(rule: CompactRule, own: bool, theme: Theme) -> Hsla {
    if !rule.enabled {
        theme.subtle
    } else if own {
        theme.accent
    } else {
        theme.muted
    }
}

// Labels one level of the compaction rules with the rule that is in effect there. The marker
// is lit when the level sets its own rule.
fn compact_level(label: &str, rule: CompactRule, own: bool, theme: Theme) -> Div {
    div()
        .pt(px(14.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(kit::marker(if own { theme.accent } else { theme.subtle }))
        .child(caps(label, theme).text_color(theme.muted))
        .child(div().flex_1())
        .child(kit::chip(rule_label(rule), rule_tone(rule, own, theme)))
}

// One line of the miniature workspace in a theme card: a short bar in a color of the theme.
fn preview_bar(width: f32, color: Hsla) -> Div {
    div()
        .h(px(3.))
        .w(px(width))
        .flex_shrink_0()
        .rounded(px(1.5))
        .bg(color)
}

// Draws a miniature workspace in the colors of `theme`: the sidebar with a chosen task, a few
// lines in the accent and the status colors, and the composer.
fn theme_preview(theme: Theme) -> Div {
    let line = || div().flex().items_center().gap(px(4.));
    div()
        .h(px(THEME_PREVIEW))
        .flex()
        .overflow_hidden()
        .bg(theme.canvas)
        .child(
            div()
                .w(px(42.))
                .h_full()
                .flex_shrink_0()
                .p(px(6.))
                .flex()
                .flex_col()
                .gap(px(5.))
                .bg(theme.sidebar)
                .border_r_1()
                .border_color(theme.border)
                .child(preview_bar(16., theme.text))
                .child(
                    div()
                        .h(px(9.))
                        .w_full()
                        .flex()
                        .rounded(px(2.))
                        .overflow_hidden()
                        .bg(theme.accent_bg)
                        .child(div().w(px(2.)).h_full().bg(theme.accent)),
                )
                .child(preview_bar(24., theme.subtle))
                .child(preview_bar(18., theme.subtle)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    line()
                        .child(preview_bar(22., theme.accent))
                        .child(preview_bar(38., theme.text)),
                )
                .child(
                    line()
                        .pl(px(8.))
                        .child(preview_bar(16., theme.info))
                        .child(preview_bar(26., theme.added))
                        .child(preview_bar(14., theme.warning)),
                )
                .child(
                    line()
                        .pl(px(8.))
                        .child(preview_bar(20., theme.removed))
                        .child(preview_bar(32., theme.muted)),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .h(px(18.))
                        .px(px(5.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .rounded(px(crate::theme::RADIUS_SMALL))
                        .bg(theme.raised)
                        .border_1()
                        .border_color(theme.edge)
                        .child(preview_bar(34., theme.subtle))
                        .child(div().size(px(8.)).rounded(px(2.)).bg(theme.accent)),
                ),
        )
}

// The chevron of a navigation row with rows under it: down while they show.
fn nav_chevron(open: bool, theme: Theme) -> Icon {
    icon(if open {
        "chevron-down"
    } else {
        "chevron-right"
    })
    .size(px(11.))
    .text_color(theme.subtle)
}

// Lays out the cards of a picker in rows of equal columns.
fn picker_grid(cards: Vec<AnyElement>) -> Div {
    div()
        .grid()
        .grid_cols(PICKER_COLUMNS)
        .gap(px(PICKER_GAP))
        .children(cards)
}

impl Shell {
    /// Builds native preferences: the categories and their pages on the left, and the open
    /// page under its title on the right.
    pub(super) fn settings_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = self.settings_page;
        let git_page = self.git_settings.as_ref().map(|view| view.read(cx).page());
        let open_git_page = git_page.filter(|_| page == SettingsPage::Git);
        let (title, detail) = match open_git_page {
            Some(git_page) => (git_page.label(), git_page.detail()),
            None => (page.label(), page.detail()),
        };
        let content = match page {
            SettingsPage::Theme => self.theme_settings(cx),
            SettingsPage::Typography => self.typography_settings(cx),
            SettingsPage::Interface => self.interface_settings(cx),
            SettingsPage::Tasks => self.task_settings(cx),
            SettingsPage::Layout => self.layout_settings(cx),
            SettingsPage::Projects => self.project_settings(cx),
            SettingsPage::Import => self.import_settings(cx),
            SettingsPage::Agents => self.agent_settings(cx),
            SettingsPage::Provider(provider, page) => self.provider_page(provider, page, cx),
            SettingsPage::Git => self.git_settings(),
            SettingsPage::Events => self.event_settings(cx),
            SettingsPage::Sound => self.sound_settings(cx),
            SettingsPage::Shortcuts => self.shortcut_settings(),
        };
        // Each page, and each page of the Git section, scrolls on its own from the top.
        let scroll = SharedString::from(format!("preferences-content:{page:?}:{open_git_page:?}"));
        div()
            .h(px(520.))
            .flex()
            .child(self.settings_navigation(git_page, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(PAGE_INSET))
                            .pt(px(20.))
                            .child(self.page_header(&page.section(), title, detail)),
                    )
                    .child(
                        div()
                            .id(scroll)
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .when(page == SettingsPage::Theme, |scroll| {
                                scroll.track_scroll(&self.theme_scroll)
                            })
                            .px(px(PAGE_INSET))
                            .pt(px(16.))
                            .pb(px(24.))
                            .child(content),
                    ),
            )
            .into_any_element()
    }

    // Builds the navigation of Preferences: every category, and under the open one its rows
    // along a dotted guide. A category of one page opens it from its own row.
    fn settings_navigation(
        &self,
        git_page: Option<GitSettingsPage>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let open = self.settings_page.category();
        let mut navigation = div()
            .id("preferences-navigation")
            .w(px(NAVIGATION_WIDTH))
            .h_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .p(px(10.))
            // A share of the panel tone, so that the dialog's glass shows through.
            .bg(t.sidebar.opacity(kit::PANE_SHARE))
            .flex()
            .flex_col()
            .gap(px(2.))
            .border_r_1()
            .border_color(t.border)
            .child(caps("Preferences", t).px(px(8.)).pt(px(4.)).pb(px(8.)));
        for (index, category) in CATEGORIES.iter().enumerate() {
            let current = std::ptr::eq(category, open);
            let rows = self.category_rows(category, git_page);
            let first = NavTarget::Page(category.pages[0]);
            navigation = navigation.child(
                self.choice_row(("settings-category", index), current && rows.is_empty())
                    .when(current, |row| row.text_color(t.text).cursor_default())
                    .child(
                        icon(category.icon)
                            .size(px(13.))
                            .when(current, |glyph| glyph.text_color(t.accent)),
                    )
                    .child(div().relative().flex_1().min_w_0().child(category.label))
                    .when(!rows.is_empty(), |row| row.child(nav_chevron(current, t)))
                    .when(!current, |row| {
                        row.on_click(
                            cx.listener(move |s, _, _, cx| s.open_settings_target(first, cx)),
                        )
                    }),
            );
            if current && !rows.is_empty() {
                navigation = navigation.child(self.nav_branch(rows, cx));
            }
        }
        navigation.into_any_element()
    }

    // Returns the rows under a category in the navigation. Agents lists its General page and
    // each provider with its pages; a category of one other page has no rows.
    fn category_rows(
        &self,
        category: &SettingsCategory,
        git_page: Option<GitSettingsPage>,
    ) -> Vec<NavRow> {
        let page = self.settings_page;
        match category.pages {
            [SettingsPage::Git] => GitSettingsPage::ALL
                .into_iter()
                .map(|candidate| {
                    NavRow::leaf(
                        NavTarget::Git(candidate),
                        candidate.label(),
                        page == SettingsPage::Git && git_page == Some(candidate),
                    )
                })
                .collect(),
            [SettingsPage::Agents] => {
                let mut rows = vec![NavRow::leaf(
                    NavTarget::Page(SettingsPage::Agents),
                    SettingsPage::Agents.label(),
                    page == SettingsPage::Agents,
                )];
                for provider in Provider::ALL {
                    rows.push(NavRow {
                        target: NavTarget::Page(SettingsPage::Provider(
                            provider,
                            ProviderPage::Connection,
                        )),
                        glyph: Some(provider.icon()),
                        label: provider.label(),
                        chosen: false,
                        open: page.provider() == Some(provider),
                        children: ProviderPage::pages(provider)
                            .map(|candidate| {
                                let target = SettingsPage::Provider(provider, candidate);
                                NavRow::leaf(
                                    NavTarget::Page(target),
                                    candidate.label(),
                                    page == target,
                                )
                            })
                            .collect(),
                    });
                }
                rows
            }
            [_] => Vec::new(),
            pages => pages
                .iter()
                .map(|candidate| {
                    NavRow::leaf(
                        NavTarget::Page(*candidate),
                        candidate.label(),
                        *candidate == page,
                    )
                })
                .collect(),
        }
    }

    // Lays out rows of the navigation along a dotted guide, each open row with its own rows
    // under it, one step further in.
    fn nav_branch(&self, rows: Vec<NavRow>, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut branch = div()
            .relative()
            .ml(px(16.))
            .pl(px(8.))
            .pb(px(4.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(2.))
                    .bottom(px(6.))
                    .child(dither::DottedRule::vertical(t.subtle.opacity(0.45))),
            );
        for row in rows {
            let target = row.target;
            let parent = !row.children.is_empty();
            // A parent row opens its first page; a chosen or open row has nothing to open.
            let idle = row.chosen || row.open;
            branch = branch.child(
                self.choice_row(target.id(), row.chosen)
                    .h(px(26.))
                    .font_family(crate::theme::sans())
                    .font_weight(FontWeight::NORMAL)
                    .text_size(text_px(12.5))
                    .when(row.open, |item| item.text_color(t.text))
                    .when(idle, |item| item.cursor_default())
                    .when_some(row.glyph, |item, glyph| {
                        item.child(
                            icon(glyph)
                                .size(px(12.))
                                .when(row.open, |glyph| glyph.text_color(t.accent)),
                        )
                    })
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(row.label),
                    )
                    .when(parent, |item| item.child(nav_chevron(row.open, t)))
                    .when(!idle, |item| {
                        item.on_click(
                            cx.listener(move |s, _, _, cx| s.open_settings_target(target, cx)),
                        )
                    }),
            );
            if row.open && parent {
                branch = branch.child(self.nav_branch(row.children, cx));
            }
        }
        branch
    }

    // Opens the page that a row of the navigation names.
    fn open_settings_target(&mut self, target: NavTarget, cx: &mut Context<Self>) {
        match target {
            NavTarget::Page(page) => self.open_settings(page),
            NavTarget::Git(page) => {
                self.open_settings(SettingsPage::Git);
                if let Some(view) = &self.git_settings {
                    view.update(cx, |view, cx| view.set_page(page, cx));
                }
            }
        }
        cx.notify();
    }

    // Returns the heading of a Preferences page: its category beside a marker, its title and
    // its description, closed by a dotted rule above the scrolling content.
    fn page_header(&self, category: &str, title: &str, detail: &str) -> Div {
        let t = self.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(kit::marker(t.accent))
                    .child(caps(category, t).text_color(t.muted)),
            )
            .child(
                div()
                    .text_size(text_px(16.))
                    .line_height(relative(1.3))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_owned()),
            )
            .child(note(detail.to_owned(), t))
            .child(
                div()
                    .pt(px(8.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.45))),
            )
    }

    /// Opens one page of the composer's provider.
    pub(super) fn open_provider_settings(&mut self, page: ProviderPage) {
        let provider = self.composer_preferences().provider;
        self.open_settings(SettingsPage::Provider(provider, page));
    }

    /// Saves the theme and applies it at once.
    pub(super) fn set_theme(&mut self, theme: ThemeId, cx: &mut Context<Self>) {
        self.settings.theme = theme;
        self.theme = Theme::with_accent(theme, self.settings.accent);
        self.save_settings();
        cx.notify();
    }

    /// Returns the setting that chooses between the dark and the light default theme. The
    /// theme picker in Settings has the other themes.
    pub(super) fn appearance_setting(&self, cx: &mut Context<Self>) -> AnyElement {
        self.setting_row(
            "Appearance",
            "Graphite is the dark theme and Paper the light one. Settings has more themes.",
            self.segmented(
                "theme-choice",
                [(false, "Dark"), (true, "Light")],
                &self.theme.light,
                |s, light, cx| {
                    s.set_theme(
                        if light {
                            ThemeId::Paper
                        } else {
                            ThemeId::Graphite
                        },
                        cx,
                    )
                },
                cx,
            ),
        )
    }

    // Returns one card of the theme picker: a miniature workspace in the theme's colors over
    // the theme's name. The chosen card carries the accent edge and a check mark.
    fn theme_card(&self, theme: ThemeId, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let chosen = self.settings.theme == theme;
        let index = ThemeId::ALL
            .iter()
            .position(|candidate| *candidate == theme)
            .unwrap_or_default();
        Button::new(("theme-card", index))
            .relative()
            .min_w_0()
            .flex_col()
            .items_stretch()
            .justify_start()
            .overflow_hidden()
            .rounded(px(crate::theme::RADIUS_SURFACE))
            .border_1()
            .border_color(if chosen { t.accent } else { t.border })
            // A card stands a little off its surface, and sinks under a press.
            .shadow(kit::plate_light(t, true))
            .accessibility_label(theme.label())
            .aria_selected(chosen)
            .focus_visible(move |card| card.border_color(t.accent.opacity(0.7)))
            .when(!chosen, |card| {
                card.cursor_pointer()
                    .hover(move |card| card.border_color(t.edge))
                    .active(move |card| card.shadow(kit::recessed_light(t)))
                    .on_press(move |position, cx| fx::ripple(position, t.muted, cx))
                    .on_click(cx.listener(move |s, _, _, cx| s.set_theme(theme, cx)))
            })
            .child(theme_preview(Theme::with_accent(
                theme,
                self.settings.accent,
            )))
            .child(
                div()
                    .relative()
                    .h(px(28.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .bg(t.raised.opacity(kit::PANE_SHARE))
                    .border_t_1()
                    .border_color(t.border)
                    // The footer closes the card, so its corners are the card's inside its edge.
                    .rounded_b(px(crate::theme::RADIUS_SURFACE) - gpui::px(1.))
                    .when(chosen, |footer| footer.layer(chosen_layer(t.accent)))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .font_family(crate::theme::sans())
                            .text_size(text_px(12.))
                            .line_height(relative(1.3))
                            .text_color(if chosen { t.text } else { t.muted })
                            .text_ellipsis()
                            .child(theme.label()),
                    )
                    .when(chosen, |footer| {
                        footer.child(
                            icon("check")
                                .size(px(12.))
                                .flex_shrink_0()
                                .text_color(t.accent),
                        )
                    }),
            )
            .into_any_element()
    }

    // Builds the theme picker: the dark themes, then the light ones, as cards in a grid.
    fn theme_choices(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut picker = setting_group("Theme", Some(ThemeId::ALL.len()), t);
        for (label, light) in [("Dark", false), ("Light", true)] {
            picker = picker
                .child(caps(label, t).pt(px(12.)).pb(px(6.)).text_color(t.muted))
                .child(picker_grid(
                    ThemeId::ALL
                        .into_iter()
                        .filter(|theme| theme.light() == light)
                        .map(|theme| self.theme_card(theme, cx))
                        .collect(),
                ));
        }
        picker.into_any_element()
    }

    // Returns one card of a font picker: the name of the typeface, set in that typeface.
    fn font_card(&self, id: impl Into<ElementId>, family: &'static str, chosen: bool) -> Button {
        let t = self.theme;
        self.choice_row(id, chosen)
            .min_w_0()
            .h(px(34.))
            .border_color(if chosen {
                t.accent.opacity(0.45)
            } else {
                t.border
            })
            .font_family(family)
            .font_weight(FontWeight::NORMAL)
            .text_size(text_px(13.))
            .when(chosen, |card| card.cursor_default())
    }

    // Returns the label of a font card. It keeps its place above the layer of a chosen card
    // and shortens with an ellipsis in a narrow card.
    fn font_label(label: &'static str) -> Div {
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .line_height(relative(1.3))
            .text_ellipsis()
            .child(label)
    }

    // Builds the picker of the typeface for the interface and prose. It lists the bundled
    // typeface, the system one and the others that this Mac has.
    fn text_font_choices(&self, cx: &mut Context<Self>) -> Div {
        let chosen = self.settings.text_font;
        picker_grid(
            TextFont::ALL
                .into_iter()
                .enumerate()
                .filter(|(_, font)| {
                    font.built_in()
                        || *font == chosen
                        || self.installed_fonts.contains(font.family())
                })
                .map(|(index, font)| {
                    self.font_card(("text-font", index), font.family(), font == chosen)
                        .accessibility_label(font.label())
                        .child(Self::font_label(font.label()))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.settings.text_font = font;
                            s.save_settings();
                            cx.notify();
                        }))
                        .into_any_element()
                })
                .collect(),
        )
        .pt(px(10.))
    }

    // Builds the picker of the monospace typeface for code and labels. It lists the bundled
    // typeface and the others that this Mac has.
    fn code_font_choices(&self, cx: &mut Context<Self>) -> Div {
        let chosen = self.settings.code_font;
        picker_grid(
            CodeFont::ALL
                .into_iter()
                .enumerate()
                .filter(|(_, font)| {
                    font.built_in()
                        || *font == chosen
                        || self.installed_fonts.contains(font.family())
                })
                .map(|(index, font)| {
                    self.font_card(("code-font", index), font.family(), font == chosen)
                        .accessibility_label(font.family())
                        .child(Self::font_label(font.family()))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.settings.code_font = font;
                            s.save_settings();
                            cx.notify();
                        }))
                        .into_any_element()
                })
                .collect(),
        )
        .pt(px(10.))
    }

    // Returns the slider of the interface scale with its percent readout. A drag shows the new
    // value and applies it on release: a scale that followed the pointer would move the slider
    // under it.
    fn scale_slider(&self, cx: &mut Context<Self>) -> AnyElement {
        let (smallest, largest) = Settings::UI_SCALE_RANGE;
        let percent = (self
            .pending_scale
            .unwrap_or(self.settings.ui_scale)
            .clamp(smallest, largest)
            * 100.)
            .round();
        self.slider_control(
            controls::Slider::new(
                "ui-scale",
                percent,
                smallest * 100.,
                largest * 100.,
                SCALE_STEP,
                cx.listener(|s, value: &f32, _, cx| {
                    s.pending_scale = Some(value / 100.);
                    cx.notify();
                }),
            )
            .on_release(cx.listener(|s, _: &(), _, cx| {
                if let Some(scale) = s.pending_scale.take() {
                    let (smallest, largest) = Settings::UI_SCALE_RANGE;
                    s.settings.ui_scale = scale.clamp(smallest, largest);
                    s.save_settings();
                    cx.notify();
                }
            }))
            .label("Interface scale"),
            format!("{percent}%"),
            self.theme.text,
        )
    }

    // Builds the Theme page: the theme picker, the accent and the chat background.
    fn theme_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        page_column()
            .child(self.theme_choices(cx))
            .child(self.accent_choices(cx))
            .child(self.background_settings(cx))
            .into_any_element()
    }

    // Builds the Background group of the Theme page: the wallpaper, with its thumbnail when it
    // is set, and the switches of the dot screen and the click waves.
    fn background_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let (width, height) = WALLPAPER_THUMBNAIL;
        let wallpaper = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .when_some(self.settings.wallpaper.clone(), |control, path| {
                control
                    .child(
                        img(path)
                            .w(px(width))
                            .h(px(height))
                            .flex_shrink_0()
                            .rounded(px(crate::theme::RADIUS_CONTROL))
                            .border_1()
                            .border_color(t.border)
                            .object_fit(ObjectFit::Cover),
                    )
                    .child(
                        self.button("wallpaper-remove", "trash-2", "Remove")
                            .on_click(cx.listener(|s, _, _, cx| s.remove_wallpaper(cx))),
                    )
            })
            .child(
                self.button("wallpaper-choose", "folder-open", "Choose…")
                    .on_click(cx.listener(|s, _, _, cx| s.choose_wallpaper(cx))),
            );
        setting_group("Background", None, t)
            .child(self.setting_row(
                WALLPAPER_LABEL,
                WALLPAPER_DETAIL,
                wallpaper.into_any_element(),
            ))
            .child(
                self.setting_row(
                    DOT_SCREEN_LABEL,
                    DOT_SCREEN_DETAIL,
                    controls::Switch::new("dot-screen", self.settings.dot_screen)
                        .label(DOT_SCREEN_LABEL)
                        .on_click(cx.listener(|s, enabled: &bool, _, cx| {
                            s.settings.dot_screen = *enabled;
                            s.save_settings();
                            cx.notify();
                        }))
                        .into_any_element(),
                ),
            )
            .child(
                self.setting_row(
                    CLICK_WAVES_LABEL,
                    CLICK_WAVES_DETAIL,
                    controls::Switch::new("click-waves", self.settings.click_waves)
                        .label(CLICK_WAVES_LABEL)
                        .on_click(cx.listener(|s, enabled: &bool, _, cx| {
                            s.settings.click_waves = *enabled;
                            s.save_settings();
                            cx.notify();
                        }))
                        .into_any_element(),
                ),
            )
            .into_any_element()
    }

    /// Opens the Theme page of preferences at the wallpaper, at the end of the page.
    pub(super) fn open_wallpaper_settings(&mut self) {
        self.open_settings(SettingsPage::Theme);
        self.theme_scroll.scroll_to_bottom();
    }

    // Asks for an image, copies it into Kodem's data directory and makes the copy the
    // wallpaper. The copy is made away from the window, which keeps the earlier wallpaper until
    // it is done.
    fn choose_wallpaper(&mut self, cx: &mut Context<Self>) {
        let selected = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose wallpaper".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = selected.await else {
                return;
            };
            let Some(source) = paths.into_iter().next() else {
                return;
            };
            let cached = cx
                .background_executor()
                .spawn(async move { crate::wallpaper::cache(&source) })
                .await;
            let _ = this.update(cx, |s, cx| match cached {
                Ok(path) => {
                    if let Some(previous) = s.settings.wallpaper.replace(path)
                        && s.settings.wallpaper.as_ref() != Some(&previous)
                    {
                        crate::wallpaper::release(&previous, cx);
                    }
                    s.save_settings();
                    cx.notify();
                }
                Err(error) => s.report_error(
                    format!("Could not use the image as wallpaper: {error:#}"),
                    cx,
                ),
            });
        })
        .detach();
    }

    // Removes the wallpaper and deletes Kodem's copy of it.
    fn remove_wallpaper(&mut self, cx: &mut Context<Self>) {
        let Some(previous) = self.settings.wallpaper.take() else {
            return;
        };
        crate::wallpaper::release(&previous, cx);
        self.save_settings();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let cleared = cx
                .background_executor()
                .spawn(async move { crate::wallpaper::clear(None) })
                .await;
            if let Err(error) = cleared {
                let _ = this.update(cx, |s, cx| {
                    s.report_error(
                        format!("Could not delete the wallpaper's copy: {error:#}"),
                        cx,
                    )
                });
            }
        })
        .detach();
    }

    // Builds the Typography page: the typeface and size of text, then of code.
    fn typography_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let (smallest_text, largest_text) = Settings::FONT_SIZE_RANGE;
        let (smallest_code, largest_code) = Settings::CODE_FONT_SIZE_RANGE;
        page_column()
            .child(
                setting_group("Text", None, t)
                    .child(self.text_font_choices(cx))
                    .child(
                        self.setting_row(
                            "Text size",
                            "Size of the interface, messages and prompt text.",
                            self.slider_control(
                                controls::Slider::new(
                                    "font-size",
                                    self.settings.font_size,
                                    smallest_text,
                                    largest_text,
                                    1.,
                                    cx.listener(move |s, value: &f32, _, cx| {
                                        s.settings.font_size =
                                            value.clamp(smallest_text, largest_text);
                                        s.save_settings();
                                        cx.notify();
                                    }),
                                )
                                .label("Text size"),
                                format!("{} PT", self.settings.font_size),
                                t.text,
                            ),
                        ),
                    ),
            )
            .child(
                setting_group("Code", None, t)
                    .child(self.code_font_choices(cx))
                    .child(
                        self.setting_row(
                            "Code size",
                            "Size of code blocks, diffs, files and terminal output.",
                            self.slider_control(
                                controls::Slider::new(
                                    "code-font-size",
                                    self.settings.code_font_size,
                                    smallest_code,
                                    largest_code,
                                    1.,
                                    cx.listener(move |s, value: &f32, _, cx| {
                                        s.settings.code_font_size =
                                            value.clamp(smallest_code, largest_code);
                                        s.save_settings();
                                        cx.notify();
                                    }),
                                )
                                .label("Code size"),
                                format!("{} PT", self.settings.code_font_size),
                                t.text,
                            ),
                        ),
                    ),
            )
            .into_any_element()
    }

    // Builds the Interface page: the scale and density of the workspace, and the composer.
    fn interface_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        page_column()
            .child(
                setting_group("Size and spacing", None, t)
                    .child(self.setting_row(
                        "Interface scale",
                        "Size of spacing, controls and icons. Text keeps the sizes of Typography.",
                        self.scale_slider(cx),
                    ))
                    .child(self.setting_row(
                        "Task density",
                        "Adjust the spacing between task rows.",
                        self.segmented(
                            "density",
                            [(false, "Compact"), (true, "Comfortable")],
                            &self.settings.comfortable,
                            |s, comfortable, cx| {
                                s.settings.comfortable = comfortable;
                                s.save_settings();
                                cx.notify();
                            },
                            cx,
                        ),
                    )),
            )
            .child(
                setting_group("Composer", None, t).child(
                    self.setting_row(
                        SKATER_LABEL,
                        SKATER_DETAIL,
                        controls::Switch::new("skater", self.settings.skater)
                            .label(SKATER_LABEL)
                            .on_click(cx.listener(|s, enabled: &bool, _, cx| {
                                s.settings.skater = *enabled;
                                s.save_settings();
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                ),
            )
            .into_any_element()
    }

    /// Returns the setting that chooses where a new task works: the current checkout or a
    /// worktree of its own.
    pub(super) fn task_location_setting(&self, cx: &mut Context<Self>) -> AnyElement {
        self.setting_row(
            "Default task location",
            "Use this for every new task. You can override it in the composer.",
            self.segmented(
                "default-task-location",
                [(false, "Current checkout"), (true, "New worktree")],
                &self.settings.default_worktree,
                |s, worktree, cx| {
                    s.settings.default_worktree = worktree;
                    s.new_worktree_override = None;
                    s.save_settings();
                    cx.notify();
                },
                cx,
            ),
        )
    }

    // Builds the Tasks page: where new tasks work, how the list sorts them and the sleep guard.
    fn task_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        page_column()
            .child(setting_group("New tasks", None, t).child(self.task_location_setting(cx)))
            .child(setting_group("Task list", None, t).child(self.setting_row(
                "Task order",
                "Sort by the last update, the name, what needs you, or the creation time.",
                self.segmented(
                    "sort",
                    [
                        (Sort::Recent, "Updated"),
                        (Sort::Name, "Name"),
                        (Sort::Attention, "Attention"),
                        (Sort::Created, "Created"),
                    ],
                    &self.settings.sort,
                    |s, sort, cx| {
                        s.settings.sort = sort;
                        s.save_settings();
                        cx.notify();
                    },
                    cx,
                ),
            )))
            .child(
                setting_group("While agents work", None, t).child(
                    self.setting_row(
                        AWAKE_LABEL,
                        AWAKE_DETAIL,
                        controls::Switch::new("prevent-sleep", self.settings.prevent_sleep)
                            .label(AWAKE_LABEL)
                            .on_click(cx.listener(|s, enabled: &bool, _, cx| {
                                s.settings.prevent_sleep = *enabled;
                                s.save_settings();
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                ),
            )
            .into_any_element()
    }

    // Builds the Layout page: where file diffs open and the width of the sidebar.
    fn layout_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        page_column()
            .child(setting_group("Diffs", None, t).child(self.setting_row(
                "Open file diffs in",
                "Choose where a file opens when you select a change.",
                self.segmented(
                    "diff-placement",
                    [
                        (DiffPlacement::Conversation, "Conversation"),
                        (DiffPlacement::RightPanel, "Right panel"),
                    ],
                    &self.settings.diff_placement,
                    |s, placement, cx| {
                        s.settings.diff_placement = placement;
                        if s.reviewing_file {
                            s.show_pane(PanePage::Changes);
                        }
                        s.save_settings();
                        cx.notify();
                    },
                    cx,
                ),
            )))
            .child(
                setting_group("Sidebar", None, t).child(
                    self.setting_row(
                        "Sidebar width",
                        "You can also drag its right edge.",
                        self.slider_control(
                            controls::Slider::new(
                                "sidebar-width",
                                self.settings.sidebar_width,
                                220.,
                                420.,
                                10.,
                                cx.listener(|s, value: &f32, _, cx| {
                                    s.settings.sidebar_width = value.clamp(220., 420.);
                                    s.save_settings();
                                    cx.notify();
                                }),
                            )
                            .label("Sidebar width"),
                            format!("{} PT", self.settings.sidebar_width as u32),
                            t.text,
                        ),
                    ),
                ),
            )
            .into_any_element()
    }

    // Builds the Import page: the dialog that imports threads of other coding agents.
    fn import_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        page_column()
            .child(
                setting_group("Threads", None, self.theme).child(
                    self.setting_row(
                        "Threads from Claude Code, Codex and OpenCode",
                        "Import conversations that you had in those apps in the folders of your projects. Each one keeps its history and continues the same conversation.",
                        self.button("open-thread-import", "arrow-down-to-line", "Import threads…")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.open_thread_import();
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                ),
            )
            .into_any_element()
    }

    /// Returns the switch that turns all task notifications on or off. `detail` says where
    /// the user selects the events.
    pub(super) fn task_notifications_setting(
        &self,
        detail: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.notification_toggle(
            "notifications-enabled",
            ("Task notifications", detail),
            self.settings.notifications.enabled,
            false,
            |n| &mut n.enabled,
            cx,
        )
    }

    // Builds the Events page: the switch of all task notifications and one for each event.
    fn event_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        // An event switch has no effect while task notifications are off as a whole.
        let muted = !self.settings.notifications.enabled;
        page_column()
            .child(
                setting_group("Task events", None, t)
                    .child(self.task_notifications_setting(
                        &format!("Send {NOTIFIER} notifications for the events selected below."),
                        cx,
                    ))
                    .child(self.notification_toggle(
                        "notifications-attention",
                        ("Needs attention", "When a task needs input, approval, or delivery reconciliation."),
                        self.settings.notifications.attention,
                        muted,
                        |n| &mut n.attention,
                        cx,
                    ))
                    .child(self.notification_toggle(
                        "notifications-completed",
                        ("Finished", "When a task finishes its turn."),
                        self.settings.notifications.completed,
                        muted,
                        |n| &mut n.completed,
                        cx,
                    ))
                    .child(self.notification_toggle(
                        "notifications-failed",
                        ("Failed", "When a task or its submission fails."),
                        self.settings.notifications.failed,
                        muted,
                        |n| &mut n.failed,
                        cx,
                    ))
                    .child(self.notification_toggle(
                        "notifications-interrupted",
                        ("Interrupted", "When a task stops before finishing."),
                        self.settings.notifications.interrupted,
                        muted,
                        |n| &mut n.interrupted,
                        cx,
                    ))
                    // Only macOS asks Kodem how to present an alert while it is active.
                    .when(cfg!(target_os = "macos"), |group| {
                        group.child(self.notification_toggle(
                            "notifications-while-focused",
                            ("Show while Kodem is active", "Show the banner and play the sound when Kodem is the frontmost app. When off, these alerts go to Notification Center only."),
                            self.settings.notifications.while_focused,
                            muted,
                            |n| &mut n.while_focused,
                            cx,
                        ))
                    }),
            )
            .child(note(NOTIFICATION_CONTROL, t))
            .into_any_element()
    }

    // Builds the Sound page: the sound of a notification, its preview and a test notification
    // with the state of its delivery.
    fn sound_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let delivery = self.workspace.as_ref().map(|w| &w.notification_delivery);
        let pending = matches!(delivery, Some(NotificationDelivery::Pending));
        let message = match delivery {
            Some(NotificationDelivery::Pending) => Some(format!(
                "Waiting for {NOTIFIER}. Allow notifications if prompted."
            )),
            Some(NotificationDelivery::Submitted) => {
                Some(format!("Notification submitted to {NOTIFIER}."))
            }
            Some(NotificationDelivery::Failed(error)) => Some(error.clone()),
            _ => None,
        };
        let failed = matches!(delivery, Some(NotificationDelivery::Failed(_)));
        page_column()
            .child(
                setting_group("Notification sound", None, t)
                    .child(self.notification_toggle(
                        "notifications-sound",
                        ("Play sound", "Use the selected sound with task notifications."),
                        self.settings.notifications.play_sound,
                        false,
                        |n| &mut n.play_sound,
                        cx,
                    ))
                    .child(self.setting_row(
                        "Notification sound",
                        "The sound that plays with a task notification. Choose a sound to hear it.",
                        self.segmented(
                            "notification-sound",
                            NotificationSound::ALL.map(|sound| (sound, sound.label())),
                            &self.settings.notifications.sound,
                            |s, sound, cx| {
                                s.settings.notifications.sound = sound;
                                s.save_settings();
                                s.preview_sound(sound, cx);
                                cx.notify();
                            },
                            cx,
                        ),
                    ))
                    .child(
                        self.setting_row(
                            "Preview sound",
                            "Play the selected sound now, without sending a notification.",
                            self.button("preview-notification-sound", "play", "Play")
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.preview_sound(s.settings.notifications.sound, cx);
                                }))
                                .into_any_element(),
                        ),
                    ),
            )
            .child(
                setting_group("Test", None, t).child(
                        self.setting_row(
                            "Test notification",
                            "Send a preview using the selected sound settings, even if task notifications are off. It shows while Kodem is active.",
                            self.button(
                                "test-notification",
                                "bell",
                                if pending {
                                    "Sending…"
                                } else {
                                    "Test notification"
                                },
                            )
                            .disabled(pending)
                            .on_click(cx.listener(|s, _, _, cx| {
                                if let Some(workspace) = &mut s.workspace {
                                    workspace.notification_delivery = NotificationDelivery::Pending;
                                }
                                s.send(core::Request::TestNotification {
                                    sound: s
                                        .settings
                                        .notifications
                                        .play_sound
                                        .then_some(s.settings.notifications.sound),
                                });
                                cx.notify();
                            }))
                            .into_any_element(),
                        ),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(note(NOTIFICATION_CONTROL, t))
                    .when_some(message, |d, message: String| {
                        d.child(note(message, t).when(failed, |d| d.text_color(t.removed)))
                    }),
            )
            .into_any_element()
    }

    // Builds the Shortcuts page: the key chords of the workspace in groups.
    fn shortcut_settings(&self) -> AnyElement {
        let t = self.theme;
        let mut page = page_column();
        for (group, entries) in SHORTCUTS {
            let mut shortcuts = setting_group(group, None, t).child(div().h(px(4.)));
            for (label, key) in entries {
                shortcuts = shortcuts.child(
                    div()
                        .py(px(7.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(div().flex_shrink_0().text_size(text_px(13.)).child(*label))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(12.))
                                .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                        )
                        .child(
                            div().flex().flex_shrink_0().gap(px(4.)).children(
                                key.split(" / ")
                                    .map(|chord| kit::keycap(chord.to_owned(), t)),
                            ),
                        ),
                );
            }
            page = page.child(shortcuts);
        }
        page.into_any_element()
    }

    // Plays a notification sound in this process, so no notification is sent.
    fn preview_sound(&mut self, sound: NotificationSound, cx: &mut Context<Self>) {
        if let Err(error) = crate::sound::preview(sound) {
            self.report_error(format!("Could not preview the sound: {error:#}"), cx);
        }
    }

    fn notification_toggle(
        &self,
        id: &'static str,
        (label, detail): (&str, &str),
        enabled: bool,
        disabled: bool,
        field: fn(&mut NotificationSettings) -> &mut bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label,
            detail,
            controls::Switch::new(id, enabled)
                .label(label.to_owned())
                .disabled(disabled)
                .on_click(cx.listener(move |s, enabled: &bool, _, cx| {
                    *field(&mut s.settings.notifications) = *enabled;
                    s.save_settings();
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// Returns a row that can be chosen. A chosen row carries the accent tint, faint dots along
    /// its bottom edge and the accent edge. Add its icon and label as children.
    pub(super) fn choice_row(&self, id: impl Into<ElementId>, chosen: bool) -> Button {
        let t = self.theme;
        self.control(id)
            .h(px(30.))
            .px(px(10.))
            .gap(px(8.))
            .justify_start()
            .overflow_hidden()
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .text_color(if chosen { t.text } else { t.muted })
            .aria_selected(chosen)
            .when(chosen, |row| row.layer(chosen_layer(t.accent)))
            .when(!chosen, |row| {
                row.hover(move |row| row.bg(t.selected).text_color(t.text))
            })
            .on_press(move |position, cx| fx::ripple(position, t.muted, cx))
    }

    /// Returns one segmented control: a bordered group with a cell for each option. The chosen
    /// cell carries the accent tint, the dithered wash and the accent edge. `choose` receives
    /// the option of the cell that the user selects.
    pub(super) fn segmented<T, L>(
        &self,
        id: &'static str,
        options: impl IntoIterator<Item = (T, L)>,
        current: &T,
        choose: impl Fn(&mut Self, T, &mut Context<Self>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement
    where
        T: Clone + PartialEq + 'static,
        L: Into<SharedString>,
    {
        let t = self.theme;
        let mut group = div()
            .flex()
            .flex_wrap()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .border_1()
            .border_color(t.edge)
            .bg(t.inset);
        for (index, (value, label)) in options.into_iter().enumerate() {
            let chosen = value == *current;
            let choose = choose.clone();
            group = group.child(
                self.control((id, index))
                    .h(px(22.))
                    .px(px(10.))
                    .rounded(px(crate::theme::RADIUS_SMALL))
                    .overflow_hidden()
                    .border_color(gpui::transparent_black())
                    .bg(gpui::transparent_black())
                    .text_color(if chosen { t.text } else { t.muted })
                    .aria_selected(chosen)
                    .when(chosen, |cell| {
                        cell.cursor_default().layer(chosen_layer(t.accent))
                    })
                    .child(label.into())
                    .when(!chosen, |cell| {
                        cell.hover(move |cell| cell.bg(t.selected).text_color(t.text))
                            .on_press(move |position, cx| fx::ripple(position, t.muted, cx))
                            .on_click(cx.listener(move |s, _, _, cx| choose(s, value.clone(), cx)))
                    }),
            );
        }
        group.into_any_element()
    }

    // Lays out a slider beside the mono readout of its value, in a column of fixed width.
    fn slider_control(&self, slider: controls::Slider, readout: String, tone: Hsla) -> AnyElement {
        div()
            .w(px(SLIDER_COLUMN))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().flex_1().min_w_0().child(slider))
            .child(
                div()
                    .w(px(44.))
                    .flex()
                    .justify_end()
                    .child(figure(readout, tone)),
            )
            .into_any_element()
    }

    /// Returns one setting: its label over its description, beside its control and above a
    /// dotted rule.
    pub(super) fn setting_row(&self, label: &str, detail: &str, control: AnyElement) -> AnyElement {
        setting_row(label, detail, control, self.theme)
    }

    // Builds the Git section: its open page. The navigation chooses the page.
    fn git_settings(&self) -> AnyElement {
        match self.git_settings.clone() {
            Some(view) => view.into_any_element(),
            None => div().into_any_element(),
        }
    }

    /// Reports whether any account of the provider is signed in; unknown until one was checked.
    pub(super) fn provider_signed_in(&self, provider: Provider) -> Option<bool> {
        let workspace = self.workspace.as_ref()?;
        workspace
            .accounts
            .iter()
            .filter(|state| state.account.provider == provider)
            .filter_map(|state| state.status.signed_in)
            .reduce(|any, signed_in| any || signed_in)
    }

    // Builds the General page of Agents: what applies to every provider.
    fn agent_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut upcoming = div().pt(px(8.)).flex().gap_2();
        for (index, candidate) in UpcomingProvider::ALL.into_iter().enumerate() {
            upcoming = upcoming.child(
                self.choice_row(("settings-provider-soon", index), false)
                    .disabled(true)
                    .flex_1()
                    .h(px(36.))
                    .px(px(12.))
                    .border_color(t.border)
                    .child(icon(candidate.icon).size(px(13.)))
                    .child(candidate.label)
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint {
                            text: candidate.hint(),
                            theme: t,
                        })
                        .into()
                    }),
            );
        }
        page_column()
            .child(
                setting_group("Messages", None, t).child(self.setting_row(
                    "Messages during a turn",
                    "Queue waits for completion. Steer updates the current turn. A provider that cannot steer queues every message.",
                    self.segmented(
                        "provider-send-mode",
                        [(SendMode::Queue, "Queue"), (SendMode::Steer, "Steer")],
                        &self.settings.send_mode,
                        |s, mode, cx| {
                            s.settings.send_mode = mode;
                            s.save_settings();
                            cx.notify();
                        },
                        cx,
                    ),
                )),
            )
            .child(self.shared_compaction(cx))
            .child(
                setting_group("Background tasks", None, t)
                    .child(
                        note(
                            "Tasks and terminals continue when you close the window. An idle host stops after five minutes when no task or shell is running.",
                            t,
                        )
                        .pt(px(8.)),
                    )
                    .child(
                        div().pt(px(8.)).flex().child(
                            self.ghost("shutdown-host", "stop", "Stop idle host and quit")
                                .on_click(
                                    cx.listener(|s, _, _, _| s.send(core::Request::Shutdown)),
                                ),
                        ),
                    ),
            )
            .child(setting_group("Coming soon", Some(UpcomingProvider::ALL.len()), t).child(upcoming))
            .into_any_element()
    }

    // Builds one page of a provider.
    fn provider_page(
        &self,
        provider: Provider,
        page: ProviderPage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match page {
            ProviderPage::Connection => self.connection_settings(provider, cx),
            ProviderPage::Accounts => self.account_settings(provider, cx),
            ProviderPage::Model => self.model_settings(provider, cx),
            ProviderPage::Access => self.access_settings(provider, cx),
            ProviderPage::Compaction => {
                let runtime = self.provider_runtime(provider);
                page_column()
                    .child(self.compaction_settings(provider, &runtime.models, cx))
                    .into_any_element()
            }
            ProviderPage::Features => page_column()
                .child(self.claude_feature_settings(cx))
                .into_any_element(),
        }
    }

    // Builds the Connection page of a provider: its CLI's runtime, its accounts in short and
    // the choice of the provider for new conversations.
    fn connection_settings(&self, provider: Provider, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let runtime = self.provider_runtime(provider);
        let connected = self.provider_signed_in(provider) == Some(true);
        let (accounts, signed_in) = self.workspace.as_ref().map_or((0, 0), |workspace| {
            workspace
                .accounts
                .iter()
                .filter(|state| state.account.provider == provider)
                .fold((0, 0), |(accounts, signed_in), state| {
                    (
                        accounts + 1,
                        signed_in + usize::from(state.status.signed_in == Some(true)),
                    )
                })
        });
        let accounts_summary = format!(
            "{accounts} {} · {signed_in} signed in. Sign-in stays with the provider's CLI.",
            if accounts == 1 { "account" } else { "accounts" }
        );
        let identity = kit::card(t)
            .p(px(12.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(kit::icon_tile(provider.icon(), t.accent, 30., false))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(text_px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(provider.label())
                            .when(provider.beta(), |name| name.child(kit::beta_chip(t))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(kit::marker(if connected { t.added } else { t.subtle }))
                            .child(caps(if connected { "Signed in" } else { "Offline" }, t)),
                    ),
            )
            .child(
                div().relative().child(
                    self.ghost("provider-refresh", "refresh", "Refresh")
                        .on_click(cx.listener(move |s, _, _, _| {
                            s.send(core::Request::ConnectRuntime {
                                provider,
                                account: None,
                            })
                        })),
                ),
            );
        let runtime_group = setting_group("Runtime", None, t)
            .child(
                div().py(px(10.)).child(kit::leader(
                    "Version",
                    runtime
                        .version
                        .clone()
                        .unwrap_or_else(|| "Not connected".into()),
                    if runtime.version.is_some() {
                        t.text
                    } else {
                        t.warning
                    },
                    t,
                )),
            )
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .child(
                self.setting_row(
                    "New conversations",
                    &format!("Start new conversations with {}.", provider.label()),
                    self.button(
                        "provider-use-default",
                        provider.icon(),
                        "Use for new conversations",
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        let preferences = s.provider_preferences(provider);
                        s.save_agent_preferences(preferences);
                        cx.notify();
                    }))
                    .into_any_element(),
                ),
            )
            .when_some(
                install_guide(provider).filter(|_| runtime.version.is_none()),
                |group, (label, url)| {
                    group.child(
                        div().pt(px(10.)).flex().child(
                            self.ghost("provider-install", "external-link", label)
                                .on_click(cx.listener(move |_, _, _, cx| cx.open_url(url))),
                        ),
                    )
                },
            );
        page_column()
            .child(identity)
            .when_some(runtime.error, |d, error| {
                d.child(error_block("Runtime error", error, t))
            })
            .child(runtime_group)
            .child(
                setting_group("Accounts", Some(accounts), t).child(
                    self.setting_row(
                        "Accounts",
                        &accounts_summary,
                        self.button("provider-accounts", "users", "Manage accounts")
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.open_settings(SettingsPage::Provider(
                                    provider,
                                    ProviderPage::Accounts,
                                ));
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                ),
            )
            .into_any_element()
    }

    // Builds the Model page of a provider: the model and reasoning that new conversations
    // start with.
    fn model_settings(&self, provider: Provider, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let runtime = self.provider_runtime(provider);
        let preferences = self.provider_preferences(provider);
        let mut models = div().pt(px(6.)).flex().flex_col().gap(px(1.));
        for (index, model) in runtime.models.iter().enumerate() {
            let mut selected = preferences.clone();
            selected.model = Some(model.id.clone());
            selected.effort =
                (!model.default_effort.is_empty()).then(|| model.default_effort.clone());
            // The approval rule follows the model.
            let selected = self.with_access(selected);
            let chosen = preferences.model.as_ref() == Some(&model.id);
            models = models.child(
                self.choice_row(("settings-model", index), chosen)
                    .w_full()
                    .font_family(crate::theme::sans())
                    .text_size(text_px(12.5))
                    .font_weight(FontWeight::NORMAL)
                    .child(icon(provider.icon()).size(px(13.)))
                    .child(model.name.clone())
                    .child(div().flex_1())
                    .when(chosen, |row| {
                        row.child(icon("check").size(px(13.)).text_color(t.accent))
                    })
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.save_agent_preferences(selected.clone());
                        cx.notify();
                    })),
            );
        }
        let mut defaults =
            setting_group("Default model", Some(runtime.models.len()), t).child(models);
        if let Some(model) = runtime
            .models
            .iter()
            .find(|model| preferences.model.as_ref() == Some(&model.id))
        {
            if !model.efforts.is_empty() {
                let current = preferences
                    .effort
                    .clone()
                    .unwrap_or_else(|| model.default_effort.clone());
                let base = preferences.clone();
                defaults = defaults.child(
                    self.setting_row(
                        "Reasoning",
                        "Levels reported by the selected model",
                        self.segmented(
                            "settings-reasoning",
                            model
                                .efforts
                                .iter()
                                .map(|effort| (effort.clone(), effort.clone())),
                            &current,
                            move |s, effort, cx| {
                                let mut selected = base.clone();
                                selected.effort = Some(effort);
                                s.save_agent_preferences(selected);
                                cx.notify();
                            },
                            cx,
                        ),
                    ),
                );
            }
            defaults = defaults.child(
                note(
                    if model.images {
                        "Supports image attachments"
                    } else {
                        "Image attachments are unavailable for this model"
                    },
                    t,
                )
                .pt(px(8.)),
            );
        }
        page_column()
            .when_some(runtime.error, |d, error| {
                d.child(error_block("Runtime error", error, t))
            })
            .child(defaults.when(runtime.models.is_empty(), |group| {
                group.child(
                    note(
                        format!("Connect {} to list its models.", provider.label()),
                        t,
                    )
                    .pt(px(8.)),
                )
            }))
            .into_any_element()
    }

    // Builds the Access page of a provider: the approval rule that new conversations start
    // with.
    fn access_settings(&self, provider: Provider, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let runtime = self.provider_runtime(provider);
        let preferences = self.provider_preferences(provider);
        let mut access = div().pt(px(6.)).flex().flex_col().gap(px(1.));
        if provider == Provider::Codex {
            for (index, mode) in [
                AccessMode::ReadOnly,
                AccessMode::Workspace,
                AccessMode::AutoReview,
                AccessMode::FullAccess,
            ]
            .into_iter()
            .enumerate()
            {
                let mut selected = preferences.clone();
                selected.access = mode;
                let chosen = preferences.access == mode;
                access = access.child(
                    self.choice_row(("settings-access", index), chosen)
                        .w_full()
                        .child(icon("shield").size(px(13.)))
                        .child(mode.label())
                        .child(div().flex_1())
                        .when(chosen, |row| {
                            row.child(icon("check").size(px(13.)).text_color(t.accent))
                        })
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.save_access_choice(selected.clone());
                            cx.notify();
                        })),
                );
            }
        } else {
            for (index, mode) in runtime.access_modes.iter().enumerate() {
                let mut selected = preferences.clone();
                selected.provider_access = Some(mode.id.clone());
                let chosen = preferences.provider_access.as_ref() == Some(&mode.id);
                access = access.child(
                    self.choice_row(("settings-access", index), chosen)
                        .w_full()
                        .h_auto()
                        .py(px(8.))
                        .child(icon("shield").size(px(13.)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(mode.name.clone())
                                .child(
                                    div()
                                        .font_family(crate::theme::sans())
                                        .font_weight(FontWeight::NORMAL)
                                        .text_size(text_px(12.))
                                        .line_height(relative(1.4))
                                        .text_color(t.muted)
                                        .child(mode.description.clone()),
                                ),
                        )
                        .when(chosen, |row| {
                            row.child(icon("check").size(px(13.)).text_color(t.accent))
                        })
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.save_access_choice(selected.clone());
                            cx.notify();
                        })),
                );
            }
        }
        page_column()
            .child(setting_group("Default access", None, t).child(access))
            .into_any_element()
    }

    /// Builds the switches for the Claude Code features that the CLI keeps off in a session
    /// that an application starts.
    pub(super) fn claude_feature_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        setting_group("Claude Code features", None, self.theme)
            .child(
                self.setting_row(
                    ARTIFACTS_LABEL,
                    ARTIFACTS_DETAIL,
                    controls::Switch::new("claude-artifacts", self.settings.claude_artifacts)
                        .label(ARTIFACTS_LABEL)
                        .on_click(cx.listener(|s, enabled: &bool, _, cx| {
                            s.settings.claude_artifacts = *enabled;
                            // The CLI lists other commands now, so the next completion reads them again.
                            s.slash = Default::default();
                            s.save_settings();
                            cx.notify();
                        }))
                        .into_any_element(),
                ),
            )
            .into_any_element()
    }

    // Changes one stored compaction rule and saves it. A scope without its own rule is left
    // alone, so a control that outlives its override changes nothing.
    fn edit_compact_rule(
        &mut self,
        scope: &CompactScope,
        edit: impl FnOnce(&mut CompactRule),
        cx: &mut Context<Self>,
    ) {
        let Some(rule) = scope.stored(&mut self.settings.auto_compact) else {
            return;
        };
        edit(rule);
        *rule = rule.bounded();
        self.save_settings();
        cx.notify();
    }

    // Gives a provider or a model its own rule, which starts as the rule in effect for it, or
    // removes that rule so that the scope follows the level above again.
    fn set_compact_override(&mut self, scope: &CompactScope, own: bool, cx: &mut Context<Self>) {
        let rules = &mut self.settings.auto_compact;
        match scope {
            CompactScope::All => return,
            CompactScope::Provider(provider) => {
                if own {
                    let rule = rules.rule(*provider, None);
                    rules.providers.entry(*provider).or_insert(rule);
                } else {
                    rules.providers.remove(provider);
                }
            }
            CompactScope::Model(provider, model) => {
                let key = AutoCompact::model_key(*provider, model);
                if own {
                    let rule = rules.rule(*provider, Some(model));
                    rules.models.entry(key).or_insert(rule);
                } else {
                    rules.models.remove(&key);
                }
            }
        }
        self.save_settings();
        cx.notify();
    }

    // The switch that gives a provider or a model its own compaction rule.
    fn compact_override_switch(
        &self,
        scope: CompactScope,
        own: bool,
        label: String,
        cx: &mut Context<Self>,
    ) -> controls::Switch {
        controls::Switch::new(format!("compact-own:{}", scope.key()), own)
            .label(label)
            .on_click(cx.listener(move |s, own: &bool, _, cx| {
                s.set_compact_override(&scope, *own, cx);
            }))
    }

    // The switch that turns automatic compaction on or off in one stored rule.
    fn compact_switch(
        &self,
        scope: CompactScope,
        rule: CompactRule,
        label: String,
        cx: &mut Context<Self>,
    ) -> controls::Switch {
        controls::Switch::new(format!("compact-enabled:{}", scope.key()), rule.enabled)
            .label(label)
            .on_click(cx.listener(move |s, enabled: &bool, _, cx| {
                let enabled = *enabled;
                s.edit_compact_rule(&scope, |rule| rule.enabled = enabled, cx);
            }))
    }

    // The threshold slider of one stored rule with its percent readout. It rests while the
    // rule is off.
    fn compact_slider(
        &self,
        scope: CompactScope,
        rule: CompactRule,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let slider = controls::Slider::new(
            format!("compact-threshold:{}", scope.key()),
            f32::from(rule.threshold),
            f32::from(CompactRule::MIN_THRESHOLD),
            f32::from(CompactRule::MAX_THRESHOLD),
            THRESHOLD_STEP,
            cx.listener(move |s, value: &f32, _, cx| {
                let threshold = value.round() as u8;
                s.edit_compact_rule(&scope, |rule| rule.threshold = threshold, cx);
            }),
        )
        .label(label)
        .disabled(!rule.enabled);
        self.slider_control(
            slider,
            format!("{}%", rule.threshold),
            if rule.enabled { t.text } else { t.subtle },
        )
    }

    // Builds the compaction rule for every provider and model that has no rule of its own.
    fn shared_compaction(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let shared = self.settings.auto_compact.default.bounded();
        setting_group("Context compaction", None, t)
            .child(
                note(
                    "When a task's context reaches this share of the model's window, Kodem asks the provider to summarize the conversation. Each provider and model can set a rule of its own on its Compaction page.",
                    t,
                )
                .pt(px(8.)),
            )
            .child(compact_level("All providers", shared, true, t))
            .child(self.setting_row(
                "Compact automatically",
                "The rule for every provider and model that has no rule of its own.",
                self.compact_switch(
                    CompactScope::All,
                    shared,
                    "Compact automatically".to_owned(),
                    cx,
                )
                .into_any_element(),
            ))
            .child(self.setting_row(
                "Threshold",
                "The share of the context window that starts compaction.",
                self.compact_slider(
                    CompactScope::All,
                    shared,
                    "Compaction threshold".to_owned(),
                    cx,
                ),
            ))
            .into_any_element()
    }

    // Builds the automatic compaction rules of one provider: its own rule, and one row for
    // each of its models.
    fn compaction_settings(
        &self,
        provider: Provider,
        models: &[Model],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let rules = &self.settings.auto_compact;
        let own = rules.providers.get(&provider).map(|rule| rule.bounded());
        let name = provider.label();
        let mut group = setting_group("Rules", None, t)
            .child(
                note(
                    format!("Without a rule of its own, {name} follows the rule for all providers in Agents › General."),
                    t,
                )
                .pt(px(8.)),
            )
            .child(compact_level(
                name,
                rules.rule(provider, None),
                own.is_some(),
                t,
            ))
            .child(self.setting_row(
                "Use its own rule",
                &format!("When this is off, {name} follows the rule for all providers."),
                self.compact_override_switch(
                    CompactScope::Provider(provider),
                    own.is_some(),
                    format!("Use its own rule for {name}"),
                    cx,
                )
                .into_any_element(),
            ));
        if let Some(rule) = own {
            group = group
                .child(
                    self.setting_row(
                        "Compact automatically",
                        &format!("Applies to every {name} model that has no custom rule."),
                        self.compact_switch(
                            CompactScope::Provider(provider),
                            rule,
                            format!("Compact automatically for {name}"),
                            cx,
                        )
                        .into_any_element(),
                    ),
                )
                .child(self.setting_row(
                    "Threshold",
                    &format!("The share of the context window for {name}."),
                    self.compact_slider(
                        CompactScope::Provider(provider),
                        rule,
                        format!("Compaction threshold for {name}"),
                        cx,
                    ),
                ));
        }
        group = group.child(div().pt(px(14.)).pb(px(2.)).child(kit::section_header(
            "Per model",
            Some(models.len()),
            t,
        )));
        if models.is_empty() {
            group = group.child(
                note(
                    format!("Connect {name} to set a rule for each of its models."),
                    t,
                )
                .pt(px(6.)),
            );
        }
        for model in models {
            let scope = CompactScope::Model(provider, model.id.clone());
            let custom = rules
                .models
                .get(&AutoCompact::model_key(provider, &model.id))
                .map(|rule| rule.bounded());
            let effective = rules.rule(provider, Some(&model.id));
            let mut row = div().py(px(8.)).flex().flex_col().gap(px(8.)).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(text_px(12.5))
                            .text_ellipsis()
                            .child(model.name.clone()),
                    )
                    .child(kit::chip(
                        rule_label(effective),
                        rule_tone(effective, custom.is_some(), t),
                    ))
                    .child(caps("Custom", t))
                    .child(self.compact_override_switch(
                        scope.clone(),
                        custom.is_some(),
                        format!("Custom rule for {}", model.name),
                        cx,
                    )),
            );
            if let Some(rule) = custom {
                row = row.child(
                    div()
                        .pl(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(self.compact_switch(
                            scope.clone(),
                            rule,
                            format!("Compact automatically for {}", model.name),
                            cx,
                        ))
                        .child(caps("Compact automatically", t))
                        .child(div().flex_1())
                        .child(self.compact_slider(
                            scope,
                            rule,
                            format!("Compaction threshold for {}", model.name),
                            cx,
                        )),
                );
            }
            group = group
                .child(row)
                .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)));
        }
        group.into_any_element()
    }
}

/// Returns where to get a provider's CLI when Kodem cannot find it.
pub(super) fn install_guide(provider: Provider) -> Option<(&'static str, &'static str)> {
    match provider {
        Provider::Claude => Some((
            "Claude Code installation guide",
            "https://code.claude.com/docs/en/overview",
        )),
        Provider::Codex => None,
        Provider::OpenCode => Some(("OpenCode installation guide", "https://opencode.ai/docs/")),
    }
}
