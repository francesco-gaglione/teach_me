# Tab Fonti e avanzamento della ricerca (Fase 4)

> **Fase 10**: la tab ha in cima la sezione «I tuoi documenti» (PDF caricati). Specifica in §13; le regole degli altri paragrafi valgono per le sole fonti web.

> Contratto dati: **da scrivere** (tech lead); i campi che servono sono in §10 «Proposte di contratto». Componenti base: `../base-components.md` (`Surface`, `ListRow`, `StatusPanel`, `Button`, `ProgressBar`, `Disclosure`, `TabRow`). Tablet orizzontale per primo (ADR 0003), poi compact. Terminologia: «fonti», «mission», «Tutor», «argomento» (mai «Resource», «Topic», «job»).
> Formato di riferimento: `RESOURCES-FORMAT.md` della skill `teach` (Matt Pocock): fonti ad alta fiducia, gruppi *Knowledge* / *Wisdom (Communities)*, ogni voce con titolo + URL + una riga di annotazione + «Use for», sezione *Gaps*, opt-out dalle community.

## 1. Scopo didattico

Le fonti non sono un elenco di link da collezionare: sono la **base di verità** da cui il Generator prende i contenuti delle lezioni (mvp §7.2). All'utente servono per tre cose, e il design le serve in quest'ordine:

1. **Fiducia calibrata**: capire *da dove* arriva ciò che imparerà (dominio sempre visibile, tipo di fonte in parole). Un elenco opaco chiederebbe fede; uno annotato permette di giudicare.
2. **Autonomia (Deci e Ryan)**: ogni fonte dice **a cosa serve per la *sua* mission** («Usala per…»). L'utente può andare oltre le lezioni quando vuole, al suo ritmo; niente «da leggere», niente spunte, niente «fonti lette 3/12» (sarebbe progresso finto: aprire un link non è imparare).
3. **Onestà epistemica**: ciò che non si è trovato (Gaps) si dice, con calma. Sapere dove la base è fragile è informazione utile per l'apprendimento, non un fallimento.

Principi applicati:
- **Carico cognitivo (Sweller)**: una lista a una colonna, ogni voce con la stessa struttura (si impara a scandirla); nessun filtro, ordinamento, ricerca o tag nella v1 (≤ ~15 voci).
- **Segnalazione (Mayer)**: l'unica cosa evidenziata in ogni voce è «Usala per» (è la parte azionabile); il resto è quieto.
- **Pre-training (Mayer)**: lo stato vuoto e le intestazioni dei gruppi spiegano che cosa sono Conoscenza e Community prima che l'utente le veda.
- **Contiguità**: annotazione e «Usala per» stanno nella stessa voce del link, mai in una nota a parte.
- **Attesa onesta**: l'avanzamento mostra fasi reali in parole, mai una percentuale inventata (la ricerca web non ha un denominatore noto), mai una barra che finge di sapere.
- **Errore = occasione**: il fallimento non è colpa dell'utente e non perde nulla di ciò che c'era.

## 2. Posizione, presenza e tab iniziale

- Da Fase 4 la `TabRow` diventa **Chat · Fonti · Mission** (ordine finale con tutte: Lezioni · Chat · Glossario · Fonti · Mission, `topic.md`; le tab senza contenuto reale restano assenti). La tab Fonti è **sempre presente** (anche prima della conferma: ha uno stato vuoto che spiega, §4.1), non è mai disabilitata. Nessun badge o contatore sulla tab.
- **Tab iniziale**, aggiorna `topic.md`: `new`/`interviewing`/`mission_proposed` → Chat; **`researching` → Fonti** (qui c'è l'avanzamento; prima era Mission); `ready` → Lezioni quando esisterà (Fase 6), fino ad allora Fonti. Poi l'ultima scelta per sessione. Cambiare tab non perde scroll né stato.
- **Nessuna transizione automatica** alla conferma della mission (invariato, `mission.md` §3): l'utente decide; la card confermata e il pannello contestuale offrono «Vedi l'avanzamento» (§7).
- Il flag `ResearchAvailable` di `mission.md` §5 si **accende** in Fase 4: la «frase onesta» e il chip «Mission confermata» spariscono, vedi §7.

## 3. Layout

### 3.1 Expanded (≥ 1100 dp, riferimento 1280 × 800)

Stessa griglia della tab Mission (`mission.md` §4.2): colonna di lettura 600 dp + gap 32 + colonna laterale `contextPaneWidth` 280 dp, gruppo centrato (912 dp). La pagina scorre per intero; la colonna laterale è in alto, non fissa (il suo contenuto è poco).

```
|<----------------------------- 1280 dp ----------------------------->|
┌──────────────────────────────────────────────────────────────────────┐
│ ←  Rust                                                          ⋮   │
│    ( 🔍 Cerco le fonti… )                                            │ chip
│  Chat   Fonti   Mission                                              │ TabRow, Fonti selezionata
│         ━━━━━                                                        │
├──────────────────────────────────────────────────────────────────────┤
│      ┌──────────── 600 ───────────┐ 32 ┌────── 280 ──────┐           │
│      │ Conoscenza · 6             │    │ La ricerca        │           │
│      │ Da qui prendo ciò che ti   │    │ 9 novembre        │           │
│      │ insegno.                   │    │ Per la mission,   │           │
│      │ ┌────────────────────────┐ │    │ versione 2.       │           │
│      │ │ 📖 Libro · doc.rust…   ↗│ │    │                   │           │
│      │ │ The Rust Book           │ │    │ 8 fonti:          │           │
│      │ │ Spiega ownership con…   │ │    │ 6 di conoscenza,  │           │
│      │ │ Usala per: capire…      │ │    │ 2 di community.   │           │
│      │ ├────────────────────────┤ │    │                   │           │
│      │ │ …                       │ │    │ [Aggiorna le fonti]│          │
│      │ └────────────────────────┘ │    └───────────────────┘           │
│      │ Community · 2              │                                    │
│      │ …                          │                                    │
│      │ Cosa non ho trovato        │                                    │
│      └────────────────────────────┘                                    │
└──────────────────────────────────────────────────────────────────────┘
```

- Colonna laterale «La ricerca» (senza riquadro, come `ContextPane`): titolo `ui.title3`; righe `ui.bodyS` `inkSoft` («Cercate il 9 novembre.» / «Per la mission, versione 2.» / «8 fonti: 6 di conoscenza, 2 di community.»); sotto, `Button` secondario «Aggiorna le fonti» (48 dp), mai primario salvo banner di §6. Se nessuna ricerca esiste o è in corso: la colonna **non compare** (il contenuto è già nella colonna principale; evita ripetizioni, Mayer: coerenza).
- Larghezza < 1100 dp: una colonna `min(600, w − 2·margine)`, margini 24 (medium) / 16 (compact), la colonna laterale diventa una **riga riepilogo** in cima (§3.2). La colonna di 600 dp tiene la misura di riga delle annotazioni a ≈ 60-70 caratteri.
- Altezza utile < 480 dp (tastiera, orizzontale telefono): TopBar, chip e TabRow scorrono via con il contenuto.

### 3.2 Compact (telefono, 360 × 800)

```
┌──────────────────────────────────────┐
│ ←  Rust                        ⋮     │
│    ( 🔍 Cerco le fonti… )            │
│  Chat   Fonti   Mission              │
├──────────────────────────────────────┤
│ 8 fonti · cercate il 9 novembre      │ ui.bodyS inkSoft
│ per la mission v2.  [Aggiorna le fonti]│ Button testuale 48 dp, a capo
│                                      │
│ Conoscenza · 6                       │ ui.title3
│ Da qui prendo ciò che ti insegno.    │ ui.bodyS inkSoft
│ ┌──────────────────────────────────┐ │
│ │ 📖 Libro · doc.rust-lang.org  ↗ │ │
│ │ The Rust Book                    │ │
│ │ …                                │ │
└──────────────────────────────────────┘
```

Scorrimento verticale unico; nessun elemento fisso.

## 4. Stati della tab

Lo stato si deriva da: `topic.state`, dal job di ricerca più recente (`queued|running|succeeded|failed`) e dall'esistenza di fonti salvate. Si ricalcola a ogni `job_updated`, `topic_updated` e al ritorno in primo piano (fallback: `GET /jobs/{id}` se l'SSE cade).

| Stato | Condizione | Contenuto |
|---|---|---|
| **A. Vuoto** | mission non confermata (`new`, `interviewing`, `mission_proposed`) | §4.1 |
| **B. In corso (prima ricerca)** | job `queued`/`running`, nessuna fonte | §4.2 |
| **C. Completato** | job `succeeded`, ≥ 1 fonte o ≥ 1 gap | §4.3 |
| **C0. Completato senza fonti** | `succeeded`, 0 fonti | §4.3 |
| **D. Fallito** | job `failed`, nessuna fonte | §4.4 |
| **E. Rifacimento** | nuovo job mentre esistono fonti precedenti | §6 |
| Caricamento | prima del primo `GET` | skeleton di 3 righe voce (barre `surfaceSunken`, niente shimmer), `stateDescription = "Caricamento"`; errore: `ErrorState` con «Riprova» |

### 4.1 Stato A: vuoto (mission non confermata)

```
Le fonti arrivano dopo la mission                 ui.title2
Le fonti sono i libri, la documentazione e gli    ui.bodyL, ink, max 600 dp
articoli affidabili da cui prendo ciò che ti
insegno. Le cerco quando hai confermato la
mission, così le scelgo per il tuo scopo.

[ Definisci la mission ]                          Button secondario 48 dp
```

- Allineato in alto, 48 dp dalla cima (come lo stato vuoto della chat), niente illustrazione.
- Il bottone porta alla tab Chat con il focus sul composer; se `state = mission_proposed` si chiama «Vai alla proposta» e scorre la chat alla card (come `mission.md` §4.5).
- Non promette tempi né nomina funzioni inesistenti.

### 4.2 Stato B: ricerca in corso

L'unica cosa nella tab è il **pannello di avanzamento** (`ResearchProgress`). Nessun altro elemento compete.

```
Sto cercando le fonti                            ui.title2
Per la tua mission: «Mantenere il backend di     ui.body inkSoft, max 3 righe (Perché della mission, ellissi)
pagamenti scritto in Rust nel mio nuovo lavoro.»

┌────────────────────────────────────────────────┐ Surface static, padding 20
│ ✓  Preparo le domande di ricerca               │ fase completata: icona check 20 (correct), testo ui.body inkSoft
│ ◌  Cerco sul web                               │ fase corrente: spinner 20 (accent), testo ui.bodyL 600 ink
│    Ho consultato 14 risultati                  │ dettaglio (opz.) ui.bodyS inkSoft, rientro 28
│ ○  Leggo le pagine migliori                    │ fase futura: cerchio vuoto 20 (inkMuted), testo ui.body inkMuted
│ ○  Valuto l'affidabilità e scrivo le note      │
│ ○  Salvo le fonti                              │
└────────────────────────────────────────────────┘
Di solito ci vogliono pochi minuti. Puoi uscire:   ui.bodyS inkSoft
la ricerca continua sul server.
```

- **Nessuna percentuale e nessuna barra a riempimento.** `progress.percent` (se il backend la manda) **si ignora**. Una barra indeterminata non si usa: la lista di fasi, con una sola corrente, già dice che si sta lavorando; il movimento è lo spinner (un solo elemento animato).
- **Le cinque fasi** (parole fisse nell'app, mappate da `progress.step`, vedi §10): `planning` «Preparo le domande di ricerca» · `searching` «Cerco sul web» · `reading` «Leggo le pagine migliori» · `evaluating` «Valuto l'affidabilità e scrivo le note» · `saving` «Salvo le fonti». Le fasi sono sempre visibili, così l'utente sa **quanto manca in forma qualitativa** (terza di cinque) senza numeri falsi. Se `progress.step` è sconosciuto (backend più nuovo dell'app): nessuna fase evidenziata, un solo spinner con il testo `progress.message`; mai crash.
- **Dettaglio** sotto la fase corrente: `progress.message` del backend, se presente (in italiano, ≤ 80 caratteri, es. «Ho consultato 14 risultati», «Leggo doc.rust-lang.org»). Aggiornamenti al massimo ogni 1 s (coalescenza); non è una live region. Conta solo fatti già avvenuti, mai stime.
- `queued` (non ancora partito): prima fase con spinner e dettaglio «In coda…»; dopo 15 s resta uguale (non si inventa un motivo).
- **Lenta**: dopo 4 minuti la frase finale diventa «Sta richiedendo più del solito. Puoi lasciare l'app: la ricerca continua sul server.» (nessun bottone: l'utente non può accelerare). Il tempo massimo lo decide il backend (job `failed` con `timeout`, §4.4).
- **Passaggio a completato**: le fasi si chiudono (tutte `check`) per 600 ms, poi crossfade `fast` verso lo stato C. Con «riduci animazioni»: sostituzione immediata.
- Il pannello **non è un blocco**: chat e mission restano usabili (la mission confermata non si modifica, ma si può scrivere al Tutor).
- Margini e misure: pannello `Surface static` (raggio `md`, bordo `line` 1 dp), righe min 40 dp, gap 12 dp, padding 20 (16 compact). Titolo e frase finale fuori dal pannello.

### 4.3 Stato C: completato

Ordine dall'alto (expanded: colonna principale; il riepilogo sta nella colonna laterale):

1. (compact/medium) riga riepilogo + «Aggiorna le fonti» (§3.2).
2. Banner di cambio mission, se serve (§6.1).
3. Gruppo **Conoscenza** (`ui.title3` «Conoscenza · 6», sotto `ui.bodyS` `inkSoft`: «Da qui prendo ciò che ti insegno.»).
4. Gruppo **Community** (`ui.title3` «Community · 2», sotto: «Dove trovi l'esperienza pratica di chi lo fa.»). Omesso se vuoto. Se la ricerca non ha cercato community (dal 2026-11-15, issue #15, sono facoltative: si cercano solo se l'utente le chiede in chat): `ui.bodyS` `inkMuted`: «Non ho cercato community: se ne vuoi, chiedimelo in chat e poi aggiorna le fonti.» al posto del gruppo (§10 `communities_skipped`).
5. **Cosa non ho trovato** (§5), se esistono gap.

Margini: 32 dp sopra i titoli di gruppo (24 compact), 8 dp fra titolo e riga di sottotitolo, 12 dp fra sottotitolo e card.

**C0, senza fonti** (ricerca finita ma nessuna fonte superava la soglia di affidabilità): `StatusPanel info`, titolo «Non ho trovato fonti affidabili», corpo «Ho cercato, ma niente era abbastanza solido per la tua mission, e preferisco non includere fonti dubbie. Qui sotto trovi cosa è mancato.» + `Button` secondario «Cerca di nuovo»; seguono i Gaps (se presenti). Mai `danger`: la ricerca ha funzionato, il risultato è scarso.

### 4.4 Stato D: fallito

```
┌────────────────────────────────────────────────┐ StatusPanel danger, icona alert-circle
│ (!) Ricerca interrotta                         │
│ Non sono riuscito a completare la ricerca.     │ corpo ui.bodyL ink su dangerSoft
│ La mission è al sicuro: puoi riprovare.        │
│ [ ↻ Riprova ]                                  │ Button secondario 48 dp
│ ▸ Dettagli                                     │ Disclosure testuale, ui.bodyS inkSoft
└────────────────────────────────────────────────┘
```

- **Fase 11**: con `error.code` = `llm_key_missing` / `llm_key_rejected` il pannello è `LlmKeyNotice` (`../llm-key.md` §6) invece di quello sotto; lo stesso per `409`/`502` alla nuova ricerca o all'aggiornamento (`../llm-key.md` §5). Con `llm_credit_exhausted` (job o `502`): pannello `credit` con «Riprova» (`../llm-key.md` §9).
- Testi per `error.code` (stesso lessico di chat §9.2, mai il `message` grezzo in primo piano):

| Codice | Titolo | Corpo |
|---|---|---|
| `upstream_failed` | Ricerca interrotta | Il servizio di ricerca non ha risposto. La mission è al sicuro: puoi riprovare. |
| `rate_limited` | Troppe richieste | Il servizio di ricerca è occupato. Aspetta un minuto e riprova. |
| `timeout` | La ricerca ha richiesto troppo tempo | Non sono riuscito a finire in tempo. Puoi riprovare: spesso basta. |
| `internal` / sconosciuto | Ricerca interrotta | Qualcosa è andato storto sul server. Riprova. |

- «Dettagli»: `Disclosure` chiusa, mostra `error.message` in testo semplice selezionabile (`ui.bodyS`, `inkSoft`).
- **Riprova**: `loading` («Riprovo…», `busy`), avvia una nuova ricerca (§10 `POST …/research`) → stato B. Errore di rete nella richiesta: stesso pannello con corpo «Non sono riuscito a raggiungere il backend. Controlla la connessione.» (la tab resta in D). `409` (un job è già in corso, altro client): ricarica in silenzio e passa a B.
- Annuncio: il pannello è `liveRegion Polite` «Ricerca interrotta. Riprova disponibile.»; focus TalkBack mai spostato.
- Sotto il pannello niente altro (non ci sono fonti). Il chip resta «Cerco le fonti…»? **No**: con job `failed` il chip diventa «Ricerca da riprovare» (`alert-circle`, variante neutral: l'errore ha già il suo pannello, il chip non deve gridare). Aperto per il tech lead: stato del Topic dopo un fallimento (§11).

## 5. Gaps: «Cosa non ho trovato»

Obiettivo: dire la verità sulla fragilità della base **senza far sentire l'utente in colpa o in ritardo**. Scelte:

- **Nome**: «Cosa non ho trovato» (non «Lacune», «Mancanze», «Problemi»): è una frase di cronaca in prima persona, riguarda la ricerca, non l'utente né la mission.
- **Variante visiva**: blocco `surfaceSunken` (nessun colore di stato, niente ambra/rosso, niente `warning`), raggio `md`, padding 16 (compact) / 20; icona `compass` 20 dp `inkSoft` accanto al titolo `ui.label` `ink`. Posizione: **in fondo**, dopo le fonti (le fonti sono il contenuto principale; i gap non devono essere la prima cosa che si legge). Mai un badge, mai un contatore («3 lacune»).
- **Contenuto**: frase introduttiva `ui.body` `inkSoft`: «Per questi punti della tua mission non ho trovato fonti abbastanza affidabili, quindi non ne ho incluse. Le lezioni su questi temi vanno prese con più cautela.»; elenco con «•» (colonna 20 dp), voci `ui.body` `ink`, ciascuna **una riga concreta** («Esempi di test d'integrazione con `sqlx`», non «materiale avanzato»); gap = argomento, mai giudizio.
- **Ripiegato?** No: sempre aperto se ≤ 4 voci; oltre 4, le prime 4 visibili e «Mostra le altre N» (Button testuale 48 dp). Nascondere i gap sarebbe disonesto; ma una lista lunghissima è rumore.
- **Nessuna azione obbligata.** Nessun bottone «Risolvi». (Se in futuro il Tutor potrà aggiungere fonti su richiesta, si aggiunge qui una riga «Se conosci una fonte valida, dillo al Tutor» **solo quando esiste la funzione**.)
- Gap vuoto: la sezione non compare (niente «Nessuna lacuna»: non è un risultato da celebrare).
- **Accessibilità**: titolo = heading livello 3; il blocco è un gruppo con `paneTitle` «Cosa non ho trovato»; ogni voce un nodo «Non trovato: …». Contrasti: `ink`/`surfaceSunken` 14,95 / 15,00; `inkSoft`/`surfaceSunken` 8,15 / 11,30.

## 6. Rifacimento dopo un cambio di mission

**Decisione: proposto all'utente, non automatico.** L'avvio automatico a ogni nuova versione sarebbe sbagliato per tre motivi:

1. **Apprendimento**: molte revisioni della mission riguardano vincoli, tempo o ordine («30 minuti al giorno», un fuori-ambito) e **non cambiano quali fonti servono**. Cambiare fonti sotto i piedi dell'utente senza che l'abbia chiesto (con lezioni già basate su quelle di prima) crea incoerenza e toglie controllo (autonomia, P9). Quando lo *scopo* cambia davvero («da backend a embedded»), l'utente lo sa e può decidere.
2. **Costo**: ogni ricerca usa gli strumenti web dell'LLM (ADR 0005: costo reale in denaro e tempo); le revisioni sono iterative (si confermano 2-3 versioni): ricerche automatiche moltiplicherebbero il costo senza valore.
3. **Carico cognitivo**: una tab che cambia da sola contenuto è un elemento che compete per l'attenzione. Un invito esplicito non lo fa.

**La prima ricerca invece è automatica** (parte con la conferma: è la risposta attesa al gesto dell'utente e il Topic non è utilizzabile senza fonti).

### 6.1 Banner «La mission è cambiata»

Appare in cima alla tab Fonti (stato C) quando `resources.mission_version < mission.current.version` e non c'è un job in corso.

```
┌────────────────────────────────────────────────┐ StatusPanel info (icona info)
│ (i) La mission è cambiata dopo la ricerca      │ titolo ui.labelL info
│ Le fonti sono state scelte per la versione 1.  │ corpo ui.bodyL ink su infoSoft
│ Se lo scopo è cambiato in modo importante,     │
│ aggiornale: ci vogliono pochi minuti. Se è     │
│ cambiato solo un dettaglio, puoi lasciarle.    │
│ [ Aggiorna le fonti ]                          │ Button PRIMARIO (unico della tab), 48 dp
└────────────────────────────────────────────────┘
```

- Il bottone dice cosa fa e non è una conferma ansiosa: nessun dialog («Sei sicuro?»): il costo è basso e la ricerca precedente resta visibile finché la nuova non riesce.
- Il banner **non si può chiudere** (si chiude da solo con l'aggiornamento) ma **non è mai bloccante né spostato in cima a Chat/Lezioni**. Nessun badge sulla tab, nessun chip diverso: il chip resta «Pronto per le lezioni» (il Topic non torna `researching`, si può continuare a studiare).
- Con il banner, la colonna laterale (expanded) mostra «Aggiorna le fonti» come secondario: un solo primario nella schermata (`base-components` §1). Quindi nel banner il bottone è primario e nella colonna **si omette** finché il banner è visibile.
- Quando `resources.mission_version == mission.current.version` il bottone «Aggiorna le fonti» resta disponibile come secondario nella colonna/riga riepilogo (per chi vuole riprovare senza aver cambiato la mission): è una scelta esplicita, con limite di frequenza lato backend (§10).
- **Dove altro si vede**: la card «Mission confermata» nella chat e la tab Mission, quando la confermata è una versione ≥ 2 e le fonti sono di una versione precedente, aggiungono una riga `ui.bodyS` `inkSoft`: «Le fonti sono della versione 1.» + Button testuale «Vai alle fonti» (cambia tab).

### 6.2 Stato E: ricerca di nuovo in corso

- Si mostra **in cima** il pannello di avanzamento di §4.2 (stesse fasi, titolo «Aggiorno le fonti») e **sotto** l'elenco precedente invariato, preceduto da `ui.bodyS` `inkSoft`: «Intanto, le fonti della ricerca precedente (mission versione 1).» Le voci restano apribili (nessun blocco, nessuna riduzione di opacità: il contenuto è valido).
- Il banner di §6.1 e il bottone «Aggiorna le fonti» **non compaiono** durante il job (disabilitato non serve: sparisce, e la frase del pannello spiega).
- **Esito riuscito**: la lista si sostituisce (crossfade `fast`), Toast «Fonti aggiornate: 8 fonti.» (4 s); le voci nuove rispetto alla ricerca precedente (confronto per URL) hanno la pill «Nuova» (`ui.labelS`, `info` su `infoSoft`, come `mission.md` §2.1) finché l'utente non esce dalla tab; poi spariscono (non è un badge persistente). Annuncio Polite «Fonti aggiornate.»
- **Esito fallito**: la lista precedente **resta** e sopra compare il pannello `danger` di §4.4 con titolo «Aggiornamento non riuscito» e corpo «Le fonti che vedi sono quelle della ricerca precedente. Puoi riprovare quando vuoi.» + Riprova. Nessuna perdita di dati.
- **Cambio di mission durante la ricerca** (una nuova versione viene confermata mentre il job gira): il job non si interrompe; a fine ricerca il banner §6.1 compare di nuovo. Nessuna annotazione speciale.

## 7. Come chat e pannello Mission segnalano la ricerca

Principio: **un solo posto mostra i dettagli** (la tab Fonti); gli altri danno un **puntatore**, mai una copia dell'avanzamento (Mayer: coerenza). Nessun messaggio automatico in chat (il contratto non lo prevede e una chat in cui il sistema parla da solo è rumore).

| Dove | In corso | Completato |
|---|---|---|
| **Chip di stato** (sotto il titolo, tutte le schermate) | `researching`: «Cerco le fonti…», icona `search`, variante neutral, **statica** (nessuna animazione: l'avanzamento è nella tab) | `ready`: «Pronto per le lezioni», `check-circle`, variante accent. Annuncio TalkBack al cambio: «Fonti pronte. Pronto per le lezioni.» (Polite) |
| **Card «Mission confermata»** (chat, `mission.md` §2.2) | al posto della frase onesta di Fase 3: `ui.body` `inkSoft` «Sto cercando le fonti per la tua mission.» + Button testuale «Vedi l'avanzamento» (cambia tab) | riga «Ho trovato 8 fonti.» + Button testuale «Vedi le fonti» |
| **ContextPane** (tab Chat ≥ 1100 dp, `mission.md` §4.5) | sotto la sintesi: `ui.bodyS` `inkSoft` «Cerco le fonti…» con spinner 16 dp + Button testuale «Vedi l'avanzamento» | «8 fonti pronte.» + «Vedi le fonti». Nessun bottone primario |
| **Tab Mission, «Cosa succede ora»** (`mission.md` §4.3.4) | il `StatusPanel info` «La mission è salvata» di Fase 3 **si sostituisce** con una riga: titolo «Cerco le fonti…», corpo «Le scelgo in base alla tua mission. Di solito ci vogliono pochi minuti.» + «Vedi l'avanzamento» | la sezione sparisce (nulla da dire: le fonti hanno la loro tab); resta solo la riga «Le fonti sono della versione N» se serve (§6.1) |
| **Toast** (se l'utente è in un'altra tab/schermata dell'argomento al completamento; 4 s) | — | «Ho trovato 8 fonti.» con azione «Vedi» → tab Fonti. Se il job fallisce: «La ricerca delle fonti non è riuscita.» con azione «Vedi» |
| **Lista cartella** (`folder.md`) | riga con chip «Cerco le fonti…» | «Pronto per le lezioni» |

- Il Toast è mostrato **una sola volta** per job e **solo se** l'utente non sta già guardando la tab Fonti.
- Al **rientro nell'argomento** a ricerca finita mentre l'app era chiusa: nessun toast; la tab iniziale è già quella giusta e il chip è aggiornato.
- Le frasi «Per ora non posso ancora cercare le fonti…» di `mission.md` §5 vanno rimosse quando `ResearchAvailable` è vero.

## 8. Card di una fonte (`ResourceRow`)

Una voce = **un solo target** (la riga intera), dentro un `Surface static` per gruppo con separatori `line` 1 dp inset 16 dp (come `ListRow`, `base-components` §4). Non una card per voce: meno bordi, più rapporto dati/inchiostro (Tufte).

### 8.1 Anatomia e gerarchia

```
┌──────────────────────────────────────────────────────┐ padding 16 (verticale 16), min 96 dp
│ 📖 Libro · doc.rust-lang.org · 2023              ↗  │ riga meta: ui.bodyS inkSoft; icona tipo 20 inkSoft; trailing external-link 20 inkSoft
│ The Rust Programming Language                        │ titolo: ui.title3 ink, a capo (mai ellissi)
│ Il riferimento ufficiale: spiega ownership, borrowing│ annotazione: ui.body ink
│ e lifetime con esempi.                               │
│ Usala per:  capire perché il compilatore rifiuta     │ «Usala per:» ui.label 600 ink + testo ui.body ink
│ il tuo codice sulle reference.                       │ rientro appeso a 0 (a capo allineato al margine)
└──────────────────────────────────────────────────────┘
```

Ordine (dal più importante per decidere se aprire):
1. **Titolo** (cos'è): `ui.title3` `ink`, max 3 righe a font 1×, a capo libero oltre; è il primo nodo a cui guarda l'occhio.
2. **Riga meta** *sopra* il titolo, più piccola: icona del tipo + tipo in parole + **dominio** + anno (se noto), separati da « · ». È sopra perché è il «contesto di fiducia» che si legge in un colpo; `ui.bodyS` `inkSoft` (≥ 4,5 su `surface`: 9,35 / 10,08). Il dominio non si tronca (a capo a «.» se serve).
3. **Annotazione** (cosa copre): `ui.body` `ink`, 1-2 frasi (≤ 200 caratteri): è il testo di Matt tradotto in italiano.
4. **Usala per** (quando ricorrere a questa fonte): etichetta `ui.label` (600) «Usala per:» seguita dal testo, stessa riga se entra. È **l'elemento segnalato** (Mayer): l'etichetta in grassetto ancora l'occhio; nessun colore, nessun riquadro.
5. **Trailing** `external-link` 20 dp: dice «esce dall'app» (non solo il tipo di tocco).

Tipo → icona Lucide e parola (non solo icona: P8). `kind` sconosciuto → `link` «Fonte»:

| `kind` | Parola | Icona |
|---|---|---|
| `documentation` | Documentazione | `file-text` |
| `book` | Libro | `book` |
| `article` | Articolo | `newspaper` |
| `paper` | Studio | `flask-conical` |
| `video` | Video | `play-circle` |
| `course` | Corso | `graduation-cap` |
| `forum` | Forum | `message-square` |
| `community` | Community | `users` |
| `local` | In presenza | `map-pin` (senza link, vedi sotto) |
| altro | Fonte | `link` |

**Niente stelle, punteggi di affidabilità, «verificato», bollini**: tutte le fonti incluse sono per definizione ad alta fiducia (il Researcher scarta il resto); un voto introdurrebbe una precisione che non esiste e un'illusione di calibrazione.

### 8.2 Stati

| Stato | Aspetto |
|---|---|
| default | fill `surface` (della superficie del gruppo) |
| pressed | fill `surfacePressed`, 100 ms |
| focus (tastiera) | anello `focus` 3 dp dentro il bordo della riga |
| senza link (`url` assente, es. corso in presenza; o URL non `https`) | non tappabile: nessun trailing, nessun `Role.Button`; per l'URL non `https` si mostra l'URL in testo semplice, `ui.bodyS` `inkSoft`, selezionabile |
| offline | tappabile comunque: il browser mostra l'errore. Nessun «visitato» |
| lungo | titolo 200 car. a capo; annotazione e «Usala per» fino a 4 righe ciascuno a 600 dp, nessuna ellissi |

### 8.3 Apertura del link

- Tocco → apre `url` nel **browser di sistema** (Custom Tab: l'utente non perde il contesto, il tasto indietro torna all'app; se non disponibile, `Intent.ACTION_VIEW`). **Solo `https`** (come `primary_source`, chat §5). Nessun webview interno.
- L'host mostrato nella riga meta è **l'host reale** dell'URL (senza `www.`, punycode visualizzato com'è): l'utente vede dove va prima di toccare, e un titolo ingannevole non può mascherare il dominio.
- **Pressione lunga** (o tasto destro / Maiusc+F10): `ActionMenu` «Copia il link» · «Apri nel browser»; stesse azioni come `customAction`. Toast di sistema o «Link copiato» (come chat §4.1).
- Nessun contatore di aperture, nessuna marcatura di «già letta»: non misurano apprendimento.

### 8.4 Accessibilità

- **Un nodo** per voce: «<Tipo>: <titolo>, <dominio>. <annotazione>. Usala per: <use_for>. Collegamento esterno» con `onClickLabel` «Apri nel browser» e `Role.Button`. Voce senza link: stesso testo senza le ultime due parti.
- Intestazioni dei gruppi `heading()` livello 2 («Conoscenza, 6 fonti»); «Cosa non ho trovato» livello 2; i sottotitoli dei gruppi sono nodi normali letti dopo il titolo.
- **Target**: tutta la riga ≥ 96 dp di altezza × larghezza piena (> 48 dp); la voce non contiene altri tappabili (un solo target).
- **Ordine di lettura**: Indietro → titolo → «Altre azioni» → chip → tab → (banner) → gruppo Conoscenza (voci) → gruppo Community → Cosa non ho trovato → colonna laterale/riga riepilogo (ultima: è secondaria; `traversalIndex`). Con il banner §6.1: il banner viene prima delle fonti (è azionabile).
- **Contrasti** (da `design-system.md` §2.4, nessun nuovo colore): `ink`/`surface` 17,17 / 13,37; `inkSoft`/`surface` 9,35 / 10,08; `inkMuted`/`surface` 6,52 / 6,96 (fasi future); `info`/`infoSoft` 5,53 / 5,55; `inkSoft`/`surfaceSunken` 8,15 / 11,30; `border`/`surface` 4,08 / 5,16. Spunta fase: `correct` su `bg`, usata con l'etichetta di testo accanto.
- **Font 200%**: tutto a capo, nessuna altezza fissa; la riga meta può andare a capo su due righe (icona+tipo / dominio·anno); le fasi del pannello restano una sotto l'altra. A 328 dp × 200% l'annotazione è ≈ 18 caratteri per riga: limite noto del telefono (stessa nota di chat).
- **Tastiera fisica**: Tab percorre bottoni e voci nell'ordine di lettura; Invio/Spazio aprono la voce; Maiusc+F10 menu.
- **Non solo colore**: tipo = icona + parola; fase = icona + testo; gap = icona + testo; errore = icona + titolo.
- **Annunci TalkBack durante l'attesa**: una live region Polite persistente annuncia **solo i cambi di fase** (≤ 5 messaggi, es. «Ricerca: cerco sul web.»), mai i dettagli né il tempo; al completamento «Fonti pronte, 8 trovate.» Il focus non si sposta mai.

## 9. Motion

Solo `feedbackAppear` (200 ms) per la comparsa di pannello/banner, crossfade `fast` fra gli stati di §4, spinner. Con «riduci animazioni»: sostituzioni immediate, spinner sostituito da un anello fisso a 270° (stessa regola di `base-components` §9). Nessuna animazione celebrativa al completamento: arrivare alla lista è già la ricompensa (P9).

## 10. Proposte di contratto (per il tech lead)

**Job di ricerca** (`kind: "research"`, contratto `api-conventions.md` §9):
- `progress.step` ∈ `planning` | `searching` | `reading` | `evaluating` | `saving`; `progress.message` opzionale in italiano ≤ 80 caratteri (dettaglio fattuale: «Ho consultato 14 risultati»); `progress.percent` **sempre `null`** (l'app lo ignora comunque). Aggiornamenti via SSE `job_updated` (coalescenza ≤ 1/s lato backend).
- `error.code` ∈ `upstream_failed`, `rate_limited`, `timeout`, `internal`.
- `result`: `{"resource_count": 8, "gap_count": 2}`.

**Avvio/riavvio**:
- La conferma della mission crea il job (già previsto, mvp §7.1). Nuovo endpoint `POST /topics/{id}/resources/research` → `202` + job; `409 conflict` se un job di ricerca è già in corso; rifiuta (`429 rate_limited`) più di una ricerca ogni, ad esempio, 10 minuti per argomento (controllo costi).

**`GET /topics/{id}/resources`** → envelope:
```json
{
  "research": { "job_id": "...", "finished_at": "2026-11-09T10:02:11Z",
                "mission_version": 2, "communities_skipped": false },
  "knowledge": [ <Resource> ],
  "wisdom":    [ <Resource> ],
  "gaps": [ "Esempi di test d'integrazione con sqlx" ],
  "job": { /* ultimo job di ricerca o null */ }
}
```
`Resource`:
| Campo | Tipo | Note |
|---|---|---|
| `id` | uuid | |
| `kind` | enum | `documentation`, `book`, `article`, `paper`, `video`, `course`, `forum`, `community`, `local`; l'app tratta i valori sconosciuti come «Fonte» |
| `title` | string ≤ 200 | |
| `url` | string \| null | `https` per le fonti online; `null` per `local` |
| `domain` | string | host normalizzato senza `www.` (calcolato dal backend; l'app lo ricalcola da `url` se assente) |
| `annotation` | string ≤ 240 | italiano, cosa copre |
| `use_for` | string ≤ 200 | italiano, a cosa serve per la mission (frase nominale, senza «Usala per:») |
| `published_on` | string \| null | `YYYY` o `YYYY-MM-DD`, se noto; l'app mostra solo l'anno |

- L'ordine dentro ogni gruppo è stabile e documentato (rilevanza decrescente scelta dal Researcher); l'app non riordina.
- `mission_version` = versione di mission usata dal Researcher (serve al banner §6.1).
- `communities_skipped`: `true` se la ricerca non ha cercato community (dal 2026-11-15 è il caso normale: si cercano solo su richiesta): il gruppo `wisdom` è vuoto e l'app mostra la riga di §4.3.
- Eventi SSE: `job_updated` già previsto; `resources_updated` (opzionale) a ricerca conclusa, o basta `topic_updated` + `job_updated`. Bastano quelli esistenti se `GET /resources` si ricarica al `succeeded`.
- Un solo elenco alla volta: una nuova ricerca riuscita **sostituisce** atomicamente l'elenco precedente; se fallisce, l'elenco precedente resta.

## 11. Aperto

1. **Stato del Topic dopo un fallimento** della *prima* ricerca: resta `researching` (il chip dice «Ricerca da riprovare», le lezioni non sono raggiungibili) o ritorna a `mission_proposed`/stato apposito? Il design assume «resta `researching`, solo Riprova». Se si vuole permettere «Continua senza fonti» serve una decisione di prodotto: il Generator senza fonti è più incline all'errore (violerebbe l'idea di base di Matt), quindi **lo sconsiglio** per la v1.
2. **Cambio di mission e ricerca**: il design propone (non avvia) il rifacimento. Se il tech lead preferisce un controllo lato backend (es. il Tutor segnala che lo scopo è cambiato in `propose_mission_change`, `scope_changed: true`), il banner potrebbe comparire solo in quel caso. Utile ma non necessario.
3. Aggiunta manuale di una fonte da parte dell'utente / rimozione di una fonte: **fuori v1** (nessun endpoint); se arriverà, la riga avrà un menu `more-vertical`.

## 12. Indicazioni per llm-engineer (Researcher)

- **Annotazione** = cosa copre, una-due frasi, italiana, senza aggettivi di marketing; **`use_for`** = frase nominale che completi «Usala per: …» (es. «capire perché il compilatore rifiuta le reference»), legata a un punto concreto della mission, non generica («approfondire», «imparare Rust»).
- Meno ma migliori: 5-12 fonti totali; Knowledge prima per importanza; Community solo se l'utente le ha chieste (dal 2026-11-15, issue #15).
- **Gap**: una riga ciascuno, in forma di **argomento** non trovato («Esempi di test d'integrazione con `sqlx`»), ≤ 100 caratteri, senza giudizi e senza proposte di rimedio; solo lacune vere rispetto ai criteri di successo della mission.
- `title` e `domain` reali (nessun URL inventato; URL verificati dalla pagina scaricata); solo `https`.
- `progress.message`: fatti già avvenuti, in italiano, brevi; mai stime di tempo né percentuali.


## 13. «I tuoi documenti» (Fase 10, PDF caricati)

Contratto: `docs/contracts/documents.md`, `resources.md` v1.1. Terminologia per l'utente: «documento», «PDF» (mai `Document`, `Resource`, «import», «job», «sezione dati»).

### 13.1 Scopo didattico

Un PDF è la fonte che l'utente **ha scelto lui**: un libro del corso, una dispensa, i suoi appunti. Cambia il rapporto con le lezioni: non «il sistema sceglie cosa insegnarmi», ma «il sistema mi guida dentro il materiale che devo studiare». Il design lo rende evidente (Mayer, **segnalazione** e **coerenza**: l'utente deve vedere che le lezioni seguono quel materiale) senza aggiungere altro da gestire. Principi:
- **Autonomia (Deci e Ryan)**: carica chi vuole, quando vuole, in qualunque stato del Topic. Nessun obbligo, nessuna ricerca automatica (costa).
- **Fiducia calibrata**: dire cosa succede al file («il file resta sul tuo dispositivo, io tengo il testo») e cosa non si può leggere, prima che l'utente aspetti per niente.
- **Carico cognitivo**: durante l'import una sola riga di stato; gli errori dicono **una** cosa da fare.
- **Pre-training**: l'indice del documento è visibile (e il Tutor lo usa nella chat): vedere la struttura prima aiuta a dire cosa studiare.
- Niente «documenti letti», percentuali, voti di qualità del PDF.

### 13.2 Posizione e layout

- La sezione sta **sempre in cima** alla colonna principale della tab (expanded: colonna 600 dp; compact: a tutta larghezza), **sopra** pannello di ricerca, banner e gruppi di fonti web. Esiste in tutti gli stati A-E (anche prima della conferma della mission), dopo il primo `GET /topics/{id}/documents` (caricamento: 1 riga skeleton, come §4).
- Header: `ui.title3` «I tuoi documenti · 2» (il «· N» solo se N ≥ 1), sotto `ui.bodyS` `inkSoft`: «Le lezioni seguono questi PDF, insieme alle fonti del web.» (omessa se N = 0: la dice lo stato vuoto). Heading livello 2 per TalkBack.
- Nello stato A (mission non confermata) sotto la sezione resta il contenuto di §4.1, più, solo se N ≥ 1, una riga `ui.bodyS` `inkSoft` sotto l'elenco: «Quando confermi la mission, cerco anche fonti sul web che completino il tuo PDF.»
- Le fonti con `kind: document` presenti in `GET …/resources` **non si mostrano** nei gruppi Conoscenza/Community e **non si contano** in «8 fonti: …» (riepilogo §3: «8 fonti web»): i documenti si vedono solo qui. Il banner di §6.1 e il bottone «Aggiorna le fonti» riguardano solo il web.
- Margine: 32 dp (24 compact) tra la sezione e il blocco successivo.

```
┌──────────── 600 dp ────────────┐
│ I tuoi documenti · 1           │ ui.title3
│ Le lezioni seguono questi PDF, │ ui.bodyS inkSoft
│ insieme alle fonti del web.    │
│ ┌────────────────────────────┐ │ Surface static (stile ListRow, §8)
│ │ 📄 Rust in Action      🗑  │ │ riga documento (§13.4)
│ │ 456 pagine · in inglese    │ │
│ │ rust-in-action.pdf         │ │
│ │ ▸ Indice · 31 sezioni      │ │
│ └────────────────────────────┘ │
│ [ ⬆ Carica un PDF ]            │ Button secondario, 48 dp
│ (banner «Aggiorna le fonti»)   │ §13.8, solo se serve
└────────────────────────────────┘
```

### 13.3 Stato vuoto (0 documenti, nessun upload in corso)

`Surface static` (fill `surface`, bordo `line`, raggio `md`, padding 20/16), non un'illustrazione (Mayer: coerenza). Contenuto:

- icona `file-text` 24 `inkSoft`; titolo `ui.title3` «Studia dal tuo PDF».
- corpo `ui.body` `ink`: «Carica un libro, una dispensa o i tuoi appunti in PDF. Le lezioni ne seguiranno le pagine e le citeranno, insieme alle fonti che trovo sul web.»
- nota `ui.bodyS` `inkSoft` (tre righe separate, 8 dp): «Servono PDF con testo selezionabile: le scansioni non riesco a leggerle.» · «Fino a 5 documenti, 50 MB l'uno.» · «Il file resta sul tuo dispositivo; io tengo il testo, e le pagine che usiamo in una lezione vengono inviate al modello che la scrive.»
- `Button` secondario «Carica un PDF» (icona `file-up` 20, 48 dp), sotto, allineato a sinistra.

### 13.4 Riga di un documento (`DocumentRow`)

Stessa famiglia di `ResourceRow` (§8): riga in `Surface static`, separatori `line` 1 dp inset 16, padding 16, min 96 dp. A differenza di `ResourceRow` la riga **non è un target unico**: contiene due controlli (indice e Rimuovi). Nessun link esterno.

**Documento pronto (`status: ready`)**

```
│ 📄  Rust in Action                          🗑 │ icona file-text 24 inkSoft; titolo ui.title3 ink (a capo, mai ellissi); trash-2 20 inkSoft in target 48x48
│     456 pagine · in inglese                    │ ui.body ink
│     rust-in-action.pdf                         │ ui.bodyS inkSoft, solo se filename (senza .pdf) ≠ title
│     ▸ Indice · 31 sezioni                      │ Disclosure (base-components), 48 dp
```

- Riga meta: «{N} pagine» (singolare «1 pagina»; `page_count` null → omesso) · lingua solo se `language` ≠ `it` e non `null`: «in inglese» (nome nella lingua del dispositivo; codice sconosciuto → omesso). La lingua serve: le lezioni sono in italiano e i termini restano nell'originale (D3), l'utente sa cosa aspettarsi.
- **Indice (sezioni): sì, ma sola lettura, chiuso di default.** Perché serve: dà la struttura prima dei contenuti (advance organizer), permette di dire al Tutor «voglio studiare i capitoli 3-5» e di verificare che le pagine citate nelle lezioni tornino. Perché non di più: il contratto non prevede di scegliere le sezioni dall'app (le sceglie il Planner), quindi **niente caselle, niente tocchi sulle voci**. Mostrato solo se `sections.length ≥ 2`; 0-1 sezioni: la riga Indice non compare.
  - Aperto: righe `ui.body` `ink`, rientro `16 dp × (level − 1)`, a destra `ui.bodyS` `inkSoft` «pp. 1-24» (una sola pagina «p. 5»; numeri tabulari), min 40 dp, nessun separatore. In cima `ui.bodyS` `inkSoft`: «Nella chat puoi dirmi quali parti vuoi studiare.» Le righe sono item della LazyColumn della pagina (fino a 300), non una lista annidata.
  - I blocchi automatici «Pagine 1-10» (documento senza segnalibri) si mostrano come arrivano, senza la colonna pagine duplicata (si omette il «pp.» di destra quando il titolo comincia per «Pagine»).
- **Rimuovi**: `IconButton` `trash-2` con `contentDescription` «Rimuovi «Rust in Action»» (tooltip lungo, long-press). Apre il dialog di §13.7. Non compare mentre il documento sta `importing` (il backend risponde `409`).

**Documento in elaborazione (`status: importing`, o upload in corso)**

```
│ 📄  rust-in-action.pdf                           │ titolo = filename finché il titolo non è noto
│     Estraggo il testo · pagina 120 di 456        │ ui.body ink (live region Polite solo al cambio di fase)
│     ▬▬▬▬▬ (indeterminata)                        │ ProgressBar lineare 4 dp, indeterminata
```

Fasi (`progress.step`, nessuna percentuale; la barra è sempre indeterminata e **non riempie mai**, perché `done/total` non è tempo rimanente):

| Momento | Testo |
|---|---|
| upload (prima del `202`, riga locale) | «Carico il file…» + a destra `Button` testuale «Annulla» (interrompe la richiesta, la riga sparisce) |
| `queued` | «In attesa di iniziare…» |
| `reading` | «Apro il file…» |
| `extracting` | «Leggo il testo · pagina {done} di {total}» (se `total` assente: «Leggo il testo…»); aggiornato al massimo ogni 1 s |
| `sectioning` | «Cerco la struttura del documento…» |
| `saving` | «Salvo il testo…» |
| sconosciuto | «Elaboro il documento…» |

Aggiunta fissa sotto, `ui.bodyS` `inkSoft`: «Puoi lasciare l'app: continuo sul server.» Niente tempi stimati. Alla fine (job `succeeded`): la app ricarica `GET …/documents` e `…/resources`; la riga passa a «pronto» con crossfade `fast`; Toast «Ho letto «Rust in Action»: 456 pagine.» (4 s); annuncio Polite uguale. Nessuna animazione celebrativa.

Se l'app perde l'SSE: stesso fallback di §4 (`GET /jobs/{id}`).

### 13.5 Errori dell'import (documento `failed`)

La riga resta in elenco in stile errore: icona `alert-circle` 24 `danger` al posto di `file-text`, titolo = filename, sotto un `StatusPanel danger` **incorporato nella riga** (non un secondo pannello): titolo `ui.labelL`, corpo `ui.body`. Errore = icona + titolo + testo (mai solo colore, mai vergogna: il file non è «sbagliato», non è leggibile). Azione primaria della riga: `Button` secondario **«Rimuovi»** (48 dp, nessun dialog di conferma: il documento non ha prodotto nulla; Toast «Documento rimosso.»). Il codice viene dal job `document_import` fallito del documento (vedi proposta C1).

| `error.code` | Titolo | Corpo |
|---|---|---|
| `no_text` | Questo PDF è una scansione | Le pagine sono immagini e non contengono testo che io possa leggere. Cerca una versione con testo selezionabile (prova a selezionare una parola con il dito) e caricala. |
| `encrypted_pdf` | Il PDF è protetto da password | Non posso aprirlo. Toglila dal tuo lettore PDF, salva una copia senza protezione e caricala di nuovo. |
| `too_many_pages` | Il PDF è troppo lungo | Ha più di 1.500 pagine. Dividilo in parti, per esempio per capitoli, e carica quella che vuoi studiare. |
| `invalid_pdf` | Non riesco ad aprire il file | Sembra danneggiato o non è un PDF. Prova ad aprirlo sul dispositivo: se si apre, salvalo di nuovo come PDF e riprova. |
| altro (`timeout`, `internal`, …, ritentabile per `jobs.md`) | Non sono riuscito a leggere il PDF | È un problema mio, non del file. Puoi riprovare. Azioni: **«Riprova»** (secondario, `POST …/jobs/{id}/retry`) e testuale «Rimuovi». |
| nessun dettaglio (job non trovato) | come «altro» ma senza «Riprova» | |

Il `message` del backend non si mostra: i testi sopra sono più utili e coerenti (resta in `Disclosure` «Dettagli» solo per «altro»). Il documento `failed` non blocca nuovi caricamenti (non è un import attivo) e **conta nel limite di 5** finché non lo si rimuove (§13.6).

### 13.6 Caricamento, limiti ed errori di rete

- **«Carica un PDF»** apre il selettore di file di sistema (Storage Access Framework, `ACTION_OPEN_DOCUMENT`, tipo `application/pdf`; nessun permesso di storage). Annullare il selettore non fa nulla. Il file scelto con tipo diverso (il selettore a volte lo permette) → 415 sotto.
- **Limite 5**: con 5 documenti (di qualunque stato) il bottone **non compare**; al suo posto `ui.bodyS` `inkSoft`: «Hai raggiunto il limite di 5 documenti. Rimuovine uno per caricarne un altro.» (un controllo disabilitato nasconderebbe il motivo).
- **Un import alla volta** (`409`): finché un documento è `importing` o in upload, al posto del bottone: «Sto leggendo un documento: potrai caricarne un altro tra poco.» (`ui.bodyS` `inkSoft`). Quando finisce il bottone riappare da solo.
- **Errori del caricamento** (nessuna riga creata): `StatusPanel danger` compatto **sopra il bottone**, live region Polite, con «Chiudi» (testuale); non è un Toast perché richiede un'azione.

| Caso | Titolo | Corpo | Azioni |
|---|---|---|---|
| `413` | Il file è troppo grande | Il limite è 50 MB. Prova a comprimere il PDF o a dividerlo in parti. | Chiudi |
| `415` | Questo non è un PDF | Per ora leggo solo file PDF. | Chiudi |
| rete / timeout / 5xx | Non sono riuscito a caricare il file | Controlla la connessione e riprova. Il file non è stato modificato. | **Riprova** (riusa lo stesso URI, senza riaprire il selettore) · Chiudi |
| `409` (limite o import attivo) | Ricarica l'elenco e mostra la riga di §13.6 corrispondente; nessun pannello | | |
| `404` (argomento eliminato altrove) | torna alla Home con il Toast di `topic.md` | | |

La dimensione si può controllare prima dell'invio (l'URI la dà): se > 50 MB, mostra direttamente il pannello `413` senza caricare.

### 13.7 Conferma dell'eliminazione

`ConfirmDialog` di `base-components` §8 (focus iniziale su Annulla):
- Titolo `ui.title2`: «Rimuovere il documento?»
- Corpo `ui.bodyL`: ««Rust in Action» non sarà più una fonte per le prossime lezioni. Le lezioni che hai già fatto restano come sono, con le loro citazioni. Il file sul tuo dispositivo non viene toccato.»
- Azioni: «Annulla» · **«Rimuovi»** (distruttivo). Errore di rete: slot errore del dialog (`base-components` §8). `409`: «Sto ancora leggendo il documento. Riprova tra poco.»
- Esito: riga che esce (`fast`), Toast «Documento rimosso.» Nessuna annotazione sulle fonti web (non cambiano). Se erano le ultime citazioni di una lezione già fatta, nulla: restano leggibili (`documents.md` §3).

### 13.8 Quando proporre «Aggiorna le fonti» dopo un caricamento

La ricerca web costa: **mai automatica**, **mai in un dialog**, **mai subito dopo il tocco di «Carica»**. Si propone solo quando c'è qualcosa da guadagnare: un documento pronto che il Researcher non conosceva.

- **Condizione**: Topic `ready` (o con ricerca già riuscita), nessun job di ricerca in corso, esiste almeno un documento `ready` con `created_at` successivo a `research.finished_at`, e l'utente non l'ha già scartato.
- **Non si propone** (il primo giro lo include): prima della conferma della mission, durante la prima ricerca, a documento ancora `importing` o `failed`, dopo una rimozione.
- **Forma**: `StatusPanel info` come il banner di §6.1, **in fondo alla sezione documenti** (sotto «Carica un PDF»), non in cima alla tab. Titolo «Ho letto il tuo PDF» (più documenti: «Ho letto i tuoi PDF»). Corpo: «Le fonti del web sono state scelte prima, senza conoscerlo. Se vuoi, cerco altre fonti che lo completino: ci vogliono pochi minuti. Le lezioni useranno il PDF in ogni caso.» Azioni: `Button` primario «Aggiorna le fonti» (unico primario della tab: se c'è anche il banner di §6.1 **si fondono**: titolo «Le fonti possono essere aggiornate», corpo §6.1 + «Hai anche aggiunto un PDF dopo l'ultima ricerca.», un solo bottone, e il banner sta dove sta §6.1) e `Button` testuale «Non ora».
- «Non ora» (o aggiornare) nasconde il banner per quei documenti (stato locale per `document_id`); non è un'azione persa: «Aggiorna le fonti» resta nella colonna/riga riepilogo (§3).
- Dopo il tocco: stato E (§6.2). Le fonti `document` non si toccano e non compaiono tra le «Nuove».
- Il banner non interrompe, non ha contatori né badge sulla tab.

### 13.9 Accessibilità

- Riga pronta, nodo unico di lettura (i due controlli restano azioni separate): «Documento: Rust in Action, 456 pagine, in inglese.» poi «Indice, 31 sezioni, chiuso» (`Role.Button`, `expanded`) e «Rimuovi «Rust in Action»».
- Riga in elaborazione: `stateDescription` = testo della fase; live region **Polite persistente**, annuncia i **cambi di fase** (≤ 5), mai il numero di pagina. Fine: «Ho letto «Rust in Action», 456 pagine.» Fallimento: titolo dell'errore + corpo (Polite); il focus non si sposta da solo.
- Ordine di lettura: header sezione → righe → bottone/limite → errore di upload → banner §13.8 → resto della tab. Il pannello d'errore di upload è un nodo con `paneTitle`.
- Target: bottone 48 dp, `trash-2` in 48 × 48, Disclosure 48 dp. Il selettore è del sistema (accessibile di suo).
- Font 200%: titoli a capo, le righe indice mettono «pp.» sotto il titolo; niente altezze fisse. Tastiera: Tab percorre bottone, righe, cestino, indice; Invio/Spazio attivano.
- Contrasti: solo coppie già verificate in `design-system.md` §2.4 (`ink`/`surface`, `inkSoft`/`surface`, `danger` e `info` sui rispettivi fondi `StatusPanel`). Nessun colore nuovo.

### 13.10 Motion

`feedbackAppear` per i pannelli di errore e il banner; crossfade `fast` tra importing → pronto; riga rimossa `fast`. Con «riduci animazioni»: sostituzioni immediate, barra indeterminata sostituita dal solo testo di fase.

### 13.11 Proposte di contratto (per il tech lead)

- **C1, errore del documento**: aggiungere a `Document` (se `failed`) `error: { code, message }` oppure `job_id`. Oggi l'app, aperta su un altro dispositivo o dopo un riavvio, non sa *perché* un documento è `failed` senza cercare il job tra `GET /topics/{id}/jobs`. Fallback nella spec: testo generico + «Rimuovi» (+ «Riprova» se il job si trova).
- **C2, documenti conosciuti dalla ricerca**: `research.document_ids` (ID dei documenti `ready` visti dall'ultima ricerca) in `GET …/resources`, per decidere il banner §13.8 senza confronti tra orari. Fallback: `created_at` > `research.finished_at`.
- **C3 (opzionale)**: `Document.ready_at`. Utile a C2 se non si adotta `document_ids`.
