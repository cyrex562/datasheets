use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

/// A cell in the graph, representing a single unit of content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cell {
    /// Unique identifier (sortable, timestamp-based)
    pub id: Ulid,

    /// Optional human-readable name for references
    pub name: Option<String>,

    /// Cell type determines behavior
    pub cell_type: CellType,

    /// Position and size on canvas (pixels)
    pub bounds: Rectangle,

    /// Cell content (inline or external reference)
    pub content: CellContent,

    /// Execution starting point flag
    pub is_start_point: bool,

    /// Parent cell if this was created by split
    pub parent: Option<Ulid>,

    /// Child cells if this was split
    pub children: Vec<Ulid>,

    /// Future: chunk ID for performance optimization
    pub chunk_id: Option<Ulid>,
}

impl Cell {
    /// Create a new cell with the given parameters
    pub fn new(
        cell_type: CellType,
        bounds: Rectangle,
        content: CellContent,
    ) -> Self {
        Self {
            id: Ulid::new(),
            name: None,
            cell_type,
            bounds,
            content,
            is_start_point: false,
            parent: None,
            children: Vec::new(),
            chunk_id: None,
        }
    }

    /// Create a new cell with a specific ID (useful for testing)
    pub fn with_id(
        id: Ulid,
        cell_type: CellType,
        bounds: Rectangle,
        content: CellContent,
    ) -> Self {
        Self {
            id,
            name: None,
            cell_type,
            bounds,
            content,
            is_start_point: false,
            parent: None,
            children: Vec::new(),
            chunk_id: None,
        }
    }

    /// Set the cell's name
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name;
    }

    /// Set the cell as a start point
    pub fn set_start_point(&mut self, is_start: bool) {
        self.is_start_point = is_start;
    }

    /// Update the cell's content
    pub fn set_content(&mut self, content: CellContent) {
        self.content = content;
    }

    /// Update the cell's type
    pub fn set_type(&mut self, cell_type: CellType) {
        self.cell_type = cell_type;
    }

    /// Update the cell's bounds
    pub fn set_bounds(&mut self, bounds: Rectangle) {
        self.bounds = bounds;
    }

    /// Add a child cell (used during split)
    pub fn add_child(&mut self, child_id: Ulid) {
        if !self.children.contains(&child_id) {
            self.children.push(child_id);
        }
    }

    /// Set the parent cell (used during split)
    pub fn set_parent(&mut self, parent_id: Option<Ulid>) {
        self.parent = parent_id;
    }
}

/// Rectangle representing position and size on canvas
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rectangle {
    /// Create a new rectangle
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Get the right edge of the rectangle
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// Get the bottom edge of the rectangle
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// Check if this rectangle intersects with another
    pub fn intersects(&self, other: &Rectangle) -> bool {
        !(self.right() <= other.x
            || other.right() <= self.x
            || self.bottom() <= other.y
            || other.bottom() <= self.y)
    }

    /// Check if this rectangle contains a point
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
}

/// Cell type determines how content is rendered and executed
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CellType {
    Text,
    Python,
    // Future: Markdown, Json, Csv, Image, VisualBlock, etc.
}

/// Cell content can be inline or reference an external file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CellContent {
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

impl CellContent {
    /// Create new inline content
    pub fn inline(content: impl Into<String>) -> Self {
        CellContent::Inline(content.into())
    }

    /// Create new external content reference
    pub fn external(path: PathBuf, summary: impl Into<String>, use_mmap: bool) -> Self {
        CellContent::External {
            path,
            summary: summary.into(),
            use_mmap,
        }
    }

    /// Get the content as a string (for inline content)
    pub fn as_str(&self) -> Option<&str> {
        match self {
            CellContent::Inline(s) => Some(s),
            CellContent::External { .. } => None,
        }
    }

    /// Check if content is empty (for inline content)
    pub fn is_empty(&self) -> bool {
        match self {
            CellContent::Inline(s) => s.is_empty(),
            CellContent::External { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_creation() {
        let cell = Cell::new(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Hello"),
        );

        assert_eq!(cell.cell_type, CellType::Text);
        assert_eq!(cell.bounds.x, 0.0);
        assert_eq!(cell.bounds.y, 0.0);
        assert_eq!(cell.bounds.width, 100.0);
        assert_eq!(cell.bounds.height, 100.0);
        assert_eq!(cell.content.as_str(), Some("Hello"));
        assert!(!cell.is_start_point);
        assert_eq!(cell.parent, None);
        assert!(cell.children.is_empty());
    }

    #[test]
    fn test_rectangle_operations() {
        let rect1 = Rectangle::new(0.0, 0.0, 100.0, 100.0);
        let rect2 = Rectangle::new(50.0, 50.0, 100.0, 100.0);
        let rect3 = Rectangle::new(200.0, 200.0, 100.0, 100.0);

        assert!(rect1.intersects(&rect2));
        assert!(!rect1.intersects(&rect3));

        assert!(rect1.contains_point(50.0, 50.0));
        assert!(!rect1.contains_point(150.0, 150.0));

        assert_eq!(rect1.right(), 100.0);
        assert_eq!(rect1.bottom(), 100.0);
    }

    #[test]
    fn test_cell_content() {
        let inline = CellContent::inline("test");
        assert_eq!(inline.as_str(), Some("test"));
        assert!(!inline.is_empty());

        let empty = CellContent::inline("");
        assert!(empty.is_empty());

        let external = CellContent::external(
            PathBuf::from("/path/to/file.txt"),
            "External file",
            false,
        );
        assert_eq!(external.as_str(), None);
        assert!(!external.is_empty());
    }

    #[test]
    fn test_cell_mutations() {
        let mut cell = Cell::new(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Hello"),
        );

        cell.set_name(Some("TestCell".to_string()));
        assert_eq!(cell.name, Some("TestCell".to_string()));

        cell.set_start_point(true);
        assert!(cell.is_start_point);

        cell.set_content(CellContent::inline("Updated"));
        assert_eq!(cell.content.as_str(), Some("Updated"));

        cell.set_type(CellType::Python);
        assert_eq!(cell.cell_type, CellType::Python);

        let new_bounds = Rectangle::new(10.0, 10.0, 200.0, 200.0);
        cell.set_bounds(new_bounds);
        assert_eq!(cell.bounds, new_bounds);

        let child_id = Ulid::new();
        cell.add_child(child_id);
        assert_eq!(cell.children.len(), 1);
        assert_eq!(cell.children[0], child_id);

        let parent_id = Ulid::new();
        cell.set_parent(Some(parent_id));
        assert_eq!(cell.parent, Some(parent_id));
    }
}
