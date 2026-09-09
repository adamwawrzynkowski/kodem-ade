//! Model capabilities, reasoning and permissions in one compact popover.
use super::*;
impl Shell {
    /// Builds only the model, reasoning or access controls requested by its trigger.
    pub(super) fn model_options(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let runtime = self
            .workspace
            .as_ref()
            .map(|w| w.runtime.clone())
            .unwrap_or_default();
        let mut content = div().flex().flex_col().flex_shrink_0().gap_2().child(
            div()
                .flex()
                .items_center()
                .px_2()
                .text_size(px(11.))
                .text_color(t.muted)
                .child(
                    icon(match self.panel {
                        Panel::Reasoning => "settings-2",
                        Panel::Access => "shield",
                        _ => "openai",
                    })
                    .size(px(14.)),
                )
                .child(div().ml_2().child(match self.panel {
                    Panel::Reasoning => "Reasoning effort",
                    Panel::Access => "Access",
                    _ => "Model",
                }))
                .child(div().flex_1())
                .when(self.panel == Panel::Models, |d| {
                    d.child(
                        self.icon_button("refresh-models", "refresh", "Refresh available models")
                            .on_click(
                                cx.listener(|s, _, _, _| s.send(core::Request::ConnectRuntime)),
                            ),
                    )
                }),
        );
        let Some(view) = self.view.clone() else {
            return content
                .child(
                    div()
                        .p_2()
                        .text_size(px(12.))
                        .text_color(t.muted)
                        .child("Create a task to choose its model, effort and access."),
                )
                .into_any_element();
        };
        let disabled = view.task.state.active();
        if self.panel == Panel::Models {
            let task_id = view.task.id.clone();
            let mut models = div()
                .id("model-list")
                .max_h(px(220.))
                .overflow_y_scroll()
                .restrict_scroll_to_axis();
            models = models.child(
                self.ghost("default-model", "bot", "Codex default")
                    .w_full()
                    .h(px(32.))
                    .justify_start()
                    .disabled(disabled)
                    .when(view.task.model.is_none(), |b| {
                        b.bg(t.selected).text_color(t.text)
                    })
                    .on_click(cx.listener(move |s, _, _, _| {
                        s.send(core::Request::EditTask {
                            task_id: task_id.clone(),
                            edit: TaskEdit::Model {
                                model: None,
                                effort: None,
                            },
                        })
                    })),
            );
            for (i, model) in runtime.models.iter().enumerate() {
                let chosen = view.task.model.as_deref() == Some(&model.id);
                let id = view.task.id.clone();
                let model_id = model.id.clone();
                let effort = model.default_effort.clone();
                models = models.child(
                    self.ghost(("model-option", i), "bot", model.name.clone())
                        .w_full()
                        .h(px(32.))
                        .justify_start()
                        .disabled(disabled)
                        .when(chosen, |b| b.bg(t.selected).text_color(t.text))
                        .child(div().flex_1())
                        .when(chosen, |b| {
                            b.child(icon("check").size(px(13.)).text_color(t.accent))
                        })
                        .on_click(cx.listener(move |s, _, _, _| {
                            s.send(core::Request::EditTask {
                                task_id: id.clone(),
                                edit: TaskEdit::Model {
                                    model: Some(model_id.clone()),
                                    effort: Some(effort.clone()),
                                },
                            })
                        })),
                );
            }
            content = content.child(models);
        }
        if self.panel == Panel::Reasoning {
            if let Some(model) = runtime
                .models
                .iter()
                .find(|m| Some(&m.id) == view.task.model.as_ref())
                .filter(|m| !m.efforts.is_empty())
            {
                let selected = view.task.effort.as_deref().unwrap_or(&model.default_effort);
                let key = format!("{}:{}:{:?}", view.task.id, model.id, model.efforts);
                if self
                    .effort_slider
                    .as_ref()
                    .is_none_or(|(old, _, _)| old != &key)
                {
                    let slider = cx.new(|cx| {
                        effort_slider::EffortSlider::new(
                            model.efforts.clone(),
                            selected.into(),
                            t,
                            cx,
                        )
                    });
                    let task_id = view.task.id.clone();
                    let model_id = model.id.clone();
                    let subscription = cx.subscribe(
                        &slider,
                        move |s, _, event: &effort_slider::EffortSelected, _| {
                            s.send(core::Request::EditTask {
                                task_id: task_id.clone(),
                                edit: TaskEdit::Model {
                                    model: Some(model_id.clone()),
                                    effort: Some(event.0.clone()),
                                },
                            });
                        },
                    );
                    self.effort_slider = Some((key, slider, subscription));
                }
                let slider = &self.effort_slider.as_ref().unwrap().1;
                slider.update(cx, |s, cx| s.configure(selected, t, disabled, cx));
                content = content.child(
                    div()
                        .p_3()
                        .rounded(px(10.))
                        .bg(t.inset)
                        .border_1()
                        .border_color(t.border)
                        .child(slider.clone()),
                );
            } else {
                content = content.child(
                    div()
                        .p_2()
                        .text_size(px(11.))
                        .text_color(t.muted)
                        .child("Choose a model first to see its supported reasoning levels."),
                );
            }
        }
        if self.panel == Panel::Access {
            for (i, access) in [
                AccessMode::ReadOnly,
                AccessMode::Workspace,
                AccessMode::FullAccess,
            ]
            .into_iter()
            .enumerate()
            {
                let id = view.task.id.clone();
                let chosen = view.task.access == access;
                let description = match access {
                    AccessMode::ReadOnly => "Inspect files without editing or network access",
                    AccessMode::Workspace => "Edit the checkout; ask for broader permissions",
                    AccessMode::FullAccess => "Unrestricted files and network; no approval prompts",
                };
                content = content.child(
                    self.ghost(
                        ("access-option", i),
                        if access == AccessMode::FullAccess {
                            "lock-open"
                        } else {
                            "shield"
                        },
                        "",
                    )
                    .w_full()
                    .h(px(43.))
                    .justify_start()
                    .disabled(disabled)
                    .when(chosen, |b| b.bg(t.selected).text_color(t.text))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap_1()
                            .child(div().text_size(px(12.)).child(access.label()))
                            .child(
                                div()
                                    .text_size(px(9.))
                                    .text_color(t.muted)
                                    .child(description),
                            ),
                    )
                    .when(chosen, |b| {
                        b.child(icon("check").size(px(13.)).text_color(t.accent))
                    })
                    .on_click(cx.listener(move |s, _, _, _| {
                        s.send(core::Request::EditTask {
                            task_id: id.clone(),
                            edit: TaskEdit::Access(access),
                        })
                    })),
                );
            }
        }
        if disabled {
            content = content.child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(10.))
                    .text_color(t.muted)
                    .child("These controls unlock when the current turn finishes."),
            );
        }
        if self.panel == Panel::Models && runtime.models.is_empty() {
            content = content.child(
                div()
                    .px_2()
                    .text_size(px(10.))
                    .text_color(t.subtle)
                    .child("Refresh to load your runtime's model catalog."),
            );
        }
        content.into_any_element()
    }
}
