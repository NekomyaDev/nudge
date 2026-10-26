#!/bin/bash
# Nudge Installer - https://github.com/NekomyaDev/nudge
# Usage: curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

set -e

REPO="NekomyaDev/nudge"
BINARY="nudgec"

# Parse CLI options (e.g. bash install.sh --version v1.2.1)
VERSION_ARG=""
while [ $# -gt 0 ]; do
    case "$1" in
        --version|-v)
            if [ -n "$2" ] && [ "${2#--}" = "$2" ]; then
                VERSION_ARG="$2"
                shift 2
            else
                echo "Error: --version requires a version string (e.g. v1.2.1 or latest)" >&2
                exit 1
            fi
            ;;
        --version=*)
            VERSION_ARG="${1#*=}"
            shift
            ;;
        --help|-h)
            echo "Nudge Installer"
            echo "Usage: install.sh [--version <ver>]"
            echo ""
            echo "Options:"
            echo "  --version, -v <ver>   Install specific version (e.g. v1.2.1) or 'latest'"
            echo "  --help, -h            Show this help message"
            exit 0
            ;;
        *)
            echo "Warning: unknown argument '$1' ignored" >&2
            shift
            ;;
    esac
done

# Resolve VERSION_OVERRIDE from CLI argument or NUDGE_VERSION environment variable
VERSION_OVERRIDE="${VERSION_ARG:-${NUDGE_VERSION:-}}"

# Resolve 'latest' via GitHub API if requested
if [ "$VERSION_OVERRIDE" = "latest" ]; then
    echo "Resolving latest release tag from GitHub..."
    if command -v curl &> /dev/null; then
        RESOLVED_TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name":' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
    elif command -v wget &> /dev/null; then
        RESOLVED_TAG=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name":' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
    else
        RESOLVED_TAG=""
    fi
    if [ -n "$RESOLVED_TAG" ]; then
        VERSION_OVERRIDE="$RESOLVED_TAG"
        echo "Resolved latest version: ${VERSION_OVERRIDE}"
    else
        echo "Warning: could not resolve latest release tag via GitHub API; falling back to default" >&2
        VERSION_OVERRIDE=""
    fi
fi
# VERSION is set per-platform below: v1.2.1 assets currently ship for linux,
# while macOS/Windows remain on the v1.2.0 release assets.

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux*)
        PLATFORM="linux"
        ARCHIVE="tar.gz"
        VERSION="${VERSION_OVERRIDE:-v1.2.1}"
        ;;
    Darwin*)
        PLATFORM="macos"
        ARCHIVE="tar.gz"
        VERSION="${VERSION_OVERRIDE:-v1.2.0}"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        PLATFORM="windows"
        ARCHIVE="zip"
        VERSION="${VERSION_OVERRIDE:-v1.2.0}"
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
echo "Downloading ${FILENAME}..."
if command -v curl &> /dev/null; then
    curl -fsSL "$URL" -o "$TMPDIR/$FILENAME"
elif command -v wget &> /dev/null; then
    wget -q "$URL" -O "$TMPDIR/$FILENAME"
else
    echo "Error: curl or wget required"
    exit 1
fi

# Checksum Verification (SHA-256)
CHECKSUM_URL="https://github.com/${REPO}/releases/download/${VERSION}/checksums.txt"
CHECKSUM_FILE="$TMPDIR/checksums.txt"
CHECKSUM_FETCHED=false

if command -v curl &> /dev/null; then
    if curl -fsSL "$CHECKSUM_URL" -o "$CHECKSUM_FILE" 2>/dev/null; then
        CHECKSUM_FETCHED=true
    fi
elif command -v wget &> /dev/null; then
    if wget -q "$CHECKSUM_URL" -O "$CHECKSUM_FILE" 2>/dev/null; then
        CHECKSUM_FETCHED=true
    fi
fi

if [ "$CHECKSUM_FETCHED" = true ] && [ -s "$CHECKSUM_FILE" ]; then
    echo "Verifying SHA-256 checksum..."
    EXPECTED_SHA=$(grep "${FILENAME}" "$CHECKSUM_FILE" | awk '{print $1}')
    if [ -n "$EXPECTED_SHA" ]; then
        if command -v sha256sum &> /dev/null; then
            ACTUAL_SHA=$(sha256sum "$TMPDIR/$FILENAME" | awk '{print $1}')
        elif command -v shasum &> /dev/null; then
            ACTUAL_SHA=$(shasum -a 256 "$TMPDIR/$FILENAME" | awk '{print $1}')
        else
            echo "Warning: sha256sum/shasum not found, skipping checksum verification"
            ACTUAL_SHA=""
        fi

        if [ -n "$ACTUAL_SHA" ]; then
            if [ "$EXPECTED_SHA" = "$ACTUAL_SHA" ]; then
                echo "Checksum verified: ${ACTUAL_SHA}"
            else
                echo "Error: SHA-256 checksum verification failed!" >&2
                echo "Expected: ${EXPECTED_SHA}" >&2
                echo "Actual:   ${ACTUAL_SHA}" >&2
                exit 1
            fi
        fi
    else
        echo "Note: '${FILENAME}' not listed in checksums.txt, proceeding"
    fi
elif [ "${NUDGE_REQUIRE_CHECKSUM:-0}" = "1" ]; then
    echo "Error: checksums.txt not available for ${VERSION} and NUDGE_REQUIRE_CHECKSUM=1" >&2
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
    chmod +x /usr/local/bin/$BINARY
else
    sudo mv "$BINARY" /usr/local/bin/
    sudo chmod +x /usr/local/bin/$BINARY
fi

# Optional Linux Desktop Integration
if [ "$PLATFORM" = "linux" ]; then
    SCRIPT_DIR_LOC="$(cd "$(dirname "${BASH_SOURCE[0]}")" 2>/dev/null && pwd || echo "")"
    if [ -n "$SCRIPT_DIR_LOC" ] && [ -x "$SCRIPT_DIR_LOC/packaging/linux-desktop/setup-desktop.sh" ]; then
        echo "Configuring Linux desktop integration (.ndg associations & launcher)..."
        "$SCRIPT_DIR_LOC/packaging/linux-desktop/setup-desktop.sh" || true
    fi
fi

echo ""
echo "Nudge ${VERSION} installed successfully!"
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
