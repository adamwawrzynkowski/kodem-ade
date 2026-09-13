//! Progressive disclosure for tool groups, individual calls and long captured output.
use super::*;
use crate::tool_output::{self, ToolKind};

impl Shell {
    /// Renders one tool group's summary and individually expandable calls at its first transcript row.
    pub(super) fn tool_calls(
        &mut self,
        conversation: &Conversation,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(transcript) = self.conversation_messages(conversation) else {
            return div().into_any_element();
        };
        if index > 0 && transcript[index - 1].kind == MessageKind::Tool {
            return div().into_any_element();
        }
        let messages: Vec<_> = transcript
            .iter()
            .skip(index)
            .take_while(|m| m.kind == MessageKind::Tool)
            .cloned()
            .collect();
        let Some(first) = messages.first() else {
            return div().into_any_element();
        };
        let t = self.theme;
        let group_key = format!("tool-group:{}:{}", first.task_id, first.id);
        let expanded = self.folded.contains(&group_key);
        let active = messages
            .iter()
            .any(|message| self.conversation_message_active(conversation, message));
        let edits = tool_output::edit_summary(&messages);
        let mut label = if messages.len() == 1 {
            "1 tool call".to_owned()
        } else {
            format!("{} tool calls", messages.len())
        };
        if edits.files > 0 {
            label.push_str(&format!(
                " and edited {} {}",
                edits.files,
                if edits.files == 1 { "file" } else { "files" },
            ));
        }
        let accessible_label = format!(
            "{} {label}{}",
            if expanded { "Collapse" } else { "Expand" },
            edits
                .lines
                .filter(|_| edits.files > 0)
                .map_or(String::new(), |lines| format!(
                    ", {} lines added, {} lines removed",
                    lines.added, lines.removed
                ),),
        );
        let group_conversation = conversation.clone();
        let mut group = div().w_full().min_w_0().px_5().py_1().child(
            self.ghost(
                SharedString::from(group_key.clone()),
                if expanded {
                    "chevron-down"
                } else {
                    "chevron-right"
                },
                label,
            )
            .w_full()
            .justify_start()
            .flex_wrap()
            .h_auto()
            .min_h(px(30.))
            .accessibility_label(accessible_label)
            .when_some(edits.lines.filter(|_| edits.files > 0), |button, lines| {
                button.child(
                    div()
                        .flex()
                        .gap_1()
                        .flex_shrink_0()
                        .child(div().text_color(t.added).child(format!("+{}", lines.added)))
                        .child(
                            div()
                                .text_color(t.removed)
                                .child(format!("−{}", lines.removed)),
                        ),
                )
            })
            .when(active, |button| {
                button.child(motion::active_icon(
                    SharedString::from(format!("tool-active:{group_key}")),
                    "loader",
                    t.accent,
                    true,
                    cx.reduce_motion(),
                ))
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                s.toggle_tool_detail(&group_conversation, &group_key, index, cx)
            })),
        );
        if expanded {
            let mut calls = div()
                .ml_3()
                .pl_3()
                .border_l_1()
                .border_color(t.border)
                .flex()
                .flex_col()
                .gap_2();
            for message in &messages {
                calls = calls.child(self.tool_call(conversation, message, index, cx));
            }
            group = group.child(calls);
        }
        group.into_any_element()
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
        group_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let record = tool_output::display_record(&message.text, message.complete);
        let output = tool_output::parse(&record, message.complete);
        let message_key = format!("{}:{}", message.task_id, message.id);
        let key = format!("tool-detail:{message_key}");
        let expanded = self.folded.contains(&key);
        let active = self.conversation_message_active(conversation, message);
        let failed = output.exit.is_some_and(|code| code != 0)
            || output
                .status
                .is_some_and(|s| matches!(s, "failed" | "declined"));
        let status = if let Some(code) = output.exit {
            format!("Exit {code}")
        } else if active {
            "Running".into()
        } else if let Some(status) = output.status {
            match status {
                "completed" => "Completed",
                "failed" => "Failed",
                "declined" => "Declined",
                "inProgress" | "in progress" => "In progress",
                other => other,
            }
            .to_owned()
        } else if message.complete {
            "Finished".into()
        } else {
            "Incomplete".into()
        };
        let color = if failed {
            t.removed
        } else if output.exit == Some(0) {
            t.added
        } else if active {
            t.accent
        } else {
            t.muted
        };
        let glyph = match output.kind {
            ToolKind::Command => "square-terminal",
            ToolKind::Files => "file",
            ToolKind::Other => "code",
        };
        let title = match (&output.kind, output.title) {
            (ToolKind::Other, "mcpToolCall") => "MCP tool",
            (ToolKind::Other, "dynamicToolCall") => "Tool call",
            (ToolKind::Other, "collabAgentToolCall") => "Agent activity",
            (ToolKind::Other, "webSearch") => "Web search",
            (ToolKind::Other, "imageView") => "View image",
            (ToolKind::Other, "imageGeneration") => "Generate image",
            (ToolKind::Other, "contextCompaction") => "Compact context",
            (_, title) => title,
        };
        let title: String = title.chars().take(160).collect();
        let detail_conversation = conversation.clone();
        let mut card = div()
            .w_full()
            .min_w_0()
            .border_1()
            .border_color(t.border)
            .rounded(px(8.))
            .overflow_hidden()
            .bg(t.inset)
            .child(
                self.ghost(
                    SharedString::from(key.clone()),
                    if expanded {
                        "chevron-down"
                    } else {
                        "chevron-right"
                    },
                    "",
                )
                .w_full()
                .h(px(36.))
                .justify_start()
                .gap_2()
                .px_2()
                .accessibility_label(format!(
                    "{} tool call: {title}",
                    if expanded { "Collapse" } else { "Expand" }
                ))
                .child(icon(glyph).text_color(color))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(t.text)
                        .when(output.kind == ToolKind::Command, |d| {
                            d.font_family("JetBrains Mono").text_size(px(11.))
                        })
                        .child(title),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(10.))
                        .text_color(color)
                        .child(status),
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.toggle_tool_detail(&detail_conversation, &key, group_index, cx)
                })),
            );
        if expanded {
            let mut details = div()
                .p_3()
                .border_t_1()
                .border_color(t.border)
                .flex()
                .flex_col()
                .gap_2();
            if output.kind == ToolKind::Command {
                details = details.child(self.message_code(
                    &format!("tool-command:{message_key}"),
                    "sh",
                    output.title,
                    Some(140.),
                    false,
                    cx,
                ));
            }
            if !output.body.is_empty() {
                let mut body = tool_output::display_text(output.body);
                let mut language = "";
                // Pretty-print standalone structured responses, while leaving logs and shell output literal.
                if body.len() <= 64 * 1024
                    && let Ok(value @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) =
                        serde_json::from_str::<serde_json::Value>(&body)
                {
                    body = serde_json::to_string_pretty(&value).unwrap_or(body);
                    language = "json";
                }
                let (excerpt, truncated) = tool_output::preview(&body);
                let full_key = format!("tool-full:{message_key}");
                let full = self.folded.contains(&full_key);
                let copied = output.body.to_owned();
                details = details.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(10.))
                        .text_color(t.muted)
                        .child(if output.kind == ToolKind::Files {
                            format!("{} changed paths", body.lines().count())
                        } else {
                            format!("Output · {} lines", body.lines().count())
                        })
                        .child(div().flex_1())
                        .child(
                            self.icon_button(
                                SharedString::from(format!("copy-output:{message_key}")),
                                "copy",
                                "Copy full output",
                            )
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))
                            }),
                        ),
                );
                details = details.child(self.message_code(
                    &format!("tool-output:{message_key}"),
                    language,
                    if full { &body } else { &excerpt },
                    Some(280.),
                    false,
                    cx,
                ));
                if truncated {
                    let full_conversation = conversation.clone();
                    details = details.child(
                        self.ghost(
                            SharedString::from(full_key.clone()),
                            if full { "minus" } else { "plus" },
                            if full {
                                "Show preview"
                            } else {
                                "Show full output"
                            },
                        )
                        .justify_start()
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.toggle_tool_detail(&full_conversation, &full_key, group_index, cx)
                        })),
                    );
                }
            } else {
                details = details.child(div().text_size(px(12.)).text_color(t.muted).child(
                    if active {
                        "Waiting for output…"
                    } else {
                        "No output recorded."
                    },
                ));
            }
            let copied = message.text.clone();
            details = details.child(
                self.ghost(
                    SharedString::from(format!("copy-tool:{message_key}")),
                    "copy",
                    "Copy tool call",
                )
                .justify_start()
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))
                }),
            );
            card = card.child(details);
        }
        card.into_any_element()
    }
}
