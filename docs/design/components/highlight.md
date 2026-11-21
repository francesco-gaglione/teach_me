# Evidenziatore (penna)

> ADR 0011 (decisioni prese, non si riaprono), contratto `lessons.md` §10. **Dall'ADR 0015 la penna ha due strumenti** (evidenziatore e penna, scrittura a mano: `handwriting.md`); questo documento descrive l'**evidenziatore** e la tavolozza comune (§6). I gesti di §4, §7 e §8 della punta valgono con lo strumento «evidenziatore» attivo. Non è un blocco SDUI: è uno strato dell'utente sul testo di `paragraph` e `callout` (campo `text`; non il titolo del callout). Token: `tokens.json` → `color.*.highlight*` e sezione `highlight`.

## 1. Scopo didattico

Segnare è **elaborazione attiva**: scegliere cosa conta mentre si legge costringe a un giudizio sul testo (encoding più profondo della sola lettura) e rende la rilettura selettiva, cioè più rapida (segmentazione, Mayer: il ritmo è dell'utente). Rischio noto: evidenziare *tutto* dà l'illusione di fluenza (Bjork) senza storage. Perciò: nessun incentivo a evidenziare (nessun contatore, badge, promemoria «non hai evidenziato nulla»), il segno si fa a mano e a parole intere, e l'app non assegna un significato ai colori: il significato lo dà chi legge.

Principi qui applicati: coerenza e carico cognitivo (nessun elemento nuovo finché non serve), contiguità (tavolozza e «Rimuovi» appaiono accanto alla mano e al segno), errore = occasione (tutto si annulla o si toglie senza vergogna né dialog), accessibilità (P8-P10 di `design-system.md` §1).

## 2. Quando esiste qualcosa di visibile (regola «nessun elemento nuovo»)

- **Senza penna la lezione è identica a oggi.** Niente icona, niente barra, niente «modalità». L'unico caso in cui un telefono senza penna vede un'evidenziazione è se la lezione ne ha già (fatte su un altro dispositivo): si disegna come in §3 e si può togliere con un tocco (§8). Nessun altro cambiamento di layout, nessun margine riservato.
- **Penna rilevata** = primo evento con `MotionEvent.getToolType()` = `TOOL_TYPE_STYLUS` o `TOOL_TYPE_ERASER` (hover incluso) sulla schermata Lezione. Solo allora compare la tavolozza (§6). Motivo: non c'è modo affidabile e universale di sapere in anticipo se esiste una penna (`FEATURE_*` non la garantisce, e molti tablet la accettano senza dichiararla); rilevare l'uso è l'unico segnale vero, e non mostra nulla a chi non la usa.
- Il flag «penna vista» vale per la sessione della schermata; non si salva.

## 3. Aspetto

### 3.1 Forma e altezza

L'evidenziazione è un **rettangolo per ogni frammento di riga**, disegnato **sopra** gli sfondi dei span (code inline) e **sotto** il testo.

```
 riga di paragrafo (interlinea 28 dp)
 ┌──────────────────────────────────────────────┐ ← 0
 │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │ ← 2 dp (boxInsetY)
 │ ░░ Quando lo sposti a un'altra variabile ░░░ │   box 24 dp
 │ ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀ │ ← linea di base 2 dp, colore …Line
 └──────────────────────────────────────────────┘ ← 28
```

| Misura | Valore |
|---|---|
| Altezza riquadro | `lineHeight − 2 × boxInsetY(2)`: **24 dp** in `paragraph` (28), **20 dp** in `callout` (24). Centrato sulla riga: fra due righe evidenziate restano 4 dp (mai un blocco continuo: la riga si legge) |
| Estensione orizzontale | dal bordo sinistro del primo glifo al bordo destro dell'ultimo del frammento, più `boxInsetX` 2 dp per lato (non oltre il bordo dell'area di testo: si ritaglia) |
| Riempimento | `highlight<Colore>` **opaco**. Non si usa alpha né un blend: i contrasti dichiarati valgono solo con il colore pieno |
| Linea di base | **2 dp** (`lineWidth`) in basso nel riquadro, colore `highlight<Colore>Line` (≥ 3:1 su `bg`, `surface` e sugli sfondi dei callout: §10). È il segnale che resta visibile dove il riempimento è quasi uguale allo sfondo (es. verde su `tipSoft`) |
| Angoli | raggio 4 dp (`cornerRadius`) solo sugli **estremi dell'evidenziazione** (inizio sul primo frammento, fine sull'ultimo); i tagli per l'andata a capo hanno angoli retti sul lato del taglio |
| Spazi | gli spazi *dentro* l'intervallo sono evidenziati (il segno è continuo); quelli ai bordi no |

### 3.2 Righe multiple e parole sillabate

- Un intervallo che attraversa righe produce un riquadro per riga, ognuno con la propria linea di base. Le righe intermedie coprono tutta l'area del testo (da margine a margine del blocco).
- Una parola **sillabata** a capo (`Hyphens.Auto`) è un'unica parola per l'evidenziazione: i due frammenti (con il trattino) sono evidenziati insieme. Il trattino è un glifo di layout, non è nel sorgente.
- Due evidenziazioni diverse che si toccano (stesso blocco, colori diversi): fra i riquadri si lascia **1 dp** (`gapBetweenColors`) per distinguerle.

### 3.3 Convivenza con gli altri elementi

| Elemento | Resa dentro l'evidenziazione |
|---|---|
| **Grassetto / corsivo** | invariati |
| **Termine `[[key]]`** | testo `ink` + tratteggio `knowledge` 1,5 dp a 3 dp sotto la linea di base: **resta tappabile** (apre lo sheet). Il tratteggio sta dentro il riquadro (baseline + 3 dp ≈ 23 dp < 26 dp) e non tocca la linea di base dell'evidenziazione. Contrasto `knowledge` sul riempimento ≥ 4,65 (§10). Il termine è un'unità **atomica**: tocchi una sua parte, lo evidenzi tutto |
| **Citazione `[^key]`** | «[n]» in `knowledge`, dentro il riquadro; tappabile; atomica |
| **Link** | `accent` sottolineato (≥ 4,86 sul riempimento); tappabile |
| **Codice inline** | il riempimento dell'evidenziazione **copre** lo sfondo `surfaceSunken` dello span; il carattere monospazio resta (è il segno di «codice»). Testo `ink` ≥ 6,36 |
| **Citazione in lingua originale** («…» in un paragrafo con `lang`) | nessun trattamento speciale: è testo normale |
| **Selezione di sistema** | disegnata sopra l'evidenziazione col suo stile abituale; vedi §9 |
| **Callout** | il riempimento sta dentro il padding di 16 dp, sul solo corpo (non sul titolo né sull'icona). Il colore del callout (`xSoft`) non cambia mai: l'evidenziazione si legge per **riempimento + linea di base**; rapporti in §10 |

Regola di tappabilità: dentro un'evidenziazione un tocco su un termine/citazione/link fa ciò che fa fuori; un tocco su testo semplice apre «Rimuovi» (§8).

## 4. Il tratto (penna)

### 4.1 Cosa riconosce

Solo eventi con tool type `STYLUS` o `ERASER`. Il dito e il mouse non evidenziano mai. Mentre la punta è giù, i tocchi del dito sulla stessa schermata si ignorano (palm rejection di sistema; l'app non scorre con la mano appoggiata).

Il gesto sulla penna si decide al primo movimento:

| Gesto | Condizione | Effetto |
|---|---|---|
| **Tocco con la penna** | punta giù e su con spostamento totale ≤ **8 dp** (`tapSlop`) e durata ≤ **400 ms** (`tapMaxDuration`) | come un tocco di dito: termine/citazione/link → la loro azione; su un'evidenziazione → «Rimuovi» (§8); altrove → nulla |
| **Tratto** | la punta si sposta > 8 dp da dove è scesa, partendo **sopra il testo evidenziabile** (corpo di `paragraph` o `callout`) | evidenzia (§4.2) o cancella (§7) |
| **Tratto fuori dal testo evidenziabile** | parte da un titolo, un titolo di callout, un esercizio, codice, margine, ecc. | la lezione scorre come per il dito (la penna non perde la funzione di scorrere) |
| **Punta ferma** > 400 ms senza muoversi | | nessun effetto (la penna **non** avvia la selezione del testo: §9) |

Un tratto iniziato dentro un blocco evidenziabile **non scorre mai la pagina**, anche se la penna esce dal blocco: l'evento si consuma al primo spostamento oltre la soglia. Il tratto appartiene al blocco in cui è sceso; punti in altri blocchi si ignorano (nessuna evidenziazione a cavallo di due blocchi: il contratto non la rappresenta).

### 4.2 Aggancio alle parole (snap)

1. **Unità**: una *parola* = sequenza di caratteri mostrati delimitata da spazi (la punteggiatura attaccata ne fa parte: «sposti,» è una parola). Gli elementi atomici (termine, citazione) sono un'unica unità anche se contengono spazi, e inglobano la parola a cui sono attaccati senza spazio.
2. **Riga colpita**: un punto del tratto appartiene a una riga se cade nella sua fascia di 28 dp (24 nel callout) estesa di **4 dp** sopra e sotto (`lineBandTolerance`). Punti nello spazio fra paragrafi o fuori fascia non contano.
3. **Parola colpita**: sulla riga, la parola il cui riquadro orizzontale contiene il punto; il riquadro include metà dello spazio che segue (passare fra due parole prende quella più vicina, mai nulla).
4. **Intervallo**: dalla **prima** all'**ultima** parola colpite *in ordine di lettura nel blocco* (continuo: comprende le parole in mezzo anche se la penna ha saltato una riga). Tornare indietro sulla stessa riga allarga l'intervallo ma non lo accorcia durante lo stesso tratto. Si prende il minimo e il massimo degli indici delle parole colpite.
5. **Conversione in sorgente** (a carico dell'app, regole di `lessons.md` §10): `start` = offset sorgente del primo carattere mostrato della prima parola; `end` = offset dopo l'ultimo carattere mostrato dell'ultima parola. Poi si **normalizza**: `start` arretra oltre i marcatori che aprono uno span il cui contenuto inizia lì (`**`, `*`, `` ` ``, `[`, `[[`), `end` avanza oltre quelli che chiudono uno span che finisce lì (`**`, `*`, `` ` ``, `](https://…)`, `]]`, `[^key]` è già atomico). Marcatori solo a metà dentro l'intervallo restano fuori. Mai uno spazio ai bordi. Un intervallo che dopo la conversione non copre alcun carattere mostrato si scarta.
6. **Un tratto che tocca zero parole** (parte nel testo ma non colpisce nessuna fascia valida) non crea nulla.

### 4.3 Anteprima durante il tratto

- Dopo il superamento della soglia, a ogni evento (compresi i punti storici di `MotionEvent`) si ricalcola l'intervallo e lo si disegna **in tempo reale** con il colore corrente: riempimento a **60 %** di opacità (`previewAlpha`), **senza** linea di base. Lo spostamento dell'intervallo è istantaneo (nessuna animazione fra parole: la latenza è la qualità percepita della penna).
- Niente scia di inchiostro libera: l'unico feedback è l'aggancio, perché il segno finale non è il gesto.
- **Al sollevamento**: l'anteprima diventa definitiva in `fast` 100 ms (alpha 60 → 100 %, compare la linea di base). Con «riduci animazioni»: istantaneo.
- **Annullamento del gesto** (`ACTION_CANCEL`, es. palm rejection o cambio di finestra): l'anteprima sparisce, nessuna modifica.
- Nessun suono, nessuna vibrazione, nessun toast alla creazione: l'azione è fatta a mano, la prova è il segno sulla pagina.

## 5. Interazione con evidenziazioni esistenti (nuovo tratto con colore C)

Lavorando sugli intervalli in parole (poi normalizzati e riconvertiti in sorgente), nello stesso blocco:

| Caso | Risultato |
|---|---|
| Tratto **dentro** un'evidenziazione dello stesso colore C | nessuna modifica (e nessun invio) |
| Tratto che **si sovrappone o tocca** (nessuna parola in mezzo) evidenziazioni dello stesso colore C | **fusione**: un'unica evidenziazione = unione degli intervalli. Anche se separate solo da spazi: gli spazi fra parole adiacenti si riempiono |
| Tratto che si sovrappone a un'evidenziazione di **colore diverso** | **sovrascrittura**: la parte coperta dal nuovo tratto prende C; la parte non coperta resta del colore vecchio. Se il nuovo tratto sta nel mezzo, il vecchio si **divide** in due; se lo copre del tutto, sparisce |
| Dopo la sovrascrittura, frammenti adiacenti dello stesso colore | si fondono (vale la riga «stesso colore») |
| Tratto che arriva a toccare un'evidenziazione di colore diverso senza sovrapporsi | nessuna interferenza (1 dp di spazio visivo, §3.2) |

Il risultato è sempre un insieme con intervalli **non sovrapposti**, come richiede il contratto. Il tetto di 500 evidenziazioni: se il nuovo insieme ne avrebbe più di 500, il tratto non si applica e si mostra il toast «Hai raggiunto il limite di evidenziazioni per questa lezione.» (nessun «Annulla»). Lo stesso vale per ogni operazione che farebbe superare 500: la gomma che divide un'evidenziazione in due e l'«Annulla» di una rimozione. Una fusione che riduce il numero è sempre ammessa.

## 6. Tavolozza (strumento e colore)

> Estesa dalla scrittura a mano (ADR 0015, `handwriting.md` §3): la tavolozza è **una sola** per evidenziatore e penna. Qui sta la forma per lo strumento «evidenziatore»; la parte della penna (colori d'inchiostro, spessori, Annulla/Ripeti, forma compatta) è in `handwriting.md` §3.

**Dove**: pillola flottante, **in basso a destra** della schermata Lezione, a 16 dp (`palette.margin`) dai bordi e sopra le barre di sistema. Non sta nella TopBar né nella colonna di lettura (non sposta nulla, non copre il testo in expanded: sta nel margine; in compact può coprire un angolo del testo, ma è temporanea ed è sotto la mano che scrive, non dove si scrive).

```
                      ┌──────────────────────────────────────────────┐
                      │ [▮ evid.][ penna ]│ ( ● ) ( ● ) ( ● ) ( ● )  │  h 56 dp, fill surfaceRaised,
                      │  strumento        │ giallo verde blu  rosa   │  ombra raised, raggio full
                      └──────────────────────────────────────────────┘
```

| Misura | Valore |
|---|---|
| Pillola | altezza **56 dp**, padding orizzontale 4 dp, `surfaceRaised`, ombra `raised`, raggio `full`. Da sinistra: **selettore dello strumento** (2 bottoni da 48 dp = 96 dp), separatore verticale 1 dp `line` con 4 dp di spazio per lato (9 dp), poi i **4 campioni** da 48 dp (192 dp). Larghezza con l'evidenziatore: 96 + 9 + 192 + 9 + 48 (Gomma, §7) + 8 = **362 dp**; sotto i **660 dp** di finestra la forma è compatta (colore in un popover, `handwriting.md` §3.2) perché 362 non entra in 328 dp (la finestra compact più stretta con margine 16) |
| Selettore dello strumento | due bottoni da 48 dp con icona 24 dp (`highlighter`, `pen-line`), nomi «Evidenziatore» e «Penna». **Selezionato**: fondo `accentSoft`, icona `accent` e barra di 20 × 3 dp `accent` in basso (forma, non solo colore). Non selezionato: icona `inkSoft`. Stato premuto: scala 0,92 in `fast`; focus tastiera: anello `focus` 3 dp |
| Campione | cerchio **36 dp**, riempimento `highlight<Colore>`, bordo 2 dp `highlight<Colore>Line` (visibile solo con l'evidenziatore) |
| Selezionato | una **spunta** (`check`, 20 dp, `ink`) dentro il campione e l'anello esterno 2 dp `ink` a 2 dp di distanza. Il selezionato non si distingue per il solo colore: forma (spunta) e anello |
| Nomi | «Giallo», «Verde», «Blu», «Rosa» |
| Stati | pressed: scala 0,92 in `fast`; focus tastiera: anello `focus` 3 dp attorno al target; nessun disabilitato |

**Scelta dello strumento**: toccando un bottone (dito o penna) lo strumento vale subito, i campioni accanto si sostituiscono (evidenziatore: 4 colori dell'evidenziatore; penna: colori d'inchiostro, spessori e Annulla/Ripeti, `handwriting.md` §3) senza animazione di spostamento (dissolvenza `fast` 100 ms degli elementi che cambiano; istantanea con «riduci animazioni»). Con lo strumento **evidenziatore** valgono **tutte** le regole di questo documento, invariate; con la **penna** i §4, §5, §7 e §8 non si applicano ai gesti della punta (la punta scrive: `handwriting.md` §5), mentre le evidenziazioni già fatte restano visibili e il dito le tocca come sempre (§8).

**Quando compare**: alla prima rilevazione della penna (§2), dissolvenza `fast` 100 ms. **Quando scompare**: **6 s** (`idleHideMs`) dopo l'ultimo evento di penna *o* tocco sulla tavolozza, dissolvenza `slow` 300 ms. Ricompare a ogni hover o contatto di penna (nessun gesto da imparare: avvicinare la penna basta). Non scompare mentre c'è un tratto in corso. Se si sovrappone alla `ContinueBar`, la tavolozza sta sopra solo finché è visibile. Lo strumento attivo resta quello scelto anche a tavolozza nascosta.

**Ultimo strumento e ultimo colore**: scelti toccando, valgono subito per i tratti successivi. Si salvano sul dispositivo (`DataStore`), **uguali per tutte le lezioni**, non sincronizzati (sono preferenze della mano e del tablet, non dati della lezione): `pen_tool` (`highlighter` \| `pen`; default **`highlighter`**, così chi non usa mai la penna come penna non cambia nulla; sconosciuto = `highlighter`) e `highlight_color` (default **`yellow`**; sconosciuto = `yellow`). I valori della penna (`pen_ink_color`, `pen_width`) sono in `handwriting.md` §3.

**Toast**: mentre la tavolozza è visibile i toast si alzano di 72 dp (`toastLift`) per non sovrapporsi.

## 7. Gomma e tasto della penna

**Modalità cancella** = punta `TOOL_TYPE_ERASER` (estremo opposto), oppure tool type `STYLUS` con `BUTTON_STYLUS_PRIMARY` premuto al momento del contatto. Per le penne senza gomma né tasto c'è in più l'interruttore **«Gomma»** della tavolozza (`handwriting.md` §3.4): acceso, la punta `STYLUS` vale come `ERASER` con questa stessa regola; si spegne toccandolo di nuovo, cambiando strumento o scegliendo un colore, e all'apertura di una lezione è spento. Durante hover e contatto compare l'anello cursore di 24 dp (`handwriting.md` §3.4).

- Si toglie **solo la parte toccata**: le parole colpite dal tratto (stessa unità e stessa fascia di §4.2, **senza** riempire fra le parole saltate: l'insieme sono le sole parole realmente passate). Un'evidenziazione toccata a metà si **divide** (le due parti restano), o si accorcia; si toglie tutta solo se tutte le sue parole sono state toccate. Dopo la rimozione si ripuliscono gli spazi ai bordi dei frammenti rimasti; i frammenti senza parole spariscono.
- **Anteprima**: le parole che la gomma ha toccato mostrano il riempimento a **35 %** (`eraseGhostAlpha`) senza linea di base finché la punta è giù. Al sollevamento spariscono in `fast` 100 ms (istantaneo con «riduci animazioni»). `ACTION_CANCEL`: nessuna modifica.
- Una gomma che non tocca alcuna evidenziazione non fa nulla. Un tratto di gomma parte e vale su tutto il testo evidenziabile del blocco in cui è sceso (come §4.1).
- Nessun toast e nessun «Annulla» per la gomma (gesto piccolo e ripetuto; per rimettere basta evidenziare di nuovo).

## 8. «Rimuovi» con un tocco

- **Tocco** (dito o penna, ≤ 8 dp e ≤ 400 ms) su un'evidenziazione, su una parte che non è termine, citazione o link: si apre un `Popover` (stile `lesson.md` §8.1: fill `surfaceRaised`, ombra `raised`, raggio `md`, padding 8 dp) con un unico Button testuale **«Rimuovi»** (icona `eraser` 20 dp a sinistra; altezza **48 dp**, larghezza min **96 dp**). Posizione: sopra la riga toccata, centrato sul punto del tocco, a **8 dp** dal bordo superiore della riga; se non c'è spazio sopra (sotto la TopBar) sotto la riga; sempre dentro lo schermo.
- Mentre è aperto, l'evidenziazione scelta mostra un contorno 2 dp `focus` attorno ai suoi riquadri (raggio 4 dp): si capisce *quale* si toglie (in una fusione lunga è quella intera).
- «Rimuovi» toglie **l'intera evidenziazione** (tutto l'intervallo contiguo dello stesso colore, anche se era nato da più tratti fusi). La gomma è lo strumento per toglierne solo una parte.
- Si chiude con: tocco fuori, scroll, Back, o toccando «Rimuovi». **Non** si chiude a tempo (nessun timeout, accessibilità).
- Dopo «Rimuovi»: l'evidenziazione sparisce in `fast`, e compare un `Toast` «Evidenziazione rimossa» con azione **«Annulla»** (6 s, come da `base-components` Toast). «Annulla» ripristina l'intervallo e il colore; dopo la scadenza non c'è più. Motivo: è il solo caso in cui un tocco distrugge lavoro intenzionale dell'utente senza un gesto deliberato come la gomma.
- Se la tavolozza è visibile il toast si alza (§6).
- Accessibilità: aprendosi, il focus passa a «Rimuovi»; al chiudersi il focus torna al paragrafo.
- Con più tocchi su evidenziazioni diverse il popover precedente si chiude.

## 9. Selezione del testo di sistema

Oggi `paragraph` e `callout` si selezionano con **pressione lunga del dito** (e mouse). Resta com'è:

- **Il dito** (pressione lunga, maniglie, copia) e il mouse **non cambiano**. La copia restituisce solo il testo, senza informazioni di evidenziazione.
- **La penna non avvia la selezione**: l'evidenziatore intercetta gli eventi di penna in fase `Initial` e li consuma prima della selezione. Così «penna ferma > 400 ms» non diventa una selezione involontaria durante la scrittura. Chi vuole copiare con la penna usa il dito.
- Se c'è una selezione attiva e la penna tocca il testo, la selezione si cancella (la penna riprende il controllo).
- Selezione e evidenziazione possono coesistere: la selezione si disegna sopra con lo stile attuale; i contrasti fra testo e riempimento non cambiano.

## 10. Contrasti calcolati (WCAG 2.x, `tools/contrast_check.py`, 130 coppie per tema)

Testo `ink` sull'evidenziazione (soglia 4,5; **tutti ≥ 6,3**, l'evidenziazione è su `bg`, su `surface` e sui tre sfondi dei callout **perché opaca**: il testo vede sempre il riempimento, non lo sfondo sotto):

| Colore | Light | Dark | Riempimento Light → Dark |
|---|---|---|---|
| `yellow` | 13,17 | 6,57 | `#FFE066` → `#5E4B00` |
| `green` | 13,12 | 6,36 | `#B8EEC2` → `#1F5A32` |
| `blue` | 12,50 | 6,39 | `#C2DFFF` → `#1F4F8C` |
| `pink` | 12,57 | 7,16 | `#FFD0E2` → `#7A2A52` |

Altro testo sul riempimento (soglia 4,5):

| Colore | `accent` (link) L / D | `knowledge` (citazione, tratteggio termine) L / D |
|---|---|---|
| `yellow` | 5,86 / 4,96 | 4,90 / 4,95 |
| `green` | 5,84 / 4,80 | 4,88 / 4,79 |
| `blue` | 5,56 / 4,82 | 4,65 / 4,81 |
| `pink` | 5,59 / 5,40 | 4,68 / 5,39 |

Linea di base (`…Line`, soglia 3 per i componenti non testuali) su ogni sfondo, Light / Dark:

| Colore | `bg` | `surface` | `surfaceRaised` | `infoSoft` | `tipSoft` | `warningSoft` |
|---|---|---|---|---|---|---|
| `yellow` | 4,9 / 10,6 | 5,1 / 9,8 | 5,1 / 8,6 | 4,2 / 5,4 | 4,3 / 5,7 | 4,3 / 6,0 |
| `green` | 5,2 / 9,4 | 5,4 / 8,7 | 5,4 / 7,6 | 4,4 / 4,8 | 4,5 / 5,1 | 4,5 / 5,3 |
| `blue` | 6,1 / 8,5 | 6,3 / 7,9 | 6,3 / 6,9 | 5,2 / 4,4 | 5,3 / 4,6 | 5,3 / 4,8 |
| `pink` | 5,8 / 8,0 | 6,0 / 7,4 | 6,0 / 6,5 | 5,0 / 4,1 | 5,1 / 4,3 | 5,1 / 4,6 |

Il **solo riempimento** ha basso contrasto contro lo sfondo (1,04-1,53 contro `bg`/callout in chiaro; 1,04-2,27 in scuro): è voluto, serve a non disturbare la lettura; la presenza dell'evidenziazione è garantita dalla linea di base (≥ 4,1) e dal testo letto da TalkBack, quindi non dipende dal solo colore. Le coppie `ink` sul campione servono anche per la spunta nella tavolozza (icona ≥ 3 richiesto).

## 11. Sincronizzazione e errori

Principio come `lesson.md` §8.1: **registrare non è un passo che l'utente aspetta, né un motivo per fermarlo**. Il segno è sempre disegnato subito dall'insieme locale (che è la verità finché ha modifiche non inviate, `lessons.md` §10).

| Aspetto | Regola |
|---|---|
| Persistenza locale | l'insieme per lezione e un flag `dirty` si salvano su disco a ogni modifica: sopravvivono a kill e riavvio |
| Quando si invia | `PUT /lessons/{id}/highlights` con l'insieme intero, **1,5 s** dopo l'ultima modifica (`syncDebounceMs`: più tratti ravvicinati = una chiamata), subito alla chiusura/pausa della schermata, al ritorno della rete e all'apertura della lezione se `dirty` |
| Fallimento (rete, 5xx) | **nulla di visibile.** Nuovi tentativi con attesa 5 s, 15 s, 60 s, poi a ogni ripresa in primo piano e al ritorno della rete; l'insieme resta `dirty` |
| Indicatore | solo se i tentativi falliscono da più di **30 s** (`syncIndicatorAfterMs`): compare l'icona `cloud-off` già prevista nella TopBar (`lesson.md` §8.1; un solo indicatore per tutto ciò che è in coda), e nel suo popover si aggiunge la riga «Alcune evidenziazioni non sono ancora salvate. Le invio appena torna la connessione.» con «Riprova ora». A sincronizzazione riuscita l'icona sparisce (annuncio Polite «Evidenziazioni salvate.»). Mai un banner o un dialog |
| `422` | bug dell'app o documento cambiato: si legge `details[].field` (`highlights[i]`), si **scarta localmente solo la voce i** e si reinvia (al massimo 3 volte); si registra nel log; l'utente non vede nulla |
| `404` (lezione eliminata) | si smette di inviare; se la lezione è aperta vale `lesson.md` §8.1 «Questa lezione non esiste più.» |
| `401` | flusso di login generale |
| Apertura della lezione | se **non** `dirty`: vale `LessonView.highlights` (sostituisce quanto in cache, senza animazione). Se `dirty`: vale il locale e si reinvia. Nessun merge fra dispositivi: vince l'ultimo invio (limite accettato, ADR 0011) e nessun avviso |
| Lezione `completed` | si evidenzia come prima (ADR 0011 p. 8) |

## 12. Accessibilità

- **Evidenziazione annunciata**: il nodo di accessibilità del paragrafo/callout contiene il testo con, **prima di ogni evidenziazione**, «evidenziato in giallo:» (colore in italiano: giallo, verde, blu, rosa) e **dopo** «fine evidenziato.», in modo che il fatto non dipenda dalla vista né dal colore. Esempio: «Quando lo sposti, evidenziato in giallo: la prima variabile non è più utilizzabile, fine evidenziato. Il compilatore…». Termini e citazioni dentro l'evidenziazione si annunciano come sempre (`paragraph.md`). Non si usano `liveRegion`.
- **`stateDescription`** del blocco, solo se ha evidenziazioni: «1 evidenziazione» / «n evidenziazioni».
- **Azioni custom** del nodo (menu azioni di TalkBack), una per ogni evidenziazione del blocco, senza tetto (il menu scorre): «Rimuovi evidenziazione: <prime 6 parole>…». Eseguita: rimozione + toast «Evidenziazione rimossa» con «Annulla» come in §8. È l'unico modo non visivo di togliere; **creare** un'evidenziazione senza vista non è nell'ambito di questa versione (vedi Aperto).
- **Colore non unico indizio**: il riquadro ha riempimento + linea di base 2 dp (due canali visivi); la presenza è anche nel testo letto. Il *colore* è una scelta dell'utente, mai un significato dato dall'app, quindi nessuna informazione obbligatoria dipende dal distinguere i quattro colori; per chi li distingue male, il nome è letto da TalkBack e i quattro riempimenti differiscono in luminosità e tinta (giallo ≠ blu per luminanza relativa).
- **Tavolozza**: `RadioGroup`; ogni campione `role = RadioButton`, `contentDescription` «Giallo», `selected` e `stateDescription` «scelto». Il gruppo: «Colore dell'evidenziatore». Target 48 dp. Non appare da sola per TalkBack-senza-penna: chi la usa la vede con il tocco della penna.
- **Popover «Rimuovi»**: 48 dp; focus iniziale sul bottone; Back lo chiude.
- **Font al 200 %**: il riquadro segue l'altezza della riga (`lineHeight − 4 dp`, in dp scalate con il font), l'aggancio funziona sulle righe a qualunque scala. La tavolozza non scala (dp), il suo testo non esiste (solo icona e nome).
- **Movimento**: l'unico effetto è la dissolvenza di 100-300 ms; con «riduci animazioni» tutto è istantaneo.
- **Tema scuro**: stessi ruoli, valori scuri (§10); il testo resta `ink` chiaro.
- **Tastiera fisica**: non evidenzia (la penna è lo strumento deciso).

## 13. Per android-dev (riassunto)

- Disegno: un `Modifier.drawBehind` / `TextLayoutResult` del testo; i riquadri si calcolano con `getBoundingBox`/`getLineTop` per ogni frammento di riga, più ritaglio di 2 dp. Il paragrafo e il callout rimangono `InlineMarkdown` condivisi; l'evidenziazione si applica come strato aggiuntivo del layout, non come `SpanStyle`, per i token di §3.1 e per sovrascrivere lo sfondo del codice inline.
- Input: `pointerInput` con `PointerEventPass.Initial`, `pointerType`: `Stylus`, `Eraser`; il tasto della penna si legge dal `MotionEvent` nativo (`BUTTON_STYLUS_PRIMARY` in `buttonState`). Consuma i down di penna dentro il testo evidenziabile così non scatta la selezione.
- Logica pura (testabile senza UI): (testo mostrato ↔ sorgente) → parole/unità; tratto → intervallo; fusione/sovrascrittura/divisione/gomma; normalizzazione ai marcatori; conversione verso `Highlight` del contratto.
- Token: `tokens.json` → `color.*.highlight*` e `highlight.*`.

## 14. Per il Generator (linee guida)

L'evidenziazione è dell'utente: il Generator non la crea né la cita. Valgono:
- **Nessun testo che inviti a evidenziare** («evidenzia questo»): non è nel catalogo e non funziona senza penna.
- Una frase-chiave all'inizio del paragrafo (`paragraph.md`) è anche la più facile da segnare: la struttura già raccomandata aiuta chi evidenzia.
- Niente `**grassetto**` extra «per far evidenziare»: il grassetto resta rado (0-2 per paragrafo).
- Il corpo del callout è evidenziabile, il titolo no: non nascondere nel titolo informazioni che servono solo lì.
- Mai testo importante *solo* in codice inline o diagrammi: non sono evidenziabili (codice inline sì dentro un paragrafo; blocchi `code` e `diagram` no).

## Contenuti al limite e casi particolari

- Paragrafo da 1200 caratteri: nessun limite particolare; fino a 500 evidenziazioni per lezione (§5).
- Parola lunghissima (URL) che va a capo con `LineBreak.Simple`: un solo frammento per riga, l'unità resta la parola.
- Testo con `Hyphens.Auto` su `lang` ≠ `it`: nessun trattino, nessuna differenza di comportamento.
- Blocco dentro un `fallback`: evidenziabile come gli altri.
- Lezione sostituita/eliminata: le evidenziazioni spariscono con lei (ADR 0011).
- Evidenziazione che dopo un aggiornamento del renderer cade su un intervallo senza caratteri mostrati: si ignora, senza errore (`lessons.md` §10).
