# Rust + Slint + mpv: huge still-image integration options (macOS PiP)

Scope: practical ways to open still images (including ~7920×17208, >400 MB on disk) in a Pipster-like stack — Rust controller, Slint UI library window, mpv as a separate OS process per floating media window via JSON IPC — with low RAM/CPU. Not Electron/Chromium. Sources as of 2025–2026.

Reference decode math for the target case: 7920×17208×4 B (RGBA8) ≈ **545 MiB** uncompressed; a >400 MB file on disk is compressed payload and does not imply peak RAM ≤ file size.

---

## Can mpv display still images efficiently? What happens with very large images (full decode)? Options for size / scale / hwdec?

### Takeaway
mpv is a competent still-image viewer for normal photos (used widely with `image-display-duration=inf` and image profiles), but it treats images as **single-frame video**: decode → full frame → VO upload/scale. For multi-gigapixel / GPU-texture-limit sizes it fails or balloons GPU/RAM; window autofit does **not** shrink-decode before upload.

### Cited Findings
- Official option `--image-display-duration=<seconds|inf>`: if the current file is an image, play it for that duration (default **5** since late 2024); `inf` keeps the file open forever until the user stops. Images are defined as having only 1 video frame and no audio; during that “playback” the player should not use significant resources. — [mpv manual (master)](https://mpv.io/manual/master/)
- Default duration was raised from 1 s to 5 s in PR #15291 (merged 2024-11-13). — [mpv PR #15291](https://github.com/mpv-player/mpv/pull/15291)
- Community image-viewer configs set `image-display-duration=inf`, high-quality scalers (`scale=spline36`, `correct-downscaling`, etc.), and use mpv’s scripting for pan/zoom. — [occivink/mpv-image-viewer mpv.conf](https://github.com/occivink/mpv-image-viewer/blob/master/mpv.conf); [occivink/mpv-image-viewer README](https://github.com/occivink/mpv-image-viewer)
- `--autofit` / `--autofit-larger` only control **window size** (GUI VOs), not decode resolution; they keep aspect ratio and limit how large the window grows. — [mpv manual autofit](https://mpv.io/manual/master/)
- Opening very large stills with OpenGL VO can yield a black window when the GPU cannot allocate a texture that big; software `vo=x11` worked where OpenGL failed (2017 issue still active into 2026). Suggested mitigations: conditional profiles on `width`/`height`, or `profile-cond=p['current-tracks/video/image']` with a software VO. — [mpv issue #4264](https://github.com/mpv-player/mpv/issues/4264)
- Anecdotal 2026 measurement on Wayland: default VO used ~**300 MB GPU** for a 3500×2333 / 3.9 MB JPEG; `vo=wlshm`/`x11` avoided nvidia-smi listing, but `wlshm` pushed memory into the compositor and OOMed after ~6 images. — [mpv issue #4264 comments](https://github.com/mpv-player/mpv/issues/4264); [Nick Janetakis mpv image config](https://nickjanetakis.com/blog/mpv-is-the-mvp-of-video-and-image-viewing)
- GPU VO path uploads decoded frames as textures; exceeding driver max image dimensions fails mapping (example: Vulkan “Requested image size … exceeds the maximum allowed dimensions”). — [mpv issue #14934](https://github.com/mpv-player/mpv/issues/14934)
- `--hwdec` is aimed at **video codecs** (h264/HEVC/etc.); still JPEG/PNG go through software image demux/decode. hwdec does not provide a documented “decode this JPEG at 1/8 size” path analogous to TurboJPEG scaling. — [mpv manual --hwdec](https://mpv.io/manual/master/)
- `--image-exts` only classifies extensions for cover-art / directory filters; it is not a memory control. — [mpv manual --image-exts](https://mpv.io/manual/master/)
- Video filters such as `--vf=scale` operate on decoded frames (full-res input then scale); they do not replace shrink-on-load decoders. — [mpv manual](https://mpv.io/manual/master/)

### Inferences
- For Pipster’s existing “one mpv process per floating window” path, **reusing mpv for ordinary images** is realistic and UX-consistent (same always-on-top window + IPC), with flags like `--loop-file=inf --image-display-duration=inf --autofit-larger=100%x100%` and an image-only profile.
- For **7920×17208** class files, expect: (1) full software decode into a ~0.5 GiB+ frame, (2) likely GPU texture failure or huge VRAM, (3) one such window already rivaling many video PiPs. mpv is **not** a tiling / progressive / shrink-on-load viewer for arbitrary huge photos.
- Helpful mpv knobs for images are mainly UX/windowing (`image-display-duration`, `autofit*`, image `profile-cond`, software VO fallback) — **not** peak-RAM reducers for multi-gigapixel stills.

### Gaps
- No primary mpv doc stating exact peak RAM for a given JPEG size on macOS `vo=libmpv`/`gpu`/`gpu-next`.
- No confirmed macOS-specific VO recommendation (Cocoa/Metal) for huge stills analogous to Linux `vo=x11`/`wlshm` workarounds.
- Unclear whether FFmpeg’s JPEG decoder inside mpv ever applies DCT scale factors when a scale filter is chained (no official guarantee found).

---

## What are Slint’s image loading limitations (full bitmap? max size?) for embedding large photos in Slint windows?

### Takeaway
Slint `Image` is a **CPU pixel-buffer / texture-backed UI asset**, not a streaming viewer: `load_from_path` / `load_from_data` decode supported formats into shared RGB(A) buffers (via `image` crate when enabled). There is **no documented max resolution**; practical limits are allocator + default `image` decode limits (~512 MiB alloc) and GPU texture size when uploaded.

### Cited Findings
- `Image::load_from_path` / `load_from_data` decode SVG/PNG/JPEG by default; more formats via `image-default-formats`. Construction APIs `from_rgb8` / `from_rgba8` / `from_rgba8_premultiplied` wrap a full `SharedPixelBuffer`. Clones share underlying data. — [Slint Rust Image docs](https://docs.slint.dev/latest/docs/rust/slint/struct.Image)
- Official docs show loading with the `image` crate into a full `RgbaImage`, then `SharedPixelBuffer::clone_from_slice` — i.e. **decode → full bitmap → Slint**. — [same](https://docs.slint.dev/latest/docs/rust/slint/struct.Image)
- `Image` is not `Send`; pixel buffers should be filled off-thread and converted on the Slint UI thread. — [same](https://docs.slint.dev/latest/docs/rust/slint/struct.Image)
- Internal `SharedPixelBuffer` stores `width`/`height` as `u32` and a contiguous pixel vector; `ImageInner::EmbeddedImage` holds that buffer. No max-dimension constant appears in the public API. — [slint image.rs](https://github.com/slint-ui/slint/blob/0917b6cd/internal/core/graphics/image.rs)
- Community guidance for in-memory decode still goes through full `image::load_from_memory` → RGB/RGBA buffer → Slint. — [slint discussion #10803](https://github.com/slint-ui/slint/discussions/10803)
- The `image` crate’s default `Limits::max_alloc` is **512 MiB** (non-strict); `max_image_width` / `max_image_height` are strict when set; decoders allocate `total_bytes()` for the full output. — [image::Limits](https://docs.rs/image/latest/image/struct.Limits.html); [image decoder_to_vec](https://docs.rs/image/latest/src/image/io/free_functions.rs.html)

### Inferences
- Embedding a 7920×17208 photo in a Slint window via `Image::load_from_path` would attempt a **full decode** (~545 MiB RGBA alone, plus decode scratch), likely hitting `image` limits or OOMing the **controller process** that hosts Slint — worse than crashing an mpv child.
- Slint remains appropriate for **screen-sized previews** (e.g. ≤ display width×height×scale) produced elsewhere, then `Image::from_rgba8`.
- Do not treat Slint as an image viewer for arbitrary user photos; keep huge-media surfaces outside the Slint library window (mpv child or dedicated viewer), matching Pipster’s video architecture.

### Gaps
- No Slint doc on GPU texture max size per backend (Metal/OpenGL/WGPU) when uploading large `SharedPixelBuffer`s.
- Whether Slint caches decoded disk images at full resolution indefinitely after `load_from_path` (cache eviction policy) was not fully verified in this pass.

---

## Best Rust ecosystem tools for downsample-on-load: libvips, MagickWand, Apple ImageIO, turbojpeg, etc. — memory profiles

### Takeaway
For arbitrary huge user photos on macOS, prefer **shrink-at-decode**: Apple **ImageIO** (`CGImageSourceCreateThumbnailAtIndex` via `objc2-image-io`) or **libvips `thumbnail` / sequential load**; JPEG-only **TurboJPEG scaling factors** (1/2, 1/4, 1/8) are excellent when format is JPEG. Avoid plain `image` crate full decode and MagickWand **without** pre-read size hints.

### Cited Findings

#### libvips
- Four open strategies: direct mmap, random-access loaders (tiled TIFF/OpenSlide), **full decompression** (small → RAM; large → temp disk; default disc threshold **100 MB** decompressed), and **sequential** (top-to-bottom, ~100-line cache, no full decompress). — [libvips How it opens files](https://www.libvips.org/API/current/how-it-opens-files.html)
- `vips_image_new_from_file(..., "access", VIPS_ACCESS_SEQUENTIAL)` avoids converting the whole image at once for large sequential pipelines. Header read is immediate; pixels load on first access. JPEG example: `fred.jpg[shrink=2]`. — [Vips.Image.new_from_file](https://www.libvips.org/API/current/ctor.Image.new_from_file.html)
- `vips_thumbnail` combines load+resize, uses shrink-on-load (JPEG/TIFF), block shrink, then Lanczos3; preferred over load-then-resize. — [Vips.Image.thumbnail](https://www.libvips.org/API/current/ctor.Image.thumbnail.html); [libvips developer checklist](https://www.libvips.org/API/8.16/Developer-checklist.html)
- Measured example (6048×4032 JPEG shrink ×0.1): `vips resize` ~124 MB / 0.23 s vs `vips thumbnail` ~69 MB / 0.08 s (includes CLI overhead). — [same checklist](https://www.libvips.org/API/8.16/Developer-checklist.html)
- 2026 workstation bench (22.5 MP → 400×267): `vips thumbnail` median peak RSS **65.4 MiB**; Magick default full decode **289.7 MiB**; Magick with `-define jpeg:size=…` **before** input **34.4 MiB**. — [VIPS Handbook benchmarking](https://vips.webp.se/en/production/benchmarking/)
- Rust: `vips` crate 0.2 documents `VipsImage::thumbnail_file` / `thumbnail_buffer` and warns not to use in-memory `thumbnail` after full decode. — [docs.rs/vips](https://docs.rs/crate/vips/latest)

#### ImageMagick / MagickWand
- Pixel cache scales as width×height×channels×depth; spills heap → mmap → disk under resource limits (`SetMagickResourceLimit`, policy.xml). A 25k×25k image can need ~4.7 GiB of cache. — [ImageMagick Architecture](https://imagemagick.org/architecture/)
- JPEG size hint must be set **before** read or Magick fully decodes first (order-sensitive). — [VIPS Handbook benchmarking](https://vips.webp.se/en/production/benchmarking/)
- Historical libvips vs ImageMagick benchmarks show IM using far more peak RSS for the same pipeline. — [libvips Speed_and_Memory_Use wiki](https://github.com/libvips/libvips/wiki/Speed_and_Memory_Use)

#### Apple ImageIO (macOS-native)
- `CGImageSourceCreateThumbnailAtIndex` with `kCGImageSourceThumbnailMaxPixelSize`, `CreateThumbnailFromImageAlways`, `CreateThumbnailWithTransform`, and `ShouldCache: false` on source creation is Apple’s recommended downsample path (WWDC 2018). — [Apple ImageIO Guide](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/ImageIOGuide/imageio_source/ikpg_source.html); [WWDC 2018 Image and Graphics Best Practices PDF](https://devstreaming-cdn.apple.com/videos/wwdc/2018/219mybpx95zm9x/219/219_image_and_graphics_best_practices.pdf)
- Rust bindings exist: **`objc2-image-io` 0.3.2** (2025) exposes `CGImageSource`, thumbnail keys, and `kCGImageSourceSubsampleFactor`. — [crates.io/objc2-image-io](https://crates.io/crates/objc2-image-io); [CGImageSource.rs](https://docs.rs/objc2-image-io/latest/src/objc2_image_io/generated/CGImageSource.rs.html)

#### TurboJPEG / jpeg-decoder / image / JPEG XL
- TurboJPEG decompress scaling is IDCT-based; factors are multiples of 1/8 (`ONE_HALF`, `ONE_QUARTER`, `ONE_EIGHTH`, etc.); ignored for lossless JPEG. — [turbojpeg::ScalingFactor](https://docs.rs/turbojpeg/latest/turbojpeg/struct.ScalingFactor.html); [tj3SetScalingFactor](https://docs.rs/turbojpeg-sys/latest/turbojpeg_sys/fn.tj3SetScalingFactor.html)
- `image` crate allocates a full output buffer (`decoder.total_bytes()`); default alloc ceiling 512 MiB — unsuitable as the primary path for >400 MB-class photos. — [image Limits / decoder_to_vec](https://docs.rs/image/latest/image/struct.Limits.html)
- JPEG XL: pure-Rust `jxl-rs` claims progressive rendering and lower memory than naive full buffers; `jpegxl-rs` wraps libjxl. Useful for JXL inputs, not a general JPEG/HEIC solution. — [libjxl/jxl-rs](https://github.com/libjxl/jxl-rs/); [inflation/jpegxl-rs](https://github.com/inflation/jpegxl-rs/)

#### Pyramid / Deep Zoom (optional cache)
- libvips `dzsave` builds DeepZoom/Zoomify/Google pyramids with memory scaling roughly with **image width**, not pixel count; used for 150k×150k-class imagery. — [libvips making image pyramids](https://www.libvips.org/API/current/making-image-pyramids.html); [libvips blog DeepZoom](https://libvips.blogspot.com/2013/03/making-deepzoom-zoomify-and-google-maps.html)

### Inferences
- **Memory ranking for “decode to screen-sized preview” (typical JPEG):** TurboJPEG scale or ImageMagick `jpeg:size` before read ≈ ImageIO thumbnail ≈ libvips thumbnail ≪ full Magick/`image` decode ≪ mpv full frame + GPU texture.
- **Best fit for Pipster on macOS:** ImageIO via `objc2-image-io` (zero extra native deps, HEIC/JPEG/PNG system codecs) **or** libvips if already shipping `vips` and needing Deep Zoom / TIFF / cross-format pipelines.
- Use TurboJPEG only as a fast path when `file`/`magic` says baseline JPEG; fall back to ImageIO/vips for HEIC/PNG/TIFF/WebP.
- MagickWand is workable only with strict resource limits + correct define ordering; heavier dependency and easier to misuse than vips/ImageIO.

### Gaps
- No published head-to-head RSS for ImageIO vs libvips thumbnail on a **7920×17208** macOS sample in this research pass.
- `magick` Rust crate maturity / packaging cost for a sandboxed .app was not audited.
- Exact ImageIO peak memory for progressive JPEG / huge HEIC edge cases remains format-dependent and not quantified here.

---

## Recommended pattern for Pipster-like apps: (A) mpv, (B) dedicated viewer, (C) screen preview + full-res on demand, (D) pyramid cache?

### Takeaway
**Hybrid C (+ selective A)** is the best default: treat images like video only when “small enough”; otherwise decode a **display-sized preview** in Rust (ImageIO/vips/turbojpeg) for Slint thumbnails / metadata, and open a floating window that either (A) uses mpv for moderate images or (B/C) a lightweight surface showing the preview with optional region full-res / (D) pyramid only when the user needs deep pan-zoom on multi-gigapixel files.

### Cited Findings
- Prior-art lightweight viewers scale-to-window rather than keeping full-res bitmaps for display: feh `--scale-down`; Qt’s recommended path is `QImageReader::setScaledSize` / `setClipRect` (handler must support the option or it still full-decodes). — [feh(1)](https://man.archlinux.org/man/feh.1.en); [Qt Wiki Loading Large Images](https://wiki.qt.io/Loading_Large_Images)
- qView caches decoded pixmaps with explicit memory caps (200 MB / 800 MB by preload mode), skips caching files larger than half the cache budget, and raises Qt6 allocation limit to 8 GiB — acknowledging huge images as a special case. — [qView qvimagecore.cpp](https://github.com/jurplel/qView/blob/dd0f8d97/src/qvimagecore.cpp); [qView Image Handling](https://deepwiki.com/jurplel/qView/5-image-handling)
- mpv-as-image-viewer is popular for quality scaling/color, but maintainers/users document GPU limits and suggest auto-profiles to **lower settings for huge images** — implying size-tiered behavior. — [occivink/mpv-image-viewer](https://github.com/occivink/mpv-image-viewer); [mpv #4264](https://github.com/mpv-player/mpv/issues/4264)
- libvips Deep Zoom is the established “convert once, tile forever” pattern for scientific/WSI zoom; OpenSeadragon recommends VIPS for conversion. — [OpenSeadragon creating zooming images](https://openseadragon.github.io/examples/creating-zooming-images/); [libvips dzsave](https://www.libvips.org/API/current/method.Image.dzsave.html)
- Pipster’s video architecture already uses **per-window mpv OS processes + Slint controller**; prior architecture notes favor process isolation for crash domains at a RAM cost. — [local prior note pip_multiwindow_architecture.md](../Video%20player%20đa%20cửa%20sổ%20nhẹ/pip_multiwindow_architecture.md)

### Inferences
| Pattern | When | RAM | Fit to Pipster |
|--------|------|-----|----------------|
| **A – mpv like video** | width×height under a budget (e.g. ≤16 MP or ≤GPU max tex) | Full frame + VO | Best UX reuse; bad for 7920×17208 |
| **B – dedicated viewer process** | Always, or only for “huge” tier | Can tile / ImageIO-only | Strong OOM isolation; more code |
| **C – screen preview + full-res on demand** | Default for all images | ~display pixels per window | Best default; zoom=crop-decode or second pass |
| **D – pyramid on first open** | User zooms deeply / files ≫ RAM | Disk + tile cache | Highest quality pan-zoom; slow first open, disk use |

- **Recommended policy:**  
  1. Probe dimensions via ImageIO/vips header (no full decode).  
  2. If under threshold → **A** (spawn mpv with image flags).  
  3. If over threshold → **C**: create screen-sized bitmap; show in floating window (either a tiny custom metal/Skia surface process, or Slint-only if preview is small).  
  4. On explicit “1:1 / deep zoom” → **D** background `dzsave` or tiled TIFF, or region decode — not automatic for every open.
- Pure **B** (always separate viewer) is justified if image decode bugs must never touch Slint; can still implement C/D inside that helper.
- Pure **D** on every open is wrong for a PiP app (latency + disk); reserve for “inspect huge photo” mode.

### Gaps
- No Pipster product decision yet on zoom UX (fit-only vs deep zoom), which gates whether D is needed.
- Exact size threshold (MP or max dimension) should be measured against Metal texture limits on target Mac GPUs.

---

## Multi-window PiP UX: many floating image windows without multiplying RAM?

### Takeaway
RAM multiplies with **decoded pixels retained per window**, not with “being an image.” Cap each window to a **display-sized** (or smaller) buffer, share one decode/thumbnail cache by content hash, recycle idle windows, and avoid N full-res bitmaps or N GPU textures of the source resolution.

### Cited Findings
- feh multi-window mode shows images in multiple windows; `--cache-size` caps imlib2 cache (default 4 MiB, max 2048 MiB) — explicit tradeoff of speed vs RAM. — [feh(1)](https://man.archlinux.org/man/feh.1.en)
- qView’s preload cache is cost-based (width×height×depth) with hard MB ceilings; large files are excluded from cache. — [qView qvimagecore.cpp](https://github.com/jurplel/qView/blob/dd0f8d97/src/qvimagecore.cpp)
- mpv: one process per window (Pipster model) duplicates process overhead; for images, each process that fully decodes still pays full-frame cost — community reports compositor/GPU blowups after only a handful of large stills. — [mpv #4264](https://github.com/mpv-player/mpv/issues/4264)
- Prior Pipster research: single-process multi-core amortizes GPU for **video**, but multi-process buys crash isolation; for images, isolation matters more because decode bugs/OOM are common with huge files. — [local prior note](../Video%20player%20đa%20cửa%20sổ%20nhẹ/pip_multiwindow_architecture.md)
- Qt large-image guidance: load scaled or clipped regions so memory ≈ viewport, not source. — [Qt Wiki Loading Large Images](https://wiki.qt.io/Loading_Large_Images)

### Inferences
- Budget example: 8 PiP windows × 1280×720×4 ≈ **30 MiB** of pixel buffers vs 8 × 545 MiB ≈ **4.3 GiB** if each keeps a 7920×17208 RGBA.
- Strategies that work with Pipster:
  - **Tiered open:** small → mpv; huge → preview-only helper.
  - **Global LRU thumbnail cache** in the Rust controller (paths → screen bitmap), windows only hold handles/IDs.
  - **One shared preview decoder worker** (thread or process) rather than decoding inside every mpv.
  - Drop / purge pixel buffers for occluded or miniaturized windows; keep file path + metadata.
- Do **not** open N mpv processes on N huge stills; that multiplies the worst case.

### Gaps
- No measurement of macOS WindowServer memory when many small Metal-backed floating windows show unique textures.
- Unclear whether reusing one long-lived “image helper” process for all preview windows beats one-shot helpers per open for RAM fragmentation.

---

## Cross-process isolation benefits for OOM safety when a bad huge file is opened

### Takeaway
Decoding untrusted/huge images in a **child process** (already true for Pipster’s mpv windows) confines allocator spikes and many crashes to that child; the Slint controller can show an error and respawn. On macOS, foreground apps are **not** aggressively jetsam-killed by default — so isolation + explicit footprint limits/watchdogs matter more than hoping the OS will save the UI process.

### Cited Findings
- macOS jetsam/memorystatus exists but for typical foreground apps is not an automatic hard per-app cap; pressure becomes swap/beachballs unless the process opted into limits. Physical footprint is the accounting metric (`ri_phys_footprint`). — [Locara macOS Memory Management notes](https://locara.dev/docs/notes/macos-memory-management); [Measuring a macOS process tree honestly](https://ineshin.space/papers/measuring-a-macos-process-tree-honestly/)
- `taskpolicy -m` can launch a process under a fatal per-process memory ceiling; exceeding the limit kills that process. — [same Ineshin paper](https://ineshin.space/papers/measuring-a-macos-process-tree-honestly/); [Ask Different memory limits](https://apple.stackexchange.com/questions/376456/is-there-some-way-to-specify-memory-limits-on-specific-apps-in-macos)
- Production pattern: PDF/image helpers run out-of-process with memory caps; on macOS, RLIMIT_DATA is awkward so supervisors poll `proc_pid_rusage` and kill over-budget helpers — keeping the main vault/UI alive. — [myCarlos sandbox issue #63](https://github.com/carlos-emr/mycarlos/issues/63)
- WebKit/WKWebView model (analogy): content process OOM terminates the content process, not necessarily the host app — host gets a termination callback. — [WKWebView process termination discussion](https://juejin.cn/post/7103463814246760485)
- If Slint/`image` decode runs in-process, a 512 MiB+ allocation failure or overcommit thrash hits the controller that owns all PiP orchestration. — [image Limits](https://docs.rs/image/latest/image/struct.Limits.html); [Slint Image](https://docs.slint.dev/latest/docs/rust/slint/struct.Image)

### Inferences
- **Keep huge decode out of the Slint process.** Prefer: mpv child (A) or a dedicated `pipster-image-helper` (B/C) that returns a small PNG/JPEG preview or shared-memory RGB of screen size over IPC.
- Add a soft budget: refuse or downscale when `width*height*4 > N` (e.g. 256 MiB) before spawning; still isolate for malicious/corrupt files that bypass probes.
- Optional: launch helpers via `taskpolicy -m` or custom footprint watchdog so one bad HEIC cannot fill swap for the whole session.
- Isolation does **not** reduce total machine RAM if the child still fully decodes — it only protects UI liveness; combine with shrink-on-load (C).

### Gaps
- No Apple public API guarantee equivalent to iOS jetsam for arbitrary Developer ID apps without private memorystatus entitlements.
- IPC design for returning previews (temp file vs POSIX shm vs Mach port) not specified for Pipster.

---

## Cross-cutting recommendation (for report synthesis)

1. **Never** `Image::load_from_path` a multi-hundred-MB photo into Slint.
2. **Probe → tier:** small/medium stills → existing mpv window path with `--image-display-duration=inf`; huge → ImageIO/vips/turbojpeg **thumbnail to window pixel size**, display in floating surface; optional Deep Zoom only on demand.
3. **Per-window RAM = f(viewport)**, with a process-wide LRU of previews.
4. **Decode in children** for OOM/crash isolation; monitor footprint on macOS.

Primary sources prioritized: mpv manual & issues, Slint Image docs, libvips opening/thumbnail/pyramid docs, Apple ImageIO/WWDC, crates.io `objc2-image-io` / `turbojpeg` / `vips` / `image`, feh/qView/Qt large-image prior art.
