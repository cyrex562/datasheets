/// Markdown link parser for wiki-style cell links
/// Supports [[cell_id]] syntax for linking between cells

use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub struct CellLink {
    /// The cell ID this link points to
    pub target_id: String,
    /// Start position in the text
    pub start: usize,
    /// End position in the text
    pub end: usize,
    /// The full link text including brackets
    pub full_text: String,
}

/// Parse wiki-style links from markdown text
/// Returns a list of all [[cell_id]] links found
pub fn parse_cell_links(text: &str) -> Vec<CellLink> {
    let mut links = Vec::new();
    
    // Simple regex for [[something]]
    let re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    
    for cap in re.captures_iter(text) {
        if let Some(m) = cap.get(0) {
            if let Some(target) = cap.get(1) {
                links.push(CellLink {
                    target_id: target.as_str().trim().to_string(),
                    start: m.start(),
                    end: m.end(),
                    full_text: m.as_str().to_string(),
                });
            }
        }
    }
    
    links
}

/// Check if a position in text is within a link
pub fn get_link_at_position(text: &str, position: usize) -> Option<CellLink> {
    let links = parse_cell_links(text);
    links.into_iter().find(|link| position >= link.start && position < link.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_link() {
        let text = "Check out [[A7]] for more info";
        let links = parse_cell_links(text);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "A7");
    }

    #[test]
    fn test_parse_multiple_links() {
        let text = "See [[A7]] and [[B2]] for details";
        let links = parse_cell_links(text);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_id, "A7");
        assert_eq!(links[1].target_id, "B2");
    }

    #[test]
    fn test_link_at_position() {
        let text = "Check out [[A7]] for more info";
        let link = get_link_at_position(text, 12);
        assert!(link.is_some());
        assert_eq!(link.unwrap().target_id, "A7");
    }

    #[test]
    fn test_no_link_at_position() {
        let text = "Check out [[A7]] for more info";
        let link = get_link_at_position(text, 0);
        assert!(link.is_none());
    }
}
