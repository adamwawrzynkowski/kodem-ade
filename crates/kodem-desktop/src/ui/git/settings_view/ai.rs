//! The AI page: the provider that runs AI features, its models and account, and the switches
//! and prompts of commit messages, code review, diff explanations and pull request reviews.
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Div, Entity, FontWeight, div};
use kodem_git::ai;
use kodem_git::settings::AiProvider;

use super::{
    GitSettingsView, list_card, page, section, segmented, setting_row, status_note, toggle_row,
    wide_setting_row,
};
use crate::theme::{px, text_px};
use crate::ui::git::components::{button, dither_icon_tile, mono_font, section_header, text_field};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, Disableable as _, DropdownMenu as _, PopupMenuItem,
    Selectable as _, Sizable as _, Textarea, TextareaState, h_flex, v_flex,
};
use crate::ui::git::settings::GitSettings;

/// Reasoning efforts offered when the chosen Codex model does not list its own.
const DEFAULT_CODEX_EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];

/// Called with the name of the model that the user picks.
type ModelChoice = Rc<dyn Fn(&str, &mut App)>;

impl GitSettingsView {
    /// Checks the AI provider: the models of the Ollama server, or the account and models of
    /// Codex.
    pub(super) fn refresh_ai_status(&mut self, cx: &mut Context<Self>) {
        let settings = GitSettings::get(cx).clone();
        self.ai_loading = true;
        self.ai_status = Some("Checking...".into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match settings.ai_provider {
                        AiProvider::Ollama => {
                            ai::ollama_models(&settings.ollama_base_url).map(|models| {
                                (
                                    format!("Connected · {} models", models.len()),
                                    models,
                                    Vec::new(),
                                )
                            })
                        }
                        AiProvider::Codex => {
                            ai::codex_info().map(|info| (info.account, Vec::new(), info.models))
                        }
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.ai_loading = false;
                match result {
                    Ok((status, models, codex_models)) => {
                        this.ai_status = Some(status);
                        this.ai_models = models;
                        this.codex_models = codex_models;
                    }
                    Err(error) => {
                        this.ai_status = Some(error.to_string());
                        this.ai_models.clear();
                        this.codex_models.clear();
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // Asks the Ollama server to download a model.
    fn install_ollama_model(&mut self, model: String, cx: &mut Context<Self>) {
        let base_url = GitSettings::get(cx).ollama_base_url.clone();
        self.ai_loading = true;
        self.ai_status = Some(format!("Installing {model}..."));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let model_for_task = model.clone();
            let result = cx
                .background_executor()
                .spawn(async move { ai::pull_ollama_model(&base_url, &model_for_task) })
                .await;
            this.update(cx, |this, cx| {
                this.ai_loading = false;
                this.ai_status = Some(match result {
                    Ok(()) => format!("Installed {model}. Select Refresh to load the model list."),
                    Err(error) => error.to_string(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // Runs the Codex sign-in.
    fn sign_in_codex(&mut self, cx: &mut Context<Self>) {
        self.ai_loading = true;
        self.ai_status = Some("Waiting for Codex sign-in...".into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { ai::codex_login() })
                .await;
            this.update(cx, |this, cx| {
                this.ai_loading = false;
                this.ai_status = Some(result.unwrap_or_else(|error| error.to_string()));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // Signs out of Codex.
    fn sign_out_codex(&mut self, cx: &mut Context<Self>) {
        self.ai_loading = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { ai::codex_logout() })
                .await;
            this.update(cx, |this, cx| {
                this.ai_loading = false;
                this.ai_status = Some(match result {
                    Ok(()) => "Signed out of Codex.".into(),
                    Err(error) => error.to_string(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Returns the AI page.
    pub(super) fn render_ai(&mut self, cx: &mut Context<Self>) -> Div {
        let s = GitSettings::get(cx).clone();
        let loading = self.ai_loading;
        let ollama = s.ai_provider == AiProvider::Ollama;
        let codex = s.ai_provider == AiProvider::Codex;

        let mut provider = section("Provider", cx).child(setting_row(
            "Backend",
            "The service that runs AI features",
            segmented(
                "ai-provider",
                AiProvider::ALL.map(|provider| (provider, provider.label())),
                &s.ai_provider,
                cx.listener(|this, provider: &AiProvider, _, cx| {
                    let provider = *provider;
                    GitSettings::update(cx, |settings| settings.ai_provider = provider);
                    this.ai_status = None;
                    this.ai_models.clear();
                    this.codex_models.clear();
                    this.refresh_ai_status(cx);
                }),
                cx,
            ),
            cx,
        ));
        if ollama {
            provider = provider.child(setting_row(
                "Ollama server URL",
                "Kodem sends diffs to this local endpoint",
                div().w(px(280.)).child(text_field(&self.ollama_url)),
                cx,
            ));
        }
        if codex {
            provider = provider
                .child(setting_row(
                    "Codex model",
                    "Select one of the models available to this account",
                    self.codex_model_picker(&s.codex_model, loading),
                    cx,
                ))
                .child(setting_row(
                    "Reasoning effort",
                    "The effort that Codex uses for all AI actions",
                    segmented(
                        "codex-effort",
                        self.codex_efforts(&s.codex_model)
                            .into_iter()
                            .map(|effort| (effort.clone(), effort)),
                        &s.codex_reasoning_effort,
                        |effort: &String, _, cx| {
                            let effort = effort.clone();
                            GitSettings::update(cx, |s| s.codex_reasoning_effort = effort);
                        },
                        cx,
                    ),
                    cx,
                ));
        }
        provider = provider
            .child(setting_row(
                "Connection",
                "Check the provider and account state",
                self.connection_actions(codex, loading, cx),
                cx,
            ))
            .when_some(self.ai_status.clone(), |this, status| {
                this.child(
                    div()
                        .pt(px(8.))
                        .child(status_note(status, cx.theme().primary)),
                )
            })
            .when(ollama && !self.ai_models.is_empty(), |this| {
                this.child(self.installed_models(cx))
            })
            .when(codex && !self.codex_models.is_empty(), |this| {
                this.child(self.codex_model_cards(&s.codex_model, cx))
            });

        page()
            .child(provider)
            .child(
                section("Commit Message Generation", cx)
                    .child(toggle_row(
                        "ai-commit",
                        "AI commit messages",
                        "Generate a commit title from staged changes",
                        s.ai_commit_messages,
                        |v, cx| GitSettings::update(cx, |s| s.ai_commit_messages = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "ai-description",
                        "Generate description",
                        "Enable the Generate description AI action",
                        s.ai_generate_description,
                        |v, cx| GitSettings::update(cx, |s| s.ai_generate_description = v),
                        cx,
                    ))
                    .child(toggle_row(
                        "ai-whole-file",
                        "Include complete files",
                        "Give the model more context from staged file contents",
                        s.ai_include_whole_file,
                        |v, cx| GitSettings::update(cx, |s| s.ai_include_whole_file = v),
                        cx,
                    ))
                    .when(ollama, |this| {
                        this.child(self.model_row(
                            "Ollama model for commit messages",
                            ("commit-model", "install-commit-model"),
                            &s.ai_commit_model,
                            Rc::new(|model, cx| {
                                let model = model.to_string();
                                GitSettings::update(cx, |s| s.ai_commit_model = model);
                            }),
                            cx,
                        ))
                    })
                    .child(prompt_row(&self.commit_prompt, cx)),
            )
            .child(
                section("Code Review", cx)
                    .child(toggle_row(
                        "ai-review",
                        "AI code review",
                        "Review staged changes before commit",
                        s.ai_code_review,
                        |v, cx| GitSettings::update(cx, |s| s.ai_code_review = v),
                        cx,
                    ))
                    .when(ollama, |this| {
                        this.child(self.model_row(
                            "Ollama model for code review",
                            ("review-model", "install-review-model"),
                            &s.ai_review_model,
                            Rc::new(|model, cx| {
                                let model = model.to_string();
                                GitSettings::update(cx, |s| s.ai_review_model = model);
                            }),
                            cx,
                        ))
                    })
                    .child(prompt_row(&self.review_prompt, cx)),
            )
            .child(
                section("Diff Explanation", cx)
                    .child(toggle_row(
                        "ai-explain",
                        "AI diff explanations",
                        "Explain a selected change or commit",
                        s.ai_explain_diffs,
                        |v, cx| GitSettings::update(cx, |s| s.ai_explain_diffs = v),
                        cx,
                    ))
                    .when(ollama, |this| {
                        this.child(self.model_row(
                            "Ollama model for explanations",
                            ("explain-model", "install-explain-model"),
                            &s.ai_explain_model,
                            Rc::new(|model, cx| {
                                let model = model.to_string();
                                GitSettings::update(cx, |s| s.ai_explain_model = model);
                            }),
                            cx,
                        ))
                    })
                    .child(prompt_row(&self.explain_prompt, cx)),
            )
            .child(
                section("Pull Request Review", cx)
                    .child(toggle_row(
                        "ai-pr-review",
                        "AI pull request review",
                        "Review a GitHub pull request diff",
                        s.ai_pull_request_review,
                        |v, cx| GitSettings::update(cx, |s| s.ai_pull_request_review = v),
                        cx,
                    ))
                    .child(prompt_row(&self.pull_request_review_prompt, cx)),
            )
    }

    // Returns the efforts of the chosen Codex model, or the usual efforts while its models are
    // not loaded.
    fn codex_efforts(&self, chosen: &str) -> Vec<String> {
        self.codex_models
            .iter()
            .find(|model| model.model == chosen || (chosen.is_empty() && model.is_default))
            .map(|model| {
                model
                    .supported_reasoning_efforts
                    .iter()
                    .map(|effort| effort.reasoning_effort.clone())
                    .collect::<Vec<_>>()
            })
            .filter(|efforts| !efforts.is_empty())
            .unwrap_or_else(|| DEFAULT_CODEX_EFFORTS.map(str::to_string).to_vec())
    }

    // Returns the picker of the Codex model. Without a saved choice it shows the default model
    // of the account. Picking a model also picks its default effort.
    fn codex_model_picker(&self, chosen: &str, loading: bool) -> AnyElement {
        let selection = if chosen.is_empty() {
            self.codex_models
                .iter()
                .find(|model| model.is_default)
                .map(|model| model.model.as_str())
                .unwrap_or_default()
        } else {
            chosen
        };
        let defaults: Vec<(String, String)> = self
            .codex_models
            .iter()
            .map(|model| (model.model.clone(), model.default_reasoning_effort.clone()))
            .collect();
        let models = defaults.iter().map(|(model, _)| model.clone()).collect();
        model_picker(
            "codex-model",
            selection,
            models,
            loading,
            Rc::new(move |model, cx| {
                let default_effort = defaults
                    .iter()
                    .find(|(name, _)| name == model)
                    .map(|(_, effort)| effort.clone());
                let model = model.to_string();
                GitSettings::update(cx, |s| {
                    s.codex_model = model;
                    if let Some(default_effort) = default_effort {
                        s.codex_reasoning_effort = default_effort;
                    }
                });
            }),
        )
    }

    // Returns the Sign In and Sign Out buttons of Codex and the Refresh button.
    fn connection_actions(&self, codex: bool, loading: bool, cx: &mut Context<Self>) -> Div {
        h_flex()
            .gap_2()
            .when(codex, |this| {
                this.child(
                    button("codex-sign-in")
                        .outline()
                        .small()
                        .label("Sign In")
                        .disabled(loading)
                        .on_click(cx.listener(|this, _, _, cx| this.sign_in_codex(cx))),
                )
                .child(
                    button("codex-sign-out")
                        .ghost()
                        .small()
                        .label("Sign Out")
                        .disabled(loading)
                        .on_click(cx.listener(|this, _, _, cx| this.sign_out_codex(cx))),
                )
            })
            .child(
                button("refresh-ai-status")
                    .outline()
                    .small()
                    .icon(icons::refresh())
                    .label("Refresh")
                    .disabled(loading)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_ai_status(cx))),
            )
    }

    // Returns the models of the Ollama server, each with buttons that use it for commit
    // messages, reviews or explanations.
    fn installed_models(&self, cx: &App) -> Div {
        v_flex()
            .w_full()
            .gap_1()
            .py_2()
            .child(div().pt_2().pb_1().child(section_header(
                "Installed models",
                Some(self.ai_models.len()),
                cx,
            )))
            .children(self.ai_models.iter().enumerate().map(|(ix, model)| {
                let commit_name = model.name.clone();
                let review_name = model.name.clone();
                let explain_name = model.name.clone();
                list_card(false, cx)
                    .gap_1()
                    .py_1p5()
                    .child(dither_icon_tile(
                        icons::sparkles(),
                        cx.theme().primary,
                        24.0,
                        false,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pl_1()
                            .font_family(mono_font())
                            .text_size(text_px(12.))
                            .child(model.name.clone()),
                    )
                    .child(
                        div()
                            .pr_1()
                            .font_family(mono_font())
                            .text_size(text_px(11.))
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{:.1} GB", model.size as f64 / 1_000_000_000.0)),
                    )
                    .child(
                        button(("use-commit-model", ix))
                            .outline()
                            .xsmall()
                            .label("Commit")
                            .on_click(move |_, _, cx| {
                                let model = commit_name.clone();
                                GitSettings::update(cx, |s| s.ai_commit_model = model);
                            }),
                    )
                    .child(
                        button(("use-review-model", ix))
                            .outline()
                            .xsmall()
                            .label("Review")
                            .on_click(move |_, _, cx| {
                                let model = review_name.clone();
                                GitSettings::update(cx, |s| s.ai_review_model = model);
                            }),
                    )
                    .child(
                        button(("use-explain-model", ix))
                            .outline()
                            .xsmall()
                            .label("Explain")
                            .on_click(move |_, _, cx| {
                                let model = explain_name.clone();
                                GitSettings::update(cx, |s| s.ai_explain_model = model);
                            }),
                    )
            }))
    }

    // Returns a card for each Codex model of the account. The chosen model is marked.
    fn codex_model_cards(&self, chosen: &str, cx: &App) -> Div {
        v_flex()
            .w_full()
            .gap_1()
            .py_2()
            .child(div().pt_2().pb_1().child(section_header(
                "Available Codex models",
                Some(self.codex_models.len()),
                cx,
            )))
            .children(self.codex_models.iter().enumerate().map(|(ix, model)| {
                let model_name = model.model.clone();
                let default_effort = model.default_reasoning_effort.clone();
                let selected = chosen == model.model || (chosen.is_empty() && model.is_default);
                list_card(selected, cx)
                    .child(dither_icon_tile(
                        icons::sparkles(),
                        if selected {
                            cx.theme().primary
                        } else {
                            cx.theme().muted_foreground
                        },
                        28.0,
                        false,
                    ))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(text_px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(model.display_name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(text_px(11.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(model.description.clone()),
                            ),
                    )
                    .child(
                        button(("select-codex-model", ix))
                            .outline()
                            .xsmall()
                            .label(if selected { "Selected" } else { "Use" })
                            .selected(selected)
                            .on_click(move |_, _, cx| {
                                let model = model_name.clone();
                                let effort = default_effort.clone();
                                GitSettings::update(cx, |s| {
                                    s.codex_model = model;
                                    s.codex_reasoning_effort = effort;
                                });
                            }),
                    )
            }))
    }

    // Returns the Model row of an Ollama feature: the model picker and a button that installs
    // the chosen model.
    fn model_row(
        &self,
        description: &'static str,
        (picker_id, install_id): (&'static str, &'static str),
        chosen: &str,
        on_select: ModelChoice,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let loading = self.ai_loading;
        let model = chosen.to_string();
        let models = self
            .ai_models
            .iter()
            .map(|model| model.name.clone())
            .collect();
        setting_row(
            "Model",
            description,
            h_flex()
                .gap_2()
                .child(model_picker(picker_id, chosen, models, loading, on_select))
                .child(
                    button(install_id)
                        .outline()
                        .small()
                        .label("Install")
                        .disabled(loading)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.install_ollama_model(model.clone(), cx)
                        })),
                ),
            cx,
        )
    }
}

// Returns a model picker: a menu button that shows the chosen model and lists the models to
// choose from. The chosen model stays in the list while the models are not loaded.
fn model_picker(
    id: &'static str,
    chosen: &str,
    mut models: Vec<String>,
    loading: bool,
    on_select: ModelChoice,
) -> AnyElement {
    if !chosen.is_empty() && !models.iter().any(|model| model == chosen) {
        models.push(chosen.to_string());
    }
    let chosen = chosen.to_string();
    let label = if chosen.is_empty() {
        "Load models to select one".to_string()
    } else {
        chosen.clone()
    };
    button(id)
        .small()
        .outline()
        .dropdown_caret(true)
        .label(label)
        .disabled(loading)
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu.min_w(px(220.)).max_h(px(320.));
            if models.is_empty() {
                return menu.label("Select Refresh to load the models");
            }
            for model in &models {
                let on_select = on_select.clone();
                let name = model.clone();
                menu = menu.item(
                    PopupMenuItem::new(model.clone())
                        .checked(*model == chosen)
                        .on_click(move |_, _, cx| on_select(&name, cx)),
                );
            }
            menu
        })
        .into_any_element()
}

// Returns the custom prompt row of an AI feature.
fn prompt_row(state: &Entity<TextareaState>, cx: &App) -> AnyElement {
    wide_setting_row(
        "Custom prompt",
        "Leave this empty to use the built-in prompt",
        Textarea::new(state).h(px(88.)),
        cx,
    )
}
