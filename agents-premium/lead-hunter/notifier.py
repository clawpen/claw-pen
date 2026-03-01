"""
Notifier module for Lead Hunter agent.
Sends lead notifications via WhatsApp through OpenClaw message tool.
"""

import subprocess
import json
import logging
from typing import Optional
from dataclasses import dataclass

logger = logging.getLogger(__name__)


@dataclass
class Lead:
    """Represents a discovered lead."""
    title: str
    url: str
    location: str
    description: str
    posted_time: Optional[str] = None
    budget: Optional[str] = None
    source: str = "kijiji"
    
    def to_notification(self) -> str:
        """Format lead as WhatsApp notification message."""
        lines = [
            f"🏠 New Lead: {self.title}",
            f"📍 Location: {self.location}",
            f"🔗 Link: {self.url}",
        ]
        
        if self.budget:
            lines.append(f"💰 Budget: {self.budget}")
        
        # Truncate description to ~200 chars for readability
        desc = self.description[:200] + "..." if len(self.description) > 200 else self.description
        lines.append(f"📝 Description: {desc}")
        
        if self.posted_time:
            lines.append(f"⏰ Posted: {self.posted_time}")
        
        lines.append(f"🔍 Source: {self.source}")
        
        return "\n".join(lines)
    
    def to_dict(self) -> dict:
        """Convert to dictionary for storage."""
        return {
            "title": self.title,
            "url": self.url,
            "location": self.location,
            "description": self.description,
            "posted_time": self.posted_time,
            "budget": self.budget,
            "source": self.source,
        }
    
    @classmethod
    def from_dict(cls, data: dict) -> "Lead":
        """Create Lead from dictionary."""
        return cls(
            title=data["title"],
            url=data["url"],
            location=data["location"],
            description=data["description"],
            posted_time=data.get("posted_time"),
            budget=data.get("budget"),
            source=data.get("source", "kijiji"),
        )


class Notifier:
    """Handles sending lead notifications."""
    
    def __init__(self, config: dict):
        self.config = config
        self.notifications_config = config.get("notifications", {})
        self.whatsapp_enabled = self.notifications_config.get("whatsapp", False)
        self.channel = self.notifications_config.get("channel", "webchat")
        self.recipient = self.notifications_config.get("recipient", "Jer")
    
    def send_lead(self, lead: Lead) -> bool:
        """Send a lead notification via configured channel."""
        if not self.whatsapp_enabled:
            logger.info(f"WhatsApp notifications disabled, skipping: {lead.title}")
            return False
        
        message = lead.to_notification()
        
        # Use OpenClaw's message tool via subprocess
        # In production, this would be called directly from main agent
        try:
            # For OpenClaw agent context, we write to a notification queue
            # that the main agent will pick up
            self._queue_notification(message)
            logger.info(f"Queued notification for lead: {lead.title}")
            return True
        except Exception as e:
            logger.error(f"Failed to send notification: {e}")
            return False
    
    def _queue_notification(self, message: str):
        """Queue notification for delivery by main agent context."""
        # In OpenClaw context, this would use the message tool directly
        # For standalone operation, write to a notification file
        import os
        from datetime import datetime
        
        queue_dir = os.path.join(os.path.dirname(__file__), "notifications")
        os.makedirs(queue_dir, exist_ok=True)
        
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        queue_file = os.path.join(queue_dir, f"pending_{timestamp}.json")
        
        with open(queue_file, "w") as f:
            json.dump({
                "channel": self.channel,
                "recipient": self.recipient,
                "message": message,
                "timestamp": timestamp,
            }, f, indent=2)
    
    def send_summary(self, stats: dict):
        """Send a summary of the lead hunting session."""
        message = (
            f"📊 Lead Hunter Summary\n"
            f"━━━━━━━━━━━━━━━━━━━━\n"
            f"🔍 Total found: {stats.get('total_found', 0)}\n"
            f"✨ New leads: {stats.get('new_leads', 0)}\n"
            f"♻️ Duplicates: {stats.get('duplicates', 0)}\n"
            f"📤 Notifications sent: {stats.get('notified', 0)}\n"
            f"📁 Sources: {', '.join(stats.get('sources', []))}"
        )
        self._queue_notification(message)


def notify_via_openclaw(message: str, channel: str = "webchat", target: str = None):
    """
    Send notification using OpenClaw's message tool.
    This function is called when running within OpenClaw agent context.
    """
    # This would be implemented to call the message tool
    # For now, it's a placeholder that works with the queue system
    pass
