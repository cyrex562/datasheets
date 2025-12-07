use crate::{Canvas, CellContent, CellType, Project, Rectangle, SplitDirection};
use anyhow::Result;
use egui::{
    epaint::PathShape, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke,
    Vec2,
};
use std::path::PathBuf;
use ulid::Ulid;

/// Main application state
pub struct GraphCellEditorApp {
    /// The canvas containing all cells and relationships
    canvas: Canvas,

    /// Currently selected cell
    selected_cell: Option<Ulid>,

    /// Canvas viewport offset (for panning)
    canvas_offset: Vec2,

    /// Canvas zoom level
    zoom: f32,

    /// Project path (if loaded)
    project_path: Option<PathBuf>,

    /// UI state
    ui_state: UiState,

    /// Status message
    status_message: String,
}

#[derive(Default)]
struct UiState {
    /// Mode for relationship creation
    relationship_mode: RelationshipMode,

    /// First cell selected for relationship creation
    relationship_source: Option<Ulid>,

    /// Show grid
    show_grid: bool,

    /// Show cell IDs
    show_cell_ids: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum RelationshipMode {
    #[default]
    None,
    SelectingSource,
    SelectingTarget,
}

impl Default for GraphCellEditorApp {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCellEditorApp {
    pub fn new() -> Self {
        // Create initial canvas with a root cell
        let canvas = Canvas::with_root_cell(
            CellType::Text,
            Rectangle::new(50.0, 50.0, 400.0, 300.0),
            CellContent::inline("Root Cell\n\nDouble-click to edit, or use the toolbar to split."),
        );

        Self {
            canvas,
            selected_cell: None,
            canvas_offset: Vec2::ZERO,
            zoom: 1.0,
            project_path: None,
            ui_state: UiState {
                show_grid: true,
                show_cell_ids: false,
                ..Default::default()
            },
            status_message: "Welcome to Graph Cell Editor!".to_string(),
        }
    }

    /// Create app from a project
    pub fn from_project(project: &Project) -> Result<Self> {
        let (manifest, canvas) = project.load()?;

        Ok(Self {
            canvas,
            selected_cell: manifest.start_cell,
            canvas_offset: Vec2::ZERO,
            zoom: 1.0,
            project_path: Some(project.root_dir().to_path_buf()),
            ui_state: UiState {
                show_grid: true,
                show_cell_ids: false,
                ..Default::default()
            },
            status_message: format!("Loaded project from {}", project.root_dir().display()),
        })
    }

    /// Save current project
    fn save_project(&mut self) {
        if let Some(path) = &self.project_path {
            match Project::open(path) {
                Ok(project) => {
                    if let Err(e) = project.save(&self.canvas) {
                        self.status_message = format!("❌ Save failed: {}", e);
                    } else {
                        self.status_message = "✓ Project saved".to_string();
                    }
                }
                Err(e) => {
                    self.status_message = format!("❌ Failed to open project: {}", e);
                }
            }
        } else {
            self.status_message = "⚠ No project loaded. Use File > Save As...".to_string();
        }
    }

    /// Render the entire UI
    fn render_ui(&mut self, ctx: &egui::Context) {
        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save").clicked() {
                        self.save_project();
                        ui.close_menu();
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.ui_state.show_grid, "Show Grid");
                    ui.checkbox(&mut self.ui_state.show_cell_ids, "Show Cell IDs");
                    if ui.button("Reset Zoom").clicked() {
                        self.zoom = 1.0;
                        self.canvas_offset = Vec2::ZERO;
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    ui.label("Graph Cell Editor - Phase 3");
                    ui.label("Version 0.1.0");
                    ui.separator();
                    ui.label("Click to select cells");
                    ui.label("Right-click for context menu");
                    ui.label("Scroll to zoom");
                });
            });
        });

        // Toolbar
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("➗ Split H").clicked() {
                    self.split_selected_cell(SplitDirection::Horizontal);
                }
                if ui.button("➗ Split V").clicked() {
                    self.split_selected_cell(SplitDirection::Vertical);
                }
                ui.separator();

                let relation_button = ui.button("🔗 Create Relationship");
                if relation_button.clicked() {
                    match self.ui_state.relationship_mode {
                        RelationshipMode::None => {
                            self.ui_state.relationship_mode = RelationshipMode::SelectingSource;
                            self.ui_state.relationship_source = None;
                            self.status_message = "Select source cell for relationship".to_string();
                        }
                        _ => {
                            self.ui_state.relationship_mode = RelationshipMode::None;
                            self.ui_state.relationship_source = None;
                            self.status_message = "Relationship creation cancelled".to_string();
                        }
                    }
                }

                ui.separator();

                ui.label(format!("Cells: {}", self.canvas.cell_count()));
                ui.label(format!("Relationships: {}", self.canvas.relationship_count()));
                ui.label(format!("Zoom: {:.0}%", self.zoom * 100.0));
            });
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_message);
            });
        });

        // Right panel (properties)
        egui::SidePanel::right("properties_panel")
            .default_width(300.0)
            .show(ctx, |ui| {
                self.render_properties_panel(ui);
            });

        // Central panel (canvas)
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_canvas(ui);
        });
    }

    /// Render the properties panel
    fn render_properties_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Properties");
        ui.separator();

        if let Some(cell_id) = self.selected_cell {
            // Clone all data we need before any mutations
            let cell_data = self.canvas.get_cell(cell_id).map(|cell| {
                (
                    cell.name.clone(),
                    cell.cell_type,
                    cell.content.clone(),
                    cell.is_start_point,
                    cell.bounds,
                )
            });

            if let Some((name_opt, cell_type_orig, content, is_start_orig, bounds)) = cell_data {
                ui.label(format!("Cell ID: {}", cell_id));
                ui.separator();

                // Cell name
                ui.label("Name:");
                let mut name = name_opt.unwrap_or_default();
                if ui.text_edit_singleline(&mut name).changed() {
                    let _ = self
                        .canvas
                        .rename_cell(cell_id, if name.is_empty() { None } else { Some(name) });
                }

                ui.separator();

                // Cell type
                ui.label("Type:");
                let mut cell_type = cell_type_orig;
                egui::ComboBox::from_label("")
                    .selected_text(format!("{:?}", cell_type))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut cell_type, CellType::Text, "Text");
                        ui.selectable_value(&mut cell_type, CellType::Python, "Python");
                    });

                if cell_type != cell_type_orig {
                    let _ = self.canvas.update_cell_type(cell_id, cell_type);
                }

                ui.separator();

                // Content
                ui.label("Content:");
                if let Some(content_str) = content.as_str() {
                    let mut content_edit = content_str.to_string();
                    if ui.text_edit_multiline(&mut content_edit).changed() {
                        let _ = self
                            .canvas
                            .update_cell_content(cell_id, CellContent::inline(content_edit));
                    }
                }

                ui.separator();

                // Start point checkbox
                let mut is_start = is_start_orig;
                if ui.checkbox(&mut is_start, "Start Point").changed() {
                    if is_start {
                        let _ = self.canvas.set_start_point(cell_id);
                    }
                }

                ui.separator();

                // Bounds info
                ui.label(format!("Position: ({:.0}, {:.0})", bounds.x, bounds.y));
                ui.label(format!("Size: {:.0} × {:.0}", bounds.width, bounds.height));

                ui.separator();

                // Relationships
                ui.label("Outgoing Relationships:");
                let outgoing = self.canvas.get_outgoing_relationships(cell_id);
                if outgoing.is_empty() {
                    ui.label("  (none)");
                } else {
                    for rel in outgoing {
                        if let Some(target) = self.canvas.get_cell(rel.to) {
                            let target_name = target
                                .name
                                .clone()
                                .unwrap_or_else(|| format!("{}", rel.to));
                            ui.label(format!("  → {}", target_name));
                        }
                    }
                }

                ui.label("Incoming Relationships:");
                let incoming = self.canvas.get_incoming_relationships(cell_id);
                if incoming.is_empty() {
                    ui.label("  (none)");
                } else {
                    for rel in incoming {
                        if let Some(source) = self.canvas.get_cell(rel.from) {
                            let source_name = source
                                .name
                                .clone()
                                .unwrap_or_else(|| format!("{}", rel.from));
                            ui.label(format!("  ← {}", source_name));
                        }
                    }
                }
            }
        } else {
            ui.label("No cell selected");
            ui.separator();
            ui.label("Click on a cell to view its properties");
        }
    }

    /// Render the canvas with cells and relationships
    fn render_canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size(), Sense::click_and_drag());

        let canvas_rect = response.rect;

        // Handle zoom with scroll
        if response.hovered() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                let zoom_delta = scroll_delta * 0.001;
                self.zoom = (self.zoom + zoom_delta).clamp(0.1, 5.0);
            }
        }

        // Handle panning with drag
        if response.dragged() {
            self.canvas_offset += response.drag_delta();
        }

        // Draw grid (optional)
        if self.ui_state.show_grid {
            self.draw_grid(&painter, canvas_rect);
        }

        // Draw all cells
        let cells: Vec<_> = self.canvas.cells().values().cloned().collect();
        for cell in &cells {
            self.draw_cell(&painter, canvas_rect, cell, &response);
        }

        // Draw all relationships
        let relationships: Vec<_> = self.canvas.relationships().values().cloned().collect();
        for rel in &relationships {
            self.draw_relationship(&painter, canvas_rect, &rel);
        }

        // Handle cell selection
        if response.clicked() {
            let click_pos = response.interact_pointer_pos().unwrap();
            let canvas_pos = self.screen_to_canvas(click_pos, canvas_rect);

            let mut clicked_cell = None;
            for cell in &cells {
                if self.is_point_in_cell(canvas_pos, &cell.bounds) {
                    clicked_cell = Some(cell.id);
                    break;
                }
            }

            // Handle relationship mode
            match self.ui_state.relationship_mode {
                RelationshipMode::SelectingSource => {
                    if let Some(cell_id) = clicked_cell {
                        self.ui_state.relationship_source = Some(cell_id);
                        self.ui_state.relationship_mode = RelationshipMode::SelectingTarget;
                        self.status_message = "Now select target cell for relationship".to_string();
                    }
                }
                RelationshipMode::SelectingTarget => {
                    if let Some(target_id) = clicked_cell {
                        if let Some(source_id) = self.ui_state.relationship_source {
                            if source_id != target_id {
                                match self.canvas.create_relationship(source_id, target_id) {
                                    Ok(_) => {
                                        self.status_message =
                                            "✓ Relationship created".to_string();
                                    }
                                    Err(e) => {
                                        self.status_message = format!("❌ Error: {}", e);
                                    }
                                }
                            } else {
                                self.status_message =
                                    "❌ Cannot create self-referential relationship".to_string();
                            }
                        }
                        self.ui_state.relationship_mode = RelationshipMode::None;
                        self.ui_state.relationship_source = None;
                    }
                }
                RelationshipMode::None => {
                    self.selected_cell = clicked_cell;
                    if let Some(cell_id) = clicked_cell {
                        if let Some(cell) = self.canvas.get_cell(cell_id) {
                            let name = cell
                                .name
                                .clone()
                                .unwrap_or_else(|| format!("{}", cell_id));
                            self.status_message = format!("Selected: {}", name);
                        }
                    } else {
                        self.status_message = "No cell selected".to_string();
                    }
                }
            }
        }
    }

    /// Draw a cell on the canvas
    fn draw_cell(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        cell: &crate::Cell,
        _response: &Response,
    ) {
        let screen_rect = self.canvas_to_screen_rect(&cell.bounds, canvas_rect);

        // Determine cell color based on type and selection
        let (fill_color, stroke_color, stroke_width) = if Some(cell.id) == self.selected_cell {
            (Color32::from_rgb(220, 240, 255), Color32::BLUE, 3.0)
        } else {
            let fill = match cell.cell_type {
                CellType::Text => Color32::from_rgb(240, 240, 240),
                CellType::Python => Color32::from_rgb(230, 255, 230),
            };
            (fill, Color32::DARK_GRAY, 2.0)
        };

        // Special highlight for start point
        let final_stroke_color = if cell.is_start_point {
            Color32::from_rgb(255, 140, 0) // Orange for start point
        } else {
            stroke_color
        };

        // Draw cell rectangle
        painter.rect(
            screen_rect,
            4.0, // rounding
            fill_color,
            Stroke::new(stroke_width, final_stroke_color),
        );

        // Draw cell name (if present) or ID
        let label = if let Some(name) = &cell.name {
            name.clone()
        } else if self.ui_state.show_cell_ids {
            format!("{}", cell.id)
        } else {
            String::new()
        };

        if !label.is_empty() {
            painter.text(
                screen_rect.left_top() + vec2(5.0, 5.0),
                Align2::LEFT_TOP,
                &label,
                FontId::proportional(14.0),
                Color32::DARK_GRAY,
            );
        }

        // Draw content preview (first few lines)
        if let Some(content) = cell.content.as_str() {
            let preview: String = content.lines().take(3).collect::<Vec<_>>().join("\n");
            let preview = if preview.len() > 50 {
                format!("{}...", &preview[..50])
            } else {
                preview
            };

            painter.text(
                screen_rect.left_top() + vec2(5.0, 25.0),
                Align2::LEFT_TOP,
                &preview,
                FontId::monospace(12.0),
                Color32::BLACK,
            );
        }

        // Draw start point indicator
        if cell.is_start_point {
            painter.text(
                screen_rect.right_top() + vec2(-20.0, 5.0),
                Align2::LEFT_TOP,
                "⭐",
                FontId::proportional(16.0),
                Color32::from_rgb(255, 140, 0),
            );
        }
    }

    /// Draw a relationship arrow
    fn draw_relationship(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        rel: &crate::Relationship,
    ) {
        let from_cell = match self.canvas.get_cell(rel.from) {
            Some(c) => c,
            None => return,
        };
        let to_cell = match self.canvas.get_cell(rel.to) {
            Some(c) => c,
            None => return,
        };

        let from_rect = self.canvas_to_screen_rect(&from_cell.bounds, canvas_rect);
        let to_rect = self.canvas_to_screen_rect(&to_cell.bounds, canvas_rect);

        let from_center = from_rect.center();
        let to_center = to_rect.center();

        // Draw arrow
        let arrow_color = Color32::from_rgb(0, 100, 200);
        let stroke = Stroke::new(2.0, arrow_color);

        painter.line_segment([from_center, to_center], stroke);

        // Draw arrowhead
        let dir = (to_center - from_center).normalized();
        let perpendicular = vec2(-dir.y, dir.x);
        let arrow_size = 10.0;
        let arrow_tip = to_center - dir * to_rect.width().min(to_rect.height()) * 0.5;

        let arrow_point1 = arrow_tip - dir * arrow_size + perpendicular * arrow_size * 0.5;
        let arrow_point2 = arrow_tip - dir * arrow_size - perpendicular * arrow_size * 0.5;

        let arrow_shape = PathShape::convex_polygon(
            vec![arrow_tip, arrow_point1, arrow_point2],
            arrow_color,
            stroke,
        );
        painter.add(arrow_shape);
    }

    /// Draw grid
    fn draw_grid(&self, painter: &egui::Painter, canvas_rect: Rect) {
        let grid_spacing = 50.0 * self.zoom;
        let grid_color = Color32::from_gray(200);

        // Vertical lines
        let mut x = (canvas_rect.left() - self.canvas_offset.x) % grid_spacing;
        while x < canvas_rect.right() {
            painter.line_segment(
                [pos2(x, canvas_rect.top()), pos2(x, canvas_rect.bottom())],
                Stroke::new(1.0, grid_color),
            );
            x += grid_spacing;
        }

        // Horizontal lines
        let mut y = (canvas_rect.top() - self.canvas_offset.y) % grid_spacing;
        while y < canvas_rect.bottom() {
            painter.line_segment(
                [pos2(canvas_rect.left(), y), pos2(canvas_rect.right(), y)],
                Stroke::new(1.0, grid_color),
            );
            y += grid_spacing;
        }
    }

    /// Convert canvas coordinates to screen coordinates
    fn canvas_to_screen(&self, pos: Pos2, canvas_rect: Rect) -> Pos2 {
        canvas_rect.left_top()
            + vec2(
                pos.x * self.zoom + self.canvas_offset.x,
                pos.y * self.zoom + self.canvas_offset.y,
            )
    }

    /// Convert screen coordinates to canvas coordinates
    fn screen_to_canvas(&self, pos: Pos2, canvas_rect: Rect) -> Pos2 {
        let relative = pos - canvas_rect.left_top();
        pos2(
            (relative.x - self.canvas_offset.x) / self.zoom,
            (relative.y - self.canvas_offset.y) / self.zoom,
        )
    }

    /// Convert canvas rectangle to screen rectangle
    fn canvas_to_screen_rect(&self, rect: &Rectangle, canvas_rect: Rect) -> Rect {
        let top_left = self.canvas_to_screen(pos2(rect.x, rect.y), canvas_rect);
        let bottom_right =
            self.canvas_to_screen(pos2(rect.x + rect.width, rect.y + rect.height), canvas_rect);
        Rect::from_two_pos(top_left, bottom_right)
    }

    /// Check if a point is inside a cell
    fn is_point_in_cell(&self, point: Pos2, cell_bounds: &Rectangle) -> bool {
        point.x >= cell_bounds.x
            && point.x <= cell_bounds.x + cell_bounds.width
            && point.y >= cell_bounds.y
            && point.y <= cell_bounds.y + cell_bounds.height
    }

    /// Split the currently selected cell
    fn split_selected_cell(&mut self, direction: SplitDirection) {
        if let Some(cell_id) = self.selected_cell {
            match self.canvas.split_cell(cell_id, direction, 0.5) {
                Ok((child1, _child2)) => {
                    self.selected_cell = Some(child1);
                    self.status_message = format!("✓ Cell split {:?}", direction);
                }
                Err(e) => {
                    self.status_message = format!("❌ Split failed: {}", e);
                }
            }
        } else {
            self.status_message = "⚠ No cell selected".to_string();
        }
    }
}

impl eframe::App for GraphCellEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_ui(ctx);
    }
}
