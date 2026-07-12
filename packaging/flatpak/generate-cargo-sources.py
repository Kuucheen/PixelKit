#!/usr/bin/env python3
"""Generate Flatpak archive sources from Cargo.lock without network access."""

import json
import pathlib
import sys
import tomllib


def main() -> int:
    root = pathlib.Path(__file__).resolve().parents[2]
    lock_path = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else root / "Cargo.lock"
    output_path = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else root / "packaging/flatpak/cargo-sources.json"
    packages = tomllib.loads(lock_path.read_text(encoding="utf-8"))["package"]
    sources = []
    for package in sorted(packages, key=lambda item: (item["name"], item["version"])):
        source = package.get("source", "")
        if not source.startswith("registry+"):
            continue
        name = package["name"]
        version = package["version"]
        sources.append(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                "sha256": package["checksum"],
                "dest": f"cargo/vendor/{name}-{version}",
            }
        )
    config = """[source.crates-io]\nreplace-with = \"vendored-sources\"\n\n[source.vendored-sources]\ndirectory = \"vendor\"\n\n[net]\noffline = true\n"""
    sources.append(
        {
            "type": "inline",
            "contents": config,
            "dest": "cargo",
            "dest-filename": "config.toml",
        }
    )
    output_path.write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {len(sources) - 1} crates to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
