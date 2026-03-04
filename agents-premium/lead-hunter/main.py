#!/usr/bin/env python3
"""
Lead Hunter Agent for And or Design
Finds clients looking for permit drawings in Northern Ontario.

Usage:
    python main.py              # Run once
    python main.py --schedule   # Run on schedule (daemon mode)
    python main.py --dry-run    # Test without sending notifications
"""

import argparse
import json
import logging
import os
import sys
import hashlib
from datetime import datetime, timedelta
from pathlib import Path
from typing import List, Dict, Set

import toml

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent))

from sources import scrape_kijiji, scrape_facebook, scrape_municipal, scrape_rss
from notifier import Notifier, Lead

# Configure logging
def setup_logging(config: dict):
    """Setup logging configuration."""
    log_config = config.get("logging", {})
    log_level = getattr(logging, log_config.get("level", "INFO"))
    log_file = log_config.get("file", "lead-hunter.log")
    
    # Create logs directory
    log_dir = Path(__file__).parent / "logs"
    log_dir.mkdir(exist_ok=True)
    log_path = log_dir / log_file
    
    logging.basicConfig(
        level=log_level,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        handlers=[
            logging.FileHandler(log_path),
            logging.StreamHandler(sys.stdout),
        ]
    )
    
    return logging.getLogger("lead-hunter")


class LeadMemory:
    """
    Manages lead storage and deduplication.
    Uses local JSON file for persistence (can be upgraded to shared memory).
    """
    
    def __init__(self, config: dict):
        self.config = config
        storage_config = config.get("storage", {})
        self.namespace = storage_config.get("namespace", "andor-design")
        self.retention_days = storage_config.get("dedup_retention_days", 30)
        
        # Memory file path
        self.memory_dir = Path(__file__).parent / "memory"
        self.memory_dir.mkdir(exist_ok=True)
        self.memory_file = self.memory_dir / "leads.json"
        
        # Load existing memory
        self.seen_urls: Set[str] = set()
        self.leads: Dict[str, dict] = {}
        self._load_memory()
    
    def _load_memory(self):
        """Load memory from disk."""
        if self.memory_file.exists():
            try:
                with open(self.memory_file, "r") as f:
                    data = json.load(f)
                    self.seen_urls = set(data.get("seen_urls", []))
                    self.leads = data.get("leads", {})
                    
                # Clean up old entries
                self._cleanup_old_leads()
                
            except Exception as e:
                logging.warning(f"Failed to load memory: {e}")
    
    def _save_memory(self):
        """Save memory to disk."""
        try:
            with open(self.memory_file, "w") as f:
                json.dump({
                    "seen_urls": list(self.seen_urls),
                    "leads": self.leads,
                    "updated": datetime.now().isoformat(),
                }, f, indent=2)
        except Exception as e:
            logging.error(f"Failed to save memory: {e}")
    
    def _cleanup_old_leads(self):
        """Remove leads older than retention period."""
        cutoff = datetime.now() - timedelta(days=self.retention_days)
        urls_to_remove = []
        
        for url, lead_data in self.leads.items():
            try:
                added = datetime.fromisoformat(lead_data.get("added", ""))
                if added < cutoff:
                    urls_to_remove.append(url)
            except:
                pass
        
        for url in urls_to_remove:
            self.seen_urls.discard(url)
            del self.leads[url]
        
        if urls_to_remove:
            logging.info(f"Cleaned up {len(urls_to_remove)} old leads from memory")
    
    def is_new(self, lead_data: dict) -> bool:
        """Check if a lead is new (not seen before)."""
        url = lead_data.get("url", "")
        return url not in self.seen_urls
    
    def add_lead(self, lead: Lead) -> bool:
        """Add a lead to memory. Returns True if new, False if duplicate."""
        if lead.url in self.seen_urls:
            return False
        
        self.seen_urls.add(lead.url)
        self.leads[lead.url] = {
            **lead.to_dict(),
            "added": datetime.now().isoformat(),
            "hash": self._hash_lead(lead),
        }
        self._save_memory()
        return True
    
    def _hash_lead(self, lead: Lead) -> str:
        """Generate a hash for the lead for deduplication."""
        content = f"{lead.title}|{lead.url}|{lead.location}"
        return hashlib.sha256(content.encode()).hexdigest()[:16]


class LeadHunter:
    """Main Lead Hunter agent."""

    def __init__(self, config_path: str = None, dry_run: bool = False):
        self.config = self._load_config(config_path)
        self.dry_run = dry_run
        self.logger = setup_logging(self.config)

        self.memory = LeadMemory(self.config)
        self.notifier = Notifier(self.config)

        # Load exclude keywords for filtering
        self.exclude_keywords = self.config.get("search", {}).get("exclude_keywords", [])
        self.exclude_service_keywords = self.config.get("search", {}).get("exclude_service_keywords", [])

        # Stats for this run
        self.stats = {
            "total_found": 0,
            "new_leads": 0,
            "duplicates": 0,
            "filtered_out": 0,
            "notified": 0,
            "errors": 0,
            "sources": [],
        }

    def _is_job_posting(self, lead_data: dict) -> bool:
        """Check if a lead looks like a job posting (hiring workers, not seeking services)."""
        title = lead_data.get("title", "").lower()
        description = lead_data.get("description", "").lower()
        combined = f"{title} {description}"

        for keyword in self.exclude_keywords:
            if keyword.lower() in combined:
                return True

        # Additional heuristics for job postings
        job_patterns = [
            r"now hiring",
            r"we are hiring",
            r"hiring \$\d+",
            r"\$\d+/hr",
            r"\$\d+ per hour",
            r"full[\s-]time",
            r"part[\s-]time",
            r"immediate start",
            r"apply now",
            r"send resume",
        ]

        import re
        for pattern in job_patterns:
            if re.search(pattern, combined, re.IGNORECASE):
                return True

        return False

    def _is_service_ad(self, lead_data: dict) -> bool:
        """Check if a lead is a service advertisement (not a homeowner seeking help)."""
        title = lead_data.get("title", "").lower()
        description = lead_data.get("description", "").lower()
        combined = f"{title} {description}"

        for keyword in self.exclude_service_keywords:
            if keyword.lower() in combined:
                return True

        # Additional heuristics for service ads
        import re
        service_patterns = [
            r"we (provide|offer|specialize)",
            r"call \d{3}[\s-]?\d{3}[\s-]?\d{4}",
            r"free (quote|estimate|consultation)",
            r"licensed\s*&\s*insured",
            r"fast\s*&\s*affordable",
            r"\d+\+?\s*years?\s*(of\s+)?experience",
        ]

        for pattern in service_patterns:
            if re.search(pattern, combined, re.IGNORECASE):
                return True

        return False
    
    def _load_config(self, config_path: str = None) -> dict:
        """Load configuration from TOML file."""
        if config_path is None:
            config_path = Path(__file__).parent / "agent.toml"
        
        try:
            with open(config_path, "r") as f:
                return toml.load(f)
        except Exception as e:
            logging.warning(f"Failed to load config from {config_path}: {e}")
            return {}
    
    def run(self):
        """Run the lead hunting process."""
        self.logger.info("=" * 50)
        self.logger.info("Lead Hunter starting...")
        self.logger.info(f"Dry run: {self.dry_run}")
        
        start_time = datetime.now()
        
        # Run all enabled scrapers
        self._run_rss()
        self._run_kijiji()
        self._run_facebook()  # Stub
        self._run_municipal()  # Stub
        
        # Send summary
        self.stats["duration"] = str(datetime.now() - start_time)
        self._log_summary()
        
        if not self.dry_run and self.stats["new_leads"] > 0:
            self.notifier.send_summary(self.stats)
        
        self.logger.info("Lead Hunter finished")
        return self.stats
    
    def _run_kijiji(self):
        """Run Kijiji scraper."""
        self.logger.info("Running Kijiji scraper...")
        self.stats["sources"].append("kijiji")

        try:
            for lead_data in scrape_kijiji(self.config):
                # Filter out job postings
                if self._is_job_posting(lead_data):
                    self.stats["filtered_out"] += 1
                    self.logger.debug(f"Filtered job posting: {lead_data.get('title', '')[:50]}...")
                    continue

                # Filter out service ads (businesses offering services)
                if self._is_service_ad(lead_data):
                    self.stats["filtered_out"] += 1
                    self.logger.debug(f"Filtered service ad: {lead_data.get('title', '')[:50]}...")
                    continue

                self.stats["total_found"] += 1

                lead = Lead(
                    title=lead_data.get("title", "Unknown"),
                    url=lead_data.get("url", ""),
                    location=lead_data.get("location", ""),
                    description=lead_data.get("description", ""),
                    budget=lead_data.get("budget"),
                    posted_time=lead_data.get("posted_time"),
                    source="kijiji",
                )

                if self.memory.add_lead(lead):
                    self.stats["new_leads"] += 1
                    self.logger.info(f"New lead: {lead.title[:50]}...")

                    if not self.dry_run:
                        if self.notifier.send_lead(lead):
                            self.stats["notified"] += 1
                else:
                    self.stats["duplicates"] += 1
                    self.logger.debug(f"Duplicate: {lead.title[:50]}...")

        except Exception as e:
            self.logger.error(f"Kijiji scraper error: {e}")
            self.stats["errors"] += 1

    def _run_rss(self):
        """Run RSS feed scraper (Google Alerts, etc.)."""
        rss_config = self.config.get("rss", {})
        feeds = rss_config.get("feeds", [])

        if not feeds:
            self.logger.info("No RSS feeds configured, skipping")
            return

        self.logger.info(f"Running RSS scraper ({len(feeds)} feeds)...")
        self.stats["sources"].append("rss")

        try:
            for lead_data in scrape_rss(self.config):
                # Filter out job postings
                if self._is_job_posting(lead_data):
                    self.stats["filtered_out"] += 1
                    self.logger.debug(f"Filtered job posting: {lead_data.get('title', '')[:50]}...")
                    continue

                # Filter out service ads
                if self._is_service_ad(lead_data):
                    self.stats["filtered_out"] += 1
                    self.logger.debug(f"Filtered service ad: {lead_data.get('title', '')[:50]}...")
                    continue

                self.stats["total_found"] += 1

                lead = Lead(
                    title=lead_data.get("title", "Unknown"),
                    url=lead_data.get("url", ""),
                    location=lead_data.get("location", ""),
                    description=lead_data.get("description", ""),
                    budget=lead_data.get("budget"),
                    posted_time=lead_data.get("posted_time"),
                    source=lead_data.get("source", "rss"),
                )

                if self.memory.add_lead(lead):
                    self.stats["new_leads"] += 1
                    self.logger.info(f"New lead: {lead.title[:50]}...")

                    if not self.dry_run:
                        if self.notifier.send_lead(lead):
                            self.stats["notified"] += 1
                else:
                    self.stats["duplicates"] += 1
                    self.logger.debug(f"Duplicate: {lead.title[:50]}...")

        except Exception as e:
            self.logger.error(f"RSS scraper error: {e}")
            self.stats["errors"] += 1
    
    def _run_facebook(self):
        """Run Facebook scraper (stub)."""
        self.logger.info("Facebook scraper not yet implemented")
        # Future: self.stats["sources"].append("facebook")
    
    def _run_municipal(self):
        """Run Municipal scraper (stub)."""
        self.logger.info("Municipal scraper not yet implemented")
        # Future: self.stats["sources"].append("municipal")
    
    def _log_summary(self):
        """Log a summary of this run."""
        self.logger.info("=" * 50)
        self.logger.info("LEAD HUNTER SUMMARY")
        self.logger.info("=" * 50)
        self.logger.info(f"Total leads found:    {self.stats['total_found']}")
        self.logger.info(f"Job postings filtered: {self.stats['filtered_out']}")
        self.logger.info(f"New leads:            {self.stats['new_leads']}")
        self.logger.info(f"Duplicates skipped:   {self.stats['duplicates']}")
        self.logger.info(f"Notifications sent:   {self.stats['notified']}")
        self.logger.info(f"Errors:               {self.stats['errors']}")
        self.logger.info(f"Sources:              {', '.join(self.stats['sources'])}")
        self.logger.info(f"Duration:             {self.stats.get('duration', 'N/A')}")
        self.logger.info("=" * 50)
    
    def run_scheduled(self):
        """Run in scheduled mode (daemon)."""
        import schedule
        import time
        
        cron = self.config.get("schedule", {}).get("cron", "0 6 * * *")
        
        # Parse cron (simplified - just handles "H M * * *" format)
        parts = cron.split()
        if len(parts) == 5:
            minute, hour = parts[0], parts[1]
            
            # Schedule daily at specified time
            schedule.every().day.at(f"{hour.zfill(2)}:{minute.zfill(2)}").do(self.run)
            
            self.logger.info(f"Scheduled to run daily at {hour}:{minute}")
            
            while True:
                schedule.run_pending()
                time.sleep(60)


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(description="Lead Hunter Agent")
    parser.add_argument("--config", "-c", help="Path to config file")
    parser.add_argument("--schedule", "-s", action="store_true", help="Run on schedule")
    parser.add_argument("--dry-run", "-n", action="store_true", help="Test without notifications")
    parser.add_argument("--once", action="store_true", help="Run once (default)")
    
    args = parser.parse_args()
    
    hunter = LeadHunter(config_path=args.config, dry_run=args.dry_run)
    
    if args.schedule:
        hunter.run_scheduled()
    else:
        stats = hunter.run()
        sys.exit(0 if stats["errors"] == 0 else 1)


if __name__ == "__main__":
    main()
