#!/usr/bin/env python3
"""Increment distro package revisions without changing the upstream version."""

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


def replace_once(text: str, pattern: str, replacement: str, label: str) -> str:
    updated, count = re.subn(
        pattern, lambda _match: replacement, text, count=1, flags=re.MULTILINE
    )
    if count != 1:
        raise SystemExit(f"Could not update {label}")
    return updated


def write_atomic(path: pathlib.Path, contents: str) -> None:
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, delete=False
    ) as handle:
        handle.write(contents)
        temporary = pathlib.Path(handle.name)
    temporary.chmod(path.stat().st_mode)
    os.replace(temporary, path)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Increment PixelKit's RPM, Debian, and Arch package revisions."
    )
    parser.add_argument("message", help="Short changelog entry for this package build")
    parser.add_argument(
        "--dry-run", action="store_true", help="Validate and print the next revisions only"
    )
    args = parser.parse_args()

    message = " ".join(args.message.split()).lstrip("-* ").rstrip(". ")
    if not message:
        parser.error("message must contain visible text")

    cargo_text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    cargo_match = re.search(r'^version = "([^"]+)"$', cargo_text, re.MULTILINE)
    if not cargo_match:
        raise SystemExit("Could not read the upstream version from Cargo.toml")
    version = cargo_match.group(1)

    rpm_path = ROOT / "packaging/rpm/pixelkit.spec"
    rpm_text = rpm_path.read_text(encoding="utf-8")
    rpm_version_match = re.search(r"^Version:\s+(\S+)$", rpm_text, re.MULTILINE)
    rpm_release_match = re.search(
        r"^Release:\s+(\d+)(%\{\?dist\})$", rpm_text, re.MULTILINE
    )
    if not rpm_version_match or not rpm_release_match:
        raise SystemExit("Could not read Version/Release from the Fedora RPM spec")
    if rpm_version_match.group(1) != version:
        raise SystemExit("Fedora RPM Version does not match Cargo.toml")
    next_rpm = int(rpm_release_match.group(1)) + 1

    debian_path = ROOT / "debian/changelog"
    debian_text = debian_path.read_text(encoding="utf-8")
    debian_match = re.match(r"pixelkit \(([^)]+)-(\d+)\) ", debian_text)
    if not debian_match:
        raise SystemExit("Could not read a numeric revision from debian/changelog")
    if debian_match.group(1) != version:
        raise SystemExit("Debian upstream version does not match Cargo.toml")
    next_debian = int(debian_match.group(2)) + 1

    arch_path = ROOT / "packaging/arch/PKGBUILD"
    arch_text = arch_path.read_text(encoding="utf-8")
    arch_version_match = re.search(r"^pkgver=(\S+)$", arch_text, re.MULTILINE)
    arch_release_match = re.search(r"^pkgrel=(\d+)$", arch_text, re.MULTILINE)
    if not arch_version_match or not arch_release_match:
        raise SystemExit("Could not read pkgver/pkgrel from the Arch PKGBUILD")
    if arch_version_match.group(1) != version:
        raise SystemExit("Arch pkgver does not match Cargo.toml")
    next_arch = int(arch_release_match.group(1)) + 1

    now = datetime.now().astimezone()
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
    rpm_date = f"{weekdays[now.weekday()]} {months[now.month - 1]} {now.day:02d} {now.year}"
    debian_date = email.utils.format_datetime(now)

    rpm_text = replace_once(
        rpm_text,
        r"^Release:\s+\d+%\{\?dist\}$",
        f"Release:        {next_rpm}%{{?dist}}",
        "the Fedora RPM release",
    )
    rpm_entry = (
        f"%changelog\n* {rpm_date} {MAINTAINER} - {version}-{next_rpm}\n"
        f"- {message}\n\n"
    )
    rpm_text = replace_once(rpm_text, r"^%changelog\n", rpm_entry, "the RPM changelog")

    debian_message = message if message.endswith(("!", "?")) else f"{message}."
    debian_entry = (
        f"pixelkit ({version}-{next_debian}) unstable; urgency=medium\n\n"
        f"  * {debian_message}\n\n"
        f" -- {MAINTAINER}  {debian_date}\n\n"
    )
    debian_text = debian_entry + debian_text
    arch_text = replace_once(
        arch_text, r"^pkgrel=\d+$", f"pkgrel={next_arch}", "the Arch package release"
    )

    print(
        f"Package revisions: RPM {next_rpm}, Debian {next_debian}, Arch {next_arch}"
    )
    if args.dry_run:
        return 0

    write_atomic(rpm_path, rpm_text)
    write_atomic(debian_path, debian_text)
    write_atomic(arch_path, arch_text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
