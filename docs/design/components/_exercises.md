# Esercizi: regole comuni

> Valgono per `quiz_single`, `quiz_multi`, `fill_blank`, `order_steps`, `flashcard`, `open_answer` (e, dove indicato, `checklist`). Ogni file di blocco descrive solo ciò che è specifico. Principi: retrieval practice (P5), feedback spiegato (P6), nessun indizio (P7), non solo colore (P8), nessuna vergogna (P9).

## 1. Cornice: `ExerciseCard`

Gli esercizi si distinguono dalla spiegazione (segnalazione, Mayer: l'utente deve capire a colpo d'occhio "qui tocca a me"), ma senza decorazione.

```
┌──────────────────────────────────────────────┐  Surface static: fill surface, bordo line 1 dp,
│ (✎) Scelta singola                  Facoltativo│  raggio md, padding 20 (16 compact)
│                                              │
│ Dopo la riga 2, si può ancora usare `a`?     │  domanda: reading.question (Literata 500 18/28)
│ Scegli una risposta.                         │  istruzione: ui.body, inkSoft
│                                              │
│  (corpo specifico del blocco)                │
│                                              │
│ [ Controlla ]  Scegli una risposta per…      │  azione + motivo
│                                              │
│ ┌ Corretto ─────────────────────────────┐    │  feedback (StatusPanel), a 12 dp
│ └───────────────────────────────────────┘    │
└──────────────────────────────────────────────┘
```

- **Intestazione** (altezza 24 dp + 12 dp sotto): icona 20 dp `inkSoft` + nome del tipo in `ui.label` `inkSoft`: «Scelta singola» (`circle-dot`), «Scelta multipla» (`check-square`), «Completa» (`text-cursor-input`), «Metti in ordine» (`list-ordered`), «Scheda» (`layers`), «Risposta aperta» (`pen-line`). A destra, solo se `required == false`, `StatusChip` neutro «Facoltativo» (fill `surfaceSunken`, testo `inkSoft`). Il tipo è ridondante con la forma ma è *dual coding* e aiuta TalkBack ("Esercizio: scelta singola"). Nessuna numerazione ("Domanda 2 di 5"): innesca il confronto col punteggio (P9).
- **Domanda**: `reading.question`, `ink`, `md` inline completo (termini del glossario **tappabili** nella domanda; vedi `paragraph.md`). **Istruzione**: una riga `ui.body` `inkSoft` sotto la domanda (8 dp), fissa per tipo.
- **Corpo**: 16 dp sotto l'istruzione.
- La card **non** cambia colore, bordo o ombra con l'esito (l'esito vive nelle opzioni/campi e nel pannello): così l'aspetto prima della risposta non contiene indizi e dopo non "punisce".

## 2. Stati

| Stato | Quando | Aspetto/comportamento |
|---|---|---|
| **non svolto** (`idle`) | arrivo nella sezione | nessun input, azione primaria disabilitata con motivo |
| **in compilazione** (`answering`) | c'è almeno un input | azione primaria attiva; l'input si può cambiare liberamente |
| **selezionato** | (quiz) opzione scelta | `ChoiceRow` selezionata: fill `accentSoft`, bordo `accent` 2 dp, indicatore pieno (`base-components` §9). Mai colori di esito |
| **controllato** (`checked`) con esito `correct` / `partial` / `incorrect` | dopo l'azione | input bloccati (`stateDescription`, non `disabled()`), marcature con icona+parola, `StatusPanel` di esito |
| **riprova** (`retrying`) | dopo "Riprova" (solo `quiz_multi`, `fill_blank`, `order_steps`) | input riaperti e ripuliti dalle marcature, pannello nascosto, caption «Secondo tentativo» in `ui.bodyS` `inkSoft`; l'esito registrato resta quello del **primo** tentativo |
| **bloccato** (`locked`) | esercizio di una sezione già superata, o ripristinato dopo riavvio | identico a `checked` (stesso feedback visibile), nessuna azione di riprova, nessuna azione di controllo |
| **non renderizzabile** | campi mancanti | riga neutra ([missing_component.md](missing_component.md)); non obbligatorio |
| **in invio / valutazione** | solo `open_answer` | vedi [open_answer.md](open_answer.md) |

Transizioni: `idle → answering → checked → (retrying → checked)`. Il tentativo è **registrato al primo `checked`**: id del blocco, esito, scelte, timestamp, numero di tentativi (contratto §5; invio al backend con coda locale). "Svolto" = ha raggiunto `checked` almeno una volta (per `flashcard`: autovalutazione data; per `open_answer`: invio accettato localmente). Lo stato (`answers`, `checked`, bozza) è salvato per `block.id` e ripristinato.

## 3. Azione di controllo

- Bottone **primario** (48 dp, `controlHeight`, a larghezza intrinseca, min 160 dp, allineato a sinistra con il testo), etichetta «Controlla» (`fill_blank`/`quiz_*`/`order_steps`), «Invia» (`open_answer`). Una scheda (`flashcard`) usa «Mostra risposta» e poi i voti.
- **Disabilitato finché manca l'input**, con il **motivo** accanto/sotto in `ui.body` `inkSoft` («Scegli una risposta per controllare.»; `fill_blank`: «Compila tutti gli spazi per controllare.» o «Scrivi qualcosa in ogni spazio»; `order_steps`: nessun disabilitato, vedi file). Mai bottone muto (design-system §10).
- Dopo l'azione il bottone **scompare** (non resta un bottone disabilitato "Controllato"): lo spazio è del pannello di esito. Il focus di accessibilità passa al pannello (§4).
- Nessun timer, nessun limite di tentativi nella UI.

## 4. Feedback (`StatusPanel`)

`StatusPanel` (`base-components` §5) sotto il corpo, a 12 dp (`sm`), apparizione `feedbackAppear` (200 ms; con "riduci animazioni" istantanea). Varianti e parole fisse:

| Esito | Variante | Etichetta | Quando |
|---|---|---|---|
| giusto | `correct` | «Corretto» | tutto giusto |
| parziale | `partial` | «Quasi» | `quiz_multi` intersecato; `fill_blank` con alcuni blank giusti |
| errato | `incorrect` | «Non proprio» | resto |
| solo registrato | `info` | «Risposta registrata» | `open_answer` in coda locale |

Il **corpo** è il `feedback` del contratto (md inline, termini tappabili, `ui.bodyL` per regola del componente). Le funzioni del feedback:
1. Dice **perché** (spiegazione), non solo l'esito (Hattie e Timperley).
2. Dopo un errore **collega** l'errore a una misconcezione plausibile e dice cosa fare diversamente (il Generator lo scrive così, vedi README).
3. Non giudica la persona: mai «Sbagliato!», «Peccato», «Ops», punti esclamativi (design-system §10).

**Ordine di lettura**: etichetta → corpo. Il pannello ha `liveRegion = Polite` e riceve il focus di accessibilità; la sua `contentDescription` è «<etichetta>. <corpo>» (testo semplice, senza markup, con i `[[termine|testo]]` ridotti a "testo").

**Scroll**: dopo "Controlla" il pannello (e la marcatura) deve essere interamente visibile: `bringIntoView` con `sectionReveal`-like 300 ms `out` (istantaneo con "riduci animazioni"). Se non entra nello schermo insieme alla domanda, vince il pannello (la domanda si è appena letta).

### Marcature per opzione/campo/passo (non solo colore)

Dopo `checked`, ogni elemento rilevante porta **icona + parola** + bordo/riempimento colore (il colore è terzo canale):

| Marcatura | Icona (20 dp) | Parola (`ui.label`) | Colore bordo/fill |
|---|---|---|---|
| scelta corretta | `check-circle` | «La tua scelta · corretta» | bordo `correct` 2 dp, fill `correctSoft` |
| scelta non corretta | `x-circle` | «La tua scelta · non corretta» | bordo `incorrect` 2 dp, fill `incorrectSoft` |
| corretta non scelta (rivelata) | `check-circle` | «Risposta corretta» (quiz_multi: «Era corretta») | bordo `correct` 1,5 dp, fill `correctSoft` |
| non scelta e non corretta | — | — | invariata (`surface`, bordo `border` 1,5), con disclosure «Perché no?» |

Testo del `label`: `ink`? No: l'icona e la parola sono nel colore `x` (≥ 4.5 su `xSoft`, tabella contrasti), il testo dell'opzione resta `ink` (≥ 7).

## 5. Riprova (facoltativa)

- Offerta su `quiz_multi`, `fill_blank`, `order_steps` dopo `incorrect`/`partial`. **Non** su `quiz_single` (3-5 opzioni: sarebbe tentativo a caso), `flashcard`, `open_answer`.
- Button **testuale** «Riprova» (icona `rotate-ccw`) sotto il pannello, a 12 dp; **non** sostituisce «Continua» (che nel frattempo è già sbloccato).
- Riprova: ripulisce le marcature e il pannello; l'input resta (fill_blank: i testi digitati restano; order_steps: l'ordine tentato resta; quiz_multi: la selezione è azzerata per non ridurre il tentativo a un cambio di una voce). Al secondo «Controlla»: stesso `StatusPanel` ma con riga aggiuntiva in `ui.bodyS` `inkSoft`: «Secondo tentativo. Per il ripasso conta il primo.»
- Non si offre una terza volta: dopo il secondo `checked` nessuna azione, il feedback resta (si impara dal feedback, non dal tentativo cieco).

## 6. Tastiera fisica (tablet con tastiera)

Un solo modello per tutti gli esercizi; i dettagli per tipo nei file. Rilevamento: `Configuration.keyboard != KEYBOARD_NOKEYS` e `hardKeyboardHidden == HARDKEYBOARDHIDDEN_NO`. Ordine di tabulazione = ordine visivo; **ogni elemento interattivo è raggiungibile con Tab** e mostra l'anello `focus` (3 dp, offset 2 dp).

| Tasto | Effetto |
|---|---|
| Tab / Maiusc+Tab | prossimo/precedente controllo (le opzioni di un quiz sono **un solo gruppo**: Tab entra una volta, le frecce si muovono dentro) |
| Frecce ↑ ↓ | dentro un gruppo di opzioni: sposta il focus (`quiz_single`: sposta **e seleziona**, come un radio group nativo; `quiz_multi`: sposta soltanto) |
| Spazio | seleziona/deseleziona l'elemento a fuoco (opzione, voce di checklist, passo di ordinamento "afferrato") |
| 1-6 | seleziona (quiz_single) / alterna (quiz_multi) l'opzione N. **Mostrata come suggerimento** `KeyHint` solo con tastiera fisica (vedi sotto) |
| Invio | su un bottone: attivalo. In `quiz_single` con un'opzione già selezionata e il focus nel gruppo: equivale a «Controlla». In un campo di testo di `fill_blank`: passa al vuoto successivo; sull'ultimo vuoto equivale a «Controlla» se tutti compilati |
| Ctrl+Invio | azione primaria del blocco a fuoco: «Controlla» / «Invia» (anche dentro un campo di testo, inclusa `open_answer` dove Invio va a capo). Se il focus è fuori da un esercizio e la sezione è sbloccata: «Continua» (`lesson.md`) |
| Esc | chiude sheet/popover; in un campo di testo non fa nulla |

**`KeyHint`**: piccolo riquadro 24 × 24 dp (raggio `sm`, bordo `line` 1 dp, fill `surface`, testo `ui.labelS` `inkMuted`, contrasto ≥ 4.5 su `surface`) con la cifra, a destra della riga dell'opzione. Presente **su tutte** le opzioni (stessa forma, nessun indizio: P7), nascosto senza tastiera fisica, `contentDescription = null` (la scorciatoia non è annunciata da TalkBack: sarebbe rumore). Non aumenta l'altezza della riga.

## 7. TalkBack

- Ordine = ordine visivo: intestazione → domanda → istruzione → corpo → azione/pannello. L'intestazione si annuncia come «Esercizio: scelta singola».
- Ogni gruppo di opzioni: `selectableGroup`; ogni opzione `Role.RadioButton`/`Role.Checkbox`, `selected`; la `ChoiceRow` si legge come un solo nodo («<testo>, non selezionato, 1 di 4»; `collectionItemInfo`).
- Dopo `checked`: ogni opzione rilevante aggiunge allo stato la marcatura («La tua scelta, non corretta») **e** il suo feedback, in un solo nodo; il pannello ha focus e live region polite.
- Il motivo del bottone disabilitato è raggiungibile subito dopo il bottone nell'ordine di lettura; il bottone usa `stateDescription` = motivo.
- Nessun annuncio all'apparire dell'esercizio. Nessun suono.

## 8. Contenuti al limite

| Caso | Regola |
|---|---|
| Domanda 400 caratteri | si disegna intera, senza ellissi; la card cresce |
| Opzione/passo/voce 200 caratteri | va a capo, l'altezza della riga cresce; le altre opzioni **restano alla loro altezza** (nessuna uniformità forzata: l'altezza grande sarebbe un cue sulle altre; la riga alta è solo testo lungo, l'indicatore resta allineato in alto) |
| 6 opzioni (quiz_multi) | stesse regole; gap fra opzioni 8 dp. A 1280 × 800 dp la card può superare il viewport: si scorre, "Controlla" è in fondo alle opzioni |
| Font 200 % | l'etichetta del tipo e «Facoltativo» vanno su due righe; le righe delle opzioni crescono; i gruppi a più colonne (voti flashcard) passano a una colonna oltre fontScale 1,3 |
| Feedback 400 caratteri | si disegna intero; mai scorrimento interno, mai troncamento |
| Nessuna opzione renderizzabile (campi vuoti) | blocco non renderizzabile (non obbligatorio) |
| Documento ripristinato con `correct` che non corrisponde più alle opzioni | si tratta come non renderizzabile |

## 9. Cosa il Generator deve evitare (comune)

- Esercizi che si rispondono dalla sola lettura dell'opzione/testo (riconoscimento banale); risposte contenute nella domanda; domanda e feedback di esercizi precedenti che rivelano la risposta di uno successivo.
- Feedback vuoto di contenuto («Esatto!», «Sbagliato.»), feedback identici per più opzioni, feedback che dà la risposta di un altro esercizio.
- Più di due esercizi consecutivi senza spiegazione in mezzo, a meno che la sezione sia di sola pratica.
- Usare un tipo di esercizio per forma e non per capacità da dimostrare (vedi README §1.6).

## 10. Codice di un esercizio (`code`, contratto `sdui.md` §4.1)

Vale per `quiz_single`, `quiz_multi`, `open_answer` e per il **fronte** di `flashcard`. Principio: contiguità spaziale e coerenza (Mayer): il codice è un oggetto da leggere, non una frase; sta separato dalla prosa serif e **sopra** la domanda, così l'ordine di lettura è «guarda questo» → «rispondi a questo».

### 10.1 Anatomia e posizione

```
┌ ExerciseCard ────────────────────────────────┐
│ (✎) Scelta singola                           │ intestazione (§1)
│ 12 dp                                        │
│ ┌ CodeSnippet ─────────────────────────────┐ │ fill codeBg, raggio sm (6), nessun bordo
│ │ let mut s = String::from("ciao");      ▒▒│ │ code.block 14/22, padding 12 v × 16 h
│ │ let r = &s;                            ▒▒│ │ ▒ = sfumatura 24 dp se c'è altro a destra
│ │ println!("{}", r);                       │ │
│ └──────────────────────────────────────────┘ │
│ 16 dp                                        │
│ Che cosa stampa il programma?                │ domanda (reading.question)
│ Scegli una risposta.                         │ istruzione
```

- Ordine nella card: intestazione → **code** → domanda → istruzione → corpo. Spazio: 12 dp tra intestazione e codice, 16 dp tra codice e domanda. Il codice occupa l'intera larghezza interna della card (card padding 20, compact 16); nessun margine proprio.
- È il rendering del blocco `code` (`code.md`) **ridotto**: stesso `codeBg`, `codeInk`, token sintattici (tabella design-system §2.4, chiaro e scuro), JetBrains Mono 400 14/22 (200 %: interlinea 28 sp), niente ligature, tab = 4 spazi, tokenizer per `language` come in `code.md`. Contrasti già verificati (13,3 / 13,1).
- **Differenze rispetto a `code.md`**: nessuna intestazione da 48 dp, nessun bottone Copia, nessun numero di riga, nessuna caption, nessun «Mostra altre righe». L'etichetta lingua è una riga `ui.labelS` `codeComment` in alto a sinistra del corpo (padding 12 dp sopra, 4 dp sotto, poi il codice) **solo se** `language` ∉ {`text`, `output`, vuoto}. Motivo: nel quiz il codice è breve e il numero di riga non serve (la domanda non rimanda a righe: il Generator non deve citarle).
- Nessun ombra, nessun bordo: sulla card `surface` il fill `codeBg` basta come contrasto di superficie.

### 10.2 Scorrimento orizzontale e overflow

- Scorrimento **solo orizzontale**, senza a capo (l'indentazione è significato), scrollbar nascosta. La larghezza di contenuto = riga più lunga; se ≤ larghezza disponibile non scorre e non c'è sfumatura.
- **Indicazione di altro a destra** (non solo colore, non solo gesto): (a) sfumatura 24 dp da `codeBg` a trasparente sul bordo destro, visibile solo se `scrollX < max`; sparisce a fine corsa; (b) l'ultimo carattere visibile non viene mai tagliato a metà quando il codice è a riposo: la riga si ferma a un limite di colonna (`scrollX` iniziale 0, il padding destro di 16 dp resta fuori dalla sfumatura); (c) se la riga più lunga eccede la larghezza di > 8 colonne, a sinistra compare la stessa sfumatura quando `scrollX > 0`. Con «riduci animazioni» la sfumatura è comunque statica (non è un'animazione).
- Primo uso: nessun hint animato. L'overflow si vede dal testo che arriva al bordo con la sfumatura.
- Il gesto orizzontale **non** deve rubare lo scroll verticale della lezione: soglia di direzione standard (`nestedScroll` / `horizontalScroll`); un trascinamento prevalentemente verticale passa alla pagina.
- Larghezza di riferimento: compact (card interna ≈ 328 dp) ≈ 38 colonne (8,4 dp/col.); tablet expanded (colonna esercizio 600-720 dp) ≈ 70-80 colonne. Il Generator mira a ≤ 60 colonne: scorre sul telefono, non sul tablet.

### 10.3 Altezza massima

- Fino a **16 righe** (`codeCollapsedLines`): altezza naturale (≤ 16 × 22 + 24 = 376 dp). Il contratto limita il codice a 1000 caratteri; con righe ≤ 60 sono ≤ ~17 righe, quindi nella pratica non c'è tetto.
- Oltre 16 righe (caso raro): stesso collasso di `code.md` (altezza 16 righe, sfumatura in basso 32 dp, Button testuale 48 dp «Mostra le altre N righe» / «Mostra meno»). **Mai scorrimento verticale interno** (scroll annidato = trappola). Lo stato espanso/collassato resta com'è dopo la risposta.

### 10.4 Telefono verticale e tablet orizzontale

| | Telefono (compact) | Tablet expanded 1280 × 800 |
|---|---|---|
| Larghezza codice | card interna (≈ 328 dp) | colonna della lezione, ≤ 720 dp (la card non si allarga oltre) |
| Padding card | 16 | 20 |
| Codice a 60 colonne | scorre (~38 visibili) | non scorre |
| Altezza card con 12 righe di codice + domanda + 4 opzioni | può superare il viewport: si scorre la pagina; il codice resta sopra, fuori vista quando si guardano le opzioni in fondo | entra in ~700 dp |

Non si usa un layout a due colonne (codice a sinistra, domanda a destra) su tablet: mantiene l'ordine di lettura unico e lo stesso comportamento su tutti gli schermi (coerenza). Con la tastiera a schermo aperta (`open_answer`), il codice è parte della pagina e scorre fuori: l'utente non perde il testo perché la bozza è nel campo; se serve rivederlo, scorre su. Nessun pannello fisso (ruberebbe spazio al campo).

### 10.5 Dopo la risposta

- Il codice **resta visibile, identico** e nella stessa posizione per tutta la vita della card (`checked`, `retrying`, `locked`, ripristino dopo riavvio): il feedback spesso lo cita, e senza vederlo sarebbe incomprensibile (contiguità). Nessuna marcatura, nessun colore di esito, nessuna riga evidenziata (la card non cambia con l'esito, §1).
- Lo scorrimento orizzontale mantiene la posizione dell'utente (non torna a 0 dopo «Controlla»).
- `bringIntoView` del pannello di feedback (§4) ignora il codice: se il codice esce dallo schermo, resta fuori.
- `open_answer`: durante la scrittura il codice resta sopra la domanda e non cambia; dopo l'invio idem.

### 10.6 Flashcard

- **Fronte**: codice sopra il testo `front` (stesse misure; 16 dp di stacco). Il fronte `reading.cardFront` 24/34 resta ma, **se c'è codice**, scala a `reading.question` 18/28 per non dominare il blocco monospazio.
- **Retro**: il codice **resta** sopra, sopra il fronte ridotto (`reading.body` `inkSoft`), perché il fronte da solo non si regge: ordine codice → fronte ridotto → hairline → risposta → voti. Nessuna dissolvenza del codice nel `cardReveal` (150 ms agisce solo sul retro; il codice non si ricompone: niente sfarfallio).
- Il codice nel retro non si attenua: stessi colori del fronte.
- Il ripasso (`screens/review.md`) non riceve `code`: le sue schede nascono dai learning record e dai termini del glossario, con fronte e retro propri (`reviews.md`), non dalle flashcard della lezione.

### 10.7 Accessibilità

- Il codice è **un solo nodo** TalkBack, **prima della domanda** nell'ordine di lettura (e prima dell'istruzione). Come in `code.md`: `text` = codice integrale con gli `\n` preservati (TalkBack legge i simboli secondo la verbosità scelta dall'utente), descrizione «Codice <lingua>, N righe». Non marcato con la lingua naturale dell'esercizio (è codice).
- Azioni custom sul nodo: «Scorri a destra» / «Scorri a sinistra» (se c'è overflow). Il contenitore scorrevole è focusable con tastiera: anello `focus` 3 dp attorno al corpo; ←/→ scorrono di 48 dp, Home/End agli estremi, ↑↓ lasciano il focus (come `code.md`). Tab: intestazione → codice → opzioni.
- Target: il codice non è un controllo (nessun target 48 dp); l'area di trascinamento è tutta la sua altezza (≥ 48 dp con una riga: altezza minima del blocco 48 dp).
- Font 200 %: righe 28 sp, nessun a capo, scorrimento più frequente; l'etichetta lingua resta su una riga.
- Il significato non dipende dai colori sintattici (`code.md`).

### 10.8 Selezione e copia

- Il testo è **selezionabile** (pressione lunga, `SelectionContainer`) con la barra di sistema (Copia, Condividi); la selezione **non** include l'etichetta lingua. Non c'è un bottone Copia dedicato (nel quiz il codice è una domanda, non materiale da portar via: meno controlli, carico cognitivo). La selezione non deve intercettare il tocco sulle opzioni (il blocco codice è fuori dal gruppo di opzioni).
- Copiato: testo integrale con `\n`, tab già espansi, senza numeri.

### 10.9 Evidenziatore e lettura ad alta voce

- **Evidenziatore** (`highlight.md`): non tocca gli esercizi (il testo evidenziabile è solo `paragraph` e `callout`; un tratto che inizia in un esercizio o in un blocco di codice scorre la pagina). Il codice di un esercizio non si evidenzia e l'evidenziazione non si estende ai blocchi vicini. Nessuna modifica a `highlight.md`.
- **Lettura ad alta voce** (`screens/lesson-audio.md`): gli esercizi non si leggono mai e il codice non si legge; quindi il codice di un esercizio è fuori dal set del contratto: nulla da specificare. Quando la lettura si ferma davanti all'esercizio obbligatorio la pagina scorre per portare **la card intera, codice incluso**, a 24 dp sotto la barra.

### 10.10 Codice inline breve nella domanda (e nelle opzioni, nel feedback)

Il difetto attuale: lo span `code.inline` con sfondo `surfaceSunken` si spezza a capo riga in due pezzi con sfondo ritagliato, e nei testi serif sembra rumore.

Regole (sostituiscono la resa di `code.inline` di `paragraph.md` per tutti i campi md, anche fuori dagli esercizi):

- Span: JetBrains Mono 400, dimensione = base − 2 sp (domanda 18 → 16; corpo 17 → 15; opzione/feedback come la loro base), `codeInk` su `surfaceSunken`; ligature disattivate. Padding orizzontale 2 dp ottenuto con **hair space** (U+200A) prima e dopo, non con padding dello span: Compose `background` non ha padding e così lo sfondo non tocca le lettere vicine. Angoli arrotondati non ottenibili con `SpanStyle.background`: accettare l'angolo retto (non disegnare custom).
- **Non andare a capo dentro lo span** quando il testo dello span ha ≤ 24 caratteri: sostituire gli spazi interni con NBSP (U+00A0) e inserire uno zero width space (U+200B, punto di a capo invisibile) dopo `(`, `::`, `.` solo se lo span supera 24 caratteri (allora si può spezzare lì, mai tra due lettere). Quindi lo sfondo non si spezza per le espressioni brevi: va a capo l'intero span sulla riga successiva. Il Validator limita comunque l'inline (`inline_code_too_long`), perciò gli span lunghi sono rari. Se uno span supera la larghezza di una riga intera, può spezzarsi: sfondo continuo per riga, non è un caso da curare.
- Solo l'etichetta testuale: nessun bordo, nessuna ombra, nessun colore sintattico (è inline: troppo piccolo per aiutare).
- Selezione e TalkBack: lo span è parte del testo; TalkBack lo legge come parte della frase, senza annunciare «codice» (rumore).
- Nei campi di tipo «una sola riga, istruzioni multiple» il Generator deve usare `code`, non gli span (§10.11).

### 10.11 Cosa il Generator deve evitare (codice di esercizio)

- Metterci **più di un concetto** o boilerplate (`fn main`, `use`, import) che non è l'oggetto della domanda; ≥ 3 righe va bene, ≤ 12 meglio; righe ≤ 60 colonne (sul telefono ne passano ~38: il resto scorre).
- Citare «la riga 2»: nel codice dell'esercizio non ci sono numeri di riga. Si cita il nome o l'istruzione («`let r = &s;`»).
- Ripetere nella domanda il codice già nel campo `code` (parafrasare cosa chiede, non rimettere le istruzioni).
- Mettere nel codice un commento che dà la risposta («// qui il valore viene mosso»).
- Mettere codice nei campi `text`/`options` di un'opzione per scrivere un programma: le opzioni sono brevi; un frammento di più istruzioni va in `code`, e le opzioni sono risposte («`a` è ancora valida», «errore di compilazione»), non alternative di codice (nel quiz le opzioni non hanno `code`).
- Usare `code` quando basta un'espressione breve (rimane inline, resta nella domanda).
