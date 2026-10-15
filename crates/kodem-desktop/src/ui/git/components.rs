//! Shared pieces of the Git workspace's views: tab rails, section headers, badges, file rows,
//! author avatars, dialog headers and dithered empty states.
use std::collections::HashSet;
use std::sync::Arc;

use gpui::ColorExt as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Asset, Div, ElementId, Entity, Hsla, Image, ImageFormat, InteractiveElement as _,
    IntoElement, ObjectFit, ParentElement as _, Pixels, RenderOnce, SharedString, StyleRefinement,
    Styled as _, StyledImage as _, Window, div, img,
};
use kodem_git::git::{CommitRef, FileChangeKind, RefKind};
use sha2::{Digest as _, Sha256};

use super::kit::{Icon, Input, InputState, LiquidButton, Sizable as _, h_flex, v_flex};
use super::settings::GitSettings;
use super::theme::{self, ActiveTheme as _};
use crate::theme::{px, text_px};
use crate::ui::dither::{Dither, DitherField, DottedRule, Edge, checker};

/// Returns the mono typeface used for hashes, counts, labels, and diff text.
pub fn mono_font() -> &'static str {
    crate::theme::mono()
}

/// Returns the shared height for the sidebar category rail and the History /
/// Changes / Pull Requests rail, so both rows line up.
pub fn tab_rail_height() -> Pixels {
    px(34.0)
}

/// Returns the size of the small uppercase labels used for section titles.
pub fn label_text_size() -> Pixels {
    text_px(10.5)
}

/// Returns a short label for each repository name: its first letter, or more characters or a
/// number when another repository already has that label.
pub fn repository_rail_labels(names: &[String]) -> Vec<String> {
    let mut used = HashSet::new();

    names
        .iter()
        .map(|name| {
            let characters = name
                .chars()
                .filter(|character| character.is_alphanumeric())
                .flat_map(char::to_uppercase)
                .collect::<Vec<_>>();
            let first = characters.first().copied().unwrap_or('?');
            let first_label = first.to_string();
            if used.insert(first_label.clone()) {
                return first_label;
            }

            for character in characters.iter().skip(1) {
                let candidate = format!("{first}{character}");
                if used.insert(candidate.clone()) {
                    return candidate;
                }
            }

            let mut suffix = 2;
            loop {
                let candidate = format!("{first}{suffix}");
                if used.insert(candidate.clone()) {
                    return candidate;
                }
                suffix += 1;
            }
        })
        .collect()
}

/// Outer bar used by both tab rails.
pub fn tab_rail(cx: &App) -> Div {
    h_flex()
        .w_full()
        .h(tab_rail_height())
        .flex_shrink_0()
        .px_1()
        .bg(cx.theme().title_bar)
        .border_b_1()
        .border_color(cx.theme().border)
}

/// One tab cell: content is centered, and the indicator sits on the
/// bottom edge so icon tabs and label tabs share the same baseline.
pub fn tab_cell(selected: bool, cx: &App) -> Div {
    let hover = cx.theme().list_hover;
    div()
        .relative()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(!selected, |cell| cell.hover(move |style| style.bg(hover)))
}

/// The selected-tab indicator: a 2px accent line with a dithered glow
/// that fades upward, like phosphor bleeding on an old display.
pub fn tab_indicator(selected: bool, cx: &App) -> Div {
    let indicator = div().absolute().bottom_0().left_1().right_1().h(px(9.));
    if !selected {
        return indicator;
    }
    indicator.child(tab_indicator_bar(cx.theme().primary))
}

/// The glow and accent line of the tab indicator, filling its parent.
/// Used on its own where the indicator slides between tabs.
pub fn tab_indicator_bar(accent: Hsla) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(
            Dither::new(DitherField::Fade {
                from: Edge::Bottom,
                start: 0.7,
                end: 0.0,
            })
            .color(accent.opacity(0.5))
            .grid(2.0, 1.0)
            .layer(),
        )
        .child(
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .h(px(2.))
                .bg(accent),
        )
}

/// A small uppercase mono label, used for section titles and table
/// headers.
pub fn mono_label(label: impl Into<SharedString>, cx: &App) -> Div {
    let label: SharedString = label.into();
    div()
        .flex_shrink_0()
        .font_family(mono_font())
        .text_size(label_text_size())
        .line_height(text_px(14.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme::liquid_colors(cx).text_tertiary)
        .child(SharedString::from(label.to_uppercase()))
}

/// A section header: an uppercase label, an optional count, and a dotted
/// rule that fills the remaining width.
pub fn section_header(label: impl Into<SharedString>, count: Option<usize>, cx: &App) -> Div {
    let colors = theme::liquid_colors(cx);
    h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .child(mono_label(label, cx))
        .when_some(count, |header, count| {
            header.child(
                div()
                    .font_family(mono_font())
                    .text_size(label_text_size())
                    .text_color(colors.text_secondary)
                    .child(count.to_string()),
            )
        })
        .child(
            div()
                .flex_1()
                .child(DottedRule::horizontal(colors.text_tertiary.opacity(0.4))),
        )
}

/// A section header for a status group, such as staged or conflicted
/// files: a dithered marker, a label and count in the tone color, and a
/// dotted rule in the same tone.
pub fn status_section_header(
    label: impl Into<SharedString>,
    count: usize,
    tone: Hsla,
    cx: &App,
) -> Div {
    h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .child(
            div()
                .relative()
                .size(px(9.))
                .flex_shrink_0()
                .rounded(px(2.))
                .overflow_hidden()
                .border_1()
                .border_color(tone.opacity(0.7))
                .child(
                    Dither::new(DitherField::Fade {
                        from: Edge::Bottom,
                        start: 1.0,
                        end: 0.4,
                    })
                    .color(tone)
                    .grid(1.0, 1.0)
                    .layer(),
                ),
        )
        .child(mono_label(label, cx).text_color(tone))
        .child(
            div()
                .font_family(mono_font())
                .text_size(label_text_size())
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(tone)
                .child(count.to_string()),
        )
        .child(
            div()
                .flex_1()
                .child(DottedRule::horizontal(tone.opacity(0.35))),
        )
}

/// Returns the layout style of a cached view that fills its parent.
/// Pass it to `Entity::cached` for panels that render again only when
/// their own entity is notified.
pub fn full_size() -> StyleRefinement {
    StyleRefinement::default().size_full()
}

/// Creates a Liquid button: a mono label on a dithered surface. Use it
/// instead of the gpui-component button everywhere in the app.
pub fn button(id: impl Into<ElementId>) -> LiquidButton {
    LiquidButton::new(id)
}

/// Creates a single-line text field in the mono face, for commit titles,
/// branch names, paths, URLs, and search.
pub fn text_field(state: &Entity<InputState>) -> Input {
    Input::new(state).font_family(mono_font())
}

/// A badge for one commit ref (branch, tag, HEAD).
pub fn ref_badge(commit_ref: &CommitRef, lane_color: Hsla, is_checked_out: bool, cx: &App) -> Div {
    let colors = theme::liquid_colors(cx);
    let accent = cx.theme().primary;
    let (color, label, filled) = match commit_ref.kind {
        RefKind::Head => (accent, SharedString::from("HEAD"), true),
        RefKind::LocalBranch => (accent, commit_ref.name.clone().into(), is_checked_out),
        RefKind::RemoteBranch => (lane_color, commit_ref.name.clone().into(), false),
        RefKind::Tag => (colors.warning, commit_ref.name.clone().into(), false),
    };
    let marker = match commit_ref.kind {
        RefKind::Tag => "◆",
        RefKind::RemoteBranch => "◇",
        RefKind::Head | RefKind::LocalBranch if is_checked_out => "●",
        RefKind::Head | RefKind::LocalBranch => "○",
    };
    h_flex()
        .flex_shrink_0()
        .h(px(18.))
        .gap_1()
        .px_1p5()
        .rounded(px(4.))
        .border_1()
        .border_color(color.opacity(if filled { 0.5 } else { 0.32 }))
        .bg(color.opacity(if filled { 0.16 } else { 0.06 }))
        .text_color(color)
        .font_family(mono_font())
        .text_size(text_px(11.))
        .when(filled, |badge| {
            badge.font_weight(gpui::FontWeight::SEMIBOLD)
        })
        .child(div().text_size(text_px(7.)).child(marker))
        .child(label)
}

/// Number of blocks in a micro diffstat.
const DIFFSTAT_BLOCKS: u32 = 5;

/// Splits the diffstat blocks into added, removed, and unchanged blocks,
/// like the compact diffstat on code review sites.
fn diffstat_blocks(additions: u32, deletions: u32) -> (u32, u32) {
    let total = additions + deletions;
    if total == 0 {
        return (0, 0);
    }
    let filled = DIFFSTAT_BLOCKS.min(total);
    let added = ((additions as f32 / total as f32) * filled as f32).round() as u32;
    let added = if additions > 0 { added.max(1) } else { 0 }.min(filled);
    let removed = if deletions > 0 {
        (filled - added).max(1).min(filled)
    } else {
        0
    };
    let added = added.min(filled - removed);
    (added, removed)
}

/// Mono "+N −N" counts with a five-block micro diffstat.
pub fn stat_chips(additions: u32, deletions: u32, cx: &App) -> Div {
    let colors = theme::liquid_colors(cx);
    let (added, removed) = diffstat_blocks(additions, deletions);
    let neutral = cx.theme().muted_foreground.opacity(0.22);
    h_flex()
        .gap_1p5()
        .flex_shrink_0()
        .items_center()
        .font_family(mono_font())
        .text_size(text_px(11.))
        .when(additions > 0, |chips| {
            chips.child(
                div()
                    .text_color(colors.success)
                    .child(format!("+{additions}")),
            )
        })
        .when(deletions > 0, |chips| {
            chips.child(
                div()
                    .text_color(colors.danger)
                    .child(format!("−{deletions}")),
            )
        })
        .child(
            h_flex()
                .gap(px(1.5))
                .children((0..DIFFSTAT_BLOCKS).map(|ix| {
                    let color = if ix < added {
                        colors.success
                    } else if ix < added + removed {
                        colors.danger
                    } else {
                        neutral
                    };
                    div().size(px(5.)).rounded(px(1.)).bg(color)
                })),
        )
}

#[derive(Clone)]
enum AvatarImageAsset {}

impl Asset for AvatarImageAsset {
    type Source = String;
    type Output = Option<Arc<Image>>;

    // The future must not borrow the app, which `async fn` would, so it is built explicitly.
    #[allow(clippy::manual_async_fn)]
    fn load(url: Self::Source, _: &mut App) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            // The window has no HTTP client of its own; curl fetches the image on the
            // background thread that loads the asset.
            let output = kodem_git::process::command("curl")
                .args(["-sSfL", "--max-time", "10", &url])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let bytes = output.stdout;
            let format = image_format(&bytes)?;
            Some(Arc::new(Image::from_bytes(format, bytes)))
        }
    }
}

/// An author's round avatar: the remote image when it loads, otherwise the initial on a
/// dithered disc.
#[derive(IntoElement)]
pub struct AuthorAvatar {
    name: SharedString,
    image_url: Option<String>,
    size: f32,
}

impl RenderOnce for AuthorAvatar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let image = self
            .image_url
            .as_ref()
            .and_then(|url| window.use_asset::<AvatarImageAsset>(url, cx))
            .flatten();
        let initial = avatar_initial(&self.name);
        let font_size = self.size * 0.5;
        let palette = theme::graph_colors(cx);
        let color = palette[1 + (gpui::hash(&initial) as usize) % (palette.len() - 1)];
        let avatar = div()
            .size(px(self.size))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().tokens.secondary);

        if let Some(image) = image {
            avatar.child(
                img(image)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded_full(),
            )
        } else {
            // A dithered disc keeps initials in the same retro texture as
            // the rest of the app.
            avatar
                .border_color(color.opacity(0.45))
                .bg(checker(color.opacity(0.32), 1.0, window))
                .text_color(color)
                .child(
                    div()
                        .h(px(font_size))
                        .line_height(text_px(font_size))
                        .text_size(text_px(font_size))
                        .font_family(mono_font())
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(initial),
                )
        }
    }
}

fn avatar_initial(name: &str) -> SharedString {
    let initial = name
        .trim()
        .chars()
        .next()
        .and_then(|character| character.to_uppercase().next())
        .unwrap_or('?');
    SharedString::from(initial.to_string())
}

/// An author avatar with a remote image and an initials fallback.
pub fn author_avatar(name: &str, email: &str, size: f32, cx: &App) -> AuthorAvatar {
    let name = name.trim();
    AuthorAvatar {
        name: SharedString::from(if name.is_empty() { "?" } else { name }),
        image_url: GitSettings::get(cx)
            .show_avatars
            .then(|| avatar_image_url(email))
            .flatten(),
        size,
    }
}

fn avatar_image_url(email: &str) -> Option<String> {
    let email = email.trim().to_lowercase();
    if email.is_empty() {
        return None;
    }
    if let Some(username) = github_noreply_username(&email) {
        return Some(format!("https://github.com/{username}.png?size=64"));
    }

    // Gravatar accepts the SHA-256 digest of the address as well as its MD5 digest.
    let hash = Sha256::digest(email.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Some(format!("https://www.gravatar.com/avatar/{hash}?s=64&d=404"))
}

fn github_noreply_username(email: &str) -> Option<&str> {
    let local = email.strip_suffix("@users.noreply.github.com")?;
    let username = local
        .rsplit_once('+')
        .map_or(local, |(_, username)| username);
    (!username.is_empty()).then_some(username)
}

fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(ImageFormat::Gif)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else {
        None
    }
}

/// The color for a file status letter.
pub fn change_kind_color(kind: FileChangeKind, cx: &App) -> Hsla {
    let colors = theme::liquid_colors(cx);
    match kind {
        FileChangeKind::Added | FileChangeKind::Untracked => colors.success,
        FileChangeKind::Deleted => colors.danger,
        FileChangeKind::Conflicted => colors.warning,
        FileChangeKind::Renamed | FileChangeKind::Copied => cx.theme().primary,
        FileChangeKind::Modified => colors.warning,
    }
}

/// A small square chip with the status letter of a file change.
pub fn change_kind_chip(kind: FileChangeKind, cx: &App) -> Div {
    let color = change_kind_color(kind, cx);
    div()
        .flex_shrink_0()
        .w(px(16.))
        .h(px(16.))
        .rounded(px(4.))
        .border_1()
        .border_color(color.opacity(0.3))
        .bg(color.opacity(0.1))
        .text_color(color)
        .font_family(mono_font())
        .text_size(text_px(10.))
        .font_weight(gpui::FontWeight::BOLD)
        .flex()
        .items_center()
        .justify_center()
        .child(kind.letter())
}

/// A file path with the folder part dimmed and the file name in full
/// strength, so long lists are easy to scan.
pub fn file_path_label(path: &str, cx: &App) -> Div {
    let colors = theme::liquid_colors(cx);
    let (folder, name) = match path.rsplit_once('/') {
        Some((folder, name)) => (Some(format!("{folder}/")), name.to_string()),
        None => (None, path.to_string()),
    };
    h_flex()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_size(text_px(11.5))
        .child(
            div()
                .flex_shrink_0()
                .text_color(cx.theme().foreground)
                .child(name),
        )
        .when_some(folder, |label, folder| {
            label.child(
                div()
                    .min_w_0()
                    .pl_1p5()
                    .overflow_hidden()
                    .text_color(colors.text_tertiary)
                    .child(folder),
            )
        })
}

/// A neutral mono count badge, like the sidebar folder counts.
pub fn count_badge(count: usize, cx: &App) -> Div {
    div()
        .flex_shrink_0()
        .h(px(16.))
        .px_1p5()
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(cx.theme().muted)
        .text_color(cx.theme().muted_foreground)
        .font_family(mono_font())
        .text_size(text_px(10.5))
        .child(count.to_string())
}

fn accent_count_label(count: usize) -> String {
    if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    }
}

/// A filled accent count badge, used for unread and ahead/behind counts.
pub fn accent_count_badge(count: usize, selected: bool, cx: &App) -> Div {
    let accent = cx.theme().primary;
    div()
        .flex_shrink_0()
        .min_w(px(16.))
        .h(px(15.))
        .px_1()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .border_1()
        .border_color(if selected {
            cx.theme().primary_foreground.opacity(0.6)
        } else {
            accent
        })
        .bg(accent)
        .text_color(cx.theme().primary_foreground)
        .font_family(mono_font())
        .text_size(text_px(10.))
        .line_height(text_px(10.))
        .font_weight(gpui::FontWeight::BOLD)
        .child(accent_count_label(count))
}

/// The header used by every dialog: an icon tile, a title, and a mono
/// detail line over a dithered glow in the tone color.
pub fn dialog_header(
    icon: impl Into<Icon>,
    title: impl IntoElement,
    detail: impl IntoElement,
    tone: Hsla,
    cx: &App,
) -> Div {
    let colors = theme::liquid_colors(cx);
    div()
        .relative()
        .w_full()
        .overflow_hidden()
        .child(
            Dither::new(
                DitherField::Glow {
                    x: 1.0,
                    y: 0.0,
                    radius: 0.62,
                    strength: 0.95,
                }
                .times(DitherField::Fade {
                    from: Edge::Top,
                    start: 1.0,
                    end: 0.35,
                }),
            )
            .color(tone.opacity(0.5))
            .grid(3.0, 2.0)
            .toned()
            .layer(),
        )
        .child(
            h_flex()
                .relative()
                .w_full()
                .gap_3()
                .px_4()
                .pt_4()
                .pb_3()
                .child(
                    div()
                        .size(px(36.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(9.))
                        .border_1()
                        .border_color(tone.opacity(0.32))
                        .bg(colors.elevated.blend(&tone.opacity(0.14)))
                        .child(icon.into().small().text_color(tone)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .text_size(text_px(14.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child(title),
                        )
                        .child(
                            div()
                                .font_family(mono_font())
                                .text_size(text_px(11.5))
                                .text_color(colors.text_tertiary)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(detail),
                        ),
                ),
        )
}

/// An empty or loading state with an animated dithered emblem.
pub fn dither_empty_state(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    field: DitherField,
    cx: &App,
) -> Div {
    let colors = theme::liquid_colors(cx);
    let title: SharedString = title.into();
    let detail: SharedString = detail.into();
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .p_6()
        .child(
            div().relative().w(px(168.)).h(px(72.)).child(
                Dither::new(field)
                    .color(cx.theme().primary.opacity(0.85))
                    .grid(4.0, 3.0)
                    .toned()
                    .animated()
                    .corner_radius(8.0)
                    .layer(),
            ),
        )
        .child(
            div()
                .text_size(text_px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(title),
        )
        .when(!detail.is_empty(), |state| {
            state.child(
                div()
                    .max_w(px(320.))
                    .text_center()
                    .text_size(text_px(11.5))
                    .text_color(colors.text_tertiary)
                    .child(detail),
            )
        })
}

/// A square icon tile with a dithered fill rising from the bottom, used
/// for list rows such as accounts, notifications, and pull requests.
pub fn dither_icon_tile(icon: impl Into<Icon>, tone: Hsla, size: f32, animated: bool) -> Div {
    let radius = (size * 0.27).round();
    let dither = Dither::new(DitherField::Fade {
        from: Edge::Bottom,
        start: 0.8,
        end: 0.0,
    })
    .color(tone.opacity(0.55))
    .grid(2.0, 1.0)
    .corner_radius(radius);
    div()
        .relative()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .overflow_hidden()
        .border_1()
        .border_color(tone.opacity(0.35))
        .bg(tone.opacity(0.08))
        .child(if animated { dither.animated() } else { dither }.layer())
        .child(
            div()
                .relative()
                .child(icon.into().size(px((size * 0.5).round())).text_color(tone)),
        )
}

/// A compact empty state for narrow panels such as the sidebar.
pub fn dither_empty_hint(text: impl Into<SharedString>, cx: &App) -> Div {
    let colors = theme::liquid_colors(cx);
    let text: SharedString = text.into();
    v_flex()
        .w_full()
        .items_center()
        .gap_2()
        .py_6()
        .child(
            div().relative().w(px(84.)).h(px(28.)).child(
                Dither::new(
                    DitherField::Glow {
                        x: 0.5,
                        y: 0.5,
                        radius: 0.55,
                        strength: 0.8,
                    }
                    .times(DitherField::Flow {
                        scale: 0.25,
                        speed: 1.5,
                        bias: 0.1,
                    }),
                )
                .color(colors.text_tertiary)
                .grid(3.0, 2.0)
                .layer(),
            ),
        )
        .child(
            div()
                .font_family(mono_font())
                .text_size(text_px(11.))
                .text_color(colors.text_tertiary)
                .child(text),
        )
}

/// A short animated strip that shows that work is in progress.
pub fn dither_loader(label: impl Into<SharedString>, cx: &App) -> Div {
    let label: SharedString = label.into();
    h_flex()
        .w_full()
        .gap_3()
        .px_3()
        .py_3()
        .items_center()
        .child(
            div()
                .relative()
                .w(px(72.))
                .h(px(12.))
                .flex_shrink_0()
                .child(
                    Dither::new(DitherField::Flow {
                        scale: 0.22,
                        speed: 6.0,
                        bias: -0.12,
                    })
                    .color(cx.theme().primary)
                    .grid(3.0, 2.0)
                    .animated()
                    .layer(),
                ),
        )
        .child(
            div()
                .font_family(mono_font())
                .text_size(text_px(11.))
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
}
