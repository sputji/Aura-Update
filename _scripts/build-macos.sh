#!/bin/bash
# ─────────────────────────────────────────────────────────
# Aura Update — Build macOS (Universal: x86_64 + arm64)
# Produit : .app bundle + .dmg
# ─────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "╔══════════════════════════════════════╗"
echo "║   Aura Update — Build macOS          ║"
echo "╚══════════════════════════════════════╝"

# 1. Prérequis
echo ""
echo "[1/5] Vérification des prérequis..."

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

# 2. Targets universelles
echo ""
echo "[2/5] Ajout des targets macOS..."
rustup target add aarch64-apple-darwin 2>/dev/null || true
rustup target add x86_64-apple-darwin 2>/dev/null || true

# 3. Dépendances npm
echo ""
echo "[3/5] Installation des dépendances..."
cd "$PROJECT_DIR"
npm install --prefer-offline

# 4. Build
echo ""
echo "[4/5] Compilation Tauri (release universal)..."
npx tauri build --target universal-apple-darwin

# 5. Résultat
echo ""
echo "╔══════════════════════════════════════╗"
echo "║         BUILD RÉUSSI !                ║"
echo "╚══════════════════════════════════════╝"

BUNDLE_DIR="$PROJECT_DIR/src-tauri/target/universal-apple-darwin/release/bundle"
echo ""
echo "  .app : $(ls "$BUNDLE_DIR/macos/"*.app 2>/dev/null || echo 'N/A')"
echo "  .dmg : $(ls "$BUNDLE_DIR/dmg/"*.dmg 2>/dev/null || echo 'N/A')"
echo ""
du -sh "$BUNDLE_DIR/macos/"*.app 2>/dev/null || true
