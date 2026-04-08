#!/bin/bash

# Test the full orchestrator → agent → Zai flow with password authentication

echo "=== Testing Orchestrator → Agent → Zai Flow ==="
echo ""
echo "Agent Info:"
echo "  Name: OpenZ"
echo "  Container ID: 347319e51090c3e4b11e7de607331a4537c1a936adafbf071f500299be457e2a"
echo "  Port: 18790"
echo "  Binding: lan (0.0.0.0:18790)"
echo "  Auth: password (clawpen)"
echo "  LLM: Zai glm-5"
echo ""

echo "Step 1: Check agent is running..."
docker ps | grep OpenZ && echo "  ✓ Agent is running" || echo "  ✗ Agent not running"

echo ""
echo "Step 2: Check agent logs for auth mode..."
docker logs OpenZ 2>&1 | grep "Using password" | head -1

echo ""
echo "Step 3: Test direct WebSocket connection (should receive challenge)..."
timeout 3 curl -s -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  http://127.0.0.1:18790/chat 2>&1 | grep -E "101|challenge" | head -2

echo ""
echo "Step 4: Check orchestrator health..."
curl -s "http://127.0.0.1:8081/health" && echo "  ✓ Orchestrator is healthy"

echo ""
echo "Step 5: Monitor orchestrator logs (watch for authentication attempts)..."
echo "  Run: tail -f orchestrator-debug.log"
echo ""
echo "=== Next Steps ==="
echo "1. Open GUI at http://127.0.0.1:5174"
echo "2. Send a message to OpenZ agent"
echo "3. Watch orchestrator logs for:"
echo "   - 'Received authentication challenge, responding...'"
echo "   - 'Authentication response sent successfully'"
echo "4. Verify message reaches Zai and gets a response"
