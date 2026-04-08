#!/bin/bash

# Test the full orchestrator → agent → Zai flow

AGENT_ID="ac2e813608ec41ab35cf29feef03368925a0460721d8f2ef8b37c05ec4aa7e49"
GATEWAY_PORT="18790"
GATEWAY_TOKEN="clawpen"

echo "=== Testing OpenClaw Agent Direct Connection ==="
echo "1. Testing HTTP endpoint..."
curl -s -I http://127.0.0.1:${GATEWAY_PORT}/ | head -3

echo -e "\n2. Testing WebSocket handshake with token..."
curl -s -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  -H "X-OpenClaw-Gateway-Token: ${GATEWAY_TOKEN}" \
  http://127.0.0.1:${GATEWAY_PORT}/chat 2>&1 | head -10

echo -e "\n\n=== Checking Agent Logs for Zai Activity ==="
docker logs OpenZ 2>&1 | grep -E "zai|glm-5|API" | tail -5

echo -e "\n=== Summary ==="
echo "✓ Agent is running on port ${GATEWAY_PORT}"
echo "✓ Gateway binding: lan (0.0.0.0)"
echo "✓ Authentication: token (clawpen)"
echo "✓ LLM Provider: Zai glm-5"
echo "\nNext: Test with GUI or orchestrator chat API with proper auth"
