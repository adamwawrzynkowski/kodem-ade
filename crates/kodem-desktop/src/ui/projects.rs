//! Appearance preferences and project presentation controls.
use super::*;

impl Shell {
    /// Builds the saved application accent picker. The first choice is the theme's own accent.
    pub(super) fn accent_choices(&self, cx: &mut Context<Self>) -> AnyElement {
        let light = self.theme.light;
        let own = Theme::new(self.settings.theme).accent;
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(kit::section_header("Accent", None, self.theme))
            .child(div().flex().flex_wrap().gap(px(6.)).children(
                AccentColor::ALL.into_iter().enumerate().map(|(i, color)| {
                    let (tone, label) = if color == AccentColor::default() {
                        (own, "Theme accent")
                    } else {
                        (crate::theme::accent_color(color, light), color.label())
                    };
                    self.swatch(
                        ("accent-choice", i),
                        tone,
                        label,
                        self.settings.accent == color,
                        false,
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.settings.accent = color;
                        s.theme = Theme::with_accent(s.settings.theme, color);
                        s.save_settings();
                        cx.notify();
                    }))
                }),
            ))
            .into_any_element()
    }

    /// Returns a square color choice; `follow` dithers it to mean "use the app accent".
    pub(super) fn swatch(
        &self,
        id: impl Into<ElementId>,
        color: Hsla,
        label: &'static str,
        selected: bool,
        follow: bool,
    ) -> Button {
        let t = self.theme;
        Button::new(id)
            .size(px(26.))
            .p(px(2.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .border_1()
            .border_color(if selected {
                t.text
            } else {
                hsla(0., 0., 0., 0.)
            })
            .hover(move |b| b.border_color(t.edge))
            .cursor_pointer()
            .accessibility_label(label)
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: label.into(),
                    theme: t,
                })
                .into()
            })
            .child(
                div()
                    .relative()
                    .size_full()
                    .rounded(px(crate::theme::RADIUS_SMALL))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(if follow { t.inset } else { color })
                    .when(follow, |d| {
                        d.child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    dither::paint_flat(window, bounds, 1., color, 0.5)
                                },
                            )
                            .absolute()
                            .inset_0(),
                        )
                    })
                    .when(selected, |d| {
                        d.child(icon("check").size(px(12.)).text_color(if follow {
                            t.text
                        } else {
                            t.on_accent
                        }))
                    }),
            )
    }

    // Describes saved agent options in one line: provider, account, model, effort and access.
    fn preferences_summary(&self, preferences: &AgentPreferences) -> String {
        let runtime = self.provider_runtime(preferences.provider);
        let mut parts = vec![preferences.provider.label().to_owned()];
        if let Some(state) = self
            .workspace
            .as_ref()
            .and_then(|w| w.account(preferences.provider, preferences.account.as_deref()))
            .filter(|state| !state.account.is_default())
        {
            parts.push(state.account.name.clone());
        }
        parts.push(preferences.model.as_ref().map_or_else(
            || {
                runtime
                    .default_model
                    .as_ref()
                    .and_then(|id| runtime.models.iter().find(|model| &model.id == id))
                    .map_or_else(
                        || preferences.provider.label().to_owned(),
                        |model| model.name.clone(),
                    )
            },
            |id| {
                runtime
                    .models
                    .iter()
                    .find(|model| &model.id == id)
                    .map_or_else(|| id.clone(), |model| model.name.clone())
            },
        ));
        if let Some(effort) = &preferences.effort {
            parts.push(effort.clone());
        }
        parts.push(if preferences.provider == Provider::Codex {
            preferences.access.label().to_owned()
        } else {
            preferences.provider_access.as_ref().map_or_else(
                || "provider default access".to_owned(),
                |id| {
                    runtime
                        .access_modes
                        .iter()
                        .find(|mode| &mode.id == id)
                        .map_or_else(|| id.clone(), |mode| mode.name.clone())
                },
            )
        });
        parts.join(" · ")
    }

    /// Lists projects with their presentation, their default agent and customization actions.
    pub(super) fn project_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut list = div().flex().flex_col().gap_3().child(self.section_title(
            "Projects",
            "Customize how repositories appear, and which provider, account and model their new tasks start with.",
        ));
        for (i, project) in self
            .workspace
            .as_ref()
            .map(|w| w.projects.clone())
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            let editing = project.clone();
            let project_id = project.id.clone();
            let clear_id = project.id.clone();
            let has_default = project.defaults.is_some();
            let summary = project.defaults.as_ref().map_or_else(
                || "Follows the global default".to_owned(),
                |defaults| self.preferences_summary(defaults),
            );
            list = list.child(
                kit::card(t)
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(sidebar::project_icon(&project, t))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_size(text_px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_ellipsis()
                                            .child(project.name),
                                    )
                                    .child(
                                        div()
                                            .font_family(crate::theme::mono())
                                            .text_size(text_px(10.5))
                                            .text_color(t.subtle)
                                            .text_ellipsis_start()
                                            .child(project.path.display().to_string()),
                                    ),
                            )
                            .child(
                                self.icon_button(
                                    ("customize-project", i),
                                    "edit",
                                    "Customize project",
                                )
                                .on_click(cx.listener(
                                    move |s, _, w, cx| {
                                        s.open_project_editor(editing.clone(), cx);
                                        w.focus(&s.aux.read(cx).focus_handle(cx), cx);
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                icon(if has_default { "star-fill" } else { "star" })
                                    .size(px(12.))
                                    .text_color(if has_default { t.accent } else { t.subtle }),
                            )
                            .child(caps("New tasks", t))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(11.))
                                    .text_color(if has_default { t.text } else { t.muted })
                                    .child(summary),
                            )
                            .child(
                                self.ghost(("project-default-edit", i), "settings-2", "Choose…")
                                    .on_click(cx.listener(move |s, _, window, cx| {
                                        // The agent menu of a new task in this project owns the choice.
                                        s.open_empty_chat(cx);
                                        s.selected_project = Some(project_id.clone());
                                        s.save_settings();
                                        s.new_preferences = None;
                                        s.open_agent_menu(Panel::Models, window, cx);
                                    })),
                            )
                            .when(has_default, |row| {
                                row.child(
                                    self.icon_button(
                                        ("project-default-clear", i),
                                        "undo-2",
                                        "Follow the global default",
                                    )
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| {
                                            s.set_project_defaults(clear_id.clone(), None, cx);
                                        },
                                    )),
                                )
                            }),
                    ),
            );
        }
        list.child(
            div().flex().child(
                self.button("settings-add-project", "plus", "Add project")
                    .on_click(cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx))),
            ),
        )
        .into_any_element()
    }

    /// Edits a project's name, color and icon before saving its presentation together.
    pub(super) fn project_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(project) = self.editing_project.clone() else {
            return div().into_any_element();
        };
        let light = self.theme.light;
        let mut colors = div().flex().flex_wrap().gap(px(6.)).child(
            self.swatch(
                "project-color-default",
                t.accent,
                "App accent",
                project.color.is_none(),
                true,
            )
            .on_click(cx.listener(|s, _, _, cx| {
                if let Some(project) = &mut s.editing_project {
                    project.color = None;
                }
                cx.notify();
            })),
        );
        for (i, color) in AccentColor::ALL.into_iter().enumerate() {
            colors = colors.child(
                self.swatch(
                    ("project-color", i),
                    crate::theme::accent_color(color, light),
                    color.label(),
                    project.color == Some(color.rgb(false)),
                    false,
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    if let Some(project) = &mut s.editing_project {
                        project.color = Some(color.rgb(false));
                    }
                    cx.notify();
                })),
            );
        }
        let icons = self.icon_picker(&project, cx);
        let commands = self.command_editor(&project, cx);
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(kit::section_header("Name", None, t))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(sidebar::project_task_tile(&project, t))
                    .child(
                        settings::input_frame(t)
                            .flex_1()
                            .min_w_0()
                            .child(div().flex_1().min_w_0().child(self.aux.clone())),
                    ),
            )
            .child(
                div()
                    .mt(px(6.))
                    .child(kit::section_header("Accent", None, t)),
            )
            .child(colors)
            .child(div().mt(px(6.)).child(kit::section_header("Icon", None, t)))
            .child(
                div()
                    .text_size(text_px(12.))
                    .text_color(t.muted)
                    .child("Use a built-in icon or choose an image. Kodem saves its own copy."),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        self.choice_row("project-icon-auto", project.icon == ProjectIcon::Auto)
                            .h(px(26.))
                            .px(px(9.))
                            .gap(px(5.))
                            .border_color(t.border)
                            .child(icon("sparkles").size(px(13.)))
                            .child(ProjectIcon::Auto.label())
                            .on_click(cx.listener(|s, _, _, cx| {
                                if let Some(project) = &mut s.editing_project {
                                    project.icon = ProjectIcon::Auto;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        self.choice_row(
                            "project-choose-image",
                            matches!(project.icon, ProjectIcon::Image(_)),
                        )
                        .h(px(26.))
                        .px(px(9.))
                        .gap(px(5.))
                        .border_color(t.border)
                        .child(icon("file").size(px(13.)))
                        .child("Choose image…")
                        .on_click(cx.listener(|s, _, _, cx| s.choose_project_icon(cx))),
                    )
                    .child(
                        settings::input_frame(t)
                            .flex_1()
                            .min_w_0()
                            .h(px(26.))
                            .border_color(t.border)
                            .child(icon("search").size(px(12.)).text_color(t.subtle))
                            .child(div().flex_1().min_w_0().child(self.icon_filter.clone())),
                    ),
            )
            .child(icons)
            .child(commands)
            .when_some(self.error.clone(), |d, error| {
                d.child(settings::error_block("Error", error, t))
            })
            .child(
                div()
                    .mt(px(4.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        self.ghost("project-remove", "close", "Remove project…")
                            .text_color(t.removed)
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.error = None;
                                s.panel = Panel::RemoveProject;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.primary("project-save", "check", "Save changes")
                            .disabled(self.aux.read(cx).text().trim().is_empty())
                            .on_click(cx.listener(|s, _, _, cx| {
                                if let Some(project) = &s.editing_project {
                                    s.send(core::Request::CustomizeProject {
                                        project_id: project.id.clone(),
                                        name: s.aux.read(cx).text(),
                                        color: project.color,
                                        icon: project.icon.clone(),
                                        commands: s.edited_commands(project, cx),
                                    });
                                }
                            })),
                    ),
            )
            .into_any_element()
    }
    // Returns the built-in icons that match the filter, in their groups, in a list that scrolls
    // inside the dialog so that the actions under it stay in view.
    fn icon_picker(&self, project: &Project, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let query = self.icon_filter.read(cx).text().trim().to_lowercase();
        let tint = sidebar::project_tint(project, t);
        let mut groups = div().flex().flex_col().gap(px(10.));
        // The position of an icon in the whole table, so that a cell keeps its identity while
        // the filter changes.
        let mut index = 0usize;
        let mut found = false;
        for (group, choices) in ProjectIcon::GROUPS {
            let whole_group = group.to_lowercase().contains(&query);
            let mut cells = div().flex().flex_wrap().gap(px(2.));
            let mut count = 0;
            for choice in choices.iter() {
                index += 1;
                if !whole_group && !choice.label().to_lowercase().contains(&query) {
                    continue;
                }
                count += 1;
                let picked = choice.clone();
                cells = cells.child(
                    self.icon_choice(
                        ("project-icon", index),
                        choice,
                        project.icon == *choice,
                        tint,
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Some(project) = &mut s.editing_project {
                            project.icon = picked.clone();
                        }
                        cx.notify();
                    })),
                );
            }
            if count > 0 {
                found = true;
                groups = groups.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(caps(group, t))
                                .child(figure(count.to_string(), t.subtle)),
                        )
                        .child(cells),
                );
            }
        }
        if !found {
            groups = groups.child(kit::empty_hint("No built-in icon has this name.", t));
        }
        div()
            .id("project-icon-list")
            .flex_shrink_0()
            .max_h(px(236.))
            .overflow_y_scroll()
            .p(px(8.))
            .rounded(px(crate::theme::RADIUS_CONTROL))
            .bg(t.inset)
            .border_1()
            .border_color(t.border)
            .child(groups)
            .into_any_element()
    }

    // Returns one cell of the icon picker: a glyph that names itself in a tooltip. The chosen
    // cell carries the tint of the project.
    fn icon_choice(
        &self,
        id: impl Into<ElementId>,
        choice: &ProjectIcon,
        chosen: bool,
        tint: Hsla,
    ) -> Button {
        let t = self.theme;
        let label = choice.label();
        self.control(id)
            .size(px(28.))
            .px_0()
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .text_color(t.muted)
            .aria_selected(chosen)
            .accessibility_label(label)
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: label.into(),
                    theme: t,
                })
                .into()
            })
            .child(icon(choice.glyph().unwrap_or_default()).size(px(16.)))
            .when(chosen, |cell| {
                cell.bg(tint.opacity(0.12))
                    .border_color(tint.opacity(0.6))
                    .text_color(tint)
            })
            .when(!chosen, |cell| {
                cell.hover(move |cell| cell.bg(t.selected).text_color(t.text))
            })
            .on_press(move |position, cx| fx::ripple(position, t.muted, cx))
    }

    fn choose_project_icon(&mut self, cx: &mut Context<Self>) {
        let Some(project_id) = self
            .editing_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            return;
        };
        let selected = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose project image".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = selected.await
                && let Some(path) = paths.first()
            {
                let _ = this.update(cx, |s, cx| {
                    if s.panel == Panel::CustomizeProject
                        && let Some(project) = s
                            .editing_project
                            .as_mut()
                            .filter(|project| project.id == project_id)
                    {
                        project.icon = ProjectIcon::Image(path.clone());
                        s.error = None;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}
