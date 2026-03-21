#!/bin/bash
# Basic test for WAT

echo "Testing WAT compilation..."
cargo build --release

echo ""
echo "Testing configuration..."
cargo run -- config show

echo ""
echo "Testing help..."
cargo run -- --help

echo ""
echo "Testing query command (will fail without API key)..."
cargo run -- query "list files" || echo "Expected error without API key"

echo ""
echo "Project structure:"
find . -name "*.rs" -type f | head -20