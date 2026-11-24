# Reference apps: RAM/CPU, architecture, multi-window lessons

Research date context: landscape through late 2025–2026. Labels: **measured** = published with methodology; **reported** = user/forum observation; **vendor claim** = product marketing/docs; **estimate** = derived from decode throughput / VRAM rules of thumb.

## Which players are known for being light while supporting many formats?

### Takeaway
mpv (and thin libmpv frontends) are the dominant “light + many formats” choice; VLC and PotPlayer stay competitive when hardware decode is on; heavy UI shells (GTK Celluloid, Plex desktop, Electron-style apps) add measurable memory/CPU overhead on top of the same decode work.

### Cited Findings
- IINA markets itself as a modern macOS player **built on mpv**, with PiP, Live Text, and native macOS UI — [IINA](https://iina.io/)
- Vivi’s Oct–Nov 2026 Linux bench (Arch, Hyprland, RTX 3060 Ti; main-process PSS; local 1080p H.264) reported **measured** idle/playing memory: VLC ~58 / ~277 MB, mpv ~153 / ~361 MB, Vivi ~117 / ~286 MB, Celluloid ~298 / ~570 MB; CPU while playing: VLC 4.4%, Vivi 5.5%, mpv 16.7%, Celluloid 32% — with the important caveat that Vivi/VLC used GPU decode while mpv/Celluloid used software defaults — [Vivi compare](https://viviplayer.app/compare.html)
- Same Vivi methodology page documents UI/engine stacks: IINA = native macOS (Swift) + libmpv; Celluloid = GTK + libmpv; VLC = Qt + libVLC; MPC-HC = Win32/MFC + DirectShow/LAV; mpv = OSC only + mpv engine — [Vivi compare](https://viviplayer.app/compare.html)
- Guru Software’s M2 Max **measured** 8K HEVC remux test (Spears & Munsil, 7680×4320): mpv 10.2% CPU / 1.8 GB RAM / 16.9 W; IINA 12.3% / 2.1 GB / 18.4 W; VLC 16.1% / 3.2 GB / 24.3 W; Elmedia 13.9% / 2.6 GB / 27.6 W — [Guru Software Mac players 2025](https://www.gurusoftware.com/finding-the-ultimate-video-player-for-mac-in-2023/)
- A Linux user RAM comparison (Calculate Linux, 1080p VP9 WebM, btop) ranked roughly: Parole/MPlayer ~92–95 MB → Totem ~160–170 → SMPlayer+MPlayer ~190+ → mpv ~192–260 → VLC ~240–270 → Celluloid/QMPlay2 ~300–350 MB (**reported**, different days, not controlled HW decode) — [Pingvinus Linux players compare](https://pingvinus.ru/note/video-players-compare)
- Celluloid documents itself as a **GTK+ frontend for mpv** using the libmpv client API — [celluloid-player/celluloid](https://github.com/celluloid-player/celluloid/)
- mpv.net documents itself as a Windows player **based on libmpv**, deliberately not DirectShow/Media Foundation — [mpvnet-player/mpv.net](https://github.com/mpvnet-player/mpv.net/)
- PotPlayer is repeatedly described as a lightweight Windows player with built-in codecs and DXVA/CUDA/QuickSync; Neowin notes a 2026 build defaulting to D3D11 video / WASAPI audio — [Neowin PotPlayer 261021](https://www.neowin.net/software/potplayer-261021/); [BleepingComputer PotPlayer](https://www.bleepingcomputer.com/download/potplayer/)
- An AnyMP4 hands-on (**measured** on i9-14900K + RTX 4070 Ti Super) reported PotPlayer 4K HEVC 10-bit @ 120 Mbps: HW decode ~3.8% CPU average / 218 MB RAM vs software ~22.4% CPU; VLC 3.0.24 used 268 MB on the same file — [AnyMP4 PotPlayer review](https://www.anymp4.com/player/potplayer-review.html)
- A 2019 Celluloid Wayland bug report (**reported**) showed ~30% CPU for Celluloid vs ~10% for bare mpv with VAAPI on the same clip; without VAAPI ~85% vs ~60% — attributed partly to per-frame DMA-buf setup — [celluloid#494](https://github.com/celluloid-player/celluloid/issues/494)

### Inferences
- **Copy:** libmpv (or equivalently thin decode engine) + minimal windowing; prefer native toolkit shells over Electron for N-window walls.
- **Avoid:** assuming “mpv-based” equals “as light as mpv” — GTK/Qt/Swift UI can add tens–hundreds of MB and fixed CPU for frame scheduling/composition.
- Format breadth in practice comes from FFmpeg/libavcodec (mpv family) or bundled codec stacks (PotPlayer, VLC), not from the GUI.

### Gaps
- No single controlled 2025–2026 cross-platform bench covering IINA, PotPlayer, MPV.net, and VLC with identical HW decode settings and PSS/RSS methodology.
- PotPlayer’s closed-source internals (exact filter graph / process model) lack a primary technical architecture whitepaper comparable to mpv/VLC docs.

## How do users run multiple videos? Multi-instance vs built-in multi-window; memory for N concurrent plays?

### Takeaway
Mainstream players mostly rely on **multiple instances / multiple player cores**, not a first-class “N floating windows” product feature; the best quantitative multi-stream guidance comes from GridPlayer’s process model and GPU NVDEC throughput, not from mpv/VLC marketing.

### Cited Findings
- Reddit r/mpv (Jun 2025): multiple instances are the **default** — launch each video separately; one commenter reported six concurrent mpv instances on a 2014 i7-4790T (**reported**, no RAM figures) — [r/mpv multiple instances](https://www.reddit.com/r/mpv/comments/1leoooo/how_to_run_multiple_instances/)
- mpv manual: `T` toggles stay-on-top; `--ontop` keeps the window above others — [mpv manual](https://mpv.io/manual/stable/)
- IINA’s `PlayerCore` maintains a static `playerCores` array and creates a new core (new mpv + windows) per instance — multi-window is multi-core, not one decoder multiplexing many views — [iina PlayerCore.swift](https://github.com/lhc70000/iina/blob/develop/iina/PlayerCore.swift); architecture summary — [DeepWiki IINA core player](https://deepwiki.com/iina/iina/2-core-player)
- PotPlayer Preferences expose explicit multi-instance modes: allow more than one instance vs force files into an existing instance/playlist — [Cyber Raiden PotPlayer settings](https://cyberraidentech.blogspot.com/2020/12/potplayer-general-and-playback-settings.html)
- GridPlayer (Python/Qt; MPV or VLC backends) is designed to play many videos in one grid; VLC path defaults to **multi-process, 4 videos per process** because many libVLC instances in one process caused CPU spikes; single-process uses a little less memory but is slower and crash-coupled — [gridplayer README](https://github.com/vzhd1701/gridplayer/); author explanation — [gridplayer#15](https://github.com/vzhd1701/gridplayer/issues/15)
- GridPlayer docs: MPV always single-process; on macOS, VLC hardware mode is forced single-process due to OS inter-process rendering restrictions — [gridplayer README](https://github.com/vzhd1701/gridplayer/)
- LinuxLinks observation (**reported**): Intel NUC 13 Iris Xe played 4 GridPlayer streams easily; 9 streams raised Render/3D ~40% and Video ~24% with “flawless” playback — [LinuxLinks GridPlayer](https://www.linuxlinks.com/gridplayer-play-multiple-videos-simultaneously/2/)
- NVIDIA NVDEC Application Note (Video Codec SDK 13.0): multiple HW decode contexts are supported with negligible switch penalty; limit is hardware performance + memory; Ampere H.264 ~748 fps @ 1080p YUV420 **per NVDEC engine**; GeForce products have a single NVDEC — [NVDEC Application Note](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvdec-application-note/index.html)
- Independent NVDEC capacity study on RTX 3090 (**measured**): ~768 fps aggregate at 1080p H.264 → ~25 cameras @ 30 fps before decoder saturation; 32 concurrent sessions ran (no hard session cap on decode) — [sadbodhs NVDEC capacity](https://sadbodhs.github.io/computer_vision_optimization/nvdec/)
- NVIDIA forum guidance (**vendor**): no fixed decoder count limit; limited by system resources; example: a decoder that does one 4K@60 can be expected to do ~4× 1080p@60 — [NVIDIA forum simultaneous decoders](https://forums.developer.nvidia.com/t/number-of-simultaneous-video-decoders/47819/3)
- Forum VRAM note (**reported**): one ffmpeg `h264_cuvid` session allocated ~87–153 MB VRAM depending on GPU/OS; sharing one CUDA context across sessions reduces overhead vs many processes — [NVIDIA forum VRAM allocation](https://forums.developer.nvidia.com/t/nvdec-nvenc-vram-allocation-differences-between-different-gpus/65151)
- Chromium historically capped concurrent VA-API decoders (experiment with 32 videos, soft FD limit) — browser multi-video is FD/GPU-process constrained, not just CPU — [chromium commit fa9b67b](https://github.com/chromium/chromium/commit/fa9b67b0569eed66a7455b462bcec887ea217e75)
- Dolby/THEOplayer FAQ: multiple simultaneous players are possible if bandwidth/RAM allow; mute extras; pause off-screen — [Dolby OptiView FAQ](https://optiview.dolby.com/docs/theoplayer/faq/possible-to-have-multiple-player-instances-at-same-time/)

### Inferences
- For a lightweight multi-window player: **prefer one process with N libmpv/render surfaces** only if composition is cheap; otherwise **shard decode across processes** (GridPlayer’s VLC lesson) when the toolkit/GPU path spikes.
- Per-stream system RAM for a thin player often lands ~200–400 MB playing (Vivi/mpv/VLC single-stream benches); N instances scale roughly linearly unless decoder/context is shared — **estimate**, not a published N-instance series.
- GPU decode capacity for 1080p30 is usually tens of streams on modern discrete GPUs; the practical desktop limit for “many windows” is often **compositor + window buffers + app overhead**, not NVDEC fps.

### Gaps
- No reliable published table of RSS/PSS for N=1/4/8 concurrent **mpv** or **IINA** 1080p HW-decoded windows on the same machine.
- Apple VideoToolbox / AMD VCN concurrent-stream memory numbers for desktop playback apps were not found in this pass.

## Niche tools for many floating video windows (security, esports, clip boards)?

### Takeaway
Two patterns dominate: (1) **fixed mosaic / video wall** inside one app (Happytime Media Client, StreamGrid, MultiViewer, GridPlayer); (2) **independent frameless always-on-top windows** per camera (Nova, fGadget Cam Monitor). Esports/racing tools optimize sync and DRM more than RAM.

### Cited Findings
- **fGadget Cam Monitor** (Windows): up to 8 frameless RTSP/ONVIF windows, move/resize, always-on-top, remembered positions across monitors — [fGadget Cam Monitor](https://futomi.app/projects/fgadget-cam-monitor/)
- **Nova**: one independent frameless window per RTSP/ONVIF camera; drag/snap layouts; `nova layout save/start` to restore wall — [ANDYwyLEE/nova](https://github.com/ANDYwyLEE/nova)
- **Happytime Media Client**: desktop multi-window surveillance/playback; grids 1/4/6/9/16; multi-monitor video wall; RTSP/RTMP/HTTP-FLV/SRT/local — [Happytime Media Client](https://www.happytimesoft.com/products/media-client/); [user manual](https://www.happytimesoft.com/support/media-client/index.html)
- **StreamGrid**: Electron + React + TypeScript multi-stream command center; 2–20+ feeds; YouTube/Twitch/RTSP/HLS/DASH; RTSP via FFmpeg→HLS (~2–3 s latency) — [LordKnish/StreamGrid](https://github.com/lordknish/streamgrid/)
- **GridPlayer**: local files + streams in a configurable grid; MPV/VLC; hardware decode recommended — [vzhd1701/gridplayer](https://github.com/vzhd1701/gridplayer/)
- **MultiViewer**: multi-view F1/motorsport streams with sync, setups, DRM/Widevine player; docs warn live timing + multiple streams need significant CPU/GPU; even high-end GPUs may struggle — [MultiViewer](https://multiviewer.app/); [requirements](https://multiviewer.app/docs/getting-started/requirements)
- MultiViewer download sizes ~180–240 MB depending on OS (**vendor packaging size**, not RAM) — [MultiViewer download](https://multiviewer.app/download)

### Inferences
- **Copy from security tools:** frameless AOT windows, persist geometry per display, independent mute, layout presets.
- **Copy from MultiViewer/GridPlayer:** explicit sync model and “setups”; do not pretend independent decoders stay frame-locked without offsets.
- **Avoid for light RAM goals:** Electron multi-stream shells (StreamGrid pattern) unless web embeds are mandatory; prefer native + libmpv/libVLC.
- Clip-board / editing multi-preview tools were not a strong separate category in desktop search results versus grids and camera walls.

### Gaps
- Few published RAM/CPU figures for Nova, fGadget, Happytime, or MultiViewer at N=4/8 streams.
- No clear “clip board” desktop product with quantitative multi-preview memory data in this survey.

## What do power users recommend for always-on-top mini players on macOS/Windows?

### Takeaway
For **video**, power users lean on mpv `--ontop` (often with `--no-border` / `--snap-window`), IINA’s native PiP/music mini player on macOS, and PotPlayer/VLC “on top” settings on Windows; dedicated floating mini-player products skew toward **music** (Amee, Cellar, MixMusic) rather than multi-video walls.

### Cited Findings
- mpv: `--ontop`; macOS-only `--ontop-level=window|system|desktop`; key `T` toggles — [mpv manual](https://mpv.io/manual/stable/); Reddit guidance matching the man page — [r/mpv always on top](https://www.reddit.com/r/mpv/comments/ribu6d/some_way_to_make_mpv_be_always_on_top_when/)
- Windows mpv PiP-style tip: `mpv --ontop --no-border --snap-window` — [r/mpv PiP Windows](https://www.reddit.com/r/mpv/comments/xevf5y/picture_in_picture_pip_for_windows_mpv/)
- IINA lists Picture in Picture and a standalone Music Mode as first-class macOS features — [IINA](https://iina.io/)
- Plex for Windows/macOS: official “Keep Plex on top of windows” setting; **no native PiP** as of 2024–2025 forum threads; users resize + AOT — [Plex Support](https://support.plex.tv/articles/configuration-and-usage-options/); [Plex forum PiP macOS May 2025](https://forums.plex.tv/t/pip-in-macos/919875)
- Reddit: Plex desktop users report longstanding memory growth (claims up to multi-GB / even 100 GB) while others see ~700 MB stable — conflicting **reported** evidence — [r/PleX memory leak](https://www.reddit.com/r/PleX/comments/1bla6pj/plex_media_player_on_windows_has_had_a_massive/)
- Vivi (libmpv + Slint): built-in miniplayer (`P`) as always-on-top compact window — [Vivi](https://viviplayer.app/)
- Amee: macOS floating always-on-top mini player for system Now Playing (Music/Spotify/browser), not a general video engine — [Amee](https://amee.thiennguyen.dev/)
- Cellar / MixMusic / Ytmdesktop PiP: always-on-top music mini players for YouTube Music/Spotify ecosystems — [Cellar mini player](https://cellar.fm/features/mini-player); [MixMusic](https://github.com/fzn0x/mixmusic); [Ytmdesktop PiP](https://ytmdesktop.com/plugins/picture-in-picture/)

### Inferences
- For Pipster-like **video** AOT: ship first-class `ontop` + borderless + geometry persistence; don’t depend on OS PiP APIs alone (Plex’s gap shows product demand).
- Music mini-player UX (hover controls, remembered corner, non-activating window) is worth copying even if the media engine differs.
- **Avoid** treating Plex desktop as a resource-efficiency reference for always-on multi-window use.

### Gaps
- Limited recent (2025–2026) aggregated “best AOT video mini player” roundups with measured idle RAM; recommendations are mostly feature-based.

## Concrete production stack combinations (what to copy / avoid)

### Takeaway
Proven production pattern: **libmpv (or mpv) for decode/render + native UI toolkit**. Celluloid/IINA/MPV.net/Vivi all validate that; GridPlayer shows when to add multi-process isolation; Electron mosaics and always-on Plex-like shells are what to avoid for low RAM.

### Cited Findings
- **IINA = Swift/AppKit + libmpv** (render API / OpenGL path documented in community architecture notes) — [IINA](https://iina.io/); [DeepWiki mpv integration](https://deepwiki.com/iina/iina/2.1-mpv-integration)
- **Celluloid = GTK4/libadwaita + libmpv** (MVC over GObject) — [celluloid GitHub](https://github.com/celluloid-player/celluloid/); [DeepWiki architecture](https://deepwiki.com/celluloid-player/celluloid/3-architecture-overview)
- **mpv.net = modern Windows GUI + libmpv** — [mpv.net](https://github.com/mpvnet-player/mpv.net/)
- **Vivi = Slint + libmpv**, cross-platform, with published footprint methodology — [Vivi compare](https://viviplayer.app/compare.html)
- **GridPlayer = Python/Qt + MPV and/or VLC**, with explicit multi-process VLC sharding — [gridplayer](https://github.com/vzhd1701/gridplayer/)
- **PotPlayer = proprietary Windows stack** with built-in codecs + DXVA/D3D11 (not libmpv) — [Neowin](https://www.neowin.net/software/potplayer-261021/); [BleepingComputer](https://www.bleepingcomputer.com/download/potplayer/)
- **VLC = Qt UI + libVLC** — still among the lowest idle PSS in Vivi’s Linux test when HW decode is used — [Vivi compare](https://viviplayer.app/compare.html)
- **StreamGrid = Electron + React + TS** for multi-stream UX — heavier baseline implied by stack choice — [StreamGrid](https://github.com/lordknish/streamgrid/)

### Inferences
- **Copy:** IINA/Celluloid/mpv.net pattern — one libmpv context per window/player core; expose mpv `hwdec`, `vo`, `ontop`; keep UI process work off the present path.
- **Copy:** GridPlayer’s decode-mode matrix (HW vs SW; process sharding; optional frame skip under overload).
- **Avoid:** embedding a full browser engine per stream; sharing one fragile media process for all tiles without crash isolation if using libVLC.
- **Avoid:** shipping without HW decode defaults — software decode inflates CPU dramatically (PotPlayer and Celluloid evidence).

### Gaps
- No authoritative public post-mortem comparing libmpv multi-context vs multi-process for 8+ AOT windows on Apple Silicon specifically.

## Rough resource ballparks: RAM for 1 vs 4 vs 8 simultaneous 1080p HW-decoded streams

### Takeaway
Hard **measured** per-player N=1/4/8 system-RAM series are scarce; the best-supported numbers are single-stream player PSS/RSS, GPU VRAM per decode session, and NVDEC fps ceilings. Treat multi-window system RAM as roughly linear in player instances unless contexts are shared.

### Cited Findings
- **Measured single-stream system RAM (Linux PSS, 1080p H.264, Vivi methodology):** playing ~277 MB (VLC, GPU), ~286 MB (Vivi, GPU), ~361 MB (mpv, software default), ~570 MB (Celluloid, software default) — [Vivi compare](https://viviplayer.app/compare.html)
- **Measured single-stream system RAM (Windows, 4K HEVC HW):** PotPlayer ~218 MB; VLC ~268 MB on same file — [AnyMP4 PotPlayer review](https://www.anymp4.com/player/potplayer-review.html)
- **Measured single-stream system RAM (macOS, 8K HEVC HW):** mpv ~1.8 GB; IINA ~2.1 GB; VLC ~3.2 GB — resolution-driven, not 1080p — [Guru Software](https://www.gurusoftware.com/finding-the-ultimate-video-player-for-mac-in-2023/)
- **Reported VRAM per NVDEC/ffmpeg session:** ~87–153 MB depending on GPU/driver; multi-process multiplies CUDA context overhead — [NVIDIA forum](https://forums.developer.nvidia.com/t/nvdec-nvenc-vram-allocation-differences-between-different-gpus/65151)
- **Measured NVDEC throughput:** Ampere ~748 fps H.264 1080p per engine (SDK note); ~768 fps aggregate on 3090 study → ~25× 1080p30 — [NVDEC Application Note](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvdec-application-note/index.html); [NVDEC capacity study](https://sadbodhs.github.io/computer_vision_optimization/nvdec/)
- GridPlayer author: 4 VLC instances per process found optimal after CPU-spike experiments (**reported** engineering note, not RAM MB) — [gridplayer#15](https://github.com/vzhd1701/gridplayer/issues/15)
- MultiViewer docs: multiple streams may require significant CPU/GPU; high-end GPUs may still struggle — qualitative only — [MultiViewer requirements](https://multiviewer.app/docs/getting-started/requirements)

### Inferences (clearly labeled estimates)
- **Estimate — system RAM, thin libmpv/VLC-class player, 1080p HW decode, separate instances:** ~0.25–0.4 GB × N → roughly **~0.3 GB (1) / ~1.0–1.6 GB (4) / ~2.0–3.2 GB (8)** before compositor/shared library accounting. Derived from Vivi/PotPlayer single-stream playing footprints, assuming near-linear instance scaling.
- **Estimate — VRAM, shared CUDA/NVDEC context:** ~0.1–0.15 GB × N → **~0.1 / 0.4–0.6 / 0.8–1.2 GB** for 1/4/8, plus display surfaces; higher if each process creates its own CUDA context.
- **Estimate — CPU with HW decode:** often single-digit to low-teens % of one core per 1080p stream on modern GPUs when composition is cheap; software decode can be an order of magnitude worse (PotPlayer 3.8% vs 22% on 4K; Celluloid vs mpv Wayland gap).
- **Decode headroom:** on a single consumer NVDEC, 8×1080p30 is usually well inside fps capacity; 8 windows failing is more likely UI/compositor/bandwidth than decoder fps.

### Gaps
- No primary-source measured RSS/PSS series for 1/4/8 concurrent 1080p HW sessions in mpv, IINA, PotPlayer, or GridPlayer on a documented machine.
- No Apple Silicon VideoToolbox per-session memory accounting suitable for the same ballpark table.
- Shared-memory savings from one process hosting N `mpv_render_context`s remain unquantified in public benches found here.

## Cross-cutting: what a new lightweight multi-window player should copy or avoid

### Takeaway
Copy the mpv-family decode stack and AOT/windowing primitives; copy GridPlayer/Nova layout and process lessons; avoid Electron mosaics, software-decode defaults, and assuming a single UI process is free.

### Cited Findings
- (Synthesis rests on findings cited in sections above; key primary links repeated for the report writer.)
- libmpv frontend success cases: IINA, Celluloid, mpv.net, Vivi — [iina.io](https://iina.io/); [celluloid](https://github.com/celluloid-player/celluloid/); [mpv.net](https://github.com/mpvnet-player/mpv.net/); [viviplayer.app/compare.html](https://viviplayer.app/compare.html)
- Multi-window product patterns: GridPlayer, Nova, fGadget, Happytime, MultiViewer — [gridplayer](https://github.com/vzhd1701/gridplayer/); [nova](https://github.com/ANDYwyLEE/nova); [fGadget](https://futomi.app/projects/fgadget-cam-monitor/); [Happytime](https://www.happytimesoft.com/products/media-client/); [MultiViewer](https://multiviewer.app/)
- HW decode / multi-session physics: NVIDIA NVDEC note + capacity study — [NVDEC Application Note](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvdec-application-note/index.html); [NVDEC capacity](https://sadbodhs.github.io/computer_vision_optimization/nvdec/)

### Inferences
- **Must-copy:** HW decode on by default; per-window mute; AOT + geometry persistence; layout presets; optional process isolation when embedding VLC-like engines.
- **Must-avoid:** per-stream Chromium; unbounded single-process libVLC; ignoring GPU VRAM and compositor costs; marketing “unlimited windows” without decode/composition budgets.
- **Differentiation opportunity:** few general-purpose **local-file** players combine Nova-style floating windows with mpv-class efficiency; most mosaics are either camera/NVR, F1 DRM, or Electron.

### Gaps
- Market still lacks a canonical open benchmark suite for “N floating 1080p AOT windows” across engines; Pipster would benefit from publishing one.
