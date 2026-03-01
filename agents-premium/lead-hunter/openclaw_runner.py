#!/usr/bin/env python3
"""
OpenClaw Runner for Lead Hunter Agent

This script runs the lead hunter agent and sends notifications
via OpenClaw's message tool.

Usage in OpenClaw agent context:
    python openclaw_runner.py

Or call from main agent:
    # In heartbeat or cron
    subprocess.run(["python", "/data/claw-pen/agents/lead-hunter/openclaw_runner.py"])
"""

import json
import os
import sys
import subprocess
from pathlib import Path
from datetime import datetime

# Add agent directory to path
sys.path.insert(0, str(Path(__file__).parent))


def run_lead_hunter():
    """Run the lead hunter and process notifications."""
    from main import LeadHunter
    
    print(f"[{datetime.now().isoformat()}] Starting Lead Hunter...")
    
    hunter = LeadHunter()
    stats = hunter.run()
    
    # Process pending notifications
    notifications_dir = Path(__file__).parent / "notifications"
    if notifications_dir.exists():
        process_notifications(notifications_dir)
    
    return stats


def process_notifications(notifications_dir: Path):
    """
    Process pending notifications and send via OpenClaw message tool.
    
    This function reads notification files and outputs them in a format
    that OpenClaw can pick up and send via the message tool.
    """
    pending_files = sorted(notifications_dir.glob("pending_*.json"))
    
    if not pending_files:
        return
    
    print(f"Processing {len(pending_files)} pending notifications...")
    
    for notif_file in pending_files:
        try:
            with open(notif_file, "r") as f:
                notif = json.load(f)
            
            # Output notification for OpenClaw to pick up
            # This will be read by the main agent context
            output_notification(notif)
            
            # Archive the notification
            archive_dir = notifications_dir / "sent"
            archive_dir.mkdir(exist_ok=True)
            notif_file.rename(archive_dir / notif_file.name)
            
        except Exception as e:
            print(f"Error processing {notif_file}: {e}")


def output_notification(notif: dict):
    """
    Output notification in format for OpenClaw to send.
    
    The main agent context will read this and use the message tool.
    """
    channel = notif.get("channel", "webchat")
    recipient = notif.get("recipient", "")
    message = notif.get("message", "")
    
    # Output as JSON for easy parsing
    output = json.dumps({
        "action": "send",
        "channel": channel,
        "target": recipient,
        "message": message,
    })
    
    # Print to stdout - main agent will capture this
    print(f"NOTIFICATION: {output}")


def main():
    """Main entry point for OpenClaw runner."""
    try:
        stats = run_lead_hunter()
        
        # Output summary for OpenClaw to capture
        summary = {
            "status": "success",
            "stats": stats,
            "timestamp": datetime.now().isoformat(),
        }
        print(f"SUMMARY: {json.dumps(summary)}")
        
        return 0
        
    except Exception as e:
        print(f"ERROR: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
