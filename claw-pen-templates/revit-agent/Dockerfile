# Revit Agent - OpenClaw with MCP Tools
# Designed for architectural modeling via Revit MCP

FROM node:22-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    wget \
    git \
    gnupg \
    ca-certificates \
    # Browser for web fetch
    chromium \
    fonts-liberation \
    # Dev tools
    jq \
    ripgrep \
    fd-find \
    # Process management
    procps \
    && rm -rf /var/lib/apt/lists/*

# Set Chromium path
ENV CHROME_BIN=/usr/bin/chromium
ENV PUPPETEER_SKIP_CHROMIUM_DOWNLOAD=true

# Copy OpenClaw (bundled from installed version)
COPY openclaw/ /usr/local/lib/node_modules/openclaw/

# Create directory structure
WORKDIR /agent
RUN mkdir -p /agent/memory \
    /agent/.openclaw/skills \
    /agent/.openclaw/workspace \
    /agent/projects

# Copy skills
COPY skills/ /agent/.openclaw/skills/

# Copy workspace files
COPY workspace/ /agent/

# Copy entrypoint
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Environment
ENV OPENCLAW_WORKSPACE=/agent
ENV NODE_ENV=production
ENV HOME=/root

# Healthcheck
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:${PORT:-18789}/health || exit 1

# Expose gateway port
EXPOSE 18789

# Start OpenClaw gateway
ENTRYPOINT ["/entrypoint.sh"]
