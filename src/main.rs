mod terminal;
mod render;
mod hotkey;
mod agent;
mod config;
mod llm;
mod tools;

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wat")]
#[command(about = "Well Assisted Terminal - Inline terminal assistant", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the agent daemon (listens for hotkey)
    Daemon,
    /// Run agent once (interactive mode)
    Run,
    /// Process a query non-interactively
    Query {
        /// The query to process
        query: String,
    },
    /// Install shell integration
    Install,
    /// Show configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Clear agent history
    Clear,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,
    /// Edit configuration
    Edit,
    /// Reset to defaults
    Reset,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = config::Config::load()?;
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Daemon => {
            run_daemon(config).await?;
        }
        Commands::Run => {
            run_interactive(config).await?;
        }
        Commands::Query { query } => {
            run_query(config, &query).await?;
        }
        Commands::Install => {
            install_shell_integration()?;
        }
        Commands::Config { action } => {
            handle_config(action)?;
        }
        Commands::Clear => {
            clear_history()?;
        }
    }
    
    Ok(())
}

/// Run the agent daemon (listens for hotkey)
async fn run_daemon(config: config::Config) -> Result<()> {
    println!("WAT (Well Assisted Terminal) daemon starting...");
    println!("Press {} to activate agent", config.hotkey.key);
    println!("Press Ctrl+C to exit");
    
    // Create hotkey interceptor
    let (interceptor, rx) = hotkey::HotkeyInterceptor::new();
    let hotkey = hotkey::Hotkey::from_str(&config.hotkey.key)?;
    
    // Start listening for hotkey
    interceptor.start_listening(hotkey.clone())?;
    
    // Fallback: terminal-based hotkey
    let terminal_hotkey = hotkey::TerminalHotkey::new(hotkey);
    
    loop {
        // Try to receive from hotkey interceptor
        if let Ok(()) = rx.try_recv() {
            // Hotkey pressed, run agent
            run_agent_once(&config).await?;
        } else {
            // Fallback to terminal hotkey
            if let Ok(()) = terminal_hotkey.wait() {
                run_agent_once(&config).await?;
            }
        }
        
        // Small sleep to prevent busy loop
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

/// Run agent once (interactive mode)
async fn run_interactive(config: config::Config) -> Result<()> {
    let mut agent = agent::Agent::new(config)?;
    agent.run_interactive().await?;
    Ok(())
}

/// Run a single query non-interactively
async fn run_query(config: config::Config, query: &str) -> Result<()> {
    let simple_agent = agent::SimpleAgent::new(config)?;
    let response = simple_agent.process_query(query).await?;
    
    // Just print the response
    println!("{}", response);
    
    Ok(())
}

/// Run agent once (for daemon)
async fn run_agent_once(config: &config::Config) -> Result<()> {
    let mut agent = agent::Agent::new(config.clone())?;
    
    // Run in a separate task to avoid blocking the daemon
    let _config_clone = config.clone();
    tokio::spawn(async move {
        if let Err(e) = agent.run_interactive().await {
            eprintln!("Agent error: {}", e);
        }
    });
    
    Ok(())
}

/// Install shell integration
fn install_shell_integration() -> Result<()> {
    println!("Installing WAT shell integration...");
    
    // Determine shell
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
    let rc_file = if shell.contains("zsh") {
        "~/.zshrc"
    } else {
        "~/.bashrc"
    };
    
    // Get binary path
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    let binary_path = current_exe.to_string_lossy();
    
    let install_cmd = format!(
        "\n# WAT - Well Assisted Terminal\n\
         alias wat='{}'\n\
         # Quick access to agent\n\
         alias wa='{} run'\n\
         # Bind F2 to trigger agent (if supported)\n\
         bind -x '\"\\eOQ\":\"{} run\"' 2>/dev/null || true\n",
        binary_path, binary_path, binary_path
    );
    
    println!("Add the following to {}:", rc_file);
    println!("{}", install_cmd);
    
    // Also suggest systemd service for Linux
    if cfg!(target_os = "linux") {
        println!("\nFor background daemon (Linux with systemd):");
        println!("  systemctl --user enable wat-daemon");
        println!("  systemctl --user start wat-daemon");
    }
    
    Ok(())
}

/// Handle configuration commands
fn handle_config(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let config = config::Config::load()?;
            let config_str = toml::to_string_pretty(&config)?;
            println!("{}", config_str);
        }
        ConfigAction::Edit => {
            let config_path = config::Config::config_path()?;
            println!("Edit configuration file: {}", config_path.display());
            
            // Try to open with default editor
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
            let status = std::process::Command::new(editor)
                .arg(&config_path)
                .status();
            
            match status {
                Ok(_) => println!("Configuration updated"),
                Err(e) => eprintln!("Failed to open editor: {}", e),
            }
        }
        ConfigAction::Reset => {
            let config = config::Config::default();
            config.save()?;
            println!("Configuration reset to defaults");
        }
    }
    
    Ok(())
}

/// Clear agent history
fn clear_history() -> Result<()> {
    let history_path = config::Config::history_path()?;
    if history_path.exists() {
        std::fs::remove_file(&history_path)?;
        println!("History cleared");
    } else {
        println!("No history found");
    }
    
    Ok(())
}
