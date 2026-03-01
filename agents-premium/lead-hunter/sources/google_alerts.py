"""
Google Alerts RSS feed source for Lead Hunter agent.
Fetches leads from configured Google Alerts RSS feeds.
"""

import feedparser
import requests
import logging
from typing import List, Generator
from datetime import datetime
from urllib.parse import quote_plus

logger = logging.getLogger(__name__)


class GoogleAlertsSource:
    """Fetch leads from Google Alerts RSS feeds."""
    
    def __init__(self, config: dict):
        self.config = config
        alerts_config = config.get("google_alerts", {})
        self.feeds = alerts_config.get("feeds", [])
        self.enabled = alerts_config.get("enabled", True)
        
        # Setup session with headers
        self.session = requests.Session()
        self.session.headers.update({
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            "Accept": "application/rss+xml, application/xml, text/xml, */*",
        })
    
    def search(self, keywords: List[str] = None, locations: List[str] = None) -> Generator[dict, None, None]:
        """
        Fetch leads from configured Google Alerts RSS feeds.
        
        Args:
            keywords: Not used for Google Alerts (alerts are pre-configured)
            locations: Not used for Google Alerts (alerts are pre-configured)
        
        Yields:
            dict: Lead data from RSS feed entries
        """
        if not self.enabled:
            logger.info("Google Alerts source is disabled")
            return
        
        if not self.feeds:
            logger.warning("No Google Alerts feeds configured")
            return
        
        seen_urls = set()
        
        for feed_url in self.feeds:
            logger.info(f"Fetching Google Alerts feed: {feed_url[:50]}...")
            
            try:
                entries = self._fetch_feed(feed_url)
                
                for entry in entries:
                    url = entry.get("link", "")
                    
                    # Dedupe by URL
                    if url and url not in seen_urls:
                        seen_urls.add(url)
                        yield entry
                    else:
                        logger.debug(f"Skipping duplicate: {url}")
                        
            except Exception as e:
                logger.error(f"Error fetching Google Alerts feed {feed_url}: {e}")
                continue
    
    def _fetch_feed(self, feed_url: str) -> List[dict]:
        """Fetch and parse an RSS feed."""
        try:
            response = self.session.get(feed_url, timeout=30)
            response.raise_for_status()
            
            # Parse RSS feed
            feed = feedparser.parse(response.text)
            
            if feed.bozo and feed.bozo_exception:
                logger.warning(f"Feed parsing warning: {feed.bozo_exception}")
            
            entries = []
            for entry in feed.entries:
                lead = self._parse_entry(entry)
                if lead:
                    entries.append(lead)
            
            logger.info(f"Found {len(entries)} entries in feed")
            return entries
            
        except requests.RequestException as e:
            logger.error(f"Request failed for {feed_url}: {e}")
            return []
    
    def _parse_entry(self, entry) -> dict:
        """Parse a feed entry into a lead dict."""
        title = entry.get("title", "")
        link = entry.get("link", "")
        
        if not title or not link:
            return None
        
        # Get description/summary
        description = entry.get("description") or entry.get("summary") or ""
        
        # Get published date
        published = entry.get("published") or entry.get("pubDate") or entry.get("updated")
        if hasattr(entry, "published_parsed") and entry.published_parsed:
            try:
                published = datetime(*entry.published_parsed[:6]).isoformat()
            except:
                pass
        
        # Extract location from title/description if possible
        location = self._extract_location(title + " " + description)
        
        return {
            "title": title,
            "url": link,
            "location": location,
            "description": description[:500] if description else "",  # Truncate long descriptions
            "posted_time": published,
            "source": "google_alerts",
            "budget": None,
        }
    
    def _extract_location(self, text: str) -> str:
        """Extract location from text if Ontario city is mentioned."""
        ontario_cities = [
            "Sudbury", "North Bay", "Timmins", "Sault Ste Marie", "Sault Ste. Marie",
            "Thunder Bay", "Sudbury", "Elliot Lake", "Temiskaming Shores",
            "Cochrane", "Kirkland Lake", "Hearst", "Kapuskasing", "Smooth Rock Falls",
            "Ontario", "Toronto", "Ottawa", "Hamilton", "London", "Windsor"
        ]
        
        text_lower = text.lower()
        for city in ontario_cities:
            if city.lower() in text_lower:
                return city
        
        return "Ontario"  # Default location


def scrape_google_alerts(config: dict) -> Generator[dict, None, None]:
    """Convenience function to scrape Google Alerts with config."""
    source = GoogleAlertsSource(config)
    yield from source.search()


if __name__ == "__main__":
    # Test the scraper
    import json
    logging.basicConfig(level=logging.INFO)
    
    test_config = {
        "google_alerts": {
            "enabled": True,
            "feeds": [
                # Example feeds - these would be real alert URLs
                # "https://www.google.com/alerts/feeds/0659398215443689/BCIN%20Ontario",
            ]
        }
    }
    
    for lead in scrape_google_alerts(test_config):
        print(json.dumps(lead, indent=2))
