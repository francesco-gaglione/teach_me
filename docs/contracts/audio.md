# Contratto: lettura ad alta voce

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1** (2026-11-13), issue #8; v1.1 (2026-11-15, issue #20: pulizia delle clip, §1 e §5, nessun cambio di forma); v1.1, nota del 2026-11-16: tempi del modello 1.7B in §1, nessun cambio di forma. Design: ADR 0013.
> Valgono `api-conventions.md`, `sdui.md`, `lessons.md`, `users.md`. I path omettono il prefisso `/api/v1`.

## 1. Concetti

- Il backend genera l'audio dei **blocchi leggibili** di una lezione con un motore di sintesi vocale sul server (oggi Qwen3-TTS, sostituibile). L'audio di un blocco è una **clip**: un file Ogg Opus mono.
- Una clip dipende da testo parlato (§2), motore, modello, voce e parametri. Se uno di questi cambia, serve una clip nuova; quelle vecchie restano in cache finché non passano 90 giorni senza uso, poi il backend le cancella.
- Ogni utente ha **una voce** (§4), valida su tutti i suoi dispositivi. Le voci sono **id opachi** dichiarati dal backend: l'app non ne conosce nessuna in anticipo.
- La generazione avviene in una **coda interna** del backend, non con un `Job` (`jobs.md`): non compare nella lista dei job e non usa la chiave LLM. Un solo blocco alla volta, a circa 4 secondi di lavoro per secondo d'audio (ADR 0013, aggiornamento 2026-11-16).
- **Quando si prepara l'audio di una lezione** (cioè si mettono in coda i blocchi senza clip per la voce attuale dell'utente):
  - subito dopo il salvataggio di una lezione nuova (job `lesson_generation` `succeeded`);
  - a ogni `GET /lessons/{id}` (`lessons.md` §3): l'apertura della lezione. Questo copre le lezioni nate prima della funzione e quelle da rigenerare dopo un cambio di voce, e rimette in coda i blocchi `failed`. La lezione aperta per ultima passa in testa alla coda.
- Se il server non ha un motore configurato, la funzione è **spenta**: `GET /tts/voices` risponde `available: false` e l'app nasconde ogni comando di ascolto.

## 2. Blocchi leggibili e testo parlato

| `type` | Testo parlato |
|---|---|
| `heading` | `text` |
| `paragraph` | `text` ripulito |
| `callout` | `title` (testo semplice, `sdui.md` §9.23) + separatore + `text` ripulito; solo `text` se manca `title`. Separatore: `" "` se `title` finisce con `.`, `!`, `?` o `:`, altrimenti `". "` |
| `diagram` | `alt` |

Gli altri tipi non sono leggibili e non compaiono nel manifest (§3). Anche `missing_component` non è leggibile, e i blocchi del suo `fallback` non hanno un manifest proprio in questa versione.

**Pulizia del markdown inline** (`sdui.md` §3), applicata dal backend:
- `**x**`, `*x*`, `` `x` `` → `x`;
- `[testo](url)` → `testo`;
- citazione `[^key]` → tolta, insieme allo spazio che la precede;
- `[[key|testo]]` → `testo`; `[[key]]` → il `term` della voce `key` del `glossary` del documento;
- le parentesi quadre rimaste diventano tonde (`[x]` → `(x)`): il motore attuale legge `[…]` come comando di stile;
- in ogni testo parlato, anche di `heading` e `alt`: spazi multipli ridotti a uno, spazi agli estremi tolti.

Un testo parlato vuoto dopo la pulizia rende il blocco non leggibile.

## 3. Manifest dell'audio di una lezione

`GET /lessons/{id}/audio` → `200`

```json
{
  "voice": "vivian",
  "blocks": [
    { "block_id": "p1", "status": "ready", "clip": { "url": "/audio/clips/7f3a…", "voice": "vivian", "duration_ms": 41230 } },
    { "block_id": "c1", "status": "pending", "clip": { "url": "/audio/clips/91bc…", "voice": "serena", "duration_ms": 12040 } },
    { "block_id": "p2", "status": "pending", "clip": null },
    { "block_id": "d1", "status": "failed", "clip": null }
  ]
}
```

| Campo | Regole |
|---|---|
| `voice` | la voce attuale dell'utente (§4); `null` con la funzione spenta |
| `blocks` | un elemento per **ogni** blocco leggibile (§2), nell'ordine del documento, anche se non è ancora stato messo in coda (in quel caso `pending`). Il manifest dipende solo dal documento, dalle clip esistenti e dalla coda: la risposta non cambia se arriva prima o dopo `GET /lessons/{id}` |
| `status` | `ready`: c'è la clip con la voce attuale. `pending`: in coda o in generazione. `failed`: 3 tentativi falliti; torna `pending` alla prossima apertura della lezione (`GET /lessons/{id}`). Un motore spento o irraggiungibile non consuma tentativi: il blocco resta `pending`. L'app tratta valori sconosciuti come `pending` |
| `clip` | la clip da riprodurre, o `null`. Con `status` `ready` è sempre presente e ha `voice` uguale a quella attuale. Con `pending` o `failed` può essere una clip **di un'altra voce** dello stesso testo, da usare finché quella nuova non è pronta |
| `clip.url` | path relativo a `/api/v1`, da chiedere con lo stesso `Authorization` |
| `clip.duration_ms` | durata, intero > 0 |

Errori: `400 invalid_id`, `404` (lezione inesistente o di un altro utente, anche con la funzione spenta). Con la funzione spenta: `200` con `voice: null` e `blocks: []`.

Il manifest **non mette in coda niente**: lo fa l'apertura della lezione (§1). Finché qualche blocco è `pending`, l'app lo richiede ogni 15 secondi mentre la lezione è aperta.

## 4. Voci

### 4.1 `GET /tts/voices` → `200`

```json
{
  "available": true,
  "default_voice": "vivian",
  "voices": [
    { "id": "vivian", "label": "Vivian", "sample_url": "/audio/clips/a81e…" },
    { "id": "serena", "label": "Serena", "sample_url": null }
  ]
}
```

- `voices`: l'elenco curato dal server (configurazione del backend), nell'ordine in cui mostrarlo. `label` 1-40 caratteri.
- `sample_url`: clip di una frase di prova fissa letta da quella voce, o `null` se non è ancora pronta (il backend mette in coda i campioni mancanti all'avvio, dopo il lavoro delle lezioni).
- Con la funzione spenta: `available: false`, `default_voice: null`, `voices: []`.

### 4.2 Voce dell'utente

- `User` (`users.md` §1) ha il campo `tts_voice`: la voce **effettiva** dell'utente. Se la voce salvata non è più nell'elenco (motore o elenco cambiati), vale `default_voice`. Con la funzione spenta: `null`.
- `PUT /me/tts-voice` con `{ "voice": "serena" }` → `200` con `User`. Errori: `422 validation_failed` (`details: [{field: "voice", issue: "unknown_voice"}]`) se l'id non è nell'elenco; `409 conflict` con la funzione spenta (controllato prima della voce).
- Cambiare voce **non** mette in coda niente: le lezioni si rigenerano quando vengono aperte (§1).

## 5. Clip

`GET /audio/clips/{clip_id}` → `200`, `Content-Type: audio/ogg`, corpo Ogg Opus mono.

- Serve l'`Authorization`, come ogni richiesta. Un utente può leggere una clip se è un campione di voce (§4.1) oppure se gli è già stata indicata in un suo manifest (§3): il backend registra il permesso quando la mette nel manifest. Per gli altri risponde `404`.
- Il permesso ricorda anche l'ultima volta che un manifest (o l'apertura della lezione) ha nominato la clip, con precisione di un giorno. Una clip non nominata da 90 giorni viene cancellata: chi la chiede dopo riceve `404` e, riaprendo la lezione, il blocco torna `pending` e si rigenera. I campioni delle voci in elenco non scadono.
- Una clip non cambia mai: `Cache-Control: private, max-age=31536000, immutable`. Supporta le richieste `Range` (`206`).
- Errori: `400 invalid_id`, `404` (anche se il file non è più sul disco), `416 range_not_satisfiable` con `Content-Range: bytes */<dimensione>` per un `Range` oltre la fine. Un `Range` multiplo o malformato si ignora e si risponde `200`.

## 6. Regole per l'app

- **Pulsante «Ascolta»** su ogni blocco presente nel manifest:
  - `ready`: riproduce o ferma la clip;
  - `pending` o `failed` con `clip` di un'altra voce: riproduce quella clip;
  - `pending` senza `clip`: disattivato, con l'etichetta «In preparazione»;
  - `failed` senza `clip`: disattivato, con l'etichetta «Audio non disponibile».
- **«Ascolta la lezione»** parte dal primo blocco leggibile visibile in cima allo schermo e prosegue nell'ordine del documento. Salta i blocchi senza clip, scorre la pagina sul blocco letto e si ferma prima del primo esercizio obbligatorio non svolto (`sdui.md` §3, `required`).
- Il blocco in lettura è evidenziato.
- **La lettura si ferma** quando si esce dalla lezione, quando si avvia un altro blocco o la lettura continua, e quando l'app va in secondo piano.
- **Velocità** 0,75× / 1× / 1,25× / 1,5×, tono invariato, salvata sul dispositivo.
