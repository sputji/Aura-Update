#!/usr/bin/env pwsh
# ─────────────────────────────────────────────────────────
# Aura Update — Build Windows (x64)
# Produit : .exe portable + MSI + NSIS installer
# ─────────────────────────────────────────────────────────

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if (-not $root) { $root = Resolve-Path "$PSScriptRoot\.." }
$project = Join-Path $root "Aura-Update"

Write-Host "╔══════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║   Aura Update — Build Windows x64    ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════╝" -ForegroundColor Cyan

# 1. Vérifier les prérequis
Write-Host "`n[1/4] Vérification des prérequis..." -ForegroundColor Yellow

$rustVersion = & rustc --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERREUR : Rust n'est pas installé. Installez-le depuis https://rustup.rs" -ForegroundColor Red
    exit 1
}
Write-Host "  Rust : $rustVersion" -ForegroundColor Green

$nodeVersion = & node --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERREUR : Node.js n'est pas installé." -ForegroundColor Red
    exit 1
}
Write-Host "  Node : $nodeVersion" -ForegroundColor Green

# 2. Installer les dépendances npm
Write-Host "`n[2/4] Installation des dépendances..." -ForegroundColor Yellow
Push-Location $project
npm install --prefer-offline
if ($LASTEXITCODE -ne 0) { Write-Host "ERREUR npm install" -ForegroundColor Red; Pop-Location; exit 1 }

# 3. Build Tauri (release)
Write-Host "`n[3/4] Compilation Tauri (release)..." -ForegroundColor Yellow
npx tauri build
if ($LASTEXITCODE -ne 0) { Write-Host "ERREUR tauri build" -ForegroundColor Red; Pop-Location; exit 1 }

Pop-Location

# 4. Récapitulatif
$releaseDir = Join-Path $project "src-tauri\target\release"
$bundleDir  = Join-Path $releaseDir "bundle"
$exe  = Join-Path $releaseDir "aura-update.exe"
$msi  = Get-ChildItem "$bundleDir\msi\*.msi" -ErrorAction SilentlyContinue | Select-Object -First 1
$nsis = Get-ChildItem "$bundleDir\nsis\*setup*.exe" -ErrorAction SilentlyContinue | Select-Object -First 1

Write-Host "`n╔══════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║         BUILD RÉUSSI !                ║" -ForegroundColor Green
Write-Host "╚══════════════════════════════════════╝" -ForegroundColor Green
Write-Host ""
Write-Host "  Portable : $exe"
if ($msi)  { Write-Host "  MSI      : $($msi.FullName)" }
if ($nsis) { Write-Host "  NSIS     : $($nsis.FullName)" }
Write-Host ""
Write-Host "Taille exe : $([math]::Round((Get-Item $exe).Length / 1MB, 1)) MB" -ForegroundColor Cyan
