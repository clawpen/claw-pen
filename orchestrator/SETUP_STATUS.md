# Claw Pen Orchestrator → Agent → Zai Flow - Verification Status

## ✅ Working Components

### 1. OpenZ Agent Container
- **Status**: Running (healthy)
- **Image**: openclaw-agent:latest
- **Container ID**: e7cfea2f44353370605c15f2ea3d7ac11c7e4395fb05092e940e0cba815e9fc5
- **Port Mapping**: 127.0.0.1:18790 → 18790
- **Gateway Binding**: ws://0.0.0.0:18790 (lan mode)
- **Authentication**: Token mode (password: "clawpen")

### 2. LLM Configuration
- **Provider**: Zai
- **Model**: glm-5
- **API Key**: Configured
- **Base URL**: https://api.z.ai/api/coding/paas/v4

### 3. Direct Agent Connection Test
```bash
# HTTP endpoint
curl -I http://127.0.0.1:18790/
# Result: HTTP/1.1 200 OK ✓

# WebSocket handshake
curl -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  -H "X-OpenClaw-Gateway-Token: clawpen" \
  http://127.0.0.1:18790/chat
# Result: HTTP/1.1 101 Switching Protocols ✓
```

## ⚠️ Known Issue

### Orchestrator → Agent Connection
**Problem**: The orchestrator uses `tokio_tungstenite::connect_async_with_config()` which doesn't support custom HTTP headers required for token authentication.

**Current Flow**:
1. Orchestrator receives WebSocket connection from GUI
2. Orchestrator tries to connect to agent at `ws://127.0.0.1:18790/chat`
3. Connection fails because orchestrator can't send `X-OpenClaw-Gateway-Token: clawpen` header

**Error Expected**: WebSocket handshake failure or authentication challenge that orchestrator can't complete

## 🔧 Potential Solutions

### Option 1: Use Password Authentication (Recommended)
OpenClaw supports password-based authentication via challenge-response that doesn't require HTTP headers. However, the current entrypoint script only checks for `GATEWAY_TOKEN` variable.

**To implement**: Modify entrypoint script to support `GATEWAY_PASSWORD` for password auth mode.

### Option 2: Modify Orchestrator WebSocket Client
Update orchestrator to use `connect_async_with_request()` instead of `connect_async_with_config()` to support custom headers.

**File to modify**: `orchestrator/src/api.rs` line 1372

### Option 3: Disable Authentication (Local Development Only)
Set auth mode to "none" but this is **not secure** for production.

## 📋 Environment Variables Used
- `BIND=lan` - Binds gateway to 0.0.0.0 (required for Docker port mapping)
- `GATEWAY_TOKEN=clawpen` - Currently used for token auth
- `ZAI_API_KEY` - Zai API key
- `LLM_PROVIDER=Zai` - LLM provider
- `LLM_MODEL=glm-5` - Model name

## 🧪 Quick Test Commands

```bash
# Check agent status
docker ps | grep OpenZ

# Check agent logs
docker logs OpenZ | tail -20

# Test WebSocket connection
curl -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  -H "X-OpenClaw-Gateway-Token: clawpen" \
  http://127.0.0.1:18790/chat

# Check orchestrator logs
tail -f orchestrator.log
```

## ✅ Conclusion

**Direct Agent Connection**: WORKING ✓
- Agent binds correctly to 0.0.0.0:18790
- Token authentication works with header
- WebSocket handshake succeeds

**Orchestrator → Agent Connection**: NEEDS FIX ⚠️
- Orchestrator can't send auth headers
- Need to implement password auth or update orchestrator WebSocket client

**Next Steps**:
1. Implement password authentication support
2. Or modify orchestrator to send token via request headers
3. Test full GUI → Orchestrator → Agent → Zai flow
