# Componenti base

> Specifica per android-dev: i componenti si costruiscono **da zero su Compose `foundation`** (`Box`, `Row`, `Column`, `BasicTextField`, `Modifier.clickable/selectable/toggleable`, `Canvas`), senza `material3`. Valori in `tokens.json`; principi in `design-system.md`. Tutti i colori sono ruoli di `TeachTheme.colors`, tutte le misure in dp/sp dei token.
> Convenzioni comuni (valgono per tutti i componenti, non si ripetono):
> - **Pressed**: cambio di riempimento in 100 ms, nessun ripple, nessuna scala. **Focus**: anello `focus` 3 dp, offset 2 dp, stessa forma del componente, solo con tastiera/switch/D-pad. **Disabled**: riempimento `disabledFill`, contenuto `disabledInk`, nessun bordo. **Error**: bordo `danger` 2 dp + icona `alert-circle` + testo `danger`.
> - Area tappabile ≥ 48 × 48 dp (si estende se il visibile è più piccolo). Distanza minima fra target: 8 dp.
> - Ogni componente imposta a mano `semantics` (ruolo, stato, azione): senza Material niente è automatico.
> - Altezze `min`, mai fisse: tutto cresce fino al font 200%.
> - Icone: Lucide, 24 dp (20 dp in chip), tratto 1,75 dp, `ink` o `inkSoft` salvo diversa indicazione.

---

## 1. Button

Tre varianti + una distruttiva. Etichetta `ui.label` (14/20, 600), a capo se necessario, mai ellissi.

```
 Primario            Secondario           Testuale          Distruttivo
┌───────────────┐   ┌───────────────┐
│  Controlla    │   │  Annulla      │     Riprova           Elimina
└───────────────┘   └───────────────┘     (accent)          (bordo+testo danger)
 fill accent         bordo border 1.5
```

| Misura | Valore |
|---|---|
| Altezza min | 48 dp (`controlHeight`); variante **grande** 56 dp (`controlHeightLarge`) per la CTA principale di una schermata |
| Larghezza min | 64 dp; variante **a tutta larghezza** (`fillMaxWidth`) per CTA in colonna |
| Padding orizzontale | 20 dp (testuale: 12 dp) |
| Icona (opzionale, a sinistra) | 20 dp, gap 8 dp; solo icona: 48 × 48 dp (variante *IconButton*, forma `md`, `contentDescription` obbligatoria) |
| Raggio | `md` (10 dp) |

| Stato | Primario | Secondario | Testuale | Distruttivo |
|---|---|---|---|---|
| default | fill `accent`, testo `onAccent` | fill `surface`, bordo `border` 1,5 dp, testo `ink` | senza fill, testo `accent` | come secondario, bordo e testo `danger` |
| pressed | fill `accentPressed` | fill `surfacePressed` | fill `surfacePressed` | fill `surfacePressed` |
| focus | anello `focus` | idem | idem | idem |
| disabled | fill `disabledFill`, testo `disabledInk` | idem, senza bordo | solo testo `disabledInk` | come secondario disabled |
| loading | come disabled, icona = spinner 20 dp (`accent`/`disabledInk`) e testo di stato ("Provo…"); `semantics.busy`; non ripremibile | — | — | — |
| error | — (gli errori sono del contesto, non del bottone) | | | |

Regole:
- Una schermata ha **al massimo un bottone primario visibile**. Annulla/indietro è secondario o testuale.
- Un bottone disabilitato **non è mai muto**: vicino (sopra o sotto) c'è il motivo in `bodyS`/`body` (`inkSoft`). Caso speciale "soft-lock": vedi `screens/lesson.md`.
- Semantics: `Role.Button`; stato disabled → `disabled()`, loading → `stateDescription = "In corso"`.
- Contrasto: `onAccent`/`accent` e `onAccent`/`accentPressed` ≥ 4.5; bordo secondario ≥ 3.

## 2. Campo di testo (`TeachTextField`)

Etichetta **sempre visibile sopra** il campo (mai solo placeholder; mai etichetta flottante che si rimpicciolisce).

```
URL del backend                          ← label  ui.label, inkSoft, gap 4 dp sotto
┌──────────────────────────────────┐
│ https://teach.tailnet.ts.net     │ 48+ dp   fill surface, bordo border 1,5, raggio sm
└──────────────────────────────────┘
Es. https://…                            ← supporting text  ui.bodyS, inkSoft, gap 4 dp
```

Variante **mascherata** (password, chiave LLM; non è il token):
```
Password
┌──────────────────────────────┬───┐
│ ••••••••••••••••             │ 👁 │   trailing: IconButton 48×48 (eye / eye-off)
└──────────────────────────────┴───┘
```

| Misura | Valore |
|---|---|
| Altezza min campo | 48 dp (`controlHeight`); multi-linea (chat): min 48, cresce fino a 5 righe, poi scorre |
| Padding interno | 12 dp orizzontale, 12 dp verticale; con trailing: il trailing occupa 48 dp e il testo ha padding destro 0 |
| Testo | `ui.bodyL` (16/24) in `ink`; placeholder `ui.bodyL` in `inkMuted` |
| Raggio | `sm` (6 dp) |
| Gap etichetta/campo/supporting | 4 dp |

| Stato | Aspetto |
|---|---|
| default | fill `surface`, bordo `border` 1,5 dp |
| pressed | (il tocco porta il focus) |
| focus | bordo `focus` 2 dp (nessun anello esterno aggiuntivo: il bordo spesso è l'indicatore); cursore `accent` |
| disabled | fill `disabledFill`, testo `disabledInk`, nessun bordo |
| error | bordo `danger` 2 dp; supporting text sostituito da **icona `alert-circle` 20 dp + messaggio** in `danger` (`ui.bodyS`, a capo, mai troncato); `semantics.error(messaggio)` |

Comportamento:
- La validazione scatta alla perdita del focus e all'azione di invio, non a ogni tasto. L'errore sparisce quando l'utente modifica il campo.
- **Mascherato**: `PasswordVisualTransformation` quando nascosto, testo in chiaro quando mostrato; il toggle `eye`/`eye-off` ha `contentDescription` "Mostra {campo}"/"Nascondi {campo}" (es. "Mostra password", "Mostra chiave") e `Role.Switch`/`toggleable`. Copia/taglia disabilitati se mascherato. **Autofill**: spento di default (segreti da incollare, es. la chiave LLM); **acceso solo per le credenziali** (`ContentType.Username`, `ContentType.Password`, `ContentType.NewPassword`: `screens/login.md`, `screens/settings.md` «Cambia password»). Niente suggerimenti/apprendimento della tastiera (`KeyboardOptions(autoCorrectEnabled = false)`, `keyboardType = Password`/`Uri` secondo il campo, flag no-personalized-learning). Il valore non entra mai nei log né in `contentDescription`.
- Ricreazione/rotazione: il testo sopravvive (stato nel ViewModel).
- Semantics: etichetta associata (`contentDescription`/`labelledBy`), `editableText`, supporting text letto dopo il valore.

## 3. Card / superficie (`Surface`)

| Variante | Aspetto | Uso |
|---|---|---|
| **static** | fill `surface`, bordo `line` 1 dp, raggio `md`, padding 16 dp, nessuna ombra | contenitore standard |
| **interactive** | come static, intera card cliccabile (`Role.Button`), pressed fill `surfacePressed`, focus anello | card "Riprendi", riga lezione |
| **emphasis** | fill `accentSoft`, senza bordo, testo `ink` | la CTA della schermata (card Ripasso, "Prossima lezione") |
| **sunken** | fill `surfaceSunken`, senza bordo | aree incassate, codice inline esteso |
| **status** | fill `xSoft`, senza bordo | solo tramite componente Esito (§5) |

- La card **non** cambia colore in base all'altezza e non ha ombra; in dark `surface` (`#1A1B1F`) è più chiara di `bg` (`#121316`), in light `#FFFFFF` su `#FBFAF8`: il raggruppamento lo fanno bordo `line`, padding e spazio.
- Titolo `ui.title3`, testo `ui.bodyL`/`ui.body`, azioni a destra o in basso (bottoni di §1).
- Una card interattiva ha **un solo** target; non annidare bottoni dentro (usare una card statica con bottone, o una riga).
- Contrasto: `ink` su tutte le superfici ≥ 7:1.

## 4. Lista / riga (`ListRow`)

```
┌────────────────────────────────────────────┐
│ [lead]  Titolo della riga            [trail]│ 56 dp (1 riga) / 72 dp (2 righe)
│         Sottotitolo / meta                  │
├ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤ separatore hairline line, inset 16 dp
```

| Misura | Valore |
|---|---|
| Altezza min | 56 dp (una riga), 72 dp (con sottotitolo); cresce con testo/font |
| Padding | 16 dp orizzontale, 8 dp verticale |
| Slot leading | icona 24 dp (`inkSoft`) o numero in cerchio 32 dp (fill `surfaceSunken`, testo `ui.label` `ink`); gap con il testo 16 dp |
| Titolo | `ui.title3` (18/26) `ink`, max 2 righe a font normale, a capo senza limite a font grande |
| Sottotitolo | `ui.body` `inkSoft` (meta-dati, stato in parole) |
| Slot trailing | `chevron-right` 24 dp `inkSoft` se naviga; testo/valore `ui.body` `inkSoft`; `more-vertical` IconButton 48 dp per menu |

Stati: default fill trasparente; pressed `surfacePressed`; focus anello 3 dp (all'interno del bordo della riga); disabled titolo `disabledInk`; error: icona `alert-circle` in `danger` nel trailing + testo in sottotitolo (mai solo colore). Tutta la riga è **un unico target** (`Role.Button`) con `contentDescription` composta ("Ownership e borrowing, completata"). Se contiene un'azione secondaria (menu), questa è un target separato ≥ 48 dp.
Le righe vivono dentro una card statica con separatori; il raggio è sul contenitore, non sulle righe.

## 5. Indicatore di stato / esito (`StatusPanel` e `StatusChip`)

Il componente che porta **giusto / sbagliato / parziale**, i callout e gli errori. Colore + icona + parola (P8).

```
StatusPanel
┌─────────────────────────────────────────┐
│ (✓)  Corretto                           │  icona 24 + etichetta  ui.labelL, colore x
│                                         │  gap 8 dp
│ L'ownership passa a `b`, quindi `a`     │  corpo  ui.bodyL, colore ink
│ non è più valido.                       │  fill xSoft, raggio md, padding 16 dp
└─────────────────────────────────────────┘

StatusChip   ( ✓ Connesso )   icona 20 + ui.labelS, fill xSoft, raggio full, altezza visiva 28 dp, non interattivo
```

| Variante | Icona | Etichetta di default | Ruoli |
|---|---|---|---|
| `correct` | `check-circle` | "Corretto" | `correct` / `correctSoft` |
| `incorrect` | `x-circle` | "Non proprio" | `incorrect` / `incorrectSoft` |
| `partial` | `minus-circle` | "Quasi" | `partial` / `partialSoft` |
| `info` | `info` | "Info" | `info` / `infoSoft` |
| `tip` | `lightbulb` | "Suggerimento" | `tip` / `tipSoft` |
| `warning` | `alert-triangle` | "Attenzione" | `warning` / `warningSoft` |
| `danger` | `alert-circle` | (titolo libero) | `danger` / `dangerSoft` |
| `progress` | spinner 24 dp `accent` | (titolo libero) | fill `surfaceSunken`, etichetta `ink` |

Regole: icona e etichetta in colore `x`; **il corpo è sempre `ink`** su `xSoft` (≥ 7:1); l'etichetta può essere sostituita con un titolo specifico ("Token non valido") ma l'icona resta. Nessun bordo; nessuna animazione oltre a `feedbackAppear` (200 ms, dissolvenza + altezza). `semantics`: `liveRegion = Polite` quando appare dopo un'azione; il testo letto è "<etichetta>. <corpo>"; l'icona ha `contentDescription = null`. Passando da una variante all'altra l'ordine resta icona → etichetta → corpo.

## 6. Barra in alto (`TopBar`)

```
┌────────────────────────────────────────────┐
│ [←]  Titolo della schermata        [⚙] [⋮] │  56 dp (cresce se il titolo va a capo)
├────────────────────────────────────────────┤  bordo inferiore `line` 1 dp SOLO quando il contenuto
```

| Misura | Valore |
|---|---|
| Altezza min | 56 dp (`topBarHeight`) + inset della status bar |
| Fill | `bg` (stessa del contenuto: nessuna elevazione) |
| Navigazione | IconButton 48 × 48 a 4 dp dal bordo: `chevron-left` (indietro) o `x` (chiudi un flusso) |
| Titolo | `ui.title2` (21/28) `ink`, 1 riga con ellissi a font normale; **fino a 2 righe** se fontScale > 1,3 |
| Azioni | max 2 IconButton 48 dp a destra (`inkSoft`, `ink` se attivo) |
| Stati | default; pressed delle icone = fill `surfacePressed` (forma `md`); focus anello; disabled icone `disabledInk` |

Semantics: il titolo è un heading (`heading()`), le icone hanno `contentDescription` ("Indietro", "Impostazioni"). Sotto la barra può esserci una riga di avanzamento (Progress, §9).

## 7. Tab (`TabRow`)

```
 Lezioni   Chat   Glossario   Fonti   Mission     ← scrollabile in orizzontale
 ━━━━━━━━                                          ← indicatore 3 dp accent sotto la tab selezionata
───────────────────────────────────────────────    ← bordo inferiore line 1 dp
```

| Misura | Valore |
|---|---|
| Altezza | 48 dp min (cresce con il font) |
| Larghezza tab | min 72 dp; padding orizzontale 16 dp; distribuzione: scrollabile, allineata a sinistra |
| Etichetta | `ui.label` (14/20, 600), una sola riga (a font grande la tab si allarga, la riga scorre) |
| Selezionata | testo `ink` + **indicatore 3 dp `accent`** (lunghezza = etichetta) |
| Non selezionata | testo `inkSoft` |

Stati: pressed fill `surfacePressed`; focus anello 3 dp; disabled testo `disabledInk`. La selezione è data da **testo scuro + sottolineatura**, non solo dal colore. Semantics: contenitore `selectableGroup`, ogni tab `Role.Tab` con `selected`. Il cambio di tab non anima il contenuto (dissolvenza `fast`).

## 8. Dialog di conferma (`ConfirmDialog`)

```
░░░░░░░░ scrim overlay α 0.4 ░░░░░░░░
░  ┌────────────────────────────┐ ░
░  │ Eliminare la cartella?     │ ░  titolo  ui.title2, ink
░  │                            │ ░
░  │ «Lavoro» contiene 3 Topic. │ ░  corpo   ui.bodyL, ink (nomina l'oggetto e le conseguenze)
░  │ Verranno eliminati.        │ ░
░  │                            │ ░
░  │        [Annulla] [Elimina] │ ░  azioni, allineate a destra, gap 8 dp
░  └────────────────────────────┘ ░
```

| Misura | Valore |
|---|---|
| Pannello | fill `surfaceRaised`, raggio `lg` (16), ombra `overlay`, padding 24 dp |
| Larghezza | `min(larghezza − 48 dp, dialogWidth 360)` in compact; `dialogWidthWide` **480 dp** in medium/expanded (vedi "Dialog su expanded" sotto) |
| Azioni | **Annulla** = Button testuale (a sinistra), **conferma** = Button primario, oppure *distruttivo* se l'azione elimina dati. Altezza 48 dp |
| Font > 1,3 | le azioni si dispongono in colonna a tutta larghezza (conferma sopra, annulla sotto); il pannello scorre se supera l'altezza |

Comportamento: il **focus iniziale va su "Annulla"** (azione sicura); tocco sullo scrim, tasto Indietro e gesto equivalgono ad Annulla. Il corpo spiega la conseguenza, non ripete il titolo. Etichette verbali ("Elimina", "Esci senza salvare"), mai "Sì/No/OK". Focus intrappolato nel dialog; `paneTitle` = titolo; alla chiusura il focus torna all'elemento che l'ha aperto. Dialog informativi (un solo bottone) usano un bottone primario "Ho capito".

**Slot errore**: se l'azione confermata fallisce (rete, 5xx), il dialog **resta aperto** e sopra le azioni appare un `StatusPanel` `danger` (live region Polite) con il motivo; il bottone di conferma torna attivo con la stessa etichetta (ritentare = ripremere). Durante l'invio: conferma in stato *loading* ("Elimino…"), "Annulla" disabilitato, scrim e Indietro ignorati (il motivo è visibile nell'etichetta del bottone).

---

## 9. Altri componenti usati dalle schermate

**Progress (`ProgressBar`)**: barra 4 dp, raggio `full`, traccia `line`, riempimento `accent`; larghezza animata 300 ms (`progress`). Sempre accompagnata da un testo ("Sezione 2 di 4"). `semantics.progressBarRangeInfo`. Indeterminato: barra che scorre; in "riduci animazioni" è fissa al 40% con il testo di stato. Spinner: anello 24 dp, tratto 2,5 dp `accent`, sempre con un testo vicino.

**Opzione selezionabile (`ChoiceRow`)**: usata da quiz, ordinamento, checklist (le specifiche dei blocchi in `components/`).
| Misura | Valore |
|---|---|
| Altezza min | 56 dp, padding 16 × 12 dp, raggio `md`, testo `ui.bodyL` `ink`, indicatore a sinistra (radio 24 dp o checkbox 24 dp, tratto `border` 1,5 dp), gap 12 dp |
| default | fill `surface`, bordo `border` 1,5 dp |
| pressed | fill `surfacePressed` |
| selected | fill `accentSoft`, bordo `accent` 2 dp, indicatore pieno `accent` (punto o spunta in `onAccent`) |
| focus | anello 3 dp |
| disabled (dopo "Controlla") | invariata nell'aspetto, non cliccabile; l'esito si segnala con icona + etichetta (§5), mai ricolorando l'opzione prima di "Controlla" |
Tutte le opzioni di una domanda hanno **identico** stile, altezza minima e padding (P7). `Role.RadioButton`/`Role.Checkbox`, `selected`, `selectableGroup`.

**Toast**: fill `toastBg`, testo `ui.bodyL` `toastInk`, azione opzionale Button testuale in `toastAccent`; raggio `md`, ombra `raised`, margine 16 dp, altezza min 48. Durata 4 s (6 s con azione). Posizione: sopra la barra inferiore. `liveRegion = Polite`. Niente toast per errori che richiedono azione (usare StatusPanel).

**Sheet (`BottomSheet`)**: fill `surfaceRaised`, angoli superiori `lg`, ombra `raised`, scrim 0.4, padding 24 dp, altezza max 90%; maniglia 32 × 4 dp (`line`, decorativa). Si chiude con trascinamento, tocco sullo scrim, Indietro **e** con un bottone "Chiudi" nel contenuto (alternativa al gesto). Focus iniziale sul titolo.

## 10. Dialog di input, menu, chip, skeleton ed errore

### 10.1 Dialog di input (`InputDialog`)

Per crea/rinomina di cartelle e argomenti (un solo campo breve). Stessa famiglia visiva di `ConfirmDialog` (§8): pannello `surfaceRaised`, raggio `lg`, ombra `overlay`, padding 24 dp, larghezza come `ConfirmDialog` (360 compact, 480 medium/expanded).

**Perché dialog e non sheet**: un campo e due azioni sono un'unica decisione breve (P1). Il dialog resta centrato con la tastiera aperta e non ha il gesto di trascinamento, che farebbe perdere il testo digitato; lo sheet è per contenuti più ricchi (termini, selezioni). Coerente con `ConfirmDialog`: l'utente impara un solo modo di "fare una cosa piccola".

```
░░░░░░░░ scrim overlay α 0.4 ░░░░░░░░
░  ┌────────────────────────────┐ ░
░  │ Nuova cartella             │ ░  titolo ui.title2 (heading, paneTitle)
░  │                            │ ░
░  │ Nome della cartella        │ ░  label ui.label inkSoft (TextField §2)
░  │ ┌────────────────────────┐ │ ░
░  │ │ Es. Lavoro             │ │ ░  campo 48 dp; focus iniziale qui, tastiera aperta
░  │ └────────────────────────┘ │ ░
░  │ (!) Dai un nome alla       │ ░  errore (icona + testo danger), SOLO se c'è
░  │     cartella.     72/80 ▸  │ ░  contatore a destra, SOLO da 80% del limite
░  │ ┌────────────────────────┐ │ ░
░  │ │(!) Non riesco a …      │ │ ░  StatusPanel danger, SOLO per errori di rete/server
░  │ └────────────────────────┘ │ ░
░  │      [Annulla]   [ Crea ]  │ ░  testuale + primario, 48 dp, gap 8 dp
░  └────────────────────────────┘ ░
```

| Aspetto | Specifica |
|---|---|
| Campo | `TeachTextField` §2; `KeyboardOptions(capitalization = Sentences, autoCorrectEnabled = false, imeAction = Done)`; `KeyboardActions(onDone = invia)`. Il campo è a capo automatico (fino a 3 righe a 360 dp, il titolo può arrivare a 120 caratteri) ma **non accetta a-capo**: `\n` incollati diventano spazi. Il testo iniziale (rinomina) è il valore attuale, **tutto selezionato**. |
| Contatore | `ui.bodyS` `inkSoft`, allineato a destra sulla riga del supporting text, formato `72/80`. Compare solo quando i caratteri (code point dopo il trim) ≥ 80% del limite (64/80, 96/120); sopra il limite diventa `danger` con l'errore `too_long`. Conta code point Unicode come il backend. Semantica: letto con il campo come "72 di 80 caratteri" solo quando visibile, senza annunci a ogni tasto. |
| Validazione locale | Dopo il trim: vuoto → `empty`; oltre il limite → `too_long`. `empty` si mostra **solo all'invio** (non all'apertura né alla perdita del focus: un campo appena aperto non è un errore). `too_long` è **in tempo reale** (eccezione al §2: è un limite che l'utente può correggere subito e non si scopre a posteriori). Nessuna chiamata di rete se la validazione locale fallisce. |
| Errore del server | `422` → l'issue di `details` (`field` + `issue`) si mostra sotto il campo con lo stesso aspetto degli errori locali; il testo resta, il focus resta nel campo, l'errore sparisce alla prima modifica. Nessun controllo locale dei duplicati (la verità è del backend). |
| Errore di rete/5xx | Il dialog **resta aperto con il testo intatto**; `StatusPanel danger` sopra le azioni: titolo breve + una frase (testi nelle schermate). Il bottone primario riprende la sua etichetta: ritentare = ripremere. |
| Invio | Primario in *loading* ("Creo…" / "Salvo…"), campo in sola lettura, "Annulla" disabilitato; la tastiera si chiude. Timeout di rete 10 s → errore di rete. |
| Rinomina senza modifiche | "Salva" con il testo uguale all'originale (dopo il trim, stessa grafia) chiude il dialog senza chiamata. Cambiare solo maiuscole è una modifica valida. |
| Chiusura | "Annulla" e Indietro chiudono scartando il testo. Il tocco sullo scrim **non** chiude (evita di perdere il testo per un tocco accidentale). Focus alla chiusura: sull'elemento che ha aperto il dialog (o, dopo una creazione riuscita, sulla nuova riga: vedi schermate). |
| Tastiera | `imePadding` sul pannello; il pannello scorre se non entra (font 200%, landscape). Il campo e il primario restano visibili. |
| Font > 1,3 | Azioni in colonna a tutta larghezza: primario sopra, "Annulla" sotto. |
| Semantics | `paneTitle` = titolo; il campo ha la label visibile; l'errore è `semantics.error(...)` e live region Polite; lo `StatusPanel` è live region Polite. |

### 10.2 Menu di azioni (`ActionMenu`)

Menu a comparsa ancorato a un `more-vertical` IconButton (e aperto anche dalla pressione lunga della riga). Voci: massimo 4, ognuna una riga da 48 dp.

| Misura | Valore |
|---|---|
| Pannello | fill `surfaceRaised`, raggio `md`, ombra `raised`, larghezza min 200 dp, max 280 dp, padding verticale 8 dp; ancorato sotto l'icona, allineato a destra, ribaltato sopra se non c'è spazio |
| Voce | altezza min 48 dp, padding orizzontale 16 dp, icona 24 dp + gap 16 dp + etichetta `ui.body`; pressed `surfacePressed`; focus anello 3 dp |
| Voce distruttiva | icona ed etichetta `danger`, separata dalle altre da una hairline `line`; sempre l'ultima |
| Chiusura | tocco fuori, Indietro, scelta di una voce. Focus iniziale sulla prima voce. Alla chiusura il focus torna all'IconButton |

Semantics: contenitore con `paneTitle` ("Azioni per <nome>"), voci `Role.Button`. Le righe che hanno un menu espongono le **stesse azioni come `customActions` TalkBack** ("Rinomina", "Elimina"), così non serve cercare il pulsante.

### 10.3 `StatusChip` varianti per lo stato dell'argomento

Aggiunge a §5 due varianti per le liste (testo sempre `ui.labelS`, icona 20 dp, tutto in colore `x` tranne dove indicato):

| Variante | Fill | Contenuto | Contrasti verificati (light / dark) |
|---|---|---|---|
| `neutral` | `surfaceSunken` | icona e testo `inkSoft` | 8,15 / 11,30 |
| `info` | `infoSoft` | icona e testo `info` | 5,53 / 5,55 |
| `accent` | `accentSoft` | icona e testo `accent` | 6,19 / 6,49 |

### 10.4 Skeleton di lista

3 righe da 72 dp dentro la card statica: icona 24 dp + due barre (60% e 35% della larghezza, 14 dp, raggio `sm`) in `surfaceSunken`. Nessuno shimmer (con "riduci animazioni" e per default: nessun movimento). `semantics`: l'intero blocco è un elemento con `stateDescription = "Caricamento"`; dopo 8 s compare sotto "Sto ancora caricando…".

### 10.5 Blocco errore a schermo intero (`ErrorState`)

Per il fallimento del **primo caricamento** (nessun dato da mostrare): colonna centrata, `StatusPanel danger` con titolo e frase, sotto i bottoni (primario "Riprova", testuale secondario opzionale). Non è un banner: non c'è contenuto sotto da preservare. Se esistono dati già caricati e un *refresh* fallisce, si usa invece il **banner** (`StatusPanel danger` in cima alla lista, con "Riprova" e "Chiudi"/azioni secondarie) e la lista resta visibile.

## 11. Checklist per android-dev

- Nessuna dipendenza `androidx.compose.material3` (né `material-icons-extended`: le icone sono vettoriali Lucide). Ripple disattivato (`indication = null`).
- Un solo `TeachTheme` con `TeachColors`, `TeachTypography`, `TeachShapes`, `TeachSpacing`.
- Preview light/dark/fontScale 2,0 per ogni componente e stato.
- Test: contrasto non si testa in UI (lo garantisce `tools/contrast_check.py`), ma si testa che ogni componente abbia `semantics` coerenti e area ≥ 48 dp.

## 11. Dialog su expanded (tablet orizzontale)

Vale per `ConfirmDialog` (§8) e `InputDialog` (§10.1).

- **Larghezza** 480 dp (`dialogWidthWide`), centrato nella **finestra** (non nel pannello che lo ha aperto), scrim su tutta la finestra. Mai a tutta larghezza: un campo da 120 caratteri a 900 dp spinge a scrivere righe non rileggibili. Le azioni restano a destra in riga (gap 8 dp); la colonna a tutta larghezza scatta solo con font > 1,3 come in compact.
- **Tastiera virtuale in orizzontale** (1280 × 800 dp: la tastiera occupa ~ 300-400 dp; altezza utile ≈ 400 dp). Il contenitore dello scrim applica `imePadding()` e centra il pannello **nell'area sopra la tastiera**. Altezza del dialog di input: ≈ 230 dp senza errori, ≈ 330 con errore di campo + StatusPanel + contatore: entra. Se l'altezza utile è minore del pannello (telefono in orizzontale, multi-window basso, font grande), il **corpo** (titolo, campo, errori) scorre e la **riga delle azioni resta fissata** in basso al pannello, sempre visibile; il campo con focus viene portato in vista (`bringIntoViewRequester`). Non si usa `adjustPan` né fullscreen dialog.
- **Tastiera fisica**: se attaccata la tastiera virtuale non compare e il dialog resta centrato. Tab/Maiusc+Tab percorrono campo → Annulla → primario (focus ring `focus` visibile); Invio nel campo = invia (`imeAction Done`), Esc = Annulla (nel `ConfirmDialog` e nell'`InputDialog`; il tocco sullo scrim continua a non chiudere l'`InputDialog`). Nel `ConfirmDialog` l'Invio sul focus iniziale (Annulla) annulla: la conferma distruttiva richiede di spostarsi sul bottone.
- Il campo ha focus e tastiera aperti all'apertura; alla chiusura la tastiera si nasconde prima del ritorno del focus.

## 12. Componenti della Chat

Specifica d'uso, misure e comportamento in `screens/chat.md`; qui solo l'elenco per android-dev (da costruire in `core/designsystem` se riusabili, altrimenti in `feature/chat`).

| Componente | Cos'è | Dove nel dettaglio |
|---|---|---|
| `MarkdownText` | Renderer del sottoinsieme markdown (paragrafi, grassetto/corsivo, codice inline, elenchi, link `https`, blocchi di codice; il resto letterale). Parametri: `text`, `streaming: Boolean`. Riusabile per altri testi del Tutor | chat §5 |
| `ChatCodeBlock` | Blocco di codice con intestazione 48 dp (lingua + «Copia codice»), scroll orizzontale con sfumatura, focusable | chat §5.1 |
| `TutorMessage` | Messaggio del Tutor: etichetta, parti in ordine, footer Copia, indicatore, pannello di errore. Itera le `parts` (parti sconosciute saltate) | chat §4.1, §6, §9.2 |
| `UserMessage` | Bolla `accentSoft` a destra, testo letterale | chat §4.2 |
| `DaySeparator` | Etichetta di giorno centrata | chat §4.3 |
| `TypingIndicator` | Tre punti 6 dp + testo di stato; varianti: «Il Tutor sta scrivendo…», «Connessione persa. Riconnetto…» (icona `wifi-off`), «Non riesco a raggiungere il backend…» + «Riprova ora». Pulsazione di opacità 1600 ms, ferma con "riduci animazioni" | chat §6.1, §9.6 |
| `NewMessagesPill` | Pillola 48 dp (fill `surfaceRaised`, bordo `border` 1,5, ombra `raised`, raggio `full`, icona `arrow-down` + «Nuovi messaggi») | chat §7 |
| `ChatComposer` | Campo multilinea + Invia 48 dp + riga motivo + contatore; scorciatoie tastiera fisica | chat §8 |
| `ChatErrorPanel` | `StatusPanel danger` con Riprova per messaggio fallito; pannello «Messaggio non inviato» sopra il composer | chat §9.2-9.3 |
| `ContextPane` | Pannello laterale 280 dp senza riquadro (titolo `ui.title3` + testo), solo expanded ≥ 1100 dp | chat §2 |
| `SuggestionChip` | `Button` secondario che **inserisce testo** nel composer (non invia) | chat §9.1 |

Icone Lucide nuove da importare: `send`, `copy`, `check`, `arrow-down`, `wifi-off`, `clock`, `rotate-ccw` (verificare quali già presenti in `TeachIcons`).


## 13. Componenti della Mission

Specifica d'uso e misure in `screens/mission.md`. Costruire in `feature/mission` (o `feature/topic`) se non riusabili; `Disclosure` e `SectionHeader` in `core/designsystem`.

| Componente | Cos'è | Dove |
|---|---|---|
| `MissionCard` | Card con banda di stato (`infoSoft` / `correctSoft` / `surfaceSunken`), sezioni, domanda di valutazione, azioni. Parametri: `mission`, `state` (`Open`, `Arriving`, `Confirming`, `Confirmed`, `Superseded`, `ConfirmError`), `previous: Mission?` (per le differenze), `onConfirm`, `onEdit`, `onShowDetails`. Usata nella chat (parte del `TutorMessage`) e nella tab Mission | mission §2 |
| `MissionSummary` | Sezioni in sola lettura (Perché, criteri numerati, Vincoli, Fuori dal percorso); variante `highlight = true/false` per il blocco criteri (`accentSoft` / `surfaceSunken`); sezioni vuote omesse | mission §2.1, §4.3 |
| `DiffTag` | Pill «Nuovo» (`ui.labelS`, `info` su `infoSoft`) e riga «Tolto dalla versione N» con icona `minus` | mission §2.1 |
| `Disclosure` | Riga 48 dp (target minimo; `DisclosureRow` di §14 ne è la stessa misura) con chevron e titolo, `Role.Button`, `stateDescription` Aperto/Chiuso, contenuto sotto; usata per versioni precedenti, card sostituite, voci tolte | mission §2.2, §4.3 |
| `VersionRow` | Riga versione con meta (stato + data) che si espande in `MissionSummary` | mission §4.3 |
| `PreferenceRow` | Testo + IconButton `trash-2` 48 dp; `customAction` «Elimina»; gruppo con etichetta `ui.label` | mission §4.4 |
| `MissionTabs` | `TabRow` Chat · Mission dell'argomento, con stato di tab per sessione | mission §4.1 |
| `ContextPane` (estensione) | Contenuto per stato (vuoto / solo bozza / confermata / confermata + modifica), scroll proprio | mission §4.5 |

`StatusChip`: la variante `info` per `mission_proposed` esiste già (§10.3); il chip di `researching` dice «Cerco le fonti…».

Icone Lucide nuove da importare (verificare in `TeachIcons`): `flag`, `history`, `minus`, `trash-2`, `chevron-down`, `chevron-right`, `check-circle`, `message-circle`, `search`.

Costanti: `MissionCardPadding = 20.dp (16.dp compact)` · `MissionPrefTextMax = 300` · `VersionsPageSize = 10` · `PrefMissionColumns = 600.dp + 32.dp + 280.dp (≥ 1100 dp)`.

---

## 14. Componenti della Lezione e dei blocchi SDUI

Specifiche complete nei file indicati; qui l'elenco per android-dev (tutti su `foundation`, token in `tokens.json`).

| Componente | Cos'è | Specifica |
|---|---|---|
| `ExerciseCard` | `Surface` static con intestazione (icona + tipo + chip «Facoltativo»), domanda, istruzione, corpo, azione, feedback | `components/_exercises.md` §1 |
| `ChoiceRow` (estesa) | stati aggiunti: *controllata* (corretta/errata/rivelata), tag di esito icona+parola, feedback in riga, disclosure «Perché no?»; `KeyHint` | `_exercises.md` §4, `quiz_single.md` |
| `KeyHint` | riquadro 24 × 24 dp con cifra, solo con tastiera fisica | `_exercises.md` §6 |
| `InlineMarkdown` | parser/renderer inline condiviso (grassetto, corsivo, codice, link, **termine** `[[ ]]` con tratteggio `knowledge`, **citazione** `[^ ]` numerata), parametro `interactive` | `components/paragraph.md` |
| `GlossarySheet`, `SourceSheet` | `BottomSheet` (§9) per termine e fonte; su expanded larghezza 480 centrata | `components/glossary_term.md`, `screens/lesson.md` §3.1 |
| `ContinueBar` | bottone «Continua»/«Completa lezione» con stati bloccato/sbloccato e riga del motivo | `screens/lesson.md` §5 |
| `LessonContextContent` | indice, glossario e fonti; usato dal pannello (280 dp) e dallo sheet | `screens/lesson.md` §3 |
| `DisclosureRow` | riga 48 dp con chevron + titolo + conteggio, per sezioni apribili | `screens/lesson.md` §3.1 |
| `CodeBlock` (lezione) | codice con numeri di riga, evidenziazione sintattica, collasso a 16 righe, copia | `components/code.md` |
| `UnavailableBlock` | riga neutra per blocchi non disponibili | `components/missing_component.md` |
| `StatusPanel` | nessuna modifica; nuove etichette fisse «Risposta registrata» (`info`) e «Quasi»/«Non proprio»/«Corretto» per esercizi | `_exercises.md` §4 |

Layout token nuovi (`tokens.json` → `layout`): `contextPaneBreakpoint` 1100, `contextPaneGap` 32, `choiceMinHeight` 56, `blankFieldWidth` 120, `blankFieldHeight` 40, `codeCollapsedLines` 16.

## 15. Componenti di Lezioni, Registro e Glossario

Specifiche nei file indicati; nessun colore o token nuovo in `tokens.json` (solo combinazioni già verificate in `design-system.md` §2.4).

| Componente | Cos'è | Specifica |
|---|---|---|
| `NextLessonCard` | `Surface` (static/emphasis/`danger`) con stati: Topic non pronto, pronta a generare, lezione da finire, preparazione in corso, fallita | `screens/topic.md` F3 |
| `GenerationProgress` | lista di fasi in parole con una sola corrente (come `ResearchProgress`); riga «Correggo la lezione» in riparazione | `screens/topic.md` F3.C |
| `LessonRow` | `ListRow` a più righe: numero, titolo, obiettivo, stato (icona + parole), pill «Nuova» | `screens/topic.md` F4 |
| `SegmentedSwitch` | due segmenti 48 dp (Registro · Glossario), selezione = icona `check` + fill `accentSoft` + bordo `accent` | `screens/progress.md` §2 |
| `RecordCard` | card con eyebrow (icona + parole per `kind`), titolo, corpo, disclosure «Come l'hai dimostrato» | `screens/progress.md` §3.2 |
| `GlossaryRow`, `GlossarySheet` (esteso) | riga a 2 righe; lo sheet aggiunge `Evita:` e lezione di origine | `screens/progress.md` §4 |
| `TermPill` | Button pillola 48 dp, bordo `border` 1,5, per i termini nuovi del riepilogo | `screens/lesson.md` §7.2 |
| `CompletionBlock` | blocco «Cosa hai dimostrato» del riepilogo con stati In corso / Riuscito / Vuoto / Fallito / Risposte da inviare | `screens/lesson.md` §7.2 |
| `QueuePopover` | popover dell'indicatore `cloud-off`: spiegazione + «Riprova ora» | `screens/lesson.md` §8.1 |
| `LessonContextChip` | pillola sopra il composer con la lezione di contesto e `x` per toglierla | `components/ask_agent.md` |

Icone Lucide nuove da importare (verificare in `TeachIcons`): `sparkles`, `circle`, `circle-dot`, `history`, `lightbulb`, `book-open`, `layers`, `clock`, `cloud-off`, `x`.


## 16. Multiutenza: nessun componente nuovo

Solo composizioni di componenti esistenti e due varianti d'uso. Nessun token nuovo.

| Elemento | Composizione | Dove |
|---|---|---|
| `LlmKeyNotice` | `StatusPanel` `warning` (`info` per i suggerimenti, `info` «Chiave salvata» a chiave cambiata) + `Button` secondario con icona `key-round`; funzione condivisa in `core/designsystem` che sceglie i testi da `variant` (`missing`/`rejected`), `context` e `phase` (`blocking`/`fixed`/`hint`) | `llm-key.md` |
| `SessionReasonPanel` | `StatusPanel` `info`, senza bottoni né ✕, in cima alla schermata di accesso | `screens/login.md` |
| Campo di sola scrittura | `TeachTextField` mascherato (§2) con valore solo in memoria, autofill spento, copia/taglia disabilitati | `screens/settings.md` §Chiave LLM |
| Riga host in sola lettura + «Cambia» | riga 48 dp con label, testo e Button testuale che la trasforma in `TeachTextField` | `screens/login.md` §Rientro |
| Stato della chiave | `StatusChip` §10.3: `info` «Non impostata», `neutral` «Impostata», `warning` «Rifiutata dal servizio» | `screens/settings.md` |

Icone Lucide nuove da importare: `key-round`, `log-out`, `clock`.
