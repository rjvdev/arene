# L'Arène des générations

**Un jeu évolutionnaire entre agents IA locaux. Des agents Qwen (via Ollama) s'affrontent génération après génération ; les meilleurs survivent et se reproduisent, les autres meurent et laissent un mémorial.**

[![CI](https://github.com/rjvdev/arene/actions/workflows/ci.yml/badge.svg)](https://github.com/rjvdev/arene/actions/workflows/ci.yml)
[![Licence : MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Statut : archivé](https://img.shields.io/badge/status-archived-lightgrey.svg)](#statut-du-projet)

[English version](README.md)

## L'idée

La seule pression de sélection peut-elle apprendre à de petits modèles de langage quand répondre, quand abandonner et quand repérer une question piège ? L'Arène tente l'expérience.

- **Questions** vérifiables automatiquement : calculs exacts de difficulté réglée par un calibreur (cible : 3 à 5 réussites par génération) + une part de pièges impossibles. Une question peut aussi être imposée au lancement.
- **Choix de chaque agent** : COMBAT, ABANDON ou REFUS, avec une confiance déclarée.
- **Barème** : combat juste = survie (départage par la confiance) ; refus d'un piège = survie ; abandon = mort honnête ; faux avec forte confiance = pire que l'abandon.
- **Génome** : modèle, goût du risque, 2-3 principes injectés dans le prompt, température, gène « outils ».
- **Reproduction** : croisement et mutation donnent la génération suivante ; les morts laissent un mémorial que les survivants peuvent lire.

Tout se pilote et se modifie depuis une interface web locale. Le programme est écrit en Rust (axum, tokio, minijinja).

## Démarrer

Prérequis : [Rust](https://rustup.rs/), git, et pour le mode réel [Ollama](https://ollama.com/) avec le modèle configuré.

```sh
git clone https://github.com/rjvdev/arene && cd arene
ollama pull qwen3:4b             # seulement pour le mode réel ; le mode simulé se passe d'IA
cargo build --release            # compile le serveur (arene) et le moteur (evolution)
./target/release/arene           # puis ouvrir http://127.0.0.1:8080
```

Un binaire Linux précompilé est joint à la [release v1.0](https://github.com/rjvdev/arene/releases/tag/v1.0).

Depuis ta machine, la connexion est automatique (réglable dans Paramètres). Sinon, la clé d'arbitre est écrite dans `donnees/cle-admin.txt` au premier démarrage.

## L'interface

| Page | Rôle |
|---|---|
| **Tableau de bord** (`/`) | Lancer / arrêter une évolution, suivre le run en direct (progression, question en cours), indicateurs, graphiques, arbre généalogique, détail par génération, mémorial |
| **Historique** (`/runs`) | Tous les runs en heure locale, notes, suppression, comparaison de deux runs (y compris les paramètres qui ont changé) |
| **Paramètres** (`/parametres`) | Formulaire de tous les réglages du moteur, vérifiés avant enregistrement |
| **Fichiers** (`/fichiers`) | Mode expert : édition brute de la config, des gabarits des pages et des résultats des runs |

## Ce que l'arbitre peut muter

Tout est dans **`agents-locaux/evolution.toml`** (formulaire dans Paramètres) : population, difficulté, pièges, principes, modèles, taux de mutation, réglages Ollama, prompt système, adresse du serveur. Les gabarits des pages (`contenu/*.html`, `contenu/graphiques.js`) et les résultats (`donnees/evolution/*.json`) sont aussi modifiables. Chaque modification est versionnée dans Git, et chaque run garde une copie des paramètres avec lesquels il a tourné.

## Outils des agents (facultatif)

À activer dans Paramètres → « Outils des agents ». Chaque agent porte un gène « outils » : la sélection découvre si s'en servir aide. Chaque appel (entrée et résultat) est visible sur la carte de l'agent.

- **Exécution Python** : conteneur Docker jetable, sans réseau, sans accès aux fichiers, lecture seule, 256 Mo, temps limité. Image : `docker pull python:3.12-slim`.
- **Recherche web** : [SearXNG](https://github.com/searxng/searxng) local. Lecture des pages publiques uniquement (adresses locales et privées refusées, y compris l'arène elle-même).

  ```sh
  docker run -d --name arene-searxng --restart unless-stopped -p 127.0.0.1:8888:8080 \
    -v "$PWD/outils/searxng:/etc/searxng" searxng/searxng:latest
  ```

  `outils/searxng/settings.yml` doit autoriser le format JSON ; il n'est pas versionné car il contient une clé.
- qwen3 n'appelle ses outils qu'en mode réflexion : les étapes d'outils réfléchissent toujours (compter 10 à 60 s par agent), seule la réponse finale suit le réglage « Réflexion ».

## Moteur en ligne de commande

```sh
./target/release/evolution reel 10 6 2   # réel : 10 générations, 6 agents, 2 appels en parallèle
./target/release/evolution sim 30 12     # simulé : instantané, sans IA (pour tester les règles)
```

## Organisation du code

```
src/evo.rs           configuration partagée + validation
src/store.rs         écriture atomique, fichiers, commits Git automatiques
src/main.rs          serveur de l'interface (axum)
src/bin/evolution.rs moteur : génome, mutations, questions, juge, calibreur, appels Ollama
contenu/             gabarits des pages + graphiques.js
agents-locaux/       evolution.toml, mémorial
donnees/evolution/   un journal JSON (+ sortie brute .log, note .note.txt) par run ; runs d'exemple fournis
```

## Tests

```sh
cargo test --release     # unitaires : barème, calibreur, config, formulaire…
bash tests/essai.sh      # bout en bout sur un serveur temporaire (runs simulés, sans GPU)
```

La CI lance les deux à chaque push.

## Démarrage automatique (facultatif)

`arene.service` est une unité systemd utilisateur qui suppose le dépôt dans `~/arene`. Adapte les chemins si tu l'as cloné ailleurs.

```sh
mkdir -p ~/.config/systemd/user && cp arene.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now arene
```

Un run lancé depuis l'interface continue si le serveur redémarre : il est repris automatiquement.

## Accès depuis une autre machine

Il faut changer `adresse_serveur` et utiliser la clé d'arbitre : la connexion automatique ne s'applique jamais à travers un tunnel.

## Statut du projet

Ce projet est **terminé et archivé**. La version 1.0 est la version finale : le dépôt est en lecture seule, les issues et les pull requests sont fermées. Tu peux le forker librement selon les termes de la [licence MIT](LICENSE) ; voir [CONTRIBUTING.md](CONTRIBUTING.md).

## Voir aussi

[odoo-multiverse](https://github.com/rjvdev/odoo-multiverse), du même auteur : Odoo 16 → 20 côte à côte avec Docker, pensé pour les développeurs et les agents IA.

## Licence

[MIT](LICENSE) © 2026 rjvdev
