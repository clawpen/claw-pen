# TOOLS.md - Local Notes

Skills define _how_ tools work. This file is for _your_ specifics — the stuff that's unique to your setup.

## What Goes Here

Things like:

- Camera names and locations
- SSH hosts and aliases
- Preferred voices for TTS
- Speaker/room names
- Device nicknames
- Anything environment-specific

## Examples

```markdown
### Cameras

- living-room → Main area, 180° wide angle
- front-door → Entrance, motion-triggered

### SSH

- home-server → 192.168.1.100, user: admin

### TTS

- Preferred voice: "Nova" (warm, slightly British)
- Default speaker: Kitchen HomePod
```

## Why Separate?

Skills are shared. Your setup is yours. Keeping them apart means you can update skills without losing your notes, and share skills without leaking your infrastructure.

---

## Ports Reference

### This Machine (pop-os)

**Tailscale:** `100.77.212.101` (fd7a:115c:a1e0::a039:d465)

| Port | Service | Notes |
|------|---------|-------|
| 22 | SSH | System |
| 53 | systemd-resolved | DNS |
| 631 | CUPS | Printing |
| 8081 | Claw Pen Orchestrator | API + GUI backend |
| 8080 | AndOR Hub | Dashboard (when running) |
| 11434 | Ollama | Local LLM |
| 1234 | LM Studio | Alternative LLM |
| 18789 | OpenClaw Gateway (main) | Cod's API |
| 18790 | Tutor Box Agent | Test agent |
| 18791-18792 | OpenClaw Gateway | Internal channels |
| 3456 | AndOR Bridge | Multi-agent routing (when running) |

### Agent Containers

- Internal port: `8080` (agent API)
- Host port: dynamically assigned by Docker

### Allocation Strategy

- **3000-3999:** Claw Pen services
- **8000-8999:** Web UIs, dashboards
- **10000+:** LLMs, external tools
- **18700-18799:** OpenClaw agents (per-agent instances)
- **18789:** Main OpenClaw gateway (always reserved for Cod)

---

Add whatever helps you do your job. This is your cheat sheet.
