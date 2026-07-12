# Flatpak / Flathub

Generate the locked Cargo source list whenever `Cargo.lock` changes:

```bash
python3 packaging/flatpak/generate-cargo-sources.py
```

Build and run locally:

```bash
flatpak install flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08
flatpak-builder --force-clean --user --install build-dir \
  packaging/flatpak/io.github.Kuucheen.PixelKit.local.yml
flatpak run io.github.Kuucheen.PixelKit
```

The non-`local` manifest uses the tagged upstream Git repository and is the
manifest intended for a Flathub submission. Before publishing under a different
GitHub owner, change the reverse-DNS app ID consistently in the manifest,
desktop file, metainfo, icon filename, `APP_ID`, and package metadata. Flathub
requires that the submitter controls the namespace.

No broad D-Bus or home-directory access is requested. The Documents permission
is used only for color exports; screenshots and shortcut bindings go through
freedesktop portals.
