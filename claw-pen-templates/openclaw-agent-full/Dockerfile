# OpenClaw Agent - Full Edition
# Includes browser, skills, and all tools for maximum capability

FROM node:22-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    # Essentials
    curl \
    wget \
    git \
    gnupg \
    ca-certificates \
    # Browser dependencies
    chromium \
    chromium-sandbox \
    fonts-liberation \
    libappindicator3-1 \
    libasound2 \
    libatk-bridge2.0-0 \
    libatk1.0-0 \
    libcups2 \
    libdbus-1-3 \
    libdrm2 \
    libgbm1 \
    libgtk-3-0 \
    libnspr4 \
    libnss3 \
    libx11-xcb1 \
    libxcomposite1 \
    libxdamage1 \
    libxrandr2 \
    xdg-utils \
    # Development tools
    gh \
    jq \
    ripgrep \
    fd-find \
    # Archive tools
    zip \
    unzip \
    # Process management
    procps \
    htop \
    && rm -rf /var/lib/apt/lists/*

# Set Chromium path for Puppeteer/Playwright
ENV CHROME_BIN=/usr/bin/chromium
ENV CHROME_PATH=/usr/bin/chromium
ENV PUPPETEER_SKIP_CHROMIUM_DOWNLOAD=true
ENV PUPPETEER_EXECUTABLE_PATH=/usr/bin/chromium

# Copy OpenClaw from the build context (bundled from installed version)
COPY openclaw/ /usr/local/lib/node_modules/openclaw/

# Create directory structure
WORKDIR /agent
RUN mkdir -p /agent/memory \
    /agent/.openclaw/skills \
    /agent/.openclaw/workspace \
    /agent/.openclaw/canvas

# Copy default skills (if available)
COPY skills/ /agent/.openclaw/skills/

# Copy entrypoint
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Environment
ENV OPENCLAW_WORKSPACE=/agent
ENV NODE_ENV=production
ENV HOME=/root

# Healthcheck
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:${PORT:-18790}/health || exit 1

# Expose gateway port (default 18790)
EXPOSE 18790

# Start OpenClaw gateway
ENTRYPOINT ["/entrypoint.sh"]
