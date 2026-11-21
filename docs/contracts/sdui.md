# Contratto: documento di lezione SDUI, catalogo v1

> Contratto condiviso tra backend (Generator, Validator) e app (renderer). Proprietario: tech lead. Versione: **catalogo v1** (2026-11-08); contratto v1.1 (2026-11-09: `sources[].url` può essere `null` per i PDF caricati); contratto v1.2 (2026-11-11: limite di prosa di `too_long` da 1500 a 2200 parole, stesso `catalog_version`); contratto v1.3 (2026-11-12: nuovo blocco `diagram`, §4 e §10, codice `diagram_structure`, stesso `catalog_version`); contratto v1.4 (2026-11-15: campo facoltativo `code` negli esercizi con una domanda, §4.1, codice `inline_code_too_long`, stesso `catalog_version`); contratto v1.5 (2026-11-15: soglia di `quiz_option_length_cue` da 1,4 a 1,3, la stessa degli eval, stesso `catalog_version`); contratto v1.6 (2026-11-19: codici `fill_blank_answer_in_text` e `quiz_option_markup_uneven` in §6, stesso `catalog_version`).
> Schema formale: `sdui-schema-v1.json` (JSON Schema 2020-12) in questa cartella. Esempi: `fixtures/`. In caso di conflitto tra questo testo e lo schema, vale questo testo e lo schema va corretto.
> Specifiche visive e didattiche di ogni componente: `docs/design/components/<tipo>.md`. Flusso della lezione: `docs/design/screens/lesson.md`.

## 1. Principi

- Una lezione è un **documento JSON** prodotto dal Generator, controllato dal **Validator** (codice, non prompt) e reso dall'app con componenti **nativi** (niente WebView).
- Il documento è una **lista piatta di blocchi**. L'app la divide in **sezioni** a ogni `heading` di livello 2 (`lesson.md`).
- L'app è **tollerante**: un blocco di tipo sconosciuto o malformato non fa crashare nulla; si mostra il `fallback` (se `missing_component`) oppure una riga neutra ("Questo contenuto non è disponibile in questa versione dell'app."). Un blocco non renderizzabile **non** conta come esercizio obbligatorio.
- Il Validator è **severo**: rifiuta ciò che non rispetta questo contratto, con errori leggibili dal Generator per il repair (§6).
- Lingua: quella dell'argomento (`it` o `en`, ADR 0019), con termini tecnici originali spiegati nel glossario (ADR 0005, D3). Testi in un'altra lingua, come le citazioni, restano nella lingua originale con `lang` sul blocco.

## 2. Documento

```json
{
  "catalog_version": 1,
  "title": "Ownership e move",
  "objective": "Prevedere se una variabile è ancora utilizzabile dopo un assegnamento.",
  "estimated_minutes": 8,
  "glossary": [
    { "key": "ownership", "term": "ownership", "definition": "La regola per cui ogni valore ha un solo proprietario...", "avoid": ["proprietà intellettuale"] }
  ],
  "sources": [
    { "key": "rust_book_4_1", "resource_id": null, "title": "The Rust Book, 4.1 What is Ownership?", "url": "https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html" }
  ],
  "blocks": [ { "id": "h1", "type": "heading", "level": 1, "text": "Ownership e move" } ]
}
```

| Campo | Tipo | Regole |
|---|---|---|
| `catalog_version` | int | `1` per questo contratto. L'app che supporta una versione minore rende comunque ciò che conosce |
| `title` | string | 1-120 caratteri |
| `objective` | string | 1-200 caratteri. **Una** vittoria concreta, formulata come capacità ("Saper…", verbo osservabile) |
| `estimated_minutes` | int | 1-20. Dalla v1.2 lo **calcola il backend** al salvataggio (`round(P/250 + 0,5·E + 2·O)`, limitato a 1-20: `P` parole di prosa di §9, `E` esercizi, `O` `open_answer`); il valore scritto dal Generator viene sostituito |
| `glossary` | array | 0-12 voci `{key, term, definition, avoid?}`. `key`: `[a-z0-9_]{1,40}`, unica. `term` 1-80, `definition` 1-400 (markdown inline), `avoid` 0-3 stringhe 1-120 ("non confonderlo con…") |
| `sources` | array | 1-8 voci `{key, resource_id, title, url}`. `key` come sopra; `resource_id`: UUID di una `Resource` del Topic o `null` solo nelle fixture/prove; `title` 1-200; `url` assoluto `https`, oppure `null` se e solo se `resource_id` è una `Resource` `document` (PDF caricato, `documents.md` §6: le pagine vanno nel `title`; dalla v1.1 del contratto, stesso `catalog_version`) |
| `blocks` | array | 3-60 blocchi (§3) |

## 3. Blocchi

Campi comuni a ogni blocco:

| Campo | Tipo | Regole |
|---|---|---|
| `id` | string | `[a-z0-9_-]{1,40}`, unico nel documento. Stabile: serve per tentativi e feedback |
| `type` | string | uno dei tipi del catalogo (§4) |
| `lang` | string? | BCP-47 (es. `"en"`) solo se il contenuto del blocco non è nella lingua dell'argomento (citazioni, ADR 0019). Assente = lingua dell'argomento |

Gli **esercizi** (`quiz_single`, `quiz_multi`, `fill_blank`, `order_steps`, `flashcard`, `open_answer`) hanno anche `required` (bool, default `true`): se `true`, la sezione non prosegue finché l'esercizio non è svolto (`lesson.md`, "Obbligatorietà e sblocco").

### Markdown inline (campi indicati come *md*)

Sottoinsieme di `docs/design/screens/chat.md` §6, **solo inline**: `**grassetto**`, `*corsivo*`, `` `codice` ``, link `[testo](https://…)` (solo `https`), più due estensioni:

- **Citazione**: `[^key]` → rimando alla fonte `sources[].key`. L'app la rende come rimando tappabile alla fonte.
- **Termine del glossario**: `[[key]]` oppure `[[key|testo visibile]]` → termine `glossary[].key`, reso con sottolineatura tratteggiata e tappabile (definizione in sheet).

Niente a capo, elenchi, intestazioni o immagini dentro un campo *md*: la struttura si fa con i blocchi.

## 4. Catalogo v1

| `type` | Campi specifici | Correzione |
|---|---|---|
| `heading` | `level` 1-3 (**obbligatorio**), `text` 1-120 (testo semplice) | — |
| `paragraph` | `text` *md* 1-1200 | — |
| `callout` | `variant` `info`\|`tip`\|`warning`, `title?` 1-80, `text` *md* 1-600 | — |
| `code` | `language` (es. `rust`, `text`), `code` 1-3000 (righe ≤ 100 caratteri consigliate), `caption?` *md* 1-200, `highlight_lines?` 1-6 numeri di riga distinti (1-based, ≤ numero di righe di `code`) | — |
| `glossary_term` | `key` (→ `glossary`) | — |
| `quiz_single` | `code?` (§4.1), `question` *md* 1-400, `options` 2-5 × `{id, text *md* 1-200, feedback *md* 1-400}`, `correct` (id di un'opzione), `required` | locale |
| `quiz_multi` | come `quiz_single` ma `correct`: 1..(n-1) id distinti; `options` 3-6 | locale (esatto: tutte e sole le corrette) |
| `fill_blank` | `text` *md* 1-600 con segnaposto `{{blank_id}}`, `blanks` 1-4 × `{id, answers[] 1-5 × 1-60, case_sensitive (default false)}`, `feedback {correct, incorrect}` *md*, `required` | locale (match normalizzato §5) |
| `order_steps` | `prompt` *md* 1-300, `steps` 3-8 × `{id, text *md* 1-200}` **nell'ordine corretto**, `feedback {correct, incorrect}` *md*, `required` | locale (l'app mescola in modo deterministico per `id` del blocco) |
| `flashcard` | `code?` (§4.1, sul fronte), `front` *md* 1-300, `back` *md* 1-600, `required` | autovalutazione (`again`/`hard`/`good`/`easy`) |
| `open_answer` | `code?` (§4.1), `question` *md* 1-400, `max_chars` 50-2000 (default 1000), `rubric` 1-1500 (**solo backend**, §7), `required` | backend (Grader) |
| `checklist` | `title?` 1-80, `items` 1-8 × `{id, text *md* 1-200}` | autodichiarata, mai obbligatoria |
| `primary_source` | `source` (→ `sources[].key`), `why` *md* 1-300 | — |
| `lesson_link` | `lesson_id` (UUID di una lezione dello stesso Topic), `label` 1-120 | — |
| `ask_agent` | `label?` 1-80 (default app: "Hai dubbi? Chiedi al Tutor"), `prompt_hint` 1-300 (testo precompilato nel composer della chat, modificabile) | — |
| `diagram` | `layout` `flow`\|`tree`\|`cycle`, `title?` 1-80 (testo semplice), `nodes` 2-8 × `{id, label 1-40, detail? 1-60}` (testo semplice), `edges?` 0-12 × `{from, to, label? 1-30}` (id di nodi; testo semplice), `caption` *md* 1-300, `alt` 1-600 (testo semplice). Regole in §10 | — |
| `missing_component` | `wanted` 1-60 (nome del componente desiderato, `snake_case`), `purpose` 1-300, `fallback` 1-6 blocchi di altri tipi (niente `missing_component` annidati) | — |

Gli `id` di opzioni, blanks, steps, items e nodi sono `[a-z0-9_-]{1,40}` e unici **nel blocco**.

### 4.1 Codice di un esercizio

`quiz_single`, `quiz_multi`, `open_answer` e `flashcard` (sul fronte) hanno un campo facoltativo `code`: il frammento di codice che la domanda chiede di leggere.

```json
"code": { "language": "rust", "code": "let mut s = String::from(\"ciao\");\nlet r = &s;\nprintln!(\"{}\", r);" }
```

- `language` come nel blocco `code` (`[a-z0-9_+#-]{1,40}`), `code` testo 1-1000 caratteri, non fatto di soli spazi, a capo ammessi (righe ≤ 60 caratteri consigliate: la card è stretta sul telefono). Nessun `caption` né `highlight_lines`.
- L'app lo mostra **dentro la card dell'esercizio, sopra la domanda** (sopra il fronte nella flashcard), come blocco monospazio senza andare a capo, con scorrimento orizzontale. Specifica visiva: `docs/design/components/_exercises.md`.
- Il Grader riceve `code` insieme alla domanda di un `open_answer`.
- Un frammento di una riga breve (un'espressione, un nome) resta codice inline nella domanda. Codice più lungo o con più istruzioni va qui: il Validator lo rifiuta inline (`inline_code_too_long`, §6).

## 5. Correzione locale (app)

- `quiz_single`: corretto se l'opzione scelta è `correct`. Feedback = `feedback` dell'opzione scelta (+ marcatura della corretta).
- `quiz_multi`: corretto se l'insieme scelto è **uguale** a `correct`; `partial` se interseca senza coincidere. Feedback = `feedback` di ogni opzione scelta e di ogni corretta non scelta.
- `fill_blank`: per ogni blank, risposta normalizzata (trim, spazi interni compressi, NFC, minuscolo salvo `case_sensitive`) uguale a una delle `answers` normalizzate allo stesso modo. Tutti giusti → `correct`; alcuni → `partial`; nessuno → `incorrect`.
- `order_steps`: corretto se l'ordine coincide; altrimenti `incorrect` (nessun parziale in v1).
- `flashcard`: nessuna correzione; l'esito è l'autovalutazione.

Invio dei tentativi al backend: `lessons.md` §5.

## 6. Validator (backend)

Il Validator riceve il documento (e il contesto: id delle `Resource` del Topic, id delle lezioni del Topic, chiavi del glossario del Topic) e restituisce **tutti** gli errori trovati, non solo il primo:

```json
{ "path": "/blocks/7/options/2/text", "code": "quiz_option_length", "message": "Le opzioni del quiz 'q2' hanno lunghezze troppo diverse (3-11 parole, massimo scarto 3). Parole per opzione: opzione 0: 3 parole; opzione 1: 9 parole; opzione 2: 11 parole. Riscrivi le opzioni fuori intervallo in modo che tutte abbiano tra 9 e 12 parole." }
```

`path` è un JSON Pointer; `message` è in italiano e pensato per il repair del Generator.

| `code` | Regola |
|---|---|
| `schema` | Il documento non rispetta lo schema (tipi, campi obbligatori, lunghezze, enum, `catalog_version`) |
| `unknown_block_type` | `type` non esiste nel catalogo v1 |
| `duplicate_id` | `id` di blocco duplicato nel documento, id duplicato dentro un blocco, o `key` duplicata in `glossary`/`sources` |
| `quiz_correct_missing` | `correct` non indica opzioni esistenti; in `quiz_multi` tutte o nessuna corrette |
| `quiz_option_length` | Le opzioni di un quiz differiscono troppo in numero di parole: (max − min) > tolleranza (default **3**, configurabile) |
| `quiz_option_length_cue` | In un `quiz_single`, la lunghezza in caratteri del testo visibile dell'opzione corretta supera di oltre **1,3 volte** la mediana delle opzioni sbagliate (la risposta si riconosce dalla lunghezza). In un `quiz_multi` vale per ciascuna corretta rispetto alla mediana delle sbagliate |
| `quiz_answer_position` | Con ≥ 3 `quiz_single` nella lezione, la risposta corretta è sempre nella stessa posizione |
| `fill_blank_mismatch` | Segnaposto `{{id}}` nel testo senza blank corrispondente, o blank non usato |
| `unknown_citation` | `[^key]` che non esiste in `sources` |
| `unknown_resource` | `sources[].resource_id` non è una `Resource` del Topic (`null` vietato in produzione) |
| `unknown_glossary_key` | `[[key]]` o `glossary_term.key` che non esiste in `glossary` del documento (il glossario del Topic non basta: il documento è autosufficiente, `lessons.md` §4) |
| `unused_entry` | Voce di `glossary` o `sources` mai usata nel documento |
| `primary_source_count` | Non c'è **esattamente un** `primary_source` |
| `ask_agent_missing` | Non c'è almeno un `ask_agent` |
| `no_required_exercise` | Nessun esercizio con `required: true` (vincolo "single tangible win") |
| `too_long` | Più di 6 sezioni (heading di livello 2) o più di **2200 parole** di prosa (1500 fino alla v1.1) (configurabile; definizione in §9). Il limite di 60 blocchi è già un errore `schema` |
| `first_block_not_title` | Il primo blocco non è un `heading` di livello 1 |
| `section_without_content` | Un `heading` di livello 2 senza blocchi prima del successivo |
| `invalid_lesson_link` | `lesson_id` non è una lezione del Topic |
| `nested_missing_component` | `missing_component` dentro un `fallback` |
| `diagram_structure` | La struttura di un `diagram` non rispetta il suo `layout` (§10): arco verso un nodo inesistente, arco da un nodo a se stesso, arco ripetuto, nodo isolato, ciclo in un `flow`, `tree` senza una sola radice o con un nodo con più genitori, `cycle` con `edges` |
| `inline_code_too_long` | (contratto v1.4) In un campo *md* qualsiasi, uno span di codice inline `` `…` `` ha più di **60 caratteri** o contiene **due o più `;`**. Il messaggio dice di spostarlo nel campo `code` dell'esercizio (§4.1) o, fuori dagli esercizi, in un blocco `code` |
| `markdown` | Markdown non ammesso in un campo *md* (a capo, intestazioni, link non `https`, immagini, HTML) |
| `fill_blank_answer_in_text` | (contratto v1.6) La risposta di un blank di `fill_blank` compare nel testo vicino al suo segnaposto (heading, `caption`, prosa vicina). Regola e soglie in `design/quality-warnings.md` §1 (a) |
| `quiz_option_markup_uneven` | (contratto v1.6) In un `quiz_single` o `quiz_multi` il markup (codice inline, grassetto, link) è presente in alcune opzioni sì e in altre no. Regola in `design/quality-warnings.md` §1 (b) |

## 7. Cosa riceve l'app

Il documento salvato dal backend è completo. Quando l'app lo richiede (`GET /lessons/{id}`, `lessons.md` §3.1) il backend **rimuove** i campi riservati: `open_answer.rubric`. Tutto il resto arriva così com'è, insieme a `catalog_version`.

## 8. Fixture

In `docs/contracts/fixtures/`:

- `lesson-*.json`: lezioni **valide** che coprono tutti i 17 tipi; usate dal Validator (devono passare) e dall'app (schermata Lezione che carica una fixture).
- `invalid/*.json`: un file per codice di errore del §6 (dove testabile senza contesto del Topic), con accanto `*.expected.json` = lista dei `code` attesi.
- `unknown-type.json`: lezione valida tranne un blocco di tipo inventato (l'app mostra il fallback; il Validator la rifiuta con `unknown_block_type`).

## 9. Precisazioni (v1.1, dopo la stesura dello schema e delle fixture)

1. **Niente `min_app_version` per blocco** (`docs/mvp.md` §6 lo prevedeva): la compatibilità si gestisce con `catalog_version` a livello di documento e con il fallback dei blocchi sconosciuti.
2. `fill_blank.feedback` e `order_steps.feedback`: `correct` e `incorrect` *md* 1-400 ciascuno.
3. Dentro `` `codice` `` il contenuto è letterale: `<`, `#`, `[[`, `[^` non sono interpretati né controllati dalla regola `markdown`.
4. **Parole di prosa** (per `too_long`): token separati da spazi del testo visibile, markup rimosso, di `paragraph.text`, `callout.title`/`text`, `quiz_*.question`, `options[].text`, `options[].feedback`.
5. `quiz_option_length` usa lo stesso conteggio di parole, sul testo visibile delle opzioni.
6. **Blocchi dentro `fallback`**: validati come gli altri (id unici, riferimenti, markdown, schema), ma **non** contano per `no_required_exercise`, `primary_source_count`, `ask_agent_missing` né per `quiz_answer_position`.
7. `unused_entry`: una voce è usata se è referenziata da un blocco (anche dentro un `fallback`); i rimandi dentro `glossary[].definition` non contano come uso.
8. `section_without_content`: un heading di livello 2 seguito solo da heading (di qualunque livello) prima del successivo heading 2 o della fine.
9. `quiz_answer_position`: "posizione" = indice in `options` (l'app **non** mescola le opzioni dei quiz; mescola solo `order_steps`).
10. `fill_blank`: un segnaposto `{{id}}` ripetuto nel testo è `fill_blank_mismatch`.
11. Il parser dell'app accetta `open_answer` sia con sia senza `rubric` (le fixture la contengono; le risposte di `GET /lessons/{id}` no) e non la mostra mai.
12. Ordine dei controlli: un blocco di `type` sconosciuto produce solo `unknown_block_type` (non viene validato contro lo schema); `missing_component` annidato produce solo `nested_missing_component`; `quiz_multi.correct` vuoto produce `quiz_correct_missing`.
13. **Campi non previsti**: il Validator li rifiuta (`schema`, come `additionalProperties: false` nello schema JSON); l'app li ignora.
14. Limiti non indicati sopra: `code.language` 1-40 caratteri `[a-z0-9_+#-]`; `missing_component.wanted` 1-60 in `snake_case`; `lang` BCP-47 `[a-z]{2,3}(-[A-Za-z0-9]{1,8})*`; `sources[].url` `https` assoluto, senza spazi, ≤ 2000 caratteri, oppure `null` (§2).
15. `quiz_multi.correct` con id ripetuti → `duplicate_id` con path `/blocks/N/correct/<i>`.
16. Le regole sul documento intero (`primary_source_count`, `ask_agent_missing`, `no_required_exercise`, `too_long`) usano `path: "/blocks"`.
17. **`catalog_version` superiore** a quella supportata dall'app: l'app non blocca, rende ciò che conosce (fallback per il resto) e mostra un `StatusPanel info` che invita ad aggiornare l'app.
18. **Correzione locale (§5), precisazioni**: l'app mescola `order_steps` con l'algoritmo deterministico di `docs/design/components/order_steps.md` e garantisce un ordine iniziale diverso dalla soluzione; nei quiz, se la risposta è errata, l'app mostra anche il `feedback` dell'opzione corretta; `quiz_multi` è `incorrect` se l'insieme scelto non interseca `correct`.
19. **Esercizi dentro un `fallback`**: per il Validator non contano (§9.6); nell'app, se sono renderizzabili, rispettano il loro `required` come gli altri.
20. `glossary[].definition` non può contenere `[[ ]]` (niente rimandi tra termini nel v1): errore `markdown`.
21. `code.highlight_lines` fuori dalle righe del codice o ripetuti → `schema`.
22. **Ordine di validazione**: se il documento ha errori `schema` (o `unknown_block_type`/`nested_missing_component`/`duplicate_id` di forma), le regole trasversali (conteggi, riferimenti, `unused_entry`, `too_long`, posizioni dei quiz) **non** vengono valutate: prima si ripara la forma, poi si rivalida. "Tutti gli errori" del §6 vale quindi per livello.
23. `callout.title` è **testo semplice** (niente markdown, citazioni o termini), ma conta per le parole di prosa.
24. **Dimensione massima** del documento accettata dal Validator: 256 KiB di JSON; oltre → un solo errore `too_long` con `path: ""`, senza ulteriore analisi.
25. I `message` degli errori citano valori dell'input troncati a 60 caratteri (con "…") e con i caratteri di controllo resi visibili.
26. "A capo" vietato nei campi *md* include `\n`, `\r`, U+0085, U+2028, U+2029.
27. **Normalizzazione di `fill_blank`** (§5): "spazi" include tutti gli spazi Unicode (categoria Z, tab e NBSP compresi): trim e compressione li trattano tutti come uno spazio ASCII.

## 10. Diagrammi (v1.3)

Un `diagram` è una figura **descritta in forma strutturata** (nodi, archi, etichette) che l'app disegna con componenti nativi e con i colori del design system; non è un'immagine né un disegno libero. È statico: niente zoom né tocco sui nodi. Decisioni e alternative scartate: `docs/adr/0010-diagrammi-sdui.md`.

```json
{
  "id": "s1_fig",
  "type": "diagram",
  "layout": "flow",
  "title": "Dal file modificato al commit",
  "nodes": [
    { "id": "wd", "label": "Working directory", "detail": "file modificati" },
    { "id": "stage", "label": "Staging area" },
    { "id": "repo", "label": "Repository" }
  ],
  "edges": [
    { "from": "wd", "to": "stage", "label": "git add" },
    { "from": "stage", "to": "repo", "label": "git commit" }
  ],
  "caption": "Un file passa sempre dalla **staging area** prima di entrare in un commit.",
  "alt": "Tre riquadri in fila: working directory, staging area, repository. Una freccia 'git add' va dal primo al secondo, una freccia 'git commit' dal secondo al terzo."
}
```

1. **`layout`** decide come l'app dispone i nodi e quali archi sono ammessi:
   - `flow`: processo a passi, anche con diramazioni. `edges` obbligatorio (≥ 1), orientato, **senza cicli**; ogni nodo è toccato da almeno un arco e il grafo è connesso. L'app dispone i nodi per livelli (livello = cammino più lungo dalla partenza).
   - `tree`: gerarchia. `edges` obbligatorio: ogni arco va dal genitore al figlio; **una sola radice** (nodo senza genitore), ogni altro nodo ha **esattamente un** genitore, tutti raggiungibili dalla radice. Gli archi quindi sono sempre `nodes − 1`.
   - `cycle`: ciclo che si ripete. I nodi sono **nell'ordine del ciclo** e l'ultimo torna al primo; `edges` deve mancare (o essere vuoto) e servono almeno 3 nodi.
2. Ogni violazione del punto 1, un arco con `from` o `to` che non è l'`id` di un nodo, un arco con `from` = `to` e due archi con la stessa coppia `from`/`to` producono `diagram_structure`, con `path` sull'arco o sul blocco. Un `cycle` con meno di 3 nodi è `diagram_structure` (lo schema non lo esprime).
3. `label`, `detail`, `title`, le etichette degli archi e `alt` sono **testo semplice**: niente markdown, citazioni o termini del glossario. Solo `caption` è *md* (e segue le regole di `markdown`, `unknown_citation`, `unknown_glossary_key`).
4. **`alt`** descrive la figura a parole, per chi non la vede: è ciò che legge lo screen reader ed è ciò che l'app mostra, insieme a `title` e `caption`, quando non riesce a disegnare il diagramma (struttura non valida, `layout` sconosciuto). Deve bastare da sola a capire il contenuto della figura. Un `diagram` che non si può disegnare **non** diventa la riga neutra del §1 se `alt` è leggibile.
5. **`caption`** dice che cosa guardare nella figura o che cosa se ne conclude; non ripete `title`.
6. I testi di un `diagram` **non** contano per le parole di prosa (§9.4). Un `diagram` non è un esercizio.
7. L'app non esce mai dalla larghezza della colonna della lezione: sceglie orientamento e ritorni a capo secondo `docs/design/components/diagram.md`. Il Generator non indica coordinate, colori o dimensioni.
8. **Ordine dei controlli**: `diagram_structure` è una regola trasversale (§9.22), valutata solo senza errori `schema`. Dentro un blocco gli errori dei singoli archi (nodo inesistente → `path` `/edges/i/from` o `/edges/i/to`; `from` = `to` e coppia ripetuta → `/edges/i`) vengono prima di quelli sul grafo: se ce n'è uno, cicli, radici e connessione non si valutano. Un ciclo in un `flow` è segnalato sull'arco che lo chiude, un secondo genitore in un `tree` sul secondo arco entrante. In un `cycle` con `edges` non vuoto c'è un solo errore, su `/edges`.
9. **Compatibilità**: `catalog_version` resta `1`. Un'app precedente alla v1.3 tratta `diagram` come tipo sconosciuto e mostra la riga neutra (§1).
