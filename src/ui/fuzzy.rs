//! Fuzzy file search with custom matcher (no external dependencies)

use std::fs;
use std::path::Path;

/// Simple fuzzy matcher using substring + scoring heuristics
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    /// Match query against text, return score (0 = no match, higher = better)
    /// 
    /// Scoring strategy:
    /// - Exact substring match: 100 - position
    /// - Start of string bonus: +50
    /// - After separator bonus: +20
    /// - Scattered match: sum of position bonuses + consecutive bonuses
    pub fn score(query: &str, text: &str) -> i64 {
        if query.is_empty() {
            return 0;
        }

        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        // Fast path: exact substring match
        if let Some(pos) = text_lower.find(&query_lower) {
            let mut score = 100 - (pos as i64);

            // Bonus: start of string
            if pos == 0 {
                score += 50;
            }

            // Bonus: after a separator (/, _, -, .)
            if pos > 0 {
                if let Some(prev_char) = text.chars().nth(pos - 1) {
                    if matches!(prev_char, '/' | '_' | '-' | '.') {
                        score += 20;
                    }
                }
            }

            return score;
        }

        // Fallback: scattered character matching
        Self::score_scattered(&query_lower, &text_lower)
    }

    /// Score scattered characters (e.g., "src" matches "s...r...c")
    fn score_scattered(query: &str, text: &str) -> i64 {
        let query_chars: Vec<char> = query.chars().collect();
        let text_chars: Vec<char> = text.chars().collect();

        if query_chars.is_empty() || text_chars.is_empty() {
            return 0;
        }

        let mut score = 0i64;
        let mut text_idx = 0;
        let mut last_match_idx = None;

        for q_char in &query_chars {
            // Find next occurrence of q_char in text
            let mut found = false;
            for (i, &t_char) in text_chars.iter().enumerate().skip(text_idx) {
                if t_char == *q_char {
                    text_idx = i + 1;
                    found = true;

                    // Consecutive character bonus
                    if let Some(last_idx) = last_match_idx {
                        if i == last_idx + 1 {
                            score += 15; // Strong bonus for consecutive matches
                        }
                    }

                    // Earlier position bonus (characters at start worth more)
                    score += ((text_chars.len() - i) as i64) / 2;

                    // Start of string bonus
                    if i == 0 {
                        score += 10;
                    }

                    // After separator bonus
                    if i > 0 && matches!(text_chars[i - 1], '/' | '_' | '-' | '.') {
                        score += 8;
                    }

                    last_match_idx = Some(i);
                    break;
                }
            }

            if !found {
                return 0; // Character not found in order, no match
            }
        }

        // Additional bonus for matching all characters
        score += 5;

        score
    }
}

/// Fuzzy file search state
pub struct FuzzySearch {
    files: Vec<String>,       // Cached file list
    results: Vec<String>,     // Current filtered results
    selection: usize,         // Selected index
    query: String,            // Current search query
    max_results: usize,       // Maximum results to show
}

impl FuzzySearch {
    /// Create a new fuzzy search instance
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            results: Vec::new(),
            selection: 0,
            query: String::new(),
            max_results: 10,
        }
    }

    /// Load files from current directory recursively
    pub fn load_files(&mut self) {
        self.files = Self::walk_directory(".", 5000); // Max 5000 files
        self.update_results();
    }

    /// Walk directory tree and collect file paths
    fn walk_directory(root: &str, max_files: usize) -> Vec<String> {
        let mut files = Vec::new();

        fn visit_dir(
            path: &Path,
            files: &mut Vec<String>,
            max_files: usize,
        ) -> std::io::Result<()> {
            if files.len() >= max_files {
                return Ok(());
            }

            // Skip hidden dirs and common ignore patterns
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.')
                    || name == "target"
                    || name == "node_modules"
                    || name == "dist"
                    || name == "build"
                    || name == "__pycache__"
                    || name == "venv"
                    || name == ".git"
                {
                    return Ok(());
                }
            }

            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    visit_dir(&entry_path, files, max_files)?;
                } else if entry_path.is_file() {
                    if let Some(path_str) = entry_path.to_str() {
                        // Strip "./" prefix for cleaner display
                        let clean = path_str
                            .strip_prefix("./")
                            .unwrap_or(path_str)
                            .to_string();
                        files.push(clean);
                    }

                    if files.len() >= max_files {
                        break;
                    }
                }
            }

            Ok(())
        }

        let _ = visit_dir(Path::new(root), &mut files, max_files);
        files.sort(); // Alphabetical order
        files
    }

    /// Update search query and refresh results
    pub fn update_query(&mut self, query: String) {
        self.query = query;
        self.update_results();
    }

    /// Refresh filtered results based on current query
    fn update_results(&mut self) {
        if self.query.is_empty() {
            // No query: show first N files
            self.results = self.files.iter().take(self.max_results).cloned().collect();
            self.selection = 0;
            return;
        }

        // Score and filter files
        let mut scored: Vec<(String, i64)> = self
            .files
            .iter()
            .filter_map(|file| {
                let score = FuzzyMatcher::score(&self.query, file);
                if score > 0 {
                    Some((file.clone(), score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score (descending), then by path length (ascending)
        scored.sort_by(|a, b| {
            b.1.cmp(&a.1) // Higher score first
                .then_with(|| a.0.len().cmp(&b.0.len())) // Shorter paths as tiebreaker
        });

        self.results = scored
            .into_iter()
            .take(self.max_results)
            .map(|(path, _)| path)
            .collect();

        self.selection = 0;
    }

    /// Move selection by delta (-1 = up, +1 = down)
    pub fn move_selection(&mut self, delta: i32) {
        if self.results.is_empty() {
            return;
        }

        let new_idx = (self.selection as i32 + delta)
            .max(0)
            .min(self.results.len() as i32 - 1) as usize;

        self.selection = new_idx;
    }

    /// Get currently selected file path
    pub fn get_selected(&self) -> Option<&str> {
        self.results.get(self.selection).map(|s| s.as_str())
    }

    /// Get current query string
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get current results
    pub fn results(&self) -> &[String] {
        &self.results
    }

    /// Get current selection index
    pub fn selection(&self) -> usize {
        self.selection
    }

    /// Check if any results are available
    pub fn has_results(&self) -> bool {
        !self.results.is_empty()
    }

    /// Get total number of cached files
    pub fn total_files(&self) -> usize {
        self.files.len()
    }

    /// Get selected file (alias for get_selected)
    pub fn selected_file(&self) -> Option<String> {
        self.get_selected().map(|s| s.to_string())
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        self.move_selection(-1);
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        self.move_selection(1);
    }

    /// Render fuzzy search results
    pub fn render(&self, _width: u16) -> Vec<String> {
        let mut lines = Vec::new();
        
        // Header
        lines.push(format!("Fuzzy Search ({}  results):", self.results.len()));
        lines.push(String::new());
        
        // Results
        for (i, file) in self.results.iter().enumerate() {
            if i == self.selection {
                lines.push(format!("> {}", file));
            } else {
                lines.push(format!("  {}", file));
            }
        }
        
        if self.results.is_empty() {
            lines.push("  No matches".to_string());
        }
        
        lines
    }
}

impl Default for FuzzySearch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_substring_match() {
        let score = FuzzyMatcher::score("main", "src/main.rs");
        assert!(score > 0, "Should match 'main' in 'src/main.rs'");
    }

    #[test]
    fn test_start_of_string_bonus() {
        let score1 = FuzzyMatcher::score("src", "src/main.rs");
        let score2 = FuzzyMatcher::score("src", "other/src/file.rs");
        assert!(
            score1 > score2,
            "Match at start should score higher: {} vs {}",
            score1,
            score2
        );
    }

    #[test]
    fn test_no_match() {
        let score = FuzzyMatcher::score("xyz", "src/main.rs");
        assert_eq!(score, 0, "Should not match 'xyz' in 'src/main.rs'");
    }

    #[test]
    fn test_scattered_match() {
        // "src" should match "s...r...c" pattern
        let score = FuzzyMatcher::score("src", "some_rust_code.rs");
        assert!(score > 0, "Should match scattered 'src'");
    }

    #[test]
    fn test_consecutive_bonus() {
        let score1 = FuzzyMatcher::score("mai", "main.rs"); // consecutive
        let score2 = FuzzyMatcher::score("mai", "m_a_i.rs"); // scattered
        assert!(
            score1 > score2,
            "Consecutive matches should score higher: {} vs {}",
            score1,
            score2
        );
    }

    #[test]
    fn test_fuzzy_search_empty_query() {
        let mut search = FuzzySearch::new();
        search.files = vec![
            "file1.txt".to_string(),
            "file2.txt".to_string(),
            "file3.txt".to_string(),
        ];
        search.update_query(String::new());

        assert_eq!(search.results().len(), 3);
        assert_eq!(search.selection(), 0);
    }

    #[test]
    fn test_fuzzy_search_filtering() {
        let mut search = FuzzySearch::new();
        search.files = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "README.md".to_string(),
            "tests/test.rs".to_string(),
        ];
        search.update_query("main".to_string());

        assert_eq!(search.results().len(), 1);
        assert_eq!(search.results()[0], "src/main.rs");
    }

    #[test]
    fn test_fuzzy_search_move_selection() {
        let mut search = FuzzySearch::new();
        search.results = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        search.selection = 0;

        search.move_selection(1);
        assert_eq!(search.selection(), 1);

        search.move_selection(1);
        assert_eq!(search.selection(), 2);

        search.move_selection(1); // Should clamp at 2
        assert_eq!(search.selection(), 2);

        search.move_selection(-1);
        assert_eq!(search.selection(), 1);

        search.move_selection(-5); // Should clamp at 0
        assert_eq!(search.selection(), 0);
    }

    #[test]
    fn test_fuzzy_search_get_selected() {
        let mut search = FuzzySearch::new();
        search.results = vec!["file1.txt".to_string(), "file2.txt".to_string()];
        search.selection = 1;

        assert_eq!(search.get_selected(), Some("file2.txt"));
    }

    #[test]
    fn test_case_insensitive_matching() {
        let score1 = FuzzyMatcher::score("MAIN", "src/main.rs");
        let score2 = FuzzyMatcher::score("main", "src/MAIN.rs");
        assert!(score1 > 0, "Should match case-insensitively");
        assert!(score2 > 0, "Should match case-insensitively");
    }

    #[test]
    fn test_separator_bonus() {
        let score1 = FuzzyMatcher::score("main", "src/main.rs"); // After /
        let score2 = FuzzyMatcher::score("main", "srcmain.rs"); // No separator
        assert!(
            score1 > score2,
            "After separator should score higher: {} vs {}",
            score1,
            score2
        );
    }

    #[test]
    fn test_empty_query() {
        let score = FuzzyMatcher::score("", "anything.txt");
        assert_eq!(score, 0, "Empty query should return 0 score");
    }
}
