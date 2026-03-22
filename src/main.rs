mod terminal;
mod renderer;
mod component;
mod components;
mod agent;
mod config;
mod llm;
mod tools;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::load()?;
    let mut agent = agent::Agent::new(config)?;
    agent.run_interactive().await?;
    Ok(())
}
