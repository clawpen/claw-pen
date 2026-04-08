#!/bin/sh
echo "Starting Tailscale agent..."
tailscaled --tun=userspace-networking --state=/var/lib/tailscale/tailscaled.state >/var/log/tailscaled.log 2>&1 &
sleep 3

if [ -n "$TAILSCALE_AUTH_KEY" ]; then
  echo "Connecting to Tailscale..."
  tailscale up --authkey="$TAILSCALE_AUTH_KEY" &
fi

sleep 5

echo "Configuring OpenClaw to bind to 0.0.0.0..."
mkdir -p /root/.openclaw

# Generate config with correct environment variables
LLM_FULL="${LLM_PROVIDER:-ollama}/${LLM_MODEL:-llama3}"

cat > /root/.openclaw/openclaw.json <<CONFIGEOF
{
  "gateway": {
    "port": ${PORT:-18790},
    "mode": "local",
    "bind": "0.0.0.0",
    "auth": {"mode": "none"}
  },
  "agents": {
    "defaults": {
      "model": {
        "primary": "$LLM_FULL"
      }
    }
  }
}
CONFIGEOF

echo "Starting OpenClaw gateway..."
cd /agent && exec node /usr/local/lib/node_modules/openclaw/dist/index.js "$@"
