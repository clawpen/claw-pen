# Agent Identities

This directory stores agent identity files that are mounted into containers at `/agent/identity.yaml`.

## Directory Structure

```
identities/
├── .gitkeep
├── README.md
├── agent-name-1/
│   └── identity.yaml
└── agent-name-2/
    └── identity.yaml
```

## Identity File Format

Each `identity.yaml` file contains:

```yaml
name: "agent-name"
display_name: "Agent Display Name"
description: "What this agent does"

identity:
  role: "Professional role or title"
  personality: "Short personality description"
  expertise:
    - skill1
    - skill2
    - skill3

system_prompt: |
  You are {{name}}, {{role}}.
  Your personality: {{personality}}
  You specialize in: {{expertise}}

  {{custom_instructions}}

metadata:
  created_at: "2026-04-01T12:00:00Z"
  template: "template-name-or-custom"
  version: "1.0.0"
```

## Template Variables

The `system_prompt` field supports these variables:
- `{{name}}` - Agent's container name
- `{{role}}` - Professional role
- `{{personality}}` - Personality description
- `{{expertise}}` - Comma-separated list of expertise
- `{{description}}` - Agent description

## How It Works

1. When creating an agent via the UI, an identity file is generated
2. The file is saved to `identities/{agent-name}/identity.yaml`
3. The file is mounted into the container at `/agent/identity.yaml` (read-only)
4. The agent reads this file on startup to configure its personality

## Manual Editing

You can manually edit identity files:

1. Navigate to `identities/{agent-name}/identity.yaml`
2. Edit the file with your changes
3. Restart the agent to apply changes

**Note:** Changes made manually will be overwritten if you update the agent through the UI.
