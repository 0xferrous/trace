#!/usr/bin/env nix-shell
#! nix-shell -i nu -p nushell nix

print "Building traces-backend docker image..."
rm -f result-backend
nix build .#docker-image-backend --out-link result-backend
print "✓ Backend docker image built successfully"
