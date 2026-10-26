# Nudge Packaging Specifications

This directory contains package definitions and manifests for various platform package managers:

## 1. Homebrew (macOS & Linux)

The formula in `packaging/homebrew/nudgec.rb` downloads and installs the prebuilt binary for:
- macOS Apple Silicon (`aarch64`)
- macOS Intel (`x86_64`)
- Linux (`x86_64`)

### Usage:
```sh
# Tap and install from source or repo
brew install packaging/homebrew/nudgec.rb
```

## 2. AUR (Arch Linux)

The `nudge-bin` package in `packaging/aur/PKGBUILD` installs the prebuilt binary to `/usr/bin/nudgec` with a `/usr/bin/nudge` symlink.

### Build and Install:
```sh
cd packaging/aur
makepkg -si
```

## 3. Snapcraft (Ubuntu / Linux)

Located in `snap/snapcraft.yaml`.

## 4. Winget (Windows)

Located in `winget/Nekomya.Nudge.*.yaml`.

## 5. Nudge App Runner & Installer (Single-Click .ndg Support)

Distributed in its dedicated repository: [`NekomyaDev/nudge-app-runner`](https://github.com/NekomyaDev/nudge-app-runner).
Provides single-click GUI installation, launcher associations (`application/x-nudge`), and interactive execution windows for Nudge applications on Linux (Windows and macOS coming soon).


