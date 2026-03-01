# Setting Up Google Alerts for Lead Hunter

This guide walks you through creating Google Alerts and connecting them to Lead Hunter via RSS feeds.

## Why Google Alerts?

Google Alerts monitors the web for new content matching your keywords and delivers results via RSS. This is perfect for finding:
- New permit applications mentioned in news
- Renovation project announcements
- Construction company news
- Municipal building permit updates

## Step-by-Step Setup

### 1. Go to Google Alerts

Visit [google.com/alerts](https://www.google.com/alerts) in your browser.

### 2. Create an Alert

For each keyword/topic you want to track:

1. **Enter your search query** in the box. Good queries for Lead Hunter:
   - `permit drawings Ontario`
   - `BCIN designer Sudbury`
   - `architect renovation North Bay`
   - `building permit application Timmins`
   - `basement apartment permit Sault Ste Marie`
   - `deck construction contractor Ontario`
   - `home renovation project [city name]`

2. **Click "Show options"** to configure:
   - **How often**: "As-it-happens" or "Once a day"
   - **Sources**: "Automatic" or select "News" for news articles
   - **Language**: English
   - **Region**: Canada (or leave as "Any region")
   - **How many**: "Only the best results"
   - **Deliver to**: **Select "RSS feed"** ← This is critical!

3. **Click "Create Alert"**

### 3. Get the RSS Feed URL

After creating the alert:

1. Look for the alert in your list
2. Click the **RSS icon** (orange icon) next to the alert
3. This opens the RSS feed in your browser
4. **Copy the URL** from the address bar

The URL will look like:
```
https://www.google.com/alerts/feeds/12345678901234567890/12345678901234567890
```

### 4. Add to Lead Hunter Configuration

Edit `/data/claw-pen/agents/lead-hunter/agent.toml` and add your feed URL:

```toml
[rss]
feeds = [
    "https://www.google.com/alerts/feeds/YOUR_USER_ID/ALERT_ID_1",
    "https://www.google.com/alerts/feeds/YOUR_USER_ID/ALERT_ID_2",
    "https://www.google.com/alerts/feeds/YOUR_USER_ID/ALERT_ID_3",
]
```

### 5. Test the Configuration

Run Lead Hunter to verify RSS feeds are working:

```bash
cd /data/claw-pen/agents/lead-hunter
python -m sources.rss
```

Or test the full agent:

```bash
python main.py --source rss
```

## Recommended Alert Queries

Create alerts for each of these patterns (customize for your target cities):

| Query | Purpose |
|-------|---------|
| `"building permit" Sudbury Ontario` | New permit applications |
| `"BCIN" designer Ontario` | BCIN professional mentions |
| `"architect" "home renovation" Ontario` | Architect projects |
| `"deck permit" Ontario` | Deck construction leads |
| `"basement apartment" permit Ontario` | Suite legalization projects |
| `"garage construction" permit | Garage builds |
| `"home addition" contractor Ontario` | Addition projects |

## Tips

- **Be specific**: `"building permit" Sudbury` is better than just `permit`
- **Use quotes** for exact phrases: `"BCIN designer"`
- **Combine terms**: `"home renovation" architect Ontario`
- **Limit region** in alert options to reduce noise
- **Create 5-10 alerts** covering different angles
- **Review periodically**: Delete alerts that produce too much noise

## Troubleshooting

**No entries appearing?**
- Verify the RSS URL opens in a browser
- Check that the feed contains entries (some feeds may be empty if no matches)
- Ensure `feedparser` is installed: `pip install feedparser`

**Too many irrelevant results?**
- Make your alert queries more specific
- Use "Only the best results" option
- Add more keywords to filter in agent.toml

**Google Alert not creating RSS option?**
- You must be logged into a Google account
- The "Deliver to: RSS feed" option appears after clicking "Show options"
