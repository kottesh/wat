mod agent;
mod component;
mod components;
mod config;
mod llm;
mod terminal;
mod tools;
mod ui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Load models configuration and convert to runtime config
    let models_config = config::ModelsConfig::load()?;
    let config = models_config.to_config()?;

    let mut agent = agent::Agent::new(config)?;
    agent.run_interactive().await?;
    Ok(())
}
