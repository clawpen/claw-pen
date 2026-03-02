# Code Index Module Design

## Overview

The `code_index` module provides semantic indexing for large codebases, enabling Claw Pen agents to efficiently search and retrieve relevant code context.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        CodeIndex                             │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐   │
│  │ IndexConfig  │  │CodeParser    │  │EmbeddingGenerator│   │
│  │              │  │              │  │                  │   │
│  │ - root       │  │ - parse()    │  │ - embed()        │   │
│  │ - patterns   │  │ - extract_*()│  │ - embed_ollama() │   │
│  │ - model      │  │              │  │ - embed_openai() │   │
│  └──────────────┘  └──────────────┘  └──────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│                      SQLite Storage                          │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────────┐   │
│  │ files    │ │ symbols  │ │ chunks   │ │ embeddings    │   │
│  └──────────┘ └──────────┘ └──────────┘ └───────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Core Types

### IndexConfig

Configuration for the code index:

```rust
pub struct IndexConfig {
    pub root: PathBuf,              // Root directory to index
    pub include_patterns: Vec<String>,  // Glob patterns to include
    pub exclude_patterns: Vec<String>,  // Glob patterns to exclude
    pub max_file_size_kb: u32,      // Skip files larger than this
    pub embedding_model: EmbeddingModel,  // TF-IDF, Ollama, or OpenAI
    pub chunk_size: usize,          // Split large files into chunks
    pub db_path: Option<PathBuf>,   // Custom database path
}
```

### CodeIndex

The main index structure:

```rust
pub struct CodeIndex {
    db: Connection,           // SQLite connection
    root: PathBuf,            // Root directory
    config: IndexConfig,      // Configuration
    embedding: EmbeddingGenerator,  // Embedding backend
    parser: CodeParser,       // Code parser
    indexed: RwLock<bool>,    // Index state
}
```

### Symbol

A code symbol (function, class, etc.):

```rust
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line_start: u32,
    pub line_end: u32,
    pub embedding: Option<Vec<f32>>,
    pub signature: Option<String>,
    pub doc_comment: Option<String>,
}
```

### SearchResult

A search result with similarity score:

```rust
pub struct SearchResult {
    pub file: PathBuf,
    pub score: f32,
    pub snippet: String,
    pub symbol: Option<Symbol>,
    pub chunk_index: Option<usize>,
    pub line_range: Option<(u32, u32)>,
}
```

## Embedding Models

### TF-IDF (Default Fallback)

- No external API required
- Works offline
- Good for keyword-based similarity
- Vocabulary built from indexed corpus

### Ollama (Local)

- Uses `nomic-embed-text` model
- Requires Ollama running at `localhost:11434`
- 768-dimensional embeddings
- Better semantic understanding

### OpenAI (Cloud)

- Uses `text-embedding-3-small`
- Requires API key
- 1536-dimensional embeddings
- Best semantic understanding

## Code Parser

The `CodeParser` extracts symbols from source code. Currently uses simple pattern matching, but is designed to be replaced with tree-sitter for full AST parsing.

Supported languages:
- Rust (fn, struct, enum, trait, const)
- Python (def, class)
- JavaScript/TypeScript (function, class, interface, const)
- Go (func, type struct, type interface)

## Indexing Flow

```
1. Walk directory tree
   ├── Filter by include/exclude patterns
   ├── Check file size limits
   └── Check mtime for incremental updates

2. For each file:
   ├── Detect language from extension
   ├── Parse to extract symbols
   ├── Split into chunks if large
   ├── Generate embeddings
   │   ├── File-level embedding
   │   ├── Symbol-level embeddings
   │   └── Chunk-level embeddings
   └── Store in SQLite

3. Store metadata:
   └── Last indexed timestamp
```

## Search Flow

```
1. Generate query embedding
   └── Using configured embedding model

2. Search multiple levels:
   ├── Search chunks (most granular)
   └── Search symbols

3. Combine and rank:
   ├── Calculate cosine similarity
   ├── Filter by threshold (> 0.1)
   ├── Sort by score
   └── Return top N results
```

## API Methods

### Core Methods

```rust
// Create new index
let index = CodeIndex::new(config)?;

// Build/update index
let stats = index.build_index().await?;

// Semantic search
let results = index.search("authentication logic", 10).await?;

// Get file content
let content = index.get_file(&path)?;

// Get context for a query (within token budget)
let context = index.get_context("error handling", 2000).await?;

// List symbols (optionally filtered)
let symbols = index.list_symbols(Some(SymbolKind::Function))?;

// Get statistics
let stats = index.get_stats()?;

// Clear index
index.clear()?;
```

## Database Schema

```sql
-- Files table
CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    mtime TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    language TEXT NOT NULL,
    content_hash TEXT,
    indexed_at TEXT NOT NULL
);

-- Symbols table
CREATE TABLE symbols (
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
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL,
    chunk_index INTEGER NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    content TEXT NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- Embeddings stored as BLOBs (sqlite-vss compatible structure)
CREATE TABLE file_embeddings (file_id INTEGER PRIMARY KEY, embedding BLOB);
CREATE TABLE symbol_embeddings (symbol_id INTEGER PRIMARY KEY, embedding BLOB);
CREATE TABLE chunk_embeddings (chunk_id INTEGER PRIMARY KEY, embedding BLOB);
```

## Future Enhancements

### Tree-Sitter Integration

Replace the simple pattern-based parser with tree-sitter for:
- Full AST parsing
- More accurate symbol extraction
- Better handling of nested structures
- Support for more languages

### SQLite-VSS Integration

Add proper vector similarity search:
- Load sqlite-vss extension
- Use vector index for fast ANN search
- Support for larger codebases

### Incremental Indexing Improvements

- Watch for file changes (notify)
- Background re-indexing
- Content hashing for better change detection

### Advanced Features

- Cross-file symbol references
- Import/dependency tracking
- Code similarity detection
- Duplicate code detection

## Usage Example

```rust
use code_index::{CodeIndex, IndexConfig, EmbeddingModel};

#[tokio::main]
async fn main() -> Result<()> {
    // Configure the index
    let config = IndexConfig {
        root: PathBuf::from("/path/to/project"),
        include_patterns: vec!["**/*.rs".into(), "**/*.py".into()],
        exclude_patterns: vec!["**/target/**".into(), "**/node_modules/**".into()],
        max_file_size_kb: 500,
        embedding_model: EmbeddingModel::TfIdf,
        chunk_size: 1000,
        db_path: None,
    };

    // Create and build index
    let index = CodeIndex::new(config)?;
    let stats = index.build_index().await?;
    println!("Indexed {} files, {} symbols", stats.total_files, stats.total_symbols);

    // Search for code
    let results = index.search("authentication logic", 10).await?;
    for result in results {
        println!("{:?} (score: {:.2})", result.file, result.score);
        if let Some(symbol) = result.symbol {
            println!("  Symbol: {} ({:?})", symbol.name, symbol.kind);
        }
    }

    // Get context for an agent
    let context = index.get_context("how does error handling work?", 4000).await?;
    println!("Context:\n{}", context);

    Ok(())
}
```

## Performance Considerations

1. **TF-IDF is fast** - No network calls, suitable for large codebases
2. **Incremental updates** - Only re-index changed files (mtime tracking)
3. **Chunking** - Large files split into manageable pieces
4. **SQLite** - Efficient storage and querying
5. **Memory efficient** - Embeddings stored as bytes, not in memory

## Testing

Run tests with:

```bash
cargo test code_index
```

Tests cover:
- TF-IDF vectorization
- Language detection
- Code parsing (Rust, Python, JS/TS, Go)
- Full indexing workflow
- Search functionality
