#!/bin/sh
set -e

# Configuration from environment
MODEL="${LLM_MODEL:-glm-5}"
PROVIDER="${LLM_PROVIDER:-zai}"
PROVIDER=$(echo "$PROVIDER" | tr '[:upper:]' '[:lower:]')
PORT="${PORT:-18790}"
BASE_URL=""

# Set API key based on provider
case "$PROVIDER" in
  zai) 
    API_KEY="$ZAI_API_KEY" 
    BASE_URL="https://api.z.ai/api/coding/paas/v4"
    ;;
  kimi|kimi-code) 
    API_KEY="$KIMI_API_KEY" 
    BASE_URL="https://api.kimi.com/coding/v1"
    PROVIDER="openai"
    ;;
  anthropic) API_KEY="$ANTHROPIC_API_KEY" ;;
  openai) 
    API_KEY="$OPENAI_API_KEY" 
    BASE_URL="${OPENAI_BASE_URL:-}"
    ;;
  google|gemini) API_KEY="$GOOGLE_API_KEY" ;;
  *) API_KEY="${ZAI_API_KEY:-${ANTHROPIC_API_KEY:-${OPENAI_API_KEY:-}}}" ;;
esac

echo "[entrypoint] Starting OpenClaw Agent (Full)"
echo "[entrypoint] Provider: $PROVIDER, Model: $MODEL, Port: $PORT"
echo "[entrypoint] API Key: ${API_KEY:+set}, BaseURL: ${BASE_URL:-default}"
echo "[entrypoint] Browser: $CHROME_BIN"

# Create OpenClaw config directory
mkdir -p /root/.openclaw/agents/dev/agent
mkdir -p /root/.openclaw/skills
mkdir -p /root/.openclaw/workspace
mkdir -p /root/.openclaw/canvas

# Link skills from /agent to config
if [ -d "/agent/.openclaw/skills" ] && [ "$(ls -A /agent/.openclaw/skills 2>/dev/null)" ]; then
    cp -r /agent/.openclaw/skills/* /root/.openclaw/skills/ 2>/dev/null || true
    echo "[entrypoint] Copied bundled skills"
fi

# Generate OpenClaw config
cat > /root/.openclaw/openclaw.json << CONF
{
  "meta": {"lastTouchedVersion": "2026.2.26"},
  "agents": {
    "defaults": {
      "workspace": "/agent",
      "skipBootstrap": true,
      "model": {"primary": "${PROVIDER}/${MODEL}"}
    },
    "list": [{
      "id": "dev",
      "default": true,
      "workspace": "/agent",
      "identity": {"name": "Agent", "emoji": "🤖"},
      "model": {"primary": "${PROVIDER}/${MODEL}"}
    }]
  },
  "gateway": {
    "mode": "local",
    "bind": "lan",
    "auth": {"mode": "none"},
    "port": ${PORT}
  },
  "browser": {
    "enabled": true,
    "headless": true
  }
}
CONF

# Generate auth profiles
if [ -n "$API_KEY" ]; then
  if [ -n "$BASE_URL" ]; then
    cat > /root/.openclaw/agents/dev/agent/auth-profiles.json << AUTH
{"version":1,"profiles":{"${PROVIDER}:default":{"type":"api_key","provider":"${PROVIDER}","key":"${API_KEY}","baseUrl":"${BASE_URL}"}},"lastGood":{"${PROVIDER}":"${PROVIDER}:default"}}
AUTH
  else
    cat > /root/.openclaw/agents/dev/agent/auth-profiles.json << AUTH
{"version":1,"profiles":{"${PROVIDER}:default":{"type":"api_key","provider":"${PROVIDER}","key":"${API_KEY}"}},"lastGood":{"${PROVIDER}":"${PROVIDER}:default"}}
AUTH
  fi
  echo "[entrypoint] Auth profiles configured"
fi

# Start OpenClaw gateway
echo "[entrypoint] Starting gateway on port $PORT..."
exec node /usr/local/lib/node_modules/openclaw/openclaw.mjs gateway run --dev --auth none --allow-unconfigured
