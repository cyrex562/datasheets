/// Example: Creating a simple computational workflow
///
/// This example demonstrates:
/// - Creating a canvas with cells
/// - Setting cell types and content
/// - Creating relationships between cells
/// - Validating the workflow
/// - Executing the workflow
/// - Saving and loading the project

use graph_cell_editor::*;
use anyhow::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    println!("=== Graph Cell Editor: Simple Workflow Example ===\n");

    // Step 1: Create a canvas with a root cell
    println!("Step 1: Creating canvas...");
    let mut canvas = Canvas::with_root_cell(
        CellType::Python,
        Rectangle::new(100.0, 100.0, 300.0, 200.0),
        CellContent::inline("# Input cell\nvalue = 42\nprint(f'Initial value: {value}')"),
    );

    // Mark the root as start point
    let root_id = canvas.root_cell().unwrap();
    canvas.set_start_point(root_id)?;
    canvas.rename_cell(root_id, Some("Input".to_string()))?;
    println!("  ✓ Created root cell 'Input'");

    // Step 2: Split the root cell to create more cells
    println!("\nStep 2: Creating additional cells...");
    let (child1, child2) = canvas.split_cell(root_id, SplitDirection::Horizontal, 0.5)?;

    // Configure first child as a processing cell
    canvas.rename_cell(child1, Some("Process".to_string()))?;
    canvas.update_cell_type(child1, CellType::Python)?;
    canvas.update_cell_content(
        child1,
        CellContent::inline("# Process the input\nresult = value * 2\nprint(f'Processed: {result}')")
    )?;
    println!("  ✓ Created 'Process' cell");

    // Configure second child as output cell
    canvas.rename_cell(child2, Some("Output".to_string()))?;
    canvas.update_cell_type(child2, CellType::Text)?;
    canvas.update_cell_content(
        child2,
        CellContent::inline("This cell will display the final result")
    )?;
    println!("  ✓ Created 'Output' cell");

    // Step 3: Create a summary cell
    println!("\nStep 3: Creating summary cell...");
    let (summary, _) = canvas.split_cell(child2, SplitDirection::Vertical, 0.5)?;
    canvas.rename_cell(summary, Some("Summary".to_string()))?;
    canvas.update_cell_type(summary, CellType::Python)?;
    canvas.update_cell_content(
        summary,
        CellContent::inline("# Summary\nprint(f'Original: {value}, Processed: {result}')")
    )?;
    println!("  ✓ Created 'Summary' cell");

    // Step 4: Create relationships (data flow)
    println!("\nStep 4: Creating data flow relationships...");
    canvas.create_relationship(root_id, child1)?;
    println!("  ✓ Input → Process");
    canvas.create_relationship(child1, summary)?;
    println!("  ✓ Process → Summary");

    // Step 5: Validate the workflow
    println!("\nStep 5: Validating workflow...");
    use validation::ValidatedCanvas;
    let result = canvas.validate();

    if result.has_errors() {
        println!("  ❌ Validation errors:");
        for error in result.errors() {
            println!("     - {}", error.message);
        }
        return Err(anyhow::anyhow!("Validation failed"));
    }

    if result.has_warnings() {
        println!("  ⚠ Warnings:");
        for warning in result.warnings() {
            println!("     - {}", warning.message);
        }
    }

    println!("  ✓ Validation passed ({} info items)", result.info().len());
    for info in result.info() {
        println!("     ℹ {}", info.message);
    }

    // Step 6: Execute the workflow
    println!("\nStep 6: Executing workflow...");
    let mut engine = ExecutionEngine::new(ExecutionMode::Run);

    match engine.execute(&canvas) {
        Ok(report) => {
            println!("  ✓ Execution completed!");
            println!("     Total cells executed: {}", report.total_cells_executed);
            println!("     Total steps: {}", report.step);
            println!("     Status: {:?}", report.status);

            if !report.log.is_empty() {
                println!("\n  Execution log:");
                for entry in &report.log {
                    let cell_name = canvas.get_cell(entry.cell_id)
                        .and_then(|c| c.name.clone())
                        .unwrap_or_else(|| format!("{}", entry.cell_id));
                    match &entry.error {
                        None => println!("     ✓ {} (step {})", cell_name, entry.step),
                        Some(err) => println!("     ❌ {} (step {}): {}", cell_name, entry.step, err),
                    }
                }
            }
        }
        Err(e) => {
            println!("  ❌ Execution failed: {}", e);
            return Err(e);
        }
    }

    // Step 7: Save the project
    println!("\nStep 7: Saving project...");
    let project_path = PathBuf::from("/tmp/simple_workflow");
    let project = Project::create(&project_path)?;
    project.save(&canvas)?;
    println!("  ✓ Project saved to {}", project_path.display());

    // Step 8: Load the project back
    println!("\nStep 8: Loading project...");
    let loaded_project = Project::open(&project_path)?;
    let (_manifest, loaded_canvas) = loaded_project.load()?;
    println!("  ✓ Project loaded successfully");
    println!("     Cells: {}", loaded_canvas.cell_count());
    println!("     Relationships: {}", loaded_canvas.relationship_count());

    println!("\n=== Example completed successfully! ===");
    println!("\nTo visualize this workflow, run:");
    println!("  cargo run --bin gui --release");
    println!("  Then open the project at: {}", project_path.display());

    Ok(())
}
