# 🔄 Aura Update — Health Center

> Centre de santé système complet pour Windows, macOS et Linux.
> **v2.2.2** — Dernière version stable

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
