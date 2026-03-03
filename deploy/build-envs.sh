#!/bin/bash
#
# Build Environment Manager
# Creates and manages Docker-based build environments
#
# Usage:
#   ./build-envs.sh list              - List available environments
#   ./build-envs.sh create <name>     - Create a build environment
#   ./build-envs.sh shell <name>      - Open shell in environment
#   ./build-envs.sh build <name>      - Build project in environment
#   ./build-envs.sh export <name>     - Export environment as tarball

set -e

BUILDS_DIR="${BUILDS_DIR:-/opt/andor/builds}"
IMAGES_DIR="${IMAGES_DIR:-/opt/andor/deploy/build-images}"

# Available build environments
declare -A ENVIRONMENTS=(
    ["nodejs"]="node:20-alpine - Node.js 20 for JS/TS projects"
    ["nodejs-18"]="node:18-alpine - Node.js 18 (LTS)"
    ["python"]="python:3.12-slim - Python 3.12 for scripts/apps"
    ["rust"]="rust:latest - Rust compiler and cargo"
    ["go"]="golang:1.22-alpine - Go 1.22"
    ["dotnet"]="mcr.microsoft.com/dotnet/sdk:8.0 - .NET 8 SDK"
    ["arduino"]="arduino/arduino-cli:latest - Arduino/embedded"
    ["alpine"]="alpine:latest - Minimal Linux for binaries"
)

list_envs() {
    echo "Available build environments:"
    echo ""
    for env in "${!ENVIRONMENTS[@]}"; do
        echo "  $env - ${ENVIRONMENTS[$env]}"
    done
    echo ""
    echo "Existing environments in $BUILDS_DIR:"
    ls -1 "$BUILDS_DIR" 2>/dev/null || echo "  (none)"
}

create_env() {
    local name=$1
    local base_image=${2:-"nodejs"}
    
    # Map to actual image
    case $base_image in
        nodejs) image="node:20-alpine" ;;
        nodejs-18) image="node:18-alpine" ;;
        python) image="python:3.12-slim" ;;
        rust) image="rust:latest" ;;
        go) image="golang:1.22-alpine" ;;
        dotnet) image="mcr.microsoft.com/dotnet/sdk:8.0" ;;
        arduino) image="arduino/arduino-cli:latest" ;;
        alpine) image="alpine:latest" ;;
        *) image="$base_image" ;;  # Allow custom images
    esac
    
    local env_dir="$BUILDS_DIR/$name"
    mkdir -p "$env_dir"/{src,output,cache}
    
    # Create Dockerfile
    cat > "$env_dir/Dockerfile" << EOF
FROM $image
WORKDIR /src
RUN apk add --no-cache git curl 2>/dev/null || apt-get update && apt-get install -y git curl && rm -rf /var/lib/apt/lists/*
VOLUME ["/src", "/output", "/cache"]
EOF
    
    # Create build script
    cat > "$env_dir/build.sh" << 'EOF'
#!/bin/bash
# Add your build commands here
npm install && npm run build
# Output files should go to /output
cp -r dist/* /output/ 2>/dev/null || cp -r build/* /output/ 2>/dev/null || echo "No output found"
EOF
    chmod +x "$env_dir/build.sh"
    
    # Build the image
    docker build -t "andor-build-$name" "$env_dir"
    
    echo "Created build environment: $name"
    echo "Source: $env_dir/src"
    echo "Output: $env_dir/output"
}

shell_env() {
    local name=$1
    local env_dir="$BUILDS_DIR/$name"
    
    if [ ! -d "$env_dir" ]; then
        echo "Environment '$name' not found. Create it first:"
        echo "  $0 create $name"
        exit 1
    fi
    
    docker run -it --rm \
        -v "$env_dir/src:/src" \
        -v "$env_dir/output:/output" \
        -v "$env_dir/cache:/cache" \
        "andor-build-$name" /bin/sh
}

build_env() {
    local name=$1
    local env_dir="$BUILDS_DIR/$name"
    
    if [ ! -d "$env_dir" ]; then
        echo "Environment '$name' not found."
        exit 1
    fi
    
    docker run --rm \
        -v "$env_dir/src:/src" \
        -v "$env_dir/output:/output" \
        -v "$env_dir/cache:/cache" \
        "andor-build-$name" /bin/sh /src/build.sh
    
    echo "Build complete. Output in: $env_dir/output"
}

export_env() {
    local name=$1
    local env_dir="$BUILDS_DIR/$name"
    local output="/tmp/andor-build-$name-$(date +%Y%m%d-%H%M%S).tar.gz"
    
    tar -czf "$output" -C "$env_dir" output/
    echo "Exported to: $output"
}

case "$1" in
    list) list_envs ;;
    create) create_env "$2" "$3" ;;
    shell) shell_env "$2" ;;
    build) build_env "$2" ;;
    export) export_env "$2" ;;
    *)
        echo "Usage: $0 {list|create|shell|build|export} [name] [base-image]"
        echo ""
        list_envs
        exit 1
        ;;
esac
