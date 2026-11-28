# RAM/CPU budgets for extremely large still-image viewing

## Exact/approximate RAM for uncompressed RGBA/BGRA of 7920×17208 and similar sizes; why a 400MB file can explode to 1GB+ decoded?

### Takeaway
Decoded RAM is almost entirely a function of **pixel count × bytes-per-pixel**, not file size: `7920×17208` is ~520 MiB as 8-bit RGBA/BGRA, and peak decode often exceeds that because of temporary YCbCr/progressive buffers, GPU dual-copies, and caches—so a few-hundred-MB compressed file routinely becomes 1 GB+ resident.

### Cited Findings
- Formula used across browser/viewer engineering: decoded bytes ≈ `width × height × 4` for 8-bit RGBA/BGRA; file size is irrelevant to the decoded bitmap cost — [Tile Viewer / browser RGBA memory math](https://tile-viewer-web.com/en/articles/image-browser-performance)
- Apple WWDC18 (Memory Deep Dive): a 2048×1536 photo that is only ~590 KB on disk still costs ~10 MB decoded (`2048×1536×4`); “memory use is related to the dimensions of the image, not its file size” — [WWDC18 session 416 transcript](https://developer.apple.com/videos/play/wwdc2018/416/)
- Calculated for this research’s target size: `7920×17208 = 136,287,360` pixels → **545,149,440 bytes ≈ 519.90 MiB (545.1 MB)** as 4 B/px RGBA/BGRA; RGB without alpha is **~389.9 MiB** at 3 B/px (local arithmetic from those dimensions)
- Reference decoded sizes (same 4 B/px rule): 8K UHD 7680×4320 ≈ **126.6 MiB**; 6000×4000 ≈ **91.6 MiB**; 16384×16384 ≈ **1024 MiB (1 GiB)**; 19937×9969 ≈ **758 MiB**; 23622×11811 ≈ **1064 MiB** — [Tile Viewer table](https://tile-viewer-web.com/en/articles/image-browser-performance); large-dimension cases computed locally
- JPEG decode intermediates: stb_image maintainer notes full decode may need roughly `input + output + 3·w·h` for YCbCr, and progressive 4:4:4 can need ~`w·h·6` extra 16-bit planes (area×13-class totals) — [stb JPEG memory issue](https://github.com/nothings/stb/issues/964)
- libjpeg/libjpeg-turbo can allocate from the SOF header before validating scans; a tiny crafted JPEG declaring ~65312×8224 can attempt ~2 GB (`×4`) allocation; apps should set `max_memory_to_use` / pixel caps — [libjpeg-turbo issue #249](https://github.com/libjpeg-turbo/libjpeg-turbo/issues/249)
- Rust `image` crate default `max_alloc` is **512 MiB** (non-strict) precisely because decode bombs are common — [image::Limits docs](https://docs.rs/image/latest/image/struct.Limits.html)
- libvips: formats without random access (typical JPEG/PNG) fully decompress first; “small” images (< **100 MB decompressed**) go to a memory buffer, larger ones to a temp disk file—so a naive pipeline still pays full decoded size once — [libvips Opening files](https://www.libvips.org/API/current/how-it-opens-files.html)
- A 16384×16384 BGRA8 Metal texture alone is ~**1 GB** of GPU memory — [Interrobang atlas note](https://interrobangapps.com/writing/temporal-atlas-filters)

### Inferences
- For Pipster-class assets (~8K×17K), **one full-res BGRA buffer ≈ 0.52 GiB**; holding two windows at full-res already ≈ 1 GiB before caches/GPU copies. Prefer never allocating that buffer for “fit window.”
- A “400 MB file” exploding past 1 GB typically means: (a) dimensions imply ≥0.5–1 GiB RGBA, (b) decode temps add tens–hundreds of MB, (c) CPU buffer + GPU/private/managed texture dual-residency, (d) ImageIO/CG caches retaining another copy.
- Always gate decode on **declared pixels × bpp** (and a hard max MP/bytes) before allocating; never trust compressed size.

### Gaps
- No single published measurement for exactly 7920×17208 with Apple ImageIO on current macOS (peak dirty vs. final buffer); figures above are arithmetic + analogous WWDC/libjpeg guidance.
- Exact ImageIO peak-multiplier for HEIC grid stitch on macOS Sequoia/Tahoe not found in primary Apple docs during this pass.

## CPU cost of full decode vs thumbnail/subsample decode vs tiled decode for JPEG/PNG/HEIC/TIFF?

### Takeaway
Full-resolution decode of ~100–270 MP images costs hundreds of ms to seconds on Apple Silicon; DCT/IDCT subsample or ImageIO thumbnail paths cut work toward the **output pixel count**, while true tiled TIFF/HEIF decode can limit CPU to **visible tiles only**—JPEG/PNG without tiling usually still require a sequential pass (or full unpack) even when only a region is needed.

### Cited Findings
- OpenBenchmarking tjbench decompression throughput (libjpeg-turbo 2.1.0): Apple **M4 ~326 MP/s**, **M2 ~223 MP/s**, **M1 ~201 MP/s** average — [OpenBenchmarking tjbench](https://openbenchmarking.org/test/pts/tjbench)
- Same suite, direct Mac mini comparison: M1 ~**147 MP/s**, M4 ~**328 MP/s** — [OpenBenchmarking result export](https://openbenchmarking.org/result/2411130-NE-APPLEM49600&grs&export=txt)
- Rule-of-thumb from those rates for `7920×17208` (~136 MP): ~**0.4–0.9 s** pure JPEG decode on M1–M4-class turbo paths if throughput holds; real ImageIO/color-convert paths are often slower
- jpeg-nitro M1 MacBook Air bench (≈24 MP camera JPEGs): ImageIO→BGRA ~**108 ms/image**, ImageIO thumbnail **3000 px** ~**67 ms**, libjpeg-turbo BGRX ~**185 ms**, VideoToolbox ~**88 ms** — [jpeg-nitro README](https://github.com/gereoffy/jpeg-nitro/tree/v1.0)
- On Intel i9 in the same suite, libjpeg-turbo **1/2 scale** ~100 ms vs full ~112–124 ms and **1/4 scale** ~96 ms—Huffman often dominates, so scale factors help less than pixel-count math suggests unless SIMD IDCT is the bottleneck — [jpeg-nitro README](https://github.com/gereoffy/jpeg-nitro/tree/v1.0)
- libjpeg-turbo supports IDCT scaling factors **1/8 … 2×** in 1/8 steps; **only 1/4 and 1/2 are SIMD-accelerated** — [libjpeg-turbo README](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/main/README.md)
- Apple: `CGImageSourceCreateThumbnailAtIndex` + `kCGImageSourceThumbnailMaxPixelSize` downsamples with a streaming API so you “only pay the dirty memory cost of the resulting image” and was cited ~**50% faster** than UIImage draw-downsample in WWDC18 — [WWDC18 416](https://developer.apple.com/videos/play/wwdc2018/416/)
- libvips sequential access: shrink/thumbnail pipelines can stream top-to-bottom from JPEG/PNG without full random-access unpack; random ops (e.g. vertical flip) force full decompress to memory/disk — [libvips Opening files](https://www.libvips.org/API/current/how-it-opens-files.html)
- libvips thumbnail of a large PNG with `VIPS_CONCURRENCY=1` reported ~**33 MB** libvips + ~**60 MB** pixel buffers in one discussion; dropping tile height further cut high-water to ~**17 MB** — [libvips discussion #4106](https://github.com/libvips/libvips/discussions/4106)
- Tiled TIFF: libtiff/libvips can fetch **any tile** and decompress only that tile; libvips keeps a recently-decompressed tile cache because libraries are often single-threaded — [libvips Opening files](https://www.libvips.org/API/current/how-it-opens-files.html)
- HEIF: phone HEIC `grid` tiles are commonly **512×512**; libheif can decode **only accessed tiles**; non-tiled HEIC still behaves as one tile — [libheif tiled images wiki](https://github.com/strukturag/libheif/wiki/Reading-and-Writing-Tiled-Images)
- PNG: no DCT subsample path analogous to JPEG; interlaced PNG is called out by libvips maintainers as “very expensive”; random access generally requires full decompress or sequential streaming — [libvips discussion #4106](https://github.com/libvips/libvips/discussions/4106); [libvips Opening files](https://www.libvips.org/API/current/how-it-opens-files.html)

### Inferences
- **First paint** should use ImageIO thumbnail / libjpeg IDCT scale / `vips_thumbnail`, targeting screen pixels—not full decode then scale.
- **JPEG** at 1/8–1/2: large RAM win (quadratic in scale); CPU win is format-dependent (Huffman vs IDCT bound).
- **TIFF (tiled + pyramid)** and **HEIF grid/tili** are the only common still formats that give true regional decode; strip TIFF/JPEG/PNG need an on-disk tile pyramid (Deep Zoom / pyramidal TIFF) for pan-zoom without full unpack.
- Budget a **shared decode pool** (1–2 concurrent heavy jobs): a single 136 MP JPEG can saturate a performance core cluster for ~0.5–1 s+.

### Gaps
- Public HEIC tile-decode ms/tile on Apple Silicon ImageIO (vs libheif) for 512×512 grids were not found with hard numbers.
- PNG deflate throughput vs JPEG for identical megapixel masters varies widely by filter/compression; no single MP/s figure should be treated as universal.

## What screen-sized "fit window" first paint strategy is standard (max dimension = window size × scale factor)?

### Takeaway
The industry-standard first paint for photos is: **downsample at decode time to roughly the window’s pixel dimensions**, using `max(width, height) in pixels = max(viewPoints) × displayScale` (or the min-scale that fits the aspect ratio), then progressively refine to tiles at higher zoom—never full-res first.

### Cited Findings
- Apple-recommended ImageIO pattern: `maxDimensionInPixels = max(pointSize.width, pointSize.height) * scale` passed as `kCGImageSourceThumbnailMaxPixelSize`, with `kCGImageSourceCreateThumbnailFromImageAlways`, `kCGImageSourceShouldCacheImmediately`, `kCGImageSourceCreateThumbnailWithTransform` — [Mehmet Baykar downsample post](https://mehmetbaykar.com/posts/reduce-ios-image-memory-with-downsampling/); [Jerry Zhang ImageIO post](https://zhangjipeng.com/post-ios-image-memory-downsampling.html)
- Prefer `CGImageSourceCreateWithURL` over loading full `Data` into memory before thumbnailing—material for large files — [Jerry Zhang](https://zhangjipeng.com/post-ios-image-memory-downsampling.html)
- Use per-window `@Environment(\.displayScale)` / the window’s screen scale rather than a global main-screen scale when multi-window / external displays differ — [Jerry Zhang](https://zhangjipeng.com/post-ios-image-memory-downsampling.html)
- WWDC18: scale to “the same dimensions as my view” before Core Image / display; ImageIO thumbnail avoids the multi-GB spike of full decode — [WWDC18 416](https://developer.apple.com/videos/play/wwdc2018/416/)
- Local fit examples for `7920×17208`:
  - 1440×900 @2× → fit-aspect ≈ **828×1800** (~**5.7 MiB**); max-side=2880 → **1325×2880** (~**14.6 MiB**)
  - 1920×1080 @2× → fit-aspect ≈ **994×2160** (~**8.2 MiB**); max-side=3840 → **1767×3840** (~**25.9 MiB**)
  - vs full-res **~520 MiB** (local arithmetic)

### Inferences
- Prefer **fit-aspect** (`scale = min(viewW/imgW, viewH/imgH)`) for true “fit window”; Apple’s `maxPixelSize` on the longer side is a close, slightly larger upper bound that still stays in the **~6–30 MiB** class for Retina laptop windows.
- First-paint budget target for a desktop viewer: **≤ ~32 MiB decoded preview per window** (≈ 4K-class max dimension), then tile on zoom-in.
- Cache key must include `(sourceID, maxPixelSize or level, colorManagement/orientation)` so two windows at different sizes don’t thrash one shared entry incorrectly.

### Gaps
- No Apple HIG document that mandates the exact formula for macOS multi-window photo viewers; practice is inferred from ImageIO samples + WWDC memory guidance.
- Whether `preparingThumbnail(of:)` on modern macOS matches ImageIO memory characteristics for >100 MP files was not confirmed with measurements here.

## Recommended tile sizes, mipmap levels, and LRU cache budgets for interactive pan/zoom?

### Takeaway
Use **256² (raster) or 512²** tiles, a power-of-two zoom pyramid (Deep Zoom–style, often 254+1 overlap → 256), and an **LRU byte budget** (typically **128–512 MiB** GPU/decoded working set) that never evicts the current viewport (+1 tile prefetch ring).

### Cited Findings
- Classic web maps: **256×256** raster tiles (Google/OSM); Mapbox/MapLibre vector often **512**; decoded tile + GPU buffers often **0.5–2 MB**, so **150–300 tiles** ≈ comfortable browser cache — [Google Maps frontend design notes](https://completefrontend.com/system-design/problems/google-maps)
- Deep Zoom / OpenSlide / libvips defaults: **tile_size=254, overlap=1** so `tile+2·overlap=256` (power of two) — [OpenSlide deepzoom.py](https://github.com/openslide/openslide-python/blob/main/openslide/deepzoom.py); [libvips making image pyramids](https://www.libvips.org/API/current/making-image-pyramids.html)
- OpenSlide generic TIFF: first directory tiled; reduced-resolution IFDs marked reduced-image for pyramid detection — [OpenSlide generic TIFF](https://openslide.org/formats/generic-tiff/)
- libvips `vips_tilecache` defaults: **128×128** tiles, up to **1000** tiles, LRU when `VIPS_ACCESS_RANDOM` — [vips_tilecache](https://www.libvips.org/API/current/method.Image.tilecache.html)
- WebGPU spatial guidance: budget LRU by **bytes** not count; example **capacityBytes 128–512 MiB**, high watermark **0.9**, low **0.7**, protect viewport + prefetch ring — [VRAM LRU cache article](https://www.spatialvisualization.org/performance-tuning-profiling-webgpu-spatial/vram-budget-management-across-tile-zoom-levels/building-an-lru-vram-cache-for-tile-buffers/)
- 512×512 `rgba8unorm` = **1 MiB** base + ~⅓ for full mip chain ≈ **1.33 MiB/tile**; 64 tiles ≈ **85 MiB**; three zoom levels resident can approach **~256 MiB** — [WebGPU tile atlas article](https://www.spatialvisualization.org/webgpu-architecture-for-spatial-visualization/texture-and-tile-atlas-management-in-webgpu/)
- XFrames MapView desktop: GPU LRU **max 512 tiles**, memory cache **1024** decoded entries — [XFrames MapView](https://xframes.dev/blog/interactive-maps-with-mapview)
- Chromium compositor: software tiles ~**256×256**; GPU tiles ~**viewport width × ¼ viewport height**; low-res tiling used during fast scroll then replaced — [CRP rasterization article summarizing cc](https://sujeet.pro/articles/crp-raster)
- Chromium image-decode working-set defaults: **128 MB** normal, **32 MB** low-end, **256 MB** when RAM ≥ ~4 GB threshold — [cc/tiles/image_decode_cache_utils.cc](https://chromium.googlesource.com/chromium/src/+/8d0b29778f6171306d8dc0727c71c5eae1e6ed72/cc/tiles/image_decode_cache_utils.cc)
- Chromium software image decode cache: drop high quality above **64 MB** image; consider **subrect** caching when original ≥ **64 MB** and a dimension ≥ **4096**, if needed subrect ≤ **50%** of full — [software_image_decode_cache.cc](https://chromium.googlesource.com/chromium/src/+/ad5c9b9b07e5b387c497d351c681d958e47c2b8f/cc/tiles/software_image_decode_cache.cc)
- Local tile RAM (RGBA, no mips): 256² = **256 KiB**; 64 tiles = **16 MiB**; 256 tiles = **64 MiB**; 512² tile = **1 MiB**; 256 such tiles = **256 MiB**

### Inferences
- Desktop still-viewer sweet spot: **256×256 or 512×512** Metal textures; mipmap **only within each tile** (or separate pyramid levels)—avoid atlas mip bleed.
- Pyramid levels: halve until ≤ one tile (Deep Zoom `onepixel`/`onetile`); for a 17K long edge, expect ~**14–15** binary levels.
- Suggested Pipster budgets (rules of thumb):
  - **L0 preview (fit window):** ≤ 32 MiB × N_visible_windows
  - **Decoded CPU tile LRU:** 64–128 MiB
  - **GPU tile LRU:** 128–256 MiB (raise toward 512 MiB on ≥32 GB Macs)
  - **Concurrent full-res slots:** 0–1 (prefer zero; use tiles)
- Prefetch **1 tile ring** around viewport; during zoom, keep previous level scaled until new tiles arrive (maps pattern).

### Gaps
- No consensus “photo app” byte budget published by Apple Photos / Lightroom for 2025–2026; map/browser numbers are the best quantitative proxies.
- Optimal tile size on Apple Silicon (256 vs 512) for HEIC grid alignment vs Metal cache lines needs app-specific profiling.

## When is GPU upload the bottleneck vs decode?

### Takeaway
Decode dominates for first presentation of compressed masters; **GPU upload/bandwidth dominates once pixels exist**, especially for multi-hundred-MB bitmaps, discrete/eGPU buses, dual managed copies, or when re-uploading on every pan—tiling keeps each transfer small and reusable.

### Cited Findings
- Apple Metal docs: discrete/eGPU bandwidth matters; **shared** storage is slow for dGPU/eGPU; **private** is fast on GPU; **managed** keeps dual copies (CPU + GPU) with sync cost; render on the GPU that drives the display — [Adjusting for GPU memory bandwidth tradeoffs](https://developer.apple.com/documentation/metal/adjusting-for-gpu-memory-bandwidth-tradeoffs)
- Metal 2D texture caps: **16384** px (Apple3–Apple9 / Mac2); **32768** on Apple10; older Apple2 **8192** — [Metal Feature Set Tables PDF](https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf); [Kyle Howells Metal tiling 2026](https://ikyle.me/blog/2026/metal-texture-tiling)
- 20K-class panorama: CPU fallback conversion **~78 s** vs GPU tiled path **~0.84–0.93 s** (~85×); 11K: CPU **~38 s** vs GPU **~0.31–0.34 s**; CPU path peak ~**1.5 GB** RAM called out — [Kyle Howells 2026](https://ikyle.me/blog/2026/metal-texture-tiling)
- Chromium: decode can run on workers, but **GPU upload is serialized** on the context-lock raster thread; when working-set exceeds budget, “at-raster” decode blocks workers — [CRP rasterization / GpuImageDecodeCache](https://sujeet.pro/articles/crp-raster)
- 16384² BGRA8 ≈ **1 GB** upload/resident texture—bandwidth-bound even if compute is trivial — [Interrobang](https://interrobangapps.com/writing/temporal-atlas-filters)
- jpeg-nitro note: after fast parallel decode, viewer architecture still cares about getting planes to the GPU efficiently (decode ≠ display) — [jpeg-nitro](https://github.com/gereoffy/jpeg-nitro/tree/v1.0)

### Inferences
- If `decode_ms << upload_ms` for screen-sized first paint, you’re over-decoding or using a bad pixel path; for fit-window thumbnails, decode should be the main cost.
- If users pan across a **full-res** texture every frame with CPU staging, upload/bandwidth wins the profile—fix with **persistent tiled MTLTextures** + UV atlas, not per-frame `replaceRegion` of the whole image.
- On Apple Silicon unified memory, upload tax is lower than dGPU/eGPU, but **allocation size and texture limits** still force tiling above 16K (or 32K on Apple10).
- Prefer decode → **IOSurface / private texture** once; reuse across frames; avoid managed double buffers for static photo tiles.

### Gaps
- Hard GB/s measurements for `MTLTexture.replaceRegion` of 520 MiB BGRA on M-series vs Intel dGPU were not located in this pass.
- ImageIO→Metal zero-copy paths (IOSurface-backed CGImage) performance vs explicit copy needs local Instruments data.

## How do multi-window apps cap concurrent full-res images (evict inactive windows to disk preview, shared decode pool, max N high-res slots)?

### Takeaway
Treat full-res as a scarce global resource: **one shared decode/LRU cache**, **byte-cost limits**, purgeable/disk-backed previews for inactive windows, and a hard **N∈{0,1,2}** “hot” high-res slots—Chromium/map apps show the quantitative pattern (128–256 MB working sets; never keep unbounded decoded bitmaps per window).

### Cited Findings
- Apple: `NSCache` aggressively caches when memory is free and discards under pressure; `countLimit` / `totalCostLimit` exist but are **not strictly enforced**; combine with `NSPurgeableData` so VM can drop content without paging — [Caching and Purgeable Memory](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/ManagingMemory/Articles/CachingandPurgeableMemory.html)
- WWDC13 efficient OS X apps: under pressure, reclaim NSCache/NSPurgeableData before swapping; name VM regions; prefer discardable caches over dirty anonymous memory — [WWDC13 notes](https://saurabhs.org/wwdc-notes/wwdc-13-building-efficient-osx-apps)
- Multi-window guidance: use **one shared NSCache** keyed by source+size; cost = decoded bytes; invalidate/version so windows don’t show stale generations — [NSCache eviction article](https://danielcosenza.com/posts/mac-nscache-eviction-lifecycle/)
- Chromium working-set budgets **32 / 128 / 256 MB** for decoded images; critical memory pressure evicts caches — [image_decode_cache_utils.cc](https://chromium.googlesource.com/chromium/src/+/8d0b29778f6171306d8dc0727c71c5eae1e6ed72/cc/tiles/image_decode_cache_utils.cc)
- PDF.js: Global/Regional/Local image caches; worker-side cleanup via `pdfDocument.cleanup()`; destroy loading tasks when UI dismisses documents to avoid doubled memory — [PDF.js discussion #20315](https://github.com/mozilla/pdf.js/discussions/20315); [PR #19368](https://github.com/mozilla/pdf.js/pull/19368)
- libvips: for large non-random formats, decompress to **temp disk** then window the file—natural “evict to disk” pattern — [libvips Opening files](https://www.libvips.org/API/current/how-it-opens-files.html)
- Process isolation: macOS ImageIO can parse out-of-process via `ImageIOXPCService` when `IIOEnableOOP=YES` / preference—contains decoder spikes/crashes; custom XPC + **IOSurface** handoff for app-owned decode pools — [Meta RTX ImageIO sandboxing](https://rtx.meta.security/mitigation/2023/09/11/Sandboxing-ImageIO-in-macOS.html); [Apple XPC Services](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html); [IOSurface cross-process notes](https://danielcosenza.com/posts/mac-iosurface-cross-process-buffer-lifecycle/)

### Inferences
- Practical multi-window policy for large stills:
  1. **All windows** keep only fit-window JPEG/HEIC preview on disk cache (few–tens of MB).
  2. **At most one** window owns a “detail” working set (visible tiles at current mip).
  3. On deactivate/occlude: drop GPU tiles → keep CPU preview → optionally drop to disk file.
  4. **Global decode queue** depth 1 (2 on ≥12-core / ≥32 GB).
  5. Cost accounting in MiB of **decoded+uploaded** bytes, not image count.
- XPC isolation helps reliability/security more than steady-state RAM unless the service is the only place that may allocate full-res scratch.

### Gaps
- No public source stating Apple Photos’ exact “max concurrent full-resolution images” constant.
- Optimal N for Pipster depends on machine RAM class; needs a small adaptive policy table (8 / 16 / 32 / 64 GB) validated empirically.

## Failure modes: memory spikes, thrashing, GPU texture limits — and mitigations

### Takeaway
Primary failure modes are **full-image decode spikes**, **cache thrash under oversubscribed tile budgets**, and **Metal 16K/32K texture dimension rejects**; mitigate with downsample-first, tiled pyramids, byte-based LRU with hysteresis, dimension checks before upload, and memory-pressure observers that demote inactive windows to disk previews.

### Cited Findings
- Spike: UIImage/full CGImage decode of huge assets → multi-GB dirty memory; WWDC case ~**7.5 GB** vs ~**93 MB** after ImageIO downsample — [WWDC18 416](https://developer.apple.com/videos/play/wwdc2018/416/)
- Even with thumbnail API, older iOS could spike (~**230 MB** while targeting ~44 MB output for a 23622×11811 source); reported fixed by iOS 13—shows decoder bugs can still spike — [Stack Overflow downsample spike](https://stackoverflow.com/questions/57708605/downsample-with-cgimagesourcecreatethumbnailatindex-make-memory-rise-dramaticall)
- Decompression bombs / header-declared huge dimensions — [libjpeg-turbo #249](https://github.com/libjpeg-turbo/libjpeg-turbo/issues/249); [image::Limits](https://docs.rs/image/latest/image/struct.Limits.html)
- Thrashing: entry-count caches can hold **40–400 MiB** for the “same” 60 tiles; use byte watermarks and don’t evict visible/prefetch tiles — [VRAM LRU article](https://www.spatialvisualization.org/performance-tuning-profiling-webgpu-spatial/vram-budget-management-across-tile-zoom-levels/building-an-lru-vram-cache-for-tile-buffers/)
- GPU limit: images > **16384** (or 32768 on Apple10) cannot be a single `MTLTexture`; must strip/grid tile or CPU fallback — [Metal Feature Set Tables](https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf); [Howells 2026](https://ikyle.me/blog/2026/metal-texture-tiling)
- Note: `7920×17208` **height 17208 > 16384**, so on pre-Apple10 Macs a **single** full-res texture is illegal even if RAM existed—tiling is mandatory for 1:1 zoom on many devices (local comparison to Metal limits)
- PDF.js large canvas: tiling desired but naive per-tile full operator replay multiplies CPU; needs region-aware rendering—warning that “tiles” without partial decode/work skipping can **worsen** CPU — [pdf.js #6419](https://github.com/mozilla/pdf.js/issues/6419)
- Chromium: when decode cache won’t fit, at-raster decode causes “raster suddenly slow”; checker-imaging shows placeholders until decode completes — [CRP rasterization](https://sujeet.pro/articles/crp-raster)
- libvips: unexpected memory often means JPEG/PNG full unpack; fix by converting to tiled TIFF/VIPS, sequential access, lower concurrency, smaller tile height — [libvips Opening files](https://www.libvips.org/API/current/how-it-opens-files.html); [discussion #4106](https://github.com/libvips/libvips/discussions/4106)

### Inferences
- Mitigation checklist for a macOS multi-window viewer:
  - **Header gate:** reject/soft-fail if `w×h×4` > policy (e.g. 256–512 MB) unless tiled path exists
  - **First paint:** ImageIO thumbnail to fit-window pixels
  - **Zoom:** pyramid tiles 256/512; LRU bytes with 0.7–0.9 watermarks
  - **Metal:** query family max 2D size; auto grid-tile when any edge exceeds
  - **Pressure:** `MemoryPressure` / `NSCache` purge → demote background windows to disk JPEG preview; cancel decode tasks
  - **Isolation:** optional `IIOEnableOOP` or app XPC decode so a decoder OOM kills a helper, not the UI
  - **Never** keep full-res RGBA for inactive windows

### Gaps
- Quantitative thrash curves (FPS vs cache size) for still-photo (not map) viewers on macOS 15/16 were not found.
- Exact JetStream/Instruments templates for ImageIO + Metal photo viewers in 2025–2026 Apple documentation were not retrieved beyond general Metal bandwidth docs.
