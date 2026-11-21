# teach_me — Design system v1 (custom)

> Proprietario: `learning-designer`. Stato: **v1 rivista (2026-11-08)**: design custom, **senza Material 3 né Material You**. I componenti si costruiscono da zero su Compose `foundation` (non si usa `androidx.compose.material3`).
> Companion: `tokens.json` (stessi valori), `base-components.md` (componenti base), `screens/*.md`, `tools/contrast_check.py`.
> Il design system serve a una cosa: **che l'utente impari e ricordi**. Ogni scelta è motivata da un principio della §1; dove è un compromesso, lo dico.

## 0. Struttura di `tokens.json`

Se divergono, ha ragione `tokens.json` e il documento è da correggere.

```
tokens.json
├── version                  2
├── color
│   └── light | dark
│       └── <ruolo>          ruoli custom (§2), piatti, camelCase, "#RRGGBB"
├── highlight                misure e soglie dell'evidenziatore (components/highlight.md); i 4+4 colori stanno in color.*.highlight*
├── handwriting              scrittura a mano: strumenti, spessori, gomma, limiti (components/handwriting.md); i colori stanno in color.*.pen*
├── colorMeta                overlayOpacity (0.4)
├── typography
│   ├── fontFamilies.{ui,reading,code}   nome, fallback, fonte
│   ├── ui.<stile>           display, title1-3, bodyL, body, bodyS, labelL, label, labelS
│   ├── reading.<stile>      body, bodyStrong, question, cardFront (serif, contenuto da studiare)
│   └── code.<stile>         block, inline
│                            ogni stile: fontFamily (chiave di fontFamilies), fontWeight,
│                            fontSize (sp), lineHeight (sp), letterSpacing (sp)
├── spacing.*                dp, scala 4
├── layout.*                 dp: margini, larghezze massime, target minimo, altezze controlli, iconStroke
├── shape.*                  dp: none, sm 6, md 10, lg 16, full
├── stroke.*                 dp: hairline, control, selected, focus, focusOffset
├── elevation.*              flat, raised, overlay: ombre {y, blur, alpha*, color}
└── motion
    ├── duration.*           ms: instant, fast, base, slow, slower
    ├── easing.*             [x1, y1, x2, y2] (cubic-bezier)
    ├── semantic.*           {duration, easing} per i casi d'uso
    └── reducedMotion        regola per "riduci animazioni"
```

Traduzione in Compose (indicativa):
- `color.*` → una `data class TeachColors` (un campo per ruolo) esposta con `staticCompositionLocalOf`, due istanze (light/dark); `MaterialTheme` non si usa. Accesso: `TeachTheme.colors.ink`.
- `typography.*` → `data class TeachTypography` con `TextStyle` per stile, in un CompositionLocal. `fontFamilies` → `FontFamily(Font(R.font.inter, ...))`; il fallback (`FontFamily.SansSerif/Serif/Monospace`) vale se il caricamento fallisce.
- `shape`, `spacing`, `layout`, `stroke` → oggetti con `Dp`. `elevation` → ombre disegnate con `Modifier.shadow` o `drawBehind`. `motion.easing` → `CubicBezierEasing`; `duration` → `tween`.
- Le interazioni (press, focus) si implementano con `Modifier.clickable(indication = null, interactionSource = …)` + `collectIsPressedAsState()`, usando i token di stato (§4), **senza ripple Material**.

## 1. Principi, con la loro base

| # | Principio | Implicazione | Base |
|---|---|---|---|
| P1 | **Un'idea per schermata/sezione.** | Lezione a sezioni, una scheda per volta, niente badge o contatori vicino al contenuto. | Carico cognitivo (Sweller): la memoria di lavoro è limitata; il carico estraneo va eliminato. |
| P2 | **Coerenza e segnalazione.** | Niente illustrazioni decorative. Il `mark` (evidenziatore) è raro. Una sola tinta d'accento. | Principi di Mayer; Tufte (rapporto dati/inchiostro). |
| P3 | **Segmentazione al ritmo dell'utente.** | Sezioni rivelate con "Continua" sotto il controllo dell'utente, nessun timer. | Mayer: segmentazione. |
| P4 | **Contiguità.** | Feedback sotto la domanda; termini del glossario in uno sheet senza cambiare schermata. | Mayer: contiguità spaziale/temporale. |
| P5 | **Prima richiama, poi rivela.** | Risposta nascosta finché l'utente non ha tentato; nessuna anteprima. | Retrieval practice e difficoltà desiderabili (Bjork; Roediger e Karpicke). |
| P6 | **Feedback immediato, specifico, spiegato.** L'errore è informazione. | Ogni esercizio spiega perché è giusto o sbagliato. | Hattie e Timperley; ipercorrezione (Butterfield e Metcalfe). |
| P7 | **Nessun indizio involontario.** | Opzioni identiche per stile, forma e peso; nessun colore o movimento prima della risposta. | Validità del test. |
| P8 | **Significato mai al solo colore.** | Esito = colore + icona + parola. | WCAG 2.2 AA 1.4.1. |
| P9 | **Motivazione intrinseca, niente manipolazione.** | Niente streak, punti, badge vuoti, colori da "game over". Il progresso è di apprendimento. | Autodeterminazione (Deci e Ryan); overjustification. |
| P10 | **Leggibilità prolungata.** | Serif per i testi lunghi, interlinea 1,65, paragrafi AAA, codice monospace ≥ 7:1. | WCAG 1.4.6 e 1.4.12. |
| P11 | **Dual coding con parsimonia.** | Un'icona accompagna ogni stato e callout; mai decorazione. | Paivio. |
| P12 | **Pre-training.** | I termini si presentano prima dei concetti che li usano. | Mayer. |
| P13 | **Impegno obbligatorio ma non punitivo.** | L'esercizio di una sezione va svolto per proseguire, ma si prosegue anche dopo un errore; il blocco è sempre spiegato. | Testing effect: richiamare è l'atto che fissa; bloccare fino alla risposta *giusta* incentiva il tentativo a caso (vedi `screens/lesson.md`). |

## 2. Linguaggio visivo "carta e inchiostro"

Identità propria, pensata per lo studio. Le regole:

1. **Superfici piatte e bordi, non ombre.** Il contenuto vive su carta calda (`bg`); le card sono `surface` con bordo `line` 1 dp (decorativo) e **nessuna elevazione**. L'ombra (`raised`, `overlay`) esiste solo per ciò che galleggia sopra il contenuto (dialog, sheet, toast). Niente elevazione tonale, niente tinte che cambiano con l'altezza.
2. **Inchiostro forte, accento unico.** Testo `ink` quasi nero su carta; un solo accento indaco (`accent`) per azione, selezione, avanzamento e link. Teal (`knowledge`) significa "conoscenza": termini e fonti.
3. **I controlli si vedono perché hanno un bordo**: ogni controllo interattivo a riposo ha un bordo `border` (≥ 3:1) di 1,5 dp, raggio `md` (10 dp). Il bottone primario è l'unico pieno. Questo rende i quiz privi di indizi (tutte le opzioni sono identiche) e i target riconoscibili anche in scala di grigi.
4. **Forma**: angoli moderati (6/10/16 dp), nessuna pillola per i controlli di contenuto; la pillola (`full`) è riservata a chip di stato e barre di avanzamento. Distinto dalla forma tutta tondeggiante di M3.
5. **Densità**: generosa nel contenuto (blocchi a 24 dp, opzioni da 56 dp), compatta nei meta-dati. Target sempre ≥ 48 dp.
6. **Pressione**: lo stato premuto è un **cambio di riempimento immediato** (`surfacePressed`, `accentPressed`) in 100 ms, senza ripple a onda né scala. Più sobrio, più prevedibile, nessun movimento da leggere come indizio.
7. **Iconografia**: set a **tratto** (outline), griglia 24 dp, **tratto 1,75 dp**, estremità e giunzioni arrotondate, mai riempite tranne gli stati di esito (spunta/X/trattino dentro cerchio, sempre outline + glifo). Set consigliato: **Lucide** (licenza ISC) esportato in `VectorDrawable`; le icone sono scelte per significato, non decorazione. Nomi Lucide usati: `check-circle` (corretto), `x-circle` (non proprio), `minus-circle` (quasi), `info`, `lightbulb`, `alert-triangle`, `alert-circle` (errore di sistema), `lock`, `key-round`, `wallet` (credito esaurito), `cloud-off`, `loader` (indeterminato), `chevron-right`, `chevron-left`, `x`, `settings`, `folder`, `eye`, `eye-off`, `book-open` (glossario), `link`, `plus`, `more-vertical`, `arrow-up`.
8. **Motion**: orienta, non decora (§8).

### 2.1 Strategia colore

- **Carta calda** (`#FBFAF8`, non bianco puro) in light; grigio quasi-neutro (`#121316`, non nero) in dark. Le card sono più chiare dello sfondo in light (`#FFFFFF`) e più chiare in dark (`#1A1B1F`): in entrambi i casi la card "emerge" per bordo e leggero scarto di tinta.
- **Giusto / sbagliato / parziale** hanno ruoli propri; il rosso (`danger`) è solo per guasti di sistema. Una risposta sbagliata è **terracotta** (`incorrect`): caldo e non allarmante (P6, P9).
- **Nessuna palette dinamica**: i contrasti sono verificati sulla palette fissa.
- Tema chiaro/scuro seguono il sistema. Il dark non è un'inversione: accenti più chiari e meno saturi per evitare vibrazioni.

### 2.2 Ruoli colore (light / dark)


**Superfici**

| Ruolo | Light | Dark | Uso |
|---|---|---|---|
| `bg` | `#FBFAF8` | `#121316` | sfondo dell'app e delle lezioni (carta calda) |
| `surface` | `#FFFFFF` | `#1A1B1F` | card, righe, blocchi di contenuto |
| `surfaceSunken` | `#F1EFEB` | `#0D0E11` | campi di testo, aree incassate |
| `surfaceRaised` | `#FFFFFF` | `#25262A` | dialog, sheet, menu (con ombra `raised`) |
| `surfacePressed` | `#ECEAE5` | `#2A2B30` | riempimento di righe/bottoni secondari premuti |

**Testo e linee**

| Ruolo | Light | Dark | Uso |
|---|---|---|---|
| `ink` | `#1B1B1F` | `#E4E2E6` | testo principale e dei paragrafi (AAA) |
| `inkSoft` | `#454650` | `#C6C5D0` | testo secondario, etichette |
| `inkMuted` | `#5C5D68` | `#A3A4B0` | placeholder, meta-dati |
| `line` | `#E3E1DC` | `#2E2F35` | separatori decorativi (nessun significato) |
| `border` | `#7C7D88` | `#8B8C97` | bordo dei controlli interattivi (>= 3:1) |

**Accento**

| Ruolo | Light | Dark | Uso |
|---|---|---|---|
| `accent` | `#3A4DA0` | `#B9C3FF` | azione primaria, link, selezione, avanzamento |
| `accentPressed` | `#2C3C85` | `#A3AFF5` | bottone primario premuto |
| `onAccent` | `#FFFFFF` | `#1B2C80` | testo/icona su accent |
| `accentSoft` | `#E3E6FF` | `#2A3678` | selezione, chip attivo |
| `accentSoftPressed` | `#D3D8FA` | `#34428A` | selezione premuta |

**Conoscenza**

| Ruolo | Light | Dark | Uso |
|---|---|---|---|
| `knowledge` | `#0B6A6B` | `#8ED2D2` | termini del glossario e fonti (teal) |
| `knowledgeSoft` | `#D5F0EF` | `#1B4A4B` | sfondo dello sheet di un termine |

**Esito e callout**

| Ruolo | Light | Dark | Uso |
|---|---|---|---|
| `correct` | `#1F6B3A` | `#8FD6A0` | 'giusto': icona, etichetta, bordo |
| `correctSoft` | `#DAF1DF` | `#1B4A2B` | sfondo feedback 'giusto' |
| `incorrect` | `#9C4A12` | `#F2B48A` | 'sbagliato' (terracotta, non rosso) |
| `incorrectSoft` | `#FBE6D6` | `#5A2E10` | sfondo feedback 'sbagliato' |
| `partial` | `#7A5900` | `#EBC15A` | 'parziale' |
| `partialSoft` | `#FFEFC2` | `#5A4200` | sfondo parziale |
| `info` | `#245D9E` | `#A5C8F5` | callout info / stato in corso |
| `infoSoft` | `#DCEBFB` | `#1E4676` | sfondo info |
| `tip` | `#2B6A2E` | `#A6D79F` | callout tip |
| `tipSoft` | `#E1F0D8` | `#254A28` | sfondo tip |
| `warning` | `#8A4B00` | `#F0B070` | callout warning, problemi correggibili dall'utente |
| `warningSoft` | `#FFE8CC` | `#5E3500` | sfondo warning |

**Errore di sistema**

| Ruolo | Light | Dark | Uso |
|---|---|---|---|
| `danger` | `#A8322B` | `#FFB4A9` | errore di sistema (guasto) |
| `dangerSoft` | `#FBE0DC` | `#5A2420` | sfondo errore di sistema |

**Stati e overlay**

| Ruolo | Light | Dark | Uso |
|---|---|---|---|
| `disabledFill` | `#E9E7E3` | `#26272B` | sfondo disabilitato |
| `disabledInk` | `#8E8F99` | `#6E6F7A` | contenuto disabilitato (esente WCAG) |
| `overlay` | `#000000` | `#000000` | oscuramento sotto dialog/sheet (opacita' 0.4) |
| `toastBg` | `#303034` | `#E4E2E6` | toast |
| `toastInk` | `#F2F0F4` | `#303034` | testo del toast |
| `toastAccent` | `#B9C3FF` | `#3A4DA0` | azione del toast |
| `focus` | `#2250A8` | `#9DBDF5` | anello di focus (3 dp) |

**Codice e segnalazione**

| Ruolo | Light | Dark | Uso |
|---|---|---|---|
| `codeBg` | `#F1EFEB` | `#1A1B20` | sfondo blocchi di codice |
| `codeInk` | `#24252B` | `#E0E0E6` | testo del codice |
| `codeLine` | `#C8C7D0` | `#454650` | bordo del blocco di codice (decorativo) |
| `codeLineHighlight` | `#E2E0F2` | `#2A2D4A` | riga evidenziata |
| `codeKeyword` | `#7A2E9E` | `#D7A8F0` | sintassi: keyword |
| `codeString` | `#1F6B3A` | `#9FD8A8` | sintassi: string |
| `codeComment` | `#5E606B` | `#A0A2AE` | sintassi: comment |
| `codeNumber` | `#9C4A12` | `#F2B48A` | sintassi: number |
| `codeFunction` | `#2250A8` | `#9DBDF5` | sintassi: function |
| `codeType` | `#0B6A6B` | `#8ED2D2` | sintassi: type |
| `codePunctuation` | `#4A4B55` | `#B4B5C0` | sintassi: punctuation |
| `mark` | `#FFE79A` | `#574709` | evidenziatore di frasi chiave (segnalazione) |
| `onMark` | `#1B1B1F` | `#FFF6D6` | testo su mark |
| `highlightYellow` / `highlightGreen` / `highlightBlue` / `highlightPink` | `#FFE066` / `#B8EEC2` / `#C2DFFF` / `#FFD0E2` | `#5E4B00` / `#1F5A32` / `#1F4F8C` / `#7A2A52` | riempimento dell'evidenziazione fatta dall'utente con la penna (`components/highlight.md`); non è il `mark` |
| `highlightYellowLine` / `…GreenLine` / `…BlueLine` / `…PinkLine` | `#8A6A00` / `#1F7A3A` / `#1F5FB0` / `#B0306A` | `#E5C03A` / `#6FCB8A` / `#7FB2F5` / `#F08AB8` | linea di base 2 dp dell'evidenziazione (≥ 3:1 su `bg`, `surface` e i tre sfondi dei callout) |
| `penInk` / `penBlue` / `penRed` / `penGreen` | `#1B1B1F` / `#1F4FB0` / `#B02E2E` / `#1F7A3A` | `#E4E2E6` / `#8FB4FF` / `#FF9E9E` / `#7ED99A` | inchiostro della penna (scrittura a mano, `components/handwriting.md` §4): ≥ 3:1 su ogni sfondo di lettura e su tutte le evidenziazioni (minimi 3,92 chiaro, 3,95 scuro). Il tratto salva il ruolo (`ink`/`blue`/`red`/`green`), non l'hex |
| `penOn` | `#FFFFFF` | `#121316` | spunta sul campione d'inchiostro nella tavolozza (≥ 6,4:1) |


Ogni stato semantico ha **due** colori: `x` (accento: icona, etichetta, bordo; leggibile anche come testo) e `xSoft` (sfondo). Il **corpo del testo** su un `xSoft` è sempre `ink` (≥ 7:1).

### 2.3 Mappatura semantica

| Significato | Colori | Icona (Lucide) | Parola |
|---|---|---|---|
| Risposta giusta | `correct` / `correctSoft` | `check-circle` | "Corretto" |
| Risposta sbagliata | `incorrect` / `incorrectSoft` | `x-circle` | "Non proprio" |
| Risposta parziale | `partial` / `partialSoft` | `minus-circle` | "Quasi" |
| Callout info | `info` / `infoSoft` | `info` | "Info" |
| Callout tip | `tip` / `tipSoft` | `lightbulb` | "Suggerimento" |
| Callout warning | `warning` / `warningSoft` | `alert-triangle` | "Attenzione" |
| Errore di sistema | `danger` / `dangerSoft` | `alert-circle` | testo del messaggio |
| Termine / fonte | `knowledge` / `knowledgeSoft` | `book-open` / `link` | sottolineatura tratteggiata |

- Le icone di esito differiscono per **forma** (spunta, X, trattino), non solo per colore.
- Etichetta "**Non proprio**" (non "Sbagliato"): descrive la risposta, non giudica la persona.
- Prima che l'utente risponda, nessun elemento di un quiz usa questi colori (P7).

### 2.4 Contrasti calcolati

Metodo: luminanza relativa sRGB WCAG 2.x, rapporto `(L1 + 0.05) / (L2 + 0.05)`. Soglie: testo normale ≥ 4.5; **componenti UI e bordi significativi ≥ 3**; **paragrafi delle lezioni (e ogni `ink`) AAA ≥ 7**. Calcolato con `python3 -I docs/design/tools/contrast_check.py` (130 coppie per tema, tutte sopra soglia, exit 0). Ogni coppia usata dall'app per testo o bordi significativi è in tabella; una nuova coppia va aggiunta allo script.

| Testo / elemento | Sfondo | Light | Dark | Min | Uso |
|---|---|---|---|---|---|
| `ink` | `bg` | 16.46 | 14.44 | 7 | Testo principale su sfondo app (AAA) |
| `ink` | `surface` | 17.17 | 13.37 | 7 | Testo principale su card (AAA) |
| `ink` | `surfaceSunken` | 14.95 | 15.00 | 7 | Testo principale su campi e aree incassate (AAA) |
| `ink` | `surfaceRaised` | 17.17 | 11.75 | 7 | Testo principale su dialog / sheet (AAA) |
| `ink` | `surfacePressed` | 14.28 | 10.98 | 7 | Testo principale su riga/superficie premuta (AAA) |
| `ink` | `accentSoft` | 13.92 | 8.60 | 7 | Testo principale su selezione (AAA) |
| `ink` | `accentSoftPressed` | 12.24 | 7.12 | 7 | Testo principale su selezione premuta (AAA) |
| `inkSoft` | `bg` | 8.97 | 10.88 | 4.5 | Testo secondario su bg |
| `inkMuted` | `bg` | 6.25 | 7.51 | 4.5 | Placeholder / meta-dati su bg |
| `accent` | `bg` | 7.32 | 10.89 | 4.5 | Link / testo accento su bg |
| `knowledge` | `bg` | 6.12 | 10.88 | 4.5 | Termini del glossario / fonti su bg |
| `border` | `bg` | 3.91 | 5.57 | 3 | Bordo dei controlli su bg (non testo) |
| `focus` | `bg` | 7.24 | 9.76 | 3 | Anello di focus su bg |
| `inkSoft` | `surface` | 9.35 | 10.08 | 4.5 | Testo secondario su surface |
| `inkMuted` | `surface` | 6.52 | 6.96 | 4.5 | Placeholder / meta-dati su surface |
| `accent` | `surface` | 7.64 | 10.09 | 4.5 | Link / testo accento su surface |
| `knowledge` | `surface` | 6.39 | 10.08 | 4.5 | Termini del glossario / fonti su surface |
| `border` | `surface` | 4.08 | 5.16 | 3 | Bordo dei controlli su surface (non testo) |
| `focus` | `surface` | 7.55 | 9.04 | 3 | Anello di focus su surface |
| `inkSoft` | `surfaceSunken` | 8.15 | 11.30 | 4.5 | Testo secondario su surfaceSunken |
| `inkMuted` | `surfaceSunken` | 5.68 | 7.81 | 4.5 | Placeholder / meta-dati su surfaceSunken |
| `accent` | `surfaceSunken` | 6.65 | 11.32 | 4.5 | Link / testo accento su surfaceSunken |
| `knowledge` | `surfaceSunken` | 5.56 | 11.30 | 4.5 | Termini del glossario / fonti su surfaceSunken |
| `border` | `surfaceSunken` | 3.55 | 5.79 | 3 | Bordo dei controlli su surfaceSunken (non testo) |
| `focus` | `surfaceSunken` | 6.58 | 10.14 | 3 | Anello di focus su surfaceSunken |
| `inkSoft` | `surfaceRaised` | 9.35 | 8.85 | 4.5 | Testo secondario su surfaceRaised |
| `inkMuted` | `surfaceRaised` | 6.52 | 6.11 | 4.5 | Placeholder / meta-dati su surfaceRaised |
| `accent` | `surfaceRaised` | 7.64 | 8.86 | 4.5 | Link / testo accento su surfaceRaised |
| `knowledge` | `surfaceRaised` | 6.39 | 8.85 | 4.5 | Termini del glossario / fonti su surfaceRaised |
| `border` | `surfaceRaised` | 4.08 | 4.53 | 3 | Bordo dei controlli su surfaceRaised (non testo) |
| `focus` | `surfaceRaised` | 7.55 | 7.94 | 3 | Anello di focus su surfaceRaised |
| `accent` | `accentSoft` | 6.19 | 6.49 | 4.5 | Accento su selezione |
| `danger` | `surfaceRaised` | 6.65 | 8.89 | 4.5 | Azione distruttiva nel dialog |
| `danger` | `surfaceSunken` | 5.79 | 11.35 | 4.5 | Errore su campo incassato |
| `danger` | `surfacePressed` | 5.53 | 8.31 | 4.5 | Azione distruttiva premuta |
| `focus` | `accentSoft` | 6.12 | 5.81 | 3 | Focus su selezione |
| `onAccent` | `accent` | 7.64 | 7.16 | 4.5 | Testo del bottone primario |
| `onAccent` | `accentPressed` | 10.04 | 5.82 | 4.5 | Bottone primario premuto |
| `accent` | `surfacePressed` | 6.35 | 8.28 | 4.5 | Bottone secondario/testuale premuto |
| `ink` | `surfacePressed` | 14.28 | 10.98 | 7 | Testo su stato premuto |
| `knowledge` | `knowledgeSoft` | 5.33 | 5.78 | 4.5 | Termine su sfondo conoscenza |
| `ink` | `knowledgeSoft` | 14.34 | 7.67 | 7 | Testo su sfondo conoscenza |
| `correct` | `bg` | 6.25 | 10.88 | 4.5 | Icona/etichetta correct su sfondo |
| `correct` | `surface` | 6.52 | 10.07 | 4.5 | Icona/etichetta correct su card |
| `correct` | `correctSoft` | 5.47 | 5.97 | 4.5 | Icona/etichetta correct su correctSoft |
| `ink` | `correctSoft` | 14.41 | 7.92 | 7 | Testo corpo su correctSoft |
| `incorrect` | `bg` | 5.92 | 10.32 | 4.5 | Icona/etichetta incorrect su sfondo |
| `incorrect` | `surface` | 6.17 | 9.56 | 4.5 | Icona/etichetta incorrect su card |
| `incorrect` | `incorrectSoft` | 5.11 | 6.37 | 4.5 | Icona/etichetta incorrect su incorrectSoft |
| `ink` | `incorrectSoft` | 14.22 | 8.90 | 7 | Testo corpo su incorrectSoft |
| `partial` | `bg` | 6.18 | 10.89 | 4.5 | Icona/etichetta partial su sfondo |
| `partial` | `surface` | 6.45 | 10.08 | 4.5 | Icona/etichetta partial su card |
| `partial` | `partialSoft` | 5.64 | 5.56 | 4.5 | Icona/etichetta partial su partialSoft |
| `ink` | `partialSoft` | 15.02 | 7.37 | 7 | Testo corpo su partialSoft |
| `info` | `bg` | 6.42 | 10.77 | 4.5 | Icona/etichetta info su sfondo |
| `info` | `surface` | 6.70 | 9.98 | 4.5 | Icona/etichetta info su card |
| `info` | `infoSoft` | 5.53 | 5.55 | 4.5 | Icona/etichetta info su infoSoft |
| `ink` | `infoSoft` | 14.16 | 7.44 | 7 | Testo corpo su infoSoft |
| `tip` | `bg` | 6.28 | 11.36 | 4.5 | Icona/etichetta tip su sfondo |
| `tip` | `surface` | 6.55 | 10.52 | 4.5 | Icona/etichetta tip su card |
| `tip` | `tipSoft` | 5.51 | 6.15 | 4.5 | Icona/etichetta tip su tipSoft |
| `ink` | `tipSoft` | 14.44 | 7.81 | 7 | Testo corpo su tipSoft |
| `warning` | `bg` | 6.52 | 9.86 | 4.5 | Icona/etichetta warning su sfondo |
| `warning` | `surface` | 6.80 | 9.14 | 4.5 | Icona/etichetta warning su card |
| `warning` | `warningSoft` | 5.72 | 5.62 | 4.5 | Icona/etichetta warning su warningSoft |
| `ink` | `warningSoft` | 14.44 | 8.22 | 7 | Testo corpo su warningSoft |
| `danger` | `bg` | 6.38 | 10.93 | 4.5 | Icona/etichetta danger su sfondo |
| `danger` | `surface` | 6.65 | 10.12 | 4.5 | Icona/etichetta danger su card |
| `danger` | `dangerSoft` | 5.32 | 7.23 | 4.5 | Icona/etichetta danger su dangerSoft |
| `ink` | `dangerSoft` | 13.73 | 9.56 | 7 | Testo corpo su dangerSoft |
| `toastInk` | `toastBg` | 11.61 | 10.21 | 4.5 | Testo del toast |
| `toastAccent` | `toastBg` | 7.70 | 5.94 | 4.5 | Azione del toast |
| `codeInk` | `codeBg` | 13.30 | 13.08 | 7 | Codice base |
| `codeInk` | `codeLineHighlight` | 11.77 | 10.16 | 7 | Codice su riga evidenziata |
| `codeKeyword` | `codeBg` | 6.74 | 8.80 | 4.5 | Sintassi keyword |
| `codeKeyword` | `codeLineHighlight` | 5.97 | 6.83 | 4.5 | Sintassi keyword su riga evidenziata |
| `codeString` | `codeBg` | 5.67 | 10.53 | 4.5 | Sintassi string |
| `codeString` | `codeLineHighlight` | 5.02 | 8.18 | 4.5 | Sintassi string su riga evidenziata |
| `codeComment` | `codeBg` | 5.44 | 6.77 | 4.5 | Sintassi comment |
| `codeComment` | `codeLineHighlight` | 4.81 | 5.26 | 4.5 | Sintassi comment su riga evidenziata |
| `codeNumber` | `codeBg` | 5.38 | 9.55 | 4.5 | Sintassi number |
| `codeNumber` | `codeLineHighlight` | 4.76 | 7.42 | 4.5 | Sintassi number su riga evidenziata |
| `codeFunction` | `codeBg` | 6.58 | 9.03 | 4.5 | Sintassi function |
| `codeFunction` | `codeLineHighlight` | 5.82 | 7.01 | 4.5 | Sintassi function su riga evidenziata |
| `codeType` | `codeBg` | 5.56 | 10.07 | 4.5 | Sintassi type |
| `codeType` | `codeLineHighlight` | 4.92 | 7.82 | 4.5 | Sintassi type su riga evidenziata |
| `codePunctuation` | `codeBg` | 7.53 | 8.44 | 4.5 | Sintassi punctuation |
| `codePunctuation` | `codeLineHighlight` | 6.66 | 6.56 | 4.5 | Sintassi punctuation su riga evidenziata |
| `onMark` | `mark` | 14.02 | 8.40 | 7 | Testo evidenziato |
| `ink` | `highlightYellow` / `Green` / `Blue` / `Pink` | 13.17 / 13.12 / 12.50 / 12.57 | 6.57 / 6.36 / 6.39 / 7.16 | 4.5 | Testo sull'evidenziazione dell'utente (dettaglio, link, citazioni e linee in `components/highlight.md` §10) |
| `focus` | `codeBg` | 6.58 | 9.03 | 3 | Focus su codice |


Non raggiungono 3:1, per scelta:
- `line` (separatori e bordi delle card) e `codeLine`: **decorativi**, nessun significato vi dipende (WCAG 1.4.11 li esclude). I confini interattivi usano `border`.
- **Disabilitato**: `disabledInk` su `disabledFill`, esente da WCAG 1.4.3. Ogni controllo disabilitato spiega comunque il motivo con un testo vicino (non affidare l'informazione al grigio).

### 2.5 Colori della sintassi

Sette ruoli ≥ 4.5:1 su `codeBg` e su `codeLineHighlight`. La sintassi non porta significato esclusivo: il codice resta leggibile in scala di grigi (parole chiave semibold, commenti in corsivo). I commenti sono più tenui ma ≥ 4.5: nel codice didattico spiegano.

## 3. Tipografia

### 3.1 Famiglie

| Ruolo | Font | Perché | Fallback |
|---|---|---|---|
| **reading** (corpo lezioni, domande, fronte schede) | **Literata** (Google Fonts, OFL) | Disegnato per la lettura su schermo; ottimi accenti italiani. | `FontFamily.Serif` |
| **ui** (titoli, bottoni, etichette, feedback) | **Inter** (OFL) | Sans neutro e leggibile a piccole dimensioni; molti pesi. | `FontFamily.SansSerif` |
| **code** | **JetBrains Mono** (OFL) | Distinzione chiara fra `0/O`, `1/l/I`; **ligature disattivate** (`fontFeatureSettings = "liga 0, calt 0"`): sul codice didattico nascondono i caratteri reali. | `FontFamily.Monospace` |

Approvvigionamento: file `.ttf` variabili in `res/font` (OFL, ~1-2 MB), non i downloadable fonts (nessuna dipendenza da GMS o rete). Pesi: Inter 400/600/700, Literata 400/500/600, JetBrains Mono 400/600. Licenze elencate nelle Impostazioni.

Serif per il contenuto, sans per l'interfaccia: separa "ciò che studio" da "ciò che uso" (segnalazione, P2).

### 3.2 Scala (sp / interlinea sp)

| Stile | Famiglia · peso | sp/lh | Dove |
|---|---|---|---|
| `ui.display` | Inter 600 | 32/40 | titolo del riepilogo |
| `ui.title1` | Inter 700 | 26/34 | titolo schermata grande, titolo lezione |
| `ui.title2` | Inter 600 | 21/28 | titolo barra in alto, heading di sezione |
| `ui.title3` | Inter 600 | 18/26 | titolo di card e righe, sotto-sezione |
| `ui.bodyL` | Inter 400 | 16/24 | testo UI, opzioni di quiz, spiegazione del feedback, campi di testo |
| `ui.body` | Inter 400 | 14/20 | testo UI secondario |
| `ui.bodyS` | Inter 400 | 12/16 | meta-dati, supporting text, didascalie |
| `ui.labelL` | Inter 700 | 16/24 | etichetta di esito ("Corretto") |
| `ui.label` | Inter 600 | 14/20 | bottoni, tab |
| `ui.labelS` | Inter 600 | 12/16 | chip, badge (minimo assoluto: **nessun testo < 12 sp**) |
| `reading.body` | Literata 400 | **17/28** (1,65) | paragrafi |
| `reading.bodyStrong` | Literata 600 | 17/28 | grassetto inline |
| `reading.question` | Literata 500 | 18/28 | testo della domanda |
| `reading.cardFront` | Literata 500 | 24/34 | fronte/retro delle schede |
| `code.block` | JetBrains Mono 400 | 14/22 | blocchi di codice |
| `code.inline` | JetBrains Mono 400 | 15/28 | codice nel paragrafo (interlinea = body: la riga non si sposta) |

### 3.3 Misura e lettura

- **Larghezza massima del testo di lettura: 600 dp** (`readingMaxWidth`), centrata. Con Literata 17 sp (carattere medio ≈ 0,5 em ≈ 8,5 dp, **stima da verificare con una misura sul dispositivo**) sono ≈ 70 caratteri per riga: dentro 45-75.
- **Telefono in verticale** (360 − 2 × 16 = 328 dp): ≈ 38-40 caratteri, sotto 45. Compromesso accettato: su schermo stretto un corpo ≥ 17 sp conta più della misura, e i paragrafi sono brevi. Se stanca, prima si prova 16 sp; **non** si riduce l'interlinea.
- Interlinea del corpo 1,65 (≥ 1,5 WCAG 1.4.12); spazio fra blocchi di testo 24 dp.
- **Allineamento a sinistra, mai giustificato**; `hyphens = None`; `lineBreak = Paragraph`.
- Codice: mai a capo automatico, scroll orizzontale con ombra sul bordo, testo selezionabile.
- Tutto in `sp`, funzionante fino a **200%**; altezze `min`, mai fisse.

## 4. Stati dei componenti (regole comuni)

Tabella di tutti i controlli in `base-components.md`. Regole trasversali:

| Stato | Regola |
|---|---|
| **default** | riempimento e bordo del componente |
| **pressed** | riempimento al token `*Pressed` in 100 ms (`press`), nessuna scala, nessun ripple |
| **focus** | anello `focus` di `stroke.focus` (3 dp) a `focusOffset` (2 dp) fuori dal bordo, forma uguale al componente; visibile solo con tastiera/switch/D-pad, non al tocco |
| **disabled** | riempimento `disabledFill`, contenuto `disabledInk`, bordo assente; non focalizzabile se non c'è un motivo da spiegare (vedi i blocchi "soft-lock" di `screens/lesson.md`) |
| **error** | bordo `danger` 2 dp **+ icona `alert-circle` + testo** nel supporting text, colore `danger`; mai solo il bordo |
| **selected** | riempimento `accentSoft`, bordo `accent` 2 dp, indicatore (radio/check) pieno |

## 5. Spacing e layout

| Token | dp | Uso |
|---|---|---|
| `xxs` | 4 | icona–testo stretto |
| `xs` | 8 | elementi correlati, distanza minima fra target |
| `sm` | 12 | padding verticale opzioni, gap domanda–feedback |
| `md` | 16 | margine schermo (compact), padding delle card |
| `lg` | 24 | gap fra blocchi di lezione, margine schermo (medium) |
| `xl` | 32 | gap fra sezioni |
| `xxl` / `xxxl` | 48 / 64 | respiro negli stati vuoti |

`layout`: `screenMarginCompact` 16 (< 600 dp), `screenMarginMedium` 24, `screenMarginExpanded` 32, `readingMaxWidth` 600, `contentMaxWidth` 840, `minTouchTarget` 48, `controlHeight` 48, `controlHeightLarge` 56, `topBarHeight` 56, `iconSize` 24, `iconStroke` 1.75, `blockGap` 24, `blockGapTight` 12, `listPaneWidth` 360, `paneHeaderHeight` 72, `dialogWidth` 360, `dialogWidthWide` 480, `contextPaneWidth` 280.

Regole base: (1) target ≥ 48 × 48 dp, ≥ 8 dp fra target adiacenti; (2) la distanza comunica il raggruppamento: feedback a 12 dp dalla domanda, blocchi a 24; (3) il testo di studio non supera mai `readingMaxWidth` (600 dp, 45-75 caratteri per riga, P7); le liste non superano `contentMaxWidth` (840 dp).

### 5.1 Layout adattivo (ADR 0003: tablet in orizzontale per primo)

Si decide dalla **window size class della finestra** (larghezza), non dal dispositivo né dall'orientamento: così rotazione, tablet verticale e multi-window seguono le stesse regole. Riferimento di progetto e di accettazione: **expanded ≈ 1280 × 800 dp**.

| Classe | Larghezza | Margine schermo | Navigazione "lista → dettaglio" |
|---|---|---|---|
| compact | < 600 | 16 (`screenMarginCompact`) | schermate impilate, una per volta |
| medium | 600-839 | 24 (`screenMarginMedium`) | schermate impilate, contenuto centrato a `contentMaxWidth` |
| **expanded** | ≥ 840 | 32 (`screenMarginExpanded`) | **list-detail a 2 pannelli** |

**Quanti pannelli: 2, non 3.** Cartelle | Argomenti della cartella; l'argomento si apre **a tutta finestra** sopra il list-detail. Motivi: (a) un terzo pannello con il dettaglio argomento sarebbe vuoto in Fase 1 e, dalla Fase 2, l'argomento è un ambiente (Chat, Lezioni, Fonti…) che ha bisogno di tutta la larghezza: stiparlo in ~560 dp lo renderebbe una versione stretta di se stesso (carico cognitivo da spazio, non da contenuto); (b) 3 pannelli sempre visibili competono per l'attenzione (P1, Sweller); (c) 2 pannelli mantengono la gerarchia leggibile: *dove sono* (sinistra) e *che cosa contiene* (destra). La regola generale per le fasi future: **si affianca solo contesto di navigazione; l'attività di studio (chat, lezione, ripasso) occupa la finestra**.

**Pannelli (expanded, 1280 dp)**:

```
|<-- 360 -->|1|<------------------ 919 ------------------>|
| Cartelle  | | Argomenti                                |
| pannello  | |   colonna lista centrata, max 840 dp     |
| fisso     | |   (margini 32 dp)                        |
```

- Pannello sinistro (navigazione): larghezza fissa `listPaneWidth` 360 dp, fill `surfaceSunken`. Pannello destro: tutto il resto (≥ 480 dp a 840), fill `bg`. Divisore verticale `line` 1 dp (`stroke.hairline`) a tutta altezza; nessuna ombra, nessun elemento trascinabile (niente handle di ridimensionamento: un controllo in più senza beneficio).
- La TopBar dell'app occupa tutta la larghezza sopra i pannelli (titolo app + Impostazioni); ogni pannello ha sotto la propria **intestazione** di `paneHeaderHeight` 72 dp (titolo del pannello `ui.title2` + azione).
- **Lezione e ripasso (Fasi 5/8)**: colonna di lettura/esercizio centrata a `readingMaxWidth` 600; lo spazio laterale ospita **contesto**, non testo più largo: a sinistra o destra un pannello `contextPaneWidth` 280 dp (indice delle sezioni con progresso, glossario dei termini della lezione). Sotto 1100 dp di larghezza il pannello di contesto diventa un bottone/sheet. Non si creano colonne di testo parallele: la lettura resta lineare.
- **Azioni primarie su expanded**: restano **dentro il pannello a cui si riferiscono**, vicino all'oggetto (contiguità, P4), non in un FAB flottante né in alto a destra della finestra. "Nuovo argomento" (primario) nell'intestazione del pannello destro, allineato a destra; "Nuova cartella" (secondario) ancorata in basso nel pannello sinistro. Un solo bottone *primario* per pannello attivo; l'altro è secondario. Motivo: con una tavoletta a due mani le azioni in basso a sinistra e in alto a destra sono entrambe raggiungibili con i pollici, e un FAB in centro-basso sarebbe lontano dalla lista che modifica.
- **Medium (tablet verticale, finestre 600-839)**: nessun list-detail (480 dp di dettaglio con 320 di lista sarebbero entrambi stretti); si usa la navigazione impilata del compact con margini 24 e contenuto centrato a `contentMaxWidth`. La selezione di cartella eventualmente fatta in expanded si conserva (vedi `home.md`).
- **Multi-window / ridimensionamento**: la classe si ricalcola a ogni cambio di larghezza; lo stato (cartella selezionata, scroll, testo dei dialog, dialog aperti) vive nel ViewModel / `rememberSaveable` e non dipende dalla classe, quindi passare da una a un'altra non ricarica né perde nulla. Nessun layout dipende da altezza: sotto 480 dp di altezza (telefono in orizzontale) valgono le regole della classe di larghezza, con TopBar e intestazioni di pannello che scorrono via con il contenuto.
- **Dialog**: larghezza `min(larghezza finestra − 48, dialogWidth 360)` in compact; `dialogWidthWide` 480 in medium/expanded. Centrati nella finestra (non nel pannello), vedi `base-components.md` §8, §10.1.

## 6. Shape e stroke

`shape`: `sm` 6 (campi di testo, blocco di codice, evidenziatore), `md` 10 (bottoni, card, opzioni, riquadri di feedback, callout), `lg` 16 (dialog, sheet, angoli superiori), `full` (chip di stato, barre di avanzamento). Una sola famiglia di raggio (10 dp) per tutti i riquadri di contenuto: card, opzioni e feedback sono lo stesso "oggetto" e non competono (P2).
`stroke`: `hairline` 1 (card, separatori), `control` 1,5 (controlli a riposo), `selected` 2 (selezione e campo in errore), `focus` 3 (+ offset 2).

## 7. Elevazione

Solo tre livelli, e solo come ombra reale; nessuna elevazione tonale.

| Token | Valore | Uso |
|---|---|---|
| `flat` | nessuna | schermate, card, blocchi di lezione, barra in alto (si distingue per bordo inferiore `line` quando il contenuto scorre sotto) |
| `raised` | y 4, blur 16, nero α 0.12 (light) / 0.5 (dark) | sheet, menu, toast |
| `overlay` | y 8, blur 32, nero α 0.18 / 0.6 | dialog; sotto, `overlay` a opacità 0.4 |

## 8. Motion

Il movimento orienta; nessuna animazione può sembrare un indizio (P7) né ritardare il feedback (P6). Durate: `instant` 0, `fast` 100, `base` 200, `slow` 300, `slower` 400 ms. Easing: `out` (0, 0, 0.2, 1) per entrate, `in` (0.4, 0, 1, 1) per uscite, `inOut` (0.4, 0, 0.2, 1) per cambi di stato.

| Evento | ms | Easing | Effetto |
|---|---|---|---|
| Pressione di un controllo (`press`) | 100 | out | cambio di riempimento |
| Comparsa del feedback | 200 | out | dissolvenza + espansione in altezza; nessun rimbalzo/shake |
| Cambio selezione | 100 | out | bordo 1,5→2 dp e riempimento, uguale per tutte le opzioni |
| Sblocco di "Continua" (`unlock`) | 200 | out | passaggio da secondario a primario per dissolvenza; nessun movimento |
| Rivelazione sezione | 300 | out | dissolvenza + scorrimento ≤ 24 dp, poi scroll alla sezione |
| Transizione di schermata | 300 in / 200 out | out / in | fade-through |
| Rivelazione risposta in una scheda | 150 | inOut | dissolvenza incrociata (niente flip 3D) |
| Avanzamento | 300 | inOut | larghezza della barra |

**Riduci animazioni** (`Settings.Global.ANIMATOR_DURATION_SCALE == 0f`): durate a 0, nessuno slide/scala/scroll animato; cambio istantaneo (o dissolvenza ≤ 100 ms). Il feedback resta visibile (icona + testo). Nessuna animazione infinita oltre allo spinner (sempre con un testo di stato). Niente lampeggio > 3 Hz.

## 9. Accessibilità (checklist trasversale)

- **Contrasto**: §2.4, in light e dark.
- **Target ≥ 48 dp**, ≥ 8 dp di distanza.
- **Font fino al 200%**: nessuna altezza fissa; i gruppi a più colonne (voti del ripasso, tab) passano a una colonna oltre fontScale 1,3 (`LocalDensity.current.fontScale`).
- **Non solo colore**: icona + parola in ogni esito; selezione = bordo 2 dp + indicatore, non solo tinta.
- **TalkBack**: ogni componente custom espone `semantics` con ruolo (`Role.Button/RadioButton/Checkbox/Tab`), stato (`selected`, `stateDescription`, `error`) e azioni. Poiché non si usa Material, **ogni componente deve impostare i ruoli a mano**. Il feedback è `liveRegion = Polite` ("Corretto. <spiegazione>"). Le icone decorative hanno `contentDescription = null`.
- **Focus** da tastiera/switch: ordine logico, anello 3 dp.
- **Nessun limite di tempo**, nessun gesto obbligatorio senza alternativa a bottone.
- **Testo** selezionabile (paragrafi, codice), mai in immagini.
- **Lingua**: UI in italiano; il contenuto può avere `lang` per blocco (proposta contratti) per la voce giusta di TalkBack.

## 10. Voce dell'interfaccia

- Italiano, seconda persona informale, verbo in testa nei bottoni ("Controlla", "Continua", "Mostra risposta").
- L'errore è sobrio e specifico: "Non proprio. Ecco perché:", mai "Sbagliato!" né "Ops!", mai punti esclamativi su un errore.
- Niente "Bravo!" generico, niente streak o confronti: una conferma è specifica ("Corretto. L'ownership passa a `b`, quindi `a` non è più valido.").
- Gli stati vuoti dicono cosa fare, non cosa manca. I blocchi (es. "Continua" non ancora attivo) dicono **cosa serve per procedere**, mai solo "non disponibile".

## 11. Terminologia dell'interfaccia (italiano)

| Concetto | Nell'interfaccia | Mai |
|---|---|---|
| Topic | **argomento** (pl. argomenti) | "Topic", "materia", "corso" |
| Folder | **cartella** | "categoria", "gruppo" |
| Mission | **mission** (invariato, è un termine del prodotto, spiegato la prima volta in Chat) | "obiettivo" in alternativa |
| Lesson | **lezione** | |
| Sessione / login | **accesso** («Accedi», «Esci»), «nome utente», «password» | "login", "logout", "account" sul titolo, "token" (l'utente non lo vede più) |
| Chiave LLM | **chiave LLM** (nei messaggi: «la chiave del servizio LLM»); stati «Impostata», «Non impostata», «Rifiutata dal servizio»; **credito** (non è un problema della chiave): «credito del servizio LLM esaurito» | nominare un provider, "API key" in inglese, "token" |

Motivo: una sola parola per cosa (pre-training, Mayer) e italiano naturale; "Topic" resta nome tecnico di codice e documenti interni. Nei testi che nominano un oggetto dell'utente si usano le virgolette basse: «Lavoro» (il nome può contenere spazi e punteggiatura, le virgolette ne mostrano i confini). I nomi non si troncano mai nei dialog e nei messaggi.

Conteggi: 0 → "Nessun argomento", 1 → "1 argomento", n → "n argomenti" (usare `plurals`).

## 12. Indice

`base-components.md` (bottone, campo di testo, card, riga, tab, esito, barra in alto, dialog, + progress, toast, sheet) · `screens/{home,folder,topic,lesson,review,settings,login}.md` · `llm-key.md` (chiave LLM mancante o rifiutata, Fase 11) · `generator-guidelines.md` (Grader, Recorder, Planner, Fase 7) · `screens/progress.md` (Registro e Glossario) · `components/` (17 blocchi SDUI + `README.md` con le linee guida per il Generator + `_exercises.md`, Fase 5) · `tools/contrast_check.py`.
