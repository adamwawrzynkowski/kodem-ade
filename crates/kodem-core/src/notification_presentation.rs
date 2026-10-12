//! Native presentation policy shared by the desktop and notification sender.
use block2::DynBlock;
use objc2::{
    ClassType, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
};
use objc2_foundation::{NSBundle, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNMutableNotificationContent, UNNotification, UNNotificationContent,
    UNNotificationPresentationOptions, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

/// `userInfo` key of the sender's choice to interrupt while Kodem is the active application.
const FOREGROUND_KEY: &str = "kodemPresentInForeground";

define_class!(
    #[unsafe(super(NSObject))]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}
    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        // macOS asks only while Kodem is the active application. It calls this on the main
        // dispatch queue of either Kodem process, so both processes must serve that queue.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn present(
            &self,
            _center: &UNUserNotificationCenter,
            notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((if interrupts(&notification.request().content()) {
                UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List
                    | UNNotificationPresentationOptions::Sound
            } else {
                UNNotificationPresentationOptions::List
            },));
        }
    }
);

// Reads the sender's choice from a notification. An unmarked notification does not interrupt.
fn interrupts(content: &UNNotificationContent) -> bool {
    let key = NSString::from_str(FOREGROUND_KEY);
    let key: &AnyObject = &key;
    content
        .userInfo()
        .objectForKey(key)
        .and_then(|value| value.downcast::<NSNumber>().ok())
        .is_some_and(|value| value.as_bool())
}

/// Keeps the process-local foreground notification delegate alive.
pub struct NotificationPresentation {
    // Notification Center retains its delegate weakly.
    _delegate: Retained<NotificationDelegate>,
}

impl NotificationPresentation {
    /// Installs the foreground presentation policy for this process without requesting
    /// permission. Keep the returned guard alive for the process's notification-handling
    /// lifetime.
    pub fn install() -> Option<Self> {
        NSBundle::mainBundle().bundleIdentifier()?;
        let delegate: Retained<NotificationDelegate> =
            unsafe { msg_send![NotificationDelegate::class(), new] };
        UNUserNotificationCenter::currentNotificationCenter()
            .setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        Some(Self {
            _delegate: delegate,
        })
    }

    /// Records in the notification whether it shows a banner and plays its sound while Kodem is
    /// the active application. Without this mark, such a notification goes to Notification
    /// Center only.
    pub fn set_foreground(content: &UNMutableNotificationContent, interrupt: bool) {
        let info = NSDictionary::from_slices(
            &[&*NSString::from_str(FOREGROUND_KEY)],
            &[&*NSNumber::new_bool(interrupt)],
        );
        // A string key with a number value is a valid property list, as `userInfo` requires.
        unsafe { content.setUserInfo(info.cast_unchecked::<AnyObject, AnyObject>()) };
    }
}
