# OpenClaw Agent Template

Full OpenClaw instance with built-in webchat - no third-party channels required.

## What It Does

This template creates a complete, self-contained OpenClaw agent with:
- **Built-in webchat** - No Discord/Slack/WhatsApp needed
- **Full OpenClaw capabilities** - All tools and skills available
- **Persistent memory** - Remembers conversations
- **Containerized** - Easy deployment with Podman/Docker

## Deployment

### Quick Start

```bash
cd /data/claw-pen
claw-pen create my-agent --from templates/openclaw-agent
```

### Manual Container Build

```bash
cd /data/claw-pen
podman build -t openclaw-agent -f templates/openclaw-agent/Containerfile .

# Run with environment variables
podman run -d \
  --name my-agent \
  -p 18789:18789 \
  -e LLM_PROVIDER=zai \
  -e LLM_MODEL=glm-5 \
  -e ZAI_API_KEY=your-key \
  -v openclaw-memory:/agent/memory \
  openclaw-agent
```

### Access the Webchat

Once running, access the webchat at:
```
http://localhost:18789
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `LLM_PROVIDER` | LLM provider (zai, openai, anthropic, ollama, lmstudio) | `zai` |
| `LLM_MODEL` | Model to use | `glm-5` |
| `ZAI_API_KEY` | z.ai API key | - |
| `OPENAI_API_KEY` | OpenAI API key | - |
| `ANTHROPIC_API_KEY` | Anthropic API key | - |

### template.yaml Options

```yaml
name: openclaw-agent
display_name: "OpenClaw Agent"
description: "Full OpenClaw instance with built-in chat"

container:
  memory_mb: 1024
  cpu_cores: 1.0
  ports:
    - 18789  # OpenClaw gateway

openclaw:
  provider: zai
  model: glm-5
  memory:
    enabled: true
    persist: true
```

## Files

```
openclaw-agent/
├── template.yaml      # Template configuration
├── Containerfile      # Container build instructions
├── entrypoint.sh      # Startup script
├── README.md          # This file
└── workspace/
    ├── SOUL.md        # Agent personality
    ├── IDENTITY.md    # Name and role
    ├── USER.md        # User context (customized at creation)
    ├── AGENTS.md      # Workspace rules
    ├── HEARTBEAT.md   # Heartbeat config
    └── MEMORY.md      # Long-term memory
```

## Customization

### Change the Personality

Edit `workspace/SOUL.md` to change how the agent behaves:
- More formal or casual?
- Specialized for a domain?
- Different communication style?

### Add Custom Tools

Mount additional skills or tools:
```bash
podman run -v /path/to/skills:/agent/.openclaw/skills ...
```

### Change the Model

Override at runtime:
```bash
podman run -e LLM_PROVIDER=openai -e LLM_MODEL=gpt-4o ...
```

## Resource Requirements

- **Memory:** 1GB minimum (2GB recommended)
- **CPU:** 1 core minimum
- **Storage:** ~500MB for container image

## Troubleshooting

### Container won't start

Check logs:
```bash
podman logs my-agent
```

### Can't connect to webchat

Verify port is exposed:
```bash
podman port my-agent
```

### API key issues

Ensure environment variables are set correctly for your chosen provider.

---

**Emoji:** 🤖  
**Type:** Template
