# Pipster

Lightweight desktop **multi-window PiP** video player for macOS (MVP).

- **UI:** Rust + Slint (library / prefs window — no Chromium)
- **Playback:** Homebrew **mpv** as one OS process per floating window, controlled via JSON IPC
- **Formats:** video (anything mpv plays) plus images (JPEG/PNG/HEIC/TIFF/WebP/BMP). Huge images open in a tiled floating viewer (pinch to zoom / pan) built from an ImageIO tile pyramid so RAM stays low.
- **Prefs:** mute-on-spawn; always-on-top (no window limit)
- **Open:** dialog, drag-and-drop onto the main window, CLI args, and Finder **Open With** (`.app`)

> Why subprocesses? Embedding `libmpv` in the same process as Slint’s OpenGL stack
> SIGBUS’d on this macOS + Homebrew setup. The plan’s fallback is used: mpv owns the
> video windows; Pipster owns the controller UI. Still lighter than Electron.

## Requirements (macOS)

```bash
brew install mpv
# optional check:
./scripts/check_libmpv_macos.sh   # verifies mpv/libmpv is installed
```

Rust stable via `rustup`.

## Build & run

```bash
cargo run -p pipster --release
cargo run -p pipster --release -- /path/to/a.mp4 /path/to/b.mkv
cargo run -p pipster --release -- --metrics
```

## Usage

- **Open…** / **⌘O** — each file opens in a new always-on-top **mpv** window (muted by default)
- **Drag & drop** video files onto the Pipster window
- **Open With** — package the `.app`, then right-click a video in Finder → Open With → Pipster
- **⌘N** — idle mpv window
- **Pause all** — pauses every active player via IPC
- Prefs: mute-on-spawn, always-on-top (saved under Application Support)

Close an mpv window with its normal window controls; Pipster reaps dead processes.

## Package `.app`

```bash
./scripts/package_macos.sh
open dist/Pipster.app
```

The app expects `mpv` on PATH (`/opt/homebrew/bin/mpv`).

## Updates

Packaged builds check [GitHub Releases](https://github.com/htdangkhoa/pipster/releases) a few seconds after launch, and via **Pipster → Check for Updates…**. Newer `Pipster-macos*.zip` assets are downloaded and swapped into the running `.app` (then relaunched). Skip with `PIPSTER_SKIP_UPDATE=1`.

## CI / Release

- **CI** (`.github/workflows/ci.yml`): fmt, clippy, test, release build on `macos-latest`
- **Release** (`.github/workflows/release.yml`): on `v*` tags (or manual dispatch), packages `.app`, uploads `Pipster-macos.zip` + arch-specific zip to the GitHub Release

```bash
# cut a release
git tag v0.2.0
git push origin v0.2.0
```

## Bake-off

Open the same 1080p file in 1 / 4 / 8 windows and compare Memory + CPU in Activity Monitor for `Pipster` and the `mpv` children.

## Layout

```
crates/pipster/            # Slint UI + menu + orchestration
crates/pipster-player/     # mpv subprocess pool + IPC
crates/pipster-governor/   # decode caps + low-RAM mpv flags
```

## License

MIT — see [LICENSE](LICENSE) and [NOTICE](NOTICE).
