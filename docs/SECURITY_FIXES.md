# Security Fixes Applied to Claw Pen Orchestrator

This document describes the security improvements made to the Claw Pen orchestrator.

## Date: 2026-02-28

## Summary

The following security vulnerabilities were identified and fixed:

### HIGH PRIORITY

#### 1. Container Network Isolation (`orchestrator/src/container.rs`)

**Issue:** Containers were using `host` network mode, which exposed all host network interfaces to containers.

**Fix:** Changed to `bridge` network mode with explicit port mappings:
- Created a dedicated Docker network `claw-pen-network` for container isolation
- Containers now use bridge mode with only necessary ports exposed
- Ports are bound to `127.0.0.1` only (not `0.0.0.0`)
- Added security options:
  - `no-new-privileges:true` - Prevents privilege escalation
  - `privileged: false` - Explicitly disables privileged mode
  - `cap_drop: ["ALL"]` - Drops all Linux capabilities
  - `cap_add: ["NET_BIND_SERVICE"]` - Adds only necessary capability

#### 2. Command Injection Prevention (`orchestrator/src/validation.rs`, `orchestrator/src/container.rs`, `orchestrator/src/containment.rs`)

**Issue:** Container names and other inputs were not validated, allowing potential command injection.

**Fix:** Created a comprehensive validation module with:
- `validate_container_name()` - Strict whitelist: `^[a-zA-Z0-9_-]+$`
- `validate_env_key()` - Environment variable key validation
- `validate_env_value()` - Environment variable value validation
- `validate_project_name()` - Project name validation
- `validate_tag()` - Tag validation
- `validate_secret_name()` - Secret name validation
- `validate_llm_model()` - LLM model name validation

All validation functions:
- Reject empty strings
- Enforce maximum length limits
- Use strict character whitelists
- Prevent shell metacharacters

#### 3. Path Traversal Prevention (`orchestrator/src/validation.rs`, `orchestrator/src/containment.rs`)

**Issue:** Volume mount paths were not validated, allowing path traversal attacks.

**Fix:** Added path validation:
- `validate_volume_path()` - Validates source paths
- `validate_container_target()` - Validates container target paths
- Checks for `..` in paths
- Blocks access to sensitive paths:
  - `/etc/passwd`, `/etc/shadow`
  - `/root/.ssh`
  - `/var/run/docker.sock`
  - `/proc`, `/sys`
- `build_mounts()` in containment.rs now filters invalid paths

### MEDIUM PRIORITY

#### 4. CORS Configuration (`orchestrator/src/main.rs`)

**Issue:** Using `CorsLayer::permissive()` which allows any origin.

**Fix:** Replaced with explicit CORS configuration:
- Only allows localhost origins for development:
  - `http://localhost:*`
  - `http://127.0.0.1:*`
  - `https://localhost`
  - `tauri://localhost`
  - `https://tauri.localhost`
- Explicit HTTP methods: GET, POST, PUT, DELETE, OPTIONS, PATCH
- Explicit headers: Authorization, Content-Type, Accept, Origin
- Credentials allowed for authenticated requests

#### 5. Input Length Validation (`orchestrator/src/validation.rs`)

**Issue:** No maximum length limits on inputs, allowing potential resource exhaustion.

**Fix:** Added maximum length constants:
- `MAX_NAME_LENGTH: 64` - Container/agent names
- `MAX_ENV_KEY_LENGTH: 128` - Environment variable keys
- `MAX_ENV_VALUE_LENGTH: 4096` - Environment variable values
- `MAX_SECRET_VALUE_LENGTH: 65536` - Secret values (64KB)
- `MAX_VOLUMES_COUNT: 32` - Maximum volumes per container
- `MAX_ENV_VARS_COUNT: 128` - Maximum environment variables
- `MAX_SECRETS_COUNT: 64` - Maximum secrets
- `MAX_TAGS_COUNT: 32` - Maximum tags
- `MAX_PROJECT_NAME_LENGTH: 128` - Project names
- `MAX_DESCRIPTION_LENGTH: 1024` - Descriptions
- `MAX_LLM_MODEL_LENGTH: 256` - LLM model names

#### 6. Error Message Sanitization (`orchestrator/src/validation.rs`, `orchestrator/src/api.rs`)

**Issue:** Error messages could expose internal paths, container IDs, and IP addresses.

**Fix:** Added `sanitize_error_message()` function that:
- Replaces filesystem paths with `[PATH]`
- Replaces container IDs (64-char hex strings) with `[ID]`
- Replaces IP addresses with `[IP]`
- Truncates messages to 500 characters

## Configuration Changes

### Required Docker Network

The orchestrator will automatically create a Docker network named `claw-pen-network` with:
- Driver: `bridge`
- Subnet: `172.28.0.0/16`
- Labels: `claw-pen=true`, `purpose=agent-isolation`

### Allowed Volume Mount Bases (Future)

For production use, volume mounts should be restricted to:
- `/data/claw-pen/volumes`
- `/data/claw-pen/projects`
- `/var/lib/claw-pen/volumes`

For development, additional paths are allowed:
- `/tmp/claw-pen-volumes`
- `./test-volumes`

## API Changes

### Validation Errors

Invalid inputs now return `400 Bad Request` with descriptive error messages (sanitized):
```json
{
  "error": "Container name contains invalid characters. Only alphanumeric, underscore (_), and hyphen (-) are allowed"
}
```

### Environment Variable Limits

- Maximum 128 environment variables per container
- Keys must start with letter or underscore
- Values limited to 4KB each

## Testing Recommendations

1. **Container Isolation:**
   - Verify containers cannot access host network
   - Verify containers are on isolated Docker network
   - Verify ports are only bound to localhost

2. **Input Validation:**
   - Test with special characters in names: `; rm -rf /`, `$(whoami)`, etc.
   - Test with path traversal: `../../../etc/passwd`
   - Test with oversized inputs

3. **CORS:**
   - Verify non-localhost origins are rejected
   - Verify credentials are properly handled

## Files Modified

- `orchestrator/src/validation.rs` (new file)
- `orchestrator/src/main.rs`
- `orchestrator/src/container.rs`
- `orchestrator/src/containment.rs`
- `orchestrator/src/api.rs`
- `orchestrator/Cargo.toml` (added `regex` dependency)

## Dependencies Added

- `regex = "1"` - For error message sanitization
