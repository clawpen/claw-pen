# AndOR Hub + OpenClaw VM Deployment

A self-hosted development hub with git, chat, video calls, file transfer, and AI assistant — all connected via Tailscale.

## Features

- **Git Server** — Push/pull repos over HTTP
- **Chat Server** — Real-time messaging with channels and DMs
- **Video Calls** — WebRTC-powered video chat
- **File Transfer** — Upload/share files with your team
- **OpenClaw AI** — Chat with Codi directly from AndOR
- **Build Environments** — Docker-based build automation
- **Tailscale** — Secure mesh VPN, no port forwarding needed

## Requirements

- Linux VM (Ubuntu 22.04+ recommended)
- 75GB disk (for repos, builds, files)
- 4GB RAM minimum (8GB recommended for builds)
- Root/sudo access

## Quick Start

### Option 1: One-liner install

```bash
curl -fsSL https://your-repo/setup-vm.sh | sudo TAILSCALE_AUTH_KEY=tskey-xxx bash
```

### Option 2: Manual setup

```bash
# Clone the repo
git clone https://github.com/YOUR_USERNAME/andor-hub.git /opt/andor
cd /opt/andor

# Run setup script
sudo ./deploy/setup-vm.sh
```

### Option 3: From your existing machine

```bash
# Copy to VM via Tailscale
scp -r ~/Desktop/software/company-chat-v2 user@andor-vm:/opt/andor
ssh user@andor-vm "cd /opt/andor && sudo ./deploy/setup-vm.sh"
```

## Post-Install

1. **Connect Tailscale** (if not using auth key):
   ```bash
   sudo tailscale up --hostname=andor-hub
   ```

2. **Create first user**:
   ```bash
   curl -X POST http://localhost:8080/api/register \
     -H "Content-Type: application/json" \
     -d '{"username":"admin","password":"your-password"}'
   ```

3. **Open AndOR Hub**:
   ```
   http://<tailscale-ip>:8080
   ```

4. **Configure OpenClaw**:
   ```bash
   openclaw dashboard
   ```

## Services

| Service | Port | Description |
|---------|------|-------------|
| AndOR Hub | 8080 | Main web interface |
| OpenClaw Gateway | 18789 | AI assistant API |
| OpenClaw Bridge | 3456 | AndOR ↔ OpenClaw relay |

## Management

```bash
# View logs
sudo journalctl -u andor-hub -f
sudo journalctl -u openclaw-gateway -f
sudo journalctl -u openclaw-bridge -f

# Restart services
sudo systemctl restart andor-hub
sudo systemctl restart openclaw-gateway

# Check status
sudo systemctl status andor-hub
```

## Build Environments

Coming soon: Pre-configured Docker environments for:
- Node.js apps
- Python projects
- Rust projects
- Go projects
- Embedded/firmware

## Architecture

```
┌─────────────────────────────────────────────┐
│                  Your Devices                │
│   (Laptop, Phone, Tablet via Tailscale)     │
└─────────────────┬───────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────┐
│               Linux VM (AndOR Hub)           │
│                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  │
│  │ AndOR    │  │ OpenClaw │  │  Docker  │  │
│  │ Hub      │◄─┤  Bridge  │◄─┤  Builds  │  │
│  │ :8080    │  │  :3456   │  │          │  │
│  └──────────┘  └────┬─────┘  └──────────┘  │
│                     │                       │
│               ┌─────▼─────┐                 │
│               │ OpenClaw  │                 │
│               │ Gateway   │                 │
│               │ :18789    │                 │
│               └───────────┘                 │
└─────────────────────────────────────────────┘
```

## Security

- Firewall only allows Tailscale interface + essential ports
- JWT-based authentication
- SQLite database with local storage only
- Optional HTTPS via reverse proxy (Caddy, Nginx)

## License

MIT
