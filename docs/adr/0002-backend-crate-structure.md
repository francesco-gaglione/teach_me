# ADR 0002 — Struttura dei crate del backend

- Stato: accettata
- Data: 2026-11-08

## Contesto

`docs/mvp.md` §3 fissa la regola delle dipendenze: `domain` non conosce framework, `application` orchestra i casi d'uso tramite i port, `infrastructure` implementa i port, `api` compone tutto. Vogliamo che il compilatore faccia rispettare la regola, non la disciplina.

## Decisione

Workspace Cargo in `backend/`, un crate per livello:

```
backend/
├── Cargo.toml              # workspace, [workspace.dependencies], lints condivisi
├── migrations/             # SQL sqlx, NNNN_descrizione.sql
├── prompts/                # proprietà di llm-engineer
├── evals/                  # proprietà di llm-engineer
├── config/                 # esempio di configurazione (teach_me.example.toml)
└── crates/
    ├── domain/             # entità, value object, regole, errori di dominio, port (trait)
    ├── application/        # casi d'uso, job runner, DTO dei casi d'uso
    ├── infrastructure/     # adapter: sqlite, llm_anthropic, search, fsrs, clock, id
    └── api/                # axum, config, composition root, binario `teach-me`
```

| Crate | Può dipendere da | Non deve dipendere da |
|---|---|---|
| `domain` | `serde`, `serde_json` (schemi e input dei tool LLM, ADR 0004), `thiserror`, `uuid`, tipo data/ora, `async-trait` | tokio (runtime), axum, sqlx, reqwest |
| `application` | `domain`, `tokio` (sync, `spawn` per i task in background dei casi d'uso, `time` per backoff e flush; mai il runtime né I/O), `tracing` | axum, sqlx, reqwest, `infrastructure` |
| `infrastructure` | `domain`, sqlx, reqwest, ... | `api`, `application` |
| `api` | tutti | — |

Regole aggiuntive:

- **Port come trait** nel dominio, iniettati come `Arc<dyn Port>` (con `async-trait` finché i trait async non sono dyn-compatibili in stable). I casi d'uso ricevono le dipendenze nel costruttore.
- `infrastructure` è un solo crate con un modulo per adapter; lo divideremo in più crate solo se i tempi di compilazione lo richiedono.
- I tipi di `api` (DTO JSON) sono separati dalle entità di dominio: il dominio non ha attributi serde legati al formato del contratto. Le conversioni stanno in `api`.
- Gli errori di dominio e applicativi sono enum `thiserror`; `api` li mappa sui codici di `docs/contracts/api-conventions.md` §7 in un solo punto.
- Lint del workspace: `unsafe_code = "forbid"`, clippy `pedantic` come warning selettivi, `-D warnings` in CI/script.
- Test: unitari dentro ogni crate; i casi d'uso si testano con fake dei port (in `application`, modulo `#[cfg(test)]` o crate `test-support` se servirà condividerli); integrazione HTTP nel crate `api` (`tests/`).

## Alternative considerate

- **Un solo crate con moduli**: più semplice, ma la regola delle dipendenze non sarebbe verificata dal compilatore.
- **Un crate per adapter** (`infra-sqlite`, `infra-anthropic`...): isolamento massimo, ma troppo cerimoniale per l'MVP.

## Conseguenze

- Aggiungere un adapter (es. OpenAI-compatible) = nuovo modulo in `infrastructure` + una riga nella composition root.
- Un `cargo tree -p domain` deve restare corto: è il controllo rapido che il dominio sia pulito.
