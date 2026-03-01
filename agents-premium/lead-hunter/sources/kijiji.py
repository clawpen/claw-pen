"""
Kijiji scraper for Lead Hunter agent.
Uses Playwright (Firefox) for browser automation to bypass blocking.
Searches for renovation and permit-related posts in Ontario cities.
"""

import logging
import time
import re
from typing import List, Generator, Optional
from urllib.parse import urljoin, quote_plus
from datetime import datetime

from bs4 import BeautifulSoup
from playwright.sync_api import sync_playwright, Browser, Page, BrowserContext

logger = logging.getLogger(__name__)

# Kijiji base URLs
KIJIJI_BASE = "https://www.kijiji.ca"
KIJIJI_SEARCH = "https://www.kijiji.ca/b-{category}/{location}/{query}/k0c{category_code}"


class KijijiScraper:
    """Scrapes Kijiji for renovation and permit-related listings using Playwright."""
    
    # Category mappings
    CATEGORIES = {
        "home_renovation": {"slug": "home-renovation-trade-services", "code": "44"},
        "general": {"slug": "general-services", "code": "45"},
        "real_estate": {"slug": "real-estate", "code": "34"},
        "housing": {"slug": "housing", "code": "37"},
    }
    
    # Location ID mappings for Ontario cities (for search URL format)
    LOCATIONS = {
        "Sudbury": {"path": "sudbury", "code": "l9004"},
        "North Bay": {"path": "north-bay", "code": "l1700027"},
        "Timmins": {"path": "timmins", "code": "l1700028"},
        "Sault Ste Marie": {"path": "sault-ste-marie", "code": "l1700026"},
        "Ontario": {"path": "ontario", "code": "l9004"},  # Broader Ontario search
    }
    
    def __init__(self, config: dict, headless: bool = True, browser_type: str = "firefox"):
        self.config = config
        self.search_config = config.get("search", {})
        self.locations = self.search_config.get("locations", list(self.LOCATIONS.keys()))
        self.keywords = self.search_config.get("keywords", [])
        self.max_results = self.search_config.get("max_results_per_query", 50)
        self.headless = headless
        self.browser_type = browser_type
        
        # Browser instances (lazy init)
        self._playwright = None
        self._browser: Optional[Browser] = None
        self._context: Optional[BrowserContext] = None
    
    def _init_browser(self):
        """Initialize Playwright browser."""
        if self._browser is not None:
            return
        
        logger.info(f"Initializing {self.browser_type} browser (headless={self.headless})...")
        self._playwright = sync_playwright().start()
        
        if self.browser_type == "firefox":
            self._browser = self._playwright.firefox.launch(headless=self.headless)
        else:
            self._browser = self._playwright.chromium.launch(headless=self.headless)
        
        # Create context with realistic settings
        self._context = self._browser.new_context(
            viewport={"width": 1920, "height": 1080},
            user_agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0",
            locale="en-CA",
            timezone_id="America/Toronto",
        )
        
        logger.info("Browser initialized successfully")
    
    def _close_browser(self):
        """Close browser and cleanup."""
        if self._context:
            self._context.close()
            self._context = None
        if self._browser:
            self._browser.close()
            self._browser = None
        if self._playwright:
            self._playwright.stop()
            self._playwright = None
        logger.info("Browser closed")
    
    def _fetch_page(self, url: str) -> Optional[str]:
        """Fetch page content using Playwright."""
        if self._browser is None:
            self._init_browser()
        
        page: Optional[Page] = None
        try:
            page = self._context.new_page()
            
            # Block unnecessary resources to speed up
            page.route("**/*.{png,jpg,jpeg,gif,svg,woff,woff2}", lambda route: route.abort())
            
            logger.debug(f"Navigating to: {url}")
            response = page.goto(url, wait_until="domcontentloaded", timeout=30000)
            
            if response and response.status == 200:
                # Wait for listings to load
                page.wait_for_selector("div[data-listing-id], div.search-item, article", timeout=10000)
                
                # Get the HTML content
                html = page.content()
                logger.debug(f"Page fetched: {len(html)} bytes")
                return html
            else:
                status = response.status if response else "no response"
                logger.warning(f"Failed to fetch {url}: status {status}")
                return None
                
        except Exception as e:
            logger.error(f"Error fetching {url}: {e}")
            return None
        finally:
            if page:
                page.close()
    
    def search(self) -> Generator[dict, None, None]:
        """
        Execute all search queries and yield results.
        Each result is a dict representing a lead.
        """
        try:
            self._init_browser()
            seen_urls = set()
            
            for location in self.locations:
                for keyword in self.keywords:
                    logger.info(f"Searching: '{keyword}' in {location}")
                    
                    try:
                        results = self._search_query(keyword, location)
                        for result in results:
                            # Dedupe by URL
                            if result["url"] not in seen_urls:
                                seen_urls.add(result["url"])
                                yield result
                            else:
                                logger.debug(f"Skipping duplicate: {result['url']}")
                        
                        # Be polite to Kijiji
                        time.sleep(3)
                        
                    except Exception as e:
                        logger.error(f"Error searching '{keyword}' in {location}: {e}")
                        continue
        finally:
            self._close_browser()
    
    def _search_query(self, keyword: str, location: str) -> List[dict]:
        """Execute a single search query and parse results."""
        results = []
        
        # Build search URL using Kijiji search format
        loc_info = self.LOCATIONS.get(location, self.LOCATIONS["Ontario"])
        
        # Use Kijiji's search URL format: /b-{location}/{query}/k0{locationCode}?dc=true
        url = f"https://www.kijiji.ca/b-{loc_info['path']}/{quote_plus(keyword)}/k0{loc_info['code']}?dc=true"
        logger.info(f"Fetching: {url}")
        
        try:
            html = self._fetch_page(url)
            if html:
                listings = self._parse_search_results(html, location)
                results.extend(listings)
        except Exception as e:
            logger.warning(f"Request failed for {url}: {e}")
        
        return results[:self.max_results]
    
    def _parse_search_results(self, html: str, location: str) -> List[dict]:
        """Parse Kijiji search results HTML."""
        results = []
        soup = BeautifulSoup(html, "lxml")
        
        # Find listing links - Kijiji uses /v- prefix for individual listings
        listing_links = soup.find_all("a", href=re.compile(r"/v-"))
        logger.debug(f"Found {len(listing_links)} listing links")
        
        seen_urls = set()
        for link in listing_links[:self.max_results * 2]:  # Look at more to account for duplicates
            try:
                lead = self._parse_listing_link(link, location)
                if lead and lead["url"] not in seen_urls:
                    seen_urls.add(lead["url"])
                    results.append(lead)
                    if len(results) >= self.max_results:
                        break
            except Exception as e:
                logger.debug(f"Failed to parse listing: {e}")
                continue
        
        return results
    
    def _parse_listing_link(self, link, location: str) -> dict:
        """Parse a single listing link element."""
        # Get title from link text
        title = link.get_text(strip=True)
        
        # Get URL
        href = link.get("href", "")
        if href.startswith("/"):
            url = urljoin(KIJIJI_BASE, href)
        else:
            url = href
        
        if not url or not title or len(title) < 5:
            return None
        
        # Try to get parent container for more context
        parent = link.find_parent("div", class_=re.compile(r"(item|card|listing|search)", re.I))
        if not parent:
            parent = link.find_parent("article")
        
        # Extract description from nearby elements
        description = ""
        if parent:
            desc_elem = parent.find("p") or parent.find("div", class_=re.compile(r"desc", re.I))
            if desc_elem:
                description = desc_elem.get_text(strip=True)
        
        # Extract price/budget
        budget = None
        if parent:
            price_text = parent.find(string=re.compile(r"\$[\d,]+"))
            if price_text:
                budget = price_text.strip()
        
        # Extract location from listing if available
        listing_location = location
        if parent:
            loc_elem = parent.find("span", class_=re.compile(r"location", re.I))
            if loc_elem:
                listing_location = loc_elem.get_text(strip=True) or location
        
        return {
            "title": title,
            "url": url,
            "location": listing_location,
            "description": description,
            "budget": budget,
            "posted_time": None,  # Would need to fetch individual page
            "source": "kijiji",
        }
    
    def get_listing_details(self, url: str) -> dict:
        """Fetch full details of a specific listing."""
        try:
            html = self._fetch_page(url)
            if not html:
                return {}
            
            soup = BeautifulSoup(html, "lxml")
            
            # Extract full description
            desc_elem = (
                soup.find("div", {"itemprop": "description"}) or
                soup.find("div", class_=re.compile(r"description", re.I))
            )
            description = desc_elem.get_text(strip=True) if desc_elem else ""
            
            # Extract contact info (if available publicly)
            contact = {}
            phone_elem = soup.find("a", href=re.compile(r"tel:"))
            if phone_elem:
                contact["phone"] = phone_elem.get("href").replace("tel:", "")
            
            return {
                "description": description,
                "contact": contact,
            }
            
        except Exception as e:
            logger.error(f"Failed to fetch listing details: {e}")
            return {}


def scrape_kijiji(config: dict, headless: bool = True, browser_type: str = "firefox") -> Generator[dict, None, None]:
    """Convenience function to scrape Kijiji with config."""
    scraper = KijijiScraper(config, headless=headless, browser_type=browser_type)
    yield from scraper.search()


if __name__ == "__main__":
    # Test the scraper
    import json
    logging.basicConfig(level=logging.INFO)
    
    test_config = {
        "search": {
            "locations": ["Sudbury"],
            "keywords": ["renovation"],
            "max_results_per_query": 5,
        }
    }
    
    print("Testing Playwright-based Kijiji scraper...")
    for lead in scrape_kijiji(test_config, headless=True):
        print(json.dumps(lead, indent=2))
