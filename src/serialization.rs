use crate::{Canvas, Cell, GraphEvent, Relationship};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use ulid::Ulid;

/// Project manifest containing metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub start_cell: Option<Ulid>,
}

impl Manifest {
    /// Create a new manifest
    pub fn new(start_cell: Option<Ulid>) -> Self {
        let now = Utc::now();
        Self {
            version: "0.1.0".to_string(),
            created: now,
            modified: now,
            start_cell,
        }
    }

    /// Update the modified timestamp
    pub fn touch(&mut self) {
        self.modified = Utc::now();
    }

    /// Save manifest to file
    pub fn save(&self, path: &Path) -> Result<()> {
        let file = File::create(path)
            .with_context(|| format!("Failed to create manifest file: {}", path.display()))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)
            .with_context(|| format!("Failed to write manifest to: {}", path.display()))?;
        Ok(())
    }

    /// Load manifest from file
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open manifest file: {}", path.display()))?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader)
            .with_context(|| format!("Failed to parse manifest from: {}", path.display()))
    }
}

/// Serializable canvas representation
#[derive(Debug, Serialize, Deserialize)]
struct SerializableCanvas {
    cells: Vec<Cell>,
    relationships: Vec<Relationship>,
    root_cell: Option<Ulid>,
}

impl SerializableCanvas {
    /// Convert from Canvas
    fn from_canvas(canvas: &Canvas) -> Self {
        Self {
            cells: canvas.cells().values().cloned().collect(),
            relationships: canvas.relationships().values().cloned().collect(),
            root_cell: canvas.root_cell(),
        }
    }

    /// Convert to Canvas
    fn to_canvas(self) -> Canvas {
        let mut canvas = Canvas::new();

        // Insert cells
        for cell in self.cells {
            canvas.cells_mut().insert(cell.id, cell);
        }

        // Insert relationships
        for rel in self.relationships {
            canvas
                .relationships_mut()
                .insert((rel.from, rel.to), rel);
        }

        // Set root cell
        if let Some(root) = self.root_cell {
            canvas.set_root_cell(root);
        }

        canvas
    }
}

/// Project structure manager
pub struct Project {
    /// Root directory of the project
    root_dir: PathBuf,
}

impl Project {
    /// Create a new project at the given path
    pub fn create(path: &Path) -> Result<Self> {
        // Create directory structure
        fs::create_dir_all(path)
            .with_context(|| format!("Failed to create project directory: {}", path.display()))?;

        let external_dir = path.join("external");
        fs::create_dir_all(&external_dir).with_context(|| {
            format!("Failed to create external directory: {}", external_dir.display())
        })?;

        let snapshots_dir = path.join("snapshots");
        fs::create_dir_all(&snapshots_dir).with_context(|| {
            format!(
                "Failed to create snapshots directory: {}",
                snapshots_dir.display()
            )
        })?;

        // Create initial manifest
        let manifest = Manifest::new(None);
        manifest.save(&path.join("manifest.json"))?;

        // Create empty cells.json
        let empty_canvas = Canvas::new();
        Self::save_canvas_internal(path, &empty_canvas)?;

        // Create empty events.jsonl
        File::create(path.join("events.jsonl")).with_context(|| {
            format!(
                "Failed to create events.jsonl: {}",
                path.join("events.jsonl").display()
            )
        })?;

        Ok(Self {
            root_dir: path.to_path_buf(),
        })
    }

    /// Open an existing project
    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(anyhow!("Project directory does not exist: {}", path.display()));
        }

        // Verify required files exist
        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            return Err(anyhow!("manifest.json not found in project directory"));
        }

        let cells_path = path.join("cells.json");
        if !cells_path.exists() {
            return Err(anyhow!("cells.json not found in project directory"));
        }

        Ok(Self {
            root_dir: path.to_path_buf(),
        })
    }

    /// Get the root directory
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Get path to manifest.json
    pub fn manifest_path(&self) -> PathBuf {
        self.root_dir.join("manifest.json")
    }

    /// Get path to cells.json
    pub fn cells_path(&self) -> PathBuf {
        self.root_dir.join("cells.json")
    }

    /// Get path to events.jsonl
    pub fn events_path(&self) -> PathBuf {
        self.root_dir.join("events.jsonl")
    }

    /// Get path to external directory
    pub fn external_dir(&self) -> PathBuf {
        self.root_dir.join("external")
    }

    /// Get path to snapshots directory
    pub fn snapshots_dir(&self) -> PathBuf {
        self.root_dir.join("snapshots")
    }

    /// Save manifest
    pub fn save_manifest(&self, manifest: &Manifest) -> Result<()> {
        manifest.save(&self.manifest_path())
    }

    /// Load manifest
    pub fn load_manifest(&self) -> Result<Manifest> {
        Manifest::load(&self.manifest_path())
    }

    /// Save canvas to cells.json
    pub fn save_canvas(&self, canvas: &Canvas) -> Result<()> {
        Self::save_canvas_internal(&self.root_dir, canvas)
    }

    fn save_canvas_internal(root_dir: &Path, canvas: &Canvas) -> Result<()> {
        let cells_path = root_dir.join("cells.json");
        let serializable = SerializableCanvas::from_canvas(canvas);

        let file = File::create(&cells_path)
            .with_context(|| format!("Failed to create cells.json: {}", cells_path.display()))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &serializable)
            .with_context(|| format!("Failed to write cells.json: {}", cells_path.display()))?;

        Ok(())
    }

    /// Load canvas from cells.json
    pub fn load_canvas(&self) -> Result<Canvas> {
        let cells_path = self.cells_path();

        let file = File::open(&cells_path)
            .with_context(|| format!("Failed to open cells.json: {}", cells_path.display()))?;
        let reader = BufReader::new(file);

        let serializable: SerializableCanvas = serde_json::from_reader(reader)
            .with_context(|| format!("Failed to parse cells.json: {}", cells_path.display()))?;

        Ok(serializable.to_canvas())
    }

    /// Append events to events.jsonl
    pub fn append_events(&self, events: &[GraphEvent]) -> Result<()> {
        let events_path = self.events_path();

        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&events_path)
            .with_context(|| format!("Failed to open events.jsonl: {}", events_path.display()))?;

        let mut writer = BufWriter::new(file);

        for event in events {
            let json = serde_json::to_string(event).with_context(|| {
                format!("Failed to serialize event: {}", events_path.display())
            })?;
            writeln!(writer, "{}", json).with_context(|| {
                format!("Failed to write event to: {}", events_path.display())
            })?;
        }

        writer
            .flush()
            .with_context(|| format!("Failed to flush events.jsonl: {}", events_path.display()))?;

        Ok(())
    }

    /// Load all events from events.jsonl
    pub fn load_events(&self) -> Result<Vec<GraphEvent>> {
        let events_path = self.events_path();

        if !events_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&events_path)
            .with_context(|| format!("Failed to open events.jsonl: {}", events_path.display()))?;
        let reader = BufReader::new(file);

        let mut events = Vec::new();
        for (line_num, line) in reader.lines().enumerate() {
            let line = line.with_context(|| {
                format!(
                    "Failed to read line {} from: {}",
                    line_num + 1,
                    events_path.display()
                )
            })?;

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            let event: GraphEvent = serde_json::from_str(&line).with_context(|| {
                format!(
                    "Failed to parse event on line {} from: {}",
                    line_num + 1,
                    events_path.display()
                )
            })?;

            events.push(event);
        }

        Ok(events)
    }

    /// Save complete project state (manifest, canvas, and events)
    pub fn save(&self, canvas: &Canvas) -> Result<()> {
        // Update manifest
        let mut manifest = self.load_manifest().unwrap_or_else(|_| Manifest::new(None));
        manifest.touch();
        manifest.start_cell = canvas.get_start_point().map(|c| c.id);
        self.save_manifest(&manifest)?;

        // Save canvas
        self.save_canvas(canvas)?;

        // Save events
        self.append_events(canvas.events())?;

        Ok(())
    }

    /// Load complete project state
    pub fn load(&self) -> Result<(Manifest, Canvas)> {
        let manifest = self.load_manifest()?;
        let canvas = self.load_canvas()?;

        Ok((manifest, canvas))
    }
}

/// Memory-mapped file handle for large external files
pub struct ExternalFileHandle {
    path: PathBuf,
    mmap: Option<Mmap>,
    size: u64,
}

impl ExternalFileHandle {
    /// Open file with memory mapping for large files (>10MB)
    pub fn open(path: PathBuf) -> Result<Self> {
        let metadata = fs::metadata(&path)
            .with_context(|| format!("Failed to read file metadata: {}", path.display()))?;
        let size = metadata.len();

        let mmap = if size > 10_000_000 {
            // Use memory mapping for large files
            let file = File::open(&path)
                .with_context(|| format!("Failed to open file for mmap: {}", path.display()))?;
            Some(unsafe {
                Mmap::map(&file)
                    .with_context(|| format!("Failed to create mmap for: {}", path.display()))?
            })
        } else {
            None
        };

        Ok(Self { path, mmap, size })
    }

    /// Read entire file content as string
    pub fn read_to_string(&self) -> Result<String> {
        if let Some(mmap) = &self.mmap {
            // Read from memory-mapped file
            String::from_utf8(mmap.to_vec()).with_context(|| {
                format!(
                    "Failed to convert mmap content to UTF-8: {}",
                    self.path.display()
                )
            })
        } else {
            // Read small file directly
            fs::read_to_string(&self.path).with_context(|| {
                format!("Failed to read file content: {}", self.path.display())
            })
        }
    }

    /// Read a range of bytes from the file
    pub fn read_range(&self, start: usize, length: usize) -> Result<&[u8]> {
        if let Some(mmap) = &self.mmap {
            if start + length > self.size as usize {
                return Err(anyhow!(
                    "Read range ({}, {}) exceeds file size {}",
                    start,
                    length,
                    self.size
                ));
            }
            Ok(&mmap[start..start + length])
        } else {
            Err(anyhow!(
                "Scoped reads only supported for memory-mapped files (>10MB)"
            ))
        }
    }

    /// Get file size
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Check if using memory mapping
    pub fn is_mmapped(&self) -> bool {
        self.mmap.is_some()
    }

    /// Get file path
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CellContent, CellType, Rectangle};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_manifest_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        let cell_id = Ulid::new();
        let manifest = Manifest::new(Some(cell_id));

        manifest.save(&manifest_path).unwrap();
        let loaded = Manifest::load(&manifest_path).unwrap();

        assert_eq!(loaded.version, "0.1.0");
        assert_eq!(loaded.start_cell, Some(cell_id));
    }

    #[test]
    fn test_project_create() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("test_project");

        let project = Project::create(&project_path).unwrap();

        assert!(project.manifest_path().exists());
        assert!(project.cells_path().exists());
        assert!(project.events_path().exists());
        assert!(project.external_dir().exists());
        assert!(project.snapshots_dir().exists());
    }

    #[test]
    fn test_project_save_load_canvas() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("test_project");

        let project = Project::create(&project_path).unwrap();

        // Create a canvas with some cells
        let mut canvas = Canvas::new();
        let cell_id = canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Test Cell"),
        );
        canvas.set_start_point(cell_id).unwrap();
        canvas
            .rename_cell(cell_id, Some("TestCell".to_string()))
            .unwrap();

        // Save canvas
        project.save_canvas(&canvas).unwrap();

        // Load canvas
        let loaded_canvas = project.load_canvas().unwrap();

        assert_eq!(loaded_canvas.cell_count(), canvas.cell_count());
        let loaded_cell = loaded_canvas.get_cell(cell_id).unwrap();
        assert_eq!(loaded_cell.name, Some("TestCell".to_string()));
        assert_eq!(loaded_cell.content.as_str(), Some("Test Cell"));
    }

    #[test]
    fn test_event_logging() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("test_project");

        let project = Project::create(&project_path).unwrap();

        // Create a canvas and generate events
        let mut canvas = Canvas::new();
        canvas.create_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 100.0, 100.0),
            CellContent::inline("Cell 1"),
        );
        canvas.create_cell(
            CellType::Python,
            Rectangle::new(150.0, 0.0, 100.0, 100.0),
            CellContent::inline("Cell 2"),
        );

        // Append events
        project.append_events(canvas.events()).unwrap();

        // Load events
        let loaded_events = project.load_events().unwrap();

        assert_eq!(loaded_events.len(), canvas.events().len());
    }

    #[test]
    fn test_save_load_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("test_project");

        let project = Project::create(&project_path).unwrap();

        // Create a complex canvas
        let mut canvas = Canvas::with_root_cell(
            CellType::Text,
            Rectangle::new(0.0, 0.0, 400.0, 300.0),
            CellContent::inline("Root"),
        );

        let root = canvas.root_cell().unwrap();
        let (child1, child2) = canvas
            .split_cell(root, crate::SplitDirection::Horizontal, 0.5)
            .unwrap();
        canvas.rename_cell(child1, Some("Top".to_string())).unwrap();
        canvas
            .rename_cell(child2, Some("Bottom".to_string()))
            .unwrap();
        canvas.create_relationship(child1, child2).unwrap();
        canvas.set_start_point(child1).unwrap();

        // Save
        project.save(&canvas).unwrap();

        // Load
        let (manifest, loaded_canvas) = project.load().unwrap();

        // Verify
        assert_eq!(loaded_canvas.cell_count(), canvas.cell_count());
        assert_eq!(
            loaded_canvas.relationship_count(),
            canvas.relationship_count()
        );
        assert_eq!(manifest.start_cell, Some(child1));

        // Verify cells
        assert_eq!(
            loaded_canvas.get_cell(child1).unwrap().name,
            Some("Top".to_string())
        );
        assert_eq!(
            loaded_canvas.get_cell(child2).unwrap().name,
            Some("Bottom".to_string())
        );

        // Verify relationship
        assert!(loaded_canvas.get_relationship(child1, child2).is_some());
    }

    #[test]
    fn test_external_file_small() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("small.txt");

        // Create a small file (< 10MB)
        fs::write(&file_path, "Hello, World!").unwrap();

        let handle = ExternalFileHandle::open(file_path).unwrap();

        assert!(!handle.is_mmapped());
        assert_eq!(handle.size(), 13);
        assert_eq!(handle.read_to_string().unwrap(), "Hello, World!");
    }

    #[test]
    fn test_corrupted_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        // Write invalid JSON
        fs::write(&manifest_path, "{ invalid json }").unwrap();

        let result = Manifest::load(&manifest_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_corrupted_cells() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("test_project");

        let project = Project::create(&project_path).unwrap();

        // Corrupt cells.json
        fs::write(&project.cells_path(), "{ invalid json }").unwrap();

        let result = project.load_canvas();
        assert!(result.is_err());
    }
}
