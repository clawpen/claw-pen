# Claw Pen Architecture

Understanding how the pieces fit together.

## High-Level Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                           User Interface                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │
│  │   Tauri App     │  │    Web UI       │  │   CLI / API Client  │  │
│  │  (Desktop GUI)  │  │   (Yew/WASM)    │  │   (curl, scripts)   │  │
│  └────────┬────────┘  └────────┬────────┘  └──────────┬──────────┘  │
└───────────┼────────────────────┼──────────────────────┼─────────────┘
            │                    │                      │
            └────────────────────┼──────────────────────┘
                                 │ HTTP/WebSocket
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        Orchestrator (Rust/Axum)                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │
│  │  REST API   │  │  WebSocket  │  │    Auth     │  │  Templates │  │
│  │  Handlers   │  │   Handler   │  │  (JWT)      │  │  Registry  │  │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬─────┘  │
│         │                │                │                │        │
│  ┌──────┴──────┐  ┌──────┴──────┐  ┌──────┴──────┐         │        │
│  │   Storage   │  │   Secrets   │  │  Snapshots  │         │        │
│  │   (JSON)    │  │  Manager    │  │   Manager   │         │        │
│  └─────────────┘  └─────────────┘  └─────────────┘         │        │
└─────────────────────────────────────────────────────────────┼────────┘
                                                              │
            ┌─────────────────────────────────────────────────┤
            │                                                 │
            ▼                                                 ▼
┌───────────────────────┐                        ┌────────────────────┐
│   Container Runtime   │                        │   Team Registry    │
│  (Docker/Podman/      │                        │   (TOML configs)   │
│   Containment)        │                        └────────────────────┘
└───────────┬───────────┘
            │
            ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        Agent Containers                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │
│  │  Agent A    │  │  Agent B    │  │  Agent C    │  │  Router    │  │
│  │ (isolated)  │  │ (isolated)  │  │ (isolated)  │  │  Agent     │  │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬─────┘  │
└─────────┼────────────────┼────────────────┼────────────────┼────────┘
          │                │                │                │
          └────────────────┴────────────────┴────────────────┘
                                    │
                                    ▼
                        ┌────────────────────┐
                        │  Model Servers     │
                        │  (OpenAI/Anthropic │
                        │   /Ollama/etc)     │
                        └────────────────────┘
```

## Core Components

### 1. Orchestrator (`/orchestrator`)

The central API server that manages everything. Built with Rust and Axum.

**Key responsibilities:**
- Agent lifecycle (create, start, stop, delete)
- Configuration management
- Authentication & authorization (JWT)
- WebSocket chat routing
- Secret and snapshot management

**Main modules:**

| Module | Purpose |
|--------|---------|
| `api.rs` | REST endpoints and WebSocket handlers |
| `auth.rs` | JWT authentication, password hashing (Argon2) |
| `container.rs` | Docker/Podman runtime client |
| `containment.rs` | Custom Containment runtime support |
| `templates.rs` | Template loading and registry |
| `teams.rs` | Team configuration and routing |
| `secrets.rs` | Secure secret storage |
| `snapshots.rs` | Agent state snapshots |
| `storage.rs` | Persistent JSON storage |

### 2. Container Runtime

Claw Pen supports multiple container backends:

**Docker/Podman (via Bollard)**
- Standard container management
- Works with Docker 20.10+ or Podman 4.0+
- Uses Docker socket (`/var/run/docker.sock`)

**Containment (Future)**
- Lightweight custom runtime
- Linux namespaces, cgroups, seccomp
- No Docker daemon required

### 3. Agent Containers

Each agent runs in an isolated container with:

- **Dedicated filesystem** - Own `/data` and `/workspace`
- **Resource limits** - Configurable memory and CPU
- **Network isolation** - Optional Tailscale mesh networking
- **Environment** - Provider API keys, custom env vars

**Container lifecycle:**
```
create → configure → start → (running) → stop → delete
                         ↑_____________|
```

### 4. Templates (`/templates`)

Pre-configured agent blueprints. Stored as YAML files.

```yaml
# templates/agents.yaml
templates:
  coding-assistant:
    name: "Coding Assistant"
    config:
      llm_provider: openai
      llm_model: gpt-4o
      memory_mb: 1024
      cpu_cores: 1.0
    env:
      OPENAI_API_KEY: ${OPENAI_API_KEY}
```

Templates are **starting points** - everything can be overridden at creation time.

### 5. Teams & Routing (`/teams`)

Multi-agent teams with intelligent message routing.

```toml
# teams/finance-team.toml
[team]
name = "Finance AI Team"

[router]
mode = "hybrid"  # keyword, llm, or hybrid

[agents]
receipts = { agent = "finn", description = "Expense receipts" }
payables = { agent = "pax", description = "Bills to pay" }

[routing.receipts]
keywords = ["receipt", "expense", "bought"]
```

See [TEAMS.md](TEAMS.md) for full documentation.

---

## Authentication Flow

Claw Pen uses JWT-based authentication:

```
┌──────────┐     POST /auth/login      ┌──────────────┐
│  Client  │ ──────────────────────────▶│ Orchestrator │
│          │     {password}             │              │
│          │                            │  (verify     │
│          │◀────────────────────────── │   password)  │
│          │  {access_token,            │              │
│          │   refresh_token}           │              │
└──────────┘                            └──────────────┘
     │
     │  All subsequent requests:
     │  Authorization: Bearer <token>
     │
     ▼
┌──────────────┐
│ API Endpoint │──▶ Validate JWT ──▶ Process request
└──────────────┘
```

**Token types:**
- **Access token** - 24-hour validity, used for API calls
- **Refresh token** - 7-day validity, get new access token without password

**Password storage:** Argon2id hashing with random salt.

---

## Data Flow: Creating an Agent

```
1. Client Request
   POST /api/agents
   {
     "name": "my-agent",
     "template": "coding-assistant",
     "config": { "memory_mb": 2048 }
   }

2. Orchestrator Processing
   ├── Validate input (name, config limits)
   ├── Load template defaults
   ├── Merge with user overrides
   ├── Generate container ID
   └── Store agent config

3. Container Creation
   ├── Pull image (if needed)
   ├── Create container with:
   │   ├── Resource limits (memory, CPU)
   │   ├── Environment variables
   │   ├── Volume mounts
   │   └── Network config
   └── Register with AndOR Bridge (if enabled)

4. Response
   {
     "id": "abc123",
     "name": "my-agent",
     "status": "created",
     "config": { ... }
   }
```

---

## Deployment Modes

### Mode 1: All Windows (WSL2)

```
Windows
├── Tauri App (GUI)
├── Orchestrator (port 3000)
├── AndOR Bridge (port 3456)
├── OpenClaw Gateway (port 18789)
└── WSL2
    └── Docker + Agent Containers
```

### Mode 2: All Linux

```
Linux (single machine)
├── Tauri App (or remote via Tailscale)
├── Orchestrator (port 3000)
├── Docker + Agent Containers
└── Model Server (Ollama/LM Studio)
```

### Mode 3: Split (Windows + Linux VM)

```
Linux VM (Tailnet)
├── Orchestrator (port 3000)
├── Docker + Agent Containers
└── Model Server

Windows
├── Tauri App
└── AndOR Bridge → connects to Linux
```

Configure via `claw-pen.toml`:
```toml
deployment-mode = "linux-native"  # or "windows-wsl"
runtime-socket = "/var/run/docker.sock"
```

---

## Networking

### Mesh Networking (Tailscale/Headscale)

Each agent can get its own Tailscale IP for secure, direct access:

```bash
# Enable in config
network-backend = "tailscale"
tailscale-auth-key = "tskey-auth-xxxxx"
```

### AndOR Bridge Integration

Automatic agent registration with [AndOR Hub](https://github.com/AchyErrorJ/andor-bridge):

```bash
ANDOR_BRIDGE__URL=http://localhost:3456
ANDOR_BRIDGE__REGISTER_ON_CREATE=true
```

Each agent gets its own DM channel for direct messaging.

---

## API Endpoints

### Authentication

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/auth/login` | Get JWT tokens (public) |
| POST | `/auth/register` | Create admin user (disabled by default) |
| POST | `/auth/refresh` | Refresh access token |
| GET | `/auth/status` | Check auth configuration |

### Agents

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/agents` | List all agents |
| POST | `/api/agents` | Create new agent |
| GET | `/api/agents/:id` | Get agent details |
| PUT | `/api/agents/:id` | Update agent config |
| DELETE | `/api/agents/:id` | Delete agent |
| POST | `/api/agents/:id/start` | Start agent |
| POST | `/api/agents/:id/stop` | Stop agent |
| GET | `/api/agents/:id/logs` | Get logs |
| WS | `/api/agents/:id/chat` | Chat with agent |
| WS | `/api/agents/:id/logs/stream` | Stream logs |

### Teams

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/teams` | List all teams |
| GET | `/api/teams/:id` | Get team config |
| POST | `/api/teams/:id/classify` | Classify a message |
| WS | `/api/teams/:id/chat` | Routed team chat |

### System

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check (public) |
| GET | `/api/templates` | List available templates |
| GET | `/api/metrics` | Global metrics |
| GET | `/api/runtime/status` | Runtime status |

---

## Security Model

### Authentication
- JWT tokens required for all API endpoints (except `/health`, `/auth/*`)
- Argon2id password hashing
- 24-hour token expiry with refresh mechanism

### Authorization
- Single-user mode (admin only) currently
- Multi-user support planned

### Container Isolation
- Each agent in separate container
- Resource limits enforced (memory, CPU)
- Network isolation with optional mesh VPN

### Secrets Management
- API keys stored encrypted at rest
- Never logged or exposed in API responses
- Per-agent secret scoping

---

## File Structure

```
claw-pen/
├── orchestrator/          # Rust API server
│   ├── src/
│   │   ├── main.rs       # Entry point, route setup
│   │   ├── api.rs        # HTTP/WebSocket handlers
│   │   ├── auth.rs       # JWT authentication
│   │   ├── container.rs  # Docker runtime client
│   │   └── ...
│   └── Cargo.toml
├── templates/             # Agent templates
│   ├── agents.yaml       # Template definitions
│   ├── providers.yaml    # Provider catalog
│   └── openclaw-agent/   # Template files
├── teams/                 # Team configurations
│   └── *.toml            # Team TOML files
├── agents/                # Example agent setups
├── tauri-app/            # Desktop GUI
├── ui/                   # Web dashboard (Yew/WASM)
└── runtime/              # Custom container runtime (future)
```

---

## Extending Claw Pen

### Adding a New Template

1. Edit `templates/agents.yaml`:
```yaml
templates:
  my-custom:
    name: "My Custom Agent"
    config:
      llm_provider: openai
      llm_model: gpt-4o
    env:
      OPENAI_API_KEY: ${OPENAI_API_KEY}
```

2. Restart the orchestrator - template loads automatically.

### Adding a New Provider

1. Edit `templates/providers.yaml`:
```yaml
providers:
  my-provider:
    tier: optional
    name: "My Provider"
    env_key: "MY_PROVIDER_API_KEY"
    models:
      - id: "model-1"
        name: "Model 1"
        context: 128000
    default_model: "model-1"
```

### Creating a Team

1. Create `teams/my-team.toml`:
```toml
[team]
name = "My Team"

[router]
mode = "keyword"

[agents]
agent1 = { agent = "agent-a", description = "Does X" }
agent2 = { agent = "agent-b", description = "Does Y" }

[routing.agent1]
keywords = ["x", "thing1"]
```

2. Restart orchestrator or call team reload endpoint.

---

## Related Documentation

- [Getting Started](GETTING-STARTED.md) - Quick start guide
- [Templates](TEMPLATES.md) - Template creation guide
- [Teams](TEAMS.md) - Multi-agent routing
- [Secrets](SECRETS.md) - Secure key management
