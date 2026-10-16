//! Keeps the computer awake while an agent works.
//!
//! The host's main loop asks every few seconds whether an agent is starting or running a turn
//! and whether the setting allows the lock. While both hold, the host keeps one lock that stops
//! the system from sleeping when the user is idle. The display can still turn off, and closing
//! a laptop's lid or choosing Sleep still puts the computer to sleep. The lock ends with the
//! host process, also when the host stops unexpectedly.
//!
//! - macOS: an IOKit power assertion of the type `PreventUserIdleSystemSleep`, which is what
//!   `caffeinate -i` takes.
//! - Windows: `SetThreadExecutionState` with `ES_SYSTEM_REQUIRED`. The state belongs to the
//!   calling thread, so the lock is taken and released on the host's main loop thread.
//! - Linux: a `systemd-inhibit` process that blocks idle sleep while it runs. It watches the
//!   host and exits when the host is gone. Without systemd nothing is locked.
use std::time::{Duration, Instant};

/// Time between two checks of whether an agent works.
const INTERVAL: Duration = Duration::from_secs(2);
/// The reason that the system shows for the lock. Windows shows none.
#[cfg(not(windows))]
const REASON: &str = "A Kodem agent is working";

/// Holds the lock while an agent works and the setting allows it. It starts with no lock.
#[derive(Default)]
pub struct Awake {
    lock: Option<Lock>,
    checked: Option<Instant>,
}

impl Awake {
    /// Reports whether the next check is due, so the caller reads the store only then.
    pub fn due(&self) -> bool {
        self.checked
            .is_none_or(|checked| checked.elapsed() >= INTERVAL)
    }

    /// Takes the lock when `wanted` is true and releases it when it is false. A lock that the
    /// system refuses is tried again at the next check.
    pub fn set(&mut self, wanted: bool) {
        self.checked = Some(Instant::now());
        if !wanted {
            self.lock = None;
        } else if self.lock.is_none() {
            self.lock = Lock::take();
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
use linux::Lock;
#[cfg(target_os = "macos")]
use mac::Lock;
#[cfg(windows)]
use windows::Lock;

#[cfg(target_os = "macos")]
mod mac {
    use std::{
        ffi::{CString, c_char, c_void},
        ptr,
    };

    const UTF8: u32 = 0x0800_0100;
    const LEVEL_ON: u32 = 255;
    const SUCCESS: i32 = 0;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            allocator: *const c_void,
            text: *const c_char,
            encoding: u32,
        ) -> *const c_void;
        fn CFRelease(object: *const c_void);
    }

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPMAssertionCreateWithName(
            kind: *const c_void,
            level: u32,
            name: *const c_void,
            id: *mut u32,
        ) -> i32;
        fn IOPMAssertionRelease(id: u32) -> i32;
    }

    /// An IOKit power assertion that stops idle system sleep.
    pub struct Lock(u32);

    impl Lock {
        /// Takes the assertion, or returns nothing when IOKit refuses it.
        pub fn take() -> Option<Self> {
            let kind = string("PreventUserIdleSystemSleep")?;
            let Some(name) = string(super::REASON) else {
                unsafe { CFRelease(kind) };
                return None;
            };
            let mut id = 0;
            let result = unsafe { IOPMAssertionCreateWithName(kind, LEVEL_ON, name, &mut id) };
            unsafe {
                CFRelease(kind);
                CFRelease(name);
            }
            (result == SUCCESS).then_some(Self(id))
        }
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            unsafe { IOPMAssertionRelease(self.0) };
        }
    }

    // Returns a new CoreFoundation string that the caller releases.
    fn string(text: &str) -> Option<*const c_void> {
        let text = CString::new(text).ok()?;
        let string = unsafe { CFStringCreateWithCString(ptr::null(), text.as_ptr(), UTF8) };
        (!string.is_null()).then_some(string)
    }
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::System::Power::{
        ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
    };

    /// The system-required state of the calling thread.
    pub struct Lock;

    impl Lock {
        /// Marks the system as required, or returns nothing when Windows refuses it.
        pub fn take() -> Option<Self> {
            let previous = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
            (previous != 0).then_some(Self)
        }
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use std::process::{Child, Command, Stdio};

    /// A `systemd-inhibit` process that holds an idle and sleep inhibitor.
    pub struct Lock(Child);

    impl Lock {
        /// Starts the inhibitor, or returns nothing when systemd is not available.
        pub fn take() -> Option<Self> {
            // The inner shell waits while the host lives, so an inhibitor of a host that
            // stopped unexpectedly ends by itself. Without systemd the start fails.
            Command::new("systemd-inhibit")
                .args([
                    "--what=idle:sleep",
                    "--who=Kodem",
                    &format!("--why={}", super::REASON),
                    "--mode=block",
                    "sh",
                    "-c",
                    "while kill -0 \"$0\" 2>/dev/null; do sleep 5; done",
                ])
                .arg(std::process::id().to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
                .map(Self)
        }
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
