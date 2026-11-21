# Mission: card di proposta, vista Mission e preferenze (Fase 3)

> Contratto dati: `../../contracts/mission.md`. Componenti base: `../base-components.md` (§13 nuovi). Tablet orizzontale per primo (ADR 0003), poi compact. Terminologia: «mission», «Tutor», «argomento» (mai «Topic», mai `mission_proposed` grezzo). Dal contratto v1.2 (Fase 12) la mission ha la sezione «Punto di partenza»: specifica in §2.5, §4.3 e §4.6.

## 1. Scopo didattico

La mission è la decisione che guida **tutte** le lezioni: se i criteri di successo sono vaghi o sbagliati, ogni lezione sarà fuori bersaglio. Quindi la card non è un "OK/Annulla" da liquidare: è un momento di **valutazione attiva**.

- **Elaborazione attiva / retrieval (Bjork)**: sopra i bottoni c'è una domanda («Riconosci te stesso in questi criteri?»), non una formula di cortesia. Confermare costa un tocco, ma la domanda invita a confrontare i criteri con ciò che l'utente vuole davvero.
- **Segnalazione (Mayer)**: i *criteri di successo* sono il blocco evidenziato (`accentSoft`) e numerato; perché, vincoli e fuori ambito sono più quieti. Si evidenzia ciò su cui l'utente deve decidere.
- **Carico cognitivo (Sweller)**: una sola cosa da fare per schermata. Le versioni vecchie e le card superate sono **ripiegate**; la proposta aperta è l'unico elemento "caldo".
- **Contiguità**: i numeri dei criteri permettono di rispondere «il 2 è troppo ampio» nel composer, sotto la card.
- **Errore = occasione**: un 409 o un errore di rete non dà colpa; si spiega cosa è successo e si aggiorna la card da sola.
- **Onestà**: dopo la conferma si dice chiaramente cosa l'app *non* fa ancora (§5), senza promettere date.

## 2. La card di proposta (`MissionCard`)

È una **parte** del `TutorMessage` (chat.md §10): blocco a larghezza piena colonna dopo il testo del Tutor, gap 12 dp, fuori dal `SelectionContainer`, ignorata dal footer «Copia». Lo stesso componente è usato nella vista Mission (§4) per la bozza in attesa.

### 2.1 Anatomia (bozza aperta)

```
|<------------------- 600 dp (colonna) ------------------->|
┌─────────────────────────────────────────────────────────┐ bordo border 1,5 dp, fill surface, raggio md
│ ⚑  Proposta di mission · versione 2                     │ BANDA infoSoft, padding 12×16 (icona + ui.labelL, colore info)
├─────────────────────────────────────────────────────────┤
│  Perché                                      ui.label inkSoft (heading)
│  Mantenere il backend di pagamenti scritto   ui.bodyL ink
│  in Rust nel mio nuovo lavoro.
│                                                          │ gap 16
│  Punto di partenza                          ui.label inkSoft (heading)
│  Programmo in Kotlin da tre anni e ho       ui.body ink (prosa, non elenco)
│  scritto un po' di C; di Rust ho letto
│  solo i primi capitoli, mai scritto codice.
│                                                          │ gap 16
│ ┌─────────────────────────────────────────────────────┐ │ blocco accentSoft, raggio sm, padding 16
│ │ Come capirai di esserci riuscito      ui.title3 ink │ │
│ │ 1  Leggo e modifico un servizio axum   ui.bodyL     │ │ numero ui.labelL inkSoft in colonna 28 dp
│ │    esistente senza aiuto                            │ │ gap tra voci 8 dp
│ │ 2  Scrivo test per un modulo con tokio              │ │
│ └─────────────────────────────────────────────────────┘ │
│                                                          │ gap 16
│  Vincoli                                    ui.label inkSoft
│  •  30 minuti al giorno                     ui.body ink
│                                                          │
│  Fuori dal percorso, per ora                ui.label inkSoft
│  •  Rust embedded                           ui.body ink
│  •  macro procedurali
│                                                          │ gap 20
│  Ti riconosci in questi criteri? Se qualcosa non        │ ui.bodyS inkSoft (domanda di valutazione)
│  torna, dimmelo nel messaggio qui sotto.                │
│                                                          │ gap 12
│                        [ Modifica ] [ Conferma la mission ] │ azioni a destra, gap 8, 48 dp
└─────────────────────────────────────────────────────────┘
```

Misure (nomi per il codice):

| Elemento | Valore |
|---|---|
| Contenitore | `Surface static` ma bordo `border` 1,5 dp (la bozza aperta è l'unica card "calda" della chat); raggio `md` 10 dp, clip della banda |
| Padding contenuto | 20 dp (expanded/medium), 16 dp (compact); banda 12 × 16 dp |
| Banda | fill `infoSoft`; icona `flag` 20 dp + etichetta `ui.labelL` in `info` (5,53 / 5,55). Testo: «Proposta di mission» + « · versione N» (la versione si omette alla v1) |
| Etichetta di sezione | `ui.label`, `inkSoft`, `heading()` livello 3; 4 dp sopra al contenuto |
| Perché | `ui.bodyL` `ink` (è il testo dell'utente: sans, non serif; la card è interfaccia nativa, non lettura del Tutor) |
| Criteri | blocco `accentSoft` (ink su accentSoft 13,9 / 8,6), raggio `sm`, padding 16; titolo `ui.title3`; voci `ui.bodyL` `ink`, numero in colonna di 28 dp (`ui.labelL`, `inkSoft`: 7,59 / 6,48 su accentSoft); gap 8 dp |
| Punto di partenza | Dopo «Perché», prima del blocco criteri. Vedi §2.5 (resa, ordine, differenze). **Sezione omessa se `starting_point` è `null`/assente** |
| Vincoli / Fuori ambito | etichetta + elenco con punto «•» (colonna 20 dp), voci `ui.body` `ink`, gap 4 dp. **Sezione omessa se l'array è vuoto** (nessun «Nessuno») |
| Domanda di valutazione | `ui.bodyS` `inkSoft`, 20 dp sopra, 12 dp sotto |
| Azioni | `Button` secondario «Modifica» + primario «Conferma la mission» (48 dp). Allineate a destra; con fontScale > 1,3 o colonna < 360 dp: in colonna a tutta larghezza, **Conferma sopra**, Modifica sotto (gap 8) |

Motivazioni: i numeri servono solo ai criteri (si cita «il 2»); vincoli e fuori ambito sono elenchi senza ordine, quindi punti. I criteri restano sempre **interi e visibili**: nessuna ellissi, nessun «Mostra altro» sulla bozza aperta (nascondere ciò che si deve valutare è il difetto da evitare). Il testo massimo è già limitato dal contratto (5 voci × 200 caratteri).

**Proposta di modifica** (esiste una mission confermata): la banda dice «Proposta di modifica · versione 3» e sotto la banda compare, prima di «Perché», una riga `ui.bodyS` `inkSoft`: «Sostituirà la versione 2, confermata il 3 novembre.» Differenze rispetto alla versione confermata (confronto esatto di stringhe, lato app):
- voce nuova o modificata in criteri/vincoli/fuori ambito: etichetta testuale **«Nuovo»** (`ui.labelS`, `info` su `infoSoft`, pill 20 dp di altezza visiva) a fine voce (a capo se manca spazio); «Perché» cambiato: stessa pill accanto all'etichetta di sezione;
- voci della versione confermata che non ci sono più: sezione `ui.bodyS` `inkMuted` «Tolto dalla versione 2» con le voci (barrate **e** precedute da icona `minus` 16 dp: la barratura da sola non basta). In fondo alla sezione, ripiegata se > 2 voci («Mostra le N voci tolte»).
Nessuna differenza rilevata (testo identico): niente pill.

### 2.2 Stati

| Stato | Quando | Aspetto |
|---|---|---|
| **aperta** | `mission.version == draft.version` e nessun turno in corso | come §2.1, bottoni attivi |
| **in arrivo** | il messaggio ha `status = streaming` (la parte compare già intera, §2.4) o c'è un turno in corso | stessa card, **entrambi i bottoni disabilitati** con motivo visibile sopra le azioni (`ui.bodyS`, `inkSoft`, sostituisce la domanda): «Il Tutor sta ancora scrivendo. Potrai confermare tra un attimo.» Motivo: evitare una conferma mentre sta per arrivare una bozza nuova (→ 409) |
| **in conferma** | tocco su Conferma, POST in corso | «Conferma la mission» diventa loading («Confermo…», spinner 20 dp, `busy`); «Modifica» disabilitato; il composer resta usabile ma l'invio è sospeso fino all'esito (motivo nel composer: «Sto confermando la mission…») |
| **confermata** | `mission.version == current.version` | banda `correctSoft`, icona `check-circle`, «Mission confermata» (`correct`); sotto: «Confermata il 8 novembre.» (`ui.bodyS` `inkSoft`), *Perché* in `ui.body` (max 3 righe, ellissi), le altre sezioni **ripiegate**; testo `ui.label` accent «Vedi la mission» (Button testuale 48 dp: apre la tab Mission). Se il Topic è `researching` e la ricerca non c'è ancora, vedi frase onesta in §5 |
| **sostituita** | versione ≠ `current` e ≠ `draft` (nuova bozza o nuova versione confermata) | banda `surfaceSunken`, icona `history`, «Sostituita da una proposta più recente» (`inkSoft`, 8,15 / 11,30); contenuto ripiegato dietro il Button testuale «Mostra questa versione» (`aria-expanded`); espansa, mostra le sezioni in sola lettura come §2.1 senza criteri evidenziati (blocco `surfaceSunken`) e senza azioni |
| **errore di conferma** (rete/5xx) | POST fallito | la card torna *aperta*; sopra le azioni `StatusPanel danger` (liveRegion): titolo «Non sono riuscito a confermare», corpo «La mission non è stata salvata. Controlla la connessione e riprova.»; il primario si chiama «Riprova». Nessun toast |
| **chiave LLM mancante o rifiutata** (`409 llm_key_missing`, `502 llm_key_rejected` alla conferma, che avvia la ricerca) | la mission non è confermata | la card resta *aperta*; al posto del pannello `danger`, `LlmKeyNotice` (`../llm-key.md` §5; `502 llm_credit_exhausted`: pannello `credit` con «Riprova», §9.2). Se il backend conferma comunque e la ricerca fallisce con quei codici, vale `resources.md` §4.4 (`../llm-key.md` §6) |
| **conflitto 409** | la bozza non è più quella aperta | nessun pannello di errore: l'app ricarica `GET /mission` e **ricalcola lo stato** (sostituita / confermata altrove); Toast `ui.bodyL`: «La mission era cambiata: ho aggiornato la proposta.» Se la card diventa «confermata», niente toast, solo l'annuncio di §6 |
| **422/versione non valida** | non dovrebbe accadere | come errore di conferma; non mostrare il dettaglio tecnico |

Lo stato si ricalcola a ogni `mission_proposed`, `mission_confirmed`, `topic_updated` e al ritorno in primo piano: nessun flag nel testo.

### 2.3 Modifica

- «Modifica» **non apre nulla**: mette il focus sul composer (tastiera aperta), porta il composer in vista e per quella digitazione cambia il **placeholder** in «Cosa vuoi cambiare nella mission? Puoi citare il numero del criterio.» (torna a «Scrivi un messaggio…» all'invio o a campo vuoto senza focus). Non pre-compila testo (l'utente deve formulare con parole sue: elaborazione attiva).
- Dopo l'invio il Topic torna `interviewing` (chip «Definiamo la mission»), ma la card **resta aperta** e confermabile finché non arriva una nuova bozza: è corretto, la bozza non è stata ritirata. Mentre il Tutor risponde vale lo stato «in arrivo».
- Nella vista Mission (§4) «Modifica» passa alla tab Chat e mette il focus sul composer con lo stesso placeholder.
- Il composer **resta sempre attivo** accanto alla card (chat.md §10): discutere a parole è sempre possibile.

### 2.4 Streaming e arrivo della card

1. Il testo di introduzione del Tutor arriva in streaming come sempre (chat.md §6).
2. La parte `mission_proposal` arriva **intera, dopo il testo**: compare con `feedbackAppear` (200 ms, dissolvenza + altezza; nessun movimento con «riduci animazioni») sotto l'ultimo paragrafo, **senza** spostare il testo sopra.
3. Auto-scroll: se l'utente era in fondo, la lista scorre **fino a far coincidere il bordo superiore della card con il bordo superiore dell'area visibile**, ma mai oltre (deve poter leggere la banda e il perché prima dei criteri); se la card è più bassa dell'area (altezza < ≈ 480 dp), resta ancorata in alto e i bottoni si raggiungono scorrendo. Se l'utente non era in fondo: nessuno scroll, compare la pillola «Nuovi messaggi».
4. Bottoni disabilitati («in arrivo») finché il messaggio non è `complete` (di norma frazioni di secondo).
5. Il Tutor prima della card riassume in 2-3 righe (chat.md §12.10): la card non deve ripetere quel riassunto a parole; è la versione *strutturata* da valutare.

### 2.5 Punto di partenza (v1.2, Fase 12)

**Ordine delle sezioni** (card e vista): Perché → **Punto di partenza** → Come capirai di esserci riuscito (criteri) → Vincoli → Fuori dal percorso, per ora.

Motivazione didattica: la lettura segue il percorso *perché voglio arrivarci → da dove parto → dove devo arrivare → con quali limiti*. Mettere il punto di partenza **subito prima** dei criteri permette di giudicarli per ciò che sono (troppo ambiziosi o troppo ovvi *per me*): è la valutazione attiva che la card chiede (§1; Bjork). Il sapere pregresso è la variabile che più condiziona l'apprendimento (Ausubel) e la persona deve vedere che il Tutor l'ha capito prima di accettare ciò che le insegnerà (e che non le ripeterà ciò che sa). Non va **dopo** i criteri (si giudicherebbero senza base) né **dopo** vincoli e fuori ambito (sono quelle sezioni a restare quiete). Non va nei vincoli (contratto §2: sono cose diverse).

**Resa**: è prosa (fino a 600 caratteri), non un elenco: niente punti, niente numeri, nessuna ricomposizione in frasi.
- Etichetta: «Punto di partenza», stessa resa delle altre (`ui.label` `inkSoft`, `heading()` livello 3).
- Corpo: `ui.body` `ink` (un gradino sotto «Perché» `ui.bodyL`: la gerarchia resta Perché > Punto di partenza, e il blocco `accentSoft` dei criteri resta l'unico elemento evidenziato; Segnalazione di Mayer). Sans, come il resto della card. Allineato a sinistra, `AnnotatedString` semplice, **nessun markdown** (il campo è testo semplice); a capo `\n` rispettati come interruzione di riga. Selezionabile.
- Larghezza: la colonna della card è 600 dp (560 dp utili): con `ui.body` sono ≈ 65-75 caratteri per riga, dentro la misura 45-75 (design-system). 600 caratteri ≈ 9 righe su tablet. Su compact (328 dp utili a 100%) ≈ 38 caratteri/riga, ≈ 16 righe: accettato come per gli altri testi della card.
- **Nessun troncamento** nella bozza aperta, nella proposta di modifica e nella vista Mission: come i criteri, è ciò che si deve valutare (§2.1). Niente ellissi, niente «Mostra altro».
- Nelle altre card: **confermata** → sezione ripiegata come le altre (non compare; resta «Vedi la mission»); **sostituita** → compare nell'espansione «Mostra questa versione», in sola lettura, senza pill né confronto.
- Se `starting_point` ha solo spazi o è vuoto (non dovrebbe: il backend lo rifiuta): trattare come `null`.

**Differenze quando una bozza sostituisce una versione confermata** (§2.1, stesso confronto esatto di stringhe; la base è la versione `replaces_version`: è `current` se la versione coincide, altrimenti la riga di `…/versions`; `null` e chiave assente equivalgono):

| Confermata | Bozza | Resa |
|---|---|---|
| `null` | testo | Pill «Nuovo» accanto all'etichetta (come «Perché» cambiato). È il caso atteso di chi aggiunge il punto di partenza a una mission precedente: nessuna riga «Prima». Se **solo** questo cambia, la riga «Sostituirà la versione N…» basta da contesto |
| testo A | testo B ≠ A | Pill «Nuovo» accanto all'etichetta + sotto il testo un `Disclosure` compatto (riga 48 dp, `ui.bodyS` `inkSoft`, chevron) **chiuso** «Com'era nella versione 2»; aperto, mostra il testo A in `ui.bodyS` `inkMuted`, preceduto dall'icona `history` 16 dp (il precedente non è mai indicato dal solo grigio). Motivo: in una prosa di 600 caratteri la persona non può vedere *quale* parola è cambiata; il confronto a richiesta evita il carico di due blocchi lunghi affiancati (Sweller) |
| testo A | testo A | nessuna pill |
| testo A | `null`/assente | non dovrebbe accadere (il tool lo impone). Difesa: la sezione non compare e al suo posto riga `ui.bodyS` `inkMuted` con icona `minus` 16 dp «Il punto di partenza della versione 2 non c'è più.» |

La pill «Nuovo» e la sua resa accessibile sono quelle già specificate (`ui.labelS`, `info` su `infoSoft`, letta «…, nuovo»). Il blocco criteri e le altre sezioni mantengono le proprie regole.

**Senza punto di partenza (`null`)**: nella **card** la sezione non compare e non c'è alcun invito (la card è una proposta del Tutor da valutare: non è il luogo per rimproverare un dato mancante; un nuovo draft lo ha sempre, §8 del contratto). L'invito vive solo nella vista Mission (§4.6).

## 3. Cosa succede dopo la conferma

1. Tocco su «Conferma la mission» → stato *in conferma* → `200`: la card passa a **confermata** (crossfade `fast` 120 ms); il focus accessibile si sposta sulla banda della card (altrimenti sparisce insieme al bottone).
2. Il chip diventa quello di `researching` (§5); il `ContextPane` mostra la mission (§4.5); il Toast non serve (l'esito è visibile nella card).
3. **Nessun messaggio nuovo in chat** (contratto). Nessuna transizione automatica di tab o di schermata: l'utente decide se guardare la tab Mission («Vedi la mission» nella card).
4. Frase onesta (§5) nella card e nella tab Mission.
5. Il composer resta attivo: se il Tutor vede dalla chat qualcosa da cambiare, proporrà un cambio (`propose_mission_change`) e comparirà una nuova card «Proposta di modifica».

## 4. Vista Mission dell'argomento

### 4.1 Dove vive: decisione

Dalla Fase 3 l'argomento ha due contenuti: **Chat** e **Mission**. Quindi, per la regola «`TabRow` solo con ≥ 2 tab» (topic.md), compare la `TabRow` con le tab **Chat · Mission** su **tutte** le classi, subito sotto il chip di stato. Mission è sempre disponibile: anche vuota ha contenuto vero (stato vuoto che spiega il termine, preferenze), quindi non è mai una tab disabilitata.

Il `ContextPane` laterale (≥ 1100 dp) **resta** nella tab Chat, ma come **sintesi** (perché + criteri) con il link «Apri la mission» che cambia tab. Non diventa la vista completa: la chat va usata con un contesto *breve* accanto (carico cognitivo), mentre la vista completa (storico, preferenze, eliminazioni) è un luogo di gestione, che non deve competere con la conversazione. Sotto 1100 dp non c'è il pannello, e **il BottomSheet «Mission» ipotizzato in chat.md §2 decade**: la tab Mission lo sostituisce (un solo percorso per la stessa cosa).

- Tab iniziale: come topic.md (`new`/`interviewing`/`mission_proposed` → Chat; `researching` → Mission). Poi si ricorda l'ultima scelta **per sessione** nel ViewModel; cambiare tab non perde scroll, bozza del composer né stream in corso (stato conservato, nessuna ricarica).
- Nessun badge sulle tab. Una bozza in attesa si vede nel chip («Conferma la mission») e in cima alla tab Mission.
- TabRow: su expanded è lunga quanto l'area (bordo inferiore a tutta larghezza), le due etichette allineate a sinistra con margine 32 dp; su compact/medium come base-components §7. Con tastiera virtuale che riduce l'altezza utile a < 480 dp, TopBar, chip e TabRow scorrono via con il contenuto (come già chip/TopBar).

### 4.2 Layout expanded (≥ 1100 dp)

```
|<----------------------------- 1280 dp ----------------------------->|
┌──────────────────────────────────────────────────────────────────────┐
│ ←  Rust                                                          ⋮   │
│    ( ⚑ Conferma la mission )                                         │
│  Chat   Mission                                                      │ TabRow, Mission selezionata
│         ━━━━━━━                                                      │
├──────────────────────────────────────────────────────────────────────┤
│      ┌──────────── 600 ───────────┐ 32 ┌────── 280 ──────┐           │ gruppo centrato (912)
│      │ [MissionCard bozza aperta] │    │ Preferenze        │           │
│      │  (se esiste)               │    │ Come ti insegno.  │           │
│      │                            │    │ Per questo        │           │
│      │ Mission attuale            │    │ argomento         │           │
│      │ Versione 1 · confermata    │    │  Esempi in Python │ [🗑]     │
│      │ il 3 novembre              │    │ Per tutti gli     │           │
│      │ Perché …                   │    │ argomenti         │           │
│      │ [Come capirai…] (accentSoft)│   │  Niente analogie  │ [🗑]     │
│      │ Vincoli …                  │    │  sportive         │           │
│      │ Fuori dal percorso …       │    └───────────────────┘           │
│      │ ▸ Versioni precedenti (2)  │                                    │
│      └────────────────────────────┘                                    │
└──────────────────────────────────────────────────────────────────────┘
```

- La pagina scorre per intero (colonna Mission + colonna Preferenze), a differenza della chat: qui non c'è composer ancorato. Le due colonne sono allineate in alto; la colonna Preferenze è `contextPaneWidth` 280, senza riquadro. Gap 32.
- Larghezza < 1100 (expanded stretto, medium, compact): **una colonna** di `min(600, w − 2·margine)`, ordine: bozza → mission attuale → «Cosa succede ora» (se Fase 3 + researching) → versioni precedenti → preferenze. Margini 24 (medium) / 16 (compact). I titoli di sezione sono `ui.title3` con 32 dp sopra (24 compact).
- Medium/compact: come sopra; la `TabRow` a 2 etichette entra anche a font 200% (la tab si allarga e la riga scorre, base-components §7).

### 4.3 Contenuto, dall'alto

1. **Bozza in attesa** (se `draft != null`): titolo di sezione «Proposta in attesa» (`ui.title3`) e la `MissionCard` aperta di §2 (stesso componente: stesso stato, stessi bottoni, stesse regole). «Modifica» qui cambia tab verso la Chat. Se esiste anche una mission attuale la card è una «Proposta di modifica» con le differenze.
2. **Mission attuale** (`current != null`): titolo «La tua mission» + riga `ui.bodyS` `inkSoft` «Versione 2 · confermata il 8 novembre». `MissionSummary`: *Perché* (`ui.bodyL`), *Punto di partenza* (§2.5; con `null`: l'invito di §4.6), *Come capirai di esserci riuscito* (blocco `accentSoft` numerato, come nella card), *Vincoli*, *Fuori dal percorso, per ora*; sezioni vuote omesse. Nessun bottone «Modifica» sulla mission confermata: la modifica passa dal Tutor; sotto, `ui.bodyS` `inkSoft`: «Per cambiare la mission, scrivilo al Tutor: ti proporrà una nuova versione da confermare.» + Button testuale «Vai alla chat» (cambia tab e dà focus al composer, placeholder §2.3).
3. **Stato vuoto** (`current == null`):
   - senza bozza: titolo «La tua mission» e testo `ui.bodyL` `inkSoft`: «Ancora da definire. La mission è lo scopo per cui impari «Rust»: guida cosa ti insegno e in che ordine. La definiamo insieme in chat.» + Button secondario «Definiscila con il Tutor» (→ Chat, focus composer). Nessuna illustrazione, nessuna card finta.
   - con bozza: sotto la card, `ui.bodyL` `inkSoft`: «Non c'è ancora una mission confermata. Conferma la proposta qui sopra o scrivi al Tutor cosa cambiare.»
4. **Cosa succede ora** (solo Fase 3, Topic `researching`): `StatusPanel info`, vedi §5.
5. **Versioni precedenti**: `Disclosure` (riga 48 dp, chevron `chevron-right`/`chevron-down`, titolo `ui.label` + conteggio «Versioni precedenti (2)»), **chiusa di default**; compare solo se esistono versioni oltre la corrente/bozza (altrimenti non compare, niente «Nessuna»). Si carica al primo apertura (`GET …/mission/versions`, spinner 20 dp + «Carico le versioni…», errore: riga `ui.bodyS` + Button testuale «Riprova»). Righe 56 dp: «Versione 1» (`ui.body` 600) + meta `ui.bodyS` `inkSoft`: «confermata il 3 novembre» o «proposta non confermata, 3 novembre». Tocco → espande inline il `MissionSummary` in sola lettura (blocco criteri `surfaceSunken`, non evidenziato; il punto di partenza compare se presente, senza invito se `null`). Una sola riga espansa alla volta; più di 10 → «Mostra altre» (Button testuale). Nessun «Ripristina»: tornare a una versione si chiede al Tutor.
6. **Preferenze** (§4.4).

### 4.4 Preferenze

Colonna destra (expanded) o ultima sezione. Titolo `ui.title3` «Preferenze»; testo `ui.bodyS` `inkSoft`: «Come preferisci imparare. Le salva il Tutor quando le dici in chat.»

```
Per questo argomento            ui.label inkSoft (heading)
Esempi in Python, non in Java      [🗑]   riga: testo ui.body ink + IconButton 48 dp
Per tutti gli argomenti         ui.label inkSoft
Niente analogie sportive           [🗑]
Valgono anche per gli altri argomenti.   ui.bodyS inkSoft (sotto il gruppo globale)
```

- Dati: `GET /preferences?topic_id=` (globali + dell'argomento), raggruppati per `scope`; gruppo vuoto omesso; ordine di creazione.
- Riga: testo a capo senza ellissi (fino a 300 caratteri), selezionabile; cestino `trash-2` 20 dp `inkSoft`, area 48 × 48, `contentDescription` «Elimina la preferenza «Esempi in Python, non in Jav…»» (citazione troncata a 40 caratteri + «…»). Riga con `customAction` TalkBack «Elimina». Altezza min 56 dp; separatore `line` 1 dp tra le righe.
- **Eliminazione con conferma** (`ConfirmDialog`, base-components §8 e §11 su expanded):
  - titolo «Eliminare questa preferenza?»; corpo per `topic`: ««Esempi in Python, non in Java» non varrà più per «Rust».»; per `global`: ««Niente analogie sportive» non varrà più per **nessun argomento**.» (la parola evidenziata in grassetto);
  - azioni «Annulla» (focus iniziale) · «Elimina» (distruttivo). Esito: la riga sparisce (`feedbackAppear` inversa 200 ms), Toast «Preferenza eliminata.» (senza «Annulla»: non c'è creazione lato app). Errore di rete: `StatusPanel danger` nel dialog: «Non sono riuscito a eliminarla. Riprova.»; `404` = già eliminata → si chiude e si rimuove la riga in silenzio.
  - Focus dopo l'eliminazione: sulla riga seguente, o sul titolo «Preferenze» se era l'ultima.
- **Stato vuoto**: `ui.body` `inkSoft`: «Nessuna preferenza ancora. Se dici al Tutor come ti piace imparare (per esempio «esempi in Python» o «niente analogie sportive»), la ricorda e la trovi qui.» Senza bottone: l'app non crea preferenze (contratto).
- Nuova preferenza salvata (evento `preference_saved`): la riga compare con `feedbackAppear`; e un Toast (4 s, azione «Vedi» → tab Mission, scroll alle preferenze) «Ho salvato una preferenza.», così l'utente sa che il sistema ricorda. Nessun annuncio TalkBack oltre al toast (Polite).
- Caricamento: skeleton di 2 righe (barre `surfaceSunken`, niente shimmer); errore: riga `ui.bodyS` «Non riesco a caricare le preferenze.» + «Riprova». Un errore nelle preferenze non blocca la mission (sezioni indipendenti).

### 4.5 `ContextPane` nella tab Chat (≥ 1100 dp)

Fisso a destra, senza riquadro, `contextPaneWidth` 280, **con scroll proprio** se più alto della finestra. Titolo `ui.title3` «La tua mission». Contenuto per stato:

| Situazione | Contenuto |
|---|---|
| Nessuna mission né bozza (Fase 2, invariato) | «Ancora da definire. La mission è lo scopo per cui impari «Rust»: guida cosa ti insegno e in che ordine.» |
| Solo bozza | `ui.body` `inkSoft`: «C'è una proposta in attesa (versione 2). Leggila nella chat e decidi se confermarla.» + Button testuale «Vai alla proposta» (scorre la chat alla card e le dà il focus). Il pannello **non ripete** il contenuto della bozza (è già nella card: coerenza di Mayer) |
| Mission confermata | `ui.bodyS` `inkSoft` «Versione 2»; *Perché* (`ui.body`, max 4 righe) ; *Come capirai di esserci riuscito* (voci numerate `ui.body`, intere); Button testuale «Apri la mission» (→ tab Mission). Vincoli e fuori ambito **non** compaiono qui |
| Confermata + bozza di modifica | quanto sopra, più in cima `StatusPanel info` compatto (`ui.bodyS`): «Proposta di modifica in attesa.» + «Vai alla proposta» |

Il pannello non ha mai bottoni primari. Ordine di lettura: ultimo (chat.md §11).

### 4.6 Invito a dire il punto di partenza (mission confermata con `starting_point: null`)

**Decisione: sì, un invito discreto, solo nella vista Mission, solo sulla mission attuale.** Motivo: una mission salvata prima della v1.2 resta valida ma le lezioni ripartono da zero senza sapere cosa la persona conosce (ripetizioni che annoiano o salti che perdono; carico cognitivo per ridondanza). Il contratto dice che il Tutor propone il cambio *quando la persona parla di ciò che sa già*: se nessuno glielo ricorda, non lo dirà. L'invito è l'unico modo, per l'app, di far nascere quella conversazione. Deve però restare discreto: nessun badge, nessun avviso rosso/giallo, nessuna presenza nel chip, nelle tab, nella card in chat, nel `ContextPane`, né nelle versioni precedenti.

Posizione: nel punto in cui comparirebbe la sezione, cioè subito dopo «Perché» e prima dei criteri. Resa: **nessun riquadro e nessun fill**; stessa etichetta di sezione «Punto di partenza» (`ui.label` `inkSoft`, heading livello 3) e sotto:

```
Punto di partenza                       ui.label inkSoft (heading)
Non hai ancora detto al Tutor cosa sai già di «Rust».    ui.body inkSoft
Se lo scrivi, le lezioni partono da lì e non ti
ripetono ciò che conosci.
[ Scrivilo al Tutor ]                    Button testuale accent, 48 dp
```

- Testo: «Non hai ancora detto al Tutor cosa sai già di «Rust». Se lo scrivi, le lezioni partono da lì e non ti ripetono ciò che conosci.» (il nome dell'argomento tra «», come in §4.3; troncato a 40 caratteri + «…» se più lungo).
- Azione: Button testuale **«Scrivilo al Tutor»**, 48 dp. L'unica azione: passa alla tab **Chat** e mette il focus sul composer (tastiera aperta), con il placeholder specifico «Cosa sai già di questo argomento e cosa non hai mai visto?» (stesso meccanismo di §2.3: torna a «Scrivi un messaggio…» all'invio o a campo vuoto senza focus). **Non pre-compila** e non invia nulla. Non esiste un endpoint per modificare la mission: sarà il Tutor, dalla risposta, a proporre una nuova versione (comparirà la card «Proposta di modifica» con «Nuovo» sul punto di partenza, §2.5).
- Scompare quando: esiste una bozza in attesa con `starting_point` valorizzato (la proposta già lo risolve: l'invito diventerebbe rumore accanto a una card che lo contiene) o quando la mission attuale ha `starting_point`. Torna se la bozza viene sostituita da una senza.
- Non è dismissibile e non ha stato persistente: è una riga di testo quieta, non un banner; sparisce da sola alla prima conferma di una versione con il campo.
- Senza mission attuale (stato vuoto §4.3.3) l'invito non compare: la mission non c'è, e il Tutor lo chiede sempre nell'intervista.

## 5. Chip di stato e frase onesta per `researching`

Il chip è non interattivo. Testi in vigore dalla Fase 3 (sostituiscono le righe corrispondenti di `topic.md` e `folder.md`):

| `state` | Testo | Icona | Variante | Annuncio TalkBack (solo al cambio) |
|---|---|---|---|---|
| `new`, `interviewing` | Definiamo la mission | `message-circle` | neutral | nessuno (invariato) |
| `mission_proposed` | Conferma la mission | `flag` | info | «Stato: conferma la mission» (Polite) |
| `researching` **in Fase 3** | **Mission confermata** | `check-circle` | neutral | «Mission confermata.» (già annunciata dalla card; qui non si ripete) |
| `researching` dalla Fase 4 | Cerco le fonti… | `search` | neutral | — |
| `ready` | Pronto per le lezioni | `check-circle` | accent | invariato |

**Decisione**: finché la ricerca non esiste, il chip *non* dice «Cerco le fonti…» (sarebbe una promessa falsa e un'attesa infinita). Dice un fatto: la mission è confermata. Il testo dipende da un flag di funzionalità in app (`ResearchAvailable`, falso in Fase 3, da accendere in Fase 4), non da un controllo di versione sparso. La stessa regola vale nelle righe della lista (cartella): stesso testo.

**Frase onesta** (card confermata *più recente* e `StatusPanel info` nella tab Mission, mentre `state = researching` e `!ResearchAvailable`):
- titolo (solo nel pannello): «La mission è salvata»
- corpo: «Per ora non posso ancora cercare le fonti né creare lezioni: arriveranno nei prossimi aggiornamenti dell'app. Intanto puoi continuare a rifinire la mission con il Tutor.»
- nella card, stessa frase senza titolo, `ui.body` `inkSoft`, sotto «Confermata il …».
Nessuna data promessa, nessun avanzamento, nessuna barra indeterminata. Quando arriverà la Fase 4 il pannello si sostituisce con l'avanzamento reale (`progress`) e il flag cambia il chip.

## 6. Accessibilità

- **Annuncio all'arrivo**: al `message_completed` di un messaggio che contiene una proposta, la live region persistente (chat.md §11) dice **una volta** (Polite): «Il Tutor ha proposto una mission per Rust. Leggi i criteri di successo e scegli se confermarla.» al posto di «Il Tutor ha risposto.». Non si legge il contenuto della card. Se l'utente è sulla tab Mission e la bozza compare lì: stesso annuncio.
- **Struttura**: la card è un gruppo (`paneTitle` = «Proposta di mission, versione 2»); banda = heading; etichette di sezione = heading di livello 3; ogni criterio è un nodo: «Criterio 1 di 2: Leggo e modifico…»; ogni vincolo/fuori ambito: «Vincolo: …» / «Fuori dal percorso: …»; pill «Nuovo» letta in coda («…, nuovo»). Le voci tolte: «Tolto dalla versione 2: …».
- **Ordine di lettura**: banda → perché → **punto di partenza** → criteri → vincoli → fuori ambito → domanda → **Modifica** → **Conferma** (ordine visivo; su colonna è invertito: Conferma poi Modifica, coerente con ciò che si vede).
- **Punto di partenza**: etichetta = heading livello 3 («Punto di partenza», con «, nuovo» in coda se c'è la pill); il testo è **un solo nodo** letto subito dopo, in un unico paragrafo, senza dividere per frasi. `Disclosure` «Com'era nella versione 2»: `Role.Button`, `stateDescription` «Aperto»/«Chiuso»; il testo precedente è letto come «Prima, nella versione 2: …». Invito di §4.6: etichetta heading, testo, poi il bottone; `contentDescription` del bottone «Scrivilo al Tutor: scrivi cosa sai già di Rust» (inizia con l'etichetta visibile, WCAG 2.5.3). Dopo il tocco il focus accessibile va al composer e TalkBack legge il placeholder specifico.
- **Bottoni**: etichetta visibile e nome accessibile coerenti (WCAG 2.5.3): «Conferma la mission» → `contentDescription` «Conferma la mission per Rust»; «Modifica» → «Modifica la mission per Rust: scrivi cosa cambiare nel messaggio». Disabilitati: `disabled()` e il motivo è il nodo letto subito prima. Loading: `stateDescription` «In corso».
- **Esiti**: conferma riuscita → live region Polite «Mission confermata.» e focus sulla banda; errore → il `StatusPanel danger` è `liveRegion Polite` e il focus resta su «Riprova»; conflitto → toast Polite.
- **Stato sostituita**: «Sostituita da una proposta più recente. Contenuto nascosto.» e il bottone «Mostra questa versione» (`expanded` false/true).
- **Non solo colore**: stato = icona + parola nella banda; «Nuovo» e «Tolto» = parola + icona/pill; criteri evidenziati anche dal titolo e dai numeri.
- **Contrasti** (da design-system §2.4 e base-components §10.3): `info`/`infoSoft` 5,53 / 5,55; `ink`/`accentSoft` 13,9 / 8,6; `inkSoft`/`accentSoft` 7,59 / 6,48; `correct`/`correctSoft` come StatusPanel; `inkSoft`/`surfaceSunken` 8,15 / 11,30; `border`/`surface` 4,08 / 5,16 per il bordo della card; `ink`/`surface` e `inkSoft`/`bg` come le altre voci (la prosa del punto di partenza usa gli stessi ruoli: `ink`, `inkSoft`, `inkMuted`, `info`/`infoSoft`). Nessun nuovo colore.
- **Font 200%**: nessuna altezza fissa; bottoni in colonna a tutta larghezza oltre 1,3×; numeri dei criteri in colonna 28 dp (il testo va a capo accanto, indentato, non sotto il numero); banda a due righe; la card può superare l'altezza della finestra: scorre con la lista, nessuna card interna con scroll. A 328 dp × 200% il testo dei criteri è ≈ 18 caratteri per riga: limite noto del telefono, non si riduce.
- **Tastiera fisica**: Tab/Maiusc+Tab percorrono (nella card) «Mostra questa versione» se presente → Modifica → Conferma; anello 3 dp. Invio/Spazio attivano. **Ctrl+Invio nel composer non conferma** (azione irreversibile-ish, mai da scorciatoia ambigua). Esc: nel dialog di eliminazione = Annulla. Nella tab Mission: Tab percorre bozza, righe versione (Invio espande), cestini delle preferenze. Cambio tab: frecce sinistra/destra sulla `TabRow` come in base-components §7 (Invio/Spazio selezionano).
- **Disclosure**: `Role.Button` con `stateDescription` «Aperto»/«Chiuso»; il contenuto compare subito dopo nell'ordine.

## 7. Testi definitivi (riepilogo)

| Dove | Testo |
|---|---|
| Banda bozza | Proposta di mission · versione 2 · (Proposta di modifica · versione 3) |
| Sezioni | Perché · Punto di partenza · Come capirai di esserci riuscito · Vincoli · Fuori dal percorso, per ora |
| Punto di partenza | Nuovo · Com'era nella versione 2 · Il punto di partenza della versione 2 non c'è più. · Non hai ancora detto al Tutor cosa sai già di «Rust». Se lo scrivi, le lezioni partono da lì e non ti ripetono ciò che conosci. · Scrivilo al Tutor · placeholder: Cosa sai già di questo argomento e cosa non hai mai visto? |
| Domanda | Ti riconosci in questi criteri? Se qualcosa non torna, dimmelo nel messaggio qui sotto. |
| Bottoni | Conferma la mission · Modifica · Confermo… · Riprova · Mostra questa versione · Vedi la mission · Vai alla proposta · Apri la mission · Vai alla chat · Definiscila con il Tutor |
| Motivo disabilitato | Il Tutor sta ancora scrivendo. Potrai confermare tra un attimo. |
| Confermata | Mission confermata · Confermata il 8 novembre. |
| Sostituita | Sostituita da una proposta più recente |
| Errore | Non sono riuscito a confermare / La mission non è stata salvata. Controlla la connessione e riprova. |
| 409 | La mission era cambiata: ho aggiornato la proposta. |
| Onesto | La mission è salvata / Per ora non posso ancora cercare le fonti né creare lezioni: arriveranno nei prossimi aggiornamenti dell'app. Intanto puoi continuare a rifinire la mission con il Tutor. |
| Pref. | Preferenze · Per questo argomento · Per tutti gli argomenti · Eliminare questa preferenza? · Preferenza eliminata. · Ho salvato una preferenza. |

Date: «8 novembre» (anno solo se diverso dall'attuale), mai formati numerici.

## 8. Cosa evitare

- Confermare con un solo bottone «OK» o con gesti (swipe); dialog di «Sei sicuro?» sulla conferma (è reversibile: si propone un cambio).
- Nascondere i criteri dietro «Mostra altro» nella bozza aperta; mostrare «0 vincoli» / «Nessuno».
- Badge o contatori sulle tab, percentuali, «mission completata al 40%».
- Animazioni celebrative (coriandoli, check che rimbalza) alla conferma: la motivazione è intrinseca; la conferma è un fatto, non un premio.
- Dire «Cerco le fonti…» in Fase 3.
- Pulsante «Annulla»/«Scarta» sulla bozza: non c'è endpoint; l'utente dice al Tutor cosa cambiare.
- Punto di partenza: spezzarlo in punti o ricomporlo in frasi, troncarlo con ellissi nella bozza, metterlo nei vincoli o dopo i criteri, mostrare «Nessuno» o un invito nella card in chat, pre-compilare il composer, badge o avvisi di «mission incompleta», ripetere l'invito nel `ContextPane` o nel chip.
