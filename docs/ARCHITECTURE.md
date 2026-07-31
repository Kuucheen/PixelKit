# Architecture

PixelKit deliberately keeps capture policy separate from color, code-detection,
and measurement logic. The result is one native binary with no privileged
helper and only a small idle daemon when global shortcuts are enabled.

## Components

| Module | Responsibility |
|---|---|
| `capture` | X11 root-image conversion, portal screenshots, PNG loading, display DPI |
| `code_detection` | Full-resolution luminance conversion, multi-symbol decoding, result geometry |
| `color` | RGB/HSV/HSL/CMYK/CIE/Oklab conversion, color names, custom format tokens |
| `measurement` | Inclusive rectangles, physical units, two edge-distance algorithms |
| `config` | XDG paths, atomic JSON saves, history de-duplication and schema defaults |
| `daemon` | X11 hotkeys or a portal GlobalShortcuts session; launches tools on demand |
| `ui` | Settings plus independent picker, magnifier, code scanner, color editor, and ruler processes |

## Capture flow

```text
activation
    │
    ├── X11 ─────── root GetImage ───────┐
    │                                     │
    └── Wayland ── Screenshot portal ─────┤
                                          ▼
                                  normalized RGBA frame
                                      │    │    │    │
                         ┌────────────┘    │    │    └────────────┐
                         ▼                 ▼    ▼                 ▼
                    Color Picker      Magnifier Code Scanner  Screen Ruler
                         │                       │                 │
                         ▼                       ▼                 ▼
              clipboard + local history  text/link details    clipboard
```

The screen is captured before the overlay appears, so a normal pick never
samples PixelKit itself. In X11 continuous-ruler mode, the overlay framebuffer
is transparent and measurement shapes are skipped for one compositor frame
before recapture. Wayland's request-based Screenshot portal cannot be polled in
that way, so recapture is explicit, asynchronous, and permission-preserving.
Captures larger than the GPU's maximum texture side are divided into lossless
tiles for rendering; the source frame is never downscaled, so the backdrop,
pixel sampling, measurements, and code detection all retain the screenshot's
original resolution. Scanner decoding runs on a worker thread and returns every
result with source-image coordinates. A dedicated multi-QR pass is merged with
the general multi-format pass before clickable regions are painted.

## Edge detection

At the cursor, four scans proceed left, right, up, and down. Every candidate is
compared with the *starting* pixel, which prevents gradual gradients from
drifting arbitrarily far. In aggregate mode, `|ΔR| + |ΔG| + |ΔB|` must be no
greater than tolerance. In per-channel mode, each of the three differences must
be no greater than tolerance. Rectangle coordinates are inclusive, matching the
PowerToys reference and making a single pixel measure 1×1 rather than 0×0.

## Background behavior

The daemon holds only the shortcut registration and blocks on an event stream.
It does not create a render context, initialize the barcode decoder, capture
pixels, inspect pointer motion, or poll configuration. Tool activations are
separate short-lived processes, which keeps failures isolated and lets the
desktop reclaim all capture memory when an overlay closes.

## Security and privacy

- X11 access is limited to the user's existing display authority.
- Wayland capture and global shortcuts go through compositor-controlled portals.
- No root, capabilities, `/dev/input`, uinput, key logger, or injection API.
- Code payloads are decoded locally. Only an explicit **Open link** action sends
  a displayed HTTP(S) URI to the compositor-controlled OpenURI portal.
- No network client or telemetry dependency is present in the runtime graph.
- Settings/history use user XDG directories and atomic replacement.
- The Flatpak manifest exposes Documents only for user-requested color exports.
