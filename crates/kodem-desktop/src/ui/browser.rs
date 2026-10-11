use super::*;
impl Shell {
    /// Builds a persistent file list beside a selectable, read-only source preview.
    pub(super) fn file_browser(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let entity = cx.entity();
        let files = uniform_list("project-files", self.files.len(), move |range, _, cx| {
            entity.update(cx, |s, cx| {
                range
                    .map(|i| {
                        let path = s.files[i].clone();
                        let label = String::from_utf8_lossy(&path).into_owned();
                        let selected = s.file_text.as_ref().is_some_and(|(p, _)| *p == label);
                        let task = s.selected.clone();
                        div()
                            .id(("file", i))
                            .relative()
                            .h(px(26.))
                            .mx(px(6.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(crate::theme::RADIUS_CONTROL))
                            .cursor_pointer()
                            .when(selected, |d| {
                                d.overflow_hidden()
                                    .bg(t.accent.opacity(0.1))
                                    .text_color(t.text)
                                    .child(dither::row_wash(
                                        t.accent,
                                        crate::theme::RADIUS_CONTROL,
                                        true,
                                        false,
                                    ))
                                    .child(kit::selection_edge(t.accent))
                            })
                            .when(!selected, |d| {
                                d.text_color(t.muted)
                                    .hover(move |d| d.bg(t.selected).text_color(t.text))
                            })
                            .child(icon("file").size(px(13.)).text_color(if selected {
                                t.accent
                            } else {
                                t.subtle
                            }))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_ellipsis_start()
                                    .font_family(crate::theme::MONO)
                                    .text_size(px(11.))
                                    .child(label),
                            )
                            .on_click(cx.listener(move |s, _, _, _| {
                                if let Some(task_id) = &task {
                                    s.send(core::Request::ReadFile {
                                        task_id: task_id.clone(),
                                        path: path.clone(),
                                    });
                                }
                            }))
                    })
                    .collect::<Vec<_>>()
            })
        })
        .w(px(280.))
        .flex_shrink_0()
        .h_full()
        .py(px(6.))
        .bg(t.sidebar)
        .border_r_1()
        .border_color(t.border);
        let mut preview = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.inset);
        if let Some((path, text)) = self.file_text.clone() {
            let extension = path.rsplit('.').next().unwrap_or("");
            let input = self.readonly(format!("viewer:{path}"), text, cx);
            input.update(cx, |i, _| i.code(extension, self.settings.light));
            let reference = path.clone();
            preview = preview
                .child(
                    kit::block_title(
                        div()
                            .flex_shrink(1.)
                            .min_w_0()
                            .text_ellipsis_start()
                            .font_family(crate::theme::MONO)
                            .text_size(px(11.5))
                            .text_color(t.text)
                            .child(path),
                        t.info,
                        t,
                    )
                    .h(px(34.))
                    .pr(px(6.))
                    .child(caps("Preview", t))
                    .child(
                        self.icon_button(
                            "reference-file",
                            "paperclip",
                            "Reference this file in the prompt",
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let mut text = s.input.read(cx).text();
                            text.push_str(&format!("\n@{reference}"));
                            s.input.update(cx, |i, cx| i.set_text(text, cx));
                            s.panel = Panel::None;
                            cx.notify();
                        })),
                    ),
                )
                .child(
                    div()
                        .id("file-content")
                        .flex_1()
                        .min_h_0()
                        .overflow_scroll()
                        .p(px(14.))
                        .font_family(crate::theme::MONO)
                        .text_size(px(12.))
                        .child(input),
                );
        } else {
            preview = preview.child(
                div().flex_1().min_h_0().child(kit::empty_state(
                    "No file selected",
                    "Select a file to preview it. You can attach its path to your next message.",
                    dither::DitherField::Glow {
                        x: 0.5,
                        y: 0.55,
                        radius: 0.7,
                        strength: 0.85,
                    }
                    .times(dither::DitherField::Flow {
                        scale: 0.18,
                        speed: 0.9,
                        bias: 0.12,
                    }),
                    t,
                )),
            );
        }
        div()
            .h(px(550.))
            .flex()
            .child(files)
            .child(preview)
            .into_any_element()
    }
}
