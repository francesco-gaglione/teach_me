# Contratto: Documenti caricati (PDF)

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1** (2026-11-09), Fase 10.
> Valgono `api-conventions.md`, `jobs.md`, `resources.md`, `lessons.md`, `sdui.md`. Decisioni del proprietario: il PDF **si aggiunge** alla ricerca web (non la sostituisce); i PDF **scansionati** (senza testo selezionabile) sono rifiutati. Libreria di estrazione: ADR 0007. I path omettono il prefisso `/api/v1`.

## 1. Concetti

- L'utente può caricare uno o più **PDF** in un Topic, in qualunque stato del Topic. Ogni PDF è un **`Document`**.
- Il backend estrae il testo **senza LLM** (nessun costo) in un job `document_import`, pagina per pagina, e lo divide in **sezioni**: dai segnalibri del PDF quando ci sono, altrimenti a blocchi di pagine consecutive (§4).
- Quando l'import riesce, il backend crea una **`Resource`** con `kind: "document"` legata al documento (`resources.md` v1.1). È quella che le lezioni citano, come le fonti web.
- **Il PDF originale non viene conservato**: resta sul dispositivo dell'utente. Il backend tiene il file solo finché l'import non è riuscito o il documento non viene eliminato, poi tiene solo il testo per pagina e le sezioni (nel database, quindi coperto dai backup).
- **PDF più web**: il Researcher riceve l'indice dei documenti del Topic e cerca sul web fonti che li completano, senza duplicarli. Le sue ricerche sostituiscono solo le fonti web: le `Resource` `document` non vengono mai toccate da una ricerca.
- **Mission**: se un documento è già importato durante l'intervista, il Tutor ne conosce titolo e indice e può chiedere quali parti studiare. La mission resta testo libero.
- **Lezioni**: il Planner sceglie per ogni lezione le sezioni del documento da usare (in ordine, se la mission dice di seguire il libro). Il Generator riceve il **testo** di quelle sezioni, entro un limite di lunghezza, e cita le pagine (§6).

## 2. Risorsa `Document`

```json
{
  "id": "01933b4e-...",
  "topic_id": "01933b4e-...",
  "resource_id": "01933b4e-...",
  "filename": "rust-in-action.pdf",
  "title": "Rust in Action",
  "status": "ready",
  "error": null,
  "size_bytes": 8421337,
  "page_count": 456,
  "language": "en",
  "sections": [
    { "id": "s1", "title": "1 Introducing Rust", "level": 1, "page_start": 1, "page_end": 24 }
  ],
  "created_at": "2026-11-09T18:02:11.420Z"
}
```

| Campo | Regole |
|---|---|
| `resource_id` | la `Resource` `document` creata all'import; `null` finché `status` non è `ready` |
| `filename` | nome del file caricato, 1-255 caratteri (ripulito dal backend: niente percorsi) |
| `title` | titolo dai metadati del PDF se plausibile, altrimenti `filename` senza estensione; 1-300 |
| `status` | `importing` \| `ready` \| `failed`. L'app tratta valori sconosciuti come `importing` |
| `error` | `null`, tranne con `status: failed`: `{code, message}` del job `document_import` che è fallito (§4), così l'app sa il motivo anche dopo un riavvio |
| `page_count` | `null` finché non è noto |
| `language` | BCP-47 stimato dal testo, `null` se incerto |
| `sections` | vuota finché `status` non è `ready`; al massimo 300 voci in ordine di pagina. `id` stabile nel documento (`s1`, `s2`…), `title` 1-200, `level` 1-3 (profondità nei segnalibri; 1 per i blocchi di pagine), pagine numerate da 1 come nel file (non le etichette stampate) |

## 3. Endpoint

| Metodo | Path | Successo | Errori |
|---|---|---|---|
| POST | `/topics/{id}/documents` | `202` `{ "document": Document, "job": Job }` (job `document_import` `queued`) | `400 invalid_id`, `404`, `409 conflict` (c'è già un job `document_import` attivo nel Topic, oppure il Topic ha già 5 documenti), `413 payload_too_large` (> 50 MiB, cioè 52.428.800 byte), `415 unsupported_media_type` (non è un PDF) |
| GET | `/topics/{id}/documents` | `200` `{ "documents": [Document] }`, dal più recente | `400 invalid_id`, `404` |
| DELETE | `/documents/{id}` | `204` | `400 invalid_id`, `404`, `409 conflict` (import in corso) |

- **Caricamento**: `multipart/form-data` con un solo campo `file`. Il backend verifica la firma `%PDF-` nel primo KB (non si fida del content type) e il limite di 50 MiB mentre riceve, senza tenere in memoria tutto il file.
- Un documento `failed` conta nel limite di 5 finché non viene eliminato. Messaggi dei `409`: "L'argomento ha già 5 documenti: eliminane uno per caricarne un altro." e, per `DELETE`, "Il documento è in fase di importazione: riprova quando ha finito."
- **Eliminazione** (risposta senza corpo): cancella documento, pagine, sezioni e la `Resource` `document`. Le lezioni già generate che la citano restano leggibili: la loro voce `sources` conserva il titolo (`url` era già `null`, §6).

## 4. Il job `document_import`

- `progress.step`: `reading` | `extracting` (con `done`/`total` = pagine; gli altri step senza contatori) | `sectioning` | `saving`.
- `result` (quando `succeeded`): `{ "document_id": "…", "page_count": 456, "section_count": 31 }`.
- Codici di `error` in più rispetto a `jobs.md` §2, **non ritentabili** (il retry fallisce di nuovo allo stesso modo; l'app propone di eliminare il documento):

| Codice | Quando | `message` |
|---|---|---|
| `invalid_pdf` | il file non si apre come PDF | "Il file non è un PDF valido." |
| `encrypted_pdf` | il PDF è protetto da password | "Il PDF è protetto da password." |
| `too_many_pages` | più di 1.500 pagine | "Il PDF ha troppe pagine (massimo 1.500)." |
| `no_text` | PDF scansionato o quasi: meno di 200 caratteri di testo in media per pagina, oppure più della metà delle pagine senza testo | "Il PDF non contiene testo selezionabile (forse è una scansione)." |

- Sezioni: dai segnalibri fino al livello 3; una sezione più lunga di 30 pagine si divide in parti ("…, parte 2"). Senza segnalibri: blocchi di 10 pagine intitolati "Pagine 1-10", "Pagine 11-20"…
- `status` ed `error` del `Document` seguono il suo job (`Job.document_id`): `importing` mentre il job è attivo (anche dopo un retry, con `error: null`), `failed` con l'errore del job, `ready` quando l'import è salvato. La `Resource` `document` nasce nella stessa transazione del salvataggio.
- La `Resource` `document` ha la `language` del documento, `und` se incerta.

## 5. Eventi

Nessun evento nuovo: `job_updated` per l'avanzamento. A job terminato l'app ricarica `GET /topics/{id}/documents` e `GET /topics/{id}/resources`.

## 6. Documenti nelle lezioni

- La `Resource` `document` (`resources.md` v1.1) ha `url: null`, `source_type: "pdf"` e `document_id`.
- Nel documento di lezione (`sdui.md` v1.1), una voce `sources` che punta a una `Resource` `document` ha **`url: null`** e indica le pagine nel `title` ("Rust in Action, cap. 4 (pp. 101-118)"). L'app la mostra come "Dal tuo PDF", senza link.
- Il Validator in produzione verifica che `url` sia `null` **se e solo se** `resource_id` è una `Resource` `document`; al salvataggio il backend sovrascrive `url` come oggi (con `null` per i documenti).
- Contesto del Generator: il testo delle sezioni scelte dal Planner, al massimo **24.000 caratteri** per lezione (intestazioni e note comprese) (taglio a fine pagina, con l'indicazione delle pagine omesse). Le sezioni da usare sono un campo del `LessonPlan` (interno al backend, non esposto dall'API).

## 7. Fuori da questa versione

PDF scansionati (OCR o lettura come immagini), altri formati (EPUB, DOCX), conservare e riaprire il PDF dall'app a una pagina, ricerca nel testo, aggiornare un documento sostituendone il file.
