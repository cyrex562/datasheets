// Graph Cell Editor - Core Library

pub mod canvas;
pub mod cell;
pub mod event;
pub mod execution;
pub mod relationship;
pub mod serialization;
pub mod ui;
pub mod validation;

// Re-export main types for convenience
pub use canvas::Canvas;
pub use cell::{Cell, CellContent, CellType, Rectangle};
pub use event::{EventType, GraphEvent, SplitDirection};
pub use execution::{CellData, ExecutionEngine, ExecutionMode, ExecutionReport, ExecutionStatus};
pub use relationship::Relationship;
pub use serialization::{ExternalFileHandle, Manifest, Project};
pub use ui::GraphCellEditorApp;
pub use validation::{ValidationIssue, ValidationResult, ValidationSeverity, ValidatedCanvas};
