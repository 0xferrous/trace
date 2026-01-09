#!/usr/bin/env nix-shell
#! nix-shell -i nu -p nushell nix

print "Building traces-cli docker image..."
rm -f result-cli
nix build .#docker-image-cli --out-link result-cli
print "✓ CLI docker image built successfully"
