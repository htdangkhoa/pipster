//! Floating AppKit image viewer.
//!
//! No NSScrollView. One content view fills the window and every redraw
//! recomputes an aspect-fit destination from current bounds:
//! - `user_zoom = 1.0` → fit window (letterboxed)
//! - resize → same formula with new bounds → ratio always preserved
//! - scroll / pinch / +/- zoom; drag to pan when zoomed in

#![cfg(target_os = "macos")]

use objc2::rc::{Allocated, Retained};
use objc2::runtime::Bool;
use objc2::{
    define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSCompositingOperation, NSEvent, NSEventModifierFlags,
    NSFloatingWindowLevel, NSImage, NSScreen, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use pipster_image::PyramidMeta;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

struct LevelCache {
    level: u32,
    image: Retained<NSImage>,
}

struct ViewIvars {
    root: RefCell<PathBuf>,
    meta: RefCell<Option<PyramidMeta>>,
    title_base: RefCell<String>,
    source_w: Cell<u32>,
    source_h: Cell<u32>,
    /// 1.0 = fit window.
    user_zoom: Cell<f64>,
    pan_x: Cell<f64>,
    pan_y: Cell<f64>,
    drag_last: Cell<Option<(f64, f64)>>,
    cache: RefCell<Option<LevelCache>>,
}

impl Default for ViewIvars {
    fn default() -> Self {
        Self {
            root: RefCell::new(PathBuf::new()),
            meta: RefCell::new(None),
            title_base: RefCell::new(String::new()),
            source_w: Cell::new(0),
            source_h: Cell::new(0),
            user_zoom: Cell::new(1.0),
            pan_x: Cell::new(0.0),
            pan_y: Cell::new(0.0),
            drag_last: Cell::new(None),
            cache: RefCell::new(None),
        }
    }
}

const KEY_EQUAL: u16 = 24;
const KEY_MINUS: u16 = 27;
const KEY_0: u16 = 29;
const KEY_KEYPAD_PLUS: u16 = 69;
const KEY_KEYPAD_MINUS: u16 = 78;
const KEY_KEYPAD_0: u16 = 82;

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "PipsterImageContentView"]
    #[ivars = ViewIvars]
    struct PipsterImageContentView;

    impl PipsterImageContentView {
        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(this: Allocated<Self>, frame: NSRect) -> Option<Retained<Self>> {
            let this = this.set_ivars(ViewIvars::default());
            let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
            Some(this)
        }

        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            true
        }

        #[unsafe(method(preservesContentDuringLiveResize))]
        fn preserves_content_during_live_resize(&self) -> bool {
            false
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, new_size: NSSize) {
            let _: () = unsafe { msg_send![super(self), setFrameSize: new_size] };
            // Bounds changed → next draw recomputes aspect-fit from new size.
            self.clamp_pan();
            self.setNeedsDisplay(true);
            self.update_title();
        }

        #[unsafe(method(viewDidEndLiveResize))]
        fn view_did_end_live_resize(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidEndLiveResize] };
            self.clamp_pan();
            self.setNeedsDisplay(true);
            self.update_title();
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let bounds = self.bounds();
            NSColor::blackColor().setFill();
            objc2_app_kit::NSBezierPath::fillRect(bounds);

            let Some((image, dest)) = self.image_and_dest() else {
                return;
            };

            unsafe {
                image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                    dest,
                    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
                    NSCompositingOperation::Copy,
                    1.0,
                    true,
                    None,
                );
            }
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let precise = event.hasPreciseScrollingDeltas();
            let flags = event.modifierFlags();
            let want_zoom = !precise
                || flags.contains(NSEventModifierFlags::Command)
                || flags.contains(NSEventModifierFlags::Control)
                || flags.contains(NSEventModifierFlags::Option);

            if want_zoom {
                let dy = if precise {
                    event.scrollingDeltaY()
                } else {
                    event.deltaY() * 10.0
                };
                if dy.abs() > 0.1 {
                    let steps = (dy.abs() / 3.0).clamp(1.0, 8.0);
                    let mut factor = 1.12_f64.powf(steps);
                    if dy < 0.0 {
                        factor = 1.0 / factor;
                    }
                    let anchor = self.convertPoint_fromView(event.locationInWindow(), None);
                    self.bump_zoom(factor, Some(anchor));
                    return;
                }
            }

            // Two-finger pan when zoomed in.
            if self.ivars().user_zoom.get() > 1.0 + f64::EPSILON {
                let dx = if precise {
                    event.scrollingDeltaX()
                } else {
                    event.deltaX() * 10.0
                };
                let dy = if precise {
                    event.scrollingDeltaY()
                } else {
                    event.deltaY() * 10.0
                };
                self.ivars().pan_x.set(self.ivars().pan_x.get() + dx);
                self.ivars().pan_y.set(self.ivars().pan_y.get() + dy);
                self.clamp_pan();
                self.setNeedsDisplay(true);
                return;
            }

            let _: () = unsafe { msg_send![super(self), scrollWheel: event] };
        }

        #[unsafe(method(magnifyWithEvent:))]
        fn magnify_with_event(&self, event: &NSEvent) {
            let m = event.magnification();
            if m.abs() > f64::EPSILON {
                let anchor = self.convertPoint_fromView(event.locationInWindow(), None);
                self.bump_zoom(1.0 + m, Some(anchor));
            }
        }

        #[unsafe(method(smartMagnifyWithEvent:))]
        fn smart_magnify_with_event(&self, _event: &NSEvent) {
            let z = self.ivars().user_zoom.get();
            if z < 1.5 {
                self.ivars().user_zoom.set(2.0);
            } else {
                self.ivars().user_zoom.set(1.0);
                self.ivars().pan_x.set(0.0);
                self.ivars().pan_y.set(0.0);
            }
            self.clamp_pan();
            self.setNeedsDisplay(true);
            self.update_title();
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let _ = self.window().map(|w| w.makeFirstResponder(Some(self)));
            let p = self.convertPoint_fromView(event.locationInWindow(), None);
            self.ivars().drag_last.set(Some((p.x, p.y)));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let p = self.convertPoint_fromView(event.locationInWindow(), None);
            if let Some((lx, ly)) = self.ivars().drag_last.get() {
                self.ivars().pan_x.set(self.ivars().pan_x.get() + (p.x - lx));
                self.ivars().pan_y.set(self.ivars().pan_y.get() + (p.y - ly));
                self.clamp_pan();
                self.setNeedsDisplay(true);
            }
            self.ivars().drag_last.set(Some((p.x, p.y)));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            self.ivars().drag_last.set(None);
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if self.handle_zoom_key(event) {
                return;
            }
            let _: () = unsafe { msg_send![super(self), keyDown: event] };
        }

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> Bool {
            if self.handle_zoom_key(event) {
                return Bool::YES;
            }
            unsafe { msg_send![super(self), performKeyEquivalent: event] }
        }
    }
);

impl PipsterImageContentView {
    fn configure(&self, root: PathBuf, meta: PyramidMeta, title: String) {
        self.ivars().source_w.set(meta.source_width);
        self.ivars().source_h.set(meta.source_height);
        self.ivars().user_zoom.set(1.0);
        self.ivars().pan_x.set(0.0);
        self.ivars().pan_y.set(0.0);
        *self.ivars().root.borrow_mut() = root;
        *self.ivars().meta.borrow_mut() = Some(meta);
        *self.ivars().title_base.borrow_mut() = title;
        *self.ivars().cache.borrow_mut() = None;
        self.setNeedsDisplay(true);
        self.update_title();
    }

    fn reload_meta(&self, meta: PyramidMeta) {
        *self.ivars().meta.borrow_mut() = Some(meta);
        *self.ivars().cache.borrow_mut() = None;
        self.setNeedsDisplay(true);
    }

    fn fit_scale(&self) -> f64 {
        let bounds = self.bounds().size;
        let src_w = self.ivars().source_w.get().max(1) as f64;
        let src_h = self.ivars().source_h.get().max(1) as f64;
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return 1e-6;
        }
        (bounds.width / src_w)
            .min(bounds.height / src_h)
            .max(1e-6)
    }

    fn dest_rect(&self) -> NSRect {
        let bounds = self.bounds();
        let src_w = self.ivars().source_w.get().max(1) as f64;
        let src_h = self.ivars().source_h.get().max(1) as f64;
        let scale = self.fit_scale() * self.ivars().user_zoom.get();
        let dw = src_w * scale;
        let dh = src_h * scale;
        let x = (bounds.size.width - dw) * 0.5 + self.ivars().pan_x.get();
        let y = (bounds.size.height - dh) * 0.5 + self.ivars().pan_y.get();
        NSRect::new(NSPoint::new(x, y), NSSize::new(dw, dh))
    }

    fn clamp_pan(&self) {
        let bounds = self.bounds().size;
        let src_w = self.ivars().source_w.get().max(1) as f64;
        let src_h = self.ivars().source_h.get().max(1) as f64;
        let scale = self.fit_scale() * self.ivars().user_zoom.get();
        let dw = src_w * scale;
        let dh = src_h * scale;
        let max_x = ((dw - bounds.width) * 0.5).max(0.0);
        let max_y = ((dh - bounds.height) * 0.5).max(0.0);
        self.ivars()
            .pan_x
            .set(self.ivars().pan_x.get().clamp(-max_x, max_x));
        self.ivars()
            .pan_y
            .set(self.ivars().pan_y.get().clamp(-max_y, max_y));
    }

    fn image_and_dest(&self) -> Option<(Retained<NSImage>, NSRect)> {
        let dest = self.dest_rect();
        if dest.size.width < 1.0 || dest.size.height < 1.0 {
            return None;
        }
        let root = self.ivars().root.borrow().clone();
        let meta = self.ivars().meta.borrow().clone()?;
        if meta.ready_level < 0 {
            return None;
        }

        let scale_factor = self
            .window()
            .map(|w| w.backingScaleFactor())
            .unwrap_or(2.0);
        let need_edge = dest.size.width.max(dest.size.height) * scale_factor;
        let source_edge = self.ivars().source_w.get().max(self.ivars().source_h.get()) as f64;
        let need = (need_edge / source_edge.max(1.0)).clamp(0.0, 1.0);
        let level = meta.best_level_for_scale(need);
        let image = load_level(self, &root, level)?;
        Some((image, dest))
    }

    fn bump_zoom(&self, factor: f64, anchor: Option<NSPoint>) {
        let old_z = self.ivars().user_zoom.get();
        let new_z = (old_z * factor).clamp(0.05, 64.0);
        if (new_z - old_z).abs() < 1e-9 {
            return;
        }

        // Keep the image point under `anchor` stable across zoom.
        if let Some(a) = anchor {
            let before = self.dest_rect();
            if before.size.width > 1.0 && before.size.height > 1.0 {
                let ux = (a.x - before.origin.x) / before.size.width;
                let uy = (a.y - before.origin.y) / before.size.height;
                self.ivars().user_zoom.set(new_z);
                let after = self.dest_rect();
                let new_ax = after.origin.x + ux * after.size.width;
                let new_ay = after.origin.y + uy * after.size.height;
                self.ivars()
                    .pan_x
                    .set(self.ivars().pan_x.get() + (a.x - new_ax));
                self.ivars()
                    .pan_y
                    .set(self.ivars().pan_y.get() + (a.y - new_ay));
            } else {
                self.ivars().user_zoom.set(new_z);
            }
        } else {
            self.ivars().user_zoom.set(new_z);
        }

        if new_z <= 1.0 + f64::EPSILON {
            self.ivars().pan_x.set(0.0);
            self.ivars().pan_y.set(0.0);
        }
        self.clamp_pan();
        self.setNeedsDisplay(true);
        self.update_title();
    }

    fn handle_zoom_key(&self, event: &NSEvent) -> bool {
        let chars = event
            .charactersIgnoringModifiers()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let code = event.keyCode();
        let zoom_in =
            matches!(chars.as_str(), "+" | "=") || code == KEY_EQUAL || code == KEY_KEYPAD_PLUS;
        let zoom_out =
            matches!(chars.as_str(), "-" | "_") || code == KEY_MINUS || code == KEY_KEYPAD_MINUS;
        let zoom_fit = chars.as_str() == "0" || code == KEY_0 || code == KEY_KEYPAD_0;

        if zoom_in {
            self.bump_zoom(1.25, None);
            true
        } else if zoom_out {
            self.bump_zoom(1.0 / 1.25, None);
            true
        } else if zoom_fit {
            self.ivars().user_zoom.set(1.0);
            self.ivars().pan_x.set(0.0);
            self.ivars().pan_y.set(0.0);
            self.setNeedsDisplay(true);
            self.update_title();
            true
        } else {
            false
        }
    }

    fn update_title(&self) {
        let Some(window) = self.window() else {
            return;
        };
        let base = self.ivars().title_base.borrow().clone();
        let pct = (self.ivars().user_zoom.get() * 100.0).round() as i32;
        window.setTitle(&NSString::from_str(&format!(
            "{base}  —  {pct}%   (scroll zoom · drag pan · 0 fit)"
        )));
    }
}

fn load_level(
    view: &PipsterImageContentView,
    root: &PathBuf,
    level: u32,
) -> Option<Retained<NSImage>> {
    {
        let cache = view.ivars().cache.borrow();
        if let Some(c) = cache.as_ref() {
            if c.level == level {
                return Some(c.image.clone());
            }
        }
    }
    let path = PyramidMeta::level_jpeg(root, level);
    let path_str = path.to_str()?;
    let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path_str))?;
    *view.ivars().cache.borrow_mut() = Some(LevelCache {
        level,
        image: Retained::clone(&image),
    });
    Some(image)
}

/// Inset from the visible screen edges — close to Safari’s system PiP.
const PIP_SCREEN_MARGIN: f64 = 20.0;

fn place_like_safari_pip(window: &NSWindow, mtm: MainThreadMarker) {
    let visible = NSScreen::mainScreen(mtm)
        .map(|s| s.visibleFrame())
        .unwrap_or(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(1280.0, 800.0),
        ));
    let frame = window.frame();
    let top_left = NSPoint::new(
        visible.origin.x + visible.size.width - frame.size.width - PIP_SCREEN_MARGIN,
        visible.origin.y + visible.size.height - PIP_SCREEN_MARGIN,
    );
    window.setFrameTopLeftPoint(top_left);
}

pub struct ImageWindow {
    pub window: Retained<NSWindow>,
    #[allow(dead_code)]
    pub source: PathBuf,
    content: Retained<PipsterImageContentView>,
    pub pyramid_root: PathBuf,
}

pub fn open_tiled_window(
    source: PathBuf,
    pyramid_root: PathBuf,
    meta: PyramidMeta,
    always_on_top: bool,
) -> Result<ImageWindow, String> {
    let mtm = MainThreadMarker::new().ok_or("image viewer requires main thread")?;

    let src_w = meta.source_width.max(1) as f64;
    let src_h = meta.source_height.max(1) as f64;

    let max_side = 520.0_f64;
    let (win_w, win_h) = if src_w >= src_h {
        (max_side, max_side * src_h / src_w)
    } else {
        (max_side * src_w / src_h, max_side)
    };

    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;

    let content_w = win_w.max(200.0);
    let content_h = win_h.max(200.0);

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(content_w, content_h),
            ),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };

    // Safari-style PiP: top-right of the visible screen area (below menu bar,
    // above Dock) with a small inset from the edges.
    place_like_safari_pip(&window, mtm);

    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Image".into());
    if always_on_top {
        window.setLevel(NSFloatingWindowLevel);
    }
    unsafe { window.setReleasedWhenClosed(false) };

    let content_frame = window
        .contentView()
        .map(|v| v.frame())
        .unwrap_or(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(win_w, win_h),
        ));

    let content: Retained<PipsterImageContentView> = unsafe {
        msg_send![
            PipsterImageContentView::alloc(mtm),
            initWithFrame: content_frame
        ]
    };
    content.configure(pyramid_root.clone(), meta, name);

    window.setContentView(Some(&*content));
    window.makeKeyAndOrderFront(None);
    let _ = window.makeFirstResponder(Some(&*content));
    content.setNeedsDisplay(true);
    content.update_title();

    Ok(ImageWindow {
        window,
        source,
        content,
        pyramid_root,
    })
}

impl ImageWindow {
    pub fn refresh_meta(&self, meta: PyramidMeta) {
        self.content.reload_meta(meta);
    }

    pub fn is_visible(&self) -> bool {
        self.window.isVisible()
    }
}
