//! OS file drops (Finder → window).
//!
//! Slint `DropArea` only receives in-app `DragArea` payloads on the winit
//! backend; winit's `DroppedFile` / `HoveredFile` are ignored unless we hook
//! them via `WinitWindowAccessor::on_winit_window_event`.

use crate::pending_open;
use crate::ui::MainWindow;
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::ComponentHandle;
use winit::event::WindowEvent;

pub fn install(main: &MainWindow) {
    let weak = main.as_weak();
    main.window().on_winit_window_event(move |_window, event| {
        match event {
            WindowEvent::HoveredFile(_) => {
                if let Some(ui) = weak.upgrade() {
                    ui.set_drop_hover(true);
                }
            }
            WindowEvent::HoveredFileCancelled => {
                if let Some(ui) = weak.upgrade() {
                    ui.set_drop_hover(false);
                }
            }
            WindowEvent::DroppedFile(path) => {
                pending_open::queue([path.clone()]);
                if let Some(ui) = weak.upgrade() {
                    ui.set_drop_hover(false);
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });
}
