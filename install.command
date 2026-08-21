#!/bin/bash
# Nudge Installer for macOS
# Double-click to install

osascript -e 'display dialog "Nudge Installer v1.2.0\n\nThis will install Nudge programming language to /usr/local/bin.\n\nClick OK to continue." with title "Nudge Installer" buttons {"Cancel", "Install"} default button "Install"' 2>/dev/null

if [ $? -ne 0 ]; then
    echo "Installation cancelled."
    exit 0
fi

echo "Installing Nudge..."

# Detect architecture
ARCH="$(uname -m)"
if [ "$ARCH" = "arm64" ]; then
    FILENAME="nudgec-v1.2.0-macos-aarch64.tar.gz"
else
    FILENAME="nudgec-v1.2.0-macos-x86_64.tar.gz"
fi

URL="https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/$FILENAME"

# Download
TMPDIR=$(mktemp -d)
curl -fsSL "$URL" -o "$TMPDIR/$FILENAME"

# Extract
cd "$TMPDIR"
tar xzf "$FILENAME"

# Install
sudo mv nudgec /usr/local/bin/
chmod +x /usr/local/bin/nudgec

# Cleanup
rm -rf "$TMPDIR"

osascript -e 'display dialog "Nudge v1.2.0 installed successfully!\n\nRun \"nudgec --help\" in Terminal to get started." with title "Nudge Installer" buttons {"OK"}'
