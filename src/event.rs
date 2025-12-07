use crate::{CellContent, CellType, Rectangle};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// A graph event with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEvent {
    pub timestamp: DateTime<Utc>,
    pub event: EventType,
}

impl GraphEvent {
    /// Create a new event with the current timestamp
    pub fn new(event: EventType) -> Self {
        Self {
            timestamp: Utc::now(),
            event,
        }
    }

    /// Create a new event with a specific timestamp
    pub fn with_timestamp(timestamp: DateTime<Utc>, event: EventType) -> Self {
        Self { timestamp, event }
    }
}

/// Types of events that can occur in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    CellCreated {
        id: Ulid,
        cell_type: CellType,
        bounds: Rectangle,
        name: Option<String>,
    },

    CellSplit {
        parent_id: Ulid,
        children: Vec<Ulid>,
        direction: SplitDirection,
        split_ratio: f32,
    },

    CellMerged {
        merged_ids: Vec<Ulid>,
        new_id: Ulid,
        new_type: CellType,
    },

    CellContentChanged {
        id: Ulid,
        new_content: CellContent,
    },

    CellTypeChanged {
        id: Ulid,
        old_type: CellType,
        new_type: CellType,
    },

    CellRenamed {
        id: Ulid,
        new_name: Option<String>,
    },

    RelationshipCreated {
        from: Ulid,
        to: Ulid,
    },

    RelationshipDeleted {
        from: Ulid,
        to: Ulid,
    },

    StartPointChanged {
        old_id: Option<Ulid>,
        new_id: Ulid,
    },

    /// For future undo/redo and time-travel debugging
    SnapshotCreated {
        snapshot_id: Ulid,
        state_hash: String,
    },
}

/// Direction for cell splitting
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal, // Top/Bottom
    Vertical,   // Left/Right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = GraphEvent::new(EventType::CellCreated {
            id: Ulid::new(),
            cell_type: CellType::Text,
            bounds: Rectangle::new(0.0, 0.0, 100.0, 100.0),
            name: Some("Test".to_string()),
        });

        assert!(event.timestamp <= Utc::now());
    }

    #[test]
    fn test_split_direction() {
        let horizontal = SplitDirection::Horizontal;
        let vertical = SplitDirection::Vertical;

        assert_ne!(horizontal, vertical);
    }

    #[test]
    fn test_event_serialization() {
        let event = GraphEvent::new(EventType::RelationshipCreated {
            from: Ulid::new(),
            to: Ulid::new(),
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: GraphEvent = serde_json::from_str(&json).unwrap();

        // Can't compare timestamps exactly due to serialization, but we can check the event type
        match (&event.event, &deserialized.event) {
            (
                EventType::RelationshipCreated { from: f1, to: t1 },
                EventType::RelationshipCreated { from: f2, to: t2 },
            ) => {
                assert_eq!(f1, f2);
                assert_eq!(t1, t2);
            }
            _ => panic!("Event type mismatch"),
        }
    }
}
