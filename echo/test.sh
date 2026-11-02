#!/usr/bin/env bash
# Tests everything that doesn't need a display/GUI. Run ./install.sh
# first if you haven't already built.

cd "$(dirname "${BASH_SOURCE[0]}")"
FAILED=0

section() { echo ""; echo "=== $1 ==="; }

section "GStreamer plugin availability (system-level, independent of Rust build)"
check_element() {
    if gst-inspect-1.0 "$1" >/dev/null 2>&1; then
        echo "  OK       $1"
    else
        echo "  MISSING  $1  ($2)"
        if [ "$3" = "required" ]; then FAILED=1; fi
    fi
}
check_element "playbin" "core playback - required" "required"
check_element "audioconvert" "core - required" "required"
check_element "autoaudiosink" "core - required" "required"
check_element "spectrum" "core - required" "required"
check_element "capsfilter" "core - required" "required"
check_element "pitch" "speed/pitch control - optional, from gst-plugins-bad's soundtouch" "optional"
check_element "freeverb" "Concert Hall reverb - optional, from gst-plugins-bad" "optional"
check_element "bs2b" "Concert Hall crossfeed - optional, from gst-plugins-bad" "optional"

section "cargo check (fast type/syntax check of the whole crate)"
if cargo check --release; then
    echo "cargo check: OK"
else
    echo "cargo check: FAILED - see compiler output above."
    echo "This is genuinely the first time this code has been checked by a"
    echo "real compiler (see docs/TODO.md) - paste the exact error, not a"
    echo "paraphrase, for a fix."
    FAILED=1
fi

if [ "$FAILED" = "1" ]; then
    echo ""
    echo "Stopping before the self-test binary check since cargo check"
    echo "already failed - fix compile errors first."
    exit 1
fi

section "Binary self-test (GStreamer init, depth model download+checksum, depth engine - no display needed)"
if cargo run --release -- --self-test; then
    echo "self-test: PASSED"
else
    echo "self-test: FAILED - see output above."
    FAILED=1
fi

section "Result"
if [ "$FAILED" = "1" ]; then
    echo "One or more checks failed. This does NOT test the actual window/webview"
    echo "(wry/tao) - that needs a real display and isn't covered by this script."
    echo "The window/webview is also the single highest-risk, least-tested part"
    echo "of the whole stack (see docs/ARCHITECTURE_DECISIONS.md's Wayland pitfall"
    echo "section) - once everything above passes, run 'cargo run --release'"
    echo "for real (with a display) and check whether a window actually appears."
    exit 1
else
    echo "All non-GUI checks passed."
    echo "Next: run 'cargo run --release' (needs a real display) to test the"
    echo "actual window/webview - the highest-risk, least-tested part of the stack."
    exit 0
fi
