//! The agent menu: provider, account, model, reasoning effort and access for one task or for
//! the next new task, with a per-project default.
use super::*;

/// Name of the effect anchor on the composer's agent chip, where keyboard-opened menus attach.
pub(super) const AGENT_ANCHOR: &str = "composer-agent";
/// Name of the effect anchor on the composer's access chip.
pub(super) const ACCESS_ANCHOR: &str = "composer-access";

impl Shell {
    // A full-width menu row. A chosen row carries the accent tint, an accent edge, dots that
    // fade from its leading edge, and a check mark.
    fn option_row(&self, id: impl Into<ElementId>, chosen: bool, disabled: bool) -> Button {
        let t = self.theme;
        self.control(id)
            .w_full()
            .h(px(30.))
            .px(px(10.))
            .gap(px(8.))
            .justify_start()
            .overflow_hidden()
            .font_family(crate::theme::sans())
            .text_size(text_px(12.5))
            .font_weight(FontWeight::NORMAL)
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .text_color(if chosen { t.text } else { t.muted })
            .disabled(disabled)
            .when(chosen, |row| {
                row.bg(t.accent.opacity(0.1))
                    .layer(dither::row_wash(t.accent, true, false))
                    .child(kit::selection_edge(t.accent))
            })
            .when(!chosen, |row| {
                row.hover(move |row| row.bg(t.selected).text_color(t.text))
            })
    }

    fn check(&self, chosen: bool) -> AnyElement {
        if chosen {
            icon("check")
                .size(px(13.))
                .text_color(self.theme.accent)
                .into_any_element()
        } else {
            div().into_any_element()
        }
    }

    /// Builds only the model, reasoning or access controls requested by its trigger.
    pub(super) fn model_options(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let runtime = self.selected_runtime();
        let mut content = div().flex().flex_col().flex_shrink_0().gap(px(2.));
        let preferences = self.composer_preferences();
        let task_id = self.view.as_ref().map(|view| view.task.id.clone());
        let disabled = self.creating_task
            || self.view.as_ref().is_some_and(|view| {
                view.task.state.active()
                    || view.task.state == RunState::DeliveryUnknown
                    || !view.outgoing.is_empty()
            });
        if self.panel == Panel::Models {
            content = content.child(self.account_options(&preferences, disabled, cx));
            let mut models = div().id("model-list").flex().flex_col().gap(px(1.)).child(
                div()
                    .px(px(8.))
                    .pt(px(10.))
                    .pb(px(4.))
                    .child(kit::section_header("Model", Some(runtime.models.len()), t)),
            );
            for (i, model) in runtime.models.iter().enumerate() {
                let chosen = preferences.model.as_deref() == Some(&model.id);
                let id = task_id.clone();
                let model_id = model.id.clone();
                let effort = model.default_effort.clone();
                models = models.child(
                    self.option_row(("model-option", i), chosen, disabled)
                        .child(icon(preferences.provider.icon()).size(px(13.)))
                        .child(
                            div()
                                .flex_shrink_0()
                                .max_w(relative(0.6))
                                .text_ellipsis()
                                .child(model.name.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .font_family(crate::theme::mono())
                                .text_size(text_px(10.5))
                                .text_color(t.subtle)
                                .child(if model.name == model.id {
                                    String::new()
                                } else {
                                    model.id.clone()
                                }),
                        )
                        .when(model.images, |row| {
                            row.child(icon("eye").size(px(12.)).text_color(t.subtle))
                        })
                        .child(self.check(chosen))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.choose_model(
                                id.clone(),
                                Some(model_id.clone()),
                                (!effort.is_empty()).then(|| effort.clone()),
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
                    "{}:{}:{}:{:?}",
                    preferences.provider.label(),
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
                content = content
                    .child(div().px(px(8.)).pt(px(6.)).child(kit::section_header(
                        "Reasoning effort",
                        None,
                        t,
                    )))
                    .child(div().px(px(8.)).py(px(6.)).child(slider.clone()));
            } else {
                content = content.child(
                    div()
                        .p_2()
                        .text_size(text_px(11.5))
                        .text_color(t.muted)
                        .child("Choose a model first to see its supported reasoning levels."),
                );
            }
        }
        let plan = plan_mode(&runtime);
        if self.panel == Panel::Mode {
            content = content.child(
                div()
                    .px(px(8.))
                    .pt(px(6.))
                    .pb(px(4.))
                    .child(kit::section_header("Mode", None, t)),
            );
            let planning = self.planning();
            for (index, (plan_row, glyph, name, detail)) in [
                (
                    false,
                    "zap",
                    "Build",
                    "The agent edits files and runs commands under the approval rule you chose.",
                ),
                (
                    true,
                    "list-checks",
                    "Plan",
                    "The agent explores and proposes a plan. It changes nothing until you approve the plan.",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                content = content.child(
                    self.access_row(
                        ("mode-option", index),
                        glyph,
                        name.to_owned(),
                        detail.to_owned(),
                        planning == plan_row,
                        disabled || plan.is_none(),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| s.set_planning(plan_row, cx))),
                );
            }
            content = content.child(
                div()
                    .px(px(8.))
                    .pt(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(kit::keycap("⇧⇥", t))
                    .child(caps("Switches between build and plan", t)),
            );
        }
        if self.panel == Panel::Access {
            content = content.child(div().px(px(8.)).pt(px(6.)).pb(px(4.)).child(
                kit::section_header(
                    if preferences.provider == Provider::Codex {
                        "Access"
                    } else {
                        "Approvals"
                    },
                    None,
                    t,
                ),
            ));
        }
        if self.panel == Panel::Access && preferences.provider == Provider::Codex {
            for (i, access) in CODEX_ACCESS.into_iter().enumerate() {
                let chosen = preferences.access == access;
                content = content.child(
                    self.access_row(
                        ("access-option", i),
                        access_glyph(access.label()),
                        access.label().to_owned(),
                        codex_access_detail(access).to_owned(),
                        chosen,
                        disabled,
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.set_access(access, cx);
                    })),
                );
            }
        }
        if self.panel == Panel::Access && preferences.provider != Provider::Codex {
            // Plan mode is a mode of work, chosen in its own menu. This menu holds only the
            // rules for what the agent may do without asking.
            let approval = self.approval_choice();
            for (index, mode) in runtime
                .access_modes
                .iter()
                .filter(|mode| !ProviderAccess::plans(&mode.id))
                .enumerate()
            {
                let mode_id = mode.id.clone();
                let chosen = approval.as_ref() == Some(&mode.id);
                content = content.child(
                    self.access_row(
                        ("provider-access", index),
                        access_glyph(&mode.id),
                        mode.name.clone(),
                        mode.description.clone(),
                        chosen,
                        disabled,
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.set_approval(mode_id.clone(), cx);
                    })),
                );
            }
            if self.planning() {
                content = content.child(
                    div()
                        .px(px(8.))
                        .py(px(4.))
                        .text_size(text_px(11.))
                        .text_color(t.subtle)
                        .child("Plan mode is on. This rule applies when the task returns to build mode."),
                );
            }
            if runtime.access_modes.is_empty() {
                content = content.child(
                    div()
                        .p_2()
                        .text_size(text_px(11.5))
                        .text_color(t.muted)
                        .child("Refresh the provider to load its access modes."),
                );
            }
        }
        if self.panel == Panel::Access && preferences.provider == Provider::Codex {
            content = content.child(
                div()
                    .px(px(8.))
                    .pt(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(kit::keycap("⇧⇥", t))
                    .child(caps("Cycles the modes that still ask", t)),
            );
        }
        if disabled {
            content = content.child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(text_px(11.))
                    .text_color(t.muted)
                    .child("These controls unlock after pending work finishes and delivery is confirmed."),
            );
        }
        if self.panel == Panel::Models && runtime.models.is_empty() {
            content = content.child(
                div()
                    .px_2()
                    .py_1()
                    .child(kit::loader_row("Reading the provider's model catalog", t)),
            );
        }
        if self.panel == Panel::Models {
            let mut rail = div()
                .w(px(132.))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .pr(px(4.))
                .child(
                    div()
                        .px(px(8.))
                        .pt(px(10.))
                        .pb(px(4.))
                        .child(kit::section_header("Provider", None, t)),
                );
            for (index, provider) in Provider::ALL.into_iter().enumerate() {
                let chosen = preferences.provider == provider;
                rail = rail.child(
                    self.option_row(("model-provider", index), chosen, disabled)
                        .child(icon(provider.icon()).size(px(13.)))
                        .child(provider.label())
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.choose_provider(provider, cx);
                        })),
                );
            }
            if let Some(error) = runtime.error {
                content = content.child(
                    div()
                        .p_2()
                        .text_size(text_px(11.))
                        .text_color(t.removed)
                        .child(error),
                );
            }
            content = content.child(self.agent_footer(&preferences, cx));
            return div()
                .flex()
                .items_start()
                .child(rail)
                .child(
                    content
                        .flex_1()
                        .min_w_0()
                        .pl(px(4.))
                        .border_l_1()
                        .border_color(t.border),
                )
                .into_any_element();
        }
        content.into_any_element()
    }

    // The accounts of the composer's provider: identity, plan and the fullest limit window.
    fn account_options(
        &self,
        preferences: &AgentPreferences,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let provider = preferences.provider;
        let current = preferences.account.as_deref().unwrap_or(DEFAULT_ACCOUNT);
        let accounts: Vec<AccountState> = self
            .workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .accounts
                    .iter()
                    .filter(|state| state.account.provider == provider)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut list = div().flex().flex_col().gap(px(1.)).child(
            div()
                .px(px(8.))
                .pt(px(10.))
                .pb(px(4.))
                .child(kit::section_header("Account", Some(accounts.len()), t)),
        );
        let limited = self.account_limit_locked();
        let locked = self.account_locked() || limited;
        for (index, state) in accounts.iter().enumerate() {
            let chosen = state.account.id == current;
            let id = (!state.account.is_default()).then(|| state.account.id.clone());
            let usage = state.status.usage.as_ref();
            let fraction = usage.and_then(usage::highest_usage);
            list = list.child(
                self.option_row(
                    ("account-option", index),
                    chosen,
                    disabled || (locked && !chosen),
                )
                .child(
                    div()
                        .size(px(7.))
                        .rounded(px(2.))
                        .flex_shrink_0()
                        .bg(self.account_tone(&state.account)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w(relative(0.42))
                        .text_ellipsis()
                        .child(state.account.name.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .font_family(crate::theme::mono())
                        .text_size(text_px(10.5))
                        .text_color(t.subtle)
                        .map(|identity| match state.status.email.as_deref() {
                            Some(email) => identity.child(self.censored_email(
                                provider,
                                &state.account.id,
                                email,
                                cx,
                            )),
                            None => identity
                                .child(state.status.organization.clone().unwrap_or_default()),
                        }),
                )
                .map(|row| match state.status.signed_in {
                    Some(false) => row.child(kit::chip("SIGNED OUT", t.removed)),
                    _ if usage.is_some_and(usage::limit_reached) => {
                        row.child(kit::chip("LIMIT", t.removed))
                    }
                    _ => row
                        .when_some(state.status.plan.as_ref(), |row, plan| {
                            row.child(kit::chip(usage::plan_name(plan).to_uppercase(), t.muted))
                        })
                        .when_some(fraction, |row, fraction| {
                            row.child(kit::meter(
                                Some(fraction),
                                kit::usage_tone(fraction, t),
                                30.,
                                t,
                            ))
                        }),
                })
                .child(self.check(chosen))
                .on_click(cx.listener(move |s, _, _, cx| s.choose_account(id.clone(), cx))),
            );
        }
        if locked {
            list = list.child(
                div()
                    .px(px(8.))
                    .py(px(4.))
                    .text_size(text_px(11.))
                    .text_color(t.subtle)
                    .child(if limited {
                        "This task's account reached its usage limit. Moving the conversation to another account would get around the limit, which the provider's terms do not allow. Wait for the reset."
                    } else {
                        "Codex keeps a started conversation on its account. Start a new task to use another one."
                    }),
            );
        }
        if provider.home_variable().is_some() {
            list = list.child(
                self.option_row("account-manage", false, false)
                    .text_color(t.subtle)
                    .child(icon("plus").size(px(13.)))
                    .child("Add or manage accounts…")
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.accounts.provider = provider;
                        s.open_settings(settings::ACCOUNTS);
                        cx.notify();
                    })),
            );
        }
        list.into_any_element()
    }

    // Saves or clears the composer's choices as the starting point of new tasks in the project.
    fn agent_footer(&self, preferences: &AgentPreferences, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut footer = div()
            .mt(px(8.))
            .pt(px(6.))
            .px(px(4.))
            .flex()
            .items_center()
            .gap(px(6.));
        if let Some(project) = self.composer_project() {
            let pinned = project.defaults.as_ref() == Some(preferences);
            let has_default = project.defaults.is_some();
            let project_id = project.id.clone();
            let name = project.name.clone();
            let defaults = preferences.clone();
            footer = footer.child(
                self.ghost(
                    "project-default",
                    if pinned { "star-fill" } else { "star" },
                    if pinned {
                        format!("Default for {name}")
                    } else {
                        format!("Make default for {name}")
                    },
                )
                .min_w_0()
                .when(pinned, |b| b.text_color(t.accent))
                .on_click(cx.listener(move |s, _, _, cx| {
                    let defaults = (!pinned).then(|| defaults.clone());
                    s.set_project_defaults(project_id.clone(), defaults, cx);
                })),
            );
            if has_default && !pinned {
                let project_id = project.id.clone();
                footer = footer.child(
                    self.icon_button("project-default-clear", "undo-2", "Clear project default")
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.set_project_defaults(project_id.clone(), None, cx);
                        })),
                );
            }
        }
        footer
            .child(div().flex_1())
            .child(
                self.icon_button("refresh-models", "refresh", "Refresh models and accounts")
                    .on_click(cx.listener(|s, _, _, _| s.connect_runtime())),
            )
            .into_any_element()
    }

    // An access mode with its consequence spelled out beneath its name.
    fn access_row(
        &self,
        id: impl Into<ElementId>,
        glyph: &'static str,
        name: String,
        detail: String,
        chosen: bool,
        disabled: bool,
    ) -> Button {
        let t = self.theme;
        self.option_row(id, chosen, disabled)
            .h_auto()
            .min_h(px(44.))
            .py(px(6.))
            .items_start()
            .child(
                div()
                    .pt(px(1.))
                    .child(icon(glyph).size(px(14.)).text_color(if chosen {
                        t.accent
                    } else {
                        t.muted
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(3.))
                    .child(
                        div()
                            .line_height(relative(1.2))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.text)
                            .child(name),
                    )
                    .when(!detail.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(text_px(11.))
                                .line_height(relative(1.3))
                                .text_color(t.muted)
                                .child(detail),
                        )
                    }),
            )
            .child(self.check(chosen))
    }

    /// Returns current task options, or for a new task the draft's choices, then the project's
    /// default, then the global default.
    pub(super) fn composer_preferences(&self) -> AgentPreferences {
        if let Some(view) = &self.view {
            // A task without a saved model runs the provider's default model, so show that one.
            return self.with_model(
                AgentPreferences {
                    provider: view.task.provider,
                    account: view.task.account.clone(),
                    provider_access: view.task.provider_access.clone(),
                    model: view.task.model.clone(),
                    effort: view.task.effort.clone(),
                    access: view.task.access,
                },
                None,
            );
        }
        self.starting_preferences(self.composer_project(), self.new_preferences.clone())
    }

    /// Returns the options that a new task in `project` starts with: `chosen` when the user
    /// picked options for this draft, then the project's default, then the options used last.
    pub(super) fn starting_preferences(
        &self,
        project: Option<&Project>,
        chosen: Option<AgentPreferences>,
    ) -> AgentPreferences {
        if let Some(chosen) = chosen.or_else(|| project?.defaults.clone()) {
            return self.with_access(
                self.with_model(chosen, Some(project.map(|project| project.id.as_str()))),
            );
        }
        // Nothing was chosen for this draft or pinned to this project: start from the last used
        // provider and account, with the model this account used last in this project.
        let mut preferences = self
            .workspace
            .as_ref()
            .map(|w| w.settings.agent_defaults.clone())
            .unwrap_or_default();
        let project = project.map(|project| project.id.as_str());
        if let Some(choice) = self.workspace.as_ref().and_then(|workspace| {
            workspace.settings.model_choice(
                preferences.provider,
                preferences.account.as_deref(),
                project,
            )
        }) {
            preferences.model = choice.model.clone();
            preferences.effort = choice.effort.clone();
        }
        self.with_access(self.with_model(preferences, Some(project)))
    }

    /// Applies the approval rule that was chosen last for the model of `preferences`, in any
    /// workspace or project. Plan mode stays: it is a mode of work, and the approval rule
    /// returns when the task leaves it.
    pub(super) fn with_access(&self, mut preferences: AgentPreferences) -> AgentPreferences {
        if preferences
            .provider_access
            .as_deref()
            .is_some_and(ProviderAccess::plans)
        {
            return preferences;
        }
        if let Some(choice) = self.workspace.as_ref().and_then(|workspace| {
            workspace
                .settings
                .access_choice(preferences.provider, preferences.model.as_deref())
        }) {
            preferences.provider_access = choice.provider_access.clone();
            preferences.access = choice.access;
        }
        preferences
    }

    // Replaces a missing or retired model with a concrete one, so no menu ever shows a
    // "provider default". `remembered` holds the project of a new task: its account's last
    // model there, then anywhere, comes before the provider's default model.
    fn with_model(
        &self,
        mut preferences: AgentPreferences,
        remembered: Option<Option<&str>>,
    ) -> AgentPreferences {
        let Some(workspace) = &self.workspace else {
            return preferences;
        };
        let runtime = workspace.runtimes.get(&preferences.provider);
        let known = |model: &str| {
            runtime.is_none_or(|runtime| {
                runtime.models.is_empty() || runtime.models.iter().any(|m| m.id == model)
            })
        };
        if preferences.model.as_deref().is_some_and(known) {
            return preferences;
        }
        if let Some(project) = remembered
            && let Some(choice) = workspace
                .settings
                .model_choice(
                    preferences.provider,
                    preferences.account.as_deref(),
                    project,
                )
                .filter(|choice| choice.model.as_deref().is_some_and(known))
        {
            preferences.model = choice.model.clone();
            preferences.effort = choice.effort.clone();
            return preferences;
        }
        if let Some(runtime) = runtime
            && let Some(default) = runtime.default_model.clone()
        {
            preferences.effort = runtime
                .models
                .iter()
                .find(|model| model.id == default)
                .map(|model| model.default_effort.clone())
                .filter(|effort| !effort.is_empty());
            preferences.model = Some(default);
        }
        preferences
    }

    /// Returns the display name of the model in `preferences`, or the provider's name while the
    /// model catalog has not been read.
    pub(super) fn model_name(&self, preferences: &AgentPreferences) -> String {
        let Some(id) = &preferences.model else {
            return preferences.provider.label().to_owned();
        };
        self.workspace
            .as_ref()
            .and_then(|workspace| workspace.runtimes.get(&preferences.provider))
            .and_then(|runtime| runtime.models.iter().find(|model| &model.id == id))
            .map_or_else(|| id.clone(), |model| model.name.clone())
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
            // The approval rule follows the model.
            self.save_agent_preferences(self.with_access(preferences));
        }
        cx.notify();
    }

    /// Reports a task whose provider cannot continue its conversation under another account.
    ///
    /// Codex keeps thread history in state that belongs to one configuration directory.
    pub(super) fn account_locked(&self) -> bool {
        self.view.as_ref().is_some_and(|view| {
            view.task.provider == Provider::Codex && view.task.provider_thread.is_some()
        })
    }

    /// Reports a started task whose account is at a usage limit.
    ///
    /// Both providers' terms forbid getting around usage limits, so the conversation stays on
    /// its account until the limit resets. The host refuses the move as well.
    pub(super) fn account_limit_locked(&self) -> bool {
        self.view
            .as_ref()
            .is_some_and(|view| view.task.provider_thread.is_some())
            && self
                .composer_account()
                .and_then(|state| state.status.usage.as_ref())
                .is_some_and(usage::limit_reached)
    }

    /// Moves the open task, or the next new task, to another account of its provider.
    pub(super) fn choose_account(&mut self, account: Option<String>, cx: &mut Context<Self>) {
        if self.composer_preferences().account == account {
            return;
        }
        if self.account_limit_locked() {
            self.report_error(
                "This task's account reached its usage limit. The conversation stays on it until the limit resets.",
                cx,
            );
            return;
        }
        if let Some(view) = &self.view {
            if view.task.state.active() || view.task.state == RunState::DeliveryUnknown {
                self.report_error("Change the account after this turn finishes.", cx);
                return;
            }
            self.send(core::Request::EditTask {
                task_id: view.task.id.clone(),
                edit: TaskEdit::Account(account.clone()),
            });
        } else {
            let mut preferences = self.composer_preferences();
            preferences.account = account.clone();
            // The other account continues with the model it used last, here or anywhere.
            preferences.model = None;
            preferences.effort = None;
            let project = self.composer_project().map(|project| project.id.clone());
            let preferences =
                self.with_access(self.with_model(preferences, Some(project.as_deref())));
            self.save_agent_preferences(preferences);
        }
        let provider = self.composer_preferences().provider;
        self.send(core::Request::ConnectRuntime { provider, account });
        cx.notify();
    }

    /// Sets or clears the options that new tasks in one project start with.
    pub(super) fn set_project_defaults(
        &mut self,
        project_id: String,
        defaults: Option<AgentPreferences>,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self
            .workspace
            .as_mut()
            .and_then(|w| w.projects.iter_mut().find(|p| p.id == project_id))
        {
            project.defaults = defaults.clone();
            let text = if defaults.is_some() {
                format!("New tasks in {} start with these options", project.name)
            } else {
                format!("{} follows the global default again", project.name)
            };
            let tone = self.theme.accent;
            self.notify_user(text, tone, cx);
        }
        self.send(core::Request::SetProjectDefaults {
            project_id,
            defaults,
        });
    }

    fn set_access(&mut self, access: AccessMode, cx: &mut Context<Self>) {
        if let Some(view) = &self.view {
            self.send(core::Request::EditTask {
                task_id: view.task.id.clone(),
                edit: TaskEdit::Access(access),
            });
        } else {
            let mut preferences = self.composer_preferences();
            preferences.access = access;
            self.save_access_choice(preferences);
        }
        cx.notify();
    }

    fn set_provider_access(&mut self, mode: String, cx: &mut Context<Self>) {
        if let Some(view) = &self.view {
            self.send(core::Request::EditTask {
                task_id: view.task.id.clone(),
                edit: TaskEdit::ProviderAccess(mode),
            });
        } else {
            let mut preferences = self.composer_preferences();
            preferences.provider_access = Some(mode);
            self.save_access_choice(preferences);
        }
        cx.notify();
    }

    /// Steps to the next access mode that still asks or only plans. Modes that remove approval
    /// are left to an explicit choice in the menu.
    pub(super) fn cycle_access(&mut self, cx: &mut Context<Self>) {
        if self.panel != Panel::None
            || self.view.as_ref().is_some_and(|view| {
                view.task.state.active() || view.task.state == RunState::DeliveryUnknown
            })
        {
            return;
        }
        let preferences = self.composer_preferences();
        let tone = self.theme.accent;
        if preferences.provider == Provider::Codex {
            let cycle = [
                AccessMode::ReadOnly,
                AccessMode::Workspace,
                AccessMode::AutoReview,
            ];
            let next = cycle
                .iter()
                .position(|mode| *mode == preferences.access)
                .map_or(cycle[1], |index| cycle[(index + 1) % cycle.len()]);
            self.set_access(next, cx);
            self.notify_user(format!("Access: {}", next.label()), tone, cx);
            return;
        }
        // A provider with a plan mode switches between build and plan; its approval rule has
        // its own menu. A review has no plan mode.
        if self.mode != TaskKind::Review && plan_mode(&self.selected_runtime()).is_some() {
            let planning = !self.planning();
            self.set_planning(planning, cx);
            self.notify_user(
                if planning {
                    "Mode: Plan"
                } else {
                    "Mode: Build"
                },
                tone,
                cx,
            );
        }
    }

    // Returns the key of the open task, or of the new-task draft, in `approval_memory`.
    fn approval_key(&self) -> String {
        self.view
            .as_ref()
            .map(|view| view.task.id.clone())
            .unwrap_or_default()
    }

    /// Reports that the composer's provider is in its plan mode.
    pub(super) fn planning(&self) -> bool {
        self.composer_preferences()
            .provider_access
            .as_deref()
            .is_some_and(ProviderAccess::plans)
    }

    /// Returns the approval rule of the composer: the provider's access mode, or while plan mode
    /// is on, the rule that returns with build mode.
    pub(super) fn approval_choice(&self) -> Option<String> {
        let runtime = self.selected_runtime();
        let known = |id: &String| runtime.access_modes.iter().any(|mode| &mode.id == id);
        let fallback = || {
            runtime
                .access_modes
                .iter()
                .filter(|mode| !ProviderAccess::plans(&mode.id))
                .find(|mode| mode.id == "default")
                .or_else(|| {
                    runtime
                        .access_modes
                        .iter()
                        .find(|mode| !ProviderAccess::plans(&mode.id))
                })
                .map(|mode| mode.id.clone())
        };
        let current = self.composer_preferences().provider_access;
        match current {
            Some(mode) if !ProviderAccess::plans(&mode) => Some(mode),
            Some(_) => self
                .approval_memory
                .get(&self.approval_key())
                .filter(|id| known(id))
                .cloned()
                .or_else(fallback),
            None => fallback(),
        }
    }

    /// Turns the provider's plan mode on or off. Leaving plan mode restores the approval rule
    /// that was chosen before or during it.
    pub(super) fn set_planning(&mut self, planning: bool, cx: &mut Context<Self>) {
        if planning == self.planning() {
            return;
        }
        let Some(approval) = self.approval_choice() else {
            return;
        };
        if planning {
            let Some(plan) = plan_mode(&self.selected_runtime()) else {
                return;
            };
            self.approval_memory.insert(self.approval_key(), approval);
            self.set_provider_access(plan.id, cx);
        } else {
            self.set_provider_access(approval, cx);
        }
    }

    // Chooses the approval rule. While plan mode is on it is only remembered, because the
    // provider has one mode at a time.
    fn set_approval(&mut self, mode: String, cx: &mut Context<Self>) {
        if self.planning() {
            self.approval_memory.insert(self.approval_key(), mode);
            cx.notify();
        } else {
            self.set_provider_access(mode, cx);
        }
    }

    /// Opens an agent menu from the keyboard, attached to the composer chip it belongs to.
    pub(super) fn open_agent_menu(
        &mut self,
        panel: Panel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let access = matches!(panel, Panel::Access | Panel::Mode);
        // A review has an approval rule and no mode of work to choose.
        if panel == Panel::Mode && self.mode == TaskKind::Review {
            return;
        }
        let anchor = if access { ACCESS_ANCHOR } else { AGENT_ANCHOR };
        let viewport = window.viewport_size();
        self.menu_anchor = Some(fx::anchor_bounds(anchor, cx).unwrap_or(Bounds::new(
            point(viewport.width / 2. + px(240.), viewport.height - px(140.)),
            Size::default(),
        )));
        self.panel = panel;
        if self.selected_runtime().models.is_empty() {
            self.connect_runtime();
        }
        cx.notify();
    }

    /// Asks the host to read the composer provider's catalog under the composer's account.
    pub(super) fn connect_runtime(&self) {
        let preferences = self.composer_preferences();
        self.send(core::Request::ConnectRuntime {
            provider: preferences.provider,
            account: preferences.account,
        });
    }

    /// Returns the account that the composer's next turn runs under, with its observed status.
    pub(super) fn composer_account(&self) -> Option<&AccountState> {
        let preferences = self.composer_preferences();
        self.workspace
            .as_ref()?
            .account(preferences.provider, preferences.account.as_deref())
    }

    /// Returns the selected provider's independent runtime snapshot.
    pub(super) fn selected_runtime(&self) -> RuntimeInfo {
        self.provider_runtime(self.composer_preferences().provider)
    }

    /// Returns cached capabilities for one provider, including disconnected state.
    pub(super) fn provider_runtime(&self, provider: Provider) -> RuntimeInfo {
        self.workspace
            .as_ref()
            .and_then(|w| w.runtimes.get(&provider))
            .cloned()
            .unwrap_or_default()
    }

    /// Returns saved options for a provider without reusing another provider's model.
    pub(super) fn provider_preferences(&self, provider: Provider) -> AgentPreferences {
        let preferences = self
            .workspace
            .as_ref()
            .and_then(|w| w.settings.provider_preferences.get(&provider))
            .cloned()
            .unwrap_or_else(|| {
                let defaults = self
                    .workspace
                    .as_ref()
                    .map(|w| w.settings.agent_defaults.clone())
                    .unwrap_or_default();
                if defaults.provider == provider {
                    defaults
                } else {
                    AgentPreferences {
                        provider,
                        ..AgentPreferences::default()
                    }
                }
            });
        self.with_access(self.with_model(preferences, Some(None)))
    }

    /// Persists draft and settings choices immediately, before the first message is sent.
    pub(super) fn save_agent_preferences(&mut self, preferences: AgentPreferences) {
        self.keep_preferences(&preferences);
        self.send(core::Request::SetAgentPreferences(preferences));
    }

    /// Persists a chosen approval rule with the other options, and makes it the rule of its
    /// model in every workspace and project.
    pub(super) fn save_access_choice(&mut self, preferences: AgentPreferences) {
        self.keep_preferences(&preferences);
        if let Some(workspace) = &mut self.workspace {
            workspace.settings.remember_access(&preferences);
        }
        self.send(core::Request::SetAccessChoice(preferences));
    }

    // The host records the same choice; this copy serves the composer until the next workspace
    // snapshot arrives.
    fn keep_preferences(&mut self, preferences: &AgentPreferences) {
        self.new_preferences = Some(preferences.clone());
        let project = self.composer_project().map(|project| project.id.clone());
        if let Some(workspace) = &mut self.workspace {
            workspace
                .settings
                .remember_model(preferences, project.as_deref());
            workspace.settings.agent_defaults = preferences.clone();
            workspace
                .settings
                .provider_preferences
                .insert(preferences.provider, preferences.clone());
        }
    }

    fn choose_provider(&mut self, provider: Provider, cx: &mut Context<Self>) {
        if self.composer_preferences().provider == provider {
            return;
        }
        let preferences = self.provider_preferences(provider);
        if let Some(view) = &self.view {
            if view.task.state.active()
                || view.task.state == RunState::DeliveryUnknown
                || !view.outgoing.is_empty()
            {
                return;
            }
            self.send(core::Request::EditTask {
                task_id: view.task.id.clone(),
                edit: TaskEdit::Provider(preferences.clone()),
            });
        } else {
            self.save_agent_preferences(preferences.clone());
        }
        self.effort_slider = None;
        self.send(core::Request::ConnectRuntime {
            provider,
            account: preferences.account,
        });
        cx.notify();
    }

    /// Reports whether the selected model accepts image attachments.
    pub(super) fn accepts_images(&self) -> bool {
        let preferences = self.composer_preferences();
        let runtime = self.selected_runtime();
        runtime
            .models
            .iter()
            .find(|model| preferences.model.as_ref() == Some(&model.id))
            .map_or(runtime.images, |model| model.images)
    }
}

const CODEX_ACCESS: [AccessMode; 4] = [
    AccessMode::ReadOnly,
    AccessMode::Workspace,
    AccessMode::AutoReview,
    AccessMode::FullAccess,
];

fn codex_access_detail(access: AccessMode) -> &'static str {
    match access {
        AccessMode::ReadOnly => "Inspect files without editing or network access",
        AccessMode::Workspace => "Edit the checkout; ask for broader permissions",
        AccessMode::AutoReview => "AI reviews permission requests and can deny unsafe actions",
        AccessMode::FullAccess => "Unrestricted files and network; no approval prompts",
    }
}

/// Returns the provider's plan mode, when it has one.
pub(super) fn plan_mode(runtime: &RuntimeInfo) -> Option<ProviderAccess> {
    runtime
        .access_modes
        .iter()
        .find(|mode| ProviderAccess::plans(&mode.id))
        .cloned()
}

// Reports a provider mode that stops asking before it acts.
fn removes_approval(mode: &str) -> bool {
    let mode = mode.to_ascii_lowercase();
    mode.contains("bypass") || mode.contains("dontask") || mode.contains("full")
}

/// Returns the icon of an access mode from its name: open for unrestricted modes, a list for
/// planning, a bot for automatic review, a shield for everything that asks.
pub(super) fn access_glyph(mode: &str) -> &'static str {
    let mode = mode.to_ascii_lowercase();
    if removes_approval(&mode) {
        "lock-open"
    } else if mode.contains("plan") {
        "list-checks"
    } else if mode.contains("auto") || mode.contains("ai ") {
        "bot"
    } else if mode.contains("edit") {
        "pencil"
    } else {
        "shield"
    }
}
