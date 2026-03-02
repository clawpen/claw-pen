#!/bin/sh
set -e

# Configure OpenClaw from environment variables
MODEL="${LLM_MODEL:-glm-5}"
PROVIDER="${LLM_PROVIDER:-zai}"
PROVIDER=$(echo "$PROVIDER" | tr '[:upper:]' '[:lower:]')
PORT="${PORT:-18790}"

case "$PROVIDER" in
  kimi|kimi-code) API_KEY="$KIMI_API_KEY" ;;
  zai) API_KEY="$ZAI_API_KEY" ;;
  anthropic) API_KEY="$ANTHROPIC_API_KEY" ;;
  openai) API_KEY="$OPENAI_API_KEY" ;;
  google|gemini) API_KEY="$GOOGLE_API_KEY" ;;
  *) API_KEY="${ZAI_API_KEY:-${ANTHROPIC_API_KEY:-${OPENAI_API_KEY:-}}}" ;;
esac

echo "[entrypoint] Provider: $PROVIDER, Model: $MODEL, Port: $PORT, API Key: ${API_KEY:+set}"

mkdir -p /root/.openclaw/agents/dev/agent

# Include port in gateway config
cat > /root/.openclaw/openclaw.json << CONF
{
  "meta": {"lastTouchedVersion": "2026.2.26"},
  "agents": {
    "defaults": {"workspace": "/root/.openclaw/workspace-dev", "skipBootstrap": true, "model": {"primary": "${PROVIDER}/${MODEL}"}},
    "list": [{"id": "dev", "default": true, "workspace": "/root/.openclaw/workspace-dev", "identity": {"name": "Agent", "emoji": "🤖"}, "model": {"primary": "${PROVIDER}/${MODEL}"}}]
  },
  "gateway": {"mode": "local", "bind": "loopback", "auth": {"mode": "none"}, "port": ${PORT}}
}
CONF

if [ -n "$API_KEY" ]; then
  cat > /root/.openclaw/agents/dev/agent/auth-profiles.json << AUTH
{"version":1,"profiles":{"${PROVIDER}:default":{"type":"api_key","provider":"${PROVIDER}","key":"${API_KEY}"}},"lastGood":{"${PROVIDER}":"${PROVIDER}:default"}}
AUTH
fi

exec node /usr/local/lib/node_modules/openclaw/openclaw.mjs gateway run --dev --auth none --allow-unconfigured
