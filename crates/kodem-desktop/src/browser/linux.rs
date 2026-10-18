//! The Linux side of the browser: a WebKitGTK page in an X11 child window of GPUI's window, placed
//! over the pane. wry places a page inside another program's window on X11 only, so the window
//! runs on X11, through XWayland in a Wayland session (see `prefer_x11`). GPUI does not run GTK's
//! loop, so the window runs GTK's waiting events often (see `pump`).
//!
//! X11 sends a key to the window under the pointer, which would hand the page the keys that the
//! user types into the window while the pointer rests on the page. So the page takes keys only
//! from the user's click on it until the window takes the keyboard back; until then its keys go
//! on to GPUI's window.
use super::engine::{Inbox, Signal, encode};
use gpui::{Bounds, Pixels, Window, px, size};
use gtk::{gdk, gio, prelude::*};
use std::{env, ffi::c_ulong, path::Path};
use webkit2gtk::{SnapshotOptions, SnapshotRegion, UserContentManagerExt, WebViewExt};
use wry::{
    Rect, WebContext, WebView, WebViewBuilder, WebViewExtUnix,
    dpi::{PhysicalPosition, PhysicalSize},
    raw_window_handle::{
        HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle,
    },
};

/// The size of a page that the pane has not shown yet, in pixels of the window.
const DEFAULT_SIZE: (f32, f32) = (1024., 768.);

/// Runs the window on X11 when the session offers X11 beside Wayland. The host, and the programs
/// that it starts, keep the session's Wayland display. Called first in `main`.
pub fn prefer_x11() {
    let x11 = env::var_os("DISPLAY").is_some_and(|display| !display.is_empty());
    if let Some(wayland) = env::var_os("WAYLAND_DISPLAY").filter(|display| !display.is_empty())
        && x11
    {
        // SAFETY: `main` calls this before it starts any other thread.
        unsafe {
            env::set_var(kodem_core::ipc::WAYLAND_DISPLAY, wayland);
            env::remove_var("WAYLAND_DISPLAY");
        }
    }
}

/// Runs the GTK events that wait, which the pages need to draw, load and answer. Does nothing
/// before the first page opens GTK.
pub fn pump() {
    if gtk::is_initialized_main_thread() {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }
}

/// What the pages of the window share: their data folder, once GTK has an X11 display.
pub struct Context {
    web: Option<WebContext>,
}

impl Context {
    /// Opens GTK on X11 and creates the shared state of the pages.
    pub fn new() -> Self {
        gdk::set_allowed_backends("x11");
        // GTK would set the locale of the whole process, which the window's libraries read.
        gtk::disable_setlocale();
        let web = gtk::init().is_ok().then(|| {
            WebContext::new(
                kodem_core::ipc::data_dir()
                    .ok()
                    .map(|root| root.join("Browser")),
            )
        });
        Self { web }
    }

    /// Returns why this computer cannot show pages: GTK found no X11 display.
    pub fn unavailable(&self) -> Option<&'static str> {
        self.web.is_none().then_some(
            "Kodem's browser needs an X11 display, which Wayland sessions offer through XWayland.",
        )
    }
}

/// Where a page lies in the window.
pub struct Host {
    /// The bounds of the page in the window and the device pixels per pixel of the window, as
    /// the pane placed it last.
    placed: Option<(Bounds<Pixels>, f32)>,
}

impl Host {
    /// Places the page over `bounds` of the window. Returns whether it moved or changed size.
    pub fn place(&mut self, view: &WebView, bounds: Bounds<Pixels>, scale: f32) -> bool {
        if self.placed == Some((bounds, scale)) {
            return false;
        }
        self.placed = Some((bounds, scale));
        let _ = view.set_bounds(rect(bounds, scale));
        true
    }

    /// Shows the page.
    pub fn show(&self, view: &WebView) {
        let _ = view.set_visible(true);
    }

    /// Hides the page.
    pub fn hide(&self, view: &WebView) {
        let _ = view.set_visible(false);
    }

    /// Returns the width of the pane, in pixels of the window.
    pub fn width(&self) -> f64 {
        self.placed
            .map_or(0., |(bounds, _)| f64::from(f32::from(bounds.size.width)))
    }
}

/// An X11 window as the window handle that wry takes for the parent of a page.
struct X11Window(c_ulong);

impl HasWindowHandle for X11Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: GPUI's window outlives the handle, which wry uses only while it builds the
        // page.
        Ok(unsafe {
            WindowHandle::borrow_raw(RawWindowHandle::Xlib(XlibWindowHandle::new(self.0)))
        })
    }
}

/// Returns the builder of a page with what this platform sets.
pub fn builder(context: &mut Context) -> WebViewBuilder<'_> {
    let builder = match &mut context.web {
        Some(web) => WebViewBuilder::new_with_web_context(web),
        None => WebViewBuilder::new(),
    };
    // A child window shows at once, so the page waits hidden until the pane places it.
    builder.with_visible(false)
}

/// Creates a page from `builder` as a hidden child window of GPUI's window. Its messages go to
/// `inbox`.
pub fn create(
    builder: WebViewBuilder<'_>,
    window: &Window,
    bounds: Option<Bounds<Pixels>>,
    inbox: Inbox,
) -> Result<(WebView, Host), String> {
    let parent = x11_window(window)?;
    let bounds = bounds.unwrap_or(Bounds {
        origin: Default::default(),
        size: size(px(DEFAULT_SIZE.0), px(DEFAULT_SIZE.1)),
    });
    let view = builder
        .with_bounds(rect(bounds, window.scale_factor()))
        .build_as_child(&X11Window(parent))
        .map_err(|error| format!("Kodem could not create the browser: {error}"))?;
    let page = view.webview();
    keys(&page, false);
    let manager = page
        .user_content_manager()
        .ok_or("Kodem could not connect to the browser.")?;
    // The handler lives with the page, so it holds the page weakly.
    let weak = page.downgrade();
    manager.connect_script_message_received(Some("kodem"), move |_, result| {
        let Some(body) = result.js_value().map(|value| value.to_string()) else {
            return;
        };
        // Focus without a click is ignored: GTK reports it whenever the pointer enters the
        // page.
        if let (Some(Signal::Clicked), Some(page)) = (inbox.receive(&body), weak.upgrade()) {
            keys(&page, true);
            inbox.focused();
        }
    });
    manager.register_script_message_handler("kodem");
    Ok((view, Host { placed: None }))
}

/// Opens a local file. WebKitGTK lets a file page read the files beside it.
pub fn load_file(view: &WebView, path: &Path, _access: &Path) -> Result<(), String> {
    view.load_url(&super::file_url(path))
        .map_err(|error| error.to_string())
}

/// Gives the keyboard to GPUI's window, away from the page.
pub fn release_focus(view: &WebView, _host: &Host) {
    keys(&view.webview(), false);
}

/// Tells whether the page takes keys.
pub fn has_focus(view: &WebView, _host: &Host) -> bool {
    page_window(&view.webview())
        .is_some_and(|window| window.events().contains(gdk::EventMask::KEY_PRESS_MASK))
}

/// Captures the visible part of a page at one pixel per pixel of GTK and hands the PNG, its
/// width and its height to `done`, or the reason that it failed.
pub fn screenshot(
    view: &WebView,
    _host: &Host,
    done: impl FnOnce(Result<(Vec<u8>, u32, u32), String>) + 'static,
) {
    let page = view.webview();
    let scale = f64::from(page.scale_factor());
    page.snapshot(
        SnapshotRegion::Visible,
        SnapshotOptions::NONE,
        None::<&gio::Cancellable>,
        move |result| {
            done(
                result
                    .map_err(|error| format!(": {error}"))
                    .and_then(|surface| rgba(surface, scale)),
            );
        },
    );
}

// Encodes a capture of WebKitGTK, whose pixels are premultiplied BGRA in memory.
fn rgba(surface: gtk::cairo::Surface, scale: f64) -> Result<(Vec<u8>, u32, u32), String> {
    let mut image = gtk::cairo::ImageSurface::try_from(surface)
        .map_err(|_| ": the capture is not an image".to_owned())?;
    image.flush();
    let width = usize::try_from(image.width()).unwrap_or_default();
    let height = usize::try_from(image.height()).unwrap_or_default();
    let stride = usize::try_from(image.stride()).unwrap_or_default();
    let data = image.data().map_err(|error| format!(": {error}"))?;
    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in data.chunks(stride).take(height) {
        for pixel in row[..width * 4].chunks_exact(4) {
            let alpha = pixel[3];
            let channel = |value: u8| match alpha {
                0 => 0,
                255 => value,
                _ => u8::try_from(u16::from(value) * 255 / u16::from(alpha)).unwrap_or(255),
            };
            pixels.extend([
                channel(pixel[2]),
                channel(pixel[1]),
                channel(pixel[0]),
                alpha,
            ]);
        }
    }
    encode(
        pixels,
        u32::try_from(width).unwrap_or_default(),
        u32::try_from(height).unwrap_or_default(),
        scale,
    )
}

// Lets the X11 window of a page take keys, or passes its keys on to GPUI's window.
fn keys(page: &webkit2gtk::WebView, on: bool) {
    let Some(window) = page_window(page) else {
        return;
    };
    let keys = gdk::EventMask::KEY_PRESS_MASK | gdk::EventMask::KEY_RELEASE_MASK;
    let events = window.events();
    window.set_events(if on {
        events | keys
    } else {
        events.difference(keys)
    });
    if on {
        page.grab_focus();
    }
}

// Returns the X11 window that holds a page.
fn page_window(page: &webkit2gtk::WebView) -> Option<gdk::Window> {
    page.toplevel().and_then(|toplevel| toplevel.window())
}

// Returns GPUI's X11 window.
fn x11_window(window: &Window) -> Result<c_ulong, String> {
    // GPUI's own `window_handle` names the window in GPUI; this one is the platform's.
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("Kodem could not reach its window: {error}"))?;
    match handle.as_raw() {
        RawWindowHandle::Xcb(handle) => Ok(c_ulong::from(handle.window.get())),
        RawWindowHandle::Xlib(handle) => Ok(handle.window),
        _ => Err(
            "Kodem's browser needs an X11 window. Start Kodem in an X11 session or with XWayland."
                .into(),
        ),
    }
}

// Returns the rectangle of `bounds` of the window in device pixels.
fn rect(bounds: Bounds<Pixels>, scale: f32) -> Rect {
    let device = |value: Pixels| (f32::from(value) * scale).round();
    Rect {
        position: PhysicalPosition::new(device(bounds.origin.x), device(bounds.origin.y)).into(),
        size: PhysicalSize::new(
            device(bounds.size.width).max(1.),
            device(bounds.size.height).max(1.),
        )
        .into(),
    }
}
