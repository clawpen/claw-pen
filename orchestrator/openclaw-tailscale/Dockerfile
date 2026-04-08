# OpenClaw Agent with Tailscale support
FROM openclaw-agent:latest

# Install Tailscale
RUN apt-get update && apt-get install -y curl ca-certificates && \
    curl -fsSL https://tailscale.com/install.sh | sh && \
    rm -rf /var/lib/apt/lists/*

# Create startup script in /usr/bin
RUN echo '#!/bin/sh\
echo "Starting Tailscale agent..."\
tailscaled --tun=userspace-networking --state=/var/lib/tailscale/tailscaled.state >/var/log/tailscaled.log 2>&1 &\
sleep 3\
if [ -n "$TAILSCALE_AUTH_KEY" ]; then\
  echo "Connecting to Tailscale..."\
  tailscale up --authkey="$TAILSCALE_AUTH_KEY" &\
fi\
sleep 5\
echo "Configuring OpenClaw to bind to 0.0.0.0..."\
mkdir -p /root/.openclaw\
cat > /root/.openclaw/openclaw.json <<JSONEOF\
{\
  "gateway": {\
    "port": ${PORT:-18790},\
    "mode": "local",\
    "bind": "0.0.0.0",\
    "auth": {"mode": "none"}\
  },\
  "agents": {\
    "defaults": {\
      "model": {\
        "primary": "${LLM_PROVIDER:-ollama}/${LLM_MODEL:-llama3}"\
      }\
    }\
  }\
}\
JSONEOF\
\
echo "Starting OpenClaw gateway..."\
cd /agent && exec node /usr/local/lib/node_modules/openclaw/dist/index.js "$@"' > /usr/local/bin/openclaw-tailscale-entry.sh && \
    chmod +x /usr/local/bin/openclaw-tailscale-entry.sh

ENTRYPOINT ["/usr/local/bin/openclaw-tailscale-entry.sh"]
