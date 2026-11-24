use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use crate::player_host::AppController;

#[derive(Debug, Clone)]
enum MenuAction {
    Open,
    NewPlayer,
    PauseAll,
    CheckUpdates,
    OpenRecent(PathBuf),
    ClearRecent,
}

/// Action map is shared with the muda event thread (`Send`). Submenu/Menu stay
/// main-thread only via `MENU_UI`.
type ActionMap = Arc<Mutex<HashMap<MenuId, MenuAction>>>;

struct MenuUi {
    /// Must outlive the process: dropping `Menu` calls `setMainMenu(None)`.
    menu: &'static Menu,
    recent_submenu: Submenu,
    actions: ActionMap,
}

thread_local! {
    static MENU_UI: std::cell::RefCell<Option<MenuUi>> = const { std::cell::RefCell::new(None) };
}

pub fn install(app: &Rc<AppController>) {
    let menu = Menu::new();
    let app_menu = Submenu::new("Pipster", true);
    let file_menu = Submenu::new("File", true);
    let recent_submenu = Submenu::new("Open Recent", true);

    let open = MenuItem::new(
        "Open…",
        true,
        Some(Accelerator::new(Modifiers::META, Code::KeyO)),
    );
    let new_player = MenuItem::new(
        "New Player Window",
        true,
        Some(Accelerator::new(Modifiers::META, Code::KeyN)),
    );
    let pause_all = MenuItem::new("Pause All", true, None);
    let check_updates = MenuItem::new("Check for Updates…", true, None);

    let actions: ActionMap = Arc::new(Mutex::new(HashMap::new()));
    {
        let mut map = actions.lock();
        map.insert(open.id().clone(), MenuAction::Open);
        map.insert(new_player.id().clone(), MenuAction::NewPlayer);
        map.insert(pause_all.id().clone(), MenuAction::PauseAll);
        map.insert(check_updates.id().clone(), MenuAction::CheckUpdates);
    }

    let _ = app_menu.append_items(&[
        &PredefinedMenuItem::about(
            Some("Pipster"),
            Some(AboutMetadata {
                name: Some("Pipster".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                copyright: Some("MIT".into()),
                ..Default::default()
            }),
        ),
        &PredefinedMenuItem::separator(),
        &check_updates,
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ]);

    let _ = file_menu.append_items(&[
        &open,
        &recent_submenu,
        &PredefinedMenuItem::separator(),
        &new_player,
        &PredefinedMenuItem::separator(),
        &pause_all,
    ]);

    let _ = menu.append_items(&[&app_menu, &file_menu]);

    // Leak so Drop does not clear NSApp.mainMenu.
    let menu: &'static Menu = Box::leak(Box::new(menu));
    #[cfg(target_os = "macos")]
    {
        menu.init_for_nsapp();
    }

    MENU_UI.with(|slot| {
        *slot.borrow_mut() = Some(MenuUi {
            menu,
            recent_submenu,
            actions: Arc::clone(&actions),
        });
    });
    refresh_recent(app);

    // Slint's winit backend may install its own default muda bar when the
    // window becomes active — re-assert ours for a short window after launch.
    #[cfg(target_os = "macos")]
    {
        let reapply = Box::leak(Box::new(slint::Timer::default()));
        let ticks = Box::leak(Box::new(0u32));
        reapply.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(100),
            || {
                MENU_UI.with(|slot| {
                    if let Some(ui) = slot.borrow().as_ref() {
                        ui.menu.init_for_nsapp();
                    }
                });
                *ticks += 1;
                if *ticks >= 30 {
                    reapply.stop();
                }
            },
        );
    }

    let (tx, rx): (Sender<MenuAction>, Receiver<MenuAction>) = mpsc::channel();
    let map = Arc::clone(&actions);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(action) = map.lock().get(event.id()).cloned() {
            let _ = tx.send(action);
        }
    }));

    let app = Rc::clone(app);
    let timer = Box::leak(Box::new(slint::Timer::default()));
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            while let Ok(action) = rx.try_recv() {
                match action {
                    MenuAction::Open => app.open_files_dialog(),
                    MenuAction::NewPlayer => app.spawn_player(None),
                    MenuAction::PauseAll => app.pause_all_public(),
                    MenuAction::CheckUpdates => app.check_for_updates(true),
                    MenuAction::OpenRecent(path) => app.spawn_player(Some(&path)),
                    MenuAction::ClearRecent => app.clear_recent_public(),
                }
            }
        },
    );
}

pub fn refresh_recent(app: &AppController) {
    MENU_UI.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(ui) = slot.as_mut() else {
            return;
        };

        while ui.recent_submenu.remove_at(0).is_some() {}

        let mut actions = ui.actions.lock();
        actions.retain(|_, action| {
            !matches!(action, MenuAction::OpenRecent(_) | MenuAction::ClearRecent)
        });

        let recent = app.recent_paths();
        let has_recent = !recent.is_empty();
        if !has_recent {
            let empty = MenuItem::new("No Recent Files", false, None);
            let _ = ui.recent_submenu.append(&empty);
        } else {
            for path in recent {
                let label = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                let item = MenuItem::new(label, true, None);
                actions.insert(item.id().clone(), MenuAction::OpenRecent(path));
                let _ = ui.recent_submenu.append(&item);
            }
        }

        let _ = ui.recent_submenu.append(&PredefinedMenuItem::separator());
        let clear = MenuItem::new("Clear Menu", has_recent, None);
        actions.insert(clear.id().clone(), MenuAction::ClearRecent);
        let _ = ui.recent_submenu.append(&clear);
    });
}
