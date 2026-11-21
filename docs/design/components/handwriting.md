# Scrittura a mano (penna)

> ADR 0015 (decisioni prese, non si riaprono), contratto `lessons.md` §11 (proposta in fondo a questo file). Non è un blocco SDUI: è uno strato dell'utente sopra l'intera lezione. Estende `highlight.md` (stessa penna, stessa tavolozza, stesso rilevamento) senza contraddirlo: con lo strumento «evidenziatore» tutto resta come in `highlight.md`. Token: `tokens.json` → `handwriting` e `color.*.pen*`.

## 1. Scopo didattico

Annotare a mano è **elaborazione e generazione** (Bjork): riformulare, collegare con una freccia, schizzare uno schema o una formula costringe a trasformare il testo invece di rileggerlo, e il disegno è un secondo canale oltre alle parole (dual coding, Mayer). Rischi noti: copiare il testo parola per parola dà fluenza senza storage, e annotare tutto toglie il ritmo alla lettura. Quindi:

- **Nessun invito a scrivere.** Niente «prendi appunti», niente contatori di tratti o di lezioni annotate, niente badge, nessun promemoria (non è un obiettivo da raggiungere, è uno strumento: P9 di `design-system.md`). Il Generator non ne parla (§12).
- **Nessun elemento nuovo finché non serve** (carico cognitivo, Sweller): senza penna la lezione è identica a oggi; la penna compare solo quando si usa (`highlight.md` §2).
- **Contiguità** (Mayer): la nota sta **sopra** il blocco che commenta, non in un riquadro separato.
- **Le note non entrano nel ripasso né nelle schede**: il ripasso è richiamo a memoria, e una nota accanto alla domanda sarebbe un indizio. Esistono solo nella schermata Lezione.
- **Errore = occasione**: tutto si annulla (§6), nessun dialog di conferma, nessun avviso.

## 2. Il modello in una pagina

```
 strumento attivo (tavolozza)   punta della penna            dito               punta gomma / tasto
 ─────────────────────────────  ───────────────────────────  ─────────────────  ───────────────────
 evidenziatore                  come highlight.md            come oggi          cancella evidenziazioni
 penna                          SCRIVE (tratto di inchiostro) come oggi          cancella tratti interi
```

- Un **tratto** appartiene al blocco di primo livello su cui è sceso la punta (§4.2); le coordinate sono divise per la larghezza del blocco (ADR 0015 punto 5).
- Le note **si vedono sempre**, con qualsiasi strumento attivo, con la penna o senza, e sempre sopra il testo e le evidenziazioni (§5.3).
- Il bottone **«Gomma»** della tavolozza (§3.4) fa fare alla punta ciò che nella tabella fa la colonna «punta gomma / tasto», per lo strumento attivo. Serve a chi ha una penna senza gomma né tasto.
- Le note si fanno e si modificano **solo con la penna** (ADR 0015 punto 11): il dito e il mouse non scrivono mai. Chi non ha la penna le vede, non può cambiarle (a parte l'azione TalkBack di §10).

## 3. Tavolozza (estende `highlight.md` §6)

La tavolozza è **una sola**, flottante in basso a destra, con le stesse regole di comparsa (alla prima rilevazione della penna), scomparsa (6 s dall'ultimo evento di penna o tocco sulla tavolozza) e posizione. Cambia il contenuto: a sinistra il **selettore dello strumento** (due bottoni, `highlight.md` §6), poi i controlli dello strumento attivo.

### 3.1 Forma estesa (finestra ≥ 660 dp di larghezza)

```
 Evidenziatore attivo (362 dp)
 ┌─────────────────────────────────────────────────────────────┐
 │ [▮highlighter][ pen-line ]│ ( ✓●) ( ● ) ( ● ) ( ● ) │ [eraser] │   h 56
 │      strumento            │  giallo verde blu rosa  │  gomma   │
 └─────────────────────────────────────────────────────────────┘

 Penna attiva (611 dp)
 ┌──────────────────────────────────────────────────────────────────────────────────────┐
 │ [ highlighter ][▮pen-line ]│ ( ✓●) ( ● ) ( ● ) ( ● ) │ (━)(━━)(━━━) │ [eraser] [↶] [↷] │   h 56
 │      strumento            │ inchiostro blu rosso verde│ spessori    │ gomma ann. rip.  │
 └──────────────────────────────────────────────────────────────────────────────────────┘
```

| Elemento | Misura e stati |
|---|---|
| Pillola | altezza 56 dp, padding orizzontale 4 dp, `surfaceRaised`, ombra `raised`, raggio `full`; separatori verticali 1 dp `line` con 4 dp per lato (9 dp). Larghezza penna: 96 (strumenti) + 9 + 192 (colori) + 9 + 144 (spessori) + 9 + 144 (Gomma, Annulla, Ripeti) + 8 = **611 dp**; evidenziatore: 96 + 9 + 192 + 9 + 48 + 8 = **362 dp**. 611 + 2 × 16 dp di margine = 643 dp: per questo la soglia fra forma estesa e compatta è **660 dp** (non più 600), e le finestre 600-659 dp usano la forma compatta |
| Colori d'inchiostro | 4 campioni da 48 dp (cerchio 36 dp pieno `penInk`/`penBlue`/`penRed`/`penGreen`, **senza** bordo); selezionato: spunta 20 dp `penOn` e anello esterno 2 dp `ink` a 2 dp. Nomi: «Inchiostro», «Blu», «Rosso», «Verde». «Inchiostro» è quasi nero in chiaro e quasi bianco in scuro (si adatta al tema, §4.1) |
| Spessori | 3 campioni da 48 dp: cerchio 36 dp `surfaceSunken`, dentro una linea orizzontale di 24 dp a `ink` di spessore 1,5 / 2,5 / 4 dp con estremi arrotondati; selezionato: anello esterno 2 dp `ink` a 2 dp. Nomi: «Sottile», «Medio», «Spesso». Lo spessore è lo stesso in dp a qualsiasi dimensione dello schermo (§4.1) |
| Gomma | bottone da 48 dp, icona `eraser` 24 dp; vedi §3.4 |
| Annulla / Ripeti | 2 bottoni da 48 dp, icone `undo-2` e `redo-2` 24 dp `ink`; disabilitati (pila vuota): icona `disabledInk`, `enabled = false`. Nomi: «Annulla» e «Ripeti» (§6) |
| Strumento attivo | i due bottoni dello strumento hanno fondo `accentSoft` + icona `accent` + barra 20 × 3 dp `accent` in basso per quello selezionato |
| Dissolvenza al cambio | gli elementi che cambiano si dissolvono in `fast` 100 ms; «riduci animazioni»: istantaneo. La pillola mantiene l'ancoraggio in basso a destra: cresce verso sinistra |
| Pressione | scala 0,92 in `fast`; focus tastiera: anello `focus` 3 dp attorno al target |

### 3.2 Forma compatta (finestra < 660 dp)

Con la Gomma né l'evidenziatore (362 dp) né la penna (611 dp) stanno in 328 dp (finestra da 360 dp meno 2 × 16 dp di margine): la pillola mostra **solo lo stato corrente** e i dettagli si aprono in un popover, per entrambi gli strumenti. Colore e spessore dell'inchiostro **si fondono in un solo bottone** (altrimenti la penna farebbe 362 dp).

```
                              ┌ popover penna (sopra la pillola, 8 dp) ┐
                              │ ( ✓●) ( ● ) ( ● ) ( ● )   colori 192    │
                              │ (━)  (━━) (━━━)          spessori 144   │
                              └─────────────────────────────────────────┘
 ┌──────────────────────────────────────────────────────────────┐
 │ [hl][▮pen] │ ( ●━ ▾ ) │ [eraser] [↶] [↷]    6 × 48 + 9 + 8 = 305 dp │   h 56   penna
 └──────────────────────────────────────────────────────────────┘
 ┌────────────────────────────────────────┐
 │ [▮hl][pen] │ ( ● ▾ ) │ [eraser]     4 × 48 + 9 + 9 + 8 = 218 dp │   h 56   evidenziatore
 └────────────────────────────────────────┘
```

- **Penna**: il bottone «Stile» mostra un cerchio 36 dp del colore d'inchiostro corrente con dentro una linea orizzontale di 24 dp `penOn` dello spessore corrente (1,5 / 2,5 / 4 dp) e un `chevron-up` 12 dp `inkMuted` accanto. Toccandolo si apre un popover con **due righe**: i 4 colori (192 dp) e i 3 spessori (144 dp). Nome del bottone: «Stile della penna», `stateDescription` «<colore>, <spessore>» (es. «Blu, medio»). Scegliere un colore **non** chiude il popover (si può scegliere anche lo spessore) e **spegne la Gomma** (§3.4); lo spessore idem. Il popover si chiude toccando fuori, con Back o dopo 6 s senza eventi (nessun timeout dopo un tocco nel popover).
- **Evidenziatore**: il bottone mostra il campione del colore corrente con `chevron-up`; il popover contiene i 4 colori e si chiude scegliendone uno.
- Stile del popover: `Popover` (fill `surfaceRaised`, ombra `raised`, raggio `md`, padding 4 dp).

### 3.3 Cosa si ricorda sul dispositivo

`DataStore`, uguale per tutte le lezioni, **non sincronizzato** (preferenza della mano e del tablet, non dato della lezione):

| Chiave | Valori | Default |
|---|---|---|
| `pen_tool` | `highlighter` \| `pen` | `highlighter` |
| `highlight_color` | come `highlight.md` §6 | `yellow` |
| `pen_ink_color` | `ink` \| `blue` \| `red` \| `green` | `ink` |
| `pen_width` | `thin` \| `medium` \| `thick` | `medium` |

Sì all'ultimo strumento ricordato: chi studia con la penna di solito resta in uno stesso modo per un'intera sessione, e rifare la scelta a ogni apertura è attrito (e la tavolozza si nasconde dopo 6 s: lo strumento va conservato comunque). Il default `highlighter` fa sì che dopo l'aggiornamento chi usa la penna ritrovi esattamente il comportamento di prima. **Ogni** valore sconosciuto o assente vale il suo default. La Gomma (§3.4) **non** si ricorda.

**Segnale dello strumento a tavolozza nascosta.** Con la penna attiva e la tavolozza nascosta il rischio è scrivere credendo di evidenziare (o viceversa). Contromisura: l'hover della penna fa ricomparire la tavolozza (`highlight.md` §6), quindi prima di scrivere si vede sempre lo strumento; in più, il primo istante di contatto è già la prova (l'inchiostro compare libero sotto la punta, l'evidenziatore no). Nessun altro indicatore.

### 3.4 Interruttore «Gomma»

Trasforma la **punta** (`TOOL_TYPE_STYLUS`) in gomma per **lo strumento attivo**. È pensato per le penne senza gomma né tasto laterale; chi li ha continua a usarli e può ignorare il bottone.

| Aspetto | Valore |
|---|---|
| Posizione | ultimo gruppo a destra, **prima** di Annulla/Ripeti (penna) o dopo i colori (evidenziatore), separato da 1 dp `line` come in §3.1. Vicino ad Annulla perché sono i due gesti «correttivi» |
| Bottone | 48 × 48 dp, icona Lucide **`eraser`** 24 dp (la stessa che `highlight.md` §8 usa per «Rimuovi») |
| Spento | icona `inkSoft`, nessun fondo |
| **Acceso** | fondo `accentSoft`, icona `accent`, barra 20 × 3 dp `accent` in basso (lo stesso linguaggio dello strumento attivo: forma + colore, non solo colore). Un bottone a due stati e non un radio: non è uno strumento, è un modo della punta |
| Pressione / focus | scala 0,92 in `fast`; anello `focus` 3 dp |

**Comportamento con la Gomma accesa**

- La punta si comporta **esattamente** come la gomma di oggi (§6.1): con la **penna** cancella tratti interi, anteprima al 35 %, al sollevamento li toglie; con l'**evidenziatore** cancella le parole evidenziate come `highlight.md` §7. Stessa area scrivibile (§5.1), stesse esclusioni (`TopBar`, tavolozza, `ContinueBar`…: lì la punta tocca).
- Il **dito non cambia**: scorre, tocca, risponde come in §7. La punta gomma fisica e il tasto laterale **funzionano come oggi a prescindere** dal bottone; con il tasto premuto a Gomma accesa si cancella comunque (il tasto non «inverte» né spegne).
- Con la penna e la Gomma accesa la punta **non risponde** agli esercizi, ai termini e ai link (§7: cancella). Con l'evidenziatore e la Gomma accesa i gesti che partono su un controllo valgono come per la gomma fisica oggi (nessuna novità).

**Quando si spegne**

1. toccando di nuovo il bottone (dito o punta);
2. cambiando **strumento** (Evidenziatore ↔ Penna);
3. scegliendo un **colore** o uno **spessore** dell'inchiostro, o un colore dell'evidenziatore: scegliere come scrivere vuol dire che si vuole scrivere (con il popover compatto vale al momento della scelta, non all'apertura);
4. uscendo dalla lezione o chiudendo l'app.

Annulla/Ripeti **non** lo spengono (si cancella, si annulla, si continua a cancellare). Una rotazione o un cambio di finestra lo conservano.

**Non si ricorda** (nessuna chiave in `DataStore`, §3.3): all'apertura di ogni lezione è spento. Motivo: con la Gomma ricordata, la volta dopo la penna «non scrive» e sembra rotta; chi cancella spesso lo accende in un tocco, mentre una penna che non scrive senza spiegazioni costa molto di più (errore = occasione, non frustrazione).

**Segnale a tavolozza nascosta.** Lo stato non si perde dalla vista perché: (a) l'hover della penna fa **ricomparire la tavolozza** (§3.3) col bottone acceso prima di toccare; (b) con la Gomma accesa, durante hover e contatto della punta compare un **anello cursore**: cerchio di 24 dp di diametro (= `2 · eraserRadius`, ciò che la gomma tocca davvero), contorno 1,5 dp `ink`, senza riempimento, centrato sulla punta, nessuna animazione, ritagliato come l'inchiostro (§4.3 livello 4: sopra le note). Mostra anche il raggio d'azione; (c) niente inchiostro compare scrivendo. L'anteprima al 35 % da sola **non basta**: compare solo se la punta tocca qualcosa, quindi su una zona vuota non direbbe nulla. L'anello non compare per la gomma fisica né per il tasto (nessuna novità per chi li usa). La tavolozza si nasconde comunque dopo 6 s: **non** resta fissa a Gomma accesa (nessun elemento permanente).

**Limiti e Annulla.** Il tetto dei tratti non limita la gomma (§8). Una passata = un'operazione di Annulla (§6.2) con la penna; con l'evidenziatore nessun Annulla (`highlight.md` §7), come oggi.

## 4. Aspetto del tratto

### 4.1 Colore, spessore, pressione

| Aspetto | Valore |
|---|---|
| Colori | 4 ruoli: `ink`, `blue`, `red`, `green` → token `penInk`, `penBlue`, `penRed`, `penGreen`. **Si salva il ruolo, non l'hex**: una nota fatta in tema chiaro si legge in tema scuro con i valori scuri (e viceversa), e il contrasto è garantito in entrambi |
| Opacità | **100 %** (opaco). Nessun alpha: i contrasti dichiarati valgono solo con il colore pieno |
| Spessore | `thin` 1,5 dp, `medium` 2,5 dp, `thick` 4 dp. **Costante in dp**, indipendente dalla scala della nota e dal font: una nota fatta su un blocco largo 600 dp e vista su uno largo 328 dp si rimpicciolisce di forma ma la linea resta leggibile |
| Estremità e giunzioni | tonde (`StrokeCap.Round`, `StrokeJoin.Round`); un tocco senza movimento è un **punto** (cerchio del diametro dello spessore: servono per «i», per i punti e per i puntini di sospensione) |
| Pressione | **non usata.** Spessore fisso per tratto. Motivi: varia troppo da penna a penna (e molte penne capacitive non la danno), raddoppierebbe la dimensione dei dati, e la lettura di una nota piccola ne guadagna in uniformità. Non è un limite del formato: la proposta di contratto (§13) lascia spazio per aggiungerla dopo in modo additivo |
| Curva | i punti si uniscono con segmenti curvi (quadratiche passanti per i punti medi): niente spigoli da campionamento. La curva è un dettaglio di disegno, non è nel dato |
| Dopo il sollevamento | il tratto raw si **semplifica** (Ramer-Douglas-Peucker, tolleranza `simplifyEpsilon` 0,4 dp convertita nello spazio del blocco) e si quantizza a interi (§13); a 0,4 dp la differenza non si vede, quindi nessun «salto» |

### 4.2 Contrasti calcolati (WCAG 2.x, `tools/contrast_check.py`, 206 coppie per tema, tutte superate)

Elemento grafico: soglia 3:1 (WCAG 1.4.11; la nota è contenuto dell'utente, ma deve restare leggibile su qualunque sfondo della lezione). Controllato su `bg`, `surface`, `surfaceSunken`, `surfaceRaised`, `codeBg`, `infoSoft`, `tipSoft`, `warningSoft`, `knowledgeSoft`, `correctSoft`, `incorrectSoft`, `partialSoft`, `dangerSoft`, `accentSoft` e sui quattro riempimenti di evidenziazione.

| Ruolo | Light (hex) | Su `bg` | Minimo su tutti gli sfondi | Dark (hex) | Su `bg` | Minimo su tutti gli sfondi |
|---|---|---|---|---|---|---|
| `ink` | `#1B1B1F` | 16,46 | 12,50 (su `highlightBlue`) | `#E4E2E6` | 14,44 | 6,36 (su `highlightGreen`) |
| `blue` | `#1F4FB0` | 7,18 | 5,45 (su `highlightBlue`) | `#8FB4FF` | 8,97 | 3,95 (su `highlightGreen`) |
| `red` | `#B02E2E` | 6,14 | 4,67 (su `highlightBlue`) | `#FF9E9E` | 9,41 | 4,14 (su `highlightGreen`) |
| `green` | `#1F7A3A` | 5,16 | 3,92 (su `highlightBlue`) | `#7ED99A` | 10,86 | 4,78 (su `highlightGreen`) |

Spunta `penOn` sui campioni (tavolozza): chiaro ≥ 5,38; scuro ≥ 8,97.

**Il colore non porta un significato dell'app**: rosso non vuol dire «errore» né verde «giusto». Sono i colori di una penna; l'utente dà il significato (stesso principio dei quattro evidenziatori). Per chi distingue male rosso e verde: il nome del colore è esposto a TalkBack nella tavolozza e nessuna informazione dipende da distinguerli.

### 4.3 Livello di disegno

Dal fondo al cielo, dentro la colonna della lezione:

1. sfondi dei blocchi e testo;
2. evidenziazioni (`highlight.md` §3: sotto il testo, ma sopra gli sfondi dei blocchi);
3. **note a mano** (sopra tutto il contenuto, anche sopra i controlli degli esercizi e le evidenziazioni);
4. tavolozza, popover, toast, `TopBar`, pannello di contesto, sheet e dialog: **sopra** le note, e ritagliano il disegno (le note non si disegnano sotto la `TopBar` né sopra il pannello di contesto: §5.1).

L'inchiostro **non intercetta tocchi del dito** né del mouse: il livello è solo disegno. L'ordine dei tratti è quello di creazione (il più recente sopra). Restaurare un tratto con Annulla lo rimette nella sua posizione di prima.

## 5. Dove si scrive

### 5.1 L'area di scrittura

È un rettangolo in coordinate della finestra, che segue lo scorrimento della lezione (le note scorrono con il testo).

```
 EXPANDED (≥ 1100 dp) 1280 × 800 dp
 ┌─────────────────────────────────────────────────────────────────────────────────────────┐
 │ TopBar                                                                   (non si scrive) │
 ├─────────────────────────────────────────────────────────────────────────────────────────┤
 │░░░░░░░░ margine ░░░░░░░░│   colonna 600 dp    │ 32 │ pannello 280 dp │░ margine destro ░│
 │ si scrive (x < 0)       │ blocchi (0 ≤ x ≤ 1) │ sì │  NON si scrive   │  NON si scrive   │
 │                         │                     │    │  (resta il dito) │                  │
 └─────────────────────────────────────────────────────────────────────────────────────────┘
        ←── area di scrittura: dal bordo sinistro della finestra a 32 dp dopo la colonna ──→

 MEDIUM e COMPACT, EXPANDED < 1100 (nessun pannello)
 ┌ finestra ──────────────────────────┐
 │ TopBar                             │
 │ ░ margine 16/24/32 │ colonna │ ░ ░ │   si scrive su tutta la larghezza della finestra,
 │ (x < 0, ma poco)   │ 0 ≤ x ≤ 1     │   margini compresi; compact: ~16 dp per lato
 └────────────────────────────────────┘
```

| Regola | Valore |
|---|---|
| Orizzontale | da `x = 0` della finestra fino a **colonna destra + 32 dp** (`contextPaneGap`) quando c'è il pannello di contesto; altrimenti a tutta la larghezza |
| Verticale | l'intero contenuto scorrevole, fra la `TopBar` e il bordo basso. Il tratto non si disegna sotto la `TopBar` (si ritaglia) |
| Esclusi | `TopBar`, pannello di contesto, tavolozza, popover, toast, sheet, dialog (qualsiasi superficie sopra: la punta lì fa ciò che fa normalmente), e i bottoni di `ContinueBar` (§7) |
| Telefono | il margine di 16 dp per lato è scrivibile; la parte di una nota fatta altrove che cade fuori dallo schermo **si ritaglia, ma resta nel dato** (ricompare sul tablet); la nota non si deforma per starci |
| Blocco dentro `fallback` | il blocco di primo livello è il `fallback` stesso: ha un solo id e una sola area |

### 5.2 A quale blocco va un tratto

Il punto di discesa della punta decide il blocco, **una volta sola**, e non cambia mai durante il tratto.

1. Si prende l'elenco dei blocchi di primo livello **visibili** (cioè le sezioni già svelate), nell'ordine del documento.
2. Ogni blocco ha una **fascia**: dal punto medio dello spazio fra il blocco precedente e questo (`blockGap` 24 dp: 12 dp sopra) al punto medio dello spazio col successivo. Per il primo blocco la fascia comincia dal bordo alto del contenuto; per l'ultimo finisce alla `ContinueBar` o in fondo.
3. La fascia è a tutta larghezza, margini compresi: un tratto nel margine sinistro ad altezza di un paragrafo appartiene a quel paragrafo.
4. **Una nota fra due blocchi** (negli spazi di 24 dp) va al blocco **più vicino**; a pari distanza, a quello **sopra**.

**Coordinate del punto**: `x = (X − sinistra della colonna) / larghezza della colonna`, `y = (Y − bordo alto del blocco) / larghezza della colonna` (stessa divisione per entrambi: il rapporto di forma si conserva). `x` esce da [0, 1] nei margini; `y` è negativo sopra il blocco e maggiore dell'altezza/larghezza sotto.

**Ridisegno**: `X = sinistra + x · larghezza`, `Y = bordo alto + y · larghezza`, con la larghezza e la posizione **attuali** del blocco. Cambiando larghezza (rotazione, finestra, telefono/tablet) la nota si scala. Cambiando l'a capo del testo (carattere più grande, larghezza diversa) la nota segue il **blocco**, non le parole: **limite accettato** (ADR 0015 punto 5). Altro limite della stessa natura: un blocco che cambia altezza (un `code` che si espande, il feedback di un esercizio) sposta ciò che sta sotto di lui ma non le note degli altri blocchi, che restano ancorate al **proprio** bordo alto.

### 5.3 Tratti che escono dall'area

- Un tratto **si chiude** (come se la punta si fosse alzata) quando la punta esce dall'area di scrittura (§5.1), da qualsiasi lato: bordo della finestra, `TopBar`, pannello, margine destro. Se la punta rientra restando giù **non** riprende: serve un nuovo contatto. Motivo: un tratto che «salta» fuori e dentro si ancorerebbe male e il gesto tipico è lo scrivere dentro un'area, non l'attraversarla.
- **Non c'è scorrimento automatico** quando si scrive vicino ai bordi (scrivere e scorrere sono due gesti separati; per scorrere si usa il dito).
- Le coordinate ammesse sono `x ∈ [-4, 5]`, `y ∈ [-4, 20]` (§13). Un punto fuori da questi intervalli chiude il tratto come sopra (non si arriva mai a un errore del backend: l'area di scrittura è più stretta dei limiti).
- Tratti già salvati che a una geometria diversa cadono (in parte o del tutto) fuori dall'area visibile si ritagliano e **restano nel dato**. Non si cancellano mai da soli.
- Se il blocco di un tratto non esiste più (lezione sostituita, `block_id` assente): il tratto non si disegna e non si invia; le note della lezione sostituita comunque si perdono (ADR 0015 punto 9).

## 6. Gomma, Annulla e Ripeti

### 6.1 Gomma: **tratto intero**

Decisione: la gomma toglie **il tratto intero**, non una parte.

- Motivi: (a) l'ADR 0015 punto 8 vuole tratti con id e un insieme semplice da rinviare: cancellare una parte vorrebbe dire tagliare un tratto in due nuovi con nuovi id, punti nuovi e dati nuovi a ogni passata; (b) il tratto di una scrittura a mano è breve (una lettera, una parola, una freccia), quindi cancellarlo intero è quasi sempre quello che si vuole; (c) Annulla/Ripeti sono lineari e prevedibili (un gesto = un'operazione su insiemi di tratti, mai geometria); (d) la gomma dell'evidenziatore è parziale perché lì l'unità è la **parola** e un'evidenziazione tagliata è un'altra evidenziazione sensata; per l'inchiostro non c'è un'unità sotto il tratto. Svantaggio accettato: una sottolineatura o una cornice lunga si cancella tutta e va rifatta; l'«Annulla» di §6.2 attenua il caso.
- **Come si attiva**: come in `highlight.md` §7: tool type `ERASER` (estremo opposto) oppure `STYLUS` con `BUTTON_STYLUS_PRIMARY` premuto al contatto, oppure `STYLUS` con l'interruttore «Gomma» acceso (§3.4). **Agisce sullo strato dello strumento attivo**: con la penna cancella tratti, con l'evidenziatore cancella parole evidenziate (§7 di `highlight.md`). Mai entrambi: se si cambia strumento si cambia ciò che la gomma tocca (prevedibile, e un tratto di gomma su un paragrafo non deve togliere per sbaglio l'evidenziazione che c'era sotto).
- **Cosa tocca**: il raggio di contatto è `eraserRadius` **12 dp** (24 dp di diametro: ≈ la punta di una gomma da matita) intorno al percorso della punta. Un tratto è toccato se la distanza fra un campione del percorso e la sua linea è ≤ 12 dp + metà dello spessore. Conta qualsiasi tratto della lezione sotto la gomma, di qualsiasi blocco, **non** solo quello su cui la gomma è scesa (a differenza dell'evidenziatore, qui il tratto è di per sé uno spazio, non testo).
- **Anteprima**: i tratti toccati mostrano il colore al **35 %** (`eraserGhostAlpha`) finché la punta è giù. Al sollevamento spariscono in `fast` 100 ms (istantaneo con «riduci animazioni»). `ACTION_CANCEL`: nessuna modifica, l'opacità torna al 100 %.
- Una gomma che non tocca niente non fa niente. Nessun toast, nessun dialog.

### 6.2 Annulla e Ripeti: **sì**

Sì, perché scrivere con la penna fa sbagliare di più che toccare con un dito, e la gomma a tratto intero toglie più di quanto si volesse. Solo per l'inchiostro (per le evidenziazioni resta il toast «Annulla» di `highlight.md` §8, perché lì l'azione è un tocco; qui il gesto di gomma è già deliberato).

- Pulsanti nella tavolozza (§3.1), visibili con la penna attiva. Scorciatoie da tastiera fisica: **Ctrl+Z** annulla, **Ctrl+Maiusc+Z** e **Ctrl+Y** ripetono (solo con focus sulla schermata Lezione e non su un campo di testo).
- Una **operazione** = un tratto scritto, oppure una passata di gomma (con tutti i tratti che ha tolto). Annulla di un tratto lo rimuove; Annulla di una gomma rimette i tratti tolti **nelle loro posizioni** di prima (ordine di disegno incluso); Ripeti rifa l'operazione.
- La pila vale per la **sessione della schermata** (si perde uscendo dalla lezione o chiudendo l'app: non si salva), profondità **50** (`undoDepth`); la 51ª fa scartare la più vecchia senza avviso. Una nuova operazione svuota la pila di Ripeti.
- Si applicano subito all'insieme locale e partono nella sincronizzazione (§9) come ogni modifica. Nessun toast, nessun annuncio visivo; TalkBack annuncia «Tratto annullato» / «Tratto ripristinato» (Polite) solo quando è la persona a premere il bottone o la scorciatoia.
- Se Ripeti/Annulla porterebbe oltre il tetto dei tratti (2000) o dei punti, non si applica e vale il messaggio del limite (§8).
- Non esiste un «Cancella tutte le note» di lezione in questa versione (vedi Aperto).

## 7. Penna attiva sugli esercizi, su termini e link

Con la penna attiva la punta **scrive sempre** sulle note; il dito non cambia.

| Elemento sotto la punta | Punta (strumento **penna**) | Dito | Punta (strumento **evidenziatore**) |
|---|---|---|---|
| Qualsiasi punto di un blocco di primo livello, margini, spazi fra i blocchi | scrive: crea un tratto (anche un punto, se non si muove) | come oggi: scorre, tocca, seleziona con la pressione lunga | come `highlight.md` (evidenzia sul testo, altrimenti scorre) |
| Opzioni, campi, ordinamento, schede, checklist e ogni controllo di risposta di un esercizio | **scrive, non risponde.** Una nota sopra un'opzione non la sceglie | risponde come oggi | un tocco con la punta risponde come oggi, un tratto scorre (`highlight.md` §4.1) |
| Termini `[[ ]]`, citazioni `[^ ]`, link, `lesson_link` | **scrive, non apre** | apre come oggi | un tocco con la punta apre (`highlight.md` §4.1) |
| `ContinueBar` (e i suoi bottoni «Continua», «Completa lezione») | la punta **tocca** (è un controllo di navigazione, non un contenuto); un tratto che parte lì non scrive e non scorre | tocca come oggi | invariato |
| `TopBar`, tavolozza, popover, sheet, dialog, toast, pannello di contesto | la punta **tocca** normalmente | come oggi | invariato |

Perché la punta non risponde agli esercizi con la penna attiva: chi annota vicino a un'opzione (una freccia, un cerchio, una nota) **non deve rispondere per sbaglio**, e una risposta registrata è un esito da cui non si torna indietro (`_exercises.md`). Il costo: con la penna in mano, per rispondere si usa il dito o si passa all'evidenziatore (due tocchi). È scelta voluta e più sicura dell'alternativa (un'opzione scelta da un puntino della penna).

**Contatto che chiude qualcosa**: se è aperto un popover («Rimuovi» di `highlight.md` §8, i popover della tavolozza) o uno sheet, il **primo** contatto della punta fuori da esso lo chiude e **non** scrive (la stessa regola del dito).

**Scorrimento con la penna attiva**: la punta **non scorre mai** (qualsiasi tratto è scrittura). Si scorre con il dito, con la rotellina del mouse, con i tasti (`lesson.md` §10) o con la barra di scorrimento. Un fling in corso si ferma al contatto della punta (come con il dito) e il contatto comincia comunque un tratto.

**Palm rejection**: come `highlight.md` §4.1: mentre la punta è giù i tocchi del dito si ignorano (sistema). Un `ACTION_CANCEL` annulla il tratto in corso senza lasciare nulla.

**Selezione del testo**: come `highlight.md` §9. La penna non seleziona; se una selezione è attiva e la penna scrive, la selezione si annulla.

## 8. Limiti e messaggi

I limiti sono quelli del contratto proposto (§13). L'app li fa rispettare **prima** che il backend li rifiuti:

| Caso | Comportamento |
|---|---|
| Tratto che supera `maxPointsPerStroke` (1000) | l'app **chiude** il tratto a 800 punti (`appSplitStrokeAtPoints`) e ne apre subito un altro con gli stessi stile e blocco, senza interrompere il gesto: l'utente non lo vede |
| 2000 tratti o 200 000 punti nella lezione | al contatto successivo con la penna **non** parte un tratto e compare un `Toast` (stile `lesson.md` §8.1, 6 s, senza azione) «Hai raggiunto il limite di note a mano per questa lezione.» La gomma e Annulla funzionano sempre. Nessun avviso prima del limite (niente ansia da contatore) |
| Punto fuori dall'intervallo ammesso | il tratto si chiude (§5.3) |

## 9. Sincronizzazione

Come `highlight.md` §11 (il segno è sempre disegnato subito dall'insieme locale, la sincronizzazione non è un passo che l'utente aspetta), con le differenze qui:

| Aspetto | Regola |
|---|---|
| Endpoint | `PUT /lessons/{id}/strokes`, insieme intero (ADR 0015 punto 8) |
| Quando si invia | **4 s** (`syncDebounceMs`) dopo l'ultima modifica (più lungo dei 1,5 s delle evidenziazioni: il corpo pesa di più e si scrive in raffiche), subito alla chiusura/pausa della schermata, al ritorno della rete e all'apertura della lezione se `dirty`. Mai durante un tratto in corso |
| Indicatore | l'icona `cloud-off` della `TopBar` (un solo indicatore per tutto ciò che è in coda) vale anche per le note; nel suo popover una riga «Alcune note a mano non sono ancora salvate. Le invio appena torna la connessione.» con «Riprova ora». Annuncio Polite a salvataggio riuscito: «Note a mano salvate.» |
| `422` con `details[].field` = `strokes[i]` | si scarta localmente **solo** il tratto i e si reinvia (al massimo 3 scarti per invio); si registra nel log; l'utente non vede nulla |
| `413` | l'insieme resta locale e visibile; non si ritenta da solo (riparte alla modifica successiva o con «Riprova ora»); nel popover la riga «Le note a mano di questa lezione sono troppe per essere salvate.» Non dovrebbe accadere: i limiti di §8 sono più stretti |
| `404`, `401`, apertura, `completed` | come `highlight.md` §11 |
| Con due dispositivi | vince l'ultimo invio, nessun merge, nessun avviso (ADR 0015 punto 8) |
| Persistenza locale | insieme e flag `dirty` si salvano su disco a ogni modifica e sopravvivono a kill e riavvio |

## 10. Accessibilità

- **TalkBack: come si annuncia un blocco con note.** Il nodo di ogni blocco di primo livello che ha note aggiunge alla sua `stateDescription` «n note a mano» (1: «1 nota a mano»); se il blocco ha anche evidenziazioni, si uniscono: «2 evidenziazioni, 1 nota a mano». Il contenuto delle note **non è leggibile** (sono disegni; non c'è riconoscimento di testo in questa versione) e non c'è `liveRegion`. Un blocco senza note non annuncia niente in più.
- **Azione TalkBack**: ogni blocco con note espone l'azione custom «Elimina le note a mano del blocco» (toglie tutti i suoi tratti, con `Toast` «Note a mano eliminate» e azione «Annulla» 6 s come `highlight.md` §8). È l'unico modo non visivo di toglierle. È disponibile **solo se la penna è stata vista in questa sessione** (ADR 0015 punto 11: senza penna le note non si modificano). Creare note senza vista non è nell'ambito di questa versione.
- **TalkBack attivo e penna**: la scrittura con la penna in esplorazione al tocco dipende dall'attraversamento dei gesti di TalkBack; non è garantita e **non si promette**: chi usa TalkBack legge le note come stato e le toglie con l'azione; scrive con la penna chi può spegnere la modalità di esplorazione. Da verificare su un dispositivo vero (Aperto).
- **Gomma**: `Modifier.toggleable(role = Role.Switch)` (annunciato «interruttore»), nome «Gomma», `stateDescription` «attiva» / «spenta», `contentDescription` dell'icona assente (il nome è del bottone). Al cambio annuncio Polite: «Gomma attiva: la punta cancella» / «Gomma spenta». Se la Gomma si spegne da sola (cambio di strumento, colore, spessore) nessun annuncio extra: lo dice già lo stato del bottone. L'anello cursore è solo visivo. Spenta/accesa non dipende dal solo colore (§3.4: barra e fondo).
- **Tavolozza**: il gruppo strumento è un `RadioGroup` «Strumento» con `RadioButton` «Evidenziatore» e «Penna» (`selected`, `stateDescription` «scelto»); colori («Colore dell'inchiostro»: «Inchiostro», «Blu», «Rosso», «Verde»), spessori («Spessore»: «Sottile», «Medio», «Spesso») «Stile della penna» (forma compatta: «<colore>, <spessore>»), la Gomma e i bottoni «Annulla»/«Ripeti» (`enabled` riflette la pila). Target 48 dp. Il focus dopo un cambio di strumento resta sul bottone dello strumento.
- **Non solo colore**: la selezione nella tavolozza ha spunta/anello/barra; l'inchiostro non porta significato dell'app; i quattro colori sono distinguibili per luminanza **e** sono nominati.
- **Font fino al 200 %**: lo spessore del tratto, la tavolozza e il disegno sono in dp (non scalano); il layout cambia, le note seguono il blocco (§5.2).
- **«Riduci animazioni»**: il tratto compare subito; la gomma toglie senza dissolvenza; il cambio della tavolozza è istantaneo. Nessun'altra animazione esiste.
- **Chi non ha la penna**: la lezione è identica a oggi. Le note di un altro dispositivo si vedono; non compare tavolozza né altro (`highlight.md` §2). Il mouse e la tastiera non scrivono.
- **Tema scuro**: stessi ruoli, valori scuri (§4.2); il tratto ha sempre contrasto ≥ 3 con ogni sfondo.
- **Contenuto sensibile**: le note sono dell'utente e per lui; non si leggono ad alta voce, non entrano negli screenshot di sistema in modo diverso dal testo.

## 11. Per android-dev

- **Architettura**: un livello di disegno sopra la `LazyColumn` della lezione (un `Canvas` a schermo del contenuto scrivibile, ritagliato a `clipRect` dell'area di §5.1) e un rilevatore di gesti al livello del contenitore della lezione, nella fase `Initial` come in `highlight.md` §13, che per lo strumento «penna» consuma gli eventi `Stylus`/`Eraser` dentro l'area scrivibile e lascia passare tutto il resto (dito, mouse, `ContinueBar`, tavolozza). Il disegno **non** intercetta tocchi.
- **Posizione dei blocchi**: si ricava da `LazyListState.layoutInfo` (offset e dimensioni degli item di primo livello) e dalla larghezza e sinistra della colonna. Tenere composto 1 item prima e dopo (`beyondBoundsItemCount`) così un tratto che entra nella finestra da un blocco appena fuori scorre in vista. Se l'item del blocco non è misurato, il suo inchiostro non si disegna finché non lo è.
- **Bassa latenza**: l'inchiostro in corso si disegna a bassa latenza con `androidx.graphics:graphics-core` (front buffer, `GLFrontBufferedRenderer`) o con `androidx.ink` se la dipendenza è già scelta; usare i punti storici e `MotionEventPredictor`. I punti **predetti** si disegnano ma non si salvano mai. Ripiego se la libreria non c'è: invalidazione del solo rettangolo sporco, mai una ricomposizione per punto. Quando il tratto si chiude, si semplifica (§4.1) e passa al disegno normale (`Path` in cache per tratto; si ricostruisce solo se cambia la scala del blocco).
- **Logica pura testabile senza UI**: aggancio al blocco (§5.2: fasce, pari distanza → sopra); conversione punto ↔ coordinate relative; chiusura del tratto fuori area; semplificazione e quantizzazione; spezzatura a 800 punti; gomma a tratto intero con raggio 12 dp; pila Annulla/Ripeti con ripristino alle posizioni originali e profondità 50; limiti; scarto per `422` indicizzato; ritaglio senza perdita dei dati.
- **Gomma**: stato `eraserOn` **non persistito** (ViewModel/`rememberSaveable`, spento all'apertura); soglia estesa/compatta 660 dp; il rilevatore di gesti tratta `Stylus` come `Eraser` quando `eraserOn`. Anello cursore: `drawCircle` 24 dp, stroke 1,5 dp `ink`, sul livello dell'inchiostro (§4.3), alimentato da hover e contatto `Stylus` (la logica di gomma, raggio 12 dp, resta la stessa).
- **Preferenze** `pen_tool`, `highlight_color`, `pen_ink_color`, `pen_width` in `DataStore`.
- **Id dei tratti**: `UUID` v4 generato alla discesa della punta.
- **Token**: `tokens.json` → `handwriting.*`, `color.*.pen*`.

## 12. Per il Generator (linee guida, da passare a llm-engineer)

La nota è dell'utente: il Generator non la crea, non la cita e non la sollecita.

- **Nessun testo che inviti a scrivere a mano** («disegna», «annota qui», «sul tuo quaderno»): non è nel catalogo, non funziona senza penna e peserebbe come un compito non verificabile. Le risposte scritte restano `open_answer` e `fill_blank`.
- **Blocchi brevi e uno scopo ciascuno** (segmentazione, Mayer): la nota segue il **blocco** e non le parole (§5.2, limite accettato). Un blocco da 60 parole mantiene la nota vicina a ciò che commenta anche se il testo si riadatta; uno da 400 parole la lascia lontana dalla frase che la ha motivata. Resta valido il limite già raccomandato in `paragraph.md`.
- **Un `id` di blocco stabile e unico** (già richiesto dal contratto): le note si agganciano a lui.
- **Non riscrivere una lezione per correggere** (ADR 0015 punto 9: sostituire una lezione cancella le note). Per un errore piccolo preferire la correzione in una lezione nuova o un `ask_agent`.
- **Non mettere informazioni essenziali nei margini né in forme che richiedono di scrivere sopra** (frecce, spazi vuoti da riempire con un disegno): il layout della lezione non lascia spazi per scrivere e non deve essere progettato per farlo.
- **Schemi e formule**: se un concetto si capisce meglio disegnandolo, un `diagram` ben fatto è già il disegno; l'utente può ricalcarlo o integrarlo con la penna senza che il Generator lo chieda.

## 13. Proposta per il contratto (`lessons.md` §11, da applicare al tech lead)

### 13.1 Forma del tratto

```json
{
  "id": "5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f10",
  "block_id": "p3",
  "color": "ink",
  "width": "medium",
  "points": [1204, 380, 1311, 402, 1498, 415]
}
```

| Campo | Regole |
|---|---|
| `id` | `UUID` v4 (stringa di 36 caratteri) generato dall'app alla discesa della punta. Unico nella lezione. Il backend non lo interpreta oltre al formato e all'unicità |
| `block_id` | id di un blocco di **primo livello** del documento (anche un `fallback`). Un blocco annidato non vale |
| `color` | `ink` \| `blue` \| `red` \| `green`. Ruolo, non hex. L'app tratta un valore sconosciuto come `ink` (e non scarta il tratto) |
| `width` | `thin` \| `medium` \| `thick`. L'app tratta un valore sconosciuto come `medium` |
| `points` | array **piatto di interi** `[x0, y0, x1, y1, …]` (lunghezza pari, ≥ 2, cioè ≥ 1 punto). Un tratto con **un solo punto** è un punto (tocco). `x` e `y` sono in **decimillesimi della larghezza del blocco** al momento del tratto, cioè `x_reale = x / 10000`, `y_reale = y / 10000` (ADR 0015 punto 5). Precisione: 1/10000 della larghezza del blocco (0,06 dp su una colonna di 600 dp; non si vede) |

Non c'è un campo `tool`: l'unico strumento che produce inchiostro è la penna (l'evidenziatore ha il suo insieme `highlights`); un campo che ha un solo valore è solo spazio. Non c'è `pressure`: nessuna pressione (§4.1). Se si aggiunge in futuro (campo `pressure`: array parallelo), sarà additivo.

L'**ordine dell'array** è l'ordine di disegno (dal fondo al cielo). Il backend lo conserva e lo restituisce com'è (a differenza delle evidenziazioni, che si riordinano per posizione nel documento).

### 13.2 Invio e lettura

```json
PUT /lessons/{id}/strokes
{ "strokes": [ { "id": "…", "block_id": "p3", "color": "ink", "width": "medium", "points": [ … ] } ] }
```

- Insieme intero, in una transazione, `{ "strokes": [] }` le toglie tutte; reinviare lo stesso corpo non cambia nulla; ammesso in ogni `status`, anche `completed`, senza cambiarlo; eliminare la lezione elimina le note (come `lessons.md` §10).
- La risposta contiene l'insieme come salvato, nell'ordine inviato. `LessonView.strokes` è l’array nudo, come `LessonView.highlights`.
- **Endpoint con limite di corpo proprio di 4 MiB** (il default di 1 MiB di `api-conventions.md` non basta: §13.4). `413 payload_too_large` oltre.
- Errori `422` con `details` `{field: "strokes[i].<campo>", issue}`: `block_id` inesistente, annidato o non intero → `invalid`; `color`/`width` mancante o sconosciuto → `invalid` (il backend accetta solo i valori del contratto; è l'app a tollerare valori sconosciuti in lettura); `points` non array di interi, dispari, vuoto → `invalid`; un `points` fuori dai limiti → `out_of_range` (sulla coppia: `strokes[i].points`); `id` duplicato → `strokes[i].id` `duplicate` (sul secondo); `id` non UUID → `invalid`; più di 2000 tratti → `strokes` `too_many`; più punti del tetto → `strokes` `too_many_points`; corpo senza `strokes` → `strokes` `invalid`. Una sola voce non valida fa rifiutare tutta la richiesta (come le evidenziazioni).

### 13.3 Limiti ragionevoli

| Limite | Valore | Motivo |
|---|---|---|
| Tratti per lezione | **2000** | una lezione di 8 sezioni con ~25 blocchi molto annotata ne ha 300-800; 2000 è oltre qualsiasi uso reale |
| Punti per tratto | **1000** (l'app ne spezza a 800) | un tratto di scrittura dopo la semplificazione ha 20-80 punti; una spirale lunga fino a qualche centinaio |
| Punti per lezione | **200 000** | tetto sulla somma, per limitare il corpo e il tempo di validazione; con 2000 tratti la media ammessa è 100 |
| `x` ammesso | **[-4, 5]** (cioè `[-40000, 50000]` in interi) | l'area scrivibile (§5.1) arriva a −0,9 nel margine di un tablet molto largo e a ≈ 1,05 a destra; i limiti lasciano un ampio margine di sicurezza |
| `y` ammesso | **[-4, 20]** (cioè `[-40000, 200000]`) | una ordinata di 20 larghezze è un blocco molto alto (un `code` lungo a 200 % di font su telefono) più una finestra di scrittura sotto |
| Corpo del `PUT` | **4 MiB** | vedi stima |
| `block_id` | 1-64 caratteri (come negli altri contratti) | |

### 13.4 Stima della dimensione

Un punto in JSON: due interi con virgola ≈ **11 byte** (es. `-1204,5380,`). Un tratto: ≈ 125 byte di intestazione (id da 36 caratteri più le chiavi `id`, `block_id`, `color`, `width`, `points`) + 11 byte per punto.

| Scenario | Tratti | Punti medi | Punti totali | Corpo |
|---|---|---|---|---|
| Qualche nota (una nota breve per sezione) | 40 | 30 | 1 200 | ≈ 18 KB |
| Lezione annotata con cura | 300 | 40 | 12 000 | ≈ 170 KB |
| **Molto annotata** (pagine di appunti, schemi) | 800 | 60 | 48 000 | ≈ 630 KB |
| Tetto del contratto | 2000 | 100 | 200 000 | ≈ 2,4 MB |

Una lezione **molto annotata sfiora i 650 KB**, meno del limite di 1 MiB ma con poco margine, perché ogni invio manda l'insieme intero; il tetto del contratto (2,4 MB) lo supera di molto. Conseguenza: serve il limite proprio di **4 MiB** per l'endpoint (non un aumento globale del default). Sono le dimensioni **dopo** la semplificazione a 0,4 dp. Lato rete: se serve ridurre, i punti si possono codificare a differenze (circa metà), ma complica backend e app e per ora non conviene; in alternativa la compressione di trasporto (`Content-Encoding: gzip`) riduce circa di 3-4 volte e non cambia il contratto. Da decidere al tech lead.

### 13.5 Regole per l'app

- Il tratto parte con il punto di discesa: `block_id` del blocco deciso da §5.2, coordinate come §5.2.
- L'app **non** invia tratti fuori dai limiti (§5.3, §8) né di un `block_id` assente.
- Sincronizzazione, `dirty`, scarto per `422` indicizzato e `413`: §9.
- Un tratto il cui `block_id` non esiste nel documento si ignora senza errore.
- `LessonView.strokes` sostituisce l'insieme locale solo se non `dirty` (come le evidenziazioni).

## Contenuti al limite e casi particolari

- **Lezione `completed`** («Rileggi la lezione»): si scrive come prima.
- **Telefono con note del tablet**: si vedono ritagliate (§5.3), ridotte in scala; non si modificano senza penna.
- **Rotazione mentre si scrive**: il tratto in corso si chiude (nessuna perdita: è già nell'insieme locale), poi si ridisegna ogni nota alla nuova geometria.
- **Multi-window / ridimensionamento**: la classe di finestra si ricalcola: la forma della tavolozza (§3.1/§3.2) segue la larghezza **attuale**; lo strumento scelto resta.
- **Blocco molto alto**: l'ancoraggio sale in `y` fino a 20 larghezze (§13.3). Oltre, il punto non si registra e il tratto si chiude.
- **Penna e mouse sullo stesso dispositivo**: il mouse non scrive mai (`TOOL_TYPE_MOUSE` non è penna).
- **Penna che entra nella finestra da fuori con la punta giù**: è un nuovo contatto; vale la regola di §5.1.
