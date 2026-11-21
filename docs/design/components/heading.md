# `heading`

## Scopo didattico
Dà la **struttura** (segnalazione, Mayer) e fissa i confini delle **sezioni** (segmentazione): un `heading` di livello 2 apre una sezione, cioè una idea e un ritmo spiegazione → esercizio. Il livello 1 è il titolo della lezione; il livello 3 è un sottotitolo dentro una sezione.

## Anatomia e misure

```
Ownership e move                         h1  ui.title1 (Inter 700 26/34, ls -0.1)  ink
Obiettivo · circa 8 min                  ObjectiveLine (solo dopo l'h1, vedi sotto)
────────────────────────────────────
Quando `a` smette di essere valida?      h2  ui.title2 (Inter 600 21/28)           ink
Cosa succede al valore                   h3  ui.title3 (Inter 600 18/26)           ink
```

| Livello | Stile | Spazio sopra | Spazio sotto | Semantics |
|---|---|---|---|---|
| 1 | `ui.title1` | 0 (è il primo blocco) | 12 dp | `heading()`, `isTraversalGroup` no |
| 2 | `ui.title2` | 48 dp (`xxl`; 0 se è il primo contenuto della sezione appena rivelata) | 12 dp | `heading()` |
| 3 | `ui.title3` | 32 dp (`xl`) | 12 dp | `heading()` |

- Colore sempre `ink`, **nessun** accento, nessuna numerazione, nessun filetto decorativo. Testo semplice (non md); `softWrap`, mai ellissi (fino a 120 caratteri va a capo).
- Le intestazioni usano Inter (UI), i paragrafi Literata: il cambio di famiglia segnala "struttura" senza bisogno di colore.
- **ObjectiveLine** (non è un blocco; la disegna l'app subito dopo l'h1 usando `objective` e `estimated_minutes`): riga `ui.label` `inkSoft` «Obiettivo» (icona `target` 20 dp) e sotto `reading.body` `ink` con `objective`; poi `ui.body` `inkSoft` «Circa 8 minuti». Gap dall'h1: 12 dp; dal blocco successivo: 24 dp. Non è un riquadro. Motivo: l'utente sa *cosa saprà fare* (obiettivo di apprendimento esplicito, advance organizer) senza che il Generator lo ripeta.
- Un h2 **non** è tappabile e non si collassa.

## Stati
Statico. Nessuno stato interattivo. Il focus di TalkBack/tastiera può arrivarci solo come nodo di lettura.

## Interazioni
Nessuna. Quando una sezione viene rivelata, il suo h2 riceve il **focus di accessibilità** e l'app scorre per portarlo sotto la barra (vedi `screens/lesson.md`).

## Accessibilità
`semantics { heading() }` su tutti i livelli (navigazione per intestazioni di TalkBack). Il livello non è esposto dalla piattaforma: l'ordine nel documento lo codifica. Titolo come `paneTitle` non serve. A font 200 % il titolo va a capo; nessuna altezza fissa.

## Contenuti al limite
- 120 caratteri: 2-3 righe in h2 a 600 dp, ok.
- Parole lunghe/identificatori (`HashMap<String, Vec<u32>>`) → spezzare con `softWrap` e `Hyphens.None` (l'identificatore non si sillaba).
- Due h2 consecutivi senza contenuto: il secondo non crea una sezione vuota (il Validator lo vieta, `section_without_content`); se capita, la sezione vuota si salta e il suo titolo si mostra comunque.
- Più di un h1: solo il primo è "titolo"; gli altri si disegnano come h2 senza aprire sezione (tolleranza).
- Primo blocco non h1 (documento tollerato): non c'è ObjectiveLine; nessun errore a schermo.

## Linee guida per il Generator
- Primo blocco: `heading` livello 1 = `title` del documento (stesso testo).
- Un `heading` livello 2 per sezione (3-6 sezioni, richiamo e chiusura comprese: `components/README.md` §1.1-1.4), **mai due consecutivi**, sempre seguiti da contenuto.
- Titoli di sezione **concreti e informativi** (la sezione si ritrova dall'indice): «Quando `a` smette di essere valida», non «Introduzione», «Approfondimento», «Parte 2». Una domanda o un'affermazione breve (≤ 60 caratteri consigliati). **Non rivelare la risposta** di un esercizio successivo (l'indice mostra tutti i titoli fin dall'inizio).
- Il livello 3 è riservato all'**esempio svolto** («Esempio: <caso concreto>»): uno per sezione di concetto, mai altrove (`components/README.md` §1.1). Segnala dove comincia il caso concreto e spezza il muro di testo; non serve per i sottoargomenti (una idea per sezione).
- Niente markdown, emoji, numerazione («1.»), punti finali.
