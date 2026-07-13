#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

project="${OBS_PROJECT:-home:kuchen:PixelKit}"
watch=false

usage() {
    cat <<'EOF'
Upload the current Debian source package to Open Build Service.

Usage:
  ./scripts/publish-obs.sh [--project PROJECT] [--watch]

The source artifacts must already exist. Build them with:
  ./scripts/build-packages.sh --clean deb-source

Options:
  --project PROJECT  OBS project (default: home:kuchen:PixelKit)
  --watch            Follow all repository builds after uploading
  -h, --help         Show this help
EOF
}

while (($#)); do
    case "$1" in
        --project)
            (($# >= 2)) || {
                printf 'error: --project requires a value\n' >&2
                exit 1
            }
            project="$2"
            shift 2
            ;;
        --watch)
            watch=true
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            printf 'error: unknown option: %s\n' "$1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

command -v osc >/dev/null || {
    printf 'error: osc is required\n' >&2
    exit 1
}

version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
debian_version="$(sed -n '1s/^pixelkit (\([^)]*\)).*/\1/p' debian/changelog)"
declare -a artifacts=(
    "dist/pixelkit_${version}.orig.tar.xz"
    "dist/pixelkit_${debian_version}.debian.tar.xz"
    "dist/pixelkit_${debian_version}.dsc"
)

for artifact in "${artifacts[@]}"; do
    [[ -f "$artifact" ]] || {
        printf 'error: missing %s\n' "$artifact" >&2
        printf 'run: ./scripts/build-packages.sh --clean deb-source\n' >&2
        exit 1
    }
done

osc api "/source/$project/pixelkit/_meta" >/dev/null || {
    printf 'error: OBS package %s/pixelkit does not exist or is inaccessible\n' \
        "$project" >&2
    exit 1
}

work="$(mktemp -d "${TMPDIR:-/tmp}/pixelkit-obs-upload.XXXXXX")"
trap 'rm -rf "$work"' EXIT
osc checkout -o "$work/pixelkit" "$project" pixelkit

find "$work/pixelkit" -maxdepth 1 -type f \
    \( -name 'pixelkit_*.orig.tar.*' -o -name 'pixelkit_*.debian.tar.*' \
    -o -name 'pixelkit_*.dsc' \) -delete
cp "${artifacts[@]}" "$work/pixelkit/"

(
    cd "$work/pixelkit"
    osc addremove
    osc commit -m "Build PixelKit $debian_version for Debian and Ubuntu"
)

printf '\nUploaded PixelKit %s to %s/pixelkit.\n' "$debian_version" "$project"
if $watch; then
    osc results "$project" pixelkit --watch
else
    printf 'Follow the builds with:\n  osc results %q pixelkit --watch\n' "$project"
fi
