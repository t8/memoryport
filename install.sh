#!/bin/sh
set -e

REPO="t8/memoryport"
INSTALL_DIR="${UC_INSTALL_DIR:-$HOME/.memoryport/bin}"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux) OS="linux" ;;
  darwin) OS="macos" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH="x64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

PLATFORM="${OS}-${ARCH}"
echo "Detected platform: $PLATFORM"

# Get latest release tag
LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
if [ -z "$LATEST" ]; then
  echo "Could not determine latest release. Check https://github.com/$REPO/releases"
  exit 1
fi
echo "Latest release: $LATEST"

# Download
URL="https://github.com/$REPO/releases/download/$LATEST/memoryport-${PLATFORM}.tar.gz"
echo "Downloading $URL..."
TMPDIR=$(mktemp -d)
curl -fsSL "$URL" -o "$TMPDIR/uc.tar.gz"

# Extract
mkdir -p "$INSTALL_DIR"
tar -xzf "$TMPDIR/uc.tar.gz" -C "$INSTALL_DIR"
chmod +x "$INSTALL_DIR/uc" "$INSTALL_DIR/uc-mcp" "$INSTALL_DIR/uc-proxy" "$INSTALL_DIR/uc-server"
rm -rf "$TMPDIR"

echo ""
echo "Memoryport installed to $INSTALL_DIR"
echo "  uc        — CLI"
echo "  uc-mcp    — MCP server"
echo "  uc-proxy  — OpenAI proxy"
echo "  uc-server — Hosted API server"

# Check if in PATH
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "Add to your PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    echo ""
    echo "Or add to your shell profile:"
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
      zsh)  echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc" ;;
      bash) echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc" ;;
      *)    echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.profile" ;;
    esac
    ;;
esac

echo ""
echo "Run 'uc init' to complete setup."
