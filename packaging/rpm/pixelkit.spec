Name:           pixelkit
Version:        0.1.1
Release:        4%{?dist}
Summary:        Native Linux color picker and screen ruler
License:        MIT
URL:            https://github.com/Kuucheen/PixelKit
Source0:        %{url}/releases/download/v%{version}/%{name}-%{version}-vendor.tar.xz

BuildRequires:  cargo >= 1.88
BuildRequires:  rust >= 1.88
BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  libX11-devel
BuildRequires:  libxcb-devel
BuildRequires:  libxkbcommon-devel
BuildRequires:  wayland-devel
Requires:       libX11
Requires:       libxcb
Requires:       libxkbcommon
Requires:       libglvnd-glx
Requires:       libwayland-client
Requires:       xdg-desktop-portal
Recommends:     xdg-desktop-portal-gtk

%description
PixelKit provides a magnified system-wide color picker, color history and
format editor, and a screen ruler with bounds and color-edge measurement modes.
It uses direct capture on X11 and freedesktop portals on Wayland.

%prep
%autosetup

%build
export CARGO_NET_OFFLINE=true
cargo build --release --frozen --offline

%check
export CARGO_NET_OFFLINE=true
cargo test --release --all-targets --frozen --offline

%install
make install DESTDIR=%{buildroot} PREFIX=%{_prefix} CARGO="cargo --offline"

%files
%license LICENSE
%doc README.md NOTICE
%{_bindir}/pixelkit
%{_datadir}/applications/io.github.Kuucheen.PixelKit.desktop
%{_datadir}/metainfo/io.github.Kuucheen.PixelKit.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.Kuucheen.PixelKit.svg
%{_datadir}/icons/hicolor/128x128/apps/io.github.Kuucheen.PixelKit.png
%{_datadir}/icons/hicolor/512x512/apps/io.github.Kuucheen.PixelKit.png
%{_prefix}/lib/systemd/user/pixelkit.service
%{_mandir}/man1/pixelkit.1*

%changelog
* Mon Jul 13 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.1-4
- Republish Debian and Ubuntu packages after OBS signing-key setup

* Mon Jul 13 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.1-3
- Fix offline vendored source configuration for clean builds

* Mon Jul 13 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.1-2
- Bigger close button for the ruler

* Mon Jul 13 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.1-1
- Release PixelKit 0.1.1

* Mon Jul 13 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.0-7
- Update application logo

* Mon Jul 13 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.0-6
- Publish under the Kuucheen GitHub and application namespaces
- Fit the picker loupe to its content and reorder ruler tolerance controls

* Sun Jul 12 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.0-5
- Restore full-resolution captures with GPU-safe lossless texture tiling
- Add exact ruler tolerance entry and rounded selected mode controls
- Match the picker swatch to the magnifier and close the editor with Escape

* Sun Jul 12 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.0-4
- Make ruler recapture asynchronous and safe across portal runtimes
- Add font-independent ruler icons and GPU-safe capture previews
- Normalize mouse-wheel zoom and add a color swatch to the picker loupe

* Sun Jul 12 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.0-3
- Add an explicit Wayland shortcut configuration command
- Report registered-but-unassigned portal actions accurately

* Sun Jul 12 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.0-2
- Register the native application ID before using Wayland portals
- Require Fedora's libwayland-client runtime package

* Sun Jul 12 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.0-1
- Initial package
