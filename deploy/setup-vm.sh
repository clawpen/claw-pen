#!/bin/bash
#
# AndOR Hub + OpenClaw Setup Script
# Run this on a fresh Linux VM (75GB+ disk, 4GB+ RAM recommended)
#
# Usage:
#   curl -fsSL https://your-repo/setup-vm.sh | bash
#   OR
#   ./setup-vm.sh
#
# Prerequisites:
#   - Ubuntu 22.04+ or Debian 12+ (or RHEL/Fedora with adjustments)
#   - Internet access
#   - Tailscale auth key (optional, for auto-join)

set -e

# --- CONFIG ---
ANDOR_REPO="${ANDOR_REPO:-https://github.com/YOUR_USERNAME/andor-hub.git}"
ANDOR_BRANCH="${ANDOR_BRANCH:-main}"
OPENCLAW_VERSION="${OPENCLAW_VERSION:-latest}"
TAILSCALE_AUTH_KEY="${TAILSCALE_AUTH_KEY:-}"
INSTALL_DIR="${INSTALL_DIR:-/opt/andor}"
NODE_VERSION="${NODE_VERSION:-20}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() { echo -e "${BLUE}[AndOR]${NC} $1"; }
ok() { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
err() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# --- DETECT OS ---
detect_os() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        OS=$ID
        VER=$VERSION_ID
    else
        err "Cannot detect OS. Supported: Ubuntu, Debian"
    fi
    log "Detected: $OS $VER"
}

# --- INSTALL DEPENDENCIES ---
install_deps() {
    log "Installing system dependencies..."
    
    if [ "$OS" = "ubuntu" ] || [ "$OS" = "debian" ]; then
        apt-get update
        apt-get install -y \
            curl wget git build-essential \
            python3 python3-pip \
            sqlite3 \
            ca-certificates \
            gnupg \
            lsb-release \
            ufw
    elif [ "$OS" = "fedora" ] || [ "$OS" = "rhel" ]; then
        dnf install -y \
            curl wget git gcc gcc-c++ make \
            python3 python3-pip \
            sqlite \
            ca-certificates \
            firewalld
    else
        warn "Unknown OS, attempting Ubuntu-style install..."
        apt-get update && apt-get install -y curl wget git build-essential python3 sqlite3
    fi
    ok "System dependencies installed"
}

# --- INSTALL NODE.JS ---
install_node() {
    if command -v node &> /dev/null && [ "$(node -v | cut -d'v' -f2 | cut -d'.' -f1)" -ge "$NODE_VERSION" ]; then
        ok "Node.js $(node -v) already installed"
        return
    fi
    
    log "Installing Node.js $NODE_VERSION..."
    curl -fsSL https://deb.nodesource.com/setup_$NODE_VERSION.x | bash -
    apt-get install -y nodejs
    ok "Node.js $(node -v) installed"
}

# --- INSTALL DOCKER (OPTIONAL) ---
install_docker() {
    if command -v docker &> /dev/null; then
        ok "Docker $(docker --version) already installed"
        return
    fi
    
    log "Installing Docker..."
    curl -fsSL https://get.docker.com | sh
    systemctl enable docker
    systemctl start docker
    usermod -aG docker $SUDO_USER 2>/dev/null || true
    ok "Docker installed"
}

# --- INSTALL TAILSCALE ---
install_tailscale() {
    if command -v tailscale &> /dev/null; then
        ok "Tailscale already installed"
        return
    fi
    
    log "Installing Tailscale..."
    curl -fsSL https://tailscale.com/install.sh | sh
    ok "Tailscale installed"
    
    if [ -n "$TAILSCALE_AUTH_KEY" ]; then
        log "Connecting to Tailscale..."
        tailscale up --authkey=$TAILSCALE_AUTH_KEY --hostname=andor-hub
        ok "Connected to Tailscale"
    else
        warn "No TAILSCALE_AUTH_KEY set. Run 'tailscale up' manually to connect."
    fi
}

# --- INSTALL OPENCLAW ---
install_openclaw() {
    if command -v openclaw &> /dev/null; then
        ok "OpenClaw already installed: $(openclaw --version)"
        return
    fi
    
    log "Installing OpenClaw..."
    npm install -g openclaw
    ok "OpenClaw installed: $(openclaw --version)"
}

# --- CLONE ANDOR HUB ---
clone_andor() {
    log "Cloning AndOR Hub to $INSTALL_DIR..."
    
    if [ -d "$INSTALL_DIR/.git" ]; then
        log "Repository exists, pulling latest..."
        cd $INSTALL_DIR
        git pull origin $ANDOR_BRANCH
    else
        mkdir -p $INSTALL_DIR
        git clone -b $ANDOR_BRANCH $ANDOR_REPO $INSTALL_DIR
    fi
    ok "AndOR Hub cloned"
}

# --- SETUP ANDOR HUB ---
setup_andor() {
    log "Setting up AndOR Hub..."
    cd $INSTALL_DIR
    
    # Install dependencies
    npm install
    ok "Dependencies installed"
    
    # Build TypeScript if needed
    if [ -f "tsconfig.json" ]; then
        log "Building TypeScript..."
        npx tsc || npm run build || true
        ok "TypeScript built"
    fi
    
    # Create .env from template if not exists
    if [ ! -f ".env" ] && [ -f ".env.template" ]; then
        cp .env.template .env
        warn ".env created from template. Edit $INSTALL_DIR/.env with your settings."
    fi
    
    # Initialize database
    if [ -f "scripts/init-db.js" ]; then
        node scripts/init-db.js
    fi
    
    ok "AndOR Hub configured"
}

# --- INSTALL PM2 ---
install_pm2() {
    if command -v pm2 &> /dev/null; then
        ok "PM2 already installed"
        return
    fi
    
    log "Installing PM2..."
    npm install -g pm2
    pm2 startup systemd -u $SUDO_USER --hp /home/$SUDO_USER 2>/dev/null || true
    ok "PM2 installed"
}

# --- CREATE OPENCLAW CONFIG ---
create_openclaw_config() {
    log "Creating OpenClaw configuration..."
    
    OPENCLAW_DIR="/home/$SUDO_USER/.openclaw"
    mkdir -p $OPENCLAW_DIR
    
    # Create channels config with AndOR integration
    cat > $OPENCLAW_DIR/channels.json << 'EOF'
{
  "andor": {
    "enabled": true,
    "type": "webhook",
    "webhookUrl": "http://localhost:8080/api/openclaw/message",
    "description": "AndOR Hub Chat"
  }
}
EOF
    
    ok "OpenClaw config created at $OPENCLAW_DIR"
}

# --- CREATE SYSTEMD SERVICES ---
create_services() {
    log "Creating systemd services..."
    
    # AndOR Hub service
    cat > /etc/systemd/system/andor-hub.service << EOF
[Unit]
Description=AndOR Hub Server
After=network.target tailscale.service
Wants=tailscale.service

[Service]
Type=simple
User=$SUDO_USER
WorkingDirectory=$INSTALL_DIR
ExecStart=/usr/bin/node $INSTALL_DIR/src/index.js
Restart=always
RestartSec=10
Environment=NODE_ENV=production
Environment=OPENCLAW_WEBHOOK_URL=http://localhost:3456/webhook

[Install]
WantedBy=multi-user.target
EOF

    # OpenClaw Bridge service
    cat > /etc/systemd/system/openclaw-bridge.service << EOF
[Unit]
Description=OpenClaw AndOR Bridge
After=network.target andor-hub.service
Wants=andor-hub.service

[Service]
Type=simple
User=$SUDO_USER
WorkingDirectory=$INSTALL_DIR
ExecStart=/usr/bin/node $INSTALL_DIR/andor-bridge.js
Restart=always
RestartSec=10
Environment=PORT=3456
Environment=OPENCLAW_GATEWAY_URL=http://localhost:18789

[Install]
WantedBy=multi-user.target
EOF

    # OpenClaw Gateway service
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

    systemctl daemon-reload
    ok "Systemd services created"
}

# --- CONFIGURE FIREWALL ---
configure_firewall() {
    log "Configuring firewall..."
    
    if command -v ufw &> /dev/null; then
        ufw --force reset
        ufw default deny incoming
        ufw default allow outgoing
        
        # Allow Tailscale
        ufw allow in on tailscale0
        
        # Allow SSH (rate limited)
        ufw limit 22/tcp
        
        # AndOR Hub
        ufw allow 8080/tcp
        
        # OpenClaw Gateway
        ufw allow 18789/tcp
        
        # OpenClaw Bridge
        ufw allow 3456/tcp
        
        ufw --force enable
        ok "UFW firewall configured"
    elif command -v firewall-cmd &> /dev/null; then
        systemctl start firewalld
        firewall-cmd --permanent --add-service=ssh
        firewall-cmd --permanent --add-port=8080/tcp
        firewall-cmd --permanent --add-port=18789/tcp
        firewall-cmd --permanent --add-port=3456/tcp
        firewall-cmd --permanent --add-interface=tailscale0
        firewall-cmd --reload
        ok "Firewalld configured"
    else
        warn "No firewall detected. Consider installing ufw."
    fi
}

# --- START SERVICES ---
start_services() {
    log "Starting services..."
    
    # Start OpenClaw gateway first
    systemctl enable openclaw-gateway
    systemctl start openclaw-gateway
    sleep 3
    
    # Start AndOR Hub
    systemctl enable andor-hub
    systemctl start andor-hub
    sleep 2
    
    # Start OpenClaw Bridge
    systemctl enable openclaw-bridge
    systemctl start openclaw-bridge
    
    ok "All services started"
}

# --- PRINT SUMMARY ---
print_summary() {
    TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "not connected")
    HOSTNAME=$(hostname)
    
    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}   AndOR Hub + OpenClaw Setup Complete!${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""
    echo -e "AndOR Hub:      ${BLUE}http://$TAILSCALE_IP:8080${NC}"
    echo -e "OpenClaw API:   ${BLUE}http://$TAILSCALE_IP:18789${NC}"
    echo -e "Bridge Status:  ${BLUE}http://$TAILSCALE_IP:3456/status${NC}"
    echo ""
    echo "Useful commands:"
    echo "  sudo journalctl -u andor-hub -f       # View AndOR logs"
    echo "  sudo journalctl -u openclaw-gateway -f # View OpenClaw logs"
    echo "  sudo systemctl restart andor-hub      # Restart AndOR"
    echo "  openclaw dashboard                     # Open OpenClaw UI"
    echo ""
    echo -e "${YELLOW}Next steps:${NC}"
    echo "  1. Edit $INSTALL_DIR/.env with your settings"
    echo "  2. Create AndOR user: curl -X POST http://localhost:8080/api/register ..."
    echo "  3. Configure OpenClaw channels: ~/.openclaw/channels.json"
    echo ""
}

# --- MAIN ---
main() {
    log "Starting AndOR Hub + OpenClaw setup..."
    echo ""
    
    detect_os
    install_deps
    install_node
    install_docker
    install_tailscale
    install_openclaw
    clone_andor
    setup_andor
    install_pm2
    create_openclaw_config
    create_services
    configure_firewall
    start_services
    print_summary
}

main "$@"
