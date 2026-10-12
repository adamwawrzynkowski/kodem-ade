//! The work trail: a run of tool calls under one summary line, with each call, its output and
//! its diff one click away, and each thought in a tray of its own between the runs.
//!
//! A tray is open while its turn can still add to it and folds when the work moves on. Failed
//! calls stay visible while their run is folded.
use super::*;
use crate::tool_output::{self, ToolView};
use std::rc::Rc;

/// Lines of an edit shown before the rest is folded behind a control.
const DIFF_PREVIEW_LINES: usize = 40;

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

impl Shell {
    /// Renders one tool run's summary and individually expandable calls at its first transcript
    /// row, or the tray of one thought at the row of that thought.
    pub(super) fn tool_calls(
        &mut self,
        conversation: &Conversation,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(transcript) = self.conversation_messages(conversation) else {
            return div().into_any_element();
        };
        if transcript.get(index).is_some_and(is_thought) {
            let message = transcript[index].clone();
            return self.thought_tray(conversation, &message, index, cx);
        }
        if index > 0 && is_call(&transcript[index - 1]) {
            return div().into_any_element();
        }
        let length = transcript[index..]
            .iter()
            .take_while(|message| is_call(message))
            .count();
        let messages = &transcript[index..index + length];
        let Some(first) = messages.first() else {
            return div().into_any_element();
        };
        let t = self.theme;
        let views: Vec<Rc<ToolView>> = messages.iter().map(tool_output::view).collect();
        let group_key = format!("tool-group:{}:{}", first.task_id, first.id);
        let running = messages.iter().zip(&views).rposition(|(message, view)| {
            view.status == ToolStatus::Running
                && self.conversation_message_active(conversation, message)
        });
        // The run opens only itself: its calls stay one line each until they are clicked.
        let expanded = self.tray_open(conversation, &group_key, index + length, running.is_some());
        let edits = tool_output::edit_summary(messages);
        // A folded run still shows what went wrong. Only the calls on screen are copied out of
        // the transcript, which is borrowed until here.
        let shown: Vec<(Message, Rc<ToolView>)> = messages
            .iter()
            .zip(&views)
            .filter(|(_, view)| expanded || view.status == ToolStatus::Failed)
            .map(|(message, view)| (message.clone(), view.clone()))
            .collect();
        let mut label = tool_output::summary(&views);
        if edits.files > 0 {
            let edited = format!(
                "edited {} {}",
                edits.files,
                if edits.files == 1 { "file" } else { "files" },
            );
            label = if label.is_empty() {
                format!("E{}", &edited[1..])
            } else {
                format!("{label} · {edited}")
            };
        }
        if label.is_empty() {
            label = "Worked".into();
        }
        let elapsed: u64 = views.iter().filter_map(|view| view.duration_ms).sum();
        let failures = views
            .iter()
            .filter(|view| view.status == ToolStatus::Failed)
            .count();
        let accessible_label = format!("{} {label}", if expanded { "Collapse" } else { "Expand" },);
        let group_conversation = conversation.clone();
        let toggle_key = group_key.clone();
        let mut summary = self
            .control(SharedString::from(group_key.clone()))
            .w_full()
            .h_auto()
            .min_h(px(28.))
            .py(px(4.))
            .px(px(6.))
            .gap(px(8.))
            .justify_start()
            .flex_wrap()
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .text_color(t.muted)
            .hover(move |s| s.bg(t.selected).text_color(t.text))
            .accessibility_label(accessible_label)
            .child(
                icon(if expanded {
                    "chevron-down"
                } else {
                    "chevron-right"
                })
                .size(px(12.))
                .text_color(t.subtle),
            )
            // The run reads as a section header: a mono label, its totals, then a dotted rule.
            .child(caps(&label, t).text_color(t.muted))
            .when_some(edits.lines.filter(|_| edits.files > 0), |button, lines| {
                button.child(kit::diffstat(lines.added, lines.removed, t))
            })
            .when(failures > 0, |button| {
                button.child(kit::chip(format!("{failures} FAILED"), t.removed))
            })
            .when(running.is_none(), |button| {
                button
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(8.))
                            .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
                    )
                    .when(elapsed >= 1000, |button| {
                        button.child(figure(tool_output::duration(elapsed), t.subtle))
                    })
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                s.set_tray(&group_conversation, &toggle_key, !expanded, index, cx)
            }));
        if let Some(position) = running {
            let view = &views[position];
            summary = summary
                .child(dither::pulse(t.accent, cx.reduce_motion()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(t.subtle)
                        .child(format!("{} {}", verb(view.category), view.title)),
                );
        }
        let mut group = div().w_full().min_w_0().px_5().py(px(2.)).child(summary);
        let mut calls = div()
            .ml(px(11.))
            .pl(px(11.))
            .pt(px(2.))
            .pb(px(4.))
            .border_l_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .gap(px(2.));
        let any_shown = !shown.is_empty();
        for (message, view) in &shown {
            calls = calls.child(self.tool_call(conversation, message, view, index, cx));
        }
        if any_shown {
            group = group.child(calls);
        }
        group.into_any_element()
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

    // Opens or folds a tray by hand. The choice holds after the tray's work ends.
    fn set_tray(
        &mut self,
        conversation: &Conversation,
        key: &str,
        open: bool,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.tray_choices.insert(key.to_owned(), open);
        if let Some(scroll) = self.conversation_scroll(conversation) {
            scroll.remeasure_items(index..index + 1);
        }
        cx.notify();
    }

    // One thought: its first sentence while folded, its text while open.
    fn thought_tray(
        &mut self,
        conversation: &Conversation,
        message: &Message,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let view = tool_output::view(message);
        let message_key = format!("{}:{}", message.task_id, message.id);
        let key = format!("thought:{message_key}");
        let active = view.status == ToolStatus::Running
            && self.conversation_message_active(conversation, message);
        let expanded = self.tray_open(conversation, &key, index + 1, active);
        let title: String = view
            .title
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        let label = if active { "Thinking" } else { "Thought" };
        let tray_conversation = conversation.clone();
        let toggle_key = key.clone();
        let header = self
            .control(SharedString::from(key))
            .w_full()
            .h_auto()
            .min_h(px(28.))
            .py(px(4.))
            .px(px(6.))
            .gap(px(8.))
            .justify_start()
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .text_color(t.muted)
            .hover(move |s| s.bg(t.selected).text_color(t.text))
            .accessibility_label(format!(
                "{} thought: {title}",
                if expanded { "Collapse" } else { "Expand" },
            ))
            .child(
                icon(if expanded {
                    "chevron-down"
                } else {
                    "chevron-right"
                })
                .size(px(12.))
                .text_color(t.subtle),
            )
            .child(if active {
                dither::pulse(t.accent, cx.reduce_motion())
            } else {
                icon(glyph(ToolCategory::Thinking))
                    .size(px(13.))
                    .text_color(t.subtle)
                    .into_any_element()
            })
            .child(caps(label, t).text_color(t.muted))
            // The open tray shows the whole thought, so its header does not repeat the start.
            .when(!expanded, |header| {
                header.child(
                    div()
                        .flex_shrink(1.)
                        .min_w_0()
                        .text_ellipsis()
                        .font_family(crate::theme::SANS)
                        .font_weight(FontWeight::NORMAL)
                        .text_size(px(12.))
                        .italic()
                        .child(title),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(8.))
                    .child(dither::DottedRule::horizontal(t.subtle.opacity(0.4))),
            )
            .when_some(view.duration_ms.filter(|ms| *ms >= 1000), |header, ms| {
                header.child(figure(tool_output::duration(ms), t.subtle))
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                s.set_tray(&tray_conversation, &toggle_key, !expanded, index, cx)
            }));
        let mut tray = div().w_full().min_w_0().px_5().py(px(2.)).child(header);
        if expanded && !view.output.is_empty() {
            tray = tray.child(
                div()
                    .ml(px(11.))
                    .pl(px(17.))
                    .pt(px(2.))
                    .pb(px(6.))
                    .border_l_1()
                    .border_color(t.border)
                    .text_size(px(self.settings.font_size - 2.))
                    .text_color(t.muted)
                    .child(self.message_blocks(
                        &format!("tool-thought:{message_key}"),
                        crate::markdown::blocks(&view.output),
                        false,
                        cx,
                    )),
            );
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
        if let Some(scroll) = self.conversation_scroll(conversation) {
            scroll.remeasure_items(index..index + 1);
        }
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
        let message_key = format!("{}:{}", message.task_id, message.id);
        let key = format!("tool-detail:{message_key}");
        let expanded = self.folded.contains(&key);
        let active = view.status == ToolStatus::Running
            && self.conversation_message_active(conversation, message);
        let (status, color) = match view.status {
            ToolStatus::Failed => (
                view.exit
                    .map_or_else(|| "FAILED".to_owned(), |code| format!("EXIT {code}")),
                t.removed,
            ),
            ToolStatus::Declined => ("DECLINED".to_owned(), t.warning),
            ToolStatus::Running if active => ("RUNNING".to_owned(), t.accent),
            ToolStatus::Running => ("STOPPED".to_owned(), t.subtle),
            ToolStatus::Completed => (String::new(), t.muted),
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
            .font_family(crate::theme::SANS)
            .font_weight(FontWeight::NORMAL)
            .text_size(px(12.))
            .bg(gpui::transparent_black())
            .border_color(gpui::transparent_black())
            .text_color(t.muted)
            .hover(move |s| s.bg(t.selected).text_color(t.text))
            .accessibility_label(format!(
                "{} {}: {title}",
                if expanded { "Collapse" } else { "Expand" },
                verb(view.category)
            ))
            .child(if active {
                dither::pulse(t.accent, cx.reduce_motion())
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
            .child(caps(verb(view.category), t))
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(relative(0.68))
                    .min_w_0()
                    .text_ellipsis()
                    .text_color(t.text.opacity(0.9))
                    .when(literal, |d| {
                        d.font_family(crate::theme::MONO).text_size(px(11.5))
                    })
                    .child(title.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .text_size(px(11.))
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
        let mut card = div().w_full().min_w_0().flex().flex_col().child(row);
        if !expanded {
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
                    .text_size(px(12.))
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
                false,
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
            details = details.child(div().text_size(px(12.)).text_color(t.subtle).child(
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
        card = card.child(details);
        card.into_any_element()
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
            .font_family(crate::theme::MONO)
            .text_size(px(self.settings.font_size - 2.5))
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
                        .flex_shrink(1.)
                        .min_w_0()
                        .text_ellipsis()
                        .font_family(crate::theme::MONO)
                        .text_size(px(11.))
                        .text_color(t.text)
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
                card.child(
                    div()
                        .px(px(6.))
                        .py(px(3.))
                        .border_t_1()
                        .border_color(t.border)
                        .flex()
                        .child(
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
