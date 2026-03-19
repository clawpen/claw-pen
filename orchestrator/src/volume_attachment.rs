// === Agent Volume Attachment ===

use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::types;

// Re-export the request types from types module
pub use crate::types::{AttachVolumeRequest, DetachVolumeRequest};

/// List volumes attached to an agent
pub async fn list_agent_volumes(
    State(state): State<Arc<crate::AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<types::VolumeMount>>, (axum::http::StatusCode, String)> {
    let containers = state.containers.read().await;
    let agent = containers
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| (axum::http::StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    Ok(Json(agent.config.volumes.clone()))
}

/// Attach a volume to an agent (requires restart)
pub async fn attach_volume_to_agent(
    State(state): State<Arc<crate::AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AttachVolumeRequest>,
) -> Result<Json<types::AgentContainer>, (axum::http::StatusCode, String)> {
    let mut containers = state.containers.write().await;
    let agent = containers
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| (axum::http::StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    // Check if volume exists
    let volumes = state.volumes.read().await;
    let volume = volumes
        .iter()
        .find(|v| v.id == req.volume_id || v.name == req.volume_id)
        .ok_or_else(|| (axum::http::StatusCode::NOT_FOUND, "Volume not found".to_string()))?;

    // Clone needed data before dropping volumes
    let volume_id = volume.id.clone();
    let volume_name = volume.name.clone();
    let agent_name_for_log = agent.name.clone();  // Clone agent name too
    let host_path = volume.host_path.clone();
    let default_target = volume.default_target.clone();
    let read_only = volume.read_only;

    // Create volume mount
    let mount = types::VolumeMount {
        source: match (&host_path, &volume_id) {
            (Some(path), _) => path.clone(),
            (None, _) => {
                // Managed volume - construct path
                format!(
                    "{}/volumes/{}",
                    state.data_dir.display(),
                    volume_id
                )
            }
        },
        target: req.target.unwrap_or_else(|| default_target.clone()),
        read_only: req.read_only.unwrap_or(read_only),
    };

    // Check if volume is already attached with same target
    let already_attached = agent.config.volumes.iter().any(|v| {
        v.source == mount.source && v.target == mount.target
    });

    if already_attached {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            format!("Volume '{}' is already attached to target '{}'", volume_name, mount.target)
        ));
    }

    // Add to agent's volumes
    agent.config.volumes.push(mount.clone());

    // Update volume's attached_agents list
    drop(volumes);
    let mut volumes = state.volumes.write().await;
    if let Some(vol) = volumes.iter_mut().find(|v| v.id == volume_id) {
        if !vol.attached_agents.contains(&id) {
            vol.attached_agents.push(id.clone());
        }
    }
    drop(volumes);

    // Persist changes
    if let Err(e) = crate::storage::upsert_agent(&crate::storage::to_stored_agent(agent)) {
        tracing::warn!("Failed to persist agent update: {}", e);
    }
    crate::save_volumes(&state.data_dir, &state.volumes.read().await);

    tracing::info!(
        "Attached volume '{}' to agent '{}', restarting agent...",
        volume_name,
        agent_name_for_log
    );

    // Persist changes
    if let Err(e) = crate::storage::upsert_agent(&crate::storage::to_stored_agent(agent)) {
        tracing::warn!("Failed to persist agent update: {}", e);
    }
    crate::save_volumes(&state.data_dir, &state.volumes.read().await);

    tracing::info!(
        "Attached volume '{}' to agent '{}', restarting agent synchronously...",
        volume_name,
        agent_name_for_log
    );

    // Clone data for restart
    let agent_id = agent.id.clone();
    let agent_runtime = agent.runtime.clone();
    let agent_config = agent.config.clone();
    let agent_name = agent.name.clone();
    let state_clone = state.clone();

    // Mark agent as stopping
    agent.status = types::AgentStatus::Stopping;
    drop(containers);

    // Do restart SYNCHRONOUSLY to ensure everything is updated before returning
    let runtime: &dyn crate::container::ContainerRuntime =
        if agent_runtime.as_deref() == Some("exo") {
            &state_clone.exo_runtime
        } else {
            &state_clone.runtime
        };

    // Stop and remove container (try both ID and name)
    let _ = runtime.stop_container(&agent_id).await;
    let _ = runtime.stop_container(&agent_name).await;
    let _ = runtime.delete_container(&agent_id).await;
    let _ = runtime.delete_container(&agent_name).await;

    // Remove old agent from storage to prevent duplicates
    let _ = crate::storage::remove_agent(&agent_id);

    // Wait for container to be fully removed
    for _ in 0..10 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if !runtime.container_exists(&agent_name).await.unwrap_or(true) {
            break;
        }
    }

    // Start the agent again with retry logic
    let new_container_id = loop {
        match runtime.create_container(&agent_name, &agent_config).await {
            Ok(id) => break id,
            Err(e) if e.to_string().contains("409") || e.to_string().contains("already in use") => {
                tracing::warn!("Container name still in use, waiting and retrying...");
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            }
            Err(e) => {
                tracing::error!("Failed to recreate container: {}", e);
                return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            }
        }
    };

    if let Err(e) = runtime.start_container(&new_container_id).await {
        tracing::error!("Failed to start container: {}", e);
        return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    // Update agent in state with new container ID and status
    let mut containers = state_clone.containers.write().await;
    let agent_pos = containers.iter().position(|c| c.id == agent_id);
    if let Some(pos) = agent_pos {
        containers[pos].id = new_container_id.clone();
        containers[pos].status = types::AgentStatus::Running;

        // Rebuild agent index since ID changed
        let mut index = state_clone.agent_index.write().await;
        *index = containers
            .iter()
            .enumerate()
            .map(|(idx, agent)| (agent.id.clone(), idx))
            .collect();

        // Persist the updated agent with new ID
        if let Err(e) = crate::storage::upsert_agent(&crate::storage::to_stored_agent(&containers[pos])) {
            tracing::warn!("Failed to persist agent after restart: {}", e);
        }
    }

    tracing::info!("Agent '{}' restarted successfully with new ID: {}", agent_name, new_container_id);

    // Return the updated agent with new ID
    let containers = state_clone.containers.read().await;
    let updated_agent = containers.iter().find(|c| c.id == new_container_id)
        .ok_or_else(|| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Agent not found after restart".to_string()))?;

    Ok(Json(updated_agent.clone()))
}

/// Detach a volume from an agent (requires restart)
pub async fn detach_volume_from_agent(
    State(state): State<Arc<crate::AppState>>,
    Path(id): Path<String>,
    Json(req): Json<DetachVolumeRequest>,
) -> Result<Json<types::AgentContainer>, (axum::http::StatusCode, String)> {
    let mut containers = state.containers.write().await;
    let agent = containers
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| (axum::http::StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    // Find and remove the volume mount
    if let Some(ref volume_id) = req.volume_id {
        // Detach by volume ID - find the mount and remove it
        agent.config.volumes = agent.config.volumes
            .iter()
            .filter(|v| {
                // Match by volume ID or by source path containing volume ID
                let source_matches = v.source.contains(volume_id) ||
                                     v.source.ends_with(volume_id);
                let target_matches = if let Some(ref target) = req.target {
                    v.target == *target
                } else {
                    true
                };
                !(source_matches && target_matches)  // Keep only volumes that DON'T match
            })
            .cloned()
            .collect();
    } else if let Some(ref target) = req.target {
        // Detach by target path
        agent.config.volumes = agent.config.volumes
            .iter()
            .filter(|v| v.target != *target)
            .cloned()
            .collect();
    } else {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Must specify volume_id or target".to_string()));
    }

    // Update volume's attached_agents list
    for mount in &agent.config.volumes {
        // Find the corresponding volume
        let all_volumes = state.volumes.read().await;
        let vol_id = all_volumes.iter().find(|v| {
            mount.source.contains(&v.id) ||
            mount.source == format!("{}/volumes/{}", state.data_dir.display(), v.id)
        }).map(|v| v.id.clone());
        drop(all_volumes);

        if let Some(vol_id) = vol_id {
            let mut volumes = state.volumes.write().await;
            if let Some(vol) = volumes.iter_mut().find(|v| v.id == vol_id) {
                vol.attached_agents.retain(|aid| aid != &id);
            }
        }
    }

    tracing::info!(
        "Detached volume from agent '{}', restarting agent synchronously...",
        agent.name
    );

    // Restart agent to apply changes
    let agent_id = agent.id.clone();
    let agent_runtime = agent.runtime.clone();
    let agent_config = agent.config.clone();
    let agent_name = agent.name.clone();
    let state_clone = state.clone();

    // Mark agent as stopping
    agent.status = types::AgentStatus::Stopping;
    drop(containers);

    // Do restart SYNCHRONOUSLY
    let runtime: &dyn crate::container::ContainerRuntime =
        if agent_runtime.as_deref() == Some("exo") {
            &state_clone.exo_runtime
        } else {
            &state_clone.runtime
        };

    // Stop and remove container (try both ID and name)
    let _ = runtime.stop_container(&agent_id).await;
    let _ = runtime.stop_container(&agent_name).await;
    let _ = runtime.delete_container(&agent_id).await;
    let _ = runtime.delete_container(&agent_name).await;

    // Remove old agent from storage to prevent duplicates
    let _ = crate::storage::remove_agent(&agent_id);

    // Wait for container to be fully removed
    for _ in 0..10 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if !runtime.container_exists(&agent_name).await.unwrap_or(true) {
            break;
        }
    }

    // Start the agent again with retry logic
    let new_container_id = loop {
        match runtime.create_container(&agent_name, &agent_config).await {
            Ok(id) => break id,
            Err(e) if e.to_string().contains("409") || e.to_string().contains("already in use") => {
                tracing::warn!("Container name still in use, waiting and retrying...");
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            }
            Err(e) => {
                tracing::error!("Failed to recreate container: {}", e);
                return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            }
        }
    };

    if let Err(e) = runtime.start_container(&new_container_id).await {
        tracing::error!("Failed to start container: {}", e);
        return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    // Update agent in state with new container ID and status
    let mut containers = state_clone.containers.write().await;
    let agent_pos = containers.iter().position(|c| c.id == agent_id);
    if let Some(pos) = agent_pos {
        containers[pos].id = new_container_id.clone();
        containers[pos].status = types::AgentStatus::Running;

        // Rebuild agent index since ID changed
        let mut index = state_clone.agent_index.write().await;
        *index = containers
            .iter()
            .enumerate()
            .map(|(idx, agent)| (agent.id.clone(), idx))
            .collect();

        // Persist the updated agent with new ID
        if let Err(e) = crate::storage::upsert_agent(&crate::storage::to_stored_agent(&containers[pos])) {
            tracing::warn!("Failed to persist agent after restart: {}", e);
        }
    }

    tracing::info!("Agent '{}' restarted successfully after volume detach with new ID: {}", agent_name, new_container_id);

    // Return the updated agent with new ID
    let containers = state_clone.containers.read().await;
    let updated_agent = containers.iter().find(|c| c.id == new_container_id)
        .ok_or_else(|| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Agent not found after restart".to_string()))?;

    Ok(Json(updated_agent.clone()))
}
