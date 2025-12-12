# SQLite-Based Storage System - Design Document
## Graph Cell Editor - Storage Architecture v0.2.0

---

## 1. Overview & Goals

### Purpose
Replace the current `cells.json` + `events.jsonl` file format with a SQLite-based storage system that provides:
- Efficient lazy loading for large projects (1000+ cells)
- Robust undo/redo via snapshots
- Support for external file editing (VS Code integration)
- Flexible attachment storage (local, remote, symlinked)
- Future-proof for collaboration and versioning

### Key Requirements
1. **Performance**: Load and interact with projects like Obsidian/Notion (fast, responsive)
2. **Reliability**: Atomic saves, corruption recovery, no data loss
3. **Flexibility**: Support inline content, external files, remote URLs, symlinks
4. **Developer Experience**: Enable external editing of Python/code cells
5. **User Experience**: Single-file projects, easy backup/restore, shareable exports

### Non-Goals (Deferred to Future)
- Real-time collaborative editing
- Branching/multiple versions
- Advanced merge conflict resolution (basic manual resolution only)

---

## 2. Architecture Overview

### File Structure

```
myproject.gcdb                    # SQLite database (main file)
myproject-files/                  # External content directory (adjacent to .gcdb)
  ├── cells/                      # Cell content stored externally
  │   ├── 01HGXY123.py           # Python cell (always external)
  │   ├── 01HGXY456.txt          # Large text cell (> 1MB)
  │   └── 01HGXY789.json         # JSON cell (user chose external)
  ├── attachments/                # User-added attachments
  │   ├── dataset.csv
  │   └── image.png
  └── cache/                      # Downloaded remote content
      └── remote_file_abc123.tmp
```

### Storage Strategy Diagram

```
Cell Content Decision Tree:
                    
Cell Type → Storage Location
    ├─ Python          → Always External (cells/*.py)
    ├─ Large (>1MB)    → Always External (cells/*.{ext})
    ├─ User Choice     → External if user requests
    └─ Default         → Inline in SQLite (if < 1MB)

External File Types:
    ├─ Local (cells/)       → Relative path stored
    ├─ Attachment (attach/) → Relative path stored  
    ├─ Symlink             → Absolute path stored
    └─ Remote (URL)        → URL stored, cached locally
```

---

## 3. Database Schema

### Core Tables

```sql
-- Project metadata
CREATE TABLE project_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Initial values:
INSERT INTO project_meta VALUES 
    ('version', '0.2.0'),
    ('created_at', datetime('now')),
    ('modified_at', datetime('now')),
    ('external_files_path', 'myproject-files'),
    ('snapshot_retention', '50'),
    ('app_version', '0.1.0');

-- Cells table (structure and metadata)
CREATE TABLE cells (
    -- Identity
    id TEXT PRIMARY KEY,                    -- ULID as string
    short_id TEXT NOT NULL UNIQUE,          -- "A7", "2K", etc.
    name TEXT,                               -- Optional user-assigned name
    
    -- Type
    cell_type TEXT NOT NULL,                 -- "Text", "Python", "Markdown", etc.
    
    -- Position and size (canvas coordinates)
    x REAL NOT NULL,
    y REAL NOT NULL,
    width REAL NOT NULL,
    height REAL NOT NULL,
    
    -- Content storage strategy
    content_location TEXT NOT NULL,          -- "inline", "external", "remote", "symlink"
    content_text TEXT,                       -- If inline: actual content. Else: NULL
    content_path TEXT,                       -- If external/remote/symlink: path or URL
    content_summary TEXT,                    -- User description for external/remote files
    content_hash TEXT,                       -- SHA256 hash for change detection
    
    -- Hierarchy (split/merge tracking)
    parent_id TEXT,                          -- ULID of parent cell if created by split
    split_direction TEXT,                    -- "Horizontal", "Vertical", or NULL
    
    -- Execution
    is_start_point INTEGER DEFAULT 0,        -- Boolean (SQLite doesn't have BOOLEAN)
    
    -- UI state
    preview_mode TEXT,                       -- "Rendered", "Raw", "Hybrid", NULL=default
    
    -- Timestamps
    created_at TEXT NOT NULL,                -- ISO 8601 timestamp
    modified_at TEXT NOT NULL,               -- ISO 8601 timestamp
    
    -- Constraints
    FOREIGN KEY (parent_id) REFERENCES cells(id) ON DELETE SET NULL,
    CHECK (content_location IN ('inline', 'external', 'remote', 'symlink')),
    CHECK (is_start_point IN (0, 1))
);

-- Indexes for performance
CREATE INDEX idx_cells_parent ON cells(parent_id);
CREATE INDEX idx_cells_modified ON cells(modified_at DESC);
CREATE INDEX idx_cells_short_id ON cells(short_id);
CREATE INDEX idx_cells_type ON cells(cell_type);
CREATE INDEX idx_cells_location ON cells(content_location);

-- Relationships (data flow graph)
CREATE TABLE relationships (
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    created_at TEXT NOT NULL,                -- ISO 8601 timestamp
    
    PRIMARY KEY (from_id, to_id),
    FOREIGN KEY (from_id) REFERENCES cells(id) ON DELETE CASCADE,
    FOREIGN KEY (to_id) REFERENCES cells(id) ON DELETE CASCADE
);

CREATE INDEX idx_relationships_from ON relationships(from_id);
CREATE INDEX idx_relationships_to ON relationships(to_id);

-- Snapshots for undo/redo (structural history)
CREATE TABLE snapshots (
    id TEXT PRIMARY KEY,                     -- ULID
    timestamp TEXT NOT NULL,                 -- ISO 8601
    description TEXT NOT NULL,               -- Human-readable: "Split cell A7 horizontally"
    operation_type TEXT NOT NULL,            -- "split", "merge", "create", "delete", "modify", "relationship"
    
    -- Snapshot data (CBOR or JSON blob)
    snapshot_data BLOB NOT NULL,             -- Contains before/after state for affected cells
    
    -- Sequence for ordering
    sequence INTEGER NOT NULL                -- Auto-incrementing sequence number
);

CREATE INDEX idx_snapshots_timestamp ON snapshots(timestamp DESC);
CREATE INDEX idx_snapshots_sequence ON snapshots(sequence DESC);

-- Execution traces (optional persistence for debugging)
CREATE TABLE execution_traces (
    id TEXT PRIMARY KEY,                     -- ULID
    timestamp TEXT NOT NULL,                 -- ISO 8601
    mode TEXT NOT NULL,                      -- "Run", "Step", "DryRun"
    start_cell_id TEXT NOT NULL,
    
    -- Execution log (CBOR or JSON blob)
    trace_data BLOB NOT NULL,                -- Array of execution steps
    
    -- Status
    status TEXT NOT NULL,                    -- "Complete", "Error", "Paused"
    error_message TEXT,                      -- If status = "Error"
    
    FOREIGN KEY (start_cell_id) REFERENCES cells(id) ON DELETE CASCADE,
    CHECK (mode IN ('Run', 'Step', 'DryRun')),
    CHECK (status IN ('Complete', 'Error', 'Paused', 'DryRunComplete'))
);

CREATE INDEX idx_execution_traces_timestamp ON execution_traces(timestamp DESC);

-- Version tracking for conflict detection
CREATE TABLE version_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,                 -- ISO 8601
    app_version TEXT NOT NULL,               -- Version of app that made the change
    operation TEXT NOT NULL,                 -- Description of operation
    conflict_marker TEXT                     -- Set if conflict detected
);

CREATE INDEX idx_version_log_timestamp ON version_log(timestamp DESC);
```

---

## 4. Content Storage Strategy

### Cell Type Mappings

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentStorageRule {
    AlwaysExternal,      // Always stored in external file
    AlwaysInline,        // Always stored in SQLite
    UserChoice,          // User can choose, default inline
    SizeDependent,       // Based on 1MB threshold
}

const CONTENT_STORAGE_RULES: &[(CellType, ContentStorageRule)] = &[
    // Code types - always external for IDE editing
    (CellType::Python, ContentStorageRule::AlwaysExternal),
    
    // Future code types
    // (CellType::Rust, ContentStorageRule::AlwaysExternal),
    // (CellType::JavaScript, ContentStorageRule::AlwaysExternal),
    
    // Text types - size dependent
    (CellType::Text, ContentStorageRule::SizeDependent),
    (CellType::Markdown, ContentStorageRule::SizeDependent),
    
    // Structured data - user choice
    (CellType::Json, ContentStorageRule::UserChoice),
    (CellType::Csv, ContentStorageRule::UserChoice),
    
    // Binary types - always external
    // (CellType::Image, ContentStorageRule::AlwaysExternal),
];

const INLINE_SIZE_THRESHOLD: usize = 1_048_576; // 1 MB
```

### Storage Location Decision Algorithm

```rust
fn determine_storage_location(
    cell_type: CellType,
    content_size: usize,
    user_preference: Option<StoragePreference>,
) -> ContentLocation {
    let rule = get_storage_rule(cell_type);
    
    match rule {
        ContentStorageRule::AlwaysExternal => {
            ContentLocation::External
        }
        
        ContentStorageRule::AlwaysInline => {
            ContentLocation::Inline
        }
        
        ContentStorageRule::UserChoice => {
            match user_preference {
                Some(StoragePreference::External) => ContentLocation::External,
                Some(StoragePreference::Inline) | None => ContentLocation::Inline,
            }
        }
        
        ContentStorageRule::SizeDependent => {
            if content_size > INLINE_SIZE_THRESHOLD {
                ContentLocation::External
            } else {
                match user_preference {
                    Some(StoragePreference::External) => ContentLocation::External,
                    Some(StoragePreference::Inline) | None => ContentLocation::Inline,
                }
            }
        }
    }
}
```

### File Path Resolution

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
enum ContentLocation {
    Inline,              // Stored in cells.content_text
    External,            // Stored in {project}-files/cells/
    Remote,              // URL, cached in {project}-files/cache/
    Symlink,             // Absolute path outside project
}

fn resolve_content_path(
    cell_id: Ulid,
    location: ContentLocation,
    stored_path: Option<&str>,
    project_dir: &Path,
) -> Result<PathBuf> {
    match location {
        ContentLocation::Inline => {
            Err(anyhow!("Inline content has no file path"))
        }
        
        ContentLocation::External => {
            // Relative to project-files/cells/
            let external_dir = project_dir.join(format!("{}-files/cells", project_name));
            let path = stored_path.ok_or(anyhow!("Missing external path"))?;
            Ok(external_dir.join(path))
        }
        
        ContentLocation::Remote => {
            // Cached locally
            let cache_dir = project_dir.join(format!("{}-files/cache", project_name));
            let path = stored_path.ok_or(anyhow!("Missing remote path"))?;
            // Hash URL to create cache filename
            let cache_file = format!("{}.cache", hash_url(path));
            Ok(cache_dir.join(cache_file))
        }
        
        ContentLocation::Symlink => {
            // Absolute path
            let path = stored_path.ok_or(anyhow!("Missing symlink path"))?;
            Ok(PathBuf::from(path))
        }
    }
}

fn generate_external_filename(cell_id: Ulid, cell_type: CellType) -> String {
    let extension = match cell_type {
        CellType::Python => "py",
        CellType::Text => "txt",
        CellType::Markdown => "md",
        CellType::Json => "json",
        CellType::Csv => "csv",
        // Future types...
    };
    
    format!("{}.{}", cell_id, extension)
}
```

---

## 5. Lazy Loading Architecture

### Data Model

```rust
// In-memory representation with lazy loading
pub struct CellHandle {
    // Metadata (always loaded from SQLite)
    pub id: Ulid,
    pub short_id: String,
    pub name: Option<String>,
    pub cell_type: CellType,
    pub bounds: Rectangle,
    pub content_location: ContentLocation,
    pub is_start_point: bool,
    pub parent: Option<Ulid>,
    pub children: Vec<Ulid>,
    pub preview_mode: Option<MarkdownPreviewMode>,
    
    // Content (loaded on demand)
    content: OnceCell<CellContent>,
    
    // Reference to database for lazy loading
    db: Arc<Database>,
}

impl CellHandle {
    /// Get cell content, loading from storage if necessary
    pub fn content(&self) -> Result<&CellContent> {
        self.content.get_or_try_init(|| {
            self.db.load_cell_content(self.id, self.content_location)
        })
    }
    
    /// Check if content is loaded without triggering load
    pub fn is_content_loaded(&self) -> bool {
        self.content.get().is_some()
    }
    
    /// Unload content to free memory
    pub fn unload_content(&mut self) {
        self.content = OnceCell::new();
    }
}
```

### Loading Strategy

```rust
pub struct Canvas {
    cells: HashMap<Ulid, CellHandle>,
    relationships: HashMap<(Ulid, Ulid), Relationship>,
    db: Arc<Database>,
    
    // Cache of loaded content
    content_cache: LruCache<Ulid, CellContent>,
}

impl Canvas {
    /// Load project metadata and cell structure (fast)
    pub fn open(db_path: &Path) -> Result<Self> {
        let db = Arc::new(Database::open(db_path)?);
        
        // Load all cell metadata (lightweight)
        let cells = db.load_all_cell_metadata()?;
        let relationships = db.load_all_relationships()?;
        
        Ok(Self {
            cells,
            relationships,
            db,
            content_cache: LruCache::new(100), // Cache last 100 loaded contents
        })
    }
    
    /// Get cell (metadata only, no content loaded)
    pub fn get_cell(&self, id: Ulid) -> Option<&CellHandle> {
        self.cells.get(&id)
    }
    
    /// Get cell with content loaded
    pub fn get_cell_with_content(&mut self, id: Ulid) -> Result<(&CellHandle, &CellContent)> {
        let cell = self.cells.get(&id)
            .ok_or_else(|| anyhow!("Cell not found: {}", id))?;
        
        let content = cell.content()?;
        Ok((cell, content))
    }
    
    /// Render visible cells (progressive loading)
    pub fn render_visible_cells(&mut self, viewport: Rectangle) -> Result<Vec<Ulid>> {
        let visible_cells: Vec<Ulid> = self.cells.iter()
            .filter(|(_, cell)| cell.bounds.intersects(&viewport))
            .map(|(id, _)| *id)
            .collect();
        
        // Load content for visible cells
        for cell_id in &visible_cells {
            if let Some(cell) = self.cells.get(cell_id) {
                // Trigger lazy load
                let _ = cell.content();
            }
        }
        
        Ok(visible_cells)
    }
}
```

### Database Implementation

```rust
impl Database {
    /// Load only cell metadata (no content)
    fn load_all_cell_metadata(&self) -> Result<HashMap<Ulid, CellHandle>> {
        let mut cells = HashMap::new();
        
        let mut stmt = self.conn.prepare("
            SELECT 
                id, short_id, name, cell_type,
                x, y, width, height,
                content_location, content_path,
                is_start_point, parent_id,
                preview_mode,
                created_at, modified_at
            FROM cells
        ")?;
        
        let cell_iter = stmt.query_map([], |row| {
            let id = Ulid::from_string(row.get(0)?)?;
            let cell = CellHandle {
                id,
                short_id: row.get(1)?,
                name: row.get(2)?,
                cell_type: CellType::from_str(row.get(3)?)?,
                bounds: Rectangle::new(
                    row.get(4)?, row.get(5)?,
                    row.get(6)?, row.get(7)?
                ),
                content_location: ContentLocation::from_str(row.get(8)?)?,
                content_path: row.get(9)?,
                is_start_point: row.get::<_, i32>(10)? != 0,
                parent: row.get::<_, Option<String>>(11)?
                    .map(|s| Ulid::from_string(&s))
                    .transpose()?,
                preview_mode: row.get::<_, Option<String>>(12)?
                    .map(|s| MarkdownPreviewMode::from_str(&s))
                    .transpose()?,
                content: OnceCell::new(),
                children: Vec::new(), // Will be populated in second pass
                db: Arc::clone(&self),
            };
            Ok((id, cell))
        })?;
        
        for cell_result in cell_iter {
            let (id, cell) = cell_result?;
            cells.insert(id, cell);
        }
        
        // Second pass: populate children lists
        for cell in cells.values_mut() {
            if let Some(parent_id) = cell.parent {
                if let Some(parent) = cells.get_mut(&parent_id) {
                    parent.children.push(cell.id);
                }
            }
        }
        
        Ok(cells)
    }
    
    /// Load content for a specific cell (called lazily)
    fn load_cell_content(
        &self,
        cell_id: Ulid,
        location: ContentLocation,
    ) -> Result<CellContent> {
        match location {
            ContentLocation::Inline => {
                let content_text: String = self.conn.query_row(
                    "SELECT content_text FROM cells WHERE id = ?1",
                    [cell_id.to_string()],
                    |row| row.get(0)
                )?;
                Ok(CellContent::Inline(content_text))
            }
            
            ContentLocation::External => {
                let path: String = self.conn.query_row(
                    "SELECT content_path FROM cells WHERE id = ?1",
                    [cell_id.to_string()],
                    |row| row.get(0)
                )?;
                
                let full_path = self.resolve_external_path(&path)?;
                let content = fs::read_to_string(&full_path)?;
                
                Ok(CellContent::Inline(content))
            }
            
            ContentLocation::Remote => {
                let url: String = self.conn.query_row(
                    "SELECT content_path FROM cells WHERE id = ?1",
                    [cell_id.to_string()],
                    |row| row.get(0)
                )?;
                
                // Check cache first
                let cache_path = self.get_cache_path(&url)?;
                if cache_path.exists() {
                    let content = fs::read_to_string(&cache_path)?;
                    Ok(CellContent::Inline(content))
                } else {
                    // Download and cache
                    let content = self.download_remote_content(&url)?;
                    fs::write(&cache_path, &content)?;
                    Ok(CellContent::Inline(content))
                }
            }
            
            ContentLocation::Symlink => {
                let path: String = self.conn.query_row(
                    "SELECT content_path FROM cells WHERE id = ?1",
                    [cell_id.to_string()],
                    |row| row.get(0)
                )?;
                
                let content = fs::read_to_string(&path)?;
                Ok(CellContent::Inline(content))
            }
        }
    }
}
```

---

## 6. Snapshot System (Undo/Redo)

### Snapshot Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: Ulid,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub operation_type: OperationType,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    CellCreated,
    CellDeleted,
    CellModified,
    CellSplit,
    CellMerged,
    RelationshipCreated,
    RelationshipDeleted,
    BatchOperation, // Multiple operations in one transaction
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Change {
    CellCreated {
        cell: SerializableCell,
    },
    CellDeleted {
        cell: SerializableCell, // Full cell for undo
    },
    CellModified {
        id: Ulid,
        before: CellDiff,
        after: CellDiff,
    },
    CellSplit {
        parent_id: Ulid,
        children: Vec<Ulid>,
        parent_before: SerializableCell,
    },
    CellMerged {
        merged_ids: Vec<Ulid>,
        merged_cells: Vec<SerializableCell>, // For undo
        new_cell: SerializableCell,
    },
    RelationshipCreated {
        from: Ulid,
        to: Ulid,
    },
    RelationshipDeleted {
        from: Ulid,
        to: Ulid,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellDiff {
    pub name: Option<Option<String>>,
    pub content: Option<String>,
    pub bounds: Option<Rectangle>,
    pub cell_type: Option<CellType>,
    // Only include changed fields
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableCell {
    // Full cell representation for snapshots
    pub id: Ulid,
    pub short_id: String,
    pub name: Option<String>,
    pub cell_type: CellType,
    pub bounds: Rectangle,
    pub content: String, // Always inline in snapshots
    pub is_start_point: bool,
    pub parent: Option<Ulid>,
    pub children: Vec<Ulid>,
    pub preview_mode: Option<MarkdownPreviewMode>,
}
```

### Snapshot Creation

```rust
impl Database {
    /// Create a snapshot before an operation
    pub fn create_snapshot(
        &self,
        operation: OperationType,
        description: String,
        changes: Vec<Change>,
    ) -> Result<Ulid> {
        let snapshot = Snapshot {
            id: Ulid::new(),
            timestamp: Utc::now(),
            description,
            operation_type: operation,
            changes,
        };
        
        // Serialize to CBOR for compact storage
        let snapshot_data = serde_cbor::to_vec(&snapshot)?;
        
        let tx = self.conn.transaction()?;
        
        // Get next sequence number
        let sequence: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM snapshots",
            [],
            |row| row.get(0)
        )?;
        
        tx.execute(
            "INSERT INTO snapshots (id, timestamp, description, operation_type, snapshot_data, sequence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                snapshot.id.to_string(),
                snapshot.timestamp.to_rfc3339(),
                &snapshot.description,
                snapshot.operation_type.to_string(),
                &snapshot_data,
                sequence
            ]
        )?;
        
        // Prune old snapshots if needed
        self.prune_snapshots(&tx)?;
        
        tx.commit()?;
        
        Ok(snapshot.id)
    }
    
    /// Prune old snapshots beyond retention limit
    fn prune_snapshots(&self, tx: &Transaction) -> Result<()> {
        let retention: i64 = tx.query_row(
            "SELECT value FROM project_meta WHERE key = 'snapshot_retention'",
            [],
            |row| row.get::<_, String>(0)?.parse::<i64>().map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        )?;
        
        tx.execute(
            "DELETE FROM snapshots 
             WHERE sequence <= (SELECT MAX(sequence) FROM snapshots) - ?1",
            [retention]
        )?;
        
        Ok(())
    }
}
```

### Undo/Redo Implementation

```rust
pub struct UndoManager {
    db: Arc<Database>,
    current_sequence: i64,
}

impl UndoManager {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let current_sequence = db.get_latest_sequence()?;
        Ok(Self {
            db,
            current_sequence,
        })
    }
    
    pub fn undo(&mut self) -> Result<Option<Snapshot>> {
        if self.current_sequence <= 0 {
            return Ok(None);
        }
        
        // Load snapshot at current sequence
        let snapshot = self.db.load_snapshot_by_sequence(self.current_sequence)?;
        
        // Apply inverse changes
        self.apply_inverse_changes(&snapshot)?;
        
        self.current_sequence -= 1;
        
        Ok(Some(snapshot))
    }
    
    pub fn redo(&mut self) -> Result<Option<Snapshot>> {
        // Check if there's a next snapshot
        let next_sequence = self.current_sequence + 1;
        if let Ok(snapshot) = self.db.load_snapshot_by_sequence(next_sequence) {
            // Re-apply changes
            self.apply_changes(&snapshot)?;
            
            self.current_sequence = next_sequence;
            
            Ok(Some(snapshot))
        } else {
            Ok(None)
        }
    }
    
    fn apply_inverse_changes(&self, snapshot: &Snapshot) -> Result<()> {
        let tx = self.db.conn.transaction()?;
        
        for change in snapshot.changes.iter().rev() {
            match change {
                Change::CellCreated { cell } => {
                    // Undo: delete the cell
                    tx.execute("DELETE FROM cells WHERE id = ?1", [cell.id.to_string()])?;
                }
                
                Change::CellDeleted { cell } => {
                    // Undo: recreate the cell
                    self.insert_cell(&tx, cell)?;
                }
                
                Change::CellModified { id, before, after: _ } => {
                    // Undo: restore before state
                    self.apply_cell_diff(&tx, *id, before)?;
                }
                
                // Handle other change types...
                _ => {}
            }
        }
        
        tx.commit()?;
        Ok(())
    }
    
    fn apply_changes(&self, snapshot: &Snapshot) -> Result<()> {
        let tx = self.db.conn.transaction()?;
        
        for change in &snapshot.changes {
            match change {
                Change::CellCreated { cell } => {
                    self.insert_cell(&tx, cell)?;
                }
                
                Change::CellDeleted { cell } => {
                    tx.execute("DELETE FROM cells WHERE id = ?1", [cell.id.to_string()])?;
                }
                
                Change::CellModified { id, before: _, after } => {
                    self.apply_cell_diff(&tx, *id, after)?;
                }
                
                // Handle other change types...
                _ => {}
            }
        }
        
        tx.commit()?;
        Ok(())
    }
}
```

---

## 7. Save/Load Mechanics

### Transaction Strategy: Aggressive (Option A)

Every operation commits immediately to ensure no data loss.

```rust
impl Canvas {
    /// Split a cell (with immediate save)
    pub fn split_cell(
        &mut self,
        cell_id: Ulid,
        direction: SplitDirection,
        split_ratio: f32,
    ) -> Result<(Ulid, Ulid)> {
        // 1. Create snapshot BEFORE making changes
        let parent_cell = self.get_cell(cell_id)
            .ok_or_else(|| anyhow!("Cell not found"))?
            .clone();
        
        let snapshot_changes = vec![
            Change::CellSplit {
                parent_id: cell_id,
                children: vec![], // Will be filled after split
                parent_before: serialize_cell(&parent_cell)?,
            }
        ];
        
        let snapshot_id = self.db.create_snapshot(
            OperationType::CellSplit,
            format!("Split cell {} {:?}", parent_cell.short_id, direction),
            snapshot_changes,
        )?;
        
        // 2. Perform split operation
        let (child1_id, child2_id) = self.split_cell_internal(cell_id, direction, split_ratio)?;
        
        // 3. Save to database (atomic transaction)
        let tx = self.db.conn.transaction()?;
        
        // Update parent cell
        let parent = self.get_cell(cell_id).unwrap();
        self.db.update_cell(&tx, parent)?;
        
        // Insert child cells
        let child1 = self.get_cell(child1_id).unwrap();
        let child2 = self.get_cell(child2_id).unwrap();
        self.db.insert_cell(&tx, child1)?;
        self.db.insert_cell(&tx, child2)?;
        
        // Update snapshot with actual child IDs
        tx.execute(
            "UPDATE snapshots 
             SET snapshot_data = ?1 
             WHERE id = ?2",
            params![
                serde_cbor::to_vec(&snapshot_changes)?,
                snapshot_id.to_string()
            ]
        )?;
        
        tx.commit()?;
        
        Ok((child1_id, child2_id))
    }
    
    /// Update cell content (with immediate save)
    pub fn update_cell_content(
        &mut self,
        cell_id: Ulid,
        new_content: String,
    ) -> Result<()> {
        let cell = self.cells.get_mut(&cell_id)
            .ok_or_else(|| anyhow!("Cell not found"))?;
        
        // Get old content for snapshot
        let old_content = cell.content()?.as_str()
            .ok_or_else(|| anyhow!("Cannot get cell content"))?
            .to_string();
        
        // Create snapshot
        let snapshot_changes = vec![
            Change::CellModified {
                id: cell_id,
                before: CellDiff {
                    content: Some(old_content),
                    ..Default::default()
                },
                after: CellDiff {
                    content: Some(new_content.clone()),
                    ..Default::default()
                },
            }
        ];
        
        self.db.create_snapshot(
            OperationType::CellModified,
            format!("Modified cell {}", cell.short_id),
            snapshot_changes,
        )?;
        
        // Update content
        cell.set_content(CellContent::Inline(new_content.clone()));
        
        // Save to database
        let tx = self.db.conn.transaction()?;
        
        match cell.content_location {
            ContentLocation::Inline => {
                tx.execute(
                    "UPDATE cells 
                     SET content_text = ?1, 
                         content_hash = ?2,
                         modified_at = ?3
                     WHERE id = ?4",
                    params![
                        new_content,
                        hash_content(&new_content),
                        Utc::now().to_rfc3339(),
                        cell_id.to_string()
                    ]
                )?;
            }
            
            ContentLocation::External => {
                // Write to external file
                let path = self.db.resolve_external_path(
                    &cell.content_path.as_ref().unwrap()
                )?;
                fs::write(&path, &new_content)?;
                
                // Update hash in database
                tx.execute(
                    "UPDATE cells 
                     SET content_hash = ?1,
                         modified_at = ?2
                     WHERE id = ?3",
                    params![
                        hash_content(&new_content),
                        Utc::now().to_rfc3339(),
                        cell_id.to_string()
                    ]
                )?;
            }
            
            // Handle other locations...
            _ => {}
        }
        
        tx.commit()?;
        
        Ok(())
    }
}
```

### Auto-Save Triggers

```rust
pub struct AutoSaveManager {
    canvas: Arc<Mutex<Canvas>>,
    last_save: Instant,
    save_interval: Duration,
    idle_threshold: Duration,
    has_unsaved_changes: Arc<AtomicBool>,
}

impl AutoSaveManager {
    pub fn new(canvas: Arc<Mutex<Canvas>>) -> Self {
        Self {
            canvas,
            last_save: Instant::now(),
            save_interval: Duration::from_secs(60), // Save every 60 seconds
            idle_threshold: Duration::from_secs(5),  // Save after 5 seconds idle
            has_unsaved_changes: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Called from UI event loop
    pub fn on_interval_tick(&mut self) {
        if self.last_save.elapsed() >= self.save_interval {
            if self.has_unsaved_changes.load(Ordering::Relaxed) {
                self.save();
            }
        }
    }
    
    /// Called when user stops typing/interacting
    pub fn on_idle(&mut self) {
        if self.has_unsaved_changes.load(Ordering::Relaxed) {
            self.save();
        }
    }
    
    /// Called when window loses focus
    pub fn on_focus_lost(&mut self) {
        if self.has_unsaved_changes.load(Ordering::Relaxed) {
            self.save();
        }
    }
    
    fn save(&mut self) {
        // With aggressive saving, individual operations already saved
        // This just updates project metadata
        let canvas = self.canvas.lock().unwrap();
        let _ = canvas.db.update_metadata("modified_at", &Utc::now().to_rfc3339());
        
        self.has_unsaved_changes.store(false, Ordering::Relaxed);
        self.last_save = Instant::now();
    }
}
```

---

## 8. External Editing Workflow

### Opening External Editor

```rust
pub struct ExternalEditorManager {
    db: Arc<Database>,
    active_edits: HashMap<Ulid, ExternalEdit>,
}

struct ExternalEdit {
    cell_id: Ulid,
    temp_path: PathBuf,
    original_hash: String,
    editor_process: Option<Child>,
}

impl ExternalEditorManager {
    /// Open cell in external editor (e.g., VS Code)
    pub fn open_in_editor(
        &mut self,
        cell_id: Ulid,
        editor_cmd: &str, // "code", "vim", etc.
    ) -> Result<()> {
        let cell = self.db.load_cell_metadata(cell_id)?;
        
        let edit_path = match cell.content_location {
            ContentLocation::Inline => {
                // Create temp file
                let content = self.db.load_cell_content(cell_id, cell.content_location)?;
                let temp_path = self.create_temp_file(cell_id, &cell.cell_type, &content)?;
                temp_path
            }
            
            ContentLocation::External => {
                // Use existing file directly
                self.db.resolve_external_path(&cell.content_path.unwrap())?
            }
            
            ContentLocation::Symlink => {
                // Use symlinked file directly
                PathBuf::from(&cell.content_path.unwrap())
            }
            
            ContentLocation::Remote => {
                // Download to temp file
                let content = self.db.load_cell_content(cell_id, cell.content_location)?;
                let temp_path = self.create_temp_file(cell_id, &cell.cell_type, &content)?;
                temp_path
            }
        };
        
        // Calculate hash before opening
        let original_hash = self.hash_file(&edit_path)?;
        
        // Launch editor (non-blocking)
        let editor_process = Command::new(editor_cmd)
            .arg(&edit_path)
            .arg("--wait") // VS Code flag to wait for window close
            .spawn()?;
        
        // Track this edit
        self.active_edits.insert(cell_id, ExternalEdit {
            cell_id,
            temp_path: edit_path.clone(),
            original_hash,
            editor_process: Some(editor_process),
        });
        
        // Start watching for process exit
        self.watch_editor_process(cell_id);
        
        Ok(())
    }
    
    /// Watch editor process and sync when it closes
    fn watch_editor_process(&mut self, cell_id: Ulid) {
        let db = Arc::clone(&self.db);
        let active_edits = Arc::clone(&self.active_edits_arc);
        
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(1));
                
                let mut edits = active_edits.lock().unwrap();
                if let Some(edit) = edits.get_mut(&cell_id) {
                    // Check if process exited
                    if let Some(ref mut process) = edit.editor_process {
                        match process.try_wait() {
                            Ok(Some(_status)) => {
                                // Process exited, sync changes
                                let _ = Self::sync_from_external_file(
                                    &db,
                                    cell_id,
                                    &edit.temp_path,
                                    &edit.original_hash,
                                );
                                
                                // Remove from active edits
                                edits.remove(&cell_id);
                                break;
                            }
                            Ok(None) => {
                                // Still running, continue watching
                            }
                            Err(_) => {
                                // Error checking process, assume closed
                                edits.remove(&cell_id);
                                break;
                            }
                        }
                    }
                } else {
                    break;
                }
            }
        });
    }
    
    /// Sync changes from external file back to database
    fn sync_from_external_file(
        db: &Database,
        cell_id: Ulid,
        file_path: &Path,
        original_hash: &str,
    ) -> Result<()> {
        // Calculate new hash
        let new_hash = hash_file(file_path)?;
        
        if new_hash == original_hash {
            // No changes, nothing to do
            return Ok(());
        }
        
        // Read updated content
        let new_content = fs::read_to_string(file_path)?;
        
        // Check for conflicts (did cell change in app while editing?)
        let current_hash = db.get_cell_content_hash(cell_id)?;
        if current_hash != original_hash {
            // CONFLICT: Cell was modified in both app and external editor
            return Self::handle_edit_conflict(db, cell_id, &new_content, file_path);
        }
        
        // No conflict, update cell content
        db.update_cell_content(cell_id, new_content)?;
        
        Ok(())
    }
    
    /// Handle conflict when cell modified in both places
    fn handle_edit_conflict(
        db: &Database,
        cell_id: Ulid,
        external_content: &str,
        external_path: &Path,
    ) -> Result<()> {
        // Create conflict file for manual resolution
        let conflict_path = external_path.with_extension("conflict");
        
        // Get current content from database
        let db_content = db.load_cell_content(cell_id, ContentLocation::Inline)?;
        
        // Write conflict file with both versions
        let conflict_content = format!(
            "<<<<<<< Database (current)\n{}\n=======\n{}\n>>>>>>> External Editor\n",
            db_content.as_str().unwrap_or(""),
            external_content
        );
        
        fs::write(&conflict_path, conflict_content)?;
        
        // Log conflict
        db.log_conflict(cell_id, "External edit conflict")?;
        
        Err(anyhow!(
            "Conflict detected: Cell {} was modified in both app and external editor. 
             See {} for conflict resolution.",
            cell_id,
            conflict_path.display()
        ))
    }
    
    fn create_temp_file(
        &self,
        cell_id: Ulid,
        cell_type: &CellType,
        content: &CellContent,
    ) -> Result<PathBuf> {
        let temp_dir = std::env::temp_dir().join("graph-cell-editor");
        fs::create_dir_all(&temp_dir)?;
        
        let filename = generate_external_filename(cell_id, *cell_type);
        let temp_path = temp_dir.join(filename);
        
        fs::write(&temp_path, content.as_str().unwrap_or(""))?;
        
        Ok(temp_path)
    }
}
```

---

## 9. Execution Tracing

### Ephemeral Execution (In-Memory)

```rust
pub struct ExecutionEngine {
    mode: ExecutionMode,
    current_step: usize,
    execution_queue: Vec<Ulid>,
    log: Vec<ExecutionLogEntry>,
    cell_outputs: HashMap<Ulid, CellData>,
    status: ExecutionStatus,
}

impl ExecutionEngine {
    pub fn execute(&mut self, canvas: &Canvas) -> Result<ExecutionReport> {
        // Execute as before...
        
        // After execution completes, log is only in memory
        Ok(ExecutionReport {
            status: self.status.clone(),
            step: self.current_step,
            log: self.log.clone(),
            total_cells_executed: self.log.len(),
        })
    }
}
```

### Optional Persistence

```rust
impl ExecutionEngine {
    /// Save execution trace to database for later review
    pub fn save_trace(&self, db: &Database, description: Option<String>) -> Result<Ulid> {
        let trace = ExecutionTrace {
            id: Ulid::new(),
            timestamp: Utc::now(),
            mode: self.mode,
            start_cell_id: self.find_start_cell()?,
            log: self.log.clone(),
            status: self.status.clone(),
            error_message: match &self.status {
                ExecutionStatus::Error(e) => Some(e.clone()),
                _ => None,
            },
        };
        
        let trace_data = serde_cbor::to_vec(&trace)?;
        
        db.conn.execute(
            "INSERT INTO execution_traces 
             (id, timestamp, mode, start_cell_id, trace_data, status, error_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                trace.id.to_string(),
                trace.timestamp.to_rfc3339(),
                trace.mode.to_string(),
                trace.start_cell_id.to_string(),
                trace_data,
                trace.status.to_string(),
                trace.error_message,
            ]
        )?;
        
        Ok(trace.id)
    }
    
    /// Load saved trace for review
    pub fn load_trace(db: &Database, trace_id: Ulid) -> Result<ExecutionTrace> {
        let (trace_data, mode, status, error): (Vec<u8>, String, String, Option<String>) = 
            db.conn.query_row(
                "SELECT trace_data, mode, status, error_message 
                 FROM execution_traces 
                 WHERE id = ?1",
                [trace_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            )?;
        
        let mut trace: ExecutionTrace = serde_cbor::from_slice(&trace_data)?;
        trace.mode = ExecutionMode::from_str(&mode)?;
        trace.status = ExecutionStatus::from_str(&status)?;
        trace.error_message = error;
        
        Ok(trace)
    }
}
```

### UI Integration

```rust
// In UI, add "Save Execution Trace" button after execution
if ui.button("💾 Save Execution Trace").clicked() {
    let description = format!("Execution at {}", Utc::now().format("%Y-%m-%d %H:%M"));
    match self.execution_engine.save_trace(&self.canvas.db, Some(description)) {
        Ok(trace_id) => {
            self.status_message = format!("✓ Saved execution trace {}", trace_id);
        }
        Err(e) => {
            self.status_message = format!("❌ Failed to save trace: {}", e);
        }
    }
}

// Add menu item to browse saved traces
if ui.button("📊 View Execution History").clicked() {
    self.show_execution_history_panel = true;
}
```

---

## 10. Import/Export Formats

### Full Project Export (.gce format)

```rust
pub struct ProjectExporter {
    db: Arc<Database>,
}

impl ProjectExporter {
    /// Export entire project as .gce (zip archive)
    pub fn export_project(&self, output_path: &Path) -> Result<()> {
        let temp_dir = tempdir()?;
        
        // 1. Copy database file
        let db_name = format!("{}.gcdb", self.get_project_name()?);
        fs::copy(self.db.path(), temp_dir.path().join(&db_name))?;
        
        // 2. Copy external files
        let external_dir = self.db.get_external_files_path()?;
        if external_dir.exists() {
            let dest_dir = temp_dir.path().join(format!("{}-files", self.get_project_name()?));
            Self::copy_dir_recursive(&external_dir, &dest_dir)?;
        }
        
        // 3. Create manifest
        let manifest = ExportManifest {
            version: "0.2.0".to_string(),
            exported_at: Utc::now(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            project_name: self.get_project_name()?,
            cell_count: self.db.count_cells()?,
            includes_history: true,
            includes_execution_traces: self.db.has_execution_traces()?,
        };
        
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(temp_dir.path().join("manifest.json"), manifest_json)?;
        
        // 4. Create zip archive
        let file = File::create(output_path)?;
        let mut zip = ZipWriter::new(file);
        
        Self::zip_directory(&mut zip, temp_dir.path(), "")?;
        
        zip.finish()?;
        
        Ok(())
    }
    
    /// Import project from .gce file
    pub fn import_project(&self, gce_path: &Path, dest_dir: &Path) -> Result<PathBuf> {
        let temp_dir = tempdir()?;
        
        // 1. Extract zip
        let file = File::open(gce_path)?;
        let mut archive = ZipArchive::new(file)?;
        archive.extract(temp_dir.path())?;
        
        // 2. Read manifest
        let manifest_path = temp_dir.path().join("manifest.json");
        let manifest_json = fs::read_to_string(&manifest_path)?;
        let manifest: ExportManifest = serde_json::from_str(&manifest_json)?;
        
        // 3. Validate version compatibility
        if !Self::is_compatible_version(&manifest.version)? {
            return Err(anyhow!(
                "Incompatible export version: {} (current: 0.2.0)",
                manifest.version
            ));
        }
        
        // 4. Move files to destination
        let db_name = format!("{}.gcdb", manifest.project_name);
        let db_path = dest_dir.join(&db_name);
        
        fs::copy(temp_dir.path().join(&db_name), &db_path)?;
        
        let external_src = temp_dir.path().join(format!("{}-files", manifest.project_name));
        if external_src.exists() {
            let external_dest = dest_dir.join(format!("{}-files", manifest.project_name));
            Self::copy_dir_recursive(&external_src, &external_dest)?;
        }
        
        Ok(db_path)
    }
}

#[derive(Serialize, Deserialize)]
struct ExportManifest {
    version: String,
    exported_at: DateTime<Utc>,
    app_version: String,
    project_name: String,
    cell_count: usize,
    includes_history: bool,
    includes_execution_traces: bool,
}
```

### Partial Export (Single Cell or Subset)

```rust
impl ProjectExporter {
    /// Export single cell as standalone file
    pub fn export_cell(&self, cell_id: Ulid, output_path: &Path) -> Result<()> {
        let cell = self.db.load_cell_metadata(cell_id)?;
        let content = self.db.load_cell_content(cell_id, cell.content_location)?;
        
        match output_path.extension().and_then(|s| s.to_str()) {
            Some("json") => {
                // Export as JSON with full metadata
                let export = CellExport {
                    id: cell.id,
                    short_id: cell.short_id,
                    name: cell.name,
                    cell_type: cell.cell_type,
                    content: content.as_str().unwrap_or("").to_string(),
                    created_at: cell.created_at,
                    modified_at: cell.modified_at,
                };
                
                let json = serde_json::to_string_pretty(&export)?;
                fs::write(output_path, json)?;
            }
            
            Some("py") | Some("txt") | Some("md") => {
                // Export just the content
                fs::write(output_path, content.as_str().unwrap_or(""))?;
            }
            
            _ => {
                // Default: export based on cell type
                let filename = generate_external_filename(cell_id, cell.cell_type);
                let path = output_path.join(filename);
                fs::write(path, content.as_str().unwrap_or(""))?;
            }
        }
        
        Ok(())
    }
    
    /// Export subset of cells as mini-project
    pub fn export_cells_subset(
        &self,
        cell_ids: &[Ulid],
        output_path: &Path,
    ) -> Result<()> {
        // Create temporary project with only selected cells
        let temp_db_path = tempdir()?.path().join("subset.gcdb");
        let subset_db = Database::create(&temp_db_path)?;
        
        // Copy selected cells and their relationships
        for &cell_id in cell_ids {
            let cell = self.db.load_full_cell(cell_id)?;
            subset_db.insert_cell_full(&cell)?;
        }
        
        // Copy relationships between selected cells
        for &from_id in cell_ids {
            for &to_id in cell_ids {
                if let Ok(rel) = self.db.get_relationship(from_id, to_id) {
                    subset_db.insert_relationship(&rel)?;
                }
            }
        }
        
        // Export as .gce
        let exporter = ProjectExporter::new(Arc::new(subset_db));
        exporter.export_project(output_path)?;
        
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct CellExport {
    id: Ulid,
    short_id: String,
    name: Option<String>,
    cell_type: CellType,
    content: String,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
}
```

---

## 11. Migration from cells.json

### Migration Utility

```rust
pub struct MigrationTool;

impl MigrationTool {
    /// Migrate from old cells.json format to new SQLite format
    pub fn migrate_from_json(
        old_project_dir: &Path,
        new_db_path: &Path,
    ) -> Result<()> {
        // 1. Read old format
        let cells_json = old_project_dir.join("cells.json");
        let manifest_json = old_project_dir.join("manifest.json");
        let events_jsonl = old_project_dir.join("events.jsonl");
        
        let old_canvas: OldSerializableCanvas = {
            let file = File::open(&cells_json)?;
            serde_json::from_reader(file)?
        };
        
        let old_manifest: OldManifest = {
            let file = File::open(&manifest_json)?;
            serde_json::from_reader(file)?
        };
        
        // 2. Create new database
        let db = Database::create(new_db_path)?;
        
        // 3. Migrate project metadata
        db.set_metadata("version", "0.2.0")?;
        db.set_metadata("created_at", &old_manifest.created.to_rfc3339())?;
        db.set_metadata("modified_at", &old_manifest.modified.to_rfc3339())?;
        
        // 4. Migrate cells
        let mut id_generator = IdGenerator::new();
        
        for old_cell in old_canvas.cells {
            let short_id = id_generator.next();
            
            // Determine content location based on type and size
            let content_location = if old_cell.cell_type == CellType::Python {
                ContentLocation::External
            } else if let Some(content) = old_cell.content.as_str() {
                if content.len() > INLINE_SIZE_THRESHOLD {
                    ContentLocation::External
                } else {
                    ContentLocation::Inline
                }
            } else {
                ContentLocation::Inline
            };
            
            // Create external file if needed
            let content_path = if content_location == ContentLocation::External {
                let external_dir = new_db_path.parent().unwrap()
                    .join(format!("{}-files/cells", 
                        new_db_path.file_stem().unwrap().to_str().unwrap()));
                fs::create_dir_all(&external_dir)?;
                
                let filename = generate_external_filename(old_cell.id, old_cell.cell_type);
                let file_path = external_dir.join(&filename);
                
                if let Some(content) = old_cell.content.as_str() {
                    fs::write(&file_path, content)?;
                }
                
                Some(format!("cells/{}", filename))
            } else {
                None
            };
            
            // Insert cell
            db.conn.execute(
                "INSERT INTO cells 
                 (id, short_id, name, cell_type, x, y, width, height,
                  content_location, content_text, content_path, content_hash,
                  is_start_point, parent_id, split_direction,
                  created_at, modified_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    old_cell.id.to_string(),
                    short_id,
                    old_cell.name,
                    old_cell.cell_type.to_string(),
                    old_cell.bounds.x,
                    old_cell.bounds.y,
                    old_cell.bounds.width,
                    old_cell.bounds.height,
                    content_location.to_string(),
                    if content_location == ContentLocation::Inline {
                        old_cell.content.as_str().map(|s| s.to_string())
                    } else {
                        None
                    },
                    content_path,
                    old_cell.content.as_str().map(|s| hash_content(s)),
                    old_cell.is_start_point as i32,
                    old_cell.parent.map(|id| id.to_string()),
                    old_cell.split_direction.map(|d| d.to_string()),
                    Utc::now().to_rfc3339(),
                    Utc::now().to_rfc3339(),
                ]
            )?;
        }
        
        // 5. Migrate relationships
        for old_rel in old_canvas.relationships {
            db.conn.execute(
                "INSERT INTO relationships (from_id, to_id, created_at)
                 VALUES (?1, ?2, ?3)",
                params![
                    old_rel.from.to_string(),
                    old_rel.to.to_string(),
                    Utc::now().to_rfc3339(),
                ]
            )?;
        }
        
        // 6. Optionally migrate events to snapshots
        if events_jsonl.exists() {
            Self::migrate_events_to_snapshots(&db, &events_jsonl)?;
        }
        
        println!("✓ Migration complete!");
        println!("  Migrated {} cells", old_canvas.cells.len());
        println!("  Migrated {} relationships", old_canvas.relationships.len());
        
        Ok(())
    }
    
    fn migrate_events_to_snapshots(db: &Database, events_path: &Path) -> Result<()> {
        let file = File::open(events_path)?;
        let reader = BufReader::new(file);
        
        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            
            let event: OldGraphEvent = serde_json::from_str(&line)?;
            
            // Convert event to snapshot (simplified)
            let description = format!("Migrated event: {:?}", event.event);
            let changes = vec![]; // Would need to reconstruct from event
            
            db.create_snapshot(
                OperationType::BatchOperation,
                description,
                changes,
            )?;
        }
        
        Ok(())
    }
}
```

---

## 12. Conflict Resolution & Versioning

### Version Tracking

```rust
impl Database {
    /// Log application version with each save
    fn log_version(&self, operation: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO version_log (timestamp, app_version, operation)
             VALUES (?1, ?2, ?3)",
            params![
                Utc::now().to_rfc3339(),
                env!("CARGO_PKG_VERSION"),
                operation,
            ]
        )?;
        Ok(())
    }
    
    /// Check if database was created by compatible version
    pub fn check_version_compatibility(&self) -> Result<bool> {
        let db_version: String = self.conn.query_row(
            "SELECT value FROM project_meta WHERE key = 'version'",
            [],
            |row| row.get(0)
        )?;
        
        let current_version = "0.2.0";
        
        // Simple version comparison (would use proper semver in production)
        Ok(db_version.starts_with("0.2"))
    }
    
    /// Get last app version that modified the database
    pub fn get_last_app_version(&self) -> Result<String> {
        self.conn.query_row(
            "SELECT app_version FROM version_log 
             ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| row.get(0)
        ).or_else(|_| Ok("unknown".to_string()))
    }
}
```

### Merge Conflict Detection

```rust
impl Database {
    /// Detect if database was modified externally
    pub fn detect_external_modifications(&self) -> Result<Vec<ConflictInfo>> {
        let mut conflicts = Vec::new();
        
        // Check for cells modified since last load
        let last_load_time = self.get_last_load_time()?;
        
        let mut stmt = self.conn.prepare(
            "SELECT id, short_id, modified_at, content_hash
             FROM cells
             WHERE modified_at > ?1"
        )?;
        
        let modified_cells = stmt.query_map([last_load_time], |row| {
            Ok(ConflictInfo {
                cell_id: Ulid::from_string(&row.get::<_, String>(0)?)?,
                short_id: row.get(1)?,
                conflict_type: ConflictType::ExternalModification,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)?.into(),
            })
        })?;
        
        for cell in modified_cells {
            conflicts.push(cell?);
        }
        
        Ok(conflicts)
    }
    
    /// Mark conflict for manual resolution
    pub fn log_conflict(&self, cell_id: Ulid, reason: &str) -> Result<()> {
        let conflict_marker = format!("CONFLICT_{}", Ulid::new());
        
        self.conn.execute(
            "INSERT INTO version_log (timestamp, app_version, operation, conflict_marker)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                Utc::now().to_rfc3339(),
                env!("CARGO_PKG_VERSION"),
                reason,
                conflict_marker,
            ]
        )?;
        
        // Update cell to mark conflict
        self.conn.execute(
            "UPDATE cells 
             SET name = COALESCE(name, '') || ' [CONFLICT]'
             WHERE id = ?1",
            [cell_id.to_string()]
        )?;
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ConflictInfo {
    pub cell_id: Ulid,
    pub short_id: String,
    pub conflict_type: ConflictType,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictType {
    ExternalModification,    // Database modified outside app
    ConcurrentEdit,          // Multiple instances editing
    ExternalFileConflict,    // External editor vs app
}
```

### Conflict Resolution UI

```rust
// In UI module
pub struct ConflictResolutionPanel {
    conflicts: Vec<ConflictInfo>,
    selected_conflict: Option<usize>,
}

impl ConflictResolutionPanel {
    pub fn render(&mut self, ui: &mut egui::Ui, canvas: &mut Canvas) {
        ui.heading("⚠️ Conflicts Detected");
        
        if self.conflicts.is_empty() {
            ui.label("No conflicts");
            return;
        }
        
        for (i, conflict) in self.conflicts.iter().enumerate() {
            ui.horizontal(|ui| {
                let selected = self.selected_conflict == Some(i);
                
                if ui.selectable_label(selected, &conflict.short_id).clicked() {
                    self.selected_conflict = Some(i);
                }
                
                ui.label(format!("{:?}", conflict.conflict_type));
                ui.label(conflict.timestamp.format("%Y-%m-%d %H:%M").to_string());
                
                if ui.button("Resolve").clicked() {
                    self.resolve_conflict(canvas, i);
                }
            });
        }
        
        // Show details for selected conflict
        if let Some(idx) = self.selected_conflict {
            if let Some(conflict) = self.conflicts.get(idx) {
                ui.separator();
                ui.heading("Conflict Details");
                
                // Show diff, allow user to choose version
                // ...
            }
        }
    }
    
    fn resolve_conflict(&mut self, canvas: &mut Canvas, conflict_idx: usize) {
        // Implementation depends on conflict type
        // For now, just remove conflict marker
        if let Some(conflict) = self.conflicts.get(conflict_idx) {
            // Remove "[CONFLICT]" from cell name
            // ...
            
            self.conflicts.remove(conflict_idx);
            if self.selected_conflict == Some(conflict_idx) {
                self.selected_conflict = None;
            }
        }
    }
}
```

---

## 13. Performance Considerations

### Indexing Strategy

All critical indexes are defined in schema (see Section 3).

Additional runtime considerations:

```rust
impl Database {
    /// Analyze query performance and suggest optimizations
    pub fn analyze_performance(&self) -> Result<PerformanceReport> {
        let mut report = PerformanceReport::default();
        
        // Check index usage
        let index_usage: Vec<(String, i64)> = self.conn.prepare(
            "SELECT name, seq FROM sqlite_stat1 WHERE tbl = 'cells'"
        )?.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
        
        report.index_count = index_usage.len();
        
        // Check database size
        let page_count: i64 = self.conn.query_row(
            "PRAGMA page_count",
            [],
            |row| row.get(0)
        )?;
        
        let page_size: i64 = self.conn.query_row(
            "PRAGMA page_size",
            [],
            |row| row.get(0)
        )?;
        
        report.db_size_bytes = page_count * page_size;
        
        // Check fragmentation
        let freelist_count: i64 = self.conn.query_row(
            "PRAGMA freelist_count",
            [],
            |row| row.get(0)
        )?;
        
        report.fragmentation_pct = (freelist_count as f64 / page_count as f64) * 100.0;
        
        // Suggest vacuum if fragmented
        if report.fragmentation_pct > 20.0 {
            report.suggestions.push("Run VACUUM to defragment database".to_string());
        }
        
        Ok(report)
    }
    
    /// Optimize database (vacuum, analyze)
    pub fn optimize(&self) -> Result<()> {
        self.conn.execute("VACUUM", [])?;
        self.conn.execute("ANALYZE", [])?;
        Ok(())
    }
}

#[derive(Default)]
pub struct PerformanceReport {
    pub index_count: usize,
    pub db_size_bytes: i64,
    pub fragmentation_pct: f64,
    pub suggestions: Vec<String>,
}
```

### Content Hash Optimization

```rust
fn hash_content(content: &str) -> String {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    
    format!("{:x}", result)
}

impl Database {
    /// Check if content changed without loading full content
    pub fn has_content_changed(&self, cell_id: Ulid, new_content: &str) -> Result<bool> {
        let stored_hash: String = self.conn.query_row(
            "SELECT content_hash FROM cells WHERE id = ?1",
            [cell_id.to_string()],
            |row| row.get(0)
        )?;
        
        let new_hash = hash_content(new_content);
        
        Ok(stored_hash != new_hash)
    }
}
```

---

## 14. Implementation Phases

### Phase 1: Core Database Infrastructure

**Goals:**
- Create SQLite schema
- Implement basic CRUD operations
- Migration from cells.json

**Tasks:**
1. Define schema SQL (cells, relationships, snapshots, etc.)
2. Implement `Database` struct with connection pooling
3. Write insert/update/delete methods for cells
4. Write insert/delete methods for relationships
5. Implement content location logic (inline vs external)
6. Create migration tool from cells.json
7. Write comprehensive unit tests

**Deliverables:**
- `database.rs` module
- `migration.rs` module
- Test suite covering all CRUD operations

---

### Phase 2: Lazy Loading & Content Management

**Goals:**
- Implement lazy loading architecture
- External file handling
- Content storage strategy

**Tasks:**
1. Implement `CellHandle` with `OnceCell`
2. Modify `Canvas` to use lazy loading
3. Implement `load_cell_metadata()` and `load_cell_content()`
4. Create content location resolver
5. Implement Python cell → external file default
6. Add content hash calculation
7. Implement LRU cache for loaded content
8. Add memory usage monitoring

**Deliverables:**
- `lazy_loading.rs` module
- Updated `Canvas` implementation
- Performance benchmarks

---

### Phase 3: Snapshot System

**Goals:**
- Implement undo/redo
- Snapshot retention policy

**Tasks:**
1. Define `Snapshot`, `Change`, `CellDiff` structures
2. Implement `create_snapshot()` with CBOR serialization
3. Implement `UndoManager` with undo/redo logic
4. Add snapshot pruning (keep last 50)
5. Integrate snapshots into Canvas operations
6. Test undo/redo for all operation types
7. Add UI controls for undo/redo

**Deliverables:**
- `snapshots.rs` module
- `UndoManager` implementation
- UI integration

---

### Phase 4: External Editing

**Goals:**
- VS Code integration
- File watching and sync

**Tasks:**
1. Implement `ExternalEditorManager`
2. Add temp file creation for inline content
3. Implement file hash tracking
4. Add process watcher for editor close
5. Implement sync from external file
6. Add conflict detection for external edits
7. Create conflict resolution UI
8. Test with VS Code, Vim, other editors

**Deliverables:**
- `external_editor.rs` module
- Conflict resolution UI
- Editor integration docs

---

### Phase 5: Import/Export

**Goals:**
- Project export as .gce
- Partial cell export
- Import validation

**Tasks:**
1. Implement `ProjectExporter`
2. Create .gce zip format
3. Add export manifest
4. Implement import with validation
5. Add version compatibility checks
6. Implement single cell export (JSON, native)
7. Implement subset export
8. Add export UI controls

**Deliverables:**
- `import_export.rs` module
- Export/import UI
- .gce format specification

---

### Phase 6: Execution Tracing

**Goals:**
- Optional trace persistence
- Trace viewer UI

**Tasks:**
1. Add `save_trace()` method to `ExecutionEngine`
2. Implement trace loading and replay
3. Create execution history panel
4. Add trace visualization (step-by-step view)
5. Implement trace export (JSON, CSV)
6. Test with complex workflows

**Deliverables:**
- Updated `execution.rs`
- Trace viewer UI panel
- Trace export functionality

---

### Phase 7: Versioning & Conflict Resolution

**Goals:**
- Version tracking
- Conflict detection and resolution

**Tasks:**
1. Implement version_log table population
2. Add compatibility checking on project open
3. Implement external modification detection
4. Create conflict logging system
5. Build conflict resolution UI
6. Add merge conflict file generation
7. Test multi-instance scenarios

**Deliverables:**
- `versioning.rs` module
- Conflict resolution panel
- Version migration guide

---

### Phase 8: Polish & Optimization

**Goals:**
- Performance tuning
- Bug fixes
- Documentation

**Tasks:**
1. Profile database queries
2. Optimize indexes based on real usage
3. Implement database VACUUM scheduler
4. Add performance monitoring UI
5. Write user documentation
6. Create developer guide
7. Conduct stress testing (10,000+ cells)
8. Fix critical bugs

**Deliverables:**
- Performance report
- User guide
- Developer documentation
- Bug fixes

---

## 15. Future Considerations

### Multi-Database Architecture (Not Implemented)

For very large projects (10,000+ cells), consider splitting:

```
myproject-structure.gcdb    # Cells, relationships, metadata
myproject-history.gcdb      # Snapshots, execution traces
```

Benefits:
- Smaller main database for faster queries
- History can be archived/compressed separately
- Easier to exclude history from exports

### Real-Time Collaboration (Future)

Collaboration would require:
1. **WebSocket server** for real-time updates
2. **Operational Transforms** or **CRDTs** for conflict-free merging
3. **User presence tracking**
4. **Per-cell locking** during edits
5. **Cloud sync** for database

Recommended approach: Use **Automerge** or **Y.js** CRDT library.

### Advanced Caching

Current: Simple LRU cache for loaded content

Future:
- **Predictive loading**: Pre-load cells likely to be viewed next
- **Spatial caching**: Cache cells in viewport + surrounding area
- **Usage-based caching**: Keep frequently accessed cells in memory
- **Disk cache**: Use SQLite temp tables for intermediate state

### Compression

For large text cells:
- Store compressed content in BLOB (zstd, lz4)
- Decompress on load
- Trade CPU for storage space

### Full-Text Search

Add FTS5 table for searching cell content:

```sql
CREATE VIRTUAL TABLE cells_fts USING fts5(
    id UNINDEXED,
    content,
    tokenize = 'porter unicode61'
);

-- Populate from cells table
INSERT INTO cells_fts (id, content)
SELECT id, content_text FROM cells WHERE content_location = 'inline';
```

### Remote Sync

Future cloud sync service:
- Delta-based sync (only changed cells)
- Conflict resolution server
- Offline-first architecture
- End-to-end encryption option

---

## 16. Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cell_crud() {
        let db = Database::create_in_memory().unwrap();
        
        // Create cell
        let cell_id = db.create_cell(/* ... */).unwrap();
        
        // Read cell
        let cell = db.load_cell_metadata(cell_id).unwrap();
        assert_eq!(cell.cell_type, CellType::Python);
        
        // Update cell
        db.update_cell_content(cell_id, "new content").unwrap();
        
        // Delete cell
        db.delete_cell(cell_id).unwrap();
        assert!(db.load_cell_metadata(cell_id).is_err());
    }
    
    #[test]
    fn test_snapshot_undo_redo() {
        let db = Database::create_in_memory().unwrap();
        let mut undo_mgr = UndoManager::new(Arc::new(db)).unwrap();
        
        // Create cell
        let cell_id = db.create_cell(/* ... */).unwrap();
        
        // Modify cell
        db.update_cell_content(cell_id, "modified").unwrap();
        
        // Undo
        undo_mgr.undo().unwrap();
        let cell = db.load_cell_content(cell_id).unwrap();
        assert_eq!(cell.as_str(), Some("original"));
        
        // Redo
        undo_mgr.redo().unwrap();
        let cell = db.load_cell_content(cell_id).unwrap();
        assert_eq!(cell.as_str(), Some("modified"));
    }
    
    #[test]
    fn test_lazy_loading() {
        let db = Database::create_in_memory().unwrap();
        let canvas = Canvas::open_from_db(Arc::new(db)).unwrap();
        
        let cell = canvas.get_cell(cell_id).unwrap();
        assert!(!cell.is_content_loaded());
        
        let content = cell.content().unwrap();
        assert!(cell.is_content_loaded());
    }
}
```

### Integration Tests

```rust
#[test]
fn test_full_workflow() {
    // Create project
    let db_path = tempdir().unwrap().path().join("test.gcdb");
    let db = Database::create(&db_path).unwrap();
    let mut canvas = Canvas::new(Arc::new(db));
    
    // Create cells
    let cell1 = canvas.create_cell(/* ... */);
    let cell2 = canvas.create_cell(/* ... */);
    
    // Split cell
    canvas.split_cell(cell1, SplitDirection::Horizontal, 0.5).unwrap();
    
    // Create relationship
    canvas.create_relationship(cell1, cell2).unwrap();
    
    // Execute
    let mut engine = ExecutionEngine::new(ExecutionMode::Run);
    let report = engine.execute(&canvas).unwrap();
    assert_eq!(report.status, ExecutionStatus::Complete);
    
    // Export
    let exporter = ProjectExporter::new(canvas.db.clone());
    exporter.export_project(&output_path).unwrap();
    
    // Import
    let imported_path = exporter.import_project(&output_path, &import_dir).unwrap();
    let imported_canvas = Canvas::open(&imported_path).unwrap();
    
    assert_eq!(imported_canvas.cell_count(), canvas.cell_count());
}
```

### Performance Tests

```rust
#[test]
fn test_large_project_performance() {
    let db = Database::create_in_memory().unwrap();
    
    // Create 10,000 cells
    let start = Instant::now();
    for _ in 0..10_000 {
        db.create_cell(/* ... */);
    }
    let create_time = start.elapsed();
    
    assert!(create_time < Duration::from_secs(10), "Create too slow");
    
    // Load metadata only
    let start = Instant::now();
    let canvas = Canvas::open_from_db(Arc::new(db)).unwrap();
    let load_time = start.elapsed();
    
    assert!(load_time < Duration::from_millis(500), "Load too slow");
    
    // Lazy load content
    let start = Instant::now();
    for cell in canvas.cells().values().take(100) {
        let _ = cell.content();
    }
    let lazy_load_time = start.elapsed();
    
    assert!(lazy_load_time < Duration::from_secs(1), "Lazy load too slow");
}
```

---

## Summary

This design document provides a comprehensive blueprint for migrating Graph Cell Editor to a SQLite-based storage system with:

1. **Efficient lazy loading** for large projects
2. **Robust undo/redo** via snapshots (last 50, configurable)
3. **External editing** support with VS Code integration
4. **Flexible attachment** storage (local, remote, symlinked)
5. **Export/import** in .gce format (zip archive)
6. **Version tracking** and conflict detection
7. **Performance optimization** through indexing and caching

The implementation is phased over 8 phases, with clear deliverables and testing strategies for each phase.

Key decisions:
- **Aggressive saving** (every operation commits immediately)
- **Python cells default to external** storage
- **1MB threshold** for inline vs external
- **Sync on editor close** (not while editing)
- **Basic conflict resolution** (manual merge)

This architecture is future-proof for collaboration, cloud sync, and advanced features while maintaining simplicity and reliability for the MVP.