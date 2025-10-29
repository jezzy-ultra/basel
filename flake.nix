{
  description = "full-fat color scheme engine. write once, theme everything.";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      inherit (nixpkgs) lib;
      eachSystem = lib.genAttrs lib.systems.flakeExposed;

      pkgsFor = eachSystem (
        system:
        import nixpkgs {
          localSystem.system = system;
          overlays = [ (import rust-overlay) ];
        }
      );
    in
    {
      devShells = eachSystem (
        system:
        let
          pkgs = pkgsFor.${system};

          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

          linuxPackages = lib.optionals pkgs.stdenv.isLinux [
            pkgs.atk
            pkgs.cairo
            pkgs.fontconfig
            pkgs.freetype
            pkgs.fribidi
            pkgs.gdk-pixbuf
            pkgs.glib
            pkgs.gtk3
            pkgs.harfbuzz
            pkgs.libsoup_3
            pkgs.pango
            pkgs.pixman
            pkgs.wayland
            pkgs.wayland-protocols
            pkgs.webkitgtk_4_1
            pkgs.xorg.libX11
            pkgs.xorg.libXcursor
            pkgs.xorg.libXdamage
            pkgs.xorg.libXext
            pkgs.xorg.libXfixes
            pkgs.xorg.libXi
            pkgs.xorg.libXrandr
            pkgs.xorg.libXrender
          ];

          darwinPackages = lib.optionals pkgs.stdenv.isDarwin (
            with pkgs.darwin.apple_sdk.frameworks;
            [
              IOKit
              Carbon
              WebKit
              Security
              Cocoa
              CoreFoundation
            ]
          );
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = [
              rustToolchain
              pkgs.pkg-config
              pkgs.openssl
            ]
            ++ linuxPackages
            ++ darwinPackages;

            shellHook = ''
              export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library"

              ${lib.optionalString pkgs.stdenv.isLinux ''
                export PKG_CONFIG_PATH="${
                  lib.concatStringsSep ":" [
                    "${pkgs.atk.dev}/lib/pkgconfig"
                    "${pkgs.cairo.dev}/lib/pkgconfig"
                    "${pkgs.fontconfig.dev}/lib/pkgconfig"
                    "${pkgs.freetype.dev}/lib/pkgconfig"
                    "${pkgs.fribidi.dev}/lib/pkgconfig"
                    "${pkgs.gdk-pixbuf.dev}/lib/pkgconfig"
                    "${pkgs.glib.dev}/lib/pkgconfig"
                    "${pkgs.gtk3.dev}/lib/pkgconfig"
                    "${pkgs.harfbuzz.dev}/lib/pkgconfig"
                    "${pkgs.libsoup_3.dev}/lib/pkgconfig"
                    "${pkgs.openssl.dev}/lib/pkgconfig"
                    "${pkgs.pango.dev}/lib/pkgconfig"
                    "${pkgs.wayland.dev}/lib/pkgconfig"
                    "${pkgs.webkitgtk_4_1.dev}/lib/pkgconfig"
                    "${pkgs.xorg.libX11.dev}/lib/pkgconfig"
                    "${pkgs.xorg.libXcursor.dev}/lib/pkgconfig"
                    "${pkgs.xorg.libXdamage.dev}/lib/pkgconfig"
                    "${pkgs.xorg.libXext.dev}/lib/pkgconfig"
                    "${pkgs.xorg.libXfixes.dev}/lib/pkgconfig"
                    "${pkgs.xorg.libXi.dev}/lib/pkgconfig"
                    "${pkgs.xorg.libXrandr.dev}/lib/pkgconfig"
                    "${pkgs.xorg.libXrender.dev}/lib/pkgconfig"
                  ]
                }"

                export LD_LIBRARY_PATH="${
                  lib.makeLibraryPath [
                    pkgs.atk
                    pkgs.cairo
                    pkgs.fontconfig
                    pkgs.freetype
                    pkgs.fribidi
                    pkgs.gdk-pixbuf
                    pkgs.glib
                    pkgs.gtk3
                    pkgs.harfbuzz
                    pkgs.libsoup_3
                    pkgs.pango
                    pkgs.pixman
                    pkgs.wayland
                    pkgs.wayland-protocols
                    pkgs.webkitgtk_4_1
                    pkgs.xorg.libX11
                    pkgs.xorg.libXcursor
                    pkgs.xorg.libXdamage
                    pkgs.xorg.libXext
                    pkgs.xorg.libXfixes
                    pkgs.xorg.libXi
                    pkgs.xorg.libXrandr
                    pkgs.xorg.libXrender
                  ]
                }"
              ''}

              ${lib.optionalString pkgs.stdenv.isDarwin ''
                export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"
              ''}

              echo ""
              echo "basel developer environment active"
              echo "rust: $(rustc --version)"
              echo "  on: ${system}"
              echo ""
            '';
          };
        }
      );
    };
}
