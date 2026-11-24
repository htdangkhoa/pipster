//! GitHub Releases auto-update for the packaged `.app`.
//!
//! Checks `htdangkhoa/pipster` for a newer tag, downloads `Pipster-macos*.zip`,
//! stages the new bundle, then swaps it in after Pipster quits.

use semver::Version;
use serde::Deserialize;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const OWNER: &str = "htdangkhoa";
const REPO: &str = "pipster";
const USER_AGENT: &str = concat!("Pipster/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct AvailableUpdate {
    pub version: String,
    pub tag: String,
    pub notes: String,
    pub download_url: String,
    pub asset_name: String,
}

#[derive(Debug)]
pub enum CheckResult {
    UpToDate { current: String },
    Available(AvailableUpdate),
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Resolve the `.app` bundle that contains this process, if any.
pub fn current_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    for ancestor in exe.ancestors() {
        if ancestor
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

pub fn check_for_update() -> Result<CheckResult, String> {
    let url = format!("https://api.github.com/repos/{OWNER}/{REPO}/releases/latest");
    let body = http_get_text(&url)?;
    let release: GhRelease =
        serde_json::from_str(&body).map_err(|e| format!("parse release JSON: {e}"))?;

    if release.draft || release.prerelease {
        return Ok(CheckResult::UpToDate {
            current: current_version().into(),
        });
    }

    let tag = release.tag_name.trim().trim_start_matches('v');
    let remote = Version::parse(tag).map_err(|e| format!("invalid remote version `{tag}`: {e}"))?;
    let local =
        Version::parse(current_version()).map_err(|e| format!("invalid local version: {e}"))?;

    if remote <= local {
        return Ok(CheckResult::UpToDate {
            current: current_version().into(),
        });
    }

    let asset = pick_macos_asset(&release.assets)
        .ok_or_else(|| "latest release has no macOS zip asset".to_string())?;

    Ok(CheckResult::Available(AvailableUpdate {
        version: remote.to_string(),
        tag: release.tag_name,
        notes: release.body.unwrap_or_default(),
        download_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
    }))
}

fn pick_macos_asset(assets: &[GhAsset]) -> Option<&GhAsset> {
    let arch = std::env::consts::ARCH;
    let preferred = [
        format!("Pipster-macos-{arch}.zip"),
        "Pipster-macos.zip".into(),
        "Pipster.app.zip".into(),
    ];
    for name in &preferred {
        if let Some(a) = assets.iter().find(|a| a.name == *name) {
            return Some(a);
        }
    }
    assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        n.ends_with(".zip") && (n.contains("macos") || n.contains("pipster") || n.contains(".app"))
    })
}

/// Download the update zip, extract `Pipster.app`, and return its path.
pub fn download_update(update: &AvailableUpdate) -> Result<PathBuf, String> {
    let cache = update_cache_dir()?;
    let _ = fs::remove_dir_all(&cache);
    fs::create_dir_all(&cache).map_err(|e| format!("create cache: {e}"))?;

    let zip_path = cache.join(&update.asset_name);
    http_download(&update.download_url, &zip_path)?;

    let extract_dir = cache.join("extract");
    fs::create_dir_all(&extract_dir).map_err(|e| format!("create extract dir: {e}"))?;

    let status = Command::new("ditto")
        .args(["-x", "-k"])
        .arg(&zip_path)
        .arg(&extract_dir)
        .status()
        .map_err(|e| format!("ditto extract failed: {e}"))?;
    if !status.success() {
        return Err(format!("ditto extract exited with {status}"));
    }

    find_app_bundle(&extract_dir)
        .ok_or_else(|| "downloaded zip does not contain Pipster.app".to_string())
}

/// Schedule replacement of the running `.app` after this process exits, then quit.
pub fn apply_and_relaunch(staged_app: &Path) -> Result<(), String> {
    let target = current_app_bundle()
        .ok_or_else(|| "not running from a .app bundle — open the zip manually".to_string())?;

    if !staged_app.exists() {
        return Err("staged app missing".into());
    }

    let pid = std::process::id();
    let script = update_cache_dir()?.join("apply_update.sh");
    let staged = staged_app.display().to_string();
    let dest = target.display().to_string();
    let contents = format!(
        r#"#!/bin/bash
set -euo pipefail
TARGET="{dest}"
STAGED="{staged}"
PID={pid}
while kill -0 "$PID" 2>/dev/null; do sleep 0.2; done
sleep 0.5
rm -rf "$TARGET"
ditto "$STAGED" "$TARGET"
open "$TARGET"
"#
    );
    fs::write(&script, contents).map_err(|e| format!("write apply script: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script)
            .map_err(|e| format!("stat script: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).map_err(|e| format!("chmod script: {e}"))?;
    }

    Command::new(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn apply script: {e}"))?;

    // Give the helper a moment to start watching our PID, then exit.
    std::thread::sleep(Duration::from_millis(150));
    std::process::exit(0);
}

pub fn open_release_page(tag: &str) {
    let url = format!("https://github.com/{OWNER}/{REPO}/releases/tag/{tag}");
    let _ = Command::new("open").arg(url).status();
}

fn update_cache_dir() -> Result<PathBuf, String> {
    let base = directories::ProjectDirs::from("app", "Pipster", "Pipster")
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp/pipster-update"));
    Ok(base.join("update"))
}

fn find_app_bundle(root: &Path) -> Option<PathBuf> {
    if root
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("app"))
    {
        return Some(root.to_path_buf());
    }
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("app"))
        {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_app_bundle(&path) {
                return Some(found);
            }
        }
    }
    None
}

fn http_get_text(url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build();
    let resp = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHub API request failed: {e}"))?;
    if !(200..300).contains(&resp.status()) {
        return Err(format!("GitHub API HTTP {}", resp.status()));
    }
    resp.into_string()
        .map_err(|e| format!("read GitHub response: {e}"))
}

fn http_download(url: &str, dest: &Path) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(300))
        .build();
    let resp = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/octet-stream")
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    if !(200..300).contains(&resp.status()) {
        return Err(format!("download HTTP {}", resp.status()));
    }
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(dest).map_err(|e| format!("create download file: {e}"))?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("read download: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("write download: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_prefers_arch_zip() {
        let assets = vec![
            GhAsset {
                name: "Pipster-macos.zip".into(),
                browser_download_url: "https://example/generic".into(),
            },
            GhAsset {
                name: format!("Pipster-macos-{}.zip", std::env::consts::ARCH),
                browser_download_url: "https://example/arch".into(),
            },
        ];
        let picked = pick_macos_asset(&assets).unwrap();
        assert!(picked.name.contains(std::env::consts::ARCH));
    }
}
