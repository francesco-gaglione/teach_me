# Prompt di teach_me

Proprietario: `llm-engineer`. Il backend li incorpora con `include_str!` (nessun file letto a runtime).
Specifica del port LLM, della configurazione dei modelli e del Context Assembler: `llm-port.md`.

## Convenzioni

- Un ruolo = una cartella. Ogni cartella ha `VERSION` (es. `tutor-phase2-v1`, da incrementare a ogni modifica del testo; il backend la scrive nei log insieme a modello e usage) e due tipi di file:
  - `system.md`: parte **statica**, nessuna variabile. Va per prima nel prompt ed è cacheable.
  - `context.md`: parte **variabile**, template con segnaposto `{{nome}}` sostituiti dal Context Assembler (sostituzione di stringhe semplice, nessun motore di template; un segnaposto non riconosciuto è un errore di avvio/test).
- Nessuna data, orario o id nel testo statico (invaliderebbe la cache).
- Nessun riferimento a feature di un provider specifico.

## Tutor, Fase 3 (`tutor/`, versione attuale `tutor-issue30-v5`)

L'**Interviewer è una modalità del Tutor**, non un prompt separato (motivazione in `llm-port.md` §10bis.4). File:

| File | Ruolo |
|---|---|
| `system.md` | statico, cacheable (blocco A) |
| `context.md` | variabile (blocco B) |
| `tools.json` | `{version, tools:[{name, description, input_schema}]}`: i 4 tool (`save_mission_draft`, `propose_mission_change`, `request_lesson`, `save_preference`). Il backend lo incorpora con `include_str!` e lo mappa su `ToolDef::Function`. Lo stesso schema (min/max di lunghezza e numero) serve al backend per validare l'input e generare i messaggi di errore |
| `tool_results.json` | testi dei `tool_result` (per `request_lesson`: `ok`, `already_running`, `already_in_turn`, `not_ready`, `llm_key_missing`, `not_allowed_with_mission_tool`) per successo, errore di validazione, tool non ammesso, ecc. con segnaposto `{{...}}` |

Variabili di `context.md`:

| Variabile | Valore | Note |
|---|---|---|
| `{{topic_title}}` | titolo, senza ritorni a capo, max 200 car. | |
| `{{topic_state}}` | `new` · `interviewing` · `mission_proposed` · `researching` · `ready` | valore dell'enum del contratto, minuscolo |
| `{{history_note}}` | vuota, oppure `I messaggi più vecchi di questa chat sono stati omessi per brevità: se ti serve un dato che non vedi, chiedilo.` | solo se il troncamento ha scartato messaggi |
| `{{mission_section}}` | vedi sotto | |
| `{{preferences_section}}` | vedi sotto | |

### `{{mission_section}}`

Ritorni a capo dentro le voci sostituiti da spazio. Sezioni, nell'ordine, solo se presenti; se non c'è nulla: `Nessuna mission confermata e nessuna bozza.`

```
Mission confermata dalla persona (versione {{n}}):
Perché: {{why}}
Criteri di successo:
- {{voce}}
Punto di partenza: {{starting_point}}    (oppure "non dichiarato" se è null)
Vincoli:
- {{voce}}            (oppure "- nessuno")
Fuori scope:
- {{voce}}            (oppure "- nessuno")

Bozza in attesa di conferma (versione {{m}}): la persona la vede come card con Conferma e Modifica.
Perché: ...
(stesso formato)
```

Dalla Fase 12 la riga `Punto di partenza:` sta su **una sola riga** (ritorni a capo sostituiti da spazio, come `Perché`), dopo i criteri e prima dei vincoli, sia nella mission confermata sia nella bozza o proposta. `non dichiarato` è il testo per `null` (mission salvate prima della v1.2): il `system.md` del Tutor ne spiega il significato e come valorizzarlo (`propose_mission_change`). Anche `tools.json` ha `starting_point` obbligatorio (1-600 caratteri) in `save_mission_draft` e `propose_mission_change`, con `required` nell'ordine `why, success_criteria, starting_point, constraints, out_of_scope`: il messaggio d'errore per un campo mancante o troppo lungo si genera dallo schema, e `tool_results.json` aggiunge ai due `validation_error` il consiglio di non inventare il campo.

Se la bozza aperta è una proposta di cambio (esiste anche la confermata) l'intestazione è `Proposta di modifica in attesa di conferma (versione {{m}}), la mission {{n}} resta valida finché non conferma:`. Così il Tutor "vede" la conferma nel turno successivo: la bozza sparisce e compare "Mission confermata".

### `{{preferences_section}}`

`Nessuna.` oppure una riga per preferenza, `created_at` crescente: `- [tutti gli argomenti] testo` / `- [questo argomento] testo`. Massimo 20 (le più recenti); se ne mancano: prima riga `(mostrate le 20 più recenti)`. Ritorni a capo sostituiti da spazio.

Il testo finale del blocco di sistema è `system.md` seguito dal blocco `context.md` renderizzato, come **due blocchi di sistema separati** (vedi `llm-port.md` §6 e §10).

## Tutor, Fase 2 (storico)

| Variabile (`context.md`) | Valore | Note |
|---|---|---|
| `{{topic_title}}` | titolo del Topic | togliere i ritorni a capo; max 200 caratteri come da contratto |
| `{{topic_state}}` | `new` o `interviewing` | valore dell'enum in minuscolo, così com'è nel contratto. In pratica il Tutor vede sempre `interviewing` (il primo messaggio sposta il Topic) ma il prompt gestisce entrambi |
| `{{history_note}}` | stringa vuota, oppure `I messaggi più vecchi di questa chat sono stati omessi per brevità: se ti serve un dato che non vedi, chiedilo.` | valorizzata solo se il troncamento ha scartato messaggi |

Il testo finale del blocco di sistema è `system.md` seguito dal blocco `context.md` renderizzato, come **due blocchi di sistema separati** (vedi `llm-port.md` §6).

## Researcher, Fase 4 (`researcher/`, versione attuale `researcher-issue30-v11`)

| File | Ruolo |
|---|---|
| `system.md` | statico, cacheable (blocco A) |
| `context.md` | variabile (blocco B) |
| `output_schema.json` | `{version, name, schema}`: schema dell'output (sottoinsieme strict: `additionalProperties:false`, tutti i campi `required`, nessun `minLength`/`maxItems`). Le regole che lo schema non può esprimere (URL tra i risultati di ricerca, https, conteggi, nessuna community se `communities_skipped`) stanno nel Validator |
| `repair_system.md` | testo di sistema della chiamata di riparazione (unico blocco, senza segnaposto): la riparazione non riusa i blocchi A e B, riceve solo l'output da correggere, gli errori e gli URL ammessi (al massimo 80) |

Modifiche di `researcher-issue15-v1..v10` (issue 15, il Researcher consegnava 2-4 fonti): le ricerche si pianificano sui candidati noti **cercati per nome e sito** ("doc.rust-lang.org book ...", "rustlings"), con un primo giro di metà budget e il resto per i buchi; per le community una query sul tipo di luogo (forum, Stack Exchange) perché i subreddit non compaiono mai nei risultati dello strumento; prima di dichiarare una lacuna si rileggono tutti i risultati; il «meno è meglio» è limitato (sotto 5 fonti solo se la ricerca non le trova); regole di forma più esplicite (`use_for` nominale anche per le community, annotazione che nomina autore o ente e non cita il livello della persona, nota di lacuna max 25 parole, siti che vendono i propri programmi); v8: esempio del primo giro per un budget qualsiasi (con 6, tre ricerche), esempio di query su un forum e non su reddit, annotazione al massimo 50 parole; v9 (decisione del proprietario 2026-11-15): community **facoltative**, cercate e consegnate solo se una preferenza le chiede (senza richiesta `communities_skipped: true`, nessuna ricerca per le community, tutto il budget a fonti `knowledge` verificate, community già salvate scartate, nessuna lacuna sulle community non chieste); v10 (revisione): una sola regola per le community chieste, in un punto a parte (al massimo due ricerche e mai più di metà del budget: una nel primo giro col nome di un luogo con dominio proprio, la seconda solo se sono usciti thread ma non la pagina principale), i subreddit non si cercano. Misure in `evals/README.md`.

Il Researcher usa lo strumento di ricerca web **lato server** (vedi `llm-port.md` §12) con `output = Text`: il JSON finale si estrae dal testo e si valida con lo schema (non `output_config.format`, vedi §12.4).

Variabili di `context.md`:

| Variabile | Valore |
|---|---|
| `{{topic_title}}` | titolo, senza ritorni a capo, max 200 car. |
| `{{today}}` | data ISO (`2026-11-09`); serve a giudicare l'attualità delle fonti. Sta nel blocco variabile, mai in `system.md` |
| `{{mission_version}}` | numero della mission confermata |
| `{{mission_why}}` | `why`, ritorni a capo sostituiti da spazio |
| `{{mission_success}}`, `{{mission_constraints}}`, `{{mission_out_of_scope}}` | una riga `- voce` per elemento; `- nessuno` se vuoto |
| `{{mission_starting_point}}` | dalla Fase 12: `starting_point` su una riga (ritorni a capo sostituiti da spazio); `non dichiarato` se `null`. Sta dopo i criteri e prima dei vincoli |
| `{{preferences_section}}` | stesso formato del Tutor (`- [tutti gli argomenti] testo`), solo preferenze rilevanti o tutte (max 20); `Nessuna.` se vuoto. La richiesta di community (senza, il Researcher non le cerca) arriva da qui |
| `{{existing_resources_section}}` | `Nessuna.` oppure una riga per risorsa: `- [knowledge] Titolo — url — use_for` |
| `{{search_budget}}` | intero = `max_uses` dello strumento (6 in produzione dal 2026-11-15) |

`{{existing_resources_section}}`: una ricerca riuscita sostituisce in blocco fonti e gap; il Researcher riceve le esistenti (nell'ordine di importanza salvato) e consegna l'elenco finale completo.

Dopo la risposta: `application` valida, scarta/ripara e salva. Il Validator (codice) verifica gli URL contro l'insieme di quelli visti nei risultati di ricerca o tra le fonti esistenti; il prompt dice di non usare URL non visti ma non è una garanzia.

## Lingua dei contenuti (issue #30, ADR 0019)

I prompt restano scritti in italiano. La lingua dell'output (`italiano` o `inglese`, scelta per Topic) arriva come segnaposto **opzionale** `{{content_language}}`: il backend lo rende in ogni template che lo usa, e un template che non lo usa resta valido. `system.md` è statico e non lo contiene: dice «nella lingua indicata in «Lingua dei contenuti» nel contesto», e la lingua sta nel `context.md` o nel messaggio utente. Con `inglese` valgono le stesse regole in inglese (seconda persona «you», stessi limiti di lunghezza e di parole); gli esempi restano in italiano. Citazioni in altra lingua: `lang` solo se il contenuto non è nella lingua del topic, più parafrasi nella lingua del topic. Il Grader accetta risposte in qualsiasi lingua. Gli errori del Validator in `{{errors}}` restano in italiano.

| File | Dove sta `{{content_language}}` |
|---|---|
| `tutor/context.md`, `researcher/context.md`, `planner/context.md`, `generator/context.md`, `recorder/context.md` | sezione «Lingua dei contenuti» |
| `grader/context.md` | riga «Lingua del feedback» |
| `planner/repair.md`, `recorder/repair.md`, `generator/repair.md`, `generator/improve.md`, `generator/kickoff_web.md` | riga all'inizio del blocco degli errori / delle fonti |
| `tutor/tool_results.json`, `researcher/repair_system.md` | assente (statici: nessun segnaposto; `repair_system` non fissa una lingua d'output oltre al JSON) |

Versioni: `tutor-issue30-v5`, `researcher-issue30-v11`, `planner-issue30-v12`, `generator-issue30-v23`, `grader-issue30-v7`, `recorder-issue30-v9` (non misurate: l'eval di regressione sull'italiano e i casi in inglese spettano alla tappa successiva).

## Fasi 6-7: Planner, Generator, Grader, Recorder (segnaposto fissati)

Lista dei segnaposto di `context.md` per ruolo (nomi stabili; il formato esatto di ogni sezione è nel seguito di questo file). `repair.md` ha i propri segnaposto.

| Ruolo | `context.md` | `repair.md` |
|---|---|---|
| `planner/` | `{{topic_title}}` `{{mission_version}}` `{{mission_why}}` `{{mission_success}}` `{{mission_starting_point}}` `{{mission_constraints}}` `{{mission_out_of_scope}}` `{{preferences_section}}` `{{records_section}}` `{{glossary_section}}` `{{lessons_section}}` `{{resources_section}}` | `{{errors}}` `{{previous_output}}` |
| `generator/` | `{{topic_title}}` `{{mission_why}}` `{{mission_success}}` `{{mission_starting_point}}` `{{mission_constraints}}` `{{mission_out_of_scope}}` `{{preferences_section}}` `{{plan_section}}` `{{records_section}}` `{{sources_section}}` `{{excerpts_section}}` `{{glossary_section}}` `{{lesson_links_section}}` | `{{errors}}` `{{previous_document}}` |
| `grader/` | `{{lesson_title}}` `{{lesson_objective}}` `{{question}}` `{{rubric}}` `{{max_chars}}` `{{answer}}` | (nessuno) |
| `recorder/` | `{{topic_title}}` `{{lesson_number}}` `{{lesson_title}}` `{{lesson_objective}}` `{{mission_why}}` `{{plan_section}}` `{{lesson_glossary_section}}` `{{exercises_section}}` `{{records_section}}` `{{glossary_section}}` | `{{errors}}` `{{previous_output}}` |

Cartelle (versioni): `planner/` `planner-issue30-v6`, `generator/` `generator-issue30-v23`, `grader/` `grader-issue30-v6`, `recorder/` `recorder-issue30-v9`. Per ciascuna: `system.md` (statico, cacheable), `context.md` (variabile), `VERSION`; più `output_schema.json` (Planner, Grader, Recorder) e `repair.md` (Planner, Generator, Recorder). Il Grader non ha `repair.md` (vedi sotto).

**Avvertenze comuni**
- Sostituzione **a passata unica**: il valore inserito non viene riesaminato. Record, risposte e titoli possono contenere `{{...}}` o `[[...]]`: non sono segnaposto. Un segnaposto di `context.md`/`repair.md` non riconosciuto è un errore di avvio/test.
- `generator/system.md` contiene **testo letterale** `{{op_a}}` e `{{id}}` (sintassi dei vuoti di `fill_blank`): `system.md` non è mai un template, il backend non deve né sostituirlo né vietare `{{` al suo interno (il test «nessun segnaposto residuo» vale per `context.md` e `repair.md` renderizzati, non per `system.md`).
- Il testo finale è `system.md` (blocco A, `cache_breakpoint: true`) + `context.md` renderizzato (blocco B, `cache_breakpoint: true`) come due blocchi di sistema; la richiesta ha un solo messaggio `user` (vedi sotto per ruolo). La riparazione riusa **gli stessi due blocchi** (cache) e ha come unico messaggio `user` il `repair.md` renderizzato.
- Ritorni a capo: in tutte le voci elencate sotto sostituiti da uno spazio, salvo dove indicato. «Testo semplice» = markup tolto: `[[key|testo]]` → `testo`, `[[key]]` → il `term` della voce di glossario (o la key), `[^key]` rimosso, `**`/`*` rimossi, i backtick restano.
- `{{mission_starting_point}}` (Fase 12, Planner e Generator; anche Researcher): `starting_point` su una sola riga (ritorni a capo sostituiti da spazio, nessun troncamento: il contratto lo limita a 600 caratteri); la stringa `non dichiarato` se è `null`. Nel template sta tra i criteri e i vincoli.
- `{{mission_why}}`, `{{mission_success}}`, `{{mission_constraints}}`, `{{mission_out_of_scope}}`, `{{preferences_section}}`: identici al Researcher (`- voce` per elemento, `- nessuno` se vuoto; preferenze `- [tutti gli argomenti] testo` / `- [questo argomento] testo`, max 20, `Nessuna.` se vuoto, prima riga `(mostrate le 20 più recenti)` se troncate). `{{topic_title}}` come per gli altri ruoli. Nel Planner `{{mission_version}}` è il numero.
- Le sezioni vuote usano il testo indicato per ciascuna (`Nessuno.` / `Nessuna.`); mai una sezione con testo vuoto.
- **Risposta del modello (tutti i ruoli):** il backend estrae il JSON in modo tollerante (testo intero; altrimenti dalla prima `{` alla `}` bilanciata corrispondente, dopo aver tolto eventuali recinti ```), poi valida. Un JSON non parsabile o fuori schema conta come errore da riparare (per il Generator come errore `schema` con `path: ""`).

### Formati condivisi

**`{{records_section}}`** (Planner, Recorder): `Nessuno.` oppure una riga per record **attivo**, `number` crescente: `- [{number}] ({kind}, lezione {n}) {title}: {body}`. `body` troncato a 400 caratteri con `…`. Massimo 60 (i `number` più alti); se ce ne sono di più, prima riga `(mostrati i 60 più recenti di {totale})`. I numeri tra parentesi quadre sono quelli che il modello cita (`record_numbers`, `supersedes`).

**`{{glossary_section}}`** (Planner, Generator, Recorder): `Nessuno.` oppure una riga per termine, ordine alfabetico di `term` (senza maiuscole), poi `key`: `- [{key}] {term}: {definition}` più, se ci sono sinonimi da evitare, ` (evita: {a}; {b})`. `definition` intera (max 400). Massimo 80 voci (le ultime aggiornate); se troncato, prima riga `(mostrati 80 di {totale}, i più recenti)`. Nel Generator il modello deve copiare `key`/`term`/`definition`/`avoid` identici: per questo non si accorcia mai una definizione.

### Planner (`planner/`)

| Variabile | Formato |
|---|---|
| `{{records_section}}`, `{{glossary_section}}` | condivisi sopra |
| `{{lessons_section}}` | `Nessuna.` oppure `- {lesson_id} · lezione {number} · {status} · "{title}": {objective}`, `number` crescente, massime 20 (le più recenti; se troncate, prima riga `(mostrate le 20 più recenti di {totale})`). `status` ∈ `not_started` `in_progress` `completed`. Titolo e obiettivo senza ritorni a capo |
| `{{resources_section}}` | una riga per risorsa `knowledge` **con `url` https** (le `community` e le risorse senza URL non si passano), nell'ordine di importanza salvato, max 12: `- {id} · {title} — {url} — {annotation} Usala per: {use_for}`. Il backend non chiama il Planner se l'elenco è vuoto (`invalid_output` non ha senso: fallire prima con un messaggio chiaro) |

Risposta: `output_schema.json` nativo (vedi `llm-port.md` §13). Controlli del Validator del piano (codice): `primary_source_id` è id di una risorsa dell'elenco passato; `other_source_ids` ⊆ elenco, senza duplicati e senza la primaria (≤ 4); `lesson_links[].lesson_id` ∈ lezioni del Topic (≤ 2); `prerequisites[].record_numbers` ⊆ record attivi, non vuoto se `basis = record`; `terms.reuse` ⊆ chiavi del glossario; `terms.introduce[].key` formato `[a-z0-9_]{1,40}`, assente dal glossario, senza duplicati, ≤ 8 (Fase 12; il prompt ne chiede al massimo 2 per concetto); `concepts` 2-5 (al più 4 con un prerequisito `general`), con `example`, `steps` e `pitfall` (limiti nella sezione Fase 12 più sotto); `general` ≤ 3; `exercises` 2-5 con almeno uno `verifies_objective`; `estimated_minutes` 1-20 (il prompt lo calcola con la formula di Fase 12); lunghezze: `title` 1-120, `objective` 1-200, `why_now` 1-400. Errori resi in `{{errors}}` come `- {campo}: {messaggio in italiano}`. Una riparazione, poi `invalid_output`. `{{previous_output}}`: il JSON del piano.

### Generator (`generator/`)

| Variabile | Formato |
|---|---|
| `{{plan_section}}` | il piano reso come testo (sotto) |
| `{{sources_section}}` | solo le fonti del piano (primaria e altre), primaria per prima: `- [PRIMARIA] {id} · {title} — {url} — {annotation} Usala per: {use_for}` / `- [utile] ...`. Il modello copia `id` in `resource_id`, `title` e `url` |
| `{{excerpts_section}}` | oggi `Nessuno.`. Predisposto per estratti delle fonti (vedi rischio in `llm-port.md` §13): quando esisteranno, un blocco per fonte `### {id}` seguito dal testo (max 3000 caratteri per fonte, max 6 fonti). Il prompt distingue già i due casi |
| `{{glossary_section}}` | condiviso (tutto il glossario, non solo `terms.reuse`: è il linguaggio canonico) |
| `{{lesson_links_section}}` | `Nessuna.` oppure `- {lesson_id} · lezione {number}: "{title}" — {reason}` per le sole lezioni di `plan.lesson_links` |

`{{plan_section}}` (dalla Fase 12; formato esatto sotto, nella sezione «Fase 12»):

```
Titolo di lavoro: {title}
Obiettivo: {objective}
Perché adesso: {why_now}
Durata stimata: {estimated_minutes} minuti
Prerequisiti da richiamare (nessuna prova: serve la sezione di richiamo):
- {statement}            (oppure "- nessuno")
Prerequisiti dichiarati dalla persona (nessuna sezione, richiamo di una clausola):
- {statement}            (oppure "- nessuno")
Prerequisiti dimostrati (non si ripassano):
- {statement} [record {n}, {n}]   (oppure "- nessuno")
Concetti (uno per sezione, in quest'ordine):
1. {name}: {key_point}
   Esempio: {example}
   Passaggi:
   1) {step}
   2) {step}
   Errore tipico: {pitfall}   (oppure "nessuno")
Termini da introdurre:
- [{key}] {term} — {hint}   (oppure "- nessuno")
Termini già nel glossario da riusare (key):
- {key}                  (oppure "- nessuno")
Esercizi previsti, in ordine:
- {type} — {purpose}{ " (verifica l'obiettivo)" se verifies_objective }
```

(Gli id di fonti e di lezioni non compaiono nel piano reso: stanno nelle sezioni delle fonti e dei collegamenti.)

Output: **emulato** (`OutputFormat::Text`), il modello restituisce il solo documento JSON. Validator reale (`sdui.md` §6, contesto: `resource_id` delle risorse del Topic, `lesson_id` delle lezioni del Topic; il glossario del Topic non basta, `lessons.md` §4), fino a 2 riparazioni. `repair.md`: `{{errors}}` = una riga per errore, `- {path} [{code}] {message}`, al massimo 30 righe (poi `- (altri {n} errori omessi)`), nell'ordine del Validator; `{{previous_document}}` = il JSON del documento precedente come ricevuto (compatto), troncato a 40.000 caratteri; oltre, `(omesso perché troppo lungo: riscrivi la lezione più corta)`. Se l'errore è `too_long` con `path: ""` (256 KiB), omettere il documento. Dopo la riparazione il Validator può trovare errori nuovi (ordine di validazione, `sdui.md` §9.22): il testo di `repair.md` lo dice.

### Grader (`grader/`)

| Variabile | Formato |
|---|---|
| `{{lesson_title}}`, `{{lesson_objective}}` | dal documento, una riga |
| `{{question}}` | `question` del blocco in **testo semplice** |
| `{{rubric}}` | `rubric` così com'è (ritorni a capo mantenuti, max 1500) |
| `{{max_chars}}` | intero, `max_chars` del blocco |
| `{{answer}}` | `answer.text` già trimmato, così com'è, con i ritorni a capo; le sequenze `<<<` e `>>>` sostituite da `‹‹‹` e `›››` (il testo è racchiuso da delimitatori `<<<RISPOSTA` / `RISPOSTA>>>`) |

Risposta: `output_schema.json` nativo. Controlli (codice): `verdict` ∈ enum; `feedback` 1-800 caratteri dopo trim; `misconception` `null` oppure 1-300. Se il controllo fallisce o il tempo (20 s) scade il tentativo resta `pending` (`lessons.md` §5.1): **nessuna riparazione** (non c'è tempo) e quindi nessun `repair.md`. Nel job `lesson_completion` (nessun tetto di 20 s) basta riprovare la stessa chiamata una volta. Il `feedback` tratta la misconcezione al suo interno in seconda persona (la persona non vede il campo `misconception`, terza persona, dato per Recorder e Planner). Va salvato come ricevuto; il prompt vieta già `[[ ]]`, `[^ ]`, link e a capo, che il codice può anche ripulire.

### Recorder (`recorder/`)

| Variabile | Formato |
|---|---|
| `{{lesson_number}}`, `{{lesson_title}}`, `{{lesson_objective}}`, `{{topic_title}}` | una riga |
| `{{mission_why}}` | come sopra |
| `{{plan_section}}` | versione ridotta del piano: `Perché adesso: {why_now}` poi `Concetti:` e `- {name}: {key_point}` |
| `{{lesson_glossary_section}}` | glossario del **documento** della lezione, formato di `{{glossary_section}}` (`- [{key}] {term}: {definition}`), `Nessuno.` se vuoto |
| `{{exercises_section}}` | un blocco per esercizio del documento, in ordine di documento, **inclusi quelli senza tentativi** e quelli dentro un `fallback` (formato sotto) |
| `{{records_section}}`, `{{glossary_section}}` | condivisi; i record sono gli **attivi** prima di questa lezione |

Formato di un esercizio (testo semplice, valori troncati a 200 caratteri tranne dove indicato):

```
[{id}] {type} · {obbligatorio|facoltativo}
Domanda: {question | prompt | front | testo con vuoti resi come ____}
Corretta: {testo della/e opzione/i corretta/e | risposte accettate per vuoto | ordine corretto "1) … 2) …" | retro della scheda}
Rubrica: {rubric}                         (solo open_answer, max 1500)
Tentativo 1: {risposta resa in modo leggibile} → {outcome}
Tentativo 2: ...
Valutazione: {verdict}; misconcezione: {misconception | nessuna}      (solo open_answer valutata)
```

- `quiz_*`: la risposta è il testo delle opzioni scelte (`"testo"`; più opzioni separate da `; `). `fill_blank`: `{blank}="valore"`. `order_steps`: l'ordine dato come testi `1) … 2) …`. `flashcard`: `autovalutazione {rating}`. `open_answer`: testo della risposta fino a 600 caratteri (`…` se tagliato), tra virgolette; `→ pending` se non valutata (e la riga `Valutazione` manca).
- Esercizio senza tentativi: riga `Nessun tentativo.` al posto dei `Tentativo`.
- `{outcome}` è quello salvato (`correct`/`partial`/`incorrect`/`pending`).
- Nessun testo di feedback delle opzioni e nessuna prosa della lezione: il Recorder giudica dalle prove, non dalla spiegazione.

Risposta: `output_schema.json` nativo. Il Validator è quello di `lessons.md` §6.3, più: formati e lunghezze di §2 e §6.1; `evidence_blocks` 1-4 id distinti; `supersedes` senza duplicati; `cards[].record_index` indice valido dell'array `records` **oppure** `cards[].term_key` chiave di un elemento di `terms` (esattamente uno dei due non `null`); chiavi dei termini uniche nella risposta. Una riparazione, poi `invalid_output`. `{{errors}}`: `- {campo}: {messaggio}` (es. `records[1].evidence_blocks[0]: l'esercizio q9 non esiste`), max 30; `{{previous_output}}`: il JSON.

Persona grammaticale: `title` dei record impersonale (idea giusta), `body` ed `evidence` in **seconda persona** (`lessons.md` §2: l'app li mostra alla persona), `definition` dei termini impersonale, `front`/`back` delle schede liberi ma in lingua diretta (quella del topic); `misconception` del Grader in terza persona. Gli id degli esercizi stanno solo in `evidence_blocks` (un controllo del backend può rifiutare `\b(id)\b` in `body`/`evidence`).

Suggerimento di costo (per rust-backend-dev): se nessun tentativo della lezione è `correct`, il Recorder non può produrre nulla di valido: il job può saltare la chiamata e chiudersi con zero record/termini/schede.

## Fase 10: PDF da studiare (versioni `tutor-phase10-v1`, `researcher-phase10-v1`, `planner-phase10-v2`, `generator-phase10-v2`)

I documenti caricati entrano nei prompt così (formati in `crates/application/src/document_context.rs`):

| Ruolo | Come | Note |
|---|---|---|
| Tutor | niente segnaposto in `context.md`: il backend appende in fondo `## Documenti caricati` solo se ce ne sono (`render_with_documents`) | un segnaposto con titolo proprio farebbe comparire «Documenti caricati» anche con `Nessuno.` e rompe il test `the_tutor_sees_the_index_of_ready_documents`; `system.md` ha la sezione «Documenti caricati» che spiega al modello come usare titolo, pagine e indice (chiede il perimetro con opzioni A/B/C, mette la risposta nella mission, non inventa contenuti) |
| Researcher | `{{documents_section}}` in `context.md`, prima di «Risorse già salvate» | completare, non duplicare; non consegna né tocca i documenti |
| Planner | `{{documents_section}}` in `context.md`, prima di «Fonti disponibili»; nuovo campo `document_sections` in `output_schema.json` | dalla Fase 12 il campo è **in `required` nello schema** (più portabile verso provider con strict "tutto required"): il Planner scrive `[]` quando non serve; un piano senza il campo non supera il controllo dello schema (va in riparazione), mentre `validate_plan` senza schema lo leggerebbe come vuoto. I piani fissi delle suite di eval lo hanno sempre |
| Generator | `{{excerpts_section}}` (già esistente) | regole nella sezione «Quando la lezione segue un PDF della persona» di `system.md`; fonti PDF con `url: null` e pagine nel `title` |

Cache: il testo statico (`system.md`) è cresciuto di circa 1,5k token per Generator e Planner; il contesto variabile resta dopo. Nessuna regola dipende da un provider.

## Fase 12: lezioni più ricche (versioni `planner-phase12-v1`, `generator-phase12-v1`)

Obiettivo: lezioni da 10-15 minuti, ogni concetto spiegato per intero (intuizione, esempio svolto, regola, errore tipico), prerequisiti non dimostrati richiamati in apertura. Specifica didattica: `docs/design/generator-guidelines.md` §7 e `docs/design/components/README.md` §1.1-1.4. I numeri sono nei prompt; le regole verificabili dal codice sono anche negli eval (`evals/README.md`, «Fase 12»).

**Planner (`output_schema.json`).** Ogni elemento di `concepts` ha ora `{name, key_point, example, steps, pitfall}` (tutti `required`, `additionalProperties: false`): `example` caso concreto con valori, diverso da quello degli esercizi (1-300 car.); `steps` 2-6 passaggi in ordine (1-150 car. ciascuno); `pitfall` la convinzione errata tipica in una frase (0-200 car.), **stringa vuota se non c'è un errore reale**. `basis: general` significa «da richiamare» (non dimostrato né dichiarato). Limiti nuovi dichiarati nel prompt e da applicare in `validate_plan`: concetti 2-5 (4 se c'è un `general`), `steps` 2-6, `terms.introduce` al massimo 8 (2 per concetto), `general` al massimo 3, esercizi 2-5 (almeno uno per concetto, solo avviso). `estimated_minutes` = `round(P/250 + 0,5·E + 2·O)` (il prompt dà la stima di `P`); il prompt non lo vincola a 10-15 se la formula dà meno.

**`{{plan_section}}`** (Generator): formato nel blocco sopra. Regole di resa: `{statement}` e testi su una riga (come oggi `one_line`); prerequisiti divisi per `basis` (`general` → «da richiamare», `mission` → «dichiarati dalla persona», `record` → «dimostrati», con `[record n, m]`); ogni gruppo vuoto stampa `- nessuno`; concetti numerati da 1 con i tre campi indentati di 3 spazi; `Errore tipico: nessuno` quando `pitfall` è vuoto o solo spazi; `Passaggi` numerati `1)`, `2)`…

**`{{records_section}}`** (Generator, nuovo): `Nessuno.` oppure una riga per record **attivo**: `- [{number}] ({kind}, lezione {n}) {title}: {body}` (come per il Planner, `number` crescente, `body` troncato a 300 caratteri con `…`). Massimo 15 righe: prima i record citati dai prerequisiti `record` del piano, poi i più recenti per arrivare a 15; se ce ne sono più di quelli mostrati, prima riga `(mostrati {k} di {totale}: quelli citati dal piano e i più recenti)`. Sezione `Nessuno.` anche se l'argomento ha record ma tutti sostituiti.

**Cache.** `system.md` del Generator è cresciuto di circa 2k token (statico, resta prima); `plan_section` e `records_section` sono variabili e stanno dopo mission e preferenze. Nessuna regola dipende da un provider.

**Tetti di token consigliati.** Planner: 8.000 token di output (JSON da 5 concetti con `example` e `steps` ~1.500 token più ragionamento). Generator: effort `low` e tetto di output 32.000 (`[llm.roles.generator] max_output_tokens`; lezione da ~1.800 parole di prosa: 5-8k token totali misurati con `low`; con `medium` il ragionamento portava a 10-20k, vedi «correzione dopo i primi eval reali»). La riparazione riscrive il documento intero: stesso tetto. (Versione precedente di questa riga: 20.000 token con effort `medium`, superata.) Modello: Planner e Generator restano di fascia alta (Sonnet o equivalente; Opus solo se gli eval mostrano che Sonnet non regge la catena); Haiku solo per prove.

## Fase 12, blocco B: punto di partenza della mission (`starting_point`)

Contratto: `docs/contracts/mission.md` v1.2, §2 e §8. Versioni: `tutor-phase12-v1`, `researcher-phase12-v1`; Planner e Generator restano `planner-phase12-v1` e `generator-phase12-v1` (mai misurati né rilasciati: il testo è cambiato ma non c'è una v1 di confronto).

| Ruolo | Segnaposto aggiunto / resa | Regola d'uso (nel `system.md`) |
|---|---|---|
| Tutor | nessun segnaposto nuovo: la riga `Punto di partenza: …` entra in `{{mission_section}}` (formato sopra) | la domanda è obbligatoria prima di proporre; una risposta vaga richiede un approfondimento concreto; `starting_point` non finisce in `constraints`; con «non dichiarato» propone un cambio (una volta) quando la persona racconta che cosa sa |
| Planner | `{{mission_starting_point}}` in `context.md` | un prerequisito coperto dal punto di partenza è `basis: mission`; ciò che dice di non sapere o non nomina è `general`; con `non dichiarato` vale solo la regola prudente (senza prove, `general`) |
| Generator | `{{mission_starting_point}}` in `context.md` | esempi e analogie agganciati a ciò che la persona sa; dichiarato non vuol dire dimostrato (clausola di richiamo, mai «come sai già»); il confronto non porta fatti non sostenuti dalle fonti |
| Researcher | `{{mission_starting_point}}` in `context.md` | livello delle fonti adatto al punto di partenza; non va citato nelle annotazioni |

Recorder e Grader non lo ricevono (non ne hanno bisogno: lavorano sulla lezione e sulla risposta). Cache: il testo è nel blocco variabile, `system.md` è cresciuto di poche decine di token per ruolo.


## Generator, Fase 12 blocco C: lettura delle fonti web (`generator-phase12-v2`)

Specifica: ADR 0009 e `llm-port.md` §14. Il testo di `generator/system.md` ha una sezione «Quando hai lo strumento di lettura web» (statica, vale solo se lo strumento c'è **e** il messaggio utente ha «Fonti da leggere»): nessuna versione doppia del file e nessun segnaposto in `system.md` o `context.md`. La regola 9 è unificata con gli estratti dei PDF («testo di una fonte»).

**Nuovo template `generator/kickoff_web.md`** (un segnaposto, `{{web_sources_list}}`). Gli URL devono stare nel **messaggio utente**, non nel system prompt: lo strumento scarica solo URL comparsi in un messaggio utente o in un risultato di strumento, e oggi `{{sources_section}}` sta nel blocco di sistema (`context.md`) mentre il messaggio utente è la costante `Procedi.`. Con lo strumento attivo il backend invia come unico messaggio utente `Procedi.` + `\n\n` + `kickoff_web.md` renderizzato; con lo strumento spento invia `Procedi.` come oggi (il template non si renderizza).

`{{web_sources_list}}`: una riga per fonte web **scelta dal piano** (primaria e `other_source_ids`, kind `knowledge` con URL https, **non** i PDF caricati, che hanno già gli estratti), nell'ordine di `{{sources_section}}` (primaria prima), al massimo 3 (= `max_uses`): `- {resource_id} · {title} — {url}`, titolo su una riga (`one_line`), URL esattamente come in `sources_section` (stessa stringa che sta in `allowed_domains` come host). Nessuna annotazione (sta già nel contesto). Se l'elenco è vuoto (piano con soli PDF) lo strumento **non** si invia e il template non si renderizza.

`repair.md`: nessun segnaposto nuovo; la riparazione prosegue la conversazione con i blocchi dello strumento (vedi `llm-port.md` §14.3).


## Fase 12, correzione dopo i primi eval reali (`generator-phase12-v3`, `planner-phase12-v2`)

Misurato il 2026-11-11 (Sonnet 5.5, Anthropic diretto): con `generator-phase12-v2` ed effort `medium` il modello spendeva il 60-80% dei token di output in ragionamento (documento ~3-4k token su 10-20k) e con 4-5 concetti esauriva il tetto di 20.000 senza scrivere il documento. Dettagli e tabella in `evals/README.md`.

**Generator v3** (cambi in `generator/system.md`, stessi segnaposto):
- In cima, «Come lavorare»: scrivere, non pianificare; niente conti di parole e blocchi (li fa il Validator); una lezione corta è un difetto, non un'economia.
- Tolti il «controllo finale» di 14 domande (induceva a ripassare tutto nel ragionamento), la formula di `estimated_minutes` (ora «copia il valore del piano»; il calcolo spetta al backend) e il paragrafo «Lavora in economia».
- Lunghezza espressa come obiettivo per concetto (~450 parole: spiegazione 270-300, esercizi 150-200) invece di una cascata di intervalli; intervalli dei paragrafi alzati (intuizione 95-115, esempio 115-140, regola 60-80) perché il modello scrive sistematicamente il 15-20% sotto l'intervallo chiesto.
- **Una sezione di concetto completa come esempio** (argomento inventato, SQL `NULL`, ~420 parole) al posto di parte delle regole numeriche.
- Quiz: «tutte le opzioni di 6-9 parole» (garantisce lo scarto di al massimo 3 parole del Validator: con effort basso la regola «entro il 30%» non bastava e due riparazioni di fila non l'hanno corretta).
- Con 3 concetti o meno, due esercizi per sezione di concetto.
Il testo resta ~40 KB (l'esempio compensa i tagli): costo fisso dell'input già coperto dalla cache (lettura a 0,10 $ per milione).

**Configurazione richiesta**: `[llm.roles.generator] effort = "low"` (con `medium` il ragionamento torna a 6-16k token). Il tetto di output di 20.000 in `LessonSettings` va alzato (vedi richieste in `evals/README.md`): è una rete di sicurezza, non il tetto atteso.

**Planner v2**: prima dei concetti elenca per ognuno che cosa serve per capirlo e confrontalo con record e punto di partenza (un elenco di prerequisiti vuoto è corretto solo se tutto è coperto); uno strumento che la persona dice di non aver mai usato è `general`; formula di `estimated_minutes` in forma già semplificata (`round(0,9 + 1,8·C + 0,8·G + 0,5·E + 2·O)`). Resta la richiesta di farla calcolare al backend.

## Fase 12, 4-5 concetti per lezione (`planner-phase12-v3`, `generator-phase12-v4`)

Decisione del proprietario (2026-11-11), dopo la misura sui piani da 2-3 concetti: con effort `low` il Generator scrive ~330-380 parole di prosa per concetto, quindi 759, 1014 e 1181 parole con 2-3 concetti (minimo 1200); con 4-5 concetti si sta nei limiti. **Ogni lezione ha 4 o 5 concetti** (esattamente 4 con un prerequisito `general`); `validate_plan` lo impone.

**Planner v3** (`planner/system.md`):
- Regola 6 riscritta: 4-5 concetti sempre, con il **test di ammissione** in quattro condizioni (esercizio che richiede un ragionamento proprio, serve all'`objective`, non anticipa un'altra lezione, `example` proprio) e l'ordine di preferenza per arrivare a 4 su argomenti stretti (prerequisito promosso, caso limite o applicazione con ragionamento nuovo, confronto con un modello noto dal punto di partenza), con i divieti (stesso concetto con altro esempio, fuori mission, anticipazioni, riassunti). Argomento troppo stretto: si allarga l'`objective` a uno vicino, non si riempie. Il motivo di ammissione resta nel ragionamento, **nessun campo nuovo nello schema** (regola in forma compatta: il Planner gira a effort `medium`).
- Regola 7 (concetti di supporto): richiamo i limiti nuovi (5, o 4 con un `general`) e il test.
- Regola 10: un solo esercizio per concetto (2-5 esercizi in tutto).
- Regola 12 (durata): tolta la formula; `estimated_minutes` è un valore qualsiasi valido (esempio 10) perché il backend lo ricalcola sul piano. Il campo resta nello schema.

**Generator v4** (`generator/system.md`): tolta la regola «con 3 concetti o meno, due esercizi per sezione»; obiettivo di lunghezza ricalibrato a ~380 parole per concetto (spiegazione 250-300, esercizi 100-150; con 4-5 concetti ~1700-2100 parole; prima ~450 per concetto, che con 5 concetti avrebbe superato il limite di 2200); `estimated_minutes`: copia il valore del piano o 10, il backend lo sostituisce. Dopo la prima misura: opzioni dei quiz di **7 o 8 parole contate una per una** (prima 6-9: il modello ne scriveva 5-9 e `quiz_option_length` falliva due volte di fila sullo stesso quiz) e in `repair.md` la regola di riscrivere tutte le opzioni del quiz nominato dall'errore. I casi `narrow_plan_four_concepts_no_filler` e `five_concepts_no_recall_block_budget` sono stati misurati con il testo v4 prima di questo ritocco; il caso PDF con il ritocco.

**Cache e costi.** Nessun cambio di struttura: il prefisso statico resta `system.md`. Planner: Sonnet 5.5, effort `medium` (JSON da 4-5 concetti ~1.500 token più ragionamento). Generator: Sonnet 5.5, effort `low`, tetto 32.000.

## Planner v4: contro la prima risposta «scheletro» (`planner-phase12-v4`, 2026-11-11, NON MISURATA)

Diagnosi del backend: in circa 3 prove su 10 la prima risposta del Planner è lo scheletro dello schema (campi vuoti, `estimated_minutes` 10, chiavi in ordine alfabetico, `EndTurn`, ~115 token). Modifica: sezione finale «Prima di rispondere» in `planner/system.md` (un piano con campi vuoti non è una risposta, va compilato per intero ora) e frase di chiusura rafforzata in `planner/context.md` (nessun segnaposto nuovo). **Questa modifica non è stata misurata**: nessuna chiamata reale. Sulla frase «es. 10» di `estimated_minutes`: non è la causa, perché lo scheletro compare già nei rapporti `2026-11-11_07-13-43-669` e `08-40-48-960`, quando il prompt chiedeva ancora la formula; resta invariata. Se la modifica non basta, la leva è il messaggio utente («Procedi.», `KICKOFF` in `application/src/generation.rs`): vedi report a rust-backend-dev.

## Diagrammi nel Generator (issue #3; `generator-phase12-v6`)

**v6 (2026-11-12, NON MISURATA)**: allineamento alle «Linee guida per il Generator» di `docs/design/components/diagram.md`. Solo `generator/system.md` (sezione «Figura»): quando sì/no con la prova della `caption` in una frase; posizione subito dopo il paragrafo che annuncia la struttura (`tree`/`cycle` dopo l'intuizione, `flow` dopo l'esempio svolto, prima della regola; prima era «dopo la regola»); tetto 1 per sezione e 2 per lezione (3 se il tema è una struttura); nodi consigliati per layout (`flow` max 2 per livello e nessun arco che salta un livello); limiti di lunghezza consigliati più stretti del contratto (label 24, detail 40, label degli archi 20, title 50, caption 140 caratteri); `alt` autosufficiente con layout, ordine di lettura e `detail`; la figura non anticipa un esercizio sulla stessa struttura. Nessuna contraddizione col contratto: i limiti del designer sono consigli dentro i limiti duri (40/60/30/80/300), che restano al Validator. Compatibilità con `concept_chain`: `chain_problems` in `crates/evals/src/roles/shape.rs` filtra i `diagram` prima di controllare la catena, quindi qualunque posizione dentro la sezione di concetto la rispetta (test `a_diagram_does_not_break_the_chain`). `repair.md` invariato rispetto a v5. Nessun eval reale eseguito.

**v5 (2026-11-12, NON MISURATA)**: introduzione del blocco `diagram`. Contratto: `docs/contracts/sdui.md` §4 e §10 (v1.3), ADR `docs/adr/0010-diagrammi-sdui.md`. Cambiano solo `generator/system.md` e `generator/repair.md`; `context.md` e i segnaposto sono invariati.

- `system.md`: catalogo con `diagram` (voce 17: campi e limiti esatti dello schema), sezione «Figura» in «Come scrivere bene ogni esercizio» (quando sì e quando no, posizione dopo la regola, testi, struttura per `layout`, divieto di anticipare la risposta di un esercizio, un esempio corretto e gli errori tipici), riga «Figure» nelle regole del Validator (`diagram_structure`), `missing_component` aggiornato (un processo, una gerarchia o un ciclo non sono più un bisogno mancante; restano disegno libero, grafico con assi, sequenza tra attori, linea del tempo), punto 6 e punto 10 (budget) col riferimento alla figura.
- `repair.md`: istruzioni per `diagram_structure`, nell'ordine più semplice (togliere o correggere l'arco, passare a `cycle` o `flow`, togliere la figura se non si può correggere senza inventare).
- Cache: `system.md` cresce di circa 1,2k token (statico, resta prima del contesto variabile). Nessuna regola dipende da un provider.
- **Dipendenza dal backend.** Il prompt non legge `output_schema.json`: il Generator risponde con testo JSON e lo schema è controllato dal Validator in Rust (`schema`). Se `rust-backend-dev` introduce uno schema di output strutturato del Generator che elenca i tipi di blocco, deve contenere `diagram` con gli stessi campi del §4; il prompt non lo duplica e non cambia.
- Modello consigliato invariato (Generator: fascia alta, Sonnet o equivalente, effort `low`); la figura aggiunge 150-400 token di output per lezione.
- Eval: `evals/generator/diagram.cases.json` (vedi `evals/README.md`, «Diagrammi»).


## Generator, opzioni dei quiz di pari lunghezza in caratteri (issue #16; `generator-phase12-v9`)

Misurato il 2026-11-15 con `v7`: `quiz_option_char_ratio_max` (1,3) falliva in 4 run su 7, con rapporti 1,32-1,62, perché la regola contava le parole e non i caratteri. In `system.md` («Quiz») le opzioni devono ora avere 7 o 8 parole e al massimo 10 caratteri di differenza tra la più lunga e la più corta, la corretta non oltre la mediana (un primo tentativo con la fascia 55-70 caratteri non è stato seguito dal modello: lunghezze 34-58). `repair.md` ha una riga per `quiz_option_length_cue`. Risultati: `evals/README.md`.

## Generator, attribuzioni con il web fetch (issue #16, tappa B; `generator-phase12-v10`)

Misurato il 2026-11-15 (`generator/phase12-fetch`, `max_fetches` 3): la rubrica `page_content` era 0,23-0,33 (gemello senza fetch 0,60): `[^key]` su regole che stanno in un'altra pagina letta o che il modello ricordava, e contenuti chiave della pagina mancanti (le tre condizioni della data race). Con la fonte irraggiungibile la lezione citava `[^key]` sugli esempi svolti e diceva cosa «spiega» la fonte. In «Quando hai lo strumento di lettura web» (`system.md`): `[^key]` di una pagina letta solo su ciò che quel testo dice (e se ne sa indicare la frase), uso dei fatti centrali (elenchi completi, messaggi, codici) e, a lettura fallita, citazione come con la sola annotazione, senza dire cosa contiene e senza `[^key]` sugli esempi. `repair.md` ripete la regola sulla lettura fallita. Il paragrafo «Quiz» della tappa A non è toccato. Cache: `system.md` cresce di circa 300 token (prefisso statico).

## Fase 17: lezione richiesta dalla chat (versioni attuali `tutor-phase17-v4`, `tutor-tools-phase17-v4`, `tutor-tool-results-phase17-v3`; v2 = solo `request_lesson`, v3 = correzioni dei difetti di `tutor/phase3`, v4 = eccezione al cambio immediato quando il punto di partenza è «non dichiarato»)

Contratto: `docs/contracts/lessons.md` §12, `mission.md` §6. Solo il Tutor cambia.

| Ruolo | Cosa cambia | Perché |
|---|---|---|
| Tutor | nuovo tool `request_lesson` (nessun argomento, `additionalProperties: false`), con esiti `ok`, `already_running`, `already_in_turn`, `not_ready`, `llm_key_missing`, `not_allowed_with_mission_tool` in `tool_results.json` (tutti senza segnaposto). Nuova sezione «Chiedere la prossima lezione» in `system.md`: solo con stato `ready` e richiesta o sì esplicito, mai di iniziativa, mai per un argomento scelto dalla persona (lo sceglie il Planner), mai nel turno di un tool di mission, nessun testo prima del tool e una riga dopo l'esito (v2: con la riga prima, il turno dopo `ok` era senza nulla da scrivere e il modello richiamava il tool, 3 run su 3 in v1), niente tempi né contenuto (la card li mostra). Può proporla una volta sola dopo la conferma | una lezione è un job a pagamento: l'ammissibilità vera la verifica il backend, il prompt evita le chiamate inutili |
| Tutor | `context.md` ha ora `{{documents_section}}` sotto `## Documenti caricati`, dopo la Mission; senza documenti il valore è `Nessuno.` e l'intestazione resta | il segnaposto rende il punto esplicito; il test `the_tutor_sees_the_index_of_ready_documents` va aggiornato (non asserisce più l'assenza di «Documenti caricati»). `system.md` dice «Nessuno.» al posto di «la sezione manca» |
| Tutor (v3, correzione dei 4 casi di `tutor/phase3`) | (1) `save_mission_draft`/`propose_mission_change`: nessun testo prima dello strumento, l'introduzione (1-2 frasi, max 40 parole, senza citare punto di partenza, vincoli, criteri) si scrive dopo l'esito, come per `request_lesson`; gli esiti `ok` lo dicono e vietano altri strumenti. (2) `constraints` deve contenere scadenze e tempo detti dalla persona anche se già nel `why`. (3) richiesta esplicita di cambio mission con nuovo obiettivo: `propose_mission_change` subito, senza intervista (criteri solo dalle parole della persona, `out_of_scope` contraddetti rimossi). (4) il riconoscimento cita 1-2 dati, non tutti. (5) criteri senza dettagli aggiunti | (1) con il testo prima il modello lo ripeteva dopo l'esito (messaggio doppio) o chiamava altri strumenti (`request_lesson`, `save_preference` con testo «placeholder»); nominare gli strumenti vietati nel prompt peggiorava (priming): ora si dice solo cosa fare. (2)-(5) misurati in `evals/README.md`, Fase 17 bis |
| Tutor (v4, eccezione al cambio immediato) | Richiesta esplicita di cambio con mission confermata dal punto di partenza «non dichiarato» e persona che non ha ancora detto cosa sa: nessuna chiamata, una sola domanda su cosa sa già, poi la proposta (in `system.md` e nella `description` di `propose_mission_change`) | La v3 («chiama subito») sovrascriveva la regola della Fase 12 (`phase12` `change_requested_with_undeclared_starting_point_asks_first` 0/1 → v4 3/3); eccezione scritta come condizione, senza nominare strumenti vietati. Misure in `evals/README.md`, Fase 17 ter |

Evals: `evals/tutor/phase17.cases.json` (3 casi, eseguiti il 2026-11-18: v1 `explicit_request_calls_tool` 0/3, v2 tutti 3/3; dettagli in `evals/README.md`, Fase 17).
