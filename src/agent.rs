use anyhow::Result;
use crate::{
    terminal::TerminalState,
    render::InlineRenderer,
    llm::{LlmClient, Message},
    tools::{self, Tool, execute_tool, is_dangerous},
    config::Config,
};

/// Main agent that handles the conversation
pub struct Agent {
    terminal: TerminalState,
    renderer: InlineRenderer,
    llm_client: LlmClient,
    history: Vec<Message>,
    last_terminal_width: usize,
}

impl Agent {
    /// Create new agent
    pub fn new(config: Config) -> Result<Self> {
        let terminal = TerminalState::new()?;
        let renderer = InlineRenderer::new(config.ui.use_colors);
        let llm_client = LlmClient::new(config)?;
        let initial_width = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
        
        Ok(Self {
            terminal,
            renderer,
            llm_client,
            history: Vec::new(),
            last_terminal_width: initial_width,
        })
    }
    
    /// Run the agent loop
    pub async fn run_interactive(&mut self) -> Result<()> {
        loop {
            // Check for terminal resize and re-render if needed
            self.check_and_handle_resize()?;
            
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
            
            // Parse for tools
            let tools = tools::parse_tools(&response.content);
            
            if tools.is_empty() {
                // No tools - show response and done
                self.history.push(Message::assistant(&response.content));
                self.renderer.render_response(&response.content)?;
                break;
            }
            
            // Show the response without the tool code blocks (those render separately)
            let display_response = tools::strip_tool_blocks(&response.content);
            if !display_response.is_empty() {
                self.renderer.render_response(&display_response)?;
            }
            self.history.push(Message::assistant(&response.content));
            
            // Execute tools
            let mut all_results = String::new();
            
            for tool in &tools {
                match tool {
                    Tool::Bash { command } => {
                        // Check for dangerous commands
                        if is_dangerous(command) {
                            self.renderer.render_error(&format!("Refusing dangerous command: {}", command))?;
                            all_results.push_str(&format!("Command refused (dangerous): {}\n", command));
                            continue;
                        }
                        
                        // Execute with timing
                        let result = execute_tool(tool)?;
                        
                        // Show result with timing, success status, and command (renders header with result)
                        self.renderer.render_tool_result_with_timing(&result.output, Some(result.duration_secs), Some("bash"), result.success, Some(command))?;
                        
                        all_results.push_str(&format!("$ {}\n{}\n", command, result.output));
                    }
                    Tool::ReadFile { path } => {
                        // Show what we're reading
                        self.renderer.render_tool_call("read_file", path)?;
                        
                        // Execute
                        let result = execute_tool(tool)?;
                        
                        // Show result with timing
                        self.renderer.render_tool_result_with_timing(&result.output, Some(result.duration_secs), Some("read_file"), result.success, None)?;
                        
                        all_results.push_str(&format!("File: {}\n{}\n", path, result.output));
                    }
                }
            }
            
            // Add results to history for next iteration
            if !all_results.is_empty() {
                self.history.push(Message::user(&format!("Tool output:\n{}", all_results)));
            }
        }
        
        Ok(())
    }
    
    /// Check for terminal resize and re-render conversation if needed
    fn check_and_handle_resize(&mut self) -> Result<()> {
        let current_width = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
        
        if current_width != self.last_terminal_width {
            self.last_terminal_width = current_width;
            
            // Clear screen and re-render entire conversation
            print!("\x1b[2J\x1b[H"); // Clear screen and move to top
            
            // Re-render conversation history in chronological order
            for msg in &self.history {
                if msg.role == "user" {
                    self.renderer.render_user_input(&msg.content)?;
                } else if msg.role == "assistant" {
                    self.renderer.render_response(&msg.content)?;
                }
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
- read_file: Read file contents. Put the file path in a ```read_file code block. Shows line numbers.

Current directory: {}

When asked to do something, use the appropriate tool. Show the tool call you're making."#, cwd)
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
