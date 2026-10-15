//! The Local Ignore page: files that the Changes view hides in the active repository without a
//! `.gitignore` entry.
use gpui::prelude::*;
use gpui::{App, Context, Div, Window, div};

use super::{GitSettingsView, list_card, page, section};
use crate::theme::{px, text_px};
use crate::ui::git::components::{
    button, dither_empty_hint, file_path_label, mono_font, text_field,
};
use crate::ui::git::icons;
use crate::ui::git::kit::{ActiveTheme as _, ButtonVariants as _, Sizable as _, h_flex};
use crate::ui::git::settings::GitSettings;
use crate::ui::settings::note;

impl GitSettingsView {
    /// Returns the Local Ignore page.
    pub(super) fn render_local_ignore(&mut self, cx: &mut Context<Self>) -> Div {
        let settings = GitSettings::get(cx);
        let repo_path = settings
            .active_repository
            .as_ref()
            .map(|path| path.display().to_string());
        let ignored: Vec<String> = repo_path
            .as_ref()
            .map(|path| settings.ignored_for(path).to_vec())
            .unwrap_or_default();
        let repo_label = repo_path.unwrap_or_else(|| "No repository".into());

        page()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(note(
                        "Hide files from the Changes view without a .gitignore entry. The list \
                         applies to the active repository only.",
                        cx.theme().kodem,
                    ))
                    .child(
                        h_flex().child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded(px(4.))
                                .border_1()
                                .border_color(cx.theme().border)
                                .bg(cx.theme().muted)
                                .font_family(mono_font())
                                .text_size(text_px(11.))
                                .text_color(cx.theme().muted_foreground)
                                .child(repo_label),
                        ),
                    ),
            )
            .child(
                section("Add File Path", cx).child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .pt(px(10.))
                        .child(div().flex_1().child(text_field(&self.ignore_path)))
                        .child(
                            button("add-ignore")
                                .primary()
                                .small()
                                .label("Add")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.add_ignored_path(window, cx);
                                })),
                        ),
                ),
            )
            .child(
                section("Ignored Files", cx).child(
                    div()
                        .pt(px(10.))
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .when(ignored.is_empty(), |this| {
                            this.child(dither_empty_hint("No ignored files", cx))
                        })
                        .children(ignored.iter().enumerate().map(|(ix, path)| {
                            let path_for_remove = path.clone();
                            list_card(false, cx)
                                .py_1p5()
                                .child(
                                    icons::file_text()
                                        .small()
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(file_path_label(path, cx).text_size(text_px(12.5)))
                                .child(
                                    button(("remove-ignore", ix))
                                        .ghost()
                                        .xsmall()
                                        .icon(icons::trash())
                                        .tooltip("Remove")
                                        .on_click(move |_, _, cx| {
                                            remove_ignored_path(&path_for_remove, cx);
                                        }),
                                )
                        })),
                ),
            )
    }

    // Adds the typed path to the ignore list of the active repository and clears the field.
    fn add_ignored_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.ignore_path.read(cx).value().trim().to_string();
        if path.is_empty() {
            return;
        }
        let Some(repo) = GitSettings::get(cx)
            .active_repository
            .as_ref()
            .map(|path| path.display().to_string())
        else {
            return;
        };
        GitSettings::update(cx, |s| {
            let list = s.local_ignored.entry(repo).or_default();
            if !list.contains(&path) {
                list.push(path);
            }
        });
        self.ignore_path
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }
}

// Removes a path from the ignore list of the active repository.
fn remove_ignored_path(path: &str, cx: &mut App) {
    let Some(repo) = GitSettings::get(cx)
        .active_repository
        .as_ref()
        .map(|path| path.display().to_string())
    else {
        return;
    };
    GitSettings::update(cx, |s| {
        if let Some(list) = s.local_ignored.get_mut(&repo) {
            list.retain(|ignored| ignored != path);
        }
    });
}
