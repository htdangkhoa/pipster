use crate::menu;
use crate::prefs::{self, AppPrefs};
use crate::ui::MainWindow;
use crate::updater::{self, CheckResult};
use pipster_governor::CapDecision;
use pipster_player::{PlayerCorePool, PlayerId};
use slint::{ComponentHandle, Weak};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "flv", "mpeg", "mpg", "ts", "m2ts", "ogv",
];

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "tif", "tiff", "webp", "bmp",
];

fn media_extensions() -> Vec<&'static str> {
    let mut all = Vec::with_capacity(VIDEO_EXTENSIONS.len() + IMAGE_EXTENSIONS.len());
    all.extend_from_slice(VIDEO_EXTENSIONS);
    all.extend_from_slice(IMAGE_EXTENSIONS);
    all
}

fn extension_is(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| list.iter().any(|ok| e.eq_ignore_ascii_case(ok)))
        .unwrap_or(false)
}

pub struct AppController {
    pool: PlayerCorePool,
    prefs: RefCell<AppPrefs>,
    players: RefCell<HashSet<PlayerId>>,
    #[cfg(target_os = "macos")]
    image_windows: RefCell<Vec<crate::image_viewer::ImageWindow>>,
    main: RefCell<Option<Weak<MainWindow>>>,
    metrics: bool,
}

impl AppController {
    pub fn new(prefs: AppPrefs, metrics: bool) -> Rc<Self> {
        let mut prefs = prefs;
        // Always run uncapped unless an advanced user edited prefs manually.
        prefs.governor.max_active = pipster_governor::UNLIMITED_ACTIVE;
        prefs.governor.pause_oldest_when_over_cap = false;

        let pool = PlayerCorePool::new(
            pipster_governor::ResourceGovernor::new(prefs.governor.clone()),
            pipster_governor::MpvLowRamProfile::default(),
        );
        Rc::new(Self {
            pool,
            prefs: RefCell::new(prefs),
            players: RefCell::new(HashSet::new()),
            #[cfg(target_os = "macos")]
            image_windows: RefCell::new(Vec::new()),
            main: RefCell::new(None),
            metrics,
        })
    }

    pub fn bind_main(self: &Rc<Self>, main: &MainWindow) {
        *self.main.borrow_mut() = Some(main.as_weak());
        self.sync_main_ui(main);

        let this = Rc::clone(self);
        main.on_open_files(move || {
            this.open_files_dialog();
        });

        let this = Rc::clone(self);
        main.on_apply_prefs(move |mute, aot| {
            {
                let mut prefs = this.prefs.borrow_mut();
                prefs.governor.mute_on_spawn = mute;
                prefs.governor.always_on_top_default = aot;
                prefs.governor.max_active = pipster_governor::UNLIMITED_ACTIVE;
                prefs.governor.pause_oldest_when_over_cap = false;
                this.pool.set_governor_config(prefs.governor.clone());
                prefs::save(&prefs);
            }
            if let Some(main) = this.upgrade_main() {
                main.set_status_text("Preferences saved.".into());
                this.sync_main_ui(&main);
            }
        });

        let this = Rc::clone(self);
        main.on_pause_all(move || {
            this.pause_all_public();
        });

        let this = Rc::clone(self);
        main.on_new_empty_player(move || {
            this.spawn_player(None);
        });

        self.start_ui_timer();
    }

    fn upgrade_main(&self) -> Option<MainWindow> {
        self.main.borrow().as_ref()?.upgrade()
    }

    pub fn upgrade_main_public(&self) -> Option<MainWindow> {
        self.upgrade_main()
    }

    fn sync_main_ui(&self, main: &MainWindow) {
        let prefs = self.prefs.borrow();
        main.set_mute_on_spawn(prefs.governor.mute_on_spawn);
        main.set_always_on_top_default(prefs.governor.always_on_top_default);
        drop(prefs);
        self.refresh_main_counters_on(main);
    }

    pub fn recent_paths(&self) -> Vec<PathBuf> {
        self.prefs.borrow().recent.clone()
    }

    pub fn clear_recent_public(self: &Rc<Self>) {
        self.prefs.borrow_mut().recent.clear();
        prefs::save(&self.prefs.borrow());
        menu::refresh_recent(self);
        if let Some(main) = self.upgrade_main() {
            main.set_status_text("Recent cleared.".into());
        }
    }

    fn refresh_main_counters(&self) {
        if let Some(main) = self.upgrade_main() {
            self.refresh_main_counters_on(&main);
        }
    }

    fn refresh_main_counters_on(&self, main: &MainWindow) {
        main.set_open_windows(self.pool.active_count() as i32);
        main.set_playing_windows(self.pool.playing_count() as i32);
    }

    pub fn pause_all_public(self: &Rc<Self>) {
        self.pool.pause_all();
        self.refresh_main_counters();
        if let Some(main) = self.upgrade_main() {
            main.set_status_text("Paused all players.".into());
        }
    }

    /// Check GitHub Releases for a newer build.
    /// `interactive`: show status + dialogs even when already up to date.
    pub fn check_for_updates(self: &Rc<Self>, interactive: bool) {
        let weak = self.main.borrow().clone();
        if interactive {
            if let Some(main) = weak.as_ref().and_then(|w| w.upgrade()) {
                main.set_status_text("Checking for updates…".into());
            }
        }
        std::thread::spawn(move || {
            let result = updater::check_for_update();
            let _ = slint::invoke_from_event_loop(move || {
                handle_update_check_result(weak, result, interactive);
            });
        });
    }

    pub fn open_files_dialog(self: &Rc<Self>) {
        let exts = media_extensions();
        let files = rfd::FileDialog::new()
            .add_filter("Media", &exts)
            .add_filter("Video", VIDEO_EXTENSIONS)
            .add_filter("Images", IMAGE_EXTENSIONS)
            .pick_files();
        if let Some(files) = files {
            for path in files {
                self.open_media(&path);
            }
        }
    }

    /// Open a video or image path with size-tiered image handling.
    pub fn open_media(self: &Rc<Self>, path: &Path) {
        if is_probably_image(path) {
            self.open_image(path);
        } else {
            self.spawn_player(Some(path));
        }
    }

    fn open_image(self: &Rc<Self>, path: &Path) {
        let info = match pipster_image::probe(path) {
            Ok(info) => info,
            Err(err) => {
                if let Some(main) = self.upgrade_main() {
                    main.set_status_text(format!("Image probe failed: {err}").into());
                }
                return;
            }
        };

        if pipster_image::needs_shrink(&info) {
            #[cfg(target_os = "macos")]
            {
                self.open_tiled_image(path);
            }
            #[cfg(not(target_os = "macos"))]
            {
                match pipster_image::ensure_fit_preview(path, pipster_image::PREVIEW_MAX_PIXEL_SIZE)
                {
                    Ok(preview) => {
                        self.spawn_player_ex(Some(&preview.path), Some(path), true, true);
                    }
                    Err(err) => {
                        if let Some(main) = self.upgrade_main() {
                            main.set_status_text(format!("Image preview failed: {err}").into());
                        }
                    }
                }
            }
        } else {
            self.spawn_player_ex(Some(path), Some(path), true, false);
        }
    }

    #[cfg(target_os = "macos")]
    fn open_tiled_image(self: &Rc<Self>, path: &Path) {
        if let Some(main) = self.upgrade_main() {
            main.set_status_text("Preparing image tiles…".into());
        }
        let always_on_top = self.prefs.borrow().governor.always_on_top_default;
        crate::image_jobs::start_open(path.to_path_buf(), always_on_top);
    }

    #[cfg(target_os = "macos")]
    pub fn drain_image_jobs(self: &Rc<Self>) {
        use crate::image_jobs::ImageJobEvent;
        for event in crate::image_jobs::take() {
            match event {
                ImageJobEvent::OverviewReady {
                    source,
                    root,
                    meta,
                    always_on_top,
                } => match crate::image_viewer::open_tiled_window(
                    source.clone(),
                    root,
                    meta,
                    always_on_top,
                ) {
                    Ok(win) => {
                        self.prefs.borrow_mut().push_recent(source.clone());
                        prefs::save(&self.prefs.borrow());
                        menu::refresh_recent(self);
                        let name = source
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| source.display().to_string());
                        if let Some(main) = self.upgrade_main() {
                            main.set_status_text(
                                format!(
                                    "Opened: {name} — scroll zoom · drag pan · 0=fit · resize keeps ratio"
                                )
                                .into(),
                            );
                        }
                        self.image_windows.borrow_mut().push(win);
                    }
                    Err(err) => {
                        if let Some(main) = self.upgrade_main() {
                            main.set_status_text(format!("Image window failed: {err}").into());
                        }
                    }
                },
                ImageJobEvent::PyramidComplete { root, meta } => {
                    for w in self.image_windows.borrow().iter() {
                        if w.pyramid_root == root {
                            w.refresh_meta(meta.clone());
                        }
                    }
                    if let Some(main) = self.upgrade_main() {
                        main.set_status_text("Image tiles ready (full zoom).".into());
                    }
                }
                ImageJobEvent::Failed { message } => {
                    tracing::warn!(%message, "image job failed");
                    if let Some(main) = self.upgrade_main() {
                        main.set_status_text(message.into());
                    }
                }
            }
        }
    }

    pub fn spawn_player(self: &Rc<Self>, path: Option<&Path>) {
        self.spawn_player_ex(path, path, false, false);
    }

    /// `load_path` is what mpv opens; `recent_path` is stored in Recents (original).
    fn spawn_player_ex(
        self: &Rc<Self>,
        load_path: Option<&Path>,
        recent_path: Option<&Path>,
        still: bool,
        preview: bool,
    ) {
        let (id, decision) = match self.pool.acquire() {
            Ok(v) => v,
            Err(err) => {
                if let Some(main) = self.upgrade_main() {
                    main.set_status_text(format!("Failed to create player: {err}").into());
                }
                return;
            }
        };

        if matches!(decision, CapDecision::Reject) {
            let _ = self.pool.release(id);
            if let Some(main) = self.upgrade_main() {
                main.set_status_text("Could not open another player.".into());
            }
            return;
        }

        {
            let prefs = self.prefs.borrow();
            let _ = self.pool.with_core_mut(id, |c| {
                c.set_always_on_top(prefs.governor.always_on_top_default);
                let _ = c.set_muted(prefs.governor.mute_on_spawn);
            });
        }

        if let Some(path) = load_path {
            let load_result = self.pool.with_core_mut(id, |c| {
                if still {
                    c.load_still(path)
                } else {
                    c.load(path)
                }
            });
            match load_result {
                Some(Ok(())) => {
                    let recent = recent_path.unwrap_or(path);
                    self.prefs.borrow_mut().push_recent(recent.to_path_buf());
                    prefs::save(&self.prefs.borrow());
                    menu::refresh_recent(self);
                }
                Some(Err(err)) => {
                    if let Some(main) = self.upgrade_main() {
                        main.set_status_text(format!("Load failed: {err}").into());
                    }
                }
                None => {}
            }
        }

        self.players.borrow_mut().insert(id);

        if let Some(main) = self.upgrade_main() {
            let name = recent_path
                .or(load_path)
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "Idle player".into());
            let status = if preview {
                format!("Opened (preview): {name}")
            } else {
                format!("Opened: {name}")
            };
            main.set_status_text(status.into());
            self.sync_main_ui(&main);
        }

        if self.metrics {
            tracing::info!(
                %id,
                still,
                preview,
                open = self.pool.active_count(),
                playing = self.pool.playing_count(),
                "spawn_player"
            );
        }
    }

    fn start_ui_timer(self: &Rc<Self>) {
        use std::sync::Once;
        static START: Once = Once::new();
        let this = Rc::clone(self);
        START.call_once(move || {
            let timer = Box::leak(Box::new(slint::Timer::default()));
            timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(500),
                move || {
                    for id in this.pool.reap_dead() {
                        this.players.borrow_mut().remove(&id);
                    }
                    #[cfg(target_os = "macos")]
                    {
                        this.drain_image_jobs();
                        this.image_windows
                            .borrow_mut()
                            .retain(|w| w.is_visible());
                    }
                    this.refresh_main_counters();
                },
            );
        });
    }
}

fn handle_update_check_result(
    weak: Option<Weak<MainWindow>>,
    result: Result<CheckResult, String>,
    interactive: bool,
) {
    match result {
        Ok(CheckResult::UpToDate { current }) => {
            if interactive {
                if let Some(main) = weak.as_ref().and_then(|w| w.upgrade()) {
                    main.set_status_text(format!("Pipster {current} is up to date.").into());
                }
            }
        }
        Ok(CheckResult::Available(update)) => {
            if let Some(main) = weak.as_ref().and_then(|w| w.upgrade()) {
                main.set_status_text(format!("Update available: v{}", update.version).into());
            }
            let notes = update.notes.trim();
            let detail = if notes.is_empty() {
                format!(
                    "Pipster {} is available (you have {}).\n\nDownload and install now?",
                    update.version,
                    updater::current_version()
                )
            } else {
                let short: String = notes.chars().take(400).collect();
                format!(
                    "Pipster {} is available (you have {}).\n\n{}\n\nDownload and install now?",
                    update.version,
                    updater::current_version(),
                    short
                )
            };
            let yes = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Info)
                .set_title("Update Available")
                .set_description(&detail)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if yes == rfd::MessageDialogResult::Yes {
                install_update(weak, update);
            } else {
                updater::open_release_page(&update.tag);
            }
        }
        Err(err) => {
            tracing::warn!(%err, "update check failed");
            if interactive {
                if let Some(main) = weak.as_ref().and_then(|w| w.upgrade()) {
                    main.set_status_text(format!("Update check failed: {err}").into());
                }
            }
        }
    }
}

fn install_update(weak: Option<Weak<MainWindow>>, update: updater::AvailableUpdate) {
    if let Some(main) = weak.as_ref().and_then(|w| w.upgrade()) {
        main.set_status_text(format!("Downloading v{}…", update.version).into());
    }
    std::thread::spawn(move || {
        let outcome = updater::download_update(&update).and_then(|staged| {
            if updater::current_app_bundle().is_some() {
                updater::apply_and_relaunch(&staged)
            } else {
                let _ = std::process::Command::new("open")
                    .arg(staged.parent().unwrap_or(staged.as_path()))
                    .status();
                Err("Downloaded update — not running from Pipster.app; opened the folder.".into())
            }
        });
        let _ = slint::invoke_from_event_loop(move || match outcome {
            Ok(()) => {}
            Err(msg) => {
                if let Some(main) = weak.as_ref().and_then(|w| w.upgrade()) {
                    main.set_status_text(msg.into());
                }
            }
        });
    });
}

pub fn is_probably_video(path: &Path) -> bool {
    extension_is(path, VIDEO_EXTENSIONS)
}

pub fn is_probably_image(path: &Path) -> bool {
    extension_is(path, IMAGE_EXTENSIONS)
}

pub fn is_probably_media(path: &Path) -> bool {
    is_probably_video(path) || is_probably_image(path)
}

pub fn apply_cli_paths(app: &Rc<AppController>, paths: &[PathBuf]) {
    for path in paths {
        if path.is_file() {
            if is_probably_image(path) || is_probably_video(path) {
                app.open_media(path);
            } else {
                // Unknown extension: still try mpv (CLI passthrough).
                app.spawn_player(Some(path));
            }
        }
    }
}
