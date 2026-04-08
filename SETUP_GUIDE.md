# Claw Pen - Local LLM Inference Setup Guide

Complete guide to set up Claw Pen with local Llama-3.3-8B inference using llama.cpp and CUDA acceleration on your RTX 3070.

## Prerequisites

### Hardware
- NVIDIA GPU with 8GB+ VRAM (RTX 3070 or better recommended)
- 16GB+ RAM
- 10GB+ free disk space

### Software
- Windows 11 with WSL2
- Docker Desktop for Windows
- CUDA Toolkit 12.4+
- Rust toolchain

## Quick Start

### 1. Start Services

Run the startup script:
```powershell
cd "F:\Software\Claw Pen"
powershell -ExecutionPolicy Bypass -File start-clawpen.ps1
```

This starts:
- llama.cpp server on port 8081
- Orchestrator on port 3001

### 2. Test Connection

Open in browser:
```
F:\Software\Claw Pen\test-e2e-chat.html
```

## Detailed Setup

### Step 1: Download llama.cpp Binaries

1. Download from: https://github.com/ggml-org/llama.cpp/releases
2. Look for: `llama-cpp-bin-win-cuda-12.4-x64.zip`
3. Extract to: `F:\Software\Claw Pen\llamacpp`

### Step 2: Download Model

Download the model to `F:\Software\Claw Pen\`:
- `Llama-3.3-8B-Instruct-Thinking-Claude-Haiku-4.5-High-Reasoning-1700x.Q4_K_M.gguf`
- Size: ~4.6GB
- Quantization: Q4_K_M (good balance of speed/quality)

### Step 3: Build Docker Images

Build the local agent image (without Tailscale):
```bash
cd "F:\Software\Claw Pen\claw-pen\orchestrator"
docker build -f Dockerfile.no-tailscale -t openclaw-agent:local .
```

### Step 4: Build Orchestrator

```bash
cd "F:\Software\Claw Pen\claw-pen\orchestrator"
cargo build
```

### Step 5: Start llama.cpp Server

```bash
cd "F:\Software\Claw Pen\llamacpp"
./llama-server.exe \
  -m "F:\Software\Claw Pen\Llama-3.3-8B-Instruct-Thinking-Claude-Haiku-4.5-High-Reasoning-1700x.Q4_K_M.gguf" \
  --port 8081 \
  --host 0.0.0.0 \
  --ctx-size 8192 \
  --n-gpu-layers 99
```

### Step 6: Start Orchestrator

**Important:** Unset Tailscale environment variable first:
```bash
unset TAILSCALE_AUTH_KEY
cd "F:\Software\Claw Pen\claw-pen"
./target/debug/claw-pen-orchestrator.exe
```

### Step 7: Create an Agent

Use the creation script:
```bash
cd "F:\Software\Claw Pen"
create-local-agent.bat my-agent 18799
```

Or create via API:
```bash
TOKEN=$(curl -s -X POST http://localhost:3001/auth/login \
  -H "Content-Type: application/json" \
  -d '{"password":"admin123"}' | jq -r '.access_token')

curl -X POST http://localhost:3001/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "local-llm",
    "llm_provider": "openai",
    "llm_model": "gpt-4o",
    "preset": "small",
    "env_vars": {
      "OPENAI_API_KEY": "local",
      "OPENAI_BASE_URL": "http://host.docker.internal:8081/v1",
      "PORT": "18799"
    },
    "image": "openclaw-agent:local"
  }'
```

## Configuration

### Port Assignments

| Service | Port | Purpose |
|---------|------|---------|
| Orchestrator API | 3001 | Main API server |
| llama.cpp | 8081 | Local LLM inference |
| Agent Gateways | 18790+ | Individual agent WebSocket ports |

### Agent Configuration

Key environment variables for local inference:
- `OPENAI_API_KEY=local` - Dummy key for local inference
- `OPENAI_BASE_URL=http://host.docker.internal:8081/v1` - Points to llama.cpp
- `PORT=18790` - Unique port per agent (increment for new agents)

### Resource Presets

| Preset | RAM | CPU | Use Case |
|--------|-----|-----|----------|
| Nano | 512MB | 0.5 | Minimal agents |
| Micro | 1GB | 1 | Simple tasks |
| Small | 2GB | 2 | Standard agents |
| Medium | 4GB | 4 | Local LLM inference |
| Large | 8GB | 8 | Heavy computation |

## Known Issues

### 1. Config Parsing Bug

**Issue:** The `network_backend = "local"` setting in `claw-pen.toml` is ignored and defaults to `Tailscale`.

**Workaround:** Unset the `TAILSCALE_AUTH_KEY` environment variable before starting:
```bash
unset TAILSCALE_AUTH_KEY
```

### 2. Agent Name Lookup in Stop/Start

**Issue:** Agent stop/start endpoints don't recognize agent names, only IDs.

**Workaround:** Use the full agent ID from `/api/agents` for stop/start operations.

### 3. Memory Limits in Agent Creation

**Issue:** Creating agents with explicit `memory_mb` fails validation.

**Workaround:** Use presets instead of explicit memory values:
```json
{
  "preset": "small",
  ...
}
```

## Troubleshooting

### Orchestrator Won't Start

**Error:** "The system cannot find the file specified"

**Solution:** Unset Tailscale environment:
```bash
unset TAILSCALE_AUTH_KEY
```

### Agent Can't Connect to llama.cpp

**Error:** "Failed to connect to agent"

**Solution:** 
1. Check llama.cpp is running: `curl http://127.0.0.1:8081/health`
2. Verify `OPENAI_BASE_URL` includes `/v1` suffix
3. Check `host.docker.internal` resolves (Docker Desktop feature)

### WebSocket Returns 404

**Error:** "Unexpected response code: 404"

**Solution:** Use full agent ID instead of name in WebSocket URL.

## Performance Optimization

See `PERFORMANCE.md` for GPU tuning tips.

## Directory Structure

```
F:\Software\Claw Pen\
├── llamacpp/                    # llama.cpp binaries
├── Llama-3.3-8B-*.gguf         # Model file
├── claw-pen/
│   ├── orchestrator/            # Orchestrator code
│   │   ├── target/debug/        # Built binaries
│   │   └── claw-pen.toml        # Config file
│   ├── templates/               # Agent templates
│   │   ├── local-inference-agent.json
│   │   └── roles/
│   │       └── local-llm/
│   │           └── HEARTBEAT.md
│   └── tauri-app/               # Desktop app
├── start-clawpen.ps1            # Startup script
├── stop-clawpen.bat             # Stop script
├── test-e2e-chat.html           # E2E test page
└── logs/                        # Service logs
```

## Next Steps

1. Create agents for different tasks
2. Set up volume mounts for workspace access
3. Configure role-based prompts
4. Set up teams and role assignments

## Support

- Logs: `F:\Software\Claw Pen\logs\`
- Status check: Run `status-clawpen.bat`
- Port conflicts: Check `PORTS.md`
