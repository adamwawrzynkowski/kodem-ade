//! Separate, compact choices for model capabilities, reasoning and permissions.
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
        let mut content = div().flex().flex_col().flex_shrink_0().gap_1();
        let preferences = self.composer_preferences();
        let task_id = self.view.as_ref().map(|view| view.task.id.clone());
        let disabled = self.creating_task
            || self
                .view
                .as_ref()
                .is_some_and(|view| view.task.state.active());
        if self.panel == Panel::Models {
            let default_task_id = task_id.clone();
            let mut models = div().id("model-list");
            models = models.child(
                self.ghost("default-model", "bot", "Codex default")
                    .w_full()
                    .h(px(32.))
                    .justify_start()
                    .disabled(disabled)
                    .when(preferences.model.is_none(), |b| {
                        b.bg(t.selected).text_color(t.text)
                    })
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.choose_model(default_task_id.clone(), None, None, cx);
                    })),
            );
            for (i, model) in runtime.models.iter().enumerate() {
                let chosen = preferences.model.as_deref() == Some(&model.id);
                let id = task_id.clone();
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
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.choose_model(
                                id.clone(),
                                Some(model_id.clone()),
                                Some(effort.clone()),
                                cx,
                            );
                        })),
                );
            }
            content = content.child(models);
        }
        if self.panel == Panel::Reasoning {
            if let Some(model) = runtime
                .models
                .iter()
                .find(|m| Some(&m.id) == preferences.model.as_ref())
                .filter(|m| !m.efforts.is_empty())
            {
                let selected = preferences
                    .effort
                    .as_deref()
                    .unwrap_or(&model.default_effort);
                let key = format!(
                    "{}:{}:{:?}",
                    task_id.as_deref().unwrap_or("new-task"),
                    model.id,
                    model.efforts
                );
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
                    let task_id = task_id.clone();
                    let model_id = model.id.clone();
                    let subscription = cx.subscribe(
                        &slider,
                        move |s, _, event: &effort_slider::EffortSelected, cx| {
                            s.choose_model(
                                task_id.clone(),
                                Some(model_id.clone()),
                                Some(event.0.clone()),
                                cx,
                            );
                        },
                    );
                    self.effort_slider = Some((key, slider, subscription));
                }
                let slider = &self.effort_slider.as_ref().unwrap().1;
                slider.update(cx, |s, cx| s.configure(selected, t, disabled, cx));
                content = content.child(div().px_2().py_1().child(slider.clone()));
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
                AccessMode::AutoReview,
                AccessMode::FullAccess,
            ]
            .into_iter()
            .enumerate()
            {
                let id = task_id.clone();
                let chosen = preferences.access == access;
                let description = match access {
                    AccessMode::ReadOnly => "Inspect files without editing or network access",
                    AccessMode::Workspace => "Edit the checkout; ask for broader permissions",
                    AccessMode::AutoReview => {
                        "AI reviews permission requests and can deny unsafe actions"
                    }
                    AccessMode::FullAccess => "Unrestricted files and network; no approval prompts",
                };
                content = content.child(
                    self.ghost(
                        ("access-option", i),
                        match access {
                            AccessMode::FullAccess => "lock-open",
                            AccessMode::AutoReview => "bot",
                            _ => "shield",
                        },
                        "",
                    )
                    .w_full()
                    .h_auto()
                    .min_h(px(50.))
                    .py_2()
                    .justify_start()
                    .disabled(disabled)
                    .when(chosen, |b| b.bg(t.selected).text_color(t.text))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(access.label()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .line_height(relative(1.25))
                                    .text_color(t.muted)
                                    .child(description),
                            ),
                    )
                    .when(chosen, |b| {
                        b.child(icon("check").size(px(13.)).text_color(t.accent))
                    })
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Some(task_id) = &id {
                            s.send(core::Request::EditTask {
                                task_id: task_id.clone(),
                                edit: TaskEdit::Access(access),
                            });
                        } else {
                            let mut preferences = s.composer_preferences();
                            preferences.access = access;
                            s.new_preferences = Some(preferences);
                        }
                        cx.notify();
                    })),
                );
            }
        }
        if disabled {
            content = content.child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .child("These controls unlock when the current turn finishes."),
            );
        }
        if self.panel == Panel::Models {
            content = content.child(
                self.ghost("refresh-models", "refresh", "Refresh models")
                    .w_full()
                    .justify_start()
                    .text_size(px(11.))
                    .on_click(cx.listener(|s, _, _, _| s.send(core::Request::ConnectRuntime))),
            );
        }
        if self.panel == Panel::Models && runtime.models.is_empty() {
            content = content.child(
                div()
                    .px_2()
                    .text_size(px(11.))
                    .text_color(t.subtle)
                    .child("Refresh to load your runtime's model catalog."),
            );
        }
        content.into_any_element()
    }
    /// Returns current task options or the draft's overrides of the saved defaults.
    pub(super) fn composer_preferences(&self) -> AgentPreferences {
        if let Some(view) = &self.view {
            AgentPreferences {
                model: view.task.model.clone(),
                effort: view.task.effort.clone(),
                access: view.task.access,
            }
        } else {
            self.new_preferences.clone().unwrap_or_else(|| {
                self.workspace
                    .as_ref()
                    .map(|w| w.settings.agent_defaults.clone())
                    .unwrap_or_default()
            })
        }
    }

    fn choose_model(
        &mut self,
        task_id: Option<String>,
        model: Option<String>,
        effort: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(task_id) = task_id {
            self.send(core::Request::EditTask {
                task_id,
                edit: TaskEdit::Model { model, effort },
            });
        } else {
            let mut preferences = self.composer_preferences();
            preferences.model = model;
            preferences.effort = effort;
            self.new_preferences = Some(preferences);
        }
        cx.notify();
    }
}
