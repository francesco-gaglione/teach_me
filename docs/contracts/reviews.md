# Contratto: Ripasso FSRS

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1.5** (2026-11-20, filtro per argomento del ripasso). v1.4 (2026-11-18, issue #29: schede da consolidare). v1.3 (2026-11-16, issue #12: motivo della sospensione, riattivazione, lista delle sospese, `409` distinti). v1.2 (2026-11-14, issue #11: FSRS-7, passi brevi, pesi personali; ADR 0014). v1.1 (2026-11-10), Fase 8.
> **v1.5:** `GET /reviews/due` accetta `topic_id` (§3.1) e la risposta ha il campo nuovo `topics`. Modifica additiva.
> **v1.4:** `ReviewCard` ha un campo nuovo, `to_consolidate` (§1.3, §2). Modifica additiva.
> **v1.3:** `ReviewCard` ha due campi nuovi (`suspension`, `suspended_at`); nuovi `POST /reviews/{card_id}/unsuspend` (§3.2) e `GET /reviews/suspended` (§3.3); i `409` di voto e annullamento hanno `code` distinti al posto di `conflict` (§4, §5).
> **v1.2:** cambiano solo le regole di scheduling (§1, §1.1, §1.2) e un caso di `409` dell'annullamento (§5). La forma di richieste e risposte non cambia.
> **Fase 11 (v1.1):** `GET /reviews/due`, i conteggi e ogni operazione sulle schede riguardano solo le schede dell'utente della sessione; una scheda altrui risponde `404`.
> Valgono `api-conventions.md` e `lessons.md` (le schede nascono dal Recorder, §6.1-6.2). Decisione D4 (ADR 0005): FSRS da un crate esistente dietro il port `ReviewScheduler` (ADR 0006, sostituita da ADR 0014 per FSRS-7). Spec visiva: `docs/design/screens/review.md`. I path omettono il prefisso `/api/v1`.

## 1. Concetti

- Una **scheda** (`ReviewCard`, fronte/retro) nasce alla chiusura di una lezione, legata a un learning record o a un termine del glossario. Il ripasso vale per **tutti i Topic insieme** per default; dalla v1.5 si può limitare a un solo Topic (§3.1).
- Lo scheduling è **FSRS-7** (v1.2; prima FSRS-5), retention desiderata **0,9**, intervallo massimo 36.500 giorni, con i **passi brevi** del §1.1 e i **pesi** dell'utente (§1.2). **Niente fuzz**: gli intervalli mostrati prima del voto devono essere quelli applicati.
- Le scadenze sono **istanti esatti** (giorni frazionari), non arrotondati al giorno: "Bene" può valere 4 giorni e 18 ore.
- Una scheda nuova è in scadenza **un giorno dopo** la sua creazione (`created_at` + 24 h): ripassare subito dopo la lezione non serve.
- Una scheda è **in scadenza** se è `active` e `due` ≤ adesso (orologio del backend, port `Clock`).
- Una scheda il cui record viene sostituito (`superseded`) è **sospesa** dal backend (`lessons.md` §6.2) con motivo `superseded`; l'utente può sospenderne una a mano (§3) con motivo `user`, e riattivarla (§3.2). Una scheda sospesa non è mai in scadenza. Una scheda `superseded` non si riattiva: la sua idea è stata sostituita da un record nuovo, che ha le sue schede.
- Ogni voto scrive un **`ReviewLog`** (voto, istante, stato FSRS prima e dopo): serve per annullare l'ultimo voto e per le statistiche future.

### 1.1 Passi brevi (v1.2)

Come ts-fsrs v6. `M` è l'intervallo del modello per il voto, `G` = 1 giorno. Passi: `1 min, 10 min` per le schede `new` e `learning`, `10 min` per `review` e `relearning`. Ogni scheda ha un indice di passo `i` (0 all'inizio).

| Voto | Passo (se esiste) | Intervallo |
|---|---|---|
| Non la ricordavo | il primo passo; `i` torna a 0. Esiste sempre se la lista non è vuota (anche per una scheda `review`) | max(passo, min(M, G)) |
| Con fatica | solo per `new`, `learning`, `relearning`: media dei primi due passi (5,5 min), oppure 1,5 × il passo se ce n'è uno solo; `i` resta | max(passo, M) |
| Bene | solo per `new`, `learning`, `relearning` e se esiste il passo `i+1`: quel passo; `i` avanza | max(passo, M) |
| Facilissima | mai | — |
| Senza passo (`Con fatica`, `Bene`, `Facilissima`) | — | max(M, G) |

Stato dopo il voto: se l'intervallo è minore di `G` (o il voto è "Non la ricordavo"), una scheda `new` diventa `learning`, una `review` diventa `relearning`, le altre restano; altrimenti la scheda è `review` e `i` torna a 0. Con i pesi di default, su una scheda nuova: 1 min, ~14 ore, ~4,8 giorni, ~54 giorni.

### 1.2 Pesi personali (v1.2)

- Ogni utente parte dai pesi di default di FSRS-7. Quando ha almeno **300 voti utili** (voti non annullati preceduti da un altro voto della stessa scheda), il backend prova ad addestrare pesi suoi, poi al massimo una volta ogni **30 giorni** dall'ultimo tentativo (7 giorni se l'ultimo è fallito per un errore) se nel frattempo è stato salvato almeno un voto utile nuovo.
- I pesi nuovi si adottano solo se prevedono meglio (`log_loss` strettamente minore) di quelli in uso sul 20% più recente dei voti utili (arrotondato per difetto), escluso dall'addestramento (ADR 0014 §11).
- Pesi salvati che il backend non riesce a usare valgono come pesi di default.
- Quando cambiano i pesi le scadenze già fissate restano; anteprime e voti successivi usano subito i pesi nuovi. Una scheda la cui memoria è stata calcolata con altri pesi (o con FSRS-5) viene ricalcolata dallo storico prima dell'anteprima e del voto.
- Nessun endpoint: l'ottimizzazione è interna al backend.

### 1.3 Schede da consolidare (v1.4)

- Una scheda nasce **da consolidare** quando il Recorder la lega a un esercizio della lezione mai superato (`lessons.md` §6.1, `unpassed_block`, controllato dal backend). Mette alla prova un punto che la persona ha sbagliato e non ha ancora corretto. Le altre schede (e tutte quelle nate prima della v1.4) non lo sono.
- Smette di esserlo, e diventa una scheda normale, quando tra i suoi voti **non annullati**, in ordine di applicazione, ce ne sono **due consecutivi** `good` o `easy`. Un `again` o `hard` interrompe la serie. Una volta normale resta normale anche dopo voti successivi peggiori; l'annullamento di uno dei due voti la fa tornare da consolidare.
- Lo scheduling non cambia: è un'informazione per la persona, non per FSRS.

## 2. Risorsa `ReviewCard`

```json
{
  "id": "01933b4e-...",
  "topic_id": "01933b4e-...",
  "topic_title": "Rust",
  "lesson_id": "01933b4e-...",
  "lesson_number": 2,
  "lesson_title": "Result e operatore ?",
  "kind": "record",
  "to_consolidate": false,
  "front": "Quando compila l'operatore `?` dentro una funzione?",
  "back": "Solo se il tipo di ritorno può contenere l'errore, per esempio `Result<T, E>`.",
  "status": "active",
  "suspension": null,
  "suspended_at": null,
  "state": "review",
  "due": "2026-11-12T09:00:00.000Z",
  "last_reviewed_at": "2026-11-10T09:00:00.000Z",
  "review_count": 3,
  "intervals_preview": { "again": 600, "hard": 172800, "good": 432000, "easy": 950400 }
}
```

| Campo | Regole |
|---|---|
| `kind` | `record` \| `term`: origine della scheda. L'app tratta valori sconosciuti come generici |
| `to_consolidate` | (v1.4) `true` se la scheda è da consolidare adesso (§1.3), altrimenti `false`. Vale anche nelle schede sospese e nelle risposte di voto e annullamento |
| `front`, `back` | *md* inline (`sdui.md` §3) |
| `status` | `active` \| `suspended` |
| `suspension` | (v1.3) motivo della sospensione: `user` (a mano, §3) \| `superseded` (record sostituito, §1); `null` se `active`. Se una scheda sospesa a mano viene poi superata, diventa `superseded`. L'app tratta valori sconosciuti come `superseded` (non riattivabile) |
| `suspended_at` | (v1.3) istante dell'ultima sospensione; `null` se `active`. Per le schede sospese prima della v1.3 vale l'istante della migrazione |
| `state` | stato FSRS: `new` \| `learning` \| `review` \| `relearning`. L'app tratta valori sconosciuti come `review` |
| `due` | prossima scadenza |
| `last_reviewed_at` | `null` se mai ripassata |
| `review_count` | numero di voti applicati (annullati esclusi). È anche la **versione** della scheda per l'idempotenza (§4) |
| `intervals_preview` | secondi tra il voto e la nuova scadenza, per ciascun voto, calcolati **adesso** con lo stesso scheduler che applicherà il voto. `null` se la scheda è `suspended` |

## 3. Endpoint

| Metodo | Path | Successo | Errori |
|---|---|---|---|
| GET | `/reviews/due` | `200` `DueReviews` (§3.1) | `400 bad_request`, `404` (v1.5, `topic_id`) |
| POST | `/reviews/{card_id}` | `200` la `ReviewCard` aggiornata (§4) | `400 invalid_id`, `404`, `409 already_reviewed`, `409 card_suspended`, `422 validation_failed` |
| POST | `/reviews/{card_id}/undo` | `200` la `ReviewCard` riportata allo stato prima dell'ultimo voto | `400 invalid_id`, `404`, `409 nothing_to_undo`, `409 undo_expired` (§5) |
| POST | `/reviews/{card_id}/suspend` | `200` la `ReviewCard` con `status: suspended` (idempotente) | `400 invalid_id`, `404` |
| POST | `/reviews/{card_id}/unsuspend` | (v1.3) `200` la `ReviewCard` con `status: active` (§3.2, idempotente) | `400 invalid_id`, `404`, `409 card_superseded` |
| GET | `/reviews/suspended` | (v1.3) `200` `SuspendedReviews` (§3.3) | `400 bad_request` |

### 3.1 `DueReviews`

`GET /reviews/due?limit=50&cursor=…&topic_id=…` — lista paginata (`api-conventions.md` §8) delle schede in scadenza, ordinate per `due` crescente, poi `id`. L'envelope ha quattro campi in più (`topics` dalla v1.5):

```json
{
  "items": [ { "...": "ReviewCard" } ],
  "next_cursor": null,
  "due_count": 12,
  "estimated_minutes": 3,
  "next_due_at": "2026-11-10T09:00:00.000Z",
  "topics": [ { "topic_id": "01933b4e-...", "topic_title": "Rust", "due_count": 8 } ]
}
```

- `topic_id` (v1.5, facoltativo, UUID): limita `items`, `due_count`, `estimated_minutes` e `next_due_at` alle schede di quel Topic. Senza `topic_id` tutto come prima.
- `topic_id` malformato → `400 bad_request`. Topic inesistente o di un altro utente → `404 not_found` (stesso isolamento della Fase 11).
- Un `cursor` vale solo con lo stesso `topic_id` con cui è stato prodotto. Usarlo con un altro filtro è un errore del client: il backend può rispondere `400 bad_request`.
- `topics` (v1.5): un elemento per ogni Topic dell'utente con almeno una scheda in scadenza adesso, con `due_count` = le sue schede in scadenza. Ordinato per `due_count` decrescente, poi `topic_title` crescente, poi `topic_id`. È calcolato **sempre su tutte le schede dell'utente, ignorando `topic_id`**, così l'app mostra i filtri stabili. Array vuoto se non c'è nulla in scadenza.
- Per il conteggio di un solo argomento basta `GET /reviews/due?limit=1&topic_id=…`.
- `due_count`: totale delle schede in scadenza adesso (non solo della pagina; con `topic_id`, solo di quel Topic).
- `estimated_minutes`: `ceil(due_count × 15 s / 60)`, 0 se non ce ne sono.
- `next_due_at`: la prima scadenza **futura** tra le schede `active` (per "La prossima scheda arriva domani"), `null` se non ce ne sono.
- Per il badge della Home basta `GET /reviews/due?limit=1`.

Sospendere a mano (`POST /reviews/{card_id}/suspend`) una scheda `active` la porta a `suspension: user` e fissa `suspended_at`. Su una scheda già sospesa non cambia nulla (né motivo né istante).

### 3.2 Riattivare (v1.3)

`POST /reviews/{card_id}/unsuspend`, senza corpo.

- Scheda con `suspension: user` → `status: active`, `suspension` e `suspended_at` a `null`. Stato FSRS, `due`, `review_count` e storico restano quelli di prima della sospensione: se `due` è già passata la scheda è subito in scadenza (FSRS gestisce il ritardo). Si ricalcola solo `intervals_preview`.
- Scheda già `active` → `200` senza modifiche (idempotente).
- Scheda con `suspension: superseded` → `409 card_superseded`.

### 3.3 `SuspendedReviews` (v1.3)

`GET /reviews/suspended?limit=50&cursor=…`: lista paginata (`api-conventions.md` §8) delle schede dell'utente con `suspension: user`, ordinate per `suspended_at` **decrescente** (le più recenti prima), poi `id`. Le schede `superseded` non compaiono. Le `ReviewCard` hanno `intervals_preview: null`. L'envelope ha un campo in più:

```json
{ "items": [ { "...": "ReviewCard" } ], "next_cursor": null, "suspended_count": 3 }
```

- `suspended_count`: totale delle schede sospese a mano (non solo della pagina). Per sapere se mostrare l'accesso alla lista basta `GET /reviews/suspended?limit=1`.

## 4. Voto

```json
POST /reviews/{card_id}
{ "rating": "good", "reviewed_at": "2026-11-10T09:00:05.000Z", "review_count": 3 }
```

- `rating`: `again` \| `hard` \| `good` \| `easy` (in app: "Non la ricordavo", "Con fatica", "Bene", "Facilissima").
- `reviewed_at` (facoltativo, assente = adesso): quando l'utente ha votato (orologio dell'app; serve alla coda offline). Il backend lo limita a [ultimo voto della scheda, oppure `created_at` se mai votata; adesso]: mai nel futuro, mai prima del voto precedente.
- `review_count`: il valore che l'app ha letto. Se non coincide con quello attuale il voto è già stato applicato (reinvio della coda) o la scheda è cambiata: `409 already_reviewed` (v1.3; prima `conflict`), e l'app scarta il voto e ricarica.
- Una scheda `suspended` → `409 card_suspended` (v1.3; prima `conflict`). Se la scheda è sospesa **e** la versione non coincide vale `card_suspended`. Una scheda non ancora in scadenza si può votare comunque (ripasso anticipato): FSRS lo gestisce.
- Il backend calcola il nuovo stato con `ReviewScheduler`, aggiorna la scheda e scrive il `ReviewLog` in una transazione.

## 5. Annulla

`POST /reviews/{card_id}/undo` ripristina lo stato salvato nell'ultimo `ReviewLog` della scheda (stato FSRS, `due`, `last_reviewed_at`, `review_count`) e lo segna come annullato. Si annulla solo l'ultimo voto, entro 10 minuti **da quando il backend lo ha applicato** (non da `reviewed_at`). Nessun voto da annullare (mai votata, oppure un secondo undo) → `409 nothing_to_undo`; voto oltre i 10 minuti, oppure il cui stato salvato è di un modello precedente (voto dato prima del passaggio a FSRS-7, v1.2) → `409 undo_expired` (v1.3; prima entrambi `conflict`). L'undo non cambia lo `status` (una scheda sospesa nel frattempo resta sospesa).

## 6. Eventi

Nessuno: il ripasso non è legato a un Topic e l'app ricarica `GET /reviews/due` quando apre Home o la schermata Ripasso.

## 7. Fuori da questa versione

Modificare una scheda, riattivare una scheda superata, statistiche di ripasso, notifiche (vedi `ADR 0012`). L'ottimizzazione dei pesi sui voti personali è entrata nella v1.2 (§1.2); la riattivazione delle schede sospese a mano nella v1.3 (§3.2); le schede da consolidare nella v1.4 (§1.3).
