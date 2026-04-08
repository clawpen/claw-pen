# Claw Pen Installer & Update Guide

## Current Status

### Stable Build (v0.1.0)
- **Commit**: `6df22a5` - "Add shell access and fix WebSocket connectivity"
- **Executable**: `target/release/claw-pen-desktop.exe`
- **Status**: ✅ Built and ready to run
- **Features**: Working agent management, shell access, WebSocket connectivity

### Tailscale Integration (Stashed)
- **Status**: ✅ Complete and tested
- **Location**: Git stash with detailed message
- **Test Results**:
  - Agent IP: 100.125.201.9
  - Successfully connected to Tailnet
  - Can communicate with other Tailnet devices
- **To Restore**: `git stash pop`

## Installer Creation

### Option 1: Simple Distribution (Current)
The built executable can be distributed directly:
```bash
# Build the application
cd tauri-app
cargo tauri build

# Find the executable
../target/release/claw-pen-desktop.exe
```

### Option 2: Professional Installer (Requires Icon Fix)
To create a proper Windows installer with icon:

1. **Create .ico file**:
   - Use online tool: https://convertio.co/png-ico/
   - Convert `icons/32x32.png` to `icons/icon.ico`

2. **Update tauri.conf.json**:
   ```json
   "icon": ["icons/icon.ico"]
   ```

3. **Build installer**:
   ```bash
   cargo tauri build
   # Installer will be in: target/release/bundle/msi/
   ```

## Update System Design

### Auto-Update Architecture
```
┌─────────────────────────────────────────────────────┐
│              Update Flow                            │
├─────────────────────────────────────────────────────┤
│  1. App checks for updates on startup              │
│  2. Compares version with GitHub releases          │
│  3. If update available, download installer        │
│  4. Verify signature/hash                          │
│  5. Apply update and restart                        │
└─────────────────────────────────────────────────────┘
```

### Implementation Plan

1. **Version Management**:
   - Use Git tags for releases (v0.1.0, v0.1.1, etc.)
   - GitHub releases for distribution
   - Semantic versioning

2. **Update Mechanism**:
   ```rust
   // In main.rs
   use tauri_plugin_updater::UpdaterExt;

   #[tauri::command]
   async fn check_updates(app: AppHandle) -> Result<bool, String> {
       if let Some(update) = app.updater()?.check().await? {
           // Download and install update
           update.download_and_install().await?;
           Ok(true)
       } else {
           Ok(false)
       }
   }
   ```

3. **Configuration**:
   ```toml
   # Cargo.toml
   [dependencies]
   tauri-plugin-updater = "2"

   # tauri.conf.json
   "updater": {
     "active": true,
     "endpoints": [
       "https://github.com/yourusername/claw-pen/releases/latest/download"
     ],
     "dialog": true,
     "pubkey": "YOUR_PUBLIC_KEY"
   }
   ```

## Release Workflow

### Creating a Release
```bash
# 1. Update version in tauri.conf.json
# 2. Commit changes
git add .
git commit -m "Release v0.1.0"

# 3. Create tag
git tag v0.1.0

# 4. Build release
cargo tauri build

# 5. Push to GitHub
git push origin main --tags

# 6. Create GitHub release with binaries
```

### Testing Updates
```bash
# Make a small change
# Bump version to v0.1.1
# Create release
# Test update from v0.1.0 to v0.1.1
```

## Deployment Options

### 1. Direct Download (Current)
- Upload executable to website
- Users download and run
- Manual updates

### 2. GitHub Releases (Recommended)
- Automatic build and release
- Built-in update mechanism
- Version tracking

### 3. Windows Store
- Reach wider audience
- Automatic updates
- Requires Microsoft account

### 4. Portable Distribution
- No installation required
- Run from USB drive
- Lower user permissions

## Quick Start

### For Users:
1. Download `claw-pen-desktop.exe`
2. Run the executable
3. Configure Docker connection
4. Start creating agents!

### For Developers:
1. Clone repository
2. Install Rust and Node.js
3. Run `cargo tauri dev` for development
4. Run `cargo tauri build` for release

## Current Working Directory

### Stable State:
- **Commit**: `6df22a5`
- **Branch**: Detached HEAD
- **Status**: Clean (except tauri.conf.json)

### To Continue Tailscale Development:
```bash
# Go back to main branch
git checkout main

# Restore Tailscale changes
git stash pop

# Continue development
```

### To Create Release:
```bash
# Build from stable commit
cargo tauri build

# Or create new branch for release
git checkout -b release/v0.1.0
git push origin release/v0.1.0
```

## Next Steps

1. **Immediate**: Test the built executable
2. **Short-term**: Create proper installer with icon
3. **Medium-term**: Implement auto-update system
4. **Long-term**: Add to Windows Store

## Troubleshooting

### Build Issues:
- Ensure Rust and Node.js are installed
- Check Docker is running
- Verify Tauri CLI is installed: `cargo install tauri-cli`

### Update Issues:
- Check GitHub releases are public
- Verify update endpoint is correct
- Test updater with manual download first

### Icon Issues:
- Use online PNG to ICO converter
- Ensure icon is 32x32 pixels minimum
- Test icon displays correctly in Windows Explorer
