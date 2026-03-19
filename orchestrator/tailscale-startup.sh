#!/bin/sh
set -e  # Exit on error

echo "=== Starting Tailscale installation ==="

# Install prerequisites
apt-get update
apt-get install -y curl ca-certificates gnupg

echo "=== Adding Tailscale repository ==="
# Add Tailscale GPG key
curl -fsSL https://pkgs.tailscale.com/stable/debian/bookworm.noarmor.gpg | tee /usr/share/keyrings/tailscale-archive-keyring.gpg > /dev/null

# Add Tailscale repository
curl -fsSL https://pkgs.tailscale.com/stable/debian/bookworm.tailscale-keyring.list | tee /etc/apt/sources.list.d/tailscale.list > /dev/null

echo "=== Installing Tailscale package ==="
# Update package list and install Tailscale
apt-get update
apt-get install -y tailscale

echo "=== Verifying installation ==="
which tailscale
tailscale version

echo "=== Starting Tailscale daemon ==="
# Create state directory
mkdir -p /var/lib/tailscale

# Start Tailscale daemon in userspace networking mode
tailscaled --tun=userspace-networking --state=/var/lib/tailscale/tailscaled.state > /var/log/tailscaled.log 2>&1 &

# Wait for daemon to start
sleep 5

echo "=== Connecting to Tailnet ==="
# Connect to Tailnet using auth key from environment
tailscale up --authkey=$TAILSCALE_AUTH_KEY --hostname=clawpen-$(hostname | head -c 10)

echo "=== Tailscale connected ==="
tailscale ip -4
tailscale status

echo "=== Starting OpenClaw ==="
# Configure and start OpenClaw gateway
cd /

# Remove old config to ensure we use the new one
rm -f /root/.openclaw/openclaw.json

# Create OpenClaw config directory
mkdir -p /root/.openclaw

# Generate OpenClaw configuration for gateway mode
# Bind to lan (all local interfaces) to accept connections via Tailnet IP
# Use simple password authentication (orchestrator knows the password)
cat > /root/.openclaw/openclaw.json <<EOF
{
  "gateway": {
    "port": ${PORT:-18792},
    "mode": "local",
    "bind": "lan",
    "auth": {
      "password": "clawpen"
    }
  },
  "agents": {
    "defaults": {
      "model": {
        "primary": "zai/glm-5"
      }
    }
  }
}
EOF

# Start OpenClaw gateway
exec node /usr/local/lib/node_modules/openclaw/dist/index.js gateway
