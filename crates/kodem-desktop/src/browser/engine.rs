//! The browser: a web view of the platform for each task, placed over the side pane of the
//! window. What differs between macOS's WebKit, Windows's WebView2 and Linux's WebKitGTK lies in
//! `native`: how a page joins the window, its messages, file pages, screenshots and the keyboard.
//!
//! Commands of the page script reply over a message handler of Kodem's own, with the nonce of the
//! command; native commands, such as a screenshot, reply from their completion handlers. wry's own
//! channel drops the messages of `file:` pages, whose address its request type cannot hold.
use super::{Event, Job, Outcome, Page, Status, Target, native};
use base64::Engine as _;
use futures::channel::mpsc::UnboundedSender;
use gpui::{Bounds, Pixels, Window};
use kodem_core::browser::{BrowserCommand, BrowserHistory, BrowserReply};
use serde_json::Value;
use std::{
    collections::{HashMap, hash_map::Entry},
    io::Cursor,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};
use wry::{NewWindowResponse, PageLoadEvent, WebView};

/// The page script, which runs before the scripts of each page.
const AGENT: &str = include_str!("agent.js");
/// The script that reports the user's clicks in each frame of a page.
const FOCUS: &str = include_str!("focus.js");
/// The value that a command script ends with once it has started.
const STARTED: &str = "\"kodem:started\"";
/// The smallest zoom that fits a wide page into the pane.
const MIN_ZOOM: f64 = 0.3;

/// A command of the page script that waits for its reply.
struct Pending {
    task_id: String,
    reply: mpsc::Sender<Outcome>,
    settle: Option<Arc<Mutex<Status>>>,
}

type PendingMap = Arc<Mutex<HashMap<String, Pending>>>;

/// Where the messages of one page go: the commands that wait for their replies, and the
/// window's events.
#[derive(Clone)]
pub(super) struct Inbox {
    task_id: String,
    pending: PendingMap,
    events: UnboundedSender<Event>,
}

/// A message of a page that is about the keyboard.
pub(super) enum Signal {
    /// The user pressed the page.
    Clicked,
    /// The page took the keyboard, by a click or otherwise.
    Focused,
}

impl Inbox {
    /// Hands a reply of the page script to the command that waits for it, or applies a measure
    /// of the page. Returns a message about the keyboard for the platform to judge.
    pub(super) fn receive(&self, body: &str) -> Option<Signal> {
        let message = serde_json::from_str::<Value>(body).ok()?;
        match message["focus"].as_str() {
            Some("click") => return Some(Signal::Clicked),
            Some("focus") => return Some(Signal::Focused),
            _ => {}
        }
        if let (Some(width), Some(viewport)) =
            (message["fit"].as_f64(), message["viewport"].as_f64())
        {
            let _ = self.events.unbounded_send(Event::Fit {
                task_id: self.task_id.clone(),
                width,
                viewport,
                fresh: message["fresh"].as_bool() == Some(true),
            });
            return None;
        }
        let entry = message["nonce"]
            .as_str()
            .and_then(|nonce| self.pending.lock().ok()?.remove(nonce))?;
        let text = message["text"].as_str().unwrap_or_default().to_owned();
        let outcome = if message["ok"].as_bool() == Some(true) {
            Outcome::Done {
                reply: BrowserReply::Text(text),
                settle: entry.settle,
            }
        } else {
            Outcome::error(text)
        };
        let _ = entry.reply.send(outcome);
        None
    }

    /// Tells the window that the page may have the keyboard.
    pub(super) fn focused(&self) {
        let _ = self
            .events
            .unbounded_send(Event::Focused(self.task_id.clone()));
    }
}

struct Tab {
    view: WebView,
    /// What the platform needs to place the page over the pane.
    host: native::Host,
    status: Arc<Mutex<Status>>,
    visible: bool,
    /// The width in CSS pixels that the content of the present page needs, once it was found
    /// wider than its view.
    content_width: Option<f64>,
    /// The page zoom that fits `content_width` into the pane.
    zoom: f64,
}

impl Tab {
    // Zooms the page out until its content fits the width of the pane, and no further than
    // its own size.
    fn fit(&mut self) {
        let pane = self.host.width();
        let zoom = self
            .content_width
            .filter(|_| pane > 0.)
            .map_or(1., |width| (pane / width).clamp(MIN_ZOOM, 1.));
        if (zoom - self.zoom).abs() > 0.005 {
            let _ = self.view.zoom(zoom);
            self.zoom = zoom;
        }
    }
}

/// The pages of the window's tasks.
pub struct Browsers {
    tabs: HashMap<String, Tab>,
    pending: PendingMap,
    events: UnboundedSender<Event>,
    /// The task whose page the pane shows in this frame.
    shown: Option<String>,
    /// Where the pane showed a page last, which a new page takes until the pane shows it.
    bounds: Option<Bounds<Pixels>>,
    /// What the pages of the window share on this platform.
    context: native::Context,
}

impl Browsers {
    /// Creates the browser, which reports page changes to `events`.
    pub fn new(events: UnboundedSender<Event>) -> Self {
        Self {
            tabs: HashMap::new(),
            pending: Arc::default(),
            events,
            shown: None,
            bounds: None,
            context: native::Context::new(),
        }
    }

    /// Returns why this computer cannot show pages, if it cannot.
    pub fn unavailable(&self) -> Option<&'static str> {
        self.context.unavailable()
    }

    /// Returns the page of a task, if it has one.
    pub fn page(&self, task_id: &str) -> Option<Page> {
        let status = self.tabs.get(task_id)?.status.lock().ok()?.clone();
        Some(Page {
            loading: status.is_loading(),
            url: status.url,
        })
    }

    /// Opens `target` in the page of a task, creating the page when the task has none.
    pub fn navigate(
        &mut self,
        task_id: &str,
        target: &Target,
        window: &Window,
    ) -> Result<(), String> {
        self.load(task_id, target, window).map(|_| ())
    }

    /// Goes back or forward in the history of a task's page, or loads it again.
    pub fn go(&mut self, task_id: &str, action: BrowserHistory) -> Result<(), String> {
        self.history(task_id, action).map(|_| ())
    }

    /// Opens the developer tools for the page of a task.
    pub fn inspect(&self, task_id: &str) {
        if let Some(tab) = self.tabs.get(task_id) {
            tab.view.open_devtools();
        }
    }

    /// Runs a command of an agent and sends its outcome. A relative path opens from `folder`.
    pub fn run(&mut self, job: Job, folder: Option<&Path>, window: &Window) {
        let Job {
            task_id,
            command,
            patient,
            reply,
        } = job;
        let outcome = match command {
            BrowserCommand::Open { target } => Target::parse(&target, folder)
                .and_then(|target| {
                    let status = self.load(&task_id, &target, window)?;
                    let _ = self.events.unbounded_send(Event::Opened(task_id.clone()));
                    Ok(Outcome::Done {
                        reply: BrowserReply::Text(format!("Opened {}.", target.url())),
                        settle: Some(status),
                    })
                })
                .unwrap_or_else(Outcome::error),
            BrowserCommand::Navigate { action } => self
                .history(&task_id, action)
                .map(|status| Outcome::Done {
                    reply: BrowserReply::Text(
                        match action {
                            BrowserHistory::Back => "Went back.",
                            BrowserHistory::Forward => "Went forward.",
                            BrowserHistory::Reload => "Reloaded the page.",
                        }
                        .into(),
                    ),
                    settle: Some(status),
                })
                .unwrap_or_else(Outcome::error),
            command => {
                let Some(tab) = self.tabs.get(&task_id) else {
                    let _ = reply.send(Outcome::error(
                        "No page is open in this task's browser. Open one with browser_open.",
                    ));
                    return;
                };
                if patient && tab.status.lock().is_ok_and(|status| status.is_loading()) {
                    let _ = reply.send(Outcome::Loading(tab.status.clone()));
                    return;
                }
                match command {
                    BrowserCommand::Page { action, arguments } => {
                        self.script(&task_id, &action, &arguments, reply);
                    }
                    _ => screenshot(tab, reply),
                }
                return;
            }
        };
        let _ = reply.send(outcome);
    }

    /// Places the page of a task over `bounds` of the window, when the pane shows it this
    /// frame. `scale` is the number of device pixels per pixel of the window. Called while the
    /// pane paints.
    pub fn place(&mut self, task_id: &str, bounds: Bounds<Pixels>, scale: f32) {
        self.bounds = Some(bounds);
        if self.shown.as_deref() != Some(task_id) {
            return;
        }
        let Some(tab) = self.tabs.get_mut(task_id) else {
            return;
        };
        if tab.host.place(&tab.view, bounds, scale) {
            tab.fit();
        }
        if !tab.visible {
            tab.host.show(&tab.view);
            tab.visible = true;
        }
    }

    /// Hides every page but the one of the task that the pane shows in this frame, which shows
    /// once it is placed.
    pub fn end_frame(&mut self, shown: Option<&str>) {
        self.shown = shown.map(str::to_owned);
        for (task_id, tab) in &mut self.tabs {
            if tab.visible && shown != Some(task_id.as_str()) {
                // A hidden page would keep the keyboard from the window.
                native::release_focus(&tab.view, &tab.host);
                tab.host.hide(&tab.view);
                tab.visible = false;
            }
        }
    }

    /// Gives the keyboard back to the window after a click outside the page.
    pub fn release_focus(&self) {
        if let Some(tab) = self.tabs.values().find(|tab| tab.visible) {
            native::release_focus(&tab.view, &tab.host);
        }
    }

    /// Tells whether the page of a task is on screen and has the keyboard.
    pub fn has_focus(&self, task_id: &str) -> bool {
        self.tabs
            .get(task_id)
            .is_some_and(|tab| tab.visible && native::has_focus(&tab.view, &tab.host))
    }

    /// Fits a task's page into the pane after it measured its content. A content wider than
    /// its view sets the width to fit; a new document starts from its own size.
    pub fn fit(&mut self, task_id: &str, width: f64, viewport: f64, fresh: bool) {
        let Some(tab) = self.tabs.get_mut(task_id) else {
            return;
        };
        if fresh {
            tab.content_width = None;
        }
        // A pixel of rounding is not an overflow.
        if width > viewport + 1. {
            tab.content_width = Some(tab.content_width.unwrap_or_default().max(width));
        }
        tab.fit();
    }

    /// Closes the pages of the tasks that `keep` rejects.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.tabs.retain(|task_id, _| keep(task_id));
    }

    // Loads a target in a task's page and returns the state of the page to wait for.
    fn load(
        &mut self,
        task_id: &str,
        target: &Target,
        window: &Window,
    ) -> Result<Arc<Mutex<Status>>, String> {
        if let Some(reason) = self.context.unavailable() {
            return Err(reason.into());
        }
        let tab = match self.tabs.entry(task_id.to_owned()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let inbox = Inbox {
                    task_id: task_id.to_owned(),
                    pending: self.pending.clone(),
                    events: self.events.clone(),
                };
                entry.insert(create(inbox, &mut self.context, window, self.bounds)?)
            }
        };
        // The load events report a failed load with nothing, so the page counts as loading
        // from the request on, and a page that never loads stops counting after a while.
        if let Ok(mut status) = tab.status.lock() {
            status.loading = Some(Instant::now());
            status.url = target.url();
        }
        match target {
            Target::Web(url) => tab.view.load_url(url).map_err(|error| error.to_string())?,
            Target::File { path, access } => native::load_file(&tab.view, path, access)?,
        }
        let _ = self.events.unbounded_send(Event::Changed);
        Ok(tab.status.clone())
    }

    // Steps in a task's history and returns the state of the page to wait for.
    fn history(
        &mut self,
        task_id: &str,
        action: BrowserHistory,
    ) -> Result<Arc<Mutex<Status>>, String> {
        let tab = self
            .tabs
            .get(task_id)
            .ok_or("No page is open in this task's browser.")?;
        let view = &tab.view;
        let possible = match action {
            BrowserHistory::Back => view.can_go_back(),
            BrowserHistory::Forward => view.can_go_forward(),
            BrowserHistory::Reload => Ok(true),
        };
        if !possible.unwrap_or(false) {
            return Err(match action {
                BrowserHistory::Back => "There is no page to go back to.",
                BrowserHistory::Forward => "There is no page to go forward to.",
                BrowserHistory::Reload => "The page cannot be loaded again.",
            }
            .into());
        }
        match action {
            BrowserHistory::Back => view.go_back(),
            BrowserHistory::Forward => view.go_forward(),
            BrowserHistory::Reload => view.reload(),
        }
        .map_err(|error| error.to_string())?;
        if let Ok(mut status) = tab.status.lock() {
            status.loading = Some(Instant::now());
        }
        Ok(tab.status.clone())
    }

    // Runs an action of the page script. Its reply arrives through the message handler.
    fn script(&self, task_id: &str, action: &str, arguments: &Value, reply: mpsc::Sender<Outcome>) {
        let Some(tab) = self.tabs.get(task_id) else {
            return;
        };
        let nonce = uuid::Uuid::new_v4().to_string();
        let nonce_json = Value::String(nonce.clone());
        let call = if action == "evaluate" {
            // The expression is part of the script instead of text that the page evaluates,
            // so a content security policy of the page does not block it.
            let expression = arguments["expression"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .trim_end_matches(';');
            format!(
                "window.__kodem.evaluate({nonce_json}, async () => {{ const value = (\n{expression}\n); return typeof value === \"function\" ? await value() : await value; }});"
            )
        } else {
            format!(
                "window.__kodem.run({nonce_json}, {}, {arguments});",
                Value::String(action.to_owned())
            )
        };
        // Actions that may start a navigation answer once it has loaded.
        let settle =
            matches!(action, "click" | "press" | "type" | "select").then(|| tab.status.clone());
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(
                nonce.clone(),
                Pending {
                    task_id: task_id.to_owned(),
                    reply,
                    settle,
                },
            );
        }
        let failure = if action == "evaluate" {
            "The script did not run. Give one JavaScript expression or function."
        } else {
            "The page did not run the command."
        };
        let pending = self.pending.clone();
        let fail = move || {
            if let Some(entry) = pending.lock().ok().and_then(|mut map| map.remove(&nonce)) {
                let _ = entry.reply.send(Outcome::error(failure));
            }
        };
        let script = format!("{AGENT}\n;{call}\n{STARTED};");
        let started = fail.clone();
        // A script that does not parse or throws before it starts ends without its last value.
        let result = tab
            .view
            .evaluate_script_with_callback(&script, move |value| {
                if value != STARTED {
                    started();
                }
            });
        if result.is_err() {
            fail();
        }
    }
}

// Creates the hidden page of a task, which reports its messages to `inbox`.
fn create(
    inbox: Inbox,
    context: &mut native::Context,
    window: &Window,
    bounds: Option<Bounds<Pixels>>,
) -> Result<Tab, String> {
    let status = Arc::new(Mutex::new(Status::default()));
    let loads = (status.clone(), inbox.clone());
    let titles = (status.clone(), inbox.events.clone());
    let windows = inbox.clone();
    let builder = native::builder(context)
        .with_initialization_script_for_main_only(AGENT, true)
        .with_initialization_script_for_main_only(FOCUS, false)
        .with_on_page_load_handler(move |event, url| {
            let (status, inbox) = &loads;
            let started = matches!(event, PageLoadEvent::Started);
            if let Ok(mut status) = status.lock() {
                status.loading = started.then(Instant::now);
                status.url = url;
            }
            if started {
                abandon(&inbox.pending, &inbox.task_id);
            }
            let _ = inbox.events.unbounded_send(Event::Changed);
        })
        .with_document_title_changed_handler(move |title| {
            let (status, events) = &titles;
            if let Ok(mut status) = status.lock() {
                status.title = title;
            }
            let _ = events.unbounded_send(Event::Changed);
        })
        .with_new_window_req_handler(move |url, _| {
            let _ = windows
                .events
                .unbounded_send(Event::NewWindow(windows.task_id.clone(), url));
            NewWindowResponse::Deny
        })
        .with_devtools(true)
        .with_accept_first_mouse(true)
        // The page takes the keyboard when the user clicks it, not when it opens.
        .with_focused(false);
    let (view, host) = native::create(builder, window, bounds, inbox)?;
    Ok(Tab {
        view,
        host,
        status,
        visible: false,
        content_width: None,
        zoom: 1.,
    })
}

// Ends the commands of a task's page script when the page navigates, because the page that
// would answer them is gone.
fn abandon(pending: &PendingMap, task_id: &str) {
    if let Ok(mut pending) = pending.lock() {
        pending.retain(|_, entry| {
            if entry.task_id != task_id {
                return true;
            }
            let _ = entry.reply.send(Outcome::error(
                "The page navigated away before the command finished. Take a new browser_snapshot.",
            ));
            false
        });
    }
}

// Captures the visible part of a task's page.
fn screenshot(tab: &Tab, reply: mpsc::Sender<Outcome>) {
    let url = tab
        .status
        .lock()
        .map(|status| status.url.clone())
        .unwrap_or_default();
    // The agent's cursor would cover the page. The engines run the script before they draw
    // the capture.
    let _ = tab
        .view
        .evaluate_script("window.__kodem && window.__kodem.conceal();");
    let hint = if tab.visible {
        ""
    } else {
        " The task's browser pane is not on screen; open the task in Kodem and try again."
    };
    native::screenshot(&tab.view, &tab.host, move |capture| {
        let outcome = match capture {
            Ok((png, width, height)) => Outcome::reply(BrowserReply::Image {
                png: base64::engine::general_purpose::STANDARD.encode(png),
                text: format!("Screenshot of {url}, {width}×{height} pixels."),
            }),
            Err(reason) => {
                Outcome::error(format!("Kodem could not capture the page{reason}.{hint}"))
            }
        };
        let _ = reply.send(outcome);
    });
}

/// Encodes a capture as PNG at one pixel per pixel of the window, with its size. `rgba` holds
/// `width` × `height` device pixels, `scale` device pixels per pixel of the window.
// macOS's WebKit captures at the window's own pixels.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(super) fn encode(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    scale: f64,
) -> Result<(Vec<u8>, u32, u32), String> {
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or(": the capture has the wrong size")?;
    let image = if scale > 1. {
        let width = ((f64::from(width) / scale).round() as u32).max(1);
        let height = ((f64::from(height) / scale).round() as u32).max(1);
        image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let mut png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|error| format!(": {error}"))?;
    Ok((png, image.width(), image.height()))
}
