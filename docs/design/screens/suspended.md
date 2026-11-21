# Schermata Schede sospese (issue #12)

> Contratto: `../../contracts/reviews.md` v1.3 (§2 `suspension`/`suspended_at`, §3.2 `unsuspend`, §3.3 `GET /reviews/suspended`). Componenti esistenti: `../base-components.md` (TopBar §6, ListRow §4, Button §1, StatusPanel §5, Toast §9, Skeleton §10.4, ErrorState §10.5). **Nessun token e nessun componente nuovo.** Sotto-schermata di Impostazioni, `settings/suspended`.
> Mostra e riattiva **solo** le schede sospese dall'utente (`suspension: user`); quelle superate da un record nuovo non compaiono (decisione del proprietario).

## 1. Scopo e accesso

Scopo: rimediare a una sospensione fatta per errore o ripensata (un'idea che torna utile). È un'azione **rara**: non merita spazio nella Home (P2, coerenza: la Home resta «cosa faccio oggi») né un badge (niente elementi che chiedono attenzione senza una decisione da prendere).

**Accesso: Impostazioni, nuova sezione «Ripasso»**, tra «Password» e «Notifiche» (aggiornato in `settings.md`).

```
Ripasso
┌──────────────────────────────────────┐
│ ⏸  Schede sospese               3  › │  ListRow 56 dp; trailing: conteggio ui.body inkSoft + chevron
└──────────────────────────────────────┘
```

- Nome: **«Schede sospese»**. Icona leading `pause` (la stessa della voce «Sospendi questa scheda»: stesso concetto, stessa icona, P6).
- **Visibile solo se `suspended_count` > 0.** Impostazioni all'apertura chiama `GET /reviews/suspended?limit=1`; finché non risponde, o se fallisce, la sezione **non c'è** (nessun errore: è una scorciatoia, non un'informazione dovuta). Dopo una riattivazione che porta il conteggio a 0 la sezione sparisce al ritorno.
- Il testo della conferma di «Sospendi» e dell'`UndoRow` in `review.md` indica questo percorso (§3 qui sotto e `review.md` §6.1).
- Descrizione TalkBack della riga: «Schede sospese, {n}. Apre l'elenco.»

## 2. Layout

Colonna centrata, `contentMaxWidth` 840 dp (lista di consultazione, come «Componenti mancanti»); niente list-detail. `LazyColumn`, margini 16 dp (`screenMarginMedium`/`Expanded` oltre), gap tra righe 0 con separatore hairline `line` inset 16 dp dentro una `Surface` statica (come ListRow).

```
Expanded / medium (colonna 840 dp)
┌─────────────────────────────────────────────────────────────┐
│ ←  Schede sospese                                           │ TopBar
├─────────────────────────────────────────────────────────────┤
│ 3 schede sospese                                            │ ui.label inkSoft (suspended_count)
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ Quando compila l'operatore ? dentro una     [ Riattiva ]│ │ front: ui.bodyL ink, max 3 righe
│ │ funzione?                                                │ │
│ │ Rust · Lezione 2: Result e operatore ?                   │ │ ui.body inkSoft, 1 riga + ellissi
│ │ Sospesa ieri                                             │ │ ui.bodyS inkSoft
│ ├ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤ │
│ │ …                                                        │ │
│ └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘

Compact (360 dp): il bottone passa sotto il testo
│ Quando compila l'operatore ? dentro una │
│ funzione?                               │
│ Rust · Lezione 2: Result e operatore ?  │
│ Sospesa ieri                            │
│ [ ↻ Riattiva ]                          │  Button secondario, 48 dp, larghezza del contenuto
```

**Per riga**: fronte (solo il fronte: il retro non serve a riconoscere la scheda e rivelarlo qui sarebbe un ripasso involontario, P7), Topic e lezione («{topic_title} · Lezione {lesson_number}: {lesson_title}»), quando è stata sospesa. Il fronte rende il markdown inline come in ripasso, senza tocco per aprire (la riga non è cliccabile; l'unico target è «Riattiva»). Padding riga 16 × 12 dp, altezza min 72 dp, cresce col font.

**Quando** (da `suspended_at`, fuso locale): «Sospesa oggi» · «Sospesa ieri» · «Sospesa il 12 nov» · «Sospesa il 12 nov 2025» (anno solo se diverso). L'ordine è quello del backend (più recenti prima): non riordinare.

## 3. Riattivazione

**Un tocco, senza conferma.** Motivo: è reversibile (Annulla nel toast), non perde dati e non prende nessuna decisione pesante; una conferma sarebbe attrito per un'azione che l'utente ha appena scelto dalla lista (P2). La sospensione, al contrario, resta con conferma in `review.md`.

Sequenza:
1. Tocco su «Riattiva»: il bottone passa a loading («Riattivo…», spinner 20 dp, `busy`); gli altri «Riattiva» restano attivi. `POST /reviews/{id}/unsuspend`.
2. `200`: la riga esce con collasso in altezza 200 ms (istantaneo con «riduci animazioni»), il conteggio scende di 1, il focus TalkBack passa alla riga successiva (o, se non c'è, al titolo della TopBar). Compare un **Toast** per 6 s con azione:
   - Scheda **già in scadenza** (`due` ≤ adesso): «Scheda riattivata. È subito nel ripasso.» (è la verità: la scheda ha mantenuto la sua scadenza ed è scaduta; niente «in ritardo»).
   - Scheda **non ancora in scadenza**: «Scheda riattivata. La rivedrai {tra …}.» con l'intervallo `due − adesso` formattato come `review.md` §4.5 (es. «tra 3 giorni»).
   - Azione **«Annulla»**: `POST /reviews/{id}/suspend`; la riga torna in cima alla lista (il backend darà alla scheda un nuovo `suspended_at`, quindi «Sospesa oggi»), conteggio +1. Se la richiesta fallisce: Toast 4 s «Non riesco ad annullare ora. La scheda resta riattivata.»
3. Annuncio Polite del testo del Toast (il focus non si sposta sul Toast).
4. Se era l'ultima, si mostra lo stato vuoto (§4) con il Toast ancora visibile.

**`409 card_superseded`** (la scheda è stata superata nel frattempo): la riga esce con lo stesso collasso, conteggio −1, Toast 6 s **senza azione**: «Questa scheda è stata sostituita da una versione più recente e non si può riattivare.» (nessun tono d'errore: non è colpa dell'utente, niente `danger`).
**`404`**: la riga esce in silenzio.
**Errore di rete / `5xx`**: la riga resta; sotto il testo compare in linea (live region Assertive per 1 annuncio) icona `cloud-off` 20 `inkSoft` + «Non riesco a raggiungere il backend. La scheda è ancora sospesa.» e il bottone diventa «Riprova». Niente Toast per gli errori che chiedono un'azione (`base-components.md` §9). La riattivazione **non** si mette in coda offline (serve l'esito certo).

## 4. Stati della lista

| Stato | Contenuto |
|---|---|
| Caricamento (primo) | 3 righe scheletro (`Skeleton` §10.4, shimmer spento con «riduci animazioni»); a11y `busy` «Carico le schede sospese». Dopo 8 s «Sto ancora caricando…»; timeout 10 s → errore |
| Con dati | §2. Intestazione «{n} schede sospese» / «1 scheda sospesa» (da `suspended_count`, aggiornato localmente) |
| Vuoto (`items` vuoti, o dopo l'ultima riattivazione) | centrato: icona `pause` 48 dp `inkSoft`, titolo `ui.title2` «Nessuna scheda sospesa», corpo `ui.bodyL` «Le schede che sospendi dal ripasso compaiono qui, e da qui le riattivi.» Nessun bottone (Indietro basta) |
| Errore di caricamento | `ErrorState` (§10.5): «Non riesco a raggiungere il backend» / «Controlla che il dispositivo sia collegato alla rete e che il backend sia acceso.» + «Riprova». `5xx`: «Il backend ha avuto un problema» / «Riprova fra poco.» `401`: si apre l'accesso (`login.md`) |
| Paginazione | `limit=50`, scroll infinito col `next_cursor`; spinner di coda 24 dp col testo «Carico altre schede»; se la pagina fallisce, riga di coda «Non riesco a caricarne altre» + «Riprova». Dopo una riattivazione/annulla non si ricarica da zero: si aggiorna la lista locale |
| Aggiornamento | si ricarica a ogni apertura (nessuna cache). Niente pull-to-refresh: nessuna schermata dell'app lo ha e la lista cambia solo per azioni fatte qui (tech lead, 2026-11-16) |

## 5. Accessibilità e stringhe

- Ogni riga è **un gruppo** (non cliccabile) letto: «{fronte}. {topic}, lezione {n}, {titolo}. Sospesa {quando}.»; poi il bottone, descrizione «Riattiva la scheda: {primi 60 caratteri del fronte}» (distinguibile fuori contesto, quando TalkBack elenca i bottoni), `Role.Button`. Azione personalizzata sul gruppo: «Riattiva».
- Target ≥ 48 dp; stato «Riattivo…» comunicato a testo, non solo dallo spinner. A font 200% il bottone va sotto il testo anche su tablet e il fronte non ha limite di righe.
- Nessun significato solo col colore: tutta la schermata è `ink`/`inkSoft`; l'unico colore è l'accento del bottone.
- Tastiera: Tab tra i «Riattiva»; Invio attiva; Ctrl+Z annulla l'ultima riattivazione finché il Toast è visibile.
- Stringhe riassunte: «Schede sospese», «Ripasso», «Riattiva», «Riattivo…», «Riprova», «Nessuna scheda sospesa», «{n} schede sospese», «Sospesa oggi/ieri/il {data}», i due Toast di esito, il Toast del `409`, «Non riesco ad annullare ora. La scheda resta riattivata.» (testi completi sopra).

## 6. Evitare

Conferma per riattivare; badge o contatore rosso su Impostazioni; la sezione visibile con 0 schede; mostrare il retro; mostrare le schede `superseded` «grigie»; contare le riattivazioni come un risultato; parole come «in ritardo», «scadute», «recuperate».
