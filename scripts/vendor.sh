#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
rm -rf vendor .cargo
mkdir -p .cargo
quiet=()
if [[ "${PIXELKIT_VENDOR_QUIET:-0}" == "1" ]]; then
    quiet=(--quiet)
fi
cargo vendor "${quiet[@]}" --locked --versioned-dirs vendor >/dev/null
{
    printf '%s\n' \
        '[source.crates-io]' \
        'replace-with = "vendored-sources"' \
        '' \
        '[source.vendored-sources]' \
        'directory = "vendor"'
} > .cargo/config.toml
echo "Vendored Cargo dependencies into vendor/."
