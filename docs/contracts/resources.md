# Contratto: Fonti (ricerca del Researcher)

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1.3** (2026-11-15), Fasi 4 e 10 (documenti caricati: `documents.md`).
> **Issue #15 (v1.3):** le community sono **facoltative**: il Researcher le cerca solo se le preferenze le chiedono; altrimenti `communities_skipped` è `true` e tutte le ricerche vanno a fonti `knowledge`. Le fonti `community` non entrano mai nelle lezioni (le scarta il backend prima del Planner).
> **Fase 11 (v1.2):** l'avvio e l'aggiornamento della ricerca senza chiave LLM rispondono `409 llm_key_missing` (`users.md` §3).
> Valgono `api-conventions.md`, `mission.md` e `jobs.md`. Spec visiva: `docs/design/screens/resources.md`. I path omettono il prefisso `/api/v1`.

## 1. Concetti

- Dopo la **prima** conferma della mission il Topic passa a `researching` e il backend crea un job `research` (`jobs.md`). Il Researcher cerca sul web con gli strumenti lato server del provider LLM (ADR 0005, D1) e produce le **fonti** del Topic, fedeli al formato `RESOURCES-FORMAT.md` della skill `teach`:
  - `knowledge`: fonti ad alta fiducia (documentazione, libri, articoli, corsi…);
  - `community`: dove imparano le persone (forum, gruppi, corsi dal vivo…), **solo se l'utente le chiede** (una preferenza, globale o del Topic, salvata dal Tutor). Non entrano mai nelle lezioni: Planner e Generator ricevono solo `knowledge` e `document`;
  - `gaps`: cosa non si è trovato.
- Quando il job riesce, il Topic passa a `ready` (evento `topic_updated`). Se fallisce, il Topic **resta `researching`** e l'utente può solo riprovare (`jobs.md` §3): niente "continua senza fonti".
- **Rifare la ricerca** (es. dopo un cambio di mission) è una scelta dell'utente (§3), mai automatica: ogni ricerca costa. Il Topic resta nel suo stato (`ready` resta `ready`).
- Le fonti `document` (PDF caricati, `documents.md`) non sono del Researcher: le crea e le elimina solo l'import o l'eliminazione del documento. Il Researcher riceve il loro indice e cerca fonti web che le completano.
- Una ricerca riuscita **sostituisce in blocco** le fonti web (`knowledge`, `community`) e i gaps del Topic (in una transazione). Il Researcher riceve le fonti esistenti nel contesto e può riproporre quelle ancora valide. Se la ricerca fallisce, l'elenco precedente resta intatto.
- Garanzie del backend (Validator), non affidate al prompt: ogni `url` compare tra i risultati di ricerca di quel job; nessuna fonte `community` se `communities_skipped` è `true`; lunghezze entro i limiti del §2.
- Limite accettato: le preferenze sono testo libero, quindi `communities_skipped` lo decide il Researcher leggendole (`false` solo se una preferenza chiede community). Il backend garantisce la coerenza tra il flag e le fonti salvate, non che il flag rispecchi la preferenza.

## 2. Risorse

### `Resource`

```json
{
  "id": "01933b4e-...",
  "kind": "knowledge",
  "source_type": "documentation",
  "title": "The Rust Programming Language",
  "url": "https://doc.rust-lang.org/book/",
  "domain": "doc.rust-lang.org",
  "language": "en",
  "annotation": "Il libro ufficiale, mantenuto dal team di Rust: copre ownership e borrowing con esempi completi.",
  "use_for": "capire ownership e borrow prima di leggere il codice del servizio",
  "created_at": "2026-11-09T14:40:12.003Z"
}
```

| Campo | Regole |
|---|---|
| `kind` | `knowledge` \| `community` \| `document` (PDF caricato, v1.1). L'app tratta valori sconosciuti come `knowledge` |
| `document_id` | solo per `kind: document`: il `Document` da cui nasce (`documents.md`); assente per le altre |
| `source_type` | `book` \| `paper` \| `article` \| `documentation` \| `video` \| `course` \| `standard` \| `pdf` (solo `document`) \| `forum` \| `social_group` \| `live_class` \| `local_group` \| `other`. L'app mostra "Fonte" per valori sconosciuti |
| `title` | 1-300 caratteri, nella lingua originale |
| `url` | `https://…` oppure `null` (per `community` senza sito, es. un corso dal vivo, e sempre per `document`). È la forma dell'URL come appare nei risultati di ricerca (o nella fonte già salvata), non quella scritta dal modello. L'app apre solo URL `https` |
| `domain` | host dell'`url` senza `www.`, calcolato dal backend; `null` se `url` è `null` |
| `language` | codice BCP-47 della fonte (`it`, `en`…) |
| `annotation` | 1-600 caratteri, nella lingua dell'argomento (ADR 0019): cosa copre e perché è affidabile. Per `document`: scritta dal backend ("PDF caricato da te, N pagine") |
| `use_for` | 1-300 caratteri, nella lingua dell'argomento (ADR 0019): frase nominale che completa "Usala per…", legata alla mission. Per `document`: testo fisso del backend |

L'ordine dell'array è: prima le fonti `document` (dalla più recente), poi quelle web nell'ordine di importanza deciso dal Researcher, stabile.

### `Gap`

```json
{ "area": "Migrazioni con sqlx in produzione", "note": "Nessuna guida affidabile e aggiornata: solo post di blog datati." }
```

`area` 1-200 caratteri, `note` 1-400 caratteri, italiano.

## 3. Endpoint

| Metodo | Path | Successo | Errori |
|---|---|---|---|
| GET | `/topics/{id}/resources` | `200` `ResourcesView` | `400 invalid_id`, `404` |
| POST | `/topics/{id}/resources/research` | `202` il `Job` `research` creato (`queued`) | `400 invalid_id`, `404`, `409 conflict` (nessuna mission confermata, oppure c'è già un job `research` attivo) |

### `ResourcesView`

```json
{
  "resources": [ { "...": "Resource" } ],
  "gaps": [ { "...": "Gap" } ],
  "communities_skipped": false,
  "research": { "job_id": "01933b4e-...", "mission_version": 3, "finished_at": "2026-11-09T14:40:12.003Z" },
  "job": { "...": "ultimo Job research del Topic, oppure null" }
}
```

- `research`: dati dell'ultima ricerca **riuscita**, `null` se non ce n'è ancora una. `mission_version` è la versione della mission usata: se la mission corrente ha una versione maggiore, l'app propone "Aggiorna le fonti".
- `job`: l'ultimo job `research` (qualunque stato), per mostrare avanzamento o errore senza una seconda chiamata. `POST …/research` crea sempre un job nuovo (il retry di `jobs.md` invece riusa quello fallito), quindi la lista dei job può contenerne diversi: l'app legge il job corrente da qui.
- Nessuna paginazione: le fonti di un Topic sono poche (indicativamente 5-12, al massimo 12 web più al massimo 5 documenti).
- `resources` può essere vuota con `gaps` non vuote: ricerca riuscita "senza fonti" (il Topic passa comunque a `ready`).

## 4. Il job `research`

- `progress.step`: `planning` | `searching` | `reading` | `evaluating` | `saving`. Il backend può saltare fasi; l'app mostra testi fissi per fase e solo lo spinner per uno `step` sconosciuto.
- `result` (quando `succeeded`): `{ "resource_count": 8, "gap_count": 2 }` (`resource_count` conta solo le fonti web).
- `error.code`: quelli di `jobs.md` §2; `invalid_output` = l'output del Researcher non ha superato il Validator neanche dopo il tentativo di riparazione.

## 5. Eventi

Nessun evento nuovo: `job_updated` (`jobs.md` §4) per l'avanzamento e `topic_updated` per `researching` → `ready`. Alla fine di un job riuscito, l'app ricarica `GET /topics/{id}/resources`.
