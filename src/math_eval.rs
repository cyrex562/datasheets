use crate::{Canvas, Cell, CellType};
use evalexpr::*;
use std::collections::HashSet;
use ulid::Ulid;

/// Parse a formula to extract cell references in [[cell_id]] format
pub fn parse_formula_references(formula: &str) -> Vec<String> {
    let links = crate::markdown_links::parse_cell_links(formula);
    links.iter().map(|link| link.target_id.clone()).collect()
}

/// Resolve a cell to its numeric value
pub fn resolve_cell_value(cell: &Cell, _canvas: &Canvas) -> Result<f64, String> {
    // First check if there's a cached computed result
    if let Some(result) = cell.computed_result {
        return Ok(result);
    }

    // For Number type cells, parse the content
    match cell.cell_type {
        CellType::NumberInt | CellType::NumberFloat | CellType::NumberCurrency => {
            if let Some(content) = cell.content.as_str() {
                content
                    .trim()
                    .parse::<f64>()
                    .map_err(|e| format!("Cannot parse number from cell {}: {}", cell.short_id, e))
            } else {
                Err(format!("Cell {} has no inline content", cell.short_id))
            }
        }
        CellType::Math => {
            // Math cells should have computed_result set by execution
            Err(format!(
                "Math cell {} has not been computed yet",
                cell.short_id
            ))
        }
        _ => Err(format!(
            "Cell {} is not a numeric type (type: {:?})",
            cell.short_id, cell.cell_type
        )),
    }
}

/// Build an evalexpr context with cell references resolved to their values
pub fn build_eval_context(formula: &str, canvas: &Canvas) -> Result<HashMapContext, String> {
    let mut context = HashMapContext::new();

    // Extract cell references
    let references = parse_formula_references(formula);

    // Resolve each reference to its value
    for ref_id in references {
        // Look up the cell by short_id
        let cell_id = canvas
            .get_cell_id_by_short_id(&ref_id)
            .ok_or_else(|| format!("Cell {} not found", ref_id))?;

        let cell = canvas
            .get_cell(cell_id)
            .ok_or_else(|| format!("Cell {} not found", ref_id))?;

        // Resolve the cell's value
        let value = resolve_cell_value(cell, canvas)?;

        // Add to context - use "cell_" prefix to avoid numeric literal interpretation
        let var_name = format!("cell_{}", ref_id);
        context
            .set_value(var_name.clone(), value.into())
            .map_err(|e| format!("Failed to set variable {}: {}", var_name, e))?;
    }

    Ok(context)
}

/// Replace [[cell_id]] references in formula with safe variable names for evalexpr
pub fn prepare_formula(formula: &str) -> String {
    // Replace [[cell_id]] with cell_cell_id to avoid numeric literal interpretation
    // For example: [[06]] becomes cell_06 (not just 06 which would be octal)
    let links = crate::markdown_links::parse_cell_links(formula);

    if links.is_empty() {
        return formula.to_string();
    }

    let mut result = String::new();
    let mut last_end = 0;

    // Process each link in order
    for link in links.iter() {
        // Add the text before this link
        result.push_str(&formula[last_end..link.start]);

        // Add the replacement (cell_ prefix)
        result.push_str(&format!("cell_{}", link.target_id));

        last_end = link.end;
    }

    // Add any remaining text after the last link
    result.push_str(&formula[last_end..]);

    result
}

/// Evaluate a mathematical expression with cell references
pub fn evaluate_expression(formula: &str, canvas: &Canvas) -> Result<f64, String> {
    // Build context with cell values
    let context = build_eval_context(formula, canvas)?;

    // Prepare formula by removing [[ ]] markers
    let prepared_formula = prepare_formula(formula);

    // Evaluate the expression
    let result = eval_with_context(&prepared_formula, &context)
        .map_err(|e| format!("Evaluation error: {}", e))?;

    // Convert to number
    match result {
        Value::Float(f) => Ok(f),
        Value::Int(i) => Ok(i as f64),
        _ => Err("Result is not a number".to_string()),
    }
}

/// Detect circular references in math cell dependencies
/// Returns Ok(()) if no cycles, or Err with the cycle path
pub fn detect_circular_references(cell_id: Ulid, canvas: &Canvas) -> Result<(), Vec<String>> {
    let mut visited = HashSet::new();
    let mut path = Vec::new();

    fn visit(
        current_id: Ulid,
        canvas: &Canvas,
        visited: &mut HashSet<Ulid>,
        path: &mut Vec<String>,
    ) -> Result<(), Vec<String>> {
        // If we've seen this cell in our current path, we have a cycle
        if visited.contains(&current_id) {
            // Find where the cycle starts in the path
            let cell = canvas.get_cell(current_id).unwrap();
            path.push(cell.short_id.clone());
            return Err(path.clone());
        }

        let cell = match canvas.get_cell(current_id) {
            Some(c) => c,
            None => return Ok(()), // Cell doesn't exist, no cycle
        };

        // Only check Math cells for dependencies
        if cell.cell_type != CellType::Math {
            return Ok(());
        }

        // Add to visited set and path
        visited.insert(current_id);
        path.push(cell.short_id.clone());

        // Get formula content
        if let Some(formula) = cell.content.as_str() {
            // Extract referenced cells
            let references = parse_formula_references(formula);

            for ref_id in references {
                // Look up the referenced cell
                if let Some(ref_cell_id) = canvas.get_cell_id_by_short_id(&ref_id) {
                    // Recursively check for cycles
                    visit(ref_cell_id, canvas, visited, path)?;
                }
            }
        }

        // Remove from visited set and path (backtrack)
        visited.remove(&current_id);
        path.pop();

        Ok(())
    }

    visit(cell_id, canvas, &mut visited, &mut path)
}

/// Get all cells that depend on the given cell (directly or indirectly)
pub fn get_dependent_math_cells(cell_id: Ulid, canvas: &Canvas) -> Vec<Ulid> {
    let mut dependents = Vec::new();
    let cell = match canvas.get_cell(cell_id) {
        Some(c) => c,
        None => return dependents,
    };

    // Search all Math cells for references to this cell
    for (id, other_cell) in canvas.cells() {
        if other_cell.cell_type != CellType::Math {
            continue;
        }

        if let Some(formula) = other_cell.content.as_str() {
            let references = parse_formula_references(formula);
            if references.contains(&cell.short_id) {
                dependents.push(*id);
            }
        }
    }

    dependents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Cell, CellContent, CellType, Rectangle};

    #[test]
    fn test_parse_formula_references() {
        let formula = "[[A7]] + [[B2]] * 2";
        let refs = parse_formula_references(formula);
        assert_eq!(refs, vec!["A7", "B2"]);
    }

    #[test]
    fn test_prepare_formula() {
        let formula = "[[A7]] + [[B2]] * 2";
        let prepared = prepare_formula(formula);
        // Formula now uses cell_ prefix to avoid numeric literal interpretation
        assert_eq!(prepared, "cell_A7 + cell_B2 * 2");
    }

    #[test]
    fn test_simple_evaluation() {
        let mut canvas = Canvas::new();

        // Create a number cell
        let cell_a = canvas.create_cell(
            CellType::NumberFloat,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("42"),
        );

        // Update the short_id to A7 for testing
        if let Some(cell) = canvas.get_cell_mut(cell_a) {
            cell.short_id = "A7".to_string();
        }

        // Evaluate expression
        let result = evaluate_expression("[[A7]] * 2", &canvas).unwrap();
        assert_eq!(result, 84.0);
    }

    // ============ Circular Reference Detection Tests ============

    #[test]
    fn test_self_reference_detected() {
        let mut canvas = Canvas::new();

        let cell = canvas.create_cell(
            CellType::Math,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline(""),
        );

        let cell_short = canvas.get_cell(cell).unwrap().short_id.clone();
        let _ =
            canvas.update_cell_content(cell, CellContent::inline(&format!("[[{}]]+1", cell_short)));

        // Should detect self-reference
        let result = detect_circular_references(cell, &canvas);
        assert!(result.is_err());
        let cycle = result.unwrap_err();
        // Cycle should contain the cell (might appear once or twice depending on detection algorithm)
        assert!(!cycle.is_empty());
        assert!(cycle.contains(&cell_short));
    }

    #[test]
    fn test_two_cell_cycle_detected() {
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

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
        let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();

        // A → B
        canvas.update_cell_content(
            cell_a,
            CellContent::inline(&format!("[[{}]]", cell_b_short)),
        );
        // B → A (creates cycle)
        canvas.update_cell_content(
            cell_b,
            CellContent::inline(&format!("[[{}]]", cell_a_short)),
        );

        // Detect cycle from A
        let result = detect_circular_references(cell_a, &canvas);
        assert!(result.is_err());
        let cycle = result.unwrap_err();
        assert!(cycle.len() >= 2);
        assert!(cycle.contains(&cell_a_short));
        assert!(cycle.contains(&cell_b_short));
    }

    #[test]
    fn test_three_cell_cycle_detected() {
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

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
        let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();
        let cell_c_short = canvas.get_cell(cell_c).unwrap().short_id.clone();

        // A → B → C → A
        canvas.update_cell_content(
            cell_a,
            CellContent::inline(&format!("[[{}]]", cell_b_short)),
        );
        canvas.update_cell_content(
            cell_b,
            CellContent::inline(&format!("[[{}]]", cell_c_short)),
        );
        canvas.update_cell_content(
            cell_c,
            CellContent::inline(&format!("[[{}]]", cell_a_short)),
        );

        let result = detect_circular_references(cell_a, &canvas);
        assert!(result.is_err());
        let cycle = result.unwrap_err();
        assert!(cycle.len() >= 3);
        assert!(cycle.contains(&cell_a_short));
        assert!(cycle.contains(&cell_b_short));
        assert!(cycle.contains(&cell_c_short));
    }

    #[test]
    fn test_four_cell_cycle_detected() {
        let mut canvas = Canvas::new();

        let cells: Vec<_> = (0..4)
            .map(|i| {
                canvas.create_cell(
                    CellType::Math,
                    Rectangle::new(i as f32 * 150.0, 0.0, 100.0, 100.0),
                    CellContent::inline(""),
                )
            })
            .collect();

        let short_ids: Vec<_> = cells
            .iter()
            .map(|&id| canvas.get_cell(id).unwrap().short_id.clone())
            .collect();

        // Create cycle: 0 → 1 → 2 → 3 → 0
        for i in 0..4 {
            let next_idx = (i + 1) % 4;
            canvas.update_cell_content(
                cells[i],
                CellContent::inline(&format!("[[{}]]", short_ids[next_idx])),
            );
        }

        let result = detect_circular_references(cells[0], &canvas);
        assert!(result.is_err());
        let cycle = result.unwrap_err();
        assert!(cycle.len() >= 4);
    }

    #[test]
    fn test_cycle_with_unused_cells() {
        let mut canvas = Canvas::new();

        // Create cycle A → B → A
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

        // Create unrelated cell C (not in cycle)
        let _cell_c = canvas.create_cell(
            CellType::Math,
            Rectangle::new(300.0, 0.0, 100.0, 100.0),
            CellContent::inline("10 + 20"),
        );

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
        let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();

        canvas.update_cell_content(
            cell_a,
            CellContent::inline(&format!("[[{}]]", cell_b_short)),
        );
        canvas.update_cell_content(
            cell_b,
            CellContent::inline(&format!("[[{}]]", cell_a_short)),
        );

        // Cycle should still be detected
        let result = detect_circular_references(cell_a, &canvas);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_cycle_linear_chain() {
        let mut canvas = Canvas::new();

        let cell_a = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("10"),
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

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
        let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();

        // Linear: A ← B ← C (no cycle)
        canvas.update_cell_content(
            cell_b,
            CellContent::inline(&format!("[[{}]]", cell_a_short)),
        );
        canvas.update_cell_content(
            cell_c,
            CellContent::inline(&format!("[[{}]]", cell_b_short)),
        );

        // No cycle should be detected
        let result = detect_circular_references(cell_c, &canvas);
        assert!(result.is_ok());
    }

    // ============ Math Evaluation Tests ============

    #[test]
    fn test_complex_formula_multiple_references() {
        let mut canvas = Canvas::new();

        let cell_a = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("10"),
        );

        let cell_b = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("20"),
        );

        let cell_c = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(300.0, 0.0, 100.0, 100.0),
            CellContent::inline("5"),
        );

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
        let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();
        let cell_c_short = canvas.get_cell(cell_c).unwrap().short_id.clone();

        let formula = format!(
            "([[{}]] + [[{}]]) * [[{}]]",
            cell_a_short, cell_b_short, cell_c_short
        );
        let result = evaluate_expression(&formula, &canvas).unwrap();
        assert_eq!(result, 150.0); // (10 + 20) * 5 = 150
    }

    #[test]
    fn test_division_by_zero_error() {
        let mut canvas = Canvas::new();

        let cell_a = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("10"),
        );

        let cell_b = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("0"),
        );

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
        let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();

        let formula = format!("[[{}]] / [[{}]]", cell_a_short, cell_b_short);
        let result = evaluate_expression(&formula, &canvas);

        // Division by zero should produce infinity or error
        assert!(result.is_ok()); // evalexpr handles this as infinity
    }

    #[test]
    fn test_missing_cell_reference_error() {
        let canvas = Canvas::new();

        let formula = "[[NONEXISTENT]] + 10";
        let result = evaluate_expression(formula, &canvas);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_syntax_error() {
        let mut canvas = Canvas::new();

        let cell_a = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("10"),
        );

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();

        let formula = format!("[[{}]] + + 10", cell_a_short); // Double plus is invalid
        let result = evaluate_expression(&formula, &canvas);
        assert!(result.is_err());
    }

    #[test]
    fn test_evalexpr_power_function() {
        let mut canvas = Canvas::new();

        let cell_a = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("2"),
        );

        let cell_b = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("3"),
        );

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
        let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();

        // Test power operation (basic math operator that should work)
        let formula = format!("[[{}]] ^ [[{}]]", cell_a_short, cell_b_short);
        let result = evaluate_expression(&formula, &canvas).unwrap();
        assert_eq!(result, 8.0); // 2^3 = 8
    }

    #[test]
    fn test_evalexpr_modulo_function() {
        let mut canvas = Canvas::new();

        let cell_a = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("10"),
        );

        let cell_b = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("3"),
        );

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();
        let cell_b_short = canvas.get_cell(cell_b).unwrap().short_id.clone();

        // Test modulo operation
        let formula = format!("[[{}]] % [[{}]]", cell_a_short, cell_b_short);
        let result = evaluate_expression(&formula, &canvas).unwrap();
        assert_eq!(result, 1.0); // 10 % 3 = 1
    }

    #[test]
    fn test_empty_formula() {
        let canvas = Canvas::new();
        let result = evaluate_expression("", &canvas);
        assert!(result.is_err());
    }

    #[test]
    fn test_whitespace_formula() {
        let canvas = Canvas::new();
        let result = evaluate_expression("   \n\t  ", &canvas);
        assert!(result.is_err());
    }

    #[test]
    fn test_formula_with_constants_and_references() {
        let mut canvas = Canvas::new();

        let cell_a = canvas.create_cell(
            CellType::NumberInt,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("10"),
        );

        let cell_a_short = canvas.get_cell(cell_a).unwrap().short_id.clone();

        let formula = format!("[[{}]] * 2 + 5", cell_a_short);
        let result = evaluate_expression(&formula, &canvas).unwrap();
        assert_eq!(result, 25.0); // 10 * 2 + 5 = 25
    }
}
