# Contratto: Cartelle e Topic

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1.1** (2026-11-10).
> **v1.1:** cartelle e argomenti appartengono all'utente della sessione. Gli elenchi contengono solo i suoi, il nome della cartella è unico **per utente**, `position` è per utente; una cartella o un argomento altrui risponde `404`, anche come destinazione di uno spostamento (`users.md` §4).
> Valgono tutte le regole di `api-conventions.md` (prefisso `/api/v1`, errori, paginazione, date, id). I path sotto omettono il prefisso.

## 1. Risorse

### Folder

```json
{
  "id": "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f",
  "name": "Lavoro",
  "position": 0,
  "topic_count": 3,
  "created_at": "2026-11-08T14:36:01.129Z",
  "updated_at": "2026-11-08T14:36:01.129Z"
}
```

| Campo | Tipo | Note |
|---|---|---|
| `name` | string | Normalizzato con trim degli spazi iniziali e finali. 1-80 caratteri (code point Unicode, dopo il trim). Unico tra le cartelle, confronto case-insensitive dopo il trim (minuscole Unicode semplici, senza normalizzazione NFC né case folding completo: `ß` e `SS` sono distinti) |
| `position` | int ≥ 0 | Assegnata dal backend alla creazione (max attuale + 1, 0 per la prima). In sola lettura in v1: il riordino arriverà con un endpoint dedicato |
| `topic_count` | int ≥ 0 | Numero di Topic nella cartella, calcolato |

### Topic

```json
{
  "id": "01933b4e-9a1b-7c3d-9e1f-2a3b4c5d6e70",
  "folder_id": "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f",
  "title": "Rust",
  "language": "it",
  "state": "new",
  "created_at": "2026-11-08T14:40:12.003Z",
  "updated_at": "2026-11-08T14:40:12.003Z"
}
```

| Campo | Tipo | Note |
|---|---|---|
| `title` | string | Trim come `name`. 1-120 caratteri. Unico **nella stessa cartella**, case-insensitive |
| `state` | enum | `new` · `interviewing` · `mission_proposed` · `researching` · `ready` (ciclo di vita in `docs/mvp.md` §5). Alla creazione è `new`. **Non modificabile via API**: cambia solo per effetto dei flussi di intervista e ricerca. L'app mappa valori sconosciuti su `Unknown` |
| `language` | enum | `"it"` \| `"en"` (ISO 639-1 minuscolo). Sempre presente nelle risposte. Alla creazione è il valore scelto nel corpo, oppure `"it"` se assente. **Non modificabile via API**: la lingua è dell'argomento e non cambia (ADR 0019). L'app mappa valori sconosciuti su `Unknown` |

`updated_at` cambia a ogni modifica della risorsa stessa (rinomina, spostamento, cambio di stato). Creare o eliminare un Topic **non** modifica `updated_at` della cartella.

## 2. Endpoint

| Metodo | Path | Successo | Errori specifici |
|---|---|---|---|
| GET | `/folders` | `200` lista paginata di `Folder` | `400 bad_request` (query) |
| POST | `/folders` | `201` `Folder` + `Location: /api/v1/folders/{id}` | `422` |
| GET | `/folders/{id}` | `200` `Folder` | `400 invalid_id`, `404` |
| PATCH | `/folders/{id}` | `200` `Folder` | `400 invalid_id`, `404`, `422` |
| DELETE | `/folders/{id}` | `204` | `400 invalid_id`, `404` |
| GET | `/folders/{id}/topics` | `200` lista paginata di `Topic` | `400`, `404` (cartella inesistente) |
| POST | `/folders/{id}/topics` | `201` `Topic` + `Location: /api/v1/topics/{id}` | `400 invalid_id`, `404` (cartella), `422` |
| GET | `/topics/{id}` | `200` `Topic` | `400 invalid_id`, `404` |
| PATCH | `/topics/{id}` | `200` `Topic` | `400 invalid_id`, `404`, `422` |
| DELETE | `/topics/{id}` | `204` | `400 invalid_id`, `404` |

### Corpi delle richieste

```json
POST /folders                 { "name": "Lavoro" }
PATCH /folders/{id}           { "name": "Lavoro e studio" }
POST /folders/{id}/topics     { "title": "Rust" }
PATCH /topics/{id}            { "title": "Rust avanzato", "folder_id": "01933b4e-..." }
```

- `PATCH /topics/{id}`: entrambi i campi opzionali (assente = invariato). `folder_id` sposta il Topic in un'altra cartella; l'unicità del titolo si verifica nella cartella di destinazione. Un `PATCH` con corpo `{}` è valido e restituisce la risorsa invariata (senza toccare `updated_at`).
- `PATCH /folders/{id}`: `name` opzionale per simmetria; `{}` restituisce la risorsa invariata.
- `POST /folders/{id}/topics`: `language` opzionale, `"it"` o `"en"`, default `"it"` (ADR 0019). Un valore diverso → `422 validation_failed` con issue `invalid` su `language` (§3). In `PATCH /topics/{id}` il campo è ignorato, come `state`.
- Rinominare con lo stesso nome (anche con maiuscole diverse) **non** è un duplicato di sé stessi.
- Un valore identico a quello attuale (stessa stringa dopo il trim) è un no-op: `updated_at` non cambia. Un cambio di sole maiuscole è una modifica e aggiorna `updated_at`.
- Precedenza degli errori: l'esistenza della risorsa nel path (`404`) viene verificata prima della validazione del corpo (`422`).
- Corpo che non è un oggetto JSON o campo di tipo errato → `422 validation_failed` con `details: null` (`api-conventions.md` §7).
- Gli id (nel path e in `folder_id`) sono accettati solo nella forma canonica con trattini (36 caratteri, maiuscole tollerate); altre forme (`simple`, `{braced}`, `urn:uuid:`) → `400 invalid_id` nel path, `folder_id: invalid` nel corpo.
- Campi sconosciuti nel corpo: ignorati. `state`, `position`, `language`, `id`, date nel corpo: ignorati (non modificabili).
- Valori `null` espliciti → `422 validation_failed`: issue `empty` per `name`/`title`, issue `invalid` per `folder_id` (non c'è nulla da "azzerare"). `"language": null` equivale ad assente (default `"it"`, ADR 0019).

### Ordinamento delle liste

- `GET /folders`: `position` crescente, poi `id` crescente.
- `GET /folders/{id}/topics`: `created_at` crescente, poi `id` crescente (gli id sono UUID v7, quindi l'ordine coincide con la creazione).
- Paginazione a cursore come in `api-conventions.md` §8 (default 50, massimo 200). Il cursore è opaco e codifica la chiave di ordinamento dell'ultimo elemento; rimane valido se nel frattempo si creano o eliminano elementi.

### Eliminazione

- `DELETE /folders/{id}` elimina **la cartella e tutti i suoi Topic** (e tutto ciò che appartiene a quei Topic: chat, mission, fonti, lezioni, record, schede). Operazione irreversibile: l'app chiede conferma nominando la cartella e il numero di Topic (`topic_count`).
- `DELETE /topics/{id}` elimina il Topic e tutto ciò che gli appartiene. L'app chiede conferma nominando il Topic.
- Le `position` delle altre cartelle non vengono ricompattate (possono avere buchi; contano solo per l'ordine).

## 3. Errori di validazione

`422 validation_failed` con `details` attribuiti al campo:

| `field` | `issue` | Quando |
|---|---|---|
| `name` / `title` | `empty` | vuoto o solo spazi dopo il trim, oppure `null` esplicito |
| `name` / `title` | `too_long` | oltre 80 (`name`) o 120 (`title`) caratteri dopo il trim |
| `name` / `title` | `duplicate` | esiste già una cartella con lo stesso nome / un Topic con lo stesso titolo nella cartella (per uno spostamento: nella cartella di destinazione, segnalato su `title`) |
| `folder_id` | `not_found` | la cartella di destinazione di uno spostamento non esiste |
| `folder_id` | `invalid` | non è un UUID valido, oppure `null` esplicito |
| `language` | `invalid` | solo in creazione del Topic: valore diverso da `"it"` e `"en"` (ADR 0019) |

Più problemi nello stesso corpo → più elementi in `details`. Un campo obbligatorio mancante in `POST` (`{}`) → `422` con issue `empty` sul campo.

Il `message` del 422 è generico ("I dati inviati non sono validi."): l'app usa `details`, non `message`.

Esempio:

```json
{
  "error": {
    "code": "validation_failed",
    "message": "I dati inviati non sono validi.",
    "details": [ { "field": "name", "issue": "duplicate" } ],
    "request_id": "01933b4e-..."
  }
}
```

L'app mostra il messaggio sotto il campo in base a `field` + `issue` (testi in `docs/design/screens/`), non il `message` del backend.

## 4. Testi lato app

I testi definitivi per ogni `issue` (e per errori di rete, 401, 404, 5xx) sono nelle specifiche delle schermate: `docs/design/screens/home.md`, `docs/design/screens/folder.md`, `docs/design/base-components.md` §10.1 (`InputDialog`). Nell'interfaccia il Topic si chiama **"argomento"** (`docs/design/design-system.md` §11).
