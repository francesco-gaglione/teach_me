# Lettura ad alta voce (schermata Lezione e Impostazioni)

> ADR 0013, contratto `docs/contracts/audio.md` §4 e §6. Estende `lesson.md` (parte della §3.2 «Dove stanno le azioni») e `settings.md`. Non è un blocco SDUI. Tablet orizzontale per primo, poi compact.

## 0. Principi

- **L'ascolto è un'alternativa o un accompagnamento della lettura, mai un'interruzione.** Niente avvio automatico, niente suono non richiesto, niente animazioni che tirano l'occhio (carico cognitivo, Sweller; coerenza, Mayer).
- **Lettura e voce insieme sono ammesse**: la lezione è testo statico e il ritmo è dell'utente. Perciò **niente evidenziazione parola per parola** («karaoke»): sposterebbe l'attenzione dal capire al seguire il cursore (effetto di ridondanza/attenzione divisa) e confonderebbe i segni.
- **Gli esercizi non si leggono mai** (non sono nel set del contratto): ascoltare domande e risposte prima di rispondere sostituirebbe il richiamo (Bjork). Il codice non si legge.
- **Significato mai solo colore, mai solo icona**: stati con icona + testo (a11y, nome accessibile).

## 1. Controllo «Ascolta» per blocco

**Posizione**: sotto il blocco, allineato a fine riga: lontano dall'inizio delle righe, non compete con la lettura. **Aspetto**: il testo «Ascolta» ripetuto sotto ogni paragrafo è rumore (contiguità sì, ripetizione no).

Visibile **su tutti i blocchi leggibili** (non solo al focus: su touch non c'è hover, e un controllo nascosto non si scopre), ma **silenzioso**: solo icona nello stato a riposo.

| Blocco | Posizione | Contrasto con il resto |
|---|---|---|
| `paragraph`, `callout` | riga propria sotto il blocco, a fine riga, **48 × 48 dp** (icona 20 dp). La riga **sostituisce** 16 dp dello spazio fra blocchi (costo netto +32 dp per blocco) | non tocca il testo né i callout (sta fuori dal riquadro) |
| `heading` (h2/h3) | **in linea**, a fine riga del titolo, 48 × 48 dp, centrato sulla riga; nessuna riga in più. h1 e `ObjectiveLine`: come h2 | il titolo lascia 48 dp a destra (a capo se serve) |
| `diagram` | sotto il diagramma, a fine riga, come i paragrafi. Nome accessibile «Ascolta la descrizione del diagramma» | l'`alt` è già nel testo accessibile: il controllo serve a chi vede |

Stati (testi dell'app, italiano). Icona 20 dp. Tutti 48 dp di altezza minima.

| Stato | Icona · etichetta | Colore | Nome accessibile / stateDescription |
|---|---|---|---|
| **A riposo** (`ready`, o clip di un'altra voce) | `volume-2` (nuova) · solo icona | `inkSoft` | «Ascolta» (+ «il paragrafo» / «il titolo» / «la descrizione del diagramma») |
| **In riproduzione** | `square` (nuova) · «Ferma» (`ui.label`) | icona e testo `accent` | «Ferma la lettura», stato «In lettura» |
| **In preparazione** (`pending` senza clip) | `clock` · solo icona | `inkSoft` | «Ascolta, in preparazione», disabilitato (`enabled=false`) |
| **Non disponibile** (`failed` senza clip) | `minus-circle` · «Audio non disponibile» | `inkSoft` | «Audio non disponibile per questo blocco», disabilitato |

- «Ferma» e «Audio non disponibile» hanno il **testo** perché sono eventi rari o in corso: servono parole, non silenzio. «In preparazione» sta a parole **una sola volta** nel controllo della lezione (§3): ripeterlo su 30 blocchi riempirebbe la pagina di avvisi. Per questi due stati disabilitati si usa `inkSoft` e non `disabledInk` (informazione da leggere: `inkSoft` su `bg` ≥ 9; `disabledInk` non raggiunge 4,5:1).
- Pressed `surfacePressed` (forma `full`); focus anello 3 dp; tastiera: Tab, Invio/Spazio.
- Avviare un blocco ferma quello in corso (contratto §6). Cambiare stato del nome accessibile **non** annuncia nulla (niente live region: la voce di TalkBack non deve sovrapporsi all'audio).
- **Sostituisce** icona `play-circle` (richiama un video) e testo «Ascolta» sempre visibile.

## 2. Segno «ora in lettura»

**Sostituisce** sfondo `accentSoft` + barra spostata dentro il testo.

Perché: `accentSoft` come riempimento di tutto il blocco (a) è molto vicino a `highlightBlue`, (b) passa sotto le evidenziazioni dell'utente cambiandone il contrasto (ADR 0011 garantisce i rapporti solo su `bg`, `surface` e sfondi dei callout), (c) la barra dentro la colonna spingeva il testo e faceva saltare a capo le righe a lettura iniziata.

| Regola | Valore |
|---|---|
| Segno | **una sola barra verticale**, nessun riempimento |
| Colore | `accent` (chiaro `#3A4DA0`, scuro `#B9C3FF`; ≥ 3:1 su `bg`) |
| Geometria | larghezza **4 dp**, raggio `full`, altezza = altezza del blocco (titolo compreso nei callout; non include la riga del controllo). Sta **nel margine**, a **12 dp dal bordo sinistro della colonna** (da −12 a −8 dp). **La colonna non si sposta di un pixel**: nessun reflow, nessuno spostamento delle evidenziazioni |
| Compact (margine 16) | la barra occupa da 4 a 8 dp dal bordo schermo: stesso schema, la colonna non cambia |
| Entrata/uscita | dissolvenza `fast` (100 ms); con «riduci animazioni» istantanea. Non pulsa, non scorre |
| Non solo colore | il controllo del blocco mostra «Ferma» (§1) e il nodo ha `stateDescription` «In lettura» |

Distinzione dall'evidenziatore (`components/highlight.md`): le evidenziazioni sono **rettangoli sulle parole**, colori caldi/pastello (giallo, verde, azzurro, rosa) con linea di base da 2 dp; il segno di lettura è **una barra verticale a bordo pagina**, mai sulle parole, di un solo colore (`accent`). Non si può fare con la penna né con il dito e non viene salvato. Non si evidenzia il termine in corso.

## 3. «Ascolta la lezione» (lettura continua)

**Avvio**: icona `volume-2` 48 dp nella TopBar, **a sinistra dell'Indice** (ordine a destra: `cloud-off` se serve, `volume-2`, `list`), nome accessibile «Ascolta la lezione». Parte dal primo blocco leggibile visibile in cima (contratto §6). Mai un secondo bottone in fondo alla pagina: una lezione ha un solo punto di comando (P1).

| Stato del controllo | Resa |
|---|---|
| Funzione spenta o lezione senza manifest | non c'è |
| Nessun blocco pronto | `volume-2` disabilitato, nome «Ascolta la lezione, in preparazione». Toccandolo non succede nulla; in alternativa tooltip lungo-tocco «In preparazione: ci vuole qualche minuto» |
| Almeno un blocco pronto | attivo; parte dai blocchi che ha, **salta** quelli senza clip |
| Alcuni blocchi pronti | come sopra; i blocchi mancanti hanno `clock` (§1) |

**In riproduzione**: sotto la TopBar compare la **barra audio** (`AudioStrip`), 48 dp, fill `surface`, filetto `line` sotto, a tutta larghezza; contenuto dentro la colonna di lettura (padding come i margini):

```
┌──────────────────────────────────────────────────┐
│ ✕ Ownership e move                      [≡]      │ TopBar
├──────────────────────────────────────────────────┤
│ [■ Ferma la lettura]                    [ 1,25× ]│ AudioStrip 48 dp
└──────────────────────────────────────────────────┘
```

- «Ferma la lettura»: pulsante testuale con `square` + testo `ui.label` `accent`, 48 dp. Nella TopBar `volume-2` sparisce mentre la barra c'è.
- **Velocità attuale**: chip `ui.label` `inkSoft` «1,25×» (48 × 48 dp), a fine barra. **Un tocco passa alla velocità successiva** (0,75× → 1× → 1,25× → 1,5× → 0,75×), vale subito sulla clip in corso e si salva sul dispositivo (la stessa di Impostazioni). Nome accessibile «Velocità 1,25 volte, tocca per cambiare». Il chip sta solo in barra: nessuna icona in più nella TopBar.
- Barra: entra con `sectionReveal` ridotto (solo dissolvenza 200 ms), esce con `fast`. Il contenuto sotto scende di 48 dp al suo apparire: avviene una volta, subito dopo il tocco dell'utente.
- **Una lettura di un singolo blocco (§1) non mostra la barra**: basta il «Ferma» del blocco.

**Scorrimento automatico**:

- All'inizio di ogni blocco, se **non è interamente nella finestra** (sotto TopBar + barra), scorre per portarlo a **24 dp sotto la barra** (`out`, 300 ms; istantaneo con «riduci animazioni»). Se è già tutto visibile non si muove nulla (meno movimento = meno carico).
- Blocco più alto della finestra: si porta l'inizio a 24 dp e lo scorrimento successivo resta all'utente.
- **Se l'utente scorre a mano**, la lettura continua ma **lo scorrimento automatico si ferma fino alla fine di quella lettura** (non si strappa il controllo all'utente); la barra `accent` mostra dove è la voce.
- Fra una sezione e l'altra: legge solo le sezioni **già rivelate**; non rivela mai una sezione (il «Continua» è dell'utente, P3). Finito l'ultimo blocco rivelato la lettura si ferma da sola, la pagina scorre su `ContinueBar` solo se non è visibile.
- Si ferma anche (contratto §6): uscita dalla lezione, avvio di un altro blocco, app in secondo piano.

**Si ferma prima di un esercizio obbligatorio non svolto**:

1. L'audio finisce il blocco precedente; **non** inizia l'esercizio.
2. La pagina scorre per portare l'esercizio a 24 dp sotto la barra (come sopra).
3. La barra audio sparisce (`fast`) e compare un `Toast` (`base-components` §Toast, 4 s, `liveRegion=Polite`): **«Ti fermo qui: ora tocca a te rispondere.»** Nessuna azione, nessun suono di allerta.
4. Perché: gli esercizi sono richiamo (Bjork). L'audio sta fino alla soglia, poi il silenzio dice che ora serve lo sforzo. Esercizi facoltativi (`required: false`): il blocco non è leggibile e non ferma niente; la lettura salta oltre.

## 4. Impostazioni → sezione «Ascolto»

Nuova sezione di `settings.md`, **tra «Password» e «Connessione»** (frequenza d'uso: dopo gli strumenti per l'account, prima della diagnostica). **Nascosta del tutto** se `GET /tts/voices` dà `available: false` o la richiesta fallisce per rete (nessun errore, nessuna sezione vuota). Intestazione `ui.label` `inkSoft`, 24 dp sopra e 8 sotto come le altre; contenuto in una `Surface` statica.

```
Ascolto
┌────────────────────────────────────────┐
│ Velocità di lettura                    │ ui.label ink
│ ┌───────┬───────┬───────┬───────┐      │ SegmentedChoice 48 dp
│ │ 0,75× │✓ 1×   │ 1,25× │ 1,5×  │      │
│ └───────┴───────┴───────┴───────┘      │
│ Solo su questo dispositivo.            │ ui.bodyS inkSoft
│                                        │
│ Voce                                   │ ui.label ink
│ ◉ Vivian                    [▶ Prova]  │ riga 56 dp
│ ○ Serena                    [🕐 In prep]│
│ ○ …                                    │
│ Vale su tutti i tuoi dispositivi.      │ ui.bodyS inkSoft
│ Le lezioni già pronte con un'altra     │
│ voce si aggiornano alla prossima       │
│ apertura.                              │
└────────────────────────────────────────┘
```

**Velocità**: `SegmentedChoice` = `SegmentedSwitch` (`progress.md` §2) a **4 segmenti** uguali, nessun colore nuovo. Etichette «0,75×», «1×», «1,25×», «1,5×». Selezione: fill `accentSoft`, bordo `accent` 2 dp, icona `check` 18 dp + testo `ink`; gli altri `surface` + `inkSoft`. `selectableGroup`/`Role.RadioButton`, nome «Velocità 0,75 volte» ecc. Frecce ←→ cambiano. A 360 dp ogni segmento ≈ 76 dp: se il testo con icona non entra (font ≥ 1,3) i segmenti diventano **una colonna di 4 righe da 48 dp**. Si salva al tocco, senza «Salva». Predefinita 1×. Etichetta sotto: «Solo su questo dispositivo.»

**Voce**: gruppo di scelta singola (`selectableGroup`, `Role.RadioButton`), una riga da 56 dp per voce nell'ordine dell'elenco: radio 24 dp a sinistra (selezionata = `circle-dot` `accent`, altrimenti `circle` `border`; il nome in `ui.bodyL` `ink`; non solo colore: la riga selezionata ha anche fill `accentSoft`). A destra il pulsante **«Prova»** (testuale, 48 dp):

| Stato del campione | Pulsante |
|---|---|
| `sample_url` presente | `volume-2` · «Prova»; riproduce la frase fissa. Durante: `square` · «Ferma». Un solo campione alla volta; avviarne uno ferma la lettura di una lezione. **Alla velocità 1×** (si giudica la voce, non il ritmo) |
| `sample_url: null` | `clock` · «In preparazione», disabilitato (`inkSoft`); si riprova a ricaricare l'elenco a ogni apertura della schermata |
| Clip non scaricabile | `minus-circle` · «Non disponibile», disabilitato |

- **Toccare la riga sceglie la voce** (`PUT /me/tts-voice`, `users.md`); «Prova» non la sceglie: si può ascoltare prima di decidere. Si salva subito; durante la richiesta il radio passa subito alla nuova voce (`stateDescription` «Salvataggio…») e altri tocchi sono ignorati; se fallisce: Toast «Non sono riuscito a salvare la voce. Riprova.» e radio sul valore precedente.
- La nota sotto l'elenco (`ui.bodyS` `inkSoft`) dice dove vale e cosa succede alle lezioni: non promette tempi.
- Un solo elemento primario per schermata: qui non c'è nessun primario.
- Ordine TalkBack: intestazione (heading) → velocità → voce (ogni riga: «Vivian, selezionata», azione «Prova»).

## 5. Cosa deve evitare il Generator

Niente indicazioni per l'audio nel testo («come puoi ascoltare…», «leggi qui sotto»): la voce e il testo devono bastare da soli. Ma: **un `paragraph` che regge da solo se letto ad alta voce** (frasi complete, nessun «vedi la tabella a destra», nessuna dipendenza da punteggiatura visiva); `alt` dei diagrammi che descrivono la relazione, non «immagine di…»; titoli `callout` brevi (si leggono prima del corpo).

## 6. Token e icone

Token esistenti: `accent`, `accentSoft`, `inkSoft`, `ink`, `surface`, `surfacePressed`, `line`, `border`, motion `fast` e `sectionReveal`. Nessun colore nuovo. **Icone nuove (Lucide)**: `volume-2` (ascolta, voce), `square` (ferma). Riusate: `clock`, `minus-circle`, `check`, `circle`, `circle-dot`. Componenti nuovi: `AudioStrip` (§3), `SegmentedChoice` (4 voci, §4).

## 7. Aperto

- Lo scorrimento automatico sospeso da un gesto manuale non riprende finché la lettura non finisce: se in prova risulta frustrante, riprende al blocco successivo.
- Controllo da schermo bloccato / notifica multimediale: fuori scope (la lettura si ferma in secondo piano, ADR 0013 §7).
