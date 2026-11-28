use crate::{ImageError, ImageInfo, PreviewInfo};
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, CFURLPathStyle, CFURL,
};
use objc2_core_graphics::CGImage;
use objc2_image_io::{
    kCGImageDestinationLossyCompressionQuality, kCGImagePropertyPixelHeight,
    kCGImagePropertyPixelWidth, kCGImageSourceCreateThumbnailFromImageAlways,
    kCGImageSourceCreateThumbnailWithTransform, kCGImageSourceShouldCache,
    kCGImageSourceThumbnailMaxPixelSize, CGImageDestination, CGImageSource,
};
use std::fs;
use std::path::Path;

fn file_url(path: &Path) -> Result<CFRetained<CFURL>, ImageError> {
    let s = path
        .to_str()
        .ok_or_else(|| ImageError::Probe(format!("non-UTF8 path: {}", path.display())))?;
    CFURL::with_file_system_path(
        None,
        Some(&CFString::from_str(s)),
        CFURLPathStyle::CFURLPOSIXPathStyle,
        false,
    )
    .ok_or_else(|| ImageError::Probe(format!("CFURL failed for {}", path.display())))
}

fn source_no_cache_options() -> CFRetained<CFDictionary<CFType, CFType>> {
    let key_cache = unsafe { kCGImageSourceShouldCache };
    CFDictionary::<CFType, CFType>::from_slices(
        &[key_cache.as_ref()],
        &[CFBoolean::new(false).as_ref()],
    )
}

fn cf_i64(dict: &CFDictionary<CFString, CFType>, key: &CFString) -> Option<i64> {
    let value = dict.get(key)?;
    let number = value.downcast::<CFNumber>().ok()?;
    number.as_i64()
}

pub fn probe(path: &Path) -> Result<ImageInfo, ImageError> {
    let url = file_url(path)?;
    let options = source_no_cache_options();
    let source =
        unsafe { CGImageSource::with_url(&url, Some(options.as_opaque())) }.ok_or_else(|| {
            ImageError::Probe(format!("CGImageSource open failed: {}", path.display()))
        })?;

    if unsafe { source.count() } == 0 {
        return Err(ImageError::Probe(format!(
            "no images in file: {}",
            path.display()
        )));
    }

    let props = unsafe { source.properties_at_index(0, Some(options.as_opaque())) }
        .ok_or_else(|| ImageError::Probe("missing image properties".into()))?;
    let props = unsafe { CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(props) };

    let width = cf_i64(&props, unsafe { kCGImagePropertyPixelWidth })
        .ok_or_else(|| ImageError::Probe("missing PixelWidth".into()))?;
    let height = cf_i64(&props, unsafe { kCGImagePropertyPixelHeight })
        .ok_or_else(|| ImageError::Probe("missing PixelHeight".into()))?;
    if width <= 0 || height <= 0 {
        return Err(ImageError::Probe(format!(
            "invalid dimensions {width}x{height}"
        )));
    }

    let uti = unsafe { source.r#type() }.map(|s| s.to_string());

    Ok(ImageInfo {
        width: width as u32,
        height: height as u32,
        uti,
    })
}

pub fn write_fit_preview(
    path: &Path,
    max_pixel_size: u32,
    dest_jpeg: &Path,
) -> Result<PreviewInfo, ImageError> {
    if max_pixel_size == 0 {
        return Err(ImageError::Preview("max_pixel_size must be > 0".into()));
    }

    if let Some(parent) = dest_jpeg.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write to a sibling temp file then rename for atomic cache fill.
    let tmp = dest_jpeg.with_extension("jpg.partial");
    let _ = fs::remove_file(&tmp);

    let url = file_url(path)?;
    let source_opts = source_no_cache_options();
    let source = unsafe { CGImageSource::with_url(&url, Some(source_opts.as_opaque())) }
        .ok_or_else(|| {
            ImageError::Preview(format!("CGImageSource open failed: {}", path.display()))
        })?;

    let max_px = CFNumber::new_i32(max_pixel_size as i32);
    let thumb_keys: [&CFType; 3] = [
        unsafe { kCGImageSourceCreateThumbnailFromImageAlways }.as_ref(),
        unsafe { kCGImageSourceCreateThumbnailWithTransform }.as_ref(),
        unsafe { kCGImageSourceThumbnailMaxPixelSize }.as_ref(),
    ];
    let thumb_vals: [&CFType; 3] = [
        CFBoolean::new(true).as_ref(),
        CFBoolean::new(true).as_ref(),
        max_px.as_ref(),
    ];
    let thumb_opts = CFDictionary::<CFType, CFType>::from_slices(&thumb_keys, &thumb_vals);

    let image =
        unsafe { source.thumbnail_at_index(0, Some(thumb_opts.as_opaque())) }.ok_or_else(|| {
            ImageError::Preview(format!("thumbnail decode failed: {}", path.display()))
        })?;

    let out_url = file_url(&tmp)?;
    let jpeg_uti = CFString::from_str("public.jpeg");
    let dest =
        unsafe { CGImageDestination::with_url(&out_url, &jpeg_uti, 1, None) }.ok_or_else(|| {
            ImageError::Preview(format!(
                "CGImageDestination create failed: {}",
                tmp.display()
            ))
        })?;

    let quality = CFNumber::new_f64(0.85);
    let prop_keys: [&CFType; 1] = [unsafe { kCGImageDestinationLossyCompressionQuality }.as_ref()];
    let prop_vals: [&CFType; 1] = [quality.as_ref()];
    let props = CFDictionary::<CFType, CFType>::from_slices(&prop_keys, &prop_vals);

    unsafe {
        dest.add_image(&image, Some(props.as_opaque()));
        if !dest.finalize() {
            let _ = fs::remove_file(&tmp);
            return Err(ImageError::Preview(format!(
                "JPEG finalize failed: {}",
                tmp.display()
            )));
        }
    }

    fs::rename(&tmp, dest_jpeg).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;

    let out_w = CGImage::width(Some(&image)) as u32;
    let out_h = CGImage::height(Some(&image)) as u32;
    tracing::info!(
        source = %path.display(),
        preview = %dest_jpeg.display(),
        out_w,
        out_h,
        max_pixel_size,
        "wrote image preview"
    );

    Ok(PreviewInfo {
        path: dest_jpeg.to_path_buf(),
        width: out_w,
        height: out_h,
    })
}

/// Decode a downsampled `CGImage` (ImageIO thumbnail path — not a full-res RGBA load
/// unless `max_pixel_size` covers the master).
pub fn thumbnail_cgimage(
    path: &Path,
    max_pixel_size: u32,
) -> Result<CFRetained<CGImage>, ImageError> {
    if max_pixel_size == 0 {
        return Err(ImageError::Preview("max_pixel_size must be > 0".into()));
    }
    let url = file_url(path)?;
    let source_opts = source_no_cache_options();
    let source = unsafe { CGImageSource::with_url(&url, Some(source_opts.as_opaque())) }
        .ok_or_else(|| {
            ImageError::Preview(format!("CGImageSource open failed: {}", path.display()))
        })?;

    let max_px = CFNumber::new_i32(max_pixel_size as i32);
    let thumb_keys: [&CFType; 3] = [
        unsafe { kCGImageSourceCreateThumbnailFromImageAlways }.as_ref(),
        unsafe { kCGImageSourceCreateThumbnailWithTransform }.as_ref(),
        unsafe { kCGImageSourceThumbnailMaxPixelSize }.as_ref(),
    ];
    let thumb_vals: [&CFType; 3] = [
        CFBoolean::new(true).as_ref(),
        CFBoolean::new(true).as_ref(),
        max_px.as_ref(),
    ];
    let thumb_opts = CFDictionary::<CFType, CFType>::from_slices(&thumb_keys, &thumb_vals);
    unsafe { source.thumbnail_at_index(0, Some(thumb_opts.as_opaque())) }
        .ok_or_else(|| ImageError::Preview(format!("thumbnail decode failed: {}", path.display())))
}

pub fn write_cgimage_jpeg(
    image: &CGImage,
    dest_jpeg: &Path,
    quality: f64,
) -> Result<(), ImageError> {
    if let Some(parent) = dest_jpeg.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dest_jpeg.with_extension("jpg.partial");
    let _ = fs::remove_file(&tmp);
    let out_url = file_url(&tmp)?;
    let jpeg_uti = CFString::from_str("public.jpeg");
    let dest =
        unsafe { CGImageDestination::with_url(&out_url, &jpeg_uti, 1, None) }.ok_or_else(|| {
            ImageError::Preview(format!(
                "CGImageDestination create failed: {}",
                tmp.display()
            ))
        })?;
    let q = CFNumber::new_f64(quality.clamp(0.1, 1.0));
    let prop_keys: [&CFType; 1] = [unsafe { kCGImageDestinationLossyCompressionQuality }.as_ref()];
    let prop_vals: [&CFType; 1] = [q.as_ref()];
    let props = CFDictionary::<CFType, CFType>::from_slices(&prop_keys, &prop_vals);
    unsafe {
        dest.add_image(image, Some(props.as_opaque()));
        if !dest.finalize() {
            let _ = fs::remove_file(&tmp);
            return Err(ImageError::Preview(format!(
                "JPEG finalize failed: {}",
                tmp.display()
            )));
        }
    }
    fs::rename(&tmp, dest_jpeg).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(())
}

pub fn cgimage_size(image: &CGImage) -> (u32, u32) {
    (
        CGImage::width(Some(image)) as u32,
        CGImage::height(Some(image)) as u32,
    )
}
