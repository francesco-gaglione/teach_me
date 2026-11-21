# Schermata di accesso

> Componenti: `../base-components.md` (TextField §2 e variante mascherata, Button §1, StatusPanel §5, Toast §9) e le composizioni di §16. Token: `../tokens.json`. Contratto: `../../contracts/users.md` §2.1-2.2, `api-conventions.md` §2 e §7. Decisione: ADR 0008. Nessun componente nuovo, nessun token nuovo.
> **È la prima schermata dell'app senza sessione**: sostituisce il ruolo che le Impostazioni avevano al primo avvio. Non esiste registrazione: gli utenti li crea il proprietario sul server.

## Scopo

Una sola cosa: entrare. Chi apre l'app la prima volta capisce **che cosa gli serve** (indirizzo, nome utente, password) e **da chi lo ottiene** (il proprietario del server). Chi rientra dopo una sessione finita capisce **perché** e trova già compilato tutto tranne la password. Carico cognitivo (Sweller): tre campi al massimo, due per chi torna; niente testo che non serva a compilarli. Errore = informazione, mai colpa (P9): l'ambra (`warning`) segnala ciò che l'utente può correggere, `danger` solo l'imprevisto, come nelle Impostazioni.

## Quando si vede

| Caso | Come ci si arriva | Cosa mostra |
|---|---|---|
| **Primo avvio** | nessun indirizzo salvato | variante *Primo accesso* |
| **Dopo un aggiornamento dell'app** | c'è un indirizzo (e un vecchio token statico, da scartare), nessun nome utente | variante *Aggiornamento*: indirizzo compilato, avviso che ora si accede con nome utente e password |
| **Uscita volontaria** (Impostazioni → Esci) | l'utente ha scelto | variante *Rientro*: indirizzo e nome utente compilati, nessun avviso; Toast «Sei uscito.» |
| **Sessione finita** | qualunque richiesta autenticata risponde `401 unauthorized` (scaduta dopo 90 giorni senza richieste, revocata dal cambio password fatto altrove, da `user passwd`, da `user disable`) | variante *Sessione finita*: avviso che spiega, indirizzo e nome utente compilati, focus sulla password |

Regola unica per il `401 unauthorized` (sostituisce ogni vecchio pannello «Il token non è valido» delle altre schermate): **nessuna schermata mostra un errore per un `401`**. Un gestore globale cancella il token, **conserva** indirizzo, nome utente e i dati locali legati a quell'utente (coda dei voti, risposte, bozze) e porta alla schermata di accesso sopra la pila di navigazione. Dopo un accesso riuscito **con lo stesso utente** (`user.id` uguale a quello della sessione finita) si torna dove si era, con bozze e scroll intatti; con un **utente diverso** la pila si azzera e si apre Home. Gli errori `401` di `POST /auth/login` (`invalid_credentials`) sono un'altra cosa e restano nella schermata.

## Layout (compact, 360 dp), primo accesso

```
┌──────────────────────────────────────┐
│                                      │  nessuna TopBar, nessun indietro
│  (margine 16 dp)                     │
│  Accedi a teach_me                   │  ui.title1, heading
│  Inserisci l'indirizzo del backend,  │  ui.bodyL, inkSoft (solo primo accesso e aggiornamento)
│  poi il nome utente e la password    │
│  che ti ha dato il proprietario      │
│  del server.                         │
│                                      │
│  Indirizzo del backend               │  label (ui.label, inkSoft)
│  ┌────────────────────────────────┐  │
│  │ https://teach.tailnet.ts.net   │  │  TextField (48 dp)
│  └────────────────────────────────┘  │
│  Es. https://teach.tailnet.ts.net    │  supporting text
│                                      │
│  Nome utente                         │
│  ┌────────────────────────────────┐  │
│  │ valentina                      │  │
│  └────────────────────────────────┘  │
│                                      │
│  Password                            │
│  ┌──────────────────────────┬─────┐  │
│  │ ••••••••••••             │ 👁  │  │  mascherato + toggle 48 dp
│  └──────────────────────────┴─────┘  │
│                                      │
│  ┌────────────────────────────────┐  │  StatusPanel (solo quando c'è un esito), liveRegion
│  │ (!) Nome utente o password …   │  │  vicino al bottone (contiguità, P4)
│  └────────────────────────────────┘  │
│  [            Accedi             ]   │  Button primario grande 56 dp, a tutta larghezza
│                                      │
│  Non hai un accesso? Te lo crea il   │  ui.bodyS, inkSoft: l'unica spiegazione di
│  proprietario del server.            │  «come ottengo un account», sempre visibile
└──────────────────────────────────────┘
```

Colonna scorrevole, margini 16 dp, gap fra gruppi 24 dp (`blockGap`), `imePadding`: il campo a fuoco, il pannello di stato e «Accedi» restano visibili sopra la tastiera (scroll automatico al campo, `bringIntoView`). Il testo introduttivo sparisce dopo il primo accesso riuscito (non serve più) e non c'è mai alcun testo di marketing o illustrazione (Mayer, coerenza).

### Variante *Rientro* (indirizzo già salvato)

L'indirizzo non è più un campo ma una riga di sola lettura, per togliere un elemento (e un possibile errore) a chi torna:

```
  Backend    teach.tailnet.ts.net   [Cambia]     ← riga 48 dp: label ui.label inkSoft + host ui.bodyL ink
  Nome utente  …                                    + Button testuale «Cambia» (48 dp)
  Password     …
```

- `Cambia` trasforma la riga nel TextField dell'indirizzo (precompilato, focus, testo selezionato) e **non si richiude** finché la schermata è aperta.
- La riga si trasforma **da sola** in campo se un tentativo fallisce con «Backend irraggiungibile» o con l'errore imprevisto `404`/risposta non riconosciuta (§Errori): è lì che l'utente deve poter correggere. Il focus passa al campo.
- `contentDescription` della riga: «Backend: teach.tailnet.ts.net. Cambia l'indirizzo».
- Nella variante *Primo accesso* e *Aggiornamento senza indirizzo valido* il campo è sempre visibile.

### Expanded / tablet orizzontale (1280 × 800 dp)

Nessun list-detail. Un'unica colonna di **480 dp** (`dialogWidthWide`) centrata nella finestra, allineata in alto con 64 dp di margine superiore (non centrata in verticale: con la tastiera virtuale, che in orizzontale occupa 300-400 dp, il modulo deve restare sopra). La tastiera fisica non cambia il layout. Il testo introduttivo e il pannello di stato restano nella colonna; se l'altezza utile con la tastiera è < 480 dp la colonna scorre. In compact largo (medium, 600-839 dp) stessa colonna di 480 dp, margini 24.

## Campi

| Campo | Comportamento |
|---|---|
| **Indirizzo del backend** | `KeyboardType.Uri`, `ImeAction.Next`, nessuna correzione né maiuscole automatiche. Accetta `https://` e `http://` senza avvisi sul cleartext (traffico via Tailscale). Spazi iniziali/finali tolti; `/` finale e un eventuale `/api/v1` digitato rimossi. Nessun autofill (`importantForAutofill = no`). |
| **Nome utente** | `KeyboardType.Text`, `capitalization = None`, `autoCorrectEnabled = false`, `ImeAction.Next`. **Autofill di sistema attivo**: `contentType = ContentType.Username`. Spazi ai bordi tolti prima dell'invio; il maiuscolo non si corregge (il backend confronta in minuscolo: «Valentina» entra) e non si riscrive nel campo. Nessuna regola di formato applicata in locale (3-32 caratteri `a-z0-9._-` sono regole di creazione, non di accesso: dirle qui suggerirebbe come indovinare). |
| **Password** | Campo mascherato (base-components §2): `ImeAction.Done`, `KeyboardType.Password`, **autofill di sistema attivo**: `contentType = ContentType.Password`. Toggle `eye`/`eye-off` 48 dp, `contentDescription` «Mostra password» / «Nascondi password», `toggleable`/`Role.Switch`. Copia e taglia disabilitati se mascherato; **incolla sempre ammesso** (gestori di password). Tornando in secondo piano (`ON_STOP`) il campo si rimaschera. Spazi **non** tolti (fanno parte della password). Il valore non entra mai in log, `contentDescription`, `SavedStateHandle` o `rememberSaveable` su disco: vive solo nello stato in memoria del ViewModel (sopravvive alla rotazione, non al processo ucciso). Nessun limite di lunghezza né minimo applicati in locale. |

**Eccezione esplicita** alla regola «niente autofill sui campi mascherati» (pensata per un token da incollare): qui l'autofill è voluto, perché un utente con una password di 10+ caratteri senza gestore di password userebbe password deboli o le riscriverebbe di continuo sul tablet. Gli altri suggerimenti della tastiera restano disattivati. A accesso riuscito android-dev completa il contesto di autofill (`AutofillManager.commit()` / Credential Manager) perché il sistema proponga di salvare le credenziali: nessun salvataggio nostro.

### Validazione locale (prima della rete)

Scatta alla perdita del focus e al tocco di «Accedi», non a ogni tasto. Errore di campo = bordo `danger` 2 dp + `alert-circle` + testo (mai solo colore), l'errore sparisce quando il campo cambia.

| Caso | Testo |
|---|---|
| Indirizzo vuoto | Inserisci l'indirizzo del backend. |
| Senza `http://`/`https://` | L'indirizzo deve iniziare con https:// o http://. |
| Host mancante o non valido | L'indirizzo non è completo: controlla il nome del server. |
| Nome utente vuoto | Inserisci il nome utente. |
| Password vuota | Inserisci la password. |

Con più errori il focus va al primo campo in errore e TalkBack annuncia il suo messaggio. Nessuna chiamata di rete se la validazione fallisce.

## Pulsante «Accedi»

- Sempre abilitato (se un campo è vuoto, il motivo compare al tocco, come sopra: un bottone spento all'avvio è muto, base-components §1). Primario grande, a tutta larghezza, 56 dp.
- Invia `POST {url}/api/v1/auth/login` con `username`, `password` e `device_name` = produttore e modello del dispositivo (per esempio «Samsung SM-X710»): non lo digita l'utente. Timeout di connessione 5 s, totale 15 s (l'hash lento del backend richiede tempo).
- **In corso**: bottone `loading` (spinner 20 dp + «Accedo…», `busy`, non ripremibile); i campi restano com'erano ma ignorano l'input (sola lettura, nessun cambio di stile: il riempimento `disabledFill` farebbe lampeggiare il modulo); la tastiera si chiude. Nessun pannello «in corso» separato: il bottone basta. Dopo 8 s sotto il bottone compare «Ci sta mettendo più del solito…» (`ui.bodyS`, `inkSoft`).
- **Successo** (`200`): salva token, `user` e indirizzo; naviga a Home **sostituendo** l'accesso (Indietro da Home chiude l'app). Nessun Toast di benvenuto. Se `user.has_llm_key` è `false`, Home mostra il suggerimento per la chiave (`../llm-key.md` §4): nessun passaggio intermedio che blocchi.
- **Invio**: Invio dalla tastiera su Password = «Accedi». Con tastiera fisica Invio su Indirizzo e Nome utente passa al campo successivo; **Ctrl+Invio** su qualunque campo = «Accedi».

## Stati ed errori (testi definitivi)

Tutti gli esiti compaiono in un `StatusPanel` **sopra «Accedi»**, `liveRegion = Polite`, con `feedbackAppear` (200 ms; istantaneo con «riduci animazioni»). Significato = icona + titolo + colore. Cambiare un campo (tranne il toggle) **chiude** il pannello, tranne *Troppi tentativi* (vedi sotto).

| Stato | Quando | Variante · icona | Titolo | Corpo |
|---|---|---|---|---|
| **Vuoto** | schermata appena aperta | nessun pannello | | |
| **In corso** | dal tocco alla risposta | (nel bottone) | | |
| **Credenziali errate** | `401 invalid_credentials` | `warning` · `key-round` | Nome utente o password non corretti | Controlla di averli scritti giusti: per sicurezza non ti dico quale dei due è sbagliato. |
| **Troppi tentativi** | `429 rate_limited` | `warning` · `clock` | Troppi tentativi | Per proteggere l'account ho sospeso gli accessi con questo nome utente. Potrai riprovare tra {attesa}. |
| **Backend irraggiungibile** | timeout, DNS, connessione rifiutata, TLS | `warning` · `cloud-off` | Backend irraggiungibile | Controlla che Tailscale sia attivo su questo dispositivo e che l'indirizzo sia giusto. + motivo in coda: «Tempo scaduto.» / «Indirizzo non trovato.» / «Connessione rifiutata.» / «Certificato non valido.» |
| **Backend non pronto** | `503 unavailable` | `warning` · `clock` | Il backend non è pronto | Si sta avviando o è in manutenzione. Riprova fra poco. |
| **Indirizzo non riconosciuto** | `404`, `405`, risposta non JSON del contratto | `danger` · `alert-circle` | Questo indirizzo non sembra un backend teach_me | Controlla l'indirizzo. Se è giusto, il backend potrebbe non essere aggiornato alla versione con gli accessi per utente. |
| **Errore imprevisto** | altro (`5xx`, `422`, `400`, codice sconosciuto) | `danger` · `alert-circle` | Errore imprevisto | Il backend ha risposto con il codice {status}. + azione testuale «Dettagli» che espande `request_id` (selezionabile, `ui.bodyS` `inkSoft`). |

`{status}` è il codice HTTP. L'app non mostra mai `error.message` in primo piano (può essere tecnico).

### Credenziali errate

- Si **conserva** tutto quello che l'utente ha scritto. Il focus va al campo Password con il testo **selezionato** (basta ricominciare a scrivere), la tastiera resta aperta e il toggle è nello stato in cui era. Il pannello non assegna la colpa né a un campo né alla persona.
- Dal terzo tentativo consecutivo fallito nella stessa apertura della schermata si aggiunge in coda al corpo: «Se non hai ancora un accesso, te lo crea il proprietario del server.» (non prima: il primo errore è quasi sempre un refuso).
- Nessun conteggio dei tentativi residui (il limite è 5 in 15 minuti ma dirlo invita a provare a tentoni e il backend lo applica comunque).

### Troppi tentativi (`Retry-After`)

- Il blocco vale **per nome utente**: se l'utente cambia il nome utente, il pannello si chiude e «Accedi» si riabilita; se lo lascia, resta in attesa.
- Attesa letta da `Retry-After` (secondi) e trasformata in `retryAt` (orologio monotono); sopravvive alla rotazione, non al processo ucciso (al riavvio il backend risponderà di nuovo `429`).
- «Accedi» è **disabilitato** (`disabledFill`/`disabledInk`) finché il tempo non scade; il motivo è il pannello, che sta sopra al bottone; `stateDescription` del bottone = «Disponibile tra {attesa}».
- `{attesa}`: meno di 60 s → «{n} secondi» (un solo secondo: «1 secondo»), aggiornato **ogni secondo** a schermo; da 60 s a meno di un'ora → minuti **arrotondati per eccesso**, «1 minuto» / «{m} minuti», aggiornato ogni 10 s; un'ora o più → «{h} ore» (arrotondato per eccesso). Header assente o non numerico: corpo «…Potrai riprovare tra qualche minuto.» e il bottone si riabilita dopo 60 s.
- **TalkBack**: il testo cambia ma **nessun annuncio a ogni tick**: il pannello annuncia una volta («Troppi tentativi. Potrai riprovare tra 14 minuti.») e, allo scadere, un secondo annuncio Polite «Puoi riprovare ad accedere.»; il pannello sparisce e «Accedi» si riabilita senza spostare il focus.
- Al termine il campo password **non** si svuota.

### Backend irraggiungibile

Il motivo preciso è una riga a parte, sempre dopo la frase d'azione. Il campo dell'indirizzo si apre e prende il focus (se la schermata era in variante *Rientro*). «Accedi» resta attivo: il ritentativo è l'azione.

### Errore imprevisto

Non ci si può fare niente di specifico: si dice la verità («codice {status}») senza dare colpe e si lascia ritentare. `danger` solo qui e nell'indirizzo non riconosciuto (un errore che l'utente non causa ma che vuole sapere perché non entra).

## Variante *Sessione finita* e *Aggiornamento*: l'avviso del motivo

Sopra il titolo, un `StatusPanel` `info` (icona `info`, `liveRegion = Polite`, nessun pulsante), **senza colore d'errore**: non è un errore dell'utente. Si chiude da solo al primo accesso riuscito; non ha ✕ (non ingombra, è l'unico motivo per cui la schermata compare).

| Variante | Titolo | Corpo |
|---|---|---|
| **Sessione finita** | Devi accedere di nuovo | La tua sessione non è più valida: è scaduta, oppure è stata chiusa dopo un cambio di password o da chi gestisce il server. I tuoi dati sono al sicuro sul server. |
| **Sessione finita, con voti o risposte in coda** | Devi accedere di nuovo | Come sopra + «{coda} restano su questo dispositivo e partiranno appena accedi con «{nome utente}».» |
| **Aggiornamento** | Ora si accede con nome utente e password | Il token che usavi prima non vale più. Il proprietario del server ti dà il tuo nome utente e la tua password: inseriscili qui sotto. |

`{coda}`: elementi non ancora inviati, composti così: solo voti → «1 voto di ripasso» / «{n} voti di ripasso»; solo risposte → «1 risposta di una lezione» / «{n} risposte di lezioni»; entrambi → «{n} voti di ripasso e {m} risposte di lezioni» (con i singolari «1 voto di ripasso e 1 risposta di una lezione»); verbo concordato: «restano» (plurale) / «resta» (un solo elemento). Il numero è nel testo perché qui è utile a decidere (non è un contatore di gioco).

Perché spiegare il motivo (e non solo mostrare il modulo): una sessione che sparisce senza spiegazione sembra un guasto o una perdita di dati; con il motivo e la rassicurazione («I tuoi dati sono al sicuro») il rientro è un gesto da 10 secondi. Il testo non può distinguere scaduta, revocata o utente disattivato (il contratto risponde `401 unauthorized` a tutti e tre) e quindi dice «è scaduta, oppure…».

Indirizzo e nome utente sono già compilati; il focus va sulla **password** con la tastiera aperta se si è nella variante *Sessione finita* **e** non c'è tastiera fisica. Con tastiera fisica il focus va sul campo password senza aprire altro.

## Focus e tastiera: riepilogo

| Variante | Focus iniziale |
|---|---|
| Primo accesso | **nessun** campo a fuoco, tastiera chiusa (il testo e il modulo si leggono interi; con tastiera virtuale in orizzontale la tastiera ne coprirebbe metà). Con tastiera fisica: primo campo |
| Aggiornamento | Nome utente |
| Rientro / Sessione finita | Password |
| Dopo un errore | vedi per ogni stato (Password selezionata; indirizzo se irraggiungibile) |

Ordine di Tab (tastiera fisica, `Tab`/`Maiusc+Tab`): [Cambia] → indirizzo → nome utente → password → toggle → Accedi. Anello di focus `focus` 3 dp (2 dp di bordo per i campi) come nel resto dell'app. Esc non fa nulla (non c'è un «annulla»).

## Accessibilità

- Titolo «Accedi a teach_me» è heading; nessuna TopBar. Ordine di lettura: avviso del motivo (se c'è) → titolo → introduzione → campi → pannello di stato → Accedi → nota finale.
- Etichette visibili sopra i campi, mai solo placeholder. Errori di campo con `semantics.error(messaggio)`; il pannello di stato è live region, letto come «<titolo>. <corpo>». Il focus di TalkBack **non viene spostato** dall'app, tranne l'effetto sul campo password dopo `invalid_credentials` (azione dell'utente appena compiuta, gli si porta il punto da correggere).
- Contrasti: tutti i colori sono ruoli già verificati (`ink` su `xSoft` ≥ 7:1, `x` su `xSoft` ≥ 4,5:1, `design-system.md` §2.4); disabilitato «Accedi» durante l'attesa è l'unica eccezione WCAG per componenti inattivi, e il motivo è scritto nel pannello (non dipende dal colore).
- Font 200%: i campi crescono, le etichette vanno a capo, «Accedi» cresce in altezza (min 56 dp), la nota finale scorre; con la tastiera aperta il pannello e «Accedi» sono raggiungibili scorrendo. Tema chiaro e scuro con gli stessi token.
- Target: ogni elemento ≥ 48 dp, distanza ≥ 8 dp (il toggle 48 dp sta nel campo, «Cambia» è 48 dp).

## Cosa non c'è (di proposito)

Niente «Registrati», «Password dimenticata», «Ricordami» (la sessione dura 90 giorni di inattività e basta), «Accedi con…», biometria, test di connessione separato (l'accesso stesso rivela raggiungibilità e indirizzo), conteggio dei tentativi residui, avviso sul cleartext, logo o illustrazioni, messaggi di benvenuto. «Non hai un accesso?» non è un link: la risposta è un'istruzione, non un percorso nell'app.
