//! "Settings…" in the macOS app menu, with Cmd+, (docs/specs/editor.md §24).
//!
//! winit gives the app the standard app menu (About, Services, Hide, Quit).
//! [`install`] adds Settings… after About, through AppKit. A click, or
//! Cmd+, which AppKit sends to the menu first, goes to the app as
//! `app.settings` through a channel its subscription reads ([`requests`]).
//! The `unsafe` calls are AppKit's; the project owner approved them with
//! phase 6c's plan.

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use iced::futures::Stream;
use iced::futures::channel::mpsc;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSString};

type Channel = (
    mpsc::UnboundedSender<()>,
    Mutex<Option<mpsc::UnboundedReceiver<()>>>,
);

/// The menu's clicks: the sender for the menu's target, the receiver for
/// the subscription, made once, whichever asks first.
fn channel() -> &'static Channel {
    static CHANNEL: OnceLock<Channel> = OnceLock::new();
    CHANNEL.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded();
        (tx, Mutex::new(Some(rx)))
    })
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class
    // adds no instance variables and no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ColdOpenSettingsTarget"]
    struct SettingsTarget;

    impl SettingsTarget {
        // SAFETY: an action method's signature: one object (the sender),
        // nothing returned.
        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            let _ = channel().0.unbounded_send(());
        }
    }
);

impl SettingsTarget {
    #[allow(unsafe_code)]
    fn new(mtm: MainThreadMarker) -> Retained<SettingsTarget> {
        // SAFETY: `init` is NSObject's designated initializer, and the
        // class adds nothing to initialize.
        unsafe { msg_send![SettingsTarget::alloc(mtm), init] }
    }
}

/// Adds Settings… to the app menu, once. Call it on the main thread once
/// the window is up, when winit has made its menu; elsewhere it does
/// nothing.
#[allow(unsafe_code)]
pub fn install() {
    static DONE: AtomicBool = AtomicBool::new(false);
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(app_menu) = app
        .mainMenu()
        .and_then(|m| m.itemAtIndex(0))
        .and_then(|i| i.submenu())
    else {
        return;
    };
    if DONE.swap(true, Ordering::Relaxed) {
        return;
    }
    let target = SettingsTarget::new(mtm);
    // SAFETY: the action names the method SettingsTarget defines above,
    // with an action's signature; the strings are valid NSStrings.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str("Settings…"),
            Some(sel!(openSettings:)),
            &NSString::from_str(","),
        )
    };
    // SAFETY: a menu item holds its target weakly. The target is never
    // released (it's leaked below), so it outlives the item.
    unsafe { item.setTarget(Some(&target)) };
    // After About and its separator, then a separator of its own, as Mac
    // apps place it.
    let n = app_menu.numberOfItems();
    let at = n.min(2);
    app_menu.insertItem_atIndex(&item, at);
    app_menu.insertItem_atIndex(&NSMenuItem::separatorItem(mtm), at + 1);
    std::mem::forget(target);
}

/// The menu's clicks, for the app's subscription. The first caller gets
/// them; a later one gets a stream that never yields.
pub fn requests() -> Pin<Box<dyn Stream<Item = ()> + Send>> {
    let rx = channel().1.lock().ok().and_then(|mut r| r.take());
    match rx {
        Some(rx) => Box::pin(rx),
        None => Box::pin(iced::futures::stream::pending()),
    }
}
