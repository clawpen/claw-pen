# Tutor Box

A patient, adaptive learning companion agent. Learn anything with guided lessons.

## What It Does

Tutor Box is an AI tutor that:
- **Assesses your level** - Figures out where you're at
- **Breaks down topics** - Complex concepts become digestible pieces
- **Teaches actively** - Explains, demonstrates, quizzes, iterates
- **Adapts to you** - Slows down when lost, speeds up when bored

## Deployment

### As a Claw Pen Agent

```bash
cd /data/claw-pen
claw-pen create tutor-box --from agents/tutor-box
```

### With OpenClaw

Copy the workspace files to your OpenClaw agent workspace:

```bash
cp -r agents/tutor-box/workspace/* ~/.openclaw/agents/<your-agent>/
```

### As a Container

```bash
# Build
podman build -t tutor-box agents/tutor-box

# Run
podman run -it \
  -e LLM_PROVIDER=openai \
  -e OPENAI_API_KEY=sk-xxx \
  tutor-box
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `LLM_PROVIDER` | LLM provider (openai, anthropic, ollama, lmstudio, zai) | `zai` |
| `LLM_MODEL` | Model to use | `glm-5` |
| `OPENAI_API_KEY` | OpenAI API key | - |
| `ANTHROPIC_API_KEY` | Anthropic API key | - |
| `ZAI_API_KEY` | z.ai API key | - |

### config.json

```json
{
  "name": "tutor-box",
  "display_name": "Tutor Box",
  "description": "Patient learning companion for any subject",
  "version": "1.0.0",
  "llm": {
    "provider": "zai",
    "model": "glm-5"
  },
  "memory": {
    "enabled": true,
    "persist": true
  },
  "channels": ["webchat"],
  "resources": {
    "memory_mb": 1024,
    "cpu_cores": 1.0
  }
}
```

## Files

```
tutor-box/
├── SOUL.md          # Core personality and teaching style
├── IDENTITY.md      # Name, emoji, tagline
├── USER.md          # Learner context
├── README.md        # This file
├── config.json      # Configuration
└── workspace/
    ├── AGENTS.md    # Container agent instructions
    ├── HEARTBEAT.md # Heartbeat config (empty = skip)
    ├── MEMORY.md    # Long-term memory
    └── TOOLS.md     # Tool configuration
```

## Usage Examples

### Learning Programming

```
You: I want to learn Python
Tutor: Great choice! Let's figure out where you are. Have you written any code before, or are you completely new to programming?
```

### Math Help

```
You: I don't understand derivatives
Tutor: No worries - they're tricky at first. Let me ask: do you know what a slope is? Like, the steepness of a hill?
```

### Language Learning

```
You: Teach me Spanish
Tutor: ¡Hola! Let's start simple. Do you know any Spanish words already, or are we starting from zero?
```

## Teaching Philosophy

1. **Never dump information** - Engage first
2. **Use analogies and stories** - Make it click
3. **Check understanding** - Before moving on
4. **Celebrate progress** - Normalize confusion
5. **Honest about limits** - "I don't know" is valid

## Constraints

- Won't do your homework for you (will help you understand)
- Won't write essays you're supposed to write
- May ask clarifying questions before explaining
- Adapts pace based on your responses

## Customization

Edit `SOUL.md` to change teaching style:
- More formal? Remove casual language
- Subject-specific? Add domain expertise
- Different pace? Adjust the adaptation rules

---

**Emoji:** 📚  
**Tagline:** "Let's figure this out together"
