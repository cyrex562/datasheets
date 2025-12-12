// Helper functions to generate test canvases with various configurations

use graph_cell_editor::{Canvas, Cell, CellContent, CellType, Rectangle};
use ulid::Ulid;

/// Create a canvas with two Number cells for testing math operations
pub fn create_math_test_canvas() -> (Canvas, Ulid, Ulid) {
    let mut canvas = Canvas::new();
    
    let cell1 = canvas.create_cell(
        CellType::NumberInt,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::inline("10"),
    );
    
    let cell2 = canvas.create_cell(
        CellType::NumberInt,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::inline("20"),
    );
    
    (canvas, cell1, cell2)
}

/// Create a canvas with a Math cell referencing two Number cells
pub fn create_math_formula_canvas() -> (Canvas, Ulid, Ulid, Ulid) {
    let (mut canvas, num1, num2) = create_math_test_canvas();
    
    // Get short IDs for formula
    let num1_short = canvas.get_cell(num1).unwrap().short_id.clone();
    let num2_short = canvas.get_cell(num2).unwrap().short_id.clone();
    
    let math_cell = canvas.create_cell(
        CellType::Math,
        Rectangle::new(300.0, 0.0, 100.0, 100.0),
        CellContent::inline(&format!("[[{}]]+[[{}]]", num1_short, num2_short)),
    );
    
    (canvas, num1, num2, math_cell)
}

/// Create a canvas with circular reference: A → B → A
pub fn create_circular_reference_2_cells() -> (Canvas, Ulid, Ulid) {
    let mut canvas = Canvas::new();
    
    let cell_a = canvas.create_cell(
        CellType::Math,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::inline(""),
    );
    
    let cell_b = canvas.create_cell(
        CellType::Math,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::inline(""),
    );
    
    // Set up circular references
    let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
    let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();
    
    // A references B
    canvas.update_cell_content(cell_a, CellContent::inline(&format!("[[{}]]", cell_b_short)));
    
    // B references A (creates cycle)
    canvas.update_cell_content(cell_b, CellContent::inline(&format!("[[{}]]", cell_a_short)));
    
    (canvas, cell_a, cell_b)
}

/// Create a canvas with circular reference: A → B → C → A
pub fn create_circular_reference_3_cells() -> (Canvas, Ulid, Ulid, Ulid) {
    let mut canvas = Canvas::new();
    
    let cell_a = canvas.create_cell(
        CellType::Math,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::inline(""),
    );
    
    let cell_b = canvas.create_cell(
        CellType::Math,
        Rectangle::new(150.0, 0.0, 100.0, 100.0),
        CellContent::inline(""),
    );
    
    let cell_c = canvas.create_cell(
        CellType::Math,
        Rectangle::new(300.0, 0.0, 100.0, 100.0),
        CellContent::inline(""),
    );
    
    // Get short IDs
    let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
    let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();
    let cell_c_short = canvas.get_cell(cell_c).unwrap().short_id.clone();
    
    // A → B → C → A
    canvas.update_cell_content(cell_a, CellContent::inline(&format!("[[{}]]", cell_b_short)));
    canvas.update_cell_content(cell_b, CellContent::inline(&format!("[[{}]]", cell_c_short)));
    canvas.update_cell_content(cell_c, CellContent::inline(&format!("[[{}]]", cell_a_short)));
    
    (canvas, cell_a, cell_b, cell_c)
}

/// Create a canvas with a self-referencing Math cell: A → A
pub fn create_self_reference() -> (Canvas, Ulid) {
    let mut canvas = Canvas::new();
    
    let cell = canvas.create_cell(
        CellType::Math,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::inline(""),
    );
    
    let cell_short = canvas.get_cell(cell).unwrap().short_id.clone();
    canvas.update_cell_content(cell, CellContent::inline(&format!("[[{}]]+1", cell_short)));
    
    (canvas, cell)
}

/// Create a split cell structure (horizontal split)
pub fn create_split_canvas_horizontal() -> (Canvas, Ulid, Ulid, Ulid) {
    let mut canvas = Canvas::new();
    
    let parent = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 200.0, 200.0),
        CellContent::inline("Parent Content"),
    );
    
    let (child1, child2) = canvas
        .split_cell(parent, graph_cell_editor::SplitDirection::Horizontal, 0.5)
        .unwrap();
    
    (canvas, parent, child1, child2)
}

/// Create a split cell structure (vertical split)
pub fn create_split_canvas_vertical() -> (Canvas, Ulid, Ulid, Ulid) {
    let mut canvas = Canvas::new();
    
    let parent = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 200.0, 200.0),
        CellContent::inline("Parent Content"),
    );
    
    let (child1, child2) = canvas
        .split_cell(parent, graph_cell_editor::SplitDirection::Vertical, 0.5)
        .unwrap();
    
    (canvas, parent, child1, child2)
}

/// Create a canvas with markdown content in a cell
pub fn create_markdown_canvas() -> (Canvas, Ulid) {
    let mut canvas = Canvas::new();
    
    let markdown_cell = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 400.0, 300.0),
        CellContent::inline("# Markdown Title\n\nThis is **bold** and *italic* text.\n\n- Item 1\n- Item 2"),
    );
    
    (canvas, markdown_cell)
}

/// Create a canvas with cells containing markdown links
pub fn create_markdown_links_canvas() -> (Canvas, Ulid, Ulid, Ulid) {
    let mut canvas = Canvas::new();
    
    let target_cell = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 0.0, 100.0, 100.0),
        CellContent::inline("Target"),
    );
    
    let target_short = canvas.get_cell(target_cell).unwrap().short_id.clone();
    
    let link_cell = canvas.create_cell(
        CellType::Text,
        Rectangle::new(150.0, 0.0, 200.0, 100.0),
        CellContent::inline(&format!("See [[{}]] for more info", target_short)),
    );
    
    let multi_link_cell = canvas.create_cell(
        CellType::Text,
        Rectangle::new(0.0, 150.0, 300.0, 100.0),
        CellContent::inline(&format!("Links: [[{}]] and [[{}]]", target_short, target_short)),
    );
    
    (canvas, target_cell, link_cell, multi_link_cell)
}
