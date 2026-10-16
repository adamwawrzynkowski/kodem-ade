//! The browser of platforms that have none yet. The pane says so, and agents read why.
use super::{Event, Job, Outcome, Page, Target};
use futures::channel::mpsc::UnboundedSender;
use gpui::{Bounds, Pixels, Window};
use kodem_core::browser::BrowserHistory;
use std::path::Path;

/// Why this platform has no browser.
const UNSUPPORTED: &str = "Kodem's browser is available on macOS only for now.";

/// The pages of the window's tasks, of which this platform has none.
pub struct Browsers;

impl Browsers {
    /// Tells whether this platform has the browser.
    pub const SUPPORTED: bool = false;

    /// Creates the browser.
    pub fn new(_events: UnboundedSender<Event>) -> Self {
        Self
    }

    /// Returns the page of a task, which is always none.
    pub fn page(&self, _task_id: &str) -> Option<Page> {
        None
    }

    /// Refuses to open a page.
    pub fn navigate(
        &mut self,
        _task_id: &str,
        _target: &Target,
        _window: &Window,
    ) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }

    /// Refuses to step in a history.
    pub fn go(&mut self, _task_id: &str, _action: BrowserHistory) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }

    /// Does nothing, as there is no page to inspect.
    pub fn inspect(&self, _task_id: &str) {}

    /// Answers the command of an agent with the reason that it cannot run.
    pub fn run(&mut self, job: Job, _folder: Option<&Path>, _window: &Window) {
        let _ = job.reply.send(Outcome::error(UNSUPPORTED));
    }

    /// Does nothing, as there is no page to place.
    pub fn place(&mut self, _task_id: &str, _bounds: Bounds<Pixels>) {}

    /// Does nothing, as there is no page to hide.
    pub fn end_frame(&mut self, _shown: Option<&str>) {}

    /// Does nothing, as no page takes the keyboard.
    pub fn release_focus(&self) {}

    /// Does nothing, as there is no page to fit.
    pub fn fit(&mut self, _task_id: &str, _width: f64, _viewport: f64, _fresh: bool) {}

    /// Does nothing, as there is no page to close.
    pub fn retain(&mut self, _keep: impl Fn(&str) -> bool) {}
}
