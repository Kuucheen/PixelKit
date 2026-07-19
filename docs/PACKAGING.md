# Packaging and release guide

## Publication identity

The application ID is `io.github.Kuucheen.PixelKit` and the upstream repository
is `https://github.com/Kuucheen/PixelKit`. Cargo, Flatpak, AppStream, native
packages, Snap, Nix and systemd metadata all use that publication identity.
Changing the Flatpak application ID after publication would create a different
application, so it must remain stable for future releases.

## Release input

```bash
./scripts/build-packages.sh --list
./scripts/build-packages.sh
```

`dist/pixelkit-VERSION-vendor.tar.xz` contains `Cargo.lock`, all registry
sources, and `.cargo/config.toml`; RPM and Debian builds are therefore offline
and reproducible. The orchestrator builds `source` and `portable` plus every
format whose native builder is available. Name one or more formats to require
them explicitly, such as:

```bash
./scripts/build-packages.sh rpm deb
./scripts/build-packages.sh --clean deb-source
./scripts/build-packages.sh --skip-checks flatpak
make packages PACKAGE_ARGS="rpm deb"
```

It checks that Cargo, RPM, Debian, Arch, Flatpak, Snap, Nix, and AppStream all
declare the same upstream version. It also regenerates
`packaging/flatpak/cargo-sources.json` from `Cargo.lock`, creates deterministic
source and portable archives, and writes `dist/SHA256SUMS` for exactly the
artifacts produced by the run. Old files are left alone unless `--clean` is
given, but they are not silently included in the new checksum manifest.

For another distro package revision of the current upstream version, use:

```bash
./scripts/build-packages.sh --bump "Describe the packaged change"
```

For a new stable upstream version, use:

```bash
./scripts/build-packages.sh --clean --set-version 0.1.1
```

`--set-version` accepts stable `X.Y.Z` versions newer than the current one. It
updates Cargo and its lockfile, both RPM specs, Debian, Arch, Flatpak, Snap,
Nix, AppStream release history, and the manual page. Fedora, Debian, and Arch
start at package revision 1 for the new upstream release; openSUSE resets to
release 0. Generated artifact names then follow the embedded version, so never
rename package files manually.

The version and package bumps are explicit because they edit tracked metadata.
They never commit, sign, tag, push, publish releases, or upload to stores. After
reviewing the changes, the maintainer still creates and pushes the release
commit and `vX.Y.Z` tag. Attach the source, portable, native package artifacts,
and checksum manifest to the GitHub release.

## Validation checklist

- `cargo test --all-targets --locked` passes on x86_64 and aarch64.
- Release binary starts on a glibc baseline no newer than the oldest supported
  distro (build in the CI container, not on a rolling host).
- `appstreamcli validate --pedantic` passes.
- `desktop-file-validate` passes.
- `flatpak-builder --force-clean` succeeds with networking disabled for build.
- `rpmlint`, `lintian`, `namcap`, and `snapcraft lint` pass for their artifacts.
- X11 and Wayland picker/magnifier/ruler smoke tests pass, including portal denial.
- Test two monitors, mixed scale factors, RGB565/24/32-bit X visuals where
  available, clipboard persistence, shortcut conflicts, and corrupt JSON.
- Confirm the release contact and GitHub noreply address are appropriate for the publisher.
- Sign the tag and publish SHA-256 checksums for every artifact.

## Repository submissions

- **Flathub:** submit `io.github.Kuucheen.PixelKit.yml` plus
  `cargo-sources.json` to a new Flathub repository. Include real screenshots
  and proof of namespace ownership.
- **Fedora/Copr:** build the vendor source archive and `packaging/rpm` spec.
  Fedora inclusion may prefer distro-packaged Rust crates; Copr accepts the
  vendored release source. PixelKit's current Copr namespace is
  `kuchen/pixelkit`; submit the generated SRPM with
  `copr-cli build pixelkit dist/pixelkit-VERSION-RELEASE.fcXX.src.rpm`. After a
  successful build, users can install it with
  `sudo dnf copr enable kuchen/pixelkit` followed by
  `sudo dnf install pixelkit`.
- **AUR:** copy `PKGBUILD`, run `updpkgsums` if switching from the tagged VCS
  source to a release tarball, then generate `.SRCINFO` with `makepkg --printsrcinfo`.
- **Debian/Ubuntu (OBS):** `./scripts/build-packages.sh --clean deb-source`
  creates the standard `.dsc`, `.orig.tar.xz`, and `.debian.tar.xz` source
  bundle. `./scripts/publish-obs.sh --watch` replaces the source bundle in the
  `pixelkit` package under `home:kuchen:PixelKit`, commits it atomically, and
  follows the remote builds. Debian 13 must build against
  `Debian:13/backports` to satisfy the Rust 1.88 MSRV; Ubuntu 24.04 uses its
  versioned Rust 1.91 tools. The configured x86_64 repositories are
  `Debian_13`, `xUbuntu_24.04`, and `xUbuntu_26.04`.

  To install from the published OBS APT repository, set `repo` to the matching
  value and add its signing key and source:

  ```bash
  repo=xUbuntu_24.04 # Debian_13, xUbuntu_24.04, or xUbuntu_26.04
  base="https://download.opensuse.org/repositories/home:/kuchen:/PixelKit/$repo"
  sudo apt install curl gpg
  sudo install -d -m 0755 /etc/apt/keyrings
  curl -fsSL "$base/Release.key" | gpg --dearmor | \
    sudo tee /etc/apt/keyrings/pixelkit-obs.gpg >/dev/null
  echo "deb [signed-by=/etc/apt/keyrings/pixelkit-obs.gpg] $base/ /" | \
    sudo tee /etc/apt/sources.list.d/pixelkit-obs.list
  sudo apt update
  sudo apt install pixelkit
  ```
- **Snap Store:** reserve the `pixelkit` name, then `snapcraft upload --release=stable`.
- **Nixpkgs:** the flake works directly; a nixpkgs PR can translate it to the
  standard package set after the upstream repository exists.

Package definitions cannot reserve store names or sign/upload artifacts on
their own. Those final publication operations require the user's store and
repository credentials.
