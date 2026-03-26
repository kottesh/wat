# WAT - Well Assisted Terminal

An inline terminal assistant with an agentic loop. Type your request, and it executes commands to help you.

![wat](https://github.com/user-attachments/assets/20e43e08-94a6-4226-a56e-315107183541)

## Features

- **Inline UI** - Appears at your command line with a clean input box
- **Native tool calling** - Uses OpenAI function calling and Anthropic tool use APIs
- **Agentic loop** - Automatically executes bash commands and continues until done
- **Streaming responses** - Real-time text and tool call streaming
- **Multiple tools** - Bash execution, file reading, and more
- **Multi-LLM support** - OpenAI, Anthropic, or any OpenAI-compatible API
- **Multiple providers** - Configure multiple LLM providers and switch between them
- **120s bash timeout** - Automatic timeout with cancellation support

## Installation

```bash
git clone https://github.com/kottesh/wat
cd wat
cargo build --release
```

Binary will be at `target/release/wat`

## Configuration

Create `~/.config/wat/models.json`:

```json
{
  "activeProvider": "right-codex",
  "activeModel": "gpt-5.3-codex",
  "providers": {
    "openai": {
      "baseUrl": "https://api.openai.com/v1",
      "api": "openai-completions",
      "apiKey": "${OPENAI_API_KEY}",
      "models": [
        { "id": "gpt-4", "name": "GPT-4" },
        { "id": "gpt-4-turbo", "name": "GPT-4 Turbo" },
        { "id": "gpt-3.5-turbo", "name": "GPT-3.5 Turbo" }
      ]
    },
    "anthropic": {
      "baseUrl": "https://api.anthropic.com/v1",
      "api": "anthropic-messages",
      "apiKey": "${ANTHROPIC_API_KEY}",
      "models": [
        { "id": "claude-3-opus-20240229", "name": "Claude 3 Opus" },
        { "id": "claude-3-sonnet-20240229", "name": "Claude 3 Sonnet" }
      ]
    }
  }
}
```

### Configuration Fields

- **`activeProvider`** - The provider to use (must match a key in `providers`)
- **`activeModel`** - The model ID to use (must exist in the active provider's models)
- **`providers`** - Map of provider configurations

### Provider Fields

- **`baseUrl`** - Base URL for the API (without `/chat/completions` or `/messages`)
- **`api`** - API format to use:
  - `openai-completions` - OpenAI chat completions format (works with OpenAI, local models via Ollama, LM Studio, etc.)
  - `anthropic-messages` - Anthropic messages format
- **`apiKey`** - API key (supports environment variable expansion like `${OPENAI_API_KEY}`)
- **`models`** - Array of available models with `id` and `name`

### Environment Variables

API keys support environment variable expansion:

```json
"apiKey": "${OPENAI_API_KEY}"
```

This will expand to the value of the `OPENAI_API_KEY` environment variable.

### Example Configurations

**OpenAI:**
```json
{
  "activeProvider": "openai",
  "activeModel": "gpt-4",
  "providers": {
    "openai": {
      "baseUrl": "https://api.openai.com/v1",
      "api": "openai-completions",
      "apiKey": "${OPENAI_API_KEY}",
      "models": [
        { "id": "gpt-4", "name": "GPT-4" }
      ]
    }
  }
}
```

**Anthropic:**
```json
{
  "activeProvider": "anthropic",
  "activeModel": "claude-3-opus-20240229",
  "providers": {
    "anthropic": {
      "baseUrl": "https://api.anthropic.com/v1",
      "api": "anthropic-messages",
      "apiKey": "${ANTHROPIC_API_KEY}",
      "models": [
        { "id": "claude-3-opus-20240229", "name": "Claude 3 Opus" }
      ]
    }
  }
}
```

**Local (Ollama):**
```json
{
  "activeProvider": "local",
  "activeModel": "llama3",
  "providers": {
    "local": {
      "baseUrl": "http://localhost:11434/v1",
      "api": "openai-completions",
      "apiKey": "not-needed",
      "models": [
        { "id": "llama3", "name": "Llama 3" },
        { "id": "codellama", "name": "Code Llama" }
      ]
    }
  }
}
```

**Custom OpenAI-compatible:**
```json
{
  "activeProvider": "custom",
  "activeModel": "your-model",
  "providers": {
    "custom": {
      "baseUrl": "https://your-api.com/v1",
      "api": "openai-completions",
      "apiKey": "${YOUR_API_KEY}",
      "models": [
        { "id": "your-model", "name": "Your Model" }
      ]
    }
  }
}
```

### Switching Models

To switch providers or models, edit `models.json` and change the `activeProvider` and/or `activeModel` fields:

```json
{
  "activeProvider": "anthropic",
  "activeModel": "claude-3-sonnet-20240229",
  ...
}
```

## Usage

```bash
# Set your API key (if using environment variables)
export OPENAI_API_KEY="your-key"

# Run the agent
wat
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
