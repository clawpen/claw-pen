# Safe Code Assistant

A **network-isolated** code analysis agent that guarantees your code never leaves your machine.

## Security Model

This agent is designed for maximum security and data privacy:

### Network Isolation
- **No network access** - Container runs with `--network none`
- Verified at startup - agent checks for network connectivity and fails if detected
- No DNS, no outbound connections, no data exfiltration possible

### Filesystem Isolation
- **Read-only mounts** by default - agent cannot modify your code
- **Explicit mount points** - you control exactly what the agent can see
- **Path validation** - agent verifies all paths stay within workspace
- **No default mounts** - user must explicitly mount directories

### Secret Management
- **No hardcoded secrets** in the image
- **Runtime injection only** - secrets provided at container creation
- **Environment warning** - agent warns if dangerous env vars are detected

### Data Sovereignty
- **All processing local** - uses local LLM (Ollama)
- **No telemetry** - no usage data sent anywhere
- **No cloud dependencies** - everything runs on your machine

## Quick Start

### Prerequisites
- Docker installed
- Ollama running locally with llama3.2 model
- Claw Pen CLI installed

### Basic Usage

```bash
# Create an agent with read-only access to your code
claw-pen create safe-code-assistant \
  --mount ~/my-project:/workspace:ro \
  --network none \
  --provider local

# The agent can ONLY:
# - Read files in /workspace
# - Use the local LLM
# - Return analysis results

# It CANNOT:
# - Access the internet
# - Send data anywhere
# - Modify your files (ro mount)
# - See anything outside /workspace
```

### With Write Access

If you want the agent to be able to create files (e.g., generate code):

```bash
claw-pen create safe-code-assistant \
  --mount ~/my-project:/workspace:rw \
  --network none \
  --provider local
```

## Verification Steps

### 1. Verify Network Isolation

```bash
# Check the container has no network
docker inspect <container-id> | grep NetworkMode
# Should show: "NetworkMode": "none"

# Or verify from inside the container
docker exec <container-id> python -c "
import socket
try:
    socket.create_connection(('8.8.8.8', 53), timeout=1)
    print('FAIL: Network access detected')
except:
    print('OK: No network access')
"
```

### 2. Verify Filesystem Isolation

```bash
# Check what's mounted
docker inspect <container-id> --format='{{json .Mounts}}' | jq

# Should only show your explicitly mounted directories
# No access to host filesystem outside mounts
```

### 3. Verify No Secrets

```bash
# Check environment variables
docker exec <container-id> env | grep -E '(AWS_|GITHUB_TOKEN|API_KEY)'
# Should be empty (or only what you explicitly injected)
```

### 4. Verify Attestation

```bash
# Check template attestation
claw-pen verify safe-code-assistant

# Should show:
# - network_free: true
# - secrets_free: true
# - sha256 hash matches
```

## What Data Stays Local

### Everything

This agent is designed so that **nothing can leave your machine**:

| Component | Location | Network Required? |
|-----------|----------|-------------------|
| Your code | Mounted from your filesystem | No |
| LLM inference | Local Ollama instance | No |
| Code indexing | SQLite database in container | No |
| Semantic search | Local vector store | No |
| Analysis results | Returned to CLI, stored locally | No |

### Data Flow

```
Your Code (~/my-project)
    ↓ (read-only mount)
Container (/workspace)
    ↓ (analysis)
Local LLM (Ollama)
    ↓ (results)
Your Terminal
```

At no point does data leave your machine.

## Architecture

```
┌─────────────────────────────────────────┐
│         Claw Pen Orchestrator           │
│         (your local machine)            │
└────────────────┬────────────────────────┘
                 │
                 │ claw-pen create
                 ↓
┌─────────────────────────────────────────┐
│      Docker Container (network: none)   │
│  ┌────────────────────────────────────┐ │
│  │   Safe Code Assistant              │ │
│  │   - analyze_file()                 │ │
│  │   - search()                       │ │
│  │   - _verify_security()             │ │
│  └────────────────────────────────────┘ │
│  ┌────────────────────────────────────┐ │
│  │   /workspace (mounted ro/rw)       │ │
│  │   Your code lives here             │ │
│  └────────────────────────────────────┘ │
└─────────────────────────────────────────┘
                 │
                 │ Ollama API (local socket)
                 ↓
┌─────────────────────────────────────────┐
│         Ollama (llama3.2)               │
│         localhost:11434                 │
└─────────────────────────────────────────┘
```

## Security Guarantees

### What This Agent CAN Do
- ✅ Read files in mounted workspace
- ✅ Analyze code using local LLM
- ✅ Perform semantic search on indexed code
- ✅ Return results to you
- ✅ Write to workspace (if mounted rw)

### What This Agent CANNOT Do
- ❌ Access the internet
- ❌ Make DNS queries
- ❌ Send data to external servers
- ❌ Read files outside workspace
- ❌ Access your SSH keys, AWS credentials, etc.
- ❌ Modify files (if mounted ro)
- ❌ Install new packages at runtime
- ❌ Spawn processes outside container

### Defense in Depth

1. **Container isolation** - Docker provides process/filesystem isolation
2. **Network none** - No network stack at all
3. **Read-only mounts** - Cannot modify source code (default)
4. **Non-root user** - Runs as unprivileged 'agent' user
5. **Startup verification** - Agent self-checks for network access
6. **Path validation** - All file access validated against workspace
7. **Explicit mounts** - Nothing mounted by default

## Advanced Usage

### Mounting Multiple Directories

```bash
claw-pen create safe-code-assistant \
  --mount ~/my-project:/workspace:ro \
  --mount ~/.config/my-tool:/config:ro \
  --network none \
  --provider local
```

### With Secrets Injection

```bash
claw-pen create safe-code-assistant \
  --mount ~/my-project:/workspace:ro \
  --secret MY_API_KEY=secret_value \
  --network none \
  --provider local
```

### Custom Model

```bash
claw-pen create safe-code-assistant \
  --mount ~/my-project:/workspace:ro \
  --network none \
  --provider local \
  --model codellama
```

## Troubleshooting

### "Network access detected" error

This means the container has network access. Ensure you're using:
```bash
--network none
```

### "Access denied - path outside workspace"

The agent tried to access a file outside the mounted workspace. This is a security feature. Ensure:
- Paths are relative to workspace
- Symlinks don't point outside workspace

### "File not found"

The requested file doesn't exist in the workspace. Check:
- File path is correct
- Mount point includes the file
- File permissions allow reading

## Building from Source

```bash
cd /data/claw-pen/templates/safe-code-assistant
docker build -t claw-pen/safe-code-assistant:latest .
```

## Contributing

When contributing to this template, ensure:
- No network calls in code
- No hardcoded secrets
- All file access goes through workspace validation
- Security checks remain in place

## License

MIT License - See LICENSE file for details
