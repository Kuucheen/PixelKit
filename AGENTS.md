# PixelKit agent guide

This file applies to the entire PixelKit repository. Read it before changing
code, packaging, desktop metadata, CI, or release configuration. The adjacent
`../PowerToys` tree is an MIT-licensed behavioral reference; its `AGENTS.md`
applies only inside that tree.

## Project intent and invariants

PixelKit is a lightweight, native Linux implementation of the PowerToys Color
Picker and Screen Ruler workflows. Preserve these properties:

- One Rust binary with short-lived picker/ruler/editor processes and a small
  event-blocked shortcut daemon.
- No privileged helper, `/dev/input`, uinput, key logger, telemetry, or runtime
  network service.
- Direct X11 integration where the protocol permits it; compositor-controlled
  `xdg-desktop-portal` APIs on Wayland and in Flatpak.
- Full-resolution pixel sampling and rendering. Never trade correctness for a
  downscaled preview.
- Local, version-tolerant JSON settings/history in XDG directories.
- The stable publication/application ID is
  `io.github.Kuucheen.PixelKit`. Do not rename it after publication.

The code uses Rust edition 2024, MSRV 1.88, and eframe/egui 0.31 with the Glow
renderer. The upstream repository is `git@github.com:Kuucheen/PixelKit.git`.

## Source map

- `src/capture.rs`: PNG loading, direct X11 root capture, Wayland Screenshot
  portal capture, RGBA normalization, and DPI.
- `src/color.rs`: color conversions, names, PowerToys-compatible format tokens.
- `src/measurement.rs`: inclusive measurement rectangles, physical units, and
  same-color edge scans.
- `src/config.rs`: XDG paths, defaults, atomic saves, and color history.
- `src/daemon.rs`: X11 hotkeys and Wayland GlobalShortcuts portal sessions.
- `src/ui/mod.rs`: shared egui style, process launching, wheel normalization,
  and lossless tiled capture textures.
- `src/ui/picker.rs`: full-screen picker and magnifier/loupe.
- `src/ui/ruler.rs`: full-screen ruler, measurements, and async recapture.
- `src/ui/editor.rs`: saved-color editor/history/export UI.
- `src/ui/hub.rs`: settings and launcher window.
- `docs/ARCHITECTURE.md`: high-level design.
- `docs/PACKAGING.md`: release and repository submission guidance.

## Non-obvious implementation findings

### Capture and GPU rendering

- `CaptureFrame` always retains the original RGBA pixels. Sampling and
  measurements operate on this full-resolution buffer.
- Some systems report an egui/OpenGL maximum texture side of 2048 even when the
  desktop capture is larger (the Fedora/KDE reference system produced
  2240x1400 captures). Uploading one oversized texture panics.
- `TiledCaptureTexture` in `src/ui/mod.rs` splits captures into lossless GPU-safe
  regions and maps each tile back into the full image rectangle. Do not replace
  this with proportional downscaling: that visibly loses screen pixels and was
  a previously reported regression.
- Keep nearest-neighbor texture sampling. Pixel/source coordinate mapping must
  continue using the original capture dimensions.
- Direct X11 capture must honor the advertised visual channel masks and scanline
  padding; do not assume one fixed BGRX byte layout.

### Wayland portals and recapture

- Wayland deliberately forbids silent global capture and key grabs. Do not add
  compositor-specific bypasses. Screenshot permission/selection dialogs are
  expected behavior.
- Each screenshot request creates its own `ashpd::zbus::Connection::session()`
  and best-effort registers `APP_ID`. Do not use ashpd's process-global cached
  connection from a short-lived Tokio runtime: a later recapture can inherit a
  connection owned by a dead runtime and stop making progress.
- Never call `capture_screen()` synchronously from an egui `update()` method.
  The old synchronous ruler refresh blocked the Wayland event loop and KDE
  terminated the app as unresponsive.
- Ruler **Recapture** uses a worker thread, waits briefly for one transparent
  overlay frame, polls through a channel, and has a 90-second recovery timeout.
  Preserve the transparent frame so PixelKit does not photograph itself.
- Wayland ruler content is a snapshot. Manual Recapture / `R` is intentional;
  continuous capture is limited to X11.

### Input and UI behavior

- Use the shared raw-event `wheel_steps()` helper for discrete behavior. Egui's
  smoothed scroll delta can turn one notched mouse-wheel event into many zoom or
  tolerance changes. Raw Line/Page or large Point events are one step; small
  touchpad Point events accumulate gradually.
- The picker loupe has no fixed minimum content width. Its box width is derived
  from the 13x13 grid, formatted value, and optional color name. The color
  swatch width is exactly the magnified grid width.
- Picker Escape/Backspace closes without picking. Escape also closes the color
  editor opened after a pick.
- Ruler mode icons are painter-drawn vectors. Do not replace them with Unicode
  glyphs; font coverage was unreliable.
- Egui's selected `Button` state defaults to a zero corner radius. Ruler mode
  buttons explicitly set their corner radius so selected controls stay rounded.
- The ruler tolerance toolbar order is `Tolerance`, numeric input, minus, plus.
  The numeric input must remain visible and directly typeable from 0 to 255.
- Measurement rectangles are inclusive. Edge scans compare every candidate to
  the starting pixel, not the previous pixel, so gradients cannot drift.

### Shortcut daemon

- The daemon must remain capture-free and GUI-free while idle. It only owns the
  shortcut registration/session and launches separate tool processes.
- On Wayland, the portal/compositor owns the final key assignment. If actions
  are registered but unassigned, use `pixelkit configure-shortcuts`.
- The application ID changed once, before publication, from the placeholder
  namespace to `io.github.Kuucheen.PixelKit`. Users upgrading an older local RPM
  may need to restart `pixelkit.service` and configure portal shortcuts again.

## Required checks

Run checks proportionally to the change. Before a release or broad handoff, run
all of these from the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo check --all-targets --locked
cargo build --release --locked
appstreamcli validate --no-net packaging/linux/io.github.Kuucheen.PixelKit.metainfo.xml
desktop-file-validate \
  packaging/linux/io.github.Kuucheen.PixelKit.desktop \
  packaging/linux/pixelkit-autostart.desktop
```

The current unit suite has 18 tests. Add focused tests for pure coordinate,
tiling, wheel, color, or measurement logic when changing those areas.

On Fedora, `cargo-fmt` and `cargo-clippy` may be separately packaged. In this
workspace, temporary extracted tools have existed under
`/tmp/pixelkit-rustfmt/usr/bin` and `/tmp/pixelkit-clippy/usr/bin`; prefer the
normal toolchain and treat those paths only as a local fallback.

For deterministic UI smoke tests, use the `--image` mode so no portal prompt is
needed:

```bash
cargo run -- color-picker --image /path/to/test.png
cargo run -- screen-ruler --image /path/to/test.png
```

When testing under Xvfb from a Wayland desktop, force winit onto X11; otherwise
the process may open on the real Wayland session despite `DISPLAY` being set:

```bash
env -u WAYLAND_DISPLAY DISPLAY=:96 XDG_SESSION_TYPE=x11 \
  WINIT_UNIX_BACKEND=x11 target/debug/pixelkit color-picker --image /path/to/test.png
```

Exercise at least one source wider than 2048 pixels when touching capture or
rendering, and visually check for tile seams, pixel softness, and source/screen
coordinate mismatch.

## Packaging conventions

- `./scripts/build-packages.sh` is the canonical local package entry point. It
  refreshes Flatpak Cargo hashes, validates version consistency, builds all
  locally available formats, and verifies `dist/SHA256SUMS`. Use explicit
  format arguments when one artifact is required.
- `make packages` is a shorthand; pass script arguments with `PACKAGE_ARGS`.
- `make dist` remains the low-level command that creates the deterministic
  `dist/pixelkit-VERSION-vendor.tar.xz` with all locked Cargo sources.
  RPM/SRPM builds use this archive offline.
- Relevant definitions are:
  - Fedora/RHEL: `packaging/rpm/pixelkit.spec`
  - openSUSE: `packaging/opensuse/pixelkit.spec`
  - Debian/Ubuntu: `debian/`
  - Arch/AUR: `packaging/arch/PKGBUILD`
  - Flatpak: `packaging/flatpak/io.github.Kuucheen.PixelKit.yml`
  - Snap: `snap/snapcraft.yaml`
  - Nix: `flake.nix`
  - AppImage: `packaging/appimage/build-appimage.sh`
- For iterative changes within one upstream version, run
  `./scripts/build-packages.sh --bump "Short changelog summary"`. The helper
  increments the Fedora `Release`, prepends a Debian changelog revision, and
  increments Arch `pkgrel` together. Do not edit only one of them manually.
- For a new stable upstream version, run
  `./scripts/build-packages.sh --clean --set-version X.Y.Z`. This updates every
  embedded version, preserves release history, and resets Fedora, Debian, and
  Arch to package revision 1. It deliberately does not commit, tag, push, or
  publish. Never rename artifact files to simulate another version.
- Fedora runtime dependency names matter. Require `libwayland-client`, not the
  nonexistent generic `wayland` capability that caused the first RPM install
  failure.
- Prefer a conventional extracted-source `rpmbuild` for a final release.
  `rpmbuild --build-in-place` can leave an empty `debugsourcefiles.list`; if it
  is used for quick local iteration, disable the optional debug package or do
  not treat it as the canonical release build.
- The package script regenerates and verifies `dist/SHA256SUMS` for artifacts
  from that run. `dist/`, RPMs, and DEBs are intentionally ignored by Git.
- Installed desktop, icon, and AppStream filenames must match
  `io.github.Kuucheen.PixelKit`.
- The AppStream component ID uses the published capitalization, while its
  developer ID is lowercase `io.github.kuucheen` to satisfy validation.
- Do not add unregistered names such as `COSMIC` to `OnlyShowIn`; the autostart
  entry is intentionally desktop-neutral.

## GitHub and release state

- `main` tracks `origin/main` at `git@github.com:Kuucheen/PixelKit.git`.
- Tag `v0.1.0` and its GitHub release already exist. Do not move or overwrite a
  published tag casually.
- CI validates formatting, Clippy, tests, release build, AppStream, desktop
  files, generated Flatpak sources, and the staged install tree.
- The release workflow builds a portable archive, DEB, RPM, SRPM, debug RPMs,
  vendored source, and checksums.
- As of 2026-07-13, the final `main` CI and the `v0.1.0` release workflow were
  green.
- The GitHub repository is currently private. Public Flathub, Fedora, AUR, and
  similar submissions require a public upstream, but changing visibility
  exposes the project. Do not make it public without explicit user approval.

## Common traps to avoid

- Do not block an egui frame on portal I/O.
- Do not downscale the captured backdrop to fit one GPU texture.
- Do not reuse a portal connection tied to a destroyed runtime.
- Do not use smoothed scroll deltas for discrete zoom/tolerance steps.
- Do not use font glyphs for required ruler icons.
- Do not change the published application ID or package URLs independently.
- Do not commit `target/`, `dist/`, vendored crates, RPM-generated `debug*.list`
  files, or `elfbins.list`.
- Preserve unrelated user changes in a dirty worktree and keep generated
  package output out of source commits.
