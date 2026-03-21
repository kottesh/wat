# WAT - Well Assisted Terminal

An inline terminal assistant with an agentic loop. Type your request, and it executes commands to help you.

## Features

- **Inline UI** - Appears at your command line with a clean input box
- **Agentic loop** - Automatically executes bash commands and continues until done
- **Bash tool** - Runs shell commands in ```bash blocks
- **Safe execution** - Refuses dangerous commands (rm -rf /, etc.)
- **Multi-LLM support** - OpenAI, Anthropic, ZhipuAI, or any OpenAI-compatible API

## Installation

```bash
git clone https://github.com/kottesh/wat
cd wat
cargo build --release
```

Binary will be at `target/release/wat`

## Configuration

Create `~/.config/wat/config.toml`:

```toml
[llm]
provider = "Custom"
model = "glm-4-flash"
api_key = "${ZHIPUAI_API_KEY}"
base_url = "https://open.bigmodel.cn/api/paas/v4/chat/completions"
temperature = 0.3
max_tokens = 2000

[ui]
use_colors = true
```

### Providers

**OpenAI:**
```toml
[llm]
provider = "OpenAI"
model = "gpt-4"
api_key = "${OPENAI_API_KEY}"
```

**Anthropic:**
```toml
[llm]
provider = "Anthropic"
model = "claude-3-sonnet-20240229"
api_key = "${ANTHROPIC_API_KEY}"
```

**Custom (OpenAI-compatible):**
```toml
[llm]
provider = "Custom"
model = "your-model"
api_key = "${YOUR_API_KEY}"
base_url = "https://your-api.com/v1/chat/completions"
```

## Usage

```bash
# Set your API key
export ZHIPUAI_API_KEY="your-key"

# Run the agent
wat run
```

### Commands

- Type your request and press Enter
- `clear` - Clear conversation history
- `exit`, `quit`, `q`, or Ctrl+C - Exit

## How it works

1. You type a request
2. LLM responds, optionally with ```bash blocks
3. Bash commands are automatically executed
4. Output is shown and fed back to the LLM
5. Loop continues until LLM responds without commands
6. Ready for next input

## License

MIT
