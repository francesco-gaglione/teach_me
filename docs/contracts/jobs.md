# Contratto: Job (operazioni lunghe)

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1.4** (2026-11-11).
> **v1.4:** nuovo codice di errore `llm_credit_exhausted` (credito del provider LLM esaurito, `users.md` §3): il retry è ammesso senza cambiare la chiave.
> **v1.3:** ogni job appartiene al proprietario dell'argomento e usa la sua chiave LLM; nuovi codici di errore `llm_key_missing` e `llm_key_rejected` (`users.md` §3). Il job di ricerca nato dalla conferma di una mission senza chiave fallisce con `llm_key_missing` e riparte con il retry. Il retry di un job LLM senza chiave risponde `409 llm_key_missing`.
> Valgono `api-conventions.md` (che per i job rimanda qui: questo documento sostituisce l'esempio del suo §9) e `chat.md` §4 per lo stream SSE. I path omettono il prefisso `/api/v1`.

## 1. Concetti

- Un **job** è un'operazione lunga eseguita in background dal backend per un Topic: `research` (ricerca delle fonti, `resources.md`), `lesson_generation` e `lesson_completion` (`lessons.md`), `document_import` (`documents.md`).
- Per ogni coppia (Topic, `kind`) può esserci **un solo job attivo** (`queued` o `running`). Eccezione: per `lesson_completion` la regola vale per (lezione, `kind`): esiste un solo job per lezione e lezioni diverse possono chiudersi in parallelo. Per `document_import` esiste un solo job per documento (il retry riusa quello), e resta un solo import attivo per Topic.
- Il backend esegue i job anche se l'app è chiusa. Dopo un riavvio del backend un job rimasto `running` torna `queued` e riparte; dopo 3 partenze interrotte fallisce con `interrupted`.
- Un job fallito **non** viene ritentato da solo: lo ritenta l'utente (§3). Il retry riusa lo stesso job (stesso `id`).

## 2. Risorsa `Job`

```json
{
  "id": "01933b4e-...",
  "topic_id": "01933b4e-...",
  "lesson_id": null,
  "document_id": null,
  "kind": "research",
  "status": "running",
  "progress": { "step": "searching", "message": "Ho cercato: documentazione ufficiale di axum", "done": null, "total": null },
  "error": null,
  "result": null,
  "attempt": 1,
  "created_at": "2026-11-09T14:36:01.129Z",
  "started_at": "2026-11-09T14:36:01.402Z",
  "finished_at": null
}
```

| Campo | Regole |
|---|---|
| `kind` | `research`, `lesson_generation`, `lesson_completion`, `document_import`. L'app mostra un testo generico per valori sconosciuti |
| `lesson_id` | `null`, tranne `lesson_completion`: la lezione che si chiude (`lessons.md` §6) |
| `document_id` | `null`, tranne `document_import`: il `Document` che si importa (`documents.md`), presente da quando il job è creato (v1.2) |
| `status` | `queued` \| `running` \| `succeeded` \| `failed` |
| `progress` | `null` oppure `{step, message, done, total}`. `step`: etichetta stabile scelta dal job (valori per `research` in `resources.md` §4); `message`: frase facoltativa nella lingua dell'argomento del job (ADR 0019) (`null` o ≤ 80 caratteri) che descrive un fatto già avvenuto, mai una stima di tempo; `done`/`total`: interi o `null`. **Nessuna percentuale** |
| `error` | `null` oppure `{code, message}`; `message` è un testo fisso per codice, nella lingua dell'argomento del job (ADR 0019). Codici: `timeout`, `interrupted`, `upstream_failed`, `rate_limited`, `invalid_output`, `internal`, `llm_key_missing`, `llm_key_rejected`, `llm_credit_exhausted` (dalla v1.4); per `document_import` anche quelli di `documents.md` §4. L'app mostra il `message` per codici sconosciuti |
| `result` | `null` finché non è `succeeded`; poi un oggetto che dipende dal `kind` (per `research`: `resources.md` §4; per `document_import`: `documents.md` §4) |
| `attempt` | numero di partenze del run corrente; 0 finché non parte; torna 0 con un retry |
| `started_at`, `finished_at` | `null` finché l'evento non accade; tornano `null` quando il job viene rimesso in coda |

## 3. Endpoint

| Metodo | Path | Successo | Errori |
|---|---|---|---|
| GET | `/topics/{id}/jobs` | `200` lista paginata (`api-conventions.md` §8), dal più recente; header `X-Last-Event-Id` come in `chat.md` | `400`, `404` (Topic) |
| POST | `/topics/{id}/jobs/{job_id}/retry` | `202` il `Job` rimesso in `queued` (`attempt` 0, `error` e `result` `null`) | `400 invalid_id`, `404` (Topic o job), `409 conflict` (job non `failed`, c'è già un job attivo dello stesso `kind` per il Topic, oppure esiste un job più recente dello stesso `kind`: si ritenta solo l'ultimo; per `lesson_completion` tutto si intende per lezione, per `document_import` per documento) |

Non esiste `GET /jobs/{id}`: lista ed eventi bastano (il job corrente di un `kind` è il primo della lista con quel `kind`).

## 4. Eventi SSE

| `type` | `payload` | Quando |
|---|---|---|
| `job_updated` | `{ "job": Job }` | Job creato, partito, avanzamento, terminato, rimesso in coda (riavvio o retry) |

- Il client fa **upsert per `job.id`**: l'evento è idempotente e sicuro nei replay.
- Alla prima conferma della mission l'ordine è `mission_confirmed` → `topic_updated` (`researching`) → `job_updated` (`queued`).
- I cambi di stato del Topic arrivano come `topic_updated` (`chat.md` §3), non dentro il job.
