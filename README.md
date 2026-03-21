# WAT - Well Assisted Terminal

An inline terminal assistant that appears at your command line when you need help.

## Features

- **Inline interface**: Appears at the command line, not as an overlay
- **Context-aware**: Knows your current directory, git status, shell history
- **Safe execution**: Warns before dangerous commands
- **Multi-LLM support**: OpenAI, Anthropic, local models
- **Tool calling**: Can execute commands, read files, search
- **Session persistence**: Remembers conversation history

## Installation

### From source
```bash
git clone https://github.com/yourusername/wat
cd wat
cargo install --path .
```

### Shell integration
```bash
wat install
```

## Usage

### Interactive mode
```bash
# Start the agent daemon
wat daemon

# Press F2 in any terminal to summon the agent
```

### One-off mode
```bash
# Run agent once
wat run

# Direct query
wat query "find large files in current directory"
```

## Configuration

Create `~/.config/wat/config.toml`:

```toml
[llm]
provider = "openai"  # openai, anthropic, local
model = "gpt-4"
api_key = "${OPENAI_API_KEY}"

[hotkey]
key = "F2"  # F2, Ctrl+Alt+;, etc.

[ui]
theme = "dark"
prompt = "🤖 > "
```

## How it works

1. **Hotkey pressed** (F2 by default)
2. **Agent takes over** the command line
3. **You type your query** inline
4. **Agent thinks** and shows progress
5. **Tools are called** (commands executed)
6. **Results shown** inline
7. **Returns to shell** when done

## Example

```
$ ls
file1.txt file2.txt

[Press F2]
🤖 > find files modified today

🤔 Thinking...
  • Looking for files modified within 24h
🔧 Running: find . -type f -mtime 0
  📋 ./file1.txt
💡 Found 1 file modified today

$  # Back to normal shell
```

## License

MIT OR Apache-2.0