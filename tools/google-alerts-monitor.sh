#!/bin/bash
# Google Alerts Monitor - Daily check for leads
# Feed: JerRoy.75@gmail.com alert

FEED_URL="https://www.google.com/alerts/feeds/01729335861758971213/11892715663217727191"
STATE_FILE="$HOME/.openclaw/workspace/tools/.google-alerts-seen.txt"
LOCKFILE="/tmp/google-alerts-monitor.lock"

# Prevent concurrent runs
if [ -f "$LOCKFILE" ]; then
    pid=$(cat "$LOCKFILE")
    if ps -p "$pid" > /dev/null 2>&1; then
        exit 0
    fi
    rm -f "$LOCKFILE"
fi
echo $$ > "$LOCKFILE"
trap "rm -f $LOCKFILE" EXIT

# Fetch feed
FEED_CONTENT=$(curl -s "$FEED_URL")

if [ -z "$FEED_CONTENT" ]; then
    echo "Failed to fetch feed"
    exit 1
fi

# Extract entries (title, link, published date)
# Google Alert RSS format: <entry><title>, <link href>, <published>
ENTRIES=$(echo "$FEED_CONTENT" | grep -oP '(?<=<entry>).*?(?=</entry>)' 2>/dev/null || true)

if [ -z "$ENTRIES" ]; then
    # No entries in feed - that's fine
    touch "$STATE_FILE"
    exit 0
fi

# Process each entry
echo "$FEED_CONTENT" | grep -oP '(?<=<entry>).*?(?=</entry>)' | while read -r entry; do
    # Extract ID, title, link
    ENTRY_ID=$(echo "$entry" | grep -oP '(?<=<id>)[^<]+' | head -1)
    TITLE=$(echo "$entry" | grep -oP '(?<=<title>)[^<]+' | head -1)
    LINK=$(echo "$entry" | grep -oP '(?<=<link href=")[^"]+' | head -1)
    PUBLISHED=$(echo "$entry" | grep -oP '(?<=<published>)[^<]+' | head -1)
    
    # Skip if we've seen this entry
    if grep -qF "$ENTRY_ID" "$STATE_FILE" 2>/dev/null; then
        continue
    fi
    
    # New entry - format message
    MSG="🔍 **Lead Alert**\n\n$TITLE\n\n$LINK"
    
    if [ -n "$PUBLISHED" ]; then
        MSG="$MSG\n\n_Published: $PUBLISHED_"
    fi
    
    # Send to webchat (via openclaw message)
    openclaw message send --channel webchat --to "webchat" "$MSG" 2>/dev/null || true
    
    # Send to WhatsApp (if configured)
    openclaw message send --channel whatsapp --to "Jer" "$MSG" 2>/dev/null || true
    
    # Mark as seen
    echo "$ENTRY_ID" >> "$STATE_FILE"
done

exit 0
