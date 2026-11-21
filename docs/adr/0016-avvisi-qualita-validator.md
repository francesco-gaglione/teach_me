# ADR 0016: Avvisi di qualità del Validator, non bloccanti

- Stato: accettata
- Data: 2026-11-19
- Richiesta "Avvisi non bloccanti del Validator". Il proprietario ha deciso il salvataggio nel database e la visibilità nell'app il 2026-11-19; i punti 1-5 sono scelte del tech lead che ne derivano. Contratto: `docs/contracts/lessons.md` §2, §3.1 e §4 (v1.7). Regole e soglie: `docs/design/quality-warnings.md`.

## Problema

Il Validator (`sdui.md` §6) blocca il documento per ogni regola violata. Alcune regole sono indizi involontari che rivelano la risposta: la lunghezza di un'opzione corretta diversa dalle sbagliate (`quiz_option_length_cue`), la posizione fissa della risposta corretta (`quiz_answer_position`). Altre sono giudizi di stile, come un paragrafo troppo lungo o troppi box di approfondimento. Queste ultime hanno falsi positivi: un paragrafo lungo può essere la scelta giusta. Trattarle tutte come errori bloccanti costringe a riparazioni inutili e fa fallire lezioni buone.

## Scelta

1. **Due nuovi errori bloccanti**, perché sono indizi involontari che rivelano la risposta come le regole già esistenti:
   - la risposta di un `fill_blank` visibile nel testo vicino al suo segnaposto;
   - markup (grassetto, codice inline e simili) presente solo in alcune opzioni di un quiz.
2. **Gli altri controlli di stile diventano avvisi**: il documento è valido, si salva e riporta l'avviso in `quality_warnings` (`lessons.md` §2). Codici: `explanation_run_too_long`, `too_many_callouts`, `fill_blank_answer_too_long`, `consecutive_code_blocks`. Il messaggio è in italiano, scritto per la persona che studia.
3. **Una riparazione in più solo per lezioni con avvisi.** Se il documento è valido ma ha avvisi, il backend tenta una riparazione dentro il limite esistente (2 riparazioni). Tiene il documento riparato solo se è valido e ha meno avvisi, altrimenti tiene l'originale. Una lezione senza avvisi non costa nulla in più.
4. **Gli avvisi si salvano nel database** con la lezione, così restano disponibili su ogni dispositivo.
5. **L'app li mostra.** Un codice sconosciuto è un avviso generico che mostra solo `message`, così nuovi codici non richiedono una versione nuova dell'app.

Decisioni del tech lead (2026-11-19), che precisano i punti sopra:

- **Punto 2 (`consecutive_code_blocks`)**: la regex delle parole di confronto si applica solo al `caption` del secondo `code`, non al `code`.
- **Punto 3 (callout)**: valgono le soglie delle eval (per sezione 1; `tip`+`info` in lezione 2; `warning` in lezione 5), non il «> 3» della richiesta.
- **Punto 4 (tenuta della riparazione)**: punteggio pesato, `explanation_run_too_long` vale 2, gli altri avvisi 1. Si tiene il documento riparato solo se è valido e con punteggio strettamente minore.
- **Punto 5 (limite riparazioni)**: la riparazione per soli avvisi parte solo da un documento già valido e non sottrae tentativi a un errore. Se il riparato ha errori si tiene il precedente valido. Massimo una riparazione per avvisi per lezione, sempre dentro `max_document_repairs`.

## Alternative scartate

- **Tutti i controlli come errori bloccanti**: più rigore, ma molti falsi positivi e più lezioni che falliscono con `invalid_output`.
- **Tutti i controlli come avvisi**: avrebbe lasciato passare gli indizi che rivelano la risposta, che è il problema da evitare.
- **Avvisi solo nei log**: l'utente non vedrebbe nulla e il controllo di qualità non avrebbe effetto sul documento.
- **Riparare sempre gli avvisi, anche senza errori**: una chiamata LLM in più per ogni lezione, anche per quelle pulite.

## Conseguenze

- Contratto `lessons.md` v1.7 (`quality_warnings` in `Lesson` e `LessonView`, §4 per le regole di generazione), additivo.
- Design `docs/design/quality-warnings.md`: regole, soglie e testi dei messaggi (learning-designer).
- Backend: nuova categoria di esito del Validator (errore o avviso), migrazione additiva per la colonna degli avvisi, una riparazione condizionata agli avvisi.
- App: visualizzazione degli avvisi per lezione, con fallback generico per codici sconosciuti.
- Costo: una chiamata LLM in più solo per le lezioni con avvisi e nessun errore bloccante, entro il limite di riparazioni già previsto.
