use anyhow::Result;
use crate::{
    terminal::TerminalState,
    render::InlineRenderer,
    llm::{LlmClient, Message},
    tools::{bash, parse_bash_commands, is_dangerous, BashResult},
    config::Config,
};

/// Main agent that handles the conversation
pub struct Agent {
    config: Config,
    terminal: TerminalState,
    renderer: InlineRenderer,
    llm_client: LlmClient,
    history: Vec<Message>,
}

impl Agent {
    /// Create new agent
    pub fn new(config: Config) -> Result<Self> {
        let terminal = TerminalState::new()?;
        let renderer = InlineRenderer::new(config.ui.use_colors);
        let llm_client = LlmClient::new(config.clone())?;
        
        Ok(Self {
            config,
            terminal,
            renderer,
            llm_client,
            history: Vec::new(),
        })
    }
    
    /// Run the agent loop
    pub async fn run_interactive(&mut self) -> Result<()> {
        loop {
            // Get user input
            self.terminal.enter_agent_mode()?;
            let input = self.terminal.read_line("");
            self.terminal.exit_agent_mode()?;
            
            let input = match input {
                Ok(i) => i,
                Err(_) => break,
            };
            
            let input = input.trim();
            
            // Exit conditions
            if input.is_empty() || input == "exit" || input == "quit" || input == "q" {
                break;
            }
            
            // Show user input
            self.renderer.render_user_input(input)?;
            
            // Special commands
            if input == "clear" {
                self.history.clear();
                println!("History cleared.");
                continue;
            }
            
            // Process with agent loop
            if let Err(e) = self.agent_loop(input).await {
                self.renderer.render_error(&e.to_string())?;
            }
        }
        
        Ok(())
    }
    
    /// Main agent loop - handles tool use
    async fn agent_loop(&mut self, query: &str) -> Result<()> {
        // Add user message
        self.history.push(Message::user(query));
        
        // Max iterations to prevent infinite loops
        let max_iterations = 10;
        
        for _ in 0..max_iterations {
            // Build messages
            let system = self.system_prompt();
            let mut messages = vec![Message::system(&system)];
            messages.extend(self.history.clone());
            
            // Get LLM response
            let spinner = self.renderer.start_spinner("Thinking...");
            let response = self.llm_client.query(messages).await;
            spinner.stop();
            
            let response = response?;
            
            // Parse for bash commands
            let commands = parse_bash_commands(&response.content);
            
            if commands.is_empty() {
                // No commands - show response and done
                self.history.push(Message::assistant(&response.content));
                self.renderer.render_response(&response.content)?;
                break;
            }
            
            // Show the response (which contains the command)
            self.renderer.render_response(&response.content)?;
            self.history.push(Message::assistant(&response.content));
            
            // Execute commands
            let mut all_results = String::new();
            
            for cmd in &commands {
                // Check for dangerous commands
                if is_dangerous(cmd) {
                    self.renderer.render_error(&format!("Refusing dangerous command: {}", cmd))?;
                    all_results.push_str(&format!("Command refused (dangerous): {}\n", cmd));
                    continue;
                }
                
                // Show what we're running
                self.renderer.render_tool_call("bash", cmd)?;
                
                // Execute
                let result = bash(cmd)?;
                let output = result.output_truncated(50);
                
                // Show result
                self.renderer.render_tool_result(&output)?;
                
                all_results.push_str(&format!("$ {}\n{}\n", cmd, output));
            }
            
            // Add results to history for next iteration
            if !all_results.is_empty() {
                self.history.push(Message::user(&format!("Command output:\n{}", all_results)));
            }
        }
        
        Ok(())
    }
    
    /// Build system prompt
    fn system_prompt(&self) -> String {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        
        format!(r#"You are WAT (Well Assisted Terminal), a command-line assistant.

Tools:
- bash: Execute shell commands. Put commands in ```bash code blocks.

Current directory: {}

When asked to do something, run the appropriate command. Show the command you're running."#, cwd)
    }
}

/// Simple agent for non-interactive queries
pub struct SimpleAgent {
    llm_client: LlmClient,
}

impl SimpleAgent {
    pub fn new(config: Config) -> Result<Self> {
        let llm_client = LlmClient::new(config)?;
        Ok(Self { llm_client })
    }
    
    pub async fn process_query(&self, query: &str) -> Result<String> {
        let messages = vec![
            Message::system("You are WAT, a terminal assistant. Be concise."),
            Message::user(query),
        ];
        
        let response = self.llm_client.query(messages).await?;
        Ok(response.content)
    }
}
