#!/usr/bin/env bash
# scripts/gui_acceptance.sh - Native OS Window Desktop Acceptance Test for Nudge UI
#
# Runs the real OS window acceptance test outside normal CI (requires a display).
# Prefers Xvfb (xvfb-run -a) for automated headless display testing.
# Falls back to active DISPLAY/WAYLAND_DISPLAY if xvfb-run is unavailable.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

echo "=========================================================="
echo " Nudge Native GUI Desktop Acceptance Test Suite"
echo "=========================================================="

# Check for display server or virtual display tool
if command -v xvfb-run >/dev/null 2>&1; then
    echo "==> Using virtual display runner: xvfb-run -a"
    xvfb-run -a cargo test -p nudge-runtime --features native-gui test_native_gui_desktop_acceptance -- --ignored --nocapture "$@"
elif [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
    echo "==> Note: xvfb-run not found; running with active display (DISPLAY=${DISPLAY:-}, WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-})"
    cargo test -p nudge-runtime --features native-gui test_native_gui_desktop_acceptance -- --ignored --nocapture "$@"
else
    echo "ERROR: Neither xvfb-run nor an active DISPLAY/WAYLAND_DISPLAY was found." >&2
    echo "This test exercises real OS window creation (winit + softbuffer) and requires a display." >&2
    echo "To run headlessly, install Xvfb (e.g. 'sudo apt-get install xvfb')." >&2
    echo "Or launch in a desktop session with DISPLAY=:0." >&2
    exit 1
fi

echo "=========================================================="
echo " Desktop GUI Acceptance Test PASSED successfully!"
echo "=========================================================="
