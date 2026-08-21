#!/bin/bash
# Build and publish Nudge to Snap Store
# Usage: ./snap-publish.sh

set -e

echo "=== Nudge Snap Publisher ==="
echo ""

# Check if snapcraft is installed
if ! command -v snapcraft &> /dev/null; then
    echo "Error: snapcraft is not installed"
    echo ""
    echo "Install it with:"
    echo "  sudo snap install snapcraft --classic"
    echo ""
    exit 1
fi

# Check if logged in
if ! snapcraft whoami &> /dev/null; then
    echo "Please login to Snapcraft first:"
    echo "  snapcraft login"
    echo ""
    exit 1
fi

echo "Building snap..."
snapcraft

echo ""
echo "Uploading to Snap Store..."
snapcraft upload nudge_1.2.0_amd64.snap

echo ""
echo "Publishing to stable channel..."
snapcraft release nudge stable

echo ""
echo "=== Done! ==="
echo ""
echo "Users can now install with:"
echo "  sudo snap install nudge --classic"
echo ""
echo "Verify at: https://snapcraft.io/nudge"
