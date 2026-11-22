# Convenzioni delle API

> Contratto condiviso tra backend (`backend/`) e app (`android/`). Proprietario: tech lead.
> Versione: **v1.7** (2026-11-22, §12: conteggio dei token del budget LLM `TEACH_ME_USER_DAILY_LLM_TOKENS` e tolleranza di concorrenza su budget e quota; v1.6 2026-11-22, audit C8: chiarimento di `429 llm_budget_exceeded` in §7 e §12, in rapporto a `jobs.md` §2; `429 rate_limited` con `Retry-After: 5` per l'apertura di uno stream SSE oltre `TEACH_ME_MAX_SSE_PER_USER`; audit M9: compatibilità e supporto, §1; v1.4 2026-11-22, audit C8/M1: `GET /ready`, `409 storage_quota_exceeded`, `429 llm_budget_exceeded`, limiti configurabili; v1.3 2026-11-11: `502 llm_credit_exhausted`; v1.2 2026-11-10: accesso con nome utente e password, isolamento per utente; v1.1: `415`, limite di upload). Ogni endpoint specifico (cartelle, topic, chat...) ha il suo file in `docs/contracts/` e rispetta queste regole.

## 1. Trasporto e versionamento

- Tutte le API applicative stanno sotto il prefisso **`/api/v1`**. Gli esempi nei contratti omettono il prefisso solo se esplicitamente indicato.
- Una modifica incompatibile (campo rimosso o rinominato, semantica cambiata) richiede `/api/v2`. Aggiungere campi opzionali, endpoint o valori di enum **non** è incompatibile: l'app deve ignorare i campi sconosciuti e tollerare valori di enum sconosciuti (mappandoli su un valore `Unknown`).
- Corpo delle richieste e delle risposte: `application/json; charset=utf-8`, salvo SSE (`text/event-stream`).
- Nomi dei campi JSON in **`snake_case`**. Valori di enum in `snake_case` (es. `"mission_proposed"`).
- Un campo opzionale senza valore si serializza come `null` (non viene omesso) nelle risposte. Nelle richieste il client può ometterlo.

### Compatibilità e supporto (aggiornamento 2026-11-22, audit M9)

- Si supporta solo l'ultima versione su `main` (invariato).
- Server e database si aggiornano insieme. Le migrazioni sono solo in avanti: non esiste una migrazione di downgrade. Tornare a una versione precedente del server significa ripristinare il backup preso da `deploy.sh` prima del deploy: è automatico se il deploy fallisce; a mano, `backup.sh restore <file>` (`backend/README.md`, «Updating with `deploy.sh`»). I dati scritti dopo il deploy vanno persi.
- Le modifiche a `/api/v1` sono solo additive: nuovi campi, endpoint o codici. L'app ignora i campi sconosciuti; un blocco SDUI sconosciuto segue il fallback di `sdui.md` §1. Una modifica incompatibile richiede `/api/v2` e una nuova ADR.
- L'app legge `api_version` da `GET /api/v1/health` (§3). Se la major di `api_version` del server è superiore a quella che l'app supporta, l'app mostra «aggiorna l'app». Il contratto non definisce altro comportamento in caso di mismatch.

## 2. Autenticazione

- Più utenti, ognuno con nome utente e password. Dettagli, sessioni e chiave LLM: **`users.md`**.
- `POST /api/v1/auth/login` restituisce un token di sessione opaco. Ogni altra richiesta sotto `/api/v1` porta l'header `Authorization: Bearer <token>`.
- Token mancante, malformato, scaduto o revocato → `401` con codice errore `unauthorized`. Credenziali errate all'accesso → `401 invalid_credentials`.
- Il backend salva solo l'hash di token e password e non li scrive mai nei log.
- Ogni risorsa appartiene all'utente che l'ha creata. Una risorsa di un altro utente risponde `404 not_found`, come se non esistesse.

## 3. Endpoint di servizio

| Metodo | Path | Auth | Risposta |
|---|---|---|---|
| GET | `/health` | no | `200 {"status": "ok"}` — liveness, per systemd/Docker |
| GET | `/ready` | no | `200` o `503` — readiness, per orchestrazione (deploy, health check del container) (aggiornamento 2026-11-22, audit C8/M1) |
| GET | `/api/v1/health` | sì | `200` con il corpo sotto |

```json
{
  "status": "ok",
  "version": "0.1.0",
  "api_version": 1,
  "catalog_version": 1,
  "server_time": "2026-11-08T14:36:01.129Z"
}
```

`api_version` è la major della API: regole di compatibilità in §1.

`GET /ready` (aggiornamento 2026-11-22, audit C8/M1): senza autenticazione, per l'orchestrazione (script di deploy, health check del container). `/health` resta invariato e misura solo la liveness.

```json
{
  "status": "ready",
  "checks": { "database": "ok", "jobs": "ok", "tts": "ok" }
}
```

- `200` con `"status": "ready"` quando `checks.database` e `checks.jobs` sono `"ok"`.
- `503` con lo stesso corpo e `"status": "not_ready"` quando `checks.database` o `checks.jobs` è `"down"`. Il `503` di questo endpoint usa questa forma e non quella d'errore del §7.
- `checks.database` e `checks.jobs` valgono `"ok"` o `"down"`. `checks.tts` vale `"ok"`, `"down"` o `"disabled"`; TTS è opzionale, quindi `"down"` non rende il servizio `not_ready`.
- Il corpo non contiene messaggi d'errore, versioni né percorsi.

Prima dell'accesso l'app verifica l'indirizzo con `GET /health` (senza autenticazione): timeout, DNS, connessione rifiutata → "backend irraggiungibile"; altro → "errore imprevisto" con il codice.

## 4. Identificatori

- Tutti gli id sono **UUID v7** (ordinati nel tempo), serializzati come stringa minuscola con trattini: `"01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f"`.
- Gli id li genera sempre il backend (port `IdGenerator`). Un id malformato in un path → `400 invalid_id`.
- Gli id dei blocchi SDUI (`block.id`) sono un'eccezione: stringhe stabili scelte dal Generator, uniche nella lezione (vedi `sdui.md`).

## 5. Date e ore

- Istanti in **RFC 3339, UTC, con suffisso `Z` e millisecondi**: `"2026-11-08T14:36:01.129Z"`. Il backend accetta in ingresso anche offset diversi da `Z` e li normalizza in UTC.
- Date senza ora (raro): `"2026-11-08"`.
- Durate: numero intero di secondi, con il suffisso nel nome del campo (`timeout_secs`).
- Campi standard delle risorse persistite: `created_at`, `updated_at`.

## 6. Metodi e codici di stato

| Operazione | Metodo | Successo |
|---|---|---|
| Lista | `GET` collezione | `200` con envelope di paginazione |
| Dettaglio | `GET` risorsa | `200` con la risorsa |
| Creazione | `POST` collezione | `201` con la risorsa creata + header `Location` |
| Modifica parziale | `PATCH` risorsa | `200` con la risorsa aggiornata. Campo assente = invariato |
| Eliminazione | `DELETE` risorsa | `204` senza corpo. Eliminare una risorsa inesistente → `404` |
| Avvio di un'operazione lunga | `POST` | `202` con il `Job` creato (vedi §9) |

Le risorse si restituiscono **senza envelope** (l'oggetto è il corpo). Solo le liste usano l'envelope del §8.

## 7. Errori

Ogni risposta di errore (4xx, 5xx) ha questo corpo:

```json
{
  "error": {
    "code": "validation_failed",
    "message": "Il nome della cartella non può essere vuoto.",
    "details": [
      { "field": "name", "issue": "empty" }
    ],
    "request_id": "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f"
  }
}
```

- `code`: stringa stabile, pensata per il codice dell'app. L'app decide il testo da mostrare in base al `code`.
- `message`: testo leggibile per la persona, generico, per log e fallback. Lingua: quella dell'header `Accept-Language` della richiesta: `en` se la prima lingua supportata è l'inglese, altrimenti italiano (default; ADR 0019). Non contiene mai dati sensibili né dettagli interni (stack trace, SQL).
- `details`: lista oppure `null` (mai `[]`). Per `validation_failed` contiene `{field, issue}` quando l'errore è attribuibile a un campo; un 422 per JSON di forma sbagliata (tipo errato, campo obbligatorio mancante) può avere `details: null`.
- `request_id`: lo stesso valore dell'header di risposta `X-Request-Id`.

| HTTP | `code` | Quando |
|---|---|---|
| 400 | `bad_request` | JSON malformato, parametro di query non valido |
| 400 | `invalid_id` | id nel path non è un UUID |
| 401 | `unauthorized` | token di sessione mancante, errato, scaduto o revocato |
| 401 | `invalid_credentials` | nome utente o password errati all'accesso (`users.md`) |
| 404 | `not_found` | risorsa inesistente, di un altro utente, o route sconosciuta |
| 405 | `method_not_allowed` | metodo non supportato sulla route (header `Allow` in risposta) |
| 409 | `conflict` | stato incompatibile (es. lezione richiesta senza mission confermata) |
| 409 | `llm_key_missing` | l'utente non ha salvato una chiave LLM (`users.md` §3) |
| 409 | `storage_quota_exceeded` | il caricamento porterebbe i documenti dell'utente oltre la quota di archiviazione (`documents.md` §3, §12 `TEACH_ME_USER_STORAGE_QUOTA_BYTES`); `details`: `[{ "field": "quota", "issue": "exceeded" }]` (aggiornamento 2026-11-22, audit C8/M1) |
| 409 | `already_reviewed` | voto su una scheda la cui versione è cambiata (`reviews.md` §4) |
| 409 | `card_suspended` | voto su una scheda sospesa (`reviews.md` §4) |
| 409 | `nothing_to_undo` | annullamento senza un voto da annullare (`reviews.md` §5) |
| 409 | `undo_expired` | annullamento di un voto oltre i 10 minuti o di un modello precedente (`reviews.md` §5) |
| 409 | `card_superseded` | riattivazione di una scheda superata da un record nuovo (`reviews.md` §3.2) |
| 413 | `payload_too_large` | corpo oltre il limite (default 1 MiB; 4 MiB per le note a mano, `lessons.md` §11; 50 MiB per l'upload dei documenti, `documents.md`) |
| 415 | `unsupported_media_type` | il file caricato non è del tipo atteso (es. non è un PDF, `documents.md`) |
| 416 | `range_not_satisfiable` | `Range` oltre la fine di una clip audio (`audio.md` §5, header `Content-Range: bytes */<dimensione>`) |
| 422 | `validation_failed` | JSON valido ma regole di dominio violate |
| 429 | `rate_limited` | limiti del provider LLM o del backend; troppi accessi falliti; apertura di uno stream SSE oltre il limite per utente (§12, `TEACH_ME_MAX_SSE_PER_USER`) (aggiornamento 2026-11-22, audit C8/M1) (header `Retry-After`; per l'apertura di uno stream SSE oltre il limite, `Retry-After: 5`) (aggiornamento 2026-11-22, audit C8) |
| 429 | `llm_budget_exceeded` | il budget giornaliero di token LLM dell'utente è esaurito (§12, `TEACH_ME_USER_DAILY_LLM_TOKENS`); alla creazione di una richiesta o di un job, header `Retry-After` = secondi fino alle 00:00 UTC; un job creato prima che parte a budget esaurito termina `failed` con questo codice, senza header (`jobs.md` §2) (aggiornamento 2026-11-22, audit C8/M1; chiarimento audit C8) |
| 500 | `internal` | errore inatteso (dettagli solo nei log) |
| 502 | `upstream_failed` | errore di un servizio esterno (LLM, ricerca) |
| 502 | `llm_key_rejected` | il provider LLM rifiuta la chiave dell'utente (`users.md` §3) |
| 502 | `llm_credit_exhausted` | il provider LLM accetta la chiave ma il credito è esaurito (`users.md` §3, dalla v1.3) |
| 503 | `unavailable` | backend in avvio o in manutenzione, oppure richiesta oltre il timeout del §12 |

Nuovi `code` si aggiungono a questa tabella. L'app tratta un `code` sconosciuto in base alla sola classe HTTP.

## 8. Paginazione

Tutte le liste usano la paginazione a **cursore**, anche quelle piccole (così il contratto non cambia quando crescono):

`GET /api/v1/folders?limit=50&cursor=<opaco>`

```json
{
  "items": [ { "...": "..." } ],
  "next_cursor": "eyJpZCI6Ij..."
}
```

- `limit`: default 50, massimo 200. Fuori intervallo o parametri di query non validi → `400 bad_request` nel formato del §7.
- `cursor`: stringa opaca restituita dalla pagina precedente; il client non la interpreta. `next_cursor: null` = ultima pagina.
- L'ordinamento di ogni lista è stabile e documentato nel contratto dell'endpoint.

## 9. Job (operazioni lunghe)

Ricerca delle fonti, generazione delle lezioni e chiusura della lezione girano come job asincroni. Risorsa, endpoint ed eventi: **`jobs.md`** (v1).

## 10. Eventi SSE

Endpoint: `GET /api/v1/topics/{id}/events` (contratto dettagliato in `chat.md` §4). Formato di ogni evento:

```
id: 42
event: job_updated
data: {"type":"job_updated","topic_id":"...","at":"2026-11-08T14:36:09.502Z","payload":{...}}

```

- `event` e `data.type` coincidono. Nomi in `snake_case`, al passato o come stato (`message_delta`, `message_completed`, `job_updated`, `mission_proposed`).
- `data` è sempre un oggetto JSON su una riga con `type`, `topic_id`, `at`, `payload`.
- `id`: intero crescente per Topic. Alla riconnessione il client invia `Last-Event-ID` e il backend rimanda gli eventi successivi ancora in buffer (finestra minima: ultimi 500 eventi o 10 minuti). Se l'id è fuori finestra, il backend invia un evento `resync_required` e il client ricarica lo stato via REST.
- Heartbeat: una riga di commento `: ping` ogni 15 secondi, per tenere aperta la connessione su reti mobili.
- L'app ignora gli eventi di tipo sconosciuto.

## 11. Header

| Header | Direzione | Scopo |
|---|---|---|
| `Authorization: Bearer <token>` | richiesta | autenticazione |
| `X-Request-Id` | entrambe | se il client lo invia (UUID) il backend lo riusa, altrimenti lo genera; sempre presente in risposta e nei log |
| `X-App-Version` | richiesta | versione dell'app (es. `0.1.0`), per log e per la compatibilità del catalogo SDUI |
| `Location` | risposta | URL della risorsa creata (201) |
| `Accept-Language` | richiesta | opzionale: lingua del `message` degli errori (`en` se la prima lingua supportata è l'inglese, altrimenti italiano; §7, ADR 0019) |

## 12. Limiti e timeout

- Corpo massimo delle richieste: 1 MiB (configurabile con `TEACH_ME_BODY_LIMIT_BYTES`, tabella sotto).
- Timeout delle richieste REST lato backend: 30 secondi (configurabile con `TEACH_ME_REQUEST_TIMEOUT_SECS`, tabella sotto). Le operazioni che possono superarlo sono job.
- Lato app: connect timeout 10 s, request timeout 30 s; nessun timeout di lettura sullo stream SSE (c'è l'heartbeat).

### Limiti configurabili (aggiornamento 2026-11-22, audit C8/M1)

Variabili d'ambiente del backend:

| Variabile | Default | Significato |
|---|---|---|
| `TEACH_ME_USER_STORAGE_QUOTA_BYTES` | `1073741824` (1 GiB) | byte totali dei documenti caricati da un utente; `0` = illimitato (`documents.md` §3, `409 storage_quota_exceeded` in §7); due caricamenti in parallelo possono superare la quota di al più un file (aggiornamento 2026-11-22) |
| `TEACH_ME_USER_DAILY_LLM_TOKENS` | `0` (disabilitato) | token LLM per utente per giorno UTC: si contano tutti i token riportati dal provider per la chiamata, cioè input + output + cache write + cache read; una chiamata che fallisce prima che il provider riporti l'uso non conta nulla; il controllo agisce solo sull'avvio di nuove richieste, quindi richieste concorrenti possono superare il budget di poco (aggiornamento 2026-11-22); a budget già esaurito, la creazione di una richiesta o di un job che usa l'LLM dell'utente risponde `429 llm_budget_exceeded` con header `Retry-After` (§7); un job creato prima, che parte a budget esaurito, termina `failed` con `error.code` `llm_budget_exceeded`, senza header HTTP (`jobs.md` §2); un job già in esecuzione può terminare normalmente |
| `TEACH_ME_MAX_SSE_PER_USER` | `8` | stream SSE contemporanei per utente; oltre il limite `429 rate_limited` |
| `TEACH_ME_BODY_LIMIT_BYTES` | `1048576` (1 MiB) | corpo massimo di una richiesta, in byte (§12, `413 payload_too_large` in §7) |
| `TEACH_ME_REQUEST_TIMEOUT_SECS` | `30` | timeout delle richieste REST lato backend, in secondi (§12) |
| `TEACH_ME_JOB_MAX_CONCURRENT` | `2` | job in esecuzione in contemporanea sull'intero server, somma di tutti gli utenti (`jobs.md`) |
| `TEACH_ME_JOB_TIMEOUT_SECS` | `900` | durata massima di un job, in secondi (`jobs.md`, codice d'errore `timeout`) |
