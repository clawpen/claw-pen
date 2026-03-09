# TOOLS.md - Revit Agent Tools

## MCP Server Endpoints

| Service | Port | URL |
|---------|------|-----|
| MCP Server | 8001 | `http://aesir:8001` |
| Revit Direct | 48884 | `http://aesir:48884` |
| Enhancer | 5000 | `http://aesir:5000` |

## RevitMCP Tools

See `~/.openclaw/skills/revit-mcp/SKILL.md` for full tool reference.

### Quick Reference

```
# Create levels
POST /tools/revit_create_levels
Body: {"levels": ["L1", "L2"], "heights": [0, 4000]}

# Create walls
POST /tools/revit_create_walls
Body: {"walls": [{"start": [0,0], "end": [5000,0], "level": "L1"}]}

# Place door
POST /tools/revit_place_door
Body: {"wall_id": 12345, "position": 2500, "family": "Single-Flush"}

# Query rooms
GET /tools/revit_get_rooms
```

## Common Families

| Type | Name |
|------|------|
| Door | Single-Flush |
| Door | Double-Flush |
| Window | Fixed |
| Window | Double-Hung |

## Wall Types

| Type | Use |
|------|-----|
| Basic Wall: Generic - 200mm | Interior partitions |
| Basic Wall: Exterior - Brick on CMU | Exterior |
| Curtain Wall | Glazing |
