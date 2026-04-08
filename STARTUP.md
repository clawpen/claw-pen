# Claw Pen - Service Management Scripts

## Quick Start

Double-click **`start-clawpen.bat`** or run:
```powershell
powershell -ExecutionPolicy Bypass -File start-clawpen.ps1
```

## Available Scripts

| Script | Purpose |
|--------|---------|
| **start-clawpen.bat** | Start all services (Batch) |
| **start-clawpen.ps1** | Start all services (PowerShell - recommended) |
| **stop-clawpen.bat** | Stop all Claw Pen services |
| **status-clawpen.bat** | Quick status check |

## What These Scripts Do

### Start Script (`start-clawpen.ps1`)

1. **Checks llama.cpp server** (port 8081)
   - If not running: starts it with your model
   - If running: reports PID

2. **Checks Orchestrator** (port 3001)
   - If not running: starts it
   - If running: reports PID

3. **Health check** - Verifies Orchestrator API responds

4. **Lists running agents** with their ports

### Stop Script (`stop-clawpen.bat`)

- Gracefully stops Orchestrator
- Gracefully stops llama.cpp

## Service URLs

After starting services:

| Service | URL |
|---------|-----|
| Orchestrator API | http://127.0.0.1:3001 |
| llama.cpp API | http://127.0.0.1:8081 |
| WebSocket Test | `F:\Software\Claw Pen\test-websocket.html` |

## Logs

Logs are stored in `F:\Software\Claw Pen\logs\`:
- `llamacpp.log` - llama.cpp server output
- `orchestrator.log` - Orchestrator output

## Troubleshooting

**Services won't start?**
1. Check logs in `F:\Software\Claw Pen\logs\`
2. Run `status-clawpen.bat` to see what's running
3. Check if ports are already in use by other apps

**WebSocket connection fails?**
1. Make sure Orchestrator is running (port 3001)
2. Make sure agent is running
3. Open `test-websocket.html` in browser to test

## Configuration

Edit these variables in `start-clawpen.ps1` if paths change:
```powershell
$LLAMACPP_PORT = 8081
$ORCHESTRATOR_PORT = 3001
$LLAMACPP_DIR = "F:\Software\Claw Pen\llamacpp"
$ORCHESTRATOR_EXE = "F:\Software\Claw Pen\claw-pen\target\debug\claw-pen-orchestrator.exe"
$MODEL_FILE = "F:\Software\Claw Pen\Llama-3.3-8B-Instruct-Thinking-Claude-Haiku-4.5-High-Reasoning-1700x.Q4_K_M.gguf"
```
