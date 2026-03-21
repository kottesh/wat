//! Basic usage examples for WAT

use wat::{SimpleAgent, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Example 1: Load configuration
    let config = Config::load()?;
    println!("Loaded configuration for model: {}", config.llm.model);

    // Example 2: Non-interactive query
    let simple_agent = SimpleAgent::new(config.clone())?;
    let response = simple_agent.process_query("List files in current directory").await?;
    println!("Response: {}", response);

    // Example 3: Interactive agent (would take over terminal)
    // Uncomment to run interactive mode:
    // use wat::Agent;
    // let mut agent = Agent::new(config)?;
    // agent.run_interactive().await?;

    Ok(())
}
