# Claw Pen Templates

Agent templates for one-click deployment.

---

## Free Templates

Located in `/agents/`

| Template | Description | Status |
|----------|-------------|--------|
| **tutor-box** | Learning companion — learn anything with guided lessons | ✅ Ready |
| **openclaw-agent** | Generic OpenClaw instance — build your own agent on top | ✅ Ready |

---

## Premium Templates

Located in `/agents-premium/`

| Template | Description | Target | Price | Status |
|----------|-------------|--------|-------|--------|
| **lead-hunter** | Kijiji scraper for business leads (Playwright + Firefox) | Small businesses | $29 | ✅ Ready |
| **billing-assistant** | Time tracking with cursor/doc focus, auto-invoicing | Lawyers, consultants | $39/mo | 🔲 Planned |
| **brief-synthesizer** | Document analysis, case summaries, cited drafts | Legal, researchers | $29 | 🔲 Planned |
| **research-tool** | Deep-dive research with citations, saved sessions | Academics, analysts | $29 | 🔲 Planned |

---

## Using Free Templates

```bash
# Clone or download Claw Pen
git clone https://github.com/clawpen/claw-pen.git
cd claw-pen

# Deploy a template
./claw-pen deploy tutor-box
```

---

## Premium Templates

Premium templates require purchase. After purchase, you'll receive access to the template files and deployment instructions.

Contact: hi@clawpen.ca (coming soon)

---

## Template Structure

Each template lives in its own folder:

```
agents/
├── tutor-box/
│   ├── config.json         # Agent configuration
│   ├── SOUL.md             # Personality and behavior
│   ├── README.md           # Documentation
│   └── workspace/          # Agent workspace
├── openclaw-agent/
│   └── ...
```

---

## Creating a New Template

1. Copy `openclaw-agent` as a starting point
2. Edit `SOUL.md` — define personality, behavior, constraints
3. Update `config.json` — model, tools, resources
4. Test thoroughly
5. Write `README.md` — usage, examples, troubleshooting
6. Add to `TEMPLATES.md`

---

## Pricing Philosophy

- **Free:** Generic or educational templates
- **Premium ($19-49):** Specialized, business-value templates
- **Subscription ($9-39/mo):** Templates with ongoing value or hosting included

> "Find 1 extra hour/month = template pays for itself"
