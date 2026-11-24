# Video decoding backends for lightweight multi-window desktop playback

## What formats and codecs does each backend cover well (H.264, HEVC, AV1, VP9; containers MP4/MOV/MKV/WebM)?

### Takeaway
For a multi-format desktop player that must handle MP4, MOV, MKV (and ideally WebM), **libmpv / FFmpeg / GStreamer / libVLC** all cover the required codecs and containers via FFmpeg-class demuxers/decoders; **platform-native AVFoundation (macOS) and base Media Foundation (Windows)** are strong for MP4/MOV + H.264/HEVC but are weaker or incomplete for MKV/WebM/VP9/AV1 depending on OS version and installed codecs.

### Cited Findings
- mpv/libmpv is built on FFmpeg for demux/decode and is recommended as an embeddable playback backend; its client API mirrors CLI options (hwdec, vo, cache, properties, IPC). — [mpv stable manual — embedding](https://mpv.io/manual/stable/)
- Default mpv hardware-decode codec whitelist includes `h264,vc1,hevc,vp8,vp9,av1,prores,...` via `--hwdec-codecs`. — [mpv options.rst (hwdec-codecs)](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst); [mpv.io manual](https://mpv.io/manual/stable/)
- GStreamer documents container support for MP4/MOV (incl. fragmented MP4), Matroska/WebM, MPEG-PS/TS, MXF, Ogg; codecs include H.264, H.265, H.266, VP8, VP9, AV1 (via hardware plugins, dedicated libs, and FFmpeg/`gst-libav`). — [GStreamer Features](https://gstreamer.freedesktop.org/features/)
- VLC/libVLC playlists document broad playable codecs (H.264, HEVC via FFmpeg, VP*, MPEG-*, etc.) and containers including MOV, MP4, MKV, WebM/FLV-class formats (historical wiki table; VLC uses FFmpeg extensively). — [VLC Features Formats](https://wiki.videolan.org/VLC_Features_Formats)
- Apple AVFoundation is described by Apple as the framework for QuickTime movies and MPEG-4 files / HLS; WWDC 2017 materials state HEVC playback in MPEG-4 and QuickTime containers with hardware decode on supported CPUs. — [AVFoundation Overview](https://developer.apple.com/av-foundation/); [WWDC17 HEIF/HEVC PDF](https://devstreaming-cdn.apple.com/videos/wwdc/2017/511tj33587vdhds/511/511_working_with_heif_and_hevc.pdf)
- Independent format matrices and app CI notes state macOS AVFoundation reads MP4/MOV/M4V/3GP but **not MKV or WebM**; codecs H.264/HEVC; AV1 only where hardware exists (e.g. M3+); not VP8/VP9. — [native_media_compressor supported formats](https://github.com/StarProxima/native_media_compressor); secondary corroboration [Mac supported formats blog](https://www.hazenstudio.com/maclc/blog/video-formats-mac-supports/)
- Microsoft Media Foundation natively lists containers including MPEG-4 (`.mp4`, `.mov`, `.m4v`), AVI, ASF; video codecs include H.264, MPEG-4 Part 2, MJPEG, WMV (base table). — [Supported Media Formats in Media Foundation](https://learn.microsoft.com/en-us/windows/win32/medfound/supported-media-formats-in-media-foundation)
- Separate Microsoft docs add **MKV** support with codecs including H.264, HEVC, VP8, VP9, AV1 (among others), with caveats (first A/V track preferred; limited chapter/menu/font features). — [Matroska (MKV) support](https://learn.microsoft.com/en-us/windows/win32/medfound/mkv-support)
- FFmpeg + GStreamer comparison: FFmpeg/`libav*` is the usual file-oriented demux/decode stack; GStreamer is pipeline-oriented and often uses `gst-libav` for FFmpeg codecs — both can decode the same common formats when plugins are present. — [Murat Demirci — FFmpeg and GStreamer](https://muratdemirci.com.tr/en/ffmpeg-gstreamer/)

### Inferences
- For **must-play MKV + WebM + MP4/MOV** with consistent cross-platform behavior, prefer **libmpv (FFmpeg)** or **GStreamer playbin3** or **libVLC**, not bare AVFoundation.
- On Windows-only, Media Foundation + MKV support may be “good enough” for many files, but extension/codec availability still varies by Windows SKU and installed codecs — a FFmpeg-backed stack remains more predictable for obscure remuxes.
- Platform-native APIs remain useful as *optional* zero-copy HW paths for MP4/MOV H.264/HEVC on Apple Silicon / modern Windows GPUs, with FFmpeg fallback for everything else.

### Gaps
- No single official 2024–2026 matrix was found that scores libmpv vs GStreamer vs libVLC on *every* obscure FourCC with measured pass rates.
- WebM-on-Media-Foundation coverage was not clearly documented in the same depth as MKV; treat WebM-on-MF as unverified without runtime probing.
- Exact AV1 hardware decode availability matrix on Intel/AMD Windows GPUs via MF was not fully enumerated in sources retrieved.


## How does hardware decode (VideoToolbox, D3D11VA, VAAPI, NVDEC) reduce CPU/RAM vs software decode, and what are concurrent HW decode session limits?

### Takeaway
Hardware decode primarily **cuts CPU** by offloading decode to fixed-function media engines; **RAM savings are real only if frames stay on the GPU** (zero-copy to the compositor). Concurrent sessions are generally **not a fixed OS constant** — they are bounded by media-engine throughput, VRAM/unified memory for surface pools, and PCIe bandwidth if frames are copied back to system RAM.

### Cited Findings
- mpv docs: with modern hardware, `hwdec` offers reduced CPU usage and possibly lower power; not enabled by default for reliability; actively supported APIs include `d3d11va`, `videotoolbox`, `vaapi`, `nvdec`, plus `-copy` variants that copy frames back to system RAM. — [mpv options.rst — hwdec](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst)
- FFmpeg hardware-accel documentation (reproduced in mailing-list archive of ffmpeg docs) states most hwaccel methods are intended for playback and **will not be faster than software on modern CPUs**, and that ffmpeg usually copies decoded frames from GPU to system memory, causing further performance loss — mainly useful for testing unless the pipeline stays on-GPU. — [ffmpeg_doc_hw_accel.txt (Cinelerra archive of FFmpeg docs)](https://lists.cinelerra-gg.org/archives/list/cin@lists.cinelerra-gg.org/message/FOXPJL7YQ76V7JPA3UDFXKL3ARZRH7AH/attachment/2/ffmpeg_doc_hw_accel.txt)
- FFmpeg `AV_HWACCEL_FLAG_UNSAFE_OUTPUT` docs note NVDEC has a **hard limit on decoder surfaces** that cannot always be guessed ahead of time; default path makes an on-device copy for safety. — [FFmpeg AVHWAccel docs](https://ffmpeg.org/doxygen/trunk/group__lavc__hwaccel.html)
- NVIDIA NVDEC Application Note (Video Codec SDK 13.0): NVDEC supports **multiple hardware decoding contexts with negligible context-switch penalty**; capacity limited by hardware performance and available memory; GeForce products have **a single NVDEC engine**; Quadro/Tesla may have 2–3; indicative per-engine 1080p decode rates (e.g. Ada ~903 fps H.264, ~1641 HEVC, ~1018 AV1). — [NVDEC Application Note](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvdec-application-note/index.html)
- NVIDIA FFmpeg guide: concurrent decode of multiple inputs in one process is supported; separate thread per decode with a **shared CUDA context**. — [FFmpeg with NVIDIA GPU Hardware Acceleration](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/ffmpeg-with-nvidia-gpu/)
- Practical Windows multi-stream note: GPU decode reduces CPU but there **may be a limit on simultaneous sessions**; raw 1080p50 transfer after decode can need ~1.5 Gb/s PCIe; 5 streams ~7.5 Gb/s — PCIe copy can erase gains. — [SRT MiniServer — Using GPU for decoding](https://help.srtminiserver.com/using_gpu_for_decoding.html)
- Measured zero-copy vs copy path (D3D11VA / RTX 3060): GPU→CPU hwdownload ~3.2 ms/frame vs ~0.04–0.06 ms import when sampling decoder surfaces directly; multi-layer 1080p60 noted as ~0.5–1.1 GB/s PCIe traffic in the copy path. — [Wabcut — Zero-copy decode](https://wabcut.zyphite.com/developer/zero-copy)
- VAAPI (FFmpeg-user, Mark Thompson): parallel streams are supported; prefer **one hwdevice shared**, **one vaapi_context per decoder**, separate fixed-size surface pools per decoder (pools can be exhausted if too small). — [FFmpeg-user VAAPI parallel streams](https://ffmpeg.org/pipermail/ffmpeg-user/2016-December/034748.html)
- Apple `VTDecompressionSession`: one session decompresses one sequence of frames; create/invalidate per stream; no published hard max session count in Apple’s API overview. — [VTDecompressionSession](https://developer.apple.com/documentation/videotoolbox/vtdecompressionsession)
- WWDC20: Video Toolbox decoders run out-of-process; hardware decode enabled by default where supported; async decode recommended; blocking in output callback causes back-pressure. — [WWDC20 Decode ProRes with AVFoundation and VideoToolbox](https://developer.apple.com/videos/play/wwdc2020/10090/)
- 2026 MacCompute guide (encode-focused, but same VT media engines): Apple does not publish a simple “N sessions per chip”; illustrative M4 16GB concurrency for VT encode jobs ~2–3×1080p or 1×4K — useful as order-of-magnitude for media-engine contention, not a decode guarantee. — [MacCompute VideoToolbox parallel matrix](https://maccompute.com/blog/articles/2026-remote-mac-m4-videotoolbox-transcode-parallel-matrix.html)
- GStreamer playback tutorial: HW decode can be essential for 1080p/4K on low-power CPUs; leave frames in GPU memory for display; playbin3 selects by element rank (`GST_PLUGIN_FEATURE_RANK`). — [GStreamer Playback tutorial 8](https://gstreamer.freedesktop.org/documentation/tutorials/playback/hardware-accelerated-video-decoding.html)
- Microsoft documents D3D11 video decoder creation and MF↔D3D11 integration but **does not publish a fixed simultaneous-session ceiling**; limits are driver/GPU specific. — [ID3D11VideoDevice::CreateVideoDecoder](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11videodevice-createvideodecoder); [Supporting D3D11 Video Decoding in MF](https://learn.microsoft.com/en-us/windows/win32/medfound/supporting-direct3d-11-video-decoding-in-media-foundation)

### Inferences
- For many simultaneous *playback* windows, **zero-copy HW decode + GPU VO** is the main CPU/RAM win; `-copy` / download-to-RAM paths often cost more system RAM and PCIe than pure software at modest resolutions.
- Concurrent capacity planning heuristic: budget by **aggregate decode fps × resolution** against NVDEC/media-engine tables and by **surface-pool VRAM**, not by a magic “max sessions = N” from Apple/Microsoft docs.
- Cap concurrent HW sessions in the app (pause background windows, throttle inactive PiPs, fall back to software after N failures) because OS APIs will not enforce a friendly global limit.

### Gaps
- Apple does not document a numeric concurrent `VTDecompressionSession` limit for decode; public numbers for 2024–2026 are mostly encode or anecdotal.
- No authoritative Microsoft “max D3D11VA sessions” constant was found.
- Few peer-reviewed RAM-delta numbers (MB saved per stream) comparing SW vs HW for identical 1080p/4K clips across VideoToolbox/D3D11VA/VAAPI in 2024–2026.


## Is libmpv preferable to raw FFmpeg for a player UI (IPC, vo, hwdec, cache settings for low resource use)?

### Takeaway
**Yes for a desktop multi-window player UI**: libmpv already integrates demux, decode, audio, timing, hardware decode selection, video output, cache controls, properties, and IPC — raw FFmpeg/`libav*` forces you to rebuild that player stack yourself.

### Cited Findings
- mpv manual: for embedding as a playback backend in a different application, **libmpv is generally recommended**; C API is very close to CLI/scripting; can be configured like the CLI player. Alternatives listed: spawn mpv with IPC (`--input-ipc-server`) or scripts. — [mpv.io manual](https://mpv.io/manual/stable/)
- libmpv exposes the same option surface used for low-resource tuning: `hwdec`, `vo` (`gpu`/`gpu-next`/`libmpv`), demuxer cache (`demuxer-max-bytes`, `demuxer-max-back-bytes`, `cache`), `vd-lavc-threads`, `hwdec-codecs`, etc. — [mpv options.rst](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst)
- Example `mpv.conf` documents that large RAM caches (`cache=yes`, `demuxer-max-bytes=500M`) and disk cache are optional and can be disabled or bounded — relevant for multi-instance. — [etc/mpv.conf](https://github.com/mpv-player/mpv/blob/master/etc/mpv.conf)
- FFmpeg/`libav*` is framed as a multimedia processing framework (CLI + C API) optimized for demux/transcode/filter graphs, not as a complete player with VO/A-V sync/OSD. — [Murat Demirci — FFmpeg and GStreamer](https://muratdemirci.com.tr/en/ffmpeg-gstreamer/)
- GStreamer `playbin`/`playbin3` is the high-level “just play” alternative if the app prefers pipeline APIs over libmpv’s player model. — [GStreamer Playback Components](https://gstreamer.freedesktop.org/documentation/application-development/highlevel/playback-components.html)
- Real-world multi-window libmpv usage exists (Qt “New Window” multi-instance RTSP issues; users report many instances); mpv maintainers indicate instance limiting is an **application concern**, not a built-in mpv feature (issue #13264, 2024). — [mpv#13264](https://github.com/mpv-player/mpv/issues/13264); [mpv#8831](https://github.com/mpv-player/mpv/issues/8831)
- Historical libmpv leak reports when rapidly creating/destroying handles with `hwdec=d3d11va` (claimed fixed for d3d11va; separate destroy leaks discussed) and Lua-related ~1.5 MB leak per create/destroy cycle — relevant if windows are opened/closed often. — [mpv#11003](https://github.com/mpv-player/mpv/issues/11003); [mpv#14451](https://github.com/mpv-player/mpv/issues/14451)

### Inferences
- Prefer **libmpv** over raw FFmpeg when the product is a *player* (multi-window UI, seek, pause, A/V sync, subtitles, GPU present).
- Prefer raw FFmpeg (or GStreamer) only if you need custom processing graphs beyond playback, or must minimize LGPL surface carefully — knowing you own VO, sync, and resource policy.
- Cap/reuse `mpv_handle` lifetimes; disable Lua in libmpv builds if create/destroy churn is high; test hwdec create/destroy loops on Windows.

### Gaps
- No recent official benchmark comparing “libmpv embed RSS” vs “minimal FFmpeg decode + custom OpenGL present” for N simultaneous windows with identical content.
- libVLC vs libmpv embed size/CPU head-to-head numbers for 2024–2026 multi-window desktop apps were not found in primary sources.


## Memory and CPU characteristics of multiple simultaneous decode pipelines vs one shared pipeline with multiple render surfaces

### Takeaway
**Independent streams require independent decoders** (one decode session per bitstream). Sharing is limited to **GPU device/context**, optional compositor, and *displaying the same decoded frames on multiple surfaces*. One shared decode pipeline cannot feed different videos; attempting to share surface pools incorrectly risks exhaustion and stalls.

### Cited Findings
- NVIDIA NVDECODE programming guide: pipeline is demux → parse → **decoder instance(s)**; create decoder instance(s) per stream; decode surfaces (`ulNumDecodeSurfaces`) trade pipelining vs GPU memory; map to output surfaces for render. — [NVDEC Video Decoder API Programming Guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/nvdec-video-decoder-api-prog-guide/)
- VAAPI expert guidance: one device shared across streams; **exactly one vaapi_context per decoder**; easiest pattern is one hwframe context **per decoder output**; shared pools must be large enough because decoders hold reference frames. — [FFmpeg-user VAAPI parallel](https://ffmpeg.org/pipermail/ffmpeg-user/2016-December/034748.html)
- GStreamer `decodebin3` keeps typically **one decoder per stream type** within a *single* media and reuses when switching tracks; separate playbin/pipeline instances are still needed for separate files/windows. Multiqueue grows per-stream to avoid starvation → memory rises with active streams. — [decodebin3](https://gstreamer.freedesktop.org/documentation/playback/decodebin3.html); [Decodebin design](https://gstreamer.freedesktop.org/documentation/additional/design/decodebin.html)
- mpv demuxer cache is **per player instance**: `demuxer-max-bytes` / `demuxer-max-back-bytes` bound forward/back buffers; defaults historically large enough that users reported ~400MB demuxer cache; multi-instance multiplies that. — [mpv#5359](https://github.com/mpv-player/mpv/issues/5359); [options.rst demuxer-max-bytes](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst)
- Community tuning note (enhanced mpv build, but formula is general): peak RSS ≈ forward + back demuxer caps + prefetch + decoder/VO overhead — e.g. 512+512 MiB demuxer alone before decode. — [TUNING.md (JJenkx)](https://github.com/JJenkx/mpv-atmos-patched/blob/main/TUNING.md)
- Jetson multi-decode sample: demonstrates parallel decode of multiple streams (up to 128) with separate decoder paths — architectural confirmation that multi-video means multi-decoder. — [Jetson 14_multivideo_decode](https://docs.nvidia.com/jetson/l4t-multimedia/l4t_mm_14_multivideo_decode.html)
- PiP platform guidance (non-desktop but illustrative): PiP changes the **render surface**, not a second decoder for the same content. — [Huawei PiP NDK guide](https://developer.huawei.com/consumer/en/doc/doccenter-capabilities/pipwindow-native)

### Inferences
- Architecture for multi-window *different* videos: **N demux+decode pipelines**, shared GPU device where possible, shared app compositor; aggressively pause/decode-throttle invisible windows.
- Architecture for multi-window *same* video (true shared pipeline): **1 decode**, N presenters/textures — large savings; only applies when content is identical.
- libmpv cannot magically share one decode across unrelated files; resource capping must be app-level (max active players, smaller demuxer caps, pause off-screen).

### Gaps
- No published desktop study with RAM/CPU tables for “N× libmpv” vs “1 decode + N OpenGL views of same file” under controlled 1080p/4K loads in 2024–2026.
- Precise macOS unified-memory cost per concurrent VT decode surface pool remains undocumented publicly.


## Recommended settings/profiles for low-RAM multi-instance playback (cache, threads, hwdec, scale)

### Takeaway
Use **small per-instance demuxer caches**, **hwdec without copy when VO can consume GPU frames**, **bounded decode threads**, downscale early for tiny windows, and an **application-level concurrent-player cap** — mpv will not limit instances for you.

### Cited Findings
- Bound demuxer RAM: set both `demuxer-max-bytes` and `demuxer-max-back-bytes` (users recommended e.g. 40 MiB each when defaults ~400 MiB caused high RAM); prefer `demuxer-readahead-secs` for time-based control. — [mpv#5359](https://github.com/mpv-player/mpv/issues/5359); [options.rst](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst)
- Sample conf shows optional large caches (`demuxer-max-bytes=500M`) and `cache-on-disk` — for multi-window local files, leave large RAM cache **off**. — [etc/mpv.conf](https://github.com/mpv-player/mpv/blob/master/etc/mpv.conf)
- `hwdec=no` default; for production try `hwdec=auto` / platform-specific `videotoolbox`, `d3d11va`, `vaapi`, `nvdec`; avoid `auto-unsafe` unless debugging; `-copy` modes increase system RAM use. — [options.rst hwdec](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst)
- `vd-lavc-threads`: default 0 = autodect up to 16 cores — for many windows, **explicitly lower** (e.g. 1–2) to prevent N×core thread storms on software decode. — [options.rst vd-lavc-threads](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst)
- `hwdec-threads` default 4; can be tuned; `hwdec-software-fallback` default 3 failed frames. — [options.rst](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst)
- Instance count: maintainers advise wrappers / app logic (e.g. umpv-style single-instance patterns); no built-in `instance-limit`. — [mpv#13264](https://github.com/mpv-player/mpv/issues/13264)
- GStreamer: control HW vs SW via plugin rank; keep frames on GPU for present; network buffering should be handled upstream of decodebin3. — [GStreamer HW decode tutorial](https://gstreamer.freedesktop.org/documentation/tutorials/playback/hardware-accelerated-video-decoding.html); [decodebin3](https://gstreamer.freedesktop.org/documentation/playback/decodebin3.html)
- Zero-copy present avoids multi-ms/frame download costs that dominate multi-window CPU/bus load. — [Wabcut zero-copy](https://wabcut.zyphite.com/developer/zero-copy)

### Inferences
- Practical libmpv multi-window profile (local files, low RAM):
  - `hwdec=auto` (or `videotoolbox` / `d3d11va` / `vaapi`) with `vo=gpu`/`gpu-next`/`libmpv` matching embed path
  - `demuxer-max-bytes=16M`–`64M`, `demuxer-max-back-bytes=8M`–`32M`, `cache=no` for local media
  - `vd-lavc-threads=2` (or 1) when SW fallback is likely
  - Disable unused tracks / don’t cache unselected audio/subs
  - Scale display size down for small PiP windows; pause decode when window is occluded
  - Hard-cap active players (e.g. 4–8 full decode; others paused or poster-frame)
- Prefer **disk cache only** for slow network inputs, not for every local window.

### Gaps
- Upstream mpv does not publish an official “multi-instance low memory” profile; values above are synthesized from option semantics + issue reports.
- Optimal `swapchain-depth` / GPU context sharing settings for N embedded `vo=libmpv` windows lack recent authoritative guidance.


## Licensing implications (LGPL FFmpeg, GPL mpv builds, etc.) for a shipped app

### Takeaway
You can ship a proprietary desktop app on **LGPL libmpv + LGPL FFmpeg** (dynamic link, notices, source offer, replaceable libs), but **stock GPL mpv/FFmpeg builds** (with `--enable-gpl` / GPL-only modules) are unsuitable for closed-source distribution; **libVLC is LGPL** with similar caveats on GPL plugins; **codec patents** (e.g. MPEG LA) are separate from copyright licenses.

### Cited Findings
- FFmpeg is LGPL 2.1+ by default; optional GPL parts make all of FFmpeg GPL if enabled; official compliance checklist: no `--enable-gpl`/`--enable-nonfree`, dynamic linking, distribute corresponding source, about-box/EULA notices, no reverse-engineering ban, don’t rename DLLs to obscure names, don’t use GPL libs like libx264. — [FFmpeg License and Legal Considerations](https://www.ffmpeg.org/legal.html)
- FFmpeg patent mini-FAQ: commercial products using patented codecs have historically attracted MPEG LA licensing attention — copyright compliance ≠ patent safety. — [FFmpeg legal.html](https://www.ffmpeg.org/legal.html)
- mpv Copyright: whole player GPL by default; **LGPL mode** via `-Dgpl=false` excluding GPL-only files; intended especially for libmpv; linked libraries (e.g. GPL FFmpeg) still affect final license; LGPL mode disables some features (e.g. certain Linux VOs, DVD, vdpau VO). — [mpv Copyright](https://github.com/mpv-player/mpv/blob/master/Copyright)
- Official mpv Windows releases ship both **GPL** (`--enable-gpl --enable-version3` FFmpeg) and **LGPL** (`-Dgpl=false` + LGPL FFmpeg) `libmpv` builds explicitly “for software that cannot link GPL.” — [mpv Releases](https://github.com/mpv-player/mpv/releases)
- libVLC / engine relicensed to LGPLv2.1+; VLC app remains GPL; VideoLAN warns some plugins are more restrictive (e.g. GPLv3); wiki advises dynamic linking and preferring LGPL plugins for proprietary apps. — [VideoLAN LGPL press](https://www.videolan.org/press/lgpl-libvlc.html); [libVLC docs](https://videolan.videolan.me/vlc/master/libvlc.html); [LibVLC wiki](https://wiki.videolan.org/LibVLC); [videolan/vlc README](https://github.com/videolan/vlc?tab=readme-ov-file)
- Wikipedia player comparison notes mpv as GPL-2.0-or-later with parts LGPL-2.1-or-later; VLC similarly dual. — [Comparison of video player software](https://en.wikipedia.org/wiki/Comparison_of_video_player_software)

### Inferences
- Shipping recommendation for a proprietary multi-window player: **dynamically link LGPL libmpv built with `-Dgpl=false` against LGPL FFmpeg**; document configure flags; host matching sources; keep GPL plugins out of the process.
- Platform AVFoundation / Media Foundation avoid FFmpeg copyright issues for supported formats but **do not remove codec patent questions** and fail the MKV/WebM product requirement on macOS without a secondary stack.
- GStreamer plugin set must be audited per-plugin license (LGPL core vs GPL plugins) similarly to VLC modules.

### Gaps
- This research is not legal advice; jurisdiction-specific patent royalty obligations for H.264/HEVC were not quantified.
- Exact feature delta between current (2025–2026) GPL vs LGPL libmpv Windows/macOS binaries beyond the Copyright file list was not re-verified against the latest release notes line-by-line.


## Synthesis for stack selection (for the report writer)

### Takeaway
**Best default for this product brief: libmpv (LGPL build) + platform hwdec + strict per-window cache/thread caps + app-level concurrency governor.** Use AVFoundation/MF only as optional native paths or for OS-integration features; use GStreamer if the team already standardizes on pipelines; use libVLC if VLC’s module ecosystem outweighs mpv’s lighter player model.

### Cited Findings
- libmpv is the documented embed path for “completely different application” players with full option parity (hwdec/vo/cache). — [mpv.io manual](https://mpv.io/manual/stable/)
- Format coverage for MKV/WebM requires FFmpeg-class stacks on macOS; AVFoundation alone is insufficient. — [AVFoundation Overview](https://developer.apple.com/av-foundation/); [native_media_compressor formats](https://github.com/StarProxima/native_media_compressor)
- Multi-instance memory scales with per-handle demuxer caps; defaults are too large for many windows. — [mpv#5359](https://github.com/mpv-player/mpv/issues/5359)
- HW decode wins when zero-copy; concurrent sessions are throughput/memory limited (NVDEC documented; Apple/MS undocumented numeric caps). — [NVDEC Application Note](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvdec-application-note/index.html); [VTDecompressionSession](https://developer.apple.com/documentation/videotoolbox/vtdecompressionsession)
- LGPL shipping path exists for both libmpv and FFmpeg if built correctly. — [mpv Releases](https://github.com/mpv-player/mpv/releases); [FFmpeg legal](https://www.ffmpeg.org/legal.html)

### Inferences
- Priority order for “many windows, low RAM/CPU, many formats, desktop”: **libmpv LGPL > GStreamer playbin3 > libVLC LGPL plugins > raw FFmpeg player DIY > platform-native-only**.
- Resource sharing strategy: share GPU device/context and compositor; never expect one decoder to serve different files; pause inactive windows; prefer displaying one decode on multiple surfaces only for duplicate views.

### Gaps
- Final product choice still needs a short bake-off measuring RSS/CPU for 1/4/8/16 windows on target Mac/Windows GPUs with the candidate profiles — not found as an existing public 2024–2026 dataset matching this exact brief.
