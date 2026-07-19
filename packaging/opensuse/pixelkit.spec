Name:           pixelkit
Version:        0.2.0
Release:        0
Summary:        Native Linux color picker, magnifier, and screen ruler
License:        MIT
Group:          Productivity/Graphics/Other
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
Requires:       libX11-6
Requires:       libxcb1
Requires:       libxkbcommon0
Requires:       libwayland-client0
Requires:       Mesa-libGL1
Requires:       xdg-desktop-portal
Recommends:     xdg-desktop-portal-gtk

%description
PixelKit provides a system-wide color picker, configurable magnifier, color editor,
and screen ruler.

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
%{_mandir}/man1/pixelkit.1%{?ext_man}

%changelog
* Sun Jul 19 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.2.0-0
- Release PixelKit 0.2.0

* Wed Jul 15 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.2-0
- Release PixelKit 0.1.2

* Mon Jul 13 2026 PixelKit contributors <70746714+Kuucheen@users.noreply.github.com> - 0.1.1-0
- Release PixelKit 0.1.1
