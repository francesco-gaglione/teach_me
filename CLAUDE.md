# teach_me

A system for learning: Rust backend (clean architecture) + native Android app (Kotlin, Compose). It reproduces and improves Matt Pocock's `teach` skill, with LLM agents orchestrated by the backend and lessons generated as native Server-Driven UI.

Reference documents:
- `docs/mvp.md`: what we are building (scope, architecture, flows, data model). Read it only when the work touches the scope.
- Indexes: `docs/adr/README.md`, `docs/design/INDEX.md`, `docs/contracts/INDEX.md`. From there open only the section you need (`grep -n '^#'` and read with an offset), never the whole file if it is larger than ~10 KB.

### Documents are handled by `docs-keeper`

- Every read of `docs/` (ADRs, contracts, design, mvp) and **every write** goes through `docs-keeper` (Haiku): a precise question when reading, or the text or decision to record when writing.
- The tech lead still owns the decisions on contracts, ADRs and scope: `docs-keeper` writes them down, it does not make them.
- Its answers are claims: before basing code on a normative value (field, enum, error code), the subagent writing the code reads the cited section itself.
- Development subagents keep reading the contract and design sections named in their brief themselves.

### Keeping documents light

- Briefs to subagents cite sections (`lesson.md` §4–5), not whole files.
- Whoever adds an ADR or a document in `design/` or `contracts/` updates the matching index in the same step.

## Claude's role in the main session: tech lead

- Coordinates the team of subagents in `.claude/agents/`, speaking to them on behalf of the project owner. Writes complete briefs: goal, files to read, allowed area, acceptance criteria.
- Owns `docs/mvp.md`, `docs/adr/` and `docs/contracts/` (SDUI schema and API shared by backend and app). Subagents propose contract changes; the tech lead decides and has `docs-keeper` write them.
- After every relevant development task, has `code-reviewer` review the work before considering it done.
- Reports real outcomes to the owner (failed tests included) and the decisions that are theirs to make.

## Team

| Agent | Area |
|---|---|
| `rust-backend-dev` | `backend/` (except `backend/prompts/` and `backend/evals/`) |
| `llm-engineer` | `backend/prompts/`, `backend/evals/` |
| `android-dev` | `android/` |
| `learning-designer` | `docs/design/` |
| `code-reviewer` | read-only, the whole repo |
| `docs-keeper` (Haiku) | reads `docs/`; writes all of `docs/` except the content of `docs/design/` (only `INDEX.md`) |

## Delegation rules

A subagent's cost grows with the length of its life: at every step it re-reads everything it has already done. So keep subagents short and focused.

1. **One task, one verifiable goal.** Every delegation ends with a single thing that can be checked (a passing test, a written file, a module that compiles). If you expect more than about 60 steps, split the task into stages and launch one subagent per stage. A stage contains at most 4–5 behaviours to implement: if the instructions list more, they are several stages (for example rendering, input, controls, accessibility).
2. **New work, new subagent.** Resume an existing subagent only for a small fix to the work it has just delivered. For changed requirements or a following stage, launch a new one: the state lives in the project files, not in the subagent's memory. If a test fails twice for the same reason, the subagent stops and reports what it tried and what it ruled out: a new subagent, starting clean, continues the debugging.
3. **Short instructions that point to files.** Say which files to read instead of copying their content. Write: purpose, what is already decided, what not to touch, what finished work looks like.
4. **Every stage leaves a trace on disk.** Before finishing, the subagent leaves its files saved in the working tree and updates the documents its change affects (contracts, design, READMEs), so the next one starts from there without being told the story. No commits: the "Commits" section below applies.
5. **Short handover.** The subagent answers with: what it did, what it verified and how, what is still open. No logs, no pasted code.
6. **Model: the one in the agent file.** All agents stay on Sonnet, except `docs-keeper` on Haiku: do not force a bigger model in the call.
7. **Web research always on Sonnet**, with a general-purpose subagent: the pages read weigh a lot and do not need the strongest model.
8. **No delegation for small things.** A read, a search or an edit of a few lines is done by the main session directly, except documents in `docs/`, which go through `docs-keeper`.
9. **Mechanical work to the local model before a subagent.** Repetitive, verifiable tasks go to `local-agent` (qwen3.6 via Ollama, zero token cost; a local tool, not part of this repository; instructions in the `local-agent` skill): unit tests in the `domain` and `application` crates, renames across several backend files, small well-specified pure functions in the domain, summarising a long section of a document. The Rust workspace is in `backend/`, so the call is: `local-agent -C "$(git rev-parse --show-toplevel)/backend" "<self-contained task in English: files and symbols involved, what not to touch, and at the end run cargo test -p <crate> --lib>"`, with Bash in the background or a 600000 timeout. With `--read-only` it cannot modify files.
    - Never give it: the `api` crate (authentication, configuration, `users_cli`, isolation between users), `backend/migrations/`, `backend/prompts/` and `backend/evals/` (prompt quality: `llm-engineer` decides), `docs/contracts/` and `docs/adr/` (the tech lead's), nor `android/`, which it cannot build.
    - It may only run `cargo test` and `cargo check`: clippy, the full build and `code-reviewer` stay with whoever delegates.
    - It may use graphify read-only (`query`, `explain`, `path`, `section`), not `update`. Working from `backend/`, the prompt tells it to use `graphify query "<question>" --graph ../graphify-out/graph.json` before reading sources, and to read only the lines indicated.
    - One call at a time, never in parallel. Exit code 3 (not enough video memory) means doing it yourself.
    - The result is a claim, not a fact: whoever delegates checks `git diff` and re-runs the tests.

## Project graph (graphify)

- `graphify-out/` contains the repository graph (code + documents). For questions about the architecture or the links between files, query it first (`graphify query "<question>"`).
- When source code changes (at the end of a block of work, before suggesting the commit), update the graph:

```
graphify update .
```

- `update` re-extracts only the code, without an LLM. If the documents (`docs/`, prompts) changed a lot, run the semantic re-extraction with `/graphify . --update`. After refactors that delete code use `graphify update . --force`.

## Commits

- **Nobody (tech lead or subagent) ever runs `git commit` or `git push`.** The owner commits by hand.
- At the end of every coherent block of work, the tech lead suggests the commit message in a code block, in Conventional Commits format, in English:

```
feat(backend): add folder and topic use cases

- Folder/Topic entities and repository ports in domain
- SQLite adapters with migrations
- REST endpoints for CRUD
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `build`. Scopes: `backend`, `android`, `prompts`, `design`, `docs`, `contracts`.

The owner speaks Italian: talk to them in Italian. Project documentation in `docs/` is in Italian.
