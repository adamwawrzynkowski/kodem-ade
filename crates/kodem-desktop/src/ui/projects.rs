//! Appearance preferences and project presentation controls.
use super::*;

impl Shell {
    /// Builds the saved application accent picker.
    pub(super) fn accent_choices(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child("Accent color")
            .child(div().flex().flex_wrap().gap_2().children(
                AccentColor::ALL.into_iter().enumerate().map(|(i, color)| {
                    self.ghost(("accent-choice", i), "circle-check", color.label())
                        .text_color(rgb(color.rgb(self.settings.light)))
                        .when(self.settings.accent == color, |b| b.bg(t.selected))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.settings.accent = color;
                            s.theme = Theme::with_accent(s.settings.light, color);
                            s.save_settings();
                            cx.notify();
                        }))
                }),
            ))
            .into_any_element()
    }

    /// Lists projects with their current presentation and customization actions.
    pub(super) fn project_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut list =
            div().flex().flex_col().gap_3().child(
                self.section_title("Projects", "Customize how repositories appear in Kodem."),
            );
        for (i, project) in self
            .workspace
            .as_ref()
            .map(|w| w.projects.clone())
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            let editing = project.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_2()
                    .border_b_1()
                    .border_color(t.border)
                    .child(sidebar::project_icon(&project, t))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_ellipsis().child(project.name))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t.muted)
                                    .text_ellipsis()
                                    .child(project.path.display().to_string()),
                            ),
                    )
                    .child(
                        self.icon_button(("customize-project", i), "edit", "Customize project")
                            .on_click(cx.listener(move |s, _, w, cx| {
                                s.aux.update(cx, |input, cx| {
                                    input.set_text(editing.name.clone(), cx)
                                });
                                s.editing_project = Some(editing.clone());
                                s.error = None;
                                s.panel = Panel::CustomizeProject;
                                w.focus(&s.aux.read(cx).focus_handle(cx), cx);
                                cx.notify();
                            })),
                    ),
            );
        }
        list.child(
            self.ghost("settings-add-project", "plus", "Add project…")
                .on_click(cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx))),
        )
        .into_any_element()
    }

    /// Edits a project's name, color and icon before saving its presentation together.
    pub(super) fn project_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(project) = self.editing_project.clone() else {
            return div().into_any_element();
        };
        let mut colors = div().flex().flex_wrap().gap_2().child(
            self.ghost("project-color-default", "refresh", "App accent")
                .when(project.color.is_none(), |b| b.bg(t.selected))
                .on_click(cx.listener(|s, _, _, cx| {
                    if let Some(project) = &mut s.editing_project {
                        project.color = None;
                    }
                    cx.notify();
                })),
        );
        for (i, color) in AccentColor::ALL.into_iter().enumerate() {
            colors = colors.child(
                self.ghost(("project-color", i), "circle-check", color.label())
                    .text_color(rgb(color.rgb(self.settings.light)))
                    .when(project.color == Some(color.rgb(false)), |b| {
                        b.bg(t.selected)
                    })
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Some(project) = &mut s.editing_project {
                            project.color = Some(color.rgb(false));
                        }
                        cx.notify();
                    })),
            );
        }
        let icons = div().flex().flex_wrap().gap_2().children(
            ProjectIcon::ALL.into_iter().enumerate().map(|(i, choice)| {
                self.ghost(
                    ("project-icon", i),
                    choice.glyph().unwrap_or("refresh"),
                    choice.label(),
                )
                .when(project.icon == choice, |b| {
                    b.bg(t.selected).text_color(t.accent)
                })
                .on_click(cx.listener(move |s, _, _, cx| {
                    if let Some(project) = &mut s.editing_project {
                        project.icon = choice.clone();
                    }
                    cx.notify();
                }))
            }),
        );
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(sidebar::project_icon(&project, t))
                    .child("Name"),
            )
            .child(
                div()
                    .p_2()
                    .bg(t.inset)
                    .border_1()
                    .border_color(t.border)
                    .rounded(px(7.))
                    .child(self.aux.clone()),
            )
            .child("Accent color")
            .child(colors)
            .child("Icon")
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .child("Use a built-in icon or choose an image. Kodem saves its own copy."),
            )
            .child(icons)
            .child(
                self.ghost("project-choose-image", "file", "Choose image…")
                    .on_click(cx.listener(|s, _, _, cx| s.choose_project_icon(cx))),
            )
            .when(matches!(project.icon, ProjectIcon::Image(_)), |d| {
                d.child(
                    div().text_size(px(11.)).text_color(t.muted).child(
                        "Custom image selected. Choose a built-in icon above to replace it.",
                    ),
                )
            })
            .when_some(self.error.clone(), |d, error| {
                d.child(div().text_color(t.removed).child(error))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mt_2()
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
                                    });
                                }
                            })),
                    ),
            )
            .into_any_element()
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
