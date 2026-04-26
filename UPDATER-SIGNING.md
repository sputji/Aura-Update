# Signature de l'Updater (Tauri v2)

Ce guide configure la signature nécessaire au système d'auto-update multi-OS.

## 1) Générer la paire de clés

```bash
tauri signer generate -w ~/.tauri/aura.key
```

Cette commande génère:
- clé privée: `~/.tauri/aura.key`
- clé publique: affichée dans la sortie terminal

## 2) Configurer la clé publique dans l'app

Dans `src-tauri/tauri.conf.json`, remplacer:
- `plugins.updater.pubkey = "REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY"`

par la clé publique réelle générée.

## 3) Configurer les secrets GitHub Actions

Ajouter ces secrets dans le dépôt GitHub:
- `TAURI_SIGNING_PRIVATE_KEY`: contenu de `~/.tauri/aura.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: mot de passe utilisé à la génération

## 4) Vérifier la release

Le workflow publie:
- binaires d'installation (`.exe`, `.msi`, `.dmg`, `.deb`, `.rpm`, `.AppImage`)
- signatures (`.sig`)
- métadonnées updater (`latest*.json` + `updater.json`)

L'app récupère ensuite `updater.json` depuis:
- `https://github.com/sputji/Aura-Update/releases/latest/download/updater.json`
