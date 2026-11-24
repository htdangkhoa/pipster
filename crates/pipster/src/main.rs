mod file_drop;
mod macos_open;
mod menu;
mod pending_open;
mod player_host;
mod prefs;
mod updater;

use player_host::{apply_cli_paths, AppController};
use prefs::AppPrefs;
use slint::ComponentHandle;
use std::path::PathBuf;
use std::rc::Rc;

slint::include_modules!();

pub mod ui {
    pub use crate::{MainWindow, PlayerWindow};
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = false;
    let mut paths: Vec<PathBuf> = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg == "--metrics" {
            metrics = true;
        } else if !arg.starts_with('-') {
            paths.push(PathBuf::from(arg));
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let prefs = AppPrefs::load_or_default();
    let app = AppController::new(prefs, metrics);

    let main = MainWindow::new()?;
    // Winit's NSApplicationDelegate exists after the first window; install
    // Open With handlers before the run loop delivers the odoc Apple Event.
    macos_open::install();
    file_drop::install(&main);
    app.bind_main(&main);
    menu::install(&app);
    install_pending_open_drain(&app);
    main.show()?;

    apply_cli_paths(&app, &paths);
    if std::env::var_os("PIPSTER_SPAWN_EMPTY").is_some() {
        app.spawn_player(None);
    }
    schedule_startup_update_check(&app);

    slint::run_event_loop()?;
    Ok(())
}

fn schedule_startup_update_check(app: &Rc<AppController>) {
    if std::env::var_os("PIPSTER_SKIP_UPDATE").is_some() {
        return;
    }
    let app = Rc::clone(app);
    let timer = Box::leak(Box::new(slint::Timer::default()));
    timer.start(
        slint::TimerMode::SingleShot,
        std::time::Duration::from_secs(3),
        move || {
            app.check_for_updates(false);
        },
    );
}

fn install_pending_open_drain(app: &Rc<AppController>) {
    let app = Rc::clone(app);
    let timer = Box::leak(Box::new(slint::Timer::default()));
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            let paths = pending_open::take();
            if paths.is_empty() {
                return;
            }
            let videos: Vec<_> = paths
                .into_iter()
                .filter(|p| player_host::is_probably_video(p))
                .collect();
            if videos.is_empty() {
                if let Some(main) = app.upgrade_main_public() {
                    main.set_status_text("Drop video files (mp4, mov, mkv, …).".into());
                }
                return;
            }
            for path in &videos {
                app.spawn_player(Some(path));
            }
        },
    );
}

trait PrefsLoad {
    fn load_or_default() -> Self;
}

impl PrefsLoad for AppPrefs {
    fn load_or_default() -> Self {
        prefs::load()
    }
}
