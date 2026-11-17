# Contributing to L'Arène des générations

Thanks for your interest!

## The project is archived

L'Arène des générations is **finished**. Version 1.0 is the final release and the repository is archived on GitHub: it is read-only, so issues and pull requests cannot be opened anymore.

The code is under the [MIT License](LICENSE): you are free to fork it, change it and build on it. If you do something fun with it, a mention of the original project is appreciated but not required.

For an actively maintained project by the same author, see [odoo-multiverse](https://github.com/rjvdev/odoo-multiverse).

## Working on a fork

Requirements: [Rust](https://rustup.rs/), git, Python 3 and curl (for the end-to-end test). Ollama is only needed for real runs.

```sh
git clone https://github.com/<you>/arene && cd arene
cargo build --release
cargo test --release      # unit tests
bash tests/essai.sh       # end to end, simulated runs, ends with "N réussis, 0 échoués"
```

The CI in `.github/workflows/ci.yml` runs the same checks and works as is on a fork.

## Conventions used in this codebase

- The interface, the code identifiers and the comments are in French; the community files are in English, and `README.fr.md` is the French translation of `README.md`.
- Everything the referee can change goes through `agents-locaux/evolution.toml` and is validated in `src/evo.rs` before it is saved.
- Writes are atomic and committed to Git (`src/store.rs`); keep it that way so that every run can be traced back to its settings.
- Agent tools stay sandboxed: no network for Python, no local or private addresses for web search.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `ci:`…).
