#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
rm -rf vendor .cargo
mkdir -p .cargo
cargo vendor --locked --versioned-dirs vendor > .cargo/config.toml
echo "Vendored Cargo dependencies into vendor/."
