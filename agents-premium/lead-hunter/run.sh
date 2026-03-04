#!/bin/bash
# Lead Hunter - Weekly Kijiji scraper for And or Design
# Runs via cron, sends results to WhatsApp and webchat

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="$SCRIPT_DIR/logs/cron.log"

# Ensure log directory exists
mkdir -p "$SCRIPT_DIR/logs"

# Log start
echo "[$(date -Iseconds)] Lead Hunter starting..." >> "$LOG_FILE"

# Run the scraper once (cron handles scheduling)
cd "$SCRIPT_DIR"
uv run python main.py --once 2>&1 | while read -r line; do
    echo "[$(date -Iseconds)] $line" >> "$LOG_FILE"
done

echo "[$(date -Iseconds)] Lead Hunter complete" >> "$LOG_FILE"
