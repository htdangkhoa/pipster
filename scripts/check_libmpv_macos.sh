#!/usr/bin/env bash
set -euo pipefail

if [[ -x /opt/homebrew/bin/mpv ]]; then
  echo "mpv: $(/opt/homebrew/bin/mpv --version | head -1)"
elif command -v mpv >/dev/null; then
  echo "mpv: $(mpv --version | head -1)"
else
  echo "mpv not found. Install with: brew install mpv" >&2
  exit 1
fi

if [[ -f /opt/homebrew/lib/libmpv.dylib ]]; then
  echo "libmpv dylib present (optional for IPC mode)"
fi
