# Exo Container Runtime - Test Plan

> **Purpose:** Comprehensive testing strategy for Exo, an agent-first container runtime.
> 
> **Audience:** Developers, QA engineers, and CI/CD pipelines.
>
> **Related:** [README.md](README.md) | [PLAN.md](PLAN.md)

---

## Table of Contents

1. [Smoke Tests](#1-smoke-tests)
2. [Isolation Tests](#2-isolation-tests)
3. [Feature Tests](#3-feature-tests)
4. [Edge Cases](#4-edge-cases)
5. [Integration Tests](#5-integration-tests)
6. [Test Execution](#6-test-execution)

---

## 1. Smoke Tests

> **Priority:** P0 - Must pass before any release
>
> **Purpose:** Verify basic container lifecycle operations work.

### 1.1 Container Lifecycle - Run

**Test:** Run a simple container and verify it starts.

```bash
# Test command
exo run --name smoke-test-1 alpine:latest -- echo "Hello from Exo"

# Expected result
# Container starts, prints "Hello from Exo", exits with code 0
```

**How to verify:**
- Output contains "Hello from Exo"
- Exit code is 0
- No error messages in stderr

**Common failure modes:**
- Image not found / pull fails
- Namespace creation fails (permissions)
- Overlay mount fails
- Seccomp profile too restrictive

---

### 1.2 Container Lifecycle - Stop

**Test:** Stop a running container gracefully.

```bash
# Start a long-running container
exo run --name smoke-test-2 -d alpine:latest -- sleep 300

# Stop it
exo stop smoke-test-2

# Verify it's stopped
exo ps -a | grep smoke-test-2 | grep -q "Stopped\|Exited"
```

**How to verify:**
- `exo ps -a` shows container as stopped/exited
- Process no longer exists on host

**Common failure modes:**
- SIGTERM not delivered properly
- Container ignores shutdown signal
- Force kill required (--force)

---

### 1.3 Container Lifecycle - Remove

**Test:** Remove a stopped container.

```bash
# Remove the container
exo rm smoke-test-2

# Verify it's gone
exo ps -a | grep -c smoke-test-2 || echo "Container removed successfully"
```

**How to verify:**
- `exo ps -a` no longer lists the container
- Container state directory removed from `/var/lib/exo/containers/`

**Common failure modes:**
- Container still running (use --force)
- State files locked by another process
- Permission denied on state directory

---

### 1.4 Container Lifecycle - Auto-remove

**Test:** Container removes itself on exit with `--rm` flag.

```bash
exo run --rm --name smoke-test-3 alpine:latest -- echo "Auto-remove test"

# Container should not exist after exit
exo ps -a | grep -c smoke-test-3 || echo "Auto-removal successful"
```

**How to verify:**
- Container executes successfully
- `exo ps -a` shows no trace of the container

**Common failure modes:**
- Cleanup fails if container crashes
- State files orphaned on error paths

---

### 1.5 Listing Containers

**Test:** List running and all containers.

```bash
# Start a container in background
exo run --name smoke-test-4 -d alpine:latest -- sleep 60

# List running containers
exo ps

# List all containers (including stopped)
exo ps -a
```

**How to verify:**
- `exo ps` shows smoke-test-4 as running
- `exo ps -a` shows all containers including stopped ones
- Output includes: CONTAINER ID, IMAGE, COMMAND, STATUS, PORTS

**Common failure modes:**
- State directory not readable
- Container metadata corrupted
- PIDs stale (process died but state not updated)

---

### 1.6 Logs Output

**Test:** View container logs.

```bash
# Run container that produces output
exo run --name smoke-test-5 alpine:latest -- sh -c "echo 'Line 1'; echo 'Line 2'; echo 'Line 3'"

# View logs
exo logs smoke-test-5

# Follow logs (for running container)
exo run --name smoke-test-6 -d alpine:latest -- sh -c "for i in 1 2 3; do echo \"Log line \$i\"; sleep 1; done"
exo logs -f smoke-test-6
```

**How to verify:**
- `exo logs` outputs all three lines
- `exo logs -f` streams new output as it arrives
- `--tail N` limits output to last N lines
- `--timestamps` includes timestamps

**Common failure modes:**
- Stdout/stderr not captured properly
- Log rotation/truncation issues
- Encoding issues with binary output

---

### 1.7 Exec into Running Container

**Test:** Execute commands in a running container.

```bash
# Start a container
exo run --name smoke-test-7 -d alpine:latest -- sleep 300

# Execute a command
exo exec smoke-test-7 -- cat /etc/os-release

# Interactive exec
exo exec -it smoke-test-7 -- /bin/sh
```

**How to verify:**
- `exo exec` returns output from command
- Interactive mode provides a working shell
- Exit code reflects command result

**Common failure modes:**
- Container not running
- Namespace entry fails
- TTY allocation issues
- Command not found in container

---

## 2. Isolation Tests

> **Priority:** P0 - Security-critical
>
> **Purpose:** Verify containers are properly isolated from the host.

### 2.1 User Namespace Isolation (PID Isolation)

**Test:** Container cannot see host processes.

```bash
# Start container
exo run --name isolation-test-1 -d alpine:latest -- sleep 60

# From inside container, try to see host PIDs
exo exec isolation-test-1 -- ps aux

# Also verify /proc view
exo exec isolation-test-1 -- cat /proc/1/cmdline
```

**Expected result:**
- `ps aux` inside container shows only container processes
- PID 1 inside container is the container's init process
- Cannot see host's PID 1 (systemd/init)

**How to verify:**
```bash
# Host PID count
ps aux | wc -l

# Container PID count (should be much smaller)
exo exec isolation-test-1 -- ps aux | wc -l

# Verify different PID namespaces
ls -la /proc/self/ns/pid
exo exec isolation-test-1 -- ls -la /proc/self/ns/pid
```

**Common failure modes:**
- User namespace not created
- `/proc` mounted from host
- `setns()` fails to enter namespace

---

### 2.2 Network Isolation

**Test:** Container cannot reach host network services.

```bash
# Start a service on host (example with Python)
python3 -m http.server 9999 &
HOST_PID=$!

# Start container
exo run --name isolation-test-2 -d alpine:latest -- sleep 60

# Try to reach host service from container
exo exec isolation-test-2 -- wget -q -O- http://host.containo.internal:9999/ 2>&1 || echo "Blocked (expected)"

# Or try by IP (more direct)
HOST_IP=$(hostname -I | awk '{print $1}')
exo exec isolation-test-2 -- wget -q -O- http://${HOST_IP}:9999/ 2>&1 || echo "Blocked (expected)"

kill $HOST_PID
```

**Expected result:**
- Container cannot reach host services on bridge network
- Container has its own network namespace
- Only allowed port mappings work

**How to verify:**
```bash
# Check network namespace
exo exec isolation-test-2 -- ip addr

# Should see only loopback and container interface (eth0)
# Should NOT see host interfaces (wlan0, eth0 of host, etc.)

# Try ping to host
exo exec isolation-test-2 -- ping -c 1 ${HOST_IP} 2>&1 || echo "Ping blocked (expected)"
```

**Common failure modes:**
- Host network mode used (--network host)
- Bridge not properly isolated
- DNS leaks host information

---

### 2.3 Filesystem Isolation

**Test:** Container cannot access host files.

```bash
# Create a test file on host
echo "SECRET_HOST_DATA" > /tmp/host-secret-test

# Start container without volume mounts
exo run --name isolation-test-3 -d alpine:latest -- sleep 60

# Try to access host file
exo exec isolation-test-3 -- cat /tmp/host-secret-test 2>&1 || echo "Access denied (expected)"

# Try to access /etc/shadow (should fail)
exo exec isolation-test-3 -- cat /etc/shadow 2>&1 || echo "Access denied (expected)"
```

**Expected result:**
- `/tmp/host-secret-test` not visible inside container
- Container has its own root filesystem
- Host `/etc`, `/home`, etc. not accessible

**How to verify:**
```bash
# Compare mount points
mount | grep -E "^/" | head -5

exo exec isolation-test-3 -- mount | grep -E "^/" | head -5

# Should see different mounts
# Container should see overlay mounts, not host mounts
```

**Common failure modes:**
- Host paths leaked via environment
- `/proc` or `/sys` mounted from host
- Symbolic links escape container

---

### 2.4 Cgroup Limits - Memory

**Test:** Memory limit is actually enforced.

```bash
# Run container with 128MB memory limit
exo run --name isolation-test-4 --memory 128M -d alpine:latest -- sleep 60

# Try to allocate more than 128MB
exo exec isolation-test-4 -- sh -c "dd if=/dev/zero bs=1M count=200 | tail -c 1" 2>&1 || echo "OOM (expected)"
```

**Expected result:**
- Process killed by OOM when exceeding limit
- Memory usage never significantly exceeds limit

**How to verify:**
```bash
# Check cgroup memory settings
cat /sys/fs/cgroup/exo/isolation-test-4/memory.max

# Should show 134217728 (128MB in bytes)

# Monitor memory while running
exo exec isolation-test-4 -- cat /sys/fs/cgroup/memory.current
```

**Common failure modes:**
- Cgroup v2 not available
- Memory controller not enabled
- Limit set but not enforced (soft limit)

---

### 2.5 Cgroup Limits - CPU

**Test:** CPU limit is actually enforced.

```bash
# Run container with 0.5 CPU limit
exo run --name isolation-test-5 --cpu 0.5 -d alpine:latest -- sleep 60

# Stress CPU (run in background inside container)
exo exec isolation-test-5 -- sh -c "while true; do :; done &"

# Monitor CPU usage from host
CONTAINER_CGROUP=/sys/fs/cgroup/exo/isolation-test-5
cat $CONTAINER_CGROUP/cpu.max
```

**Expected result:**
- CPU usage capped at ~50% of one core
- Container cannot consume all host CPU

**How to verify:**
```bash
# Check cgroup CPU settings
cat /sys/fs/cgroup/exo/isolation-test-5/cpu.max

# Should show something like "50000 100000" (50% of 100ms period)
```

**Common failure modes:**
- CPU controller not enabled
- Quota/period calculation wrong
- Multi-core systems need special handling

---

## 3. Feature Tests

> **Priority:** P1 - Important functionality
>
> **Purpose:** Verify optional features work correctly.

### 3.1 GPU Passthrough - NVIDIA

**Test:** NVIDIA GPU accessible inside container.

```bash
# Check if NVIDIA GPU exists on host
nvidia-smi || echo "No NVIDIA GPU on host - SKIP"

# Run with GPU support
exo run --gpu --gpu-type nvidia --name gpu-test-1 nvidia/cuda:12.1.0-base-ubuntu22.04 -- nvidia-smi
```

**Expected result:**
- `nvidia-smi` works inside container
- GPU devices visible at `/dev/nvidia*`
- CUDA libraries available

**How to verify:**
```bash
# Inside container
exo exec gpu-test-1 -- nvidia-smi -L  # List GPUs
exo exec gpu-test-1 -- ls -la /dev/nvidia*
```

**Common failure modes:**
- NVIDIA Container Toolkit not installed
- Driver version mismatch
- Device nodes not created
- Library paths not set

---

### 3.2 GPU Passthrough - AMD

**Test:** AMD GPU accessible inside container.

```bash
# Check if AMD GPU exists
rocm-smi || echo "No AMD GPU on host - SKIP"

# Run with AMD GPU support
exo run --gpu --gpu-type amd --name gpu-test-2 rocm/pytorch:latest -- rocm-smi
```

**Expected result:**
- `rocm-smi` works inside container
- AMD GPU devices visible
- ROCm libraries available

**Common failure modes:**
- ROCm not installed
- `/dev/kfd` and `/dev/dri/` not accessible
- Wrong render node

---

### 3.3 Volume Mounts - Read/Write

**Test:** Volume mounts work correctly.

```bash
# Create test directory on host
mkdir -p /tmp/exo-volume-test
echo "host-data" > /tmp/exo-volume-test/host-file.txt

# Run container with volume mount
exo run --name volume-test-1 -v /tmp/exo-volume-test:/data -d alpine:latest -- sleep 60

# Read from mounted volume
exo exec volume-test-1 -- cat /data/host-file.txt

# Write to mounted volume
exo exec volume-test-1 -- sh -c "echo 'container-data' > /data/container-file.txt"

# Verify on host
cat /tmp/exo-volume-test/container-file.txt
```

**Expected result:**
- Host files readable inside container
- Container writes visible on host
- Permissions work correctly

**How to verify:**
```bash
# Check mount inside container
exo exec volume-test-1 -- mount | grep /data

# Verify bidirectional sync
echo "new-host-data" > /tmp/exo-volume-test/new-file.txt
exo exec volume-test-1 -- cat /data/new-file.txt
```

**Common failure modes:**
- Mount propagation issues
- Permission denied (user namespace mapping)
- SELinux/AppArmor blocking access
- Path doesn't exist on host

---

### 3.4 Volume Mounts - Read-Only

**Test:** Read-only mounts enforce no writes.

```bash
# Try to write to read-only mount (should fail)
exo run --name volume-test-2 -v /tmp/exo-volume-test:/data:ro alpine:latest -- sh -c "echo 'test' > /data/should-fail.txt" 2>&1 || echo "Write blocked (expected)"
```

**Common failure modes:**
- Read-only flag not respected
- Mount still writable via race condition

---

### 3.5 Environment Variables

**Test:** Environment variables passed correctly.

```bash
# Run with environment variables
exo run --name env-test-1 \
  -e MY_VAR=hello \
  -e ANOTHER_VAR=world \
  -e PATH=/custom/path:$PATH \
  alpine:latest -- sh -c 'echo "MY_VAR=$MY_VAR"; echo "ANOTHER_VAR=$ANOTHER_VAR"; echo "PATH=$PATH"'
```

**Expected result:**
- All specified env vars visible
- Default env vars still present (PATH, HOME, etc.)
- Multi-line values work correctly

**How to verify:**
```bash
# Check all environment
exo exec env-test-1 -- env | sort
```

**Common failure modes:**
- Special characters not escaped
- Empty values handled incorrectly
- Environment too large

---

### 3.6 Port Mapping

**Test:** Port mappings route traffic correctly.

```bash
# Run container with port mapping
exo run --name port-test-1 -d -p 8080:80 alpine:latest -- sh -c "while true; do echo 'Hello from container' | nc -l -p 80; done"

# Give it a moment to start
sleep 2

# Test from host (with timeout)
timeout 5 curl http://localhost:8080 || echo "Connection test completed"
```

**Expected result:**
- Traffic to host:8080 reaches container:80
- Multiple port mappings work
- TCP and UDP both work

**How to verify:**
```bash
# Check listening ports
ss -tlnp | grep 8080
```

**Common failure modes:**
- Port already in use on host
- iptables/nftables rules not created
- Firewall blocking traffic

---

### 3.7 Resource Limits (Combined)

**Test:** Multiple resource limits work together.

```bash
exo run --name resource-test-1 \
  --memory 256M \
  --cpu 1.5 \
  -d alpine:latest -- sleep 60

# Verify both limits
cat /sys/fs/cgroup/exo/resource-test-1/memory.max 2>/dev/null || echo "Cgroup path may vary"
cat /sys/fs/cgroup/exo/resource-test-1/cpu.max 2>/dev/null || echo "Cgroup path may vary"
```

---

## 4. Edge Cases

> **Priority:** P2 - Error handling robustness
>
> **Purpose:** Verify graceful handling of unusual situations.

### 4.1 Container Name Conflicts

**Test:** Cannot create container with duplicate name.

```bash
# Create first container
exo run --name conflict-test alpine:latest -- echo "first"

# Try to create second with same name
exo run --name conflict-test alpine:latest -- echo "second" 2>&1 || echo "Name conflict detected (expected)"

# Cleanup
exo rm conflict-test 2>/dev/null
```

**Expected result:**
- Second run fails with clear error message
- Original container unaffected

**Common failure modes:**
- Silently overwrites existing container
- UUID collision (very rare)
- Inconsistent state after error

---

### 4.2 Invalid Images

**Test:** Graceful handling of non-existent images.

```bash
# Non-existent image
exo run --name invalid-test-1 nonexistent/image:latest -- echo "test" 2>&1 || echo "Image not found (expected)"

# Invalid image format (if loading from file)
exo run --name invalid-test-2 /path/to/corrupted/image.tar -- echo "test" 2>&1 || echo "Invalid image (expected)"
```

**Expected result:**
- Clear error message about missing/invalid image
- No partial container state left behind

**Common failure modes:**
- Hangs trying to pull
- Leaves partial state
- Segfault on malformed image

---

### 4.3 Resource Exhaustion

**Test:** Handling when host resources exhausted.

```bash
# Create many containers until failure
for i in $(seq 1 50); do
  exo run --name exhaust-test-$i -d alpine:latest -- sleep 300 || {
    echo "Failed at container $i"
    break
  }
done

# Cleanup
for i in $(seq 1 50); do
  exo rm -f exhaust-test-$i 2>/dev/null
done
```

**Expected result:**
- Graceful failure with clear error message
- Existing containers continue running
- System remains stable

**Common failure modes:**
- System crash/freeze
- Other processes killed
- State corruption

---

### 4.4 Concurrent Container Operations

**Test:** Race conditions in concurrent operations.

```bash
# Create multiple containers simultaneously
for i in $(seq 1 10); do
  exo run --name concurrent-test-$i -d alpine:latest -- sleep 60 &
done
wait

# Verify all created
exo ps | grep concurrent-test | wc -l

# Cleanup
for i in $(seq 1 10); do
  exo rm -f concurrent-test-$i 2>/dev/null
done
```

**Expected result:**
- All containers created successfully
- No state corruption
- No deadlocks

**Common failure modes:**
- Race condition in state file writes
- Lock contention
- UUID collisions

---

### 4.5 Cleanup on Crash

**Test:** Resources cleaned up after container crash.

```bash
# Run container that will crash
exo run --name crash-test-1 alpine:latest -- sh -c "kill -9 $$" 2>&1

# Check for leftover state (paths may vary)
ls /var/lib/exo/containers/crash-test-1 2>&1 || echo "State cleaned up"
ls /sys/fs/cgroup/exo/crash-test-1 2>&1 || echo "Cgroup cleaned up"
```

**Expected result:**
- All resources released
- No zombie processes
- No orphaned state files

**Common failure modes:**
- Memory leak
- File descriptors leaked
- Cgroups not removed

---

## 5. Integration Tests

> **Priority:** P1 - Claw Pen specific
>
> **Purpose:** Verify Exo works correctly as part of the Claw Pen agent system.

### 5.1 Stdio Communication

**Test:** Stdin/stdout work correctly for agent communication.

```bash
# Full round-trip test
echo "PING" | exo run --rm -i alpine:latest -- sh -c "read msg; echo \"PONG: \$msg\""
```

**Expected result:**
- Stdin reaches container process
- Stdout captured correctly
- Pipes work bidirectionally

**Common failure modes:**
- Buffering issues
- PTY vs pipe differences
- Binary data corruption

---

### 5.2 Agent Channel Protocol

**Test:** Structured messages work via agent channel.

```bash
# Start container with JSON message handling
exo run --name channel-test-1 -d python:3.12-slim -- python -c "
import sys
import json

for line in sys.stdin:
    try:
        msg = json.loads(line)
        if msg.get('type') == 'tool_request':
            response = {
                'type': 'observation',
                'id': msg.get('id'),
                'content': f\"Executed: {msg.get('tool', 'unknown')}\"
            }
            print(json.dumps(response), flush=True)
    except:
        pass
"

# Send a test message
echo '{"type":"tool_request","id":"test-1","tool":"bash","args":{"cmd":"echo hello"}}' | exo exec -i channel-test-1 -- cat

# Cleanup
exo rm -f channel-test-1
```

**Expected result:**
- JSON messages parsed correctly
- Response follows protocol
- Message IDs preserved

**Common failure modes:**
- JSON encoding/decoding errors
- Message size limits exceeded
- Type mismatches

---

### 5.3 Process Spawning and Monitoring

**Test:** Tool process spawning works correctly.

```bash
# Start agent container
exo run --name process-test-1 -d python:3.12-slim -- sleep 300

# Spawn a subprocess via exec
exo exec process-test-1 -- python -c "
import subprocess
result = subprocess.run(['echo', 'subprocess test'], capture_output=True, text=True)
print(result.stdout)
"

# Verify process isolation
exo exec process-test-1 -- ps aux

# Cleanup
exo rm -f process-test-1
```

**Expected result:**
- Subprocesses run inside container
- Process tree visible
- Exit codes propagated

**Common failure modes:**
- Process not reaped (zombies)
- Signal handling issues
- Resource limits not applied to children

---

### 5.4 Tool Bus Integration

**Test:** Tool execution via tool bus works.

```bash
# This tests the tool bus implementation
# Run a container that implements the tool protocol
exo run --name toolbus-test-1 -d python:3.12-slim -- python -c "
import sys
import json
import subprocess

# Tool bus implementation
def execute_tool(tool, args):
    if tool == 'bash':
        result = subprocess.run(args.get('cmd', ''), shell=True, capture_output=True, text=True)
        return {'stdout': result.stdout, 'stderr': result.stderr, 'exit_code': result.returncode}
    return {'error': f'Unknown tool: {tool}'}

print('Tool bus ready', flush=True)

for line in sys.stdin:
    try:
        msg = json.loads(line)
        if msg.get('type') == 'tool_request':
            result = execute_tool(msg.get('tool'), msg.get('args', {}))
            response = {'type': 'observation', 'id': msg.get('id'), 'content': result}
            print(json.dumps(response), flush=True)
    except Exception as e:
        print(json.dumps({'type': 'error', 'message': str(e)}), flush=True)
"

# Verify it started
sleep 2
exo logs toolbus-test-1 | grep -q "Tool bus ready" && echo "Tool bus started successfully"

# Cleanup
exo rm -f toolbus-test-1
```

---

## 6. Test Execution

### 6.1 Prerequisites

```bash
# Check Exo is installed/built
exo --version || cargo build --release --manifest-path /home/codi/Desktop/software/exo/Cargo.toml

# Check kernel support
uname -r  # Should be 4.18+ for cgroup v2

# Check cgroup v2
mount | grep cgroup2

# Check user namespaces
cat /proc/sys/kernel/unprivileged_userns_clone 2>/dev/null || echo "Check kernel config for user namespaces"

# For GPU tests
nvidia-smi 2>/dev/null || echo "No NVIDIA GPU"
rocm-smi 2>/dev/null || echo "No AMD GPU"
```

### 6.2 Running Tests

```bash
# Run automated smoke tests
./test.sh smoke

# Run specific test category
./test.sh isolation
./test.sh features
./test.sh edge-cases
./test.sh integration

# Run all tests
./test.sh all

# Verbose output
./test.sh -v smoke
```

### 6.3 CI/CD Integration

```yaml
# Example GitHub Actions workflow
name: Exo Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      
      - name: Build Exo
        run: cargo build --release
        
      - name: Run Smoke Tests
        run: ./test.sh smoke
        
      - name: Run Isolation Tests
        run: ./test.sh isolation
        
      - name: Run Integration Tests
        run: ./test.sh integration
```

### 6.4 Test Report Format

```
EXO TEST REPORT
===============
Date: YYYY-MM-DD
Commit: abc123
Host: Ubuntu 22.04, Kernel 6.1

SMOKE TESTS (7/7 passed)
  [PASS] 1.1 Container Run
  [PASS] 1.2 Container Stop
  [PASS] 1.3 Container Remove
  [PASS] 1.4 Auto-remove
  [PASS] 1.5 List Containers
  [PASS] 1.6 Logs Output
  [PASS] 1.7 Exec

ISOLATION TESTS (5/5 passed)
  [PASS] 2.1 User Namespace Isolation
  [PASS] 2.2 Network Isolation
  [PASS] 2.3 Filesystem Isolation
  [PASS] 2.4 Memory Limits
  [PASS] 2.5 CPU Limits

FEATURE TESTS (6/8 passed, 2 skipped)
  [SKIP] 3.1 NVIDIA GPU - No GPU
  [SKIP] 3.2 AMD GPU - No GPU
  [PASS] 3.3 Volume Mounts R/W
  [PASS] 3.4 Volume Mounts Read-Only
  [PASS] 3.5 Environment Variables
  [PASS] 3.6 Port Mapping
  [PASS] 3.7 Resource Limits

EDGE CASES (5/5 passed)
  [PASS] 4.1 Name Conflicts
  [PASS] 4.2 Invalid Images
  [PASS] 4.3 Resource Exhaustion
  [PASS] 4.4 Concurrent Operations
  [PASS] 4.5 Cleanup on Crash

INTEGRATION TESTS (4/4 passed)
  [PASS] 5.1 Stdio Communication
  [PASS] 5.2 Agent Channel Protocol
  [PASS] 5.3 Process Spawning
  [PASS] 5.4 Tool Bus Integration

SUMMARY: 27/27 tests passed (100%)
         2 tests skipped (no GPU available)
```

---

## Appendix A: Test Data

### Sample Container Config

```toml
# test-container.toml
[container]
name = "test-agent"
image = "python:3.12-slim"

[container.resources]
memory = "512M"
cpu = "1"

[container.runtime]
workdir = "/app"
env = [
    "TEST_MODE=true",
    "LOG_LEVEL=debug"
]

[container.network]
mode = "bridge"
port_mappings = [
    { host_port = 9000, container_port = 8000 }
]

[process]
command = ["python", "-m", "agent"]
```

### Sample Test Images

```bash
# Minimal test images
alpine:latest           # Basic shell
python:3.12-slim        # Python runtime
node:20-slim            # Node.js runtime
nvidia/cuda:12.1-base   # NVIDIA GPU
rocm/pytorch:latest     # AMD GPU
```

---

## Appendix B: Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "Permission denied" | User namespace not available | Check kernel config |
| "No space left on device" | Overlay storage full | Clean up old images |
| "Container not found" | State corrupted | Check /var/lib/exo/containers |
| "Cgroup creation failed" | Cgroup v2 not mounted | Mount cgroup2 filesystem |
| "Network namespace failed" | Missing netlink | Install libnetlink |

### Debug Commands

```bash
# Check container state
ls -la /var/lib/exo/containers/ 2>/dev/null || ls -la /var/lib/containment/containers/

# Check cgroups
ls -la /sys/fs/cgroup/exo/ 2>/dev/null || ls -la /sys/fs/cgroup/containment/

# Check namespaces
ls -la /proc/<pid>/ns/

# Check overlay mounts
mount | grep overlay

# Check network
ip netns list
```

---

*Document Version: 1.0*
*Last Updated: 2024*
*Author: Exo Development Team*
