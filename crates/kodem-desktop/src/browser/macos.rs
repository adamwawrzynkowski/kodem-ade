//! The macOS side of the browser: a WebKit view in a host view of its own, which is a subview of
//! GPUI's view that the pane places over itself, so the page draws above GPUI and only while the
//! pane shows it. A docked Web Inspector shares the host with the page: WebKit splits and restores
//! the page's superview, which must therefore be the pane and not the whole window.
use super::engine::Inbox;
use block2::RcBlock;
use gpui::{Bounds, Pixels, Window};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, ProtocolObject},
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSView,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{
    NSBundle, NSDictionary, NSError, NSNumber, NSObjectProtocol, NSProcessInfo, NSString, NSURL,
    ns_string,
};
use objc2_web_kit::{
    WKScriptMessage, WKScriptMessageHandler, WKSnapshotConfiguration, WKUserContentController,
};
use std::{cell::Cell, path::Path, ptr::NonNull};
use wry::{
    Rect, WebView, WebViewBuilder, WebViewExtMacOS,
    dpi::{LogicalPosition, LogicalSize},
    raw_window_handle::{
        AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
    },
};

/// The size of a page that the pane has not shown yet, in points.
const DEFAULT_SIZE: (f64, f64) = (1024., 768.);

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the handler does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KodemBrowserReplies"]
    #[ivars = Inbox]
    /// Receives the messages of the page scripts.
    struct Replies;

    unsafe impl NSObjectProtocol for Replies {}

    unsafe impl WKScriptMessageHandler for Replies {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
            // SAFETY: WebKit passes a valid message on the main thread.
            let body = unsafe { message.body() };
            let Ok(body) = body.downcast::<NSString>() else {
                return;
            };
            // A click, and focus that the page took in any other way, such as a click in a
            // frame or the window's return to the front, give the page the keyboard.
            if self.ivars().receive(&body.to_string()).is_some() {
                self.ivars().focused();
            }
        }
    }
);

impl Replies {
    // Creates the handler of one page.
    fn new(inbox: Inbox, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(inbox);
        // SAFETY: NSObject's init is the designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// What the pages of the window share, which is nothing on macOS: WebKit keeps one data store.
pub struct Context;

impl Context {
    /// Creates the shared state of the pages.
    pub fn new() -> Self {
        Self
    }

    /// Returns why this computer cannot show pages, which it always can.
    pub fn unavailable(&self) -> Option<&'static str> {
        None
    }
}

/// The view that holds a page and its docked Web Inspector, placed over the pane.
pub struct Host {
    host: Retained<NSView>,
    /// GPUI's view, which holds the host and takes the keyboard back from the page.
    window_view: Retained<NSView>,
}

impl Host {
    /// Places the host over `bounds` of the window. Returns whether it moved or changed size.
    pub fn place(&self, _view: &WebView, bounds: Bounds<Pixels>, _scale: f32) -> bool {
        // Compared with the host's own frame, so that a frame that changed outside the pane
        // returns to the pane.
        let frame = frame(&self.window_view, bounds);
        let moved = self.host.frame() != frame;
        if moved {
            self.host.setFrame(frame);
        }
        moved
    }

    /// Shows the page.
    pub fn show(&self, _view: &WebView) {
        self.host.setHidden(false);
    }

    /// Hides the page.
    pub fn hide(&self, _view: &WebView) {
        self.host.setHidden(true);
    }

    /// Returns the width of the pane, in points.
    pub fn width(&self) -> f64 {
        self.host.frame().size.width
    }
}

impl Drop for Host {
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

/// Returns the builder of a page with what this platform sets.
pub fn builder(_context: &mut Context) -> WebViewBuilder<'_> {
    WebViewBuilder::new().with_user_agent(user_agent())
}

/// Creates a page from `builder` as a hidden subview of the window's view. Its messages go to
/// `inbox`.
pub fn create(
    builder: WebViewBuilder<'_>,
    window: &Window,
    bounds: Option<Bounds<Pixels>>,
    inbox: Inbox,
) -> Result<(WebView, Host), String> {
    let mtm = MainThreadMarker::new().ok_or("The browser runs off the main thread.")?;
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
    let host = Host { host, window_view };
    let view = builder
        .with_bounds(Rect {
            position: LogicalPosition::new(0., 0.).into(),
            size: LogicalSize::new(frame.size.width, frame.size.height).into(),
        })
        .build_as_child(&HostHandle(&host.host))
        .map_err(|error| format!("Kodem could not create the browser: {error}"))?;
    // The page fills its host and follows its size, except while WebKit docks the inspector.
    view.webview().setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    let replies = Replies::new(inbox, mtm);
    // SAFETY: The controller keeps the handler until the page is gone; both are on the main
    // thread.
    unsafe {
        view.manager()
            .addScriptMessageHandler_name(ProtocolObject::from_ref(&*replies), ns_string!("kodem"));
    }
    Ok((view, host))
}

/// Opens a local file, whose page may read the files in `access`.
pub fn load_file(view: &WebView, path: &Path, access: &Path) -> Result<(), String> {
    let file = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let folder = NSURL::fileURLWithPath(&NSString::from_str(&access.to_string_lossy()));
    // SAFETY: Both are file URLs, and the view lives on the main thread.
    unsafe {
        view.webview()
            .loadFileURL_allowingReadAccessToURL(&file, &folder);
    }
    Ok(())
}

/// Gives the keyboard to GPUI's view, away from the page and its Web Inspector.
pub fn release_focus(_view: &WebView, host: &Host) {
    if let Some(window) = host.host.window() {
        window.makeFirstResponder(Some(&host.window_view));
    }
}

/// Tells whether the page or its Web Inspector has the keyboard.
pub fn has_focus(_view: &WebView, host: &Host) -> bool {
    host.host
        .window()
        .and_then(|window| window.firstResponder())
        .and_then(|responder| responder.downcast::<NSView>().ok())
        .is_some_and(|view| view.isDescendantOf(&host.host))
}

/// Captures the visible part of a page at one pixel per point and hands the PNG, its width and
/// its height to `done`, or the reason that it failed.
pub fn screenshot(
    view: &WebView,
    _host: &Host,
    done: impl FnOnce(Result<(Vec<u8>, u32, u32), String>) + 'static,
) {
    let Some(mtm) = MainThreadMarker::new() else {
        done(Err(": the browser runs off the main thread".into()));
        return;
    };
    let view = view.webview();
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
    // WebKit calls the handler once.
    let done = Cell::new(Some(done));
    let handler = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
        // SAFETY: WebKit passes an image or an error, each valid for the call.
        let (image, error) = unsafe { (image.as_ref(), error.as_ref()) };
        let capture = image.and_then(png).ok_or_else(|| {
            error
                .map(|error| format!(": {}", error.localizedDescription()))
                .unwrap_or_default()
        });
        if let Some(done) = done.take() {
            done(capture);
        }
    });
    // SAFETY: The handler lives until WebKit calls it, which it does on the main thread.
    unsafe {
        view.takeSnapshotWithConfiguration_completionHandler(Some(&configuration), &handler);
    }
}

// Encodes an image as PNG, with its size in pixels.
fn png(image: &NSImage) -> Option<(Vec<u8>, u32, u32)> {
    let tiff = image.TIFFRepresentation()?;
    let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
    // SAFETY: Empty properties are valid for PNG.
    let data = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some((
        data.to_vec(),
        u32::try_from(bitmap.pixelsWide()).ok()?,
        u32::try_from(bitmap.pixelsHigh()).ok()?,
    ))
}

// Returns Safari's user agent. WebKit's own lacks Safari's version, so sites take the page for
// an unknown browser and serve it their legacy pages.
fn user_agent() -> String {
    // Safari's version follows the WebKit of the system, which updates with Safari.
    let installed = NSBundle::bundleWithPath(ns_string!("/Applications/Safari.app"))
        .and_then(|bundle| {
            bundle.objectForInfoDictionaryKey(ns_string!("CFBundleShortVersionString"))
        })
        .and_then(|version| version.downcast::<NSString>().ok())
        .map(|version| version.to_string());
    let version = installed.unwrap_or_else(|| {
        // Safari took the number of macOS with macOS 26; before, it ran three ahead.
        let major = NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion;
        format!("{}.0", if major >= 26 { major } else { major + 3 })
    });
    format!(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Safari/605.1.15"
    )
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
