#!/bin/bash
#
# AndOR Hub + OpenClaw Bootstrap Script
# 
# Run this on a fresh Ubuntu 22.04+ VM
# Requirements: 75GB disk, 4GB RAM recommended
#
# Usage:
#   wget -qO- https://your-server/bootstrap.sh | bash
#   OR
#   ./bootstrap.sh

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "${BLUE}[AndOR]${NC} $1"; }
ok() { echo -e "${GREEN}[✓]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }
err() { echo -e "${RED}[✗]${NC} $1"; exit 1; }

# Check for root
if [ "$EUID" -ne 0 ]; then
  err "Please run as root: sudo ./bootstrap.sh"
fi

echo ""
echo -e "${CYAN}╔══════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     AndOR Hub + OpenClaw Bootstrap Script           ║${NC}"
echo -e "${CYAN}║                                                      ║${NC}"
echo -e "${CYAN}║  This will set up:                                   ║${NC}"
echo -e "${CYAN}║  - Node.js 20                                        ║${NC}"
echo -e "${CYAN}║  - Tailscale (secure networking)                     ║${NC}"
echo -e "${CYAN}║  - OpenClaw (AI assistant)                           ║${NC}"
echo -e "${CYAN}║  - AndOR Hub (chat, git, files)                      ║${NC}"
echo -e "${CYAN}║  - LM Studio / Ollama (local LLM)                    ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════╝${NC}"
echo ""

# Ask for configuration
read -p "$(echo -e ${CYAN}Install LM Studio for local LLM? [Y/n]:${NC} )" -n 1 -r
echo
INSTALL_LMSTUDIO=${REPLY:-Y}

read -p "$(echo -e ${CYAN}Install Ollama instead? [y/N]:${NC} )" -n 1 -r
echo
INSTALL_OLLAMA=${REPLY:-N}

read -p "$(echo -e ${CYAN}AndOR Hub git repo URL [press Enter for default]:${NC} )" REPO_URL
REPO_URL=${REPO_URL:-"https://github.com/yourusername/andor-hub.git"}

# --- STEP 1: System Dependencies ---
log "Step 1/8: Installing system dependencies..."
apt-get update -qq
apt-get install -y -qq \
  curl wget git build-essential \
  python3 python3-pip \
  sqlite3 ca-certificates gnupg lsb-release \
  ufw jq > /dev/null
ok "System dependencies installed"

# --- STEP 2: Node.js ---
log "Step 2/8: Installing Node.js 20..."
if command -v node &> /dev/null && [ "$(node -v | cut -d'v' -f2 | cut -d'.' -f1)" -ge 20 ]; then
  ok "Node.js $(node -v) already installed"
else
  curl -fsSL https://deb.nodesource.com/setup_20.x | bash - > /dev/null
  apt-get install -y -qq nodejs > /dev/null
  ok "Node.js $(node -v) installed"
fi

# --- STEP 3: Tailscale ---
log "Step 3/8: Installing Tailscale..."
if command -v tailscale &> /dev/null; then
  ok "Tailscale already installed"
else
  curl -fsSL https://tailscale.com/install.sh | sh
  ok "Tailscale installed"
  
  echo ""
  warn "Tailscale needs to be connected!"
  echo "  Run: tailscale up"
  echo "  Or set TAILSCALE_AUTH_KEY env var for auto-connect"
  
  if [ -n "$TAILSCALE_AUTH_KEY" ]; then
    tailscale up --authkey=$TAILSCALE_AUTH_KEY --hostname=andor-hub
    ok "Connected to Tailscale"
  else
    read -p "$(echo -e ${CYAN}Connect to Tailscale now? [Y/n]:${NC} )" -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Nn]$ ]]; then
      tailscale up --hostname=andor-hub
    fi
  fi
fi

# --- STEP 4: OpenClaw ---
log "Step 4/8: Installing OpenClaw..."
if command -v openclaw &> /dev/null; then
  ok "OpenClaw already installed: $(openclaw --version)"
else
  npm install -g openclaw
  ok "OpenClaw installed"
  
  # Start OpenClaw gateway
  openclaw gateway start
  sleep 2
  ok "OpenClaw gateway started on port 18789"
fi

# --- STEP 5: Local LLM ---
log "Step 5/8: Setting up local LLM..."

if [[ $INSTALL_OLLAMA =~ ^[Yy]$ ]]; then
  log "Installing Ollama..."
  curl -fsSL https://ollama.com/install.sh | sh
  systemctl start ollama
  ok "Ollama installed"
  
  # Pull a default model
  read -p "$(echo -e ${CYAN}Pull a model? (e.g., llama3.2, mistral) [llama3.2]:${NC} )" MODEL
  MODEL=${MODEL:-llama3.2}
  ollama pull $MODEL
  ok "Model $MODEL downloaded"
  
  LLM_URL="http://localhost:11434/v1"
  
elif [[ $INSTALL_LMSTUDIO =~ ^[Yy]$ ]] || [[ -z $INSTALL_LMSTUDIO ]]; then
  warn "LM Studio requires desktop environment or manual download"
  echo "  Download from: https://lmstudio.ai"
  echo "  After install, load a model and start the server on port 1234"
  LLM_URL="http://localhost:1234/v1"
else
  warn "No local LLM installed. Configure manually."
  LLM_URL="http://localhost:1234/v1"
fi

# --- STEP 6: AndOR Hub ---
log "Step 6/8: Installing AndOR Hub..."

INSTALL_DIR="/opt/andor"

if [ -d "$INSTALL_DIR/.git" ]; then
  log "Repository exists, updating..."
  cd $INSTALL_DIR
  git pull
else
  log "Cloning AndOR Hub..."
  mkdir -p $INSTALL_DIR
  git clone $REPO_URL $INSTALL_DIR
fi

cd $INSTALL_DIR

# Install dependencies
npm install --production

# Build if needed
if [ -f "tsconfig.json" ]; then
  npm run build 2>/dev/null || npx tsc 2>/dev/null || true
fi

ok "AndOR Hub installed"

# --- STEP 7: Configuration ---
log "Step 7/8: Configuring..."

# Generate secrets
ADMIN_SECRET=$(openssl rand -hex 32)
JWT_SECRET=$(openssl rand -hex 32)

# Get Tailscale IP
TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "")

# Create .env
cat > $INSTALL_DIR/.env << EOF
# AndOR Hub Configuration - Generated by bootstrap

# Server
PORT=8080
HOST=0.0.0.0
NODE_ENV=production

# Paths
DATA_DIR=/var/lib/andor
REPO_ROOT=/var/lib/andor/git-data
WORKING_DIR=/var/lib/andor/workspace

# Security
ADMIN_SECRET=$ADMIN_SECRET
JWT_SECRET=$JWT_SECRET

# OpenClaw Integration
OPENCLAW_ENABLED=true
OPENCLAW_GATEWAY_URL=http://localhost:18789

# Local LLM
LM_STUDIO_URL=$LLM_URL

# Storage
STORAGE_TYPE=local
STORAGE_PATH=/var/lib/andor/uploads
EOF

# Create data directories
mkdir -p /var/lib/andor/{git-data,uploads,builds,logs,workspace}
chown -R $SUDO_USER:$SUDO_USER /var/lib/andor
chown -R $SUDO_USER:$SUDO_USER $INSTALL_DIR

ok "Configuration created"

# --- STEP 8: Services ---
log "Step 8/8: Creating systemd services..."

# OpenClaw service
cat > /etc/systemd/system/openclaw-gateway.service << EOF
[Unit]
Description=OpenClaw Gateway
After=network.target

[Service]
Type=simple
User=$SUDO_USER
ExecStart=/usr/bin/openclaw gateway start
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

# AndOR Hub service
cat > /etc/systemd/system/andor-hub.service << EOF
[Unit]
Description=AndOR Hub Server
After=network.target openclaw-gateway.service
Wants=openclaw-gateway.service

[Service]
Type=simple
User=$SUDO_USER
WorkingDirectory=$INSTALL_DIR
ExecStart=/usr/bin/node $INSTALL_DIR/src/index.js
Restart=always
RestartSec=10
EnvironmentFile=$INSTALL_DIR/.env

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable openclaw-gateway
systemctl enable andor-hub

ok "Services created"

# --- Start services ---
log "Starting services..."

systemctl start openclaw-gateway
sleep 2
systemctl start andor-hub
sleep 2

ok "Services started"

# --- Firewall ---
log "Configuring firewall..."
ufw --force reset > /dev/null 2>&1
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 8080/tcp
ufw allow 18789/tcp
if ip link show tailscale0 &> /dev/null; then
  ufw allow in on tailscale0
fi
ufw --force enable > /dev/null
ok "Firewall configured"

# --- Summary ---
LOCAL_IP=$(hostname -I | awk '{print $1}')
TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "not connected")

echo ""
echo -e "${GREEN}╔══════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║           Bootstrap Complete! 🎉                    ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  ${CYAN}AndOR Hub:${NC}       http://$LOCAL_IP:8080"
if [ -n "$TAILSCALE_IP" ] && [ "$TAILSCALE_IP" != "not connected" ]; then
  echo -e "  ${CYAN}Tailscale:${NC}       http://$TAILSCALE_IP:8080"
fi
echo -e "  ${CYAN}OpenClaw API:${NC}    http://localhost:18789"
echo ""
echo -e "  ${CYAN}Config:${NC}          $INSTALL_DIR/.env"
echo -e "  ${CYAN}Data:${NC}            /var/lib/andor/"
echo ""
echo -e "${YELLOW}Creating admin user...${NC}"
read -p "Username [admin]: " ADMIN_USER
ADMIN_USER=${ADMIN_USER:-admin}
read -s -p "Password: " ADMIN_PASS
echo
read -s -p "Confirm password: " ADMIN_PASS2
echo

if [ "$ADMIN_PASS" != "$ADMIN_PASS2" ]; then
  warn "Passwords don't match, skipping user creation"
else
  curl -s -X POST http://localhost:8080/register \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$ADMIN_USER\",\"password\":\"$ADMIN_PASS\",\"inviteCode\":\"$ADMIN_SECRET\"}" > /dev/null
  
  if [ $? -eq 0 ]; then
    ok "User '$ADMIN_USER' created!"
  else
    warn "Could not create user. Create manually at http://localhost:8080/register"
  fi
fi

echo ""
echo -e "${YELLOW}Next Steps:${NC}"
echo "  1. Start your local LLM (LM Studio or Ollama)"
echo "  2. Open AndOR Hub: http://${TAILSCALE_IP:-$LOCAL_IP}:8080"
echo "  3. Login with: $ADMIN_USER / (your password)"
echo "  4. Chat with Codi in any channel! 🧩"
echo ""
echo -e "${YELLOW}Admin Secret (for adding more users):${NC} $ADMIN_SECRET"
echo ""
