use std::path::PathBuf;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use dirs;

/// Models configuration loaded from models.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsConfig {
    pub active_provider: String,
    pub active_model: String,
    pub providers: HashMap<String, Provider>,
}

/// Provider definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub base_url: String,
    pub api: String,
    pub api_key: String,
    pub models: Vec<Model>,
}

/// Model within a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
}

/// API type enum
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ApiType {
    OpenAiCompletions,
    AnthropicMessages,
}

impl ApiType {
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "openai-completions" => Ok(ApiType::OpenAiCompletions),
            "anthropic-messages" => Ok(ApiType::AnthropicMessages),
            _ => anyhow::bail!("Unknown API type: {}. Valid options: openai-completions, anthropic-messages", s),
        }
    }
}

/// Runtime configuration used by the application
#[derive(Debug, Clone)]
pub struct Config {
    #[allow(dead_code)] // Stored for future /model command
    pub provider_name: String,
    pub model_id: String,
    #[allow(dead_code)] // Stored for future UI display
    pub model_name: String,
    pub base_url: String,
    pub api_type: ApiType,
    pub api_key: String,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();
        
        // OpenAI provider
        providers.insert("openai".to_string(), Provider {
            base_url: "https://api.openai.com/v1".to_string(),
            api: "openai-completions".to_string(),
            api_key: "${OPENAI_API_KEY}".to_string(),
            models: vec![
                Model { id: "gpt-4".to_string(), name: "GPT-4".to_string() },
                Model { id: "gpt-4-turbo".to_string(), name: "GPT-4 Turbo".to_string() },
                Model { id: "gpt-3.5-turbo".to_string(), name: "GPT-3.5 Turbo".to_string() },
            ],
        });
        
        // Anthropic provider
        providers.insert("anthropic".to_string(), Provider {
            base_url: "https://api.anthropic.com/v1".to_string(),
            api: "anthropic-messages".to_string(),
            api_key: "${ANTHROPIC_API_KEY}".to_string(),
            models: vec![
                Model { id: "claude-3-opus-20240229".to_string(), name: "Claude 3 Opus".to_string() },
                Model { id: "claude-3-sonnet-20240229".to_string(), name: "Claude 3 Sonnet".to_string() },
                Model { id: "claude-3-haiku-20240307".to_string(), name: "Claude 3 Haiku".to_string() },
            ],
        });
        
        Self {
            active_provider: "openai".to_string(),
            active_model: "gpt-4".to_string(),
            providers,
        }
    }
}

impl ModelsConfig {
    /// Load configuration from models.json
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        
        if path.exists() {
            let json_str = std::fs::read_to_string(&path)
                .context("Failed to read models.json")?;
            
            let mut config: ModelsConfig = serde_json::from_str(&json_str)
                .context("Failed to parse models.json")?;
            
            // Expand environment variables in all API keys
            for provider in config.providers.values_mut() {
                provider.api_key = shellexpand::env(&provider.api_key)
                    .unwrap_or(std::borrow::Cow::Borrowed(&provider.api_key))
                    .to_string();
            }
            
            Ok(config)
        } else {
            // Create default config
            let config = Self::default();
            config.save()?;
            eprintln!("Created default configuration at: {}", path.display());
            Ok(config)
        }
    }
    
    /// Save configuration to models.json
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        
        // Create config directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }
        
        let json_str = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        std::fs::write(&path, json_str)
            .context("Failed to write models.json")?;
        
        Ok(())
    }
    
    /// Convert to runtime Config
    pub fn to_config(&self) -> Result<Config> {
        // Get the active provider
        let provider = self.providers.get(&self.active_provider)
            .with_context(|| format!("Active provider '{}' not found in configuration", self.active_provider))?;
        
        // Find the active model
        let model = provider.models.iter()
            .find(|m| m.id == self.active_model)
            .with_context(|| format!("Active model '{}' not found in provider '{}'", self.active_model, self.active_provider))?;
        
        // Parse API type
        let api_type = ApiType::from_str(&provider.api)?;
        
        Ok(Config {
            provider_name: self.active_provider.clone(),
            model_id: model.id.clone(),
            model_name: model.name.clone(),
            base_url: provider.base_url.clone(),
            api_type,
            api_key: provider.api_key.clone(),
        })
    }
    
    /// Get config directory path
    pub fn config_dir() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .context("Failed to get config directory")?
            .join("wat");
        
        Ok(dir)
    }
    
    /// Get config file path
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("models.json"))
    }
    
    /// List all provider names
    #[allow(dead_code)] // Public API for /model command
    pub fn list_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
    
    /// List models for a specific provider
    #[allow(dead_code)] // Public API for /model command
    pub fn list_models(&self, provider: &str) -> Option<&Vec<Model>> {
        self.providers.get(provider).map(|p| &p.models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_models_config() {
        let json = r#"{
            "activeProvider": "test",
            "activeModel": "test-model-1",
            "providers": {
                "test": {
                    "baseUrl": "https://example.com/v1",
                    "api": "openai-completions",
                    "apiKey": "test-key",
                    "models": [
                        {"id": "test-model-1", "name": "Test Model 1"},
                        {"id": "test-model-2", "name": "Test Model 2"}
                    ]
                }
            }
        }"#;

        let config: ModelsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.active_provider, "test");
        assert_eq!(config.active_model, "test-model-1");
        assert_eq!(config.providers.len(), 1);
        assert!(config.providers.contains_key("test"));
    }

    #[test]
    fn test_to_runtime_config() {
        let json = r#"{
            "activeProvider": "test",
            "activeModel": "test-model-1",
            "providers": {
                "test": {
                    "baseUrl": "https://example.com/v1",
                    "api": "openai-completions",
                    "apiKey": "test-key",
                    "models": [
                        {"id": "test-model-1", "name": "Test Model 1"}
                    ]
                }
            }
        }"#;

        let models_config: ModelsConfig = serde_json::from_str(json).unwrap();
        let config = models_config.to_config().unwrap();
        
        assert_eq!(config.provider_name, "test");
        assert_eq!(config.model_id, "test-model-1");
        assert_eq!(config.model_name, "Test Model 1");
        assert_eq!(config.base_url, "https://example.com/v1");
        assert_eq!(config.api_type, ApiType::OpenAiCompletions);
        assert_eq!(config.api_key, "test-key");
    }

    #[test]
    fn test_api_type_parsing() {
        assert!(matches!(
            ApiType::from_str("openai-completions").unwrap(),
            ApiType::OpenAiCompletions
        ));
        assert!(matches!(
            ApiType::from_str("anthropic-messages").unwrap(),
            ApiType::AnthropicMessages
        ));
        assert!(ApiType::from_str("invalid").is_err());
    }

    #[test]
    fn test_missing_provider_error() {
        let json = r#"{
            "activeProvider": "missing",
            "activeModel": "test-model",
            "providers": {
                "test": {
                    "baseUrl": "https://example.com/v1",
                    "api": "openai-completions",
                    "apiKey": "test-key",
                    "models": [{"id": "test-model", "name": "Test"}]
                }
            }
        }"#;

        let models_config: ModelsConfig = serde_json::from_str(json).unwrap();
        let result = models_config.to_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

    #[test]
    fn test_missing_model_error() {
        let json = r#"{
            "activeProvider": "test",
            "activeModel": "missing-model",
            "providers": {
                "test": {
                    "baseUrl": "https://example.com/v1",
                    "api": "openai-completions",
                    "apiKey": "test-key",
                    "models": [{"id": "test-model", "name": "Test"}]
                }
            }
        }"#;

        let models_config: ModelsConfig = serde_json::from_str(json).unwrap();
        let result = models_config.to_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing-model"));
    }
}
