use eframe::egui;
use graph_cell_editor::GraphCellEditorApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Graph Cell Editor"),
        ..Default::default()
    };

    eframe::run_native(
        "Graph Cell Editor",
        options,
        Box::new(|_cc| Ok(Box::new(GraphCellEditorApp::new()))),
    )
}
