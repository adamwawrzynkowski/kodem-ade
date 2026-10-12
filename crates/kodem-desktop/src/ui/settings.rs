use super::*;

/// Index of the provider section.
pub(super) const PROVIDERS: usize = 2;
/// Index of the projects section.
pub(super) const PROJECTS: usize = 5;
/// Index of the accounts section.
pub(super) const ACCOUNTS: usize = 6;
/// Preference sections in navigation order: stable index, icon and label.
pub(super) const SECTIONS: [(usize, &str, &str); 7] = [
    (0, "sun", "Appearance"),
    (1, "folder", "Workspace"),
    (PROVIDERS, "settings-2", "Providers"),
    (ACCOUNTS, "users", "Accounts"),
    (PROJECTS, "layers", "Projects"),
    (3, "bell", "Notifications"),
    (4, "keyboard", "Shortcuts"),
];

/// Space between the groups of one preferences section.
const GROUP_GAP: f32 = 18.;
/// Width of the control column that holds a slider and its readout.
const SLIDER_COLUMN: f32 = 220.;
/// Widest control column of a settings row. A wider segmented control wraps.
const CONTROL_COLUMN: f32 = 340.;
/// Step of a compaction threshold slider, in percent.
const THRESHOLD_STEP: f32 = 5.;

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

/// The look of a chosen surface as one layer: the accent tint, dots that fade from its leading
/// edge and the accent edge. Add it to a relatively positioned surface before its content.
pub(super) fn chosen_layer(tone: Hsla, radius: f32) -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded(px(radius))
        .bg(tone.opacity(0.1))
        .child(dither::row_wash(tone, radius, true, false))
        .child(kit::selection_edge(tone))
}

/// The frame of a text field in a dialog or a form: an inset surface with the focus edge. Add
/// the input as a child that fills the width.
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
                .text_size(px(12.))
                .line_height(relative(1.45))
                .text_color(theme.text)
                .child(message.into()),
        )
}

// Starts a group of settings: a section header that its rows follow without a gap.
fn setting_group(label: &str, count: Option<usize>, theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .child(kit::section_header(label, count, theme))
}

// Returns a short explanation in prose below a group header or between rows.
fn note(text: impl Into<SharedString>, theme: Theme) -> Div {
    div()
        .text_size(px(12.))
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

impl Shell {
    /// Builds native preferences with section navigation and labeled controls.
    pub(super) fn settings_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut navigation = div()
            .w(px(180.))
            .h_full()
            .flex_shrink_0()
            .p(px(10.))
            .bg(t.sidebar)
            .flex()
            .flex_col()
            .gap(px(2.))
            .border_r_1()
            .border_color(t.border)
            .child(caps("Preferences", t).px(px(8.)).pt(px(4.)).pb(px(8.)));
        for (i, glyph, label) in SECTIONS {
            navigation = navigation.child(
                self.choice_row(("settings-section", i), self.settings_section == i)
                    .child(icon(glyph).size(px(13.)))
                    .child(label)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.open_settings(i);
                        cx.notify();
                    })),
            );
        }
        let section = match self.settings_section {
            PROJECTS => self.project_settings(cx),
            ACCOUNTS => self.account_settings(cx),
            0 => self.appearance_settings(cx),
            1 => self.workspace_settings(cx),
            PROVIDERS => self.provider_settings(cx),
            3 => self.notification_settings(cx),
            _ => self.shortcut_settings(),
        };
        div()
            .h(px(520.))
            .flex()
            .child(navigation)
            .child(
                div()
                    .id("preferences-content")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .px(px(28.))
                    .py(px(24.))
                    .child(section),
            )
            .into_any_element()
    }

    // Saves the dark or light appearance and applies it at once.
    fn set_light(&mut self, light: bool, cx: &mut Context<Self>) {
        self.settings.light = light;
        self.theme = Theme::with_accent(light, self.settings.accent);
        self.save_settings();
        cx.notify();
    }

    fn appearance_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut themes = div().pt(px(10.)).flex().gap(px(12.));
        for (i, light) in [false, true].into_iter().enumerate() {
            let preview = Theme::with_accent(light, self.settings.accent);
            let chosen = self.settings.light == light;
            themes = themes.child(
                div()
                    .id(("theme-preview", i))
                    .flex_1()
                    .h(px(96.))
                    .p(px(6.))
                    .flex()
                    .gap(px(6.))
                    .bg(preview.canvas)
                    .rounded(px(crate::theme::RADIUS_SURFACE))
                    .border_1()
                    .border_color(if chosen { t.text } else { t.border })
                    .when(!chosen, |d| d.hover(move |d| d.border_color(t.edge)))
                    .cursor_pointer()
                    .child(
                        div()
                            .w(px(34.))
                            .rounded(px(2.))
                            .bg(preview.sidebar)
                            .p(px(4.))
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(div().h(px(3.)).w(px(14.)).bg(preview.text))
                            .child(
                                div()
                                    .h(px(8.))
                                    .w_full()
                                    .rounded(px(1.))
                                    .bg(preview.selected),
                            )
                            .child(div().h(px(3.)).w(px(20.)).bg(preview.subtle))
                            .child(div().h(px(3.)).w(px(16.)).bg(preview.subtle)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .pt_2()
                            .child(div().h(px(4.)).w(px(48.)).bg(preview.muted))
                            .child(div().h(px(3.)).w_full().bg(preview.border))
                            .child(div().h(px(3.)).w(relative(0.7)).bg(preview.border))
                            .child(div().flex_1())
                            .child(
                                div()
                                    .h(px(22.))
                                    .px(px(4.))
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .bg(preview.raised)
                                    .border_1()
                                    .border_color(preview.edge)
                                    .rounded(px(3.))
                                    .child(div().size(px(10.)).rounded(px(2.)).bg(preview.accent)),
                            ),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| s.set_light(light, cx))),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(GROUP_GAP))
            .child(self.section_title(
                "Appearance",
                "Set the workspace's contrast and reading size.",
            ))
            .child(
                setting_group("Theme", None, t)
                    .child(themes)
                    .child(self.setting_row(
                        "Appearance",
                        "Graphite in the dark appearance, paper in the light one.",
                        self.segmented(
                            "theme-choice",
                            [(false, "Dark"), (true, "Light")],
                            &self.settings.light,
                            Self::set_light,
                            cx,
                        ),
                    )),
            )
            .child(self.accent_choices(cx))
            .child(
                setting_group("Reading", None, t)
                    .child(
                        self.setting_row(
                            "Conversation text",
                            "Size of messages and prompt text.",
                            self.slider_control(
                                controls::Slider::new(
                                    "font-size",
                                    self.settings.font_size,
                                    12.,
                                    22.,
                                    1.,
                                    cx.listener(|s, value: &f32, _, cx| {
                                        s.settings.font_size = value.clamp(12., 22.);
                                        s.save_settings();
                                        cx.notify();
                                    }),
                                )
                                .label("Conversation text size"),
                                format!("{} PT", self.settings.font_size),
                                t.text,
                            ),
                        ),
                    )
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
            .into_any_element()
    }

    fn workspace_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(GROUP_GAP))
            .child(self.section_title("Workspace", "Choose how projects and tasks are arranged."))
            .child(
                setting_group("Tasks", None, t)
                    .child(self.setting_row(
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
                    ))
                    .child(self.setting_row(
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
                    )),
            )
            .child(
                setting_group("Layout", None, t)
                    .child(self.setting_row(
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
                                    s.show_review = true;
                                }
                                s.save_settings();
                                cx.notify();
                            },
                            cx,
                        ),
                    ))
                    .child(
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

    fn notification_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let delivery = self.workspace.as_ref().map(|w| &w.notification_delivery);
        let pending = matches!(delivery, Some(NotificationDelivery::Pending));
        let message = match delivery {
            Some(NotificationDelivery::Pending) => {
                Some("Waiting for macOS. Allow notifications if prompted.".into())
            }
            Some(NotificationDelivery::Submitted) => {
                Some("Notification submitted to macOS.".into())
            }
            Some(NotificationDelivery::Failed(error)) => Some(error.clone()),
            _ => None,
        };
        let failed = matches!(delivery, Some(NotificationDelivery::Failed(_)));
        // An event switch has no effect while task notifications are off as a whole.
        let muted = !self.settings.notifications.enabled;
        div()
            .flex()
            .flex_col()
            .gap(px(GROUP_GAP))
            .child(self.section_title(
                "Notifications",
                "Get task updates, even after closing the window.",
            ))
            .child(
                setting_group("Events", None, t)
                    .child(self.notification_toggle(
                        "notifications-enabled",
                        ("Task notifications", "Send macOS notifications for the events selected below."),
                        self.settings.notifications.enabled,
                        false,
                        |n| &mut n.enabled,
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
                    )),
            )
            .child(
                setting_group("Sound", None, t)
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
                        "The sound that plays with a task notification.",
                        self.segmented(
                            "notification-sound",
                            NotificationSound::ALL.map(|sound| (sound, sound.label())),
                            &self.settings.notifications.sound,
                            |s, sound, cx| {
                                s.settings.notifications.sound = sound;
                                s.save_settings();
                                cx.notify();
                            },
                            cx,
                        ),
                    ))
                    .child(
                        self.setting_row(
                            "Test notification",
                            "Send a preview using the selected sound settings, even if task notifications are off.",
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
                    .child(note(
                        "macOS controls notification permission, banners, sounds, and Focus settings.",
                        t,
                    ))
                    .when_some(message, |d, message: String| {
                        d.child(note(message, t).when(failed, |d| d.text_color(t.removed)))
                    }),
            )
            .into_any_element()
    }

    fn shortcut_settings(&self) -> AnyElement {
        let t = self.theme;
        let mut shortcuts = setting_group("Shortcuts", None, t);
        for (label, key) in [
            ("Command palette", "⌘ K"),
            ("New task", "⌘ N"),
            ("Open repository", "⌘ O"),
            ("Search tasks", "⌘ F"),
            ("Next or previous task", "⌃ Tab / ⌃ ⇧ Tab / ⇧ ⌘ ] / ⇧ ⌘ ["),
            ("Go to a task in the feed", "⌘ 1 – ⌘ 9"),
            ("Next task that needs you", "⌥ ⌘ A"),
            ("Send message", "Return"),
            ("Send the other way during a turn", "⌘ Return"),
            ("New line in prompt", "⇧ Return"),
            ("Provider, account and model", "⇧ ⌘ M"),
            ("Reasoning effort", "⇧ ⌘ E"),
            ("Access mode", "⇧ ⌘ A"),
            ("Switch build and plan, or cycle Codex access", "⇧ Tab"),
            ("Copy diff selection", "⌘ C"),
            ("Toggle sidebar", "⌘ B"),
            ("Toggle changes", "⌘ D"),
            ("Toggle terminal", "⌃ `"),
            ("Settings", "⌘ ,"),
            ("Close dialog or menu", "Esc"),
        ] {
            shortcuts = shortcuts.child(
                div()
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().flex_shrink_0().text_size(px(13.)).child(label))
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
        div()
            .flex()
            .flex_col()
            .gap(px(GROUP_GAP))
            .child(self.section_title(
                "Keyboard shortcuts",
                "Move around the workspace without leaving the keyboard.",
            ))
            .child(shortcuts)
            .into_any_element()
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

    /// Returns a settings section heading: its title before a dotted rule, over its description.
    pub(super) fn section_title(&self, title: &str, detail: &str) -> AnyElement {
        let t = self.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(16.))
                            .line_height(relative(1.3))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_owned()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.45))),
                    ),
            )
            .child(note(detail.to_owned(), t))
            .into_any_element()
    }

    /// Returns a row that can be chosen. A chosen row carries the accent tint, dots that fade
    /// from its leading edge and the accent edge. Add its icon and label as children.
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
            .when(chosen, |row| {
                row.child(chosen_layer(t.accent, crate::theme::RADIUS_CONTROL))
            })
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
                        cell.cursor_default()
                            .child(chosen_layer(t.accent, crate::theme::RADIUS_SMALL))
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

    fn setting_row(&self, label: &str, detail: &str, control: AnyElement) -> AnyElement {
        let t = self.theme;
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
                            .child(div().text_size(px(13.)).child(label.to_owned()))
                            .child(note(detail.to_owned(), t)),
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
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .into_any_element()
    }

    // Reports whether any account of the provider is signed in; unknown until one was checked.
    fn provider_signed_in(&self, provider: Provider) -> Option<bool> {
        let workspace = self.workspace.as_ref()?;
        workspace
            .accounts
            .iter()
            .filter(|state| state.account.provider == provider)
            .filter_map(|state| state.status.signed_in)
            .reduce(|any, signed_in| any || signed_in)
    }

    fn provider_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let provider = self.settings_provider;
        let runtime = self.provider_runtime(provider);
        let preferences = self.provider_preferences(provider);
        let mut chooser = div().flex().gap_2();
        for (index, candidate) in Provider::ALL.into_iter().enumerate() {
            let connected = self.provider_signed_in(candidate) == Some(true);
            chooser = chooser.child(
                self.choice_row(("settings-provider", index), candidate == provider)
                    .flex_1()
                    .h(px(44.))
                    .px(px(12.))
                    .border_color(t.border)
                    .child(icon(candidate.icon()).size(px(13.)))
                    .child(candidate.label())
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(kit::marker(if connected { t.added } else { t.subtle }))
                            .child(caps(if connected { "Signed in" } else { "Offline" }, t)),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.settings_provider = candidate;
                        s.send(core::Request::ConnectRuntime {
                            provider: candidate,
                            account: None,
                        });
                        cx.notify();
                    })),
            );
        }
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
        let connection = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .py(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div().flex_1().min_w_0().child(kit::leader(
                            "Runtime",
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
                    .child(
                        self.ghost("provider-refresh", "refresh", "Refresh")
                            .on_click(cx.listener(move |s, _, _, _| {
                                s.send(core::Request::ConnectRuntime {
                                    provider,
                                    account: None,
                                })
                            })),
                    ),
            )
            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
            .child(
                self.setting_row(
                    "Accounts",
                    &accounts_summary,
                    self.button("provider-accounts", "users", "Manage accounts")
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.accounts.provider = provider;
                            s.open_settings(ACCOUNTS);
                            cx.notify();
                        }))
                        .into_any_element(),
                ),
            )
            .child(
                div()
                    .pt(px(10.))
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .child(
                        self.ghost(
                            "provider-use-default",
                            provider.icon(),
                            "Use for new conversations",
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let preferences = s.provider_preferences(provider);
                            s.save_agent_preferences(preferences);
                            cx.notify();
                        })),
                    )
                    .when_some(
                        install_guide(provider).filter(|_| runtime.version.is_none()),
                        |d, (label, url)| {
                            d.child(
                                self.ghost("provider-install", "external-link", label)
                                    .on_click(cx.listener(move |_, _, _, cx| cx.open_url(url))),
                            )
                        },
                    ),
            );
        let mut models = div().pt(px(6.)).flex().flex_col().gap(px(1.));
        for (index, model) in runtime.models.iter().enumerate() {
            let mut selected = preferences.clone();
            selected.model = Some(model.id.clone());
            selected.effort =
                (!model.default_effort.is_empty()).then(|| model.default_effort.clone());
            let chosen = preferences.model.as_ref() == Some(&model.id);
            models = models.child(
                self.choice_row(("settings-model", index), chosen)
                    .w_full()
                    .font_family(crate::theme::SANS)
                    .text_size(px(12.5))
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
                            s.save_agent_preferences(selected.clone());
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
                                        .font_family(crate::theme::SANS)
                                        .font_weight(FontWeight::NORMAL)
                                        .text_size(px(12.))
                                        .line_height(relative(1.4))
                                        .text_color(t.muted)
                                        .child(mode.description.clone()),
                                ),
                        )
                        .when(chosen, |row| {
                            row.child(icon("check").size(px(13.)).text_color(t.accent))
                        })
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.save_agent_preferences(selected.clone());
                            cx.notify();
                        })),
                );
            }
        }
        let messages = setting_group("Messages", None, t).child(if runtime.steering {
            self.setting_row(
                "Messages during a turn",
                "Queue waits for completion. Steer updates the current turn.",
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
            )
        } else {
            note(
                "Messages sent during a turn are queued. This provider does not support steering.",
                t,
            )
            .pt(px(8.))
            .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .gap(px(GROUP_GAP))
            .child(self.section_title(
                "Providers",
                "Connect a local CLI and choose the defaults for new conversations.",
            ))
            .child(chooser)
            .when_some(runtime.error, |d, error| {
                d.child(error_block("Runtime error", error, t))
            })
            .child(connection)
            .child(defaults)
            .child(setting_group("Default access", None, t).child(access))
            .child(messages)
            .child(self.compaction_settings(provider, &runtime.models, cx))
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

    // Builds the automatic compaction rules for one provider: the rule for every provider, the
    // provider's own rule, and one row for each of its models.
    fn compaction_settings(
        &self,
        provider: Provider,
        models: &[Model],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let rules = &self.settings.auto_compact;
        let shared = rules.default.bounded();
        let own = rules.providers.get(&provider).map(|rule| rule.bounded());
        let name = provider.label();
        let mut group = setting_group("Context compaction", None, t)
            .child(
                note(
                    "When a task's context reaches this share of the model's window, Kodem asks the provider to summarize the conversation.",
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
                            .text_size(px(12.5))
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

// Returns where to get a provider's CLI when Kodem cannot find it.
fn install_guide(provider: Provider) -> Option<(&'static str, &'static str)> {
    match provider {
        Provider::Claude => Some((
            "Claude Code installation guide",
            "https://code.claude.com/docs/en/overview",
        )),
        Provider::Codex => None,
    }
}
