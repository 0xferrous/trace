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

        svmReleasesList = pkgs.fetchurl {
          url = "https://binaries.soliditylang.org/linux-amd64/list.json";
          sha256 = "sha256-bdIZHHwZM4v31Vjgyrb1JvmKQQPsB+h5WAoMDd7IWrw=";
        };

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ openssl ];
          version = "0.1.0";
          env = {
            SVM_RELEASES_LIST_JSON = svmReleasesList;
          };
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        traces-cli = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            pname = "traces-cli";
            cargoExtraArgs = "-p traces-cli";
          }
        );

        backend = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            pname = "backend";
            cargoExtraArgs = "-p backend";
          }
        );

        docker-image-cli = pkgs.dockerTools.buildLayeredImage {
          name = "traces-cli";
          tag = "latest";
          contents = [ traces-cli ];
          config = {
            Cmd = [ "${traces-cli}/bin/traces-cli" ];
            Env = [ "PATH=${pkgs.lib.makeBinPath [ traces-cli ]}" ];
          };
        };

        docker-image-backend = pkgs.dockerTools.buildLayeredImage {
          name = "traces-backend";
          tag = "latest";
          contents = [ backend ];
          config = {
            Cmd = [ "${backend}/bin/backend" ];
            Env = [ "PATH=${pkgs.lib.makeBinPath [ backend ]}" ];
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
          default = traces-cli;
          inherit
            traces-cli
            backend
            docker-image-cli
            docker-image-backend
            ;
        };
      }
    );
}
