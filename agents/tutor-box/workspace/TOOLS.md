# TOOLS.md - Local Notes

Container-specific tool configuration.

## LLM Provider

Set via LLM_PROVIDER environment variable:
- `openai` - OpenAI API (requires OPENAI_API_KEY)
- `anthropic` - Claude (requires ANTHROPIC_API_KEY)
- `ollama` - Local Ollama (connects to host.docker.internal:11434)
- `lmstudio` - LM Studio (connects to host.docker.internal:1234)

## Networking

Container connects to host services via:
- `host.docker.internal` - Host machine
- Tailscale - If configured
