#!/bin/bash
set -e

# Set default port
export PORT=${PORT:-18789}

# Configure MCP server URL if provided
if [ -n "$REVITMCP_SERVER_URL" ]; then
    echo "Revit MCP Server: $REVITMCP_SERVER_URL"
fi

# Start OpenClaw gateway
echo "Starting Revit Agent on port $PORT..."
exec node /usr/local/lib/node_modules/openclaw/dist/cli.js gateway run --workspace /agent --port $PORT
