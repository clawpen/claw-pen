# Claw Pen 🦀

> ⚠️ **WORK IN PROGRESS** — Early development. Not ready for production use.

![Status](https://img.shields.io/badge/status-WIP-red) ![Version](https://img.shields.io/badge/version-0.1.0--alpha-orange)

Container orchestration for OpenClaw agents. Rust runtime + orchestrator + Yew/Tauri UI.

## Quick Start

```bash
# Clone
git clone https://github.com/yourname/claw-pen.git
cd claw-pen

# Configure
cp orchestrator/.env.example orchestrator/.env
# Edit .env with your settings

# Build orchestrator
cd orchestrator
cargo build --release

# Run (requires Docker)
./target/release/claw-pen-orchestrator

# Build UI (requires trunk)
cd ../ui
trunk build
```

## Requirements

- Docker (for container runtime)
- Rust 1.75+
- `trunk` for UI builds: `cargo install trunk`

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
[Claw Pen UI (Tailscale)] ──→ [Orchestrator API] ──→ [Container Runtime]
                                      │
                        ┌─────────────┼─────────────┐
                        ↓             ↓             ↓
                    [Agent 1]     [Agent 2]     [Agent N]
                   (Tailscale)   (Tailscale)   (Tailscale)
```

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
- `orchestrator/` — Rust API layer, config management
- `ui/` — Yew frontend (Tauri-compatible)

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

- **Runtime:** Rust (Docker via bollard, swap for custom later)
- **Orchestrator:** Rust (axum)
- **UI:** Yew (WebAssembly) + Tauri
- **Networking:** Tailscale mesh

## Goals

- Isolated agent containers, each with own Tailscale address
- Per-container config: LLM provider, cores, RAM, env vars
- Web dashboard to view/manage all agents
- Cross-platform (compile on Linux, run on Windows)

## Local Models

For agents using local LLMs (Ollama, llama.cpp, vLLM), run a **shared model server** on the host:

```
[Model Server (GPU)] ← HTTP → [Agent Containers]
     Ollama/:11434
```

Benefits:
- One model in memory, shared by multiple agents
- GPU utilization stays efficient
- Agents using local models need less RAM allocated

Configure in `.env`:
```
MODEL_SERVERS__OLLAMA__ENDPOINT=http://localhost:11434
MODEL_SERVERS__OLLAMA__DEFAULT_MODEL=llama3.2
```

Cloud providers (OpenAI, Anthropic, etc.) work out of the box — just set API keys per-agent.
