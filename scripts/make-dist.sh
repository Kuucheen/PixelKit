#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
name="pixelkit-${version}"
output="${PWD}/dist"
staging="$(mktemp -d)"
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}"
trap 'rm -rf "$staging"' EXIT

mkdir -p "$output" "$staging/$name"
tar --exclude-vcs --exclude=target --exclude=dist --exclude=vendor --exclude=.cargo \
    --exclude=.flatpak-builder --exclude=.idea --exclude=AppDir \
    --exclude='*.AppImage' \
    -cf - . | tar -xf - -C "$staging/$name"
(
    cd "$staging/$name"
    PIXELKIT_VENDOR_QUIET=1 ./scripts/vendor.sh
    test -s .cargo/config.toml
    mkdir -p "$staging/cargo-home"
    CARGO_HOME="$staging/cargo-home" CARGO_NET_OFFLINE=true \
        cargo fetch --locked --offline
)
tar --sort=name --mtime="@$source_date_epoch" --owner=0 --group=0 --numeric-owner \
    -C "$staging" -cJf "$output/${name}-vendor.tar.xz" "$name"
sha256sum "$output/${name}-vendor.tar.xz"
