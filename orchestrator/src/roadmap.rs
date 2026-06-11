//! Roadmap / Curriculum Tracking Module for Claw Pen Chat
//!
//! Provides lesson roadmaps that students follow, with teacher oversight.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Mutex;

// ─── Roadmap Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct Roadmap {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
    pub topics: Vec<RoadmapTopic>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoadmapTopic {
    pub id: String,
    pub roadmap_id: String,
    pub title: String,
    pub description: Option<String>,
    pub order_index: i64,
    pub lessons: Vec<RoadmapLesson>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoadmapLesson {
    pub id: String,
    pub topic_id: String,
    pub title: String,
    pub description: Option<String>,
    pub order_index: i64,
    pub completion_criteria: Option<String>,
    pub system_prompt: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StudentProgress {
    pub user_id: String,
    pub lesson_id: String,
    pub status: ProgressStatus,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub messages_count: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum ProgressStatus {
    NotStarted,
    InProgress,
    Completed,
}

impl ProgressStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProgressStatus::NotStarted => "not_started",
            ProgressStatus::InProgress => "in_progress",
            ProgressStatus::Completed => "completed",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "completed" => ProgressStatus::Completed,
            "in_progress" => ProgressStatus::InProgress,
            _ => ProgressStatus::NotStarted,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserMetrics {
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub role: String,
    pub total_messages: i64,
    pub messages_today: i64,
    pub last_active: Option<String>,
    pub current_lesson: Option<String>,
    pub roadmap_progress_pct: f64,
}

// ─── Schema ────────────────────────────────────────────────────────────────

pub const SCHEMA_ROADMAPS: &str = r#"
CREATE TABLE IF NOT EXISTS roadmaps (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    is_active   INTEGER NOT NULL DEFAULT 0,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS roadmap_topics (
    id           TEXT PRIMARY KEY,
    roadmap_id   TEXT NOT NULL,
    title        TEXT NOT NULL,
    description  TEXT,
    order_index  INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(roadmap_id) REFERENCES roadmaps(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_topics_roadmap ON roadmap_topics(roadmap_id, order_index);

CREATE TABLE IF NOT EXISTS roadmap_lessons (
    id                   TEXT PRIMARY KEY,
    topic_id             TEXT NOT NULL,
    title                TEXT NOT NULL,
    description          TEXT,
    order_index          INTEGER NOT NULL DEFAULT 0,
    completion_criteria  TEXT,
    system_prompt        TEXT,
    created_at           DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(topic_id) REFERENCES roadmap_topics(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_lessons_topic ON roadmap_lessons(topic_id, order_index);

CREATE TABLE IF NOT EXISTS student_progress (
    user_id        TEXT NOT NULL,
    lesson_id      TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'not_started' CHECK(status IN ('not_started','in_progress','completed')),
    started_at     DATETIME,
    completed_at   DATETIME,
    messages_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, lesson_id),
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY(lesson_id) REFERENCES roadmap_lessons(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_progress_user ON student_progress(user_id);

CREATE TABLE IF NOT EXISTS user_metrics (
    user_id         TEXT PRIMARY KEY,
    total_messages  INTEGER NOT NULL DEFAULT 0,
    messages_today  INTEGER NOT NULL DEFAULT 0,
    last_active     DATETIME,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);
"#;

// ─── Roadmap API (impl on ChatDb) ──────────────────────────────────────────

impl crate::chat_db::ChatDb {
    /// Create schema tables for roadmaps
    pub fn migrate_roadmaps(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(SCHEMA_ROADMAPS)?;
        Ok(())
    }

    // ─── Roadmap CRUD ───────────────────────────────────────────────────────

    pub fn create_roadmap(
    &self, name: &str, description: Option<&str>) -> Result<Roadmap> {
        let conn = self.conn.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO roadmaps (id, name, description, is_active, created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            params![id, name, description],
        )?;
        Ok(Roadmap {
            id,
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            is_active: false,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            topics: vec![],
        })
    }

    pub fn list_roadmaps(&self) -> Result<Vec<Roadmap>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, is_active, created_at, updated_at
             FROM roadmaps ORDER BY updated_at DESC",
        )?;
        let mut roadmaps = stmt
            .query_map([], |row| {
                Ok(Roadmap {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    is_active: row.get::<_, i64>(3)? != 0,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    topics: vec![],
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        for roadmap in &mut roadmaps {
            roadmap.topics = Self::load_topics(&conn, &roadmap.id)?;
        }
        Ok(roadmaps)
    }

    pub fn get_roadmap(&self, id: &str) -> Result<Option<Roadmap>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, is_active, created_at, updated_at
             FROM roadmaps WHERE id = ?1",
        )?;
        let mut roadmap = stmt
            .query_map(params![id], |row| {
                Ok(Roadmap {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    is_active: row.get::<_, i64>(3)? != 0,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    topics: vec![],
                })
            })?
            .next()
            .transpose()?;

        if let Some(ref mut r) = roadmap {
            r.topics = Self::load_topics(&conn, &r.id)?;
        }
        Ok(roadmap)
    }

    fn load_topics(conn: &Connection, roadmap_id: &str) -> Result<Vec<RoadmapTopic>> {
        let mut stmt = conn.prepare(
            "SELECT id, roadmap_id, title, description, order_index
             FROM roadmap_topics WHERE roadmap_id = ?1 ORDER BY order_index",
        )?;
        let mut topics = stmt
            .query_map(params![roadmap_id], |row| {
                Ok(RoadmapTopic {
                    id: row.get(0)?,
                    roadmap_id: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    order_index: row.get(4)?,
                    lessons: vec![],
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        for topic in &mut topics {
            topic.lessons = Self::load_lessons(conn, &topic.id)?;
        }
        Ok(topics)
    }

    fn load_lessons(conn: &Connection, topic_id: &str) -> Result<Vec<RoadmapLesson>> {
        let mut stmt = conn.prepare(
            "SELECT id, topic_id, title, description, order_index, completion_criteria, system_prompt, created_at
             FROM roadmap_lessons WHERE topic_id = ?1 ORDER BY order_index",
        )?;
        let lessons = stmt
            .query_map(params![topic_id], |row| {
                Ok(RoadmapLesson {
                    id: row.get(0)?,
                    topic_id: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    order_index: row.get(4)?,
                    completion_criteria: row.get(5)?,
                    system_prompt: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(lessons)
    }

    pub fn set_active_roadmap(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE roadmaps SET is_active = 0", [])?;
        conn.execute(
            "UPDATE roadmaps SET is_active = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn delete_roadmap(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM roadmaps WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ─── Topic CRUD ───────────────────────────────────────────────────────

    pub fn create_topic(
    &self, roadmap_id: &str, title: &str, description: Option<&str>, order_index: i64) -> Result<RoadmapTopic> {
        let conn = self.conn.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO roadmap_topics (id, roadmap_id, title, description, order_index)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, roadmap_id, title, description, order_index],
        )?;
        Ok(RoadmapTopic {
            id,
            roadmap_id: roadmap_id.to_string(),
            title: title.to_string(),
            description: description.map(|s| s.to_string()),
            order_index,
            lessons: vec![],
        })
    }

    // ─── Lesson CRUD ──────────────────────────────────────────────────────

    pub fn create_lesson(
        &self,
        topic_id: &str,
        title: &str,
        description: Option<&str>,
        order_index: i64,
        completion_criteria: Option<&str>,
        system_prompt: Option<&str>,
    ) -> Result<RoadmapLesson> {
        let conn = self.conn.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO roadmap_lessons (id, topic_id, title, description, order_index, completion_criteria, system_prompt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, topic_id, title, description, order_index, completion_criteria, system_prompt],
        )?;
        Ok(RoadmapLesson {
            id,
            topic_id: topic_id.to_string(),
            title: title.to_string(),
            description: description.map(|s| s.to_string()),
            order_index,
            completion_criteria: completion_criteria.map(|s| s.to_string()),
            system_prompt: system_prompt.map(|s| s.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    // ─── Student Progress ─────────────────────────────────────────────────

    pub fn get_student_progress(&self, user_id: &str) -> Result<Vec<StudentProgress>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT user_id, lesson_id, status, started_at, completed_at, messages_count
             FROM student_progress WHERE user_id = ?1",
        )?;
        let progress = stmt
            .query_map(params![user_id], |row| {
                Ok(StudentProgress {
                    user_id: row.get(0)?,
                    lesson_id: row.get(1)?,
                    status: ProgressStatus::parse(&row.get::<_, String>(2)?),
                    started_at: row.get(3)?,
                    completed_at: row.get(4)?,
                    messages_count: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(progress)
    }

    pub fn update_lesson_progress(
        &self,
        user_id: &str,
        lesson_id: &str,
        status: ProgressStatus,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        match status {
            ProgressStatus::InProgress => {
                conn.execute(
                    "INSERT INTO student_progress (user_id, lesson_id, status, started_at, messages_count)
                     VALUES (?1, ?2, ?3, ?4, 1)
                     ON CONFLICT(user_id, lesson_id) DO UPDATE SET
                     status = excluded.status, started_at = COALESCE(started_at, excluded.started_at)",
                    params![user_id, lesson_id, status.as_str(), now],
                )?;
            }
            ProgressStatus::Completed => {
                conn.execute(
                    "INSERT INTO student_progress (user_id, lesson_id, status, started_at, completed_at, messages_count)
                     VALUES (?1, ?2, ?3, ?4, ?4, 1)
                     ON CONFLICT(user_id, lesson_id) DO UPDATE SET
                     status = excluded.status, completed_at = excluded.completed_at",
                    params![user_id, lesson_id, status.as_str(), now],
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn get_all_students_progress(&self) -> Result<Vec<(String, StudentProgress)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT sp.user_id, sp.lesson_id, sp.status, sp.started_at, sp.completed_at, sp.messages_count,
                    u.username, u.display_name
             FROM student_progress sp
             JOIN users u ON u.id = sp.user_id
             ORDER BY sp.user_id, sp.started_at",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(6)?, StudentProgress {
                    user_id: row.get(0)?,
                    lesson_id: row.get(1)?,
                    status: ProgressStatus::parse(&row.get::<_, String>(2)?),
                    started_at: row.get(3)?,
                    completed_at: row.get(4)?,
                    messages_count: row.get(5)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ─── User Metrics ─────────────────────────────────────────────────────

    pub fn record_user_activity(&self, user_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        
        conn.execute(
            "INSERT INTO user_metrics (user_id, total_messages, messages_today, last_active)
             VALUES (?1, 1, 1, ?2)
             ON CONFLICT(user_id) DO UPDATE SET
             total_messages = total_messages + 1,
             messages_today = CASE WHEN date(last_active) = date(?2) THEN messages_today + 1 ELSE 1 END,
             last_active = excluded.last_active",
            params![user_id, now],
        )?;
        Ok(())
    }

    pub fn get_user_metrics(&self) -> Result<Vec<UserMetrics>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT u.id, u.username, u.display_name, u.role,
                    COALESCE(um.total_messages, 0) as total_messages,
                    COALESCE(um.messages_today, 0) as messages_today,
                    um.last_active
             FROM users u
             LEFT JOIN user_metrics um ON um.user_id = u.id
             WHERE u.role = 'student' OR u.role = 'teacher'
             ORDER BY um.last_active DESC NULLS LAST",
        )?;
        let metrics = stmt
            .query_map([], |row| {
                Ok(UserMetrics {
                    user_id: row.get(0)?,
                    username: row.get(1)?,
                    display_name: row.get(2)?,
                    role: row.get(3)?,
                    total_messages: row.get(4)?,
                    messages_today: row.get(5)?,
                    last_active: row.get(6)?,
                    current_lesson: None, // populated separately
                    roadmap_progress_pct: 0.0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(metrics)
    }

    // ─── Auto-advance lesson on message ───────────────────────────────────

    pub fn auto_progress_on_message(&self, user_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let roadmap_id: Option<String> = conn.query_row(
            "SELECT id FROM roadmaps WHERE is_active = 1 LIMIT 1",
            [],
            |row| row.get(0),
        ).optional()?;

        let roadmap_id = match roadmap_id {
            Some(id) => id,
            None => return Ok(None),
        };

        let lesson_id: Option<String> = conn.query_row(
            "SELECT l.id FROM roadmap_lessons l
             JOIN roadmap_topics t ON t.id = l.topic_id
             WHERE t.roadmap_id = ?1
             AND l.id NOT IN (
                 SELECT lesson_id FROM student_progress
                 WHERE user_id = ?2 AND status = 'completed'
             )
             ORDER BY t.order_index, l.order_index
             LIMIT 1",
            params![roadmap_id, user_id],
            |row| row.get(0),
        ).optional()?;

        let lesson_id = match lesson_id {
            Some(id) => id,
            None => return Ok(None),
        };

        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO student_progress (user_id, lesson_id, status, started_at, messages_count)
             VALUES (?1, ?2, 'in_progress', ?3, 1)
             ON CONFLICT(user_id, lesson_id) DO UPDATE SET
             messages_count = messages_count + 1,
             status = CASE WHEN status = 'not_started' THEN 'in_progress' ELSE status END",
            params![user_id, lesson_id, now],
        )?;

        Ok(Some(lesson_id))
    }

    pub fn get_current_lesson(&self, user_id: &str) -> Result<Option<(String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let result: Option<(String, String, String)> = conn.query_row(
            "SELECT l.id, l.title, t.title
             FROM roadmap_lessons l
             JOIN roadmap_topics t ON t.id = l.topic_id
             JOIN roadmaps r ON r.id = t.roadmap_id
             WHERE r.is_active = 1
             AND l.id NOT IN (
                 SELECT lesson_id FROM student_progress
                 WHERE user_id = ?1 AND status = 'completed'
             )
             ORDER BY t.order_index, l.order_index
             LIMIT 1",
            params![user_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?;
        Ok(result)
    }
}

// ─── API Handlers (moved to roadmap_api.rs) ───────────────────────────────
