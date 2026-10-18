//! The Windows side of the browser: a WebView2 child window of GPUI's window, placed over the
//! pane. GPUI's composition lies behind child windows (see vendor/gpui_ce_windows), so the page
//! draws above GPUI. The pages keep their data in the browser folder of Kodem's data directory,
//! because WebView2's default folder beside the program cannot be written once it is installed.
use super::engine::{Inbox, encode};
use gpui::{Bounds, Pixels, Window, px, size};
use std::{cell::Cell, path::Path, rc::Rc};
use webview2_com::{
    CapturePreviewCompletedHandler, FocusChangedEventHandler,
    Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
    WebMessageReceivedEventHandler, take_pwstr,
};
use windows::{
    Win32::{
        System::Com::{IStream, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET},
        UI::Shell::SHCreateMemStream,
    },
    core::PWSTR,
};
use wry::{
    Rect, WebContext, WebView, WebViewBuilder, WebViewExtWindows,
    dpi::{PhysicalPosition, PhysicalSize},
};

/// The size of a page that the pane has not shown yet, in pixels of the window.
const DEFAULT_SIZE: (f32, f32) = (1024., 768.);

/// What the pages of the window share: their data folder.
pub struct Context {
    web: WebContext,
}

impl Context {
    /// Creates the shared state of the pages.
    pub fn new() -> Self {
        let folder = kodem_core::ipc::data_dir()
            .ok()
            .map(|root| root.join("Browser"));
        Self {
            web: WebContext::new(folder),
        }
    }

    /// Returns why this computer cannot show pages. A missing WebView2 Runtime shows when the
    /// first page fails to open.
    pub fn unavailable(&self) -> Option<&'static str> {
        None
    }
}

/// Where a page lies in the window, and whether it has the keyboard.
pub struct Host {
    /// The bounds of the page in the window and the device pixels per pixel of the window, as
    /// the pane placed it last.
    placed: Option<(Bounds<Pixels>, f32)>,
    /// Set by WebView2's focus events, which also report focus that moves into a frame.
    focused: Rc<Cell<bool>>,
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

/// Returns the builder of a page with what this platform sets.
pub fn builder(context: &mut Context) -> WebViewBuilder<'_> {
    // A child window shows at once, so the page waits hidden until the pane places it.
    WebViewBuilder::new_with_web_context(&mut context.web).with_visible(false)
}

/// Creates a page from `builder` as a hidden child window of GPUI's window. Its messages go to
/// `inbox`.
pub fn create(
    builder: WebViewBuilder<'_>,
    window: &Window,
    bounds: Option<Bounds<Pixels>>,
    inbox: Inbox,
) -> Result<(WebView, Host), String> {
    let bounds = bounds.unwrap_or(Bounds {
        origin: Default::default(),
        size: size(px(DEFAULT_SIZE.0), px(DEFAULT_SIZE.1)),
    });
    let view = builder
        .with_bounds(rect(bounds, window.scale_factor()))
        .build_as_child(window)
        .map_err(|error| {
            format!(
                "Kodem could not create the browser: {error}. The browser needs the Microsoft Edge WebView2 Runtime."
            )
        })?;
    let focused = Rc::new(Cell::new(false));
    listen(&view, inbox, focused.clone())
        .map_err(|error| format!("Kodem could not connect to the browser: {error}"))?;
    Ok((
        view,
        Host {
            placed: None,
            focused,
        },
    ))
}

// Hands the messages of a page's scripts to `inbox`, and follows the keyboard into and out of
// the page.
fn listen(view: &WebView, inbox: Inbox, focused: Rc<Cell<bool>>) -> windows::core::Result<()> {
    let webview = view.webview();
    let controller = view.controller();
    let messages = inbox.clone();
    let lost = focused.clone();
    let mut token = 0;
    // SAFETY: The handlers are COM objects that WebView2 calls on this thread while the page
    // lives.
    unsafe {
        webview.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                let mut text = PWSTR::null();
                if let Some(args) = args
                    && args.TryGetWebMessageAsString(&mut text).is_ok()
                {
                    // Clicks show in the focus events below, which frames report too.
                    let _ = messages.receive(&take_pwstr(text));
                }
                Ok(())
            })),
            &mut token,
        )?;
        controller.add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |controller, _| {
                focused.set(true);
                // Popups of the page, such as the options of a select, open where the window
                // is now.
                if let Some(controller) = controller {
                    let _ = controller.NotifyParentWindowPositionChanged();
                }
                inbox.focused();
                Ok(())
            })),
            &mut token,
        )?;
        controller.add_LostFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                lost.set(false);
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}

/// Opens a local file. WebView2 lets a file page read the files beside it.
pub fn load_file(view: &WebView, path: &Path, _access: &Path) -> Result<(), String> {
    view.load_url(&super::file_url(path))
        .map_err(|error| error.to_string())
}

/// Gives the keyboard to GPUI's window, away from the page.
pub fn release_focus(view: &WebView, _host: &Host) {
    let _ = view.focus_parent();
}

/// Tells whether the page has the keyboard.
pub fn has_focus(_view: &WebView, host: &Host) -> bool {
    host.focused.get()
}

/// Captures the visible part of a page at one pixel per pixel of the window and hands the PNG,
/// its width and its height to `done`, or the reason that it failed.
pub fn screenshot(
    view: &WebView,
    host: &Host,
    done: impl FnOnce(Result<(Vec<u8>, u32, u32), String>) + 'static,
) {
    let scale = host.placed.map_or(1., |(_, scale)| f64::from(scale));
    // WebView2 calls the handler once, unless the capture fails to start.
    let done = Rc::new(Cell::new(Some(done)));
    let finish = {
        let done = done.clone();
        move |capture| {
            if let Some(done) = done.take() {
                done(capture);
            }
        }
    };
    // SAFETY: A stream without initial data needs no arguments.
    let Some(stream) = (unsafe { SHCreateMemStream(None) }) else {
        finish(Err(": Windows had no memory for the capture".into()));
        return;
    };
    let captured = stream.clone();
    let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
        let capture = result
            .map_err(|error| format!(": {}", error.message()))
            .and_then(|()| read(&captured))
            .and_then(|png| {
                let image = image::load_from_memory(&png)
                    .map_err(|error| format!(": {error}"))?
                    .into_rgba8();
                let (width, height) = image.dimensions();
                encode(image.into_raw(), width, height, scale)
            });
        if let Some(done) = done.take() {
            done(capture);
        }
        Ok(())
    }));
    // SAFETY: The stream and the handler are COM objects that live until WebView2 is done.
    let started = unsafe {
        view.webview().CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &handler,
        )
    };
    if let Err(error) = started {
        finish(Err(format!(": {}", error.message())));
    }
}

// Reads the whole of a memory stream.
fn read(stream: &IStream) -> Result<Vec<u8>, String> {
    let failed = |error: windows::core::Error| format!(": {}", error.message());
    let mut stat = STATSTG::default();
    // SAFETY: The stream is valid, and the buffer has the size that the stream reports.
    unsafe {
        stream.Stat(&mut stat, STATFLAG_NONAME).map_err(failed)?;
        stream.Seek(0, STREAM_SEEK_SET, None).map_err(failed)?;
        let mut data = vec![0; usize::try_from(stat.cbSize).unwrap_or_default()];
        let mut read = 0;
        stream
            .Read(
                data.as_mut_ptr().cast(),
                u32::try_from(data.len()).unwrap_or(u32::MAX),
                Some(&mut read),
            )
            .ok()
            .map_err(failed)?;
        data.truncate(usize::try_from(read).unwrap_or_default());
        Ok(data)
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
