# Claw Pen 🦀

> Easy OpenClaw agent deployment. Create, configure, and manage isolated AI agents with one command.

![Status](https://img.shields.io/badge/status-WIP-red) ![Version](https://img.shields.io/badge/version-1.0.0--alpha-orange) ![CI](https://github.com/AchyErrorJ/claw-pen/actions/workflows/ci.yml/badge.svg)

## What It Does

Claw Pen lets you run multiple AI agents in isolated containers, each with its own:
- LLM provider and model
- Memory and CPU limits
- Environment variables and secrets
- Network identity (via Tailscale)

Perfect for:
- Running specialized agents for different tasks
- Testing agents with different models side-by-side
- Isolating production agents from experiments
- Building multi-agent teams with smart routing

## Quick Start

### Install

```bash
# Linux/macOS
curl -fsSL https://raw.githubusercontent.com/AchyErrorJ/claw-pen/main/scripts/install.sh | bash

# Or build from source
git clone https://github.com/AchyErrorJ/claw-pen.git
cd claw-pen/orchestrator && cargo build --release
```

### Set Password

```bash
./claw-pen-orchestrator --set-password
```

### Start & Create Agent

```bash
# Start the orchestrator
./claw-pen-orchestrator

# In another terminal, login and create an agent
export TOKEN=$(curl -s -X POST http://localhost:3000/auth/login \
  -H "Content-Type: application/json" \
  -d '{"password": "your-password"}' | jq -r '.access_token')

curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-agent", "template": "coding-assistant"}'
```

📖 **[Full Getting Started Guide →](docs/GETTING-STARTED.md)**

## Templates

Create agents instantly from pre-configured templates:

| Template | Provider | Use Case |
|----------|----------|----------|
| `openclaw-agent` | z.ai | Full OpenClaw with built-in webchat |
| `coding-assistant` | OpenAI | General coding |
| `code-reviewer` | Anthropic | PR reviews |
| `local-assistant` | Ollama | Private, offline |
| `lm-studio` | LM Studio | Local with GUI |
| `researcher` | OpenAI | Web research |
| `tutor-box` | z.ai | Learning companion |

Override anything at creation:

```bash
# Local researcher
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "local-researcher",
    "template": "researcher",
    "config": {
      "llm_provider": "ollama",
      "llm_model": "llama3.2"
    }
  }'
```

## Teams & Routing

Group specialists into a team with automatic message routing:

```toml
# teams/finance.toml
[team]
name = "Finance AI Team"

[router]
mode = "hybrid"

[agents]
receipts = { agent = "finn", description = "Expense receipts" }
payables = { agent = "pax", description = "Bills to pay" }

[routing.receipts]
keywords = ["receipt", "expense", "bought"]
```

One endpoint → routed to the right specialist automatically.

📖 **[Teams Documentation →](docs/TEAMS.md)**

## Architecture

```
┌─────────────────┐
│  Tauri App      │  Desktop GUI with setup wizard
│  (or Web UI)    │
└────────┬────────┘
         │ HTTP/WebSocket
         ▼
┌─────────────────┐
│  Orchestrator   │  Rust API (Axum), JWT auth
│  (port 3000)    │  Template & team management
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Container      │  Docker/Podman/Containment/Exo
│  Runtime        │
└────────┬────────┘
         │
    ┌────┴────┬─────────┐
    ▼         ▼         ▼
┌───────┐ ┌───────┐ ┌───────┐
│Agent A│ │Agent B│ │Agent C│
│(isolated)│(isolated)│(isolated)│
└───────┘ └───────┘ └───────┘
```

Each agent runs in its own container with dedicated resources and networking.

📖 **[Architecture Deep Dive →](docs/ARCHITECTURE.md)**

## Authentication

Claw Pen uses JWT authentication for all API endpoints.

### Setup

```bash
# Set admin password
./claw-pen-orchestrator --set-password
```

### Login

```bash
# Get access token
curl -X POST http://localhost:3000/auth/login \
  -H "Content-Type: application/json" \
  -d '{"password": "your-password"}'

# Returns:
# {
#   "access_token": "eyJ...",
#   "refresh_token": "eyJ...",
#   "expires_in": 86400
# }
```

### Use Token

```bash
# HTTP requests
curl http://localhost:3000/api/agents \
  -H "Authorization: Bearer YOUR_TOKEN"

# WebSocket connections
wscat -c "ws://localhost:3000/api/agents/my-agent/chat?token=YOUR_TOKEN"
```

### Refresh

```bash
curl -X POST http://localhost:3000/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "YOUR_REFRESH_TOKEN"}'
```

## Deployment Modes

| Mode | Orchestrator | Containers | Best For |
|------|--------------|------------|----------|
| `linux-native` | Linux | Linux | Servers, single machine |
| `windows-wsl` | Windows | WSL2 | Windows development |
| `split` | Linux VM | Linux VM | Windows GUI + Linux backend |

Configure in `claw-pen.toml`:
```toml
deployment-mode = "linux-native"
runtime-socket = "/var/run/docker.sock"
```

## Local Models

Run agents with local LLMs - no API keys needed.

### Ollama

```bash
# Install and start
curl https://ollama.ai/install.sh | sh
ollama serve &
ollama pull llama3.2

# Create agent
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "local-agent", "template": "local-assistant"}'
```

### LM Studio

1. Download from lmstudio.ai
2. Load a model and start the server
3. Use the `lm-studio` template

## API Reference

### Authentication

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/auth/login` | POST | Get JWT tokens |
| `/auth/refresh` | POST | Refresh access token |
| `/auth/status` | GET | Check auth config |

### Agents

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/agents` | GET | List agents |
| `/api/agents` | POST | Create agent |
| `/api/agents/:id` | GET/PUT/DELETE | Get/update/delete agent |
| `/api/agents/:id/start` | POST | Start agent |
| `/api/agents/:id/stop` | POST | Stop agent |
| `/api/agents/:id/chat` | WS | Chat with agent |
| `/api/agents/:id/logs` | GET | Get logs |

### Teams

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/teams` | GET | List teams |
| `/api/teams/:id` | GET | Get team config |
| `/api/teams/:id/chat` | WS | Routed team chat |

## Project Structure

```
claw-pen/
├── orchestrator/     # Rust API server
├── templates/        # Agent templates (YAML)
├── teams/            # Team configurations (TOML)
├── agents/           # Example agent setups
├── tauri-app/        # Desktop GUI (Tauri)
├── ui/               # Web dashboard (Yew/WASM)
└── runtime/          # Custom container runtime (future)
```

## Prerequisites

- **Docker** 20.10+ (or Podman 4.0+) OR **Exo** container runtime
- **Rust** 1.70+ (building from source)
- **Node.js** 18+ (Tauri app)
- **4GB RAM** minimum (8GB+ for local models)

## Documentation

- [Getting Started](docs/GETTING-STARTED.md) - Step-by-step guide
- [Architecture](docs/ARCHITECTURE.md) - How it works
- [Templates](docs/TEMPLATES.md) - Template guide
- [Teams](docs/TEAMS.md) - Multi-agent routing
- [Security Fixes](docs/SECURITY_FIXES.md) - Security notes

## Links

- **GitHub:** https://github.com/AchyErrorJ/claw-pen
- **Discord:** https://discord.gg/claw-pen
- **Website:** https://claw-pen.dev

## Status

⚠️ **Work in Progress** - Early development. Not ready for production use.

Contributions welcome! See [GitHub Issues](https://github.com/AchyErrorJ/claw-pen/issues) for roadmap.

## License

MIT
