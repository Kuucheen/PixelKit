#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

app_id="io.github.Kuucheen.PixelKit"
dist="$root/dist"
skip_checks=false
clean=false
bump_message=""
set_version=""
list_only=false
explicit_formats=false
declare -a requested=()
declare -a artifacts=()
rpm_dependencies_checked=false
rpm_dependencies_available=false
declare -a missing_rpm_dependencies=()

log() {
    printf '\n==> %s\n' "$*"
}

warn() {
    printf 'warning: %s\n' "$*" >&2
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Build PixelKit release artifacts and generate dist/SHA256SUMS.

Usage:
  ./scripts/build-packages.sh [OPTIONS] [FORMAT ...]

Formats:
  source      Vendored, offline source archive
  portable    Portable binary tarball
  rpm         Fedora/RHEL binary RPM and source RPM
  opensuse    openSUSE binary RPM and source RPM
  deb         Debian/Ubuntu package through cargo-deb
  deb-source  Debian source package for OBS and Debian/Ubuntu builders
  arch        Arch package through makepkg
  flatpak     Single-file Flatpak bundle
  appimage    AppImage through linuxdeploy
  snap        Snap through snapcraft
  nix         Validate the Nix package build
  all         Every format whose native builder is available

With no FORMAT, the script builds source, portable, and every package format
whose native builder is available on this host. Naming a format explicitly
makes a missing builder an error.

Options:
  --bump MESSAGE   Increment RPM, Debian, and Arch package revisions first
  --set-version X.Y.Z
                   Set a new upstream version and reset package revisions
  --skip-checks    Skip tests and metadata validation
  --clean          Remove previous generated package output before building
  --list           Show which formats can be built on this host
  -h, --help       Show this help

Examples:
  ./scripts/build-packages.sh
  ./scripts/build-packages.sh rpm deb
  ./scripts/build-packages.sh --clean deb-source
  ./scripts/build-packages.sh --bump "Fix picker tooltip sizing"
  ./scripts/build-packages.sh --clean --set-version 0.1.1
EOF
}

host_ids() {
    if [[ -r /etc/os-release ]]; then
        (
            # The distribution owns this shell-compatible metadata file.
            # shellcheck disable=SC1091
            source /etc/os-release
            printf '%s %s\n' "${ID:-}" "${ID_LIKE:-}"
        )
    fi
}

has_cargo_deb() {
    cargo deb --version >/dev/null 2>&1
}

has_rpm_build_dependencies() {
    if $rpm_dependencies_checked; then
        $rpm_dependencies_available
        return
    fi

    rpm_dependencies_checked=true
    local dependency
    for dependency in rust cargo gcc make libX11-devel libxcb-devel \
        libxkbcommon-devel wayland-devel; do
        if ! rpm -q "$dependency" >/dev/null 2>&1; then
            missing_rpm_dependencies+=("$dependency")
        fi
    done
    if ((${#missing_rpm_dependencies[@]} == 0)); then
        rpm_dependencies_available=true
    fi
    $rpm_dependencies_available
}

format_available() {
    local format="$1"
    local ids
    ids=" $(host_ids) "
    case "$format" in
        source | portable | deb-source)
            command -v cargo >/dev/null && command -v tar >/dev/null
            ;;
        rpm)
            command -v rpmbuild >/dev/null &&
                [[ "$ids" == *" fedora "* || "$ids" == *" rhel "* ||
                    "$ids" == *" centos "* ]] &&
                has_rpm_build_dependencies
            ;;
        opensuse)
            command -v rpmbuild >/dev/null &&
                [[ "$ids" == *" opensuse "* || "$ids" == *" suse "* ]] &&
                has_rpm_build_dependencies
            ;;
        deb) command -v cargo >/dev/null && has_cargo_deb ;;
        arch)
            command -v makepkg >/dev/null &&
                [[ "$ids" == *" arch "* ]]
            ;;
        flatpak) command -v flatpak-builder >/dev/null && command -v flatpak >/dev/null ;;
        appimage) command -v linuxdeploy >/dev/null ;;
        snap) command -v snapcraft >/dev/null ;;
        nix) command -v nix >/dev/null ;;
        *) return 1 ;;
    esac
}

missing_tool_hint() {
    local ids
    ids=" $(host_ids) "
    case "$1" in
        rpm)
            if [[ "$ids" == *" fedora "* || "$ids" == *" rhel "* ||
                "$ids" == *" centos "* ]] && ((${#missing_rpm_dependencies[@]} > 0)); then
                printf 'missing RPM build packages: %s (run: sudo dnf builddep packaging/rpm/pixelkit.spec)' \
                    "${missing_rpm_dependencies[*]}"
            else
                printf 'rpmbuild on Fedora/RHEL'
            fi
            ;;
        opensuse) printf 'rpmbuild on openSUSE' ;;
        deb) printf 'cargo-deb (install with: cargo install cargo-deb --locked)' ;;
        arch) printf 'makepkg on Arch Linux' ;;
        flatpak) printf 'flatpak-builder and flatpak' ;;
        appimage) printf 'linuxdeploy' ;;
        snap) printf 'snapcraft' ;;
        nix) printf 'nix' ;;
        *) printf 'cargo and standard archive tools' ;;
    esac
}

show_formats() {
    local format
    for format in source portable rpm opensuse deb deb-source arch flatpak appimage snap nix; do
        if format_available "$format"; then
            printf '%-10s available\n' "$format"
        else
            printf '%-10s unavailable (%s)\n' "$format" "$(missing_tool_hint "$format")"
        fi
    done
}

while (($#)); do
    case "$1" in
        --bump)
            (($# >= 2)) || die "--bump requires a changelog message"
            bump_message="$2"
            shift 2
            ;;
        --set-version)
            (($# >= 2)) || die "--set-version requires an X.Y.Z version"
            set_version="$2"
            shift 2
            ;;
        --skip-checks)
            skip_checks=true
            shift
            ;;
        --clean)
            clean=true
            shift
            ;;
        --list)
            list_only=true
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        --*) die "unknown option: $1" ;;
        *)
            requested+=("$1")
            explicit_formats=true
            shift
            ;;
    esac
done

if [[ -n "$bump_message" && -n "$set_version" ]]; then
    die "--bump and --set-version cannot be used together"
fi

if $list_only; then
    show_formats
    exit 0
fi

all_formats=(source portable rpm opensuse deb deb-source arch flatpak appimage snap nix)
if ((${#requested[@]} == 0)); then
    requested=(source portable)
    for format in rpm opensuse deb deb-source arch flatpak appimage snap nix; do
        if format_available "$format"; then
            requested+=("$format")
        else
            warn "skipping $format: $(missing_tool_hint "$format")"
        fi
    done
elif [[ " ${requested[*]} " == *" all "* ]]; then
    requested=()
    explicit_formats=false
    for format in "${all_formats[@]}"; do
        if format_available "$format"; then
            requested+=("$format")
        else
            warn "skipping $format: $(missing_tool_hint "$format")"
        fi
    done
fi

for required_command in python3 sha256sum realpath; do
    command -v "$required_command" >/dev/null || die "$required_command is required"
done

declare -A seen=()
declare -a formats=()
for format in "${requested[@]}"; do
    case "$format" in
        source | portable | rpm | opensuse | deb | deb-source | arch | flatpak | appimage | snap | nix) ;;
        *) die "unknown format: $format (use --help for the supported list)" ;;
    esac
    if $explicit_formats && ! format_available "$format"; then
        die "$format unavailable: $(missing_tool_hint "$format")"
    fi
    if [[ -z "${seen[$format]:-}" ]]; then
        formats+=("$format")
        seen[$format]=1
    fi
done

if [[ -n "$set_version" ]]; then
    log "Setting upstream version to $set_version"
    python3 scripts/set-version.py "$set_version"
elif [[ -n "$bump_message" ]]; then
    log "Incrementing distro package revisions"
    python3 scripts/bump-package-release.py "$bump_message"
fi

version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
[[ -n "$version" ]] || die "could not read the version from Cargo.toml"

check_version() {
    local label="$1"
    local actual="$2"
    [[ "$actual" == "$version" ]] ||
        die "$label version is $actual, but Cargo.toml is $version"
}

rpm_version="$(sed -n 's/^Version:[[:space:]]*//p' packaging/rpm/pixelkit.spec | head -1)"
opensuse_version="$(sed -n 's/^Version:[[:space:]]*//p' packaging/opensuse/pixelkit.spec | head -1)"
arch_version="$(sed -n 's/^pkgver=//p' packaging/arch/PKGBUILD | head -1)"
snap_version="$(sed -n "s/^version: '\([^']*\)'/\1/p" snap/snapcraft.yaml | head -1)"
nix_version="$(sed -n 's/^[[:space:]]*version = "\([^"]*\)";/\1/p' flake.nix | head -1)"
flatpak_version="$(sed -n 's/^[[:space:]]*tag: v//p' packaging/flatpak/io.github.Kuucheen.PixelKit.yml | head -1)"
appstream_version="$(sed -n 's/.*<release version="\([^"]*\)".*/\1/p' packaging/linux/io.github.Kuucheen.PixelKit.metainfo.xml | head -1)"
debian_version="$(sed -n '1s/^pixelkit (\([^)]*\)).*/\1/p' debian/changelog)"
debian_upstream="${debian_version%-*}"

check_version "Fedora RPM" "$rpm_version"
check_version "openSUSE RPM" "$opensuse_version"
check_version "Arch" "$arch_version"
check_version "Snap" "$snap_version"
check_version "Nix" "$nix_version"
check_version "Flatpak" "$flatpak_version"
check_version "AppStream" "$appstream_version"
check_version "Debian" "$debian_upstream"

case "$(uname -m)" in
    x86_64 | amd64)
        package_arch="x86_64"
        deb_arch="amd64"
        ;;
    aarch64 | arm64)
        package_arch="aarch64"
        deb_arch="arm64"
        ;;
    *)
        package_arch="$(uname -m)"
        deb_arch="$(uname -m)"
        ;;
esac

if $clean; then
    log "Removing previous generated package output"
    rm -rf -- "$dist" "$root/target/package" "$root/AppDir"
    find "$root" -maxdepth 1 -type f -name 'PixelKit-*.AppImage' -delete
fi

mkdir -p "$dist"
work="$(mktemp -d "${TMPDIR:-/tmp}/pixelkit-packages.XXXXXX")"
trap 'rm -rf "$work"' EXIT

log "Refreshing Flatpak Cargo source hashes"
python3 packaging/flatpak/generate-cargo-sources.py \
    Cargo.lock "$work/cargo-sources.json" >/dev/null
if cmp -s "$work/cargo-sources.json" packaging/flatpak/cargo-sources.json; then
    printf 'Flatpak source hashes are current.\n'
else
    install -m644 "$work/cargo-sources.json" packaging/flatpak/cargo-sources.json
    printf 'Updated packaging/flatpak/cargo-sources.json.\n'
fi

if ! $skip_checks; then
    log "Running pre-package checks"
    if cargo fmt --version >/dev/null 2>&1; then
        cargo fmt --all -- --check
    else
        warn "cargo-fmt is unavailable; CI will still enforce formatting"
    fi
    if cargo clippy --version >/dev/null 2>&1; then
        cargo clippy --all-targets --locked -- -D warnings
    else
        warn "cargo-clippy is unavailable; CI will still enforce linting"
    fi
    cargo test --all-targets --locked
    if command -v appstreamcli >/dev/null; then
        appstreamcli validate --no-net \
            packaging/linux/io.github.Kuucheen.PixelKit.metainfo.xml
    else
        warn "appstreamcli is unavailable; skipping AppStream validation"
    fi
    if command -v desktop-file-validate >/dev/null; then
        desktop-file-validate \
            packaging/linux/io.github.Kuucheen.PixelKit.desktop \
            packaging/linux/pixelkit-autostart.desktop
    else
        warn "desktop-file-validate is unavailable; skipping desktop validation"
    fi
fi

release_built=false
source_built=false

record_artifact() {
    local path="$1"
    [[ -f "$path" ]] || die "expected artifact was not created: $path"
    artifacts+=("$(realpath "$path")")
}

ensure_release_build() {
    if ! $release_built; then
        log "Building the release binary"
        cargo build --release --locked
        release_built=true
    fi
}

build_source() {
    local archive="$dist/pixelkit-${version}-vendor.tar.xz"
    if ! $source_built; then
        log "Building the vendored source archive"
        ./scripts/make-dist.sh
        source_built=true
    fi
    record_artifact "$archive"
}

build_portable() {
    ensure_release_build
    local name="pixelkit-${version}-linux-${package_arch}"
    local stage="$work/$name"
    local archive="$dist/$name.tar.xz"
    local epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}"
    log "Building the portable archive"
    mkdir -p "$stage"
    install -m755 target/release/pixelkit "$stage/pixelkit"
    install -m644 README.md LICENSE NOTICE "$stage/"
    tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
        -C "$work" -cJf "$archive" "$name"
    record_artifact "$archive"
}

build_rpm() {
    local label="$1"
    local slug="$2"
    local spec="$3"
    local top="$work/rpmbuild-$slug"
    local rpm_options=()
    if $skip_checks; then
        rpm_options+=(--nocheck)
    fi
    build_source
    log "Building the $label RPM and source RPM"
    mkdir -p "$top"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
    cp "$dist/pixelkit-${version}-vendor.tar.xz" "$top/SOURCES/"
    cp "$spec" "$top/SPECS/pixelkit.spec"
    rpmbuild "${rpm_options[@]}" --define "_topdir $top" -ba "$top/SPECS/pixelkit.spec"
    while IFS= read -r -d '' package; do
        local destination="$dist/$(basename "$package")"
        cp "$package" "$destination"
        record_artifact "$destination"
    done < <(find "$top/RPMS" "$top/SRPMS" -type f -name '*.rpm' -print0)
}

build_deb() {
    ensure_release_build
    local package="$dist/pixelkit_${debian_version}_${deb_arch}.deb"
    log "Building the Debian package"
    cargo deb --locked --no-build --deb-version "$debian_version" --output "$package"
    record_artifact "$package"
}

build_deb_source() {
    build_source
    log "Building the Debian source package"
    ./scripts/make-debian-source.sh
    record_artifact "$dist/pixelkit_${version}.orig.tar.xz"
    record_artifact "$dist/pixelkit_${debian_version}.debian.tar.xz"
    record_artifact "$dist/pixelkit_${debian_version}.dsc"
}

build_arch() {
    local pkgrel
    local source_archive="$dist/pixelkit-${version}-vendor.tar.xz"
    local source_name="$(basename "$source_archive")"
    local package_root="$work/arch-package"
    local makepkg_options=(--cleanbuild --force --noconfirm)
    if $skip_checks; then
        makepkg_options+=(--nocheck)
    fi
    pkgrel="$(sed -n 's/^pkgrel=//p' packaging/arch/PKGBUILD | head -1)"
    build_source
    log "Building the Arch package"
    mkdir -p "$package_root" "$work/arch-build"
    cp packaging/arch/PKGBUILD "$package_root/PKGBUILD"
    cp "$source_archive" "$package_root/$source_name"
    local source_hash
    source_hash="$(sha256sum "$source_archive" | cut -d' ' -f1)"
    sed -i \
        -e "s|^source=.*|source=(\"$source_name\")|" \
        -e "s|^sha256sums=.*|sha256sums=('$source_hash')|" \
        -e 's|cd "$pkgname"|cd "$pkgname-$pkgver"|g' \
        "$package_root/PKGBUILD"
    bash -n "$package_root/PKGBUILD"
    (
        cd "$package_root"
        PKGDEST="$dist" SRCDEST="$package_root" BUILDDIR="$work/arch-build" \
            makepkg "${makepkg_options[@]}"
    )
    local matches=("$dist/pixelkit-${version}-${pkgrel}-"*.pkg.tar.*)
    ((${#matches[@]} > 0)) || die "makepkg did not create an Arch package"
    local package
    for package in "${matches[@]}"; do
        record_artifact "$package"
    done
}

build_flatpak() {
    local repository="$work/flatpak-repo"
    local build_dir="$work/flatpak-build"
    local bundle="$dist/${app_id}-${version}-${package_arch}.flatpak"
    log "Building the Flatpak bundle"
    flatpak-builder --force-clean --repo="$repository" \
        "$build_dir" packaging/flatpak/io.github.Kuucheen.PixelKit.local.yml
    flatpak build-bundle "$repository" "$bundle" "$app_id"
    record_artifact "$bundle"
}

build_appimage() {
    ensure_release_build
    local package="$dist/PixelKit-${version}-${package_arch}.AppImage"
    log "Building the AppImage"
    OUTPUT="$package" ./packaging/appimage/build-appimage.sh
    record_artifact "$package"
}

build_snap() {
    local output="$work/snap-output"
    log "Building the Snap"
    mkdir -p "$output"
    snapcraft pack --output "$output" "$root"
    local matches=("$output/"*.snap)
    ((${#matches[@]} == 1)) || die "snapcraft did not create exactly one Snap"
    local package="$dist/$(basename "${matches[0]}")"
    cp "${matches[0]}" "$package"
    record_artifact "$package"
}

build_nix() {
    log "Validating the Nix package"
    nix build --out-link "$work/nix-result" .#default
}

for format in "${formats[@]}"; do
    case "$format" in
        source) build_source ;;
        portable) build_portable ;;
        rpm) build_rpm "Fedora/RHEL" fedora packaging/rpm/pixelkit.spec ;;
        opensuse) build_rpm "openSUSE" opensuse packaging/opensuse/pixelkit.spec ;;
        deb) build_deb ;;
        deb-source) build_deb_source ;;
        arch) build_arch ;;
        flatpak) build_flatpak ;;
        appimage) build_appimage ;;
        snap) build_snap ;;
        nix) build_nix ;;
    esac
done

if ((${#artifacts[@]} > 0)); then
    log "Writing and verifying SHA-256 checksums"
    mapfile -t artifacts < <(printf '%s\n' "${artifacts[@]}" | LC_ALL=C sort -u)
    checksum_tmp="$work/SHA256SUMS"
    : > "$checksum_tmp"
    (
        cd "$dist"
        for artifact in "${artifacts[@]}"; do
            sha256sum "$(basename "$artifact")"
        done
    ) >> "$checksum_tmp"
    install -m644 "$checksum_tmp" "$dist/SHA256SUMS"
    (cd "$dist" && sha256sum -c SHA256SUMS)

    for candidate in "$dist"/*; do
        [[ -f "$candidate" ]] || continue
        [[ "$(basename "$candidate")" == "SHA256SUMS" ]] && continue
        current=false
        for artifact in "${artifacts[@]}"; do
            if [[ "$(realpath "$candidate")" == "$artifact" ]]; then
                current=true
                break
            fi
        done
        if ! $current; then
            warn "dist/$(basename "$candidate") was not produced by this run and is not listed in SHA256SUMS; use --clean to remove old output"
        fi
    done
fi

log "Package build complete"
printf 'Version: %s\n' "$version"
if ((${#artifacts[@]} > 0)); then
    printf 'Artifacts:\n'
    for artifact in "${artifacts[@]}"; do
        printf '  dist/%s\n' "$(basename "$artifact")"
    done
    printf '  dist/SHA256SUMS\n'
else
    printf 'No file artifact is produced for the selected validation target.\n'
fi
