//! Tool registry managing all available tools

use std::collections::HashMap;
use std::sync::Arc;

use super::bash::BashTool;
use super::definition::ToolDefinition;
use super::read_file::ReadFileTool;
use crate::tools::ParameterSchema;

/// Registry of all available tools
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    /// Create a new registry with default tools
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };

        // Register bash tool
        registry.register(ToolDefinition {
            name: "bash".to_string(),
            description: "Execute shell commands".to_string(),
            parameters: ParameterSchema::new()
                .add_string("command", "The shell command to execute", true),
            executor: Arc::new(BashTool::new()),
        });

        // Register read_file tool
        registry.register(ToolDefinition {
            name: "read_file".to_string(),
            description: "Read file contents with line numbers".to_string(),
            parameters: ParameterSchema::new()
                .add_string("path", "Path to the file to read", true),
            executor: Arc::new(ReadFileTool::new()),
        });

        registry
    }

    /// Register a tool
    pub fn register(&mut self, tool: ToolDefinition) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// Get all tool definitions
    pub fn get_all_definitions(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    /// List all tool names
    #[allow(dead_code)] // Public API for debugging/listing
    pub fn list_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.list_names().len(), 2);
        assert!(registry.get("bash").is_some());
        assert!(registry.get("read_file").is_some());
    }

    #[test]
    fn test_get_tool() {
        let registry = ToolRegistry::new();
        let bash_tool = registry.get("bash").unwrap();
        assert_eq!(bash_tool.name, "bash");
        assert_eq!(bash_tool.description, "Execute shell commands");
    }

    #[test]
    fn test_get_nonexistent_tool() {
        let registry = ToolRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_get_all_definitions() {
        let registry = ToolRegistry::new();
        let all_tools = registry.get_all_definitions();
        assert_eq!(all_tools.len(), 2);
    }

    #[test]
    fn test_list_names() {
        let registry = ToolRegistry::new();
        let names = registry.list_names();
        assert!(names.contains(&"bash".to_string()));
        assert!(names.contains(&"read_file".to_string()));
    }
}
