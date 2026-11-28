# Pyramid / tiled formats and partial-decode strategies for huge still images

## What formats support tiled or pyramidal storage (TIFF/BigTIFF pyramid, JPEG2000/JP2, HEIF/AVIF tiles, OpenSlide, Deep Zoom/DZI, IIIF, WebP, PNG limitations)?

### Takeaway
True random-access viewing without a full bitmap decode requires either tiled storage (TIFF/BigTIFF tiles, HEIF grid/tili, JPEG2000 tiles) or an explicit multi-resolution pyramid (pyramidal TIFF, OpenSlide WSI, Deep Zoom/DZI, IIIF tile sets, HEIF `pymd`). Consumer JPEG/PNG/WebP are not pyramidal; they offer limited shrink/crop/progressive helpers but not a general tile pyramid.

### Cited Findings
- libvips can write tiled TIFF with `tile=true` (default tile 128×128), optional `pyramid=true` (decreasing-size pages), and `bigtiff=true` for files >4GB; pyramid layers can be consecutive pages or SubIFDs (`subifd`) for better OME compatibility — [libvips tiffsave](https://www.libvips.org/API/current/method.Image.tiffsave.html)
- OpenSlide treats a generic TIFF as pyramidal when the first IFD is tiled and later tiled IFDs have the “reduced resolution” bit set; missing/zero-byte tiles render transparent (OpenSlide ≥4.0.0) — [OpenSlide generic tiled TIFF](https://openslide.org/formats/generic-tiff/)
- OpenSlide vendor formats (Aperio `.svs`, Leica `.scn`, Philips, Ventana `.bif`, etc.) are generally single-file pyramidal tiled TIFF/BigTIFF with vendor metadata — [OpenSlide formats](https://openslide.org/formats/)
- libtiff provides tile-oriented I/O: tile width/length must be multiples of 16; `TIFFComputeTile` + `TIFFReadEncodedTile` decode one rectangular tile without reading the whole image; strip/scanline random access is limited when compression uses multi-row strips — [libtiff Using The TIFF Library](https://libtiff.gitlab.io/libtiff/libtiff.html); [TIFFReadEncodedTile](https://libtiff.gitlab.io/libtiff/functions/TIFFReadEncodedTile.html); [TIFFtile](https://libtiff.gitlab.io/libtiff/functions/TIFFtile.html)
- libvips `dzsave` builds Deep Zoom (`.dzi` + `_files`), Zoomify, Google Maps, and IIIF/IIIF3 layouts with small memory; default Deep Zoom tile size 254 with overlap 1 (classic Microsoft Deep Zoom defaults) — [libvips Building image pyramids](https://www.libvips.org/API/current/making-image-pyramids.html)
- IIIF Image API 3.0 exposes region (`full` / `x,y,w,h` / `pct:…`) and size parameters so clients request only a viewport tile at a scale factor; Image Information can advertise `tiles` with `width`/`height` and power-of-two `scaleFactors` — [IIIF Image API 3.0](https://iiif.io/api/image/3.0/)
- JPEG2000 (OpenJPEG) supports decoding a single tile (`opj_decode_tile_data`) and setting a decode ROI (`opj_set_decode_area`) after `opj_read_header`, plus resolution reduction (`-r` / reduce factor 2ⁿ) — [OpenJPEG openjpeg.c](https://www.openjpeg.org/doxygen/openjpeg_8c.html); [opj_decompress DeepWiki](https://deepwiki.com/uclouvain/openjpeg/3.2-decompression-utility-(opj_decompress))
- libheif supports three tiling schemes: HEIF `grid` (phone cameras often use ~512×512 tiles; metadata-heavy; ≤65535 tiles), ISO 23001-17 `unci` (built-in tiling, lossless codecs only), and proprietary `tili` (low overhead, skipped tiles, streaming-oriented); unified read API `heif_image_handle_get_image_tiling` + `heif_image_handle_decode_image_tile` — [libheif tiled images wiki](https://github.com/strukturag/libheif/wiki/Reading-and-Writing-Tiled-Images) (updated Oct 30, 2024)
- libheif also documents multiresolution pyramids as a `pymd` entity group of layer images (ordered low→high res) with per-layer binning/tile counts — [libheif tiled images wiki](https://github.com/strukturag/libheif/wiki/Reading-and-Writing-Tiled-Images)
- libwebp advanced decode supports crop-then-scale so output RAM tracks the requested output size, not the full source, plus incremental decode (`WebPIDecoder`) — [WebP API Documentation](https://developers.google.com/speed/webp/docs/api); [libwebp decode.h](https://github.com/webmproject/libwebp/blob/main/src/webp/decode.h)
- PNG has no native tile index or mipmap pyramid; Adam7 interlacing enables progressive display but still requires multi-pass processing over the image structure; libpng’s row APIs assume sequential/pass-oriented reads, not random 2D tiles — [libpng manual](https://www.libpng.org/pub/png/libpng-manual.txt); [PNG Definitive Guide ch.14](https://libpng.org/pub/png/book/chapter14.html)
- Baseline JPEG is MCU/scanline-oriented (not a tile pyramid); libjpeg-turbo documents partial decode via `jpeg_skip_scanlines` / `jpeg_crop_scanline` (crop snapped to iMCU) and IDCT shrink ratios M/8 — [libjpeg-turbo libjpeg.txt](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/main/doc/libjpeg.txt)

### Inferences
- For a desktop “open arbitrary huge file” viewer, **on-disk formats that already ship tiles+pyramid** (tiled pyramidal TIFF, OpenSlide WSI, HEIF grid/tili+pymd, JP2 tiled) can be viewed with near-zero import cost; everything else needs an external pyramid (DZI/IIIF/tiled TIFF) or format-specific shrink/crop.
- WebP is closer to JPEG than to TIFF: region+scale helpers help first paint, but pan/zoom at 1:1 across a multi-hundred-MP image still wants a built tile pyramid.
- PNG is the weakest consumer format for huge stills: progressive Adam7 helps streaming UX, not RAM-bounded pan/zoom.

### Gaps
- No primary 2025–2026 Apple Preview source was found documenting whether Preview builds an on-disk pyramid vs. ImageIO thumbnails only for huge stills.
- Exact industry prevalence of HEIF `tili`/`pymd` in consumer cameras vs. phone `grid` HEIC is not quantified in the sources reviewed; phone HEIC grid is documented, `tili` is described as proprietary/experimental in packaging.


## How do viewers show a 7920×17208 (~400MB+) image quickly without decoding the full bitmap?

### Takeaway
Fast first paint uses a **bounded downsample or embedded thumbnail**; interactive pan/zoom then loads **only tiles at the current pyramid level covering the viewport** (plus a small prefetch margin), never a full RGBA framebuffer of the source.

### Cited Findings
- Apple Image I/O: `CGImageSourceCreateThumbnailAtIndex` with `kCGImageSourceThumbnailMaxPixelSize` and `kCGImageSourceCreateThumbnailFromImageIfAbsent`/`Always` produces a size-bounded preview; WWDC 2018 guidance uses `kCGImageSourceShouldCache: false` on the source and `kCGImageSourceShouldCacheImmediately: true` on the thumbnail options for downsample-on-open — [Apple ImageIO Guide](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/ImageIOGuide/imageio_source/ikpg_source.html); [WWDC 2018 Image and Graphics Best Practices PDF](https://devstreaming-cdn.apple.com/videos/wwdc/2018/219mybpx95zm9x/219/219_image_and_graphics_best_practices.pdf)
- Image I/O incremental sources (`CGImageSourceCreateIncremental` + `CGImageSourceUpdateData`) allow drawing as bytes accumulate for large/network loads — [Apple ImageIO Guide](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/ImageIOGuide/imageio_source/ikpg_source.html)
- libvips documents shrink-on-load: JPEG 1/2, 1/4, 1/8 via libjpeg; WebP general shrink-on-load; OpenSlide/TIFF pyramids select a subsampled layer; HEIC embedded thumbnails are preferred over EXIF JPEG thumbs — [libvips HOWTO Image shrinking (wiki via Context7)](https://github.com/libvips/libvips/wiki/HOWTO----Image-shrinking)
- OpenSlide `openslide_read_region(osr, dest, x, y, level, w, h)` reads an arbitrary rectangle at a chosen pyramid level into a caller buffer — [OpenSlide API](https://openslide.org/api/openslide_8h.html)
- OpenSeadragon loads missing tiles via ImageLoader/ImageJob from a TileSource (`getTileUrl` / custom `downloadTileStart`); DZI supplies per-level tile URLs — [OpenSeadragon custom TileSource](https://openseadragon.github.io/examples/tilesource-custom-advanced/); [DziTileSource](https://openseadragon.github.io/docs/OpenSeadragon.DziTileSource.html)
- Photoshop stores image data as a **mipmap / pyramidal set of tiles** so zoomed-out previews and zoomed-in edits operate on appropriate resolution; a software VM pages tiles between RAM and scratch when documents exceed memory — [Chrome Developers: How Photoshop solved files larger than memory](https://developer.chrome.com/blog/how-photoshop-solved-working-with-files-larger-than-can-fit-into-memory)
- Adobe documents Cache Levels / Cache Tile Size so high-resolution documents keep multiple cached versions for faster redraw; “Huge Pixel Dimensions” preset recommended for panoramas/matte paintings — [Photoshop Performance preferences](https://helpx.adobe.com/my_ms/photoshop/using/performance-preferences.html)

### Inferences
- For 7920×17208, a practical viewer pipeline is: (1) ImageIO/libvips thumbnail ≤ screen max edge for first paint in tens–hundreds of ms; (2) if format has native levels/tiles, bind OpenSlide/libtiff/libheif/OpenJPEG region reads; (3) else async convert to tiled pyramid on disk while showing the thumbnail.
- “~400MB+” usually refers to **compressed file size**; the decoded RGBA cost is larger still (~520 MiB for this resolution)—so avoiding full decode matters even more than file size suggests.

### Gaps
- No measured first-paint latency for Preview/Photoshop on exactly 7920×17208 was found in primary docs; timing claims would need local benchmarks.


## What is the typical memory model: decode only visible tiles at current zoom, keep a multi-level pyramid cache, LRU tile cache sizes?

### Takeaway
Professional and deep-zoom viewers keep a **bounded LRU (or VM-paged) cache of decoded tiles**, select the pyramid level nearest the current zoom, decode only tiles intersecting the viewport (+ prefetch), and evict least-recently-used tiles when a count or RAM budget is exceeded.

### Cited Findings
- OpenSeadragon `TileCache` stores tiles for a Viewer; eviction is driven by `maxImageCacheCount` (default discussed as **200**); when exceeded, least-recently-displayed tiles are removed — [OpenSeadragon TileCache](https://openseadragon.github.io/docs/OpenSeadragon.TileCache.html); [GitHub issue #2054](https://github.com/openseadragon/openseadragon/issues/2054)
- Maintainers note memory ≈ `tile_px² × 4 × cached_tiles` (e.g. 256×256×4 = 262 KiB/tile); users rarely load an entire base level because LRU discards tiles as the viewport pans — [GitHub issue #2054](https://github.com/openseadragon/openseadragon/issues/2054)
- CacheRecord can be shared by multiple Tile references; keyed typically by tile URL / level / x / y — [OpenSeadragon CacheRecord](https://openseadragon.github.io/docs/OpenSeadragon.CacheRecord.html); [DziTileSource](https://openseadragon.github.io/docs/OpenSeadragon.DziTileSource.html)
- Photoshop VM: RAM reserved for VM is split into pages; image tiles (square pixel regions of a layer) consume one or more pages; source/dest tiles for an operation are faulted in from scratch files while other pages are flushed — [Chrome Developers Photoshop VM article](https://developer.chrome.com/blog/how-photoshop-solved-working-with-files-larger-than-can-fit-into-memory)
- Photoshop Cache Levels default 4 (up to 8); higher levels improve redraw for large pixel dimensions; cache tile size trades filter throughput vs. brush locality — [Photoshop Performance preferences](https://helpx.adobe.com/my_ms/photoshop/using/performance-preferences.html)
- Adobe community experts describe the on-screen zoomed-out view as operating on the image pyramid/cache, so blend/adjustment previews at <100% can differ from true full-res results — [Adobe Community: cache levels](https://community.adobe.com/questions-712/i-work-with-very-large-files-containing-many-layers-in-photoshop-what-should-cache-levels-be-set-at-1624614); [Adobe Community: zoomed-out detail](https://community.adobe.com/questions-712/is-there-a-way-to-stop-photoshop-from-offloading-details-when-i-zoom-too-far-1166717)
- libheif network reader can request file ranges for individual tiles (`request_range` / `preload_range_hint` / `release_file_range`), enabling a streaming tile cache without downloading the whole HEIF — [libheif tiled images wiki](https://github.com/strukturag/libheif/wiki/Reading-and-Writing-Tiled-Images)
- IIIF practice notes often prefer ~512×512 tiles as a museum-sector default balancing request count vs. over-fetch during pan — [DAW: Configuring IIIF Image API](https://www.digital-asset-workflow.org/iiif-image-delivery-manifest-generation/configuring-the-iiif-image-api/)

### Inferences
- A desktop still-viewer can mirror OpenSeadragon’s model locally: disk-backed JPEG/WebP tiles + in-RAM LRU of decoded GPU textures / CGImages, budgeted by count or MiB.
- Multi-level cache usually means **keep a few coarse levels resident** (cheap, good for overview) and **aggressively LRU the finest level**.
- Typical practical budgets: ~50–200 tiles → ~13–50 MiB decoded RGBA at 256², or ~50–200 MiB at 512²—orders of magnitude below a full 7920×17208 framebuffer.

### Gaps
- No single “industry standard” LRU MiB number for native desktop apps; OpenSeadragon’s default of 200 tiles is web-specific guidance, not a hard desktop standard.
- Exact Preview.app tile-cache internals are not publicly documented in the sources found.


## Can common consumer formats (JPEG, PNG, HEIC) be partially decoded / thumbnail-extracted without full decode? What APIs/libraries do this?

### Takeaway
**Yes, with format-specific limits:** JPEG and WebP support shrink-on-load and limited region decode; HEIC often has an embedded thumbnail and may be a tile `grid`; PNG can stream/progressively decode rows/passes but cannot efficiently random-access 2D tiles. Prefer ImageIO thumbnails, libvips shrink-on-load, libjpeg-turbo crop/skip, libwebp crop+scale, and libheif thumbnail/tile APIs.

### Cited Findings
- **JPEG (libjpeg-turbo):** `scale_num/scale_denom` ratios M/8 (1/8 uses DC of each 8×8 block—very fast); `jpeg_skip_scanlines` for vertical jumps; `jpeg_crop_scanline` for horizontal crop snapped to iMCU boundaries — [libjpeg-turbo libjpeg.txt](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/main/doc/libjpeg.txt); [libvips discussion cited in libjxl #4297](https://github.com/libjxl/libjxl/issues/4297)
- **JPEG caveats:** shrink-on-load can show edge artifacts when MCU padding is atypical; not a substitute for a full tile pyramid when panning at 1:1 — [libjpeg-turbo issue #297](https://github.com/libjpeg-turbo/libjpeg-turbo/issues/297)
- **PNG:** progressive/Adam7 and `png_read_rows` allow streaming and row-oriented reads; interlaced files need multiple passes with retained row buffers; there is no standard PNG tile-offset table for O(1) 2D region decode — [libpng manual](https://www.libpng.org/pub/png/libpng-manual.txt)
- **WebP:** `WebPDecoderOptions.use_cropping` + `use_scaling` so memory scales with output; incremental `WebPIAppend` API — [Google WebP API](https://developers.google.com/speed/webp/docs/api)
- **HEIC/AVIF (libheif):** `heif_image_handle_get_list_of_thumbnail_IDs` / `heif_image_handle_get_thumbnail` for embedded thumbs; `heif_image_handle_decode_image_tile` for grid/tili/unci; full decode API remains `heif_decode_image` — [libheif README](https://github.com/strukturag/libheif); [fileformats.dev libheif guide](https://fileformats.dev/heic/libraries/cpp/libheif); [libheif tiling wiki](https://github.com/strukturag/libheif/wiki/Reading-and-Writing-Tiled-Images)
- **Apple ImageIO (macOS/iOS):** thumbnail creation and incremental loading as above; modern writeups emphasize bounding `kCGImageSourceThumbnailMaxPixelSize` to display size × scale — [danielcosenza.com ImageIO article](https://danielcosenza.com/posts/mac-imageio-incremental-decoding/); [Apple ImageIO Guide](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/ImageIOGuide/imageio_source/ikpg_source.html)
- **libvips:** unified shrink-on-load path across JPEG/WebP/pyramidal TIFF/OpenSlide/HEIC thumbnail — [libvips Image shrinking HOWTO](https://github.com/libvips/libvips/wiki/HOWTO----Image-shrinking)
- **JPEG2000:** OpenJPEG ROI + resolution reduction + per-tile decode — [OpenJPEG docs](https://www.openjpeg.org/doxygen/openjpeg_8c.html)

### Inferences
- Best “open anything” first paint on Apple platforms: **ImageIO thumbnail API** (handles HEIC/JPEG/PNG/TIFF uniformly), then escalate to specialized libraries for true region decode.
- For pan/zoom of a non-tiled 400MB JPEG/PNG, partial decode APIs reduce work for **overview** and **narrow strips**, but continuous 1:1 browsing still benefits from converting to tiled pyramid once.

### Gaps
- Whether ImageIO’s HEIC path always uses grid-tile decode under the hood for `CGImageSourceCreateThumbnailAtIndex` vs. always decoding then scaling is not spelled out in Apple’s public guide.
- Exact behavior of EXIF JPEG thumbnail vs. full decode when `CreateThumbnailFromImageIfAbsent` is false was not re-verified against current macOS 15/16 release notes in this pass.


## What preprocessing (build pyramid on import, convert to tiled TIFF, generate mipmaps) is worth it for a desktop viewer that opens arbitrary user files?

### Takeaway
**Always:** size-bounded downsample for first paint (cheap, no persistent cache). **Worth persistent preprocessing** when the user will pan/zoom, when the file lacks native tiles/levels, or when the file is opened repeatedly: convert to **tiled pyramidal TIFF (or DZI/IIIF tiles)** with libvips. On-the-fly shrink is enough for “fit to window”; disk pyramids win for interactive exploration.

### Cited Findings
- libvips `dzsave` “can generate pyramids for large images using only a small amount of memory” and supports Deep Zoom / Zoomify / Google / IIIF layouts; `tiffsave … tile pyramid bigtiff` is the simpler single-file alternative — [libvips Building image pyramids](https://www.libvips.org/API/current/making-image-pyramids.html); [tiffsave](https://www.libvips.org/API/current/method.Image.tiffsave.html)
- OpenSlide-aware workflows can `dzsave` from a lower OpenSlide level (`file.mrxs[level=1]`) to avoid decoding full base resolution while building a viewer pyramid — [libvips Building image pyramids](https://www.libvips.org/API/current/making-image-pyramids.html)
- libvips guidance: prefer format shrink-on-load, keep ~2× headroom, then do a high-quality final shrink (JPEG block-shrink alone aliases) — [libvips Image shrinking HOWTO](https://github.com/libvips/libvips/wiki/HOWTO----Image-shrinking)
- Photoshop’s production approach for edit (not just view) is mipmapped tiles + scratch VM—heavier than a viewer needs, but validates pyramid+tile as the scalable model — [Chrome Developers Photoshop VM](https://developer.chrome.com/blog/how-photoshop-solved-working-with-files-larger-than-can-fit-into-memory)
- IIIF/Deep Zoom servers typically pre-tile once so clients never ask the origin to decode the full raster per pan — [IIIF Image API 3.0](https://iiif.io/api/image/3.0/); [libvips dzsave](https://www.libvips.org/API/current/making-image-pyramids.html)
- HEIF can store a `pymd` group so overview layers exist in-file without a sidecar pyramid—useful if you control export, not for arbitrary user JPEGs — [libheif tiling wiki](https://github.com/strukturag/libheif/wiki/Reading-and-Writing-Tiled-Images)

### Inferences
- Recommended policy for a desktop “arbitrary files” viewer (2025–2026 best practice synthesis):
  1. **Immediate:** ImageIO/libvips thumbnail ≤ max(screen) for first paint.
  2. **If native pyramid/tiles exist** (tiled TIFF, OpenSlide, JP2 tiles, HEIF grid/tili): stream regions; optional light disk cache of decoded tiles.
  3. **If not** (common JPEG/PNG/untiled TIFF) **and** user zooms/pans: background build **tiled pyramidal BigTIFF** or **DZI** into an app cache directory keyed by content hash; keep original file read-only.
  4. Prefer **single-file pyramidal TIFF** for local desktop cache (one inode, libtiff/OpenSlide/libvips friendly); prefer **DZI/IIIF** if sharing with web viewers.
- Tradeoff: on-the-fly downsample = zero disk, poor 1:1 pan CPU; import pyramid = disk/time cost (often seconds–minutes for multi-hundred-MP), then pan/zoom is tile-cheap and RAM-bounded.

### Gaps
- Optimal tile size (256 vs 512) and JPEG quality for a local desktop cache were not empirically benchmarked here; IIIF museum practice favors 512, Deep Zoom historically 254+overlap, libvips TIFF default 128.
- Whether to delete pyramid cache automatically (size budget) is a product decision without a single cited standard.


## Approximate RAM for full RGBA decode of 7920×17208 vs tiled viewport-only approach

### Takeaway
Full 8-bit RGBA decode of 7920×17208 is about **520 MiB** of pixel buffers alone (before decoder scratch, ICC transforms, and GPU copies). A viewport-only tiled approach for a ~2560×1440 display is on the order of **~14 MiB** for the visible framebuffer, or **~15–50 MiB** with a modest LRU of 256² tiles—roughly **10–35× less** than full decode, often more when zoomed out on a coarse pyramid level.

### Cited Findings
- Computed pixel counts (this research): 7920×17208 = **136,287,360** pixels; ×4 bytes RGBA = **545,149,440 bytes ≈ 519.9 MiB**; ×3 bytes RGB ≈ **389.9 MiB** — calculation from stated dimensions
- OpenSeadragon maintainer example: 256×256×4 = **262 KiB per decoded tile**; default cache 200 tiles ⇒ up to **~50 MiB** of tile RGBA if all slots filled with 256² tiles — [GitHub issue #2054](https://github.com/openseadragon/openseadragon/issues/2054)
- Computed viewport example: 2560×1440×4 ≈ **14.1 MiB**; covering that viewport with 256² tiles needs about **10×6 = 60** tiles ≈ **15 MiB** decoded (before overlap/prefetch) — calculation from stated dimensions
- Photoshop notes that opening for edit decompresses far beyond on-disk size; VM + mipmap tiles exist specifically because full residency is often impossible — [Chrome Developers Photoshop VM](https://developer.chrome.com/blog/how-photoshop-solved-working-with-files-larger-than-can-fit-into-memory)
- WebP docs state advanced crop/scale decode makes memory track **output** size, not input—same principle as viewport-sized decode — [Google WebP API](https://developers.google.com/speed/webp/docs/api)
- libjpeg-turbo documents partial decode as useful in memory-constrained environments so the whole JPEG need not be materialized — [libjpeg-turbo libjpeg.txt](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/main/doc/libjpeg.txt)

### Inferences
- For the user’s ~400MB compressed file: expect **decoded** working set ≫ file size if naïvely expanded; tiled viewing keeps RAM near **viewport × bpp × (1 + prefetch) × (#cached zoom levels)**.
- At fit-to-window (~e.g. image scaled to ~2000 px tall), a single downsampled buffer might be only a few MiB—another reason thumbnail/shrink-on-load is the right first paint.
- GPU upload often doubles peak transiently (CPU bitmap + texture); tile streaming keeps that spike per-tile.

### Gaps
- Peak RAM including decoder scratch (JPEG Huffman tables, PNG zlib window, HEVC CTU buffers) was not measured; cited figures are **framebuffer/tile** estimates only.
- 16-bit/channel or wide-gamut float pipelines would roughly double or quadruple the RGBA numbers above; not specified for the sample image.
