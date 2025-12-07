use graph_cell_editor::{Canvas, CellContent, CellType, Rectangle, SplitDirection};

fn main() {
    println!("Graph Cell Editor - Phase 1: Core Data Model");
    println!("=============================================\n");

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
    println!("   All core data model operations working correctly.\n");
}
