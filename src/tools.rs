use std::process::Command;
use anyhow::Result;

/// Execute a bash command and return output
pub fn bash(command: &str) -> Result<BashResult> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    
    Ok(BashResult {
        success: output.status.success(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
    })
}

/// Result of a bash command
#[derive(Debug, Clone)]
pub struct BashResult {
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl BashResult {
    /// Get combined output (stdout + stderr)
    pub fn output(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
    
    /// Get truncated output
    pub fn output_truncated(&self, max_lines: usize) -> String {
        let output = self.output();
        let lines: Vec<&str> = output.lines().collect();
        
        if lines.len() > max_lines {
            let truncated: Vec<&str> = lines[..max_lines].to_vec();
            format!("{}\n... ({} more lines)", truncated.join("\n"), lines.len() - max_lines)
        } else {
            output
        }
    }
}

/// Parse bash commands from LLM response
pub fn parse_bash_commands(response: &str) -> Vec<String> {
    let mut commands = Vec::new();
    
    // Look for ```bash or ```sh blocks
    let markers = ["```bash\n", "```sh\n", "```shell\n", "```\n"];
    let end_marker = "```";
    
    for marker in markers {
        let mut search_start = 0;
        while let Some(start) = response[search_start..].find(marker) {
            let abs_start = search_start + start;
            let command_start = abs_start + marker.len();
            
            if let Some(end) = response[command_start..].find(end_marker) {
                let command = response[command_start..command_start + end].trim();
                if !command.is_empty() {
                    commands.push(command.to_string());
                }
                search_start = command_start + end + end_marker.len();
            } else {
                break;
            }
        }
    }
    
    commands
}

/// Check if a command looks dangerous
pub fn is_dangerous(command: &str) -> bool {
    let dangerous = [
        "rm -rf /",
        "rm -rf ~",
        "rm -rf *",
        "mkfs",
        "dd if=",
        "> /dev/sd",
        "chmod -R 777 /",
        ":(){ :|:& };:",
    ];
    
    dangerous.iter().any(|d| command.contains(d))
}
