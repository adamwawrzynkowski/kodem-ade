use super::*;
impl Shell {
    /// Builds native preferences with section navigation and labeled controls.
    pub(super) fn settings_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut navigation = div()
            .w(px(172.))
            .h_full()
            .flex_shrink_0()
            .p_3()
            .bg(t.sidebar)
            .flex()
            .flex_col()
            .gap_1()
            .border_r_1()
            .border_color(t.border);
        for (i, (glyph, label)) in [
            ("sun", "Appearance"),
            ("folder", "Workspace"),
            ("bot", "Codex"),
            ("keyboard", "Shortcuts"),
        ]
        .into_iter()
        .enumerate()
        {
            navigation = navigation.child(
                self.ghost(("settings-section", i), glyph, label)
                    .h(px(34.))
                    .justify_start()
                    .when(self.settings_section == i, |b| {
                        b.bg(t.selected).text_color(t.text)
                    })
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.settings_section = i;
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
            .p_6()
            .flex()
            .flex_col()
            .gap_4();
        match self.settings_section {
            0 => {
                body = body.child(self.section_title(
                    "Appearance",
                    "Set the workspace's contrast and reading size.",
                ));
                let mut themes = div().flex().gap_3();
                for (i, (light, name, glyph)) in [(false, "Dark", "moon"), (true, "Light", "sun")]
                    .into_iter()
                    .enumerate()
                {
                    let preview = Theme::new(light);
                    themes = themes.child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .id(("theme-preview", i))
                                    .h(px(100.))
                                    .p_2()
                                    .flex()
                                    .gap_2()
                                    .bg(preview.canvas)
                                    .rounded(px(8.))
                                    .border_2()
                                    .border_color(if self.settings.light == light {
                                        t.accent
                                    } else {
                                        t.border
                                    })
                                    .cursor_pointer()
                                    .child(div().w(px(32.)).rounded(px(3.)).bg(preview.sidebar))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .pt_2()
                                            .child(
                                                div()
                                                    .h(px(5.))
                                                    .w(px(48.))
                                                    .bg(preview.muted)
                                                    .rounded_full(),
                                            )
                                            .child(
                                                div()
                                                    .h(px(4.))
                                                    .w_full()
                                                    .bg(preview.border)
                                                    .rounded_full(),
                                            )
                                            .child(div().flex_1())
                                            .child(
                                                div()
                                                    .h(px(25.))
                                                    .bg(preview.raised)
                                                    .border_1()
                                                    .border_color(preview.border)
                                                    .rounded(px(4.)),
                                            ),
                                    )
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        s.settings.light = light;
                                        s.theme = Theme::new(light);
                                        s.save_settings();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                self.ghost(("theme-choice", i), glyph, name)
                                    .when(self.settings.light == light, |b| b.text_color(t.accent))
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        s.settings.light = light;
                                        s.theme = Theme::new(light);
                                        s.save_settings();
                                        cx.notify();
                                    })),
                            ),
                    );
                }
                body = body
                    .child(themes)
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
                                .child(
                                    div()
                                        .w(px(42.))
                                        .text_center()
                                        .text_size(px(12.))
                                        .child(format!("{} px", self.settings.font_size)),
                                )
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
                                .child(
                                    div().text_size(px(12.)).child(format!(
                                        "{} px",
                                        self.settings.sidebar_width as u32
                                    )),
                                )
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
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Task order"),
                    );
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
                        self.ghost(
                            ("sort", i),
                            if self.settings.sort == sort {
                                "circle-check"
                            } else {
                                "chat"
                            },
                            label,
                        )
                        .justify_start()
                        .when(self.settings.sort == sort, |b| {
                            b.text_color(t.accent).bg(t.accent_bg)
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
                let runtime = self
                    .workspace
                    .as_ref()
                    .map(|w| w.runtime.clone())
                    .unwrap_or_default();
                body = body
                    .child(self.section_title(
                        "Codex",
                        "The local runtime powers your task conversations.",
                    ))
                    .child(
                        self.setting_row(
                            "Runtime",
                            &runtime.version.unwrap_or_else(|| "Not installed".into()),
                            self.ghost("connect-codex", "refresh", "Refresh connection")
                                .on_click(
                                    cx.listener(|s, _, _, _| s.send(core::Request::ConnectRuntime)),
                                )
                                .into_any_element(),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.))
                            .child(
                                icon(if runtime.authenticated == Some(true) {
                                    "circle-check"
                                } else {
                                    "shield"
                                })
                                .text_color(t.accent),
                            )
                            .child(match runtime.authenticated {
                                Some(true) => "Signed in",
                                Some(false) => "Sign in with codex login in Terminal",
                                None => "Connect to check your account",
                            }),
                    )
                    .child(div().text_size(px(12.)).text_color(t.muted).child(
                        "Choose a model and reasoning effort from the conversation composer.",
                    ));
                if let Some(error) = runtime.error {
                    body = body.child(
                        div()
                            .p_3()
                            .rounded(px(7.))
                            .bg(t.removed_bg)
                            .text_size(px(12.))
                            .text_color(t.removed)
                            .child(error),
                    );
                }
                body = body.child(div().mt_4().border_t_1().border_color(t.border).pt_4().flex().flex_col().gap_3()
                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("Background tasks"))
                    .child(div().text_size(px(12.)).text_color(t.muted).child("Tasks and terminals continue when you close the window. An idle host stops after five minutes when no task or shell is running."))
                    .child(self.ghost("shutdown-host", "stop", "Stop idle host and quit").justify_start().on_click(cx.listener(|s, _, _, _| s.send(core::Request::Shutdown)))));
            }
            _ => {
                body = body.child(self.section_title(
                    "Keyboard shortcuts",
                    "Move around the workspace without leaving the keyboard.",
                ));
                for (label, key) in [
                    ("Open repository", "⌘ O"),
                    ("New task", "⌘ N"),
                    ("Send message", "⌘ Return"),
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
                            .pb_2()
                            .border_b_1()
                            .border_color(t.border)
                            .text_size(px(12.))
                            .child(label)
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .bg(t.raised)
                                    .border_1()
                                    .border_color(t.border)
                                    .text_color(t.muted)
                                    .child(key),
                            ),
                    );
                }
            }
        }
        div()
            .h(px(490.))
            .flex()
            .child(navigation)
            .child(body)
            .into_any_element()
    }
    fn section_title(&self, title: &str, detail: &str) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .mb_2()
            .child(
                div()
                    .text_size(px(19.))
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
    fn setting_row(&self, label: &str, detail: &str, control: AnyElement) -> AnyElement {
        div()
            .py_3()
            .flex()
            .items_center()
            .gap_3()
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
                            .text_size(px(11.))
                            .text_color(self.theme.muted)
                            .child(detail.to_owned()),
                    ),
            )
            .child(control)
            .into_any_element()
    }
}
