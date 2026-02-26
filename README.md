# Claw Pen 🦀

> ⚠️ **WORK IN PROGRESS** — Early development. Not ready for production use.

![Status](https://img.shields.io/badge/status-WIP-red) ![Version](https://img.shields.io/badge/version-0.1.0--alpha-orange) ![CI](https://github.com/AchyErrorJ/claw-pen/actions/workflows/ci.yml/badge.svg)

**Easy OpenClaw agent deployment.** Create, configure, and manage isolated AI agents with one command.

## Quick Start

### Install

```bash
# Linux/macOS
curl -fsSL https://raw.githubusercontent.com/AchyErrorJ/claw-pen/main/scripts/install.sh | bash

# Or build from source
git clone https://github.com/AchyErrorJ/claw-pen.git
cd claw-pen/orchestrator && cargo build --release
```

### Create an Agent

```bash
# From template (easiest)
claw-pen create --template coding-assistant --name my-coder

# Custom
claw-pen create --name my-agent --provider openai --model gpt-4o
```

### Templates

Templates are just starting points — override anything at creation:

| Template | Default Provider | Typical Use Case |
|----------|------------------|------------------|
| `coding-assistant` | OpenAI | General coding |
| `code-reviewer` | Anthropic | PR reviews |
| `local-assistant` | Ollama | Private, local |
| `lm-studio` | LM Studio | Local, easy GUI |
| `researcher` | OpenAI | Web research |
| `devops` | OpenAI | Infrastructure |
| `kimi` | Kimi (Moonshot) | Long-context Chinese/English (OAuth) |
| `zai` | z.ai | z.ai powered assistant (OAuth) |

> **OAuth Providers:** Kimi and z.ai use OAuth authentication. Tokens are managed by the OpenClaw Gateway — no API keys needed in container config. Configure once via `openclaw configure`.

> 💡 These are suggestions, not requirements. Use any template with any provider/model:

```bash
# Researcher template, but local
claw-pen create --template researcher --provider ollama --model llama3.2

# Coding assistant with a smaller model
claw-pen create --template coding-assistant --model gpt-4o-mini

# Skip templates entirely, bring your own config
claw-pen create --name custom --provider lmstudio --model "" --memory 4096
```

## Deployment Modes

Claw Pen supports three deployment patterns. Choose based on your setup:

### Mode 1: All Windows (WSL2)

Everything runs on Windows except containers.

```
Windows
├── Tauri App (GUI)
├── Orchestrator (port 3000)
├── AndOR Bridge (port 3456)
├── AndOR Hub
├── OpenClaw Gateway (port 18789)
├── Model Server (Ollama/LM Studio)
└── WSL2
    └── Containment Runtime + Agent Containers
```

Configure:
```bash
DEPLOYMENT_MODE=windows-wsl
RUNTIME_SOCKET=//./pipe/docker_engine
ANDOR_BRIDGE__URL=http://localhost:3456
```

### Mode 2: All Linux

Everything runs on a single Linux machine (VM or bare metal).

```
Linux
├── Tauri App (local or remote via Tailscale)
├── Orchestrator (port 3000)
├── AndOR Bridge (port 3456)
├── AndOR Hub
├── OpenClaw Gateway (port 18789)
├── Model Server (Ollama/LM Studio)
└── Containment Runtime + Agent Containers
```

Configure:
```bash
DEPLOYMENT_MODE=linux-native
RUNTIME_SOCKET=/var/run/docker.sock
ANDOR_BRIDGE__URL=http://localhost:3456
```

### Mode 3: Split (Windows + Linux VM)

Orchestrator and containers on Linux, GUI and bridge on Windows. Connected via Tailscale.

```
Linux VM (Tailnet: linux-agent-host)
├── Orchestrator (port 3000)
├── Containment Runtime + Agent Containers
├── Model Server (optional)
└── OpenClaw Gateway (if agents need it)

Windows (Tailnet: windows-desktop)
├── Tauri App
├── AndOR Bridge (connects to Linux orchestrator)
├── AndOR Hub
└── Model Server (optional, shared with Linux)
```

Configure on Linux:
```bash
DEPLOYMENT_MODE=linux-native
RUNTIME_SOCKET=/var/run/docker.sock
```

Configure AndOR Bridge on Windows:
```bash
ANDOR_BRIDGE__URL=http://linux-agent-host.tailXXXX.ts.net:3456
OPENCLAW_GATEWAY_URL=http://linux-agent-host.tailXXXX.ts.net:18789
```

Configure Orchestrator on Linux to accept remote connections:
```bash
ORCHESTRATOR_BIND=0.0.0.0:3000
```

---

**Quick reference:**

| Mode | Orchestrator | Containers | AndOR Bridge | Best For |
|------|--------------|------------|--------------|----------|
| `windows-wsl` | Windows | WSL2 (Containment) | Windows | Windows dev, simple setup |
| `linux-native` | Linux | Linux (Containment) | Linux | Servers, single machine |
| `split` | Linux VM | Linux VM (Containment) | Windows | Hybrid, Windows GUI + Linux backend |

## Architecture

```
[Tauri Desktop App] ──→ [Orchestrator API] ──→ [Containment Runtime]
        │                     │                      │
        │              ┌──────┴──────┐              │
        │              ↓             ↓              ↓
   Setup wizard    [Agent 1]     [Agent N]    Linux namespaces
   Agent management (Tailscale)   (Tailscale)  cgroups, seccomp
   Settings/config                          │
        │                                   └── WSL2 (on Windows)
        └── [Yew Web UI] ←── Mobile/browser monitoring
```

**Runtime:** Uses [Containment](https://github.com/containment/container) — a lightweight container runtime optimized for AI agents. No Docker required.

### User Flow

1. **Install** → Tauri app with setup wizard
2. **Setup** → Checks Docker, WSL2, Tailscale; pulls images
3. **Manage** → Create agents, configure providers, start/stop
4. **Monitor** → Yew dashboard for on-the-go status (optional)

## Projects

- `runtime/` — Rust container runtime (WIP by Jer)
- `orchestrator/` — Rust API layer, config management, serves Yew UI
- `ui/` — Yew web dashboard (monitoring on the go)
- `tauri-app/` — Desktop app with setup wizard (planned)

For the UI, build the WASM and serve via Tauri on Windows.

## Tech Stack

- **Runtime:** Containment (Linux namespaces, cgroups, seccomp)
- **Orchestrator:** Rust (axum), serves Yew UI
- **Desktop App:** Tauri (setup wizard, full management)
- **Web Dashboard:** Yew (WASM, mobile-friendly monitoring)
- **Networking:** Tailscale mesh

## Goals

- One-click install via Tauri setup wizard
- GUI-first: create, configure, manage agents from desktop app
- Lightweight web dashboard for mobile monitoring
- Isolated agent containers, each with own Tailscale address
- Per-container config: LLM provider, cores, RAM, env vars
- Cross-platform (compile on Linux, run on Windows)

## AndOR Bridge Integration

Claw Pen can automatically register agents with [AndOR Hub](https://github.com/your-repo/andor-bridge) for per-agent DM channels.

Configure in `.env`:
```
ANDOR_BRIDGE__URL=http://localhost:3456
ANDOR_BRIDGE__REGISTER_ON_CREATE=true
```

When enabled:
- Creating an agent → Registers with AndOR Bridge
- Deleting an agent → Unregisters from AndOR Bridge
- Each agent gets its own DM channel via @mention or channel name

All communication stays on your Tailscale network.

## Local Models

For agents using local LLMs (Ollama, llama.cpp, vLLM, LM Studio), run a **shared model server** on the host:

```
[Model Server (GPU)] ← HTTP → [Agent Containers]
     Ollama/:11434
```

Benefits:
- One model in memory, shared by multiple agents
- GPU utilization stays efficient
- Agents using local models need less RAM allocated

### LM Studio

1. Download and install [LM Studio](https://lmstudio.ai/)
2. Load a model in LM Studio
3. Start the local server (default: `http://localhost:1234`)
4. Create an agent:

```bash
curl -X POST http://localhost:3000/api/agents \
  -H "Content-Type: application/json" \
  -d '{"name": "my-local-agent", "template": "lm-studio"}'
```

Configure in `.env`:
```
MODEL_SERVERS__LM_STUDIO__ENDPOINT=http://localhost:1234
```

### Ollama

```bash
ollama serve
ollama pull llama3.2
```

Configure in `.env`:
```
MODEL_SERVERS__OLLAMA__ENDPOINT=http://localhost:11434
MODEL_SERVERS__OLLAMA__DEFAULT_MODEL=llama3.2
```

Cloud providers (OpenAI, Anthropic, etc.) work out of the box — just set API keys per-agent.

## Project Structure

```
claw-pen/
├── orchestrator/     # REST API + Docker runtime
├── ui/               # Yew web dashboard (monitoring)
├── tauri-app/        # Desktop app with setup wizard (planned)
├── runtime/          # Custom container runtime (future)
├── images/           # Pre-built OpenClaw container images
├── scripts/          # Install scripts
└── templates/        # Agent configuration templates
```
