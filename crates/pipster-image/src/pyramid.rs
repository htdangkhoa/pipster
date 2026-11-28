//! Multi-resolution JPEG pyramid for pan/zoom (one file per level).
//!
//! Each level is a single ImageIO thumbnail JPEG (`z{N}/level.jpg`). The viewer
//! draws the visible region with `fromRect` — no CGImage tile cropping (which
//! fought CG's bottom-left coords and scrambled the image).

use crate::cache::pyramid_dir_for_source;
use crate::{ImageError, ImageInfo};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Bump when on-disk layout changes (invalidates cache).
pub const PYRAMID_FORMAT: u32 = 3;

/// Do not build levels larger than this edge (keeps decode/RAM bounded).
pub const MAX_LEVEL_EDGE: u32 = crate::SHRINK_MAX_EDGE;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelMeta {
    pub level: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyramidMeta {
    #[serde(default)]
    pub format_version: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub max_level: u32,
    /// Coarsest built level (overview).
    #[serde(default)]
    pub min_level: u32,
    /// Finest built level (inclusive).
    pub ready_level: i32,
    pub complete: bool,
    pub levels: Vec<LevelMeta>,
}

impl PyramidMeta {
    pub fn root_for(source: &Path) -> Result<PathBuf, ImageError> {
        pyramid_dir_for_source(source)
    }

    pub fn meta_path(root: &Path) -> PathBuf {
        root.join("meta.json")
    }

    pub fn level_jpeg(root: &Path, level: u32) -> PathBuf {
        root.join(format!("z{level}")).join("level.jpg")
    }

    pub fn load(root: &Path) -> Result<Self, ImageError> {
        let text = fs::read_to_string(Self::meta_path(root))?;
        Ok(serde_json::from_str(&text).map_err(|e| ImageError::Preview(e.to_string()))?)
    }

    pub fn save(&self, root: &Path) -> Result<(), ImageError> {
        fs::create_dir_all(root)?;
        let tmp = root.join("meta.json.partial");
        fs::write(&tmp, serde_json::to_string_pretty(self).unwrap())?;
        fs::rename(tmp, Self::meta_path(root))?;
        Ok(())
    }

    pub fn best_level_for_scale(&self, scale: f64) -> u32 {
        let ready = self.ready_level.max(0) as u32;
        let min_l = self.min_level.min(ready);
        if self.max_level == 0 {
            return min_l;
        }
        let desired = if scale <= 0.0 {
            min_l
        } else {
            let ratio = scale.clamp(0.0, 1.0);
            let from_full = (-ratio.log2()).floor() as i32;
            let level = self.max_level as i32 - from_full;
            level.clamp(0, self.max_level as i32) as u32
        };
        desired.clamp(min_l, ready)
    }
}

pub fn compute_max_level(width: u32, height: u32) -> u32 {
    let edge = width.max(height).max(1).min(MAX_LEVEL_EDGE);
    let mut level = 0u32;
    let mut size = 1u32;
    while size < edge {
        size = size.saturating_mul(2);
        level += 1;
        if level > 31 {
            break;
        }
    }
    level
}

pub fn level_dimensions(full_w: u32, full_h: u32, max_level: u32, level: u32) -> (u32, u32) {
    // Cap the "full" size we pyramid against so max level stays within MAX_LEVEL_EDGE.
    let cap = MAX_LEVEL_EDGE as f64;
    let edge = full_w.max(full_h) as f64;
    let (fw, fh) = if edge > cap {
        let s = cap / edge;
        (
            ((full_w as f64) * s).round().max(1.0) as u32,
            ((full_h as f64) * s).round().max(1.0) as u32,
        )
    } else {
        (full_w, full_h)
    };
    let shift = max_level.saturating_sub(level);
    let scale = 1u32 << shift;
    ((fw + scale - 1) / scale, (fh + scale - 1) / scale)
}

pub fn overview_level(full_w: u32, full_h: u32, max_level: u32, target: u32) -> u32 {
    for level in 0..=max_level {
        let (w, h) = level_dimensions(full_w, full_h, max_level, level);
        if w.max(h) >= target {
            return level;
        }
    }
    max_level
}

fn empty_meta(info: &ImageInfo) -> PyramidMeta {
    let max_level = compute_max_level(info.width, info.height);
    let min_level = overview_level(
        info.width,
        info.height,
        max_level,
        crate::PREVIEW_MAX_PIXEL_SIZE,
    );
    let mut levels = Vec::with_capacity(max_level as usize + 1);
    for level in 0..=max_level {
        let (w, h) = level_dimensions(info.width, info.height, max_level, level);
        levels.push(LevelMeta {
            level,
            width: w,
            height: h,
        });
    }
    PyramidMeta {
        format_version: PYRAMID_FORMAT,
        source_width: info.width,
        source_height: info.height,
        max_level,
        min_level,
        ready_level: -1,
        complete: false,
        levels,
    }
}

fn wipe_root(root: &Path) {
    let _ = fs::remove_dir_all(root);
    let _ = fs::create_dir_all(root);
}

/// Build overview level only (fast open).
pub fn ensure_pyramid_overview(source: &Path) -> Result<(PathBuf, PyramidMeta), ImageError> {
    let info = crate::probe(source)?;
    let root = PyramidMeta::root_for(source)?;
    fs::create_dir_all(&root)?;

    if let Ok(meta) = PyramidMeta::load(&root) {
        let jpeg = PyramidMeta::level_jpeg(&root, meta.min_level);
        if meta.format_version == PYRAMID_FORMAT
            && meta.source_width == info.width
            && meta.source_height == info.height
            && meta.ready_level >= meta.min_level as i32
            && jpeg.is_file()
        {
            return Ok((root, meta));
        }
        wipe_root(&root);
    }

    let mut meta = empty_meta(&info);
    meta.save(&root)?;
    let overview = meta.min_level;
    build_level(source, &root, &mut meta, overview)?;
    Ok((root, meta))
}

/// Build finer levels up to MAX_LEVEL_EDGE.
pub fn continue_pyramid_build(source: &Path, root: &Path) -> Result<PyramidMeta, ImageError> {
    let mut meta = PyramidMeta::load(root)?;
    if meta.format_version != PYRAMID_FORMAT {
        return Err(ImageError::Preview("pyramid format outdated".into()));
    }
    let next = (meta.ready_level + 1).max(meta.min_level as i32) as u32;
    for level in next..=meta.max_level {
        build_level(source, root, &mut meta, level)?;
    }
    meta.complete = true;
    meta.save(root)?;
    Ok(meta)
}

#[cfg(target_os = "macos")]
fn build_level(
    source: &Path,
    root: &Path,
    meta: &mut PyramidMeta,
    level: u32,
) -> Result<(), ImageError> {
    use crate::imageio::{cgimage_size, thumbnail_cgimage, write_cgimage_jpeg};

    let dest = PyramidMeta::level_jpeg(root, level);
    if meta.ready_level >= level as i32 && dest.is_file() {
        return Ok(());
    }

    let expected = &meta.levels[level as usize];
    let max_px = expected
        .width
        .max(expected.height)
        .max(1)
        .min(MAX_LEVEL_EDGE);
    tracing::info!(level, max_px, "building pyramid level jpeg");

    let image = thumbnail_cgimage(source, max_px)?;
    let (iw, ih) = cgimage_size(&image);
    meta.levels[level as usize] = LevelMeta {
        level,
        width: iw,
        height: ih,
    };

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    write_cgimage_jpeg(&image, &dest, 0.88)?;

    meta.format_version = PYRAMID_FORMAT;
    meta.ready_level = level as i32;
    meta.complete = meta.ready_level as u32 >= meta.max_level;
    meta.save(root)?;
    tracing::info!(level, ready = meta.ready_level, path = %dest.display(), "level ready");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn build_level(
    _source: &Path,
    _root: &Path,
    _meta: &mut PyramidMeta,
    _level: u32,
) -> Result<(), ImageError> {
    Err(ImageError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_stay_within_cap() {
        let m = compute_max_level(7920, 17208);
        let (w, h) = level_dimensions(7920, 17208, m, m);
        assert!(w.max(h) <= MAX_LEVEL_EDGE);
        let ov = overview_level(7920, 17208, m, 2048);
        let (ow, oh) = level_dimensions(7920, 17208, m, ov);
        assert!(ow.max(oh) >= 2048 || ov == m);
    }
}
