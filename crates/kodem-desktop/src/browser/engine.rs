//! The macOS browser: a WebKit view for each task, placed over the side pane of the window.
//!
//! Each page lies in a host view of its own, a subview of GPUI's view that the pane places over
//! itself, so the page draws above GPUI and only while the pane shows it. A docked Web Inspector
//! shares the host with the page: WebKit splits and restores the page's superview, which must
//! therefore be the pane and not the whole window. Commands of the page script reply over a WebKit message handler of Kodem's own, with the
//! nonce of the command; native commands, such as a screenshot, reply from their completion
//! handlers. wry's own channel drops the messages of `file:` pages, whose address its request
//! type cannot hold.
use super::{Event, Job, Outcome, Page, Status, Target};
use base64::Engine as _;
use block2::RcBlock;
use futures::channel::mpsc::UnboundedSender;
use gpui::{Bounds, Pixels, Window};
use kodem_core::browser::{BrowserCommand, BrowserHistory, BrowserReply};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, ProtocolObject},
};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{
    NSDictionary, NSError, NSNumber, NSObjectProtocol, NSString, NSURL, ns_string,
};
use objc2_web_kit::{
    WKScriptMessage, WKScriptMessageHandler, WKSnapshotConfiguration, WKUserContentController,
};
use serde_json::Value;
use std::{
    collections::{HashMap, hash_map::Entry},
    path::Path,
    ptr::NonNull,
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};
use wry::{
    NewWindowResponse, PageLoadEvent, Rect, WebView, WebViewBuilder, WebViewExtMacOS,
    dpi::{LogicalPosition, LogicalSize},
    raw_window_handle::{
        AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
    },
};

/// The page script, which runs before the scripts of each page.
const AGENT: &str = include_str!("agent.js");
/// The value that a command script ends with once it has started.
const STARTED: &str = "\"kodem:started\"";
/// The size of a page that the pane has not shown yet, in points.
const DEFAULT_SIZE: (f64, f64) = (1024., 768.);
/// The smallest zoom that fits a wide page into the pane.
const MIN_ZOOM: f64 = 0.3;

/// A command of the page script that waits for its reply.
struct Pending {
    task_id: String,
    reply: mpsc::Sender<Outcome>,
    settle: Option<Arc<Mutex<Status>>>,
}

type PendingMap = Arc<Mutex<HashMap<String, Pending>>>;

/// The commands that wait for the replies of a page, and where its clicks are reported.
struct RepliesIvars {
    task_id: String,
    pending: PendingMap,
    events: UnboundedSender<Event>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the handler does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KodemBrowserReplies"]
    #[ivars = RepliesIvars]
    /// Receives the replies of the page script.
    struct Replies;

    unsafe impl NSObjectProtocol for Replies {}

    unsafe impl WKScriptMessageHandler for Replies {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
            // SAFETY: WebKit passes a valid message on the main thread.
            let body = unsafe { message.body() };
            if let Ok(body) = body.downcast::<NSString>() {
                receive(self.ivars(), &body.to_string());
            }
        }
    }
);

impl Replies {
    // Creates the handler of one page.
    fn new(
        task_id: String,
        pending: PendingMap,
        events: UnboundedSender<Event>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RepliesIvars {
            task_id,
            pending,
            events,
        });
        // SAFETY: NSObject's init is the designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

struct Tab {
    view: WebView,
    /// The view that holds the page and a docked Web Inspector, placed over the pane.
    host: Retained<NSView>,
    /// GPUI's view, which holds the host and takes the keyboard back from the page.
    window_view: Retained<NSView>,
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
        let pane = self.host.frame().size.width;
        let zoom = self
            .content_width
            .filter(|_| pane > 0.)
            .map_or(1., |width| (pane / width).clamp(MIN_ZOOM, 1.));
        if (zoom - self.zoom).abs() > 0.005 {
            let _ = self.view.zoom(zoom);
            self.zoom = zoom;
        }
    }

    // Gives the keyboard to GPUI's view, away from the page and its Web Inspector.
    fn release_focus(&self) {
        if let Some(window) = self.host.window() {
            window.makeFirstResponder(Some(&self.window_view));
        }
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        self.host.removeFromSuperview();
    }
}

/// A host view as the window handle that wry takes for the parent of a page.
struct HostHandle<'a>(&'a NSView);

impl HasWindowHandle for HostHandle<'_> {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = AppKitWindowHandle::new(NonNull::from(self.0).cast());
        // SAFETY: The host outlives the handle, which wry uses only while it builds the page.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(handle)) })
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
}

impl Browsers {
    /// Tells whether this platform has the browser.
    pub const SUPPORTED: bool = true;

    /// Creates the browser, which reports page changes to `events`.
    pub fn new(events: UnboundedSender<Event>) -> Self {
        Self {
            tabs: HashMap::new(),
            pending: Arc::default(),
            events,
            shown: None,
            bounds: None,
        }
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

    /// Opens Web Inspector for the page of a task.
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
    /// frame. Called while the pane paints.
    pub fn place(&mut self, task_id: &str, bounds: Bounds<Pixels>) {
        self.bounds = Some(bounds);
        if self.shown.as_deref() != Some(task_id) {
            return;
        }
        let Some(tab) = self.tabs.get_mut(task_id) else {
            return;
        };
        // Compared with the host's own frame, so that a frame that changed outside the pane
        // returns to the pane.
        let frame = frame(&tab.window_view, bounds);
        if tab.host.frame() != frame {
            tab.host.setFrame(frame);
            tab.fit();
        }
        if !tab.visible {
            tab.host.setHidden(false);
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
                tab.release_focus();
                tab.host.setHidden(true);
                tab.visible = false;
            }
        }
    }

    /// Gives the keyboard back to the window after a click outside the page.
    pub fn release_focus(&self) {
        if let Some(tab) = self.tabs.values().find(|tab| tab.visible) {
            tab.release_focus();
        }
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
        let tab = match self.tabs.entry(task_id.to_owned()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(create(
                task_id,
                window,
                &self.pending,
                &self.events,
                self.bounds,
            )?),
        };
        // The load events report a failed load with nothing, so the page counts as loading
        // from the request on, and a page that never loads stops counting after a while.
        if let Ok(mut status) = tab.status.lock() {
            status.loading = Some(Instant::now());
            status.url = target.url();
        }
        match target {
            Target::Web(url) => tab.view.load_url(url).map_err(|error| error.to_string())?,
            Target::File { path, access } => {
                let file = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
                let folder = NSURL::fileURLWithPath(&NSString::from_str(&access.to_string_lossy()));
                // SAFETY: Both are file URLs, and the view lives on the main thread.
                unsafe {
                    tab.view
                        .webview()
                        .loadFileURL_allowingReadAccessToURL(&file, &folder);
                }
            }
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
        let view = tab.view.webview();
        // SAFETY: The view lives on the main thread.
        let navigation = unsafe {
            match action {
                BrowserHistory::Back => view.goBack(),
                BrowserHistory::Forward => view.goForward(),
                BrowserHistory::Reload => view.reload(),
            }
        };
        if navigation.is_none() {
            return Err(match action {
                BrowserHistory::Back => "There is no page to go back to.",
                BrowserHistory::Forward => "There is no page to go forward to.",
                BrowserHistory::Reload => "The page cannot be loaded again.",
            }
            .into());
        }
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

// Creates the page of a task as a hidden subview of the window's view.
fn create(
    task_id: &str,
    window: &Window,
    pending: &PendingMap,
    events: &UnboundedSender<Event>,
    bounds: Option<Bounds<Pixels>>,
) -> Result<Tab, String> {
    let mtm = MainThreadMarker::new().ok_or("The browser runs off the main thread.")?;
    let status = Arc::new(Mutex::new(Status::default()));
    let loads = (
        status.clone(),
        events.clone(),
        task_id.to_owned(),
        pending.clone(),
    );
    let titles = (status.clone(), events.clone());
    let windows = (events.clone(), task_id.to_owned());
    let window_view = window_view(window)?;
    let frame = bounds.map_or(
        CGRect::new(
            CGPoint::new(0., 0.),
            CGSize::new(DEFAULT_SIZE.0, DEFAULT_SIZE.1),
        ),
        |bounds| frame(&window_view, bounds),
    );
    let host = NSView::initWithFrame(NSView::alloc(mtm), frame);
    host.setHidden(true);
    window_view.addSubview(&host);
    let view = WebViewBuilder::new()
        .with_initialization_script_for_main_only(AGENT, true)
        .with_on_page_load_handler(move |event, url| {
            let (status, events, task_id, pending) = &loads;
            let started = matches!(event, PageLoadEvent::Started);
            if let Ok(mut status) = status.lock() {
                status.loading = started.then(Instant::now);
                status.url = url;
            }
            if started {
                abandon(pending, task_id);
            }
            let _ = events.unbounded_send(Event::Changed);
        })
        .with_document_title_changed_handler(move |title| {
            let (status, events) = &titles;
            if let Ok(mut status) = status.lock() {
                status.title = title;
            }
            let _ = events.unbounded_send(Event::Changed);
        })
        .with_new_window_req_handler(move |url, _| {
            let (events, task_id) = &windows;
            let _ = events.unbounded_send(Event::NewWindow(task_id.clone(), url));
            NewWindowResponse::Deny
        })
        .with_devtools(true)
        .with_accept_first_mouse(true)
        .with_bounds(Rect {
            position: LogicalPosition::new(0., 0.).into(),
            size: LogicalSize::new(frame.size.width, frame.size.height).into(),
        })
        .build_as_child(&HostHandle(&host))
        .map_err(|error| {
            host.removeFromSuperview();
            format!("Kodem could not create the browser: {error}")
        })?;
    // The page fills its host and follows its size, except while WebKit docks the inspector.
    view.webview().setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    let replies = Replies::new(task_id.to_owned(), pending.clone(), events.clone(), mtm);
    // SAFETY: The controller keeps the handler until the page is gone; both are on the main
    // thread.
    unsafe {
        view.manager()
            .addScriptMessageHandler_name(ProtocolObject::from_ref(&*replies), ns_string!("kodem"));
    }
    Ok(Tab {
        view,
        host,
        window_view,
        status,
        visible: false,
        content_width: None,
        zoom: 1.,
    })
}

// Hands a reply of the page script to the command that waits for it, or reports a click.
fn receive(ivars: &RepliesIvars, body: &str) {
    let Ok(message) = serde_json::from_str::<Value>(body) else {
        return;
    };
    if message["focus"].as_bool() == Some(true) {
        let _ = ivars.events.unbounded_send(Event::Focused);
        return;
    }
    if let (Some(width), Some(viewport)) = (message["fit"].as_f64(), message["viewport"].as_f64()) {
        let _ = ivars.events.unbounded_send(Event::Fit {
            task_id: ivars.task_id.clone(),
            width,
            viewport,
            fresh: message["fresh"].as_bool() == Some(true),
        });
        return;
    }
    let Some(entry) = message["nonce"]
        .as_str()
        .and_then(|nonce| ivars.pending.lock().ok()?.remove(nonce))
    else {
        return;
    };
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

// Captures the visible part of a page at one pixel per point.
fn screenshot(tab: &Tab, reply: mpsc::Sender<Outcome>) {
    let Some(mtm) = MainThreadMarker::new() else {
        let _ = reply.send(Outcome::error("The browser runs off the main thread."));
        return;
    };
    let view = tab.view.webview();
    let url = tab
        .status
        .lock()
        .map(|status| status.url.clone())
        .unwrap_or_default();
    let hidden = tab.host.isHidden();
    // The agent's cursor would cover the page. WebKit runs the script before it draws the
    // snapshot, which waits for the next screen update.
    let _ = tab
        .view
        .evaluate_script("window.__kodem && window.__kodem.conceal();");
    // SAFETY: The configuration and the view are used on the main thread.
    let configuration = unsafe { WKSnapshotConfiguration::new(mtm) };
    let scale = view
        .window()
        .map_or(1., |window| window.backingScaleFactor());
    // SAFETY: The width is a number, as the property requires.
    unsafe {
        configuration.setSnapshotWidth(Some(&NSNumber::new_f64(view.bounds().size.width / scale)));
        configuration.setAfterScreenUpdates(true);
    }
    let handler = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
        // SAFETY: WebKit passes an image or an error, each valid for the call.
        let (image, error) = unsafe { (image.as_ref(), error.as_ref()) };
        let outcome = match image.and_then(png) {
            Some((data, width, height)) => Outcome::reply(BrowserReply::Image {
                png: base64::engine::general_purpose::STANDARD.encode(data),
                text: format!("Screenshot of {url}, {width}×{height} pixels."),
            }),
            None => {
                let reason = error
                    .map(|error| format!(": {}", error.localizedDescription()))
                    .unwrap_or_default();
                let hint = if hidden {
                    " The task's browser pane is not on screen; open the task in Kodem and try again."
                } else {
                    ""
                };
                Outcome::error(format!("Kodem could not capture the page{reason}.{hint}"))
            }
        };
        let _ = reply.send(outcome);
    });
    // SAFETY: The handler lives until WebKit calls it, which it does on the main thread.
    unsafe {
        view.takeSnapshotWithConfiguration_completionHandler(Some(&configuration), &handler);
    }
}

// Encodes an image as PNG, with its size in pixels.
fn png(image: &NSImage) -> Option<(Vec<u8>, isize, isize)> {
    let tiff = image.TIFFRepresentation()?;
    let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
    // SAFETY: Empty properties are valid for PNG.
    let data = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some((data.to_vec(), bitmap.pixelsWide(), bitmap.pixelsHigh()))
}

// Returns GPUI's view, which holds the pages of the window.
fn window_view(window: &Window) -> Result<Retained<NSView>, String> {
    // GPUI's own `window_handle` names the window in GPUI; this one is the platform's.
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("Kodem could not reach its window: {error}"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("Kodem's window is not an AppKit window.".into());
    };
    // SAFETY: GPUI's handle points to its live view, which is used on the main thread.
    unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) }
        .ok_or_else(|| "Kodem's window has no view.".into())
}

// Returns the frame in GPUI's view that covers `bounds` of the window, in points.
fn frame(window_view: &NSView, bounds: Bounds<Pixels>) -> CGRect {
    let x = f64::from(f32::from(bounds.origin.x));
    let top = f64::from(f32::from(bounds.origin.y));
    let width = f64::from(f32::from(bounds.size.width));
    let height = f64::from(f32::from(bounds.size.height));
    // GPUI measures from the top; an unflipped AppKit view measures from the bottom.
    let y = if window_view.isFlipped() {
        top
    } else {
        window_view.bounds().size.height - top - height
    };
    CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
}
