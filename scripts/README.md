# Quick Start

## Install

```bash
# Linux/macOS
curl -fsSL https://raw.githubusercontent.com/AchyErrorJ/claw-pen/main/scripts/install.sh | bash

# Or with Cargo
cargo install claw-pen
```

## Create an Agent

```bash
# Interactive setup
claw-pen create

# From template
claw-pen create --template coding-assistant --name my-coder

# Custom config
claw-pen create --name my-agent --provider openai --model gpt-4o --memory 1024
```

## Manage Agents

```bash
# List all agents
claw-pen list

# Start/stop
claw-pen start my-agent
claw-pen stop my-agent

# View logs
claw-pen logs my-agent

# Access agent
claw-pen shell my-agent
```

## Web UI

```bash
claw-pen ui
# Opens http://localhost:3000
```

## Templates

Built-in templates for common use cases:

| Template | Description |
|----------|-------------|
| `coding-assistant` | General coding helper |
| `code-reviewer` | PR/code reviews |
| `local-assistant` | Runs on Ollama |
| `researcher` | Web search + summarization |
| `devops` | Infrastructure help |

```bash
claw-pen create --template local-assistant --name local
```

## Requirements

- Docker
- (Optional) Tailscale for remote access
- (Optional) Ollama for local models
