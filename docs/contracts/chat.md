# Contratto: Chat con il Tutor ed eventi SSE

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1.3** (2026-11-18).
> **v1.3:** nuova parte `lesson_request` (lezione richiesta in chat, `lessons.md` §12).
> **v1.2:** un messaggio fallito può avere `error.code` `llm_credit_exhausted` (`users.md` §3); lo snapshot `mission_proposal` può avere `starting_point` (`mission.md` §8).
> **v1.1:** l'invio e il retry di un messaggio senza chiave LLM rispondono `409 llm_key_missing` senza creare nulla; un messaggio fallito può avere `error.code` `llm_key_rejected` o `llm_key_missing` (`users.md` §3). Messaggi ed eventi sono visibili solo al proprietario dell'argomento.
> Valgono `api-conventions.md` (prefisso `/api/v1`, errori, paginazione, date, id, §10 eventi SSE) e `folders-topics.md`. I path sotto omettono il prefisso.

## 1. Risorsa `ChatMessage`

```json
{
  "id": "01933b4e-...",
  "topic_id": "01933b4e-...",
  "role": "assistant",
  "status": "complete",
  "parts": [
    { "type": "text", "text": "Ciao! Perché vuoi imparare Rust?" }
  ],
  "error": null,
  "created_at": "2026-11-08T14:36:01.129Z",
  "last_event_id": 1762612561129042,
  "updated_at": "2026-11-08T14:36:04.877Z"
}
```

| Campo | Tipo | Note |
|---|---|---|
| `role` | enum | `user` · `assistant`. (I messaggi di sistema e i tool call interni non sono esposti.) |
| `status` | enum | `streaming` (risposta in generazione) · `complete` · `failed`. I messaggi `user` sono sempre `complete` |
| `parts` | array | Contenuto ordinato. In v1 c'è solo `{"type": "text", "text": ...}`; si aggiungono parti native (es. `mission_proposal`; dalla v1.3 `lesson_request`, `lessons.md` §12; dalla v1.2 il suo snapshot della mission può avere `starting_point`, assente nei messaggi più vecchi: `mission.md` §8). L'app **ignora le parti di tipo sconosciuto** (le salta, non crasha). Un messaggio `streaming` contiene il testo accumulato fino a quel momento |
| `error` | object \| null | Solo con `status: failed`: `{ "code", "message" }` come in `api-conventions.md` §7 (senza `request_id`, `details`). Codici tipici: `upstream_failed`, `rate_limited`, `internal`, `llm_key_rejected`, `llm_key_missing`, `llm_credit_exhausted` (dalla v1.2) |
| `error.message` | string | Il testo di `message` è nella lingua dell'argomento della chat (ADR 0019). |
| `last_event_id` | int \| null | Id dell'ultimo evento SSE il cui contenuto è già riflesso in questo messaggio (per uno `streaming`: l'ultimo `message_delta` incluso in `parts`, oppure il `message_created` se `parts` è vuoto). `null` solo per messaggi creati prima di questo campo. Serve al client per non applicare due volte lo stesso delta (§4) |

## 2. Endpoint

| Metodo | Path | Successo | Errori specifici |
|---|---|---|---|
| GET | `/topics/{id}/messages` | `200` lista paginata di `ChatMessage` | `400`, `404` (Topic) |
| POST | `/topics/{id}/messages` | `202` `SendMessageResponse` | `400 invalid_id`, `404`, `409 conflict`, `422` |
| POST | `/topics/{id}/messages/{message_id}/retry` | `202` `ChatMessage` (l'assistant rimesso in `streaming`) | `400 invalid_id`, `404`, `409 conflict` |
| GET | `/topics/{id}/events` | `200` `text/event-stream` (§4) | `400 invalid_id`, `404` |

### Lista dei messaggi

- Ordine: **dal più recente al più vecchio** (`created_at` decrescente, poi `id` decrescente). La prima pagina contiene gli ultimi messaggi; `next_cursor` porta ai più vecchi (scroll verso l'alto). L'app li mostra invertiti (più vecchio in alto).
- Paginazione a cursore standard (default 50, max 200).
- Header di risposta **`X-Last-Event-Id`**: l'ultimo id di evento emesso per il Topic, letto **prima** di interrogare i messaggi (quindi tutto ciò che è successo fino a quell'id è riflesso nella risposta, salvo i delta dei messaggi `streaming`, coperti da `last_event_id`). Se il Topic non ha ancora eventi in questo run vale la **base del run** (vedi §4): non è mai `0`.

### Invio di un messaggio

```json
POST /topics/{id}/messages
{ "text": "Voglio imparare Rust per lavoro" }
```

- `text`: trim; 1-8000 caratteri (code point). Errori `422` con `details` come in `folders-topics.md` §3: `{field: "text", issue: "empty" | "too_long"}`.
- `lesson_id` (facoltativo): la chat è stata aperta da una lezione (`lessons.md` §7).
- Il backend salva subito il messaggio `user`, crea il messaggio `assistant` in stato `streaming` (con `parts: []`) e avvia la generazione **in background**: la generazione continua anche se l'app si disconnette.
- Risposta `202`:

```json
{
  "user_message": { "...": "ChatMessage, role user, status complete" },
  "assistant_message": { "...": "ChatMessage, role assistant, status streaming, parts []" }
}
```

- **Un solo turno alla volta per Topic**: se c'è già un messaggio `assistant` in `streaming` → `409 conflict`.
- I due casi di `409 conflict` della chat (turno già in corso; messaggio non riprovabile) hanno lo stesso `code`; l'app li distingue dall'endpoint chiamato.
- Primo messaggio in un Topic `new` → il Topic passa a `interviewing` (evento `topic_updated`, `docs/mvp.md` §5). Nessun'altra transizione di stato.

### Riprova

- Si può ritentare solo un messaggio `assistant` in stato `failed` **ed è l'ultimo del Topic**; altrimenti `409 conflict`. Il backend azzera `parts` ed `error`, lo rimette in `streaming` e rigenera la risposta allo stesso contesto (il messaggio mantiene lo stesso `id`) ed emette `message_created` con quel messaggio: **il client applica `message_created` come upsert per `id`** (sostituisce un messaggio esistente con lo stesso id).
- Se il backend si riavvia durante uno streaming, all'avvio i messaggi rimasti `streaming` diventano `failed` con `code: "internal"` (si possono ritentare).

## 3. Eventi della chat

Tutti gli eventi seguono `api-conventions.md` §10 (`id` crescente per Topic, `event` = `data.type`, `data` = `{type, topic_id, at, payload}`).

| `type` | `payload` | Quando |
|---|---|---|
| `message_created` | `{ "message": ChatMessage }` | Creato un messaggio (sia `user` sia `assistant` in `streaming`). Utile se lo stesso Topic è aperto altrove |
| `message_delta` | `{ "message_id": "...", "part_index": 0, "text": "Perché " }` | Frammento di testo da **appendere** alla parte `part_index` (di tipo `text`) del messaggio. Se la parte non esiste ancora la si crea |
| `message_completed` | `{ "message": ChatMessage }` | Generazione finita: il messaggio completo e definitivo (`status: complete`). L'app **sostituisce** il contenuto accumulato con questo |
| `message_failed` | `{ "message": ChatMessage }` | Generazione fallita (`status: failed`, `error` valorizzato). Il testo parziale resta in `parts` |
| `topic_updated` | `{ "topic": Topic }` | Il Topic è cambiato (es. stato `new` → `interviewing`). Stessa forma di `folders-topics.md` |
| `resync_required` | `{}` | `Last-Event-ID` fuori dalla finestra di buffer: l'app ricarica messaggi e Topic via REST. L'evento ha come `id` l'ultimo id emesso per il Topic: il client lo adotta come posizione corrente |
| `topic_updated` (da `PATCH /topics/{id}`) | `{ "topic": Topic }` | Emesso anche quando il Topic viene rinominato o spostato (solo se qualcosa cambia) |

Regole:
- Ordine garantito per Topic: `message_created` → zero o più `message_delta` → `message_completed` o `message_failed`.
- I `message_delta` di un messaggio arrivano nell'ordine di generazione; il client li applica in ordine di `id` dell'evento.
- Dopo `message_completed`/`message_failed` non arrivano altri delta per quel messaggio.
- L'app ignora eventi di tipo sconosciuto.

## 4. Stream SSE

`GET /topics/{id}/events` con `Accept: text/event-stream`, header `Authorization` come ogni API.

- **Apertura** (procedura del client): (1) `GET /topics/{id}/messages` → messaggi + `X-Last-Event-Id` = P; (2) posizione di partenza = il minimo tra P e i `last_event_id` dei messaggi `streaming` presenti; (3) apre lo stream con `Last-Event-ID` = quella posizione, così il backend rimanda dal buffer tutto ciò che il REST potrebbe non riflettere. Gli eventi rimandati sono idempotenti: `message_created` è un upsert, `message_completed`/`message_failed` sostituiscono, e **un `message_delta` si applica solo se il suo id è maggiore dell'ultimo evento già riflesso in quel messaggio** (`last_event_id` dal REST, poi aggiornato dal client a ogni delta applicato). Se la posizione è fuori finestra arriva `resync_required` e si ripete da (1).
- Alla connessione **senza** `Last-Event-ID` il backend invia solo eventi futuri (uso non previsto dall'app, utile per strumenti e test).
- Alla riconnessione **con** `Last-Event-ID: <n>`: il backend rimanda gli eventi con id > n ancora in buffer (finestra minima: ultimi 500 eventi **o** 10 minuti, per Topic), poi prosegue con quelli nuovi. Se n è fuori finestra (o il backend è stato riavviato e il buffer è vuoto), invia per primo `resync_required`.
- Gli `id` degli eventi sono interi (64 bit, < 2^53) crescenti **per Topic e monotoni anche tra riavvii del backend**: a ogni avvio la numerazione riparte da una base derivata dall'istante di avvio (millisecondi Unix × 1000), sempre maggiore di qualunque id del run precedente. Un `Last-Event-ID` minore del primo id ancora in buffer, maggiore dell'ultimo emesso, non numerico o comunque non riconosciuto → `resync_required`. Un header vuoto equivale ad assente. Un `Last-Event-ID` **uguale** all'ultimo id emesso (o alla base del run, se il Topic non ha ancora eventi) è valido: nessun replay e nessun `resync_required`.
- Heartbeat: commento `: ping` ogni 15 s.
- Nessun timeout lato backend sullo stream (escluso dal timeout REST di 30 s). Il client si riconnette con backoff (1 s, 2 s, 5 s, poi ogni 10 s) finché la schermata del Topic è visibile; chiude lo stream quando non lo è.
- Più client possono essere connessi allo stesso Topic.

## 5. Contesto inviato al modello (informativo)

Non fa parte del contratto HTTP, ma fissa l'intento: il Tutor riceve dal Context Assembler il prompt di sistema del ruolo, i dati del Topic (titolo, stato) e la cronologia della chat (troncata ai messaggi più recenti entro un budget di token). I messaggi `failed` e l'assistant in `streaming` non entrano nella cronologia inviata al modello (dettagli in `backend/prompts/llm-port.md` §10, decisioni in `docs/adr/0004-llm-provider-port.md`).

Il testo del Tutor usa il sottoinsieme di markdown definito in `docs/design/screens/chat.md` (paragrafi, grassetto, corsivo, codice inline e a blocchi, elenchi fino a 2 livelli, link `https`); il backend lo inoltra così com'è, l'app lo rende. Il testo dell'utente è letterale.
