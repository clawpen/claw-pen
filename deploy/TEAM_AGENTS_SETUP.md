# Team Agents Setup Guide

Connect containerized AI agents to AndOR Hub as team members with both chat and git access.

## Two Modes

### Multiplexed Mode (Recommended for 16GB or less)

```
┌─────────────────────────────────────────────┐
│  Single OpenClaw Container (~2GB)           │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐            │
│  │ PM  │ │ Dev │ │ QA  │ │ ... │  roles     │
│  └──┬──┘ └──┬──┘ └──┬──┘ └──┬──┘            │
│     │       │       │       │               │
│     └───────┴───────┴───────┘               │
│             │                               │
│     Role switching via                      │
│     system prompt injection                 │
└─────────────────────────────────────────────┘
```

**Pros**: Memory efficient (2GB total), fast role switching
**Cons**: Shared context between roles, single point of failure

### Isolated Mode (For 32GB+ or maximum separation)

```
┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐
│ PM      │  │ Dev     │  │ QA      │  │ ...     │
│ ~1.5GB  │  │ ~1.5GB  │  │ ~1.5GB  │  │ ~1.5GB  │
│ Agent 1 │  │ Agent 2 │  │ Agent 3 │  │ Agent N │
└─────────┘  └─────────┘  └─────────┘  └─────────┘
```

**Pros**: Full isolation, independent scaling, can run different models
**Cons**: ~10.5GB for 7 roles, more containers to manage

## Switching Modes

Edit `deploy/team-agents.json`:

```json
{
  "_mode": "multiplexed",  // or "isolated"
  ...
}
```

Then restart the bridge.

## Overview

### Memory Requirements

| Mode | RAM Needed | Containers | Use Case |
|------|------------|------------|----------|
| Multiplexed | ~2-3GB | 1 | 16GB machines, testing |
| Isolated | ~10-12GB | 7 | 32GB+ machines, production |

```
┌─────────────────────────────────────────────────────────────┐
│                    AndOR Hub (Windows)                       │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐            │
│  │ Alex PM │ │ Dan Dev │ │ Sam QA  │ │ Taylor  │  ...team   │
│  └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘            │
│       │           │           │           │                  │
│       └───────────┴───────────┴───────────┘                  │
│                           │                                  │
│                    Git Server + Chat                         │
└───────────────────────────┼─────────────────────────────────┘
                            │ Webhook
                    AndOR Bridge (Linux:3456)
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
   ┌────┴────┐        ┌────┴────┐        ┌────┴────┐
   │ Agent 1 │        │ Agent 2 │        │ Agent N │
   │ OpenClaw│        │ OpenClaw│        │ OpenClaw│
   └────┬────┘        └────┬────┘        └────┬────┘
        │                  │                  │
        └──────────────────┴──────────────────┘
                           │
                   Git Clone/Push
                           │
                    AndOR Git Server
```

## Quick Start

### 1. Seed Agent Users in AndOR Hub

On the AndOR Hub machine (Windows or wherever the DB lives):

```bash
cd /data/claw-pen/deploy
node seed-team-agents.js /path/to/local_chat.db
```

This creates user accounts for each team agent:
- `alex-pm` - Project Manager
- `dan-dev` - Developer
- `sam-qa` - QA Engineer
- `diana-design` - Designer
- `taylor-devops` - DevOps
- `morgan-security` - Security
- `jordan-architect` - Architect

### 2. Start the Team Bridge

On the Linux machine running OpenClaw:

```bash
cd /data/claw-pen/deploy

# Set environment if needed
export OPENCLAW_GATEWAY_URL=http://127.0.0.1:18789
export ANDOR_SERVER_URL=https://aesir.tailb0b4db.ts.net:8080

# Start the bridge
node andor-team-bridge.js
```

### 3. Configure AndOR Hub Webhook

In AndOR Hub, configure the OpenClaw webhook URL:

```
http://<linux-ip>:3456/webhook
```

Or if running on the same Tailscale network:

```
http://100.77.212.101:3456/webhook
```

### 4. Chat with Agents

In AndOR Hub chat:

- **@mention**: `@dan-dev can you review this code?`
- **Channel name**: Create a channel named "dev" → Dan responds automatically
- **Prefix**: `dev what's the best approach here?`

### 5. Git Access

Agents can clone/push using their credentials:

```bash
# In a container, agent can do:
git clone http://dan-dev:dan-dev-secret@andor.example.com/repo.git
```

## Agent Templates

Templates are in `/data/claw-pen/templates/agents.yaml`:

| Template | Role | Focus |
|----------|------|-------|
| `team-pm` | Alex | Requirements, priorities, shipping |
| `team-developer` | Dan | Code, architecture, best practices |
| `team-qa` | Sam | Testing, edge cases, quality |
| `team-designer` | Diana | UX, accessibility, user empathy |
| `team-devops` | Taylor | CI/CD, infra, monitoring |
| `team-security` | Morgan | Threats, hardening, compliance |
| `team-architect` | Jordan | System design, scalability |

## Creating Agent Containers

Using Claw Pen API:

```bash
# Create a developer agent
curl -X POST http://localhost:8081/api/agents \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "name": "dan-dev",
    "template": "team-developer",
    "config": {
      "llm_provider": "zai",
      "llm_model": "glm-5",
      "memory_mb": 1536
    }
  }'
```

## Customization

### Add New Roles

1. Add template to `templates/agents.yaml`:

```yaml
  team-custom:
    name: "Custom Role (Name)"
    description: "What this role focuses on"
    role_config:
      display_name: "Name (Role)"
      emoji: "🎯"
      personality: |
        You are Name, a custom role. You focus on:
        - Thing 1
        - Thing 2
      triggers: ["name", "role", "trigger"]
    config:
      llm_provider: zai
      llm_model: glm-5
    git_access: true
```

2. Add to `deploy/team-agents.json`

3. Re-run seed script: `node seed-team-agents.js /path/to/db`

### Change Passwords

Edit `deploy/team-agents.json` and re-run the seed script.

**Important**: Change default passwords in production!

## Architecture Notes

### Why the Bridge?

AndOR Hub runs on Windows, which can't run OpenClaw directly. The bridge:
- Receives webhooks from AndOR Hub
- Routes messages to the correct agent
- Injects role-specific personalities
- Returns responses to AndOR chat

### Git Authentication

AndOR Hub uses Basic auth for git operations. Agent credentials work for both:
- Chat: via webhook → bridge → OpenClaw
- Git: `http://username:password@andor/repo.git`

### Session Isolation

Each agent-role combination gets its own session:

```
andor:<conversation-id>:pm
andor:<conversation-id>:developer
andor:<conversation-id>:qa
```

This means agents don't share context across roles.

## Monitoring

- **Status page**: http://localhost:3456/status
- **Health check**: http://localhost:3456/health
- **Agent list**: http://localhost:3456/agents
- **Stats**: http://localhost:3456/stats

## Troubleshooting

### Agent not responding

1. Check bridge logs for errors
2. Verify webhook URL in AndOR Hub
3. Check trigger spelling (`@dan-dev` not `@dan`)

### Git push fails

1. Verify agent user exists in DB: `SELECT * FROM users WHERE username='dan-dev'`
2. Check password matches
3. Verify repo exists and is accessible

### Multiple agents respond

This happens if triggers overlap. Check `team-agents.json` for conflicts.
