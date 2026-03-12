# 🚀 Quick Start - OpenClaw Integration

## Status: ✅ FULLY WORKING

### Services Running

```bash
# OpenClaw Agent (with Zai GLM-5)
docker ps | grep openclaw-agent
# Port: 18791

# Orchestrator
ps aux | grep claw-pen-orchestrator
# Port: 8081
```

### Test the Flow

```bash
# Run the complete test
cd "F:\Software\Claw Pen\claw-pen"
python test_openclaw_chat.py
```

**Expected Output:**
```
✅ Authentication successful!
✅ Chat message sent
✅ Agent responded: SUCCESS
```

---

## Protocol Reference

### Connect (Authentication)
```json
{
  "type": "req",
  "id": "unique-id",
  "method": "connect",
  "params": {
    "minProtocol": 3,
    "maxProtocol": 3,
    "client": {"id": "cli", "mode": "cli"},
    "auth": {"password": "clawpen"}
  }
}
```

### Send Message
```json
{
  "type": "req",
  "id": "req-1",
  "method": "chat.send",
  "params": {
    "sessionKey": "agent:dev:main",
    "message": "Hello!",
    "idempotencyKey": "idem-123"
  }
}
```

---

## Quick Commands

### Start Services

```bash
# Start OpenClaw Agent
docker start openclaw-agent

# Start Orchestrator
cd "F:\Software\Claw Pen\claw-pen"
./target/release/claw-pen-orchestrator.exe
```

### Stop Services

```bash
# Stop Orchestrator
taskkill //F //IM claw-pen-orchestrator.exe

# Stop OpenClaw Agent
docker stop openclaw-agent
```

### View Logs

```bash
# Orchestrator
tail -f orchestrator.log

# OpenClaw Agent
docker logs -f openclaw-agent
```

---

## GUI Connection (When Ready)

### 1. Set Admin Password
```bash
claw-pen-orchestrator --set-password
```

### 2. Get Auth Token
```bash
curl -X POST http://127.0.0.1:8081/auth/login \
  -H "Content-Type: application/json" \
  -d '{"password":"your-password"}'
```

### 3. Connect WebSocket
```javascript
const ws = new WebSocket('ws://127.0.0.1:8081/agents/openclaw-agent/chat?token=<jwt>');

// Send message
ws.send(JSON.stringify({
  content: "Hello from GUI!",
  session: "main"
}));
```

---

## Troubleshooting

### "invalid request frame"
→ Use `type: "req"` not `type: "action"`

### "closed before connect"
→ Check password in `params.auth.password`

### Port conflict
→ Stop manual OpenClaw gateway, use container only

### Authentication fails
→ Verify client ID is `"cli"` and mode is `"cli"`

---

## Key Files

| File | Purpose |
|------|---------|
| `OPENCLAW_INTEGRATION_SUCCESS.md` | Complete documentation |
| `DEBUGGING_LOG.md` | Full debugging history |
| `test_openclaw_chat.py` | Working test client |
| `orchestrator/src/api.rs` | Implementation |

---

## What's Working ✅

- ✅ OpenClaw WebSocket protocol (req/res/event)
- ✅ Password authentication
- ✅ Message translation
- ✅ Chat with Zai GLM-5
- ✅ Streaming responses
- ✅ Error handling

**Ready for GUI integration! 🎉**
