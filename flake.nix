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
      self,
    }:
    let
      inherit (nixpkgs) lib;
      eachSystem = lib.genAttrs lib.systems.flakeExposed;
      cargo = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      workspace = cargo.workspace.package;
      pkgsFor = eachSystem (
        system:
        import nixpkgs {
          localSystem.system = system;
          overlays = [ (import rust-overlay) ];
        }
      );
    in
    {
      packages = eachSystem (
        system:
        let
          pkgs = pkgsFor.${system};
          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          };
        in
        {
          default = rustPlatform.buildRustPackage {
            pname = "basel";
            version = workspace.version;
            src = lib.cleanSource ./.;
            cargoLock.lockFile = ./Cargo.lock;

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

            meta = with lib; {
              description = workspace.description;
              homepage = workspace.repository;
              license = workspace.license;
              mainProgram = "basel";
              platforms = platforms.unix;
            };
          };
        }
      );

      devShells = eachSystem (
        system:
        let
          pkgs = pkgsFor.${system};
          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          pkg = self.packages.${system}.default;
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ pkg ];

            nativeBuildInputs = [
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
               echo "basel developer environment active"
               echo "rust: $(rustc --version)"
               echo "  on: ${system}"
               echo ""
            '';
          };
        }
      );

      formatter = eachSystem (system: pkgsFor.${system}.nixfmt-rfc-style);
    };
}
