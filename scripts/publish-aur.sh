#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

aur_url="ssh://aur@aur.archlinux.org/pixelkit.git"
aur_dir="${AUR_DIR:-$(dirname "$root")/PixelKit-AUR}"
arch_image="${ARCH_IMAGE:-docker.io/library/archlinux:latest}"
push=true
requested_version=""

log() {
    printf '\n==> %s\n' "$*"
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Update, verify, commit, and push the PixelKit AUR package.

Usage:
  ./scripts/publish-aur.sh [OPTIONS] [VERSION]

VERSION defaults to the version in Cargo.toml. The matching GitHub release and
SHA256SUMS must already be public.

Options:
  --aur-dir DIR    AUR checkout (default: adjacent PixelKit-AUR directory)
  --image IMAGE    Arch container image
                   (default: docker.io/library/archlinux:latest)
  --no-push        Prepare and verify PKGBUILD/.SRCINFO without committing
  -h, --help       Show this help

Environment:
  AUR_DIR          Alternative default AUR checkout
  ARCH_IMAGE       Alternative default Arch container image

The script clones the AUR repository when DIR does not exist. By default it
commits "Update to VERSION" and pushes after every validation succeeds.
Recipe changes beyond a release version/checksum update still require a full
Arch package build and namcap review.
EOF
}

while (($#)); do
    case "$1" in
        --aur-dir)
            (($# >= 2)) || die "--aur-dir requires a value"
            aur_dir="$2"
            shift 2
            ;;
        --image)
            (($# >= 2)) || die "--image requires a value"
            arch_image="$2"
            shift 2
            ;;
        --no-push)
            push=false
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        --*)
            die "unknown option: $1"
            ;;
        *)
            [[ -z "$requested_version" ]] || die "only one VERSION may be specified"
            requested_version="$1"
            shift
            ;;
    esac
done

for required_command in git curl awk sed podman realpath; do
    command -v "$required_command" >/dev/null || die "$required_command is required"
done

repo_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
[[ "$repo_version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
    die "Cargo.toml does not contain a stable X.Y.Z version"
version="${requested_version:-$repo_version}"
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
    die "VERSION must use stable X.Y.Z form"
[[ "$version" == "$repo_version" ]] ||
    die "requested version $version does not match Cargo.toml ($repo_version)"
git rev-parse --verify --quiet "refs/tags/v${version}^{commit}" >/dev/null ||
    die "local tag v$version is missing; publish the GitHub release first"

if [[ ! -e "$aur_dir" ]]; then
    log "Cloning the PixelKit AUR repository"
    mkdir -p "$(dirname "$aur_dir")"
    git clone "$aur_url" "$aur_dir"
fi
[[ -d "$aur_dir/.git" ]] || die "$aur_dir is not a Git checkout"
aur_dir="$(realpath "$aur_dir")"

origin="$(git -C "$aur_dir" remote get-url origin 2>/dev/null)" ||
    die "$aur_dir has no origin remote"
[[ "$origin" == *"aur.archlinux.org/pixelkit.git" ]] ||
    die "$aur_dir origin is not the PixelKit AUR repository: $origin"
[[ -z "$(git -C "$aur_dir" status --porcelain=v1)" ]] ||
    die "$aur_dir has uncommitted changes"

log "Updating the AUR checkout"
git -C "$aur_dir" pull --ff-only
[[ -f "$aur_dir/PKGBUILD" ]] || die "$aur_dir/PKGBUILD is missing"
[[ -f "$aur_dir/.SRCINFO" ]] || die "$aur_dir/.SRCINFO is missing"

archive="pixelkit-${version}-vendor.tar.xz"
checksums_url="https://github.com/Kuucheen/PixelKit/releases/download/v${version}/SHA256SUMS"
log "Reading the published checksum for $archive"
checksums="$(curl --fail --silent --show-error --location --retry 3 "$checksums_url")"
mapfile -t matches < <(
    awk -v file="$archive" '$2 == file || $2 == "*" file { print $1 }' <<<"$checksums"
)
((${#matches[@]} == 1)) ||
    die "expected one checksum for $archive in $checksums_url"
sha="${matches[0],,}"
[[ "$sha" =~ ^[0-9a-f]{64}$ ]] || die "published checksum is not SHA-256: $sha"

replace_assignment() {
    local key="$1"
    local value="$2"
    local file="$3"
    local count
    count="$(grep -c "^${key}=" "$file" || true)"
    [[ "$count" == 1 ]] || die "expected one $key assignment in $file"
    sed -i "s|^${key}=.*|${key}=${value}|" "$file"
}

replace_assignment pkgver "$version" "$aur_dir/PKGBUILD"
replace_assignment pkgrel 1 "$aur_dir/PKGBUILD"
replace_assignment sha256sums "('$sha')" "$aur_dir/PKGBUILD"

work="$(mktemp -d "${TMPDIR:-/tmp}/pixelkit-aur-publish.XXXXXX")"
trap 'rm -rf "$work"' EXIT

run_arch_container() {
    podman run --rm --userns=keep-id \
        -e SRCDEST=/tmp \
        -e BUILDDIR=/tmp \
        -e PKGDEST=/tmp \
        -v "$aur_dir:/pkg:ro,Z" \
        -w /pkg \
        "$arch_image" \
        "$@"
}

log "Generating .SRCINFO with $arch_image"
run_arch_container makepkg --printsrcinfo >"$work/.SRCINFO"
[[ -s "$work/.SRCINFO" ]] || die "makepkg produced an empty .SRCINFO"
install -m 0644 "$work/.SRCINFO" "$aur_dir/.SRCINFO"

grep -Fqx $'\tpkgver = '"$version" "$aur_dir/.SRCINFO" ||
    die ".SRCINFO does not contain pkgver $version"
grep -Fqx $'\tsha256sums = '"$sha" "$aur_dir/.SRCINFO" ||
    die ".SRCINFO does not contain the published checksum"

log "Verifying the published source archive"
run_arch_container makepkg --verifysource --noconfirm

git -C "$aur_dir" diff --check
while IFS= read -r path; do
    [[ -z "$path" ]] && continue
    case "$path" in
        PKGBUILD | .SRCINFO) ;;
        *) die "unexpected changed file in AUR checkout: $path" ;;
    esac
done < <(git -C "$aur_dir" status --porcelain=v1 | cut -c4-)

if git -C "$aur_dir" diff --quiet -- PKGBUILD .SRCINFO; then
    printf '\nPixelKit %s is already current in %s.\n' "$version" "$aur_dir"
    exit 0
fi

git -C "$aur_dir" diff -- PKGBUILD .SRCINFO

if ! $push; then
    printf '\nPrepared and verified PixelKit %s in %s.\n' "$version" "$aur_dir"
    printf 'Review, commit, and push the two changed files when ready.\n'
    exit 0
fi

log "Publishing PixelKit $version to the AUR"
git -C "$aur_dir" add -- PKGBUILD .SRCINFO
git -C "$aur_dir" diff --cached --check
git -C "$aur_dir" commit -m "Update to $version"
git -C "$aur_dir" push
printf '\nPublished PixelKit %s to the AUR.\n' "$version"
