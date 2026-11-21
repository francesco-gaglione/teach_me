# `diagram`

> Contratto: `docs/contracts/sdui.md` §4 (riga `diagram`) e §10; decisioni: `docs/adr/0010-diagrammi-sdui.md`. Nessun token nuovo: tutto è in `../tokens.json`. Figura **statica**, disegnata dall'app in Compose (Canvas + testo), solo nelle lezioni.

## Scopo didattico
**Dual coding** (Paivio) e **contiguità spaziale** (Mayer): una struttura (passaggi, gerarchia, ciclo) si capisce più in fretta e si ricorda meglio vista che descritta, perché le relazioni sono *posizione e direzione* invece di frasi da tenere in memoria di lavoro (Sweller: la figura la scarica).

- **Aiuta** quando la relazione tra le parti è il contenuto: un processo con ordine o ramificazioni (`flow`), una classificazione o scomposizione (`tree`), un processo che ritorna su se stesso (`cycle`).
- **È decorativa** (e va tolta: coerenza di Mayer) quando ripete un elenco o una frase già chiara, quando ha 2 nodi, quando i nodi sono tutti "cose" senza relazione, o quando il testo dice già tutto. Una figura che non si guarda per capire, ma solo per abbellire, costa attenzione senza dare niente.
- **Non è un esercizio**: non si risponde, non si corregge, non conta per il progresso. Non deve però **anticipare la risposta** di un esercizio (vedi Linee guida).

## Anatomia

```
┌───────────────────────────────────────────────┐ ← FigureFrame: fill surfaceSunken, raggio md, nessun bordo, padding 16
│ Il ciclo del metodo scientifico               │ ← title (opzionale): ui.labelL, ink
│                                               │ gap 16
│   ┌───────────┐        ┌───────────┐          │
│   │ Nodo      │ ─────▶ │ Nodo      │          │ ← area del disegno (W = larghezza colonna − 32)
│   │ dettaglio │ etich. │           │          │
│   └───────────┘        └───────────┘          │
└───────────────────────────────────────────────┘
  Caption: cosa guardare / cosa se ne conclude.     ← fuori dal riquadro, 8 dp sotto, ui.bodyL, ink (md inline)
```

- `FigureFrame`: larghezza = colonna (≤ 600 dp, token `layout.readingMaxWidth`), altezza = contenuto. Fill `surfaceSunken`, raggio `shape.md` (10), padding 16 dp (`spacing.md`), nessun bordo, nessuna ombra (`elevation.flat`). Area di disegno `W = larghezzaColonna − 32` (568 dp nella colonna da 600; 296 dp su un telefono da 360 con margini 16).
- **Spaziature del blocco**: blocco precedente → figura e figura → blocco successivo 24 dp (`blockGap`); riquadro → `caption` 8 dp (`spacing.xs`). `title` → disegno 16 dp.
- **`title`** (se presente): `ui.labelL` (16/24, 700), `ink`, allineato a sinistra, può andare a capo. Se assente non si lascia spazio vuoto.
- **`caption`**: `ui.bodyL` (16/24), `ink`, markdown inline come in `paragraph.md` (grassetto, corsivo, codice inline, termini, citazioni); nessuna cornice. Fa parte del blocco ma è un nodo di testo separato e selezionabile.

### Nodo
| Proprietà | Valore |
|---|---|
| Forma | rettangolo arrotondato, raggio `shape.md` (10) |
| Fill / bordo | `surface` / `border`, spessore `stroke.control` (1,5 dp) |
| Padding | 12 dp orizzontale, 8 dp verticale |
| Altezza minima | 48 dp (`layout.minTouchTarget`: stessa riga ottica dei controlli; non è un target, la figura non è interattiva) |
| Larghezza minima | 104 dp; massima 200 dp (in orizzontale e albero), oltre il testo va a capo |
| `label` | `ui.label` (14/20, 600), `ink`, centrata, a capo su più righe, mai troncata |
| `detail` | sotto la label, gap 2 dp, `ui.bodyS` (12/16, 400), `inkSoft`, centrato, a capo |

Tutti i nodi della figura hanno lo **stesso aspetto**: nessun nodo "speciale", nessun colore semantico. Nessun elemento deve dire cosa è giusto o più importante (P9: il colore non porta significato). Contrasti: `ink` su `surface` 17,2 (chiaro) / 13,4 (scuro); `inkSoft` su `surface` 9,4 / 10,1; bordo `border` su `surface` 4,1 / 5,2 e su `surfaceSunken` 3,6 / 5,8 (≥ 3:1 per i componenti grafici, WCAG 1.4.11).

### Freccia
- Linea `stroke.control` (1,5 dp), colore `inkSoft`, estremi arrotondati. Contrasto di `inkSoft` su `surfaceSunken` 8,2 (chiaro) / 11,3 (scuro).
- **Punta** piena (triangolo, 8 dp di lunghezza × 8 di base) sul nodo di arrivo, colore `inkSoft`. La freccia si ferma a 2 dp dal bordo del nodo.
- Percorsi **ortogonali** (segmenti orizzontali/verticali con angoli a 90°, raggio 0); mai diagonali o curve (a parte i capi del `cycle`, sotto). Una freccia tra nodi allineati è una linea retta.
- Nel `tree` i collegamenti **non hanno punta**: la direzione è l'alto→basso (o l'indentazione). Nel `flow` e nel `cycle` sì.

### Etichetta di un arco (`edges[].label`)
- Pillola di testo: `ui.bodyS` (12/16), `inkSoft`, centrata, fill `surfaceSunken` (interrompe la linea), padding 2 dp × 4 dp, nessun bordo. Contrasto `inkSoft` su `surfaceSunken` 8,2 / 11,3.
- Posizione: **a metà del segmento principale** della freccia, sopra la linea (flow orizzontale) o a destra di essa (segmenti verticali). Contiguità spaziale: sempre attaccata alla sua freccia, mai in una legenda.
- A capo fino a 3 righe nello spazio tra i due nodi; mai troncata.

## Tre layout

Sia `W` l'area di disegno e `fs` il font scale di sistema. **Orientamento e a capo si decidono misurando il testo reale** (`TextMeasurer`, stessi stili, `fs` incluso), mai con soglie sul numero di caratteri.

### Regola comune (la figura non esce mai dalla colonna)
1. Si prova l'orientamento preferito con le misure sotto; se la somma delle larghezze necessarie ≤ `W`, si usa.
2. Altrimenti si prova l'orientamento alternativo (verticale).
3. Se nessuno dei due sta in `W` → **fallback testuale** (sotto). Mai scorrimento orizzontale, mai scala ridotta (il testo resta alla dimensione dei token), mai nodi che escono dal riquadro.
4. Con `fs` > 1,3 l'orientamento preferito è sempre quello verticale (il testo ingrandito ha bisogno di righe lunghe). Il testo non si ridimensiona mai oltre `fs`.
5. L'altezza è libera: la figura può essere alta quanto serve (scorre con la lezione). Dopo l'orientamento non si "comprime" mai uno spazio sotto i minimi della tabella.

### `flow` (processo con ordine)
Archi diretti, senza cicli (garantito dal Validator). I nodi si dispongono in **livelli** (lunghezza del cammino più lungo dalle sorgenti: sorgenti al livello 0); i nodi di un livello sono nell'ordine in cui compaiono in `nodes`, riordinati solo per ridurre gli incroci.

```
Orizzontale (preferito, tablet)                 Verticale (telefono, fs > 1,3, o 4+ livelli con etichette)

[Dati]─▶[Confronto]──d'accordo──▶[Regge]          [Dati]
                  └─in disaccordo─▶[Smentita]        ▼
                                                  [Confronto]
                                                  ├──d'accordo──▶ ┐
                                                  ▼                 ▼
                                               [Regge]          [Smentita]
```

| Misura | Orizzontale | Verticale |
|---|---|---|
| Livelli | colonne, da sinistra a destra | righe, dall'alto in basso |
| Nodi dello stesso livello | impilati, gap 16 dp | affiancati, gap 12 dp, larghezza uguale |
| Spazio tra livelli | 48 dp (arco senza etichetta), 96 dp (con etichetta) | 40 dp (con o senza etichetta; l'etichetta sta a destra della freccia) |
| Larghezza del nodo | 104-160 dp (misura del testo) | 104-200 dp |

- Larghezza necessaria in orizzontale = Σ larghezze dei livelli + Σ spazi. In verticale = il livello più largo (nodi affiancati + gap). Nodi di un livello affiancati che non ci stanno in `W` (es. 3 nodi su un telefono da 296 dp) → il verticale non è disegnabile → fallback.
- Archi che **saltano un livello** (a→c con b in mezzo): percorso lungo una corsia di scavalco di 12 dp fuori dai nodi (sotto in orizzontale, a destra in verticale), massimo 2; oltre → fallback.
- Più sorgenti: tutte al livello 0, stesso livello (colonna/riga).

### `tree` (gerarchia)
Un solo nodo radice, ogni altro nodo ha un solo genitore (garantito dal Validator). Ordine dei figli = ordine in `edges`.

```
Dall'alto (preferito, tablet)                    Elenco indentato (telefono, fs > 1,3, o troppo largo)

          [Variabili]                            [Variabili]
        ┌──────┼──────┐                           ├─[Indipendente]
  [Indipend.][Dipend.][Di controllo]               │   la cambi tu
                                                   ├─[Dipendente]
                                                   └─[Di controllo]
```

- **Dall'alto**: un livello per profondità, nodi di un livello affiancati con gap 12 dp, connettori a gomito (stub verticale 16 dp dal genitore, barra orizzontale, discesa al figlio). Spazio verticale tra livelli 40 dp. Il genitore è centrato sui figli. Serve che il livello più largo (nodi di larghezza 104-160 dp) stia in `W`: in pratica **al massimo 4 nodi nello stesso livello** nella colonna da 600 dp.
- **Elenco indentato** (sempre disegnabile, qualunque profondità ≤ 7): un nodo per riga, indentazione 24 dp per livello, connettore verticale a sinistra (linea `stroke.control`, `inkSoft`) con gomito di 16 dp verso ogni nodo; gap verticale 12 dp; larghezza del nodo = `W` − indentazione (≥ 104 dp, sempre vero con ≤ 8 nodi e profondità ≤ 7 se `W` ≥ 296... altrimenti il nodo va a capo di più righe: non esce mai).
- L'etichetta di un arco sta sul tratto verticale che arriva al figlio (dall'alto) o sul gomito, sopra il connettore (elenco).

### `cycle` (ciclo)
Nodi nell'ordine del ciclo, l'ultimo torna al primo; nessun `edges` da disegnare (le frecce sono implicite tra nodi consecutivi, e senza etichette).

```
Anello orizzontale (preferito, tablet)            Colonna con ritorno (telefono, fs > 1,3)

┌────────────┐ ──▶ ┌────────────┐                 ┌───────────┐ ◀┐
│ 1 Osserva  │     │ 2 Ipotesi  │                 │ Osserva   │  │
└────────────┘     └────────────┘                 └───────────┘  │
      ▲                  │                            ▼          │
┌────────────┐ ◀── ┌────────────┐                 ┌───────────┐  │
│ 4 Analisi  │     │ 3 Esperim. │                 │ Ipotesi   │  │
└────────────┘     └────────────┘                 └───────────┘  │
                                                        ...      ┘
```

- **Anello orizzontale**: i nodi occupano due righe: la riga alta da sinistra a destra con ⌈n/2⌉ nodi, la riga bassa da destra a sinistra con ⌊n/2⌋ nodi (allineati sotto gli ultimi della riga alta). Le frecce: tra nodi della stessa riga orizzontali (gap 32 dp), ai due estremi verticali (nodo estremo → nodo sotto/sopra). Il ritorno dall'ultimo nodo al primo è una freccia verticale sul lato sinistro. Con n = 3: 2 nodi in alto, 1 in basso sotto il secondo, e il ritorno è una L (sotto → primo nodo). Necessario: ⌈n/2⌉ × 104 + (⌈n/2⌉ − 1) × 32 ≤ `W` (quindi fino a 4 nodi per riga: n ≤ 8 sta in 568 dp).
- **Colonna con ritorno** (verticale): nodi impilati, larghezza `W` − 32, gap 32 dp con freccia in giù; dall'ultimo nodo parte una linea sul **lato destro** (corsia di 24 dp) che sale fino al primo con punta sul primo nodo; sulla linea, a metà, l'icona `repeat` (Lucide, 20 dp, `inkSoft`) su fill `surfaceSunken`, `contentDescription = null`: dual coding del "torna all'inizio" senza dipendere dal solo tracciato.
- Il cerchio vero (nodi su una circonferenza) è escluso: con 3-8 testi di larghezza diversa le etichette si sovrappongono o si spezzano in modo imprevedibile.
- **Numero d'ordine**: nei nodi del `cycle` a colonna e ad anello, un cerchio di 20 dp (`ui.labelS`, 12/16, `onAccent` su `accent`) in alto a sinistra del nodo indica la posizione 1…n (il ciclo non ha un inizio ovvio; la numerazione dice da dove leggere e rafforza la direzione senza colore). Non c'è numerazione nel `flow` e nel `tree`.

## Tema chiaro e scuro
Solo ruoli semantici (`surfaceSunken`, `surface`, `border`, `ink`, `inkSoft`, `accent`/`onAccent`); nessun colore hard-coded né `isSystemInDarkTheme()` nel blocco. Rapporti di contrasto verificati con `../tools/contrast_check.py`/script equivalente: testo (`ink`/`inkSoft` su `surface` e `surfaceSunken`) ≥ 8,1; bordo nodi e frecce ≥ 3,5 (chiaro) e ≥ 5,2 (scuro); `onAccent` su `accent` come negli altri componenti. In scuro il riquadro (`surfaceSunken` #0D0E11) è più scuro dello sfondo e i nodi (`surface` #1A1B1F) si staccano per fill + bordo: il bordo non è mai omesso.

## Stati
Statico: nessun idle/focus/risposta/giusto/sbagliato/rivelato/disabilitato. Non focusabile da tastiera (non interattivo), nessuna animazione d'ingresso oltre a `sectionReveal` della sezione, nessuna `liveRegion`. Con `reducedMotion` non cambia nulla.

## Fallback testuale
Si usa quando: `layout` sconosciuto; struttura che il renderer controlla e trova non valida (arco su nodo inesistente, ecc.: l'app non assume che il Validator abbia girato); nessun orientamento sta in `W`; più di 2 archi di scavalco. **Non** quando `alt` è vuoto o assente: allora il blocco non è renderizzabile e vale la riga neutra del §1 (`missing_component.md`).

```
┌───────────────────────────────────────────────┐ stesso FigureFrame
│ Il ciclo del metodo scientifico               │ title (se c'è): ui.labelL, ink
│ ⓘ Figura descritta a parole                   │ icona `image-off` 20 dp + ui.labelS, inkMuted
│                                               │ gap 8
│ Quattro tappe disposte in cerchio, collegate  │ alt: reading.body (17/28), ink,
│ da frecce nello stesso verso: osservazione... │ misura ≤ 600 dp
└───────────────────────────────────────────────┘
  Caption come sempre.
```

- Stesso riquadro, senza disegno: `alt` mostrato per intero (selezionabile) come testo normale, con il titolo e la `caption` sotto. Etichetta «Figura descritta a parole» (inkMuted su `surfaceSunken` 5,7 / 7,8) perché chi legge sappia che non è un errore e perché sia ancora una vera spiegazione.
- **Nessun messaggio di errore** né toast: il fallback è una versione degradata ma completa del blocco (P9).
- Lo screen reader legge lo stesso testo come testo, non come immagine: nel fallback il riquadro è un solo nodo che unisce titolo e `alt` **senza** `role = Image` (il ruolo immagine della sezione Accessibilità vale solo per la figura disegnata).

## Accessibilità
- **Un solo elemento** per TalkBack: il `FigureFrame` è un nodo `Modifier.semantics(mergeDescendants = false) { role = Role.Image; contentDescription = "<title>. <alt>" }` (senza il titolo se assente) con `clearAndSetSemantics` sul contenuto disegnato: nodi, frecce, etichette e numeri **non** sono focalizzabili né letti. `alt` è la descrizione ed è l'unica fonte per chi non vede: è per questo che il contratto la vuole autosufficiente. La `caption` è un nodo di testo separato subito dopo (letto come testo, con termini/citazioni tappabili).
- Nessun significato affidato al colore: la direzione è nella freccia e, nel `cycle`, nei numeri; la gerarchia è nella posizione. Tutti i nodi sono uguali.
- **Font scaling fino al 200 %**: il testo non si ridimensiona mai per far stare la figura; con `fs` > 1,3 si passa al verticale/elenco indentato, i nodi crescono in altezza e vanno a capo, l'altezza totale aumenta. Il riquadro non scorre mai in orizzontale. Se anche così non sta in `W` → fallback testuale.
- Lingua (`lang` ≠ `it`): testo di nodi, etichette e `alt` marcato con `LocaleList(lang)` come il resto del blocco.
- Nessun target di tocco (non interattivo). Selezione/copia: solo `caption` e, nel fallback, `alt`.

## Contenuti al limite
- Etichetta di nodo da 40 caratteri: nodo a 3-4 righe (larghezza massima 200 dp); il Generator è tenuto a 24 (sotto). `detail` 60 caratteri: 3-4 righe a 12 sp.
- Etichetta di arco da 30 caratteri: 3 righe in 88 dp.
- Nodo con `label` vuota: struttura non valida → fallback. `title` o `caption` vuoti: non si disegnano, il blocco sì.
- 8 nodi `flow` in verticale su un telefono: figura alta ~ 700 dp, accettabile ma da evitare (sotto).

## Linee guida per il Generator
**Quando.** Una figura solo se la *relazione* tra le parti è il contenuto e un paragrafo la farebbe leggere con fatica: ordine e ramificazioni di un processo (`flow`), scomposizione/classificazione (`tree`), ciclo che si ripete (`cycle`). **No** per elenchi, confronti a due, definizioni, liste di proprietà, cronologie di date (paragrafo o `callout`), per ripetere con altri nomi un paragrafo accanto, per abbellire. Se non sai dire in una frase cosa si deve vedere nella figura (è la `caption`), non farla.

**Dove.** Nella **sezione di concetto**, subito dopo il paragrafo che introduce la struttura e prima del paragrafo di spiegazione passo-passo: il testo precedente la annuncia ("nello schema qui sotto…"), la figura segue senza altro blocco in mezzo (contiguità). Per il `cycle` e il `tree` la posizione giusta è dopo l'intuizione; per un `flow` che riassume l'esempio svolto, subito dopo l'esempio. Mai come primo blocco di una sezione, mai nell'intro né nella sezione di richiamo, mai a fine sezione dopo gli esercizi, mai tra due esercizi.

**Quante.** Al massimo **1 per sezione** e **2 per lezione** (3 solo se la lezione ha come tema una struttura): oltre diventa decorazione e spezza il ritmo. Una figura non sostituisce il testo: il testo spiega, la figura mostra la struttura (non ripetere in un paragrafo ogni nodo: ridondanza, Mayer).

**Quanto grande.** Contratto: 2-8 nodi, 12 archi. Per una resa leggibile su tutti i dispositivi:
| Layout | Nodi consigliati | Massimo consigliato | Note |
|---|---|---|---|
| `flow` | 3-5 | 6 | al massimo **2 nodi per livello** (rami paralleli ≤ 2), nessun arco che salti un livello, pochi incroci; se più, dividi in due figure |
| `tree` | 4-7 | 8 | un solo livello con più di 4 figli è un elenco: preferisci un paragrafo; profondità ≤ 3 |
| `cycle` | 3-6 | 8 | in ordine del ciclo, l'ultimo torna al primo; niente `edges` |
- **`label` dei nodi: 1-3 parole, ≤ 24 caratteri** (il contratto ammette 40), sostantivi o forme brevi ("Staging area", "Ipotesi"), mai una frase intera, mai con punto finale. Se serve dettaglio: `detail`.
- **`detail`**: ≤ 40 caratteri, un esempio o un chiarimento ("la cambi tu"), mai una frase che duplica la label. Su metà dei nodi al massimo.
- **`label` degli archi**: ≤ 20 caratteri, solo quando dice *perché* si passa da un nodo all'altro ("d'accordo", "git add"); mai "poi", "va a". Per i `cycle` non esistono.
- **`title`**: opzionale, ≤ 50 caratteri, dice *cosa* è la figura ("Il ciclo del metodo scientifico"), non una frase. **`caption`**: dice *cosa guardare* o *che cosa se ne conclude* (1 frase, ≤ 140 caratteri consigliati), non ripete il `title`; il grassetto, se c'è, una sola volta.
- **`alt`**: autosufficiente (lo legge chi non vede e sostituisce la figura se non si disegna): dice il layout in parole ("percorso da sinistra a destra", "albero con radice…", "quattro tappe in cerchio"), elenca i nodi **nell'ordine di lettura** con i `detail` e dice come si connettono. 2-4 frasi. Mai "figura che mostra", mai una descrizione di colori o posizioni in pixel.
- **La figura non anticipa un esercizio.** Se la sezione (o una successiva) ha un `order_steps`, un `quiz_*` o un `fill_blank` che chiedono l'ordine, la classificazione o il passo successivo di *quella stessa struttura*, la figura (e `alt`, `caption`, `title`) non mostra l'ordine o la risposta: usa la figura per un caso diverso, o per un livello di dettaglio diverso, o mettila in una sezione che non ha esercizio sullo stesso contenuto. L'esercizio deve poter chiedere un richiamo a memoria (Bjork), non una copia da ciò che si vede due blocchi sopra. Regola pratica: se togliendo la figura l'esercizio diventerebbe difficile *nel modo giusto*, la figura è al posto sbagliato.
- Tutti i testi sono **testo semplice** salvo la `caption`: niente markdown, termini del glossario o citazioni dentro nodi, archi, `title`, `alt` (il glossario e le fonti vanno nel paragrafo che annuncia la figura, o nella `caption`).
- Mai coordinate, colori, forme, dimensioni o indicazioni di orientamento: li decide l'app.
