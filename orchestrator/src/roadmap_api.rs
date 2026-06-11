//! Roadmap / Curriculum Tracking API Handlers for Claw Pen

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;
use crate::auth::AuthError;
use crate::roadmap::{
    Roadmap, RoadmapTopic, RoadmapLesson, ProgressStatus,
    StudentProgress, UserMetrics,
};

// ─── Roadmap CRUD ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateRoadmapRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTopicRequest {
    pub title: String,
    pub description: Option<String>,
    pub order_index: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateLessonRequest {
    pub title: String,
    pub description: Option<String>,
    pub order_index: i64,
    pub completion_criteria: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProgressRequest {
    pub status: String,
}

pub async fn list_roadmaps(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    if role != "admin" && role != "teacher" {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    let roadmaps = state.chat_db.list_roadmaps()
        .map_err(|_| AuthError::InvalidCredentials)?;
    Ok(Json(serde_json::json!({ "roadmaps": roadmaps })))
}

pub async fn create_roadmap(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateRoadmapRequest>,
) -> Result<Json<Roadmap>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    if role != "admin" && role != "teacher" {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    let roadmap = state.chat_db.create_roadmap(&req.name, req.description.as_deref())
        .map_err(|_| AuthError::InvalidCredentials)?;
    Ok(Json(roadmap))
}

pub async fn get_roadmap(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    if role != "admin" && role != "teacher" && role != "student" {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    let roadmap = state.chat_db.get_roadmap(&id)
        .map_err(|_| AuthError::InvalidCredentials)?;
    match roadmap {
        Some(r) => Ok(Json(serde_json::json!({ "roadmap": r }))),
        None => Err(AuthError::InvalidCredentials),
    }
}

pub async fn set_active_roadmap(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    if role != "admin" && role != "teacher" {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    state.chat_db.set_active_roadmap(&id)
        .map_err(|_| AuthError::InvalidCredentials)?;
    Ok(Json(serde_json::json!({ "id": id, "active": true })))
}

pub async fn delete_roadmap(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    if role != "admin" && role != "teacher" {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    state.chat_db.delete_roadmap(&id)
        .map_err(|_| AuthError::InvalidCredentials)?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Topic CRUD ───────────────────────────────────────────────────────────

pub async fn create_topic(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(roadmap_id): Path<String>,
    Json(req): Json<CreateTopicRequest>,
) -> Result<Json<RoadmapTopic>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    if role != "admin" && role != "teacher" {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    let topic = state.chat_db.create_topic(
        &roadmap_id, &req.title, req.description.as_deref(), req.order_index
    ).map_err(|_| AuthError::InvalidCredentials)?;
    Ok(Json(topic))
}

// ─── Lesson CRUD ──────────────────────────────────────────────────────────

pub async fn create_lesson(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(topic_id): Path<String>,
    Json(req): Json<CreateLessonRequest>,
) -> Result<Json<RoadmapLesson>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    if role != "admin" && role != "teacher" {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    let lesson = state.chat_db.create_lesson(
        &topic_id, &req.title, req.description.as_deref(),
        req.order_index, req.completion_criteria.as_deref(), req.system_prompt.as_deref()
    ).map_err(|_| AuthError::InvalidCredentials)?;
    Ok(Json(lesson))
}

// ─── Student Progress ───────────────────────────────────────────────────────

pub async fn get_student_progress(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    let caller_id = claims.sub.clone();
    if role != "admin" && role != "teacher" && caller_id != user_id {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    let progress = state.chat_db.get_student_progress(&user_id)
        .map_err(|_| AuthError::InvalidCredentials)?;
    Ok(Json(serde_json::json!({ "progress": progress })))
}

pub async fn update_lesson_progress(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path((user_id, lesson_id)): Path<(String, String)>,
    Json(req): Json<UpdateProgressRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    let caller_id = claims.sub.clone();
    if role != "admin" && role != "teacher" && caller_id != user_id {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    let status = match req.status.as_str() {
        "completed" => ProgressStatus::Completed,
        "in_progress" => ProgressStatus::InProgress,
        _ => ProgressStatus::NotStarted,
    };
    state.chat_db.update_lesson_progress(&user_id, &lesson_id, status)
        .map_err(|_| AuthError::InvalidCredentials)?;
    Ok(Json(serde_json::json!({
        "user_id": user_id,
        "lesson_id": lesson_id,
        "status": req.status,
    })))
}

// ─── User Metrics ─────────────────────────────────────────────────────────

pub async fn get_user_metrics(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    if role != "admin" && role != "teacher" {
        return Err(AuthError::InvalidCredentials);
    }
    drop(auth);

    let metrics = state.chat_db.get_user_metrics()
        .map_err(|_| AuthError::InvalidCredentials)?;
    Ok(Json(serde_json::json!({ "metrics": metrics })))
}

// ─── Active Roadmap for Student ───────────────────────────────────────────

pub async fn get_active_roadmap(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, AuthError> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token)?;
    let role = claims.role.as_deref().unwrap_or("admin");
    if role != "admin" && role != "teacher" && role != "student" {
        return Err(AuthError::InvalidCredentials);
    }
    let user_id = claims.sub.clone();
    drop(auth);

    let roadmaps = state.chat_db.list_roadmaps()
        .map_err(|_| AuthError::InvalidCredentials)?;
    let active = roadmaps.into_iter().find(|r| r.is_active);
    
    if let Some(roadmap) = active {
        let student_progress = state.chat_db.get_student_progress(&user_id)
            .map_err(|_| AuthError::InvalidCredentials)?;
        let progress_map: std::collections::HashMap<&str, &str> = student_progress
            .iter()
            .map(|p| (p.lesson_id.as_str(), p.status.as_str()))
            .collect();
        
        let topics_json: Vec<_> = roadmap.topics.iter().map(|topic| {
            let lessons_json: Vec<_> = topic.lessons.iter().map(|lesson| {
                serde_json::json!({
                    "id": lesson.id,
                    "title": lesson.title,
                    "description": lesson.description,
                    "status": progress_map.get(lesson.id.as_str()).unwrap_or(&"not_started"),
                })
            }).collect();
            serde_json::json!({
                "id": topic.id,
                "name": topic.title,
                "description": topic.description,
                "lessons": lessons_json,
            })
        }).collect();
        
        return Ok(Json(serde_json::json!({
            "has_active": true,
            "roadmap": {
                "id": roadmap.id,
                "name": roadmap.name,
                "description": roadmap.description,
                "topics": topics_json,
            },
        })));
    }
    
    Ok(Json(serde_json::json!({
        "has_active": false,
        "roadmap": null,
    })))
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn extract_token(headers: &axum::http::HeaderMap) -> Result<&str, AuthError> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(AuthError::InvalidToken)
}
