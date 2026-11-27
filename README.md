<div align="center">

# teach_me

**Your own AI tutor that agrees on a goal with you, writes the lessons and makes sure you remember them.**

[![CI](https://github.com/francesco-gaglione/teach_me/actions/workflows/ci.yml/badge.svg)](https://github.com/francesco-gaglione/teach_me/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/backend-Rust-orange?logo=rust)
![Kotlin](https://img.shields.io/badge/app-Kotlin%20%2B%20Compose-7F52FF?logo=kotlin)
![Self-hosted](https://img.shields.io/badge/self--hosted-yes-success)

![A generated lesson about TCP, with its table of contents, glossary and sources](.github/assets/lesson.png)

</div>

Pick a topic, any topic. A tutor interviews you about what you already know and why you want to learn it, proposes a **mission** with concrete success criteria, goes looking for sources, and then writes lessons for *you*: not chat replies, but native Android screens with exercises, explained feedback, a glossary and citations. Whatever you learn comes back later as spaced-repetition reviews, so it stays learned.

It runs on your own server, supports several users, and each user brings their own LLM API key.

## How it works

```mermaid
flowchart LR
    A[📁 Topic] --> B[💬 Interview]
    B --> C[🎯 Mission<br/>success criteria]
    C --> D[🔎 Research<br/>sources]
    D --> E[🗺️ Plan]
    E --> F[📖 Lessons<br/>native UI]
    F --> G[✅ Exercises<br/>explained feedback]
    G --> H[🧠 Learning record]
    H --> I[🔁 FSRS reviews]
    I -. what you forget .-> F
```

1. **Interview and mission.** The tutor asks where you start from, what you want to be able to do and how much time you have, then proposes a mission you can edit or confirm.
2. **Research and plan.** A researcher agent collects sources (web pages, or PDFs you upload); a planner turns the mission into a sequence of lessons.
3. **Lessons as native UI.** A generator writes each lesson as Server-Driven UI that the app renders with native components; a validator checks it before you ever see it.
4. **Practice.** Exercises are graded with an explanation of *why* you were right or wrong; open answers are graded by an LLM grader.
5. **Remember.** What you have shown you know goes into an evidence-based learning record and comes back as FSRS-7 reviews, with weights tuned to your own answers.

## Features

- 🧩 **Rich lesson blocks**: single and multiple choice quizzes, fill in the blank, order the steps, open answers, flashcards, checklists, code, diagrams, callouts, primary sources, glossary terms.
- 🔁 **Spaced repetition** with FSRS-7, review notifications and per-topic review filters.
- 💬 **Chat with the tutor** about the topic, starting from any lesson or exercise.
- ✍️ **Highlighter and handwriting** on top of the lesson, for a stylus on a tablet.
- 🔊 **Read aloud**, with a self-hosted text-to-speech engine (Qwen3-TTS, CPU only).
- 📄 **Your own sources**: upload PDFs and the lessons cite them.
- 🌍 **Italian and English**, for the interface (follows the device) and for the content (chosen per topic).
- 👥 **Multi-user and self-hosted**: each user's API key is stored encrypted on the server.
- 📱 **Tablet first**: designed for a landscape tablet, works on phones too.

| Exercise with explained feedback | Mission agreed with the tutor |
|---|---|
| ![A fill-in-the-blank exercise with its explained feedback](.github/assets/exercise.png) | ![The mission proposed by the tutor, with success criteria](.github/assets/mission.png) |

## Architecture

```mermaid
flowchart LR
    App["📱 Android app<br/>Kotlin · Compose<br/>SDUI renderer"] <-- "REST + SSE" --> API["🦀 Rust backend<br/>axum · clean architecture"]
    API --> Agents["🤖 LLM agents<br/>tutor · interviewer · researcher<br/>planner · generator · grader · recorder"]
    Agents --> LLM["☁️ LLM provider<br/>(Anthropic, OpenRouter)"]
    API --> DB[("🗄️ SQLite")]
    API --> TTS["🔊 TTS service"]
```

- **Backend**: a Rust workspace split into `domain`, `application`, `infrastructure` and `api`, with an axum HTTP API, SQLite, a background job runner and Docker deployment. Each agent has its own prompt, structured output and **evaluation suite** that measures the quality of what it produces.
- **Contracts first**: the API and the SDUI schema live in [`docs/contracts/`](docs/contracts/INDEX.md), and both backend and app test against them.
- **App**: Kotlin and Jetpack Compose with its own design system (no Material), built around readability, and a renderer that turns SDUI blocks into native components.

## Built by directing a team of AI agents

Most of the code was written by AI coding agents (Claude Code), directed by me. I acted as product owner and tech lead: I set the scope, made the architectural decisions and reviewed the results, while specialised agents implemented each step.

The process is part of the repository, so you can judge it yourself:

- [`CLAUDE.md`](CLAUDE.md) and [`.claude/agents/`](.claude/agents/) define the team (backend, Android, LLM engineering, learning design, code review, documentation) and the rules for delegating work in short, verifiable steps;
- [`docs/adr/`](docs/adr/README.md) records the decisions and why they were taken;
- [`docs/contracts/`](docs/contracts/INDEX.md) holds the contracts that backend and app both test against;
- every relevant step was reviewed by a separate read-only reviewer agent, and CI runs formatting, lints and the full test suites.

The question behind the project, besides the app itself: how far can a single developer get by running a small team of agents with clear contracts, an explicit architecture and tests as the definition of done?

## Inspiration

The idea comes from Matt Pocock's [`teach` skill](https://github.com/mattpocock/skills/tree/main/skills/productivity/teach), a prompt that turns a coding agent into a tutor: it interviews you, agrees on what you want to learn, and keeps a record of what you have understood. The learning flow and the formats of resources, glossary and learning record follow that skill.

teach_me takes the idea out of the chat and into a full application: specialised agents instead of one prompt, lessons as native screens instead of chat text, spaced repetition so that you keep what you learn, and a self-hosted multi-user server.

## Quick start

You need a Linux x86_64 machine with Docker and the Compose plugin, an Android 10+ phone or tablet, and an [Anthropic API key](https://console.anthropic.com/). [Tailscale](https://tailscale.com/) is recommended to reach the server from outside your home.

**1. Start the server**

```bash
git clone https://github.com/francesco-gaglione/teach_me.git
cd teach_me/backend/deploy
cp .env.example .env && chmod 600 .env
# In .env set TEACH_ME_SECRET_KEY to the output of `openssl rand -base64 32` (keep a copy: it encrypts the users' API keys)
# and BIND_IP to the address the app will use: `tailscale ip -4` (recommended), or the machine's LAN IP
# (traffic, passwords included, then travels as plain HTTP on your network; not 127.0.0.1, which clashes with a second port mapping).
docker compose up -d --build
```

The first build compiles the backend and takes a few minutes. Check it with `curl http://<BIND_IP>:8080/health`.

Read-aloud is on by default: on its first start the `tts` service downloads about 4.5 GB in the background, and the rest of the app works in the meantime. To skip it, set `TEACH_ME_TTS_URL=` (empty) in `.env` and start only `docker compose up -d --build teach-me jaeger`.

**2. Set a password for the first user**

A new database already has a user named `owner`, without a password:

```bash
docker compose exec teach-me teach-me user passwd owner   # prints a generated password once
```

To add more people: `docker compose exec teach-me teach-me user add <name>`.

**3. Install the app**

Download `teach_me-<version>.apk` from the [latest release](https://github.com/francesco-gaglione/teach_me/releases/latest) and install it. You may need to allow installs from unknown sources. Open the app, enter `http://<BIND_IP>:8080` and sign in as `owner`, then paste your Anthropic API key in Settings, under *LLM key*. Create a folder, add a topic, and the tutor starts the interview.

[`backend/README.md`](backend/README.md) covers configuration, models, backups and updates. [`android/README.md`](android/README.md) explains how to build the app yourself.

### Development

```bash
cd backend
export TEACH_ME_SECRET_KEY="$(openssl rand -base64 32)"
cargo run -p api --bin teach-me
```

The Android app needs JDK 21 and the Android SDK: see [`android/README.md`](android/README.md#build).

## What is in the repository

| Folder | What |
|---|---|
| [`backend/`](backend/README.md) | Rust workspace, axum, SQLite, job runner, Docker deployment |
| `backend/prompts/`, `backend/evals/` | Agent prompts and the evaluation suites that measure lesson quality |
| [`android/`](android/README.md) | Kotlin + Jetpack Compose app |
| [`docs/`](docs/mvp.md) | Scope (`mvp.md`), decisions (`adr/`), API and SDUI contracts (`contracts/`), visual and interaction design (`design/`) |

The documentation is written in Italian; code, comments and commit messages are in English.

## Credits

- [`teach` skill](https://github.com/mattpocock/skills) by Matt Pocock (MIT): the learning flow and the formats of resources, glossary and learning record.
- [`fsrs`](https://github.com/open-spaced-repetition/fsrs-rs) (BSD-3-Clause) and the FSRS research by the open-spaced-repetition project.
- Read-aloud runs [Qwen3-TTS](https://github.com/QwenLM/Qwen3-TTS) through [qwen3-tts](https://github.com/gabriele-mastrapasqua/qwen3-tts) (MIT); the model is downloaded at first start, not shipped here.
- [PDFium](https://pdfium.googlesource.com/pdfium/) for PDF text extraction, [Lucide](https://lucide.dev) icons (ISC).
- Fonts bundled in the app: [Inter](https://github.com/rsms/inter), [Literata](https://github.com/googlefonts/literata) and [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono), all under the SIL Open Font License 1.1 (license texts in [`licenses/fonts/`](licenses/fonts/)).

## License

Code: [MIT](LICENSE). Bundled fonts: SIL Open Font License 1.1, see [`licenses/fonts/`](licenses/fonts/).
