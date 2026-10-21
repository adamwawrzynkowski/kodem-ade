//! Window-sized attachment previews using the shared workspace controls and palette.
use super::*;
use std::path::PathBuf;

const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 4.;
const ZOOM_STEP: f32 = 1.25;

pub(super) struct ImagePreview {
    path: PathBuf,
    name: String,
    zoom: f32,
    focus: FocusHandle,
    scroll: ScrollHandle,
}

impl Shell {
    /// Opens an imported image fitted to the window, independently of its draft or message.
    pub(super) fn open_image_preview(
        &mut self,
        attachment: &Attachment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.image_preview = Some(ImagePreview {
            path: attachment.path.clone(),
            name: attachment.name.clone(),
            zoom: 1.,
            focus,
            scroll: ScrollHandle::new(),
        });
        self.dragging_sidebar = false;
        self.dragging_review = false;
        self.dragging_terminal = None;
        cx.notify();
    }

    fn close_image_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.image_preview = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn zoom_image_preview(&mut self, factor: f32, cx: &mut Context<Self>) {
        if let Some(preview) = &mut self.image_preview {
            preview.zoom = (preview.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            cx.notify();
        }
    }

    /// Renders the active preview with isolated keyboard focus and a scrollable zoom area.
    pub(super) fn render_image_preview(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let preview = self.image_preview.as_ref()?;
        let t = self.theme;
        // Outer margins, panel borders, toolbar and image padding leave this fit area.
        let viewport = window.viewport_size();
        let scale = crate::theme::ui_scale();
        let width = (f32::from(viewport.width) / scale - 58.).max(1.);
        let height = (f32::from(viewport.height) / scale - 134.).max(1.);
        let image_width = width * preview.zoom;
        let image_height = height * preview.zoom;

        let surface = div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .rounded(px(crate::theme::RADIUS_FLOATING))
            .border_1()
            .border_color(t.edge)
            .bg(t.inset)
            .shadow(t.floating())
            .overflow_hidden()
            .child(
                // The window title is a block title: the file name, a dotted rule, then zoom.
                kit::block_title(
                    div()
                        .flex_shrink(1.)
                        .min_w_0()
                        .text_ellipsis()
                        .font_family(crate::theme::mono())
                        .text_size(text_px(11.5))
                        .text_color(t.text)
                        .child(preview.name.clone()),
                    t.info,
                    t,
                )
                .h(px(42.))
                .pl(px(14.))
                .pr(px(8.))
                .gap(px(6.))
                .child(
                    self.icon_button("image-zoom-out", "minus", "Zoom out")
                        .disabled(preview.zoom <= MIN_ZOOM)
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.zoom_image_preview(1. / ZOOM_STEP, cx);
                        })),
                )
                .child(
                    div()
                        .w(px(84.))
                        .flex()
                        .justify_center()
                        .child(caps(format!("{:.0}% of fit", preview.zoom * 100.), t)),
                )
                .child(
                    self.icon_button("image-zoom-in", "plus", "Zoom in")
                        .disabled(preview.zoom >= MAX_ZOOM)
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.zoom_image_preview(ZOOM_STEP, cx);
                        })),
                )
                .child(
                    self.icon_button("image-close", "close", "Close preview · Esc")
                        .on_click(cx.listener(|s, _, window, cx| {
                            s.close_image_preview(window, cx);
                        })),
                ),
            )
            .child(
                div().flex_1().min_h_0().p_3().child(
                    div()
                        .id("image-preview-scroll")
                        .size_full()
                        .overflow_scroll()
                        .track_scroll(&preview.scroll)
                        .child(
                            div()
                                .w(px(image_width.max(width)))
                                .h(px(image_height.max(height)))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    img(preview.path.clone())
                                        .w(px(image_width))
                                        .h(px(image_height))
                                        .flex_shrink_0()
                                        .object_fit(ObjectFit::Contain)
                                        .with_loading(move || {
                                            kit::loader_row("Loading image…", t).into_any_element()
                                        })
                                        .with_fallback(move || {
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.))
                                                .child(kit::chip("ERROR", t.removed))
                                                .child(figure(
                                                    "This image could not be loaded.",
                                                    t.muted,
                                                ))
                                                .into_any_element()
                                        }),
                                ),
                        ),
                ),
            );
        Some(
            div()
                .id("image-preview")
                .track_focus(&preview.focus)
                .key_context("ImagePreview")
                .tab_group()
                .size_full()
                .flex()
                .flex_col()
                .p(px(16.))
                // Leave the native window controls accessible above the preview.
                .pt(gpui::px(chrome::TITLE_BAR) + px(4.))
                .bg(t.sidebar)
                .text_color(t.text)
                .font_family(crate::theme::sans())
                .text_size(text_px(12.))
                .on_action(cx.listener(|s, _: &Cancel, window, cx| {
                    s.close_image_preview(window, cx);
                }))
                .on_action(cx.listener(Self::request_quit))
                .on_action(cx.listener(Self::close_window))
                .child(
                    div()
                        .relative()
                        .size_full()
                        .min_h_0()
                        .child(dither::shadow(t, crate::theme::RADIUS_FLOATING, 6.))
                        .child(surface),
                ),
        )
    }
}
