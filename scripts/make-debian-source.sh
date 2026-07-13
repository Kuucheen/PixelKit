#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

dist="$root/dist"
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
debian_version="$(sed -n '1s/^pixelkit (\([^)]*\)).*/\1/p' debian/changelog)"
debian_upstream="${debian_version%-*}"
source_name="pixelkit-$version"
vendor_archive="$dist/$source_name-vendor.tar.xz"
orig_archive="$dist/pixelkit_${version}.orig.tar.xz"
debian_archive="$dist/pixelkit_${debian_version}.debian.tar.xz"
dsc="$dist/pixelkit_${debian_version}.dsc"

[[ -n "$version" ]] || {
    printf 'error: could not read Cargo.toml version\n' >&2
    exit 1
}
[[ "$debian_upstream" == "$version" ]] || {
    printf 'error: Debian version %s does not match Cargo.toml %s\n' \
        "$debian_upstream" "$version" >&2
    exit 1
}
[[ -f "$vendor_archive" ]] || {
    printf 'error: missing %s; run ./scripts/make-dist.sh first\n' \
        "$vendor_archive" >&2
    exit 1
}

for command in tar sha1sum sha256sum md5sum stat awk; do
    command -v "$command" >/dev/null || {
        printf 'error: %s is required\n' "$command" >&2
        exit 1
    }
done

work="$(mktemp -d "${TMPDIR:-/tmp}/pixelkit-debian-source.XXXXXX")"
trap 'rm -rf "$work"' EXIT
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}"

tar -xJf "$vendor_archive" -C "$work"
[[ -d "$work/$source_name" ]] || {
    printf 'error: %s does not contain the expected %s directory\n' \
        "$vendor_archive" "$source_name" >&2
    exit 1
}

# Debian's 3.0 (quilt) source format keeps upstream files and the debian/
# directory in separate archives. Cargo's vendor/ and .cargo/ remain in the
# orig archive so every OBS build can stay offline.
rm -rf "$work/$source_name/debian"
tar --sort=name --mtime="@$source_date_epoch" --owner=0 --group=0 --numeric-owner \
    -C "$work" -cJf "$orig_archive" "$source_name"
tar --sort=name --mtime="@$source_date_epoch" --owner=0 --group=0 --numeric-owner \
    -C "$root" -cJf "$debian_archive" debian

control_field() {
    local field="$1"
    sed -n "s/^${field}: //p" debian/control | head -1
}

build_depends="$({
    awk '
        /^Build-Depends:/ {
            collecting = 1
            sub(/^Build-Depends:[[:space:]]*/, "")
            printf "%s", $0
            next
        }
        collecting && /^[[:space:]]/ {
            sub(/^[[:space:]]+/, "")
            printf " %s", $0
            next
        }
        collecting { exit }
    ' debian/control
    printf '\n'
} | sed 's/[[:space:]]\+/ /g; s/^ //; s/ $//')"

checksum_line() {
    local algorithm="$1"
    local file="$2"
    local hash
    hash="$($algorithm "$file" | awk '{print $1}')"
    printf ' %s %s %s\n' "$hash" "$(stat -c %s "$file")" "$(basename "$file")"
}

temporary_dsc="$work/$(basename "$dsc")"
{
    printf 'Format: 3.0 (quilt)\n'
    printf 'Source: pixelkit\n'
    printf 'Binary: pixelkit\n'
    printf 'Architecture: any\n'
    printf 'Version: %s\n' "$debian_version"
    printf 'Maintainer: %s\n' "$(control_field Maintainer)"
    printf 'Homepage: %s\n' "$(control_field Homepage)"
    printf 'Standards-Version: %s\n' "$(control_field Standards-Version)"
    printf 'Vcs-Browser: %s\n' "$(control_field Vcs-Browser)"
    printf 'Vcs-Git: %s\n' "$(control_field Vcs-Git)"
    printf 'Build-Depends: %s\n' "$build_depends"
    printf 'Package-List:\n'
    printf ' pixelkit deb graphics optional arch=any\n'
    printf 'Checksums-Sha1:\n'
    checksum_line sha1sum "$orig_archive"
    checksum_line sha1sum "$debian_archive"
    printf 'Checksums-Sha256:\n'
    checksum_line sha256sum "$orig_archive"
    checksum_line sha256sum "$debian_archive"
    printf 'Files:\n'
    checksum_line md5sum "$orig_archive"
    checksum_line md5sum "$debian_archive"
} > "$temporary_dsc"
install -m644 "$temporary_dsc" "$dsc"

printf 'Created Debian source package:\n'
printf '  %s\n' "$orig_archive" "$debian_archive" "$dsc"
