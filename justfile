build-web:
    trunk build --config ./crates/web/trunk.toml index.html

serve-web:
    trunk serve --config ./crates/web/trunk.toml -p 1111 index.html

backend:
    cargo r --bin trace-backend -- -p 2222

# Prefetch flake inputs to avoid duplicate fetching
flake-prefetch:
    nix flake prefetch
    nix flake prefetch-inputs

build-cargo-artifacts:
    nix build .#cargoArtifacts

# Build docker images in parallel
[parallel]
docker-build: docker-build-cli docker-build-backend

docker-build-cli: flake-prefetch
    .github/workflows/build-docker-cli.nu

docker-build-backend: flake-prefetch
    .github/workflows/build-docker-backend.nu

# Publish docker images
docker-publish:
    .github/workflows/publish-docker.nu
