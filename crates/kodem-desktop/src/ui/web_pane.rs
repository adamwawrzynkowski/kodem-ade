use super::*;
use crate::browser::{Event, HOME, Job, Target};
use kodem_core::browser::BrowserHistory;

// The pages that the side pane offers: the page, its icon, its name and its shortcut.
const PANE_PAGES: [(PanePage, &str, &str, &str); 2] = [
    (PanePage::Changes, "panel-right", "Changes", "⌘D"),
    (PanePage::Browser, "globe", "Browser", "⇧⌘B"),
];

impl Shell {
    /// Builds the side pane, which shows the choice of its pages, the changes of the task or its
    /// browser.
    pub(super) fn side_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.pane_page {
            PanePage::Picker => self.pane_picker(cx),
            PanePage::Changes => self.review(cx, false),
            PanePage::Browser => self.web_pane(window, cx),
        }
    }

    /// Returns the width of the side pane beside the conversation, or in its place when the
    /// window is narrow.
    pub(super) fn side_width(&self) -> f32 {
        // The workspace rail, the sidebar when it is open, and the rules between the panes.
        let leading = workspaces::RAIL
            + if self.settings.collapsed {
                2.
            } else {
                self.settings.sidebar_width + 3.
            };
        if self.window_width <= 1100. {
            self.window_width - leading
        } else {
            self.settings.review_width.min(self.window_width * 0.55)
        }
    }

    // Builds the choice of what the side pane shows: one raised row for each page, with the
    // shortcut that opens it.
    fn pane_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut rows = div().w_full().max_w(px(360.)).flex().flex_col().gap(px(6.));
        for (i, (page, glyph, label, keys)) in PANE_PAGES.into_iter().enumerate() {
            rows = rows.child(
                self.button(("pane-page-option", i), glyph, label)
                    .w_full()
                    .h(px(34.))
                    .px(px(12.))
                    .gap(px(10.))
                    .justify_start()
                    .text_size(text_px(12.))
                    .child(div().flex_1())
                    .child(kit::keycap(keys, t))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.show_pane(page);
                        s.save_settings();
                        cx.notify();
                    })),
            );
        }
        let header = div()
            .h(px(36.))
            .flex_shrink_0()
            .px(px(12.))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(t.border)
            .child(kit::section_header("Side pane", None, t));
        div()
            .relative()
            .id("side-picker")
            .w(px(self.side_width()))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .map(|panel| kit::panel_glass(panel, t))
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(rows),
            )
            .into_any_element()
    }

    /// Opens the side pane on a page.
    pub(super) fn show_pane(&mut self, page: PanePage) {
        self.show_review = true;
        self.pane_page = page;
    }

    /// Opens the side pane on a page, or closes it when it shows that page.
    pub(super) fn toggle_pane(&mut self, page: PanePage) {
        if self.show_review && self.pane_page == page {
            self.show_review = false;
            if self.panel == Panel::Comparison {
                self.panel = Panel::None;
            }
        } else {
            self.show_pane(page);
        }
        self.save_settings();
    }

    /// Runs a browser command of an agent of a task.
    pub(super) fn browser_job(&mut self, job: Job, window: &mut Window, cx: &mut Context<Self>) {
        let folder = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.tasks.iter().find(|task| task.id == job.task_id))
            .map(|task| task.cwd.clone());
        self.browsers
            .borrow_mut()
            .run(job, folder.as_deref(), window);
        cx.notify();
    }

    /// Applies a change of a task's page.
    pub(super) fn browser_event(
        &mut self,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            Event::Changed => {}
            // The user sees what an agent of the open task opens.
            Event::Opened(task_id) => {
                if self.selected.as_ref() == Some(&task_id) && self.has_checkout() && !self.git_open
                {
                    self.show_pane(PanePage::Browser);
                    self.save_settings();
                }
            }
            Event::NewWindow(task_id, url) => {
                let opened =
                    self.browsers
                        .borrow_mut()
                        .navigate(&task_id, &Target::Web(url), window);
                if let Err(error) = opened {
                    self.report_error(error, cx);
                }
            }
            // The window's text fields give up the keyboard and the editing shortcuts to the
            // page, once the page has the keyboard indeed: a late message must not take a field's
            // focus after the user clicked back into the window.
            Event::Focused(task_id) => {
                if self.browsers.borrow().has_focus(&task_id) {
                    window.focus(&self.focus, cx);
                }
            }
            Event::Fit {
                task_id,
                width,
                viewport,
                fresh,
            } => self
                .browsers
                .borrow_mut()
                .fit(&task_id, width, viewport, fresh),
        }
        cx.notify();
    }

    /// Opens what the address field holds in the open task's page.
    pub(super) fn browser_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task_id) = self.selected.clone() else {
            return;
        };
        let text = self.browser_address.read(cx).text();
        let folder = self
            .view
            .as_ref()
            .filter(|view| view.task.id == task_id)
            .map(|view| view.task.cwd.clone());
        let opened = Target::parse(&text, folder.as_deref()).and_then(|target| {
            self.browsers
                .borrow_mut()
                .navigate(&task_id, &target, window)?;
            Ok(target.url())
        });
        match opened {
            Ok(url) => {
                self.browser_address.update(cx, |input, cx| {
                    input.set_text(url.clone(), cx);
                });
                self.browser_address_url = Some((task_id, url));
                // Out of the field, the address follows the page again.
                window.focus(&self.focus, cx);
            }
            Err(error) => self.report_error(error, cx),
        }
        cx.notify();
    }

    // Steps in the history of the open task's page.
    fn browser_go(&mut self, action: BrowserHistory, cx: &mut Context<Self>) {
        let Some(task_id) = &self.selected else {
            return;
        };
        let stepped = self.browsers.borrow_mut().go(task_id, action);
        if let Err(error) = stepped {
            self.report_error(error, cx);
        }
    }

    // Builds the browser of the open task: its controls, and the place that its page covers.
    fn web_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let task_id = self.selected.clone();
        // A task's browser opens the home page the first time that the pane shows it without a
        // page. A page that the user or an agent opened stays.
        let unavailable = self.browsers.borrow().unavailable();
        if let Some(id) = &task_id
            && unavailable.is_none()
            && self.browsers.borrow().page(id).is_none()
            && self.browser_home.insert(id.clone())
        {
            let opened = self
                .browsers
                .borrow_mut()
                .navigate(id, &Target::Web(HOME.into()), window);
            if let Err(error) = opened {
                self.report_error(error, cx);
            }
        }
        let page = task_id
            .as_ref()
            .and_then(|id| self.browsers.borrow().page(id));
        // The field follows the page, except while the user edits it.
        if let (Some(id), Some(page)) = (&task_id, &page)
            && self.browser_address_url.as_ref() != Some(&(id.clone(), page.url.clone()))
            && !self.browser_address.focus_handle(cx).is_focused(window)
        {
            let url = page.url.clone();
            self.browser_address.update(cx, |input, cx| {
                input.set_text(url.clone(), cx);
            });
            self.browser_address_url = Some((id.clone(), url));
        } else if page.is_none()
            && self.browser_address_url.is_some()
            && !self.browser_address.focus_handle(cx).is_focused(window)
        {
            self.browser_address
                .update(cx, |input, cx| input.set_text(String::new(), cx));
            self.browser_address_url = None;
        }
        let open = page.is_some();
        let loading = page.as_ref().is_some_and(|page| page.loading);
        let header = div()
            .h(px(36.))
            .flex_shrink_0()
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(2.))
            .border_b_1()
            .border_color(t.border)
            .child(
                self.icon_button("browser-back", "arrow-left", "Back")
                    .disabled(!open)
                    .on_click(cx.listener(|s, _, _, cx| s.browser_go(BrowserHistory::Back, cx))),
            )
            .child(
                self.icon_button("browser-forward", "arrow-right", "Forward")
                    .disabled(!open)
                    .on_click(cx.listener(|s, _, _, cx| s.browser_go(BrowserHistory::Forward, cx))),
            )
            .child(
                self.icon_button("browser-reload", "refresh-cw", "Reload")
                    .disabled(!open)
                    .on_click(cx.listener(|s, _, _, cx| s.browser_go(BrowserHistory::Reload, cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(72.))
                    .h(px(24.))
                    .mx(px(4.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(crate::theme::RADIUS_CONTROL))
                    .bg(t.inset)
                    .border_1()
                    .border_color(t.border)
                    .font_family(crate::theme::mono())
                    .text_size(text_px(11.5))
                    .child(
                        icon(if loading { "loader" } else { "globe" })
                            .size(px(12.))
                            .text_color(if loading { t.accent } else { t.subtle }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(self.browser_address.clone()),
                    ),
            )
            .child(
                self.icon_button("browser-inspect", "scan-eye", "Inspect the page")
                    .disabled(!open)
                    .on_click(cx.listener(|s, _, _, _| {
                        if let Some(task_id) = &s.selected {
                            s.browsers.borrow().inspect(task_id);
                        }
                    })),
            );
        let body = match (&task_id, open) {
            _ if let Some(reason) = unavailable => kit::empty_state(
                "No browser here",
                reason,
                dither::DitherField::Glow {
                    x: 0.5,
                    y: 0.55,
                    radius: 0.7,
                    strength: 0.85,
                },
                t,
            )
            .into_any_element(),
            (Some(task_id), true) => {
                // A page is a view of the platform over the window. It shows once the pane has
                // stopped sliding, and only while no menu or dialog lies over it.
                if self.window_width <= 1100. || self.reveals.review >= 1. {
                    self.browser_frame = Some(task_id.clone());
                }
                let browsers = self.browsers.clone();
                let task_id = task_id.clone();
                div()
                    .flex_1()
                    .min_h_0()
                    .bg(t.canvas)
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                browsers.borrow_mut().place(
                                    &task_id,
                                    bounds,
                                    window.scale_factor(),
                                );
                                // A press anywhere in the window takes the keyboard back from
                                // the page. The listener is the window's own, so a layer that
                                // occludes the press, such as the composer, does not hide it.
                                // A press on the page goes to the page, not to the window.
                                let browsers = browsers.clone();
                                window.on_mouse_event(move |_: &MouseDownEvent, phase, _, _| {
                                    if phase == DispatchPhase::Capture {
                                        browsers.borrow().release_focus();
                                    }
                                });
                            },
                        )
                        .size_full(),
                    )
                    .into_any_element()
            }
            _ => kit::empty_state(
                "Open a page",
                "Type an address or a file path above. The agents of this task open pages here too, so you can watch them test their work.",
                dither::DitherField::Glow {
                    x: 0.5,
                    y: 0.55,
                    radius: 0.7,
                    strength: 0.85,
                },
                t,
            )
            .into_any_element(),
        };
        div()
            .relative()
            .id("side-browser")
            .w(px(self.side_width()))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .map(|panel| kit::panel_glass(panel, t))
            .overflow_hidden()
            .child(header)
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
            .into_any_element()
    }
}
