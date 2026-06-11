//! Chat persistence layer — users, classes, conversations, messages.
//!
//! Schema ported (and adapted) from the ANDORCHAT TypeScript prototype.
//! Classroom-specific changes vs the original company-chat schema:
//!   - `users.role` (student | teacher | observer | admin) replaces flat membership
//!   - `classes` + `class_members` for Brightspace roster sync
//!   - `conversations` repurposed for student↔agent threads + teacher channels
//!   - Dropped: build_environments, docker_containers, build_artifacts,
//!     session_memories, session_catalog, remote_sessions (coding-agent leftovers
//!     that don't apply to classroom use)
//!
//! Storage strategy:
//!   - Metadata (users, conversations, participants, classes) → SQLite.
//!   - Message content stays in JSONL on disk for now (`data/agents/<name>/
//!     conversations/<session_id>.jsonl`). The `messages` table holds only
//!     pointers + indexed metadata for fast querying.
//!   - Phase 2 may move full transcripts into SQLite once we have FTS.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

pub use crate::roadmap::{Roadmap, RoadmapTopic, RoadmapLesson, StudentProgress, ProgressStatus, UserMetrics};

/// Single-process Mutex wrapper. SQLite handles its own internal locking; the
/// Mutex prevents Rust borrow conflicts only. For multi-thread workloads this
/// is fine — SQLite's WAL mode lets readers proceed without blocking.
pub struct ChatDb {
    pub(crate) conn: Mutex<Connection>,
}

impl ChatDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).context("opening chat.db")?;

        // WAL mode for concurrent readers + a single writer.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        let db = Self { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    /// Apply all schema migrations idempotently.
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(SCHEMA_V1)?;

        // Schema version bookkeeping
        conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );")?;
        conn.execute(
            "INSERT OR IGNORE INTO schema_version (version) VALUES (1)",
            [],
        )?;

        // Migration: add approval_status if not exists (schema v2)
        let has_approval_status: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('users') WHERE name = 'approval_status'",
            [],
            |row| row.get(0),
        )?;
        if has_approval_status == 0 {
            conn.execute(
                "ALTER TABLE users ADD COLUMN approval_status TEXT NOT NULL DEFAULT 'pending' CHECK(approval_status IN ('pending','approved','rejected'))",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_users_status ON users(approval_status)",
                [],
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (2)",
                [],
            )?;
            tracing::info!("Migrated schema v2: added users.approval_status");
        }

        // Migration: add color to conversations if not exists (schema v7)
        let has_color: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'color'",
            [],
            |row| row.get(0),
        )?;
        if has_color == 0 {
            conn.execute(
                "ALTER TABLE conversations ADD COLUMN color TEXT DEFAULT '#2a7f7f'",
                [],
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (7)",
                [],
            )?;
            tracing::info!("Migrated schema v7: added conversations.color");
        }

        // Migration: add title to conversations if not exists (schema v3)
        let has_title: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'title'",
            [],
            |row| row.get(0),
        )?;
        if has_title == 0 {
            conn.execute(
                "ALTER TABLE conversations ADD COLUMN title TEXT DEFAULT 'New Chat'",
                [],
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (3)",
                [],
            )?;
            tracing::info!("Migrated schema v3: added conversations.title");
        }

        // Migration: add system_prompts table (schema v4)
        let has_system_prompts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='system_prompts'",
            [],
            |row| row.get(0),
        )?;
        if has_system_prompts == 0 {
            conn.execute_batch(SCHEMA_SYSTEM_PROMPTS)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (4)",
                [],
            )?;
            tracing::info!("Migrated schema v4: added system_prompts table");
        }

        // Migration: add system_prompt column to conversations (schema v6)
        let has_conv_prompt: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'system_prompt'",
            [],
            |row| row.get(0),
        )?;
        if has_conv_prompt == 0 {
            conn.execute(
                "ALTER TABLE conversations ADD COLUMN system_prompt TEXT",
                [],
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (6)",
                [],
            )?;
            tracing::info!("Migrated schema v6: added conversations.system_prompt");
        }
        let has_user_prompt: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('users') WHERE name = 'system_prompt'",
            [],
            |row| row.get(0),
        )?;
        if has_user_prompt == 0 {
            conn.execute(
                "ALTER TABLE users ADD COLUMN system_prompt TEXT",
                [],
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (5)",
                [],
            )?;
            tracing::info!("Migrated schema v5: added users.system_prompt");
        }

        // Migration: add roadmaps schema (schema v6)
        let has_roadmaps: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='roadmaps'",
            [],
            |row| row.get(0),
        )?;
        if has_roadmaps == 0 {
            use crate::roadmap::SCHEMA_ROADMAPS;
            conn.execute_batch(SCHEMA_ROADMAPS)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (6)",
                [],
            )?;
            tracing::info!("Migrated schema v6: added roadmaps, topics, lessons, progress, metrics tables");
        }

        Ok(())
    }

    // ─── Users ──────────────────────────────────────────────────────────────

    pub fn create_user(&self, user: &NewUser) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, display_name, password_hash, role, approval_status, lti_subject, lti_issuer)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                user.id, user.username, user.display_name,
                user.password_hash, user.role.as_str(), user.approval_status.as_str(),
                user.lti_subject, user.lti_issuer,
            ],
        )?;
        Ok(())
    }

    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, username, display_name, password_hash, role, approval_status,
                    lti_subject, lti_issuer, created_at, system_prompt
             FROM users WHERE username = ?1",
            params![username],
            row_to_user,
        )
        .optional()
        .context("get_user_by_username")
    }

    pub fn get_user_by_lti(&self, issuer: &str, subject: &str) -> Result<Option<User>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, username, display_name, password_hash, role, approval_status,
                    lti_subject, lti_issuer, created_at, system_prompt
             FROM users WHERE lti_issuer = ?1 AND lti_subject = ?2",
            params![issuer, subject],
            row_to_user,
        )
        .optional()
        .context("get_user_by_lti")
    }

    pub fn get_or_create_user_from_claims(&self, user_id: &str, username: Option<&str>, role: UserRole) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM users WHERE id = ?1",
            params![user_id],
            |row| row.get(0),
        )?;
        if exists == 0 {
            conn.execute(
                "INSERT INTO users (id, username, display_name, password_hash, role, approval_status, lti_subject, lti_issuer)
                 VALUES (?1, ?2, ?2, NULL, ?3, 'approved', NULL, NULL)",
                params![user_id, username.unwrap_or(user_id), role.as_str()],
            )?;
            tracing::info!("Created synthetic user for {} (role: {})", user_id, role.as_str());
        }
        Ok(())
    }

    pub fn get_user(&self, id: &str) -> Result<Option<User>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, username, display_name, password_hash, role, approval_status,
                    lti_subject, lti_issuer, created_at, system_prompt
             FROM users WHERE id = ?1",
            params![id],
            row_to_user,
        )
        .optional()
        .context("get_user")
    }

    // ─── Classes ────────────────────────────────────────────────────────────

    pub fn create_class(&self, class: &NewClass) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO classes (id, name, lti_context_id, lti_issuer, owner_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![class.id, class.name, class.lti_context_id, class.lti_issuer, class.owner_id],
        )?;
        Ok(())
    }

    pub fn add_class_member(&self, class_id: &str, user_id: &str, role: ClassRole) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO class_members (class_id, user_id, role) VALUES (?1, ?2, ?3)",
            params![class_id, user_id, role.as_str()],
        )?;
        Ok(())
    }

    pub fn list_classes_for_teacher(&self, owner_id: &str) -> Result<Vec<ClassRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, lti_context_id, lti_issuer, owner_id, created_at
             FROM classes WHERE owner_id = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![owner_id], row_to_class)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn list_students_in_class(&self, class_id: &str) -> Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT u.id, u.username, u.display_name, u.password_hash, u.role,
                    u.lti_subject, u.lti_issuer, u.created_at
             FROM users u
             JOIN class_members cm ON cm.user_id = u.id
             WHERE cm.class_id = ?1 AND cm.role = 'student'
             ORDER BY u.display_name",
        )?;
        let rows = stmt.query_map(params![class_id], row_to_user)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ─── Conversations + agent assignment ───────────────────────────────────

    /// A conversation is the (user, agent) pairing for a student tutor session.
    /// Teacher dashboards list conversations by class to peek into student chats.
    pub fn create_or_get_conversation(
        &self,
        user_id: &str,
        agent_id: &str,
        class_id: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        if let Some(existing) = conn
            .query_row(
                "SELECT id FROM conversations
                 WHERE user_id = ?1 AND agent_id = ?2
                   AND COALESCE(class_id,'') = COALESCE(?3,'')",
                params![user_id, agent_id, class_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(existing);
        }
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO conversations (id, user_id, agent_id, class_id) VALUES (?1, ?2, ?3, ?4)",
            params![id, user_id, agent_id, class_id],
        )?;
        Ok(id)
    }

    pub fn list_conversations_for_class(&self, class_id: &str) -> Result<Vec<ConversationRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.user_id, c.agent_id, c.class_id, c.created_at, c.last_message_at,
                    u.display_name AS user_display
             FROM conversations c
             JOIN users u ON u.id = c.user_id
             WHERE c.class_id = ?1
             ORDER BY c.last_message_at DESC NULLS LAST",
        )?;
        let rows = stmt.query_map(params![class_id], row_to_conversation)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn touch_conversation(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE conversations SET last_message_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // ─── Agent assignments (the RBAC core) ─────────────────────────────────

    /// Assign a user to an agent with a role.
    /// Roles: "owner" (teacher who created/owns it), "chat_user" (student who
    /// can chat with it), "observer" (teacher peek, read-only).
    pub fn assign_agent(
        &self,
        agent_id: &str,
        user_id: &str,
        role: AgentRole,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO agent_assignments
             (agent_id, user_id, role, assigned_at)
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)",
            params![agent_id, user_id, role.as_str()],
        )?;
        Ok(())
    }

    pub fn unassign_agent(&self, agent_id: &str, user_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM agent_assignments WHERE agent_id = ?1 AND user_id = ?2",
            params![agent_id, user_id],
        )?;
        Ok(())
    }

    /// Returns the assignment role for (agent, user) if one exists.
    pub fn get_assignment(&self, agent_id: &str, user_id: &str) -> Result<Option<AgentRole>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT role FROM agent_assignments WHERE agent_id = ?1 AND user_id = ?2",
            params![agent_id, user_id],
            |row| {
                let s: String = row.get(0)?;
                Ok(AgentRole::parse(&s))
            },
        )
        .optional()
        .context("get_assignment")
    }

    /// True if no assignments exist for this agent. Used for backward
    /// compatibility: legacy agents created before RBAC are unscoped, so we
    /// treat them as admin-only rather than locked out.
    pub fn agent_is_unassigned(&self, agent_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_assignments WHERE agent_id = ?1",
            params![agent_id],
            |row| row.get(0),
        )?;
        Ok(count == 0)
    }

    pub fn list_assignments_for_user(&self, user_id: &str) -> Result<Vec<AgentAssignment>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT agent_id, user_id, role, assigned_at
             FROM agent_assignments WHERE user_id = ?1 ORDER BY assigned_at DESC",
        )?;
        let rows = stmt.query_map(params![user_id], |row| {
            Ok(AgentAssignment {
                agent_id: row.get(0)?,
                user_id: row.get(1)?,
                role: AgentRole::parse(&row.get::<_, String>(2)?),
                assigned_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn list_assignments_for_agent(&self, agent_id: &str) -> Result<Vec<AgentAssignment>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT agent_id, user_id, role, assigned_at
             FROM agent_assignments WHERE agent_id = ?1 ORDER BY assigned_at",
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok(AgentAssignment {
                agent_id: row.get(0)?,
                user_id: row.get(1)?,
                role: AgentRole::parse(&row.get::<_, String>(2)?),
                assigned_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ─── Approval Workflow ────────────────────────────────────────────────

    pub fn list_pending_users(&self) -> Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, display_name, password_hash, role, approval_status,
                    lti_subject, lti_issuer, created_at, system_prompt
             FROM users WHERE approval_status = 'pending' ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_user)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn list_all_users(&self) -> Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, display_name, password_hash, role, approval_status,
                    lti_subject, lti_issuer, created_at, system_prompt
             FROM users ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_user)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn update_user_status(&self, user_id: &str, status: ApprovalStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE users SET approval_status = ?1 WHERE id = ?2",
            params![status.as_str(), user_id],
        )?;
        Ok(())
    }

    pub fn delete_user(&self, user_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM users WHERE id = ?1",
            params![user_id],
        )?;
        Ok(())
    }
}

// ─── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending  => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected  => "rejected",
        }
    }
    pub fn parse(s: &str) -> ApprovalStatus {
        match s {
            "approved" => ApprovalStatus::Approved,
            "rejected" => ApprovalStatus::Rejected,
            _          => ApprovalStatus::Pending,
        }
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserRole {
    Student,
    Teacher,
    Observer,
    Admin,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Student  => "student",
            UserRole::Teacher  => "teacher",
            UserRole::Observer => "observer",
            UserRole::Admin    => "admin",
        }
    }
    pub fn parse(s: &str) -> UserRole {
        match s {
            "teacher"  => UserRole::Teacher,
            "observer" => UserRole::Observer,
            "admin"    => UserRole::Admin,
            _          => UserRole::Student,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// Teacher who owns and configures the agent.
    Owner,
    /// User who can chat with the agent (typically a student).
    ChatUser,
    /// Read-only peek into the conversation (typically a teacher observing
    /// a student's chat with a tutor they configured).
    Observer,
}

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRole::Owner    => "owner",
            AgentRole::ChatUser => "chat_user",
            AgentRole::Observer => "observer",
        }
    }
    pub fn parse(s: &str) -> AgentRole {
        match s {
            "owner"    => AgentRole::Owner,
            "observer" => AgentRole::Observer,
            _          => AgentRole::ChatUser,
        }
    }
    pub fn can_chat(&self) -> bool {
        matches!(self, AgentRole::Owner | AgentRole::ChatUser)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentAssignment {
    pub agent_id: String,
    pub user_id: String,
    pub role: AgentRole,
    pub assigned_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassRole {
    Student,
    Teacher,
    Observer,
}
impl ClassRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClassRole::Student  => "student",
            ClassRole::Teacher  => "teacher",
            ClassRole::Observer => "observer",
        }
    }
}

pub struct NewUser {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub password_hash: Option<String>,   // None for LTI-only users
    pub role: UserRole,
    pub approval_status: ApprovalStatus,
    pub lti_subject: Option<String>,
    pub lti_issuer: Option<String>,
}

pub struct NewClass {
    pub id: String,
    pub name: String,
    pub lti_context_id: Option<String>,
    pub lti_issuer: Option<String>,
    pub owner_id: String,                 // teacher user_id
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub password_hash: Option<String>,
    pub role: UserRole,
    pub approval_status: ApprovalStatus,
    pub lti_subject: Option<String>,
    pub lti_issuer: Option<String>,
    pub created_at: String,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClassRow {
    pub id: String,
    pub name: String,
    pub lti_context_id: Option<String>,
    pub lti_issuer: Option<String>,
    pub owner_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ConversationRow {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub class_id: Option<String>,
    pub created_at: String,
    pub last_message_at: Option<String>,
    pub user_display: Option<String>,
}

// ─── Row mappers ───────────────────────────────────────────────────────────

fn row_to_user(row: &rusqlite::Row) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        username: row.get(1)?,
        display_name: row.get(2)?,
        password_hash: row.get(3)?,
        role: UserRole::parse(&row.get::<_, String>(4)?),
        approval_status: ApprovalStatus::parse(&row.get::<_, String>(5)?),
        lti_subject: row.get(6)?,
        lti_issuer: row.get(7)?,
        created_at: row.get(8)?,
        system_prompt: row.get(9).ok(),
    })
}

fn row_to_class(row: &rusqlite::Row) -> rusqlite::Result<ClassRow> {
    Ok(ClassRow {
        id: row.get(0)?,
        name: row.get(1)?,
        lti_context_id: row.get(2)?,
        lti_issuer: row.get(3)?,
        owner_id: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn row_to_conversation(row: &rusqlite::Row) -> rusqlite::Result<ConversationRow> {
    Ok(ConversationRow {
        id: row.get(0)?,
        user_id: row.get(1)?,
        agent_id: row.get(2)?,
        class_id: row.get(3)?,
        created_at: row.get(4)?,
        last_message_at: row.get(5)?,
        user_display: row.get(6)?,
    })
}

// ─── Schema ────────────────────────────────────────────────────────────────

const SCHEMA_SYSTEM_PROMPTS: &str = r#"
CREATE TABLE IF NOT EXISTS system_prompts (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    content     TEXT NOT NULL,
    is_active   INTEGER NOT NULL DEFAULT 0,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);
"#;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY,
    username        TEXT UNIQUE NOT NULL,
    display_name    TEXT,
    password_hash   TEXT,                       -- NULL for LTI-only users
    role            TEXT NOT NULL DEFAULT 'student'
                        CHECK(role IN ('student','teacher','observer','admin')),
    lti_subject     TEXT,
    lti_issuer      TEXT,
    created_at      DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (lti_issuer, lti_subject)
);
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);

CREATE TABLE IF NOT EXISTS classes (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    lti_context_id  TEXT,                       -- Brightspace course context
    lti_issuer      TEXT,
    owner_id        TEXT NOT NULL,              -- the teacher
    created_at      DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(owner_id) REFERENCES users(id),
    UNIQUE (lti_issuer, lti_context_id)
);
CREATE INDEX IF NOT EXISTS idx_classes_owner ON classes(owner_id);

CREATE TABLE IF NOT EXISTS class_members (
    class_id    TEXT NOT NULL,
    user_id     TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT 'student'
                    CHECK(role IN ('student','teacher','observer')),
    joined_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (class_id, user_id),
    FOREIGN KEY(class_id) REFERENCES classes(id),
    FOREIGN KEY(user_id)  REFERENCES users(id)
);
CREATE INDEX IF NOT EXISTS idx_class_members_user ON class_members(user_id);

-- Per-(user, agent[, class]) thread. The actual transcript stays in JSONL;
-- this row exists for fast listing + the teacher's class-roster view.
CREATE TABLE IF NOT EXISTS conversations (
    id                  TEXT PRIMARY KEY,
    user_id             TEXT NOT NULL,
    agent_id            TEXT NOT NULL,
    class_id            TEXT,                   -- NULL for teacher's own agent
    title               TEXT DEFAULT 'New Chat',
    system_prompt       TEXT,                   -- per-conversation system prompt override
    created_at          DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_message_at     DATETIME,
    FOREIGN KEY(user_id)  REFERENCES users(id),
    FOREIGN KEY(class_id) REFERENCES classes(id),
    UNIQUE (user_id, agent_id, class_id)
);
CREATE INDEX IF NOT EXISTS idx_conversations_class ON conversations(class_id, last_message_at);
CREATE INDEX IF NOT EXISTS idx_conversations_user ON conversations(user_id);

-- Lightweight "teacher saw this" tracking. Full read receipts later if needed.
CREATE TABLE IF NOT EXISTS observations (
    observer_id     TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    last_seen_at    DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (observer_id, conversation_id),
    FOREIGN KEY(observer_id) REFERENCES users(id),
    FOREIGN KEY(conversation_id) REFERENCES conversations(id)
);

-- RBAC core: who can do what with which agent.
-- An agent with NO rows here is "unassigned" and falls back to admin-only access.
CREATE TABLE IF NOT EXISTS agent_assignments (
    agent_id     TEXT NOT NULL,
    user_id      TEXT NOT NULL,
    role         TEXT NOT NULL DEFAULT 'chat_user'
                     CHECK(role IN ('owner','chat_user','observer')),
    assigned_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (agent_id, user_id),
    FOREIGN KEY(user_id) REFERENCES users(id)
);
-- Messages table (added for simple chat proxy)
CREATE TABLE IF NOT EXISTS messages (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role            TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(conversation_id) REFERENCES conversations(id)
);
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id, created_at);

"#;

// ─── Simple Chat Types (Phase One refactor) ─────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatConversation {
    pub id: String,
    pub title: String,
    pub system_prompt: Option<String>,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemPrompt {
    pub id: String,
    pub name: String,
    pub content: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

// ─── Simple Chat API (Phase One refactor) ───────────────────────────────

impl ChatDb {
    pub fn list_conversations(&self, user_id: &str) -> Result<Vec<ChatConversation>> {
    let conn = self.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, COALESCE(title, 'New Chat'), system_prompt, COALESCE(color, '#2a7f7f'), created_at, last_message_at,
                (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
         FROM conversations c
         WHERE user_id = ?1
         ORDER BY COALESCE(last_message_at, created_at) DESC",
    )?;
    let rows = stmt
        .query_map(params![user_id], |row| {
            Ok(ChatConversation {
                id: row.get(0)?,
                title: row.get(1)?,
                system_prompt: row.get(2).ok(),
                color: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5).unwrap_or_default(),
                message_count: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn create_conversation(&self, user_id: &str, title: &str) -> Result<ChatConversation> {
    let colors = ["#2a7f7f", "#b85c38", "#d4a843", "#5a7a5a", "#8a6a8a"];
    let color = colors[uuid::Uuid::new_v4().as_u128() as usize % colors.len()];
    let conn = self.conn.lock().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO conversations (id, user_id, agent_id, title, color, created_at, last_message_at)
         VALUES (?1, ?2, 'chat', ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        params![id, user_id, title, color],
    )?;
    Ok(ChatConversation {
        id,
        title: title.to_string(),
        system_prompt: None,
        color: color.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        message_count: 0,
    })
}

pub fn get_conversation(&self, id: &str, user_id: &str) -> Result<ChatConversation> {
    let conn = self.conn.lock().unwrap();
    let row = conn.query_row(
        "SELECT id, COALESCE(title, 'New Chat'), system_prompt, COALESCE(color, '#2a7f7f'), created_at, last_message_at,
                (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
         FROM conversations c
         WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
        |row| {
            Ok(ChatConversation {
                id: row.get(0)?,
                title: row.get(1)?,
                system_prompt: row.get(2).ok(),
                color: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5).unwrap_or_default(),
                message_count: row.get(6)?,
            })
        },
    )?;
    Ok(row)
}

pub fn delete_conversation(&self, id: &str, user_id: &str) -> Result<()> {
    let conn = self.conn.lock().unwrap();
    conn.execute(
        "DELETE FROM messages WHERE conversation_id = ?1",
        params![id],
    )?;
    conn.execute(
        "DELETE FROM conversations WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )?;
    Ok(())
}

pub fn clear_messages(&self, conversation_id: &str, user_id: &str) -> Result<()> {
    let conn = self.conn.lock().unwrap();
    conn.execute(
        "DELETE FROM messages WHERE conversation_id = ?1 AND conversation_id IN (
            SELECT id FROM conversations WHERE user_id = ?2
        )",
        params![conversation_id, user_id],
    )?;
    Ok(())
}

pub fn update_conversation_title(&self, id: &str, user_id: &str, title: &str) -> Result<()> {
    let conn = self.conn.lock().unwrap();
    conn.execute(
        "UPDATE conversations SET title = ?1 WHERE id = ?2 AND user_id = ?3",
        params![title, id, user_id],
    )?;
    Ok(())
}

pub fn update_conversation_color(&self, id: &str, user_id: &str, color: &str) -> Result<()> {
    let conn = self.conn.lock().unwrap();
    conn.execute(
        "UPDATE conversations SET color = ?1 WHERE id = ?2 AND user_id = ?3",
        params![color, id, user_id],
    )?;
    Ok(())
}

pub fn get_messages(
    &self,
    conversation_id: &str,
    user_id: &str,
) -> Result<Vec<ChatMessage>> {
    let conn = self.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT m.id, m.role, m.content, m.created_at
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE m.conversation_id = ?1 AND c.user_id = ?2
         ORDER BY m.created_at ASC",
    )?;
    let rows = stmt
        .query_map(params![conversation_id, user_id], |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_messages_paginated(
    &self,
    conversation_id: &str,
    user_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatMessage>> {
    let conn = self.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT m.id, m.role, m.content, m.created_at
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         WHERE m.conversation_id = ?1 AND c.user_id = ?2
         ORDER BY m.created_at ASC
         LIMIT ?3 OFFSET ?4",
    )?;
    let rows = stmt
        .query_map(params![conversation_id, user_id, limit, offset], |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn add_message(
    &self,
    conversation_id: &str,
    user_id: &str,
    role: &str,
    content: &str,
) -> Result<ChatMessage> {
    let conn = self.conn.lock().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)",
        params![id, conversation_id, role, content],
    )?;
    conn.execute(
        "UPDATE conversations SET last_message_at = CURRENT_TIMESTAMP WHERE id = ?1 AND user_id = ?2",
        params![conversation_id, user_id],
    )?;
    Ok(ChatMessage {
        id,
        role: role.to_string(),
        content: content.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
    }

    // ─── System Prompts ───────────────────────────────────────────────────────

    pub fn get_active_system_prompt(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT content FROM system_prompts WHERE is_active = 1 LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("get_active_system_prompt")
    }

    pub fn list_system_prompts(&self) -> Result<Vec<SystemPrompt>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, content, is_active, created_at, updated_at
             FROM system_prompts
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SystemPrompt {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    content: row.get(2)?,
                    is_active: row.get::<_, i64>(3)? != 0,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn create_system_prompt(&self, name: &str, content: &str) -> Result<SystemPrompt> {
        let conn = self.conn.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO system_prompts (id, name, content, is_active, created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            params![id, name, content],
        )?;
        Ok(SystemPrompt {
            id,
            name: name.to_string(),
            content: content.to_string(),
            is_active: false,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub fn update_system_prompt(&self, id: &str, name: Option<&str>, content: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if let Some(name) = name {
            conn.execute(
                "UPDATE system_prompts SET name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![name, id],
            )?;
        }
        if let Some(content) = content {
            conn.execute(
                "UPDATE system_prompts SET content = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![content, id],
            )?;
        }
        Ok(())
    }

    pub fn set_active_system_prompt(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE system_prompts SET is_active = 0",
            [],
        )?;
        conn.execute(
            "UPDATE system_prompts SET is_active = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn delete_system_prompt(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM system_prompts WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // ─── Admin: View Any User's Conversations ───────────────────────────────

    pub fn admin_list_user_conversations(&self, target_user_id: &str) -> Result<Vec<ChatConversation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, COALESCE(title, 'New Chat'), system_prompt, COALESCE(color, '#2a7f7f'), created_at, last_message_at,
                    (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
             FROM conversations c
             WHERE user_id = ?1
             ORDER BY COALESCE(last_message_at, created_at) DESC",
        )?;
        let rows = stmt
            .query_map(params![target_user_id], |row| {
                Ok(ChatConversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    system_prompt: row.get(2).ok(),
                    color: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5).unwrap_or_default(),
                    message_count: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn admin_get_conversation_messages(&self, conversation_id: &str) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, role, content, created_at
             FROM messages
             WHERE conversation_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map(params![conversation_id], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ─── Per-User System Prompt ─────────────────────────────────────────────

    pub fn set_user_system_prompt(&self, user_id: &str, prompt: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE users SET system_prompt = ?1 WHERE id = ?2",
            params![prompt, user_id],
        )?;
        Ok(())
    }

    pub fn get_user_system_prompt(&self, user_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT system_prompt FROM users WHERE id = ?1",
            params![user_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .context("get_user_system_prompt")
        .map(|opt| opt.flatten())
    }

    // ─── Per-Conversation System Prompt ───────────────────────────────────

    pub fn get_conversation_system_prompt(&self, conversation_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT system_prompt FROM conversations WHERE id = ?1",
            params![conversation_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .context("get_conversation_system_prompt")
        .map(|opt| opt.flatten())
    }

    pub fn set_conversation_system_prompt(&self, conversation_id: &str, prompt: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE conversations SET system_prompt = ?1 WHERE id = ?2",
            params![prompt, conversation_id],
        )?;
        Ok(())
    }
}
