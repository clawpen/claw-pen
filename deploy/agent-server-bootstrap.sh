#!/bin/bash
#
# Agent Container Server Bootstrap
# Creates multiple OpenClaw agent containers with shared memory
#
# Usage:
#   ./agent-server-bootstrap.sh

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "${BLUE}[AgentServer]${NC} $1"; }
ok() { echo -e "${GREEN}[✓]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }
err() { echo -e "${RED}[✗]${NC} $1"; exit 1; }

# Config
AGENT_DATA_DIR="${AGENT_DATA_DIR:-/var/lib/agent-server}"
SHARED_MEMORY_DIR="$AGENT_DATA_DIR/shared-memory"
AGENTS_DIR="$AGENT_DATA_DIR/agents"
BASE_PORT="${BASE_PORT:-18789}"

echo ""
echo -e "${CYAN}╔══════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║        Agent Container Server Bootstrap              ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════╝${NC}"
echo ""

# Check for root
if [ "$EUID" -ne 0 ]; then
  err "Please run as root: sudo ./agent-server-bootstrap.sh"
fi

# --- Install Docker ---
log "Checking Docker..."
if ! command -v docker &> /dev/null; then
  log "Installing Docker..."
  curl -fsSL https://get.docker.com | sh
  systemctl enable docker
  systemctl start docker
  ok "Docker installed"
else
  ok "Docker already installed"
fi

# --- Install Tailscale ---
log "Checking Tailscale..."
if ! command -v tailscale &> /dev/null; then
  log "Installing Tailscale..."
  curl -fsSL https://tailscale.com/install.sh | sh
  ok "Tailscale installed"
  
  if [ -n "$TAILSCALE_AUTH_KEY" ]; then
    tailscale up --authkey=$TAILSCALE_AUTH_KEY --hostname=agent-server
    ok "Connected to Tailscale"
  else
    warn "Run 'tailscale up' to connect"
  fi
else
  ok "Tailscale already installed"
fi

# --- Create directories ---
log "Creating directories..."
mkdir -p "$SHARED_MEMORY_DIR"
mkdir -p "$AGENTS_DIR"
ok "Directories created"

# --- Create shared memory structure ---
log "Initializing shared memory..."
mkdir -p "$SHARED_MEMORY_DIR"/{sessions,embeddings,context}
cat > "$SHARED_MEMORY_DIR/config.json" << 'EOF'
{
  "version": "1.0",
  "created": "TIMESTAMP",
  "agents": []
}
EOF
sed -i "s/TIMESTAMP/$(date -Iseconds)/" "$SHARED_MEMORY_DIR/config.json"
ok "Shared memory initialized"

# --- Create agent management script ---
log "Creating agent management tool..."
cat > /usr/local/bin/agent-ctl << 'AGENTCTL'
#!/bin/bash
#
# agent-ctl - Manage OpenClaw agent containers
#
# Usage:
#   agent-ctl spawn <name> [--model model] [--port port]
#   agent-ctl list
#   agent-ctl logs <name>
#   agent-ctl kill <name>
#   agent-ctl exec <name> <command>

AGENT_DATA_DIR="${AGENT_DATA_DIR:-/var/lib/agent-server}"
SHARED_MEMORY_DIR="$AGENT_DATA_DIR/shared-memory"
AGENTS_DIR="$AGENT_DATA_DIR/agents"
BASE_PORT="${BASE_PORT:-18789}"

spawn_agent() {
  local name=$1
  local model=${2:-"zai/glm-5"}
  local port=${3:-""}
  
  # Auto-assign port
  if [ -z "$port" ]; then
    port=$(docker ps --filter "label=agent-server" --format '{{.Label "port"}}' 2>/dev/null | sort -n | tail -1)
    port=${port:-$BASE_PORT}
    port=$((port + 1))
  fi
  
  # Check if name exists
  if docker ps -a --filter "name=agent-$name" --format '{{.Names}}' | grep -q "agent-$name"; then
    echo "Error: Agent '$name' already exists"
    exit 1
  fi
  
  # Create agent directory
  mkdir -p "$AGENTS_DIR/$name"
  
  # Create container
  docker run -d \
    --name "agent-$name" \
    --label agent-server=true \
    --label "agent-name=$name" \
    --label "port=$port" \
    --network host \
    -v "$AGENTS_DIR/$name:/root/.openclaw" \
    -v "$SHARED_MEMORY_DIR:/shared-memory:ro" \
    -e "PORT=$port" \
    -e "OPENCLAW_MODEL=$model" \
    node:20-alpine \
    sh -c "npm install -g openclaw && openclaw gateway start --port $port"
  
  echo "✓ Agent '$name' spawned on port $port"
  echo "  URL: http://localhost:$port"
  
  # Update shared memory config
  local config="$SHARED_MEMORY_DIR/config.json"
  if [ -f "$config" ]; then
    # Add agent to config (simple append for now)
    echo "{\"name\":\"$name\",\"port\":$port,\"model\":\"$model\",\"created\":\"$(date -Iseconds)\"}" >> "$SHARED_MEMORY_DIR/agents.jsonl"
  fi
}

list_agents() {
  echo "Agent Containers:"
  echo "-----------------"
  docker ps --filter "label=agent-server" --format "table {{.Names}}\t{{.Status}}\t{{.Label \"port\"}}" 2>/dev/null || echo "No agents running"
}

logs_agent() {
  local name=$1
  docker logs "agent-$name" -f
}

kill_agent() {
  local name=$1
  docker rm -f "agent-$name"
  echo "✓ Agent '$name' killed"
}

exec_agent() {
  local name=$1
  shift
  docker exec "agent-$name" "$@"
}

case "$1" in
  spawn)
    shift
    spawn_agent "$@"
    ;;
  list)
    list_agents
    ;;
  logs)
    shift
    logs_agent "$@"
    ;;
  kill)
    shift
    kill_agent "$@"
    ;;
  exec)
    shift
    exec_agent "$@"
    ;;
  *)
    echo "Usage: agent-ctl {spawn|list|logs|kill|exec} ..."
    echo ""
    echo "Commands:"
    echo "  spawn <name> [--model model] [--port port]  Create new agent"
    echo "  list                                        List all agents"
    echo "  logs <name>                                 View agent logs"
    echo "  kill <name>                                 Remove agent"
    echo "  exec <name> <cmd>                           Run command in agent"
    exit 1
    ;;
esac
AGENTCTL

chmod +x /usr/local/bin/agent-ctl
ok "agent-ctl installed"

# --- Create OpenClaw Dockerfile for faster spawns ---
log "Creating agent image..."
cat > /tmp/Dockerfile.openclaw-agent << 'EOF'
FROM node:20-alpine

# Pre-install OpenClaw
RUN npm install -g openclaw

# Create directories
RUN mkdir -p /root/.openclaw /shared-memory

WORKDIR /root

# Default command
CMD ["openclaw", "gateway", "start"]
EOF

docker build -t openclaw-agent:latest /tmp/ 2>/dev/null || warn "Could not pre-build image, will build on first spawn"
ok "Agent image prepared"

# --- Spawn default agent (Codi) ---
log "Spawning default agent 'codi'..."
agent-ctl spawn codi --model zai/glm-5 --port 18789
sleep 5

# --- Summary ---
TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "not connected")
LOCAL_IP=$(hostname -I | awk '{print $1}')

echo ""
echo -e "${GREEN}╔══════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║       Agent Container Server Ready! 🧩              ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  ${CYAN}Agent Control:${NC}    agent-ctl"
echo ""
echo "  Commands:"
echo "    agent-ctl list              # List running agents"
echo "    agent-ctl spawn kimi        # Create new agent"
echo "    agent-ctl logs codi         # View agent logs"
echo "    agent-ctl kill codi         # Remove agent"
echo ""
echo -e "  ${CYAN}Default Agent:${NC}"
echo "    Name:   codi"
echo "    Port:   18789"
echo "    URL:    http://localhost:18789"
if [ "$TAILSCALE_IP" != "not connected" ]; then
  echo "    Remote: http://$TAILSCALE_IP:18789"
fi
echo ""
echo -e "  ${CYAN}Shared Memory:${NC}    $SHARED_MEMORY_DIR"
echo ""
