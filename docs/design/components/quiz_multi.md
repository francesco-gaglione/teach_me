# `quiz_multi`

> Comune: [_exercises.md](_exercises.md). Struttura e misure come [quiz_single.md](quiz_single.md); qui le differenze.

## Scopo didattico
Far **classificare** (quali tra queste vale/non vale) richiamando più elementi; verifica discriminazione fra concetti vicini. Più esigente di `quiz_single`: l'esito esatto richiede tutte e sole le corrette, quindi il feedback **opzione per opzione** è la parte più preziosa. Rischio: l'utente seleziona tutto: l'istruzione non dice quante sono corrette (no indizio) ma dice che possono essere più di una.

## Anatomia

```
│ (☑) Scelta multipla                               │
│ Quali di queste operazioni spostano il valore?    │
│ Scegli tutte le risposte corrette.                │  istruzione fissa
│ ┌───────────────────────────────────────────┐    │
│ │ ☐  `let b = a;` con `a: String`          [1]│    │  checkbox 24 dp, quadrato raggio sm
│ └───────────────────────────────────────────┘    │
│ ┌───────────────────────────────────────────┐    │
│ │ ☑  `let b = a;` con `a: i32`             [2]│    │  selezionata: fill accentSoft, bordo accent 2, spunta onAccent su fill accent
│ └───────────────────────────────────────────┘    │
│ …  (3-6 opzioni)                                  │
│ [ Controlla ]  Scegli almeno una risposta.        │
```

Dopo il controllo (parziale):

```
│ ┌ (−) Quasi ─────────────────────────────────┐   │ StatusPanel partial
│ │ Hai trovato 1 delle 2 risposte corrette e   │   │ corpo generato dall'app:
│ │ 1 che non lo è.                             │   │ "Hai trovato {k} delle {n} risposte corrette
│ └─────────────────────────────────────────────┘   │  [ e {m} che non lo {è/sono}]."
│ [↺ Riprova]                                       │
```

Il corpo del pannello è **testo generato dall'app** (non LLM), perché il contratto non ha un `feedback` complessivo: «Hai trovato 2 delle 2 risposte corrette.» (correct), «Hai trovato 1 delle 2 risposte corrette e 1 che non lo è.» / «Hai trovato 1 delle 3 risposte corrette.» (partial), «Nessuna delle risposte che hai scelto è corretta.» (incorrect; nessuna intersezione). Singolare/plurale con `plurals`. Il perché sta nelle opzioni.

## Differenze di stato/misura rispetto a `quiz_single`

| Punto | `quiz_multi` |
|---|---|
| Indicatore | checkbox 24 dp (raggio `sm`), tratto `border` 1,5; selezionata: fill `accent`, spunta `onAccent` |
| Selezione | tocco alterna; nessun limite |
| «Controlla» | attivo con **almeno 1** selezionata; motivo: «Scegli almeno una risposta per controllare.» |
| Esito | `correct`: insieme uguale a `correct`; `partial`: interseca senza coincidere; `incorrect`: nessuna intersezione (nessuna intersezione) |
| Marcature | scelta corretta ✓, scelta non corretta ✗, **corretta non scelta** «Era corretta» (icona `check-circle`), non scelta e non corretta: invariata + «Perché no?» |
| Feedback per opzione | mostrato sotto il testo di **ogni opzione scelta** e di **ogni corretta non scelta** (contratto §5); le altre dietro «Perché no?» |
| Riprova | sì (selezione azzerata); caption «Secondo tentativo. Per il ripasso conta il primo.» |
| Tastiera | ↑↓ spostano il focus (non selezionano); Spazio o 1-6 alternano; Ctrl+Invio controlla |
| TalkBack | gruppo `selectableGroup`, ogni riga `Role.Checkbox` con `toggleableState`; annuncio all'ingresso del gruppo: «Scelta multipla, più risposte possibili» |

## Contenuti al limite
- 6 opzioni da 200 caratteri e feedback di 400 ciascuna: dopo il controllo la card può raggiungere ~1500 dp; scorre. Il pannello di sintesi resta sotto le opzioni (contiguità con «Riprova») e il `bringIntoView` porta all'inizio del pannello; le opzioni espanse si leggono scorrendo verso l'alto.
- Tutte le opzioni selezionate: ammesso, l'esito sarà `partial`/`incorrect`; nessun avviso («hai scelto tutto» sarebbe un indizio).

## Linee guida per il Generator
Vedi README §5. In più:
- La domanda dice «Quali…?» e che **ci sono più risposte** senza dirne il numero; 3-6 opzioni; **1..(n−1) corrette**, mai tutte, mai nessuna (`quiz_correct_missing`). Le corrette non devono essere sempre in numero uguale a quello degli errori né in posizioni "a scacchiera".
- Ogni opzione ha `feedback` (sia le corrette sia le errate): scrivilo perché sia letto da chi l'ha **scelta** (motivo per cui sbagliava / perché regge) e, per le corrette, anche da chi **non l'ha scelta** (perché doveva sceglierla). Quindi per le corrette scrivi sempre una spiegazione che regge da sola («Anche questa sposta il valore, perché…»).
- Opzioni indipendenti fra loro (la correttezza di una non deve dedursi da un'altra).
- Usalo per distinzioni (movimento vs copia; vero in questo caso vs sempre), non per elenchi di fatti.

> **Codice nella domanda** (`code` facoltativo, contratto `sdui.md` §4.1): posizione, scorrimento, accessibilità e comportamento dopo la risposta in [_exercises.md](_exercises.md) §10.
