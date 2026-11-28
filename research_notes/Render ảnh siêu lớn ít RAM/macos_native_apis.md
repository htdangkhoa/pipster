# macOS-native APIs for large still-image rendering with low memory

## How does ImageIO / CGImageSource enable thumbnails, incremental loading, and subsampled decode without a full-size bitmap?

### Takeaway
ImageIO’s preferred path for huge stills is: open a `CGImageSource` from a URL (not a fully decoded bitmap), create a size-capped thumbnail with `CGImageSourceCreateThumbnailAtIndex` (optionally with `kCGImageSourceSubsampleFactor` 2/4/8 for JPEG/HEIF/TIFF/PNG), and use `CGImageSourceCreateIncremental` only when bytes arrive in chunks—not as a substitute for spatial tiling.

### Cited Findings
- Image sources are the preferred way (macOS 10.4+) to bring image data into an app; a source can expose images, thumbnails, and properties without the app managing a raw full-size buffer itself — [Creating and Using Image Sources (Apple)](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/ImageIOGuide/imageio_source/ikpg_source.html)
- Thumbnail creation via `CGImageSourceCreateThumbnailAtIndex` supports options including `kCGImageSourceCreateThumbnailFromImageIfAbsent` / `…Always`, `kCGImageSourceCreateThumbnailWithTransform`, and `kCGImageSourceThumbnailMaxPixelSize` so Image I/O can produce a max-dimension-capped thumbnail (from embedded thumbs or by generating one) — [Creating and Using Image Sources (Apple)](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/ImageIOGuide/imageio_source/ikpg_source.html)
- WWDC 2018 documents the production downsample pattern: create the source with `kCGImageSourceShouldCache: false` (represent the file; do not decode yet), then thumbnail with `kCGImageSourceCreateThumbnailFromImageAlways`, `kCGImageSourceShouldCacheImmediately`, `kCGImageSourceCreateThumbnailWithTransform`, and `kCGImageSourceThumbnailMaxPixelSize` set to the on-screen max dimension in pixels; decoded buffer size then tracks display size, not file native size — [WWDC 2018 Session 219 slides PDF](https://devstreaming-cdn.apple.com/videos/wwdc/2018/219mybpx95zm9x/219/219_image_and_graphics_best_practices.pdf); [WWDC 2018 transcript index](https://nonstrict.eu/wwdcindex/wwdc2018/219/)
- WWDC 2018 states decoding is CPU-intensive, the decoded image buffer persists for repeat rendering, and its size is proportional to the **original** image dimensions, not the view size; downsampling shrinks that persistent allocation (example cited in session materials: ~31.5 MiB → ~18.4 MiB for the demo case) — [WWDC 2018 Session 219 slides PDF](https://devstreaming-cdn.apple.com/videos/wwdc/2018/219mybpx95zm9x/219/219_image_and_graphics_best_practices.pdf); [WWDCNotes summary](https://wwdcnotes.com/documentation/wwdc18-219-image-and-graphics-best-practices/)
- `kCGImageSourceSubsampleFactor` scales returned image data by integer factor **2, 4, or 8**; if unsupported, ImageIO may return a larger/full-size image; supported only for **JPEG, HEIF, TIFF, and PNG** (macOS 10.11+) — [kCGImageSourceSubsampleFactor (Apple Developer)](https://developer.apple.com/documentation/imageio/kcgimagesourcesubsamplefactor)
- WebKit’s ImageDecoderCG uses `kCGImageSourceSubsampleFactor` together with `kCGImageSourceThumbnailMaxPixelSize` / `CGImageSourceCreateThumbnailAtIndex` for asynchronous/drawing-sized decodes, and `CGImageSourceCreateImageAtIndex` for synchronous native-size decode—illustrating the public API combination used in shipping Apple software — [WebKit ImageDecoderCG.cpp](https://github.com/WebKit/WebKit/blob/main/Source/WebCore/platform/graphics/cg/ImageDecoderCG.cpp)
- Incremental loading: `CGImageSourceCreateIncremental` creates an empty source; `CGImageSourceUpdateData` / `UpdateDataProvider` must each time receive **all accumulated bytes so far** (not only the newest chunk); apps can create partial images with `CGImageSourceCreateImageAtIndex` and poll `CGImageSourceGetStatusAtIndex` until `kCGImageStatusComplete` — [Creating and Using Image Sources (Apple)](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/ImageIOGuide/imageio_source/ikpg_source.html); [CGImageSourceUpdateData](https://apple-docs.everest.mt/docs/imageio/cgimagesourceupdatedata(_:_:_:)/); [CGImageSourceCreateIncremental](https://apple-docs.everest.mt/docs/imageio/cgimagesourcecreateincremental(_:)/)
- Practical guidance from implementers: prefer `CGImageSourceCreateWithURL` over loading entire multi-hundred-MB files into `NSData` first; always set `kCGImageSourceCreateThumbnailWithTransform` for EXIF orientation; `kCGImageSourceCreateThumbnailFromImageAlways` avoids missing-embedded-thumbnail edge cases — [Fast Thumbnails with CGImageSource (macguru.dev)](https://macguru.dev/fast-thumbnails-with-cgimagesource/)
- `kCGImageSourceShouldCacheImmediately` is documented as controlling whether decode/cache happens at image-creation time, enabling background-thread decode control in the WWDC pattern; community reports note it is not always a hard “decode finished before return” guarantee on every OS path — [WWDC 2018 transcript](https://nonstrict.eu/wwdcindex/wwdc2018/219/); [Stack Overflow discussion](https://stackoverflow.com/questions/22994153/image-decompression-in-ios-7)

### Inferences
- For a ~7920×17208 / >400 MB file, first paint should use URL-backed `CGImageSource` + `ThumbnailMaxPixelSize` ≈ viewport max dimension (and optionally subsample 2/4/8 for JPEG/HEIF/TIFF/PNG), not `CreateImageAtIndex` at native size.
- Incremental ImageIO is for progressive byte arrival (network/stream); it does **not** by itself provide viewport tiling. Once the file is on disk, URL + thumbnail/subsample is the memory win.
- Subsample factor alone is coarse (powers of two) and format-limited; combine with `ThumbnailMaxPixelSize` for viewport-accurate sizing.

### Gaps
- Apple docs do not publish exact JPEG MCU / HEIF tile decode internals for how much RAM ImageIO uses while producing a thumbnail from a 400 MB+ file.
- No primary Apple doc found stating whether `CGImageSourceCreateThumbnailAtIndex` always avoids allocating a temporary full-resolution buffer for every codec (behavior is format-dependent in practice).
- Official numeric guarantee that `ShouldCacheImmediately` completes decode before return is weaker than the name implies; treat WWDC’s “control when decode happens” as the authoritative intent.

---

## How do CATiledLayer / NSScrollView + tiled drawing work for large images, and what are memory characteristics?

### Takeaway
`CATiledLayer` asynchronously draws only tiles intersecting the visible (and LOD) region on background threads; AppKit uses tiled backing for oversized `NSScrollView` document views precisely because ordinary `CALayer`s are bounded by GPU texture size—so memory tracks visible tiles (+ LOD cache), not the full image, **if** draw code loads tile data on demand rather than decoding the whole bitmap first.

### Cited Findings
- Apple’s Core Animation guide states `CATiledLayer` is “optimized for displaying large images in an efficient manner” and “manages a large image by breaking it into smaller tiles that can be managed and rendered individually,” with support for zooming in/out — [Setting Up Layer Objects (Apple)](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/SettingUpLayerObjects/SettingUpLayerObjects.html)
- Official overview: as the renderer needs data, `draw(in:)` / `drawLayer:inContext:` runs on **one or more background threads** for a single tile; clip bounds and CTM determine tile bounds and resolution; invalidation via `setNeedsDisplay` is asynchronous; **do not set `contents` directly**—that disables tiled async provision and turns the layer into a regular `CALayer` — [CATiledLayer (Apple Developer)](https://developer.apple.com/documentation/quartzcore/catiledlayer)
- Defaults: `tileSize` = 256×256; `levelsOfDetail` = 1 (each level half the previous resolution); `levelsOfDetailBias` = 0 (extra magnified levels, e.g. bias 2 → 2× and 4×) — [CATiledLayer Class Reference (archive)](https://leopard-adc.pepas.com/documentation/GraphicsImaging/Reference/CATiledLayer_class/Introduction/Introduction.html)
- AppKit release notes: ordinary layer-backed views fail beyond GPU max texture size (historically ~2046×2046 on then-current GL hardware); for large scroll document views AppKit uses `CATiledLayer` with a grid of tiles (default 256×256) drawn as they become visible and **garbage-collectable when offscreen**; tiles appear asynchronously while scrolling; enclosing `NSScrollView` / `NSClipView` must also be layer-backed for correct scrolling — [Mac OS X AppKit release notes (Layer Size Limits and Tiled Layers)](https://leopard-adc.pepas.com/releasenotes/Cocoa/AppKit.html)
- WWDC 2013 “Optimizing Drawing and Scrolling on OS X”: layer-backed large document views are chopped into tiles; only tiles intersecting the visible clip area are drawn; each visible tile gets its own `drawRect:`; apps must honor `dirtyRect` / `getRectsBeingDrawn:`; AppKit may choose dynamic tile aspect ratios for skinny/tall views — [WWDC 2013 Session 215 index](https://nonstrict.eu/wwdcindex/wwdc2013/215/)
- Apple PhotoScroller sample: nested scroll views + `CATiledLayer` for pan/zoom of high-resolution photos; `TilingView` loads per-scale/row/col tile images (`tileForScale:row:col:`) rather than one giant bitmap — [PhotoScroller sample (Apple archive)](https://developer.apple.com/library/archive/samplecode/PhotoScroller/Listings/Classes_TilingView_h.html); [PhotoScroller ReadMe (GitHub mirror)](https://github.com/robertwalker/PhotoScroller)
- ZoomingPDFViewer pattern (applicable conceptually to huge stills): show a low-res full-page stand-in immediately, then overlay `CATiledLayer` content that draws only the clipped tile region (for PDF: `CGContextDrawPDFPage`; for images: draw the corresponding image rect/tile file) — [ZoomingPDFViewer sample](https://developer.apple.com/library/archive/samplecode/ZoomingPDFViewer/Introduction/Intro.html); [PDFScrollView.m archive listing](https://web.archive.org/web/20120323033735/developer.apple.com/library/ios/samplecode/ZoomingPDFViewer/Listings/Classes_PDFScrollView_m.html)
- Practitioner warning: attaching a full decoded image to the tiled layer (e.g. via `imageNamed:` / loading the whole file) recreates the original memory problem; tiles must be **on-disk slices or region decodes loaded on demand** — [Subduing CATiledLayer (CIMGF)](https://www.cimgf.com/2011/03/01/subduing-catiledlayer/); [iOS Scroll Views / tiling chapter](https://www.apeth.com/iOSBook/ch20.html)

### Inferences
- Memory characteristic (when done correctly): O(visible_tiles × tile_pixels × bytes_per_pixel × LOD_cache), plus compressed file pages faulted by the decoder—not O(full_width × full_height).
- For 17208-tall images, AppKit’s automatic tiled document backing helps **view** backing stores, but the app must still avoid decoding the full CGImage in `drawRect:`; pair tiling with ImageIO region/thumbnail strategies or pre-tiled assets (PhotoScroller style).
- PDFView-style UX for stills ≈ low-res overview + tiled high-res fill-in + LOD bias for sharp zoom—not a single `NSImageView`.

### Gaps
- Exact modern AppKit tile eviction policy / default tile size under Metal (vs historical OpenGL 256² notes) is not fully specified in current public docs.
- No Apple-published RAM formula for how many offscreen tiles `CATiledLayer` retains under memory pressure on Apple Silicon.

---

## Can Metal / IOSurface / CVPixelBuffer stream tiles to the GPU without keeping the full image in RAM?

### Takeaway
Yes, at the architecture level: decode/upload **only tile-sized** textures (or sparse-texture tiles), share them via IOSurface-backed `CVPixelBuffer` / `MTLTexture` (including across processes with `IOSurfaceCreateXPCObject`), and optionally use sparse textures so unmapped regions consume no texture memory—provided no step materializes a full-resolution CPU bitmap or single oversized `MTLTexture`.

### Cited Findings
- Apple Metal Feature Set Tables: Maximum **2D texture width and height** is **16,384 px** for Apple3–Apple9 and Mac2 families; **32,768 px** for Apple10–Apple11; sparse color textures supported from **Apple6** (with placement/automatic heap nuances by generation) — [Metal Feature Set Tables PDF](https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf)
- Sparse textures: query tile pixel dimensions with `sparseTileSize(with:pixelFormat:sampleCount:)`; map/unmap memory in sparse-tile units so only needed regions are backed — [sparseTileSize documentation](https://apple-docs.everest.mt/docs/metal/mtldevice/sparsetilesize(with:pixelformat:samplecount:)/); [Converting between pixel and sparse tile regions](https://apple-docs.everest.mt/docs/metal/converting-between-pixel-regions-and-sparse-tile-regions/)
- WWDC 2020: IOSurface abstracts image memory shareable across Core Video, Metal, **processes**, and GPU memory regions; `CVPixelBuffer` usually wraps an IOSurface; recycling via `CVPixelBufferPool` requires holding use counts / completing Metal work before release — [Decode ProRes with AVFoundation and VideoToolbox (WWDC20)](https://developer.apple.com/videos/play/wwdc2020/10090/)
- Recommended Metal bridge: `CVMetalTextureCacheCreateTextureFromImage` keeps MTLTexture↔IOSurface mapping alive and avoids manual IOSurface use-count tracking; if using IOSurface directly, `IOSurfaceIncrementUseCount` until command-buffer completion — [WWDC20 ProRes session](https://developer.apple.com/videos/play/wwdc2020/10090/); [WWDC22 HDR/EDR + Metal](https://developer.apple.com/videos/play/wwdc2022/110565/?time=177)
- WWDC 2021: Metal textures rendered into IOSurface-backed `CVPixelBuffer` planes share the same underlying system-memory surface (zero-copy between Metal and Core Video consumers on Apple silicon paths described) — [Create image processing apps powered by Apple silicon (WWDC21)](https://developer.apple.com/videos/play/wwdc2021/10153/)
- Practical oversized-panorama write-up aligning with the feature tables: if dimensions exceed device max texture size, split into vertical strips or a grid of textures; single `MTKTextureLoader` upload fails past the limit — [Loading Oversized 360 Panoramas with Metal (Kyle Howells)](https://ikyle.me/blog/2026/metal-texture-tiling)
- Cross-process: Apple XPC docs explicitly call out `IOSurfaceCreateXPCObject` / `IOSurfaceLookupFromXPCObject` to pass surfaces between an XPC drawing service and the main app without CFPropertyList limitations — [Creating XPC Services (Apple)](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html)
- Independent engineering write-up: create IOSurface, wrap as Metal texture in producer and consumer, display via `CALayer.contents = surface`; no pixel copy across process boundary when synchronized correctly — [Russ Bishop – Cross-process Rendering](http://russbishop.net/cross-process-rendering)

### Inferences
- Streaming tiles without full-image RAM means: **CPU-side** never holds width×height×4; **GPU-side** holds only mapped sparse tiles or a small LRU of dense tile textures sized well under 16K.
- IOSurface/CVPixelBuffer are the right **transport** for tiles (especially helper→UI process); they do not magically decode a gigapixel file—you still need ImageIO/tile files/custom decoders feeding each tile.
- Unified memory on Apple Silicon reduces discrete-VRAM paging concerns but does **not** remove the need for tiling: address space and jetsam/memory-pressure still punish full decoded buffers (~545 MB for 7920×17208 RGBA8 alone).

### Gaps
- No Apple sample titled “stream still-image tiles via sparse MTLTexture from ImageIO” was found; sparse texture docs target general resource residency, not ImageIO integration.
- Public docs do not quantify whether `CGImage.cropping(to:)` / `CGImageCreateWithImageInRect` can decode a region of JPEG/HEIF without decompressing sibling regions for every format.

---

## What does Preview.app or Photos do conceptually for large photos (publicly documented patterns only)?

### Takeaway
Apple does not publish Preview.app’s internal large-image pipeline; the publicly documented patterns that match “fast first paint + pan/zoom” are PhotoScroller/ZoomingPDFViewer-style tiled layers, ImageIO downsampling (WWDC 2018), and Quick Look thumbnails for lightweight previews—not reverse-engineered Preview behavior.

### Cited Findings
- PhotoScroller (Apple sample) is the canonical documented pattern for rich pan/zoom of high-resolution photos using `CATiledLayer` and pre-sliced tiles at multiple scales — [PhotoScroller](https://developer.apple.com/library/archive/samplecode/PhotoScroller/History/History.html); [ReadMe mirror](https://github.com/robertwalker/PhotoScroller)
- ZoomingPDFViewer documents the “low-resolution stand-in + tiled high-resolution overlay” UX used for large zoomable page content — [ZoomingPDFViewer](https://developer.apple.com/library/archive/samplecode/ZoomingPDFViewer/Introduction/Intro.html)
- `PDFView` (PDFKit) encapsulates zoom/navigation for PDFs as a high-level widget; custom large-content viewers are steered toward tiled layers when building outside that widget — [PDFView documentation](https://developer.apple.com/documentation/pdfkit/pdfview.md); [CATiledLayer large-image guidance](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/SettingUpLayerObjects/SettingUpLayerObjects.html)
- Quick Look client API `QLThumbnailImageCreate(allocator, url, maxThumbnailSize, options)` returns a thumbnail (or NULL), is thread-safe but synchronous (call off the main thread), and takes a **maximum** size—useful for Finder-like first previews without decoding full resolution — [QLThumbnailRequest Reference](https://leopard-adc.pepas.com/documentation/UserExperience/Reference/QLThumbnailRequest_Ref/Reference/reference.html)
- WWDC 2018 Image & Graphics Best Practices is Apple’s public teaching for “don’t decode full resolution to show a smaller view,” which is the same product requirement Preview/Photos face for large stills — [WWDC 2018 Session 219](https://nonstrict.eu/wwdcindex/wwdc2018/219/)

### Inferences
- Conceptually (documented samples, not Preview source): (1) metadata + cheap thumbnail for instant UI, (2) display-sized downsample for fit-to-window, (3) tiled/LOD decode for 1:1 and zoomed inspection.
- Relying on “what Preview does” without samples is unsafe for implementation; stick to PhotoScroller + ImageIO + optional Metal tile upload.

### Gaps
- **No reliable public Apple documentation** of Preview.app or Photos.app internal tiling, memory caps, or decoder choices for multi-hundred-MB stills.
- Photos framework (`PHImageManager` target-size requests) is iOS/macOS Photos-library oriented and was not deeply sourced here for arbitrary filesystem gigapixel files.

---

## Are there limits (max texture size on Apple Silicon GPUs, max CGImage dimensions) that force tiling?

### Takeaway
Yes for GPU textures: a 7920×**17208** image exceeds the **16,384** max 2D texture dimension on Apple3–Apple9 / Mac2 GPUs (typical Apple Silicon Macs through M4-class), so a single `MTLTexture` is impossible and tiling (or Apple10’s 32K limit on newer GPUs) is required; CGImage itself has no well-documented fixed max size—memory and format encoders are the practical ceilings.

### Cited Findings
- Metal Feature Set Tables — Maximum 2D texture width and height: Apple2 = 8,192; Apple3–Apple9 = **16,384**; Apple10–Apple11 = **32,768**; Mac2 = **16,384** — [Metal Feature Set Tables PDF](https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf)
- Same limits summarized with device-family mapping and tiling strategies for panoramas above the cap — [Kyle Howells Metal texture tiling](https://ikyle.me/blog/2026/metal-texture-tiling)
- Historical AppKit note: layer-backed views constrained by max GL texture size; `CATiledLayer` exists specifically to work around that for large document views — [AppKit release notes](https://leopard-adc.pepas.com/releasenotes/Cocoa/AppKit.html)
- `kCGImageDestinationImageMaxPixelSize` is an **encoder option** to rescale output, not a statement of CGImage’s maximum dimensions — [kCGImageDestinationImageMaxPixelSize](https://apple-docs.everest.mt/docs/imageio/kcgimagedestinationimagemaxpixelsize/)
- Informal/old testing: some ImageIO destinations refused dimensions above ~65,500 for JPEG/TIFF; PNG/JPEG 2000 could go higher; not an official CGImage max — [BigPicture test notes](https://jizoh.jp/macpro/bigPicture.html); anecdotal CG failures past ~32767 in older discussions — [Stack Overflow](https://stackoverflow.com/questions/11529162/does-cgbitmapcontextcreate-have-a-size-limit)
- WWDC 2018: full-resolution decode memory scales with width×height×4 (approx.), causing fragmentation, compressor CPU, and jetsam risk—independent of GPU texture caps — [WWDC 2018 Session 219](https://nonstrict.eu/wwdcindex/wwdc2018/219/)

### Inferences
- For the stated target **7920×17208**: height **> 16384** ⇒ **must tile** for Metal on current mainstream Apple Silicon Mac GPUs; even if a future Apple10 Mac allows 32K textures, a full RGBA8 buffer is still ~0.5 GB+ and should be tiled for RAM.
- Tiling is forced by **two** independent limits: (1) MTLTexture max dimension, (2) decoded bitmap RAM / AppKit layer backing policy.

### Gaps
- Apple does not publish a single official “maximum CGImage width/height” for all macOS versions; treat CGImage limits as soft/memory/format-specific.
- Mapping of every M-series chip to exact `MTLGPUFamily` for 16K vs 32K should be verified at runtime with `supportsFamily` rather than assumed from marketing names alone.

---

## Which APIs work well from a non-AppKit-primary app (e.g. Rust calling Objective-C/Swift helpers, or a separate viewer process)?

### Takeaway
**ImageIO, Core Graphics, Metal, IOSurface, and Core Video are C/ObjC frameworks usable from Rust FFI or a helper process without AppKit**; **CATiledLayer, NSScrollView, PDFView, and SwiftUI/AppKit views require a GUI process**. The robust split is: helper decodes tiles → IOSurface/XPC → AppKit/Metal viewer composites.

### Cited Findings
- ImageIO / `CGImageSource*` APIs are C-based (Core Foundation) and documented for creating sources from URL/data, thumbnails, and incremental updates—no AppKit requirement — [Image I/O Guide](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/ImageIOGuide/imageio_source/ikpg_source.html)
- Metal, IOSurface, `CVPixelBuffer`, and `CVMetalTextureCache` are usable in non-UI code paths; Apple documents IOSurface sharing into XPC services that perform drawing while the main app displays — [Creating XPC Services](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html); [WWDC20 IOSurface/Metal](https://developer.apple.com/videos/play/wwdc2020/10090/)
- `IOSurfaceCreateXPCObject` / `IOSurfaceLookupFromXPCObject` are the supported primitives for passing surfaces across XPC; `MTLSharedTextureHandle` is oriented to `NSXPCConnection` / NSSecureCoding paths — [Creating XPC Services](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html); [Stack Overflow / russbishop on shared handles](https://stackoverflow.com/questions/53286138/how-do-you-use-mtlsharedtexturehandle-or-mtlsharedeventhandle-with-the-c-xpc-int)
- `CATiledLayer` is a QuartzCore `CALayer` subclass typically hosted by `NSView`/`UIView`; AppKit scroll tiling behavior is an AppKit feature — [CATiledLayer](https://developer.apple.com/documentation/quartzcore/catiledlayer); [AppKit tiled document views](https://leopard-adc.pepas.com/releasenotes/Cocoa/AppKit.html)
- Quick Look `QLThumbnailImageCreate` is a client C API (synchronous, background-thread recommended) returning `CGImageRef`—usable from a helper to produce preview bitmaps without AppKit UI — [QLThumbnailRequest Reference](https://leopard-adc.pepas.com/documentation/UserExperience/Reference/QLThumbnailRequest_Ref/Reference/reference.html)
- Cross-process display pattern: helper renders into IOSurface; app sets `CALayer.contents` to the surface or wraps it as `MTLTexture`—pixels need not be copied if synchronized — [Russ Bishop – Cross-process Rendering](http://russbishop.net/cross-process-rendering)

### Inferences
- **Works from Rust/FFI helper (no AppKit):** ImageIO downsample/subsample, CGImage tile extraction, Metal encode to IOSurface, Quick Look thumbnails, file I/O, custom tile caches on disk.
- **Requires AppKit/GUI process:** `NSScrollView`+`CATiledLayer` document views, `PDFView`, SwiftUI `Image`/`ScrollView`, windowing, eventing, accessibility.
- Recommended architecture for a Rust-primary app: Rust owns file/tile cache → thin ObjC/Swift helper for ImageIO/Metal → optional separate viewer process receiving IOSurfaces; first frame can be a Quick Look or ImageIO thumbnail while tiles fill in.

### Gaps
- No first-party Apple guide titled “embedding ImageIO in a Rust app”; FFI stability and bridging details are community/practice.
- Sandbox/hardened-runtime entitlement nuances for IOSurface + XPC between a non-App Store Rust binary and a viewer were not exhaustively researched here.
)
