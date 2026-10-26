#!/usr/bin/env bash
# setup-desktop.sh — Installs Nudge Linux Desktop Integration (.ndg single-click support)
# Part of the Nudge Project (https://github.com/NekomyaDev/nudge)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOGO_FILE="$SCRIPT_DIR/../../assets/logo.svg"

# Detect whether running as root or user
if [ "$(id -u)" -eq 0 ]; then
    TARGET_BIN="/usr/local/bin"
    TARGET_APPS="/usr/share/applications"
    TARGET_MIME="/usr/share/mime/packages"
    TARGET_MIME_DIR="/usr/share/mime"
    TARGET_ICONS_APPS="/usr/share/icons/hicolor/scalable/apps"
    TARGET_ICONS_MIME="/usr/share/icons/hicolor/scalable/mimetypes"
    IS_SYSTEM=1
    echo "Sistem geneli kurulum yapılıyor (/usr/share, /usr/local/bin)..."
else
    TARGET_BIN="$HOME/.local/bin"
    TARGET_APPS="$HOME/.local/share/applications"
    TARGET_MIME="$HOME/.local/share/mime/packages"
    TARGET_MIME_DIR="$HOME/.local/share/mime"
    TARGET_ICONS_APPS="$HOME/.local/share/icons/hicolor/scalable/apps"
    TARGET_ICONS_MIME="$HOME/.local/share/icons/hicolor/scalable/mimetypes"
    IS_SYSTEM=0
    echo "Kullanıcı alanı kurulumu yapılıyor ($HOME/.local)..."
fi

# 1. Create target directories
mkdir -p "$TARGET_BIN"
mkdir -p "$TARGET_APPS"
mkdir -p "$TARGET_MIME"
mkdir -p "$TARGET_ICONS_APPS"
mkdir -p "$TARGET_ICONS_MIME"

# 2. Install binaries
echo "→ Komut dosyaları yükleniyor..."
cp -f "$SCRIPT_DIR/nudge-runner" "$TARGET_BIN/nudge-runner"
chmod +x "$TARGET_BIN/nudge-runner"

cp -f "$SCRIPT_DIR/nudge-app-installer" "$TARGET_BIN/nudge-app-installer"
chmod +x "$TARGET_BIN/nudge-app-installer"

# 3. Install MIME definition
echo "→ application/x-nudge MIME türü kaydediliyor..."
cp -f "$SCRIPT_DIR/application-x-nudge.xml" "$TARGET_MIME/application-x-nudge.xml"

# 4. Install Icons
echo "→ Uygulama ve dosya simgeleri yükleniyor..."
if [ -f "$LOGO_FILE" ]; then
    cp -f "$LOGO_FILE" "$TARGET_ICONS_APPS/nudge.svg"
    cp -f "$LOGO_FILE" "$TARGET_ICONS_MIME/application-x-nudge.svg"
else
    # Fallback to creating SVG if logo not found
    cat << 'EOF' > "$TARGET_ICONS_APPS/nudge.svg"
<svg width="512" height="512" viewBox="0 0 512 512" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#4F46E5"/><stop offset="1" stop-color="#7C3AED"/></linearGradient>
    <linearGradient id="spark" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#FDE68A"/><stop offset="1" stop-color="#F59E0B"/></linearGradient>
  </defs>
  <rect width="512" height="512" rx="112" fill="url(#bg)"/>
  <path d="M148 382 V130 H222 L290 236 V130 H364 V382 H290 L222 276 V382 Z" fill="#FFFFFF"/>
  <circle cx="396" cy="118" r="24" fill="url(#spark)"/>
</svg>
EOF
    cp -f "$TARGET_ICONS_APPS/nudge.svg" "$TARGET_ICONS_MIME/application-x-nudge.svg"
fi

# 5. Install Desktop entry
echo "→ Masaüstü başlatıcısı (nudge-app-installer.desktop) yükleniyor..."
sed "s|Exec=nudge-app-installer|Exec=$TARGET_BIN/nudge-app-installer|g" \
    "$SCRIPT_DIR/nudge-app-installer.desktop" > "$TARGET_APPS/nudge-app-installer.desktop"

# 6. Update databases and MIME associations
echo "→ Sistem veritabanları güncelleniyor..."
if command -v update-mime-database &>/dev/null; then
    update-mime-database "$TARGET_MIME_DIR" 2>/dev/null || true
fi

if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "$TARGET_APPS" 2>/dev/null || true
fi

if command -v xdg-mime &>/dev/null; then
    xdg-mime default nudge-app-installer.desktop application/x-nudge 2>/dev/null || true
    xdg-mime default nudge-app-installer.desktop text/x-nudge 2>/dev/null || true
fi

echo ""
echo -e "\033[1;32m✓ Nudge Linux Masaüstü Entegrasyonu Tamamlandı!\033[0m"
echo "  • .ndg dosyalarına dosya yöneticisinden (Dolphin, Nautilus vs.) çift tıklayarak tek tıkla kurabilir veya çalıştırabilirsiniz."
echo "  • Nudge Uygulama Yükleyicisi Başlat Menüsü'ne eklendi."
echo ""
