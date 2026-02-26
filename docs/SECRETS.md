# Secrets Management

Claw Pen uses **file-based secrets** instead of environment variables for better security.

## How It Works

1. Secrets are stored on the host in a secure directory (default: `/var/lib/claw-pen/secrets/`)
2. Each secret is a single file with restricted permissions (0600)
3. Containers mount secrets read-only at `/run/secrets/{name}`
4. Agent reads the secret file at runtime

## Benefits

- **Not visible in /proc** — env vars leak via `/proc/{pid}/environ`
- **Not in container inspect** — doesn't show in container metadata
- **Rotatable** — update file on host, restart container
- **Scoped per-agent** — each agent only gets its own secrets

## Usage

### Store a secret

```bash
# CLI
claw-pen secrets set my-agent OPENAI_API_KEY sk-...

# Or directly
echo "sk-..." > /var/lib/claw-pen/secrets/my-agent/OPENAI_API_KEY
chmod 600 /var/lib/claw-pen/secrets/my-agent/OPENAI_API_KEY
```

### In agent code

```bash
# Secret is mounted at /run/secrets/OPENAI_API_KEY
export OPENAI_API_KEY=$(cat /run/secrets/OPENAI_API_KEY)
```

### With OpenClaw

OpenClaw agents automatically load secrets from `/run/secrets/` if present:

```yaml
# agent-config.yaml
llm:
  provider: openai
  # No key needed here - loaded from /run/secrets/OPENAI_API_KEY
```

## Directory Structure

```
/var/lib/claw-pen/secrets/
├── agent-coder/
│   ├── OPENAI_API_KEY
│   └── ANTHROPIC_API_KEY
├── agent-researcher/
│   └── OPENAI_API_KEY
└── agent-local/
    └── (no secrets needed for local)
```

## Security Notes

- Secrets directory should be owned by root or claw-pen user
- File permissions: 0600 (owner read/write only)
- Consider encrypting at rest with fscrypt or similar
- Secrets are NOT included in container snapshots/exports
