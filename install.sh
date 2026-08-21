#!/bin/bash
# Nudge Installer - https://github.com/NekomyaDev/nudge
# Usage: curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

set -e

REPO="NekomyaDev/nudge"
BINARY="nudgec"
VERSION="v1.2.0"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux*)
        PLATFORM="linux"
        ARCHIVE="tar.gz"
        ;;
    Darwin*)
        PLATFORM="macos"
        ARCHIVE="tar.gz"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        PLATFORM="windows"
        ARCHIVE="zip"
        ;;
    *)
        echo "Error: Unsupported OS: $OS"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)
        ARCH_NAME="x86_64"
        ;;
    aarch64|arm64)
        ARCH_NAME="aarch64"
        ;;
    *)
        echo "Error: Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# Construct download URL
if [ "$PLATFORM" = "windows" ]; then
    FILENAME="${BINARY}-${VERSION}-${PLATFORM}-${ARCH_NAME}.zip"
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${FILENAME}"
else
    if [ "$PLATFORM" = "macos" ] && [ "$ARCH_NAME" = "aarch64" ]; then
        FILENAME="${BINARY}-${VERSION}-macos-aarch64.tar.gz"
    else
        FILENAME="${BINARY}-${VERSION}-${PLATFORM}-${ARCH_NAME}.tar.gz"
    fi
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${FILENAME}"
fi

echo "Installing Nudge ${VERSION}..."
echo "Platform: ${PLATFORM}-${ARCH_NAME}"
echo "Download: ${URL}"

# Create temp directory
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

# Download
echo "Downloading..."
if command -v curl &> /dev/null; then
    curl -fsSL "$URL" -o "$TMPDIR/$FILENAME"
elif command -v wget &> /dev/null; then
    wget -q "$URL" -O "$TMPDIR/$FILENAME"
else
    echo "Error: curl or wget required"
    exit 1
fi

# Extract
echo "Extracting..."
cd "$TMPDIR"
if [ "$ARCHIVE" = "zip" ]; then
    unzip -q "$FILENAME"
else
    tar xzf "$FILENAME"
fi

# Install
echo "Installing to /usr/local/bin..."
if [ -w /usr/local/bin ]; then
    mv "$BINARY" /usr/local/bin/
else
    sudo mv "$BINARY" /usr/local/bin/
fi

# Make executable
chmod +x /usr/local/bin/$BINARY

echo ""
echo "✓ Nudge ${VERSION} installed successfully!"
echo ""
echo "Run 'nudgec --help' to get started."
echo ""
echo "Quick start:"
echo "  nudgec check hello.ndg    # Type check"
echo "  nudgec build hello.ndg    # Compile to Python"
echo "  nudgec test hello.ndg     # Run tests"
echo ""
echo "Documentation: https://github.com/NekomyaDev/nudge"
echo "VS Code Extension: https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"
