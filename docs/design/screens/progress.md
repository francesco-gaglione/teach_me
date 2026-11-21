# Tab «Cosa sai»: Registro e Glossario (Fase 7)

> Contratto: `docs/contracts/lessons.md` (§2 `LearningRecord`, `GlossaryTerm`; §3 endpoint). Posizione nella schermata Argomento e tab iniziale: `topic.md` (sezione Fase 6-7). Componenti base: `../base-components.md`. Riferimento: tablet orizzontale (1280 × 800 dp), poi medium e compact. Terminologia: «registro», «glossario», «lezione», «Tutor» (mai «Learning record», «superseded», «job»).

## 1. Scopo didattico

Rendere **visibile ciò che è stato dimostrato**, non ciò che è stato letto né quanto si è "avanzati". Tre funzioni, in quest'ordine:

1. **Competenza percepita e calibrata** (Bandura, autoefficacia; metacognizione): un elenco di frasi concrete («Sai prevedere quando…») è un progresso che si può verificare, a differenza di una percentuale. Ogni voce dice *come* è stato dimostrato (evidenza), così non è un complimento ma un fatto.
2. **Consolidamento a distanza** (Bjork): rileggere dopo giorni ciò che si è appreso è già un richiamo leggero. La tab è una consultazione volontaria, mai un compito.
3. **Trasparenza** su cosa guida le lezioni successive (autonomia, Deci e Ryan): il registro è la memoria che il Tutor e il Planner usano. L'utente la vede, quindi sa perché la prossima lezione parte da lì.

Principi applicati: **P9** (niente punteggi, percentuali, serie, livelli, medaglie; nessun conteggio come traguardo), **P6** (l'errore corretto è informazione: «idea chiarita», non «errore»), **P1** (una vista alla volta: Registro *o* Glossario), **Tufte** (liste di testo, niente grafici, nessuna decorazione).

## 2. Struttura della tab

Nella tab, sotto la `TabRow`, un selettore a due voci (`SegmentedSwitch`) e sotto il contenuto della voce scelta.

```
 Lezioni   Chat   Cosa sai   Fonti   Mission
                  ━━━━━━━━
 ┌────────────────────┬────────────────────┐
 │ ✓ Registro         │   Glossario        │   SegmentedSwitch, 48 dp
 └────────────────────┴────────────────────┘
 (contenuto)
```

**`SegmentedSwitch`** (nuovo componente piccolo, nessun colore nuovo): contenitore largo la colonna (max 360 dp, allineato a sinistra), altezza 48 dp, raggio `md`, bordo `border` 1,5, due segmenti uguali. Segmento selezionato: fill `accentSoft`, bordo `accent` 2 dp, testo `ui.label` `ink` **con icona `check` 18 dp a sinistra** (la selezione non è solo colore: icona + peso del testo); non selezionato: fill `surface`, testo `ui.label` `inkSoft`. Contrasti: `ink`/`accentSoft` 13,9 / 8,6; `inkSoft`/`surface` 9,35 / 10,08. Pressed: `surfacePressed`; focus: anello 3 dp. Semantics: `selectableGroup`, ogni segmento `Role.RadioButton` con `selected`; etichette «Registro» e «Glossario». Tastiera: frecce ←→ cambiano segmento, Tab entra/esce dal gruppo.

- **Voce iniziale**: Registro. Si ricorda l'ultima voce nella sessione.
- Il cambio di voce non anima (dissolvenza `fast`), conserva lo scroll di ciascuna.
- Colonna `min(600, w − 2·margine)`; expanded: colonna 600 allineata con le altre tab (gruppo di 912 dp), **nessun pannello laterale**.
- **Ricarica**: i dati si rileggono (`GET …/learning-records?status=all`, `GET …/glossary`) all'apertura della tab, alla ripresa in primo piano e quando un job `lesson_completion` del Topic riesce. Nessun pull-to-refresh obbligatorio.

## 3. Registro («Cosa hai dimostrato»)

### 3.1 Layout

```
Cosa hai dimostrato                               ui.title2
Quello che hai fatto negli esercizi, non quello  ui.body inkSoft, max 600 dp
che hai solo letto. Lo uso per scegliere le
prossime lezioni.

┌──────────────────────────────────────────────┐ Surface static, padding 20 (16 compact)
│ (✓) Hai dimostrato                  Lezione 2 │ eyebrow: icona 18 inkSoft + ui.labelS inkSoft ; a destra «Lezione 2» ui.labelS inkSoft
│ Dopo un move la variabile di partenza non è  │ title: ui.title3 ink (a capo, mai ellissi)
│ più utilizzabile                             │
│ Pensavi che `a` restasse valida dopo         │ body: reading.body 17/28 ink (md inline)
│ `let b = a;`. Ora prevedi l'errore del       │
│ compilatore. Ti servirà con i riferimenti.   │
│ ▸ Come l'hai dimostrato                      │ Disclosure 48 dp (ui.label ink)
├──────────────────────────────────────────────┤
│ …                                            │
└──────────────────────────────────────────────┘
▸ Come è cambiata la tua comprensione            Disclosure 48 dp (record `superseded`)
```

Un record = **una card statica** (`Surface static`, `blockGap` 12 dp fra card): ogni record è un'unità autonoma e, a differenza delle fonti, ha testo lungo da leggere in sé. Ordine: `number` decrescente (più recente in cima), cioè l'ordine del contratto; **nessun raggruppamento per lezione**, nessun filtro.

### 3.2 Anatomia di un record (`RecordCard`)

| Elemento | Specifica |
|---|---|
| Eyebrow | icona Lucide 18 dp `inkSoft` + `ui.labelS` `inkSoft`; testo e icona dipendono da `kind` (tabella sotto). A destra «Lezione N» (`ui.labelS` `inkSoft`), che è un **Button testuale** di 48 dp (area di tocco) e apre il Riepilogo di quella lezione (se la lezione non è in cache si carica con `GET /lessons/{id}`; il numero viene dalla lista lezioni, altrimenti «Lezione» senza numero) |
| `title` | `ui.title3` `ink`, a capo libero. È una frase di competenza |
| `body` | `reading.body` 17/28 `ink`, md inline (grassetto, corsivo, codice inline; `[[ ]]` non previsti: se compaiono si mostra il testo senza markup) |
| «Come l'hai dimostrato» | `Disclosure` chiusa di default (la prova interessa quando la si cerca; tenere chiusa limita la lunghezza): dentro `evidence` in `ui.body` `inkSoft` |
| Padding / raggio | 20 (16 compact), `md`, bordo `line` 1 dp |

| `kind` | Eyebrow (icona + parole) | Perché |
|---|---|---|
| `demonstrated` | `check` · «Hai dimostrato» | Competenza: fatto, non lode |
| `misconception_corrected` | `lightbulb` · «Idea chiarita» | **Informazione, non colpa**: la parola dice che qualcosa si è chiarito, non che si è sbagliato. Nessun `x`, nessun rosso/ambra, nessuna parola come «errore», «sbagliato», «correzione». La card è identica a quella di `demonstrated` (stessa superficie, stessi colori): nessun record è "meno" di un altro. Anzi, correggere un'idea ha il valore più alto di apprendimento (ipercorrezione, Butterfield e Metcalfe), e il testo lo presenta così |
| sconosciuto | `check` · «Registrato» | tolleranza a versioni più nuove |

**Titolo e corpo parlano dell'idea giusta**: il `title` dice cosa si sa ora, mai l'idea sbagliata (rievocare l'errore lo rinforzerebbe). La convinzione precedente compare **una volta**, nel `body`, in forma «Pensavi che… Ora…». Regole per il Recorder in `../generator-guidelines.md`.

### 3.3 Record sostituiti (`superseded`)

**Sì, visibili, ma in fondo e chiusi.** Perché: mostrare *come è cambiata* la comprensione è una forma di metacognizione e di progresso ("prima sapevo questo, ora so di più"); nasconderli del tutto toglierebbe una storia vera. Ma competono con il presente: quindi non nella lista principale.

- In fondo alla lista, una `Disclosure` (48 dp, chevron + `ui.label` `ink`) «Come è cambiata la tua comprensione», chiusa di default, mostrata **solo se** esiste almeno un record `superseded`. Niente conteggio.
- Dentro, i record `superseded` con la stessa `RecordCard` ma **attenuati**: titolo `ui.title3` `inkSoft`, corpo `reading.body` `inkSoft` (contrasti `inkSoft`/`surface` 9,35 / 10,08, regge AAA); eyebrow `history` · «Superato da una comprensione più precisa». Sotto il corpo una riga con Button testuale (48 dp) «Vedi come si è approfondito» **se `superseded_by` è presente e il record successore è nell'elenco** → scorre alla `RecordCard` del successore e la evidenzia per 1,5 s (bordo `accent` 2 dp, poi torna `line`; con «riduci animazioni» il bordo resta fino al tocco altrove). Se `superseded_by` è `null` o il record non è nell'elenco: nessun bottone, solo «Superato».
- Ordine: `number` decrescente. Nessuna scheda di ripasso o azione sui record superati.
- Parole: mai «obsoleto», «vecchio», «errato».
- Dati: una sola richiesta `GET …/learning-records?status=all`, divisa lato app (`status`).

### 3.4 Stati della vista Registro

| Stato | Contenuto |
|---|---|
| Caricamento | skeleton: titolo + 2 riquadri card (barre `surfaceSunken`, niente shimmer), `stateDescription` «Caricamento» |
| Errore | `ErrorState` «Non riesco a caricare il registro.» + «Riprova» |
| **Vuoto** (0 record) | titolo `ui.title3` «Il registro si riempie con le lezioni», corpo `ui.bodyL` `ink`: «Quando completi una lezione, ci scrivo ciò che hai dimostrato negli esercizi. Ci finisce solo quello che hai fatto, non tutto quello che hai letto.» + (se esiste una lezione non `completed`) Button secondario «Vai alle lezioni»; se tutte le lezioni sono `completed` e il registro è vuoto, aggiunge sotto, `ui.body` `inkSoft`: «Se una lezione non ha aggiunto niente, è normale: succede quando gli esercizi sono ancora difficili.» |
| Solo record `superseded` (nessun `active`) | non dovrebbe succedere; si mostra il vuoto e la disclosure |
| Dopo un job `completion` riuscito mentre la tab è aperta | la lista si aggiorna; i nuovi record **non** si animano né si segnano «Nuovo»; annuncio Polite «Registro aggiornato.» |
| Offline con dati in cache | ultimo elenco noto, riga `ui.bodyS` `inkSoft` icona `cloud-off`: «Elenco dell'ultimo aggiornamento.» |

**Nessuna azione** sui record (modifica, eliminazione, segnalazione) in v1: il registro è memoria del sistema. Se un record è sbagliato l'utente lo dice al Tutor (non esiste endpoint): vedi Aperto. Nessun «Ripassa» (le schede arrivano in Fase 8): quando arriveranno, in fondo alla card comparirà un Button testuale; ora non si promette.

### 3.5 Accessibilità (Registro)

- Intestazione «Cosa hai dimostrato» `heading()` livello 2; ogni card un gruppo; `contentDescription` della card: «<eyebrow>. <title>. <body>. Dalla lezione N.» (senza markup), con `customActions` «Mostra come l'hai dimostrato» e «Apri la lezione N». Disclosure con `stateDescription` Aperto/Chiuso, focus ring 3 dp.
- Eyebrow e «Lezione N» non sono annunciati due volte (la card ha un solo nodo compatto, ma i due Button restano target separati ≥ 48 dp).
- Font 200 %: tutto a capo; eyebrow e «Lezione N» vanno su due righe; nessuna altezza fissa; a 328 dp × 200 % `reading.body` ≈ 22 caratteri per riga (limite noto del telefono).
- Ordine di lettura: titolo → intro → record dal più recente → disclosure finale.

## 4. Glossario

### 4.1 Layout

```
Glossario                                         ui.title2
Le parole che hai dimostrato di saper usare.      ui.body inkSoft

( 🔍 Cerca un termine )                           TeachTextField 48 dp, solo se i termini sono > 20
A                                                 ui.label inkSoft, 32 dp (lettera) — solo se > 12 termini
┌──────────────────────────────────────────────┐ Surface static, ListRow
│ (📖) borrowing                            ›  │ lead book-open 24 knowledge ; term ui.title3 ink
│      L'atto di prendere in prestito un       │ definition ui.body inkSoft, max 2 righe (ellissi ammessa: il testo intero sta nello sheet)
│      valore senza diventarne proprietari.    │
├──────────────────────────────────────────────┤
```

- **Dati**: `GET …/glossary` (ordine alfabetico del backend, senza distinzione di maiuscole, poi `key`). L'app **non riordina**. Le **iniziali** si ricavano da `term` (prima lettera, senza accento, maiuscola); cifre e simboli sotto «#». Le intestazioni di lettera compaiono solo se i termini sono più di 12 (con pochi termini sarebbero rumore).
- **Ricerca**: campo solo oltre 20 termini (un filtro che serve solo quando la lista è lunga: Hick). Filtra lato app, senza distinzione di maiuscole né accenti, su `term`, `key` e `definition`; nessun risultato: «Nessun termine con «xyz».» (`ui.body` `inkSoft`) + Button testuale «Cancella la ricerca». Il campo non è focalizzato all'apertura (niente tastiera improvvisa). `ImeAction.Search` chiude la tastiera.
- **Riga** (`ListRow` a 2 righe, min 72 dp, un solo target): `book-open` 24 `knowledge` (contrasto `knowledge`/`surface` 6,39 / 10,08) come lead, `term` `ui.title3` `ink` (a capo libero), prima parte della `definition` `ui.body` `inkSoft` (max 2 righe a font 1×; il `[[ ]]` non esiste nelle definizioni). Trailing `chevron-right`.
- Nessun conteggio di termini, nessuna lezione di origine nella riga (sta nello sheet), nessun «nuovo».

### 4.2 Sheet del termine

Lo stesso `GlossarySheet` di `../components/glossary_term.md` (BottomSheet su ogni classe; su expanded larghezza 480 centrata), **esteso** con la riga di origine quando si apre dal glossario del Topic o dal riepilogo. Il contenuto usa `GlossaryTerm` (non il glossario del documento).

```
──────  (maniglia)
Termine                           ui.label knowledge
ownership                         ui.title2
La regola per cui ogni valore ha un solo proprietario, che lo libera quando esce di scope.   reading.body (md inline)
Evita:
  • proprietà                     ui.body inkSoft, elenco puntato (0-3)
Introdotto nella lezione 2 · rivisto nella 4     ui.body inkSoft (riga di origine)
[ Apri la lezione 4 ]  [ Chiudi ]   Button testuale 48 + secondario 48
```

- `Evita:` (etichetta `ui.label` `ink`) e l'elenco compaiono **solo se `avoid` non è vuoto**. Sono parole *da non usare* per questo concetto (sinonimi imprecisi o confondibili); l'elenco dice quali, senza giudicare («Evita:» e non «Errore comune»).
- **Riga di origine** (v1.5, `GlossaryTerm.first_lesson_id`; sostituisce la vecchia riga «Lezione 2 · titolo» e la riga «Rivisto nella lezione N» derivata da `updated_at`). Un solo nodo di testo, `ui.body` `inkSoft`, numeri dalla lista delle lezioni del Topic (`Lesson.number`):

| `first_lesson_id` | `lesson_id` | Riga |
|---|---|---|
| uuid A | uguale ad A | «Introdotto nella lezione 2» (mai rivisto: niente «rivisto») |
| uuid A | diverso (B) | «Introdotto nella lezione 2 · rivisto nella 4» (A = origine, B = ultima revisione) |
| `null` | B | «Rivisto nella lezione 4» (termine nato prima della v1.5 e già rivisto: la lezione d'origine non è nota, quindi **non** si scrive «introdotto» né si inventa un numero) |

  - **Un numero non risolvibile** (lezione assente dalla lista, lista non ancora caricata): si omette solo quella metà. Se non se ne risolve nessuna, la riga **non compare**; il bottone resta con l'etichetta generica «Apri la lezione» e apre `lesson_id` (deciso dal tech lead il 2026-11-18: dalla schermata Lezione la lista delle lezioni non è disponibile, e senza bottone la lezione non sarebbe raggiungibile); nessun errore, nessun segnaposto. Una lezione eliminata non lascia mai «lezione ?».
  - Nessun titolo di lezione nella riga (il numero basta, la riga resta corta anche a font 200 %); il titolo sta nel nome accessibile del bottone.
  - Motivazione didattica: dire *dove* un termine è nato e dove è stato ripreso è un indizio di richiamo («era quella con i borrow») e mostra che il glossario è vivo (spaced re-exposure, Bjork) senza numeri né conteggi (P9).
- **Bottone «Apri la lezione N»**: apre sempre la lezione di `lesson_id` (**l'ultima** che ha toccato il termine), con il numero nell'etichetta («Apri la lezione 4») così non c'è ambiguità quando la riga ne cita due. Motivo: la definizione mostrata è quella dell'ultima revisione, e quella lezione contiene l'esercizio che la ha messa alla prova; la lezione d'origine si raggiunge dalla tab Lezioni. Destinazione: Riepilogo della lezione (`lesson.md` §7) se completata, altrimenti la lezione alla posizione salvata. Se `lesson_id` non è risolvibile (vedi sopra) il bottone **non compare** (resta «Chiudi»), anche se l'origine sì.
- Se `updated_at` ≠ `created_at` ma `first_lesson_id` è `null`: vale la terza riga (nessun'altra informazione da `updated_at`).
- **Campo assente** (backend precedente alla v1.5): si tratta come `null` esplicito. Il dato mancante non cambia la regola: la riga dice solo ciò che si sa.
- **TalkBack**: la riga è un nodo letto così com'è; il bottone ha `contentDescription` «Apri la lezione 4: <titolo>» (inizia con l'etichetta visibile, WCAG 2.5.3).
- Focus iniziale sul titolo; Esc/Indietro/scrim/«Chiudi» chiudono e restituiscono il focus alla riga. Non cambia schermata e non sposta lo scroll.
- **Chat**: nessun bottone «Chiedi al Tutor» nello sheet in v1 (la consultazione resta pura, P1).

### 4.3 Stati della vista Glossario

| Stato | Contenuto |
|---|---|
| Caricamento / errore | come il Registro: skeleton di 4 righe; `ErrorState` «Non riesco a caricare il glossario.» + «Riprova» |
| **Vuoto** | titolo «Il glossario si riempie con le lezioni»; corpo `ui.bodyL` `ink`: «Ci finiscono i termini che dimostri di saper usare negli esercizi, non tutti quelli che incontri. Così contiene solo parole tue.»; senza bottone |
| Ricerca senza risultati | vedi §4.1 |
| Aggiornamento dopo un job riuscito | la lista si aggiorna senza animazioni; annuncio Polite «Glossario aggiornato.» |

### 4.4 Accessibilità (Glossario)

- Riga: un nodo «<term>. <definizione, prima frase>. Apri la definizione» con `Role.Button`. Intestazioni di lettera `heading()` livello 3. Campo di ricerca con etichetta «Cerca un termine» e `contentDescription` con il numero di risultati dopo il filtro (`liveRegion` Polite con debounce 500 ms).
- Tastiera: Tab fra campo e righe; frecce ↑↓ fra le righe; Invio apre lo sheet.
- Font 200 %: righe a capo, la definizione in anteprima passa da 2 a 3 righe.

## 5. Motion

Solo quelle comuni (dissolvenza `fast` fra viste, `feedbackAppear` 200 ms per pannelli, disclosure istantanea con «riduci animazioni»). Nessuna celebrazione per un record nuovo (P9: la competenza è la ricompensa, non un'animazione).

## 6. Costanti

`GlossarySearchThreshold = 20` · `GlossaryLetterHeadersThreshold = 12` · `RecordCardPadding = 20.dp (16.dp compact)` · `RecordHighlightDuration = 1500.ms` · `SegmentedSwitchMaxWidth = 360.dp` · `RegisterStatusParam = "all"`.

## 7. Proposte ai contratti (per il tech lead)

1. ~~**`GlossaryTerm.first_lesson_id`**~~ **Applicata** (`lessons.md` §2, v1.5): specifica in §4.2.
2. **`LearningRecord` in seconda persona**: l'esempio di §2 (`body`: «Pensava che…», `evidence`: «Ha sbagliato…») è in terza persona, ma l'app mostra questi testi **alla persona**: leggersi in terza persona suona clinico e distanzia. Proposta: dichiarare nel contratto che `body` e `evidence` sono in **seconda persona** («Pensavi che…», «Hai scelto…»); `title` impersonale-affermativo (frase di competenza). Il Tutor li legge benissimo comunque. `misconception` del Grader resta in terza persona (non è mostrata). Dettagli in `../generator-guidelines.md`.
3. **`LearningRecord.block_ids`** (facoltativo, solo per il riepilogo): permetterebbe di collegare «Rivedi» e le frasi di competenza agli esercizi esatti. Non necessario in v1: il riepilogo funziona senza.
4. **Registro e glossario filtrabili per lezione** (`?lesson_id=`): non serve alla v1 (il riepilogo usa `completion`, la tab mostra tutto).

## 8. Aperto

- **Correzione di un record sbagliato** da parte dell'utente (record impreciso, valutazione ingiusta): non c'è endpoint né azione in v1. Un record scorretto sul registro vale per il Tutor e per il Planner. Suggerisco una azione futura «Segnala al Tutor» (apre la chat con il record come contesto) più che «Elimina»; da decidere con il proprietario.
- **Schede di ripasso** (Fase 8): quando esisteranno API e scheduling, ogni record/termine potrà avere «Ripassa»; la scheda è già legata a un record o termine.
