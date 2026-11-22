# Baseline delle eval

Politica: ADR `docs/adr/0021-politica-eval.md` (punto 5: i risultati si confrontano con una baseline salvata, indicizzata per `prompt_version`). Qui stanno le baseline **committate**; `evals/results/` (git-ignored) contiene solo le run grezze.

## Cosa è una baseline

Il risultato di una run reale di una suite, con la `prompt_version` che l'ha prodotta, sul modello di produzione del ruolo (vedi `evals/README.md`), con **almeno 3 run per caso** (il giudice oscilla di 0,2-0,3 tra run: con una sola run non si distingue il rumore da una regressione).

Un file per suite: `<ruolo>__<suite>__<prompt_version>.json`, per esempio `tutor__conversation__tutor-v4.json`. Quando una modifica ai prompt è accettata, si aggiunge il nuovo file e si rimuove il vecchio della stessa suite (la storia è in git).

## Formato

`report.json` del runner (`crates/evals/src/report.rs`) **non va committato così com'è**: contiene testo di input e output di ogni turno, ogni chiamata e i verdetti del giudice (centinaia di KB, con contenuti generati). Si committa una sua riduzione, ottenuta da una suite di una run:

```json
{
  "suite": "tutor/conversation",
  "prompt_version": "tutor-v4",
  "model": "claude-sonnet-5-5",
  "judge_model": "claude-sonnet-5-5",
  "date": "2026-11-22",
  "runs_per_case": 3,
  "cost_usd": 0.0,
  "cases": {
    "vague_start": { "runs_checks_ok": 3, "rubric_mean": 0.71, "rubric_min": 0.5, "failed_checks": [] }
  }
}
```

I campi vengono da `report.json`: `suites[].id`, `suites[].prompt_version`, `started_at` (la data), `suites[].cost_usd`, `suites[].cases[].{id, runs_checks_ok, rubric_mean, rubric_min}`, `runs[].models` (il modello del soggetto) e `options` (`runs`, giudice). `failed_checks` = i nomi dei controlli con esito fallito nelle run del caso (`runs[].turns[].checks`). Il modello del giudice è quello passato con `--judge-model`.

Il runner produce questo formato da solo (`crates/evals/src/baseline.rs`): il report grezzo **non** è usabile come baseline per dimensione e perché non è indicizzato per suite. Rispetto all'esempio, ogni caso ha due campi in più, necessari al confronto: `runs` (le run del caso, per confrontare `runs_checks_ok` come rapporto se il numero di run cambia) e `cost_per_run_usd` (costo di soggetto e giudice diviso per le run). `model` è l'elenco dei modelli visti nelle run del soggetto.

Salvare, dopo una run reale (con `--runs 3`):

```
cargo run -p evals --bin run-evals -- --suite tutor/conversation --runs 3 --save-baseline evals/baselines/tutor__conversation__tutor-v4.json
```

Con più suite nella stessa run, `--save-baseline` vuole una **cartella esistente** e scrive un file per suite, già col nome `<ruolo>__<suite>__<prompt_version>.json`. Le suite saltate non producono file.

## Confronto con una baseline

`--baseline <FILE|CARTELLA>` (anche insieme a `--save-baseline`) confronta la run con la baseline, per caso, dopo la run: stampa una tabella breve e termina con **exit code 3** se c'è almeno una regressione. Il file si legge, e la destinazione di `--save-baseline` si verifica, prima della run: un percorso sbagliato non costa nulla. Con una cartella si leggono tutti i `*.json`, abbinati per `suite`.

```
cargo run -p evals --bin run-evals -- --suite tutor/conversation --runs 3 --baseline evals/baselines/tutor__conversation__tutor-v4.json
```

Non sono fatali, ma compaiono nelle note: suite o casi presenti solo da un lato; `prompt_version` diversa (è il caso normale); due file della stessa suite in una cartella (si usa il primo, con avviso). Modello o giudice diversi: la suite **non** è confrontabile, compare in evidenza come «NOT COMPARED» e viene saltata. Se alla fine nessun caso è stato confrontato, l'exit code è **4** (non 0: «niente da confrontare» non è un via libera). Un caso la cui `rubric_mean` passa da un valore a nessuno (giudice fallito) è una regressione.

Le soglie sotto sono implementate così (costanti in `baseline.rs`). Dove il testo lascia margine: `runs_checks_ok` si confronta come rapporto su `runs`; «un miglioramento di qualità che giustifichi» il costo = `rubric_mean` salita di almeno 0,10; le regole «mai prima del turno N» sono i casi `vague_pure` e `skip_interview` di `tutor/tools`, che devono avere tutte le run con i controlli ok. Le spiegazioni scritte di una regressione restano a chi legge.

Criterio per ogni caso:

1. Confronta `prompt_version`: la baseline e la run devono riguardare la stessa suite, lo stesso modello e lo stesso giudice; altrimenti il confronto non vale.
2. **Regressione** (la modifica non si accetta senza una spiegazione scritta) se vale almeno una di:
   - `runs_checks_ok` scende, o un controllo deterministico prima mai fallito ora fallisce (i controlli sono deterministici: una regressione qui è reale, non rumore);
   - `rubric_mean` scende di più di 0,10 (la soglia copre il rumore osservato del giudice con 3 run; con meno run non si conclude nulla);
   - `rubric_min` scende di più di 0,20;
   - le regole «mai prima del turno N» (`tutor/tools`: `vague_pure`, `skip_interview`) non passano in 3/3 run;
   - il costo per run sale di oltre il 25% senza un miglioramento di qualità che lo giustifichi.
3. Un caso che migliora o resta entro le soglie non è una regressione. Se tutti i casi sono entro le soglie, si sostituisce la baseline con la run nuova.
4. Un caso già sotto la soglia della suite (`pass_criteria`) nella baseline non peggiora ulteriormente: il confronto è con la baseline, non con la soglia.
