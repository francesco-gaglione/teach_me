# `flashcard`

> Comune: [_exercises.md](_exercises.md). Il ripasso vero (`screens/review.md`) usa lo stesso modello; qui è la versione dentro la lezione.

## Scopo didattico
**Richiamo di un fatto da fissare** con autovalutazione onesta. L'azione naturale deve essere *provare a ricordare prima di guardare* (retrieval practice, P5): il retro non è visibile e non c'è nessun aiuto. L'autovalutazione serve a calibrare (metacognizione) e a fissare il primo intervallo del ripasso spaziato, non a "dare un voto". Nessun colore sui voti (non premiare né punire la sincerità).

## Anatomia

Fronte:

```
┌──────────────────────────────────────────────────┐ ExerciseCard
│ (▤) Scheda                                        │
│                                                  │
│ Che cosa succede al valore di `a` dopo           │ front: reading.cardFront (Literata 500 24/34), ink
│ `let b = a;` se `a` è una `String`?              │
│                                                  │
│ Prova a ricordare prima di guardare la risposta. │ ui.body inkSoft
│ [ Mostra risposta ]                              │ Button secondario 48 dp (non primario: non è il passo "forte")
└──────────────────────────────────────────────────┘
```

Retro + voto:

```
│ Che cosa succede al valore di `a` …  (ridotto)   │ front: reading.body 17/28, inkSoft
│ ──────────                                       │ hairline line
│ Viene spostato: `a` non è più valida, il valore  │ back: reading.cardFront 24/34 ink (≤ 200 car.)
│ appartiene a `b`.                                │
│                                                  │
│ Quanto ti è venuta in mente?                     │ ui.label
│ ┌───────────────┐ ┌───────────────┐              │ 4 bottoni IDENTICI, secondari, min 64 dp
│ │ Non la        │ │ Con fatica    │              │ riga 2×2 (expanded/medium e compact fino a fontScale 1,3)
│ │ ricordavo     │ │               │              │
│ └───────────────┘ └───────────────┘              │
│ ┌───────────────┐ ┌───────────────┐              │
│ │ Bene          │ │ Facilissima   │              │
│ └───────────────┘ └───────────────┘              │
```

## Misure e token

| Elemento | Valore |
|---|---|
| Fronte | `reading.cardFront` 24/34 per la domanda (fatto da richiamare: dimensione "scheda"), `ink`. Card padding 20 (16 compact) |
| Retro | la risposta in `reading.cardFront` `ink` (stessa dimensione del fronte; sopra 200 caratteri `reading.body`); il fronte ridotto in `reading.body` `inkSoft` sopra, separato da hairline `line`; `cardReveal` 150 ms dissolvenza incrociata, **nessun flip 3D** |
| «Mostra risposta» | Button secondario 48 dp, a sinistra; sempre visibile sotto il fronte |
| Voti | 4 Button **secondari identici**: fill `surface`, bordo `border` 1,5, testo `ink` `ui.label`, min 64 dp, gap 8 dp, ordine fisso (peggiore→migliore): «Non la ricordavo» (`again`) · «Con fatica» (`hard`) · «Bene» (`good`) · «Facilissima» (`easy`). Nessun colore di esito, nessuna icona differente, nessuna riga di intervallo (in lezione non c'è la previsione FSRS) |
| Oltre fontScale 1,3 | i 4 voti in una colonna |
| Dopo il voto | i bottoni scompaiono; resta riga `ui.body` `inkSoft` con icona `check` 20: «Valutazione: Con fatica. La rivedrai nel ripasso.» + Button testuale «Cambia» (48 dp) che riapre i voti (l'ultimo voto prima di «Continua» vale; dopo che l'utente ha continuato è bloccato) |

## Stati
non svolto (fronte) → **rivelato** (retro + voti, `answering`) → **valutato** (`checked`). La rivelazione si fa **solo con tocco/tasto**: nessun timer, nessuna rivelazione automatica. `required: true` → il blocco è svolto alla valutazione, non alla rivelazione.

## Interazione e tastiera
Tocco su «Mostra risposta» (Spazio/Invio quando ha il focus; **Spazio** è anche scorciatoia globale del blocco quando a fuoco). Con tastiera: 1-4 danno il voto dopo la rivelazione (cifre mostrate come `KeyHint` sui bottoni con tastiera fisica); Ctrl+Invio non ha un'azione primaria qui (nessun voto di default: sarebbe un indizio di che voto "dare").

## Accessibilità
Fronte e retro sono un'unica area: alla rivelazione il retro è annunciato (`liveRegion = Polite`: «Risposta: …») e il focus passa a «Quanto ti è venuta in mente?». Voti: `Role.Button`, `contentDescription` «Con fatica». Ordine: fronte → bottone → (retro → titolo voti → quattro voti). Contrasto `ink` su `surface` ≥ 13,4.

## Contenuti al limite
- `front` 300 caratteri in 24/34: ≈ 5-6 righe; ok. `back` 600 caratteri in 24 sp sarebbero ≈ 12 righe: il retro usa `reading.body` 17/28 se supera 200 caratteri (soglia unica per leggibilità).
- Fronte con codice inline: ok; blocchi di codice non sono ammessi in `front`/`back` (md solo inline).
- Termini/citazioni: nel `front` non interattivi; nel `back` tappabili dopo la rivelazione.

## Linee guida per il Generator
Vedi README §8. In breve: una domanda **precisa, a risposta breve**, il fatto da fissare; il retro è la risposta in 1-3 frasi, con il motivo se utile (aiuta a ricordare); ogni scheda deve reggersi **fuori contesto** (diventerà una carta di ripasso); mai la risposta nel fronte; max 1 per sezione e di norma `required: false` se la sezione ha già un altro esercizio; mai per concetti che richiedono capire (usare quiz/fill/open).

> **Codice nella domanda** (`code` facoltativo, contratto `sdui.md` §4.1): posizione, scorrimento, accessibilità e comportamento dopo la risposta in [_exercises.md](_exercises.md) §10.
