#!/usr/bin/env python3
"""Set PixelKit's upstream version consistently across package metadata."""

from __future__ import annotations

import argparse
import email.utils
import os
import pathlib
import re
import tempfile
from datetime import datetime


ROOT = pathlib.Path(__file__).resolve().parents[1]
MAINTAINER = "PixelKit contributors <70746714+Kuucheen@users.noreply.github.com>"
VERSION_PATTERN = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$"
)


def replace_once(text: str, pattern: str, replacement: str, label: str) -> str:
    updated, count = re.subn(
        pattern, lambda _match: replacement, text, count=1, flags=re.MULTILINE
    )
    if count != 1:
        raise SystemExit(f"Could not update {label}")
    return updated


def capture(text: str, pattern: str, label: str) -> str:
    match = re.search(pattern, text, re.MULTILINE)
    if not match:
        raise SystemExit(f"Could not read {label}")
    return match.group(1)


def write_atomic(path: pathlib.Path, contents: str) -> None:
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, delete=False
    ) as handle:
        handle.write(contents)
        temporary = pathlib.Path(handle.name)
    temporary.chmod(path.stat().st_mode)
    os.replace(temporary, path)


def rpm_date(now: datetime) -> str:
    weekdays = ("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")
    months = (
        "Jan",
        "Feb",
        "Mar",
        "Apr",
        "May",
        "Jun",
        "Jul",
        "Aug",
        "Sep",
        "Oct",
        "Nov",
        "Dec",
    )
    return f"{weekdays[now.weekday()]} {months[now.month - 1]} {now.day:02d} {now.year}"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Set PixelKit's upstream version and reset distro revisions."
    )
    parser.add_argument("version", help="New stable version in X.Y.Z form")
    parser.add_argument(
        "--dry-run", action="store_true", help="Validate and show changes without writing"
    )
    args = parser.parse_args()

    version_match = VERSION_PATTERN.fullmatch(args.version)
    if not version_match:
        parser.error("version must be a stable semantic version in X.Y.Z form")
    next_version = args.version
    next_parts = tuple(int(part) for part in version_match.groups())

    paths = {
        "cargo": ROOT / "Cargo.toml",
        "lock": ROOT / "Cargo.lock",
        "rpm": ROOT / "packaging/rpm/pixelkit.spec",
        "opensuse": ROOT / "packaging/opensuse/pixelkit.spec",
        "arch": ROOT / "packaging/arch/PKGBUILD",
        "debian": ROOT / "debian/changelog",
        "flatpak": ROOT / "packaging/flatpak/io.github.Kuucheen.PixelKit.yml",
        "snap": ROOT / "snap/snapcraft.yaml",
        "nix": ROOT / "flake.nix",
        "appstream": ROOT / "packaging/linux/io.github.Kuucheen.PixelKit.metainfo.xml",
        "manpage": ROOT / "docs/pixelkit.1",
    }
    texts = {name: path.read_text(encoding="utf-8") for name, path in paths.items()}

    current_version = capture(
        texts["cargo"], r'^version = "([^"]+)"$', "Cargo.toml package version"
    )
    current_match = VERSION_PATTERN.fullmatch(current_version)
    if not current_match:
        raise SystemExit(f"Current Cargo version is not stable X.Y.Z: {current_version}")
    current_parts = tuple(int(part) for part in current_match.groups())
    if next_parts <= current_parts:
        raise SystemExit(
            f"New version {next_version} must be greater than current version {current_version}"
        )

    debian_version = capture(
        texts["debian"], r"^pixelkit \(([^)]+)\) ", "Debian changelog version"
    )
    debian_upstream = debian_version.rsplit("-", maxsplit=1)[0]
    declared_versions = {
        "Cargo.lock": capture(
            texts["lock"],
            r'^\[\[package\]\]\nname = "pixelkit"\nversion = "([^"]+)"$',
            "Cargo.lock PixelKit version",
        ),
        "Fedora RPM": capture(texts["rpm"], r"^Version:\s+(\S+)$", "RPM version"),
        "openSUSE RPM": capture(
            texts["opensuse"], r"^Version:\s+(\S+)$", "openSUSE version"
        ),
        "Arch": capture(texts["arch"], r"^pkgver=(\S+)$", "Arch pkgver"),
        "Debian": debian_upstream,
        "Flatpak": capture(
            texts["flatpak"], r"^\s*tag: v(\S+)$", "Flatpak release tag"
        ),
        "Snap": capture(texts["snap"], r"^version: '([^']+)'$", "Snap version"),
        "Nix": capture(
            texts["nix"], r'^\s*version = "([^"]+)";$', "Nix version"
        ),
        "AppStream": capture(
            texts["appstream"],
            r'<release version="([^"]+)"',
            "latest AppStream release",
        ),
        "man page": capture(
            texts["manpage"], r'"PixelKit ([^"]+)"', "manual page version"
        ),
    }
    mismatches = [
        f"{label}={value}"
        for label, value in declared_versions.items()
        if value != current_version
    ]
    if mismatches:
        raise SystemExit(
            "Version metadata is inconsistent with "
            f"Cargo.toml ({current_version}): {', '.join(mismatches)}"
        )

    now = datetime.now().astimezone()
    rpm_changelog_date = rpm_date(now)
    debian_date = email.utils.format_datetime(now)
    appstream_date = now.date().isoformat()

    updated = dict(texts)
    updated["cargo"] = replace_once(
        updated["cargo"],
        r'^version = "[^"]+"$',
        f'version = "{next_version}"',
        "Cargo.toml version",
    )
    updated["lock"] = replace_once(
        updated["lock"],
        r'^(\[\[package\]\]\nname = "pixelkit"\n)version = "[^"]+"$',
        f'[[package]]\nname = "pixelkit"\nversion = "{next_version}"',
        "Cargo.lock PixelKit version",
    )

    updated["rpm"] = replace_once(
        updated["rpm"], r"^Version:\s+\S+$", f"Version:        {next_version}", "RPM version"
    )
    updated["rpm"] = replace_once(
        updated["rpm"],
        r"^Release:\s+\d+%\{\?dist\}$",
        "Release:        1%{?dist}",
        "RPM release",
    )
    rpm_entry = (
        f"%changelog\n* {rpm_changelog_date} {MAINTAINER} - {next_version}-1\n"
        f"- Release PixelKit {next_version}\n\n"
    )
    updated["rpm"] = replace_once(
        updated["rpm"], r"^%changelog\n", rpm_entry, "RPM changelog"
    )

    updated["opensuse"] = replace_once(
        updated["opensuse"],
        r"^Version:\s+\S+$",
        f"Version:        {next_version}",
        "openSUSE version",
    )
    updated["opensuse"] = replace_once(
        updated["opensuse"],
        r"^Release:\s+\S+$",
        "Release:        0",
        "openSUSE release",
    )
    opensuse_entry = (
        f"%changelog\n* {rpm_changelog_date} {MAINTAINER} - {next_version}-0\n"
        f"- Release PixelKit {next_version}\n\n"
    )
    updated["opensuse"] = replace_once(
        updated["opensuse"],
        r"^%changelog\n",
        opensuse_entry,
        "openSUSE changelog",
    )

    updated["arch"] = replace_once(
        updated["arch"], r"^pkgver=\S+$", f"pkgver={next_version}", "Arch pkgver"
    )
    updated["arch"] = replace_once(
        updated["arch"], r"^pkgrel=\d+$", "pkgrel=1", "Arch pkgrel"
    )

    debian_entry = (
        f"pixelkit ({next_version}-1) unstable; urgency=medium\n\n"
        f"  * New upstream release {next_version}.\n\n"
        f" -- {MAINTAINER}  {debian_date}\n\n"
    )
    updated["debian"] = debian_entry + updated["debian"]
    updated["flatpak"] = replace_once(
        updated["flatpak"],
        r"^(\s*)tag: v\S+$",
        f"        tag: v{next_version}",
        "Flatpak release tag",
    )
    updated["snap"] = replace_once(
        updated["snap"],
        r"^version: '[^']+'$",
        f"version: '{next_version}'",
        "Snap version",
    )
    updated["nix"] = replace_once(
        updated["nix"],
        r'^\s*version = "[^"]+";$',
        f'            version = "{next_version}";',
        "Nix version",
    )

    appstream_entry = (
        f'  <releases>\n    <release version="{next_version}" date="{appstream_date}">\n'
        "      <description>\n"
        f"        <p>PixelKit {next_version} upstream release.</p>\n"
        "      </description>\n"
        "    </release>\n"
    )
    updated["appstream"] = replace_once(
        updated["appstream"],
        r"^  <releases>\n",
        appstream_entry,
        "AppStream release history",
    )
    updated["manpage"] = replace_once(
        updated["manpage"],
        r'"PixelKit [^"]+"',
        f'"PixelKit {next_version}"',
        "manual page version",
    )

    print(f"Upstream version: {current_version} -> {next_version}")
    print("Package revisions: RPM 1, Debian 1, Arch 1, openSUSE 0")
    if args.dry_run:
        return 0

    for name, path in paths.items():
        write_atomic(path, updated[name])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
