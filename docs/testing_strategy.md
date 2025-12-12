# Testing Methodology - Graph Cell Editor
## Comprehensive Testing Strategy for Rust egui Desktop Application

---

## 1. Overview & Testing Philosophy

### Core Principles

1. **Test What Matters**: Focus on execution correctness, data persistence, and user interaction flows
2. **Start Simple**: Begin with fixed test cases, add property-based tests incrementally
3. **Track State Changes**: Every user action should have verifiable state changes and side effects
4. **Fail Fast**: Tests should catch regressions immediately during development
5. **Readable Tests**: Tests are documentation - they should clearly express intent

### Testing Priorities (Ranked)

1. **Execution Engine Correctness** - Critical for app functionality
2. **Database Persistence** - Data integrity is non-negotiable
3. **User Interactions & UI State** - UX correctness and state management
4. Graph operations (split, merge, relationships)
5. File operations and external editing
6. Validation and error handling

### Testing Pyramid

```
                    ╱╲
                   ╱  ╲
                  ╱ E2E╲           <- Few: Full workflows (5-10 tests)
                 ╱──────╲
                ╱        ╲
               ╱Integration╲       <- Some: Multi-component (20-30 tests)
              ╱────────────╲
             ╱              ╲
            ╱  Unit + Prop.  ╲    <- Many: Component-level (100+ tests)
           ╱──────────────────╲
```

---

## 2. Test Organization Structure

### Directory Layout

```
graph-cell-editor/
├── src/
│   ├── lib.rs
│   ├── canvas.rs
│   │   └── mod tests { ... }           # Unit tests alongside code
│   ├── execution.rs
│   │   └── mod tests { ... }
│   ├── database.rs
│   │   └── mod tests { ... }
│   └── ui.rs
│       └── mod tests { ... }
│
├── tests/
│   ├── integration/
│   │   ├── mod.rs
│   │   ├── workflow_tests.rs           # Full user workflows
│   │   ├── database_integration.rs     # Database + Canvas
│   │   └── execution_integration.rs    # Canvas + Execution
│   │
│   ├── property/
│   │   ├── mod.rs
│   │   ├── execution_properties.rs     # Execution determinism
│   │   ├── serialization_properties.rs # Roundtrip tests
│   │   └── undo_redo_properties.rs     # Undo/redo invariants
│   │
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── ui_test_harness.rs         # UI testing framework
│   │   ├── interaction_tests.rs        # User interaction tests
│   │   └── state_tracking_tests.rs     # UI state verification
│   │
│   └── fixtures/
│       ├── mod.rs
│       ├── sample_graphs.rs            # Pre-built test graphs
│       └── test_data.rs                # Test data generators
│
└── Cargo.toml
```

### Dependencies

```toml
[dev-dependencies]
# Property-based testing
proptest = "1.4"

# Snapshot testing
insta = "1.34"

# Temporary directories for file tests
tempfile = "3.8"

# Assertions and test utilities
assert_matches = "1.5"
pretty_assertions = "1.4"

# Async testing (if needed later)
tokio-test = "0.4"

# Mocking (if needed)
mockall = "0.12"
```

---

## 3. Unit Testing Strategy

### 3.1 Database Testing

**Goal**: Verify all CRUD operations, transactions, and data integrity

```rust
// tests/database.rs or src/database.rs mod tests

use crate::database::Database;
use tempfile::TempDir;

#[test]
fn test_create_cell_with_inline_content() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Database::create(&db_path).unwrap();
    
    let cell_id = db.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("Hello World".to_string()),
        "A1".to_string(),
    ).unwrap();
    
    // Verify cell was created
    let cell = db.load_cell_metadata(cell_id).unwrap();
    assert_eq!(cell.short_id, "A1");
    assert_eq!(cell.cell_type, CellType::Text);
    assert_eq!(cell.content_location, ContentLocation::Inline);
    
    // Verify content can be loaded
    let content = db.load_cell_content(cell_id, cell.content_location).unwrap();
    assert_eq!(content.as_str(), Some("Hello World"));
}

#[test]
fn test_python_cell_defaults_to_external_storage() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Database::create(&db_path).unwrap();
    
    let code = "def hello():\n    return 'world'";
    let cell_id = db.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 200.0, 150.0),
        CellContent::Inline(code.to_string()),
        "B2".to_string(),
    ).unwrap();
    
    // Verify Python cell is stored externally
    let cell = db.load_cell_metadata(cell_id).unwrap();
    assert_eq!(cell.content_location, ContentLocation::External);
    
    // Verify file was created
    let external_path = db.resolve_external_path(&cell.content_path.unwrap()).unwrap();
    assert!(external_path.exists());
    
    // Verify content is correct
    let loaded_content = std::fs::read_to_string(&external_path).unwrap();
    assert_eq!(loaded_content, code);
}

#[test]
fn test_large_content_auto_external_storage() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Database::create(&db_path).unwrap();
    
    // Create content > 1MB
    let large_content = "A".repeat(2_000_000); // 2MB
    
    let cell_id = db.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 300.0, 300.0),
        CellContent::Inline(large_content.clone()),
        "C3".to_string(),
    ).unwrap();
    
    let cell = db.load_cell_metadata(cell_id).unwrap();
    assert_eq!(cell.content_location, ContentLocation::External);
}

#[test]
fn test_transaction_rollback_on_error() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Database::create(&db_path).unwrap();
    
    let cell_id = db.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("Original".to_string()),
        "D4".to_string(),
    ).unwrap();
    
    // Attempt invalid operation (should rollback)
    let result = db.create_relationship(cell_id, cell_id); // Self-reference
    assert!(result.is_err());
    
    // Verify database is still consistent
    let cell = db.load_cell_metadata(cell_id).unwrap();
    assert_eq!(cell.content.as_str(), Some("Original"));
}

#[test]
fn test_concurrent_reads() {
    use std::sync::Arc;
    use std::thread;
    
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Arc::new(Database::create(&db_path).unwrap());
    
    let cell_id = db.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("Shared".to_string()),
        "E5".to_string(),
    ).unwrap();
    
    // Spawn multiple readers
    let mut handles = vec![];
    for _ in 0..10 {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            db_clone.load_cell_metadata(cell_id).unwrap()
        });
        handles.push(handle);
    }
    
    // All reads should succeed
    for handle in handles {
        let cell = handle.join().unwrap();
        assert_eq!(cell.short_id, "E5");
    }
}
```

### 3.2 Execution Engine Testing

**Goal**: Verify execution correctness, determinism, and error handling

```rust
// src/execution.rs mod tests

#[test]
fn test_single_cell_execution() {
    let mut canvas = Canvas::new();
    let cell_id = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(42)".to_string()),
    );
    
    canvas.set_start_point(cell_id).unwrap();
    
    let mut engine = ExecutionEngine::new(ExecutionMode::Run);
    let report = engine.execute(&canvas).unwrap();
    
    assert_eq!(report.status, ExecutionStatus::Complete);
    assert_eq!(report.total_cells_executed, 1);
    assert_eq!(report.log[0].output, CellData::Number(42.0));
}

#[test]
fn test_execution_with_data_flow() {
    let mut canvas = Canvas::new();
    
    // Cell 1: produces data
    let cell1 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(10)".to_string()),
    );
    
    // Cell 2: consumes data
    let cell2 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(input_0 * 2)".to_string()),
    );
    
    canvas.create_relationship(cell1, cell2).unwrap();
    canvas.set_start_point(cell1).unwrap();
    
    let mut engine = ExecutionEngine::new(ExecutionMode::Run);
    let report = engine.execute(&canvas).unwrap();
    
    assert_eq!(report.status, ExecutionStatus::Complete);
    assert_eq!(report.total_cells_executed, 2);
    assert_eq!(report.log[1].output, CellData::Number(20.0));
}

#[test]
fn test_execution_step_mode() {
    let mut canvas = Canvas::new();
    
    let cell1 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(1)".to_string()),
    );
    
    let cell2 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(2)".to_string()),
    );
    
    canvas.create_relationship(cell1, cell2).unwrap();
    canvas.set_start_point(cell1).unwrap();
    
    let mut engine = ExecutionEngine::new(ExecutionMode::Step);
    
    // First step
    let report1 = engine.execute(&canvas).unwrap();
    assert_eq!(report1.status, ExecutionStatus::Paused);
    assert_eq!(report1.step, 1);
    
    // Continue
    let report2 = engine.continue_execution(&canvas).unwrap();
    assert_eq!(report2.status, ExecutionStatus::Complete);
    assert_eq!(report2.step, 2);
}

#[test]
fn test_execution_error_handling() {
    let mut canvas = Canvas::new();
    
    let cell = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("undefined_variable".to_string()),
    );
    
    canvas.set_start_point(cell).unwrap();
    
    let mut engine = ExecutionEngine::new(ExecutionMode::Run);
    let result = engine.execute(&canvas);
    
    assert!(result.is_err());
}

#[test]
fn test_execution_dry_run_mode() {
    let mut canvas = Canvas::new();
    
    let cell = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(100)".to_string()),
    );
    
    canvas.set_start_point(cell).unwrap();
    
    let mut engine = ExecutionEngine::new(ExecutionMode::DryRun);
    let report = engine.execute(&canvas).unwrap();
    
    assert_eq!(report.status, ExecutionStatus::DryRunComplete);
    assert!(report.log[0].dry_run);
}

#[test]
fn test_execution_determinism() {
    // Same graph executed twice should produce identical results
    fn create_test_graph() -> Canvas {
        let mut canvas = Canvas::new();
        let cell1 = canvas.create_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::Inline("set_output(42)".to_string()),
        );
        canvas.set_start_point(cell1).unwrap();
        canvas
    }
    
    let canvas1 = create_test_graph();
    let canvas2 = create_test_graph();
    
    let mut engine1 = ExecutionEngine::new(ExecutionMode::Run);
    let report1 = engine1.execute(&canvas1).unwrap();
    
    let mut engine2 = ExecutionEngine::new(ExecutionMode::Run);
    let report2 = engine2.execute(&canvas2).unwrap();
    
    assert_eq!(report1.log[0].output, report2.log[0].output);
}

#[test]
fn test_execution_conflict_detection() {
    let mut canvas = Canvas::new();
    
    let cell1 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(1)".to_string()),
    );
    
    let cell2 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(2)".to_string()),
    );
    
    let cell3 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(75.0, 150.0, 100.0, 100.0),
        CellContent::Inline("set_output(input_0)".to_string()),
    );
    
    // Create diamond: cell1 and cell2 both write to cell3 in same step
    canvas.create_relationship(cell1, cell3).unwrap();
    canvas.create_relationship(cell2, cell3).unwrap();
    canvas.set_start_point(cell1).unwrap();
    
    let mut engine = ExecutionEngine::new(ExecutionMode::Run);
    let result = engine.execute(&canvas);
    
    // Should detect conflict
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Conflict"));
}
```

### 3.3 Canvas Operations Testing

**Goal**: Verify graph operations maintain invariants

```rust
// src/canvas.rs mod tests

#[test]
fn test_split_cell_horizontal() {
    let mut canvas = Canvas::new();
    let parent = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 200.0, 200.0),
        CellContent::Inline("Parent".to_string()),
    );
    
    let (child1, child2) = canvas
        .split_cell(parent, SplitDirection::Horizontal, 0.5)
        .unwrap();
    
    // Verify parent still exists but has children
    let parent_cell = canvas.get_cell(parent).unwrap();
    assert_eq!(parent_cell.children.len(), 2);
    assert!(parent_cell.children.contains(&child1));
    assert!(parent_cell.children.contains(&child2));
    
    // Verify child1 (top half)
    let child1_cell = canvas.get_cell(child1).unwrap();
    assert_eq!(child1_cell.bounds.y, 0.0);
    assert_eq!(child1_cell.bounds.height, 100.0);
    assert_eq!(child1_cell.content.as_str(), Some("Parent")); // Inherits content
    assert_eq!(child1_cell.parent, Some(parent));
    
    // Verify child2 (bottom half)
    let child2_cell = canvas.get_cell(child2).unwrap();
    assert_eq!(child2_cell.bounds.y, 100.0);
    assert_eq!(child2_cell.bounds.height, 100.0);
    assert_eq!(child2_cell.content.as_str(), Some("")); // Empty
    assert_eq!(child2_cell.parent, Some(parent));
}

#[test]
fn test_split_cell_vertical() {
    let mut canvas = Canvas::new();
    let parent = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 200.0, 200.0),
        CellContent::Inline("Parent".to_string()),
    );
    
    let (child1, child2) = canvas
        .split_cell(parent, SplitDirection::Vertical, 0.5)
        .unwrap();
    
    let child1_cell = canvas.get_cell(child1).unwrap();
    let child2_cell = canvas.get_cell(child2).unwrap();
    
    // Verify split is vertical
    assert_eq!(child1_cell.bounds.x, 0.0);
    assert_eq!(child1_cell.bounds.width, 100.0);
    assert_eq!(child2_cell.bounds.x, 100.0);
    assert_eq!(child2_cell.bounds.width, 100.0);
}

#[test]
fn test_split_preserves_start_point() {
    let mut canvas = Canvas::new();
    let parent = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 200.0, 200.0),
        CellContent::Inline("set_output(42)".to_string()),
    );
    
    canvas.set_start_point(parent).unwrap();
    
    let (child1, _child2) = canvas
        .split_cell(parent, SplitDirection::Horizontal, 0.5)
        .unwrap();
    
    // Child1 should inherit start point
    let child1_cell = canvas.get_cell(child1).unwrap();
    assert!(child1_cell.is_start_point);
}

#[test]
fn test_merge_cells() {
    let mut canvas = Canvas::new();
    
    let cell1 = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("Cell 1".to_string()),
    );
    
    let cell2 = canvas.create_cell(
        CellType::Text,
        Rectangle::new(100.0, 0.0, 100.0, 100.0),
        CellContent::Inline("Cell 2".to_string()),
    );
    
    let merged = canvas
        .merge_cells(
            vec![cell1, cell2],
            CellType::Text,
            CellContent::Inline("Merged".to_string()),
        )
        .unwrap();
    
    // Old cells should be deleted
    assert!(canvas.get_cell(cell1).is_none());
    assert!(canvas.get_cell(cell2).is_none());
    
    // New merged cell should exist
    let merged_cell = canvas.get_cell(merged).unwrap();
    assert_eq!(merged_cell.content.as_str(), Some("Merged"));
    assert_eq!(merged_cell.bounds.width, 200.0); // Bounding box of both
}

#[test]
fn test_adjacency_detection() {
    let mut canvas = Canvas::new();
    
    let cell1 = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("".to_string()),
    );
    
    let cell2 = canvas.create_cell(
        CellType::Text,
        Rectangle::new(100.0, 0.0, 100.0, 100.0), // Adjacent right
        CellContent::Inline("".to_string()),
    );
    
    let cell3 = canvas.create_cell(
        CellType::Text,
        Rectangle::new(200.0, 200.0, 100.0, 100.0), // Far away
        CellContent::Inline("".to_string()),
    );
    
    assert!(canvas.are_cells_adjacent(cell1, cell2).unwrap());
    assert!(!canvas.are_cells_adjacent(cell1, cell3).unwrap());
}

#[test]
fn test_relationship_creates_data_flow() {
    let mut canvas = Canvas::new();
    
    let cell1 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(10)".to_string()),
    );
    
    let cell2 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(input_0 * 2)".to_string()),
    );
    
    canvas.create_relationship(cell1, cell2).unwrap();
    
    // Verify relationship exists
    assert!(canvas.get_relationship(cell1, cell2).is_some());
    
    // Verify it enables data flow
    let outgoing = canvas.get_outgoing_relationships(cell1);
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].to, cell2);
}

#[test]
fn test_delete_cell_cascades_relationships() {
    let mut canvas = Canvas::new();
    
    let cell1 = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("".to_string()),
    );
    
    let cell2 = canvas.create_cell(
        CellType::Text,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::Inline("".to_string()),
    );
    
    canvas.create_relationship(cell1, cell2).unwrap();
    
    // Delete cell1
    canvas.delete_cell(cell1).unwrap();
    
    // Relationship should be deleted too
    assert_eq!(canvas.relationship_count(), 0);
}
```

### 3.4 Snapshot/Undo System Testing

**Goal**: Verify undo/redo correctness and state preservation

```rust
// src/snapshots.rs mod tests

#[test]
fn test_snapshot_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Database::create(&db_path).unwrap();
    
    let cell = SerializableCell {
        id: Ulid::new(),
        short_id: "A1".to_string(),
        name: None,
        cell_type: CellType::Text,
        bounds: Rectangle::new(0.0, 0.0, 100.0, 100.0),
        content: "Test".to_string(),
        is_start_point: false,
        parent: None,
        children: vec![],
        preview_mode: None,
    };
    
    let changes = vec![Change::CellCreated { cell }];
    
    let snapshot_id = db.create_snapshot(
        OperationType::CellCreated,
        "Created cell A1".to_string(),
        changes,
    ).unwrap();
    
    // Verify snapshot exists
    let snapshot = db.load_snapshot(snapshot_id).unwrap();
    assert_eq!(snapshot.description, "Created cell A1");
    assert_eq!(snapshot.changes.len(), 1);
}

#[test]
fn test_undo_cell_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Arc::new(Database::create(&db_path).unwrap());
    
    let cell_id = db.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("Original".to_string()),
        "A1".to_string(),
    ).unwrap();
    
    // Cell should exist
    assert!(db.load_cell_metadata(cell_id).is_ok());
    
    // Undo creation
    let mut undo_mgr = UndoManager::new(db.clone()).unwrap();
    undo_mgr.undo().unwrap();
    
    // Cell should be deleted
    assert!(db.load_cell_metadata(cell_id).is_err());
}

#[test]
fn test_undo_redo_cell_modification() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Arc::new(Database::create(&db_path).unwrap());
    
    let cell_id = db.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("Original".to_string()),
        "A1".to_string(),
    ).unwrap();
    
    // Modify content
    db.update_cell_content(cell_id, "Modified".to_string()).unwrap();
    
    let mut undo_mgr = UndoManager::new(db.clone()).unwrap();
    
    // Verify modified state
    let content = db.load_cell_content(cell_id, ContentLocation::Inline).unwrap();
    assert_eq!(content.as_str(), Some("Modified"));
    
    // Undo
    undo_mgr.undo().unwrap();
    let content = db.load_cell_content(cell_id, ContentLocation::Inline).unwrap();
    assert_eq!(content.as_str(), Some("Original"));
    
    // Redo
    undo_mgr.redo().unwrap();
    let content = db.load_cell_content(cell_id, ContentLocation::Inline).unwrap();
    assert_eq!(content.as_str(), Some("Modified"));
}

#[test]
fn test_undo_cell_split() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Arc::new(Database::create(&db_path).unwrap());
    let mut canvas = Canvas::new_with_db(db.clone());
    
    let parent = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 200.0, 200.0),
        CellContent::Inline("Parent".to_string()),
    );
    
    let (child1, child2) = canvas
        .split_cell(parent, SplitDirection::Horizontal, 0.5)
        .unwrap();
    
    // Children should exist
    assert!(canvas.get_cell(child1).is_some());
    assert!(canvas.get_cell(child2).is_some());
    
    // Undo split
    let mut undo_mgr = UndoManager::new(db.clone()).unwrap();
    undo_mgr.undo().unwrap();
    
    // Reload canvas to see undo effect
    let canvas = Canvas::load_from_db(db.clone()).unwrap();
    
    // Children should be gone
    assert!(canvas.get_cell(child1).is_none());
    assert!(canvas.get_cell(child2).is_none());
    
    // Parent should be back to original state
    let parent_cell = canvas.get_cell(parent).unwrap();
    assert_eq!(parent_cell.children.len(), 0);
}

#[test]
fn test_snapshot_pruning() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.gcdb");
    let db = Database::create(&db_path).unwrap();
    
    // Set retention to 5 snapshots
    db.set_metadata("snapshot_retention", "5").unwrap();
    
    // Create 10 snapshots
    for i in 0..10 {
        let changes = vec![Change::CellModified {
            id: Ulid::new(),
            before: CellDiff::default(),
            after: CellDiff::default(),
        }];
        
        db.create_snapshot(
            OperationType::CellModified,
            format!("Modification {}", i),
            changes,
        ).unwrap();
    }
    
    // Should only have 5 snapshots (oldest pruned)
    let snapshot_count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM snapshots",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert_eq!(snapshot_count, 5);
}
```

---

## 4. Property-Based Testing

### 4.1 Test Data Generators

**Create generators for complex test scenarios**

```rust
// tests/property/generators.rs

use proptest::prelude::*;
use crate::{Cell, CellType, Rectangle, CellContent, Canvas};

/// Generate valid rectangles
pub fn rectangle_strategy() -> impl Strategy<Value = Rectangle> {
    (0.0f32..1000.0, 0.0f32..1000.0, 10.0f32..500.0, 10.0f32..500.0)
        .prop_map(|(x, y, width, height)| Rectangle::new(x, y, width, height))
}

/// Generate cell types
pub fn cell_type_strategy() -> impl Strategy<Value = CellType> {
    prop_oneof![
        Just(CellType::Text),
        Just(CellType::Python),
    ]
}

/// Generate cell content (small strings for testing)
pub fn cell_content_strategy() -> impl Strategy<Value = CellContent> {
    prop_oneof![
        "[a-z]{10,100}".prop_map(|s| CellContent::Inline(s)),
        "set_output\\([0-9]{1,3}\\)".prop_map(|s| CellContent::Inline(s)),
    ]
}

/// Generate a single cell
pub fn cell_strategy() -> impl Strategy<Value = (CellType, Rectangle, CellContent)> {
    (cell_type_strategy(), rectangle_strategy(), cell_content_strategy())
}

/// Generate a small canvas (1-10 cells)
pub fn small_canvas_strategy() -> impl Strategy<Value = Canvas> {
    prop::collection::vec(cell_strategy(), 1..10)
        .prop_map(|cells| {
            let mut canvas = Canvas::new();
            for (cell_type, bounds, content) in cells {
                canvas.create_cell(cell_type, bounds, content);
            }
            canvas
        })
}

/// Generate executable Python code
pub fn python_code_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Simple output
        (0..100i32).prop_map(|n| format!("set_output({})", n)),
        
        // Input transformation
        "[+\\-*/]".prop_map(|op| format!("set_output(input_0 {} 2)", op)),
        
        // Multiple operations
        (0..10i32, 0..10i32).prop_map(|(a, b)| 
            format!("x = {}\ny = {}\nset_output(x + y)", a, b)
        ),
    ]
}

/// Generate execution graph (cells with relationships)
pub fn execution_graph_strategy() -> impl Strategy<Value = Canvas> {
    (2..5usize).prop_flat_map(|num_cells| {
        let cells = prop::collection::vec(python_code_strategy(), num_cells);
        cells.prop_map(|codes| {
            let mut canvas = Canvas::new();
            let mut cell_ids = vec![];
            
            // Create cells
            for code in codes {
                let id = canvas.create_cell(
                    CellType::Python,
                    Rectangle::new(0.0, 0.0, 100.0, 100.0),
                    CellContent::Inline(code),
                );
                cell_ids.push(id);
            }
            
            // Create chain of relationships
            for i in 0..cell_ids.len() - 1 {
                canvas.create_relationship(cell_ids[i], cell_ids[i + 1]).unwrap();
            }
            
            // Set start point
            canvas.set_start_point(cell_ids[0]).unwrap();
            
            canvas
        })
    })
}
```

### 4.2 Execution Properties

**Verify execution engine invariants**

```rust
// tests/property/execution_properties.rs

use proptest::prelude::*;
use crate::execution::*;

proptest! {
    #[test]
    fn execution_is_deterministic(canvas in execution_graph_strategy()) {
        // Same graph executed twice produces same results
        let mut engine1 = ExecutionEngine::new(ExecutionMode::Run);
        let report1 = engine1.execute(&canvas).unwrap();
        
        let mut engine2 = ExecutionEngine::new(ExecutionMode::Run);
        let report2 = engine2.execute(&canvas).unwrap();
        
        prop_assert_eq!(report1.log.len(), report2.log.len());
        
        for (log1, log2) in report1.log.iter().zip(report2.log.iter()) {
            prop_assert_eq!(log1.cell_id, log2.cell_id);
            prop_assert_eq!(log1.output, log2.output);
        }
    }
    
    #[test]
    fn execution_completes_or_errors(canvas in execution_graph_strategy()) {
        let mut engine = ExecutionEngine::new(ExecutionMode::Run);
        let result = engine.execute(&canvas);
        
        // Should either complete successfully or return error
        match result {
            Ok(report) => {
                prop_assert!(
                    report.status == ExecutionStatus::Complete ||
                    matches!(report.status, ExecutionStatus::Error(_))
                );
            }
            Err(_) => {
                // Error is acceptable
            }
        }
    }
    
    #[test]
    fn step_mode_executes_incrementally(canvas in execution_graph_strategy()) {
        let mut engine = ExecutionEngine::new(ExecutionMode::Step);
        
        let mut steps = 0;
        loop {
            let report = engine.execute(&canvas).unwrap();
            steps += 1;
            
            if report.status == ExecutionStatus::Complete {
                break;
            }
            
            prop_assert_eq!(report.status, ExecutionStatus::Paused);
            prop_assert!(steps <= 100); // Prevent infinite loops
        }
        
        // Should have executed at least 1 step
        prop_assert!(steps >= 1);
    }
    
    #[test]
    fn dry_run_does_not_modify_state(canvas in execution_graph_strategy()) {
        let canvas_before = canvas.clone();
        
        let mut engine = ExecutionEngine::new(ExecutionMode::DryRun);
        let _ = engine.execute(&canvas);
        
        // Canvas should be unchanged
        prop_assert_eq!(canvas.cell_count(), canvas_before.cell_count());
    }
    
    #[test]
    fn all_cells_in_execution_path_are_executed(canvas in execution_graph_strategy()) {
        let mut engine = ExecutionEngine::new(ExecutionMode::Run);
        let report = engine.execute(&canvas).unwrap();
        
        if report.status == ExecutionStatus::Complete {
            // Count reachable cells from start point
            let start = canvas.get_start_point().unwrap();
            let reachable = count_reachable_cells(&canvas, start.id);
            
            // All reachable cells should be executed
            prop_assert_eq!(report.total_cells_executed, reachable);
        }
    }
}

fn count_reachable_cells(canvas: &Canvas, start: Ulid) -> usize {
    let mut visited = HashSet::new();
    let mut queue = vec![start];
    
    while let Some(cell_id) = queue.pop() {
        if visited.contains(&cell_id) {
            continue;
        }
        visited.insert(cell_id);
        
        for rel in canvas.get_outgoing_relationships(cell_id) {
            queue.push(rel.to);
        }
    }
    
    visited.len()
}
```

### 4.3 Serialization Properties

**Verify save/load roundtrips**

```rust
// tests/property/serialization_properties.rs

use proptest::prelude::*;
use tempfile::TempDir;

proptest! {
    #[test]
    fn database_roundtrip_preserves_cells(canvas in small_canvas_strategy()) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.gcdb");
        
        // Save canvas
        let db = Database::create(&db_path).unwrap();
        for (cell_id, cell) in canvas.cells() {
            db.insert_cell(cell).unwrap();
        }
        
        // Load canvas
        let loaded_cells = db.load_all_cell_metadata().unwrap();
        
        // Should have same number of cells
        prop_assert_eq!(loaded_cells.len(), canvas.cell_count());
        
        // Each cell should match
        for (cell_id, cell) in canvas.cells() {
            let loaded = loaded_cells.get(cell_id).unwrap();
            prop_assert_eq!(cell.cell_type, loaded.cell_type);
            prop_assert_eq!(cell.bounds, loaded.bounds);
        }
    }
    
    #[test]
    fn export_import_roundtrip_preserves_graph(canvas in small_canvas_strategy()) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.gcdb");
        let export_path = temp_dir.path().join("export.gce");
        
        // Save canvas to database
        let db = Arc::new(Database::create(&db_path).unwrap());
        // ... save canvas ...
        
        // Export
        let exporter = ProjectExporter::new(db.clone());
        exporter.export_project(&export_path).unwrap();
        
        // Import to new location
        let import_dir = temp_dir.path().join("imported");
        fs::create_dir_all(&import_dir).unwrap();
        let imported_db_path = exporter.import_project(&export_path, &import_dir).unwrap();
        
        // Load imported canvas
        let imported_db = Database::open(&imported_db_path).unwrap();
        let imported_canvas = Canvas::load_from_db(Arc::new(imported_db)).unwrap();
        
        // Should match original
        prop_assert_eq!(imported_canvas.cell_count(), canvas.cell_count());
        prop_assert_eq!(imported_canvas.relationship_count(), canvas.relationship_count());
    }
    
    #[test]
    fn content_hash_detects_changes(content in "[a-z]{100,1000}") {
        let hash1 = hash_content(&content);
        let hash2 = hash_content(&content);
        
        // Same content = same hash
        prop_assert_eq!(hash1, hash2);
        
        // Different content = different hash
        let modified = format!("{}X", content);
        let hash3 = hash_content(&modified);
        prop_assert_ne!(hash1, hash3);
    }
}
```

### 4.4 Undo/Redo Properties

**Verify undo/redo invariants**

```rust
// tests/property/undo_redo_properties.rs

use proptest::prelude::*;

/// Generate a sequence of operations
#[derive(Debug, Clone)]
enum CanvasOperation {
    CreateCell(CellType, Rectangle, CellContent),
    DeleteCell(usize), // Index into existing cells
    ModifyCell(usize, String),
    SplitCell(usize, SplitDirection),
}

fn operation_strategy(max_cells: usize) -> impl Strategy<Value = CanvasOperation> {
    prop_oneof![
        // Create cell (always valid)
        (cell_type_strategy(), rectangle_strategy(), cell_content_strategy())
            .prop_map(|(t, r, c)| CanvasOperation::CreateCell(t, r, c)),
        
        // Delete/modify/split only if we have cells
        (0..max_cells).prop_map(CanvasOperation::DeleteCell),
        (0..max_cells, "[a-z]{10,50}").prop_map(|(i, s)| CanvasOperation::ModifyCell(i, s)),
        (0..max_cells, prop_oneof![Just(SplitDirection::Horizontal), Just(SplitDirection::Vertical)])
            .prop_map(|(i, d)| CanvasOperation::SplitCell(i, d)),
    ]
}

fn operation_sequence_strategy() -> impl Strategy<Value = Vec<CanvasOperation>> {
    prop::collection::vec(operation_strategy(10), 1..20)
}

proptest! {
    #[test]
    fn undo_redo_is_identity(operations in operation_sequence_strategy()) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.gcdb");
        let db = Arc::new(Database::create(&db_path).unwrap());
        let mut canvas = Canvas::new_with_db(db.clone());
        let mut undo_mgr = UndoManager::new(db.clone()).unwrap();
        
        let mut cell_ids = vec![];
        
        // Apply operations
        for op in &operations {
            match op {
                CanvasOperation::CreateCell(t, r, c) => {
                    let id = canvas.create_cell(*t, *r, c.clone());
                    cell_ids.push(id);
                }
                CanvasOperation::DeleteCell(i) if !cell_ids.is_empty() => {
                    let idx = i % cell_ids.len();
                    let _ = canvas.delete_cell(cell_ids[idx]);
                    cell_ids.remove(idx);
                }
                CanvasOperation::ModifyCell(i, content) if !cell_ids.is_empty() => {
                    let idx = i % cell_ids.len();
                    let _ = canvas.update_cell_content(cell_ids[idx], content.clone());
                }
                CanvasOperation::SplitCell(i, dir) if !cell_ids.is_empty() => {
                    let idx = i % cell_ids.len();
                    if let Ok((c1, c2)) = canvas.split_cell(cell_ids[idx], *dir, 0.5) {
                        cell_ids.push(c1);
                        cell_ids.push(c2);
                    }
                }
                _ => {}
            }
        }
        
        let state_after_ops = canvas.clone();
        
        // Undo all operations
        for _ in 0..operations.len() {
            undo_mgr.undo().ok();
        }
        
        // Redo all operations
        for _ in 0..operations.len() {
            undo_mgr.redo().ok();
        }
        
        // Reload canvas to see final state
        let canvas_after_redo = Canvas::load_from_db(db).unwrap();
        
        // State should match
        prop_assert_eq!(canvas_after_redo.cell_count(), state_after_ops.cell_count());
    }
    
    #[test]
    fn undo_is_inverse_of_operation(operations in operation_sequence_strategy()) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.gcdb");
        let db = Arc::new(Database::create(&db_path).unwrap());
        let mut canvas = Canvas::new_with_db(db.clone());
        let mut undo_mgr = UndoManager::new(db.clone()).unwrap();
        
        for op in operations {
            let state_before = canvas.clone();
            
            // Apply operation
            match op {
                CanvasOperation::CreateCell(t, r, c) => {
                    canvas.create_cell(t, r, c);
                }
                _ => continue, // Skip complex ops for this test
            }
            
            // Undo
            undo_mgr.undo().unwrap();
            
            // Reload
            let canvas_after_undo = Canvas::load_from_db(db.clone()).unwrap();
            
            // Should match state before operation
            prop_assert_eq!(canvas_after_undo.cell_count(), state_before.cell_count());
        }
    }
}
```

---

## 5. UI Testing Framework

### 5.1 UI Test Harness

**Create utilities for testing egui UI without rendering**

```rust
// tests/ui/ui_test_harness.rs

use egui::{Context, RawInput, ViewportId};
use crate::ui::GraphCellEditorApp;
use std::sync::Arc;

/// Test harness for UI testing without actual rendering
pub struct UiTestHarness {
    pub app: GraphCellEditorApp,
    pub ctx: Context,
    input_state: InputState,
}

/// Track input state for simulating user actions
#[derive(Default)]
struct InputState {
    mouse_pos: egui::Pos2,
    mouse_down: bool,
    keys_down: std::collections::HashSet<egui::Key>,
    modifiers: egui::Modifiers,
}

impl UiTestHarness {
    pub fn new() -> Self {
        Self {
            app: GraphCellEditorApp::new(),
            ctx: Context::default(),
            input_state: InputState::default(),
        }
    }
    
    pub fn from_project(project_path: &Path) -> Result<Self> {
        let project = Project::open(project_path)?;
        let app = GraphCellEditorApp::from_project(&project)?;
        
        Ok(Self {
            app,
            ctx: Context::default(),
            input_state: InputState::default(),
        })
    }
    
    /// Run one frame of the UI
    pub fn run_frame(&mut self) {
        let mut frame = create_test_frame();
        self.app.update(&self.ctx, &mut frame);
    }
    
    /// Simulate mouse move
    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.input_state.mouse_pos = egui::pos2(x, y);
        self.ctx.input_mut(|i| {
            i.events.push(egui::Event::PointerMoved(egui::pos2(x, y)));
        });
    }
    
    /// Simulate mouse click at current position
    pub fn mouse_click(&mut self, button: egui::PointerButton) {
        self.ctx.input_mut(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: self.input_state.mouse_pos,
                button,
                pressed: true,
                modifiers: self.input_state.modifiers,
            });
        });
        self.run_frame();
        
        self.ctx.input_mut(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: self.input_state.mouse_pos,
                button,
                pressed: false,
                modifiers: self.input_state.modifiers,
            });
        });
        self.run_frame();
    }
    
    /// Simulate mouse drag from start to end
    pub fn mouse_drag(&mut self, from: (f32, f32), to: (f32, f32)) {
        self.mouse_move(from.0, from.1);
        
        self.ctx.input_mut(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: self.input_state.mouse_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: self.input_state.modifiers,
            });
        });
        self.run_frame();
        
        // Simulate drag motion
        let steps = 10;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let x = from.0 + (to.0 - from.0) * t;
            let y = from.1 + (to.1 - from.1) * t;
            self.mouse_move(x, y);
            self.run_frame();
        }
        
        self.ctx.input_mut(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: self.input_state.mouse_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: self.input_state.modifiers,
            });
        });
        self.run_frame();
    }
    
    /// Simulate text input
    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.ctx.input_mut(|i| {
                i.events.push(egui::Event::Text(ch.to_string()));
            });
            self.run_frame();
        }
    }
    
    /// Simulate key press
    pub fn press_key(&mut self, key: egui::Key) {
        self.ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers: self.input_state.modifiers,
                physical_key: None,
            });
        });
        self.run_frame();
        
        self.ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key,
                pressed: false,
                repeat: false,
                modifiers: self.input_state.modifiers,
                physical_key: None,
            });
        });
        self.run_frame();
    }
    
    /// Set modifier keys (Ctrl, Shift, Alt)
    pub fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        self.input_state.modifiers = egui::Modifiers {
            ctrl,
            shift,
            alt,
            ..Default::default()
        };
    }
    
    /// Find a button by its text and click it
    pub fn click_button(&mut self, button_text: &str) -> Result<()> {
        // This requires inspecting the UI tree
        // For now, we'll simulate based on known button positions
        // In practice, you'd extend egui to support widget querying
        
        // Simplified: just run a frame and hope the button is there
        self.run_frame();
        
        // In real implementation, you'd need to:
        // 1. Traverse the UI tree
        // 2. Find widgets with matching text
        // 3. Get their screen positions
        // 4. Simulate click at that position
        
        Ok(())
    }
    
    /// Get current canvas state
    pub fn canvas(&self) -> &Canvas {
        &self.app.canvas
    }
    
    /// Get selected cell ID
    pub fn selected_cell(&self) -> Option<Ulid> {
        self.app.selected_cell
    }
    
    /// Get status message
    pub fn status_message(&self) -> &str {
        &self.app.status_message
    }
    
    /// Check if validation panel is showing errors
    pub fn has_validation_errors(&self) -> bool {
        !self.app.validation_issues.is_empty()
    }
}

fn create_test_frame() -> eframe::Frame {
    // Create a minimal frame for testing
    // This is a simplified version
    eframe::Frame {
        // ... minimal frame setup
    }
}
```

### 5.2 UI State Tracking

**Track all state changes during user interactions**

```rust
// tests/ui/state_tracker.rs

use std::collections::VecDeque;

/// Records all state changes during a test
#[derive(Default)]
pub struct StateTracker {
    events: VecDeque<StateEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateEvent {
    // UI State Changes
    CellSelected { cell_id: Option<Ulid> },
    StatusMessageChanged { message: String },
    ValidationIssuesChanged { count: usize },
    
    // Canvas State Changes
    CellCreated { cell_id: Ulid },
    CellDeleted { cell_id: Ulid },
    CellModified { cell_id: Ulid, field: String },
    CellSplit { parent_id: Ulid, children: Vec<Ulid> },
    CellMerged { merged_ids: Vec<Ulid>, new_id: Ulid },
    
    RelationshipCreated { from: Ulid, to: Ulid },
    RelationshipDeleted { from: Ulid, to: Ulid },
    
    // Database Side Effects
    DatabaseWrite { operation: String },
    ExternalFileCreated { path: PathBuf },
    ExternalFileModified { path: PathBuf },
    
    // Execution Events
    ExecutionStarted { mode: ExecutionMode },
    ExecutionCompleted { status: ExecutionStatus },
    CellExecuted { cell_id: Ulid, output: CellData },
}

impl StateTracker {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn record(&mut self, event: StateEvent) {
        self.events.push_back(event);
    }
    
    pub fn events(&self) -> &VecDeque<StateEvent> {
        &self.events
    }
    
    pub fn last_event(&self) -> Option<&StateEvent> {
        self.events.back()
    }
    
    pub fn find_event<F>(&self, predicate: F) -> Option<&StateEvent>
    where
        F: Fn(&StateEvent) -> bool,
    {
        self.events.iter().find(|e| predicate(e))
    }
    
    pub fn count_events<F>(&self, predicate: F) -> usize
    where
        F: Fn(&StateEvent) -> bool,
    {
        self.events.iter().filter(|e| predicate(e)).count()
    }
    
    pub fn clear(&mut self) {
        self.events.clear();
    }
    
    /// Get a sequence of events matching a pattern
    pub fn event_sequence(&self, pattern: &[StateEvent]) -> bool {
        if pattern.is_empty() {
            return true;
        }
        
        let events: Vec<_> = self.events.iter().collect();
        
        for window in events.windows(pattern.len()) {
            if window.iter().zip(pattern).all(|(a, b)| *a == b) {
                return true;
            }
        }
        
        false
    }
}

/// Wrapper around UiTestHarness with state tracking
pub struct TrackedUiTestHarness {
    harness: UiTestHarness,
    tracker: StateTracker,
}

impl TrackedUiTestHarness {
    pub fn new() -> Self {
        Self {
            harness: UiTestHarness::new(),
            tracker: StateTracker::new(),
        }
    }
    
    /// Run frame and record state changes
    pub fn run_frame(&mut self) {
        let before_state = self.capture_state();
        self.harness.run_frame();
        let after_state = self.capture_state();
        
        self.record_state_diff(&before_state, &after_state);
    }
    
    /// Mouse click with state tracking
    pub fn mouse_click(&mut self, button: egui::PointerButton) {
        self.harness.mouse_click(button);
        // State recording happens in run_frame
    }
    
    /// Access tracker
    pub fn tracker(&self) -> &StateTracker {
        &self.tracker
    }
    
    /// Access harness
    pub fn harness(&self) -> &UiTestHarness {
        &self.harness
    }
    
    fn capture_state(&self) -> AppState {
        AppState {
            selected_cell: self.harness.selected_cell(),
            cell_count: self.harness.canvas().cell_count(),
            relationship_count: self.harness.canvas().relationship_count(),
            status_message: self.harness.status_message().to_string(),
            validation_error_count: self.harness.has_validation_errors() as usize,
        }
    }
    
    fn record_state_diff(&mut self, before: &AppState, after: &AppState) {
        if before.selected_cell != after.selected_cell {
            self.tracker.record(StateEvent::CellSelected {
                cell_id: after.selected_cell,
            });
        }
        
        if before.status_message != after.status_message {
            self.tracker.record(StateEvent::StatusMessageChanged {
                message: after.status_message.clone(),
            });
        }
        
        if before.cell_count != after.cell_count {
            if after.cell_count > before.cell_count {
                // Cell(s) created
                self.tracker.record(StateEvent::CellCreated {
                    cell_id: Ulid::new(), // Would need to track actual ID
                });
            } else {
                // Cell(s) deleted
                self.tracker.record(StateEvent::CellDeleted {
                    cell_id: Ulid::new(),
                });
            }
        }
    }
}

#[derive(Clone)]
struct AppState {
    selected_cell: Option<Ulid>,
    cell_count: usize,
    relationship_count: usize,
    status_message: String,
    validation_error_count: usize,
}
```

### 5.3 UI Interaction Tests

**Test complete user interaction workflows**

```rust
// tests/ui/interaction_tests.rs

use super::ui_test_harness::*;
use super::state_tracker::*;

#[test]
fn test_create_cell_workflow() {
    let mut harness = TrackedUiTestHarness::new();
    
    // Initial state
    assert_eq!(harness.harness().canvas().cell_count(), 1); // Root cell
    
    // Click on canvas to create cell (simplified - assumes button exists)
    harness.mouse_click(egui::PointerButton::Primary);
    
    // Verify cell was created
    assert_eq!(harness.harness().canvas().cell_count(), 2);
    
    // Verify state tracking recorded the event
    assert!(harness.tracker().find_event(|e| matches!(e, StateEvent::CellCreated { .. })).is_some());
}

#[test]
fn test_split_cell_workflow() {
    let mut harness = TrackedUiTestHarness::new();
    
    // Get root cell
    let root_id = harness.harness().canvas().root_cell().unwrap();
    
    // Select cell (simulate click on cell)
    harness.mouse_click(egui::PointerButton::Primary);
    harness.run_frame();
    
    assert_eq!(harness.harness().selected_cell(), Some(root_id));
    
    // Click "Split H" button (simplified)
    // In real test, we'd find the button's position and click it
    harness.click_button("Split H").unwrap();
    
    // Verify split occurred
    let root_cell = harness.harness().canvas().get_cell(root_id).unwrap();
    assert_eq!(root_cell.children.len(), 2);
    
    // Verify state events
    assert!(harness.tracker().find_event(|e| {
        matches!(e, StateEvent::CellSplit { parent_id, .. } if *parent_id == root_id)
    }).is_some());
}

#[test]
fn test_create_relationship_workflow() {
    let mut harness = TrackedUiTestHarness::new();
    
    // Create two cells
    let cell1_id = harness.harness().canvas().create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(1)".to_string()),
    );
    
    let cell2_id = harness.harness().canvas().create_cell(
        CellType::Python,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(input_0)".to_string()),
    );
    
    // Click "Create Relationship" button
    harness.click_button("🔗 Create Relationship").unwrap();
    
    // Click on source cell
    harness.mouse_move(50.0, 50.0); // Center of cell1
    harness.mouse_click(egui::PointerButton::Primary);
    
    // Click on target cell
    harness.mouse_move(200.0, 50.0); // Center of cell2
    harness.mouse_click(egui::PointerButton::Primary);
    
    // Verify relationship created
    assert!(harness.harness().canvas().get_relationship(cell1_id, cell2_id).is_some());
    
    // Verify state event
    assert!(harness.tracker().find_event(|e| {
        matches!(e, StateEvent::RelationshipCreated { from, to } 
            if *from == cell1_id && *to == cell2_id)
    }).is_some());
}

#[test]
fn test_cell_resize_with_snapping() {
    let mut harness = TrackedUiTestHarness::new();
    
    let cell_id = harness.harness().canvas().root_cell().unwrap();
    let original_bounds = harness.harness().canvas().get_cell(cell_id).unwrap().bounds;
    
    // Select cell
    harness.mouse_move(200.0, 150.0); // Center
    harness.mouse_click(egui::PointerButton::Primary);
    
    // Drag resize handle (bottom-right corner)
    harness.mouse_drag((400.0, 300.0), (500.0, 400.0));
    
    // Verify cell was resized
    let new_bounds = harness.harness().canvas().get_cell(cell_id).unwrap().bounds;
    assert_ne!(original_bounds, new_bounds);
    assert!(new_bounds.width > original_bounds.width);
    assert!(new_bounds.height > original_bounds.height);
    
    // Verify state tracked the change
    assert!(harness.tracker().find_event(|e| {
        matches!(e, StateEvent::CellModified { field, .. } if field == "bounds")
    }).is_some());
}

#[test]
fn test_inline_edit_cell_content() {
    let mut harness = TrackedUiTestHarness::new();
    
    let cell_id = harness.harness().canvas().root_cell().unwrap();
    
    // Double-click to enter edit mode
    harness.mouse_move(200.0, 150.0);
    harness.mouse_click(egui::PointerButton::Primary);
    harness.mouse_click(egui::PointerButton::Primary);
    
    // Type new content
    harness.set_modifiers(true, false, false); // Ctrl
    harness.press_key(egui::Key::A); // Select all
    harness.set_modifiers(false, false, false);
    harness.type_text("New content");
    
    // Save with Ctrl+Enter
    harness.set_modifiers(true, false, false);
    harness.press_key(egui::Key::Enter);
    harness.set_modifiers(false, false, false);
    
    // Verify content changed
    let cell = harness.harness().canvas().get_cell(cell_id).unwrap();
    assert_eq!(cell.content.as_str(), Some("New content"));
    
    // Verify database was updated
    assert!(harness.tracker().find_event(|e| {
        matches!(e, StateEvent::DatabaseWrite { operation } if operation.contains("UPDATE cells"))
    }).is_some());
}

#[test]
fn test_execution_button_workflow() {
    let mut harness = TrackedUiTestHarness::new();
    
    // Setup executable graph
    let cell1 = harness.harness().canvas().create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(42)".to_string()),
    );
    
    harness.harness().canvas().set_start_point(cell1).unwrap();
    
    // Click "Run" button
    harness.click_button("▶ Run").unwrap();
    
    // Verify execution events
    assert!(harness.tracker().find_event(|e| {
        matches!(e, StateEvent::ExecutionStarted { mode: ExecutionMode::Run })
    }).is_some());
    
    assert!(harness.tracker().find_event(|e| {
        matches!(e, StateEvent::ExecutionCompleted { status: ExecutionStatus::Complete })
    }).is_some());
}

#[test]
fn test_validation_panel_shows_errors() {
    let mut harness = TrackedUiTestHarness::new();
    
    // Create invalid state (no start point)
    let cell1 = harness.harness().canvas().create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(1)".to_string()),
    );
    
    // Click validate button
    harness.click_button("✓ Validate").unwrap();
    
    // Verify validation errors shown
    assert!(harness.harness().has_validation_errors());
    
    // Verify state event
    assert!(harness.tracker().find_event(|e| {
        matches!(e, StateEvent::ValidationIssuesChanged { count } if *count > 0)
    }).is_some());
}
```

---

## 6. Integration Testing

### 6.1 Full Workflow Tests

**Test complete end-to-end scenarios**

```rust
// tests/integration/workflow_tests.rs

#[test]
fn test_full_document_creation_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path().join("test_project");
    
    // 1. Create new project
    let project = Project::create(&project_path).unwrap();
    let db = Arc::new(Database::open(&project.manifest_path()).unwrap());
    let mut canvas = Canvas::new_with_db(db.clone());
    
    // 2. Create document structure
    let title = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 800.0, 100.0),
        CellContent::Inline("# My Document".to_string()),
    );
    
    let intro = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 100.0, 800.0, 200.0),
        CellContent::Inline("This is the introduction...".to_string()),
    );
    
    let code = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 300.0, 800.0, 200.0),
        CellContent::Inline("# Example code\nprint('Hello')".to_string()),
    );
    
    // 3. Save project
    project.save(&canvas).unwrap();
    
    // 4. Close and reopen
    drop(canvas);
    drop(db);
    
    let project2 = Project::open(&project_path).unwrap();
    let (manifest, loaded_canvas) = project2.load().unwrap();
    
    // 5. Verify all cells preserved
    assert_eq!(loaded_canvas.cell_count(), 3);
    assert!(loaded_canvas.get_cell(title).is_some());
    assert!(loaded_canvas.get_cell(intro).is_some());
    assert!(loaded_canvas.get_cell(code).is_some());
    
    // 6. Verify Python cell is external
    let code_cell = loaded_canvas.get_cell(code).unwrap();
    assert_eq!(code_cell.content_location, ContentLocation::External);
}

#[test]
fn test_execution_workflow_with_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path().join("exec_test");
    
    let project = Project::create(&project_path).unwrap();
    let db = Arc::new(Database::open(&project.manifest_path()).unwrap());
    let mut canvas = Canvas::new_with_db(db.clone());
    
    // Create execution graph
    let cell1 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(10)".to_string()),
    );
    
    let cell2 = canvas.create_cell(
        CellType::Python,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(input_0 * 2)".to_string()),
    );
    
    canvas.create_relationship(cell1, cell2).unwrap();
    canvas.set_start_point(cell1).unwrap();
    
    // Execute
    let mut engine = ExecutionEngine::new(ExecutionMode::Run);
    let report = engine.execute(&canvas).unwrap();
    
    assert_eq!(report.status, ExecutionStatus::Complete);
    assert_eq!(report.log[1].output, CellData::Number(20.0));
    
    // Save execution trace
    engine.save_trace(&db, Some("Test execution".to_string())).unwrap();
    
    // Save project
    project.save(&canvas).unwrap();
    
    // Reload and verify execution trace exists
    let traces: Vec<Ulid> = db.conn.prepare(
        "SELECT id FROM execution_traces"
    ).unwrap()
    .query_map([], |row| {
        Ok(Ulid::from_string(&row.get::<_, String>(0)?).unwrap())
    }).unwrap()
    .collect::<Result<Vec<_>, _>>().unwrap();
    
    assert_eq!(traces.len(), 1);
}

#[test]
fn test_undo_redo_persistence_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path().join("undo_test");
    
    let project = Project::create(&project_path).unwrap();
    let db = Arc::new(Database::open(&project.manifest_path()).unwrap());
    let mut canvas = Canvas::new_with_db(db.clone());
    let mut undo_mgr = UndoManager::new(db.clone()).unwrap();
    
    // Perform operations
    let cell1 = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::Inline("First".to_string()),
    );
    
    canvas.update_cell_content(cell1, "Modified".to_string()).unwrap();
    
    let (child1, child2) = canvas.split_cell(cell1, SplitDirection::Horizontal, 0.5).unwrap();
    
    // Save
    project.save(&canvas).unwrap();
    
    // Undo last operation
    undo_mgr.undo().unwrap();
    
    // Reload canvas
    let canvas_after_undo = Canvas::load_from_db(db.clone()).unwrap();
    
    // Split should be undone
    assert!(canvas_after_undo.get_cell(child1).is_none());
    assert!(canvas_after_undo.get_cell(child2).is_none());
    
    // Redo
    undo_mgr.redo().unwrap();
    let canvas_after_redo = Canvas::load_from_db(db.clone()).unwrap();
    
    // Split should be back
    assert!(canvas_after_redo.get_cell(child1).is_some());
    assert!(canvas_after_redo.get_cell(child2).is_some());
}
```

---

## 7. Test Fixtures and Utilities

### 7.1 Common Test Fixtures

```rust
// tests/fixtures/sample_graphs.rs

/// Create a simple linear graph for testing
pub fn create_linear_graph(num_cells: usize) -> Canvas {
    let mut canvas = Canvas::new();
    let mut prev_cell = None;
    
    for i in 0..num_cells {
        let cell = canvas.create_cell(
            CellType::Python,
            Rectangle::new((i * 150) as f32, 0.0, 100.0, 100.0),
            CellContent::Inline(format!("set_output({})", i)),
        );
        
        if let Some(prev) = prev_cell {
            canvas.create_relationship(prev, cell).unwrap();
        } else {
            canvas.set_start_point(cell).unwrap();
        }
        
        prev_cell = Some(cell);
    }
    
    canvas
}

/// Create a diamond graph (fork and join)
pub fn create_diamond_graph() -> Canvas {
    let mut canvas = Canvas::new();
    
    let start = canvas.create_cell(
        CellType::Python,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::Inline("set_output(1)".to_string()),
    );
    
    let left = canvas.create_cell(
        CellType::Python,
        Rectangle::new(0.0, 150.0, 100.0, 100.0),
        CellContent::Inline("set_output(input_0 * 2)".to_string()),
    );
    
    let right = canvas.create_cell(
        CellType::Python,
        Rectangle::new(300.0, 150.0, 100.0, 100.0),
        CellContent::Inline("set_output(input_0 * 3)".to_string()),
    );
    
    let end = canvas.create_cell(
        CellType::Python,
        Rectangle::new(150.0, 300.0, 100.0, 100.0),
        CellContent::Inline("set_output(input_0 + input_1)".to_string()),
    );
    
    canvas.create_relationship(start, left).unwrap();
    canvas.create_relationship(start, right).unwrap();
    canvas.create_relationship(left, end).unwrap();
    canvas.create_relationship(right, end).unwrap();
    canvas.set_start_point(start).unwrap();
    
    canvas
}

/// Create a document-like structure
pub fn create_document_structure() -> Canvas {
    let mut canvas = Canvas::new();
    
    let root = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 800.0, 600.0),
        CellContent::Inline("Document Root".to_string()),
    );
    
    // Split into header and body
    let (header, body) = canvas.split_cell(root, SplitDirection::Horizontal, 0.15).unwrap();
    
    // Split body into content and footer
    let (content, footer) = canvas.split_cell(body, SplitDirection::Horizontal, 0.85).unwrap();
    
    // Split content into left and right
    let (left_panel, right_panel) = canvas.split_cell(content, SplitDirection::Vertical, 0.3).unwrap();
    
    canvas
}
```

### 7.2 Test Helpers

```rust
// tests/fixtures/test_helpers.rs

/// Assert two floats are approximately equal
pub fn assert_approx_eq(a: f32, b: f32, epsilon: f32) {
    assert!(
        (a - b).abs() < epsilon,
        "Expected {} ≈ {}, difference: {}",
        a, b, (a - b).abs()
    );
}

/// Assert canvas has expected structure
pub fn assert_canvas_structure(
    canvas: &Canvas,
    expected_cells: usize,
    expected_relationships: usize,
) {
    assert_eq!(canvas.cell_count(), expected_cells);
    assert_eq!(canvas.relationship_count(), expected_relationships);
}

/// Create a temporary project for testing
pub fn create_test_project() -> (TempDir, Project) {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path().join("test_project");
    let project = Project::create(&project_path).unwrap();
    (temp_dir, project)
}

/// Verify execution report
pub fn assert_execution_success(report: &ExecutionReport, expected_cells: usize) {
    assert_eq!(report.status, ExecutionStatus::Complete);
    assert_eq!(report.total_cells_executed, expected_cells);
    assert!(report.log.iter().all(|entry| entry.error.is_none()));
}
```

---

## 8. Testing Checklist

### Feature Completion Criteria

For each feature, verify:

- [ ] Unit tests pass for all components
- [ ] Property tests pass (if applicable)
- [ ] Integration tests pass
- [ ] UI interaction tests pass (if UI-related)
- [ ] State changes tracked correctly
- [ ] Database persistence works
- [ ] Undo/redo works (if applicable)
- [ ] No regressions in existing tests
- [ ] Code coverage meets threshold
- [ ] Manual testing confirms expected behavior

### Before Merging

- [ ] All tests pass locally
- [ ] No warnings in test output
- [ ] Test names are descriptive
- [ ] Test failures are easy to diagnose
- [ ] Tests run in reasonable time (< 30s for unit tests)
- [ ] No flaky tests

---

## 9. Running Tests

### Basic Commands

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_execution_determinism

# Run tests in specific module
cargo test execution::

# Run with output
cargo test -- --nocapture

# Run property tests (may take longer)
cargo test --release -- --include-ignored

# Run only fast tests
cargo test --lib
```

### Test Organization Commands

```bash
# Unit tests only
cargo test --lib

# Integration tests only
cargo test --test integration

# Property tests only
cargo test --test property

# UI tests only
cargo test --test ui
```

---

## 10. Summary

This testing methodology provides:

1. **Comprehensive Unit Tests** - For all core components
2. **Property-Based Tests** - For invariant verification
3. **UI Testing Framework** - For interaction testing without rendering
4. **State Tracking** - To verify all side effects
5. **Integration Tests** - For complete workflows
6. **Test Fixtures** - For common scenarios

### Key Innovations

- **egui testing without rendering** using Context alone
- **State tracking system** for comprehensive verification
- **Property-based execution testing** for correctness guarantees
- **Snapshot-based undo/redo verification**

### Testing Priority Matrix

```
┌─────────────────────┬──────────┬─────────────┬───────────┐
│ Component           │ Coverage │ Test Types  │ Priority  │
├─────────────────────┼──────────┼─────────────┼───────────┤
│ Execution Engine    │  95%+    │ Unit + Prop │ CRITICAL  │
│ Database            │  95%+    │ Unit + Prop │ CRITICAL  │
│ Canvas Operations   │  90%+    │ Unit + Prop │ HIGH      │
│ Snapshot/Undo       │  90%+    │ Unit + Prop │ HIGH      │
│ UI Interactions     │  80%+    │ UI Tests    │ HIGH      │
│ File Operations     │  85%+    │ Integration │ MEDIUM    │
│ Validation          │  80%+    │ Unit        │ MEDIUM    │
└─────────────────────┴──────────┴─────────────┴───────────┘
```

This methodology ensures that features are thoroughly tested and that regressions are caught early in development.