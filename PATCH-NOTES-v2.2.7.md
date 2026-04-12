# Aura-Update v2.2.7

Date: 2026-04-13

## Correctifs majeurs

- Correction de la fonction "Selectionner les bloatwares":
  - Suppression de l'etat ambigu "Analyse en cours..." quand la selection est deja chargee.
  - Ajout d'un statut clair apres scan: selection prete / purge annulee.
  - Texte clarifie pour eviter toute confusion sur les applications critiques.
  - Nouvelle action de reactivation: possibilite de restaurer les applications precedemment desinstallees (best-effort).
  - Ajout d'un cache de scan pour eviter les rescans inutiles et fluidifier l'affichage.

- Correction du popup "Fichiers temporaires volumineux":
  - Le bouton "Nettoyer maintenant" lance maintenant un vrai nettoyage immediat.
  - Enchainement complet: scan -> nettoyage -> retour visuel + toast -> refresh sante.
  - Gestion du cas sans fichiers a nettoyer.

- Nettoyage plus fiable et comprehensible:
  - Ajout d'une selection explicite par case a cocher dans l'onglet Nettoyage.
  - Ajout des actions "Tout selectionner" / "Tout deselectionner".
  - Affichage dynamique du volume et du nombre d'elements selectionnes.
  - Le bouton nettoyage respecte la selection utilisateur.

- Nettoyage backend renforce (Windows/macOS/Linux):
  - Meilleure suppression des fichiers/dossiers read-only.
  - Strategie best-effort recursive pour augmenter le taux de nettoyage effectif.
  - Compatibilite multi-OS conservee.

## Securite et fiabilite Windows

- Protection de la fiabilite Windows sur la purge bloatware:
  - Les composants Xbox/Game Bar critiques ont ete exclus de la purge pour eviter les regressions (ex: Win+G).

## Mise a jour de version

- Version application passee a 2.2.7 dans:
  - package.json
  - src-tauri/Cargo.toml
  - src-tauri/tauri.conf.json

## Notes de validation

- Correctifs verifies sur les chemins frontend + backend concernes.
- Aucun changement de plateforme specifique n'a retire la compatibilite macOS/Linux/Windows.