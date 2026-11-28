//! Probe image headers, write screen-sized JPEG previews, and build tile pyramids
//! for pan/zoom via ImageIO (macOS).

mod cache;
#[cfg(target_os = "macos")]
mod imageio;
#[cfg(not(target_os = "macos"))]
mod imageio_stub;
mod pyramid;

use std::path::{Path, PathBuf};

pub use cache::preview_cache_path;
pub use pyramid::{
    compute_max_level, continue_pyramid_build, ensure_pyramid_overview, level_dimensions,
    PyramidMeta, MAX_LEVEL_EDGE,
};

/// Max edge for generated PiP JPEG previews (Retina-friendly, ≤ ~16 MiB RGBA).
pub const PREVIEW_MAX_PIXEL_SIZE: u32 = 2048;

/// Soft edge threshold: above this, prefer shrink-before-mpv.
pub const SHRINK_MAX_EDGE: u32 = 8192;

/// Decoded RGBA budget (bytes). `width * height * 4` above this → shrink.
pub const SHRINK_RGBA_BUDGET: u64 = 128 * 1024 * 1024;

/// Metal 2D texture max on Apple3–Apple9 / Mac2 — always shrink past this edge.
pub const METAL_HARD_EDGE: u32 = 16384;

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub uti: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreviewInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("image I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot probe image: {0}")]
    Probe(String),
    #[error("cannot write preview: {0}")]
    Preview(String),
    #[error("image support requires macOS")]
    UnsupportedPlatform,
}

/// Whether opening the original in mpv would likely blow RAM / GPU texture limits.
pub fn needs_shrink(info: &ImageInfo) -> bool {
    let max_edge = info.width.max(info.height);
    if max_edge > METAL_HARD_EDGE || max_edge > SHRINK_MAX_EDGE {
        return true;
    }
    let rgba = (info.width as u64)
        .saturating_mul(info.height as u64)
        .saturating_mul(4);
    rgba > SHRINK_RGBA_BUDGET
}

/// Read pixel dimensions from the file header (no full bitmap decode).
pub fn probe(path: &Path) -> Result<ImageInfo, ImageError> {
    #[cfg(target_os = "macos")]
    {
        imageio::probe(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(ImageError::UnsupportedPlatform)
    }
}

/// Decode a fit thumbnail and write JPEG to `dest_jpeg`.
pub fn write_fit_preview(
    path: &Path,
    max_pixel_size: u32,
    dest_jpeg: &Path,
) -> Result<PreviewInfo, ImageError> {
    #[cfg(target_os = "macos")]
    {
        imageio::write_fit_preview(path, max_pixel_size, dest_jpeg)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, max_pixel_size, dest_jpeg);
        Err(ImageError::UnsupportedPlatform)
    }
}

/// Return a cached fit-preview JPEG path, creating it if missing.
pub fn ensure_fit_preview(path: &Path, max_pixel_size: u32) -> Result<PreviewInfo, ImageError> {
    let dest = preview_cache_path(path, max_pixel_size)?;
    if dest.is_file() {
        let meta = probe(&dest).unwrap_or(ImageInfo {
            width: max_pixel_size,
            height: max_pixel_size,
            uti: Some("public.jpeg".into()),
        });
        return Ok(PreviewInfo {
            path: dest,
            width: meta.width,
            height: meta.height,
        });
    }
    write_fit_preview(path, max_pixel_size, &dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrink_thresholds() {
        assert!(!needs_shrink(&ImageInfo {
            width: 3840,
            height: 2160,
            uti: None,
        }));
        assert!(needs_shrink(&ImageInfo {
            width: 7920,
            height: 17208,
            uti: None,
        }));
        assert!(needs_shrink(&ImageInfo {
            width: 9000,
            height: 6000,
            uti: None,
        }));
        // Just over 128 MiB RGBA: 6000×6000×4 = ~137 MiB
        assert!(needs_shrink(&ImageInfo {
            width: 6000,
            height: 6000,
            uti: None,
        }));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_fixture_tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-media")
            .join(name)
    }

    #[test]
    fn small_image_no_shrink() {
        let path = fixture("small.jpg");
        if !path.is_file() {
            eprintln!("skip: missing {}", path.display());
            return;
        }
        let info = probe(&path).expect("probe small");
        assert!(!needs_shrink(&info), "{info:?}");
    }

    #[test]
    fn wide_edge_shrinks_and_previews() {
        let path = fixture("wide_edge.jpg");
        if !path.is_file() {
            eprintln!("skip: missing {}", path.display());
            return;
        }
        let info = probe(&path).expect("probe wide");
        assert!(needs_shrink(&info), "{info:?}");
        let preview = ensure_fit_preview(&path, PREVIEW_MAX_PIXEL_SIZE).expect("preview");
        assert!(preview.path.is_file());
        assert!(preview.width.max(preview.height) <= PREVIEW_MAX_PIXEL_SIZE);
        let again = ensure_fit_preview(&path, PREVIEW_MAX_PIXEL_SIZE).expect("cache hit");
        assert_eq!(again.path, preview.path);
    }

    #[test]
    fn budget_square_shrinks() {
        let path = fixture("budget_square.jpg");
        if !path.is_file() {
            eprintln!("skip: missing {}", path.display());
            return;
        }
        let info = probe(&path).expect("probe budget");
        assert!(needs_shrink(&info), "{info:?}");
        let preview = ensure_fit_preview(&path, PREVIEW_MAX_PIXEL_SIZE).expect("preview");
        assert!(preview.width.max(preview.height) <= PREVIEW_MAX_PIXEL_SIZE);
    }

    #[test]
    fn wide_edge_pyramid_overview() {
        let path = fixture("wide_edge.jpg");
        if !path.is_file() {
            eprintln!("skip: missing {}", path.display());
            return;
        }
        let (root, meta) = ensure_pyramid_overview(&path).expect("overview");
        assert!(meta.ready_level >= 0);
        assert!(root.join("meta.json").is_file());
        let level = meta.ready_level as u32;
        let jpeg = PyramidMeta::level_jpeg(&root, level);
        assert!(jpeg.is_file(), "missing level jpeg {}", jpeg.display());
    }
}
