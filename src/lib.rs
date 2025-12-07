// Graph Cell Editor - Core Library

pub mod canvas;
pub mod cell;
pub mod event;
pub mod relationship;
pub mod serialization;

// Re-export main types for convenience
pub use canvas::Canvas;
pub use cell::{Cell, CellContent, CellType, Rectangle};
pub use event::{EventType, GraphEvent, SplitDirection};
pub use relationship::Relationship;
pub use serialization::{ExternalFileHandle, Manifest, Project};
