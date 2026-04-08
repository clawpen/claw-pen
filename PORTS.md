# Claw Pen - Port Configuration

This document keeps track of all ports used by Claw Pen to avoid conflicts.

## ⚠️ Known Issues

1. **Config Parsing Bug**: `network_backend = "local"` in config file defaults to `Tailscale`
   - **Workaround**: `unset TAILSCALE_AUTH_KEY` before starting orchestrator
2. **Agent Name Lookup**: Only works for WebSocket connections, not stop/start API calls
   - **Workaround**: Use full agent ID for stop/start operations

## Service Ports

| Service | Port | Purpose | Config Location |
|---------|------|---------|-----------------|
| **Orchestrator** | 3001 | Main API server | `orchestrator/src/main.rs:271` |
| **llama.cpp Server** | 8081 | Local LLM inference | Started manually |
| **Agent Gateway Base** | 18790 | First agent port | Orchestrator assigned |
| **Agent Gateways** | 18790-18999 | Individual agent WebSocket ports | Dynamic assignment |

## Agent Port Assignments

| Agent Name | Gateway Port | Container ID | Status |
|------------|--------------|--------------|--------|
| llama-test | 18796 | 9fff0b62... | running |
| local-cuda | 18797 | 7aeb508c... | running |
| cuda-test | 18798 | 1dc52dab... | running |

## External/Conflicting Ports

| Port | Process | Notes |
|------|---------|-------|
| 3000 | PM2/Node.js | External service - NOT Claw Pen |

## Configuration Files

- **Orchestrator**: `orchestrator/src/main.rs` - hardcoded to `127.0.0.1:3001`
- **Tauri App**: `tauri-app/dist/index.html` - `ORCHESTRATOR_URL = 'http://localhost:3001'`
- **Tauri WebSocket**: `tauri-app/dist/index.html` - `ws://127.0.0.1:3001/api/agents/...`

## Important Notes

1. **Orchestrator must be on 3001** - Tauri app hardcodes this URL
2. **Agents use 18790+** - Each agent gets a unique port for its OpenClaw gateway
3. **llama.cpp uses 8081** - Local inference server
4. **Port 3000 is NOT Claw Pen** - It's a PM2-managed Node.js process (separate service)

## To Start Services

```bash
# 1. Start llama.cpp
cd "F:\Software\Claw Pen\llamacpp"
./llama-server.exe -m "F:\Software\Claw Pen\Llama-3.3-8B-Instruct-Thinking-Claude-Haiku-4.5-High-Reasoning-1700x.Q4_K_M.gguf" --port 8081 --host 0.0.0.0

# 2. Start orchestrator
cd "F:\Software\Claw Pen\claw-pen"
./target/debug/claw-pen-orchestrator.exe

# 3. Start Tauri app (development)
cd "F:\Software\Claw Pen\claw-pen\tauri-app"
npm run tauri dev
```
