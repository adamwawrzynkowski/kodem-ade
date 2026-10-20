//! The work trail: a run of tool calls under one summary line, with each call, its output and
//! its diff one click away, and each thought in a tray of its own between the runs.
//!
//! Runs and thoughts that follow one another without a reply between them are one stretch of
//! work. A stretch of more than one tray has a parent tray over all of them, which folds the
//! whole stretch into one line. Its trays stay foldable inside it. A stretch of one tray has no
//! parent: the tray's own line is the plate, and its calls or its thought hang in the well.
//!
//! A tray is open while its turn can still add to it and folds when the work moves on. Failed
//! calls stay visible while their run or their stretch is folded.
use super::*;
use crate::tool_output::{self, EditSummary, ToolView};
use std::{ops::Range, rc::Rc};

/// Lines of an edit shown before the rest is folded behind a control.
const DIFF_PREVIEW_LINES: usize = 40;

/// How recently a row or a call must have started for its first frame to grow it into place.
const FRESH_MS: u64 = 700;

/// The kinds of work that the tally of a stretch counts, in the order that it shows them.
const TALLY: [ToolCategory; 10] = [
    ToolCategory::Command,
    ToolCategory::Read,
    ToolCategory::Edit,
    ToolCategory::Search,
    ToolCategory::Fetch,
    ToolCategory::Agent,
    ToolCategory::Mcp,
    ToolCategory::Plan,
    ToolCategory::Other,
    ToolCategory::Thinking,
];

fn glyph(category: ToolCategory) -> &'static str {
    match category {
        ToolCategory::Command => "square-terminal",
        ToolCategory::Read => "file-text",
        ToolCategory::Edit => "pencil",
        ToolCategory::Search => "search",
        ToolCategory::Fetch => "globe",
        ToolCategory::Agent => "bot",
        ToolCategory::Plan => "list-checks",
        ToolCategory::Thinking => "sparkles",
        ToolCategory::Mcp => "layers",
        ToolCategory::Other => "code",
    }
}

fn verb(category: ToolCategory) -> &'static str {
    match category {
        ToolCategory::Command => "Run",
        ToolCategory::Read => "Read",
        ToolCategory::Edit => "Edit",
        ToolCategory::Search => "Search",
        ToolCategory::Fetch => "Fetch",
        ToolCategory::Agent => "Agent",
        ToolCategory::Plan => "Plan",
        ToolCategory::Thinking => "Thought",
        ToolCategory::Mcp => "MCP",
        ToolCategory::Other => "Tool",
    }
}

// A thought has a tray of its own, so it also ends the run of calls before it.
fn is_thought(message: &Message) -> bool {
    message.kind == MessageKind::Tool
        && message
            .tool
            .as_ref()
            .is_some_and(|tool| tool.category == ToolCategory::Thinking)
}

fn is_call(message: &Message) -> bool {
    message.kind == MessageKind::Tool && !is_thought(message)
}

// Reports whether a message started so recently that it grows into place when it first shows.
fn fresh(message: &Message) -> bool {
    now().saturating_sub(message.created) < FRESH_MS
}

// Returns the stretch of tool rows around `index`: the thoughts and the runs of calls that
// follow one another without another message between them.
fn work_rows(transcript: &[Message], index: usize) -> Range<usize> {
    let mut start = index.min(transcript.len());
    while start > 0 && transcript[start - 1].kind == MessageKind::Tool {
        start -= 1;
    }
    let mut end = index.min(transcript.len());
    while end < transcript.len() && transcript[end].kind == MessageKind::Tool {
        end += 1;
    }
    start..end
}

// Returns the rows of a stretch where a shown tray starts, in order. Under a folded parent only
// the runs with a failed call are shown. `views` holds the view of each row of the stretch.
fn shown_trays(
    transcript: &[Message],
    rows: Range<usize>,
    views: &[Rc<ToolView>],
    failures_only: bool,
) -> Vec<usize> {
    rows.clone()
        .filter(|&row| {
            let message = &transcript[row];
            if is_thought(message) {
                return !failures_only;
            }
            if row > rows.start && is_call(&transcript[row - 1]) {
                return false;
            }
            !failures_only
                || transcript[row..rows.end]
                    .iter()
                    .zip(&views[row - rows.start..])
                    .take_while(|(message, _)| is_call(message))
                    .any(|(_, view)| view.status == ToolStatus::Failed)
        })
        .collect()
}

/// What every row of one stretch of tool work reads. A frame builds it once for the stretch,
/// so each row in view costs its own tray instead of a pass over the whole stretch.
pub(super) struct Stretch {
    rows: Range<usize>,
    /// The view of each row of the stretch, in order.
    views: Vec<Rc<ToolView>>,
    trays: usize,
    /// A call of the stretch runs in its thread now.
    running: bool,
    /// The rows where a shown tray starts under an open parent, then under a folded one.
    shown: [Vec<usize>; 2],
}

// Counts the trays of a stretch of tool rows. Each thought is one tray, and so is each run of
// calls.
fn tray_count(messages: &[Message]) -> usize {
    let mut count = 0;
    let mut in_run = false;
    for message in messages {
        let call = is_call(message);
        if !call || !in_run {
            count += 1;
        }
        in_run = call;
    }
    count
}

// Names what a run or a stretch did in one line: its calls by kind, its thoughts, its edits.
fn work_label(calls: &[Rc<ToolView>], thoughts: usize, edits: &EditSummary) -> String {
    let mut parts = Vec::new();
    let summary = tool_output::summary(calls);
    if !summary.is_empty() {
        parts.push(summary);
    }
    if thoughts > 0 {
        parts.push(format!(
            "{thoughts} {}",
            if thoughts == 1 { "thought" } else { "thoughts" },
        ));
    }
    if edits.files > 0 {
        parts.push(format!(
            "edited {} {}",
            edits.files,
            if edits.files == 1 { "file" } else { "files" },
        ));
    }
    let mut label = parts.join(" · ");
    if let Some(first) = label.get(..1) {
        label = first.to_uppercase() + &label[1..];
    }
    if label.is_empty() {
        label = "Worked".into();
    }
    label
}

// Raises the summary line of a stretch of work into a plate. While a well hangs under it, the
// plate squares its bottom corners onto the well. GPUI takes one hover style per element, so
// this sets the line's only one.
fn plate(line: Button, t: Theme, well: bool) -> Button {
    line.overflow_hidden()
        .rounded(px(crate::theme::RADIUS_SURFACE))
        .when(well, |plate| plate.rounded_b(px(0.)))
        .bg(t.raised)
        .border_color(t.border)
        .shadow(kit::plate_light(t, !well))
        .layer(kit::shade(t.dot.opacity(0.3), dither::Edge::Bottom))
        .layer(kit::glint(t.accent.opacity(0.45), dither::Edge::Bottom))
        .hover(move |s| s.bg(t.control).text_color(t.text))
}

// Lays the summary line of a tray flat inside a well. It sets the line's only hover style.
fn flat(line: Button, t: Theme) -> Button {
    line.bg(gpui::transparent_black())
        .border_color(gpui::transparent_black())
        .hover(move |s| s.bg(t.selected).text_color(t.text))
}

// The summary line of a tray or of a stretch: one line that opens or folds what hangs under it.
fn summary_line(line: Button) -> Button {
    line.w_full()
        .h_auto()
        .min_h(px(28.))
        .py(px(4.))
        .px(px(6.))
        .gap(px(8.))
        .justify_start()
}

// A chevron that turns from pointing right, folded, to pointing down, open, as `openness` goes
// from 0 to 1.
fn chevron(openness: f32, color: Hsla) -> Svg {
    svg()
        .path("chevron-right")
        .size(px(12.))
        .flex_shrink_0()
        .text_color(color)
        .with_transformation(Transformation::rotate(radians(
            std::f32::consts::FRAC_PI_2 * openness.clamp(0., 1.),
        )))
}

// The name of a tray in sentence case. It shrinks to an ellipsis before the line wraps.
fn tray_label(text: impl IntoElement, color: Hsla) -> Div {
    div()
        .flex_shrink(1.)
        .min_w_0()
        .text_ellipsis()
        .font_family(crate::theme::sans())
        .font_weight(FontWeight::MEDIUM)
        .text_size(text_px(12.5))
        .text_color(color)
        .child(text)
}

// A mono caps label that a highlight sweeps across while its work runs.
fn caps_gleam(text: &str, t: Theme, reduce_motion: bool) -> Div {
    div()
        .flex_shrink_0()
        .font_family(crate::theme::mono())
        .text_size(text_px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .letter_spacing(text_px(crate::theme::LABEL_TRACKING))
        .line_height(text_px(14.))
        .child(motion::gleam(text.to_uppercase(), t, reduce_motion))
}

// Sentence-case text that a highlight sweeps across while its work runs, such as the call that a
// run is making now.
fn live_text(text: impl Into<SharedString>, t: Theme, reduce_motion: bool) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .font_family(crate::theme::sans())
        .font_weight(FontWeight::NORMAL)
        .text_size(text_px(12.))
        .child(motion::gleam(text, t, reduce_motion))
}

// Hangs the calls or the text of a tray from a dotted rail under the tray's chevron. The rows
// on the rail start under the tray's glyph.
fn rail(content: Div, t: Theme) -> Div {
    content.relative().pl(px(20.)).pt(px(2.)).pb(px(4.)).child(
        div()
            .absolute()
            .left(px(11.))
            .top(px(2.))
            .bottom(px(6.))
            .w(px(1.))
            .child(dither::DottedRule::vertical(t.subtle.opacity(0.35))),
    )
}

// The tally of a stretch: the glyph of each kind of work that it did and how many times.
fn tally(views: &[Rc<ToolView>], t: Theme) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.))
        .children(TALLY.iter().filter_map(|&category| {
            let count = views
                .iter()
                .filter(|view| view.category == category)
                .count();
            (count > 0).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(icon(glyph(category)).size(px(12.)).text_color(t.subtle))
                    .child(figure(count.to_string(), t.muted))
            })
        }))
}

impl Shell {
    /// Renders the tray of one transcript row of tool work: the run of calls that starts at the
    /// row, or the thought of the row. The first row of a stretch of several trays also carries
    /// the parent tray of the stretch, and a folded parent hides the rows under it. A row of new
    /// work grows into place the first time it shows.
    pub(super) fn tool_calls(
        &mut self,
        conversation: &Conversation,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(stretch) = self.work_stretch(conversation, index) else {
            return div().into_any_element();
        };
        let rows = stretch.rows.clone();
        let Some((first_key, row_key, row_fresh)) = self
            .conversation_messages(conversation)
            .and_then(|transcript| {
                let first = transcript.get(rows.start)?;
                let row = transcript.get(index)?;
                Some((
                    format!("{}:{}", first.task_id, first.id),
                    format!("{}:{}", row.task_id, row.id),
                    fresh(row),
                ))
            })
        else {
            return div().into_any_element();
        };
        let row = if stretch.trays < 2 {
            self.work_tray(conversation, index, false, true, cx)
                .map(|tray| {
                    div()
                        .w_full()
                        .min_w_0()
                        .px_5()
                        .py(px(4.))
                        .child(tray)
                        .into_any_element()
                })
        } else {
            self.work_stretch_row(conversation, &stretch, index, &first_key, cx)
        };
        let Some(row) = row else {
            return div().into_any_element();
        };
        let enter = self.folds.slot(
            &format!("enter-row:{row_key}"),
            true,
            row_fresh,
            cx.reduce_motion(),
        );
        motion::fold(row, &enter, None)
    }

    // One row of a stretch of several trays: the parent's plate on the first row of the
    // stretch, and the slice of the well that holds the row's tray. Each row folds its own slice,
    // so the slices of a folding parent close together.
    fn work_stretch_row(
        &mut self,
        conversation: &Conversation,
        stretch: &Stretch,
        index: usize,
        first_key: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let rows = stretch.rows.clone();
        let key = format!("work:{first_key}");
        let open = self.tray_open(conversation, &key, rows.end, stretch.running);
        let fold = self
            .folds
            .slot(&format!("{key}@{index}"), open, false, cx.reduce_motion());
        // A folding parent keeps its trays until they have closed.
        let shown = fold.shown();
        let trays = &stretch.shown[usize::from(!shown)];
        let header = (index == rows.start)
            .then(|| self.work_header(conversation, stretch, &key, open, fold.openness, cx));
        let t = self.theme;
        // Each row draws its own slice of the well, so the trays of a stretch read as one
        // recessed list under their parent's plate.
        let (first_tray, last_tray) = (trays.first() == Some(&index), trays.last() == Some(&index));
        let tray = self
            .work_tray(conversation, index, !shown, false, cx)
            .map(|tray| {
                let slice = kit::well(t, first_tray, last_tray).child(tray);
                if shown {
                    motion::fold(slice, &fold, None)
                } else {
                    slice.into_any_element()
                }
            });
        if header.is_none() && tray.is_none() {
            return None;
        }
        // The stretch ends after its last tray, or after its plate when nothing is under it.
        let ends = last_tray || (header.is_some() && trays.is_empty());
        Some(
            div()
                .w_full()
                .min_w_0()
                .px_5()
                .when(header.is_some(), |row| row.pt(px(4.)))
                .when(ends, |row| row.pb(px(4.)))
                .children(header)
                .children(tray)
                .into_any_element(),
        )
    }

    // The parent tray of a stretch of work: a caps title, the tally of what its runs and
    // thoughts did, then a dotted rule to its time, on a raised plate. While trays hang below it
    // in a recessed well, the plate squares its bottom corners onto the well.
    // `openness` is how far the trays under the plate are open in this frame.
    fn work_header(
        &self,
        conversation: &Conversation,
        stretch: &Stretch,
        key: &str,
        open: bool,
        openness: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let rows = stretch.rows.clone();
        let running = stretch.running;
        let reduce = cx.reduce_motion();
        let messages = self
            .conversation_messages(conversation)
            .and_then(|transcript| transcript.get(rows.clone()))
            .unwrap_or_default();
        // The plate keeps its square corners on the well until the trays have folded.
        let well = !stretch.shown[usize::from(!open && openness <= 0.)].is_empty();
        let views = &stretch.views;
        let (thoughts, calls): (Vec<_>, Vec<_>) = views
            .iter()
            .cloned()
            .partition(|view| view.category == ToolCategory::Thinking);
        let edits = tool_output::edit_summary(messages);
        let label = work_label(&calls, thoughts.len(), &edits);
        let elapsed: u64 = views.iter().filter_map(|view| view.duration_ms).sum();
        let failures = calls
            .iter()
            .filter(|view| view.status == ToolStatus::Failed)
            .count();
        let toggle_conversation = conversation.clone();
        let toggle_key = key.to_owned();
        self.control(SharedString::from(key.to_owned()))
            .map(summary_line)
            .min_h(px(30.))
            .px(px(8.))
            .flex_wrap()
            .map(|line| plate(line, t, well))
            .text_color(t.muted)
            .accessibility_label(format!(
                "{} {label}",
                if open { "Collapse" } else { "Expand" },
            ))
            .child(chevron(openness, t.muted))
            .when(running, |button| {
                button.child(dither::pulse(t.accent, reduce))
            })
            .child(if running {
                caps_gleam("Working", t, reduce)
            } else {
                caps("Worked", t).text_color(t.text)
            })
            .child(tally(views, t))
            .when_some(edits.lines.filter(|_| edits.files > 0), |button, lines| {
                button.child(kit::diffstat(lines.added, lines.removed, t))
            })
            .when(failures > 0, |button| {
                button.child(kit::chip(format!("{failures} FAILED"), t.removed))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(8.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.3))),
            )
            .when(!running && elapsed >= 1000, |button| {
                button.child(figure(tool_output::duration(elapsed), t.subtle))
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                s.set_tray(&toggle_conversation, &toggle_key, !open, rows.clone(), cx)
            }))
            .into_any_element()
    }

    // Returns the tray that starts at a row: a thought, or the run of calls whose first call
    // is the row. With `failures_only`, the parent of the row is folded, and only the failed
    // calls of a run remain. A `plated` tray has no parent, so its line is the plate.
    fn work_tray(
        &mut self,
        conversation: &Conversation,
        index: usize,
        failures_only: bool,
        plated: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let transcript = self.conversation_messages(conversation)?;
        let message = transcript.get(index)?;
        if is_thought(message) {
            if failures_only {
                return None;
            }
            let message = message.clone();
            return Some(self.thought_tray(conversation, &message, index, plated, cx));
        }
        if index > 0 && is_call(&transcript[index - 1]) {
            return None;
        }
        self.tool_run(conversation, index, failures_only, plated, cx)
    }

    // One run of calls: its summary line and its individually expandable calls on a rail.
    fn tool_run(
        &mut self,
        conversation: &Conversation,
        index: usize,
        failures_only: bool,
        plated: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let stretch = self.work_stretch(conversation, index)?;
        let t = self.theme;
        let reduce = cx.reduce_motion();
        let (length, group_key, first_id, running) = {
            let transcript = self.conversation_messages(conversation)?;
            let length = transcript[index..]
                .iter()
                .take_while(|message| is_call(message))
                .count();
            let messages = &transcript[index..index + length];
            let first = messages.first()?;
            let offset = index.checked_sub(stretch.rows.start)?;
            let views = stretch.views.get(offset..offset + length)?;
            let running = messages.iter().zip(views).rposition(|(message, view)| {
                view.status == ToolStatus::Running
                    && self.conversation_message_active(conversation, message)
            });
            (
                length,
                format!("tool-group:{}:{}", first.task_id, first.id),
                first.id.clone(),
                running,
            )
        };
        let offset = index - stretch.rows.start;
        let views = &stretch.views[offset..offset + length];
        // The run opens only itself: its calls stay one line each until they are clicked.
        let expanded = !failures_only
            && self.tray_open(conversation, &group_key, index + length, running.is_some());
        let fold = (!failures_only).then(|| self.folds.slot(&group_key, expanded, false, reduce));
        // A folding run keeps its calls until they have closed.
        let unfolded = fold.as_ref().is_some_and(motion::Fold::shown);
        let transcript = self.conversation_messages(conversation)?;
        let messages = &transcript[index..index + length];
        let edits = tool_output::edit_summary(messages);
        // A folded run still shows what went wrong. Only the calls on screen are copied out of
        // the transcript, which is borrowed until here.
        let shown: Vec<(Message, Rc<ToolView>)> = messages
            .iter()
            .zip(views)
            .filter(|(_, view)| unfolded || view.status == ToolStatus::Failed)
            .map(|(message, view)| (message.clone(), view.clone()))
            .collect();
        let any_shown = !shown.is_empty();
        if failures_only && !any_shown {
            return None;
        }
        let label = work_label(views, 0, &edits);
        let elapsed: u64 = views.iter().filter_map(|view| view.duration_ms).sum();
        let failures = views
            .iter()
            .filter(|view| view.status == ToolStatus::Failed)
            .count();
        // A run of one kind of call takes that kind's glyph.
        let run_glyph = match views.first() {
            Some(first) if views.iter().all(|view| view.category == first.category) => {
                glyph(first.category)
            }
            _ => "list-ordered",
        };
        let accessible_label = format!("{} {label}", if expanded { "Collapse" } else { "Expand" },);
        let group_conversation = conversation.clone();
        let toggle_key = group_key.clone();
        let summary = self
            .control(SharedString::from(group_key.clone()))
            .map(summary_line)
            .map(|line| {
                if plated {
                    plate(line, t, any_shown)
                } else {
                    flat(line, t)
                }
            })
            .text_color(t.muted)
            .accessibility_label(accessible_label)
            .child(chevron(
                fold.as_ref().map_or(0., |fold| fold.openness),
                if plated { t.muted } else { t.subtle },
            ))
            .child(if running.is_some() {
                dither::pulse(t.accent, reduce)
            } else {
                icon(run_glyph)
                    .size(px(13.))
                    .text_color(if failures > 0 { t.removed } else { t.subtle })
                    .into_any_element()
            })
            .child(tray_label(label, if plated { t.text } else { t.muted }))
            .when_some(edits.lines.filter(|_| edits.files > 0), |button, lines| {
                button.child(kit::diffstat(lines.added, lines.removed, t))
            })
            .when(failures > 0, |button| {
                button.child(kit::chip(format!("{failures} FAILED"), t.removed))
            })
            // A running run names the call that it makes now where its time will be.
            .child(match running {
                Some(position) => {
                    let view = &views[position];
                    let title = view.title.lines().next().unwrap_or_default();
                    live_text(
                        format!("{} {title}", verb(view.category))
                            .trim_end()
                            .to_owned(),
                        t,
                        reduce,
                    )
                }
                None => div().flex_1().min_w(px(8.)),
            })
            .when(running.is_none() && elapsed >= 1000, |button| {
                button.child(figure(tool_output::duration(elapsed), t.subtle))
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                s.set_tray(
                    &group_conversation,
                    &toggle_key,
                    !expanded,
                    index..index + 1,
                    cx,
                )
            }));
        // Under a folded parent the run has no line of its own, and the rail of the parent
        // already leads its failed calls.
        let mut group = div()
            .w_full()
            .min_w_0()
            .when(!failures_only, |group| group.child(summary));
        let mut calls = div().flex().flex_col().gap(px(1.));
        if !failures_only {
            calls = rail(calls, t);
        }
        for (message, view) in &shown {
            let call = self.tool_call(conversation, message, view, index, cx);
            // A call that joins the run grows into place. The run's first call comes with the
            // run's own row.
            calls = calls.child(if message.id == first_id {
                call
            } else {
                let enter = self.folds.slot(
                    &format!("enter-call:{}:{}", message.task_id, message.id),
                    true,
                    fresh(message),
                    reduce,
                );
                motion::fold(call, &enter, Some(t.inset))
            });
        }
        if any_shown {
            let body = if plated {
                kit::well(t, true, true).child(calls).into_any_element()
            } else {
                calls.into_any_element()
            };
            group = group.child(match &fold {
                Some(fold) if unfolded => motion::fold(body, fold, (!plated).then_some(t.inset)),
                _ => body,
            });
        }
        Some(group.into_any_element())
    }

    // Returns the stretch of tool work around row `index`. The rows of a stretch in view read
    // it on every frame, so a frame builds it once and keeps it for each of its rows.
    fn work_stretch(&mut self, conversation: &Conversation, index: usize) -> Option<Rc<Stretch>> {
        if let Some(stretch) = self.work_stretches.get(&(conversation.clone(), index)) {
            return Some(stretch.clone());
        }
        let transcript = self.conversation_messages(conversation)?;
        let rows = work_rows(transcript, index);
        let messages = transcript.get(rows.clone())?;
        let views: Vec<Rc<ToolView>> = messages.iter().map(tool_output::view).collect();
        let running = messages.iter().zip(&views).any(|(message, view)| {
            view.status == ToolStatus::Running
                && self.conversation_message_active(conversation, message)
        });
        let stretch = Rc::new(Stretch {
            trays: tray_count(messages),
            running,
            shown: [false, true]
                .map(|failures_only| shown_trays(transcript, rows.clone(), &views, failures_only)),
            rows: rows.clone(),
            views,
        });
        for row in rows {
            self.work_stretches
                .insert((conversation.clone(), row), stretch.clone());
        }
        Some(stretch)
    }

    // Reports whether a tray is open. A tray follows its work unless the user set it by hand: it
    // is open while a call in it runs, and while it ends the transcript of a working
    // conversation, where the turn can still add to it. `end` is the row after the tray.
    fn tray_open(&self, conversation: &Conversation, key: &str, end: usize, running: bool) -> bool {
        self.tray_choices.get(key).copied().unwrap_or_else(|| {
            running
                || (self.conversation_working(conversation)
                    && self
                        .conversation_messages(conversation)
                        .is_some_and(|messages| messages.len() == end))
        })
    }

    // Opens or folds a tray by hand. The choice holds after the tray's work ends. `rows` are
    // the transcript rows whose height the tray decides.
    fn set_tray(
        &mut self,
        conversation: &Conversation,
        key: &str,
        open: bool,
        rows: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        self.tray_choices.insert(key.to_owned(), open);
        self.remeasure_messages(conversation, rows.clone());
        self.remeasure_after_fold(conversation, rows, cx);
        cx.notify();
    }

    // Measures `rows` again once a fold that starts now has ended. The list keeps the height
    // that it last measured for a row out of view, which may be one from the middle of the fold.
    fn remeasure_after_fold(
        &self,
        conversation: &Conversation,
        rows: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        if cx.reduce_motion() {
            return;
        }
        let conversation = conversation.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(motion::FOLD).await;
            this.update(cx, |s, cx| {
                s.remeasure_messages(&conversation, rows);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // One thought: its first sentence while folded, its text on a rail while open.
    fn thought_tray(
        &mut self,
        conversation: &Conversation,
        message: &Message,
        index: usize,
        plated: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let reduce = cx.reduce_motion();
        let view = tool_output::view(message);
        let message_key = format!("{}:{}", message.task_id, message.id);
        let key = format!("thought:{message_key}");
        let active = view.status == ToolStatus::Running
            && self.conversation_message_active(conversation, message);
        let expanded = self.tray_open(conversation, &key, index + 1, active);
        let fold = self.folds.slot(&key, expanded, false, reduce);
        let open_text = fold.shown() && !view.output.is_empty();
        let title: String = view
            .title
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        let label_color = if plated { t.text } else { t.muted };
        let tray_conversation = conversation.clone();
        let toggle_key = key.clone();
        let header = self
            .control(SharedString::from(key))
            .map(summary_line)
            .map(|line| {
                if plated {
                    plate(line, t, open_text)
                } else {
                    flat(line, t)
                }
            })
            .text_color(t.muted)
            .accessibility_label(format!(
                "{} thought: {title}",
                if expanded { "Collapse" } else { "Expand" },
            ))
            .child(chevron(
                fold.openness,
                if plated { t.muted } else { t.subtle },
            ))
            .child(if active {
                dither::pulse(t.accent, reduce)
            } else {
                icon(glyph(ToolCategory::Thinking))
                    .size(px(13.))
                    .text_color(t.subtle)
                    .into_any_element()
            })
            .child(
                if active {
                    tray_label(motion::gleam("Thinking", t, reduce), label_color)
                } else {
                    tray_label("Thought", label_color)
                }
                .flex_shrink_0(),
            )
            // The open tray shows the whole thought, so its header does not repeat the start.
            .when(!expanded, |header| {
                header.child(
                    div()
                        .flex_shrink(1.)
                        .min_w_0()
                        .text_ellipsis()
                        .font_family(crate::theme::sans())
                        .font_weight(FontWeight::NORMAL)
                        .text_size(text_px(12.))
                        .text_color(t.subtle)
                        .italic()
                        .child(title),
                )
            })
            .child(div().flex_1().min_w(px(8.)))
            .when_some(view.duration_ms.filter(|ms| *ms >= 1000), |header, ms| {
                header.child(figure(tool_output::duration(ms), t.subtle))
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                s.set_tray(
                    &tray_conversation,
                    &toggle_key,
                    !expanded,
                    index..index + 1,
                    cx,
                )
            }));
        let mut tray = div().w_full().min_w_0().child(header);
        if open_text {
            let text = rail(div(), t).child(
                div()
                    .px(px(6.))
                    .pb(px(2.))
                    .text_size(text_px(12.))
                    .text_color(t.muted)
                    .child(self.message_blocks(
                        &format!("tool-thought:{message_key}"),
                        &crate::markdown::blocks(&view.output),
                        self.message_arriving(conversation, message),
                        cx,
                    )),
            );
            let body = if plated {
                kit::well(t, true, true).child(text).into_any_element()
            } else {
                text.into_any_element()
            };
            tray = tray.child(motion::fold(body, &fold, (!plated).then_some(t.inset)));
        }
        tray.into_any_element()
    }

    fn toggle_tool_detail(
        &mut self,
        conversation: &Conversation,
        key: &str,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if !self.folded.remove(key) {
            self.folded.insert(key.to_owned());
        }
        self.remeasure_messages(conversation, index..index + 1);
        self.remeasure_after_fold(conversation, index..index + 1, cx);
        cx.notify();
    }

    fn tool_call(
        &mut self,
        conversation: &Conversation,
        message: &Message,
        view: &ToolView,
        group_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let reduce = cx.reduce_motion();
        let message_key = format!("{}:{}", message.task_id, message.id);
        let key = format!("tool-detail:{message_key}");
        let expanded = self.folded.contains(&key);
        let fold = self.folds.slot(&key, expanded, false, reduce);
        let active = view.status == ToolStatus::Running
            && self.conversation_message_active(conversation, message);
        // A running call shows its pulse and its moving title, so only an ended call has a chip.
        let (status, color) = match view.status {
            ToolStatus::Failed => (
                view.exit
                    .map_or_else(|| "FAILED".to_owned(), |code| format!("EXIT {code}")),
                t.removed,
            ),
            ToolStatus::Declined => ("DECLINED".to_owned(), t.warning),
            ToolStatus::Running if !active => ("STOPPED".to_owned(), t.subtle),
            ToolStatus::Running | ToolStatus::Completed => (String::new(), t.muted),
        };
        let edit_lines = message
            .file_edits
            .iter()
            .filter_map(|edit| edit.lines)
            .reduce(|mut total, lines| {
                total.added += lines.added;
                total.removed += lines.removed;
                total
            });
        let literal = matches!(
            view.category,
            ToolCategory::Command | ToolCategory::Read | ToolCategory::Edit | ToolCategory::Search
        );
        let title: String = view
            .title
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        let detail_conversation = conversation.clone();
        let toggle_key = key.clone();
        let row = self
            .control(SharedString::from(key.clone()))
            .w_full()
            .h(px(26.))
            .px(px(6.))
            .gap(px(8.))
            .justify_start()
            .font_family(crate::theme::sans())
            .font_weight(FontWeight::NORMAL)
            .text_size(text_px(12.))
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .text_color(t.muted)
            .when(fold.shown(), |row| row.bg(t.selected.opacity(0.5)))
            .hover(move |s| s.bg(t.selected).text_color(t.text))
            // The title is cut to the row's width; the pointer shows its whole first line.
            .when(!active && !title.is_empty(), |row| {
                let hint = title.clone();
                row.tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: hint.clone(),
                        theme: t,
                    })
                    .into()
                })
            })
            .accessibility_label(format!(
                "{} {}: {title}",
                if expanded { "Collapse" } else { "Expand" },
                verb(view.category)
            ))
            .child(if active {
                dither::pulse(t.accent, reduce)
            } else {
                icon(glyph(view.category))
                    .size(px(13.))
                    .text_color(if view.status == ToolStatus::Failed {
                        t.removed
                    } else {
                        t.subtle
                    })
                    .into_any_element()
            })
            // The verbs share one width, so the titles of a run start in one column.
            .child(caps(verb(view.category), t).min_w(px(48.)))
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(relative(0.68))
                    .min_w_0()
                    .text_ellipsis()
                    .text_color(t.text.opacity(0.9))
                    .when(literal, |d| {
                        d.font_family(crate::theme::mono()).text_size(text_px(11.5))
                    })
                    .map(|d| {
                        if !active {
                            d.child(title.clone())
                        } else if title.is_empty() {
                            d.child(motion::gleam("Starting…", t, reduce))
                        } else {
                            d.child(motion::gleam(title.clone(), t, reduce))
                        }
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .text_size(text_px(11.))
                    .text_color(t.subtle)
                    .child(view.detail.lines().next().unwrap_or_default().to_owned()),
            )
            .when_some(edit_lines, |row, lines| {
                row.child(kit::diffstat(lines.added, lines.removed, t))
            })
            .when_some(view.duration_ms.filter(|ms| *ms >= 1000), |row, ms| {
                row.child(figure(tool_output::duration(ms), t.subtle))
            })
            .when(!status.is_empty(), |row| {
                row.child(kit::chip(status, color))
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                s.toggle_tool_detail(&detail_conversation, &toggle_key, group_index, cx)
            }));
        let card = div().w_full().min_w_0().flex().flex_col().child(row);
        if !fold.shown() {
            return card.into_any_element();
        }
        let mut details = div()
            .ml(px(6.))
            .mt(px(2.))
            .mb(px(6.))
            .flex()
            .flex_col()
            .gap_2();
        if view.category == ToolCategory::Command {
            details = details.child(self.message_code(
                &format!("tool-command:{message_key}"),
                "sh",
                &view.title,
                Some(140.),
                false,
                cx,
            ));
        } else if view.title.lines().count() > 1 || view.title.chars().count() > 200 {
            details = details.child(self.message_code(
                &format!("tool-subject:{message_key}"),
                "",
                &view.title,
                Some(140.),
                false,
                cx,
            ));
        }
        if !view.name.is_empty() && view.name != verb(view.category) {
            details = details.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(caps("Tool", t))
                    .child(figure(view.name.clone(), t.muted)),
            );
        }
        if !view.detail.is_empty() && view.detail.lines().count() > 1 {
            details = details.child(
                div()
                    .text_size(text_px(12.))
                    .text_color(t.muted)
                    .child(view.detail.clone()),
            );
        }
        if !view.diff.is_empty() {
            details = details.child(self.inline_diff(conversation, message, view, group_index, cx));
        }
        if !view.output.is_empty() {
            let (body, language) = (&view.output, view.language);
            let (excerpt, truncated) = tool_output::preview(body);
            let full_key = format!("tool-full:{message_key}");
            let full = self.folded.contains(&full_key);
            details = details.child(self.message_code(
                &format!("tool-output:{message_key}"),
                language,
                if full { body } else { &excerpt },
                Some(280.),
                self.message_arriving(conversation, message),
                cx,
            ));
            if truncated {
                let full_conversation = conversation.clone();
                details = details.child(
                    div().flex().child(
                        self.ghost(
                            SharedString::from(full_key.clone()),
                            if full { "minus" } else { "plus" },
                            if full {
                                "Show preview".to_owned()
                            } else {
                                format!("Show all {} lines", body.lines().count())
                            },
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.toggle_tool_detail(&full_conversation, &full_key, group_index, cx)
                        })),
                    ),
                );
            }
        } else if view.diff.is_empty() {
            details = details.child(div().text_size(text_px(12.)).text_color(t.subtle).child(
                if active {
                    "Waiting for output…"
                } else {
                    "No output recorded."
                },
            ));
        }
        let copied = tool_output::record(message);
        details = details.child(
            div().flex().child(
                self.ghost(
                    SharedString::from(format!("copy-tool:{message_key}")),
                    "copy",
                    "Copy call and output",
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                    let tone = s.theme.accent;
                    s.notify_user("Copied", tone, cx);
                })),
            ),
        );
        card.child(motion::fold(details, &fold, Some(t.inset)))
            .into_any_element()
    }

    // The changed lines of one edit, colored by side, with the rest folded after a preview.
    fn inline_diff(
        &mut self,
        conversation: &Conversation,
        message: &Message,
        view: &ToolView,
        group_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let full_key = format!("tool-diff:{}:{}", message.task_id, message.id);
        let full = self.folded.contains(&full_key);
        let limit = if full {
            view.diff.len()
        } else {
            DIFF_PREVIEW_LINES
        };
        let mut rows = div()
            .w_full()
            .font_family(crate::theme::mono())
            .text_size(code_px(11.5))
            .line_height(relative(1.5));
        for line in view.diff.iter().take(limit) {
            let (sign, color, background) = match line.kind {
                LineKind::Added => ("+", t.added, t.added_bg),
                LineKind::Removed => ("−", t.removed, t.removed_bg),
                LineKind::Header => ("", t.info, t.header),
                LineKind::Context => ("", t.muted, gpui::transparent_black()),
            };
            let number = match line.kind {
                LineKind::Removed => line.old,
                _ => line.new.or(line.old),
            };
            rows = rows.child(
                div()
                    .flex()
                    .bg(background)
                    // A solid edge in the line's color marks changed lines down the gutter.
                    .child(div().w(px(2.)).flex_shrink_0().bg(match line.kind {
                        LineKind::Added | LineKind::Removed => color,
                        _ => gpui::transparent_black(),
                    }))
                    .child(
                        div()
                            .w(px(38.))
                            .flex_shrink_0()
                            .pr(px(6.))
                            .text_right()
                            .text_color(t.subtle)
                            .child(number.map_or(String::new(), |number| number.to_string())),
                    )
                    .child(
                        div()
                            .w(px(12.))
                            .flex_shrink_0()
                            .text_color(color)
                            .child(sign),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_nowrap()
                            .text_color(if line.kind == LineKind::Context {
                                t.muted
                            } else {
                                t.text
                            })
                            .child(line.text.replace('\t', "    ")),
                    ),
            );
        }
        let hidden = view.diff.len().saturating_sub(limit);
        let path = message
            .file_edits
            .first()
            .map(|edit| edit.path.clone())
            .unwrap_or_else(|| view.title.clone());
        let diff_conversation = conversation.clone();
        let (added, removed) =
            view.diff
                .iter()
                .fold((0, 0), |(added, removed), line| match line.kind {
                    LineKind::Added => (added + 1, removed),
                    LineKind::Removed => (added, removed + 1),
                    _ => (added, removed),
                });
        kit::block(t)
            .child(
                kit::block_title(
                    div()
                        .id("diff-path")
                        .flex_shrink(1.)
                        .min_w_0()
                        .text_ellipsis()
                        .font_family(crate::theme::mono())
                        .text_size(text_px(11.))
                        .text_color(t.text)
                        .tooltip({
                            let hint = path.clone();
                            move |_, cx| {
                                cx.new(|_| Hint {
                                    text: hint.clone(),
                                    theme: t,
                                })
                                .into()
                            }
                        })
                        .child(path),
                    t.info,
                    t,
                )
                .pr(px(10.))
                .child(kit::diffstat(added, removed, t)),
            )
            .child(
                div()
                    .id(SharedString::from(format!("diff-scroll:{full_key}")))
                    .w_full()
                    .max_h(px(360.))
                    .overflow_scroll()
                    .py(px(4.))
                    .child(rows),
            )
            .when(hidden > 0 || full, |card| {
                card.child(dither::DottedRule::horizontal(t.subtle.opacity(0.4)))
                    .child(
                        div().px(px(6.)).py(px(3.)).flex().child(
                            self.ghost(
                                SharedString::from(full_key.clone()),
                                if full { "minus" } else { "plus" },
                                if full {
                                    "Show less".to_owned()
                                } else {
                                    format!("Show {hidden} more lines")
                                },
                            )
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    s.toggle_tool_detail(
                                        &diff_conversation,
                                        &full_key,
                                        group_index,
                                        cx,
                                    )
                                },
                            )),
                        ),
                    )
            })
            .into_any_element()
    }
}
