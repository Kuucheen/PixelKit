#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
command -v linuxdeploy >/dev/null 2>&1 || {
    echo "linuxdeploy is required: https://github.com/linuxdeploy/linuxdeploy" >&2
    exit 1
}

cargo build --release --locked
rm -rf AppDir
make install DESTDIR="$PWD/AppDir" PREFIX=/usr CARGO="cargo --offline"

export OUTPUT="${OUTPUT:-PixelKit-$(uname -m).AppImage}"
linuxdeploy \
    --appdir AppDir \
    --desktop-file packaging/linux/io.github.Kuucheen.PixelKit.desktop \
    --icon-file packaging/linux/512x512/io.github.Kuucheen.PixelKit.png \
    --output appimage
