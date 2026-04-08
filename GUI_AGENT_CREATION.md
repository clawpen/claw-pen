# 🎯 How to Create an Agent in the GUI

## The Issue
The GUI couldn't see the agent because:
- Our container was created manually with `docker run`
- The GUI only shows agents created through the orchestrator's API

## Solution: Create Agent Through GUI

### Step 1: Refresh the GUI
1. Go to http://127.0.0.1:5174/
2. Click **"Refresh"** or reload the page

### Step 2: Create New Agent
1. Click **"Create Agent"** button
2. Select template: **"OpenClaw Agent"**
3. Name it: `openclaw-agent` (or any name you prefer)
4. Click **"Create"**

### What Will Happen
The GUI will:
1. ✅ Use our updated template (with `openclaw-agent:custom` image)
2. ✅ Create a new container with Zai GLM-5 configured
3. ✅ Register it in the orchestrator's agent list
4. ✅ Show up in the GUI's agent list

### Current Status
```
✅ Template updated to use correct image (openclaw-agent:custom)
✅ Template configured for Zai GLM-5
✅ Port set to 18790 (orchestrator default)
✅ Custom image has password authentication
```

### Alternative: Quick Test Without GUI

If you want to test immediately without the GUI:

```bash
# Stop the current container
docker stop openclaw-agent

# The GUI will create a new one when you click "Create Agent"
```

---

## Why This Works

The orchestrator has a **template system**:
- Templates are in: `F:\Software\Claw Pen\claw-pen\templates\openclaw-agent\`
- I just updated the template to use our custom image
- When you create an agent through the GUI, it uses this template
- The agent gets automatically registered with the orchestrator

## What's in the Template

```yaml
name: openclaw-agent
display_name: "OpenClaw Agent"
description: "Full OpenClaw instance with Zai GLM-5"

container:
  image: openclaw-agent:custom  # ← Our custom image!
  ports:
    - 18790                    # ← Correct port

openclaw:
  provider: zai
  model: glm-5                  # ← Zai configured
```

---

**Try creating the agent through the GUI now and let me know if it appears! 🚀**
