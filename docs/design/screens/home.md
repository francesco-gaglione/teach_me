# Schermata Home

> Raffinata in Fase 1 (cartelle). Componenti: `../base-components.md` (TopBar, ListRow, Surface, Button, StatusPanel, Toast, `InputDialog` §10.1, `ConfirmDialog` §8, `ActionMenu` §10.2, Skeleton §10.4, `ErrorState` §10.5). Token: `../tokens.json`. Terminologia: `../design-system.md` §11 (si dice **argomento**, mai "Topic").

## Scopo didattico

> **Fase 11**: l'icona Impostazioni ora porta anche a utente, uscita, password e chiave (`settings.md`). La banda sulla chiave LLM è in `../llm-key.md` §4.1.

Home risponde a: **"che cosa faccio ora per imparare?"**. Priorità a regime: (1) il ripasso di oggi (il ritorno spaziato fissa la memoria, Bjork); (2) riprendere la lezione in corso; (3) navigare fra cartelle. Niente streak né contatori di attività (P9).

**Stato per fase.** Fase 1: solo il punto (3). **Fase 8: esiste il punto (1)**, specificato in fondo (§ «Ripasso di oggi»). Il punto (2), «Riprendi la lezione», entra in Home con il contratto v1.8 (§«Riprendi la lezione»). Finché una card non è vera non si mostra, nemmeno come placeholder: direbbe una cosa non vera e occuperebbe attenzione con rumore (P2).

## Layout (compact, 360 dp)

Colonna scorrevole, margini 16 dp (`screenMarginCompact`), larghezza max `contentMaxWidth` 840 dp centrata, padding inferiore 88 dp per non coprire l'ultima riga con il bottone.

```
┌──────────────────────────────────────┐
│ teach_me                       ⚙     │  TopBar: ui.title2; IconButton "Impostazioni" 48 dp
├──────────────────────────────────────┤
│ Cartelle                             │  ui.label, inkSoft, heading (24 dp sopra, 8 dp sotto)
│ ┌──────────────────────────────────┐ │  Surface static, righe separate da hairline
│ │ ▢  Lavoro                    ⋮  │ │  ListRow 72 dp: icona folder 24 | titolo ui.title3
│ │    3 argomenti                   │ │  | sottotitolo ui.body inkSoft | IconButton ⋮ 48 dp
│ ├──────────────────────────────────┤ │
│ │ ▢  Hobby                     ⋮  │ │
│ │    Nessun argomento              │ │
│ └──────────────────────────────────┘ │
│                                      │
│        [ +  Nuova cartella ]         │  Button primario grande 56 dp, ancorato in basso
└──────────────────────────────────────┘  sopra l'inset di sistema, margine 16 dp
```

- **Riga cartella**: l'intera riga (escluso ⋮) è un target → apre la Cartella. Sottotitolo = conteggio dal campo `topic_count` ("Nessun argomento" / "1 argomento" / "n argomenti"). Nessun chevron (il ⋮ occupa lo slot trailing; la riga è chiaramente tappabile per pressed state e per il ruolo). Nome a capo fino a 2 righe a font normale, senza limite a font grande, mai ellissi.
- **⋮**: apre `ActionMenu` con **Rinomina** (icona `pencil`) e, separata, **Elimina** (icona `trash-2`, `danger`). Stesse azioni con pressione lunga sulla riga e come `customActions` TalkBack.
- **Ordine**: come arriva dal backend (`position`, poi id). Una nuova cartella finisce in fondo; dopo la creazione la lista scorre fino alla nuova riga.
- **Nuova cartella**: apre `InputDialog` (vedi sotto). Nello stato vuoto il bottone ancorato **non c'è**: la CTA è una sola (un solo primario per schermata, base-components §1).
- Pull-to-refresh: non previsto. La lista si ricarica a ogni `ON_START` della schermata (ritorno da Cartella, da Impostazioni, dall'app in background) senza skeleton se ci sono già dati (aggiornamento silenzioso).

## Stati

| Stato | Contenuto |
|---|---|
| **Caricamento (primo)** | Titolo "Cartelle" + skeleton di 3 righe (§10.4). Nessun bottone "Nuova cartella" finché non si sa se la lista è vuota. Dopo 8 s: "Sto ancora caricando…". Il timeout di rete (10 s) porta a *Errore di caricamento*. |
| **Vuoto** (nessuna cartella) | Niente illustrazione. Centrato nello spazio: `ui.title1` "Inizia da una cartella"; `ui.bodyL` "Una cartella raccoglie gli argomenti che vuoi studiare, ad esempio *Lavoro* o *Hobby*."; Button primario grande "Crea la prima cartella". Larghezza testo max `readingMaxWidth`. |
| **Lista** | Come in layout. |
| **Errore di caricamento (nessun dato)** | `ErrorState`: `StatusPanel danger` titolo "Non riesco a raggiungere il backend", corpo "Controlla che il dispositivo sia collegato alla rete e che il backend sia acceso." Bottoni: primario "Riprova", testuale "Apri le impostazioni". Un `401` non si mostra qui: apre l'accesso (`login.md`, regola globale della Fase 11). Errore `5xx`: titolo "Il backend ha avuto un problema", corpo "Riprova fra poco. Se continua, controlla i log del backend.", primario "Riprova". |
| **Errore di aggiornamento (dati già visibili)** | Banner `StatusPanel danger` sopra "Cartelle": "Non riesco ad aggiornare l'elenco." + bottoni testuali "Riprova", "Chiudi". La lista resta com'è. Scompare da solo al primo caricamento riuscito. |
| **Senza sessione** (primo avvio, sessione finita) | Home non si vede: si apre l'accesso (`login.md`). |
| **Senza chiave LLM** (`has_llm_key = false` o rifiuto noto) | Banda di suggerimento in cima, non bloccante: `../llm-key.md` §4.1. Non sostituisce nessuno degli stati sopra. |

L'errore del **caricamento** è sempre mostrato sulla schermata; l'errore di **un'azione** (crea, rinomina, elimina) è sempre mostrato **dentro il dialog dell'azione**, che resta aperto (vedi sotto): mai un toast di errore, mai un errore che compare lontano dal gesto (contiguità, P4).

## Crea cartella

Tocco su "Nuova cartella" / "Crea la prima cartella" → `InputDialog` (§10.1):

| Elemento | Testo |
|---|---|
| Titolo | Nuova cartella |
| Label | Nome della cartella |
| Placeholder | Es. Lavoro |
| Limite | 80 caratteri (contatore da 64) |
| Primario / loading | Crea / Creo… |
| Secondario | Annulla |

Successo (`201`): il dialog si chiude; la nuova riga compare in fondo e la lista scorre fino a lei; Toast "Cartella «Lavoro» creata" (4 s, live region Polite, senza azione). Il focus TalkBack va sulla nuova riga. **Si resta su Home**: la cartella è un contenitore, entrarci subito non serve a un'idea per volta (P1); chi vuole la apre.

Testi di errore (campo `name`, sotto il campo, icona `alert-circle` + testo `danger`):

| Caso | Testo |
|---|---|
| `empty` (locale o server) | Dai un nome alla cartella. |
| `too_long` (locale in tempo reale o server) | Al massimo 80 caratteri: ora sono 92. *(il numero è la lunghezza attuale; dal server, senza numero: "Al massimo 80 caratteri.")* |
| `duplicate` (server) | Hai già una cartella con questo nome. Scegline un altro. |

Errori dell'azione (`StatusPanel danger` nel dialog, sopra le azioni; il testo digitato resta):

| Caso | Titolo | Corpo |
|---|---|---|
| Rete / timeout | Non riesco a raggiungere il backend | La cartella non è stata creata. Riprova quando sei di nuovo collegato. |
| `401` | — | Nessun pannello: regola globale, si apre l'accesso (`login.md`). Il testo digitato nel dialog è perso solo se l'utente rientra con un utente diverso. |
| `5xx` o altro | Qualcosa è andato storto | La cartella non è stata creata. Riprova fra poco. |

## Rinomina cartella

`InputDialog`, titolo "Rinomina cartella", label e limiti come sopra, campo precompilato e selezionato, primario "Salva" / "Salvo…". Successo: dialog chiuso, riga aggiornata, Toast "Cartella rinominata in «Lavoro e studio»". Errori: stessi testi; per rete "La cartella non è stata rinominata."; `404` (eliminata altrove): dialog chiuso, lista ricaricata, Toast "Questa cartella non esiste più."

## Elimina cartella

`ConfirmDialog` (§8) con distruttivo. Il numero è `topic_count` della riga.

| Caso | Titolo | Corpo | Conferma |
|---|---|---|---|
| ≥ 2 argomenti | Eliminare la cartella «Lavoro»? | Contiene 3 argomenti. Verranno eliminati anche loro e tutto ciò che contengono. Non si può annullare. | Elimina cartella e argomenti |
| 1 argomento | Eliminare la cartella «Lavoro»? | Contiene 1 argomento. Verrà eliminato anche lui e tutto ciò che contiene. Non si può annullare. | Elimina cartella e argomento |
| vuota | Eliminare la cartella «Lavoro»? | La cartella è vuota. Non si può annullare. | Elimina cartella |

Annulla (testuale, focus iniziale) · conferma = Button distruttivo. L'etichetta di conferma dice cosa si perde: non c'è un "Sì". Il nome non si tronca nel titolo (va a capo). Nessun campo "scrivi il nome per confermare": la conferma nominativa con conteggio + focus su Annulla + etichetta esplicita bastano per questa app personale; da riconsiderare se arrivano più utenti.

- In corso: conferma "Elimino…" (loading), Annulla disabilitato.
- Successo (`204`): dialog chiuso, riga rimossa; Toast "Cartella «Lavoro» eliminata" (nessun "Annulla": l'operazione è irreversibile e dirlo con un'azione finta sarebbe falso). Focus alla riga successiva (o precedente; se la lista è vuota, al titolo dello stato vuoto).
- `404` = già eliminata: trattata come successo (stesso Toast).
- Errore rete/5xx: dialog aperto con `StatusPanel`: "Non riesco a raggiungere il backend" / "La cartella non è stata eliminata. Riprova quando sei di nuovo collegato."

## Accessibilità

- Ordine TalkBack: TopBar (titolo come heading, poi Impostazioni) → [banner errore se c'è] → heading "Cartelle" → righe in ordine → "Nuova cartella". Stato vuoto: titolo (heading) → testo → bottone.
- Riga cartella: un elemento, `Role.Button`, `contentDescription` = "Lavoro, 3 argomenti"; azioni personalizzate "Rinomina", "Elimina"; il ⋮ è un secondo target ("Altre azioni per Lavoro", `Role.Button`, 48 × 48 dp). Distanza fra i due target ≥ 8 dp.
- Skeleton: un solo elemento "Caricamento delle cartelle".
- Annunci (live region Polite tramite Toast): creata, rinominata, eliminata, come sopra. Gli errori nei dialog sono live region (si annunciano da soli).
- Font 200%: la riga cresce (titolo a capo, sottotitolo sotto); il bottone "Nuova cartella" resta 56 dp min e può andare a capo su due righe; la lista ha padding inferiore proporzionale (88 dp × fontScale).
- Dialog: focus trap, focus iniziale come da §8 / §10.1; alla chiusura il focus torna all'origine (⋮ o bottone), tranne dopo creazione/eliminazione (vedi sopra).
- Tema: nessun colore nuovo; `danger` verificato ≥ 6,65 su `surface` e ≥ 5,32 su `dangerSoft` (light), ≥ 7,23 (dark).

## Evitare

Card o testi non ancora reali, badge di qualsiasi tipo (anche sull'icona dell'app), parole come «in ritardo» o «arretrato», contatori globali ("12 argomenti in totale"), badge, ordinamento "per ultimo usato" non definito dal contratto, errori in toast per azioni che l'utente deve ritentare.

## Expanded / tablet orizzontale (layout principale, ADR 0003)

Classe expanded (≥ 840 dp, riferimento 1280 × 800). Home e Cartella **diventano un'unica schermata list-detail a 2 pannelli**: la schermata Cartella di `folder.md` non esiste come destinazione separata, è il pannello destro. Regole generali e misure: `../design-system.md` §5.1.

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ teach_me                                                                           ⚙   │ TopBar 56, tutta larghezza
├─────────────────────────┬──────────────────────────────────────────────────────────────┤
│ Cartelle                │ Lavoro                       3 argomenti   ⋮  [ + Nuovo      │ intestazioni 72 dp
│                         │                                               argomento ]   │
│ ┃ ▢ Lavoro          ⋮  │ ┌──────────────────────────────────────────────────────────┐ │
│ ┃   3 argomenti         │ │ Rust                                                  ⋮ │ │ lista argomenti:
│   ▢ Hobby           ⋮  │ │ ( 💬 Definiamo la mission )                               │ │ colonna centrata,
│     Nessun argomento    │ ├──────────────────────────────────────────────────────────┤ │ max 840 dp
│   ▢ Studio          ⋮  │ │ Fotografia                                            ⋮ │ │
│     1 argomento         │ │ ( ✓ Pronto per le lezioni )                               │ │
│                         │ ├──────────────────────────────────────────────────────────┤ │
│                         │ │ Contabilità                                           ⋮ │ │
│ [ + Nuova cartella ]    │ └──────────────────────────────────────────────────────────┘ │
│        360 dp           │                        ~919 dp                               │
└─────────────────────────┴──────────────────────────────────────────────────────────────┘
```

**Pannello sinistro "Cartelle"** (360 dp, `surfaceSunken`)
- Intestazione 72 dp: titolo "Cartelle" (`ui.title2`). Sotto, lista scorrevole di `ListRow` identiche al compact (72 dp, icona, nome, conteggio, ⋮ 48 dp, pressione lunga = menu), senza `Surface` incorniciata: righe separate da hairline, margini laterali 16 dp.
- **Selezione persistente**: la riga selezionata ha fill `accentSoft`, barra verticale `accent` di 4 dp sul bordo sinistro, titolo `ui.title3` in peso 600 e `semantics { selected = true }`. Non solo colore: barra + peso. Il tocco su una riga cambia la selezione; non c'è un secondo tocco "per entrare".
- **Nuova cartella**: Button **secondario** a tutta larghezza (margini 16), ancorato in fondo al pannello sopra l'inset di sistema, 48 dp. È secondario perché l'azione frequente è sull'argomento (pannello destro); in stato "nessuna cartella" diventa la CTA primaria (vedi sotto).
- Rinomina/Elimina: ⋮ della riga (qualunque riga, anche non selezionata; non cambia la selezione) e dal ⋮ dell'intestazione destra per la cartella selezionata. Dialog identici al compact, centrati nella finestra (`../base-components.md` §11).

**Pannello destro** = contenuto della cartella selezionata (specifica in `folder.md`, sezione Expanded): intestazione con nome (`ui.title1`, a capo fino a 2 righe), conteggio, ⋮ e "Nuovo argomento" (primario); sotto la lista argomenti.

**Selezione iniziale**: la lista è sempre accompagnata da un contenuto. All'avvio e dopo il primo caricamento si seleziona (nell'ordine) la **cartella selezionata l'ultima volta** (salvata in `SavedStateHandle` e, per i riavvii, nelle preferenze locali), altrimenti la **prima**. Mostrare il pannello destro vuoto "per scelta" costringerebbe a un tocco senza informazione in cambio. Il pannello destro vuoto "Scegli una cartella" esiste solo come ripiego (selezione non risolvibile per un istante dopo un `404`): `ui.bodyL` `inkSoft` centrato, testo "Scegli una cartella per vedere i suoi argomenti.", nessuna illustrazione.

**Stati**
| Stato | Expanded |
|---|---|
| Caricamento | Skeleton di 3 righe nel pannello sinistro; pannello destro vuoto (solo `bg`, nessuno skeleton: non si sa ancora quale cartella). |
| **Nessuna cartella** | Niente pannelli: una sola colonna centrata a `readingMaxWidth` con lo stato vuoto del compact (titolo, testo, primario grande "Crea la prima cartella"). Il divisore appare solo quando c'è la prima cartella (creata, diventa selezionata). |
| Errore di caricamento | `ErrorState` nel pannello sinistro se fallisce l'elenco; se fallisce solo il contenuto della cartella, `ErrorState` nel pannello destro (il sinistro resta usabile). |
| Errore di aggiornamento | Banner sopra la lista del pannello interessato. |

**Creazione**: dopo "Crea" la nuova cartella **diventa selezionata** (si va al suo pannello destro vuoto con "Crea il primo argomento": in expanded il passo successivo è visibile subito, senza navigare), la lista a sinistra scorre fino a lei; Toast come in compact.

**Eliminare la cartella selezionata**: dialog come in compact. A successo: Toast, la selezione passa alla **cartella successiva**, se non c'è alla **precedente**, se non ci sono più cartelle si passa allo stato "Nessuna cartella". Il focus va alla riga selezionata. Eliminare una cartella **non** selezionata non cambia la selezione né il pannello destro. `404` su una cartella: trattato come eliminata (stessa regola).

**Rinomina**: la riga e il titolo del pannello destro si aggiornano insieme.

**Back**: Home expanded è la radice; Indietro esce dall'app (la selezione non è un passo di navigazione: cambiare cartella non riempie lo stack). Aprire un argomento lo apre a tutta finestra sopra il list-detail; Indietro lo chiude e restituisce il list-detail con selezione e scroll invariati.

**Transizioni compact ↔ expanded** (rotazione, split-screen, ridimensionamento): la cartella selezionata vive nel ViewModel come id.
- compact → expanded: se si era in `Cartella(id)`, expanded mostra il list-detail con `id` selezionata; se si era in Home, si seleziona la cartella salvata o la prima. Un argomento aperto resta aperto.
- expanded → compact: se la selezione è stata fatta **dall'utente** (tocco, creazione) si mostra `Cartella(id)` sopra Home (Indietro → lista), perché è ciò che stava guardando; se era la selezione automatica non toccata, si mostra Home. Scroll e dialog aperti sopravvivono (testo dei campi incluso).
- medium: come compact.

**Accessibilità (aggiunte)**: ordine di focus TopBar → pannello sinistro (titolo, righe, "Nuova cartella") → pannello destro (titolo, azioni, righe). Ogni pannello è un `paneTitle` ("Cartelle", "Argomenti di Lavoro"). Cambiando la selezione si annuncia "Lavoro, selezionata" e il pannello destro non ruba il focus. Con tastiera fisica: frecce su/giù fra le righe, Invio seleziona, Tab passa al pannello destro. Font 200%: il pannello sinistro resta 360 dp (nomi a capo, righe più alte); sotto 1000 dp di larghezza utile e fontScale > 1,5 si passa al layout impilato (come medium) per non lasciare < 480 dp al contenuto.


---

## Ripasso di oggi (Fase 8)

Spec della schermata che la card apre: `review.md`. Dati: `GET /reviews/due?limit=1` → `due_count`, `estimated_minutes`, `next_due_at` (`reviews.md` §3.1). Si interroga in parallelo alle cartelle a ogni `ON_START` (dopo un flush della coda dei voti, max 2 s: `review.md` §8).

### Perché sopra «Cartelle»

Il ripasso è l'azione che più aiuta a ricordare (Bjork) e dura pochi minuti: è la prima risposta a «che cosa faccio ora?». Sta **sopra** la lista e non la sostituisce; sotto non cambia nulla.

### Cosa si mostra (decisione)

| Situazione | Cosa compare |
|---|---|
| `due_count` ≥ 1 | **Card «Ripasso di oggi»** (sotto) |
| `due_count` = 0 e `next_due_at` ≠ `null` | **Una riga discreta, non una card**: icona `layers` 20 `inkSoft` + `ui.body` `inkSoft` «Nessun ripasso per ora. La prossima scheda arriva domani.» (frase di `review.md` §4.5). Non tappabile, senza bordo, senza fill |
| `due_count` = 0 e `next_due_at` = `null` (nessuna scheda mai creata, o tutte sospese) | **Niente**. Dire «Nessun ripasso» a chi non ha mai chiuso una lezione sarebbe rumore (P2) |
| Errore o dati non ancora arrivati | **Niente** durante il caricamento. Se fallisce senza dati in cache: riga `cloud-off` 20 `inkSoft` + «Non riesco a controllare il ripasso.» + Button testuale «Riprova» (48 dp). Se ci sono dati recenti in cache: si mostrano quelli, in silenzio |
| Nessuna cartella (stato vuoto) | Niente: non c'è ancora nulla da ripassare |

Perché la riga a `due_count` = 0 e non il nulla: l'utente deve poter **dedurre che il sistema c'è e che non è in arretrato** («va bene così, la prossima arriva domani»): toglie l'ansia del «forse dovrei controllare» (P9) con una riga, senza una card vuota che chieda attenzione (P2). Il tono è neutro: niente spunta verde, niente «Complimenti», niente «Hai finito tutto».

**Niente salti di layout**: la lista delle cartelle non aspetta il ripasso più di **600 ms**; se la risposta arriva dopo, la card si inserisce con dissolvenza + espansione in altezza 200 ms (istantanea con «riduci animazioni») e il focus TalkBack non si sposta.

### Card «Ripasso di oggi» (compact, 360 dp)

```
┌──────────────────────────────────────┐
│ teach_me                       ⚙     │
├──────────────────────────────────────┤
│ ┌──────────────────────────────────┐ │  Surface statica: fill surface, bordo line 1, raggio md,
│ │ ▤  Ripasso di oggi               │ │  padding 16. Icona layers 24 ink | titolo ui.title3 ink
│ │    12 schede · circa 3 minuti    │ │  ui.body inkSoft (a capo se serve)
│ │                                  │ │
│ │ [      Inizia il ripasso       ] │ │  Button PRIMARIO 48 dp, a tutta larghezza della card
│ └──────────────────────────────────┘ │
│ Cartelle                             │  24 dp sopra (gap dalla card), come prima
│ ...                                  │
│        [ +  Nuova cartella ]         │  da SECONDARIO mentre la card è visibile (vedi sotto)
└──────────────────────────────────────┘
```

| Elemento | Testo e regole |
|---|---|
| Titolo | «Ripasso di oggi» (heading) |
| Riga di stato | «{n} schede · circa {m} minuti». 1 scheda: «1 scheda · circa 1 minuto». `n = min(due_count, 20)` (la sessione è di 20: `review.md` §7); `m = ceil(n × 15 / 60)` (con `due_count` ≤ 20 coincide con `estimated_minutes`; sopra 20 lo calcola l'app, perché il campo del backend è sull'intero arretrato) |
| Seconda riga, solo se `due_count` > 20 | `ui.bodyS` `inkSoft`: «In tutto ne hai 34 in scadenza. Ne ripassi 20 per volta.» (mai «arretrato», mai «in ritardo») |
| Pulsante | «Inizia il ripasso». Un solo primario nella card. Apre `review.md` a tutta finestra |
| Card nel complesso | non è tappabile: **solo il pulsante** apre il ripasso (un solo target, niente ambiguità fra card e bottone) |
| Colori | solo token esistenti. Nessun colore di urgenza, nessun rosso, nessun badge numerico, nessuna animazione di richiamo (pulsazioni, bagliori) |

**Un solo primario per schermata.** Quando la card è visibile, «Nuova cartella» (compact: ancorato in basso) passa da **primario** grande a **secondario** grande (56 dp); quando la card non c'è, torna primario com'era. In expanded «Nuova cartella» è già secondario e la card sta nel pannello sinistro (un primario per pannello: `design-system.md` §5.1).

### Filtro per argomento (chip)

Dati: `topics: [{topic_id, topic_title, due_count}]` di `GET /reviews/due` (`reviews.md` v1.5), già ordinato per `due_count` decrescente e poi titolo; calcolato sempre su tutte le schede. Il filtro è **solo per scegliere cosa ripassare ora**: non cambia la card di altro.

**Quando compare**: solo se `topics` ha **≥ 2 voci**. Con un solo argomento il filtro non ha scelta da offrire (P2): niente chip.

**Dove**: dentro la card, **sotto la riga di stato** (e sotto la seconda riga «In tutto ne hai…» se c'è), gap 12 dp, **sopra** il pulsante (gap 16 dp). Così la sequenza si legge «quanto → su cosa → via» e il primario resta l'ultimo elemento della card.

```
│ ▤  Ripasso di oggi                    │
│    12 schede · circa 3 minuti         │
│ [Tutti (12)] [Rust (8)] [SQL (4)] ▸   │  riga di chip, scorre in orizzontale
│ [        Inizia il ripasso         ]  │
```

| Elemento | Regola |
|---|---|
| Componente | `FilterChip` a **selezione singola**, costruito con token esistenti: altezza visiva 32 dp, **area tappabile 48 dp** (padding verticale invisibile), raggio `full`, padding orizzontale 12 dp, gap 8 dp. Testo `ui.label`. Non selezionato: fill `surface`, bordo `border` 1,5 dp, testo `ink`. **Selezionato**: fill `accentSoft`, bordo `accent` 1,5 dp, testo `ink` (`ink`/`accentSoft` 13,9 / 8,6) **e icona `check` 16 `ink` a sinistra**: lo stato non è solo colore |
| Etichette | «Tutti (12)» · «Rust (8)» · «SQL (4)». Il numero è `due_count` (somma reale, non limitata a 20: dice quante ce ne sono, il tetto lo dice la riga di stato). «Tutti» = somma dei `due_count` di `topics`. Titolo lungo: una riga, ellissi a 24 caratteri visibili (nodo a11y completo) |
| Predefinito | **«Tutti» sempre selezionato** all'apertura di Home e ad ogni ritorno in primo piano dopo una sessione. **Non ricordato fra sessioni** né fra riavvii: motivazione didattica, **interleaving**: mescolare gli argomenti produce un richiamo più duraturo che ripassarli a blocchi (Rohrer e Taylor, Bjork), quindi il default deve restare il misto; filtrare è una scelta consapevole e occasionale. La selezione vive solo nello stato della schermata |
| Ordine | quello di `topics` (più schede prima, poi titolo). «Tutti» sempre primo |
| Compact | la riga di chip **scorre in orizzontale** (non va a capo: la card non cresce con il numero di argomenti), margine sinistro = padding card, l'ultimo chip resta **tagliato a destra** come indizio di scorrimento se non entra. Il chip selezionato è riportato in vista. Nessuna scorciatoia di scroll personalizzata |
| Expanded (card 328 dp) | stessa riga scorrevole. Con tastiera: ←/→ fra i chip, Invio/Spazio selezionano |
| Font 200% | la riga **diventa una colonna a tutta larghezza** (chip a 48 dp min, uno sotto l'altro, testo a capo, nessuna ellissi) (niente pulsante «Altri» aggiuntivo: la colonna scorre insieme alla Home). Così nessun testo si perde e nessun target scorre di lato a un utente con ingrandimento |
| Con filtro attivo | la riga di stato diventa «{n} schede · circa {m} minuti» calcolata **su quell'argomento** (stessa formula: `n = min(due_count, 20)`, `m = ceil(n × 15 / 60)`; es. «8 schede · circa 2 minuti»; 1: «1 scheda · circa 1 minuto»). La seconda riga, se `due_count` > 20: «In tutto ne hai 34 di Rust in scadenza. Ne ripassi 20 per volta.» Titolo card invariato «Ripasso di oggi». **Pulsante**: «Ripassa Rust» (con «Tutti»: «Inizia il ripasso»). Apre `review.md` §7.3 con `topic_id` |
| Aggiornamento | il tocco su un chip aggiorna riga di stato e pulsante **dai dati già in mano** (`topics[].due_count`), senza richiesta e senza animazione (istantaneo: nessun salto di altezza, il numero cambia al posto). Il `GET` con `topic_id` parte solo all'apertura del ripasso |
| Argomento sparito | se al ritorno in Home il chip selezionato non è più in `topics` (ripassato tutto o eliminato): torna «Tutti» in silenzio, senza messaggio |
| Mai | colore di urgenza, badge, percentuali, un «consigliato» su un argomento, ordine per «più in ritardo» annunciato come tale |

**TalkBack**: ordine card → titolo (heading) → riga di stato → gruppo di chip (`selectableGroup`, annuncio «Filtra per argomento») → pulsante. Ogni chip: `Role.RadioButton`, `selected`, descrizione «Tutti, 12 schede, selezionato» / «Rust, 8 schede, non selezionato». Al cambio selezione la riga di stato è `liveRegion = Polite` («8 schede, circa 2 minuti») e il pulsante ha `contentDescription` «Ripassa Rust, 8 schede, circa 2 minuti». Il focus resta sul chip toccato.

### Expanded (≥ 840 dp)

Nel **pannello sinistro** (360 dp), **sopra** l'intestazione «Cartelle»: card o riga discreta, margini laterali 16, larghezza 328, gap 16 dp dall'intestazione. Il resto del pannello (intestazione, lista, «Nuova cartella» secondario in fondo) non cambia. Pulsante «Inizia il ripasso» primario 48 dp a tutta larghezza della card. Il pannello destro non cambia: il ripasso **non** è un contenuto della cartella selezionata.

```
┌─────────────────────────┬──────────────────────────────────┐
│ teach_me            ⚙   │                                  │
├─────────────────────────┤ Lavoro        3 argomenti  ⋮  [+]│
│ ┌─────────────────────┐ │ ...                              │
│ │ ▤ Ripasso di oggi   │ │                                  │
│ │   12 schede · circa │ │                                  │
│ │   3 minuti          │ │                                  │
│ │ [Inizia il ripasso] │ │                                  │
│ └─────────────────────┘ │                                  │
│ Cartelle                │                                  │
│ ┃ ▢ Lavoro          ⋮  │                                  │
└─────────────────────────┴──────────────────────────────────┘
```

Il ripasso si apre sopra il list-detail a tutta finestra (come un argomento); Indietro/✕ restituisce Home con selezione e scroll invariati. Con font 200% il pannello resta 360 dp: la card cresce in altezza, il resto scorre.

### Accessibilità (aggiunte)

- Ordine TalkBack: TopBar → [banner errore] → card (titolo come heading → riga di stato → «Inizia il ripasso») oppure la riga discreta → «Cartelle» → righe → «Nuova cartella».
- Pulsante: `contentDescription` «Inizia il ripasso, 12 schede, circa 3 minuti». Riga discreta: testo letto per intero, nessun ruolo.
- Nessun annuncio automatico all'apparire della card (non è un evento).
- Font 200%: titolo e riga di stato a capo, pulsante min 48 dp che può andare su due righe.

### Riprendi la lezione (Fase 8b, contratto `lessons.md` v1.8)

Fonte: `GET /lessons?status=in_progress&limit=1` (primo elemento = attività più recente; `Lesson` + `topic_title` + `last_activity_at`). La card compare **sotto** quella del ripasso. Motivo del cambio rispetto alla decisione precedente: ora esiste la query globale, e la card ripaga il ritorno nel punto in cui si era (effetto Zeigarnik, senza ansia: nessun contatore).

**Compact (360 dp)**: `Surface` statica come la card del ripasso (fill `surface`, bordo `line` 1, raggio `md`, padding 16), gap 16 dp dalla card sopra. Icona `book-open` 24 `ink` | titolo lezione `ui.title3` `ink` (max 2 righe, poi a capo libero: mai ellissi) · riga argomento `ui.body` `inkSoft` («Rust in pratica», il `topic_title`) · riga di stato `ui.bodyS` `inkSoft`: «Lezione {N} · circa {M} minuti» (`N = number`, `M = estimated_minutes`; 1 minuto: «circa 1 minuto»). Sotto, `Button` **secondario** «Riprendi la lezione» 48 dp, a tutta larghezza. Un solo primario per schermata resta quello del ripasso (o «Nuova cartella» se la card ripasso manca: in quel caso la card Riprendi non fa cambiare la regola di §«Ripasso di oggi»).

**Expanded**: la card sta nel pannello sinistro (360 dp) sotto quella del ripasso, stesso layout; il list-detail a destra non cambia. Il tocco apre la lezione **sopra il list-detail a tutta finestra**, come il ripasso; Indietro/✕ restituisce Home con selezione e scroll invariati.

**Stati**: lista vuota → card **assente**, nessun segnaposto. Errore o caricamento → card assente, **nessun banner** (non è un dato critico: il banner d'errore di Home resta per le cartelle). La card appare senza shimmer quando arriva la risposta e non sposta lo scroll se l'utente è già sceso (inserimento sotto la card ripasso, altrimenti sopra la lista). Si aggiorna al ritorno in Home. Solo il pulsante è tappabile (come il ripasso).

**Accessibilità**: ordine TalkBack: … card ripasso → card Riprendi (titolo lezione come heading → argomento → riga di stato → «Riprendi la lezione») → «Cartelle». `contentDescription` del pulsante: «Riprendi la lezione {titolo}, argomento {topic_title}, lezione {N}, circa {M} minuti». Nessun annuncio all'apparire. Font 200%: titolo, argomento e stato vanno a capo, il pulsante (min 48 dp) può andare su due righe e la card cresce in altezza. Nessun componente nuovo: `Surface`, `Button` secondario e le righe di testo già usati dalla card del ripasso.
