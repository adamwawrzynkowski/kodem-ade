use super::*;
impl Shell {
    /// Builds a persistent file list beside a selectable, read-only source preview.
    pub(super) fn file_browser(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut files = div()
            .id("project-files")
            .w(px(248.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .py_2()
            .bg(t.sidebar)
            .border_r_1()
            .border_color(t.border);
        for (i, path) in self.files.iter().take(5000).cloned().enumerate() {
            let label = String::from_utf8_lossy(&path).into_owned();
            let selected = self.file_text.as_ref().is_some_and(|(p, _)| *p == label);
            let task = self.selected.clone();
            files = files.child(
                div()
                    .id(("file", i))
                    .h(px(29.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(t.selected))
                    .hover(move |d| d.bg(t.raised))
                    .child(icon("file").size(px(13.)).text_color(t.muted))
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
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
                    })),
            );
        }
        if self.files.len() > 5000 {
            files = files.child(
                div()
                    .p_3()
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .child("Showing the first 5,000 paths."),
            );
        }
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
                    div()
                        .h(px(40.))
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_b_1()
                        .border_color(t.border)
                        .bg(t.canvas)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(px(12.))
                                .child(path),
                        )
                        .child(
                            self.ghost("reference-file", "paperclip", "Reference")
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
                        .p_4()
                        .font_family("JetBrains Mono")
                        .text_size(px(12.))
                        .child(input),
                );
        } else {
            preview = preview.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_color(t.muted)
                    .child(icon("file-text").size(px(28.)))
                    .child(div().text_size(px(13.)).child("Select a file to preview"))
                    .child(
                        div()
                            .text_size(px(11.))
                            .child("Attach its path to your next message."),
                    ),
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
