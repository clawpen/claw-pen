# Claw Pen Templates Guide

Templates are pre-configured agent blueprints that let you deploy agents quickly with sensible defaults.

## Quick Reference

| Template | Provider | Default Model | Use Case |
|----------|----------|---------------|----------|
| `openclaw-agent` | z.ai | glm-5 | Full OpenClaw with built-in webchat |
| `coding-assistant` | OpenAI | gpt-4o | General coding tasks |
| `code-reviewer` | Anthropic | claude-3-5-sonnet | PR reviews, code quality |
| `local-assistant` | Ollama | llama3.2 | Private, offline usage |
| `lm-studio` | LM Studio | (auto) | Local with GUI management |
| `researcher` | OpenAI | gpt-4o | Web search, summarization |
| `devops` | OpenAI | gpt-4o | Infrastructure, deployment |
| `tutor-box` | z.ai | glm-5 | Learning and education |
| `kimi` | Moonshot | moonshot-v1-128k | Long-context Chinese/English |
| `zai` | z.ai | (default) | z.ai powered assistant |

---

## Using Templates

### Basic Usage

```bash
# Create from template (uses all defaults)
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-coder", "template": "coding-assistant"}'
```

### Override Configuration

Templates are starting points - override anything:

```bash
# Coding assistant with different model
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "fast-coder",
    "template": "coding-assistant",
    "config": {
      "llm_model": "gpt-4o-mini",
      "memory_mb": 512
    }
  }'
```

### Use a Different Provider

```bash
# Researcher with local Ollama model
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

## Template Configuration

### Configurable Options

| Option | Type | Description |
|--------|------|-------------|
| `llm_provider` | string | AI provider (openai, anthropic, ollama, etc.) |
| `llm_model` | string | Model identifier |
| `memory_mb` | int | Container memory limit (MB) |
| `cpu_cores` | float | CPU cores allocated |
| `env_vars` | object | Environment variables |
| `volumes` | array | Volume mounts |
| `secrets` | array | Secret names to inject |
| `ports` | array | Exposed ports |

### Example: Full Custom Config

```bash
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "production-agent",
    "template": "openclaw-agent",
    "config": {
      "llm_provider": "anthropic",
      "llm_model": "claude-opus-4-6",
      "memory_mb": 4096,
      "cpu_cores": 2.0,
      "env_vars": {
        "LOG_LEVEL": "debug",
        "MAX_TOKENS": "8192"
      },
      "volumes": [
        {"host": "/data/projects", "target": "/workspace"}
      ]
    },
    "tags": ["production", "critical"]
  }'
```

---

## Available Providers

### Essential Tier

| Provider | Env Key | Notes |
|----------|---------|-------|
| **OpenAI** | `OPENAI_API_KEY` | GPT models, Codex subscription option |
| **Anthropic** | `ANTHROPIC_API_KEY` | Claude models with prompt caching |
| **OpenRouter** | `OPENROUTER_API_KEY` | Unified access to 100+ models |

### Popular Tier

| Provider | Env Key | Notes |
|----------|---------|-------|
| **z.ai (GLM)** | `ZAI_API_KEY` | GLM-5, strong multilingual |
| **Venice AI** | `VENICE_API_KEY` | Privacy-first, Claude/GPT via proxy |
| **Moonshot (Kimi)** | `MOONSHOT_API_KEY` | Large context, Chinese/English |
| **Mistral** | `MISTRAL_API_KEY` | European AI, vision support |
| **Together AI** | `TOGETHER_API_KEY` | Open-source models at scale |

### Free Tier

| Provider | Env Key | Notes |
|----------|---------|-------|
| **HuggingFace** | `HF_TOKEN` | Free tier available, many models |

### Local (No API Key)

| Provider | Endpoint | Notes |
|----------|----------|-------|
| **Ollama** | localhost:11434 | Auto-discovers local models |
| **LM Studio** | localhost:1234 | Start server from GUI |

---

## Creating Custom Templates

### Method 1: YAML Definition

Add to `templates/agents.yaml`:

```yaml
templates:
  my-custom:
    name: "My Custom Agent"
    description: "Does something specific"
    config:
      llm_provider: anthropic
      llm_model: claude-sonnet-4
      memory_mb: 2048
      cpu_cores: 1.5
    env:
      ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY}
      CUSTOM_SETTING: "value"
```

### Method 2: Template Directory

Create a full template with custom files:

```
templates/
└── my-template/
    ├── template.yaml      # Template metadata
    ├── Containerfile      # Custom container (optional)
    ├── entrypoint.sh      # Custom startup (optional)
    └── workspace/         # Default workspace files
        └── config.json
```

**template.yaml:**
```yaml
name: "My Template"
description: "Custom template with files"
config:
  llm_provider: openai
  llm_model: gpt-4o
  memory_mb: 1024
env:
  OPENAI_API_KEY: ${OPENAI_API_KEY}
volumes:
  - host: ""
    target: "/workspace"
```

---

## Template Inheritance

Templates can extend other templates (planned feature):

```yaml
templates:
  base-coder:
    config:
      memory_mb: 1024
      cpu_cores: 1.0

  python-coder:
    extends: base-coder
    config:
      llm_model: gpt-4o
    env:
      PYTHON_VERSION: "3.12"
```

---

## Provider Configuration

### Setting API Keys

**Per-agent (at creation):**
```bash
curl -X POST http://localhost:3000/api/agents \
  -d '{
    "name": "my-agent",
    "template": "coding-assistant",
    "config": {
      "env_vars": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }'
```

**Global (in .env):**
```bash
# .env
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
```

**Via Secrets Manager:**
```bash
# Store secret
curl -X POST http://localhost:3000/api/agents/my-agent/secrets \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"name": "OPENAI_API_KEY", "value": "sk-..."}'

# List secrets
curl http://localhost:3000/api/agents/my-agent/secrets \
  -H "Authorization: Bearer $TOKEN"
```

### Local Models (Ollama)

1. Install Ollama: `curl https://ollama.ai/install.sh | sh`
2. Pull a model: `ollama pull llama3.2`
3. Start the server: `ollama serve`
4. Create agent with `local-assistant` template

```bash
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "local-agent",
    "template": "local-assistant",
    "config": {
      "llm_model": "llama3.2"
    }
  }'
```

### LM Studio

1. Download LM Studio from lmstudio.ai
2. Load a model in the GUI
3. Start the local server (default port 1234)
4. Create agent:

```bash
curl -X POST http://localhost:3000/api/agents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "lm-agent", "template": "lm-studio"}'
```

---

## Premium Templates

Some templates are premium add-ons with specialized capabilities:

| Template | Description | Price |
|----------|-------------|-------|
| `lead-hunter` | Kijiji scraper for business leads | $29 one-time |
| `billing-assistant` | Time tracking with auto-invoicing | $39/mo (planned) |
| `brief-synthesizer` | Document analysis, case summaries | $29 (planned) |
| `research-tool` | Deep research with citations | $29 (planned) |

Premium templates include:
- Specialized tools and integrations
- Pre-configured workflows
- Dedicated support

Contact sales or check the template catalog for availability.

---

## Template Best Practices

### 1. Start with Templates, Customize Later

```bash
# Quick start
claw-pen create --template coding-assistant --name dev-agent

# Later, update with specific needs
curl -X PUT http://localhost:3000/api/agents/dev-agent \
  -d '{"config": {"llm_model": "gpt-4o-mini"}}'
```

### 2. Use Tags for Organization

```bash
curl -X POST http://localhost:3000/api/agents \
  -d '{
    "name": "prod-coder",
    "template": "coding-assistant",
    "tags": ["production", "backend"]
  }'

# Filter by tag
curl "http://localhost:3000/api/agents?tag=production"
```

### 3. Resource Allocation Guidelines

| Use Case | Memory | CPU |
|----------|--------|-----|
| Simple chat | 512MB | 0.5 |
| Coding | 1-2GB | 1.0 |
| Local models | 4-8GB | 2-4 |
| Multi-tool agents | 2-4GB | 1.5-2 |

### 4. Security

- Store API keys as secrets, not env vars in config files
- Use environment variable substitution: `${OPENAI_API_KEY}`
- Rotate keys regularly via secrets manager

---

## Troubleshooting

### "Template not found"

- Check template name spelling
- Verify template exists: `curl http://localhost:3000/api/templates`
- Ensure YAML syntax is valid in `agents.yaml`

### API key not working

- Verify key is set: check env vars or secrets
- For local models, ensure server is running
- Check provider endpoint configuration

### Agent uses wrong model

- Model override may not have taken effect
- Verify config was applied: `GET /api/agents/:id`
- Restart agent after config changes

### Out of memory

- Increase `memory_mb` in config
- For local models, ensure host has enough RAM
- Check container logs for OOM errors

---

## Related Documentation

- [Getting Started](GETTING-STARTED.md) - First agent setup
- [Architecture](ARCHITECTURE.md) - How templates work
- [Template Catalog](../templates/TEMPLATES.md) - Full template list
- [Providers](../templates/providers.yaml) - Provider configurations
