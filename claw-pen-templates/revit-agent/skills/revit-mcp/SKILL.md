# Revit MCP Skill

Connect to Revit via MCP (Model Context Protocol) server for architectural modeling.

## Prerequisites

- RevitMCP server running on Windows (aesir)
- Tailscale or network connectivity between agent and Revit host
- Revit with AriA addin loaded

## Configuration

Set environment variables:
```bash
REVITMCP_SERVER_URL=http://100.121.148.91:8001  # MCP server
REVITMCP_REVIT_URL=http://100.121.148.91:48884  # Revit direct API
REVITMCP_AUTH_TOKEN=                            # Optional auth
```

## Available Tools

### Structure

| Tool | Description |
|------|-------------|
| `revit_create_levels` | Create building levels from list |
| `revit_create_grids` | Create column grids |
| `revit_create_walls` | Create walls from specifications |
| `revit_create_floors` | Create floor slabs |
| `revit_create_roofs` | Create roof elements |
| `revit_create_ceiling` | Add ceilings to rooms |

### Families

| Tool | Description |
|------|-------------|
| `revit_place_doors` | Place doors in walls |
| `revit_place_windows` | Place windows in walls |
| `revit_place_family` | Place any family instance |
| `revit_load_family` | Load family from library |

### Rooms & Spaces

| Tool | Description |
|------|-------------|
| `revit_create_rooms` | Create rooms from boundaries |
| `revit_tag_rooms` | Add room tags |
| `revit_set_room_properties` | Set room data (name, number, dept) |

### Views

| Tool | Description |
|------|-------------|
| `revit_create_view` | Create floor plan/section/3D view |
| `revit_create_sheet` | Create drawing sheet |
| `revit_place_view` | Place view on sheet |

### Query

| Tool | Description |
|------|-------------|
| `revit_get_elements` | Query elements by category/type |
| `revit_get_rooms` | List all rooms with data |
| `revit_get_level_info` | Get level heights and info |
| `revit_take_snapshot` | Capture current view as image |

### Model Management

| Tool | Description |
|------|-------------|
| `revit_new_project` | Start new project from template |
| `revit_save_project` | Save current project |
| `revit_export_ifc` | Export to IFC |
| `revit_export_dwgs` | Export DWG files |

## Usage Patterns

### Create a Floor Plan

```python
# 1. Set up levels
revit_create_levels(["Level 1", "Level 2", "Roof"], heights=[0, 4000, 8000])

# 2. Create grids
revit_create_grids(
    horizontal=["A", "B", "C", "D"],
    vertical=["1", "2", "3", "4"],
    spacing=6000
)

# 3. Draw walls
revit_create_walls([
    {"start": [0, 0], "end": [24000, 0], "level": "Level 1", "height": 4000},
    {"start": [24000, 0], "end": [24000, 18000], "level": "Level 1", "height": 4000},
    # ... more walls
])

# 4. Add openings
revit_place_doors(wall_ids=[...], positions=[...])
revit_place_windows(wall_ids=[...], positions=[...])

# 5. Create rooms
revit_create_rooms(names=["Lobby", "Office 1", "Office 2"], boundaries=[...])
```

### Healthcare Layout (50-bed hospital)

```python
# Department zones
departments = {
    "Emergency": {"beds": 8, "area": 800},
    "Med-Surg": {"beds": 30, "area": 3000},
    "ICU": {"beds": 6, "area": 400},
    "Surgery": {"ors": 4, "area": 600},
    "Imaging": {"rooms": 6, "area": 500},
    "Support": {"area": 1500}
}

# Create levels
revit_create_levels(
    ["Basement", "Ground", "Level 2", "Level 3", "Roof"],
    heights=[-3000, 0, 4500, 9000, 13500]
)

# Model each department...
```

## Error Handling

- If MCP server is unreachable, tools return connection error
- If Revit operation fails, error message includes element context
- Always check return status before proceeding with dependent operations

## Best Practices

1. **Batch operations** - Group similar elements for efficiency
2. **Level-first** - Always create levels before walls/floors
3. **Use grids** - Grid-based modeling is more robust
4. **Verify before placing** - Check family exists before placing
5. **Save frequently** - Call save after major operations
