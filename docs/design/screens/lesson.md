# Schermata Lezione

> Fase 5. Riferimento: **tablet orizzontale (expanded, 1280 × 800 dp)**, poi medium e compact (ADR 0003). Contratto: `docs/contracts/sdui.md`. Specifiche dei blocchi: `../components/` (README, `_exercises.md`, un file per tipo). Ogni scelta è motivata dal principio (P1-P12, `design-system.md` §1).

## 1. Scopo didattico

Portare l'utente a **una vittoria concreta** (`objective`, formulato come capacità): capire un'idea e *dimostrarla* in un esercizio con feedback spiegato. Lezione da **10-15 minuti** (`estimated_minutes`, calcolato dal backend), di 4-5 concetti, che spiega per intero un concetto per sezione, a **sezioni** al ritmo dell'utente (segmentazione, P3), una idea per sezione (P1). La schermata non misura e non giudica: **registra tentativi** (il primo conta) e mostra cosa si è dimostrato.

## 2. Modello: documento → sezioni

- Lista piatta di blocchi → **sezioni**: il blocco prima del primo `heading` livello 2 è la **sezione 0** («Introduzione»: h1 + `ObjectiveLine` + blocchi introduttivi); ogni h2 apre una sezione fino al successivo h2. `N` = numero di h2. Senza h2: una sola sezione, progresso nascosto.
- **Rivelazione**: all'apertura è visibile la sezione 0 (o la sezione salvata). Una sezione si rivela con «Continua» in fondo alla precedente (`ContinueBar`, §6). Le sezioni già viste restano in pagina e si possono rileggere scorrendo.
- **Progresso** = sezioni, non blocchi: sottotitolo della TopBar «Sezione 2 di 4» (nella sezione 0 il sottotitolo è «Introduzione»). `ProgressBar` (4 dp, `accent`, traccia `line`) a filo del bordo inferiore della TopBar, a tutta larghezza: valore = `(k − 1 + e/E) / N` con `k` sezione corrente, `E` esercizi obbligatori della sezione, `e` svolti (0 se `E = 0`); 100 % dopo «Completa lezione». Animazione `progress` 300 ms. `progressBarRangeInfo` + testo: mai una barra senza parole.
- **Stato persistente** (per `lesson_id`, ripristinato a riapertura e dopo riavvio): sezioni rivelate, risposte/bozze/ordinamenti per `block.id`, esiti del primo tentativo, checklist, termini aperti, disclosure del pannello, posizione di scroll (`block.id` + offset). Se il documento cambia (rigenerazione), lo stato di `id` che non esistono più si scarta.

## 3. Layout expanded (≥ 840 dp) di riferimento: 1280 × 800 dp

Il testo non è mai più largo di `readingMaxWidth` 600 (45-75 caratteri per riga, P10); lo spazio laterale ospita **contesto**, non testo più largo (ADR 0003 §2). Gruppo `colonna + gap + pannello` = 600 + 32 (`contextPaneGap`) + 280 (`contextPaneWidth`) = **912 dp**, centrato nella finestra.

```
 1280 dp
┌───────────────────────────────────────────────────────────────────────────────────────────┐
│ ✕  Ownership e move                                                          ☁̸ (se coda)   │ TopBar 56 dp
│    Sezione 2 di 4                                                                          │ sottotitolo ui.body inkSoft
│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│ ProgressBar 4 dp
│                                                                                           │
│        184        ┌───────────── 600 ──────────────┐  32  ┌──────── 280 ────────┐  184   │
│                   │ Quando `a` smette di essere      │      │ INDICE              │        │
│                   │ valida?                  (h2)    │      │ ✓ Introduzione      │        │ pannello di contesto
│                   │                                  │      │ ● Quando `a` smette │        │ (sticky, scroll proprio,
│                   │ In Rust ogni valore ha un unico  │      │ ○ Copia e clone     │        │  top = TopBar + 24 dp)
│                   │ proprietario. Quando [lo sposti] │      │ ○ Riepilogo         │        │
│                   │ …                                │      │ ─────────────       │        │
│                   │ ┌──────────────────────────────┐ │      │ ▸ GLOSSARIO (2)     │        │ disclosure
│                   │ │ let a = String::from("ciao");│ │      │ ▸ FONTI (1)         │        │ disclosure
│                   │ └──────────────────────────────┘ │      │                     │        │
│                   │ ┌ Scelta singola ──────────────┐ │      └─────────────────────┘        │
│                   │ │ …esercizio…                  │ │                                     │
│                   │ │ [Controlla]                  │ │                                     │
│                   │ └──────────────────────────────┘ │                                     │
│                   │ (🔒) Rispondi alla domanda per   │                                     │
│                   │      continuare.                 │                                     │
│                   │ [         Continua            ] │  56 dp, larghezza colonna             │
│                   └──────────────────────────────────┘                                     │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```


| Elemento | Specifica |
|---|---|
| **TopBar** | `TopBar` di `base-components` §6: ✕ «Chiudi lezione» (48 dp, a sinistra), titolo `title` (1 riga a font normale, `ui.title3`; a font ≥ 1,5 fino a 2 righe), sottotitolo «Sezione k di N» `ui.body` `inkSoft`. A destra: indicatore `cloud-off` 24 dp con `contentDescription` «Alcune risposte non sono ancora inviate» solo se la coda locale è non vuota; su compact/medium e expanded < 1100: pulsante **Indice** (`list`, 48 dp, «Indice, termini e fonti»). Nessun menu ⋮ in Fase 5 |
| **Colonna di lettura** | larghezza `min(600, finestra − 2×margine)`; allineata al gruppo centrato; padding verticale: 24 dp sopra il primo blocco, 96 dp in fondo (spazio di respiro dopo `ContinueBar`) |
| **Pannello di contesto** | `contextPaneWidth` 280 dp, a **destra** della colonna (la lettura resta il primo oggetto del flusso, l'indice è consultazione: P1); `position: sticky` sotto la TopBar a 24 dp; altezza = viewport − TopBar − 48; scorrimento interno proprio; fill trasparente (su `bg`), **nessun bordo**, separato dalla colonna da 32 dp di vuoto; titoli dei blocchi del pannello `ui.label` (maiuscole normali: «Indice», «Glossario», «Fonti») |
| **Breakpoint** | pannello visibile se larghezza finestra ≥ `contextPaneBreakpoint` **1100 dp**; sotto, il pannello non esiste e il suo contenuto sta nello sheet del pulsante Indice (§3.3) |
| **Margini** | oltre il gruppo il resto è `bg` vuoto; a 840-1099 dp: colonna centrata 600 con margini ≥ 32 |

### 3.1 Contenuto del pannello (consultazione volontaria, mai in primo piano)

Tre blocchi, in questo ordine, ognuno con intestazione disclosure (48 dp, `chevron-down/right` 20 dp + testo `ui.label` `ink`, conteggio `(n)` in `inkSoft`). **Indice** aperto di default, **Glossario** e **Fonti** chiusi di default; lo stato aperto/chiuso è ricordato.

**Indice** — una riga (min 48 dp, padding 12 × 8, raggio `md`) per sezione:

| Stato | Icona (20 dp) | Testo | Riga |
|---|---|---|---|
| completata (superata con «Continua») | `check` `inkSoft` | titolo `ui.body` `ink`; sotto `ui.bodyS` `inkSoft`: «Completata» | tappabile → scorre alla sezione |
| **corrente** | `circle-dot` `accent` | titolo `ui.label` `ink`; sotto «In corso» | fill `accentSoft` (`ink` 13,9 / 8,6), tappabile |
| non ancora rivelata | `circle` `border` | titolo `ui.body` `inkSoft`; sotto «Da fare» | **non tappabile**; `stateDescription` «Si sblocca dopo la sezione precedente», nessun movimento |

Sezione 0 si chiama «Introduzione». I titoli di tutte le sezioni sono visibili fin dall'inizio (advance organizer: l'utente vede dove va), per questo il Generator non mette risposte nei titoli. Nessuna stima "minuti rimasti" (pressione, P9): `estimated_minutes` è fisso, mostrato sotto il titolo («Circa 12 minuti»).

**Glossario** — elenco dei termini della lezione **già incontrati** nelle sezioni rivelate (prima menzione); una riga (48 dp) per termine: `ui.body` `ink` + `check` 16 dp `inkSoft` se già aperto. Vuoto: «I termini compariranno man mano che leggi.» (`ui.body` `inkSoft`). Tocco → `GlossarySheet` (`../components/glossary_term.md`). *Perché solo i già incontrati*: il pannello è sempre visibile e le definizioni potrebbero essere la risposta di un esercizio ancora da fare (retrieval, P5).

**Fonti** — fonti citate (`[^key]`) o `primary_source` delle sezioni rivelate: riga con numero come nel testo «[1]» `ui.labelS` `knowledge` + titolo (`ui.body` `ink`, a capo) + dominio (`ui.bodyS` `inkSoft`). Tocco → sheet «Fonte» (titolo, dominio, «Apri» che porta al browser, «Chiudi»). **Fonte senza `url` (dal tuo PDF)**: riga e sheet diversi, vedi §3.3.

### 3.2 Dove stanno le azioni

| Azione | Posizione | Motivo |
|---|---|---|
| **Continua** / **Completa lezione** | `ContinueBar` **in fondo alla sezione corrente, nella colonna** (non sticky, non in alto a destra) | vicina a ciò che si è appena letto/svolto (contiguità, P4); un bottone sempre visibile spingerebbe a saltare (P3: il ritmo è dell'utente) e coprirebbe il contenuto. A 800 dp di altezza la sezione può essere più lunga del viewport: «Continua» si raggiunge scorrendo, ed è la prima cosa dopo l'ultimo blocco |
| **Controlla / Invia** | dentro l'esercizio (`_exercises.md` §3) | contiguità |
| **Chiudi lezione** (✕) | TopBar, a sinistra | salva e torna all'argomento senza dialog (nessuna perdita) |
| **Progresso** | TopBar (sottotitolo + barra) | sempre visibile, non intrusivo; il pannello ripete lo stato per sezione |
| **Ascolta** (blocco) / **Ascolta la lezione** | icona silenziosa sotto ogni blocco leggibile; `volume-2` nella TopBar con barra audio mentre legge | l'ascolto è opzionale e non compete con la lettura; spec in `lesson-audio.md` (issue #8) |

### 3.3 Medium e compact (e expanded < 1100)

```
 compact 360 × 800                       sheet "Indice" (dal pulsante, o dalla voce nel TopBar)
┌─────────────────────────┐             ┌─────────────────────────┐
│ ✕ Ownership e move  [≡] │ TopBar      │ ──── (maniglia)         │ BottomSheet, max 90% altezza
│   Sezione 2 di 4        │             │ Indice                  │
│▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░│             │  ✓ Introduzione         │
│ colonna, margine 16     │             │  ● Quando `a` smette…   │
│ …                       │             │  ○ Copia e clone        │
│ [      Continua       ] │             │ ▸ Glossario (2)         │
└─────────────────────────┘             │ ▸ Fonti (1)             │
                                         │ [ Chiudi ]              │
                                         └─────────────────────────┘
```

- Una colonna, margine 16 (compact) / 24 (medium) / 32 (expanded stretto), larghezza max 600, centrata. Il resto come expanded.
- Il pulsante **Indice** apre il `BottomSheet` con **lo stesso contenuto** del pannello (stesso composable `LessonContextContent`, fuori da un contenitore diverso). Su expanded < 1100 lo sheet è largo `dialogWidthWide` 480 e ancorato in basso/centro; il resto della lezione resta visibile dietro lo scrim. Tocco su una sezione chiude lo sheet e scorre alla sezione.
- Sotto 360 dp o con font ≥ 1,5: invariato (colonna unica).
- Rotazione/ridimensionamento: lo stato (§2) non dipende dalla classe di larghezza; il passaggio ≥1100 ↔ <1100 mostra/nasconde il pannello senza perdere posizione di scroll.

## 4. Regole di flusso

1. **Prima richiama, poi rivela** (P5). Gli esercizi stanno dopo la spiegazione breve della sezione; i quiz non mostrano mai risposta o suggerimento prima di «Controlla»; la flashcard rivela solo a richiesta; `open_answer` richiede di scrivere per inviare (`../components/_exercises.md`).
2. **Controlla esplicito**: la scelta si può cambiare finché non si preme. Poi risposta bloccata e feedback specifico con icona e parole (non solo colore) a 12 dp dalla domanda (`feedbackAppear` 200 ms). Dopo un errore il feedback spiega *perché sembrava ragionevole e perché no* (P6).
3. **Un esercizio obbligatorio per sezione sblocca «Continua»** (§5). I facoltativi (`required: false`) mostrano il chip «Facoltativo» e non bloccano.
4. **Nessun timer, nessun punteggio numerico, nessuna soglia** (P9): il riepilogo dice *cosa hai dimostrato*.
5. **Uscita**: ✕ salva e torna all'argomento, senza dialog. Il sistema Indietro fa lo stesso. Riaprire riporta alla posizione salvata (non all'inizio).
6. **Pre-training** (P12): i termini del glossario compaiono prima dei concetti; sono tappabili ovunque (tratteggio `knowledge`) e aprono lo `GlossarySheet`.
7. **Fonte e dubbi** in chiusura (ultima sezione): `primary_source` («Per approfondire»), `lesson_link`, `ask_agent` («Hai dubbi? Chiedi al Tutor»), poi «Completa lezione».
8. **Blocchi sconosciuti / malformati / `missing_component`**: vedi `../components/missing_component.md` (fallback o riga neutra, mai blocco o colore d'errore, mai obbligatori).
9. **Lingua**: `lang` per blocco → `LocaleList` del testo per TalkBack.
10. **Nessun contenuto si anima** all'ingresso salvo la rivelazione di sezione: `sectionReveal` 300 ms (dissolvenza + slide ≤ 24 dp), poi scroll animato 300 ms `out` fino a portare l'h2 a 24 dp sotto la TopBar; con «riduci animazioni»: istantaneo. Il focus di accessibilità va all'h2 della nuova sezione.

11. **Una sezione = un concetto spiegato per intero.** Nell'ordine: intuizione, esempio svolto (h3 «Esempio: …»), regola, errore tipico (`callout` `warning`), poi 1-2 esercizi (`../components/README.md` §1.1). Il renderer non cambia: la struttura è nel documento; qui si fissano le conseguenze per la schermata (§4.1).
12. **Richiamo dei prerequisiti** (se il piano ne ha di «da richiamare»): è la **prima** sezione h2, con esercizio facoltativo (§4.2). Titolo nell'indice «Cosa ti serve: …».

### 4.1 Sezioni da ~300 parole sul tablet orizzontale (1280 × 800)

Colonna di lettura 600 dp ≈ 63 caratteri per riga (nella fascia 45-75). Una sezione di concetto occupa **2-3 schermate** di scorrimento, non una: è voluto (spiegazione completa) ed è il motivo per cui la lunghezza si controlla con la **densità di interruzioni**, non con meno parole.

```
┌ colonna 600 dp ───────────────────────────┐
│ ## Quando `a` smette di essere valida      │ h2 (24 dp sotto la TopBar dopo la rivelazione)
│ [paragraph  40-80 parole]   intuizione     │ ┐ ≤ 2 paragrafi consecutivi
│ ### Esempio: la String spostata            │ ┘ (≈ 180 parole ≈ 1 schermata)
│ [code 3-12 righe]                          │ riposo visivo (monospazio, sfondo `surface`)
│ [paragraph 50-100]  passo per passo        │ ┐
│ [paragraph 30-70]   regola, **grassetto**  │ ┘ coppia finale
│ [callout warning  Errore tipico: …]        │ riposo visivo (icona + tinta)
│ [quiz / fill_blank …]                      │ ExerciseCard: la pausa attiva
│ [ContinueBar]                              │
└────────────────────────────────────────────┘
```

- **Punti di riposo**: dopo l'intuizione (h3), dopo il codice, dopo la regola (callout), dopo l'esercizio. Nessun tratto di sola prosa supera **≈ 180 parole** (2 paragrafi) né una schermata (≈ 740 dp utili sotto TopBar e barra).
- **Rivelazione**: la sezione si rivela intera (non a blocchi): al tocco di «Continua» scorre alla h2 (§4.10). L'utente non perde il filo, perché l'h3 e il callout sono ancore visive per la rilettura e per TalkBack (navigazione per intestazioni).
- **Rilettura**: l'h3 «Esempio:» è il punto a cui si torna da un esercizio che lo richiede; non servono link né pulsanti nuovi.
- **Compact (< 600 dp)**: stessa struttura; i paragrafi diventano ≈ 40 caratteri per riga: la regola dei 2 paragrafi consecutivi resta valida. **Font 200 %**: ogni paragrafo occupa ≈ il doppio; la sezione scorre di più ma non cambia struttura.
- **Progresso**: invariato (sezioni, non blocchi): con 6 sezioni «Sezione 4 di 6». Nessun «minuti rimasti» (P9).

### 4.2 Sezione di richiamo

Prima sezione h2, solo se il piano ha prerequisiti `general`. Contenuto (`../components/README.md` §1.2): 1 paragrafo che annuncia, **1 esercizio `required: false`**, 1 paragrafo di ripasso per prerequisito (max 3). L'esercizio sta **prima** del ripasso (retrieval prima della rilettura).

- Esercizio facoltativo ⇒ `E = 0`: la barra di progresso non avanza dentro la sezione (valore `(k − 1)/N`) e «Continua» è **già sbloccato** (§5): chi sa il prerequisito prosegue senza attrito; la voce dell'indice dice «Da fare» come le altre e, una volta superata, «Completata».
- Nessun testo che giudichi il livello della persona («ripasso per principianti»): il titolo e il primo paragrafo parlano di ciò che la lezione *usa*, non di ciò che la persona *non sa*.
- Chi ha dimostrato (`record`) o dichiarato (`mission`) il prerequisito non vede la sezione: l'assenza è la regola, non uno stato da spiegare.

## 5. Obbligatorietà e sblocco

**Cosa è obbligatorio.** Un esercizio (`quiz_single`, `quiz_multi`, `fill_blank`, `order_steps`, `flashcard`, `open_answer`) con `required == true` (default). **Non** lo sono: `required: false`, `checklist`, `glossary_term`, `callout`, `lesson_link`, `ask_agent`, `primary_source`, blocchi sconosciuti o non renderizzabili. Gli esercizi dentro un `fallback` di `missing_component` si comportano come tutti gli altri.

**Quando è "svolto".** Dopo che l'utente ha *dato una risposta*, giusta o sbagliata: quiz/fill/order dopo «Controlla»; flashcard dopo l'autovalutazione; `open_answer` dopo l'invio accettato **localmente** (la valutazione del Grader può arrivare dopo: se la rete manca, la risposta resta in coda, conta come svolta e il pannello dice «Risposta registrata»: un guasto non dell'utente non blocca). 

**Si può proseguire dopo una risposta sbagliata. Sì.** Il valore sta nel tentativo di richiamo e nel feedback spiegato (P5, P6); bloccare finché è giusta produce prove a caso su 3-5 opzioni, contraddice P9 e falsa la misura (conta il primo tentativo). L'errore non si perde: è registrato, genera scheda di ripasso (Fase 7-8) e compare in «Da rivedere» nel riepilogo.

**Riprova facoltativa** su `quiz_multi`, `fill_blank`, `order_steps` dopo `incorrect`/`partial`: Button testuale «Riprova» sotto il feedback, non sostituisce «Continua»; il secondo tentativo non cambia l'esito registrato («Per il ripasso conta il primo»); una sola volta. Mai su `quiz_single`, `flashcard`, `open_answer`. Dettagli in `_exercises.md` §5.

### La barra «Continua» (`ContinueBar`)

Soft-lock, mai un bottone muto. Un solo `ContinueBar` per volta: quello della sezione corrente (le precedenti scompaiono quando si continua). È l'ultimo elemento della sezione, a 24 dp dall'ultimo blocco, larghezza della colonna.

```
Sezione con esercizio non svolto           Esercizio svolto (giusto o sbagliato)
┌──────────────────────────────┐           ┌──────────────────────────────┐
│ (…esercizio…)                │           │ (…esercizio + feedback…)     │
│ (🔒) Rispondi alla domanda   │ ui.body   │                              │
│      per continuare.         │ inkSoft   │ [        Continua          ] │ primario, 56 dp, accent
│ [🔒       Continua         ] │ secondario│                              │ sblocco 200 ms
└──────────────────────────────┘ 56 dp
```

| Stato | Aspetto | Semantics |
|---|---|---|
| **Bloccato** | Button **secondario** 56 dp (`controlHeightLarge`): fill `surface`, bordo `border` 1,5, testo `ink`, icona `lock` 20 dp a sinistra; **sopra**, riga `lock` + `ui.body` `inkSoft`: «Rispondi alla domanda per continuare.» (plurale: «Mancano ancora 2 domande.»). **Non** grigio-disabilitato: il grigio dice "rotto", questo dice "manca una cosa" | `Role.Button`, **non** `disabled()`; `stateDescription` «Bloccato: rispondi alla domanda per continuare» |
| **Tocco mentre bloccato** | Mai muto: scorre (`scrollTo`, 300 ms `out`; istantaneo con «riduci animazioni») alla **prima domanda obbligatoria non svolta** della sezione, sposta lì il focus di accessibilità e aggiorna la riga: «Manca ancora: la domanda qui sopra.» Nessuna vibrazione, nessun colore d'errore | annuncio polite della riga |
| **Sbloccato** | Button **primario** 56 dp, `accent`, testo `onAccent` (7,6 / 7,2), nessuna icona `lock`; la riga di avviso scompare; transizione `unlock` 200 ms (dissolvenza) | annuncio polite «Puoi continuare» |
| **Sezione senza esercizio obbligatorio** | sempre sbloccato, nessuna riga | — |
| **Ultima sezione** | stesso comportamento, etichetta «Completa lezione» | — |

Etichette: «Continua» (sezioni intermedie), «Completa lezione» (ultima). Dopo «Continua» la barra della sezione sparisce e la nuova sezione si rivela (§4.10).

**Font 200 %**: riga di avviso e bottone restano adiacenti (non sticky); il tocco sul bottone bloccato porta alla domanda.

## 6. Tastiera fisica (tablet con tastiera)

Estende `../components/_exercises.md` §6. Obiettivo: poter percorrere una lezione senza toccare lo schermo.

| Contesto | Tasto | Effetto |
|---|---|---|
| Ovunque nella lezione | Tab / Maiusc+Tab | ordine visivo: TopBar → colonna (blocco per blocco) → pannello di contesto |
| | **F6** (e Maiusc+F6) | passa fra regioni: colonna ⇄ pannello (⇄ TopBar) |
| | **Ctrl+Invio** | azione primaria contestuale: «Controlla»/«Invia» dell'esercizio a fuoco; se il focus è fuori da un esercizio e `ContinueBar` è sbloccata: «Continua»/«Completa lezione»; se è bloccata: va alla prima domanda non svolta (come il tocco) |
| | Esc | chiude sheet/popover; non esce dalla lezione |
| | Alt+← o tasto Indietro | esce (come ✕, stato salvato) |
| | Pag↑ / Pag↓, Spazio (focus sulla colonna), Home / Fine | scorrimento della colonna (quando il focus non è su un controllo che li usa) |
| Quiz | ↑↓, 1-6, Spazio, Invio | `_exercises.md` §6, `quiz_single.md`, `quiz_multi.md` |
| Completa | Invio / Tab | campo successivo; ultimo → «Controlla» |
| Ordinamento | Spazio, ↑↓, Alt+↑/↓, Esc | afferra, sposta, annulla (`order_steps.md`) |
| Scheda | Spazio mostra; 1-4 dà il voto | `flashcard.md` |
| Indice (pannello) | ↑↓ fra le righe, Invio apre/scorre, Spazio alterna disclosure | focus ring 3 dp |

L'anello di focus 3 dp (offset 2) appare solo con tastiera/D-pad. Suggerimenti visivi delle scorciatoie (`KeyHint`) solo con tastiera fisica collegata. Nessuna scorciatoia a lettera singola (si digita in campi di testo; si evitano conflitti con IME e TalkBack).

## 7. Riepilogo (dopo «Completa lezione»; Fase 7)

Schermata a tutta finestra, **senza pannello**, colonna centrata 600 dp (compact: margine 16). Ha **due parti con tempi diversi**: la parte **locale**, istantanea (calcolata dagli esiti dei tentativi), e il blocco **«Cosa hai dimostrato»**, che arriva dal backend a job `lesson_completion` finito (di solito pochi secondi, può essere di più). La prima non aspetta la seconda (P1, P5: niente attesa davanti all'unica vittoria che conta), e la seconda cresce **in fondo**, così non sposta ciò che si sta leggendo.

Si raggiunge in due modi: (a) **dopo «Completa lezione»**; (b) toccando una lezione `completed` nella lista o «Lezione N» in un record: stessa schermata, ricostruita da `LessonView` (`attempts`, `completion`).

```
┌──────────────────────────────────────┐
│ ✕                                    │  TopBar (✕ «Chiudi», torna all'argomento)
│ Lezione completata                   │  ui.title1
│ (target) Obiettivo                   │  ui.label inkSoft
│ Saper prevedere se una variabile è   │  reading.body: `objective`
│ ancora utilizzabile dopo l'assegnamento.
│                                      │
│ Cosa hai fatto                       │  ui.title3   (parte locale, invariata dalla Fase 5)
│  ✓ Quando `a` smette di essere       │
│    valida · Corretto al primo tentativo
│  ◦ Copia e clone · Da rivedere       │
│                                      │
│ Da rivedere                          │  solo se esistono esiti incorretti/parziali al 1° tentativo
│  ◦ «Dopo la riga 2, si può ancora    │
│    usare a?»  [Rivedi]               │
│                                      │
│ ┌ Cosa hai dimostrato ─────────────┐ │  blocco asincrono (§7.2), Surface static
│ │ ◌ Registro cosa hai dimostrato…  │ │
│ └──────────────────────────────────┘ │
│ [   Torna all'argomento   ]          │  Button primario 56 dp (SEMPRE attivo)
│ [ Chiedi al Tutor ]  [Rileggi la lezione]│ secondari
└──────────────────────────────────────┘
```

Linguaggio di competenza («Hai dimostrato…», «Corretto al primo tentativo»), mai «Bravo!», mai punteggi, percentuali, serie. «Da rivedere» è informazione, non colpa. Una sola vittoria in evidenza: l'`objective`.

### 7.1 Parte locale: «Cosa hai fatto» e «Da rivedere»

Invariate rispetto alla Fase 5 (icona + parole: `check-circle` «Corretto al primo tentativo» / «Corretto al secondo tentativo»; `minus-circle` «Da rivedere»), con l'aggiunta dei casi di `open_answer` (§8):

| Esito di `open_answer` | Riga in «Cosa hai fatto» |
|---|---|
| `correct` | `check-circle` «Corretto» |
| `partial` | `minus-circle` «Da rivedere» (e compare anche in «Da rivedere» con «Rivedi») |
| `incorrect` | `minus-circle` «Da rivedere» |
| `pending` / in coda locale | `clock` `inkSoft` «Risposta registrata · la valuto a breve» (`info`, mai colore di allarme). Si aggiorna da sola quando il job la valuta (step `grading`) |

### 7.2 Blocco «Cosa hai dimostrato» (stato del job `lesson_completion`)

`Surface static` (bordo `line` 1 dp, raggio `md`, padding 20 / 16 compact), titolo `ui.title3` «Cosa hai dimostrato» sempre visibile. Il corpo dipende dallo stato (derivato da `LessonView.completion.job`, da `job_updated` e, senza SSE, da un'interrogazione ogni 3 s finché il job è attivo; mai più di una per volta).

**Prima di tutto: invio delle risposte.** «Completa lezione» invia prima la coda locale dei tentativi (`lessons.md` §6: dopo la chiusura sarebbero rifiutati). Il bottone passa a `loading` «Invio le tue risposte…» (`busy`, ≤ qualche secondo). Esiti:

| Esito dell'invio | Cosa si vede |
|---|---|
| Tutto inviato → `POST …/complete` `202` | si apre il Riepilogo (parte locale subito) e il blocco in stato **In corso** |
| Rete assente / backend irraggiungibile | si apre comunque il Riepilogo (la parte locale è vera e non dipende dalla rete); il blocco mostra `StatusPanel info` «Lezione completata, risposte da inviare» corpo «Appena torna la connessione invio le tue risposte e registro cosa hai dimostrato. Non devi fare altro.» + Button testuale «Riprova ora»; l'app riprova da sola alla ripresa della rete e in primo piano. «Torna all'argomento» resta attivo. Nel frattempo la lezione è, **per il backend, ancora `in_progress`** |
| `409 llm_key_missing` / `502 llm_key_rejected` su `complete` (nessun job creato, la lezione non è chiusa) | **non** si apre il Riepilogo: si resta nella lezione e al posto del bottone in errore compare `LlmKeyNotice` (`../llm-key.md` §5); inserita la chiave, «Completa lezione» si ripreme |
| `502 llm_credit_exhausted` su `complete` | come sopra, ma il pannello è `credit` con «Riprova» (ripete `complete`); nessun rimando alle Impostazioni (`../llm-key.md` §9.2) |
| `409 conflict` su `complete` (già chiusa, per esempio da un altro dispositivo) | si tratta come riuscito: si carica `LessonView` e si mostra lo stato reale |
| `409` su `attempts` (lezione chiusa altrove) | le risposte locali ancora in coda si scartano; il blocco mostra lo stato reale; in cima alla colonna `StatusPanel info` «Questa lezione è stata completata da un altro dispositivo. Le risposte nuove non si possono più registrare.» |

**Stato A: in corso** (`queued` / `running`)

```
┌──────────────────────────────────────────────┐
│ Cosa hai dimostrato                          │
│ ◌  Registro cosa hai dimostrato…             │  spinner 20 accent + ui.body ink
│    Puoi uscire: lo faccio comunque.          │  ui.bodyS inkSoft
└──────────────────────────────────────────────┘
```

- Una sola riga con il testo dello **step corrente** (non una lista: è un passaggio secondario, e tre fasi da qualche secondo non meritano una lista): `grading` «Valuto le tue risposte aperte…» · `recording` «Registro cosa hai dimostrato…» · `saving` «Salvo il registro…» · `queued` e step sconosciuto «Registro cosa hai dimostrato…». Nessuna percentuale, nessuna stima. `progress.message` (se presente) in `ui.bodyS` `inkSoft` sotto.
- Dopo 60 s: la frase diventa «Sta richiedendo più del solito. Puoi uscire: il registro si aggiorna comunque.»
- Live region Polite persistente: annuncia solo i cambi di step e l'esito. Il focus non si sposta.

**Stato B: riuscito, con elementi** (`succeeded`, `records` o `terms` non vuoti)

Il pannello si sostituisce (crossfade `fast`; con «riduci animazioni» istantaneo) con, nell'ordine, solo i gruppi non vuoti:

1. **«Hai dimostrato»** (`ui.label` `inkSoft`, solo se `records`): una `RecordCard` per record (stessa di `progress.md` §3.2: eyebrow «Hai dimostrato» / «Idea chiarita», titolo, corpo, «Come l'hai dimostrato» chiusa; senza il Button «Lezione N», qui ridondante), fino a 5. Le card sono **dentro** il pannello senza una seconda cornice (si usa la separazione con hairline `line`, non card annidate).
2. **«Parole nuove nel glossario»** (solo se `terms`): riga a capo libero di `TermPill` (Button 48 dp, fill `surface`, bordo `border` 1,5, raggio `full`, testo `ui.label` `ink`, icona `book-open` 16 `knowledge`; gap 8): tocco → `GlossarySheet` con `origin` (`progress.md` §4.2). Un termine «rivisto» è identico a uno nuovo (non si fa la differenza in v1).
3. **Schede di ripasso** (solo se `review_card_count` > 0): riga `ui.body` `inkSoft` con icona `layers` 20 `inkSoft`: «Ho preparato 4 schede per il ripasso.» (1: «Ho preparato 1 scheda per il ripasso.»). Sotto, `ui.bodyS` `inkSoft`: «Le ritroverai nel ripasso tra circa 1 giorno.» (1 scheda: «La ritroverai nel ripasso tra circa 1 giorno.»). Il «circa 1 giorno» è fisso: una scheda nuova è in scadenza 24 h dopo la creazione (`reviews.md` §1). **Nessun Button «Vai al ripasso»** (decisione Fase 8): appena chiusa la lezione le sue schede non sono in scadenza, e il pulsante porterebbe a schede di altre lezioni o a «Nessun ripasso per ora», interrompendo il momento di chiusura (P1). Il ripasso si trova in Home. Testo identico in compact, medium ed expanded.
4. Button testuale «Vedi tutto nel registro» (48 dp) → argomento, tab «Cosa sai» › Registro.

Annuncio Polite una sola volta: «Registro aggiornato: 2 cose registrate, 3 parole nuove.» (testi con plurali).

**Stato C: riuscito, nessun record e nessun termine** (`record_count` = 0 e `term_count` = 0): **esito legittimo, non un errore**. Nessun pannello colorato (un `StatusPanel info` suonerebbe come un avviso): testo semplice nel blocco:

- `ui.body` `ink`: «Questa volta non ho aggiunto niente al registro.»
- `ui.body` `inkSoft`: «Ci scrivo solo ciò che dimostri negli esercizi, e qui non c'era ancora abbastanza: succede, soprattutto con un argomento nuovo.»
- Button secondario «Chiedi al Tutor» (apre la chat con il testo «Ho completato la lezione «{titolo}» ma non sono sicuro di aver capito. Da dove conviene ripartire?»).

Perché questo testo: dice *cosa fa il registro* (regola oggettiva) e *perché è vuoto* (nessuna colpa, nessun «non hai superato», nessun «hai sbagliato tutto»), normalizza (Dweck: la difficoltà è parte del processo), e offre un'uscita (il Tutor) invece di un vicolo cieco. Mai: «Nessun progresso», «Non hai dimostrato nulla», «Riprova la lezione» (la lezione chiusa non si rifà).

**Stato D: fallito** (`failed`)

`StatusPanel danger` (icona `alert-circle`) dentro il blocco, schema di `resources.md` §4.4: titolo, corpo, `Button` secondario «Riprova» (`POST /topics/{id}/jobs/{job_id}/retry`, poi stato A), `Disclosure` «Dettagli» con `error.message`.

| `error.code` | Titolo | Corpo |
|---|---|---|
| `invalid_output` | Non sono riuscito a registrare la lezione | La lezione è completata e le tue risposte sono al sicuro. Il registro non si è aggiornato: riprova, spesso basta. |
| `upstream_failed` | Registro non aggiornato | Il servizio non ha risposto. La lezione è completata e le tue risposte sono al sicuro: puoi riprovare. |
| `rate_limited` | Troppe richieste | Il servizio è occupato. Aspetta un minuto e riprova. |
| `timeout` | Il registro ha richiesto troppo tempo | Non sono riuscito a finire in tempo. Puoi riprovare: spesso basta. |
| `interrupted` | Registro non aggiornato | Il server si è riavviato più volte. Riprova. |
| `internal` / sconosciuto | Registro non aggiornato | Qualcosa è andato storto sul server. Riprova. |
| `llm_key_missing` / `llm_key_rejected` (Fase 11) | non è questa tabella: `LlmKeyNotice` (`../llm-key.md` §6) | «Riprova» solo dopo il salvataggio di una chiave. `llm_credit_exhausted`: pannello `credit` con «Riprova» subito (`../llm-key.md` §9.3) |

Mentre il job è `failed`, **tutto il resto del riepilogo resta valido** (la parte locale non dipende dal registro). Il Toast in uscita e la riga nella lista lezioni lo segnalano (`topic.md` F4). Errore di rete su «Riprova»: stesso pannello, corpo «Non sono riuscito a raggiungere il backend. Controlla la connessione.».

### 7.3 Azioni in fondo

| Azione | Tipo | Comportamento |
|---|---|---|
| **Torna all'argomento** | primario 56 dp, **sempre attivo** (anche con job in corso o fallito: nulla va perso) | chiude il Riepilogo e apre la tab **Lezioni** dell'argomento |
| **Chiedi al Tutor** | secondario | chat con il testo «Ho appena finito la lezione «{titolo}». Puoi aiutarmi a ripassare i punti che non mi sono chiari?» e `lesson_id` (`../components/ask_agent.md`) |
| **Rileggi la lezione** | secondario, solo se il Riepilogo è aperto da una lezione `completed` (o dopo la chiusura) | apre la lezione in sola lettura: tutte le sezioni rivelate, esercizi bloccati con il loro feedback, nessuna «Continua»; sottotitolo TopBar «Completata»; esercizio senza tentativo: corpo disabilitato + riga `ui.body` `inkSoft` «Questa lezione è chiusa: non si possono più inviare risposte.» |

Su expanded: i tre bottoni in colonna a larghezza piena colonna (primario sopra); mai in riga (l'accidentale è peggio dell'ordine verticale).

### 7.4 Tastiera, accessibilità

- Ordine: TopBar → titolo → obiettivo → «Cosa hai fatto» → «Da rivedere» → blocco «Cosa hai dimostrato» → azioni. Il blocco è un gruppo con `paneTitle` «Cosa hai dimostrato».
- Il passaggio da stato A a B/C/D: live region Polite, **senza spostare il focus**.
- Tab percorre gli elementi; Ctrl+Invio = «Torna all'argomento».
- Contrasti: nessun colore nuovo (`ink`/`surface`, `inkSoft`/`surface` 9,35 / 10,08, `knowledge`/`surface` 6,39 / 10,08).
- Font 200 %: i `TermPill` vanno a capo uno per riga; i bottoni restano in colonna.

## 8. Stati della schermata

| Stato | Cosa si vede |
|---|---|
| Caricamento | Skeleton: tre righe di testo e un blocco sotto la TopBar (pannello assente); dopo 8 s «Sto ancora caricando…» |
| Errore di caricamento | `StatusPanel danger` con `cloud-off`, «Non riesco a caricare la lezione.», «Riprova»; ✕ sempre disponibile |
| Documento non utilizzabile (JSON non valido, `blocks` assente, nessun blocco renderizzabile) | Pagina semplice: «Questa lezione non si può aprire.» + «Torna all'argomento» + «Chiedi al Tutor» (nessun dettaglio tecnico) |
| `catalog_version` superiore a quella supportata | **Non bloccante**: si rende ciò che si conosce; in testa alla colonna `StatusPanel info`: «Questa lezione usa componenti più recenti dell'app: alcune parti potrebbero mancare. Aggiorna l'app per vederle.» |
| Lezione senza sezioni h2 | una sezione; nessun «Sezione k di N»; «Completa lezione» in fondo |
| Tutti i blocchi di una sezione non renderizzabili | riga neutra unica; la sezione si sblocca da sola |
| Lezione `completed` riaperta | §7.3 «Rileggi la lezione» |

### 8.1 Invio dei tentativi (Fase 7)

Principio: **registrare non è mai un passo che l'utente aspetta né un motivo per bloccarlo** (un guasto non è colpa sua). Ogni esercizio che arriva a `checked` genera un tentativo che entra subito nella **coda locale** (persistita su disco, sopravvive a riavvio); l'app lo invia in background. «Continua» **non dipende mai** dall'invio.

| Aspetto | Regola |
|---|---|
| **Quando si invia** | alla comparsa in coda (batch fino a 50, al massimo un `open_answer` per richiesta: se ce ne sono due, due richieste in sequenza), poi a ogni ripresa in primo piano, al ritorno della rete, all'apertura della lezione e prima di «Completa lezione» |
| **Chiusi (quiz, fill, order) e flashcard** | l'invio è silenzioso: nessun pannello, nessun indicatore per lezione. Il tentativo si considera registrato alla risposta `200`; il backend conserva il **primo** inviato (idempotenza, `lessons.md` §5), quindi il rinvio non duplica |
| **Indicatore di coda** | `cloud-off` 24 dp nella TopBar, visibile **solo** se la coda è non vuota da più di 5 s (evita sfarfallio con la rete buona). Tocco → piccolo `Popover` (fill `surfaceRaised`, ombra `raised`, raggio `md`, padding 16): «Alcune risposte non sono ancora inviate. Le invio appena torna la connessione.» + Button testuale «Riprova ora» (48 dp). Scompare da solo a coda vuota; annuncio Polite «Risposte inviate.» |
| **`open_answer`** | è l'unico esercizio la cui risposta **attende** il backend (valutazione sincrona, fino a 20 s): il pannello mostra «Sto valutando la tua risposta…» (`open_answer.md`). Il client rinuncia dopo **25 s** (poco più del limite del backend) e passa a «Risposta registrata» senza perdere nulla; la richiesta può comunque avere avuto successo sul server, e il rinvio è idempotente |
| **`pending`** | (risposta `200` con `outcome: pending` o rinuncia del client) la risposta conta come svolta e **non blocca** «Continua»; pannello `info` «Risposta registrata» (`open_answer.md`); riprovo automatico **una volta** alla prossima ripresa in primo piano o al ritorno della rete (non a tappeto: ogni valutazione è una chiamata a pagamento) e a richiesta con «Valuta ora»; al peggio la valuta il job di chiusura (step `grading`) |
| **Errore `422`** per un tentativo | bug dell'app o documento cambiato: il tentativo si scarta dalla coda dopo un solo tentativo e si registra nel log, senza mostrare nulla (l'esercizio resta svolto localmente). Un `open_answer` scartato mostra «Non sono riuscito a valutare questa risposta. L'ho salvata.» |
| **`409`** (lezione `completed`) | la coda di quella lezione si svuota; vedi §7.2 per il messaggio |
| **`404`** (lezione eliminata) | coda svuotata; se la lezione è aperta: «Questa lezione non esiste più.» + «Torna all'argomento» |
| **Ripristino** | all'apertura di una lezione si carica `LessonView.attempts`: gli esiti del backend vincono su quelli locali già inviati; i tentativi locali non ancora inviati restano e si rinviano. Un `open_answer` valutato mostra il suo feedback (`open_answer.md`) |
| **Offline dall'inizio** | la lezione già scaricata si legge e svolge; «Completa lezione» → §7.2 «rete assente» |
| **Secondo tentativo** (quiz_multi, fill, order) | entra in coda con `attempt_number: 2`; il backend lo accetta solo se il primo esiste e non è `correct` (l'app lo garantisce già), altrimenti `422`, che si scarta in silenzio |

## 9. Accessibilità

Evidenziatore con la penna (tavolozza flottante solo dopo il primo uso della penna, rimozione, sincronizzazione silenziosa, indicatore `cloud-off` condiviso con §8.1): `../components/highlight.md`. Si evidenzia anche a lezione completata (§7.3 «Rileggi la lezione»). Scrittura a mano sopra la lezione (strumento «penna» nella stessa tavolozza, area di scrittura, gomma, Annulla/Ripeti): `../components/handwriting.md`; l'area di scrittura usa i margini della colonna di §3.

- Una sola colonna di lettura; l'ordine di lettura è l'ordine visivo; il pannello viene **dopo** la colonna (lo si raggiunge con F6 o dopo l'ultimo blocco).
- Intestazioni (`heading()`) per h1/h2/h3 → navigazione per intestazioni di TalkBack; nuova sezione → focus all'h2 e annuncio polite «Sezione k di N: <titolo>».
- `ContinueBar` e esercizi: `stateDescription`, live region, ordini di focus descritti nei file dei blocchi. Il motivo del blocco è sempre accanto al bottone nell'albero.
- Feedback degli esercizi: live region polite «Corretto. <spiegazione>».
- Font fino a 200 %: la colonna riflette, le opzioni crescono in altezza, il sottotitolo e il titolo vanno a capo, i gruppi a più colonne passano a una colonna oltre fontScale 1,3, `fill_blank` passa alla modalità a elenco oltre 1,5.
- Contrasto: tutte le coppie sono già in `design-system.md` §2.4 / `tools/contrast_check.py` (nessun colore nuovo in Fase 5).
- Target ≥ 48 dp; gap ≥ 8 dp. Eccezione motivata: span inline di termini/citazioni nel testo (WCAG 2.5.8).
- Codice: un nodo con testo integrale, azioni «Copia codice» e scorrimento; selezionabile.
- Il testo informativo non scende sotto 12 sp e mai in `labelSmall` salvo etichette brevi (`ui.labelS` per chip e numeri di citazione).
- «Riduci animazioni»: tutte le durate a 0 (`design-system.md` §8), nessuno scroll animato.
- Un'animazione non è mai l'unico segnale di un cambio di stato.

## 10. Proposte ai contratti (per il tech lead; non applicate)

Vedi il report. In sintesi: `code.highlight_lines`, avvisi non bloccanti del Validator (`warnings`) per la qualità didattica (gap esercizio, markup nelle opzioni, risposte lunghe nei vuoti), precisazioni di §5 (mescolamento, feedback della corretta, `incorrect` del multi), glossario senza `[[ ]]` nelle definizioni, `catalog_version` superiore, esercizi in `fallback`.


### 3.3 Fonti «Dal tuo PDF» (Fase 10)

Una voce di `sources[]` con **`url: null`** è un PDF caricato dall'utente (`documents.md` §6; il Validator garantisce `url` null ⇔ documento). L'app non guarda `resource_id` né interpreta il `title`: le pagine sono già nel testo del titolo («Rust in Action, cap. 4 (pp. 101-118)»). Per l'utente è la fonte più concreta: è **il suo** materiale, e può verificare la lezione aprendo il file alle pagine indicate (contiguità e controllo, non fede).

**Citazione nel testo `[^key]`**: identica alle altre, «[1]» `ui.labelS` `knowledge` (stesso stile: due aspetti diverserebbero senza ragione e un indizio non serve). Tocco → sheet.

**Riga nel pannello Fonti** (expanded) e nell'elenco compact:

```
[1]  Rust in Action, cap. 4 (pp. 101-118)      titolo ui.body ink, a capo, mai ellissi
     📄 Dal tuo PDF                            ui.bodyS inkSoft, icona file-text 16 al posto del dominio
```

Nessun trailing `external-link`: non esce dall'app. Se esistono fonti web e PDF, **prima i PDF** (è il materiale che l'utente sta seguendo), poi le altre, ciascun gruppo nell'ordine di `sources[]`; il numero «[n]» resta quello del testo.

**Sheet «Fonte» per un PDF** (stesso `Sheet` della fonte web, altezza al contenuto):
- icona `file-text` 24 `inkSoft` + `ui.label` `ink` «Dal tuo PDF»;
- titolo `ui.title3` `ink` (con le pagine);
- corpo `ui.body` `ink`: «Per ricontrollare questa parte, apri il tuo file alle pagine indicate. Io non conservo il PDF.»
- una sola azione: **«Chiudi»** (nessun «Apri»: non c'è URL e l'app non riapre il file nella v1). Focus iniziale sul titolo.
- Se il documento è stato rimosso dal Topic nel frattempo: lo sheet è identico (si basa solo sulla voce `sources`); le lezioni già fatte non cambiano.

**`primary_source` con `url: null`**: vedi `../components/primary_source.md` (card non tappabile).

**Intestazione della lezione e riepilogo**: se la lezione ha almeno una fonte con `url: null`, sotto il titolo e «Circa 12 minuti» compare una riga `ui.bodyS` `inkSoft` con icona `file-text` 16: «Dal tuo PDF: Rust in Action, cap. 4 (pp. 101-118)» (titolo della voce; più voci: la prima e «+1»; la riga è testo, non tappabile; a capo libero). Serve a rendere **evidente che la lezione segue il documento** già prima di leggere; è derivata dal documento di lezione, nessun campo nuovo. Nel Riepilogo (§7) la stessa riga sotto «Per approfondire».

Accessibilità: la citazione è letta «Fonte 1: <titolo>, dal tuo PDF» (il titolo com'è, pagine incluse: non si interpreta). La riga del pannello è tappabile (apre lo sheet): nodo unico «Fonte 1: <titolo>. Dal tuo PDF», `Role.Button`, `onClickLabel` «Mostra la fonte». Contrasti: coppie esistenti (`knowledge`/`surface` 6,4 / 10,1; `inkSoft`/`surface` 9,35 / 10,08).
