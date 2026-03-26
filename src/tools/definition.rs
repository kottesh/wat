//! Tool definition and JSON schema types

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use super::executor::ToolExecutor;

/// Complete tool definition with schema and executor
#[derive(Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: ParameterSchema,
    pub executor: Arc<dyn ToolExecutor>,
}

impl ToolDefinition {
    /// Create a clone suitable for passing to provider
    #[allow(dead_code)] // Future use for history conversion
    pub fn clone_for_provider(&self) -> Self {
        Self {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
            executor: self.executor.clone(),
        }
    }
}

/// JSON Schema for tool parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: HashMap<String, PropertySchema>,
    pub required: Vec<String>,
}

/// Schema for a single property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySchema {
    #[serde(rename = "type")]
    pub property_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<PropertySchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

impl ParameterSchema {
    /// Create a new parameter schema
    pub fn new() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: HashMap::new(),
            required: Vec::new(),
        }
    }

    /// Add a string property
    pub fn add_string(mut self, name: &str, description: &str, required: bool) -> Self {
        self.properties.insert(
            name.to_string(),
            PropertySchema {
                property_type: "string".to_string(),
                description: description.to_string(),
                items: None,
                enum_values: None,
            },
        );
        if required {
            self.required.push(name.to_string());
        }
        self
    }

    /// Add a number property
    #[allow(dead_code)] // Future use for numeric parameters
    pub fn add_number(mut self, name: &str, description: &str, required: bool) -> Self {
        self.properties.insert(
            name.to_string(),
            PropertySchema {
                property_type: "number".to_string(),
                description: description.to_string(),
                items: None,
                enum_values: None,
            },
        );
        if required {
            self.required.push(name.to_string());
        }
        self
    }

    /// Add a boolean property
    #[allow(dead_code)] // Future use for boolean parameters
    pub fn add_boolean(mut self, name: &str, description: &str, required: bool) -> Self {
        self.properties.insert(
            name.to_string(),
            PropertySchema {
                property_type: "boolean".to_string(),
                description: description.to_string(),
                items: None,
                enum_values: None,
            },
        );
        if required {
            self.required.push(name.to_string());
        }
        self
    }

    /// Convert to OpenAI function parameters format
    pub fn to_openai(&self) -> Value {
        serde_json::to_value(self).expect("Failed to serialize schema")
    }

    /// Convert to Anthropic tool input_schema format
    pub fn to_anthropic(&self) -> Value {
        serde_json::to_value(self).expect("Failed to serialize schema")
    }
}

impl Default for ParameterSchema {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_schema_builder() {
        let schema = ParameterSchema::new()
            .add_string("command", "The command to execute", true)
            .add_boolean("verbose", "Enable verbose output", false);

        assert_eq!(schema.schema_type, "object");
        assert_eq!(schema.properties.len(), 2);
        assert_eq!(schema.required.len(), 1);
        assert_eq!(schema.required[0], "command");
        assert!(schema.properties.contains_key("command"));
        assert!(schema.properties.contains_key("verbose"));
    }

    #[test]
    fn test_schema_to_openai() {
        let schema = ParameterSchema::new()
            .add_string("path", "File path", true);

        let openai_schema = schema.to_openai();
        assert!(openai_schema.is_object());
        
        let obj = openai_schema.as_object().unwrap();
        assert_eq!(obj.get("type").unwrap().as_str().unwrap(), "object");
        assert!(obj.contains_key("properties"));
        assert!(obj.contains_key("required"));
    }

    #[test]
    fn test_schema_to_anthropic() {
        let schema = ParameterSchema::new()
            .add_string("query", "Search query", true);

        let anthropic_schema = schema.to_anthropic();
        assert!(anthropic_schema.is_object());
        
        // Anthropic and OpenAI use the same JSON Schema format
        let obj = anthropic_schema.as_object().unwrap();
        assert_eq!(obj.get("type").unwrap().as_str().unwrap(), "object");
    }
}
