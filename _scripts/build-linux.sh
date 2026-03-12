#!/bin/bash
# ─────────────────────────────────────────────────────────
# Aura Update — Build Linux (x86_64)
# Produit : .deb + .rpm + AppImage
# ─────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "╔══════════════════════════════════════╗"
echo "║   Aura Update — Build Linux x64      ║"
echo "╚══════════════════════════════════════╝"

# 1. Prérequis
echo ""
echo "[1/4] Vérification des prérequis..."

if ! command -v rustc &> /dev/null; then
    echo "ERREUR : Rust n'est pas installé. Installez-le avec : curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi
echo "  Rust : $(rustc --version)"

if ! command -v node &> /dev/null; then
    echo "ERREUR : Node.js n'est pas installé."
    exit 1
fi
echo "  Node : $(node --version)"

# Dépendances système (Debian/Ubuntu)
if command -v apt-get &> /dev/null; then
    echo ""
    echo "  Vérification des paquets système..."
    PKGS="libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev"
    MISSING=""
    for pkg in $PKGS; do
        if ! dpkg -s "$pkg" &>/dev/null; then
            MISSING="$MISSING $pkg"
        fi
    done
    if [ -n "$MISSING" ]; then
        echo "  Installation des paquets manquants :$MISSING"
        sudo apt-get update && sudo apt-get install -y $MISSING
    fi
fi

# Dépendances système (Fedora/RHEL)
if command -v dnf &> /dev/null; then
    echo ""
    echo "  Vérification des paquets système..."
    PKGS="webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel openssl-devel"
    for pkg in $PKGS; do
        rpm -q "$pkg" &>/dev/null || sudo dnf install -y "$pkg"
    done
fi

# 2. Dépendances npm
echo ""
echo "[2/4] Installation des dépendances..."
cd "$PROJECT_DIR"
npm install --prefer-offline

# 3. Build
echo ""
echo "[3/4] Compilation Tauri (release)..."
npx tauri build

# 4. Résultat
echo ""
echo "╔══════════════════════════════════════╗"
echo "║         BUILD RÉUSSI !                ║"
echo "╚══════════════════════════════════════╝"

BUNDLE_DIR="$PROJECT_DIR/src-tauri/target/release/bundle"
echo ""
echo "  .deb     : $(ls "$BUNDLE_DIR/deb/"*.deb 2>/dev/null || echo 'N/A')"
echo "  .rpm     : $(ls "$BUNDLE_DIR/rpm/"*.rpm 2>/dev/null || echo 'N/A')"
echo "  AppImage : $(ls "$BUNDLE_DIR/appimage/"*.AppImage 2>/dev/null || echo 'N/A')"
echo ""
