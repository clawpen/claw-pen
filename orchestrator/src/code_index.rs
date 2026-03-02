//! Semantic Code Indexing System for Claw Pen
//!
//! This module provides efficient semantic indexing for large codebases,
//! enabling agents to search and retrieve relevant code context.
//!
//! # Features
//! - Multi-language parsing via tree-sitter
//! - Semantic embeddings (TF-IDF fallback, Ollama, or OpenAI)
//! - Incremental indexing with mtime tracking
//! - SQLite-vss for vector similarity search
//!
//! # Example
//! ```ignore
//! use code_index::{CodeIndex, IndexConfig, EmbeddingModel};
//!
//! let config = IndexConfig {
//!     root: PathBuf::from("/path/to/project"),
//!     include_patterns: vec!["**/*.rs".into(), "**/*.py".into()],
//!     exclude_patterns: vec!["**/target/**".into(), "**/node_modules/**".into()],
//!     max_file_size_kb: 500,
//!     embedding_model: EmbeddingModel::TfIdf,
//!     chunk_size: 1000,
//! };
//!
//! let index = CodeIndex::new(config)?;
//! index.build_index().await?;
//!
//! let results = index.search("authentication logic", 10).await?;
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, BTreeMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use tracing::{debug, info, warn};

// ============================================================================
// Configuration Types
// ============================================================================

/// Configuration for the code index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    /// Root directory to index
    pub root: PathBuf,
    
    /// Glob patterns for files to include (e.g., ["**/*.rs", "**/*.py"])
    pub include_patterns: Vec<String>,
    
    /// Glob patterns for files to exclude (e.g., ["**/target/**", "**/node_modules/**"])
    pub exclude_patterns: Vec<String>,
    
    /// Maximum file size in KB to index (skip larger files)
    pub max_file_size_kb: u32,
    
    /// Embedding model to use
    pub embedding_model: EmbeddingModel,
    
    /// Size of text chunks for splitting large files
    pub chunk_size: usize,
    
    /// Database path (defaults to root/.code_index.db)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_path: Option<PathBuf>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            include_patterns: vec![
                "**/*.rs".into(),
                "**/*.py".into(),
                "**/*.js".into(),
                "**/*.ts".into(),
                "**/*.go".into(),
                "**/*.java".into(),
                "**/*.c".into(),
                "**/*.cpp".into(),
                "**/*.h".into(),
                "**/*.md".into(),
                "**/*.toml".into(),
                "**/*.yaml".into(),
                "**/*.yml".into(),
                "**/*.json".into(),
            ],
            exclude_patterns: vec![
                "**/target/**".into(),
                "**/node_modules/**".into(),
                "**/.git/**".into(),
                "**/dist/**".into(),
                "**/build/**".into(),
                "**/.venv/**".into(),
                "**/__pycache__/**".into(),
                "**/*.lock".into(),
                "**/Cargo.lock".into(),
                "**/package-lock.json".into(),
            ],
            max_file_size_kb: 500,
            embedding_model: EmbeddingModel::TfIdf,
            chunk_size: 1000,
            db_path: None,
        }
    }
}

// ============================================================================
// Embedding Model Types
// ============================================================================

/// Supported embedding models
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingModel {
    /// Local Ollama with nomic-embed-text
    LocalOllama,
    
    /// OpenAI text-embedding-3-small
    OpenAI,
    
    /// TF-IDF fallback (no API needed)
    TfIdf,
}

impl Default for EmbeddingModel {
    fn default() -> Self {
        Self::TfIdf
    }
}

// ============================================================================
// Symbol Types
// ============================================================================

/// Kind of code symbol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Interface,
    Enum,
    Constant,
    Variable,
    Module,
    Trait,
    Method,
    Property,
    TypeAlias,
    Macro,
}

impl SymbolKind {
    /// Get display name for the symbol kind
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::Constant => "constant",
            Self::Variable => "variable",
            Self::Module => "module",
            Self::Trait => "trait",
            Self::Method => "method",
            Self::Property => "property",
            Self::TypeAlias => "type_alias",
            Self::Macro => "macro",
        }
    }
}

/// A code symbol (function, class, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol name
    pub name: String,
    
    /// Kind of symbol
    pub kind: SymbolKind,
    
    /// Starting line number (1-indexed)
    pub line_start: u32,
    
    /// Ending line number (1-indexed)
    pub line_end: u32,
    
    /// Symbol-level embedding (for semantic search)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    
    /// Full signature or definition text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    
    /// Documentation comments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
}

// ============================================================================
// Indexed File Types
// ============================================================================

/// A file that has been indexed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    /// File path relative to root
    pub path: PathBuf,
    
    /// Last modification time
    pub mtime: DateTime<Utc>,
    
    /// File size in bytes
    pub size_bytes: u64,
    
    /// Detected language
    pub language: String,
    
    /// Symbols found in the file
    pub symbols: Vec<Symbol>,
    
    /// File-level embedding
    pub embedding: Vec<f32>,
    
    /// Content chunks for large files
    pub chunks: Vec<ContentChunk>,
}

/// A chunk of content from a large file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentChunk {
    /// Chunk index
    pub index: usize,
    
    /// Starting line number
    pub line_start: u32,
    
    /// Ending line number
    pub line_end: u32,
    
    /// Chunk content
    pub content: String,
    
    /// Chunk embedding
    pub embedding: Vec<f32>,
}

// ============================================================================
// Search Types
// ============================================================================

/// A search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// File path
    pub file: PathBuf,
    
    /// Similarity score (0.0 to 1.0)
    pub score: f32,
    
    /// Relevant code snippet
    pub snippet: String,
    
    /// Matching symbol if applicable
    pub symbol: Option<Symbol>,
    
    /// Chunk index if result is from a chunk
    pub chunk_index: Option<usize>,
    
    /// Line numbers for the snippet
    pub line_range: Option<(u32, u32)>,
}

/// Search options
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Maximum results to return
    pub limit: usize,
    
    /// Minimum similarity score
    pub min_score: f32,
    
    /// Filter by file pattern
    pub file_filter: Option<String>,
    
    /// Filter by symbol kind
    pub symbol_kind: Option<SymbolKind>,
    
    /// Include file content in results
    pub include_content: bool,
}

// ============================================================================
// Statistics Types
// ============================================================================

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Total files indexed
    pub total_files: usize,
    
    /// Total symbols indexed
    pub total_symbols: usize,
    
    /// Total chunks
    pub total_chunks: usize,
    
    /// Languages detected with file counts
    pub languages: BTreeMap<String, usize>,
    
    /// Symbol kinds with counts
    pub symbol_kinds: BTreeMap<String, usize>,
    
    /// Last index time
    pub last_indexed: Option<DateTime<Utc>>,
    
    /// Index size in bytes
    pub index_size_bytes: u64,
    
    /// Embedding model in use
    pub embedding_model: String,
}

// ============================================================================
// TF-IDF Implementation
// ============================================================================

/// TF-IDF vectorizer for fallback embeddings
#[derive(Debug, Clone, Default)]
pub struct TfIdfVectorizer {
    /// Document frequency for each term
    document_freq: HashMap<String, usize>,
    
    /// Total number of documents
    num_docs: usize,
    
    /// Vocabulary size (for dimension)
    vocab_size: usize,
    
    /// Term to index mapping
    term_to_idx: HashMap<String, usize>,
}

impl TfIdfVectorizer {
    /// Create a new TF-IDF vectorizer
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Fit the vectorizer on a corpus
    pub fn fit(&mut self, documents: &[&str]) {
        self.num_docs = documents.len();
        self.term_to_idx.clear();
        self.document_freq.clear();
        
        // Build vocabulary and document frequencies
        for doc in documents {
            let terms = self.tokenize(doc);
            let unique_terms: HashSet<_> = terms.into_iter().collect();
            
            for term in unique_terms {
                *self.document_freq.entry(term.clone()).or_insert(0) += 1;
                if !self.term_to_idx.contains_key(&term) {
                    self.term_to_idx.insert(term, self.vocab_size);
                    self.vocab_size += 1;
                }
            }
        }
        
        debug!("TF-IDF vocabulary size: {}", self.vocab_size);
    }
    
    /// Transform a document to TF-IDF vector
    pub fn transform(&self, document: &str) -> Vec<f32> {
        if self.vocab_size == 0 || self.num_docs == 0 {
            return vec![];
        }
        
        let mut vector = vec![0.0f32; self.vocab_size];
        let terms = self.tokenize(document);
        let term_freq = self.compute_term_freq(&terms);
        
        for (term, tf) in term_freq {
            if let Some(&idx) = self.term_to_idx.get(&term) {
                let df = self.document_freq.get(&term).copied().unwrap_or(1);
                let idf = ((self.num_docs as f32 + 1.0) / (df as f32 + 1.0)).ln() + 1.0;
                vector[idx] = tf * idf;
            }
        }
        
        // L2 normalize
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut vector {
                *val /= norm;
            }
        }
        
        vector
    }
    
    /// Fit and transform in one step
    pub fn fit_transform(&mut self, documents: &[&str]) -> Vec<Vec<f32>> {
        self.fit(documents);
        documents.iter().map(|doc| self.transform(doc)).collect()
    }
    
    /// Compute cosine similarity between two vectors
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm_a > 0.0 && norm_b > 0.0 {
            dot / (norm_a * norm_b)
        } else {
            0.0
        }
    }
    
    /// Tokenize text into terms
    fn tokenize(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| s.len() > 1)
            .map(|s| s.to_string())
            .collect()
    }
    
    /// Compute term frequencies
    fn compute_term_freq(&self, terms: &[String]) -> HashMap<String, f32> {
        let mut freq = HashMap::new();
        let total = terms.len() as f32;
        
        for term in terms {
            *freq.entry(term.clone()).or_insert(0.0) += 1.0;
        }
        
        if total > 0.0 {
            for count in freq.values_mut() {
                *count /= total;
            }
        }
        
        freq
    }
}

// ============================================================================
// Embedding Generator
// ============================================================================

/// Embedding generator that supports multiple backends
pub struct EmbeddingGenerator {
    model: EmbeddingModel,
    tfidf: Arc<RwLock<TfIdfVectorizer>>,
    ollama_url: Option<String>,
    openai_key: Option<String>,
}

impl EmbeddingGenerator {
    /// Create a new embedding generator
    pub fn new(model: EmbeddingModel) -> Self {
        Self {
            model,
            tfidf: Arc::new(RwLock::new(TfIdfVectorizer::new())),
            ollama_url: None,
            openai_key: None,
        }
    }
    
    /// Set Ollama URL
    pub fn with_ollama_url(mut self, url: String) -> Self {
        self.ollama_url = Some(url);
        self
    }
    
    /// Set OpenAI API key
    pub fn with_openai_key(mut self, key: String) -> Self {
        self.openai_key = Some(key);
        self
    }
    
    /// Fit the TF-IDF model on a corpus (only needed for TfIdf backend)
    pub fn fit_tfidf(&self, documents: &[&str]) {
        if matches!(self.model, EmbeddingModel::TfIdf) {
            let mut tfidf = self.tfidf.write().unwrap();
            tfidf.fit(documents);
        }
    }
    
    /// Generate embedding for text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match &self.model {
            EmbeddingModel::TfIdf => {
                let tfidf = self.tfidf.read().unwrap();
                Ok(tfidf.transform(text))
            }
            EmbeddingModel::LocalOllama => {
                self.embed_ollama(text).await
            }
            EmbeddingModel::OpenAI => {
                self.embed_openai(text).await
            }
        }
    }
    
    /// Generate embeddings for multiple texts
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // For now, process sequentially. Could be parallelized later.
        let mut embeddings = Vec::with_capacity(texts.len());
        for text in texts {
            embeddings.push(self.embed(text).await?);
        }
        Ok(embeddings)
    }
    
    /// Embed using Ollama
    async fn embed_ollama(&self, text: &str) -> Result<Vec<f32>> {
        let url = self.ollama_url.as_deref()
            .unwrap_or("http://localhost:11434");
        
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/api/embeddings", url))
            .json(&serde_json::json!({
                "model": "nomic-embed-text",
                "prompt": text
            }))
            .send()
            .await
            .context("Failed to call Ollama API")?;
        
        if !response.status().is_success() {
            anyhow::bail!("Ollama API error: {}", response.status());
        }
        
        let json: serde_json::Value = response.json().await?;
        let embedding = json["embedding"]
            .as_array()
            .context("Invalid Ollama response")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();
        
        Ok(embedding)
    }
    
    /// Embed using OpenAI
    async fn embed_openai(&self, text: &str) -> Result<Vec<f32>> {
        let api_key = self.openai_key.as_ref()
            .context("OpenAI API key not set")?;
        
        let client = reqwest::Client::new();
        let response = client
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "model": "text-embedding-3-small",
                "input": text
            }))
            .send()
            .await
            .context("Failed to call OpenAI API")?;
        
        if !response.status().is_success() {
            anyhow::bail!("OpenAI API error: {}", response.status());
        }
        
        let json: serde_json::Value = response.json().await?;
        let embedding = json["data"][0]["embedding"]
            .as_array()
            .context("Invalid OpenAI response")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();
        
        Ok(embedding)
    }
    
    /// Get the embedding dimension
    pub fn dimension(&self) -> usize {
        match &self.model {
            EmbeddingModel::TfIdf => {
                self.tfidf.read().unwrap().vocab_size
            }
            EmbeddingModel::LocalOllama => 768,  // nomic-embed-text
            EmbeddingModel::OpenAI => 1536,      // text-embedding-3-small
        }
    }
}

// ============================================================================
// Language Detection
// ============================================================================

/// Detect language from file extension
pub fn detect_language(path: &Path) -> String {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    match ext.as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "tsx" => "typescript",
        "jsx" => "javascript",
        "go" => "go",
        "java" => "java",
        "kt" => "kotlin",
        "kts" => "kotlin",
        "c" => "c",
        "cpp" | "cc" | "cxx" => "cpp",
        "h" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "m" => "objective-c",
        "mm" => "objective-cpp",
        "scala" => "scala",
        "rs" => "rust",
        "lua" => "lua",
        "r" => "r",
        "sh" | "bash" => "bash",
        "zsh" => "zsh",
        "ps1" => "powershell",
        "sql" => "sql",
        "html" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "less" => "less",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "md" => "markdown",
        "rst" => "restructuredtext",
        "vue" => "vue",
        "svelte" => "svelte",
        _ => "unknown",
    }.to_string()
}

// ============================================================================
// Tree-Sitter Parser (Stub)
// ============================================================================

/// Tree-sitter based code parser
/// 
/// Note: Full implementation requires tree-sitter crate and language bindings.
/// This is a stub that extracts symbols using simple regex patterns.
pub struct CodeParser {
    // In a full implementation, this would hold tree-sitter parsers
    // for each language
}

impl CodeParser {
    /// Create a new code parser
    pub fn new() -> Self {
        Self {}
    }
    
    /// Parse a file and extract symbols
    pub fn parse(&self, content: &str, language: &str) -> Vec<Symbol> {
        match language {
            "rust" => self.parse_rust(content),
            "python" => self.parse_python(content),
            "javascript" | "typescript" => self.parse_js_ts(content),
            "go" => self.parse_go(content),
            _ => self.parse_generic(content),
        }
    }
    
    /// Parse Rust code
    fn parse_rust(&self, content: &str) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        
        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            
            // Functions
            if line.starts_with("pub fn ") || line.starts_with("fn ") {
                if let Some(name) = self.extract_name(line, "fn") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(line.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Structs
            else if line.starts_with("pub struct ") || line.starts_with("struct ") {
                if let Some(name) = self.extract_name(line, "struct") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Struct,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(line.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Enums
            else if line.starts_with("pub enum ") || line.starts_with("enum ") {
                if let Some(name) = self.extract_name(line, "enum") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Enum,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(line.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Traits
            else if line.starts_with("pub trait ") || line.starts_with("trait ") {
                if let Some(name) = self.extract_name(line, "trait") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Trait,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(line.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Constants
            else if line.starts_with("pub const ") || line.starts_with("const ") {
                if let Some(name) = self.extract_name(line, "const") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Constant,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(line.to_string()),
                        doc_comment: None,
                    });
                }
            }
        }
        
        symbols
    }
    
    /// Parse Python code
    fn parse_python(&self, content: &str) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        
        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            
            // Functions
            if trimmed.starts_with("def ") {
                if let Some(name) = self.extract_name(trimmed, "def") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(trimmed.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Classes
            else if trimmed.starts_with("class ") {
                if let Some(name) = self.extract_name(trimmed, "class") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Class,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(trimmed.to_string()),
                        doc_comment: None,
                    });
                }
            }
        }
        
        symbols
    }
    
    /// Parse JavaScript/TypeScript code
    fn parse_js_ts(&self, content: &str) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        
        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            
            // Functions (various forms)
            if trimmed.starts_with("function ") 
                || trimmed.starts_with("export function ")
                || trimmed.starts_with("async function ") {
                if let Some(name) = self.extract_name(trimmed, "function") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(trimmed.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Classes
            else if trimmed.starts_with("class ") || trimmed.starts_with("export class ") {
                if let Some(name) = self.extract_name(trimmed, "class") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Class,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(trimmed.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Interfaces (TypeScript)
            else if trimmed.starts_with("interface ") || trimmed.starts_with("export interface ") {
                if let Some(name) = self.extract_name(trimmed, "interface") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Interface,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(trimmed.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Const declarations
            else if trimmed.starts_with("const ") || trimmed.starts_with("export const ") {
                if let Some(name) = self.extract_name_eq(trimmed, "const") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Constant,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(trimmed.to_string()),
                        doc_comment: None,
                    });
                }
            }
        }
        
        symbols
    }
    
    /// Parse Go code
    fn parse_go(&self, content: &str) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        
        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            
            // Functions
            if trimmed.starts_with("func ") {
                if let Some(name) = self.extract_name_go_func(trimmed) {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(trimmed.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Structs (type X struct)
            else if trimmed.starts_with("type ") && trimmed.contains(" struct") {
                if let Some(name) = self.extract_name(trimmed, "type") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Struct,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(trimmed.to_string()),
                        doc_comment: None,
                    });
                }
            }
            
            // Interfaces (type X interface)
            else if trimmed.starts_with("type ") && trimmed.contains(" interface") {
                if let Some(name) = self.extract_name(trimmed, "type") {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Interface,
                        line_start: line_num as u32 + 1,
                        line_end: line_num as u32 + 1,
                        embedding: None,
                        signature: Some(trimmed.to_string()),
                        doc_comment: None,
                    });
                }
            }
        }
        
        symbols
    }
    
    /// Generic parser for unknown languages
    fn parse_generic(&self, content: &str) -> Vec<Symbol> {
        // Just return empty for unknown languages
        let _ = content;
        Vec::new()
    }
    
    /// Extract name after a keyword (e.g., "fn name(")
    fn extract_name(&self, line: &str, keyword: &str) -> Option<String> {
        let after_keyword = line.split(keyword).nth(1)?;
        let name = after_keyword
            .trim()
            .split(|c: char| c == '(' || c == '<' || c == '{' || c == ':' || c == '=')
            .next()?
            .trim()
            .to_string();
        
        if name.is_empty() { None } else { Some(name) }
    }
    
    /// Extract name for const declarations (e.g., "const NAME =")
    fn extract_name_eq(&self, line: &str, keyword: &str) -> Option<String> {
        let after_keyword = line.split(keyword).nth(1)?;
        let name = after_keyword
            .trim()
            .split('=')
            .next()?
            .trim()
            .to_string();
        
        if name.is_empty() { None } else { Some(name) }
    }
    
    /// Extract Go function name (handles both "func Name()" and "func (r Type) Name()")
    fn extract_name_go_func(&self, line: &str) -> Option<String> {
        let after_func = line.strip_prefix("func ")?;
        
        // Check for method receiver
        if after_func.starts_with('(') {
            // Method: func (r Type) Name()
            let after_receiver = after_func.split(')').nth(1)?;
            let name = after_receiver
                .trim()
                .split('(')
                .next()?
                .trim()
                .to_string();
            if name.is_empty() { None } else { Some(name) }
        } else {
            // Function: func Name()
            let name = after_func
                .split('(')
                .next()?
                .trim()
                .to_string();
            if name.is_empty() { None } else { Some(name) }
        }
    }
}

impl Default for CodeParser {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Main Code Index
// ============================================================================

/// The main code index structure
pub struct CodeIndex {
    /// SQLite connection for storage
    db: Connection,
    
    /// Root directory being indexed
    root: PathBuf,
    
    /// Configuration
    config: IndexConfig,
    
    /// Embedding generator
    embedding: EmbeddingGenerator,
    
    /// Code parser
    parser: CodeParser,
    
    /// Index state
    indexed: RwLock<bool>,
}

impl CodeIndex {
    /// Create a new code index
    pub fn new(config: IndexConfig) -> Result<Self> {
        let db_path = config.db_path.clone()
            .unwrap_or_else(|| config.root.join(".code_index.db"));
        
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let db = Connection::open(&db_path)
            .context("Failed to open index database")?;
        
        let embedding = EmbeddingGenerator::new(config.embedding_model.clone());
        let parser = CodeParser::new();
        
        let mut index = Self {
            db,
            root: config.root.clone(),
            config,
            embedding,
            parser,
            indexed: RwLock::new(false),
        };
        
        index.init_db()?;
        
        Ok(index)
    }
    
    /// Initialize database schema
    fn init_db(&self) -> Result<()> {
        self.db.execute_batch(
            r#"
            -- Files table
            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                mtime TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                language TEXT NOT NULL,
                content_hash TEXT,
                indexed_at TEXT NOT NULL
            );
            
            -- Symbols table
            CREATE TABLE IF NOT EXISTS symbols (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                signature TEXT,
                doc_comment TEXT,
                FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
            );
            
            -- Chunks table
            CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                chunk_index INTEGER NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                content TEXT NOT NULL,
                FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
            );
            
            -- Embeddings table (sqlite-vss compatible structure)
            -- Note: Full vector search requires sqlite-vss extension
            -- This is a simplified version that stores embeddings as BLOBs
            CREATE TABLE IF NOT EXISTS file_embeddings (
                file_id INTEGER PRIMARY KEY,
                embedding BLOB NOT NULL,
                FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
            );
            
            CREATE TABLE IF NOT EXISTS symbol_embeddings (
                symbol_id INTEGER PRIMARY KEY,
                embedding BLOB NOT NULL,
                FOREIGN KEY (symbol_id) REFERENCES symbols(id) ON DELETE CASCADE
            );
            
            CREATE TABLE IF NOT EXISTS chunk_embeddings (
                chunk_id INTEGER PRIMARY KEY,
                embedding BLOB NOT NULL,
                FOREIGN KEY (chunk_id) REFERENCES chunks(id) ON DELETE CASCADE
            );
            
            -- Index metadata
            CREATE TABLE IF NOT EXISTS index_metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            
            -- Create indexes for faster queries
            CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
            CREATE INDEX IF NOT EXISTS idx_symbols_file_id ON symbols(file_id);
            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
            CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);
            CREATE INDEX IF NOT EXISTS idx_chunks_file_id ON chunks(file_id);
            "#,
        )?;
        
        Ok(())
    }
    
    /// Build or update the index
    pub async fn build_index(&self) -> Result<IndexStats> {
        info!("Building code index for {:?}", self.root);
        
        // Collect all documents first for TF-IDF fitting
        let mut all_docs = Vec::new();
        let mut files_to_index = Vec::new();
        
        // Walk directory tree
        for entry in walkdir::WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            
            if !path.is_file() {
                continue;
            }
            
            // Check include/exclude patterns
            if !self.should_index(path) {
                continue;
            }
            
            // Check file size
            let metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            
            let size_kb = metadata.len() / 1024;
            if size_kb > self.config.max_file_size_kb as u64 {
                debug!("Skipping large file: {:?}", path);
                continue;
            }
            
            // Check if file needs re-indexing
            let mtime = metadata.modified()?;
            if !self.needs_reindex(path, &mtime)? {
                debug!("File unchanged, skipping: {:?}", path);
                continue;
            }
            
            // Read file content
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,  // Skip binary files
            };
            
            all_docs.push(content.clone());
            files_to_index.push((path.to_path_buf(), content, mtime, metadata.len()));
        }
        
        info!("Found {} files to index", files_to_index.len());
        
        // Fit TF-IDF if using that model
        if matches!(self.config.embedding_model, EmbeddingModel::TfIdf) {
            let docs: Vec<&str> = all_docs.iter().map(|s| s.as_str()).collect();
            self.embedding.fit_tfidf(&docs);
        }
        
        // Index each file
        let mut total_symbols = 0;
        let mut total_chunks = 0;
        let mut languages = BTreeMap::new();
        let mut symbol_kinds = BTreeMap::new();
        
        for (path, content, mtime, size) in files_to_index {
            let stats = self.index_file(&path, &content, mtime, size).await?;
            total_symbols += stats.0;
            total_chunks += stats.1;
            *languages.entry(stats.2).or_insert(0) += 1;
            for kind in stats.3 {
                *symbol_kinds.entry(kind).or_insert(0) += 1;
            }
        }
        
        // Update metadata
        let now = Utc::now().to_rfc3339();
        self.db.execute(
            "INSERT OR REPLACE INTO index_metadata (key, value) VALUES ('last_indexed', ?)",
            params![now],
        )?;
        
        *self.indexed.write().unwrap() = true;
        
        let stats = self.get_stats()?;
        info!("Index complete: {} files, {} symbols", stats.total_files, stats.total_symbols);
        
        Ok(stats)
    }
    
    /// Index a single file
    async fn index_file(
        &self,
        path: &Path,
        content: &str,
        mtime: SystemTime,
        size: u64,
    ) -> Result<(usize, usize, String, Vec<String>)> {
        let relative_path = path.strip_prefix(&self.root)?;
        let language = detect_language(path);
        let mtime_dt: DateTime<Utc> = mtime.into();
        let indexed_at = Utc::now().to_rfc3339();
        
        // Delete existing entries
        self.db.execute(
            "DELETE FROM files WHERE path = ?",
            params![relative_path.to_string_lossy().to_string()],
        )?;
        
        // Insert file record
        self.db.execute(
            "INSERT INTO files (path, mtime, size_bytes, language, indexed_at) VALUES (?, ?, ?, ?, ?)",
            params![
                relative_path.to_string_lossy().to_string(),
                mtime_dt.to_rfc3339(),
                size as i64,
                &language,
                indexed_at,
            ],
        )?;
        
        let file_id = self.db.last_insert_rowid();
        
        // Parse symbols
        let symbols = self.parser.parse(content, &language);
        let symbol_count = symbols.len();
        let mut symbol_kinds = Vec::new();
        
        for symbol in &symbols {
            self.db.execute(
                "INSERT INTO symbols (file_id, name, kind, line_start, line_end, signature, doc_comment) VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    file_id,
                    &symbol.name,
                    symbol.kind.as_str(),
                    symbol.line_start as i32,
                    symbol.line_end as i32,
                    &symbol.signature,
                    &symbol.doc_comment,
                ],
            )?;
            
            let symbol_id = self.db.last_insert_rowid();
            
            // Generate and store symbol embedding
            let symbol_text = format!("{} {} {}", symbol.name, symbol.kind.as_str(), symbol.signature.as_deref().unwrap_or(""));
            let embedding = self.embedding.embed(&symbol_text).await?;
            let embedding_bytes = Self::embedding_to_bytes(&embedding);
            
            self.db.execute(
                "INSERT INTO symbol_embeddings (symbol_id, embedding) VALUES (?, ?)",
                params![symbol_id, &embedding_bytes],
            )?;
            
            symbol_kinds.push(symbol.kind.as_str().to_string());
        }
        
        // Split into chunks if needed
        let chunks = self.chunk_content(content);
        let chunk_count = chunks.len();
        
        for (idx, chunk) in chunks.iter().enumerate() {
            self.db.execute(
                "INSERT INTO chunks (file_id, chunk_index, line_start, line_end, content) VALUES (?, ?, ?, ?, ?)",
                params![
                    file_id,
                    idx as i32,
                    chunk.line_start as i32,
                    chunk.line_end as i32,
                    &chunk.content,
                ],
            )?;
            
            let chunk_id = self.db.last_insert_rowid();
            
            // Generate and store chunk embedding
            let embedding = self.embedding.embed(&chunk.content).await?;
            let embedding_bytes = Self::embedding_to_bytes(&embedding);
            
            self.db.execute(
                "INSERT INTO chunk_embeddings (chunk_id, embedding) VALUES (?, ?)",
                params![chunk_id, &embedding_bytes],
            )?;
        }
        
        // Generate file-level embedding
        let file_embedding = self.embedding.embed(content).await?;
        let file_embedding_bytes = Self::embedding_to_bytes(&file_embedding);
        
        self.db.execute(
            "INSERT INTO file_embeddings (file_id, embedding) VALUES (?, ?)",
            params![file_id, &file_embedding_bytes],
        )?;
        
        Ok((symbol_count, chunk_count, language, symbol_kinds))
    }
    
    /// Check if a file should be indexed based on patterns
    fn should_index(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let path_str = path_str.replace('\\', "/");
        
        // Check exclude patterns first
        for pattern in &self.config.exclude_patterns {
            if glob_match::glob_match(pattern, &path_str) {
                return false;
            }
        }
        
        // Check include patterns
        for pattern in &self.config.include_patterns {
            if glob_match::glob_match(pattern, &path_str) {
                return true;
            }
        }
        
        false
    }
    
    /// Check if a file needs re-indexing
    fn needs_reindex(&self, path: &Path, mtime: &SystemTime) -> Result<bool> {
        let relative_path = path.strip_prefix(&self.root)?;
        let path_str = relative_path.to_string_lossy().to_string();
        
        let stored_mtime: Option<String> = self.db
            .query_row(
                "SELECT mtime FROM files WHERE path = ?",
                params![path_str],
                |row| row.get(0),
            )
            .optional()?;
        
        match stored_mtime {
            None => Ok(true),  // New file
            Some(stored) => {
                let stored_time: DateTime<Utc> = stored.parse()
                    .unwrap_or_else(|_| Utc::now());
                let current_time: DateTime<Utc> = (*mtime).into();
                Ok(current_time > stored_time)
            }
        }
    }
    
    /// Split content into chunks
    fn chunk_content(&self, content: &str) -> Vec<ContentChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();
        
        if lines.is_empty() {
            return chunks;
        }
        
        let mut current_chunk = String::new();
        let mut chunk_start = 1u32;
        let mut current_len = 0;
        
        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx as u32 + 1;
            
            if current_len + line.len() > self.config.chunk_size && !current_chunk.is_empty() {
                chunks.push(ContentChunk {
                    index: chunks.len(),
                    line_start: chunk_start,
                    line_end: line_num - 1,
                    content: current_chunk.clone(),
                    embedding: vec![],  // Will be filled later
                });
                
                current_chunk.clear();
                chunk_start = line_num;
                current_len = 0;
            }
            
            current_chunk.push_str(line);
            current_chunk.push('\n');
            current_len += line.len() + 1;
        }
        
        // Add final chunk
        if !current_chunk.is_empty() {
            chunks.push(ContentChunk {
                index: chunks.len(),
                line_start: chunk_start,
                line_end: lines.len() as u32,
                content: current_chunk,
                embedding: vec![],
            });
        }
        
        chunks
    }
    
    /// Convert embedding to bytes for storage
    fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(embedding.len() * 4);
        for val in embedding {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        bytes
    }
    
    /// Convert bytes to embedding
    fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }
    
    // ========================================================================
    // Search API
    // ========================================================================
    
    /// Semantic search for code
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let query_embedding = self.embedding.embed(query).await?;
        
        // Search chunks (most granular)
        let mut results = self.search_chunks(&query_embedding, limit).await?;
        
        // Also search symbols
        let symbol_results = self.search_symbols(&query_embedding, limit).await?;
        results.extend(symbol_results);
        
        // Sort by score and deduplicate
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        
        Ok(results)
    }
    
    /// Search chunks by embedding similarity
    async fn search_chunks(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let mut stmt = self.db.prepare(
            "SELECT c.file_id, c.chunk_index, c.line_start, c.line_end, c.content, e.embedding, f.path
             FROM chunks c
             JOIN chunk_embeddings e ON c.id = e.chunk_id
             JOIN files f ON c.file_id = f.id"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,  // file_id
                row.get::<_, i32>(1)?,  // chunk_index
                row.get::<_, u32>(2)?,  // line_start
                row.get::<_, u32>(3)?,  // line_end
                row.get::<_, String>(4)?,  // content
                row.get::<_, Vec<u8>>(5)?,  // embedding
                row.get::<_, String>(6)?,  // path
            ))
        })?;
        
        let mut results = Vec::new();
        
        for row_result in rows {
            let (file_id, chunk_idx, line_start, line_end, content, embedding_bytes, path) = row_result?;
            let embedding = Self::bytes_to_embedding(&embedding_bytes);
            let score = TfIdfVectorizer::cosine_similarity(query_embedding, &embedding);
            
            if score > 0.1 {  // Minimum threshold
                results.push(SearchResult {
                    file: PathBuf::from(path),
                    score,
                    snippet: self.create_snippet(&content, 200),
                    symbol: None,
                    chunk_index: Some(chunk_idx as usize),
                    line_range: Some((line_start, line_end)),
                });
            }
        }
        
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        
        Ok(results)
    }
    
    /// Search symbols by embedding similarity
    async fn search_symbols(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let mut stmt = self.db.prepare(
            "SELECT s.id, s.name, s.kind, s.line_start, s.line_end, s.signature, s.doc_comment, e.embedding, f.path
             FROM symbols s
             JOIN symbol_embeddings e ON s.id = e.symbol_id
             JOIN files f ON s.file_id = f.id"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,  // id
                row.get::<_, String>(1)?,  // name
                row.get::<_, String>(2)?,  // kind
                row.get::<_, u32>(3)?,  // line_start
                row.get::<_, u32>(4)?,  // line_end
                row.get::<_, Option<String>>(5)?,  // signature
                row.get::<_, Option<String>>(6)?,  // doc_comment
                row.get::<_, Vec<u8>>(7)?,  // embedding
                row.get::<_, String>(8)?,  // path
            ))
        })?;
        
        let mut results = Vec::new();
        
        for row_result in rows {
            let (_, name, kind, line_start, line_end, signature, doc_comment, embedding_bytes, path) = row_result?;
            let embedding = Self::bytes_to_embedding(&embedding_bytes);
            let score = TfIdfVectorizer::cosine_similarity(query_embedding, &embedding);
            
            if score > 0.1 {
                let symbol = Symbol {
                    name,
                    kind: Self::parse_symbol_kind(&kind),
                    line_start,
                    line_end,
                    embedding: None,
                    signature,
                    doc_comment,
                };
                
                let snippet = symbol.signature.clone().unwrap_or_else(|| symbol.name.clone());
                
                results.push(SearchResult {
                    file: PathBuf::from(path),
                    score,
                    snippet,
                    symbol: Some(symbol),
                    chunk_index: None,
                    line_range: Some((line_start, line_end)),
                });
            }
        }
        
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        
        Ok(results)
    }
    
    /// Parse symbol kind from string
    fn parse_symbol_kind(s: &str) -> SymbolKind {
        match s {
            "function" => SymbolKind::Function,
            "class" => SymbolKind::Class,
            "struct" => SymbolKind::Struct,
            "interface" => SymbolKind::Interface,
            "enum" => SymbolKind::Enum,
            "constant" => SymbolKind::Constant,
            "variable" => SymbolKind::Variable,
            "module" => SymbolKind::Module,
            "trait" => SymbolKind::Trait,
            "method" => SymbolKind::Method,
            "property" => SymbolKind::Property,
            "type_alias" => SymbolKind::TypeAlias,
            "macro" => SymbolKind::Macro,
            _ => SymbolKind::Function,
        }
    }
    
    /// Create a snippet from content
    fn create_snippet(&self, content: &str, max_len: usize) -> String {
        let trimmed = content.trim();
        if trimmed.len() <= max_len {
            return trimmed.to_string();
        }
        
        // Try to break at a word boundary
        let mut end = max_len;
        while end > max_len / 2 && !trimmed.chars().nth(end).map(|c| c.is_whitespace()).unwrap_or(false) {
            end -= 1;
        }
        
        format!("{}...", &trimmed[..end])
    }
    
    /// Get file content
    pub fn get_file(&self, path: &Path) -> Result<String> {
        let full_path = self.root.join(path);
        std::fs::read_to_string(&full_path)
            .with_context(|| format!("Failed to read file: {:?}", full_path))
    }
    
    /// Get relevant context for a query (within token budget)
    pub async fn get_context(&self, query: &str, max_tokens: usize) -> Result<String> {
        // Rough estimate: 4 chars per token
        let max_chars = max_tokens * 4;
        
        let results = self.search(query, 20).await?;
        
        let mut context = String::new();
        let mut total_chars = 0;
        
        for result in results {
            let file_header = format!("\n--- {} (score: {:.2}) ---\n", result.file.display(), result.score);
            
            let content = if let Some((start, end)) = result.line_range {
                // Get the relevant lines
                match self.get_file_lines(&result.file, start, end) {
                    Ok(lines) => lines,
                    Err(_) => result.snippet.clone(),
                }
            } else {
                result.snippet.clone()
            }
            
            let entry = format!("{}{}\n", file_header, content);
            
            if total_chars + entry.len() > max_chars {
                break;
            }
            
            context.push_str(&entry);
            total_chars += entry.len();
        }
        
        Ok(context)
    }
    
    /// Get specific lines from a file
    fn get_file_lines(&self, path: &Path, start: u32, end: u32) -> Result<String> {
        let content = self.get_file(path)?;
        let lines: Vec<&str> = content.lines().collect();
        
        let start_idx = (start.saturating_sub(1)) as usize;
        let end_idx = std::cmp::min(end as usize, lines.len());
        
        if start_idx < lines.len() {
            Ok(lines[start_idx..end_idx].join("\n"))
        } else {
            Ok(String::new())
        }
    }
    
    /// List all symbols, optionally filtered
    pub fn list_symbols(&self, kind_filter: Option<SymbolKind>) -> Result<Vec<(PathBuf, Symbol)>> {
        let sql = match kind_filter {
            Some(kind) => "SELECT s.name, s.kind, s.line_start, s.line_end, s.signature, s.doc_comment, f.path FROM symbols s JOIN files f ON s.file_id = f.id WHERE s.kind = ?",
            None => "SELECT s.name, s.kind, s.line_start, s.line_end, s.signature, s.doc_comment, f.path FROM symbols s JOIN files f ON s.file_id = f.id",
        };
        
        let mut stmt = self.db.prepare(sql)?;
        
        let rows = if let Some(kind) = kind_filter {
            stmt.query_map(params![kind.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
        } else {
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
        };
        
        let mut symbols = Vec::new();
        
        for row_result in rows {
            let (name, kind, line_start, line_end, signature, doc_comment, path) = row_result?;
            
            symbols.push((
                PathBuf::from(path),
                Symbol {
                    name,
                    kind: Self::parse_symbol_kind(&kind),
                    line_start,
                    line_end,
                    embedding: None,
                    signature,
                    doc_comment,
                },
            ));
        }
        
        Ok(symbols)
    }
    
    /// Get index statistics
    pub fn get_stats(&self) -> Result<IndexStats> {
        let total_files: i64 = self.db
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        
        let total_symbols: i64 = self.db
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
        
        let total_chunks: i64 = self.db
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;
        
        // Get language distribution
        let mut lang_stmt = self.db.prepare("SELECT language, COUNT(*) FROM files GROUP BY language")?;
        let lang_rows = lang_stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        
        let mut languages = BTreeMap::new();
        for row in lang_rows {
            let (lang, count) = row?;
            languages.insert(lang, count as usize);
        }
        
        // Get symbol kind distribution
        let mut kind_stmt = self.db.prepare("SELECT kind, COUNT(*) FROM symbols GROUP BY kind")?;
        let kind_rows = kind_stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        
        let mut symbol_kinds = BTreeMap::new();
        for row in kind_rows {
            let (kind, count) = row?;
            symbol_kinds.insert(kind, count as usize);
        }
        
        // Get last indexed time
        let last_indexed: Option<String> = self.db
            .query_row(
                "SELECT value FROM index_metadata WHERE key = 'last_indexed'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        
        let last_indexed = last_indexed
            .and_then(|s| s.parse().ok());
        
        // Get index size
        let db_path = self.config.db_path.clone()
            .unwrap_or_else(|| self.root.join(".code_index.db"));
        let index_size_bytes = std::fs::metadata(&db_path)
            .map(|m| m.len())
            .unwrap_or(0);
        
        Ok(IndexStats {
            total_files: total_files as usize,
            total_symbols: total_symbols as usize,
            total_chunks: total_chunks as usize,
            languages,
            symbol_kinds,
            last_indexed,
            index_size_bytes,
            embedding_model: format!("{:?}", self.config.embedding_model),
        })
    }
    
    /// Clear the index
    pub fn clear(&self) -> Result<()> {
        self.db.execute("DELETE FROM file_embeddings", [])?;
        self.db.execute("DELETE FROM symbol_embeddings", [])?;
        self.db.execute("DELETE FROM chunk_embeddings", [])?;
        self.db.execute("DELETE FROM symbols", [])?;
        self.db.execute("DELETE FROM chunks", [])?;
        self.db.execute("DELETE FROM files", [])?;
        self.db.execute("DELETE FROM index_metadata", [])?;
        
        *self.indexed.write().unwrap() = false;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_tf_idf_vectorizer() {
        let mut vectorizer = TfIdfVectorizer::new();
        
        let docs = vec![
            "function authenticate user",
            "function login user",
            "class User data",
        ];
        
        vectorizer.fit(&docs);
        
        let vec1 = vectorizer.transform("authenticate user");
        let vec2 = vectorizer.transform("login user");
        let vec3 = vectorizer.transform("class data");
        
        assert!(!vec1.is_empty());
        assert!(!vec2.is_empty());
        assert!(!vec3.is_empty());
        
        // Similar docs should have higher similarity
        let sim_12 = TfIdfVectorizer::cosine_similarity(&vec1, &vec2);
        let sim_13 = TfIdfVectorizer::cosine_similarity(&vec1, &vec3);
        
        assert!(sim_12 > sim_13, "Similar docs should have higher similarity");
    }
    
    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language(Path::new("main.rs")), "rust");
        assert_eq!(detect_language(Path::new("app.py")), "python");
        assert_eq!(detect_language(Path::new("index.js")), "javascript");
        assert_eq!(detect_language(Path::new("main.go")), "go");
    }
    
    #[test]
    fn test_code_parser_rust() {
        let parser = CodeParser::new();
        let code = r#"
fn main() {
    println!("Hello");
}

pub struct User {
    name: String,
}

pub enum Status {
    Active,
    Inactive,
}
"#;
        
        let symbols = parser.parse(code, "rust");
        
        assert_eq!(symbols.len(), 3);
        assert!(symbols.iter().any(|s| s.name == "main" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "User" && s.kind == SymbolKind::Struct));
        assert!(symbols.iter().any(|s| s.name == "Status" && s.kind == SymbolKind::Enum));
    }
    
    #[test]
    fn test_code_parser_python() {
        let parser = CodeParser::new();
        let code = r#"
def authenticate(user):
    pass

class UserService:
    def __init__(self):
        pass
"#;
        
        let symbols = parser.parse(code, "python");
        
        assert_eq!(symbols.len(), 3);
        assert!(symbols.iter().any(|s| s.name == "authenticate" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "UserService" && s.kind == SymbolKind::Class));
    }
    
    #[tokio::test]
    async fn test_code_index_basic() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create test file
        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, r#"
fn hello() {
    println!("Hello, world!");
}

fn authenticate(user: &str) -> bool {
    true
}
"#).unwrap();
        
        let config = IndexConfig {
            root: temp_dir.path().to_path_buf(),
            include_patterns: vec!["**/*.rs".into()],
            exclude_patterns: vec![],
            max_file_size_kb: 500,
            embedding_model: EmbeddingModel::TfIdf,
            chunk_size: 1000,
            db_path: None,
        };
        
        let index = CodeIndex::new(config).unwrap();
        let stats = index.build_index().await.unwrap();
        
        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.total_symbols, 2);
        
        // Test search
        let results = index.search("authentication function", 10).await.unwrap();
        assert!(!results.is_empty());
        
        // Test list symbols
        let symbols = index.list_symbols(None).unwrap();
        assert_eq!(symbols.len(), 2);
    }
}
