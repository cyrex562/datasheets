// Graph Cell Editor - Core Library

pub mod cell;
pub mod canvas;
pub mod event;
pub mod relationship;

// Re-export main types for convenience
pub use cell::{Cell, CellContent, CellType, Rectangle};
pub use canvas::Canvas;
pub use event::{EventType, GraphEvent, SplitDirection};
pub use relationship::Relationship;
