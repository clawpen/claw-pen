//! Input validation and sanitization for security
//!
//! This module provides centralized validation logic to prevent:
//! - Command injection attacks
//! - Path traversal attacks
//! - Resource exhaustion via oversized inputs
//! - Invalid container names and identifiers

use anyhow::{anyhow, Result};
use std::path::{Component, Path, PathBuf};

/// Maximum lengths for various input fields
pub const MAX_NAME_LENGTH: usize = 64;
pub const MAX_ENV_KEY_LENGTH: usize = 128;
pub const MAX_ENV_VALUE_LENGTH: usize = 4096;
pub const MAX_SECRET_VALUE_LENGTH: usize = 65536; // 64KB
pub const MAX_VOLUMES_COUNT: usize = 32;
pub const MAX_ENV_VARS_COUNT: usize = 128;
pub const MAX_SECRETS_COUNT: usize = 64;
pub const MAX_TAGS_COUNT: usize = 32;
pub const MAX_PROJECT_NAME_LENGTH: usize = 128;
pub const MAX_DESCRIPTION_LENGTH: usize = 1024;
pub const MAX_LLM_MODEL_LENGTH: usize = 256;

/// Allowed base directories for volume mounts
/// These are the only directories from which containers can mount volumes
pub const ALLOWED_MOUNT_BASES: &[&str] = &[
    "/data/claw-pen/volumes",
    "/data/claw-pen/projects",
    "/var/lib/claw-pen/volumes",
];

/// Development/testing mount bases (only allowed in debug builds)
#[cfg(debug_assertions)]
pub const DEV_MOUNT_BASES: &[&str] = &["/tmp/claw-pen-volumes", "./test-volumes"];

/// Validate a container name against a strict whitelist
/// 
/// Container names must:
/// - Be 1-64 characters long
/// - Contain only alphanumeric characters, underscores, and hyphens
/// - Not start with a hyphen
/// - Not be empty
pub fn validate_container_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("Container name cannot be empty"));
    }

    if name.len() > MAX_NAME_LENGTH {
        return Err(anyhow!(
            "Container name too long (max {} characters)",
            MAX_NAME_LENGTH
        ));
    }

    if name.starts_with('-') {
        return Err(anyhow!("Container name cannot start with a hyphen"));
    }

    // Strict whitelist: only alphanumeric, underscore, and hyphen
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

    if !valid {
        return Err(anyhow!(
            "Container name contains invalid characters. Only alphanumeric, underscore (_), and hyphen (-) are allowed"
        ));
    }

    Ok(())
}

/// Validate an agent ID
/// Agent IDs are typically hex strings or UUIDs, so we allow a broader character set
pub fn validate_agent_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(anyhow!("Agent ID cannot be empty"));
    }

    if id.len() > 128 {
        return Err(anyhow!("Agent ID too long"));
    }

    // Allow alphanumeric, hyphens (for UUIDs), and colons (for container IDs)
    let valid = id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':' || c == '_');

    if !valid {
        return Err(anyhow!("Agent ID contains invalid characters"));
    }

    Ok(())
}

/// Validate a project name
pub fn validate_project_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("Project name cannot be empty"));
    }

    if name.len() > MAX_PROJECT_NAME_LENGTH {
        return Err(anyhow!(
            "Project name too long (max {} characters)",
            MAX_PROJECT_NAME_LENGTH
        ));
    }

    // Allow alphanumeric, spaces, hyphens, underscores
    let valid = name
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_');

    if !valid {
        return Err(anyhow!(
            "Project name contains invalid characters"
        ));
    }

    Ok(())
}

/// Validate a tag
pub fn validate_tag(tag: &str) -> Result<()> {
    if tag.is_empty() {
        return Err(anyhow!("Tag cannot be empty"));
    }

    if tag.len() > 64 {
        return Err(anyhow!("Tag too long"));
    }

    let valid = tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/');

    if !valid {
        return Err(anyhow!("Tag contains invalid characters"));
    }

    Ok(())
}

/// Validate an environment variable key
pub fn validate_env_key(key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(anyhow!("Environment variable key cannot be empty"));
    }

    if key.len() > MAX_ENV_KEY_LENGTH {
        return Err(anyhow!(
            "Environment variable key too long (max {} characters)",
            MAX_ENV_KEY_LENGTH
        ));
    }

    // Env keys must start with letter or underscore, followed by alphanumeric or underscore
    let mut chars = key.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(anyhow!(
            "Environment variable key must start with a letter or underscore"
        ));
    }

    let valid = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(anyhow!(
            "Environment variable key contains invalid characters"
        ));
    }

    Ok(())
}

/// Validate an environment variable value
pub fn validate_env_value(value: &str) -> Result<()> {
    if value.len() > MAX_ENV_VALUE_LENGTH {
        return Err(anyhow!(
            "Environment variable value too long (max {} characters)",
            MAX_ENV_VALUE_LENGTH
        ));
    }

    // Check for null bytes which could cause issues
    if value.contains('\0') {
        return Err(anyhow!("Environment variable value cannot contain null bytes"));
    }

    Ok(())
}

/// Validate a secret value
pub fn validate_secret_value(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(anyhow!("Secret value cannot be empty"));
    }

    if value.len() > MAX_SECRET_VALUE_LENGTH {
        return Err(anyhow!(
            "Secret value too long (max {} bytes)",
            MAX_SECRET_VALUE_LENGTH
        ));
    }

    Ok(())
}

/// Validate a secret name
pub fn validate_secret_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("Secret name cannot be empty"));
    }

    if name.len() > 64 {
        return Err(anyhow!("Secret name too long (max 64 characters)"));
    }

    // Secret names should be filesystem-safe
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');

    if !valid {
        return Err(anyhow!(
            "Secret name contains invalid characters. Use alphanumeric, underscore, hyphen, or dot"
        ));
    }

    // Prevent path traversal in secret names
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(anyhow!("Secret name cannot contain path separators or '..'"));
    }

    Ok(())
}

/// Validate a volume mount path for path traversal attacks
/// 
/// Returns the canonicalized path if valid, or an error if the path is unsafe
pub fn validate_volume_path(source: &str) -> Result<PathBuf> {
    // Check for empty path
    if source.is_empty() {
        return Err(anyhow!("Volume source path cannot be empty"));
    }

    // Check for obvious path traversal attempts
    if source.contains("..") {
        return Err(anyhow!("Volume path cannot contain '..' (path traversal denied)"));
    }

    // Check for null bytes
    if source.contains('\0') {
        return Err(anyhow!("Volume path cannot contain null bytes"));
    }

    // Convert to Path and check components
    let path = Path::new(source);
    
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(anyhow!("Volume path cannot contain '..' (path traversal denied)"));
            }
            Component::Prefix(_) => {
                // Windows drive letter or UNC path - reject for consistency
                return Err(anyhow!("Volume path cannot use prefix components"));
            }
            _ => {}
        }
    }

    // Canonicalize the path to resolve any remaining tricks
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| anyhow!("Failed to resolve volume path: {}", e))?;

    // Check if the canonical path is within an allowed base directory
    if !is_path_allowed(&canonical) {
        return Err(anyhow!(
            "Volume path must be within an allowed directory. Allowed bases: {}",
            ALLOWED_MOUNT_BASES.join(", ")
        ));
    }

    Ok(canonical)
}

/// Check if a canonical path is within an allowed base directory
fn is_path_allowed(path: &Path) -> bool {
    // In debug builds, also check development mount bases
    #[cfg(debug_assertions)]
    let all_bases: Vec<&str> = ALLOWED_MOUNT_BASES
        .iter()
        .chain(DEV_MOUNT_BASES.iter())
        .copied()
        .collect();
    
    #[cfg(not(debug_assertions))]
    let all_bases = ALLOWED_MOUNT_BASES;

    for base in all_bases {
        let base_path = Path::new(base);
        if let Ok(canonical_base) = std::fs::canonicalize(base_path) {
            if path.starts_with(&canonical_base) {
                return true;
            }
        }
    }

    false
}

/// Validate a container target path (path inside container)
pub fn validate_container_target(target: &str) -> Result<()> {
    if target.is_empty() {
        return Err(anyhow!("Container target path cannot be empty"));
    }

    // Must be an absolute path
    if !target.starts_with('/') {
        return Err(anyhow!("Container target path must be absolute (start with /)"));
    }

    // Check for path traversal
    if target.contains("..") {
        return Err(anyhow!("Container target path cannot contain '..'"));
    }

    // Check for null bytes
    if target.contains('\0') {
        return Err(anyhow!("Container target path cannot contain null bytes"));
    }

    // Check for suspicious paths
    let suspicious = [
        "/etc/passwd",
        "/etc/shadow",
        "/root",
        "/var/run/docker.sock",
        "/var/run/containerd.sock",
        "/proc",
        "/sys",
    ];

    for suspicious_path in suspicious {
        if target.starts_with(suspicious_path) {
            return Err(anyhow!(
                "Container target path '{}' is not allowed for security reasons",
                target
            ));
        }
    }

    Ok(())
}

/// Validate LLM model name
pub fn validate_llm_model(model: &str) -> Result<()> {
    if model.is_empty() {
        return Err(anyhow!("LLM model name cannot be empty"));
    }

    if model.len() > MAX_LLM_MODEL_LENGTH {
        return Err(anyhow!(
            "LLM model name too long (max {} characters)",
            MAX_LLM_MODEL_LENGTH
        ));
    }

    // Allow alphanumeric, hyphens, underscores, dots, colons, and forward slashes
    let valid = model
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ':' || c == '/');

    if !valid {
        return Err(anyhow!("LLM model name contains invalid characters"));
    }

    Ok(())
}

/// Validate description text
pub fn validate_description(desc: &str) -> Result<()> {
    if desc.len() > MAX_DESCRIPTION_LENGTH {
        return Err(anyhow!(
            "Description too long (max {} characters)",
            MAX_DESCRIPTION_LENGTH
        ));
    }

    // Check for null bytes
    if desc.contains('\0') {
        return Err(anyhow!("Description cannot contain null bytes"));
    }

    Ok(())
}

/// Sanitize an error message for client display
/// 
/// This removes potentially sensitive information like:
/// - Internal filesystem paths
/// - Container IDs
/// - Hostnames and IP addresses
/// - Stack traces
pub fn sanitize_error_message(error: &str) -> String {
    let mut sanitized = error.to_string();

    // Replace common path patterns
    let path_patterns = [
        "/data/claw-pen/",
        "/var/lib/",
        "/etc/",
        "/home/",
        "/root/",
        "/usr/",
        "/opt/",
        "C:\\",
        "\\\\",
    ];

    for pattern in path_patterns {
        if sanitized.contains(pattern) {
            // Find and replace the entire path
            if let Some(start) = sanitized.find(pattern) {
                let end = sanitized[start..]
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                    .map(|i| start + i)
                    .unwrap_or(sanitized.len());
                sanitized.replace_range(start..end, "[PATH]");
            }
        }
    }

    // Replace container IDs (long hex strings)
    let container_id_pattern = regex::Regex::new(r"[a-f0-9]{64}").unwrap();
    sanitized = container_id_pattern.replace(&sanitized, "[ID]").to_string();

    // Replace IP addresses
    let ip_pattern = regex::Regex::new(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}").unwrap();
    sanitized = ip_pattern.replace(&sanitized, "[IP]").to_string();

    // Truncate if too long
    if sanitized.len() > 500 {
        sanitized.truncate(500);
        sanitized.push_str("...");
    }

    sanitized
}

/// Validate memory configuration
pub fn validate_memory_mb(memory_mb: u32) -> Result<()> {
    if memory_mb == 0 {
        return Err(anyhow!("Memory limit must be greater than 0"));
    }

    if memory_mb > 65536 {
        return Err(anyhow!("Memory limit cannot exceed 65536 MB (64 GB)"));
    }

    Ok(())
}

/// Validate CPU configuration
pub fn validate_cpu_cores(cpu_cores: f32) -> Result<()> {
    if cpu_cores <= 0.0 {
        return Err(anyhow!("CPU cores must be greater than 0"));
    }

    if cpu_cores > 128.0 {
        return Err(anyhow!("CPU cores cannot exceed 128"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_container_name() {
        assert!(validate_container_name("my-agent").is_ok());
        assert!(validate_container_name("my_agent").is_ok());
        assert!(validate_container_name("agent123").is_ok());
        assert!(validate_container_name("Agent_Test-1").is_ok());

        assert!(validate_container_name("").is_err());
        assert!(validate_container_name("-agent").is_err());
        assert!(validate_container_name("agent name").is_err());
        assert!(validate_container_name("agent;rm -rf /").is_err());
        assert!(validate_container_name("$(whoami)").is_err());
        assert!(validate_container_name(&"a".repeat(65)).is_err());
    }

    #[test]
    fn test_validate_env_key() {
        assert!(validate_env_key("API_KEY").is_ok());
        assert!(validate_env_key("_PRIVATE").is_ok());
        assert!(validate_env_key("myVar123").is_ok());

        assert!(validate_env_key("").is_err());
        assert!(validate_env_key("123KEY").is_err());
        assert!(validate_env_key("MY-KEY").is_err());
    }

    #[test]
    fn test_sanitize_error_message() {
        let error = "Failed to read /data/claw-pen/secrets/api.key: permission denied";
        let sanitized = sanitize_error_message(error);
        assert!(!sanitized.contains("/data/claw-pen/secrets"));
        assert!(sanitized.contains("[PATH]"));
    }
}
