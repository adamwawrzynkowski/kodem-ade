//! The diff viewer: one file's changes as a virtual list of rows with syntax colors, line
//! numbers, whitespace markers and optional word wrap, an overview ruler beside the text, and
//! side-by-side comparisons of images, binary files and Git LFS objects.
use std::rc::Rc;
use std::sync::Arc;

use gpui::ColorExt as _;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, Div, EventEmitter, Hsla, Image, ImageFormat, IntoElement,
    ObjectFit, Pixels, Render, ScrollStrategy, ScrollWheelEvent, SharedString, StyledText, TextRun,
    Window, canvas, div, fill, font, img, point, size,
};
use kodem_core::Settings;
use kodem_git::git::{
    DiffLineKind, FileComparison, FileDiff, FileRevision, LfsPointer, PreviewImageFormat,
};

use crate::theme::{RADIUS_CONTROL, code_px, px, text_px};
use crate::ui::dither::{BarSegment, Dither, DitherBar, DitherField, DottedRule, Edge};
use crate::ui::git::components::{
    button, dither_empty_hint, dither_empty_state, file_path_label, mono_font, mono_label,
    tinted_badge,
};
use crate::ui::git::icons;
use crate::ui::git::kit::{
    ActiveTheme as _, ButtonVariants as _, IconName, Selectable as _, Sizable as _,
    VirtualListScrollHandle, h_flex, v_flex, v_virtual_list,
};
use crate::ui::git::settings::GitSettings;
use crate::ui::git::syntax::{DiffSyntaxHighlighter, SyntaxSpan};
use crate::ui::git::theme;
use crate::ui::kit;
use crate::ui::overview_ruler::{
    OverviewRuler, RowMark, RulerPalette, RulerScroll, ScrollMetrics, render_ruler,
};

/// Returns the size of diff text: Kodem's code size.
fn diff_font_size() -> Pixels {
    code_px(Settings::CODE_FONT_SIZE)
}

/// Returns the height of one diff row: the code size with room above and below.
fn diff_row_height() -> Pixels {
    diff_font_size() + px(9.)
}

#[derive(Clone)]
struct FormattedDiffText {
    text: SharedString,
    syntax: Vec<SyntaxSpan>,
}

fn format_diff_text(
    text: &str,
    syntax: &[SyntaxSpan],
    tab_width: usize,
    show_whitespace: bool,
) -> FormattedDiffText {
    // Without a tab or shown whitespace the text and its spans stay as they are.
    if !show_whitespace && !text.contains('\t') {
        return FormattedDiffText {
            text: SharedString::from(text.to_owned()),
            syntax: syntax
                .iter()
                .filter(|span| span.range.end <= text.len())
                .cloned()
                .collect(),
        };
    }
    let mut output = String::with_capacity(text.len());
    let mut offsets = vec![0; text.len() + 1];
    for (source_start, character) in text.char_indices() {
        offsets[source_start] = output.len();
        match character {
            '\t' if show_whitespace => {
                output.push('→');
                output.extend(std::iter::repeat_n('·', tab_width.saturating_sub(1)));
            }
            '\t' => output.extend(std::iter::repeat_n(' ', tab_width)),
            ' ' if show_whitespace => output.push('·'),
            _ => output.push(character),
        }
        offsets[source_start + character.len_utf8()] = output.len();
    }
    let syntax = syntax
        .iter()
        .filter(|span| span.range.end <= text.len())
        .map(|span| SyntaxSpan {
            range: offsets[span.range.start]..offsets[span.range.end],
            kind: span.kind,
        })
        .collect();
    FormattedDiffText {
        text: output.into(),
        syntax,
    }
}

fn format_byte_count(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes < KIB {
        format!("{} B", bytes as u64)
    } else if bytes < MIB {
        format!("{:.1} KB", bytes / KIB)
    } else if bytes < GIB {
        format!("{:.1} MB", bytes / MIB)
    } else {
        format!("{:.1} GB", bytes / GIB)
    }
}

fn format_signed_byte_count(bytes: i128) -> String {
    let sign = if bytes >= 0 { "+" } else { "-" };
    format!("{sign}{}", format_byte_count(bytes.unsigned_abs() as u64))
}

fn gpui_image_format(format: PreviewImageFormat) -> ImageFormat {
    match format {
        PreviewImageFormat::Png => ImageFormat::Png,
        PreviewImageFormat::Jpeg => ImageFormat::Jpeg,
        PreviewImageFormat::Webp => ImageFormat::Webp,
        PreviewImageFormat::Gif => ImageFormat::Gif,
        PreviewImageFormat::Svg => ImageFormat::Svg,
        PreviewImageFormat::Bmp => ImageFormat::Bmp,
        PreviewImageFormat::Tiff => ImageFormat::Tiff,
        PreviewImageFormat::Ico => ImageFormat::Ico,
        PreviewImageFormat::Pnm => ImageFormat::Pnm,
    }
}

#[derive(Clone)]
struct PreviewRevision {
    image: Option<Arc<Image>>,
    dimensions: Option<(u32, u32)>,
    logical_byte_count: Option<u64>,
    decoded_byte_count: Option<u64>,
    lfs_pointer: Option<LfsPointer>,
    load_error: Option<SharedString>,
}

impl From<&FileRevision> for PreviewRevision {
    fn from(revision: &FileRevision) -> Self {
        let image = revision
            .image_bytes
            .as_ref()
            .zip(revision.image_format)
            .map(|(bytes, format)| {
                Arc::new(Image::from_bytes(gpui_image_format(format), bytes.clone()))
            });
        Self {
            image,
            dimensions: revision.dimensions,
            logical_byte_count: revision.logical_byte_count,
            decoded_byte_count: revision.decoded_byte_count,
            lfs_pointer: revision.lfs_pointer.clone(),
            load_error: revision.load_error.clone().map(SharedString::from),
        }
    }
}

#[derive(Clone)]
struct PreviewComparison {
    before: Option<PreviewRevision>,
    after: Option<PreviewRevision>,
    is_image: bool,
}

impl From<&FileComparison> for PreviewComparison {
    fn from(comparison: &FileComparison) -> Self {
        Self {
            before: comparison.before.as_ref().map(PreviewRevision::from),
            after: comparison.after.as_ref().map(PreviewRevision::from),
            is_image: comparison.is_image,
        }
    }
}

impl PreviewComparison {
    fn is_lfs(&self) -> bool {
        self.before
            .as_ref()
            .and_then(|revision| revision.lfs_pointer.as_ref())
            .is_some()
            || self
                .after
                .as_ref()
                .and_then(|revision| revision.lfs_pointer.as_ref())
                .is_some()
    }
}

/// Extra height that gives hunk headers room to breathe.
const HUNK_HEADER_EXTRA_HEIGHT: f32 = 10.0;

/// The parts of a `@@ -a,b +c,d @@ context` hunk header.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HunkHeaderParts {
    old_range: String,
    new_range: String,
    context: String,
}

/// Splits a unified diff hunk header into its ranges and context. Returns
/// `None` when the header does not follow the unified format.
fn parse_hunk_header(header: &str) -> Option<HunkHeaderParts> {
    let rest = header.trim().strip_prefix("@@")?;
    let (ranges, context) = rest.split_once("@@")?;
    let mut ranges = ranges.split_whitespace();
    let old_range = ranges.next()?.strip_prefix('-')?.to_string();
    let new_range = ranges.next()?.strip_prefix('+')?.to_string();
    Some(HunkHeaderParts {
        old_range,
        new_range,
        context: context.trim().to_string(),
    })
}

/// One visual row in the flattened diff.
#[derive(Clone)]
enum DiffRow {
    HunkHeader(SharedString),
    Line {
        kind: DiffLineKind,
        old_line: Option<u32>,
        new_line: Option<u32>,
        text: SharedString,
        syntax: Vec<SyntaxSpan>,
        formatted: Option<FormattedDiffText>,
    },
    Binary,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffRenderStrategy {
    Comparison,
    VirtualizedText,
}

fn diff_render_strategy(has_comparison: bool, _word_wrap: bool) -> DiffRenderStrategy {
    if has_comparison {
        DiffRenderStrategy::Comparison
    } else {
        DiffRenderStrategy::VirtualizedText
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DiffFormatKey {
    tab_width: usize,
    show_whitespace: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct DiffLayoutKey {
    format: DiffFormatKey,
    /// The text size and row height in window pixels, which follow the code size setting and
    /// the interface scale.
    font_size: f32,
    row_height: f32,
    show_numbers: bool,
    word_wrap: bool,
    viewport_width: f32,
}

/// Diff rows with syntax highlights, built away from the UI thread.
/// Highlighting a large file can take a long time, so build this on a
/// background thread and give it to [`DiffView::set_diff`].
pub struct PreparedDiff {
    title: SharedString,
    file_format: SharedString,
    comparison: Option<PreviewComparison>,
    language: Option<SharedString>,
    rows: Vec<DiffRow>,
}

impl PreparedDiff {
    /// Builds the rows and syntax highlights of one file diff.
    pub fn new(title: impl Into<SharedString>, diff: &FileDiff) -> Self {
        let title: SharedString = title.into();
        let file_format = std::path::Path::new(title.as_ref())
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_uppercase())
            .unwrap_or_else(|| "FILE".to_string())
            .into();
        let first_line = diff
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .find(|line| line.old_line == Some(1) || line.new_line == Some(1))
            .map(|line| line.text.as_str());
        let mut highlighter = DiffSyntaxHighlighter::detect(title.as_ref(), first_line);
        let language = highlighter.as_ref().map(DiffSyntaxHighlighter::language);
        let mut rows = Vec::new();
        if diff.is_binary {
            rows.push(DiffRow::Binary);
        } else if diff.hunks.is_empty() {
            rows.push(DiffRow::Empty);
        } else {
            for hunk in &diff.hunks {
                if let Some(highlighter) = highlighter.as_mut() {
                    highlighter.begin_hunk();
                }
                rows.push(DiffRow::HunkHeader(hunk.header.clone().into()));
                for line in &hunk.lines {
                    let syntax = highlighter
                        .as_mut()
                        .map(|highlighter| highlighter.highlight_line(line.kind, &line.text))
                        .unwrap_or_default();
                    rows.push(DiffRow::Line {
                        kind: line.kind,
                        old_line: line.old_line,
                        new_line: line.new_line,
                        text: line.text.clone().into(),
                        syntax,
                        formatted: None,
                    });
                }
            }
        }
        Self {
            title,
            file_format,
            comparison: diff.comparison.as_ref().map(PreviewComparison::from),
            language,
            rows,
        }
    }
}

/// Emitted when the user clicks the close button.
pub struct DiffCloseRequested;

impl EventEmitter<DiffCloseRequested> for DiffView {}

/// Renders one file diff with old/new gutters.
#[derive(Default)]
pub struct DiffView {
    /// The path of the file whose diff shows.
    pub title: SharedString,
    /// Whether the header shows a close button, which emits [`DiffCloseRequested`].
    pub closable: bool,
    rows: Vec<DiffRow>,
    scroll: VirtualListScrollHandle,
    comparison: Option<PreviewComparison>,
    file_format: SharedString,
    language: Option<SharedString>,
    format_key: Option<DiffFormatKey>,
    layout_key: Option<DiffLayoutKey>,
    item_sizes: Rc<Vec<gpui::Size<Pixels>>>,
    viewport_width: f32,
    /// How far unwrapped rows scroll to the left, so long lines can be read.
    scroll_x: Pixels,
    /// The width of the longest unwrapped row, which bounds `scroll_x`.
    content_width: Pixels,
    /// The overview ruler beside the text.
    ruler: OverviewRuler,
}

impl RulerScroll for DiffView {
    fn ruler(&self) -> &OverviewRuler {
        &self.ruler
    }

    fn scroll_metrics(&self) -> ScrollMetrics {
        ScrollMetrics {
            offset: -f32::from(self.scroll.offset().y),
            max: f32::from(self.scroll.max_offset().y),
            viewport: f32::from(self.scroll.bounds().size.height),
        }
    }

    fn set_scroll_offset(&mut self, offset: f32, cx: &mut Context<Self>) {
        let current = self.scroll.offset();
        self.scroll.set_offset(point(current.x, gpui::px(-offset)));
        cx.notify();
    }
}

impl DiffView {
    /// Creates an empty diff view.
    pub fn new() -> Self {
        Self::default()
    }

    /// Shows a diff that [`PreparedDiff::new`] built.
    pub fn set_diff(&mut self, prepared: PreparedDiff, cx: &mut Context<Self>) {
        self.title = prepared.title;
        self.file_format = prepared.file_format;
        self.comparison = prepared.comparison;
        self.language = prepared.language;
        self.rows = prepared.rows;
        self.scroll_x = Pixels::ZERO;
        self.format_key = None;
        self.layout_key = None;
        self.item_sizes = Rc::new(Vec::new());
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// Clears the diff and shows the empty hint.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.title = SharedString::default();
        self.rows.clear();
        self.scroll_x = Pixels::ZERO;
        self.comparison = None;
        self.file_format = SharedString::default();
        self.language = None;
        self.format_key = None;
        self.layout_key = None;
        self.item_sizes = Rc::new(Vec::new());
        cx.notify();
    }

    fn prepare_formatted_rows(&mut self, key: DiffFormatKey) {
        if self.format_key == Some(key) {
            return;
        }

        for row in &mut self.rows {
            if let DiffRow::Line {
                text,
                syntax,
                formatted,
                ..
            } = row
            {
                *formatted = Some(format_diff_text(
                    text,
                    syntax,
                    key.tab_width,
                    key.show_whitespace,
                ));
            }
        }
        self.format_key = Some(key);
        self.layout_key = None;
    }

    fn rebuild_layout_cache(&mut self, window: &mut Window, cx: &App) {
        let settings = GitSettings::get(cx);
        let format = DiffFormatKey {
            tab_width: settings.tab_width.max(1) as usize,
            show_whitespace: settings.show_whitespace,
        };
        let viewport_width = if self.viewport_width > 0.0 {
            self.viewport_width.round()
        } else {
            800.0
        };
        let key = DiffLayoutKey {
            format,
            font_size: f32::from(diff_font_size()),
            row_height: f32::from(diff_row_height()),
            show_numbers: settings.show_line_numbers,
            word_wrap: settings.diff_word_wrap,
            viewport_width: if settings.diff_word_wrap {
                viewport_width
            } else {
                0.0
            },
        };
        if self.layout_key == Some(key) && self.item_sizes.len() == self.rows.len() {
            return;
        }

        self.prepare_formatted_rows(format);
        let row_height = gpui::px(key.row_height);
        // The gutter, prefix and padding are interface lengths; the viewport is window pixels.
        let gutter_width = if key.show_numbers {
            f32::from(px(84.0))
        } else {
            0.0
        };
        let text_width =
            gpui::px((viewport_width - gutter_width - f32::from(px(18.0 + 14.0))).max(64.0));
        let text_system = window.text_system();
        let sizes = self
            .rows
            .iter()
            .map(|row| {
                let height = if matches!(row, DiffRow::HunkHeader(_)) {
                    row_height + px(HUNK_HEADER_EXTRA_HEIGHT)
                } else if key.word_wrap {
                    match row {
                        DiffRow::Line {
                            formatted: Some(formatted),
                            ..
                        } if !formatted.text.is_empty() => text_system
                            .shape_text(
                                formatted.text.clone(),
                                gpui::px(key.font_size),
                                &[TextRun {
                                    len: formatted.text.len(),
                                    font: font(mono_font()),
                                    ..Default::default()
                                }],
                                Some(text_width),
                                None,
                            )
                            .ok()
                            .map(|lines| {
                                lines.iter().fold(Pixels::ZERO, |height, line| {
                                    height + line.size(row_height).height
                                })
                            })
                            .unwrap_or(row_height),
                        _ => row_height,
                    }
                } else {
                    row_height
                };
                size(gpui::px(100.0), height.max(row_height))
            })
            .collect();
        self.item_sizes = Rc::new(sizes);
        // Unwrapped rows scroll sideways as far as the longest line reaches. Diff text is mono,
        // so the line with the most characters is the widest one.
        self.content_width = if key.word_wrap {
            Pixels::ZERO
        } else {
            self.rows
                .iter()
                .filter_map(|row| match row {
                    DiffRow::Line {
                        formatted: Some(formatted),
                        ..
                    } => Some(&formatted.text),
                    _ => None,
                })
                .max_by_key(|text| text.chars().count())
                .map(|text| {
                    let line = text_system.shape_line(
                        text.clone(),
                        gpui::px(key.font_size),
                        &[TextRun {
                            len: text.len(),
                            font: font(mono_font()),
                            ..Default::default()
                        }],
                        None,
                    );
                    line.width + gpui::px(gutter_width) + px(18.0 + 14.0)
                })
                .unwrap_or_default()
        };
        self.layout_key = Some(key);
    }

    /// Returns how far unwrapped rows can scroll to the left.
    fn max_scroll_x(&self) -> Pixels {
        (self.content_width - gpui::px(self.viewport_width)).max(Pixels::ZERO)
    }

    /// Scrolls unwrapped rows sideways with the horizontal part of a scroll gesture. The list
    /// itself scrolls only vertically.
    fn scroll_horizontally(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height()).x;
        if delta == Pixels::ZERO {
            return;
        }
        let can_move = if delta > Pixels::ZERO {
            self.scroll_x > Pixels::ZERO
        } else {
            self.scroll_x < self.max_scroll_x()
        };
        let delta = window
            .take_scroll_delta(
                self.scroll.scroll_container_id(),
                point(delta, Pixels::ZERO),
                point(can_move, false),
            )
            .x;
        if delta == Pixels::ZERO {
            return;
        }
        let offset = (self.scroll_x - delta)
            .min(self.max_scroll_x())
            .max(Pixels::ZERO);
        if offset != self.scroll_x {
            self.scroll_x = offset;
            cx.notify();
        }
    }

    /// Builds a row, shifted to the left by the horizontal scroll and widened so that its fill
    /// still reaches the right edge.
    fn render_scrolled_row(&self, ix: usize, cx: &App) -> Div {
        let row = self.render_row(ix, cx);
        if self.scroll_x > Pixels::ZERO {
            row.ml(-self.scroll_x)
                .w(gpui::px(self.viewport_width) + self.scroll_x)
        } else {
            row
        }
    }

    /// Returns what each row shows, in order.
    fn row_marks(&self) -> Vec<RowMark> {
        self.rows
            .iter()
            .map(|row| match row {
                DiffRow::HunkHeader(_) => RowMark::Hunk,
                DiffRow::Line {
                    kind: DiffLineKind::Added,
                    ..
                } => RowMark::Added,
                DiffRow::Line {
                    kind: DiffLineKind::Removed,
                    ..
                } => RowMark::Removed,
                _ => RowMark::Context,
            })
            .collect()
    }

    fn render_row(&self, ix: usize, cx: &App) -> Div {
        let settings = GitSettings::get(cx);
        let show_numbers = settings.show_line_numbers;
        let word_wrap = settings.diff_word_wrap;
        let colors = theme::liquid_colors(cx);
        let row_h = diff_row_height();
        let rendered_row_h = self
            .item_sizes
            .get(ix)
            .map(|size| size.height)
            .unwrap_or(row_h);
        let gutter_w = 42.0;

        let base = h_flex()
            .w_full()
            .h(rendered_row_h)
            .min_h(rendered_row_h)
            .items_start()
            .font_family(mono_font())
            .text_size(diff_font_size())
            .line_height(row_h);

        match &self.rows[ix] {
            DiffRow::HunkHeader(header) => {
                let accent = cx.theme().primary;
                let content = match parse_hunk_header(header) {
                    Some(parts) => h_flex()
                        .relative()
                        .gap_1p5()
                        .items_center()
                        .child(hunk_range_chip(
                            format!("−{}", parts.old_range),
                            colors.danger,
                        ))
                        .child(hunk_range_chip(
                            format!("+{}", parts.new_range),
                            colors.success,
                        ))
                        .when(!parts.context.is_empty(), |row| {
                            row.child(
                                div()
                                    .pl_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_color(colors.text_secondary)
                                    .child(parts.context),
                            )
                        }),
                    None => h_flex().relative().text_color(accent).child(header.clone()),
                };
                base.relative()
                    .items_center()
                    .px_2()
                    .bg(accent.opacity(0.05))
                    .child(
                        Dither::new(DitherField::Fade {
                            from: Edge::Left,
                            start: 0.5,
                            end: -0.2,
                        })
                        .color(accent.opacity(0.4))
                        .grid(2.0, 1.0)
                        .layer(),
                    )
                    .child(content)
            }
            DiffRow::Binary => base
                .px_2()
                .text_color(cx.theme().muted_foreground)
                .child("Binary file. No text diff available."),
            DiffRow::Empty => base
                .px_2()
                .text_color(cx.theme().muted_foreground)
                .child("No changes."),
            DiffRow::Line {
                kind,
                old_line,
                new_line,
                formatted,
                ..
            } => {
                let (bg, prefix, prefix_color) = match kind {
                    DiffLineKind::Added => (Some(colors.addition_fill), "+", colors.success),
                    DiffLineKind::Removed => (Some(colors.deletion_fill), "-", colors.danger),
                    DiffLineKind::Context => (None, " ", cx.theme().muted_foreground),
                };
                let formatted = formatted
                    .as_ref()
                    .expect("diff rows are prepared before rendering");
                let code_theme = cx.theme().kodem;
                let highlights = formatted.syntax.iter().filter_map(move |span| {
                    Some((span.range.clone(), span.kind.style(&code_theme)?))
                });
                let marker = match kind {
                    DiffLineKind::Added => Some(colors.success),
                    DiffLineKind::Removed => Some(colors.danger),
                    DiffLineKind::Context => None,
                };
                base.relative()
                    .when_some(bg, |this, bg| this.bg(bg))
                    .when_some(marker, |this, marker| {
                        // A solid two-point edge in the line's color marks added and removed
                        // lines.
                        this.child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(2.))
                                .bg(marker),
                        )
                    })
                    .when(show_numbers, |this| {
                        // The gutter is its own column, tinted by line kind,
                        // so changed lines stand out even when scrolled.
                        let (gutter_bg, number_color) = match kind {
                            DiffLineKind::Added => {
                                (colors.success.opacity(0.13), colors.success.opacity(0.85))
                            }
                            DiffLineKind::Removed => {
                                (colors.danger.opacity(0.13), colors.danger.opacity(0.85))
                            }
                            DiffLineKind::Context => {
                                (colors.panel_secondary, colors.text_tertiary.opacity(0.8))
                            }
                        };
                        this.child(
                            h_flex()
                                .flex_shrink_0()
                                .h_full()
                                .w(px(gutter_w * 2.0))
                                .items_start()
                                .bg(gutter_bg)
                                .border_r_1()
                                .border_color(colors.separator)
                                .text_color(number_color)
                                .child(
                                    div()
                                        .w(px(gutter_w))
                                        .pr_1()
                                        .text_right()
                                        .child(old_line.map(|n| n.to_string()).unwrap_or_default()),
                                )
                                .child(
                                    div()
                                        .w(px(gutter_w))
                                        .pr_1p5()
                                        .text_right()
                                        .child(new_line.map(|n| n.to_string()).unwrap_or_default()),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(18.))
                            .text_color(prefix_color)
                            .text_center()
                            .child(prefix),
                    )
                    .child(
                        div()
                            .flex_1()
                            .when(word_wrap, |this| this.min_w_0().whitespace_normal())
                            .when(!word_wrap, |this| this.whitespace_nowrap())
                            .child(
                                StyledText::new(formatted.text.clone()).with_highlights(highlights),
                            ),
                    )
            }
        }
    }

    fn size_summary(&self, comparison: &PreviewComparison) -> Option<(String, Option<i128>)> {
        let before = comparison
            .before
            .as_ref()
            .and_then(|revision| revision.logical_byte_count);
        let after = comparison
            .after
            .as_ref()
            .and_then(|revision| revision.logical_byte_count);
        match (before, after) {
            (Some(before), Some(after)) if before != after => Some((
                format!(
                    "{} → {}",
                    format_byte_count(before),
                    format_byte_count(after)
                ),
                Some(after as i128 - before as i128),
            )),
            (Some(before), Some(_)) => Some((format_byte_count(before), None)),
            (Some(before), None) => Some((format_byte_count(before), None)),
            (None, Some(after)) => Some((format_byte_count(after), None)),
            (None, None) => None,
        }
    }

    fn oid_summary(&self, comparison: &PreviewComparison) -> Option<String> {
        let before = comparison
            .before
            .as_ref()
            .and_then(|revision| revision.lfs_pointer.as_ref())
            .map(LfsPointer::short_oid);
        let after = comparison
            .after
            .as_ref()
            .and_then(|revision| revision.lfs_pointer.as_ref())
            .map(LfsPointer::short_oid);
        match (before, after) {
            (Some(before), Some(after)) if before != after => Some(format!("{before} → {after}")),
            (Some(before), Some(_)) | (Some(before), None) => Some(before),
            (None, Some(after)) => Some(after),
            (None, None) => None,
        }
    }

    fn summary_item(label: &'static str, value: impl Into<SharedString>, cx: &App) -> Div {
        h_flex()
            .gap_1p5()
            .flex_shrink_0()
            .items_center()
            .child(mono_label(label, cx))
            .child(
                div()
                    .text_size(text_px(11.5))
                    .font_family(mono_font())
                    .text_color(cx.theme().foreground)
                    .child(value.into()),
            )
    }

    fn summary_divider(cx: &App) -> Div {
        div()
            .mx_2()
            .w(px(1.))
            .h(px(14.))
            .flex_shrink_0()
            .child(DottedRule::vertical(
                theme::liquid_colors(cx).text_tertiary.opacity(0.4),
            ))
    }

    fn checkerboard(cx: &App) -> Div {
        let base = if cx.theme().is_dark() {
            gpui::hsla(0.0, 0.0, 0.13, 1.0)
        } else {
            gpui::hsla(0.0, 0.0, 0.93, 1.0)
        };
        let alternate = if cx.theme().is_dark() {
            gpui::hsla(0.0, 0.0, 0.18, 1.0)
        } else {
            gpui::hsla(0.0, 0.0, 0.85, 1.0)
        };
        div().absolute().inset_0().child(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window, _| {
                    window.paint_quad(fill(bounds, base));
                    let square = f32::from(px(10.0));
                    let columns = (f32::from(bounds.size.width) / square).ceil() as usize;
                    let rows = (f32::from(bounds.size.height) / square).ceil() as usize;
                    for row in 0..rows {
                        for column in 0..columns {
                            if (row + column) % 2 == 0 {
                                let square_bounds = Bounds::new(
                                    point(
                                        bounds.origin.x + gpui::px(column as f32 * square),
                                        bounds.origin.y + gpui::px(row as f32 * square),
                                    ),
                                    size(gpui::px(square), gpui::px(square)),
                                );
                                window.paint_quad(fill(square_bounds, alternate));
                            }
                        }
                    }
                },
            )
            .size_full(),
        )
    }

    fn render_revision_metadata(&self, revision: &PreviewRevision, cx: &App) -> Div {
        let colors = theme::liquid_colors(cx);
        h_flex()
            .w_full()
            .min_h(px(28.))
            .px_2()
            .gap_2()
            .overflow_hidden()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_size(text_px(11.5))
            .font_family(mono_font())
            .text_color(cx.theme().muted_foreground)
            .when_some(revision.logical_byte_count, |this, bytes| {
                this.child(format_byte_count(bytes))
            })
            .when_some(
                revision
                    .decoded_byte_count
                    .filter(|_| revision.lfs_pointer.is_some()),
                |this, bytes| {
                    this.child(Self::summary_divider(cx)).child(
                        div()
                            .text_color(colors.text_tertiary)
                            .child(format!("Decoded {}", format_byte_count(bytes))),
                    )
                },
            )
            .when_some(revision.lfs_pointer.as_ref(), |this, pointer| {
                this.child(Self::summary_divider(cx)).child(
                    div()
                        .text_color(cx.theme().primary)
                        .child(pointer.short_oid()),
                )
            })
    }

    fn render_revision_pane(
        &self,
        label: &'static str,
        revision: Option<&PreviewRevision>,
        before: bool,
        is_image: bool,
        cx: &App,
    ) -> Div {
        let colors = theme::liquid_colors(cx);
        let accent = if before {
            colors.danger
        } else {
            colors.success
        };
        let dimensions = revision
            .and_then(|revision| revision.dimensions)
            .map(|(width, height)| format!("{width} × {height}"));
        let content = match revision {
            None => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(cx.theme().muted_foreground)
                .child(if before {
                    "No previous version"
                } else {
                    "File deleted"
                })
                .into_any_element(),
            Some(revision) if is_image && revision.image.is_some() => {
                let image = revision.image.clone().expect("image checked above");
                div()
                    .relative()
                    .size_full()
                    .overflow_hidden()
                    .child(Self::checkerboard(cx))
                    .child(
                        img(image)
                            .absolute()
                            .inset_0()
                            .p_4()
                            .object_fit(ObjectFit::ScaleDown),
                    )
                    .into_any_element()
            }
            Some(revision) if is_image => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .px_4()
                .text_center()
                .text_color(colors.warning)
                .child(
                    revision
                        .load_error
                        .clone()
                        .unwrap_or_else(|| "Image data could not be decoded.".into()),
                )
                .into_any_element(),
            Some(revision) => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_3()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded(px(RADIUS_CONTROL))
                        .bg(cx.theme().primary.opacity(0.14))
                        .text_color(cx.theme().primary)
                        .font_family(mono_font())
                        .text_size(text_px(11.5))
                        .child(if revision.lfs_pointer.is_some() {
                            "LFS object"
                        } else {
                            "Binary file"
                        }),
                )
                .when_some(revision.logical_byte_count, |this, bytes| {
                    this.child(
                        div()
                            .font_family(mono_font())
                            .text_size(text_px(12.5))
                            .text_color(cx.theme().foreground)
                            .child(format_byte_count(bytes)),
                    )
                })
                .when_some(revision.load_error.clone(), |this, error| {
                    this.child(
                        div()
                            .px_4()
                            .text_center()
                            .text_size(text_px(11.5))
                            .text_color(colors.warning)
                            .child(error),
                    )
                })
                .into_any_element(),
        };

        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                h_flex()
                    .w_full()
                    .h(px(28.))
                    .flex_shrink_0()
                    .px_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().size(px(6.)).rounded(px(1.)).bg(accent))
                    .child(mono_label(label, cx))
                    .child(div().flex_1())
                    .when_some(dimensions, |this, dimensions| {
                        this.child(
                            div()
                                .font_family(mono_font())
                                .text_size(text_px(11.5))
                                .text_color(cx.theme().muted_foreground)
                                .child(dimensions),
                        )
                    }),
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .when_some(revision, |this, revision| {
                this.child(self.render_revision_metadata(revision, cx))
            })
    }

    fn render_comparison(&self, comparison: &PreviewComparison, cx: &App) -> gpui::AnyElement {
        let colors = theme::liquid_colors(cx);
        let size_summary = self.size_summary(comparison);
        let oid_summary = self.oid_summary(comparison);
        v_flex()
            .size_full()
            .min_h_0()
            .child(
                h_flex()
                    .w_full()
                    .h(px(32.))
                    .flex_shrink_0()
                    .px_3()
                    .items_center()
                    .overflow_hidden()
                    .bg(colors.panel_secondary)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(Self::summary_item("Format", self.file_format.clone(), cx))
                    .when_some(size_summary, |this, (value, delta)| {
                        this.child(Self::summary_divider(cx))
                            .child(Self::summary_item("Size", value, cx))
                            .when_some(delta, |this, delta| {
                                this.child(
                                    div()
                                        .ml_1()
                                        .font_family(mono_font())
                                        .text_size(text_px(11.5))
                                        .text_color(if delta > 0 {
                                            colors.danger
                                        } else {
                                            colors.success
                                        })
                                        .child(format!("({})", format_signed_byte_count(delta))),
                                )
                            })
                    })
                    .when_some(oid_summary, |this, oid| {
                        this.child(Self::summary_divider(cx))
                            .child(Self::summary_item("OID", oid, cx))
                    }),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_revision_pane(
                        "Before",
                        comparison.before.as_ref(),
                        true,
                        comparison.is_image,
                        cx,
                    ))
                    .child(
                        div()
                            .w(px(1.))
                            .h_full()
                            .flex_shrink_0()
                            .bg(cx.theme().border),
                    )
                    .child(self.render_revision_pane(
                        "After",
                        comparison.after.as_ref(),
                        false,
                        comparison.is_image,
                        cx,
                    )),
            )
            .into_any_element()
    }
}

/// A mono chip with one side of a hunk range, for example `+12,9`.
fn hunk_range_chip(label: String, color: Hsla) -> Div {
    tinted_badge(div().flex_shrink_0().px_1(), color, 0.1).child(label)
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.rebuild_layout_cache(window, cx);
        // The view may have grown since the last scroll.
        self.scroll_x = self.scroll_x.min(self.max_scroll_x());
        let sizes = self.item_sizes.clone();
        let title = self.title.clone();
        let has_close = self.closable;
        let comparison = self.comparison.clone();
        let language = self.language.clone();
        let word_wrap = GitSettings::get(cx).diff_word_wrap;
        let known_viewport_width = self.viewport_width;
        let diff_view = cx.entity();
        let is_empty = self.rows.is_empty() && comparison.is_none();
        let is_binary = matches!(self.rows.as_slice(), [DiffRow::Binary]);
        let has_no_changes = matches!(self.rows.as_slice(), [DiffRow::Empty]);
        let marks: Rc<[RowMark]> = self.row_marks().into();
        let additions = marks.iter().filter(|mark| **mark == RowMark::Added).count();
        let deletions = marks
            .iter()
            .filter(|mark| **mark == RowMark::Removed)
            .count();
        let settings = GitSettings::get(cx);
        let show_numbers = settings.show_line_numbers;
        let show_whitespace = settings.show_whitespace;
        let show_text_tools = comparison.is_none() && !is_empty && !is_binary;
        let colors = theme::liquid_colors(cx);
        let body = match diff_render_strategy(comparison.is_some(), word_wrap) {
            _ if is_empty => dither_empty_hint("Select a file to see its diff", cx)
                .size_full()
                .justify_center()
                .into_any_element(),
            _ if is_binary => dither_empty_state(
                "Binary file",
                "Git cannot show a text diff for this file.",
                DitherField::Glow {
                    x: 0.5,
                    y: 0.5,
                    radius: 0.5,
                    strength: 0.9,
                }
                .times(DitherField::Flow {
                    scale: 0.18,
                    speed: 1.2,
                    bias: 0.1,
                }),
                cx,
            )
            .into_any_element(),
            _ if has_no_changes => dither_empty_state(
                "No changes",
                "This file has no line changes. Only its mode or metadata changed.",
                DitherField::Wave {
                    level: 0.3,
                    amplitude: 0.04,
                    speed: 1.0,
                },
                cx,
            )
            .into_any_element(),
            DiffRenderStrategy::Comparison => self.render_comparison(
                comparison
                    .as_ref()
                    .expect("comparison strategy requires comparison data"),
                cx,
            ),
            DiffRenderStrategy::VirtualizedText => v_flex()
                .id("diff-body")
                .relative()
                .size_full()
                .when(word_wrap, |this| this.overflow_x_hidden())
                .child({
                    let mut list =
                        v_virtual_list(cx.entity(), "diff-rows", sizes, |this, range, _, cx| {
                            range.map(|ix| this.render_scrolled_row(ix, cx)).collect()
                        })
                        .track_scroll(&self.scroll);
                    // A sideways gesture scrolls the rows sideways, not the list down.
                    list.style().restrict_scroll_to_axis = Some(true);
                    list
                })
                // The overview ruler replaces the vertical scrollbar; sideways scrolling of
                // unwrapped rows reaches the ends of long lines.
                .when(!word_wrap, |this| {
                    this.on_scroll_wheel(cx.listener(
                        |this, event: &ScrollWheelEvent, window, cx| {
                            this.scroll_horizontally(event, window, cx);
                        },
                    ))
                })
                // Measures the text area, so wrapped rows know the width they wrap at.
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            let width = f32::from(bounds.size.width);
                            if (width - known_viewport_width).abs() <= 1.0 {
                                return;
                            }
                            cx.defer(move |cx| {
                                diff_view.update(cx, |this, cx| {
                                    if (this.viewport_width - width).abs() > 1.0 {
                                        this.viewport_width = width;
                                        if word_wrap {
                                            this.layout_key = None;
                                            cx.notify();
                                        }
                                    }
                                });
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .into_any_element(),
        };
        let ruler = show_text_tools.then(|| {
            let colors = theme::liquid_colors(cx);
            let palette = RulerPalette {
                added: colors.success,
                removed: colors.danger,
                hunk: cx.theme().primary,
                context: colors.ink,
                viewport: cx.theme().foreground,
                background: colors.panel_secondary,
                border: cx.theme().border,
            };
            render_ruler(self, marks.clone(), palette, cx)
        });

        v_flex()
            .size_full()
            .bg(cx.theme().colors.list)
            .overflow_hidden()
            .child(
                h_flex()
                    .w_full()
                    .h(px(34.))
                    .flex_shrink_0()
                    .pl_3()
                    .pr_1p5()
                    .gap_1p5()
                    .items_center()
                    .bg(theme::liquid_colors(cx).panel_secondary)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(file_path_label(&title, cx))
                    .when(show_text_tools && additions + deletions > 0, |this| {
                        this.child(
                            h_flex()
                                .flex_shrink_0()
                                .gap_1p5()
                                .items_center()
                                .font_family(mono_font())
                                .text_size(text_px(11.))
                                .child(
                                    div()
                                        .text_color(colors.success)
                                        .child(format!("+{additions}")),
                                )
                                .child(
                                    div()
                                        .text_color(colors.danger)
                                        .child(format!("−{deletions}")),
                                )
                                .child(div().relative().w(px(48.)).h(px(8.)).child(
                                    DitherBar::new(
                                        vec![
                                            BarSegment {
                                                weight: additions as f32,
                                                color: colors.success,
                                            },
                                            BarSegment {
                                                weight: deletions as f32,
                                                color: colors.danger,
                                            },
                                        ],
                                        cx.theme().muted_foreground.opacity(0.25),
                                    ),
                                )),
                        )
                    })
                    .when(
                        self.comparison
                            .as_ref()
                            .is_some_and(PreviewComparison::is_lfs),
                        |this| this.child(kit::chip("LFS", cx.theme().primary)),
                    )
                    .when(self.comparison.is_some(), |this| {
                        this.child(kit::chip(
                            self.file_format.clone(),
                            cx.theme().muted_foreground,
                        ))
                    })
                    .when_some(
                        language.filter(|_| self.comparison.is_none()),
                        |this, language| {
                            this.child(kit::chip(language, cx.theme().muted_foreground))
                        },
                    )
                    .when(show_text_tools, |this| {
                        this.child(
                            h_flex()
                                .flex_shrink_0()
                                .h(px(24.))
                                .p(px(2.))
                                .gap(px(2.))
                                .items_center()
                                .rounded(px(RADIUS_CONTROL))
                                .border_1()
                                .border_color(cx.theme().border)
                                .bg(cx.theme().muted)
                                .child(
                                    button("diff-toggle-wrap")
                                        .ghost()
                                        .xsmall()
                                        .icon(icons::wrap_text())
                                        .tooltip("Word wrap")
                                        .selected(word_wrap)
                                        .on_click(|_, _, cx| {
                                            GitSettings::update(cx, |settings| {
                                                settings.diff_word_wrap = !settings.diff_word_wrap
                                            });
                                        }),
                                )
                                .child(
                                    button("diff-toggle-whitespace")
                                        .ghost()
                                        .xsmall()
                                        .icon(icons::pilcrow())
                                        .tooltip("Show whitespace")
                                        .selected(show_whitespace)
                                        .on_click(|_, _, cx| {
                                            GitSettings::update(cx, |settings| {
                                                settings.show_whitespace = !settings.show_whitespace
                                            });
                                        }),
                                )
                                .child(
                                    button("diff-toggle-line-numbers")
                                        .ghost()
                                        .xsmall()
                                        .icon(icons::list_ordered())
                                        .tooltip("Line numbers")
                                        .selected(show_numbers)
                                        .on_click(|_, _, cx| {
                                            GitSettings::update(cx, |settings| {
                                                settings.show_line_numbers =
                                                    !settings.show_line_numbers
                                            });
                                        }),
                                ),
                        )
                    })
                    .child(
                        button("copy-path")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .tooltip("Copy path")
                            .on_click({
                                let title = title.clone();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        title.to_string(),
                                    ));
                                }
                            }),
                    )
                    .when(has_close, |this| {
                        this.child(
                            button("close-diff")
                                .ghost()
                                .xsmall()
                                .icon(IconName::Close)
                                .on_click(cx.listener(|_, _, _, cx| {
                                    cx.emit(DiffCloseRequested);
                                })),
                        )
                    }),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .relative()
                            .overflow_hidden()
                            .child(body),
                    )
                    .children(ruler),
            )
    }
}
