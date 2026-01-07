{
  description = "traces-tui - A Rust TUI application";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      crane,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        craneLib = crane.mkLib pkgs;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          pname = "traces-cli";
          version = "0.1.0";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        traces-tui = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            cargoExtraArgs = "-p traces-cli";
          }
        );

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
            trunk
            pkg-config
            openssl
            (rust-bin.selectLatestNightlyWith (
              toolchain:
              toolchain.default.override {
                extensions = [ "rust-src" ];
                targets = [ "wasm32-unknown-unknown" ];
              }
            ))
          ];
        };

        packages = {
          default = traces-tui;
          inherit traces-tui docker-image;
        };
      }
    );
}
