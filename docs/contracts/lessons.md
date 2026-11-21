# Contratto: Lezioni, tentativi, learning record e glossario

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1.9** (2026-11-21).
> **v1.9 (ADR 0019):** campo di sola lettura `language` in `LessonView` (§3.1): la lingua dell'argomento della lezione (`"it"` o `"en"`), per la lingua di default dei blocchi SDUI senza `lang` (`sdui.md` §3). Modifica additiva.
> **v1.8:** `document_sources` in `Lesson` (§2), con pagina minima e massima per documento usato dal piano; `GET /lessons?status=in_progress` per la card «Riprendi la lezione» (§3.2); step `reading` «Leggo le fonti» nel job (§4). Modifica additiva.
> **v1.7:** `quality_warnings` in `Lesson` (§2) e `LessonView` (§3.1): avvisi non bloccanti del Validator, regole e soglie in `design/quality-warnings.md`. Il Validator (§4) distingue errori e avvisi. Modifica additiva.
> **v1.6:** una scheda del Recorder può indicare l'esercizio mai superato da cui nasce (`unpassed_block`, §6.1): il backend lo controlla (§6.3) e la scheda è «da consolidare» nel ripasso (`reviews.md` v1.4). Modifica additiva.
> **v1.5:** `GlossaryTerm.first_lesson_id` (la lezione in cui il termine è nato); lezione richiesta dalla chat col tool `request_lesson` del Tutor e parte di messaggio `lesson_request` (§12). Modifiche additive.
> **v1.4:** note a mano dell'utente sopra la lezione: `LessonView.strokes` e `PUT /lessons/{id}/strokes` (§11, ADR 0015). Modifica additiva.
> **v1.3:** `GET /lessons/{id}` mette in coda l'audio dei blocchi leggibili che mancano per la voce dell'utente, e il salvataggio di una lezione nuova fa lo stesso (`audio.md` §1). Il manifest è `GET /lessons/{id}/audio`. Modifica additiva.
> **v1.2:** evidenziazioni dell'utente sul testo della lezione: `LessonView.highlights` e `PUT /lessons/{id}/highlights` (§10, ADR 0011). Modifica additiva.
> **v1.1:** generazione e chiusura di una lezione senza chiave LLM rispondono `409 llm_key_missing`; l'invio dei tentativi non fallisce mai per la chiave, le risposte aperte restano in attesa di valutazione (`users.md` §3).
> Valgono `api-conventions.md`, `jobs.md`, `sdui.md` (documento di lezione, catalogo v1), `resources.md`, `mission.md`. Formati di riferimento della skill `teach` (Matt Pocock): `LEARNING-RECORD-FORMAT.md`, `GLOSSARY-FORMAT.md`. I path omettono il prefisso `/api/v1`.

## 1. Concetti

- **Lezione**: un documento SDUI (`sdui.md`) generato per un Topic `ready`, numerato in ordine (1, 2, 3…) nel Topic. Si genera **solo su richiesta** dell'utente ("Genera lezione"), con un job `lesson_generation` (§4). Ogni generazione costa: niente generazioni automatiche.
- La generazione passa da **Planner** (decide cosa insegnare ora: zona di sviluppo prossimale, una vittoria concreta) → **Generator** (scrive il documento) → **Validator** (codice, `sdui.md` §6) → al massimo **2 riparazioni** (3 documenti in tutto) → salvataggio. Se il terzo documento non passa, il job fallisce con `invalid_output` e l'utente può riprovare.
- **Tentativo** (`Attempt`): la risposta a un esercizio. Conta il **primo** tentativo; il secondo esiste solo per la "Riprova" facoltativa (`lesson.md` §5) di `quiz_multi`, `fill_blank`, `order_steps`. Gli esercizi chiusi li corregge l'app (`sdui.md` §5) e il backend registra l'esito ricevuto; `open_answer` lo valuta il **Grader** sul backend.
- **Chiusura**: "Completa lezione" segna la lezione `completed` e avvia un job `lesson_completion`: il **Recorder** legge tentativi e valutazioni e produce **learning record**, **termini del glossario** e **schede di ripasso**.
- **Coverage ≠ learning** (regola della skill, garantita dal backend, non dal prompt): un record o un termine nasce solo da esercizi **superati** in questa lezione (§6.3). Una misconcezione corretta deve generare un record: è un'istruzione al Recorder (misurata dagli eval), mentre il backend garantisce solo che ogni record abbia prove valide.
- Le schede di ripasso (fronte/retro, ADR 0005 D2) scheduling e API: `reviews.md`.

## 2. Risorse

### `Lesson`

```json
{
  "id": "01933b4e-...",
  "topic_id": "01933b4e-...",
  "number": 3,
  "title": "Lifetimes in pratica",
  "objective": "Saper dire quale riferimento vive più a lungo in una funzione con due parametri.",
  "estimated_minutes": 8,
  "status": "in_progress",
  "catalog_version": 1,
  "mission_version": 3,
  "created_at": "2026-11-09T15:02:11.420Z",
  "completed_at": null,
  "quality_warnings": [],
  "document_sources": []
}
```

Lezione che segue due documenti caricati (esempio v1.8):

```json
{
  "document_sources": [
    { "document_id": "01933b50-...", "title": "Appunti Rust, cap. 3", "page_start": 12, "page_end": 18 },
    { "document_id": "01933b5a-...", "title": "Note sui lifetime", "page_start": 2, "page_end": 4 }
  ]
}
```

| Campo | Regole |
|---|---|
| `document_sources` | (v1.8) array, sempre presente, `[]` se la lezione non segue documenti caricati. Elemento: `{ "document_id": uuid, "title": string, "page_start": int, "page_end": int }`, uno per documento usato dal piano del Planner, nell'ordine del piano. `page_start` e `page_end` sono la pagina minima e la massima delle sezioni scelte dal piano per quel documento (1-based, inclusive). Calcolato in lettura: un documento eliminato sparisce dall'array. Presente ovunque compaia `Lesson` (liste e `LessonView`). Un'app precedente alla v1.8 ignora il campo |
| `quality_warnings` | (v1.7) array, sempre presente, `[]` se nessun avviso. Elemento: `{ "code": string, "block_id": string \| null, "message": string }`. `code` è uno di: `explanation_run_too_long`, `too_many_callouts`, `fill_blank_answer_too_long`, `consecutive_code_blocks`; l'app tratta un codice sconosciuto come avviso generico (mostra `message`). `message` è nella lingua dell'argomento (ADR 0019), scritto per la persona che studia. `block_id` è l'id del blocco interessato (il primo, se più blocchi), `null` se l'avviso riguarda la lezione intera. Regole e soglie: `design/quality-warnings.md`. Lezioni precedenti alla v1.7: `[]`. Un'app precedente alla v1.7 ignora il campo |
| `number` | 1, 2, 3… nel Topic, senza buchi, assegnato al salvataggio |
| `title`, `objective`, `estimated_minutes` | copiati dal documento (`estimated_minutes` è quello calcolato dal backend, `sdui.md` §2) |
| `status` | `not_started` → `in_progress` (primo tentativo registrato) → `completed` (`POST …/complete`). L'app tratta valori sconosciuti come `not_started` |
| `mission_version` | versione della mission usata per generarla |
| `completed_at` | `null` finché non è `completed` |

L'avanzamento per sezioni ("in corso · 2/4") è stato locale dell'app (`lesson.md` §2), non del backend.

### `Attempt`

```json
{
  "id": "01933b4e-...",
  "block_id": "q2",
  "block_type": "open_answer",
  "attempt_number": 1,
  "answer": { "text": "Perché `a` è stata spostata in `b`." },
  "outcome": "partial",
  "grade": {
    "verdict": "partial",
    "feedback": "Giusto che il valore si sposta in `b`; manca *perché* `a` non è più valida: il proprietario è uno solo.",
    "misconception": null
  },
  "answered_at": "2026-11-09T15:10:02.000Z",
  "created_at": "2026-11-09T15:10:02.311Z"
}
```

| Campo | Regole |
|---|---|
| `block_type` | tipo del blocco esercizio nel documento |
| `attempt_number` | `1`, oppure `2` solo per `quiz_multi`, `fill_blank`, `order_steps` e solo se il tentativo 1 esiste e non è `correct` |
| `answer` | forma per tipo, §2.1 |
| `outcome` | `correct` \| `partial` \| `incorrect` \| `pending` (solo `open_answer` non ancora valutato). `quiz_single` e `order_steps` non hanno `partial` |
| `grade` | solo `open_answer`: `null` finché `pending`; poi `{verdict, feedback, misconception}`. `verdict` = `outcome`; `feedback` *md* inline (`sdui.md` §3) 1-800, nella lingua dell'argomento (ADR 0019), rivolto alla persona; `misconception` `null` oppure 1-300 caratteri: la convinzione sbagliata riconosciuta, in terza persona ("Pensa che…") |
| `answered_at` | istante in cui l'utente ha risposto (orologio dell'app) |

#### 2.1 `answer` per tipo

| `block_type` | `answer` | Validazione |
|---|---|---|
| `quiz_single` | `{ "option": "b" }` | id di un'opzione del blocco |
| `quiz_multi` | `{ "options": ["a", "c"] }` | 1..n id distinti del blocco |
| `fill_blank` | `{ "blanks": { "b1": "borrow" } }` | esattamente gli id dei blank del blocco; valori 0-120 caratteri |
| `order_steps` | `{ "steps": ["s2", "s1", "s3"] }` | permutazione degli id degli step |
| `flashcard` | `{ "rating": "good" }` | `again` \| `hard` \| `good` \| `easy`; il backend ne deriva `outcome`: `again` → `incorrect`, `hard` → `partial`, `good`/`easy` → `correct` |
| `open_answer` | `{ "text": "…" }` | trim, 1..`max_chars` del blocco |

`checklist` non produce tentativi (è autodichiarata, `sdui.md` §4).

### `LearningRecord`

Equivalente strutturato di un file `learning-records/NNNN-slug.md` della skill.

```json
{
  "id": "01933b4e-...",
  "number": 4,
  "kind": "misconception_corrected",
  "title": "Dopo un move la variabile di partenza non è più utilizzabile",
  "body": "Pensavi che `a` restasse valida dopo `let b = a;` con una `String`; ora prevedi correttamente l'errore del compilatore. Conta per i prossimi argomenti sui riferimenti.",
  "evidence": "Hai sbagliato la domanda «Dopo la riga 2…» e poi hai spiegato correttamente perché `a` non è più valida (lezione 2).",
  "status": "active",
  "superseded_by": null,
  "lesson_id": "01933b4e-...",
  "created_at": "2026-11-09T15:14:40.002Z"
}
```

| Campo | Regole |
|---|---|
| `number` | 1, 2, 3… nel Topic (come `LR-NNNN`), assegnato al salvataggio |
| `kind` | `demonstrated` (ha dimostrato di saper usare qualcosa di non banale) \| `misconception_corrected` (credeva una cosa sbagliata e ora vede perché). L'app mostra un testo generico per valori sconosciuti |
| `title` | 1-120, *md* inline (solo `` `codice` ``, grassetto e corsivo), nella lingua dell'argomento (ADR 0019), la cosa appresa |
| `body` | 1-600, *md* inline, **in seconda persona** (l'app lo mostra alla persona): 1-3 frasi su cosa è stato appreso e perché conta per le prossime lezioni |
| `evidence` | 1-400, nella lingua dell'argomento (ADR 0019), in seconda persona: come lo hai dimostrato |
| `status` | `active` \| `superseded` |
| `superseded_by` | `null` oppure l'`id` del record che lo sostituisce |
| `lesson_id` | lezione da cui è nato |

### `GlossaryTerm`

Equivalente strutturato di una voce di `GLOSSARY.md`.

```json
{
  "id": "01933b4e-...",
  "key": "ownership",
  "term": "ownership",
  "definition": "La regola per cui ogni valore ha un solo proprietario, che lo libera quando esce di scope.",
  "avoid": ["proprietà"],
  "lesson_id": "01933b4e-...",
  "first_lesson_id": "01933b4e-...",
  "created_at": "2026-11-09T15:14:40.002Z",
  "updated_at": "2026-11-09T15:14:40.002Z"
}
```

| Campo | Regole |
|---|---|
| `key` | `[a-z0-9_]{1,40}`, unica nel Topic. È la stessa chiave dei `glossary[].key` dei documenti (`sdui.md` §2) |
| `term` | 1-80, nella forma usata dalla comunità (ADR 0005, D3: anche in inglese) |
| `definition` | 1-400, *md* inline senza `[[ ]]`: cosa **è** il termine, 1-2 frasi, nella lingua dell'argomento (ADR 0019) |
| `avoid` | 0-3 stringhe 1-120: sinonimi da evitare |
| `lesson_id` | lezione che lo ha aggiunto o rivisto per ultima |
| `first_lesson_id` | uuid \| null (v1.5): lezione che lo ha aggiunto la prima volta; non cambia quando il termine viene rivisto. `null` solo per termini nati prima della v1.5 e già rivisti (la lezione d'origine non è nota); per quelli mai rivisti vale `lesson_id`. Un'app precedente alla v1.5 ignora il campo |

Un termine esistente si **rivede sul posto** (stessa `key`, `updated_at` aggiornato), non si duplica.

## 3. Endpoint

| Metodo | Path | Successo | Errori |
|---|---|---|---|
| POST | `/topics/{id}/lessons` | `202` il `Job` `lesson_generation` (`queued`) | `400 invalid_id`, `404`, `409 conflict` (Topic non `ready`, oppure c'è già un job `lesson_generation` attivo) |
| GET | `/topics/{id}/lessons` | `200` lista paginata di `Lesson`, `number` **decrescente** | `400`, `404` |
| GET | `/lessons?status=in_progress` | `200` lista paginata di `LessonInProgress` (§3.2) | `400 bad_request` (`status` mancante o diverso da `in_progress`) |
| GET | `/lessons/{id}` | `200` `LessonView` (§3.1); prepara l'audio (`audio.md` §1) | `400 invalid_id`, `404` |
| GET | `/lessons/{id}/audio` | `200` manifest dell'audio (`audio.md` §3) | `400 invalid_id`, `404` |
| POST | `/lessons/{id}/attempts` | `200` `{ "attempts": [Attempt] }` (§5) | `400 invalid_id`, `404`, `409 conflict` (lezione `completed`), `422 validation_failed` |
| PUT | `/lessons/{id}/highlights` | `200` `{ "highlights": [Highlight] }` (§10) | `400 invalid_id`, `404`, `422 validation_failed` |
| PUT | `/lessons/{id}/strokes` | `200` `{ "strokes": [Stroke] }` (§11) | `400 invalid_id`, `404`, `413 payload_too_large` (corpo oltre 4 MiB), `422 validation_failed` |
| POST | `/lessons/{id}/complete` | `202` il `Job` `lesson_completion` (`queued`) | `400 invalid_id`, `404`, `409 conflict` (già `completed`) |
| GET | `/topics/{id}/glossary` | `200` lista paginata di `GlossaryTerm`, ordine alfabetico di `term` (senza distinzione di maiuscole), poi `key` | `400`, `404` |
| GET | `/topics/{id}/learning-records` | `200` lista paginata di `LearningRecord`, `number` decrescente. Query `status=active` (default) \| `all` | `400`, `404` |

`POST /topics/{id}/lessons` non ha corpo (un corpo vuoto o `{}` è accettato). Il job `lesson_generation` corrente è il primo con quel `kind` di `GET /topics/{id}/jobs` (`jobs.md` §3); il retry di un job fallito è quello di `jobs.md`.

### 3.1 `LessonView`

```json
{
  "lesson": { "...": "Lesson" },
  "language": "it",
  "document": { "...": "documento SDUI senza i campi riservati (sdui.md §7)" },
  "attempts": [ { "...": "Attempt" } ],
  "highlights": [ { "...": "Highlight" } ],
  "strokes": [ { "...": "Stroke" } ],
  "completion": null,
  "quality_warnings": []
}
```

- `quality_warnings`: copia di `Lesson.quality_warnings` (v1.7, §2), `[]` se nessun avviso. Un'app precedente alla v1.7 ignora il campo.
- `document_sources`: copia di `Lesson.document_sources` (v1.8, §2), `[]` se la lezione non segue documenti. Un'app precedente alla v1.8 ignora il campo.
- `language` (v1.9, ADR 0019): lingua dell'argomento della lezione, `"it"` o `"en"`, sempre presente. Di sola lettura: l'app non la invia mai. Un blocco SDUI senza `lang` è nella lingua indicata qui (`sdui.md` §3). Un'app precedente alla v1.9 ignora il campo.
- `attempts`: tutti i tentativi della lezione, ordinati per `created_at`. Riflettono sempre lo stato attuale: un `open_answer` valutato dopo (reinvio o job `lesson_completion`) arriva con `outcome` e `grade` aggiornati. L'app li usa per ripristinare gli esiti e le valutazioni di `open_answer` (per esempio su un altro dispositivo o dopo aver svuotato i dati).
- `highlights`: le evidenziazioni dell'utente (§10), `[]` se non ce ne sono. Un'app precedente alla v1.2 ignora il campo.
- `strokes`: le note a mano dell'utente (§11), `[]` se non ce ne sono, nell'ordine di disegno. Un'app precedente alla v1.4 ignora il campo.
- `completion`: `null` finché la lezione non è `completed`; poi:

```json
{
  "job": { "...": "ultimo Job lesson_completion della lezione" },
  "records": [ { "...": "LearningRecord nati da questa lezione" } ],
  "terms": [ { "...": "GlossaryTerm aggiunti o rivisti da questa lezione" } ],
  "review_card_count": 3
}
```

`records`, `terms` e `review_card_count` sono vuoti/0 finché il job non è `succeeded`.

### 3.2 `LessonInProgress` (v1.8)

`GET /lessons?status=in_progress` → `200` lista paginata (cursore, `api-conventions.md` §8) di `LessonInProgress`: tutti i campi di `Lesson` (§2, compreso `document_sources`), più:

- `topic_title` (string);
- `last_activity_at` (data ISO del tentativo più recente della lezione).

Solo le lezioni `in_progress` dei Topic dell'utente, ordinate per `last_activity_at` decrescente, poi `id`. `status` è obbligatorio e oggi accetta solo `in_progress`: mancante o diverso → `400 bad_request`. Uso: card «Riprendi la lezione» in Home (`limit=1`).

## 4. Job `lesson_generation`

- `progress.step`: `planning` ("Scelgo cosa insegnare") · [`reading` ("Leggo le fonti"), v1.8: solo quando il piano segue documenti caricati o la lettura web delle fonti è attiva; senza `done`/`total`] · `writing` ("Scrivo la lezione") · `checking` ("Controllo la lezione") · `saving`. Durante una riparazione lo step torna `writing` con `done`/`total` = numero del documento / 3 e `message` "Correggo la lezione". Regola per l'app: `step == writing` con `done` > 1 = riparazione in corso. L'app mostra testi fissi per step e solo lo spinner per uno step sconosciuto.
- `result` (quando `succeeded`): `{ "lesson_id": "…", "lesson_number": 3 }`.
- `error.code`: quelli di `jobs.md` §2; `invalid_output` = il Planner non ha prodotto un piano valido dopo una riparazione, oppure nessuno dei 3 documenti ha superato il Validator.
- Al salvataggio, in una transazione: lezione (documento completo con `rubric`, `catalog_version`, piano del Planner, `mission_version`) e una `ComponentRequest` per ogni blocco `missing_component` (`wanted`, `purpose`, `fallback`; l'API è in `component-requests.md`).
- Contesto del Validator in produzione (`sdui.md` §6): `sources[].resource_id` deve essere una `Resource` del Topic (`null` vietato); `lesson_link.lesson_id` una lezione esistente del Topic. Al salvataggio il backend sovrascrive `sources[].url` con quello della `Resource` indicata (il modello non inventa URL).
- (v1.7) Il Validator distingue **errori** (bloccanti: il documento non si salva e va riparato, `sdui.md` §6) e **avvisi** (non bloccanti: il documento si salva con `quality_warnings`, §2). Due nuovi errori bloccanti: una risposta di `fill_blank` visibile nel testo vicino al suo segnaposto, e markup solo in alcune opzioni di un quiz.
- (v1.7) Se il documento è valido ma ha avvisi, il backend tenta una riparazione, dentro il limite di riparazioni esistente. Tiene il documento riparato solo se è valido e ha meno avvisi; altrimenti tiene quello di partenza. Gli avvisi del documento salvato finiscono in `quality_warnings` (§2).
- (v1.7, decisioni del tech lead) Tenuta della riparazione per punteggio: `explanation_run_too_long` vale 2, gli altri avvisi 1. Si tiene il documento riparato solo se è valido e con punteggio strettamente minore; altrimenti si tiene quello di partenza.
- (v1.7) Limite riparazioni: la riparazione per soli avvisi parte solo da un documento già valido, quindi non sottrae tentativi a un errore. Se il riparato ha errori si tiene il precedente valido, senza altre riparazioni. Massimo una riparazione per avvisi per lezione, sempre dentro `max_document_repairs`.
- Il documento è autosufficiente: ogni `[[key]]` e `glossary_term.key` deve stare nel `glossary` del documento. Per i termini già nel glossario del Topic il Generator ne riusa `key` e `definition`. (Questo restringe `sdui.md` §6 `unknown_glossary_key`: il glossario del Topic non basta più.)

## 5. Tentativi

```json
POST /lessons/{id}/attempts
{
  "attempts": [
    { "block_id": "q1", "attempt_number": 1, "answer": { "option": "b" }, "outcome": "correct", "answered_at": "2026-11-09T15:08:40.000Z" },
    { "block_id": "q2", "attempt_number": 1, "answer": { "text": "Perché…" }, "answered_at": "2026-11-09T15:10:02.000Z" }
  ]
}
```

- 1-50 tentativi per richiesta, **al massimo un `open_answer`** (la valutazione è sincrona, §5.1).
- `outcome` è obbligatorio per `quiz_single`, `quiz_multi`, `fill_blank`, `order_steps` (correzione locale dell'app, `sdui.md` §5: il backend la registra e non la ricalcola) e vietato per `flashcard` e `open_answer`.
- `outcome` presente per `flashcard`/`open_answer` → `issue: "invalid"`; assente per i tipi corretti dall'app → `issue: "empty"`.
- Validazione: `block_id` deve essere un blocco esercizio del documento (anche dentro un `fallback`); `answer` della forma del §2.1; regole di `attempt_number` del §2. Errori `422` con `details` `{field: "attempts[i].<campo>", issue}`; un solo tentativo non valido fa rifiutare tutta la richiesta.
- **Idempotenza, vince il primo**: la coppia (`block_id`, `attempt_number`) è unica per lezione. Un tentativo già registrato non viene sovrascritto: la risposta restituisce quello salvato. Così l'app può reinviare la sua coda senza duplicati.
- Il primo tentativo registrato porta la lezione da `not_started` a `in_progress`.
- La risposta contiene i tentativi della richiesta **come salvati**, nello stesso ordine.

### 5.1 Valutazione di `open_answer`

- Il backend chiama il Grader (domanda, `rubric`, obiettivo della lezione, risposta) **dentro la richiesta**, con un tempo massimo di 20 s. Se riesce, il tentativo torna con `outcome` = `verdict` e `grade` compilato.
- Se il Grader fallisce o va oltre il tempo, il tentativo viene salvato comunque con `outcome: "pending"` e `grade: null` (risposta `200`): l'app mostra «Risposta registrata». **Reinviare** lo stesso tentativo (stessa coppia) ritenta la valutazione di uno `pending`; uno già valutato non viene rivalutato.
- I tentativi ancora `pending` alla chiusura li valuta il job `lesson_completion` (§6).

## 6. Chiusura e job `lesson_completion`

- `POST /lessons/{id}/complete`: la lezione diventa subito `completed` (`completed_at`), e nasce il job `lesson_completion` (uno per lezione; il retry è quello di `jobs.md`). L'app invia prima la coda dei tentativi; i tentativi che arrivano dopo la chiusura sono rifiutati (`409`).
- `progress.step`: `grading` (valuta gli `open_answer` `pending`) · `recording` ("Registro cosa hai dimostrato") · `saving`.
- `result` (quando `succeeded`): `{ "lesson_id": "…", "record_count": 2, "term_count": 3, "review_card_count": 4 }`.
- Se un `open_answer` resta `pending` dopo la valutazione nel job, non conta come esercizio superato.

### 6.1 Cosa produce il Recorder

Al massimo **5 record**, **8 termini** (nuovi o rivisti) e **10 schede** per lezione. Ogni elemento cita come prova gli `id` dei blocchi esercizio della lezione.

- **Record**: `kind`, `title`, `body`, `evidence`, i blocchi di prova, e facoltativamente i `number` dei record attivi che **sostituisce** (la comprensione si è approfondita o corretta). Nessun record per ciò che è stato solo spiegato, già detto da un termine del glossario, o per un semplice diario della lezione.
- **Termini**: `key`, `term`, `definition`, `avoid`, i blocchi di prova. Solo termini che la persona ha dimostrato di saper usare. Una `key` esistente nel Topic = revisione del termine.
- **Schede di ripasso** (`ReviewCard`, fronte/retro, ADR 0005 D2): `front` 1-300 e `back` 1-600 *md* inline, ciascuna legata a **un** record o termine prodotto in questa esecuzione. Una scheda chiede di richiamare, non di riconoscere.
  - (v1.6) `unpassed_block`: `null`, oppure l'`id` dell'esercizio della lezione **mai superato** (nessun tentativo `correct`) di cui la scheda mette alla prova il punto sbagliato. Una scheda così è **da consolidare** (`ReviewCard.to_consolidate`, `reviews.md` §1.3). Resta legata al record o termine più vicino, che non afferma il punto sbagliato.

### 6.2 Cosa salva il backend (una transazione)

- Record nuovi con `number` progressivo; i record sostituiti passano a `superseded` con `superseded_by`, e le loro schede vengono **sospese**. Le schede nuove nascono nello stato FSRS `new` con `due` = `created_at` + 24 h (`reviews.md`).
- Termini nuovi o rivisti (sul posto, per `key`).
- Schede di ripasso legate al loro record o termine.
- Se il job fallisce, nulla di parziale resta salvato.

### 6.3 Garanzie del backend (Validator del Recorder, non affidate al prompt)

Un tentativo è **superato** se il suo `outcome` è `correct` (primo o secondo tentativo).

| Regola | Controllo |
|---|---|
| Prove esistenti | Ogni blocco citato è un esercizio della lezione con almeno un tentativo |
| `demonstrated` | Almeno un blocco citato ha un tentativo superato |
| `misconception_corrected` | Almeno un blocco citato ha il **primo** tentativo `incorrect`/`partial` (o una `misconception` del Grader) **e** almeno un blocco citato ha un tentativo superato (anche il secondo tentativo dello stesso blocco) |
| Termini | Almeno un blocco citato ha un tentativo superato; `key` e lunghezze come §2 |
| Sostituzioni | I `number` citati sono record **attivi** del Topic |
| Schede | Ognuna legata a un record o termine prodotto in questa esecuzione; lunghezze come §6.1 |
| Scheda da consolidare | (v1.6) `unpassed_block`, se presente, è un esercizio della lezione con almeno un tentativo e **nessun** tentativo superato |
| Limiti | Al massimo 5 record, 8 termini, 10 schede |

Come per il Researcher: una riparazione con l'elenco degli errori, poi `invalid_output`. Una lezione senza esercizi superati produce zero record, zero termini, zero schede (job comunque `succeeded`): è un esito legittimo.

## 7. Chat dal bottone `ask_agent`

`POST /topics/{id}/messages` (`chat.md` §2) accetta il campo facoltativo `lesson_id`: l'app lo invia quando la chat è stata aperta da un blocco `ask_agent` (o da «Chiedi al Tutor» del riepilogo), con il testo precompilato da `prompt_hint`. Il backend verifica che la lezione sia del Topic (`422` `{field: "lesson_id", issue: "invalid"}` se non è un UUID, `issue: "not_found"` se non è una lezione del Topic) e, per quel turno, aggiunge al contesto del Tutor titolo, obiettivo e numero della lezione. Il campo non compare nel `ChatMessage` e non viene salvato: il retry di una risposta fallita (`chat.md` §2) avviene senza il contesto della lezione.

## 8. Eventi

Nessun evento nuovo: `job_updated` (`jobs.md` §4) per generazione e chiusura. Quando un job `lesson_generation` riesce, l'app ricarica `GET /topics/{id}/lessons`; quando un job `lesson_completion` riesce, ricarica `GET /lessons/{id}` (o, se non ascolta gli eventi del Topic, lo interroga ogni 3 s finché il job è attivo).

## 9. Fuori da questa versione

- Lezione su un argomento indicato dall'utente: anche dalla chat (§12) sceglie il Planner.
- (v1.8, decisione del tech lead) Le «pagine strutturate in `sources`», proposte dal designer (F10), non si fanno: le copre `document_sources` (§2); le pagine restano nel `title` della fonte (`sdui.md` §2).
- Record di conoscenze pregresse dichiarate in chat e di cambi di mission (casi 2 e 4 della skill): li scriverà il Tutor in una versione successiva.
- API delle schede di ripasso e scheduling: `reviews.md`. API delle `ComponentRequest`: `component-requests.md`.

## 10. Evidenziazioni (v1.2)

Segni dell'utente sul testo di una lezione, fatti con la penna (ADR 0011). Servono solo a chi legge: non entrano nei contesti degli agenti.

### `Highlight`

```json
{ "block_id": "p3", "start": 12, "end": 47, "color": "yellow" }
```

| Campo | Regole |
|---|---|
| `block_id` | un blocco `paragraph` o `callout` del documento (anche dentro un `fallback`) |
| `start`, `end` | intervallo semiaperto `[start, end)` nel **sorgente** del campo `text` del blocco (il markdown inline così come sta nel documento), in **unità di codice UTF-16**. `0 ≤ start < end ≤ lunghezza di text` |
| `color` | `yellow` \| `green` \| `blue` \| `pink`. L'app tratta un valore sconosciuto come `yellow` |

Un'evidenziazione non ha id: la identifica la terna (`block_id`, `start`, `end`).

### Invio

```json
PUT /lessons/{id}/highlights
{ "highlights": [ { "block_id": "p3", "start": 12, "end": 47, "color": "yellow" } ] }
```

- Il corpo è l'**insieme intero** delle evidenziazioni della lezione: sostituisce quello salvato, in una transazione. `{ "highlights": [] }` le toglie tutte. Reinviare lo stesso corpo non cambia nulla.
- Al massimo **500** evidenziazioni per lezione.
- Nello stesso blocco gli intervalli **non si sovrappongono** (possono toccarsi: `end` di uno = `start` del successivo). Fusioni, divisioni e cambi di colore li risolve l'app prima di inviare.
- Errori `422` con `details` `{field: "highlights[i].<campo>", issue}`: `block_id` inesistente o di un tipo non evidenziabile → `invalid`; `start`/`end` mancante o non intero → `invalid`; `start`/`end` intero fuori dai limiti (anche negativo) → `out_of_range`; `start ≥ end` → `highlights[i].start` `out_of_range`; corpo senza l'array `highlights` → `highlights` `invalid`; `color` sconosciuto → `invalid`; sovrapposizione → `highlights[i].start` `overlap` (sul secondo dei due); più di 500 → `highlights` `too_many`. Una sola voce non valida fa rifiutare tutta la richiesta.
- La risposta contiene l'insieme **come salvato**, ordinato per posizione del blocco nel documento e poi per `start`. Lo stesso ordine vale per `LessonView.highlights`.
- È ammesso in ogni `status` della lezione, anche `completed`, e non lo cambia.
- Il backend non controlla che `start` e `end` cadano su confini di parola o di marcatore: è compito dell'app (sotto).
- Eliminare la lezione (o il suo Topic) elimina le sue evidenziazioni.

### Regole per l'app

- L'app converte tra sorgente e testo mostrato con il proprio parser del markdown inline. Un carattere mostrato è evidenziato se la sua posizione nel sorgente cade nell'intervallo.
- Gli elementi che il renderer sostituisce (`[[key]]`, `[[key|testo]]`, `[^key]`) sono **atomici**: si evidenziano per intero e l'intervallo copre tutto il loro sorgente. I marcatori (`**`, `*`, `` ` ``, la parte `](https://…)` di un link) non contano: un intervallo può includerli o no senza cambiare ciò che si vede.
- Un intervallo che dopo la conversione non copre alcun carattere mostrato si ignora, senza errori.
- **Sincronizzazione**: l'insieme locale è la verità finché ha modifiche non inviate; l'app reinvia l'insieme intero finché il `PUT` riesce. Senza modifiche in sospeso, all'apertura della lezione vale `LessonView.highlights`. Con due dispositivi vince l'ultimo invio (limite accettato, ADR 0011).
- **`422`**: se `details` indica `highlights[i]`, l'app scarta quella voce e reinvia, al massimo 3 scarti per invio. Senza indice (`too_many`, corpo non valido) o oltre i 3 scarti, l'insieme resta locale e visibile ma non si ritenta da solo: riparte con una nuova modifica, alla riapertura della lezione o con «Riprova ora».

## 11. Note a mano (v1.4)

Tratti disegnati dall'utente con la penna sopra la lezione (ADR 0015, design `docs/design/components/handwriting.md`). Servono solo a chi legge: non entrano nei contesti degli agenti e non si riconoscono come testo.

### `Stroke`

```json
{
  "id": "5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f10",
  "block_id": "p3",
  "color": "ink",
  "width": "medium",
  "points": [1204, 380, 1311, 402, 1498, 415]
}
```

| Campo | Regole |
|---|---|
| `id` | UUID (36 caratteri, forma canonica con trattini, minuscole o maiuscole) generato dall'app. Unico nella lezione senza distinguere maiuscole e minuscole; il backend lo restituisce come inviato |
| `block_id` | id di un blocco di **primo livello** del documento (una voce diretta di `blocks`, di qualsiasi tipo; un `missing_component` con il suo `fallback` vale come blocco unico, i blocchi annidati no) |
| `color` | `ink` \| `blue` \| `red` \| `green`. In lettura l'app tratta un valore sconosciuto come `ink` |
| `width` | `thin` \| `medium` \| `thick`. In lettura l'app tratta un valore sconosciuto come `medium` |
| `points` | array piatto di interi `[x0, y0, x1, y1, …]`, lunghezza pari e ≥ 2 (un solo punto è un puntino). Coordinate dall'angolo in alto a sinistra del blocco, in **decimillesimi della larghezza del blocco** (`x = 10000` è il bordo destro). `x ∈ [-40000, 50000]`, `y ∈ [-40000, 200000]` |

Niente pressione e niente campo `tool`: un campo `pressure` futuro sarà additivo.

### Invio

```json
PUT /lessons/{id}/strokes
{ "strokes": [ { "id": "…", "block_id": "p3", "color": "ink", "width": "medium", "points": [1204, 380] } ] }
```

- Il corpo è l'**insieme intero** dei tratti della lezione: sostituisce quello salvato, in una transazione. `{ "strokes": [] }` li toglie tutti. Reinviare lo stesso corpo non cambia nulla.
- Limiti: al massimo **2000** tratti, **1000** punti per tratto, **200 000** punti in tutto. Corpo fino a **4 MiB** (limite proprio di questo endpoint, `api-conventions.md`); oltre, `413 payload_too_large`.
- L'**ordine dell'array è l'ordine di disegno** (il primo sta sotto): il backend lo conserva. La risposta contiene l'insieme come salvato, nello stesso ordine; lo stesso vale per `LessonView.strokes`.
- Errori `422` con `details` `{field, issue}`: corpo senza l'array `strokes` → `strokes` `invalid`; `id` mancante o non UUID → `strokes[i].id` `invalid`; `id` ripetuto → `strokes[i].id` `duplicate` (sul secondo); `block_id` inesistente, annidato o mancante → `strokes[i].block_id` `invalid`; `color` o `width` mancante o sconosciuto → `strokes[i].color`/`strokes[i].width` `invalid`; `points` mancante, non array di interi, vuoto o di lunghezza dispari → `strokes[i].points` `invalid`; un intero fuori dagli intervalli → `strokes[i].points` `out_of_range`; più di 1000 punti in un tratto → `strokes[i].points` `too_many`; più di 2000 tratti → `strokes` `too_many`; più di 200 000 punti in tutto → `strokes` `too_many_points`. Una sola voce non valida fa rifiutare tutta la richiesta.
- È ammesso in ogni `status` della lezione, anche `completed`, e non lo cambia.
- Eliminare la lezione (o il suo Topic) elimina i suoi tratti.

### Regole per l'app

- Ridisegno: `X = sinistra del blocco + x / 10000 · larghezza`, `Y = bordo alto del blocco + y / 10000 · larghezza`, con la geometria attuale del blocco. La nota segue il blocco, non le parole (limite accettato, ADR 0015).
- Un tratto il cui `block_id` non esiste nel documento si ignora, senza errori.
- **Sincronizzazione** come le evidenziazioni (§10): l'insieme locale è la verità finché ha modifiche non inviate; senza modifiche in sospeso, all'apertura vale `LessonView.strokes`; con due dispositivi vince l'ultimo invio.
- **`422`**: se `details` indica `strokes[i]`, l'app scarta quel tratto e reinvia, al massimo 3 scarti per invio. Senza indice o oltre i 3 scarti, e con `413`, l'insieme resta locale e visibile ma non si ritenta da solo (riparte con una nuova modifica, alla riapertura o con «Riprova ora»).

## 12. Lezione richiesta dalla chat (v1.5)

La persona può chiedere la prossima lezione al Tutor in chat. Il Tutor chiama il tool interno `request_lesson` (`mission.md` §6), che ha lo **stesso effetto** di `POST /topics/{id}/lessons`: mette in coda un job `lesson_generation` (§4) e il Planner sceglie che cosa insegnare. Il tool non ha argomenti.

### Ammissibilità (verificata dal backend, non affidata al prompt)

| Caso | Esito del tool (al modello, non all'utente) |
|---|---|
| Topic `ready` e nessun job `lesson_generation` attivo | job creato (`job_updated` come per l'endpoint), parte `lesson_request` |
| Topic non `ready` (mission non confermata o ricerca in corso) | errore `not_ready`: nessun job, nessuna parte. Il Tutor spiega che cosa manca |
| C'è già un job `lesson_generation` attivo | nessun job nuovo; parte `lesson_request` con il job attivo (la persona vede l'avanzamento di quello). Il Tutor dice che una lezione è già in preparazione |
| Chiave LLM mancante | come l'endpoint: errore `llm_key_missing` al modello, nessun job, nessuna parte |

Al massimo un `request_lesson` per turno del Tutor, e mai nello stesso turno di un tool di mission (`save_mission_draft`, `propose_mission_change`): un messaggio ha al più **una card** (`mission_proposal` o `lesson_request`).

### Parte di messaggio `lesson_request`

Nuovo tipo di parte in `ChatMessage.parts` (`chat.md` §1), nel messaggio del Tutor che ha chiamato `request_lesson`:

```json
{ "type": "lesson_request", "job_id": "01933b4e-..." }
```

- Come `mission_proposal` (`mission.md` §3, §7): è sempre **l'ultima** parte, viene aggiunta **alla chiusura** del messaggio (`message_completed` o `message_failed`), resta se il messaggio fallisce; "Riprova" azzera le parti e il Tutor può richiamare il tool (con un job ormai attivo o già riuscito: vedi la tabella sopra; se il job è riuscito ne nasce uno nuovo).
- È uno **riferimento**, non uno snapshot: lo stato della card lo dà il job, letto da `GET /topics/{id}/jobs` e aggiornato con `job_updated`. `queued`/`running`: avanzamento con i testi per step del §4. `succeeded`: «Lezione N pronta» con il bottone «Apri lezione» (`result.lesson_id`, `result.lesson_number`). `failed`: il `message` dell'errore; il bottone «Riprova» (`POST /topics/{id}/jobs/{job_id}/retry`) solo se è l'ultimo job `lesson_generation` del Topic, altrimenti la card dice solo che la generazione non è riuscita.
- Se il backend si riavvia dopo che il tool ha messo in coda il job ma prima della chiusura del messaggio, il messaggio diventa `failed` **senza** la parte, mentre il job continua (`jobs.md` §1) e compare nella tab Lezioni. "Riprova" segue la tabella sopra: con il job ancora attivo la card punta a quello, con il job già riuscito ne nasce uno nuovo.
- Se il job non è nella lista caricata (pagine più vecchie), l'app mostra la card senza stato e senza bottoni finché non lo trova; non è un errore.
- App vecchie che non conoscono il tipo lo ignorano (regola di `chat.md`).
