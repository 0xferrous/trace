build-web:
    trunk build --config ./crates/web/trunk.toml index.html

serve-web:
    trunk serve --config ./crates/web/trunk.toml -p 1111 index.html

# Build docker images in parallel
[parallel]
docker-build: docker-build-cli docker-build-backend

docker-build-cli:
    .github/workflows/build-docker-cli.nu

docker-build-backend:
    .github/workflows/build-docker-backend.nu

# Publish docker images
docker-publish:
    .github/workflows/publish-docker.nu
