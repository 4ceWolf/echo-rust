#!/usr/bin/env bash
set -e

# Arch package names below were checked against real package pages
# before writing this, not assumed - see docs/ARCHITECTURE_DECISIONS.md
# for sources. The one real trap: `webkit2gtk` (no version suffix) is an
# AUR package intentionally pinned to the old 2.50.x WebKit1-era branch -
# `webkit2gtk-4.1` is the current official [extra] repo package and the
# one this project actually needs (confirmed against a real, current
# wry-based app's own documented Arch install steps).

echo "Installing system dependencies (pacman)..."
sudo pacman -S --needed --noconfirm \
    base-devel \
    rust \
    gtk3 \
    webkit2gtk-4.1 \
    gstreamer \
    gst-plugins-base \
    gst-plugins-good \
    gst-plugins-bad \
    gst-plugins-ugly \
    gst-libav \
    openssl

echo ""
echo "Building (release mode - this will take a while the first time,"
echo "especially compiling gstreamer-rs and ort's dependencies)..."
cd "$(dirname "${BASH_SOURCE[0]}")"
cargo build --release

echo ""
if [ -f "target/release/echo" ]; then
    echo "Build succeeded: target/release/echo"
else
    echo "Build finished but target/release/echo wasn't found - check the"
    echo "output above for the actual error. This is the FIRST time this"
    echo "code has ever been compiled (see docs/TODO.md's 'Build"
    echo "verification constraint' note) - a real compile error here is"
    echo "expected, not a sign the plan is wrong. Paste the exact error"
    echo "output, not a paraphrase, when asking for a fix."
    exit 1
fi

echo ""
echo "Run ./test.sh next to check things without needing a GUI."
