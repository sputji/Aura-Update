# Aura-Update

**Gestionnaire de mises à jour universel, simple et performant.**

Aura-Update est un utilitaire léger conçu pour maintenir vos applications à jour automatiquement sur Windows et Linux. Il simplifie la gestion des paquets en détectant et installant les mises à jour disponibles en un seul clic.

🌐 **Site Officiel :** [https://www.auraneo.fr/aura-update/](https://www.auraneo.fr/aura-update/)

## 🚀 Fonctionnalités

- **Multi-plateforme :** Fonctionne nativement sur Windows 10/11 et les distributions Linux majeures (Debian, Ubuntu, Fedora, Arch...).
- **Détection Automatique :** Identifie automatiquement le gestionnaire de paquets de votre système (`winget`, `apt`, `dnf`, `pacman`...).
- **Interface Moderne :** Une interface graphique claire et intuitive (Thème sombre).
- **Mode Administrateur :** Gestion intelligente des privilèges pour les installations nécessitant des droits élevés.
- **Mises à jour par lot :** Mettez à jour tout votre système en un clic.
- **Léger & Rapide :** Développé avec Electron et Node.js pour une performance optimale.

## 🛠️ Installation

Téléchargez la dernière version depuis notre site officiel :
[https://www.auraneo.fr/aura-update/](https://www.auraneo.fr/aura-update/)

### Prérequis
- **Windows :** Windows 10 (1809+) ou Windows 11. (Winget est généralement préinstallé).
- **Linux :** Une distribution récente avec `apt`, `dnf` ou `pacman`.

## 🏗️ Compilation

Si vous souhaitez compiler le projet vous-même à partir du code source :

```bash
# Installer les dépendances
npm install

# Compiler pour Windows (.exe)
npm run dist

# Compiler pour Linux (.AppImage, .deb)
npm run dist:linux
```

Les exécutables seront générés dans le dossier `dist/`.

## ⚖️ Licence et Propriété

Ce logiciel est la propriété exclusive de la société **Aura Néo**.

- **Gratuit :** Vous pouvez utiliser ce logiciel gratuitement.
- **Propriétaire :** Le code source est fermé. Il est strictement interdit de le copier, le modifier, le décompiler ou le pirater.
- **Distribution :** Le logiciel ne doit être téléchargé que depuis le site officiel.

Ce logiciel est soumis au droit français. Pour plus de détails, consultez le fichier `LICENSE_FR.txt` inclus.

---
© 2025 Aura Néo. Tous droits réservés.