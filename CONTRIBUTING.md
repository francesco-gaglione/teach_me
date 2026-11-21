# Contributing to teach_me

Thanks for your interest! Bug reports, ideas and pull requests are welcome.

## Before you start

- For anything larger than a small fix, **open an issue first** so we can agree on the approach.
- Decisions are recorded in [`docs/adr/`](docs/adr/README.md) and the API and SDUI contracts in [`docs/contracts/`](docs/contracts/INDEX.md). A change that touches a contract must update the contract too, and both backend and app must keep passing their contract tests.
- The documentation in `docs/` is written in Italian; code, comments, commit messages and pull requests are in English.

## Project layout

| Folder | What | Guide |
|---|---|---|
| `backend/` | Rust workspace (`domain`, `application`, `infrastructure`, `api`) | [`backend/README.md`](backend/README.md) |
| `backend/prompts/`, `backend/evals/` | Agent prompts and their evaluation suites | [`backend/evals/README.md`](backend/evals/README.md) |
| `android/` | Kotlin + Jetpack Compose app | [`android/README.md`](android/README.md) |

The backend follows clean architecture: `domain` must not depend on `tokio`, `axum`, `sqlx` or `reqwest` (`cargo tree -p domain` checks it).

## Checks

Run the same checks as CI before opening a pull request.

Backend:

```bash
cd backend
./scripts/check.sh   # cargo fmt --check, clippy -D warnings, tests
```

Android (JDK 21, Android SDK in `local.properties`):

```bash
cd android
./gradlew ktlintCheck lint testDebugUnitTest assembleDebug
```

If you change a prompt, run the evaluation suite of that agent and report the result in the pull request: prompts are judged by their evals, not by eye.

## Pull requests

- One topic per pull request, with tests for new behaviour.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/): `feat(backend): ...`, `fix(android): ...`, `docs: ...`.
- Never commit secrets: `.env`, keystores and API keys are git-ignored, keep them that way.

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).
