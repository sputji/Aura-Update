# Changelog

## [v0.3.2] - 2025-12-09

### 🚀 Nouveautés Critiques
- **Windows Update (Natif) :** Le logiciel interroge désormais directement le service Windows Update (via COM/PowerShell) en plus de Winget. Cela permet de détecter et d'installer les mises à jour du système d'exploitation, les correctifs de sécurité et les patchs cumulatifs.
  - Recherche parallèle ultra-rapide (Winget + Windows Update simultanément).
  - Installation via le déclencheur natif `USOClient` pour une fiabilité maximale.

## [v0.3.1] - 2025-11-28

### 🚀 Nouveautés & Améliorations
- **Portabilité Totale :** L'application est désormais 100% portable. Elle détecte automatiquement l'environnement et ne dépend plus d'aucune structure de dossier fixe.
- **Menu d'Application Complet :** Intégration d'un menu natif avec accès rapide :
  - **Outils :** Nettoyage des fichiers temporaires système et nettoyage des caches/logs de l'application.
  - **Liens :** Accès direct au site Aura Néo et à la page du projet.
  - **Logs :** Ouverture facile du dossier de logs pour le support.
- **Outils de Maintenance :** 
  - Ajout d'une fonctionnalité pour supprimer les fichiers temporaires de Windows (`%TEMP%`) et Linux (`/tmp`) de manière sécurisée.
  - Option pour vider les logs et le cache de l'application.

### 🐛 Corrections de Bugs
- **Détection des Pilotes (Intel/Nvidia) :** Correction majeure du moteur de détection Winget qui ignorait certaines mises à jour de pilotes (ex: *Intel® Wireless Bluetooth®*) car elles n'avaient pas de version standard.
- **Parsing Universel :** Réécriture complète du moteur d'analyse des mises à jour pour supporter toutes les langues système et tous les encodages (UTF-8 forcé).
- **Correction "0 Mise à jour" :** Résolution du bug où l'application ne trouvait rien sur les ordinateurs d'autres utilisateurs à cause de différences de formatage dans la console Windows.

## [v0.3.0] - 2025-11-07

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
