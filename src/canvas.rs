use crate::{Cell, CellContent, CellType, EventType, GraphEvent, Rectangle, Relationship, SplitDirection};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use ulid::Ulid;

/// Canvas containing all cells and relationships
#[derive(Debug, Clone)]
pub struct Canvas {
    /// All cells indexed by ID
    cells: HashMap<Ulid, Cell>,

    /// Data flow relationships (execution graph)
    relationships: HashMap<(Ulid, Ulid), Relationship>,

    /// Optional: track the original/root cell
    root_cell: Option<Ulid>,

    /// Event log for history tracking
    events: Vec<GraphEvent>,

    // Future: spatial index (quadtree) for fast adjacency queries
}

impl Canvas {
    /// Create a new empty canvas
    pub fn new() -> Self {
        Self {
            cells: HashMap::new(),
            relationships: HashMap::new(),
            root_cell: None,
            events: Vec::new(),
        }
    }

    /// Create a canvas with an initial root cell
    pub fn with_root_cell(cell_type: CellType, bounds: Rectangle, content: CellContent) -> Self {
        let mut canvas = Self::new();
        let cell = Cell::new(cell_type, bounds, content);
        let cell_id = cell.id;
        canvas.cells.insert(cell_id, cell);
        canvas.root_cell = Some(cell_id);

        canvas.log_event(EventType::CellCreated {
            id: cell_id,
            cell_type,
            bounds,
            name: None,
        });

        canvas
    }

    // ========== Cell CRUD Operations ==========

    /// Create a new cell and add it to the canvas
    pub fn create_cell(
        &mut self,
        cell_type: CellType,
        bounds: Rectangle,
        content: CellContent,
    ) -> Ulid {
        let cell = Cell::new(cell_type, bounds, content);
        let cell_id = cell.id;

        self.log_event(EventType::CellCreated {
            id: cell_id,
            cell_type,
            bounds,
            name: None,
        });

        self.cells.insert(cell_id, cell);
        cell_id
    }

    /// Get a cell by ID
    pub fn get_cell(&self, id: Ulid) -> Option<&Cell> {
        self.cells.get(&id)
    }

    /// Get a mutable reference to a cell by ID
    pub fn get_cell_mut(&mut self, id: Ulid) -> Option<&mut Cell> {
        self.cells.get_mut(&id)
    }

    /// Get all cells
    pub fn cells(&self) -> &HashMap<Ulid, Cell> {
        &self.cells
    }

    /// Get mutable access to cells (for deserialization)
    pub(crate) fn cells_mut(&mut self) -> &mut HashMap<Ulid, Cell> {
        &mut self.cells
    }

    /// Update a cell's content
    pub fn update_cell_content(&mut self, id: Ulid, content: CellContent) -> Result<()> {
        let cell = self
            .cells
            .get_mut(&id)
            .ok_or_else(|| anyhow!("Cell not found: {}", id))?;

        cell.set_content(content.clone());

        self.log_event(EventType::CellContentChanged {
            id,
            new_content: content,
        });

        Ok(())
    }

    /// Update a cell's type
    pub fn update_cell_type(&mut self, id: Ulid, new_type: CellType) -> Result<()> {
        let cell = self
            .cells
            .get_mut(&id)
            .ok_or_else(|| anyhow!("Cell not found: {}", id))?;

        let old_type = cell.cell_type;
        cell.set_type(new_type);

        self.log_event(EventType::CellTypeChanged {
            id,
            old_type,
            new_type,
        });

        Ok(())
    }

    /// Rename a cell
    pub fn rename_cell(&mut self, id: Ulid, name: Option<String>) -> Result<()> {
        let cell = self
            .cells
            .get_mut(&id)
            .ok_or_else(|| anyhow!("Cell not found: {}", id))?;

        cell.set_name(name.clone());

        self.log_event(EventType::CellRenamed { id, new_name: name });

        Ok(())
    }

    /// Delete a cell (and all its relationships)
    pub fn delete_cell(&mut self, id: Ulid) -> Result<()> {
        if !self.cells.contains_key(&id) {
            return Err(anyhow!("Cell not found: {}", id));
        }

        // Remove all relationships involving this cell
        let relationships_to_remove: Vec<(Ulid, Ulid)> = self
            .relationships
            .keys()
            .filter(|(from, to)| *from == id || *to == id)
            .copied()
            .collect();

        for (from, to) in relationships_to_remove {
            self.delete_relationship(from, to)?;
        }

        // Remove the cell
        self.cells.remove(&id);

        // Clear root cell if it was the deleted cell
        if self.root_cell == Some(id) {
            self.root_cell = None;
        }

        Ok(())
    }

    /// Set a cell as the start point
    pub fn set_start_point(&mut self, id: Ulid) -> Result<()> {
        // Find current start point
        let old_start = self
            .cells
            .values()
            .find(|c| c.is_start_point)
            .map(|c| c.id);

        // Clear old start point
        if let Some(old_id) = old_start {
            if let Some(old_cell) = self.cells.get_mut(&old_id) {
                old_cell.set_start_point(false);
            }
        }

        // Set new start point
        let cell = self
            .cells
            .get_mut(&id)
            .ok_or_else(|| anyhow!("Cell not found: {}", id))?;
        cell.set_start_point(true);

        self.log_event(EventType::StartPointChanged {
            old_id: old_start,
            new_id: id,
        });

        Ok(())
    }

    // ========== Relationship CRUD Operations ==========

    /// Create a relationship between two cells
    pub fn create_relationship(&mut self, from: Ulid, to: Ulid) -> Result<()> {
        if !self.cells.contains_key(&from) {
            return Err(anyhow!("Source cell not found: {}", from));
        }
        if !self.cells.contains_key(&to) {
            return Err(anyhow!("Destination cell not found: {}", to));
        }
        if from == to {
            return Err(anyhow!("Cannot create self-referential relationship"));
        }

        let relationship = Relationship::new(from, to);
        self.relationships.insert((from, to), relationship);

        self.log_event(EventType::RelationshipCreated { from, to });

        Ok(())
    }

    /// Get a relationship
    pub fn get_relationship(&self, from: Ulid, to: Ulid) -> Option<&Relationship> {
        self.relationships.get(&(from, to))
    }

    /// Get all relationships
    pub fn relationships(&self) -> &HashMap<(Ulid, Ulid), Relationship> {
        &self.relationships
    }

    /// Get mutable access to relationships (for deserialization)
    pub(crate) fn relationships_mut(&mut self) -> &mut HashMap<(Ulid, Ulid), Relationship> {
        &mut self.relationships
    }

    /// Delete a relationship
    pub fn delete_relationship(&mut self, from: Ulid, to: Ulid) -> Result<()> {
        if self.relationships.remove(&(from, to)).is_none() {
            return Err(anyhow!("Relationship not found: {} -> {}", from, to));
        }

        self.log_event(EventType::RelationshipDeleted { from, to });

        Ok(())
    }

    /// Get all relationships starting from a cell
    pub fn get_outgoing_relationships(&self, from: Ulid) -> Vec<&Relationship> {
        self.relationships
            .values()
            .filter(|r| r.from == from)
            .collect()
    }

    /// Get all relationships ending at a cell
    pub fn get_incoming_relationships(&self, to: Ulid) -> Vec<&Relationship> {
        self.relationships
            .values()
            .filter(|r| r.to == to)
            .collect()
    }

    // ========== Cell Splitting ==========

    /// Split a cell into two cells
    pub fn split_cell(
        &mut self,
        cell_id: Ulid,
        direction: SplitDirection,
        split_ratio: f32,
    ) -> Result<(Ulid, Ulid)> {
        if split_ratio <= 0.0 || split_ratio >= 1.0 {
            return Err(anyhow!(
                "Split ratio must be between 0.0 and 1.0 (exclusive)"
            ));
        }

        let cell = self
            .cells
            .get(&cell_id)
            .ok_or_else(|| anyhow!("Cell not found: {}", cell_id))?
            .clone();

        // Create new cells with new IDs
        let child1_id = Ulid::new();
        let child2_id = Ulid::new();

        // Calculate bounds for split cells
        let (bounds1, bounds2) = match direction {
            SplitDirection::Horizontal => {
                let split_y = cell.bounds.y + (cell.bounds.height * split_ratio);
                (
                    Rectangle::new(
                        cell.bounds.x,
                        cell.bounds.y,
                        cell.bounds.width,
                        split_y - cell.bounds.y,
                    ),
                    Rectangle::new(
                        cell.bounds.x,
                        split_y,
                        cell.bounds.width,
                        cell.bounds.y + cell.bounds.height - split_y,
                    ),
                )
            }
            SplitDirection::Vertical => {
                let split_x = cell.bounds.x + (cell.bounds.width * split_ratio);
                (
                    Rectangle::new(
                        cell.bounds.x,
                        cell.bounds.y,
                        split_x - cell.bounds.x,
                        cell.bounds.height,
                    ),
                    Rectangle::new(
                        split_x,
                        cell.bounds.y,
                        cell.bounds.x + cell.bounds.width - split_x,
                        cell.bounds.height,
                    ),
                )
            }
        };

        // Create child cells
        // Child 1 inherits content, Child 2 is empty
        let child1 = Cell {
            id: child1_id,
            name: None, // Names must be re-assigned by user
            cell_type: cell.cell_type,
            bounds: bounds1,
            content: cell.content.clone(), // Inherits content
            is_start_point: cell.is_start_point,
            parent: Some(cell_id),
            children: vec![],
            chunk_id: cell.chunk_id,
        };

        let child2 = Cell {
            id: child2_id,
            name: None,
            cell_type: cell.cell_type,
            bounds: bounds2,
            content: CellContent::Inline(String::new()), // Empty
            is_start_point: false,
            parent: Some(cell_id),
            children: vec![],
            chunk_id: cell.chunk_id,
        };

        // Update parent cell's children list
        let parent_cell = self.cells.get_mut(&cell_id).unwrap();
        parent_cell.add_child(child1_id);
        parent_cell.add_child(child2_id);

        // Insert cells
        self.cells.insert(child1_id, child1);
        self.cells.insert(child2_id, child2);

        // Log event
        self.log_event(EventType::CellSplit {
            parent_id: cell_id,
            children: vec![child1_id, child2_id],
            direction,
            split_ratio,
        });

        Ok((child1_id, child2_id))
    }

    // ========== Cell Merging ==========

    /// Merge multiple cells into one
    pub fn merge_cells(
        &mut self,
        cell_ids: Vec<Ulid>,
        new_type: CellType,
        merged_content: CellContent,
    ) -> Result<Ulid> {
        if cell_ids.len() < 2 {
            return Err(anyhow!("Must merge at least 2 cells"));
        }

        // Verify all cells exist
        for id in &cell_ids {
            if !self.cells.contains_key(id) {
                return Err(anyhow!("Cell not found: {}", id));
            }
        }

        // Calculate bounding box for merged cell
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for id in &cell_ids {
            let cell = &self.cells[id];
            min_x = min_x.min(cell.bounds.x);
            min_y = min_y.min(cell.bounds.y);
            max_x = max_x.max(cell.bounds.right());
            max_y = max_y.max(cell.bounds.bottom());
        }

        let merged_bounds = Rectangle::new(min_x, min_y, max_x - min_x, max_y - min_y);

        // Create new merged cell
        let new_id = Ulid::new();
        let merged_cell = Cell::new(new_type, merged_bounds, merged_content);

        // Delete all old cells (this also removes their relationships)
        for id in &cell_ids {
            self.delete_cell(*id)?;
        }

        // Insert merged cell
        self.cells.insert(new_id, merged_cell);

        // Log event
        self.log_event(EventType::CellMerged {
            merged_ids: cell_ids,
            new_id,
            new_type,
        });

        Ok(new_id)
    }

    // ========== Adjacency Detection ==========

    /// Check if two cells are adjacent (share an edge)
    pub fn are_cells_adjacent(&self, id1: Ulid, id2: Ulid) -> Result<bool> {
        let cell1 = self
            .cells
            .get(&id1)
            .ok_or_else(|| anyhow!("Cell not found: {}", id1))?;
        let cell2 = self
            .cells
            .get(&id2)
            .ok_or_else(|| anyhow!("Cell not found: {}", id2))?;

        Ok(Self::rectangles_adjacent(&cell1.bounds, &cell2.bounds))
    }

    /// Check if two rectangles are adjacent
    fn rectangles_adjacent(r1: &Rectangle, r2: &Rectangle) -> bool {
        // Horizontal adjacency (left/right)
        let horizontal_adjacent = (r1.right() == r2.x || r2.right() == r1.x)
            && !(r1.bottom() <= r2.y || r2.bottom() <= r1.y);

        // Vertical adjacency (top/bottom)
        let vertical_adjacent = (r1.bottom() == r2.y || r2.bottom() == r1.y)
            && !(r1.right() <= r2.x || r2.right() <= r1.x);

        horizontal_adjacent || vertical_adjacent
    }

    /// Find all cells adjacent to a given cell
    pub fn find_adjacent_cells(&self, cell_id: Ulid) -> Result<Vec<Ulid>> {
        let cell = self
            .cells
            .get(&cell_id)
            .ok_or_else(|| anyhow!("Cell not found: {}", cell_id))?;

        let adjacent: Vec<Ulid> = self
            .cells
            .iter()
            .filter(|(id, other)| {
                **id != cell_id && Self::rectangles_adjacent(&cell.bounds, &other.bounds)
            })
            .map(|(id, _)| *id)
            .collect();

        Ok(adjacent)
    }

    // ========== Event Logging ==========

    /// Log an event
    fn log_event(&mut self, event: EventType) {
        self.events.push(GraphEvent::new(event));
    }

    /// Get all events
    pub fn events(&self) -> &[GraphEvent] {
        &self.events
    }

    /// Clear event log
    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    // ========== Utility Methods ==========

    /// Get the root cell ID
    pub fn root_cell(&self) -> Option<Ulid> {
        self.root_cell
    }

    /// Set the root cell (for deserialization)
    pub(crate) fn set_root_cell(&mut self, root: Ulid) {
        self.root_cell = Some(root);
    }

    /// Get the start point cell
    pub fn get_start_point(&self) -> Option<&Cell> {
        self.cells.values().find(|c| c.is_start_point)
    }

    /// Count cells
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Count relationships
    pub fn relationship_count(&self) -> usize {
        self.relationships.len()
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_creation() {
        let canvas = Canvas::new();
        assert_eq!(canvas.cell_count(), 0);
        assert_eq!(canvas.relationship_count(), 0);
    }

    #[test]
    fn test_canvas_with_root_cell() {
        let canvas = Canvas::with_root_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Root"),
        );
        assert_eq!(canvas.cell_count(), 1);
        assert!(canvas.root_cell().is_some());
    }

    #[test]
    fn test_cell_crud() {
        let mut canvas = Canvas::new();

        // Create
        let id = canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Test"),
        );
        assert_eq!(canvas.cell_count(), 1);

        // Read
        let cell = canvas.get_cell(id).unwrap();
        assert_eq!(cell.content.as_str(), Some("Test"));

        // Update content
        canvas
            .update_cell_content(id, CellContent::inline("Updated"))
            .unwrap();
        let cell = canvas.get_cell(id).unwrap();
        assert_eq!(cell.content.as_str(), Some("Updated"));

        // Update type
        canvas.update_cell_type(id, CellType::Python).unwrap();
        let cell = canvas.get_cell(id).unwrap();
        assert_eq!(cell.cell_type, CellType::Python);

        // Rename
        canvas.rename_cell(id, Some("TestCell".to_string())).unwrap();
        let cell = canvas.get_cell(id).unwrap();
        assert_eq!(cell.name, Some("TestCell".to_string()));

        // Delete
        canvas.delete_cell(id).unwrap();
        assert_eq!(canvas.cell_count(), 0);
    }

    #[test]
    fn test_relationship_crud() {
        let mut canvas = Canvas::new();

        let id1 = canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Cell 1"),
        );
        let id2 = canvas.create_cell(
            CellType::Text,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("Cell 2"),
        );

        // Create relationship
        canvas.create_relationship(id1, id2).unwrap();
        assert_eq!(canvas.relationship_count(), 1);

        // Read relationship
        let rel = canvas.get_relationship(id1, id2).unwrap();
        assert_eq!(rel.from, id1);
        assert_eq!(rel.to, id2);

        // Delete relationship
        canvas.delete_relationship(id1, id2).unwrap();
        assert_eq!(canvas.relationship_count(), 0);
    }

    #[test]
    fn test_split_cell_horizontal() {
        let mut canvas = Canvas::new();

        let id = canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Parent"),
        );

        let (child1, child2) = canvas
            .split_cell(id, SplitDirection::Horizontal, 0.5)
            .unwrap();

        // Check parent cell
        let parent = canvas.get_cell(id).unwrap();
        assert_eq!(parent.children.len(), 2);

        // Check child 1 (inherits content)
        let c1 = canvas.get_cell(child1).unwrap();
        assert_eq!(c1.content.as_str(), Some("Parent"));
        assert_eq!(c1.bounds.height, 50.0);
        assert_eq!(c1.parent, Some(id));

        // Check child 2 (empty)
        let c2 = canvas.get_cell(child2).unwrap();
        assert_eq!(c2.content.as_str(), Some(""));
        assert_eq!(c2.bounds.height, 50.0);
        assert_eq!(c2.parent, Some(id));

        assert_eq!(canvas.cell_count(), 3); // Parent + 2 children
    }

    #[test]
    fn test_split_cell_vertical() {
        let mut canvas = Canvas::new();

        let id = canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Parent"),
        );

        let (child1, child2) = canvas
            .split_cell(id, SplitDirection::Vertical, 0.5)
            .unwrap();

        let c1 = canvas.get_cell(child1).unwrap();
        let c2 = canvas.get_cell(child2).unwrap();

        assert_eq!(c1.bounds.width, 50.0);
        assert_eq!(c2.bounds.width, 50.0);
    }

    #[test]
    fn test_merge_cells() {
        let mut canvas = Canvas::new();

        let id1 = canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Cell 1"),
        );
        let id2 = canvas.create_cell(
            CellType::Text,
            Rectangle::new(100.0, 0.0, 100.0, 100.0),
            CellContent::inline("Cell 2"),
        );

        let merged_id = canvas
            .merge_cells(
                vec![id1, id2],
                CellType::Text,
                CellContent::inline("Merged"),
            )
            .unwrap();

        // Old cells should be gone
        assert!(canvas.get_cell(id1).is_none());
        assert!(canvas.get_cell(id2).is_none());

        // New cell should exist
        let merged = canvas.get_cell(merged_id).unwrap();
        assert_eq!(merged.content.as_str(), Some("Merged"));
        assert_eq!(merged.bounds.width, 200.0);

        assert_eq!(canvas.cell_count(), 1);
    }

    #[test]
    fn test_adjacency_detection() {
        // Adjacent horizontally
        let r1 = Rectangle::new(0.0, 0.0, 100.0, 100.0);
        let r2 = Rectangle::new(100.0, 0.0, 100.0, 100.0);
        assert!(Canvas::rectangles_adjacent(&r1, &r2));

        // Adjacent vertically
        let r3 = Rectangle::new(0.0, 100.0, 100.0, 100.0);
        assert!(Canvas::rectangles_adjacent(&r1, &r3));

        // Not adjacent
        let r4 = Rectangle::new(200.0, 200.0, 100.0, 100.0);
        assert!(!Canvas::rectangles_adjacent(&r1, &r4));

        // Overlapping (not adjacent)
        let r5 = Rectangle::new(50.0, 50.0, 100.0, 100.0);
        assert!(!Canvas::rectangles_adjacent(&r1, &r5));
    }

    #[test]
    fn test_find_adjacent_cells() {
        let mut canvas = Canvas::new();

        let center = canvas.create_cell(
            CellType::Text,
            Rectangle::new(100.0, 100.0, 100.0, 100.0),
            CellContent::inline("Center"),
        );

        let left = canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 100.0, 100.0, 100.0),
            CellContent::inline("Left"),
        );

        let right = canvas.create_cell(
            CellType::Text,
            Rectangle::new(200.0, 100.0, 100.0, 100.0),
            CellContent::inline("Right"),
        );

        let top = canvas.create_cell(
            CellType::Text,
            Rectangle::new(100.0, 0.0, 100.0, 100.0),
            CellContent::inline("Top"),
        );

        let bottom = canvas.create_cell(
            CellType::Text,
            Rectangle::new(100.0, 200.0, 100.0, 100.0),
            CellContent::inline("Bottom"),
        );

        let far_away = canvas.create_cell(
            CellType::Text,
            Rectangle::new(500.0, 500.0, 100.0, 100.0),
            CellContent::inline("Far"),
        );

        let adjacent = canvas.find_adjacent_cells(center).unwrap();
        assert_eq!(adjacent.len(), 4);
        assert!(adjacent.contains(&left));
        assert!(adjacent.contains(&right));
        assert!(adjacent.contains(&top));
        assert!(adjacent.contains(&bottom));
        assert!(!adjacent.contains(&far_away));
    }

    #[test]
    fn test_start_point() {
        let mut canvas = Canvas::new();

        let id1 = canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Cell 1"),
        );
        let id2 = canvas.create_cell(
            CellType::Text,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("Cell 2"),
        );

        // Set id1 as start point
        canvas.set_start_point(id1).unwrap();
        assert!(canvas.get_cell(id1).unwrap().is_start_point);
        assert!(!canvas.get_cell(id2).unwrap().is_start_point);

        // Change to id2
        canvas.set_start_point(id2).unwrap();
        assert!(!canvas.get_cell(id1).unwrap().is_start_point);
        assert!(canvas.get_cell(id2).unwrap().is_start_point);
    }

    #[test]
    fn test_event_logging() {
        let mut canvas = Canvas::new();

        canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Test"),
        );

        assert_eq!(canvas.events().len(), 1);

        match &canvas.events()[0].event {
            EventType::CellCreated { cell_type, .. } => {
                assert_eq!(*cell_type, CellType::Text);
            }
            _ => panic!("Expected CellCreated event"),
        }
    }
}
