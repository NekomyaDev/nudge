#!/usr/bin/env bash
# uninstall-desktop.sh — Removes Nudge Linux Desktop Integration
# Part of the Nudge Project (https://github.com/NekomyaDev/nudge)

set -e

if [ "$(id -u)" -eq 0 ]; then
    TARGET_BIN="/usr/local/bin"
    TARGET_APPS="/usr/share/applications"
    TARGET_MIME="/usr/share/mime/packages"
    TARGET_MIME_DIR="/usr/share/mime"
    TARGET_ICONS_APPS="/usr/share/icons/hicolor/scalable/apps"
    TARGET_ICONS_MIME="/usr/share/icons/hicolor/scalable/mimetypes"
else
    TARGET_BIN="$HOME/.local/bin"
    TARGET_APPS="$HOME/.local/share/applications"
    TARGET_MIME="$HOME/.local/share/mime/packages"
    TARGET_MIME_DIR="$HOME/.local/share/mime"
    TARGET_ICONS_APPS="$HOME/.local/share/icons/hicolor/scalable/apps"
    TARGET_ICONS_MIME="$HOME/.local/share/icons/hicolor/scalable/mimetypes"
fi

echo "Nudge masaüstü entegrasyonu kaldırılıyor..."

rm -f "$TARGET_BIN/nudge-runner"
rm -f "$TARGET_BIN/nudge-app-installer"
rm -f "$TARGET_APPS/nudge-app-installer.desktop"
rm -f "$TARGET_MIME/application-x-nudge.xml"
rm -f "$TARGET_ICONS_APPS/nudge.svg"
rm -f "$TARGET_ICONS_MIME/application-x-nudge.svg"

if command -v update-mime-database &>/dev/null; then
    update-mime-database "$TARGET_MIME_DIR" 2>/dev/null || true
fi

if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "$TARGET_APPS" 2>/dev/null || true
fi

echo "✓ Masaüstü entegrasyonu kaldırıldı."
