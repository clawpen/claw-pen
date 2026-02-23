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

| Mode | Host | Containers | Best For |
|------|------|------------|----------|
| `windows-wsl` | Windows | WSL2 Linux | Development, Windows machines |
| `linux-native` | Linux | Native Linux | Production, Linux servers |
| `all-windows` | Windows | Windows containers | Windows-only environments |

Configure in `.env`:
```
DEPLOYMENT_MODE=windows-wsl
```

## Architecture

```
[Tauri Desktop App] ──→ [Orchestrator API] ──→ [Container Runtime]
        │                     │
        │              ┌──────┴──────┐
        │              ↓             ↓
   Setup wizard    [Agent 1]     [Agent N]
   Agent management (Tailscale)   (Tailscale)
   Settings/config
        │
        └── [Yew Web UI] ←── Mobile/browser monitoring
```

### User Flow

1. **Install** → Tauri app with setup wizard
2. **Setup** → Checks Docker, WSL2, Tailscale; pulls images
3. **Manage** → Create agents, configure providers, start/stop
4. **Monitor** → Yew dashboard for on-the-go status (optional)

### Windows + WSL2 Setup

```
Windows Host
├── WSL2 Distro (Ubuntu/Debian)
│   └── Agent Containers (Linux)
│       └── Each with Tailscale IP
├── Claw Pen Orchestrator (Windows .exe)
├── Claw Pen UI (Tauri app)
└── Ollama (Windows or WSL2)
```

## Projects

- `runtime/` — Rust container runtime (WIP by Jer)
- `orchestrator/` — Rust API layer, config management, serves Yew UI
- `ui/` — Yew web dashboard (monitoring on the go)
- `tauri-app/` — Desktop app with setup wizard (planned)

## Cross-Compilation (Linux → Windows)

From this Linux VM, compile for your Windows host:

```bash
# Install mingw-w64 target
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64

# Build orchestrator for Windows
cd orchestrator
cargo build --release --target x86_64-pc-windows-gnu

# Output: target/x86_64-pc-windows-gnu/release/claw-pen-orchestrator.exe
```

For the UI, build the WASM and serve via Tauri on Windows.

## Tech Stack

- **Runtime:** Rust (Docker via bollard, custom later)
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
