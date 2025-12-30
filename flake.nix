{
  description = "traces-tui - A Rust TUI application";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    fenix.url = "github:nix-community/fenix";
  };

  outputs = { self, nixpkgs, flake-utils, crane, fenix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;
        fenixPackages = fenix.packages.${system};

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          pname = "traces-cli";
          version = "0.1.0";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        traces-tui = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "-p traces-cli";
        });

        docker-image = pkgs.dockerTools.buildLayeredImage {
          name = "traces-tui";
          tag = "latest";
          contents = [ traces-tui ];
          config = {
            Cmd = [ "${traces-tui}/bin/traces-cli" ];
            Env = [ "PATH=${pkgs.lib.makeBinPath [ traces-tui ]}" ];
          };
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # rustc
            # cargo
            # rust-analyzer
            # rustfmt
            # clippy
            trunk
            # wasm-bindgen-cli_0_2_99
            (fenixPackages.combine [
              fenixPackages.stable.toolchain
              fenixPackages.targets.wasm32-unknown-unknown.stable.toolchain
            ])
          ];

          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
          CFLAGS_wasm32_unknown_unknown="-mno-reference-types";
        };

        packages = {
          default = traces-tui;
          inherit traces-tui docker-image;
        };
      }
    );
}
