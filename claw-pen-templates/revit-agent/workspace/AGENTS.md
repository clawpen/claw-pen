# AGENTS.md - Revit Agent Workspace

This agent has direct Revit integration via MCP.

## Every Session

1. Read `SOUL.md` - who you are
2. Read `MEMORY.md` - project context
3. Check MCP connection status

## MCP Connection

The MCP server runs on Windows (aesir). Connection via:
- Tailscale: `http://100.121.148.91:8001`
- Or local network IP

Test connection:
```bash
curl http://$REVITMCP_SERVER_URL/health
```

## Workflow

### Design Tasks
1. Understand requirements
2. Plan massing/zoning
3. Create levels and grids
4. Model walls and floors
5. Add families and details
6. Create views and sheets

### Always
- Explain design decisions
- Follow code requirements
- Use appropriate wall/floor types
- Maintain level consistency

## Memory

- Project notes → `MEMORY.md`
- Session logs → `memory/YYYY-MM-DD.md`
- Design decisions → document in model and memory

## Safety

- Don't delete without asking
- Save before major changes
- Verify family names before loading
