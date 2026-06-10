#!/bin/bash
# Exo Agent with Kimi configuration

export KIMI_API_KEY="${KIMI_API_KEY:-${KIMI_PLUGIN_API_KEY:-}}"
export KIMI_CLAW_ID="${KIMI_CLAW_ID:-exo-agent}"
export EXO_AGENT_CONFIG="${EXO_AGENT_CONFIG:-/root/.openclaw/workspace/claw-pen/exo-kimi-config.toml}"

exec /root/.openclaw/workspace/claw-pen/exo-agent "$@"
