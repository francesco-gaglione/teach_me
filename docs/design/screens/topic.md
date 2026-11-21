# Schermata Topic (con tab)

> **Tab: Lezioni · Chat · Cosa sai · Fonti · Mission.** Questo file descrive la struttura della schermata Argomento (TopBar, chip di stato, tab, expanded) e, nelle sezioni «Struttura delle tab» (F1-F9), la tab Lezioni. Le altre tab hanno la loro specifica: `chat.md`, `progress.md` («Cosa sai»), `resources.md` (Fonti), `mission.md`.

## Struttura della schermata

Dall'alto: TopBar (Indietro, titolo, ⋮) → chip di stato → `TabRow` → contenuto della tab. La `TabRow` compare solo quando le tab *disponibili* sono almeno due (una barra con voci vuote o disabilitate mostrerebbe funzioni che non esistono e ogni tab disabilitata ha bisogno di un motivo, base-components §1); oggi sono cinque e sempre presenti (F1).

```
┌──────────────────────────────────────┐
│ ←  Rust                        ⋮     │  TopBar ui.title2 (2 righe se fontScale > 1,3)
│    ( 💬 Definiamo la mission )       │  StatusChip sotto il titolo, 16 dp dai margini
├──────────────────────────────────────┤
│ Lezioni | Chat | Cosa sai | Fonti | … │  TabRow scrollabile (base-components §7)
│ ━━━━━━━                              │  indicatore 3dp accent
├──────────────────────────────────────┤
│  (contenuto della tab)               │
└──────────────────────────────────────┘
```

- **⋮**: `ActionMenu` "Rinomina argomento" · "Elimina argomento". Dialog e testi identici a `folder.md` (rinomina: `InputDialog` precompilato; elimina: `ConfirmDialog` «Eliminare l'argomento «Rust»?»). La rinomina aggiorna titolo e Toast; l'eliminazione torna alla Cartella con Toast "Argomento «Rust» eliminato".
- Stati: caricamento (titolo noto dalla navigazione, chip assente, testo "Sto caricando…"), errore di caricamento (`ErrorState`), `404` (torna alla Cartella con Toast "Questo argomento non esiste più.").
- Accessibilità: ordine Indietro → titolo (heading) → "Altre azioni" → chip ("Stato: definiamo la mission") → tab → contenuto.

### In expanded / tablet orizzontale

Nessun terzo pannello: l'argomento si apre **a tutta finestra** sopra il list-detail (motivazione: `../design-system.md` §5.1). TopBar come compact (Indietro, titolo, ⋮), chip di stato sotto il titolo, tab sopra il contenuto. Il contenuto sta in colonna a `readingMaxWidth` 600 con margini 32 dp; lo spazio laterale è riservato a contesto (mission in sintesi nella Chat, indice nella lezione), non a testo più largo. ⋮, dialog a 480 dp e 404 come in compact. Indietro → list-detail con la stessa cartella selezionata e lo stesso scroll.

### Chat nell'Argomento

Specifica completa della chat: `chat.md`. Qui solo ciò che riguarda la schermata Argomento.

- **Struttura (tutte le classi)**: area chat che riempie il resto (lista messaggi scrollabile + composer ancorato in basso). Expanded: colonna chat a `readingMaxWidth` 600 + pannello di contesto 280 dp a destra se la finestra ≥ 1100 dp (`chat.md` §2). Compact/medium: colonna sola.
- **Scroll con la tastiera aperta**: TopBar e chip **scorrono via** con la lista quando l'altezza utile è < 480 dp (tastiera virtuale in orizzontale, telefono); il composer resta sempre visibile sopra la tastiera. Con altezza sufficiente, TopBar e chip restano fissi.
- **Stato dell'argomento aperto**: titolo noto dalla navigazione; la cronologia si carica con skeleton (`chat.md` §4.4); errore del primo caricamento = `ErrorState`.
- **Eliminare/rinominare con un turno in corso**: l'eliminazione mostra il dialog di sempre; il testo del `ConfirmDialog` aggiunge, solo se un turno è in corso, «La risposta del Tutor in corso andrà persa.» Rinominare non interrompe nulla.
- **Uscire dalla schermata** durante lo streaming: lo stream SSE si chiude (contratto), il backend continua; al ritorno si ricaricano i messaggi e si riapre lo stream. Nessun avviso di uscita: nulla va perso (la bozza del composer si conserva, `chat.md` §8).
- **Accessibilità**: ordine Indietro → titolo → «Altre azioni» → chip → lista messaggi → composer → pannello di contesto (`chat.md` §11).

### Mission, chip di stato, eliminazione

Specifica completa: `mission.md`. Qui solo ciò che riguarda la schermata Argomento.

- **Chip di stato** (non interattivo, testo + icona, non solo colore):

| Stato | Testo | Icona |
|---|---|---|
| new / interviewing | Definiamo la mission | `message-circle` |
| mission_proposed | Conferma la mission | `flag` (variante info) |
| researching | Cerco le fonti… | `search` |
| researching, job fallito | Ricerca da riprovare | (vedi `resources.md`) |
| ready | Pronto per le lezioni | `check-circle` |

  `new` e `interviewing` mostrano lo stesso testo (per l'utente è la stessa fase): all'invio del primo messaggio il chip non cambia visibilmente e non c'è annuncio TalkBack per quella transizione.
- **ContextPane** (≥ 1100 dp, solo tab Chat): sintesi della mission, non la vista completa (`mission.md` §4.5). Sotto 1100 dp: nessun pannello, nessun BottomSheet.
- **Eliminare l'argomento**: il `ConfirmDialog` aggiunge, se esiste una mission, «Verranno eliminate anche la mission e le preferenze di questo argomento.» (le preferenze di un argomento vanno con esso, contratto).

## Scopo didattico

Il Topic è l'ambiente di studio di un argomento con una **mission** (perché lo imparo). La schermata deve rendere ovvio il *passo successivo* in base allo stato del Topic, e tenere le altre viste a un tocco ma non in competizione (P1).

## Contenuto delle tab

- **Lezioni**: card «Prossima lezione» e lista delle lezioni, F3-F5 (sotto). Un solo bottone primario; lo stato di una lezione è sempre testo + icona, mai un voto numerico.
- **Chat**: `chat.md`. I messaggi del Tutor sono testo di lettura su `bg`, a tutta colonna (senza riquadro); il messaggio dell'utente è in `accentSoft` a destra (max 80%); card nativa di mission (`chat.md` §10); pulsante «Nuovi messaggi».
- **Cosa sai**: Registro e Glossario, `progress.md`.
- **Fonti**: due gruppi **Knowledge** e **Community**, sezione **Lacune** in fondo se presente; `resources.md`.
- **Mission**: `mission.md` §4 (non c'è «Modifica» sulla mission confermata: si passa dal Tutor).

## Accessibilità

Tab con `Role.Tab`, selezione annunciata; chip di stato con `contentDescription` completo ("Stato: pronto per le lezioni"). Il job di generazione annuncia le fasi con live region `Polite`. Target ≥ 48 dp ovunque; la riga di lezione è un unico target.

## Evitare

Contatori di lezioni "da recuperare", badge sulle tab, percentuale "completamento del Topic" (non misura apprendimento).

## Fonti nella schermata Argomento

Specifica completa: `resources.md`.

- **Tab**: Fonti sempre presente (stato vuoto prima della conferma della mission), nessun badge.
- **Tab iniziale**: vedi F1 (`researching` → Fonti).
- **Chip**: `researching` = «Cerco le fonti…»; job fallito = «Ricerca da riprovare»; `ready` = «Pronto per le lezioni».
- **Puntatori** alla ricerca da card «Mission confermata», `ContextPane`, tab Mission e Toast: `resources.md` §7.

## Struttura delle tab: Lezioni e «Cosa sai»

> Contratto: `docs/contracts/lessons.md`, `jobs.md`. Specifica del Glossario e del Registro: `progress.md`. Riferimento: tablet orizzontale (1280 × 800 dp), poi medium e compact (ADR 0003). Terminologia: «lezione», «argomento», «registro», «Tutor» (mai «job», «Learning record», «Attempt»).

### F1. Struttura delle tab (decisione)

**Cinque tab: Lezioni · Chat · Cosa sai · Fonti · Mission.** Glossario e Registro **non** hanno tab proprie: stanno insieme nella tab **«Cosa sai»**, con un selettore a due voci («Registro» · «Glossario»).

| Opzione valutata | Esito |
|---|---|
| Sei tab (Lezioni · Chat · Registro · Glossario · Fonti · Mission) | Scartata. A 360 dp con font 1,0 le etichette sommano ≈ 420 dp: la riga scorre già con 5, con 6 la metà delle viste è fuori vista e con font 200 % ne resta visibile una. Troppe scelte allo stesso livello = carico estraneo (Sweller) |
| Glossario e Registro come sezioni di una stessa pagina scorrevole | Scartata. Il glossario cresce (fino a 8 termini per lezione): spingerebbe il registro, che è la parte che racconta i progressi, fuori vista |
| Registro e Glossario dentro la tab Lezioni | Scartata. Le lezioni sono l'*attività*; registro e glossario sono *consultazione* di ciò che si sa: due modi d'uso diversi (P1) |
| Fondere Fonti e Mission in una tab «Base» | Scartata: riscriverebbe `mission.md` e `resources.md` già accettati, per guadagnare una tab |
| Barra di navigazione in basso su compact | Scartata: introdurrebbe un secondo modello di navigazione dentro la stessa schermata, accanto al `TabRow` (base-components §7) |

**Perché un'unica tab «Cosa sai»**: registro e glossario sono due facce della stessa domanda («che cosa ho dimostrato finora?»): il registro dice *che cosa sai fare*, il glossario *con quali parole lo dici*. Il nome è in seconda persona e in linguaggio di competenza (P9: niente «Progressi», «Statistiche», «Punteggio»).

**Ordine**: dall'azione alla consultazione, dal più usato al meno: Lezioni (fare) · Chat (chiedere) · Cosa sai (verificare) · Fonti e Mission (la base, si consultano di rado).

**Presenza**: tutte e cinque sempre presenti, mai disabilitate né con badge. Una tab senza contenuto ha uno stato vuoto che spiega (Lezioni a Topic non `ready`: F3.A; «Cosa sai» prima della prima lezione completata: `progress.md`). Motivo: posizioni stabili (memoria motoria; niente layout che salta quando compare una tab).

**Su compact**: `TabRow` scrollabile (base-components §7); la tab selezionata è sempre riportata in vista all'apertura e al cambio, e la tab successiva resta **tagliata a destra** (indizio di scorrimento). A 360 dp e font 1,0 sono visibili circa 3,5 tab. Etichette corte (`Cosa sai`, 7 caratteri) per questo. Nessuna icona nelle tab. Tastiera fisica: Ctrl+Tab / Ctrl+Maiusc+Tab cambia tab; Alt+1…5 va alla tab N.

**Tab iniziale**:

| Stato del Topic | Tab iniziale |
|---|---|
| `new`, `interviewing`, `mission_proposed` | Chat |
| `researching` | Fonti |
| **`ready`** | **Lezioni** |
| `ready` con un job `lesson_generation` attivo | Lezioni (anche se l'ultima tab ricordata è un'altra, una sola volta per apertura dell'argomento) |

Poi vale l'ultima tab scelta **nella sessione** dell'app per quel Topic (non persistita oltre: ogni apertura a freddo riparte dallo stato, perché lo stato è il suggerimento del passo successivo). Aprendo l'argomento dalla Lezione (`ask_agent`, riepilogo): la tab è quella richiesta (`ask_agent.md`). Cambiare tab non perde scroll, bozze né stream.

**Chip di stato**: `ready` = «Pronto per le lezioni». Non cambia mentre si genera o si chiude una lezione: lo stato del Topic non cambia, e un chip che oscilla sarebbe rumore.

**Nessun badge sulle tab** per «lezione pronta», «registro aggiornato», «lezioni da fare» (P9): l'avviso è un Toast una tantum (F3.C) e le righe nella tab.

### F2. Expanded e pannello di contesto

Griglia comune delle tab: colonna `readingMaxWidth` 600 + gap `contextPaneGap` 32 + `ContextPane` `contextPaneWidth` 280 (gruppo di 912 dp centrato, come Fonti e Mission), **solo a ≥ 1100 dp**; sotto, colonna sola `min(600, w − 2·margine)` (margini 32 / 24 / 16). Il pannello non sostituisce nulla di ciò che la colonna dice: aggiunge il **contesto** di cui la tab si serve.

| Tab | Pannello a ≥ 1100 dp |
|---|---|
| Lezioni | **sì**, vedi F5 |
| Chat | sintesi della mission (`mission.md` §4.5); se la chat è stata aperta da una lezione, in cima il blocco «Lezione» (`../components/ask_agent.md`) |
| Cosa sai | no (colonna 600 allineata come le altre tab) |
| Fonti, Mission | invariato |

Sotto 1100 dp il contenuto del pannello **non** si sposta in uno sheet: nelle Lezioni è ripetizione di ciò che le altre tab già mostrano (coerenza, Mayer).

### F3. Tab Lezioni

Dati: `GET /topics/{id}/lessons` (lista, `number` decrescente), job corrente = primo di `GET /topics/{id}/jobs` con `kind = lesson_generation`, stato del Topic. Si ricalcola a ogni `job_updated`, `topic_updated` e al ritorno in primo piano.

Struttura della colonna, dall'alto: **Card «Prossima lezione»** (una sola, stati F3.A-F3.D) → [riga «Ripassa questo argomento», F3.B2, solo con schede in scadenza] → etichetta «Lezioni» (`ui.title3`) → lista (F4). Gap 32 dp fra card e lista (24 compact). Il **solo bottone primario della tab** è quello della card.

```
 expanded (≥ 1100)                                          compact (360)
┌──────────── 600 ────────────┐ 32 ┌──── 280 ────┐          ┌────────────────────────┐
│ ┌────────────────────────┐  │    │ Perché lo    │          │ ┌────────────────────┐ │
│ │ Prossima lezione       │  │    │ studi        │          │ │ Prossima lezione   │ │
│ │ Scelgo io…             │  │    │ «Mantenere…» │          │ │ …                  │ │
│ │ [ Genera lezione ]     │  │    │ Vedi la      │          │ │ [ Genera lezione ] │ │
│ └────────────────────────┘  │    │ mission      │          │ └────────────────────┘ │
│ Lezioni                     │    │              │          │ Lezioni                │
│ ┌────────────────────────┐  │    │ Hai già      │          │ ┌────────────────────┐ │
│ │ 3 Lifetimes in pratica │  │    │ dimostrato   │          │ │ 3 Lifetimes…       │ │
│ │   Saper dire quale…    │  │    │ ✓ Dopo un    │          │ │ …                  │ │
│ │   ◐ In corso · 2 di 4  │  │    │   move…      │          │ └────────────────────┘ │
│ ├────────────────────────┤  │    │ ✓ …          │          └────────────────────────┘
│ │ 2 Ownership e borrowing│  │    │ Vedi tutto   │
│ …                           │    └──────────────┘
```

#### F3.A Card quando il Topic non è `ready`

`Surface static` (non emphasis: non c'è un'azione primaria da evidenziare), padding 20 (16 compact). Titolo `ui.title3`, corpo `ui.body` `inkSoft`, un solo bottone **secondario** 48 dp. La lista non compare (non ci sono lezioni).

| Stato del Topic | Titolo | Corpo | Bottone |
|---|---|---|---|
| `new`, `interviewing` | Prima definiamo la mission | Scelgo cosa insegnarti partendo dal tuo scopo. Appena la mission è confermata e ho trovato le fonti, potrai generare la prima lezione. | «Definisci la mission» → tab Chat, focus sul composer |
| `mission_proposed` | Prima conferma la mission | La lezione parte dalla mission: quando la confermi cerco le fonti e poi potrai generare la prima lezione. | «Vai alla proposta» → tab Chat sulla card |
| `researching` | Sto cercando le fonti | Le lezioni si scrivono a partire dalle fonti: appena le ho, potrai generare la prima. | «Vedi l'avanzamento» → tab Fonti |
| `researching` con ricerca fallita (job `research` `failed`) | La ricerca delle fonti va riprovata | Senza fonti non scrivo lezioni: preferisco non inventare. | «Vai alle fonti» → tab Fonti |

Nessuna illustrazione, nessuna promessa di tempi. Se `GET …/lessons` restituisce lezioni anche a Topic non `ready` (non dovrebbe), la lista compare comunque sotto.

#### F3.B Card «Prossima lezione» (Topic `ready`, nessun job attivo)

`Surface emphasis` (fill `accentSoft`, nessun bordo), padding 20 (16 compact), raggio `md`. Titolo «Prossima lezione» `ui.title3` `ink`; corpo `ui.body` `ink` (13,9 / 8,6); nota `ui.bodyS` `ink` (su `accentSoft` non si usa `inkSoft`, il contrasto non è verificato). Bottone **primario** «Genera lezione» (48 dp, icona `sparkles` 20 a sinistra) allineato a sinistra con il testo.

| Situazione | Corpo | Nota | Azione |
|---|---|---|---|
| Nessuna lezione ancora | Scelgo io cosa insegnarti, partendo dalla tua mission e dalle fonti. | Di solito ci vogliono pochi minuti. Puoi lasciare l'app: la lezione si prepara sul server. | **«Genera lezione»** (primario) |
| Ci sono lezioni e **l'ultima è `completed`** | Scelgo io la prossima in base alla tua mission e a ciò che hai già dimostrato. | come sopra | **«Genera lezione»** (primario) |
| Ci sono lezioni e **l'ultima non è `completed`** (`not_started` o `in_progress`) | Hai una lezione da finire: «Lifetimes in pratica». Scelgo meglio la prossima dopo che l'hai completata, perché parto da ciò che dimostri. | — | **«Riprendi la lezione»** (primario, apre la lezione) + Button **testuale** «Genera un'altra lezione» (48 dp) |

*Perché «Riprendi» passa avanti a «Genera»* (non è un blocco: «Genera un'altra lezione» funziona sempre): il Planner sceglie in base ai record, e i record nascono alla **chiusura** di una lezione; generare la 4 prima di chiudere la 3 produce un piano cieco a ciò che hai appena imparato (zona di sviluppo prossimale, Vygotskij) e spreca una generazione a pagamento. Inoltre una pila di lezioni non iniziate è un debito che si accumula (P9), non un obiettivo. Ma la scelta resta all'utente (autonomia). Le lezioni `not_started` di tentativi precedenti restano in lista.

Con `409 llm_key_missing` / `502 llm_key_rejected` sul `POST` (nessun job creato) la card resta com'è e sopra il bottone compare `LlmKeyNotice` (`../llm-key.md` §5). Con `502 llm_credit_exhausted`: pannello `credit` con «Riprova» (`../llm-key.md` §9.2).

Tocco su «Genera lezione»: il bottone passa a `loading` («Avvio…», `busy`) fino alla risposta `202` del POST; poi la card diventa F3.C (anche per l'evento `job_updated` di un altro dispositivo). `409 conflict` (un job è già attivo, oppure il Topic non è più `ready`): ricarica in silenzio e mostra lo stato reale. Errore di rete sul POST: sotto il bottone `StatusPanel danger` compatto «Non sono riuscito a raggiungere il backend. Controlla la connessione.»; il bottone resta attivo.

#### F3.B2 Entrata «Ripassa questo argomento»

Dati: `GET /reviews/due?limit=1&topic_id={id}` → `due_count`, `estimated_minutes` (`reviews.md` v1.5). Si interroga all'apertura della tab Lezioni e al ritorno in primo piano / dal ripasso; la tab non aspetta la risposta (nessuno skeleton: la riga compare quando arriva).

**Comparsa**: solo se `due_count` ≥ 1. A **0** (o `next_due_at` nullo, errore, nessuna scheda): **niente**, nemmeno una riga discreta (qui il «va tutto bene» lo dà già Home; P2). Errore di rete: niente e in silenzio.

**Posizione e gerarchia**: **fra la card «Prossima lezione» e l'etichetta «Lezioni»**, nella stessa colonna, gap 16 dp dalla card, 32 (24 compact) verso «Lezioni». Aspetto: `Button` **testuale** 48 dp a tutta larghezza della colonna, allineato a sinistra come il testo, icona `layers` 20 `accent` + testo `ui.label` `accent`: **«Ripassa questo argomento · 8 schede»** (1: «· 1 scheda»; il numero è `min(due_count, 20)`, la sessione è di 20). Sotto, nello stesso target, `ui.bodyS` `inkSoft`: «Circa 2 minuti» (`estimated_minutes`, formula di `home.md`). Nessuna card, nessun fill, nessun bordo: **non ruba il primario** della tab, che resta quello di «Prossima lezione» (un solo primario per tab; con «Riprendi la lezione» il testuale «Genera un'altra lezione» è già nella card: questo è un terzo livello e sta fuori dalla card). Perché non in card: studiare una nuova lezione e ripassare sono due attività, la prima è la principale (P1).

**Comportamento**: apre `review.md` con `topic_id` (§7.3), il titolo diventa «Ripasso · {argomento}». Indietro dal ripasso riporta alla tab Lezioni invariata (scroll e tab selezionata), poi la riga si ricalcola. Mai in tab diverse da Lezioni (Chat, Fonti ecc. non cambiano).

**Accessibilità**: nodo `Role.Button`, `contentDescription` «Ripassa questo argomento, 8 schede, circa 2 minuti». Ordine TalkBack: card «Prossima lezione» → questa riga → «Lezioni» (heading). Tastiera fisica: **Alt+R** (solo se la riga è visibile; Alt+G resta di «Genera lezione»). A font 200% il testo va a capo e il target cresce.

#### F3.C Preparazione in corso (job `queued` / `running`)

La card diventa `Surface static` (non più emphasis: nessuna azione) con il **pannello di avanzamento**, lo stesso linguaggio di `resources.md` §4.2 (lista di fasi in parole, una sola corrente, nessuna percentuale, nessuna barra). **Il bottone «Genera lezione» sparisce** (non disabilitato: la card spiega il perché). La lista delle lezioni sotto resta intera e usabile.

```
┌────────────────────────────────────────────────┐ Surface static, padding 20 (16 compact)
│ Sto preparando la lezione                      │ ui.title3 ink
│ ✓  Scelgo cosa insegnare                       │ completata: check 20 (correct), ui.body inkSoft
│ ◌  Scrivo la lezione                           │ corrente: spinner 20 (accent), ui.bodyL 600 ink
│ ○  Controllo la lezione                        │ futura: cerchio 20 (inkMuted), ui.body inkMuted
│ ○  Salvo la lezione                            │
│ Puoi uscire: la lezione si prepara sul server. │ ui.bodyS inkSoft
└────────────────────────────────────────────────┘
```

| `progress.step` | Riga | Note |
|---|---|---|
| `planning` | «Scelgo cosa insegnare» | |
| `reading` | «Leggo le fonti» | Prima di `planning`/`writing` quando il job legge le fonti. Stessa riga delle altre (icona + `ui.bodyL`), nessun dettaglio né numero di pagine. Se il Topic non ha fonti lo step non arriva e la riga non compare |
| `writing` | «Scrivo la lezione» | |
| `checking` | «Controllo la lezione» | |
| `saving` | «Salvo la lezione» | |
| `writing` con `message` = «Correggo la lezione» (o `done` > 1) | la riga «Scrivo la lezione» **diventa** «Correggo la lezione», corrente; «Controllo la lezione» **torna futura** | **Riparazione.** Nessun colore di allarme, nessun numero di tentativi (`done`/`total` si ignorano: «tentativo 2 di 3» suonerebbe come un fallimento). Sotto la riga, dettaglio fisso `ui.bodyS` `inkSoft`: «Ho trovato qualcosa da sistemare e lo sto sistemando.». La lista ricalca ciò che succede davvero: dire «Controllo» mentre si riscrive sarebbe falso |
| sconosciuto | nessuna riga evidenziata; un solo spinner + `progress.message` (se assente: «Sto preparando la lezione…») | mai crash |
| `queued` | prima riga con spinner e dettaglio «In coda…» | |

- Dettaglio `progress.message` (≤ 80 car.) sotto la riga corrente, `ui.bodyS` `inkSoft`, coalescenza 1 s, **non** è live region (come Fonti).
- **Lenta**: dopo 4 minuti la frase finale diventa «Sta richiedendo più del solito. Puoi lasciare l'app: la lezione continua sul server.»
- **Annunci TalkBack**: live region Polite persistente solo sui **cambi di fase** («Preparo la lezione: scrivo.»); al completamento «Lezione pronta: <titolo>.» Il focus non si sposta.
- **Fine, esito riuscito**: le fasi si chiudono (tutte `check`) per 600 ms, poi crossfade `base` (200 ms, `motion.duration.base`, easing standard di `tokens.json`) del pannello verso F3.B; contemporaneamente la nuova riga in cima alla lista entra con fade-in `base` (nessuno slide, nessuna scala), senza spostare lo scroll (variante «da finire» che apre la nuova lezione) e la lista si ricarica; la nuova riga in cima ha la pill «Nuova» (`ui.labelS`, `info` su `infoSoft`: 5,53 / 5,55) finché l'utente non esce dalla tab. Nessuna animazione celebrativa (P9). Con «riduci animazioni»: sostituzione immediata.
- **Toast** (una volta per job, solo se l'utente è in un'altra tab o schermata dell'argomento e non sulla Lezione, 6 s): «La lezione 3 è pronta.» azione «Vedi» → tab Lezioni; se fallisce: «Non sono riuscito a preparare la lezione.» azione «Vedi». Rientrando a job finito mentre l'app era chiusa: nessun toast.
- Rete persa durante l'attesa: nessun messaggio (il job è sul server); al ritorno si ricarica la lista dei job.

#### F3.D Preparazione fallita (job `failed`)

`StatusPanel danger` (icona `alert-circle`) **al posto** della card, stesso schema di `resources.md` §4.4: titolo, corpo, `Button` secondario «Riprova» (icona `rotate-ccw`, 48 dp; riusa lo stesso job: `POST /topics/{id}/jobs/{job_id}/retry`; mentre parte: `loading` «Riprovo…»), `Disclosure` «Dettagli» con `error.message` selezionabile. La lista sotto resta intera. Non si offre «Genera lezione» in parallelo (un'azione sola).

| `error.code` | Titolo | Corpo |
|---|---|---|
| `invalid_output` | Non sono riuscito a scrivere una lezione valida | Ho provato più volte, ma il risultato non passava i miei controlli. Non hai perso nulla: riprova, spesso basta. |
| `upstream_failed` | Preparazione interrotta | Il servizio che scrive le lezioni non ha risposto. Non hai perso nulla: puoi riprovare. |
| `rate_limited` | Troppe richieste | Il servizio è occupato. Aspetta un minuto e riprova. |
| `timeout` | La lezione ha richiesto troppo tempo | Non sono riuscito a finire in tempo. Puoi riprovare: spesso basta. |
| `interrupted` | Preparazione interrotta | Il server si è riavviato più volte mentre scrivevo. Riprova. |
| `internal` / sconosciuto | Preparazione interrotta | Qualcosa è andato storto sul server. Riprova. |
| `llm_key_missing` / `llm_key_rejected` | non è questa tabella: pannello `LlmKeyNotice` con «Inserisci/Sostituisci la chiave», «Riprova» solo dopo il salvataggio di una chiave | testi in `../llm-key.md` §6. `llm_credit_exhausted`: stesso posto, pannello `credit` con «Riprova» subito (`../llm-key.md` §9.3) |

Annuncio: `liveRegion Polite` «Preparazione interrotta. Riprova disponibile.» Errore di rete su «Riprova»: stesso pannello con corpo «Non sono riuscito a raggiungere il backend. Controlla la connessione.» `409` su Riprova: ricarica in silenzio (c'è già un job più recente o attivo). Il chip del Topic **non** cambia (resta «Pronto per le lezioni»: l'errore ha il suo pannello).

#### F3.E Card, riepilogo di tutti gli stati

| Stato | Variante superficie | Azione |
|---|---|---|
| Topic non `ready` | static | secondario: porta alla tab giusta |
| `ready`, libero | emphasis | **primario** «Genera lezione» |
| `ready`, lezione da finire | emphasis | **primario** «Riprendi la lezione» + testuale «Genera un'altra lezione» |
| job attivo | static | nessuna (spinner) |
| job fallito | `danger` | secondario «Riprova» |

### F4. Lista delle lezioni

Dentro `Surface static` unico, righe `ListRow` ad altezza libera (min 96 dp), separatori `line` 1 dp inset 16. Non una card per lezione (rapporto dati/inchiostro, Tufte). Ordine: `number` decrescente (la più recente, che di solito è la prossima da fare, in cima); i numeri nel cerchio da 32 dp ricordano la sequenza.

```
┌──────────────────────────────────────────────┐
│ (3)  Lifetimes in pratica        [Nuova]   ›  │ numero: cerchio 32 dp surfaceSunken, ui.label ink
│      Saper dire quale riferimento vive più   │ titolo ui.title3 ink (max 2 righe a font 1×)
│      a lungo in una funzione.                │ objective ui.body inkSoft (max 2 righe a font 1×, poi libero)
│      ◐ In corso · 2 di 4 sezioni · 8 minuti  │ meta ui.bodyS inkSoft, icona 16 + parole
├──────────────────────────────────────────────┤
│ (2)  Ownership e borrowing                 ›  │
│      …                                       │
│      ✓ Completata · 6 minuti                 │
```

| `status` | Icona (16) + parole nella meta | Tocco |
|---|---|---|
| `not_started` | `circle` · «Da iniziare» | apre la lezione dall'inizio |
| `in_progress` | `circle-dot` (`accent`) · «In corso» | apre la lezione nella posizione salvata |
| `completed` | `check` (`inkSoft`) · «Completata» | apre il **Riepilogo** (`lesson.md` §7), da cui «Rileggi la lezione» |
| sconosciuto | come `not_started` | |

- **«2 di 4 sezioni» è stato locale dell'app**, **non** del backend (`lessons.md` §2: il backend conosce solo `status`). Si mostra **solo se** su questo dispositivo esiste lo stato di lettura della lezione (`lesson.md` §2) e la lezione è `in_progress`; dopo aver svuotato i dati o su un altro dispositivo la riga dice solo «In corso». Non si deduce mai dai `attempts` del backend: il conteggio sarebbe sbagliato (sezioni senza esercizi). `completed`: mai frazione.
- La meta termina con i minuti: «Circa 8 minuti» (`estimated_minutes`; sulla riga lunga a font grande va a capo).
- **Completata con registro non riuscito** (il job `lesson_completion` di quella lezione è `failed`; si conosce da `GET /topics/{id}/jobs`): la meta diventa `alert-circle` 16 `inkSoft` + «Completata · registro da riprovare» (testo semplice, niente rosso: l'azione sta nel Riepilogo). Con job attivo: «Completata» e basta.
- **Nessun voto, nessun «x/y corrette», nessun punteggio** nella lista.
- Pill «Nuova» (solo per la lezione appena generata, F3.C).
- Riga = **un solo target** (`Role.Button`), `contentDescription`: «Lezione 3: Lifetimes in pratica. Obiettivo: …. In corso, 2 sezioni su 4. Circa 8 minuti.» `onClickLabel` «Apri».
- **Paginazione**: la lista è paginata (`api-conventions.md` §8): si carica la pagina successiva quando restano 3 righe alla fine, con una riga skeleton in coda; se la pagina fallisce, in coda `Button` testuale «Riprova a caricare le altre» (48 dp). Nessun contatore di lezioni.
- **Vuota** (Topic `ready`, 0 lezioni): l'etichetta «Lezioni» e la lista non compaiono: c'è solo la card F3.B. Nessun testo «Nessuna lezione» (la card dice già cosa fare).
- **Caricamento**: skeleton = un riquadro card (altezza 144 dp) + 3 righe (barre `surfaceSunken`, niente shimmer), `stateDescription` «Caricamento».
- **Errore** del primo caricamento: `ErrorState` nella colonna («Non riesco a caricare le lezioni.» + «Riprova»); se invece falliscono solo i job, si mostra la lista e la card F3.B (un doppio avvio è già coperto dal `409`).
- Tastiera fisica: righe nel tab order; Invio/Spazio aprono; frecce ↑↓ spostano il focus fra righe.

### F5. Pannello di contesto della tab Lezioni (≥ 1100 dp)

*Perché esiste*: la card dice che la prossima lezione si sceglie «in base alla tua mission e a ciò che hai già dimostrato»: il pannello **mostra** quei due ingredienti (trasparenza sul perché di una scelta, contiguità con il pulsante). Niente metriche.

Senza riquadro (come `ContextPane`), due blocchi, titoli `ui.label`:
1. **Perché lo studi**: il «perché» della mission confermata, `ui.body` `ink`, max 4 righe con «Mostra tutto» al tocco, + Button testuale «Vedi la mission» (cambia tab).
2. **Hai già dimostrato**: i titoli (`ui.body` `ink`, icona `check` 16 `inkSoft`) degli ultimi **3** record `active` (`GET …/learning-records`), + Button testuale «Vedi tutto» (cambia tab su «Cosa sai» › Registro). Senza record: il blocco non compare (nessun «Ancora nulla»).

Nessun conteggio, nessun elenco delle lezioni, nessun «prossimo passo» (lo dice la card). Ordine di lettura: dopo la lista (`traversalIndex`).

### F6. Accessibilità e tastiera della tab

- Ordine: Indietro → titolo → «Altre azioni» → chip → tab → card → «Lezioni» (heading livello 2) → righe → pannello.
- La card è un gruppo con `paneTitle` «Prossima lezione»; gli stati sono live region Polite solo per le transizioni (preparazione → pronta/fallita).
- Contrasti (da `design-system.md` §2.4, nessun colore nuovo): `ink`/`accentSoft` 13,9 / 8,6; `inkSoft`/`surface` 9,35 / 10,08; `inkMuted`/`surface` 6,52 / 6,96; `info`/`infoSoft` 5,53 / 5,55; `accent`/`surface` (icona `circle-dot`) ≥ 4,5.
- Font 200 %: ogni testo va a capo; il bottone «Genera lezione» e il testuale vanno in colonna; le fasi restano una sotto l'altra.
- «Riduci animazioni»: sostituzioni istantanee (`durationScale` 0: pannello → card e nuova riga senza dissolvenza), spinner → anello fisso a 270°.
- **Tastiera fisica (tablet)**: Ctrl+Tab / Ctrl+Maiusc+Tab e Alt+1…5 per le tab (già in §Tab). Nella tab Lezioni: Tab percorre card → righe; ↑↓ spostano il focus fra le righe; Invio/Spazio aprono la lezione; **Alt+G** = «Genera lezione» (solo se la card lo mostra, mai sul bottone di Riprendi), **Alt+R** = «Ripassa questo argomento» (solo se la riga F3.B2 c'è). Nessuna scorciatoia a lettera singola né Ctrl+Invio: nel composer della chat Invio e Ctrl+Invio **inviano** (`chat.md` §8) e Alt+1…5/Ctrl+Tab non sono intercettati dal campo di testo, quindi non confliggono; le scorciatoie di tab funzionano anche con il focus nel composer solo se Ctrl/Alt sono premuti.
- Non solo colore: stato di lezione = icona + parola; fase = icona + testo.

### F7. Elimina/rinomina l'argomento (aggiornamento)

Il `ConfirmDialog` di eliminazione, se esistono lezioni, aggiunge: «Verranno eliminate anche le lezioni e ciò che hai dimostrato in questo argomento.» (il registro e il glossario sono del Topic). Se una lezione è in preparazione: «La lezione in preparazione andrà persa.»

### F8. Costanti

`LessonRowMinHeight = 96.dp` · `LessonNumberCircle = 32.dp` · `LessonGenerationSlowAfter = 4.minutes` · `LessonsPaginationPrefetch = 3` · `RegisterPaneRecords = 3` · `ContextPaneBreakpoint = 1100.dp`. Tutti in codice (nessuna novità in `tokens.json`).

### F9. Evitare

Contatori («3 lezioni da fare»), percentuale del Topic, barra di avanzamento sul Topic, badge sulle tab, streak, «lezione del giorno», generazione automatica, conto alla rovescia, stima di minuti rimasti nella preparazione.


## Lezioni che seguono il tuo PDF

Obiettivo didattico: far **vedere** che le lezioni stanno seguendo il documento caricato (segnalazione, Mayer; autonomia, Deci e Ryan), senza nuove funzioni. Specifica del caricamento: `resources.md` §13.

- **Card «Prossima lezione» (F3.B)**, quando il Topic ha almeno un documento `ready`: alla nota `ui.bodyS` `ink` si aggiunge, a capo, «Seguo anche il tuo PDF «Rust in Action»: scelgo io quali pagine, partendo dalla tua mission.» (più documenti: «Seguo anche i tuoi PDF.»). Nessun selettore di pagine (le sceglie il Planner); se l'utente ha idee, la chat è il posto (testo nel pannello Mission, sotto).
- **Lista lezioni (F4)**: se la lezione ha fonti dal PDF, la riga meta (dopo lo stato e i minuti) aggiunge icona `file-text` 16 + «Dal tuo PDF: pp. 101-118» su una riga propria (`ui.bodyS` `inkSoft`, a capo libero, mai ellissi). Dati: `Lesson.document_sources` (`[{document_id, title, page_start, page_end}]`, contratto v1.8); array vuoto o assente = nessuna riga. Un documento: «Dal tuo PDF: pp. 101-118» (`page_start == page_end`: «p. 101»). Più documenti: **una riga per documento**, con il titolo: ««Rust in Action»: pp. 101-118» (stessa icona e stile, a capo libero). Più di 3 documenti: le prime 3 e «e altri {k}». Anche il riferimento nella lezione resta (`lesson.md` §3.3). `contentDescription` della riga: si aggiunge «Dal tuo PDF, pagine 101-118.»
- **Pannello Mission / tab Mission**: quando esiste almeno un documento `ready`, il `ContextPane` e la tab Mission mostrano sotto la mission una riga `ui.bodyS` `inkSoft` con icona `file-text`: «Studi dal tuo PDF «Rust in Action».» + Button testuale «Vai ai documenti» (cambia tab su Fonti). Non fa parte del testo della mission (è libero) e non si modifica da lì.
- **Chat con il Tutor**: se un documento viene importato durante l'intervista, il Tutor ne conosce titolo e indice e chiede quali parti studiare (prompt: llm-engineer). L'app non mostra nulla di speciale: la domanda del Tutor basta. Se l'utente carica il PDF **dopo** la conferma della mission, nessun messaggio automatico in chat (rumore): vale il banner di `resources.md` §13.8.
- Nessun contatore di pagine «studiate», nessuna barra di avanzamento nel documento (la lezione completata non equivale a pagina capita; il progresso reale è nel registro di ciò che hai dimostrato).
