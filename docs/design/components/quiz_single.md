# `quiz_single`

> Regole comuni (cornice, stati, feedback, riprova, tastiera, TalkBack): [_exercises.md](_exercises.md). Qui solo ciò che è specifico.

## Scopo didattico
**Retrieval practice con feedback elaborato**. Non serve a "misurare": serve a far *tentare* una previsione e poi a spiegare perché ogni opzione è giusta o sembra giusta e non lo è (P5, P6). Il quiz è valido solo se **nessuna opzione tradisce la risposta** (P7): stesso stile, stessa forma, stessa altezza di partenza, nessun elemento colorato o animato prima di «Controlla».

## Anatomia

Prima di controllare:

```
┌──────────────────────────────────────────────────┐ ExerciseCard
│ (◉) Scelta singola                                │
│ Dopo la riga 2, si può ancora usare `a`?          │ reading.question
│ Scegli una risposta.                              │ ui.body inkSoft
│                                                  │
│ ┌──────────────────────────────────────────────┐ │ ChoiceRow ×N (2-5), gap 8 dp
│ │ ○  Sì, perché `a` è ancora nello scope    [1] │ │ min 56 dp, raggio md, bordo border 1,5
│ └──────────────────────────────────────────────┘ │ [1] = KeyHint (solo tastiera fisica)
│ ┌──────────────────────────────────────────────┐ │
│ │ ○  No, perché il valore è stato spostato  [2] │ │
│ └──────────────────────────────────────────────┘ │
│ ┌──────────────────────────────────────────────┐ │
│ │ ○  Sì, ma solo per leggerlo               [3] │ │
│ └──────────────────────────────────────────────┘ │
│                                                  │
│ [ Controlla ]  Scegli una risposta per controllare.│
└──────────────────────────────────────────────────┘
```

Dopo aver controllato (sbagliato, scelta 1; la corretta è 2):

```
│ ┌─ fill incorrectSoft, bordo incorrect 2 ───────┐ │
│ │ ◉  Sì, perché `a` è ancora nello scope        │ │
│ │ (x) La tua scelta · non corretta              │ │ icona 20 + ui.label, incorrect
│ └───────────────────────────────────────────────┘ │
│ ┌─ fill correctSoft, bordo correct 1,5 ─────────┐ │
│ │ ○  No, perché il valore è stato spostato      │ │
│ │ (✓) Risposta corretta                         │ │
│ │ Con `let b = a;` la proprietà passa a `b`: …  │ │ feedback dell'opzione corretta, ui.body ink
│ └───────────────────────────────────────────────┘ │
│ ┌───────────────────────────────────────────────┐ │ non scelta, errata: invariata
│ │ ○  Sì, ma solo per leggerlo                    │ │
│ │ ▸ Perché no?                                   │ │ disclosure 48 dp (Button testuale)
│ └───────────────────────────────────────────────┘ │
│ ┌ (x) Non proprio ─────────────────────────────┐  │ StatusPanel incorrect, a 12 dp
│ │ Sembra ragionevole perché `a` è ancora        │  │ corpo = feedback dell'opzione SCELTA
│ │ nello scope, ma lo scope non decide…          │  │
│ └───────────────────────────────────────────────┘  │
```

## Misure e token (`ChoiceRow`, `base-components` §9)

| Elemento | Valore |
|---|---|
| Altezza min riga | 56 dp (`choiceMinHeight`), cresce col testo; tutte le righe stesso padding **16 × 12** e stessa tipografia |
| Testo | `ui.bodyL` (Inter 16/24) `ink`, md inline con `interactive = false` |
| Indicatore | radio 24 dp, tratto `border` 1,5; selezionato: punto pieno `accent` 12 dp dentro anello `accent` 2 dp |
| Gap fra righe | 8 dp (`xs`), esattamente il minimo fra target |
| Default | fill `surface`, bordo `border` 1,5 dp (contrasto bordo ≥ 3,9 su `surface`) |
| Pressed | fill `surfacePressed`, 100 ms |
| Selezionata | fill `accentSoft`, bordo `accent` 2 dp, indicatore pieno; `selectionChange` 100 ms uguale per tutte |
| Focus | anello `focus` 3 dp, offset 2 dp, sul contorno della riga |
| Controllata | fill/bordo vedi [_exercises.md](_exercises.md) §4; indicatore resta nella posizione della scelta |
| `KeyHint` | 24 × 24 dp a destra, solo con tastiera fisica |
| Tag di esito | riga sotto il testo: icona 20 + `ui.label` nel colore `x`, padding top 8 dp |
| Feedback in opzione | `ui.body` (14/20) `ink`, separato dal tag da 4 dp; md inline con termini tappabili (`interactive = true` **solo dopo** `checked`: l'opzione non è più selezionabile) |

**Nessun indizio (P7)**: le righe prima di "Controlla" sono bit-per-bit identiche salvo il testo e la selezione. Nessun ordinamento o animazione che dipenda dalla correttezza; l'altezza varia solo col testo. Il Validator limita la differenza di lunghezza (`quiz_option_length`). L'app **non riordina** le opzioni (l'ordine è del Generator, che varia la posizione della corretta).

## Interazione

- Tocco su una riga = seleziona (cambia liberamente finché non controlla). Una sola attiva. Non si deseleziona toccando di nuovo (come un radio nativo).
- «Controlla» attivo solo con una selezione; motivo sotto il bottone finché disabilitato: «Scegli una risposta per controllare.».
- Dopo «Controlla»: righe bloccate; pannello a 12 dp; disclosure «Perché no?» sulle errate non scelte (apre `feedback` dell'opzione; `expandVertically` 200 ms; stato `expanded` in `semantics`); **nessun «Riprova»**.
- Tastiera: ↑↓ muovono focus e selezionano; 1-5 selezionano; Invio con selezione = Controlla; Ctrl+Invio = Controlla. Dopo il controllo il focus va al pannello.

## Accessibilità
`selectableGroup`; ogni riga `Role.RadioButton` con `selected`, `collectionItemInfo` (riga N di M). Prima del controllo, TalkBack legge «<testo>, non selezionato, 2 di 4». Dopo: «<testo>. La tua scelta, non corretta. <feedback>» (un nodo). Pannello con live region polite e focus. Nessun uso di colore da solo. Font 200 %: le righe crescono, nessuna altezza fissa.

## Contenuti al limite
- 2 opzioni (es. Vero/Falso): ok ma sconsigliato (50 % a caso): il Validator non lo vieta; il Generator preferisce 3-4.
- 5 opzioni da 200 caratteri: altezza ≈ 5 × 100 = 500 dp; scorre; «Controlla» in fondo.
- Opzione con codice inline lungo (`Vec<HashMap<String, u32>>`): `softWrap` con spezzatura sicura.
- Feedback dell'opzione mancante/vuoto (documento non valido): il pannello mostra solo l'etichetta.

## Linee guida per il Generator
Vedi README §5. Riassunto operativo:
- Domanda completa, risolvibile senza vedere le opzioni; una sola idea; riferita a un esempio nuovo rispetto a quello spiegato.
- 3-4 opzioni (max 5), tutte della stessa forma grammaticale, differenza di lunghezza ≤ 3 parole, nessun grassetto/termine/citazione nelle opzioni.
- Ogni distrattore = una misconcezione reale, nominata nel `feedback` («Sembra giusto perché…, ma…»); il feedback della corretta spiega la regola.
- Varia la posizione della corretta fra i quiz della lezione (`quiz_answer_position`).
- Vietati: «Nessuna delle precedenti», «Tutte le precedenti», negazioni doppie, opzioni con «sempre/mai» solo negli errati, la corretta che ripete parole della domanda mentre gli errati no.
- Il feedback non cita la lettera o la posizione dell'opzione.

> **Codice nella domanda** (`code` facoltativo, contratto `sdui.md` §4.1): posizione, scorrimento, accessibilità e comportamento dopo la risposta in [_exercises.md](_exercises.md) §10.
