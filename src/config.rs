use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use dirs;

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub hotkey: HotkeyConfig,
    pub ui: UiConfig,
    pub tools: ToolsConfig,
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
}

/// LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmProvider {
    OpenAI,
    Anthropic,
    Local,
    Custom,
}

impl LlmProvider {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(LlmProvider::OpenAI),
            "anthropic" => Ok(LlmProvider::Anthropic),
            "local" => Ok(LlmProvider::Local),
            "custom" => Ok(LlmProvider::Custom),
            _ => anyhow::bail!("Unknown LLM provider: {}", s),
        }
    }
    
    #[allow(dead_code)]
    pub fn to_string(&self) -> String {
        match self {
            LlmProvider::OpenAI => "openai".to_string(),
            LlmProvider::Anthropic => "anthropic".to_string(),
            LlmProvider::Local => "local".to_string(),
            LlmProvider::Custom => "custom".to_string(),
        }
    }
}

/// Hotkey configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub key: String,
    pub enabled: bool,
}

/// UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
    pub prompt: String,
    pub use_colors: bool,
    pub show_thinking: bool,
    pub show_tools: bool,
}

/// Tools configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    pub allow_command_execution: bool,
    pub confirm_dangerous_commands: bool,
    pub max_output_lines: u32,
    pub allowed_commands: Vec<String>,
    pub blocked_commands: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig {
                provider: LlmProvider::OpenAI,
                model: "gpt-4".to_string(),
                api_key: "${OPENAI_API_KEY}".to_string(),
                base_url: None,
                temperature: 0.3,
                max_tokens: 2000,
            },
            hotkey: HotkeyConfig {
                key: "F2".to_string(),
                enabled: true,
            },
            ui: UiConfig {
                theme: "dark".to_string(),
                prompt: "> ".to_string(),
                use_colors: true,
                show_thinking: true,
                show_tools: true,
            },
            tools: ToolsConfig {
                allow_command_execution: true,
                confirm_dangerous_commands: true,
                max_output_lines: 50,
                allowed_commands: vec![
                    "ls".to_string(),
                    "find".to_string(),
                    "grep".to_string(),
                    "cat".to_string(),
                    "head".to_string(),
                    "tail".to_string(),
                    "wc".to_string(),
                    "du".to_string(),
                    "df".to_string(),
                    "ps".to_string(),
                    "git".to_string(),
                ],
                blocked_commands: vec![
                    "rm -rf".to_string(),
                    "chmod 777".to_string(),
                    "dd".to_string(),
                    "mkfs".to_string(),
                    "fdisk".to_string(),
                ],
            },
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        
        if config_path.exists() {
            let config_str = std::fs::read_to_string(&config_path)
                .context("Failed to read config file")?;
            
            let mut config: Config = toml::from_str(&config_str)
                .context("Failed to parse config file")?;
            
            // Expand environment variables in API key
            config.llm.api_key = shellexpand::env(&config.llm.api_key)
                .unwrap_or(std::borrow::Cow::Borrowed(&config.llm.api_key))
                .to_string();
            
            Ok(config)
        } else {
            // Create default config
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }
    
    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        
        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }
        
        let config_str = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        std::fs::write(&config_path, config_str)
            .context("Failed to write config file")?;
        
        Ok(())
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
        Ok(Self::config_dir()?.join("config.toml"))
    }
    
    /// Get data directory path
    pub fn data_dir() -> Result<PathBuf> {
        let dir = dirs::data_dir()
            .context("Failed to get data directory")?
            .join("wat");
        
        Ok(dir)
    }
    
    /// Get history file path
    pub fn history_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("history.json"))
    }
    
    /// Get sessions directory path
    #[allow(dead_code)]
    pub fn sessions_dir() -> Result<PathBuf> {
        let dir = Self::data_dir()?.join("sessions");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}