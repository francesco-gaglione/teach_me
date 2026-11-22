# ADR 0021 — Politica delle eval

- Stato: accettata
- Data: 2026-11-22
- Contesto: audit esterno del 22/11/2026, punto C3. Non esisteva una ADR sulle eval: questa è la prima.

## Problema

Le eval dei prompt (`backend/evals/`) chiamano il provider LLM con chiavi reali e costano. Il repository è pubblico su GitHub e la CI pubblica non deve avere chiavi. Senza una regola, una modifica ai prompt può peggiorare le lezioni senza che nessuno se ne accorga, oppure può passare senza una misura del costo e della qualità.

## Scelta

1. **Smoke eval in CI, a costo zero.** La CI esegue una suite di smoke con un provider LLM finto, senza chiavi e senza spesa. Controlla il cablaggio e il formato, non la qualità delle risposte.
2. **Eval reali manuali.** Le eval con LLM reale restano manuali, lanciate dal proprietario o dal tech lead.
3. **Eval reali obbligatorie prima di unire modifiche sotto `backend/prompts/`.** Ogni cambiamento dei prompt si verifica con eval reali prima del merge.
4. **Modello più economico capace.** Le eval reali usano il modello più economico in grado di svolgere il compito, perché il credito è limitato.
5. **Confronto con una baseline salvata.** I risultati si confrontano con una baseline salvata, indicizzata per `prompt_version`.

## Alternative scartate

- **Chiavi reali nella CI pubblica:** espongono le chiavi e consumano credito a ogni push.
- **Solo eval manuali, senza smoke in CI:** nessun controllo automatico sul cablaggio a ogni modifica.

## Conseguenze

- La CI non dipende da chiavi e non costa.
- Le modifiche ai prompt richiedono una run reale manuale prima del merge, con il costo che ne deriva.
- Il confronto con la baseline rende visibile una regressione per versione di prompt.
