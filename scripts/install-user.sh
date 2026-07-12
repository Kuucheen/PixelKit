#!/usr/bin/env bash
set -euo pipefail

prefix="${PREFIX:-$HOME/.local}"
repo="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo"
cargo build --locked --release
user_unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
make install PREFIX="$prefix" SYSTEMD_USER_UNIT_DIR="$user_unit_dir"
sed -i "s|ExecStart=/usr/bin/pixelkit|ExecStart=$prefix/bin/pixelkit|" "$user_unit_dir/pixelkit.service"

if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload
    echo "Enable shortcuts with: systemctl --user enable --now pixelkit.service"
else
    autostart="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
    mkdir -p "$autostart"
    install -Dm644 packaging/linux/pixelkit-autostart.desktop "$autostart/pixelkit-autostart.desktop"
    echo "Installed desktop autostart entry."
fi
