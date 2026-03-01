# Lead Hunter Agent

Automated lead finder for And or Design (BCIN permit services).

## Purpose

Scrapes online sources to find clients looking for permit drawings in Northern Ontario:
- Sudbury
- North Bay
- Timmins
- Sault Ste. Marie

## Current Features

- ✅ **Kijiji Scraper** - Searches renovation/permit-related listings
- 🔲 Facebook Groups - Stub (future)
- 🔲 Municipal Permits - Stub (future)

## Installation

```bash
cd /data/claw-pen/agents/lead-hunter
pip install -r requirements.txt
```

## Usage

### Run Once
```bash
python main.py
```

### Dry Run (Test Mode)
```bash
python main.py --dry-run
```

### Scheduled Mode
```bash
python main.py --schedule
```

### With Custom Config
```bash
python main.py --config /path/to/custom-agent.toml
```

## Configuration

Edit `agent.toml` to customize:

```toml
[agent]
name = "lead-hunter"
org = "andor-design"

[search]
locations = ["Sudbury", "North Bay", "Timmins", "Sault Ste Marie"]
keywords = ["permit drawings", "architect", "BCIN", "basement apartment"]

[notifications]
whatsapp = true
channel = "webchat"
recipient = "Jer"

[schedule]
cron = "0 6 * * *"  # 6 AM daily
```

## Output

### Logs
Logs are written to `logs/lead-hunter.log`

### Memory
Seen leads are stored in `memory/leads.json` for deduplication.

### Notifications
Pending notifications are queued in `notifications/` directory.

When running within OpenClaw context, use `openclaw_runner.py` to send notifications via the message tool.

## Notification Format

```
🏠 New Lead: [Title]
📍 Location: [City]
🔗 Link: [URL]
💰 Budget: [if listed]
📝 Description: [snippet]
⏰ Posted: [time]
🔍 Source: kijiji
```

## Directory Structure

```
lead-hunter/
├── agent.toml          # Configuration
├── main.py             # Entry point
├── openclaw_runner.py  # OpenClaw integration
├── notifier.py         # Notification handler
├── requirements.txt    # Python dependencies
├── README.md           # This file
├── sources/
│   ├── __init__.py
│   ├── kijiji.py       # Kijiji scraper
│   ├── facebook.py     # Facebook stub
│   └── municipal.py    # Municipal stub
├── logs/               # Runtime logs
├── memory/             # Lead storage
└── notifications/      # Pending notifications
```

## Development

### Adding New Sources

1. Create a new scraper in `sources/`
2. Implement the `search()` generator that yields lead dicts
3. Import and call in `main.py`
4. Update `sources/__init__.py`

### Lead Data Format

```python
{
    "title": str,
    "url": str,
    "location": str,
    "description": str,
    "budget": Optional[str],
    "posted_time": Optional[str],
    "source": str,
}
```

## Notes

- Be respectful of rate limits when scraping
- Kijiji may block automated access; use appropriate delays
- Check Terms of Service before implementing new scrapers
- Municipal permit data may require FOI requests in some cases

---

## Edge Cases & Troubleshooting

### Common Issues

#### No leads found

**Symptoms:** Agent runs but reports 0 leads

**Causes & Fixes:**
1. **Rate limited by Kijiji** - Wait 1 hour, or reduce `max_results_per_query`
2. **Keywords too specific** - Add broader terms like "renovation help" or "contractor needed"
3. **Wrong location IDs** - Verify Kijiji location IDs in `agent.toml`
4. **All results filtered as service ads** - Review `exclude_service_keywords` list

#### Too many false positives

**Symptoms:** Getting job postings or competitor ads

**Causes & Fixes:**
1. **Job postings leaking through** - Add patterns to `exclude_keywords`:
   ```toml
   exclude_keywords = [
     "hiring",
     "now hiring",
     "we are hiring",
     # Add more patterns you see
   ]
   ```
2. **Service ads not filtered** - Add to `exclude_service_keywords`:
   ```toml
   exclude_service_keywords = [
     "we provide",
     "professional services",
     # Add patterns from false positives
   ]
   ```

#### Duplicate notifications

**Symptoms:** Same lead notified multiple times

**Causes & Fixes:**
1. **Memory file corrupted** - Delete `memory/leads.json` to reset
2. **URL changes** - Some sites add tracking parameters; enable URL normalization

#### Scraper fails silently

**Symptoms:** Errors in logs, no leads found

**Causes & Fixes:**
1. **Playwright not installed** - Run: `playwright install firefox`
2. **Firefox not available** - Install Firefox or use chromium
3. **Network issues** - Check internet connection, try VPN if blocked

### Rate Limiting

Kijiji may block automated access. Mitigation strategies:

1. **Increase delays** between requests (in code)
2. **Reduce `max_results_per_query`** to 10 or less
3. **Run less frequently** - Change cron to weekly
4. **Use residential proxy** if available

### Data Quality

#### Validating Leads

Before acting on a lead, verify:
- [ ] Listing still exists (URL works)
- [ ] Contact info is present
- [ ] Budget/expectations are realistic
- [ ] Location is actually in your service area

#### Handling Incomplete Data

Some leads may have missing fields:
- `budget` is often not listed
- `posted_time` may be relative ("2 days ago")
- `location` may be vague ("Northern Ontario")

The agent handles these gracefully but note them in notifications.

### Scaling Considerations

#### Multiple Locations

To search more cities, add to `agent.toml`:
```toml
[search]
locations = ["Sudbury", "North Bay", "Timmins", "Sault Ste Marie", "Parry Sound", "Hearst"]
```

Note: More locations = more API calls = higher rate limit risk.

#### Multiple Sources

To add new scrapers:
1. Create `sources/newsource.py` with `search()` generator
2. Import in `sources/__init__.py`
3. Call in `main.py` similar to `_run_kijiji()`
4. Test with `--dry-run` first

### Memory Management

#### Clearing Old Leads

Leads older than `dedup_retention_days` are auto-removed. To manually clear:
```bash
rm memory/leads.json
```

#### Memory File Size

If `leads.json` grows large (>10MB):
1. Reduce `dedup_retention_days` in config
2. Manually prune old entries
3. Consider switching to SQLite for large scale

---

## Production Checklist

Before deploying to production:

- [ ] Test with `--dry-run` for at least 3 runs
- [ ] Verify notification delivery works
- [ ] Set up log rotation for `logs/` directory
- [ ] Configure appropriate cron schedule
- [ ] Add monitoring for failed runs
- [ ] Document any custom keywords/filters

---

## Support

For issues or feature requests, contact the Claw Pen team.

