# Multi-window PiP-style desktop video player architecture

## Single-process multi-window vs multi-process: RAM, crash isolation, GPU contexts

### Takeaway
For many concurrent floating video windows, a **single-process host with multiple isolated player cores** (IINA/libmpv pattern) is the strongest low-RAM path because GPU runtime and composition device costs amortize across windows; multi-process gives crash isolation at the cost of duplicated process/GPU overhead.

### Cited Findings
- mpv’s CLI core is effectively one playback loop / one window per process; maintainers state the core does not support multiple windows in one instance, while libmpv users can host multiple windows. — [mpv issue #8742](https://github.com/mpv-player/mpv/issues/8742)
- IINA runs **one OS process** with a pool of `PlayerCore` objects; each core has its own window and mpv handle; closing a window stops mpv into idle and **reuses** the core/window rather than destroying them. — [IINA issue #6311](https://github.com/iina/iina/issues/6311); [IINA PlayerCore.swift](https://github.com/lhc70000/iina/blob/develop/iina/PlayerCore.swift)
- IINA documents that each player core is isolated (own window, mpv instance, plugin instances) while still living in one app process; a global plugin entry coordinates across cores. — [IINA Global Entry Point](https://docs.iina.io/pages/global-entry.html); [IINA Getting Started](https://docs.iina.io/pages/getting-started.html)
- libmpv allows multiple player cores via separate `mpv_create()` handles in one process; each core may have its own render context; OpenGL must be current and **the same context that created that render context**; different cores can call render APIs concurrently. — [mpv render.h](https://github.com/mpv-player/mpv/blob/master/include/mpv/render.h); [mpv issue #4379](https://github.com/mpv-player/mpv/issues/4379)
- At most one render context exists per mpv core (it represents the main video output). — [mpvpaper issue #42 citing render.h](https://github.com/GhostNaN/mpvpaper/issues/42)
- VideoLAN historically treated libvlc as a singleton-per-app: one `libvlc_new()` initializes threads/module bank; multiple OS instances add memory overhead; multiple players within one instance was the intended multi-stream path (with caveats that multiple players/instances were under-tested). — [Hacker Guide/libvlc](https://wiki.videolan.org/Hacker_Guide/libvlc/); [vlc-devel Aug 2009](https://mailman.videolan.org/pipermail/vlc-devel/2009-August/064101.html)
- Separate VLC executable instances are explicitly supported via UI prefs / `--no-one-instance` when independent processes are desired. — [VLC HowTo/Play multiple instances](https://wiki.videolan.org/VLC_HowTo/Play_multiple_instances)
- Windows DWM composition uses **one composition process** (`dwm.exe`) per session and states that **all applications share a single Direct3D device for composition**, offering “considerable memory savings,” and can skip composing fully occluded windows. — [Microsoft DirectComposition architecture](https://learn.microsoft.com/en-us/windows/win32/directcomp/architecture-and-components)
- A 2026 Wayland multi-window host profile (terminal, not video, but same window/GPU amortization lesson) measured GPU host first-window RSS ~60–61 MB then **~1–2 MB per additional window**, vs CPU/SHM host ~20 MB per extra window; authors conclude shared-GPU single-process hosting is the strongest low-RAM path and removed a daemon/thin-client multi-process design after profiling. — [handterm OPTIMIZATION.md](https://github.com/1jehuang/handterm/blob/master/OPTIMIZATION.md)

### Inferences
- **RAM:** Prefer single-process + N player cores + shared GPU device/queues; expect a large fixed first-window GPU cost and a smaller per-window surface/decoder cost. Multi-process (one OS process per video) duplicates runtime, module caches, and often GPU contexts → worse for “many PiP windows.”
- **Crash isolation:** Multi-process wins (one decoder crash kills one window). Single-process can mitigate with careful sandboxing of decode threads and recycling cores after faults, but a process-wide GPU/driver fault still takes everything.
- **GPU contexts:** One process can share a GPU device across windows while still giving each player its own swapchain/texture/render context (libmpv: one render context per core). Separate processes typically cannot share one GL/D3D device cheaply across process boundaries without explicit cross-process sharing APIs.

### Gaps
- No public head-to-head RAM/CPU numbers for “N concurrent 480p PiP windows” comparing IINA-style single-process vs N×mpv processes on the same machine.
- PotPlayer / Celluloid internal multi-window process models were not documented in primary sources found during this pass.

---

## One decoder feeding multiple windows vs one player per window (independent seek/pause)

### Takeaway
**Independent seek/pause requires one player/decoder timeline per window.** One decoder can feed many windows only for **mirrored/clone** views of the same playback position; sharing breaks as soon as windows need different positions or pause states.

### Cited Findings
- IINA: each window is tied to a separate player core with its own mpv instance; cores do not share playback data; plugins needing cross-window sync use explicit message passing. — [IINA Getting Started](https://docs.iina.io/pages/getting-started.html); [IINA Global Entry Point](https://docs.iina.io/pages/global-entry.html)
- mpv playback is organized around one `MPContext` / playloop that decodes and pushes to one VO/AO path per core. — [mpv DOCS/tech-overview.txt](https://github.com/mpv-player/mpv/blob/master/DOCS/tech-overview.txt)
- Only one `mpv_render_context` per mpv core (main video output). — [mpvpaper issue #42](https://github.com/GhostNaN/mpvpaper/issues/42)
- DisplayLink’s video-clone sample sends **the same frame** to all connected displays for identical synchronized content (clone, not independent players). — [DisplayLink Video Clone](https://displaylink.github.io/displaylink-direct/code-samples/video-clone/)
- Huawei AVCodec guidance: pause stops submitting encoded input and stops delivering decoded frames to the surface; resume restarts those threads — implying the decoder’s progress is the single timeline for that surface path. — [Decoded Video Playback Control (surface mode)](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/video-decoding-play-remote)
- Linux V4L2 stateless decoder docs: pause by ceasing to queue `OUTPUT` buffers; without input, the codec stays idle (again one decode timeline). — [V4L2 Stateless Video Decoder Interface](https://dri.freedesktop.org/docs/drm/media/uapi/v4l/dev-stateless-decoder.html)

### Inferences
- Architecture for Pipster-like “many independent floating players”: **one player instance (decode + demux + clock) per window**, optionally sharing demux cache/file handles at a higher layer only when safe.
- Optional optimization: a “clone mode” where secondary windows sample the primary’s last decoded texture (no second decoder) for wall displays — useful, but not a substitute for independent control.
- OS-native PiP APIs are single-session oriented (see macOS section below); custom multi-window players must implement their own surfaces rather than stacking multiple system PiP controllers.

### Gaps
- No primary paper found that quantifies CPU savings of “decode once, blit N times” vs N decoders for desktop PiP workloads with independent seek.
- Hardware decoder session limits (how many concurrent HWDEC sessions per GPU) vary by vendor/driver and were not inventoried here.

---

## Always-on-top, borderless, translucent, resizable floating windows (macOS / Windows / Linux)

### Takeaway
Bypass OS single-PiP limits by creating normal (or panel) top-level windows with floating levels / `WS_EX_TOPMOST` / layer-shell or EWMH `_NET_WM_STATE_ABOVE`, then style them borderless/translucent and optionally click-through — these are ordinary windows, so **many** can exist.

### Cited Findings

#### macOS (AppKit)
- `NSPanel.isFloatingPanel` makes a panel float above other windows; Apple’s HIG-like guidance: float only if small, mouse-oriented, needs to stay visible while working in standard windows, and typically hides on app deactivate. — [Apple isFloatingPanel](https://developer.apple.com/documentation/appkit/nspanel/isfloatingpanel)
- Practical floating recipe: `NSPanel` with `.nonactivatingPanel`, `isFloatingPanel = true`, `level = .floating` (or higher e.g. `.mainMenu` / `mainMenu+N` for above menu bar), `collectionBehavior` including `.fullScreenAuxiliary` / `.canJoinAllSpaces`, optional borderless + clear/opaque flags. — [Cindori floating panel](https://cindori.com/developer/floating-panel); [Stack Overflow fullscreen float](https://stackoverflow.com/questions/36205834/allow-an-nswindow-nspanel-to-float-above-full-screen-apps); [NotchStar NSPanel notes](https://www.notchstar.com/blog/nspanel-above-the-menu-bar/)
- Click-through: `NSWindow.ignoresMouseEvents = true` makes the **entire** window transparent to mouse events. — [Apple ignoresMouseEvents](https://developer.apple.com/documentation/appkit/nswindow/ignoresmouseevents)
- Per-pixel / shaped click-through is subtler: borderless + non-layer-backed transparent windows can pass clicks in clear areas; forcing `ignoresMouseEvents = false` or enabling `wantsLayer` can break that; `hitTest` returning `nil` is another pattern. — [Stack Overflow click-through NSWindow](https://stackoverflow.com/questions/29441015/click-through-custom-nswindow)
- System PiP (`AVPictureInPictureController`) is an AVKit feature for a movable floating video window; documentation focuses on a single custom-player PiP controller path (iOS-originated PiP model), not N concurrent system PiP windows — motivating custom windows for many surfaces. — [Apple Media Playback Guide – PiP](https://developer.apple.com/library/archive/documentation/AudioVideo/Conceptual/MediaPlaybackGuide/Contents/Resources/en.lproj/UsingAVKitPlatformFeatures/UsingAVKitPlatformFeatures.html)

#### Windows (Win32 / DWM)
- `WS_EX_TOPMOST` (0x00000008): window stays above all non-topmost windows even when deactivated; toggle via `SetWindowPos`. — [Microsoft Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
- `WS_EX_TOOLWINDOW`: floating-toolbar style (shorter title bar; excluded from taskbar / Alt+Tab by default). — [same MS docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
- `WS_EX_LAYERED`: layered window (required for many alpha/transparency paths); `WS_EX_NOACTIVATE`: click does not activate / bring to foreground. — [same MS docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
- Click-through overlay pattern: combine `WS_EX_LAYERED | WS_EX_TRANSPARENT` (and often `WS_EX_TOPMOST`), set opacity with `SetLayeredWindowAttributes`; note `WS_EX_TRANSPARENT` makes the **whole** window pass mouse input. — [Stack Overflow layered click-through](https://stackoverflow.com/questions/39855720/windows-forms-pass-clicks-through-a-partially-transparent-always-on-top-window); [CodeProject layered click-through](https://www.codeproject.com/Articles/12365/How-to-make-a-transparent-and-click-through-window)
- DWM composes all app windows in one engine; occluded windows can skip composition work; shared D3D composition device. — [Microsoft DirectComposition architecture](https://learn.microsoft.com/en-us/windows/win32/directcomp/architecture-and-components)

#### Linux (Wayland / X11)
- **Wayland (wlroots family):** `zwlr_layer_shell_v1` places surfaces on layers `background | bottom | top | overlay` with anchors, exclusive zone, margins, and keyboard interactivity; multiple surfaces may share a layer. Input can be disabled by setting an empty input region. — [wlr-layer-shell-unstable-v1](https://wayland.app/protocols/wlr-layer-shell-unstable-v1)
- Layer-shell is widely supported on Sway/Hyprland/KWin/labwc/etc., but **Mutter is listed as unsupported (`x`)** on the protocol compatibility table — GNOME Wayland apps cannot rely on layer-shell for overlays. — [same Wayland Explorer page](https://wayland.app/protocols/wlr-layer-shell-unstable-v1)
- Layer-shell concepts: overlay is above everything; exclusive zone `Ignore` for floating overlays that should not push other windows; empty/non-focusable overlays can accidentally block input if mismanaged. — [Quickshell Layer Shell Concepts](https://programmersd21.github.io/the_quickshell_book/part-3-windows/layer-shell-concepts)
- **X11/EWMH:** `_NET_WM_STATE_ABOVE` requests always-on-top; clients must send a `_NET_WM_STATE` **ClientMessage** to the root (ADD/REMOVE/TOGGLE), not set the property unilaterally; WMs may treat ABOVE as user-preference-oriented. — [Freedesktop EWMH §5.7](https://specifications.freedesktop.org/wm/latest/ar01s05.html); [Stack Overflow X11 always-on-top](https://stackoverflow.com/questions/20733215/how-to-make-a-window-always-on-top)

### Inferences
- Treat each PiP as a **first-class top-level window** with floating z-order, not as nested OS PiP.
- Resizability: use normal window chrome (`NSResizableWindowMask` / `WS_THICKFRAME` / xdg toplevel) or custom hit regions on borderless windows; borderless + custom resize grips is common for video HUD chrome.
- Wayland portability: prefer xdg-toplevel + compositor-specific “always on top” where available; use layer-shell only where present, with a GNOME fallback (regular toplevel + user pin, or XWayland caveats).

### Gaps
- Windows AppWindow / WinUI Presenter APIs for modern packaged apps were not deep-dived; Win32 styles above remain the portable desktop path.
- No single cross-platform toolkit doc that maps these exact flags for Tauri/Electron/Qt side-by-side with video surfaces.

---

## Keeping idle or paused windows cheap

### Takeaway
Paused/idle windows should **stop demux/decode and audio**, retain a **last-frame texture/surface** for display, recycle player cores when closed, and optionally **lower present rate / decode resolution** for tiny visible windows that are still playing.

### Cited Findings
- IINA on close: send mpv `stop`, enter idle-active, cache the player (mpv core + window) for reuse — avoids reconstruct cost and follows Apple’s reuse guidance. — [IINA issue #6311](https://github.com/iina/iina/issues/6311); [PlayerCore.swift](https://github.com/lhc70000/iina/blob/develop/iina/PlayerCore.swift)
- Pause implementation pattern: block/stop feeding decoder input and stop pushing frames to the surface; last displayed frame remains. — [Huawei AVCodec pause/resume](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/video-decoding-play-remote)
- V4L2: pause by not queuing more `OUTPUT` buffers → codec idle. — [V4L2 Stateless Decoder](https://dri.freedesktop.org/docs/drm/media/uapi/v4l/dev-stateless-decoder.html)
- Windows DWM skips composition work for **fully occluded** windows — hiding or covering unused PiPs reduces compositor GPU/CPU. — [Microsoft DirectComposition architecture](https://learn.microsoft.com/en-us/windows/win32/directcomp/architecture-and-components)
- Wayland: clients own buffers; compositor decides scanout vs GL compose; zero-copy dmabuf paths exist but only a limited number of buffers can be directly scanned out — many simultaneous video surfaces may force composition. — [FOSDEM Wayland slides (Tizen/Enlightenment)](https://archive.fosdem.org/2016/schedule/event/enlightenment_of_wayland/attachments/slides/1233/export/events/attachments/enlightenment_of_wayland/slides/1233/Tizen_E_Wayland.pdf); [Optimal buffer allocation on Wayland (abstract)](https://doi.org/10.5446/56820)

### Inferences
- State machine per window: `playing → paused (decode off, keep texture) → idle/stopped (release decoder + large pools, keep window shell) → destroyed/pooled`.
- Still-playing but “background” strategies: decode at display size (not source 4K) for small PiPs; cap FPS (e.g. 15–24) when window area is below a threshold; mute by default and decode audio only for focused/unmuted windows.
- Prefer retaining GPU texture of last frame over re-decoding a keyframe on every unpause if resume latency matters; free textures when idle for long.

### Gaps
- No authoritative “FPS throttle for small video windows” engineering post from major desktop players found; recommendations above are synthesized from decode/pause and compositor behavior.
- Exact VRAM retained by a paused HWDEC surface across macOS VideoToolbox / DXVA / VAAPI was not measured.

---

## Coordination: main library window + child players; IPC if multi-process

### Takeaway
Use a **controller/main window** that owns library/playlist state and spawns/manages child player cores in-process (IINA global pattern). Only introduce IPC if you choose multi-process isolation (mpv’s socket/umpv style).

### Cited Findings
- IINA: `PlayerCore.newPlayerCore` reuses idle cores or creates new ones; `openURLs` can open each URL in its own window; preference `alwaysOpenInNewWindow` drives behavior; duplicate URL can be skipped. — [PlayerCore.swift](https://github.com/lhc70000/iina/blob/develop/iina/PlayerCore.swift)
- IINA global plugin API: `global.createPlayerInstance({ url, disableWindowAnimation, disableUI, ... })` for programmatically controlled windows (presentations/video walls); `global.postMessage` / `onMessage` for cross-core coordination. — [IINA Global Entry Point](https://docs.iina.io/pages/global-entry.html)
- mpv multi-process coordination: secondary launches connect to `--input-ipc-server` and send JSON IPC (`loadfile`, etc.), then exit (umpv / single-instance wrappers). — [mpv issue #4954](https://github.com/mpv-player/mpv/issues/4954); [MPV-Single-Instance](https://github.com/ashik4u/MPV-Single-Instance); [umpv discussion in #3811](https://github.com/mpv-player/mpv/issues/3811)
- VLC libvlc: one instance initializes shared subsystems; playlist/control threads spawned at init — multi-player coordination is in-process API rather than OS IPC. — [Hacker Guide/libvlc](https://wiki.videolan.org/Hacker_Guide/libvlc/)

### Inferences
- Recommended control plane for low RAM: **in-process message bus** (commands: open, seek, pause, volume, close, layout) from main UI → player cores; no serialization tax.
- If multi-process: define a narrow IPC (path, start position, audio device, window geometry, always-on-top) over sockets/named pipes; keep media bytes out of IPC; expect higher baseline RAM.
- Playlist ownership should stay in main process/core; children hold only “now playing” + local transport state.

### Gaps
- Celluloid (GTK mpv frontend) multi-window coordination docs were not located in this pass.
- No standard schema for “PiP window manager” IPC across players.

---

## Pitfalls: GPU memory, compositor cost, audio mixing

### Takeaway
Costs scale with **decoded frame size × window count × buffer depth** plus compositor pressure; audio is usually fine if each player opens a **shared-mode** stream and the OS mixes, but N unmuted videos still mean N decode+resample paths and clipping/loudness issues — mute-by-default and focus-follows-audio policies are essential.

### Cited Findings
- DWM: shared composition device saves memory vs per-process composition; can detect full occlusion and avoid wasting CPU/GPU; composes to screen back buffer without an extra per-process copy. — [Microsoft DirectComposition architecture](https://learn.microsoft.com/en-us/windows/win32/directcomp/architecture-and-components)
- Wayland: each window/surface has client-allocated buffers (SHM or GPU); compositor may GL-compose or assign to hardware overlay layers; mid-range HW may only support a handful of direct-scanout overlays — excess video windows fall back to composition. — [FOSDEM Wayland slides](https://archive.fosdem.org/2016/schedule/event/enlightenment_of_wayland/attachments/slides/1233/export/events/attachments/enlightenment_of_wayland/slides/1233/Tizen_E_Wayland.pdf); [dmabuf feedback talk abstract](https://doi.org/10.5446/56820)
- Empirical multi-window RSS slopes (non-video host) show GPU-shared single-process incremental cost far below CPU/SHM or multi-process duplication. — [handterm OPTIMIZATION.md](https://github.com/1jehuang/handterm/blob/master/OPTIMIZATION.md)
- Wayland compositor benches explicitly measure cost of N extra toplevels and multi-animated surfaces (compositor CPU/GPU per frame). — [wayland-compositor-bench](https://github.com/grigio/wayland-compositor-bench)
- Windows WASAPI shared mode: each client writes to an endpoint buffer; **Audiodg.dll audio engine mixes streams from multiple applications/processes** in software; exclusive mode denies sharing. Engine mixes **PCM only**. — [Microsoft User-Mode Audio Components](https://learn.microsoft.com/en-us/windows/win32/coreaudio/user-mode-audio-components)
- Historical VLC discussion: multiple audio outputs / mixing across players was a design concern when debating multiple instances vs multiple players. — [vlc-devel Aug 2009](https://mailman.videolan.org/pipermail/vlc-devel/2009-August/064101.html)

### Inferences
- **GPU memory:** Prefer one shared device; size swapchains to window pixels; use 8-bit surfaces; destroy decode pools on idle; avoid keeping full-resolution textures for postage-stamp PiPs.
- **Compositor:** Many always-on-top updating videos force continuous composition; paused windows (static texture) and occlusion help; don’t animate chrome at 60 Hz needlessly.
- **Audio:** Default mute new windows; allow one “audio focus” window; or app-side mix to a single WASAPI/Core Audio stream if you need ducking/limiter across PiPs. Relying on OS mix is simpler but N decoders still burn CPU.
- **Crash/GPU reset:** One bad HWDEC stream in-process can reset the shared GPU device — isolation vs RAM is the central tradeoff.

### Gaps
- No cited quantitative “N×1080p PiP ⇒ X MB VRAM” table from mpv/IINA/VLC maintainers.
- Linux PipeWire/Pulse per-stream overhead for dozens of video players was not specifically sourced.
- Apple Core Audio multi-client mixing internals were not fetched at the same depth as WASAPI.
