# Desktop frameworks for multi-window video player

Research focus: lightweight desktop stacks for many floating PiP-style video windows (2025–2026). Priority: RAM/CPU with many concurrent players; binary size secondary. Multi-format (MP4/MOV/MKV) must be feasible.

---

## How do Tauri 2, native (Swift/AppKit or SwiftUI / WinUI/WPF), Qt/QML, Flutter desktop, and Electron/CEF compare for multi-window apps in terms of per-window RAM, CPU, and binary size?

### Takeaway
Native toolkits and Qt win on per-window RAM/CPU because each window is an OS window plus a media engine instance, not a browser/webview process. Electron scales worst with window count (renderer per `BrowserWindow`). Tauri wins hard on **installer size** but on Windows its **idle private RAM is often near Electron** because WebView2 is still Chromium; macOS WKWebView multi-window demos show a larger Tauri advantage. Flutter sits between: ~15–25 MB binaries, no Chromium, but video still pays a texture/compositor tax vs native/Qt+mpv.

### Cited Findings
- Electron: each `BrowserWindow` loads content in a **separate renderer process**; destroying the window terminates that renderer — [Electron Process Model](https://www.electronjs.org/docs/latest/tutorial/process-model)
- Tauri 2: one Rust **core** process orchestrates one or more **system WebView** processes (WebView2 / WKWebView / WebKitGTK); webview is dynamically linked, not bundled — [Tauri Process Model](https://v2.tauri.app/concept/process-model/)
- Hopp (Apr 2025, macOS, N=1, 6 windows): Tauri **~172 MB** vs Electron **~409 MB**; bundles **8.6 MiB** (Tauri) vs **244 MiB** (Electron) — [Hopp: Tauri vs Electron](https://www.gethopp.app/blog/tauri-vs-electron)
- Dawloom (Oct 2026, Windows 11, process-tree private memory, 1 idle window): WPF **74.3 MB** / Tauri **142 MB** (7 processes) / Electron **149 MB** (4 processes); NSIS installers Tauri **1.05 MB** vs Electron **106.1 MB**; on-disk Electron **~367 MB** vs Tauri **3.00 MB** — [Dawloom Tauri vs Electron vs WPF](https://dawloom.com/blog/tauri-vs-electron-vs-wpf-benchmark/)
- Lleg (Windows, empty page, median of 5): Electron 45 private WS **103.3 MB** vs Tauri 2 WebView2 **108 MB** — swapping Electron→Tauri “saved no memory” for one empty window — [Lleg: Electron vs Tauri empty window](https://lleg.dev/posts/electron-vs-tauri-empty-window-memory/)
- adarSonmez harness (Windows, data-heavy React UI, whole process tree): Electron idle working set **348 MB** / private **220 MB**; Tauri idle WS **407 MB** / private **212 MB** (Tauri more processes: 7 vs 4) — [desktop-framework-benchmark](https://github.com/adarSonmez/desktop-framework-benchmark)
- Historical methodology dispute: counting shared Chromium pages can make Electron look better than Tauri/WebKit for **single** heavy web pages; USS/PSS on Ubuntu for default apps: Electron USS **118 MB** / PSS **207 MB** vs Tauri USS **125 MB** / PSS **185 MB** — [tauri#5889](https://github.com/tauri-apps/tauri/issues/5889)
- Ecosystem summary: Tauri process-tree RSS including WebKit helpers measured **~211 MiB**; official Tauri v1 benchmarks page removed in v2 era; treat memory numbers as platform-dependent — [rust-gui-desktop-ecosystem-state report/04-tauri.md](https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/report/04-tauri.md)
- Flutter minimal macOS Release UI probe: **~19.94 MB** app (engine ~14.1 MB + Dart ~4.6 MB + ICU ~0.86 MB) vs SwiftUI arm64 probe **~79 KB** — [bitchat DESKTOP-RESEARCH.md](https://github.com/youhanasheriff/bitchat-desktop/blob/main/docs/DESKTOP-RESEARCH.md)
- Secondary blog (less rigorous method): macOS file-manager demo claimed Tauri installer **3.8 MB** / Flutter **18 MB** / Electron **165 MB**; idle memory claims Tauri 25–40 MB / Flutter 45–70 MB / Electron 90–180 MB — **conflicts with Dawloom/Lleg process-tree private memory** — [Kanopy Labs comparison](https://kanopylabs.com/blog/tauri-vs-electron-vs-flutter-desktop-apps)
- Stack Overflow report (methodology informal): 20 Electron child windows ≈ **23 processes / ~800 MB**; `window.open` path consolidated to ~4 processes / ~400 MB — [SO: process per window](https://stackoverflow.com/questions/57528101/why-electron-spawns-a-process-per-each-window)
- Flutter `desktop_multi_window`: **new Flutter engine per window**; plugins must be re-registered per engine — [desktop_multi_window docs](https://pub.dev/documentation/desktop_multi_window/latest/); confirmed by Flutter team for secondary-window video plugin failures — [flutter#176632](https://github.com/flutter/flutter/issues/176632)
- Flutter `multiview_desktop`: all OS windows share **one engine / one isolate / one memory space** — [multiview_desktop on pub.dev](https://pub.dev/packages/multiview_desktop)
- Flutter+fvp same 1080p clip vs Tauri WebView2 / PotPlayer: reporter measured Flutter **~18% CPU / ~400 MB** vs Tauri WebView2 **0–1% CPU / ~200 MB** vs PotPlayer **~0% / ~180 MB**; maintainer attributes gap partly to Flutter pipeline vs native/Qt — [fvp#296](https://github.com/wang-bin/fvp/issues/296)
- Qt Multimedia ships separate FFmpeg / GStreamer backends (Arch qt6 6.12: multimedia **~6.9 MB** installed; ffmpeg backend **~3.2 MB**; gstreamer backend **~2.1 MB** — libraries only, not full app) — [Arch qt6-multimedia](https://archlinux.org/packages/extra/x86_64/qt6-multimedia/), [qt6-multimedia-ffmpeg](https://archlinux.org/packages/extra/x86_64/qt6-multimedia-ffmpeg/), [qt6-multimedia-gstreamer](https://archlinux.org/packages/extra/x86_64/qt6-multimedia-gstreamer/)

### Inferences
- **Scaling with many windows** matters more than single-window idle: Electron’s per-window renderer makes Hopp’s 6-window gap (~2.4×) the relevant signal for PiP-style UIs; Dawloom’s 1-window Tauri≈Electron result is also real for Windows WebView2.
- **Binary size ranking (typical):** SwiftUI/AppKit tiny (OS frameworks) ≪ Tauri (no browser) ≪ Flutter (~15–25 MB) ≪ self-contained WPF/Avalonia (~50–140 MB) ≪ Electron/CEF (~100–300+ MB). Adding libmpv/FFmpeg codecs adds tens of MB to all stacks that bundle them.
- For **many concurrent video surfaces**, expected CPU/RAM efficiency order: **native (AppKit/WPF) + libmpv ≈ Qt/QML + libmpv > Flutter (single-engine multi-view) ≫ Tauri/Electron with `<video>`/webview decode ≫ Electron with one Chromium renderer per PiP window**.
- Kanopy’s “Tauri 25–40 MB idle” should not be used for decision-making against process-tree private-byte studies.

### Gaps
- No single published apples-to-apples harness measuring **N empty windows + N 1080p players** across Tauri, Qt, Flutter, Electron, and Swift/WPF with the same methodology.
- Reliable **per-window incremental MB** for Qt/QML and SwiftUI AppKit empty windows is largely absent from public blogs (only informal or app-specific).
- CEF-as-embedded (not Electron) multi-window numbers for 2025–2026 were not found in this pass.

---

## Which frameworks allow true native OS windows (not webview per window) and hardware-accelerated video surfaces without a Chromium process per window?

### Takeaway
**True native OS windows + HW video without Chromium:** Swift/AppKit (or SwiftUI wrapping AppKit), WPF/WinUI, Qt Widgets/QML, and Flutter desktop (native window + Skia/Impeller view). **Tauri and Electron always involve a browser engine for UI**; they can still host a **native video surface under/beside a transparent webview**, but the webview/Chromium process tree remains unless UI is non-web.

### Cited Findings
- Tauri UI is always WebView-backed (WebView2 / WKWebView / WebKitGTK) — [Tauri Process Model](https://v2.tauri.app/concept/process-model/)
- Electron UI is Chromium renderers per `BrowserWindow` — [Electron Process Model](https://www.electronjs.org/docs/latest/tutorial/process-model/)
- libmpv embedding options: (1) native `wid` child window, (2) **render API** into app OpenGL/D3D (recommended over `wid` especially on macOS/Qt) — [mpv-examples libmpv README](https://github.com/mpv-player/mpv-examples/blob/master/libmpv/README.md)
- Official QML example: `QQuickFramebufferObject` + `mpv_render_context_render` into FBO with `hwdec=auto` — [mpv-examples qml/main.cpp](https://github.com/mpv-player/mpv-examples/blob/master/libmpv/qml/main.cpp)
- IINA (macOS): Swift UI + `PlayerCore` / `MPVController` + OpenGL render path; **one process, multiple player/window instances**; closing a window `stop`s mpv and **caches** player+window for reuse — [IINA PlayerCore.swift](https://github.com/lhc70000/iina/blob/develop/iina/PlayerCore.swift); maintainer description of multi-window / cache — [iina#6311](https://github.com/iina/iina/issues/6311)
- Celluloid: GTK4/libadwaita frontend for **libmpv** (not GStreamer), video via OpenGL into `GtkGLArea` / graphics offload — [celluloid README](https://github.com/celluloid-player/celluloid/); [DeepWiki video area](https://deepwiki.com/celluloid-player/celluloid/4.2-video-display-area)
- Tauri + native mpv under transparent WebView (Windows spike, 2026): `wid` + `vo=gpu-next` / `d3d11`, WebView2 transparent on top; rejected copying frames into webview for 4K — [NOVA ADR 0002](https://nova.snowbros.me/docs/adr-0002)
- Cathode (Tauri v2): libmpv **render API** to native GL surface **behind** transparent webview (NSOpenGLView / ANGLE D3D11 / GtkGLArea) — [kaiserbh/cathode](https://github.com/kaiserbh/cathode)
- wry/Tauri community: mpv/`wid` and GStreamer possible; OpenGL-in-webview still incomplete/flicker reports — [wry discussion #284](https://github.com/tauri-apps/wry/discussions/284)
- Flutter: native OS windows; multi-engine (`desktop_multi_window`) vs multi-view single engine (`multiview_desktop`) — [desktop_multi_window](https://pub.dev/documentation/desktop_multi_window/latest/); [multiview_desktop](https://pub.dev/packages/multiview_desktop)
- Qt Multimedia GStreamer backend is default on embedded Linux; for full pipeline control Qt docs recommend considering GStreamer `qml6glsink` rather than only Qt Multimedia abstraction — [Qt 6.12 GStreamer backend](https://doc.qt.io/qt-6/qtmultimedia-gstreamer.html)
- GStreamer GTK4: `gtk4paintablesink` can produce GL textures / DMABufs; GTK 4.14+ DMABuf path — [GStreamer gtk4paintablesink](https://gstreamer.freedesktop.org/documentation/gtk4/)

### Inferences
- For PiP **child windows**, the discriminating question is not “can the OS open a window?” (all can) but **what lives inside each window**:
  - Native/Qt/Flutter: window chrome + GPU surface / Flutter view
  - Tauri: typically **another WebView** (or webview+native surface composite)
  - Electron: typically **another Chromium renderer**
- Hybrid Tauri/mpv can avoid **decoding** in Chromium but **cannot** eliminate WebView2/WKWebView helper memory for any window that still hosts HTML UI.
- Best “no Chromium anywhere” stacks for this product shape: **AppKit/SwiftUI (macOS-first), Qt/QML cross-platform, or Flutter with single-engine multi-view**—not Tauri/Electron unless UI is minimal and video is fully native-sided.

### Gaps
- No official Tauri API that creates a **non-webview** window for pure video (community/custom raw-window-handle workarounds only).
- WinUI 3 / Windows App SDK multi-window + MediaPlayerFrame vs WPF MediaElement comparative memory data not found.
- Whether Tauri can share one WebView2 environment across many floating video windows without proportional renderer growth is documented as “site isolation / same origin may share,” but **not quantified** for N video windows — [fuyu memory research](https://github.com/TixXin/fuyu/blob/main/docs/research/memory-and-performance.md)

---

## What is the realistic memory overhead of one empty window vs one video-playing window in each stack?

### Takeaway
Empty-window baselines (Windows, private process-tree): **native WPF ~74 MB app**, **Tauri/Electron ~100–150 MB** for one idle web UI. Adding windows: Electron tends to add a **full renderer** each time; Tauri on macOS looked cheaper per window in Hopp’s 6-window demo; on Windows WebView2 helpers dominate and Tauri≈Electron for one window. Video-playing overhead is dominated by **decoder + GPU surfaces + frame buffers**, not the UI toolkit—native/mpv and PotPlayer-class players reported ~180–200 MB for one 1080p stream; Flutter+fvp reported ~400 MB / high CPU for the same clip in one user report.

### Cited Findings
- **Empty / idle UI (1 window, Windows):** WPF private **74.3 MB**; Tauri **142 MB**; Electron **149 MB** — [Dawloom](https://dawloom.com/blog/tauri-vs-electron-vs-wpf-benchmark/)
- **Empty page (1 window, Windows, median 5):** Electron private WS **103.3 MB**; Tauri **108 MB** — [Lleg](https://lleg.dev/posts/electron-vs-tauri-empty-window-memory/)
- **6 windows (macOS, N=1):** Tauri **~172 MB** total; Electron **~409 MB** total ⇒ rough implied Electron extra ≈ **~40+ MB per window** if linear (not measured as marginal by author) — [Hopp](https://www.gethopp.app/blog/tauri-vs-electron)
- **Hidden Tauri WebView2 does not free RAM by default:** `WebviewWindow::hide()` left `IsVisible=true`; `TrySuspend` dropped tree private WS from ~72 MB to **~6.3 MB after ~30 s** (commit stayed ~142 MB—pages swapped, not freed) — [tauri#16618](https://github.com/tauri-apps/tauri/issues/16618) (issue text dated against tauri 2.12.x / 2026)
- Fuyu project measurements (heavy canvas page): visible renderer private **138–140 MB** + GPU **120–136 MB**; hide without `SetIsVisible(false)` left animation/CPU running — [fuyu memory-and-performance.md](https://github.com/TixXin/fuyu/blob/main/docs/research/memory-and-performance.md)
- **Video playing (user report, Windows):** PotPlayer **~180 MB**; Tauri+WebView2 **~200 MB / 0–1% CPU**; Flutter+fvp **~400 MB / ~18% CPU** for same file — [fvp#296](https://github.com/wang-bin/fvp/issues/296)
- IINA caches idle mpv cores after window close rather than destroying them (idle-active), so “closed” windows may still retain player memory by design — [iina#6311](https://github.com/iina/iina/issues/6311)
- Historical mpv OpenGL/Qt leak reports (playback loops); 2026 discussion of FenceSync leak ~**8–9 MiB/min** RSS while rendering, with a fix PR referenced — [mpv#8516](https://github.com/mpv-player/mpv/issues/8516)
- Electron profiling guidance: sum **main + Tab + GPU**; decoded frames often live in GPU process, not V8 heap — [hackyjs Electron memory profiling](https://hackyjs.com/posts/profiling-an-electron-apps-real-memory-hogs-before-a-rust-rewrite)

### Inferences
- Practical planning numbers (order-of-magnitude, not guarantees):
  | Stack | Empty extra window (order) | Playing 1080p window (order) |
  |---|---|---|
  | Native + libmpv | low tens of MB (window+idle core) | ~decoder/GPU dominated; often ~150–250 MB class for one stream in real players |
  | Qt/QML + libmpv | similar to native if no web engine | similar; watch GL context/leak issues on destroy/recreate |
  | Flutter multi-view (1 engine) | cheap UI-wise; texture upload cost when playing | higher than native/mpv for same clip in reported cases |
  | Flutter multi-engine | expensive (new engine each window) | worse |
  | Tauri (webview UI) | shared WebView2 helpers + possible extra renderer | video-in-webview ≈ Chromium media stack; native-mpv-under-webview ≈ mpv + still pay webview base |
  | Electron | often +tens of MB per BrowserWindow renderer | Chromium media + GPU; multiplies with window count |
- For **many PiP windows**, destroying/suspending unused webviews matters as much as choosing a stack; Tauri hide≠suspend on Windows.

### Gaps
- No published **marginal MB per additional empty NSWindow / QWindow / FlutterView** tables for 2025–2026.
- No controlled measurement of **Tauri native-mpv under webview × N floating windows** memory.
- macOS-specific private/RSS caveats (compressed memory) make cross-OS comparisons fragile — Electron docs note macOS RSS quirks — [Electron process.md](https://github.com/electron/electron/blob/main/docs/api/process.md)

---

## Which stack best embeds libmpv, FFmpeg, or GStreamer without dragging a heavy web runtime?

### Takeaway
**Best fit: Qt/QML or native (Swift/AppKit, GTK, WPF with native interop) + libmpv** — production-proven (IINA, Celluloid), official examples, HW decode via render API, no browser. **FFmpeg/GStreamer** integrate cleanly into Qt Multimedia or custom GStreamer pipelines. Tauri can embed libmpv in Rust **but still ships a web UI runtime**. Electron/CEF is the worst match: you either use Chromium media or fight the web runtime while also linking native codecs.

### Cited Findings
- libmpv recommended path: render API into app GL; `wid` embedding has macOS/Qt stability issues — [mpv-examples README](https://github.com/mpv-player/mpv-examples/blob/master/libmpv/README.md)
- First-class QML/Qt OpenGL examples in upstream mpv-examples — [qml example](https://github.com/mpv-player/mpv-examples/blob/master/libmpv/qml/main.cpp)
- IINA: production macOS player, Swift + libmpv, multi-window single process — [IINA](https://github.com/lhc70000/iina); [DeepWiki MPV integration](https://deepwiki.com/iina/iina/2.1-mpv-integration)
- Celluloid: GTK4 + libmpv (clarifies naming: not a GStreamer app) — [celluloid](https://github.com/celluloid-player/celluloid/)
- Qt Multimedia: pluggable **FFmpeg** and **GStreamer** backends; GStreamer not bundled with Qt on Linux (distro-provided) — [Qt Multimedia GStreamer](https://doc.qt.io/qt-6/qtmultimedia-gstreamer.html); package sizes — [Arch packages](https://archlinux.org/packages/extra/x86_64/qt6-multimedia/)
- GStreamer + GTK4 paintablesink for custom pipelines — [gtk4paintablesink](https://gstreamer.freedesktop.org/documentation/gtk4/)
- Tauri hybrids exist (NOVA wid embedding; Cathode render API; mpv-tauri plugins) but retain WebView — [NOVA ADR](https://nova.snowbros.me/docs/adr-0002); [cathode](https://github.com/kaiserbh/cathode); [wry#284](https://github.com/tauri-apps/wry/discussions/284)
- Flutter video via MDK/fvp etc. avoids Chromium but still uses Flutter engine; reported higher CPU/RAM than native/Tauri webview for same clip — [fvp#296](https://github.com/wang-bin/fvp/issues/296)
- Multi-format (MKV/MOV/MP4): libmpv (FFmpeg demux/decode under the hood) and GStreamer/`gst-libav` are the industry paths used by serious desktop players; HTML5 `<video>` in Electron/Tauri webviews is a weaker container/codec story for MKV and exotic codecs (inferred from engine choice of IINA/Celluloid/mpv ecosystem; HTML5 limitation not separately re-benchmarked here).

### Inferences
- **Ranking for “embed serious codecs, no heavy web runtime”:**  
  1. **Qt/QML + libmpv** (best cross-platform docs/examples + native windows)  
  2. **Native per OS + libmpv** (best macOS-first: IINA pattern; Windows/Linux more DIY)  
  3. **GTK + libmpv or GStreamer** (Linux-strong)  
  4. **Flutter + MDK/fvp/GStreamer FFI** (no Chromium, but compositor tax + multi-window plugin maturity risk)  
  5. **Tauri + libmpv** (good decode path, web runtime remains)  
  6. **Electron/CEF + native addon** (possible, but always pay Chromium)
- Prefer **one mpv handle / render context per playing window**, destroy on close; avoid naïve recreate loops given historical GL leak reports.

### Gaps
- Side-by-side binary size of “Qt+libmpv player” vs “Tauri+libmpv+webview” installers not measured in cited sources.
- WinUI MediaPlayer vs libmpv on Windows for MKV/HDR comparative quality/perf not covered.
- Legal/licensing of bundling FFmpeg/mpv (GPL/LGPL) affects distribution but was out of scope for performance notes.

---

## Cross-platform tradeoffs for macOS-first but potentially Windows/Linux later?

### Takeaway
**macOS-first with optional later ports:** Swift/AppKit + libmpv (IINA architecture) maximizes multi-window PiP efficiency on the primary platform, at the cost of rewriting UI for Windows/Linux (or keeping a thin shared Rust/C++ core). **Qt/QML + libmpv** is the strongest **one-codebase** choice if Windows/Linux are likely and RAM with many windows is the top constraint. **Tauri** maximizes shipping speed and tiny downloads but fights WebView memory and fragile native-surface compositing per OS. **Flutter** is a middle path if the team is Dart-centric and uses **single-engine multi-view**, accepting video pipeline overhead. **Electron** is the poorest fit for many concurrent video windows.

### Cited Findings
- macOS Tauri uses WKWebView; Windows uses WebView2 (Chromium)—platform memory behavior diverges (Hopp macOS multi-window win for Tauri vs Dawloom/Lleg Windows near-parity) — [Tauri process model footnote](https://v2.tauri.app/concept/process-model/); [Hopp](https://www.gethopp.app/blog/tauri-vs-electron); [Dawloom](https://dawloom.com/blog/tauri-vs-electron-vs-wpf-benchmark/)
- mpv `wid` embedding “stability problems on OSX when using Qt”; render API recommended — [mpv-examples README](https://github.com/mpv-player/mpv-examples/blob/master/libmpv/README.md)
- NOVA Tauri mpv `wid` path: **Windows accepted**; macOS/Linux embedding “need their own spike” — [NOVA ADR 0002](https://nova.snowbros.me/docs/adr-0002)
- Cathode claims cross-platform native GL surfaces under Tauri webview (macOS/Windows/Linux) — [cathode](https://github.com/kaiserbh/cathode)
- Ventic notes macOS cannot use child-window `--wid` the same way; uses in-process `vo=libmpv` + `NSOpenGLView` under WKWebView — [ventic instructions](https://tomevault.io/tome/ventic/ventic)
- Flutter multi-window: official multi-engine plugin has Windows plugin-registration footguns for video — [flutter#176632](https://github.com/flutter/flutter/issues/176632); newer multi-view packages aim to fix cost — [multiview_desktop](https://pub.dev/packages/multiview_desktop)
- Qt Multimedia backend choice is OS-dependent (GStreamer emphasized on embedded Linux; FFmpeg package available) — [Qt GStreamer backend docs](https://doc.qt.io/qt-6/qtmultimedia-gstreamer.html)
- IINA proves macOS multi-window libmpv at production quality; Celluloid proves Linux GTK+libmpv — [IINA](https://github.com/lhc70000/iina); [Celluloid](https://github.com/celluloid-player/celluloid/)

### Inferences
- **If macOS is ≥80% of users and many PiP windows is the product:** ship **AppKit/SwiftUI + libmpv**; share a C/Rust media helper later for Windows.
- **If cross-platform is a near-term requirement:** **Qt 6/QML + libmpv render API** is the evidence-backed sweet spot (native windows, HW video, no Chromium, official examples).
- **If web UI talent is the constraint:** Tauri 2 + native mpv surface is viable, but budget **per-OS compositing work** (transparent webview z-order, resize, DPI) and accept **~100 MB+ webview floor** on Windows even before N videos.
- **Avoid Electron** for this problem unless CEF is already mandated; multi-window video multiplies Chromium cost.
- Binary size is a weak decision driver here: even Flutter’s ~20 MB or Tauri’s ~1–9 MB is dominated later by **codec libraries and GPU memory**, while Electron’s 100–300 MB tax is real but secondary to runtime RAM with many windows.

### Gaps
- No public 2025–2026 bake-off of **identical PiP multi-window UX** on AppKit vs Qt vs Flutter vs Tauri-mpv hybrid.
- Linux Wayland-specific multi-window + HW overlay constraints for mpv/GStreamer not deeply surveyed here.
- App Store / notarization / sandbox impacts of bundling libmpv/FFmpeg on macOS not researched in this pass.
