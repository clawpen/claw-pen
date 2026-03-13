# Claw Pen

> Multi-agent orchestration platform. Run isolated AI agents in containers with a Tauri desktop UI.

![Status](https://img.shields.io/badge/status-strong%20alpha-orange) ![Version](https://img.shields.io/badge/version-0.2.0-blue) ![Rust](https://img.shields.io/badge/rust-1.70+-93450d?logo=rust)

## What is Claw Pen?

Claw Pen is a **container-based multi-agent orchestrator** with a desktop GUI. It lets you:

- **Create isolated AI agents** — Each runs in its own container with configurable resources
- **Volume management** — Attach external data to agents with read-only or read-write access
- **Chat with agents** — Built-in WebSocket chat interface via Tauri desktop app
- **Manage multiple LLM providers** — OpenAI, Anthropic, Kimi, z.ai, Ollama, LM Studio, and more
- **Organize agents into teams** — Router agents intelligently route messages to specialists
- **Persist agent memory** — SQLite-backed memory with export/import capabilities
- **Secure by default** — Container escape prevention, seccomp filters, JWT auth, secrets management

## Architecture

```
+-------------------------------------------------------------+
|                    Tauri Desktop App                         |
|              (Rust backend + Web frontend)                   |
+----------------------+--------------------------------------+
                       | HTTP / WebSocket
                       v
+-------------------------------------------------------------+
|                  Orchestrator (Rust/Axum)                    |
|  +---------+ +---------+ +---------+ +---------+ +-------+  |
|  | REST API| |WebSocket| |  Auth   | |Secrets  | |Teams  |  |
|  |         | | Gateway | | (JWT)   | |Manager  | |Router |  |
|  +----+----+ +----+----+ +----+----+ +----+----+ +---+----+  |
|       +-------------+----------+-----------+----------+      |
+----------------------+--------------------------------------+
                       |
         +-------------+-------------+
         |             |             |
         v             v             v
   +---------+   +---------+   +----------+
   | Docker  |   |  Exo    |   |Containment|
   |Runtime  |   |Runtime  |   |Runtime   |
   +----+----+   +----+----+   +-----+----+
        |             |              |
        +-------------+--------------+
                      |
                      v
        +-------------------------+
        |    Agent Containers     |
        |  (OpenClaw instances)   |
        +-------------------------+
```

## Features

### Agent Management
- Create agents from templates (coding-assistant, researcher, local-llm, etc.)
- Configure CPU, memory, and provider per agent
- Start/stop/restart agents individually or in batch
- Real-time logs via WebSocket streaming

### Built-in Chat
- WebSocket-based chat interface
- Session persistence per agent
- Typing indicators and real-time responses
- Message history export

### Volume Management
- Attach external directories to agents
- Managed volumes (stored in orchestrator data directory)
- Bind mounts (host paths)
- Read-only or read-write access modes
- Automatic agent restart when volumes change
- Visual security indicators in GUI

### Container Security
- Privileged mode disabled (prevents container escape)
- Seccomp and AppArmor security filters
- Capability dropping (drop ALL, only add NET_BIND_SERVICE)
- Device cgroup restrictions (whitelist only required devices)
- User namespace isolation
- Resource limits (memory, CPU, process count)
- OOM killer enabled (prevents DoS)

### Security
- JWT-based authentication with Argon2 password hashing
- Per-agent secrets (API keys, tokens)
- Ed25519 device identity for Tauri app
- Input validation and sanitization
- CORS protection

### Templates
Pre-configured agent templates:
- `coding-assistant` — OpenAI GPT-4o for coding tasks
- `code-reviewer` — Anthropic Claude for code review
- `researcher` — Web search and summarization
- `local-assistant` — Ollama for local inference
- `openclaw-agent` — Full OpenClaw instance with built-in chat

### Teams and Routing
- Group agents into teams with a router
- Router intelligently classifies and routes messages
- Team chat with automatic routing to specialists

### Persistence
- SQLite-backed agent memory
- Snapshots for backup/restore
- JSON-based configuration storage
- Import/export agents

## Quick Start

### Prerequisites

- **Docker** 20.10+ (or Podman/Containment)
- **Rust** 1.70+
- **Node.js** 18+ (for Tauri app)

### Install

```bash
# Clone the repository
git clone https://github.com/clawpen/claw-pen.git
cd claw-pen

# Build the orchestrator
cargo build --release -p claw-pen-orchestrator

# Build the Tauri desktop app
cd tauri-app
cargo tauri build
cd ..
```

### Run

```bash
# 1. Start the orchestrator (port 3000)
./target/release/claw-pen-orchestrator

# 2. In another terminal, set admin password
./target/release/claw-pen-orchestrator --set-password

# 3. Launch the Tauri desktop app
./tauri-app/src-tauri/target/release/claw-pen-desktop
```

### Create Your First Agent

Via the Tauri app GUI:
1. Login with your admin password
2. Click "Create Agent"
3. Select provider (OpenAI, Anthropic, Ollama, etc.)
4. Enter API key
5. Click Create — your agent is now running!

Or via API:
```bash
curl -X POST http://localhost:3000/api/agents \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "name": "my-coder",
    "template": "coding-assistant",
    "config": {
      "llm_provider": "openai",
      "llm_model": "gpt-4o",
      "memory_mb": 1024,
      "api_key": "sk-..."
    }
  }'
```

## Supported LLM Providers

| Provider | Authentication | Notes |
|----------|---------------|-------|
| OpenAI | API Key | GPT-4o, GPT-4, GPT-3.5 |
| Anthropic | API Key | Claude 3.5 Sonnet, Opus |
| Kimi | API Key | Moonshot AI (OpenClaw gateway) |
| z.ai | API Key | GLM models |
| Ollama | Local | Self-hosted models |
| LM Studio | Local | GUI-based local inference |
| HuggingFace | Token | Various open models |

## Project Structure

```
claw-pen/
├── orchestrator/          # Rust/Axum API server
│   ├── src/
│   │   ├── api.rs         # REST API handlers
│   │   ├── auth.rs        # JWT authentication
│   │   ├── container.rs   # Container runtime (Docker/Exo)
│   │   ├── teams.rs       # Team/router management
│   │   └── types.rs       # Core data structures
│   └── Cargo.toml
├── tauri-app/             # Tauri v2 desktop app
│   ├── src/               # Rust backend
│   ├── dist/              # Web frontend
│   └── Cargo.toml
├── templates/             # Agent templates
│   ├── agents.yaml        # Template definitions
│   └── openclaw-agent/    # Container image
├── runtime/               # Future: custom runtime
├── deploy/                # Deployment configs
├── docs/                  # Documentation
└── scripts/               # Install scripts
```

## Configuration

The orchestrator loads config from (in order):
1. Environment variables
2. `claw-pen.toml` (current directory)
3. `/etc/claw-pen/config.toml`

Example `claw-pen.toml`:
```toml
[server]
host = "0.0.0.0"
port = 3000

[runtime]
type = "docker"  # or "exo"

[network]
backend = "bridge"  # or "tailscale", "headscale"

[andor_bridge]
url = "https://andor.example.com"
```

## Development

```bash
# Run orchestrator in dev mode (with auto-reload)
cargo watch -x "run -p claw-pen-orchestrator"

# Run Tauri app in dev mode
cd tauri-app
cargo tauri dev

# Run tests
cargo test --workspace

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy --workspace --all-targets
```

## API Reference

The orchestrator exposes a REST API:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/auth/login` | POST | Get JWT token |
| `/api/agents` | GET | List all agents |
| `/api/agents` | POST | Create new agent |
| `/api/agents/:id` | GET | Get agent details |
| `/api/agents/:id` | DELETE | Delete agent |
| `/api/agents/:id/start` | POST | Start agent |
| `/api/agents/:id/stop` | POST | Stop agent |
| `/api/agents/:id/chat` | WS | Chat WebSocket |
| `/api/agents/:id/logs` | WS | Log stream |
| `/api/volumes` | GET | List volumes |
| `/api/volumes` | POST | Create volume |
| `/api/agents/:id/volumes` | GET | List agent volumes |
| `/api/agents/:id/volumes/attach` | POST | Attach volume |
| `/api/agents/:id/volumes/detach` | POST | Detach volume |
| `/api/templates` | GET | List templates |
| `/api/teams` | GET | List teams |
| `/api/system/stats` | GET | Resource usage |

Full API docs: [docs/API.md](docs/API.md) (TODO)

## Security

> **🔒 Strong Alpha Status**: The system is functional and includes comprehensive security hardening, but has not yet undergone a professional security audit. Use in production at your own risk.

See [docs/SECURITY_FIXES.md](docs/SECURITY_FIXES.md) for security audit history.

Key security features:
- **Container Escape Prevention**: Privileged mode disabled, seccomp/AppArmor filters, capability dropping
- **Volume Isolation**: Agents cannot access host filesystem outside mounted volumes
- **Argon2id password hashing**: Industry-best password hashing
- **JWT authentication**: Short expiry + refresh tokens
- **Per-agent secret isolation**: Secrets never shared between agents
- **Container network isolation**: Agents isolated in dedicated network
- **Input validation**: All endpoints validate and sanitize input
- **No secrets in environment**: Secrets mounted as files at runtime

### Security Audit Recommendations

Before production deployment, we recommend:
1. **Professional security audit** of container escape prevention
2. **Penetration testing** of volume attachment/detachment flows
3. **Review of seccomp/AppArmor profiles** for completeness
4. **Audit of authentication/authorization** flows
5. **Testing of race conditions** in volume attachment (synchronous operations implemented)
6. **Review of error handling** for information disclosure
7. **Performance testing** under load with multiple agents/volumes

## Roadmap

### Completed ✅
- [x] Volume management and attachment
- [x] Container security hardening (seccomp, AppArmor, capability dropping)
- [x] Visual security indicators in GUI
- [x] Synchronous agent restarts for volume changes
- [x] Agent index management and ID tracking

### In Progress 🚧
- [ ] Custom runtime (Containment)
- [ ] GPU passthrough for local models

### Planned 📋
- [ ] Web UI (Yew/WASM)
- [ ] Multi-node cluster support
- [ ] GitHub/GitLab CI integration
- [ ] Agent marketplace
- [ ] **Professional security audit** (priority for production)

## License

MIT — See [LICENSE](LICENSE)

## Contributing

Contributions welcome! Please read our [Contributing Guide](CONTRIBUTING.md) (TODO).

---

<p align="center">Built with Rust + Tauri</p>
