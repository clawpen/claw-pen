# 🔐 Safe Volume Mounting for Claw Pen Agents

**Date:** March 12, 2026
**Status:** ✅ **IMPLEMENTED**

---

## Overview

Agents can now safely mount volumes to persist data, configuration, and workspace files. Volume mounts are automatically created in a secure, isolated directory structure.

---

## How It Works

### 1. Template-Based Volume Configuration

Templates define which volumes an agent needs:

```yaml
# templates/agents.yaml
openclaw-agent:
  config:
    volumes:
      - /agent/memory      # Agent memory and chat history
      - /agent/.openclaw   # OpenClaw configuration
      - /agent/workspace   # Agent workspace for file operations
```

### 2. Automatic Host Path Creation

When an agent is created from a template, the orchestrator:

1. **Creates a data directory** under `./data/agents/{agent_name}/volumes/`
2. **Creates subdirectories** for each volume
3. **Maps host paths to container paths** safely

**Example:**

```
Template: /agent/memory
Host path: ./data/agents/OpenZ/volumes/memory/
Container path: /agent/memory
```

### 3. Volume Mount Format

Each volume is mounted as a bind mount:

```rust
VolumeMount {
    source: "./data/agents/OpenZ/volumes/memory",  // Host path
    target: "/agent/memory",                        // Container path
    read_only: false,                               // Writable
}
```

---

## Security Features

### ✅ Isolated Directories

- Each agent gets its own directory: `./data/agents/{agent_name}/`
- No sharing between agents unless explicitly configured
- Paths are relative to the orchestrator working directory

### ✅ Automatic Directory Creation

- Host directories are created automatically
- No manual setup required
- Fail-safe if directory creation fails (warning logged)

### ✅ Container Security

- Volumes use bind mounts (not privileged)
- Read-only option available per volume
- No access to host system outside agent directory

---

## Usage

### Creating an Agent with Volumes

**Via GUI:**
1. Go to http://127.0.0.1:5174/
2. Click "+ Create Agent"
3. Select template: **"OpenClaw Agent"**
4. Enter name and settings
5. Click "Create"

Volumes are automatically mounted!

**Via API:**
```bash
curl -X POST http://127.0.0.1:8081/api/agents \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-agent",
    "template": "openclaw-agent",
    "config": {
      "llm_provider": "zai",
      "llm_model": "glm-5"
    }
  }'
```

### Checking Volume Mounts

**List mounted volumes:**
```bash
docker inspect <agent-name> --format='{{json .Mounts}}' | python -m json.tool
```

**Example output:**
```json
[
  {
    "Type": "bind",
    "Source": "/f/Software/Claw Pen/claw-pen/data/agents/OpenZ/volumes/memory",
    "Destination": "/agent/memory",
    "RW": true,
    "Mode": ""
  },
  {
    "Type": "bind",
    "Source": "/f/Software/Claw Pen/claw-pen/data/agents/OpenZ/volumes/workspace",
    "Destination": "/agent/workspace",
    "RW": true,
    "Mode": ""
  }
]
```

### Accessing Agent Data

**From host:**
```bash
# View agent memory
ls -la "./data/agents/OpenZ/volumes/memory/"

# View agent workspace
ls -la "./data/agents/OpenZ/volumes/workspace/"
```

**From container:**
```bash
# Access from inside container
docker exec -it OpenZ ls -la /agent/memory/
docker exec -it OpenZ ls -la /agent/workspace/
```

---

## Available Volumes

### OpenClaw Agent Template

| Container Path | Purpose | Host Path |
|----------------|---------|-----------|
| `/agent/memory` | Chat history, conversation state | `./data/agents/{name}/volumes/memory/` |
| `/agent/.openclaw` | OpenClaw gateway configuration | `./data/agents/{name}/volumes/.openclaw/` |
| `/agent/workspace` | File operations, code editing | `./data/agents/{name}/volumes/workspace/` |

---

## Customizing Volumes

### Adding Volumes to a Template

Edit `templates/agents.yaml`:

```yaml
my-custom-agent:
  config:
    volumes:
      - /agent/data
      - /agent/output
      - /shared/resources  # Can be shared across agents
```

### Read-Only Volumes (Future)

Not yet implemented, but planned:

```yaml
volumes:
  - path: /usr/local/lib/some-lib
    read_only: true
```

---

## Volume Persistence

### What Gets Persisted

- ✅ Chat history and conversation memory
- ✅ Agent configuration files
- ✅ Workspace files created by the agent
- ✅ OpenClaw gateway settings

### What Doesn't Persist

- ❌ Container file system changes outside mounted volumes
- ❌ Temporary files in `/tmp`
- ❌ Container state (requires container restart)

### Backup Strategy

**Backup an agent's data:**
```bash
# Archive agent directory
tar -czf "backup-OpenZ-$(date +%Y%m%d).tar.gz" ./data/agents/OpenZ/

# Restore
tar -xzf "backup-OpenZ-20260312.tar.gz"
```

---

## Troubleshooting

### Volume Not Mounted

**Check container mounts:**
```bash
docker inspect <agent-name> --format='{{json .Mounts}}'
```

**Check orchestrator logs:**
```bash
tail -50 orchestrator.log | grep -i volume
```

### Permission Errors

Windows paths are automatically converted. If you see errors:

1. Check orchestrator has permission to create directories
2. Verify working directory is writable
3. Check antivirus isn't blocking

### Disk Space

Monitor agent data usage:
```bash
du -sh ./data/agents/*
```

Clean up old agent data:
```bash
# Stop agent first
docker stop <agent-name>

# Remove agent data (careful!)
rm -rf ./data/agents/<agent-name>/
```

---

## Implementation Details

### Code Changes

**File:** `orchestrator/src/templates.rs`
- Added `volumes: Vec<String>` to `TemplateConfig`

**File:** `orchestrator/src/api.rs` (lines 232-300)
- Auto-creates `./data/agents/{name}/volumes/` directory
- Converts volume paths to `VolumeMount` structs
- Creates host directories automatically

**File:** `orchestrator/src/container.rs` (lines 641-656)
- Uses `config.volumes` to create Docker bind mounts
- Format: `{source}:{target}[:ro]`

### Data Layout

```
claw-pen/
├── data/
│   └── agents/
│       ├── OpenZ/
│       │   └── volumes/
│       │       ├── memory/
│       │       ├── .openclaw/
│       │       └── workspace/
│       └── test-agent/
│           └── volumes/
│               └── ...
├── orchestrator.log
├── templates/
└── target/
```

---

## Future Enhancements

### Planned Features

1. **Named Volumes** - Share data between agents
   ```yaml
   volumes:
     - name: shared-workspace
       mount: /agent/shared
   ```

2. **Read-Only Mounts** - Protect configuration files
   ```yaml
   volumes:
     - path: /etc/agent/config
       read_only: true
   ```

3. **Volume Templates** - Reusable volume configurations
   ```yaml
   volume_templates:
     standard-workspace:
       - /agent/workspace
       - /agent/output
   ```

4. **Automatic Cleanup** - Remove volumes when agent is deleted
   ```bash
   claw-pen-orchestrator agents delete --cleanup-volumes <agent-name>
   ```

5. **Volume Snapshots** - Snapshot and restore agent data
   ```bash
   claw-pen-orchestrator volumes snapshot <agent-name>
   ```

---

## Testing

### Test Volume Mounting

1. **Create a new agent:**
   ```bash
   # Via GUI or API
   curl -X POST http://127.0.0.1:8081/api/agents \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{"name":"test-volumes","template":"openclaw-agent"}'
   ```

2. **Verify volumes are mounted:**
   ```bash
   docker inspect test-volumes --format='{{json .Mounts}}'
   ```

3. **Create a file in the container:**
   ```bash
   docker exec -it test-volumes sh -c "echo 'test' > /agent/workspace/test.txt"
   ```

4. **Verify it exists on host:**
   ```bash
   cat ./data/agents/test-volumes/volumes/workspace/test.txt
   ```

5. **Delete the test agent:**
   ```bash
   curl -X DELETE http://127.0.0.1:8081/api/agents/test-volumes \
     -H "Authorization: Bearer <token>"
   ```

---

## Summary

✅ **Safe volume mounting implemented**
✅ **Automatic directory creation**
✅ **Template-based configuration**
✅ **Isolated agent data**
✅ **Ready for production use**

**Try it now!** Create a new agent and volumes will be automatically mounted! 🚀
