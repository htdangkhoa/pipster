use crate::ImageError;
use directories::ProjectDirs;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

fn project_dirs() -> Result<ProjectDirs, ImageError> {
    ProjectDirs::from("app", "Pipster", "Pipster").ok_or_else(|| {
        ImageError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no application support directory",
        ))
    })
}

fn preview_dir() -> Result<PathBuf, ImageError> {
    let dir = project_dirs()?.data_dir().join("image-previews");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn source_digest(source: &Path, extra: u64) -> Result<u64, ImageError> {
    let canonical = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let meta = fs::metadata(&canonical)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let size = meta.len();
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    mtime.hash(&mut hasher);
    size.hash(&mut hasher);
    extra.hash(&mut hasher);
    Ok(hasher.finish())
}

/// Pyramid tile root for a source file under Application Support.
pub fn pyramid_dir_for_source(source: &Path) -> Result<PathBuf, ImageError> {
    // Include format version so crop/layout fixes don't reuse bad tiles.
    let digest = source_digest(source, u64::from(crate::pyramid::PYRAMID_FORMAT))?;
    let dir = project_dirs()?
        .data_dir()
        .join("image-pyramids")
        .join(format!("v{}_{digest:016x}", crate::pyramid::PYRAMID_FORMAT));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Stable cache path under Application Support for `(source, mtime, size, max_pixel_size)`.
pub fn preview_cache_path(source: &Path, max_pixel_size: u32) -> Result<PathBuf, ImageError> {
    let digest = source_digest(source, max_pixel_size as u64)?;
    Ok(preview_dir()?.join(format!("{digest:016x}_{max_pixel_size}.jpg")))
}
