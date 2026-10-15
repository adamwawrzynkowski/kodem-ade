//! The setup dialog: what Kodem found for each provider and support tool on this computer, the
//! installers of those that are missing, and the choices that most users make before the first
//! task.
//!
//! The dialog opens once by itself and later from the command palette. Each control writes the
//! same setting as its row in Preferences, so the dialog stores nothing of its own.
use super::*;

/// Longest part of a provider's failure report that a row shows, in characters.
const FAILURE_LIMIT: usize = 240;

/// What a row says while a tool's installer runs in its terminal window.
const INSTALLER_RUNNING: &str =
    "The installer runs in a terminal window. Choose Check again when it finishes.";

/// Where the installers come from, below the tools.
#[cfg(target_os = "macos")]
const INSTALLER_NOTE: &str = "Installers open in Terminal. Claude Code and Codex come from their official sources, Git from Apple's Command Line Tools, and Git LFS and GitHub CLI from Homebrew.";
#[cfg(windows)]
const INSTALLER_NOTE: &str = "Installers open in a PowerShell window. Claude Code and Codex come from their official sources, and Git, Git LFS and GitHub CLI from winget.";
#[cfg(not(any(target_os = "macos", windows)))]
const INSTALLER_NOTE: &str = "Installers open in a terminal window. Claude Code and Codex come from their official sources, and Git, Git LFS and GitHub CLI from your distribution's package manager, which asks for your password.";

// What the setup found for one provider.
#[derive(Clone, Copy, PartialEq)]
enum Found {
    // The CLI did not answer yet.
    Checking,
    // The CLI did not start.
    Unavailable,
    // The CLI runs and no account is signed in.
    SignedOut,
    // The CLI runs and did not report a sign-in state yet.
    Installed,
    // The CLI runs and an account is signed in.
    Ready,
}

impl Shell {
    /// Opens the setup dialog and asks each provider's CLI for its catalog, which shows
    /// whether that CLI is installed, and the host for the versions of the support tools.
    pub(super) fn open_setup(&mut self) {
        self.panel = Panel::Setup;
        self.check_providers();
        self.send(core::Request::InspectTools);
    }

    // Asks the host to start the CLI of each provider. The result arrives with the workspace.
    fn check_providers(&self) {
        for provider in Provider::ALL {
            self.send(core::Request::ConnectRuntime {
                provider,
                account: None,
            });
        }
    }

    // Reads each provider again: its CLI, and the sign-in state of each account that does not
    // wait for a sign-in.
    fn recheck_providers(&self) {
        self.check_providers();
        self.send(core::Request::InspectTools);
        let Some(workspace) = &self.workspace else {
            return;
        };
        for state in &workspace.accounts {
            if !state.status.signing_in {
                self.send(core::Request::RefreshAccount {
                    provider: state.account.provider,
                    id: state.account.id.clone(),
                });
            }
        }
    }

    // Derives the state of a provider from its runtime and from the sign-in state of its
    // accounts. A sign-in report comes from the CLI, so a signed-out provider is installed.
    fn provider_found(&self, provider: Provider, runtime: &RuntimeInfo) -> Found {
        match (
            self.provider_signed_in(provider),
            &runtime.version,
            &runtime.error,
        ) {
            (Some(false), _, _) => Found::SignedOut,
            (_, _, Some(_)) => Found::Unavailable,
            (_, None, None) => Found::Checking,
            (Some(true), Some(_), None) => Found::Ready,
            (None, Some(_), None) => Found::Installed,
        }
    }

    // Builds the row of one provider: what Kodem found, and the next step when there is one.
    fn setup_provider(
        &self,
        index: usize,
        provider: Provider,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let runtime = self.provider_runtime(provider);
        let found = self.provider_found(provider, &runtime);
        let name = runtime
            .version
            .clone()
            .unwrap_or_else(|| provider.label().to_owned());
        // The account that new tasks of this provider use is the one to sign in.
        let account = self
            .provider_preferences(provider)
            .account
            .unwrap_or_else(|| DEFAULT_ACCOUNT.to_owned());
        let signing_in = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.account(provider, Some(&account)))
            .is_some_and(|state| state.status.signing_in);
        let is_default = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.settings.agent_defaults.provider == provider);
        let installing =
            found == Found::Unavailable && self.tool_installers.contains(&Tool::of(provider));
        let (status, tone) = match found {
            Found::Checking => ("CHECKING", t.subtle),
            Found::Unavailable if installing => ("INSTALLING", t.info),
            Found::Unavailable => ("NOT AVAILABLE", t.removed),
            Found::SignedOut => ("SIGN-IN NEEDED", t.warning),
            Found::Installed => ("INSTALLED", t.info),
            Found::Ready => ("READY", t.added),
        };
        let detail = match found {
            Found::Checking => format!(
                "Kodem looks for the {} CLI on this computer.",
                provider.label()
            ),
            Found::Unavailable if installing => INSTALLER_RUNNING.to_owned(),
            Found::Unavailable => runtime
                .error
                .as_deref()
                .unwrap_or_default()
                .chars()
                .take(FAILURE_LIMIT)
                .collect(),
            Found::SignedOut if signing_in => {
                "Finish the sign-in that opened in Terminal and your browser.".to_owned()
            }
            Found::SignedOut => format!("{name} is installed. Sign in to use it."),
            Found::Installed => {
                format!("{name} is installed. Kodem did not read its sign-in state yet.")
            }
            Found::Ready => format!("{name} is installed and an account is signed in."),
        };
        let mut row = div()
            .py(px(10.))
            .flex()
            .items_center()
            .gap(px(12.))
            .child(kit::icon_tile(
                provider.icon(),
                tone,
                32.,
                found == Found::Checking || signing_in,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().text_size(text_px(13.)).child(provider.label()))
                            .child(kit::chip(status, tone))
                            .when(is_default && found == Found::Ready, |d| {
                                d.child(kit::chip("NEW TASKS", t.muted))
                            }),
                    )
                    .child(settings::note(detail, t)),
            );
        match found {
            Found::SignedOut => {
                row = row.child(
                    self.primary(
                        ("setup-sign-in", index),
                        "log-in",
                        if signing_in {
                            "Waiting for sign-in"
                        } else {
                            "Sign in"
                        },
                    )
                    .disabled(signing_in)
                    .on_click(cx.listener(move |s, _, _, _| {
                        s.send(core::Request::SignInAccount {
                            provider,
                            id: account.clone(),
                        })
                    })),
                );
            }
            Found::Ready if !is_default => {
                row = row.child(
                    self.ghost(("setup-default", index), "star", "Use for new tasks")
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let preferences = s.provider_preferences(provider);
                            s.save_agent_preferences(preferences);
                            cx.notify();
                        })),
                );
            }
            Found::Unavailable => {
                if let Some((label, url)) = settings::install_guide(provider) {
                    row = row.child(
                        self.ghost(("setup-guide", index), "external-link", label)
                            .on_click(cx.listener(move |_, _, _, cx| cx.open_url(url))),
                    );
                }
                row = row.child(self.install_button(
                    ("setup-install", index),
                    Tool::of(provider),
                    installing,
                    cx,
                ));
            }
            Found::Checking | Found::Installed | Found::Ready => {}
        }
        row.into_any_element()
    }

    // Builds the button that opens a tool's installer in a terminal window. After the installer
    // opened, it offers to run it again in case that run failed.
    fn install_button(
        &self,
        id: impl Into<ElementId>,
        tool: Tool,
        installing: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let label = if installing {
            "Install again"
        } else {
            "Install"
        };
        let button = if installing {
            self.ghost(id, "arrow-down", label)
        } else {
            self.primary(id, "arrow-down", label)
        };
        button.on_click(cx.listener(move |s, _, _, _| {
            s.send(core::Request::InstallTool { tool });
        }))
    }

    // Builds the row of one support tool: its version, or why Kodem needs it and its installer.
    fn setup_tool(&self, index: usize, tool: Tool, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let checked = self.tools.iter().find(|found| found.tool == tool);
        let version = checked.and_then(|found| found.version.clone());
        let installing = version.is_none() && self.tool_installers.contains(&tool);
        let (status, tone) = match (checked, &version) {
            (None, _) => ("CHECKING", t.subtle),
            (Some(_), Some(_)) => ("INSTALLED", t.added),
            (Some(_), None) if installing => ("INSTALLING", t.info),
            // Kodem cannot track changes without Git; the others serve some projects only.
            (Some(_), None) if tool == Tool::Git => ("MISSING", t.removed),
            (Some(_), None) => ("OPTIONAL", t.warning),
        };
        let (glyph, purpose) = match tool {
            Tool::Git => (
                "branch",
                "Kodem needs Git for changes, worktrees, checkpoints and reviews.",
            ),
            Tool::GitLfs => (
                "archive",
                "Repositories that keep large files in Git LFS need it.",
            ),
            _ => (
                "git-pull-request",
                "Reviews list pull requests with it.",
            ),
        };
        let detail = match (&version, installing) {
            (Some(version), _) => version.clone(),
            (None, true) => INSTALLER_RUNNING.to_owned(),
            (None, false) => purpose.to_owned(),
        };
        div()
            .py(px(10.))
            .flex()
            .items_center()
            .gap(px(12.))
            .child(kit::icon_tile(
                glyph,
                tone,
                32.,
                checked.is_none() || installing,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().text_size(text_px(13.)).child(tool.label()))
                            .child(kit::chip(status, tone)),
                    )
                    .child(settings::note(detail, t)),
            )
            .when(checked.is_some() && version.is_none(), |row| {
                row.child(self.install_button(("setup-tool", index), tool, installing, cx))
            })
            .into_any_element()
    }

    /// Builds the content of the setup dialog: the providers and support tools found on this
    /// computer with installers for the missing ones, the Claude Code features, and the
    /// workspace choices that most users make first.
    pub(super) fn setup_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut providers = settings::setting_group("Providers", Some(Provider::ALL.len()), t);
        for (index, provider) in Provider::ALL.into_iter().enumerate() {
            providers = providers
                .child(self.setup_provider(index, provider, cx))
                .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)));
        }
        providers = providers.child(
            div()
                .pt(px(10.))
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .child(
                    self.ghost("setup-check", "refresh", "Check again")
                        .on_click(cx.listener(|s, _, _, _| s.recheck_providers())),
                )
                .child(
                    self.ghost("setup-providers", "settings-2", "Provider settings")
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.open_settings(settings::PROVIDERS);
                            cx.notify();
                        })),
                ),
        );
        let mut tools = settings::setting_group("Tools", Some(Tool::SUPPORT.len()), t);
        for (index, tool) in Tool::SUPPORT.into_iter().enumerate() {
            tools = tools
                .child(self.setup_tool(index, tool, cx))
                .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)));
        }
        tools = tools.child(div().pt(px(10.)).child(settings::note(INSTALLER_NOTE, t)));
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(settings::note(
                "Kodem runs the coding agents that are installed on this computer, and their own CLIs keep the sign-in. This is what Kodem found. Install what is missing here, and change each choice later in Settings.",
                t,
            ))
            .child(providers)
            .child(tools)
            .child(self.claude_feature_settings(cx))
            .child(
                settings::setting_group("Workspace", None, t)
                    .child(self.appearance_setting(cx))
                    .child(self.task_location_setting(cx))
                    .child(self.task_notifications_setting(
                        &format!("Send {} notifications when a task needs you, finishes, fails or stops. Select the events in Settings.", settings::NOTIFIER),
                        cx,
                    )),
            )
            .into_any_element()
    }
}
