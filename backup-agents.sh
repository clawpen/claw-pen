#!/bin/bash
# Backup OpenClaw agents and configs

BACKUP_DIR=~/.openclaw/backups
DATE=$(date +%Y-%m-%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/openclaw-backup-$DATE.tar.gz"

mkdir -p $BACKUP_DIR

# Backup agents
tar -czf $BACKUP_FILE \
  -C ~/.openclaw \
  agents/ \
  workspace/agents.json \
  workspace/IDENTITY.md \
  workspace/USER.md \
  workspace/SOUL.md \
  workspace/MEMORY.md \
  openclaw.json \
  2>/dev/null

echo "Backup created: $BACKUP_FILE"
echo "Size: $(du -h $BACKUP_FILE | cut -f1)"

# Keep last 10 backups
ls -t $BACKUP_DIR/*.tar.gz | tail -n +11 | xargs rm -f 2>/dev/null
echo "Kept last 10 backups"
