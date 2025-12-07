# Graph Cell Editor Examples

This directory contains example code and projects demonstrating various features of the Graph Cell Editor.

## Running Examples

To run an example:

```bash
cargo run --example simple_workflow
```

## Available Examples

### 1. simple_workflow.rs

A comprehensive example demonstrating the complete workflow:

- Creating a canvas with multiple cells
- Setting cell types (Python, Text)
- Creating data flow relationships
- Validating the workflow
- Executing Python cells
- Saving and loading projects

**What it does:**
- Creates an "Input" cell that sets a value
- Creates a "Process" cell that doubles the value
- Creates a "Summary" cell that displays both values
- Establishes data flow relationships
- Validates and executes the workflow
- Saves the project to `/tmp/simple_workflow`

**To run:**
```bash
cargo run --example simple_workflow
```

## Example Workflows

### Basic Data Processing

```rust
use graph_cell_editor::*;

// Create canvas
let mut canvas = Canvas::with_root_cell(
    CellType::Python,
    Rectangle::new(0.0, 0.0, 400.0, 300.0),
    CellContent::inline("data = [1, 2, 3, 4, 5]")
);

// Set as start point
let root = canvas.get_root_cell().unwrap().id;
canvas.set_start_point(root)?;
```

### Cell Splitting

```rust
// Split horizontally
let (left, right) = canvas.split_cell(cell_id, SplitDirection::Horizontal, 0.5)?;

// Split vertically
let (top, bottom) = canvas.split_cell(cell_id, SplitDirection::Vertical, 0.5)?;
```

### Creating Relationships

```rust
// Create data flow from source to target
canvas.create_relationship(source_id, target_id)?;

// Check relationships
let outgoing = canvas.get_outgoing_relationships(cell_id);
let incoming = canvas.get_incoming_relationships(cell_id);
```

### Validation

```rust
use graph_cell_editor::validation::ValidatedCanvas;

let result = canvas.validate();

// Check for errors
if result.has_errors() {
    for error in result.errors() {
        println!("Error: {}", error.message);
    }
}

// Check for warnings
if result.has_warnings() {
    for warning in result.warnings() {
        println!("Warning: {}", warning.message);
    }
}

// Get cells with issues
let issues = canvas.cells_with_issues(&result);
for (cell_id, severity) in issues {
    println!("Cell {} has {:?} severity issue", cell_id, severity);
}
```

### Execution

```rust
use graph_cell_editor::ExecutionMode;

// Run mode - execute all cells
let mut engine = ExecutionEngine::new(ExecutionMode::Run);
let report = engine.execute(&canvas)?;

// Step mode - pause between cells
let mut engine = ExecutionEngine::new(ExecutionMode::Step);
let report = engine.execute(&canvas)?;

// Dry-run mode - validate only
let mut engine = ExecutionEngine::new(ExecutionMode::DryRun);
let report = engine.execute(&canvas)?;

// Check execution results
match report.status {
    ExecutionStatus::Complete => println!("Success!"),
    ExecutionStatus::Paused => println!("Paused at step {}", report.step),
    ExecutionStatus::Error(e) => println!("Error: {}", e),
    _ => {}
}
```

### Python Cell Examples

#### Simple Calculation
```python
# Cell: Calculate
x = 10
y = 20
result = x + y
print(f"Sum: {result}")
```

#### Data Processing
```python
# Cell: ProcessData
data = [1, 2, 3, 4, 5]
squared = [x**2 for x in data]
total = sum(squared)
print(f"Sum of squares: {total}")
```

#### Cell References
```python
# Cell: UseResults
# Assumes there's a cell named "Calculate"
previous_result = cell:Calculate
doubled = previous_result * 2
print(f"Doubled: {doubled}")
```

## Common Patterns

### Creating a Linear Pipeline

```rust
// Create start cell
let mut canvas = Canvas::with_root_cell(/*...*/);
let start = canvas.root_cell().unwrap();
canvas.set_start_point(start)?;

// Split into pipeline stages
let (stage1, stage2) = canvas.split_cell(start, SplitDirection::Horizontal, 0.33)?;
let (stage2, stage3) = canvas.split_cell(stage2, SplitDirection::Horizontal, 0.5)?;

// Connect pipeline
canvas.create_relationship(start, stage1)?;
canvas.create_relationship(stage1, stage2)?;
canvas.create_relationship(stage2, stage3)?;
```

### Creating a Branching Workflow

```rust
// Start
let start = /*...*/;

// Create two branches
let (branch_a, branch_b) = canvas.split_cell(start, SplitDirection::Horizontal, 0.5)?;

// Both branches process data from start
canvas.create_relationship(start, branch_a)?;
canvas.create_relationship(start, branch_b)?;

// Create merge point
let (merge, _) = canvas.split_cell(branch_b, SplitDirection::Vertical, 0.5)?;
canvas.create_relationship(branch_a, merge)?;
canvas.create_relationship(branch_b, merge)?;
```

### Error Handling

```rust
use anyhow::Result;

fn create_workflow() -> Result<Canvas> {
    let mut canvas = Canvas::with_root_cell(/*...*/);

    // Operations that can fail
    let root_id = canvas.root_cell()
        .ok_or_else(|| anyhow::anyhow!("No root cell"))?;

    canvas.set_start_point(root_id)?;
    let (a, b) = canvas.split_cell(root_id, SplitDirection::Horizontal, 0.5)?;
    canvas.create_relationship(a, b)?;

    // Validate before returning
    let result = canvas.validate();
    if result.has_errors() {
        return Err(anyhow::anyhow!("Validation failed"));
    }

    Ok(canvas)
}
```

## Validation Scenarios

### Detecting Cycles

```rust
// Create a cycle (A -> B -> C -> A)
canvas.create_relationship(a, b)?;
canvas.create_relationship(b, c)?;
canvas.create_relationship(c, a)?; // Creates cycle

let result = canvas.validate();
// Will have warnings about cycles
assert!(result.has_warnings());
```

### Finding Orphans

```rust
// Cell without path from start point
let orphan = canvas.create_cell(/*...*/)?;
// Don't connect it to the graph

let result = canvas.validate();
// Will have info about orphaned cells
for info in result.info() {
    if info.issue_type == ValidationIssueType::OrphanCell {
        println!("Orphan detected: {}", info.message);
    }
}
```

### Missing Start Point

```rust
let mut canvas = Canvas::new();
// Don't set a start point

let result = canvas.validate();
// Will have error about missing start point
assert!(result.has_errors());
```

## Project Management

### Creating a Project

```rust
use std::path::PathBuf;

let path = PathBuf::from("/path/to/project");
let project = Project::create(&path)?;

// Save canvas
project.save(&canvas)?;
```

### Loading a Project

```rust
let project = Project::open(&path)?;
let (manifest, canvas) = project.load()?;

println!("Project: {}", manifest.project_name);
println!("Cells: {}", canvas.cell_count());
```

### Updating a Project

```rust
let project = Project::open(&path)?;
let (_, mut canvas) = project.load()?;

// Modify canvas
canvas.create_cell(/*...*/)?;

// Save changes
project.save(&canvas)?;
```

## Tips and Best Practices

1. **Always set a start point** before execution
   ```rust
   canvas.set_start_point(start_cell_id)?;
   ```

2. **Validate before executing**
   ```rust
   let result = canvas.validate();
   if !result.is_valid() {
       return Err(anyhow::anyhow!("Validation failed"));
   }
   ```

3. **Use meaningful cell names**
   ```rust
   canvas.rename_cell(cell_id, Some("DataLoader".to_string()))?;
   ```

4. **Check execution reports**
   ```rust
   let report = engine.execute(&canvas)?;
   for entry in report.log {
       if let Some(error) = entry.error {
           println!("Cell {} failed: {}", entry.cell_id, error);
       }
   }
   ```

5. **Handle errors gracefully**
   ```rust
   match canvas.split_cell(id, direction, ratio) {
       Ok((child1, child2)) => { /* use children */ },
       Err(e) => { /* handle error */ },
   }
   ```

## Further Reading

- See [design.md](../design.md) for architecture details
- See [README.md](../README.md) for full documentation
- Run `cargo doc --open` for API documentation
