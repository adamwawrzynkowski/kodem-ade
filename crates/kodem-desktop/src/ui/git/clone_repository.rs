//! The Clone Repository dialog. It lists the repositories of a linked
//! account, or takes any URL, and shows `git clone` progress on a
//! dithered bar. Without a linked account it offers the link form.

use std::path::PathBuf;

use futures::StreamExt as _;
use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    App, Context, Entity, FontWeight, IntoElement, PathPromptOptions, Render, SharedString,
    Subscription, WeakEntity, Window, div, relative,
};
use kodem_git::git::{CloneProgress, clone_repository, repository_name_from_url};
use kodem_git::github::{self, RemoteRepository};
use kodem_git::settings::Account;

use crate::theme::{px, text_px};
use crate::ui::dither::{Dither, DitherField, Edge};
use crate::ui::git::account_link::{AccountLinkEvent, AccountLinkForm};
use crate::ui::git::components::{
    button, dialog_header, dither_empty_hint, dither_loader, mono_font, text_field,
};
use crate::ui::git::contextual_actions::{
    callout, divider, modal_footer, open_liquid_dialog, section_label,
};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, Icon, IconName, InputEvent, InputState,
    ScrollableElement as _, Selectable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::theme;
use crate::ui::git::workspace::{GitWorkspace, home_relative_path};

const DIALOG_WIDTH: f32 = 580.0;
/// Rows shown before the repository list scrolls.
const LIST_HEIGHT: f32 = 252.0;

/// Where the repository comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloneSource {
    /// A linked account, by its index in the settings.
    Account(usize),
    Url,
}

enum RepositoryList {
    Loading,
    Loaded(Vec<RemoteRepository>),
    Failed(SharedString),
}

/// The repositories of one account, and which account they belong to.
struct LoadedRepositories {
    host: String,
    username: String,
    list: RepositoryList,
}

/// Progress of a running clone.
struct CloneRun {
    phase: SharedString,
    fraction: f32,
}

/// Lets the user pick a repository and a folder, then clones it and
/// opens it in the workspace.
pub struct CloneRepositoryDialog {
    workspace: WeakEntity<GitWorkspace>,
    source: CloneSource,
    link_form: Entity<AccountLinkForm>,
    search: Entity<InputState>,
    url: Entity<InputState>,
    repositories: Option<LoadedRepositories>,
    selected: Option<String>,
    directory: PathBuf,
    run: Option<CloneRun>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl CloneRepositoryDialog {
    fn new(
        workspace: WeakEntity<GitWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let link_form = cx.new(|cx| AccountLinkForm::new(window, cx));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Filter repositories"));
        let url = cx.new(|cx| {
            InputState::new(window, cx).placeholder("https://github.com/owner/repository.git")
        });
        let subscriptions = vec![
            cx.subscribe_in(&link_form, window, |this, _, event, _, cx| {
                let AccountLinkEvent::Linked(account) = event;
                let accounts = &GitSettings::get(cx).accounts;
                if let Some(ix) = accounts.iter().position(|saved| {
                    saved.host == account.host && saved.username == account.username
                }) {
                    this.select_source(CloneSource::Account(ix), cx);
                }
            }),
            cx.subscribe_in(&search, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(
                &url,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        this.error = None;
                        cx.notify();
                    }
                    InputEvent::PressEnter => this.start_clone(window, cx),
                },
            ),
        ];
        let directory = GitSettings::get(cx).suggested_clone_directory();
        let mut this = Self {
            workspace,
            source: CloneSource::Account(0),
            link_form,
            search,
            url,
            repositories: None,
            selected: None,
            directory,
            run: None,
            error: None,
            _subscriptions: subscriptions,
        };
        this.select_source(CloneSource::Account(0), cx);
        this
    }

    fn account(&self, cx: &App) -> Option<Account> {
        match self.source {
            CloneSource::Account(ix) => GitSettings::get(cx).accounts.get(ix).cloned(),
            CloneSource::Url => None,
        }
    }

    fn select_source(&mut self, source: CloneSource, cx: &mut Context<Self>) {
        self.source = source;
        self.selected = None;
        self.error = None;
        if let Some(account) = self.account(cx) {
            self.load_repositories(account, false, cx);
        }
        cx.notify();
    }

    /// Loads the repository list of `account`, unless it is already
    /// shown. `force` loads it again.
    fn load_repositories(&mut self, account: Account, force: bool, cx: &mut Context<Self>) {
        let current = self.repositories.as_ref().is_some_and(|loaded| {
            loaded.host == account.host && loaded.username == account.username
        });
        if current && !force {
            return;
        }
        self.repositories = Some(LoadedRepositories {
            host: account.host.clone(),
            username: account.username.clone(),
            list: RepositoryList::Loading,
        });
        let host = account.host.clone();
        let token = account.token.clone();
        let task = cx
            .background_executor()
            .spawn(async move { github::account_repositories(&host, &token) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                let Some(loaded) = this.repositories.as_mut() else {
                    return;
                };
                if loaded.host != account.host || loaded.username != account.username {
                    return;
                }
                loaded.list = match result {
                    Ok(repositories) => RepositoryList::Loaded(repositories),
                    Err(error) => RepositoryList::Failed(error.to_string().into()),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The URL that Clone uses for the current source.
    fn clone_url(&self, cx: &App) -> Option<String> {
        match self.source {
            CloneSource::Account(_) => self.selected.clone(),
            CloneSource::Url => {
                let url = self.url.read(cx).value().trim().to_string();
                (!url.is_empty()).then_some(url)
            }
        }
    }

    fn choose_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Clone Here".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(directory) = paths.into_iter().next()
            {
                this.update(cx, |this, cx| {
                    this.directory = directory;
                    this.error = None;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn start_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.run.is_some() {
            return;
        }
        let Some(url) = self.clone_url(cx) else {
            return;
        };
        let directory = self.directory.clone();
        self.error = None;
        self.run = Some(CloneRun {
            phase: "Connecting".into(),
            fraction: 0.0,
        });
        cx.notify();

        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<CloneProgress>();
        let task = cx.background_executor().spawn(async move {
            clone_repository(&url, &directory, |progress| {
                sender.unbounded_send(progress).ok();
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            // The channel closes when the clone returns and drops the sender.
            while let Some(progress) = receiver.next().await {
                let updated = this.update(cx, |this, cx| {
                    if let Some(run) = this.run.as_mut() {
                        run.phase = progress.phase.clone().into();
                        run.fraction = run.fraction.max(progress.overall_fraction());
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    return;
                }
            }
            let result = task.await;
            this.update_in(cx, |this, window, cx| this.finish_clone(result, window, cx))
                .ok();
        })
        .detach();
    }

    fn finish_clone(
        &mut self,
        result: anyhow::Result<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run = None;
        match result {
            Ok(path) => {
                let directory = self.directory.clone();
                GitSettings::update(cx, |settings| settings.clone_directory = Some(directory));
                window.close_dialog(cx);
                self.workspace
                    .update(cx, |workspace, cx| {
                        workspace.add_repository(path, window, cx)
                    })
                    .ok();
            }
            Err(error) => {
                self.error = Some(error.to_string().into());
                cx.notify();
            }
        }
    }

    fn render_sources(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let accounts = GitSettings::get(cx).accounts.clone();
        let cloning = self.run.is_some();
        let mut sources = accounts
            .iter()
            .enumerate()
            .map(|(ix, account)| {
                let label = if accounts.len() > 1 || account.host != "github.com" {
                    format!("{} · {}", account.username, account.host)
                } else {
                    account.username.clone()
                };
                (
                    CloneSource::Account(ix),
                    SharedString::from(label),
                    IconName::Github,
                )
            })
            .collect::<Vec<_>>();
        if accounts.is_empty() {
            sources.push((CloneSource::Account(0), "GitHub".into(), IconName::Github));
        }
        sources.push((CloneSource::Url, "URL".into(), IconName::Globe));

        h_flex()
            .w_full()
            .flex_wrap()
            .gap_1()
            .children(
                sources
                    .into_iter()
                    .enumerate()
                    .map(|(ix, (source, label, icon))| {
                        let selected = self.source == source;
                        button(("clone-source", ix))
                            .small()
                            .outline()
                            .when(selected, |button| button.primary())
                            .selected(selected)
                            .icon(icon)
                            .label(label)
                            .disabled_readable(cloning && !selected, cx)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.select_source(source, cx)),
                            )
                    }),
            )
    }

    fn render_repository_list(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = theme::liquid_colors(cx);
        let Some(loaded) = self.repositories.as_ref() else {
            return div().into_any_element();
        };
        let frame = v_flex()
            .w_full()
            .h(px(LIST_HEIGHT))
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(8.))
            .overflow_hidden()
            .bg(colors.panel_secondary);
        let repositories = match &loaded.list {
            RepositoryList::Loading => {
                return frame
                    .child(dither_loader(
                        format!("Loading repositories of {}", loaded.username),
                        cx,
                    ))
                    .into_any_element();
            }
            RepositoryList::Failed(error) => {
                return frame
                    .p_3()
                    .gap_2()
                    .child(callout(error.clone(), colors.danger))
                    .child(
                        h_flex().child(
                            button("clone-retry")
                                .small()
                                .outline()
                                .icon(icons::refresh())
                                .label("Try Again")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(account) = this.account(cx) {
                                        this.load_repositories(account, true, cx);
                                    }
                                })),
                        ),
                    )
                    .into_any_element();
            }
            RepositoryList::Loaded(repositories) => repositories,
        };

        let filter = self.search.read(cx).value().trim().to_lowercase();
        let matches = repositories
            .iter()
            .filter(|repository| {
                filter.is_empty() || repository.full_name.to_lowercase().contains(&filter)
            })
            .collect::<Vec<_>>();
        if matches.is_empty() {
            let message = if repositories.is_empty() {
                "This account has no repositories."
            } else {
                "No repository matches the filter."
            };
            return frame
                .justify_center()
                .child(dither_empty_hint(message, cx))
                .into_any_element();
        }

        let cloning = self.run.is_some();
        frame
            .id("clone-repository-list")
            .overflow_y_scrollbar()
            .children(matches.into_iter().enumerate().map(|(ix, repository)| {
                let selected = self.selected.as_deref() == Some(repository.clone_url.as_str());
                let clone_url = repository.clone_url.clone();
                let accent = cx.theme().primary;
                h_flex()
                    .id(("clone-repository", ix))
                    .relative()
                    .w_full()
                    .flex_shrink_0()
                    .gap_2p5()
                    .px_3()
                    .py_2()
                    .when(ix > 0, |row| {
                        row.border_t_1().border_color(cx.theme().border)
                    })
                    .when(selected, |row| {
                        row.bg(accent.opacity(0.1)).child(
                            Dither::new(DitherField::Fade {
                                from: Edge::Left,
                                start: 0.5,
                                end: -0.1,
                            })
                            .color(accent.opacity(0.4))
                            .grid(2.0, 1.0)
                            .layer(),
                        )
                    })
                    .when(!selected && !cloning, |row| {
                        row.cursor_pointer()
                            .hover(|row| row.bg(cx.theme().list_hover))
                    })
                    .when(!cloning, |row| {
                        row.on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = Some(clone_url.clone());
                            this.error = None;
                            cx.notify();
                        }))
                    })
                    .child(
                        div().relative().child(
                            if repository.private {
                                Icon::new(IconName::EyeOff)
                            } else {
                                icons::git_branch()
                            }
                            .small()
                            .text_color(if selected {
                                accent
                            } else {
                                cx.theme().muted_foreground
                            }),
                        ),
                    )
                    .child(
                        v_flex()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .font_family(mono_font())
                                    .text_size(text_px(12.5))
                                    .font_weight(if selected {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(repository.full_name.clone()),
                            )
                            .when_some(repository.description.clone(), |column, description| {
                                column.child(
                                    div()
                                        .text_size(text_px(11.5))
                                        .text_color(colors.text_tertiary)
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(description),
                                )
                            }),
                    )
                    .when(repository.fork, |row| {
                        row.child(
                            div()
                                .relative()
                                .font_family(mono_font())
                                .text_size(text_px(10.5))
                                .text_color(colors.text_tertiary)
                                .child("fork"),
                        )
                    })
                    .when(repository.stargazers_count > 0, |row| {
                        row.child(
                            h_flex()
                                .relative()
                                .gap_0p5()
                                .font_family(mono_font())
                                .text_size(text_px(10.5))
                                .text_color(colors.text_tertiary)
                                .child(Icon::new(IconName::Star).xsmall())
                                .child(repository.stargazers_count.to_string()),
                        )
                    })
            }))
            .into_any_element()
    }

    fn render_progress(&self, cx: &App) -> impl IntoElement {
        let accent = cx.theme().primary;
        let (phase, fraction) = self
            .run
            .as_ref()
            .map_or((SharedString::default(), 0.0), |run| {
                (run.phase.clone(), run.fraction)
            });
        v_flex()
            .w_full()
            .gap_1p5()
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .font_family(mono_font())
                    .text_size(text_px(11.))
                    .text_color(cx.theme().muted_foreground)
                    .child(phase)
                    .child(format!("{:.0}%", fraction * 100.0)),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(10.))
                    .rounded(px(3.))
                    .overflow_hidden()
                    .bg(accent.opacity(0.08))
                    .child(
                        Dither::new(DitherField::Flow {
                            scale: 0.2,
                            speed: 2.0,
                            bias: -0.3,
                        })
                        .color(accent.opacity(0.35))
                        .grid(2.0, 1.0)
                        .animated()
                        .layer(),
                    )
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(relative(fraction.max(0.02)))
                            .child(
                                Dither::new(DitherField::Flow {
                                    scale: 0.12,
                                    speed: 10.0,
                                    bias: 0.35,
                                })
                                .color(accent)
                                .grid(2.0, 2.0)
                                .animated()
                                .layer(),
                            ),
                    ),
            )
    }
}

impl Render for CloneRepositoryDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::liquid_colors(cx);
        let has_accounts = !GitSettings::get(cx).accounts.is_empty();
        let cloning = self.run.is_some();
        let url = self.clone_url(cx);
        let destination = url.as_deref().map_or_else(
            || home_relative_path(&self.directory),
            |url| home_relative_path(&self.directory.join(repository_name_from_url(url))),
        );

        let body = v_flex()
            .w_full()
            .gap_3()
            .px_4()
            .py_3()
            .child(self.render_sources(cx))
            .map(|body| match self.source {
                CloneSource::Account(_) if !has_accounts => body
                    .child(
                        div()
                            .text_size(text_px(12.5))
                            .text_color(colors.text_secondary)
                            .child(
                                "Link a GitHub account to pick from your repositories. The \
                                 token also signs in HTTPS fetch, pull, and push.",
                            ),
                    )
                    .child(self.link_form.clone()),
                CloneSource::Account(_) => body
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(div().flex_1().child(text_field(&self.search)))
                            .child(
                                button("clone-reload")
                                    .small()
                                    .ghost()
                                    .icon(icons::refresh())
                                    .tooltip("Reload repositories")
                                    .disabled_readable(cloning, cx)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(account) = this.account(cx) {
                                            this.load_repositories(account, true, cx);
                                        }
                                    })),
                            ),
                    )
                    .child(self.render_repository_list(cx)),
                CloneSource::Url => body
                    .child(section_label("REPOSITORY URL", cx))
                    .child(text_field(&self.url))
                    .child(
                        div()
                            .font_family(mono_font())
                            .text_size(text_px(11.))
                            .text_color(colors.text_tertiary)
                            .child("HTTPS and SSH URLs both work."),
                    ),
            })
            .child(section_label("CLONE INTO", cx))
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .px_2p5()
                            .py_1p5()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().muted)
                            .child(
                                Icon::new(IconName::FolderOpen)
                                    .small()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(mono_font())
                                    .text_size(text_px(12.5))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(destination),
                            ),
                    )
                    .child(
                        button("clone-choose-folder")
                            .small()
                            .outline()
                            .label("Choose…")
                            .disabled_readable(cloning, cx)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_directory(window, cx);
                            })),
                    ),
            )
            .when(cloning, |body| body.child(self.render_progress(cx)))
            .when_some(self.error.clone(), |body, error| {
                body.child(callout(error, colors.danger))
            });

        v_flex()
            .w_full()
            .overflow_hidden()
            .bg(cx.theme().popover)
            .child(dialog_header(
                icons::pull(),
                "Clone Repository",
                "Copy a remote repository to this computer",
                cx.theme().primary,
                cx,
            ))
            .child(divider(cx))
            .child(body)
            .child(divider(cx))
            .child(modal_footer(
                "clone-cancel",
                "Cancel",
                move |_, window, cx| {
                    if !cloning {
                        window.close_dialog(cx);
                    }
                },
                button("clone-confirm")
                    .primary()
                    .label(if cloning { "Cloning…" } else { "Clone" })
                    .icon(icons::pull())
                    .loading(cloning)
                    .disabled_readable(url.is_none() || cloning, cx)
                    .on_click(cx.listener(|this, _, window, cx| this.start_clone(window, cx))),
            ))
    }
}

/// Opens the Clone Repository dialog. A finished clone opens in
/// `workspace`.
pub fn open_clone_dialog(workspace: WeakEntity<GitWorkspace>, window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| CloneRepositoryDialog::new(workspace, window, cx));
    open_liquid_dialog(view, px(DIALOG_WIDTH), window, cx);
}
