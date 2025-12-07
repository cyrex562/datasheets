use graph_cell_editor::{Canvas, CellContent, CellType, Project, Rectangle, SplitDirection};

fn main() {
    println!("Graph Cell Editor - Phase 1 & 2 Demo");
    println!("=====================================\n");

    // Phase 1: Core Data Model
    println!("Phase 1: Core Data Model");
    println!("------------------------");

    // Create a canvas with a root cell
    let mut canvas = Canvas::with_root_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 400.0, 300.0),
        CellContent::inline("Root Cell"),
    );

    println!("✓ Created canvas with root cell");
    println!("  Cells: {}", canvas.cell_count());

    // Get the root cell ID
    let root_id = canvas.root_cell().unwrap();

    // Split the root cell horizontally
    let (top, bottom) = canvas
        .split_cell(root_id, SplitDirection::Horizontal, 0.5)
        .unwrap();

    println!("\n✓ Split root cell horizontally");
    println!("  Cells: {} (root + 2 children)", canvas.cell_count());

    // Split the top cell vertically
    let (top_left, top_right) = canvas
        .split_cell(top, SplitDirection::Vertical, 0.5)
        .unwrap();

    println!("\n✓ Split top cell vertically");
    println!("  Cells: {} (root + top + bottom + top_left + top_right)", canvas.cell_count());

    // Update content in cells
    canvas
        .update_cell_content(top_left, CellContent::inline("Top Left"))
        .unwrap();
    canvas
        .update_cell_content(top_right, CellContent::inline("Top Right"))
        .unwrap();
    canvas
        .update_cell_content(bottom, CellContent::inline("Bottom"))
        .unwrap();

    // Name the cells
    canvas.rename_cell(top_left, Some("DataInput".to_string())).unwrap();
    canvas.rename_cell(top_right, Some("Process".to_string())).unwrap();
    canvas.rename_cell(bottom, Some("Output".to_string())).unwrap();

    println!("\n✓ Updated cell content and names");

    // Create relationships
    canvas.create_relationship(top_left, top_right).unwrap();
    canvas.create_relationship(top_right, bottom).unwrap();

    println!("\n✓ Created data flow relationships");
    println!("  Relationships: {}", canvas.relationship_count());

    // Set start point
    canvas.set_start_point(top_left).unwrap();

    println!("\n✓ Set start point");

    // Display canvas structure
    println!("\n📊 Canvas Structure:");
    println!("  └─ Cells: {}", canvas.cell_count());
    println!("  └─ Relationships: {}", canvas.relationship_count());
    println!("  └─ Events logged: {}", canvas.events().len());

    // Test adjacency detection
    let adjacent = canvas.find_adjacent_cells(top).unwrap();
    println!("\n🔗 Adjacency Analysis:");
    println!("  Cells adjacent to 'top' cell: {}", adjacent.len());

    println!("\n✅ Phase 1 implementation complete!");

    // Phase 2: Serialization
    println!("\n\nPhase 2: Serialization");
    println!("----------------------\n");

    // Create a temporary project directory
    let temp_dir = std::env::temp_dir().join("graph_cell_editor_demo");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    // Create project
    let project = Project::create(&temp_dir).unwrap();
    println!("✓ Created project at: {}", temp_dir.display());
    println!("  - manifest.json");
    println!("  - cells.json");
    println!("  - events.jsonl");
    println!("  - external/");
    println!("  - snapshots/");

    // Save the canvas
    project.save(&canvas).unwrap();
    println!("\n✓ Saved canvas to project");
    println!("  Events logged: {}", canvas.events().len());

    // Load the canvas back
    let (manifest, loaded_canvas) = project.load().unwrap();
    println!("\n✓ Loaded canvas from project");
    println!("  Manifest version: {}", manifest.version);
    println!("  Created: {}", manifest.created);
    println!("  Modified: {}", manifest.modified);
    println!("  Start cell: {:?}", manifest.start_cell);

    // Verify loaded canvas matches original
    println!("\n📊 Verification:");
    println!("  Original cells: {}", canvas.cell_count());
    println!("  Loaded cells: {}", loaded_canvas.cell_count());
    println!("  Original relationships: {}", canvas.relationship_count());
    println!("  Loaded relationships: {}", loaded_canvas.relationship_count());

    // Check if specific cells are preserved
    if let Some(cell) = loaded_canvas.get_cell(top_left) {
        println!("\n  ✓ Cell 'DataInput' preserved:");
        println!("    Name: {:?}", cell.name);
        println!("    Content: {:?}", cell.content.as_str());
        println!("    Type: {:?}", cell.cell_type);
        println!("    Is start point: {}", cell.is_start_point);
    }

    // Load events
    let loaded_events = project.load_events().unwrap();
    println!("\n✓ Loaded {} events from events.jsonl", loaded_events.len());

    println!("\n✅ Phase 2 implementation complete!");
    println!("   All serialization operations working correctly.\n");

    // Cleanup
    std::fs::remove_dir_all(&temp_dir).ok();
}
