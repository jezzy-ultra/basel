{
  description = "full-fat color scheme engine. write once, theme everything.";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    crane,
  }: let
    inherit (nixpkgs) lib;
    eachSystem = lib.genAttrs lib.systems.flakeExposed;
    cargo = builtins.fromTOML (builtins.readFile ./Cargo.toml);
    workspace = cargo.workspace.package;

    pkgsFor = eachSystem (
      system:
        import nixpkgs {
          localSystem.system = system;
          overlays = [(import rust-overlay)];
        }
    );
  in {
    packages = eachSystem (
      system: let
        pkgs = pkgsFor.${system};
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;

          pname = "theythemer";
          version = workspace.version;

          nativeBuildInputs = with pkgs; [
            clang
            mold
            pkg-config
          ];

          buildInputs = with pkgs; [
            openssl
            zlib
          ];

          RUSTFLAGS = "-C link-arg=-fuse-ld=mold";

          # disable sccache in nix builds
          CARGO_BUILD_RUSTC_WRAPPER = "";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      in {
        default = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            meta = with lib; {
              description = workspace.description;
              homepage = workspace.repository;
              license = workspace.license;
              mainProgram = "they";
              platforms = platforms.unix;
            };
          }
        );
      }
    );

    devShells = eachSystem (
      system: let
        pkgs = pkgsFor.${system};
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        pkg = self.packages.${system}.default;
      in {
        default = pkgs.mkShell {
          inputsFrom = [pkg];

          nativeBuildInputs = [
            pkgs.alejandra
            rustToolchain
          ];

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          RUST_BACKTRACE = "1";

          shellHook = ''
            ${lib.optionalString pkgs.stdenv.isLinux ''
              export LD_LIBRARY_PATH="${lib.makeLibraryPath pkg.buildInputs}:''${LD_LIBRARY_PATH:-}"
            ''}

            ${lib.optionalString pkgs.stdenv.isDarwin ''
              export DYLD_LIBRARY_PATH="${lib.makeLibraryPath pkg.buildInputs}:''${DYLD_LIBRARY_PATH:-}"
            ''}

             echo ""
             echo "theythemer developer environment active"
             echo "rust: $(rustc --version)"
             echo "  on: ${system}"
             echo ""
          '';
        };
      }
    );

    formatter = eachSystem (system: pkgsFor.${system}.alejandra);
  };
}
