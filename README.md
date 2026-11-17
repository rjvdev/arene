# L'Arène des générations

**An evolutionary game between local AI agents. Qwen agents running on Ollama compete generation after generation; the best survive and breed, the rest die and leave a memorial.**

[![CI](https://github.com/rjvdev/arene/actions/workflows/ci.yml/badge.svg)](https://github.com/rjvdev/arene/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Status: archived](https://img.shields.io/badge/status-archived-lightgrey.svg)](#project-status)

[Version française](README.fr.md)

## The idea

Can selection pressure alone teach small language models when to answer, when to give up and when to spot a trick question? The Arena tries it.

- **Questions** are checked automatically: exact calculations whose difficulty is tuned by a calibrator (target: 3 to 5 successes per generation), plus a share of impossible trick questions. A question can also be imposed at launch.
- **Each agent chooses** to FIGHT, GIVE UP or REFUSE, with a declared confidence.
- **Scoring**: a correct fight survives (ties broken by confidence); refusing a trick survives; giving up is an honest death; a wrong answer given with high confidence is worse than giving up.
- **Genome**: model, risk appetite, 2-3 principles injected into the prompt, temperature, and a "tools" gene.
- **Reproduction**: crossover and mutation produce the next generation; the dead leave a memorial the survivors can read.

Everything is driven and editable from a local web interface. The program is written in Rust (axum, tokio, minijinja); the interface and the code identifiers are in French.

## Quick start

Requirements: [Rust](https://rustup.rs/), git, and for real runs [Ollama](https://ollama.com/) with the configured model.

```sh
git clone https://github.com/rjvdev/arene && cd arene
ollama pull qwen3:4b             # only for real runs; simulated runs need no AI
cargo build --release            # builds the server (arene) and the engine (evolution)
./target/release/arene           # then open http://127.0.0.1:8080
```

A prebuilt Linux binary is attached to the [v1.0 release](https://github.com/rjvdev/arene/releases/tag/v1.0).

From your own machine you are logged in automatically (configurable in Settings). Otherwise, the referee key is written to `donnees/cle-admin.txt` on first start.

## The interface

| Page | Purpose |
|---|---|
| **Dashboard** (`/`) | Start / stop an evolution, follow the run live (progress, current question), indicators, charts, family tree, per-generation detail, memorial |
| **History** (`/runs`) | Every run in local time, notes, deletion, comparison of two runs (including the settings that changed) |
| **Settings** (`/parametres`) | A form for every engine setting, validated before saving |
| **Files** (`/fichiers`) | Expert mode: raw editing of the config, the page templates and the run results |

## What the referee can mutate

Everything lives in **`agents-locaux/evolution.toml`** (form in Settings): population, difficulty, traps, principles, models, mutation rates, Ollama settings, system prompt, server address. Page templates (`contenu/*.html`, `contenu/graphiques.js`) and results (`donnees/evolution/*.json`) are editable too. Every change is committed to Git automatically, and each run keeps a copy of the settings it ran with.

## Agent tools (optional)

Enable them in Settings → "Outils des agents". Each agent carries a "tools" gene, so selection finds out whether using them helps. Every call (input and result) is shown on the agent's card.

- **Python execution**: throwaway Docker container, no network, no file access, read-only, 256 MB, time-limited. Image: `docker pull python:3.12-slim`.
- **Web search**: local [SearXNG](https://github.com/searxng/searxng). Public pages only (local and private addresses are refused, the Arena included).

  ```sh
  docker run -d --name arene-searxng --restart unless-stopped -p 127.0.0.1:8888:8080 \
    -v "$PWD/outils/searxng:/etc/searxng" searxng/searxng:latest
  ```

  `outils/searxng/settings.yml` must allow the JSON format. It is not versioned because it contains a secret key.
- qwen3 only calls tools in thinking mode: tool steps always think (count 10 to 60 s per agent), only the final answer follows the "Réflexion" setting.

## Command-line engine

```sh
./target/release/evolution reel 10 6 2   # real: 10 generations, 6 agents, 2 parallel calls
./target/release/evolution sim 30 12     # simulated: instant, no AI (to test the rules)
```

## Code layout

```
src/evo.rs           shared configuration + validation
src/store.rs         atomic writes, files, automatic Git commits
src/main.rs          web interface server (axum)
src/bin/evolution.rs engine: genome, mutations, questions, judge, calibrator, Ollama calls
contenu/             page templates + graphiques.js
agents-locaux/       evolution.toml, memorial
donnees/evolution/   one JSON log (+ raw .log output, .note.txt note) per run; 8 sample runs included
```

## Tests

```sh
cargo test --release     # unit tests: scoring, calibrator, config, form…
bash tests/essai.sh      # end to end on a temporary server (simulated runs, no GPU)
```

The CI runs both on every push.

## Start on boot (optional)

`arene.service` is a systemd user unit that expects the repository in `~/arene`. Adjust the paths if you cloned it elsewhere.

```sh
mkdir -p ~/.config/systemd/user && cp arene.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now arene
```

A run started from the interface keeps going if the server restarts: it is resumed automatically.

## Exposing it beyond your machine

Change `adresse_serveur` and use the referee key: automatic local login never applies through a tunnel.

## Project status

This project is **finished and archived**. Version 1.0 is the final release: the repository is read-only, and issues and pull requests are closed. You are welcome to fork it under the terms of the [MIT License](LICENSE); see [CONTRIBUTING.md](CONTRIBUTING.md).

## See also

[odoo-multiverse](https://github.com/rjvdev/odoo-multiverse), by the same author: run Odoo 16 → 20 side by side with Docker, built for developers and AI agents.

## License

[MIT](LICENSE) © 2026 rjvdev
