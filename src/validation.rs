use crate::{Canvas, CellType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use ulid::Ulid;

/// Validation severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationSeverity {
    Info,    // Blue - informational
    Warning, // Yellow - potential issue
    Error,   // Red - blocks execution
}

/// Validation issue
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: ValidationSeverity,
    pub message: String,
    pub affected_cells: Vec<Ulid>,
    pub issue_type: ValidationIssueType,
}

/// Types of validation issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationIssueType {
    Cycle,
    NoStartPoint,
    OrphanCell,
    MissingReference,
    TypeMismatch,
    SyntaxError,
}

/// Complete validation result
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationResult {
    /// Create a new empty validation result
    pub fn new() -> Self {
        Self {
            issues: Vec::new(),
        }
    }

    /// Add an issue
    pub fn add_issue(&mut self, issue: ValidationIssue) {
        self.issues.push(issue);
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|i| i.severity == ValidationSeverity::Error)
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        self.issues
            .iter()
            .any(|i| i.severity == ValidationSeverity::Warning)
    }

    /// Get all errors
    pub fn errors(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Error)
            .collect()
    }

    /// Get all warnings
    pub fn warnings(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Warning)
            .collect()
    }

    /// Get all info messages
    pub fn info(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Info)
            .collect()
    }

    /// Check if validation passed (no errors)
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }
}

/// Validator for canvas graphs
pub struct Validator;

impl Validator {
    /// Run all validations on a canvas
    pub fn validate(canvas: &Canvas) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Check for start point
        if canvas.get_start_point().is_none() && canvas.cell_count() > 0 {
            result.add_issue(ValidationIssue {
                severity: ValidationSeverity::Error,
                message: "No start point set. Execution cannot begin.".to_string(),
                affected_cells: vec![],
                issue_type: ValidationIssueType::NoStartPoint,
            });
        }

        // Detect cycles
        if let Some(cycle_cells) = Self::detect_cycles(canvas) {
            result.add_issue(ValidationIssue {
                severity: ValidationSeverity::Warning,
                message: format!(
                    "Cycle detected in relationship graph involving {} cells. This may cause infinite loops.",
                    cycle_cells.len()
                ),
                affected_cells: cycle_cells,
                issue_type: ValidationIssueType::Cycle,
            });
        }

        // Detect orphan cells (unreachable from start point)
        if let Some(start) = canvas.get_start_point() {
            let orphans = Self::find_orphan_cells(canvas, start.id);
            if !orphans.is_empty() {
                result.add_issue(ValidationIssue {
                    severity: ValidationSeverity::Info,
                    message: format!(
                        "{} cell(s) are unreachable from the start point and will not execute.",
                        orphans.len()
                    ),
                    affected_cells: orphans,
                    issue_type: ValidationIssueType::OrphanCell,
                });
            }
        }

        // Validate cell references
        for cell in canvas.cells().values() {
            if cell.cell_type == CellType::Python {
                if let Some(content) = cell.content.as_str() {
                    // Check for cell: references
                    if content.contains("cell:") {
                        let missing_refs = Self::check_cell_references(canvas, cell.id, content);
                        if !missing_refs.is_empty() {
                            result.add_issue(ValidationIssue {
                                severity: ValidationSeverity::Error,
                                message: format!(
                                    "Missing cell references: {}",
                                    missing_refs.join(", ")
                                ),
                                affected_cells: vec![cell.id],
                                issue_type: ValidationIssueType::MissingReference,
                            });
                        }
                    }
                }
            }
        }

        result
    }

    /// Detect cycles in the relationship graph using DFS
    fn detect_cycles(canvas: &Canvas) -> Option<Vec<Ulid>> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut cycle_cells = Vec::new();

        for cell_id in canvas.cells().keys() {
            if !visited.contains(cell_id) {
                if Self::dfs_detect_cycle(
                    canvas,
                    *cell_id,
                    &mut visited,
                    &mut rec_stack,
                    &mut cycle_cells,
                ) {
                    return Some(cycle_cells);
                }
            }
        }

        None
    }

    /// DFS helper for cycle detection
    fn dfs_detect_cycle(
        canvas: &Canvas,
        cell_id: Ulid,
        visited: &mut HashSet<Ulid>,
        rec_stack: &mut HashSet<Ulid>,
        cycle_cells: &mut Vec<Ulid>,
    ) -> bool {
        visited.insert(cell_id);
        rec_stack.insert(cell_id);

        // Check all outgoing relationships
        for rel in canvas.get_outgoing_relationships(cell_id) {
            let target = rel.to;

            if !visited.contains(&target) {
                if Self::dfs_detect_cycle(canvas, target, visited, rec_stack, cycle_cells) {
                    cycle_cells.push(cell_id);
                    return true;
                }
            } else if rec_stack.contains(&target) {
                // Cycle detected
                cycle_cells.push(cell_id);
                cycle_cells.push(target);
                return true;
            }
        }

        rec_stack.remove(&cell_id);
        false
    }

    /// Find orphan cells (unreachable from start point)
    fn find_orphan_cells(canvas: &Canvas, start_id: Ulid) -> Vec<Ulid> {
        let mut reachable = HashSet::new();
        let mut queue = vec![start_id];

        // BFS to find all reachable cells
        while let Some(cell_id) = queue.pop() {
            if reachable.contains(&cell_id) {
                continue;
            }
            reachable.insert(cell_id);

            // Add downstream cells
            for rel in canvas.get_outgoing_relationships(cell_id) {
                queue.push(rel.to);
            }
        }

        // Find orphans (cells not reachable)
        canvas
            .cells()
            .keys()
            .filter(|id| !reachable.contains(id))
            .copied()
            .collect()
    }

    /// Check for missing cell references in Python code
    fn check_cell_references(canvas: &Canvas, _cell_id: Ulid, content: &str) -> Vec<String> {
        let mut missing = Vec::new();

        // Simple regex-like search for "from cell:Name" or "import cell:Name"
        for line in content.lines() {
            if line.contains("cell:") {
                // Extract cell name after "cell:"
                if let Some(start) = line.find("cell:") {
                    let after_colon = &line[start + 5..];
                    let name = after_colon
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or("");

                    if !name.is_empty() {
                        // Check if this cell name exists
                        let exists = canvas
                            .cells()
                            .values()
                            .any(|c| c.name.as_deref() == Some(name) && c.cell_type == CellType::Python);

                        if !exists {
                            missing.push(format!("cell:{}", name));
                        }
                    }
                }
            }
        }

        missing.sort();
        missing.dedup();
        missing
    }
}

/// Extension trait for Canvas to add validation
pub trait ValidatedCanvas {
    /// Validate the canvas
    fn validate(&self) -> ValidationResult;

    /// Get cells with validation issues
    fn cells_with_issues(&self, result: &ValidationResult) -> HashMap<Ulid, ValidationSeverity>;
}

impl ValidatedCanvas for Canvas {
    fn validate(&self) -> ValidationResult {
        Validator::validate(self)
    }

    fn cells_with_issues(&self, result: &ValidationResult) -> HashMap<Ulid, ValidationSeverity> {
        let mut cells = HashMap::new();

        for issue in &result.issues {
            for cell_id in &issue.affected_cells {
                cells
                    .entry(*cell_id)
                    .and_modify(|severity| {
                        // Keep the highest severity
                        if issue.severity as u8 > *severity as u8 {
                            *severity = issue.severity;
                        }
                    })
                    .or_insert(issue.severity);
            }
        }

        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CellContent, Rectangle};

    #[test]
    fn test_no_start_point_error() {
        let mut canvas = Canvas::new();
        canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Test"),
        );

        let result = Validator::validate(&canvas);
        assert!(result.has_errors());
        assert_eq!(result.errors()[0].issue_type, ValidationIssueType::NoStartPoint);
    }

    #[test]
    fn test_cycle_detection() {
        let mut canvas = Canvas::new();

        let cell1 = canvas.create_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("pass"),
        );

        let cell2 = canvas.create_cell(
            CellType::Python,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("pass"),
        );

        // Create a cycle: cell1 -> cell2 -> cell1
        canvas.create_relationship(cell1, cell2).unwrap();
        canvas.create_relationship(cell2, cell1).unwrap();
        canvas.set_start_point(cell1).unwrap();

        let result = Validator::validate(&canvas);
        assert!(result.has_warnings());

        let warnings = result.warnings();
        assert!(warnings.iter().any(|w| w.issue_type == ValidationIssueType::Cycle));
    }

    #[test]
    fn test_orphan_detection() {
        let mut canvas = Canvas::new();

        let cell1 = canvas.create_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("pass"),
        );

        let _cell2 = canvas.create_cell(
            CellType::Python,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("pass"),
        );

        canvas.set_start_point(cell1).unwrap();

        let result = Validator::validate(&canvas);
        assert!(result.has_warnings() || result.info().len() > 0);

        let info = result.info();
        assert!(info.iter().any(|i| i.issue_type == ValidationIssueType::OrphanCell));
    }

    #[test]
    fn test_missing_cell_reference() {
        let mut canvas = Canvas::new();

        let cell = canvas.create_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("from cell:NonExistent import data"),
        );

        canvas.set_start_point(cell).unwrap();

        let result = Validator::validate(&canvas);
        assert!(result.has_errors());

        let errors = result.errors();
        assert!(errors.iter().any(|e| e.issue_type == ValidationIssueType::MissingReference));
    }

    #[test]
    fn test_valid_canvas() {
        let mut canvas = Canvas::with_root_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("set_output(42)"),
        );

        let root = canvas.root_cell().unwrap();
        canvas.set_start_point(root).unwrap();

        let result = Validator::validate(&canvas);
        assert!(result.is_valid());
        assert!(!result.has_errors());
    }

    #[test]
    fn test_cells_with_issues() {
        let mut canvas = Canvas::new();

        let cell = canvas.create_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("from cell:Missing import data"),
        );

        canvas.set_start_point(cell).unwrap();

        let result = canvas.validate();
        let cells_with_issues = canvas.cells_with_issues(&result);

        assert!(cells_with_issues.contains_key(&cell));
        assert_eq!(cells_with_issues[&cell], ValidationSeverity::Error);
    }
}
