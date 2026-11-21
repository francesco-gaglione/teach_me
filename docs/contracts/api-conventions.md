# Convenzioni delle API

> Contratto condiviso tra backend (`backend/`) e app (`android/`). Proprietario: tech lead.
> Versione: **v1.3** (2026-11-11, Fase 12: `502 llm_credit_exhausted`; v1.2 2026-11-10, Fase 11: accesso con nome utente e password, isolamento per utente; v1.1 Fase 10: `415`, limite di upload). Ogni endpoint specifico (cartelle, topic, chat...) ha il suo file in `docs/contracts/` e rispetta queste regole.

## 1. Trasporto e versionamento

- Tutte le API applicative stanno sotto il prefisso **`/api/v1`**. Gli esempi nei contratti omettono il prefisso solo se esplicitamente indicato.
- Una modifica incompatibile (campo rimosso o rinominato, semantica cambiata) richiede `/api/v2`. Aggiungere campi opzionali, endpoint o valori di enum **non** è incompatibile: l'app deve ignorare i campi sconosciuti e tollerare valori di enum sconosciuti (mappandoli su un valore `Unknown`).
- Corpo delle richieste e delle risposte: `application/json; charset=utf-8`, salvo SSE (`text/event-stream`).
- Nomi dei campi JSON in **`snake_case`**. Valori di enum in `snake_case` (es. `"mission_proposed"`).
- Un campo opzionale senza valore si serializza come `null` (non viene omesso) nelle risposte. Nelle richieste il client può ometterlo.

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

Prima dell'accesso l'app verifica l'indirizzo con `GET /health` (senza autenticazione): timeout, DNS, connessione rifiutata → "backend irraggiungibile"; altro → "errore imprevisto" con il codice.

## 4. Identificatori

- Tutti gli id sono **UUID v7** (ordinati nel tempo), serializzati come stringa minuscola con trattini: `"01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f"`.
- Gli id li genera sempre il backend (port `IdGenerator`). Un id malformato in un path → `400 invalid_id`.
- Gli id dei blocchi SDUI (`block.id`) sono un'eccezione: stringhe stabili scelte dal Generator, uniche nella lezione (vedi `sdui.md`, fase 5).

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
| 409 | `already_reviewed` | voto su una scheda la cui versione è cambiata (`reviews.md` §4) |
| 409 | `card_suspended` | voto su una scheda sospesa (`reviews.md` §4) |
| 409 | `nothing_to_undo` | annullamento senza un voto da annullare (`reviews.md` §5) |
| 409 | `undo_expired` | annullamento di un voto oltre i 10 minuti o di un modello precedente (`reviews.md` §5) |
| 409 | `card_superseded` | riattivazione di una scheda superata da un record nuovo (`reviews.md` §3.2) |
| 413 | `payload_too_large` | corpo oltre il limite (default 1 MiB; 4 MiB per le note a mano, `lessons.md` §11; 50 MiB per l'upload dei documenti, `documents.md`) |
| 415 | `unsupported_media_type` | il file caricato non è del tipo atteso (es. non è un PDF, `documents.md`) |
| 416 | `range_not_satisfiable` | `Range` oltre la fine di una clip audio (`audio.md` §5, header `Content-Range: bytes */<dimensione>`) |
| 422 | `validation_failed` | JSON valido ma regole di dominio violate |
| 429 | `rate_limited` | limiti del provider LLM o del backend; troppi accessi falliti (header `Retry-After`) |
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

Ricerca delle fonti, generazione delle lezioni e chiusura della lezione girano come job asincroni. Risorsa, endpoint ed eventi: **`jobs.md`** (v1, Fase 4), che sostituisce l'esempio provvisorio che stava qui.

## 10. Eventi SSE

Endpoint: `GET /api/v1/topics/{id}/events` (contratto dettagliato in fase 2). Formato di ogni evento:

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
| `X-App-Version` | richiesta | versione dell'app (es. `0.1.0`), per log e, dalla fase 5, per la compatibilità del catalogo SDUI |
| `Location` | risposta | URL della risorsa creata (201) |
| `Accept-Language` | richiesta | opzionale: lingua del `message` degli errori (`en` se la prima lingua supportata è l'inglese, altrimenti italiano; §7, ADR 0019) |

## 12. Limiti e timeout

- Corpo massimo delle richieste: 1 MiB (configurabile).
- Timeout delle richieste REST lato backend: 30 secondi. Le operazioni che possono superarlo sono job.
- Lato app: connect timeout 10 s, request timeout 30 s; nessun timeout di lettura sullo stream SSE (c'è l'heartbeat).
