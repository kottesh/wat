//! Layout engine - component stacking and spacing

/// Spacing configuration for components
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spacing {
    pub above: u16,
    pub below: u16,
}

impl Spacing {
    pub fn new(above: u16, below: u16) -> Self {
        Self { above, below }
    }
    
    pub fn none() -> Self {
        Self { above: 0, below: 0 }
    }
    
    pub fn below(lines: u16) -> Self {
        Self { above: 0, below: lines }
    }
    
    pub fn above(lines: u16) -> Self {
        Self { above: lines, below: 0 }
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Self { above: 0, below: 1 }
    }
}

/// Simple layout engine for stacking components
pub struct Layout;

impl Layout {
    /// Stack component lines with spacing
    /// 
    /// Takes component-rendered lines and adds blank line spacing
    /// according to the spacing rules.
    pub fn stack_with_spacing(
        components: Vec<(Vec<String>, Spacing)>
    ) -> Vec<String> {
        let mut result = Vec::new();

        for (i, (lines, spacing)) in components.into_iter().enumerate() {
            // Add spacing above (except for first component)
            if i > 0 {
                for _ in 0..spacing.above {
                    result.push(String::new());
                }
            }

            // Add component lines
            result.extend(lines);

            // Add spacing below
            for _ in 0..spacing.below {
                result.push(String::new());
            }
        }

        result
    }

    /// Add a single blank line separator
    pub fn add_separator(lines: &mut Vec<String>) {
        lines.push(String::new());
    }

    /// Trim trailing blank lines
    pub fn trim_trailing_blank(mut lines: Vec<String>) -> Vec<String> {
        while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            lines.pop();
        }
        lines
    }

    /// Ensure at least one blank line at the end
    pub fn ensure_trailing_blank(mut lines: Vec<String>) -> Vec<String> {
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spacing_new() {
        let spacing = Spacing::new(2, 3);
        assert_eq!(spacing.above, 2);
        assert_eq!(spacing.below, 3);
    }

    #[test]
    fn test_spacing_none() {
        let spacing = Spacing::none();
        assert_eq!(spacing.above, 0);
        assert_eq!(spacing.below, 0);
    }

    #[test]
    fn test_spacing_below() {
        let spacing = Spacing::below(2);
        assert_eq!(spacing.above, 0);
        assert_eq!(spacing.below, 2);
    }

    #[test]
    fn test_spacing_above() {
        let spacing = Spacing::above(3);
        assert_eq!(spacing.above, 3);
        assert_eq!(spacing.below, 0);
    }

    #[test]
    fn test_spacing_default() {
        let spacing = Spacing::default();
        assert_eq!(spacing.above, 0);
        assert_eq!(spacing.below, 1);
    }

    #[test]
    fn test_stack_with_spacing_single() {
        let components = vec![
            (vec!["line1".to_string(), "line2".to_string()], Spacing::default()),
        ];

        let result = Layout::stack_with_spacing(components);
        assert_eq!(result, vec![
            "line1",
            "line2",
            "", // default below spacing
        ]);
    }

    #[test]
    fn test_stack_with_spacing_multiple() {
        let components = vec![
            (vec!["comp1".to_string()], Spacing::below(1)),
            (vec!["comp2".to_string()], Spacing::below(1)),
            (vec!["comp3".to_string()], Spacing::none()),
        ];

        let result = Layout::stack_with_spacing(components);
        assert_eq!(result, vec![
            "comp1",
            "",     // below spacing for comp1
            "comp2",
            "",     // below spacing for comp2
            "comp3",
            // no below spacing for comp3
        ]);
    }

    #[test]
    fn test_stack_with_above_spacing() {
        let components = vec![
            (vec!["comp1".to_string()], Spacing::none()),
            (vec!["comp2".to_string()], Spacing::new(2, 1)),
        ];

        let result = Layout::stack_with_spacing(components);
        assert_eq!(result, vec![
            "comp1",
            "",     // above spacing for comp2 (line 1)
            "",     // above spacing for comp2 (line 2)
            "comp2",
            "",     // below spacing for comp2
        ]);
    }

    #[test]
    fn test_stack_with_spacing_empty_components() {
        let components = vec![];
        let result = Layout::stack_with_spacing(components);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_add_separator() {
        let mut lines = vec!["line1".to_string()];
        Layout::add_separator(&mut lines);
        assert_eq!(lines, vec!["line1", ""]);
    }

    #[test]
    fn test_trim_trailing_blank() {
        let lines = vec![
            "line1".to_string(),
            "line2".to_string(),
            "".to_string(),
            "".to_string(),
        ];

        let result = Layout::trim_trailing_blank(lines);
        assert_eq!(result, vec!["line1", "line2"]);
    }

    #[test]
    fn test_trim_trailing_blank_no_trailing() {
        let lines = vec!["line1".to_string(), "line2".to_string()];
        let result = Layout::trim_trailing_blank(lines);
        assert_eq!(result, vec!["line1", "line2"]);
    }

    #[test]
    fn test_ensure_trailing_blank() {
        let lines = vec!["line1".to_string(), "line2".to_string()];
        let result = Layout::ensure_trailing_blank(lines);
        assert_eq!(result, vec!["line1", "line2", ""]);
    }

    #[test]
    fn test_ensure_trailing_blank_already_has() {
        let lines = vec!["line1".to_string(), "".to_string()];
        let result = Layout::ensure_trailing_blank(lines);
        assert_eq!(result, vec!["line1", ""]);
    }

    #[test]
    fn test_ensure_trailing_blank_empty() {
        let lines: Vec<String> = vec![];
        let result = Layout::ensure_trailing_blank(lines);
        assert_eq!(result.len(), 0);
    }
}
