use super::*;
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
        for (i, (glyph, label)) in [
            ("sun", "Appearance"),
            ("folder", "Workspace"),
            ("settings-2", "Providers"),
            ("bell", "Notifications"),
            ("keyboard", "Shortcuts"),
            ("folder", "Projects"),
        ]
        .into_iter()
        .enumerate()
        {
            navigation = navigation.child(
                self.ghost(("settings-section", i), glyph, label)
                    .h(px(30.))
                    .px(px(8.))
                    .justify_start()
                    .when(self.settings_section == i, |b| {
                        b.bg(t.selected).text_color(t.text)
                    })
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.settings_section = i;
                        if i == 2 {
                            s.settings_provider = s.composer_preferences().provider;
                            s.send(core::Request::ConnectRuntime {
                                provider: s.settings_provider,
                            });
                        }
                        cx.notify();
                    })),
            );
        }
        let mut body = div()
            .id("preferences-content")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .px(px(28.))
            .py(px(24.))
            .flex()
            .flex_col()
            .gap(px(14.));
        match self.settings_section {
            5 => {
                body = body.child(self.project_settings(cx));
            }
            0 => {
                body = body.child(self.section_title(
                    "Appearance",
                    "Set the workspace's contrast and reading size.",
                ));
                let mut themes = div().flex().gap(px(12.));
                for (i, (light, name, glyph)) in [(false, "Dark", "moon"), (true, "Light", "sun")]
                    .into_iter()
                    .enumerate()
                {
                    let preview = Theme::with_accent(light, self.settings.accent);
                    themes = themes.child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .id(("theme-preview", i))
                                    .h(px(96.))
                                    .p(px(6.))
                                    .flex()
                                    .gap(px(6.))
                                    .bg(preview.canvas)
                                    .rounded(px(crate::theme::RADIUS_SURFACE))
                                    .border_1()
                                    .border_color(if self.settings.light == light {
                                        t.accent
                                    } else {
                                        t.border
                                    })
                                    .when(self.settings.light == light, |d| {
                                        d.shadow(vec![BoxShadow {
                                            inset: false,
                                            color: t.accent.opacity(0.35),
                                            offset: point(px(0.), px(0.)),
                                            blur_radius: px(0.),
                                            spread_radius: px(2.),
                                        }])
                                    })
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
                                            .child(
                                                div().h(px(3.)).w(relative(0.7)).bg(preview.border),
                                            )
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
                                                    .child(
                                                        div()
                                                            .size(px(10.))
                                                            .rounded(px(2.))
                                                            .bg(preview.accent),
                                                    ),
                                            ),
                                    )
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        s.settings.light = light;
                                        s.theme = Theme::with_accent(light, s.settings.accent);
                                        s.save_settings();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                self.ghost(("theme-choice", i), glyph, name)
                                    .justify_start()
                                    .px(px(4.))
                                    .when(self.settings.light == light, |b| b.text_color(t.text))
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        s.settings.light = light;
                                        s.theme = Theme::with_accent(light, s.settings.accent);
                                        s.save_settings();
                                        cx.notify();
                                    })),
                            ),
                    );
                }
                body = body
                    .child(themes)
                    .child(self.accent_choices(cx))
                    .child(
                        self.setting_row(
                            "Conversation text",
                            "Size of messages and prompt text.",
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    self.icon_button("font-minus", "minus", "Decrease text size")
                                        .on_click(cx.listener(|s, _, _, cx| {
                                            s.settings.font_size =
                                                (s.settings.font_size - 1.).max(12.);
                                            s.save_settings();
                                            cx.notify();
                                        })),
                                )
                                .child(div().w(px(48.)).flex().justify_center().child(figure(
                                    format!("{} PT", self.settings.font_size),
                                    t.text,
                                )))
                                .child(
                                    self.icon_button("font-plus", "plus", "Increase text size")
                                        .on_click(cx.listener(|s, _, _, cx| {
                                            s.settings.font_size =
                                                (s.settings.font_size + 1.).min(22.);
                                            s.save_settings();
                                            cx.notify();
                                        })),
                                )
                                .into_any_element(),
                        ),
                    )
                    .child(
                        self.setting_row(
                            "Task density",
                            "Adjust the spacing between task rows.",
                            self.button(
                                "density",
                                "settings-2",
                                if self.settings.comfortable {
                                    "Comfortable"
                                } else {
                                    "Compact"
                                },
                            )
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.settings.comfortable = !s.settings.comfortable;
                                s.save_settings();
                                cx.notify();
                            }))
                            .into_any_element(),
                        ),
                    );
            }
            1 => {
                body = body
                    .child(
                        self.section_title(
                            "Workspace",
                            "Choose how projects and tasks are arranged.",
                        ),
                    )
                    .child(
                        self.setting_row(
                            "Default task location",
                            "Use this for every new task. You can override it in the composer.",
                            self.button(
                                "default-task-location",
                                "branch",
                                if self.settings.default_worktree {
                                    "New worktree"
                                } else {
                                    "Current checkout"
                                },
                            )
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.settings.default_worktree = !s.settings.default_worktree;
                                s.new_worktree_override = None;
                                s.save_settings();
                                cx.notify();
                            }))
                            .into_any_element(),
                        ),
                    )
                    .child(
                        self.setting_row(
                            "Open file diffs in",
                            "Choose where a file opens when you select a change.",
                            self.button(
                                "diff-placement",
                                if self.settings.diff_placement == DiffPlacement::Conversation {
                                    "chat"
                                } else {
                                    "panel-right"
                                },
                                if self.settings.diff_placement == DiffPlacement::Conversation {
                                    "Conversation"
                                } else {
                                    "Right panel"
                                },
                            )
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.settings.diff_placement =
                                    if s.settings.diff_placement == DiffPlacement::Conversation {
                                        DiffPlacement::RightPanel
                                    } else {
                                        DiffPlacement::Conversation
                                    };
                                if s.reviewing_file {
                                    s.show_review = true;
                                }
                                s.save_settings();
                                cx.notify();
                            }))
                            .into_any_element(),
                        ),
                    )
                    .child(
                        self.setting_row(
                            "Sidebar width",
                            "You can also drag its right edge.",
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    self.icon_button(
                                        "sidebar-narrow",
                                        "chevron-left",
                                        "Narrow sidebar",
                                    )
                                    .on_click(cx.listener(
                                        |s, _, _, cx| {
                                            s.settings.sidebar_width =
                                                (s.settings.sidebar_width - 20.).max(220.);
                                            s.save_settings();
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(div().w(px(48.)).flex().justify_center().child(figure(
                                    format!("{} PT", self.settings.sidebar_width as u32),
                                    t.text,
                                )))
                                .child(
                                    self.icon_button(
                                        "sidebar-widen",
                                        "chevron-right",
                                        "Widen sidebar",
                                    )
                                    .on_click(cx.listener(
                                        |s, _, _, cx| {
                                            s.settings.sidebar_width =
                                                (s.settings.sidebar_width + 20.).min(420.);
                                            s.save_settings();
                                            cx.notify();
                                        },
                                    )),
                                )
                                .into_any_element(),
                        ),
                    )
                    .child(caps("Task order", t).mt(px(6.)));
                for (i, (sort, label)) in [
                    (Sort::Recent, "Recently updated"),
                    (Sort::Name, "Name"),
                    (Sort::Attention, "Needs attention"),
                    (Sort::Created, "Recently created"),
                ]
                .into_iter()
                .enumerate()
                {
                    body = body.child(
                        self.ghost(("sort", i), "chat", label)
                            .justify_start()
                            .child(div().flex_1())
                            .when(self.settings.sort == sort, |b| {
                                b.text_color(t.text)
                                    .bg(t.selected)
                                    .child(icon("check").size(px(13.)).text_color(t.accent))
                            })
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.settings.sort = sort;
                                s.save_settings();
                                cx.notify();
                            })),
                    );
                }
            }
            2 => {
                body = body.child(self.provider_settings(cx));
                body = body.child(div().mt_4().border_t_1().border_color(t.border).pt_4().flex().flex_col().gap_3()
                    .child(caps("Background tasks", t))
                    .child(div().text_size(px(12.)).text_color(t.muted).child("Tasks and terminals continue when you close the window. An idle host stops after five minutes when no task or shell is running."))
                    .child(self.ghost("shutdown-host", "stop", "Stop idle host and quit").justify_start().on_click(cx.listener(|s, _, _, _| s.send(core::Request::Shutdown)))));
            }
            3 => {
                body = body
                    .child(self.section_title(
                        "Notifications",
                        "Get task updates, even after closing the window.",
                    ))
                    .child(self.notification_toggle(
                        "notifications-enabled",
                        "Task notifications",
                        "Send macOS notifications for the events selected below.",
                        self.settings.notifications.enabled,
                        |n| &mut n.enabled,
                        cx,
                    ))
                    .child(self.notification_toggle(
                        "notifications-attention",
                        "Needs attention",
                        "When a task needs input, approval, or delivery reconciliation.",
                        self.settings.notifications.attention,
                        |n| &mut n.attention,
                        cx,
                    ))
                    .child(self.notification_toggle(
                        "notifications-completed",
                        "Finished",
                        "When a task finishes its turn.",
                        self.settings.notifications.completed,
                        |n| &mut n.completed,
                        cx,
                    ))
                    .child(self.notification_toggle(
                        "notifications-failed",
                        "Failed",
                        "When a task or its submission fails.",
                        self.settings.notifications.failed,
                        |n| &mut n.failed,
                        cx,
                    ))
                    .child(self.notification_toggle(
                        "notifications-interrupted",
                        "Interrupted",
                        "When a task stops before finishing.",
                        self.settings.notifications.interrupted,
                        |n| &mut n.interrupted,
                        cx,
                    ))
                    .child(self.notification_toggle(
                        "notifications-sound",
                        "Play sound",
                        "Use the selected sound with task notifications.",
                        self.settings.notifications.play_sound,
                        |n| &mut n.play_sound,
                        cx,
                    ));
                let mut sounds = div().flex().flex_wrap().gap_2();
                for (i, sound) in NotificationSound::ALL.into_iter().enumerate() {
                    let selected = self.settings.notifications.sound == sound;
                    sounds = sounds.child(
                        self.ghost(
                            ("notification-sound", i),
                            if selected { "circle-check" } else { "bell" },
                            sound.label(),
                        )
                        .border_1()
                        .border_color(t.border)
                        .when(selected, |b| {
                            b.bg(t.selected).border_color(t.edge).text_color(t.text)
                        })
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.settings.notifications.sound = sound;
                            s.save_settings();
                            cx.notify();
                        })),
                    );
                }
                let delivery = self.workspace.as_ref().map(|w| &w.notification_delivery);
                let pending = matches!(delivery, Some(NotificationDelivery::Pending));
                body = body
                    .child(caps("Notification sound", t).mt(px(6.)))
                    .child(sounds)
                    .child(self.setting_row("Test notification", "Send a preview using the selected sound settings, even if task notifications are off.",
                        self.button("test-notification", "bell", if pending { "Sending…" } else { "Test notification" })
                            .disabled(pending)
                            .on_click(cx.listener(|s, _, _, cx| {
                                if let Some(workspace) = &mut s.workspace {
                                    workspace.notification_delivery = NotificationDelivery::Pending;
                                }
                                s.send(core::Request::TestNotification {
                                    sound: s.settings.notifications.play_sound.then_some(s.settings.notifications.sound),
                                });
                                cx.notify();
                            })).into_any_element()))
                    .child(div().text_size(px(12.)).text_color(t.muted).child("macOS controls notification permission, banners, sounds, and Focus settings."));
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
                if let Some(message) = message {
                    body = body.child(
                        div()
                            .text_size(px(12.))
                            .text_color(
                                if matches!(delivery, Some(NotificationDelivery::Failed(_))) {
                                    t.removed
                                } else {
                                    t.muted
                                },
                            )
                            .child(message),
                    );
                }
            }
            _ => {
                body = body.child(self.section_title(
                    "Keyboard shortcuts",
                    "Move around the workspace without leaving the keyboard.",
                ));
                for (label, key) in [
                    ("Open repository", "⌘ O"),
                    ("New task", "⌘ N"),
                    ("Send message", "Return / ⌘ Return"),
                    ("New line in prompt", "Shift Return"),
                    ("Copy diff selection", "⌘ C"),
                    ("Search tasks", "⌘ F"),
                    ("Toggle sidebar", "⌘ B"),
                    ("Toggle changes", "⌘ D"),
                    ("Settings", "⌘ ,"),
                    ("Close dialog", "Esc"),
                ] {
                    body = body.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pb(px(10.))
                            .border_b_1()
                            .border_color(t.border)
                            .text_size(px(13.))
                            .child(label)
                            .child(div().flex().gap(px(4.)).children(key.split(" / ").map(
                                |chord| {
                                    div()
                                        .px(px(6.))
                                        .h(px(20.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(crate::theme::RADIUS_SMALL))
                                        .bg(t.raised)
                                        .border_1()
                                        .border_b_2()
                                        .border_color(t.edge)
                                        .font_family(crate::theme::MONO)
                                        .text_size(px(11.))
                                        .text_color(t.text)
                                        .child(chord.to_owned())
                                },
                            ))),
                    );
                }
            }
        }
        div()
            .h(px(520.))
            .flex()
            .child(navigation)
            .child(body)
            .into_any_element()
    }
    fn notification_toggle(
        &self,
        id: &'static str,
        label: &str,
        detail: &str,
        enabled: bool,
        field: fn(&mut NotificationSettings) -> &mut bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label,
            detail,
            self.toggle(id, label.to_owned(), enabled)
                .on_click(cx.listener(move |s, _, _, cx| {
                    let enabled = field(&mut s.settings.notifications);
                    *enabled = !*enabled;
                    s.save_settings();
                    cx.notify();
                }))
                .into_any_element(),
        )
    }
    /// Returns a settings section heading and its description.
    pub(super) fn section_title(&self, title: &str, detail: &str) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .mb(px(6.))
            .child(
                div()
                    .text_size(px(18.))
                    .line_height(relative(1.3))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(self.theme.muted)
                    .child(detail.to_owned()),
            )
            .into_any_element()
    }
    /// Returns a two-state switch with a square pixel knob and a dithered off track.
    fn toggle(&self, id: impl Into<ElementId>, label: String, on: bool) -> Button {
        let t = self.theme;
        Button::new(id)
            .relative()
            .w(px(34.))
            .h(px(18.))
            .p(px(2.))
            .flex_shrink_0()
            .rounded(px(crate::theme::RADIUS_SMALL + 1.))
            .border_1()
            .border_color(if on { t.accent } else { t.edge })
            .bg(if on { t.accent } else { t.inset })
            .cursor_pointer()
            .justify_start()
            .accessibility_label(format!("{label}: {}", if on { "on" } else { "off" }))
            .focus(move |s| s.border_color(t.text))
            .when(!on, |b| {
                b.child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            dither::paint_flat(window, bounds, 1., t.text.opacity(0.18), 0.5)
                        },
                    )
                    .absolute()
                    .inset(px(2.)),
                )
            })
            .child(
                div()
                    .size(px(12.))
                    .rounded(px(2.))
                    .when(on, |d| d.ml(px(14.)))
                    .bg(if on { t.on_accent } else { t.muted }),
            )
    }
    fn setting_row(&self, label: &str, detail: &str, control: AnyElement) -> AnyElement {
        div()
            .py(px(12.))
            .flex()
            .items_center()
            .gap(px(16.))
            .border_b_1()
            .border_color(self.theme.border)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_size(px(13.)).child(label.to_owned()))
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(relative(1.4))
                            .text_color(self.theme.muted)
                            .child(detail.to_owned()),
                    ),
            )
            .child(control)
            .into_any_element()
    }
    fn provider_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let provider = self.settings_provider;
        let runtime = self.provider_runtime(provider);
        let preferences = self.provider_preferences(provider);
        let mut chooser = div().flex().gap_2();
        for (index, candidate) in Provider::ALL.into_iter().enumerate() {
            let connected = self.provider_runtime(candidate).authenticated == Some(true);
            chooser = chooser.child(
                self.ghost(
                    ("settings-provider", index),
                    candidate.icon(),
                    candidate.label(),
                )
                .flex_1()
                .h(px(44.))
                .px(px(12.))
                .justify_start()
                .border_1()
                .border_color(if candidate == provider {
                    t.edge
                } else {
                    t.border
                })
                .when(candidate == provider, |b| {
                    b.bg(t.selected).text_color(t.text)
                })
                .child(div().flex_1())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .child(
                            div()
                                .size(px(5.))
                                .bg(if connected { t.added } else { t.subtle }),
                        )
                        .child(caps(if connected { "Signed in" } else { "Offline" }, t)),
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.settings_provider = candidate;
                    s.send(core::Request::ConnectRuntime {
                        provider: candidate,
                    });
                    cx.notify();
                })),
            );
        }
        let mut content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.section_title(
                "Providers",
                "Connect a local CLI and choose the defaults for new conversations.",
            ))
            .child(chooser)
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
            .child(
                self.setting_row(
                    "Runtime",
                    &runtime
                        .version
                        .clone()
                        .unwrap_or_else(|| "Not connected".into()),
                    self.ghost("provider-refresh", "refresh", "Refresh")
                        .on_click(cx.listener(move |s, _, _, _| {
                            s.send(core::Request::ConnectRuntime { provider })
                        }))
                        .into_any_element(),
                ),
            )
            .child(
                self.setting_row(
                    "Authentication",
                    match runtime.authenticated {
                        Some(true) => "Signed in through the provider CLI",
                        Some(false) => "Sign in in Terminal, then refresh the connection",
                        None => "Refresh the connection to check authentication",
                    },
                    self.ghost("provider-login", "copy", "Copy sign-in command")
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                provider.login_command().into(),
                            ))
                        }))
                        .into_any_element(),
                ),
            )
            .child(
                div()
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .bg(t.inset)
                    .border_1()
                    .border_color(t.border)
                    .font_family(crate::theme::MONO)
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .child(format!("$ {}", provider.login_command())),
            )
            .child(
                self.setting_row(
                    "Sign out",
                    "Run the command in Terminal, then refresh the connection",
                    self.ghost("provider-logout", "copy", "Copy sign-out command")
                        .on_click(cx.listener(move |_, _, _, cx| {
                            let command = match provider {
                                Provider::Codex => "codex logout",
                                Provider::Devin => "devin auth logout",
                            };
                            cx.write_to_clipboard(ClipboardItem::new_string(command.into()));
                        }))
                        .into_any_element(),
                ),
            )
            .when(
                provider == Provider::Devin && runtime.version.is_none(),
                |d| {
                    d.child(
                        self.ghost(
                            "provider-install",
                            "external-link",
                            "Devin installation guide",
                        )
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.open_url("https://docs.devin.ai/get-started/devin-intro")
                        })),
                    )
                },
            )
            .child(caps("Default model", t).mt(px(6.)));
        let default_preferences = preferences.clone();
        content = content.child(
            self.ghost(
                "settings-default-model",
                provider.icon(),
                format!("{} default", provider.label()),
            )
            .w_full()
            .justify_start()
            .when(preferences.model.is_none(), |b| b.bg(t.selected))
            .on_click(cx.listener(move |s, _, _, cx| {
                let mut preferences = default_preferences.clone();
                preferences.model = None;
                preferences.effort = None;
                s.save_agent_preferences(preferences);
                cx.notify();
            })),
        );
        for (index, model) in runtime.models.iter().enumerate() {
            let mut selected = preferences.clone();
            selected.model = Some(model.id.clone());
            selected.effort =
                (!model.default_effort.is_empty()).then(|| model.default_effort.clone());
            content = content.child(
                self.ghost(
                    ("settings-model", index),
                    provider.icon(),
                    model.name.clone(),
                )
                .w_full()
                .justify_start()
                .when(preferences.model.as_ref() == Some(&model.id), |b| {
                    b.bg(t.selected)
                })
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.save_agent_preferences(selected.clone());
                    cx.notify();
                })),
            );
        }
        if let Some(model) = runtime
            .models
            .iter()
            .find(|model| preferences.model.as_ref() == Some(&model.id))
        {
            if !model.efforts.is_empty() {
                let mut efforts = div().flex().flex_wrap().gap_1();
                for (index, effort) in model.efforts.iter().enumerate() {
                    let mut selected = preferences.clone();
                    selected.effort = Some(effort.clone());
                    let chosen = preferences
                        .effort
                        .as_deref()
                        .unwrap_or(&model.default_effort)
                        == effort;
                    efforts = efforts.child(
                        self.ghost(("settings-reasoning", index), "settings-2", effort.clone())
                            .when(chosen, |b| b.bg(t.selected))
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.save_agent_preferences(selected.clone());
                                cx.notify();
                            })),
                    );
                }
                content = content.child(self.setting_row(
                    "Reasoning",
                    "Levels reported by the selected model",
                    efforts.into_any_element(),
                ));
            }
            content = content.child(div().text_size(px(11.)).text_color(t.muted).child(
                if model.images {
                    "Supports image attachments"
                } else {
                    "Image attachments are unavailable for this model"
                },
            ));
        }
        content = content.child(caps("Default access", t).mt(px(6.)));
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
                content = content.child(
                    self.ghost(("settings-access", index), "shield", mode.label())
                        .w_full()
                        .justify_start()
                        .when(preferences.access == mode, |b| b.bg(t.selected))
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
                content = content.child(
                    self.setting_row(
                        &mode.name,
                        &mode.description,
                        self.ghost(
                            ("settings-access", index),
                            "shield",
                            if preferences.provider_access.as_ref() == Some(&mode.id) {
                                "Selected"
                            } else {
                                "Select"
                            },
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.save_agent_preferences(selected.clone());
                            cx.notify();
                        }))
                        .into_any_element(),
                    ),
                );
            }
        }
        if runtime.steering {
            content = content.child(
                self.setting_row(
                    "Messages during a turn",
                    "Queue waits for completion. Steer updates the current turn.",
                    self.ghost(
                        "provider-send-mode",
                        "arrow-right",
                        if self.settings.send_mode == SendMode::Queue {
                            "Queue"
                        } else {
                            "Steer"
                        },
                    )
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.settings.send_mode = if s.settings.send_mode == SendMode::Queue {
                            SendMode::Steer
                        } else {
                            SendMode::Queue
                        };
                        s.save_settings();
                        cx.notify();
                    }))
                    .into_any_element(),
                ),
            );
        } else {
            content = content.child(div().text_size(px(11.)).text_color(t.muted).child(
                "Messages sent during a turn are queued. This provider does not support steering.",
            ));
        }
        if let Some(error) = runtime.error {
            content = content.child(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .bg(t.removed_bg)
                    .border_1()
                    .border_color(t.removed.opacity(0.3))
                    .text_size(px(12.))
                    .text_color(t.removed)
                    .child(error),
            );
        }
        content.into_any_element()
    }
}
