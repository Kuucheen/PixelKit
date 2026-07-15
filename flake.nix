{
  description = "PixelKit Linux color picker and screen ruler";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      packages = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "pixelkit";
            version = "0.1.2";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
            buildInputs = [ pkgs.libGL pkgs.libxkbcommon pkgs.wayland pkgs.libX11 pkgs.libxcb ];

            postInstall = ''
              install -Dm644 packaging/linux/io.github.Kuucheen.PixelKit.desktop $out/share/applications/io.github.Kuucheen.PixelKit.desktop
              install -Dm644 packaging/linux/io.github.Kuucheen.PixelKit.metainfo.xml $out/share/metainfo/io.github.Kuucheen.PixelKit.metainfo.xml
              install -Dm644 packaging/linux/io.github.Kuucheen.PixelKit.svg $out/share/icons/hicolor/scalable/apps/io.github.Kuucheen.PixelKit.svg
              install -Dm644 packaging/linux/io.github.Kuucheen.PixelKit.png $out/share/icons/hicolor/128x128/apps/io.github.Kuucheen.PixelKit.png
              install -Dm644 packaging/linux/512x512/io.github.Kuucheen.PixelKit.png $out/share/icons/hicolor/512x512/apps/io.github.Kuucheen.PixelKit.png
              install -Dm644 packaging/linux/pixelkit.service $out/lib/systemd/user/pixelkit.service
              install -Dm644 docs/pixelkit.1 $out/share/man/man1/pixelkit.1
            '';

            postFixup = ''
              wrapProgram $out/bin/pixelkit \
                --prefix LD_LIBRARY_PATH : ${nixpkgs.lib.makeLibraryPath [ pkgs.libGL pkgs.libxkbcommon pkgs.wayland pkgs.libX11 pkgs.libxcb ]}
            '';

            meta = {
              description = "Native Linux color picker and screen ruler";
              homepage = "https://github.com/Kuucheen/PixelKit";
              license = pkgs.lib.licenses.mit;
              mainProgram = "pixelkit";
              platforms = pkgs.lib.platforms.linux;
            };
          };
        });

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/pixelkit";
        };
      });
    };
}
