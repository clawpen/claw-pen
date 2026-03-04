# Teams & Router Agent Architecture

This document explains how teams work in claw-pen, allowing you to create a single entry point that routes messages to specialist agents.

## Overview

Teams provide a **router agent** pattern where:

1. User sends a message to a single team endpoint
2. The router classifies the message by intent
3. The message is forwarded to the appropriate specialist agent
4. The agent's response is returned to the user in the same conversation

```
                    ┌─────────────────┐
                    │   User Message  │
                    └────────┬────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │  Router Agent   │
                    │   (Classifier)  │
                    └────────┬────────┘
                             │
         ┌───────────────────┼───────────────────┐
         │                   │                   │
         ▼                   ▼                   ▼
   ┌───────────┐       ┌───────────┐       ┌───────────┐
   │  Agent A  │       │  Agent B  │       │  Agent C  │
   │(Receipts) │       │(Payables) │       │(Reports)  │
   └───────────┘       └───────────┘       └───────────┘
```

## Team Configuration

Teams are defined in TOML files in the `teams/` directory:

```toml
# teams/finance-team.toml

[team]
name = "Finance AI Team"
description = "Receipts, invoicing, payables, and financial reporting"
version = "1.0.0"

[router]
name = "finance-router"
mode = "hybrid"              # "keyword" | "llm" | "hybrid"
confidence_threshold = 0.7   # 0.0-1.0
clarify_on_low_confidence = true

[agents]
receipts = { agent = "finn", description = "Handles expense receipts" }
payables = { agent = "pax", description = "Handles bills to pay" }
receivables = { agent = "rae", description = "Handles client invoices" }
reporter = { agent = "rex", description = "Handles financial reports" }

[routing.receipts]
keywords = ["receipt", "expense", "bought", "purchased", "spent"]
examples = [
    "I bought office supplies",
    "Here's a receipt from Staples",
]

[routing.payables]
keywords = ["pay", "bill", "due", "owe", "vendor", "supplier"]
examples = [
    "Need to pay the electric bill",
    "We owe $200 to the supplier",
]

[responses]
routing_ack = "Let me pass this to {agent_name}..."
clarification_needed = "I need a bit more info to help you best."
```

## Routing Modes

### Keyword Mode (Fast)

Simple keyword matching. Good for well-defined categories with distinct vocabulary.

```toml
[router]
mode = "keyword"
```

- **Pros:** Fast, no LLM calls, predictable
- **Cons:** Less flexible, may miss edge cases

### LLM Mode (Accurate)

Uses an LLM to classify intent. Best for complex or ambiguous messages.

```toml
[router]
mode = "llm"
```

- **Pros:** Handles nuance, understands context
- **Cons:** Costs tokens, slower

### Hybrid Mode (Recommended)

Tries keyword matching first, falls back to LLM if confidence is low.

```toml
[router]
mode = "hybrid"
confidence_threshold = 0.7
```

- **Pros:** Best of both worlds
- **Cons:** Slightly more complex

## API Endpoints

### List Teams

```
GET /api/teams
```

Returns all configured teams.

### Get Team

```
GET /api/teams/:id
```

Returns a specific team's configuration.

### Classify Message

```
POST /api/teams/:id/classify
Content-Type: application/json

{
  "message": "I need to submit a receipt for lunch"
}
```

Returns the classification result:

```json
{
  "intent": "receipts",
  "confidence": 0.85,
  "matched_keywords": ["receipt"],
  "needs_clarification": false
}
```

### Team Chat (WebSocket)

```
GET /api/teams/:id/chat
```

WebSocket endpoint for conversational routing. Messages are classified and routed to specialist agents.

**Protocol:**

```json
// Client sends:
{
  "content": "I bought some office supplies"
}

// Server responds with routing ack:
{
  "role": "assistant",
  "content": "Let me pass this to Handles expense receipts...",
  "routing_to": "finn",
  "classification": { "intent": "receipts", "confidence": 0.9 }
}

// Then agent response:
{
  "role": "assistant",
  "content": "[Agent response here]",
  "from_agent": "finn"
}
```

**Clarification Flow:**

If confidence is below threshold:

```json
// Server asks:
{
  "role": "assistant",
  "content": "I'm not sure what you need. Are you asking about:\n\n• receipts: Handles expense receipts\n• payables: Handles bills to pay\n• receivables: Handles client invoices",
  "classification": { "intent": "unknown", "confidence": 0.3, "needs_clarification": true }
}
```

## Implementation Details

### Router Agent

The router agent is a lightweight classifier that:

1. Receives the user message
2. Matches against routing rules (keywords/examples)
3. Optionally calls an LLM for classification
4. Returns the target agent ID or asks for clarification

### Specialist Agents

Specialist agents are standard claw-pen agents configured with:

- Their specific domain knowledge (via system prompt)
- Access to relevant tools/databases
- Response format appropriate to their domain

### Conversation Continuity

The router maintains conversation context:

- Each message includes the full conversation history
- The router can see previous classifications
- Agents can access conversation context for follow-up questions

## Best Practices

### 1. Distinct Routing Keywords

Make sure each agent has unique, non-overlapping keywords:

```toml
# Good - distinct keywords
[routing.receipts]
keywords = ["receipt", "bought", "purchased"]

[routing.payables]
keywords = ["owe", "bill due", "pay vendor"]

# Bad - overlapping keywords
[routing.receipts]
keywords = ["money", "spending"]  # Too vague

[routing.payables]
keywords = ["money", "payment"]   # Overlaps with receipts
```

### 2. Clear Agent Descriptions

Descriptions are shown to users when asking for clarification:

```toml
[agents]
receipts = { agent = "finn", description = "Money you've spent (receipts, purchases)" }
payables = { agent = "pax", description = "Money you owe (bills, vendor payments)" }
```

### 3. Confidence Threshold

Tune based on your use case:

- **0.5-0.6:** Aggressive routing, may misclassify
- **0.7-0.8:** Balanced (recommended)
- **0.9+:** Conservative, will ask for clarification often

### 4. Example Messages

Add examples to help with LLM classification:

```toml
[routing.receipts]
examples = [
    "I bought office supplies",
    "Here's a receipt from Staples",
    "Paid for client lunch",
]
```

## Future Enhancements

- [ ] Full LLM classification implementation
- [ ] Conversation history for context-aware routing
- [ ] Multi-agent routing (parallel execution)
- [ ] Custom routing logic via WASM plugins
- [ ] A/B testing of routing strategies
- [ ] Analytics on routing accuracy

## Example: Creating a Finance Team

1. Create the team config:

```bash
cp teams/_example-team.toml teams/my-finance-team.toml
```

2. Edit the configuration with your agents and routing rules.

3. The team is automatically loaded on orchestrator startup.

4. Connect via WebSocket:

```javascript
const ws = new WebSocket('ws://localhost:3000/api/teams/my-finance-team/chat');

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  console.log(msg);
};

ws.send(JSON.stringify({
  content: "I need to submit a receipt"
}));
```

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         Orchestrator                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ TeamRegistry│  │   Router    │  │     API Endpoints       │  │
│  │             │──│  Classifier │──│  /api/teams/:id/chat    │  │
│  │ teams/*.toml│  │             │  │  /api/teams/:id/classify│  │
│  └─────────────┘  └──────┬──────┘  └─────────────────────────┘  │
│                          │                                      │
└──────────────────────────┼──────────────────────────────────────┘
                           │
         ┌─────────────────┼─────────────────┐
         │                 │                 │
         ▼                 ▼                 ▼
   ┌───────────┐     ┌───────────┐     ┌───────────┐
   │ Container │     │ Container │     │ Container │
   │  Agent A  │     │  Agent B  │     │  Agent C  │
   │  (finn)   │     │   (pax)   │     │   (rae)   │
   └───────────┘     └───────────┘     └───────────┘
```

## Comparison with Alternatives

### Option A: Special Agent Type with Child Agents

❌ More complex, requires new agent type
❌ Tight coupling between router and children
✅ Single configuration file

### Option B: Orchestrator-level Routing (Our Choice)

✅ Decoupled from agent implementation
✅ Works with any existing agent
✅ Easy to configure via TOML
✅ Can be extended without code changes

### Option C: Team Config with Router

✅ Same as Option B
✅ Team is a first-class concept
✅ Clear separation of concerns

We chose **Option C** because it:
- Keeps teams as a separate, manageable concept
- Allows teams to be created/modified without code changes
- Works seamlessly with existing agents
- Provides clear API endpoints for team operations
