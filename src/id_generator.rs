/// Alphanumeric ID generator for cells
/// Generates short, case-insensitive IDs like "A7", "2K", etc.
/// Automatically expands to more digits when namespace is exhausted

use std::collections::HashSet;

const CHARS: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J',
    'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T',
    'U', 'V', 'W', 'X', 'Y', 'Z',
];

#[derive(Debug, Clone)]
pub struct IdGenerator {
    /// Current ID length (starts at 2)
    length: usize,
    /// Counter for next ID
    counter: u64,
    /// Maximum value before needing to expand
    max_value: u64,
}

impl IdGenerator {
    pub fn new() -> Self {
        Self::with_length(2)
    }

    pub fn with_length(length: usize) -> Self {
        let max_value = (CHARS.len() as u64).pow(length as u32);
        Self {
            length,
            counter: 0,
            max_value,
        }
    }

    /// Generate the next ID
    pub fn next(&mut self) -> String {
        if self.counter >= self.max_value {
            // Expand to next length
            self.expand();
        }

        let id = self.encode(self.counter);
        self.counter += 1;
        id
    }

    /// Encode a number to base-36 alphanumeric string
    fn encode(&self, mut num: u64) -> String {
        let base = CHARS.len() as u64;
        let mut result = Vec::new();

        // Generate digits
        for _ in 0..self.length {
            let digit = (num % base) as usize;
            result.push(CHARS[digit]);
            num /= base;
        }

        result.reverse();
        result.into_iter().collect()
    }

    /// Expand to the next length, upgrading existing IDs
    fn expand(&mut self) {
        self.length += 1;
        self.max_value = (CHARS.len() as u64).pow(self.length as u32);
        // Reset counter since we're in a new namespace
        self.counter = 0;
    }

    /// Upgrade an existing ID to the new length by prefixing with '0'
    pub fn upgrade_id(id: &str) -> String {
        format!("0{}", id)
    }

    /// Get all existing IDs from a set and determine appropriate length
    pub fn from_existing_ids(existing_ids: &HashSet<String>) -> Self {
        if existing_ids.is_empty() {
            return Self::new();
        }

        // Find the maximum length
        let max_len = existing_ids.iter().map(|id| id.len()).max().unwrap_or(2);

        // Parse existing IDs to find the highest counter value
        let mut max_counter = 0u64;

        for id in existing_ids {
            if id.len() == max_len {
                if let Some(counter) = Self::decode(id) {
                    max_counter = max_counter.max(counter);
                }
            }
        }

        let mut generator = Self::with_length(max_len);
        generator.counter = max_counter + 1;
        generator
    }

    /// Decode an ID back to its counter value
    fn decode(id: &str) -> Option<u64> {
        let base = CHARS.len() as u64;
        let mut result = 0u64;

        for c in id.chars() {
            let digit = CHARS.iter().position(|&ch| ch == c.to_ascii_uppercase())?;
            result = result * base + digit as u64;
        }

        Some(result)
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_generation() {
        let mut gen = IdGenerator::new();
        assert_eq!(gen.next(), "00");
        assert_eq!(gen.next(), "01");
        assert_eq!(gen.next(), "02");
    }

    #[test]
    fn test_expansion() {
        let mut gen = IdGenerator::with_length(1);
        // Generate all 36 single-char IDs
        for i in 0..36 {
            let id = gen.next();
            assert_eq!(id.len(), 1);
        }
        // Next should expand to 2 chars
        let id = gen.next();
        assert_eq!(id.len(), 2);
        assert_eq!(id, "00");
    }

    #[test]
    fn test_upgrade_id() {
        assert_eq!(IdGenerator::upgrade_id("A7"), "0A7");
        assert_eq!(IdGenerator::upgrade_id("2K"), "02K");
    }

    #[test]
    fn test_from_existing() {
        let mut existing = HashSet::new();
        existing.insert("A7".to_string());
        existing.insert("2K".to_string());
        existing.insert("ZZ".to_string());

        let mut gen = IdGenerator::from_existing_ids(&existing);
        let next = gen.next();
        // Should not conflict with existing
        assert!(!existing.contains(&next));
    }
}
