# L'Arène des générations

Une population d'agents Qwen (en local, via Ollama) s'affronte génération après génération sur des
questions vérifiables automatiquement. Les meilleurs survivent et se reproduisent (croisement + mutation),
les autres meurent et laissent un mémorial. Tout se pilote et se modifie depuis une interface web locale.

## Démarrer
```bash
cd ~/arene
cargo build --release          # compile le serveur (arene) et le moteur (evolution)
./target/release/arene         # puis ouvrir http://127.0.0.1:8080
```
Prérequis : Rust, et pour le mode réel Ollama avec le modèle configuré (`ollama pull qwen3:4b`).
Depuis ta machine, la connexion est automatique (réglable dans Paramètres). Sinon, la clé est dans
`donnees/cle-admin.txt`.

## L'interface
| Page | Rôle |
|---|---|
| **Tableau de bord** (`/`) | Lancer / arrêter une évolution, suivre le run en direct (progression, question en cours), indicateurs, graphiques, arbre généalogique, détail par génération, mémorial |
| **Historique** (`/runs`) | Tous les runs en heure locale, notes, suppression, comparaison de deux runs (y compris les paramètres qui ont changé) |
| **Paramètres** (`/parametres`) | Formulaire de tous les réglages du moteur, vérifiés avant enregistrement |
| **Fichiers** (`/fichiers`) | Mode expert : édition brute de la config, des gabarits des pages et des résultats des runs |

## Le jeu
- Questions : calculs exacts de difficulté réglée par un calibreur (cible : 3 à 5 réussites par
  génération) + une part de pièges impossibles. Une question peut aussi être imposée au lancement.
- Choix de chaque agent : COMBAT, ABANDON ou REFUS, avec une confiance déclarée.
- Barème : combat juste = survie (départage par la confiance) ; refus d'un piège = survie ;
  abandon = mort honnête ; faux avec forte confiance = pire que l'abandon.
- Génome : modèle, goût du risque, 2-3 principes injectés dans le prompt, température.

## Ce que l'arbitre peut muter
Tout est dans **`agents-locaux/evolution.toml`** (formulaire dans Paramètres) : population, difficulté,
pièges, principes, modèles, taux de mutation, réglages Ollama, prompt système, adresse du serveur.
Les gabarits des pages (`contenu/*.html`, `contenu/graphiques.js`) et les résultats
(`donnees/evolution/*.json`) sont aussi modifiables. Chaque modification est versionnée dans Git, et
chaque run garde une copie des paramètres avec lesquels il a tourné.

## Outils des agents (facultatif)
À activer dans Paramètres → « Outils des agents ». Chaque agent porte un gène « outils » : la sélection
découvre si s'en servir aide. Chaque appel (entrée et résultat) est visible sur la carte de l'agent.
- **Exécution Python** : conteneur Docker jetable, sans réseau, sans accès aux fichiers, lecture seule,
  256 Mo, temps limité. Image : `docker pull python:3.12-slim`.
- **Recherche web** : SearXNG local. Lecture des pages publiques uniquement (adresses locales et privées
  refusées, y compris l'arène elle-même).
  ```bash
  docker run -d --name arene-searxng --restart unless-stopped -p 127.0.0.1:8888:8080 \
    -v ~/arene/outils/searxng:/etc/searxng searxng/searxng:latest
  ```
  (`outils/searxng/settings.yml` doit autoriser le format JSON ; il n'est pas versionné car il contient une clé.)
- qwen3 n'appelle ses outils qu'en mode réflexion : les étapes d'outils réfléchissent toujours
  (compter 10 à 60 s par agent), seule la réponse finale suit le réglage « Réflexion ».

## Moteur en ligne de commande
```bash
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
agents-locaux/       evolution.toml
donnees/evolution/   un journal JSON (+ sortie brute .log, note .note.txt) par run
```

## Tests
```bash
cargo test --release     # unitaires : barème, calibreur, config, formulaire…
bash tests/essai.sh      # bout en bout sur un serveur temporaire (runs simulés, sans GPU)
```

## Démarrage automatique (facultatif)
```bash
mkdir -p ~/.config/systemd/user && cp ~/arene/arene.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now arene
```
Un run lancé depuis l'interface continue si le serveur redémarre : il est repris automatiquement.

## Pour plus tard
Pour rendre l'interface accessible ailleurs que sur ta machine, il faut changer `adresse_serveur`
(et utiliser la clé d'arbitre : la connexion automatique ne s'applique jamais à travers un tunnel).
