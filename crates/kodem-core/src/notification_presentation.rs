//! Native presentation policy shared by the desktop and notification sender.
use block2::DynBlock;
use objc2::{ClassType, define_class, msg_send, rc::Retained, runtime::ProtocolObject};
use objc2_foundation::{NSBundle, NSObject, NSObjectProtocol};
use objc2_user_notifications::{
    UNNotification, UNNotificationPresentationOptions, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

define_class!(
    #[unsafe(super(NSObject))]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}
    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }
    }
);

/// Keeps the process-local foreground notification delegate alive.
pub struct NotificationPresentation {
    // Notification Center retains its delegate weakly.
    _delegate: Retained<NotificationDelegate>,
}

impl NotificationPresentation {
    /// Enables foreground banners and sound for this process without requesting permission.
    /// Keep the returned guard alive for the process's notification-handling lifetime.
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
}
