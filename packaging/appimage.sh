#!/usr/bin/env bash
# VerseKit - builds the AppImage for Linux.
# Copyright (C) 2026 Xharig
# SPDX-License-Identifier: GPL-3.0-only
#
# Runs in a container with Ubuntu 22.04 (glibc 2.35, so that the AppImage also starts on older systems):
#
#   docker run --rm -v "$PWD":/src -w /src ubuntu:22.04 packaging/appimage.sh --all
#
#   --tools   install the build tools and Rust (fixed version) into the container
#   --build   build the program (cargo build --release --locked)
#   --pack    pack target/release/versekit into dist/VerseKit-x86_64.AppImage
#   --all     the three in this order
#
# AppImageTool and its runtime are fetched in fixed versions and checked against the checksums below before
# they are used; the program file may only depend on the C library of the system (checked with ldd).

set -euo pipefail

RUST_VERSION='1.99.1'
RUSTUP_VERSION='1.29.1'
RUSTUP_SHA256='dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71'
APPIMAGETOOL_VERSION='1.9.1'
APPIMAGETOOL_SHA256='ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0'
RUNTIME_VERSION='20251108'
RUNTIME_SHA256='2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d'
OUT='dist/VerseKit-x86_64.AppImage'
# The libraries the program file may be linked against; everything else it opens at run time when it is there
# (graphics, windows, keyboard), so that a missing one does not stop the start.
ALLOWED_LIBRARIES='linux-vdso|ld-linux-x86-64|libc\.so|libm\.so|libgcc_s\.so|libpthread\.so|libdl\.so|librt\.so|libutil\.so'

tools() {
    export DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends \
        build-essential pkg-config ca-certificates curl wget file git openssh-client xvfb \
        libx11-6 libx11-xcb1 libxcursor1 libxi6 libxrandr2 libxkbcommon-x11-0 libgl1 libegl1
    if ! command -v rustup >/dev/null 2>&1; then
        fetch_checked "https://static.rust-lang.org/rustup/archive/$RUSTUP_VERSION/x86_64-unknown-linux-gnu/rustup-init" \
            /tmp/rustup-init "$RUSTUP_SHA256"
        chmod +x /tmp/rustup-init
        /tmp/rustup-init -y --profile minimal --default-toolchain "$RUST_VERSION"
    fi
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
    rustup toolchain install "$RUST_VERSION" --profile minimal
    rustc +"$RUST_VERSION" --version
}

build() {
    # shellcheck disable=SC1091
    [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
    cargo +"$RUST_VERSION" build --release --locked -p versekit-app
}

check_libraries() {
    local binary="$1" foreign
    foreign="$(ldd "$binary" | grep -Ev "$ALLOWED_LIBRARIES" || true)"
    if [ -n "$foreign" ]; then
        echo "The program file depends on libraries the AppImage does not bring:" >&2
        echo "$foreign" >&2
        return 1
    fi
}

fetch_checked() {
    local url="$1" file="$2" sum="$3"
    wget -q -O "$file" "$url"
    echo "$sum  $file" | sha256sum -c -
}

pack() {
    local binary='target/release/versekit' work appdir
    [ -x "$binary" ] || { echo "missing: $binary" >&2; return 1; }
    check_libraries "$binary"
    work="$(mktemp -d)"
    appdir="$work/VerseKit.AppDir"
    mkdir -p "$appdir/usr/bin" "$(dirname "$OUT")"
    install -m 755 "$binary" "$appdir/usr/bin/versekit"
    install -m 644 assets/icon.png "$appdir/versekit.png"
    install -m 644 packaging/versekit.desktop "$appdir/versekit.desktop"
    install -m 755 packaging/AppRun "$appdir/AppRun"
    (
        cd "$work"
        fetch_checked "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL_VERSION/appimagetool-x86_64.AppImage" \
            appimagetool "$APPIMAGETOOL_SHA256"
        fetch_checked "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME_VERSION/runtime-x86_64" \
            runtime "$RUNTIME_SHA256"
        chmod +x appimagetool
    )
    # No FUSE in a container: the tool unpacks itself and runs.
    ARCH=x86_64 "$work/appimagetool" --appimage-extract-and-run --runtime-file "$work/runtime" "$appdir" "$OUT"
    rm -rf "$work"
    ls -l "$OUT"
    sha256sum "$OUT"
}

case "${1:-}" in
    --tools) tools ;;
    --build) build ;;
    --pack) pack ;;
    --all) tools && build && pack ;;
    *) sed -n '2,16p' "$0"; exit 2 ;;
esac
