#!/usr/bin/env python3
"""
Chat Transcript Exporter
Exports OpenClaw chat sessions to markdown files with timestamps.
Runs periodically via cron to capture conversation history.
"""

import json
import os
from datetime import datetime
from pathlib import Path

# Paths
SESSIONS_DIR = Path.home() / ".openclaw" / "agents" / "main" / "sessions"
OUTPUT_DIR = Path.home() / ".openclaw" / "workspace" / "memory" / "chat"
STATE_FILE = OUTPUT_DIR / ".export-state.json"

def load_state():
    """Load last exported positions for each session."""
    if STATE_FILE.exists():
        return json.loads(STATE_FILE.read_text())
    return {}

def save_state(state):
    """Save export state."""
    STATE_FILE.write_text(json.dumps(state, indent=2))

def get_active_sessions():
    """Get list of active sessions from sessions.json."""
    sessions_file = SESSIONS_DIR / "sessions.json"
    if not sessions_file.exists():
        return []
    
    data = json.loads(sessions_file.read_text())
    sessions = []
    
    for key, session in data.items():
        if "sessionId" in session and "sessionFile" in session:
            sessions.append({
                "key": key,
                "sessionId": session["sessionId"],
                "sessionFile": session["sessionFile"],
                "channel": session.get("lastChannel", "unknown"),
                "chatType": session.get("chatType", "unknown"),
            })
    
    return sessions

def format_message(msg):
    """Extract readable content from a message."""
    role = msg.get("role", "unknown")
    
    # Skip tool calls and results
    if role in ("toolCall", "toolResult"):
        return None
    
    content = msg.get("content", [])
    text_parts = []
    
    if isinstance(content, str):
        text_parts.append(content)
    elif isinstance(content, list):
        for part in content:
            if isinstance(part, dict):
                if part.get("type") == "text":
                    text_parts.append(part.get("text", ""))
                elif "text" in part:
                    text_parts.append(part["text"])
            elif isinstance(part, str):
                text_parts.append(part)
    
    if not text_parts:
        return None
    
    return {
        "role": role,
        "text": "\n".join(text_parts),
    }

def export_session(session, state):
    """Export new messages from a session."""
    session_file = Path(session["sessionFile"])
    if not session_file.exists():
        return []
    
    session_id = session["sessionId"]
    last_line = state.get(session_id, 0)
    
    # Read new lines
    lines = session_file.read_text().strip().split("\n")
    new_lines = lines[last_line:]
    
    if not new_lines:
        return []
    
    messages = []
    for line in new_lines:
        if not line.strip():
            continue
        try:
            entry = json.loads(line)
            if entry.get("type") == "message":
                msg = entry.get("message", {})
                formatted = format_message(msg)
                if formatted:
                    timestamp = entry.get("timestamp", "")
                    messages.append({
                        **formatted,
                        "timestamp": timestamp,
                    })
        except json.JSONDecodeError:
            continue
    
    # Update state
    state[session_id] = len(lines)
    
    return messages

def write_transcript(messages, session, output_date):
    """Write messages to daily transcript file."""
    if not messages:
        return
    
    output_file = OUTPUT_DIR / f"{output_date}.md"
    
    # Create header if new file
    if not output_file.exists():
        header = f"# Chat Transcript - {output_date}\n\n"
        output_file.write_text(header)
    
    # Format session info
    session_label = session.get("key", session["sessionId"])
    channel = session.get("channel", "unknown")
    
    # Append messages
    with open(output_file, "a") as f:
        f.write(f"\n---\n\n## Session: {session_label} ({channel})\n\n")
        
        for msg in messages:
            ts = msg.get("timestamp", "")
            if ts:
                try:
                    dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
                    time_str = dt.strftime("%H:%M:%S")
                except:
                    time_str = ts
            else:
                time_str = "??"
            
            role = msg["role"]
            text = msg["text"]
            
            # Format based on role
            if role == "user":
                f.write(f"### [{time_str}] Jer\n\n{text}\n\n")
            elif role == "assistant":
                f.write(f"### [{time_str}] Codi\n\n{text}\n\n")
            else:
                f.write(f"### [{time_str}] {role}\n\n{text}\n\n")
    
    return output_file

def main():
    """Main export function."""
    # Ensure output directory exists
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    
    # Load state
    state = load_state()
    
    # Get active sessions
    sessions = get_active_sessions()
    
    # Current date for output file
    output_date = datetime.now().strftime("%Y-%m-%d")
    
    exported = []
    for session in sessions:
        messages = export_session(session, state)
        if messages:
            output_file = write_transcript(messages, session, output_date)
            exported.append({
                "session": session["key"],
                "messages": len(messages),
                "file": str(output_file),
            })
    
    # Save state
    save_state(state)
    
    # Log results
    if exported:
        print(f"[{datetime.now().isoformat()}] Exported:")
        for e in exported:
            print(f"  - {e['session']}: {e['messages']} messages -> {e['file']}")
    else:
        print(f"[{datetime.now().isoformat()}] No new messages to export")
    
    return exported

if __name__ == "__main__":
    main()
