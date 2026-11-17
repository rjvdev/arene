# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The project is archived: 1.0 is the final release.

## [1.0.1] - 2026-11-17

### Added

- MIT License.
- English README (`README.md`); the French one moves to `README.fr.md`.
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, this changelog, issue and pull request templates.
- CI: `cargo test` and the end-to-end test `tests/essai.sh` on every push and pull request; Dependabot for GitHub Actions.

### Changed

- `Cargo.toml`: version `1.0.0`, `license`, `repository`, `readme` and `keywords` fields.
- README: install instructions start from `git clone` instead of assuming `~/arene`.

No code change: the program is identical to 1.0.

## [1.0] - 2026-11-17

First and final public release, with a prebuilt Linux binary.

### Added

- Evolution engine (`evolution`): genome (model, risk appetite, principles, temperature, tools gene), crossover, mutation, calibrator targeting 3 to 5 successes per generation, trick questions, judge, memorial of the dead.
- Real mode (Qwen via Ollama, parallel calls, per-generation time limit, random turn order, duration estimate at launch) and simulated mode without AI.
- Web interface (`arene`, axum): live dashboard with charts and family tree, run history and comparison, settings form validated before saving, expert file editor.
- Every change committed to Git automatically; each run keeps a copy of its settings; interrupted runs resume after a server restart.
- Referee key and automatic local login (never through a tunnel).
- Agent tools: Python in a throwaway, network-less Docker container, and web search through a local SearXNG (private addresses refused).
- Sample runs, unit tests and an end-to-end test.

[1.0.1]: https://github.com/rjvdev/arene/compare/v1.0...v1.0.1
[1.0]: https://github.com/rjvdev/arene/releases/tag/v1.0
