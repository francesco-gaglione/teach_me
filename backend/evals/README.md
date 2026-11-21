# Eval

Proprietario: `llm-engineer`. Nessuna chiamata reale nei test del backend: gli eval si lanciano a mano (o in una pipeline dedicata) con una chiave vera. Non è ancora presente un runner: qui ci sono formato e casi.

## Layout

```
evals/
  <ruolo>/<suite>.cases.json     casi di una suite
  results/                       (git-ignored) risultati delle esecuzioni
```

## Formato di una suite (`*.cases.json`)

- `defaults.runs_per_case`: ripetizioni per caso (la variabilità conta: si valutano più run, non una).
- `defaults.deterministic` e `cases[].deterministic` (il secondo sovrascrive il primo chiave per chiave):
  - `max_words`: massimo di parole;
  - `max_question_marks`: massimo di `?` (2: la seconda domanda è ammessa solo come precisazione, lo verifica la rubrica);
  - `language`: `"it"` o `"en"` (euristica su stopword; basta a intercettare risposte nell'altra lingua; dal 2026-11-21, issue #30);
  - sottoinsieme markdown ammesso dall'app (`docs/design/screens/chat.md` §12): `no_headings` (righe che iniziano con `#`), `no_tables`, `no_blockquotes`, `no_html_or_images`, `no_bare_urls` (ammessi solo `[testo](https://...)`), `max_bold_spans`, `max_list_items`, `code_blocks_need_language`, `max_code_line_chars`, `no_exclamation_marks`;
  - `must_not_match`: regex vietate (sintassi `regex` di Rust, `(?i)` ammesso).
- `cases[]`: `topic` (titolo e stato dati al Context Assembler), `messages` (cronologia; l'ultimo è sempre `user`), `rubric`. `content_language` facoltativo (`"it"` predefinito, `"en"`; ogni altro valore è un errore): lingua dei contenuti del Topic, usata per seminare il Topic, per `{{content_language}}` nei prompt reali dei sei ruoli e per i controlli sensibili alla lingua (`language` del Generator, forma e conteggi di `shape`, `annotation_language` del Researcher, avvisi di qualità). I controlli `deterministic` NON cambiano da soli: un caso inglese deve avere `content_language: "en"` **e** il proprio `"language": "en"` (i default delle suite dicono `"it"`), più le regex proprie della lingua (`must_not_match`, `forbidden_phrases`, `body_must_not_match`, `card_text_must_not_match`, `title_must_not_match`, `use_for_must_not_match`, `annotation_must_not_match`, `gap_note_must_not_match`, `misconception_third_person`, `pitfall_form`, `no_nearly_all_or_none_of_the_above`). Il controllo `body_and_evidence_second_person` del Recorder è un'euristica solo italiana: sui casi inglesi passa sempre (va letto dalla rubrica `second_person`).
- `rubric[]`: criteri sì/no valutati da un giudice LLM; `weight` pesa il criterio. Punteggio della run = somma dei pesi dei criteri soddisfatti / somma dei pesi.
- `pass_criteria`: soglie lette dal runner.

## Come si valuta

1. Il runner costruisce la richiesta **con il Context Assembler reale** (non una copia) e la invia al provider del ruolo configurato; salva testo, usage, latenza, `prompt_version`, modello.
2. Controlli deterministici sul testo.
3. Giudice LLM, **modello diverso e non più debole di quello valutato** (proposta: `claude-opus-5-5`, effort `low`), con output strutturato `{criterion_id: {met: bool, reason: string}}`. Il giudice riceve la conversazione, la risposta e i criteri, non il prompt del Tutor. Prima di fidarsi del giudice si controllano a mano 10-15 verdetti.
4. Per ogni modifica a un prompt: rieseguire la suite, confrontare con la baseline salvata in `results/`, accettare la modifica solo se i punteggi non peggiorano e nessun controllo deterministico regredisce.

## Costo indicativo

5 casi x 3 run con Sonnet 5.5 (Tutor) più giudice: poche migliaia di token per chiamata, nell'ordine di pochi centesimi per esecuzione completa (da misurare alla prima esecuzione reale con `usage`).

## Risultati

Nessuna esecuzione ancora: non c'era una chiave disponibile in questa fase. I casi sono validi come JSON ma non sono stati valutati contro un modello.

## Suite con tool e multi-turno (`tutor/phase3.cases.json`, Fase 3)

Estensione del formato per conversazioni con tool. Le suite della Fase 2 restano valide.

- `topic`: oltre a `title` e `state`, `mission: {current?, draft?}` (stessa forma del contratto, con `version`) e `preferences: [{scope, text}]`. Il Context Assembler reale li rende in `{{mission_section}}` e `{{preferences_section}}`.
- `history`: messaggi precedenti (opzionale) prima del primo step.
- `steps[]`, eseguiti in ordine; ogni `user` è un turno scriptato (l'utente non reagisce al testo reale del modello: per questo i casi sono scritti perché lo script abbia senso qualunque sia la domanda del Tutor):
  - `{"user": "...", "expect": {...}}`: invia il messaggio, esegue il ciclo dei tool **con la logica reale** (validazione dallo schema di `tools.json`, risultati da `tool_results.json`, dedup, ammissibilità) su uno store in memoria, controlla `expect`.
  - `{"event": "confirm_mission"}`: il runner conferma la bozza aperta (diventa `current`, il Topic passa a `researching`); nessun messaggio in chat, come da contratto.
  - Prima di ogni step `user` lo stato torna a `interviewing` se era `mission_proposed` (come l'app). Con una mission confermata, `propose_mission_change` e la sua conferma non cambiano lo stato del Topic (`ready` resta `ready`).
- `expect.tools`: `called` (tutti devono essere stati chiamati nel turno), `not_called`, `max_calls`, `input_checks` sull'input dell'**ultimo** tool call di quel nome:
  - `success_criteria_count: [min,max]`; `fields_must_match` / `fields_must_not_match`: `{campo: regex}` valida su ogni voce dei campi array o sul testo del campo stringa (`why`, `text`); `fields_any_match`: `{campo: regex}` passa se **almeno una** voce corrisponde (per esempio una scadenza tra i vincoli; dal 2026-11-18); `preference_scope`.
  - I default di suite `tool_input`: `criteria_must_not_match` (criteri che iniziano con capire/conoscere/imparare/… = vaghi), `why_min_words`, `fields_within_schema_limits`.
- `expect.deterministic` (sul testo del turno, oltre ai default): `max_words_when_tool_called` (introduzione di una frase), `text_must_not_repeat_tool_fields` (nessuna sequenza di 5 parole consecutive di un campo dell'input compare nel testo, salvo quelle scritte dalla persona nel messaggio del turno; fino al 2026-11-18 erano 4 parole e senza eccezione), `no_list_when_tool_called`. (`max_words_when_tool_called` = 70: l'introduzione è un riassunto di 2-3 righe, non una frase sola.)
- `expect.rubric`: come prima, giudicata solo sul turno dello step, con la conversazione fino a lì e (per il giudice) i tool call fatti e il loro input.
- Un `expect` assente = nessun controllo su quel turno (serve solo a far avanzare la conversazione).
- Risultati da riportare per caso: `tool_calls` per turno, numero di round di tool, errori di validazione incontrati (indica schemi o descrizioni poco chiari), usage con `cache_read_tokens`.
- Metrica di fiducia: le regole "mai prima del turno N" (`vague_pure`, `skip_interview`) devono passare in **3/3 run**; le altre soglie come in Fase 2. Da confrontare tra `effort: low` e `medium` e tra Sonnet 5.5 e Haiku 5.5 prima di fissare il default.

Non eseguita: manca la chiave API.

## Suite del Researcher (`researcher/phase4.cases.json`, Fase 4)

Un turno (con eventuali continuazioni `pause_turn`), nessuna conversazione. Il runner costruisce la richiesta con il Context Assembler reale (variabili in `prompts/README.md`), con lo strumento di ricerca lato server e `max_uses = search_budget`, e legge l'output JSON dal testo finale.

Campi di un caso: `topic.title`, `mission` (forma del contratto), `preferences[{scope,text}]`, `existing_resources[{kind,title,url,use_for}]`, `deterministic`, `rubric`.

Controlli deterministici nuovi (oltre a quelli del formato generale):
- `schema_valid` contro `prompts/researcher/output_schema.json`; `resources_count: [min,max]`; `min_per_kind` / `max_per_kind`.
- `urls_must_be_https`, `no_duplicate_urls`.
- **`urls_must_be_in_search_results`**: ogni `url` deve comparire tra i risultati di ricerca della run oppure tra le `existing_resources` del caso (stessa regola del Validator). Insieme = URL dei blocchi `web_search_tool_result`; se il provider non li restituisce (OpenRouter in streaming, vedi `llm-port.md` §12.6) si usa la richiesta non in streaming oppure si uniscono gli URL delle citazioni. È la garanzia di "never trust parametric knowledge": un URL ricordato a memoria o inventato fa fallire la run. Lo stesso controllo va nel Validator del backend.
- `max_web_searches`: `usage.server_tool_use.web_search_requests` <= budget.
- `annotation_language`, `annotation_words`, `use_for_words`, `annotation_must_not_match`, `use_for_must_not_match` (vieta "Usala per" ripetuto e gli "imparare." nudi).
- `community_url_must_not_match`: una community punta al luogo, non a un thread.
- `communities_skipped` (valore atteso del campo), `no_community_when_skipped` (se true nessuna risorsa `community`).
- `knowledge_before_community` (ordine), `gap_note_max_words`, `gap_note_must_not_match` (niente rimedi nelle lacune).
- `must_include_url_matching` / `must_not_include_url_matching` (regex sugli URL), `existing_urls_kept_must_be_identical`, `no_equivalent_mirror_of_existing` (la ricerca sostituisce in blocco: le fonti salvate tenute devono avere lo stesso URL).

Costo: il costo domina sulle ricerche, non sull'output. Misurato il 2026-11-09 con Haiku 5.5 via OpenRouter e 1 ricerca: 0,009-0,014 $ (di cui 0,01 $ è la tariffa della ricerca; ~22k token di input per i risultati di una sola ricerca). Ogni passo successivo del loop lato server rilegge i risultati precedenti, quindi con N ricerche in sequenza l'input cresce circa come N^2 (mitigato dal caching automatico del prefisso, osservato in streaming). Stima prudente per una run con Sonnet 5.5 e 4 ricerche: 0,3-0,6 $. Prima esecuzione consigliata: Haiku 5.5, 1 run, `search_budget` 2 (poche decine di centesimi per tutta la suite), poi Sonnet solo sui casi dubbi. Giudice: modello diverso, non più debole, output strutturato `{criterion_id: {met, reason}}`, senza ricerca.

Risultati reali (2026-11-15, Sonnet 5.5 su Anthropic diretto, 2 run per caso; il runner ora salva in `extras` query e URL visti):

| Prompt | Ricerche | tech_rust_cli | hobby_opt_out | strength_with_community | rerun_with_existing | costo per run (con giudice) |
|---|---|---|---|---|---|---|
| `researcher-phase12-v1` (prima misura, 1 run) | 4 | 2 fonti | 6 fonti, rubrica 0,75 | 4 fonti | 4 fonti, rubrica 0,69 | circa 0,16 $ |
| `researcher-issue15-v7` | 4 | 5-5 fonti, controlli 2/2, rubrica 0,69 (min 0,50) | 5-6, 2/2, 0,58 (min 0,42) | 3-6, **0/2**, 0,33 | 5-5, 2/2, 0,85 | 0,16-0,17 $ (suite: 1,31 $) |
| `researcher-issue15-v7` | 6 (dal 2026-11-15 anche in produzione e nella suite) | 5-6, 2/2, 0,66 (min 0,50) | 5-6, 2/2, 0,75 (min 0,67) | 6-6, 1/2, 0,75 | 7-8, 2/2, 0,85 (min 0,69) | 0,19-0,23 $ (suite: 1,73 $) |
| `researcher-issue15-v8` (correzioni della revisione) | 6 | 5-8 fonti, 2/2, 0,69 (min 0,69) | 5-6, 2/2, 0,67 (min 0,50) | 4-6, 1/2 (la run con 4 fonti non ha community), 0,42 (min 0,33) | 6-8, 2/2, 0,54 (min 0,46) | 0,20-0,23 $ (suite: 1,76 $) |
| `researcher-issue15-v9` (community facoltative, 2026-11-15) | 6 | 7-9 fonti, 3/3, 0,51 (min 0,35), 0 community, `communities_skipped: true` | 5-6, 3/3, 0,64 (min 0,50), 0 community | 6-6, 3/3, 0,58 (min 0,50), 1 community in ogni run (caso opt-in) | 7-8, 3/3, 0,79 (min 0,64), 0 community | 0,21-0,24 $ (suite da 3 run per caso: 2,75 $) |
| `researcher-issue15-v10` (revisione della v9, 2026-11-16, **versione attuale**) | 6 | 8 fonti in ogni run, 3/3, 0,78 (min 0,71), 0 community | 6, 2/3 («ottima» in un'annotazione), 0,72 (min 0,50), 0 community | 5-6, 1/3 (community in 1 run su 3), 0,36 (min 0,17) | 7-8, 3/3, 0,88 (min 0,79), community salvata scartata in ogni run | 0,22-0,24 $ (suite: 2,73 $) |

Con 4 ricerche il numero di fonti passa da 2-4 a 3-6 (mediana 5); `urls_must_be_in_search_results` e `opt_out` passano in tutte le 16 run misurate. Con 6 ricerche le fonti sono 5-8 in tutte le run e fallisce solo un controllo di forma (`annotation_words`, 62 parole). Il costo sale di circa 0,03-0,05 $ per run. **Decisione del tech lead (2026-11-15): `max_searches = 6` in `deploy/teach_me.toml` e `search_budget` 6 nella suite.** `pause_turn` reale: mai osservato in 46 chiamate (4 e 6 ricerche): il modello lancia tutte le ricerche nello stesso passo e il turno finisce con `end_turn`; la continuazione resta coperta dai test con risposte reali registrate (`infrastructure/tests/anthropic.rs`, fixture `web_search_pause_turn.json`). Lo strumento di ricerca non restituisce mai pagine di reddit.com: un subreddit non può superare `urls_must_be_in_search_results`.

Stato finale (2026-11-16): `researcher-issue15-v10` con 6 ricerche. Senza richiesta di community (il caso normale) zero community, nessuna lacuna sulle community, 6-8 fonti, rubrica 0,72-0,88: è il comportamento voluto dal proprietario. Con richiesta (`strength_with_community`) la community arriva in 1 run su 3 nella misura finale (3/3 con la v9, ma 2 su 4 nelle prove): resta fragile perché lo strumento di ricerca non restituisce reddit e per i forum spesso solo thread, che il Validator non accetta come luogo. Quando manca, la lacuna lo dichiara. La rubrica oscilla di 0,2-0,3 tra run con lo stesso prompt (giudizi opinabili: edizione Brown come «mirror», siti che vendono programmi): per il criterio «≥ 0,75 per caso» servono più run.

**v9, community facoltative (decisione del proprietario, issue 15, 2026-11-15).** Il Researcher cerca e consegna community solo se una preferenza le chiede; senza richiesta `communities_skipped: true`, nessuna ricerca per le community, tutto il budget a fonti `knowledge`. Casi: `tech_rust_cli` e `rerun_with_existing` ora senza preferenze si aspettano zero community (`max_per_kind {community: 0}`, `communities_skipped: true`, rubrica sulla seconda fonte solida al posto della community); `strength_with_community` ha la preferenza «Mi piacerebbe un forum o un gruppo dove far controllare la tecnica» (`min_per_kind {community: 1}`); `hobby_opt_out` invariato. Misura finale: 12 run su 12 passano tutti i controlli deterministici (`urls_must_be_in_search_results` incluso, zero community senza richiesta, una community in ognuna delle 3 run opt-in), tutte le run usano 6 ricerche; fonti per run 5-9. Costo 0,21-0,24 $ per run; spesa della misura finale 2,75 $, spesa totale dell'issue 15 con le iterazioni circa 4,2 $ su questa modifica. Iterazioni: la v9 iniziale con richiesta di community consegnava 0 community con 4 fonti (le ricerche andavano a query generiche «forum form check»); aggiunta la regola «con la richiesta dedica due ricerche, una sul tipo di luogo e una col nome di un forum con dominio proprio». Punti deboli: con la richiesta il caso resta fragile (in 2 run di prova su 4 la community mancava perché il forum compariva solo come thread, mai la pagina principale); la rubrica media di `tech_rust_cli` (0,51, min 0,35) e `hobby_opt_out` è sotto la soglia 0,75, con giudizi opinabili (quiz Brown come doppione del libro, `use_for` non nominali, fonti che vendono programmi come Stronger by Science), e non è stata inseguita; con 3 run il giudice oscilla di 0,2-0,3.

Cause e punti deboli rimasti:
- Il modello lancia tutte le ricerche nello stesso passo, anche se il prompt chiede un primo giro di metà budget: i buchi non si colmano in un secondo passo, quindi il numero di ricerche resta il vincolo vero. Le query ora cercano il candidato per nome e sito (`doc.rust-lang.org ...`, `rustlings ...`), il che ha tolto lo scarto delle fonti non comparse.
- **Lo strumento di ricerca non restituisce mai reddit.com** (verificato su `reddit.com/r/weightroom`, `r/Fitness`: nessun URL reddit tra i 35 risultati). Con 4 ricerche `strength_with_community` non trova quasi mai una community (0/2); con 6 la trova (forum di Starting Strength) ma l'annotazione sfora le 60 parole una volta su due.
- La rubrica media resta sotto 0,75 in `tech_rust_cli` (0,66-0,69) e, con 4 ricerche, in `hobby_opt_out` (0,58): il giudice contesta `use_for` non del tutto nominali, `use_for` generici rispetto al CLI su CSV, sovrapposizioni (libro ufficiale e edizione Brown, tutorial e cookbook del crate csv) e annotazioni di affidabilità vaghe. In `hobby_opt_out` una annotazione cita un thread di forum (rubrica `opt_out`) e Kodak AJ-3 copre anche la stampa (fuori scope). Il giudice è rumoroso (stessa uscita, 0,50-0,75).
- Spesa totale dell'issue 15 (2026-11-15/16, tutte le run del Researcher): circa 17 $, più una run interrotta senza report.

## Fasi 6-7: Generator, Grader, Recorder (`generator/phase6.cases.json`, `grader/phase7.cases.json`, `recorder/phase7.cases.json`)

Solo file: nessun runner, nessuna esecuzione (nessuna chiamata a pagamento in questa fase). Le fixture usano id e URL **inventati a scopo di test** (UUID `0193a000-…`, URL di libri e documentazione plausibili ma non verificati): non sono fonti.

Struttura comune: `defaults` (`runs_per_case`, `deterministic`, `notes`), `pass_criteria`, `cases[]` con `id`, `description`, input, `expect` (dove serve), `deterministic` (sovrascrive i default chiave per chiave), `rubric[]` (giudice LLM, come sopra). Il runner usa il Context Assembler reale e il Validator reale del backend.

### Generator (5 casi x 3 run)

Il **piano è un input fisso** (`plan`, forma di `planner/output_schema.json`), così la suite misura solo il Generator. Input del caso: `topic`, `mission`, `preferences`, `plan`, `sources` (con `resource_id`), `excerpts` (vuoto), `topic_glossary`, `lessons`. Per ogni run: Generator → Validator → fino a 2 riparazioni. Casi: `rust_first_lesson` (lezione tecnica, prima del Topic), `photo_reuse_glossary` (riuso esatto del glossario, `missing_component` ammesso), `rust_second_lesson_recall_and_link` (ripasso di un record e `lesson_link` con id del contesto), `strength_thin_sources` (nessun numero o citazione inventati con sole annotazioni), `preference_more_practice` (la preferenza non deve violare le regole del Validator).
Controlli deterministici nuovi: `validator_pass_first_try` e `validator_pass_within_3` (si registrano il numero di documenti e i codici di errore per run), `sources_copied_exactly`, `topic_glossary_copied_exactly`, `lesson_links_must_be_in`, `prose_words`, `sections`, `forbidden_phrases`, `objective_not_repeated_in_paragraph`, `open_answer_rubric_words`, `quiz_correct_position_varies`, `numbers_need_citation` (euristico), `max_same_type_consecutive`, `must_include_block_types`, `block_type_count` (`{"<tipo>": [min, max]}` con `<tipo>` un tipo del catalogo, altrimenti la suite non si carica; estremi inclusi, conta anche i blocchi dentro i `fallback`; il valore è validato al caricamento della suite), `min_exercises`, `exercises_with_code` (`[min, max]`: esercizi con il campo `code`), `exercise_types_subset_of_plan` (solo avviso).
Soglie: 100% entro 3 documenti, >= 60% al primo, rubrica media >= 0.8, nessuna regressione. Confronto da fare prima di fissare il default: Opus vs Sonnet (entrambi `high`) e, per le prove, Haiku.

### Grader (6 casi x 3 run)

Input: `lesson_title`, `lesson_objective`, `question` (testo semplice), `rubric`, `max_chars`, `answer`. Atteso: `expect.verdict` (insieme accettato) e `expect.misconception` (`null` o `present`). Casi: risposta corretta con parole proprie e refusi, parziale (manca il perché), errata con misconcezione, prompt injection, risposta in inglese su Topic non tecnico, «non so». Controlli: schema, lunghezze (1-800 / 1-300), nessun punto esclamativo, nessuna parola vietata (lessico di `docs/design/generator-guidelines.md` §1), nessun a capo, nessun `[[`/`[^`/link, nessun voto numerico, `misconception` in terza persona, nessuna fuga della rubrica. Si registra la latenza (tetto di produzione 20 s). Soglie in `pass_criteria`; confronto Haiku vs Sonnet sugli stessi casi prima di fissare il default.

### Recorder (6 casi x 3 run)

Input: `lesson`, `plan` ridotto, `lesson_glossary`, `exercise_blocks` (blocchi SDUI degli esercizi), `attempts` (`block_id`, `attempt_number`, `answer`, `outcome`, `grade` per le aperte), `active_records`, `topic_glossary`. Atteso: `expect` con intervalli per `records`/`terms`/`cards` e vincoli su tipo, chiavi e sostituzioni. Casi (con i tre obbligatori): `explained_only_zero_records` (nessun esercizio superato → tre array vuoti), `misconception_corrected` (secondo tentativo corretto dopo un primo errore), `supersession` (record attivo #2 sostituito), `flashcard_only_weak_evidence` (prova debole → vuoto), `existing_term_and_record_not_duplicated`, `rich_lesson_limits` (limiti 5/8/10, sobrietà, schede autonome).
Controlli deterministici: i controlli del Validator di `lessons.md` §6.3 (prove esistenti e superate, misconcezione con primo tentativo fallito, sostituzioni su record attivi, schede legate a un solo elemento, limiti), lunghezze di §2/§6.1, nessuna lode in `body`, nessun id di esercizio nel testo visibile, schede senza rimandi alla lezione. Soglie: i tre casi di sobrietà devono passare in 3/3 run; Validator al primo colpo >= 85%.

### Risultati reali

Nessuna esecuzione: lo schema dei file è stato solo validato come JSON, e gli `output_schema.json` verificati in locale contro il sottoinsieme strict (vedi `prompts/llm-port.md` §13.2).

## Fase 9: controlli aggiunti dopo le lezioni reali

Dalla revisione `docs/design/review-mvp.md` (problemi 1-7), nuovi controlli deterministici nelle suite (più severi del Validator di proposito):
- Generator: `quiz_option_char_ratio_max` (max/min dei caratteri delle opzioni di un quiz), `fill_blank_answer_not_in_adjacent_text`, `closing_active` (checklist 2-3 voci nell'ultima sezione, sintesi <= 45 parole), `consecutive_code_blocks_need_missing_component` (warn), `glossary_entries_max`, `output_tokens` (ragionamento compreso). Nuovi casi: `go_to_rust_compare_and_code_options`, `fill_blank_no_leak`.
- Grader: `feedback_chars_max_by_verdict` (correct 300, altri 350), nuovi casi `partial_clone_no_criterion` e `correct_long_answer_short_feedback`.
- Recorder: `card_front_single_question`, `card_front_chars_max` (170), `card_back_max_sentences`, `card_back_chars_max` (250), `demonstrated_text_must_not_match`; nuovi casi `real_lesson_1_unproven_point` e `real_lesson_2_second_attempt_pass` (tentativi reali).
Non eseguiti: nessuna chiamata a pagamento.

## Come si lancia

Da `backend/` (o dalla radice del repo). Usa la configurazione del server (`TEACH_ME_CONFIG`; per OpenRouter `TEACH_ME_LLM_PROVIDER=openrouter` e `TEACH_ME_OPENROUTER_API_KEY`; altrimenti `TEACH_ME_ANTHROPIC_API_KEY`). Non serve `TEACH_ME_API_TOKEN`.

```
cargo run -p evals --bin run-evals -- [--suite <ruolo/suite>]... [--case <id|suite:id>]... \
    [--runs N] [--judge-model claude-sonnet-5-5] [--max-cost-usd X | --no-budget] [--dry-run]
```

- `--suite`: ripetibile; senza, tutte (`tutor/phase2`, `tutor/phase3`, `tutor/phase12`, `researcher/phase4`, `generator/phase6`, `generator/phase10`, `generator/phase12`, `planner/phase10`, `planner/phase12`, `grader/phase7`, `recorder/phase7`).
- `--case`: ripetibile. `suite:id` sceglie un caso di quella suite; un `id` nudo filtra solo le suite che hanno un caso con quell'id. Le suite a cui nessun `--case` si riferisce girano per intero.
- `--runs N`: sovrascrive `runs_per_case`. `--judge-model`: modello del giudice (default `claude-sonnet-5-5`).
- `--max-cost-usd X`: tetto di spesa, **2 USD se omesso**. `--max-cost-usd 0` o `--no-budget` lo tolgono. Prima di ogni run si controlla spesa + stima della run; prima di ogni chiamata se ne prenota la stima (prompt a ~3 caratteri per token, risposta fino al limite reale della richiesta del ruolo, per esempio 32.000 token per il Generator, più le ricerche) e la chiamata è rifiutata se spesa + prenotazioni in corso + stima supererebbero il tetto. La prenotazione è atomica e viene sostituita dal costo reale (usage e prezzi di `prompts/llm-port.md` §9) a chiamata finita: lo sforamento possibile è al massimo la differenza fra stima e costo reale. Il costo del giudice conta nello stesso tetto.
- `--dry-run`: nessuna chiamata; elenca i casi e stima il costo.
- Risultati in `evals/results/<timestamp>/` (`report.json`, `summary.md`; git-ignored).

## Fase 10: PDF da studiare (`planner/phase10.cases.json`, `generator/phase10.cases.json`)

Documenti **finti, scritti a mano** (libro di fantasia «Systems Programming with Rust», id `0193d100-…` documento e `0193e100-…` risorsa; URL delle fonti web plausibili ma non verificati). Il campo `documents` di un caso: `{document_id, resource_id, title, language, page_count, sections[{id,title,level,page_start,page_end}], pages{"<n>": testo}}`; il runner lo scrive nel database in memoria come farebbe l'import (documento `ready`, `Resource` `document`, sezioni, pagine). Il testo delle pagine imita i difetti dell'estrazione reale (ADR 0007): rientri del codice persi, lettera capolettera mancante, legature perse («o ers», «speci cation»).

### Planner (`planner/phase10`, 4 casi)

Nuovo ruolo del runner (`roles/planner.rs`): esegue il job reale di generazione fino al piano; il Generator è rifiutato dal decoratore (`PlannedLlm::set_refuse_generator`), quindi una run costa solo il Planner (e la riparazione, se serve) più il giudice. Il piano è letto dalle chiamate registrate; i riferimenti sono controllati come fa `validate_plan` del backend (privato al crate `application`, quindi una copia ridotta: fonti, documento, sezioni esistenti e non ripetute, massimo 6, risorsa del documento tra le fonti). Casi: `follow_book_in_order_after_two_lessons` (mission «capitoli 3-5 in ordine», due lezioni già fatte: atteso 3.3, non le coperte), `borrow_error_goal_picks_narrow_sections` (obiettivo mirato: sezioni piccole del capitolo 4), `book_does_not_cover_the_goal` (il libro non tratta serde: nessuna sezione, fonte primaria web), `whole_chapter_is_too_big` (capitolo di 30 pagine: solo 5.1).

Controlli deterministici: `plan_valid_first_try` (avviso) e `plan_valid`, `planner_calls_max`, `document_sections_include_one_of` / `_exclude` / `_only_from` / `_empty` / `_max` / `_max_pages` (somma delle pagine distinte, ~10 pagine = il limite di 24.000 caratteri del Generator), `no_parent_and_child`, `document_sections_in_book_order`, `primary_is_document`, `document_resource_in_sources_when_chosen`, `title_not_a_chapter_number`, `exercises_not_about_layout`.

### Generator (`generator/phase10`, 2 casi)

Piano fisso con `document_sections`. Il runner rilegge dalla richiesta reale la sezione «Estratti delle fonti»: pagine mostrate (`[p. N]`) e pagine omesse dalla nota. Casi: `pdf_chapter_moves_with_flat_code` (sezione 3.2, pp. 51-54, codice senza rientri, legature perse) e `pdf_excerpt_cut_by_the_limit` (sezione 7.1, pp. 161-166: due pagine di listato superano 24.000 caratteri, le pp. 164-166 sono omesse).

Controlli nuovi: `document_sources` (url `null` se e solo se la fonte è un PDF, pagine nel title tra quelle mostrate e non omesse, al massimo 3 voci PDF, title <= 200), `pdf_anchor_opening` (il blocco dopo il titolo è un paragrafo <= 45 parole che cita il PDF e nomina le pagine), `pages_mentioned_within_excerpt` (nessun numero di pagina nel testo fuori dall'estratto), `no_page_exercises` (nessun esercizio su pagine, capitoli, listati), `code_indentation_rebuilt` (nessuna riga a colonna 0 dopo un `{`), `code_lines_in_excerpt` (avviso: quota delle righe di codice presenti nell'estratto, senza spazi). `sources_copied_exactly` salta le fonti PDF (titolo con pagine e `url` null: le copre `document_sources`). Il giudice vede gli estratti dati al Generator.

### Come si lancia (Sonnet per Planner e Generator)

Un file di configurazione fuori dal repo con `[llm.roles.planner]` e `[llm.roles.generator]` (`model = "claude-sonnet-5-5"`; effort `medium` per il Planner, per il Generator **nessun override**: vale il predefinito `low`, con `medium` il ragionamento consuma il 60-80% dei token), poi `TEACH_ME_CONFIG=… TEACH_ME_LLM_PROVIDER=openrouter TEACH_ME_OPENROUTER_API_KEY=… cargo run -p evals --bin run-evals -- --runs 1 --suite planner/phase10 --suite generator/phase10 --max-cost-usd 0.6`. Senza override i default dei due ruoli sono Opus.

### Risultati reali (2026-11-09, Sonnet 5.5 via OpenRouter, effort medium, 1 run per caso, giudice Sonnet 5.5)

Costo totale 0,4976 $ (tetto 0,60 $), in quattro esecuzioni.
- `planner-phase10-v1`, 4 casi: 0,1146 $. 3 su 4 con controlli duri a posto; `follow_book_in_order_after_two_lessons` saltava 3.3 per passare a 4.1 (rubrica 0,57). Rubriche 0,57 / 0,75 / 1,00 / 0,75; due dei verdetti bassi erano difetti del materiale del giudice (non vedeva la `resource_id` del documento né i record), corretti nel runner.
- `planner-phase10-v2` (regola «prima sezione non coperta»), 2 casi: 0,0729 $. `follow_book_in_order_after_two_lessons` 1/1 controlli, rubrica 1,00; `whole_chapter_is_too_big` 1/1, rubrica 1,00. I casi 2 e 3 non sono stati rieseguiti con v2.
- `generator-phase10-v1`, 2 casi: 0,2053 $. Tutti i controlli PDF a posto in entrambi (url null, pagine nel title tra le mostrate, nessuna pagina omessa citata, ancora in apertura, rientri ricostruiti, nessun esercizio su pagine). Fallito solo `quiz_option_char_ratio_max` (difetto generale, non del PDF). Rubriche 0,88 (il `why` della fonte primaria ripeteva un passaggio già spiegato) e 0,75 (confronto con `go func()` non sostenuto dall'estratto).
- `generator-phase10-v2`, `pdf_excerpt_cut_by_the_limit`: 0,1048 $. Rubrica 1,00; fallito solo `quiz_option_char_ratio_max` (1,44). `pdf_chapter_moves_with_flat_code` non è stato rieseguito con v2.
Una sola run per caso: sono indizi, non le soglie di `pass_criteria` (3 run).

## Fase 12: lezioni più ricche (`planner/phase12.cases.json`, `generator/phase12.cases.json`; prompt `planner-phase12-v1`, `generator-phase12-v1`)

Solo file: **nessuna esecuzione reale** (il credito è limitato; il backend ora conosce i campi del piano e i controlli nuovi). Il caricamento fallisce per chiavi di controllo sconosciute nei ruoli Planner e Generator. Id e URL delle fonti sono inventati a scopo di test (URL plausibili, non verificati). Soglie in `pass_criteria` di ciascun file. Le regole sono quelle di `docs/design/generator-guidelines.md` §7.

### Planner (7 casi x 3 run: 5 di blocco A + la coppia del blocco B)

Stesso formato di `planner/phase10` (`records`, `lessons`, `sources`; `documents: []` = nessun PDF). Profili: `beginner_zero_records_general_prereqs` (nessun record, la mission non dichiara nulla: prerequisiti `general`, nessuno `record`/`mission`), `records_cover_prerequisites` (due record e Python dichiarato: `record` con i numeri giusti, `mission`, al più un `general`), `obvious_looking_prerequisite_is_general` (cookie e sessione sembrano ovvi a chi fa web ma nessun record né la mission li copre: `general`), `big_prerequisite_becomes_support_concept` (immagine contro container: concetto di supporto, non riga di prerequisito), `convention_pitfall_not_invented` (argomento di convenzione: `pitfall` vuoto dove non c'è un errore reale).

Controlli deterministici **già esistenti**: `plan_valid` (con `validate_plan` aggiornato dal backend), `plan_valid_first_try`, `planner_calls_max`, `title_not_a_chapter_number`.

Controlli **nuovi da implementare in `crates/evals`** (sul JSON del piano, `roles/planner.rs` e `checks.rs`):
- `concepts_count: [min,max]`; `concepts_max_when_general: n` (se esiste un prerequisito `general`, i concetti sono al più n).
- `concept_fields: {example_chars:[1,300], steps_count:[2,6], step_chars:[1,150], pitfall_chars_max:200}` su ogni concetto (i campi esistono, `steps` è un array di stringhe).
- `pitfall_form: regex`: il `pitfall` di ogni concetto la soddisfa (`^(Pensare che .+)?$`: vuoto oppure comincia con «Pensare che»).
- `new_terms_max: {per_concept, total}`: `terms.introduce.len() <= total` e `<= per_concept * concetti`.
- `general_prerequisites_max: n`; `general_count: [min,max]` (numero di prerequisiti con `basis` `general`); `prerequisite_basis_max: {basis: n}` e `prerequisite_basis_min: {basis: n}`; `record_numbers_subset_of: [..]` (unione dei `record_numbers`).
- `example_not_in_exercises` (avviso): nessuna sequenza di 4 parole consecutive di un `example` compare in un `purpose` di esercizio, né viceversa.
- `exercises_at_least_one_per_concept` (avviso): `exercises.len() >= concepts.len()`.
- `estimated_minutes_formula: {tolerance}`: con `P = 220 + 450*C + 200*[c'è un general]`, `E = esercizi + [c'è un general]`, `O` = numero di `open_answer`, atteso `round(P/250 + 0,5*E + 2*O)` entro la tolleranza; `estimated_minutes_range: {range, severity}` (avviso).
- Metrica da riportare per caso: quota media di prerequisiti `general` sul totale (0 se non ci sono prerequisiti), per verificare che i profili senza record ne producano e quelli coperti no.

### Generator (3 casi x 3 run)

Piano fisso con i campi nuovi; il caso ha anche `records` (come nel Planner) che il runner scrive nell'argomento e l'Assembler passa in `{{records_section}}`. Casi: `beginner_no_records_recall_section` (foglio di calcolo: nessun record, due `general`, tre concetti, il terzo senza `pitfall`; atteso richiamo e 5 sezioni), `rust_second_lesson_records_no_recall` (due record, un prerequisito `mission`, nessun `general`; atteso 4 sezioni, nessun richiamo, move agganciato non rispiegato, `topic_glossary_copied_exactly`), `small_plan_two_concepts` (due concetti, un solo `pitfall`: 3 sezioni, deve arrivare a 1200 parole senza aggiungere elementi). Il campo `deterministic.recall.expected` del caso conferma ciò che il runner calcola dal piano.

Controlli **nuovi da implementare** (sul documento validato, `roles/generator.rs`/`sdui.rs`/`checks.rs`; i nomi sono le chiavi di `defaults.deterministic`; ogni voce ha la sua descrizione nel file):
1. `prose_words: [1200,2200]` (esiste già: nuovo intervallo) e `prose_words_target` (avviso 1500-1900). Conteggio come contratto §9 (`paragraph`, `callout` title+text, domande, opzioni, feedback dei quiz).
2. `sections_from_plan`: h2 = concetti + 1 + [c'è un `general`].
3. `explanation_run_words_max {max:300, defect_above:350}`: parole consecutive di `paragraph` e `callout` tra l'inizio della sezione (o un esercizio) e l'esercizio successivo; `code` non conta né interrompe. Riportare il massimo per run.
4. `paragraphs_consecutive_max: 2` (conteggio sui `type` di blocchi top-level; un `diagram` tra paragrafi non azzera la serie); `paragraph_words` (avviso, 30-110).
5. `concept_chain` e `h3_per_concept_section`: per ogni sezione di concetto (le h2 che non sono richiamo né chiusura, nell'ordine del piano) la sequenza `paragraph, heading3 «Esempio: …», code?, paragraph, paragraph, callout warning «Errore tipico: …»?, esercizi+`, ignorando i `glossary_term`; il callout se e solo se il `pitfall` dello stesso indice non è vuoto; titolo <= 8 parole, testo <= 300 caratteri; esattamente un h3 per sezione di concetto e nessuno altrove.
6. `callouts {per_section_max, warning_max, tip_plus_info_max}`; `intro {paragraphs, words_max, no_exercise, no_h3}` (blocchi tra l'h1 e la prima h2).
7. `recall_iff_general`: presenza della sezione «Cosa ti serve:» come prima h2 se e solo se il piano ha un `general`; composizione (1 esercizio `quiz_single`/`fill_blank` con `required: false` prima dei paragrafi di ripasso, 1-3 ripassi, nessuna flashcard, nessun h3, <= 300 parole).
8. `closing {words_max:120}` (paragraph, checklist e `why` della fonte primaria dell'ultima h2); `closing_active.closing_summary_words_max` sale a 60 (esiste già).
9. `estimated_minutes_formula {tolerance}`; `exercises_per_concept_section: [1,2]`.
10. `forbidden_phrases` (esiste: nuove regex) e `forbidden_phrases_warn` (stessa logica, ma avviso).
11. `term_first_mention_is_glossary_link`: la prima occorrenza di ogni `terms.introduce[].term` nella prosa è un `[[key]]` (o un `glossary_term` precedente); ignora i `codice inline`, i titoli e i blocchi codice. Falsi positivi attesi per termini che sono parole comuni: si leggono a mano i falliti.
12. `glossary_entries_max` sale a 12; `output_tokens.warn_above` a 14000. Il runner legge `records` e `deterministic.recall.expected`; `topic_glossary_copied_exactly` e `sources_copied_exactly` restano.
13. `estimated_minutes_range` (Generator, array `[min,max]`): l'`estimated_minutes` del documento; è un **errore** se fuori (a differenza del Planner, dove è `{range, severity}` e avviso). Se il backend lo accetta, preferisco la forma oggetto `{range, severity: "warn"}` anche qui: è un'informazione di taratura, non un difetto.

Interpretazioni scelte dal backend e accettate (o decise qui):
- `explanation_run_words_max`: la corsa va da inizio sezione, o da un esercizio, fino al prossimo esercizio o a fine sezione; intro e chiusura escluse. Sopra `max` avviso, sopra `defect_above` errore. Accettata.
- `paragraph_words`: si applica a **tutti** i paragrafi, richiamo compreso. Il prompt chiede per il richiamo un paragrafo d'invito di 20-40 parole e ripassi di 25-60: con il minimo a 30 avrebbero generato avvisi sistematici. Deciso: non si esclude il richiamo, ma il minimo dei casi è sceso a 20 (`range: [20,110]`). Nessuna modifica al backend.
- `concept_chain`: rigido, esattamente 2 paragrafi dopo l'h3 (è la catena del prompt: intuizione, h3 esempio, regola). Accettata.
- `closing`: conta paragrafi, checklist (title e items) e `primary_source.why` dell'ultima h2. Accettata.
- `recall_iff_general`: «paragrafi di ripasso» = i paragrafi **dopo** l'esercizio di richiamo; il paragrafo d'invito (20-40 parole) che lo precede non si conta. Da confermare lato backend.
- `estimated_minutes_formula`: senza `tolerance` vale 0. Nei casi il Planner usa ±1 (la formula lavora su costanti, `P = 220 + 450*C + 200*[general]`, quindi il modello sbaglia solo per arrotondamento) e il Generator ±2: `P` sono le parole che il modello ha scritto senza poterle contare, e un errore del 10% su 1700 parole vale quasi 1 minuto, più l'arrotondamento. Se dopo il primo giro reale il Generator rientra spesso in ±1, si stringe.

### Blocco B: punto di partenza della mission (`starting_point`)

Contratto `docs/contracts/mission.md` §2 e §8. **Forma delle mission nei casi**: oltre a `version, why, success, constraints, out_of_scope`, ogni `mission` ha `starting_point` (stringa oppure `null`, tra `success` e `constraints`; per le suite del Tutor la forma è quella del contratto, `success_criteria`). `null` = «non dichiarato». Il seed del runner (`seed.rs`, `App::mission`) deve scriverlo nella colonna della mission e il Context Assembler passarlo come `{{mission_starting_point}}`; finché non lo fa, il campo è ignorato (nessun errore: la mission è un `Value` libero).

Migrazione dei casi esistenti (nessun cambio del comportamento atteso): le conoscenze di partenza che stavano in `constraints` («So già programmare in Python», «Vengo da Go e C++», «Conosco già ownership e borrowing», «Uso git ogni giorno…», «Non ho mai usato Docker…», «Ho una laurea in lettere…») sono passate in `starting_point`; restano in `constraints` tempo, strumenti e perimetro (per esempio «Ho caricato il libro…»). Dove la mission non dichiarava conoscenze (`photo_reuse_glossary`, `rust_second_lesson_recall_and_link`, `fill_blank_no_leak`, `book_does_not_cover_the_goal`, tre casi del Researcher) il campo è `null`.

- **Tutor** `tutor/phase12.cases.json` (5 casi, stesso formato e stessi default di `phase3`): `asks_starting_point_before_proposing` (nessuna bozza al turno 1; «un po', le basi» ottiene un approfondimento concreto; con la risposta concreta arriva la bozza con `starting_point` e senza la conoscenza in `constraints`), `starting_point_in_first_message_not_asked_again`, `from_scratch_is_written_explicitly` («parto da zero» scritto esplicitamente, nessuna insistenza), `confirmed_without_starting_point` (non lo solleva da solo, propone il cambio una volta quando la persona racconta che cosa sa, non lo ripete), `change_requested_with_undeclared_starting_point_asks_first` (cambio richiesto con confermata senza punto di partenza: chiede prima, poi propone). Da riportare: quante volte il Tutor propone la bozza **prima** di aver chiesto il punto di partenza (atteso 0 su `asks_starting_point_before_proposing` turno 1, in 3/3 run) e se `starting_point` finisce in `constraints`. `phase3` è stata aggiornata: i messaggi degli utenti scriptati dei casi che portano a una bozza includono il punto di partenza (altrimenti il Tutor, correttamente, lo chiederebbe), e le bozze/mission iniziali hanno `starting_point`.
- **Planner** `planner/phase12.cases.json`: la coppia `declared_prerequisite_is_mission` / `unnamed_prerequisite_is_general` (stessa lezione su Docker, stesse fonti, nessun record; cambia solo `starting_point`: dichiara il terminale / non lo nomina). Atteso: il prerequisito sul terminale è `mission` nel primo (`prerequisite_basis_min {mission:1}`) e `general` nel secondo (`general_count [1,3]`, `prerequisite_basis_max {mission:0}`). Usa solo controlli già esistenti.
- **Generator e Researcher**: nessun controllo duro nuovo. `researcher/phase4` `tech_rust_cli` ha un punto di partenza dichiarato e la rubrica `level_fits_starting_point`. Per il Generator il punto di partenza passa dai casi migrati (`rust_first_lesson`, `go_to_rust_compare_and_code_options`: confronti con Python e Go; `rust_second_lesson_records_no_recall`: richiamo in una clausola, non rispiegato); la rubrica già esistente sui confronti li copre.
- Controlli nuovi richiesti al backend: **nessuno**.

### Come confrontare con la Fase 10

La Fase 10 resta la baseline per i controlli che non cambiano significato (`quiz_option_char_ratio_max`, `fill_blank_answer_not_in_adjacent_text`, `topic_glossary_copied_exactly`, `sources_copied_exactly`, `closing_active`, `validator_pass_*`): non devono peggiorare. I controlli di lunghezza (`prose_words`, `sections`, `closing_summary_words_max`) cambiano intervallo apposta e non sono confrontabili. Primo giro reale consigliato dopo il passo del backend: il modello più economico capace, 1 run per caso con `--max-cost-usd` basso, poi 3 run solo sui casi dubbi.

### Risultati reali

Suite `generator/phase6` e `generator/phase10`: i piani fissi sono stati migrati alla forma di Fase 12 (2-5 concetti con `example`, `steps`, `pitfall`, `document_sections: []`, `estimated_minutes` dalla formula) e le soglie di lunghezza portate ai valori di Fase 12; i controlli di contenuto (citazioni, pagine del PDF, fedeltà alla fonte, quiz, fill_blank) sono invariati e sono il confronto con la Fase 10. Dove c'era un solo concetto ne è stato aggiunto un secondo coerente con la fonte (vedi `defaults.notes` di `generator/phase6`). `planner/phase10`: `planner_calls_max` da 1 a 2. Verifica senza LLM: tutti i piani fissi delle tre suite del Generator (12 casi) superano `validate_plan` del backend (prova a secco con `World::reference_errors`, test temporaneo poi rimosso).

Nessuna esecuzione reale: i casi sono JSON validi e i piani fissi del Generator sono stati controllati contro `planner/output_schema.json` (campi richiesti, limiti di `steps`, `example`, `pitfall`, numero di sezioni attese).


## Fase 12 blocco C: budget dei blocchi e lettura delle fonti web (`generator-phase12-v2`)

**`generator/phase12`**: `defaults.deterministic.blocks_max = {"max": 50, "severity": "warn"}` (blocchi di primo livello, heading compresi; la chiave ha una sola soglia: l'errore a 60 lo dà già il Validator, `schema` su `/blocks`) e due casi al limite: `five_concepts_no_recall_block_budget` (5 concetti senza richiamo, 6 h2, 5 esercizi, git branch/merge) e `four_concepts_with_recall_block_budget` (4 concetti con richiamo, 6 h2, CERCA.VERT). Da riportare sempre il massimo di blocchi per run; soglia `block_budget` in `pass_criteria`. Fonti dei due casi: URL plausibili non verificati, come nel resto della suite. Piani fissi verificati con `World::reference_errors`.

**`generator/phase12-fetch`** (5 casi x 2 run, non in streaming): il Generator con lo strumento web fetch (ADR 0009, `prompts/llm-port.md` §14). Casi: `rust_book_page_facts` (una pagina reale e stabile del libro di Rust: elementi della pagina nella lezione), `three_pages_requests_and_cost` e il gemello `three_pages_baseline_no_fetch` (stesso piano con lo strumento spento per il caso: richieste, token e sovraccosto), `unreachable_source_still_generates` (dominio `.invalid`: la lezione si genera senza attribuire contenuti), `injection_in_fetched_page` (pagina fixture con istruzioni ostili). Le pagine reali sono state lette il 2026-11-11 (raggiungibili, testo statico, `robots.txt` di doc.rust-lang.org non le vieta).

Lo strumento è acceso **solo** per le suite e i casi con `requires.web_fetch`; le altre suite del Generator girano sempre a strumento spento. Con lo strumento spento (`max_fetches = 0` o capability assente) la suite intera viene saltata e `--suite generator/phase12-fetch` esplicito dà un errore chiaro.

Campi del file letti dal runner:
- `requires.web_fetch` (suite e caso): serve `[llm.roles.generator] max_fetches >= 1` e `server_web_fetch`.
- `web_fetch: "off"` (caso): quel caso gira con lo strumento spento anche se la config lo accende. Lo ha `three_pages_baseline_no_fetch`, che ha anche `requires.web_fetch: true` di proposito: a strumento spento viene saltato con la suite, a strumento acceso gira spento, così la baseline sta nella stessa run della misura.
- `requires.fixture_pages` + `fixture_pages`: il caso `injection_in_fetched_page` non legge dalla rete. Lo strumento gira sui server del provider e una pagina di prova andrebbe pubblicata, cosa che il proprietario non può fare (issue #26). Il runner (`InjectingLlm` in `src/llm.rs`) mette quindi la pagina nella conversazione **come già letta**: dopo il messaggio iniziale aggiunge un turno dell'assistente con i blocchi `server_tool_use` e `web_fetch_tool_result` (testo di `fixture_pages`), poi «Continua.», e toglie lo strumento. Il modello trova il testo ostile dove e come troverebbe una lettura vera; non sceglie da sé di leggere, ma per l'injection non conta (il fetch vero lo misurano gli altri casi). L'API accetta questi blocchi (prova del 2026-11-19). Nel rapporto `extras.web_fetch.injected_pages` dice quali pagine sono state iniettate. URL `.invalid` di proposito: nessuno può leggerlo davvero. `evals/generator/pages/variabili-ambiente.html` resta come versione HTML della stessa pagina, senza commenti (un commento «questa è una prova» arriverebbe al modello), se un giorno si potrà pubblicarla.
- `fixture_page_served` (in `fetch_checks`, bool, errore): la pagina letta all'URL di `fixture_pages` contiene almeno l'80% dei paragrafi della fixture (confronto senza markup, minuscolo, spazi collassati). Con la pagina iniettata passa sempre: resta come guardia se un giorno la pagina sarà letta davvero.
- `citations_in_fetched_pages` (in `fetch_checks`, `{"max_unsupported": n}`, avviso): per ogni frase che porta `[^key]` di una fonte la cui pagina è stata letta, i nomi che la frase usa (span di codice che nominano qualcosa, come `&String`, `first_word`, `String::from`, e token come `4.1` o `E0499`) devono comparire nel testo della pagina. Conta solo la frase che il marcatore chiude, non tutto il blocco. Gli esempi scritti dalla lezione non contano: span con percorsi, indici, letterali, assegnamenti, chiamate con argomenti, virgole, parole di 3 lettere o meno e variabili in minuscolo (`/home/ana/bin`, `&s[3..5]`, `"ciao"`, `SALUTO=ciao`, `ls`, `&frase`). Misura le attribuzioni sbagliate viste nella #16 (regole del §4.1 attribuite al §4.3, nomi che la pagina non usa). Metrica corrispondente in `report_metrics`: `unsupported_citations`. Ripassato sulle 15 lezioni della suite del 2026-11-19 con il testo vero delle pagine: 0 segnalazioni (prima della correzione 5-6 a lezione, tutte falsi positivi). Limite: vede solo nomi e numeri, non una frase in prosa attribuita alla pagina sbagliata; quella resta al giudice (`nothing_unread_attributed`).
- `fetch_checks` (suite `defaults` e caso): le chiavi **restano in `fetch_checks`**, non vanno in `deterministic`: il runner le legge da lì. `web_fetch_requests [min,max]` (somma di `usage.web_fetches` su tutte le chiamate), `fetch_errors [min,max]`, `fetch_urls_only_from_sources`, `no_refetch_in_repair`, `must_match_min {min, patterns[]}`, `user_message_has_no_sources_list` (solo gemello). Severità: `web_fetch_requests` sopra il massimo errore, sotto il minimo avviso; `must_match_min` sotto soglia avviso; gli altri errore. `fetched_tokens` è una stima (caratteri/4). Con una pagina iniettata (`fixture_pages`) il controllo `web_fetch_requests` conta anche quella, la metrica omonima di `report_metrics` solo le letture reali del provider.
- `report_metrics` (caso): cosa riportare. Il giudice riceve anche il testo delle pagine lette, 40000 caratteri per pagina e 80000 in tutto al massimo (fino al 2026-11-19 erano 12000: le pagine del libro di Rust sono 13-18k e il giudice ne vedeva solo l'inizio, quindi dava «non sostenuto» a frasi prese dalla fine).

**Prova reale minima** (una chiamata, costo atteso sotto 0,30 $):
1. Config fuori dal repo con `[llm.providers.openrouter] server_web_fetch = true`, `[llm.roles.generator] model = "claude-haiku-5-5"` e `max_fetches = 1`.
2. `cargo run -p evals --bin run-evals -- --suite generator/phase12-fetch --case rust_book_page_facts --runs 1 --judge-model claude-haiku-5-5 --max-cost-usd 0.30`
3. In `report.json` guardare: `calls[].web_fetches` (>= 1 sulla prima chiamata), `turns[0].extras.web_fetch.pages` (testo letto, token stimati), i controlli `web_fetch_requests` (atteso 1) e `fetch_urls_only_from_sources`. Se la chiamata fallisce con **400**, annotare il messaggio: è il punto 1 dell'ADR (OpenRouter non accetta `web_fetch_20250910`, o non con `output_config.format`) e si riprova con API diretta o con `OutputFormat::Text` (`llm-port.md` §14.1).

Risultati reali: nessuna esecuzione (manca la chiave). Costo stimato dal `--dry-run` (2026-11-11, prenotazione al tetto reale di 32.000 token per il Generator): ~0,48 $ a run per un caso del Generator e ~0,08 $ a run per un caso del Planner; la stima è prudente (le run reali del Generator costano 0,05-0,19 $). La riga «0,95 $ a caso per 2 run» era di prima che il tetto fosse quello reale: superata.


## Fase 12: primi risultati reali e correzione (2026-11-11, Sonnet 5.5 diretto, giudice Haiku 5.5, 1 run per caso)

**Diagnosi (misurata).** Con `generator-phase12-v2`, effort `medium`, tetto 20.000: il documento valido pesa 9-12k caratteri di JSON, cioè 2,8-4k token; l'`output_tokens` totale era 9,6-17,7k nei casi riusciti, quindi **il ragionamento era 6-14k token (60-80%)**; con 4-5 concetti il ragionamento da solo arrivava al tetto (`MaxTokens`, nessun documento, 0,21 $ buttati). Con effort `low` lo stesso modello usa ~0,5-1k token di ragionamento. Non c'è un conteggio separato del ragionamento nel rapporto (stima: `output_tokens` meno caratteri del documento / ~3,2).
Altra causa trovata: con effort `low` il modello scrive paragrafi più corti dell'intervallo chiesto (61-81 parole dove si chiedevano 80-100) e per ogni concetto arriva a ~300-350 parole di prosa, non 450: per piani da 2 concetti il minimo di 1200 non è raggiungibile con la grammatica dei blocchi attuale (vedi richieste).

Prima/dopo (parole di prosa = misura del runner; il conteggio dei miei script può differire del 5-10%). Dopo = `generator-phase12-v3`, effort `low`.

| caso | versione | parole | token output | costo (compresa riparazione) | esito |
|---|---|---|---|---|---|
| `small_plan_two_concepts` | v2 medium | 671 | 9.603 | 0,10 $ | valido al primo colpo |
| | v3 low (3 prove) | 466 / 630 / 686-740 | 2.929-3.800 | 0,04-0,10 $ | valido (2 prove su 3 con una riparazione) ; `prose_words` fallito sempre |
| `beginner_no_records_recall_section` | v2 medium | 1004 | 11.970 | 0,17 $ | valido al primo colpo |
| | v3 low | 1086 / 1137 / ~1200 (3 prove) | 5.400-6.400 | 0,06-0,20 $ | valido (l'ultima dopo una riparazione e senza controlli falliti) |
| `rust_second_lesson_records_no_recall` | v2 medium | 909 | 17.713 | 0,19 $ | valido al primo colpo |
| | v3 low | 1099 | 6.286 | 0,07 $ | valido al primo colpo; falliti `prose_words`, `estimated_minutes_formula`, `quiz_option_char_ratio_max` (1,33), `term_first_mention` |
| `five_concepts_no_recall_block_budget` | v2 medium | - | 20.000 (MaxTokens) | 0,21 $ | **job fallito** |
| | v3 low (2 prove) | 1652 (prima prova: `quiz_option_length` non corretto in 2 tentativi, job fallito) / ~1900 | 7.161-7.991 | 0,13-0,18 $ | seconda prova valida al primo colpo, 49 blocchi |
| `four_concepts_with_recall_block_budget` | v2 medium | - | 20.000 (MaxTokens) | 0,21 $ | **job fallito** |
| | v3 low | ~1720 | 7.400 (+7.400 riparazione) | 0,18 $ | valido dopo una riparazione (primo documento: `schema`, causa non registrata) |

Rubrica: **non confrontabile**. Il giudice Haiku 5.5 dà 0,0-0,4 anche a documenti che superano tutti i controlli duri e dichiara mancanze che non ci sono (per esempio «manca la sintesi di chiusura» con la sintesi presente). Prima di usare le rubriche per decidere serve un giudice più forte (Sonnet 5.5, non Haiku) e il controllo a mano di 10-15 verdetti: non fatto per mancanza di budget.

Spesa totale di questo compito: ~1,65 $ (somma dei `summary.md`; tetto 2,00 $).

**Planner** (`planner-phase12-v2`, effort `medium`): il caso `beginner_zero_records_general_prereqs` non chiamava il Planner perché **la fixture era sbagliata** (`source_type: "tutorial"` non esiste tra i tipi di risorsa: il seed falliva in silenzio, 0 chiamate); corretto in `article`. Con v2: `unnamed_prerequisite_is_general` 1/1 (prima 0 `general`), `records_cover_prerequisites` 1/1 (formula dei minuti giusta, nessuna riparazione), `beginner_zero_records_general_prereqs` ora produce un piano ma con 0 prerequisiti `general` (statistica da zero: il modello li ha incorporati nei concetti; il caso lo considera un difetto, è discutibile). Costo 0,09 $ per 3 casi. Anomalia non spiegata: in 2 casi su 7 la prima risposta del Planner era un oggetto quasi vuoto (116-129 token, errori «title è vuoto…») e la riparazione ha rimediato; il rapporto non salva il testo del primo tentativo.

**Tutor** (`tutor/phase12`, 3 casi rivisti): letti i dialoghi, il Tutor si comporta bene e i casi erano troppo rigidi: chiede il criterio di successo quando manca prima di salvare la bozza (primo caso), chiede i dettagli del lavoro da automatizzare dopo «parto da zero», chiede il criterio del nuovo obiettivo prima di proporre il cambio. Ho aggiunto un turno finale scriptato in ciascun caso (la risposta che dà il criterio mancante) e spostato lì l'attesa dello strumento; ai turni intermedi resta solo `not_called` e una rubrica sull'assenza di domande già risposte. Nuova esecuzione: `from_scratch_is_written_explicitly` 1/1 (0,80), `change_requested_...` 1/1 (1,00), `starting_point_in_first_message_not_asked_again` 0/1: la bozza arriva ma `constraints` non contiene la scadenza di tre settimane (la persona la aveva detta nel primo messaggio, il Tutor tiene solo «mezz'ora al giorno»): difetto reale ma piccolo del Tutor, non toccato (1 run). Rubriche Haiku di nuovo poco affidabili (per esempio «due domande» dove ce n'è una).

## Fase 12: 4-5 concetti per lezione (2026-11-11; `planner-phase12-v3`, `generator-phase12-v4`)

**Decisione del proprietario**: ogni lezione ha almeno 4 concetti (4 o 5; con un richiamo esattamente 4). Motivo, misurato dal tech lead con il codice finale (rapporto `evals/results/2026-11-11_09-14-53-155`, `generator-phase12-v3`, effort `low`, giudice Haiku, 0,51 $): con 2-3 concetti la prosa era di 759, 1014 e 1181 parole (minimo 1200); con 5 concetti nei limiti (8.161 token, 0,09 $) e con 4 concetti più richiamo nei limiti dopo una riparazione (7.839 token, 0,19 $).

**Fixture.** Tutti i piani fissi hanno 4-5 concetti (4 con un `general`) e 4-5 esercizi, con `estimated_minutes` ricalcolato con la formula del backend. `generator/phase12`: `beginner_no_records_recall_section` (+1 concetto: «scegliere che cosa fissare»), `rust_second_lesson_records_no_recall` (+1: fine del prestito), `small_plan_two_concepts` diventa **`narrow_plan_four_concepts_no_filler`** (Conventional Commits: struttura e tipi sono le due idee proprie, più un cambiamento incompatibile e un commit con due scopi; verifica che il Generator non riempia con tipi rari, riepiloghi o sezioni oltre il piano), i due casi `*_block_budget` invariati. `generator/phase12-fetch`, `phase6`, `phase10`: +1 o +2 concetti coerenti con le fonti del caso (pagine mostrate del PDF per `phase10`; i controlli su citazioni e pagine omesse non sono toccati). Tolto `estimated_minutes_formula` (non misura più il modello); `estimated_minutes_range` 10-15 è un **avviso** sulla durata calcolata dal backend. `prose_words` minimo 1200 in tutte le suite. `planner/phase12` e `planner/phase10`: `concepts_count` [4,5] ([4,4] dove il piano ha un `general`); nuovo caso `narrow_topic_reaches_four_without_filler` (rubrica con le domande del designer). `general_count` parte da 0 in `beginner_zero_records_general_prereqs` e `obvious_looking_prerequisite_is_general`: la promozione a concetto di supporto è ammessa (prima regola per arrivare a 4) e non conta come `general`; la copertura mancante la coglie la rubrica.
Il test `the_phase12_suites_are_listed_and_estimated_in_a_dry_run` (`crates/evals/tests/end_to_end.rs`) cerca ancora il caso `small_plan_two_concepts`: va aggiornato a `narrow_plan_four_concepts_no_filler` (richiesta a rust-backend-dev).

**Giudice.** Haiku come giudice è inaffidabile (0,0-0,4 su documenti che passano tutti i controlli duri). Si usa **`claude-sonnet-5-5`** (è già il default del runner). Costo misurato per caso e per run: **~0,011-0,016 $** per un caso del Planner e **~0,029-0,031 $** per un caso del Generator (il giudice legge il documento intero). Anche con Sonnet le rubriche sono severe e vanno lette: per esempio due verdetti sui piani del Planner erano discutibili (`records_cover_prerequisites` 0,29). Le rubriche servono a trovare cosa leggere a mano, non a decidere; prima di fidarsi vanno controllati 10-15 verdetti (non fatto per il tetto di spesa).

### Risultati reali (Sonnet 5.5 diretto, Planner `medium`, Generator `low` predefinito, tetto 32.000, giudice Sonnet 5.5, 1 run per caso)

Spesa: **~1,02 $** (tetto 1,20 $; fermo a 1,10 $).

| caso | concetti | parole di prosa | token output | costo (con giudice) | esito | riparazioni |
|---|---|---|---|---|---|---|
| `planner/phase12` beginner_zero_records | 4 | - | 117 + 2.015 | 0,050 $ | `general_count` 0 (fallito: soglia poi rilassata) | 1 (primo tentativo quasi vuoto) |
| `planner/phase12` records_cover_prerequisites | 4 | - | 2.260 | 0,026 $ | controlli ok | 0 |
| `planner/phase12` obvious_looking_prerequisite_is_general | 4 | - | 2.312 | 0,027 $ | `general_count`/`basis_min` falliti: cookie promosso a concetto; durata 16 min (avviso) | 0 |
| `planner/phase12` big_prerequisite_becomes_support_concept | 4 | - | 113 + 1.955 | 0,029 $ | controlli ok | 1 (primo tentativo quasi vuoto) |
| `planner/phase12` convention_pitfall_not_invented | 4 | - | 2.036 | 0,023 $ | controlli ok | 0 |
| `planner/phase12` declared_prerequisite_is_mission | 4 | - | 2.211 | 0,025 $ | controlli ok | 0 |
| `planner/phase12` unnamed_prerequisite_is_general | 4 | - | 2.048 | 0,024 $ | controlli ok | 0 |
| `planner/phase12` narrow_topic_reaches_four_without_filler (2 prove, la seconda dopo un ritocco del prompt) | 4 / 4 | - | 2.804 / 114 + 2.035 | 0,032 / 0,067 $ | controlli ok; rubrica 0,39 / 0,50; vedi sotto | 0 / 1 (primo tentativo quasi vuoto) |
| `generator/phase12` narrow_plan_four_concepts_no_filler | 4 | 1.367 | 6.129 | 0,140 $ | valido al primo colpo; falliti `estimated_minutes_range` (9 min), `quiz_option_char_ratio_max` (1,43); avviso `prose_words_target` | 0 |
| `generator/phase12` five_concepts_no_recall_block_budget | 5 | nel target 1500-1900 (nessun avviso) | 7.695 + 2 x 7.7k | 0,324 $ | **valido solo al terzo documento**: `quiz_option_length` per due volte di fila sullo stesso quiz (5-9 parole); falliti `term_first_mention_is_glossary_link`, `forbidden_phrases` («semplicemente»), `quiz_option_char_ratio_max` (1,47); 47 blocchi | 2 |
| `generator/phase10` pdf_chapter_moves_with_flat_code (con ritocco quiz) | 4 | nei limiti (nessun errore) | 6.252 | 0,143 $ | valido al primo colpo; **tutti i controlli del PDF passati** (fonti, ancora, pagine, rientri del codice, nessun esercizio sulle pagine); fallito solo `quiz_option_char_ratio_max` (1,38); avviso durata 9 min | 0 |

Letture (contano più delle rubriche):
- **Planner, 4 concetti: ottenuti in 8 casi su 8** senza che sia servita una riparazione per il numero (le 2 riparazioni sono del primo tentativo quasi vuoto, vedi sotto). Con `general` ha scritto esattamente 4, ma quasi sempre ha **promosso i prerequisiti a concetti** invece di dichiararli `general` (0 `general` nei 2 casi su 2 dove se ne aspettava almeno uno): è la regola 7, coerente con il nuovo vincolo, ma i prerequisiti non dimostrati non hanno più il richiamo.
- **Riempitivo nel piano stretto** (caso `narrow_topic_reaches_four_without_filler`: `git commit --amend`, mission con tre criteri). Il piano di entrambe le prove è: «amend sostituisce l'ultimo commit», «correggere il messaggio», «aggiungere un file dimenticato», «scegliere tra amend e revert». Il quarto è un confronto con il modello noto (`revert`) ed è ammesso dalla regola; il secondo e il terzo sono **lo stesso `--amend` con un dato diverso** (messaggio, file): gli esercizi richiedono procedure diverse (un comando contro mettere in staging prima), quindi non sono ripetizioni vuote, ma il primo concetto già contiene l'idea. Il Planner ha anche unito due capacità nell'`objective` («correggere con amend e dire quando usare revert»), contro la regola 4. Il ritocco al prompt (esempio negativo sulla `SOMMA` e «una capacità, non due unite da e») **non ha cambiato il piano** (rubrica 0,39 → 0,50, stesso schema): la mission del caso elenca già come criteri «correggi il messaggio» e «aggiungi un file», e il Planner li segue. Conclusione: il Planner non inventa concetti fuori mission, ma segue i criteri della mission anche quando sono due usi della stessa idea. Rimisura con un `narrow_topic` la cui mission non elenchi le varianti prima di dire che il test di ammissione funziona.
- **Lezione stretta (Conventional Commits, 4 concetti)**: letta per intero. Le quattro sezioni insegnano cose diverse (struttura, scelta del tipo, `!`/`BREAKING CHANGE`, un commit un tipo), ognuna con intuizione, esempio svolto che scrive ogni passo del piano, regola in grassetto, errore tipico (tre su quattro, il secondo senza come da piano), un esercizio; la chiusura è attiva (checklist, fonte, `ask_agent`). **Nessun riempitivo né sezione di riepilogo**. Difetti: prosa 1.367 parole (sopra il minimo, sotto il target 1.500-1.900: i paragrafi di intuizione escono da 73-74 parole contro le 95-115 chieste); durata calcolata 9 minuti (fuori da 10-15); `docs` e `refactor` compaiono nella spiegazione oltre il piano (il piano dice solo «tipi diversi»); la domanda del quiz ha opzioni di lunghezza diversa in caratteri.
- **Budget dei blocchi a 5 concetti**: 47 blocchi (sotto 50), 6 sezioni, prosa nel target; il difetto è `quiz_option_length` ripetuto e tre fallimenti dei controlli stretti (termini senza `[[key]]`, «semplicemente»).
- **Durata**: con 4 concetti e ~1.370-1.600 parole la formula del backend dà 9 minuti (1 `open_answer`): il minimo di 1200 parole non basta da solo per 10-15 minuti. Il tech lead valuti se alzare il minimo, abbassare la fascia o contare diversamente.

**Correzioni al prompt dopo le misure**
- Generator v4: opzioni dei quiz **di 7 o 8 parole, contate una per una** (prima «6-9 parole»: il modello ne scrive 5-9) e `repair.md` dice di riscrivere **tutte** le opzioni del quiz nominato (il percorso ne indica una sola): due riparazioni di fila non avevano corretto lo stesso quiz. Non rimisurato sul caso a 5 concetti (budget): verificato solo in modo indiretto sul caso PDF (valido al primo colpo, 1 documento).
- Planner v3: esempio negativo sull'argomento stretto e «una capacità, non due unite da e» (effetto non visibile in 1 prova).

**Anomalia aperta del Planner (rust-backend-dev).** In 3 prove su 10 di oggi (2 casi su 8 più la seconda prova del caso stretto; già 2 su 7 nella misura precedente) la prima risposta del Planner è un oggetto con tutti i campi vuoti (113-117 token, errori «title è vuoto», `concepts` 0 voci); la riparazione rimedia (~2.000 token, 0,025 $). `extras.attempts` esiste solo per il Generator: per il Planner non si vede il testo del primo tentativo. Con questo non si può dire se è un problema del prompt, dello schema o dell'effort `medium`.

**Non misurato** (budget): `beginner_no_records_recall_section` e `four_concepts_with_recall_block_budget` (le configurazioni con richiamo), `rust_second_lesson_records_no_recall`, i casi `phase6` e `phase12-fetch` con piani da 4 concetti (i piani fissi sono stati controllati con uno script che replica i limiti di `validate_plan`: conteggi, lunghezze, forma del `pitfall`; non con il codice del backend, e le suite si caricano e stimano nel dry-run), `planner/phase10`, il ritocco quiz su un caso a 5 concetti, e ogni verifica a mano di 10-15 verdetti del giudice. Una sola run per caso: nessuna stima di varianza.

## Da misurare al prossimo giro reale (non ancora misurato)

- **Planner `planner-phase12-v4`**: la chiusura «Prima di rispondere» e la frase rafforzata in `context.md` contro la prima risposta vuota (scheletro). Metrica: quota di `plan_valid_first_try` e di primi tentativi con `concepts` vuoto su almeno 10 run per caso (base: circa 3 su 10 vuoti).
- Il nuovo messaggio d'errore di `quiz_option_length` (elenca le parole di ogni opzione e l'intervallo valido) sul caso da 5 concetti: verificare che la riparazione riesca entro 2 tentativi.
- Le configurazioni con richiamo (`four_concepts_with_recall_block_budget` e simili).
- `generator/phase6` e `generator/phase12-fetch` con i piani nuovi (dal 2026-11-11 i due piani fissi di `phase6` hanno i `records` che citano).
- Il caso stretto del Planner con una mission che non elenchi le varianti come criteri.

## Diagrammi (`generator/diagram.cases.json`, issue #3, `generator-phase12-v6`)

Quattro casi con piano fisso, 2 run ciascuno: tre argomenti che **hanno** una struttura (`flow_http_cache_branch`, `tree_python_exception_hierarchy`, `cycle_tdd_red_green_refactor`: ciascuno atteso con almeno una `diagram` valida del proprio `layout`) e uno che **non** ne ha (`list_of_types_no_decorative_figure`, il piano di `narrow_plan_four_concepts_no_filler`: atteso nessuna figura decorativa). Id e URL delle fonti sono inventati a scopo di test.

Controlli deterministici: `validator_pass_first_try` / `_within_3` (il Validator reale include `diagram_structure`, quindi «figura valida» è verificata), `must_include_block_types: ["diagram", ...]` nei casi positivi, `block_type_count` (`{"diagram": [1, 2]}` nei tre casi positivi, `{"diagram": [0, 0]}` nel negativo; nel caso senza struttura una figura è per definizione decorativa, quindi non si usa `[0, 1]`) e tutti i controlli di forma e lunghezza di `generator/phase12`, `concept_chain` e `explanation_run_words_max` compresi (ignorano `diagram` come ignorano `glossary_term`). Le rubriche `figure_*` giudicano ciò che il codice non vede: figura non decorativa, posizione, testi, risposta non anticipata.

Ancora non coperto dal codice (fuori dall'area del llm-engineer, richiesta a rust-backend-dev):
- `fill_blank_answer_not_in_adjacent_text` e `must_not_match_in_text`/`forbidden_phrases` dovrebbero leggere anche i testi della figura (`title`, `label`, `detail`, etichette degli archi, `caption`, `alt`): oggi la risposta di un esercizio anticipata in un nodo non è intercettata e solo la rubrica `figure_no_answer_leak` la vede.
- Facoltativo: `diagram_text_lengths` (label 2-4 parole, `alt` >= 10 parole).

Soglie: vedi `pass_criteria` nel file. Non eseguita: nessuna chiamata a pagamento. Stima del dry-run del runner (tetto prudente di 0,476 $ per run): 3,81 $ per le 8 run, sopra il tetto di default di 2 $; la stima realistica, dai costi misurati (0,05-0,19 $ per lezione con Sonnet 5.5 più giudice), è di circa 1-1,5 $ (usare `--max-cost-usd`).


## Issue #22: codice degli esercizi nel campo `code` (`generator-phase12-v7`, `generator/issue22`)

Prompt: `generator/system.md` (campo `code?` di `quiz_single`, `quiz_multi`, `open_answer`, `flashcard`, regole di `docs/design/components/_exercises.md` §10.11, `inline_code_too_long` tra le regole del Validator; la guida ai quiz non dice più di scrivere le opzioni come varianti di codice lunghe), `generator/repair.md` (come spostare il codice), `grader/context.md` (una riga: il blocco di codice dopo la domanda ne fa parte). `grader/VERSION` non cambia (è anche in `output_schema.json`).

Fixture: `recorder/phase7` caso 7 aggiornato (la domanda di `q4` aveva l'intera funzione inline: ora il frammento è nel campo `code`; le opzioni di `q3` accorciate a <= 60 caratteri, `ParseIntError` -> `E`, parametro `s` sostituito dal letterale `"7"`). I `plan.concepts[].example` dei piani fissi del Generator restano su una riga con più istruzioni: sono input del Generator, non testo della lezione (il prompt dice di spezzarli in un blocco `code` con una istruzione per riga).

Suite `generator/issue22` (1 caso, 1 run; piano e record di `rust_second_lesson_records_no_recall`, esercizi 1 e 4 che chiedono di leggere un frammento): `validator_pass_first_try`/`within_3` e quattro criteri di rubrica sul `code` (`code_field_used`, `code_is_compact`, `question_does_not_repeat_code`, `options_are_answers`). Manca un controllo deterministico «almeno un esercizio con `code`» (chiave `exercises_with_code` da aggiungere al runner).

Risultato reale (2026-11-15, Sonnet 5.5 diretto, effort `low` da `deploy/teach_me.toml`, giudice Sonnet 5.5, 1 run, 0,254 $: scrittura 0,128 + repair 0,094 + giudice 0,032): primo tentativo non valido (`inline_code_too_long` sulla domanda dell'`open_answer`, 58 caratteri e 3 `;`, più due `unused_entry` del glossario), repair valido al secondo. Documento finale: `quiz_single` e `open_answer` con `code` (3 e 4 righe, una istruzione per riga, nessun numero di riga né commento), opzioni del quiz che sono risposte. Criteri `code_field_used`, `code_is_compact`, `options_are_answers` soddisfatti; `question_does_not_repeat_code` fallito secondo il giudice (la domanda del quiz aggiunge la firma `stampa(t: &String)`, che non è nel frammento: discutibile). Rubrica 0,43: gli altri criteri falliti (`complete_explanation`, `nothing_assumed`, `canonical_terms`, `no_padding`) e `estimated_minutes` 9 fuori da 10-15 non riguardano questa modifica. Un solo run: il primo tentativo ha ancora messo il codice inline nell'`open_answer`, il repair lo ha sistemato; da riprovare con più run prima di dire che il prompt basta.

Tutor `tutor/phase17.cases.json` (issue 17, 3 casi, eseguiti il 2026-11-18): `explicit_request_calls_tool`, `specific_topic_is_not_requested`, `not_ready_does_not_call`. Nota: il runner deve rispondere a `request_lesson` con un esito di `tool_results.json` (`ok`); se il runner non conosce il tool va adattato.

Esiti (Tutor e giudice `claude-sonnet-5-5`, 3 run per caso):
- `tutor-phase17-v1` (0,099 $): `explicit_request_calls_tool` 0/3 (rubrica 0,60): scriveva «Avvio la lezione.», chiamava `request_lesson` (`ok`) e al giro dopo lo richiamava senza testo (28 token), ottenendo `already_running` e «La lezione è già in preparazione…» (due frasi contraddittorie, `max_calls` 2). `specific_topic_is_not_requested` 3/3. `not_ready_does_not_call` 3/3 ma `says_wait` fallita 1 volta su 3 (risposta in più frasi).
- `tutor-phase17-v2` (0,089 $, poi di nuovo 0,060 $ dopo aver riallineato i prefissi dei testi `ok`/`already_in_turn` ai test del backend): tutti e tre i casi 3/3, rubriche 1,00, `tool_rounds` 1 (nessuna seconda chiamata), testo finale «Avvio la lezione.» in 3 run su 3.
- Causa: il giro dopo `ok` lasciava due strade, scrivere la riga mancante o «chiudere senza testo». Con la riga già scritta prima del tool, il modello non ha nulla da dire e un turno finale vuoto è quello che i modelli evitano: richiama il tool (senza argomenti, quindi ripetibile a costo zero; in più «avanzamento» nella `description` e nell'esito lo faceva sembrare uno strumento di stato). Correzione: nessun testo prima del tool, la riga si scrive dopo l'esito (il turno finale non è mai vuoto), l'esito dice che lo strumento non restituisce altri dati e di non richiamarlo, la `description` dice di chiamarlo con `{}`. L'ordine visibile non cambia: la card sta comunque in fondo al messaggio. Ipotesi non necessaria: lo schema `properties: {}` (tenuto, Anthropic richiede un oggetto).
- Rubrica `one_line` riformulata («testo complessivo di una riga», prima «testo prima dello strumento»: non misurava più la cosa giusta, ora che il testo viene dopo). Rubrica `says_wait`: era troppo rigida («in una frase») per ciò che conta, cioè ricerca in corso e lezione richiedibile appena finita, senza date; ora ammette due frasi brevi. Il prompt di `system.md` resta «in una frase»: non l'ho stretto. Nota: una run su 3 scrive «lo stato è `researching`», che mostra il nome interno dello stato alla persona; non è coperto da una rubrica.
- Regressione `tutor/phase3` (`--runs 1`, 0,19 $, più 0,22 $ per una base con i soli testi `request_lesson` della v1): stessi esiti con e senza la modifica. 4 casi su 8 passano i controlli (`vague_pure`, `skip_interview`, `preference_in_passing`, `ready_change_implicit`); falliscono `vague_becomes_precise`, `precise_first_message`, `correction_after_proposal` e `ready_change_explicit` (non chiama `propose_mission_change`, riprende l'intervista) in entrambe le misure. Quindi non sono regressioni dei tool di mission da `request_lesson`, ma difetti già presenti del `system.md` del Tutor, una sola run per caso. Da indagare a parte.
- Costo totale della tappa: 0,661 $.

### Fase 17 bis: i 4 casi falliti di `tutor/phase3` (2026-11-18, `tutor-phase17-v3`)

Misura di partenza (v2, 3 run per caso, 0,57 $): `vague_becomes_precise` 0/3, `precise_first_message` 0/3, `correction_after_proposal` 0/3, `ready_change_explicit` 0/3; gli altri 4 casi 3/3.

Cause per caso:
- `correction_after_proposal`: **caso troppo rigido** (bug del caso, non del prompt). Il runner applica `fields_must_match` a **ogni voce** dell'array: `success_criteria: kubernetes|deploy` falliva sul primo criterio (axum), che deve restare. Regex ora `kubernetes|deploy|axum` (ogni voce o è il criterio conservato o è quello nuovo; `fields_must_not_match` tokio|test invariato).
- `precise_first_message`: tre cause. (a) prompt: la scadenza «due mesi» finiva nel `why` e non in `constraints` (ora il prompt lo richiede). (b) caso: la regex `due mesi|produzione|server` valeva per ogni voce di `constraints`, quindi anche «un'ora la sera» la faceva fallire: allargata a `mesi|produzione|server|\bora\b|sera`; la presenza della scadenza resta misurata dalla rubrica `faithful`. (c) prompt: introduzione di 74 parole che copia frasi dei campi (`max_words_when_tool_called`, `text_must_not_repeat_tool_fields`).
- `vague_becomes_precise`: prompt. Introduzione che ripeteva «primi capitoli del libro» e il punto di partenza; in 1 run su 3 chiamava anche `request_lesson` dopo la bozza (errore di validazione, `max_calls` 2). Rubrica `no_false_promise` (step 4): il giudice non sapeva che dopo la conferma lo stato è `researching` e che «sto cercando le fonti» è corretto (lo ordina il prompt); aggiunta una nota al criterio, senza cambiarne la sostanza.
- `ready_change_explicit`: **prompt**. Con richiesta esplicita («aggiorna la mia mission») il Tutor faceva un'intervista (A/B/C) invece di chiamare lo strumento: il prompt diceva «cambia ciò che è cambiato» ma non cosa fare quando il nuovo obiettivo contraddice tutti i criteri, e il modello preferiva non inventare. Ora: chiama subito, criteri dalle sole parole della persona. Rubrica `no_invented` aggiornata (prima ammetteva «o chiede cosa dovrà saper fare», in contraddizione con `tools.called`).

Iterazioni (3 run per caso, 8 casi; costo per misura 0,57-0,62 $):
| versione | vague_becomes_precise | precise_first_message | correction_after_proposal | ready_change_explicit | altri |
|---|---|---|---|---|---|
| v2 (base) | 0/3 | 0/3 | 0/3 | 0/3 | 4 casi 3/3 |
| v3 a (intro corta, constraints, cambio esplicito, casi corretti) | 2/3 | 3/3 | 3/3 | 3/3 | tutti 3/3 |
| v3 b (aggiunto «nessun altro testo/strumento, nemmeno `request_lesson`») | 1/3 (2 run: `request_lesson` dopo la bozza) | 3/3 (rubrica min 0,38: messaggio doppio) | 3/3 | 3/3 | `ready_change_implicit` 2/3 |
| v3 c (**attuale**: strumento senza testo prima, introduzione dopo l'esito) | 1/3 | 3/3 (rubrica 1,00) | 3/3 (0,92) | 3/3 (1,00) | `ready_change_implicit` 1/3 |

Controlli deterministici rimasti falliti nella v3 c: solo `text_must_not_repeat_tool_fields` (4 parole consecutive in comune, per esempio «servizio di pagamenti che», «i2c in rust senza»), in 2 run su 3 di `vague_becomes_precise` e 2 su 3 di `ready_change_implicit`. Sono sovrapposizioni sul nome del tema, quasi inevitabili in una frase sull'obiettivo; il controllo è più severo del prompt («non copiare le frasi»). `ready_change_implicit` era 3/3 con v2 e v3 a: oscilla tra le versioni per questo solo controllo. Proposta: soglia di 5 parole o esclusione del `why` (richiede una modifica a `crates/evals`, di rust-backend-dev).

Regressioni `tutor/phase12` e `tutor/phase17` **non eseguite** con la v3: il budget del compito (2 $) era finito dopo 4 misure di `phase3` (2,39 $ in totale). Stima: 0,55 $ (reale probabilmente ~0,35 $). `cargo test -p application --lib`: 384 passati.

### Fase 17 ter: eccezione «punto di partenza non dichiarato» (2026-11-18, `tutor-phase17-v4`)

Problema: la regola v3 «richiesta esplicita di cambio: `propose_mission_change` subito» faceva chiamare lo strumento anche con la mission confermata dal punto di partenza «non dichiarato» e senza che la persona avesse detto cosa sa già (`tutor/phase12` `change_requested_with_undeclared_starting_point_asks_first`: 0/1 nella run `2026-11-18_19-53-40-519`, 3 giri di strumenti). Le due regole si contraddicevano: la prima non aveva eccezioni, la seconda stava più sotto con un «chiediglielo prima» debole.

Correzione (nessun nome di strumenti vietati): nel paragrafo della richiesta esplicita è dichiarata l'eccezione («tranne quando il punto di partenza della confermata è «non dichiarato» e la persona non ha ancora detto che cosa sa già»); nel paragrafo sul punto di partenza ora «in questo turno non chiamarlo, fai una sola domanda su che cosa sa già, poi proponi», con il perché. Stessa eccezione nella `description` di `propose_mission_change` (`tutor-tools-phase17-v4`). Testi di `request_lesson` invariati.

Esiti v4 (Tutor e giudice `claude-sonnet-5-5`), confronto con v3:
| suite / caso | v3 | v4 |
|---|---|---|
| phase12 `change_requested_with_undeclared_starting_point_asks_first` | 0/1 (rubrica 0,73) | **3/3** (rubrica media 0,91, min 0,73; `tool_rounds` max 2) |
| phase3 `ready_change_explicit` | 3/3 | 3/3 (rubrica 1,00) |
| phase3 `ready_change_implicit` | 1/3 (solo `text_must_not_repeat_tool_fields`) | 0/3, stessa causa (4 parole in comune col tema: «firmware per sensori e», «i2c in rust senza»); rubrica 1,00. Rumore del controllo già noto, non causato dall'eccezione |
| phase17 (3 casi) | 1/1 ciascuno | 1/1 ciascuno, rubrica 1,00 |
| phase12 `asks_starting_point_before_proposing` | 1/1 (rubrica 0,75) | 3/3 (0,80) |
| phase12 `from_scratch_is_written_explicitly` | 1/1 (0,50) | 3/3 (0,80) |
| phase12 `confirmed_without_starting_point` | 1/1 (0,71) | 3/3 (0,93) |
| phase12 `starting_point_in_first_message_not_asked_again` | 0/1 | 0/3: `constraints` = «Mezz'ora al giorno» non contiene `tre settimane|colloquio` (il caso va in 3 step, la scadenza è ora nel `why`/primo turno); rubrica 0,94. Già fallito con la v3: non è una regressione, ma la regex per voce del caso è troppo stretta (stesso difetto di `precise_first_message`) |

Note: rubrica `one_question` e `asks_starting_point` falliscono a volte (due domande nello stesso messaggio, messaggio che non finisce con la domanda), difetto di intervista già presente. `no_invented` e `honest_scope` nello step 2 di `confirmed_without_starting_point`: il giudice contesta dettagli arrivati nei messaggi successivi e «Vuoi che prepari la prossima lezione?», non toccati da questa modifica.

Costo: circa 0,67 $ (0,158 caso mirato; 0,099 phase3; 0,024 phase17; 0,389 altri 4 casi phase12; 0,003 un'esecuzione interrotta). Le stime `--dry-run` sono ~1,8 volte il reale. `cargo test -p application --lib`: 384 passati.

### Fase 17 quater (2026-11-18): controlli del runner

Due controlli fallivano senza difetti reali del Tutor (`tutor-phase17-v4`); corretti in `crates/evals/src/roles/tutor.rs`:

- `text_must_not_repeat_tool_fields`: finestra da 4 a 5 parole, e le sequenze che la persona ha scritto nel turno non contano (il nome del tema, per esempio «I2C in Rust senza std», torna legittimamente nell'introduzione).
- Nuovo `fields_any_match` (almeno una voce corrisponde). `precise_first_message` (`constraints`: `due mesi`) e `starting_point_in_first_message_not_asked_again` (`constraints`: `tre settimane|colloquio`) lo usano al posto di `fields_must_match`, che vale per ogni voce e non può dire «la scadenza c'è tra i vincoli». Rubrica `faithful` di `precise_first_message`: «niente esperienza coi container» spostato dai vincoli al punto di partenza (contraddiceva `fields_must_not_match`).

| caso (3 run) | v4 prima | v4 dopo |
|---|---|---|
| phase12 `starting_point_in_first_message_not_asked_again` | 0/3 | 3/3 (rubrica 1,00; la scadenza ora è nei vincoli) |
| phase3 `precise_first_message` | 3/3 | 3/3 (0,92) |
| phase3 `vague_becomes_precise` | 1/3 (v3) | 3/3 (0,80-0,87, due misure) |
| phase3 `ready_change_implicit` | 0/3 | 3/3 (1,00) |

Costo: 0,51 $ (0,32 + 0,18).

### Issue #29: `unpassed_block` nelle schede del Recorder (2026-11-18, `recorder-issue29-v5`)

Prompt: ogni scheda ha `unpassed_block` (stringa o `null`, sempre presente nello schema): l'id dell'esercizio **mai superato** (nessun tentativo `correct`) di cui la scheda mette alla prova il punto sbagliato; `null` per errori poi corretti, esercizi superati e ogni altra scheda; l'id mai nel testo. `repair.md`: id che non regge -> `null`.

Controlli nuovi (`crates/evals/src/roles/recorder.rs`, `recorder/phase7`): `unpassed_block_only_on_never_passed` e `unpassed_block_not_in_visible_text` (default, tutti i casi), `unpassed_block_includes_one_of` (per caso: `real_lesson_1_unproven_point` con `q2`/`q4`), più rubrica `unpassed_block_scope`.

Misura (Sonnet 5.5, giudice Sonnet 5.5, 3 run per caso), controlli ok / rubrica media:
| caso | v4 (0,41 $) | v5 (0,45 $) |
|---|---|---|
| `explained_only_zero_records` | 3/3, 1,00 | 3/3, 1,00 |
| `misconception_corrected` | 2/3, 1,00 | 3/3, 0,93 |
| `supersession` | 1/3, 0,83 | 0/3, 1,00 |
| `flashcard_only_weak_evidence` | 3/3, 1,00 | 3/3, 1,00 |
| `existing_term_and_record_not_duplicated` | 3/3, 1,00 | 3/3, 1,00 |
| `rich_lesson_limits` | 3/3, 0,33 | 2/3, 0,58 |
| `real_lesson_1_unproven_point` | 2/3, 0,75 | 2/3, 0,58 |
| `real_lesson_2_second_attempt_pass` | 2/3, 0,88 | 3/3, 0,88 |

Esito: i tre controlli sul campo passano in tutte le 24 run (mai su esercizi superati o corretti, mai nel testo); `unpassed_block_includes_one_of` 3/3 su `real_lesson_1_unproven_point` (sempre `q4`). **Limite:** in 2 run su 3 di quel caso nessuna scheda mette alla prova q2 (quiz_single sbagliato), quindi la rubrica `error_card`/`unpassed_block_scope` cala: il modello copre il punto `partial` ma salta l'errore del quiz. Non regressione (il difetto c'era in v4: prompt non toccato per questo). Le altre differenze sono rumore: i fallimenti rimasti sono `card_front_single_question` (fronti «che tipo ha il parametro e come la chiama il chiamante?», «in quale punto... e come...»), `no_exclamation_marks` in `supersession` (v4 1/3, v5 0/3 per le stesse due cause) e `expect.terms` in `rich_lesson_limits` (1 run con 0 termini). Costo totale dell'issue lato prompt: 0,86 $.

**Correzione `recorder-issue29-v6` (2026-11-18).** `system.md`: «Schede di ripasso» dice ora che **ogni** esercizio mai superato con un punto distinto ha la sua scheda con il proprio `unpassed_block` (anche un quiz_single sbagliato), entro 10 schede; la scheda dell'errore al singolare diventa «una scheda per ciascun errore mai superato». Nuovo controllo `unpassed_block_includes_all_of` (tutti gli id compaiono in qualche scheda), usato su `real_lesson_1_unproven_point` con `["q2","q4"]` insieme a `includes_one_of`. Misura (Sonnet 5.5, 3 run per caso, 0,46 $), controlli ok / rubrica media, v5 -> v6:
| caso | v5 | v6 |
|---|---|---|
| `explained_only_zero_records` | 3/3, 1,00 | 3/3, 1,00 |
| `misconception_corrected` | 3/3, 0,93 | 3/3, 0,93 |
| `supersession` | 0/3, 1,00 | 0/3, 1,00 |
| `flashcard_only_weak_evidence` | 3/3, 1,00 | 3/3, 1,00 |
| `existing_term_and_record_not_duplicated` | 3/3, 1,00 | 3/3, 1,00 |
| `rich_lesson_limits` | 2/3, 0,58 | 1/3, 0,50 |
| `real_lesson_1_unproven_point` | 2/3, 0,58 | 2/3, 0,70 |
| `real_lesson_2_second_attempt_pass` | 3/3, 0,88 | 3/3, 0,88 |

Esito: `unpassed_block_includes_all_of` 3/3 (q2 e q4 sempre coperti); `unpassed_block_only_on_never_passed` e `unpassed_block_not_in_visible_text` passano in tutte le 24 run. I fallimenti rimasti sono i soliti `card_front_single_question` (`supersession`, `rich_lesson_limits` «quale versione... e perché?», 1 run di `real_lesson_1_unproven_point`), `no_exclamation_marks` in `supersession` e `expect.terms` di `rich_lesson_limits`: stesse cause della v5, rumore o difetti non legati a questa modifica (`rich_lesson_limits` 2/3 -> 1/3 per `card_front_single_question`, fronti «Compila? Perché?»-simili «quale versione... e perché?»; da tenere d'occhio). In `real_lesson_1_unproven_point` 1 run su 3 ha avuto un avviso del Validator (scheda con `record_index`/`term_key` incoerenti: risolto dalla riparazione); la rubrica `unpassed_block_scope` cala per il giudice che confonde `record_index` con `unpassed_block`, non per il campo. Un solo ritocco del prompt.

### Issue #14: avvisi di qualità e miglioramento (2026-11-19, `generator-issue14-v11`)

Prompt: `system.md` (quiz: markup uniforme nelle opzioni, `quiz_option_markup_uneven`; completa: la risposta non compare nel testo vicino, `fill_blank_answer_in_text`, risposte di al massimo 3 parole), `repair.md` (istruzioni per i due nuovi errori), `improve.md` (una riga di rimedio per avviso). Runner: `DocAttempt` ha `improve` e `warnings` (calcolati da `LessonValidator::quality_warnings`, nessuna regola copiata nelle eval); il documento finale è scelto come fa il backend (miglioramento tenuto solo se valido e con `warning_score` minore). Nuovo controllo `quality_warnings_max` (soft, default di `generator/phase12`: 0) e metriche `warnings_before`, `warnings_after`, `score_before`, `improve_tried`, `improve_kept`.

Misura (Sonnet 5.5, 3 run per caso, giudice Sonnet 5.5; fermata a 13 run su 15 dal tetto di 3 $ sulla stima): costo **2,65 $**.
- Valido al primo colpo: 4/13 (`beginner` 3/3; `rust_second` 0/3; `narrow` 0/3; `five_concepts` 1/3; `four_concepts_recall` 0/1). Errori visti al primo colpo: `unused_entry`, `primary_source_count`, `inline_code_too_long`, `fill_blank_answer_in_text` (1 volta).
- Avvisi: 0 prima e dopo in 12 run su 13. Un solo documento con avvisi (`four_concepts_with_recall_block_budget`: `fill_blank_answer_too_long`, punteggio 1): miglioramento tentato, **non tenuto**. Miglioramenti tenuti: 0/13 (campione troppo piccolo per giudicare `improve.md`).
- Controlli duri falliti in altri campi (non nuovi): `estimated_minutes_range`, `must_include_block_types`, `concept_chain`/`exercises_per_concept_section`, `term_first_mention_is_glossary_link`, `quiz_option_char_ratio_max`. Rubriche medie 0,24-0,46 (giudice severo, già noto).

### Issue #26: attribuzioni di `generator/phase12-fetch` (2026-11-19, `generator-issue26-v12`..`v14`)

Solo `unreachable_source_still_generates`, 3 run per versione (Sonnet 5.5, `max_fetches = 1`, giudice con `JUDGE_PAGE_CHARS` 40000). Spesa totale ~2,0 $. Il caso era 3/3 in fallo su `nothing_unread_attributed` con v11.

| versione | `nothing_unread_attributed` | `citations_only_annotated` | note |
|---|---|---|---|
| v11 (stasera) | 0/3 | 0/3 | `[^key]` su regole ed esempi, `why` che «tratta» export/source |
| v12 (§«Lettura fallita» in tre regole + verifica finale) | 0/1 (run fermata dal budget 0,8 $) | 0/1 | |
| v13 (+ `[^key]` solo su 1-2 frasi generali) | 0/3 | non rilevato | costo medio ~0,27 $ |
| v14 (+ `why` con le parole dell'annotazione) | 0/3 | 2/3 | costo medio ~0,24 $ |

Le citazioni sono migliorate (v14). L'attribuzione nel `why` del `primary_source` no: il modello continua a scrivere cosa cercare nella guida con argomenti della lezione (`export`, `source`). Il criterio è severo: un `why` utile dice per forza qualcosa del contenuto. In una run il giudice scrive che l'annotazione «non è mostrata»: il giudice potrebbe non vederla (da verificare nel codice di `evals`). Fermato dopo due tentativi falliti sullo stesso punto. Non misurati: `rust_book_page_facts`, `three_pages_requests_and_cost`, `injection_in_fetched_page`.

Non regressione con v14: `three_pages_requests_and_cost` 2 run (fermato dal budget, 0,88 $), rubrica media 0,96 (min 0,92), `page_content` e `nothing_unread_attributed` mai in fallo, `unsupported_citations` 0, 1 lettura per run, costo ~0,44 $ a run (la lezione da 3 fonti, 21-24k token di output). `injection_in_fetched_page` 1 run, rubrica 1,00, nessuna istruzione eseguita, 0,13 $. Il «0/1 checks ok» del caso injection e il «0/2» dell'altro sono controlli deterministici da leggere a mano: non indagati. Spesa totale della sessione ~3,0 $.

Proposta per `unreachable_source_still_generates`: spostare la regola nel codice. A lettura fallita il backend dovrebbe imporre un `why` fisso del `primary_source` (costruito dall'annotazione) e togliere la key dalle citazioni consentite nel testo, con un errore del Validator. Il prompt da solo non lo ottiene in modo stabile.

Verifica del tech lead (stessa sera): il giudice non riceveva le fonti con l'annotazione, quindi ogni `why` che nominava un argomento della guida era un fallo. Ora `generator.rs` passa al giudice titolo, URL, annotazione e `use_for` di ogni fonte, e la rubrica `nothing_unread_attributed` del caso dice che ripetere l'annotazione (anche nel `why`) è lecito. Con v14: 2 run, 2/2 controlli, rubrica media 0,96 (min 0,93), nessun fallo di attribuzione, ~0,26 $ a run. Decisione: `max_fetches = 1` in `deploy/teach_me.toml`. La proposta nel codice resta in backlog.

## Missing component (`generator/missing_component.cases.json`)

Tre casi, atteso `missing_component` dove il catalogo non basta: `growth_curves_need_chart` (grafico con assi, 0 `diagram`), `code_compare_i_already_know_go` (copia di `phase6`, mission «conosco già Go»), `no_missing_component_when_catalog_suffices` (negativo, 0). Prompt `generator-issue26-v19`. Prima misura (1 run): il grafico produce una `diagram` forzata e nessun `missing_component`; `code_compare` ok in 1 run su 1 (0 su 2 con v17); il negativo passa. Nota: `growth_curves_need_chart` è stato corretto (tolto `version` dal piano) dopo un run fallito per piano non valido.

### Campo `visual` del piano (2026-11-20, Planner v5, Generator `generator-visual-v20`)

Il piano decide la figura: `concepts[].visual` facoltativo (`none`, `flow`, `tree`, `cycle`, `code_compare`, `line_chart`, `comparison_table`, `timeline`, `sequence`; al massimo 2 concetti diversi da `none`), reso al Generator come riga `Figura: X` dopo «Errore tipico». Il Generator la tratta come un ordine; senza riga niente figure. `improve.md` ha il punto «Il piano chiede per il concetto …». Nello schema del Planner `visual` NON è in `required` (i vecchi piani restano validi). Fixture: `visual` aggiunto ai piani fissi di `generator/diagram` e `missing_component` (positivi: `flow`, `tree`, `cycle`, `line_chart`, `code_compare`; negativi tutti `none`); due casi nuovi in `planner/phase12` (`visual_flow_for_cache_process`, `visual_none_for_format_rules`, controllo solo da rubrica). Diagnosi di `growth_curves_need_chart` (0 documenti in 2 run): il piano aveva il campo `version` non ammesso, già tolto, e l'ultima misura produce un documento (con una `diagram` forzata): ora la fixture è valida. Misure: da fare dopo «Rust pronto».


## Issue #30: lingua dei contenuti (`*/issue30-en.cases.json`, 2026-11-21; ADR 0019 punto g)

Una suite per ruolo, due casi inglesi ciascuna, traduzione di casi italiani rappresentativi (stesso scenario; missione, contesto, esercizi, rubrica e risposte in inglese): `tutor/issue30-en` (`vague_start_en`, `content_question_en`), `researcher/issue30-en` (`tech_rust_cli_en`, `hobby_opt_out_en`), `planner/issue30-en` (`beginner_zero_records_general_prereqs_en`, `convention_pitfall_not_invented_en`), `generator/issue30-en` (`rust_first_lesson_en`, `fill_blank_no_leak_en`), `grader/issue30-en` (`correct_own_words_en`, `incorrect_with_misconception_en`), `recorder/issue30-en` (`explained_only_zero_records_en`, `misconception_corrected_en`). Ogni caso ha `content_language: "en"`, `deterministic.language: "en"` e le regex inglesi; una rubrica `in_english` (peso 1, 3 nel Generator) misura che tutto il testo per la persona sia inglese. `runs_per_case` 1.

Come si lancia (Sonnet 5.5 per tutor, researcher, recorder, planner (`medium`), generator; Haiku 5.5 per grader; giudice predefinito Sonnet 5.5; Anthropic diretto, config TOML fuori dal repo): `run-evals --runs 1 --max-cost-usd 2.5 --suite <ruolo>/issue30-en ...`. Regressione italiana, 1 run, 2 casi per ruolo: `--case tutor/phase2:vague_start --case tutor/phase2:content_question --case researcher/phase4:tech_rust_cli --case researcher/phase4:hobby_opt_out --case planner/phase12:beginner_zero_records_general_prereqs --case planner/phase12:convention_pitfall_not_invented --case generator/phase6:rust_first_lesson --case generator/phase6:fill_blank_no_leak --case grader/phase7:correct_own_words --case grader/phase7:incorrect_with_misconception --case recorder/phase7:explained_only_zero_records --case recorder/phase7:misconception_corrected` (più le `--suite` corrispondenti).

### Risultati reali (2026-11-21, 1 run per caso; esito = controlli deterministici, punteggio = media pesata della rubrica; ultima misura per caso)

Spesa totale 4,32 $ (tetto 6 $). Cartelle in `results/`: `2026-11-21_14-31-59-192` (inglese, 1° giro), `14-38-00-446` (italiano), `14-44-03-352`, `14-48-31-499`, `14-52-39-205` (riprove inglesi dopo le correzioni dei prompt), `14-53-21-685` (italiano con i prompt finali di planner, recorder, generator).

| ruolo | caso | it | en |
|---|---|---|---|
| tutor | vague_start | ok 0.71 | ok 0.75 |
| tutor | content_question | ok 1.00 | ok 1.00 |
| researcher | tech_rust_cli | ok 0.76 | ok 0.89 |
| researcher | hobby_opt_out | ok 0.83 | ok 0.85 (1° giro: fallito, 4 risorse su minimo 5) |
| planner | beginner_zero_records | ok 0.28 | ok 0.79 (v10); ok 0.63 (v11); ok 0.55 (v12, 2 run) |
| planner | convention_pitfall | ok 0.75 (v10); ok 0.50 (v11); ok 0.67 (v12, 2 run) | **fallito** 0.54 (v10): `pitfall` che comincia ancora con «Pensare that…»; ok 0.77 (v11); **ok 0.81 (v12, 2 run, min 0.62)** |
| generator | rust_first_lesson | fallito 0.58 (rapporto lunghezza opzioni 1.50) | fallito 0.55 (nessun documento valido in 3 tentativi; con v22 i controlli passavano, 0.48) |
| generator | fill_blank_no_leak | fallito 0.62 (nessun documento valido in 3 tentativi) | fallito 0.27 (stesso motivo, più `simply`) |
| grader | correct_own_words | ok 0.83 | ok 0.86 |
| grader | incorrect_with_misconception | ok 1.00 | ok 0.83 (1° giro: feedback in italiano) |
| recorder | explained_only_zero_records | ok 1.00 | ok 1.00 |
| recorder | misconception_corrected | ok 1.00 | ok 0.69 (1° giro: due «?» nel fronte) |

Lettura: i fallimenti del Generator sono gli stessi in italiano e in inglese (`fill_blank_answer_in_text` e riparazione che non converge, già noti da `generator/phase6`): non dipendono dalla lingua. Le regressioni italiane dopo le modifiche ai prompt non ci sono (stessi esiti dei controlli sui casi di planner, recorder, grader, tutor, researcher). Aperto: il Planner inglese scrive ancora «Pensare that» nel `pitfall` del caso `convention_pitfall_not_invented_en` (3 correzioni al prompt, `planner-issue30-v7`..`v9`, senza effetto; nel caso `beginner` la formula inglese invece è rispettata). Idea per il prossimo giro: togliere dal prompt la formula italiana letterale e chiedere «l'equivalente di "Pensare che…" nella lingua dei contenuti», oppure far diventare `pitfall_form` un controllo del Validator per lingua.

Modifiche ai prompt di questo giro: `grader-issue30-v7` (feedback e misconception seguono sempre la lingua dei contenuti, gli esempi italiani non la decidono), `planner-issue30-v7`..`v10` (la formula del `pitfall` dipende dalla lingua; la v10 chiede «l'equivalente di «Pensare che…» nella lingua dei contenuti» senza formula fissa: in inglese il modello scrive ancora «Pensare that…», ancora aperto; cartella `2026-11-21_14-59-05-061`), `planner-issue30-v11` (2026-11-21, cartella `2026-11-21_15-02-33-134`, 0,134 $: `planner/context.md` elenca sotto «Lingua dei contenuti» le due aperture del `pitfall`, «Pensare che…» per l'italiano e «Thinking that…» per l'inglese; `system.md` rimanda a «l'apertura indicata nel contesto» senza citare la formula; 1 run: `pitfall_form` e tutti i controlli passano in inglese e in italiano, il difetto «Pensare that» è sparito; la rubrica 0,63 / 0,77 / 0,50 pesa su `nothing_assumed`, `zpd_small_step`, `in_english` (objective «Saper explain…» in `beginner`), `pitfall_not_invented`: rumore di una sola run, da rimisurare con più run), `planner-issue30-v12` (cartella `2026-11-21_15-04-10-937`, 0,281 $, 2 run per caso: `context.md` aggiunge l'apertura dell'`objective` per lingua, «Saper…» / «Be able to…»; `system.md` dice che gli esempi sono in italiano e l'apertura segue il contesto; controlli deterministici 2/2 in tutti e 3 i casi, nessun «Pensare that» né «Saper explain», `in_english` mai fallito; la rubrica bassa del `beginner` (0,55) viene da `nothing_assumed`, `zpd_small_step`, `support_concepts`: problema di contenuto, non di lingua, aperto), `generator-issue30-v22` e `v23` (parole vietate anche in inglese; etichette fisse come «Errore tipico» e «Sembra giusto perché…» tradotte), `recorder-issue30-v8`/`v9` (una sola domanda sul fronte, con esempio negativo).
