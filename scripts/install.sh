#!/bin/bash
# Claw Pen Quick Install
# curl -fsSL https://claw-pen.dev/install.sh | bash

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}🦀 Installing Claw Pen...${NC}"

# Check dependencies
command -v docker >/dev/null 2>&1 || {
    echo -e "${RED}Error: Docker is required${NC}"
    echo "Install Docker: https://docs.docker.com/get-docker/"
    exit 1
}

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux*)  PLATFORM="linux";;
    Darwin*) PLATFORM="macos";;
    *)       echo -e "${RED}Unsupported OS: $OS${NC}"; exit 1;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)  ARCH="amd64";;
    aarch64) ARCH="arm64";;
    *)       echo -e "${RED}Unsupported architecture: $ARCH${NC}"; exit 1;;
esac

# Download latest release
VERSION="${CLAW_PEN_VERSION:-latest}"
DOWNLOAD_URL="https://github.com/AchyErrorJ/claw-pen/releases/download/${VERSION}/claw-pen-${PLATFORM}-${ARCH}"

echo "Downloading Claw Pen ${VERSION} for ${PLATFORM}/${ARCH}..."

INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
TMP_FILE="/tmp/claw-pen"

curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE"
chmod +x "$TMP_FILE"

# Install
if [ "$EUID" -ne 0 ] && [ "$INSTALL_DIR" = "/usr/local/bin" ]; then
    sudo mv "$TMP_FILE" "$INSTALL_DIR/claw-pen"
else
    mv "$TMP_FILE" "$INSTALL_DIR/claw-pen"
fi

# Pull OpenClaw agent image
echo "Pulling OpenClaw agent image..."
docker pull ghcr.io/achyerrorj/claw-pen-openclaw-agent:latest

echo -e "${GREEN}✓ Claw Pen installed!${NC}"
echo ""
echo "Quick start:"
echo "  claw-pen create --name my-agent --provider openai"
echo "  claw-pen start my-agent"
echo ""
echo "Documentation: https://github.com/AchyErrorJ/claw-pen"
