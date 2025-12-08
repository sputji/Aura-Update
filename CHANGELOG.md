# Changelog

## [v0.3.0] - 2025-12-07

### 🚀 Nouveautés Majeures
- **Refonte Complète du Backend :** Abandon de PowerShell au profit d'une architecture native Node.js pour une meilleure stabilité et performance.
- **Support Multi-plateforme Amélioré :** Compatibilité native Windows (Winget) et Linux (APT, DNF, Pacman).
- **Mode Administrateur Fiable :** Nouveau mécanisme d'élévation de privilèges via VBScript (Windows) pour éviter les crashs lors du redémarrage.
- **Interface Utilisateur Moderne :** Nouvelle UI "Dark Dimmed" inspirée de GitHub, plus ergonomique et réactive.
- **Internationalisation (i18n) :** Support complet Français 🇫🇷 et Anglais 🇺🇸 avec changement à la volée.

### ✨ Améliorations
- **Détection Intelligente des Mises à jour :**
  - Nouveau parser Winget robuste capable de lire les colonnes dynamiquement, corrigeant le bug des "0 mises à jour" ou des ID tronqués.
  - Mise à jour automatique des sources Winget pour éviter les erreurs de "Hash Mismatch".
- **Installation Résiliente :** Ajout de l'option `--force` pour débloquer les installations récalcitrantes.
- **Barre de Progression :** 
  - Animation indéterminée pour les mises à jour unitaires.
  - Progression en pourcentage réel pour les mises à jour globales.

### 🔒 Sécurité & Légal
- **Protection du Code :** Packaging ASAR activé par défaut pour masquer le code source.
- **Licence :** Passage en licence Propriétaire (Aura Néo). Interdiction de copie/modification.
- **Logs :** Nouveau système de logs centralisé dans `logs/app.log` pour faciliter le débogage sans exposer d'informations sensibles.

### 🐛 Corrections de Bugs (vs v0.2.14)
- Correction du crash lors du passage en mode Administrateur.
- Correction du bug où 13 mises à jour étaient détectées mais ne s'installaient jamais.
- Correction des erreurs de chemins relatifs/absolus lors du lancement.
- Suppression de la dépendance à un serveur HTTP local (port 8080) qui causait des conflits pare-feu.
