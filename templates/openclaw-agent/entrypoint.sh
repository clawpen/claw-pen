#!/bin/sh
set -e

# Configure OpenClaw from environment variables
MODEL="${LLM_MODEL:-glm-5}"
PROVIDER="${LLM_PROVIDER:-zai}"
API_KEY="${ZAI_API_KEY:-${ANTHROPIC_API_KEY:-${OPENAI_API_KEY:-}}}"

# Create config directory
mkdir -p /root/.openclaw/agents/dev/agent

# Write OpenClaw config
cat > /root/.openclaw/openclaw.json << CONF
{
  "meta": {"lastTouchedVersion": "2026.2.26"},
  "agents": {
    "defaults": {
      "workspace": "/root/.openclaw/workspace-dev",
      "skipBootstrap": true,
      "model": {"primary": "${PROVIDER}/${MODEL}"}
    },
    "list": [{
      "id": "dev",
      "default": true,
      "workspace": "/root/.openclaw/workspace-dev",
      "identity": {"name": "Agent", "emoji": "🤖"},
      "model": {"primary": "${PROVIDER}/${MODEL}"}
    }]
  },
  "gateway": {"mode": "local", "bind": "loopback", "auth": {"mode": "none"}}
}
CONF

# Write auth profiles if API key is set
if [ -n "$API_KEY" ]; then
  cat > /root/.openclaw/agents/dev/agent/auth-profiles.json << AUTH
{
  "version": 1,
  "profiles": {
    "${PROVIDER}:default": {
      "type": "api_key",
      "provider": "${PROVIDER}",
      "key": "${API_KEY}"
    }
  },
  "lastGood": {"${PROVIDER}": "${PROVIDER}:default"}
}
AUTH
fi

# Start OpenClaw
exec openclaw gateway run --workspace /agent
