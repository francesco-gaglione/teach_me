# Schermata Impostazioni

> Componenti: `../base-components.md` (TopBar, TextField, Button, StatusPanel, StatusChip, ListRow, Toast, ConfirmDialog, composizioni di §16). Token: `../tokens.json`.
> Raggiungibile da Home (icona `settings`) e dai messaggi su chiave LLM mancante o rifiutata (`../llm-key.md`). **Non è più la schermata iniziale**: il primo avvio porta all'accesso (`login.md`). Rivista in Fase 11 (multiutenza): sparisce il token, entrano utente, uscita, password e chiave LLM.

## Scopo

Tre cose, in quest'ordine di frequenza: **sistemare la chiave LLM** (l'unica impostazione da cui dipende il fatto che l'app insegni), **sapere con quale account si è collegati e uscire**, **cambiare la password**; più la verifica della connessione per chi ha un problema di rete. Una cosa per sezione, ogni sezione con il proprio titolo (segmentazione, Mayer). Nessun elemento didattico da proteggere qui: conta la chiarezza e il non sbagliare azioni irreversibili.

## Layout (compact, 360 dp)

```
┌──────────────────────────────────────┐
│ ←  Impostazioni                      │  TopBar (ui.title2), indietro sempre visibile
├──────────────────────────────────────┤
│ Account                              │  intestazione di sezione: ui.label, inkSoft
│ ┌──────────────────────────────────┐ │  Surface static
│ │ Valentina                        │ │  display_name, ui.title3, ink
│ │ @valentina                       │ │  username, ui.body, inkSoft
│ │ [ ⎋  Esci ]                      │ │  Button secondario 48 dp (icona log-out)
│ └──────────────────────────────────┘ │
│                                      │
│ Chiave LLM                           │
│ ┌──────────────────────────────────┐ │  Surface static (stati: §Chiave LLM)
│ │ ( ✓ Impostata )                  │ │  StatusChip
│ │ La chiave serve a scrivere le    │ │
│ │ lezioni… Non la mostro mai.      │ │
│ │ [ Sostituisci ] [ Rimuovi ]      │ │
│ └──────────────────────────────────┘ │
│                                      │
│ Password                             │
│  Cambia password                 ›   │  ListRow 56 dp → settings/password
│                                      │
│ Connessione                          │
│ ┌──────────────────────────────────┐ │
│ │ Backend  teach.tailnet.ts.net    │ │  riga di sola lettura
│ │ [  Prova connessione  ]          │ │  Button secondario
│ │ (✓) Connesso …                   │ │  StatusPanel (live region)
│ └──────────────────────────────────┘ │
│ ──────────────────────────────       │  hairline
│  Componenti mancanti             ›   │  ListRow 56 dp
│  Licenze open source             ›   │  ListRow 56 dp
└──────────────────────────────────────┘
```

Colonna scorrevole, margini 16 dp, gap fra sezioni 24 dp, intestazioni di sezione 24 dp sopra e 8 dp sotto (come «Cartelle» in Home). **Larghezza massima `readingMaxWidth` 600 dp** (prima 840): ora la schermata contiene moduli, e un campo password a 840 dp spinge righe troppo lunghe; la sotto-schermata «Componenti mancanti» resta a 840. In medium/expanded la colonna è centrata, margini `screenMarginMedium`/`screenMarginExpanded`; nessun list-detail (è una schermata di servizio, un solo compito alla volta). Con la tastiera aperta (`imePadding`) il campo attivo, il suo errore e il bottone restano visibili.

Ordine delle sezioni: Account, Chiave LLM, Password, **Ripasso** (solo se `suspended_count` > 0: una ListRow 56 dp «Schede sospese» con conteggio, che apre `suspended.md`; assente durante il caricamento e in caso di errore), Notifiche (ADR 0012), **Ascolto** (solo se la lettura ad alta voce è attiva: `lesson-audio.md` §4), Connessione, poi le due righe di servizio. La chiave sta sopra la password perché è l'azione che l'utente viene a fare dai messaggi di errore e al primo accesso.

### Ingresso diretto dalla chiave (`settings?section=llm-key&reason=missing|rejected`)

Aperta da un messaggio su chiave mancante o rifiutata (`../llm-key.md`): la lista scorre in modo che la sezione «Chiave LLM» sia **in cima** (sotto la TopBar), la sezione è **già in modalità di scrittura** (campo visibile, anche se la chiave era «Impostata»: nel caso `rejected` si sostituisce) e il campo prende il **focus con la tastiera**. Indietro riporta alla schermata di partenza intatta (bozze, scroll). Non c'è un ingresso diretto per le altre sezioni.

## Sezione Account

| Elemento | Specifica |
|---|---|
| Nome | `display_name` (`ui.title3`, `ink`), a capo, mai troncato |
| Nome utente | «@» + `username` (`ui.body`, `inkSoft`); il «@» è solo grafico, la `contentDescription` legge «Nome utente: valentina» |
| **Esci** | Button secondario con icona `log-out`, 48 dp, a tutta larghezza in compact, a destra e largo quanto il testo da medium in su |
| Dati | letti dalla cache dell'ultimo `GET /me`; aggiornati ogni volta che la schermata si apre (`ON_START`, silenzioso: niente skeleton se c'è la cache). Se `GET /me` fallisce per rete si tiene la cache; se risponde `401` vale la regola globale di `login.md` |

Nessun avatar, nessuna iniziale in un cerchio, nessuna data di creazione: non aiutano a studiare né a decidere (Tufte).

### Esci

1. Tocco su «Esci». Se **non ci sono** voti di ripasso né risposte di lezione in coda: si esce subito, senza conferma (uscire è reversibile: basta rientrare). Se ce ne sono, **prima si tenta di inviarli**: il bottone diventa `loading` («Invio…»), al massimo 3 s, una richiesta alla volta come per la coda normale.
2. Se la coda si svuota: si esce senza dialog. Se restano elementi (rete assente, errore): **ConfirmDialog**.

| Elemento | Testo |
|---|---|
| Titolo | Ci sono dati non ancora inviati |
| Corpo | {coda} non sono ancora arrivati al server. Se esci ora restano su questo dispositivo e partiranno quando accederai di nuovo come «{nome utente}». Se disinstalli l'app prima, andranno persi. |
| Annulla (testuale, **focus iniziale**) | Annulla |
| Conferma (primario) | Esci comunque |

`{coda}` come in `login.md`: «1 voto di ripasso», «3 voti di ripasso», «2 risposte di lezioni», «3 voti di ripasso e 2 risposte di lezioni»; con un solo elemento il verbo è «non è ancora arrivato», con più «non sono ancora arrivati». La conferma è un Button **primario**, non distruttivo: nulla va perso (i dati restano, legati all'utente). Il dialog dice invece **quando** si perderebbero (disinstallare), che è l'unica informazione che serve a decidere. Il dialog segue le regole di `base-components.md` §8 e §11.

3. Esito di «Esci»: `POST /auth/logout` con timeout 5 s; **comunque vada** il token locale viene cancellato e si apre l'accesso (variante *Rientro*, Toast «Sei uscito.»). Se la chiamata non è andata a buon fine (rete assente) la sessione sul server scade da sola ma l'app non lo segnala: il token è già stato scartato dal dispositivo.
4. Si conservano: indirizzo, nome utente, dati locali legati all'utente (coda, bozze, cache). Si cancellano: token, password (mai stata salvata da noi), chiave (mai stata sul dispositivo). Su un tablet condiviso un altro utente non vede nulla del precedente: ogni archivio è per utente (`ADR 0008`, Conseguenze).

## Sezione Chiave LLM

**Scopo.** La chiave è un segreto di sola scrittura: l'app non la riceve mai dal backend, né intera né in parte (`users.md` §1). L'interfaccia non può quindi mostrarla, mostrarne le ultime cifre o «confermarla»: deve dire **solo se c'è**, e il contratto non dice se funziona (il backend non la verifica al salvataggio, `users.md` §2.4). Il testo lo dice onestamente, perché promettere «chiave valida» sarebbe falso e porterebbe a scoprirlo nel momento peggiore, durante una lezione.

Testo introduttivo, sempre visibile (`ui.body`, `inkSoft`): «Serve a scrivere le lezioni, a rispondere in chat e a valutare le risposte aperte. Si salva cifrata sul server e questa app non la rivede mai: per cambiarla la sostituisci.»

### Stati

| Stato | Quando | Chip (`StatusChip`) | Contenuto sotto il testo introduttivo |
|---|---|---|---|
| **Non impostata** | `has_llm_key = false` | `info` · `info` «Non impostata» | Campo di scrittura + «Salva chiave» (vedi sotto). Nessun pulsante «Rimuovi» |
| **Impostata** | `has_llm_key = true` e nessun rifiuto noto dopo l'ultimo salvataggio | `neutral` · `check` «Impostata» | Riga `ui.bodyS` `inkSoft`: «Il server non la controlla quando la salvi: se non funziona, lo scopri al primo uso.» Bottoni: **Sostituisci chiave** (secondario) e **Rimuovi chiave** (distruttivo) |
| **Impostata, rifiutata** | l'app ha ricevuto `llm_key_rejected` dopo l'ultimo salvataggio (stato locale, per utente: `llmKeyRejectedAt`) | `warning` · `alert-triangle` «Rifiutata dal servizio» | Riga `ui.body` `ink`: «L'ultima volta il servizio non ha accettato questa chiave: può essere sbagliata, scaduta o non abilitata a questo uso. Sostituiscila con una valida.» Bottoni: **Sostituisci chiave** (primario) e **Rimuovi chiave** (distruttivo) |
| **In sostituzione** | tocco su «Sostituisci chiave» o ingresso diretto | il chip dello stato di partenza resta | Campo di scrittura + **Salva chiave** (primario) + **Annulla** (testuale); «Rimuovi» nascosto |
| **Controllo dello stato** | primo `GET /me` senza cache | nessun chip: riga «Controllo…» con spinner 24 dp | — |

**Il credito esaurito (`llm_credit_exhausted`) non cambia nessuno stato di questa schermata**: non imposta `llmKeyRejectedAt`, il chip resta «Impostata» e non compare alcun indicatore (la chiave è giusta e non c'è nulla da fare qui; `../llm-key.md` §9.4). Se arriva quel codice con un rifiuto noto, il rifiuto viene azzerato.

`warning` e `info`, non `danger`: la chiave è un'azione che l'utente può compiere subito. Il chip è sempre icona + parola. Il chip «Rifiutata» **non** è mai l'effetto di un controllo di questa schermata (non ne esiste uno): nasce solo da un uso reale; sparisce al salvataggio di una nuova chiave.

### Campo di scrittura

```
Chiave API
┌──────────────────────────────┬───┐
│ ••••••••••••••••••••         │ 👁 │   mascherato, toggle «Mostra chiave»/«Nascondi chiave»
└──────────────────────────────┴───┘
Incolla la chiave del servizio LLM che usa il server.
```

- Variante mascherata di base-components §2, `ImeAction.Done`, `KeyboardType.Password`, **autofill e suggerimenti disattivati** (`importantForAutofill = no`, no-personalized-learning): è un segreto, non una credenziale da ricordare. Incolla ammesso; copia e taglia disabilitati se mascherato. Il toggle serve a controllare l'incolla (chiavi lunghe e simili fra loro).
- Spazi ai bordi tolti prima dell'invio. Il valore vive solo nella memoria del ViewModel: **mai** in `SavedStateHandle`, DataStore, log, `contentDescription`, schermate recenti (il campo esce dall'albero accessibile quando mascherato). Si svuota a salvataggio riuscito, ad Annulla, e all'uscita dalla schermata.
- **Salva chiave** (primario, 48 dp), loading «Salvo…». Validazione locale: vuota → «Incolla la chiave.»; più di 500 caratteri dopo il trim → «La chiave è troppo lunga: al massimo 500 caratteri.» Stesso testo per `422` con `field: "api_key"`. Nessun controllo del formato (`sk-…`): il provider può cambiare.
- **Esito riuscito** (`200`): il campo si svuota e si chiude, il chip diventa «Impostata», Toast «Chiave salvata.» (se si è arrivati da un messaggio su chiave mancante/rifiutata: «Chiave salvata. Puoi tornare indietro e riprovare.»). Si azzera il rifiuto noto, e il contatore globale `llmKeyVersion` sale (i pannelli dei messaggi su chiave si aggiornano al ritorno, `../llm-key.md` §3). Il focus va sul chip.
- **Errori dell'azione** (`StatusPanel` sopra «Salva chiave», il testo digitato resta): rete → `warning` · `cloud-off` «Backend irraggiungibile» / «La chiave non è stata salvata. Riprova quando sei di nuovo collegato.»; `5xx` → `danger` «Qualcosa è andato storto» / «La chiave non è stata salvata. Riprova fra poco.»; `401` → regola globale.

### Rimuovi chiave

`ConfirmDialog` distruttivo, con focus su Annulla:

| Elemento | Testo |
|---|---|
| Titolo | Rimuovere la tua chiave LLM? |
| Corpo | Senza chiave non posso scrivere lezioni, rispondere in chat né valutare le risposte aperte. Argomenti e lezioni già scritte restano dove sono. Quello che sto preparando in questo momento potrebbe interrompersi. Potrai inserire un'altra chiave quando vuoi. |
| Annulla (focus iniziale) | Annulla |
| Conferma (distruttivo) | Rimuovi la chiave |
| In corso | Rimuovo… (Annulla disabilitato) |

Esito: `204` → chip «Non impostata», campo di scrittura, Toast «Chiave rimossa.», `llmKeyVersion` +1. Errore: dialog aperto con `StatusPanel` (rete: «Non riesco a raggiungere il backend» / «La chiave non è stata rimossa. Riprova quando sei di nuovo collegato.»). Nessun «Annulla» nel Toast (non si può recuperare una chiave che l'app non possiede).

## Sezione Password → sotto-schermata «Cambia password»

Riga `ListRow` 56 dp «Cambia password» con chevron, apre `settings/password`. Una sotto-schermata e non un dialog: tre campi più avviso più errori, in orizzontale con la tastiera virtuale (≈ 400 dp utili) non entrerebbero in un dialog da 480 × 330 (`base-components.md` §11).

```
┌──────────────────────────────────────┐
│ ←  Cambia password                   │  TopBar
├──────────────────────────────────────┤
│ (i) Gli altri dispositivi dovranno   │  StatusPanel info, sempre visibile, in cima
│     accedere di nuovo.               │
│                                      │
│ Password attuale                     │
│ ┌──────────────────────────┬─────┐   │
│ │ ••••••••                 │ 👁  │   │
│ └──────────────────────────┴─────┘   │
│ Nuova password                       │
│ ┌──────────────────────────┬─────┐   │
│ │ ••••••••••••             │ 👁  │   │
│ └──────────────────────────┴─────┘   │
│ Da 10 a 128 caratteri. Una frase     │  supporting text
│ lunga va benissimo.                  │
│ Ripeti la nuova password             │
│ ┌──────────────────────────┬─────┐   │
│ │ ••••••••••••             │ 👁  │   │
│ └──────────────────────────┴─────┘   │
│ [      Cambia password       ]       │  primario grande 56 dp
└──────────────────────────────────────┘
```

- Compact: a tutta larghezza con margini 16 dp. Medium/expanded: colonna di **480 dp** (`dialogWidthWide`) centrata, come il modulo di accesso.
- **Avviso** (`StatusPanel info`, `info`): titolo «Gli altri dispositivi dovranno accedere di nuovo», corpo «Cambiando la password, ogni altro dispositivo collegato al tuo account verrà disconnesso. Questo dispositivo resta collegato.». Sta **in cima e sempre visibile**, non in un dialog dopo il tocco: l'utente lo legge prima di decidere (e prima di rimanere stupito su un altro tablet).
- **Ripeti la nuova password**: serve perché non esiste un recupero via email: un refuso nella nuova password significherebbe restare chiusi fuori da ogni altro dispositivo. È l'unica ridondanza richiesta ed è giustificata dall'irreversibilità.
- Ogni campo ha il proprio toggle «Mostra password» / «Nascondi password» (indipendente). `ImeAction.Next, Next, Done`. Autofill: `ContentType.Password` per la password attuale, `ContentType.NewPassword` per i due campi nuovi (i gestori di password possono proporre una password generata); copia/taglia disabilitati se mascherati, incolla ammesso; rimascherati in `ON_STOP`; valori solo in memoria.
- A fine operazione riuscita si completa il contesto di autofill perché il gestore aggiorni la password salvata.

### Validazione ed errori (testi definitivi)

Locale, alla perdita del focus e al tocco di «Cambia password»; poi del server. Errore di campo = bordo `danger` 2 dp + `alert-circle` + testo.

| Campo | Caso | Origine | Testo |
|---|---|---|---|
| Password attuale | vuota | locale | Inserisci la password attuale. |
| Password attuale | `current_password: wrong` | server (`422`) | La password attuale non è corretta. *(il focus va al campo, testo selezionato; gli altri campi restano)* |
| Nuova password | vuota | locale | Scegli una nuova password. |
| Nuova password | `too_short` | locale (< 10) o server | Troppo corta: servono almeno 10 caratteri. *(locale aggiunge: «Ora sono {n}.»)* |
| Nuova password | `too_long` | locale (> 128) o server | Troppo lunga: al massimo 128 caratteri. |
| Ripeti | diversa dalla nuova | locale, alla perdita del focus e all'invio | Le due password non coincidono. |

Lunghezze contate in caratteri Unicode (punti di codice), come le conta il backend; nessun'altra regola di composizione (niente «almeno un numero»: il contratto non la impone e le regole peggiorano le password). Nessun indicatore di «forza» (sarebbe una stima non verificabile). Con più errori il focus va al primo in errore.

Errori dell'azione (`StatusPanel` sopra il bottone): rete → `warning` · `cloud-off` «Backend irraggiungibile» / «La password non è stata cambiata. Riprova quando sei di nuovo collegato.»; `5xx` o altro → `danger` «Qualcosa è andato storto» / «La password non è stata cambiata. Riprova fra poco.»; `401` → regola globale (non succede per la password attuale errata: quella è `422`).

**Esito riuscito** (`204`): si torna alle Impostazioni, Toast (6 s) «Password cambiata. Gli altri dispositivi dovranno accedere di nuovo.». Il token di questo dispositivo resta valido: **nessun nuovo accesso** qui. Campi svuotati.
**Uscita con campi compilati** (Indietro): nessun dialog di conferma (si perdono solo password digitate: riscriverle è banale e un dialog su un segreto è rumore); i valori vengono scartati.
**Stato del bottone**: sempre attivo, loading «Cambio…»; durante il `loading` i campi ignorano l'input.

## Sezione Connessione

Sostituisce «URL del backend» e «Token». **L'indirizzo non si modifica più da qui**: si sceglie all'accesso (`login.md`), e cambiarlo con una sessione aperta significherebbe presentare il token di un server a un altro. Qui si **mostra** e si **verifica**.

| Elemento | Specifica |
|---|---|
| Riga «Backend» | label `ui.label` `inkSoft` + host (senza schema, `ui.bodyL`, `ink`, selezionabile, a capo) |
| Nota | `ui.bodyS` `inkSoft`: «Per usare un altro backend, esci e accedi con il suo indirizzo.» |
| **Prova connessione** | Button secondario. `GET {url}/api/v1/health` con `Authorization: Bearer <token>`; timeout di connessione 5 s, totale 10 s. Durante la prova `loading` («Provo…», `busy`, non ripremibile) |

Area di stato: `StatusPanel` sotto «Prova connessione», `liveRegion = Polite`, struttura icona + titolo (`ui.labelL`) + dettaglio (`ui.bodyL`). Appare con `feedbackAppear`; **torna assente** ad ogni nuova apertura della schermata.

| Stato | Quando | Variante · icona | Titolo | Dettaglio |
|---|---|---|---|---|
| Inattivo | nessuna prova | (assente) | — | — |
| In corso | dal tocco alla risposta | `progress` · spinner 24 dp | Mi collego… | Contatto {host} |
| Connesso | `200` | `correct` · `check-circle` | Connesso | Versione {version} · catalogo v{catalog_version} |
| Backend irraggiungibile | timeout, DNS, rifiuto, TLS | `warning` · `cloud-off` | Backend irraggiungibile | Controlla che Tailscale sia attivo su questo dispositivo. + motivo: «Tempo scaduto.» / «Indirizzo non trovato.» / «Connessione rifiutata.» / «Certificato non valido.» |
| Errore imprevisto | altro (`5xx`, `404`, corpo non JSON, `429`…) | `danger` · `alert-circle` | Errore imprevisto | Il backend ha risposto con il codice {status}. + `error.message` (max 2 righe) + «Dettagli» che espande `request_id` |

Lo stato «Token non valido» **non esiste più**: un `401` non è un esito della prova, è la fine della sessione e vale la regola globale (si apre l'accesso con il motivo; `login.md`). «Salva» e il suo confronto con i valori salvati spariscono: l'unico dato modificabile qui (la chiave) e la password hanno i propri pulsanti.

## Righe di servizio

«Componenti mancanti» e «Licenze open source»: invariate (specifiche in fondo a questo file). Dopo la sezione Connessione, separate da una hairline `line`.

## Stati dei controlli

| Elemento | default | pressed | focus | disabled / loading |
|---|---|---|---|---|
| Esci, Sostituisci, Prova connessione | secondario | fill `surfacePressed` | anello 3 dp | loading con spinner + testo |
| Rimuovi chiave | distruttivo (bordo e testo `danger`) | fill `surfacePressed` | anello 3 dp | — |
| Salva chiave, Cambia password | primario | `accentPressed` | anello 3 dp | loading con spinner + testo |
| TextField | bordo `border` | — | bordo `focus` 2 dp | in sola lettura durante loading, stesso aspetto |
| TextField con errore | bordo `danger` 2 dp + messaggio | — | bordo `danger` 2 dp | — |

Un solo bottone primario visibile per schermata (base-components §1): nelle Impostazioni **«Sostituisci chiave» è primario solo nello stato «rifiutata»**, «Salva chiave» solo con il campo aperto, e i due non compaiono insieme.

## Accessibilità

- Ordine TalkBack: Indietro → titolo → Account (nome, nome utente, Esci) → Chiave LLM (stato, testo, azioni) → Password → Connessione → righe di servizio. Le intestazioni di sezione sono heading.
- Il chip dello stato della chiave è letto come «Chiave LLM: impostata» / «non impostata» / «rifiutata dal servizio». Il campo di scrittura è una `TextField` con etichetta «Chiave API»; il valore non viene mai letto da TalkBack quando è mascherato (nemmeno la lunghezza: il campo si annuncia come «campo protetto»).
- Tutti i pannelli di stato sono live region Polite; i dialog intrappolano il focus (focus iniziale su Annulla), alla chiusura il focus torna al bottone che li ha aperti, tranne dopo «Rimuovi» (va al chip) e «Esci» (la schermata cambia).
- Font 200%: etichette e righe vanno a capo, i bottoni crescono in altezza, i due bottoni della chiave vanno in colonna a tutta larghezza oltre 1,3. Tastiera fisica: Tab segue l'ordine sopra, Invio nel campo = Salva/Cambia, Esc = Annulla nel campo della chiave e nei dialog.
- Contrasti: tutti ruoli già verificati (`design-system.md` §2.4). Significato mai solo colore: chip e pannelli portano icona e parole.

## Casi particolari

- **Rotazione/ricreazione**: lo stato dei dialog e delle sotto-schermate sopravvive; i valori dei campi segreti (chiave, password) sopravvivono alla rotazione (ViewModel) ma non al processo ucciso né all'uscita dalla schermata.
- **Chiave impostata dal server mentre l'app è aperta** (`user set-llm-key`): al prossimo `ON_START` il `GET /me` aggiorna il chip; nessun messaggio.
- **Primo accesso senza chiave**: la guida è in Home e nei messaggi di contesto (`../llm-key.md` §4), non qui: le Impostazioni non si aprono da sole.

## Cosa non c'è (di proposito)

Niente campo token né indirizzo modificabile, niente selettore del tema, niente «ricordami», niente elenco dei dispositivi collegati né «esci da tutti» (il contratto non lo prevede), niente creazione o gestione di altri utenti, niente recupero password, niente mostra-chiave, niente test della chiave (il contratto non lo prevede: proposta nel rapporto), niente indicatore «connesso» permanente.

---

## Sotto-schermata: Componenti mancanti (Fase 9)

> Strumento per il proprietario-sviluppatore, **non didattico**: nessun principio P1-P12 in gioco, solo leggibilità. Contratto: `../../contracts/component-requests.md`. Raggiungibile solo da Impostazioni.

**Scopo**: dire quali componenti il Generator ha chiesto più spesso e non esistevano, per scegliere cosa implementare nel prossimo catalogo.

### Layout (compact)

```
┌──────────────────────────────────────┐
│ ←  Componenti mancanti               │  TopBar
├──────────────────────────────────────┤
│ I componenti che le lezioni hanno    │  ui.body, inkSoft (una riga di contesto)
│ chiesto e il catalogo non ha.        │
│ ┌──────────────────────────────────┐ │
│ │ interactive_timeline        ×4   │ │  Card: wanted in monospazio (ui.labelL),
│ │ catalogo v1 · 9 nov - 12 nov     │ │  count a destra in chip "×4" (testo, non solo colore)
│ │ Mostrare l'ordine degli eventi…  │ │  primo purpose (ui.bodyS, max 2 righe)
│ │ Mostra esempi (3)            ⌄   │ │  azione testuale 48 dp
│ └──────────────────────────────────┘ │
│  (espanso: per ogni esempio)         │
│   Ownership e move · Rust            │  lesson_title · topic_title (ui.bodyS)
│   «purpose completo»  12 nov         │
└──────────────────────────────────────┘
```

- Lista ordinata come il backend (`count` desc); non riordinare né filtrare lato app. `LazyColumn`, gap 12 dp, margini 16 dp, `contentMaxWidth` 840 dp centrato.
- **Tablet/landscape**: stessa colonna centrata a 840 dp (è una lista di consultazione, niente master-detail). Le card si espandono sul posto.
- Date: formato breve locale ("9 nov"); se `first` e `last` cadono nello stesso giorno, una data sola. Intervallo = `first_requested_at` - `last_requested_at`.
- `wanted` in `ui.code` (monospazio), mai tradotto né riformattato. Card non cliccabili salvo "Mostra esempi"; gli esempi non navigano alla lezione (MVP).
- Copia: tocco lungo su `wanted` copia il nome (Toast "Copiato"), utile per incollarlo in un brief.
- Paginazione: scroll infinito col cursore standard; spinner di coda 24 dp mentre carica; se la pagina fallisce, riga di coda "Non riesco a caricarne altri" + "Riprova".

### Stati

| Stato | Contenuto |
|---|---|
| Caricamento (primo) | 3 card scheletro (shimmer disattivato con "riduci animazioni"), `busy` a11y "Carico" |
| Vuoto | Icona `puzzle` 48 dp, titolo "Nessun componente mancante", corpo "Quando una lezione avrà bisogno di qualcosa che il catalogo non ha, lo vedrai qui." Nessun bottone. |
| Errore | `StatusPanel` come in Impostazioni (rete irraggiungibile, imprevisto; un `401` apre l'accesso, `login.md`) + bottone "Riprova" |
| Pull-to-refresh | Sì (caricamento a ogni apertura della schermata, nessuna cache) |

A11y: ogni card è un'unica unità con descrizione "{wanted}, richiesto {count} volte, catalogo v{n}"; il chip del conteggio non è l'unico portatore del numero (testo "richiesto 4 volte" nel semantic). Font 200%: wanted va a capo ovunque (`break` su `_`), chip sotto il titolo.

## Sotto-schermata: Licenze open source

Scelta più semplice che funzioni: **lista statica generata a build time**, non plugin esterni di UI.
- Una schermata `settings/licenses`: TopBar "Licenze open source", lista di ListRow (56 dp): nome libreria (`ui.labelL`) + licenza e versione (`ui.bodyS`, `inkSoft`). Tocco su una riga apre un dettaglio nella stessa schermata (espansione) col testo completo della licenza in monospazio, scorrevole.
- Fonte dati: file `licenses.json` negli assets, prodotto dal plugin Gradle `com.google.android.gms.oss-licenses-plugin` oppure `com.mikepenz.aboutlibraries` (a scelta di android-dev: va bene quello che produce un JSON/elenco senza UI propria). Il rendering è nostro, coi token, così tema chiaro/scuro e font scaling funzionano.
- Includere anche i font (licenza OFL) e le icone (Lucide, ISC) usati dal design system. Vuoto/errore: non applicabile (dati locali); se il file manca, messaggio "Elenco licenze non disponibile in questa build".
