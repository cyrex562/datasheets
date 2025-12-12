use crate::{Canvas, Cell, CellContent, CellType};
use anyhow::{anyhow, Result};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use ulid::Ulid;

/// Data types that can be passed between cells
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CellData {
    None,
    Text(String),
    Number(f64),
    Boolean(bool),
    Json(serde_json::Value),
    Binary(Vec<u8>),
}

impl CellData {
    /// Attempt to coerce this data to match target type
    pub fn coerce_to_string(&self) -> String {
        match self {
            CellData::None => String::new(),
            CellData::Text(s) => s.clone(),
            CellData::Number(n) => n.to_string(),
            CellData::Boolean(b) => b.to_string(),
            CellData::Json(v) => v.to_string(),
            CellData::Binary(b) => {
                // Output as hex with warning
                format!("[Binary data: {} bytes]", b.len())
            }
        }
    }

    /// Check if data is empty/none
    pub fn is_none(&self) -> bool {
        matches!(self, CellData::None)
    }
}

/// Execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Run,    // Execute until completion
    Step,   // Execute one step, then pause
    DryRun, // Validate without executing
}

/// Execution status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    NotStarted,
    Running,
    Paused,
    Complete,
    DryRunComplete,
    Error(String),
}

/// Single execution log entry
#[derive(Debug, Clone)]
pub struct ExecutionLogEntry {
    pub step: usize,
    pub cell_id: Ulid,
    pub cell_name: Option<String>,
    pub output: CellData,
    pub dry_run: bool,
    pub error: Option<String>,
}

/// Complete execution report
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub status: ExecutionStatus,
    pub step: usize,
    pub log: Vec<ExecutionLogEntry>,
    pub total_cells_executed: usize,
}

/// Execution engine state
pub struct ExecutionEngine {
    /// Current execution mode
    mode: ExecutionMode,

    /// Current execution step
    current_step: usize,

    /// Cells queued for execution
    execution_queue: Vec<Ulid>,

    /// Cells executed in current step (for conflict detection)
    executed_this_step: HashSet<Ulid>,

    /// Execution log
    log: Vec<ExecutionLogEntry>,

    /// Cell outputs (stored between steps)
    cell_outputs: HashMap<Ulid, CellData>,

    /// Execution status
    status: ExecutionStatus,
}

impl ExecutionEngine {
    /// Create a new execution engine
    pub fn new(mode: ExecutionMode) -> Self {
        Self {
            mode,
            current_step: 0,
            execution_queue: Vec::new(),
            executed_this_step: HashSet::new(),
            log: Vec::new(),
            cell_outputs: HashMap::new(),
            status: ExecutionStatus::NotStarted,
        }
    }

    /// Start execution from the start cell
    pub fn execute(&mut self, canvas: &Canvas) -> Result<ExecutionReport> {
        // Find start cell
        let start_cell = canvas
            .get_start_point()
            .ok_or_else(|| anyhow!("No start point set"))?;

        self.status = ExecutionStatus::Running;
        self.execution_queue = vec![start_cell.id];
        self.current_step = 0;
        self.executed_this_step.clear();
        self.cell_outputs.clear();
        self.log.clear();

        // Execute until queue is empty or step mode pauses
        while !self.execution_queue.is_empty() {
            self.current_step += 1;

            // Execute one step
            if let Err(e) = self.execute_step(canvas) {
                self.status = ExecutionStatus::Error(e.to_string());
                return Err(e);
            }

            // In step mode, pause after each step
            if self.mode == ExecutionMode::Step {
                self.status = ExecutionStatus::Paused;
                return Ok(self.create_report());
            }
        }

        // Execution complete
        self.status = if matches!(self.mode, ExecutionMode::DryRun) {
            ExecutionStatus::DryRunComplete
        } else {
            ExecutionStatus::Complete
        };

        Ok(self.create_report())
    }

    /// Execute one step of the execution graph
    fn execute_step(&mut self, canvas: &Canvas) -> Result<()> {
        // Current step cells (sorted by ULID for deterministic order)
        let mut current_step_cells = self.execution_queue.clone();
        current_step_cells.sort();
        self.execution_queue.clear();
        self.executed_this_step.clear();

        for cell_id in current_step_cells {
            let cell = canvas
                .get_cell(cell_id)
                .ok_or_else(|| anyhow!("Cell not found: {}", cell_id))?;

            // Gather inputs from upstream cells
            let inputs = self.gather_inputs(canvas, cell_id);

            // Execute cell
            let output = if matches!(self.mode, ExecutionMode::DryRun) {
                // Dry-run: validate without executing
                self.validate_cell(cell, &inputs)?
            } else {
                self.execute_cell(canvas, cell, &inputs)?
            };

            // Log execution
            self.log.push(ExecutionLogEntry {
                step: self.current_step,
                cell_id,
                cell_name: cell.name.clone(),
                output: output.clone(),
                dry_run: matches!(self.mode, ExecutionMode::DryRun),
                error: None,
            });

            // Store output
            self.cell_outputs.insert(cell_id, output.clone());

            // Find downstream cells
            let downstream = canvas.get_outgoing_relationships(cell_id);

            for rel in downstream {
                let target_id = rel.to;

                // Conflict detection: check if target already executed this step
                if self.executed_this_step.contains(&target_id) {
                    return Err(anyhow!(
                        "Conflict: Cell {} written twice in step {}",
                        target_id,
                        self.current_step
                    ));
                }

                self.executed_this_step.insert(target_id);

                // Queue for next step
                if !self.execution_queue.contains(&target_id) {
                    self.execution_queue.push(target_id);
                }
            }
        }

        Ok(())
    }

    /// Gather inputs from upstream cells
    fn gather_inputs(&self, canvas: &Canvas, cell_id: Ulid) -> Vec<CellData> {
        let incoming = canvas.get_incoming_relationships(cell_id);
        incoming
            .iter()
            .filter_map(|rel| self.cell_outputs.get(&rel.from).cloned())
            .collect()
    }

    /// Execute a single cell
    fn execute_cell(&self, canvas: &Canvas, cell: &Cell, inputs: &[CellData]) -> Result<CellData> {
        match cell.cell_type {
            CellType::Text => execute_text_cell(cell, inputs),
            CellType::Python => execute_python_cell(canvas, cell, inputs),
            CellType::Math => execute_math_cell(canvas, cell),
            CellType::NumberInt | CellType::NumberFloat | CellType::NumberCurrency => {
                execute_number_cell(cell)
            }
        }
    }

    /// Validate a cell (dry-run mode)
    fn validate_cell(&self, cell: &Cell, inputs: &[CellData]) -> Result<CellData> {
        match cell.cell_type {
            CellType::Text => Ok(CellData::Text("(dry-run)".to_string())),
            CellType::Python => validate_python_cell(cell, inputs),
            CellType::Math => Ok(CellData::Text("(dry-run-math)".to_string())),
            CellType::NumberInt | CellType::NumberFloat | CellType::NumberCurrency => {
                Ok(CellData::Text("(dry-run-number)".to_string()))
            }
        }
    }

    /// Create execution report
    fn create_report(&self) -> ExecutionReport {
        ExecutionReport {
            status: self.status.clone(),
            step: self.current_step,
            log: self.log.clone(),
            total_cells_executed: self.log.len(),
        }
    }

    /// Get current status
    pub fn status(&self) -> &ExecutionStatus {
        &self.status
    }

    /// Get execution log
    pub fn log(&self) -> &[ExecutionLogEntry] {
        &self.log
    }

    /// Continue execution (for step mode)
    pub fn continue_execution(&mut self, canvas: &Canvas) -> Result<ExecutionReport> {
        if !matches!(self.status, ExecutionStatus::Paused) {
            return Err(anyhow!("Execution is not paused"));
        }

        self.status = ExecutionStatus::Running;

        // Execute until queue is empty or step mode pauses again
        while !self.execution_queue.is_empty() {
            self.current_step += 1;

            if let Err(e) = self.execute_step(canvas) {
                self.status = ExecutionStatus::Error(e.to_string());
                return Err(e);
            }

            // In step mode, pause after each step
            if self.mode == ExecutionMode::Step {
                self.status = ExecutionStatus::Paused;
                return Ok(self.create_report());
            }
        }

        // Execution complete
        self.status = if matches!(self.mode, ExecutionMode::DryRun) {
            ExecutionStatus::DryRunComplete
        } else {
            ExecutionStatus::Complete
        };

        Ok(self.create_report())
    }

    /// Recalculate Math cells that depend on a changed cell
    /// Returns the list of recalculated cells and any errors
    pub fn recalculate_dependents(
        &mut self,
        changed_cell_id: Ulid,
        canvas: &mut Canvas,
    ) -> Result<Vec<(Ulid, Result<f64, String>)>> {
        let mut results = Vec::new();

        // Get all Math cells that depend on the changed cell
        let dependents = crate::math_eval::get_dependent_math_cells(changed_cell_id, canvas);

        // Execute each dependent cell
        for cell_id in dependents {
            let cell = match canvas.get_cell(cell_id) {
                Some(c) => c,
                None => continue,
            };

            // Execute the math cell
            let result = match execute_math_cell(canvas, cell) {
                Ok(CellData::Number(value)) => {
                    // Get target cell ID before mutable borrow
                    let target_cell_id = canvas.get_cell(cell_id)
                        .and_then(|c| c.result_target_cell);
                    
                    // Store result in cell's computed_result
                    if let Some(cell_mut) = canvas.get_cell_mut(cell_id) {
                        cell_mut.computed_result = Some(value);
                    }

                    // If there's a result_target_cell, update it
                    if let Some(target_id) = target_cell_id {
                        if let Some(target_cell) = canvas.get_cell_mut(target_id) {
                            target_cell.computed_result = Some(value);
                            
                            // For Number type cells, also update their content
                            if matches!(target_cell.cell_type, CellType::NumberInt | CellType::NumberFloat | CellType::NumberCurrency) {
                                let formatted = match target_cell.cell_type {
                                    CellType::NumberInt => format!("{}", value as i64),
                                    CellType::NumberFloat => format!("{:.prec$}", value, prec = target_cell.decimal_precision as usize),
                                    CellType::NumberCurrency => format!("{:.prec$}", value, prec = target_cell.decimal_precision as usize),
                                    _ => value.to_string(),
                                };
                                target_cell.content = CellContent::inline(formatted);
                            }
                        }
                    }

                    // Store in outputs map
                    self.cell_outputs.insert(cell_id, CellData::Number(value));

                    Ok(value)
                }
                Ok(_) => Err("Math cell returned non-numeric result".to_string()),
                Err(e) => Err(e.to_string()),
            };

            results.push((cell_id, result));
        }

        Ok(results)
    }
}

/// Execute a text cell (display only, pass through inputs)
fn execute_text_cell(_cell: &Cell, inputs: &[CellData]) -> Result<CellData> {
    // Text cells just pass through their first input, or return None
    Ok(inputs.first().cloned().unwrap_or(CellData::None))
}

/// Execute a Python cell
fn execute_python_cell(_canvas: &Canvas, cell: &Cell, inputs: &[CellData]) -> Result<CellData> {
    let code = cell
        .content
        .as_str()
        .ok_or_else(|| anyhow!("Python cell has no inline content"))?;

    Python::with_gil(|py| {
        // Create globals dict for execution
        let globals = PyDict::new_bound(py);

        // Inject inputs as variables
        for (i, input) in inputs.iter().enumerate() {
            let py_value = celldata_to_python(py, input)?;
            let key = format!("input_{}", i);
            globals.set_item(key, py_value)?;
        }

        // Also add as 'inputs' list
        let mut inputs_list = Vec::new();
        for input in inputs.iter() {
            inputs_list.push(celldata_to_python(py, input)?);
        }
        globals.set_item("inputs", inputs_list)?;

        // Create output storage
        let output_dict = PyDict::new_bound(py);
        globals.set_item("__output__", &output_dict)?;

        // Add helper function for setting output
        let set_output_code = r#"
def set_output(value, key='result'):
    __output__[key] = value
"#;
        py.run_bound(set_output_code, Some(&globals), None)?;

        // Execute the cell code
        py.run_bound(code, Some(&globals), None)
            .map_err(|e| anyhow!("Python execution error: {}", e))?;

        // Extract output
        let output = output_dict
            .get_item("result")?
            .or_else(|| output_dict.get_item("output").ok().flatten())
            .or_else(|| output_dict.get_item("value").ok().flatten());

        match output {
            Some(val) => python_to_celldata(&val),
            None => Ok(CellData::None),
        }
    })
}

/// Validate Python cell (check syntax)
fn validate_python_cell(cell: &Cell, _inputs: &[CellData]) -> Result<CellData> {
    let code = cell
        .content
        .as_str()
        .ok_or_else(|| anyhow!("Python cell has no inline content"))?;

    Python::with_gil(|py| {
        // Try to compile the code to check for syntax errors
        py.run_bound(
            &format!("compile({:?}, '<cell>', 'exec')", code),
            None,
            None,
        )
        .map_err(|e| anyhow!("Python syntax error: {}", e))?;

        Ok(CellData::Text("(syntax valid)".to_string()))
    })
}

/// Convert CellData to Python object
fn celldata_to_python(py: Python, data: &CellData) -> PyResult<PyObject> {
    match data {
        CellData::None => Ok(py.None()),
        CellData::Text(s) => Ok(s.to_object(py)),
        CellData::Number(n) => Ok(n.to_object(py)),
        CellData::Boolean(b) => Ok(b.to_object(py)),
        CellData::Json(v) => {
            // Convert JSON to Python dict/list
            let json_str = v.to_string();
            let py_json = py.import_bound("json")?;
            let py_loads = py_json.getattr("loads")?;
            py_loads.call1((json_str,))?.extract()
        }
        CellData::Binary(b) => Ok(b.to_object(py)),
    }
}

/// Convert Python object to CellData
fn python_to_celldata(obj: &Bound<'_, PyAny>) -> Result<CellData> {
    // Try different conversions
    if obj.is_none() {
        Ok(CellData::None)
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(CellData::Text(s))
    } else if let Ok(n) = obj.extract::<f64>() {
        Ok(CellData::Number(n))
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(CellData::Boolean(b))
    } else if let Ok(bytes) = obj.extract::<Vec<u8>>() {
        Ok(CellData::Binary(bytes))
    } else {
        // Try to convert to JSON
        Python::with_gil(|py| {
            let py_json = py.import_bound("json")?;
            let py_dumps = py_json.getattr("dumps")?;
            let json_str: String = py_dumps.call1((obj,))?.extract()?;
            let json_value: serde_json::Value = serde_json::from_str(&json_str)?;
            Ok(CellData::Json(json_value))
        })
    }
}

/// Execute a Math cell - evaluate expression with cell references
fn execute_math_cell(canvas: &Canvas, cell: &Cell) -> Result<CellData> {
    // Get formula from cell content
    let formula = cell
        .content
        .as_str()
        .ok_or_else(|| anyhow!("Math cell has no inline content"))?;

    // Check for circular references
    if let Err(cycle) = crate::math_eval::detect_circular_references(cell.id, canvas) {
        return Err(anyhow!("Circular reference detected: {}", cycle.join(" → ")));
    }

    // Evaluate the expression
    let result = crate::math_eval::evaluate_expression(formula, canvas)
        .map_err(|e| anyhow!("Math evaluation error in cell {}: {}", cell.short_id, e))?;

    Ok(CellData::Number(result))
}

/// Execute a Number cell - parse value from content
fn execute_number_cell(cell: &Cell) -> Result<CellData> {
    let content = cell
        .content
        .as_str()
        .ok_or_else(|| anyhow!("Number cell has no inline content"))?;

    let value = content
        .trim()
        .parse::<f64>()
        .map_err(|e| anyhow!("Cannot parse number from cell {}: {}", cell.short_id, e))?;

    Ok(CellData::Number(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CellContent, Rectangle};

    #[test]
    fn test_celldata_coerce_to_string() {
        assert_eq!(CellData::None.coerce_to_string(), "");
        assert_eq!(CellData::Text("hello".to_string()).coerce_to_string(), "hello");
        assert_eq!(CellData::Number(42.0).coerce_to_string(), "42");
        assert_eq!(CellData::Boolean(true).coerce_to_string(), "true");
    }

    #[test]
    fn test_text_cell_execution() {
        let cell = Cell::new(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Test"),
            "A1".to_string(),
        );

        let inputs = vec![CellData::Text("input data".to_string())];
        let output = execute_text_cell(&cell, &inputs).unwrap();

        assert_eq!(output, CellData::Text("input data".to_string()));
    }

    #[test]
    fn test_python_cell_simple() {
        let mut canvas = Canvas::new();
        let cell_id = canvas.create_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("set_output(42)"),
        );

        let cell = canvas.get_cell(cell_id).unwrap();
        let output = execute_python_cell(&canvas, cell, &[]).unwrap();

        assert_eq!(output, CellData::Number(42.0));
    }

    #[test]
    fn test_python_cell_with_inputs() {
        let mut canvas = Canvas::new();
        let cell_id = canvas.create_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("set_output(input_0 * 2)"),
        );

        let cell = canvas.get_cell(cell_id).unwrap();
        let inputs = vec![CellData::Number(21.0)];
        let output = execute_python_cell(&canvas, cell, &inputs).unwrap();

        assert_eq!(output, CellData::Number(42.0));
    }

    #[test]
    fn test_execution_engine_simple() {
        let mut canvas = Canvas::with_root_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("set_output(100)"),
        );

        let root = canvas.root_cell().unwrap();
        canvas.set_start_point(root).unwrap();

        let mut engine = ExecutionEngine::new(ExecutionMode::Run);
        let report = engine.execute(&canvas).unwrap();

        assert_eq!(report.status, ExecutionStatus::Complete);
        assert_eq!(report.total_cells_executed, 1);
        assert_eq!(report.log[0].output, CellData::Number(100.0));
    }

    #[test]
    fn test_execution_engine_with_relationship() {
        let mut canvas = Canvas::new();

        let cell1 = canvas.create_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("set_output(10)"),
        );

        let cell2 = canvas.create_cell(
            CellType::Python,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("set_output(input_0 * 2)"),
        );

        canvas.create_relationship(cell1, cell2).unwrap();
        canvas.set_start_point(cell1).unwrap();

        let mut engine = ExecutionEngine::new(ExecutionMode::Run);
        let report = engine.execute(&canvas).unwrap();

        assert_eq!(report.status, ExecutionStatus::Complete);
        assert_eq!(report.total_cells_executed, 2);
        assert_eq!(report.log[1].output, CellData::Number(20.0));
    }

    #[test]
    fn test_dry_run_mode() {
        let mut canvas = Canvas::with_root_cell(
            CellType::Python,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("set_output(42)"),
        );

        let root = canvas.root_cell().unwrap();
        canvas.set_start_point(root).unwrap();

        let mut engine = ExecutionEngine::new(ExecutionMode::DryRun);
        let report = engine.execute(&canvas).unwrap();

        assert_eq!(report.status, ExecutionStatus::DryRunComplete);
        assert!(report.log[0].dry_run);
    }
}
