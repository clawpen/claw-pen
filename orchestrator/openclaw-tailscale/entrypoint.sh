#!/bin/sh
# Custom entrypoint for OpenClaw with Tailscale support

# Start Tailscale in background if auth key is provided
if [ -n "$TAILSCALE_AUTH_KEY" ]; then
    echo "Starting Tailscale..."
    mkdir -p /var/run/tailscale /var/lib/tailscale

    # Start tailscaled with userspace networking (no TUN device needed)
    tailscaled --tun=userspace-networking \
        --state=/var/lib/tailscale/tailscaled.state \
        --socket=/var/run/tailscale/tailscaled.sock \
        >/var/log/tailscaled.log 2>&1 &

    # Wait for tailscaled to start
    sleep 3

    # Connect to Tailscale
    tailscale up --authkey="$TAILSCALE_AUTH_KEY" >/dev/null 2>&1 &

    echo "Tailscale started"
fi

# Create/update OpenClaw config to bind to all interfaces (for Tailscale access)
mkdir -p /root/.openclaw
cat > /root/.openclaw/openclaw.json <<EOF
{
  "gateway": {
    "port": ${PORT:-18790},
    "mode": "local",
    "bind": "0.0.0.0",
    "auth": {
      "mode": "none"
    }
  },
  "agents": {
    "defaults": {
      "model": {
        "primary": "${LLM_PROVIDER:-ollama}/${LLM_MODEL:-llama3}"
      }
    }
  }
}
EOF

# Run the original OpenClaw entrypoint
exec /entrypoint.sh "$@"
