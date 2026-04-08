#!/bin/sh
set -e

echo "Starting Tailscale daemon..."
mkdir -p /var/lib/tailscale /var/run/tailscale
tailscaled --tun=userspace-networking --state=/var/lib/tailscale/tailscaled.state >/var/log/tailscaled.log 2>&1 &

# Wait for tailscaled to start
sleep 3

if [ -n "$TAILSCALE_AUTH_KEY" ]; then
  echo "Connecting to Tailscale network..."
  tailscale up --authkey="$TAILSCALE_AUTH_KEY" --accept-routes=true &
fi

# Wait for Tailscale connection
sleep 8

echo "Configuring OpenClaw for lan mode (accessible via Tailscale)..."
mkdir -p /root/.openclaw

# Get configuration from environment or use defaults
PORT=${PORT:-18790}
LLM_PROVIDER=${LLM_PROVIDER:-ollama}
LLM_MODEL=${LLM_MODEL:-llama3}
LLM_FULL="${LLM_PROVIDER}/${LLM_MODEL}"

# Create OpenClaw config with lan binding
cat > /root/.openclaw/openclaw.json <<EOF
{
  "gateway": {
    "port": ${PORT},
    "mode": "local",
    "bind": "lan",
    "auth": {"mode": "none"}
  },
  "agents": {
    "defaults": {
      "model": {
        "primary": "${LLM_FULL}"
      }
    }
  }
}
EOF

echo "OpenClaw configured to bind to lan mode on port ${PORT}"
echo "Starting OpenClaw gateway..."

# Execute original entrypoint
exec /entrypoint.sh "$@"
