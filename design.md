# Graph Cell Editor - Design Document

## Project Overview

A desktop application for creating interactive, executable documents composed of cells arranged in a 2D canvas. Cells can contain different types of data (text, Python code, Markdown, JSON) and can influence adjacent cells through directed relationships, similar to cellular automata. The application combines TreeSheets-style cell splitting with graph-based execution semantics.

**Target Framework:** egui (Rust)
**MVP Scope:** Single canvas, single layer, Text and Python cell types, 2D splitting, Run/Step/Dry-Run execution modes, save/load support

---

## Core Concepts

### Two-Layer System

The application operates on two distinct but related layers:

#### Layer 1: Structural Adjacency
- **Purpose:** Visual layout and organization
- **Created by:** Cell split and merge operations
- **Semantics:** Parent-child relationships based on split order
- **Rendering:** Cell borders show spatial adjacency
- **Execution:** No execution semantics - purely organizational
- **No z-ordering:** Cells exist in a single flat layer

#### Layer 2: Data Flow Relationships
- **Purpose:** Execution graph for data propagation
- **Created by:** User explicitly creating relationships between cells
- **Semantics:** Directed edges indicating data flow (A→B means A provides input/output to B)
- **Rendering:** Arrows on cell borders showing flow direction
- **Execution:** These edges define the execution graph

**Key Insight:** Cells can be spatially adjacent without having a data flow relationship. Users might create complex "documents" with many cells that don't interact at execution time.

### Cell Types

Each cell has a type that determines:
- How its content is rendered
- How it executes (if executable)
- What input it accepts
- What output it produces

**MVP Types:**
- **Text:** Simple text display, no execution
- **Python:** Executable Python code with PyO3

**Future Types:**
- Markdown (rendered)
- JSON (pretty-printed, validated)
- CSV (tabular display)
- Image (PNG, SVG display)
- Visual Programming Blocks (node-based visual code)
- Custom display types (graphs, charts, etc.)

### Cell Operations

#### Split
- Divides one cell into two cells (horizontal or vertical)
- Original cell (A) becomes two new cells (A1, A2)
- A1 inherits the content and type from A
- A2 is empty but has the same type as A1
- Parent-child relationships created: A→A1, A→A2
- **No data flow relationships created automatically**
- UI may show "ghosted" suggested relationships based on previous relationships

#### Merge
- Combines multiple cells into one
- Destructive operation - merged cell is effectively new
- User must select which type the merged cell should be
- Adjacency edges are recalculated
- Data flow relationships are **not** preserved (user must recreate)

---

## Architecture

### Data Structures
```rust
use ulid::Ulid;
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cell {
    /// Unique identifier (sortable, timestamp-based)
    id: Ulid,

    /// Optional human-readable name for references
    name: Option<String>,

    /// Cell type determines behavior
    cell_type: CellType,

    /// Position and size on canvas (pixels)
    bounds: Rectangle,

    /// Cell content (inline or external reference)
    content: CellContent,

    /// Execution starting point flag
    is_start_point: bool,

    /// Parent cell if this was created by split
    parent: Option<Ulid>,

    /// Child cells if this was split
    children: Vec<Ulid>,

    /// Future: chunk ID for performance optimization
    chunk_id: Option<Ulid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Rectangle {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CellType {
    Text,
    Python,
    // Future: Markdown, Json, Csv, Image, VisualBlock, etc.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CellContent {
    /// Content stored directly in cell
    Inline(String),

    /// Reference to external file (uses memory-mapped I/O for large files)
    External {
        path: PathBuf,
        /// User-provided description/summary
        summary: String,
        /// Flag to use memory-mapped I/O for large files
        use_mmap: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Relationship {
    /// Source cell (data flows FROM this cell)
    from: Ulid,

    /// Destination cell (data flows TO this cell)
    to: Ulid,

    // Future: transformation functions, filters, etc.
}

#[derive(Debug, Serialize, Deserialize)]
struct Canvas {
    /// All cells indexed by ID
    cells: HashMap<Ulid, Cell>,

    /// Data flow relationships (execution graph)
    relationships: HashMap<(Ulid, Ulid), Relationship>,

    /// Optional: track the original/root cell
    root_cell: Option<Ulid>,

    // Future: spatial index (quadtree) for fast adjacency queries
}
```

### Large File Handling
```rust
use memmap2::Mmap;

struct ExternalFileHandle {
    path: PathBuf,
    mmap: Option<Mmap>,
    size: u64,
}

impl ExternalFileHandle {
    /// Open file with memory mapping for large files (>10MB)
    fn open(path: PathBuf) -> Result<Self> {
        let metadata = std::fs::metadata(&path)?;
        let size = metadata.len();

        let mmap = if size > 10_000_000 {
            // Use memory mapping for large files
            let file = std::fs::File::open(&path)?;
            Some(unsafe { Mmap::map(&file)? })
        } else {
            None
        };

        Ok(Self { path, mmap, size })
    }

    /// Scoped read of a region of the file
    fn read_range(&self, start: usize, length: usize) -> Result<&[u8]> {
        if let Some(mmap) = &self.mmap {
            Ok(&mmap[start..start + length])
        } else {
            // For small files, read directly
            // (In practice, cache this in memory)
            todo!("Implement direct read for small files")
        }
    }
}
```

### CellType Trait
```rust
trait CellTypeHandler {
    /// Render the cell content in the UI
    fn render(&self, content: &CellContent, ui: &mut egui::Ui);

    /// Execute the cell (if executable)
    fn execute(&self, content: &CellContent, inputs: Vec<CellData>) -> Result<CellData>;

    /// Validate content (syntax check, etc.)
    fn validate(&self, content: &CellContent) -> ValidationResult;

    /// Define acceptable input types
    fn accepts_input(&self, data_type: DataType) -> bool;

    /// Define output type(s)
    fn output_type(&self) -> DataType;
}
```

### Cell Data Types
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
enum CellData {
    None,
    Text(String),
    Number(f64),
    Json(serde_json::Value),
    Binary(Vec<u8>),
    // Future: more specific types
}

impl CellData {
    /// Attempt to coerce this data to match target type
    fn coerce_to(&self, target_type: &CellType) -> Result<CellData> {
        match (self, target_type) {
            (CellData::None, _) => Ok(CellData::None),
            (CellData::Text(s), CellType::Text) => Ok(self.clone()),
            (CellData::Json(v), CellType::Text) => Ok(CellData::Text(v.to_string())),
            (CellData::Number(n), CellType::Text) => Ok(CellData::Text(n.to_string())),
            (CellData::Binary(b), CellType::Text) => {
                // Output as hex representation with warning
                Ok(CellData::Text(hex::encode(b)))
            },
            // ... more coercion rules
            _ => Err("Type coercion not possible"),
        }
    }
}
```

### Event Sourcing
```rust
#[derive(Debug, Serialize, Deserialize)]
struct GraphEvent {
    timestamp: DateTime<Utc>,
    event: EventType,
}

#[derive(Debug, Serialize, Deserialize)]
enum EventType {
    CellCreated {
        id: Ulid,
        cell_type: CellType,
        bounds: Rectangle,
        name: Option<String>,
    },

    CellSplit {
        parent_id: Ulid,
        children: Vec<Ulid>,
        direction: SplitDirection,
        split_ratio: f32,
    },

    CellMerged {
        merged_ids: Vec<Ulid>,
        new_id: Ulid,
        new_type: CellType,
    },

    CellContentChanged {
        id: Ulid,
        new_content: CellContent,
    },

    CellTypeChanged {
        id: Ulid,
        old_type: CellType,
        new_type: CellType,
    },

    CellRenamed {
        id: Ulid,
        new_name: Option<String>,
    },

    RelationshipCreated {
        from: Ulid,
        to: Ulid,
    },

    RelationshipDeleted {
        from: Ulid,
        to: Ulid,
    },

    StartPointChanged {
        old_id: Option<Ulid>,
        new_id: Ulid,
    },

    // For future undo/redo and time-travel debugging
    SnapshotCreated {
        snapshot_id: Ulid,
        state_hash: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum SplitDirection {
    Horizontal,  // Top/Bottom
    Vertical,    // Left/Right
}
```

---

## Algorithms

### Cell Splitting
```rust
fn split_cell(
    canvas: &mut Canvas,
    cell_id: Ulid,
    direction: SplitDirection,
    split_ratio: f32,
) -> Result<(Ulid, Ulid)> {
    let cell = canvas.cells.get(&cell_id)
        .ok_or("Cell not found")?
        .clone();

    // Create new cells with new IDs
    let child1_id = Ulid::new();
    let child2_id = Ulid::new();

    // Calculate bounds for split cells
    let (bounds1, bounds2) = match direction {
        SplitDirection::Horizontal => {
            let split_y = cell.bounds.y + (cell.bounds.height * split_ratio);
            (
                Rectangle {
                    x: cell.bounds.x,
                    y: cell.bounds.y,
                    width: cell.bounds.width,
                    height: split_y - cell.bounds.y,
                },
                Rectangle {
                    x: cell.bounds.x,
                    y: split_y,
                    width: cell.bounds.width,
                    height: cell.bounds.y + cell.bounds.height - split_y,
                }
            )
        },
        SplitDirection::Vertical => {
            let split_x = cell.bounds.x + (cell.bounds.width * split_ratio);
            (
                Rectangle {
                    x: cell.bounds.x,
                    y: cell.bounds.y,
                    width: split_x - cell.bounds.x,
                    height: cell.bounds.height,
                },
                Rectangle {
                    x: split_x,
                    y: cell.bounds.y,
                    width: cell.bounds.x + cell.bounds.width - split_x,
                    height: cell.bounds.height,
                }
            )
        }
    };

    // Create child cells
    // Child 1 inherits content, Child 2 is empty
    let child1 = Cell {
        id: child1_id,
        name: None,  // Names must be re-assigned by user
        cell_type: cell.cell_type.clone(),
        bounds: bounds1,
        content: cell.content.clone(),  // Inherits content
        is_start_point: cell.is_start_point,
        parent: Some(cell_id),
        children: vec![],
        chunk_id: cell.chunk_id,
    };

    let child2 = Cell {
        id: child2_id,
        name: None,
        cell_type: cell.cell_type.clone(),
        bounds: bounds2,
        content: CellContent::Inline(String::new()),  // Empty
        is_start_point: false,
        parent: Some(cell_id),
        children: vec![],
        chunk_id: cell.chunk_id,
    };

    // Update parent cell's children list
    let mut parent_cell = cell;
    parent_cell.children = vec![child1_id, child2_id];

    // Insert cells
    canvas.cells.insert(child1_id, child1);
    canvas.cells.insert(child2_id, child2);
    canvas.cells.insert(cell_id, parent_cell);

    // Note: We do NOT automatically create relationships
    // The UI may suggest relationships based on previous state

    // Log event
    log_event(EventType::CellSplit {
        parent_id: cell_id,
        children: vec![child1_id, child2_id],
        direction,
        split_ratio,
    });

    Ok((child1_id, child2_id))
}
```

### Adjacency Detection
```rust
fn are_cells_adjacent(c1: &Rectangle, c2: &Rectangle) -> bool {
    // Check if rectangles share an edge (touching but not overlapping)

    // Horizontal adjacency (left/right)
    let horizontal_adjacent =
        (c1.x + c1.width == c2.x || c2.x + c2.width == c1.x) &&
        !(c1.y + c1.height <= c2.y || c2.y + c2.height <= c1.y);

    // Vertical adjacency (top/bottom)
    let vertical_adjacent =
        (c1.y + c1.height == c2.y || c2.y + c2.height == c1.y) &&
        !(c1.x + c1.width <= c2.x || c2.x + c2.width <= c1.x);

    horizontal_adjacent || vertical_adjacent
}

fn find_adjacent_cells(canvas: &Canvas, cell_id: Ulid) -> Vec<Ulid> {
    let cell = &canvas.cells[&cell_id];
    canvas.cells.iter()
        .filter(|(id, other)| {
            **id != cell_id && are_cells_adjacent(&cell.bounds, &other.bounds)
        })
        .map(|(id, _)| *id)
        .collect()
}
```

### Execution Algorithm
```rust
fn execute_graph(
    canvas: &mut Canvas,
    mode: ExecutionMode,
) -> Result<ExecutionReport> {
    // 1. Find start cell
    let start_cell_id = canvas.cells.values()
        .find(|c| c.is_start_point)
        .map(|c| c.id)
        .ok_or("No start point set")?;

    // 2. Initialize execution state
    let mut execution_queue = vec![start_cell_id];
    let mut executed_this_step = HashSet::new();
    let mut step_count = 0;
    let mut execution_log = Vec::new();
    let dry_run = matches!(mode, ExecutionMode::DryRun);

    while !execution_queue.is_empty() {
        step_count += 1;

        // Current step cells (sorted by ULID for deterministic order)
        let mut current_step_cells = execution_queue.clone();
        current_step_cells.sort();
        execution_queue.clear();
        executed_this_step.clear();

        for cell_id in current_step_cells {
            let cell = &canvas.cells[&cell_id];

            // 3. Gather inputs from upstream cells
            let inputs = gather_inputs(canvas, cell_id);

            // 4. Execute cell (or simulate in dry-run mode)
            let output = if dry_run {
                // In dry-run, validate without executing
                validate_cell_execution(cell, inputs)?
            } else {
                execute_cell(cell, inputs)?
            };

            execution_log.push(ExecutionLogEntry {
                step: step_count,
                cell_id,
                output: output.clone(),
                dry_run,
            });

            // 5. Find downstream cells
            let downstream = find_downstream_cells(canvas, cell_id);

            for target_id in downstream {
                // 6. Conflict detection
                if executed_this_step.contains(&target_id) {
                    return Err(format!(
                        "Conflict: Cell {} written twice in step {}",
                        target_id, step_count
                    ));
                }

                // 7. Type coercion
                let target_cell = &canvas.cells[&target_id];
                let coerced_output = if type_coercion_enabled() {
                    output.coerce_to(&target_cell.cell_type)?
                } else {
                    output.clone()
                };

                // 8. Write to target cell (skip in dry-run mode)
                if !dry_run {
                    write_to_cell(canvas, target_id, coerced_output)?;
                }
                executed_this_step.insert(target_id);

                // 9. Queue for next step
                if !execution_queue.contains(&target_id) {
                    execution_queue.push(target_id);
                }
            }
        }

        // 10. Step mode: pause for user input
        if mode == ExecutionMode::Step {
            // Return partial results, wait for user to click "Step" again
            return Ok(ExecutionReport {
                status: ExecutionStatus::Paused,
                step: step_count,
                log: execution_log,
            });
        }
    }

    Ok(ExecutionReport {
        status: if dry_run {
            ExecutionStatus::DryRunComplete
        } else {
            ExecutionStatus::Complete
        },
        step: step_count,
        log: execution_log,
    })
}

fn find_downstream_cells(canvas: &Canvas, cell_id: Ulid) -> Vec<Ulid> {
    canvas.relationships.iter()
        .filter(|((from, _), _)| *from == cell_id)
        .map(|((_, to), _)| *to)
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum ExecutionMode {
    Run,     // Execute until completion
    Step,    // Execute one step, then pause
    DryRun,  // Validate without executing (test mode)
}

#[derive(Debug)]
enum ExecutionStatus {
    Complete,
    Paused,
    DryRunComplete,
    Error(String),
}
```

### Python Cell Execution
```rust
fn execute_python_cell(
    cell: &Cell,
    canvas: &Canvas,
    inputs: Vec<CellData>,
) -> Result<CellData> {
    let code = match &cell.content {
        CellContent::Inline(s) => s.clone(),
        CellContent::External { path, use_mmap, .. } => {
            if *use_mmap {
                // Read using memory-mapped file
                read_mmap_file(path)?
            } else {
                std::fs::read_to_string(path)?
            }
        }
    };

    Python::with_gil(|py| {
        // 1. Setup cell import system
        setup_cell_importer(py, canvas)?;

        // 2. Create a module for this cell
        let module = PyModule::new(py, &format!("cell_{}", cell.id))?;

        // 3. Inject inputs as variables
        for (i, input) in inputs.iter().enumerate() {
            let py_value = celldata_to_python(py, input)?;
            module.add(&format!("input_{}", i), py_value)?;
        }

        // 4. Create output storage (key-value store or queue)
        let output_store = PyDict::new(py);
        module.add("__output__", output_store)?;

        // Add helper function for setting output
        module.add("set_output", wrap_pyfunction!(set_output_fn, module)?)?;

        // 5. Execute code
        py.run(&code, Some(module.dict()), None)
            .map_err(|e| format!("Python execution error: {}", e))?;

        // 6. Extract output from store
        let output = output_store.get_item("result")
            .or_else(|| output_store.get_item("output"))
            .or_else(|| output_store.get_item("value"));

        match output {
            Some(val) => {
                // Try pickle/jsonpickle for complex Python types
                python_to_celldata_with_pickle(val)
            },
            None => Ok(CellData::None),
        }
    })
}

#[pyfunction]
fn set_output_fn(key: &str, value: &PyAny) -> PyResult<()> {
    // Store in __output__ dict
    let locals = Python::with_gil(|py| py.eval("locals()", None, None))?;
    let output_store = locals.get_item("__output__")?;
    output_store.set_item(key, value)?;
    Ok(())
}

fn setup_cell_importer(py: Python, canvas: &Canvas) -> PyResult<()> {
    // Custom import hook for "from cell:Name import ..."
    let import_code = r#"
import sys
from importlib.abc import MetaPathFinder, Loader
from importlib.machinery import ModuleSpec

class CellImporter(MetaPathFinder, Loader):
    def __init__(self, cell_registry):
        self.cells = cell_registry

    def find_spec(self, fullname, path, target=None):
        if fullname.startswith('cell:'):
            cell_name = fullname[5:]  # Remove 'cell:' prefix
            if cell_name in self.cells:
                return ModuleSpec(fullname, self)
        return None

    def create_module(self, spec):
        return None  # Use default module creation

    def exec_module(self, module):
        cell_name = module.__name__[5:]
        cell_code = self.cells[cell_name]
        exec(cell_code, module.__dict__)

# Registry will be populated by Rust
cell_registry = {}
sys.meta_path.insert(0, CellImporter(cell_registry))
"#;

    py.run(import_code, None, None)?;

    // Populate cell registry
    let locals = py.eval("locals()", None, None)?;
    let cell_registry = locals.get_item("cell_registry")?;

    for cell in canvas.cells.values() {
        if let Some(name) = &cell.name {
            if cell.cell_type == CellType::Python {
                let code = match &cell.content {
                    CellContent::Inline(s) => s.clone(),
                    CellContent::External { path, use_mmap, .. } => {
                        if *use_mmap {
                            read_mmap_file(path).unwrap_or_default()
                        } else {
                            std::fs::read_to_string(path).unwrap_or_default()
                        }
                    }
                };
                cell_registry.set_item(name.as_str(), code)?;
            }
        }
    }

    Ok(())
}
```

---

## Validation

### Validation Types
```rust
#[derive(Debug, Clone)]
enum ValidationResult {
    Ok,
    Warnings(Vec<Warning>),
    Errors(Vec<Error>),
}

#[derive(Debug, Clone)]
struct Warning {
    severity: WarningSeverity,
    message: String,
    affected_cells: Vec<Ulid>,
}

#[derive(Debug, Clone)]
enum WarningSeverity {
    Info,     // Blue - informational
    Warning,  // Yellow - potential issue
}

#[derive(Debug, Clone)]
struct Error {
    message: String,
    affected_cells: Vec<Ulid>,
}
```

### Validation Checks

**MVP Validations:**

1. **Cycle Detection (Warning)**
   - Detect cycles in relationship graph using DFS
   - Cycles are allowed but must be user-confirmed
   - Display warning with affected cells highlighted

2. **Type Compatibility (Warning/Error)**
   - For each relationship A→B, check if A's output type is compatible with B's input type
   - With type coercion enabled: warn if coercion might lose data
   - With type coercion disabled: error if types don't match exactly
   - Special warning for Binary→Text coercion (hex representation)

3. **Missing Start Point (Error)**
   - If user tries to execute but no start point is set
   - Blocks execution

4. **Python Syntax (Error)**
   - Validate Python code syntax
   - Highlight syntax errors in cell

**Future Validations:**

5. **Static Conflict Detection**
   - Analyze graph for cells with multiple parents that would execute in same step
   - Warn user before execution

6. **Orphan Detection (Info)**
   - Find cells unreachable from start point
   - Inform user these cells won't execute

7. **Reference Validation**
   - Check that `from cell:Name` references exist
   - Check that referenced cells are Python type

### Validation Triggers

Run validation after:
- Cell split
- Cell merge
- Relationship created/deleted
- Cell type changed
- Cell content changed (Python syntax only)
- Start point changed

Display results in:
- Status panel (list of warnings/errors)
- Cell highlighting (yellow for warnings, red for errors)
- Relationship highlighting (highlight problematic edges)

---

## User Interface

### Main Window Layout
```
┌─────────────────────────────────────────────────────────────┐
│ Menu: File | Edit | View | Execute | Help                    │
├─────────────────────────────────────────────────────────────┤
│ Toolbar: [Split H][Split V][Merge][Relation][Run][Step][Dry]│
├────────────────────────┬────────────────────────────────────┤
│                        │                                    │
│                        │                                    │
│   Canvas               │   Properties Panel                 │
│   (scrollable,         │   ┌────────────────────────────┐ │
│    zoomable)           │   │ Cell: 01HGXY1234           │ │
│                        │   │ Name: DataLoader           │ │
│   ┌──────┐  ┌──────┐  │   │ Type: Python               │ │
│   │Cell A│→ │Cell B│  │   │ Start Point: [x]           │ │
│   │Python│  │Text  │  │   │                            │ │
│   └──────┘  └──────┘  │   │ Content:                   │ │
│      ↓                 │   │ [Edit] [External File...]  │ │
│   ┌──────┐            │   └────────────────────────────┘ │
│   │Cell C│            │                                    │
│   │JSON  │            │   Validation Results               │
│   └──────┘            │   ⚠ Type mismatch: Cell A→B       │
│                        │   ⚠ Binary data: use specific type│
│                        │   ℹ Cell D unreachable            │
│                        │                                    │
├────────────────────────┴────────────────────────────────────┤
│ Status: Ready | 23 cells | 15 relationships | Dry-run: Pass│
└─────────────────────────────────────────────────────────────┘
```

### Canvas Interactions

**Mouse:**
- Left click: Select cell
- Double click: Edit cell content (inline editor)
- Right click: Context menu (Split, Merge, Delete, etc.)
- Drag: Pan canvas
- Scroll: Zoom in/out
- Shift + Click on border: Create/toggle relationship

**Relationship Creation:**
1. User clicks "Relation" button or presses 'R'
2. Click on source cell
3. Click on border of target cell
4. Arrow appears showing A→B relationship
5. Click arrow to reverse direction or right-click to delete

**Cell Splitting:**
1. Select cell
2. Click "Split H" or "Split V" (or keyboard shortcut)
3. Optional: drag split line to adjust ratio
4. Confirm split
5. UI shows ghosted suggested relationships based on previous state

**Execution Modes:**
- **Run:** Execute entire graph until completion
- **Step:** Execute one step at a time with pause
- **Dry-Run:** Validate execution without modifying cells (test for errors)

### Visual Design

**Cell Appearance:**
- Border color indicates type (blue=text, green=python, etc.)
- Border thickness indicates selection state
- Yellow highlight for warnings
- Red highlight for errors
- Orange highlight for binary data warnings
- Start point cell has special marker (star icon in corner)
- Single flat layer (no z-ordering)

**Relationship Arrows:**
- Drawn on cell borders
- Arrowhead shows direction
- Thickness/color indicates state (normal, selected, error)
- Hovering shows tooltip with type information

**Validation Feedback:**
- Real-time highlighting as user edits
- Status panel shows all issues
- Click on issue to jump to affected cells
- Dry-run results shown in status bar

---

## File Format

### Project Structure
```
my_project/
├── manifest.json          # Project metadata
├── events.jsonl           # Event log (one JSON object per line)
├── cells.json             # All cell data and relationships
├── snapshots/             # State snapshots for undo/redo (future)
│   ├── snapshot_01HGXY.json
│   └── snapshot_01HGXZ.json
└── external/              # External files referenced by cells
    ├── data.csv
    └── script.py
```

### manifest.json
```json
{
  "version": "0.1.0",
  "created": "2025-12-06T10:00:00Z",
  "modified": "2025-12-06T15:30:00Z",
  "start_cell": "01HGXY1234ABC"
}
```

### events.jsonl
```jsonl
{"timestamp":"2025-12-06T10:00:00Z","event":{"CellCreated":{"id":"01HGXY1234ABC","cell_type":"Python","bounds":{"x":0,"y":0,"width":200,"height":100},"name":"DataLoader"}}}
{"timestamp":"2025-12-06T10:01:00Z","event":{"CellSplit":{"parent_id":"01HGXY1234ABC","children":["01HGXY5678DEF","01HGXY9ABC123"],"direction":"Horizontal","split_ratio":0.5}}}
{"timestamp":"2025-12-06T10:02:00Z","event":{"RelationshipCreated":{"from":"01HGXY5678DEF","to":"01HGXY9ABC123"}}}
{"timestamp":"2025-12-06T10:05:00Z","event":{"SnapshotCreated":{"snapshot_id":"01HGXYA1234","state_hash":"abc123def456"}}}
```

### cells.json
```json
{
  "cells": [
    {
      "id": "01HGXY1234ABC",
      "name": "DataLoader",
      "cell_type": "Python",
      "bounds": {"x": 0, "y": 0, "width": 200, "height": 100},
      "content": {
        "Inline": "def load_data():\n    return [1, 2, 3]"
      },
      "is_start_point": true,
      "parent": null,
      "children": [],
      "chunk_id": null
    },
    {
      "id": "01HGXY5678DEF",
      "name": "Display",
      "cell_type": "Text",
      "bounds": {"x": 250, "y": 0, "width": 200, "height": 100},
      "content": {
        "External": {
          "path": "external/output.txt",
          "summary": "Processing results",
          "use_mmap": false
        }
      },
      "is_start_point": false,
      "parent": null,
      "children": [],
      "chunk_id": null
    }
  ],
  "relationships": [
    {
      "from": "01HGXY1234ABC",
      "to": "01HGXY5678DEF"
    }
  ]
}
```

---

## Implementation Phases

### Phase 1: Core Data Model
1. Define all core structs (Cell, Relationship, Canvas, etc.)
2. Implement Cell CRUD operations
3. Implement split algorithm (horizontal and vertical)
4. Implement merge algorithm
5. Implement relationship CRUD
6. Implement adjacency detection
7. Write comprehensive unit tests for all graph operations

### Phase 2: Serialization
8. Implement save to JSON (cells.json, manifest.json)
9. Implement load from JSON
10. Implement event logging to JSONL with timestamps
11. Test save/load round-trips with complex graphs
12. Add error handling for corrupted files
13. Implement memory-mapped file reading for large external files

### Phase 3: UI Shell
14. Setup egui application structure
15. Implement canvas rendering (cells as rectangles with borders, single layer)
16. Implement mouse interaction (select, pan, zoom)
17. Implement split UI (buttons and mouse controls)
18. Implement relationship visualization (arrows on borders)
19. Implement relationship creation UI (click source, click target)
20. Implement properties panel (cell details, edit metadata)
21. Implement status panel (validation results)

### Phase 4: Execution Engine
22. Implement Text cell type (display only, no execution)
23. Implement Python cell type (PyO3 integration)
24. Implement execution algorithm (Run mode)
25. Implement Step mode (pause between steps)
26. Implement Dry-Run mode (validation without execution)
27. Implement error display in UI
28. Add execution logging and visualization

### Phase 5: Validation & Polish
29. Implement type compatibility validation
30. Implement cycle detection
31. Implement cell naming system (UI for setting names)
32. Implement cell references in Python (`from cell:Name`)
33. Implement validation highlighting in UI
34. Add comprehensive error messages
35. Performance testing with large graphs

### Phase 6: Future Enhancements (Roadmap)

**Near-term:**
- Additional cell types (Markdown, JSON, CSV, Image)
- Visual Programming Block cell type
- Chunking system for performance
- Scoped reads/writes for large files

**Medium-term:**
- Undo/redo system using event log and snapshots
- Time-travel debugging (replay execution from any point)
- Reactive execution mode (trigger on cell change)
- Static conflict detection
- Cell grouping for organization
- Plugin system for custom cell types

**Long-term:**
- Import/export to various formats (Jupyter, Markdown, HTML, etc.)
- Multi-user collaboration with operational transforms/CRDTs
- WASM/sandboxing for secure Python execution
- Advanced visual programming features
- Mobile/web versions

---

## Example Use Cases

### Example 1: Data Pipeline
```
[CSV Cell] → [Python Cell] → [Graph Cell]
data.csv    Process data     Display chart
```
Python code:
```python
import pandas as pd

# Input comes from adjacent CSV cell
df = pd.read_csv(input_0)  # input_0 is from relationship

# Process
result = df.groupby('category').sum()

# Output to next cell
set_output('result', result.to_json())
```

### Example 2: Documentation with Live Code
```
[Markdown]  [Python]  [Markdown]
  Intro     Example    Results
             Code

The Markdown cells contain explanatory text.
The Python cell demonstrates a concept with live execution.
Results are fed back into documentation.
```

### Example 3: Simulation (Chip-8 Emulator)
```
[Display Cell: 64x32 pixels]
         ↑
    [CPU Cell]
    ↙    ↓    ↘
[RAM] [Registers] [Stack]

- Display cell shows pixel grid
- CPU cell executes instruction cycle
- Cycle creates feedback loops
- User confirms cycle is intentional
- Step mode allows debugging instruction-by-instruction
- Chunking allows efficient simulation of large systems
```

### Example 4: Complex Document
```
[Title]     [Author]    [Date]
[Abstract - spans 3 columns]
[Intro]     [Method]    [Results]
[Graph]     [Analysis]  [Conclusion]

No relationships - purely organizational.
Each cell contains text/markdown.
User can export as PDF or HTML (future feature).
All cells in single flat layer.
```

### Example 5: Visual Programming + Python
```
[Visual Block]  →  [Python]  →  [Display]
   Data Flow       Validation     Results

Visual programming blocks for logic flow.
Python for complex calculations.
Display for output visualization.
```

---

## Technical Notes

### Type Coercion Matrix

| From ↓ / To → | Text | Python | JSON | Binary | Future Types |
|---------------|------|--------|------|--------|--------------|
| None | "" | None | null | [] | (type-specific) |
| Text | ✓ | str | Parse or wrap | encode | (convert) |
| Number | str | float | number | bytes | (convert) |
| Json | str | dict | ✓ | serialize | (convert) |
| Binary | hex⚠ | bytes | base64 | ✓ | (convert) |

⚠ Binary→Text outputs hex with warning to specify proper type

### Performance Considerations

- **Large graphs:** Use spatial indexing (quadtree) for fast adjacency queries
- **Chunking:** Execute max 4096 cells per step (configurable)
- **Large files:** Use memory-mapped I/O for files >10MB
- **Scoped reads:** Read only necessary portions of large files
- **Lazy loading:** Only load cell content when needed
- **Caching:** Cache execution results when possible
- **PyO3 overhead:** Consider subprocess isolation for production

### Security Considerations

- **Sandboxing:** Python code runs in same process (MVP)
  - Future: use WASM, containers, or subprocesses

- **File system access:** Python can access host file system
  - Future: restrict to project directory

- **Network access:** Python can make network requests
  - Future: add permission system

---

## Dependencies

```toml
[dependencies]
egui = "0.29"
eframe = "0.29"
ulid = "1.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
pyo3 = { version = "0.22", features = ["auto-initialize"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1.0"
memmap2 = "0.9"
hex = "0.4"

# For pickle support (Python object serialization)
serde-pickle = "1.1"
```

---

## Roadmap

### MVP (Phase 1-5)

- ✓ Core graph data model
- ✓ Two-layer system (structure + data flow)
- ✓ Text and Python cell types
- ✓ 2D splitting/merging
- ✓ Run/Step/Dry-Run execution modes
- ✓ Memory-mapped files for large data
- ✓ Save/load with event logging
- ✓ Basic validation

### Phase 6: Enhanced Features

- Additional cell types (Markdown, JSON, CSV, Image)
- Visual Programming Block cell type
- Performance optimizations (chunking, spatial indexing)
- Enhanced validation (static conflict detection)

### Phase 7: Time-Travel & History

- Undo/redo using event log
- State snapshots
- Time-travel debugging
- Replay execution from any point

### Phase 8: Extensibility

- Plugin system for custom cell types
- Import/export various formats
- Custom validation rules
- Custom execution triggers

### Phase 9: Collaboration

- Multi-user editing
- Operational transforms or CRDTs
- Conflict resolution
- Real-time synchronization

### Phase 10: Advanced Features

- WASM sandboxing
- Advanced visual programming
- Mobile/web versions
- Cloud sync and sharing
