# 🔄 Aura Update — Health Center

> Centre de santé système complet pour Windows, macOS et Linux.
> **v2.2.7** — Dernière version stable

<p align="center">
  <img src="frontend/icons/icon.png" alt="Aura Update" width="128" />
</p>

---

## 🎯 Présentation

**Aura Update Health Center** est un utilitaire gratuit et multiplateforme qui maintient votre ordinateur en pleine forme. Bien plus qu'un simple gestionnaire de mises à jour, c'est un véritable **centre de santé système** qui analyse, nettoie, optimise et protège votre machine.

Ultra-léger (~8-12 MB), il offre des performances natives sur chaque plateforme grâce à son architecture Rust + WebView natif.

---

## ✨ Fonctionnalités

| Fonctionnalité | Description |
|----------------|-------------|
| 📊 **Score de Santé** | Score sur 100 évaluant l'état de votre système en un coup d'œil |
| 🌡️ **Monitoring** | Température CPU/GPU et batterie en temps réel |
| 🔄 **Mises à Jour** | Détection et installation via gestionnaires natifs (winget, apt, brew…) |
| 🧹 **Nettoyage** | Fichiers temporaires, caches navigateurs, résidus OS |
| 🚀 **Démarrage** | Gestion des programmes au démarrage |
| ⚡ **Processus** | Surveillance et arrêt des applications gourmandes |
| 🎮 **Mode Turbo** | Libère 100% des ressources CPU/RAM pour le jeu ou le travail |
| ❄️ **Cool Boost** | Ventilateurs au maximum avec détection automatique du fabricant |
| 🚗 **Autopilot** | Maintenance complète en un clic |
| 🗑️ **Bloatwares** | Suppression des applications pré-installées indésirables |
| 🔒 **Télémétrie** | Désactivation granulaire de la télémétrie système |
| 📸 **Snapshots** | Points de restauration avant chaque opération majeure |
| 🤖 **Analyse IA** | Assistant IA optionnel (opt-in, compatible IA locale) |
| 📅 **Planification** | Maintenance automatique programmée |
| 🌐 **Contrôle Distant** | Dashboard accessible depuis smartphone via QR code |
| 📂 **Backup** | Dossier de sauvegarde personnalisable |
| 🎓 **Tutoriel** | Guide interactif au premier lancement |
| 🌍 **Bilingue** | Interface FR/EN avec changement instantané |
| ⬆️ **Auto-Update App** | Mise à jour automatique multi-OS intégrée |

---

## 💻 Plateformes

| Plateforme | Formats |
|------------|---------|
| Windows 10/11 | Installateur NSIS (FR/EN) + MSI |
| macOS 10.15+ | .app + .dmg |
| Linux (Debian/Ubuntu) | .deb + AppImage |
| Linux (Fedora/RHEL) | .rpm + AppImage |
| Linux (Arch) | AppImage |

---

## 📥 Téléchargement

**Site officiel** : [auraneo.fr/aura-update](https://auraneo.fr/aura-update)

> ⚠️ **SmartScreen** : L'exécutable n'étant pas encore signé avec un certificat EV,
> Windows SmartScreen peut afficher un avertissement au premier lancement.
> Cliquez sur « Plus d'infos » puis « Exécuter quand même ».

---

## ⬆️ Auto-Update Multi-OS (v2.2.7)

- Vérification silencieuse au démarrage (désactivable dans Paramètres)
- Vérification manuelle depuis le tray via “Vérifier les mises à jour d'Aura”
- Item tray dynamique lorsqu'une nouvelle version est détectée
- Modal premium de mise à jour avec:
  - version actuelle → version cible
  - release notes
  - barre de progression téléchargement/installation
  - verrouillage de fermeture pendant installation
- Redémarrage automatique de l'application après installation

---

## 🛡️ Sécurité et Vie Privée

- ✅ **100% local** — Aucune donnée envoyée à des serveurs externes
- ✅ **Mode portable** — Toute la configuration reste à côté de l'application
- ✅ **IA optionnelle** — Consentement explicite requis, compatible avec les IA locales
- ✅ **Gestionnaires natifs** — Utilise winget/apt/brew, pas de téléchargements tiers
- ✅ **Code protégé** — Binaire Rust optimisé + frontend obfusqué

---

## 🎨 Interface

- **Glassmorphism** : Cartes translucides avec blur et saturation
- **Thème Dark/Light** : Changement automatique ou manuel
- **Particules animées** : Couleurs interpolées par onglet
- **Arc de santé SVG** : Animation avec glow dynamique
- **Turbo Cockpit** : Design « cockpit » dédié pour le mode Turbo

---

## ❓ FAQ

**L'application est-elle gratuite ?**
Oui, Aura Update Health Center est gratuit.

**Mes données sont-elles envoyées quelque part ?**
Non. Tout fonctionne en local. La seule exception est l'analyse IA (optionnelle, consentement explicite requis).

**Puis-je utiliser une IA locale ?**
Oui. L'app est compatible avec toute API au format OpenAI, y compris les instances locales (vLLM, Ollama…).

**Pourquoi SmartScreen affiche un avertissement ?**
L'exécutable n'a pas encore un certificat de signature reconnu. Cliquez sur « Plus d'infos » → « Exécuter quand même ».

---

## 📄 Licence

Licence propriétaire Aura Néo.
Voir [LICENSE_FR.txt](LICENSE_FR.txt) (CLUF) et [LICENSE_EN.txt](LICENSE_EN.txt) (EULA).

Le logiciel est distribué gratuitement depuis le site officiel [auraneo.fr](https://auraneo.fr).
Toute redistribution non autorisée est interdite.

---

## 📬 Contact

- **Site web** : [auraneo.fr](https://auraneo.fr)
- **Email** : contact@auraneo.fr

---

## 📋 Changelog v2.2.7

- ⬆️ **Auto-Update applicatif multi-OS** intégré dans l'application
- 🧭 **Tray dynamique** avec action de vérification et indication “nouvelle version”
- 🪟 **Modal de mise à jour premium** (notes de version, progression, verrouillage de fermeture)
- ⚙️ **Nouveau réglage utilisateur** pour activer/désactiver la vérification au démarrage
- 🧹 **Sécurité cleanup Windows**: suppression Xbox/Game Bar retirée de la purge bloatware
- ♻️ **Réactivation bloatwares**: ajout de la restauration de sélection

---

## 📋 Changelog v2.2.6

- 🐛 **Fix critique : détection Windows Update** — Les mises à jour Windows (KB cumulative, sécurité) n'étaient plus détectées. Cause racine : `Start-Job` + `Wait-Job -Timeout 40` tuait la recherche COM avant qu'elle ne se termine (une recherche WU réelle prend 60-90s). Correction : exécution directe du COM sans Start-Job, timeout Rust augmenté à 120s avec `kill_on_drop` comme filet de sécurité.
- 🐛 **Fix logique reboot_pending** — Quand un redémarrage était en attente, les nouvelles mises à jour disponibles étaient masquées. Les deux (notification reboot + mises à jour) sont maintenant affichées.
- ⏱️ **Initialisation services WU** — Délai d'attente après démarrage de wuauserv/BITS passé de 500ms à 3s pour laisser le service s'initialiser complètement.
- ⏱️ **Timeouts frontend augmentés** — Splash et scan manuel passés à 130s pour correspondre au timeout Rust (120s + marge).

---

## 📋 Changelog v2.2.5

- 🤖 **Sélection dynamique des modèles IA** — Le champ texte est remplacé par un menu déroulant qui charge automatiquement les vrais modèles disponibles depuis l'API du fournisseur (OpenAI, Gemini, Grok, Ollama, Aura-IA). Bouton 🔄 pour rafraîchir.
- 🔒 **Protection Vie Privée** — Refonte complète du contrôle de télémétrie en 5 catégories : Windows (DiagTrack, WerSvc), Office, NVIDIA (NvTelemetryContainer, tâches planifiées), Navigateurs (Edge, Chrome, Firefox), Pistage publicitaire (Advertising ID, Activity History, Localisation, Cortana, Inking & Typing).
- 🚀 **Turbo Mode Ultra** — Arrêt de 7 services Windows, plan d'alimentation Ultimate Performance, CPU 100%, GPU max perf NVIDIA, effets visuels désactivés, résolution timer 1ms, optimisation réseau Nagle.
- ❄️ **Cool Boost GPU** — Support NVIDIA via nvidia-smi : persistence mode, power limit, clocks max, contrôle ventilateurs, registre PowerMizer.
- 🤖 **Grok v4** — Migration vers grok-4-1-fast-non-reasoning (nouveau modèle xAI disponible).
- 🔗 **Aura-IA fiable** — Tolérance TLS pour auraneo.fr, retry automatique sur erreur de connexion/timeout, fallback 3 modes (Rapide/Réflexions/Intelligent) si le serveur ne répond pas.
- ✅ **Activation IA en 1 clic** — Correction du bug qui exigeait 2 clics pour activer l'IA la première fois. Le consentement active maintenant directement le toggle.
- 🎓 **Tutorial premier lancement** — Les zones montrées par le tutoriel sont maintenant clairement visibles. Le contenu réel de l'élément est affiché, le reste est assombri.
- 💾 **Persistance des réglages** — Toutes les préférences (Protection Vie Privée, options IA, etc.) sont correctement sauvegardées et restaurées au redémarrage.
- 🎮 **Détection GPU réelle** — Filtrage des adaptateurs virtuels (Parsec, Remote Desktop, RDP) pour afficher le vrai GPU matériel.
- 🌍 **i18n complet** — Traductions FR/EN mises à jour pour toutes les nouvelles fonctionnalités.

---

## 📋 Changelog v2.2.4

- 🎮 **Détection GPU réelle** — Filtrage des adaptateurs virtuels (Parsec, Remote Desktop, RDP, Microsoft Basic Display) pour afficher le vrai GPU matériel avec fallback automatique.
- 🔧 **Fix TypeError results.reduce** — Gestion robuste du retour `CleanupReport` : double vérification `Array.isArray` et extraction défensive `report.items`, plus protection null-safe des checkboxes.
- 🎨 **Refonte UI Caches Navigateurs** — Nouvelle interface avec icônes colorées par navigateur (Chrome 🟡, Edge 🔵, Firefox 🟠, Brave 🟤, Opera 🔴), badge de résumé animé, liste scrollable stylisée, états vides élégants.
- ⚙️ **Event listeners Turbo** — Correction de l'organisation du code : les boutons Scanner/Nettoyer navigateurs sont correctement rattachés à l'onglet Turbo avec vérification défensive `.

---

## 📋 Changelog v2.2.3

- 🤖 **IA universelle** — Endpoint, modèle et clé API configurables. Compatible Ollama, Gemini, OpenAI et toute API OpenAI-compatible. Détection automatique des serveurs locaux (localhost) avec désactivation SSL et timeout étendu.
- 🎯 **Cercle de santé SVG** — Correction du contraste texte (visible en thème sombre), transition fluide `stroke-dashoffset`, rafraîchissement automatique toutes les 10 secondes.
- 🌐 **Nettoyage navigateurs → Turbo** — La section granulaire (Chrome, Edge, Firefox, Brave, Opera, Opera GX) est désormais dans l'onglet Turbo. Chemins de cache Chromium corrigés (`Cache_Data`). Les processus navigateurs sont fermés automatiquement avant le nettoyage pour déverrouiller les fichiers.

---

## 📋 Changelog v2.2.2

- 🎯 **Cercle de santé SVG** — Score animé avec couleurs dynamiques (rouge/jaune/vert) et glow
- ❄️ **Cool Boost étendu** — Compatibilité 12 fabricants (Acer, Corsair, Razer, Samsung, Huawei, Toshiba, Gigabyte…)
- 📅 **Nettoyage automatique planifié** — Quotidien, hebdomadaire ou mensuel
- 🌐 **Nettoyage navigateurs granulaire** — Cache, historique, cookies, sessions par navigateur (Chrome, Edge, Firefox, Brave, Opera, Opera GX)
- 🗑️ **Désinstalleur bloatwares multi-OS** — Windows (AppxPackage), macOS (Applications), Linux (apt purge)
- 📂 **Dossier de sauvegarde personnalisable** — Backup local avec métadonnées et restauration
- 💾 **Points de restauration automatiques** — Liés au dossier de backup choisi
- ⚠️ **Alerte fichiers temporaires** — Popup si les fichiers temporaires dépassent 1 Go

---

**Développé avec ❤️ par [Aura Néo](https://auraneo.fr)**
