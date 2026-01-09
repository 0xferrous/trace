#!/usr/bin/env nix-shell
#! nix-shell -i nu -p nushell docker

# Required environment variables
let registry = $env.REGISTRY? | default (error make { msg: "REGISTRY env var is required" })
let image_name = $env.IMAGE_NAME? | default (error make { msg: "IMAGE_NAME env var is required" })

# Optional: VERSION for tagging (defaults to 'latest')
let version = $env.VERSION? | default "latest"

# Check if we should skip push (e.g., for pull requests)
let skip_push = ($env.SKIP_PUSH? | default "false") == "true"

print "Loading and tagging docker images..."

# Load CLI image
if ("result-cli" | path exists) {
    print "Loading CLI image..."
    docker load < result-cli
    let cli_tag = $"($registry)/($image_name)-cli:($version)"
    docker tag traces-cli:latest $cli_tag
    print $"✓ CLI image tagged as ($cli_tag)"
} else {
    error make { msg: "result-cli not found. Run 'just docker-build' first." }
}

# Load backend image
if ("result-backend" | path exists) {
    print "Loading backend image..."
    docker load < result-backend
    let backend_tag = $"($registry)/($image_name)-backend:($version)"
    docker tag traces-backend:latest $backend_tag
    print $"✓ Backend image tagged as ($backend_tag)"
} else {
    error make { msg: "result-backend not found. Run 'just docker-build' first." }
}

# Push images (skip if SKIP_PUSH is true)
if $skip_push {
    print "Skipping push (SKIP_PUSH=true)"
    exit 0
}

print $"Pushing images to ($registry)..."
let cli_tag = $"($registry)/($image_name)-cli:($version)"
let backend_tag = $"($registry)/($image_name)-backend:($version)"

docker push $cli_tag
print $"✓ Pushed ($cli_tag)"

docker push $backend_tag
print $"✓ Pushed ($backend_tag)"

# Also tag and push 'latest' if VERSION is not 'latest'
if $version != "latest" {
    print "Also pushing 'latest' tags..."
    docker tag $cli_tag $"($registry)/($image_name)-cli:latest"
    docker push $"($registry)/($image_name)-cli:latest"
    print $"✓ Pushed ($registry)/($image_name)-cli:latest"

    docker tag $backend_tag $"($registry)/($image_name)-backend:latest"
    docker push $"($registry)/($image_name)-backend:latest"
    print $"✓ Pushed ($registry)/($image_name)-backend:latest"
}

print "✓ All images published successfully"
