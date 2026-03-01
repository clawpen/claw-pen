"""
RSS/Atom feed parser for Lead Hunter agent.
Supports Google Alerts and generic RSS feeds.
"""

import feedparser
import logging
import re
from typing import List, Generator
from datetime import datetime
from dateutil import parser as date_parser

logger = logging.getLogger(__name__)

# Default keywords to filter for construction/permit leads
DEFAULT_KEYWORDS = [
    "permit",
    "BCIN",
    "architect",
    "renovation",
    "basement apartment",
    "deck",
    "garage",
    "addition",
    "construction",
    "building permit",
    "home improvement",
    "contractor",
    "blueprint",
    "drawings",
]


class RSSScraper:
    """Parses RSS/Atom feeds for renovation and permit-related leads."""
    
    def __init__(self, config: dict):
        self.config = config
        self.rss_config = config.get("rss", {})
        self.feeds = self.rss_config.get("feeds", [])
        self.keywords = self.rss_config.get("keywords", config.get("search", {}).get("keywords", DEFAULT_KEYWORDS))
        self.max_entries_per_feed = self.rss_config.get("max_entries_per_feed", 50)
        
        # Compile keyword regex for filtering
        keyword_pattern = "|".join(re.escape(kw) for kw in self.keywords)
        self.keyword_regex = re.compile(keyword_pattern, re.IGNORECASE)
    
    def search(self) -> Generator[dict, None, None]:
        """
        Parse all RSS feeds and yield filtered entries.
        Each result is a dict representing a lead.
        """
        if not self.feeds:
            logger.warning("No RSS feeds configured in agent.toml [rss.feeds]")
            return
        
        for feed_url in self.feeds:
            # Skip commented/placeholder URLs
            feed_url = feed_url.strip()
            if not feed_url or feed_url.startswith("#"):
                continue
            
            logger.info(f"Parsing RSS feed: {feed_url}")
            
            try:
                entries = self._parse_feed(feed_url)
                for entry in entries:
                    if self._matches_keywords(entry):
                        yield self._format_lead(entry, feed_url)
                    else:
                        logger.debug(f"Entry doesn't match keywords: {entry.get('title', 'No title')}")
                        
            except Exception as e:
                logger.error(f"Error parsing feed {feed_url}: {e}")
                continue
    
    def _parse_feed(self, feed_url: str) -> List[dict]:
        """Parse a single RSS/Atom feed and return entries."""
        entries = []
        
        # feedparser handles both RSS and Atom formats automatically
        feed = feedparser.parse(feed_url)
        
        if feed.bozo and feed.bozo_exception:
            logger.warning(f"Feed parsing warning for {feed_url}: {feed.bozo_exception}")
        
        if not feed.entries:
            logger.warning(f"No entries found in feed: {feed_url}")
            return entries
        
        feed_title = feed.feed.get("title", "Unknown Feed")
        
        for entry in feed.entries[:self.max_entries_per_feed]:
            parsed_entry = {
                "title": entry.get("title", ""),
                "link": entry.get("link", ""),
                "summary": self._clean_summary(entry.get("summary", "")),
                "content": self._extract_content(entry),
                "published": self._parse_date(entry),
                "feed_title": feed_title,
                "feed_url": feed_url,
            }
            entries.append(parsed_entry)
        
        logger.info(f"Parsed {len(entries)} entries from {feed_title}")
        return entries
    
    def _clean_summary(self, summary: str) -> str:
        """Clean HTML and extra whitespace from summary."""
        if not summary:
            return ""
        
        # Remove HTML tags
        summary = re.sub(r'<[^>]+>', '', summary)
        # Collapse whitespace
        summary = re.sub(r'\s+', ' ', summary).strip()
        # Truncate if too long
        if len(summary) > 500:
            summary = summary[:497] + "..."
        
        return summary
    
    def _extract_content(self, entry) -> str:
        """Extract full content from entry if available."""
        content = ""
        
        # Try content field first (Atom)
        if hasattr(entry, "content") and entry.content:
            content = entry.content[0].get("value", "")
        # Try description (RSS)
        elif hasattr(entry, "description"):
            content = entry.description
        # Fallback to summary
        elif hasattr(entry, "summary"):
            content = entry.summary
        
        return self._clean_summary(content)
    
    def _parse_date(self, entry) -> str:
        """Parse and normalize publication date."""
        date_str = None
        
        # Try various date fields
        for field in ["published", "pubDate", "updated", "created"]:
            if hasattr(entry, field):
                date_str = getattr(entry, field)
                if date_str:
                    break
        
        if not date_str:
            return ""
        
        try:
            # dateutil parser handles most formats
            dt = date_parser.parse(date_str)
            return dt.isoformat()
        except Exception as e:
            logger.debug(f"Failed to parse date '{date_str}': {e}")
            return date_str
    
    def _matches_keywords(self, entry: dict) -> bool:
        """Check if entry matches any configured keywords."""
        # Search in title and summary
        searchable_text = f"{entry.get('title', '')} {entry.get('summary', '')}"
        
        return bool(self.keyword_regex.search(searchable_text))
    
    def _format_lead(self, entry: dict, feed_url: str) -> dict:
        """Format RSS entry as a lead dict."""
        # Extract potential location from title/summary
        location = self._extract_location(entry)
        
        return {
            "title": entry.get("title", "Untitled"),
            "url": entry.get("link", ""),
            "location": location,
            "description": entry.get("summary", ""),
            "content": entry.get("content", ""),
            "posted_time": entry.get("published", ""),
            "source": "rss",
            "feed_title": entry.get("feed_title", ""),
            "feed_url": feed_url,
        }
    
    def _extract_location(self, entry: dict) -> str:
        """Try to extract location from entry content."""
        # Common Ontario location patterns
        location_patterns = [
            r"\b(Sudbury|North Bay|Timmins|Sault Ste\.? Marie)\b",
            r"\b(Toronto|Ottawa|Hamilton|London|Kingston)\b",
            r"\b(Ontario|ON)\b",
        ]
        
        text = f"{entry.get('title', '')} {entry.get('summary', '')}"
        
        for pattern in location_patterns:
            match = re.search(pattern, text, re.IGNORECASE)
            if match:
                return match.group(1)
        
        return "Unknown"


def scrape_rss(config: dict) -> Generator[dict, None, None]:
    """Convenience function to scrape RSS feeds with config."""
    scraper = RSSScraper(config)
    yield from scraper.search()


if __name__ == "__main__":
    # Test the parser
    import json
    logging.basicConfig(level=logging.INFO)
    
    test_config = {
        "rss": {
            "feeds": [
                # Add a test feed URL here
            ],
            "max_entries_per_feed": 10,
        },
        "search": {
            "keywords": DEFAULT_KEYWORDS,
        }
    }
    
    for lead in scrape_rss(test_config):
        print(json.dumps(lead, indent=2))
