#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
name="pixelkit-${version}"
output="${PWD}/dist"
staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT

mkdir -p "$output" "$staging/$name"
tar --exclude-vcs --exclude=target --exclude=dist --exclude=vendor --exclude=.cargo \
    -cf - . | tar -xf - -C "$staging/$name"
(
    cd "$staging/$name"
    ./scripts/vendor.sh
)
tar -C "$staging" -cJf "$output/${name}-vendor.tar.xz" "$name"
sha256sum "$output/${name}-vendor.tar.xz"
