# MEMORY.md - Long-Term Memory

Started fresh: 2026-02-23

---

## About Jer

- Name: Jer
- Timezone: EST (America/Toronto)
- Wants Codi to grow with them, be an extension of who they are
- Into coding and projects - still learning the details
- Currently interested in: vector databases, especially **sqlite-vss**

## Projects

- **Claw Pen** (`/data/claw-pen`) - OpenClaw agent deployment tool (Rust)
  - Creates/manages isolated AI agents in containers with Tailscale
  - Components: orchestrator (API), runtime (WIP), ui (Yew), tauri-app
  - AndOR Bridge integration for per-agent DM channels
  - **Domain:** clawpen.ca
  - **Status (2026-02-27):** Tauri app builds and runs, Tutor Box agent on :18790

- **Containment** (`/home/codi/Desktop/software/Containment`) - Container runtime for AI agents
  - Rust-based, uses Linux namespaces, cgroups, seccomp
  - Features: overlay2 storage, image management, agent channel, foreign binary support
  - **Status (2026-02-26):** Builds successfully, needs `/var/lib/openclaw` directory with write permissions

- **lead-hunter** (`/data/claw-pen/agents/lead-hunter`) - Kijiji scraper for And or Design
  - Uses Playwright + Firefox for browser automation (bypasses 403 blocks)
  - Searches for permit drawings, BCIN, building permit leads in Northern Ontario
  - Weekly cron: Mondays at 9 AM EST
  - **Status (2026-02-26):** Working, 140 leads collected (mostly competitors, needs keyword refinement)

## Tools & Automation

- **Google Alerts Monitor** - Daily check for leads, delivers to webchat + WhatsApp
- **Chat Transcript Export** - Every 30 min, saves to `memory/chat/YYYY-MM-DD.md`

## Preferences

- **Always spawn agents** for long tasks to keep the chat open (added to AGENTS.md 2026-02-26)

## Notes

- WhatsApp gateway has been unstable (status 440 disconnects) - 2026-02-23
- Claw Pen supports 3 deployment modes: windows-wsl, linux-native, split (Windows GUI + Linux backend)

## 2026-02-27: OpenClaw-Agent Template

Created `openclaw-agent` template for claw-pen - full OpenClaw instance in a container.

**Key learnings:**
- npm `openclaw` package is a stub - must bundle from installed version
- Use `openclaw gateway run` not `start` in containers (no systemd)
- Bundle from `~/.nvm/versions/node/v24.13.1/lib/node_modules/openclaw` (~480MB)
- OpenClaw requires Node >= 22.12.0 (not 20)
- Container needs `NODE_OPTIONS="--max-old-space-size=1536"` to avoid OOM
- Use `--network host` for port access, or map ports explicitly
- Gateway binds to loopback by default - Control UI needs special config for external access

**Architecture:**
- AndOR Hub = optional web UI dashboard
- AndOR Bridge = optional multi-agent routing
- For single agent: just webchat channel, no extra servers needed

**New templates:**
- `openclaw-agent` - Full OpenClaw with built-in chat
- `tutor-box` - Learning companion (learn anything)

**End-to-end test (2026-02-27):** ✅ Working
- Claw Pen orchestrator on :3000
- Tutor Box running on :18790
- Control UI accessible at http://localhost:18790
- Tauri desktop app in progress (Rust WebSockets)

**Tauri app issue:** webkit2gtk (Linux) has WebSocket incompatibility with OpenClaw. Solution: Rust-based WebSocket (`tokio-tungstenite`) with Tauri events for frontend communication.
