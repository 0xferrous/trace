build-web:
    trunk build --config ./crates/web/trunk.toml index.html
serve-web:
    trunk serve --config ./crates/web/trunk.toml -p 1111 index.html
