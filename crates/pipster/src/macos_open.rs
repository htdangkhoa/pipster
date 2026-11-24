//! Finder "Open With" delivers files via Apple Events to NSApplicationDelegate,
//! not argv. Winit's delegate does not implement open handlers, which produces:
//! "Pipster cannot open files in the Movie format."
//!
//! After Slint/winit creates `WinitApplicationDelegate`, we subclass it (no new
//! ivars), swap the live delegate's class, and queue paths for a Slint timer.

use crate::pending_open;
use std::path::PathBuf;

fn queue_paths(paths: impl IntoIterator<Item = PathBuf>) {
    pending_open::queue(paths);
}

#[cfg(not(target_os = "macos"))]
pub fn install() {}

#[cfg(target_os = "macos")]
pub fn install() {
    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, ProtocolObject, Sel};
    use objc2::{sel, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSApplicationDelegateReply};
    use objc2_foundation::{NSArray, NSString, NSURL};
    use std::sync::Once;

    unsafe extern "C-unwind" fn application_open_urls(
        _this: &AnyObject,
        _cmd: Sel,
        app: &NSApplication,
        urls: &NSArray<NSURL>,
    ) {
        let mut paths = Vec::with_capacity(urls.count());
        for url in urls {
            if let Some(path) = url.path() {
                paths.push(PathBuf::from(path.to_string()));
            }
        }
        queue_paths(paths);
        app.replyToOpenOrPrint(NSApplicationDelegateReply::Success);
    }

    unsafe extern "C-unwind" fn application_open_files(
        _this: &AnyObject,
        _cmd: Sel,
        app: &NSApplication,
        filenames: &NSArray<NSString>,
    ) {
        let mut paths = Vec::with_capacity(filenames.count());
        for name in filenames {
            paths.push(PathBuf::from(name.to_string()));
        }
        queue_paths(paths);
        app.replyToOpenOrPrint(NSApplicationDelegateReply::Success);
    }

    unsafe extern "C-unwind" fn application_open_file(
        _this: &AnyObject,
        _cmd: Sel,
        _app: &NSApplication,
        filename: &NSString,
    ) -> Bool {
        queue_paths([PathBuf::from(filename.to_string())]);
        Bool::YES
    }

    fn isa_swap(cls: &'static AnyClass) {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::error!("macos open handler must run on the main thread");
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let Some(delegate) = app.delegate() else {
            tracing::error!("NSApplication has no delegate yet");
            return;
        };

        // SAFETY: subclass of WinitApplicationDelegate with no extra ivars.
        // winit's `is_kind_of::<WinitApplicationDelegate>()` still succeeds.
        unsafe {
            let obj: &AnyObject = ProtocolObject::as_ref(&delegate);
            let _ = AnyObject::set_class(obj, cls);
        }
        // Extra retain for process lifetime (NSApp also retains the delegate).
        std::mem::forget(delegate);
    }

    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Some(winit_cls) = AnyClass::get(c"WinitApplicationDelegate") else {
            tracing::error!("WinitApplicationDelegate missing; Finder Open With will not work");
            return;
        };

        if let Some(existing) = AnyClass::get(c"PipsterApplicationDelegate") {
            isa_swap(existing);
            return;
        }

        let mut builder = ClassBuilder::new(c"PipsterApplicationDelegate", winit_cls)
            .expect("allocate PipsterApplicationDelegate");

        // SAFETY: signatures match NSApplicationDelegate document-open methods.
        unsafe {
            builder.add_method(
                sel!(application:openURLs:),
                application_open_urls as unsafe extern "C-unwind" fn(_, _, _, _),
            );
            builder.add_method(
                sel!(application:openFiles:),
                application_open_files as unsafe extern "C-unwind" fn(_, _, _, _),
            );
            builder.add_method(
                sel!(application:openFile:),
                application_open_file as unsafe extern "C-unwind" fn(_, _, _, _) -> _,
            );
        }

        isa_swap(builder.register());
        tracing::info!("installed Finder Open With handlers on app delegate");
    });
}
