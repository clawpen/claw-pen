/*
 * Shared Memory Module for Claw Pen
 * ==================================
 *
 * This module provides a shared memory system using SQLite with sqlite-vss extension
 * for vector similarity search. It enables agents to store, retrieve, and share memories
 * with semantic search capabilities.
 *
 * ## Setup Instructions for sqlite-vss
 * ------------------------------------
 *
 * ### Option 1: Load as SQLite Extension (Recommended)
 *
 * 1. Install sqlite-vss:
 *    ```bash
 *    # Download the latest release from https://github.com/asg017/sqlite-vss/releases
 *    # Or build from source:
 *    git clone https://github.com/asg017/sqlite-vss
 *    cd sqlite-vss
 *    make loadable
 *    ```
 *
 * 2. The extension file (vss0.so on Linux, vss0.dylib on macOS, vss0.dll on Windows)
 *    should be placed in a known location, e.g., /usr/local/lib/sqlite/vss0.so
 *
 * 3. Set environment variable before running:
 *    ```bash
 *    export SQLITE_VSS_PATH=/usr/local/lib/sqlite/vss0.so
 *    ```
 *
 * 4. The SharedMemory struct will automatically load the extension on initialization.
 *
 * ### Option 2: Use sqlite-vec (Newer Alternative)
 *
 * sqlite-vec is a newer, simpler vector search extension:
 *    ```bash
 *    # See https://github.com/asg017/sqlite-vec
 *    ```
 *
 * ### Dependencies Required
 *
 * The Cargo.toml should include:
 *    rusqlite = { version = "0.31", features = ["bundled"] }
 *
 * ### Embedding Format
 *
 * Embeddings should be provided as Vec<f32>. The default expected dimension is 1536
 * (OpenAI text-embedding-ada-002), but this can be configured.
 *
 * ### Database Schema
 *
 * The module creates the following tables:
 *
 * ```sql
 * -- Agent memories with vector embeddings
 * CREATE TABLE memories (
 *     id INTEGER PRIMARY KEY,
 *     org TEXT NOT NULL DEFAULT 'default',
 *     agent_id TEXT NOT NULL,
 *     content TEXT NOT NULL,
 *     embedding BLOB,
 *     metadata TEXT,  -- JSON
 *     created_at TEXT NOT NULL,
 *     updated_at TEXT NOT NULL
 * );
 *
 * -- Virtual table for vector similarity search
 * CREATE VIRTUAL TABLE vss_memories USING vss0(
 *     embedding(1536)
 * );
 *
 * -- Task queue for inter-agent communication
 * CREATE TABLE tasks (
 *     id INTEGER PRIMARY KEY,
 *     from_agent TEXT NOT NULL,
 *     to_agent TEXT,
 *     task_type TEXT NOT NULL,
 *     payload TEXT,  -- JSON
 *     priority INTEGER DEFAULT 0,
 *     status TEXT DEFAULT 'pending',
 *     created_at TEXT NOT NULL,
 *     claimed_at TEXT,
 *     completed_at TEXT
 * );
 *
 * -- Agent status tracking
 * CREATE TABLE agent_statuses (
 *     agent_id TEXT PRIMARY KEY,
 *     status TEXT NOT NULL,
 *     last_heartbeat TEXT NOT NULL,
 *     metadata TEXT  -- JSON
 * );
 *
 * -- Org namespace constants:
 * --   ORG_COMMON = "common"  -- shared knowledge across orgs
 * --   ORG_ALL = "all"        -- query everything (no org filter)
 * --   ORG_DEFAULT = "default" -- default org when none specified
 * ```
 */

// Allow dead_code for public API items that may not be used internally
#![allow(dead_code)]

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use thiserror::Error;

// Default embedding dimension (OpenAI ada-002)
const DEFAULT_EMBEDDING_DIM: usize = 1536;

/// Special org namespace constants
pub const ORG_COMMON: &str = "common"; // Shared knowledge across orgs
pub const ORG_ALL: &str = "all"; // Query everything (no org filter)
pub const ORG_DEFAULT: &str = "default"; // Default org when none specified

/// Errors specific to shared memory operations
#[derive(Debug, Error)]
pub enum SharedMemoryError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Failed to load sqlite-vss extension: {0}")]
    ExtensionLoad(String),

    #[error("Invalid embedding dimension: expected {expected}, got {actual}")]
    InvalidEmbeddingDimension { expected: usize, actual: usize },

    #[error("Memory not found: {0}")]
    MemoryNotFound(i64),

    #[error("Task not found: {0}")]
    TaskNotFound(i64),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Configuration for SharedMemory
#[derive(Debug, Clone)]
pub struct SharedMemoryConfig {
    /// Path to the SQLite database file
    pub database_path: PathBuf,
    /// Path to the sqlite-vss extension (optional, will try env var if not set)
    pub vss_extension_path: Option<PathBuf>,
    /// Embedding dimension (default: 1536 for OpenAI ada-002)
    pub embedding_dim: usize,
}

impl Default for SharedMemoryConfig {
    fn default() -> Self {
        Self {
            database_path: PathBuf::from("/data/claw-pen/shared/memory.db"),
            vss_extension_path: std::env::var("SQLITE_VSS_PATH").ok().map(PathBuf::from),
            embedding_dim: DEFAULT_EMBEDDING_DIM,
        }
    }
}

/// A stored memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: i64,
    pub org: String,
    pub agent_id: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A memory entry without the ID (for insertion)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMemory {
    pub org: Option<String>,
    pub agent_id: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: Option<serde_json::Value>,
}

impl NewMemory {
    /// Get the org, defaulting to ORG_DEFAULT if not set
    pub fn org_or_default(&self) -> &str {
        self.org.as_deref().unwrap_or(ORG_DEFAULT)
    }
}

/// Search result with similarity score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub memory: Memory,
    pub similarity: f32,
}

/// Task status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Claimed,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Claimed => write!(f, "claimed"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(TaskStatus::Pending),
            "claimed" => Ok(TaskStatus::Claimed),
            "in_progress" => Ok(TaskStatus::InProgress),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            "cancelled" => Ok(TaskStatus::Cancelled),
            _ => Err(format!("Unknown task status: {}", s)),
        }
    }
}

/// A task in the shared queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub from_agent: String,
    pub to_agent: Option<String>,
    pub task_type: String,
    pub payload: Option<serde_json::Value>,
    pub priority: i32,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// A new task to be pushed to the queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTask {
    pub from_agent: String,
    pub to_agent: Option<String>,
    pub task_type: String,
    pub payload: Option<serde_json::Value>,
    #[serde(default)]
    pub priority: i32,
}

/// Agent status entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusEntry {
    pub agent_id: String,
    pub status: String,
    pub last_heartbeat: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// The main SharedMemory struct that manages the SQLite connection
#[derive(Debug)]
pub struct SharedMemory {
    conn: Arc<Mutex<Connection>>,
    config: SharedMemoryConfig,
    vss_enabled: bool,
}

impl SharedMemory {
    /// Create a new SharedMemory instance with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(SharedMemoryConfig::default())
    }

    /// Create a new SharedMemory instance with custom configuration
    pub fn with_config(config: SharedMemoryConfig) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = config.database_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create database directory: {:?}", parent))?;
        }

        // Open database connection
        let conn = Connection::open(&config.database_path)
            .with_context(|| format!("Failed to open database at {:?}", config.database_path))?;

        // Try to load VSS extension
        let vss_enabled = Self::load_vss_extension(&conn, &config.vss_extension_path)
            .map_err(|e| {
                tracing::warn!(
                    "sqlite-vss extension not loaded (vector search disabled): {}",
                    e
                );
                e
            })
            .is_ok();

        let shared_memory = Self {
            conn: Arc::new(Mutex::new(conn)),
            config,
            vss_enabled,
        };

        // Initialize schema
        shared_memory.initialize_schema()?;

        if shared_memory.vss_enabled {
            tracing::info!("SharedMemory initialized with vector search enabled");
        } else {
            tracing::warn!(
                "SharedMemory initialized WITHOUT vector search (sqlite-vss not available)"
            );
        }

        Ok(shared_memory)
    }

    /// Attempt to load the sqlite-vss extension
    fn load_vss_extension(_conn: &Connection, extension_path: &Option<PathBuf>) -> Result<()> {
        let path = extension_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No VSS extension path configured"))?;

        // Note: rusqlite's load_extension requires unsafe and the bundled feature
        // may not support extensions. In production, you may need to:
        // 1. Use a system SQLite with extension support
        // 2. Or use the sqlite-vss static binding if available

        // This is a placeholder - actual implementation depends on how sqlite-vss
        // is deployed. Some options:
        // - unsafe { conn.load_extension(path, Some("sqlite3_vss_init"))?; }
        // - Use a custom build with sqlite-vss linked statically

        tracing::info!("Attempting to load sqlite-vss from {:?}", path);

        // For now, we'll work without VSS and do approximate search
        // Real implementation would load the extension here
        Err(anyhow::anyhow!(
            "sqlite-vss extension loading not yet implemented - using fallback search"
        ))
    }

    /// Initialize the database schema
    fn initialize_schema(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Create memories table
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                org TEXT NOT NULL DEFAULT 'default',
                agent_id TEXT NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                metadata TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_memories_org ON memories(org);
            CREATE INDEX IF NOT EXISTS idx_memories_agent_id ON memories(agent_id);
            CREATE INDEX IF NOT EXISTS idx_memories_org_agent ON memories(org, agent_id);
            CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at);

            -- Task queue
            CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                from_agent TEXT NOT NULL,
                to_agent TEXT,
                task_type TEXT NOT NULL,
                payload TEXT,
                priority INTEGER DEFAULT 0,
                status TEXT DEFAULT 'pending',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                claimed_at TEXT,
                completed_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
            CREATE INDEX IF NOT EXISTS idx_tasks_to_agent ON tasks(to_agent);
            CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks(priority DESC);

            -- Agent statuses
            CREATE TABLE IF NOT EXISTS agent_statuses (
                agent_id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                last_heartbeat TEXT NOT NULL DEFAULT (datetime('now')),
                metadata TEXT
            );
            "#,
        )?;

        // Create VSS virtual table if extension is available
        if self.vss_enabled {
            conn.execute_batch(&format!(
                r#"
                CREATE VIRTUAL TABLE IF NOT EXISTS vss_memories USING vss0(
                    embedding({})
                );
                "#,
                self.config.embedding_dim
            ))?;
        }

        Ok(())
    }

    // ========================================================================
    // Memory Operations
    // ========================================================================

    /// Store a new memory with optional embedding
    ///
    /// # Arguments
    /// * `org` - Organization namespace (use ORG_DEFAULT if None)
    /// * `memory` - The memory to store
    pub fn store_memory(&self, org: Option<&str>, memory: &NewMemory) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let org = org.unwrap_or(memory.org_or_default());
        let now = Utc::now().to_rfc3339();
        let embedding_blob = memory
            .embedding
            .as_ref()
            .map(|e| Self::embedding_to_blob(e));
        let metadata_json = memory
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        conn.execute(
            r#"
            INSERT INTO memories (org, agent_id, content, embedding, metadata, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            "#,
            params![
                org,
                memory.agent_id,
                memory.content,
                embedding_blob,
                metadata_json,
                now,
            ],
        )?;

        let id = conn.last_insert_rowid();

        // If VSS is enabled, also insert into the virtual table
        if self.vss_enabled {
            if let Some(ref embedding) = memory.embedding {
                let _ = conn.execute(
                    "INSERT INTO vss_memories (rowid, embedding) VALUES (?1, ?2)",
                    params![id, Self::embedding_to_blob(embedding)],
                );
            }
        }

        tracing::debug!(
            "Stored memory {} for agent {} in org {}",
            id,
            memory.agent_id,
            org
        );
        Ok(id)
    }

    /// Search memories by vector similarity
    ///
    /// # Arguments
    /// * `org` - Organization namespace. Use ORG_ALL to search across all orgs,
    ///   ORG_COMMON for shared knowledge, or a specific org name.
    /// * `query_embedding` - The query vector
    /// * `limit` - Maximum number of results
    pub fn search_memories(
        &self,
        org: &str,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>> {
        if self.vss_enabled {
            self.search_memories_vss(org, query_embedding, limit)
        } else {
            self.search_memories_fallback(org, query_embedding, limit)
        }
    }

    /// Search using sqlite-vss (when available)
    fn search_memories_vss(
        &self,
        org: &str,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let query_blob = Self::embedding_to_blob(query_embedding);

        let (query, params): (String, Vec<Box<dyn rusqlite::ToSql>>) = if org == ORG_ALL {
            (
                r#"
                SELECT m.id, m.org, m.agent_id, m.content, m.embedding, m.metadata, m.created_at, m.updated_at, v.distance
                FROM memories m
                JOIN vss_memories v ON m.rowid = v.rowid
                WHERE vss_search(v.embedding, ?1)
                ORDER BY v.distance ASC
                LIMIT ?2
                "#.to_string(),
                vec![Box::new(query_blob), Box::new(limit as i32)]
            )
        } else {
            (
                r#"
                SELECT m.id, m.org, m.agent_id, m.content, m.embedding, m.metadata, m.created_at, m.updated_at, v.distance
                FROM memories m
                JOIN vss_memories v ON m.rowid = v.rowid
                WHERE vss_search(v.embedding, ?1) AND m.org = ?2
                ORDER BY v.distance ASC
                LIMIT ?3
                "#.to_string(),
                vec![Box::new(query_blob), Box::new(org.to_string()), Box::new(limit as i32)]
            )
        };

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&query)?;

        let results = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(MemorySearchResult {
                    memory: Memory {
                        id: row.get(0)?,
                        org: row.get(1)?,
                        agent_id: row.get(2)?,
                        content: row.get(3)?,
                        embedding: row
                            .get::<_, Option<Vec<u8>>>(4)?
                            .map(|b| Self::blob_to_embedding(&b)),
                        metadata: row
                            .get::<_, Option<String>>(5)?
                            .map(|s| serde_json::from_str(&s))
                            .transpose()
                            .unwrap_or(None),
                        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                        updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    },
                    similarity: 1.0 - row.get::<_, f32>(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Fallback search using cosine similarity in memory (when VSS not available)
    fn search_memories_fallback(
        &self,
        org: &str,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Get memories with embeddings, filtered by org if not ORG_ALL
        let query = if org == ORG_ALL {
            "SELECT id, org, agent_id, content, embedding, metadata, created_at, updated_at FROM memories WHERE embedding IS NOT NULL".to_string()
        } else {
            "SELECT id, org, agent_id, content, embedding, metadata, created_at, updated_at FROM memories WHERE embedding IS NOT NULL AND org = ?1".to_string()
        };

        let mut stmt = conn.prepare(&query)?;

        let memories = if org == ORG_ALL {
            stmt.query_map([], |row| {
                let embedding_blob: Vec<u8> = row.get(4)?;
                let embedding = Self::blob_to_embedding(&embedding_blob);
                Ok((
                    Memory {
                        id: row.get(0)?,
                        org: row.get(1)?,
                        agent_id: row.get(2)?,
                        content: row.get(3)?,
                        embedding: Some(embedding.clone()),
                        metadata: row
                            .get::<_, Option<String>>(5)?
                            .map(|s| serde_json::from_str(&s))
                            .transpose()
                            .unwrap_or(None),
                        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                        updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    },
                    embedding,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(params![org], |row| {
                let embedding_blob: Vec<u8> = row.get(4)?;
                let embedding = Self::blob_to_embedding(&embedding_blob);
                Ok((
                    Memory {
                        id: row.get(0)?,
                        org: row.get(1)?,
                        agent_id: row.get(2)?,
                        content: row.get(3)?,
                        embedding: Some(embedding.clone()),
                        metadata: row
                            .get::<_, Option<String>>(5)?
                            .map(|s| serde_json::from_str(&s))
                            .transpose()
                            .unwrap_or(None),
                        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                        updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    },
                    embedding,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };

        // Calculate similarities
        let mut results: Vec<MemorySearchResult> = memories
            .into_iter()
            .map(|(memory, embedding)| {
                let similarity = Self::cosine_similarity(query_embedding, &embedding);
                MemorySearchResult { memory, similarity }
            })
            .collect();

        // Sort by similarity (descending) and take top N
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }

    /// List all memories (optionally filtered by org and agent_id)
    ///
    /// # Arguments
    /// * `org` - Optional org filter. Use ORG_ALL or None to list across all orgs.
    /// * `agent_id` - Optional agent filter
    pub fn list_all(&self, org: Option<&str>, agent_id: Option<&str>) -> Result<Vec<Memory>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let (query, params): (String, Vec<Box<dyn rusqlite::ToSql>>) = match (org, agent_id) {
            (Some(o), Some(a)) if o != ORG_ALL => (
                "SELECT id, org, agent_id, content, embedding, metadata, created_at, updated_at
                 FROM memories WHERE org = ?1 AND agent_id = ?2 ORDER BY created_at DESC"
                    .to_string(),
                vec![Box::new(o.to_string()), Box::new(a.to_string())],
            ),
            (Some(o), None) if o != ORG_ALL => (
                "SELECT id, org, agent_id, content, embedding, metadata, created_at, updated_at
                 FROM memories WHERE org = ?1 ORDER BY created_at DESC"
                    .to_string(),
                vec![Box::new(o.to_string())],
            ),
            (Some(_), Some(a)) => {
                // This is ORG_ALL with agent_id filter
                (
                    "SELECT id, org, agent_id, content, embedding, metadata, created_at, updated_at
                     FROM memories WHERE agent_id = ?1 ORDER BY created_at DESC"
                        .to_string(),
                    vec![Box::new(a.to_string())],
                )
            }
            (Some(_), None) => {
                // This is ORG_ALL without agent_id filter
                (
                    "SELECT id, org, agent_id, content, embedding, metadata, created_at, updated_at
                     FROM memories ORDER BY created_at DESC"
                        .to_string(),
                    vec![],
                )
            }
            (None, Some(a)) => (
                "SELECT id, org, agent_id, content, embedding, metadata, created_at, updated_at
                 FROM memories WHERE agent_id = ?1 ORDER BY created_at DESC"
                    .to_string(),
                vec![Box::new(a.to_string())],
            ),
            (None, None) => (
                "SELECT id, org, agent_id, content, embedding, metadata, created_at, updated_at
                 FROM memories ORDER BY created_at DESC"
                    .to_string(),
                vec![],
            ),
        };

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&query)?;
        let memories = stmt
            .query_map(params_refs.as_slice(), Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(memories)
    }

    /// Get a specific memory by ID
    pub fn get_memory(&self, id: i64) -> Result<Option<Memory>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            "SELECT id, org, agent_id, content, embedding, metadata, created_at, updated_at 
             FROM memories WHERE id = ?1",
        )?;

        stmt.query_row(params![id], Self::row_to_memory)
            .optional()
            .map_err(SharedMemoryError::from)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Delete a memory by ID
    pub fn delete(&self, id: i64) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let rows_affected = conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;

        // Also delete from VSS table if enabled
        if self.vss_enabled && rows_affected > 0 {
            let _ = conn.execute("DELETE FROM vss_memories WHERE rowid = ?1", params![id]);
        }

        Ok(rows_affected > 0)
    }

    /// Delete all memories for an agent within an org
    ///
    /// # Arguments
    /// * `org` - Organization namespace
    /// * `agent_id` - The agent ID to delete memories for
    pub fn delete_agent_memories(&self, org: &str, agent_id: &str) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Get IDs first for VSS cleanup
        let ids: Vec<i64> = if self.vss_enabled {
            conn.prepare("SELECT id FROM memories WHERE org = ?1 AND agent_id = ?2")?
                .query_map(params![org, agent_id], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![]
        };

        let rows_affected = conn.execute(
            "DELETE FROM memories WHERE org = ?1 AND agent_id = ?2",
            params![org, agent_id],
        )?;

        // Clean up VSS table
        if self.vss_enabled {
            for id in ids {
                let _ = conn.execute("DELETE FROM vss_memories WHERE rowid = ?1", params![id]);
            }
        }

        Ok(rows_affected)
    }

    // ========================================================================
    // Task Queue Operations
    // ========================================================================

    /// Push a new task to the queue
    pub fn push_task(&self, task: &NewTask) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let payload_json = task
            .payload
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        conn.execute(
            r#"
            INSERT INTO tasks (from_agent, to_agent, task_type, payload, priority, status, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, 'pending', datetime('now'))
            "#,
            params![
                task.from_agent,
                task.to_agent,
                task.task_type,
                payload_json,
                task.priority,
            ],
        )?;

        let id = conn.last_insert_rowid();
        tracing::debug!("Pushed task {} from agent {}", id, task.from_agent);
        Ok(id)
    }

    /// Pop the next available task (optionally for a specific agent)
    pub fn pop_task(&self, for_agent: Option<&str>) -> Result<Option<Task>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find highest priority pending task
        let mut stmt = if let Some(_agent) = for_agent {
            conn.prepare(
                r#"
                SELECT id, from_agent, to_agent, task_type, payload, priority, status, created_at, claimed_at, completed_at
                FROM tasks 
                WHERE status = 'pending' AND (to_agent IS NULL OR to_agent = ?1)
                ORDER BY priority DESC, created_at ASC
                LIMIT 1
                "#
            )?
        } else {
            conn.prepare(
                r#"
                SELECT id, from_agent, to_agent, task_type, payload, priority, status, created_at, claimed_at, completed_at
                FROM tasks 
                WHERE status = 'pending'
                ORDER BY priority DESC, created_at ASC
                LIMIT 1
                "#
            )?
        };

        let task = if for_agent.is_some() {
            stmt.query_row(params![for_agent], Self::row_to_task)
                .optional()?
        } else {
            stmt.query_row([], Self::row_to_task).optional()?
        };

        if let Some(task) = task {
            // Mark as claimed
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE tasks SET status = 'claimed', claimed_at = ?1 WHERE id = ?2",
                params![now, task.id],
            )?;

            let mut claimed_task = task;
            claimed_task.status = TaskStatus::Claimed;
            claimed_task.claimed_at = Some(Utc::now());

            Ok(Some(claimed_task))
        } else {
            Ok(None)
        }
    }

    /// List tasks (optionally filtered by status and/or agent)
    pub fn list_tasks(&self, status: Option<TaskStatus>, agent: Option<&str>) -> Result<Vec<Task>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut query = "SELECT id, from_agent, to_agent, task_type, payload, priority, status, created_at, claimed_at, completed_at FROM tasks WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(s) = &status {
            query.push_str(&format!(" AND status = ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(s.to_string()));
        }

        if let Some(a) = agent {
            query.push_str(&format!(
                " AND (to_agent IS NULL OR to_agent = ?{})",
                params_vec.len() + 1
            ));
            params_vec.push(Box::new(a.to_string()));
        }

        query.push_str(" ORDER BY priority DESC, created_at ASC");

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&query)?;
        let tasks = stmt
            .query_map(params_refs.as_slice(), Self::row_to_task)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(tasks)
    }

    /// Update task status
    pub fn update_task_status(&self, task_id: i64, status: TaskStatus) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let now = Utc::now().to_rfc3339();

        match status {
            TaskStatus::InProgress => {
                conn.execute(
                    "UPDATE tasks SET status = ?1 WHERE id = ?2",
                    params![status.to_string(), task_id],
                )?;
            }
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
                conn.execute(
                    "UPDATE tasks SET status = ?1, completed_at = ?2 WHERE id = ?3",
                    params![status.to_string(), now, task_id],
                )?;
            }
            _ => {
                conn.execute(
                    "UPDATE tasks SET status = ?1 WHERE id = ?2",
                    params![status.to_string(), task_id],
                )?;
            }
        }

        Ok(())
    }

    // ========================================================================
    // Agent Status Operations
    // ========================================================================

    /// Update agent status
    pub fn update_status(
        &self,
        agent_id: &str,
        status: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let metadata_json = metadata.map(|m| serde_json::to_string(&m)).transpose()?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            r#"
            INSERT INTO agent_statuses (agent_id, status, last_heartbeat, metadata)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(agent_id) DO UPDATE SET
                status = excluded.status,
                last_heartbeat = excluded.last_heartbeat,
                metadata = excluded.metadata
            "#,
            params![agent_id, status, now, metadata_json],
        )?;

        tracing::debug!("Updated status for agent {}: {}", agent_id, status);
        Ok(())
    }

    /// Get all agent statuses
    pub fn get_all_statuses(&self) -> Result<Vec<AgentStatusEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            "SELECT agent_id, status, last_heartbeat, metadata FROM agent_statuses ORDER BY last_heartbeat DESC"
        )?;

        let statuses = stmt
            .query_map([], |row| {
                Ok(AgentStatusEntry {
                    agent_id: row.get(0)?,
                    status: row.get(1)?,
                    last_heartbeat: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    metadata: row
                        .get::<_, Option<String>>(3)?
                        .map(|s| serde_json::from_str(&s))
                        .transpose()
                        .unwrap_or(None),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(statuses)
    }

    /// Get status for a specific agent
    pub fn get_status(&self, agent_id: &str) -> Result<Option<AgentStatusEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            "SELECT agent_id, status, last_heartbeat, metadata FROM agent_statuses WHERE agent_id = ?1"
        )?;

        stmt.query_row(params![agent_id], |row| {
            Ok(AgentStatusEntry {
                agent_id: row.get(0)?,
                status: row.get(1)?,
                last_heartbeat: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                metadata: row
                    .get::<_, Option<String>>(3)?
                    .map(|s| serde_json::from_str(&s))
                    .transpose()
                    .unwrap_or(None),
            })
        })
        .optional()
        .map_err(|e| anyhow::anyhow!(e))
    }

    /// Remove stale agent statuses (not updated for a while)
    pub fn cleanup_stale_statuses(&self, max_age_seconds: i64) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let rows_affected = conn.execute(
            "DELETE FROM agent_statuses WHERE datetime(last_heartbeat) < datetime('now', ?1 || ' seconds')",
            params![format!("-{}", max_age_seconds)],
        )?;

        Ok(rows_affected)
    }

    // ========================================================================
    // Utility Functions
    // ========================================================================

    /// Check if vector search is available
    pub fn is_vss_enabled(&self) -> bool {
        self.vss_enabled
    }

    /// Convert embedding vector to blob for storage
    fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
        let mut blob = Vec::with_capacity(embedding.len() * 4);
        for &val in embedding {
            blob.extend_from_slice(&val.to_le_bytes());
        }
        blob
    }

    /// Convert blob back to embedding vector
    fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
        let len = blob.len() / 4;
        let mut embedding = Vec::with_capacity(len);
        for chunk in blob.chunks_exact(4) {
            let bytes: [u8; 4] = chunk.try_into().unwrap_or([0; 4]);
            embedding.push(f32::from_le_bytes(bytes));
        }
        embedding
    }

    /// Calculate cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if mag_a == 0.0 || mag_b == 0.0 {
            return 0.0;
        }

        dot_product / (mag_a * mag_b)
    }

    /// Helper to convert a database row to a Memory
    fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<Memory> {
        Ok(Memory {
            id: row.get(0)?,
            org: row.get(1)?,
            agent_id: row.get(2)?,
            content: row.get(3)?,
            embedding: row
                .get::<_, Option<Vec<u8>>>(4)?
                .map(|b| Self::blob_to_embedding(&b)),
            metadata: row
                .get::<_, Option<String>>(5)?
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .unwrap_or(None),
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }

    /// Helper to convert a database row to a Task
    fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
        Ok(Task {
            id: row.get(0)?,
            from_agent: row.get(1)?,
            to_agent: row.get(2)?,
            task_type: row.get(3)?,
            payload: row
                .get::<_, Option<String>>(4)?
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .unwrap_or(None),
            priority: row.get(5)?,
            status: row
                .get::<_, String>(6)?
                .parse()
                .unwrap_or(TaskStatus::Pending),
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            claimed_at: row.get::<_, Option<String>>(8)?.map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now())
            }),
            completed_at: row.get::<_, Option<String>>(9)?.map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now())
            }),
        })
    }
}

impl Default for SharedMemory {
    fn default() -> Self {
        Self::new().expect("Failed to create default SharedMemory")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_memory() -> SharedMemory {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");

        // Keep temp dir alive by leaking it (for test simplicity)
        std::mem::forget(dir);

        SharedMemory::with_config(SharedMemoryConfig {
            database_path: db_path,
            vss_extension_path: None,
            embedding_dim: 4, // Small for testing
        })
        .expect("Failed to create test memory")
    }

    #[test]
    fn test_store_and_retrieve_memory() {
        let mem = create_test_memory();

        let new_mem = NewMemory {
            org: Some("test-org".to_string()),
            agent_id: "agent-1".to_string(),
            content: "Test memory content".to_string(),
            embedding: Some(vec![0.1, 0.2, 0.3, 0.4]),
            metadata: Some(serde_json::json!({"key": "value"})),
        };

        let id = mem
            .store_memory(None, &new_mem)
            .expect("Failed to store memory");
        assert!(id > 0);

        let retrieved = mem.get_memory(id).expect("Failed to get memory");
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.org, "test-org");
        assert_eq!(retrieved.agent_id, "agent-1");
        assert_eq!(retrieved.content, "Test memory content");
        assert_eq!(retrieved.embedding, Some(vec![0.1, 0.2, 0.3, 0.4]));
    }

    #[test]
    fn test_store_memory_default_org() {
        let mem = create_test_memory();

        let new_mem = NewMemory {
            org: None,
            agent_id: "agent-1".to_string(),
            content: "Test memory content".to_string(),
            embedding: None,
            metadata: None,
        };

        let id = mem
            .store_memory(None, &new_mem)
            .expect("Failed to store memory");
        let retrieved = mem.get_memory(id).expect("Failed to get memory").unwrap();
        assert_eq!(retrieved.org, ORG_DEFAULT);
    }

    #[test]
    fn test_search_memories_fallback() {
        let mem = create_test_memory();

        // Store some memories with different embeddings
        mem.store_memory(
            None,
            &NewMemory {
                org: Some("org-1".to_string()),
                agent_id: "agent-1".to_string(),
                content: "First memory".to_string(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: None,
            },
        )
        .unwrap();

        mem.store_memory(
            None,
            &NewMemory {
                org: Some("org-1".to_string()),
                agent_id: "agent-2".to_string(),
                content: "Second memory".to_string(),
                embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
                metadata: None,
            },
        )
        .unwrap();

        mem.store_memory(
            None,
            &NewMemory {
                org: Some("org-2".to_string()),
                agent_id: "agent-3".to_string(),
                content: "Third memory (different org)".to_string(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: None,
            },
        )
        .unwrap();

        // Search with a query similar to first memory, scoped to org-1
        let results = mem
            .search_memories("org-1", &[0.9, 0.1, 0.0, 0.0], 10)
            .expect("Search failed");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].memory.content, "First memory");
        assert!(results[0].similarity > results[1].similarity);

        // Search across all orgs
        let all_results = mem
            .search_memories(ORG_ALL, &[0.9, 0.1, 0.0, 0.0], 10)
            .expect("Search failed");
        assert_eq!(all_results.len(), 3);

        // Search common org (should be empty)
        let common_results = mem
            .search_memories(ORG_COMMON, &[0.9, 0.1, 0.0, 0.0], 10)
            .expect("Search failed");
        assert_eq!(common_results.len(), 0);
    }

    #[test]
    fn test_task_queue() {
        let mem = create_test_memory();

        // Push tasks
        let task1_id = mem
            .push_task(&NewTask {
                from_agent: "agent-1".to_string(),
                to_agent: Some("agent-2".to_string()),
                task_type: "process".to_string(),
                payload: Some(serde_json::json!({"data": "test"})),
                priority: 1,
            })
            .unwrap();

        let task2_id = mem
            .push_task(&NewTask {
                from_agent: "agent-1".to_string(),
                to_agent: None,
                task_type: "broadcast".to_string(),
                payload: None,
                priority: 5, // Higher priority
            })
            .unwrap();

        // Pop should get higher priority task first
        let popped = mem.pop_task(None).unwrap().unwrap();
        assert_eq!(popped.id, task2_id);
        assert_eq!(popped.status, TaskStatus::Claimed);

        // Pop for specific agent
        let popped2 = mem.pop_task(Some("agent-2")).unwrap().unwrap();
        assert_eq!(popped2.id, task1_id);
    }

    #[test]
    fn test_agent_status() {
        let mem = create_test_memory();

        mem.update_status("agent-1", "running", Some(serde_json::json!({"cpu": 50})))
            .unwrap();
        mem.update_status("agent-2", "idle", None).unwrap();

        let statuses = mem.get_all_statuses().unwrap();
        assert_eq!(statuses.len(), 2);

        let status1 = mem.get_status("agent-1").unwrap().unwrap();
        assert_eq!(status1.status, "running");
        assert!(status1.metadata.is_some());
    }

    #[test]
    fn test_delete_memory() {
        let mem = create_test_memory();

        let id = mem
            .store_memory(
                None,
                &NewMemory {
                    org: None,
                    agent_id: "agent-1".to_string(),
                    content: "To be deleted".to_string(),
                    embedding: None,
                    metadata: None,
                },
            )
            .unwrap();

        assert!(mem.delete(id).unwrap());
        assert!(!mem.delete(id).unwrap()); // Already deleted
        assert!(mem.get_memory(id).unwrap().is_none());
    }

    #[test]
    fn test_delete_agent_memories() {
        let mem = create_test_memory();

        // Store memories for agent-1 in org-1
        mem.store_memory(
            None,
            &NewMemory {
                org: Some("org-1".to_string()),
                agent_id: "agent-1".to_string(),
                content: "Memory 1".to_string(),
                embedding: None,
                metadata: None,
            },
        )
        .unwrap();

        mem.store_memory(
            None,
            &NewMemory {
                org: Some("org-1".to_string()),
                agent_id: "agent-1".to_string(),
                content: "Memory 2".to_string(),
                embedding: None,
                metadata: None,
            },
        )
        .unwrap();

        // Store memory for agent-1 in org-2
        mem.store_memory(
            None,
            &NewMemory {
                org: Some("org-2".to_string()),
                agent_id: "agent-1".to_string(),
                content: "Memory 3".to_string(),
                embedding: None,
                metadata: None,
            },
        )
        .unwrap();

        // Delete agent-1 memories in org-1
        let deleted = mem.delete_agent_memories("org-1", "agent-1").unwrap();
        assert_eq!(deleted, 2);

        // Verify org-1 memories are deleted
        let org1_memories = mem.list_all(Some("org-1"), Some("agent-1")).unwrap();
        assert_eq!(org1_memories.len(), 0);

        // Verify org-2 memory still exists
        let org2_memories = mem.list_all(Some("org-2"), Some("agent-1")).unwrap();
        assert_eq!(org2_memories.len(), 1);
    }

    #[test]
    fn test_list_all_with_org_filter() {
        let mem = create_test_memory();

        mem.store_memory(
            None,
            &NewMemory {
                org: Some("org-a".to_string()),
                agent_id: "agent-1".to_string(),
                content: "Memory A1".to_string(),
                embedding: None,
                metadata: None,
            },
        )
        .unwrap();

        mem.store_memory(
            None,
            &NewMemory {
                org: Some("org-a".to_string()),
                agent_id: "agent-2".to_string(),
                content: "Memory A2".to_string(),
                embedding: None,
                metadata: None,
            },
        )
        .unwrap();

        mem.store_memory(
            None,
            &NewMemory {
                org: Some("org-b".to_string()),
                agent_id: "agent-1".to_string(),
                content: "Memory B1".to_string(),
                embedding: None,
                metadata: None,
            },
        )
        .unwrap();

        // List all in org-a
        let org_a = mem.list_all(Some("org-a"), None).unwrap();
        assert_eq!(org_a.len(), 2);

        // List all in org-b
        let org_b = mem.list_all(Some("org-b"), None).unwrap();
        assert_eq!(org_b.len(), 1);

        // List all across all orgs
        let all = mem.list_all(Some(ORG_ALL), None).unwrap();
        assert_eq!(all.len(), 3);

        // List all with org and agent filter
        let org_a_agent1 = mem.list_all(Some("org-a"), Some("agent-1")).unwrap();
        assert_eq!(org_a_agent1.len(), 1);
        assert_eq!(org_a_agent1[0].content, "Memory A1");
    }
}
