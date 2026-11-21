# `glossary_term` e sheet del glossario

## Scopo didattico
**Pre-training** (Mayer): i termini nuovi si presentano *prima* dei concetti che li usano, in modo che la memoria di lavoro non debba imparare termine e concetto insieme. Il blocco `glossary_term` è la presentazione esplicita in-flow del termine; i `[[key]]` inline sono il richiamo a richiesta. "Non confonderlo con…" (`avoid`) previene le misconcezioni più comuni (contrasto, Bjork).

## Il blocco in-flow

```
┌──────────────────────────────────────────────┐ fill knowledgeSoft, raggio md, padding 16, nessun bordo
│ (📖) Termine                                  │ icona book-marked 24 (knowledge) + ui.label knowledge
│ ownership                                    │ term: ui.title3 (Inter 600 18/26), ink
│ La regola per cui ogni valore ha un solo     │ definition: reading.body 17/28, ink (md inline)
│ proprietario alla volta.                     │
│ ─────────                                    │ hairline line 1 dp (solo se c'è `avoid`)
│ Non confonderlo con: proprietà intellettuale │ ui.body inkSoft; "Non confonderlo con" in ui.label
└──────────────────────────────────────────────┘
```

- Contrasto: `ink` su `knowledgeSoft` 14,3 / 7,7; `knowledge` su `knowledgeSoft` 5,3 / 5,8 (etichetta e icona).
- `avoid` è una riga con gli elementi separati da «;» (max 3) preceduta da «Non confonderlo con:». Se vuoto, nessuna riga.
- Il blocco non è interattivo (il tocco non apre niente: la definizione è già lì), ma la selezione del testo funziona.
- Larghezza piena colonna, gap 24 dp.
- **Chiave sconosciuta** (`key` non in `glossary`): blocco non renderizzabile → riga neutra di fallback, nessun errore a schermo.

## Sheet del glossario (da `[[key]]` e dal pannello di contesto)

Un solo componente `GlossarySheet`, `BottomSheet` di `base-components` §9, **su ogni classe di larghezza** (su expanded: larghezza `dialogWidthWide` 480 centrata, angoli superiori `lg`, ancorato in basso; la lettura sotto resta visibile dietro lo scrim 0,4).

```
──────  (maniglia)
Termine                           ui.label knowledge
ownership                         ui.title2
La regola per cui ogni valore…    reading.body
Non confonderlo con
  • proprietà intellettuale       ui.body inkSoft
[ Chiudi ]                        Button secondario 48 dp
```

- Contenuto: `term` (`ui.title2`), `definition` (md inline; `interactive = false` per i `[[…]]` interni: la definizione non rimanda ad altri termini nel v1), `avoid` come elenco puntato, Button «Chiudi». Se il termine è citato più volte, lo sheet non cambia.
- Focus iniziale sul titolo (`term`); Esc/Indietro/scrim/«Chiudi» chiudono e restituiscono il focus al span di origine.
- **Non sposta lo scroll** della lezione e non cambia schermata (contiguità, P4).
- Memorizza (nello stato locale della lezione) quali termini sono stati aperti: nel pannello di contesto sono segnati con `check` 16 dp `inkSoft` (non "vinti": solo "già visti").

## Stati
Blocco: statico. Termine inline: normale / premuto (`surfacePressed` 100 ms) / focus (tastiera). Sheet: aperto/chiuso.

## Accessibilità
- Blocco: un nodo «Termine: ownership. <definizione>. Non confonderlo con: …».
- Span inline: «<testo>, termine del glossario» con azione custom «Apri definizione».
- Sheet: `paneTitle` = termine; focus iniziale sul titolo; chiusura restituisce il focus al punto di origine; `Esc` chiude.

## Contenuti al limite
- Definizione 400 caratteri: ≈ 6 righe; lo sheet non supera il 90 % dell'altezza (scorre).
- 3 voci `avoid` da 120 caratteri: vanno a capo.
- `term` lungo: va a capo.

## Linee guida per il Generator
- **Glossario del documento** (0-12 voci): solo termini che l'utente non conosce *e* che servono all'obiettivo. `term` nella forma originale della comunità (ADR 0005 D3); `definition` in italiano, 1-2 frasi ≤ 250 caratteri, definita **per funzione** («La regola per cui…»), non circolare, senza usare altri termini ancora sconosciuti.
- `avoid`: la confusione *realistica* (un termine vicino o l'uso comune della parola), 1-2 voci brevi.
- Blocco `glossary_term`: solo per i 1-2 termini **indispensabili** a capire la sezione, posti prima del paragrafo che li usa. Tutti gli altri compaiono solo inline `[[key]]` alla prima menzione (poi testo semplice).
- Ogni voce di glossario deve essere usata (`unused_entry`). Nessuna definizione che contenga la risposta di un esercizio della lezione.
