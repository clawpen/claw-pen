# GPU Options for Community AI Node - Canadian Prices (CAD)

**Prices from eBay.ca - February 28, 2026**

---

## Budget Option: Used RTX 3090 (24GB VRAM)

| Card | Price (CAD) | Notes |
|------|-------------|-------|
| EVGA RTX 3090 FTW3 | $1,350 | "Like New", free shipping |
| Gigabyte RTX 3090 Turbo | $1,336-$1,364 | Blower cooler (good for servers) |
| Zotac RTX 3090 Passive Server | $1,362 | Server-grade, NVLink support |
| EVGA RTX 3090 Kingpin | $1,399 | Water block included |
| RTX 3090 Founders Edition | $1,144-$1,599 | Varies by condition |

**Best value:** Used RTX 3090 blower/server cards ~$1,350 CAD

---

## Mid-Range Option: Used RTX A6000 (48GB VRAM)

| Card | Price (CAD) | Notes |
|------|-------------|-------|
| NVIDIA RTX A6000 48GB (used) | $6,700-$7,300 | Professional card, NVLink |
| PNY RTX A6000 48GB | $7,200-$8,500 | Open box/used |
| Dell OEM RTX A6000 48GB | $7,200-$9,500 | New/open box |

**Best value:** Used A6000 ~$6,700-7,200 CAD

---

## New Options (Retail Canada)

| Card | VRAM | Est. Price (CAD) | Source |
|------|------|------------------|--------|
| RTX 4090 | 24GB | $2,200-2,600 | Canada Computers, Memory Express |
| RTX 6000 Ada | 48GB | $8,500-10,000 | B&H Photo (ships to Canada) |
| L40S | 48GB | $6,000-7,000 | Datacenter distributors |

---

## Recommended Configurations

### Phase 1: Pilot ($3,000-4,000 CAD)
```
2x Used RTX 3090 (24GB each) = $2,700
Server case + 1000W PSU = $400
10GbE NIC + switch = $200
Total: ~$3,300 CAD

Runs: Llama 70B (quantized), Qwen3 34B (full precision)
Serves: 20-40 concurrent users
```

### Phase 2: Growth ($8,000-10,000 CAD)
```
2x Used RTX A6000 (48GB each) = $14,000
OR
4x Used RTX 3090 (24GB each) = $5,400 + server = $7,000

Runs: Llama 70B (full), Qwen3 120B (quantized)
Serves: 50-100 concurrent users
```

### Phase 3: Production ($25,000-35,000 CAD)
```
4x RTX 6000 Ada (48GB each) = $36,000
OR
4x Used A6000 (48GB each) = $28,000
OR
4x A100 80GB (used) = $35,000-40,000

Runs: Kimi K2, Qwen3 Coder 480B (quantized)
Serves: 200+ concurrent users
```

---

## Distributed Setup (2 Machines)

**Machine 1 + Machine 2 = $7,000-8,000 CAD**

| Component | Each Machine | Total (x2) |
|-----------|--------------|------------|
| 2x RTX 3090 | $2,700 | $5,400 |
| Desktop PC (CPU, RAM, case) | $800 | $1,600 |
| 10GbE NIC | $50 | $100 |
| 10GbE switch | - | $150 |
| **Total** | | **~$7,250** |

**Total VRAM:** 96GB (48GB per machine)
**Runs:** Llama 70B full precision, or 120B quantized
**Latency:** +1-2ms over single machine (acceptable for chat)

---

## Canadian Retailers

| Retailer | Notes |
|----------|-------|
| **Canada Computers** | Best for consumer GPUs (4090) |
| **Memory Express** | Good prices, price matching |
| **Newegg.ca** | Wide selection, frequent sales |
| **B&H Photo** | US but ships to Canada, pro GPUs |
| **Amazon.ca** | Sometimes competitive |

---

## Used Market Sources

| Source | Risk | Notes |
|--------|------|-------|
| **eBay.ca** | Medium | Buyer protection, check seller rating |
| **Kijiji** | High | Local pickup, cash, no protection |
| **Facebook Marketplace** | High | Same as Kijiji |
| **Reddit r/canadianhardwareswap** | Medium | Community reputation helps |
| **Server surplus dealers** | Low | Often have A6000/A100 |

---

## Taxes & Duties

| Purchase Type | Tax |
|---------------|-----|
| Canadian retailer | HST/GST (13% ON) |
| eBay Canada | HST/GST included in price |
| US import | Duties (~6%) + HST/GST |

---

## Summary: What to Present

| Scenario | Hardware | Cost | Capability |
|----------|----------|------|------------|
| **Minimum viable** | 1x RTX 3090 | $1,500 | 7B-13B models, 10 users |
| **Pilot** | 2x RTX 3090 | $3,500 | 70B quantized, 30 users |
| **Growth** | 2x A6000 or 4x 3090 | $8,000 | 70B full, 80 users |
| **Production** | 4x A6000 or 4x A100 | $30,000 | Full Kimi K2, 200+ users |
