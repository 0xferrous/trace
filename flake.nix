{
  description = "traces-tui - A Rust TUI application";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        traces-tui = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });

        docker-image = pkgs.dockerTools.buildLayeredImage {
          name = "traces-tui";
          tag = "latest";
          contents = [ traces-tui ];
          config = {
            Cmd = [ "${traces-tui}/bin/traces-tui" ];
            Env = [ "PATH=${pkgs.lib.makeBinPath [ traces-tui ]}" ];
          };
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rust-analyzer
            rustfmt
            clippy
          ];

          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        };

        packages = {
          default = traces-tui;
          inherit traces-tui docker-image;
        };
      }
    );
}
