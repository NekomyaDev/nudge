# Nudge Installer for Windows
# Usage: irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$REPO = "NekomyaDev/nudge"
$BINARY = "nudgec"
$VERSION = "v1.2.0"

# Detect architecture
$ARCH = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "x86" }

Write-Host "Installing Nudge $VERSION..." -ForegroundColor Cyan
Write-Host "Platform: windows-$ARCH"

# Download URL
$FILENAME = "$BINARY-$VERSION-windows-$ARCH.zip"
$URL = "https://github.com/$REPO/releases/download/$VERSION/$FILENAME"

Write-Host "Download: $URL"

# Create temp directory
$TMPDIR = New-TemporaryFile | ForEach-Object { Remove-Item $_; New-Item -ItemType Directory -Path $_ }

try {
    # Download
    Write-Host "Downloading..."
    Invoke-WebRequest -Uri $URL -OutFile "$TMPDIR\$FILENAME" -UseBasicParsing

    # Extract
    Write-Host "Extracting..."
    Expand-Archive -Path "$TMPDIR\$FILENAME" -DestinationPath $TMPDIR -Force

    # Install directory
    $INSTALL_DIR = "$env:LOCALAPPDATA\Nudge"
    if (-not (Test-Path $INSTALL_DIR)) {
        New-Item -ItemType Directory -Path $INSTALL_DIR | Out-Null
    }

    # Copy binary
    Write-Host "Installing to $INSTALL_DIR..."
    Copy-Item "$TMPDIR\$BINARY.exe" "$INSTALL_DIR\$BINARY.exe" -Force

    # Add to PATH if not already there
    $PATH = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($PATH -notlike "*$INSTALL_DIR*") {
        [Environment]::SetEnvironmentVariable("Path", "$PATH;$INSTALL_DIR", "User")
        $env:Path = "$env:Path;$INSTALL_DIR"
        Write-Host "Added $INSTALL_DIR to PATH" -ForegroundColor Yellow
    }

    Write-Host ""
    Write-Host "Nudge $VERSION installed successfully!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Run 'nudgec --help' to get started."
    Write-Host ""
    Write-Host "Quick start:"
    Write-Host "  nudgec check hello.ndg    # Type check"
    Write-Host "  nudgec build hello.ndg    # Compile to Python"
    Write-Host "  nudgec test hello.ndg     # Run tests"
    Write-Host ""
    Write-Host "Documentation: https://github.com/NekomyaDev/nudge"
    Write-Host "VS Code Extension: https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"
} finally {
    Remove-Item -Recurse -Force $TMPDIR
}
