# Getting Started with Claw Pen

Get your first AI agent running in under 5 minutes.

## Prerequisites

Before you start, make sure you have:

- **Docker** 20.10+ (or Podman 4.0+) OR **Exo** container runtime
- **Node.js** 18+ (for Tauri app, optional)
- **Rust** 1.70+ (for building from source)
- **4GB RAM** minimum (8GB+ recommended for local models)

### Container Runtime Options

Claw Pen supports two container runtimes:

| Runtime | Description | Best For |
|---------|-------------|----------|
| **Docker** | Default, widely supported | Most users, production |
| **Exo** | Agent-first container runtime | AI agents, rootless, fast spawning |

#### Installing Exo (Optional)

If you want to use Exo instead of Docker:

```bash
# From source
git clone https://github.com/clawpen/exo.git
cd exo
cargo build --release
sudo cp target/release/exo /usr/local/bin/

# Verify installation
exo --version
```

Exo features:
- **Agent-first communication** — Stdio + tool bus, not HTTP
- **Tool-level sandboxing** — Each tool gets its own security context
- **Fast spawning** — Daemonless, spin up in milliseconds
- **Rootless by default** — User namespaces, no system privileges required

### Verify Prerequisites

```bash
# Check Docker
docker --version  # Should show 20.10+

# Or check Exo (if using Exo)
exo --version

# Check Rust (if building from source)
rustc --version   # Should show 1.70+

# Check Node.js (for Tauri desktop app)
node --version    # Should show 18+
```

## Installation

### Option 1: Quick Install (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/AchyErrorJ/claw-pen/main/scripts/install.sh | bash
```

This downloads the pre-built binary and installs it to `~/.local/bin/claw-pen`.

### Option 2: Build from Source

```bash
git clone https://github.com/AchyErrorJ/claw-pen.git
cd claw-pen/orchestrator
cargo build --release
```

The binary will be at `target/release/claw-pen-orchestrator`.

### Option 3: Tauri Desktop App

For a GUI experience with a setup wizard:

```bash
cd claw-pen/tauri-app
npm install
npm run tauri build
```

Install the resulting package for your platform.

---

## Configure Container Runtime

By default, Claw Pen uses Docker. To use Exo instead:

### Via Config File

Create `claw-pen.toml`:

```toml
# Use Exo runtime instead of Docker
container-runtime = "exo"

# Optional: Custom path to exo binary
# exo-path = "/usr/local/bin/exo"

# Other settings
deployment-mode = "linux-native"
network-backend = "tailscale"
```

### Via Environment Variable

```bash
export CONTAINER_RUNTIME=exo
# Optional: custom exo path
export EXO_PATH=/path/to/exo
```

---

## Set Your Admin Password

Claw Pen uses JWT authentication. Before using the API, set an admin password:

```bash
# Using the orchestrator binary
./claw-pen-orchestrator --set-password

# Or if installed via script
claw-pen-orchestrator --set-password
```

You'll be prompted to enter a password (minimum 8 characters).

> **Alternative:** Enable one-time registration with `ENABLE_REGISTRATION=true` environment variable, then call `POST /auth/register` with your password. Disable it after first use.

---

## Start the Orchestrator

```bash
# From the orchestrator directory
./claw-pen-orchestrator

# Or with custom config
./claw-pen-orchestrator --config /path/to/claw-pen.toml
```

The orchestrator starts on `http://localhost:3000` by default.

You should see:
```
🦀 Claw Pen orchestrator listening on 0.0.0.0:3000
🔐 JWT authentication enabled - all API endpoints require Bearer token
   GET /auth/status to check auth configuration
   POST /auth/login to authenticate
```

---

## Get Your Access Token

All API endpoints require authentication. Get a token:

```bash
# Login and save the token
curl -X POST http://localhost:3000/auth/login \
  -H "Content-Type: application/json" \
  -d '{"password": "your-password"}' \
  | jq -r '.access_token' > /tmp/token.txt

# Use in subsequent requests
export TOKEN=$(cat /tmp/token.txt)
```

The token is valid for 24 hours. Use `/auth/refresh` to get a new one without re-entering your password.

---

## Create Your First Agent

### Using a Template (Easiest)

```bash
# Create a coding assistant (uses global runtime - Docker by default)
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-coder",
    "template": "coding-assistant"
  }'
```

### With Exo Runtime (Per-Agent)

You can specify the runtime per-agent, regardless of the global default:

```bash
# Create an agent that uses Exo runtime
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "exo-agent",
    "template": "coding-assistant",
    "runtime": "exo"
  }'
```

### With Custom Configuration

```bash
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-agent",
    "template": "openclaw-agent",
    "config": {
      "llm_provider": "anthropic",
      "llm_model": "claude-sonnet-4",
      "memory_mb": 2048,
      "cpu_cores": 2.0,
      "env_vars": {
        "ANTHROPIC_API_KEY": "your-key-here"
      }
    }
  }'
```

### Using a Local Model (No API Key)

```bash
# With Ollama (install from ollama.ai first)
ollama serve &
ollama pull llama3.2

curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "local-agent",
    "template": "local-assistant"
  }'
```

---

## Start and Chat with Your Agent

### Start the Agent

```bash
curl -X POST http://localhost:3000/api/agents/my-coder/start \
  -H "Authorization: Bearer $TOKEN"
```

### Chat via WebSocket

```bash
# Using wscat (npm install -g wscat)
wscat -c "ws://localhost:3000/api/agents/my-coder/chat?token=$TOKEN"
```

Send messages as JSON:
```json
{"content": "Help me write a Python function to sort a list"}
```

### View Logs

```bash
# Stream logs
curl -N http://localhost:3000/api/agents/my-coder/logs \
  -H "Authorization: Bearer $TOKEN"

# Or via WebSocket
wscat -c "ws://localhost:3000/api/agents/my-coder/logs/stream?token=$TOKEN"
```

---

## Manage Your Agents

### List All Agents

```bash
curl http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN"
```

### Filter by Runtime

```bash
# List only Docker agents
curl "http://localhost:3000/api/agents?runtime=docker" \
  -H "Authorization: Bearer $TOKEN"

# List only Exo agents
curl "http://localhost:3000/api/agents?runtime=exo" \
  -H "Authorization: Bearer $TOKEN"
```

### Stop an Agent

```bash
curl -X POST http://localhost:3000/api/agents/my-coder/stop \
  -H "Authorization: Bearer $TOKEN"
```

### Delete an Agent

```bash
curl -X DELETE http://localhost:3000/api/agents/my-coder \
  -H "Authorization: Bearer $TOKEN"
```

---

## Available templates

| Template | Description | Provider | Use Case |
|----------|-------------|----------|----------|
| `openclaw-agent` | Full OpenClaw with built-in webchat | z.ai | General assistant |
| `coding-assistant` | General coding helper | OpenAI | Programming tasks |
| `code-reviewer` | Reviews code quality & security | Anthropic | PR reviews |
| `local-assistant` | Runs locally via Ollama | Ollama | Private, offline |
| `lm-studio` | Runs locally via LM Studio | LM Studio | Local with GUI |
| `researcher` | Web search and summarization | OpenAI | Research tasks |
| `devops` | Infrastructure and deployment | OpenAI | DevOps tasks |
| `tutor-box` | Adaptive learning companion | z.ai | Learning anything |

Override any template's provider/model:

```bash
# Researcher template with local model
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

---

## Runtime Status

Check the current runtime configuration:

```bash
curl http://localhost:3000/api/runtime/status \
  -H "Authorization: Bearer $TOKEN"

# Returns:
# {
#   "runtime": "exo",
#   "version": "1.0.0",
#   "agents": {
#     "total": 5,
#     "running": 2
#   }
# }
```

---

## Next Steps

- [Architecture Overview](ARCHITECTURE.md) - How it all works
- [Template Guide](TEMPLATES.md) - Create custom templates
- [Teams & Routing](TEAMS.md) - Multi-agent teams with smart routing
- [GitHub Repository](https://github.com/AchyErrorJ/claw-pen)

---

## Troubleshooting

### "Invalid credentials" on login

- Make sure you've set a password with `--set-password`
- Check the password is correct (8+ characters)
- Verify the orchestrator is running

### "Connection refused" errors

- Check the orchestrator is running: `curl http://localhost:3000/health`
- Verify the port (default 3000) isn't blocked by firewall

### Agent won't start

- Check Docker is running: `docker ps`
- Or check Exo is available: `exo ps`
- View orchestrator logs for error details
- Verify the template exists: `curl http://localhost:3000/api/templates -H "Authorization: Bearer $TOKEN"`

### "No admin password set" warning

Run the password setup:
```bash
./claw-pen-orchestrator --set-password
```

### Token expired

Refresh your token:
```bash
curl -X POST http://localhost:3000/auth/refresh \
  -H "Content-Type: application/json" \
  -d "{\"refresh_token\": \"$(cat /tmp/refresh.txt)\"}"
```

### WebSocket connection fails

Make sure to include the token as a query parameter:
```
ws://localhost:3000/api/agents/my-coder/chat?token=YOUR_JWT_TOKEN
```

### Local model (Ollama/LM Studio) not working

1. Ensure the model server is running:
   ```bash
   # Ollama
   ollama serve
   ollama list  # Check available models

   # LM Studio - start server from GUI
   ```

2. Configure endpoint in `.env`:
   ```
   MODEL_SERVERS__OLLAMA__ENDPOINT=http://localhost:11434
   MODEL_SERVERS__LM_STUDIO__ENDPOINT=http://localhost:1234
   ```

### Exo runtime not available

1. Verify exo is installed:
   ```bash
   exo --version
   ```

2. If not in PATH, specify the path in config:
   ```toml
   exo-path = "/path/to/exo"
   ```

3. Or via environment variable:
   ```bash
   export EXO_PATH=/path/to/exo
   ```

### Docker permission denied

Add your user to the docker group:
```bash
sudo usermod -aG docker $USER
# Log out and back in
```

---

## Need Help?

- [GitHub Issues](https://github.com/AchyErrorJ/claw-pen/issues) - Bug reports
- [Discord](https://discord.gg/claw-pen) - Community support
- [Documentation](https://claw-pen.dev/docs) - Full docs
