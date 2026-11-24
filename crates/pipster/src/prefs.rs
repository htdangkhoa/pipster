use directories::ProjectDirs;
use pipster_governor::GovernorConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppPrefs {
    pub governor: GovernorConfig,
    pub recent: Vec<PathBuf>,
    pub main_geometry: Option<WindowGeometry>,
    pub last_player_geometry: Option<WindowGeometry>,
}

fn prefs_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("app", "Pipster", "Pipster")?;
    Some(dirs.config_dir().join("prefs.json"))
}

pub fn load() -> AppPrefs {
    let Some(path) = prefs_path() else {
        return AppPrefs::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => AppPrefs::default(),
    }
}

pub fn save(prefs: &AppPrefs) {
    let Some(path) = prefs_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(prefs) {
        let _ = fs::write(path, text);
    }
}

/// Max entries kept in Open Recent (File menu).
pub const RECENT_LIMIT: usize = 20;

impl AppPrefs {
    pub fn push_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(RECENT_LIMIT);
    }
}
