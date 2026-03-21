#!/bin/bash
# WAT installation script

set -e

echo "Installing WAT (Well Assisted Terminal)..."

# Build release binary
echo "Building WAT..."
cargo build --release

# Install binary
echo "Installing binary to /usr/local/bin/wat..."
sudo cp target/release/wat /usr/local/bin/wat

# Create configuration directory
echo "Creating configuration directory..."
mkdir -p ~/.config/wat

# Create default config if it doesn't exist
if [ ! -f ~/.config/wat/config.toml ]; then
    echo "Creating default configuration..."
    cat > ~/.config/wat/config.toml << 'EOF'
[llm]
provider = "openai"
model = "gpt-4"
api_key = "${OPENAI_API_KEY}"
temperature = 0.3
max_tokens = 2000

[hotkey]
key = "F2"
enabled = true

[ui]
theme = "dark"
prompt = "🤖 > "
use_colors = true
show_thinking = true
show_tools = true

[tools]
allow_command_execution = true
confirm_dangerous_commands = true
max_output_lines = 50
allowed_commands = [
    "ls",
    "find",
    "grep",
    "cat",
    "head",
    "tail",
    "wc",
    "du",
    "df",
    "ps",
    "git",
]
blocked_commands = [
    "rm -rf",
    "chmod 777",
    "dd",
    "mkfs",
    "fdisk",
]
EOF
fi

# Create data directory
echo "Creating data directory..."
mkdir -p ~/.local/share/wat/sessions

# Install shell integration
echo "Installing shell integration..."
SHELL_RC="$HOME/.bashrc"
if [[ "$SHELL" == *"zsh"* ]]; then
    SHELL_RC="$HOME/.zshrc"
fi

if ! grep -q "WAT - Well Assisted Terminal" "$SHELL_RC"; then
    cat >> "$SHELL_RC" << 'EOF'

# WAT - Well Assisted Terminal
alias wat='/usr/local/bin/wat'
alias wa='/usr/local/bin/wat run'
# Bind F2 to trigger agent (if supported)
bind -x '"\eOQ":"/usr/local/bin/wat run"' 2>/dev/null || true
EOF
    echo "Added WAT aliases to $SHELL_RC"
else
    echo "WAT already installed in $SHELL_RC"
fi

# Create systemd service for Linux
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo "Creating systemd service..."
    mkdir -p ~/.config/systemd/user
    
    cat > ~/.config/systemd/user/wat-daemon.service << EOF
[Unit]
Description=WAT Terminal Assistant Daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/local/bin/wat daemon
Restart=on-failure
Environment="DISPLAY=:0"
Environment="TERM=xterm-256color"
Environment="OPENAI_API_KEY=${OPENAI_API_KEY}"

[Install]
WantedBy=default.target
EOF
    
    systemctl --user daemon-reload
    echo "Systemd service created"
    echo "To enable: systemctl --user enable wat-daemon"
    echo "To start: systemctl --user start wat-daemon"
fi

echo ""
echo "Installation complete! 🎉"
echo ""
echo "Next steps:"
echo "1. Set your OpenAI API key:"
echo "   export OPENAI_API_KEY='your-key-here'"
echo "   Or edit ~/.config/wat/config.toml"
echo ""
echo "2. Source your shell config:"
echo "   source $SHELL_RC"
echo ""
echo "3. Try it out:"
echo "   wa                    # Run agent once"
echo "   wat daemon            # Start daemon (press F2)"
echo "   wat query 'find large files'  # Non-interactive"
echo ""
echo "For help: wat --help"