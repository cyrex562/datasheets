use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Relationship between two cells representing data flow
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Relationship {
    /// Source cell (data flows FROM this cell)
    pub from: Ulid,

    /// Destination cell (data flows TO this cell)
    pub to: Ulid,

    // Future: transformation functions, filters, etc.
}

impl Relationship {
    /// Create a new relationship
    pub fn new(from: Ulid, to: Ulid) -> Self {
        Self { from, to }
    }

    /// Check if this relationship involves a given cell
    pub fn involves(&self, cell_id: Ulid) -> bool {
        self.from == cell_id || self.to == cell_id
    }

    /// Check if this relationship starts from a given cell
    pub fn starts_from(&self, cell_id: Ulid) -> bool {
        self.from == cell_id
    }

    /// Check if this relationship ends at a given cell
    pub fn ends_at(&self, cell_id: Ulid) -> bool {
        self.to == cell_id
    }

    /// Reverse the direction of this relationship
    pub fn reversed(&self) -> Self {
        Self {
            from: self.to,
            to: self.from,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relationship_creation() {
        let from_id = Ulid::new();
        let to_id = Ulid::new();
        let rel = Relationship::new(from_id, to_id);

        assert_eq!(rel.from, from_id);
        assert_eq!(rel.to, to_id);
    }

    #[test]
    fn test_relationship_involves() {
        let from_id = Ulid::new();
        let to_id = Ulid::new();
        let other_id = Ulid::new();
        let rel = Relationship::new(from_id, to_id);

        assert!(rel.involves(from_id));
        assert!(rel.involves(to_id));
        assert!(!rel.involves(other_id));
    }

    #[test]
    fn test_relationship_direction() {
        let from_id = Ulid::new();
        let to_id = Ulid::new();
        let rel = Relationship::new(from_id, to_id);

        assert!(rel.starts_from(from_id));
        assert!(!rel.starts_from(to_id));

        assert!(rel.ends_at(to_id));
        assert!(!rel.ends_at(from_id));
    }

    #[test]
    fn test_relationship_reversed() {
        let from_id = Ulid::new();
        let to_id = Ulid::new();
        let rel = Relationship::new(from_id, to_id);
        let reversed = rel.reversed();

        assert_eq!(reversed.from, to_id);
        assert_eq!(reversed.to, from_id);
    }
}
