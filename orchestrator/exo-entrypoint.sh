#!/bin/sh
# Universal Claw Pen agent entrypoint for the Exo runtime.
# Supports both exo-agent (bare-bones Rust agent) and OpenClaw (legacy).
#
# Phase 1 of the exo integration. Phase 2 publishes a real image to a
# sovereign registry (Boréal-hosted) and this script collapses back into
# the Dockerfile.

set -e

MODEL="${LLM_MODEL:-glm-5}"
PROVIDER="${LLM_PROVIDER:-zai}"
PROVIDER=$(echo "$PROVIDER" | tr '[:upper:]' '[:lower:]')
PORT="${PORT:-18790}"

echo "[entrypoint] Provider: $PROVIDER, Model: $MODEL, Port: $PORT"

# --- Check for exo-agent (new bare-bones Rust agent) ---
EXO_AGENT="/clawpen/exo-agent"
if [ -f "$EXO_AGENT" ]; then
    echo "[entrypoint] Using exo-agent (bare-bones Rust agent)"
    
    # Resolve API key per provider
    API_KEY=""
    case "$PROVIDER" in
      zai) API_KEY="$ZAI_API_KEY" ;;
      kimi|kimi-code|kimicode) API_KEY="${KIMI_API_KEY:-$KIMICODE_API_KEY}" ;;
      anthropic) API_KEY="$ANTHROPIC_API_KEY" ;;
      openai) API_KEY="$OPENAI_API_KEY" ;;
      google|gemini) API_KEY="$GOOGLE_API_KEY" ;;
      access) API_KEY="$ACCESS_API_KEY" ;;
      huggingface|hf) API_KEY="$HF_TOKEN" ;;
      *) API_KEY="${API_KEY:-${ZAI_API_KEY:-${ANTHROPIC_API_KEY:-${OPENAI_API_KEY:-}}}}" ;;
    esac
    
    if [ -z "$API_KEY" ]; then
        echo "[entrypoint] ERROR: No API key found for provider '$PROVIDER'"
        exit 1
    fi
    
    # Create a workspace directory for the agent
    mkdir -p /agent/workspace
    
    # Write a simple config file for exo-agent
    cat > /agent/config.json << CONF
{
    "provider": "$PROVIDER",
    "model": "$MODEL",
    "api_key": "$API_KEY",
    "port": $PORT,
    "workspace": "/agent/workspace"
}
CONF
    
    echo "[entrypoint] Starting exo-agent on port $PORT..."
    exec "$EXO_AGENT" --config /agent/config.json
fi

# --- Fall back to OpenClaw (legacy) ---
echo "[entrypoint] exo-agent not found, falling back to OpenClaw"

OPENCLAW_ENTRY="/clawpen-runtime/node_modules/openclaw/openclaw.mjs"
if [ ! -f "$OPENCLAW_ENTRY" ]; then
  echo "[entrypoint] FATAL: openclaw not found at $OPENCLAW_ENTRY"
  echo "  Bootstrap on the host: sudo mkdir -p /opt/clawpen-runtime &&"
  echo "    cd /opt/clawpen-runtime && npm init -y && npm install openclaw@latest"
  ls -la /clawpen-runtime/ 2>&1 | head -10
  exit 1
fi
echo "[entrypoint] Using openclaw from host bind-mount"

GATEWAY_PASSWORD="${GATEWAY_PASSWORD:-}"
GATEWAY_TOKEN="${GATEWAY_TOKEN:-}"
ORCHESTRATOR_DEVICE_ID="${ORCHESTRATOR_DEVICE_ID:-}"
ORCHESTRATOR_PUBLIC_KEY="${ORCHESTRATOR_PUBLIC_KEY:-}"

# Force the gateway to bind on PORT — newer openclaw reads OPENCLAW_GATEWAY_PORT
# and falls back to a default (18789) ignoring the config file's gateway.port.
export OPENCLAW_GATEWAY_PORT="$PORT"

# --- Resolve API key and base URL per provider ---
BASE_URL=""
case "$PROVIDER" in
  zai)
    API_KEY="$ZAI_API_KEY"
    BASE_URL="https://api.z.ai/api/coding/paas/v4"
    ;;
  kimi|kimi-code|kimicode)
    API_KEY="${KIMI_API_KEY:-$KIMICODE_API_KEY}"
    PROVIDER="kimi"
    ;;
  moonshot)
    API_KEY="${MOONSHOT_API_KEY:-$KIMI_API_KEY}"
    ;;
  anthropic)
    API_KEY="$ANTHROPIC_API_KEY"
    ;;
  openai)
    API_KEY="$OPENAI_API_KEY"
    BASE_URL="${OPENAI_BASE_URL:-}"
    ;;
  google|gemini)
    API_KEY="$GOOGLE_API_KEY"
    PROVIDER="google"
    ;;
  deepseek)
    API_KEY="$DEEPSEEK_API_KEY"
    ;;
  groq)
    API_KEY="$GROQ_API_KEY"
    ;;
  mistral)
    API_KEY="$MISTRAL_API_KEY"
    ;;
  ollama)
    API_KEY=""
    BASE_URL="${OLLAMA_BASE_URL:-http://host.docker.internal:11434}"
    ;;
  lmstudio)
    API_KEY=""
    BASE_URL="${LMSTUDIO_BASE_URL:-http://host.docker.internal:1234/v1}"
    PROVIDER="openai"
    ;;
  *)
    API_KEY="${API_KEY:-${ZAI_API_KEY:-${ANTHROPIC_API_KEY:-${OPENAI_API_KEY:-}}}}"
    ;;
esac

MODEL_ID="${PROVIDER}/${MODEL}"
echo "[entrypoint] Model ID: $MODEL_ID"
echo "[entrypoint] API_KEY: ${API_KEY:+set (${#API_KEY} chars)}"
echo "[entrypoint] BASE_URL: ${BASE_URL:-default}"
echo "[entrypoint] GATEWAY_TOKEN: ${GATEWAY_TOKEN:+set}"

# --- Gateway auth ---
if [ -n "$GATEWAY_TOKEN" ]; then
  GATEWAY_AUTH="{\"mode\": \"token\", \"token\": \"${GATEWAY_TOKEN}\"}"
elif [ -n "$GATEWAY_PASSWORD" ]; then
  GATEWAY_AUTH="{\"mode\": \"password\", \"password\": \"${GATEWAY_PASSWORD}\"}"
else
  GATEWAY_AUTH="{\"mode\": \"none\"}"
fi

if [ -n "$GATEWAY_TOKEN" ] || [ -n "$GATEWAY_PASSWORD" ]; then
  BIND_MODE="lan"
else
  BIND_MODE="loopback"
fi

# --- Write OpenClaw config ---
rm -f /root/.openclaw/openclaw.json /root/.openclaw/openclaw.json.bak
mkdir -p /root/.openclaw/agents/dev/agent

cat > /root/.openclaw/openclaw.json << CONF
{
  "meta": {"lastTouchedVersion": "2026.4.14"},
  "agents": {
    "defaults": {
      "workspace": "/root/.openclaw/workspace-dev",
      "skipBootstrap": true,
      "model": {"primary": "${MODEL_ID}"},
      "llm": {"idleTimeoutSeconds": 0},
      "timeoutSeconds": 600
    },
    "list": [{
      "id": "dev",
      "default": true,
      "workspace": "/root/.openclaw/workspace-dev",
      "identity": {"name": "Agent", "emoji": "🤖"},
      "model": {"primary": "${MODEL_ID}"}
    }]
  },
  "gateway": {
    "mode": "local",
    "bind": "${BIND_MODE}",
    "auth": ${GATEWAY_AUTH},
    "port": ${PORT},
    "controlUi": {"allowedOrigins": ["*"]}
  }
}
CONF

# --- Write auth profiles ---
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
  echo "[entrypoint] Auth profile configured for provider: $PROVIDER"
else
  echo "[entrypoint] WARNING: No API key found for provider '$PROVIDER'"
fi

# --- Identity volume (teacher-authored system prompt) ---
if [ -f /identity/system_prompt.md ]; then
  CHARS=$(wc -c < /identity/system_prompt.md)
  echo "[entrypoint] Identity volume present (${CHARS} chars) — injection deferred to Phase 1.5"
else
  echo "[entrypoint] No /identity/system_prompt.md found"
fi

# --- Pre-seed device pairing for orchestrator ---
if [ -n "$ORCHESTRATOR_DEVICE_ID" ] && [ -n "$ORCHESTRATOR_PUBLIC_KEY" ]; then
  mkdir -p /root/.openclaw/devices
  NOW_MS=$(date +%s)000
  DEVICE_TOKEN="${ORCHESTRATOR_DEVICE_TOKEN:-$(head -c 32 /dev/urandom | base64 | tr '+/' '-_' | tr -d '=')}"
  cat > /root/.openclaw/devices/paired.json << PAIRED
{
  "${ORCHESTRATOR_DEVICE_ID}": {
    "deviceId": "${ORCHESTRATOR_DEVICE_ID}",
    "publicKey": "${ORCHESTRATOR_PUBLIC_KEY}",
    "displayName": "Claw Pen Orchestrator",
    "platform": "rust",
    "deviceFamily": "Desktop",
    "clientId": "cli",
    "clientMode": "cli",
    "role": "operator",
    "roles": ["operator"],
    "scopes": ["operator.admin", "operator.read", "operator.write", "operator.approvals", "operator.pairing"],
    "approvedScopes": ["operator.admin", "operator.read", "operator.write", "operator.approvals", "operator.pairing"],
    "tokens": {
      "operator": {
        "token": "${DEVICE_TOKEN}",
        "role": "operator",
        "scopes": ["operator.admin", "operator.read", "operator.write", "operator.approvals", "operator.pairing"],
        "createdAtMs": ${NOW_MS}
      }
    },
    "createdAtMs": ${NOW_MS},
    "approvedAtMs": ${NOW_MS}
  }
}
PAIRED
  echo "[entrypoint] Orchestrator device pre-paired: ${ORCHESTRATOR_DEVICE_ID}"
else
  echo "[entrypoint] No ORCHESTRATOR_DEVICE_ID set, skipping device pre-pairing"
fi

echo "[entrypoint] Starting OpenClaw gateway (bind: ${BIND_MODE})..."
exec node "$OPENCLAW_ENTRY" gateway run --dev --allow-unconfigured
